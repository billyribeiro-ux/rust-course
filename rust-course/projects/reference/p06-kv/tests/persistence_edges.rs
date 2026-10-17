//! Focused recovery audit: only owned temporary files and one isolated child process.
use p06_kv::{Store, replay};
use std::{fs, path::PathBuf, process::Command};

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        // Wall-clock timestamps can repeat across concurrent test threads.
        // Atomic allocation plus create_dir's no-overwrite check establishes ownership.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "course-kv-persistence-{label}-{}-{sequence}",
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
fn relative_store_keeps_its_file_when_process_cwd_changes() {
    let directory = Scratch::new("cwd");
    // Changing cwd is process-global, so perform it only in a single-test child.
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "cwd_child", "--nocapture"])
        .env("RUST_COURSE_KV_CWD_CHILD", "1")
        .current_dir(&directory.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cwd_child() {
    if std::env::var_os("RUST_COURSE_KV_CWD_CHILD").is_none() {
        return;
    }
    fs::create_dir("later").unwrap();
    let mut store = Store::open("data").unwrap();
    store.put(b"a", b"before").unwrap();
    std::env::set_current_dir("later").unwrap();
    #[cfg(unix)]
    store.compact().unwrap();
    store.put(b"b", b"after").unwrap();
    drop(store);
    let recovered = Store::open("../data").unwrap();
    assert_eq!(recovered.get(b"b"), Some(b"after".as_slice()));
    assert!(
        !std::path::Path::new("data").exists(),
        "compaction created another data file in the new cwd"
    );
}

#[test]
fn bounded_length_damage_can_resemble_an_incomplete_final_payload() {
    let directory = Scratch::new("length");
    let path = directory.0.join("data");
    {
        let mut store = Store::open(&path).unwrap();
        store.put(b"a", b"one").unwrap();
    }
    let mut bytes = fs::read(&path).unwrap();
    // The original record was complete. Change only its declared value length
    // from 3 to 4, still well inside the bounds. KV01 has no separate header CRC.
    bytes[8..12].copy_from_slice(&4u32.to_le_bytes());
    let recovery = replay(bytes.as_slice()).unwrap();
    assert!(recovery.torn_tail);
    assert_eq!(recovery.valid_bytes, 0);
    assert!(recovery.entries.is_empty());
    // Demonstrate the documented policy on this disposable damaged copy only.
    fs::write(&path, &bytes).unwrap();
    let store = Store::open(&path).unwrap();
    assert!(store.recovered_torn_tail);
    assert_eq!(fs::metadata(&path).unwrap().len(), 0);
}
