//! What happened when you telephoned a business.

use sqlx::Row;
use webleads_domain::{after_no_answer, with_no_answers, Contact, Status, Verdict};

use crate::filter::{on_the_list, stored_status, Wanted};
use crate::{today, Result, Store};

/// The most unanswered calls we will store against one business.
///
/// The stepper cannot reach it — it is there so a broken caller cannot write a
/// number with no meaning into the column.
const MOST_NO_ANSWERS: i64 = 99;

/// A call you said you would make, and whether it is already late.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Due {
    pub id: String,
    pub name: String,
    pub phone: Option<String>,
    /// When you said you would ring, as `2026-09-21 14:30`.
    pub when: String,
    /// Already past. Worked out here, because the page cannot read a clock: a
    /// WebAssembly build has no timezone database in it.
    pub overdue: bool,
    /// Whose move it is: true when they promised to ring you, false when you
    /// promised to ring them. Both are appointments, and both belong in the panel.
    pub theirs: bool,
    pub note: String,
}

impl Store {
    /// What we know about dealing with this business.
    ///
    /// A business with no row here has never been touched, which is the same thing as
    /// [`Status::default`] — so the absence of a row means something, and we do not
    /// write one for every business we have merely heard of.
    pub async fn contact(&self, business_id: &str) -> Result<Contact> {
        let row = sqlx::query(
            "SELECT c.status, c.note, c.not_reached_counter, c.follow_up_tstamp,
                    (SELECT MAX(called_on) FROM call WHERE business_id = ?1) AS called_on
             FROM contact c WHERE c.business_id = ?1",
        )
        .bind(business_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None => Ok(Contact::default()),
            Some(row) => Ok(Contact {
                status: row.get::<String, _>("status").parse()?,
                note: row.get("note"),
                // The latest call, read from the call table rather than a single
                // column that could only ever hold one.
                called_on: row.get("called_on"),
                not_reached_counter: row.get("not_reached_counter"),
                follow_up_tstamp: row.get("follow_up_tstamp"),
            }),
        }
    }

    /// Record where a business has got to, and come back with the whole contact.
    ///
    /// Three things happen besides the state itself, and all three are rules rather
    /// than conveniences:
    ///
    /// - **"No answer" counts.** Each press is another unanswered call, and past
    ///   [`webleads_domain::GIVE_UP_AFTER`] of them the state becomes `dead` — the
    ///   rule lives in [`after_no_answer`], not here.
    /// - **A call is written down** when the state you chose means you have just
    ///   rung them. Only on a change, except for "no answer", which is the one you
    ///   press again and again.
    /// - **Leaving "ring back" clears the time**, so a business you have sold does
    ///   not sit in the coming-up panel for ever.
    ///
    /// Moving back to "to call" keeps every call already made: the counters depend on
    /// them, and losing them would be losing data to undo a misclick.
    pub async fn set_status(&self, business_id: &str, status: Status) -> Result<Contact> {
        let before = self.contact(business_id).await?;

        // "No answer" is the only state that changes something besides itself.
        let (status, counter) = match status {
            Status::NotReached => after_no_answer(before.not_reached_counter),
            other => (other, before.not_reached_counter),
        };

        // A time to ring back only belongs to the state that means it.
        let ring_back = match status {
            Status::FollowUp => before.follow_up_tstamp.clone(),
            _ => None,
        };

        self.write_contact(business_id, status, counter, ring_back.as_deref())
            .await?;

        // Pressing "no answer" twice is two unanswered calls; pressing "sold" twice
        // is one sale and a slipped finger.
        if rang_them(status) && (status == Status::NotReached || status != before.status) {
            self.record_call(business_id).await?;
        }

        self.contact(business_id).await
    }

    /// Correct how many times nobody answered.
    ///
    /// The stepper in the picker. Going past the limit gives up on the number and
    /// coming back under it takes it up again — one rule, in
    /// [`with_no_answers`], whether the count changed because you rang or because you
    /// fixed it by hand. No call is recorded: you are correcting the record, not
    /// making one.
    pub async fn set_no_answers(&self, business_id: &str, count: i64) -> Result<Contact> {
        let before = self.contact(business_id).await?;
        let count = count.clamp(0, MOST_NO_ANSWERS);
        let status = with_no_answers(before.status, count);
        self.write_contact(
            business_id,
            status,
            count,
            before.follow_up_tstamp.as_deref(),
        )
        .await?;
        self.contact(business_id).await
    }

    /// Set — or clear — when to ring this business back.
    ///
    /// Setting a time means somebody is ringing somebody at it, so it puts the
    /// business into a state that carries one — unless it is already in one, in
    /// which case whose move it is has already been said and must not be overruled:
    /// changing the time on a "they'll call" must not turn it into a call you owe. Clearing it leaves the state alone: the promise stands, you have simply
    /// stopped saying when.
    pub async fn set_ring_back(&self, business_id: &str, when: Option<&str>) -> Result<Contact> {
        let before = self.contact(business_id).await?;
        let status = match when {
            Some(_) if before.status.carries_a_time() => before.status,
            Some(_) => Status::FollowUp,
            None => before.status,
        };
        self.write_contact(business_id, status, before.not_reached_counter, when)
            .await?;
        self.contact(business_id).await
    }

