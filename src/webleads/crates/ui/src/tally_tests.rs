//! Tests for [`crate::tally`] — the scoreboard's arithmetic.

use super::*;

#[test]
fn the_scoreboard_leaves_out_the_pile_you_have_not_started() {
    let shown = worth_counting();
    assert!(!shown.contains(&Status::NeedCall));
    assert_eq!(shown.len(), 6);
}

#[test]
fn the_order_is_the_order_the_chips_use() {
    let shown: Vec<&str> = worth_counting().iter().map(|s| s.describe()).collect();
    assert_eq!(
        shown,
        vec![
            "no answer",
            "ring back",
            "they'll call",
            "sold",
            "said no",
            "line dead"
        ]
    );
}

#[test]
fn a_state_the_server_left_out_reads_nought() {
    let counted = vec![("sold".to_owned(), 3)];
    assert_eq!(how_many(&counted, Status::Sold), 3);
    assert_eq!(how_many(&counted, Status::Dead), 0);
}

#[test]
fn a_word_we_do_not_know_is_not_mistaken_for_one_we_do() {
    let counted = vec![("sold_out".to_owned(), 9)];
    assert_eq!(how_many(&counted, Status::Sold), 0);
}
