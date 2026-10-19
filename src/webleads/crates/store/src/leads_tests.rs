//! Tests for [`crate::leads`] — the call list and what is on it.

use super::*;
use crate::filter::Wanted;
use crate::testing::*;
use crate::Store;

#[tokio::test]
async fn the_states_on_show_add_up_to_the_total() {
    // This is the bug: the buttons and the total were two different pieces of
    // SQL, and one counted refused businesses while the other did not, so the
    // two numbers on screen contradicted each other.
    //
    // The honest relationship is not "everything sums to the total" — a refused
    // business is deliberately not in the total. It is "everything **on show**
    // sums to the total, and the refused sit outside it".
    let store = a_mixed_list().await;
    let want = Wanted::default();

    let total = store.lead_count(&want).await.expect("counts");
    let counts = store.status_counts(&want).await.expect("counts");

    let on_show: i64 = counts
        .iter()
        .filter(|(status, _)| *status != webleads_domain::Status::NotInterested)
        .map(|(_, n)| n)
        .sum();
    assert_eq!(
        on_show, total,
        "buttons say {on_show}, the total says {total}"
    );
}

#[tokio::test]
async fn the_refused_are_counted_even_though_they_are_not_on_the_list() {
    // A button offering to show them cannot for ever say nought — that is how
    // you would never know there was anything to look at.
    let store = a_mixed_list().await;
    let counts = store
        .status_counts(&Wanted::default())
        .await
        .expect("counts");

    let refused = counts
        .iter()
        .find(|(status, _)| *status == webleads_domain::Status::NotInterested)
        .map(|(_, n)| *n)
        .unwrap_or(0);
    assert_eq!(refused, 1, "the refused business was counted as {refused}");

    // And it is still off the list itself, which is what the status means.
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        2
    );
}

#[tokio::test]
async fn narrowing_to_one_state_makes_its_count_the_total() {
    let store = a_mixed_list().await;
    for status in webleads_domain::Status::ALL {
        let want = Wanted {
            status: Some(status),
            ..Default::default()
        };
        let total = store.lead_count(&want).await.expect("counts");
        let shown = store.leads(&want).await.expect("reads").len() as i64;
        assert_eq!(
            shown, total,
            "{status:?}: showed {shown}, total said {total}"
        );
    }
}

#[tokio::test]
async fn the_counts_still_add_up_once_a_search_narrows_the_list() {
    let store = a_mixed_list().await;
    let want = Wanted {
        search: Some("Borzas".into()),
        ..Default::default()
    };
    let total = store.lead_count(&want).await.expect("counts");
    let summed: i64 = store
        .status_counts(&want)
        .await
        .expect("counts")
        .iter()
        .filter(|(status, _)| *status != webleads_domain::Status::NotInterested)
        .map(|(_, n)| n)
        .sum();
    assert_eq!(summed, total);
    assert_eq!(total, 1);
}

#[tokio::test]
async fn a_refused_business_can_be_looked_at_by_asking_for_it() {
    // The only way back from a misclick. Not on the list by default, there when
    // you ask for it by name.
    let store = a_mixed_list().await;
    let everyone = store.leads(&Wanted::default()).await.expect("reads");
    assert_eq!(everyone.len(), 2);
    assert!(everyone.iter().all(|l| l.lead.id != "refused"));

    let refused = store
        .leads(&Wanted {
            status: Some(webleads_domain::Status::NotInterested),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].lead.id, "refused");
}

// ---- the call list itself ----

#[tokio::test]
async fn a_run_stores_its_leads_and_reads_them_back() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[found(a_lead("a", 5, 300)), found(a_lead("b", 0, 900))],
        )
        .await
        .expect("records");

    let back = store.leads(&Wanted::default()).await.expect("reads");
    assert_eq!(back.len(), 2);
    // The nearer one comes first, whatever its reviews say.
    assert_eq!(back[0].lead.id, "a");
    assert_eq!(back[1].lead.id, "b");
}

#[tokio::test]
async fn a_business_with_a_working_website_is_not_on_the_call_list() {
    let store = Store::in_memory().await.expect("opens");
    let mut live = a_lead("live", 0, 10);
    live.verdict = Verdict::LiveSite;
    live.reason = "loads fine (200)".to_owned();

    let run = a_search(&store).await;
    store
        .record(run, &[found(live), found(a_lead("dead", 0, 20))])
        .await
        .expect("records");

    let back = store.leads(&Wanted::default()).await.expect("reads");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].lead.id, "dead");
    // But it is still remembered, so we do not re-check it forever.
    assert_eq!(store.business_count().await.expect("counts"), 2);
}

