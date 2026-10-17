//! Local teaching API. Demo credentials are fixtures, NOT production authentication.
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
#[derive(Clone)]
pub struct App {
    pub pool: PgPool,
}
#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub &'static str);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
fn database(error: sqlx::Error) -> ApiError {
    eprintln!("database operation failed: {error}");
    ApiError(StatusCode::SERVICE_UNAVAILABLE, "storage unavailable")
}
#[derive(Clone, Copy)]
pub struct Actor {
    pub id: i64,
    pub team: i64,
    pub write: bool,
}
pub fn actor(headers: &HeaderMap, write: bool) -> Result<Actor, ApiError> {
    let token = headers.get("authorization").and_then(|h| h.to_str().ok());
    let actor = match token {
        Some("Bearer demo-team1-writer") => Actor {
            id: 1,
            team: 1,
            write: true,
        },
        Some("Bearer demo-team1-reader") => Actor {
            id: 2,
            team: 1,
            write: false,
        },
        Some("Bearer demo-team2-writer") => Actor {
            id: 3,
            team: 2,
            write: true,
        },
        _ => return Err(ApiError(StatusCode::UNAUTHORIZED, "demo identity required")),
    };
    if write && !actor.write {
        return Err(ApiError(StatusCode::FORBIDDEN, "write permission required"));
    }
    Ok(actor)
}
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Issue {
    pub id: i64,
    pub team_id: i64,
    pub title: String,
    pub closed: bool,
    pub revision: i64,
}
#[derive(Deserialize)]
pub struct NewIssue {
    pub title: String,
}
#[derive(Deserialize)]
pub struct Page {
    pub after: Option<i64>,
    pub limit: Option<i64>,
}
#[derive(Deserialize)]
pub struct Close {
    pub expected_revision: i64,
}
pub fn validate_title(title: &str) -> Result<&str, ApiError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "title must contain 1..200 characters",
        ));
    }
    Ok(title)
}
async fn admission(
    State(permits): State<std::sync::Arc<tokio::sync::Semaphore>>,
    request: Request,
    next: Next,
) -> Response {
    let Ok(_permit) = permits.try_acquire_owned() else {
        return ApiError(StatusCode::TOO_MANY_REQUESTS, "request budget full").into_response();
    };
    match tokio::time::timeout(std::time::Duration::from_secs(5), next.run(request)).await {
        Ok(response) => response,
        Err(_) => ApiError(
            StatusCode::REQUEST_TIMEOUT,
            "deadline exceeded; reconcile operation outcome",
        )
        .into_response(),
    }
}
pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/issues", get(list).post(create))
        .route("/issues/{id}/close", post(close))
        .layer(DefaultBodyLimit::max(4096))
        .layer(middleware::from_fn_with_state(
            std::sync::Arc::new(tokio::sync::Semaphore::new(16)),
            admission,
        ))
        .with_state(App { pool })
}
async fn list(
    State(app): State<App>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Result<Json<Vec<Issue>>, ApiError> {
    let who = actor(&headers, false)?;
    let limit = page.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) || page.after.unwrap_or(0) < 0 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid page"));
    }
    let rows=sqlx::query_as::<_,Issue>("SELECT id,team_id,title,closed,revision FROM issues WHERE team_id=$1 AND id>$2 ORDER BY id LIMIT $3").bind(who.team).bind(page.after.unwrap_or(0)).bind(limit).fetch_all(&app.pool).await.map_err(database)?;
    Ok(Json(rows))
}
async fn create(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<NewIssue>,
) -> Result<(StatusCode, Json<Issue>), ApiError> {
    let who = actor(&headers, true)?;
    let title = validate_title(&input.title)?;
    let issue=sqlx::query_as::<_,Issue>("INSERT INTO issues(team_id,title) VALUES($1,$2) RETURNING id,team_id,title,closed,revision").bind(who.team).bind(title).fetch_one(&app.pool).await.map_err(database)?;
    Ok((StatusCode::CREATED, Json(issue)))
}
async fn close(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<Close>,
) -> Result<Json<Issue>, ApiError> {
    let who = actor(&headers, true)?;
    close_issue(&app.pool, who, id, input.expected_revision)
        .await
        .map(Json)
}
/// Update and audit are one transaction. Team predicate prevents cross-team access.
pub async fn close_issue(
    pool: &PgPool,
    who: Actor,
    id: i64,
    revision: i64,
) -> Result<Issue, ApiError> {
    if !who.write {
        return Err(ApiError(StatusCode::FORBIDDEN, "write permission required"));
    }
    let mut tx = pool.begin().await.map_err(database)?;
    let row =
        sqlx::query("SELECT revision,closed FROM issues WHERE id=$1 AND team_id=$2 FOR UPDATE")
            .bind(id)
            .bind(who.team)
            .fetch_optional(&mut *tx)
            .await
            .map_err(database)?
            .ok_or(ApiError(StatusCode::NOT_FOUND, "issue not found"))?;
    if row.get::<i64, _>("revision") != revision || row.get::<bool, _>("closed") {
        return Err(ApiError(StatusCode::CONFLICT, "revision conflict"));
    }
    let issue=sqlx::query_as::<_,Issue>("UPDATE issues SET closed=true,revision=revision+1 WHERE id=$1 RETURNING id,team_id,title,closed,revision").bind(id).fetch_one(&mut *tx).await.map_err(database)?;
    sqlx::query("INSERT INTO issue_audit(issue_id,revision,actor,action) VALUES($1,$2,$3,'close')")
        .bind(id)
        .bind(issue.revision)
        .bind(who.id)
        .execute(&mut *tx)
        .await
        .map_err(database)?;
    tx.commit().await.map_err(database)?;
    Ok(issue)
}
#[cfg(feature = "verified-query")]
pub async fn verified_count(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!("SELECT count(*) as \"count!\" FROM issues")
        .fetch_one(pool)
        .await?;
    Ok(row.count)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn title_and_roles() {
        assert!(validate_title("   ").is_err());
        assert_eq!(validate_title("  fix me ").unwrap(), "fix me");
        let mut h = HeaderMap::new();
        h.insert("authorization", "Bearer demo-team1-reader".parse().unwrap());
        assert!(actor(&h, false).is_ok());
        assert_eq!(actor(&h, true).err().unwrap().0, StatusCode::FORBIDDEN);
    }
}
