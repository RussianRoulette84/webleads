//! Tests for [`crate::link`].

use super::*;

fn health(scraper: bool) -> Result<api::Health, String> {
    Ok(api::Health {
        scraper,
        scraper_detail: None,
        machine: crate::terminal::Machine {
            user: "caller".to_owned(),
            host: "workstation".to_owned(),
            dir: "~/webleads".to_owned(),
        },
    })
}

#[test]
fn both_answering_is_online_and_can_scrape() {
    let link = link_of(&health(true));
    assert_eq!(link, Link::Online);
    assert!(link.can_scrape());
    assert_eq!(link.command(), None);
}

#[test]
fn a_silent_scraper_says_how_to_start_the_scraper() {
    let link = link_of(&health(false));
    assert_eq!(link, Link::ScraperOffline);
    assert!(!link.can_scrape());
    assert_eq!(link.command(), Some(START_SCRAPER));
}

#[test]
fn a_silent_server_says_how_to_start_everything() {
    let link = link_of(&Err("Load failed".to_owned()));
    assert_eq!(link, Link::ServerOffline);
    assert!(!link.can_scrape());
    assert_eq!(link.command(), Some(START_ALL));
}

#[test]
fn nothing_is_pressable_before_the_first_answer() {
    assert!(!Link::Checking.can_scrape());
    assert_eq!(Link::Checking.command(), None);
}

#[test]
fn the_light_says_online_or_offline() {
    assert_eq!(Link::Online.said(), "SCRAPER ONLINE");
    assert_eq!(Link::ScraperOffline.said(), "SCRAPER OFFLINE");
}
