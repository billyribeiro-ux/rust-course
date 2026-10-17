use std::process::Command;

#[test]
fn progress_uses_stderr_while_sorted_results_stay_on_stdout() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_p07-indexer"))
        .arg(&source)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("progress started received=0/1"));
    assert!(stderr.contains("progress running received=1/1 indexed=1"));
    assert!(stderr.contains("progress finished received=1/1 indexed=1"));
    assert!(stderr.contains("submitted=1 without_result=0 unsubmitted=0"));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(" Indexed"));
    assert!(stdout.contains("submitted=1"));
    assert!(!stdout.contains("progress "));
}

#[cfg(unix)]
#[test]
fn closed_progress_sink_is_an_error_without_a_success_report() {
    use std::{os::fd::OwnedFd, os::unix::net::UnixStream, process::Stdio};
    let (writer, reader) = UnixStream::pair().unwrap();
    drop(reader);
    let writer: OwnedFd = writer.into();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_p07-indexer"))
        .arg(source)
        .stderr(Stdio::from(writer))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[cfg(unix)]
#[test]
fn closed_result_sink_returns_an_io_error_without_panicking() {
    use std::{io::pipe, process::Stdio};
    let (reader, writer) = pipe().unwrap();
    drop(reader);
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_p07-indexer"))
        .arg(source)
        .stdout(Stdio::from(writer))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("progress finished received=1/1"));
    assert!(stderr.contains("BrokenPipe"));
    assert!(!stderr.contains("panicked"));
}
