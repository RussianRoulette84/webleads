//! The one thing in the page that has to ask what time it is.
//!
//! # What this file is here to teach
//!
//! Everywhere else the page refuses to know the time: a WebAssembly build carries no
//! timezone database, so "is this ring-back overdue" is the server's question. But a
//! *suggestion* is different. When you say you will ring somebody back, the sensible
//! offer is tomorrow afternoon by the clock on the phone in your hand — and that
//! clock is the browser's, not Budapest's.
//!
//! The date arithmetic is the browser's too. Adding a day across the end of February
//! in a leap year is a well-known way to write a bug, and `Date` already does it.
//! What is left here is the formatting, which is pure and tested.

/// The hour we offer to ring somebody back at, on the twenty-four hour clock.
///
/// One in the afternoon: after the morning rush, before anybody shuts for the day.
pub const RING_BACK_HOUR: u32 = 13;

/// Tomorrow at one in the afternoon, in the shape the date field wants.
///
/// The browser's own clock and the browser's own calendar. Nothing here knows or
/// cares what month it is.
pub fn tomorrow_at_one() -> String {
    let when = js_sys::Date::new_0();
    when.set_date(when.get_date() + 1);
    when.set_hours(RING_BACK_HOUR);
    when.set_minutes(0);
    stamp_for_input(
        when.get_full_year(),
        when.get_month() + 1,
        when.get_date(),
        when.get_hours(),
        when.get_minutes(),
    )
}

/// Right now, in the shape the date field wants.
///
/// What "they said they would ring" is written down as: the moment you were told,
/// which you then move if they named one. The browser's clock again — the phone in
/// your hand is the only clock that knows what time it is where you are standing.
pub fn right_now() -> String {
    let when = js_sys::Date::new_0();
    stamp_for_input(
        when.get_full_year(),
        when.get_month() + 1,
        when.get_date(),
        when.get_hours(),
        when.get_minutes(),
    )
}

/// A date and time written the only way the browser's field will read it.
///
/// `2026-09-22T13:00`. Anything else — a missing nought, a space instead of the T —
/// shows as an empty field, which looks exactly like having lost the time.
///
/// Pure, and tested, because that shape is easy to get subtly wrong and impossible
/// to notice until somebody's ring-back has quietly vanished.
pub fn stamp_for_input(year: u32, month: u32, day: u32, hour: u32, minute: u32) -> String {
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}")
}

#[cfg(test)]
#[path = "clock_tests.rs"]
mod tests;
