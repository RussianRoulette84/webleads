//! Tests for [`crate::state`].

use super::*;

/// One row of the file, as the reader sees it: headings and cells.
fn row(tier: &str, hu: &str) -> TargetRow {
    with(tier, &[("words_hu", hu)])
}

/// A row with whichever language columns a test needs.
fn with(tier: &str, words: &[(&str, &str)]) -> TargetRow {
    let mut row = TargetRow::new();
    row.insert("rank".to_owned(), "1".to_owned());
    row.insert("tier".to_owned(), tier.to_owned());
    row.insert("english".to_owned(), "Plumbers".to_owned());
    row.insert("no_site_odds".to_owned(), "4".to_owned());
    row.insert("budget".to_owned(), "4".to_owned());
    row.insert("note".to_owned(), "Emergency work".to_owned());
    for (heading, cell) in words {
        row.insert((*heading).to_owned(), (*cell).to_owned());
    }
    row
}

#[test]
fn one_word_in_a_column_gives_one_search_word() {
    let target = parse_target_row(&row("S", "vízvezeték-szerelő")).expect("usable");
    assert_eq!(target.words("hu"), vec!["vízvezeték-szerelő"]);
    assert_eq!(target.tier, Tier::S);
    assert_eq!(target.english, "Plumbers");
}

#[test]
fn semicolons_split_the_column_into_several_search_words() {
    let target = parse_target_row(&row("A", "borbély;barbershop;fodrász")).expect("usable");
    assert_eq!(target.words("hu"), vec!["borbély", "barbershop", "fodrász"]);
}

#[test]
fn spacing_around_the_semicolons_does_not_survive() {
    let target = parse_target_row(&row("B", "  borbély ;  barbershop  ")).expect("usable");
    assert_eq!(target.words("hu"), vec!["borbély", "barbershop"]);
}

#[test]
fn empty_pieces_between_semicolons_are_dropped() {
    let target = parse_target_row(&row("C", "borbély;;;barbershop;")).expect("usable");
    assert_eq!(target.words("hu"), vec!["borbély", "barbershop"]);
}

#[test]
fn a_trade_with_no_word_to_search_for_is_not_a_trade() {
    // Keeping it would put a row on the screen that can never be searched.
    assert!(parse_target_row(&row("S", "")).is_none());
    assert!(parse_target_row(&row("S", "   ")).is_none());
    assert!(parse_target_row(&row("S", ";;;")).is_none());
}

#[test]
fn every_words_column_becomes_a_language() {
    // Adding a language should be an edit to a spreadsheet, not to the program.
    let target = parse_target_row(&with(
        "S",
        &[
            ("words_hu", "vízvezeték-szerelő"),
            ("words_en", "plumber;plumbing"),
        ],
    ))
    .expect("usable");
    assert_eq!(target.words("hu"), vec!["vízvezeték-szerelő"]);
    assert_eq!(target.words("en"), vec!["plumber", "plumbing"]);
    let langs: Vec<&str> = target.languages().collect();
    assert_eq!(langs, vec!["en", "hu"]);
}

#[test]
fn a_blank_column_means_the_trade_is_not_searched_in_that_language() {
    // A Gulf trade with no Hungarian, which must not put a word into a Hungarian run.
    let target = parse_target_row(&with(
        "A",
        &[("words_hu", "   "), ("words_en", "shisha cafe")],
    ))
    .expect("usable");
    assert!(!target.searched_in("hu"));
    assert!(target.searched_in("en"));
    let langs: Vec<&str> = target.languages().collect();
    assert_eq!(
        langs,
        vec!["en"],
        "the blank one is left out, not kept empty"
    );
}

#[test]
fn a_trade_with_no_words_in_any_language_is_refused() {
    let nothing = parse_target_row(&with("S", &[("words_hu", ""), ("words_en", "  ")]));
    assert!(nothing.is_none());
}

#[test]
fn a_column_heading_that_is_not_a_language_is_ignored() {
    // The file carries notes and numbers too; only `words_*` is a language.
    let target =
        parse_target_row(&with("S", &[("words_hu", "vízvezeték-szerelő")])).expect("usable");
    let langs: Vec<&str> = target.languages().collect();
    assert_eq!(langs, vec!["hu"]);
    assert_eq!(target.note, "Emergency work");
    assert_eq!(target.no_site_odds, 4);
}

#[test]
fn a_row_with_no_english_name_is_refused() {
    // The English name is the key every business on the list already stores.
    let mut nameless = row("S", "vízvezeték-szerelő");
    nameless.insert("english".to_owned(), "   ".to_owned());
    assert!(parse_target_row(&nameless).is_none());
}

