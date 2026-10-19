//! Talking to our own server from inside the browser.
//!
//! The types here mirror what the server sends. They are written out again rather
//! than shared with the `store` crate because that crate depends on SQLite, which has
//! no business being compiled into a web page. `domain` *is* shared — which is the
//! point of having a crate that depends on nothing.

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use webleads_domain::{Contact, Lead, Sort, Status, Target, Verdict};

/// A lead with the two dates the database keeps.
///
/// `#[serde(flatten)]` matches the server's own `flatten`: the JSON is one flat
/// object, not a lead nested inside a wrapper.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LeadRow {
    #[serde(flatten)]
    pub lead: Lead,
    pub first_seen: String,
    pub last_seen: String,
    /// Where this business has got to, and what you wrote about it.
    pub contact: Contact,
    /// How many times you have telephoned.
    pub call_count: i64,
    /// Today's opening hours, worked out by the server.
    ///
    /// Not worked out here: knowing what day it is in Budapest needs a timezone
    /// database, and a WebAssembly page has none.
    #[serde(default)]
    pub hours_today: Option<String>,
    /// What the shop itself would say its hours are, when that differs from yours.
    /// Absent for a business in your own city — the same time twice is noise.
    #[serde(default)]
    pub hours_there: Option<String>,
    /// The language this business's own city describes its trades in, so a Dubai row
    /// reads "barber shop" and a Budapest one "fodrászat" on the same screen.
    #[serde(default)]
    pub city_lang: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LeadsResponse {
    #[allow(
        dead_code,
        reason = "part of the server's answer; the page counts rows itself"
    )]
    pub count: usize,
    /// How many are in each state, as the server counted them.
    pub counts: Vec<(String, i64)>,
    /// The trades actually on the list, for the dropdown.
    #[serde(default)]
    pub trades: Vec<(String, i64)>,
    /// The cities with anything on the list: which one, what it is called, how many.
    #[serde(default)]
    pub cities: Vec<(i64, String, i64)>,
    /// The language the list's own controls read in — the trade dropdown and the
    /// trade picker. Each row carries its own city's language separately.
    #[serde(default)]
    pub lang: String,
    /// How far the sliders need to reach.
    #[serde(default)]
    pub extremes: Extremes,
    /// The city whose clock is yours. The list opens on it.
    #[serde(default)]
    pub home_city: Option<i64>,
    /// How many there are altogether, so the page can say "50 of 380".
    pub total: i64,
    /// How many of these the newest search found for the first time.
    /// How many are being kept off the screen because you hid them.
    #[serde(default)]
    pub hidden_count: i64,
    /// The pictures we already hold, by business id.
    #[serde(default)]
    pub shots: std::collections::HashMap<String, Shot>,
    pub leads: Vec<LeadRow>,
}

/// What comes back the moment a job is asked for — not when it finishes.
#[derive(Debug, Clone, Deserialize)]
pub struct JobStarted {
    pub job_id: u64,
    /// How many things it has to get through.
    pub rows: usize,
    /// True when a job was already going and this is that one.
    #[serde(default)]
    pub already_running: bool,
}

/// What photographing every broken website came to.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Pictures {
    pub taken: usize,
    /// How many there were to take.
    pub of: usize,
}

/// How a background job is getting on.
///
/// `#[serde(tag = "state")]` matches the server: the JSON carries a `state` field
/// saying which of these it is, and serde picks the right one from it.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobState {
    Running {
        done: usize,
        total: usize,
        step: String,
    },
    Done {
        summary: Pictures,
    },
    Failed {
        reason: String,
    },
}

/// What the server sends when something goes wrong.
#[derive(Debug, Clone, Deserialize)]
struct ErrorResponse {
    error: String,
}

/// Every request either produces a value or a sentence explaining itself.
///
/// A `String` for the error rather than a type: the only thing this side does with a
/// failure is show it to the person looking at the screen.
pub type Answer<T> = Result<T, String>;

