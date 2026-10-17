use std::process::Command;

#[test]
fn fixture_output_is_exact_and_batch_independent() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/ticks.csv");
    let mut baseline = None;
    for batch in ["1", "2", "64", "1024"] {
        let output = Command::new(env!("CARGO_BIN_EXE_tick_replay"))
            .args([path, batch])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout.clone()).unwrap();
        assert!(text.contains("events=5"));
        assert!(text.contains("ALFA,3,7,6992500,100.0000,99.5000,99.5000,101.2500\n"));
        assert!(text.contains("BETA,2,5,2505000,50.5000,49.5000,49.5000,50.5000\n"));
        match &baseline {
            None => baseline = Some(output.stdout),
            Some(expected) => assert_eq!(&output.stdout, expected),
        }
    }
}

#[test]
fn unknown_or_excess_arguments_and_bad_limits_fail_without_a_summary() {
    for args in [
        vec![],
        vec!["missing", "0"],
        vec!["missing", "1025"],
        vec!["missing", "1", "extra"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_tick_replay"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_batch_is_an_error_and_native_path_failure_is_not_a_panic() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    for args in [
        vec![OsString::from("missing"), OsString::from_vec(vec![0xff])],
        vec![OsString::from_vec(b"missing-\xff".to_vec())],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_tick_replay"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
}

#[test]
fn broken_output_and_diagnostic_pipes_do_not_panic() {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_tick_replay"))
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/ticks.csv"))
        .stdout(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_tick_replay"))
        .stderr(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}
