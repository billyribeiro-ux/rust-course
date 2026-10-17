#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Native paths need not be UTF-8. Only an absent setting selects a default.
    let path = std::env::var_os("WORKBENCH_LOG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("workbench-events.jsonl"));
    let address: std::net::SocketAddr = match std::env::var("WORKBENCH_ADDR") {
        Ok(value) => value.parse()?,
        Err(std::env::VarError::NotPresent) => ([127, 0, 0, 1], 4116).into(),
        Err(error) => return Err(error.into()),
    };
    if !address.ip().is_loopback() {
        return Err("teaching identity fixtures require loopback binding".into());
    }
    // Validate configuration before creating or replaying persistent state.
    let store = std::sync::Arc::new(std::sync::Mutex::new(p16_workbench::Store::open(&path)?));
    let listener = tokio::net::TcpListener::bind(address).await?;
    // Required banners must be writable before a replay worker can commit events.
    {
        let mut output = std::io::stdout().lock();
        writeln!(output, "LISTEN_ADDR={}", listener.local_addr()?)?;
        writeln!(
            output,
            "SIMULATED ONLY; one journal writer and replay worker; journal={path:?}"
        )?;
        output.flush()?;
    }
    let (stop, stopping) = tokio::sync::watch::channel(false);
    let mut server_stopping = stop.subscribe();
    let worker = tokio::spawn(p16_workbench::http::worker(store.clone(), stopping));
    let app = p16_workbench::http::router(store);
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            while !*server_stopping.borrow_and_update() {
                if server_stopping.changed().await.is_err() {
                    break;
                }
            }
        })
        .into_future();
    tokio::pin!(server);
    let (result, signal_result) = tokio::select! {
        result = &mut server => (result, Ok(())),
        signal = tokio::signal::ctrl_c() => {
            let _ = stop.send(true);
            (server.await, signal)
        }
    };
    let _ = stop.send(true);
    let worker_result = worker.await;
    result?;
    worker_result?;
    signal_result?;
    Ok(())
}
use std::{future::IntoFuture, io::Write};