/// What part of the list the page is asking for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Asking {
    pub status: Option<Status>,
    pub search: String,
    /// Fewest reviews. Zero means the slider has not been touched.
    pub least_reviews: i64,
    /// Lowest rating. Zero means untouched — and raising it drops the unrated.
    pub least_rating: f64,
    /// Furthest away, in metres. Zero means untouched.
    pub within_m: i64,
    pub trade: Option<String>,
    pub why: Option<Verdict>,
    pub sort: Sort,
    pub descending: bool,
    pub limit: usize,
    pub offset: usize,
    /// Only what the newest search found for the first time.
    /// Only what one search was the first to find.
    pub from_run: Option<i64>,
    /// One city only — the widest narrowing, and the first one you reach.
    pub city: Option<i64>,
    /// Only what the searches of one area first found.
    pub from_area: Option<String>,
    /// Only businesses rung at least this many times with no answer.
    pub rings_at_least: i64,
    /// Only calls promised within this stretch of time.
    pub due: Option<webleads_domain::Due>,
}

impl Asking {
    /// The question as a web address.
    pub fn query(&self) -> String {
        let mut parts = vec![
            format!("limit={}", self.limit),
            format!("offset={}", self.offset),
        ];
        if let Some(status) = self.status {
            parts.push(format!("status={}", status.as_str()));
        }
        let search = self.search.trim();
        if !search.is_empty() {
            parts.push(format!("search={}", encode(search)));
        }
        // A slider at zero is sent as nothing at all — the server would ignore it
        // anyway, and a shorter address is easier to read when something goes wrong.
        if self.least_reviews > 0 {
            parts.push(format!("least_reviews={}", self.least_reviews));
        }
        if self.least_rating > 0.0 {
            parts.push(format!("least_rating={}", self.least_rating));
        }
        if self.within_m > 0 {
            parts.push(format!("within_m={}", self.within_m));
        }
        if let Some(trade) = self.trade.as_deref().filter(|t| !t.trim().is_empty()) {
            parts.push(format!("trade={}", encode(trade)));
        }
        if let Some(why) = self.why {
            parts.push(format!("why={}", why.as_str()));
        }
        if self.sort != Sort::default() {
            parts.push(format!("sort={}", self.sort.as_str()));
        }
        if self.descending {
            parts.push("descending=true".to_owned());
        }
        if let Some(city) = self.city {
            parts.push(format!("city={city}"));
        }
        if let Some(run) = self.from_run {
            parts.push(format!("from_run={run}"));
        }
        if let Some(area) = self.from_area.as_deref().filter(|a| !a.trim().is_empty()) {
            parts.push(format!("from_area={}", encode(area)));
        }
        if self.rings_at_least > 0 {
            parts.push(format!("rings_at_least={}", self.rings_at_least));
        }
        if let Some(due) = self.due {
            parts.push(format!("due={}", due.as_str()));
        }
        parts.join("&")
    }
}

/// Fetch one page of the call list.
pub async fn leads(asking: &Asking) -> Answer<LeadsResponse> {
    ask(
        "GET /api/leads",
        Request::get(&format!("/api/leads?{}", asking.query())),
    )
    .await
}

/// Is everything we depend on running?
pub async fn health() -> Answer<Health> {
    ask("GET /api/health", Request::get("/api/health")).await
}

