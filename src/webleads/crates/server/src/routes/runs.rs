//! Starting a scrape, watching it, and stopping it.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use webleads_domain::{ScrapeOptions, Target, Tier};
use webleads_store::{Run, Word};

use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;

/// How often the event stream looks for something new to say.
const TICK: Duration = Duration::from_millis(1500);

/// What to search for, as the page asks for it.
#[derive(Debug, Default, Deserialize)]
pub struct StartRequest {
    /// Whole tiers — "all of S", "S and A".
    #[serde(default)]
    pub tiers: Vec<String>,
    /// Individual trades, by their English name.
    #[serde(default)]
    pub trades: Vec<String>,
    /// Anything you typed yourself, one per line.
    #[serde(default)]
    pub words: Vec<String>,
    /// The settings. Left out, ours are used.
    #[serde(default)]
    pub options: Option<ScrapeOptions>,
}

#[derive(Debug, Serialize)]
pub struct StartResponse {
    pub run_id: i64,
    pub words: usize,
}

/// `POST /api/runs` — start searching.
pub async fn start(
    State(state): State<SharedState>,
    Json(request): Json<StartRequest>,
) -> ApiResult<Json<StartResponse>> {
    // One run at a time, for the same reason as one import at a time: two scrapes
    // means twice the traffic to Google, which is exactly what we are being careful
    // about. Claimed by the database, which is also what survives a restart.
    if let Some(running) = state.store.unfinished_scrape().await? {
        return Err(ApiError::BadRequest(format!(
            "run {} is still going — stop it first",
            running.id
        )));
    }

    let searching = state.searching();
    let mut options = settings_for(&state, &searching, request.options.clone()).await;
    let words = chosen_words(&state.targets(), &request, &searching.label, &options.lang);
    if words.is_empty() {
        return Err(ApiError::BadRequest(nothing_to_search(
            &searching.label,
            &options.lang,
        )));
    }
    // The words we are about to search, put into the settings before they are
    // checked. Without this the settings would be judged with an empty keyword list
    // and refuse themselves — which is what happened the first time.
    //
    // Each word is then searched on its own, so the loop replaces this list with one
    // word at a time. It is here so the settings are validated against what the run
    // will actually ask for.
    options.keywords = words.iter().map(|(phrase, _)| phrase.clone()).collect();

    if let Err(wrong) = options.validate() {
        let said: Vec<String> = wrong
            .iter()
            .map(|w| format!("{}: {}", w.field, w.said))
            .collect();
        return Err(ApiError::BadRequest(said.join("; ")));
    }

    let run_id = state
        .store
        .begin_scrape(&searching, &words, &options)
        .await?;
    tracing::info!(run_id, words = words.len(), area = %searching.label, "scrape starting");

    // The loop runs in the background; this answers at once.
    let running = state.clone();
    tokio::spawn(async move {
        crate::scrape::work_through(running.clone(), run_id, options).await;
        // And whatever is lined up behind it.
        crate::scrape::work_through_queue(running).await;
    });

    Ok(Json(StartResponse {
        run_id,
        words: words.len(),
    }))
}

/// `POST /api/runs/queue` — line this search up rather than starting it now.
///
/// The same request as starting one. What comes back is a run that has not begun,
/// with its words already written down so you can see exactly what it will do.
pub async fn queue(
    State(state): State<SharedState>,
    Json(request): Json<StartRequest>,
) -> ApiResult<Json<StartResponse>> {
    let searching = state.searching();
    // The settings travel with it. Rebuilt from the defaults when its turn came,
    // anything chosen would be lost in the queue.
    let mut options = settings_for(&state, &searching, request.options.clone()).await;
    let words = chosen_words(&state.targets(), &request, &searching.label, &options.lang);
    if words.is_empty() {
        return Err(ApiError::BadRequest(nothing_to_search(
            &searching.label,
            &options.lang,
        )));
    }
    options.keywords = words.iter().map(|(phrase, _)| phrase.clone()).collect();
    if let Err(wrong) = options.validate() {
        let said: Vec<String> = wrong
            .iter()
            .map(|w| format!("{}: {}", w.field, w.said))
            .collect();
        return Err(ApiError::BadRequest(said.join("; ")));
    }
    let run_id = state
        .store
        .queue_scrape(&searching, &words, &options)
        .await?;
    tracing::info!(run_id, words = words.len(), area = %searching.label, "scrape queued");

    // If nothing is going, this one starts straight away — which is what "line it up"
    // means when the line is empty.
    let taking = state.clone();
    tokio::spawn(async move { crate::scrape::work_through_queue(taking).await });

    Ok(Json(StartResponse {
        run_id,
        words: words.len(),
    }))
}

