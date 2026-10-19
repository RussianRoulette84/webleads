//! The call list as a table: one row per business.

use leptos::*;

use webleads_domain::Sort;

use crate::api::{Asking, LeadRow};
use crate::contact::NoteCell;
use crate::status::StatusPicker;

#[component]
pub fn LeadTable(
    rows: Vec<LeadRow>,
    /// The hundred trades, so a row can print its own in Hungarian. Empty until the
    /// list has arrived, which only means a row reads in English for a moment.
    targets: Vec<webleads_domain::Target>,
    /// The pictures we already hold, by business id.
    shots: std::collections::HashMap<String, crate::api::Shot>,
    asking: ReadSignal<Asking>,
    set_asking: WriteSignal<Asking>,
    /// Bumped when a row hides itself, so the list reloads without it.
    on_change: WriteSignal<u32>,
    /// Bumped when a state or a ring-back time is saved, so the coming-up panel
    /// catches up. Deliberately not `on_change`: reloading the whole list every time
    /// you mark a business would throw away where you had got to in it.
    on_contact: WriteSignal<u32>,
) -> impl IntoView {
    view! {
        // No sideways scrolling: the columns share the window and the long ones wrap.
        <table class="table u-wide wl-table">
            <thead>
                <tr>
                    {SORTABLE.into_iter().map(|(column, align)| view! {
                        <Heading column=column align=align asking=asking set_asking=set_asking/>
                    }).collect_view()}
                    // Not sortable, and nor should they be: sorting a list by the
                    // first letter of your own notes is nobody's morning.
                    <th>"Notes"</th>
                    <th></th>
                </tr>
            </thead>
            <tbody>
                {rows.into_iter()
                    .map(|row| {
                        let shot = shots.get(&row.lead.id).cloned();
                        // Worked out here rather than in the row: the lookup needs
                        // the whole list, and a row has no business holding it.
                        let trade = trade_name(&targets, &row.lead, row.city_lang.as_deref());
                        view! {
                            <LeadLine
                                row=row shot=shot trade=trade
                                on_change=on_change on_contact=on_contact
                            />
                        }
                    })
                    .collect_view()}
            </tbody>
        </table>
    }
}

/// The columns, in the order they appear, and which way their contents sit.
const SORTABLE: [(Sort, &str); 9] = [
    (Sort::Name, "left"),
    (Sort::Trade, "left"),
    (Sort::Phone, "left"),
    // No certificate column: what it would say is already the second line of "Why".
    (Sort::Why, "left"),
    (Sort::Hours, "left"),
    (Sort::Reviews, "right"),
    (Sort::Distance, "right"),
    (Sort::FirstSeen, "left"),
    // By how much work is left, so what you have not touched comes first.
    (Sort::Status, "left"),
];

/// One heading you can click to sort by.
#[component]
fn Heading(
    column: Sort,
    align: &'static str,
    asking: ReadSignal<Asking>,
    set_asking: WriteSignal<Asking>,
) -> impl IntoView {
    let chosen = move || asking.get().sort == column;

    let click = move |_| {
        set_asking.update(|a| {
            if a.sort == column {
                // Same column again — turn it round.
                a.descending = !a.descending;
            } else {
                // A new column opens the way that column reads best: busiest first
                // for a number, A to Z for a word.
                a.sort = column;
                a.descending = column.biggest_first();
            }
        });
    };

    view! {
        <th style=format!("text-align:{align}")>
            <button
                class="btn btn--quiet"
                style=format!(
                    "background:none;border:0;padding:0;font:inherit;letter-spacing:inherit;\
                     cursor:pointer;text-align:{align}"
                )
                on:click=click
            >
                {column.heading()}
                // A small mark, only on the column actually in use — an arrow on
                // every heading is nine arrows nobody reads.
                {move || chosen().then(|| view! {
                    <span class="is-accent" style="margin-left:4px">
                        {if asking.get().descending { "↓" } else { "↑" }}
                    </span>
                })}
            </button>
        </th>
    }
}

