//! The scraper's own settings, on the page rather than buried in the code.
//!
//! # What this file is here to teach
//!
//! Showing a default rather than hiding it. Every one of these has a sensible value
//! and a sentence saying what moving it does; the sentence lives beside the setting
//! in `domain`.

use leptos::*;

use webleads_domain::options::{CITY_ZOOM, DEEPEST, USUAL_DEPTH};
use webleads_domain::{ScrapeOptions, LANGUAGES};

/// One setting the page can move, and what it is worth saying about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Language,
    Zoom,
    Depth,
    TimeLimit,
    FastMode,
}

impl Setting {
    /// The name the person reads.
    pub fn label(self) -> &'static str {
        match self {
            Self::Language => "Language",
            Self::Zoom => "Zoom",
            Self::Depth => "Depth",
            Self::TimeLimit => "Give up after",
            Self::FastMode => "Fast mode",
        }
    }

    /// The name the explanation is filed under, in `domain`.
    pub fn field(self) -> &'static str {
        match self {
            Self::Language => "lang",
            Self::Zoom => "zoom",
            Self::Depth => "depth",
            Self::TimeLimit => "max_time_s",
            Self::FastMode => "fast_mode",
        }
    }

    /// The sentence saying what it does. Written in `domain`, beside the setting.
    pub fn help(self) -> &'static str {
        ScrapeOptions::help(self.field()).unwrap_or("")
    }

    /// Everything the page offers, in the order it shows them.
    pub const ALL: [Setting; 5] = [
        Self::Language,
        Self::Zoom,
        Self::Depth,
        Self::TimeLimit,
        Self::FastMode,
    ];
}

/// Is this setting still at the value we would have used anyway?
///
/// Pure, so "you have moved something" is a tested rule rather than an impression.
pub fn at_default(setting: Setting, options: &ScrapeOptions) -> bool {
    let ours = ScrapeOptions::default();
    match setting {
        // Remembered rather than moved: the language is yours until you pick another,
        // and the server searches in it whether or not the settings are sent.
        Setting::Language => true,
        Setting::Zoom => options.zoom == ours.zoom,
        Setting::Depth => options.depth == ours.depth,
        Setting::TimeLimit => options.max_time_s == ours.max_time_s,
        Setting::FastMode => options.fast_mode == ours.fast_mode,
    }
}

/// What fast mode does to the two settings it cares about.
///
/// It **needs** a zoom — without a legal one the job is accepted, dies mid-run, and
/// then blocks every other job on the scraper — and it takes no notice of the depth,
/// which returns twenty per search whatever it says. So it takes the zoom over and
/// hides both switches: a control that does nothing, or that you can break the run
/// with, is worse than no control.
///
/// Returns what the zoom should become. Pure, so "turning it off puts things back"
/// is a test rather than something you find out by clicking.
pub fn zoom_for_fast(fast_on: bool, before: Option<u8>) -> Option<u8> {
    if fast_on {
        // Whatever it was, it is twelve now.
        Some(CITY_ZOOM)
    } else {
        // Back as it was before fast mode took it — off if it was off. You never
        // asked for a zoom of twelve; you should not be left holding one.
        before
    }
}

/// Is this setting's own switch shown at all?
///
/// Both of them belong to fast mode while fast mode is on.
pub fn switch_shown(setting: Setting, fast_on: bool) -> bool {
    !(fast_on && matches!(setting, Setting::Zoom | Setting::Depth))
}

/// How many settings have been moved off their default.
pub fn moved(options: &ScrapeOptions) -> usize {
    Setting::ALL
        .iter()
        // What fast mode forced is not something you moved: switching one thing on
        // would otherwise claim you had changed two.
        .filter(|s| switch_shown(**s, options.fast_mode))
        .filter(|s| !at_default(**s, options))
        .count()
}

