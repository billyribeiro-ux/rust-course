#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:4109").await?;
    let (tx, rx) = tokio::sync::watch::channel(false);
    {
        let mut output = std::io::stdout().lock();
        writeln!(
            output,
            "Local framed chat on 127.0.0.1:4109; Ctrl-C stops it"
        )?;
        output.flush()?;
    }
    let server = p09_chat::serve(listener, rx);
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => result,
        signal = tokio::signal::ctrl_c() => {
            // Keep polling the same server future until it drains its owned tasks.
            let _ = tx.send(true);
            let result = server.await;
            signal?;
            result
        }
    }
}
use std::io::Write;
