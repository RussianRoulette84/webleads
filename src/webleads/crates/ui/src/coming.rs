//! The calls with a time on them — the ones you promised and the ones they did.
//!
//! A panel above the list rather than a page of its own: a promise to ring back on
//! Tuesday is worth nothing if you have to go and look for it. A shop that said it
//! would ring you sits here too, marked as theirs, because one that never rings is
//! the easiest call of the day to win back.

use leptos::*;

use crate::api;

#[component]
pub fn ComingUp(
    /// Bumped whenever a state or a ring-back time is saved, so this catches up.
    reload: ReadSignal<u32>,
) -> impl IntoView {
    let due = create_local_resource(
        move || reload.get(),
        |_| async move { api::coming_up().await },
    );

    view! {
        <Suspense fallback=|| ().into_view()>
            {move || due.get().and_then(Result::ok).map(|due| {
                // It hides itself when there is nothing coming up, like the hidden
                // panel and the queue. An empty panel is a row of furniture.
                if due.is_empty() {
                    return ().into_view();
                }
                let late = due.iter().filter(|d| d.overdue).count();
                let theirs = due.iter().filter(|d| d.theirs).count();
                view! {
                    <div class="panel wl-coming">
                        <div class="field__label">
                            "Coming up"
                            <span class="is-dim" style="margin-left:10px">
                                {describe_due(due.len(), late, theirs)}
                            </span>
                        </div>
                        {due.into_iter().map(|call| {
                            let id = call.id.clone();
                            view! {
                                <div class=move || if call.overdue {
                                    "wl-coming__line is-late"
                                } else {
                                    "wl-coming__line"
                                }>
                                    <span class="wl-coming__when">
                                        {call.when.clone()}
                                        // The one thing on this screen that should
                                        // shout: a call you meant to make yesterday.
                                        {call.overdue.then(|| view! {
                                            <span class="tag is-warn wl-coming__late">"late"</span>
                                        })}
                                    </span>
                                    <span class="wl-coming__who">
                                        {call.name.clone()}
                                        // Whose move it is, said on the line, so a
                                        // call you are waiting on is never mistaken
                                        // for one you owe.
                                        {call.theirs.then(|| view! {
                                            <span class="tag wl-coming__whose">
                                                "they ring"
                                            </span>
                                        })}
                                        {(!call.note.trim().is_empty()).then(|| view! {
                                            <span class="is-dim wl-coming__note">
                                                {call.note.clone()}
                                            </span>
                                        })}
                                    </span>
                                    {call.phone.clone().map(|phone| view! {
                                        <a
                                            class="link u-keep"
                                            href=format!("tel:{}", dial(&phone))
                                        >
                                            {phone}
                                        </a>
                                    })}
                                    // Straight to the row, lit the same way the map's
                                    // pins light it.
                                    <button
                                        class="btn btn--quiet wl-coming__jump"
                                        on:click=move |_| crate::map::show_row(&id)
                                    >
                                        "go to it"
                                    </button>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                }
                .into_view()
            })}
        </Suspense>
    }
}

/// How many calls are waiting, how many are late, and how many are theirs to make.
///
/// Pure, so the sentence is checked without a browser.
pub fn describe_due(total: usize, late: usize, theirs: usize) -> String {
    let calls = if total == 1 {
        "1 call".to_owned()
    } else {
        format!("{total} calls")
    };
    let mut said = calls;
    if late > 0 {
        said = format!("{said}, {late} late");
    }
    if theirs > 0 {
        said = format!("{said}, {theirs} they ring");
    }
    said
}

/// A number the telephone can dial.
///
/// `tel:` wants no spaces or brackets; a person wants them. So the shown number and
/// the dialled one are two different strings.
pub fn dial(phone: &str) -> String {
    phone
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '+')
        .collect()
}

#[cfg(test)]
#[path = "coming_tests.rs"]
mod tests;
