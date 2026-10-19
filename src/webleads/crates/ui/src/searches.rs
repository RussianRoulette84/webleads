//! Every search you have run, as a strip you can press to open one.
//!
//! # What this file is here to teach
//!
//! Giving a record a way back into the thing it describes. A list of past searches is
//! only half useful; pressing one and seeing *its* leads is the other half, and it is
//! what makes "Budapest this morning, Dubai this afternoon" two lists rather than one
//! heap.
//!
//! A strip in the filters, because which search you are looking at is one of the
//! first things you narrow by.

use leptos::*;

use crate::api;

#[component]
pub fn PastSearches(
    /// Bumped whenever a search starts, finishes or is taken out, so the strip is
    /// never stale.
    reload: ReadSignal<u32>,
    /// What the call list is narrowed to, so the strip can say which one is showing.
    asking: ReadSignal<api::Asking>,
    set_asking: WriteSignal<api::Asking>,
) -> impl IntoView {
    let past = create_local_resource(
        move || reload.get(),
        |_| async move { api::past_runs().await },
    );
    // The one under the pointer, so its details can be read without choosing it.
    let (peeked, set_peeked) = create_signal(None::<api::Run>);
    let (report, set_report) = create_signal(None::<(i64, Vec<api::Word>)>);

    let show = move |id: Option<i64>| {
        set_report.set(None);
        set_asking.update(|a| {
            // One at a time: an area and a search together would read as "and", and
            // the pair you would have to guess at.
            a.from_area = None;
            a.from_run = id;
        });
    };

    // The search you have asked to forget, and what it would cost. Two signals
    // rather than one pair: the dialog opens the moment you press, and the price
    // arrives a beat later.
    let (forgetting, set_forgetting) = create_signal(None::<api::Run>);
    let (cost, set_cost) = create_signal(None::<api::ForgetCost>);

    let ask_forget = move |run: api::Run| {
        let id = run.id;
        set_cost.set(None);
        set_forgetting.set(Some(run));
        spawn_local(async move {
            if let Ok(price) = api::forget_cost(id).await {
                set_cost.set(Some(price));
            }
        });
    };

    let forget = move |id: i64| {
        spawn_local(async move {
            if api::forget_run(id).await.is_ok() {
                set_forgetting.set(None);
                set_cost.set(None);
                // Whatever it was showing is gone, so stop narrowing by it — and that
                // same change is what tells the call list to read itself again.
                set_asking.update(|a| {
                    if a.from_run == Some(id) {
                        a.from_run = None;
                    }
                });
                past.refetch();
            }
        });
    };

    let show_words = move |id: i64| {
        spawn_local(async move {
            if let Ok(words) = api::run_words(id).await {
                set_report.set(Some((id, words)));
            }
        });
    };

    view! {
        <div class="field__label">"Past searches"</div>
        <Suspense fallback=move || view! {
            <div class="wl-strip"><span class="spinner"></span></div>
        }>
            {move || past.get().map(|answer| match answer {
                Err(problem) => view! { <div class="alert is-bad">{problem}</div> }.into_view(),
                Ok(runs) => {
                    // Only the searches of the city being looked at. Left whole, the
                    // strip under Dubai would be a row of Budapest searches that all
                    // narrow to nothing.
                    let known: Vec<api::Run> =
                        crate::cities::searches_in_city(&runs, asking.get().city);
                    view! {
                        // Sideways, because there will be dozens of these and they
                        // must not push the page down.
                        <div class="wl-strip">
                            <button
                                class=move || chip_class(asking.get().from_run.is_none())
                                on:click=move |_| show(None)
                            >
                                "All"
                            </button>
                            {known.clone().into_iter().map(|run| {
                                let id = run.id;
                                let mine = run.clone();
                                let chosen = move || asking.get().from_run == Some(id);
                                view! {
                                    <button
                                        class=move || chip_class(chosen())
                                        title="Show only what this search was the first to find"
                                        on:mouseenter=move |_| set_peeked.set(Some(mine.clone()))
                                        on:mouseleave=move |_| set_peeked.set(None)
                                        on:click=move |_| show(Some(id))
                                    >
                                        {short_name(&run)}
                                        <span class="wl-chip__count">{run.leads_found}</span>
                                    </button>
                                }
                            }).collect_view()}
                        </div>

                        // What the one under the pointer — or the one you chose —
                        // actually is. Hovering tells you before you commit to it.
                        {move || {
                            let showing = peeked.get().or_else(|| {
                                let chosen = asking.get().from_run?;
                                known.iter().find(|r| r.id == chosen).cloned()
                            });
                            showing.map(|run| {
                                let id = run.id;
                                let mine_again = run.clone();
                                view! {
                                    <div class="wl-strip__said">
                                        {describe_run(&run)}
                                        <button
                                            class="btn btn--quiet wl-strip__words"
                                            on:click=move |_| show_words(id)
                                        >
                                            "which words"
                                        </button>
                                        <button
                                            class="btn btn--quiet wl-strip__words wl-strip__forget"
                                            title="Forget this search"
                                            on:click=move |_| ask_forget(mine_again.clone())
                                        >
                                            "forget"
                                        </button>
                                    </div>
                                }
                            })
                        }}
                    }.into_view()
                }
            })}
        </Suspense>

        // Asked before anything is thrown away, and it says the real number rather
        // than "are you sure?" — because sometimes the real number is three hundred.
        {move || forgetting.get().map(|run| {
            let id = run.id;
            let said = describe_run(&run);
            view! {
                <div class="wl-ask" role="dialog" aria-modal="true" aria-label="Forget this search">
                    <div class="dialog wl-ask__box">
                        <h5>"Forget this search?"</h5>
                        <p class="wl-ask__which">{said}</p>
                        {move || match cost.get() {
                            None => view! { <p><span class="spinner"></span>" working out what it would cost"</p> }.into_view(),
                            Some(price) => view! {
                                {forget_warning(&price).map(|warning| view! {
                                    <p class="wl-ask__cost">{warning}</p>
                                })}
                                <p>{forget_kept(&price)}</p>
                            }.into_view(),
                        }}
                        <div class="wl-ask__buttons">
                            <button
                                class="btn btn--secondary"
                                on:click=move |_| { set_forgetting.set(None); set_cost.set(None); }
                            >
                                "Keep it"
                            </button>
                            <button class="btn btn--danger" on:click=move |_| forget(id)>
                                "Forget it"
                            </button>
                        </div>
                    </div>
                </div>
            }
        })}

        {move || report.get().map(|(id, words)| view! {
            <div style="margin-top:10px">
                <div class="field__label">{format!("What search {id} searched for")}</div>
                <crate::report::HitReport words=words/>
            </div>
        })}
    }
}

