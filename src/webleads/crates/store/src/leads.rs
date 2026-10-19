//! Reading the call list back out, and the shape it comes back in.

use sqlx::Row;

use crate::filter::{on_the_list, stored_status, Wanted};
use webleads_domain::{Contact, Hours, Lead, Status, Tls, Verdict};

use crate::{Result, Store};

/// Everything one row of the call list needs, written once.
///
/// Two readers want it — the list and a single business — and a column added to one
/// but not the other is a field that is right on one screen and wrong on the next.
/// The state's own column is generated, so what a business with no contact row counts
/// as is decided in exactly one place: see [`stored_status`].
fn lead_columns() -> String {
    format!(
        "b.id, b.name, b.phone, b.category, b.address, b.maps_url,
                    b.trade, b.open_hours, b.lat, b.lon, b.first_seen, b.last_seen,
                    l.verdict, l.reason, l.website, l.social_url,
                    l.reviews, l.rating, l.distance_m, l.tls, l.tls_detail,
                    {} AS status,
                    COALESCE(c.note, '')      AS note,
                    COALESCE(c.not_reached_counter, 0) AS not_reached_counter,
                    c.follow_up_tstamp,
                    (SELECT MAX(called_on) FROM call WHERE business_id = b.id) AS called_on,
                    (SELECT COUNT(*) FROM call WHERE business_id = b.id)       AS call_count,
                    city.lang AS city_lang,
                    city.zone AS city_zone",
        stored_status()
    )
}

impl Store {
    /// The extra `WHERE` this request needs, with everything looked up first.
    ///
    /// Seven readers need exactly the same fragment — the list, the count, the
    /// trades, the cities, the map, the sliders' range and the state counts. One place
    /// builds it, so a reader cannot narrow by one rule and forget another.
    ///
    /// This only builds the *inlined* half. The eight bound parameters are each
    /// reader's own to bind, and the state counts bound three of them for a long
    /// time — which is exactly the way for two numbers on one screen to disagree.
    pub(crate) async fn narrowing_for(&self, want: &Wanted) -> Result<String> {
        let area_runs = match want.from_area.as_deref() {
            Some(area) => Some(self.runs_in_area(area).await?),
            None => None,
        };
        Ok(want.narrowing(area_runs.as_deref()))
    }

    /// One business by its id, whatever the filters say.
    ///
    /// For the things you do to a single row — photographing its website, reading it
    /// back after a change. Deliberately not filtered: you asked for this one.
    pub async fn lead(&self, business_id: &str) -> Result<Option<StoredLead>> {
        let lead_columns = lead_columns();
        let row = sqlx::query(&format!(
            // The same joins as the shared rule, because the columns are the same
            // columns: one of them is the city's language.
            "SELECT {lead_columns}
             FROM lead l
             JOIN business b ON b.id = l.business_id
             LEFT JOIN contact c ON c.business_id = b.id
             LEFT JOIN city ON city.id = b.city_id
             WHERE l.is_latest = 1 AND b.id = ?1"
        ))
        .bind(business_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_lead).transpose()
    }

    /// The call list: businesses worth telephoning, best first.
    ///
    /// The ordering is the same rule as [`webleads_domain::Lead::sort_key`] — never
    /// reviewed first, then quietest, then nearest — expressed in SQL so the database
    /// does the sorting rather than us sorting a list we already paid to build.
    pub async fn leads(&self, want: &Wanted) -> Result<Vec<StoredLead>> {
        let (lead_columns, on_the_list) = (lead_columns(), on_the_list());
        let narrowing = self.narrowing_for(want).await?;
        // Only the "open today" order needs the clocks, so only it pays for them.
        let ahead = match want.sort {
            webleads_domain::Sort::Hours => self.minutes_ahead().await?,
            _ => Vec::new(),
        };
        let rows = sqlx::query(&format!(
            "SELECT {lead_columns}
             {on_the_list}{narrowing}
             ORDER BY {}
             LIMIT ?9 OFFSET ?10",
            want.order_by(&ahead)
        ))
        .bind(want.status.map(Status::as_str))
        .bind(want.search_pattern())
        .bind(want.every_state)
        .bind(want.least_reviews)
        .bind(want.least_rating)
        .bind(want.within_m)
        .bind(&want.trade)
        .bind(want.why.map(Verdict::as_str))
        .bind(want.limit)
        .bind(want.offset)
        .fetch_all(&self.pool)
        .await?;

        // `collect::<Result<Vec<_>, _>>()` turns a list of results into a result
        // holding a list — the first failure stops it and comes back as the error.
        rows.into_iter().map(row_to_lead).collect()
    }

