//! Recording what happened when you telephoned a business.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use webleads_domain::{tidy_ring_back, Status};

use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;

/// The longest note we will store.
///
/// Not a security measure — it is our own page — but a note is a line or two, and a
/// megabyte arriving in this field means something has gone wrong rather than that
/// somebody had a lot to say.
const MAX_NOTE: usize = 4_000;

/// The most unanswered calls a caller may claim.
///
/// The stepper cannot reach it. It is here so a broken caller cannot write a number
/// with no meaning into the column.
const MOST_NO_ANSWERS: i64 = 99;

/// How many coming-up calls the panel asks for when it does not say.
const COMING_UP: i64 = 20;
/// And the most it will ever hand back.
const MOST_COMING_UP: i64 = 200;

#[derive(Debug, Deserialize)]
pub struct StatusRequest {
    /// One of: need_call, not_reached, follow_up, sold, not_interested, dead.
    pub status: String,
}

/// `POST /api/leads/:id/status` — say where a business has got to.
pub async fn set_status(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<StatusRequest>,
) -> ApiResult<Json<webleads_domain::Contact>> {
    // Parsed into the enum before it reaches the database, so a typo comes back as a
    // 400 naming the word rather than being stored and confusing us later.
    let status: Status = request
        .status
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("`{}` is not a status", request.status)))?;

    let contact = state.store.set_status(&id, status).await?;
    // The state that came back, not the one asked for: pressing "no answer" for the
    // fifth time gives up on the number, and the log should say what happened.
    tracing::info!(
        business = %id,
        asked = status.as_str(),
        became = contact.status.as_str(),
        rung = contact.not_reached_counter,
        "state changed"
    );
    Ok(Json(contact))
}

#[derive(Debug, Deserialize)]
pub struct NoAnswersRequest {
    /// How many times nobody answered.
    pub count: i64,
}

/// `POST /api/leads/:id/no_answers` — correct how many times nobody answered.
///
/// Its own address rather than a field on the status one: correcting the count is not
/// the same act as ringing again, and only one of the two writes a call down.
pub async fn set_no_answers(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<NoAnswersRequest>,
) -> ApiResult<Json<webleads_domain::Contact>> {
    if !(0..=MOST_NO_ANSWERS).contains(&request.count) {
        return Err(ApiError::BadRequest(format!(
            "{} unanswered calls is not a number we will store; the range is 0 to {MOST_NO_ANSWERS}",
            request.count
        )));
    }
    let contact = state.store.set_no_answers(&id, request.count).await?;
    tracing::info!(
        business = %id,
        rung = contact.not_reached_counter,
        became = contact.status.as_str(),
        "unanswered calls corrected"
    );
    Ok(Json(contact))
}

#[derive(Debug, Deserialize)]
pub struct RingBackRequest {
    /// When to ring back, as the browser's field writes it. `None` clears it.
    pub when: Option<String>,
}

/// `POST /api/leads/:id/ring_back` — set or clear when to ring a business back.
pub async fn set_ring_back(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<RingBackRequest>,
) -> ApiResult<Json<webleads_domain::Contact>> {
    // Tidied into the one shape we store, and refused if it is not a time at all.
    // An empty field is how the page says "clear it", which is not an error.
    let when = match request
        .when
        .as_deref()
        .map(str::trim)
        .filter(|w| !w.is_empty())
    {
        None => None,
        Some(typed) => Some(
            tidy_ring_back(typed)
                .ok_or_else(|| ApiError::BadRequest(format!("`{typed}` is not a date and time")))?,
        ),
    };

    let contact = state.store.set_ring_back(&id, when.as_deref()).await?;
    tracing::info!(business = %id, when = ?contact.follow_up_tstamp, "ring-back set");
    Ok(Json(contact))
}

#[derive(Debug, Deserialize)]
pub struct NoteRequest {
    pub note: String,
}

/// `POST /api/leads/:id/note` — save what you wrote about a business.
pub async fn set_note(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<NoteRequest>,
) -> ApiResult<Json<webleads_domain::Contact>> {
    if request.note.len() > MAX_NOTE {
        return Err(ApiError::BadRequest(format!(
            "that note is {} characters; the limit is {MAX_NOTE}",
            request.note.len()
        )));
    }

    state.store.set_note(&id, request.note.trim()).await?;
    Ok(Json(state.store.contact(&id).await?))
}

#[derive(Debug, Default, Deserialize)]
pub struct ComingUpRequest {
    pub limit: Option<i64>,
}

/// How many coming-up calls to hand back.
///
/// Pure, so the limits are a test rather than a hope — and a caller asking for the
/// whole database gets the most we will send rather than an error.
fn how_many(asked: Option<i64>) -> i64 {
    asked.unwrap_or(COMING_UP).clamp(1, MOST_COMING_UP)
}

/// `GET /api/coming_up` — the calls you said you would make, soonest first.
pub async fn coming_up(
    State(state): State<SharedState>,
    Query(request): Query<ComingUpRequest>,
) -> ApiResult<Json<Vec<webleads_store::Due>>> {
    Ok(Json(state.store.coming_up(how_many(request.limit)).await?))
}

/// `GET /api/tally` — how the calling is going, today and altogether.
///
/// Also the cheapest question on the whole program: a second screen asks it every
/// few seconds and only fetches the list again when the moment it carries has moved.
pub async fn tally(State(state): State<SharedState>) -> ApiResult<Json<webleads_store::Tally>> {
    Ok(Json(state.store.tally().await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_that_is_not_a_status_is_refused_before_it_reaches_the_database() {
        for bad in ["banana", "", "no answer", "NOT_REACHED", "new", "called"] {
            assert!(
                bad.parse::<Status>().is_err(),
                "`{bad}` should not parse as a status"
            );
        }
    }

    #[test]
    fn every_real_status_is_accepted() {
        assert_eq!(Status::ALL.len(), 7);
        for status in Status::ALL {
            assert_eq!(status.as_str().parse::<Status>().expect("parses"), status);
        }
    }

    #[test]
    fn a_note_longer_than_the_limit_is_refused() {
        // A note is a line or two. A megabyte arriving here means something is wrong.
        assert!("x".repeat(MAX_NOTE).len() <= MAX_NOTE);
        assert!("x".repeat(MAX_NOTE + 1).len() > MAX_NOTE);
    }

    #[test]
    fn a_count_of_unanswered_calls_has_to_be_a_number_we_would_write() {
        for count in [0, 1, 5, MOST_NO_ANSWERS] {
            assert!((0..=MOST_NO_ANSWERS).contains(&count));
        }
        for nonsense in [-1, MOST_NO_ANSWERS + 1, i64::MAX] {
            assert!(!(0..=MOST_NO_ANSWERS).contains(&nonsense));
        }
    }

    #[test]
    fn an_empty_ring_back_field_clears_it_rather_than_failing() {
        // The page sends the field as it stands, and an empty one means "no time".
        for blank in [Some(""), Some("   "), None] {
            let when = blank.map(str::trim).filter(|w| !w.is_empty());
            assert_eq!(when, None);
        }
    }

    #[test]
    fn the_coming_up_panel_cannot_ask_for_the_whole_database() {
        assert_eq!(how_many(None), COMING_UP);
        assert_eq!(how_many(Some(5)), 5);
        assert_eq!(how_many(Some(5_000)), MOST_COMING_UP);
        // Nothing and nonsense both come back as one call rather than none.
        assert_eq!(how_many(Some(0)), 1);
        assert_eq!(how_many(Some(-9)), 1);
    }
}
