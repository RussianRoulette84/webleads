//! Tests for [`crate::lead`].

use super::*;

#[test]
fn google_id_wins_when_there_is_one() {
    let id = Lead::identity(Some("0x47c4:0xdead"), "Fodrászat", 47.5, 19.1);
    assert_eq!(id, "0x47c4:0xdead");
}

#[test]
fn without_an_id_the_name_and_place_stand_in() {
    // Fast mode strips the id, so this path is the one that actually runs then.
    let a = Lead::identity(None, "Fodrászat", 47.514900, 19.077800);
    let b = Lead::identity(Some("  "), "  fodrászat ", 47.51491, 19.07781);
    assert_eq!(a, b, "the same shop reported twice must collapse to one");
}

#[test]
fn two_shops_at_one_address_stay_two() {
    let a = Lead::identity(None, "Fodrászat", 47.514900, 19.077800);
    let b = Lead::identity(None, "Petra'z Hair", 47.514900, 19.077800);
    assert_ne!(a, b);
}

#[test]
fn the_quiet_and_the_close_come_first() {
    let mut keys = [(true, 41_u32, 100_u32), (false, 0, 900), (true, 2, 700)];
    keys.sort();
    assert_eq!(
        keys[0],
        (false, 0, 900),
        "no reviews at all goes to the top"
    );
}

#[test]
fn only_three_verdicts_are_worth_calling() {
    assert!(Verdict::NoSite.is_lead());
    assert!(Verdict::SocialOnly.is_lead());
    assert!(Verdict::DeadSite.is_lead());
    assert!(!Verdict::LiveSite.is_lead());
}

/// A lead with nothing filled in but a phone number, for the dialling tests.
fn with_phone(phone: Option<&str>) -> Lead {
    Lead {
        id: "x".into(),
        name: "Shop".into(),
        phone: phone.map(str::to_owned),
        category: None,
        trade: None,
        hours: Default::default(),
        address: None,
        maps_url: None,
        website: None,
        social_url: None,
        verdict: Verdict::NoSite,
        reason: String::new(),
        reviews: 0,
        rating: None,
        distance_m: 0,
        position: Some((47.5149, 19.0778)),
        tls: Tls::Unknown,
    }
}

#[test]
fn every_verdict_has_a_word_for_a_person() {
    assert_eq!(Verdict::NoSite.describe(), "no website");
    assert_eq!(Verdict::SocialOnly.describe(), "social only");
    assert_eq!(Verdict::DeadSite.describe(), "broken site");
    assert_eq!(Verdict::LiveSite.describe(), "has a site");
}

#[test]
fn the_stored_word_and_the_shown_word_are_not_the_same_thing() {
    // If these ever collapse into one function, a reworded label would silently
    // change what is written to the database. This test is the reminder.
    for verdict in [
        Verdict::NoSite,
        Verdict::SocialOnly,
        Verdict::DeadSite,
        Verdict::LiveSite,
    ] {
        assert_ne!(verdict.as_str(), verdict.describe());
    }
}

#[test]
fn a_verdict_survives_being_written_down_and_read_back() {
    for verdict in [
        Verdict::NoSite,
        Verdict::SocialOnly,
        Verdict::DeadSite,
        Verdict::LiveSite,
    ] {
        let written = verdict.as_str();
        let read: Verdict = written.parse().expect("reads back");
        assert_eq!(read, verdict);
    }
}

#[test]
fn a_word_that_is_not_a_verdict_is_refused_and_says_so() {
    let error = "banana".parse::<Verdict>().expect_err("refused");
    assert!(error.to_string().contains("banana"));
    assert!("".parse::<Verdict>().is_err());
    // The shown word must not be accepted as the stored one.
    assert!("no website".parse::<Verdict>().is_err());
}

#[test]
fn a_telephone_number_loses_everything_a_dialler_cannot_use() {
    assert_eq!(
        with_phone(Some("+36 1 234 5001")).dial_number().as_deref(),
        Some("+3612345001")
    );
    assert_eq!(
        with_phone(Some("(061)2345001")).dial_number().as_deref(),
        Some("0612345001")
    );
    assert_eq!(
        with_phone(Some("06-20-555-0101")).dial_number().as_deref(),
        Some("06205550101")
    );
}

#[test]
fn the_plus_is_kept_only_when_it_was_at_the_front() {
    assert_eq!(
        with_phone(Some("  +3612345001")).dial_number().as_deref(),
        Some("+3612345001")
    );
    // A plus in the middle is a separator, not a country code.
    assert_eq!(
        with_phone(Some("061 234+5001")).dial_number().as_deref(),
        Some("0612345001")
    );
}

