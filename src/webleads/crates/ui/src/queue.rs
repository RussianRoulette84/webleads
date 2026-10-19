//! The searches lined up behind the one that is running.
//!
//! Its own file rather than another hundred lines of `run.rs`: starting a search and
//! watching a line of them waiting are two different jobs.

use leptos::*;

use crate::api;

/// What is lined up, and the hold on it.
#[component]
pub fn QueuePanel(reload: ReadSignal<u32>, on_change: WriteSignal<u32>) -> impl IntoView {
    let queue = create_local_resource(move || reload.get(), |_| async move { api::queue().await });

    let hold = move |paused: bool| {
        spawn_local(async move {
            let _ = api::hold_queue(paused).await;
            on_change.update(|n| *n += 1);
        });
    };

    let forget = move |id: i64| {
        spawn_local(async move {
            let _ = api::forget_queued(id).await;
            on_change.update(|n| *n += 1);
        });
    };

    view! {
        {move || queue.get().and_then(Result::ok).filter(|q| !q.waiting.is_empty() || q.paused)
            .map(|q| {
                let paused = q.paused;
                view! {
                    <div style="margin-top:16px">
                        <div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap">
                            <div class="field__label" style="margin:0">
                                {describe_queue(q.waiting.len(), paused)}
                            </div>
                            <button
                                class=move || if paused { "btn" } else { "btn btn--quiet" }
                                style="font-size:15px;padding:4px 9px"
                                on:click=move |_| hold(!paused)
                            >
                                {if paused { "Carry on" } else { "Hold the queue" }}
                            </button>
                        </div>
                        <div style="display:flex;flex-direction:column;gap:4px;margin-top:8px">
                            {q.waiting.into_iter().map(|run| {
                                let id = run.id;
                                view! {
                                    <div style="display:flex;gap:10px;align-items:center">
                                        <span style="flex:1;font-size:16px">
                                            {run.source.clone()}
                                            <span class="is-dim">
                                                {format!(" · {} words", run.words_total)}
                                            </span>
                                        </span>
                                        <button
                                            class="btn btn--quiet"
                                            style="font-size:14px"
                                            on:click=move |_| forget(id)
                                        >
                                            "take out"
                                        </button>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    </div>
                }
            })}
    }
}

/// What the queue's heading says.
///
/// Pure, so the wording is tested — including the awkward case of a queue that is
/// holding with nothing in it, which reads as a mistake unless it is worded.
pub fn describe_queue(waiting: usize, paused: bool) -> String {
    match (waiting, paused) {
        (0, true) => "the queue is holding, and empty".to_owned(),
        (0, false) => "nothing lined up".to_owned(),
        (1, true) => "1 search lined up, holding".to_owned(),
        (1, false) => "1 search lined up".to_owned(),
        (n, true) => format!("{n} searches lined up, holding"),
        (n, false) => format!("{n} searches lined up"),
    }
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
