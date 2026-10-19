//! Tests for [`crate::phones`].

use super::*;
use crate::filter::Wanted;
use crate::testing::*;

/// One business with the number written however Google sent it.
async fn a_list_with(phone: &str) -> Store {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    let mut lead = a_lead("shop", 3, 100);
    lead.phone = Some(phone.to_owned());
    store.record(run, &[found(lead)]).await.expect("records");
    store
}

async fn stored_phone(store: &Store) -> Option<String> {
    store
        .leads(&Wanted::default())
        .await
        .expect("reads")
        .first()
        .and_then(|l| l.lead.phone.clone())
}

#[tokio::test]
async fn a_mobile_number_is_saved_in_its_tidy_form() {
    let store = a_list_with("06 20 555 0103").await;
    assert_eq!(stored_phone(&store).await.as_deref(), Some("+36205550103"));
}

#[tokio::test]
async fn a_budapest_landline_loses_its_brackets_and_spaces() {
    let store = a_list_with("(06 1) 234 5002").await;
    assert_eq!(stored_phone(&store).await.as_deref(), Some("+3612345002"));
}

#[tokio::test]
async fn one_already_international_is_only_closed_up() {
    let store = a_list_with("+36 1 234 5001").await;
    assert_eq!(stored_phone(&store).await.as_deref(), Some("+3612345001"));
}

#[tokio::test]
async fn nothing_readable_is_left_exactly_as_it_arrived() {
    // Mangling a number nobody can ring does not improve it.
    let store = a_list_with("call the shop").await;
    assert_eq!(stored_phone(&store).await.as_deref(), Some("call the shop"));
}

#[tokio::test]
async fn numbers_collected_before_this_are_tidied_once() {
    // What a database written by the earlier version looks like.
    let store = a_list_with("06 20 555 0103").await;
    sqlx::query("UPDATE business SET phone = '06 20 555 0103'")
        .execute(&store.pool)
        .await
        .expect("puts the old shape back");

    assert_eq!(store.tidy_phones().await.expect("tidies"), 1);
    assert_eq!(stored_phone(&store).await.as_deref(), Some("+36205550103"));
}

#[tokio::test]
async fn a_list_already_tidy_is_left_alone() {
    // After the first pass there is nothing to do, and it must say so rather than
    // rewriting every row at every start.
    let store = a_list_with("06 20 555 0103").await;
    assert_eq!(store.tidy_phones().await.expect("tidies"), 0);
}

#[tokio::test]
async fn tidying_never_loses_a_business() {
    let store = a_list_with("(06 1) 234 5002").await;
    let before = store.lead_count(&Wanted::default()).await.expect("counts");
    store.tidy_phones().await.expect("tidies");
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        before
    );
}
