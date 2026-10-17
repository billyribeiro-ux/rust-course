//! Read failures must preserve the distinction between invalid input and unavailable state.
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use p16_workbench::{Store, http::router};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[tokio::test]
async fn poisoned_state_is_unavailable_on_every_read_route() {
    let path = std::env::temp_dir().join(format!(
        "course-read-poison-{}-{}.jsonl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Arc::new(Mutex::new(Store::open(&path).unwrap()));
    let worker = Arc::clone(&store);
    // Deliberately panic while holding the guard; join contains the injected failure.
    assert!(
        std::thread::spawn(move || {
            let _guard = worker.lock().unwrap();
            panic!("injected state-owner panic");
        })
        .join()
        .is_err()
    );
    let app = router(Arc::clone(&store));
    for route in [
        "/api/snapshot",
        "/api/jobs",
        "/api/history?limit=10",
        "/api/orders/missing",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(route)
                    .header("authorization", "Bearer demo-reader")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{route}"
        );
    }
    drop(app);
    drop(store);
    std::fs::remove_file(path).unwrap();
}
