//! The terminal, the buttons under it, and everything that feeds them.
//!
//! # What this file is here to teach
//!
//! Three feeds into one window without any of them knowing about each other: the
//! page's own requests (pushed from `api::ask`), the server's log (polled here), and
//! the search itself (pushed from the run panel). The window is a view of what
//! happened, and none of the three had to be changed to sit beside the others.

use leptos::*;

use crate::api;
use crate::link::{link_of, Link, ScraperLight, HEALTH_TICK};
use crate::terminal::{Terminal, Tone};

/// How often to ask the server what it has written.
///
/// Two seconds, because a search writes something roughly every three: any faster is
/// asking for nothing, any slower and the window lags behind the thing it watches.
const TICK: u64 = 2_000;

#[component]
pub fn ScrapeConsole(
    /// The search that is going, or paused, or nothing at all. Owned by the page so
    /// the panel below and these buttons cannot disagree about it.
    run: RwSignal<Option<api::Run>>,
    /// Bumped by SCRAPE NOW. The searching box below watches it and starts the
    /// search its picker is set to — the choice lives there, the button lives here.
    on_start: WriteSignal<u32>,
    /// Bumped whenever the search changes, so the list and the strip catch up.
    on_change: WriteSignal<u32>,
) -> impl IntoView {
    let session = crate::terminal::provide_session();
    // The box lives on the session so that a request three files away can ask
    // whether the detail is wanted without being handed a signal.
    let debug = session.detail();
    let (problem, set_problem) = create_signal(None::<String>);

    // Where each log was read up to. Offsets the server hands us, handed straight
    // back — nothing here needs to understand them.
    let read_to = store_value((None::<u64>, None::<u64>));

    // The server's own log, as it is written.
    let poll = move || {
        let (backend, scraper) = read_to.get_value();
        spawn_local(async move {
            let Ok(tail) = api::logs(backend, scraper, debug.get_untracked()).await else {
                return;
            };
            read_to.set_value((Some(tail.backend.at), Some(tail.scraper.at)));
            // The search first: it is what you are watching. The web server's own
            // lines are the quieter half.
            for part in [&tail.scraper, &tail.backend] {
                if part.skipped {
                    session.say("… jumped to the end of the log", Tone::Dim);
                }
                for line in &part.lines {
                    session.say(line.clone(), crate::terminal::tone_of(line));
                }
            }
        });
    };

    let ticking = set_interval_with_handle(poll, std::time::Duration::from_millis(TICK)).ok();
    let ticking: StoredValue<Option<leptos::leptos_dom::helpers::IntervalHandle>> =
        store_value(ticking);
    on_cleanup(move || {
        if let Some(handle) = ticking.get_value() {
            handle.clear();
        }
    });

    // Is the thing that does the searching there? Asked now and every few seconds,
    // and said in the window whenever the answer changes — so the first line is a
    // real one, and a scraper that dies mid-afternoon says so.
    let (link, set_link) = create_signal(Link::Checking);
    let check = move || {
        spawn_local(async move {
            let health = api::health().await;
            if let Ok(answer) = &health {
                session.set_machine(answer.machine.clone());
            }
            let now = link_of(&health);
            if now == link.get_untracked() {
                return;
            }
            set_link.set(now);
            match health {
                Ok(health) if health.scraper => {
                    session.say("scraper is up · ready to search", Tone::Good);
                }
                Ok(health) => session.say(
                    format!(
                        "{} — start it in the webleads folder with: {}",
                        health
                            .scraper_detail
                            .unwrap_or_else(|| "the scraper is not running".to_owned()),
                        crate::link::START_SCRAPER
                    ),
                    Tone::Bad,
                ),
                Err(reason) => session.say(reason, Tone::Bad),
            }
        });
    };
    check();
    let checking =
        set_interval_with_handle(check, std::time::Duration::from_millis(HEALTH_TICK)).ok();
    let checking: StoredValue<Option<leptos::leptos_dom::helpers::IntervalHandle>> =
        store_value(checking);
    on_cleanup(move || {
        if let Some(handle) = checking.get_value() {
            handle.clear();
        }
    });

    let pressed = move |what: &'static str| {
        set_problem.set(None);
        let Some(going) = run.get() else {
            return;
        };
        let id = going.id;
        spawn_local(async move {
            let answer = match what {
                "pause" => api::pause_run(id).await,
                "continue" => api::continue_run(id).await,
                _ => api::stop_run(id).await,
            };
            match answer {
                Ok(said) => session.say(said, Tone::Note),
                Err(reason) => {
                    session.say(reason.clone(), Tone::Bad);
                    set_problem.set(Some(reason));
                }
            }
            on_change.update(|n| *n += 1);
        });
    };

    // Which buttons exist at all, rather than which are greyed out: CONTINUE means
    // nothing when nothing is paused, and a row of dead buttons is a row of
    // questions about why they are dead.
    let offered = move |button: Button| offer(run.get().as_ref()).contains(&button);

    view! {
        <section class="wl-console">
            <Terminal session=session/>

            <div class="wl-console__buttons">
                {move || offered(Button::Scrape).then(|| view! {
                    <button
                        class="btn wl-scrape"
                        title="Start the search the box below is set to"
                        disabled=move || !link.get().can_scrape()
                        on:click=move |_| {
                            session.say("starting the search…", Tone::Note);
                            on_start.update(|n| *n += 1);
                        }
                    >
                        "SCRAPE NOW"
                    </button>
                })}
                // Under SCRAPE NOW: whether pressing it can work, and if not, why.
                <ScraperLight link=link/>
                {move || offered(Button::Pause).then(|| view! {
                    <button
                        class="btn btn--quiet"
                        title="Finish the word it is on, then hold"
                        on:click=move |_| pressed("pause")
                    >
                        "PAUSE"
                    </button>
                })}
                {move || offered(Button::Continue).then(|| view! {
                    <button
                        class="btn"
                        title="Carry on from where it was paused"
                        on:click=move |_| pressed("continue")
                    >
                        "CONTINUE"
                    </button>
                })}
                {move || offered(Button::Stop).then(|| view! {
                    <button
                        class="btn btn--danger btn--quiet"
                        title="End the search, keeping everything found so far"
                        on:click=move |_| pressed("stop")
                    >
                        "STOP"
                    </button>
                })}

                // Only when there is something to say about it.
                {move || run.get().map(|going| view! {
                    <span class="is-dim wl-console__state">{describe(Some(&going))}</span>
                })}
            </div>

            {move || run.get().filter(|r| r.still_going()).map(|going| view! {
                <crate::run::Working run=going/>
            })}

            {move || problem.get().map(|text| view! {
                <div class="alert is-bad" style="margin-top:10px">{text}</div>
            })}
        </section>
    }
}

