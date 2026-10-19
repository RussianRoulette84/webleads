//! The controls that narrow the call list.
//!
//! One collapsible group rather than another row of things above the table — with a
//! search box, five filters, a map and sortable headings, the page runs out of room
//! quickly.

use leptos::*;
use webleads_domain::{Due, Status, Verdict};

use crate::api::{Asking, Extremes};
use crate::cities::CityRow;
use crate::scale::{reach, step_for};
use crate::searches::{chip_class, PastSearches};

/// Every reason a business can be on the list, for the "why" filter.
const REASONS: [Verdict; 4] = [
    Verdict::NoSite,
    Verdict::SocialOnly,
    Verdict::DeadSite,
    Verdict::InsecureSite,
];

/// How many unanswered calls the "no answer" option offers to narrow by.
const RUNG_AT_LEAST: [i64; 3] = [2, 3, 4];

/// Everything that narrows the list, in one box.
///
/// Which city, which search, which state, and the sliders. They belong together — you
/// pick a search, then a state, then you tighten it — and in one box you can see
/// everything you have narrowed to.
#[component]
pub fn FilterBar(
    asking: ReadSignal<Asking>,
    set_asking: WriteSignal<Asking>,
    /// The state the list is narrowed to, and the way to change it.
    only: ReadSignal<Option<Status>>,
    set_only: WriteSignal<Option<Status>>,
    /// How many are in each state, as the server counted them.
    counts: Memo<Vec<(String, i64)>>,
    /// The trades actually on the list: the name stored, the name to show, and how
    /// many of each. The two names differ — one is English, one is Hungarian.
    trades: Memo<Vec<(String, String, i64)>>,
    /// The cities with anything on the list, for the row above everything else.
    cities: Memo<Vec<(i64, String, i64)>>,
    /// How far the sliders need to reach, and what the rating one would hide.
    extremes: Memo<Extremes>,
    /// Bumped whenever a search starts or finishes, so the strip keeps up.
    searches: ReadSignal<u32>,
    /// What the list is, said under the heading: its cities and its trades.
    children: Children,
) -> impl IntoView {
    // Pressing the state you are already in takes the narrowing off: that is the way
    // back to everybody.
    let pick = move |status: Status| {
        set_only.update(|now| *now = (*now != Some(status)).then_some(status));
        // The options belong to the state that owns them; leaving it drops them.
        set_asking.update(|a| {
            a.rings_at_least = 0;
            a.due = None;
        });
    };

    view! {
        // Framed like the masthead: the one box the list answers to.
        <section class="wl-frame wl-filter">
            <div class="wl-frame__inner">
                <h4 class="heading">"Filter results"</h4>
                <p class="is-dim wl-filter__about">{children()}</p>
                // City, then search, then state — each one narrows what the one above it
                // left, and they are read top to bottom.
                <CityRow cities=cities asking=asking set_asking=set_asking/>
                <PastSearches reload=searches asking=asking set_asking=set_asking/>

                <div class="field__label wl-filter__label">"Status"</div>
                <div class="wl-strip">
                    {Status::ALL.into_iter().map(|status| {
                        let n = move || counts.get()
                            .iter()
                            .find(|(name, _)| *name == status.as_str())
                            .map(|(_, n)| *n)
                            .unwrap_or(0);
                        let chosen = move || only.get() == Some(status);
                        view! {
                            <button
                                class=move || chip_class(chosen())
                                title="Press again to show everybody"
                                on:click=move |_| pick(status)
                            >
                                {status.describe()}
                                <span class="wl-chip__count">{n}</span>
                            </button>
                        }
                    }).collect_view()}
                </div>

                // What the chosen state carries with it. Nothing for the states that
                // carry nothing — an empty row of options is furniture.
                {move || match only.get() {
                    Some(Status::NotReached) => view! {
                        <div class="wl-strip wl-strip--options">
                            <span class="is-dim wl-strip__lead">"rung at least"</span>
                            <button
                                class=move || chip_class(asking.get().rings_at_least == 0)
                                on:click=move |_| set_asking.update(|a| a.rings_at_least = 0)
                            >
                                "any"
                            </button>
                            {RUNG_AT_LEAST.into_iter().map(|least| view! {
                                <button
                                    class=move || chip_class(asking.get().rings_at_least == least)
                                    on:click=move |_| set_asking.update(|a| a.rings_at_least = least)
                                >
                                    {format!("{least}×")}
                                </button>
                            }).collect_view()}
                        </div>
                    }.into_view(),
                    Some(Status::FollowUp) => view! {
                        <div class="wl-strip wl-strip--options">
                            <span class="is-dim wl-strip__lead">"due"</span>
                            <button
                                class=move || chip_class(asking.get().due.is_none())
                                on:click=move |_| set_asking.update(|a| a.due = None)
                            >
                                "any time"
                            </button>
                            {Due::ALL.into_iter().map(|due| view! {
                                <button
                                    class=move || chip_class(asking.get().due == Some(due))
                                    on:click=move |_| set_asking.update(|a| a.due = Some(due))
                                >
                                    {due.describe()}
                                </button>
                            }).collect_view()}
                        </div>
                    }.into_view(),
                    _ => ().into_view(),
                }}

                <Filters
                    asking=asking set_asking=set_asking
                    trades=trades extremes=extremes
                />
            </div>
        </section>
    }
}

