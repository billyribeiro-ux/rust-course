use checkpoint_p04_config::{Error, resolve};
fn main() -> Result<(), Error> {
    let config = resolve(&[Some("invalid-earlier-value"), None, Some("9000")])?;
    println!("effective port: {}", config.port());
    Ok(())
}
