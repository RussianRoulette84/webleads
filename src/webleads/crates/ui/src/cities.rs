//! The city row: the widest narrowing, and the first one you reach.
//!
//! # What this file is here to teach
//!
//! Three narrowings in a row, each one narrowing what the one above it left: a city,
//! then one of its searches, then a state. They have to clear each other or they read
//! as "and" — pick Dubai and then a Budapest search and the screen goes empty with
//! nothing to say why. So picking a city here lets go of the search and the area
//! below it, exactly as those two already let go of one another.

use leptos::*;

use crate::api;
use crate::searches::chip_class;

/// The row of cities, above the searches.
#[component]
pub fn CityRow(
    /// The cities with anything on the list, as the last answer reported them:
    /// which one, what it is called, and how many.
    cities: Memo<Vec<(i64, String, i64)>>,
    asking: ReadSignal<api::Asking>,
    set_asking: WriteSignal<api::Asking>,
) -> impl IntoView {
    let show = move |id: Option<i64>| {
        set_asking.update(|a| {
            a.city = id;
            // The two narrowings below this one belong to whichever city you were
            // looking at. Kept, they would read as "and" and show you nothing.
            a.from_run = None;
            a.from_area = None;
        });
    };

    view! {
        // Nothing at all until there is more than one city. One city and a row that
        // says "all · Budapest" is a control with no choice in it, taking up the top
        // of the screen to tell you what you already know.
        {move || (cities.get().len() > 1).then(|| view! {
            <div class="field__label wl-filter__label">"City"</div>
            <div class="wl-strip">
                <button
                    class=move || chip_class(asking.get().city.is_none())
                    title="Every city"
                    on:click=move |_| show(None)
                >
                    "all"
                </button>
                {move || cities.get().into_iter().map(|(id, name, count)| {
                    let chosen = move || asking.get().city == Some(id);
                    view! {
                        <button
                            class=move || chip_class(chosen())
                            title=format!("Only the leads found in {name}")
                            on:click=move |_| show(if chosen() { None } else { Some(id) })
                        >
                            {chip_words(&name, count)}
                        </button>
                    }
                }).collect_view()}
            </div>
        })}
    }
}

/// What a city's chip says.
///
/// Pure, so the wording is a test rather than something read off a screen.
pub fn chip_words(name: &str, count: i64) -> String {
    format!("{name} {count}")
}

/// Which searches belong to the city being looked at.
///
/// The strip below the cities has to shorten with them, or picking Dubai leaves a row
/// of Budapest searches under it that all narrow to nothing.
///
/// A run with no city recorded is shown whatever city you pick: it predates cities,
/// and hiding it would be losing a search rather than narrowing to one.
///
/// Pure, and tested, because "the strip shortens" is otherwise something you check by
/// counting chips on a screen.
pub fn searches_in_city(runs: &[api::Run], city: Option<i64>) -> Vec<api::Run> {
    let Some(city) = city else {
        return runs.to_vec();
    };
    runs.iter()
        .filter(|run| run.city.is_none() || run.city == Some(city))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "cities_tests.rs"]
mod tests;
