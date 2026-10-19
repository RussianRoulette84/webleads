//! Tests for the two anchors every path in the program is measured from.

use super::*;
use std::path::Path;

#[test]
fn the_workspace_is_two_steps_above_the_server_crate() {
    let manifest = Path::new("/anywhere/webleads/src/webleads/crates/server");
    assert_eq!(
        workspace_root_from(manifest).expect("has somewhere to go"),
        Path::new("/anywhere/webleads/src/webleads")
    );
}

#[test]
fn the_repository_is_two_steps_above_the_workspace() {
    let workspace = Path::new("/anywhere/webleads/src/webleads");
    assert_eq!(
        repo_root_from(workspace).expect("has somewhere to go"),
        Path::new("/anywhere/webleads")
    );
}

#[test]
fn a_workspace_with_nowhere_to_go_is_an_error_not_a_guess() {
    // Better a refusal than silently deciding the root is `/` and writing there.
    assert!(workspace_root_from(Path::new("/")).is_err());
    assert!(workspace_root_from(Path::new("/crates")).is_err());
}

#[test]
fn a_repository_with_nowhere_to_go_is_an_error_not_a_guess() {
    assert!(repo_root_from(Path::new("/")).is_err());
    assert!(repo_root_from(Path::new("/webleads")).is_err());
}

#[test]
fn the_scraper_traffic_is_written_down_by_default() {
    // The detail lines were silently thrown away for a while: they were written as
    // debug, and the filter let nothing below info through for that target. A test,
    // because nothing else about the program notices when they stop appearing.
    assert!(DEFAULT_LOGGING.contains("scraper=debug"));
    assert!(EnvFilter::try_new(DEFAULT_LOGGING).is_ok());
}