/// The warning, for a search that would take businesses with it.
///
/// `None` when it would take none — silence is the right answer then, and a line
/// saying "0 businesses would be lost" is noise dressed as care.
///
/// Pure, so the sentence that decides whether three hundred telephone numbers get
/// thrown away is tested rather than eyeballed.
pub fn forget_warning(cost: &api::ForgetCost) -> Option<String> {
    match cost.businesses_lost {
        0 => None,
        1 => Some(
            "1 business leaves your call list for good — no other search has ever seen it."
                .to_owned(),
        ),
        many => Some(format!(
            "{many} businesses leave your call list for good — no other search has ever seen them."
        )),
    }
}

/// What survives it, said plainly. Always said, including when the answer is nothing,
/// because "what happens to my leads" is the question being asked.
pub fn forget_kept(cost: &api::ForgetCost) -> String {
    let words = match cost.words {
        1 => "1 search word".to_owned(),
        many => format!("{many} search words"),
    };
    match cost.businesses_kept {
        0 => format!("The record of the search goes, and {words} with it."),
        1 => format!(
            "1 business stays, wearing what an earlier search decided about it. \
             The record of the search goes, and {words} with it."
        ),
        many => format!(
            "{many} businesses stay, wearing what an earlier search decided about them. \
             The record of the search goes, and {words} with it."
        ),
    }
}

