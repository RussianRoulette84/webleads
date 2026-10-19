//! The web interface.
//!
//! One module per thing the page asks for, so no single file has to hold the whole
//! surface. [`router`] is the only place that knows which address goes where.

pub mod areas;
pub mod assemble;
pub mod contacts;
pub mod export;
pub mod health;
pub mod hidden;
pub mod jobs;
pub mod leads;
pub mod log;
pub mod logs;
pub mod runs;
pub mod shots;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::state::SharedState;

/// How many leads one page holds when the caller does not say.
pub const DEFAULT_LIMIT: i64 = 50;
/// The most one request will ever hand back, whatever it asks for.
pub const MAX_LIMIT: i64 = 500;

/// Build the router.
///
/// `with_state` is what makes the `State` extractor in each handler work — axum takes
/// the value once here and clones it into every request, which for an `Arc` is cheap.
pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/api/leads", get(leads::leads))
        .route("/api/leads/pins", get(leads::pins))
        .route("/api/leads/:id/status", post(contacts::set_status))
        .route("/api/leads/:id/note", post(contacts::set_note))
        .route("/api/leads/:id/no_answers", post(contacts::set_no_answers))
        .route("/api/leads/:id/ring_back", post(contacts::set_ring_back))
        .route("/api/coming_up", get(contacts::coming_up))
        .route("/api/tally", get(contacts::tally))
        .route("/api/leads/:id/shot", post(shots::take_one))
        .route("/api/shots", post(shots::take_many))
        .route("/api/targets", get(leads::targets).put(leads::save_targets))
        .route("/api/export", get(export::export))
        .route("/api/export.xlsx", get(export::export_xlsx))
        .route("/api/export.html", get(export::export_page))
        .route("/api/areas", get(areas::areas).post(areas::add))
        .route("/api/areas/:id", delete(areas::remove))
        .route("/api/areas/:id/choose", post(areas::choose))
        .route("/api/places", get(areas::look_up))
        .route("/api/cities", get(areas::cities))
        .route("/api/cities/:id/centre", post(areas::move_centre))
        .route("/api/search_lang", post(runs::set_search_lang))
        .route(
            "/api/your_zone",
            get(runs::your_zone).post(runs::set_your_zone),
        )
        .route("/api/hidden", get(hidden::hidden).post(hidden::hide))
        .route("/api/hidden/:id", delete(hidden::unhide))
        .route("/api/health", get(health::health))
        .route("/api/log", post(log::report))
        .route("/api/logs", get(logs::tail))
        .route("/api/runs", post(runs::start))
        .route("/api/runs/estimate", post(runs::estimate))
        .route("/api/runs/queue", get(runs::waiting).post(runs::queue))
        .route("/api/runs/queue/pause", post(runs::hold))
        .route("/api/runs/queue/:id", delete(runs::forget))
        .route("/api/runs/current", get(runs::current))
        .route("/api/runs/past", get(runs::past))
        .route("/api/runs/:id", delete(runs::erase))
        .route("/api/runs/:id/forget_cost", get(runs::forget_cost))
        .route("/api/runs/:id/words", get(runs::words))
        .route("/api/runs/:id/stop", post(runs::stop))
        .route("/api/runs/:id/pause", post(runs::pause))
        .route("/api/runs/:id/continue", post(runs::carry_on))
        .route("/api/runs/:id/events", get(runs::events))
        .route("/api/jobs/:id", get(jobs::job_status))
        .route("/api/jobs/:id/stop", post(jobs::stop_job))
        .with_state(state)
}

/// Flatten an error and everything underneath it into one sentence.
///
/// Shared, because a failure shown on the page and a failure written to the log
/// should say the same thing.
pub fn describe(err: &crate::error::ApiError) -> String {
    let mut parts = vec![err.to_string()];
    let mut cause: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(err);
    while let Some(next) = cause {
        parts.push(next.to_string());
        cause = next.source();
    }
    parts.join(": ")
}
