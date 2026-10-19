//! Tests for [`crate::contacts`].

use crate::filter::Wanted;
use crate::testing::*;
use crate::Store;
use webleads_domain::{Status, GIVE_UP_AFTER};

/// A store with one business already on the call list.
async fn with_one() -> Store {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(run, &[found(a_lead("a", 0, 10))])
        .await
        .expect("records");
    store
}

/// Everything on the list, however many there are.
async fn all_of_it(store: &Store) -> Vec<crate::StoredLead> {
    store
        .leads(&Wanted {
            limit: 100,
            offset: 0,
            ..Default::default()
        })
        .await
        .expect("reads")
}

#[tokio::test]
async fn a_business_nobody_has_touched_is_one_to_call() {
    let store = with_one().await;
    let contact = store.contact("a").await.expect("reads");
    assert_eq!(contact.status, Status::NeedCall);
    assert_eq!(contact.note, "");
    assert_eq!(contact.called_on, None);
    assert_eq!(contact.not_reached_counter, 0);
    assert_eq!(contact.follow_up_tstamp, None);
}

#[tokio::test]
async fn a_business_we_have_never_heard_of_is_also_one_to_call() {
    // No row rather than an error — the absence of a contact means "not touched".
    let store = with_one().await;
    assert_eq!(
        store.contact("nobody").await.expect("reads").status,
        Status::NeedCall
    );
}

#[tokio::test]
async fn every_one_of_the_six_states_survives_being_written_down() {
    let store = with_one().await;
    for status in Status::ALL {
        let back = store.set_status("a", status).await.expect("sets");
        // "No answer" counts, so the fifth press of it gives up on the number —
        // which is the one state that can answer with something else.
        if status != Status::NotReached {
            assert_eq!(back.status, status, "{status:?} did not hold");
        }
        assert_eq!(store.contact("a").await.expect("reads").status, back.status);
    }
}

#[tokio::test]
async fn saying_nobody_answered_counts_it_and_writes_the_call_down() {
    let store = with_one().await;
    let back = store
        .set_status("a", Status::NotReached)
        .await
        .expect("sets");
    assert_eq!(back.status, Status::NotReached);
    assert_eq!(back.not_reached_counter, 1);
    assert!(back.called_on.is_some(), "the day you rang must be kept");

    let again = store
        .set_status("a", Status::NotReached)
        .await
        .expect("sets");
    assert_eq!(again.not_reached_counter, 2);
    // Pressing the same state again is a second call, because it is a second ring.
    assert_eq!(store.calls("a").await.expect("reads").len(), 2);
}

#[tokio::test]
async fn the_fifth_unanswered_call_gives_up_on_the_number() {
    let store = with_one().await;
    for _ in 0..GIVE_UP_AFTER {
        let back = store
            .set_status("a", Status::NotReached)
            .await
            .expect("sets");
        assert_eq!(back.status, Status::NotReached);
    }
    let dead = store
        .set_status("a", Status::NotReached)
        .await
        .expect("sets");
    assert_eq!(dead.status, Status::Dead);
    assert_eq!(dead.not_reached_counter, GIVE_UP_AFTER + 1);
    // And a dead number is off the call list.
    assert!(all_of_it(&store).await.is_empty());
}

#[tokio::test]
async fn correcting_the_count_moves_the_state_and_records_no_call() {
    let store = with_one().await;
    store
        .set_status("a", Status::NotReached)
        .await
        .expect("sets");
    let calls = store.calls("a").await.expect("reads").len();

    let dead = store.set_no_answers("a", 9).await.expect("sets");
    assert_eq!(dead.status, Status::Dead);
    assert_eq!(dead.not_reached_counter, 9);

    let back = store.set_no_answers("a", 1).await.expect("sets");
    assert_eq!(back.status, Status::NotReached);
    // Correcting the record is not making a telephone call.
    assert_eq!(store.calls("a").await.expect("reads").len(), calls);
}

#[tokio::test]
async fn a_count_outside_what_we_would_store_is_brought_back_in() {
    let store = with_one().await;
    assert_eq!(
        store
            .set_no_answers("a", -5)
            .await
            .expect("sets")
            .not_reached_counter,
        0
    );
    assert_eq!(
        store
            .set_no_answers("a", 9_999)
            .await
            .expect("sets")
            .not_reached_counter,
        99
    );
}

#[tokio::test]
async fn going_back_to_to_call_keeps_the_calls_you_have_already_made() {
    // Going back keeps them: the counters depend on the calls, and losing them would
    // be losing data to undo a misclick.
    let store = with_one().await;
    store
        .set_status("a", Status::NotReached)
        .await
        .expect("sets");
    store.set_status("a", Status::NeedCall).await.expect("sets");

    let contact = store.contact("a").await.expect("reads");
    assert_eq!(contact.status, Status::NeedCall);
    assert!(
        contact.called_on.is_some(),
        "the call happened; forgetting it is losing data"
    );
    assert_eq!(store.calls("a").await.expect("reads").len(), 1);
}

