//! Remembers every business we have seen, and what we decided about it.
//!
//! SQLite, through `sqlx`. One file on disk, no server to run, and it survives a
//! container relaunch because it lives in the project folder.
//!
//! # A deliberate choice about queries
//!
//! `sqlx` offers `query!`, a macro that checks your SQL against a real database while
//! the program is being compiled — a genuinely lovely thing. We use the plain `query`
//! function instead, because the macro needs a live database to exist at *build* time.
//! That would mean a fresh clone of this repository could not be compiled until
//! somebody had first created a database, and it would mean the container could not
//! build without one. The cost is that a typo in SQL shows up when a test runs rather
//! than when the compiler runs — which the tests at the bottom of this file cover.
//!
//! # What this crate is here to teach
//!
//! Connection pools, migrations as numbered files, transactions, and mapping rows to
//! types by hand so it is clear what is happening.

#![forbid(unsafe_code)]

pub mod areas;
pub mod cities;
pub mod contacts;
pub mod filter;
pub mod gather;
pub mod hidden;
pub mod leads;
pub mod merge;
pub mod opening;
pub mod phones;

pub use merge::Merged;
pub use runs::{ForgetCost, Run, RunState, Word, WordState};
pub mod record;
pub mod runs;
pub mod settings;
pub mod tally;
pub mod verdicts;

pub use areas::{InCity, SavedArea};
pub use cities::{Remeasured, SavedCity};
pub use contacts::Due;
pub use filter::Wanted;
pub use gather::Gathered;
pub use hidden::Hidden;
pub use leads::{Extremes, Pin, StoredLead};
pub use tally::Tally;
pub use verdicts::Suspect;

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use webleads_domain::Lead;