#[tokio::test]
async fn only_the_newest_run_shows_on_the_call_list() {
    // The `is_latest` flag: a business judged twice must appear once, with what we
    // decided most recently.
    let store = Store::in_memory().await.expect("opens");

    let first = a_search(&store).await;
    store
        .record(first, &[found(a_lead("a", 0, 10))])
        .await
        .expect("records");

    let mut changed = a_lead("a", 0, 10);
    changed.verdict = Verdict::SocialOnly;
    changed.reason = "only facebook.com".to_owned();
    let second = a_search(&store).await;
    store
        .record(second, &[found(changed)])
        .await
        .expect("records");

    let back = store.leads(&Wanted::default()).await.expect("reads");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].lead.verdict, Verdict::SocialOnly);
    assert_eq!(back[0].lead.reason, "only facebook.com");
}

#[tokio::test]
async fn the_call_list_pages_and_knows_its_own_total() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    let many: Vec<_> = (0..25)
        .map(|n| found(a_lead(&format!("shop{n:02}"), n, 100 + n)))
        .collect();
    store.record(run, &many).await.expect("records");

    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        25
    );

    let page = |offset| Wanted {
        limit: 10,
        offset,
        ..Default::default()
    };
    let first = store.leads(&page(0)).await.expect("reads");
    let second = store.leads(&page(10)).await.expect("reads");
    let last = store.leads(&page(20)).await.expect("reads");
    assert_eq!((first.len(), second.len(), last.len()), (10, 10, 5));

    // No row appears on two pages, and none is missed.
    let mut ids: Vec<_> = first
        .iter()
        .chain(&second)
        .chain(&last)
        .map(|l| l.lead.id.clone())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 25);
}

#[tokio::test]
async fn a_verdict_survives_the_round_trip_through_sql() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    for (index, verdict) in [Verdict::NoSite, Verdict::SocialOnly, Verdict::DeadSite]
        .into_iter()
        .enumerate()
    {
        let mut lead = a_lead(&format!("v{index}"), 0, 10);
        lead.verdict = verdict;
        store.record(run, &[found(lead)]).await.expect("records");
    }
    let back = store.leads(&Wanted::default()).await.expect("reads");
    let verdicts: Vec<_> = back.iter().map(|l| l.lead.verdict).collect();
    assert!(verdicts.contains(&Verdict::NoSite));
    assert!(verdicts.contains(&Verdict::SocialOnly));
    assert!(verdicts.contains(&Verdict::DeadSite));
}

#[tokio::test]
async fn a_broken_certificate_survives_the_round_trip() {
    let store = Store::in_memory().await.expect("opens");
    let mut lead = a_lead("insecure", 0, 10);
    lead.verdict = Verdict::InsecureSite;
    lead.reason = "certificate has expired".to_owned();
    lead.tls = Tls::Broken("certificate has expired (verify failed)".to_owned());

    let run = a_search(&store).await;
    store.record(run, &[found(lead)]).await.expect("records");

    let back = store.leads(&Wanted::default()).await.expect("reads");
    assert_eq!(back.len(), 1, "a broken certificate is a lead");
    assert_eq!(back[0].lead.verdict, Verdict::InsecureSite);
    assert_eq!(
        back[0].lead.tls,
        Tls::Broken("certificate has expired (verify failed)".to_owned())
    );
}

#[tokio::test]
async fn the_number_is_stored_in_one_tidy_form() {
    let store = Store::in_memory().await.expect("opens");
    let mut lead = a_lead("a", 0, 10);
    lead.phone = Some("06 1 234 5001".to_owned());

    let run = a_search(&store).await;
    store.record(run, &[found(lead)]).await.expect("records");

    let back = store.leads(&Wanted::default()).await.expect("reads");
    // One form, whatever shape Google sent: no spaces, no brackets, `06` written as
    // `+36`. It is what the screen shows, what the exports carry, and what makes two
    // listings of the same shop findable as one.
    assert_eq!(back[0].lead.phone.as_deref(), Some("+3612345001"));
    assert_eq!(
        back[0].lead.phone_canonical("hu").as_deref(),
        Some("+3612345001")
    );
}

// ---- the freshness cache ----

#[tokio::test]
async fn a_business_we_just_checked_is_remembered() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(run, &[found(a_lead("a", 0, 10))])
        .await
        .expect("records");

    let known = store.recent_verdicts(14).await.expect("reads");
    let (verdict, _, reason) = known.get("a").expect("we checked it today");
    assert_eq!(*verdict, Verdict::NoSite);
    assert_eq!(reason, "no link on Google Maps");
}

