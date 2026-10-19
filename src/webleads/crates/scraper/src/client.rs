//! Driving the Google Maps scraper over its REST interface.
//!
//! The scraper runs on the Mac; we run in a container. Everything here is the seam
//! between their shape and ours, kept in one file so the rest of the program never has
//! to know what their JSON looks like.

use serde::{Deserialize, Serialize};
use webleads_domain::ScrapeOptions;

use crate::{Error, Result};

/// Where the scraper answers.
///
/// **In this container**, alongside us. The binary is the Linux build and it runs
/// here quite happily, which means no part of this depends on anything being started
/// on the Mac. `127.0.0.1` is right precisely because we share a machine with it.
pub const BASE_URL: &str = "http://127.0.0.1:8080";

/// Overridden with `WEBLEADS_SCRAPER_URL` when it is somewhere else.
pub fn base_url() -> String {
    std::env::var("WEBLEADS_SCRAPER_URL").unwrap_or_else(|_| BASE_URL.to_owned())
}

/// How long to wait on an ordinary call.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// How long to wait for results. A finished job's CSV can be large and the scraper
/// builds it on demand.
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// Their id for a job. A string, because it is theirs and we do not get to choose.
pub type JobId = String;

/// A job as the scraper describes it.
///
/// **Their field names are capitalised** — `ID`, `Status`, `Name` — and getting that
/// wrong is silent: serde simply fails to parse, every poll returns an error, and a
/// finished search waits for ever. It cost a live run to find, which is why each one
/// is spelled out here rather than left to a convention.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteJob {
    #[serde(rename = "ID")]
    pub id: JobId,
    #[serde(default, rename = "Name")]
    pub name: String,
    #[serde(default, rename = "Status")]
    pub status: String,
    #[serde(default, rename = "Date")]
    pub date: Option<String>,
}

/// Where a scraper job has got to.
///
/// Their `status` is a bare string, so this is where we decide what each one means
/// rather than comparing strings at every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteState {
    /// Queued, not started.
    Waiting,
    /// Running now.
    Working,
    /// Finished, results ready to download.
    Finished,
    /// Stopped without results.
    Failed,
    /// A word we do not recognise. Treated as still working, because guessing
    /// "finished" would throw away results and guessing "failed" would lose a run.
    Unknown,
}

impl RemoteState {
    /// Read their word.
    pub fn read(status: &str) -> Self {
        match status.trim().to_lowercase().as_str() {
            "pending" | "queued" | "new" => Self::Waiting,
            "working" | "running" | "in_progress" => Self::Working,
            "ok" | "done" | "finished" | "completed" | "success" => Self::Finished,
            "failed" | "error" | "cancelled" | "canceled" => Self::Failed,
            _ => Self::Unknown,
        }
    }

    /// Is there anything left to wait for?
    pub fn still_going(self) -> bool {
        matches!(self, Self::Waiting | Self::Working | Self::Unknown)
    }

    /// One word for it, for the log.
    pub fn word(self) -> &'static str {
        match self {
            Self::Waiting => "queued",
            Self::Working => "working",
            Self::Finished => "finished",
            Self::Failed => "failed",
            Self::Unknown => "unrecognised",
        }
    }
}

/// What we send to start a job.
///
/// Their field names, not ours — `radius` rather than `radius_m`, `max_time` rather
/// than `max_time_s`, and the coordinates as **strings**, which is their choice and
/// not one we can argue with.
#[derive(Debug, Serialize)]
struct ScrapeRequest<'a> {
    name: &'a str,
    keywords: &'a [String],
    lang: &'a str,
    /// Always sent, like the depth: left out, their side reads it as nought and the
    /// job dies mid-run with `invalid zoom level: 0` — accepted, then silently
    /// stuck, which is the worst of both.
    zoom: u8,
    lat: String,
    lon: String,
    radius: u32,
    /// Always sent: the scraper refuses a job without one — `422 missing depth`.
    depth: u8,
    fast_mode: bool,
    /// Always false. Our leads have no website, so there is nothing to read an
    /// address off — it costs a great deal of time to find almost nothing.
    email: bool,
    max_time: u32,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    proxies: &'a [String],
}

