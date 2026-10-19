//! What every request handler can reach.
//!
//! Shared state rather than globals: axum hands this to handlers as an extractor, so
//! a handler's signature says exactly what it needs, and a test can build a different
//! one without touching anything global.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use webleads_domain::{Area, Target, Tier};
use webleads_store::Store;

/// The starting point until you set your own on the page: Hősök tere, Budapest.
pub const CENTRE_LAT: f64 = 47.514900;
pub const CENTRE_LON: f64 = 19.077800;
/// How many businesses one search may pull from Google before we stop asking.
pub const MAX_RESULTS: u32 = 200;
pub const AREA_LABEL: &str = "Budapest";
pub const AREA_ADDRESS: &str = "Hősök tere, Budapest, Hungary";

/// **Home: where a search with no city is measured from.**
///
/// Every city carries its own centre, which you can move on the page; this is only
/// for a search never filed under one. It carries no radius on purpose: home
/// measures, it never decides what is in and what is out.
pub fn home() -> Area {
    Area {
        label: "home".to_owned(),
        address: AREA_ADDRESS.to_owned(),
        lat: CENTRE_LAT,
        lon: CENTRE_LON,
        max_results: MAX_RESULTS,
    }
}

/// The area a search looks at, until you choose another one.
///
/// Whole city, no fence — which is what has been searched all along.
pub fn whole_city() -> Area {
    Area {
        label: AREA_LABEL.to_owned(),
        address: AREA_ADDRESS.to_owned(),
        lat: CENTRE_LAT,
        lon: CENTRE_LON,
        max_results: MAX_RESULTS,
    }
}

/// `Arc` so cloning this for each request copies a pointer, not the data.
pub type SharedState = Arc<AppState>;

#[derive(Debug)]
pub struct AppState {
    pub store: Store,
    /// The trades. Read at start-up and re-read whenever the list is edited on the
    /// page, which is why it is behind a lock rather than owned outright.
    pub targets: std::sync::RwLock<Vec<Target>>,
    /// Where the trades file lives, so an edit can be written back to it.
    pub targets_path: std::path::PathBuf,
    /// Where you are. Every distance on every row is measured from this, whichever
    /// area the lead was found in.
    pub home: Area,
    /// The area the next search will look at. Chosen, and therefore changing — so it
    /// travels with the run rather than being read from here once a run is going.
    pub searching: std::sync::RwLock<Area>,
    /// Where the pictures of broken websites are kept, and served from.
    pub shots_dir: PathBuf,
    /// Where our own log files are written, so the terminal can read them back.
    pub log_dir: PathBuf,
    pub http: webleads_sitecheck::Clients,
    /// Imports that have run or are running, so the page can watch one.
    pub jobs: Jobs,
    /// Runs somebody has asked to stop.
    ///
    /// A set rather than a flag: the loop checks it between words and again while
    /// waiting, so pressing stop takes effect within a few seconds rather than at the
    /// end of a ninety-minute run.
    pub stopping: std::sync::Mutex<std::collections::HashSet<i64>>,
    /// Runs somebody has asked to pause.
    ///
    /// Beside the stop list and read the same way, but acted on differently: a stop
    /// abandons the word being searched, a pause lets it finish first. Never
    /// abandoning a search Google is already running for us is the whole point —
    /// that search is paid for, and half of it is worth nothing.
    pub holding: std::sync::Mutex<std::collections::HashSet<i64>>,
}

impl AppState {
    /// Has somebody asked this run to stop?
    pub fn stop_asked(&self, run_id: i64) -> bool {
        self.stopping
            .lock()
            .expect("the stop list is never poisoned")
            .contains(&run_id)
    }

    /// Ask a run to stop. It notices within a few seconds.
    pub fn ask_to_stop(&self, run_id: i64) {
        self.stopping
            .lock()
            .expect("the stop list is never poisoned")
            .insert(run_id);
    }

    /// Has somebody asked this run to pause?
    pub fn hold_asked(&self, run_id: i64) -> bool {
        self.holding
            .lock()
            .expect("the hold list is never poisoned")
            .contains(&run_id)
    }

    /// Ask a run to pause. It notices at the end of the word it is on.
    pub fn ask_to_hold(&self, run_id: i64) {
        self.holding
            .lock()
            .expect("the hold list is never poisoned")
            .insert(run_id);
    }

    /// Forget the request, once it has been obeyed or taken back.
    pub fn clear_hold(&self, run_id: i64) {
        self.holding
            .lock()
            .expect("the hold list is never poisoned")
            .remove(&run_id);
    }