/// Everything that can go wrong talking to the database.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the database could not be opened or queried")]
    Database(#[from] sqlx::Error),

    #[error("the database schema could not be applied")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("the database holds a value we do not understand")]
    Corrupt(#[from] webleads_domain::UnknownVerdict),

    #[error("the database holds a status we do not understand")]
    BadStatus(#[from] webleads_domain::UnknownStatus),

    #[error("the database could not be backed up: {0}")]
    Backup(String),

    /// Asked for something we will not do — not a failure, a refusal.
    #[error("{0}")]
    Refused(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One business and where it is, kept together.
///
/// One struct rather than three lists passed side by side — leads, latitudes,
/// longitudes — which would have to trust each other to stay the same length and in
/// the same order.
#[derive(Debug, Clone)]
pub struct Found {
    pub lead: Lead,
    pub lat: f64,
    pub lon: f64,
    /// Did we actually fetch this website on this run, or reuse what we already knew?
    ///
    /// Only a real fetch moves the freshness date. Without this a cached verdict
    /// would keep marking itself fresh and the site would never be looked at again.
    pub freshly_checked: bool,
    /// The search phrase that turned this business up, when it came from a scrape
    /// rather than a file. Kept so a surprising result can be traced to the word
    /// that produced it.
    pub found_by: Option<String>,
    /// Their week as Google sent it, kept unparsed.
    pub open_hours: Option<String>,
}

/// What a write actually did, counted by the database rather than inferred.
#[derive(Debug, Default)]
pub struct Recorded {
    /// Businesses we had never met before.
    pub new: usize,
    /// Businesses we already knew about.
    pub seen_before: usize,
    /// Ids met earlier in this same batch. A file that lists one shop twice must
    /// not count it as two new businesses — which is exactly what the old
    /// before-and-after subtraction got wrong.
    seen_this_batch: std::collections::HashSet<String>,
}

/// A handle to the database.
///
/// It wraps a *pool* rather than a single connection. A pool hands out connections to
/// whoever needs one and takes them back afterwards, which is what lets several web
/// requests read at once without each one paying to open the file again.
#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Open the database at `path`, creating it and applying the schema if needed.
    pub async fn open(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            // Write-ahead logging lets readers carry on while a write is happening,
            // which matters the moment the web page polls while an import runs.
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            // Without this, SQLite ignores REFERENCES clauses entirely.
            .foreign_keys(true)
            // SQLite allows one writer at a time, and by default a second one gives
            // up *instantly* with "database is locked" rather than waiting. With a
            // pool of connections that is not a rare event — two imports one after
            // the other was enough to trigger it. Five seconds is far longer than any
            // of our writes take, so a real deadlock would still surface rather than
            // hang forever.
            .busy_timeout(Duration::from_secs(5));

        // Before anything touches it. One bad migration and every `first_seen` date
        // ever collected is gone, and those cannot be recovered by re-scraping.
        if let Err(err) = back_up(path).await {
            // A failed backup must not stop the program starting — but it must be
            // loud, because running without one is a risk you should know you are
            // taking.
            tracing::warn!("⚠️ WARNING: could not back the database up: {err}");
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await?;

        Self::from_pool(pool).await
    }

    /// Open a database that lives only in memory. Used by the tests.
    pub async fn in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1) // an in-memory database vanishes with its connection
            .connect("sqlite::memory:")
            .await?;
        Self::from_pool(pool).await
    }

    async fn from_pool(pool: SqlitePool) -> Result<Self> {
        // `migrate!` reads the `migrations/` folder at compile time and embeds those
        // files in the binary, so a deployed program carries its own schema. It runs
        // only the files this database has not seen yet.
        sqlx::migrate!("../../migrations").run(&pool).await?;
        let store = Self { pool };
        // Anything collected before the sortable columns existed has none of them,
        // and would sort as "we do not know" for ever otherwise.
        store.rebuild_opening_times().await?;
        // Numbers written before they were tidied still hold Google's own spacing.
        store.tidy_phones().await?;
        Ok(store)
    }
}

/// Today in Budapest, as `2026-09-19`.
///
/// Dates, not timestamps: we compare runs by day, and "did we see this business
/// before today" is the question `first_seen` exists to answer.
///
/// **Budapest, not UTC.** Hungary is an hour or two ahead, so an import run at one in
/// the morning would be stamped with yesterday if we used UTC — which would quietly
/// corrupt the one column the whole "new since last run" idea rests on.
///
/// The timezone data is bundled into the binary rather than read from the machine, so
/// a changed container image cannot move every date we have already stored.
pub(crate) fn today() -> String {
    webleads_domain::today_here_date()
}

/// How many days of backups to keep.
const KEEP_BACKUPS: usize = 7;

/// Copy the database aside before opening it.
///
/// One a day, not one per restart: restarting the server five times while working
/// should not push a week of history out of the folder.
///
/// Older ones are moved to `trash`, never deleted outright — this project's one
/// unbreakable rule, and doubly right for the only copy of something.
async fn back_up(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Ok(()); // nothing to lose yet
    }
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let folder = parent.join("backups");
    std::fs::create_dir_all(&folder).map_err(|e| Error::Backup(e.to_string()))?;

    let today = today();
    let destination = folder.join(format!("webleads-{today}.db"));
    if destination.exists() {
        return Ok(()); // already done today
    }

    // `VACUUM INTO` rather than copying the file.
    //
    // We run in write-ahead logging mode, which means recent writes live in a
    // separate `-wal` file next to the database. Copying only the `.db` gave a
    // backup with **no tables in it at all** — 4 KB of nothing, sitting there looking
    // like insurance. This asks SQLite to write a complete, consistent copy instead,
    // which is the one way to be sure everything is in it.
    let source = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(path).read_only(true))
        .await?;

    // The path goes in by hand because `VACUUM INTO` will not take a bound parameter.
    // It is a path we built ourselves from a date, so there is nothing to smuggle in.
    let escaped = destination.display().to_string().replace('\'', "''");
    let result = sqlx::query(&format!("VACUUM INTO '{escaped}'"))
        .execute(&source)
        .await;
    source.close().await;
    result?;

    tracing::info!(path = %destination.display(), "database backed up");
    prune_backups(&folder).map_err(|e| Error::Backup(e.to_string()))?;
    Ok(())
}

/// Keep the newest [`KEEP_BACKUPS`] and trash the rest.
fn prune_backups(folder: &Path) -> std::io::Result<()> {
    prune_old_files(folder, "webleads-", KEEP_BACKUPS)
}

