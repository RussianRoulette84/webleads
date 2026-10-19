//! Hiding businesses you never want to see again, and bringing them back.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;

/// What to hide: one business, or the chain a business belongs to.
#[derive(Debug, Deserialize)]
pub struct HideRequest {
    pub business_id: String,
    /// True to hide every branch of the chain rather than this one shop.
    #[serde(default)]
    pub whole_chain: bool,
}

/// `GET /api/hidden` — everything you have hidden, with what each one catches.
pub async fn hidden(
    State(state): State<SharedState>,
) -> ApiResult<Json<Vec<webleads_store::Hidden>>> {
    Ok(Json(state.store.hidden().await?))
}

/// `POST /api/hidden` — hide one business, or its whole chain.
///
/// The name is read from the database rather than taken from the request: the caller
/// says *which* business, and what it is called is ours to know.
pub async fn hide(
    State(state): State<SharedState>,
    Json(asked): Json<HideRequest>,
) -> ApiResult<Json<webleads_store::Hidden>> {
    let name = state
        .store
        .business_name(&asked.business_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("no business with id {}", asked.business_id)))?;

    let id = if asked.whole_chain {
        state.store.hide_chain(&name).await?
    } else {
        state.store.hide_business(&asked.business_id, &name).await?
    };

    let all = state.store.hidden().await?;
    all.into_iter()
        .find(|h| h.id == id)
        .map(Json)
        .ok_or_else(|| ApiError::NotFound("the rule was not written".into()))
}

/// `DELETE /api/hidden/:id` — put one back on the list.
pub async fn unhide(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<bool>> {
    let brought_back = state.store.unhide(id).await?;
    if !brought_back {
        return Err(ApiError::NotFound(format!("nothing hidden with id {id}")));
    }
    Ok(Json(true))
}
