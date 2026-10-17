use tokio::{
    sync::{mpsc, watch},
    time::{Instant, sleep_until},
};

pub const MAX_ITEMS: usize = 1024;
#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    Deadline,
    Cancelled,
    Closed,
    Limit,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub items: Vec<u8>,
    pub stop: Stop,
}
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidBudget;
impl std::fmt::Display for InvalidBudget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("maximum item count exceeds 1024")
    }
}
impl std::error::Error for InvalidBudget {}

/// Reuse one pinned timer for the entire collection operation.
/// An observed true shutdown wins; a ready deadline is checked before data or
/// false-valued watch updates. A dropped shutdown owner also ends collection.
/// The caller retains receiver ownership and any unread queued items.
pub async fn collect_until(
    receiver: &mut mpsc::Receiver<u8>,
    deadline: Instant,
    shutdown: &mut watch::Receiver<bool>,
    maximum: usize,
) -> Result<Report, InvalidBudget> {
    if maximum > MAX_ITEMS {
        return Err(InvalidBudget);
    }
    let mut items = Vec::with_capacity(maximum);
    let timer = sleep_until(deadline);
    tokio::pin!(timer);
    loop {
        if *shutdown.borrow() {
            return Ok(Report {
                items,
                stop: Stop::Cancelled,
            });
        }
        if items.len() == maximum {
            return Ok(Report {
                items,
                stop: Stop::Limit,
            });
        }
        tokio::select! {
            biased;
            _ = &mut timer => {
                let stop = if *shutdown.borrow() { Stop::Cancelled } else { Stop::Deadline };
                return Ok(Report { items, stop });
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { return Ok(Report { items, stop: Stop::Cancelled }); }
            }
            next = receiver.recv() => match next {
                Some(value) => items.push(value),
                None => return Ok(Report { items, stop: Stop::Closed }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[tokio::test(start_paused = true)]
    async fn steady_arrivals_do_not_restart_the_absolute_deadline() {
        let (sender, mut receiver) = mpsc::channel(4);
        let (shutdown_owner, mut shutdown) = watch::channel(false);
        let producer = tokio::spawn(async move {
            for value in 1..=4 {
                tokio::time::sleep(Duration::from_millis(4)).await;
                if sender.send(value).await.is_err() {
                    return;
                }
            }
        });
        let start = Instant::now();
        let report = collect_until(
            &mut receiver,
            start + Duration::from_millis(10),
            &mut shutdown,
            10,
        )
        .await
        .unwrap();
        assert_eq!(
            report,
            Report {
                items: vec![1, 2],
                stop: Stop::Deadline
            }
        );
        assert_eq!(Instant::now() - start, Duration::from_millis(10));
        producer.abort();
        assert!(producer.await.unwrap_err().is_cancelled());
        drop(shutdown_owner);
    }
    #[tokio::test(start_paused = true)]
    async fn limit_preserves_unread_channel_items() {
        let (sender, mut receiver) = mpsc::channel(2);
        sender.send(7).await.unwrap();
        sender.send(8).await.unwrap();
        let (owner, mut shutdown) = watch::channel(false);
        let report = collect_until(
            &mut receiver,
            Instant::now() + Duration::from_secs(1),
            &mut shutdown,
            1,
        )
        .await
        .unwrap();
        assert_eq!(
            report,
            Report {
                items: vec![7],
                stop: Stop::Limit
            }
        );
        assert_eq!(receiver.recv().await, Some(8));
        drop(owner);
    }
    #[tokio::test(start_paused = true)]
    async fn closed_cancelled_and_invalid_are_distinct() {
        let (sender, mut receiver) = mpsc::channel(1);
        drop(sender);
        let (owner, mut shutdown) = watch::channel(false);
        assert_eq!(
            collect_until(
                &mut receiver,
                Instant::now() + Duration::from_secs(1),
                &mut shutdown,
                2
            )
            .await
            .unwrap()
            .stop,
            Stop::Closed
        );
        owner.send(true).unwrap();
        assert_eq!(
            collect_until(&mut receiver, Instant::now(), &mut shutdown, 2)
                .await
                .unwrap()
                .stop,
            Stop::Cancelled
        );
        assert_eq!(
            collect_until(&mut receiver, Instant::now(), &mut shutdown, MAX_ITEMS + 1).await,
            Err(InvalidBudget)
        );
    }
}
