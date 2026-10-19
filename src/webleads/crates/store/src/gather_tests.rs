//! Tests for gathering a city's searches into one.

use super::*;
use crate::testing::*;
use crate::{RunState, Wanted};

/// A search that is over, which is the only kind worth gathering.
async fn a_finished_search(store: &Store) -> i64 {
    let id = a_search(store).await;
    store
        .set_run_state(id, RunState::Done, None)
        .await
        .expect("ends");
    id
}

#[test]
fn numbers_go_into_the_query_the_way_sql_reads_a_list() {
    assert_eq!(number_list(&[1]), "1");
    assert_eq!(number_list(&[3, 7, 11]), "3, 7, 11");
    assert_eq!(number_list(&[]), "");
}

#[tokio::test]
async fn a_citys_searches_become_one_and_keep_every_business() {
    let store = Store::in_memory().await.expect("opens");
    let first = a_finished_search(&store).await;
    store
        .record(
            first,
            &[found(a_lead("a", 0, 10)), found(a_lead("b", 1, 20))],
        )
        .await
        .expect("records");
    let second = a_finished_search(&store).await;
    store
        .record(second, &[found(a_lead("c", 2, 30))])
        .await
        .expect("records");

    let before = store.lead_count(&Wanted::default()).await.expect("counts");
    let gathered = store.gather_searches().await.expect("gathers");

    assert_eq!(gathered.len(), 1, "one city, one gathering");
    assert_eq!(gathered[0].searches, 2);
    assert_eq!(gathered[0].kept, first);
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        before,
        "nobody leaves the call list"
    );
    assert_eq!(store.past_scrapes(50).await.expect("reads").len(), 1);
}

#[tokio::test]
async fn everything_now_belongs_to_the_search_that_stayed() {
    let store = Store::in_memory().await.expect("opens");
    let first = a_finished_search(&store).await;
    store
        .record(first, &[found(a_lead("a", 0, 10))])
        .await
        .expect("records");
    let second = a_finished_search(&store).await;
    store
        .record(second, &[found(a_lead("b", 1, 20))])
        .await
        .expect("records");

    store.gather_searches().await.expect("gathers");

    let want = Wanted {
        from_run: Some(first),
        ..Default::default()
    };
    assert_eq!(
        store.lead_count(&want).await.expect("counts"),
        2,
        "both evenings answer to one search now"
    );
}

#[tokio::test]
async fn a_business_seen_twice_keeps_the_newer_verdict() {
    let store = Store::in_memory().await.expect("opens");
    let first = a_finished_search(&store).await;
    store
        .record(first, &[found(a_lead("a", 0, 10))])
        .await
        .expect("records");
    let second = a_finished_search(&store).await;
    let mut again = a_lead("a", 0, 10);
    again.reviews = 99;
    store
        .record(second, &[found(again)])
        .await
        .expect("records");

    store.gather_searches().await.expect("gathers");

    let rows = store.leads(&Wanted::default()).await.expect("reads");
    assert_eq!(rows.len(), 1, "one row, not two");
    assert_eq!(rows[0].lead.reviews, 99, "the later evening's reading");
}

#[tokio::test]
async fn one_search_on_its_own_is_left_exactly_as_it_was() {
    let store = Store::in_memory().await.expect("opens");
    let only = a_finished_search(&store).await;
    store
        .record(only, &[found(a_lead("a", 0, 10))])
        .await
        .expect("records");

    assert!(store.gather_searches().await.expect("gathers").is_empty());
    assert_eq!(store.past_scrapes(50).await.expect("reads").len(), 1);
}