/// Tell the server something went wrong in here.
///
/// Best effort: if this fails there is nothing useful left to do about it, and
/// reporting a failure to report a failure would be its own small tragedy.
pub async fn report_problem(message: &str, at: Option<&str>) {
    // Built *and* sent. `.map(|r| r.send())` makes the future and drops it, which
    // looks like it works and quietly reports nothing at all.
    if let Ok(request) =
        Request::post("/api/log").json(&serde_json::json!({ "message": message, "at": at }))
    {
        let _ = request.send().await;
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Health {
    pub scraper: bool,
    pub scraper_detail: Option<String>,
    pub machine: crate::terminal::Machine,
}

/// Mark a business called, sold, or not interested.
pub async fn set_status(id: &str, status: Status) -> Answer<Contact> {
    let request = carrying(
        Request::post(&format!("/api/leads/{}/status", encode(id))),
        &serde_json::json!({ "status": status.as_str() }),
    )?;
    ask("POST /api/leads/:id/status", request).await
}

/// Correct how many times nobody answered.
pub async fn set_no_answers(id: &str, count: i64) -> Answer<Contact> {
    let request = carrying(
        Request::post(&format!("/api/leads/{}/no_answers", encode(id))),
        &serde_json::json!({ "count": count }),
    )?;
    ask("POST /api/leads/:id/no_answers", request).await
}

/// Set — or clear — when to ring a business back.
pub async fn set_ring_back(id: &str, when: Option<&str>) -> Answer<Contact> {
    let request = carrying(
        Request::post(&format!("/api/leads/{}/ring_back", encode(id))),
        &serde_json::json!({ "when": when }),
    )?;
    ask("POST /api/leads/:id/ring_back", request).await
}

/// One call you said you would make.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Due {
    pub id: String,
    pub name: String,
    pub phone: Option<String>,
    /// When you said you would ring, as `2026-09-21 14:30`.
    pub when: String,
    /// Already past. Worked out by the server, because a page compiled to
    /// WebAssembly has no timezone database and cannot tell what time it is in
    /// Budapest.
    pub overdue: bool,
    /// Whose move it is: true when they promised to ring you.
    pub theirs: bool,
    pub note: String,
}

/// The calls you said you would make, soonest first.
pub async fn coming_up() -> Answer<Vec<Due>> {
    ask("GET /api/coming_up", Request::get("/api/coming_up")).await
}

/// How the calling is going, today and altogether.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Tally {
    /// Each state and how many went into it today, as the server spells the state.
    pub today: Vec<(String, i64)>,
    pub ever: Vec<(String, i64)>,
    pub called_today: i64,
    pub called_ever: i64,
    /// When anything was last marked anywhere. The page keeps the last one it saw;
    /// when this is different, somebody marked something on another device.
    pub changed_at: String,
}

/// The scoreboard. The cheapest question the server answers, which is why a second
/// screen may ask it every few seconds.
pub async fn tally() -> Answer<Tally> {
    ask("GET /api/tally", Request::get("/api/tally")).await
}

/// Save what you wrote about a business.
pub async fn set_note(id: &str, note: &str) -> Answer<Contact> {
    let request = carrying(
        Request::post(&format!("/api/leads/{}/note", encode(id))),
        &serde_json::json!({ "note": note }),
    )?;
    ask("POST /api/leads/:id/note", request).await
}

/// Make a business id safe to put in a web address.
///
/// Fast mode builds ids like `zuglói fodrászat|47.5149,19.0778` — accents, a bar and
/// commas, none of which belong in a URL untouched.
pub(crate) fn encode(id: &str) -> String {
    id.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The trades, and the language they would be searched in.
///
/// One request for both. Asked separately, the trades won the race and the picker's
/// tier counts were built while the language was still unknown — which made every
/// tier read "(0)" for the rest of the session.
pub async fn targets() -> Answer<Trades> {
    ask("GET /api/targets", Request::get("/api/targets")).await
}

/// The trades and their language, as the server sends them.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Trades {
    #[serde(default)]
    pub trades: Vec<Target>,
    /// Empty when the server does not know, which the page says rather than guessing.
    #[serde(default)]
    pub lang: String,
    /// Where the next search would look.
    #[serde(default)]
    pub place: String,
}

/// Save an edited trade list.
///
/// The whole list, because the file is the list. What comes back is what was written,
/// already in rank order — so an edited rank takes effect without the page sorting
/// itself.
pub async fn save_targets(targets: &[Target]) -> Answer<Vec<Target>> {
    let request = carrying(Request::put("/api/targets"), &targets)?;
    ask("PUT /api/targets", request).await
}

/// Turn a response into a value, or into the server's own explanation.
///
/// Checking the status *first* is the whole point. Trying to parse an error page as
/// a lead list would produce a confusing message about JSON when the real answer —
/// "there is no out/nope.csv" — was right there in the body.
fn read<T: serde::de::DeserializeOwned>(ok: bool, status: u16, body: &str) -> Answer<T> {
    if !ok {
        return Err(match serde_json::from_str::<ErrorResponse>(body) {
            Ok(problem) => problem.error,
            Err(_) => format!("the server answered {status}"),
        });
    }
    serde_json::from_str::<T>(body).map_err(|e| format!("the server's answer made no sense: {e}"))
}

