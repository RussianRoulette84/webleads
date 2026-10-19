//! Putting right a verdict we have already stored.
//!
//! Every verdict on the list was decided once, by whatever the rules said that day.
//! When a rule turns out to have been wrong, the rows it decided are wrong with it —
//! and the only honest thing to do is go back over them.
//!
//! What this crate does *not* do is decide anything: the judging lives in
//! `sitecheck`, and the caller hands the answer in.

use sqlx::Row;
use webleads_domain::{Tls, Verdict};

use crate::{Result, Store};

/// A business whose verdict is worth asking about again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suspect {
    pub business_id: String,
    pub name: String,
    /// The address we judged, exactly as we stored it.
    pub website: String,
    /// What we said about it last time.
    pub reason: String,
}

impl Store {
    /// Every business we called broken, newest verdict only.
    ///
    /// For going back over a rule that has changed. Ordered by name so a long run
    /// reads like a list rather than like a shuffle.
    pub async fn called_broken(&self) -> Result<Vec<Suspect>> {
        let rows = sqlx::query(
            "SELECT b.id, b.name, l.website, l.reason
             FROM lead l
             JOIN business b ON b.id = l.business_id
             WHERE l.is_latest = 1
               AND l.verdict = ?1
               AND l.website IS NOT NULL
               AND TRIM(l.website) <> ''
               AND b.merged_into IS NULL
             ORDER BY b.name COLLATE NOCASE",
        )
        .bind(Verdict::DeadSite.as_str())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Suspect {
                business_id: row.get("id"),
                name: row.get("name"),
                website: row.get("website"),
                reason: row.get("reason"),
            })
            .collect())
    }

    /// Write a fresh verdict over the newest row for one business.
    ///
    /// The date of the check moves too. Without it the next import would see a
    /// business it checked weeks ago, fetch the site again, and reach the same
    /// answer we have just written down.
    pub async fn rewrite_verdict(
        &self,
        business_id: &str,
        verdict: Verdict,
        reason: &str,
        tls: &Tls,
    ) -> Result<u64> {
        let (tls_word, detail) = match tls {
            Tls::Unknown => ("unknown", None),
            Tls::Valid => ("valid", None),
            Tls::Broken(detail) => ("broken", Some(detail.clone())),
        };

        let done = sqlx::query(
            "UPDATE lead
                SET verdict = ?2, reason = ?3, tls = ?4, tls_detail = ?5, tls_rank = ?6
              WHERE business_id = ?1 AND is_latest = 1",
        )
        .bind(business_id)
        .bind(verdict.as_str())
        .bind(reason)
        .bind(tls_word)
        .bind(detail)
        // How much it matters, not how it is spelled: broken first. The same order
        // `record` writes, so sorting by certificate does not change meaning.
        .bind(match tls {
            Tls::Broken(_) => 0_i64,
            Tls::Unknown => 1,
            Tls::Valid => 2,
        })
        .execute(&self.pool)
        .await?
        .rows_affected();

        sqlx::query("UPDATE business SET checked_on = ?2 WHERE id = ?1")
            .bind(business_id)
            .bind(crate::today())
            .execute(&self.pool)
            .await?;

        Ok(done)
    }
}

#[cfg(test)]
#[path = "verdicts_tests.rs"]
mod tests;
