//! Tests for [`crate::areas`] — the places you can search.

use super::*;

fn somewhere(label: &str) -> Area {
    Area {
        label: label.to_owned(),
        address: format!("{label}, Budapest"),
        lat: 47.5,
        lon: 19.1,
        max_results: 200,
    }
}

#[tokio::test]
async fn a_fresh_database_already_has_somewhere_to_search() {
    // Nothing is more useless than a program that cannot search until you configure
    // it. Budapest is there from the start, in the city of the same name.
    let store = Store::in_memory().await.expect("opens");
    let areas = store.areas().await.expect("lists");

    let names: Vec<&str> = areas.iter().map(|a| a.area.label.as_str()).collect();
    assert_eq!(names, vec!["Budapest"]);
}

#[tokio::test]
async fn an_area_you_add_comes_back_as_you_wrote_it() {
    let store = Store::in_memory().await.expect("opens");
    let id = store.add_area(&somewhere("Óbuda")).await.expect("adds");

    let back = store.area(id).await.expect("reads").expect("it is there");
    assert_eq!(back.area.label, "Óbuda");
    assert!(back.area.address.contains("Óbuda"));
}

#[tokio::test]
async fn two_areas_cannot_share_a_name() {
    // The name goes into the search words, so two of them would be two searches
    // nobody could tell apart afterwards.
    let store = Store::in_memory().await.expect("opens");
    store.add_area(&somewhere("Óbuda")).await.expect("adds");
    let again = store.add_area(&somewhere("Óbuda")).await;
    assert!(matches!(again, Err(crate::Error::Refused(_))));
}

#[tokio::test]
async fn an_area_with_no_name_is_refused() {
    let store = Store::in_memory().await.expect("opens");
    let outcome = store.add_area(&somewhere("   ")).await;
    assert!(matches!(outcome, Err(crate::Error::Refused(_))));
}

#[tokio::test]
async fn the_chosen_area_is_remembered() {
    let store = Store::in_memory().await.expect("opens");
    let id = store.add_area(&somewhere("Óbuda")).await.expect("adds");
    assert!(store.choose_area(id).await.expect("chooses"));

    let chosen = store
        .chosen_area()
        .await
        .expect("reads")
        .expect("one is chosen");
    assert_eq!(chosen.id, id);
}

#[tokio::test]
async fn choosing_somewhere_that_is_not_there_changes_nothing() {
    let store = Store::in_memory().await.expect("opens");
    assert!(!store.choose_area(9_999).await.expect("asks"));
    let chosen = store
        .chosen_area()
        .await
        .expect("reads")
        .expect("still one");
    assert_eq!(chosen.area.label, "Budapest", "the first one stands in");
}

#[tokio::test]
async fn forgetting_the_chosen_area_leaves_somewhere_to_search() {
    // Otherwise the next search would have nowhere to look and no way to say so.
    let store = Store::in_memory().await.expect("opens");
    let id = store.add_area(&somewhere("Óbuda")).await.expect("adds");
    store.choose_area(id).await.expect("chooses");
    assert!(store.remove_area(id).await.expect("removes"));

    let chosen = store
        .chosen_area()
        .await
        .expect("reads")
        .expect("stands in");
    assert_ne!(chosen.id, id);
}
