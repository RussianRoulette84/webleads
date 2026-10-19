//! The trade list, to read and to edit.
//!
//! # What this file is here to teach
//!
//! Editing a file through a screen without inventing a database for it. The hundred
//! trades live in one spreadsheet, hand-edited as often as not, and that file stays
//! the truth — this sends the whole list back and the server writes it out again.
//!
//! One list, not a row at a time. Saving a row would mean deciding what a row's
//! identity is and what happens when two of them are edited at once, and there is
//! one of you.

use leptos::*;
use webleads_domain::{Target, Tier, LANGUAGES};

use crate::api;

/// The trade list, closed until you open it, under the picker that chooses from it.
///
/// Closed, because a hundred rows of eight fields each would bury the picker under a
/// spreadsheet nobody asked to see.
#[component]
pub fn TradeList(
    /// The list the picker offers. Read from when the editor opens, written to when it
    /// saves, so the picker above never offers yesterday's list.
    list: RwSignal<Vec<Target>>,
) -> impl IntoView {
    let (open, set_open) = create_signal(false);
    let (rows, set_rows) = create_signal(Vec::<Target>::new());
    let (said, set_said) = create_signal(None::<String>);
    let (problem, set_problem) = create_signal(None::<String>);
    let (saving, set_saving) = create_signal(false);
    // What the file held when we last read or wrote it, so "unsaved changes" is a
    // fact rather than a flag somebody has to remember to set.
    let filed = store_value(Vec::<Target>::new());
    let (narrow, set_narrow) = create_signal(String::new());

    // Taken from the picker's list the first time it is opened — already loaded, so
    // there is nothing to fetch.
    let show = move |_| {
        let opening = !open.get();
        set_open.set(opening);
        if opening && rows.get().is_empty() {
            filed.set_value(list.get_untracked());
            set_rows.set(list.get_untracked());
        }
    };

    let save = move |_| {
        set_saving.set(true);
        set_said.set(None);
        set_problem.set(None);
        let sending = rows.get();
        spawn_local(async move {
            match api::save_targets(&sending).await {
                Ok(written) => {
                    filed.set_value(written.clone());
                    set_rows.set(written.clone());
                    list.set(written.clone());
                    set_said.set(Some(saved_said(written.len())));
                }
                Err(reason) => set_problem.set(Some(reason)),
            }
            set_saving.set(false);
        });
    };

    // Change one field of one row. The row is found by where it sits, because that is
    // what you pressed — a name is editable and cannot identify its own row.
    let edit = move |at: usize, change: Change| {
        set_rows.update(|all| {
            if let Some(row) = all.get_mut(at) {
                apply(row, change);
            }
        });
        set_said.set(None);
    };

    // A new trade starts from its name: an empty row is one the server refuses the
    // moment you save, so there is no way to add one.
    let (new_name, set_new_name) = create_signal(String::new());
    let can_add = move || new_trade_name(&new_name.get(), &rows.get());
    let add = move || {
        let Ok(name) = can_add() else {
            return;
        };
        set_rows.update(|all| {
            let next = all
                .iter()
                .map(|t| t.rank)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            all.push(blank_trade(next, name));
        });
        set_new_name.set(String::new());
        set_said.set(None);
    };

    let remove = move |at: usize| {
        set_rows.update(|all| {
            if at < all.len() {
                all.remove(at);
            }
        });
        set_said.set(None);
    };

    let unsaved = move || rows.get() != filed.get_value();

    view! {
        <div class="wl-trades">
            <div class="wl-trades__bar">
                <button class="btn" on:click=show>
                    {move || if open.get() { "DONE EDITING" } else { "EDIT THE TRADES" }}
                </button>
                {move || open.get().then(|| view! {
                    <span class="is-dim wl-trades__count">
                        {move || counted(&rows.get())}
                    </span>
                })}
                {move || (open.get() && unsaved()).then(|| view! {
                    <span class="is-warn wl-trades__count">"unsaved"</span>
                })}
            </div>

            {move || open.get().then(|| view! {
                <div class="wl-trades__tools">
                    <input
                        class="field wl-trades__find"
                        type="text"
                        placeholder="Narrow the list — a name, a word, a note"
                        prop:value=move || narrow.get()
                        on:input=move |ev| set_narrow.set(event_target_value(&ev))
                    />
                    <input
                        class="field wl-trades__find"
                        type="text"
                        placeholder="New trade, in English"
                        prop:value=move || new_name.get()
                        on:input=move |ev| set_new_name.set(event_target_value(&ev))
                        on:keydown=move |ev| if ev.key() == "Enter" { add() }
                    />
                    <button
                        class="btn btn--quiet"
                        disabled=move || can_add().is_err()
                        title=move || can_add().err().unwrap_or_default()
                        on:click=move |_| add()
                    >
                        "ADD A TRADE"
                    </button>
                    <button
                        class="btn"
                        disabled=move || saving.get() || !unsaved()
                        on:click=save
                    >
                        {move || if saving.get() { "SAVING…" } else { "SAVE THE LIST" }}
                    </button>
                    <button
                        class="btn btn--quiet"
                        title="Throw away every change since the last save"
                        disabled=move || saving.get() || !unsaved()
                        on:click=move |_| {
                            set_rows.set(filed.get_value());
                            set_said.set(None);
                            set_problem.set(None);
                        }
                    >
                        "UNDO"
                    </button>
                </div>

                {move || problem.get().map(|text| view! {
                    <div class="alert is-bad" style="margin-bottom:10px">{text}</div>
                })}
                {move || said.get().map(|text| view! {
                    <div class="alert is-ok" style="margin-bottom:10px">{text}</div>
                })}

                <div class="wl-trades__scroll">
                    <table class="table wl-trades__table">
                        <thead>
                            <tr>
                                <th>"Rank"</th>
                                <th>"Tier"</th>
                                <th>"Name"</th>
                                {LANGUAGES.into_iter().map(|(_, name)| view! {
                                    <th>{format!("{name} words")}</th>
                                }).collect_view()}
                                <th title="How often this trade turns up with no website, 1 to 5">
                                    "Odds"
                                </th>
                                <th title="What they can afford to pay, 1 to 5">"Pays"</th>
                                <th>"Note"</th>
                                <th></th>
                            </tr>
                        </thead>
                        <tbody>
                            {move || {
                                let typed = narrow.get();
                                rows.get().into_iter().enumerate()
                                    .filter(|(_, row)| shows(row, &typed))
                                    .map(|(at, row)| view! {
                                        <TradeRow
                                            row=row at=at
                                            on_edit=edit on_remove=remove
                                        />
                                    })
                                    .collect_view()
                            }}
                        </tbody>
                    </table>
                </div>
            })}
        </div>
    }
}

