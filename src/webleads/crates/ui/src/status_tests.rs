//! Tests for the wording around the state picker.

use super::*;

#[test]
fn an_unanswered_call_is_counted_in_words() {
    assert_eq!(no_answers_said(1), "rung once, no answer");
    assert_eq!(no_answers_said(2), "rung 2 times, no answer");
    assert_eq!(no_answers_said(5), "rung 5 times, no answer");
}

#[test]
fn nothing_is_said_when_nothing_has_happened() {
    // A row that says "rung 0 times" is worse than a row that says nothing.
    assert_eq!(no_answers_said(0), "");
    assert_eq!(no_answers_said(-1), "");
}

#[test]
fn the_list_offers_every_state_and_no_others() {
    assert_eq!(Status::ALL.len(), 7);
    // Each one reads as something you would say out loud.
    for status in Status::ALL {
        let said = status.describe();
        assert!(!said.is_empty());
        assert!(
            said.chars()
                .all(|c| c.is_ascii_lowercase() || c == ' ' || c == '\''),
            "`{said}` is not plain words"
        );
    }
}

#[test]
fn the_call_they_promised_sits_under_the_one_you_promised() {
    let order: Vec<&str> = Status::ALL.iter().map(|s| s.describe()).collect();
    let ring_back = order.iter().position(|s| *s == "ring back").expect("there");
    let they_call = order
        .iter()
        .position(|s| *s == "they'll call")
        .expect("there");
    assert_eq!(they_call, ring_back + 1);
}

#[test]
fn a_stored_time_goes_into_the_browsers_own_field_unchanged() {
    // The field only understands its own shape, and a value it cannot read shows as
    // an empty field — which looks exactly like having lost the time.
    assert_eq!(ring_back_for_input("2026-09-21 14:30"), "2026-09-21T14:30");
}
