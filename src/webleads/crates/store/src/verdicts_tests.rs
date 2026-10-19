//! Tests for [`crate::verdicts`].

use super::*;
use crate::filter::Wanted;
use crate::testing::*;

/// One business we decided was broken, with the address we judged.
async fn called_broken_once(website: &str) -> Store {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    let mut lead = a_lead("shop", 3, 100);
    lead.website = Some(website.to_owned());
    lead.verdict = Verdict::DeadSite;
    lead.reason = "server answers 404".to_owned();
    store.record(run, &[found(lead)]).await.expect("records");
    store
}

#[tokio::test]
async fn the_ones_we_called_broken_come_back_with_what_we_judged() {
    let store = called_broken_once("https://example.hu/kapcsolat.html").await;
    let suspects = store.called_broken().await.expect("reads");
    assert_eq!(suspects.len(), 1);
    assert_eq!(suspects[0].business_id, "shop");
    assert_eq!(suspects[0].website, "https://example.hu/kapcsolat.html");
    assert_eq!(suspects[0].reason, "server answers 404");
}

#[tokio::test]
async fn a_business_with_no_website_is_never_a_suspect() {
    // Nothing was judged, so there is nothing to judge again.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(run, &[found(a_lead("shop", 1, 10))])
        .await
        .expect("records");
    assert!(store.called_broken().await.expect("reads").is_empty());
}

#[tokio::test]
async fn a_working_site_behind_a_dead_link_leaves_the_call_list() {
    // The whole point: Google stored the contact page, it 404s, the site is fine —
    // and this business should never have been on the list.
    let store = called_broken_once("https://example.hu/kapcsolat.html").await;
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        1
    );

    let changed = store
        .rewrite_verdict(
            "shop",
            Verdict::LiveSite,
            "site itself works; Google's link to /kapcsolat.html is dead",
            &Tls::Valid,
        )
        .await
        .expect("rewrites");
    assert_eq!(changed, 1);

    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        0,
        "a business with a working website is not a lead"
    );
    // And it is no longer waiting to be judged again.
    assert!(store.called_broken().await.expect("reads").is_empty());
}

#[tokio::test]
async fn a_rewrite_keeps_what_it_learned_about_the_certificate() {
    let store = called_broken_once("https://example.hu/kapcsolat.html").await;
    store
        .rewrite_verdict(
            "shop",
            Verdict::InsecureSite,
            "certificate expired",
            &Tls::Broken("certificate expired".to_owned()),
        )
        .await
        .expect("rewrites");

    let back = store.lead("shop").await.expect("reads").expect("there");
    assert_eq!(back.lead.verdict, Verdict::InsecureSite);
    assert_eq!(back.lead.tls, Tls::Broken("certificate expired".to_owned()));
    // Still a lead: a browser shows a warning before anybody sees the shop.
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        1
    );
}

#[tokio::test]
async fn a_rewrite_says_so_when_there_was_nothing_to_rewrite() {
    let store = called_broken_once("https://example.hu/x.html").await;
    let changed = store
        .rewrite_verdict("nobody", Verdict::LiveSite, "loads fine (200)", &Tls::Valid)
        .await
        .expect("tries");
    assert_eq!(changed, 0, "a business we do not hold cannot be changed");
}

#[tokio::test]
async fn the_date_of_the_check_moves_with_the_verdict() {
    // Otherwise the next import fetches the same site to reach the answer we have
    // just written down.
    let store = called_broken_once("https://example.hu/x.html").await;
    store
        .rewrite_verdict("shop", Verdict::LiveSite, "loads fine (200)", &Tls::Valid)
        .await
        .expect("rewrites");

    let fresh = store.recent_verdicts(14).await.expect("reads");
    assert_eq!(
        fresh.get("shop").map(|(v, _, _)| *v),
        Some(Verdict::LiveSite)
    );
}
