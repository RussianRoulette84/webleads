//! One row of the scraper's CSV, and how it becomes something we can use.

use serde::Deserialize;
use webleads_domain::{Area, Lead, Tls, Verdict};

/// A row exactly as the scraper writes it.
///
/// The column names are not ours, so `#[serde(rename)]` maps them. Every field is
/// `Option<String>` because the scraper leaves cells empty rather than omitting them —
/// and in fast mode it leaves `cid`, `place_id` and `link` empty for every row.
#[derive(Debug, Clone, Deserialize)]
pub struct RawRow {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub review_count: Option<String>,
    #[serde(default)]
    pub review_rating: Option<String>,
    #[serde(default)]
    pub latitude: Option<String>,
    #[serde(default)]
    pub longitude: Option<String>,
    #[serde(default)]
    pub cid: Option<String>,
    #[serde(default)]
    pub place_id: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    /// When they are open, as Google's own JSON. Read but not understood here —
    /// `webleads_domain::Hours` is the one place that knows what it means.
    #[serde(default)]
    pub open_hours: Option<String>,
}

/// A business, cleaned up, before anyone has decided whether it is a lead.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub id: String,
    pub name: String,
    pub phone: Option<String>,
    pub category: Option<String>,
    pub address: Option<String>,
    pub maps_url: Option<String>,
    pub website: Option<String>,
    pub reviews: u32,
    pub rating: Option<f32>,
    pub lat: f64,
    pub lon: f64,
    pub permanently_closed: bool,
    /// Their week, still as Google's JSON.
    pub open_hours: Option<String>,
}

