//! Tests for [`crate::log`].

use super::*;

#[test]
fn every_line_says_where_it_came_from() {
    assert_eq!(label("leads"), "[leads]");
    assert_eq!(label("search"), "[search]");
}

#[test]
fn a_quick_step_is_said_in_milliseconds() {
    assert_eq!(took(120.0), "120 ms");
    assert_eq!(took(999.0), "999 ms");
}

#[test]
fn a_slow_one_is_said_in_seconds() {
    // "2500 ms" means nothing at a glance; "2.5 s" means something.
    assert_eq!(took(2_500.0), "2.5 s");
    assert_eq!(took(153_000.0), "153.0 s");
}

#[test]
fn nothing_at_all_still_reads_as_a_time() {
    assert_eq!(took(0.0), "0 ms");
}