#[tokio::test]
async fn a_time_to_ring_back_is_kept_and_puts_the_business_in_that_state() {
    let store = with_one().await;
    let back = store
        .set_ring_back("a", Some("2026-09-21 14:30"))
        .await
        .expect("sets");
    assert_eq!(back.status, Status::FollowUp);
    assert_eq!(back.follow_up_tstamp.as_deref(), Some("2026-09-21 14:30"));

    // Clearing the time leaves the state alone: you have still promised to ring.
    let cleared = store.set_ring_back("a", None).await.expect("sets");
    assert_eq!(cleared.status, Status::FollowUp);
    assert_eq!(cleared.follow_up_tstamp, None);
}

#[tokio::test]
async fn leaving_ring_back_clears_the_time_with_it() {
    // Otherwise a business you have sold to sits in the coming-up panel for ever.
    let store = with_one().await;
    store
        .set_ring_back("a", Some("2026-09-21 14:30"))
        .await
        .expect("sets");
    let sold = store.set_status("a", Status::Sold).await.expect("sets");
    assert_eq!(sold.follow_up_tstamp, None);
}

#[tokio::test]
async fn the_coming_up_list_is_soonest_first_and_says_what_is_late() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                found(a_lead("soon", 0, 10)),
                found(a_lead("later", 1, 20)),
                found(a_lead("untouched", 2, 30)),
            ],
        )
        .await
        .expect("records");
    // One in the past, one a long way off.
    store
        .set_ring_back("soon", Some("2020-01-01 09:00"))
        .await
        .expect("sets");
    store
        .set_ring_back("later", Some("2099-01-01 09:00"))
        .await
        .expect("sets");

    let due = store.coming_up(50).await.expect("reads");
    assert_eq!(due.len(), 2, "only the ones with a time on them");
    assert_eq!(due[0].id, "soon");
    assert!(due[0].overdue, "a call from 2020 is late");
    assert_eq!(due[1].id, "later");
    assert!(!due[1].overdue);
}

#[tokio::test]
async fn a_call_they_promised_is_coming_up_too_and_says_it_is_theirs() {
    let store = with_one().await;
    store.set_status("a", Status::TheyCall).await.expect("sets");
    store
        .set_ring_back("a", Some("2099-01-01 09:00"))
        .await
        .expect("sets");

    let due = store.coming_up(50).await.expect("reads");
    assert_eq!(due.len(), 1);
    assert!(due[0].theirs, "they said they would ring");
}

#[tokio::test]
async fn a_call_you_promised_is_yours_to_make() {
    let store = with_one().await;
    store
        .set_ring_back("a", Some("2099-01-01 09:00"))
        .await
        .expect("sets");
    let due = store.coming_up(50).await.expect("reads");
    assert!(!due[0].theirs);
}

#[tokio::test]
async fn a_business_you_have_finished_with_is_not_coming_up_any_more() {
    let store = with_one().await;
    store
        .set_ring_back("a", Some("2099-01-01 09:00"))
        .await
        .expect("sets");
    assert_eq!(store.coming_up(50).await.expect("reads").len(), 1);

    store.set_status("a", Status::Sold).await.expect("sets");
    assert!(store.coming_up(50).await.expect("reads").is_empty());
}

#[tokio::test]
async fn a_note_is_kept_and_can_be_changed() {
    let store = with_one().await;
    store
        .set_note("a", "call back after five")
        .await
        .expect("sets");
    assert_eq!(
        store.contact("a").await.expect("reads").note,
        "call back after five"
    );

    store
        .set_note("a", "ask for the owner")
        .await
        .expect("sets");
    assert_eq!(
        store.contact("a").await.expect("reads").note,
        "ask for the owner"
    );
}

#[tokio::test]
async fn a_note_and_a_state_do_not_overwrite_each_other() {
    // They are written by two different controls and share one row, so this is the
    // obvious thing to get wrong — and the count and the time now share it too.
    let store = with_one().await;
    store
        .set_status("a", Status::NotReached)
        .await
        .expect("sets");
    store
        .set_note("a", "no answer, try Tuesday")
        .await
        .expect("sets");

    let contact = store.contact("a").await.expect("reads");
    assert_eq!(contact.status, Status::NotReached);
    assert_eq!(contact.not_reached_counter, 1);
    assert_eq!(contact.note, "no answer, try Tuesday");
    assert!(contact.called_on.is_some());
}

