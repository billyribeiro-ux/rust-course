use std::{future::IntoFuture, io::Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // VarError::NotUnicode contains the original value; do not expose credentials.
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL must contain a valid UTF-8 test/application URL")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(3))
        .connect(&url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:4110").await?;
    {
        let mut output = std::io::stdout().lock();
        writeln!(
            output,
            "Local DEMO identity issue API: http://127.0.0.1:4110"
        )?;
        output.flush()?;
    }
    let (stop, mut stopping) = tokio::sync::watch::channel(false);
    let server = axum::serve(listener, p10_issues::router(pool.clone()))
        .with_graceful_shutdown(async move {
            while !*stopping.borrow_and_update() {
                if stopping.changed().await.is_err() {
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
    pool.close().await;
    result?;
    signal_result?;
    Ok(())
}
