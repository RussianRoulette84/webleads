//! The businesses you never want to see again.
//!
//! # What this file is here to teach
//!
//! Hiding without deleting. Every row here removes businesses from the call list, and
//! every one of them can be undone — the business itself keeps its dates, its notes
//! and every telephone call, because a list that can lose work you have already done
//! is a list you stop trusting.

use sqlx::Row;
use webleads_domain::{chain_label, chain_pattern};

use crate::filter::Wanted;
use crate::{today, Error, Result, Store};

/// One rule about what not to show, and what it currently catches.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Hidden {
    pub id: i64,
    /// What you hid, in words — a shop's name or a chain's.
    pub label: String,
    /// True for a chain, false for a single business.
    pub is_chain: bool,
    /// How many businesses it is keeping off the list right now.
    pub catches: i64,
    pub added_on: String,
}

impl Store {
    /// Hide one business by its id.
    pub async fn hide_business(&self, business_id: &str, label: &str) -> Result<i64> {
        let done =
            sqlx::query("INSERT INTO hidden (business_id, label, added_on) VALUES (?1, ?2, ?3)")
                .bind(business_id)
                .bind(label)
                .bind(today())
                .execute(&self.pool)
                .await?;
        Ok(done.last_insert_rowid())
    }

    /// Hide every branch of the chain this business belongs to.
    ///
    /// Refuses a name with nothing in it: the pattern would be `%%`, which matches
    /// every row, and the whole list would vanish with no obvious way back.
    pub async fn hide_chain(&self, name: &str) -> Result<i64> {
        let label = chain_label(name);
        if label.is_empty() {
            return Err(Error::Refused(
                "a chain needs a name — hiding on a blank one would hide everything".into(),
            ));
        }
        let done =
            sqlx::query("INSERT INTO hidden (name_like, label, added_on) VALUES (?1, ?2, ?3)")
                .bind(chain_pattern(name))
                .bind(&label)
                .bind(today())
                .execute(&self.pool)
                .await?;
        Ok(done.last_insert_rowid())
    }

    /// What one business is called, for hiding it by name.
    ///
    /// The caller says which business; what it is called is ours to know, so a
    /// request cannot hide a chain by claiming a name that is not there.
    pub async fn business_name(&self, business_id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT name FROM business WHERE id = ?1")
            .bind(business_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("name")))
    }

    /// Everything you have hidden, newest first, with how many each one catches.
    pub async fn hidden(&self) -> Result<Vec<Hidden>> {
        let rows = sqlx::query(
            "SELECT h.id, h.label, h.added_on, h.name_like IS NOT NULL AS is_chain,
                    (SELECT COUNT(*) FROM business b
                      WHERE (h.business_id IS NOT NULL AND h.business_id = b.id)
                         OR (h.name_like IS NOT NULL
                             AND LOWER(b.name) LIKE h.name_like ESCAPE '\\')) AS catches
             FROM hidden h
             ORDER BY h.id DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Hidden {
                id: row.get("id"),
                label: row.get("label"),
                is_chain: row.get::<i64, _>("is_chain") == 1,
                catches: row.get("catches"),
                added_on: row.get("added_on"),
            })
            .collect())
    }

    /// Put one back on the list.
    pub async fn unhide(&self, id: i64) -> Result<bool> {
        let done = sqlx::query("DELETE FROM hidden WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// How many businesses this request would show if nothing were hidden.
    ///
    /// The number beside the filters, so a list that has quietly shrunk always says
    /// why.
    pub async fn hidden_count(&self, want: &Wanted) -> Result<i64> {
        self.lead_count(&Wanted {
            hidden_only: true,
            ..want.clone()
        })
        .await
    }
}

#[cfg(test)]
#[path = "hidden_tests.rs"]
mod tests;
