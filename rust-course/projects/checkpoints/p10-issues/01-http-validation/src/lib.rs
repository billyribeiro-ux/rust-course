//! Pure HTTP boundary checkpoint. No issue is stored; /issues/validate only validates.
//! Credentials are public local test fixtures, not deployable authentication.
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Deserialize)]
struct Input {
    title: String,
}
#[derive(Serialize)]
struct Validated {
    title: String,
}
async fn validate(
    headers: HeaderMap,
    Json(input): Json<Input>,
) -> Result<Json<Validated>, StatusCode> {
    match headers.get("authorization").and_then(|v| v.to_str().ok()) {
        Some("Bearer demo-team1-writer") => {}
        Some("Bearer demo-team1-reader") => return Err(StatusCode::FORBIDDEN),
        _ => return Err(StatusCode::UNAUTHORIZED),
    }
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(Json(Validated {
        title: title.to_owned(),
    }))
}
async fn admit(
    State(slots): State<Arc<tokio::sync::Semaphore>>,
    request: Request,
    next: Next,
) -> Response {
    let Ok(_permit) = slots.try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    match tokio::time::timeout(std::time::Duration::from_secs(1), next.run(request)).await {
        Ok(response) => response,
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    }
}
pub fn router() -> Router {
    Router::new()
        .route("/issues/validate", post(validate))
        .layer(DefaultBodyLimit::max(4096))
        .layer(middleware::from_fn_with_state(
            Arc::new(tokio::sync::Semaphore::new(4)),
            admit,
        ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    async fn request(identity: &str, body: String) -> Response {
        router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/issues/validate")
                    .header("authorization", identity)
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn writer_normalizes_without_persistence() {
        let response = request("Bearer demo-team1-writer", r#"{"title":" fix "}"#.into()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(response.into_body(), 4096).await.unwrap().as_ref(),
            br#"{"title":"fix"}"#
        );
    }
    #[tokio::test]
    async fn permission_is_a_server_boundary() {
        assert_eq!(
            request("Bearer demo-team1-reader", r#"{"title":"fix"}"#.into())
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request("unknown", r#"{"title":"fix"}"#.into())
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    #[tokio::test]
    async fn expected_input_failure_is_explicit() {
        assert_eq!(
            request("Bearer demo-team1-writer", r#"{"title":" "}"#.into())
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            request("Bearer demo-team1-writer", "{".into())
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request("Bearer demo-team1-writer", "x".repeat(4097))
                .await
                .status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
}