/// What comes back when a job is created. Their spelling again.
#[derive(Debug, Deserialize)]
struct ScrapeResponse {
    #[serde(alias = "ID")]
    id: JobId,
}

/// A client for the scraper.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
}

impl Client {
    /// Build one pointed at the usual address.
    pub fn new() -> Result<Self> {
        Self::at(&base_url())
    }

    /// Build one pointed somewhere else. Used by the tests.
    pub fn at(base: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| Error::Scraper(format!("could not build the client: {e}")))?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        })
    }

    /// Say what we asked the scraper and what it said back.
    ///
    /// **The only place these calls are recorded.** Between two search words the
    /// program is entirely occupied with this traffic — a job created, polled every
    /// three seconds, downloaded, deleted — and none of it was written down, so a
    /// perfectly busy run looked frozen for minutes at a time.
    fn said(&self, method: &str, path: &str, status: u16, started: std::time::Instant) {
        tracing::info!(
            target: "scraper",
            "{}",
            describe_call(method, path, status, started.elapsed().as_millis())
        );
    }

    /// The body we are about to send, for whoever has the detail turned on.
    ///
    /// `debug!` rather than `info!`: this is the whole request, every time, and the
    /// terminal drops these lines unless the box is ticked.
    fn sent(&self, method: &str, path: &str, body: &str) {
        tracing::debug!(target: "scraper", "→ {method} {path} {}", snip(body));
    }

    /// And the body that came back.
    fn got(&self, method: &str, path: &str, body: &str) {
        tracing::debug!(target: "scraper", "← {method} {path} {}", snip(body));
    }

    /// Queue a scrape and get its id back.
    pub async fn create_job(&self, options: &ScrapeOptions) -> Result<JobId> {
        let name = options
            .name
            .clone()
            .unwrap_or_else(|| options.keywords.join(", "));

        let body = ScrapeRequest {
            name: &name,
            keywords: &options.keywords,
            lang: &options.lang,
            zoom: options.zoom.unwrap_or(CITY_ZOOM),
            // Strings, because that is what their interface asks for.
            lat: format!("{}", options.lat),
            lon: format!("{}", options.lon),
            radius: WIDEST_RADIUS_M,
            // Their interface demands this one, so ours turns "not chosen" into the
            // usual value rather than leaving it out and being refused.
            depth: options.depth.unwrap_or(USUAL_DEPTH),
            fast_mode: options.fast_mode,
            email: false,
            max_time: options.max_time_s,
            proxies: &options.proxies,
        };

        // Serialised here rather than by `.json()` so the very bytes that go out are
        // the ones the log shows. A log built from a second copy is a log that can
        // disagree with what was sent.
        let outgoing = serde_json::to_string(&body)
            .map_err(|e| Error::Scraper(format!("could not write the job: {e}")))?;
        self.sent("POST", "/api/v1/jobs", &outgoing);

        let started = std::time::Instant::now();
        let response = self
            .http
            .post(format!("{}/api/v1/jobs", self.base))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(outgoing)
            .send()
            .await
            .map_err(|e| Error::Scraper(describe(&e)))?;

        let status = response.status();
        self.said("POST", "/api/v1/jobs", status.as_u16(), started);
        let detail = response
            .text()
            .await
            .map_err(|e| Error::Scraper(format!("could not read the job id: {e}")))?;
        self.got("POST", "/api/v1/jobs", &detail);

        if status != reqwest::StatusCode::CREATED {
            let short: String = detail.chars().take(300).collect();
            return Err(Error::Scraper(format!(
                "the scraper refused the job ({}): {short}",
                status.as_u16()
            )));
        }

        let created: ScrapeResponse = serde_json::from_str(&detail)
            .map_err(|e| Error::Scraper(format!("could not read the job id: {e}")))?;
        Ok(created.id)
    }

    /// Every job the scraper knows about.
    pub async fn jobs(&self) -> Result<Vec<RemoteJob>> {
        let started = std::time::Instant::now();
        let response = self
            .http
            .get(format!("{}/api/v1/jobs", self.base))
            .send()
            .await
            .map_err(|e| Error::Scraper(describe(&e)))?;
        self.said("GET", "/api/v1/jobs", response.status().as_u16(), started);

        let detail = response
            .text()
            .await
            .map_err(|e| Error::Scraper(format!("could not read the job list: {e}")))?;
        self.got("GET", "/api/v1/jobs", &detail);

        // An empty list comes back as `null` rather than `[]`, so ask for the
        // permissive shape and turn it into a list ourselves.
        let jobs: Option<Vec<RemoteJob>> = serde_json::from_str(&detail)
            .map_err(|e| Error::Scraper(format!("could not read the job list: {e}")))?;
        Ok(jobs.unwrap_or_default())
    }

    /// Where one job has got to.
    ///
    /// Asked through the whole list rather than the single-job route, because the
    /// single route has been seen to answer 404 for a job that exists while it is
    /// still queued — and a job vanishing would abandon a word for no reason.
    pub async fn state(&self, id: &str) -> Result<RemoteState> {
        let jobs = self.jobs().await?;
        Ok(jobs
            .iter()
            .find(|j| j.id == id)
            .map(|j| RemoteState::read(&j.status))
            .unwrap_or(RemoteState::Unknown))
    }

    /// Fetch a finished job's results.
    pub async fn download_csv(&self, id: &str) -> Result<String> {
        let started = std::time::Instant::now();
        let response = self
            .http
            .get(format!("{}/api/v1/jobs/{id}/download", self.base))
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await
            .map_err(|e| Error::Scraper(describe(&e)))?;
        self.said(
            "GET",
            &format!("/api/v1/jobs/{id}/download"),
            response.status().as_u16(),
            started,
        );

        if !response.status().is_success() {
            return Err(Error::Scraper(format!(
                "no results for job {id} ({})",
                response.status().as_u16()
            )));
        }
        let csv = response
            .text()
            .await
            .map_err(|e| Error::Scraper(format!("could not read the results: {e}")))?;
        // Not the spreadsheet itself - a hundred businesses would bury the window.
        // Its size and its first line are what tell you it arrived and looks right.
        tracing::debug!(
            target: "scraper",
            "← GET /api/v1/jobs/{id}/download {}",
            describe_sheet(&csv)
        );
        Ok(csv)
    }

    /// Remove a job and its results from the scraper.
    ///
    /// Best effort. A job we cannot delete is untidy, not broken, and failing a run
    /// over it would be the wrong trade.
    pub async fn delete_job(&self, id: &str) {
        let started = std::time::Instant::now();
        let sent = self
            .http
            .delete(format!("{}/api/v1/jobs/{id}", self.base))
            .send()
            .await;
        if let Ok(response) = &sent {
            self.said(
                "DELETE",
                &format!("/api/v1/jobs/{id}"),
                response.status().as_u16(),
                started,
            );
            tracing::debug!(target: "scraper", "← DELETE /api/v1/jobs/{id} {}", response.status());
        }
        if let Err(err) = sent {
            tracing::warn!(target: "scraper", "⚠️ WARNING: could not delete job {id}: {err}");
        }
    }

    /// Is the scraper there at all?
    pub async fn is_up(&self) -> bool {
        self.jobs().await.is_ok()
    }
}

