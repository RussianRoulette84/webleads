//! Tests for [`crate::tally`] — the scoreboard of a day's calling.

use super::*;
use crate::testing::*;

/// Three businesses on the list, nothing marked yet.
async fn three_to_call() -> Store {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                found(a_lead("one", 3, 100)),
                found(a_lead("two", 4, 200)),
                found(a_lead("three", 5, 300)),
            ],
        )
        .await
        .expect("records");
    store
}

#[test]
fn the_scoreboard_leaves_out_the_ones_you_have_not_rung() {
    // "To call" is the size of the pile, not a measure of a day — and a scoreboard
    // of six that opens with "two thousand" tells you nothing about today.
    let shown = worth_counting();
    assert!(!shown.contains(&Status::NeedCall));
    assert_eq!(shown.len(), 6);
}

#[test]
fn a_state_nobody_is_in_counts_as_nought() {
    let counted = [(Status::Sold, 2)];
    assert_eq!(how_many(&counted, Status::Sold), 2);
    assert_eq!(how_many(&counted, Status::Dead), 0);
}

#[tokio::test]
async fn nothing_marked_is_a_scoreboard_of_noughts() {
    let store = three_to_call().await;
    let tally = store.tally().await.expect("counts");
    assert_eq!(tally.today, Vec::new());
    assert_eq!(tally.called_today, 0);
    assert_eq!(tally.called_ever, 0);
    assert_eq!(tally.changed_at, "");
}

#[tokio::test]
async fn marking_one_business_shows_up_in_both_columns() {
    let store = three_to_call().await;
    store
        .set_status("one", Status::NotInterested)
        .await
        .expect("marks");

    let tally = store.tally().await.expect("counts");
    assert_eq!(how_many(&tally.today, Status::NotInterested), 1);
    assert_eq!(how_many(&tally.ever, Status::NotInterested), 1);
}

#[tokio::test]
async fn saying_no_to_two_counts_two_not_one() {
    let store = three_to_call().await;
    for id in ["one", "two"] {
        store
            .set_status(id, Status::NotInterested)
            .await
            .expect("marks");
    }
    let tally = store.tally().await.expect("counts");
    assert_eq!(how_many(&tally.today, Status::NotInterested), 2);
}

#[tokio::test]
async fn changing_your_mind_moves_the_business_rather_than_counting_it_twice() {
    let store = three_to_call().await;
    store.set_status("one", Status::Sold).await.expect("marks");
    store
        .set_status("one", Status::NotInterested)
        .await
        .expect("marks again");

    let tally = store.tally().await.expect("counts");
    assert_eq!(how_many(&tally.today, Status::Sold), 0);
    assert_eq!(how_many(&tally.today, Status::NotInterested), 1);
}

#[tokio::test]
async fn ringing_the_same_shop_twice_is_one_shop_called() {
    let store = three_to_call().await;
    store.record_call("one").await.expect("rings");
    store.record_call("one").await.expect("rings again");
    store.record_call("two").await.expect("rings");

    let tally = store.tally().await.expect("counts");
    assert_eq!(tally.called_today, 2);
    assert_eq!(tally.called_ever, 2);
}

#[tokio::test]
async fn the_change_marker_carries_a_time_not_only_a_date() {
    // This is what a second screen compares against. A date alone would mean a
    // phone could not tell this morning's marking from this afternoon's, and would
    // sit there showing yesterday's states all day.
    let store = three_to_call().await;
    store.set_status("one", Status::Sold).await.expect("marks");

    let tally = store.tally().await.expect("counts");
    assert_eq!(
        tally.changed_at.len(),
        16,
        "expected `2026-10-13 14:05`, got `{}`",
        tally.changed_at
    );
    assert!(tally.changed_at.contains(':'));
    assert!(tally
        .changed_at
        .starts_with(&webleads_domain::today_here_date()));
}

#[tokio::test]
async fn businesses_folded_onto_one_number_are_counted_once() {
    // All three share a telephone, so merging keeps one and points the others at
    // it. Counting the pointers as well would say you sold three times.
    let store = three_to_call().await;
    store.set_status("one", Status::Sold).await.expect("marks");
    store.set_status("two", Status::Sold).await.expect("marks");
    store.merge_duplicates().await.expect("merges");

    let tally = store.tally().await.expect("counts");
    assert_eq!(how_many(&tally.ever, Status::Sold), 1);
}
