//! The call list: what this whole program exists to put on a screen.
//!
//! # What this file is here to teach
//!
//! - `create_local_resource` — a value that is not there yet because it is coming
//!   over the network. It re-runs when the thing it depends on changes.
//! - `<Suspense>` — what to show while that is happening, without writing a single
//!   `if loading { ... }`.
//! - Polling a background job without blocking anything, using `set_timeout`.
//! - Why it is `local`: the future from a browser fetch cannot be moved between
//!   threads, and in a web page there is only one thread anyway.

use std::time::Duration;

use leptos::leptos_dom::helpers::IntervalHandle;
use leptos::*;

use webleads_domain::Status;

use crate::api::{self, JobState, LeadRow};
use crate::coming::ComingUp;
use crate::filters::FilterBar;
use crate::hidden::HiddenPanel;
use crate::map::Map;
use crate::run::RunPanel;
use crate::table::LeadTable;

/// How many leads to fetch at a time.
const PAGE: usize = 50;

/// How far down the page we have scrolled.
fn how_far_down() -> f64 {
    web_sys::window()
        .and_then(|w| w.scroll_y().ok())
        .unwrap_or(0.0)
}

/// How tall the window is.
fn window_height() -> f64 {
    web_sys::window()
        .and_then(|w| w.inner_height().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// How tall the whole page is.
fn page_height() -> f64 {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
        .map(|e| f64::from(e.scroll_height()))
        .unwrap_or(0.0)
}

/// Put the page back where it was, with no animation — this is a restoration, not a
/// journey.
fn scroll_back_to(top: f64) {
    if let Some(window) = web_sys::window() {
        let how = web_sys::ScrollToOptions::new();
        how.set_top(top);
        how.set_behavior(web_sys::ScrollBehavior::Instant);
        window.scroll_to_with_scroll_to_options(&how);
    }
}

/// How long to wait for the rows to be drawn before putting the scroll back.
///
/// Setting it in the same breath as the rows lands on a page that is still the old
/// height, and the browser clamps it to whatever fits.
const AFTER_DRAWING: Duration = Duration::from_millis(80);

/// How often to ask a running import how it is getting on.
const POLL: Duration = Duration::from_millis(600);

/// How often to ask whether anything was marked somewhere else.
///
/// One cheap question — five counts and a timestamp — so this can be often enough
/// that the phone and the desktop never visibly disagree, without the list itself
/// being fetched unless something actually changed.
const WATCH: Duration = Duration::from_secs(5);

#[component]
pub fn Leads() -> impl IntoView {
    // Bumping this number is how we say "fetch the list again". The resource below
    // watches it, so setting it re-runs the fetch — no manual refresh function.
    let (reload, set_reload) = create_signal(0_u32);
    // Rows fetched so far. Pages are appended rather than refetched, so page ten
    // costs fifty rows, not five hundred.
    let (rows, set_rows) = create_signal(Vec::<LeadRow>::new());
    let (offset, set_offset) = create_signal(0_usize);
    let (search, set_search) = create_signal(String::new());
    // Everything the list is narrowed and ordered by, in one place — the same shape
    // the server reads, so the two cannot drift.
    let (asking, set_asking) = create_signal(api::Asking {
        limit: PAGE,
        ..Default::default()
    });
    let (message, set_message) = create_signal(Option::<Result<String, String>>::None);
    // `None` means everyone still on the list.
    let (only, set_only) = create_signal(Option::<Status>::None);
    // Bumped whenever a row's state or ring-back time is saved. The coming-up panel
    // watches it; the list itself deliberately does not, or marking one business
    // would reload every row and lose your place.
    let (contacts, set_contacts) = create_signal(0_u32);
    // The search itself, shared between the terminal at the top and the searching box
    // below it, so the buttons and the progress cannot disagree about what is going.
    let run_now = create_rw_signal(Option::<api::Run>::None);
    // Bumped by SCRAPE NOW; the searching box watches it and starts what its picker
    // is set to.
    let (start_asked, set_start_asked) = create_signal(0_u32);
    // `None` when nothing is running. This is what draws the progress bar.
    let (progress, set_progress) = create_signal(Option::<(usize, usize, String)>::None);
    // Which job the bar is showing, so it can be stopped.
    let (job_now, set_job_now) = create_signal(Option::<u64>::None);

    // Only the filter, the search and an explicit reload start a fresh list. "Load
    // more" appends instead, which is why the offset is not in here.
    // The whole question, assembled. The status buttons and the search box live in
    // their own signals for historical reasons; everything else is in `asking`.
    let question = move || api::Asking {
        status: only.get(),
        search: search.get(),
        offset: 0,
        ..asking.get()
    };

    // The reload counter is part of what the list is asking for, not a separate
    // nudge: a resource only re-runs when its source changes, so leaving it out
    // meant every "fetch the list again" in this file quietly did nothing.
    let leads = create_local_resource(
        move || (reload.get(), question()),
        |(_, asking)| async move { api::leads(&asking).await },
    );

    // A fresh list replaces whatever was there; the offset goes back to the start.
    create_effect(move |_| {
        if let Some(Ok(response)) = leads.get() {
            set_rows.set(response.leads.clone());
            set_offset.set(response.leads.len());
        }
    });

    // The list opens on the city you live in. Without this it opens on everything,
    // and everything is ordered with the newest search first — so a morning's
    // calling started in whichever city you searched last.
    //
    // Once only: pressing "all" afterwards has to stick.
    let opened_on = store_value(false);
    create_effect(move |_| {
        if opened_on.get_value() {
            return;
        }
        let Some(Ok(response)) = leads.get() else {
            return;
        };
        opened_on.set_value(true);
        // One city is no choice at all, and narrowing to it would say nothing.
        if response.cities.len() < 2 {
            return;
        }
        if let Some(home) = response.home_city {
            set_asking.update(|a| a.city = Some(home));
        }
    });

    // Whether the scraper is up is the terminal's first line now, so nothing here
    // needs to ask.

    // The trades the dropdown offers, and the rows the map pins — both read from the
    // last answer rather than worked out again here.
    // The trades never change while we run, so this fetches once: the source closure
    // returns the same `()` every time, and a resource only re-runs when it changes.
    let trades = create_local_resource(|| (), |_| api::targets());
    // The language the searching will use, from the same answer as the trades, so the
    // picker never counts its tiers before the language is known.
    // The place the next search will look at, from the same answer as the trades.
    let searching_place = create_memo(move |_| {
        trades
            .get()
            .and_then(Result::ok)
            .map(|t| t.place)
            .unwrap_or_default()
    });
    let search_lang = create_memo(move |_| {
        trades
            .get()
            .and_then(Result::ok)
            .map(|t| t.lang)
            .unwrap_or_default()
    });
    // Each one carries what the server stores, what a person reads, and how many
    // there are — the stored name is English and the screen is Hungarian, so the
    // two cannot be the same string.
    // The cities with anything on the list, for the row above the searches.
    let cities_on_list = create_memo(move |_| {
        leads
            .get()
            .and_then(Result::ok)
            .map(|r| r.cities)
            .unwrap_or_default()
    });
    let trades_on_list = create_memo(move |_| {
        let known = trades
            .get()
            .and_then(Result::ok)
            .map(|t| t.trades)
            .unwrap_or_default();
        let answer = leads.get().and_then(Result::ok);
        // The language of the list you are looking at, as the server worked it out.
        let lang = answer.as_ref().map(|r| r.lang.clone()).unwrap_or_default();
        answer
            .map(|r| r.trades)
            .unwrap_or_default()
            .into_iter()
            .map(|(stored, count)| {
                let shown = webleads_domain::name_for(&known, &stored, &lang)
                    .unwrap_or_else(|| stored.clone());
                (stored, shown, count)
            })
            .collect::<Vec<_>>()
    });

    // Every match, for the map — asked for separately from the table's one page.
    // The answer carries the city it was fetched for, so the map can tell a fresh set
    // of pins from the ones already on it rather than guessing from the filter — which
    // changes a moment before the pins do.
    let pin_set = create_local_resource(
        move || (reload.get(), question()),
        |(_, asking)| async move { (asking.city, api::pins(&asking).await) },
    );
    let pins = create_memo(move |_| {
        pin_set
            .get()
            .and_then(|(_, got)| got.ok())
            .unwrap_or_default()
    });
    let pins_city = create_memo(move |_| pin_set.get().and_then(|(city, _)| city));

    // Where the list is measured from: the chosen city's centre, or your home city's
    // when the list shows them all. Asked again whenever it is moved.
    let centres =
        create_local_resource(move || reload.get(), |_| async move { api::cities().await });
    let home_spot = Signal::derive(move || {
        let id = asking
            .get()
            .city
            .or_else(|| leads.get().and_then(Result::ok).and_then(|r| r.home_city))?;
        let city = centres.get()?.ok()?.into_iter().find(|c| c.id == id)?;
        let label = city
            .centre_label
            .clone()
            .unwrap_or_else(|| city.name.clone());
        Some((city.centre_lat, city.centre_lon, label))
    });

    // Which businesses we already hold a picture of.
    let shots = create_memo(move |_| {
        leads
            .get()
            .and_then(Result::ok)
            .map(|r| r.shots)
            .unwrap_or_default()
    });

    // How many are in each state, for the numbers on the chips.
    let state_counts = create_memo(move |_| {
        leads
            .get()
            .and_then(Result::ok)
            .map(|r| r.counts)
            .unwrap_or_default()
    });

    // How many the list is keeping off the screen because you hid them.
    let hidden_count = create_memo(move |_| {
        leads
            .get()
            .and_then(Result::ok)
            .map(|r| r.hidden_count)
            .unwrap_or(0)
    });

    // How far the sliders reach, from the data rather than a guess.
    let extremes = create_memo(move |_| {
        leads
            .get()
            .and_then(Result::ok)
            .map(|r| r.extremes)
            .unwrap_or_default()
    });

    // The scoreboard, which doubles as the way two screens stay in step. It is
    // refetched when you mark something yourself and on its own clock; both go
    // through the same counter.
    let (scores_turn, set_scores_turn) = create_signal(0_u32);
    let scores = create_local_resource(
        move || (scores_turn.get(), contacts.get()),
        |_| async move { api::tally().await },
    );
    let tally = create_memo(move |_| scores.get().and_then(Result::ok).unwrap_or_default());

    let watching =
        set_interval_with_handle(move || set_scores_turn.update(|n| *n += 1), WATCH).ok();
    let watching: StoredValue<Option<IntervalHandle>> = store_value(watching);
    on_cleanup(move || {
        if let Some(handle) = watching.get_value() {
            handle.clear();
        }
    });

    // Read the list again without losing where you are.
    //
    // A plain reload asks for the first fifty and throws the rest away: a list you
    // had scrolled a long way down collapses to a screenful, the page gets shorter
    // than your scroll position, and the browser drags you back to the top. Which is
    // exactly what happened every time the phone came back from another app. So this
    // asks for as many rows as are already on screen, and puts the scroll back.
    let refresh_in_place = move || {
        let keep = rows.get_untracked().len().max(PAGE);
        let was_at = how_far_down();
        let mut again = question();
        again.limit = keep;
        again.offset = 0;
        spawn_local(async move {
            match api::leads(&again).await {
                Ok(response) => {
                    let arrived = response.leads.len();
                    set_rows.set(response.leads);
                    set_offset.set(arrived);
                    set_timeout(move || scroll_back_to(was_at), AFTER_DRAWING);
                }
                Err(problem) => set_message.set(Some(Err(problem))),
            }
        });
    };

    // When the moment the scoreboard carries has moved and it was not us who moved
    // it, somebody marked a business on the other device — so the rows on this one
    // are out of date and the list is fetched again.
    let seen_at = store_value(None::<String>);
    create_effect(move |_| {
        let now = tally.get().changed_at;
        if now.is_empty() {
            return;
        }
        let before = seen_at.get_value();
        seen_at.set_value(Some(now.clone()));
        if before.is_some() && before.as_deref() != Some(now.as_str()) {
            crate::log::step("leads", "marked somewhere else — reading the list again");
            refresh_in_place();
        }
    });

    // Ask the server how the job is doing, again and again, until it stops running.
    // A recursive `set_timeout` rather than an interval: each round waits for the
    // answer before asking again, so a slow reply cannot pile requests up behind it.
    fn watch(
        id: u64,
        set_progress: WriteSignal<Option<(usize, usize, String)>>,
        set_message: WriteSignal<Option<Result<String, String>>>,
        set_reload: WriteSignal<u32>,
    ) {
        spawn_local(async move {
            match api::job(id).await {
                Ok(JobState::Running { done, total, step }) => {
                    set_progress.set(Some((done, total, step)));
                    set_timeout(
                        move || watch(id, set_progress, set_message, set_reload),
                        POLL,
                    );
                }
                Ok(JobState::Done { summary }) => {
                    set_progress.set(None);
                    set_message.set(Some(Ok(describe_pictures(&summary))));
                    set_reload.update(|n| *n += 1);
                }
                Ok(JobState::Failed { reason }) => {
                    set_progress.set(None);
                    set_message.set(Some(Err(reason)));
                }
                Err(problem) => {
                    set_progress.set(None);
                    set_message.set(Some(Err(problem)));
                }
            }
        });
    }

    // Fetch the next page and add it. The whole point of item seven: this asks for
    // fifty more, not for everything again with a bigger number.
    let (loading_more, set_loading_more) = create_signal(false);
    // How many there are altogether, so scrolling knows when to stop asking.
    let total = create_memo(move |_| {
        leads
            .get()
            .and_then(Result::ok)
            .map(|r| r.total)
            .unwrap_or(0)
    });

    let more = move || {
        if loading_more.get() {
            return;
        }
        // Everything is already here. Asking again would fetch nothing, for ever.
        if (rows.get().len() as i64) >= total.get() {
            return;
        }
        crate::log::step("leads", &format!("fifty more, from {}", offset.get()));
        set_loading_more.set(true);
        let mut next = question();
        next.offset = offset.get();
        spawn_local(async move {
            match api::leads(&next).await {
                Ok(response) => {
                    let arrived = response.leads.len();
                    set_rows.update(|rows| rows.extend(response.leads));
                    set_offset.update(|n| *n += arrived);
                }
                Err(problem) => set_message.set(Some(Err(problem))),
            }
            set_loading_more.set(false);
        });
    };

    // Reaching the bottom asks for the next fifty. The button stays — a list that
    // only grows when you scroll is a list you cannot ask to finish loading.
    // Far enough down that the way back is worth offering.
    let (deep, set_deep) = create_signal(false);

    let scrolling = window_event_listener(ev::scroll, move |_| {
        let down = how_far_down();
        let tall = window_height();
        set_deep.set(crate::scale::far_from_the_top(down, tall));
        if crate::scale::near_the_bottom(down, tall, page_height()) {
            more();
        }
    });
    on_cleanup(move || scrolling.remove());

    let load_more = move |_| more();

    // A second tab is never more than a glance out of date. Reading the list again
    // when the tab comes back is cheaper and quieter than asking the server every
    // few seconds — and it keeps your place, because coming back from the telephone
    // app is the one moment you most need the row you were on.
    let refresh_on_focus = window_event_listener(ev::focus, move |_| {
        refresh_in_place();
    });
    on_cleanup(move || refresh_on_focus.remove());

    // Photograph every broken site the filter is showing, before a calling session.
    let take_pictures = move |_| {
        crate::log::step("pictures", "photographing every broken site on show");
        set_message.set(None);
        set_progress.set(Some((0, 0, "taking pictures".into())));
        let asking = question();
        spawn_local(async move {
            match api::take_shots(&asking).await {
                Ok(started) => {
                    // Pressing it twice does not start a second round; the server
                    // hands back the one already going, and the page should say so
                    // rather than looking as though nothing happened.
                    let step = if started.already_running {
                        "pictures were already being taken — watching those"
                    } else {
                        "taking pictures"
                    };
                    set_job_now.set(Some(started.job_id));
                    set_progress.set(Some((0, started.rows, step.into())));
                    watch(started.job_id, set_progress, set_message, set_reload);
                }
                Err(problem) => {
                    set_progress.set(None);
                    set_message.set(Some(Err(problem)));
                }
            }
        });
    };

    view! {
        // The terminal, above everything: it is what you watch while a search runs,
        // and the buttons that start, hold and end one live under it.
        <crate::console::ScrapeConsole
            run=run_now on_start=set_start_asked on_change=set_reload
        />

        // Searching Google Maps sits above the list, because it is what fills it.
        {move || trades.get().and_then(Result::ok).map(|got| view! {
            <RunPanel
                targets=got.trades on_change=set_reload
                filter=asking set_filter=set_asking
                lang=search_lang place=searching_place
                run=run_now start_asked=start_asked
            />
        })}

        <div class="panel">
            <h4 class="heading">"The call list"</h4>

            <div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap;margin-bottom:14px">
                <div style="flex:1;min-width:220px;position:relative;display:flex">
                    <input
                        class="field field--search"
                        style="flex:1"
                        type="search"
                        placeholder="search name, address, trade, number or your notes…"
                        prop:value=move || search.get()
                        on:input=move |ev| set_search.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Escape" {
                                set_search.set(String::new());
                            }
                        }
                    />
                    // Selecting the text and deleting it was the only way before.
                    {move || (!search.get().is_empty()).then(|| view! {
                        <button
                            class="btn btn--quiet"
                            style="position:absolute;right:6px;top:50%;transform:translateY(-50%);\
                                   border:0;background:none;padding:2px 6px;font-size:15px"
                            title="Clear the search"
                            on:click=move |_| set_search.set(String::new())
                        >
                            "✕"
                        </button>
                    })}
                </div>
                // Plain links rather than buttons: the browser downloads what a link
                // points at, and the address already carries the filter and search.
                <a
                    class="btn btn--quiet"
                    title="Numbers sort as numbers, headings stay put, your notes come too"
                    href=move || format!("/api/export.xlsx?{}", question().query())
                >
                    "Spreadsheet"
                </a>
                <a
                    class="btn btn--quiet"
                    title="Plain text, opens anywhere"
                    href=move || format!("/api/export?{}", question().query())
                >
                    "CSV"
                </a>
                // One file with everything in it, including the map. Works with this
                // server switched off.
                <a
                    class="btn btn--quiet"
                    title="One page you can keep, with the map, working without the server"
                    href=move || format!("/api/export.html?{}", question().query())
                >
                    "Page"
                </a>
                // One press before a calling session, rather than a wait on each row.
                <button
                    class="btn btn--quiet"
                    title="Photograph every broken website the filter is showing"
                    disabled=move || progress.get().is_some()
                    on:click=take_pictures
                >
                    "Pictures of all"
                </button>
            </div>

            // The progress bar only exists while something is running, so `Option`
            // is exactly right — `None` renders nothing at all.
            {move || progress.get().map(|(done, total, step)| {
                // `checked_div` gives None rather than dividing by zero, which is what
                // a job with nothing to do would otherwise do.
                let percent = (done * 100).checked_div(total).unwrap_or(0);
                view! {
                    <div style="display:flex;gap:12px;align-items:center;margin-bottom:18px">
                        <div class="progress" style="flex:1">
                            {format!("{step} — {done} of {total}")}
                            <b>
                                <i style=format!("width:{percent}%")></i>
                                <em>{format!("{percent}%")}</em>
                            </b>
                        </div>
                        // Anything that runs for minutes needs a way out of it.
                        {move || job_now.get().map(|id| view! {
                            <button
                                class="btn btn--danger btn--quiet"
                                style="font-size:13px"
                                title="Stop after the one it is on. Everything done so far is kept."
                                on:click=move |_| {
                                    crate::log::step("job", "asked it to stop");
                                    spawn_local(async move {
                                        let _ = api::stop_job(id).await;
                                    });
                                }
                            >
                                "Stop"
                            </button>
                        })}
                    </div>
                }
            })}

            {move || message.get().map(|outcome| match outcome {
                Ok(text) => view! { <div class="alert is-ok">{text}</div> },
                Err(text) => view! { <div class="alert is-bad">{text}</div> },
            })}

            // `<Suspense>` shows the fallback until everything read inside it has
            // arrived, then swaps in the real thing.
            // The way back to the search box, once the list has carried you away
            // from it.
            {move || deep.get().then(|| view! {
                <button
                    class="btn"
                    style="position:fixed;right:20px;bottom:20px;z-index:40;font-size:15px;\
                           padding:9px 14px;background:var(--wl-soot)"
                    title="Back to the top"
                    on:click=move |_| {
                        if let Some(window) = web_sys::window() {
                            let how = web_sys::ScrollToOptions::new();
                            how.set_top(0.0);
                            how.set_behavior(web_sys::ScrollBehavior::Smooth);
                            window.scroll_to_with_scroll_to_options(&how);
                        }
                    }
                >
                    "↑ top"
                </button>
            })}

            // The map is a view of whatever the filters are showing.
            <Map
                pins=pins
                city=pins_city
                home=home_spot
                centre=(47.514900, 19.077800)
            />

            <div class="fleuron">"\u{2766}"</div>

            // What the list is keeping off the screen, and the way back.
            <HiddenPanel count=hidden_count on_change=set_reload reload=reload/>

            // What you got done today, and what you have got done ever.
            <crate::tally::Scoreboard tally=tally/>

            // The calls you said you would make. Hides itself when there are none.
            <ComingUp reload=contacts/>

            // Everything that narrows the list, right above the rows it narrows — by
            // city, then search, then state, then the sliders.
            <FilterBar
                asking=asking set_asking=set_asking
                only=only set_only=set_only
                counts=state_counts trades=trades_on_list cities=cities_on_list
                extremes=extremes
                searches=reload
            >
                // Which cities, and how many trades — read off the list rather than
                // named here.
                {move || cities_on_list.get().is_empty().then(|| view! {
                    <span>"Nothing gathered yet. "</span>
                })}
                {move || (!cities_on_list.get().is_empty()).then(|| view! {
                    <span>{describe_cities(&cities_on_list.get())}</span>
                })}
                {move || trades.get().and_then(Result::ok).map(|t| {
                    format!("{} trades on the list. ", t.trades.len())
                })}
            </FilterBar>

            <Suspense fallback=move || view! {
                <div class="empty"><span class="spinner"></span>" Reading the list…"</div>
            }>
                {move || match leads.get() {
                    None => ().into_view(),
                    Some(Err(problem)) => view! {
                        <div class="alert is-bad">{problem}</div>
                    }.into_view(),
                    Some(Ok(response)) => {
                        let total = response.total;
                        // The rows come from the signal, not from the response, so a
                        // "load more" adds to what is already on screen.
                        view! {
                            {move || {
                                let showing = rows.get();
                                if showing.is_empty() {
                                    let narrowed = {
                                        let a = asking.get();
                                        !search.get().trim().is_empty()
                                            || only.get().is_some()
                                            || a.least_reviews > 0
                                            || a.least_rating > 0.0
                                            || a.within_m > 0
                                            || a.trade.is_some()
                                            || a.why.is_some()
                                            || a.from_run.is_some()
                                            || a.from_area.is_some()
                                            || a.city.is_some()
                                    };
                                    view! {
                                        <div class="empty">
                                            {if narrowed {
                                                "Nothing matches what you have narrowed to."
                                            } else {
                                                "Nothing on the list yet. Search Google above to fill it."
                                            }}
                                            // Told apart from an empty list, and
                                            // undoable in one press.
                                            {narrowed.then(|| view! {
                                                <div style="margin-top:10px">
                                                    <button
                                                        class="btn btn--quiet"
                                                        style="font-size:15px"
                                                        on:click=move |_| {
                                                            set_search.set(String::new());
                                                            set_only.set(None);
                                                            set_asking.update(|a| {
                                                                a.least_reviews = 0;
                                                                a.least_rating = 0.0;
                                                                a.within_m = 0;
                                                                a.trade = None;
                                                                a.why = None;
                                                                a.from_run = None;
                                                                a.from_area = None;
                                                                a.city = None;
                                                            });
                                                        }
                                                    >
                                                        "Show everything again"
                                                    </button>
                                                </div>
                                            })}
                                        </div>
                                    }.into_view()
                                } else {
                                    let count = showing.len();
                                    let more = (count as i64) < total;
                                    view! {
                                        <LeadTable
                                            rows=showing asking=asking
                                            set_asking=set_asking on_change=set_reload
                                            on_contact=set_contacts
                                            shots=shots.get()
                                            targets=trades.get()
                                                .and_then(Result::ok)
                                                .map(|t| t.trades)
                                                .unwrap_or_default()
                                        />
                                        <div style="display:flex;gap:14px;align-items:center;margin-top:18px">
                                            <span class="is-dim" style="font-size:16px">
                                                {format!("Showing {count} of {total}")}
                                            </span>
                                            {more.then(|| view! {
                                                <button class="btn btn--quiet" on:click=load_more>
                                                    {move || if loading_more.get() { "Fetching…" } else { "Load more" }}
                                                </button>
                                            })}
                                        </div>
                                    }.into_view()
                                }
                            }}
                        }.into_view()
                    }
                }}
            </Suspense>
        </div>
    }
}

/// Which cities the list holds, in a few words, read off what is actually there.
///
/// Pure, and tested — the wording has to hold for one city, two, and a dozen.
pub fn describe_cities(cities: &[(i64, String, i64)]) -> String {
    let names: Vec<&str> = cities.iter().map(|(_, name, _)| name.as_str()).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => format!("{one}. "),
        [first, second] => format!("{first} and {second}. "),
        // Past two, the chips above say which; a sentence listing a dozen is a
        // sentence nobody finishes.
        many => format!("{} cities. ", many.len()),
    }
}

/// What a round of picture-taking came to, in one line.
///
/// Pure, so the sentence is a test rather than something you read off a screen once
/// and never again.
fn describe_pictures(summary: &api::Pictures) -> String {
    match (summary.taken, summary.of) {
        (0, 0) => "nothing to photograph".to_owned(),
        (0, of) => format!("no pictures taken, of {of} tried"),
        (taken, of) if taken == of => format!("{taken} pictures taken"),
        (taken, of) => format!("{taken} pictures taken, of {of} tried"),
    }
}

#[cfg(test)]
#[path = "leads_tests.rs"]
mod tests;