/// Which class a chip wears. One shade, not a Christmas tree.
pub fn chip_class(chosen: bool) -> &'static str {
    if chosen {
        "wl-chip is-on"
    } else {
        "wl-chip"
    }
}

/// A search in a few words, for the chip itself.
///
/// The area if we know it, the day if we do not. Pure, so a long district name
/// getting cut off is a test rather than something you notice on screen.
pub fn short_name(run: &api::Run) -> String {
    let place = place_of(run);
    // The day alone, off the front of the timestamp: the chip has no room for a
    // clock, and two searches on one day are told apart by the area anyway.
    let day = run.started_at.split(' ').next().unwrap_or("");
    match day.is_empty() {
        true => place,
        false => format!("{place} · {day}"),
    }
}

/// Where a search was looking, in as few words as we have.
///
/// The area if we know it, the file it read if we do not, and its number if we have
/// neither — one chip that says "search 12" beats two chips that say nothing.
pub fn place_of(run: &api::Run) -> String {
    let name = run
        .area
        .clone()
        .filter(|a| !a.trim().is_empty())
        .unwrap_or_else(|| run.source.clone());
    if name.trim().is_empty() {
        format!("search {}", run.id)
    } else {
        name
    }
}

/// The whole of one search in one line, for under the strip.
pub fn describe_run(run: &api::Run) -> String {
    let (_, ended) = describe_end(run);
    // The place without the day, because the whole timestamp follows it — the chip
    // says the day, this line says the hour as well, and saying it twice reads as a
    // mistake.
    format!(
        "{} · {} · {} leads from {} businesses · {}",
        place_of(run),
        run.started_at,
        run.leads_found,
        run.found_total,
        ended
    )
}

/// The clock part of a moment, for one that carries an hour.
///
/// Our earlier searches recorded only the day. `None` rather than a made-up midnight:
/// a search that says it ended at 00:00 is worse than one that does not say.
pub fn clock_of(moment: &str) -> Option<&str> {
    moment.split_once(' ')?.1.get(..5).filter(|clock| {
        let mut parts = clock.split(':');
        matches!((parts.next(), parts.next()), (Some(h), Some(m))
            if h.len() == 2 && m.len() == 2
                && h.chars().all(|c| c.is_ascii_digit())
                && m.chars().all(|c| c.is_ascii_digit()))
    })
}

/// How a search ended, in words and in a colour.
///
/// Pure, so the wording is tested. A failed search that simply says "0 found" reads
/// as a search that worked and found nothing, which is a very different thing from
/// one that never got off the ground.
pub fn describe_end(run: &api::Run) -> (&'static str, String) {
    // " 22:00" when we know the hour, nothing at all when we do not.
    let at = match run.finished_at.as_deref().and_then(clock_of) {
        Some(clock) => format!(" {clock}"),
        None => String::new(),
    };
    match run.state.as_str() {
        "queued" => ("is-dim", format!("waiting · {} words", run.words_total)),
        "working" => (
            "is-accent",
            match &run.now_searching {
                Some(word) => format!(
                    "searching “{word}” · {} of {}",
                    run.words_done, run.words_total
                ),
                None => format!("searching · {} of {}", run.words_done, run.words_total),
            },
        ),
        "done" => ("is-ok", format!("finished{at} · {} words", run.words_total)),
        "stopped" => (
            "is-warn",
            format!(
                "you stopped it{at} · {} of {} words",
                run.words_done, run.words_total
            ),
        ),
        _ => (
            "is-bad",
            match &run.note {
                Some(why) => format!("failed{at} — {why}"),
                None => format!("failed{at}"),
            },
        ),
    }
}

#[cfg(test)]
#[path = "searches_tests.rs"]
mod tests;