#[component]
pub fn Filters(
    asking: ReadSignal<Asking>,
    set_asking: WriteSignal<Asking>,
    /// The trades actually on the list: stored name, shown name, how many.
    trades: Memo<Vec<(String, String, i64)>>,
    /// How far the sliders need to reach, and what the rating one would hide.
    extremes: Memo<Extremes>,
) -> impl IntoView {
    // How many filters are doing something, so a collapsed group still says whether
    // you are looking at everything or not.
    let narrowing = move || {
        let a = asking.get();
        usize::from(a.least_reviews > 0)
            + usize::from(a.least_rating > 0.0)
            + usize::from(a.within_m > 0)
            + usize::from(a.trade.is_some())
            + usize::from(a.why.is_some())
    };

    let clear = move |_| {
        set_asking.update(|a| {
            a.least_reviews = 0;
            a.least_rating = 0.0;
            a.within_m = 0;
            a.trade = None;
            a.why = None;
        });
    };

    view! {
        // The filters are always on the screen. Behind a button they were a thing to
        // remember rather than a thing to use, and the only question left is whether
        // any of them are doing something — which the line below answers.
        <div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap;margin-bottom:12px">
            {move || (narrowing() > 0).then(|| view! {
                <span class="is-dim" style="font-size:15px">
                    {said_narrowing(narrowing())}
                </span>
                <button
                    class="btn btn--quiet"
                    style="font-size:15px;padding:4px 9px"
                    on:click=clear
                >
                    "Show everything"
                </button>
            })}
        </div>

        {view! {
            <div class="panel" style="padding:14px;margin-bottom:14px">
                <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(230px,1fr));gap:16px">

                    <Slider
                        label="Reviews, at least"
                        value=Signal::derive(move || asking.get().least_reviews as f64)
                        max=Signal::derive(move || reach(extremes.get().most_reviews, 20))
                        step=Signal::derive(move || step_for(reach(extremes.get().most_reviews, 20)))
                        shown=Signal::derive(move || {
                            let n = asking.get().least_reviews;
                            if n == 0 { "any".to_owned() } else { n.to_string() }
                        })
                        on_change=move |v| set_asking.update(|a| a.least_reviews = v as i64)
                    />

                    <Slider
                        label="Rating, at least"
                        value=Signal::derive(move || asking.get().least_rating)
                        max=Signal::derive(|| 5.0)
                        step=Signal::derive(|| 0.1)
                        shown=Signal::derive(move || {
                            let r = asking.get().least_rating;
                            if r <= 0.0 { "any".to_owned() } else { format!("{r:.1}★") }
                        })
                        // Raising it silently dropped every business Google has no
                        // stars for, which on this list is most of the good ones.
                        note=Signal::derive(move || {
                            let hidden = extremes.get().unrated;
                            if asking.get().least_rating > 0.0 && hidden > 0 {
                                format!("hides {hidden} with no rating")
                            } else {
                                String::new()
                            }
                        })
                        on_change=move |v| set_asking.update(|a| a.least_rating = v)
                    />

                    <Slider
                        label="Within"
                        value=Signal::derive(move || asking.get().within_m as f64)
                        max=Signal::derive(move || reach(extremes.get().furthest_m, 2_000))
                        step=Signal::derive(move || step_for(reach(extremes.get().furthest_m, 2_000)))
                        shown=Signal::derive(move || {
                            let m = asking.get().within_m;
                            if m == 0 {
                                "anywhere".to_owned()
                            } else if m < 1000 {
                                format!("{m} m")
                            } else {
                                format!("{:.1} km", m as f64 / 1000.0)
                            }
                        })
                        on_change=move |v| set_asking.update(|a| a.within_m = v as i64)
                    />

                    <div>
                        <div class="field__label">"Trade"</div>
                        <select
                            class="select"
                            on:change=move |ev| {
                                let picked = event_target_value(&ev);
                                set_asking.update(|a| {
                                    a.trade = (!picked.is_empty()).then_some(picked);
                                });
                            }
                        >
                            <option value="" selected=move || asking.get().trade.is_none()>
                                "any trade"
                            </option>
                            {move || trades.get().into_iter().map(|(stored, shown, n)| {
                                let chosen = asking.get().trade.as_deref() == Some(stored.as_str());
                                view! {
                                    // The value is what the server knows this trade
                                    // by; the words are what you read.
                                    <option value=stored selected=chosen>
                                        {format!("{shown} ({n})")}
                                    </option>
                                }
                            }).collect_view()}
                        </select>
                    </div>

                    <div>
                        <div class="field__label">"Why they qualified"</div>
                        <div style="display:flex;gap:5px;flex-wrap:wrap">
                            <button
                                class=move || { if asking.get().why.is_none() { "btn" } else { "btn btn--quiet" } }
                                style="font-size:14px;padding:3px 7px"
                                on:click=move |_| set_asking.update(|a| a.why = None)
                            >
                                "any"
                            </button>
                            {REASONS.into_iter().map(|why| view! {
                                <button
                                    class=move || { if asking.get().why == Some(why) { "btn" } else { "btn btn--quiet" } }
                                    style="font-size:14px;padding:3px 7px"
                                    on:click=move |_| set_asking.update(|a| a.why = Some(why))
                                >
                                    {why.describe()}
                                </button>
                            }).collect_view()}
                        </div>
                    </div>
                </div>
            </div>
        }}
    }
}

