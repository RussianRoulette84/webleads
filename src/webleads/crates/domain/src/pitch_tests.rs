//! Tests for [`crate::pitch`].

use super::*;
use crate::{Lead, Tls, Verdict};

/// A barber with 41 reviews and no website — the commonest lead there is.
fn a_barber() -> Lead {
    Lead {
        id: "a".into(),
        name: "Borzas Barber".into(),
        phone: Some("+36 1 234 5001".into()),
        category: Some("Barber shop".into()),
        trade: Some("Barbers".into()),
        hours: Default::default(),
        address: Some("Hősök tere 1".into()),
        maps_url: None,
        website: None,
        social_url: None,
        verdict: Verdict::NoSite,
        reason: "no link on Google Maps".into(),
        reviews: 41,
        rating: Some(4.6),
        distance_m: 696,
        position: Some((47.5149, 19.0778)),
        tls: Tls::Unknown,
    }
}

#[test]
fn the_commonest_lead_reads_the_way_it_should() {
    assert_eq!(
        a_barber().pitch(),
        "Barbers · 41 reviews, 4.6★ · no website at all"
    );
}

#[test]
fn the_reason_for_the_call_always_comes_last() {
    // It is what you say last, so it sits last.
    let pitch = a_barber().pitch();
    assert!(pitch.ends_with("no website at all"), "{pitch}");
}

#[test]
fn a_business_nobody_has_reviewed_is_not_called_nought_reviews() {
    // Saying "0 reviews" out loud would be rude, and it is not the point — being
    // quiet is already why they sort to the top of the list.
    let mut quiet = a_barber();
    quiet.reviews = 0;
    quiet.rating = None;
    assert_eq!(quiet.pitch(), "Barbers · no website at all");
    assert!(!quiet.pitch().contains('0'), "{}", quiet.pitch());
}

#[test]
fn one_review_is_not_called_one_reviews() {
    let mut barely = a_barber();
    barely.reviews = 1;
    assert!(barely.pitch().contains("1 review,"), "{}", barely.pitch());
    assert!(!barely.pitch().contains("1 reviews"), "{}", barely.pitch());
}

#[test]
fn reviews_without_a_rating_still_say_how_many() {
    let mut unrated = a_barber();
    unrated.rating = None;
    assert_eq!(unrated.pitch(), "Barbers · 41 reviews · no website at all");
}

#[test]
fn the_trade_we_searched_for_is_the_one_that_is_said() {
    let mut lead = a_barber();
    lead.trade = Some("Plumbers".into());
    lead.category = Some("Establishment".into());
    assert!(lead.pitch().starts_with("Plumbers ·"), "{}", lead.pitch());
}

#[test]
fn a_business_with_no_trade_simply_leaves_it_out() {
    // Everything imported before we did our own searching is in this state.
    let mut unknown = a_barber();
    unknown.trade = None;
    unknown.category = None;
    assert_eq!(unknown.pitch(), "41 reviews, 4.6★ · no website at all");
}

// ---- why they qualified ----

#[test]
fn a_facebook_only_business_is_told_which_network() {
    // "only a Facebook page" is worth saying; "only a social page" is not.
    let mut social = a_barber();
    social.verdict = Verdict::SocialOnly;
    social.social_url = Some("https://facebook.com/borzas".into());
    assert!(
        social.pitch().ends_with("only a Facebook page"),
        "{}",
        social.pitch()
    );
}

#[test]
fn each_network_is_named_by_its_own_name() {
    for (url, expected) in [
        ("https://instagram.com/x", "only an Instagram page"),
        ("https://tiktok.com/@x", "only a TikTok page"),
        ("https://linkedin.com/x", "only a LinkedIn page"),
        ("https://linktr.ee/x", "only a Linktree page"),
    ] {
        let mut social = a_barber();
        social.verdict = Verdict::SocialOnly;
        social.social_url = Some(url.into());
        assert!(social.pitch().ends_with(expected), "{}", social.pitch());
    }
}

