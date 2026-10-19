//! Where to search: picking an area, and adding one by typing an address.

use leptos::leptos_dom::helpers::TimeoutHandle;
use leptos::*;

use crate::api;

/// How long to wait after the last keystroke before looking an address up.
///
/// Their rules ask for no more than one request a second, and this is also what stops
/// the suggestions flickering while you are still typing.
pub const PAUSE: u64 = 500;

#[component]
pub fn AreaPanel(
    /// Bumped whenever the area changes, so what depends on it is asked again.
    on_change: WriteSignal<u32>,
    /// What the call list is narrowed to, so an area can narrow it to its own leads.
    filter: ReadSignal<crate::api::Asking>,
    set_filter: WriteSignal<crate::api::Asking>,
) -> impl IntoView {
    let (reload, set_reload) = create_signal(0_u32);
    let (problem, set_problem) = create_signal(None::<String>);

    let areas = create_local_resource(move || reload.get(), |_| async move { api::areas().await });

    let chosen = create_memo(move |_| {
        let answer = areas.get().and_then(Result::ok)?;
        let id = answer.chosen?;
        answer.areas.into_iter().find(|a| a.id == id)
    });

    let changed = move || {
        set_reload.update(|n| *n += 1);
        on_change.update(|n| *n += 1);
    };

    let choose = move |id: i64| {
        spawn_local(async move {
            crate::log::step("area", &format!("searching area {id} from now on"));
            match api::choose_area(id).await {
                Ok(_) => changed(),
                Err(reason) => set_problem.set(Some(reason)),
            }
        });
    };

    let forget = move |id: i64| {
        spawn_local(async move {
            match api::remove_area(id).await {
                Ok(_) => changed(),
                Err(reason) => set_problem.set(Some(reason)),
            }
        });
    };

    view! {
        {move || problem.get().map(|reason| view! {
            <div class="alert is-bad" style="margin-bottom:10px">{reason}</div>
        })}

        <div class="field__label">"Where to search"</div>
        // The places you have kept, as chips — the same shape as the filters above,
        // so choosing where to search looks like every other choice on the page.
        <div class="wl-strip">
            {move || areas.get().and_then(Result::ok).map(|answer| {
                let last_one = answer.areas.len() <= 1;
                answer.areas.into_iter().map(|area| {
                    let id = area.id;
                    let mine = Some(id) == answer.chosen;
                    view! {
                        <span class="wl-place">
                            <button
                                class=move || crate::searches::chip_class(mine)
                                title="Search here from now on"
                                on:click=move |_| choose(id)
                            >
                                {area.label.clone()}
                            </button>
                            // Any of them, including the one being searched: the
                            // server moves to whatever is left, and a search already
                            // going carries its own area with it, so it cannot be
                            // pulled out from under one. Only the last place stays —
                            // there has to be somewhere to search.
                            {(!last_one).then(|| view! {
                                <button
                                    class="wl-place__forget"
                                    title="Forget this place. The leads it found stay."
                                    on:click=move |_| forget(id)
                                >
                                    "✕"
                                </button>
                            })}
                        </span>
                    }
                }).collect_view()
            })}
        </div>

        // Where the next search will look, and the way to its own leads.
        {move || chosen.get().map(|area| {
            let label = store_value(area.label.clone());
            let showing = move || filter.get().from_area == Some(label.get_value());
            view! {
                <div class="wl-place__said">
                    "Searching "<span class="is-accent">{area.label.clone()}</span>
                    <button
                        class="btn btn--quiet wl-place__list"
                        on:click=move |_| {
                            let mine = label.get_value();
                            set_filter.update(|a| {
                                a.from_run = None;
                                a.from_area = (a.from_area.as_deref() != Some(mine.as_str()))
                                    .then_some(mine);
                            });
                        }
                    >
                        {move || if showing() { "showing its leads ✕" } else { "show its leads" }}
                    </button>
                </div>
            }
        })}

        // The main thing on this panel: type a place and search it. It sat at the
        // bottom under a quiet label, which is why the whole thing read as fixed.
        <AddressBox on_added=move |_| changed()/>

    }
}

