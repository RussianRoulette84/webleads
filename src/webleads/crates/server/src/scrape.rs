//! Working through a run's words, one at a time.
//!
//! The loop lives here rather than in a route so that it can be started two ways: by
//! you pressing go, and by the server finding a half-finished run on start-up.

use std::sync::Arc;
use std::time::Duration;

use webleads_domain::ScrapeOptions;
use webleads_scraper::{Client, RemoteState};
use webleads_store::{RunState, Word};

use crate::routes::assemble::{assemble, Searched};
use crate::state::SharedState;

/// How often to ask the scraper whether a word has finished.
///
/// A search takes tens of seconds, so asking twice a second would be a thousand
/// pointless questions per word.
/// How long a verdict stays good for.
///
/// A shop with no website a fortnight ago almost certainly still has none, and
/// fetching it again to find that out is both wasted effort and another roll of the
/// dice on a slow server looking broken.
pub const FRESH_FOR_DAYS: i64 = 14;

const POLL: Duration = Duration::from_secs(3);

/// How long to wait for one word before giving up on it and moving to the next.
///
/// Generous: a deep search of a busy area legitimately takes minutes. One word
/// hanging must not cost the other hundred and thirty-six.
const WORD_TIMEOUT: Duration = Duration::from_secs(600);

/// How many unreadable answers in a row before we say something.
///
/// A few is a hiccup. Twenty is a bug, and twenty silent ones is how a finished
/// search waited ten minutes for nothing.
const COMPLAIN_AFTER: u32 = 5;

/// Work through every word of a run.
///
/// **One word at a time**, on purpose. Google notices a scraper that pushes, and the
/// symptom is results quietly drying up rather than an error — which is the hard kind
/// to spot. One at a time is the quiet setting, and losing a word to a block is
/// better than losing a run.
pub async fn work_through(state: SharedState, run_id: i64, options: ScrapeOptions) {
    tracing::info!(target: "scraper", run_id, "run starting");

    let client = match Client::new() {
        Ok(client) => client,
        Err(err) => return stop_with(&state, run_id, RunState::Failed, &err.to_string()).await,
    };

    // Say so plainly now rather than failing obscurely on the first word.
    if !client.is_up().await {
        return stop_with(
            &state,
            run_id,
            RunState::Failed,
            // It runs here, beside us — it is the Linux build. Saying "on your Mac"
            // sent you to the wrong machine to fix it.
            "the scraper is not running — start it here with `./bin/gmaps -web -addr :8080`",
        )
        .await;
    }

    loop {
        // Somebody may have pressed stop between words.
        if state.stop_asked(run_id) {
            return stop_with(&state, run_id, RunState::Stopped, "you pressed stop").await;
        }

        // Or pause. Here and nowhere else: this is the moment between two words, so
        // no search Google is already running for us is thrown away — and the next
        // word has not been started, so continuing costs nothing.
        if state.hold_asked(run_id) {
            return hold_here(&state, run_id).await;
        }

        let word = match state.store.next_word(run_id).await {
            Ok(Some(word)) => word,
            Ok(None) => break, // nothing left
            Err(err) => return stop_with(&state, run_id, RunState::Failed, &err.to_string()).await,
        };

        if let Err(err) = one_word(&state, &client, run_id, &word, &options).await {
            // One word failing is not a run failing. The next may be perfectly fine,
            // and losing ninety minutes to one bad search would be the wrong trade.
            tracing::warn!(target: "scraper", "⚠️ WARNING: `{}` failed: {err}", word.phrase);
            let _ = state.store.word_failed(word.id, &err).await;
        }
    }

    tracing::info!(target: "scraper", run_id, "run finished");
    let _ = state
        .store
        .set_run_state(run_id, RunState::Done, None)
        .await;
    state.clear_stop(run_id);
}

/// Put the run down where it stands, between two words.
///
/// The words keep their own states, so continuing later picks up exactly here — and
/// the queue is deliberately **not** started: a paused search still holds the floor,
/// or continuing it would find another search already going.
async fn hold_here(state: &SharedState, run_id: i64) {
    match state.store.hold_run(run_id).await {
        Ok(true) => tracing::info!(target: "scraper", run_id, "paused between words"),
        // It finished or was stopped in the same breath. Whatever it is now is the
        // truth; saying it is held would be a lie.
        Ok(false) => tracing::info!(target: "scraper", run_id, "nothing left to pause"),
        Err(err) => tracing::warn!(target: "scraper", "⚠️ WARNING: could not pause: {err}"),
    }
    state.clear_hold(run_id);
}

