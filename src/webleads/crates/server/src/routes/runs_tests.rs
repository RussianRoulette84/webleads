//! Tests for [`crate::routes::runs`] — what a run actually searches for.

use super::*;

/// A few trades across the tiers, as `research/TARGETS.csv` holds them.
fn some_trades() -> Vec<Target> {
    vec![
        Target {
            rank: 1,
            tier: Tier::S,
            english: "Plumbers".into(),
            words: [("hu".to_owned(), vec!["vízvezeték-szerelő".to_owned()])]
                .into_iter()
                .collect(),
            no_site_odds: 4,
            budget: 4,
            note: String::new(),
        },
        Target {
            rank: 2,
            tier: Tier::S,
            english: "Electricians".into(),
            words: [("hu".to_owned(), vec!["villanyszerelő".to_owned()])]
                .into_iter()
                .collect(),
            no_site_odds: 4,
            budget: 4,
            note: String::new(),
        },
        Target {
            rank: 40,
            tier: Tier::B,
            english: "Barbers".into(),
            // Two words for one trade — both real, and they find different shops.
            words: [(
                "hu".to_owned(),
                vec!["borbély".to_owned(), "barbershop".to_owned()],
            )]
            .into_iter()
            .collect(),
            no_site_odds: 3,
            budget: 2,
            note: String::new(),
        },
    ]
}

fn asking(tiers: &[&str], trades: &[&str], words: &[&str]) -> StartRequest {
    StartRequest {
        tiers: tiers.iter().map(|s| (*s).to_owned()).collect(),
        trades: trades.iter().map(|s| (*s).to_owned()).collect(),
        words: words.iter().map(|s| (*s).to_owned()).collect(),
        options: None,
    }
}

#[test]
fn a_whole_tier_brings_in_every_trade_in_it() {
    let chosen = chosen_words(&some_trades(), &asking(&["S"], &[], &[]), "Budapest", "hu");
    assert_eq!(chosen.len(), 2);
    assert!(chosen
        .iter()
        .any(|(p, _)| p.starts_with("vízvezeték-szerelő")));
    assert!(chosen.iter().any(|(p, _)| p.starts_with("villanyszerelő")));
}

#[test]
fn a_trade_with_two_hungarian_words_becomes_two_searches() {
    // Both are real and they find different shops, which is why the file holds both.
    let chosen = chosen_words(
        &some_trades(),
        &asking(&[], &["Barbers"], &[]),
        "Budapest",
        "hu",
    );
    assert_eq!(chosen.len(), 2);
    assert!(chosen.iter().any(|(p, _)| p == "borbély Budapest"));
    assert!(chosen.iter().any(|(p, _)| p == "barbershop Budapest"));
}

#[test]
fn the_place_is_put_on_the_end_of_every_word() {
    let chosen = chosen_words(&some_trades(), &asking(&["S"], &[], &[]), "Budapest", "hu");
    assert!(chosen.iter().all(|(p, _)| p.ends_with("Budapest")));
}

#[test]
fn the_trade_travels_with_the_word_so_the_column_can_be_filled() {
    let chosen = chosen_words(
        &some_trades(),
        &asking(&[], &["Barbers"], &[]),
        "Budapest",
        "hu",
    );
    assert!(chosen
        .iter()
        .all(|(_, trade)| trade.as_deref() == Some("Barbers")));
}

#[test]
fn a_word_you_typed_yourself_has_no_trade_claimed_for_it() {
    // We did not put it on the list, so we cannot pretend to know what it is — and a
    // wrong trade is worse than a blank one.
    let chosen = chosen_words(
        &some_trades(),
        &asking(&[], &[], &["kutyakozmetika"]),
        "Budapest",
        "hu",
    );
    assert_eq!(chosen.len(), 1);
    assert_eq!(chosen[0].0, "kutyakozmetika");
    assert_eq!(chosen[0].1, None);
}

