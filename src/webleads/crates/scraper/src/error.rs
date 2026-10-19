//! What can go wrong talking to the scraper, as a type rather than a string.

use std::path::PathBuf;

/// `thiserror` writes the `Display` and `Error` implementations from these attributes.
/// The `#[from]` on a field also writes `From`, which is what lets `?` convert a
/// `csv::Error` into this without a single line of glue.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read {path}")]
    Unreadable {
        path: PathBuf,
        #[source]
        cause: std::io::Error,
    },

    #[error("{path} is not the scraper's CSV — {detail}")]
    NotScraperCsv { path: PathBuf, detail: String },

    #[error("{0}")]
    Scraper(String),

    #[error("row {row} of {path} could not be read")]
    BadRow {
        path: PathBuf,
        row: usize,
        #[source]
        cause: csv::Error,
    },
}

/// A shorthand so every function does not repeat the error type.
pub type Result<T> = std::result::Result<T, Error>;
