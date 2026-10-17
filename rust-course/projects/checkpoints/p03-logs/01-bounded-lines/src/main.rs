use checkpoint_p03_logs::{Line, read_line};
fn main() -> std::io::Result<()> {
    let mut input = &b"too-long\nok\nlast"[..];
    loop {
        let line = read_line(&mut input, 4)?;
        println!("{line:?}");
        if line == Line::End {
            break;
        }
    }
    Ok(())
}