/// What the heading says about the settings.
pub fn describe_moved(moved: usize) -> String {
    match moved {
        0 => "Settings — all at their usual values".to_owned(),
        1 => "Settings — 1 changed".to_owned(),
        n => format!("Settings — {n} changed"),
    }
}

/// How long a time limit reads, in words.
pub fn describe_limit(seconds: u32) -> String {
    match seconds {
        0 => "no limit at all".to_owned(),
        s if s < 120 => format!("{s} seconds"),
        s if s < 5_400 => format!("{} minutes", s / 60),
        s => format!("{} hours", s / 3600),
    }
}

#[component]
pub fn Settings(
    options: ReadSignal<ScrapeOptions>,
    set_options: WriteSignal<ScrapeOptions>,
    /// Bumped when your clock or your starting point changes, so the list is read
    /// again.
    on_clock: WriteSignal<u32>,
    /// The city of the place being searched: the one whose starting point you move.
    city: Signal<Option<i64>>,
) -> impl IntoView {
    let note = move |setting: Setting| {
        view! {
            <div class="is-muted" style="font-size:14px;margin-top:3px;max-width:44ch">
                {setting.help()}
            </div>
        }
    };
    // What the zoom was before fast mode took it over, so switching fast off can put
    // it back rather than leaving you holding a twelve you never asked for.
    let zoom_before: StoredValue<Option<u8>> = store_value(options.get_untracked().zoom);
    let fast_on = move || options.get().fast_mode;

    let marked = move |setting: Setting| {
        if at_default(setting, &options.get()) {
            "field__label"
        } else {
            "field__label is-accent"
        }
    };

    view! {
        <div style="display:flex;gap:10px;align-items:center;margin:4px 0 10px">
            <div class="field__label" style="margin:0">
                {move || describe_moved(moved(&options.get()))}
            </div>
            {move || (moved(&options.get()) > 0).then(|| view! {
                <button
                    class="btn btn--quiet"
                    style="font-size:14px;padding:3px 7px"
                    on:click=move |_| set_options.update(|o| {
                        *o = ScrapeOptions { lang: o.lang.clone(), ..ScrapeOptions::default() };
                    })
                >
                    "back to the usual"
                </button>
            })}
        </div>

        <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(230px,1fr));gap:16px;\
                    margin-bottom:14px">
            // Two columns wide, so your clock and your starting point fit side by side.
            <div class="wl-lang-cell">
                <div class=move || marked(Setting::Language)>
                    {Setting::Language.label()}
                </div>
                <select
                    class="select"
                    on:change=move |ev| {
                        let picked = event_target_value(&ev);
                        set_options.update(|o| o.lang = picked.clone());
                        // Remembered on the server, so the next visit searches in it too.
                        spawn_local(async move {
                            if let Err(reason) = crate::api::set_search_lang(&picked).await {
                                crate::terminal::say(reason, crate::terminal::Tone::Bad);
                            }
                        });
                    }
                >
                    // The languages the trade list actually has words for. Offering
                    // one it has none for would be offering a search that returns
                    // nothing and takes three minutes to say so.
                    {LANGUAGES.into_iter().map(|(code, name)| {
                        let chosen = move || options.get().lang == code;
                        view! { <option value=code selected=chosen>{name}</option> }
                    }).collect_view()}
                </select>
                {note(Setting::Language)}
                // Your clock, and your starting point to the right of it.
                <div class="wl-you">
                    <div class="wl-you__half">
                        <crate::home::YourClock on_changed=on_clock/>
                    </div>
                    <div class="wl-you__half">
                        <crate::home::StartingPoint city=city on_moved=on_clock/>
                    </div>
                </div>
            </div>

            // Off by default, and left out of the request entirely while it is off:
            // Google chooses its own view, which is what you want for a district or
            // a town. A fixed sixteen is a few streets wide and misses the edges of
            // anything bigger.
            //
            // While fast mode is on it belongs to fast mode — see `zoom_for_fast`.
            {move || switch_shown(Setting::Zoom, fast_on()).then(|| view! {
                <div>
                    <label class="wl-optional">
                        <input
                            type="checkbox"
                            prop:checked=move || options.get().zoom.is_some()
                            on:change=move |ev| {
                                let on = event_target_checked(&ev);
                                let zoom = on.then_some(CITY_ZOOM);
                                zoom_before.set_value(zoom);
                                set_options.update(|o| o.zoom = zoom);
                            }
                        />
                        "Set the zoom level"
                    </label>
                    {move || options.get().zoom.map(|zoom| view! {
                        <div class=move || marked(Setting::Zoom)>
                            {Setting::Zoom.label()}" — "
                            <span class="is-accent">{zoom.to_string()}</span>
                        </div>
                        <input
                            class="slider" type="range" min="1" max="21" step="1"
                            prop:value=zoom.to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<u8>() {
                                    zoom_before.set_value(Some(v));
                                    set_options.update(|o| o.zoom = Some(v));
                                }
                            }
                        />
                    })}
                    {note(Setting::Zoom)}
                </div>
            })}

            // The slider stops at fifteen because sixteen upwards was measured
            // returning nothing at all — see `USUAL_DEPTH` in `domain`.
            {move || switch_shown(Setting::Depth, fast_on()).then(|| view! {
                <div>
                    <label class="wl-optional">
                        <input
                            type="checkbox"
                            prop:checked=move || options.get().depth.is_some()
                            on:change=move |ev| {
                                let on = event_target_checked(&ev);
                                set_options.update(|o| o.depth = on.then_some(USUAL_DEPTH));
                            }
                        />
                        "Set how far to scroll"
                    </label>
                    {move || options.get().depth.map(|depth| view! {
                        <div class=move || marked(Setting::Depth)>
                            {Setting::Depth.label()}" — "
                            <span class="is-accent">{depth.to_string()}</span>
                        </div>
                        <input
                            class="slider" type="range" min="1" max=DEEPEST.to_string() step="1"
                            prop:value=depth.to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<u8>() {
                                    set_options.update(|o| o.depth = Some(v));
                                }
                            }
                        />
                    })}
                    {note(Setting::Depth)}
                </div>
            })}

            <div>
                <div class=move || marked(Setting::TimeLimit)>
                    {Setting::TimeLimit.label()}" — "
                    <span class="is-accent">
                        {move || describe_limit(options.get().max_time_s)}
                    </span>
                </div>
                <input
                    class="slider" type="range" min="300" max="7200" step="300"
                    prop:value=move || options.get().max_time_s.to_string()
                    on:input=move |ev| {
                        if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                            set_options.update(|o| o.max_time_s = v);
                        }
                    }
                />
                {note(Setting::TimeLimit)}
            </div>

            <div>
                <div class=move || marked(Setting::FastMode)>{Setting::FastMode.label()}</div>
                <label class="switch" style="margin-top:6px">
                    <input
                        type="checkbox"
                        prop:checked=move || options.get().fast_mode
                        on:change=move |ev| {
                            let on = event_target_checked(&ev);
                            set_options.update(|o| {
                                o.fast_mode = on;
                                // It needs a legal zoom or the job hangs, and it
                                // ignores the depth entirely. Both switches go.
                                o.zoom = zoom_for_fast(on, zoom_before.get_value());
                            });
                        }
                    />
                    <span><b></b></span>
                    {move || if options.get().fast_mode { "on" } else { "off" }}
                </label>
                {note(Setting::FastMode)}
                {move || options.get().fast_mode.then(|| view! {
                    <div class="is-warn" style="font-size:14px;margin-top:4px;max-width:44ch">
                        "Twenty businesses per search, whatever the zoom and the \
                         scroll say — so both are set for you and put away. It looks \
                         wherever the words point, so name the place in the words."
                    </div>
                })}
            </div>
        </div>
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