/// Carry a paused search on from where it stopped.
///
/// The same loop as before — [`work_through`] asks for the next word, and the one
/// left mid-flight is handed back first so its remote job is reused rather than paid
/// for twice.
pub async fn carry_on(state: SharedState, run_id: i64) -> bool {
    let options = state
        .store
        .run_options(run_id)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    match state.store.resume_run(run_id).await {
        Ok(true) => {
            state.clear_hold(run_id);
            tokio::spawn(work_through(state, run_id, options));
            true
        }
        _ => false,
    }
}

/// Work through whatever is lined up, one search at a time.
///
/// Called when a search finishes, when one is queued, and at start-up. Safe to call
/// at any of those moments: taking the next one is a single step in the database, so
/// two callers arriving together cannot both take it.
pub async fn work_through_queue(state: SharedState) {
    loop {
        match state.store.queue_paused().await {
            Ok(true) => {
                tracing::info!(target: "scraper", "the queue is holding");
                return;
            }
            Ok(false) => {}
            Err(err) => {
                tracing::warn!("⚠️ WARNING: could not read whether the queue is holding: {err}");
                return;
            }
        }

        let next = match state.store.start_next_queued().await {
            Ok(Some(run)) => run,
            Ok(None) => return,
            Err(err) => {
                tracing::error!("‼️ ERROR: could not take the next search: {err}");
                return;
            }
        };

        // What it was asked to do, not what we would have chosen today. Its own
        // area is written down with it, and is what the record says it searched —
        // it does not decide what to keep.
        let options = match state.store.run_options(next.id).await {
            Ok(Some(kept)) => kept,
            _ => ScrapeOptions::around(&state.searching()),
        };

        tracing::info!(target: "scraper", run_id = next.id, "taking the next search off the queue");
        work_through(state.clone(), next.id, options).await;
    }
}

/// Search one word and store whatever it turns up.
async fn one_word(
    state: &SharedState,
    client: &Client,
    run_id: i64,
    word: &Word,
    options: &ScrapeOptions,
) -> Result<(), String> {
    // A word already `searching` is one a restart picked up — its scraper job may
    // well still be running, so reuse it rather than searching the same thing twice.
    let remote_id = match &word.remote_id {
        Some(existing) => {
            tracing::info!(target: "scraper", "picking `{}` back up", word.phrase);
            existing.clone()
        }
        None => {
            let mut this_word = options.clone();
            this_word.keywords = vec![word.phrase.clone()];
            this_word.name = Some(format!("webleads: {}", word.phrase));

            let id = client
                .create_job(&this_word)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .word_searching(word.id, &id)
                .await
                .map_err(|e| e.to_string())?;
            tracing::info!(target: "scraper", "searching `{}`", word.phrase);
            id
        }
    };

    // Wait for it, checking now and then.
    let started = std::time::Instant::now();
    let mut unreadable = 0u32;
    loop {
        if state.stop_asked(run_id) {
            // The scraper has no way to cancel a word it is already searching, so it
            // finishes that one on its own; deleting it keeps it from starting if it
            // is still waiting, and leaves no orphan job behind.
            client.delete_job(&remote_id).await;
            return Err("you pressed stop".to_owned());
        }
        if started.elapsed() > WORD_TIMEOUT {
            client.delete_job(&remote_id).await;
            return Err("the search took too long".to_owned());
        }

        match client.state(&remote_id).await {
            Ok(RemoteState::Finished) => break,
            Ok(RemoteState::Failed) => return Err("the scraper gave up on it".to_owned()),
            Ok(other) => {
                unreadable = 0;
                // A word at depth 15 takes about three minutes, and without this line
                // those three minutes look exactly like a hung program.
                tracing::debug!(
                    target: "scraper",
                    "`{}` · {} · {}",
                    word.phrase,
                    other.word(),
                    waited(started.elapsed())
                );
                tokio::time::sleep(POLL).await;
            }
            // A hiccup talking to the scraper is not the word's fault. Keep waiting
            // rather than throwing the search away — but **say so**.
            //
            // Once it has failed a few times in a row it is not a hiccup, and the log
            // says so — a finished search could otherwise wait ten silent minutes
            // because we could not read the answer.
            Err(err) => {
                unreadable += 1;
                if unreadable == COMPLAIN_AFTER {
                    tracing::warn!(
                        target: "scraper",
                        "⚠️ WARNING: cannot read the scraper's answer after {unreadable} tries — \
                         still waiting, but something is wrong: {err}"
                    );
                }
                tokio::time::sleep(POLL).await;
            }
        }
    }

    let csv = client
        .download_csv(&remote_id)
        .await
        .map_err(|e| e.to_string())?;

    let found = store_results(state, run_id, word, &csv).await?;
    tracing::info!(target: "scraper", "`{}` returned {found}", word.phrase);

    state
        .store
        .word_done(word.id, found as i64)
        .await
        .map_err(|e| e.to_string())?;

    // Tidy up after ourselves so their job list does not fill with ours.
    client.delete_job(&remote_id).await;
    Ok(())
}