/// Type an address, pick the match, keep it as an area.
#[component]
fn AddressBox<F>(on_added: F) -> impl IntoView
where
    F: Fn(()) + Copy + 'static,
{
    let (typed, set_typed) = create_signal(String::new());
    let (found, set_found) = create_signal(Vec::<api::Place>::new());
    let (looking, set_looking) = create_signal(false);
    let (said, set_said) = create_signal(None::<String>);
    // Which city the new area belongs to. Empty means "whatever the lookup said" —
    // filling it in is how you correct the lookup, not how you answer it.
    let (city, set_city) = create_signal(String::new());

    // Typing looks it up by itself, a moment after you stop. The lookup asks for one
    // request a second and a keystroke is faster than that, so the wait is what keeps
    // us inside their rules as well as what stops the list flickering.
    let waiting: StoredValue<Option<TimeoutHandle>> = store_value(None);

    let look = move |_| {
        let address = typed.get();
        crate::log::step("area", &format!("looking up “{address}”"));
        set_looking.set(true);
        set_said.set(None);
        spawn_local(async move {
            match api::look_up(&address).await {
                Ok(places) if places.is_empty() => {
                    crate::log::warn("area", "nothing found at that address");
                    set_said.set(Some("nothing found at that address".to_owned()));
                    set_found.set(Vec::new());
                }
                Ok(places) => set_found.set(places),
                Err(reason) => set_said.set(Some(reason)),
            }
            set_looking.set(false);
        });
    };

    let keep = move |place: api::Place, label: String| {
        // Yours if you typed one, otherwise whatever the lookup reported. An area in
        // no city is allowed and still searchable; its leads simply sit under no chip.
        let belongs_to = pick_city(&city.get(), place.city.as_deref());
        spawn_local(async move {
            // The name is what goes into the search words — "borbély Zugló", not the
            // whole postal address — so which of them you keep it under matters.
            match api::add_area(&label, &place, belongs_to.as_deref()).await {
                Ok(_) => {
                    set_found.set(Vec::new());
                    set_typed.set(String::new());
                    set_city.set(String::new());
                    on_added(());
                }
                Err(reason) => set_said.set(Some(reason)),
            }
        });
    };

    // What you typed, kept under your own words.
    //
    // A place still needs coordinates — the search is centred on them and the fence
    // is measured from them — so the best match for what you typed supplies those,
    // and nothing else. The name is yours: "Zugló" searches better than "XIV.
    // kerület, Budapest, Közép-Magyarország, Magyarország".
    let keep_as_typed = move || {
        let Some(name) = typed_name(&typed.get()) else {
            return;
        };
        // Already looked up while you were typing: no second request for it.
        if let Some(place) = found.get().first().cloned() {
            keep(place, name);
            return;
        }
        set_looking.set(true);
        set_said.set(None);
        spawn_local(async move {
            match api::look_up(&name).await {
                Ok(places) => match places.first() {
                    Some(place) => keep(place.clone(), name),
                    None => set_said.set(Some(
                        "nothing found there — a place needs somewhere to be".to_owned(),
                    )),
                },
                Err(reason) => set_said.set(Some(reason)),
            }
            set_looking.set(false);
        });
    };

    view! {
        <div class="field__label">"Search another place"</div>
        <div class="wl-find">
            <input
                class="field wl-find__box"
                type="text"
                placeholder="A district, a town, or a street address — anywhere in the world"
                prop:value=move || typed.get()
                on:input=move |ev| {
                    set_typed.set(event_target_value(&ev));
                    if let Some(pending) = waiting.get_value() {
                        pending.clear();
                    }
                    if typed.get().trim().len() < 3 {
                        set_found.set(Vec::new());
                        return;
                    }
                    waiting.set_value(
                        set_timeout_with_handle(
                            move || look(()),
                            std::time::Duration::from_millis(PAUSE),
                        )
                        .ok(),
                    );
                }
                // Enter keeps what you typed, rather than only fetching the list.
                on:keydown=move |ev| if ev.key() == "Enter" { keep_as_typed() }
            />
            <button
                class="btn wl-find__add"
                title="Keep this place under exactly what you typed"
                disabled=move || typed_name(&typed.get()).is_none()
                on:click=move |_| keep_as_typed()
            >
                "ADD"
            </button>
            {move || looking.get().then(|| view! {
                <span class="is-dim wl-find__said">"looking…"</span>
            })}
        </div>

        // The city the new area will belong to. Only worth showing once there is
        // something to look at: before that it is a box asking about a place you have
        // not found yet.
        {move || (!found.get().is_empty()).then(|| view! {
            <div class="wl-find__city">
                <span class="is-dim">"City"</span>
                <input
                    class="field wl-find__cityname"
                    type="text"
                    prop:value=move || city.get()
                    placeholder=move || {
                        found.get().first()
                            .and_then(|p| p.city.clone())
                            .unwrap_or_else(|| "none".to_owned())
                    }
                    title="Which city these leads belong under. Leave it to take what \
                           the lookup found."
                    on:input=move |ev| set_city.set(event_target_value(&ev))
                />
            </div>
        })}

        {move || said.get().map(|text| view! {
            <div class="is-warn" style="font-size:15px;margin-bottom:6px">{text}</div>
        })}

        // What was found, as things you can press. The whole row is the button, so the
        // match you are looking at is the thing you click.
        {move || (!found.get().is_empty()).then(|| view! {
            <div class="listbox__panel wl-find__list">
                {found.get().into_iter().map(|place| {
                    let keeping = place.clone();
                    let names = name_choices(&place.label);
                    let first = names.first().cloned().unwrap_or_default();
                    view! {
                        <div
                            class="listbox__option wl-find__hit"
                            title="Keep this place and search it"
                            on:click=move |_| keep(keeping.clone(), first.clone())
                        >
                            <span class="wl-find__name">
                                {names.first().cloned().unwrap_or_default()}
                            </span>
                            <span class="is-dim wl-find__full">{place.label.clone()}</span>
                            // Which town and country this is, because without the old
                            // Hungary-only fence "Downtown Dubai" also matches a
                            // street in Astana.
                            {place.city.clone().map(|town| {
                                let country = place.country.clone().unwrap_or_default();
                                view! {
                                    <span class="is-dim wl-find__where">
                                        {where_said(&town, &country)}
                                    </span>
                                }
                            })}
                            // And the other way of naming the same place, because
                            // "XIV. kerület" and "Zugló, Budapest" find different
                            // things when they are appended to every search word.
                            {names.get(1).cloned().map(|other| {
                                let also = place.clone();
                                let name = other.clone();
                                view! {
                                    <button
                                        class="wl-tag wl-find__also"
                                        title="Keep it under this name instead"
                                        on:click=move |ev| {
                                            ev.stop_propagation();
                                            keep(also.clone(), name.clone());
                                        }
                                    >
                                        {other}
                                    </button>
                                }
                            })}
                        </div>
                    }
                }).collect_view()}
            </div>
        })}
    }
}