/// What is lined up, and whether the queue is holding.
#[derive(Debug, Serialize)]
pub struct Queue {
    pub waiting: Vec<webleads_store::Run>,
    pub paused: bool,
}

/// `GET /api/runs/queue` — what is lined up.
pub async fn waiting(State(state): State<SharedState>) -> ApiResult<Json<Queue>> {
    Ok(Json(Queue {
        waiting: state.store.queued().await?,
        paused: state.store.queue_paused().await?,
    }))
}

/// `DELETE /api/runs/queue/:id` — take one out of the line.
pub async fn forget(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<bool>> {
    if !state.store.forget_queued(id).await? {
        return Err(ApiError::BadRequest(
            "that search is not waiting — a run that has started is stopped, not deleted".into(),
        ));
    }
    Ok(Json(true))
}

/// What forgetting this search would take with it.
///
/// Asked before the question is put, so the dialog can say the real number rather
/// than "are you sure?".
pub async fn forget_cost(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<webleads_store::ForgetCost>> {
    match state.store.forget_cost(id).await? {
        Some(cost) => Ok(Json(cost)),
        None => Err(ApiError::NotFound(format!("there is no search {id}"))),
    }
}

/// Forget a search that is over, and everything it wrote down about itself.
///
/// Separate from [`forget`], which takes one out of the queue before it starts. This
/// one is for the list of past searches, and the businesses it found are left alone.
pub async fn erase(State(state): State<SharedState>, Path(id): Path<i64>) -> ApiResult<Json<bool>> {
    if !state.store.delete_run(id).await? {
        return Err(ApiError::NotFound(format!("there is no search {id}")));
    }
    Ok(Json(true))
}

/// Hold the queue, or let it carry on.
#[derive(Debug, Deserialize)]
pub struct Holding {
    pub paused: bool,
}

/// `POST /api/runs/queue/pause` — hold after the current search, or carry on.
pub async fn hold(
    State(state): State<SharedState>,
    Json(asked): Json<Holding>,
) -> ApiResult<Json<Queue>> {
    state.store.pause_queue(asked.paused).await?;
    // Letting it carry on takes the next one at once rather than at the end of a
    // search that may not be running.
    if !asked.paused {
        let taking = state.clone();
        tokio::spawn(async move { crate::scrape::work_through_queue(taking).await });
    }
    waiting(State(state)).await
}

/// Turn tiers, trades and typed words into search phrases.
///
/// Pure, so what gets searched can be checked without a database or a network — and
/// it is the one place that decides what a run actually costs.
/// The settings a search will actually run with.
///
/// The request's own when it names them; otherwise the defaults around the chosen
/// area, in the language you last picked (English until you pick another). The page
/// only sends its settings once you have moved one, so without the remembered
/// language a search would quietly run in whatever the default happened to be.
async fn settings_for(
    state: &SharedState,
    searching: &webleads_domain::Area,
    theirs: Option<ScrapeOptions>,
) -> ScrapeOptions {
    // Asked for by name: yours to get wrong, and yours to get right.
    if let Some(theirs) = theirs {
        return theirs;
    }
    let mut options = ScrapeOptions::around(searching);
    match state.store.search_lang().await {
        Ok(lang) => options.lang = lang,
        Err(err) => tracing::warn!("⚠️ WARNING: could not read the search language: {err}"),
    }
    options
}

/// Why a search has nothing to do, said usefully.
///
/// Pure, so the sentence is a test. "Pick a tier" is no help when you have picked
/// three and none of them has a word in the language this city speaks.
pub fn nothing_to_search(place: &str, lang: &str) -> String {
    format!(
        "nothing to search for in {place}. Searches there are made in {lang}, and \
         none of the trades you picked has a {lang} word — pick others, or type your \
         own words."
    )
}

pub fn chosen_words(
    targets: &[Target],
    request: &StartRequest,
    place: &str,
    // The language the city we are searching describes its trades in. A trade with no
    // words in it is skipped rather than searched in somebody else's language —
    // "vízvezető-szerelő Dubai" finds nothing and costs three minutes to find out.
    lang: &str,
) -> Vec<(String, Option<String>)> {
    let tiers: Vec<Tier> = request
        .tiers
        .iter()
        .filter_map(|t| Tier::parse(t))
        .collect();

    let mut words: Vec<(String, Option<String>)> = Vec::new();
    for target in targets {
        let wanted = tiers.contains(&target.tier)
            || request
                .trades
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&target.english));
        if wanted {
            for phrase in target.phrases(place, lang) {
                words.push((phrase, Some(target.english.clone())));
            }
        }
    }

    // Your own words have no trade — we did not put them on the list, so we cannot
    // claim to know what they are.
    for typed in &request.words {
        let typed = typed.trim();
        if !typed.is_empty() {
            words.push((typed.to_owned(), None));
        }
    }

    // The same trade picked twice, by tier and by name, is still one search.
    words.dedup_by(|a, b| a.0 == b.0);
    words
}