/// Put one word's results through the line that already exists.
///
/// Parse, check the websites, store, merge. Every step is tested on its own; a scrape
/// feeds it.
async fn store_results(
    state: &SharedState,
    run_id: i64,
    word: &Word,
    csv: &str,
) -> Result<usize, String> {
    let (places, skipped) = webleads_scraper::read_csv_from_str(csv).map_err(|e| e.to_string())?;
    if skipped > 0 {
        tracing::warn!(target: "scraper", "⚠️ WARNING: {skipped} unusable rows from `{}`", word.phrase);
    }
    let found = places.len();
    if places.is_empty() {
        return Ok(0);
    }

    // Anything we judged recently keeps its verdict rather than being fetched again.
    let known = state
        .store
        .recent_verdicts(FRESH_FOR_DAYS)
        .await
        .map_err(|e| e.to_string())?;

    let mut to_fetch = Vec::new();
    for place in &places {
        if !known.contains_key(&place.id) {
            to_fetch.push(place.website.clone().unwrap_or_default());
        }
    }
    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // A run is stopped between words rather than between websites, so the checking
    // itself is left to finish what it started.
    let carry_on = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let fetched = webleads_sitecheck::check_many(&state.http, &to_fetch, &counter, &carry_on).await;

    let mut outcomes = Vec::with_capacity(places.len());
    let mut fresh = vec![false; places.len()];
    let mut next = 0usize;
    for (index, place) in places.iter().enumerate() {
        match known.get(&place.id) {
            Some((verdict, tls, reason)) => outcomes.push(webleads_sitecheck::Outcome {
                verdict: *verdict,
                reason: reason.clone(),
                tls: tls.clone(),
            }),
            None => {
                outcomes.push(fetched[next].clone());
                fresh[index] = true;
                next += 1;
            }
        }
    }

    let from = Searched {
        phrase: word.phrase.clone(),
        trade: word.trade.clone(),
    };
    // Distances are measured from the centre of the city this search was in, so
    // "nearest first" and the "within" slider mean the same thing in every city. A
    // run with no city recorded measures from home — every such run is a Budapest
    // one.
    let measure_from = match state.store.city_of_run(run_id).await {
        Ok(Some(city)) => city.city.centre(),
        Ok(None) => state.home.clone(),
        Err(err) => {
            tracing::warn!(
                target: "scraper",
                "⚠️ WARNING: could not read the city of run {run_id}, measuring from home: {err}"
            );
            state.home.clone()
        }
    };
    let judged = assemble(places, outcomes, &fresh, Some(&from), &measure_from);

    state
        .store
        .record(run_id, &judged.found)
        .await
        .map_err(|e| e.to_string())?;
    let merged = state
        .store
        .merge_duplicates()
        .await
        .map_err(|e| e.to_string())?;
    if !merged.is_empty() {
        tracing::info!(target: "scraper", "{} joined as duplicates", merged.len());
    }

    Ok(found)
}

/// How long something has been going, for a person rather than for a machine.
///
/// Pure, so "two minutes and change reads as 2m 14s" is a test rather than something
/// you find out by watching a log for two minutes.
pub fn waited(how_long: Duration) -> String {
    let seconds = how_long.as_secs();
    if seconds < 60 {
        return format!("{seconds}s");
    }
    format!("{}m {}s", seconds / 60, seconds % 60)
}

/// End a run, saying why.
async fn stop_with(state: &SharedState, run_id: i64, ended: RunState, why: &str) {
    tracing::info!(target: "scraper", run_id, "run {}: {why}", ended.as_str());
    let _ = state.store.set_run_state(run_id, ended, Some(why)).await;
    state.clear_stop(run_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn we_wait_between_asking_rather_than_hammering_the_scraper() {
        // A search takes tens of seconds. Asking twice a second would be a thousand
        // pointless questions per word.
        assert!(POLL >= Duration::from_secs(1));
        assert!(POLL <= Duration::from_secs(10));
    }

    #[test]
    fn how_long_it_has_been_going_reads_as_minutes_and_seconds() {
        assert_eq!(waited(Duration::from_secs(0)), "0s");
        assert_eq!(waited(Duration::from_secs(59)), "59s");
        assert_eq!(waited(Duration::from_secs(60)), "1m 0s");
        assert_eq!(waited(Duration::from_secs(134)), "2m 14s");
        assert_eq!(waited(Duration::from_secs(3_600)), "60m 0s");
    }

    #[test]
    fn one_word_is_given_long_enough_to_be_slow() {
        // A deep search of a busy area legitimately takes minutes — but one word
        // hanging must not cost the other hundred and thirty-six.
        assert!(WORD_TIMEOUT >= Duration::from_secs(300));
        assert!(WORD_TIMEOUT <= Duration::from_secs(1800));
    }
}
