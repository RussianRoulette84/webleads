//! Tests for [`crate::clock`].
//!
//! The clock itself is the browser's and cannot be tested here. The shape it gets
//! written in can, and that is the half that breaks.

use super::*;
use webleads_domain::tidy_ring_back;

#[test]
fn a_date_and_time_is_written_the_way_the_field_reads_it() {
    assert_eq!(stamp_for_input(2026, 9, 22, 13, 0), "2026-09-22T13:00");
}

#[test]
fn single_figures_keep_their_leading_nought() {
    // `2026-9-2T9:5` is a field the browser shows as empty, which reads as a
    // ring-back that was never saved.
    assert_eq!(stamp_for_input(2026, 1, 2, 9, 5), "2026-01-02T09:05");
}

#[test]
fn what_we_write_is_what_the_server_will_accept() {
    // The two halves have to agree: the field's shape goes straight back to the
    // server, which refuses anything it cannot read as a time.
    let offered = stamp_for_input(2026, 12, 31, RING_BACK_HOUR, 0);
    assert_eq!(
        tidy_ring_back(&offered),
        Some("2026-12-31 13:00".to_owned())
    );
}

#[test]
fn the_hour_we_offer_is_one_in_the_afternoon() {
    assert_eq!(RING_BACK_HOUR, 13);
}

#[test]
fn the_moment_we_write_down_is_one_the_server_will_accept() {
    // "They'll call" saves the moment you were told without asking you to type it,
    // so that moment has to go back to the server in a shape it can read.
    let offered = stamp_for_input(2026, 10, 13, 16, 45);
    assert_eq!(
        tidy_ring_back(&offered),
        Some("2026-10-13 16:45".to_owned())
    );
}
