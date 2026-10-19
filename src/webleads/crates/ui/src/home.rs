//! Where the list is measured from, and the way to move it.
//!
//! Nearest-first is only worth anything measured from where you actually start the
//! day, and a ring-back only means something on the clock you call by. Both come with
//! the program set to Budapest; these are where you put your own — on the page, so
//! neither has to be written into a file.

use leptos::leptos_dom::helpers::TimeoutHandle;
use leptos::*;

use crate::api::{self, CityCentre, Place};
use crate::areas::{where_said, PAUSE};

/// Your starting point: where the list is measured from, nearest first.
///
/// Always there and always editable. It belongs to the city of the place being
/// searched, starts at that city's centre, and moves to wherever you type.
#[component]
pub fn StartingPoint(
    /// The city of the place being searched.
    city: Signal<Option<i64>>,
    /// Bumped after a move, so the list and the map are read again.
    on_moved: WriteSignal<u32>,
) -> impl IntoView {
    let (reload, set_reload) = create_signal(0_u32);
    // Asked again when the city changes too: a city added after the page opened is
    // not in the first answer, and the box would have nothing to show for it.
    let cities = create_local_resource(
        move || (reload.get(), city.get()),
        |_| async move { api::cities().await },
    );
    let shown = create_memo(move |_| {
        let id = city.get()?;
        cities.get()?.ok()?.into_iter().find(|c| c.id == id)
    });

    // What you are typing, while you type. `None` shows the starting point itself.
    let (typed, set_typed) = create_signal(None::<String>);
    let (found, set_found) = create_signal(Vec::<Place>::new());
    let (said, set_said) = create_signal(None::<String>);
    let waiting: StoredValue<Option<TimeoutHandle>> = store_value(None);

    let close = move || {
        set_typed.set(None);
        set_found.set(Vec::new());
    };

    let look = move || {
        let address = typed.get_untracked().unwrap_or_default();
        spawn_local(async move {
            match api::look_up(&address).await {
                Ok(places) if places.is_empty() => {
                    set_said.set(Some("nothing found at that address".to_owned()));
                    set_found.set(Vec::new());
                }
                Ok(places) => {
                    set_said.set(None);
                    set_found.set(places);
                }
                Err(reason) => set_said.set(Some(reason)),
            }
        });
    };

    let pick = move |place: Place| {
        let Some(id) = city.get_untracked() else {
            return;
        };
        spawn_local(async move {
            // Kept under a name you can read, not the whole postal address the
            // lookup answers with.
            let named = Place {
                label: short_centre(&place),
                ..place.clone()
            };
            match api::move_centre(id, &named).await {
                Ok(done) => {
                    crate::log::step(
                        "home",
                        &format!(
                            "measuring from {} — {} distances changed",
                            named.label, done.moved
                        ),
                    );
                    close();
                    set_said.set(None);
                    set_reload.update(|n| *n += 1);
                    on_moved.update(|n| *n += 1);
                }
                Err(reason) => set_said.set(Some(reason)),
            }
        });
    };

    view! {
        <div class="field__label wl-clock__label">"Your starting point"</div>
        <input
            class="field wl-clock__box"
            type="text"
            placeholder="Type your street"
            disabled=move || shown.get().is_none()
            prop:value=move || typed.get().unwrap_or_else(|| starting_point(shown.get().as_ref()))
            on:focus=move |_| set_typed.set(Some(String::new()))
            on:blur=move |_| {
                // Late enough for a click on a suggestion to land first.
                let _ = set_timeout_with_handle(close, std::time::Duration::from_millis(250));
            }
            on:input=move |ev| {
                let text = event_target_value(&ev);
                set_typed.set(Some(text.clone()));
                if let Some(pending) = waiting.get_value() {
                    pending.clear();
                }
                if text.trim().len() < 3 {
                    set_found.set(Vec::new());
                    return;
                }
                waiting.set_value(
                    set_timeout_with_handle(look, std::time::Duration::from_millis(PAUSE)).ok(),
                );
            }
            on:keydown=move |ev| if ev.key() == "Escape" { close() }
        />
        {move || (!found.get().is_empty()).then(|| view! {
            <div class="listbox__panel wl-find__list">
                {found.get().into_iter().map(|place| {
                    let keeping = place.clone();
                    let town = place.city.clone().unwrap_or_default();
                    let country = place.country.clone().unwrap_or_default();
                    view! {
                        <div
                            class="listbox__option wl-find__hit"
                            title="Measure the list from here"
                            on:mousedown=move |_| pick(keeping.clone())
                        >
                            <span class="wl-find__name">{place.label.clone()}</span>
                            <span class="is-dim wl-find__where">{where_said(&town, &country)}</span>
                        </div>
                    }
                }).collect_view()}
            </div>
        })}
        <div class="is-muted" style="font-size:14px;margin-top:3px;max-width:44ch">
            "The list is ordered nearest first from here. Type an address to move it."
        </div>
        {move || said.get().map(|text| view! {
            <div class="is-warn" style="font-size:15px;margin-top:4px">{text}</div>
        })}
    }
}

