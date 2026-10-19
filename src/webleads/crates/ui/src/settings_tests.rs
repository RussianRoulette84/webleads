//! Tests for [`crate::settings`].

use super::*;

#[test]
fn every_setting_has_its_explanation() {
    // A setting that reaches the page without its sentence is a setting nobody can
    // judge.
    for setting in Setting::ALL {
        assert!(
            !setting.help().is_empty(),
            "{} has nothing to say for itself",
            setting.label()
        );
    }
}

#[test]
fn nothing_is_marked_as_changed_to_begin_with() {
    let ours = ScrapeOptions::default();
    assert_eq!(moved(&ours), 0);
    for setting in Setting::ALL {
        assert!(at_default(setting, &ours));
    }
}

#[test]
fn moving_one_marks_exactly_one() {
    let mut mine = ScrapeOptions::default();
    // Turning the zoom on at all is a change: by default it is left out of the
    // request entirely and Google chooses its own.
    mine.zoom = Some(12);
    assert_eq!(moved(&mine), 1);
    assert!(!at_default(Setting::Zoom, &mine));
    assert!(at_default(Setting::Depth, &mine));
}

#[test]
fn the_two_optional_ones_start_switched_off() {
    // Off means the setting is not sent at all, which is what a district or a town
    // wants — a fixed view either misses the edges or drifts into the next one.
    let ours = ScrapeOptions::default();
    assert_eq!(ours.zoom, None);
    assert_eq!(ours.depth, None);
}

#[test]
fn the_heading_says_how_many_are_changed() {
    assert_eq!(describe_moved(0), "Settings — all at their usual values");
    assert_eq!(describe_moved(1), "Settings — 1 changed");
    assert_eq!(describe_moved(3), "Settings — 3 changed");
}

#[test]
fn a_time_limit_reads_in_words() {
    assert_eq!(describe_limit(3_600), "60 minutes");
    assert_eq!(describe_limit(90), "90 seconds");
    assert_eq!(describe_limit(7_200), "2 hours");
}

#[test]
fn every_setting_is_filed_under_a_name_the_domain_knows() {
    // If a field name drifts, the explanation silently disappears rather than
    // failing — so the name is checked here.
    for setting in Setting::ALL {
        assert!(
            ScrapeOptions::help(setting.field()).is_some(),
            "{} is filed under a name nothing recognises",
            setting.label()
        );
    }
}

// ---- fast mode takes the zoom over ----

#[test]
fn turning_fast_mode_on_sets_the_zoom_to_twelve() {
    // It cannot run without a legal one: no zoom, or a zoom outside 1–21, and the
    // job is accepted, dies mid-run, and blocks the scraper behind it.
    assert_eq!(zoom_for_fast(true, None), Some(12));
    assert_eq!(zoom_for_fast(true, Some(16)), Some(12));
}

#[test]
fn turning_it_off_again_puts_the_zoom_back_as_it_was() {
    // Off if it was off — you never asked for a twelve.
    assert_eq!(zoom_for_fast(false, None), None);
    // And at whatever you had if you had one.
    assert_eq!(zoom_for_fast(false, Some(16)), Some(16));
}

#[test]
fn fast_mode_hides_both_switches_and_nothing_else() {
    for setting in Setting::ALL {
        let shown = switch_shown(setting, true);
        match setting {
            Setting::Zoom | Setting::Depth => assert!(!shown, "{setting:?} should hide"),
            _ => assert!(shown, "{setting:?} should stay"),
        }
    }
    // With fast mode off, everything is on show.
    for setting in Setting::ALL {
        assert!(switch_shown(setting, false), "{setting:?}");
    }
}

#[test]
fn what_fast_mode_forced_is_not_counted_as_something_you_moved() {
    // Switching one thing on must not claim you have changed two.
    let mut mine = ScrapeOptions::default();
    mine.fast_mode = true;
    mine.zoom = zoom_for_fast(true, None);
    assert_eq!(moved(&mine), 1, "only fast mode itself");
}

#[test]
fn the_depth_you_are_offered_never_reaches_the_empty_range() {
    // Fifteen returns 108 businesses; eighteen returns none. The slider stops at the
    // last one measured working.
    assert_eq!(USUAL_DEPTH, DEEPEST);
    assert_eq!(DEEPEST, 15);
}

#[test]
fn the_language_is_remembered_not_moved() {
    // Picking Hungarian is a choice the server keeps, not a setting to put back.
    let hungarian = ScrapeOptions {
        lang: "hu".to_owned(),
        ..ScrapeOptions::default()
    };
    assert_eq!(moved(&hungarian), 0);
}
