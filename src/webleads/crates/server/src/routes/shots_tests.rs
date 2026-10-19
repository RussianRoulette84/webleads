//! Tests for [`crate::routes::shots`] — which rows are worth a picture.

use super::*;
use webleads_domain::{Contact, Lead, Status, Tls};

fn a_lead(verdict: Verdict, website: Option<&str>) -> StoredLead {
    StoredLead {
        lead: Lead {
            id: "a".into(),
            name: "Borzas Barber".into(),
            phone: Some("+36 1 234 5001".into()),
            category: None,
            trade: None,
            hours: Default::default(),
            address: None,
            maps_url: None,
            website: website.map(str::to_owned),
            social_url: None,
            verdict,
            reason: "whatever".into(),
            reviews: 0,
            rating: None,
            distance_m: 100,
            position: None,
            tls: Tls::Unknown,
        },
        first_seen: "2026-09-01".into(),
        last_seen: "2026-09-19".into(),
        contact: Contact {
            status: Status::NeedCall,
            note: String::new(),
            called_on: None,
            not_reached_counter: 0,
            follow_up_tstamp: None,
        },
        call_count: 0,
        hours_today: None,
        hours_there: None,
        city_lang: None,
    }
}

#[test]
fn a_broken_site_is_worth_photographing() {
    // The picture is the argument on the telephone.
    let lead = a_lead(Verdict::DeadSite, Some("https://example.com"));
    assert_eq!(
        worth_photographing(&lead).as_deref(),
        Some("https://example.com")
    );
}

#[test]
fn a_site_with_a_broken_certificate_is_too() {
    let lead = a_lead(Verdict::InsecureSite, Some("https://example.com"));
    assert!(worth_photographing(&lead).is_some());
}

#[test]
fn a_business_with_no_website_has_nothing_to_photograph() {
    // Which is the whole reason it is a lead.
    let lead = a_lead(Verdict::NoSite, None);
    assert_eq!(worth_photographing(&lead), None);
}

#[test]
fn a_social_page_is_not_theirs_to_show() {
    // Photographing somebody's Facebook page proves nothing about their website.
    let lead = a_lead(Verdict::SocialOnly, Some("https://facebook.com/shop"));
    assert_eq!(worth_photographing(&lead), None);
}

#[test]
fn a_blank_address_is_not_an_address() {
    let lead = a_lead(Verdict::DeadSite, Some("   "));
    assert_eq!(worth_photographing(&lead), None);
}
