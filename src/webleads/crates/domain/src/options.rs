//! Every setting a scrape can take, with its default and its explanation.
//!
//! The help text lives here, beside the default it describes. Put it in the markup
//! instead and the two drift apart the first time a default changes.

use serde::{Deserialize, Serialize};

/// What went wrong with a set of settings, and which field caused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidOption {
    pub field: &'static str,
    pub said: String,
}

/// The twelve settings the scraper accepts, minus the one we removed.
///
/// Email extraction is deliberately absent. It works by visiting each business's
/// website looking for an address, and our leads are defined by not having one — so
/// it costs a request per business and finds almost nothing. Leaving the field out
/// entirely means it cannot be switched on by accident.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrapeOptions {
    pub name: Option<String>,
    pub keywords: Vec<String>,
    pub lang: String,
    /// How tight Google's map view is. `None` leaves it out of the request
    /// entirely and lets Google decide — which is what you want for a district or a
    /// town, where a fixed view either misses the edges or drifts into the next one.
    pub zoom: Option<u8>,
    pub lat: f64,
    pub lon: f64,
    /// How far to scroll each result list. `None` leaves it out and takes the
    /// scraper's own.
    pub depth: Option<u8>,
    pub fast_mode: bool,
    pub max_time_s: u32,
    /// Never rendered back into the page and never logged.
    pub proxies: Vec<String>,
}

/// The zoom to start from, and what fast mode is given.
///
/// Twelve is a city or a large district — what this is used for. Sixteen is a few
/// streets, and misses the edges of anything bigger than a neighbourhood.
///
/// **In fast mode the number makes no difference and its presence is everything.**
/// Measured: zoom 1, 10, 12, 14, 16 and 21 all return the same twenty businesses,
/// while 0, 22, 99 and no zoom at all are accepted and then die mid-run with
/// `invalid zoom level` — leaving a job stuck at "working" that blocks every other
/// job on the scraper behind it.
pub const CITY_ZOOM: u8 = 12;

/// How far to scroll each result list, and the most we will offer.
///
/// **Measured, on the 20th of September 2026**, one busy word from one point:
/// depth 1 → 16 businesses, 5 → 40, 10 → 78, 12 → 88, **15 → 108**, and then
/// **18 → nothing and 20 → nothing**, each run twice. Google's own ceiling is about
/// 120 per search, so fifteen reaches most of what there is; past it the run finishes
/// "successfully" with an empty file, which is the worst way to lose an afternoon.
///
/// Sixteen and seventeen were never tried. See `docs/SCRAPER_SETTINGS.md`.
pub const USUAL_DEPTH: u8 = 15;

/// The deepest we will let anybody ask for. See [`USUAL_DEPTH`] for why.
pub const DEEPEST: u8 = 15;

/// The zoom levels the scraper will actually accept. Outside this a fast job hangs.
pub const ZOOM_RANGE: std::ops::RangeInclusive<u8> = 1..=21;

impl ScrapeOptions {
    /// The settings for searching one area.
    ///
    /// Everything else stays as [`Default`] — the language, the zoom, the depth. The
    /// only thing an area decides is where to look and how wide.
    pub fn around(area: &crate::Area) -> Self {
        Self {
            lat: area.lat,
            lon: area.lon,
            ..Default::default()
        }
    }
}

/// `Default` is a trait — a shared name for "the sensible starting value". Writing
/// it by hand rather than deriving it lets each number carry a reason.
impl Default for ScrapeOptions {
    fn default() -> Self {
        Self {
            name: None,
            keywords: Vec::new(),
            // English until you pick another on the page, which is then remembered.
            lang: "en".to_owned(),
            // Left out of the request unless you turn it on.
            zoom: None,
            lat: 47.514_900,
            lon: 19.077_800,
            // Left out of the request unless you turn it on.
            depth: None,
            // Off: it strips the fields we use to tell two businesses apart.
            fast_mode: false,
            max_time_s: 3600,
            proxies: Vec::new(),
        }
    }
}