#[test]
fn a_network_we_do_not_recognise_is_still_said_generally() {
    let mut social = a_barber();
    social.verdict = Verdict::SocialOnly;
    social.social_url = Some("https://somewhere-new.com/x".into());
    assert!(
        social.pitch().ends_with("only a social page"),
        "{}",
        social.pitch()
    );
}

#[test]
fn a_broken_site_says_what_kind_of_broken() {
    for (reason, expected) in [
        ("parked or placeholder page", "website is a parked page"),
        ("timed out", "website does not load"),
        ("page is effectively empty", "website is empty"),
        ("server answers 500", "website is down"),
    ] {
        let mut dead = a_barber();
        dead.verdict = Verdict::DeadSite;
        dead.reason = reason.into();
        assert!(
            dead.pitch().ends_with(expected),
            "`{reason}` gave {}",
            dead.pitch()
        );
    }
}

#[test]
fn a_broken_certificate_is_the_sharpest_thing_we_can_offer() {
    let mut insecure = a_barber();
    insecure.verdict = Verdict::InsecureSite;
    insecure.tls = Tls::Broken("certificate has expired (verify failed)".into());
    assert!(
        insecure
            .pitch()
            .ends_with("certificate expired — visitors see a security warning"),
        "{}",
        insecure.pitch()
    );
}

#[test]
fn a_self_signed_certificate_says_so_rather_than_just_broken() {
    let mut insecure = a_barber();
    insecure.verdict = Verdict::InsecureSite;
    insecure.tls = Tls::Broken("certificate is self-signed (verify failed)".into());
    assert!(
        insecure.pitch().contains("self-signed"),
        "{}",
        insecure.pitch()
    );
}

#[test]
fn a_certificate_we_cannot_name_still_says_visitors_see_a_warning() {
    // OpenSSL gives one generic message for most failures, so this is the common case.
    let mut insecure = a_barber();
    insecure.verdict = Verdict::InsecureSite;
    insecure.tls = Tls::Broken("certificate is not trusted (verify failed)".into());
    assert!(
        insecure
            .pitch()
            .ends_with("certificate is broken — visitors see a security warning"),
        "{}",
        insecure.pitch()
    );
}

#[test]
fn a_certificate_problem_outranks_whatever_else_we_found() {
    // It is the most specific thing we can offer, so it wins wherever we found one.
    let mut both = a_barber();
    both.verdict = Verdict::DeadSite;
    both.reason = "server answers 500".into();
    both.tls = Tls::Broken("certificate has expired".into());
    assert!(
        both.pitch().contains("certificate expired"),
        "{}",
        both.pitch()
    );
}

#[test]
fn a_good_certificate_does_not_get_mentioned_at_all() {
    let mut fine = a_barber();
    fine.tls = Tls::Valid;
    assert_eq!(
        fine.pitch(),
        "Barbers · 41 reviews, 4.6★ · no website at all"
    );
}

#[test]
fn a_business_with_nothing_known_still_says_something() {
    // Every lead qualified for *some* reason, so there is always a last part.
    let bare = Lead {
        trade: None,
        category: None,
        reviews: 0,
        rating: None,
        social_url: None,
        tls: Tls::Unknown,
        ..a_barber()
    };
    let pitch = bare.pitch();
    assert!(!pitch.is_empty());
    assert_eq!(pitch, "no website at all");
}

#[test]
fn the_parts_are_separated_so_they_read_as_pauses() {
    let pitch = a_barber().pitch();
    assert_eq!(pitch.matches(" · ").count(), 2, "{pitch}");
}

#[test]
fn the_grammar_is_right_for_every_network_we_name() {
    // "a Instagram page" is the sort of thing that makes a tool feel unfinished.
    assert_eq!(article_for("Instagram"), "an");
    assert_eq!(article_for("Facebook"), "a");
    assert_eq!(article_for("TikTok"), "a");
    assert_eq!(article_for("LinkedIn"), "a");
    assert_eq!(article_for("Linktree"), "a");
}
