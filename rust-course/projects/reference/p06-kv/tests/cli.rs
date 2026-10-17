//! Exercise the real process boundary, including native paths and rejected requests.
use std::{ffi::OsStr, fs, path::PathBuf, process::Command};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        // Wall-clock timestamps can repeat across concurrent test threads.
        // Atomic allocation plus create_dir's no-overwrite check establishes ownership.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("course-kv-cli-{}-{sequence}", std::process::id()));
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

fn rejected(path: &OsStr, args: &[&OsStr]) {
    let output = Command::new(env!("CARGO_BIN_EXE_p06-kv"))
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    assert!(output.stdout.is_empty());
}

#[test]
fn rejected_commands_neither_create_files_nor_repair_an_existing_torn_tail() {
    let scratch = Scratch::new();
    let missing = scratch.0.join("missing");
    let torn = scratch.0.join("torn");
    fs::write(&torn, b"KV").unwrap();
    let long_key = "x".repeat(p06_kv::MAX_KEY + 1);
    let cases: Vec<Vec<&OsStr>> = vec![
        vec![],
        vec![OsStr::new("typo")],
        vec![OsStr::new("put"), OsStr::new("key")],
        vec![OsStr::new("get"), OsStr::new("key"), OsStr::new("extra")],
        vec![OsStr::new("delete"), OsStr::new("")],
        vec![OsStr::new("put"), OsStr::new(&long_key), OsStr::new("v")],
        vec![OsStr::new("compact"), OsStr::new("extra")],
    ];
    for args in cases {
        rejected(missing.as_os_str(), &args);
        rejected(torn.as_os_str(), &args);
        assert!(!missing.exists());
        assert!(!scratch.0.join("missing.lock").exists());
        assert!(!scratch.0.join("torn.lock").exists());
        assert_eq!(fs::read(&torn).unwrap(), b"KV");
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_text_is_an_input_error_and_native_unicode_paths_round_trip() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let scratch = Scratch::new();
    let path = scratch.0.join("native-café");
    let invalid = OsString::from_vec(vec![0xff]);
    for args in [
        vec![invalid.as_os_str()],
        vec![OsStr::new("get"), invalid.as_os_str()],
        vec![OsStr::new("put"), OsStr::new("key"), invalid.as_os_str()],
    ] {
        rejected(path.as_os_str(), &args);
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 0);
    }
    fs::create_dir(&path).unwrap();
    let path = path.join("data");
    for (args, expected) in [
        (vec!["put", "café", "value"], ""),
        (vec!["get", "café"], "value\n"),
        (vec!["delete", "café"], ""),
        (vec!["get", "café"], "(missing)\n"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_p06-kv"))
            .arg(&path)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
    }
    assert!(path.exists());
    assert!(!scratch.0.join("native-�").exists());
}

#[cfg(unix)]
#[test]
fn non_utf8_native_path_obeys_filesystem_support_without_panicking() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let scratch = Scratch::new();
    let directory = scratch.0.join(OsString::from_vec(b"native-\xff".to_vec()));
    // Some Unix filesystems reject non-UTF-8 names. That is an I/O failure,
    // not a reason for argument iteration to panic or rewrite the path.
    let directory_result = fs::create_dir(&directory);
    let path = directory.join("data");
    let output = Command::new(env!("CARGO_BIN_EXE_p06-kv"))
        .arg(&path)
        .args(["put", "key", "value"])
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    if directory_result.is_ok() {
        assert!(output.status.success(), "{output:?}");
        let output = Command::new(env!("CARGO_BIN_EXE_p06-kv"))
            .arg(&path)
            .args(["get", "key"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"value\n");
    } else {
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty());
    }
    assert!(!scratch.0.join("native-�").exists());
}

#[test]
fn broken_output_and_diagnostic_pipes_are_errors_without_a_panic() {
    let scratch = Scratch::new();
    let path = scratch.0.join("data");
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_p06-kv"))
        .arg(&path)
        .args(["get", "key"])
        .stdout(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_p06-kv"))
        .arg(&path)
        .arg("typo")
        .stderr(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    // Warning output is best effort after an authorized valid command repairs.
    fs::write(&path, b"KV").unwrap();
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_p06-kv"))
        .arg(&path)
        .args(["get", "key"])
        .stderr(writer)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"(missing)\n");
    assert_eq!(fs::read(path).unwrap(), b"");
}
