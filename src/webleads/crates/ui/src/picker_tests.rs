//! Tests for [`crate::picker`].

use super::*;

fn a_trade(english: &str, hu: &[&str], tier: Tier) -> Target {
    in_languages(english, &[("hu", hu)], tier)
}

/// A trade with words in whichever languages a test needs.
fn in_languages(english: &str, words: &[(&str, &[&str])], tier: Tier) -> Target {
    Target {
        rank: 1,
        tier,
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
        budget: 4,
        note: String::new(),
    }
}

#[test]
fn typing_hungarian_finds_the_trade() {
    // The whole point: you think in the word Google will be asked, not in English.
    let plumbers = a_trade("Plumbers", &["vízvezeték-szerelő"], Tier::S);
    assert!(matches(&plumbers, "szerel"));
    assert!(matches(&plumbers, "VÍZVEZETÉK"));
}

#[test]
fn typing_english_finds_it_too() {
    let plumbers = a_trade("Plumbers", &["vízvezeték-szerelő"], Tier::S);
    assert!(matches(&plumbers, "plumb"));
    assert!(matches(&plumbers, "  Plumbers  "));
}

#[test]
fn something_that_matches_neither_language_is_left_out() {
    let plumbers = a_trade("Plumbers", &["vízvezeték-szerelő"], Tier::S);
    assert!(!matches(&plumbers, "dentist"));
}

#[test]
fn an_empty_box_shows_everything() {
    let plumbers = a_trade("Plumbers", &["vízvezeték-szerelő"], Tier::S);
    assert!(matches(&plumbers, ""));
    assert!(matches(&plumbers, "   "));
}

#[test]
fn a_trade_with_two_words_is_two_searches() {
    // Twenty trades is twenty-seven searches, and it is the searches that cost the
    // hour — so the count on screen has to be of searches.
    let targets = vec![
        a_trade("Plumbers", &["vízvezeték-szerelő"], Tier::S),
        a_trade(
            "Heating & cooling",
            &["klímaszerelő", "fűtésszerelő"],
            Tier::S,
        ),
    ];
    let chosen = vec!["Plumbers".to_owned(), "Heating & cooling".to_owned()];
    assert_eq!(how_much(&targets, &chosen, &[], "hu"), (3, 2));
}

#[test]
fn your_own_words_count_as_searches_too() {
    let targets = vec![a_trade("Plumbers", &["vízvezeték-szerelő"], Tier::S)];
    let own = vec!["kandallós Zugló".to_owned()];
    assert_eq!(
        how_much(&targets, &["Plumbers".to_owned()], &own, "hu"),
        (2, 1)
    );
}

#[test]
fn choosing_nothing_costs_nothing() {
    let targets = vec![a_trade("Plumbers", &["vízvezeték-szerelő"], Tier::S)];
    assert_eq!(how_much(&targets, &[], &[], "hu"), (0, 0));
    assert_eq!(describe_choice(0, 0, 0), "nothing chosen yet");
}

#[test]
fn the_count_reads_as_a_sentence() {
    assert_eq!(describe_choice(27, 20, 0), "27 searches across 20 trades");
    assert_eq!(describe_choice(1, 1, 0), "1 search across 1 trade");
    assert_eq!(
        describe_choice(3, 1, 2),
        "3 searches across 1 trade, 2 of your own"
    );
}

#[test]
fn words_of_your_own_alone_still_read_properly() {
    assert_eq!(describe_choice(2, 0, 2), "2 searches, 2 of your own");
}

#[test]
fn the_panel_says_where_it_will_look_and_in_what_language() {
    assert_eq!(
        searching_said("Budapest", "hu"),
        "Searching Budapest in Hungarian"
    );
    assert_eq!(
        searching_said("Downtown Dubai", "en"),
        "Searching Downtown Dubai in English"
    );
}

#[test]
fn a_language_we_have_no_name_for_is_said_by_its_code() {
    assert_eq!(searching_said("Warsaw", "pl"), "Searching Warsaw in pl");
}

#[test]
fn a_place_with_no_language_says_so_rather_than_reading_as_hungarian() {
    // The fault this line replaces: for a while every city searched in Hungarian and
    // nothing on the screen said so.
    let said = searching_said("Deira", "");
    assert!(said.contains("Deira"), "{said}");
    assert!(said.contains("not set"), "{said}");
    assert!(!said.contains("Hungarian"), "{said}");
}

#[test]
fn a_tier_gives_every_trade_in_it_in_list_order() {
    let list = [
        a_trade("Plumbers", &["vízvezeték-szerelő"], Tier::S),
        a_trade("Barbers", &["borbély"], Tier::B),
        a_trade("Electricians", &["villanyszerelő"], Tier::S),
    ];
    assert_eq!(
        tier_names(&list, Tier::S, "hu"),
        vec!["Plumbers", "Electricians"]
    );
    assert!(tier_names(&list, Tier::C, "hu").is_empty());
}

#[test]
fn a_tier_ticks_only_what_can_be_searched_in_the_language() {
    // The count on the button and the ticks it makes must agree.
    let list = [
        a_trade("Plumbers", &["vízvezeték-szerelő"], Tier::S),
        in_languages("Abaya tailors", &[("en", &["abaya tailor"])], Tier::S),
    ];
    assert_eq!(tier_names(&list, Tier::S, "hu"), vec!["Plumbers"]);
    assert_eq!(tier_names(&list, Tier::S, "en"), vec!["Abaya tailors"]);
}

#[test]
fn all_the_tiers_together_are_every_trade_you_can_search() {
    let list = [
        a_trade("Plumbers", &["vízvezeték-szerelő"], Tier::S),
        a_trade("Roofers", &["tetőfedő"], Tier::A),
        a_trade("Bakers", &["pék"], Tier::B),
        a_trade("Barbers", &["borbély"], Tier::C),
    ];
    let all: Vec<String> = TIERS
        .iter()
        .flat_map(|tier| tier_names(&list, *tier, "hu"))
        .collect();
    assert_eq!(all.len(), searchable_in(&list, "hu").len());
}
