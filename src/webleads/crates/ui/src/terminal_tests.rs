//! Tests for [`crate::terminal`].

use super::*;

#[test]
fn a_request_reads_as_the_command_that_would_have_made_it() {
    assert_eq!(
        as_command("GET /api/leads?limit=50"),
        "curl -s /api/leads?limit=50"
    );
    assert_eq!(as_command("POST /api/runs"), "curl -s -XPOST /api/runs");
    assert_eq!(
        as_command("DELETE /api/hidden/3"),
        "curl -s -XDELETE /api/hidden/3"
    );
}

#[test]
fn anything_that_is_not_a_request_is_left_as_it_was_written() {
    assert_eq!(as_command("taking a picture"), "taking a picture");
    assert_eq!(as_command(""), "");
}

#[test]
fn a_log_line_is_coloured_by_what_it_says_about_itself() {
    assert_eq!(
        tone_of("2026-09-20T13:26:14Z  INFO scraper: run starting run_id=6"),
        Tone::Plain
    );
    assert_eq!(
        tone_of("2026-09-20T13:26:14Z  WARN scraper: ⚠️ WARNING: `x` failed"),
        Tone::Warn
    );
    assert_eq!(
        tone_of("2026-09-20T13:26:14Z ERROR frontend: ‼️ ERROR: it broke"),
        Tone::Bad
    );
    assert_eq!(
        tone_of("2026-09-20T13:26:14Z DEBUG request{method=GET}: started"),
        Tone::Dim
    );
}

#[test]
fn a_line_that_says_nothing_about_itself_is_said_quietly() {
    // Better a quiet line than a shout: the loud colours have to mean something.
    assert_eq!(tone_of("just some output"), Tone::Dim);
    assert_eq!(tone_of(""), Tone::Dim);
}

#[test]
fn every_tone_draws_with_a_class_the_stylesheet_already_has() {
    for tone in [
        Tone::Plain,
        Tone::Note,
        Tone::Good,
        Tone::Warn,
        Tone::Bad,
        Tone::Dim,
        Tone::Command,
    ] {
        let class = tone.class();
        assert!(class.starts_with("terminal__line"), "{class}");
    }
    // And no two of the coloured ones look the same, or the colour says nothing.
    assert_ne!(Tone::Warn.class(), Tone::Bad.class());
    assert_ne!(Tone::Good.class(), Tone::Plain.class());
}

#[test]
fn saying_a_line_with_no_window_open_is_not_a_reason_to_fall_over() {
    // Outside a running page there is no context, and a request that logs itself
    // must not take the caller down with it.
    assert!(session().is_none());
    say("nobody is listening", Tone::Plain);
    typed("curl -s /api/leads");
}

#[test]
fn the_window_follows_the_newest_line_while_you_are_at_the_bottom() {
    // 600 tall, showing 340 of it, scrolled to the end.
    assert!(near_the_bottom(260, 340, 600));
    // A couple of pixels out is still the bottom — browsers land untidily.
    assert!(near_the_bottom(250, 340, 600));
}

#[test]
fn it_stops_following_when_you_scroll_up_to_read_something() {
    // Being yanked back to the bottom mid-sentence is the worst thing a log window
    // can do to you.
    assert!(!near_the_bottom(0, 340, 6_000));
    assert!(!near_the_bottom(2_000, 340, 6_000));
}

#[test]
fn a_window_with_nothing_in_it_counts_as_at_the_bottom() {
    // Otherwise the very first line would arrive out of sight.
    assert!(near_the_bottom(0, 340, 0));
    assert!(near_the_bottom(0, 340, 340));
}

#[test]
fn a_short_answer_goes_into_the_window_whole() {
    assert_eq!(snip(r#"{"ok":true}"#), r#"{"ok":true}"#);
    assert_eq!(snip("  spaced  "), "spaced");
    assert_eq!(snip(""), "");
}

#[test]
fn a_long_answer_is_cut_and_says_how_much_was_left_out() {
    // The list of leads is a quarter of a megabyte; whole, it would be the window.
    let long = "x".repeat(BODY_SHOWN + 40);
    let cut = snip(&long);
    assert!(cut.starts_with(&"x".repeat(BODY_SHOWN)));
    assert!(cut.ends_with(&format!("(of {} characters)", BODY_SHOWN + 40)));
}

#[test]
fn cutting_an_answer_never_splits_a_letter_in_half() {
    // Hungarian business names are full of these. Cutting by bytes would hand the
    // window something it cannot draw.
    let long = "ő".repeat(BODY_SHOWN + 10);
    assert_eq!(
        snip(&long).chars().filter(|c| *c == 'ő').count(),
        BODY_SHOWN
    );
}

#[test]
fn the_title_names_the_real_machine() {
    let machine = Machine {
        user: "root".to_owned(),
        host: "workstation".to_owned(),
        dir: "~/Projects/webleads".to_owned(),
    };
    assert_eq!(
        title_said(Some(&machine)),
        "root@workstation ◆ ~/Projects/webleads"
    );
}

#[test]
fn before_the_server_has_said_the_title_invents_nothing() {
    assert_eq!(title_said(None), "connecting…");
}
