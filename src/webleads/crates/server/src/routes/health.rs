//! Is everything this depends on actually running?

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::SharedState;

/// Where the Google Maps scraper answers.
///
/// Beside us in this container. Asked through the scraper crate so the two can never
/// disagree about where it lives.
pub fn scraper_url() -> String {
    format!("{}/api/v1/jobs", webleads_scraper::client::base_url())
}

/// How long to wait before calling it not running. Short — this is asked on page load
/// and a slow answer is as useless as no answer.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(Debug, Serialize)]
pub struct Health {
    /// Us. Always true — you are reading our answer.
    pub server: bool,
    /// The Google Maps scraper. Searching cannot do anything without it.
    pub scraper: bool,
    /// What went wrong reaching it, when it did.
    pub scraper_detail: Option<String>,
    /// Who the server runs as and where, for the terminal's prompt.
    pub machine: crate::machine::Machine,
}

/// `GET /api/health` — what is up and what is not.
///
/// Searching depends on the scraper. Finding out it is not running by watching a
/// search fail is finding out too late, and it looks like our bug rather than a
/// service that was never started.
pub async fn health(State(state): State<SharedState>) -> Json<Health> {
    let answer = state
        .http
        .strict
        .get(scraper_url())
        .timeout(TIMEOUT)
        .send()
        .await;

    let (scraper, scraper_detail) = match answer {
        Ok(response) if response.status().is_success() => (true, None),
        Ok(response) => (
            false,
            Some(format!("answers {}", response.status().as_u16())),
        ),
        Err(err) if err.is_timeout() => (false, Some("did not answer in time".to_owned())),
        Err(_) => (false, Some("not running".to_owned())),
    };

    if !scraper {
        tracing::debug!("scraper unreachable: {scraper_detail:?}");
    }

    // The project folder is the one the logs sit in.
    let repo = state.log_dir.parent().unwrap_or(&state.log_dir);
    Json(Health {
        server: true,
        scraper,
        scraper_detail,
        machine: crate::machine::read(repo),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_health_check_and_the_client_look_in_the_same_place() {
        // Two addresses that could drift apart is how you get a page saying the
        // scraper is up while every search fails.
        assert!(scraper_url().starts_with(webleads_scraper::client::BASE_URL));
        assert!(scraper_url().ends_with("/api/v1/jobs"));
    }

    #[test]
    fn the_check_gives_up_quickly() {
        // Asked on every page load. Waiting eight seconds to be told something is not
        // running is worse than not asking.
        assert!(TIMEOUT <= std::time::Duration::from_secs(5));
    }
}
