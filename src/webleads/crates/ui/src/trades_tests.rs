//! Tests for [`crate::trades`].

use super::*;

fn plumbers() -> Target {
    Target {
        rank: 1,
        tier: Tier::S,
        english: "Plumbers".to_owned(),
        words: [
            ("hu".to_owned(), vec!["vízvezeték-szerelő".to_owned()]),
            (
                "en".to_owned(),
                vec!["plumber".to_owned(), "plumbing works".to_owned()],
            ),
        ]
        .into_iter()
        .collect(),
        no_site_odds: 4,
        budget: 4,
        note: "Emergency work".to_owned(),
    }
}

#[test]
fn editing_a_name_trims_it() {
    let mut row = plumbers();
    apply(&mut row, Change::Name("  Plumbing  ".to_owned()));
    assert_eq!(row.english, "Plumbing");
}

#[test]
fn semicolons_split_a_cell_of_words() {
    let mut row = plumbers();
    apply(
        &mut row,
        Change::Words("en", "plumber; drain cleaning ;;".to_owned()),
    );
    assert_eq!(row.words("en"), vec!["plumber", "drain cleaning"]);
}

#[test]
fn emptying_a_cell_means_the_trade_is_not_searched_in_that_language() {
    // Not an empty list under the language — the absence of the language, which is
    // what the file writes as a blank column and the picker reads as "not offered".
    let mut row = plumbers();
    apply(&mut row, Change::Words("en", "   ".to_owned()));
    assert!(!row.searched_in("en"));
    assert!(row.searched_in("hu"), "the other language is untouched");
    let langs: Vec<&str> = row.languages().collect();
    assert_eq!(langs, vec!["hu"]);
}

#[test]
fn a_number_that_is_not_a_number_leaves_the_field_as_it_was() {
    // Otherwise the field empties itself while you are halfway through typing in it.
    let mut row = plumbers();
    apply(&mut row, Change::Rank("".to_owned()));
    assert_eq!(row.rank, 1);
    apply(&mut row, Change::Odds("lots".to_owned()));
    assert_eq!(row.no_site_odds, 4);
}

#[test]
fn the_two_scores_are_held_between_one_and_five() {
    let mut row = plumbers();
    apply(&mut row, Change::Odds("9".to_owned()));
    assert_eq!(row.no_site_odds, 5);
    apply(&mut row, Change::Budget("0".to_owned()));
    assert_eq!(row.budget, 1);
}

#[test]
fn a_tier_letter_we_do_not_know_leaves_the_tier_alone() {
    let mut row = plumbers();
    apply(&mut row, Change::Tier("A".to_owned()));
    assert_eq!(row.tier, Tier::A);
    apply(&mut row, Change::Tier("Z".to_owned()));
    assert_eq!(row.tier, Tier::A);
}

#[test]
fn a_new_trade_starts_with_scores_the_list_allows() {
    // Nought is not a number the list allows, and a new row the server refuses the
    // moment you press save is a row that reads as broken.
    let fresh = blank_trade(101, "Locksmiths".to_owned());
    assert_eq!(fresh.rank, 101);
    assert!((1..=5).contains(&fresh.no_site_odds));
    assert!((1..=5).contains(&fresh.budget));
    assert_eq!(fresh.english, "Locksmiths");
}

#[test]
fn a_trade_with_no_name_cannot_be_added() {
    for blank in ["", "   ", "\t"] {
        assert!(new_trade_name(blank, &[plumbers()]).is_err(), "{blank:?}");
    }
}

#[test]
fn a_trade_already_on_the_list_cannot_be_added_again() {
    let existing = [plumbers()];
    let name = existing[0].english.to_uppercase();
    assert!(new_trade_name(&name, &existing).is_err());
}

#[test]
fn a_new_name_is_added_tidied() {
    assert_eq!(
        new_trade_name("  Locksmiths ", &[plumbers()]),
        Ok("Locksmiths".to_owned())
    );
}

#[test]
fn narrowing_matches_a_name_a_word_or_a_note() {
    let row = plumbers();
    assert!(shows(&row, "plumb"), "the English name");
    assert!(shows(&row, "szerel"), "the Hungarian word");
    assert!(shows(&row, "emergency"), "the note");
    assert!(shows(&row, "PLUMB"), "whatever the capitals");
    assert!(!shows(&row, "barber"));
}

#[test]
fn an_empty_narrowing_shows_everything() {
    assert!(shows(&plumbers(), ""));
    assert!(shows(&plumbers(), "   "));
}

#[test]
fn the_count_says_the_searches_not_only_the_trades() {
    // The number that costs the afternoon. One trade here, three searches.
    let said = counted(&[plumbers()]);
    assert!(said.contains("1 trades"), "{said}");
    assert!(said.contains("1 Hungarian"), "{said}");
    assert!(said.contains("2 English"), "{said}");
}

#[test]
fn an_empty_list_still_counts_itself() {
    let said = counted(&[]);
    assert!(said.contains("0 trades"), "{said}");
}

#[test]
fn a_save_says_how_many_went_to_the_file() {
    assert_eq!(saved_said(104), "104 trades written to the file");
}