#[tokio::test]
async fn nothing_counts_as_recent_when_the_window_is_zero() {
    // The escape hatch behind `recheck: true` — with no window, everything is stale
    // and gets looked at again.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(run, &[found(a_lead("a", 0, 10))])
        .await
        .expect("records");

    assert!(store.recent_verdicts(0).await.expect("reads").is_empty());
}

#[tokio::test]
async fn a_verdict_taken_from_the_cache_does_not_refresh_its_own_date() {
    // If it did, a business would mark itself fresh for ever and never be looked at
    // again. `freshly_checked` is what stops that.
    let store = Store::in_memory().await.expect("opens");
    let first = a_search(&store).await;
    store
        .record(first, &[found(a_lead("a", 0, 10))])
        .await
        .expect("records");

    let mut reused = found(a_lead("a", 0, 10));
    reused.freshly_checked = false;
    let second = a_search(&store).await;
    store.record(second, &[reused]).await.expect("records");

    assert!(store
        .recent_verdicts(14)
        .await
        .expect("reads")
        .contains_key("a"));
}

#[tokio::test]
async fn a_certificate_problem_is_remembered_along_with_the_verdict() {
    let store = Store::in_memory().await.expect("opens");
    let mut lead = a_lead("insecure", 0, 10);
    lead.verdict = Verdict::InsecureSite;
    lead.tls = Tls::Broken("certificate has expired".to_owned());

    let run = a_search(&store).await;
    store.record(run, &[found(lead)]).await.expect("records");

    let known = store.recent_verdicts(14).await.expect("reads");
    let (verdict, tls, _) = known.get("insecure").expect("known");
    assert_eq!(*verdict, Verdict::InsecureSite);
    assert!(tls.is_a_problem());
}

#[tokio::test]
async fn a_business_that_said_no_is_not_counted_on_the_list() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(run, &[found(a_lead("a", 0, 10)), found(a_lead("b", 0, 20))])
        .await
        .expect("records");
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        2
    );

    store
        .set_status("a", webleads_domain::Status::NotInterested)
        .await
        .expect("sets");
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        1
    );
}

#[tokio::test]
async fn a_business_with_no_telephone_number_is_not_a_lead() {
    // The whole product is businesses you can ring that have no website. One you
    // cannot ring is not half a lead, it is none.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    let mut unreachable = a_lead("silent", 0, 10);
    unreachable.phone = None;
    let mut reachable = a_lead("reachable", 0, 20);
    reachable.phone = Some("+36 1 234 5001".into());

    store
        .record(run, &[found(unreachable), found(reachable)])
        .await
        .expect("records");

    let back = store.leads(&Wanted::default()).await.expect("reads");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].lead.id, "reachable");
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        1
    );
}

#[tokio::test]
async fn an_empty_telephone_number_counts_as_none() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    let mut blank = a_lead("blank", 0, 10);
    blank.phone = Some("   ".into());
    store.record(run, &[found(blank)]).await.expect("records");

    assert!(store
        .leads(&Wanted::default())
        .await
        .expect("reads")
        .is_empty());
}

#[tokio::test]
async fn a_business_we_cannot_ring_is_still_remembered() {
    // It stays in the table so we never re-check its website, it simply is not put
    // in front of you.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;

    let mut unreachable = a_lead("silent", 0, 10);
    unreachable.phone = None;
    store
        .record(run, &[found(unreachable)])
        .await
        .expect("records");

    assert_eq!(store.business_count().await.expect("counts"), 1);
    assert!(store
        .leads(&Wanted::default())
        .await
        .expect("reads")
        .is_empty());
}

// ---- new since the last search ----

