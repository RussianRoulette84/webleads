//! Watching a job that runs in the background.
//!
//! One thing uses this: photographing every broken website on the list, which takes
//! minutes. The page starts it, gets a number back straight away, and watches that
//! number.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use crate::error::{ApiError, ApiResult};
use crate::state::{JobState, SharedState};

/// What comes back the moment a job is asked for — not when it finishes.
#[derive(Debug, Serialize)]
pub struct JobStarted {
    pub job_id: u64,
    /// How many things it has to get through.
    pub rows: usize,
    /// True when this handed back a job that was already going, rather than
    /// starting a new one.
    #[serde(default)]
    pub already_running: bool,
}

/// `POST /api/jobs/{id}/stop` — stop after the item it is on.
///
/// Everything done so far is kept. The same bargain as stopping a search.
pub async fn stop_job(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<&'static str>> {
    let job = state
        .jobs
        .get(id)
        .ok_or_else(|| ApiError::NotFound(format!("there is no job {id}")))?;
    job.ask_to_stop();
    tracing::info!(job_id = id, "asked to stop");
    Ok(Json("stopping"))
}

/// `GET /api/jobs/{id}` — how is that job getting on?
pub async fn job_status(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<JobState>> {
    state
        .jobs
        .get(id)
        .map(|job| Json(job.snapshot()))
        .ok_or_else(|| ApiError::NotFound(format!("there is no job {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::describe;

    #[test]
    fn a_new_job_starts_at_nothing_done() {
        let jobs = crate::state::Jobs::default();
        let (id, job) = jobs
            .start_exclusive(31, "checking websites")
            .expect("starts");
        assert!(id > 0);
        match job.snapshot() {
            JobState::Running { done, total, step } => {
                assert_eq!((done, total), (0, 31));
                assert_eq!(step, "checking websites");
            }
            other => panic!("expected Running, got {other:?}"),
        }
    }

    #[test]
    fn the_progress_counter_is_what_the_page_reads() {
        let jobs = crate::state::Jobs::default();
        let (_, job) = jobs
            .start_exclusive(10, "checking websites")
            .expect("starts");
        job.done.store(4, std::sync::atomic::Ordering::Relaxed);
        match job.snapshot() {
            JobState::Running { done, total, .. } => assert_eq!((done, total), (4, 10)),
            other => panic!("expected Running, got {other:?}"),
        }
    }

    #[test]
    fn a_finished_job_stops_reporting_progress() {
        let jobs = crate::state::Jobs::default();
        let (_, job) = jobs
            .start_exclusive(10, "checking websites")
            .expect("starts");
        job.set(JobState::Done {
            summary: serde_json::json!({ "leads_found": 3 }),
        });
        // The counter is still there; the state is what the page goes by.
        job.done.store(10, std::sync::atomic::Ordering::Relaxed);
        assert!(matches!(job.snapshot(), JobState::Done { .. }));
    }

    #[test]
    fn a_job_number_we_never_handed_out_is_not_found() {
        let jobs = crate::state::Jobs::default();
        let (id, _) = jobs
            .start_exclusive(1, "checking websites")
            .expect("starts");
        assert!(jobs.get(id).is_some());
        assert!(jobs.get(id + 999).is_none());
    }

    #[test]
    fn each_job_in_turn_gets_its_own_number() {
        // Only one can run at a time now, so they are numbered in turn rather than
        // side by side — but no two ever share a number.
        let jobs = crate::state::Jobs::default();
        let mut seen = Vec::new();
        for _ in 0..3 {
            let (id, job) = jobs.start_exclusive(1, "checking").expect("starts");
            job.set(JobState::Done {
                summary: serde_json::json!({}),
            });
            seen.push(id);
        }
        let mut unique = seen.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            seen.len(),
            "two jobs shared a number: {seen:?}"
        );
    }

    #[test]
    fn a_failure_is_one_sentence_with_the_real_cause_in_it() {
        let error = ApiError::Store(webleads_store::Error::Corrupt(
            webleads_domain::UnknownVerdict("banana".into()),
        ));
        let sentence = describe(&error);
        // The top says what broke, the end says which value did it.
        assert!(sentence.contains("database"));
        assert!(sentence.contains("banana"), "got `{sentence}`");
    }
}
