//! Rebuilding the stored opening times from Google's own JSON.
//!
//! The seven `opens_*` columns exist so the database can sort by when a shop opens.
//! They are filled when a business is written — but a database written before those
//! columns existed has none of them, and a change to the parsing would leave the old
//! answers behind. Both are fixed by rebuilding from `open_hours`, which is kept
//! exactly as Google sent it for this reason.

use sqlx::Row;
use webleads_domain::Hours;

use crate::{Result, Store};

impl Store {
    /// Fill in the opening times for any business that has hours but no parsed ones.
    ///
    /// Returns how many were rebuilt. Runs at every open: it is a few rows at most
    /// after the first time, and a list that sorts wrongly is worse than a moment at
    /// start-up.
    pub async fn rebuild_opening_times(&self) -> Result<u64> {
        let waiting = sqlx::query(
            "SELECT id, open_hours
             FROM business
             WHERE open_hours IS NOT NULL
               AND TRIM(open_hours) NOT IN ('', '{}')
               AND opens_mon IS NULL AND opens_tue IS NULL AND opens_wed IS NULL
               AND opens_thu IS NULL AND opens_fri IS NULL AND opens_sat IS NULL
               AND opens_sun IS NULL",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut rebuilt = 0;
        for row in waiting {
            let id: String = row.get("id");
            let json: String = row.get("open_hours");
            let opens = Hours::parse(&json).opens_each_day();
            // Nothing readable in there. Writing seven NULLs over seven NULLs would
            // only mean looking at it again next time.
            if opens.iter().all(Option::is_none) {
                continue;
            }

            sqlx::query(
                "UPDATE business SET
                     opens_mon = ?2, opens_tue = ?3, opens_wed = ?4, opens_thu = ?5,
                     opens_fri = ?6, opens_sat = ?7, opens_sun = ?8
                 WHERE id = ?1",
            )
            .bind(&id)
            .bind(opens[0].map(i64::from))
            .bind(opens[1].map(i64::from))
            .bind(opens[2].map(i64::from))
            .bind(opens[3].map(i64::from))
            .bind(opens[4].map(i64::from))
            .bind(opens[5].map(i64::from))
            .bind(opens[6].map(i64::from))
            .execute(&self.pool)
            .await?;
            rebuilt += 1;
        }

        if rebuilt > 0 {
            tracing::info!(rebuilt, "opening times rebuilt from the stored hours");
        }
        Ok(rebuilt)
    }
}
