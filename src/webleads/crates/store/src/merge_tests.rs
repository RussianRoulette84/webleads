//! Tests for [`crate::merge`].

use super::*;
use crate::filter::Wanted;
use crate::testing::*;
use crate::{Found, Store};

#[test]
fn a_sale_survives_any_merge() {
    for other in Status::ALL {
        assert_eq!(further_along(Status::Sold, other), Status::Sold);
        assert_eq!(further_along(other, Status::Sold), Status::Sold);
    }
}

#[test]
fn a_refusal_beats_a_promise_to_ring_back_but_not_a_sale() {
    // Saying no is the later thing to have learned than promising a call back.
    assert_eq!(
        further_along(Status::FollowUp, Status::NotInterested),
        Status::NotInterested
    );
    assert_eq!(
        further_along(Status::NotInterested, Status::Sold),
        Status::Sold
    );
}

#[test]
fn anything_beats_never_having_touched_it() {
    for other in Status::ALL {
        assert_eq!(further_along(Status::NeedCall, other), other);
    }
}

#[test]
fn a_number_that_stopped_answering_loses_to_a_conversation() {
    // Dead is reached by counting rings. It must not undo what somebody said.
    for spoken_to in [Status::FollowUp, Status::NotInterested, Status::Sold] {
        assert_eq!(further_along(Status::Dead, spoken_to), spoken_to);
    }
    assert_eq!(further_along(Status::Dead, Status::NeedCall), Status::Dead);
}

#[test]
fn the_rule_does_not_care_which_way_round_it_is_asked() {
    for a in Status::ALL {
        for b in Status::ALL {
            assert_eq!(further_along(a, b), further_along(b, a), "{a:?} vs {b:?}");
        }
    }
}

/// A business with a chosen name and telephone number.
fn shop(id: &str, name: &str, phone: &str) -> Found {
    let mut lead = a_lead(id, 0, 100);
    lead.name = name.to_owned();
    lead.phone = Some(phone.to_owned());
    found(lead)
}

#[tokio::test]
async fn two_listings_on_one_number_become_one_business() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                shop("old", "Fodrászat", "+36 1 234 5001"),
                shop("new", "Fodraszat Zuglo", "06 1 234 5001"),
            ],
        )
        .await
        .expect("records");
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        2
    );

    let merged = store.merge_duplicates().await.expect("merges");
    assert_eq!(merged.len(), 1);
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        1
    );
}

#[tokio::test]
async fn the_number_written_two_ways_still_counts_as_one() {
    // `+36 1 234 5001` and `06 1 234 5001` are the same line. This only works
    // because the canonical column exists — which is what it is for.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                shop("a", "Shop A", "+36 1 234 5001"),
                shop("b", "Shop B", "(061) 234 5001"),
            ],
        )
        .await
        .expect("records");
    assert_eq!(store.merge_duplicates().await.expect("merges").len(), 1);
}

#[tokio::test]
async fn different_numbers_are_left_well_alone() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                shop("a", "Shop A", "+36 1 234 5001"),
                shop("b", "Shop B", "+36 1 234 5003"),
            ],
        )
        .await
        .expect("records");
    assert!(store.merge_duplicates().await.expect("merges").is_empty());
    // Counted as businesses rather than leads: without a telephone number they are
    // not leads at all, which is a separate rule and not what this test is about.
    assert_eq!(store.business_count().await.expect("counts"), 2);
}

#[tokio::test]
async fn a_business_with_no_number_is_never_merged_with_another() {
    // Otherwise every business without a telephone would collapse into one.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    let mut a = shop("a", "Shop A", "");
    let mut b = shop("b", "Shop B", "");
    a.lead.phone = None;
    b.lead.phone = None;
    store.record(run, &[a, b]).await.expect("records");

    assert!(store.merge_duplicates().await.expect("merges").is_empty());
    // Counted as businesses rather than leads: without a telephone number they are
    // not leads at all, which is a separate rule and not what this test is about.
    assert_eq!(store.business_count().await.expect("counts"), 2);
}

