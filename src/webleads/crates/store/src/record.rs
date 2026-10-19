//! Writing a run into the database.

use webleads_domain::Tls;

use crate::{today, Found, Recorded, Result, Store};

impl Store {
    /// Start a run and return its id.
    pub async fn begin_run(&self, source: &str) -> Result<i64> {
        let now = today();
        // `execute` rather than `RETURNING` with `fetch_one`.
        //
        // `fetch_one` reads the first row and drops the rest of the result, which on
        // a statement that writes leaves the connection holding SQLite's write lock.
        // The next write then fails with "database is locked" — which is exactly what
        // happened on every second import. `execute` runs the statement to the end,
        // and `last_insert_rowid` is how SQLite reports the row it has just made.
        let done = sqlx::query("INSERT INTO run (source, started_at) VALUES (?1, ?2)")
            .bind(source)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(done.last_insert_rowid())
    }

    /// Record what we found, for one run.
    ///
    /// The whole batch goes in one transaction: either every row lands or none does.
    /// A half-written run would be worse than no run, because you could not tell.
    ///
    /// The first-seen rule lives in the `ON CONFLICT` clause. When a business is
    /// already known, `first_seen` is simply not in the update list, so it keeps the
    /// date it has always had. That is the whole mechanism.
    pub async fn record(&self, run_id: i64, found: &[Found]) -> Result<Recorded> {
        let mut tx = self.pool.begin().await?;
        let now = today();
        let mut recorded = Recorded::default();

        // Which city these were found in — read from the run rather than passed in,
        // because the run is the thing that knows, and every caller already has one.
        // `None` for a run made before cities existed, which cannot happen after the
        // schema was applied but is not worth failing over.
        let city_id: Option<i64> = sqlx::query_scalar("SELECT city_id FROM run WHERE id = ?1")
            .bind(run_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();

        // And which country that city is in, because the telephone rules are the
        // country's: `06` is a Hungarian trunk prefix and a Sharjah area code, so the
        // same digits mean two different towns. Empty when there is no city, which
        // leaves every number's digits exactly as they arrived — better untidy than
        // filed under the wrong country.
        let country: String = match city_id {
            Some(id) => sqlx::query_scalar("SELECT country_code FROM city WHERE id = ?1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .unwrap_or_default(),
            None => String::new(),
        };

        for item in found {
            let lead = &item.lead;

            // Ask first, then write. "Was this business already in the table?" is a
            // question only the database can answer, and it has to be asked *before*
            // the insert — afterwards the row exists either way.
            //
            // The obvious shortcut — checking whether `first_seen` came back as
            // today — does not work: a second import on the same day would report
            // every business as new. Inside the transaction this extra read is cheap
            // and, unlike the shortcut, it is right.
            let known: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM business WHERE id = ?1")
                .bind(&lead.id)
                .fetch_optional(&mut *tx)
                .await?;

            // `insert` returns false when we met this shop earlier in this same
            // batch, so a file listing one business twice counts it once.
            let first_time_here = recorded.seen_this_batch.insert(lead.id.clone());
            if first_time_here {
                if known.is_some() {
                    recorded.seen_before += 1;
                } else {
                    recorded.new += 1;
                }
            }

            // Worked out once, here, rather than on every read — and it is what lets
            // the database sort by when a shop opens. Seven values, because one for
            // "today" is wrong by tomorrow morning.
            let opens = item
                .open_hours
                .as_deref()
                .map(webleads_domain::Hours::parse)
                .unwrap_or_default()
                .opens_each_day();

            sqlx::query(
                "INSERT INTO business (id, name, phone, phone_canonical, category, address,
                                       maps_url, lat, lon, first_seen, last_seen,
                                       checked_on, trade, found_by, open_hours,
                                       opens_mon, opens_tue, opens_wed, opens_thu,
                                       opens_fri, opens_sat, opens_sun, first_run_id,
                                       city_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?12, ?13, ?14,
                         ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
                 ON CONFLICT(id) DO UPDATE SET
                     name            = excluded.name,
                     phone           = COALESCE(excluded.phone, business.phone),
                     phone_canonical = COALESCE(excluded.phone_canonical, business.phone_canonical),
                     category        = COALESCE(excluded.category, business.category),
                     address         = COALESCE(excluded.address, business.address),
                     maps_url        = COALESCE(excluded.maps_url, business.maps_url),
                     last_seen       = excluded.last_seen,
                     -- Only move the freshness date when we actually looked. A
                     -- business whose verdict came out of the cache must keep its
                     -- old date, or it would stay \"fresh\" forever and never be
                     -- checked again.
                     checked_on      = CASE WHEN ?11 THEN excluded.checked_on
                                            ELSE business.checked_on END,
                     -- The trade we searched for, kept once we know it. A later
                     -- import that does not know it must not blank it out.
                     trade           = COALESCE(excluded.trade, business.trade),
                     found_by        = COALESCE(excluded.found_by, business.found_by),
                     -- Hours we already have are not blanked by a later fast-mode
                     -- scrape that knows none.
                     open_hours      = COALESCE(excluded.open_hours, business.open_hours),
                     opens_mon       = COALESCE(excluded.opens_mon, business.opens_mon),
                     opens_tue       = COALESCE(excluded.opens_tue, business.opens_tue),
                     opens_wed       = COALESCE(excluded.opens_wed, business.opens_wed),
                     opens_thu       = COALESCE(excluded.opens_thu, business.opens_thu),
                     opens_fri       = COALESCE(excluded.opens_fri, business.opens_fri),
                     opens_sat       = COALESCE(excluded.opens_sat, business.opens_sat),
                     opens_sun       = COALESCE(excluded.opens_sun, business.opens_sun)
                     -- `first_run_id` and `city_id` are deliberately absent, like
                     -- `first_seen`: the search that first found a business never
                     -- changes, and nor does the town it was found in.",
            )
            .bind(&lead.id)
            .bind(&lead.name)
            // The tidy form is what gets saved: `06 20 555 0103` goes in as
            // `+36205550103`. Anything with no digits in it is kept exactly as it
            // arrived — mangling an unreadable number does not improve it.
            .bind(
                lead.phone_canonical(&country)
                    .or_else(|| lead.phone.clone()),
            )
            .bind(lead.phone_canonical(&country))
            .bind(&lead.category)
            .bind(&lead.address)
            .bind(&lead.maps_url)
            .bind(item.lat)
            .bind(item.lon)
            .bind(&now)
            .bind(item.freshly_checked)
            .bind(item.freshly_checked.then(|| now.clone()))
            .bind(&item.lead.trade)
            .bind(&item.found_by)
            .bind(&item.open_hours)
            .bind(opens[0].map(i64::from))
            .bind(opens[1].map(i64::from))
            .bind(opens[2].map(i64::from))
            .bind(opens[3].map(i64::from))
            .bind(opens[4].map(i64::from))
            .bind(opens[5].map(i64::from))
            .bind(opens[6].map(i64::from))
            .bind(run_id)
            .bind(city_id)
            .execute(&mut *tx)
            .await?;

            // Anything we knew about this business before is no longer the newest.
            sqlx::query("UPDATE lead SET is_latest = 0 WHERE business_id = ?1")
                .bind(&lead.id)
                .execute(&mut *tx)
                .await?;

            let (tls, tls_detail) = match &lead.tls {
                Tls::Unknown => ("unknown", None),
                Tls::Valid => ("valid", None),
                Tls::Broken(detail) => ("broken", Some(detail.clone())),
            };

            sqlx::query(
                "INSERT INTO lead (business_id, run_id, verdict, reason, website,
                                   social_url, reviews, rating, distance_m,
                                   tls, tls_detail, tls_rank, is_latest)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1)
                 ON CONFLICT(business_id, run_id) DO UPDATE SET
                     verdict    = excluded.verdict,
                     reason     = excluded.reason,
                     tls        = excluded.tls,
                     tls_detail = excluded.tls_detail,
                     tls_rank   = excluded.tls_rank,
                     is_latest  = 1",
            )
            .bind(&lead.id)
            .bind(run_id)
            .bind(lead.verdict.as_str())
            .bind(&lead.reason)
            .bind(&lead.website)
            .bind(&lead.social_url)
            .bind(i64::from(lead.reviews))
            .bind(lead.rating)
            .bind(i64::from(lead.distance_m))
            .bind(tls)
            .bind(tls_detail)
            // How much it matters, not how it is spelled: broken first.
            .bind(match &lead.tls {
                Tls::Broken(_) => 0_i64,
                Tls::Unknown => 1,
                Tls::Valid => 2,
            })
            .execute(&mut *tx)
            .await?;
        }

        // Nothing above is real until this line. If we returned early with `?`, the
        // transaction is dropped and SQLite rolls all of it back.
        tx.commit().await?;
        Ok(recorded)
    }
}

