//! The places you can search, kept where you can add to them.

use sqlx::Row;
use webleads_domain::Area;

use crate::{today, Error, Result, Store};

/// Which city a new area goes into.
pub enum InCity<'a> {
    /// None: still searchable, its leads simply sit under no chip.
    None,
    /// One we already keep, by its number.
    Known(i64),
    /// A new one, made in the same step, centred where the area is.
    New {
        city: &'a webleads_domain::City,
        centre_label: &'a str,
    },
}

/// A saved area, with the number the page uses to point at it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SavedArea {
    pub id: i64,
    /// The city it belongs to, if any — whose starting point a search here uses.
    pub city_id: Option<i64>,
    #[serde(flatten)]
    pub area: Area,
}

impl Store {
    /// Every area you can search, oldest first.
    pub async fn areas(&self) -> Result<Vec<SavedArea>> {
        let rows =
            sqlx::query("SELECT id, label, address, lat, lon, city_id FROM area ORDER BY id")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(read_area).collect())
    }

    /// One area by its number.
    pub async fn area(&self, id: i64) -> Result<Option<SavedArea>> {
        let row =
            sqlx::query("SELECT id, label, address, lat, lon, city_id FROM area WHERE id = ?1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(read_area))
    }

    /// Keep a new area, in no city.
    pub async fn add_area(&self, area: &Area) -> Result<i64> {
        self.add_area_in(area, InCity::None).await
    }

    /// Keep a new area, and the city it belongs to — made here if it is new.
    ///
    /// All or nothing, in one transaction: a refused city must not leave its area
    /// behind, or the next try says "there is already an area called London" about a
    /// place the page never showed you.
    ///
    /// A name is required and must be new: the name is what goes into the search
    /// words, so two areas called the same thing would be two searches nobody could
    /// tell apart afterwards.
    pub async fn add_area_in(&self, area: &Area, city: InCity<'_>) -> Result<i64> {
        let label = area.label.trim();
        if label.is_empty() {
            return Err(Error::Refused(
                "an area needs a name — it is what goes into the search words".into(),
            ));
        }
        if let InCity::New { city, .. } = &city {
            self.check_new_city(city).await?;
        }

        let mut tx = self.pool.begin().await?;
        let taken: Option<(i64,)> = sqlx::query_as("SELECT id FROM area WHERE label = ?1")
            .bind(label)
            .fetch_optional(&mut *tx)
            .await?;
        if taken.is_some() {
            return Err(Error::Refused(format!(
                "there is already an area called {label}"
            )));
        }

        let city_id = match city {
            InCity::None => None,
            InCity::Known(id) => Some(id),
            InCity::New { city, centre_label } => {
                Some(crate::cities::insert_city(&mut tx, city, Some(centre_label)).await?)
            }
        };

        // The table still has a `radius_m` column from when areas had a fence. It is
        // written as nothing and read by nobody; dropping a column in SQLite means
        // rebuilding the table, and an unused column costs nothing.
        let done = sqlx::query(
            "INSERT INTO area (label, address, lat, lon, added_on, city_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(label)
        .bind(&area.address)
        .bind(area.lat)
        .bind(area.lon)
        .bind(today())
        .bind(city_id)
        .execute(&mut *tx)
        .await?;
        let id = done.last_insert_rowid();
        tx.commit().await?;
        Ok(id)
    }

    /// Forget an area. The leads it found stay exactly where they are.
    pub async fn remove_area(&self, id: i64) -> Result<bool> {
        let done = sqlx::query("DELETE FROM area WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The area chosen for the next search, remembered across restarts.
    ///
    /// Falls back to the first area on the list when nothing has been chosen, or when
    /// what was chosen has since been forgotten — a missing setting must not leave
    /// the program with nowhere to search.
    pub async fn chosen_area(&self) -> Result<Option<SavedArea>> {
        if let Some(id) = self.setting(crate::settings::CHOSEN_AREA).await? {
            if let Ok(id) = id.parse::<i64>() {
                if let Some(area) = self.area(id).await? {
                    return Ok(Some(area));
                }
            }
        }
        Ok(self.areas().await?.into_iter().next())
    }

    /// Choose the area the next search will look at.
    pub async fn choose_area(&self, id: i64) -> Result<bool> {
        if self.area(id).await?.is_none() {
            return Ok(false);
        }
        self.set_setting(crate::settings::CHOSEN_AREA, &id.to_string())
            .await?;
        Ok(true)
    }
}

/// One row as an area. Written by hand so nothing is guessed.
fn read_area(row: sqlx::sqlite::SqliteRow) -> SavedArea {
    SavedArea {
        id: row.get("id"),
        city_id: row.get("city_id"),
        area: Area {
            label: row.get("label"),
            address: row.get("address"),
            lat: row.get("lat"),
            lon: row.get("lon"),
            max_results: 200,
        },
    }
}

#[cfg(test)]
#[path = "areas_tests.rs"]
mod tests;
