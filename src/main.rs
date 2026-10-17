use axum::{Router, routing::get};
use std::io::{self, Write};

#[tokio::main]
async fn main() -> io::Result<()> {
    let app = Router::new().route("/", get(|| async { "Hello, Axum!\n" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    {
        let mut output = io::stdout().lock();
        writeln!(output, "Server running at http://{}", listener.local_addr()?)?;
        output.flush()?;
    }
    axum::serve(listener, app).await
}