impl ScrapeOptions {
    /// Check the whole set and collect every problem, not just the first.
    ///
    /// A `Vec` of failures rather than bailing on the first one: the interface shows
    /// each message beside the field that caused it, so it needs all of them.
    pub fn validate(&self) -> Result<(), Vec<InvalidOption>> {
        let mut wrong = Vec::new();
        let mut complain = |field, said: &str| {
            wrong.push(InvalidOption {
                field,
                said: said.to_owned(),
            });
        };

        if self.keywords.is_empty() {
            complain(
                "keywords",
                "Nothing to search for. Choose at least one trade.",
            );
        }
        if !(-90.0..=90.0).contains(&self.lat) {
            complain("lat", "Latitude has to be between -90 and 90.");
        }
        if !(-180.0..=180.0).contains(&self.lon) {
            complain("lon", "Longitude has to be between -180 and 180.");
        }
        // One is refused where it is typed rather than accepted and then hung on.
        if self.zoom.is_some_and(|zoom| !ZOOM_RANGE.contains(&zoom)) {
            complain(
                "zoom",
                "Zoom runs from 1 to 21. Outside that a fast search is accepted and then hangs.",
            );
        }
        if self
            .depth
            .is_some_and(|depth| !(1..=DEEPEST).contains(&depth))
        {
            complain(
                "depth",
                "Depth has to be between 1 and 15. Past fifteen the search comes back empty.",
            );
        }
        if self.lang.trim().len() != 2 {
            complain("lang", "A language is a two letter code, like hu or en.");
        }
        if self.max_time_s == 0 {
            complain("max_time_s", "A run needs more than no time at all.");
        }

        if wrong.is_empty() {
            Ok(())
        } else {
            Err(wrong)
        }
    }

