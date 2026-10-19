//! Tests for [`crate::searches`].

use super::*;

fn a_run(state: &str, note: Option<&str>) -> api::Run {
    api::Run {
        id: 12,
        state: state.to_owned(),
        source: "Zugló".into(),
        note: note.map(str::to_owned),
        words_total: 27,
        words_done: 27,
        found_total: 956,
        now_searching: None,
        area: Some("Zugló".into()),
        city: None,
        finished_at: None,
        started_at: "2026-09-19".into(),
        leads_found: 74,
    }
}

#[test]
fn a_finished_search_says_so_quietly() {
    let (tone, said) = describe_end(&a_run("done", None));
    assert_eq!(tone, "is-ok");
    assert!(said.contains("finished"), "{said}");
}

#[test]
fn a_failed_search_says_why_rather_than_showing_a_nought() {
    // The three failures sitting in the database would otherwise read as searches
    // that worked and found nothing at all.
    let (tone, said) = describe_end(&a_run("failed", Some("the scraper is not running")));
    assert_eq!(tone, "is-bad");
    assert!(said.contains("the scraper is not running"), "{said}");
}

#[test]
fn a_failure_with_no_reason_recorded_still_reads_as_a_failure() {
    let (tone, said) = describe_end(&a_run("failed", None));
    assert_eq!(tone, "is-bad");
    assert_eq!(said, "failed");
}

#[test]
fn one_you_stopped_is_told_apart_from_one_that_broke() {
    let mut mine = a_run("stopped", None);
    mine.words_done = 9;
    let (tone, said) = describe_end(&mine);
    assert_eq!(tone, "is-warn");
    assert!(said.contains("you stopped it"), "{said}");
    assert!(said.contains("9 of 27"), "{said}");
}

#[test]
fn one_still_going_names_the_word_it_is_on() {
    let mut going = a_run("working", None);
    going.now_searching = Some("borbély Zugló".into());
    going.words_done = 4;
    let (tone, said) = describe_end(&going);
    assert_eq!(tone, "is-accent");
    assert!(said.contains("borbély Zugló"), "{said}");
    assert!(said.contains("4 of 27"), "{said}");
}

#[test]
fn one_still_waiting_says_how_much_work_it_is() {
    let (tone, said) = describe_end(&a_run("queued", None));
    assert_eq!(tone, "is-dim");
    assert!(said.contains("waiting"), "{said}");
    assert!(said.contains("27 words"), "{said}");
}

// ---- the strip ----

#[test]
fn a_chip_says_where_and_when_in_the_room_it_has() {
    let run = a_run("done", None);
    assert_eq!(short_name(&run), "Zugló · 2026-09-19");
}

#[test]
fn a_search_with_no_area_falls_back_to_what_it_read() {
    let mut run = a_run("done", None);
    run.area = None;
    assert_eq!(short_name(&run), "Zugló · 2026-09-19");
    // And with neither, it is at least distinguishable from the one beside it.
    run.source = String::new();
    assert_eq!(short_name(&run), "search 12 · 2026-09-19");
    assert_eq!(place_of(&run), "search 12");
}

#[test]
fn the_clock_is_left_off_the_chip_but_not_off_the_line_below() {
    let mut run = a_run("done", None);
    run.started_at = "2026-09-19 14:32:01".into();
    assert_eq!(short_name(&run), "Zugló · 2026-09-19");
    assert!(
        describe_run(&run).contains("14:32"),
        "{}",
        describe_run(&run)
    );
}

#[test]
fn the_line_below_carries_what_you_would_choose_it_for() {
    let said = describe_run(&a_run("done", None));
    assert!(said.contains("74 leads"), "{said}");
    assert!(said.contains("956 businesses"), "{said}");
    assert!(said.contains("finished"), "{said}");
}

#[test]
fn a_search_that_failed_says_so_in_the_line_below() {
    let said = describe_run(&a_run("failed", Some("the scraper was not running")));
    assert!(said.contains("the scraper was not running"), "{said}");
}

#[test]
fn only_the_chosen_chip_wears_the_brass() {
    assert_eq!(chip_class(true), "wl-chip is-on");
    assert_eq!(chip_class(false), "wl-chip");
}

