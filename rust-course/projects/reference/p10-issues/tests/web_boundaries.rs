//! No live database or browser is needed: every application request below is
//! rejected before SQL. These assertions describe public local demo identities,
//! never a production authentication protocol.
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

type Trace = Arc<Mutex<Vec<&'static str>>>;
fn note(trace: &Trace, event: &'static str) {
    // Tests fail immediately on poisoned state. The guard ends before any await.
    trace.lock().expect("test trace lock").push(event);
}
async fn inner(State(trace): State<Trace>, request: Request, next: Next) -> Response {
    note(&trace, "inner request");
    let response = next.run(request).await;
    note(&trace, "inner response");
    response
}
async fn outer(State(trace): State<Trace>, request: Request, next: Next) -> Response {
    note(&trace, "outer request");
    let response = next.run(request).await;
    note(&trace, "outer response");
    response
}
async fn handler(State(trace): State<Trace>) -> &'static str {
    note(&trace, "handler");
    "ok"
}

#[tokio::test]
async fn repeated_router_layers_wrap_in_reverse_request_order() {
    let trace = Trace::default();
    let app = Router::new()
        .route("/", get(handler))
        .layer(middleware::from_fn_with_state(trace.clone(), inner))
        .layer(middleware::from_fn_with_state(trace.clone(), outer))
        .with_state(trace.clone());
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        *trace.lock().unwrap(),
        [
            "outer request",
            "inner request",
            "handler",
            "inner response",
            "outer response"
        ]
    );
}
fn app() -> Router {
    // connect_lazy opens no connection. All exercised paths reject before SQL.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused@127.0.0.1:1/course_no_connection")
        .unwrap();
    p10_issues::router(pool)
}
#[tokio::test]
async fn ambient_cookie_and_spoofed_proxy_headers_do_not_identify_an_actor() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/issues")
                .header("cookie", "session=demo-team1-writer")
                .header("x-forwarded-user", "demo-team1-writer")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
#[tokio::test]
async fn recognized_reader_is_still_forbidden_to_mutate() {
    let response = app()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/issues")
                .header("authorization", "Bearer demo-team1-reader")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"valid but forbidden"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
#[tokio::test]
async fn extraction_can_reject_before_in_handler_identity_checks() {
    for (body, expected) in [
        ("{".to_owned(), StatusCode::BAD_REQUEST),
        ("x".repeat(4097), StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/issues")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
}
#[tokio::test]
async fn no_cross_origin_preflight_permission_is_granted() {
    let response = app()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/issues")
                .header("origin", "https://untrusted.invalid")
                .header("access-control-request-method", "POST")
                .header(
                    "access-control-request-headers",
                    "authorization,content-type",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}
#[tokio::test]
async fn an_origin_header_is_not_an_identity_or_authorization_check() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/issues")
                .header("origin", "http://127.0.0.1:4173")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
