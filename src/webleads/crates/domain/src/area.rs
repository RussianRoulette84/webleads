//! Where to search, how much of it to take, and optionally how far out.

use serde::{Deserialize, Serialize};

/// A place to search from.
///
/// `#[derive(...)]` asks the compiler to write these implementations for us.
/// - `Debug` lets `{:?}` print the value, which is what `dbg!` and most test
///   failures use.
/// - `Clone` allows an explicit copy. We do not derive `Copy`, because this struct
///   owns a `String` and copying it silently on every use would hide real work.
/// - `PartialEq` gives us `==`, which the tests below rely on.
/// - `Serialize`/`Deserialize` come from serde and turn this into JSON and back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Area {
    /// What a human calls it: "Budapest Zugló".
    pub label: String,
    /// The address we geocoded to get the coordinates. Kept so the number can
    /// always be traced back to the words that produced it.
    pub address: String,
    pub lat: f64,
    pub lon: f64,
    /// How many businesses one search may pull from Google before we stop asking.
    /// This, rather than distance, is what keeps a run from going on all night.
    pub max_results: u32,
}

impl Area {
    /// Distance in metres between two points on the ground.
    ///
    /// The haversine formula. `f64::to_radians` and friends are methods on the
    /// number itself — in Rust a primitive can have methods, there is no separate
    /// `Math` object to reach for.
    pub fn distance_m(&self, lat: f64, lon: f64) -> f64 {
        const EARTH_RADIUS_M: f64 = 6_371_000.0;

        let (p1, p2) = (self.lat.to_radians(), lat.to_radians());
        let dp = (lat - self.lat).to_radians();
        let dl = (lon - self.lon).to_radians();

        let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
        2.0 * EARTH_RADIUS_M * a.sqrt().asin()
    }

    // No fence: Google is asked to look around the place and whatever it sends back
    // is kept. How far away something is shows on the row, and the list's own
    // "within" filter narrows by it when you want that. A radius here would only
    // throw leads away.
}

// `#[cfg(test)]` means this module is compiled only when running tests, so the test
// code costs nothing in the real binary.
#[cfg(test)]
mod tests {
    use super::*;

    fn zuglo() -> Area {
        Area {
            label: "Budapest Zugló".into(),
            address: "Hősök tere".into(),
            lat: 47.514_900,
            lon: 19.077_800,
            max_results: 200,
        }
    }

    #[test]
    fn a_point_on_itself_is_zero_away() {
        let area = zuglo();
        assert_eq!(area.distance_m(area.lat, area.lon).round(), 0.0);
    }

    #[test]
    fn a_kilometre_north_measures_a_kilometre() {
        // One thousandth of a degree of latitude is about 111 m, so this is 1 km.
        let area = zuglo();
        let north = area.lat + 0.009;
        let metres = area.distance_m(north, area.lon);
        assert!((995.0..1005.0).contains(&metres), "got {metres} m");
    }

    #[test]
    fn the_rotterdam_row_is_still_measured_even_though_nothing_is_thrown_away() {
        // A Dutch dentist on a Budapest list. Nothing drops it — the distance is what
        // tells you, and it shows on the row as "1.1 km" or "1157 km".
        let (lat, lon) = (51.9209, 4.4777);
        let km = zuglo().distance_m(lat, lon) / 1000.0;
        assert!((1100.0..1200.0).contains(&km), "got {km} km");
    }

    #[test]
    fn the_measurement_does_not_care_which_way_round_it_is_asked() {
        let area = zuglo();
        let there = Area {
            lat: 47.6,
            lon: 19.2,
            ..zuglo()
        };
        let one = area.distance_m(there.lat, there.lon);
        let other = there.distance_m(area.lat, area.lon);
        assert!((one - other).abs() < 1.0, "{one} vs {other}");
    }
}
