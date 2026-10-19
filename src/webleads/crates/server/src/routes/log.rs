//! Taking what the browser reports and writing it down.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;

/// The longest report we will take. A stack trace is long; a megabyte is somebody
/// looping.
const MAX_REPORT: usize = 8_000;

#[derive(Debug, Deserialize)]
pub struct Report {
    /// What went wrong, in the browser's own words.
    pub message: String,
    /// Where it happened, if the browser said.
    #[serde(default)]
    pub at: Option<String>,
}

/// `POST /api/log` — the page telling us something went wrong.
///
/// The browser is the one place we could not see. `logs/frontend.log` sat empty from
/// the day it was made, so a page that misbehaved left no trace at all and the only
/// evidence was you saying "it did something odd".
pub async fn report(
    State(_state): State<SharedState>,
    Json(report): Json<Report>,
) -> ApiResult<()> {
    if report.message.len() > MAX_REPORT {
        return Err(ApiError::BadRequest("that report is too long".to_owned()));
    }

    // A separate target, so this lands in `frontend.log` rather than being mixed in
    // with the server's own troubles. Written as an error because the page only
    // reports when something has actually gone wrong.
    match &report.at {
        Some(at) => tracing::error!(target: "frontend", "‼️ ERROR: {} — at {at}", report.message),
        None => tracing::error!(target: "frontend", "‼️ ERROR: {}", report.message),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_longer_than_the_limit_is_refused() {
        assert!("x".repeat(MAX_REPORT).len() <= MAX_REPORT);
        assert!("x".repeat(MAX_REPORT + 1).len() > MAX_REPORT);
    }

    #[test]
    fn a_report_without_a_location_is_still_worth_having() {
        // `window.onerror` does not always say where. Half a report beats none.
        let bare: Report = serde_json::from_str(r#"{"message":"it broke"}"#).expect("parses");
        assert_eq!(bare.message, "it broke");
        assert_eq!(bare.at, None);
    }

    #[test]
    fn a_report_with_a_location_keeps_it() {
        let full: Report =
            serde_json::from_str(r#"{"message":"it broke","at":"leads.rs:42"}"#).expect("parses");
        assert_eq!(full.at.as_deref(), Some("leads.rs:42"));
    }
}
