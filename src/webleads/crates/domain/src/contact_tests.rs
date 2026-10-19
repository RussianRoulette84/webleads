//! Tests for the state of a lead.

use super::*;

#[test]
fn a_business_nobody_has_rung_is_where_everything_starts() {
    assert_eq!(Status::default(), Status::NeedCall);
    assert_eq!(Contact::default().status, Status::NeedCall);
    assert_eq!(Contact::default().note, "");
    assert_eq!(Contact::default().called_on, None);
    assert_eq!(Contact::default().not_reached_counter, 0);
    assert_eq!(Contact::default().follow_up_tstamp, None);
}

#[test]
fn three_states_are_work_you_still_have_and_three_are_finished() {
    assert!(Status::NeedCall.still_on_the_list());
    assert!(Status::NotReached.still_on_the_list());
    assert!(Status::FollowUp.still_on_the_list());
    assert!(Status::TheyCall.still_on_the_list());
    assert!(!Status::Sold.still_on_the_list());
    assert!(!Status::NotInterested.still_on_the_list());
    assert!(!Status::Dead.still_on_the_list());
}

#[test]
fn the_database_reads_the_list_rule_from_here() {
    // The words, not the labels — this is what goes into the query.
    let words = Status::words_on_the_list();
    assert_eq!(
        words,
        vec!["need_call", "not_reached", "follow_up", "they_call"]
    );
    // And it cannot drift from the rule above, whatever anybody adds later.
    for status in Status::ALL {
        assert_eq!(
            words.contains(&status.as_str()),
            status.still_on_the_list(),
            "{status:?} disagrees with itself"
        );
    }
}

#[test]
fn a_status_survives_being_written_down_and_read_back() {
    for status in Status::ALL {
        assert_eq!(
            status.as_str().parse::<Status>().expect("reads back"),
            status
        );
    }
}

#[test]
fn the_stored_word_and_the_shown_word_are_kept_apart() {
    // If these ever became one function, rewording a label would silently change
    // what is already written in the database.
    assert_eq!(Status::NeedCall.as_str(), "need_call");
    assert_eq!(Status::NeedCall.describe(), "to call");
    assert_eq!(Status::NotReached.as_str(), "not_reached");
    assert_eq!(Status::NotReached.describe(), "no answer");
    assert_eq!(Status::Dead.describe(), "line dead");
}

#[test]
fn a_word_that_is_not_a_status_is_refused_and_says_so() {
    let error = "banana".parse::<Status>().expect_err("refused");
    assert!(error.to_string().contains("banana"));
    assert!("".parse::<Status>().is_err());
    // The shown word must not be accepted as the stored one.
    assert!("no answer".parse::<Status>().is_err());
    assert!("to call".parse::<Status>().is_err());
    // Nor words that are not states at all.
    assert!("new".parse::<Status>().is_err());
    assert!("called".parse::<Status>().is_err());
}

#[test]
fn every_status_appears_exactly_once_in_the_list_we_show() {
    assert_eq!(Status::ALL.len(), 7);
    for status in Status::ALL {
        assert_eq!(
            Status::ALL.iter().filter(|s| **s == status).count(),
            1,
            "{status:?} appears more than once"
        );
    }
}

#[test]
fn sorting_by_state_puts_the_mornings_work_first() {
    let mut ordered = Status::ALL;
    ordered.sort_by_key(|s| s.work_left());
    assert_eq!(
        ordered.map(Status::describe),
        [
            "to call",
            "ring back",
            "they'll call",
            "no answer",
            "sold",
            "said no",
            "line dead"
        ]
    );
}

#[test]
fn no_two_states_claim_the_same_place_in_either_order() {
    // Two states with the same rank would sort — or merge — at random.
    let mut work: Vec<u8> = Status::ALL.iter().map(|s| s.work_left()).collect();
    work.sort_unstable();
    work.dedup();
    assert_eq!(work.len(), Status::ALL.len());

    let mut along: Vec<u8> = Status::ALL.iter().map(|s| s.progress()).collect();
    along.sort_unstable();
    along.dedup();
    assert_eq!(along.len(), Status::ALL.len());
}