    /// How many businesses are on the call list altogether.
    ///
    /// Asked separately from [`Store::leads`] so the page can say "50 of 380" without
    /// fetching all 380.
    pub async fn lead_count(&self, want: &Wanted) -> Result<i64> {
        let on_the_list = on_the_list();
        let narrowing = self.narrowing_for(want).await?;
        let row = sqlx::query(&format!("SELECT COUNT(*) AS n {on_the_list}{narrowing}"))
            .bind(want.status.map(Status::as_str))
            .bind(want.search_pattern())
            .bind(want.every_state)
            .bind(want.least_reviews)
            .bind(want.least_rating)
            .bind(want.within_m)
            .bind(&want.trade)
            .bind(want.why.map(Verdict::as_str))
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("n"))
    }

    /// How many businesses we know about in total.
    pub async fn business_count(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM business")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("n"))
    }

    /// What we decided about these businesses recently, so we need not ask again.
    ///
    /// Returns only the ones checked within `days`, with the verdict and certificate
    /// we found last time. Everything else is missing from the map, which is the
    /// caller's signal to go and fetch it.
    ///
    /// A fortnight is the default elsewhere: a shop with no website two weeks ago
    /// almost certainly still has none, and re-fetching to find that out is both
    /// wasted effort and another roll of the dice on a slow server being mistaken for
    /// a broken one.
    pub async fn recent_verdicts(
        &self,
        days: i64,
    ) -> Result<std::collections::HashMap<String, (Verdict, Tls, String)>> {
        let rows = sqlx::query(
            "SELECT b.id, l.verdict, l.reason, l.tls, l.tls_detail
             FROM business b
             JOIN lead l ON l.business_id = b.id AND l.is_latest = 1
             WHERE b.checked_on IS NOT NULL
               AND julianday(?1) - julianday(b.checked_on) < ?2",
        )
        .bind(crate::today())
        .bind(days)
        .fetch_all(&self.pool)
        .await?;

        let mut known = std::collections::HashMap::new();
        for row in rows {
            let verdict: Verdict = row.get::<String, _>("verdict").parse()?;
            known.insert(
                row.get::<String, _>("id"),
                (verdict, tls_from_row(&row), row.get::<String, _>("reason")),
            );
        }
        Ok(known)
    }

    /// When a business was first seen, if we know it at all. Used by the tests that
    /// prove importing twice does not move the date.
    pub async fn first_seen(&self, id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT first_seen FROM business WHERE id = ?1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("first_seen")))
    }
}

/// A lead as it comes back out of the database, with the dates the database knows.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoredLead {
    #[serde(flatten)]
    pub lead: Lead,
    pub first_seen: String,
    pub last_seen: String,
    /// Where this business has got to, and what you wrote about it.
    pub contact: Contact,
    /// How many times you have telephoned. `called_on` on the contact is the latest.
    pub call_count: i64,
    /// Today's opening hours, already worked out.
    ///
    /// Resolved here rather than in the browser: working out what day it is in
    /// Budapest needs a timezone database, and a WebAssembly page has none — asking
    /// it to try left the whole table blank.
    pub hours_today: Option<String>,
    /// What the shop says its own hours are today, when that differs from yours.
    ///
    /// `None` for a business in your own city — there is nothing to convert, so
    /// showing the same time twice would be noise.
    pub hours_there: Option<String>,
    /// The language this business's own city describes its trades in.
    ///
    /// So a Dubai row reads "barber shop" and a Budapest one "fodrászat", on the same
    /// screen. `None` for a business belonging to no city.
    pub city_lang: Option<String>,
}

