//! Choosing what to search for: the trades, and words of your own.
//!
//! # What this file is here to teach
//!
//! A chooser you can see the state of: type to narrow, press to choose, and what you
//! have chosen sits in front of you as something you can take back off.

use leptos::*;
use webleads_domain::{Target, Tier};

/// Does this trade match what you typed?
///
/// Either language: the English name or any of the words in any language. Pure, so
/// "szerel" finding the plumbers is a test rather than a hope.
///
/// Every language, not only the one being searched: you may well know a trade by its
/// English name while searching Dubai, or the other way about.
pub fn matches(target: &Target, typed: &str) -> bool {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return true;
    }
    target.english.to_lowercase().contains(&typed)
        || target
            .words
            .values()
            .flatten()
            .any(|word| word.to_lowercase().contains(&typed))
}

/// The trades that can be searched in this language at all.
///
/// A trade with no words in it is not offered: there is no pálinka distillery in
/// Dubai, and choosing one would put a word into the run that finds nothing and costs
/// three minutes to find that out.
///
/// Pure, and tested — "the picker only offers what this city has" is otherwise
/// something you check by scrolling a list of a hundred.
pub fn searchable_in(targets: &[Target], lang: &str) -> Vec<Target> {
    targets
        .iter()
        .filter(|t| t.searched_in(lang))
        .cloned()
        .collect()
}

/// How many searches a choice comes to, and how many trades it covers.
///
/// A trade with two words in the language being searched is two searches, which is
/// why the count of trades is not the count of searches — and it is the searches that
/// cost an hour.
pub fn how_much(
    targets: &[Target],
    chosen: &[String],
    own: &[String],
    lang: &str,
) -> (usize, usize) {
    let picked = || targets.iter().filter(|t| chosen.contains(&t.english));
    let trades = picked().filter(|t| t.searched_in(lang)).count();
    let searches: usize = picked().map(|t| t.words(lang).len()).sum();
    (searches + own.len(), trades)
}

/// What the count says, in words.
/// Where the next search will look, and in what language.
///
/// Said in words, so nobody has to work it out from the word counts.
///
/// Pure, and tested. A language we do not know says so rather than reading as some
/// other language.
pub fn searching_said(place: &str, lang: &str) -> String {
    let place = place.trim();
    let named = match lang.trim() {
        "" => return format!("Searching {place} — the language is not set for it"),
        code => webleads_domain::LANGUAGES
            .iter()
            .find(|(known, _)| *known == code)
            .map(|(_, name)| (*name).to_owned())
            .unwrap_or_else(|| code.to_owned()),
    };
    format!("Searching {place} in {named}")
}

pub fn describe_choice(searches: usize, trades: usize, own: usize) -> String {
    if searches == 0 {
        return "nothing chosen yet".to_owned();
    }
    let mut said = format!("{searches} search{}", if searches == 1 { "" } else { "es" });
    if trades > 0 {
        said.push_str(&format!(
            " across {trades} trade{}",
            if trades == 1 { "" } else { "s" }
        ));
    }
    if own > 0 {
        said.push_str(&format!(", {own} of your own"));
    }
    said
}

/// Every tier, best first: the buttons, and what "All" chooses.
pub const TIERS: [Tier; 4] = [Tier::S, Tier::A, Tier::B, Tier::C];

/// The names of every trade in one tier that can be searched in this language, in
/// list order — the same trades the tier's count counts. Pure.
pub fn tier_names(targets: &[Target], tier: Tier, lang: &str) -> Vec<String> {
    searchable_in(targets, lang)
        .into_iter()
        .filter(|t| t.tier == tier)
        .map(|t| t.english)
        .collect()
}