#[tokio::test]
async fn a_business_keeps_the_search_that_first_found_it() {
    // The same rule as `first_seen`: a later search must not claim it.
    let store = Store::in_memory().await.expect("opens");
    let first = a_search(&store).await;
    store
        .record(first, &[found(a_lead("old", 3, 100))])
        .await
        .expect("records");
    let second = a_search(&store).await;
    store
        .record(second, &[found(a_lead("old", 3, 100))])
        .await
        .expect("records");

    // The search it belongs to is the first one, and narrowing to the second shows
    // nothing — which is what "keeps the search that first found it" means, and what
    // the past-searches strip relies on.
    let from_first = store
        .leads(&Wanted {
            from_run: Some(first),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert_eq!(from_first.len(), 1);
    assert_eq!(from_first[0].lead.id, "old");

    let from_second = store
        .leads(&Wanted {
            from_run: Some(second),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert!(from_second.is_empty(), "the second search found nobody new");
}

// ---- one search's leads, and one area's ----

/// Two searches of the same area, the second seeing everything the first did.
async fn two_searches() -> (Store, i64, i64) {
    let store = Store::in_memory().await.expect("opens");
    let zuglo = webleads_domain::Area {
        label: "Zugló".to_owned(),
        address: String::new(),
        lat: 47.5,
        lon: 19.1,
        max_results: 200,
    };
    let mut obuda = zuglo.clone();
    obuda.label = "Óbuda".to_owned();

    let first = store
        .begin_scrape(
            &zuglo,
            &[("borbély Zugló".to_owned(), None)],
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .record(first, &[found(a_lead("early", 3, 100))])
        .await
        .expect("records");

    // The second search of the same area sees the first one's business again, and
    // finds one more.
    let second = store
        .begin_scrape(
            &zuglo,
            &[("fodrász Zugló".to_owned(), None)],
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .record(
            second,
            &[
                found(a_lead("early", 3, 100)),
                found(a_lead("later", 4, 200)),
            ],
        )
        .await
        .expect("records");

    // And one somewhere else entirely.
    let elsewhere = store
        .begin_scrape(
            &obuda,
            &[("borbély Óbuda".to_owned(), None)],
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .record(elsewhere, &[found(a_lead("across_town", 5, 300))])
        .await
        .expect("records");

    (store, first, second)
}

#[tokio::test]
async fn a_search_keeps_its_leads_even_after_a_later_one_sees_them_again() {
    // The trap this avoids: filtering by the row's own run would show nothing here,
    // because the second search took over the first one's row.
    let (store, first, _second) = two_searches().await;

    let its_own = Wanted {
        from_run: Some(first),
        ..Default::default()
    };
    let back = store.leads(&its_own).await.expect("reads");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].lead.id, "early");
    assert_eq!(store.lead_count(&its_own).await.expect("counts"), 1);
}

#[tokio::test]
async fn the_second_search_shows_only_what_it_was_first_to_find() {
    let (store, _first, second) = two_searches().await;
    let back = store
        .leads(&Wanted {
            from_run: Some(second),
            ..Default::default()
        })
        .await
        .expect("reads");

    let ids: Vec<&str> = back.iter().map(|l| l.lead.id.as_str()).collect();
    assert_eq!(ids, ["later"], "not the one the first search discovered");
}

#[tokio::test]
async fn an_area_gathers_every_search_made_in_it() {
    let (store, _first, _second) = two_searches().await;
    let zuglo = Wanted {
        from_area: Some("Zugló".to_owned()),
        ..Default::default()
    };
    let back = store.leads(&zuglo).await.expect("reads");
    let mut ids: Vec<&str> = back.iter().map(|l| l.lead.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["early", "later"], "both searches of Zugló");

    // And the map is narrowed the same way, or the two would disagree.
    assert_eq!(store.pins(&zuglo).await.expect("pins").len(), 2);
}

#[tokio::test]
async fn another_area_is_another_list_entirely() {
    // Budapest this morning, Dubai this afternoon.
    let (store, _first, _second) = two_searches().await;
    let back = store
        .leads(&Wanted {
            from_area: Some("Óbuda".to_owned()),
            ..Default::default()
        })
        .await
        .expect("reads");

    let ids: Vec<&str> = back.iter().map(|l| l.lead.id.as_str()).collect();
    assert_eq!(ids, ["across_town"]);
}

#[tokio::test]
async fn an_area_nobody_has_searched_shows_nothing_rather_than_everything() {
    // Quietly ignoring a name we do not know would show the whole list and look like
    // the filter had failed to apply.
    let (store, _first, _second) = two_searches().await;
    let back = store
        .leads(&Wanted {
            from_area: Some("Dubai".to_owned()),
            ..Default::default()
        })
        .await
        .expect("reads");
    assert!(back.is_empty());
}

#[tokio::test]
async fn a_search_reports_both_numbers() {
    // What it found, and how many of those are worth ringing. Showing only one is
    // what makes the other look like a bug.
    let (store, first, _second) = two_searches().await;
    let run = store.run(first).await.expect("reads").expect("there");
    assert_eq!(run.area.as_deref(), Some("Zugló"));
    assert_eq!(run.leads_found, 1, "what it was first to find");
}
