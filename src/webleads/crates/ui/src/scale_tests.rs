//! Tests for [`crate::scale`].

use super::*;

#[test]
fn the_busiest_lead_is_always_reachable() {
    // The track reaches the busiest business: 380 reviews must not sit beyond its end.
    for most in [0, 1, 7, 23, 199, 380, 4_000, 51_234] {
        assert!(
            reach(most, 20) >= most as f64,
            "{most} reviews could not be selected"
        );
    }
}

#[test]
fn a_nearly_empty_list_still_gets_a_usable_slider() {
    // One lead with three reviews, and a slider three notches wide is no slider.
    assert_eq!(reach(3, 20), 20.0);
}

#[test]
fn the_top_is_a_round_number() {
    // 382 rounds up to 390, and 23 to 25 — the end of the track should read cleanly.
    assert_eq!(reach(382, 20), 390.0);
    assert_eq!(reach(23, 20), 25.0);
    // Already round, so it stays where it is.
    assert_eq!(reach(380, 20), 380.0);
}

#[test]
fn small_ranges_move_one_at_a_time() {
    // Eleven reviews has to be selectable; in fives it is not.
    assert_eq!(step_for(20.0), 1.0);
}

#[test]
fn large_ranges_move_in_strides() {
    // Twenty thousand metres in single metres is twenty thousand nudges.
    assert_eq!(step_for(20_000.0), 500.0);
}

#[test]
fn every_step_divides_its_own_reach() {
    // Otherwise the last nudge lands short of the top and the busiest lead is only
    // nearly reachable.
    for most in [0, 3, 19, 76, 340, 9_100, 44_000] {
        let top = reach(most, 20);
        let step = step_for(top);
        assert_eq!(
            top % step,
            0.0,
            "reach {top} is not a whole number of {step}"
        );
    }
}

// ---- when to fetch the next fifty ----

#[test]
fn the_top_of_a_long_page_asks_for_nothing() {
    // 3000 pixels of list, a 900 pixel window, and we have not moved.
    assert!(!near_the_bottom(0.0, 900.0, 3_000.0));
}

#[test]
fn a_screen_from_the_end_asks_for_more() {
    // The rows should arrive before you reach the bottom, not after.
    assert!(near_the_bottom(1_200.0, 900.0, 3_000.0));
}

#[test]
fn the_very_bottom_certainly_asks() {
    assert!(near_the_bottom(2_100.0, 900.0, 3_000.0));
}

#[test]
fn a_page_that_does_not_scroll_never_asks() {
    // Otherwise a short list would pull the whole database in one go the moment it
    // appeared.
    assert!(!near_the_bottom(0.0, 900.0, 600.0));
    assert!(!near_the_bottom(0.0, 900.0, 900.0));
}

// ---- when to offer the way back up ----

#[test]
fn one_screen_down_is_not_lost_yet() {
    // You can still see where you came from.
    assert!(!far_from_the_top(400.0, 900.0));
    assert!(!far_from_the_top(1_800.0, 900.0));
}

#[test]
fn two_screens_down_and_the_search_box_is_long_gone() {
    assert!(far_from_the_top(1_801.0, 900.0));
    assert!(far_from_the_top(6_000.0, 900.0));
}

#[test]
fn the_top_of_the_page_never_offers_it() {
    assert!(!far_from_the_top(0.0, 900.0));
}

#[test]
fn a_window_of_no_height_is_not_an_invitation() {
    // It happens for a moment while the page is starting up.
    assert!(!far_from_the_top(500.0, 0.0));
}
