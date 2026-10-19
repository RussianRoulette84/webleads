//! A city: a country, a clock, a language and a centre.
//!
//! # What this file is here to teach
//!
//! Where a city's facts come from. Its clock is read off the map from where it is,
//! its search language from which country it is in, and its telephones from the
//! short table below. So a city anywhere in the world can be added, and nothing
//! about it has to be typed by hand.

use serde::{Deserialize, Serialize};

/// A city we look for businesses in, and the areas inside it belong to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct City {
    pub name: String,
    /// Two lower-case letters, as the address lookup reports them.
    pub country_code: String,
    /// An IANA zone name. This city's opening hours are read in it — not your
    /// ring-backs, which are in your own time wherever the shop is.
    pub zone: String,
    /// What we ask Google in, and which column of the trade list we search with.
    pub lang: String,
    /// Distances in this city are measured from here.
    pub centre_lat: f64,
    pub centre_lon: f64,
}

impl City {
    /// Where distances in this city are measured from, as an area.
    ///
    /// Reuses [`crate::Area::distance_m`] rather than a second haversine.
    pub fn centre(&self) -> crate::Area {
        crate::Area {
            label: self.name.clone(),
            address: String::new(),
            lat: self.centre_lat,
            lon: self.centre_lon,
            max_results: 0,
        }
    }

    /// How far a business in this city is from its centre, in metres.
    pub fn distance_m(&self, lat: f64, lon: f64) -> f64 {
        self.centre().distance_m(lat, lon)
    }
}

/// The languages the trade list is written in, and what each is called.
///
/// **In the order they matter, not alphabetical.** It decides the order of the
/// columns in the trades file and of the columns on the screen — Hungarian first
/// because that is the market the list was built for. Sorting them instead put
/// English first and quietly reshuffled a hand-edited file.
///
/// A third language becomes a `words_*` column in the file and a line here. One that
/// appears in the file but not here still works: it gets a column after these.
pub const LANGUAGES: [(&str, &str); 2] = [("hu", "Hungarian"), ("en", "English")];

/// Every language of these trades, the ones we know about first and in order.
///
/// Pure, and tested: the order is the file's column order, and a file whose columns
/// move about is a file that reads as damaged.
pub fn languages_in_order(found: &[String]) -> Vec<String> {
    let mut ordered: Vec<String> = LANGUAGES
        .iter()
        .map(|(code, _)| (*code).to_owned())
        .filter(|code| found.contains(code))
        .collect();
    // Anything the file has that we have not written down here, kept rather than
    // dropped — losing a column would lose every word in it.
    let mut rest: Vec<String> = found
        .iter()
        .filter(|code| !ordered.contains(code))
        .cloned()
        .collect();
    rest.sort_unstable();
    ordered.extend(rest);
    ordered
}

/// What we know about a country's telephones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Country {
    /// Two lower-case letters.
    pub code: &'static str,
    /// What a person calls it.
    pub name: &'static str,
    /// The dialling code, without the plus.
    pub dials: &'static str,
    /// What a number written for people at home starts with, longest first, so
    /// `0036` is tried before `06` and the `36` is not swallowed.
    pub trunks: &'static [&'static str],
}

/// The countries whose telephone numbers we can write for dialling.
///
/// A country missing from here still works everywhere else — its clock comes from
/// the map and its search language from [`lang_for`] — but its numbers keep their
/// digits untouched rather than being guessed at. Adding a row is how a country's
/// numbers become dialable from abroad.
pub const COUNTRIES: &[Country] = &[
    Country {
        code: "hu",
        name: "Hungary",
        dials: "36",
        trunks: &["0036", "06"],
    },
    Country {
        code: "ae",
        name: "United Arab Emirates",
        dials: "971",
        trunks: &["00971", "0"],
    },
    Country {
        code: "gb",
        name: "United Kingdom",
        dials: "44",
        trunks: &["0044", "0"],
    },
];

/// What we know about this country, if we know it.
///
/// Pure. The two letters come from the address lookup, which sends them lower case,
/// but a stored value is ours to be careful about.
pub fn country(code: &str) -> Option<&'static Country> {
    let code = code.trim().to_lowercase();
    COUNTRIES.iter().find(|c| c.code == code)
}

/// The language a city in this country is searched in.
///
/// The trade list has Hungarian and English words, so Hungary searches in Hungarian
/// and everywhere else in English — Google answers in English across most of the
/// world, Dubai included. A third language in the file becomes a line here. Pure.
pub fn lang_for(code: &str) -> &'static str {
    if code.trim().eq_ignore_ascii_case("hu") {
        "hu"
    } else {
        "en"
    }
}

/// How far from a city its starting point may be moved, in kilometres.
///
/// Generous enough for a suburb or a commute, and far short of the next country.
pub const CENTRE_REACH_KM: f64 = 100.0;

/// Whether a new starting point is in reach of the city it measures.
///
/// `near` is the city's own areas, or its present centre when it has none. A spot
/// beyond [`CENTRE_REACH_KM`] of every one of them is another place entirely, and
/// measuring Budapest from London re-measures every lead in it as fourteen hundred
/// kilometres away. Pure.
pub fn check_centre_reach(
    city: &str,
    label: &str,
    lat: f64,
    lon: f64,
    near: &[crate::Area],
) -> Result<(), String> {
    let nearest_km = near
        .iter()
        .map(|spot| spot.distance_m(lat, lon) / 1000.0)
        .fold(f64::INFINITY, f64::min);
    if nearest_km <= CENTRE_REACH_KM {
        return Ok(());
    }
    Err(format!(
        "{} is {:.0} km from {city} — your starting point has to be in {city}. \
         To search there, add it under \"Search another place\".",
        label.trim(),
        nearest_km
    ))
}

/// A new spot to measure a city from, checked before anything is moved.
///
/// Gives back the label tidied, or the reason in words. Pure, so a bad request is a
/// test rather than a row of nonsense distances.
pub fn check_centre(label: &str, lat: f64, lon: f64) -> Result<String, String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("the new centre needs a name, as the address box gives it".to_owned());
    }
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return Err(format!("{lat}, {lon} is not a place on the map"));
    }
    Ok(label.to_owned())
}

#[cfg(test)]
#[path = "city_tests.rs"]
mod tests;