    /// Forget the request once the run has actually stopped.
    /// The area the next search will look at.
    ///
    /// Cloned rather than handed out behind the lock: a caller holding it across an
    /// `await` would block every other reader, and an area is four numbers and a
    /// name.
    pub fn searching(&self) -> Area {
        self.searching
            .read()
            .expect("the chosen area is never poisoned")
            .clone()
    }

    /// The trades, as they stand.
    ///
    /// Cloned rather than handed out behind the lock, like the chosen area: a caller
    /// holding it across an `await` would block every other reader.
    pub fn targets(&self) -> Vec<Target> {
        self.targets
            .read()
            .expect("the trades are never poisoned")
            .clone()
    }

    /// Replace the trades, and write them back to the file they came from.
    ///
    /// The file first: if it cannot be written the list in memory is left alone, so
    /// what the page shows after a failed save is still what would be searched.
    pub fn set_targets(&self, targets: Vec<Target>) -> anyhow::Result<()> {
        write_targets(&self.targets_path, &targets)?;
        *self.targets.write().expect("the trades are never poisoned") = targets;
        Ok(())
    }

    /// Which language the place we are searching describes its trades in.
    ///
    /// **The one answer, used by the searching and by the screen.** They were two
    /// walks through the same three tables, and they disagreed: the screen read the
    /// city's language while a search quietly asked in Hungarian, so choosing Dubai
    /// and pressing go searched Dubai for `vízvezeték-szerelő`.
    ///
    /// `None` when the place has no city, or its city no language. Not a guess — a
    /// wrong language here is a run that finds nothing and takes an afternoon to say
    /// so.
    pub async fn searching_language(&self, city: Option<i64>) -> Option<String> {
        // The city you asked about, if you asked about one.
        if let Some(id) = city {
            if let Ok(Some(found)) = self.store.city(id).await {
                return Some(found.city.lang).filter(|lang| !lang.trim().is_empty());
            }
            // You named a city we do not keep. Saying nothing beats answering about
            // a different one.
            return None;
        }
        // Otherwise the city of the place the next search will look at.
        let area = self.store.chosen_area().await.ok()??;
        let city = self.store.city_of_area(area.id).await.ok()??;
        Some(city.city.lang).filter(|lang| !lang.trim().is_empty())
    }

    /// Choose the area the next search will look at.
    ///
    /// The database remembers it across restarts; this is the copy every request
    /// reads, so choosing an area is one write rather than a query per request.
    pub fn search_in(&self, area: Area) {
        *self
            .searching
            .write()
            .expect("the chosen area is never poisoned") = area;
    }

    pub fn clear_stop(&self, run_id: i64) {
        self.stopping
            .lock()
            .expect("the stop list is never poisoned")
            .remove(&run_id);
    }
}

/// How an import is getting on.
///
/// A job can take minutes — a few hundred websites at eight seconds each — so the
/// work happens in the background and this is what the page reads.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobState {
    /// Still going. `done` out of `total` websites have been checked.
    Running {
        done: usize,
        total: usize,
        step: String,
    },
    /// Finished. The numbers are the ones the import produced.
    Done { summary: serde_json::Value },
    /// Stopped, and this is why — in words, not a stack trace.
    Failed { reason: String },
}

/// One import, and a counter it keeps bumping as it works.
#[derive(Debug)]
pub struct Job {
    pub state: std::sync::Mutex<JobState>,
    /// When this job stopped running. `None` while it is still going.
    pub finished: std::sync::Mutex<Option<Instant>>,
    /// Bumped by the site checker as each website finishes. An atomic rather than a
    /// message, because a progress bar only ever wants the latest number and does
    /// not care about the ones it missed.
    pub done: Arc<AtomicUsize>,
    pub total: AtomicUsize,
    /// Somebody has asked this job to stop.
    ///
    /// Checked between items rather than mid-item: a picture half written is worse
    /// than one picture more than you wanted. An `Arc` so the work itself can hold
    /// the same flag — the website checking reads it without knowing what a job is.
    pub stop: Arc<std::sync::atomic::AtomicBool>,
}

