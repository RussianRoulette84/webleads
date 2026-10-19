//! What the page says in the browser's console.
//!
//! # What this file is here to teach
//!
//! One logger, used by everything. Console lines written ad hoc by each feature end up
//! in six different shapes, and the one you need is the one that was never written.
//!
//! Every line carries where it came from, what happened, and — when it was a request —
//! how long it took. Colours follow the page: brass for a step, green for something
//! that worked, amber for a warning, rust for a failure.

use wasm_bindgen::JsValue;

/// Our own colours, so the console reads like the page rather than like a stack trace.
const BRASS: &str = "color:#c9962f;font-weight:600";
const GREEN: &str = "color:#5fb08a;font-weight:600";
const AMBER: &str = "color:#e2a355;font-weight:600";
const RUST: &str = "color:#a8453a;font-weight:600";
const DIM: &str = "color:#6b6459";

/// A step beginning: what we are about to do.
pub fn step(area: &str, what: &str) {
    say(BRASS, area, what, None);
}

/// A step that worked, with what came of it.
pub fn done(area: &str, what: &str) {
    say(GREEN, area, what, None);
}

/// A step that worked, and how long it took.
pub fn timed(area: &str, what: &str, ms: f64) {
    say(GREEN, area, what, Some(ms));
}

/// Something worth knowing that is not a failure.
pub fn warn(area: &str, what: &str) {
    say(AMBER, area, what, None);
}

/// Something that failed, in the words the person would be shown.
pub fn bad(area: &str, what: &str) {
    say(RUST, area, what, None);
}

/// One line, coloured.
fn say(colour: &str, area: &str, what: &str, ms: Option<f64>) {
    let tail = match ms {
        Some(ms) => took(ms),
        None => String::new(),
    };
    let line = format!("%c{}%c {}%c {}", label(area), what, tail);
    web_sys::console::log_4(
        &JsValue::from_str(&line),
        &JsValue::from_str(colour),
        &JsValue::from_str("color:inherit"),
        &JsValue::from_str(DIM),
    );
}

/// The badge at the start of a line: `[leads]`.
///
/// Pure, so the shape of every line in the console is fixed by a test rather than by
/// whoever wrote the newest one.
pub fn label(area: &str) -> String {
    format!("[{area}]")
}

/// How long something took, in words rather than a bare number.
///
/// Pure. Milliseconds up to a second, then seconds — 2500 ms means nothing at a
/// glance, "2.5 s" means something.
pub fn took(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{:.1} s", ms / 1000.0)
    }
}

/// The clock, for measuring how long one request took.
///
/// `js_sys::Date::now()` rather than the browser's finer timer: this is for a console
/// line a person reads, and a millisecond is already more precision than that needs.
pub fn now() -> f64 {
    js_sys::Date::now()
}

/// How long since a moment from [`now`].
pub fn since(started: f64) -> f64 {
    now() - started
}

#[cfg(test)]
#[path = "log_tests.rs"]
mod tests;