#[test]
fn the_line_below_does_not_say_the_day_twice() {
    // The chip already carries the day; the line carries the whole timestamp. Both
    // of them saying the date reads as a bug.
    let mut run = a_run("done", None);
    run.started_at = "2026-09-19 14:32:01".into();
    let said = describe_run(&run);
    assert_eq!(said.matches("2026-09-19").count(), 1, "{said}");
    assert!(said.starts_with("Zugló · 2026-09-19 14:32"), "{said}");
}

// ---- the moment a search ended ----

#[test]
fn a_finished_search_says_what_time_it_finished() {
    let mut run = a_run("done", None);
    run.finished_at = Some("2026-09-19 22:00:37".into());
    let (_, said) = describe_end(&run);
    assert!(said.contains("finished 22:00"), "{said}");
    // The seconds are not worth the width.
    assert!(!said.contains(":37"), "{said}");
}

#[test]
fn a_search_from_before_we_recorded_the_hour_says_nothing_rather_than_midnight() {
    // Backfilled searches carry only the day. "finished 00:00" would be a fact we
    // invented, and it would look exactly like a real one.
    let mut run = a_run("done", None);
    run.finished_at = Some("2026-09-19".into());
    let (_, said) = describe_end(&run);
    assert!(said.starts_with("finished ·"), "{said}");
}

#[test]
fn a_search_still_going_has_no_time_to_show() {
    let (_, said) = describe_end(&a_run("working", None));
    assert!(!said.contains(':') || said.contains("searching"), "{said}");
}

#[test]
fn stopping_and_failing_carry_the_hour_too() {
    let mut run = a_run("stopped", None);
    run.finished_at = Some("2026-09-20 18:09:26".into());
    assert!(describe_end(&run).1.contains("you stopped it 18:09"));

    let mut broken = a_run("failed", Some("the scraper was not running"));
    broken.finished_at = Some("2026-09-20 09:05:00".into());
    let said = describe_end(&broken).1;
    assert!(said.contains("failed 09:05"), "{said}");
    assert!(said.contains("the scraper was not running"), "{said}");
}

#[test]
fn a_moment_with_no_clock_on_it_gives_nothing_back() {
    assert_eq!(clock_of("2026-09-19 22:00:37"), Some("22:00"));
    assert_eq!(clock_of("2026-09-19"), None);
    assert_eq!(clock_of(""), None);
    assert_eq!(clock_of("2026-09-19 2:0"), None);
    assert_eq!(clock_of("2026-09-19 not a time"), None);
}

// ---- what we say before forgetting a search ----

fn a_cost(lost: i64, kept: i64, words: i64) -> api::ForgetCost {
    api::ForgetCost {
        words,
        businesses_lost: lost,
        businesses_kept: kept,
    }
}

#[test]
fn a_search_that_would_cost_nothing_gets_no_warning() {
    // Silence is the right answer. "0 businesses would be lost" is noise dressed
    // as care, and it teaches you to click through the real warning.
    assert_eq!(forget_warning(&a_cost(0, 0, 27)), None);
}

#[test]
fn a_search_that_would_take_businesses_says_how_many() {
    let said = forget_warning(&a_cost(346, 0, 27)).expect("there is a warning");
    assert!(said.contains("346 businesses"), "{said}");
    assert!(said.contains("for good"), "{said}");
}

#[test]
fn one_business_is_not_one_businesses() {
    let said = forget_warning(&a_cost(1, 0, 27)).expect("there is a warning");
    assert!(said.contains("1 business leaves"), "{said}");
    assert!(!said.contains("businesses"), "{said}");
    assert!(said.contains("it."), "{said}");
}

#[test]
fn what_survives_is_always_said_even_when_it_is_only_the_record() {
    // "What happens to my leads" is the question being asked, so it gets an answer
    // whether or not there is bad news beside it.
    let said = forget_kept(&a_cost(5, 0, 27));
    assert!(said.contains("record of the search goes"), "{said}");
    assert!(said.contains("27 search words"), "{said}");
}

#[test]
fn businesses_an_earlier_search_saw_are_named_as_survivors() {
    let said = forget_kept(&a_cost(0, 12, 3));
    assert!(said.contains("12 businesses stay"), "{said}");
    assert!(said.contains("earlier search"), "{said}");
    assert!(said.contains("3 search words"), "{said}");
}

#[test]
fn one_survivor_and_one_word_are_both_said_singly() {
    let said = forget_kept(&a_cost(0, 1, 1));
    assert!(said.contains("1 business stays"), "{said}");
    assert!(said.contains("1 search word with it"), "{said}");
}