#[cfg(test)]
mod tests {
    use crate::filter::Wanted;
    use crate::testing::*;
    use crate::Store;
    use webleads_domain::Verdict;

    #[tokio::test]
    async fn a_run_stores_its_leads_and_reads_them_back() {
        let store = Store::in_memory().await.expect("opens");
        let run = store.begin_run("seed.csv").await.expect("run");
        let leads = [a_lead("a", 5, 300), a_lead("b", 0, 900)];
        store
            .record(run, &leads.iter().cloned().map(found).collect::<Vec<_>>())
            .await
            .expect("records");

        let back = store
            .leads(&Wanted {
                limit: 100,
                offset: 0,
                ..Default::default()
            })
            .await
            .expect("reads");
        assert_eq!(back.len(), 2);
        // The nearer one comes first, whatever its reviews say.
        assert_eq!(back[0].lead.id, "a");
        assert_eq!(back[0].lead.verdict, Verdict::NoSite);
        assert_eq!(back[1].lead.id, "b");
    }

    #[tokio::test]
    async fn importing_the_same_file_twice_adds_nothing_and_moves_no_date() {
        let store = Store::in_memory().await.expect("opens");
        let leads = [a_lead("a", 1, 100)];

        let first = store.begin_run("seed.csv").await.expect("run");
        store
            .record(first, &leads.iter().cloned().map(found).collect::<Vec<_>>())
            .await
            .expect("records");
        let date_after_first = store.first_seen("a").await.expect("reads");

        let second = store.begin_run("seed.csv").await.expect("run");
        store
            .record(
                second,
                &leads.iter().cloned().map(found).collect::<Vec<_>>(),
            )
            .await
            .expect("records");

        assert_eq!(store.business_count().await.expect("counts"), 1);
        assert_eq!(
            store.first_seen("a").await.expect("reads"),
            date_after_first
        );
        // And the list shows it once, from the newer run — not twice.
        assert_eq!(
            store
                .leads(&Wanted {
                    limit: 100,
                    offset: 0,
                    ..Default::default()
                })
                .await
                .expect("reads")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn a_file_listing_one_shop_twice_counts_it_once() {
        // Two rows for one shop. Counting businesses before and after and subtracting
        // would call it one new and one already known — when there is only one, and it
        // is new.
        let store = Store::in_memory().await.expect("opens");
        let run = store.begin_run("seed.csv").await.expect("run");
        let twice = [found(a_lead("a", 0, 10)), found(a_lead("a", 0, 10))];

        let counted = store.record(run, &twice).await.expect("records");
        assert_eq!(counted.new, 1);
        assert_eq!(counted.seen_before, 0);
        assert_eq!(store.business_count().await.expect("counts"), 1);
    }

    #[tokio::test]
    async fn the_second_import_reports_them_all_as_already_known() {
        let store = Store::in_memory().await.expect("opens");
        let leads = [found(a_lead("a", 0, 10)), found(a_lead("b", 3, 20))];

        let first = store.begin_run("seed.csv").await.expect("run");
        let counted = store.record(first, &leads).await.expect("records");
        assert_eq!((counted.new, counted.seen_before), (2, 0));

        let second = store.begin_run("seed.csv").await.expect("run");
        let counted = store.record(second, &leads).await.expect("records");
        assert_eq!((counted.new, counted.seen_before), (0, 2));
    }

    #[tokio::test]
    async fn a_mix_of_old_and_new_is_counted_exactly() {
        let store = Store::in_memory().await.expect("opens");
        let first = store.begin_run("a.csv").await.expect("run");
        store
            .record(first, &[found(a_lead("known", 0, 10))])
            .await
            .expect("records");

        let second = store.begin_run("b.csv").await.expect("run");
        let counted = store
            .record(
                second,
                &[found(a_lead("known", 0, 10)), found(a_lead("fresh", 0, 20))],
            )
            .await
            .expect("records");
        assert_eq!((counted.new, counted.seen_before), (1, 1));
    }
}
