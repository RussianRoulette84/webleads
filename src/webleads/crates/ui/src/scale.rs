//! How far a slider reaches, and how much one nudge moves it.
//!
//! Both were fixed numbers before — two hundred reviews and twenty kilometres — so a
//! busier lead than that could not be reached at all, and the far end of the track
//! did nothing.

/// The top of a slider: the data's own maximum, rounded up so the end of the track
/// is a number a person would say out loud.
///
/// `least_top` keeps a nearly empty list usable — one lead with three reviews should
/// not give you a slider three notches wide.
pub fn reach(most: i64, least_top: i64) -> f64 {
    let top = most.max(least_top).max(1) as f64;
    let step = step_for(top);
    (top / step).ceil() * step
}

/// How much one nudge moves, taken from the range — so twenty reviews and twenty
/// thousand metres both feel right on the same kind of control.
pub fn step_for(max: f64) -> f64 {
    match max {
        m if m <= 20.0 => 1.0,
        m if m <= 100.0 => 5.0,
        m if m <= 1_000.0 => 10.0,
        m if m <= 10_000.0 => 100.0,
        _ => 500.0,
    }
}

/// Are we close enough to the bottom to ask for the next fifty?
///
/// A screen's worth of warning, so the rows arrive before you reach the end rather
/// than after. Pure — the three numbers come from the browser, the decision does not.
pub fn near_the_bottom(scrolled: f64, window_height: f64, page_height: f64) -> bool {
    // A page shorter than the window cannot be scrolled at all, and asking for more
    // the moment it loads would fetch the whole list in one go.
    if page_height <= window_height {
        return false;
    }
    scrolled + window_height >= page_height - window_height
}

/// Have we come far enough down to want a way back up?
///
/// Two screens. One is not far enough to have lost your place; by two you are a long
/// way from the search box. Pure, so the boundary is a test rather than a feeling.
pub fn far_from_the_top(scrolled: f64, window_height: f64) -> bool {
    window_height > 0.0 && scrolled > window_height * 2.0
}

#[cfg(test)]
#[path = "scale_tests.rs"]
mod tests;
