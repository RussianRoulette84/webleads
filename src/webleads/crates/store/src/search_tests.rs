//! Tests for searching the call list, and for the calls you make.

use crate::filter::Wanted;
use crate::testing::*;

// ---- search ----

#[tokio::test]
async fn a_business_can_be_found_by_part_of_its_name() {
    let store = a_mixed_list().await;
    let found = store
        .leads(&Wanted {
            search: Some("borzas".into()),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].lead.name, "Borzas Barber");
}

#[tokio::test]
async fn search_does_not_care_about_capital_letters_or_accents_you_typed() {
    let store = a_mixed_list().await;
    for typed in ["BORZAS", "Borzas", "barber"] {
        let found = store
            .leads(&Wanted {
                search: Some(typed.into()),
                ..Default::default()
            })
            .await
            .expect("reads");
        assert_eq!(found.len(), 1, "searching for `{typed}`");
    }
}

#[tokio::test]
async fn a_business_can_be_found_by_something_only_you_wrote() {
    // "that one I wrote 'ask for the owner' about" — the reason the search looks
    // at the notes and not only the name.
    let store = a_mixed_list().await;
    store
        .set_note("fresh", "ask for the owner")
        .await
        .expect("sets");

    let found = store
        .leads(&Wanted {
            search: Some("owner".into()),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].lead.id, "fresh");
}

#[tokio::test]
async fn a_business_can_be_found_by_its_address() {
    let store = a_mixed_list().await;
    let found = store
        .leads(&Wanted {
            search: Some("Hősök".into()),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn a_percent_sign_searches_for_a_percent_sign() {
    // Unescaped it would mean "anything" to SQL and match every business.
    let store = a_mixed_list().await;
    let found = store
        .leads(&Wanted {
            search: Some("%".into()),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert!(
        found.is_empty(),
        "a bare % matched {} businesses",
        found.len()
    );
}

#[tokio::test]
async fn an_empty_search_is_the_same_as_no_search() {
    let store = a_mixed_list().await;
    for typed in ["", "   "] {
        let found = store
            .leads(&Wanted {
                search: Some(typed.into()),
                ..Default::default()
            })
            .await
            .expect("reads");
        assert_eq!(found.len(), 2, "searching for `{typed}`");
    }
}

#[tokio::test]
async fn a_search_that_matches_nothing_gives_nothing() {
    let store = a_mixed_list().await;
    let found = store
        .leads(&Wanted {
            search: Some("kangaroo".into()),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert!(found.is_empty());
}

// ---- calls ----

#[tokio::test]
async fn ringing_the_same_shop_again_is_a_second_call() {
    // Two calls are two rows — a single date per shop could only ever remember one.
    let store = a_mixed_list().await;
    store.record_call("fresh").await.expect("call");
    store.record_call("fresh").await.expect("call");

    assert_eq!(store.calls("fresh").await.expect("reads").len(), 2);
    let row = store
        .leads(&Wanted::default())
        .await
        .expect("reads")
        .into_iter()
        .find(|l| l.lead.id == "fresh")
        .expect("on the list");
    assert_eq!(row.call_count, 2);
    assert!(row.contact.called_on.is_some());
}

#[tokio::test]
async fn a_business_never_rung_shows_no_calls() {
    let store = a_mixed_list().await;
    let row = store
        .leads(&Wanted::default())
        .await
        .expect("reads")
        .into_iter()
        .find(|l| l.lead.id == "fresh")
        .expect("on the list");
    assert_eq!(row.call_count, 0);
    assert_eq!(row.contact.called_on, None);
}