/// Something that can be sent: a request still being built, or one already built.
///
/// Two shapes reach [`ask`] — a plain `Request::get(…)` that nobody has added a body
/// to, and one that has been given JSON and is therefore already finished. This lets
/// both go through the same door rather than having two of them.
pub(crate) trait Sendable {
    async fn fire(self) -> std::result::Result<gloo_net::http::Response, gloo_net::Error>;

    /// The body going out, for the window. Empty for a request that carries none.
    fn sending(&self) -> &str {
        ""
    }
}

impl Sendable for gloo_net::http::Request {
    async fn fire(self) -> std::result::Result<gloo_net::http::Response, gloo_net::Error> {
        self.send().await
    }
}

impl Sendable for gloo_net::http::RequestBuilder {
    async fn fire(self) -> std::result::Result<gloo_net::http::Response, gloo_net::Error> {
        self.build()?.send().await
    }
}

/// A request with a body, keeping the very text of that body.
///
/// `RequestBuilder::json` writes the body and throws the text away, which leaves the
/// window nothing to show but the name of a Rust type. This serialises once and
/// holds on to what it sent.
pub(crate) struct WithBody {
    request: gloo_net::http::Request,
    body: String,
}

impl Sendable for WithBody {
    async fn fire(self) -> std::result::Result<gloo_net::http::Response, gloo_net::Error> {
        self.request.send().await
    }

    fn sending(&self) -> &str {
        &self.body
    }
}

/// Give a request a JSON body, remembering what it was.
pub(crate) fn carrying<T: Serialize>(
    builder: gloo_net::http::RequestBuilder,
    value: &T,
) -> Answer<WithBody> {
    let body =
        serde_json::to_string(value).map_err(|e| format!("could not write the request: {e}"))?;
    let request = builder
        .header("Content-Type", "application/json")
        .body(body.clone())
        .map_err(|e| format!("could not build the request: {e}"))?;
    Ok(WithBody { request, body })
}

/// A request that never reached the server, in words, with the command that fixes it.
/// Pure.
pub fn unreachable_said() -> String {
    format!(
        "the webleads server is offline — start it in the webleads folder with: {}",
        crate::link::START_ALL
    )
}

/// The questions the page asks by itself every few seconds.
///
/// Left out of the window: one that writes down its own clock ticking fills up with
/// itself and buries the search you are watching.
const BACKGROUND: [&str; 4] = [
    "/api/logs",
    "/api/runs/current",
    "/api/tally",
    "/api/health",
];

/// Is this request worth a line in the window? Everything you caused is; the page's
/// own background checks are not. Pure, so the rule is a test.
pub fn shown_in_window(what: &str) -> bool {
    !BACKGROUND.iter().any(|path| what.contains(path))
}

