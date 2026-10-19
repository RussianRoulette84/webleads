//! Tests for [`crate::cities`].

use super::*;

fn a_run(id: i64, city: Option<i64>) -> api::Run {
    api::Run {
        id,
        state: "done".to_owned(),
        source: "Budapest".to_owned(),
        note: None,
        words_total: 1,
        words_done: 1,
        found_total: 20,
        now_searching: None,
        area: Some("Budapest".to_owned()),
        city,
        started_at: "2026-09-21".to_owned(),
        finished_at: None,
        leads_found: 3,
    }
}

#[test]
fn a_chip_says_the_city_and_how_many() {
    assert_eq!(chip_words("Budapest", 2473), "Budapest 2473");
    assert_eq!(chip_words("Dubai", 3), "Dubai 3");
}

#[test]
fn asking_for_no_city_leaves_every_search_in_the_strip() {
    let runs = [a_run(1, Some(10)), a_run(2, Some(20))];
    assert_eq!(searches_in_city(&runs, None).len(), 2);
}

#[test]
fn choosing_a_city_shortens_the_strip_to_its_own_searches() {
    // Otherwise picking Dubai leaves a row of Budapest searches under it that all
    // narrow to nothing.
    let runs = [a_run(1, Some(10)), a_run(2, Some(20)), a_run(3, Some(10))];
    let kept = searches_in_city(&runs, Some(10));
    let ids: Vec<i64> = kept.iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![1, 3]);
}

#[test]
fn a_search_from_before_cities_is_shown_whichever_city_you_pick() {
    // Hiding it would be losing a search rather than narrowing to one.
    let runs = [a_run(1, None), a_run(2, Some(20))];
    let kept = searches_in_city(&runs, Some(10));
    let ids: Vec<i64> = kept.iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![1]);
}

#[test]
fn a_city_with_no_searches_of_its_own_leaves_an_empty_strip() {
    let runs = [a_run(1, Some(20))];
    assert!(searches_in_city(&runs, Some(10)).is_empty());
}