impl Job {
    /// Ask this job to stop after the item it is on.
    pub fn ask_to_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Has somebody asked it to stop?
    pub fn stop_asked(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// The current state, with the live counter folded in.
    pub fn snapshot(&self) -> JobState {
        let state = self
            .state
            .lock()
            .expect("job state is never poisoned")
            .clone();
        match state {
            JobState::Running { step, .. } => JobState::Running {
                done: self.done.load(Ordering::Relaxed),
                total: self.total.load(Ordering::Relaxed),
                step,
            },
            other => other,
        }
    }

    /// Move the job to a new state.
    ///
    /// Anything that is not `Running` stamps the finishing time, which is what lets
    /// [`Jobs`] forget it later.
    pub fn set(&self, state: JobState) {
        let running = matches!(state, JobState::Running { .. });
        *self.state.lock().expect("job state is never poisoned") = state;
        if !running {
            *self.finished.lock().expect("never poisoned") = Some(Instant::now());
        }
    }

    /// Is this job still going?
    pub fn is_running(&self) -> bool {
        matches!(self.snapshot(), JobState::Running { .. })
    }
}

/// Every import this run has seen.
///
/// An `RwLock` rather than a `Mutex`: the page reads this far more often than an
/// import writes to it, and readers do not have to queue behind each other.
#[derive(Debug, Default)]
pub struct Jobs {
    next_id: AtomicU64,
    inner: std::sync::RwLock<HashMap<u64, Arc<Job>>>,
}

/// How long a finished job is kept so the page can read its result.
pub const KEEP_FINISHED_FOR: Duration = Duration::from_secs(3600);

/// The most jobs kept at once, however recent they are.
pub const KEEP_AT_MOST: usize = 50;

impl Jobs {
    /// Start a job, unless one is already running.
    ///
    /// The check and the start are **one operation**, under one lock. Doing them as
    /// two — look, then start — is a race: two requests arriving together both look,
    /// both see nothing running, and both start — and two at once means twice as many
    /// requests to the same small businesses, which is exactly what the politeness
    /// rules are there to prevent.
    ///
    /// `Err` carries the job that was already going — the caller wanted an import,
    /// there is one, and that is the one to watch.
    pub fn start_exclusive(
        &self,
        total: usize,
        step: &str,
    ) -> std::result::Result<(u64, Arc<Job>), (u64, Arc<Job>)> {
        let mut jobs = self.inner.write().expect("jobs are never poisoned");

        if let Some((id, job)) = jobs
            .iter()
            .find(|(_, job)| job.is_running())
            .map(|(id, job)| (*id, Arc::clone(job)))
        {
            return Err((id, job));
        }

        let (id, job) = self.make(total, step);
        jobs.insert(id, Arc::clone(&job));
        forget_old(&mut jobs);
        Ok((id, job))
    }

    /// Build a job without putting it anywhere. Called while the lock is held.
    fn make(&self, total: usize, step: &str) -> (u64, Arc<Job>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let job = Arc::new(Job {
            state: std::sync::Mutex::new(JobState::Running {
                done: 0,
                total,
                step: step.to_owned(),
            }),
            finished: std::sync::Mutex::new(None),
            done: Arc::new(AtomicUsize::new(0)),
            stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            total: AtomicUsize::new(total),
        });
        (id, job)
    }