#[test]
fn a_dead_number_loses_to_anybody_you_have_spoken_to() {
    assert!(Status::Sold.progress() > Status::NotInterested.progress());
    assert!(Status::NotInterested.progress() > Status::FollowUp.progress());
    assert!(Status::FollowUp.progress() > Status::NotReached.progress());
    assert!(Status::NotReached.progress() > Status::Dead.progress());
    // But it still beats a business nobody has touched at all.
    assert!(Status::Dead.progress() > Status::NeedCall.progress());
}

#[test]
fn the_fifth_unanswered_call_is_the_one_that_gives_up() {
    // Four is still worth another try.
    for rung in 0..GIVE_UP_AFTER {
        let (status, count) = after_no_answer(rung);
        assert_eq!(status, Status::NotReached, "after {} calls", rung + 1);
        assert_eq!(count, rung + 1);
    }
    // The fifth is not.
    let (status, count) = after_no_answer(GIVE_UP_AFTER);
    assert_eq!(status, Status::Dead);
    assert_eq!(count, GIVE_UP_AFTER + 1);
}

#[test]
fn correcting_the_count_by_hand_moves_the_state_both_ways() {
    // Stepping past the limit gives up.
    assert_eq!(with_no_answers(Status::NotReached, 9), Status::Dead);
    // Stepping back down takes it up again — the way back from a mistake.
    assert_eq!(with_no_answers(Status::Dead, 2), Status::NotReached);
    // And it never touches a state the count has nothing to do with.
    for status in [Status::Sold, Status::NotInterested, Status::FollowUp] {
        assert_eq!(with_no_answers(status, 99), status);
        assert_eq!(with_no_answers(status, 0), status);
    }
    // Nor does it invent a call that never happened.
    assert_eq!(with_no_answers(Status::NeedCall, 0), Status::NeedCall);
}

#[test]
fn a_count_that_has_gone_negative_still_counts_upwards() {
    // Nothing should write a negative count, but reading one back must not make
    // the next call subtract from it.
    let (status, count) = after_no_answer(-3);
    assert_eq!(count, 1);
    assert_eq!(status, Status::NotReached);
}

#[test]
fn the_browsers_own_date_field_is_tidied_into_what_we_store() {
    assert_eq!(
        tidy_ring_back("2026-09-21T14:30").as_deref(),
        Some("2026-09-21 14:30")
    );
    // Seconds on the end are dropped: we ring back at half past two, not at
    // half past two and nine seconds.
    assert_eq!(
        tidy_ring_back("2026-09-21T14:30:09").as_deref(),
        Some("2026-09-21 14:30")
    );
    // Already in our own shape, and padded where the browser was lazy.
    assert_eq!(
        tidy_ring_back(" 2026-9-1 9:05 ").as_deref(),
        Some("2026-09-01 09:05")
    );
}

#[test]
fn anything_that_is_not_a_time_is_refused_rather_than_stored() {
    for rubbish in [
        "",
        "tomorrow",
        "2026-09-21",
        "2026-09-21T25:00",
        "2026-09-21T10:74",
        "2026-13-21T10:00",
        "2026-09-41T10:00",
        "1899-09-21T10:00",
    ] {
        assert_eq!(tidy_ring_back(rubbish), None, "`{rubbish}` was accepted");
    }
}

#[test]
fn a_stored_time_goes_back_into_the_field_it_came_from() {
    assert_eq!(ring_back_for_input("2026-09-21 14:30"), "2026-09-21T14:30");
    let typed = "2026-09-21T14:30";
    let stored = tidy_ring_back(typed).expect("a time");
    assert_eq!(ring_back_for_input(&stored), typed);
}

