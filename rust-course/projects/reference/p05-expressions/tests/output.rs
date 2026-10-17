//! Actual OS-pipe regression: the only read end is closed before the child starts.
use std::{io::pipe, process::Command};

#[test]
fn closed_stdout_is_a_reported_io_failure_not_a_panic() {
    let (reader, writer) = pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_expression-language"))
        .env_clear()
        .args(["1+2"])
        .stdout(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(!diagnostic.is_empty());
    assert!(!diagnostic.contains("panicked"), "{diagnostic}");
}

#[test]
fn closed_diagnostic_sink_does_not_panic() {
    let (reader, writer) = pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_expression-language"))
        .env_clear()
        .args(["--invalid"])
        .stderr(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}