#[tokio::test]
async fn a_business_that_said_no_disappears_from_the_call_list() {
    // The whole point of the state. It must not come back on the next import.
    let store = with_one().await;
    assert_eq!(all_of_it(&store).await.len(), 1);

    store
        .set_status("a", Status::NotInterested)
        .await
        .expect("sets");
    assert!(all_of_it(&store).await.is_empty());
    assert_eq!(
        store.lead_count(&Wanted::default()).await.expect("counts"),
        0
    );

    // Re-importing the same business must not bring it back.
    let run = a_search(&store).await;
    store
        .record(run, &[found(a_lead("a", 0, 10))])
        .await
        .expect("records");
    assert!(all_of_it(&store).await.is_empty());
}

#[tokio::test]
async fn the_three_states_that_are_still_work_stay_on_the_list() {
    let store = with_one().await;
    for status in [Status::NeedCall, Status::NotReached, Status::FollowUp] {
        store.set_status("a", status).await.expect("sets");
        // Start each one fresh, or the count would carry over and give up.
        store.set_no_answers("a", 0).await.expect("sets");
        assert_eq!(all_of_it(&store).await.len(), 1, "{status:?} fell off");
    }
    for status in [Status::Sold, Status::NotInterested, Status::Dead] {
        store.set_status("a", status).await.expect("sets");
        assert!(all_of_it(&store).await.is_empty(), "{status:?} stayed on");
    }
}

#[tokio::test]
async fn the_list_can_be_narrowed_to_one_state() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                found(a_lead("fresh", 0, 10)),
                found(a_lead("rung", 1, 20)),
                found(a_lead("won", 2, 30)),
            ],
        )
        .await
        .expect("records");
    store
        .set_status("rung", Status::NotReached)
        .await
        .expect("sets");
    store.set_status("won", Status::Sold).await.expect("sets");

    let to_call = store
        .leads(&Wanted {
            status: Some(Status::NeedCall),
            limit: 100,
            offset: 0,
            ..Default::default()
        })
        .await
        .expect("reads");
    assert_eq!(to_call.len(), 1);
    assert_eq!(to_call[0].lead.id, "fresh");

    let sold = store
        .leads(&Wanted {
            status: Some(Status::Sold),
            limit: 100,
            offset: 0,
            ..Default::default()
        })
        .await
        .expect("reads");
    assert_eq!(sold.len(), 1);
    assert_eq!(sold[0].lead.id, "won");

    // Everybody still worth ringing: the sale is finished with.
    assert_eq!(all_of_it(&store).await.len(), 2);
}

#[tokio::test]
async fn the_counts_say_how_many_are_in_each_state() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                found(a_lead("a", 0, 10)),
                found(a_lead("b", 1, 20)),
                found(a_lead("c", 2, 30)),
            ],
        )
        .await
        .expect("records");
    store
        .set_status("b", Status::NotReached)
        .await
        .expect("sets");

    let counts = store
        .status_counts(&Wanted::default())
        .await
        .expect("counts");
    let find = |want: Status| {
        counts
            .iter()
            .find(|(s, _)| *s == want)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    };
    // Businesses with no contact row count as ones to call, which is most of them.
    assert_eq!(find(Status::NeedCall), 2);
    assert_eq!(find(Status::NotReached), 1);
}

#[tokio::test]
async fn the_counts_come_back_in_the_order_we_show_them() {
    let store = with_one().await;
    store.set_status("a", Status::Sold).await.expect("sets");
    let counts = store
        .status_counts(&Wanted::default())
        .await
        .expect("counts");
    let order: Vec<_> = counts.iter().map(|(s, _)| *s).collect();
    let mut sorted = order.clone();
    sorted.sort_by_key(|s| Status::ALL.iter().position(|a| a == s).unwrap_or(9));
    assert_eq!(order, sorted);
}

// ---- the options that belong to a state ----

#[tokio::test]
async fn the_list_can_be_narrowed_to_the_often_rung() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[found(a_lead("once", 0, 10)), found(a_lead("thrice", 1, 20))],
        )
        .await
        .expect("records");
    store.set_no_answers("once", 1).await.expect("sets");
    store.set_no_answers("thrice", 3).await.expect("sets");

    let rung_at_least = |least: i64| Wanted {
        rings_at_least: Some(least),
        limit: 100,
        ..Default::default()
    };
    let three = store.leads(&rung_at_least(3)).await.expect("reads");
    assert_eq!(three.len(), 1);
    assert_eq!(three[0].lead.id, "thrice");
    // Everyone rung at all.
    assert_eq!(
        store.leads(&rung_at_least(1)).await.expect("reads").len(),
        2
    );
    // And nobody has been rung four times.
    assert!(store
        .leads(&rung_at_least(4))
        .await
        .expect("reads")
        .is_empty());
}

