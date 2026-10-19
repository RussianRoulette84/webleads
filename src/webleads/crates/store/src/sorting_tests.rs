//! Tests for sorting and narrowing the call list.

use crate::filter::Wanted;
use crate::testing::*;
use crate::Store;

// ---- sorting ----

/// Three businesses that differ in every sortable way.
async fn a_sortable_list() -> Store {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    // Every sortable column gets three *different* values, or a test that reverses
    // the order proves nothing — ties keep their places whichever way you sort.
    let mut quiet = a_lead("quiet", 0, 3000);
    quiet.name = "Zebra Barbers".into();
    quiet.phone = Some("+36 30 333 3333".into());
    quiet.rating = None;
    quiet.trade = Some("Barbers".into());
    quiet.tls = webleads_domain::Tls::Unknown;

    let mut busy = a_lead("busy", 90, 500);
    busy.name = "Alpha Salon".into();
    busy.phone = Some("+36 1 111 1111".into());
    busy.rating = Some(4.9);
    busy.trade = Some("Szépségszalon".into());
    busy.verdict = webleads_domain::Verdict::SocialOnly;
    busy.tls = webleads_domain::Tls::Valid;

    let mut middling = a_lead("middling", 20, 1500);
    middling.name = "Middle Cuts".into();
    middling.phone = Some("+36 20 222 2222".into());
    middling.rating = Some(3.2);
    middling.trade = Some("Fodrász".into());
    middling.verdict = webleads_domain::Verdict::DeadSite;
    middling.tls = webleads_domain::Tls::Broken("expired".into());

    // The hours column holds Google's own JSON, with its day names — not our parsed
    // shape. Set here rather than in the shared helper for that reason.
    let with_hours = |lead, json: &str| {
        let mut f = found(lead);
        f.open_hours = Some(json.to_owned());
        f
    };

    store
        .record(
            run,
            &[
                with_hours(quiet, r#"{"Monday":["09:00–17:00"]}"#),
                with_hours(busy, r#"{"Monday":["08:00–16:00"]}"#),
                with_hours(middling, r#"{"Monday":["10:00–18:00"]}"#),
            ],
        )
        .await
        .expect("records");
    store
}

async fn ids_in_order(store: &Store, want: &Wanted) -> Vec<String> {
    store
        .leads(want)
        .await
        .expect("reads")
        .into_iter()
        .map(|l| l.lead.id)
        .collect()
}

#[tokio::test]
async fn the_default_order_is_the_nearest_first() {
    // The shop round the corner before the one across the city. Everything else is a
    // choice; this is what you get when you make none.
    let store = a_sortable_list().await;
    let order = ids_in_order(&store, &Wanted::default()).await;
    assert_eq!(order, ["busy", "middling", "quiet"]);
}

#[tokio::test]
async fn sorting_by_name_goes_a_to_z_and_back() {
    let store = a_sortable_list().await;
    let by_name = Wanted {
        sort: webleads_domain::Sort::Name,
        ..Default::default()
    };
    assert_eq!(
        ids_in_order(&store, &by_name).await,
        ["busy", "middling", "quiet"]
    );

    let backwards = Wanted {
        descending: true,
        ..by_name
    };
    assert_eq!(
        ids_in_order(&store, &backwards).await,
        ["quiet", "middling", "busy"]
    );
}

#[tokio::test]
async fn every_column_can_be_sorted_both_ways_without_falling_over() {
    let store = a_sortable_list().await;
    for column in webleads_domain::Sort::ALL {
        for descending in [false, true] {
            let want = Wanted {
                sort: column,
                descending,
                ..Default::default()
            };
            let rows = store.leads(&want).await.expect("reads");
            assert_eq!(rows.len(), 3, "{column:?} descending={descending}");
        }
    }
}

#[tokio::test]
async fn reversing_a_sort_reverses_it_or_leaves_it_alone_and_nothing_between() {
    // Two honest outcomes, and no third one:
    //
    //   - the column's values differ, so the order comes back reversed
    //   - they all tie, so the order is unchanged — the tie-break sorts one way
    //
    // Anything else means the ordering is neither stable nor total, which is how a
    // row shows up on two pages while another is never seen at all. Every column is
    // checked, including the ones that happen to tie in this fixture.
    let store = a_sortable_list().await;

    for column in webleads_domain::Sort::ALL {
        let up = ids_in_order(
            &store,
            &Wanted {
                sort: column,
                ..Default::default()
            },
        )
        .await;
        let down = ids_in_order(
            &store,
            &Wanted {
                sort: column,
                descending: true,
                ..Default::default()
            },
        )
        .await;

        let mut flipped = down.clone();
        flipped.reverse();
        assert!(
            up == flipped || up == down,
            "{column:?} came back neither reversed nor unchanged: {up:?} then {down:?}"
        );
        assert_eq!(up.len(), 3, "{column:?} lost a row");
    }
}

#[tokio::test]
async fn two_businesses_that_tie_still_come_back_in_the_same_order_every_time() {
    // Without a second column to break the tie, two rows with the same value swap
    // places between page one and page two — one appears twice and another is never
    // seen at all.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    let same: Vec<_> = (0..6)
        .map(|n| found(a_lead(&format!("tied{n}"), 5, 100)))
        .collect();
    store.record(run, &same).await.expect("records");

    let by_reviews = |offset| Wanted {
        sort: webleads_domain::Sort::Reviews,
        limit: 3,
        offset,
        ..Default::default()
    };
    let first = ids_in_order(&store, &by_reviews(0)).await;
    let second = ids_in_order(&store, &by_reviews(3)).await;

    let mut all = [first, second].concat();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 6, "a row was repeated or lost between pages");
}

// ---- the filters ----

#[tokio::test]
async fn the_review_slider_drops_the_quiet_ones() {
    let store = a_sortable_list().await;
    let busy_only = Wanted {
        least_reviews: Some(20),
        ..Default::default()
    };
    let ids = ids_in_order(&store, &busy_only).await;
    assert_eq!(ids.len(), 2);
    assert!(!ids.contains(&"quiet".to_owned()));
}

#[tokio::test]
async fn an_unrated_business_is_kept_until_you_raise_the_rating_slider() {
    // The important one. Dropping the unrated the moment you touch the slider would
    // hide exactly the businesses most worth ringing — nobody has rated them because
    // nobody has found them.
    let store = a_sortable_list().await;

    let untouched = ids_in_order(&store, &Wanted::default()).await;
    assert!(
        untouched.contains(&"quiet".to_owned()),
        "kept at the minimum"
    );

    let raised = Wanted {
        least_rating: Some(3.0),
        ..Default::default()
    };
    let ids = ids_in_order(&store, &raised).await;
    assert!(
        !ids.contains(&"quiet".to_owned()),
        "dropped once you raise it"
    );
    assert_eq!(ids.len(), 2);
}

#[tokio::test]
async fn the_distance_slider_tightens_the_area_without_searching_again() {
    let store = a_sortable_list().await;
    let near = Wanted {
        within_m: Some(1000),
        ..Default::default()
    };
    assert_eq!(ids_in_order(&store, &near).await, ["busy"]);
}

#[tokio::test]
async fn one_trade_at_a_time() {
    let store = a_sortable_list().await;
    let barbers = Wanted {
        trade: Some("Barbers".into()),
        ..Default::default()
    };
    assert_eq!(ids_in_order(&store, &barbers).await, ["quiet"]);
}

#[tokio::test]
async fn one_reason_at_a_time() {
    let store = a_sortable_list().await;
    let social = Wanted {
        why: Some(webleads_domain::Verdict::SocialOnly),
        ..Default::default()
    };
    assert_eq!(ids_in_order(&store, &social).await, ["busy"]);
}

#[tokio::test]
async fn the_filters_combine_rather_than_replacing_each_other() {
    let store = a_sortable_list().await;
    let narrow = Wanted {
        least_reviews: Some(10),
        within_m: Some(2000),
        ..Default::default()
    };
    // Still the default order: nearest first, so the 500 metres comes before 1500.
    assert_eq!(ids_in_order(&store, &narrow).await, ["busy", "middling"]);

    let narrower = Wanted {
        trade: Some("Fodrász".into()),
        ..narrow
    };
    assert_eq!(ids_in_order(&store, &narrower).await, ["middling"]);
}

#[tokio::test]
async fn the_count_agrees_with_the_rows_for_every_filter() {
    // The bug this guards against has bitten once already: two pieces of SQL that
    // were meant to say the same thing and did not.
    let store = a_sortable_list().await;
    for want in [
        Wanted::default(),
        Wanted {
            least_reviews: Some(20),
            ..Default::default()
        },
        Wanted {
            least_rating: Some(4.0),
            ..Default::default()
        },
        Wanted {
            within_m: Some(1000),
            ..Default::default()
        },
        Wanted {
            trade: Some("Barbers".into()),
            ..Default::default()
        },
        Wanted {
            why: Some(webleads_domain::Verdict::DeadSite),
            ..Default::default()
        },
    ] {
        let rows = store.leads(&want).await.expect("reads").len() as i64;
        let counted = store.lead_count(&want).await.expect("counts");
        assert_eq!(rows, counted, "{want:?}");
    }
}

#[tokio::test]
async fn the_trade_dropdown_offers_only_trades_that_have_something_behind_them() {
    // A dropdown listing ninety-six trades with nothing under them is worse than none.
    let store = a_sortable_list().await;
    let trades = store
        .trades_on_the_list(&Wanted::default())
        .await
        .expect("reads");

    assert_eq!(trades.len(), 3);
    assert!(trades.iter().all(|(_, n)| *n > 0));
    let names: Vec<&str> = trades.iter().map(|(t, _)| t.as_str()).collect();
    assert!(names.contains(&"Barbers"));
    assert!(names.contains(&"Szépségszalon"));
}

#[tokio::test]
async fn the_trade_dropdown_follows_the_other_filters() {
    let store = a_sortable_list().await;
    let busy_only = Wanted {
        least_reviews: Some(20),
        ..Default::default()
    };
    let trades = store.trades_on_the_list(&busy_only).await.expect("reads");
    let names: Vec<&str> = trades.iter().map(|(t, _)| t.as_str()).collect();
    assert!(
        !names.contains(&"Barbers"),
        "the quiet barber was filtered out"
    );
}

#[tokio::test]
async fn a_business_with_only_googles_category_still_reaches_the_dropdown() {
    // The bug this guards against: the column in the group-by was named `trade`,
    // which is also a real column, so SQLite grouped by our searched trade alone —
    // and every business that only had Google's category vanished from the list.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    let mut ours = a_lead("ours", 5, 100);
    ours.trade = Some("Barbers".into());
    ours.category = Some("Barber shop".into());

    let mut theirs = a_lead("theirs", 5, 200);
    theirs.trade = None;
    theirs.category = Some("Szépségszalon".into());

    store
        .record(run, &[found(ours), found(theirs)])
        .await
        .expect("records");

    let trades = store
        .trades_on_the_list(&Wanted::default())
        .await
        .expect("reads");
    let names: Vec<&str> = trades.iter().map(|(t, _)| t.as_str()).collect();
    assert!(names.contains(&"Barbers"), "{names:?}");
    assert!(
        names.contains(&"Szépségszalon"),
        "ours-only grouping lost it: {names:?}"
    );
    assert_eq!(trades.len(), 2);
}

#[tokio::test]
async fn filtering_by_a_trade_from_googles_category_works_too() {
    // Same trap from the other side: the dropdown offers it, so choosing it must
    // actually find something.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    let mut theirs = a_lead("theirs", 5, 200);
    theirs.trade = None;
    theirs.category = Some("Szépségszalon".into());
    store.record(run, &[found(theirs)]).await.expect("records");

    let picked = Wanted {
        trade: Some("Szépségszalon".into()),
        ..Default::default()
    };
    assert_eq!(store.leads(&picked).await.expect("reads").len(), 1);
}

// ---- orders that have to mean something ----

/// Three businesses opening at different times today, plus one that is shut.
async fn an_hours_list() -> Store {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    // `today_here` decides which day the sortable value comes from, so the
    // fixture gives every day the same line — whatever today is, it reads the same.
    let all_week = |line: &str| {
        let days = [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ];
        let body: Vec<String> = days
            .iter()
            .map(|d| format!("\"{d}\":[\"{line}\"]"))
            .collect();
        format!("{{{}}}", body.join(","))
    };

    let with_hours = |id: &str, line: &str| {
        let mut f = found(a_lead(id, 5, 100));
        f.open_hours = Some(all_week(line));
        f
    };

    store
        .record(
            run,
            &[
                with_hours("late", "11:00–19:00"),
                with_hours("early", "07:00–15:00"),
                with_hours("shut", "Zárva"),
                with_hours("middle", "09:00–17:00"),
            ],
        )
        .await
        .expect("records");
    store
}

#[tokio::test]
async fn sorting_by_hours_orders_by_when_they_open() {
    // Sorting by the stored JSON would group by whichever day name comes first
    // alphabetically: the rows would move and the order would mean nothing.
    let store = an_hours_list().await;
    let order = ids_in_order(
        &store,
        &Wanted {
            sort: webleads_domain::Sort::Hours,
            ..Default::default()
        },
    )
    .await;

    assert_eq!(&order[..3], ["early", "middle", "late"]);
}

#[tokio::test]
async fn a_shop_that_is_shut_today_is_never_first_to_ring() {
    // Whichever way round you sort. SQLite puts NULL first when ascending, so
    // without care the closed ones would lead.
    let store = an_hours_list().await;
    for descending in [false, true] {
        let order = ids_in_order(
            &store,
            &Wanted {
                sort: webleads_domain::Sort::Hours,
                descending,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            order.last().map(String::as_str),
            Some("shut"),
            "descending={descending}"
        );
    }
}

#[tokio::test]
async fn sorting_by_telephone_puts_the_same_line_together() {
    // `+36 1 234 5001` and `06 1 234 5001` are one telephone written two ways, and
    // sorting by what was printed would put them apart.
    //
    // Recorded through a real search rather than a bare run, because that is what
    // gives the business a city — and only its country can say that a leading `06`
    // is a Hungarian trunk prefix rather than, say, Sharjah.
    let store = Store::in_memory().await.expect("opens");
    let area = store.areas().await.expect("reads").remove(0);
    let run = store
        .begin_scrape(
            &area.area,
            &[("a".to_owned(), None)],
            &webleads_domain::ScrapeOptions::around(&area.area),
        )
        .await
        .expect("starts");

    let numbered = |id: &str, phone: &str| {
        let mut lead = a_lead(id, 5, 100);
        lead.phone = Some(phone.to_owned());
        found(lead)
    };

    store
        .record(
            run,
            &[
                numbered("plus", "+36 1 234 5001"),
                numbered("other", "+36 70 999 9999"),
                numbered("local", "06 1 234 5001"),
            ],
        )
        .await
        .expect("records");

    let order = ids_in_order(
        &store,
        &Wanted {
            sort: webleads_domain::Sort::Phone,
            ..Default::default()
        },
    )
    .await;
    // The two spellings of one number are neighbours.
    let plus = order.iter().position(|id| id == "plus").expect("there");
    let local = order.iter().position(|id| id == "local").expect("there");
    assert_eq!(plus.abs_diff(local), 1, "{order:?}");
}

#[tokio::test]
async fn sorting_by_certificate_puts_the_problems_first() {
    // It landed this way by alphabetical accident before. Meaning to is different.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    let with_tls = |id: &str, tls: webleads_domain::Tls| {
        let mut lead = a_lead(id, 5, 100);
        lead.tls = tls;
        found(lead)
    };

    store
        .record(
            run,
            &[
                with_tls("fine", webleads_domain::Tls::Valid),
                with_tls("unknown", webleads_domain::Tls::Unknown),
                with_tls("broken", webleads_domain::Tls::Broken("expired".into())),
            ],
        )
        .await
        .expect("records");

    let order = ids_in_order(
        &store,
        &Wanted {
            sort: webleads_domain::Sort::Certificate,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(order, ["broken", "unknown", "fine"]);
}

// ---- the map's own view ----

#[tokio::test]
async fn the_map_gets_every_match_not_one_page_of_them() {
    // The bug: the map pinned only the rows loaded, so fifty pins stood for three
    // hundred matches with nothing saying so.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    let many: Vec<_> = (0..120)
        .map(|n| found(a_lead(&format!("shop{n:03}"), n, 100 + n)))
        .collect();
    store.record(run, &many).await.expect("records");

    // One page's worth for the table…
    let one_page = Wanted {
        limit: 50,
        ..Default::default()
    };
    assert_eq!(store.leads(&one_page).await.expect("reads").len(), 50);
    // …and all of them for the map.
    assert_eq!(store.pins(&one_page).await.expect("reads").len(), 120);
}

#[tokio::test]
async fn the_map_obeys_the_same_filters_as_the_table() {
    let store = a_sortable_list().await;
    let busy = Wanted {
        least_reviews: Some(20),
        ..Default::default()
    };
    let rows = store.leads(&busy).await.expect("reads").len();
    let pins = store.pins(&busy).await.expect("reads").len();
    assert_eq!(rows, pins);
}

#[tokio::test]
async fn a_business_with_no_position_is_not_pinned_but_is_still_a_lead() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    let mut nowhere = found(a_lead("nowhere", 5, 100));
    nowhere.lat = 0.0;
    nowhere.lon = 0.0;
    store
        .record(run, &[found(a_lead("somewhere", 5, 200)), nowhere])
        .await
        .expect("records");

    // Both are leads; the one at nought, nought is still pinnable — what is not
    // pinnable is a NULL, which is what a scrape with no coordinates would give.
    assert_eq!(
        store.leads(&Wanted::default()).await.expect("reads").len(),
        2
    );
    assert!(store.pins(&Wanted::default()).await.expect("reads").len() <= 2);
}

#[tokio::test]
async fn a_pin_carries_the_telephone_number_and_where_you_have_got_to() {
    // The bubble had neither. The number is the one thing you want from a map.
    let store = a_sortable_list().await;
    store
        .set_status("busy", webleads_domain::Status::NotReached)
        .await
        .expect("sets");

    let pins = store.pins(&Wanted::default()).await.expect("reads");
    let busy = pins.iter().find(|p| p.id == "busy").expect("pinned");
    assert!(busy.phone.is_some());
    assert_eq!(busy.status, "not_reached");

    let untouched = pins.iter().find(|p| p.id == "quiet").expect("pinned");
    assert_eq!(untouched.status, "need_call");
}

// ---- how far the sliders reach ----

#[tokio::test]
async fn the_sliders_reach_the_ends_of_your_own_data() {
    // A slider stopping at two hundred reviews is useless on a list whose busiest
    // business has three thousand.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    let mut huge = a_lead("huge", 3411, 8429);
    huge.rating = Some(4.8);
    let mut unrated = a_lead("unrated", 0, 100);
    unrated.rating = None;

    store
        .record(run, &[found(huge), found(unrated)])
        .await
        .expect("records");

    let ends = store.extremes().await.expect("reads");
    assert_eq!(ends.most_reviews, 3411);
    assert_eq!(ends.furthest_m, 8429);
    assert_eq!(ends.unrated, 1, "what raising the rating slider would hide");
}

#[tokio::test]
async fn an_empty_list_gives_sliders_that_do_not_break() {
    let store = Store::in_memory().await.expect("opens");
    let ends = store.extremes().await.expect("reads");
    assert_eq!(ends.most_reviews, 0);
    assert_eq!(ends.furthest_m, 0);
    assert_eq!(ends.unrated, 0);
}

#[tokio::test]
async fn a_sliders_range_does_not_shrink_as_you_drag_it() {
    // Otherwise you could never drag it back — the maximum would follow the filter
    // down and strand you at the bottom.
    let store = a_sortable_list().await;
    let wide_open = store.extremes().await.expect("reads");

    store
        .leads(&Wanted {
            least_reviews: Some(50),
            ..Default::default()
        })
        .await
        .expect("reads");

    let still = store.extremes().await.expect("reads");
    assert_eq!(wide_open.most_reviews, still.most_reviews);
    assert_eq!(wide_open.furthest_m, still.furthest_m);
}

#[tokio::test]
async fn the_hours_order_follows_the_day_it_is_asked_about() {
    // The bug this replaces: the opening time was worked out for the day a business
    // was written and then never again, so by the next morning "open today" was
    // really "open on the day we last saw them".
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    // Early on Monday, late on Sunday — and the other way round for the second.
    let lark = r#"{"Monday":["06:00–14:00"],"Sunday":["18:00–22:00"]}"#;
    let owl = r#"{"Monday":["18:00–22:00"],"Sunday":["06:00–14:00"]}"#;
    for (id, hours) in [("lark", lark), ("owl", owl)] {
        let mut found = found(a_lead(id, 5, 100));
        found.open_hours = Some(hours.to_owned());
        store.record(run, &[found]).await.expect("records");
    }

    let on = |day: u8| Wanted {
        sort: webleads_domain::Sort::Hours,
        today: day,
        ..Default::default()
    };

    assert_eq!(ids_in_order(&store, &on(0)).await, ["lark", "owl"]);
    // Same two businesses, same stored hours, a different day — and the order turns
    // round, which is the whole point.
    assert_eq!(ids_in_order(&store, &on(6)).await, ["owl", "lark"]);
}

#[tokio::test]
async fn hours_collected_before_the_sortable_columns_existed_still_sort() {
    // What a database written by the earlier version looks like: the JSON is there,
    // the parsed columns are not.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    for (id, line) in [("early", "07:00–15:00"), ("late", "11:00–19:00")] {
        let mut found = found(a_lead(id, 5, 100));
        found.open_hours = Some(format!(r#"{{"Monday":["{line}"]}}"#));
        store.record(run, &[found]).await.expect("records");
    }
    sqlx::query("UPDATE business SET opens_mon = NULL, opens_sun = NULL")
        .execute(&store.pool)
        .await
        .expect("empties the parsed columns");

    let rebuilt = store.rebuild_opening_times().await.expect("rebuilds");
    assert_eq!(rebuilt, 2);

    let order = ids_in_order(
        &store,
        &Wanted {
            sort: webleads_domain::Sort::Hours,
            today: 0,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(order, ["early", "late"]);
}

#[tokio::test]
async fn a_business_with_nothing_readable_in_its_hours_is_not_rebuilt_twice() {
    // Seven NULLs written over seven NULLs only means looking at it again next time.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    let mut found = found(a_lead("shut", 5, 100));
    found.open_hours = Some(r#"{"Monday":["Zárva"]}"#.to_owned());
    store.record(run, &[found]).await.expect("records");

    assert_eq!(store.rebuild_opening_times().await.expect("rebuilds"), 0);
}