#[test]
fn no_number_and_a_number_with_no_digits_both_give_nothing() {
    assert_eq!(with_phone(None).dial_number(), None);
    // An empty tel: link looks like it should work and does not. Worse than none.
    assert_eq!(with_phone(Some("")).dial_number(), None);
    assert_eq!(with_phone(Some("   ")).dial_number(), None);
    assert_eq!(with_phone(Some("call us!")).dial_number(), None);
}

#[test]
fn the_same_shop_written_three_ways_becomes_one_number() {
    // This is the whole point: these are the same Budapest landline.
    let forms = [
        "+36 1 234 5001",
        "0036 1 234 5001",
        "06 1 234 5001",
        "(061)2345001",
    ];
    let canonical: Vec<_> = forms
        .iter()
        .filter_map(|f| normalise_phone(f, "hu"))
        .collect();
    assert_eq!(canonical.len(), 4);
    assert!(
        canonical.iter().all(|c| c == "+3612345001"),
        "got {canonical:?}"
    );
}

#[test]
fn a_mobile_number_normalises_the_same_way() {
    assert_eq!(
        normalise_phone("06 20 555 0101", "hu").as_deref(),
        Some("+36205550101")
    );
    assert_eq!(
        normalise_phone("+36-30-555-0102", "hu").as_deref(),
        Some("+36305550102")
    );
}

#[test]
fn zero_zero_three_six_is_read_before_zero_six() {
    // If `06` were tried first, `0036…` would lose its country code and become
    // `+3636…`. The order of those two checks is the whole test.
    assert_eq!(
        normalise_phone("0036202223333", "hu").as_deref(),
        Some("+36202223333")
    );
}

#[test]
fn a_number_written_with_a_plus_keeps_the_country_it_came_with() {
    // A Dutch number, seen while working Hungary. Read without its plus it would be
    // `31101234567`, and the same shop written `+31 10 …` and `0031 10 …` would stop
    // matching itself.
    assert_eq!(
        normalise_phone("+31 10 123 4567", "hu").as_deref(),
        Some("+31101234567")
    );
    // And our own, which is the same rule and not a special case.
    assert_eq!(
        normalise_phone("+36 1 234 5001", "hu").as_deref(),
        Some("+3612345001")
    );
}

#[test]
fn a_number_with_no_prefix_we_know_keeps_its_digits_untouched() {
    // Inventing a country code in front of it would be worse than leaving it alone,
    // because it would read as a shop in a country it is not in.
    assert_eq!(
        normalise_phone("555 1234", "hu").as_deref(),
        Some("5551234")
    );
}

#[test]
fn the_same_digits_mean_different_countries() {
    // `06` is the Hungarian trunk prefix, and in the UAE it is Sharjah — so a Sharjah
    // landline must never be filed as Hungarian.
    assert_eq!(
        normalise_phone("06 561 2345", "hu").as_deref(),
        Some("+365612345"),
        "in Hungary the 06 comes off"
    );
    assert_eq!(
        normalise_phone("06 561 2345", "ae").as_deref(),
        Some("+97165612345"),
        "in the UAE only the nought comes off, and 6 is Sharjah"
    );
}

#[test]
fn a_dubai_number_written_locally_gets_its_own_country_code() {
    assert_eq!(
        normalise_phone("04 123 4567", "ae").as_deref(),
        Some("+97141234567")
    );
    assert_eq!(
        normalise_phone("00971 4 123 4567", "ae").as_deref(),
        Some("+97141234567")
    );
    // Written both ways, it is one number — which is the whole job.
    assert_eq!(
        normalise_phone("04 123 4567", "ae"),
        normalise_phone("+971 4 123 4567", "ae")
    );
}

#[test]
fn a_country_we_have_never_heard_of_leaves_a_number_alone() {
    // Better untidy than filed under rules we made up for it.
    assert_eq!(
        normalise_phone("06 1 234 5001", "zz").as_deref(),
        Some("0612345001")
    );
    assert_eq!(
        normalise_phone("06 1 234 5001", "").as_deref(),
        Some("0612345001")
    );
}

