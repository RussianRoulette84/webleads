//! Reads what the Google Maps scraper produced, and (later) drives it over HTTP.
//!
//! Two halves: [`client`] starts crawls and fetches what they produced, and
//! [`place`] turns the resulting CSV into types the rest of the program understands.
//!
//! # What this crate is here to teach
//!
//! `serde` renaming columns we did not choose, `Option<String>` as the honest way to
//! say "this cell may be empty", and a typed error enum built with `thiserror` so a
//! caller can tell "the file is missing" from "row 14 is malformed".

#![forbid(unsafe_code)]

pub mod client;
pub mod error;
pub mod place;

use std::path::Path;

pub use client::{Client, JobId, RemoteJob, RemoteState};
pub use error::{Error, Result};
pub use place::{Place, RawRow};

/// Read one of the scraper's CSV files into places.
///
/// Rows without a name or a position are skipped rather than failing the whole file —
/// one unusable row out of a hundred should not cost you the other ninety-nine. The
/// count of skipped rows comes back so the caller can log it instead of it vanishing.
pub fn read_csv(path: &Path) -> Result<(Vec<Place>, usize)> {
    // `map_err` turns the io error into ours, attaching the path. The standard error
    // says "no such file"; ours says *which* file, which is the part you need at 2am.
    let file = std::fs::File::open(path).map_err(|cause| Error::Unreadable {
        path: path.to_path_buf(),
        cause,
    })?;
    read_rows(csv::Reader::from_reader(file), path)
}

/// Read rows from wherever they are coming from.
///
/// Shared by the file and the live scrape so the two can never drift — the same
/// columns, the same skipping, the same errors.
fn read_rows<R: std::io::Read>(
    mut reader: csv::Reader<R>,
    path: &Path,
) -> Result<(Vec<Place>, usize)> {
    // Fail early and clearly if this is some other CSV rather than producing a hundred
    // confusing per-row errors.
    let headers = reader.headers().map_err(|cause| Error::BadRow {
        path: path.to_path_buf(),
        row: 0,
        cause,
    })?;
    for needed in ["title", "latitude", "longitude"] {
        if !headers.iter().any(|h| h == needed) {
            return Err(Error::NotScraperCsv {
                path: path.to_path_buf(),
                detail: format!("no `{needed}` column"),
            });
        }
    }

    let mut places = Vec::new();
    let mut skipped = 0usize;

    // `deserialize` hands back one `Result` per row. `enumerate` gives the row number
    // for the error message; +2 because row 1 is the header and humans count from one.
    for (index, row) in reader.deserialize::<RawRow>().enumerate() {
        let row = row.map_err(|cause| Error::BadRow {
            path: path.to_path_buf(),
            row: index + 2,
            cause,
        })?;
        match row.into_place() {
            Some(place) => places.push(place),
            None => skipped += 1,
        }
    }

    Ok((places, skipped))
}

/// Read the scraper's CSV out of a string rather than a file.
///
/// A live scrape hands us the results over HTTP, so there is no file to open. The
/// parsing is the same; only where the bytes come from differs.
pub fn read_csv_from_str(text: &str) -> Result<(Vec<Place>, usize)> {
    read_rows(
        csv::Reader::from_reader(text.as_bytes()),
        Path::new("<the scraper>"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Write a CSV to a temporary file and read it back.
    fn read_from_text(text: &str) -> Result<(Vec<Place>, usize)> {
        let path = std::env::temp_dir().join(format!("webleads-test-{}.csv", text.len()));
        let mut file = std::fs::File::create(&path).expect("temp file");
        file.write_all(text.as_bytes()).expect("write");
        read_csv(&path)
    }

    const FULL: &str = "title,category,address,website,phone,review_count,review_rating,latitude,longitude,cid,place_id,link,status\n\
        Borzas Barber,Barber shop,Hősök tere 1,,+36 1 234 5678,0,,47.5149,19.0778,123,,https://maps.google.com/?cid=123,Open\n";

    /// Fast mode leaves the three identity columns empty.
    const FAST: &str = "title,category,address,website,phone,review_count,review_rating,latitude,longitude,cid,place_id,link,status\n\
        Zuglói Fodrászat,Hair salon,Bosnyák tér 1,,,7,4.5,47.5149,19.0778,,,,\n";

    #[test]
    fn a_normal_row_keeps_its_google_identity() {
        let (places, skipped) = read_from_text(FULL).expect("reads");
        assert_eq!(skipped, 0);
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].id, "123");
        assert_eq!(places[0].name, "Borzas Barber");
        assert_eq!(places[0].phone.as_deref(), Some("+36 1 234 5678"));
        assert_eq!(places[0].reviews, 0);
        // An empty website cell must be None, not Some("").
        assert_eq!(places[0].website, None);
    }

    #[test]
    fn fast_mode_still_produces_an_identity_and_a_link() {
        let (places, _) = read_from_text(FAST).expect("reads");
        let place = &places[0];
        assert_eq!(place.id, "zuglói fodrászat|47.5149,19.0778");
        let url = place.maps_url.as_deref().expect("rebuilt link");
        assert!(url.starts_with("https://www.google.com/maps/search/"));
        assert!(url.contains("Fodr"));
        assert_eq!(place.reviews, 7);
        assert_eq!(place.rating, Some(4.5));
    }

    #[test]
    fn a_row_with_no_name_is_skipped_not_fatal() {
        let text = format!("{FULL},,,,,,,,,,,,\n");
        let (places, skipped) = read_from_text(&text).expect("reads");
        assert_eq!(places.len(), 1);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn some_other_csv_is_rejected_by_name() {
        let error = read_from_text("name,age\nBob,7\n").expect_err("rejected");
        assert!(error.to_string().contains("not the scraper's CSV"));
    }

    #[test]
    fn a_missing_file_says_which_file() {
        let error = read_csv(Path::new("/nowhere/at/all.csv")).expect_err("rejected");
        assert!(error.to_string().contains("/nowhere/at/all.csv"));
    }
}
