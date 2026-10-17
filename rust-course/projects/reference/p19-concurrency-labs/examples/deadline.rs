use concurrency_labs::deadlines::collect_until;
use tokio::{
    sync::{mpsc, watch},
    time::{Duration, Instant},
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (sender, mut receiver) = mpsc::channel(2);
    sender.send(7).await?;
    sender.send(8).await?;
    drop(sender);
    let (shutdown_owner, mut shutdown) = watch::channel(false);
    let report = collect_until(
        &mut receiver,
        Instant::now() + Duration::from_secs(1),
        &mut shutdown,
        10,
    )
    .await?;
    println!("items={:?} stop={:?}", report.items, report.stop);
    drop(shutdown_owner);
    Ok(())
}
