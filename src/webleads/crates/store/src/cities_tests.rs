//! Tests for [`crate::cities`].

use super::*;
use crate::testing::*;
use crate::Wanted;

fn dubai() -> City {
    City {
        name: "Dubai".to_owned(),
        country_code: "ae".to_owned(),
        zone: "Asia/Dubai".to_owned(),
        lang: "en".to_owned(),
        centre_lat: 25.2048,
        centre_lon: 55.2708,
    }
}

#[tokio::test]
async fn the_schema_arrives_with_budapest_already_in_it() {
    // Everything on the list today was found in a Budapest district, so the city has
    // to exist before the first business can point at it.
    let store = Store::in_memory().await.expect("opens");
    let cities = store.cities().await.expect("reads");
    assert_eq!(cities.len(), 1);
    assert_eq!(cities[0].city.name, "Budapest");
    assert_eq!(cities[0].city.country_code, "hu");
    assert_eq!(cities[0].city.zone, "Europe/Budapest");
    assert_eq!(cities[0].city.lang, "hu");
}

#[tokio::test]
async fn budapest_is_centred_where_distances_have_always_been_measured_from() {
    // If this moves, every distance on the list changes and nobody asked it to.
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    assert!((budapest.city.centre_lat - 47.514900).abs() < 0.000_001);
    assert!((budapest.city.centre_lon - 19.077800).abs() < 0.000_001);
}

#[tokio::test]
async fn a_new_city_comes_back_with_what_was_stored() {
    let store = Store::in_memory().await.expect("opens");
    let id = store.add_city(&dubai()).await.expect("adds");
    let back = store.city(id).await.expect("reads").expect("there");
    assert_eq!(back.city, dubai());
}

#[tokio::test]
async fn a_city_is_found_by_name_whatever_the_capitals() {
    // The address lookup decides the capitals, not us.
    let store = Store::in_memory().await.expect("opens");
    store.add_city(&dubai()).await.expect("adds");
    assert!(store.city_named("dubai").await.expect("reads").is_some());
    assert!(store
        .city_named("  DUBAI  ")
        .await
        .expect("reads")
        .is_some());
    assert!(store.city_named("Dubaj").await.expect("reads").is_none());
}

#[tokio::test]
async fn the_same_city_twice_is_refused_rather_than_kept_twice() {
    // Two cities of one name would split its leads between them for ever.
    let store = Store::in_memory().await.expect("opens");
    store.add_city(&dubai()).await.expect("adds");
    let again = store.add_city(&dubai()).await;
    assert!(matches!(again, Err(Error::Refused(_))), "{again:?}");
}

#[tokio::test]
async fn a_city_with_no_name_is_refused() {
    let store = Store::in_memory().await.expect("opens");
    let nameless = City {
        name: "   ".to_owned(),
        ..dubai()
    };
    assert!(matches!(
        store.add_city(&nameless).await,
        Err(Error::Refused(_))
    ));
}

#[tokio::test]
async fn a_clock_nobody_keeps_is_refused_when_it_is_stored_not_when_it_is_read() {
    // Stored happily, it would fail every time an opening hour was read — a long way
    // from the mistake, with nothing to say what caused it.
    let store = Store::in_memory().await.expect("opens");
    let wrong = City {
        zone: "Asia/Dubay".to_owned(),
        ..dubai()
    };
    let refused = store.add_city(&wrong).await;
    assert!(matches!(refused, Err(Error::Refused(_))), "{refused:?}");
}

#[tokio::test]
async fn the_country_and_the_language_are_kept_in_lower_case() {
    // Our own country table is keyed by two lower-case letters.
    let store = Store::in_memory().await.expect("opens");
    let shouting = City {
        country_code: "AE".to_owned(),
        lang: "EN".to_owned(),
        ..dubai()
    };
    let id = store.add_city(&shouting).await.expect("adds");
    let back = store.city(id).await.expect("reads").expect("there");
    assert_eq!(back.city.country_code, "ae");
    assert_eq!(back.city.lang, "en");
}