/// Which city a new area belongs to: yours if you typed one, the lookup's otherwise.
///
/// `None` when neither has anything to say. An area in no city is allowed — it is
/// still searchable, and its leads simply sit under no chip — which is better than
/// filing them under a town we guessed.
///
/// Pure, and tested: "leave it blank and it takes what was found" is the whole
/// behaviour of that box.
pub fn pick_city(typed: &str, found: Option<&str>) -> Option<String> {
    let typed = typed.trim();
    if !typed.is_empty() {
        return Some(typed.to_owned());
    }
    found
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

/// Where a match is, in a few words: the town and the country.
///
/// Pure, so the wording is a test. Either half may be missing — the lookup reports
/// what it has — and a stray comma with nothing after it looks like a fault.
pub fn where_said(town: &str, country: &str) -> String {
    let town = town.trim();
    let country = country.trim();
    match (town.is_empty(), country.is_empty()) {
        (true, true) => String::new(),
        (true, false) => country.to_owned(),
        (false, true) => town.to_owned(),
        // The town alone when they are the same word, or a chip would read
        // "Dubaj, Dubaj".
        (false, false) if town.eq_ignore_ascii_case(country) => town.to_owned(),
        (false, false) => format!("{town}, {country}"),
    }
}

/// What you typed, ready to be kept as a name — or nothing, if it is not one.
///
/// Pure, so "a box with two spaces in it is not a place" is a test rather than an
/// area called "  " that you cannot tell from the others.
pub fn typed_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// The ways of naming a place that are worth offering.
///
/// The lookup answers with the whole postal address —
/// `XIV. kerület, Budapest, Közép-Magyarország, Magyarország` — and that is not
/// something to append to a hundred search words. The first part alone is usually
/// right; the first two are right when the first is a district number that means
/// nothing on its own.
///
/// At most two, because a third is a decision rather than a suggestion. Pure.
pub fn name_choices(label: &str) -> Vec<String> {
    let parts: Vec<&str> = label
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let mut names = Vec::new();
    if let Some(first) = parts.first() {
        names.push((*first).to_owned());
    }
    if let (Some(first), Some(second)) = (parts.first(), parts.get(1)) {
        let pair = format!("{first}, {second}");
        if !names.contains(&pair) {
            names.push(pair);
        }
    }
    if names.is_empty() {
        names.push(label.trim().to_owned());
    }
    names
}

#[cfg(test)]
#[path = "areas_tests.rs"]
mod tests;
