//! Tests for [`crate::runs`] — a scrape and the words it works through.

use super::*;

/// The area these runs are searching. Only its name and centre matter here.
fn zuglo() -> webleads_domain::Area {
    webleads_domain::Area {
        label: "Zugló".to_owned(),
        address: "Hősök tere".to_owned(),
        lat: 47.514_900,
        lon: 19.077_800,
        max_results: 200,
    }
}
use crate::Store;

/// Three words, as a run would be given them.
fn three_words() -> Vec<(String, Option<String>)> {
    vec![
        ("borbély Budapest".into(), Some("Barbers".into())),
        (
            "vízvezeték-szerelő Budapest".into(),
            Some("Plumbers".into()),
        ),
        ("fodrászat Budapest".into(), Some("Hairdressers".into())),
    ]
}

#[tokio::test]
async fn a_scrape_queues_its_words_in_order() {
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    let words = store.words(run).await.expect("reads");
    assert_eq!(words.len(), 3);
    assert_eq!(words[0].phrase, "borbély Budapest");
    assert_eq!(words[2].phrase, "fodrászat Budapest");
    assert!(words.iter().all(|w| w.state == WordState::Waiting));
}

#[tokio::test]
async fn the_trade_travels_with_the_word() {
    // This is where the Trade column finally gets its answer — from what we searched
    // for, not from whatever fast mode left behind.
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    let words = store.words(run).await.expect("reads");
    assert_eq!(words[0].trade.as_deref(), Some("Barbers"));
    assert_eq!(words[1].trade.as_deref(), Some("Plumbers"));
}

#[tokio::test]
async fn the_words_are_worked_through_one_at_a_time_in_order() {
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    let first = store.next_word(run).await.expect("reads").expect("a word");
    assert_eq!(first.position, 0);
    store
        .word_searching(first.id, "job-1")
        .await
        .expect("marks");
    store.word_done(first.id, 12).await.expect("marks");

    let second = store.next_word(run).await.expect("reads").expect("a word");
    assert_eq!(second.position, 1);
}

#[tokio::test]
async fn a_run_with_nothing_left_hands_back_no_word() {
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    while let Some(word) = store.next_word(run).await.expect("reads") {
        store.word_searching(word.id, "job").await.expect("marks");
        store.word_done(word.id, 1).await.expect("marks");
    }
    assert!(store.next_word(run).await.expect("reads").is_none());
}

#[tokio::test]
async fn a_word_left_mid_search_is_picked_up_first_after_a_restart() {
    // The whole point of keeping this in the database. Its scraper job may well still
    // be running, and searching the same word again would waste a minute and annoy
    // Google for nothing.
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    let first = store.next_word(run).await.expect("reads").expect("a word");
    store
        .word_searching(first.id, "job-7")
        .await
        .expect("marks");
    // ...and here the server stops.

    let resumed = store.next_word(run).await.expect("reads").expect("a word");
    assert_eq!(
        resumed.id, first.id,
        "it should carry on, not start the next word"
    );
    assert_eq!(resumed.state, WordState::Searching);
    assert_eq!(
        resumed.remote_id.as_deref(),
        Some("job-7"),
        "the scraper's job id has to survive, or the results are lost"
    );
}

#[tokio::test]
async fn a_half_finished_scrape_is_found_on_start_up() {
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    let found = store
        .unfinished_scrape()
        .await
        .expect("reads")
        .expect("a run");
    assert_eq!(found.id, run);
    assert_eq!(found.state, RunState::Working);
    assert_eq!(found.words_total, 3);
    assert_eq!(found.words_done, 0);
}

#[tokio::test]
async fn a_finished_scrape_is_not_picked_up_again() {
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .set_run_state(run, RunState::Done, None)
        .await
        .expect("sets");

    assert!(store.unfinished_scrape().await.expect("reads").is_none());
}