/// What to send when nothing has been chosen.
///
/// **Neither one can actually be left out.** A job with no depth is refused outright
/// (`422 missing depth`), and one with no zoom is accepted and then dies while it
/// runs (`invalid zoom level: 0`), leaving our side waiting on a job that will never
/// finish. So "not chosen" means "we choose": a city-wide view, and the usual depth.
///
/// **Both come from `domain`, and there is exactly one of each**, so the depth the page
/// promises is the depth the scraper receives.
pub use webleads_domain::options::{CITY_ZOOM, USUAL_DEPTH};

/// How wide a radius the scraper is asked for: as much as it will take.
///
/// **Nothing is thrown away for being far**, so there is no reason to ask for less —
/// and in fast mode the scraper applies the radius as a hard filter of its own, so a
/// small one can empty a run that should have found plenty.
pub const WIDEST_RADIUS_M: u32 = 50_000;

/// One call to the scraper, in the shape a terminal reads well.
///
/// Pure, so the shape is a test rather than something you squint at in a log: the
/// method, what we asked for, what came back, and how long it took.
pub fn describe_call(method: &str, path: &str, status: u16, took_ms: u128) -> String {
    format!("{method} {path} → {status} · {took_ms} ms")
}

/// The most of a body that goes into the log.
///
/// The job list is asked for every three seconds and grows with every job on it;
/// whole, it would be most of the log and none of it would be read.
const BODY_SHOWN: usize = 600;

