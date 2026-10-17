//! Real loopback route checks use a lazy pool and never contact PostgreSQL.
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[tokio::test]
async fn authorization_and_input_rejection_precede_database_work() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_millis(20))
        .connect_lazy("postgresql://localhost:9/unused_fixture")
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, p14_scheduler::http::router(pool))
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    let cases = [
        (
            "POST",
            "/jobs",
            "",
            r#"{"id":"a","payload":"work"}"#.to_owned(),
            401,
        ),
        (
            "POST",
            "/jobs",
            "Bearer demo-reader",
            r#"{"id":"a","payload":"work"}"#.to_owned(),
            401,
        ),
        ("POST", "/jobs/a/cancel", "", String::new(), 401),
        (
            "POST",
            "/jobs/a/cancel",
            "Bearer unknown",
            String::new(),
            401,
        ),
        (
            "POST",
            "/jobs",
            "Bearer demo-operator",
            r#"{"id":"","payload":"work"}"#.to_owned(),
            422,
        ),
        (
            "POST",
            "/jobs",
            "Bearer demo-operator",
            serde_json::json!({"id":"a","payload":"x".repeat(1001)}).to_string(),
            422,
        ),
        ("POST", "/jobs", "Bearer demo-operator", "{".into(), 400),
        (
            "POST",
            "/jobs",
            "Bearer demo-operator",
            "x".repeat(4097),
            413,
        ),
        ("GET", "/health", "", String::new(), 200),
    ];
    let mut observed = Vec::new();
    for (method, path, identity, body, expected) in cases {
        let response = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
            let request = format!("{method} {path} HTTP/1.1\r\nHost: {address}\r\nAuthorization: {identity}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut response = Vec::new();
            stream.take(8193).read_to_end(&mut response).await.unwrap();
            response
        }).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        let status = response
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u16>()
            .unwrap();
        observed.push((path.to_owned(), status, expected));
    }
    stop.send(()).unwrap();
    server.await.unwrap();
    for (route, status, expected) in observed {
        assert_eq!(status, expected, "{route}");
    }
}
