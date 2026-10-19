//! Where one business has got to, as six rows on its own row of the list.
//!
//! # What this file is here to teach
//!
//! Seven states, four of which carry something besides themselves — a count of
//! unanswered calls, and a time on each of the two states that have one. They are
//! drawn as rows, all of them on the screen at once with a tick against the one it
//! is in. It was a dropdown first: one press to open, one to choose, and on a phone
//! the panel covered the row it belonged to. Short rows cost less height than that
//! cost patience.

use leptos::*;
use webleads_domain::{ring_back_for_input, Contact, Status};

use crate::api;

/// The state of a business, and the way to change it.
#[component]
pub fn StatusPicker(
    id: String,
    contact: Contact,
    /// The state the row is in, which the row itself reads to colour its edge and
    /// to fade a business you have finished with. Shared rather than copied: the
    /// tick moving while the card looks untouched is what made marking a business
    /// feel like pressing a dead button.
    shows: RwSignal<Status>,
    /// Bumped whenever something is saved, so the coming-up panel catches up.
    on_saved: WriteSignal<u32>,
) -> impl IntoView {
    let id = store_value(id);
    let (state, set_state) = create_signal(contact);
    let (problem, set_problem) = create_signal(None::<String>);

    // One place where an answer from the server lands, so every control in here
    // shows the same thing afterwards — including the state the server chose for
    // itself when the fifth unanswered call gave up on the number.
    let took = move |answer: api::Answer<Contact>| match answer {
        Ok(contact) => {
            shows.set(contact.status);
            set_state.set(contact);
            set_problem.set(None);
            on_saved.update(|n| *n += 1);
        }
        Err(reason) => set_problem.set(Some(reason)),
    };

    let choose = move |wanted: Status| {
        // A state that carries a time and then hands you an empty field is half an
        // answer. So one is written down for you and changing it is one press.
        // Which one depends on whose move it is: a call you owe is offered tomorrow
        // afternoon, and a call they promised is written down as the moment they
        // promised it, which is now.
        let offer = (wanted.carries_a_time() && state.get_untracked().follow_up_tstamp.is_none())
            .then(|| match wanted {
                Status::TheyCall => crate::clock::right_now(),
                _ => crate::clock::tomorrow_at_one(),
            });
        spawn_local(async move {
            took(api::set_status(&id.get_value(), wanted).await);
            if let Some(when) = offer {
                took(api::set_ring_back(&id.get_value(), Some(&when)).await);
            }
        });
    };

    let count_to = move |count: i64| {
        spawn_local(async move {
            took(api::set_no_answers(&id.get_value(), count).await);
        });
    };

    let ring_back_at = move |when: Option<String>| {
        spawn_local(async move {
            took(api::set_ring_back(&id.get_value(), when.as_deref()).await);
        });
    };

    view! {
        // Every state, always on the screen. No button to open, nothing to cover
        // the row it belongs to.
        <div class="wl-state">
            <div class="wl-state__rows">
                {Status::ALL.into_iter().map(|option| {
                    let chosen = move || state.get().status == option;
                    view! {
                        <div
                            class="wl-state__row"
                            aria-selected=move || chosen().to_string()
                            on:click=move |_| choose(option)
                        >
                            <span class="tick">"✓"</span>
                            <span class="wl-state__name">{option.describe()}</span>
                            // The count sits on the row it belongs to, and so does
                            // the time — rather than in a second control somewhere
                            // else that you have to find.
                            {(option == Status::NotReached).then(|| view! {
                                <Stepper
                                    count=Signal::derive(move || state.get().not_reached_counter)
                                    on_change=count_to
                                />
                            })}
                            {option.carries_a_time().then(|| view! {
                                <RingBack
                                    when=Signal::derive(move || state.get().follow_up_tstamp.clone())
                                    about=if option == Status::TheyCall {
                                        "When they said they would ring"
                                    } else {
                                        "When to ring them back"
                                    }
                                    on_change=ring_back_at
                                />
                            })}
                        </div>
                    }
                }).collect_view()}
            </div>

            // What the state carries, under the rows: rung four times, or due back
            // on Tuesday.
            {move || {
                let now = state.get();
                (now.status == Status::NotReached && now.not_reached_counter > 0).then(|| {
                    view! {
                        <div class="is-dim wl-state__said">
                            {no_answers_said(now.not_reached_counter)}
                        </div>
                    }
                })
            }}

            {move || problem.get().map(|reason| view! {
                <div class="is-bad wl-state__said">{reason}</div>
            })}
        </div>
    }
}

