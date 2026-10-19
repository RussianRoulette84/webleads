//! A scrape, and the words it still has to work through.
//!
//! Kept in the database rather than in memory on purpose. A run takes an hour and a
//! half; a restart in the middle should carry on from the word it reached rather than
//! throwing the lot away.

use sqlx::Row;

use crate::{today, Result, Store};

/// Where a run has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// Lined up, not started. Its words are already written down, so you can see
    /// exactly what it will search before it does.
    Queued,
    /// Still working through its words.
    Working,
    /// You pressed pause. It finished the word it was on and stopped there.
    ///
    /// A state in the database rather than a flag in memory, so a restart cannot
    /// quietly turn a paused search into an abandoned one. It is **not** picked up
    /// again by itself when the server comes back: waiting for you is what held
    /// means.
    Held,
    /// Every word done.
    Done,
    /// You pressed stop. Everything found is kept.
    Stopped,
    /// Something went wrong that ended the run.
    Failed,
}

impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Working => "working",
            Self::Held => "held",
            Self::Done => "done",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }

    /// Is there anything left to do?
    ///
    /// Held counts. There are words still to search, and the rule that refuses to
    /// start a second search while one is under way has to see it — or continuing a
    /// paused search would find its place taken by another.
    pub fn still_going(self) -> bool {
        matches!(self, Self::Working | Self::Held)
    }

    /// Is it moving right now?
    ///
    /// Apart from [`RunState::still_going`] on purpose: what draws a progress bar and
    /// what blocks a second search are two different questions, and a paused search
    /// answers them differently.
    pub fn working_now(self) -> bool {
        self == Self::Working
    }

    /// Is it over?
    ///
    /// The third of these on purpose, and not the opposite of either: a queued run is
    /// neither going nor ended. This one decides whether the moment it ended gets
    /// written down, so `Held` must stay out of it — a paused search you mean to
    /// carry on has not finished, however much it looks stopped.
    pub fn ends_the_run(self) -> bool {
        matches!(self, Self::Done | Self::Stopped | Self::Failed)
    }
}

impl std::str::FromStr for RunState {
    type Err = crate::Error;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        match text {
            "queued" => Ok(Self::Queued),
            "working" => Ok(Self::Working),
            "held" => Ok(Self::Held),
            "done" => Ok(Self::Done),
            "stopped" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            other => Err(crate::Error::Backup(format!(
                "`{other}` is not a run state"
            ))),
        }
    }
}

/// Where one search word has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WordState {
    Waiting,
    Searching,
    Done,
    Failed,
}

impl WordState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Searching => "searching",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

impl std::str::FromStr for WordState {
    type Err = crate::Error;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        match text {
            "waiting" => Ok(Self::Waiting),
            "searching" => Ok(Self::Searching),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            other => Err(crate::Error::Backup(format!(
                "`{other}` is not a word state"
            ))),
        }
    }
}

/// One search word, and what became of it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Word {
    pub id: i64,
    pub position: i64,
    /// What we search Google for: "borbély Budapest".
    pub phrase: String,
    /// The trade it came from — where the Trade column gets its answer.
    pub trade: Option<String>,
    pub state: WordState,
    /// The scraper's own id, so a restart can pick the job back up rather than
    /// searching the same word twice.
    pub remote_id: Option<String>,
    /// How many businesses came back. **Nought is the interesting number** — it
    /// usually means the Hungarian is wrong, which is what the hit report is for.
    pub found: Option<i64>,
    pub note: Option<String>,
}

/// What forgetting a search would take with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ForgetCost {
    /// How many search words its record holds.
    pub words: i64,
    /// Businesses that would leave the call list, because no other search has ever
    /// seen them and there is nothing to fall back on.
    pub businesses_lost: i64,
    /// Businesses that would stay, wearing the verdict an earlier search gave them.
    pub businesses_kept: i64,
}

/// A run, as the page sees it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Run {
    pub id: i64,
    pub state: RunState,
    pub source: String,
    pub started_at: String,
    /// When it ended, for a search that has. `None` while it is still going, and
    /// `None` for the searches made before anything wrote this down.
    pub finished_at: Option<String>,
    pub note: Option<String>,
    pub words_total: i64,
    pub words_done: i64,
    pub found_total: i64,
    /// The word being searched right this moment, if one is.
    ///
    /// Without it the page reads "0 of 27" for two and a half minutes and looks
    /// stuck — the counter only moves when a word *finishes*.
    pub now_searching: Option<String>,
    /// Where this search was looking. `None` for anything recorded before areas
    /// existed.
    pub area: Option<String>,
    /// Which city it was looking in. `None` for a search made before cities existed,
    /// or one whose area had none named.
    pub city: Option<i64>,
    /// How many businesses on the call list this search was the first to find.
    ///
    /// Different from `found_total`, and both are shown: a search finds everything
    /// Google returns, while the call list keeps only what is worth ringing.
    pub leads_found: i64,
}

