//! Tests for [`crate::city`].

use super::*;

fn budapest() -> City {
    City {
        name: "Budapest".to_owned(),
        country_code: "hu".to_owned(),
        zone: "Europe/Budapest".to_owned(),
        lang: "hu".to_owned(),
        centre_lat: 47.514900,
        centre_lon: 19.077800,
    }
}

#[test]
fn a_country_we_know_is_found_whatever_the_capitals() {
    // The address lookup sends them lower case; a value read back out of our own
    // database is ours to be careful about.
    assert_eq!(country("hu").map(|c| c.name), Some("Hungary"));
    assert_eq!(country("HU").map(|c| c.name), Some("Hungary"));
    assert_eq!(
        country("  ae  ").map(|c| c.name),
        Some("United Arab Emirates")
    );
}

#[test]
fn a_country_we_do_not_know_is_nothing_rather_than_a_guess() {
    // Its numbers are kept as they were written rather than given a dialling code we
    // made up.
    assert_eq!(country("zz"), None);
    assert_eq!(country(""), None);
}

#[test]
fn hungary_searches_in_hungarian_and_everywhere_else_in_english() {
    assert_eq!(lang_for("hu"), "hu");
    assert_eq!(lang_for(" HU "), "hu");
    // English, not Arabic: Dubai's listings are in English, and the hours come back
    // in English with it.
    assert_eq!(lang_for("ae"), "en");
    assert_eq!(lang_for("gb"), "en");
    assert_eq!(lang_for("zz"), "en");
}

#[test]
fn a_british_number_is_known() {
    assert_eq!(country("GB").map(|c| c.dials), Some("44"));
}

#[test]
fn every_country_is_written_down_once_and_properly() {
    for entry in COUNTRIES {
        assert_eq!(entry.code.len(), 2, "{} is not two letters", entry.code);
        assert_eq!(entry.code.to_lowercase(), entry.code);
        assert!(!entry.name.is_empty());
        assert!(entry.dials.chars().all(|c| c.is_ascii_digit()));
        assert!(!entry.trunks.is_empty());
        // Longest first, or `0036` would have its `00` read as a trunk prefix and the
        // `36` swallowed into the subscriber number.
        let mut sorted = entry.trunks.to_vec();
        sorted.sort_by_key(|t| std::cmp::Reverse(t.len()));
        assert_eq!(
            sorted,
            entry.trunks.to_vec(),
            "{}'s trunks are out of order",
            entry.code
        );
    }
    let mut codes: Vec<&str> = COUNTRIES.iter().map(|c| c.code).collect();
    codes.sort_unstable();
    let count = codes.len();
    codes.dedup();
    assert_eq!(codes.len(), count, "a country is written down twice");
}

#[test]
fn a_city_measures_from_its_own_centre() {
    let city = budapest();
    // Its own centre is nought away from itself.
    assert!(city.distance_m(47.514900, 19.077800) < 1.0);
    // And Csepel is ten kilometres off, not a few hundred.
    let csepel = city.distance_m(47.4205, 19.0700);
    assert!(csepel > 5_000.0 && csepel < 12_000.0, "{csepel} m");
}

#[test]
fn a_citys_centre_carries_no_result_cap_of_its_own() {
    // It is a point to measure from, not somewhere to search. A cap here would be a
    // second, quieter answer to "how many results" than the area's.
    assert_eq!(budapest().centre().max_results, 0);
}

#[test]
fn the_languages_keep_the_order_they_matter_in() {
    // Sorted instead, English came first and a hand-edited file's columns quietly
    // reshuffled themselves the moment it was saved.
    let found = ["en".to_owned(), "hu".to_owned()];
    assert_eq!(languages_in_order(&found), vec!["hu", "en"]);
}

#[test]
fn a_language_we_have_not_written_down_is_kept_after_the_ones_we_have() {
    // Dropping its column would drop every word in it.
    let found = ["ar".to_owned(), "en".to_owned(), "hu".to_owned()];
    assert_eq!(languages_in_order(&found), vec!["hu", "en", "ar"]);
}

#[test]
fn only_the_languages_actually_there_get_a_column() {
    let found = ["en".to_owned()];
    assert_eq!(languages_in_order(&found), vec!["en"]);
    assert!(languages_in_order(&[]).is_empty());
}

#[test]
fn every_language_we_offer_is_two_letters_and_named() {
    for (code, name) in LANGUAGES {
        assert_eq!(code.len(), 2, "{code}");
        assert_eq!(code.to_lowercase(), code);
        assert!(!name.is_empty());
    }
}

#[test]
fn a_new_centre_keeps_its_name_tidied() {
    assert_eq!(
        check_centre("  Hősök tere, Budapest ", 47.5149, 19.0778),
        Ok("Hősök tere, Budapest".to_owned())
    );
}

#[test]
fn a_new_centre_with_no_name_is_refused() {
    // The page says "measured from …", and it has to have something to say.
    assert!(check_centre("   ", 47.5149, 19.0778).is_err());
}

#[test]
fn a_new_centre_off_the_map_is_refused() {
    for (lat, lon) in [
        (91.0, 0.0),
        (-91.0, 0.0),
        (0.0, 181.0),
        (0.0, -181.0),
        (f64::NAN, 0.0),
    ] {
        assert!(check_centre("somewhere", lat, lon).is_err(), "{lat}, {lon}");
    }
    // The edges themselves are real places.
    assert!(check_centre("the pole", 90.0, 180.0).is_ok());
}

fn spot(lat: f64, lon: f64) -> crate::Area {
    crate::Area {
        label: "here".into(),
        address: String::new(),
        lat,
        lon,
        max_results: 0,
    }
}

#[test]
fn a_starting_point_inside_the_city_is_fine() {
    let budapest = [spot(47.4979, 19.0402)];
    assert_eq!(
        check_centre_reach("Budapest", "Hősök tere", 47.5149, 19.0778, &budapest),
        Ok(())
    );
}

#[test]
fn a_starting_point_in_another_country_is_refused_and_says_where_to_go() {
    let budapest = [spot(47.4979, 19.0402)];
    let said = check_centre_reach("Budapest", "London", 51.5074, -0.1278, &budapest)
        .expect_err("London is not in Budapest");
    assert!(said.contains("London is 14"), "{said}");
    assert!(said.contains("Search another place"), "{said}");
}

#[test]
fn any_one_area_in_reach_is_enough() {
    let city = [spot(47.4979, 19.0402), spot(46.2530, 20.1414)];
    assert!(check_centre_reach("City", "Szeged", 46.25, 20.15, &city).is_ok());
}
