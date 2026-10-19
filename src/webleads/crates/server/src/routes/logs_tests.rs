//! Tests for [`crate::routes::logs`].

use super::*;

#[test]
fn only_the_two_logs_we_named_can_be_asked_for() {
    // The whole point of the enum: there is no way to ask for a path at all, so
    // there is nothing to escape out of.
    assert_eq!(Which::Backend.prefix(), "backend.log");
    assert_eq!(Which::Scraper.prefix(), "scraper.log");
    for which in [Which::Backend, Which::Scraper] {
        let prefix = which.prefix();
        assert!(!prefix.contains('/'), "{prefix}");
        assert!(!prefix.contains(".."), "{prefix}");
    }
}

#[test]
fn the_colour_codes_come_out_and_the_words_stay() {
    let raw = "\u{1b}[2m2026-09-20T13:26:14\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \
               \u{1b}[2mwebleads_server\u{1b}[0m: database ready";
    assert_eq!(
        plain(raw),
        "2026-09-20T13:26:14  INFO webleads_server: database ready"
    );
}

#[test]
fn a_line_with_no_codes_in_it_is_left_alone() {
    let said = "2026-09-19T14:02:55.735895Z  INFO scraper: run starting run_id=6";
    assert_eq!(plain(said), said);
}

#[test]
fn the_noisy_lines_are_dropped_unless_you_ask_for_them() {
    let text = "2026-09-20 INFO scraper: searching `fodrász Budapest`\n\
                2026-09-20 DEBUG request{method=GET uri=/api/leads}: started\n\
                2026-09-20  WARN scraper: google refused two searches\n";

    let quiet = tidy(text, false);
    assert_eq!(quiet.len(), 2, "{quiet:?}");
    assert!(quiet.iter().all(|l| !l.contains("DEBUG")));

    let everything = tidy(text, true);
    assert_eq!(everything.len(), 3);
}

#[test]
fn blank_lines_never_reach_the_page() {
    // A terminal full of empty rows looks broken.
    let text = "INFO one\n\n   \n\nINFO two\n";
    assert_eq!(tidy(text, true), vec!["INFO one", "INFO two"]);
}

#[test]
fn a_half_written_last_line_is_still_offered() {
    // It is whole by the next read, and holding it back would make the window lag
    // behind the thing it is watching.
    let lines = tidy("INFO whole\nINFO half", true);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1], "INFO half");
}

#[test]
fn nothing_written_yet_is_no_lines_rather_than_a_failure() {
    // The scraper log is empty until the first search of the day.
    assert!(tidy("", false).is_empty());
    assert!(tidy("\n\n", false).is_empty());
}

#[test]
fn the_newest_rolled_file_is_the_one_being_written() {
    // `tracing-appender` names them by date, so the last in order is today's.
    let dir = std::env::temp_dir().join(format!("webleads-logs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("makes the folder");
    for name in [
        "backend.log",
        "backend.log.2026-09-19",
        "backend.log.2026-09-20",
        "scraper.log.2026-09-18",
        "not-ours.log",
    ] {
        std::fs::write(dir.join(name), "INFO hello\n").expect("writes");
    }

    let newest = newest_log(&dir, Which::Backend).expect("finds one");
    assert_eq!(
        newest.file_name().and_then(|n| n.to_str()),
        Some("backend.log.2026-09-20")
    );
    // And each log finds its own, never the other's.
    let scraper = newest_log(&dir, Which::Scraper).expect("finds one");
    assert_eq!(
        scraper.file_name().and_then(|n| n.to_str()),
        Some("scraper.log.2026-09-18")
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_log_that_does_not_exist_yet_is_simply_missing() {
    let nowhere = std::env::temp_dir().join("webleads-no-such-folder-ever");
    assert_eq!(newest_log(&nowhere, Which::Backend), None);
}

#[test]
fn a_page_that_has_just_opened_is_shown_nothing_that_happened_before_it() {
    let dir = std::env::temp_dir().join(format!("webleads-tail-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("makes the folder");
    let many: String = (0..200).map(|n| format!("INFO line {n}\n")).collect();
    std::fs::write(dir.join("scraper.log.2026-09-20"), &many).expect("writes");

    // Not one of the two hundred already written: a warning from an hour ago
    // scrolling past as though it were happening now is worse than an empty window.
    let first = read_since(&dir, Which::Scraper, None, false);
    assert!(first.lines.is_empty(), "{:?}", first.lines);
    assert!(first.at > 0, "it still says where to carry on from");

    // Nothing new since: nothing said.
    let again = read_since(&dir, Which::Scraper, Some(first.at), false);
    assert!(again.lines.is_empty());
    assert_eq!(again.at, first.at);

    // And what is written afterwards comes back, on its own.
    std::fs::write(
        dir.join("scraper.log.2026-09-20"),
        format!("{many}INFO fresh\n"),
    )
    .expect("writes");
    let more = read_since(&dir, Which::Scraper, Some(first.at), false);
    assert_eq!(more.lines, vec!["INFO fresh"]);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_log_that_shrank_is_read_from_the_top_rather_than_past_its_end() {
    // It rotated at midnight, or the pruning took it. Seeking to yesterday's offset
    // would read nothing for ever.
    let dir = std::env::temp_dir().join(format!("webleads-rolled-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("makes the folder");
    std::fs::write(dir.join("backend.log.2026-09-20"), "INFO a fresh file\n").expect("writes");

    let part = read_since(&dir, Which::Backend, Some(900_000), false);
    assert_eq!(part.lines, vec!["INFO a fresh file"]);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_window_never_shows_itself_asking_for_the_log() {
    // Left in, it feeds on itself: every answer makes the next one bigger.
    let text = "DEBUG request{method=GET uri=/api/logs?debug=false}: started\n\
                DEBUG request{method=GET uri=/api/logs?debug=false}: finished\n\
                 INFO scraper: searching `fodrász Budapest`\n";
    assert_eq!(
        tidy(text, true),
        vec!["INFO scraper: searching `fodrász Budapest`"]
    );
    assert!(is_our_own_watching(
        "DEBUG request{method=GET uri=/api/logs}: started"
    ));
    assert!(!is_our_own_watching(
        "DEBUG request{method=GET uri=/api/leads}: started"
    ));
}