    /// The state, the count and the time, written in one go.
    ///
    /// One writer rather than three, so a caller cannot change the state and leave
    /// the count behind — which is exactly how a business ends up dead with no
    /// unanswered calls against it.
    ///
    /// The stamp is a moment, not a day: a second screen watches the latest of them
    /// to tell whether somebody marked something on another device, and a date alone
    /// could not tell this morning's marking from this afternoon's.
    async fn write_contact(
        &self,
        business_id: &str,
        status: Status,
        not_reached_counter: i64,
        follow_up_tstamp: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO contact
                 (business_id, status, not_reached_counter, follow_up_tstamp, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(business_id) DO UPDATE SET
                 status              = excluded.status,
                 not_reached_counter = excluded.not_reached_counter,
                 follow_up_tstamp    = excluded.follow_up_tstamp,
                 updated_at          = excluded.updated_at",
        )
        .bind(business_id)
        .bind(status.as_str())
        .bind(not_reached_counter)
        .bind(follow_up_tstamp)
        .bind(webleads_domain::now_here())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Write down that you telephoned this business today.
    ///
    /// Public because marking someone's state is not the only way to ring them —
    /// you may already have them marked and simply be ringing again.
    pub async fn record_call(&self, business_id: &str) -> Result<()> {
        sqlx::query("INSERT INTO call (business_id, called_on) VALUES (?1, ?2)")
            .bind(business_id)
            .bind(today())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Every call to this business, newest first.
    pub async fn calls(&self, business_id: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT called_on FROM call WHERE business_id = ?1 ORDER BY called_on DESC",
        )
        .bind(business_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get("called_on")).collect())
    }

    /// Save whatever you wrote about this business.
    pub async fn set_note(&self, business_id: &str, note: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO contact (business_id, note, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(business_id) DO UPDATE SET
                 note       = excluded.note,
                 updated_at = excluded.updated_at",
        )
        .bind(business_id)
        .bind(note)
        .bind(webleads_domain::now_here())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Every call that has a time on it, soonest first.
    ///
    /// Both the ones you promised and the ones they promised: a shop waiting to ring
    /// you is still an appointment, and one that never rings is worth chasing. A shop
    /// you have since sold to, or given up on, is not coming up any more.
    pub async fn coming_up(&self, limit: i64) -> Result<Vec<Due>> {
        let rows = sqlx::query(
            "SELECT b.id, b.name, b.phone, c.status, c.follow_up_tstamp AS when_to, c.note
             FROM contact c
             JOIN business b ON b.id = c.business_id
             WHERE c.status IN (?1, ?2)
               AND c.follow_up_tstamp IS NOT NULL
               AND b.merged_into IS NULL
             ORDER BY c.follow_up_tstamp ASC
             LIMIT ?3",
        )
        .bind(Status::FollowUp.as_str())
        .bind(Status::TheyCall.as_str())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let now = webleads_domain::now_here();
        Ok(rows
            .into_iter()
            .map(|row| {
                let when: String = row.get("when_to");
                let status: String = row.get("status");
                Due {
                    overdue: webleads_domain::ring_back_overdue(&when, &now),
                    theirs: status == Status::TheyCall.as_str(),
                    id: row.get("id"),
                    name: row.get("name"),
                    phone: row.get("phone"),
                    when,
                    note: row.get("note"),
                }
            })
            .collect())
    }

    /// How many businesses are sitting in each state.
    ///
    /// Businesses with no contact row have never been touched, so they count as
    /// [`Status::default`] — which is why this counts them separately rather than
    /// only reading the contact table.
    pub async fn status_counts(&self, want: &Wanted) -> Result<Vec<(Status, i64)>> {
        let (on_the_list, status) = (on_the_list(), stored_status());
        // Counted through the same filter the list itself uses, so the buttons and
        // "showing X of Y" can never disagree again. Only the status narrowing is
        // dropped — otherwise every button would report its own count as the total.
        let counting = Wanted {
            status: None,
            // Every state, the finished ones included — a button that offers to show
            // them cannot for ever report nought.
            every_state: true,
            ..want.clone()
        };
        let narrowing = self.narrowing_for(&counting).await?;
        let rows = sqlx::query(&format!(
            "SELECT {status} AS status, COUNT(*) AS n
             {on_the_list}{narrowing}
             GROUP BY {status}"
        ))
        // All eight, in the order the fragment declares them. Three were bound here
        // for a long time and the other five arrived as NULL — and every one of those
        // clauses reads `?n IS NULL OR …`, so they quietly did nothing. The chips said
        // 2504 to call while the list showed 127.
        .bind(counting.status.map(Status::as_str))
        .bind(counting.search_pattern())
        .bind(counting.every_state)
        .bind(counting.least_reviews)
        .bind(counting.least_rating)
        .bind(counting.within_m)
        .bind(&counting.trade)
        .bind(counting.why.map(Verdict::as_str))
        .fetch_all(&self.pool)
        .await?;

        let mut counts = Vec::new();
        for row in rows {
            let status: Status = row.get::<String, _>("status").parse()?;
            counts.push((status, row.get::<i64, _>("n")));
        }
        // Always in the order we show them, whatever order SQLite grouped them in.
        counts.sort_by_key(|(status, _)| Status::ALL.iter().position(|s| s == status).unwrap_or(9));
        Ok(counts)
    }
}

/// Does choosing this state mean you have just rung them?
///
/// Pure, so "which presses write a call down" is a test rather than something you
/// discover from a call count that looks wrong. Going back to "to call" is undoing a
/// misclick, and "line dead" is reached by counting the calls that are already
/// written down — neither is a telephone call.
pub fn rang_them(status: Status) -> bool {
    match status {
        Status::NotReached
        | Status::FollowUp
        | Status::TheyCall
        | Status::Sold
        | Status::NotInterested => true,
        Status::NeedCall | Status::Dead => false,
    }
}

#[cfg(test)]
#[path = "contacts_tests.rs"]
mod tests;