#[tokio::test]
async fn every_area_the_schema_arrives_with_is_already_in_budapest() {
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    for area in store.areas().await.expect("reads") {
        let city = store.city_of_area(area.id).await.expect("reads");
        assert_eq!(
            city.map(|c| c.id),
            Some(budapest.id),
            "{} has no city",
            area.area.label
        );
    }
}

#[tokio::test]
async fn an_area_can_be_moved_to_another_city() {
    let store = Store::in_memory().await.expect("opens");
    let dubai_id = store.add_city(&dubai()).await.expect("adds");
    let area = store.areas().await.expect("reads").remove(0);
    store
        .area_belongs_to(area.id, dubai_id)
        .await
        .expect("moves");
    let city = store
        .city_of_area(area.id)
        .await
        .expect("reads")
        .expect("there");
    assert_eq!(city.id, dubai_id);
}

#[tokio::test]
async fn forgetting_a_city_keeps_the_businesses_that_were_in_it() {
    // A business is a business whether or not we still keep a record of the town.
    let store = Store::in_memory().await.expect("opens");
    let run = store.begin_run("seed.csv").await.expect("run");
    store
        .record(run, &[found(a_lead("one", 3, 100))])
        .await
        .expect("records");

    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    store.remove_city(budapest.id).await.expect("forgets");

    assert!(store.city_named("Budapest").await.expect("reads").is_none());
    assert!(store.lead("one").await.expect("reads").is_some());
    for area in store.areas().await.expect("reads") {
        assert_eq!(store.city_of_area(area.id).await.expect("reads"), None);
    }
}

#[tokio::test]
async fn a_city_that_was_never_added_is_nothing_rather_than_an_error() {
    let store = Store::in_memory().await.expect("opens");
    assert_eq!(store.city(9_999).await.expect("reads"), None);
    assert_eq!(store.city_of_area(9_999).await.expect("reads"), None);
}

#[tokio::test]
async fn a_scraped_business_is_filed_under_the_city_it_was_found_in() {
    // The whole point of the column. The run takes its city from the area, and the
    // business takes it from the run — not from the area directly, because an area
    // can be forgotten while its searches remain.
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    let area = store.areas().await.expect("reads").remove(0);

    let run = store
        .begin_scrape(
            &area.area,
            &[("fodrászat Budapest".to_owned(), Some("Barbers".to_owned()))],
            &webleads_domain::ScrapeOptions::around(&area.area),
        )
        .await
        .expect("starts");
    store
        .record(run, &[found(a_lead("one", 3, 100))])
        .await
        .expect("records");

    assert_eq!(
        store.city_of_business("one").await.expect("reads"),
        Some(budapest.id)
    );
}

#[tokio::test]
async fn a_business_keeps_the_first_city_that_found_it() {
    // Like its first-seen date. Being found again from somewhere else does not move
    // a shop to another town.
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    let dubai_id = store.add_city(&dubai()).await.expect("adds");
    let area = store.areas().await.expect("reads").remove(0);
    let options = webleads_domain::ScrapeOptions::around(&area.area);

    let first = store
        .begin_scrape(&area.area, &[("a".to_owned(), None)], &options)
        .await
        .expect("starts");
    store
        .record(first, &[found(a_lead("one", 3, 100))])
        .await
        .expect("records");

    // Now the same area belongs to Dubai, and the same shop is found again.
    store
        .area_belongs_to(area.id, dubai_id)
        .await
        .expect("moves");
    let second = store
        .begin_scrape(&area.area, &[("a".to_owned(), None)], &options)
        .await
        .expect("starts");
    store
        .record(second, &[found(a_lead("one", 3, 100))])
        .await
        .expect("records");

    assert_eq!(
        store.city_of_business("one").await.expect("reads"),
        Some(budapest.id)
    );
}

/// A list with two cities on it: two businesses in Budapest, one in Dubai.
async fn a_list_in_two_cities() -> (Store, i64, i64) {
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    let dubai_id = store.add_city(&dubai()).await.expect("adds");
    let area = store.areas().await.expect("reads").remove(0);
    let options = webleads_domain::ScrapeOptions::around(&area.area);

    let here = store
        .begin_scrape(&area.area, &[("a".to_owned(), None)], &options)
        .await
        .expect("starts");
    store
        .record(
            here,
            &[
                found(a_lead("pest-one", 3, 100)),
                found(a_lead("pest-two", 4, 200)),
            ],
        )
        .await
        .expect("records");

    store
        .area_belongs_to(area.id, dubai_id)
        .await
        .expect("moves");
    let there = store
        .begin_scrape(&area.area, &[("b".to_owned(), None)], &options)
        .await
        .expect("starts");
    store
        .record(there, &[found(a_lead("dubai-one", 5, 300))])
        .await
        .expect("records");

    (store, budapest.id, dubai_id)
}