#[test]
fn your_own_words_are_searched_exactly_as_typed() {
    // You may well want a phrase we would not have built — the place is already in it,
    // or it is deliberately broader.
    let chosen = chosen_words(
        &some_trades(),
        &asking(&[], &[], &["fodrászat Zugló 14. kerület"]),
        "Budapest",
        "hu",
    );
    assert_eq!(chosen[0].0, "fodrászat Zugló 14. kerület");
}

#[test]
fn an_empty_typed_word_is_ignored() {
    let chosen = chosen_words(
        &some_trades(),
        &asking(&[], &[], &["", "   "]),
        "Budapest",
        "hu",
    );
    assert!(chosen.is_empty());
}

#[test]
fn picking_a_trade_and_its_tier_still_searches_it_once() {
    // The obvious way to double an hour and a half of searching by accident.
    let both = asking(&["S"], &["Plumbers"], &[]);
    let chosen = chosen_words(&some_trades(), &both, "Budapest", "hu");
    let plumbing = chosen
        .iter()
        .filter(|(p, _)| p.starts_with("vízvezeték-szerelő"))
        .count();
    assert_eq!(plumbing, 1, "searched the same word twice");
}

#[test]
fn several_tiers_can_be_asked_for_at_once() {
    let chosen = chosen_words(
        &some_trades(),
        &asking(&["S", "B"], &[], &[]),
        "Budapest",
        "hu",
    );
    assert_eq!(chosen.len(), 4); // two S trades, one B trade with two words
}

#[test]
fn a_tier_letter_we_do_not_recognise_is_simply_ignored() {
    let chosen = chosen_words(&some_trades(), &asking(&["Z"], &[], &[]), "Budapest", "hu");
    assert!(chosen.is_empty());
}

#[test]
fn a_trade_name_matches_whatever_the_capitals() {
    let chosen = chosen_words(
        &some_trades(),
        &asking(&[], &["plumbers"], &[]),
        "Budapest",
        "hu",
    );
    assert_eq!(chosen.len(), 1);
}

#[test]
fn asking_for_nothing_gives_nothing_to_search() {
    // The route turns this into a refusal rather than starting an empty run.
    let chosen = chosen_words(&some_trades(), &asking(&[], &[], &[]), "Budapest", "hu");
    assert!(chosen.is_empty());
}

#[test]
fn tiers_trades_and_your_own_words_all_go_in_together() {
    let everything = asking(&["S"], &["Barbers"], &["kutyakozmetika"]);
    let chosen = chosen_words(&some_trades(), &everything, "Budapest", "hu");
    assert_eq!(chosen.len(), 5); // 2 from S, 2 from Barbers, 1 typed
}

#[test]
fn the_order_is_the_order_they_will_be_searched_in() {
    // A run picked up after a restart works through them by position, so this is not
    // cosmetic — it is what makes carrying on deterministic.
    let chosen = chosen_words(
        &some_trades(),
        &asking(&["S", "B"], &[], &[]),
        "Budapest",
        "hu",
    );
    assert!(chosen[0].0.starts_with("vízvezeték-szerelő"));
    assert!(chosen[1].0.starts_with("villanyszerelő"));
    assert!(chosen[2].0.starts_with("borbély"));
}

#[test]
fn a_search_with_nothing_to_do_says_which_language_it_would_have_asked_in() {
    // "Pick a tier" is no help when you have picked three and none of them has a
    // word in the language this city speaks.
    let said = nothing_to_search("Dubai", "en");
    assert!(said.contains("Dubai"), "{said}");
    assert!(said.contains("en"), "{said}");
    assert!(said.contains("type your own words"), "{said}");
}

#[test]
fn a_trade_with_no_words_in_the_citys_language_contributes_nothing() {
    // The shape of the fault: the Hungarian list searched in Dubai. Asked in English,
    // a Hungarian-only trade offers no words at all rather than its Hungarian ones.
    let chosen = chosen_words(&some_trades(), &asking(&["S"], &[], &[]), "Dubai", "en");
    assert!(chosen.is_empty(), "{chosen:?}");
}
