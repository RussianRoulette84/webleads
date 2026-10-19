//! Tests for [`crate::report`] — what the hit report says about each word.

use super::*;

fn word(state: &str, found: Option<i64>) -> Word {
    Word {
        phrase: "borbély Budapest".into(),
        trade: Some("Barbers".into()),
        state: state.into(),
        found,
        note: None,
    }
}

#[test]
fn a_word_that_found_nothing_is_said_loudly() {
    // Nought is the number worth acting on — it nearly always means a bad Hungarian
    // word rather than a trade nobody in Budapest practises.
    let (class, shown) = describe_hits(&word("done", Some(0)));
    assert_eq!(shown, "nothing");
    assert_eq!(class, "is-warn");
}

#[test]
fn a_word_that_found_businesses_is_said_plainly() {
    let (class, shown) = describe_hits(&word("done", Some(14)));
    assert_eq!(shown, "14");
    assert_eq!(class, "", "a good result should not shout");
}

#[test]
fn a_word_still_being_searched_says_so_rather_than_nothing() {
    // Otherwise a word in flight would look identical to one that found nothing.
    assert_eq!(describe_hits(&word("searching", None)).1, "searching…");
    assert_eq!(describe_hits(&word("waiting", None)).1, "waiting");
}

#[test]
fn a_failed_word_shows_why_it_failed() {
    let mut failed = word("failed", None);
    failed.note = Some("the search took too long".into());
    let (class, shown) = describe_hits(&failed);
    assert_eq!(shown, "the search took too long");
    assert_eq!(class, "is-bad");
}

#[test]
fn a_failed_word_with_no_reason_still_says_it_failed() {
    assert_eq!(describe_hits(&word("failed", None)).1, "failed");
}

// ---- the cost, before you commit ----
