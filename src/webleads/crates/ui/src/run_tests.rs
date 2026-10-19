//! Tests for [`crate::run`] — what the page says about a run.

use super::*;

fn word(state: &str, found: Option<i64>) -> Word {
    Word {
        phrase: "borbély Budapest".into(),
        trade: Some("Barbers".into()),
        state: state.into(),
        found,
        note: None,
    }
}

// ---- what the console says about a run ----

fn a_run(now_searching: Option<&str>, done: i64, found: i64) -> Run {
    Run {
        id: 1,
        state: "working".into(),
        source: "Zugló".into(),
        note: None,
        words_total: 27,
        words_done: done,
        found_total: found,
        now_searching: now_searching.map(str::to_owned),
        area: Some("Zugló".into()),
        city: None,
        finished_at: None,
        started_at: "2026-09-19".into(),
        leads_found: 0,
    }
}

#[test]
fn the_console_says_which_word_it_is_on() {
    // The complaint this answers: "0 of 27" sat there for two and a half minutes and
    // looked stuck, when it was in the middle of the first word all along.
    let said = describe_progress(&a_run(Some("vízvezeték-szerelő Zugló"), 0, 0));
    assert!(said.contains("vízvezeték-szerelő Zugló"), "{said}");
    assert!(said.contains("0 of 27"), "{said}");
}

#[test]
fn between_words_it_says_what_the_run_is_doing() {
    let said = describe_progress(&a_run(None, 3, 210));
    assert!(said.contains("working"), "{said}");
    assert!(said.contains("3 of 27"), "{said}");
    assert!(said.contains("210 businesses"), "{said}");
}

#[test]
fn nothing_chosen_says_so_and_the_button_stays_off() {
    let (words, said) = cost(Some(api::Estimate {
        words: 0,
        seconds: None,
    }));
    assert_eq!(words, 0);
    assert_eq!(said, "nothing chosen yet");
}

#[test]
fn before_anything_has_been_measured_we_admit_it() {
    // An invented number is worse than none — you might plan an afternoon around it.
    let (words, said) = cost(Some(api::Estimate {
        words: 137,
        seconds: None,
    }));
    assert_eq!(words, 137);
    assert!(said.contains("137 searches"), "{said}");
    assert!(said.contains("not known"), "{said}");
}

#[test]
fn once_we_have_measured_it_we_say_how_long() {
    let (_, said) = cost(Some(api::Estimate {
        words: 137,
        seconds: Some(5_480),
    }))
    .clone();
    assert!(said.contains("137 searches"), "{said}");
    assert!(said.contains("1.5 hours"), "{said}");
}

#[test]
fn one_search_is_not_called_one_searches() {
    let (_, said) = cost(Some(api::Estimate {
        words: 1,
        seconds: Some(40),
    }));
    assert!(said.starts_with("1 search,"), "{said}");
}

#[test]
fn a_duration_is_said_in_units_a_person_uses() {
    assert_eq!(how_long(40), "40 seconds");
    assert_eq!(how_long(90), "90 seconds");
    assert_eq!(how_long(600), "10 minutes");
    assert_eq!(how_long(5_400), "90 minutes");
    assert_eq!(how_long(5_480), "1.5 hours");
    assert_eq!(how_long(10_800), "3.0 hours");
}

#[test]
fn a_run_is_only_going_while_it_says_it_is() {
    let going = Run {
        id: 1,
        state: "working".into(),
        source: "Budapest".into(),
        note: None,
        words_total: 10,
        words_done: 3,
        found_total: 40,
        now_searching: None,
        area: Some("Zugló".into()),
        city: None,
        finished_at: None,
        started_at: "2026-09-19".into(),
        leads_found: 0,
    };
    assert!(going.still_going());

    for ended in ["done", "stopped", "failed"] {
        let stopped = Run {
            state: ended.into(),
            ..going.clone()
        };
        assert!(
            !stopped.still_going(),
            "`{ended}` should not count as going"
        );
    }
}

// ---- which run the buttons follow ----

fn in_state(id: i64, state: &str) -> Run {
    Run {
        id,
        state: state.into(),
        ..a_run(None, 0, 0)
    }
}

#[test]
fn whatever_the_server_says_is_going_is_what_the_buttons_follow() {
    // The bug: the page held a finished run while a new one was searching, so it
    // offered SCRAPE NOW and no way to stop.
    let shown = next_run(Some(in_state(1, "done")), Some(in_state(2, "working")));
    assert_eq!(
        shown.map(|r| (r.id, r.state)),
        Some((2, "working".to_owned()))
    );
}

#[test]
fn a_finished_run_stays_on_screen_when_nothing_else_is_going() {
    let shown = next_run(Some(in_state(1, "done")), None);
    assert_eq!(shown.map(|r| r.id), Some(1));
}

#[test]
fn a_run_the_page_thought_was_going_has_ended_when_the_server_has_none() {
    assert!(next_run(Some(in_state(1, "working")), None).is_none());
    assert!(next_run(Some(in_state(1, "held")), None).is_none());
}
