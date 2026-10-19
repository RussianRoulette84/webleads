//! Pairing what the scraper found with what we decided about each website.
//!
//! Pure: no network, no database, no clock. Everything here can be tested one case at
//! a time, which is what the file it was split out of could not be.

use webleads_store::Found;

/// Where a batch of businesses came from, when it came from a search.
#[derive(Debug, Clone)]
pub struct Searched {
    /// The words we typed: "borbély Budapest".
    pub phrase: String,
    /// The trade that word belongs to, which is what the Trade column shows.
    pub trade: Option<String>,
}

/// Places paired with their verdicts, plus the coordinates the database wants.
///
/// Three lists rather than one list of triples because that is the shape
/// always in step, which the tests check.
#[derive(Debug, Default)]
pub struct Judged {
    /// Each business with its position, in one piece rather than three lists that
    /// have to be kept in step by hand.
    pub found: Vec<Found>,
    /// Dropped because Google says they have shut for good.
    pub closed: usize,
}

/// Pair every place with what we decided about its website.
///
/// Everything gets kept, not only the leads. A business with a working website is not
/// worth telephoning, but it is worth *remembering* — otherwise the next import
/// fetches its homepage all over again to reach the same conclusion. The call list is
/// produced by filtering on the way out, in the database.
///
/// Pure: no network, no database, no clock. The one rule it applies is that a
/// business Google says has closed for good is nobody's customer.
pub fn assemble(
    places: Vec<webleads_scraper::Place>,
    outcomes: Vec<webleads_sitecheck::Outcome>,
    fresh: &[bool],
    // Where these came from, when they came from a scrape. `None` for an imported
    // file, which cannot know which word found what.
    from: Option<&Searched>,
    // What each business's distance is measured from: the centre of the city the
    // search was in, so a Dubai lead reads kilometres rather than thousands of them.
    // Nothing is fenced out.
    measure_from: &webleads_domain::Area,
) -> Judged {
    let mut out = Judged::default();
    // `zip` stops at the shorter of the two, so a mismatch loses rows rather than
    // pairing a place with somebody else's verdict.
    for (index, (place, outcome)) in places.into_iter().zip(outcomes).enumerate() {
        if place.permanently_closed {
            out.closed += 1;
            continue;
        }
        // Nothing is dropped for being far. How far it is shows on the row, and the
        // list's own "within" filter narrows by it when you want that — but a lead
        // thrown away here is one you never knew existed.
        let (lat, lon) = (place.lat, place.lon);
        // Kept as Google sent it, so a better parser can be applied later.
        let open_hours = place.open_hours.clone();
        out.found.push(Found {
            lead: place.into_lead(
                outcome.verdict,
                outcome.reason,
                outcome.tls,
                from.and_then(|f| f.trade.clone()),
                measure_from,
            ),
            lat,
            lon,
            found_by: from.map(|f| f.phrase.clone()),
            open_hours,
            // Only a real fetch moves the freshness date. A cached verdict marking
            // itself fresh would mean the site was never looked at again.
            freshly_checked: fresh.get(index).copied().unwrap_or(true),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use webleads_domain::{Area, Tls, Verdict};
    use webleads_scraper::Place;
    use webleads_sitecheck::Outcome;

    fn zuglo() -> Area {
        Area {
            label: "Budapest Zugló".into(),
            address: "Hősök tere".into(),
            lat: 47.514900,
            lon: 19.077800,
            max_results: 200,
        }
    }

    fn place(id: &str, closed: bool) -> Place {
        Place {
            id: id.into(),
            name: format!("Shop {id}"),
            phone: Some("+36 1 234 5001".into()),
            category: Some("Barber shop".into()),
            address: Some("Hősök tere 1".into()),
            maps_url: None,
            website: None,
            reviews: 0,
            rating: None,
            lat: 47.5149,
            lon: 19.0778,
            permanently_closed: closed,
            open_hours: None,
        }
    }

    fn outcome(verdict: Verdict) -> Outcome {
        Outcome {
            verdict,
            reason: verdict.describe().to_owned(),
            tls: Tls::Unknown,
        }
    }

    /// Somewhere far from Zugló, to search while measuring from somewhere else.
    fn another_district() -> Area {
        Area {
            label: "Óbuda".into(),
            address: "Fő tér 1".into(),
            // A few kilometres away, across the river.
            lat: 47.5400,
            lon: 19.0400,
            max_results: 200,
        }
    }

    fn place_at(id: &str, lat: f64, lon: f64) -> Place {
        Place {
            lat,
            lon,
            ..place(id, false)
        }
    }

    // ---- assemble ----

    #[test]
    fn distance_is_measured_from_the_point_we_are_given() {
        // Not from anywhere the searching decided for itself. A business found while
        // searching Óbuda is reported as how far it is from the point handed in —
        // which is its own city's centre, so "nearest first" means the same thing on
        // every row of every city.
        let obuda = another_district();
        let out = assemble(
            vec![place_at("there", obuda.lat, obuda.lon)],
            vec![outcome(Verdict::NoSite)],
            &[true],
            None,
            &zuglo(),
        );

        assert_eq!(out.found.len(), 1, "it is inside the area being searched");
        let away = out.found[0].lead.distance_m;
        let expected = zuglo().distance_m(obuda.lat, obuda.lon).round() as u32;
        assert_eq!(away, expected);
        assert!(away > 3_000, "a few kilometres, not nought: {away}");
    }

    #[test]
    fn a_business_in_its_own_city_is_a_few_kilometres_away_not_a_few_thousand() {
        // The whole point of measuring per city. Measured from Zugló, a Dubai shop
        // reads four thousand kilometres and the "within" slider is useless there.
        let dubai_centre = Area {
            label: "Dubai".into(),
            address: String::new(),
            lat: 25.2048,
            lon: 55.2708,
            max_results: 0,
        };
        // A shop in Downtown Dubai, a few kilometres from the middle.
        let out = assemble(
            vec![place_at("there", 25.1948, 55.2782)],
            vec![outcome(Verdict::NoSite)],
            &[true],
            None,
            &dubai_centre,
        );
        let away = out.found[0].lead.distance_m;
        assert!(away < 5_000, "a few kilometres, not a few thousand: {away}");

        // And from Zugló it would have been four thousand of them.
        let wrong = zuglo().distance_m(25.1948, 55.2782).round() as u32;
        assert!(wrong > 4_000_000, "{wrong}");
    }

    #[test]
    fn every_business_is_kept_not_only_the_ones_worth_calling() {
        let out = assemble(
            vec![place("a", false), place("b", false)],
            vec![outcome(Verdict::NoSite), outcome(Verdict::LiveSite)],
            &[true; 8],
            None,
            &zuglo(),
        );
        // Two in, two out. The live one is remembered so we never re-check it blind.
        assert_eq!(out.found.len(), 2);
        assert_eq!(out.found[1].lead.verdict, Verdict::LiveSite);
    }

    #[test]
    fn a_business_that_has_shut_for_good_is_dropped() {
        let out = assemble(
            vec![
                place("open", false),
                place("shut", true),
                place("also", false),
            ],
            vec![
                outcome(Verdict::NoSite),
                outcome(Verdict::NoSite),
                outcome(Verdict::NoSite),
            ],
            &[true; 8],
            None,
            &zuglo(),
        );
        assert_eq!(out.found.len(), 2);
        assert!(out.found.iter().all(|f| f.lead.id != "shut"));
    }

    #[test]
    fn the_three_lists_stay_the_same_length_and_in_step() {
        let out = assemble(
            vec![place("a", false), place("shut", true), place("c", false)],
            vec![
                outcome(Verdict::NoSite),
                outcome(Verdict::DeadSite),
                outcome(Verdict::SocialOnly),
            ],
            &[true; 8],
            None,
            &zuglo(),
        );
        // The position travels with the lead now, so they cannot come apart.
        assert!(out.found.iter().all(|f| f.lat > 47.0 && f.lon > 19.0));
        // The dropped row must not shift the remaining verdicts along by one.
        assert_eq!(out.found[0].lead.verdict, Verdict::NoSite);
        assert_eq!(out.found[1].lead.verdict, Verdict::SocialOnly);
    }

    #[test]
    fn a_short_list_of_verdicts_loses_rows_rather_than_mispairing_them() {
        // If this ever pairs place `b` with place `a`'s verdict, the call list starts
        // telling people the wrong reason. Losing the row is the safer failure.
        let out = assemble(
            vec![place("a", false), place("b", false)],
            vec![outcome(Verdict::DeadSite)],
            &[true; 8],
            None,
            &zuglo(),
        );
        assert_eq!(out.found.len(), 1);
        assert_eq!(out.found[0].lead.id, "a");
        assert_eq!(out.found[0].lead.verdict, Verdict::DeadSite);
    }

    #[test]
    fn nothing_in_gives_nothing_out_rather_than_a_panic() {
        let out = assemble(vec![], vec![], &[], None, &zuglo());
        assert!(out.found.is_empty());
        assert_eq!(out.closed, 0);
    }

    fn at(id: &str, lat: f64, lon: f64) -> Place {
        Place {
            lat,
            lon,
            ..place(id, false)
        }
    }

    // ---- the radius, which is the bug that started this ----

    #[test]
    fn with_no_fence_a_business_in_rotterdam_is_kept() {
        // Not a mistake — it is the default, and it must not filter silently.
        let out = assemble(
            vec![at("rotterdam", 51.9209, 4.4777)],
            vec![outcome(Verdict::NoSite)],
            &[true; 8],
            None,
            &zuglo(),
        );
        assert_eq!(out.found.len(), 1);
    }

    #[test]
    fn the_rotterdam_business_is_kept_now_and_measured_rather_than_dropped() {
        // A lead from far away is kept: thrown away here it is one you never knew
        // existed, and how far it is shows on the row — 1157 km is its own warning.
        let out = assemble(
            vec![
                at("zuglo", 47.5149, 19.0778),
                at("rotterdam", 51.9209, 4.4777),
            ],
            vec![outcome(Verdict::NoSite), outcome(Verdict::NoSite)],
            &[true; 8],
            None,
            &zuglo(),
        );
        assert_eq!(out.found.len(), 2);
        let far = out
            .found
            .iter()
            .find(|f| f.lead.id == "rotterdam")
            .expect("kept");
        assert!(far.lead.distance_m > 1_000_000, "{}", far.lead.distance_m);
    }

    #[test]
    fn the_only_thing_dropped_is_a_business_that_has_shut_for_good() {
        let out = assemble(
            vec![
                at("near", 47.5149, 19.0778),
                place("shut", true),
                at("far", 51.9209, 4.4777),
            ],
            vec![
                outcome(Verdict::NoSite),
                outcome(Verdict::NoSite),
                outcome(Verdict::NoSite),
            ],
            &[true; 8],
            None,
            &zuglo(),
        );
        // The far one is kept — only the shut one goes, and it is counted rather
        // than silently discarded.
        assert_eq!(out.found.len(), 2);
        assert_eq!(out.closed, 1);
    }

    #[test]
    fn every_verdict_stays_with_the_business_it_was_made_about() {
        // If a place were removed but not its verdict, every business after it would
        // be given somebody else's reason.
        let out = assemble(
            vec![at("far", 51.9209, 4.4777), at("near", 47.5149, 19.0778)],
            vec![outcome(Verdict::DeadSite), outcome(Verdict::SocialOnly)],
            &[true; 8],
            None,
            &zuglo(),
        );
        assert_eq!(out.found.len(), 2);
        assert_eq!(out.found[0].lead.id, "far");
        assert_eq!(out.found[0].lead.verdict, Verdict::DeadSite);
        assert_eq!(out.found[1].lead.id, "near");
        assert_eq!(out.found[1].lead.verdict, Verdict::SocialOnly);
    }

    // ---- the certificate ----

    #[test]
    fn a_broken_certificate_reaches_the_stored_lead() {
        let mut found = outcome(Verdict::InsecureSite);
        found.tls = Tls::Broken("certificate has expired".into());
        let out = assemble(
            vec![place("a", false)],
            vec![found],
            &[true],
            None,
            &zuglo(),
        );
        assert_eq!(out.found[0].lead.verdict, Verdict::InsecureSite);
        assert!(out.found[0].lead.tls.is_a_problem());
        assert_eq!(out.found[0].lead.tls.describe(), "certificate has expired");
    }

    // ---- paging ----
}