/// One editable row.
#[component]
fn TradeRow<E, R>(row: Target, at: usize, on_edit: E, on_remove: R) -> impl IntoView
where
    E: Fn(usize, Change) + Copy + 'static,
    R: Fn(usize) + Copy + 'static,
{
    view! {
        <tr class="wl-trades__row">
            <td data-label="Rank">
                <input
                    class="field wl-trades__num" type="number" min="1" max="255"
                    prop:value=row.rank.to_string()
                    on:change=move |ev| on_edit(at, Change::Rank(event_target_value(&ev)))
                />
            </td>
            <td data-label="Tier">
                <select
                    class="select wl-trades__tier"
                    on:change=move |ev| on_edit(at, Change::Tier(event_target_value(&ev)))
                >
                    {[Tier::S, Tier::A, Tier::B, Tier::C].into_iter().map(|tier| {
                        let word = format!("{tier:?}");
                        let chosen = tier == row.tier;
                        view! { <option value=word.clone() selected=chosen>{word}</option> }
                    }).collect_view()}
                </select>
            </td>
            <td data-label="Name">
                <input
                    class="field wl-trades__name" type="text"
                    title="The name every business on the list records its trade by. \
                           Changing it does not change theirs."
                    prop:value=row.english.clone()
                    on:change=move |ev| on_edit(at, Change::Name(event_target_value(&ev)))
                />
            </td>
            {LANGUAGES.into_iter().map(|(code, name)| {
                let said = row.words(code).join(";");
                view! {
                    <td data-label=format!("{name} words")>
                        <input
                            class="field wl-trades__words" type="text"
                            placeholder="not searched"
                            title="Several words to a trade, separated by semicolons. \
                                   Empty means this trade is not searched in this \
                                   language at all."
                            prop:value=said
                            on:change=move |ev| {
                                on_edit(at, Change::Words(code, event_target_value(&ev)));
                            }
                        />
                    </td>
                }
            }).collect_view()}
            <td data-label="Odds">
                <input
                    class="field wl-trades__num" type="number" min="1" max="5"
                    prop:value=row.no_site_odds.to_string()
                    on:change=move |ev| on_edit(at, Change::Odds(event_target_value(&ev)))
                />
            </td>
            <td data-label="Pays">
                <input
                    class="field wl-trades__num" type="number" min="1" max="5"
                    prop:value=row.budget.to_string()
                    on:change=move |ev| on_edit(at, Change::Budget(event_target_value(&ev)))
                />
            </td>
            <td data-label="Note">
                <input
                    class="field wl-trades__note" type="text"
                    prop:value=row.note.clone()
                    on:change=move |ev| on_edit(at, Change::Note(event_target_value(&ev)))
                />
            </td>
            <td>
                <button
                    class="btn btn--danger btn--quiet wl-trades__out"
                    title="Take this trade off the list"
                    on:click=move |_| on_remove(at)
                >
                    "✕"
                </button>
            </td>
        </tr>
    }
}