/// Turn one database row into types. Written by hand so nothing is guessed.
fn row_to_lead(row: sqlx::sqlite::SqliteRow) -> Result<StoredLead> {
    // Parsed on the way out rather than on the way in, so a better parser can be
    // applied to everything we already hold.
    let hours = row
        .get::<Option<String>, _>("open_hours")
        .as_deref()
        .map(Hours::parse)
        .unwrap_or_default();

    // Google writes the hours in the shop's own time, which is the one clock you
    // cannot dial by. Moved onto yours, so the whole list reads in one time and you
    // can see at a glance who is open while you are at your desk. Nought for a
    // business in your own city, and nought for a city whose clock we cannot read.
    let ahead = row
        .get::<Option<String>, _>("city_zone")
        .as_deref()
        .map(webleads_domain::minutes_ahead_of_you)
        .unwrap_or(0);
    let ours = hours.shifted(-ahead);

    let verdict: String = row.get("verdict");
    let verdict: Verdict = verdict.parse()?;

    Ok(StoredLead {
        lead: Lead {
            id: row.get("id"),
            name: row.get("name"),
            phone: row.get("phone"),
            category: row.get("category"),
            trade: row.get("trade"),
            address: row.get("address"),
            maps_url: row.get("maps_url"),
            website: row.get("website"),
            social_url: row.get("social_url"),
            verdict,
            reason: row.get("reason"),
            reviews: row.get::<i64, _>("reviews").max(0) as u32,
            rating: row.get::<Option<f64>, _>("rating").map(|r| r as f32),
            distance_m: row.get::<i64, _>("distance_m").max(0) as u32,
            position: match (
                row.get::<Option<f64>, _>("lat"),
                row.get::<Option<f64>, _>("lon"),
            ) {
                (Some(lat), Some(lon)) => Some((lat, lon)),
                _ => None,
            },
            tls: tls_from_row(&row),
            // Parsed on the way out rather than on the way in, so a better parser
            // can be applied to everything we already hold.
            // On your clock, so the whole week beneath the row reads in one time too.
            hours: ours.clone(),
        },
        first_seen: row.get("first_seen"),
        last_seen: row.get("last_seen"),
        hours_today: ours.line_for(webleads_domain::today_here()),
        // And what the shop itself would say, for when you are talking to them.
        hours_there: (ahead != 0)
            .then(|| hours.line_for(webleads_domain::today_here()))
            .flatten(),
        contact: Contact {
            status: row.get::<String, _>("status").parse()?,
            note: row.get("note"),
            called_on: row.get("called_on"),
            not_reached_counter: row.get("not_reached_counter"),
            follow_up_tstamp: row.get("follow_up_tstamp"),
        },
        call_count: row.get("call_count"),
        city_lang: row.get("city_lang"),
    })
}

/// Read the certificate columns out of a row.
///
/// Two readers need this and they must agree, so it lives in one place.
fn tls_from_row(row: &sqlx::sqlite::SqliteRow) -> Tls {
    match row.get::<String, _>("tls").as_str() {
        "valid" => Tls::Valid,
        "broken" => Tls::Broken(
            row.get::<Option<String>, _>("tls_detail")
                .unwrap_or_else(|| "certificate is not trusted".to_owned()),
        ),
        // Anything else, including a row written before this column existed.
        _ => Tls::Unknown,
    }
}

/// Every trade currently on the call list, for the dropdown.
///
/// Built from what actually came back rather than from the hundred we search for —
/// a dropdown offering ninety-six trades with nothing behind them is worse than none.
impl Store {
    pub async fn trades_on_the_list(&self, want: &Wanted) -> Result<Vec<(String, i64)>> {
        let on_the_list = on_the_list();
        // The trade narrowing itself is dropped, or every option would report its
        // own count as the whole list.
        let counting = Wanted {
            trade: None,
            ..want.clone()
        };
        let narrowing = self.narrowing_for(&counting).await?;
        let rows = sqlx::query(&format!(
            // `shown_trade`, not `trade`: `business.trade` is a real column, and
            // SQLite picks the column over the alias in GROUP BY. Naming it the same
            // silently grouped by our searched trade alone and lost every business
            // that only had Google's category.
            "SELECT COALESCE(NULLIF(TRIM(b.trade), ''), b.category) AS shown_trade,
                    COUNT(*) AS n
             {on_the_list}{narrowing}
             GROUP BY shown_trade
             HAVING shown_trade IS NOT NULL AND TRIM(shown_trade) <> ''
             ORDER BY n DESC, shown_trade COLLATE NOCASE"
        ))
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

        Ok(rows
            .into_iter()
            .map(|row| (row.get("shown_trade"), row.get("n")))
            .collect())
    }

    /// How many leads each city has, with everything else the list is narrowed by
    /// still applied.
    ///
    /// The city narrowing itself is dropped, like the trade's above: otherwise every
    /// chip would report the chosen city's total as its own and the row would read
    /// the same whichever one you pressed.
    ///
    /// Cities with nothing on the list are left out. A chip saying "Dubai (0)" that
    /// shows you an empty screen is a question you answer once and resent after that.
    pub async fn cities_on_the_list(&self, want: &Wanted) -> Result<Vec<(i64, String, i64)>> {
        let on_the_list = on_the_list();
        let counting = Wanted {
            city: None,
            ..want.clone()
        };
        let narrowing = self.narrowing_for(&counting).await?;
        let rows = sqlx::query(&format!(
            "SELECT city.id AS city_id, city.name AS city_name, COUNT(*) AS n
             {on_the_list}{narrowing}
               AND b.city_id IS NOT NULL
             GROUP BY city.id
             ORDER BY n DESC, city_name COLLATE NOCASE"
        ))
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

        Ok(rows
            .into_iter()
            .map(|row| (row.get("city_id"), row.get("city_name"), row.get("n")))
            .collect())
    }
}

