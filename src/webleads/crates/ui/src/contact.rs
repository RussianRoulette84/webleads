//! What you wrote down about one business.
//!
//! # What this file is here to teach
//!
//! Saving on a pause rather than on every keystroke, using `set_timeout_with_handle`
//! so the previous, now-pointless save can be cancelled — and saving on the two other
//! moments a note can be lost: clicking away, and closing the tab.
//!
//! The state of a business is its own column and its own file: see
//! [`crate::status`].

use leptos::leptos_dom::helpers::TimeoutHandle;
use leptos::*;

use crate::api;

/// How long to wait after you stop typing before saving a note.
///
/// Only one of three moments a note is written. Clicking away and closing the tab
/// both save at once — waiting this out was how a note could be typed and lost.
const SETTLE_MS: f64 = 700.0;

/// Where a note has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
    /// Nothing to say — either untouched, or typed and not yet written.
    Nothing,
    Saving,
    /// Written. Shown, so you can see it landed rather than hoping.
    Done,
}

/// The note for one business, saved as you pause typing.
#[component]
pub fn NoteCell(id: String, note: String) -> impl IntoView {
    let (typed_note, set_note) = create_signal(note.clone());
    // Everything `save_now` touches has to be copyable, or it can only be used once
    // — and it is used from three places. `store_value` is how a String becomes
    // something a closure can copy.
    let original = store_value(note);
    // Three states rather than a bool: you need to be able to see that it *landed*,
    // not only that something was happening.
    let (save_state, set_save_state) = create_signal(Saved::Nothing);
    let (problem, set_problem) = create_signal(Option::<String>::None);
    let id = store_value(id);

    let save_now = move || {
        let text = typed_note.get();
        if text == original.get_value() && matches!(save_state.get(), Saved::Nothing) {
            return; // nothing has changed
        }
        set_save_state.set(Saved::Saving);
        let business = id.get_value();
        spawn_local(async move {
            match api::set_note(&business, &text).await {
                Ok(_) => set_save_state.set(Saved::Done),
                Err(err) => {
                    set_save_state.set(Saved::Nothing);
                    set_problem.set(Some(err));
                }
            }
        });
    };

    // The handle of the save that has not happened yet, so a new keystroke can cancel
    // it. Without this, typing twenty characters would send twenty requests.
    let pending: StoredValue<Option<TimeoutHandle>> = store_value(None);

    let typing = move |ev| {
        set_note.set(event_target_value(&ev));
        set_save_state.set(Saved::Nothing);
        if let Some(handle) = pending.get_value() {
            handle.clear();
        }
        let handle =
            set_timeout_with_handle(save_now, std::time::Duration::from_millis(SETTLE_MS as u64))
                .ok();
        pending.set_value(handle);
    };

    // Clicking away saves at once rather than waiting out the pause.
    let left_the_box = move |_| {
        if let Some(handle) = pending.get_value() {
            handle.clear();
        }
        save_now();
    };

    // And so does closing the tab: a note typed and the window shut within the pause
    // would otherwise be lost silently, which is the worst way to lose anything.
    let on_leaving = window_event_listener(ev::beforeunload, move |_| {
        if let Some(handle) = pending.get_value() {
            handle.clear();
        }
        save_now();
    });
    on_cleanup(move || on_leaving.remove());

    view! {
        <textarea
            class="field field--notes wl-note"
            placeholder="notes…"
            prop:value=move || typed_note.get()
            on:input=typing
            on:blur=left_the_box
        ></textarea>
        {move || match save_state.get() {
            Saved::Nothing => ().into_view(),
            Saved::Saving => view! {
                <span class="is-dim" style="font-size:14px">"saving…"</span>
            }.into_view(),
            // The point of the third state: you can see it landed.
            Saved::Done => view! {
                <span class="is-ok" style="font-size:14px">"saved"</span>
            }.into_view(),
        }}
        {move || problem.get().map(|text| view! {
            <span class="is-bad" style="font-size:14px">{text}</span>
        })}
    }
}
