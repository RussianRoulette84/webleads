//! Starting a scrape and watching it happen.
//!
//! # What this file is here to teach
//!
//! - Listening to server-sent events, so a ninety-minute run tells us when something
//!   changes instead of us asking five thousand times.
//! - Reading the answer before committing to it — the estimate.

use leptos::leptos_dom::helpers::IntervalHandle;
use leptos::*;
use webleads_domain::{Target, Tier};

use crate::api::{self, Run, Word};

/// How often to ask the server what is going.
///
/// The events carry the detail; this is what keeps the buttons honest. Asked whether
/// or not the page thinks anything is running, so a search it lost track of still
/// brings STOP back within a few seconds.
const CHECK_TICK: u64 = 3_000;

#[component]
pub fn RunPanel(
    /// The trades, for the picker.
    targets: Vec<Target>,
    /// The search that is going, shared with the terminal above so the two cannot
    /// disagree about what is happening.
    run: RwSignal<Option<Run>>,
    /// Bumped by SCRAPE NOW up in the terminal. The choice of what to search lives
    /// here, in the picker; the button that starts it lives up there.
    start_asked: ReadSignal<u32>,
    /// Bumped when a run finds something, so the call list refreshes itself.
    on_change: WriteSignal<u32>,
    /// What the call list is narrowed to. Named `filter` rather than `asking`
    /// because this file already has an `asking` — the words to search for.
    filter: ReadSignal<api::Asking>,
    set_filter: WriteSignal<api::Asking>,
    /// The language the place being searched describes its trades in, as the server
    /// worked it out. The picker offers only the trades that have words in it.
    lang: Memo<String>,
    /// The name of that place.
    place: Memo<String>,
) -> impl IntoView {
    // The S tier, chosen as the page opens: the trades most likely to have no website
    // and the means to pay for one, ready to search with one press. Its trades are
    // ticked once the search language is known, below.
    let (tiers, set_tiers) = create_signal(vec![Tier::S]);
    let (trades, set_trades) = create_signal(Vec::<String>::new());
    let (own_words, set_own_words) = create_signal(Vec::<String>::new());
    let (options, set_options) = create_signal(webleads_domain::ScrapeOptions::default());
    // Written through the shared signal, read through it too — `set_run` and `run`
    // below are the two halves of the one the page owns.
    let set_run = run.write_only();
    let run = run.read_only();
    let (words, set_words) = create_signal(Vec::<Word>::new());
    let (estimate, set_estimate) = create_signal(Option::<api::Estimate>::None);
    let (problem, set_problem) = create_signal(Option::<String>::None);
    let (report_open, set_report_open) = create_signal(false);
    // Bumped whenever the queue or the area changes, so both are asked again.
    let (queue_change, set_queue_change) = create_signal(0_u32);

    // Where the next search will look, asked again whenever an area is chosen — the
    // name the page opened with goes stale the moment you press another chip.
    let areas = create_local_resource(
        move || queue_change.get(),
        |_| async move { api::areas().await },
    );
    let searching_here = create_memo(move |_| {
        areas
            .get()
            .and_then(Result::ok)
            .and_then(|a| {
                let chosen = a.chosen?;
                a.areas
                    .into_iter()
                    .find(|area| area.id == chosen)
                    .map(|area| area.label)
            })
            .unwrap_or_else(|| place.get())
    });

    // The city of that place: the one whose starting point the settings move.
    let searching_city = create_memo(move |_| {
        let answer = areas.get().and_then(Result::ok)?;
        let chosen = answer.chosen?;
        answer
            .areas
            .into_iter()
            .find(|area| area.id == chosen)?
            .city_id
    });

    // One list for the picker and the editor under it, so a saved edit is what the
    // picker offers straight away.
    let targets = create_rw_signal(targets);

    // The language you last searched in, as the server remembers it, put into the
    // settings once it arrives — so the dropdown, the picker and the search agree.
    create_effect(move |synced: Option<bool>| {
        let remembered = lang.get();
        if synced == Some(true) || remembered.is_empty() {
            return synced.unwrap_or(false);
        }
        set_options.update(|o| o.lang = remembered);
        true
    });
    // What the picker counts words in: the language this search will be made in.
    let search_lang = create_memo(move |_| options.get().lang);

    // Tick the opening tier's trades the first time the language is known — only the
    // ones that can be searched in it, so the ticks match the count on the button.
    create_effect(move |ticked: Option<bool>| {
        if ticked == Some(true) {
            return true;
        }
        let lang = lang.get();
        if lang.is_empty() {
            return false;
        }
        if tiers.get_untracked() == vec![Tier::S] && trades.get_untracked().is_empty() {
            set_trades.set(crate::picker::tier_names(
                &targets.get_untracked(),
                Tier::S,
                &lang,
            ));
        }
        true
    });

    // What is currently asked for.
    let asking = move || api::StartRequest {
        tiers: tiers.get().iter().map(|t| format!("{t:?}")).collect(),
        trades: trades.get(),
        words: own_words.get(),
        // Only when something has actually been moved. An unchanged set would say
        // the same as sending nothing, and sending nothing is one less thing to read
        // in the log.
        options: (crate::settings::moved(&options.get()) > 0).then(|| options.get()),
    };

    // The cost, kept up to date as you choose. Answered before you commit to an hour
    // and a half rather than after.
    create_effect(move |_| {
        let request = asking();
        spawn_local(async move {
            set_estimate.set(api::estimate(&request).await.ok());
        });
    });

    // Which run the events are coming from, so one run is never listened to twice.
    let listening = store_value(None::<i64>);

    // A run says when something changes rather than being asked five thousand times.
    let listen = move |id: i64| {
        use wasm_bindgen::prelude::*;
        if listening.get_value() == Some(id) {
            return;
        }
        listening.set_value(Some(id));
        let Ok(source) = web_sys::EventSource::new(&format!("/api/runs/{id}/events")) else {
            return;
        };
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
            move |event: web_sys::MessageEvent| {
                let Some(text) = event.data().as_string() else {
                    return;
                };
                if let Ok(fresh) = serde_json::from_str::<Run>(&text) {
                    let finished = !fresh.still_going();
                    // One line per change, so the console tells the whole story of a
                    // run — which word it is on, how many have landed, how many
                    // businesses so far.
                    let said = describe_progress(&fresh);
                    if finished {
                        match fresh.note.clone() {
                            Some(why) => crate::log::bad("search", &format!("{said} — {why}")),
                            None => crate::log::done("search", &said),
                        }
                    } else {
                        crate::log::step("search", &said);
                    }
                    set_run.set(Some(fresh));
                    // Rows arrive as each word lands, rather than at the end.
                    on_change.update(|n| *n += 1);
                    if finished {
                        set_report_open.set(true);
                        // So the record of searches shows it straight away.
                        set_queue_change.update(|n| *n += 1);
                    }
                }
            },
        );
        // **The server sends a named event**, and `onmessage` only ever receives
        // unnamed ones — so this listened to nothing at all and the screen was
        // driven entirely by the ten-second fallback below.
        let _ = source.add_event_listener_with_callback("run", on_message.as_ref().unchecked_ref());
        on_message.forget();
    };

    // Whatever the server says is going — including a run carried on after a restart,
    // or one started from another screen — and its events, once.
    let look = move || {
        spawn_local(async move {
            if let Ok(found) = api::current_run().await {
                if let Some(going) = found.as_ref().filter(|r| r.still_going()) {
                    listen(going.id);
                }
                set_run.set(next_run(run.get_untracked(), found));
            }
        });
    };

    // What is going right now: asked as the page opens, then every few seconds.
    look();
    let ticking = set_interval_with_handle(look, std::time::Duration::from_millis(CHECK_TICK)).ok();
    let ticking: StoredValue<Option<IntervalHandle>> = store_value(ticking);
    on_cleanup(move || {
        if let Some(handle) = ticking.get_value() {
            handle.clear();
        }
    });

    let go = move || {
        set_problem.set(None);
        let request = asking();
        spawn_local(async move {
            match api::start_run(&request).await {
                Ok(started) => {
                    listen(started.run_id);
                    look();
                }
                Err(err) => {
                    crate::terminal::say(err.clone(), crate::terminal::Tone::Bad);
                    set_problem.set(Some(err));
                }
            }
        });
    };

    // SCRAPE NOW, pressed up in the terminal. The first run of this effect is the
    // page building itself, not a press, so it starts nothing.
    create_effect(move |seen: Option<u32>| {
        let asked = start_asked.get();
        if seen.is_some_and(|before| before != asked) {
            if cost(estimate.get()).0 == 0 {
                crate::terminal::say(
                    "nothing chosen — pick some trades below first",
                    crate::terminal::Tone::Warn,
                );
            } else {
                go();
            }
        }
        asked
    });

    // Every search is started from the terminal. Asked for while one is going, the
    // server queues it, and the panel below shows what is waiting.

    let show_report = move |_| {
        let Some(going) = run.get() else { return };
        spawn_local(async move {
            if let Ok(found) = api::run_words(going.id).await {
                set_words.set(found);
                set_report_open.set(true);
            }
        });
    };

    view! {
        <div class="panel" style="margin-bottom:22px">
            <h4 class="heading">"Google Maps Search Settings"</h4>

            // Where the next search will look, and how to change it.
            <crate::areas::AreaPanel
                on_change=set_queue_change filter=filter set_filter=set_filter
            />

            // What the search is set to look for. Its own settings live here with
            // the place, because both are "how this search is set up".
            <crate::settings::Settings
                options=options set_options=set_options on_clock=on_change
                city=Signal::derive(move || searching_city.get())
            />

            // What is lined up behind whatever is going now.
            <crate::queue::QueuePanel reload=queue_change on_change=set_queue_change/>
        </div>

        <div class="panel" style="margin-bottom:22px">
            <h4 class="heading">"Trades to search for"</h4>
            <Picker
                targets=targets
                tiers=tiers set_tiers=set_tiers
                trades=trades set_trades=set_trades
                own_words=own_words set_own_words=set_own_words
                estimate=estimate
                lang=search_lang place=searching_here
            />

            // Everything already done is a strip at the top of the filters now,
            // always in front of you rather than behind a button down here.

            {move || problem.get().map(|text| view! {
                <div class="alert is-bad" style="margin-top:14px">{text}</div>
            })}

            // The list itself, to read and edit: the trades you pick from above.
            <crate::trades::TradeList list=targets/>

            {move || run.get().filter(|r| !r.still_going()).map(|done| view! {
                <div style="display:flex;gap:12px;align-items:center;margin-top:14px;flex-wrap:wrap">
                    <span class="is-dim" style="font-size:16px">
                        {format!(
                            "last run: {} — {} of {} words, {} businesses",
                            done.state, done.words_done, done.words_total, done.found_total
                        )}
                    </span>
                    <button class="btn btn--quiet" style="font-size:15px" on:click=show_report>
                        "Which words worked"
                    </button>
                </div>
            })}

            {move || (report_open.get() && !words.get().is_empty()).then(|| view! {
                <crate::report::HitReport words=words.get()/>
            })}
        </div>
    }
}

