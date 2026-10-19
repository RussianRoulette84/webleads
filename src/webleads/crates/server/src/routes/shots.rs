//! Pictures of broken websites — one at a time, or the whole filtered list.

use std::collections::HashMap;

use axum::extract::{Path as UrlPath, Query, State};
use axum::Json;
use serde::Serialize;
use webleads_domain::Verdict;
use webleads_store::StoredLead;

use crate::error::{ApiError, ApiResult};
use crate::routes::describe;
use crate::routes::leads::Paging;
use crate::shot;
use crate::state::{JobState, SharedState};

/// The most pictures one "do all of them" will take.
///
/// Each is a browser start and a page load, so this is minutes rather than seconds.
const MAX_AT_ONCE: usize = 200;

/// One picture, and what it turned out to show.
#[derive(Debug, Clone, Serialize)]
pub struct Shot {
    /// Where the page can load it from.
    pub url: String,
    /// What the picture shows, in words — "nothing loaded", or the real page.
    pub answered: String,
}

/// `POST /api/leads/:id/shot` — photograph this business's website.
pub async fn take_one(
    State(state): State<SharedState>,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<Json<Shot>> {
    let lead = state
        .store
        .lead(&id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("no business with id {id}")))?;

    let url = worth_photographing(&lead).ok_or_else(|| {
        ApiError::BadRequest("there is no website to photograph — that is the lead".into())
    })?;

    let file = state.shots_dir.join(shot::safe_name(&id));
    let answered = shot::take(&url, &file)
        .await
        .map_err(|err| ApiError::BadRequest(err.to_string()))?;

    tracing::info!(%id, %url, "photographed a broken website");
    Ok(Json(Shot {
        url: format!("/shots/{}", shot::safe_name(&id)),
        answered,
    }))
}

/// `POST /api/shots` — photograph every broken site the filter is showing.
///
/// Answers straight away with a job number; the page watches it through
/// `GET /api/jobs/{id}`, the same way an import is watched.
pub async fn take_many(
    State(state): State<SharedState>,
    Query(paging): Query<Paging>,
) -> ApiResult<Json<crate::routes::jobs::JobStarted>> {
    let want = webleads_store::Wanted {
        limit: MAX_AT_ONCE as i64,
        offset: 0,
        ..paging.wanted()
    };
    let waiting: Vec<(String, String)> = state
        .store
        .leads(&want)
        .await?
        .iter()
        .filter_map(|lead| worth_photographing(lead).map(|url| (lead.lead.id.clone(), url)))
        .collect();

    let total = waiting.len();
    if total == 0 {
        return Err(ApiError::BadRequest(
            "nothing here has a website to photograph".into(),
        ));
    }

    let (job_id, job) = match state.jobs.start_exclusive(total, "taking pictures") {
        Ok(fresh) => fresh,
        Err((job_id, job)) => {
            return Ok(Json(crate::routes::jobs::JobStarted {
                job_id,
                rows: job.total.load(std::sync::atomic::Ordering::Relaxed),
                already_running: true,
            }))
        }
    };

    let folder = state.shots_dir.clone();
    tokio::spawn(async move {
        let mut taken = 0_usize;
        let mut refused: Option<String> = None;

        for (id, url) in waiting {
            // Between pictures, not during one: a half-written file is worse than one
            // picture more than you asked for.
            if job.stop_asked() {
                job.set(JobState::Done {
                    summary: serde_json::json!({
                        "taken": taken, "of": total, "stopped": true,
                    }),
                });
                tracing::info!(taken, of = total, "you stopped the pictures");
                return;
            }
            match shot::take(&url, &folder.join(shot::safe_name(&id))).await {
                Ok(_) => taken += 1,
                // A browser that is not there will not be there for the next one
                // either, so that one stops the lot rather than failing two hundred
                // times over.
                Err(err @ shot::ShotError::NoBrowser(_)) => {
                    refused = Some(err.to_string());
                    break;
                }
                Err(err) => tracing::warn!("⚠️ WARNING: {}", describe_shot(&err)),
            }
            job.done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        match refused {
            Some(reason) => job.set(JobState::Failed { reason }),
            None => job.set(JobState::Done {
                summary: serde_json::json!({ "taken": taken, "of": total }),
            }),
        }
    });

    Ok(Json(crate::routes::jobs::JobStarted {
        job_id,
        rows: total,
        already_running: false,
    }))
}

/// Which pictures we already hold, by business.
///
/// Read from the folder rather than from the database: the pictures are files, and a
/// database row saying one exists when it does not is a broken image on the page.
pub async fn taken(folder: &std::path::Path, leads: &[StoredLead]) -> HashMap<String, Shot> {
    let mut found = HashMap::new();
    for lead in leads {
        let name = shot::safe_name(&lead.lead.id);
        if !folder.join(&name).exists() {
            continue;
        }
        // Written beside the picture when it was taken. Missing on anything taken
        // before that was recorded, which is not worth failing over.
        let answered = tokio::fs::read_to_string(folder.join(&name).with_extension("txt"))
            .await
            .unwrap_or_else(|_| "taken earlier".to_owned());
        found.insert(
            lead.lead.id.clone(),
            Shot {
                url: format!("/shots/{name}"),
                answered,
            },
        );
    }
    found
}

/// Is this business worth photographing, and at what address?
///
/// Only the ones with something to show: a business with no website at all has
/// nothing to photograph, and that is the whole reason it is a lead.
///
/// Pure, so which rows the camera offers itself on can be tested.
pub fn worth_photographing(lead: &StoredLead) -> Option<String> {
    match lead.lead.verdict {
        Verdict::DeadSite | Verdict::InsecureSite | Verdict::LiveSite => lead
            .lead
            .website
            .clone()
            .filter(|url| !url.trim().is_empty()),
        // No site at all, or only a social page — there is nothing of theirs to show.
        Verdict::NoSite | Verdict::SocialOnly => None,
    }
}

/// One failure, in a sentence.
fn describe_shot(err: &shot::ShotError) -> String {
    describe(&ApiError::BadRequest(err.to_string()))
}

#[cfg(test)]
#[path = "shots_tests.rs"]
mod tests;
