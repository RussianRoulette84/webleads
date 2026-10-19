//! Tests for [`crate::areas`].

use super::*;

#[test]
fn the_name_that_goes_into_a_search_is_the_short_one() {
    // "borbély Zugló", not "borbély Zugló, Budapest, 1145, Magyarország".
    let names = name_choices("Zugló, Budapest, 1145, Magyarország");
    assert_eq!(names[0], "Zugló");
}

#[test]
fn the_district_and_its_city_are_offered_as_well() {
    // "XIV. kerület" on its own finds a district number; with the city it finds the
    // place. Which is right depends on the place, so both are offered.
    let names = name_choices("XIV. kerület, Budapest, Közép-Magyarország, Magyarország");
    assert_eq!(names, vec!["XIV. kerület", "XIV. kerület, Budapest"]);
}

#[test]
fn never_more_than_two_ways_of_saying_it() {
    // A third is a decision rather than a suggestion.
    let names = name_choices("a, b, c, d, e");
    assert_eq!(names.len(), 2);
}

#[test]
fn a_name_with_no_comma_is_left_alone() {
    assert_eq!(name_choices("Óbuda"), vec!["Óbuda"]);
    assert_eq!(name_choices("  Óbuda  "), vec!["Óbuda"]);
}

#[test]
fn a_lookup_that_answers_with_nothing_useful_still_offers_something() {
    // Better one odd name you can see than an empty list you cannot press.
    assert_eq!(name_choices(""), vec![""]);
    // Nothing usable between the commas: the address itself, tidied, is all there
    // is to offer. A lookup has never answered like this, and if one does the row
    // is still something you can read and press.
    assert_eq!(name_choices(" , , "), vec![", ,"]);
}

// ---- keeping a place under the name you typed ----

#[test]
fn what_you_typed_is_what_it_is_called() {
    // "Zugló" searches better than the postal address the lookup answers with.
    assert_eq!(typed_name("Zugló"), Some("Zugló".to_owned()));
    assert_eq!(
        typed_name("  Zugló, Budapest  "),
        Some("Zugló, Budapest".to_owned())
    );
}

#[test]
fn an_empty_box_is_not_a_place() {
    // An area called "  " is one you cannot tell from the others.
    assert_eq!(typed_name(""), None);
    assert_eq!(typed_name("   "), None);
    assert_eq!(typed_name("\t\n"), None);
}

#[test]
fn the_city_you_type_wins_over_the_one_that_was_found() {
    // The box is how you correct the lookup, not how you answer it.
    assert_eq!(
        pick_city("Budapest", Some("Dubaj")).as_deref(),
        Some("Budapest")
    );
    assert_eq!(
        pick_city("  Budapest  ", Some("Dubaj")).as_deref(),
        Some("Budapest")
    );
}

#[test]
fn leaving_the_city_blank_takes_what_the_lookup_found() {
    assert_eq!(pick_city("", Some("Dubai")).as_deref(), Some("Dubai"));
    assert_eq!(pick_city("   ", Some("Dubai")).as_deref(), Some("Dubai"));
}

#[test]
fn an_area_can_belong_to_no_city_at_all() {
    // Still searchable; its leads simply sit under no chip. Better than filing them
    // under a town we guessed at.
    assert_eq!(pick_city("", None), None);
    assert_eq!(pick_city("  ", Some("   ")), None);
}

#[test]
fn a_match_says_its_town_and_its_country() {
    assert_eq!(
        where_said("Dubai", "United Arab Emirates"),
        "Dubai, United Arab Emirates"
    );
    assert_eq!(
        where_said("Budapest", "Magyarország"),
        "Budapest, Magyarország"
    );
}

#[test]
fn a_town_that_is_also_the_country_is_said_once() {
    // Otherwise a Gulf city-state reads "Dubaj, Dubaj".
    assert_eq!(where_said("Dubaj", "Dubaj"), "Dubaj");
    assert_eq!(where_said("Monaco", "monaco"), "Monaco");
}

#[test]
fn a_missing_half_leaves_no_stray_comma() {
    // A comma with nothing after it reads as a fault.
    assert_eq!(where_said("Dubai", ""), "Dubai");
    assert_eq!(where_said("", "Hungary"), "Hungary");
    assert_eq!(where_said("", ""), "");
}
