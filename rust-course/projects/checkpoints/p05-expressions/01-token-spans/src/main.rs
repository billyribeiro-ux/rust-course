use checkpoint_p05_expressions::{Error, lex};
fn main() -> Result<(), Error> {
    for token in lex("12 + 3*4")? {
        println!("{:?} {}..{}", token.kind, token.start, token.end);
    }
    Ok(())
}
