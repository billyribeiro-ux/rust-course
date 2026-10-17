#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut bytes = &[0, 0, 0, 2, 0xc3, 0xa9][..];
    let message = p09_chat::read_frame(&mut bytes).await?;
    println!("{message}: {} bytes", message.len());
    Ok(())
}