#[test]
fn a_letter_that_is_not_a_tier_throws_the_row_out() {
    assert!(parse_target_row(&row("X", "borbély")).is_none());
    assert!(parse_target_row(&row("", "borbély")).is_none());
    assert!(parse_target_row(&row("s", "borbély")).is_none());
}

#[test]
fn a_row_with_no_note_is_still_a_usable_trade() {
    let mut r = row("S", "borbély");
    r.remove("note");
    let target = parse_target_row(&r).expect("usable");
    assert_eq!(target.note, "");
    assert_eq!(target.promise(), 8);
}

#[test]
fn home_is_where_we_say_it_is() {
    let home = home();
    assert_eq!(home.lat, CENTRE_LAT);
    assert_eq!(home.lon, CENTRE_LON);
    assert!(home.address.contains("Hősök"));
    // Home measures, and that is all it does: there is no fence anywhere, so nothing
    // is dropped for being far from here or from anywhere else.
}

#[test]
fn home_measures_the_distance_the_rows_show() {
    // The far side of Budapest is several kilometres away, and that number is what
    // the "Away" column prints whichever area the business was found in.
    let away = home().distance_m(47.4205, 19.0700).round() as u32;
    assert!((5_000..20_000).contains(&away), "{away} m");
    assert_eq!(home().distance_m(CENTRE_LAT, CENTRE_LON).round() as u32, 0);
}

#[test]
fn the_area_we_search_starts_as_the_whole_city() {
    // Which is what has been searched all along. Until you choose another one, the
    // behaviour is exactly what it was.
    let city = whole_city();
    assert_eq!(city.label, AREA_LABEL);

    assert_eq!(city.lat, CENTRE_LAT);
    assert_eq!(city.max_results, MAX_RESULTS);
}

#[test]
fn only_one_import_can_be_running() {
    // Two tabs, or a stray curl. The second must join the first rather than
    // start a competing one.
    let jobs = Jobs::default();
    let (first, _job) = jobs.start_exclusive(11, "checking").expect("starts");

    let second = jobs.start_exclusive(11, "checking");
    let (id, _) = second.expect_err("a second import must not start");
    assert_eq!(id, first, "it should hand back the one already running");
}

#[test]
fn a_new_import_can_start_once_the_last_one_finished() {
    let jobs = Jobs::default();
    let (first, job) = jobs.start_exclusive(11, "checking").expect("starts");
    job.set(JobState::Done {
        summary: serde_json::json!({}),
    });

    let (second, _) = jobs.start_exclusive(11, "checking").expect("starts");
    assert_ne!(second, first);
}

#[test]
fn a_failed_import_does_not_block_the_next_one() {
    let jobs = Jobs::default();
    let (_, job) = jobs.start_exclusive(11, "checking").expect("starts");
    job.set(JobState::Failed {
        reason: "the database failed".into(),
    });
    assert!(jobs.start_exclusive(11, "checking").is_ok());
}

#[test]
fn a_finished_job_is_forgotten_once_it_is_old() {
    let mut jobs = HashMap::new();
    let old = Arc::new(Job {
        state: std::sync::Mutex::new(JobState::Done {
            summary: serde_json::json!({}),
        }),
        finished: std::sync::Mutex::new(Some(
            Instant::now() - KEEP_FINISHED_FOR - Duration::from_secs(1),
        )),
        done: Arc::new(AtomicUsize::new(0)),
        stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        total: AtomicUsize::new(0),
    });
    jobs.insert(1, old);

    forget_old(&mut jobs);
    assert!(jobs.is_empty());
}

#[test]
fn a_job_still_running_is_never_forgotten_however_long_it_takes() {
    // A scrape can legitimately run for an hour. Forgetting it mid-flight would
    // leave the page watching a number that no longer exists.
    let mut jobs = HashMap::new();
    jobs.insert(
        1,
        Arc::new(Job {
            state: std::sync::Mutex::new(JobState::Running {
                done: 3,
                total: 500,
                step: "checking".into(),
            }),
            finished: std::sync::Mutex::new(None),
            done: Arc::new(AtomicUsize::new(3)),
            stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            total: AtomicUsize::new(500),
        }),
    );

    forget_old(&mut jobs);
    assert_eq!(jobs.len(), 1);
}

#[test]
fn a_recently_finished_job_is_kept_so_the_page_can_read_it() {
    let jobs_store = Jobs::default();
    let (id, job) = jobs_store.start_exclusive(1, "checking").expect("starts");
    job.set(JobState::Done {
        summary: serde_json::json!({ "leads_found": 3 }),
    });
    // Starting another prunes, and this one is seconds old.
    jobs_store.start_exclusive(1, "checking").expect("starts");
    assert!(jobs_store.get(id).is_some());
}

