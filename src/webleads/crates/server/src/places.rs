//! Turning a typed address into coordinates.
//!
//! # What this file is here to teach
//!
//! Using somebody else's public service politely, and keeping the reading of their
//! answer apart from the asking. The parsing is pure and tested; the asking is a few
//! lines around it.
//!
//! OpenStreetMap's own lookup: free, no key, not Google. Their rules ask for no more
//! than one request a second and for us to say who we are — both of which this does,
//! and typing addresses by hand is well inside either way.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Where the lookup lives.
pub const LOOKUP: &str = "https://nominatim.openstreetmap.org/search";

/// Who we are, as their rules ask. A lookup with no name attached is the one thing
/// they block.
pub const AGENT: &str = "webleads/0.1 (private lead tool, one address at a time)";

/// Their rule, and ours: one request a second, never more.
pub const GAP: Duration = Duration::from_secs(1);

/// How many matches to offer. Enough to choose from, few enough to read.
pub const MATCHES: usize = 5;

/// Which languages we would like their answer in, best first, for a search in `lang`.
///
/// This is not decoration. **The name that comes back is appended to every search
/// word**, so it has to be the name Google knows the place by in the language the
/// search asks in: asking in English turned Zugló into "14th district", and asking
/// in Hungarian turns Vienna into "Bécs".
///
/// A Hungarian search asks for Hungarian first, which keeps the districts named the
/// way they are named, then English. Any other asks for English, which also keeps
/// Dubai out of Arabic script. Asking for nothing at all gives the local script
/// everywhere. Pure.
pub fn readable_in(lang: &str) -> &'static str {
    if lang.trim().eq_ignore_ascii_case("hu") {
        "hu,en"
    } else {
        "en"
    }
}

/// Which clock is kept at this spot, read off the map.
///
/// What lets a city be added anywhere in the world without anyone typing its zone.
/// The map data comes with the program, so this asks nobody. `None` for a spot with
/// no zone at all. The map is loaded once, on first use.
pub fn zone_at(lat: f64, lon: f64) -> Option<String> {
    static FINDER: std::sync::OnceLock<tzf_rs::DefaultFinder> = std::sync::OnceLock::new();
    let zone = FINDER
        .get_or_init(tzf_rs::DefaultFinder::new)
        .get_tz_name(lon, lat)
        .trim();
    (!zone.is_empty()).then(|| zone.to_owned())
}

/// One place the lookup found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Found {
    /// The full name it came back with, so you can tell two Fő squares apart.
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    /// Which town this is in, as they name it. The city a new area belongs to is
    /// offered from this rather than typed again.
    pub city: Option<String>,
    /// The country, for reading — "United Arab Emirates".
    pub country: Option<String>,
    /// And its two letters, which is what decides the search language and the zone.
    pub country_code: Option<String>,
}

/// What their answer looks like, as far as we care.
#[derive(Debug, Deserialize)]
struct Answer {
    display_name: String,
    /// Sent as text, not as numbers — which is why this is a `String` and not an
    /// `f64`. Reading it as a number here would silently drop every result.
    lat: String,
    lon: String,
    /// Only present when we ask for `addressdetails`, and its own fields come and go
    /// with the kind of place — hence `Option` all the way down.
    #[serde(default)]
    address: Option<Parts>,
}

/// The pieces of an address we use. Everything else they send is ignored.
#[derive(Debug, Deserialize)]
struct Parts {
    /// A city proper. Absent for a village or a hamlet, which is why the next three
    /// exist.
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    town: Option<String>,
    #[serde(default)]
    village: Option<String>,
    /// The region. Last resort: better "Dubai Emirate" than nothing to offer.
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    country_code: Option<String>,
}

impl Parts {
    /// The town this place is in, whatever they happened to call the field.
    ///
    /// Their answer names the settlement differently depending on how big it is —
    /// `city` for Dubai, `town` for a market town, `village` for a hamlet. Taking only
    /// `city` would offer nothing for most of the countryside.
    fn town_name(&self) -> Option<String> {
        [&self.city, &self.town, &self.village, &self.state]
            .into_iter()
            .flatten()
            .find(|name| !name.trim().is_empty())
            .cloned()
    }
}

/// The address as a question we can ask, or a refusal.
///
/// Pure. A blank lookup would ask them for everything in the world, which is exactly
/// the kind of request that gets a tool blocked.
pub fn question(address: &str) -> Result<String, String> {
    let address = address.trim();
    if address.is_empty() {
        return Err("type an address first".to_owned());
    }
    if address.len() < 3 {
        return Err("that is too short to look up".to_owned());
    }
    Ok(address.to_owned())
}

/// Read their answer into places we can keep.
///
/// Pure, so their shape is checked against a real reply rather than hoped at. Anything
/// unreadable is dropped rather than failing the lot: four good matches and one odd
/// one is still four good matches.
pub fn read_answer(json: &str) -> Vec<Found> {
    let Ok(answers) = serde_json::from_str::<Vec<Answer>>(json) else {
        return Vec::new();
    };

    answers
        .into_iter()
        .filter_map(|answer| {
            let parts = answer.address;
            Some(Found {
                label: answer.display_name,
                lat: answer.lat.parse().ok()?,
                lon: answer.lon.parse().ok()?,
                city: parts.as_ref().and_then(Parts::town_name),
                country: parts.as_ref().and_then(|p| p.country.clone()),
                // Lower case, always: they send "ae", and a country code that
                // sometimes arrives shouting would not match our own tables.
                country_code: parts
                    .as_ref()
                    .and_then(|p| p.country_code.clone())
                    .map(|code| code.trim().to_lowercase())
                    .filter(|code| code.len() == 2),
            })
        })
        .take(MATCHES)
        .collect()
}

/// What we ask them, as a list of parameters.
///
/// Pure and tested, because one of these decides whether the whole world is
/// searchable or only one country — with a country fence, typing "Dubai" comes back
/// with a Dubai Supermarket on Rákóczi út.
pub fn lookup_params(question: &str) -> Vec<(&'static str, String)> {
    vec![
        ("q", question.to_owned()),
        ("format", "jsonv2".to_owned()),
        ("limit", MATCHES.to_string()),
        // The town and the country of each match, so adding an area can offer which
        // city it belongs to instead of asking twice.
        ("addressdetails", "1".to_owned()),
    ]
}

/// Ask the lookup about one address.
pub async fn look_up(
    http: &reqwest::Client,
    address: &str,
    lang: &str,
) -> Result<Vec<Found>, String> {
    let question = question(address)?;

    let response = http
        .get(LOOKUP)
        .query(&lookup_params(&question))
        .header(reqwest::header::USER_AGENT, AGENT)
        .header(reqwest::header::ACCEPT_LANGUAGE, readable_in(lang))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("could not reach the address lookup: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "the address lookup answered {}",
            response.status().as_u16()
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("could not read the address lookup's answer: {e}"))?;
    Ok(read_answer(&body))
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