    /// What the interface shows when you rest on the small **i** beside a field.
    pub fn help(field: &str) -> Option<&'static str> {
        Some(match field {
            "keywords" => {
                "Which trades to search for. Each trade is one or more words in the \
                city's own language; the count tells you how many searches that is."
            }
            "lang" => {
                "Which language Google answers in, and which trade words get \
                searched. The city's own: a business describes itself in the language \
                of the place it is in, and asking in another finds a fraction of them."
            }
            "zoom" => {
                "How tight Google's map view is, 1 to 21. Off means we choose — 12, \
                a city or a district. Switch it on to pin it yourself: 16 is a few \
                streets, and it misses the edges of anything bigger. Fast mode needs \
                one, and takes no notice of which."
            }
            "depth" => {
                "How far to scroll each result list, and the one setting that really \
                changes how much you get: ten brings back about 78 businesses, \
                fifteen about 108. Fifteen is as high as it goes — past that the \
                search comes back empty. Fast mode ignores it entirely."
            }
            "fast_mode" => {
                "One request instead of a browser: far quicker, and it still \
                brings back the telephone, the website and the opening times. It \
                looks wherever the words point rather than staying near your area, \
                so put the place in the words — “fodrászat Óbuda” or “barber shop \
                Deira”, not just the trade on its own."
            }
            "max_time_s" => {
                "When to give up on a run. Whatever it found by then is \
                kept, never discarded."
            }
            "proxies" => {
                "Only needed once Google starts refusing us. What you enter \
                is stored outside the project and never shown again."
            }
            "name" => "A label for this run, so you can find it in the list later.",
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- settings for one area ----

    #[test]
    fn an_area_decides_where_to_look_and_nothing_else() {
        let zuglo = crate::Area {
            label: "Zugló".into(),
            address: "Hősök tere".into(),
            lat: 47.514_900,
            lon: 19.077_800,
            max_results: 200,
        };
        let asking = ScrapeOptions::around(&zuglo);
        assert_eq!(asking.lat, zuglo.lat);
        assert_eq!(asking.lon, zuglo.lon);
        // How far to look is not the area's business: nothing is thrown away for
        // being far, so there is no fence to carry.
        assert_eq!(asking.zoom, None);
        assert_eq!(asking.depth, None);
    }

    #[test]
    fn an_area_decides_only_where_to_look() {
        // Everything else — the language, the zoom, the depth — is ours and stays put.
        let city = crate::Area {
            label: "Budapest".into(),
            address: String::new(),
            lat: 1.0,
            lon: 2.0,
            max_results: 200,
        };
        let asking = ScrapeOptions::around(&city);
        let ours = ScrapeOptions::default();
        assert_eq!(asking.lang, ours.lang);
        assert_eq!(asking.zoom, ours.zoom);
        assert_eq!(asking.depth, ours.depth);
        assert_eq!(asking.fast_mode, ours.fast_mode);
    }

    #[test]
    fn settings_built_for_an_area_are_settings_we_would_accept() {
        let city = crate::Area {
            label: "Budapest".into(),
            address: String::new(),
            lat: 47.5,
            lon: 19.1,
            max_results: 200,
        };
        let mut asking = ScrapeOptions::around(&city);
        asking.keywords = vec!["borbély Budapest".to_owned()];
        assert!(asking.validate().is_ok());
    }

    #[test]
    fn the_defaults_are_usable_once_a_trade_is_chosen() {
        let mut options = ScrapeOptions::default();
        assert!(options.validate().is_err(), "no keywords means no run");
        options
            .keywords
            .push("vízvezeték-szerelő Budapest Zugló".to_owned());
        assert!(options.validate().is_ok());
    }

    #[test]
    fn every_problem_comes_back_at_once() {
        let options = ScrapeOptions {
            zoom: Some(99),
            depth: Some(0),
            lat: 1000.0,
            ..Default::default()
        };
        let wrong = options.validate().expect_err("that should not be accepted");
        let fields: Vec<&str> = wrong.iter().map(|w| w.field).collect();
        assert!(fields.contains(&"zoom"));
        assert!(fields.contains(&"depth"));
        assert!(fields.contains(&"lat"));
        assert!(fields.contains(&"keywords"));
    }

    // ---- what the measurements decided ----

    #[test]
    fn the_depth_we_offer_is_the_deepest_one_that_works() {
        // Measured: 15 returns 108 businesses, 18 and 20 return none, twice each.
        assert_eq!(USUAL_DEPTH, 15);
        assert_eq!(DEEPEST, USUAL_DEPTH, "we start at the most we allow");
    }

    #[test]
    fn a_depth_past_the_measured_ceiling_is_refused() {
        let too_deep = ScrapeOptions {
            keywords: vec!["fodrászat".into()],
            depth: Some(18),
            ..Default::default()
        };
        let wrong = too_deep.validate().expect_err("refused");
        assert!(wrong.iter().any(|w| w.field == "depth"), "{wrong:?}");
        // And the message says what would happen, not just that it is out of range.
        assert!(wrong.iter().any(|w| w.said.contains("empty")), "{wrong:?}");
    }

    #[test]
    fn every_depth_that_was_measured_working_is_allowed() {
        for depth in [1, 5, 10, 12, 15] {
            let asking = ScrapeOptions {
                keywords: vec!["fodrászat".into()],
                depth: Some(depth),
                ..Default::default()
            };
            assert!(asking.validate().is_ok(), "depth {depth} was refused");
        }
    }

    #[test]
    fn a_zoom_outside_what_the_scraper_takes_is_refused_before_it_can_hang() {
        // Nought and twenty-two are accepted by the scraper and then kill the job
        // mid-run, blocking everything behind it. Refusing them here is the fix.
        for zoom in [0, 22, 99] {
            let asking = ScrapeOptions {
                keywords: vec!["fodrászat".into()],
                zoom: Some(zoom),
                ..Default::default()
            };
            let wrong = asking.validate().expect_err("refused");
            assert!(wrong.iter().any(|w| w.field == "zoom"), "zoom {zoom}");
        }
        for zoom in [1, 12, 21] {
            let asking = ScrapeOptions {
                keywords: vec!["fodrászat".into()],
                zoom: Some(zoom),
                ..Default::default()
            };
            assert!(asking.validate().is_ok(), "zoom {zoom} was refused");
        }
    }

    #[test]
    fn every_setting_explains_itself() {
        for field in [
            "keywords",
            "lang",
            "zoom",
            "depth",
            "fast_mode",
            "max_time_s",
            "proxies",
            "name",
        ] {
            assert!(
                ScrapeOptions::help(field).is_some(),
                "{field} has no help text"
            );
        }
    }
}