/// One field of one trade, changed.
///
/// A type rather than eight closures, so what can be edited is written down in one
/// place and [`apply`] can be tested without a screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Rank(String),
    Tier(String),
    Name(String),
    /// A language, and its words with semicolons between them.
    Words(&'static str, String),
    Odds(String),
    Budget(String),
    Note(String),
}

/// Put one change into one trade.
///
/// Pure, and tested. A number that is not a number, or a tier letter we do not know,
/// **leaves the field as it was** — the alternative is a field that empties itself
/// while you are halfway through typing in it.
pub fn apply(row: &mut Target, change: Change) {
    match change {
        Change::Rank(said) => {
            if let Ok(rank) = said.trim().parse() {
                row.rank = rank;
            }
        }
        Change::Tier(said) => {
            if let Some(tier) = Tier::parse(&said) {
                row.tier = tier;
            }
        }
        Change::Name(said) => row.english = said.trim().to_owned(),
        Change::Words(lang, said) => {
            let words: Vec<String> = said
                .split(';')
                .map(|word| word.trim().to_owned())
                .filter(|word| !word.is_empty())
                .collect();
            // An emptied cell means "not searched here", which is the absence of the
            // language rather than an empty list under it.
            if words.is_empty() {
                row.words.remove(lang);
            } else {
                row.words.insert(lang.to_owned(), words);
            }
        }
        Change::Odds(said) => {
            if let Ok(odds) = said.trim().parse::<u8>() {
                row.no_site_odds = odds.clamp(1, 5);
            }
        }
        Change::Budget(said) => {
            if let Ok(budget) = said.trim().parse::<u8>() {
                row.budget = budget.clamp(1, 5);
            }
        }
        Change::Note(said) => row.note = said.trim().to_owned(),
    }
}

/// The name a new trade would be added under, or why it cannot be.
///
/// Empty is refused, and so is a name already on the list — two rows of one trade
/// would split its leads between them. Pure, so the rule is a test.
pub fn new_trade_name(typed: &str, existing: &[Target]) -> Result<String, String> {
    let name = typed.trim();
    if name.is_empty() {
        return Err("type the trade's name first".to_owned());
    }
    if existing
        .iter()
        .any(|t| t.english.trim().eq_ignore_ascii_case(name))
    {
        return Err(format!("{name} is already on the list"));
    }
    Ok(name.to_owned())
}

/// A new trade under its name, ready for its words to be filled in.
///
/// Middle of the road on both scores rather than nought: nought is not a number the
/// list allows, and a new row that the server refuses the moment you press save is a
/// row that reads as broken.
pub fn blank_trade(rank: u8, name: String) -> Target {
    Target {
        rank,
        tier: Tier::C,
        english: name,
        words: std::collections::BTreeMap::new(),
        no_site_odds: 3,
        budget: 3,
        note: String::new(),
    }
}

/// Does this trade match what you typed in the narrowing box?
///
/// Every field, because you may remember a trade by its Hungarian word, its English
/// one, or the note you left on it. Pure, and tested.
pub fn shows(row: &Target, typed: &str) -> bool {
    let typed = typed.trim().to_lowercase();
    if typed.is_empty() {
        return true;
    }
    row.english.to_lowercase().contains(&typed)
        || row.note.to_lowercase().contains(&typed)
        || row
            .words
            .values()
            .flatten()
            .any(|word| word.to_lowercase().contains(&typed))
}

/// How many trades, and how many searches each language comes to.
///
/// The number that actually matters: a hundred trades is 137 Hungarian searches and
/// 186 English ones, and it is the searches that cost the afternoon. Pure, and
/// tested.
pub fn counted(rows: &[Target]) -> String {
    let mut said = vec![format!("{} trades", rows.len())];
    for (code, name) in LANGUAGES {
        let searches: usize = rows.iter().map(|t| t.words(code).len()).sum();
        said.push(format!("{searches} {name}"));
    }
    said.join(" · ")
}

/// What a save says afterwards.
pub fn saved_said(count: usize) -> String {
    format!("{count} trades written to the file")
}

#[cfg(test)]
#[path = "trades_tests.rs"]
mod tests;
