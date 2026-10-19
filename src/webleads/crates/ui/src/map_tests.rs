//! Tests for [`crate::map`].

use super::*;

#[test]
fn the_commonest_lead_gets_the_colour_that_stands_out() {
    // Most of the list has no website at all, and that is the one you are hunting.
    assert_eq!(colour_for(Verdict::NoSite), "accent");
}

#[test]
fn the_two_certificate_shaped_problems_share_a_colour() {
    // They are the same conversation on the telephone — "people cannot find you" —
    // and a map with five colours on it is a map nobody reads.
    assert_eq!(
        colour_for(Verdict::SocialOnly),
        colour_for(Verdict::InsecureSite)
    );
}

#[test]
fn a_broken_site_is_told_apart_from_the_rest() {
    assert_ne!(colour_for(Verdict::DeadSite), colour_for(Verdict::NoSite));
    assert_ne!(
        colour_for(Verdict::DeadSite),
        colour_for(Verdict::SocialOnly)
    );
}

#[test]
fn there_are_only_three_colours_among_the_leads() {
    // Four verdicts, three colours. Deliberate.
    let mut colours: Vec<&str> = [
        Verdict::NoSite,
        Verdict::SocialOnly,
        Verdict::DeadSite,
        Verdict::InsecureSite,
    ]
    .into_iter()
    .map(colour_for)
    .collect();
    colours.sort_unstable();
    colours.dedup();
    assert_eq!(colours.len(), 3);
}

#[test]
fn a_working_website_is_never_given_a_lead_colour() {
    // It is not a lead and is never pinned, but if it ever were it should not look
    // like one.
    let live = colour_for(Verdict::LiveSite);
    for lead in [Verdict::NoSite, Verdict::SocialOnly, Verdict::DeadSite] {
        assert_ne!(live, colour_for(lead));
    }
}

#[test]
fn the_colour_is_chosen_from_the_word_the_server_sends() {
    // The pins arrive as JSON, so the verdict is a string by the time it gets here.
    assert_eq!(tone_for("no_site"), colour_for(Verdict::NoSite));
    assert_eq!(tone_for("dead_site"), colour_for(Verdict::DeadSite));
}

#[test]
fn a_word_we_do_not_know_still_gets_a_pin() {
    // A colour we cannot choose is no reason to lose a business with a telephone.
    assert_eq!(tone_for("something_new"), "dim");
    assert_eq!(tone_for(""), "dim");
}