#[test]
fn a_prefix_with_no_number_after_it_is_not_a_number() {
    assert_eq!(normalise_phone("06", "hu"), None);
    assert_eq!(normalise_phone("0036", "hu"), None);
    // A country code and nothing else, ours or anybody's.
    assert_eq!(normalise_phone("+36", "hu"), None);
    assert_eq!(normalise_phone("+971", "ae"), None);
    assert_eq!(normalise_phone("+1", "hu"), None);
}

#[test]
fn nothing_in_gives_nothing_out() {
    assert_eq!(normalise_phone("", "hu"), None);
    assert_eq!(normalise_phone("   ", "hu"), None);
    assert_eq!(normalise_phone("call us", "hu"), None);
}

#[test]
fn a_lead_hands_back_its_own_canonical_number() {
    assert_eq!(
        with_phone(Some("06 1 234 5001"))
            .phone_canonical("hu")
            .as_deref(),
        Some("+3612345001")
    );
    assert_eq!(with_phone(None).phone_canonical("hu"), None);
}

#[test]
fn dialling_and_canonical_are_different_jobs() {
    // `dial_number` keeps what a telephone needs from what Google wrote;
    // `phone_canonical` produces one shape so duplicates can be spotted. On a
    // number written the local way they legitimately differ.
    let lead = with_phone(Some("06 1 234 5001"));
    assert_eq!(lead.dial_number().as_deref(), Some("0612345001"));
    assert_eq!(lead.phone_canonical("hu").as_deref(), Some("+3612345001"));
}

#[test]
fn a_broken_certificate_puts_a_business_on_the_call_list() {
    // The whole reason this verdict exists: their site works, so it would be easy to
    // drop them — but every visitor sees a security warning first.
    assert!(Verdict::InsecureSite.is_lead());
    assert_eq!(Verdict::InsecureSite.describe(), "certificate broken");
    assert_eq!(Verdict::InsecureSite.as_str(), "insecure_site");
    assert_eq!(
        "insecure_site".parse::<Verdict>().expect("reads"),
        Verdict::InsecureSite
    );
}

#[test]
fn a_certificate_is_only_a_problem_when_it_is_broken() {
    assert!(!Tls::Unknown.is_a_problem());
    assert!(!Tls::Valid.is_a_problem());
    assert!(Tls::Broken("expired in 2019".into()).is_a_problem());
}

#[test]
fn the_certificate_column_says_nothing_when_there_is_nothing_to_say() {
    assert_eq!(Tls::Unknown.describe(), "");
    assert_eq!(Tls::Valid.describe(), "");
    // And when there is, it is the sentence you say on the telephone.
    assert_eq!(
        Tls::Broken("expired in 2019".into()).describe(),
        "expired in 2019"
    );
}

#[test]
fn the_trade_we_searched_for_beats_the_one_google_guessed() {
    let mut lead = with_phone(None);
    lead.category = Some("Establishment".into());
    lead.trade = Some("Barbers".into());
    assert_eq!(lead.shown_trade(), Some("Barbers"));
}

#[test]
fn googles_category_is_used_when_we_have_nothing_of_our_own() {
    // Everything imported before we did our own searching is in this state.
    let mut lead = with_phone(None);
    lead.category = Some("Hair salon".into());
    lead.trade = None;
    assert_eq!(lead.shown_trade(), Some("Hair salon"));
}

#[test]
fn a_blank_is_better_than_a_guess() {
    // Fast mode strips the category, and nothing we searched can be invented
    // afterwards. A wrong trade would send you into a call with the wrong
    // opening line, which is worse than an empty column.
    let mut lead = with_phone(None);
    lead.category = None;
    lead.trade = None;
    assert_eq!(lead.shown_trade(), None);
}

#[test]
fn an_empty_string_counts_as_nothing_at_all() {
    let mut lead = with_phone(None);
    lead.trade = Some("  ".into());
    lead.category = Some("Hair salon".into());
    assert_eq!(lead.shown_trade(), Some("Hair salon"));

    lead.category = Some(String::new());
    assert_eq!(lead.shown_trade(), None);
}

#[test]
fn the_commonest_lead_wears_the_colour_that_stands_out() {
    // Most of the list has no website at all, and that is the one you are hunting.
    assert_eq!(Verdict::NoSite.tone(), "accent");
}

#[test]
fn the_two_certificate_shaped_problems_share_a_colour() {
    // They are the same conversation on the telephone.
    assert_eq!(Verdict::SocialOnly.tone(), Verdict::InsecureSite.tone());
}

#[test]
fn a_working_site_is_the_quiet_colour() {
    assert_eq!(Verdict::LiveSite.tone(), "dim");
}