#[test]
fn the_number_of_jobs_kept_has_a_ceiling() {
    // An hour is no protection against a busy hour.
    let jobs = Jobs::default();
    for _ in 0..KEEP_AT_MOST + 20 {
        let (_, job) = jobs.start_exclusive(1, "checking").expect("starts");
        job.set(JobState::Done {
            summary: serde_json::json!({}),
        });
    }
    let held = jobs.inner.read().expect("not poisoned").len();
    assert!(held <= KEEP_AT_MOST, "kept {held}");
}

// ---- stopping a job ----

#[test]
fn a_job_starts_willing_to_carry_on() {
    let jobs = Jobs::default();
    let (_id, job) = jobs.start_exclusive(65, "taking pictures").expect("starts");
    assert!(!job.stop_asked());
}

#[test]
fn asking_it_to_stop_is_seen_by_the_work_itself() {
    // The flag is shared with the tasks doing the work, so pressing stop reaches
    // them with nothing in between.
    let jobs = Jobs::default();
    let (_id, job) = jobs.start_exclusive(65, "taking pictures").expect("starts");
    let held_by_the_work = std::sync::Arc::clone(&job.stop);

    job.ask_to_stop();

    assert!(job.stop_asked());
    assert!(held_by_the_work.load(std::sync::atomic::Ordering::Relaxed));
}

#[test]
fn stopping_one_job_leaves_another_alone() {
    let jobs = Jobs::default();
    let (first_id, first) = jobs.start_exclusive(10, "one").expect("starts");
    first.set(JobState::Done {
        summary: serde_json::json!({}),
    });
    let (_second_id, second) = jobs.start_exclusive(10, "two").expect("starts");

    first.ask_to_stop();
    assert!(jobs.get(first_id).expect("still there").stop_asked());
    assert!(!second.stop_asked());
}

// ---- writing the trades back ----

fn a_target(english: &str, words: &[(&str, &[&str])]) -> Target {
    Target {
        rank: 7,
        tier: Tier::A,
        english: english.to_owned(),
        words: words
            .iter()
            .map(|(lang, said)| {
                (
                    (*lang).to_owned(),
                    said.iter().map(|w| (*w).to_owned()).collect(),
                )
            })
            .collect(),
        no_site_odds: 4,
        budget: 3,
        note: "a note, with a comma".to_owned(),
    }
}

#[test]
fn the_headings_carry_a_column_for_every_language() {
    let headings = target_headings(&["hu".to_owned(), "en".to_owned()]);
    assert_eq!(
        headings,
        vec![
            "rank",
            "tier",
            "english",
            "words_hu",
            "words_en",
            "no_site_odds",
            "budget",
            "note"
        ]
    );
}

#[test]
fn the_languages_of_a_list_come_out_once_each_in_the_order_they_matter() {
    // Hungarian first, not alphabetically: these are the file's columns, and a file
    // whose columns move about reads as damaged.
    let list = [
        a_target("One", &[("hu", &["a"]), ("en", &["b"])]),
        a_target("Two", &[("en", &["c"])]),
    ];
    assert_eq!(languages_of(&list), vec!["hu", "en"]);
}

#[test]
fn a_trade_writes_its_words_with_semicolons_between_them() {
    let row = target_row(
        &a_target("Barbers", &[("hu", &["borbély", "barbershop"])]),
        &["hu".to_owned(), "en".to_owned()],
    );
    assert_eq!(row[2], "Barbers");
    assert_eq!(row[3], "borbély;barbershop");
    // A language it has no words for writes an empty cell, which is how the reader
    // hears "not searched there".
    assert_eq!(row[4], "");
}

#[test]
fn what_we_write_is_what_we_read_back() {
    // The two halves are each other's inverse, which is the only thing that makes
    // editing the list on the page safe.
    let before = a_target(
        "Barbers",
        &[("hu", &["borbély", "barbershop"]), ("en", &["barber shop"])],
    );
    let languages = languages_of(std::slice::from_ref(&before));
    let headings = target_headings(&languages);
    let cells = target_row(&before, &languages);

    let mut row = TargetRow::new();
    for (heading, cell) in headings.iter().zip(cells.iter()) {
        row.insert(heading.clone(), cell.clone());
    }
    let after = parse_target_row(&row).expect("usable");

    assert_eq!(after.english, before.english);
    assert_eq!(after.tier, before.tier);
    assert_eq!(after.rank, before.rank);
    assert_eq!(after.words, before.words);
    assert_eq!(after.no_site_odds, before.no_site_odds);
    assert_eq!(after.budget, before.budget);
    assert_eq!(after.note, before.note);
}
