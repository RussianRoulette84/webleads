//! Tests for [`crate::queue`].

use super::*;

#[test]
fn one_search_lined_up_is_said_in_the_singular() {
    assert_eq!(describe_queue(1, false), "1 search lined up");
    assert_eq!(describe_queue(3, false), "3 searches lined up");
}

#[test]
fn a_holding_queue_says_so() {
    assert!(describe_queue(2, true).contains("holding"));
    assert!(!describe_queue(2, false).contains("holding"));
}

#[test]
fn a_queue_holding_with_nothing_in_it_still_reads_as_a_sentence() {
    // Otherwise it says "0 searches lined up" beside a Carry on button, which reads
    // as a bug rather than as a state you chose.
    assert_eq!(describe_queue(0, true), "the queue is holding, and empty");
    assert_eq!(describe_queue(0, false), "nothing lined up");
}