/// One of the four buttons under the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Scrape,
    Pause,
    Continue,
    Stop,
}

/// Which buttons to put on the screen at all.
///
/// Shown, not greyed out. A CONTINUE with nothing to continue is a question you have
/// to answer every time you look at it, and the answer is always "nothing".
///
/// Pure, so the rule is a test rather than four conditions scattered through a view.
pub fn offer(run: Option<&api::Run>) -> Vec<Button> {
    let Some(run) = run else {
        // Nothing going: the only thing you can do is start one.
        return vec![Button::Scrape];
    };
    // Running: hold it or end it. Starting another is refused anyway — one search
    // at a time, because Google notices anything faster.
    if run.working_now() {
        return vec![Button::Pause, Button::Stop];
    }
    // Paused: carry on, or give up on it.
    if run.held() {
        return vec![Button::Continue, Button::Stop];
    }
    // Lined up but not started: it can still be called off.
    if run.state == "queued" {
        return vec![Button::Stop];
    }
    // Done, stopped, failed: it is over, and the next one is yours to start.
    vec![Button::Scrape]
}

/// What the search is doing, in a few words, beside the buttons.
///
/// Pure, so the wording is a test rather than something read off a screen.
pub fn describe(run: Option<&api::Run>) -> String {
    let Some(run) = run else {
        return "nothing running".to_owned();
    };
    let words = format!("{} of {} words", run.words_done, run.words_total);
    match run.state.as_str() {
        "working" => match &run.now_searching {
            Some(word) => format!("searching “{word}” · {words}"),
            None => format!("searching · {words}"),
        },
        "held" => format!("paused · {words} · press continue"),
        "queued" => format!("queued · {} words", run.words_total),
        other => format!("{other} · {words}"),
    }
}

#[cfg(test)]
#[path = "console_tests.rs"]
mod tests;
