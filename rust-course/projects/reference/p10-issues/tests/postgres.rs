//! Opt-in destructive tests: use only the documented disposable local fixture.
mod support;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;
#[tokio::test]
#[ignore = "requires COURSE_TEST_DATABASE_URL pointing to a disposable course database"]
async fn postgres_authorization_transaction_pagination() {
    let pool = support::isolated_pool("COURSE_TEST_DATABASE_URL", "rust_course_test_issues").await;
    sqlx::migrate!().run(&pool).await.unwrap();
    // Test database is explicitly disposable; only these teaching tables are cleared.
    sqlx::raw_sql("TRUNCATE issue_audit,issues RESTART IDENTITY")
        .execute(&pool)
        .await
        .unwrap();
    let app = p10_issues::router(pool.clone());
    async fn call(
        app: axum::Router,
        method: &str,
        path: &str,
        token: &str,
        body: &str,
    ) -> (StatusCode, Value) {
        let response = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    assert_eq!(
        call(
            app.clone(),
            "POST",
            "/issues",
            "demo-team1-reader",
            r#"{"title":"deny"}"#
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, issue) = call(
        app.clone(),
        "POST",
        "/issues",
        "demo-team1-writer",
        r#"{"title":"Alice's bug"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = issue["id"].as_i64().unwrap();
    assert_eq!(
        call(app.clone(), "GET", "/issues", "demo-team2-writer", "")
            .await
            .1,
        serde_json::json!([])
    );
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("/issues/{id}/close"),
            "demo-team2-writer",
            r#"{"expected_revision":0}"#
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("/issues/{id}/close"),
            "demo-team1-writer",
            r#"{"expected_revision":0}"#
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("/issues/{id}/close"),
            "demo-team1-writer",
            r#"{"expected_revision":0}"#
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            app.clone(),
            "GET",
            &format!("/issues?after={id}&limit=1"),
            "demo-team1-reader",
            ""
        )
        .await
        .1,
        serde_json::json!([])
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM issue_audit")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    // A failing audit write must roll back the preceding issue update.
    let (_, second) = call(
        app.clone(),
        "POST",
        "/issues",
        "demo-team1-writer",
        r#"{"title":"rollback"}"#,
    )
    .await;
    let second = second["id"].as_i64().unwrap();
    sqlx::query(
        "INSERT INTO issue_audit(issue_id,revision,actor,action) VALUES($1,1,1,'injected')",
    )
    .bind(second)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        call(
            app.clone(),
            "POST",
            &format!("/issues/{second}/close"),
            "demo-team1-writer",
            r#"{"expected_revision":0}"#
        )
        .await
        .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let closed: bool = sqlx::query_scalar("SELECT closed FROM issues WHERE id=$1")
        .bind(second)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!closed);
    let (_, concurrent) = call(
        app.clone(),
        "POST",
        "/issues",
        "demo-team1-writer",
        r#"{"title":"race"}"#,
    )
    .await;
    let concurrent = concurrent["id"].as_i64().unwrap();
    let path = format!("/issues/{concurrent}/close");
    let (a, b) = tokio::join!(
        call(
            app.clone(),
            "POST",
            &path,
            "demo-team1-writer",
            r#"{"expected_revision":0}"#
        ),
        call(
            app.clone(),
            "POST",
            &path,
            "demo-team1-writer",
            r#"{"expected_revision":0}"#
        )
    );
    assert!(
        (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
            || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT)
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM issue_audit WHERE issue_id=$1")
        .bind(concurrent)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    pool.close().await;
}