#[component]
fn LeadLine(
    row: LeadRow,
    /// This business's trade, in Hungarian, already looked up.
    trade: String,
    /// The picture of this business's website, if we have taken one.
    shot: Option<crate::api::Shot>,
    on_change: WriteSignal<u32>,
    on_contact: WriteSignal<u32>,
) -> impl IntoView {
    let lead = row.lead;
    // Where this business has got to. A signal rather than the value it arrived
    // with, so marking it repaints the row at once.
    let state = create_rw_signal(row.contact.status);
    let today_line = row.hours_today.clone();
    let pitch = lead.pitch();
    // `tel:` needs the number without the spaces a human reads it with. The cleaning
    // lives in `domain` so it can be tested — see `Lead::dial_number`.
    let dial = lead.dial_number();

    view! {
        // The map looks rows up by this. The state colours the row's left edge,
        // which takes no space from the name.
        <tr
            data-lead=lead.id.clone()
            class=move || format!("wl-row wl-{}", state.get().as_str())
        >
            <td class="wl-cell wl-cell--name">
                <div class="wl-name">{lead.name.clone()}</div>
                // The address, and nothing else: the trade and why they qualified are
                // the next two columns along.
                {lead.address.clone().map(|a| view! {
                    <div class="is-dim wl-address">{a}</div>
                })}
            </td>
            <td class="is-dim wl-cell" data-label="Trade">{trade}</td>
            <td class="u-keep wl-cell" data-label="Phone">
                {match (lead.phone.clone(), dial) {
                    (Some(shown), Some(number)) => view! {
                        <a class="link" href=format!("tel:{number}")>{shown}</a>
                    }.into_view(),
                    _ => view! { <span class="is-dim">"no number"</span> }.into_view(),
                }}
            </td>
            <td class="wl-cell" data-label="Why">
                // The tag and the way out to their site on one line, so a narrow
                // column wraps the words rather than dropping the arrow below them.
                <span class="wl-why">
                    <span class=verdict_class(lead.verdict)>{lead.verdict.describe()}</span>
                    // Straight to their site in a new tab. Only where there is one to
                    // open: a business with no website is the whole point of the list.
                    {lead.website.clone().filter(|w| !w.trim().is_empty()).map(|url| view! {
                        <a
                            class="link wl-out"
                            href=url.clone()
                            target="_blank"
                            rel="noreferrer"
                            title=format!("Open {url} in a new tab")
                        >
                            "↗"
                        </a>
                    })}
                </span>
                // The reason, unless it only repeats the tag above it.
                {worth_saying(lead.verdict, &lead.reason).then(|| view! {
                    <div class="is-dim" style="font-size:16px">{lead.reason.clone()}</div>
                })}
                {lead.social_url.clone().map(|url| view! {
                    <a class="link" style="font-size:16px" href=url.clone() target="_blank" rel="noreferrer">
                        {url}
                    </a>
                })}
            </td>
            <td class="is-dim u-keep wl-cell" data-label="Hours" style="font-size:16px">
                <HoursCell
                    today=today_line.clone()
                    there=row.hours_there.clone()
                    week=lead.hours.week_named()
                />
            </td>
            <td class="u-keep wl-cell wl-cell--figure" data-label="Reviews">
                {if lead.reviews == 0 {
                    view! { <span class="tag is-accent">"none"</span> }.into_view()
                } else {
                    view! { <span>{lead.reviews}</span> }.into_view()
                }}
            </td>
            <td class="u-keep wl-cell wl-cell--figure" data-label="Distance">{describe_distance(lead.distance_m)}</td>
            <td class="is-dim u-keep wl-cell" data-label="First seen" style="font-size:16px">
                {row.first_seen.clone()}
                {(row.call_count > 0).then(|| view! {
                    <div class="is-accent" style="font-size:15px">
                        {if row.call_count == 1 {
                            "called once".to_owned()
                        } else {
                            format!("called {} times", row.call_count)
                        }}
                    </div>
                })}
                // When the two dates differ, the second one says "and it was still
                // there on this run" — worth knowing before you telephone.
                {(row.last_seen != row.first_seen).then(|| view! {
                    <div style="font-size:15px">"still there "{row.last_seen.clone()}</div>
                })}
            </td>
            <td class="wl-statecell wl-cell" data-label="State">
                <StatusPicker
                    id=lead.id.clone()
                    contact=row.contact.clone()
                    shows=state
                    on_saved=on_contact
                />
            </td>
            // Your own words, in a column of their own rather than under the state.
            <td class="wl-notecell wl-cell" data-label="Notes">
                <NoteCell id=lead.id.clone() note=row.contact.note.clone()/>
            </td>
            <td class="wl-doings wl-cell">
                // A class rather than an inline style, so the buttons can wrap inside
                // their own column instead of pushing the table sideways.
                <div class="wl-doings__row">
                    {lead.maps_url.clone().map(|url| view! {
                        <a
                            class="btn btn--quiet wl-icon"
                            href=url target="_blank" rel="noreferrer"
                            title="Open this business on Google Maps"
                        >
                            "⌖"
                        </a>
                    })}
                    <ShotButton
                        id=lead.id.clone()
                        website=lead.website.clone()
                        shot=shot
                    />
                    <CopyRow text=row_as_text(
                        &lead, today_line.as_deref(), &pitch, &row.contact.note
                    )/>
                    <MoreDoings id=lead.id.clone() on_change=on_change/>
                </div>
            </td>
        </tr>
    }
}

