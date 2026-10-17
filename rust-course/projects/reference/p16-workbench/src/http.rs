use crate::{Event, Order, Store, Tick};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use std::sync::{Arc, Mutex};
pub type Shared = Arc<Mutex<Store>>;
static WORK: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(8);
pub struct Error(StatusCode, &'static str);
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
fn authorize(headers: &HeaderMap, write: bool) -> Result<(), Error> {
    match headers.get("authorization").and_then(|v| v.to_str().ok()) {
        Some("Bearer demo-operator") => Ok(()),
        Some("Bearer demo-reader") if !write => Ok(()),
        Some("Bearer demo-reader") => Err(Error(StatusCode::FORBIDDEN, "operator required")),
        _ => Err(Error(
            StatusCode::UNAUTHORIZED,
            "local demo identity required",
        )),
    }
}
#[derive(Clone)]
struct Admission {
    permits: Arc<tokio::sync::Semaphore>,
    deadline: std::time::Duration,
}
async fn admit(State(budget): State<Admission>, request: Request, next: Next) -> Response {
    let Ok(_permit) = budget.permits.try_acquire_owned() else {
        return Error(StatusCode::TOO_MANY_REQUESTS, "request budget full").into_response();
    };
    match tokio::time::timeout(budget.deadline, next.run(request)).await {
        Ok(response) => response,
        Err(_) => Error(
            StatusCode::REQUEST_TIMEOUT,
            "request deadline; retry with same logical identity",
        )
        .into_response(),
    }
}
pub fn router(store: Shared) -> Router {
    router_with_admission(
        store,
        Admission {
            permits: Arc::new(tokio::sync::Semaphore::new(16)),
            deadline: std::time::Duration::from_secs(5),
        },
    )
}
fn router_with_admission(store: Shared, admission: Admission) -> Router {
    Router::new()
        .route("/health", get(|| async { "simulated workbench ok" }))
        .route("/api/snapshot", get(snapshot))
        .route("/api/ticks", post(tick))
        .route("/api/orders", post(order))
        .route("/api/orders/{id}", get(order_result))
        .route("/api/watchlist", put(watchlist))
        .route("/api/history", get(history))
        .route("/api/jobs", get(jobs))
        .route("/api/jobs/replay", post(schedule))
        .route("/api/jobs/{id}/cancel", post(cancel))
        .layer(DefaultBodyLimit::max(16384))
        .layer(middleware::from_fn_with_state(admission, admit))
        .with_state(store)
}
async fn snapshot(State(s): State<Shared>, h: HeaderMap) -> Result<Json<crate::Snapshot>, Error> {
    authorize(&h, false)?;
    let permit = WORK
        .try_acquire()
        .map_err(|_| Error(StatusCode::TOO_MANY_REQUESTS, "work budget full"))?;
    let value = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        s.lock().map(|s| s.engine.snapshot()).map_err(|_| ())
    })
    .await
    .map_err(|_| Error(StatusCode::SERVICE_UNAVAILABLE, "worker unavailable"))?
    .map_err(|_| Error(StatusCode::SERVICE_UNAVAILABLE, "state unavailable"))?;
    Ok(Json(value))
}
async fn commit(s: Shared, event: Event) -> Result<Json<serde_json::Value>, Error> {
    let permit = WORK
        .try_acquire()
        .map_err(|_| Error(StatusCode::TOO_MANY_REQUESTS, "work budget full"))?;
    let applied = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        s.lock()
            .map_err(|_| crate::CommitError::Storage("state unavailable".into()))?
            .commit(event)
    })
    .await
    .map_err(|_| Error(StatusCode::SERVICE_UNAVAILABLE, "worker unavailable"))?
    .map_err(|error| match error {
        crate::CommitError::Rejected(reason) => Error(StatusCode::UNPROCESSABLE_ENTITY, reason),
        crate::CommitError::Conflict(reason) => Error(StatusCode::CONFLICT, reason),
        crate::CommitError::Storage(reason) => {
            eprintln!("workbench storage outcome uncertain: {reason}");
            Error(
                StatusCode::SERVICE_UNAVAILABLE,
                "storage unavailable; preserve logical identity and reconcile",
            )
        }
    })?;
    Ok(Json(
        serde_json::json!({"applied":applied,"simulated":true}),
    ))
}
async fn tick(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(t): Json<Tick>,
) -> Result<Json<serde_json::Value>, Error> {
    authorize(&h, true)?;
    commit(s, Event::Tick(t)).await
}
async fn order(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(o): Json<Order>,
) -> Result<Json<serde_json::Value>, Error> {
    authorize(&h, true)?;
    commit(s, Event::Order(o)).await
}
#[derive(serde::Deserialize)]
struct Watchlist {
    symbols: Vec<String>,
}
async fn watchlist(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(w): Json<Watchlist>,
) -> Result<Json<serde_json::Value>, Error> {
    authorize(&h, true)?;
    commit(s, Event::Watchlist(w.symbols)).await
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[tokio::test]
    async fn read_and_write_boundary() {
        let path = std::env::temp_dir().join(format!(
            "course-auth-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app = router(Arc::new(Mutex::new(Store::open(&path).unwrap())));
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/snapshot")
                    .header("authorization", "Bearer demo-reader")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/orders")
                    .header("authorization", "Bearer demo-reader")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"order_id":"x","symbol":"RUST","side":"buy","quantity":1}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        std::fs::remove_file(path).unwrap();
    }
}

#[derive(serde::Deserialize)]
struct HistoryPage {
    after: Option<u64>,
    limit: Option<usize>,
}
async fn inspect<T: Send + 'static>(
    s: Shared,
    read: impl FnOnce(&crate::Engine) -> Result<T, &'static str> + Send + 'static,
) -> Result<T, Error> {
    let permit = WORK
        .try_acquire()
        .map_err(|_| Error(StatusCode::TOO_MANY_REQUESTS, "work budget full"))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let store = s
            .lock()
            .map_err(|_| Error(StatusCode::SERVICE_UNAVAILABLE, "state unavailable"))?;
        read(&store.engine)
            .map_err(|_| Error(StatusCode::BAD_REQUEST, "invalid request or expired cursor"))
    })
    .await
    .map_err(|_| Error(StatusCode::SERVICE_UNAVAILABLE, "worker unavailable"))?
}
async fn history(
    State(s): State<Shared>,
    h: HeaderMap,
    Query(p): Query<HistoryPage>,
) -> Result<Json<crate::replay::History>, Error> {
    authorize(&h, false)?;
    inspect(s, move |e| {
        e.history(p.after.unwrap_or(0), p.limit.unwrap_or(100))
    })
    .await
    .map(Json)
}
async fn jobs(
    State(s): State<Shared>,
    h: HeaderMap,
) -> Result<Json<Vec<crate::replay::JobView>>, Error> {
    authorize(&h, false)?;
    inspect(s, |e| Ok(e.jobs())).await.map(Json)
}
async fn schedule(
    State(s): State<Shared>,
    h: HeaderMap,
    Json(spec): Json<crate::replay::ReplaySpec>,
) -> Result<Json<serde_json::Value>, Error> {
    authorize(&h, true)?;
    commit(s, Event::ScheduleReplay(spec)).await
}
async fn cancel(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, Error> {
    authorize(&h, true)?;
    commit(s, Event::CancelReplay { job_id: id }).await
}
/// One worker, one durable tick per poll; shares the request admission budget.
pub async fn worker(store: Shared, mut stop: tokio::sync::watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
    loop {
        tokio::select! {_=stop.changed()=>break,_=interval.tick()=>{
         let Ok(permit)=WORK.try_acquire() else{continue;};let state=store.clone();
         match tokio::task::spawn_blocking(move||{let _permit=permit;state.lock().map_err(|_|crate::CommitError::Storage("state unavailable".into()))?.advance_replay()}).await{Ok(Ok(_))=>{},Ok(Err(error))=>{eprintln!("replay worker stopped after storage error: {error}");break;},Err(error)=>{eprintln!("replay worker join error: {error}");break;}}
        }}
    }
}
#[cfg(test)]
mod boundary_tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    fn fixture() -> (Shared, std::path::PathBuf) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        // Relaxed ordering suffices: this counter allocates unique fixture names only.
        let ordinal = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "course-boundary-{}-{}-{}.jsonl",
            ordinal,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (Arc::new(Mutex::new(Store::open(&path).unwrap())), path)
    }
    async fn request(
        app: Router,
        method: &str,
        path: &str,
        role: &str,
        body: &str,
    ) -> (StatusCode, serde_json::Value) {
        let r = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {role}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = r.status();
        let b = to_bytes(r.into_body(), 65536).await.unwrap();
        (
            status,
            serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null),
        )
    }
    #[tokio::test]
    async fn replay_routes_reader_denial_history_and_cancellation() {
        let (store, path) = fixture();
        let app = router(store.clone());
        let body = r#"{"job_id":"route-job","ticks":[{"event_id":"j1","symbol":"RUST","price_cents":100,"sequence":1},{"event_id":"j2","symbol":"RUST","price_cents":102,"sequence":2}]}"#;
        assert_eq!(
            request(app.clone(), "POST", "/api/jobs/replay", "demo-reader", body)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(
                app.clone(),
                "POST",
                "/api/jobs/replay",
                "demo-operator",
                body
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            request(
                app.clone(),
                "POST",
                "/api/jobs/replay",
                "demo-operator",
                body
            )
            .await
            .1["applied"],
            false
        );
        store.lock().unwrap().advance_replay().unwrap();
        let (status, history) = request(
            app.clone(),
            "GET",
            "/api/history?limit=1",
            "demo-reader",
            "",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(history["points"].as_array().unwrap().len(), 1);
        assert_eq!(
            request(
                app.clone(),
                "GET",
                "/api/history?limit=999999",
                "demo-reader",
                ""
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request(
                app.clone(),
                "POST",
                "/api/jobs/route-job/cancel",
                "demo-reader",
                ""
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(
                app.clone(),
                "POST",
                "/api/jobs/route-job/cancel",
                "demo-operator",
                ""
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            request(app.clone(), "GET", "/api/jobs", "demo-reader", "")
                .await
                .1[0]["status"],
            "cancelled"
        );
        let order = r#"{"order_id":"lookup-order","symbol":"RUST","side":"buy","quantity":1}"#;
        assert_eq!(
            request(app.clone(), "POST", "/api/orders", "demo-operator", order)
                .await
                .0,
            StatusCode::OK
        );
        let (status, fill) = request(
            app.clone(),
            "GET",
            "/api/orders/lookup-order",
            "demo-reader",
            "",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(fill["order_id"], "lookup-order");
        assert_eq!(fill["price_cents"], 100);
        assert_eq!(
            request(
                app.clone(),
                "GET",
                "/api/orders/not-present",
                "demo-reader",
                ""
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request(app, "GET", "/api/orders/lookup-order", "unknown", "")
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn worker_processes_and_joins_after_shutdown() {
        let (store, path) = fixture();
        store
            .lock()
            .unwrap()
            .commit(Event::ScheduleReplay(crate::replay::ReplaySpec {
                job_id: "worker".into(),
                ticks: vec![Tick {
                    event_id: "worker-t".into(),
                    symbol: "RUST".into(),
                    price_cents: 1,
                    sequence: 1,
                }],
            }))
            .unwrap();
        let (tx, rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(worker(store.clone(), rx));
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if store.lock().unwrap().engine.jobs()[0].status
                    == crate::replay::ReplayStatus::Completed
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        tx.send(true).unwrap();
        handle.await.unwrap();
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    use axum::{body::Body, http::Request as HttpRequest};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tower::ServiceExt;
    fn fixture() -> (Shared, std::path::PathBuf) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        // Relaxed ordering suffices: this counter allocates unique fixture names only.
        let ordinal = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "course-admission-{}-{}-{}.jsonl",
            ordinal,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (Arc::new(Mutex::new(Store::open(&path).unwrap())), path)
    }
    #[tokio::test]
    async fn overload_rejects_before_domain_work() {
        let (store, path) = fixture();
        let permits = Arc::new(tokio::sync::Semaphore::new(1));
        let held = permits.clone().acquire_owned().await.unwrap();
        let app = router_with_admission(
            store.clone(),
            Admission {
                permits,
                deadline: std::time::Duration::from_secs(1),
            },
        );
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/snapshot")
                    .header("authorization", "Bearer demo-reader")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(store.lock().unwrap().engine.snapshot().events, 0);
        drop(held);
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn slow_request_body_hits_deadline() {
        let (store, path) = fixture();
        let app = router_with_admission(
            store.clone(),
            Admission {
                permits: Arc::new(tokio::sync::Semaphore::new(1)),
                deadline: std::time::Duration::from_millis(30),
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        client.write_all(b"POST /api/orders HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer demo-operator\r\nContent-Type: application/json\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{").await.unwrap();
        let mut bytes = [0; 1024];
        let count =
            tokio::time::timeout(std::time::Duration::from_secs(2), client.read(&mut bytes))
                .await
                .unwrap()
                .unwrap();
        assert!(String::from_utf8_lossy(&bytes[..count]).starts_with("HTTP/1.1 408"));
        assert_eq!(store.lock().unwrap().engine.snapshot().events, 0);
        drop(client);
        server.abort();
        let _ = server.await;
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
}

async fn order_result(
    State(s): State<Shared>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<crate::Fill>, Error> {
    authorize(&h, false)?;
    let result = inspect(s, move |e| Ok(e.fill(&id).cloned())).await?;
    result
        .map(Json)
        .ok_or(Error(StatusCode::NOT_FOUND, "order not found"))
}

#[cfg(test)]
mod commit_status_tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    async fn post(app: Router, path: &str, body: &str) -> StatusCode {
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", "Bearer demo-operator")
                .header("content-type", "application/json")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
    }
    #[tokio::test]
    async fn domain_rejection_is_definite_and_identity_conflict_is_distinct() {
        let path = std::env::temp_dir().join(format!(
            "course-status-domain-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Arc::new(Mutex::new(Store::open(&path).unwrap()));
        let app = router(store.clone());
        let missing = r#"{"order_id":"missing-quote","symbol":"RUST","side":"buy","quantity":1}"#;
        assert_eq!(
            post(app.clone(), "/api/orders", missing).await,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(store.lock().unwrap().snapshot().events, 0);
        assert_eq!(
            post(
                app.clone(),
                "/api/ticks",
                r#"{"event_id":"t","symbol":"RUST","price_cents":1000,"sequence":1}"#
            )
            .await,
            StatusCode::OK
        );
        let before = store.lock().unwrap().snapshot();
        assert_eq!(
            post(
                app.clone(),
                "/api/orders",
                r#"{"order_id":"too-expensive","symbol":"RUST","side":"buy","quantity":10000}"#
            )
            .await,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(store.lock().unwrap().snapshot(), before);
        assert_eq!(
            post(
                app.clone(),
                "/api/orders",
                r#"{"order_id":"known","symbol":"RUST","side":"buy","quantity":1}"#
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            post(
                app,
                "/api/orders",
                r#"{"order_id":"known","symbol":"RUST","side":"buy","quantity":2}"#
            )
            .await,
            StatusCode::CONFLICT
        );
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn write_failure_and_poison_report_uncertainty_not_validation() {
        let path = std::env::temp_dir().join(format!(
            "course-status-storage-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Arc::new(Mutex::new(Store::open(&path).unwrap()));
        let app = router(store.clone());
        store.lock().unwrap().file = std::fs::File::open(&path).unwrap();
        let tick = r#"{"event_id":"t","symbol":"RUST","price_cents":1000,"sequence":1}"#;
        assert_eq!(
            post(app.clone(), "/api/ticks", tick).await,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            post(app, "/api/ticks", tick).await,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(store.lock().unwrap().snapshot().events, 0);
        assert!(store.lock().unwrap().poisoned);
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
}