/// Just enough about one lead to put it on a map.
///
/// A separate, much smaller shape than [`StoredLead`] on purpose: the map wants
/// **every** match, not one page of them, and asking for the full row of three
/// hundred businesses to draw three hundred dots would be wasteful.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Pin {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub phone: Option<String>,
    pub trade: Option<String>,
    /// Why they qualified, as a word.
    pub why: String,
    /// Where you have got to with them.
    pub status: String,
}

/// The extremes of the current list, so a slider can reach both ends of it.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct Extremes {
    /// The busiest business on the list.
    pub most_reviews: i64,
    /// The furthest one, in metres.
    pub furthest_m: i64,
    /// How many have no rating at all — what raising the rating slider would cost.
    pub unrated: i64,
}

impl Store {
    /// Every match, as a pin.
    ///
    /// Deliberately ignores the page: the map showed only the rows loaded, which
    /// meant fifty pins out of three hundred matches with nothing saying so.
    pub async fn pins(&self, want: &Wanted) -> Result<Vec<Pin>> {
        let (on_the_list, status) = (on_the_list(), stored_status());
        let narrowing = self.narrowing_for(want).await?;
        let everything = Wanted {
            limit: 5_000,
            offset: 0,
            ..want.clone()
        };
        let rows = sqlx::query(&format!(
            "SELECT b.id, b.name, b.lat, b.lon, b.phone,
                    COALESCE(NULLIF(TRIM(b.trade), ''), b.category) AS shown_trade,
                    l.verdict,
                    {status} AS status
             {on_the_list}{narrowing}
               AND b.lat IS NOT NULL AND b.lon IS NOT NULL
             LIMIT ?9 OFFSET ?10"
        ))
        .bind(everything.status.map(Status::as_str))
        .bind(everything.search_pattern())
        .bind(everything.every_state)
        .bind(everything.least_reviews)
        .bind(everything.least_rating)
        .bind(everything.within_m)
        .bind(&everything.trade)
        .bind(everything.why.map(Verdict::as_str))
        .bind(everything.limit)
        .bind(everything.offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Pin {
                id: row.get("id"),
                name: row.get("name"),
                lat: row.get("lat"),
                lon: row.get("lon"),
                phone: row.get("phone"),
                trade: row.get("shown_trade"),
                why: row.get("verdict"),
                status: row.get("status"),
            })
            .collect())
    }

    /// How far the sliders need to reach, and what the rating one would hide.
    ///
    /// Taken from the data rather than guessed: a slider that stops at two hundred
    /// reviews is useless on a list whose busiest business has three thousand.
    pub async fn extremes(&self) -> Result<Extremes> {
        let on_the_list = on_the_list();
        // Deliberately unfiltered — a slider's range must not shrink as you drag it,
        // or you can never drag it back.
        let unfiltered = Wanted::default();
        let narrowing = unfiltered.narrowing(None);
        let row = sqlx::query(&format!(
            "SELECT COALESCE(MAX(l.reviews), 0)     AS most_reviews,
                    COALESCE(MAX(l.distance_m), 0)  AS furthest_m,
                    SUM(CASE WHEN l.rating IS NULL THEN 1 ELSE 0 END) AS unrated
             {on_the_list}{narrowing}"
        ))
        .bind(unfiltered.status.map(Status::as_str))
        .bind(unfiltered.search_pattern())
        .bind(unfiltered.every_state)
        .bind(unfiltered.least_reviews)
        .bind(unfiltered.least_rating)
        .bind(unfiltered.within_m)
        .bind(&unfiltered.trade)
        .bind(unfiltered.why.map(Verdict::as_str))
        .fetch_one(&self.pool)
        .await?;

        Ok(Extremes {
            most_reviews: row.get("most_reviews"),
            furthest_m: row.get("furthest_m"),
            unrated: row.get::<Option<i64>, _>("unrated").unwrap_or(0),
        })
    }
}

#[cfg(test)]
#[path = "leads_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "search_tests.rs"]
mod search_tests;

#[cfg(test)]
#[path = "sorting_tests.rs"]
mod sorting_tests;