/// What a row's trade column should say.
///
/// In the language of **this business's own city**, because that is the word the
/// business itself would use — so a Dubai row reads "barber shop" and a Budapest one
/// "fodrászat", on the same screen. Google's own category is already local, and a
/// word you typed yourself is in whatever language you typed it, so the English name
/// from our own list is the only one that needs looking up.
///
/// Pure, so "each row reads in its own city's language" is a test rather than
/// something you check by scrolling.
pub fn trade_name(
    targets: &[webleads_domain::Target],
    lead: &webleads_domain::Lead,
    lang: Option<&str>,
) -> String {
    match lead.shown_trade() {
        None => "—".to_owned(),
        Some(shown) => match lang {
            // No city, no language of its own: the name as it is stored, which is
            // better than picking somebody else's language for it.
            None => shown.to_owned(),
            Some(lang) => {
                webleads_domain::name_for(targets, shown, lang).unwrap_or_else(|| shown.to_owned())
            }
        },
    }
}

/// Is the reason worth a second line, or does it only repeat the tag?
///
/// "No website" is always followed by "no link on Google Maps", which says the same
/// thing twice and costs a line on every row of the list. Pure, so which verdicts
/// say something new is a test rather than an opinion in the middle of a view.
pub fn worth_saying(verdict: webleads_domain::Verdict, reason: &str) -> bool {
    use webleads_domain::Verdict;
    if reason.trim().is_empty() {
        return false;
    }
    // The other reasons carry the detail the tag cannot: which code the server
    // answered, which social site it was, what is wrong with the certificate.
    verdict != Verdict::NoSite
}

/// Which state class the tag wears. Four shades, not a Christmas tree.
fn verdict_class(verdict: webleads_domain::Verdict) -> &'static str {
    use webleads_domain::Verdict;
    match verdict {
        Verdict::NoSite => "tag is-accent",
        Verdict::SocialOnly => "tag is-warn",
        Verdict::InsecureSite => "tag is-warn",
        Verdict::DeadSite => "tag",
        Verdict::LiveSite => "tag is-dim",
    }
}