/// How many unanswered calls, in words.
///
/// Pure, so the sentence is a test rather than something you read off a screen.
pub fn no_answers_said(count: i64) -> String {
    match count {
        n if n <= 0 => String::new(),
        1 => "rung once, no answer".to_owned(),
        n => format!("rung {n} times, no answer"),
    }
}

/// Up and down arrows around a number.
///
/// The design sheet drew this and nothing has ever used it. Both arrows stop the
/// press reaching the row behind them, or changing the count would also change the
/// state — and pressing "down" would be how you gave up on a number.
#[component]
fn Stepper<F>(count: Signal<i64>, on_change: F) -> impl IntoView
where
    F: Fn(i64) + Copy + 'static,
{
    view! {
        <span class="wl-step" on:click=|ev| ev.stop_propagation()>
            <button
                class="wl-step__arrow"
                title="One fewer unanswered call"
                on:click=move |ev| {
                    ev.stop_propagation();
                    on_change((count.get() - 1).max(0));
                }
            >
                "−"
            </button>
            <b class="wl-step__count">{move || count.get()}</b>
            <button
                class="wl-step__arrow"
                title="One more unanswered call"
                on:click=move |ev| {
                    ev.stop_propagation();
                    on_change(count.get() + 1);
                }
            >
                "+"
            </button>
        </span>
    }
}

/// The date and time to ring back: the time as words, and a field you open to change
/// it.
///
/// The field is the browser's own — a calendar of our own would be a week's work to
/// be worse at the one thing this does. What is ours is *when it is on the screen*.
/// On a phone that field fills the window with a calendar, and the only way out used
/// to be to choose a date or press escape, which threw away the state list with it.
/// Now DONE puts the calendar away and leaves everything else exactly where it was.
#[component]
fn RingBack<F>(
    when: Signal<Option<String>>,
    /// What this time means, for the tooltip — the two states that carry one differ
    /// only in whose move it is, and that is the one thing worth saying out loud.
    about: &'static str,
    on_change: F,
) -> impl IntoView
where
    F: Fn(Option<String>) + Copy + 'static,
{
    let (picking, set_picking) = create_signal(false);

    view! {
        // Every press in here stops at this box. Without it, choosing a date would
        // also be choosing whichever state the box happens to sit on.
        <span class="wl-when" on:click=|ev| ev.stop_propagation()>
            // Shut: the time you set, as a word you can read at arm's length.
            {move || (!picking.get()).then(|| view! {
                <button
                    class="wl-when__shown"
                    title=about
                    on:click=move |ev| {
                        ev.stop_propagation();
                        set_picking.set(true);
                    }
                >
                    {move || when.get().unwrap_or_else(|| "set a time".to_owned())}
                </button>
            })}

            // Open: the browser's field, and a way out of it that is not "choose
            // something".
            {move || picking.get().then(|| view! {
                <input
                    class="field wl-when__field"
                    type="datetime-local"
                    title=about
                    prop:value=move || {
                        when.get().map(|w| ring_back_for_input(&w)).unwrap_or_default()
                    }
                    on:change=move |ev| {
                        let typed = event_target_value(&ev);
                        // An emptied field means "no time", which is not an error.
                        on_change((!typed.trim().is_empty()).then_some(typed));
                    }
                />
                <button
                    class="wl-when__done"
                    title="Put the calendar away and keep this time"
                    on:click=move |ev| {
                        ev.stop_propagation();
                        set_picking.set(false);
                    }
                >
                    "DONE"
                </button>
            })}

            {move || when.get().map(|_| view! {
                <button
                    class="wl-when__clear"
                    title="Take the time off"
                    on:click=move |ev| {
                        ev.stop_propagation();
                        set_picking.set(false);
                        on_change(None);
                    }
                >
                    "✕"
                </button>
            })}
        </span>
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
