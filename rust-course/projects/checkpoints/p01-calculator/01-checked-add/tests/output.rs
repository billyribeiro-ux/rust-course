use std::{io::pipe, process::Command};
#[test]
fn closed_output_returns_failure_without_panicking() {
    let (reader, writer) = pipe().unwrap();
    drop(reader);
    let result = Command::new(env!("CARGO_BIN_EXE_checkpoint-p01-calculator"))
        .stdout(writer)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(
        !String::from_utf8(result.stderr)
            .unwrap()
            .contains("panicked")
    );
}
