//! Tidying the telephone numbers already collected.
//!
//! New businesses are written with a tidy number. Everything gathered before that
//! still holds whatever shape Google sent — `06 20 555 0103`, `(06 1) 234 5002` — so
//! this rewrites those once, and then has nothing to do ever again.

use sqlx::Row;
use webleads_domain::normalise_phone;

use crate::{Result, Store};

impl Store {
    /// Rewrite any stored number that is not already in its tidy form.
    ///
    /// Returns how many were changed. Runs at every open: after the first time it
    /// finds nothing, and a list where the same number appears three ways is worse
    /// than a moment at start-up.
    pub async fn tidy_phones(&self) -> Result<u64> {
        // Each business's own country, because the rules are the country's: `06` is a
        // Hungarian trunk prefix and a Sharjah area code. A business with no city has
        // no country, and its number keeps its digits untouched rather than being
        // read under somebody else's rules.
        let rows = sqlx::query(
            "SELECT b.id, b.phone, COALESCE(city.country_code, '') AS country_code
               FROM business b
               LEFT JOIN city ON city.id = b.city_id
              WHERE b.phone IS NOT NULL AND TRIM(b.phone) <> ''",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut tidied = 0;
        for row in rows {
            let id: String = row.get("id");
            let stored: String = row.get("phone");
            let country: String = row.get("country_code");
            let Some(tidy) = normalise_phone(&stored, &country) else {
                // Nothing readable in it. Left exactly as it is.
                continue;
            };
            if tidy == stored {
                continue;
            }

            sqlx::query("UPDATE business SET phone = ?2, phone_canonical = ?2 WHERE id = ?1")
                .bind(&id)
                .bind(&tidy)
                .execute(&self.pool)
                .await?;
            tidied += 1;
        }

        if tidied > 0 {
            tracing::info!(tidied, "telephone numbers tidied");
        }
        Ok(tidied)
    }
}

#[cfg(test)]
#[path = "phones_tests.rs"]
mod tests;
