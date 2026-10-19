//! Tests for [`crate::sheet`] — what lands in each column, and in what form.

use super::*;
use webleads_domain::{Contact, Lead, Status, Tls, Verdict};

fn a_row() -> StoredLead {
    StoredLead {
        lead: Lead {
            id: "a".into(),
            name: "Borzas Barber".into(),
            phone: Some("06 30 555 0104".into()),
            category: Some("Barber shop".into()),
            trade: Some("Barbers".into()),
            hours: Default::default(),
            address: Some("Hősök tere 1".into()),
            maps_url: Some("https://maps.google.com/?cid=1".into()),
            website: None,
            social_url: None,
            verdict: Verdict::NoSite,
            reason: "no link on Google Maps".into(),
            reviews: 1_000,
            rating: Some(4.6),
            distance_m: 240,
            position: Some((47.5149, 19.0778)),
            tls: Tls::Unknown,
        },
        first_seen: "2026-09-01".into(),
        last_seen: "2026-09-19".into(),
        contact: Contact {
            status: Status::NotReached,
            note: "ask for the owner".into(),
            called_on: Some("2026-09-19".into()),
            not_reached_counter: 2,
            follow_up_tstamp: None,
        },
        call_count: 2,
        hours_today: Some("10:00–20:00".into()),
        hours_there: None,
        city_lang: None,
    }
}

#[test]
fn every_row_has_a_cell_for_every_heading() {
    // A row that has drifted from the headings puts your notes under "Maps".
    assert_eq!(sheet_row(&a_row()).len(), HEADINGS.len());
}

#[test]
fn reviews_are_a_number_so_excel_sorts_them_properly() {
    // The whole reason this file exists: as text, 1000 sorts before 99.
    let row = sheet_row(&a_row());
    let reviews = HEADINGS
        .iter()
        .position(|h| *h == "Reviews")
        .expect("column");
    assert_eq!(row[reviews], Cell::Number(1_000.0));
}

#[test]
fn the_telephone_number_stays_text_so_it_keeps_its_leading_zero() {
    let row = sheet_row(&a_row());
    let phone = HEADINGS.iter().position(|h| *h == "Phone").expect("column");
    assert_eq!(row[phone], Cell::Text("06 30 555 0104".into()));
}

#[test]
fn a_missing_rating_is_an_empty_cell_rather_than_a_zero() {
    // A zero would read as "rated nought out of five", which is a lie.
    let mut unrated = a_row();
    unrated.lead.rating = None;
    let row = sheet_row(&unrated);
    let rating = HEADINGS
        .iter()
        .position(|h| *h == "Rating")
        .expect("column");
    assert_eq!(row[rating], Cell::Empty);
}

#[test]
fn what_to_say_on_the_telephone_is_in_there() {
    // The line the CSV never carried, and the one you actually read aloud.
    let row = sheet_row(&a_row());
    let pitch = HEADINGS
        .iter()
        .position(|h| *h == "What to say")
        .expect("column");
    assert!(matches!(row[pitch], Cell::Text(ref t) if !t.is_empty()));
}

#[test]
fn your_own_note_is_in_there_too() {
    let row = sheet_row(&a_row());
    let notes = HEADINGS.iter().position(|h| *h == "Notes").expect("column");
    assert_eq!(row[notes], Cell::Text("ask for the owner".into()));
}

#[test]
fn the_file_is_a_real_spreadsheet() {
    // An .xlsx is a zip, and every zip starts with these two letters. A file that
    // does not is one Excel refuses to open.
    let bytes = to_xlsx(&[a_row(), a_row()]).expect("writes");
    assert_eq!(&bytes[..2], b"PK");
    assert!(bytes.len() > 1_000, "suspiciously small: {}", bytes.len());
}

#[test]
fn an_empty_list_still_writes_a_file_with_its_headings() {
    // Downloading an empty filter should give you an empty sheet, not an error.
    let bytes = to_xlsx(&[]).expect("writes");
    assert_eq!(&bytes[..2], b"PK");
}

#[test]
fn the_name_says_it_is_a_spreadsheet() {
    let want = webleads_store::Wanted::default();
    assert!(file_name(&want).ends_with(".xlsx"));
    assert!(!file_name(&want).contains(".csv"));
}

#[test]
fn the_spreadsheet_has_a_column_for_every_day() {
    // The point of the whole change: a file read tomorrow morning has to say what
    // happens tomorrow, not what happened when it was made.
    for day in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
        assert!(HEADINGS.contains(&day), "no column for {day}");
    }
}

#[test]
fn each_day_lands_in_its_own_column() {
    let mut row = a_row();
    row.lead.hours =
        webleads_domain::Hours::parse(r#"{"Monday":["08:00–17:00"],"Sunday":["Zárva"]}"#);
    let cells = sheet_row(&row);

    let monday = HEADINGS.iter().position(|h| *h == "Mon").expect("column");
    let sunday = HEADINGS.iter().position(|h| *h == "Sun").expect("column");
    let wednesday = HEADINGS.iter().position(|h| *h == "Wed").expect("column");

    assert_eq!(cells[monday], Cell::Text("08:00–17:00".into()));
    assert_eq!(cells[sunday], Cell::Text("closed".into()));
    // Nothing was said about Wednesday, and an empty cell is the honest answer.
    assert_eq!(cells[wednesday], Cell::Empty);
}
