//! Making one search out of a city's many.

use sqlx::Row;

use crate::{Result, Store};

/// What one city's searches became.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Gathered {
    pub city: String,
    /// The search that remains, wearing the city's name.
    pub kept: i64,
    /// How many searches went into it, the one that remains included.
    pub searches: i64,
}

impl Store {
    /// Gather every finished search of a city into one.
    ///
    /// A city searched on six evenings reads as six searches, each holding whatever
    /// it happened to see first — so none of them is "the leads in Budapest" and
    /// pressing any one of them shows a sixth of the city. Afterwards there is one
    /// search per city, holding everything those evenings found between them.
    ///
    /// What survives a gathering: every business, the newest verdict on each, and
    /// each word's best result. What does not: the separate dates, and the duplicate
    /// verdicts earlier evenings had recorded for a business a later one looked at
    /// again. A search still going is left alone — it has nothing to gather yet.
    pub async fn gather_searches(&self) -> Result<Vec<Gathered>> {
        let mut done = Vec::new();
        for (city_id, name) in self.cities_with_searches().await? {
            let searches = self.finished_searches_in(city_id).await?;
            if searches.len() < 2 {
                continue;
            }
            let kept = *searches.iter().min().expect("checked above");
            self.gather_into(kept, &searches, &name).await?;
            done.push(Gathered {
                city: name,
                kept,
                searches: searches.len() as i64,
            });
        }
        Ok(done)
    }

    /// Every city that has a search recorded against it, oldest city first.
    async fn cities_with_searches(&self) -> Result<Vec<(i64, String)>> {
        let rows = sqlx::query(
            "SELECT c.id AS id, c.name AS name
               FROM city c
              WHERE EXISTS (SELECT 1 FROM run WHERE run.city_id = c.id)
              ORDER BY c.id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("id"), row.get("name")))
            .collect())
    }

    /// The searches of one city that are over, oldest first.
    async fn finished_searches_in(&self, city_id: i64) -> Result<Vec<i64>> {
        let rows = sqlx::query(
            "SELECT id FROM run
              WHERE city_id = ?1 AND state NOT IN ('working', 'waiting')
              ORDER BY id",
        )
        .bind(city_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|row| row.get("id")).collect())
    }

    /// Fold a group of searches into the one that stays.
    ///
    /// All of it in one transaction: half a gathering would leave businesses pointing
    /// at a search that is no longer there.
    async fn gather_into(&self, kept: i64, group: &[i64], city: &str) -> Result<()> {
        let all = number_list(group);
        let mut tx = self.pool.begin().await?;

        // One verdict per business — the newest this group holds. The rest would
        // collide with it the moment they moved onto the same search.
        sqlx::query(&format!(
            "DELETE FROM lead
              WHERE run_id IN ({all})
                AND (business_id, run_id) NOT IN (
                    SELECT business_id, MAX(run_id) FROM lead
                     WHERE run_id IN ({all})
                     GROUP BY business_id)"
        ))
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!(
            "UPDATE lead SET run_id = ?1 WHERE run_id IN ({all})"
        ))
        .bind(kept)
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!(
            "UPDATE run_word SET run_id = ?1 WHERE run_id IN ({all})"
        ))
        .bind(kept)
        .execute(&mut *tx)
        .await?;

        // A word searched on three evenings is one word. The evening it worked best
        // is the one worth keeping, because the report is there to say whether the
        // word itself is any good.
        sqlx::query(
            "DELETE FROM run_word
              WHERE run_id = ?1
                AND id <> (SELECT w.id FROM run_word w
                            WHERE w.run_id = ?1 AND w.phrase = run_word.phrase
                            ORDER BY COALESCE(w.found, -1) DESC, w.id
                            LIMIT 1)",
        )
        .bind(kept)
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!(
            "UPDATE business SET first_run_id = ?1 WHERE first_run_id IN ({all})"
        ))
        .bind(kept)
        .execute(&mut *tx)
        .await?;

        // The one that stays speaks for the whole city now, and for every evening
        // that went into it.
        sqlx::query(&format!(
            "UPDATE run
                SET area_label  = ?2,
                    state       = 'done',
                    note        = NULL,
                    started_at  = (SELECT MIN(started_at) FROM run WHERE id IN ({all})),
                    finished_at = (SELECT MAX(finished_at) FROM run WHERE id IN ({all})),
                    area_lat    = (SELECT centre_lat FROM city WHERE id = run.city_id),
                    area_lon    = (SELECT centre_lon FROM city WHERE id = run.city_id),
                    area_radius_m = (SELECT MAX(area_radius_m) FROM run WHERE id IN ({all}))
              WHERE id = ?1"
        ))
        .bind(kept)
        .bind(city)
        .execute(&mut *tx)
        .await?;

        sqlx::query(&format!("DELETE FROM run WHERE id IN ({all}) AND id <> ?1"))
            .bind(kept)
            .execute(&mut *tx)
            .await?;

        // Exactly one verdict per business stands, which is the highest search number
        // it has — the same rule the rest of the program reads by.
        sqlx::query("UPDATE lead SET is_latest = 0")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE lead SET is_latest = 1
              WHERE (business_id, run_id) IN (
                    SELECT business_id, MAX(run_id) FROM lead GROUP BY business_id)",
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}

/// A list of numbers as SQL writes them.
///
/// They come from the database as whole numbers and go straight back into it, so
/// there is nothing here a person could have typed.
pub fn number_list(numbers: &[i64]) -> String {
    numbers
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "gather_tests.rs"]
mod tests;