#[tokio::test]
async fn a_stopped_run_is_not_picked_up_again_either() {
    // Pressing stop has to mean stop, including across a restart.
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .set_run_state(run, RunState::Stopped, Some("you pressed stop"))
        .await
        .expect("sets");

    assert!(store.unfinished_scrape().await.expect("reads").is_none());
    let run = store.run(run).await.expect("reads").expect("a run");
    assert_eq!(run.state, RunState::Stopped);
    assert_eq!(run.note.as_deref(), Some("you pressed stop"));
}

#[tokio::test]
async fn one_word_failing_does_not_end_the_run() {
    // The next word may be perfectly fine. Losing ninety minutes because one search
    // timed out would be the wrong trade.
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    let first = store.next_word(run).await.expect("reads").expect("a word");
    store
        .word_searching(first.id, "job-1")
        .await
        .expect("marks");
    store
        .word_failed(first.id, "the scraper did not answer in time")
        .await
        .expect("marks");

    let next = store.next_word(run).await.expect("reads").expect("a word");
    assert_eq!(next.position, 1);
    assert!(store.unfinished_scrape().await.expect("reads").is_some());
}

#[tokio::test]
async fn the_progress_counts_the_failed_words_as_done_with() {
    // Otherwise a run with one bad word would never reach the end of its bar.
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    let first = store.next_word(run).await.expect("reads").expect("a word");
    store.word_searching(first.id, "j").await.expect("marks");
    store
        .word_failed(first.id, "timed out")
        .await
        .expect("marks");

    let progress = store.run(run).await.expect("reads").expect("a run");
    assert_eq!(progress.words_done, 1);
    assert_eq!(progress.words_total, 3);
}

#[tokio::test]
async fn the_run_adds_up_what_every_word_found() {
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    for found in [12, 0, 7] {
        let word = store.next_word(run).await.expect("reads").expect("a word");
        store.word_searching(word.id, "j").await.expect("marks");
        store.word_done(word.id, found).await.expect("marks");
    }

    let run = store.run(run).await.expect("reads").expect("a run");
    assert_eq!(run.found_total, 19);
    assert_eq!(run.words_done, 3);
}

#[tokio::test]
async fn a_word_that_found_nothing_is_recorded_as_nought_not_as_missing() {
    // Nought is the interesting number — it usually means the Hungarian is wrong.
    // Left as `None` it would be indistinguishable from a word never searched.
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    let word = store.next_word(run).await.expect("reads").expect("a word");
    store.word_searching(word.id, "j").await.expect("marks");
    store.word_done(word.id, 0).await.expect("marks");

    let report = store.words(run).await.expect("reads");
    assert_eq!(report[0].found, Some(0));
    assert_eq!(report[0].state, WordState::Done);
    // And a word nobody has searched yet is still unmistakably that.
    assert_eq!(report[1].found, None);
}

#[tokio::test]
async fn there_is_no_estimate_until_something_has_been_measured() {
    // An estimate made up before we have measured anything is worse than saying
    // "I do not know yet".
    let store = Store::in_memory().await.expect("opens");
    assert_eq!(store.seconds_per_word().await.expect("reads"), None);
}

#[tokio::test]
async fn a_run_state_survives_being_written_down_and_read_back() {
    for state in [
        RunState::Working,
        RunState::Done,
        RunState::Stopped,
        RunState::Failed,
    ] {
        assert_eq!(
            state.as_str().parse::<RunState>().expect("reads back"),
            state
        );
    }
    for state in [
        WordState::Waiting,
        WordState::Searching,
        WordState::Done,
        WordState::Failed,
    ] {
        assert_eq!(
            state.as_str().parse::<WordState>().expect("reads back"),
            state
        );
    }
}

#[test]
fn only_a_working_run_has_anything_left_to_do() {
    assert!(RunState::Working.still_going());
    assert!(!RunState::Done.still_going());
    assert!(!RunState::Stopped.still_going());
    assert!(!RunState::Failed.still_going());
}

#[test]
fn a_word_that_is_not_a_state_is_refused() {
    assert!("banana".parse::<RunState>().is_err());
    assert!("".parse::<WordState>().is_err());
}

// ---- the queue ----