/// Today's hours, with the rest of the week a press away.
///
/// The whole week on every row is a very tall table; "closed today, opens Monday at
/// eight" is still what you need on a Saturday, so it is here rather than nowhere.
#[component]
fn HoursCell(
    today: Option<String>,
    /// What the shop itself says, when it is in another timezone. Absent for a
    /// business in your own city.
    there: Option<String>,
    week: Vec<(&'static str, String)>,
) -> impl IntoView {
    view! {
        // Blank for anything scraped in fast mode, which sends no hours at all. A
        // blank beats a guess.
        //
        // **On your clock.** Google writes them in the shop's own time, which is the
        // one you cannot dial by — so a Dubai shop's "10 am–11 pm" reads here as
        // "08:00–21:00", and the whole list is one time.
        <div>{today.unwrap_or_default()}</div>
        // And what they would say themselves, small, for when you are talking to
        // them. Only when the two differ.
        {there.map(|said| view! {
            <div class="is-dim wl-hours__there">{format!("{said} there")}</div>
        })}
        // The rest of the week under it, a day to a line. As one long line it was
        // unreadable, and it was what made the table too wide for the window.
        {(!week.is_empty()).then(|| view! {
            <div class="is-dim wl-week">
                {week.into_iter().map(|(day, line)| view! {
                    <div>
                        <span class="wl-day">{day}</span>
                        {webleads_domain::plain_hours(&line)}
                    </div>
                }).collect_view()}
            </div>
        })}
    }
}

/// How far away, in words rather than a bare number.
///
/// With the area fence off a business can be anywhere, and `1157967 m` reads as a
/// number you skim past rather than as "this one is in another country". Anything
/// past a few kilometres is said in kilometres, and anything absurd is said loudly.
///
/// Pure, so every boundary can be checked without a browser.
pub fn describe_distance(metres: u32) -> View {
    let text = distance_words(metres);
    if is_suspiciously_far(metres) {
        view! { <span class="is-warn">{text}</span> }.into_view()
    } else {
        view! { <span>{text}</span> }.into_view()
    }
}

/// The wording on its own, so the boundaries can be tested.
pub fn distance_words(metres: u32) -> String {
    if metres < 1_000 {
        format!("{metres} m")
    } else {
        format!("{:.1} km", f64::from(metres) / 1000.0)
    }
}

/// Is this far enough away to be worth a second look?
pub fn is_suspiciously_far(metres: u32) -> bool {
    metres >= 20_000
}

/// A whole row as text you can paste somewhere.
///
/// Pure, so its shape is tested rather than eyeballed — and so a change to it shows
/// up as a failing test rather than as a surprise on your clipboard.
pub fn row_as_text(
    lead: &webleads_domain::Lead,
    today: Option<&str>,
    pitch: &str,
    note: &str,
) -> String {
    let mut lines = vec![lead.name.clone()];

    if let Some(phone) = &lead.phone {
        lines.push(phone.clone());
    }
    if let Some(address) = &lead.address {
        lines.push(address.clone());
    }
    if let Some(today) = today.filter(|t| !t.trim().is_empty()) {
        lines.push(format!("Open today: {today}"));
    }
    lines.push(pitch.to_owned());
    // Your own note, which is the part you actually wrote.
    if !note.trim().is_empty() {
        lines.push(format!("Note: {}", note.trim()));
    }
    if let Some(url) = &lead.maps_url {
        lines.push(url.clone());
    }

    lines.join("\n")
}

/// A picture of the broken website, taken on the spot — and shown here.
///
/// Only on rows that have a website at all — a business with none has nothing to
/// photograph, and that is the whole reason it is a lead.
///
/// Once a picture exists the button becomes the way to look at it: hover for a
/// thumbnail, press for the whole thing. That replaced a second "see it" link that
/// opened the image in a tab of its own — two controls for one picture, and the tab
/// lost the row you were working.
#[component]
fn ShotButton(
    id: String,
    website: Option<String>,
    shot: Option<crate::api::Shot>,
) -> impl IntoView {
    let id = store_value(id);
    let (have, set_have) = create_signal(shot);
    let (working, set_working) = create_signal(false);
    let (problem, set_problem) = create_signal(None::<String>);
    // Hovering shows a thumbnail; pressing opens it properly. Two signals rather
    // than one, because the big one stays up when the pointer leaves.
    let (peeking, set_peeking) = create_signal(false);
    let (full, set_full) = create_signal(false);

    let take = move || {
        set_working.set(true);
        set_problem.set(None);
        spawn_local(async move {
            crate::log::step("picture", "photographing this website");
            match crate::api::take_shot(&id.get_value()).await {
                Ok(shot) => {
                    crate::log::done("picture", &shot.answered);
                    set_have.set(Some(shot));
                }
                // Said out loud rather than left as an empty square — usually "no
                // browser here" or a site that never answered.
                Err(reason) => set_problem.set(Some(reason)),
            }
            set_working.set(false);
        });
    };

    // One button, two jobs: take the picture, then look at it.
    let pressed = move |_| {
        if have.get().is_some() {
            set_peeking.set(false);
            set_full.set(true);
        } else {
            take();
        }
    };

    // Nothing of theirs to photograph.
    if website.as_deref().unwrap_or("").trim().is_empty() {
        return ().into_view();
    }

    view! {
        <span
            class="wl-shot"
            on:mouseenter=move |_| set_peeking.set(true)
            on:mouseleave=move |_| set_peeking.set(false)
        >
            <button
                class="btn btn--quiet"
                title=move || if have.get().is_some() {
                    "Look at the picture we took"
                } else {
                    "Photograph their website as it is now"
                }
                disabled=move || working.get()
                on:click=pressed
            >
                {move || match (working.get(), have.get().is_some()) {
                    (true, _) => "…".to_owned(),
                    (_, true) => "Picture ✓".to_owned(),
                    _ => "Picture".to_owned(),
                }}
            </button>

            // The thumbnail, while the pointer is on the button.
            {move || (peeking.get() && !full.get())
                .then(|| have.get())
                .flatten()
                .map(|shot| view! {
                    <span class="wl-shot__peek">
                        <img src=shot.url.clone() alt="" loading="lazy"/>
                    </span>
                })}
        </span>

        // The whole picture, over the page. Anywhere outside it closes it, and so
        // does Escape — a picture you cannot get out of is a trap.
        {move || full.get()
            .then(|| have.get())
            .flatten()
            .map(|shot| view! {
                <FullShot url=shot.url.clone() said=shot.answered.clone() close=set_full/>
            })}

        {move || problem.get().map(|reason| view! {
            <div class="is-warn" style="font-size:14px">{reason}</div>
        })}
    }
    .into_view()
}

/// One picture, as big as the window allows.
#[component]
fn FullShot(url: String, said: String, close: WriteSignal<bool>) -> impl IntoView {
    // Escape closes it. Listened for on the window, because the press does not
    // necessarily land inside the overlay.
    let on_key = window_event_listener(ev::keydown, move |ev| {
        if ev.key() == "Escape" {
            close.set(false);
        }
    });
    on_cleanup(move || on_key.remove());

    view! {
        <div class="wl-shot__full" on:click=move |_| close.set(false)>
            // The picture itself swallows the press, so clicking the image does not
            // close what you just opened.
            <img src=url on:click=|ev| ev.stop_propagation() alt=said.clone()/>
            <div class="wl-shot__said">{said}" · press anywhere to close"</div>
        </div>
    }
}

/// The doings you want rarely, behind one mark.
///
/// Hiding a business or a chain is a handful of presses a week; five buttons on nine
/// hundred rows is a column of noise.
#[component]
fn MoreDoings(id: String, on_change: WriteSignal<u32>) -> impl IntoView {
    let (open, set_open) = create_signal(false);
    let id = store_value(id);

    let hide = move |whole_chain: bool| {
        set_open.set(false);
        spawn_local(async move {
            crate::log::step(
                "hidden",
                if whole_chain {
                    "hiding the whole chain"
                } else {
                    "hiding one business"
                },
            );
            let _ = crate::api::hide(&id.get_value(), whole_chain).await;
            on_change.update(|n| *n += 1);
        });
    };

    view! {
        <span style="position:relative">
            <button
                class="btn btn--quiet wl-icon"
                title="More"
                on:click=move |_| set_open.update(|o| *o = !*o)
            >
                "⋯"
            </button>
            {move || open.get().then(|| view! {
                <div class="listbox__panel wl-more">
                    <div class="listbox__option" on:click=move |_| hide(false)>
                        "Hide this business"
                    </div>
                    <div class="listbox__option" on:click=move |_| hide(true)>
                        "Hide the whole chain"
                    </div>
                </div>
            })}
        </span>
    }
}

/// A button that puts one row on the clipboard.
///
/// Says "copied" for a moment afterwards. A button that appears to do nothing is a
/// button you press four times.
#[component]
fn CopyRow(text: String) -> impl IntoView {
    let (said, set_said) = create_signal(false);
    let text = store_value(text);

    let copy = move |_| {
        let Some(window) = web_sys::window() else {
            return;
        };
        let clipboard = window.navigator().clipboard();
        // `127.0.0.1` counts as a secure context, so this works on the page as it is.
        let _ = clipboard.write_text(&text.get_value());
        set_said.set(true);
        let _ = set_timeout_with_handle(
            move || set_said.set(false),
            std::time::Duration::from_millis(1400),
        );
    };

    view! {
        <button class="btn btn--quiet" style="font-size:14px" on:click=copy>
            {move || if said.get() { "copied" } else { "Copy" }}
        </button>
    }
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;