#[test]
fn a_call_you_meant_to_make_yesterday_is_overdue() {
    let now = "2026-09-20 11:00";
    assert!(ring_back_overdue("2026-09-19 16:00", now));
    assert!(ring_back_overdue("2026-09-20 10:59", now));
    // The minute it comes due, it is due.
    assert!(ring_back_overdue("2026-09-20 11:00", now));
    assert!(!ring_back_overdue("2026-09-20 11:01", now));
    assert!(!ring_back_overdue("2026-10-01 09:00", now));
}

#[test]
fn a_contact_answers_for_its_own_status() {
    let refused = Contact {
        status: Status::NotInterested,
        note: "said no twice".into(),
        called_on: Some("2026-09-19".into()),
        not_reached_counter: 2,
        follow_up_tstamp: None,
    };
    assert!(!refused.still_on_the_list());
    assert!(Contact::default().still_on_the_list());
}

// ---- which of the promised calls to show ----

/// A moment in UTC, so the cut-off can be checked without waiting for a date.
fn utc(y: i32, m: u8, d: u8, h: u8, min: u8) -> time::OffsetDateTime {
    let month = time::Month::try_from(m).expect("month");
    let date = time::Date::from_calendar_date(y, month, d).expect("date");
    time::PrimitiveDateTime::new(date, time::Time::from_hms(h, min, 0).expect("time")).assume_utc()
}

#[test]
fn overdue_means_up_to_this_very_minute() {
    // 12:00 UTC is 14:00 in Budapest in September.
    assert_eq!(
        Due::Overdue.cut_off(utc(2026, 9, 20, 12, 0)),
        "2026-09-20 14:00"
    );
}

#[test]
fn today_runs_to_the_end_of_the_day_where_the_shop_is() {
    assert_eq!(
        Due::Today.cut_off(utc(2026, 9, 20, 12, 0)),
        "2026-09-20 23:59"
    );
    // Late in London is already tomorrow there, and tomorrow is what counts.
    assert_eq!(
        Due::Today.cut_off(utc(2026, 9, 20, 23, 30)),
        "2026-09-21 23:59"
    );
}

#[test]
fn a_week_is_seven_days_including_today() {
    assert_eq!(
        Due::Week.cut_off(utc(2026, 9, 20, 12, 0)),
        "2026-09-27 23:59"
    );
    // And it counts over the end of a month.
    assert_eq!(
        Due::Week.cut_off(utc(2026, 9, 28, 12, 0)),
        "2026-10-05 23:59"
    );
}

#[test]
fn the_cut_offs_widen_in_the_order_they_are_offered() {
    let now = utc(2026, 9, 20, 12, 0);
    let said: Vec<String> = Due::ALL.into_iter().map(|d| d.cut_off(now)).collect();
    let mut sorted = said.clone();
    sorted.sort();
    assert_eq!(said, sorted, "a wider option must not come back earlier");
}

#[test]
fn a_word_that_is_not_one_of_the_three_is_refused() {
    for word in Due::ALL {
        assert_eq!(word.as_str().parse::<Due>().expect("parses"), word);
    }
    assert!("soon".parse::<Due>().is_err());
    assert!("".parse::<Due>().is_err());
}

#[test]
fn the_two_states_with_a_time_are_the_two_where_somebody_promised_one() {
    assert!(Status::FollowUp.carries_a_time());
    assert!(Status::TheyCall.carries_a_time());
    for status in [
        Status::NeedCall,
        Status::NotReached,
        Status::Sold,
        Status::NotInterested,
        Status::Dead,
    ] {
        assert!(!status.carries_a_time(), "{status:?} has no time to show");
    }
}

#[test]
fn a_call_they_promised_is_still_a_call_that_happened() {
    // You spoke to somebody to be told they would ring back, so it counts as a
    // call — the same as ringing back does.
    assert_eq!("they_call".parse::<Status>().ok(), Some(Status::TheyCall));
    assert_eq!(Status::TheyCall.describe(), "they'll call");
}
