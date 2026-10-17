use std::{fs, path::PathBuf};
use task_journal::{Journal, Status};
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        // Wall-clock timestamps can repeat across concurrent test threads.
        // Atomic allocation plus create_dir's no-overwrite check establishes ownership.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "rust-course-journal-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create test directory: {error}"),
            }
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn saves_reloads_and_preserves_corrupt_input() {
    let scratch = Scratch::new();
    let path = scratch.0.join("tasks.txt");
    let mut journal = Journal::load(&path).unwrap();
    let id = journal.add("real filesystem roundtrip").unwrap();
    journal.save(&path).unwrap();
    let mut loaded = Journal::load(&path).unwrap();
    loaded.complete(id).unwrap();
    loaded.save(&path).unwrap();
    assert_eq!(
        Journal::load(&path).unwrap().tasks()[0].status(),
        Status::Done
    );
    fs::write(&path, "corrupt data").unwrap();
    assert!(Journal::load(&path).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "corrupt data");
}
#[test]
fn checked_in_fixture_roundtrips() {
    let original = include_str!("../fixtures/tasks-v1.txt");
    assert_eq!(Journal::decode(original).unwrap().encode(), original);
    assert!(Journal::decode(include_str!("../fixtures/corrupt-duplicate.txt")).is_err());
}