    /// Look one up. `None` when the number is not one we handed out.
    pub fn get(&self, id: u64) -> Option<Arc<Job>> {
        self.inner
            .read()
            .expect("jobs are never poisoned")
            .get(&id)
            .cloned()
    }
}

/// The heading a column of search words is written under: `words_hu`, `words_en`.
const WORDS_PREFIX: &str = "words_";

/// One row of the trades file, as written on disk.
///
/// Read into a map of heading to cell rather than into named fields, because the
/// languages are the file's business and not the program's: adding a `words_ar`
/// column should be an edit to a spreadsheet, not to this struct.
type TargetRow = std::collections::BTreeMap<String, String>;

/// Turn one row of the file into a trade, or `None` if the row is unusable.
///
/// Pure: no file, no logging, no surprises. Everything that decides what a trade *is*
/// lives here, which is what lets it be tested without a CSV on disk.
fn parse_target_row(row: &TargetRow) -> Option<Target> {
    let cell = |name: &str| row.get(name).map(|v| v.trim()).unwrap_or_default();
    let tier = Tier::parse(cell("tier"))?;
    let english = cell("english").to_owned();
    if english.is_empty() {
        return None;
    }

    // Every `words_*` column is a language. Several words to a cell, separated by
    // semicolons, because *borbély* and *barbershop* find different shops.
    let words: std::collections::BTreeMap<String, Vec<String>> = row
        .iter()
        .filter_map(|(heading, cell)| {
            let lang = heading
                .trim()
                .strip_prefix(WORDS_PREFIX)?
                .trim()
                .to_lowercase();
            if lang.is_empty() {
                return None;
            }
            let said: Vec<String> = cell
                .split(';')
                .map(|word| word.trim().to_owned())
                .filter(|word| !word.is_empty())
                .collect();
            // An empty cell means "not searched in this language", which is a real
            // answer. It is left out rather than kept as an empty list so that
            // "which languages does this trade have" needs no second thought.
            (!said.is_empty()).then_some((lang, said))
        })
        .collect();

    // A trade with no word in any language cannot be searched for at all.
    if words.is_empty() {
        return None;
    }

    Some(Target {
        rank: cell("rank").parse().unwrap_or(u8::MAX),
        tier,
        english,
        words,
        no_site_odds: cell("no_site_odds").parse().unwrap_or(0),
        budget: cell("budget").parse().unwrap_or(0),
        note: cell("note").to_owned(),
    })
}

/// The headings of the trades file, for a given set of languages.
///
/// Pure, so the shape of the file we write is a test rather than something you find
/// out by reading it back. The order is the order a person edits in: what it is, what
/// it is worth, then the notes.
pub fn target_headings(languages: &[String]) -> Vec<String> {
    let mut headings = vec!["rank".to_owned(), "tier".to_owned(), "english".to_owned()];
    headings.extend(languages.iter().map(|lang| format!("{WORDS_PREFIX}{lang}")));
    headings.extend(["no_site_odds", "budget", "note"].map(str::to_owned));
    headings
}

/// Every language any of these trades has words for, in a settled order.
///
/// The file gets a column for each. A language that has quietly lost its last word
/// still gets its column, because dropping one would lose the fact that it was
/// there — so the caller passes in what it wants kept.
pub fn languages_of(targets: &[Target]) -> Vec<String> {
    let mut found: Vec<String> = targets
        .iter()
        .flat_map(|t| t.words.keys().cloned())
        .collect();
    found.sort_unstable();
    found.dedup();
    // In the order they matter rather than alphabetically: the columns of a
    // hand-edited file should not reshuffle themselves because it was saved.
    webleads_domain::languages_in_order(&found)
}

/// One trade as a row of the file.
///
/// Pure. Several words to a cell, separated by semicolons, which is how the reader
/// expects them — the two are each other's inverse and a test says so.
pub fn target_row(target: &Target, languages: &[String]) -> Vec<String> {
    let mut cells = vec![
        target.rank.to_string(),
        format!("{:?}", target.tier),
        target.english.clone(),
    ];
    cells.extend(languages.iter().map(|lang| target.words(lang).join(";")));
    cells.push(target.no_site_odds.to_string());
    cells.push(target.budget.to_string());
    cells.push(target.note.clone());
    cells
}

/// Write the trades back to their file.
///
/// Written to a file beside it and then moved into place, so a failure halfway
/// through leaves the old list intact rather than half of a new one. This file is the
/// only thing in `data/` that is kept in the repository, and it is hand-edited as
/// well as edited on the page — losing it to a half-write would be losing work.
pub fn write_targets(path: &Path, targets: &[Target]) -> anyhow::Result<()> {
    let languages = languages_of(targets);
    let beside = path.with_extension("csv.writing");

    let mut writer = csv::Writer::from_path(&beside)?;
    writer.write_record(target_headings(&languages))?;
    for target in targets {
        writer.write_record(target_row(target, &languages))?;
    }
    writer.flush()?;
    drop(writer);

    std::fs::rename(&beside, path)?;
    tracing::info!(count = targets.len(), path = %path.display(), "trades written");
    Ok(())
}

/// Read the trades file.
///
/// A malformed row is skipped with a warning rather than stopping the server — the
/// list is a convenience, and refusing to start because row 61 has a stray comma
/// would be the wrong trade-off.
pub fn read_targets(path: &Path) -> anyhow::Result<Vec<Target>> {
    let mut reader = csv::Reader::from_path(path)?;
    let mut targets = Vec::new();

    for row in reader.deserialize::<TargetRow>() {
        let row = match row {
            Ok(row) => row,
            Err(err) => {
                tracing::warn!("⚠️ WARNING: skipping a row of TARGETS.csv: {err}");
                continue;
            }
        };
        let english = row.get("english").cloned().unwrap_or_default();
        match parse_target_row(&row) {
            Some(target) => targets.push(target),
            None => tracing::warn!("⚠️ WARNING: `{english}` has no usable tier or words, skipping"),
        }
    }

    targets.sort_by_key(|t| t.rank);
    Ok(targets)
}

/// Forget jobs nobody is going to ask about again.
///
/// Two rules, because either alone leaves a hole: an hour after finishing so a long
/// quiet day does not accumulate, and a hard ceiling so a busy hour cannot either.
/// A running job is never forgotten, whatever the count.
fn forget_old(jobs: &mut HashMap<u64, Arc<Job>>) {
    jobs.retain(
        |_, job| match *job.finished.lock().expect("never poisoned") {
            None => true, // still going
            Some(when) => when.elapsed() < KEEP_FINISHED_FOR,
        },
    );

    if jobs.len() <= KEEP_AT_MOST {
        return;
    }
    // Oldest numbers first — they were handed out in order.
    let mut ids: Vec<u64> = jobs
        .iter()
        .filter(|(_, job)| !job.is_running())
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    let too_many = jobs.len() - KEEP_AT_MOST;
    for id in ids.into_iter().take(too_many) {
        jobs.remove(&id);
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
