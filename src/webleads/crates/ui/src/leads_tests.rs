//! Tests for [`crate::leads`].

use super::*;

#[test]
fn a_round_of_pictures_says_what_it_came_to() {
    assert_eq!(
        describe_pictures(&api::Pictures { taken: 48, of: 48 }),
        "48 pictures taken"
    );
    // Some sites never answer, so fewer than were tried is the usual outcome.
    assert_eq!(
        describe_pictures(&api::Pictures { taken: 31, of: 48 }),
        "31 pictures taken, of 48 tried"
    );
}

#[test]
fn nothing_to_photograph_says_so_rather_than_saying_nought() {
    assert_eq!(
        describe_pictures(&api::Pictures { taken: 0, of: 0 }),
        "nothing to photograph"
    );
    // And a round where every site refused is not the same thing.
    assert_eq!(
        describe_pictures(&api::Pictures { taken: 0, of: 12 }),
        "no pictures taken, of 12 tried"
    );
}

#[test]
fn one_city_is_named_on_its_own() {
    let one = [(1, "Budapest".to_owned(), 2473)];
    assert_eq!(describe_cities(&one), "Budapest. ");
}

#[test]
fn two_cities_are_joined_by_an_and() {
    let two = [(1, "Budapest".to_owned(), 2473), (2, "Dubai".to_owned(), 5)];
    assert_eq!(describe_cities(&two), "Budapest and Dubai. ");
}

#[test]
fn past_two_cities_it_says_how_many_rather_than_listing_them() {
    // A sentence listing a dozen towns is a sentence nobody finishes, and the chips
    // above it already say which.
    let many: Vec<(i64, String, i64)> = (1..=5).map(|n| (n, format!("City {n}"), 10)).collect();
    assert_eq!(describe_cities(&many), "5 cities. ");
}

#[test]
fn an_empty_list_names_no_city_at_all() {
    assert_eq!(describe_cities(&[]), "");
}