#[tokio::test]
async fn the_one_we_have_known_longest_is_the_one_that_survives() {
    // `first_seen` is the column the whole "new since last run" idea rests on.
    // Folding the older into the newer would throw away exactly that.
    let store = Store::in_memory().await.expect("opens");
    let first = a_search(&store).await;
    store
        .record(
            first,
            &[shop("old", "Known Since Before", "+36 1 234 5001")],
        )
        .await
        .expect("records");

    let second = a_search(&store).await;
    store
        .record(second, &[shop("new", "Just Turned Up", "06 1 234 5001")])
        .await
        .expect("records");

    let merged = store.merge_duplicates().await.expect("merges");
    assert_eq!(merged[0].kept, "old", "the older one must survive");
    assert_eq!(merged[0].gone, "new");
}

#[tokio::test]
async fn nothing_is_deleted_and_the_trail_says_where_it_went() {
    // The whole reason a wrong join is not one-way.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                shop("kept", "Shop A", "+36 1 234 5001"),
                shop("gone", "Shop B", "06 1 234 5001"),
            ],
        )
        .await
        .expect("records");
    store.merge_duplicates().await.expect("merges");

    // Both rows still exist — 2 businesses, 1 on the list.
    assert_eq!(store.business_count().await.expect("counts"), 2);
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        1
    );
    assert_eq!(
        store.merged_into("gone").await.expect("reads").as_deref(),
        Some("kept")
    );
    assert_eq!(store.merged_into("kept").await.expect("reads"), None);
}

#[tokio::test]
async fn both_sets_of_notes_survive_the_merge() {
    // Losing a note because the other row happened to be older would be the
    // wrong trade — you wrote both of them.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                shop("a", "Shop A", "+36 1 234 5001"),
                shop("b", "Shop B", "06 1 234 5001"),
            ],
        )
        .await
        .expect("records");
    store
        .set_note("a", "ask for the owner")
        .await
        .expect("sets");
    store.set_note("b", "closed Mondays").await.expect("sets");

    store.merge_duplicates().await.expect("merges");
    let survivor = store.contact("a").await.expect("reads");
    assert!(
        survivor.note.contains("ask for the owner"),
        "{}",
        survivor.note
    );
    assert!(
        survivor.note.contains("closed Mondays"),
        "{}",
        survivor.note
    );
}

#[tokio::test]
async fn the_furthest_along_status_wins() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                shop("a", "Shop A", "+36 1 234 5001"),
                shop("b", "Shop B", "06 1 234 5001"),
            ],
        )
        .await
        .expect("records");
    store.set_status("b", Status::Sold).await.expect("sets");

    store.merge_duplicates().await.expect("merges");
    assert_eq!(
        store.contact("a").await.expect("reads").status,
        Status::Sold
    );
}

#[tokio::test]
async fn the_calls_to_both_are_all_kept() {
    // They all happened, whoever we thought we were ringing.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                shop("a", "Shop A", "+36 1 234 5001"),
                shop("b", "Shop B", "06 1 234 5001"),
            ],
        )
        .await
        .expect("records");
    store.record_call("a").await.expect("call");
    store.record_call("b").await.expect("call");
    store.record_call("b").await.expect("call");

    store.merge_duplicates().await.expect("merges");
    assert_eq!(store.calls("a").await.expect("reads").len(), 3);
}

#[tokio::test]
async fn merging_twice_does_nothing_the_second_time() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                shop("a", "Shop A", "+36 1 234 5001"),
                shop("b", "Shop B", "06 1 234 5001"),
            ],
        )
        .await
        .expect("records");

    assert_eq!(store.merge_duplicates().await.expect("merges").len(), 1);
    assert!(store.merge_duplicates().await.expect("merges").is_empty());
}

#[tokio::test]
async fn three_listings_on_one_number_all_fold_into_the_oldest() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                shop("a", "Shop A", "+36 1 234 5001"),
                shop("b", "Shop B", "06 1 234 5001"),
                shop("c", "Shop C", "0036 1 234 5001"),
            ],
        )
        .await
        .expect("records");

    assert_eq!(store.merge_duplicates().await.expect("merges").len(), 2);
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        1
    );
}