/// Keep the newest `keep` files whose name contains `marker`, and trash the rest.
///
/// Shared by the backups and the logs. The log files piled up one a day forever
/// while this sat here doing the same job for the backups — which is the whole
/// reason it is now public rather than private to this file.
///
/// Both kinds of file carry the date in their name, so sorting by name sorts by age.
///
/// Old files go to `trash`, never `rm` — this project's one unbreakable rule, and
/// doubly right for something that might be the only copy.
pub fn prune_old_files(folder: &Path, marker: &str, keep: usize) -> std::io::Result<()> {
    if !folder.is_dir() {
        return Ok(());
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains(marker))
        })
        .collect();
    files.sort();

    while files.len() > keep {
        let oldest = files.remove(0);
        match std::process::Command::new("trash").arg(&oldest).status() {
            Ok(status) if status.success() => {
                tracing::info!(path = %oldest.display(), "old file trashed");
            }
            // If `trash` is not on this machine, leave the file alone and say so. An
            // undeleted log costs a few megabytes; a deleted backup could cost
            // everything, and this code does both jobs.
            _ => tracing::warn!(
                "⚠️ WARNING: could not trash {} — leaving it alone",
                oldest.display()
            ),
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod testing;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::Wanted;
    use crate::testing::*;
    use webleads_domain::Verdict;

    #[tokio::test]
    async fn the_schema_applies_to_an_empty_database() {
        let store = Store::in_memory().await.expect("opens");
        assert_eq!(store.business_count().await.expect("counts"), 0);
        assert!(store
            .leads(&Wanted {
                limit: 100,
                offset: 0,
                ..Default::default()
            })
            .await
            .expect("reads")
            .is_empty());
    }

    #[test]
    fn today_is_a_plain_date_with_no_time_on_the_end() {
        let date = today();
        assert_eq!(date.len(), 10, "got {date}");
        let parts: Vec<&str> = date.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].len(), 4);
        assert_eq!(parts[1].len(), 2);
        assert_eq!(parts[2].len(), 2);
        assert!(date.chars().all(|c| c.is_ascii_digit() || c == '-'));
        // A timestamp here would break the "same day counts as the same run" rule.
        assert!(!date.contains('T'));
        assert!(!date.contains(':'));
    }

    #[test]
    fn two_calls_on_the_same_day_give_the_same_answer() {
        assert_eq!(today(), today());
    }

    #[test]
    fn each_kind_of_database_failure_says_which_thing_broke() {
        let corrupt = Error::Corrupt(webleads_domain::UnknownVerdict("banana".into()));
        assert!(corrupt.to_string().contains("value"));

        // And the value itself survives underneath, for the log.
        let cause = std::error::Error::source(&corrupt).expect("has a cause");
        assert!(cause.to_string().contains("banana"));
    }

    #[test]
    fn a_verdict_the_database_does_not_know_is_refused_not_guessed() {
        // Defaulting an unknown word to NoSite would quietly put a business with a
        // working website onto the call list.
        let error: std::result::Result<Verdict, _> = "banana".parse();
        assert!(error.is_err());
        let wrapped: Error = error.unwrap_err().into();
        assert!(matches!(wrapped, Error::Corrupt(_)));
    }

    #[test]
    fn a_stored_date_is_the_budapest_one_not_the_utc_one() {
        // Whatever the clock says right now, the two must agree on format, and on any
        // day the program is run they may legitimately differ by one.
        let budapest = today();
        let utc_now = time::OffsetDateTime::now_utc().date().to_string();
        assert_eq!(budapest.len(), utc_now.len());
        assert_eq!(
            budapest,
            webleads_domain::date_here(time::OffsetDateTime::now_utc())
        );
    }

    #[tokio::test]
    async fn a_backup_actually_contains_the_data() {
        // The whole point, and the thing that was wrong first time: copying the file
        // gave a 4 KB backup with no tables in it, because everything recent was
        // sitting in the write-ahead log beside it.
        let dir = std::env::temp_dir().join("webleads-backup-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("t.db");

        {
            let store = Store::open(&path).await.expect("opens");
            let run = store.begin_run("seed.csv").await.expect("run");
            store
                .record(run, &[found(a_lead("a", 0, 10)), found(a_lead("b", 1, 20))])
                .await
                .expect("records");
        }

        // Opening again on a later day is what triggers a backup; force one by
        // removing today's and opening once more.
        let backup = dir.join("backups").join(format!("webleads-{}.db", today()));
        let _ = std::fs::remove_file(&backup);
        {
            let _store = Store::open(&path).await.expect("opens again");
        }

        assert!(backup.is_file(), "no backup was written");
        let restored = Store::open(&backup).await.expect("the backup opens");
        assert_eq!(
            restored.business_count().await.expect("counts"),
            2,
            "the backup is missing the businesses"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_second_start_on_the_same_day_does_not_make_a_second_backup() {
        // Restarting the server five times while working should not push a week of
        // history out of the folder.
        let dir = std::env::temp_dir().join("webleads-backup-once");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("t.db");

        for _ in 0..3 {
            let _store = Store::open(&path).await.expect("opens");
        }

        let folder = dir.join("backups");
        if folder.is_dir() {
            let count = std::fs::read_dir(&folder).expect("reads").count();
            assert!(count <= 1, "made {count} backups in one day");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn backing_up_a_database_that_does_not_exist_yet_is_not_a_failure() {
        let dir = std::env::temp_dir().join("webleads-backup-none");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        // A brand new install has nothing to lose, and must not refuse to start.
        assert!(back_up(&dir.join("nothing-here.db")).await.is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
