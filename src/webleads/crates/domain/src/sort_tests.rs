//! Tests for [`crate::sort`].

use super::*;

#[test]
fn the_default_is_the_nearest_first() {
    // A morning of calling is a morning of driving, and that is the judgement the
    // list opens with.
    assert_eq!(Sort::default(), Sort::Distance);
}

#[test]
fn every_column_survives_being_written_down_and_read_back() {
    for column in Sort::ALL {
        assert_eq!(column.as_str().parse::<Sort>().expect("reads back"), column);
    }
}

#[test]
fn a_column_nobody_has_heard_of_is_refused() {
    // The page builds these from its own headings, so an unknown one means a bug
    // here rather than a caller doing something odd — the caller falls back to the
    // default order.
    assert!("banana".parse::<Sort>().is_err());
    assert!("".parse::<Sort>().is_err());
}

#[test]
fn an_attempt_to_smuggle_sql_in_is_simply_not_a_column() {
    // The whole reason this is an enum. There is no way to hold a value we did not
    // write down, so the dangerous version is not discouraged — it cannot be built.
    for nasty in [
        "name; DROP TABLE lead",
        "name' OR '1'='1",
        "reviews, (SELECT 1)",
        "1=1--",
    ] {
        assert!(nasty.parse::<Sort>().is_err(), "`{nasty}` was accepted");
    }
}

#[test]
fn the_shown_word_is_not_the_stored_word() {
    // Rewording a heading must not change what a saved address means.
    for column in Sort::ALL {
        assert_ne!(column.as_str(), column.heading(), "{column:?}");
    }
}

#[test]
fn every_column_appears_exactly_once() {
    assert_eq!(Sort::ALL.len(), 10);
    for column in Sort::ALL {
        assert_eq!(
            Sort::ALL.iter().filter(|s| **s == column).count(),
            1,
            "{column:?} is listed more than once"
        );
    }
}

#[test]
fn no_two_columns_share_a_word() {
    // Two columns with the same name would make one of them unreachable.
    let mut words: Vec<&str> = Sort::ALL.iter().map(|s| s.as_str()).collect();
    words.sort_unstable();
    let before = words.len();
    words.dedup();
    assert_eq!(words.len(), before);
}

#[test]
fn numbers_open_with_their_biggest_first() {
    // You click "Reviews" to see the busiest, not the quietest. Getting this the
    // wrong way round means every column needs two clicks to be useful.
    assert!(Sort::Reviews.biggest_first());
    assert!(Sort::FirstSeen.biggest_first(), "newest first");
}

#[test]
fn words_and_distance_open_the_other_way() {
    // A to Z for names, and nearest first for distance — the near ones are the ones
    // you can visit.
    assert!(!Sort::Name.biggest_first());
    assert!(!Sort::Trade.biggest_first());
    assert!(!Sort::Distance.biggest_first());
}

#[test]
fn every_column_has_a_heading_worth_reading() {
    for column in Sort::ALL {
        let heading = column.heading();
        assert!(!heading.is_empty());
        assert!(
            heading.chars().next().expect("a letter").is_uppercase(),
            "{heading} should read like a heading"
        );
    }
}
