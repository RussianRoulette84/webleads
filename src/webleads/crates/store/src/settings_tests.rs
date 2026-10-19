//! Tests for [`crate::settings`].

use super::*;

#[tokio::test]
async fn a_setting_that_was_never_written_is_simply_missing() {
    let store = Store::in_memory().await.expect("opens");
    assert_eq!(store.setting("nothing_here").await.expect("asks"), None);
}

#[tokio::test]
async fn a_setting_survives_being_written_twice() {
    let store = Store::in_memory().await.expect("opens");
    store.set_setting("colour", "brass").await.expect("writes");
    store.set_setting("colour", "ink").await.expect("writes");
    assert_eq!(
        store.setting("colour").await.expect("asks").as_deref(),
        Some("ink")
    );
}

#[tokio::test]
async fn the_queue_starts_unpaused() {
    // A fresh database must not hold the queue for a reason nobody set.
    let store = Store::in_memory().await.expect("opens");
    assert!(!store.queue_paused().await.expect("asks"));
}

#[tokio::test]
async fn pausing_and_letting_it_carry_on_both_stick() {
    let store = Store::in_memory().await.expect("opens");
    store.pause_queue(true).await.expect("pauses");
    assert!(store.queue_paused().await.expect("asks"));
    store.pause_queue(false).await.expect("carries on");
    assert!(!store.queue_paused().await.expect("asks"));
}

#[tokio::test]
async fn a_setting_we_cannot_read_is_not_a_reason_to_stop_working() {
    // Anything other than a plain "yes" means the queue carries on.
    let store = Store::in_memory().await.expect("opens");
    store
        .set_setting(QUEUE_PAUSED, "banana")
        .await
        .expect("writes");
    assert!(!store.queue_paused().await.expect("asks"));
}

#[tokio::test]
async fn searches_are_made_in_english_until_you_pick_another() {
    let store = Store::in_memory().await.expect("opens");
    assert_eq!(store.search_lang().await.expect("reads"), "en");
}

#[tokio::test]
async fn the_language_you_pick_is_remembered() {
    let store = Store::in_memory().await.expect("opens");
    store.set_search_lang(" HU ").await.expect("keeps");
    assert_eq!(store.search_lang().await.expect("reads"), "hu");
}

#[tokio::test]
async fn a_language_the_trade_list_is_not_written_in_is_refused() {
    let store = Store::in_memory().await.expect("opens");
    assert!(matches!(
        store.set_search_lang("xx").await,
        Err(crate::Error::Refused(_))
    ));
    assert_eq!(store.search_lang().await.expect("reads"), "en");
}

#[tokio::test]
async fn your_clock_starts_on_budapest_and_keeps_what_you_pick() {
    let store = Store::in_memory().await.expect("opens");
    assert_eq!(store.your_zone().await.expect("reads"), "Europe/Budapest");
    store.set_your_zone(" Asia/Dubai ").await.expect("keeps");
    assert_eq!(store.your_zone().await.expect("reads"), "Asia/Dubai");
}

#[tokio::test]
async fn a_clock_nobody_keeps_is_refused() {
    let store = Store::in_memory().await.expect("opens");
    assert!(matches!(
        store.set_your_zone("Mars/Olympus").await,
        Err(crate::Error::Refused(_))
    ));
    assert_eq!(store.your_zone().await.expect("reads"), "Europe/Budapest");
}
