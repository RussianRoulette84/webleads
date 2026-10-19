//! The cities you look for businesses in.
//!
//! # What this file is here to teach
//!
//! A level above the areas, added because two of the three areas were districts of
//! the third. A list narrowed by area could never show the whole of Budapest at once,
//! and no area had anywhere to record what language to search in or what time it is
//! there. Both belong to the city, once, rather than to each district separately.

use sqlx::Row;
use webleads_domain::City;

use crate::{today, Error, Result, Store};

/// The columns a city is read from. One list, so two readers cannot disagree.
const CITY_COLUMNS: &str =
    "id, name, country_code, zone, lang, centre_lat, centre_lon, centre_label";

/// A saved city, with the number the page points at it by.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SavedCity {
    pub id: i64,
    #[serde(flatten)]
    pub city: City,
    /// What the centre is called — "Hősök tere, Budapest". Empty when nobody has
    /// said, which the page reads as "the centre of the city".
    pub centre_label: Option<String>,
}

impl Store {
    /// Every city, oldest first.
    pub async fn cities(&self) -> Result<Vec<SavedCity>> {
        let rows = sqlx::query(&format!("SELECT {CITY_COLUMNS} FROM city ORDER BY id"))
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(read_city).collect())
    }

    /// One city by its number.
    pub async fn city(&self, id: i64) -> Result<Option<SavedCity>> {
        let row = sqlx::query(&format!("SELECT {CITY_COLUMNS} FROM city WHERE id = ?1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(read_city))
    }

    /// The city you live in: the one whose clock is your clock.
    ///
    /// What the page opens on, so a day's calling starts where you are rather than
    /// wherever the newest search happened to be. `None` when no city keeps your
    /// zone, which is the only honest answer — the page then opens on all of them.
    pub async fn home_city(&self) -> Result<Option<i64>> {
        let row = sqlx::query("SELECT id FROM city WHERE zone = ?1 ORDER BY id LIMIT 1")
            .bind(webleads_domain::your_zone_name())
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get("id")))
    }

    /// The city of one area, if the area has one.
    pub async fn city_of_area(&self, area_id: i64) -> Result<Option<SavedCity>> {
        let row = sqlx::query(&format!(
            "SELECT {CITY_COLUMNS} FROM city
              WHERE id = (SELECT city_id FROM area WHERE id = ?1)"
        ))
        .bind(area_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(read_city))
    }

    /// The city with this name, if we already keep one.
    ///
    /// Adding an area offers a city name the address lookup reported, and the usual
    /// answer is one we already have — Zugló and Óbuda are both Budapest.
    pub async fn city_named(&self, name: &str) -> Result<Option<SavedCity>> {
        let row = sqlx::query(&format!(
            "SELECT {CITY_COLUMNS} FROM city WHERE name = ?1 COLLATE NOCASE"
        ))
        .bind(name.trim())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(read_city))
    }

    /// Which city a business was first found in.
    ///
    /// Only for the tests and for looking into the data by hand — the list narrows by
    /// city in one clause of the query it already runs, not by asking this per row.
    pub async fn city_of_business(&self, business_id: &str) -> Result<Option<i64>> {
        let found: Option<Option<i64>> =
            sqlx::query_scalar("SELECT city_id FROM business WHERE id = ?1")
                .bind(business_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(found.flatten())
    }

    /// The city one search was made in.
    ///
    /// Read off the run rather than through its area, because an area can be
    /// forgotten while its searches remain.
    pub async fn city_of_run(&self, run_id: i64) -> Result<Option<SavedCity>> {
        let row = sqlx::query(&format!(
            "SELECT {CITY_COLUMNS} FROM city
              WHERE id = (SELECT city_id FROM run WHERE id = ?1)"
        ))
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(read_city))
    }

    /// How far ahead of your clock each city is, in minutes.
    ///
    /// Worked out now, not stored: Hungary keeps summer time and the Gulf does not, so
    /// a number written down in September is an hour wrong by November. Reading a
    /// handful of city rows is cheaper than being wrong for six months.
    ///
    /// Only the "open today" order needs it, so only that asks.
    pub async fn minutes_ahead(&self) -> Result<Vec<(i64, i32)>> {
        let rows = sqlx::query("SELECT id, zone FROM city")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let zone: String = row.get("zone");
                (
                    row.get::<i64, _>("id"),
                    webleads_domain::minutes_ahead_of_you(&zone),
                )
            })
            // A city on your own clock needs no arm in the expression.
            .filter(|(_, minutes)| *minutes != 0)
            .collect())
    }

    /// Keep a new city.
    pub async fn add_city(&self, city: &City) -> Result<i64> {
        self.check_new_city(city).await?;
        let mut tx = self.pool.begin().await?;
        let id = insert_city(&mut tx, city, None).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Whether this city can be kept.
    ///
    /// The name must be new. Two cities of the same name would be two sets of chips
    /// nobody could tell apart, and the leads would be split between them for ever.
    pub(crate) async fn check_new_city(&self, city: &City) -> Result<()> {
        let name = city.name.trim();
        if name.is_empty() {
            return Err(Error::Refused("a city needs a name".to_owned()));
        }
        if self.city_named(name).await?.is_some() {
            return Err(Error::Refused(format!(
                "there is already a city called {name}"
            )));
        }
        // Not a lookup we can do without: a zone the clock does not know would be
        // stored happily and then fail every time an opening hour was read.
        if time_tz::timezones::get_by_name(city.zone.trim()).is_none() {
            return Err(Error::Refused(format!(
                "`{}` is not a clock anybody keeps — it should read like Europe/Budapest",
                city.zone
            )));
        }
        Ok(())
    }

    /// Put an area in a city.
    pub async fn area_belongs_to(&self, area_id: i64, city_id: i64) -> Result<()> {
        sqlx::query("UPDATE area SET city_id = ?1 WHERE id = ?2")
            .bind(city_id)
            .bind(area_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Forget a city, and let go of anything pointing at it.
    ///
    /// The areas and the businesses keep existing with no city rather than
    /// disappearing with it — a business is a business whether or not we still keep a
    /// record of the town it was found in.
    pub async fn remove_city(&self, id: i64) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE area SET city_id = NULL WHERE city_id = ?1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE business SET city_id = NULL WHERE city_id = ?1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM city WHERE id = ?1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// What a remeasure came to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Remeasured {
    /// How many leads were looked at.
    pub considered: usize,
    /// How many came out with a different number than they had.
    pub moved: usize,
    /// The biggest single change, in metres. Nought when nothing moved.
    pub biggest_change_m: u32,
}

impl Store {
    /// Measure every lead of one city from that city's centre.
    ///
    /// Puts the whole column on one footing after a centre moves, and reports how many
    /// rows changed rather than assuming.
    ///
    /// A business with no coordinates is skipped: there is nothing to measure.
    pub async fn remeasure_city(&self, city_id: i64, city: &City) -> Result<Remeasured> {
        let rows = sqlx::query(
            "SELECT l.rowid AS row_id, l.distance_m AS was, b.lat AS lat, b.lon AS lon
               FROM lead l
               JOIN business b ON b.id = l.business_id
              WHERE b.city_id = ?1 AND b.lat IS NOT NULL AND b.lon IS NOT NULL",
        )
        .bind(city_id)
        .fetch_all(&self.pool)
        .await?;

        let mut counted = Remeasured::default();
        let mut tx = self.pool.begin().await?;
        for row in rows {
            counted.considered += 1;
            let was: Option<i64> = row.get("was");
            let now = city.distance_m(row.get("lat"), row.get("lon")).round() as i64;
            if was == Some(now) {
                continue;
            }
            counted.moved += 1;
            if let Some(was) = was {
                let by = was.abs_diff(now) as u32;
                counted.biggest_change_m = counted.biggest_change_m.max(by);
            }
            sqlx::query("UPDATE lead SET distance_m = ?1 WHERE rowid = ?2")
                .bind(now)
                .bind(row.get::<i64, _>("row_id"))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(counted)
    }

    /// Measure a city from somewhere else: your own street rather than its middle.
    ///
    /// Moves the centre, names it, and re-measures every lead of the city from there,
    /// so the nearest-first list is right the moment it is read again. The spot is
    /// checked first; a city we do not keep is a refusal.
    pub async fn move_centre(
        &self,
        city_id: i64,
        label: &str,
        lat: f64,
        lon: f64,
    ) -> Result<(SavedCity, Remeasured)> {
        let label = webleads_domain::check_centre(label, lat, lon).map_err(Error::Refused)?;
        let Some(mut saved) = self.city(city_id).await? else {
            return Err(Error::Refused(format!("there is no city {city_id}")));
        };
        let near = self.anchors_of(&saved).await?;
        webleads_domain::check_centre_reach(&saved.city.name, &label, lat, lon, &near)
            .map_err(Error::Refused)?;
        sqlx::query(
            "UPDATE city SET centre_label = ?1, centre_lat = ?2, centre_lon = ?3 WHERE id = ?4",
        )
        .bind(&label)
        .bind(lat)
        .bind(lon)
        .bind(city_id)
        .execute(&self.pool)
        .await?;

        saved.city.centre_lat = lat;
        saved.city.centre_lon = lon;
        saved.centre_label = Some(label);
        let counted = self.remeasure_city(city_id, &saved.city).await?;
        Ok((saved, counted))
    }

    /// Where a city is: its own areas, or its centre when it has none.
    async fn anchors_of(&self, saved: &SavedCity) -> Result<Vec<webleads_domain::Area>> {
        let areas: Vec<(f64, f64)> = sqlx::query_as("SELECT lat, lon FROM area WHERE city_id = ?1")
            .bind(saved.id)
            .fetch_all(&self.pool)
            .await?;
        let spots = if areas.is_empty() {
            vec![saved.city.centre()]
        } else {
            areas
                .into_iter()
                .map(|(lat, lon)| webleads_domain::Area {
                    label: saved.city.name.clone(),
                    address: String::new(),
                    lat,
                    lon,
                    max_results: 0,
                })
                .collect()
        };
        Ok(spots)
    }

    /// How many leads belong to no city at all.
    ///
    /// They cannot be measured against one, and guessing the town would be inventing
    /// data — so they are counted and said out loud instead.
    pub async fn leads_with_no_city(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM lead l
               JOIN business b ON b.id = l.business_id
              WHERE b.city_id IS NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }
}

/// Write a city that has been checked, inside the caller's transaction.
pub(crate) async fn insert_city(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    city: &City,
    centre_label: Option<&str>,
) -> Result<i64> {
    let done = sqlx::query(
        "INSERT INTO city (name, country_code, zone, lang, centre_lat, centre_lon, added_on, centre_label)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(city.name.trim())
    .bind(city.country_code.trim().to_lowercase())
    .bind(city.zone.trim())
    .bind(city.lang.trim().to_lowercase())
    .bind(city.centre_lat)
    .bind(city.centre_lon)
    .bind(today())
    .bind(centre_label.map(str::trim).filter(|l| !l.is_empty()))
    .execute(&mut **tx)
    .await?;
    Ok(done.last_insert_rowid())
}

/// One row as a city.
fn read_city(row: sqlx::sqlite::SqliteRow) -> SavedCity {
    SavedCity {
        id: row.get("id"),
        city: City {
            name: row.get("name"),
            country_code: row.get("country_code"),
            zone: row.get("zone"),
            lang: row.get("lang"),
            centre_lat: row.get("centre_lat"),
            centre_lon: row.get("centre_lon"),
        },
        centre_label: row.get("centre_label"),
    }
}

#[cfg(test)]
#[path = "cities_tests.rs"]
mod tests;
