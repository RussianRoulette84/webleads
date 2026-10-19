//! Tests for [`crate::page`] — the file you can open with the server switched off.

use super::*;
use webleads_domain::{Contact, Lead, Status, Tls, Verdict};

fn a_lead(name: &str, verdict: Verdict) -> StoredLead {
    StoredLead {
        lead: Lead {
            id: "a".into(),
            name: name.into(),
            phone: Some("06 30 555 0104".into()),
            category: Some("Barber shop".into()),
            trade: Some("Barbers".into()),
            hours: Default::default(),
            address: Some("Hősök tere 1".into()),
            maps_url: Some("https://maps.google.com/?cid=1".into()),
            website: None,
            social_url: None,
            verdict,
            reason: "no link on Google Maps".into(),
            reviews: 0,
            rating: None,
            distance_m: 240,
            position: Some((47.5149, 19.0778)),
            tls: Tls::Unknown,
        },
        first_seen: "2026-09-01".into(),
        last_seen: "2026-09-19".into(),
        contact: Contact {
            status: Status::NeedCall,
            note: "ask for the owner".into(),
            called_on: None,
            not_reached_counter: 0,
            follow_up_tstamp: None,
        },
        call_count: 0,
        hours_today: Some("9:00–17:00".into()),
        hours_there: None,
        city_lang: None,
    }
}

fn a_heading() -> Heading {
    Heading {
        area: "Budapest, Zugló".into(),
        centre: (47.5149, 19.0778),
        made_on: "2026-09-19".into(),
    }
}

#[test]
fn the_page_carries_every_lead_inside_it() {
    // The whole promise: no server, no requests, everything already in the file.
    let page = build(&[a_lead("Borzas Barber", Verdict::NoSite)], &a_heading());
    assert!(page.contains("Borzas Barber"));
    assert!(page.contains("application/json"));
    assert!(!page.contains("/api/"), "it must not call back to us");
}

#[test]
fn a_business_named_like_a_script_tag_cannot_break_the_page() {
    // Somebody's name is somebody else's text, and this one ends a script tag early
    // unless the slash is escaped. The rest of the page would then be shown as text.
    let page = build(
        &[a_lead("</script> Barbers", Verdict::NoSite)],
        &a_heading(),
    );
    assert!(!page.contains("</script> Barbers"));
    assert!(page.contains("<\\/script> Barbers"));
}

#[test]
fn the_page_carries_what_to_say_on_the_telephone() {
    // It is the line you actually read aloud.
    let page = build(&[a_lead("Borzas Barber", Verdict::NoSite)], &a_heading());
    assert!(page.contains("\"pitch\""));
}

#[test]
fn your_own_notes_come_with_it() {
    let page = build(&[a_lead("Borzas Barber", Verdict::NoSite)], &a_heading());
    assert!(page.contains("ask for the owner"));
}

#[test]
fn the_map_is_in_there_with_its_pins() {
    let page = build(&[a_lead("Borzas Barber", Verdict::NoSite)], &a_heading());
    assert!(page.contains("webleads-map"));
    // The same drawing code as the live page, not a lookalike.
    assert!(page.contains("webleadsMapPins"));
    // And the pin's own mark.
    assert!(page.contains("&#10086;"));
}

#[test]
fn the_page_wears_our_own_colours() {
    let page = build(&[a_lead("Borzas Barber", Verdict::NoSite)], &a_heading());
    assert!(page.contains("--wl-ink"), "the stylesheet is missing");
}

#[test]
fn an_empty_list_still_makes_a_page() {
    let page = build(&[], &a_heading());
    assert!(page.contains("<table"));
    // The count on the left is filled in by the script; the total is written here.
    assert!(page.contains("of 0 shown"));
}

#[test]
fn the_area_is_escaped_before_it_reaches_the_title() {
    let mut heading = a_heading();
    heading.area = "<script>alert(1)</script>".into();
    let page = build(&[], &heading);
    assert!(!page.contains("<script>alert(1)"));
    assert!(page.contains("&lt;script&gt;"));
}

#[test]
fn a_business_with_no_position_still_appears_in_the_list() {
    // It simply is not pinned. Dropping it from the table would lose a lead.
    let mut lead = a_lead("Nowhere Barbers", Verdict::NoSite);
    lead.lead.position = None;
    let page = build(&[lead], &a_heading());
    assert!(page.contains("Nowhere Barbers"));
    assert!(page.contains("\"lat\":null"));
}

#[test]
fn the_reason_a_row_wears_is_the_one_the_map_uses() {
    // One rule, in `domain`, so the table's tag and the pin cannot disagree.
    let row = row_for(&a_lead("Borzas Barber", Verdict::DeadSite));
    assert_eq!(row.tone, Verdict::DeadSite.tone());
}
