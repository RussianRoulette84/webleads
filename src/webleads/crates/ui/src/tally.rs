//! The scoreboard: what you got done today, and what you have got done ever.
//!
//! # What this file is here to teach
//!
//! A number on a screen is only useful if you trust it, and you only trust it if it
//! moves the moment you press something. So this panel is fed from two directions:
//! it refetches when you mark a business yourself, and it refetches on its own
//! timer, which is how a change made on the phone reaches the desktop.
//!
//! The words are the state's own words, so the scoreboard and the chips above the
//! list can never disagree about what "said no" means.

use leptos::*;
use webleads_domain::Status;

use crate::api;

/// The states worth putting on a scoreboard, in the order they are shown.
///
/// "To call" is left out: it is the size of the pile you have not started, which is
/// not a measure of a day's work and would dwarf every other number on the row.
pub fn worth_counting() -> Vec<Status> {
    Status::ALL
        .into_iter()
        .filter(|status| *status != Status::NeedCall)
        .collect()
}

/// How many are in one state, from the server's counted list.
///
/// Pure. The server leaves out a state nobody is in, and every reader of this wants
/// nought rather than nothing.
pub fn how_many(counts: &[(String, i64)], status: Status) -> i64 {
    counts
        .iter()
        .find(|(each, _)| each == status.as_str())
        .map(|(_, n)| *n)
        .unwrap_or(0)
}

/// Today's calling and all of it, side by side.
#[component]
pub fn Scoreboard(
    /// The scoreboard the page is holding. Shared, because the same answer also
    /// tells the list when another device has changed something.
    tally: Memo<api::Tally>,
) -> impl IntoView {
    view! {
        <div class="panel wl-score">
            <div class="wl-score__band">
                <div class="field__label wl-score__when">"Today"</div>
                <Row
                    called=Signal::derive(move || tally.get().called_today)
                    counts=Signal::derive(move || tally.get().today)
                />
            </div>
            <div class="wl-score__band">
                <div class="field__label wl-score__when">"All days"</div>
                <Row
                    called=Signal::derive(move || tally.get().called_ever)
                    counts=Signal::derive(move || tally.get().ever)
                />
            </div>
        </div>
    }
}

/// One line of the scoreboard: how many you rang, then where they ended up.
#[component]
fn Row(called: Signal<i64>, counts: Signal<Vec<(String, i64)>>) -> impl IntoView {
    view! {
        <div class="wl-score__row">
            <div class="wl-score__cell">
                <b class="wl-score__n">{move || called.get()}</b>
                <span class="wl-score__name">"called"</span>
            </div>
            {worth_counting().into_iter().map(|status| view! {
                <div class="wl-score__cell" data-state=status.as_str()>
                    <b class="wl-score__n">
                        {move || how_many(&counts.get(), status)}
                    </b>
                    <span class="wl-score__name">{status.describe()}</span>
                </div>
            }).collect_view()}
        </div>
    }
}

#[cfg(test)]
#[path = "tally_tests.rs"]
mod tests;
