//! Every lead on a map.
//!
//! # What this file is here to teach
//!
//! Talking to a JavaScript library from Rust, and keeping the map a **view** of the
//! filter rather than a second source of truth. The map never decides what is shown —
//! it is told, and redraws. Two things that each think they own the list is how they
//! start disagreeing.

use leptos::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;
use webleads_domain::Verdict;

/// Leaflet, reached through the browser rather than through a Rust binding.
///
/// `#[wasm_bindgen]` here says "these functions exist in JavaScript, call them" — the
/// small glue file in `index.html` does the actual work, because a full typed binding
/// for one map would be far more code than the map itself.
#[wasm_bindgen]
extern "C" {
    /// Build the map once, in the element with this id.
    #[wasm_bindgen(js_name = webleadsMapInit)]
    fn map_init(element_id: &str, lat: f64, lon: f64);

    /// Replace every pin. Takes the whole set rather than adding one at a time,
    /// because the map is a view of the filter and the filter changes all at once.
    #[wasm_bindgen(js_name = webleadsMapPins)]
    fn map_pins(pins_json: &str, start_again: bool);

    /// Mark the spot every distance is measured from.
    #[wasm_bindgen(js_name = webleadsMapHome)]
    fn map_home(lat: f64, lon: f64, label: &str);

    /// Scroll to a business's row and light it for two seconds.
    #[wasm_bindgen(js_name = webleadsShowRow)]
    fn show_row_js(business_id: &str);
}

/// Point at one row: scroll to it and light it up.
///
/// The map's own animation, borrowed. The coming-up panel wants exactly what a pin's
/// "jump to result" does, and a second copy of it would look slightly different.
pub fn show_row(business_id: &str) {
    show_row_js(business_id);
}

#[component]
pub fn Map(
    /// Every match, from the server — not the rows that happen to be loaded.
    pins: Memo<Vec<crate::api::Pin>>,
    /// Which city the pins on offer belong to. The map frames itself afresh when this
    /// changes: the view you left behind belongs to another country.
    city: Memo<Option<i64>>,
    /// The middle of the search area, for the first view.
    centre: (f64, f64),
    /// Where the list is measured from, and what it is called — marked with our icon.
    home: Signal<Option<(f64, f64, String)>>,
) -> impl IntoView {
    // Built once, after the element exists. Building it before would find nothing to
    // build into.
    create_effect(move |built: Option<bool>| {
        if built == Some(true) {
            return true;
        }
        map_init("webleads-map", centre.0, centre.1);
        true
    });

    create_effect(move |_| {
        if let Some((lat, lon, label)) = home.get() {
            map_home(lat, lon, &label);
        }
    });

    // Redrawn whenever the filter changes. The map holds no opinion of its own.
    // Framed once per city. Afterwards the view is yours — panning and zooming are
    // never undone — but a set of pins from another country gets a frame of its own,
    // or you are left looking at an empty sea.
    let framed_for = store_value(None::<Option<i64>>);
    create_effect(move |_| {
        let showing = pins.get();
        let belongs_to = city.get();
        let moved = !showing.is_empty() && framed_for.get_value() != Some(belongs_to);
        if moved {
            framed_for.set_value(Some(belongs_to));
        }
        let painted: Vec<Painted<'_>> = showing
            .iter()
            .map(|pin| Painted {
                pin,
                tone: tone_for(&pin.why),
            })
            .collect();
        if let Ok(json) = serde_json::to_string(&painted) {
            map_pins(&json, moved);
        }
    });

    view! {
        <div class="wl-mapbox" style="margin-bottom:18px">
            <div class="wl-mapbox__in">
                <div id="webleads-map" style="height:420px;width:100%"></div>
            </div>
        </div>
        <p class="is-dim" style="font-size:15px;margin:-12px 0 16px">
            {move || {
                let n = pins.get().len();
                format!(
                    "{n} on the map. Close together they group up — hold ⌘ and scroll to \
                     zoom where the pointer is."
                )
            }}
        </p>
    }
}

/// A pin plus the colour it wears.
///
/// The colour is decided here, in Rust, where it can be tested — the drawing side
/// only turns the name into a hex value. Two places each deciding which colour means
/// what is how they start disagreeing.
#[derive(Serialize)]
struct Painted<'a> {
    #[serde(flatten)]
    pin: &'a crate::api::Pin,
    tone: &'static str,
}

/// The colour name for a verdict as the server spells it.
///
/// Anything we cannot read is drawn in the quiet colour rather than dropped — a pin
/// with no colour is still a business with a telephone number.
pub fn tone_for(why: &str) -> &'static str {
    why.parse::<Verdict>().map(colour_for).unwrap_or("dim")
}

/// Which colour a pin wears.
///
/// The choice itself lives in `domain`, because the live map and the standalone page
/// both draw these pins and they have to agree about which reason is which.
pub fn colour_for(verdict: Verdict) -> &'static str {
    verdict.tone()
}

#[cfg(test)]
#[path = "map_tests.rs"]
mod tests;
