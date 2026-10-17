//! A bounded one-round HTTP health monitor for literal loopback socket addresses.
use std::{io, net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::watch,
    task::JoinSet,
    time::{sleep, timeout},
};

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub concurrency: usize,
    pub attempts: usize,
    pub attempt_timeout: Duration,
    pub endpoint_timeout: Duration,
    pub backoff: Duration,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            concurrency: 4,
            attempts: 2,
            attempt_timeout: Duration::from_millis(300),
            endpoint_timeout: Duration::from_secs(1),
            backoff: Duration::from_millis(25),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Healthy(u16),
    Unhealthy(u16),
    TimedOut,
    Failed(io::ErrorKind),
    InvalidResponse,
    Cancelled,
}
#[derive(Clone, Debug)]
pub struct ResultRow {
    pub address: SocketAddr,
    pub status: Status,
    pub attempts: usize,
}
#[derive(Debug)]
pub struct Report {
    pub rows: Vec<ResultRow>,
    pub peak_in_flight: usize,
}

async fn cancelled(mut shutdown: watch::Receiver<bool>) {
    loop {
        if *shutdown.borrow() {
            return;
        }
        if shutdown.changed().await.is_err() {
            return;
        }
    }
}

async fn request(address: SocketAddr) -> io::Result<Status> {
    let mut stream = TcpStream::connect(address).await?;
    let request = format!("GET /health HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    let mut line = [0u8; 256];
    let mut used = 0;
    while used < line.len() {
        let n = stream.read(&mut line[used..used + 1]).await?;
        if n == 0 {
            return Ok(Status::InvalidResponse);
        }
        used += 1;
        if used >= 2 && &line[used - 2..used] == b"\r\n" {
            let Ok(text) = std::str::from_utf8(&line[..used - 2]) else {
                return Ok(Status::InvalidResponse);
            };
            let mut parts = text.split_ascii_whitespace();
            if !matches!(parts.next(), Some("HTTP/1.1" | "HTTP/1.0")) {
                return Ok(Status::InvalidResponse);
            }
            let Some(code) = parts
                .next()
                .filter(|s| s.len() == 3)
                .and_then(|s| s.parse::<u16>().ok())
            else {
                return Ok(Status::InvalidResponse);
            };
            return Ok(if (200..300).contains(&code) {
                Status::Healthy(code)
            } else if (100..600).contains(&code) {
                Status::Unhealthy(code)
            } else {
                Status::InvalidResponse
            });
        }
    }
    Ok(Status::InvalidResponse)
}

async fn probe(address: SocketAddr, config: Config, shutdown: watch::Receiver<bool>) -> ResultRow {
    let mut attempts = 0;
    let work = async {
        let mut status = Status::TimedOut;
        for attempt in 0..config.attempts {
            if attempt > 0 {
                sleep(config.backoff).await;
            }
            attempts += 1;
            status = match timeout(config.attempt_timeout, request(address)).await {
                Ok(Ok(status)) => status,
                Ok(Err(error)) => Status::Failed(error.kind()),
                Err(_) => Status::TimedOut,
            };
            if matches!(status, Status::Healthy(_)) {
                break;
            }
        }
        status
    };
    let status = tokio::select! {
        biased;
        () = cancelled(shutdown) => Status::Cancelled,
        result = timeout(config.endpoint_timeout, work) => result.unwrap_or(Status::TimedOut),
    };
    ResultRow {
        address,
        status,
        attempts,
    }
}

pub async fn monitor(
    addresses: Vec<SocketAddr>,
    config: Config,
    shutdown: watch::Receiver<bool>,
) -> io::Result<Report> {
    if addresses.len() > 128
        || addresses.iter().any(|a| !a.ip().is_loopback())
        || !(1..=32).contains(&config.concurrency)
        || !(1..=4).contains(&config.attempts)
        || config.attempt_timeout.is_zero()
        || config.attempt_timeout > Duration::from_secs(10)
        || config.endpoint_timeout.is_zero()
        || config.endpoint_timeout > Duration::from_secs(30)
        || config.backoff > Duration::from_secs(1)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "literal loopback endpoints and bounded nonzero limits required",
        ));
    }
    let mut tasks = JoinSet::new();
    let mut next = 0;
    let mut peak = 0;
    let mut rows = vec![None; addresses.len()];
    loop {
        if *shutdown.borrow() || shutdown.has_changed().is_err() {
            tasks.abort_all();
            break;
        }
        while next < addresses.len() && tasks.len() < config.concurrency {
            let index = next;
            let address = addresses[index];
            let shutdown = shutdown.clone();
            tasks.spawn(async move { (index, probe(address, config, shutdown).await) });
            next += 1;
        }
        peak = peak.max(tasks.len());
        if tasks.is_empty() {
            break;
        }
        tokio::select! {
            biased;
            () = cancelled(shutdown.clone()) => { tasks.abort_all(); break; }
            result = tasks.join_next() => match result {
                Some(Ok((index, row))) => rows[index] = Some(row),
                Some(Err(error)) => { tasks.abort_all(); while tasks.join_next().await.is_some() {} return Err(io::Error::other(error)); },
                None => break,
            }
        }
    }
    // Await aborted tasks: no detached work remains after this function returns.
    while tasks.join_next().await.is_some() {}
    let rows = addresses
        .into_iter()
        .enumerate()
        .map(|(index, address)| {
            rows[index].take().unwrap_or(ResultRow {
                address,
                status: Status::Cancelled,
                attempts: 0,
            })
        })
        .collect();
    Ok(Report {
        rows,
        peak_in_flight: peak,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    async fn server(responses: Vec<&'static [u8]>) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0; 512];
                let _ = stream.read(&mut request).await;
                stream.write_all(response).await.unwrap();
            }
        });
        (address, task)
    }
    #[tokio::test]
    async fn retries_then_recovers() {
        let (address, task) = server(vec![
            b"HTTP/1.1 503 Unavailable\r\n\r\n",
            b"HTTP/1.1 200 OK\r\n\r\n",
        ])
        .await;
        let (_tx, rx) = watch::channel(false);
        let report = monitor(vec![address], Config::default(), rx).await.unwrap();
        assert_eq!(report.rows[0].status, Status::Healthy(200));
        assert_eq!(report.rows[0].attempts, 2);
        task.await.unwrap();
    }
    #[tokio::test]
    async fn malformed_response_is_bounded() {
        let (address, task) = server(vec![b"not-http\r\n"]).await;
        let (_tx, rx) = watch::channel(false);
        let report = monitor(
            vec![address],
            Config {
                attempts: 1,
                ..Config::default()
            },
            rx,
        )
        .await
        .unwrap();
        assert_eq!(report.rows[0].status, Status::InvalidResponse);
        task.await.unwrap();
    }
    #[tokio::test]
    async fn hung_endpoint_has_a_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let (_tx, rx) = watch::channel(false);
        let config = Config {
            attempts: 1,
            attempt_timeout: Duration::from_millis(30),
            ..Config::default()
        };
        let report = timeout(Duration::from_secs(3), monitor(vec![address], config, rx))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(report.rows[0].status, Status::TimedOut);
        task.abort();
        let _ = task.await;
    }
    #[tokio::test]
    async fn concurrency_cap_and_input_order_hold() {
        let (a, one) = server(vec![b"HTTP/1.1 200 OK\r\n\r\n"]).await;
        let (b, two) = server(vec![b"HTTP/1.1 204 No Content\r\n\r\n"]).await;
        let (_tx, rx) = watch::channel(false);
        let report = monitor(
            vec![b, a],
            Config {
                concurrency: 1,
                ..Config::default()
            },
            rx,
        )
        .await
        .unwrap();
        assert_eq!(report.peak_in_flight, 1);
        assert_eq!(report.rows[0].address, b);
        assert_eq!(report.rows[1].address, a);
        one.await.unwrap();
        two.await.unwrap();
    }
    #[tokio::test]
    async fn shutdown_aborts_in_flight_and_unstarted_work() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            let _ = accepted_tx.send(());
            std::future::pending::<()>().await;
        });
        let (tx, rx) = watch::channel(false);
        let work = tokio::spawn(monitor(
            vec![address; 3],
            Config {
                concurrency: 1,
                ..Config::default()
            },
            rx,
        ));
        accepted_rx.await.unwrap();
        tx.send(true).unwrap();
        let report = timeout(Duration::from_secs(3), work)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            report
                .rows
                .iter()
                .all(|row| row.status == Status::Cancelled)
        );
        server.abort();
        let _ = server.await;
    }
    #[tokio::test]
    async fn rejects_remote_addresses() {
        let (_tx, rx) = watch::channel(false);
        assert!(
            monitor(vec!["192.0.2.1:80".parse().unwrap()], Config::default(), rx)
                .await
                .is_err()
        );
    }
}
