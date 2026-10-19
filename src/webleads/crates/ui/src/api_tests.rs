//! Tests for [`crate::api`].

use super::*;

#[test]
fn the_page_s_own_background_checks_stay_out_of_the_window() {
    for quiet in [
        "GET /api/logs?backend=1",
        "GET /api/runs/current",
        "GET /api/tally",
    ] {
        assert!(!shown_in_window(quiet), "{quiet}");
    }
}

#[test]
fn everything_you_caused_is_written_down() {
    for loud in [
        "POST /api/runs",
        "POST /api/runs/:id/stop",
        "GET /api/leads",
        "POST /api/search_lang",
    ] {
        assert!(shown_in_window(loud), "{loud}");
    }
}

#[test]
fn an_unreachable_server_says_so_and_how_to_start_it() {
    let said = unreachable_said();
    assert!(said.contains("offline"), "{said}");
    assert!(said.contains(crate::link::START_ALL), "{said}");
    assert!(!said.contains("TypeError"), "{said}");
}

#[test]
fn the_health_check_every_few_seconds_stays_out_of_the_window() {
    assert!(!shown_in_window("GET /api/health"));
}
