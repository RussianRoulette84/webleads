//! Asking the server about searching: starting one, watching it, the queue, and the
//! record of the ones already done.
//!
//! Split from the rest of the asking because it is a job of its own — the call list
//! and the searching that fills it are two different conversations with the server.

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

use crate::api::{ask, encode, Answer, Asking, JobStarted};

/// Where a run has got to, as the server describes it.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Run {
    pub id: i64,
    pub state: String,
    pub source: String,
    pub note: Option<String>,
    pub words_total: i64,
    pub words_done: i64,
    pub found_total: i64,
    /// The word being searched right now, if one is.
    #[serde(default)]
    pub now_searching: Option<String>,
    /// Where this search was looking.
    #[serde(default)]
    pub area: Option<String>,
    /// Which city it was looking in. Empty for the searches made before cities
    /// existed, which is why the strip shows those whichever city you pick.
    #[serde(default)]
    pub city: Option<i64>,
    /// The day it started.
    #[serde(default)]
    pub started_at: String,
    /// When it ended, for one that has. Empty while it is still going, and empty for
    /// the searches made before this was written down.
    #[serde(default)]
    pub finished_at: Option<String>,
    /// How many on the call list this search was the first to find.
    #[serde(default)]
    pub leads_found: i64,
}

impl Run {
    /// Is there anything left to do?
    ///
    /// A paused search counts: it has half its words left, and the buttons have to
    /// offer to continue it rather than treating it as finished.
    pub fn still_going(&self) -> bool {
        self.state == "working" || self.state == "held"
    }

    /// Is it moving right now?
    pub fn working_now(&self) -> bool {
        self.state == "working"
    }

    /// Is it waiting for you to press continue?
    pub fn held(&self) -> bool {
        self.state == "held"
    }
}

/// One search word and what it returned. The hit report is a list of these.
#[derive(Debug, Clone, Deserialize)]
pub struct Word {
    pub phrase: String,
    pub trade: Option<String>,
    pub state: String,
    /// How many businesses came back. **Nought is the interesting number** — it
    /// usually means the Hungarian is wrong.
    pub found: Option<i64>,
    pub note: Option<String>,
}

/// What to search for.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StartRequest {
    pub tiers: Vec<String>,
    pub trades: Vec<String>,
    pub words: Vec<String>,
    /// The scraper's own settings. Left out, the server uses its defaults — which is
    /// what happened on every search before now, so moving one changed nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<webleads_domain::ScrapeOptions>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartResponse {
    pub run_id: i64,
    #[allow(
        dead_code,
        reason = "the run itself reports the count once it is going"
    )]
    pub words: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Estimate {
    pub words: usize,
    /// `None` until a run has finished. We would rather say "not yet known" than
    /// invent a number you might plan an afternoon around.
    pub seconds: Option<u64>,
}

/// Start searching.
pub async fn start_run(request: &StartRequest) -> Answer<StartResponse> {
    let request = crate::api::carrying(Request::post("/api/runs"), request)?;
    ask("POST /api/runs", request).await
}

/// What would that cost, before you commit to it.
pub async fn estimate(request: &StartRequest) -> Answer<Estimate> {
    let request = crate::api::carrying(Request::post("/api/runs/estimate"), request)?;
    ask("POST /api/runs/estimate", request).await
}

/// The run going on now, if there is one.
pub async fn current_run() -> Answer<Option<Run>> {
    ask("GET /api/runs/current", Request::get("/api/runs/current")).await
}

/// The searches already done, newest first.
pub async fn past_runs() -> Answer<Vec<Run>> {
    ask("GET /api/runs/past", Request::get("/api/runs/past")).await
}

/// Every word of a run and what it returned.
pub async fn run_words(id: i64) -> Answer<Vec<Word>> {
    ask(
        "GET /api/runs/:id/words",
        Request::get(&format!("/api/runs/{id}/words")),
    )
    .await
}

/// One log's new lines, and where to carry on from.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Part {
    pub at: u64,
    pub lines: Vec<String>,
    #[serde(default)]
    pub skipped: bool,
}