/// An empty cell and a missing cell mean the same thing to us, so collapse both to
/// `None`. Doing this once here keeps every later `if x.is_some()` honest.
fn tidy(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

impl RawRow {
    /// Turn a raw row into a `Place`, or `None` if it has no name or no position.
    pub fn into_place(self) -> Option<Place> {
        let name = tidy(self.title)?;
        let lat = tidy(self.latitude)?.parse().ok()?;
        let lon = tidy(self.longitude)?.parse().ok()?;

        // `as_deref` turns `&Option<String>` into `Option<&str>` without copying the
        // string — `Lead::identity` only needs to look at it.
        let google_id = tidy(self.cid).or_else(|| tidy(self.place_id));
        let id = Lead::identity(google_id.as_deref(), &name, lat, lon);

        let address = tidy(self.address);
        let maps_url = tidy(self.link).or_else(|| rebuild_maps_url(&name, address.as_deref()));

        Some(Place {
            id,
            name,
            phone: tidy(self.phone),
            category: tidy(self.category),
            address,
            maps_url,
            website: tidy(self.website),
            reviews: tidy(self.review_count)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            rating: tidy(self.review_rating).and_then(|v| v.parse().ok()),
            lat,
            lon,
            permanently_closed: tidy(self.status)
                .is_some_and(|s| s.to_lowercase().contains("permanently closed")),
            // `{}` from fast mode is no hours at all, and `tidy` already treats an
            // empty cell that way.
            open_hours: tidy(self.open_hours).filter(|h| h != "{}"),
        })
    }
}

/// Fast mode strips the Maps link, so build one from what is left.
fn rebuild_maps_url(name: &str, address: Option<&str>) -> Option<String> {
    let query = match address {
        Some(a) => format!("{name} {a}"),
        None => name.to_owned(),
    };
    let escaped: String = query
        .chars()
        .map(|c| match c {
            ' ' => "+".to_owned(),
            c if c.is_ascii_alphanumeric() => c.to_string(),
            c => c
                .to_string()
                .bytes()
                .map(|b| format!("%{b:02X}"))
                .collect::<String>(),
        })
        .collect();
    Some(format!("https://www.google.com/maps/search/{escaped}"))
}

impl Place {
    /// Attach a verdict and become a `Lead`.
    ///
    /// Takes `self` by value: a `Place` becomes a `Lead` and there is no reason to keep
    /// the old one, so this moves the strings across instead of cloning them.
    pub fn into_lead(
        self,
        verdict: Verdict,
        reason: String,
        tls: Tls,
        trade: Option<String>,
        area: &Area,
    ) -> Lead {
        let distance_m = area.distance_m(self.lat, self.lon).round() as u32;
        let social = verdict == Verdict::SocialOnly;
        Lead {
            id: self.id,
            name: self.name,
            phone: self.phone,
            category: self.category,
            trade,
            address: self.address,
            maps_url: self.maps_url,
            website: if social { None } else { self.website.clone() },
            social_url: if social { self.website } else { None },
            verdict,
            reason,
            reviews: self.reviews,
            rating: self.rating,
            distance_m,
            position: Some((self.lat, self.lon)),
            tls,
            hours: self
                .open_hours
                .as_deref()
                .map(webleads_domain::Hours::parse)
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row with everything filled in, which each test then spoils in one way.
    fn full_row() -> RawRow {
        RawRow {
            title: Some("Borzas Barber".into()),
            category: Some("Barber shop".into()),
            address: Some("Hősök tere 1".into()),
            website: Some("https://borzas.hu".into()),
            phone: Some("+36 1 234 5001".into()),
            review_count: Some("12".into()),
            review_rating: Some("4.5".into()),
            latitude: Some("47.5149".into()),
            longitude: Some("19.0778".into()),
            cid: Some("123".into()),
            place_id: Some("ChIJabc".into()),
            link: Some("https://maps.google.com/?cid=123".into()),
            status: Some("Open".into()),
            open_hours: Some(r#"{"Monday":["8 am–5 pm"]}"#.into()),
        }
    }

    fn zuglo() -> Area {
        Area {
            label: "Budapest Zugló".into(),
            address: "Hősök tere".into(),
            lat: 47.514900,
            lon: 19.077800,
            max_results: 200,
        }
    }

    // ---- tidy ----

    #[test]
    fn tidy_treats_missing_and_empty_as_the_same_thing() {
        assert_eq!(tidy(None), None);
        assert_eq!(tidy(Some(String::new())), None);
        assert_eq!(tidy(Some("   ".into())), None);
        assert_eq!(tidy(Some("\t\n".into())), None);
    }

    #[test]
    fn tidy_trims_but_keeps_what_is_inside() {
        assert_eq!(tidy(Some("  Borzas  ".into())), Some("Borzas".into()));
        assert_eq!(tidy(Some("a b".into())), Some("a b".into()));
    }

    // ---- rebuild_maps_url ----

    #[test]
    fn a_rebuilt_link_uses_the_name_and_the_address() {
        let url = rebuild_maps_url("Borzas Barber", Some("Hősök tere 1")).expect("built");
        assert!(url.starts_with("https://www.google.com/maps/search/"));
        assert!(url.contains("Borzas+Barber"));
        assert!(url.contains("tere"));
    }

    #[test]
    fn a_rebuilt_link_works_with_no_address_at_all() {
        let url = rebuild_maps_url("Fodrászat", None).expect("built");
        assert!(url.starts_with("https://www.google.com/maps/search/"));
        // No trailing separator left behind where the address would have been.
        assert!(!url.ends_with('+'));
    }

    #[test]
    fn hungarian_letters_are_escaped_rather_than_pasted_raw() {
        let url = rebuild_maps_url("Fodrászat", None).expect("built");
        // A raw á in a URL is not something every client handles the same way.
        assert!(!url.contains('á'));
        assert!(url.contains('%'));
    }

    // ---- into_place ----

    #[test]
    fn a_complete_row_becomes_a_complete_place() {
        let place = full_row().into_place().expect("usable");
        assert_eq!(place.id, "123"); // cid wins over place_id
        assert_eq!(place.name, "Borzas Barber");
        assert_eq!(place.reviews, 12);
        assert_eq!(place.rating, Some(4.5));
        assert!(!place.permanently_closed);
    }

    #[test]
    fn place_id_stands_in_when_there_is_no_cid() {
        let mut row = full_row();
        row.cid = None;
        assert_eq!(row.into_place().expect("usable").id, "ChIJabc");
    }

    #[test]
    fn a_row_with_no_name_or_no_position_is_unusable() {
        for spoil in [
            |r: &mut RawRow| r.title = None,
            |r: &mut RawRow| r.title = Some("  ".into()),
            |r: &mut RawRow| r.latitude = None,
            |r: &mut RawRow| r.longitude = None,
        ] {
            let mut row = full_row();
            spoil(&mut row);
            assert!(row.into_place().is_none());
        }
    }

    #[test]
    fn coordinates_that_are_not_numbers_make_the_row_unusable() {
        // Better to drop the row than to place a business at 0,0 off the Gulf of
        // Guinea, which is what a silent default would do.
        let mut row = full_row();
        row.latitude = Some("north a bit".into());
        assert!(row.into_place().is_none());
    }

    #[test]
    fn a_review_count_that_is_not_a_number_counts_as_none() {
        let mut row = full_row();
        row.review_count = Some("many".into());
        row.review_rating = Some("good".into());
        let place = row.into_place().expect("usable");
        assert_eq!(place.reviews, 0);
        assert_eq!(place.rating, None);
    }

    #[test]
    fn a_business_google_says_is_shut_is_marked_as_shut() {
        let mut row = full_row();
        row.status = Some("Permanently closed".into());
        assert!(row.into_place().expect("usable").permanently_closed);

        let mut row = full_row();
        row.status = Some("PERMANENTLY CLOSED".into());
        assert!(row.into_place().expect("usable").permanently_closed);

        let mut row = full_row();
        row.status = Some("Temporarily closed".into());
        assert!(!row.into_place().expect("usable").permanently_closed);
    }

    // ---- into_lead: the swap ----

    #[test]
    fn a_social_verdict_moves_the_link_out_of_the_website_column() {
        let mut row = full_row();
        row.website = Some("https://facebook.com/borzas".into());
        let place = row.into_place().expect("usable");

        let lead = place.into_lead(
            Verdict::SocialOnly,
            "only facebook.com".into(),
            Tls::Broken("certificate has expired".into()),
            None,
            &zuglo(),
        );
        // This is the whole point: they do not have a website, they have a page.
        assert_eq!(lead.website, None);
        assert_eq!(
            lead.social_url.as_deref(),
            Some("https://facebook.com/borzas")
        );
    }

    #[test]
    fn every_other_verdict_leaves_the_link_where_it_was() {
        for verdict in [Verdict::NoSite, Verdict::DeadSite, Verdict::LiveSite] {
            let place = full_row().into_place().expect("usable");
            let lead = place.into_lead(verdict, "because".into(), Tls::Unknown, None, &zuglo());
            assert_eq!(lead.website.as_deref(), Some("https://borzas.hu"));
            assert_eq!(lead.social_url, None, "{verdict:?} must not move the link");
        }
    }

    #[test]
    fn the_distance_is_measured_by_us_not_copied_from_anywhere() {
        let place = full_row().into_place().expect("usable");
        let lead = place.into_lead(
            Verdict::NoSite,
            "no link".into(),
            Tls::Unknown,
            None,
            &zuglo(),
        );
        // Hősök tere is the centre, and this row sits a few metres off it.
        assert!(lead.distance_m < 100, "got {} m", lead.distance_m);

        let mut row = full_row();
        row.latitude = Some("47.5300".into());
        let far = row.into_place().expect("usable");
        let far = far.into_lead(
            Verdict::NoSite,
            "no link".into(),
            Tls::Unknown,
            None,
            &zuglo(),
        );
        assert!(far.distance_m > 1000, "got {} m", far.distance_m);
    }

    #[test]
    fn the_verdict_and_the_reason_arrive_untouched() {
        let place = full_row().into_place().expect("usable");
        let lead = place.into_lead(
            Verdict::DeadSite,
            "server answers 500".into(),
            Tls::Unknown,
            None,
            &zuglo(),
        );
        assert_eq!(lead.verdict, Verdict::DeadSite);
        assert_eq!(lead.reason, "server answers 500");
        assert_eq!(lead.name, "Borzas Barber");
        assert_eq!(lead.reviews, 12);
    }
}