#[derive(Debug, Serialize)]
pub struct Estimate {
    pub words: usize,
    /// `None` until a run has actually finished. An estimate invented before we have
    /// measured anything is worse than admitting we do not know yet.
    pub seconds: Option<u64>,
}

/// `POST /api/runs/estimate` — what would this cost, before you commit to it.
pub async fn estimate(
    State(state): State<SharedState>,
    Json(request): Json<StartRequest>,
) -> ApiResult<Json<Estimate>> {
    let searching = state.searching();
    // The same settings the search itself would use, so the estimate counts the same
    // words the run would actually do.
    let options = settings_for(&state, &searching, request.options.clone()).await;
    let words = chosen_words(&state.targets(), &request, &searching.label, &options.lang);
    let each = state.store.seconds_per_word().await?;
    Ok(Json(Estimate {
        words: words.len(),
        seconds: each.map(|s| (s * words.len() as f64).round() as u64),
    }))
}

/// How many past searches the page is offered. Enough to cover a week of work.
const PAST_SHOWN: i64 = 25;

/// `GET /api/runs/past` — the searches already done, newest first.
///
/// Failed ones included on purpose: "it found nothing because the scraper was off" is
/// the most useful thing this list can say, and hiding it would leave you wondering
/// where a search went.
pub async fn past(State(state): State<SharedState>) -> ApiResult<Json<Vec<Run>>> {
    Ok(Json(state.store.past_scrapes(PAST_SHOWN).await?))
}

/// `GET /api/runs/current` — the run going on now, if any.
pub async fn current(State(state): State<SharedState>) -> ApiResult<Json<Option<Run>>> {
    // A paused search counts as the current one. It has nobody watching it and half
    // its words left, so the page has to be able to find it again and continue it.
    match state.store.unfinished_scrape().await? {
        Some(run) => Ok(Json(Some(run))),
        None => Ok(Json(state.store.held_scrape().await?)),
    }
}

/// The language to search in, as picked on the page.
#[derive(Debug, Deserialize)]
pub struct SearchLang {
    pub lang: String,
}

/// `POST /api/search_lang` — remember which language searches are made in.
pub async fn set_search_lang(
    State(state): State<SharedState>,
    Json(asked): Json<SearchLang>,
) -> ApiResult<Json<String>> {
    state.store.set_search_lang(&asked.lang).await?;
    let lang = state.store.search_lang().await?;
    tracing::info!(lang = %lang, "search language remembered");
    Ok(Json(lang))
}

