use std::{
    io::{self, Write},
    net::SocketAddr,
    process::ExitCode,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::watch,
};
fn arguments() -> io::Result<Vec<SocketAddr>> {
    let mut addresses = Vec::new();
    for argument in std::env::args_os().skip(1) {
        if addresses.len() == 128 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "at most 128 endpoints are supported",
            ));
        }
        let text = argument.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "endpoint must be valid UTF-8")
        })?;
        let address = text.parse::<SocketAddr>().map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("expected IP:PORT endpoint: {error}"),
            )
        })?;
        if !address.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "only literal loopback endpoints are supported",
            ));
        }
        addresses.push(address);
    }
    Ok(addresses)
}

async fn run() -> io::Result<()> {
    // Parse every endpoint before creating a listener, signal task or network request.
    let mut addresses = arguments()?;
    let mut demo = None;
    if addresses.is_empty() {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        demo = Some(tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0; 512];
                let _ = stream.read(&mut buf).await;
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK",
                    )
                    .await;
            }
        }));
        addresses.push(address);
    }
    let (tx, rx) = watch::channel(false);
    let signal = tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = tx.send(true);
    });
    let result = p08_monitor::monitor(addresses, p08_monitor::Config::default(), rx).await;
    signal.abort();
    let _ = signal.await;
    if let Some(demo) = demo {
        demo.abort();
        let _ = demo.await;
    }
    let report = result?;
    // All async work is drained before taking the synchronous stdout lock.
    let mut output = io::stdout().lock();
    for row in report.rows {
        writeln!(
            output,
            "{} {:?} attempts={}",
            row.address, row.status, row.attempts
        )?;
    }
    writeln!(output, "peak_in_flight={}", report.peak_in_flight)?;
    output.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "monitor: {error}");
            if error.kind() == io::ErrorKind::InvalidInput {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}