impl Store {
    /// Start a scrape now, with its words queued in order.
    pub async fn begin_scrape(
        &self,
        area: &webleads_domain::Area,
        words: &[(String, Option<String>)],
        options: &webleads_domain::ScrapeOptions,
    ) -> Result<i64> {
        self.write_scrape(area, words, RunState::Working, options)
            .await
    }

    /// Line a scrape up to run when the ones before it have finished.
    pub async fn queue_scrape(
        &self,
        area: &webleads_domain::Area,
        words: &[(String, Option<String>)],
        options: &webleads_domain::ScrapeOptions,
    ) -> Result<i64> {
        self.write_scrape(area, words, RunState::Queued, options)
            .await
    }

    /// One write for both, so a queued run and a started one are the same thing in
    /// every way except when they begin.
    async fn write_scrape(
        &self,
        area: &webleads_domain::Area,
        words: &[(String, Option<String>)],
        state: RunState,
        options: &webleads_domain::ScrapeOptions,
    ) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        let now = today();

        let done = sqlx::query(
            // `area_radius_m` is left empty: an area has no fence.
            "INSERT INTO run (source, started_at, state, kind,
                              area_label, area_lat, area_lon,
                              options_json, city_id)
             VALUES (?1, ?2, ?3, 'scrape', ?4, ?5, ?6, ?7,
                     (SELECT city_id FROM area WHERE label = ?4))",
        )
        .bind(&area.label)
        .bind(&now)
        .bind(state.as_str())
        .bind(&area.label)
        .bind(area.lat)
        .bind(area.lon)
        // What the search was actually asked to do, so a queued one cannot lose it.
        .bind(serde_json::to_string(options).ok())
        .execute(&mut *tx)
        .await?;
        let run_id = done.last_insert_rowid();

        for (position, (phrase, trade)) in words.iter().enumerate() {
            sqlx::query(
                "INSERT INTO run_word (run_id, position, phrase, trade) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(run_id)
            .bind(position as i64)
            .bind(phrase)
            .bind(trade)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(run_id)
    }

    /// The area a run was searching, as it was written down when the run was made.
    ///
    /// This is what makes a run resumed after a restart exact rather than
    /// approximate: before it existed, a resumed run silently used the defaults.
    pub async fn run_area(&self, run_id: i64) -> Result<Option<webleads_domain::Area>> {
        let row = sqlx::query(
            "SELECT area_label, area_lat, area_lon, area_radius_m FROM run WHERE id = ?1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|row| {
            let label: Option<String> = row.get("area_label");
            let lat: Option<f64> = row.get("area_lat");
            let lon: Option<f64> = row.get("area_lon");
            Some(webleads_domain::Area {
                label: label?,
                address: String::new(),
                lat: lat?,
                lon: lon?,
                max_results: 200,
            })
        }))
    }

    /// The settings a search was asked to use.
    ///
    /// `None` for anything recorded before they were kept, which then falls back to
    /// the defaults — the behaviour everything had before.
    pub async fn run_options(&self, run_id: i64) -> Result<Option<webleads_domain::ScrapeOptions>> {
        let row = sqlx::query("SELECT options_json FROM run WHERE id = ?1")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row
            .and_then(|row| row.get::<Option<String>, _>("options_json"))
            .and_then(|json| serde_json::from_str(&json).ok()))
    }

    /// Everything lined up and not yet started, oldest first.
    pub async fn queued(&self) -> Result<Vec<Run>> {
        let rows = sqlx::query("SELECT id FROM run WHERE state = 'queued' ORDER BY id")
            .fetch_all(&self.pool)
            .await?;

        let mut waiting = Vec::new();
        for row in rows {
            if let Some(run) = self.run(row.get::<i64, _>("id")).await? {
                waiting.push(run);
            }
        }
        Ok(waiting)
    }

    /// Take the next queued search and mark it started, in one step.
    ///
    /// One operation, like the import lock: looking first and starting afterwards is
    /// how two of them start at once.
    pub async fn start_next_queued(&self) -> Result<Option<Run>> {
        let mut tx = self.pool.begin().await?;

        // Not while something is already going. One search at a time, whatever route
        // asked for it.
        // A paused search still holds the floor: its words are half done and
        // continuing it must not find another search already going.
        let busy: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM run WHERE state IN ('working', 'held') AND kind = 'scrape' LIMIT 1",
        )
        .fetch_optional(&mut *tx)
        .await?;
        if busy.is_some() {
            return Ok(None);
        }

        let next: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM run WHERE state = 'queued' ORDER BY id LIMIT 1")
                .fetch_optional(&mut *tx)
                .await?;
        let Some((run_id,)) = next else {
            return Ok(None);
        };

        sqlx::query("UPDATE run SET state = 'working', started_at = ?2 WHERE id = ?1")
            .bind(run_id)
            .bind(today())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        self.run(run_id).await
    }

    /// Take one out of the queue. Its words go with it.
    ///
    /// Refuses anything already running: stopping a search that is going is what the
    /// stop button is for, and quietly deleting it would lose what it had found.
    pub async fn forget_queued(&self, run_id: i64) -> Result<bool> {
        let done = sqlx::query("DELETE FROM run WHERE id = ?1 AND state = 'queued'")
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// What forgetting a search would cost, before anybody agrees to it.
    ///
    /// Asked first and shown, because the honest answer is sometimes "three hundred
    /// businesses leave your call list" and that must never be a surprise.
    pub async fn forget_cost(&self, run_id: i64) -> Result<Option<ForgetCost>> {
        if self.run(run_id).await?.is_none() {
            return Ok(None);
        }
        let row = sqlx::query(
            "SELECT
               (SELECT COUNT(*) FROM run_word WHERE run_id = ?1) AS words,
               -- Businesses this search is the only one ever to have seen. Nothing
               -- is left to fall back on, so these really do go.
               (SELECT COUNT(*) FROM business b
                 WHERE EXISTS (SELECT 1 FROM lead WHERE business_id = b.id AND run_id = ?1)
                   AND NOT EXISTS (SELECT 1 FROM lead
                                    WHERE business_id = b.id AND run_id <> ?1)) AS lost,
               -- Businesses an earlier search also saw. They stay, wearing whatever
               -- that earlier search decided about them.
               (SELECT COUNT(*) FROM business b
                 WHERE EXISTS (SELECT 1 FROM lead WHERE business_id = b.id AND run_id = ?1)
                   AND EXISTS (SELECT 1 FROM lead
                                WHERE business_id = b.id AND run_id <> ?1)) AS kept",
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(Some(ForgetCost {
            words: row.get("words"),
            businesses_lost: row.get("lost"),
            businesses_kept: row.get("kept"),
        }))
    }

    /// Forget a search that is over.
    ///
    /// Its words go with it, and so do the verdicts it recorded — the database
    /// cascades both. That is the dangerous part and the reason for
    /// [`Store::forget_cost`]: a business whose only verdict came from this search
    /// leaves the call list with it.
    ///
    /// A business an **earlier** search also saw is put back the way that search left
    /// it. Without this it would keep its row and lose its current verdict, which
    /// makes it invisible rather than out of date — a business you can no longer see
    /// and can no longer be told about is worse than one wearing an old judgement.
    ///
    /// Refuses anything still going, the same way [`Store::forget_queued`] refuses
    /// anything that has started: deleting a search from under the scraper would leave
    /// it writing words into a run that is not there.
    ///
    /// `Ok(false)` means there was no such search. Being asked to forget something
    /// already forgotten is not a failure.
    pub async fn delete_run(&self, run_id: i64) -> Result<bool> {
        let Some(run) = self.run(run_id).await? else {
            return Ok(false);
        };
        if run.state.still_going() {
            return Err(crate::Error::Refused(
                "that search is still going — stop it first, then forget it".into(),
            ));
        }

        let mut tx = self.pool.begin().await?;
        // Nothing may point at a search that is gone.
        sqlx::query("UPDATE business SET first_run_id = NULL WHERE first_run_id = ?1")
            .bind(run_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM run WHERE id = ?1")
            .bind(run_id)
            .execute(&mut *tx)
            .await?;
        // Give back the newest verdict still standing to anything left without one.
        // Runs are numbered in the order they happened, so the highest id left is the
        // most recent thing we knew about that business.
        sqlx::query(
            "UPDATE lead SET is_latest = 1
              WHERE (business_id, run_id) IN (
                    SELECT business_id, MAX(run_id) FROM lead
                     WHERE business_id NOT IN (SELECT business_id FROM lead WHERE is_latest = 1)
                     GROUP BY business_id)",
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// The next word to search, if there is one.
    ///
    /// A word already `searching` comes back first: that is a run picked up after a
    /// restart, and its scraper job may well still be running. Searching it again
    /// would waste a minute and annoy Google for nothing.
    pub async fn next_word(&self, run_id: i64) -> Result<Option<Word>> {
        let row = sqlx::query(
            "SELECT id, position, phrase, trade, state, remote_id, found, note
             FROM run_word
             WHERE run_id = ?1 AND state IN ('searching', 'waiting')
             ORDER BY CASE state WHEN 'searching' THEN 0 ELSE 1 END, position
             LIMIT 1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_word).transpose()
    }

    /// Mark a word as being searched now, with the scraper's job id.
    pub async fn word_searching(&self, word_id: i64, remote_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE run_word SET state = 'searching', remote_id = ?2, started_at = ?3
             WHERE id = ?1",
        )
        .bind(word_id)
        .bind(remote_id)
        // A moment, not a date. These two are subtracted from each other to work out
        // how long a search takes, and a date minus a date is always nought — which
        // is why the page could never say how long a run would take.
        .bind(webleads_domain::moment_now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark a word finished, with how many businesses it returned.
    pub async fn word_done(&self, word_id: i64, found: i64) -> Result<()> {
        sqlx::query(
            "UPDATE run_word SET state = 'done', found = ?2, finished_at = ?3 WHERE id = ?1",
        )
        .bind(word_id)
        .bind(found)
        .bind(webleads_domain::moment_now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark a word failed, and why.
    ///
    /// One word failing does not end a run — the next word may be perfectly fine, and
    /// losing ninety minutes because one search timed out would be the wrong trade.
    pub async fn word_failed(&self, word_id: i64, why: &str) -> Result<()> {
        sqlx::query(
            "UPDATE run_word SET state = 'failed', note = ?2, finished_at = ?3 WHERE id = ?1",
        )
        .bind(word_id)
        .bind(why)
        .bind(webleads_domain::moment_now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Move a run to a new state.
    ///
    /// A state that ends the run also stamps the moment it ended, so a finished search
    /// reads as finished to anything reading the table, not only to the server.
    ///
    /// `CASE` rather than two queries: a run that is merely paused must keep the end
    /// it already has, and a run ending twice must not move its own end forward.
    pub async fn set_run_state(
        &self,
        run_id: i64,
        state: RunState,
        note: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE run
                SET state = ?2,
                    note  = ?3,
                    finished_at = CASE WHEN ?4 AND finished_at IS NULL
                                       THEN ?5 ELSE finished_at END
              WHERE id = ?1",
        )
        .bind(run_id)
        .bind(state.as_str())
        .bind(note)
        .bind(state.ends_the_run())
        .bind(webleads_domain::moment_now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Everything we know about one run.
    pub async fn run(&self, run_id: i64) -> Result<Option<Run>> {
        let row = sqlx::query(
            "SELECT r.id, r.state, r.source, r.started_at, r.finished_at, r.note,
                    (SELECT COUNT(*) FROM run_word WHERE run_id = r.id) AS words_total,
                    (SELECT COUNT(*) FROM run_word
                      WHERE run_id = r.id AND state IN ('done','failed')) AS words_done,
                    (SELECT COALESCE(SUM(found), 0) FROM run_word WHERE run_id = r.id) AS found_total,
                    (SELECT phrase FROM run_word
                      WHERE run_id = r.id AND state = 'searching'
                      ORDER BY position LIMIT 1) AS now_searching,
                    r.area_label,
                    r.city_id,
                    -- What this search was the first to find, and that is still worth
                    -- ringing. `first_run_id` is written once, so this number holds
                    -- even after a later search sees the same shops again.
                    (SELECT COUNT(*) FROM business b
                      WHERE b.first_run_id = r.id AND b.merged_into IS NULL
                        AND b.phone IS NOT NULL AND TRIM(b.phone) <> '') AS leads_found
             FROM run r WHERE r.id = ?1",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_run).transpose()
    }

    /// The searches made in one area.
    ///
    /// Looked up rather than trusted: the name comes from a request, and what leaves
    /// here is a list of our own ids.
    pub async fn runs_in_area(&self, area: &str) -> Result<Vec<i64>> {
        let rows = sqlx::query("SELECT id FROM run WHERE area_label = ?1")
            .bind(area)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| row.get::<i64, _>("id"))
            .collect())
    }

    /// The searches already done, newest first.
    ///
    /// Failed ones included on purpose: "it found nothing because the scraper was
    /// off" is the single most useful thing this list can tell you, and hiding it
    /// would leave you wondering where a search went.
    ///
    /// Imports are left out — this is the record of searching Google, not of reading
    /// files.
    pub async fn past_scrapes(&self, how_many: i64) -> Result<Vec<Run>> {
        let rows = sqlx::query(
            "SELECT id FROM run
             WHERE kind = 'scrape' AND state <> 'queued'
             ORDER BY id DESC LIMIT ?1",
        )
        .bind(how_many.clamp(1, 200))
        .fetch_all(&self.pool)
        .await?;

        let mut past = Vec::new();
        for row in rows {
            if let Some(run) = self.run(row.get::<i64, _>("id")).await? {
                past.push(run);
            }
        }
        Ok(past)
    }

    /// Put a running search on hold.
    ///
    /// Only one that is actually running: pausing something already finished would
    /// rewrite the answer. Returns whether anything changed, so the route can say
    /// "there was nothing to pause" rather than pretending.
    pub async fn hold_run(&self, run_id: i64) -> Result<bool> {
        let done = sqlx::query("UPDATE run SET state = 'held' WHERE id = ?1 AND state = 'working'")
            .bind(run_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(done > 0)
    }

    /// Take a held search off hold, ready to carry on.
    ///
    /// The words themselves are untouched: the one left mid-flight is still marked
    /// `searching`, and [`Store::next_word`] hands it back first so its remote job is
    /// reused rather than paid for a second time.
    pub async fn resume_run(&self, run_id: i64) -> Result<bool> {
        let done = sqlx::query("UPDATE run SET state = 'working' WHERE id = ?1 AND state = 'held'")
            .bind(run_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(done > 0)
    }

    /// The search that is on hold, if one is.
    ///
    /// For the page on load: a held search has nobody watching it, so the buttons
    /// have to be able to find it again.
    pub async fn held_scrape(&self) -> Result<Option<Run>> {
        let row = sqlx::query(
            "SELECT id FROM run WHERE kind = 'scrape' AND state = 'held'
             ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None => Ok(None),
            Some(row) => self.run(row.get::<i64, _>("id")).await,
        }
    }

    /// A scrape that was still working when we stopped, if there is one.
    ///
    /// **This is what makes a restart survivable.** Called on start-up: if a run was
    /// half done, it is picked up and carried on rather than quietly abandoned.
    pub async fn unfinished_scrape(&self) -> Result<Option<Run>> {
        let row = sqlx::query(
            "SELECT id FROM run WHERE kind = 'scrape' AND state = 'working'
             ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None => Ok(None),
            Some(row) => self.run(row.get::<i64, _>("id")).await,
        }
    }

    /// Every word of a run, in order. This is the hit report.
    pub async fn words(&self, run_id: i64) -> Result<Vec<Word>> {
        let rows = sqlx::query(
            "SELECT id, position, phrase, trade, state, remote_id, found, note
             FROM run_word WHERE run_id = ?1 ORDER BY position",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_word).collect()
    }

    /// How long the last finished scrape took per word, in seconds.
    ///
    /// `None` until one has finished — an estimate made up before we have measured
    /// anything is worse than saying "I do not know yet".
    pub async fn seconds_per_word(&self) -> Result<Option<f64>> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS words,
                    SUM(julianday(finished_at) - julianday(started_at)) * 86400.0 AS seconds
             FROM run_word
             WHERE state = 'done'
               -- Only words we actually timed. Anything recorded before the times
               -- carried a clock reads as nought seconds, and averaging those in
               -- would report a search as instant.
               AND finished_at > started_at",
        )
        .fetch_one(&self.pool)
        .await?;

        let words: i64 = row.get("words");
        let seconds: Option<f64> = row.get("seconds");
        match (words, seconds) {
            (0, _) | (_, None) => Ok(None),
            (words, Some(seconds)) if seconds > 0.0 => Ok(Some(seconds / words as f64)),
            _ => Ok(None),
        }
    }
}

fn row_to_word(row: sqlx::sqlite::SqliteRow) -> Result<Word> {
    Ok(Word {
        id: row.get("id"),
        position: row.get("position"),
        phrase: row.get("phrase"),
        trade: row.get("trade"),
        state: row.get::<String, _>("state").parse()?,
        remote_id: row.get("remote_id"),
        found: row.get("found"),
        note: row.get("note"),
    })
}

fn row_to_run(row: sqlx::sqlite::SqliteRow) -> Result<Run> {
    Ok(Run {
        id: row.get("id"),
        state: row.get::<String, _>("state").parse()?,
        source: row.get("source"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        note: row.get("note"),
        words_total: row.get("words_total"),
        words_done: row.get("words_done"),
        found_total: row.get("found_total"),
        now_searching: row.get("now_searching"),
        area: row.get("area_label"),
        city: row.get("city_id"),
        leads_found: row.get("leads_found"),
    })
}

#[cfg(test)]
#[path = "runs_tests.rs"]
mod tests;