#[component]
pub fn TradePicker(
    targets: ReadSignal<Vec<Target>>,
    tiers: ReadSignal<Vec<Tier>>,
    set_tiers: WriteSignal<Vec<Tier>>,
    trades: ReadSignal<Vec<String>>,
    set_trades: WriteSignal<Vec<String>>,
    /// The language the place being searched describes its trades in. Only the trades
    /// with words in it are offered — a pálinka distillery in Dubai is a search that
    /// finds nothing and takes three minutes to say so.
    lang: Memo<String>,
) -> impl IntoView {
    let (typed, set_typed) = create_signal(String::new());
    let lang_now = move || lang.get();

    let toggle = move |name: String| {
        set_trades.update(|chosen| match chosen.iter().position(|t| *t == name) {
            Some(at) => {
                chosen.remove(at);
            }
            None => chosen.push(name),
        });
    };

    let take_tier = move |tier: Tier| {
        let names = tier_names(&targets.get_untracked(), tier, &lang_now());
        let already = tiers.get().contains(&tier);
        set_tiers.update(|chosen| match chosen.iter().position(|t| *t == tier) {
            Some(at) => {
                chosen.remove(at);
            }
            None => chosen.push(tier),
        });
        // The tier buttons and the ticks say the same thing, so pressing a tier ticks
        // its trades rather than leaving two different answers on screen.
        set_trades.update(|chosen| {
            if already {
                chosen.retain(|name| !names.contains(name));
            } else {
                for name in names {
                    if !chosen.contains(&name) {
                        chosen.push(name);
                    }
                }
            }
        });
    };

    view! {
        // Whole tiers, for the common case.
        <div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap;margin-bottom:10px">
            <span class="field__label" style="margin:0 6px 0 0">"Tiers"</span>
            {TIERS.into_iter().map(|tier| {
                // Inside the closure, not outside it. Read once while the view was
                // built, the count froze at whatever the language was then — and at
                // that moment it was still empty, so every tier read "(0)" for ever.
                let count = move || {
                    searchable_in(&targets.get(), &lang_now())
                        .iter()
                        .filter(|t| t.tier == tier)
                        .count()
                };
                view! {
                    <button
                        class=move || if tiers.get().contains(&tier) { "btn" } else { "btn btn--quiet" }
                        style="font-size:15px;padding:4px 9px"
                        on:click=move |_| take_tier(tier)
                    >
                        {move || format!("{tier:?} ({})", count())}
                    </button>
                }
            }).collect_view()}
            // Every tier at once, with every trade in them ticked.
            <button
                class=move || if tiers.get().len() == TIERS.len() { "btn" } else { "btn btn--quiet" }
                style="font-size:15px;padding:4px 9px"
                on:click=move |_| {
                    let lang = lang_now();
                    let targets = targets.get_untracked();
                    set_trades.set(
                        TIERS.iter().flat_map(|tier| tier_names(&targets, *tier, &lang)).collect(),
                    );
                    set_tiers.set(TIERS.to_vec());
                }
            >
                {move || format!("All ({})", searchable_in(&targets.get(), &lang_now()).len())}
            </button>
            <button
                class="btn btn--quiet"
                style="font-size:15px;padding:4px 9px;margin-left:8px"
                on:click=move |_| {
                    set_trades.set(Vec::new());
                    set_tiers.set(Vec::new());
                }
            >
                "none"
            </button>
        </div>

        // What you have chosen, in front of you rather than behind a toggle.
        {move || (!trades.get().is_empty()).then(|| view! {
            <div class="multiselect" style="margin-bottom:10px">
                {trades.get().into_iter().map(|name| {
                    let mine = name.clone();
                    view! {
                        <button
                            class="wl-tag"
                            title="Take this one off"
                            on:click=move |_| toggle(mine.clone())
                        >
                            {name}" ✕"
                        </button>
                    }
                }).collect_view()}
            </div>
        })}

        // Type to narrow the hundred.
        <label class="searchbox" style="margin-bottom:8px;display:block">
            <input
                class="field field--search"
                type="search"
                placeholder="narrow the trades — “szerel”, “plumb”, “fodrász”…"
                prop:value=move || typed.get()
                on:input=move |ev| set_typed.set(event_target_value(&ev))
            />
            <button
                class="clear"
                hidden=move || typed.get().is_empty()
                on:click=move |_| set_typed.set(String::new())
            >
                "✕"
            </button>
        </label>

        <div class="listbox__panel" style="position:static;max-height:210px">
            {move || {
                let showing: Vec<Target> = searchable_in(&targets.get(), &lang_now())
                    .into_iter()
                    .filter(|t| matches(t, &typed.get()))
                    .collect();
                if showing.is_empty() {
                    return view! {
                        <div class="is-dim" style="font-size:16px;padding:8px 12px">
                            "No trade matches that."
                        </div>
                    }
                    .into_view();
                }
                showing.into_iter().map(|target| {
                    let name = target.english.clone();
                    let chosen = move || trades.get().contains(&name);
                    let mine = target.english.clone();
                    view! {
                        <div
                            class="listbox__option"
                            aria-selected=move || chosen().to_string()
                            on:click=move |_| toggle(mine.clone())
                        >
                            <span class="tick">"✓"</span>
                            <span style="flex:1">
                                {target.english.clone()}
                                <span class="is-dim" style="margin-left:8px">
                                    {target.words(lang_now().as_str()).join(" · ")}
                                </span>
                            </span>
                            <span class="is-dim" style="font-size:15px">
                                {format!("{:?}", target.tier)}
                            </span>
                        </div>
                    }
                }).collect_view()
            }}
        </div>
    }
}

/// Words of your own, added one at a time.
#[component]
pub fn OwnWords(
    words: ReadSignal<Vec<String>>,
    set_words: WriteSignal<Vec<String>>,
) -> impl IntoView {
    let (typing, set_typing) = create_signal(String::new());

    let add = move || {
        let word = typing.get().trim().to_owned();
        if word.is_empty() {
            return;
        }
        set_words.update(|all| {
            if !all.contains(&word) {
                all.push(word);
            }
        });
        set_typing.set(String::new());
    };

    view! {
        <div style="padding-top:20px">
            <div class="field__label">"Or words of your own"</div>
            <div class="wl-float">
        <div style="display:flex;gap:8px;align-items:center;flex-wrap:wrap;margin-bottom:8px">
            <input
                class="field field--search"
                style="flex:1;min-width:220px"
                type="text"
                placeholder="a word to search for, then Enter"
                prop:value=move || typing.get()
                on:input=move |ev| set_typing.set(event_target_value(&ev))
                // The complaint this answers: typing here looked as though it did
                // nothing at all, because nothing appeared until the run started.
                on:keydown=move |ev| if ev.key() == "Enter" { add() }
            />
            <button
                class="btn btn--quiet"
                disabled=move || typing.get().trim().is_empty()
                on:click=move |_| add()
            >
                "Add"
            </button>
        </div>

        {move || (!words.get().is_empty()).then(|| view! {
            <div class="multiselect" style="margin-top:10px">
                {words.get().into_iter().map(|word| {
                    let mine = word.clone();
                    view! {
                        <button
                            class="wl-tag"
                            title="Take this one off"
                            on:click=move |_| set_words.update(|all| all.retain(|w| *w != mine))
                        >
                            {word}" ✕"
                        </button>
                    }
                }).collect_view()}
            </div>
        })}
            </div>
        </div>
    }
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;