/// What both logs have written since we last looked.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Tail {
    pub backend: Part,
    pub scraper: Part,
}

/// What the server has written to its logs since these offsets.
///
/// Deliberately **not** shown in the terminal itself — see `ask`: a window that logs
/// itself asking for the log fills up with itself in seconds.
pub async fn logs(backend: Option<u64>, scraper: Option<u64>, debug: bool) -> Answer<Tail> {
    let mut parts = vec![format!("debug={debug}")];
    if let Some(at) = backend {
        parts.push(format!("backend={at}"));
    }
    if let Some(at) = scraper {
        parts.push(format!("scraper={at}"));
    }
    ask(
        "GET /api/logs",
        Request::get(&format!("/api/logs?{}", parts.join("&"))),
    )
    .await
}

/// Finish the word it is on, then hold.
pub async fn pause_run(id: i64) -> Answer<String> {
    ask(
        "POST /api/runs/:id/pause",
        Request::post(&format!("/api/runs/{id}/pause")),
    )
    .await
}

/// Carry on from where it was paused.
pub async fn continue_run(id: i64) -> Answer<String> {
    ask(
        "POST /api/runs/:id/continue",
        Request::post(&format!("/api/runs/{id}/continue")),
    )
    .await
}

/// Abandon a run, keeping everything already found.
pub async fn stop_run(id: i64) -> Answer<String> {
    ask(
        "POST /api/runs/:id/stop",
        Request::post(&format!("/api/runs/{id}/stop")),
    )
    .await
}

/// One place you can search.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SavedArea {
    pub id: i64,
    pub label: String,
    #[allow(
        dead_code,
        reason = "shown only in the lookup, kept so the shape matches"
    )]
    pub address: String,
    pub lat: f64,
    pub lon: f64,
    /// The city it belongs to, if any.
    #[serde(default)]
    pub city_id: Option<i64>,
}

/// The areas, and which one the next search will use.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Areas {
    pub areas: Vec<SavedArea>,
    pub chosen: Option<i64>,
}

/// One place the address lookup found.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Place {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    /// Which town the lookup says this is in, so adding an area can offer the city
    /// rather than ask for it.
    #[serde(default)]
    pub city: Option<String>,
    /// And its country, for reading.
    #[serde(default)]
    pub country: Option<String>,
    /// Two letters. A city we have not seen before takes its clock and its search
    /// language from this.
    #[serde(default)]
    pub country_code: Option<String>,
}

/// A city, as far as where its distances come from.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CityCentre {
    pub id: i64,
    pub name: String,
    pub centre_lat: f64,
    pub centre_lon: f64,
    /// "Hősök tere, Budapest". Empty when nobody has said.
    #[serde(default)]
    pub centre_label: Option<String>,
}

/// What moving a centre came to.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MovedCentre {
    /// How many leads came out with a different distance.
    pub moved: usize,
}

/// What is lined up, and whether the queue is holding.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Queue {
    pub waiting: Vec<Run>,
    pub paused: bool,
}

/// Everywhere you can search.
pub async fn areas() -> Answer<Areas> {
    ask("GET /api/areas", Request::get("/api/areas")).await
}

/// Look an address up.
pub async fn look_up(address: &str) -> Answer<Vec<Place>> {
    ask(
        "GET /api/places",
        Request::get(&format!("/api/places?q={}", encode(address))),
    )
    .await
}

/// Keep a place you looked up as an area.
pub async fn add_area(label: &str, place: &Place, city: Option<&str>) -> Answer<SavedArea> {
    let request = crate::api::carrying(
        Request::post("/api/areas"),
        &serde_json::json!({
            "label": label,
            "address": place.label,
            "lat": place.lat,
            "lon": place.lon,
            // The city this area belongs to. Whatever the lookup reported unless you
            // said otherwise — and a city we already keep is simply used again, which
            // is how Zugló and Óbuda both end up under Budapest.
            "city": city,
            "country_code": place.country_code,
        }),
    )?;
    ask("POST /api/areas", request).await
}