/// One line describing where a run has got to, for the console.
///
/// Pure, so the wording is tested rather than eyeballed in a browser.
pub fn describe_progress(run: &Run) -> String {
    let where_it_is = match &run.now_searching {
        Some(word) => format!("searching “{word}”"),
        None => run.state.clone(),
    };
    format!(
        "{where_it_is} · {} of {} words · {} businesses",
        run.words_done, run.words_total, run.found_total
    )
}

/// Choosing what to search for.
#[component]
#[allow(
    clippy::too_many_arguments,
    reason = "a form with seven fields is a form"
)]
fn Picker(
    targets: RwSignal<Vec<Target>>,
    tiers: ReadSignal<Vec<Tier>>,
    set_tiers: WriteSignal<Vec<Tier>>,
    trades: ReadSignal<Vec<String>>,
    set_trades: WriteSignal<Vec<String>>,
    own_words: ReadSignal<Vec<String>>,
    set_own_words: WriteSignal<Vec<String>>,
    estimate: ReadSignal<Option<api::Estimate>>,
    /// The language the place being searched describes its trades in.
    lang: Memo<String>,
    /// The name of that place, so the panel can say where it is about to look.
    place: Memo<String>,
) -> impl IntoView {
    view! {
        <crate::picker::TradePicker
            targets=targets.read_only()
            tiers=tiers set_tiers=set_tiers
            trades=trades set_trades=set_trades
            lang=lang
        />

        <crate::picker::OwnWords words=own_words set_words=set_own_words/>

        // Which place, and in which language — said, rather than left to be worked
        // out from the word counts.
        {move || view! {
            <div class="is-dim wl-picked wl-picked__lang">
                {crate::picker::searching_said(&place.get(), &lang.get())}
            </div>
        }}

        // What you have chosen adds up to, and roughly how long it would take. Only
        // once you have chosen something: "nothing chosen yet" under an empty picker
        // is a line that tells you what you can already see.
        {move || {
            let (searches, trades_chosen) = crate::picker::how_much(
                &targets.get(), &trades.get(), &own_words.get(), &lang.get(),
            );
            (searches > 0).then(|| {
                let chosen = crate::picker::describe_choice(
                    searches, trades_chosen, own_words.get().len(),
                );
                // The count is ours and instant; how long it takes is the server's,
                // measured from the searches already done.
                let said = match cost(estimate.get()).1 {
                    long if long.contains("about") => format!(
                        "{chosen} · {}",
                        long.split_once("— ").map_or(long.clone(), |(_, t)| t.to_owned())
                    ),
                    _ => chosen,
                };
                view! { <div class="is-dim wl-picked">{said}</div> }
            })
        }}
    }
}

