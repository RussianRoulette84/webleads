//! Tests for the coming-up panel's wording.

use super::*;

#[test]
fn one_call_is_said_in_the_singular() {
    assert_eq!(describe_due(1, 0, 0), "1 call");
    assert_eq!(describe_due(4, 0, 0), "4 calls");
}

#[test]
fn what_is_late_is_said_out_loud() {
    assert_eq!(describe_due(3, 2, 0), "3 calls, 2 late");
    assert_eq!(describe_due(1, 1, 0), "1 call, 1 late");
}

#[test]
fn the_calls_you_are_waiting_on_are_counted_separately() {
    assert_eq!(describe_due(5, 0, 2), "5 calls, 2 they ring");
    assert_eq!(describe_due(5, 1, 2), "5 calls, 1 late, 2 they ring");
    assert_eq!(describe_due(1, 1, 1), "1 call, 1 late, 1 they ring");
}

#[test]
fn a_number_is_stripped_down_before_it_is_dialled() {
    // Shown as a person reads it, dialled as a telephone needs it.
    assert_eq!(dial("+36 1 234 5678"), "+3612345678");
    assert_eq!(dial("(06 1) 234-5678"), "0612345678");
    assert_eq!(dial(""), "");
}
