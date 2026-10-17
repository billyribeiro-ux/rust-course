use std::{future::IntoFuture, io::Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Never format VarError::NotUnicode: it carries the original potential credential.
    let url = std::env::var("SCHEDULER_DATABASE_URL")
        .map_err(|_| "SCHEDULER_DATABASE_URL must contain a valid UTF-8 test/application URL")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(3))
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::raw_sql("SET statement_timeout = '3000ms'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    // Admission and required startup output must succeed before work is spawned.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:4114").await?;
    {
        let mut output = std::io::stdout().lock();
        writeln!(
            output,
            "Local scheduler :4114; one worker, five-second leases"
        )?;
        output.flush()?;
    }
    let worker_pool = pool.clone();
    let (stop, mut stopping) = tokio::sync::watch::channel(false);
    let mut server_stopping = stop.subscribe();
    let worker = tokio::spawn(async move {
        let mut clock = tokio::time::interval(std::time::Duration::from_millis(200));
        loop {
            if *stopping.borrow_and_update() {
                break;
            }
            tokio::select! {
                changed = stopping.changed() => { if changed.is_err() { break; } },
                _ = clock.tick() => {
                    // Worker diagnostics are best-effort observations, not a durable audit.
                    // A closed output stream must not panic and abandon an owned lease.
                    let now=match p14_scheduler::now_ms(){Ok(now)=>now,Err(error)=>{let _=writeln!(std::io::stderr().lock(),"worker clock error: {error}");break;}};
                    match p14_scheduler::claim(&worker_pool,now).await {
                        Ok(Some(lease)) => {
                            match p14_scheduler::complete_with_clock(&worker_pool,&lease,p14_scheduler::now_ms).await {
                                Ok(done)=>{let _=writeln!(std::io::stdout().lock(),"job={} token={} committed={done}",lease.id,lease.token);},
                                Err(error)=>{let _=writeln!(std::io::stderr().lock(),"worker storage error: {error}");},
                            }
                        }
                        Ok(None)=>{},
                        Err(error)=>{let _=writeln!(std::io::stderr().lock(),"claim error: {error}");},
                    }
                }
            }
        }
    });
    let app = p14_scheduler::http::router(pool.clone());
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
    // Every server exit, including an error, revokes the worker before propagating errors.
    let _ = stop.send(true);
    let worker_result = worker.await;
    pool.close().await;
    result?;
    worker_result?;
    signal_result?;
    Ok(())
}
