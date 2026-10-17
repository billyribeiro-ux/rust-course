use checkpoint_p06_kv::{Error, replay};
fn main() -> Result<(), Error> {
    println!("{:?}", replay(&[1, 7, 7, 2, 4])?);
    Ok(())
}
