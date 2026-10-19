//! Tests for [`crate::console`].

use super::*;

fn a_run(state: &str) -> api::Run {
    api::Run {
        id: 7,
        state: state.to_owned(),
        source: "Zugló".into(),
        note: None,
        words_total: 27,
        words_done: 12,
        found_total: 318,
        now_searching: Some("fodrász Budapest".into()),
        area: Some("Zugló".into()),
        city: None,
        finished_at: None,
        started_at: "2026-09-20 14:02".into(),
        leads_found: 41,
    }
}

#[test]
fn nothing_running_says_so_rather_than_saying_nothing() {
    assert_eq!(describe(None), "nothing running");
}

#[test]
fn a_search_in_progress_says_which_word_and_how_far() {
    let said = describe(Some(&a_run("working")));
    assert!(said.contains("fodrász Budapest"), "{said}");
    assert!(said.contains("12 of 27 words"), "{said}");
}

#[test]
fn between_words_it_still_says_how_far_it_has_got() {
    let mut between = a_run("working");
    between.now_searching = None;
    let said = describe(Some(&between));
    assert!(said.starts_with("searching · "), "{said}");
    assert!(said.contains("12 of 27"), "{said}");
}

#[test]
fn a_paused_search_says_what_to_press() {
    // The one state where the screen has to tell you the next move, because nothing
    // will happen until you make it.
    let said = describe(Some(&a_run("held")));
    assert!(said.starts_with("paused"), "{said}");
    assert!(said.contains("continue"), "{said}");
}

#[test]
fn a_queued_search_counts_words_and_not_progress() {
    // Nothing has been searched yet, so "0 of 27 done" would be noise.
    let said = describe(Some(&a_run("queued")));
    assert_eq!(said, "queued · 27 words");
}

#[test]
fn a_finished_search_is_named_by_what_happened_to_it() {
    for state in ["done", "stopped", "failed"] {
        let said = describe(Some(&a_run(state)));
        assert!(said.starts_with(state), "{said}");
    }
}

#[test]
fn what_each_button_can_do_follows_the_state() {
    // The rules the buttons are lit by, checked here rather than by clicking.
    let working = a_run("working");
    assert!(working.working_now() && working.still_going() && !working.held());

    let held = a_run("held");
    assert!(held.held() && held.still_going() && !held.working_now());

    for finished in ["done", "stopped", "failed"] {
        let run = a_run(finished);
        assert!(!run.still_going(), "{finished}");
        assert!(!run.working_now() && !run.held(), "{finished}");
    }
}

// ---- which buttons are there at all ----

#[test]
fn with_nothing_running_the_only_thing_to_do_is_start_one() {
    assert_eq!(offer(None), vec![Button::Scrape]);
}

#[test]
fn a_running_search_offers_only_hold_and_end() {
    // No SCRAPE NOW: one search at a time, because Google notices anything faster.
    // No CONTINUE: there is nothing paused to continue.
    assert_eq!(
        offer(Some(&a_run("working"))),
        vec![Button::Pause, Button::Stop]
    );
}

#[test]
fn a_paused_search_offers_only_carry_on_and_give_up() {
    assert_eq!(
        offer(Some(&a_run("held"))),
        vec![Button::Continue, Button::Stop]
    );
}

#[test]
fn one_lined_up_can_still_be_called_off() {
    assert_eq!(offer(Some(&a_run("queued"))), vec![Button::Stop]);
}

#[test]
fn once_it_is_over_the_next_one_is_yours_to_start() {
    for finished in ["done", "stopped", "failed"] {
        assert_eq!(
            offer(Some(&a_run(finished))),
            vec![Button::Scrape],
            "{finished}"
        );
    }
}

#[test]
fn no_state_ever_offers_a_button_that_would_do_nothing() {
    // The whole point: a button you can see is a button that works.
    for state in ["working", "held", "queued", "done", "stopped", "failed"] {
        let run = a_run(state);
        let shown = offer(Some(&run));
        assert!(!shown.is_empty(), "{state} offers nothing at all");
        if shown.contains(&Button::Pause) {
            assert!(run.working_now(), "{state} offers pause but is not running");
        }
        if shown.contains(&Button::Continue) {
            assert!(run.held(), "{state} offers continue but is not paused");
        }
        if shown.contains(&Button::Stop) {
            assert!(
                run.still_going() || state == "queued",
                "{state} offers stop with nothing to stop"
            );
        }
        if shown.contains(&Button::Scrape) {
            assert!(
                !run.still_going(),
                "{state} offers a new search over a live one"
            );
        }
    }
}
