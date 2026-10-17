//! Local fixture API; public bearer strings demonstrate authorization only.
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::PgPool;
#[derive(Deserialize)]
struct Input {
    id: String,
    payload: String,
}
fn authorize(h: &HeaderMap) -> Result<(), StatusCode> {
    if h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer demo-operator") {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
async fn enqueue(
    State(pool): State<PgPool>,
    h: HeaderMap,
    Json(i): Json<Input>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    authorize(&h)?;
    if i.id.is_empty() || i.id.len() > 80 || i.payload.is_empty() || i.payload.len() > 1000 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let inserted = crate::enqueue(&pool, &i.id, &i.payload)
        .await
        .map_err(|e| match e {
            sqlx::Error::Protocol(ref message) if message == "queue budget reached" => {
                StatusCode::TOO_MANY_REQUESTS
            }
            sqlx::Error::Protocol(_) => StatusCode::CONFLICT,
            _ => StatusCode::SERVICE_UNAVAILABLE,
        })?;
    Ok(Json(serde_json::json!({"inserted":inserted})))
}
async fn cancel(
    State(pool): State<PgPool>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    authorize(&h)?;
    let cancelled = crate::cancel(&pool, &id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(serde_json::json!({"cancelled":cancelled})))
}
async fn admission(
    State(permits): State<std::sync::Arc<tokio::sync::Semaphore>>,
    request: Request,
    next: Next,
) -> Response {
    let Ok(_permit) = permits.try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    match tokio::time::timeout(std::time::Duration::from_secs(5), next.run(request)).await {
        Ok(response) => response,
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    }
}

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/jobs", post(enqueue))
        .route("/jobs/{id}/cancel", post(cancel))
        .layer(DefaultBodyLimit::max(4096))
        .layer(middleware::from_fn_with_state(
            std::sync::Arc::new(tokio::sync::Semaphore::new(16)),
            admission,
        ))
        .with_state(pool)
}