#[tokio::test]
async fn the_list_can_be_narrowed_to_what_is_due_by_a_moment() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(
            run,
            &[
                found(a_lead("soon", 0, 10)),
                found(a_lead("later", 1, 20)),
                found(a_lead("untouched", 2, 30)),
            ],
        )
        .await
        .expect("records");
    store
        .set_ring_back("soon", Some("2026-09-20 09:00"))
        .await
        .expect("sets");
    store
        .set_ring_back("later", Some("2026-12-24 09:00"))
        .await
        .expect("sets");

    let due_by = |when: &str| Wanted {
        due_before: Some(when.to_owned()),
        limit: 100,
        ..Default::default()
    };
    let overdue = store
        .leads(&due_by("2026-09-20 12:00"))
        .await
        .expect("reads");
    assert_eq!(overdue.len(), 1, "only the one already past");
    assert_eq!(overdue[0].lead.id, "soon");

    // A business with no promised call is never due, however wide the window.
    let wide = store
        .leads(&due_by("2099-01-01 00:00"))
        .await
        .expect("reads");
    assert_eq!(wide.len(), 2);
    assert!(wide.iter().all(|l| l.lead.id != "untouched"));
}

#[tokio::test]
async fn a_moment_we_did_not_write_narrows_nothing_rather_than_reaching_the_query() {
    // Belt and braces: the value comes from our own clock, and anything else is
    // ignored instead of being pasted into the SQL.
    let store = with_one().await;
    let odd = Wanted {
        due_before: Some("' OR 1=1 --".to_owned()),
        limit: 100,
        ..Default::default()
    };
    assert_eq!(store.leads(&odd).await.expect("reads").len(), 1);
}

/// Every chip's own count, against the list narrowed to that one state.
///
/// The right comparison: the chips deliberately include the businesses that said no,
/// which the list leaves out — so the *totals* differ on purpose and only the
/// per-state numbers can be checked against each other.
async fn chips_agree_with_the_list(store: &Store, want: &Wanted) {
    for (status, counted) in store.status_counts(want).await.expect("counts") {
        let only_that = Wanted {
            status: Some(status),
            ..want.clone()
        };
        assert_eq!(
            counted,
            store.lead_count(&only_that).await.expect("counts"),
            "the {status:?} chip disagrees with the list"
        );
    }
}

#[tokio::test]
async fn the_state_counts_follow_the_sliders() {
    // The test that would have caught it: the chips bound three of the eight
    // parameters, so the five sliders and dropdowns did nothing to them. The chips
    // said 2504 to call while the list showed 127.
    let store = a_mixed_list().await;
    chips_agree_with_the_list(&store, &Wanted::default()).await;

    let busy = Wanted {
        least_reviews: Some(2),
        ..Default::default()
    };
    chips_agree_with_the_list(&store, &busy).await;

    // And it really did narrow something, or the test proves nothing.
    let before: i64 = store
        .status_counts(&Wanted::default())
        .await
        .expect("counts")
        .iter()
        .map(|(_, n)| n)
        .sum();
    let after: i64 = store
        .status_counts(&busy)
        .await
        .expect("counts")
        .iter()
        .map(|(_, n)| n)
        .sum();
    assert!(after < before, "{after} is not fewer than {before}");
}

#[tokio::test]
async fn the_state_counts_follow_the_trade_the_distance_and_the_reason() {
    let store = a_mixed_list().await;
    for narrowed in [
        Wanted {
            trade: Some("Barber shop".to_owned()),
            ..Default::default()
        },
        Wanted {
            within_m: Some(15),
            ..Default::default()
        },
        Wanted {
            least_rating: Some(4.0),
            ..Default::default()
        },
        Wanted {
            why: Some(webleads_domain::Verdict::NoSite),
            ..Default::default()
        },
    ] {
        chips_agree_with_the_list(&store, &narrowed).await;
    }
}

#[tokio::test]
async fn changing_the_time_on_a_call_they_promised_does_not_make_it_one_you_owe() {
    // Setting a time normally means "ring back", because that is what writing one
    // down usually means. On a business that already said *they* would ring, it
    // must not quietly change whose move it is.
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(run, &[found(a_lead("one", 3, 100))])
        .await
        .expect("records");

    store
        .set_status("one", Status::TheyCall)
        .await
        .expect("marks");
    let after = store
        .set_ring_back("one", Some("2026-10-14 09:30"))
        .await
        .expect("sets a time");

    assert_eq!(after.status, Status::TheyCall);
    assert_eq!(after.follow_up_tstamp.as_deref(), Some("2026-10-14 09:30"));
}

#[tokio::test]
async fn a_call_they_promised_counts_as_having_rung_them() {
    let store = Store::in_memory().await.expect("opens");
    let run = a_search(&store).await;
    store
        .record(run, &[found(a_lead("one", 3, 100))])
        .await
        .expect("records");

    store
        .set_status("one", Status::TheyCall)
        .await
        .expect("marks");
    assert_eq!(store.calls("one").await.expect("reads").len(), 1);
}