/// Ask the server something, and say so in the console.
///
/// Every request in the page goes through here, so the console shows what was asked,
/// what came back and how long it took — in one shape, from one place. A feature that
/// logs for itself is a feature that logs differently from all the others.
pub(crate) async fn ask<T: serde::de::DeserializeOwned>(
    what: &str,
    request: impl Sendable,
) -> Answer<T> {
    let started = crate::log::now();
    crate::log::step("ask", what);
    // And into the window on the page, as the command that would have made it — but
    // only once the answer is back, so a command and its answer are written together
    // and cannot be separated by the seven other requests the page makes at the same
    // moment.
    let show = shown_in_window(what);
    let command = crate::terminal::as_command(what);
    let outgoing = request.sending().to_owned();

    let outgoing = if outgoing.is_empty() {
        None
    } else {
        Some(format!("→ {}", crate::terminal::snip(&outgoing)))
    };

    let response = match request.fire().await {
        Ok(response) => response,
        Err(e) => {
            // What the browser says — "TypeError: Load failed" — goes to the log; you
            // get what it means and what to do about it.
            crate::log::bad("ask", &format!("{what} — could not reach the server: {e}"));
            let reason = unreachable_said();
            if show {
                crate::terminal::typed(command);
                if let Some(line) = outgoing {
                    crate::terminal::fine(line);
                }
                crate::terminal::say(format!("curl: {reason}"), crate::terminal::Tone::Bad);
            }
            return Err(reason);
        }
    };

    let status = response.status();
    let ok = response.ok();
    // As text first, then parsed from the text — so the very bytes the server sent
    // are the ones the window can show. Parsing straight into the type leaves
    // nothing to print but the type's name.
    let body = match response.text().await {
        Ok(text) => text,
        Err(e) => {
            let reason = format!("could not read the server's answer: {e}");
            crate::log::bad("ask", &format!("{what} — {reason}"));
            if show {
                crate::terminal::typed(command);
                if let Some(line) = outgoing {
                    crate::terminal::fine(line);
                }
                crate::terminal::say(reason.clone(), crate::terminal::Tone::Bad);
            }
            return Err(reason);
        }
    };

    match read::<T>(ok, status, &body) {
        Ok(value) => {
            let took = crate::log::since(started);
            crate::log::timed("ask", what, took);
            if show {
                crate::terminal::typed(command);
                if let Some(line) = outgoing {
                    crate::terminal::fine(line);
                }
                crate::terminal::say(
                    format!(
                        "{status} · {} · {} bytes",
                        crate::log::took(took),
                        body.len()
                    ),
                    crate::terminal::Tone::Dim,
                );
                crate::terminal::fine(format!("← {}", crate::terminal::snip(&body)));
            }
            Ok(value)
        }
        Err(reason) => {
            crate::log::bad("ask", &format!("{what} — {reason}"));
            if show {
                crate::terminal::typed(command);
                if let Some(line) = outgoing {
                    crate::terminal::fine(line);
                }
                crate::terminal::say(format!("{status} {reason}"), crate::terminal::Tone::Bad);
                crate::terminal::fine(format!("← {}", crate::terminal::snip(&body)));
            }
            Err(reason)
        }
    }
}

/// Stop a job after the item it is on. What it has done so far is kept.
pub async fn stop_job(id: u64) -> Answer<String> {
    ask(
        "POST /api/jobs/:id/stop",
        Request::post(&format!("/api/jobs/{id}/stop")),
    )
    .await
}

/// How is that job getting on?
pub async fn job(id: u64) -> Answer<JobState> {
    ask(
        "GET /api/jobs/:id",
        Request::get(&format!("/api/jobs/{id}")),
    )
    .await
}

/// The searching half of this conversation lives next door.
pub use crate::api_runs::*;

/// One rule about what not to show, and what it catches.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Hidden {
    pub id: i64,
    /// What you hid, in words.
    pub label: String,
    /// True for a whole chain, false for a single shop.
    pub is_chain: bool,
    /// How many businesses it is keeping off the list.
    pub catches: i64,
    pub added_on: String,
}

/// Everything you have hidden.
pub async fn hidden() -> Answer<Vec<Hidden>> {
    ask("GET /api/hidden", Request::get("/api/hidden")).await
}

/// Hide one business, or every branch of its chain.
pub async fn hide(business_id: &str, whole_chain: bool) -> Answer<Hidden> {
    let request = carrying(
        Request::post("/api/hidden"),
        &serde_json::json!({
            "business_id": business_id,
            "whole_chain": whole_chain,
        }),
    )?;
    ask("POST /api/hidden", request).await
}

/// Put one back on the list.
pub async fn unhide(id: i64) -> Answer<bool> {
    ask(
        "DELETE /api/hidden/:id",
        Request::delete(&format!("/api/hidden/{id}")),
    )
    .await
}

/// Just enough about one lead to put it on a map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub phone: Option<String>,
    pub trade: Option<String>,
    pub why: String,
    pub status: String,
}

/// How far the sliders need to reach, and what the rating one would hide.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct Extremes {
    pub most_reviews: i64,
    pub furthest_m: i64,
    pub unrated: i64,
}

/// Every match, as a pin.
///
/// Its own request rather than reading the table's rows: the map wants **all** of
/// them, and the table wants one page. Sharing meant the map quietly showed whatever
/// page happened to be loaded.
pub async fn pins(asking: &Asking) -> Answer<Vec<Pin>> {
    ask(
        "GET /api/leads/pins",
        Request::get(&format!("/api/leads/pins?{}", asking.query())),
    )
    .await
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
