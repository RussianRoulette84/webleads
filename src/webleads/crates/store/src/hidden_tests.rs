//! Tests for [`crate::hidden`] — what is kept off the call list.

use super::*;
use crate::testing::*;

/// Three businesses: two branches of one chain, and one on its own.
async fn a_list_with_a_chain() -> Store {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    let mut first = a_lead("zuglo", 3, 100);
    first.name = "Best Barbers Zugló".into();
    let mut second = a_lead("obuda", 4, 200);
    second.name = "Best Barbers Óbuda".into();
    let mut alone = a_lead("borzas", 5, 300);
    alone.name = "Borzas Barber".into();

    store
        .record(run, &[found(first), found(second), found(alone)])
        .await
        .expect("records");
    store
}

#[tokio::test]
async fn hiding_one_business_leaves_the_rest() {
    let store = a_list_with_a_chain().await;
    store
        .hide_business("zuglo", "Best Barbers Zugló")
        .await
        .expect("hides");

    let want = Wanted::default();
    let left: Vec<String> = store
        .leads(&want)
        .await
        .expect("reads")
        .into_iter()
        .map(|l| l.lead.id)
        .collect();
    assert_eq!(left.len(), 2);
    assert!(!left.contains(&"zuglo".to_owned()));
    // And the count agrees with the rows, which is the rule the list rests on.
    assert_eq!(store.lead_count(&want).await.expect("counts"), 2);
}

#[tokio::test]
async fn hiding_a_chain_takes_every_branch() {
    let store = a_list_with_a_chain().await;
    store.hide_chain("Best Barbers Zugló").await.expect("hides");

    let left: Vec<String> = store
        .leads(&Wanted::default())
        .await
        .expect("reads")
        .into_iter()
        .map(|l| l.lead.id)
        .collect();
    assert_eq!(left, ["borzas"], "both branches should have gone");
}

#[tokio::test]
async fn a_hidden_business_is_off_the_map_too() {
    // The map and the table must never disagree about what exists.
    let store = a_list_with_a_chain().await;
    store.hide_chain("Best Barbers Zugló").await.expect("hides");
    assert_eq!(store.pins(&Wanted::default()).await.expect("pins").len(), 1);
}

#[tokio::test]
async fn the_hidden_ones_are_counted_so_the_list_says_why_it_shrank() {
    let store = a_list_with_a_chain().await;
    store.hide_chain("Best Barbers Zugló").await.expect("hides");

    let want = Wanted::default();
    assert_eq!(store.hidden_count(&want).await.expect("counts"), 2);
    assert_eq!(store.lead_count(&want).await.expect("counts"), 1);
}

#[tokio::test]
async fn bringing_one_back_restores_it_exactly() {
    let store = a_list_with_a_chain().await;
    let id = store
        .hide_business("borzas", "Borzas Barber")
        .await
        .expect("hides");
    assert!(store.unhide(id).await.expect("unhides"));

    let back = store.leads(&Wanted::default()).await.expect("reads");
    assert_eq!(back.len(), 3);
}

#[tokio::test]
async fn hiding_never_loses_your_notes() {
    // The whole reason this hides rather than deletes.
    let store = a_list_with_a_chain().await;
    store
        .set_status("borzas", webleads_domain::Status::NotReached)
        .await
        .expect("writes the state");
    store
        .set_note("borzas", "call back at five")
        .await
        .expect("writes the note");

    let id = store
        .hide_business("borzas", "Borzas Barber")
        .await
        .expect("hides");
    store.unhide(id).await.expect("unhides");

    let back = store.leads(&Wanted::default()).await.expect("reads");
    let borzas = back
        .iter()
        .find(|l| l.lead.id == "borzas")
        .expect("it is back");
    assert_eq!(borzas.contact.note, "call back at five");
    assert_eq!(borzas.contact.status, webleads_domain::Status::NotReached);
}

#[tokio::test]
async fn the_panel_says_what_each_rule_catches() {
    let store = a_list_with_a_chain().await;
    store.hide_chain("Best Barbers Zugló").await.expect("hides");
    store
        .hide_business("borzas", "Borzas Barber")
        .await
        .expect("hides");

    let list = store.hidden().await.expect("lists");
    assert_eq!(list.len(), 2);
    // Newest first, so the one you have just hidden is at the top.
    assert_eq!(list[0].label, "Borzas Barber");
    assert!(!list[0].is_chain);
    assert_eq!(list[0].catches, 1);

    assert_eq!(list[1].label, "Best Barbers");
    assert!(list[1].is_chain);
    assert_eq!(list[1].catches, 2);
}

#[tokio::test]
async fn hiding_a_blank_name_is_refused_rather_than_hiding_everything() {
    let store = a_list_with_a_chain().await;
    let outcome = store.hide_chain("   ").await;
    assert!(matches!(outcome, Err(crate::Error::Refused(_))));
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        3,
        "the list must be untouched"
    );
}