/// Your clock, and every one you could choose instead.
#[derive(Debug, Serialize)]
pub struct Zones {
    pub zone: &'static str,
    pub all: Vec<&'static str>,
}

/// `GET /api/your_zone` — the clock ring-backs and "today" are read in.
pub async fn your_zone() -> Json<Zones> {
    Json(Zones {
        zone: webleads_domain::your_zone_name(),
        all: webleads_domain::all_zones(),
    })
}

/// A clock to switch to.
#[derive(Debug, Deserialize)]
pub struct NewZone {
    pub zone: String,
}

/// `POST /api/your_zone` — read every time in this clock from now on.
pub async fn set_your_zone(
    State(state): State<SharedState>,
    Json(asked): Json<NewZone>,
) -> ApiResult<Json<String>> {
    let zone = state.store.set_your_zone(&asked.zone).await?;
    let zone = webleads_domain::set_your_zone(&zone).map_err(ApiError::BadRequest)?;
    tracing::info!(zone, "your clock changed");
    Ok(Json(zone.to_owned()))
}

/// `GET /api/runs/:id/words` — every word and what it returned. The hit report.
pub async fn words(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<Word>>> {
    Ok(Json(state.store.words(id).await?))
}

/// `POST /api/runs/:id/stop` — abandon cleanly, keeping everything already found.
pub async fn stop(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<&'static str>> {
    state.ask_to_stop(id);
    tracing::info!(run_id = id, "stop asked");
    Ok(Json("stopping"))
}

/// `POST /api/runs/:id/pause` — finish the word it is on, then hold.
///
/// Not stop: the search Google is already running for us is paid for, and abandoning
/// it halfway would throw that away. So the word finishes and the run stops there.
pub async fn pause(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<&'static str>> {
    state.ask_to_hold(id);
    tracing::info!(target: "scraper", run_id = id, "pause asked — will hold after this word");
    Ok(Json("pausing"))
}

/// `POST /api/runs/:id/continue` — carry on from where it was paused.
///
/// Says plainly when there was nothing to continue rather than answering "carrying
/// on" to a press that did nothing.
pub async fn carry_on(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<&'static str>> {
    // Taking back a pause that has not been obeyed yet is enough on its own: the run
    // is still working, and the loop will simply not stop at the next word.
    if state.hold_asked(id) {
        state.clear_hold(id);
        tracing::info!(target: "scraper", run_id = id, "pause taken back before it was obeyed");
        return Ok(Json("carrying on"));
    }

    if crate::scrape::carry_on(state.clone(), id).await {
        tracing::info!(target: "scraper", run_id = id, "continuing");
        return Ok(Json("carrying on"));
    }
    Err(ApiError::BadRequest(format!("search {id} is not on hold")))
}

/// `GET /api/runs/:id/events` — what is happening, as it happens.
///
/// Server-sent events rather than the page asking every second. A run takes an hour
/// and a half; that is five thousand pointless requests, and this is one connection
/// that stays open and says something only when there is something to say.
pub async fn events(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    use futures_util::stream;

    // `unfold` builds a stream from a value that changes each time round: here, what
    // we last told the page, so an unchanged run says nothing at all.
    let stream = stream::unfold(
        (state, id, None::<String>),
        move |(state, id, last)| async move {
            loop {
                tokio::time::sleep(TICK).await;

                let run = match state.store.run(id).await {
                    Ok(Some(run)) => run,
                    // The run is gone, or the database is unhappy. Either way there is
                    // nothing more to report about it.
                    _ => return None,
                };

                let said = serde_json::to_string(&run).unwrap_or_default();
                let finished = !run.state.still_going();

                if Some(&said) != last.as_ref() {
                    let event = Event::default().event("run").data(said.clone());
                    return Some((Ok(event), (state, id, Some(said))));
                }
                if finished {
                    return None; // nothing will change again
                }
            }
        },
    );

    // A keep-alive comment now and then, so a proxy or a quiet run does not look like
    // a dropped connection.
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
#[path = "runs_tests.rs"]
mod tests;
