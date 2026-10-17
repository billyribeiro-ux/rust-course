//! Actual OS-pipe regression: the only read end is closed before the child starts.
use std::{io::pipe, process::Command};

#[test]
fn closed_stdout_is_a_reported_io_failure_not_a_panic() {
    let (reader, writer) = pipe().unwrap();
    drop(reader);
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tasks-v1.txt");
    let output = Command::new(env!("CARGO_BIN_EXE_task-journal"))
        .env_clear()
        .args([fixture.as_os_str(), std::ffi::OsStr::new("list")])
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
    let output = Command::new(env!("CARGO_BIN_EXE_task-journal"))
        .env_clear()
        .args(["missing-file", "wrong-command"])
        .stderr(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn failed_acknowledgement_does_not_undo_saved_task() {
    let directory =
        std::env::temp_dir().join(format!("rust-course-journal-output-{}", std::process::id()));
    // create_dir must succeed before this test owns (and may remove) the path.
    std::fs::create_dir(&directory).unwrap();
    struct OwnedDirectory(std::path::PathBuf);
    impl Drop for OwnedDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let owned = OwnedDirectory(directory);
    let path = owned.0.join("journal.txt");
    let (reader, writer) = pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_task-journal"))
        .arg(&path)
        .args(["add", "saved despite lost output"])
        .stdout(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let journal = task_journal::Journal::load(&path).unwrap();
    assert_eq!(journal.tasks().len(), 1);
    assert_eq!(journal.tasks()[0].title(), "saved despite lost output");
}