/// How many filters are doing something, in words.
///
/// Pure, so the sentence is a test rather than something read off a screen.
pub fn said_narrowing(how_many: usize) -> String {
    match how_many {
        1 => "1 filter is narrowing the list".to_owned(),
        n => format!("{n} filters are narrowing the list"),
    }
}

/// One slider with its label and what it currently means.
///
/// Shared rather than copied: the area's reach is the same control as the distance
/// filter, and two of them would drift apart.
#[component]
pub fn Slider<F>(
    label: &'static str,
    value: Signal<f64>,
    /// The top of the track, which comes from the data rather than a guess.
    max: Signal<f64>,
    step: Signal<f64>,
    /// What the number means in words — "any", "4.2★", "3.0 km".
    shown: Signal<String>,
    /// What this setting costs, shown only while it is costing something.
    #[prop(optional, into)]
    note: Signal<String>,
    on_change: F,
) -> impl IntoView
where
    F: Fn(f64) + 'static,
{
    view! {
        <div>
            <div class="field__label">
                {label}" — "<span class="is-accent">{move || shown.get()}</span>
            </div>
            <input
                class="slider"
                type="range"
                min="0"
                max=move || max.get().to_string()
                step=move || step.get().to_string()
                prop:value=move || value.get().to_string()
                on:input=move |ev| {
                    if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                        on_change(v);
                    }
                }
            />
            {move || {
                let note = note.get();
                (!note.is_empty()).then(|| view! {
                    <div class="is-muted" style="font-size:14px;margin-top:2px">{note}</div>
                })
            }}
        </div>
    }
}

#[cfg(test)]
#[path = "filters_tests.rs"]
mod tests;
