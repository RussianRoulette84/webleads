//! Tests for [`crate::hidden`].

use super::*;

#[test]
fn two_branches_of_one_chain_share_a_pattern() {
    assert_eq!(
        chain_pattern("Best Barbers Zugló"),
        chain_pattern("Best Barbers Óbuda")
    );
}

#[test]
fn a_different_chain_gets_a_different_pattern() {
    assert_ne!(
        chain_pattern("Best Barbers Zugló"),
        chain_pattern("Borzas Barber")
    );
}

#[test]
fn the_pattern_is_lower_case_because_the_names_are_not() {
    // Google sends whatever case it likes, so the comparison is done lowered.
    assert_eq!(chain_pattern("BEST Barbers"), chain_pattern("best barbers"));
    assert!(!chain_pattern("BEST Barbers").contains('B'));
}

#[test]
fn a_one_word_name_still_gets_a_pattern() {
    assert_eq!(chain_pattern("Pilla"), "%pilla%");
}

#[test]
fn a_percent_sign_in_a_name_is_looked_for_rather_than_matching_everything() {
    // Otherwise "100% Hajstúdió" would hide the entire list.
    let pattern = chain_pattern("100% Hajstúdió");
    assert!(pattern.contains("\\%"), "not escaped: {pattern}");
}

#[test]
fn punctuation_around_the_name_is_dropped() {
    assert_eq!(
        chain_pattern("  „Zugló Diva”  "),
        chain_pattern("Zugló Diva")
    );
}

#[test]
fn the_label_says_the_words_not_the_pattern() {
    // What you hid, in a form you can read back.
    assert_eq!(chain_label("Best Barbers Zugló"), "Best Barbers");
}

#[test]
fn an_empty_name_cannot_hide_everything() {
    // A blank pattern would match every row. It has to be refused higher up, and
    // this is the test that says so out loud.
    assert_eq!(chain_pattern("   "), "%%");
    assert!(chain_label("   ").is_empty());
}