/// Your clock, and every one you could choose instead.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Zones {
    pub zone: String,
    pub all: Vec<String>,
}

/// The clock ring-backs and "today" are read in.
pub async fn your_zone() -> Answer<Zones> {
    ask("GET /api/your_zone", Request::get("/api/your_zone")).await
}

/// Read every time in this clock from now on.
pub async fn set_your_zone(zone: &str) -> Answer<String> {
    let request = crate::api::carrying(
        Request::post("/api/your_zone"),
        &serde_json::json!({ "zone": zone }),
    )?;
    ask("POST /api/your_zone", request).await
}

/// Remember which language searches are made in.
pub async fn set_search_lang(lang: &str) -> Answer<String> {
    let request = crate::api::carrying(
        Request::post("/api/search_lang"),
        &serde_json::json!({ "lang": lang }),
    )?;
    ask("POST /api/search_lang", request).await
}

/// Every city, with where each one is measured from.
pub async fn cities() -> Answer<Vec<CityCentre>> {
    ask("GET /api/cities", Request::get("/api/cities")).await
}

/// Measure a city from a place you looked up.
pub async fn move_centre(city: i64, place: &Place) -> Answer<MovedCentre> {
    let request = crate::api::carrying(
        Request::post(&format!("/api/cities/{city}/centre")),
        &serde_json::json!({ "label": place.label, "lat": place.lat, "lon": place.lon }),
    )?;
    ask("POST /api/cities/:id/centre", request).await
}

/// Search here next.
pub async fn choose_area(id: i64) -> Answer<SavedArea> {
    ask(
        "POST /api/areas/:id/choose",
        Request::post(&format!("/api/areas/{id}/choose")),
    )
    .await
}

/// Forget an area. The leads it found stay.
pub async fn remove_area(id: i64) -> Answer<bool> {
    ask(
        "DELETE /api/areas/:id",
        Request::delete(&format!("/api/areas/{id}")),
    )
    .await
}

/// What is lined up.
pub async fn queue() -> Answer<Queue> {
    ask("GET /api/runs/queue", Request::get("/api/runs/queue")).await
}

/// Take one out of the line.
pub async fn forget_queued(id: i64) -> Answer<bool> {
    ask(
        "DELETE /api/runs/queue/:id",
        Request::delete(&format!("/api/runs/queue/{id}")),
    )
    .await
}

/// What forgetting a past search would take with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct ForgetCost {
    pub words: i64,
    /// Businesses that would leave the call list for good.
    pub businesses_lost: i64,
    /// Businesses that would stay, wearing an earlier search's verdict.
    pub businesses_kept: i64,
}

/// Ask the price before paying it.
pub async fn forget_cost(id: i64) -> Answer<ForgetCost> {
    ask(
        "GET /api/runs/:id/forget_cost",
        Request::get(&format!("/api/runs/{id}/forget_cost")),
    )
    .await
}

/// Forget a search that is over.
pub async fn forget_run(id: i64) -> Answer<bool> {
    ask(
        "DELETE /api/runs/:id",
        Request::delete(&format!("/api/runs/{id}")),
    )
    .await
}

/// Hold the queue after the current search, or let it carry on.
pub async fn hold_queue(paused: bool) -> Answer<Queue> {
    let request = crate::api::carrying(
        Request::post("/api/runs/queue/pause"),
        &serde_json::json!({ "paused": paused }),
    )?;
    ask("POST /api/runs/queue/pause", request).await
}

/// A picture of one business's website, and what it turned out to show.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Shot {
    pub url: String,
    pub answered: String,
}

/// Photograph one business's website.
pub async fn take_shot(id: &str) -> Answer<Shot> {
    ask(
        "POST /api/leads/:id/shot",
        Request::post(&format!("/api/leads/{}/shot", encode(id))),
    )
    .await
}

/// Photograph every website the filter is showing.
pub async fn take_shots(asking: &Asking) -> Answer<JobStarted> {
    ask(
        "POST /api/shots",
        Request::post(&format!("/api/shots?{}", asking.query())),
    )
    .await
}
