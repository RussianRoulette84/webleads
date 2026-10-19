//! Tests for [`crate::table`].

use super::*;

#[test]
fn near_enough_to_walk_is_said_in_metres() {
    assert_eq!(distance_words(0), "0 m");
    assert_eq!(distance_words(696), "696 m");
    assert_eq!(distance_words(999), "999 m");
}

#[test]
fn further_off_is_said_in_kilometres() {
    assert_eq!(distance_words(1_000), "1.0 km");
    assert_eq!(distance_words(1_507), "1.5 km");
    // The Rotterdam row that started all of this. "1157967 m" reads as a number
    // you skim past; "1158.0 km" reads as a problem.
    assert_eq!(distance_words(1_157_967), "1158.0 km");
}

#[test]
fn anything_across_the_country_is_flagged_rather_than_just_printed() {
    assert!(!is_suspiciously_far(696));
    assert!(!is_suspiciously_far(1_507));
    // Still plausibly Budapest.
    assert!(!is_suspiciously_far(19_999));
    assert!(is_suspiciously_far(20_000));
    assert!(is_suspiciously_far(1_157_967));
}

// ---- copying a whole row ----

use webleads_domain::{Lead, Tls, Verdict};

fn a_lead() -> Lead {
    Lead {
        id: "a".into(),
        name: "Borzas Barber".into(),
        phone: Some("+36 1 234 5001".into()),
        category: Some("Barber shop".into()),
        trade: Some("Barbers".into()),
        address: Some("Hősök tere 1, Budapest".into()),
        maps_url: Some("https://maps.google.com/?cid=1".into()),
        website: None,
        social_url: None,
        verdict: Verdict::NoSite,
        reason: "no link on Google Maps".into(),
        reviews: 41,
        rating: Some(4.6),
        distance_m: 696,
        position: Some((47.5149, 19.0778)),
        tls: Tls::Unknown,
        hours: Default::default(),
    }
}

#[test]
fn a_copied_row_holds_what_you_would_need_to_ring_them() {
    let lead = a_lead();
    let text = row_as_text(&lead, Some("08:00–17:00"), &lead.pitch(), "");

    for wanted in [
        "Borzas Barber",
        "+36 1 234 5001",
        "Hősök tere 1",
        "Open today: 08:00–17:00",
        "no website at all",
        "maps.google.com",
    ] {
        assert!(text.contains(wanted), "`{wanted}` is missing from:\n{text}");
    }
}

#[test]
fn the_name_comes_first_and_the_number_second() {
    // You paste this into a message and read it top to bottom.
    let lead = a_lead();
    let text = row_as_text(&lead, None, &lead.pitch(), "");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Borzas Barber");
    assert_eq!(lines[1], "+36 1 234 5001");
}

#[test]
fn a_business_with_no_hours_simply_leaves_that_line_out() {
    // Most of what we hold was scraped in fast mode, which sends no hours at all.
    let lead = a_lead();
    let text = row_as_text(&lead, None, &lead.pitch(), "");
    assert!(!text.contains("Open today"), "{text}");
    assert!(text.contains("Borzas Barber"));
}

#[test]
fn an_empty_hours_line_counts_as_no_hours() {
    let lead = a_lead();
    for blank in [Some(""), Some("   ")] {
        let text = row_as_text(&lead, blank, &lead.pitch(), "");
        assert!(!text.contains("Open today"), "{text}");
    }
}

#[test]
fn a_business_with_almost_nothing_still_copies_something_useful() {
    let bare = Lead {
        phone: None,
        address: None,
        maps_url: None,
        ..a_lead()
    };
    let text = row_as_text(&bare, None, &bare.pitch(), "");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "the name and why you are ringing:\n{text}");
    assert_eq!(lines[0], "Borzas Barber");
    assert!(lines[1].contains("no website at all"));
}

#[test]
fn the_reason_for_the_call_is_always_in_there() {
    // It is the whole point of the row.
    let lead = a_lead();
    let text = row_as_text(&lead, Some("09:00–17:00"), &lead.pitch(), "");
    assert!(text.contains(&lead.pitch()), "{text}");
}

#[test]
fn your_own_note_is_on_the_clipboard_too() {
    // It was the one part of the row the button left behind, and the part you wrote.
    let lead = a_lead();
    let text = row_as_text(
        &lead,
        None,
        &lead.pitch(),
        "  ask for the owner after five  ",
    );

    assert!(
        text.contains("Note: ask for the owner after five"),
        "the note is missing from:\n{text}"
    );
}

#[test]
fn a_row_with_no_note_gains_no_empty_line() {
    let lead = a_lead();
    let text = row_as_text(&lead, None, &lead.pitch(), "   ");

    assert!(!text.contains("Note:"));
    assert!(!text.contains("\n\n"), "a blank line crept in:\n{text}");
}

// ---- what the "why" column says, and what it leaves out ----

#[test]
fn a_business_with_no_website_does_not_say_so_twice() {
    // The tag already reads "no website". The reason underneath always read "no link
    // on Google Maps", which is the same fact and a line on every row.
    assert!(!worth_saying(
        webleads_domain::Verdict::NoSite,
        "no link on Google Maps"
    ));
}

#[test]
fn every_other_reason_carries_something_the_tag_cannot() {
    use webleads_domain::Verdict;
    assert!(worth_saying(Verdict::DeadSite, "server answers 404"));
    assert!(worth_saying(Verdict::SocialOnly, "only a Facebook page"));
    assert!(worth_saying(
        Verdict::InsecureSite,
        "certificate is not trusted"
    ));
}

#[test]
fn an_empty_reason_is_never_worth_a_line() {
    use webleads_domain::Verdict;
    for verdict in [Verdict::NoSite, Verdict::DeadSite, Verdict::SocialOnly] {
        assert!(!worth_saying(verdict, ""));
        assert!(!worth_saying(verdict, "   "));
    }
}

fn barbers() -> webleads_domain::Target {
    webleads_domain::Target {
        rank: 1,
        tier: webleads_domain::Tier::C,
        english: "Barbers".to_owned(),
        words: [("hu".to_owned(), vec!["borbély".to_owned()])]
            .into_iter()
            .collect(),
        no_site_odds: 4,
        budget: 4,
        note: String::new(),
    }
}

#[test]
fn a_row_in_an_english_city_reads_in_english() {
    assert_eq!(trade_name(&[barbers()], &a_lead(), Some("en")), "Barbers");
}

#[test]
fn a_row_in_a_hungarian_city_reads_in_hungarian() {
    assert_eq!(trade_name(&[barbers()], &a_lead(), Some("hu")), "borbély");
}

#[test]
fn a_row_in_no_city_reads_as_it_was_stored() {
    assert_eq!(trade_name(&[barbers()], &a_lead(), None), "Barbers");
}

#[test]
fn a_trade_we_do_not_list_reads_as_it_was_stored() {
    let lead = Lead {
        trade: Some("Falconers".into()),
        ..a_lead()
    };
    assert_eq!(trade_name(&[barbers()], &lead, Some("hu")), "Falconers");
}
