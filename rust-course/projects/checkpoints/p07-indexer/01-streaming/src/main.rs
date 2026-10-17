fn main() -> std::io::Result<()> {
    let result = checkpoint_p07_streaming::count(&b"hello world\nlast"[..], 1024)?;
    println!(
        "bytes={} lines={} words={}",
        result.bytes, result.lines, result.words
    );
    Ok(())
}