#[tokio::test]
async fn a_queued_search_is_an_ordinary_run_that_has_not_started() {
    // Same words, same report, same recovery — only its state differs. That is why
    // nothing else in the program had to learn a new idea.
    let store = Store::in_memory().await.expect("opens");
    let id = store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");

    let run = store.run(id).await.expect("reads").expect("it is there");
    assert_eq!(run.state, RunState::Queued);
    assert_eq!(run.words_total, 3, "its words are written down already");
    assert_eq!(run.words_done, 0);
    // And it is not mistaken for one that is going.
    assert!(!run.state.still_going());
    assert!(store.unfinished_scrape().await.expect("looks").is_none());
}

#[tokio::test]
async fn the_queue_comes_back_in_the_order_you_lined_it_up() {
    let store = Store::in_memory().await.expect("opens");
    let mut obuda = zuglo();
    obuda.label = "Óbuda".into();

    store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");
    store
        .queue_scrape(
            &obuda,
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");

    let waiting = store.queued().await.expect("lists");
    assert_eq!(waiting.len(), 2);
    assert_eq!(waiting[0].source, "Zugló");
    assert_eq!(waiting[1].source, "Óbuda");
}

#[tokio::test]
async fn the_next_one_starts_only_when_nothing_else_is_going() {
    let store = Store::in_memory().await.expect("opens");
    let going = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");

    // One search at a time, whatever route asked for it.
    assert!(store.start_next_queued().await.expect("asks").is_none());

    store
        .set_run_state(going, RunState::Done, None)
        .await
        .expect("finishes");
    let started = store
        .start_next_queued()
        .await
        .expect("asks")
        .expect("takes it");
    assert_eq!(started.state, RunState::Working);
    assert!(store.queued().await.expect("lists").is_empty());
}

#[tokio::test]
async fn an_empty_queue_simply_has_nothing_to_start() {
    let store = Store::in_memory().await.expect("opens");
    assert!(store.start_next_queued().await.expect("asks").is_none());
}

#[tokio::test]
async fn one_can_be_taken_out_of_the_queue_without_touching_the_others() {
    let store = Store::in_memory().await.expect("opens");
    let first = store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");
    store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");

    assert!(store.forget_queued(first).await.expect("forgets"));
    let left = store.queued().await.expect("lists");
    assert_eq!(left.len(), 1);
    assert_ne!(left[0].id, first);
}

#[tokio::test]
async fn a_search_that_is_going_cannot_be_quietly_deleted() {
    // Stopping one that is running is what the stop button is for; deleting it would
    // throw away whatever it had already found.
    let store = Store::in_memory().await.expect("opens");
    let going = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    assert!(!store.forget_queued(going).await.expect("asks"));
    assert!(store.run(going).await.expect("reads").is_some());
}

#[tokio::test]
async fn a_run_remembers_the_area_it_was_searching() {
    // A run picked up after a restart uses what it was asked to do, not the defaults.
    let store = Store::in_memory().await.expect("opens");
    let id = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    let area = store
        .run_area(id)
        .await
        .expect("reads")
        .expect("it is there");
    assert_eq!(area.label, "Zugló");
    assert_eq!(area.lat, zuglo().lat);
}

// ---- how long a search takes ----

#[tokio::test]
async fn with_nothing_measured_yet_we_say_so_rather_than_guess() {
    // An invented estimate is worse than "not known yet": you would plan an evening
    // around it.
    let store = Store::in_memory().await.expect("opens");
    assert_eq!(store.seconds_per_word().await.expect("asks"), None);
}

#[tokio::test]
async fn a_finished_word_measures_how_long_it_took() {
    // The times carry a clock, not just a date — without one every search measures
    // as nought seconds and the page says "not known" for ever.
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    let word = store.next_word(run).await.expect("asks").expect("a word");

    store
        .word_searching(word.id, "job-1")
        .await
        .expect("starts");
    store.word_done(word.id, 76).await.expect("finishes");

    // The real pair of writes takes a fraction of a second, which is not a search.
    // Stretching them apart is the only way to check the measuring itself.
    sqlx::query(
        "UPDATE run_word SET started_at = '2026-09-19 18:00:00',
                             finished_at = '2026-09-19 18:01:30'
         WHERE id = ?1",
    )
    .bind(word.id)
    .execute(&store.pool)
    .await
    .expect("stretches the two apart");

    let each = store
        .seconds_per_word()
        .await
        .expect("asks")
        .expect("measured");
    assert!((each - 90.0).abs() < 1.0, "ninety seconds, not {each}");
}

#[tokio::test]
async fn words_recorded_before_we_timed_them_are_left_out_of_the_average() {
    // Rows from before the clock was added read as nought seconds. Averaging those in
    // would report a search as instant, which is the opposite of useful.
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    let first = store.next_word(run).await.expect("asks").expect("a word");
    store
        .word_searching(first.id, "job-1")
        .await
        .expect("starts");
    store.word_done(first.id, 76).await.expect("finishes");
    sqlx::query(
        "UPDATE run_word SET started_at = '2026-09-19', finished_at = '2026-09-19'
         WHERE id = ?1",
    )
    .bind(first.id)
    .execute(&store.pool)
    .await
    .expect("makes it look old");

    let second = store.next_word(run).await.expect("asks").expect("a word");
    store
        .word_searching(second.id, "job-2")
        .await
        .expect("starts");
    store.word_done(second.id, 40).await.expect("finishes");
    sqlx::query(
        "UPDATE run_word SET started_at = '2026-09-19 18:00:00',
                             finished_at = '2026-09-19 18:02:00'
         WHERE id = ?1",
    )
    .bind(second.id)
    .execute(&store.pool)
    .await
    .expect("stretches the two apart");

    let each = store
        .seconds_per_word()
        .await
        .expect("asks")
        .expect("measured");
    assert!(
        (each - 120.0).abs() < 1.0,
        "the old row should not have halved it: {each}"
    );
}

// ---- the record of past searches ----

#[tokio::test]
async fn the_searches_already_done_come_back_newest_first() {
    let store = Store::in_memory().await.expect("opens");
    let mut obuda = zuglo();
    obuda.label = "Óbuda".into();

    let first = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .set_run_state(first, RunState::Done, None)
        .await
        .expect("ends");
    let second = store
        .begin_scrape(
            &obuda,
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .set_run_state(second, RunState::Done, None)
        .await
        .expect("ends");

    let past = store.past_scrapes(25).await.expect("lists");
    assert_eq!(past.len(), 2);
    assert_eq!(past[0].source, "Óbuda", "newest first");
    assert_eq!(past[1].source, "Zugló");
}

#[tokio::test]
async fn a_search_that_failed_is_still_in_the_record() {
    // "It found nothing because the scraper was off" is the most useful thing this
    // list can tell you. Hiding it would leave you wondering where a search went.
    let store = Store::in_memory().await.expect("opens");
    let id = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .set_run_state(id, RunState::Failed, Some("the scraper is not running"))
        .await
        .expect("ends");

    let past = store.past_scrapes(25).await.expect("lists");
    assert_eq!(past.len(), 1);
    assert_eq!(past[0].state, RunState::Failed);
    assert_eq!(past[0].note.as_deref(), Some("the scraper is not running"));
}

#[tokio::test]
async fn something_still_waiting_is_not_in_the_record_yet() {
    // It belongs to the queue until it has actually run.
    let store = Store::in_memory().await.expect("opens");
    store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");
    assert!(store.past_scrapes(25).await.expect("lists").is_empty());
}

#[tokio::test]
async fn reading_a_file_is_not_a_search() {
    let store = Store::in_memory().await.expect("opens");
    store.begin_run("seed.csv").await.expect("imports");
    assert!(store.past_scrapes(25).await.expect("lists").is_empty());
}

#[tokio::test]
async fn asking_for_a_silly_number_of_them_is_brought_into_range() {
    let store = Store::in_memory().await.expect("opens");
    let id = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .set_run_state(id, RunState::Done, None)
        .await
        .expect("ends");

    assert_eq!(store.past_scrapes(0).await.expect("lists").len(), 1);
    assert_eq!(store.past_scrapes(-5).await.expect("lists").len(), 1);
}

#[tokio::test]
async fn a_search_remembers_the_settings_it_was_asked_for() {
    // The bug this closes: a queued search rebuilt its settings from the defaults
    // when its turn came, so anything chosen was lost between pressing and running.
    let store = Store::in_memory().await.expect("opens");
    let asked = webleads_domain::ScrapeOptions {
        zoom: Some(12),
        fast_mode: true,
        max_time_s: 600,
        ..Default::default()
    };

    let id = store
        .queue_scrape(&zuglo(), &three_words(), &asked)
        .await
        .expect("queues");

    let kept = store
        .run_options(id)
        .await
        .expect("reads")
        .expect("they were written down");
    assert_eq!(kept.zoom, Some(12));
    assert!(kept.fast_mode);
    assert_eq!(kept.max_time_s, 600);
}

#[tokio::test]
async fn a_search_recorded_before_settings_were_kept_falls_back() {
    // Anything from before the column existed has none, and must run on the defaults
    // rather than failing.
    let store = Store::in_memory().await.expect("opens");
    let id = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    sqlx::query("UPDATE run SET options_json = NULL WHERE id = ?1")
        .bind(id)
        .execute(&store.pool)
        .await
        .expect("makes it look old");

    assert!(store.run_options(id).await.expect("asks").is_none());
}

// ---- pausing a search ----

#[test]
fn held_is_written_and_read_back_like_any_other_state() {
    assert_eq!(RunState::Held.as_str(), "held");
    assert_eq!("held".parse::<RunState>().expect("parses"), RunState::Held);
    // And the word we store is not the word a person reads elsewhere.
    assert!("paused".parse::<RunState>().is_err());
}

#[test]
fn a_held_search_still_counts_as_going_but_is_not_moving() {
    // Two different questions: what blocks a second search, and what draws a bar.
    assert!(RunState::Held.still_going());
    assert!(!RunState::Held.working_now());
    assert!(RunState::Working.still_going());
    assert!(RunState::Working.working_now());
    for finished in [RunState::Done, RunState::Stopped, RunState::Failed] {
        assert!(!finished.still_going(), "{finished:?}");
        assert!(!finished.working_now(), "{finished:?}");
    }
}

#[tokio::test]
async fn pausing_holds_the_search_and_continuing_picks_it_up() {
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    assert!(store.hold_run(run).await.expect("holds"));
    let held = store.run(run).await.expect("reads").expect("there");
    assert_eq!(held.state, RunState::Held);
    // The page can find it again with nobody watching.
    assert_eq!(
        store.held_scrape().await.expect("reads").map(|r| r.id),
        Some(run)
    );

    assert!(store.resume_run(run).await.expect("resumes"));
    assert_eq!(
        store.run(run).await.expect("reads").expect("there").state,
        RunState::Working
    );
    assert!(store.held_scrape().await.expect("reads").is_none());
}

#[tokio::test]
async fn only_a_running_search_can_be_paused_and_only_a_held_one_continued() {
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");

    // Nothing to continue: it is not on hold.
    assert!(!store.resume_run(run).await.expect("tries"));
    store.hold_run(run).await.expect("holds");
    // Nothing to pause: it is already on hold.
    assert!(!store.hold_run(run).await.expect("tries"));

    // And a finished search is finished, whichever button you press.
    store
        .set_run_state(run, RunState::Stopped, Some("you pressed stop"))
        .await
        .expect("stops");
    assert!(!store.hold_run(run).await.expect("tries"));
    assert!(!store.resume_run(run).await.expect("tries"));
}

#[tokio::test]
async fn a_held_search_keeps_its_place_against_the_queue() {
    // The trap this closes: if a paused search did not count as going, the next
    // queued one would start and continuing would find the floor taken.
    let store = Store::in_memory().await.expect("opens");
    let first = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");

    store.hold_run(first).await.expect("holds");
    assert!(
        store.start_next_queued().await.expect("looks").is_none(),
        "the queue must wait for the paused search"
    );

    store.resume_run(first).await.expect("resumes");
    store
        .set_run_state(first, RunState::Done, None)
        .await
        .expect("finishes");
    assert!(store.start_next_queued().await.expect("looks").is_some());
}

#[tokio::test]
async fn a_held_search_is_not_picked_up_by_itself_after_a_restart() {
    // Waiting for you is what held means. Only a search left mid-flight is resumed.
    let store = Store::in_memory().await.expect("opens");
    let run = store
        .begin_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("starts");
    assert!(store.unfinished_scrape().await.expect("reads").is_some());

    store.hold_run(run).await.expect("holds");
    assert!(
        store.unfinished_scrape().await.expect("reads").is_none(),
        "a paused search must not restart itself"
    );
}

// ---- which states end a run ----

#[test]
fn the_three_ways_a_run_ends_all_count_as_ended() {
    // Finished on its own, stopped by hand, or broken — all three are an end, and all
    // three are worth a moment on the record.
    for state in [RunState::Done, RunState::Stopped, RunState::Failed] {
        assert!(
            state.ends_the_run(),
            "{} should end the run",
            state.as_str()
        );
    }
}

#[test]
fn a_run_still_going_has_not_ended() {
    for state in [RunState::Queued, RunState::Working] {
        assert!(!state.ends_the_run(), "{} is still going", state.as_str());
    }
}

#[test]
fn pausing_is_not_ending() {
    // The one worth its own test: held looks like a stop and is not one. Treating it
    // as an end would stamp a finishing time on a search you mean to carry on.
    assert!(!RunState::Held.ends_the_run());
}

/// When a run says it ended, read back the way the page reads it.
async fn ended_at(store: &Store, id: i64) -> Option<String> {
    store
        .run(id)
        .await
        .expect("reads")
        .expect("it is there")
        .finished_at
}

#[tokio::test]
async fn a_search_that_ends_records_when_it_ended() {
    let store = Store::in_memory().await.expect("opens");
    let id = store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");

    // Pausing is not ending, so it leaves no mark.
    store
        .set_run_state(id, RunState::Held, None)
        .await
        .expect("pauses");
    assert!(
        ended_at(&store, id).await.is_none(),
        "a paused search has not finished"
    );

    store
        .set_run_state(id, RunState::Done, None)
        .await
        .expect("finishes");
    let ended = ended_at(&store, id).await;
    assert!(ended.is_some(), "a finished search says when it finished");

    // And ending twice does not move the moment it already recorded.
    store
        .set_run_state(id, RunState::Stopped, None)
        .await
        .expect("again");
    assert_eq!(ended_at(&store, id).await, ended);
}

// ---- forgetting a search that is over ----

#[tokio::test]
async fn forgetting_a_search_takes_its_words_with_it() {
    let store = Store::in_memory().await.expect("opens");
    let id = store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");
    store
        .set_run_state(id, RunState::Done, None)
        .await
        .expect("finishes");

    assert!(store.delete_run(id).await.expect("forgets"));
    assert!(store.run(id).await.expect("reads").is_none());
    assert!(
        store.words(id).await.expect("reads").is_empty(),
        "its words go with it"
    );
}

#[tokio::test]
async fn a_search_still_going_is_refused_rather_than_deleted() {
    // Deleting it underneath the scraper would leave it writing words into a run
    // that is not there.
    let store = Store::in_memory().await.expect("opens");
    let id = store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");

    for state in [RunState::Working, RunState::Held] {
        store
            .set_run_state(id, state, None)
            .await
            .expect("changes state");
        assert!(
            store.delete_run(id).await.is_err(),
            "{} must be refused",
            state.as_str()
        );
        assert!(store.run(id).await.expect("reads").is_some(), "still there");
    }
}

#[tokio::test]
async fn forgetting_a_search_nobody_has_is_not_a_failure() {
    let store = Store::in_memory().await.expect("opens");
    assert!(!store.delete_run(9999).await.expect("says no"));
}

#[tokio::test]
async fn forgetting_the_only_search_that_saw_a_business_takes_it_too() {
    // Not a nicety — the whole reason the page asks first. A business nothing else
    // has ever seen has no verdict left to stand on, so it leaves the call list with
    // the search. `forget_cost` is what says this out loud beforehand.
    let store = Store::in_memory().await.expect("opens");
    let id = store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");
    store
        .record(
            id,
            &[crate::testing::found(crate::testing::a_lead(
                "keep-me", 0, 10,
            ))],
        )
        .await
        .expect("records");
    store
        .set_run_state(id, RunState::Done, None)
        .await
        .expect("finishes");

    let before = store.leads(&Default::default()).await.expect("reads").len();
    assert!(before > 0, "there is something to lose");

    store.delete_run(id).await.expect("forgets");

    let after = store.leads(&Default::default()).await.expect("reads");
    assert!(
        after.len() < before,
        "a business only this search ever saw has nothing left to stand on"
    );
}

#[tokio::test]
async fn a_business_an_earlier_search_also_saw_keeps_its_older_verdict() {
    // The case that matters: two searches covered the same street. Forgetting the
    // newer one must put the business back the way the older one left it, not make
    // it vanish because its current verdict came from the search you deleted.
    let store = Store::in_memory().await.expect("opens");
    let words = three_words();
    let options = webleads_domain::ScrapeOptions::default();

    let older = store
        .queue_scrape(&zuglo(), &words, &options)
        .await
        .expect("queues");
    store
        .record(
            older,
            &[crate::testing::found(crate::testing::a_lead(
                "seen-twice",
                0,
                10,
            ))],
        )
        .await
        .expect("records");
    store
        .set_run_state(older, RunState::Done, None)
        .await
        .expect("finishes");

    let newer = store
        .queue_scrape(&zuglo(), &words, &options)
        .await
        .expect("queues");
    store
        .record(
            newer,
            &[crate::testing::found(crate::testing::a_lead(
                "seen-twice",
                0,
                10,
            ))],
        )
        .await
        .expect("records");
    store
        .set_run_state(newer, RunState::Done, None)
        .await
        .expect("finishes");

    // And the tool says so before anybody agrees to it.
    let cost = store
        .forget_cost(newer)
        .await
        .expect("reads")
        .expect("it is there");
    assert_eq!(cost.businesses_lost, 0, "nothing is only this search's");
    assert_eq!(cost.businesses_kept, 1);

    store.delete_run(newer).await.expect("forgets");

    let after = store.leads(&Default::default()).await.expect("reads");
    assert!(
        after.iter().any(|row| row.lead.id == "seen-twice"),
        "it is still on the call list, wearing the older search's verdict"
    );
}

#[tokio::test]
async fn the_cost_of_forgetting_is_counted_before_anything_is_deleted() {
    let store = Store::in_memory().await.expect("opens");
    let id = store
        .queue_scrape(
            &zuglo(),
            &three_words(),
            &webleads_domain::ScrapeOptions::default(),
        )
        .await
        .expect("queues");
    store
        .record(
            id,
            &[crate::testing::found(crate::testing::a_lead(
                "only-here",
                0,
                10,
            ))],
        )
        .await
        .expect("records");

    let cost = store
        .forget_cost(id)
        .await
        .expect("reads")
        .expect("it is there");
    assert_eq!(cost.words, 3);
    assert_eq!(cost.businesses_lost, 1, "nothing else has ever seen it");
    assert_eq!(cost.businesses_kept, 0);
    // Counting the cost must not have cost anything.
    assert!(store.run(id).await.expect("reads").is_some());
}

#[tokio::test]
async fn the_cost_of_forgetting_a_search_nobody_has_is_simply_missing() {
    let store = Store::in_memory().await.expect("opens");
    assert!(store.forget_cost(9999).await.expect("reads").is_none());
}
