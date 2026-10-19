//! How the calling is going: today's work, and the work of every day.
//!
//! # What this file is here to teach
//!
//! Two questions that look like one. "How many did I ring today" and "how many have
//! I ever rung" are the same `GROUP BY` with and without a date, so they are one
//! file and one pass of thought rather than six separate counters kept up to date by
//! hand.
//!
//! It also carries the moment anything last changed, which is what lets a second
//! device — your phone, while the desktop is open — notice that it is out of date
//! without asking for the whole list every few seconds.

use serde::Serialize;
use sqlx::Row;
use webleads_domain::Status;

use crate::{Result, Store};

/// The state of the calling, counted twice.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Tally {
    /// Each state and how many businesses were put into it today.
    pub today: Vec<(Status, i64)>,
    /// Each state and how many businesses are in it altogether.
    pub ever: Vec<(Status, i64)>,
    /// How many different businesses you telephoned today.
    pub called_today: i64,
    /// And how many you have telephoned since the beginning.
    pub called_ever: i64,
    /// When anything was last marked, as `2026-10-13 14:05`, or empty if nothing
    /// ever was. A second screen compares this with what it last saw; when it has
    /// moved, something changed somewhere else and the rows are stale.
    pub changed_at: String,
}

impl Store {
    /// Today's calling and all of it, in one round trip's worth of thinking.
    ///
    /// Deliberately not narrowed by the filter on the screen: a day's work is a
    /// day's work whichever city you happen to be looking at.
    pub async fn tally(&self) -> Result<Tally> {
        let today = crate::today();
        Ok(Tally {
            today: self.states_on(Some(&today)).await?,
            ever: self.states_on(None).await?,
            called_today: self.calls_on(Some(&today)).await?,
            called_ever: self.calls_on(None).await?,
            changed_at: self.last_marked().await?,
        })
    }

    /// Each state and its count, either for one day or for all of them.
    ///
    /// A merged-away business is somebody else's row now and counting it would
    /// double every number it touches.
    async fn states_on(&self, day: Option<&str>) -> Result<Vec<(Status, i64)>> {
        let rows = sqlx::query(
            "SELECT c.status AS status, COUNT(*) AS n
             FROM contact c
             JOIN business b ON b.id = c.business_id
             WHERE b.merged_into IS NULL
               AND (?1 IS NULL OR substr(c.updated_at, 1, 10) = ?1)
             GROUP BY c.status",
        )
        .bind(day)
        .fetch_all(&self.pool)
        .await?;

        let mut counts = Vec::new();
        for row in rows {
            let status: Status = row.get::<String, _>("status").parse()?;
            counts.push((status, row.get::<i64, _>("n")));
        }
        counts.sort_by_key(|(status, _)| Status::ALL.iter().position(|s| s == status).unwrap_or(9));
        Ok(counts)
    }

    /// How many different businesses were telephoned, on one day or ever.
    ///
    /// Different businesses, not calls: ringing the same shop three times in an
    /// afternoon is one shop you got through to the list on.
    async fn calls_on(&self, day: Option<&str>) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(DISTINCT k.business_id) AS n
             FROM call k
             JOIN business b ON b.id = k.business_id
             WHERE b.merged_into IS NULL
               AND (?1 IS NULL OR k.called_on = ?1)",
        )
        .bind(day)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>("n"))
    }

    /// The last moment anything was marked, or an empty string if nothing was.
    async fn last_marked(&self) -> Result<String> {
        let row = sqlx::query("SELECT MAX(updated_at) AS at FROM contact")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<Option<String>, _>("at").unwrap_or_default())
    }
}

/// The states worth counting on a scoreboard, in the order they are shown.
///
/// Pure, and its own function because the screen and the spreadsheet must offer the
/// same six in the same order. "To call" is left out on purpose: it is everything
/// you have not done, which is the one number that is not a measure of a day's work.
pub fn worth_counting() -> Vec<Status> {
    Status::ALL
        .into_iter()
        .filter(|status| *status != Status::NeedCall)
        .collect()
}

/// How many are in one state, from a counted list.
///
/// Pure. A state nobody is in is absent from the list rather than nought, and every
/// reader of these counts wants nought.
pub fn how_many(counts: &[(Status, i64)], status: Status) -> i64 {
    counts
        .iter()
        .find(|(each, _)| *each == status)
        .map(|(_, n)| *n)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "tally_tests.rs"]
mod tests;