/// What a run would cost, in words a person reads.
///
/// Pure, so the wording can be tested — including the case where we have measured
/// nothing yet and should say so rather than inventing a number you might plan an
/// afternoon around.
pub fn cost(estimate: Option<api::Estimate>) -> (usize, String) {
    let Some(estimate) = estimate else {
        return (0, "working out the cost…".to_owned());
    };
    if estimate.words == 0 {
        return (0, "nothing chosen yet".to_owned());
    }

    let words = format!(
        "{} search{}",
        estimate.words,
        if estimate.words == 1 { "" } else { "es" }
    );
    match estimate.seconds {
        None => (
            estimate.words,
            format!("{words} — how long is not known until one run has finished"),
        ),
        Some(seconds) => (
            estimate.words,
            format!("{words}, about {}", how_long(seconds)),
        ),
    }
}

/// A duration in words rather than in seconds.
pub fn how_long(seconds: u64) -> String {
    match seconds {
        0..=90 => format!("{seconds} seconds"),
        91..=5400 => {
            let minutes = (seconds + 30) / 60;
            format!("{minutes} minutes")
        }
        _ => {
            let hours = seconds as f64 / 3600.0;
            format!("{hours:.1} hours")
        }
    }
}

/// The progress of a run that is going on now, under the buttons that control it.
///
/// Which word it is on sits beside the buttons already, so it is not said twice.
#[component]
pub fn Working(run: Run) -> impl IntoView {
    let percent = if run.words_total == 0 {
        0
    } else {
        (run.words_done * 100 / run.words_total).clamp(0, 100)
    };

    view! {
        <div class="progress" style="margin:14px 0 6px">
            {format!(
                "{} of {} words done, {} businesses so far",
                run.words_done, run.words_total, run.found_total
            )}
            <b><i style=format!("width:{percent}%")></i><em>{format!("{percent}%")}</em></b>
        </div>
        <span class="is-dim" style="font-size:16px">
            "Everything found so far is already saved."
        </span>
    }
}

/// What the page should believe is going, given what it believed and what the server
/// just said.
///
/// The server wins: whenever it says a search is going, that search is the one shown,
/// so STOP is on the screen for as long as anything runs. With nothing going, a run
/// the page saw finish stays as "last run"; one it thought was still going has ended.
/// Pure, so the rule is a test.
pub fn next_run(believed: Option<Run>, server: Option<Run>) -> Option<Run> {
    match (believed, server) {
        (_, Some(going)) => Some(going),
        (Some(ours), None) if ours.still_going() => None,
        (ours, None) => ours,
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
