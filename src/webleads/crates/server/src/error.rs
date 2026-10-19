//! One error type for the whole web layer, which knows its own HTTP status.
//!
//! The point of this file: a handler returns `Result<T, ApiError>` and simply uses
//! `?`. Whatever went wrong arrives at the browser as the right status code with a
//! readable message — never as a cheerful `200 OK` with a problem hidden inside.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("the database failed")]
    Store(#[from] webleads_store::Error),

    #[error("the scraper's file could not be read")]
    Scraper(#[from] webleads_scraper::Error),

    /// Something on our side went wrong, and it is not the caller's fault.
    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    /// Which status code this is, in one place rather than spread across handlers.
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Scraper(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            // A refusal is the caller asking for something we will not do, not the
            // database breaking. Calling it a server failure would send us hunting
            // for a fault that is not there.
            Self::Store(webleads_store::Error::Refused(_)) => StatusCode::BAD_REQUEST,
            Self::Store(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// `IntoResponse` is the trait axum uses to turn a value into an HTTP response.
/// Implementing it here is what lets a handler return this type directly.
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Walk the chain of causes so the log holds the whole story, not just the
        // top line. `std::error::Error::source` is how one error points at the one
        // underneath it — this is why `#[source]` and `#[from]` above are worth it.
        let mut chain = vec![self.to_string()];
        let mut cause: Option<&(dyn std::error::Error + 'static)> =
            std::error::Error::source(&self);
        while let Some(err) = cause {
            chain.push(err.to_string());
            cause = err.source();
        }
        let detail = chain.join(": ");

        if status.is_server_error() {
            tracing::error!("‼️ ERROR: {detail}");
        } else {
            tracing::warn!("⚠️ WARNING: {detail}");
        }

        // The browser gets the top-level message. Internal failures keep their
        // details in the log rather than handing them to whoever is connected.
        let shown = match &self {
            // A refusal is already a sentence for a person; "the database failed"
            // in front of it calls a correct answer a breakdown.
            Self::Store(webleads_store::Error::Refused(why)) => why.clone(),
            _ if status.is_server_error() => self.to_string(),
            _ => detail,
        };

        (status, Json(serde_json::json!({ "error": shown }))).into_response()
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_failure_gets_the_right_status_code() {
        assert_eq!(
            ApiError::NotFound("no such file".into()).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiError::BadRequest("bad name".into()).status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn a_refusal_reads_as_the_refusal_and_not_a_database_failure() {
        let error = ApiError::Store(webleads_store::Error::Refused(
            "your starting point has to be in Budapest".into(),
        ));
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("reads");
        let text = String::from_utf8(body.to_vec()).expect("text");
        assert!(
            text.contains("your starting point has to be in Budapest"),
            "{text}"
        );
        assert!(!text.contains("database"), "{text}");
    }

    #[test]
    fn a_bad_csv_is_the_callers_fault_not_ours() {
        // 422 rather than 500: the file is wrong, the server is fine. A 500 here would
        // send somebody looking for a bug that is not there.
        let error = ApiError::Scraper(webleads_scraper::Error::NotScraperCsv {
            path: "/out/x.csv".into(),
            detail: "no `title` column".into(),
        });
        assert_eq!(error.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn a_database_failure_is_ours_and_says_so() {
        let error = ApiError::Store(webleads_store::Error::Corrupt(
            webleads_domain::UnknownVerdict("banana".into()),
        ));
        assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(error.status().is_server_error());
    }

    #[test]
    fn the_message_a_person_sees_is_the_message_we_wrote() {
        assert_eq!(
            ApiError::NotFound("there is no out/nope.csv".into()).to_string(),
            "there is no out/nope.csv"
        );
    }

    #[test]
    fn a_wrapped_failure_still_names_what_actually_broke() {
        // The top line says "the database failed"; the cause underneath says which
        // value it choked on. Losing that chain is how a log becomes useless.
        let error = ApiError::Store(webleads_store::Error::Corrupt(
            webleads_domain::UnknownVerdict("banana".into()),
        ));
        assert!(error.to_string().contains("database"));

        // Walk the whole chain, the way `into_response` does when it logs. "banana"
        // sits two levels down — the top says the database failed, the middle says it
        // held a value we do not understand, and only the bottom says which value.
        let mut chain = vec![error.to_string()];
        let mut cause: Option<&(dyn std::error::Error + 'static)> =
            std::error::Error::source(&error);
        while let Some(err) = cause {
            chain.push(err.to_string());
            cause = err.source();
        }
        assert!(chain.len() >= 3, "chain was only {chain:?}");
        assert!(
            chain.iter().any(|line| line.contains("banana")),
            "the value that broke it never reached the log: {chain:?}"
        );
    }
}
