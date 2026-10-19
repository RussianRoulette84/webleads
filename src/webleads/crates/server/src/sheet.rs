//! The call list as a real spreadsheet file.
//!
//! # What this file is here to teach
//!
//! Keeping the *decisions* — which columns, and what kind of value each one holds —
//! apart from the library that writes the bytes. The decisions are pure and tested;
//! the writing is a few lines that hand them to `rust_xlsxwriter`.
//!
//! A number stored as text is the whole reason this exists: the CSV sorts 1000 before
//! 99 inside Excel, because to Excel they were words.

use rust_xlsxwriter::{Format, Workbook, XlsxError};
use webleads_store::StoredLead;

/// One value in the sheet, and what Excel should think it is.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// Words. Also a telephone number — `06 30 …` must keep its leading zero.
    Text(String),
    /// A real number, so sorting and summing inside Excel work.
    Number(f64),
    /// Nothing known. An empty cell, not the word "none".
    Empty,
}

/// The columns, in order. Longer than the CSV's: this one carries the pitch line,
/// the rating and whether the business is new, none of which the CSV knew.
pub const HEADINGS: [&str; 27] = [
    "Name",
    "Phone",
    "Why",
    "Detail",
    "Certificate",
    "Trade",
    "Address",
    "Reviews",
    "Rating",
    "Distance (m)",
    "Open today",
    // A column per day. Today alone is no use to somebody reading this tomorrow.
    "Mon",
    "Tue",
    "Wed",
    "Thu",
    "Fri",
    "Sat",
    "Sun",
    "What to say",
    "Status",
    // How many times nobody answered, and when you said you would ring back.
    "No answers",
    "Ring back",
    "Calls",
    "Last called",
    "Notes",
    "First seen",
    "Maps",
];

/// How wide each column starts out, in characters.
const WIDTHS: [f64; 27] = [
    30.0, 16.0, 14.0, 30.0, 18.0, 18.0, 34.0, 9.0, 7.0, 12.0, 14.0, 13.0, 13.0, 13.0, 13.0, 13.0,
    13.0, 13.0, 36.0, 12.0, 11.0, 17.0, 7.0, 12.0, 30.0, 11.0, 10.0,
];

/// One day's hours out of the week, Monday being nought.
fn day(hours: &webleads_domain::Hours, which: usize) -> Option<String> {
    hours.day_lines().into_iter().nth(which).flatten()
}

/// One business as a row of cells.
///
/// Pure, so what lands in each column can be checked without writing a file.
pub fn sheet_row(row: &StoredLead) -> Vec<Cell> {
    let lead = &row.lead;

    let text = |value: &str| {
        if value.is_empty() {
            Cell::Empty
        } else {
            Cell::Text(value.to_owned())
        }
    };

    vec![
        Cell::Text(lead.name.clone()),
        // A word rather than TRUE/FALSE: this is read by a person, not a formula.
        // Text on purpose. As a number, `06 30 555 0104` loses its leading zero and
        // becomes a telephone number nobody can ring.
        text(lead.phone.as_deref().unwrap_or("")),
        Cell::Text(lead.verdict.describe().to_owned()),
        text(&lead.reason),
        text(lead.tls.describe()),
        text(lead.shown_trade().unwrap_or("")),
        text(lead.address.as_deref().unwrap_or("")),
        Cell::Number(f64::from(lead.reviews)),
        lead.rating
            .map_or(Cell::Empty, |r| Cell::Number(f64::from(r))),
        Cell::Number(f64::from(lead.distance_m)),
        text(row.hours_today.as_deref().unwrap_or("")),
        // Monday to Sunday, each in its own column, so a Saturday can be sorted on.
        text(day(&lead.hours, 0).as_deref().unwrap_or("")),
        text(day(&lead.hours, 1).as_deref().unwrap_or("")),
        text(day(&lead.hours, 2).as_deref().unwrap_or("")),
        text(day(&lead.hours, 3).as_deref().unwrap_or("")),
        text(day(&lead.hours, 4).as_deref().unwrap_or("")),
        text(day(&lead.hours, 5).as_deref().unwrap_or("")),
        text(day(&lead.hours, 6).as_deref().unwrap_or("")),
        Cell::Text(lead.pitch()),
        Cell::Text(row.contact.status.describe().to_owned()),
        Cell::Number(row.contact.not_reached_counter as f64),
        text(row.contact.follow_up_tstamp.as_deref().unwrap_or("")),
        Cell::Number(row.call_count as f64),
        text(row.contact.called_on.as_deref().unwrap_or("")),
        text(&row.contact.note),
        text(&row.first_seen),
        text(lead.maps_url.as_deref().unwrap_or("")),
    ]
}

/// The whole list as the bytes of an `.xlsx` file.
///
/// The heading row is frozen and carries Excel's own filter arrows, so a thousand
/// rows are still usable after you have scrolled past the top.
pub fn to_xlsx(leads: &[StoredLead]) -> Result<Vec<u8>, XlsxError> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet().set_name("Leads")?;

    let heading = Format::new().set_bold();
    for (column, (title, width)) in HEADINGS.iter().zip(WIDTHS).enumerate() {
        let column = column as u16;
        sheet.write_string_with_format(0, column, *title, &heading)?;
        sheet.set_column_width(column, width)?;
    }

    for (index, row) in leads.iter().enumerate() {
        // Row 0 is the headings, so the first business is row 1.
        let line = index as u32 + 1;
        for (column, cell) in sheet_row(row).into_iter().enumerate() {
            let column = column as u16;
            match cell {
                Cell::Text(value) => sheet.write_string(line, column, value)?,
                Cell::Number(value) => sheet.write_number(line, column, value)?,
                Cell::Empty => sheet,
            };
        }
    }

    sheet.set_freeze_panes(1, 0)?;
    let last_row = leads.len() as u32;
    let last_column = HEADINGS.len() as u16 - 1;
    sheet.autofilter(0, 0, last_row.max(1), last_column)?;

    workbook.save_to_buffer()
}

/// What the downloaded file is called.
pub fn file_name(want: &webleads_store::Wanted) -> String {
    crate::routes::export::file_name(want).replace(".csv", ".xlsx")
}

#[cfg(test)]
#[path = "sheet_tests.rs"]
mod tests;
