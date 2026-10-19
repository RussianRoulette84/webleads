//! Joining businesses that turn out to be the same shop.

use sqlx::Row;
use webleads_domain::Status;

use crate::{Result, Store};

/// One join that happened, for the log and for putting right later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    /// The business that survived.
    pub kept: String,
    pub kept_name: String,
    /// The one folded into it.
    pub gone: String,
    pub gone_name: String,
    /// The number they had in common.
    pub phone: String,
}

impl Store {
    /// Join businesses that share a telephone number.
    ///
    /// Two listings with the same number are almost always one shop that Google knows
    /// twice — a slightly different name, a moved pin, a second entry somebody made
    /// years ago. Left alone they are two rows on your call list and you ring the same
    /// person twice.
    ///
    /// **The one that has been known longest wins.** It carries the `first_seen` date,
    /// which is the whole point of that column, so folding the older into the newer
    /// would throw away exactly the history we keep.
    ///
    /// Nothing is deleted. The loser keeps its row with `merged_into` pointing at the
    /// winner, so a join that turns out to be wrong — a chain, a shared office, a
    /// mistyped number — can be seen and undone rather than being one-way.
    pub async fn merge_duplicates(&self) -> Result<Vec<Merged>> {
        let groups = sqlx::query(
            "SELECT phone_canonical, COUNT(*) AS n
             FROM business
             WHERE phone_canonical IS NOT NULL
               AND phone_canonical <> ''
               AND merged_into IS NULL
             GROUP BY phone_canonical
             HAVING n > 1",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut merged = Vec::new();
        for group in groups {
            let phone: String = group.get("phone_canonical");
            merged.extend(self.merge_one_number(&phone).await?);
        }

        for m in &merged {
            tracing::info!(
                "merged `{}` into `{}` — both on {}",
                m.gone_name,
                m.kept_name,
                m.phone
            );
        }
        Ok(merged)
    }

    /// Fold every business on one number into the longest-known of them.
    async fn merge_one_number(&self, phone: &str) -> Result<Vec<Merged>> {
        // Oldest first, so the first row is the one that keeps its history.
        let rows = sqlx::query(
            // Oldest date first. When two were first seen on the same day — which is
            // every business found in one import — the one we wrote down first wins.
            // Falling back to the id would pick alphabetically, which means nothing.
            "SELECT id, name FROM business
             WHERE phone_canonical = ?1 AND merged_into IS NULL
             ORDER BY first_seen ASC, rowid ASC",
        )
        .bind(phone)
        .fetch_all(&self.pool)
        .await?;

        let Some(winner) = rows.first() else {
            return Ok(Vec::new());
        };
        let (kept, kept_name): (String, String) = (winner.get("id"), winner.get("name"));

        let mut merged = Vec::new();
        for loser in rows.iter().skip(1) {
            let (gone, gone_name): (String, String) = (loser.get("id"), loser.get("name"));
            self.fold(&kept, &gone).await?;
            merged.push(Merged {
                kept: kept.clone(),
                kept_name: kept_name.clone(),
                gone,
                gone_name,
                phone: phone.to_owned(),
            });
        }
        Ok(merged)
    }

    /// Move everything from one business onto another.
    ///
    /// All of it in one transaction: half a merge would leave a business whose leads
    /// point one way and whose notes point another, which is worse than not merging.
    async fn fold(&self, kept: &str, gone: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // The calls simply move across — they all happened, whoever we thought we
        // were ringing.
        sqlx::query("UPDATE call SET business_id = ?1 WHERE business_id = ?2")
            .bind(kept)
            .bind(gone)
            .execute(&mut *tx)
            .await?;

        // The contacts combine rather than one winning outright. Losing a note
        // because the other row happened to be older would be the wrong trade.
        let contacts = sqlx::query(
            "SELECT business_id, status, note, not_reached_counter, follow_up_tstamp
             FROM contact WHERE business_id IN (?1, ?2)",
        )
        .bind(kept)
        .bind(gone)
        .fetch_all(&mut *tx)
        .await?;

        if !contacts.is_empty() {
            let mut status = Status::default();
            let mut notes: Vec<String> = Vec::new();
            // The most unanswered calls either of them collected, and the soonest
            // promise to ring back. Both are the answer you would want if you had
            // known all along that these were one shop.
            let mut no_answers = 0_i64;
            let mut ring_back: Option<String> = None;
            for row in &contacts {
                let theirs: Status = row.get::<String, _>("status").parse()?;
                status = further_along(status, theirs);
                let note: String = row.get("note");
                if !note.trim().is_empty() && !notes.contains(&note) {
                    notes.push(note);
                }
                no_answers = no_answers.max(row.get::<i64, _>("not_reached_counter"));
                if let Some(theirs) = row.get::<Option<String>, _>("follow_up_tstamp") {
                    let sooner = match ring_back.as_deref() {
                        None => true,
                        Some(soonest) => theirs.as_str() < soonest,
                    };
                    if sooner {
                        ring_back = Some(theirs);
                    }
                }
            }
            // A time to ring back only belongs to the state that means it.
            let ring_back = ring_back.filter(|_| status == Status::FollowUp);

            sqlx::query("DELETE FROM contact WHERE business_id IN (?1, ?2)")
                .bind(kept)
                .bind(gone)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO contact
                     (business_id, status, note, not_reached_counter, follow_up_tstamp,
                      updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(kept)
            .bind(status.as_str())
            .bind(notes.join(" · "))
            .bind(no_answers)
            .bind(ring_back)
            .bind(webleads_domain::now_here())
            .execute(&mut *tx)
            .await?;
        }

        // The leads move too, but the winner may already have a row for the same run,
        // and `(business_id, run_id)` is the key. Anything that would collide is
        // dropped rather than failing the whole merge — the winner's own verdict for
        // that run is the one to keep.
        sqlx::query(
            "DELETE FROM lead WHERE business_id = ?2
             AND run_id IN (SELECT run_id FROM lead WHERE business_id = ?1)",
        )
        .bind(kept)
        .bind(gone)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE lead SET business_id = ?1 WHERE business_id = ?2")
            .bind(kept)
            .bind(gone)
            .execute(&mut *tx)
            .await?;

        // Only the newest of everything the winner now holds is the current one.
        sqlx::query(
            "UPDATE lead SET is_latest = CASE
                 WHEN run_id = (SELECT MAX(run_id) FROM lead WHERE business_id = ?1)
                 THEN 1 ELSE 0 END
             WHERE business_id = ?1",
        )
        .bind(kept)
        .execute(&mut *tx)
        .await?;

        // The loser stays, pointing at where it went.
        sqlx::query("UPDATE business SET merged_into = ?1 WHERE id = ?2")
            .bind(kept)
            .bind(gone)
            .execute(&mut *tx)
            .await?;

        // And the winner keeps the earliest date either of them had.
        sqlx::query(
            "UPDATE business SET first_seen = (
                 SELECT MIN(first_seen) FROM business WHERE id IN (?1, ?2)
             ) WHERE id = ?1",
        )
        .bind(kept)
        .bind(gone)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Where a business ended up, if it was merged away.
    pub async fn merged_into(&self, id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT merged_into FROM business WHERE id = ?1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.and_then(|r| r.get::<Option<String>, _>("merged_into")))
    }
}

/// Which of two states is further along.
///
/// The ranking itself is [`Status::progress`], in the domain, because the same
/// question is asked elsewhere: sold beats a refusal, a refusal beats a promise to
/// ring back, and a number that stopped answering loses to anybody you have actually
/// spoken to.
///
/// Pure, so the rule that decides what survives a merge can be tested on its own.
pub fn further_along(a: Status, b: Status) -> Status {
    if b.progress() > a.progress() {
        b
    } else {
        a
    }
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