/// Your clock: the time ring-backs and opening hours are shown in.
///
/// One of the search settings, saved the moment you pick a zone the clock knows.
#[component]
pub fn YourClock(
    /// Bumped after a change, so the list's times are read again.
    on_changed: WriteSignal<u32>,
) -> impl IntoView {
    let (reload, set_reload) = create_signal(0_u32);
    let zones = create_local_resource(
        move || reload.get(),
        |_| async move { api::your_zone().await },
    );
    let zone_now = move || zones.get().and_then(Result::ok).map(|z| z.zone);
    let (said, set_said) = create_signal(None::<String>);

    let pick_zone = move |typed: String| {
        let known = zones
            .get()
            .and_then(Result::ok)
            .is_some_and(|z| z.all.iter().any(|name| *name == typed.trim()));
        if !known {
            set_said.set(Some(format!(
                "{} is not a clock anybody keeps",
                typed.trim()
            )));
            return;
        }
        spawn_local(async move {
            match api::set_your_zone(&typed).await {
                Ok(zone) => {
                    crate::log::step("home", &format!("your clock is {zone}"));
                    set_said.set(None);
                    set_reload.update(|n| *n += 1);
                    on_changed.update(|n| *n += 1);
                }
                Err(reason) => set_said.set(Some(reason)),
            }
        });
    };

    view! {
        <div class="field__label wl-clock__label">"Your clock"</div>
        <input
            class="field wl-clock__box"
            type="text"
            list="wl-zones"
            placeholder="Europe/London"
            prop:value=move || zone_now().unwrap_or_default()
            on:change=move |ev| pick_zone(event_target_value(&ev))
        />
        <datalist id="wl-zones">
            {move || zones.get().and_then(Result::ok).map(|z| {
                z.all.into_iter().map(|name| view! { <option value=name/> }).collect_view()
            })}
        </datalist>
        <div class="is-muted" style="font-size:14px;margin-top:3px;max-width:44ch">
            "Ring-backs and opening hours are shown in this time. Type a city to find yours."
        </div>
        {move || said.get().map(|text| view! {
            <div class="is-warn" style="font-size:15px;margin-top:4px">{text}</div>
        })}
    }
}

/// The starting point, as the box shows it: its name, or the city's centre when
/// nobody has named one. Empty when there is no city to measure from. Pure.
pub fn starting_point(city: Option<&CityCentre>) -> String {
    match city {
        None => String::new(),
        Some(CityCentre {
            centre_label: Some(label),
            ..
        }) if !label.trim().is_empty() => label.trim().to_owned(),
        Some(city) => format!("the centre of {}", city.name),
    }
}

/// A looked-up place, named the way you would say it: "Andrássy út 12, Budapest".
///
/// The lookup answers with the whole postal address, house number first. The first
/// part is the place; a bare number goes after the street it belongs to; the town is
/// added when the name does not already carry it. Pure.
pub fn short_centre(place: &Place) -> String {
    let parts: Vec<&str> = place
        .label
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let is_number = |p: &str| p.chars().next().is_some_and(|c| c.is_ascii_digit());
    let name = match parts.as_slice() {
        [] => return place.label.trim().to_owned(),
        [number, street, ..] if is_number(number) => format!("{street} {number}"),
        [first, ..] => (*first).to_owned(),
    };
    match place.city.as_deref().map(str::trim) {
        Some(town) if !town.is_empty() && !name.contains(town) => format!("{name}, {town}"),
        _ => name,
    }
}

#[cfg(test)]
#[path = "home_tests.rs"]
mod tests;