/// A body short enough to read on one screen, saying so when it was cut.
///
/// Pure, and tested - "it says how much was left out" is the sort of thing that
/// quietly stops being true.
pub fn snip(body: &str) -> String {
    let tidy = body.trim();
    let length = tidy.chars().count();
    if length <= BODY_SHOWN {
        return tidy.to_owned();
    }
    let kept: String = tidy.chars().take(BODY_SHOWN).collect();
    format!("{kept}… (of {length} characters)")
}

/// A spreadsheet in one line: how big it is and what its first line says.
///
/// Pure, so what the log promises about a download is a test.
pub fn describe_sheet(csv: &str) -> String {
    let rows = csv.lines().count().saturating_sub(1);
    let first = csv.lines().next().unwrap_or("").trim();
    let head: String = first.chars().take(120).collect();
    format!("{} bytes · {rows} rows · {head}", csv.len())
}

/// Say what went wrong in words rather than in library jargon.
fn describe(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        return "the scraper did not answer in time".to_owned();
    }
    if err.is_connect() {
        return "the scraper is not running".to_owned();
    }
    let text = err.to_string();
    let short: String = text.chars().take(120).collect();
    format!("the scraper could not be reached ({short})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_body_goes_into_the_log_whole() {
        assert_eq!(snip("{\"ok\":true}"), "{\"ok\":true}");
        assert_eq!(snip("  spaced  "), "spaced");
    }

    #[test]
    fn a_long_body_is_cut_and_says_how_much_was_left_out() {
        let long = "x".repeat(BODY_SHOWN + 50);
        let cut = snip(&long);
        assert!(cut.starts_with(&"x".repeat(BODY_SHOWN)));
        assert!(cut.ends_with(&format!("(of {} characters)", BODY_SHOWN + 50)));
    }

    #[test]
    fn cutting_a_body_never_splits_a_letter_in_half() {
        // Hungarian names are full of these, and cutting by bytes would produce
        // rubbish the terminal cannot draw.
        let long = "\u{151}".repeat(BODY_SHOWN + 10);
        assert_eq!(
            snip(&long).chars().filter(|c| *c == '\u{151}').count(),
            BODY_SHOWN
        );
    }

    #[test]
    fn a_downloaded_spreadsheet_is_described_by_its_size_and_first_line() {
        let csv = "name,phone\nFodr\u{e1}sz,+36 1 111 1111\nKulcs,+36 1 222 2222\n";
        let said = describe_sheet(csv);
        assert!(said.contains("2 rows"), "{said}");
        assert!(said.contains("name,phone"), "{said}");
        assert!(said.contains(&format!("{} bytes", csv.len())), "{said}");
    }

    #[test]
    fn an_empty_spreadsheet_is_described_without_falling_over() {
        let said = describe_sheet("");
        assert!(said.contains("0 rows"), "{said}");
    }

    #[test]
    fn every_job_state_has_a_word_for_the_log() {
        assert_eq!(RemoteState::Waiting.word(), "queued");
        assert_eq!(RemoteState::Working.word(), "working");
        assert_eq!(RemoteState::Finished.word(), "finished");
        assert_eq!(RemoteState::Failed.word(), "failed");
        assert_eq!(RemoteState::Unknown.word(), "unrecognised");
    }

    #[test]
    fn their_words_for_a_job_are_read_into_ours() {
        assert_eq!(RemoteState::read("pending"), RemoteState::Waiting);
        assert_eq!(RemoteState::read("working"), RemoteState::Working);
        assert_eq!(RemoteState::read("ok"), RemoteState::Finished);
        assert_eq!(RemoteState::read("failed"), RemoteState::Failed);
    }

    #[test]
    fn their_capitals_and_spacing_do_not_matter() {
        assert_eq!(RemoteState::read("  OK  "), RemoteState::Finished);
        assert_eq!(RemoteState::read("Working"), RemoteState::Working);
    }

    #[test]
    fn the_depth_the_page_shows_is_the_depth_the_scraper_is_sent() {
        // The test that would have caught the duplicate: this file kept its own
        // `USUAL_DEPTH` beside the domain's, agreeing by luck. Change one and the
        // page promises a depth the scraper never receives.
        let ours = ScrapeOptions::default();
        assert_eq!(ours.depth, None, "off until you ask");
        let sent = ours.depth.unwrap_or(USUAL_DEPTH);
        assert_eq!(sent, webleads_domain::options::USUAL_DEPTH);
        assert_eq!(
            sent, 15,
            "measured: 15 returns 108 businesses, 18 returns none"
        );
    }

    #[test]
    fn the_zoom_is_the_same_on_both_sides_too() {
        let ours = ScrapeOptions::default();
        assert_eq!(ours.zoom, None);
        assert_eq!(
            ours.zoom.unwrap_or(CITY_ZOOM),
            webleads_domain::options::CITY_ZOOM
        );
    }

    #[test]
    fn the_scraper_is_always_asked_for_as_much_as_it_will_take() {
        // Nothing is dropped for being far, so asking for less can only lose leads —
        // and in fast mode the scraper filtered by this number itself, which is how
        // a perfectly good search came back empty.
        assert_eq!(WIDEST_RADIUS_M, 50_000);
    }

    #[test]
    fn a_setting_we_have_not_chosen_is_sent_as_the_one_we_would_choose() {
        // Not sent as a zero or a guess: left out, so the scraper uses its own.
        let ours = ScrapeOptions::default();
        assert_eq!(ours.zoom, None);
        assert_eq!(ours.depth, None);

        let body = ScrapeRequest {
            name: "x",
            keywords: &ours.keywords,
            lang: &ours.lang,
            zoom: ours.zoom.unwrap_or(CITY_ZOOM),
            lat: "47.5".to_owned(),
            lon: "19.1".to_owned(),
            radius: WIDEST_RADIUS_M,
            depth: ours.depth.unwrap_or(USUAL_DEPTH),
            fast_mode: ours.fast_mode,
            email: false,
            max_time: ours.max_time_s,
            proxies: &ours.proxies,
        };
        let json = serde_json::to_string(&body).expect("writes");
        // Neither can be left out: without a depth the job is refused, and without a
        // zoom it is accepted and then dies while it runs. "Not chosen" becomes the
        // value we would choose.
        assert!(json.contains("\"zoom\":12"), "{json}");
        assert!(json.contains("\"depth\":15"), "{json}");

        // And when you do choose one, it goes.
        let chosen = ScrapeRequest {
            zoom: 17,
            depth: 14,
            ..body
        };
        let json = serde_json::to_string(&chosen).expect("writes");
        assert!(json.contains("\"zoom\":17"), "{json}");
        assert!(json.contains("\"depth\":14"), "{json}");
    }

    #[test]
    fn a_call_to_the_scraper_reads_as_one_line() {
        // What the terminal shows between words. Method, what we asked for, what came
        // back, how long — and nothing else, because there are hundreds of these.
        assert_eq!(
            describe_call("POST", "/api/v1/jobs", 201, 34),
            "POST /api/v1/jobs → 201 · 34 ms"
        );
        assert_eq!(
            describe_call("GET", "/api/v1/jobs/abc/download", 200, 1_204),
            "GET /api/v1/jobs/abc/download → 200 · 1204 ms"
        );
    }

    #[test]
    fn a_refusal_is_said_as_plainly_as_a_success() {
        // A 404 or a 500 must read the same way, or the eye skips past the one line
        // that mattered.
        let said = describe_call("DELETE", "/api/v1/jobs/abc", 404, 3);
        assert!(said.contains("404"), "{said}");
        assert!(said.starts_with("DELETE "), "{said}");
    }

    #[test]
    fn a_word_we_do_not_know_is_treated_as_still_going() {
        // Guessing "finished" would throw away results that never arrived; guessing
        // "failed" would abandon a word for no reason. Waiting is the safe read.
        let unknown = RemoteState::read("something-new");
        assert_eq!(unknown, RemoteState::Unknown);
        assert!(unknown.still_going());
    }

    #[test]
    fn only_finishing_and_failing_stop_the_waiting() {
        assert!(RemoteState::Waiting.still_going());
        assert!(RemoteState::Working.still_going());
        assert!(!RemoteState::Finished.still_going());
        assert!(!RemoteState::Failed.still_going());
    }

    #[test]
    fn we_look_for_the_scraper_beside_us() {
        // It is the Linux build and it runs in this container, so loopback is right.
        // Pointing at the Mac was the mistake — nothing here needs anything started
        // over there.
        assert!(BASE_URL.contains("127.0.0.1"));
        assert!(!BASE_URL.contains("host.docker.internal"));
    }

    #[test]
    fn the_address_can_be_moved_without_a_rebuild() {
        // If it ever does end up somewhere else, an environment variable is a far
        // better answer than editing this line.
        assert_eq!(base_url(), BASE_URL, "nothing is overriding it here");
    }

    #[test]
    fn a_trailing_slash_does_not_produce_a_double_one() {
        let tidy = Client::at("http://example.com/").expect("builds");
        assert_eq!(tidy.base, "http://example.com");
    }

    #[test]
    fn what_we_send_uses_their_field_names_not_ours() {
        let options = ScrapeOptions {
            name: Some("Zugló barbers".into()),
            keywords: vec!["borbély Budapest".into()],
            ..Default::default()
        };
        let body = ScrapeRequest {
            name: options.name.as_deref().unwrap_or(""),
            keywords: &options.keywords,
            lang: &options.lang,
            zoom: options.zoom.unwrap_or(CITY_ZOOM),
            lat: format!("{}", options.lat),
            lon: format!("{}", options.lon),
            radius: WIDEST_RADIUS_M,
            depth: options.depth.unwrap_or(USUAL_DEPTH),
            fast_mode: options.fast_mode,
            email: false,
            max_time: options.max_time_s,
            proxies: &options.proxies,
        };
        let json = serde_json::to_string(&body).expect("serialises");

        // Theirs, not ours.
        assert!(json.contains("\"radius\""), "{json}");
        assert!(json.contains("\"max_time\""), "{json}");
        assert!(!json.contains("radius_m"), "{json}");
        assert!(!json.contains("max_time_s"), "{json}");
        // Coordinates as strings, which is their choice.
        assert!(json.contains("\"lat\":\""), "{json}");
    }

    #[test]
    fn we_never_ask_for_email_addresses() {
        // Our leads have no website, so there is nothing to read an address off. It
        // costs a great deal of time to find almost nothing, and we removed it on
        // purpose — this is the guard against it creeping back.
        let options = ScrapeOptions::default();
        let body = ScrapeRequest {
            name: "",
            keywords: &options.keywords,
            lang: &options.lang,
            zoom: options.zoom.unwrap_or(CITY_ZOOM),
            lat: String::new(),
            lon: String::new(),
            radius: WIDEST_RADIUS_M,
            depth: options.depth.unwrap_or(USUAL_DEPTH),
            fast_mode: options.fast_mode,
            email: false,
            max_time: options.max_time_s,
            proxies: &options.proxies,
        };
        let json = serde_json::to_string(&body).expect("serialises");
        assert!(json.contains("\"email\":false"), "{json}");
    }

    #[test]
    fn an_empty_proxy_list_is_left_out_entirely() {
        // Sending `"proxies": []` is harmless but noisy; more to the point, a proxy
        // must never appear anywhere it could be read back, and the safest list is
        // one that is not there.
        let options = ScrapeOptions::default();
        let body = ScrapeRequest {
            name: "",
            keywords: &options.keywords,
            lang: &options.lang,
            zoom: options.zoom.unwrap_or(CITY_ZOOM),
            lat: String::new(),
            lon: String::new(),
            radius: WIDEST_RADIUS_M,
            depth: options.depth.unwrap_or(USUAL_DEPTH),
            fast_mode: options.fast_mode,
            email: false,
            max_time: options.max_time_s,
            proxies: &options.proxies,
        };
        let json = serde_json::to_string(&body).expect("serialises");
        assert!(!json.contains("proxies"), "{json}");
    }

    /// Exactly what the scraper sends, copied from a live run.
    const A_REAL_ANSWER: &str = r#"[{"ID":"1e9bd4fd-8b67-4a81-bd5a-1af81e0fda7f",
        "Name":"webleads: borbély Budapest","Date":"2026-09-19T14:02:55Z",
        "Status":"ok","Data":{"keywords":["borbély Budapest"],"lang":"hu"}}]"#;

    #[test]
    fn their_capitalised_field_names_are_read_correctly() {
        // This is the bug that cost a live run. Their JSON says `ID` and `Status`,
        // not `id` and `status`. Getting it wrong is silent: parsing simply fails,
        // every poll errors, and a finished search waits for ever.
        let jobs: Vec<RemoteJob> = serde_json::from_str(A_REAL_ANSWER).expect("parses");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "1e9bd4fd-8b67-4a81-bd5a-1af81e0fda7f");
        assert_eq!(jobs[0].status, "ok");
        assert_eq!(jobs[0].name, "webleads: borbély Budapest");
    }

    #[test]
    fn a_finished_job_in_their_own_words_reads_as_finished() {
        let jobs: Vec<RemoteJob> = serde_json::from_str(A_REAL_ANSWER).expect("parses");
        assert_eq!(RemoteState::read(&jobs[0].status), RemoteState::Finished);
        assert!(!RemoteState::read(&jobs[0].status).still_going());
    }

    #[test]
    fn an_empty_job_list_comes_back_as_null_not_as_a_list() {
        // Their idea, not ours, and it would fail to parse as a plain Vec.
        let empty: Option<Vec<RemoteJob>> = serde_json::from_str("null").expect("parses");
        assert_eq!(empty.unwrap_or_default().len(), 0);
    }

    #[test]
    fn the_id_of_a_newly_created_job_is_read_whichever_way_they_spell_it() {
        let lower: ScrapeResponse = serde_json::from_str(r#"{"id":"abc"}"#).expect("parses");
        assert_eq!(lower.id, "abc");
        let upper: ScrapeResponse = serde_json::from_str(r#"{"ID":"abc"}"#).expect("parses");
        assert_eq!(upper.id, "abc");
    }
}