#[tokio::test]
async fn narrowing_to_a_city_shows_only_its_businesses() {
    let (store, budapest, dubai_id) = a_list_in_two_cities().await;

    let here = store
        .leads(&Wanted {
            city: Some(budapest),
            ..Default::default()
        })
        .await
        .expect("reads");
    let names: Vec<&str> = here.iter().map(|l| l.lead.id.as_str()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"pest-one") && names.contains(&"pest-two"));

    let there = store
        .leads(&Wanted {
            city: Some(dubai_id),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert_eq!(there.len(), 1);
    assert_eq!(there[0].lead.id, "dubai-one");
}

#[tokio::test]
async fn asking_for_no_city_shows_every_city() {
    let (store, _, _) = a_list_in_two_cities().await;
    let all = store.leads(&Wanted::default()).await.expect("reads");
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn the_count_follows_the_city_as_well_as_the_list() {
    // The number beside "showing 2 of 3" has to agree with the rows, or the page
    // offers a "load more" that fetches nothing.
    let (store, budapest, _) = a_list_in_two_cities().await;
    let want = Wanted {
        city: Some(budapest),
        ..Default::default()
    };
    assert_eq!(store.lead_count(&want).await.expect("counts"), 2);
}

#[tokio::test]
async fn each_chip_counts_its_own_city_not_the_chosen_one() {
    // The narrowing has to be dropped for this count, or pressing Budapest would
    // make every chip read "2".
    let (store, budapest, dubai_id) = a_list_in_two_cities().await;
    let counts = store
        .cities_on_the_list(&Wanted {
            city: Some(budapest),
            ..Default::default()
        })
        .await
        .expect("counts");

    let budapest_count = counts
        .iter()
        .find(|(id, ..)| *id == budapest)
        .map(|(.., n)| *n);
    let dubai_count = counts
        .iter()
        .find(|(id, ..)| *id == dubai_id)
        .map(|(.., n)| *n);
    assert_eq!(budapest_count, Some(2));
    assert_eq!(dubai_count, Some(1));
}

#[tokio::test]
async fn a_chip_carries_the_citys_name_for_reading() {
    let (store, budapest, _) = a_list_in_two_cities().await;
    let counts = store
        .cities_on_the_list(&Wanted::default())
        .await
        .expect("counts");
    let named = counts
        .iter()
        .find(|(id, ..)| *id == budapest)
        .expect("there");
    assert_eq!(named.1, "Budapest");
}

#[tokio::test]
async fn a_city_with_nothing_on_the_list_gets_no_chip() {
    // A chip reading "(0)" that shows an empty screen is a question you answer once.
    let store = Store::in_memory().await.expect("opens");
    store.add_city(&dubai()).await.expect("adds");
    let counts = store
        .cities_on_the_list(&Wanted::default())
        .await
        .expect("counts");
    assert!(counts.is_empty(), "{counts:?}");
}

#[tokio::test]
async fn the_other_narrowings_still_apply_inside_a_city() {
    // City, then search, then state — each one narrows what the one above it left.
    let (store, budapest, _) = a_list_in_two_cities().await;
    let narrowed = store
        .leads(&Wanted {
            city: Some(budapest),
            least_reviews: Some(4),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert_eq!(narrowed.len(), 1);
    assert_eq!(narrowed[0].lead.id, "pest-two");
}

#[tokio::test]
async fn a_city_that_does_not_exist_shows_nothing_rather_than_everything() {
    // Quietly ignoring an id we do not know would show the whole list under a chip
    // that says otherwise.
    let (store, _, _) = a_list_in_two_cities().await;
    let none = store
        .leads(&Wanted {
            city: Some(9_999),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert!(none.is_empty());
}

/// A business standing at a given point, recorded through the real scrape path so it
/// gets a city the way a real one does.
async fn a_business_at(store: &Store, id: &str, lat: f64, lon: f64, was_m: u32) {
    let area = store.areas().await.expect("reads").remove(0);
    let run = store
        .begin_scrape(
            &area.area,
            &[("a".to_owned(), None)],
            &webleads_domain::ScrapeOptions::around(&area.area),
        )
        .await
        .expect("starts");
    let mut lead = a_lead(id, 3, was_m);
    lead.position = Some((lat, lon));
    let mut item = found(lead);
    item.lat = lat;
    item.lon = lon;
    store.record(run, &[item]).await.expect("records");
}

#[tokio::test]
async fn remeasuring_puts_a_lead_at_its_own_citys_centre_at_nought() {
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    // Standing exactly on the centre, but recorded with a distance of nine kilometres.
    a_business_at(
        &store,
        "centre",
        budapest.city.centre_lat,
        budapest.city.centre_lon,
        9_999,
    )
    .await;

    let counted = store
        .remeasure_city(budapest.id, &budapest.city)
        .await
        .expect("remeasures");
    assert_eq!(counted.considered, 1);
    assert_eq!(counted.moved, 1, "it was 9999 m and should now be nought");

    let lead = store.lead("centre").await.expect("reads").expect("there");
    assert_eq!(lead.lead.distance_m, 0);
}

#[tokio::test]
async fn remeasuring_a_second_time_finds_nothing_left_to_change() {
    // The claim being made about Budapest — that not one number moves — checked
    // rather than trusted.
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    a_business_at(&store, "one", 47.54, 19.04, 100).await;

    store
        .remeasure_city(budapest.id, &budapest.city)
        .await
        .expect("once");
    let again = store
        .remeasure_city(budapest.id, &budapest.city)
        .await
        .expect("twice");
    assert_eq!(again.moved, 0);
    assert_eq!(again.biggest_change_m, 0);
}

#[tokio::test]
async fn a_business_in_its_own_city_reads_kilometres_not_thousands_of_them() {
    // The whole point. Measured from Budapest a Dubai shop reads four thousand
    // kilometres, and the "within" slider is useless there.
    let store = Store::in_memory().await.expect("opens");
    let dubai_id = store.add_city(&dubai()).await.expect("adds");
    let area = store.areas().await.expect("reads").remove(0);
    store
        .area_belongs_to(area.id, dubai_id)
        .await
        .expect("moves");
    // A shop in Downtown Dubai, recorded as though it were measured from Budapest.
    a_business_at(&store, "there", 25.1948, 55.2782, 4_019_000).await;

    let city = store.city(dubai_id).await.expect("reads").expect("there");
    store
        .remeasure_city(dubai_id, &city.city)
        .await
        .expect("remeasures");

    let lead = store.lead("there").await.expect("reads").expect("there");
    assert!(
        lead.lead.distance_m < 5_000,
        "a few kilometres, not a few thousand: {}",
        lead.lead.distance_m
    );
}

#[tokio::test]
async fn a_city_with_nothing_in_it_has_nothing_to_remeasure() {
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    let counted = store
        .remeasure_city(budapest.id, &budapest.city)
        .await
        .expect("remeasures");
    assert_eq!(counted.considered, 0);
}

#[tokio::test]
async fn leads_belonging_to_no_city_are_counted_not_measured() {
    // `begin_run` has no area, so anything it records has no city — exactly the shape
    // of the rows from before cities existed. They are said out loud rather than
    // measured against a town we would have had to guess.
    let store = Store::in_memory().await.expect("opens");
    let run = store.begin_run("seed.csv").await.expect("run");
    store
        .record(run, &[found(a_lead("one", 3, 100))])
        .await
        .expect("records");
    assert_eq!(store.leads_with_no_city().await.expect("counts"), 1);
}

#[tokio::test]
async fn a_foreign_shops_hours_read_on_your_own_clock() {
    // The whole reason for the conversion: Google writes them in the shop's time,
    // which is the one clock you cannot dial by.
    let store = Store::in_memory().await.expect("opens");
    let dubai_id = store.add_city(&dubai()).await.expect("adds");
    let area = store.areas().await.expect("reads").remove(0);
    store
        .area_belongs_to(area.id, dubai_id)
        .await
        .expect("moves");

    let run = store
        .begin_scrape(
            &area.area,
            &[("a".to_owned(), None)],
            &webleads_domain::ScrapeOptions::around(&area.area),
        )
        .await
        .expect("starts");
    let mut item = found(a_lead("shop", 3, 100));
    item.open_hours = Some(
        r#"{"Monday":["10 am-11 pm"],"Tuesday":["10 am-11 pm"],"Wednesday":["10 am-11 pm"],
            "Thursday":["10 am-11 pm"],"Friday":["10 am-11 pm"],"Saturday":["10 am-11 pm"],
            "Sunday":["10 am-11 pm"]}"#
            .to_owned(),
    );
    store.record(run, &[item]).await.expect("records");

    let back = store.lead("shop").await.expect("reads").expect("there");
    // Dubai is two hours ahead in summer and three in winter, so ten in the morning
    // there is seven or eight here — never ten.
    let ours = back.hours_today.expect("today");
    assert!(
        ours.starts_with("07:00") || ours.starts_with("08:00"),
        "{ours} was not moved onto your clock"
    );
    // And the shop's own time is kept, for when you are talking to them.
    assert_eq!(back.hours_there.as_deref(), Some("10 am-11 pm"));
}

#[tokio::test]
async fn a_shop_in_your_own_city_is_not_converted_and_says_nothing_about_it() {
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
    let mut item = found(a_lead("shop", 3, 100));
    item.open_hours = Some(
        r#"{"Monday":["9:00-17:00"],"Tuesday":["9:00-17:00"],"Wednesday":["9:00-17:00"],
            "Thursday":["9:00-17:00"],"Friday":["9:00-17:00"],"Saturday":["9:00-17:00"],
            "Sunday":["9:00-17:00"]}"#
            .to_owned(),
    );
    store.record(run, &[item]).await.expect("records");

    let back = store.lead("shop").await.expect("reads").expect("there");
    // Tidied into one shape, but not moved — nine in the morning here is nine.
    assert_eq!(back.hours_today.as_deref(), Some("09:00\u{2013}17:00"));
    assert_eq!(back.hours_there, None, "the same time twice would be noise");
}

#[tokio::test]
async fn the_clocks_we_offer_the_sort_leave_out_your_own_city() {
    // A city on your clock needs no arm in the expression, and Budapest is yours.
    let store = Store::in_memory().await.expect("opens");
    assert!(store.minutes_ahead().await.expect("reads").is_empty());

    let dubai_id = store.add_city(&dubai()).await.expect("adds");
    let ahead = store.minutes_ahead().await.expect("reads");
    assert_eq!(ahead.len(), 1);
    assert_eq!(ahead[0].0, dubai_id);
    assert!(
        ahead[0].1 == 120 || ahead[0].1 == 180,
        "Dubai is two or three hours ahead, not {}",
        ahead[0].1
    );
}

#[tokio::test]
async fn the_city_you_live_in_is_the_one_whose_clock_is_yours() {
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    store.add_city(&dubai()).await.expect("adds");

    // Dubai is the newer city and holds plenty of leads; it is still not home.
    assert_eq!(store.home_city().await.expect("reads"), Some(budapest.id));
}

#[tokio::test]
async fn no_city_keeps_your_clock_and_none_is_claimed_to() {
    let store = Store::in_memory().await.expect("opens");
    sqlx::query("UPDATE city SET zone = 'Asia/Dubai'")
        .execute(&store.pool)
        .await
        .expect("moves the only city");
    assert_eq!(store.home_city().await.expect("reads"), None);
}

#[tokio::test]
async fn budapest_starts_on_a_public_square() {
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    assert_eq!(
        budapest.centre_label.as_deref(),
        Some("Hősök tere, Budapest")
    );
}

#[tokio::test]
async fn moving_the_centre_measures_every_lead_from_the_new_spot() {
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    // A shop at Keleti station, a couple of kilometres from Hősök tere.
    a_business_at(&store, "keleti", 47.5003, 19.0839, 1_700).await;

    let (moved, counted) = store
        .move_centre(budapest.id, "  Keleti, Budapest ", 47.5003, 19.0839)
        .await
        .expect("moves");
    assert_eq!(counted.moved, 1);
    assert_eq!(moved.centre_label.as_deref(), Some("Keleti, Budapest"));

    let lead = store.lead("keleti").await.expect("reads").expect("there");
    assert_eq!(lead.lead.distance_m, 0, "it now stands on the centre");

    let back = store
        .city(budapest.id)
        .await
        .expect("reads")
        .expect("there");
    assert_eq!(back, moved, "what came back is what was stored");
}

#[tokio::test]
async fn a_bad_new_centre_moves_nothing() {
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    assert!(matches!(
        store.move_centre(budapest.id, "", 47.5, 19.0).await,
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        store.move_centre(9_999, "nowhere", 47.5, 19.0).await,
        Err(Error::Refused(_))
    ));
    let still = store
        .city(budapest.id)
        .await
        .expect("reads")
        .expect("there");
    assert_eq!(still, budapest);
}

fn london_area() -> webleads_domain::Area {
    webleads_domain::Area {
        label: "London".to_owned(),
        address: "London, England, United Kingdom".to_owned(),
        lat: 51.5074,
        lon: -0.1278,
        max_results: 200,
    }
}

fn london() -> City {
    City {
        name: "London".to_owned(),
        country_code: "gb".to_owned(),
        zone: "Europe/London".to_owned(),
        lang: "en".to_owned(),
        centre_lat: 51.5074,
        centre_lon: -0.1278,
    }
}

#[tokio::test]
async fn a_new_area_and_its_new_city_are_kept_together() {
    let store = Store::in_memory().await.expect("opens");
    let city = london();
    let id = store
        .add_area_in(
            &london_area(),
            crate::InCity::New {
                city: &city,
                centre_label: "London, England",
            },
        )
        .await
        .expect("adds");
    let kept = store
        .city_of_area(id)
        .await
        .expect("reads")
        .expect("in a city");
    assert_eq!(kept.city.zone, "Europe/London");
    let area = store.area(id).await.expect("reads").expect("there");
    assert_eq!(area.city_id, Some(kept.id));
    assert_eq!(kept.centre_label.as_deref(), Some("London, England"));
}

#[tokio::test]
async fn a_refused_city_leaves_no_area_behind() {
    // The area used to be written first, so a refused city left an invisible area
    // that made the next try say "there is already an area called London".
    let store = Store::in_memory().await.expect("opens");
    let wrong = City {
        zone: "Europe/Nowhere".to_owned(),
        ..london()
    };
    let before = store.areas().await.expect("reads").len();
    let refused = store
        .add_area_in(
            &london_area(),
            crate::InCity::New {
                city: &wrong,
                centre_label: "London",
            },
        )
        .await;
    assert!(matches!(refused, Err(Error::Refused(_))));
    assert_eq!(store.areas().await.expect("reads").len(), before);
    assert!(store.city_named("London").await.expect("reads").is_none());
}

#[tokio::test]
async fn a_taken_area_name_makes_no_city_either() {
    let store = Store::in_memory().await.expect("opens");
    let city = london();
    let taken = webleads_domain::Area {
        label: "Budapest".to_owned(),
        ..london_area()
    };
    let refused = store
        .add_area_in(
            &taken,
            crate::InCity::New {
                city: &city,
                centre_label: "London",
            },
        )
        .await;
    assert!(matches!(refused, Err(Error::Refused(_))));
    assert!(store.city_named("London").await.expect("reads").is_none());
}

#[tokio::test]
async fn a_starting_point_in_another_country_is_refused() {
    let store = Store::in_memory().await.expect("opens");
    let budapest = store
        .city_named("Budapest")
        .await
        .expect("reads")
        .expect("there");
    let refused = store
        .move_centre(budapest.id, "London", 51.5074, -0.1278)
        .await;
    assert!(matches!(refused, Err(Error::Refused(_))));
    let still = store
        .city(budapest.id)
        .await
        .expect("reads")
        .expect("there");
    assert_eq!(still.centre_label, budapest.centre_label);
}
