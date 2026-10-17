use std::process::Command;

#[test]
fn one_count_is_accepted_but_malformed_and_extra_arguments_are_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_p12-replay"))
        .arg("3")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("events=3"));
    for args in [vec!["3", "extra"], vec!["-1"], vec!["3.5"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_p12-replay"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_count_is_an_input_error() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let output = Command::new(env!("CARGO_BIN_EXE_p12-replay"))
        .arg(OsString::from_vec(vec![0xff]))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("UTF-8"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

#[test]
fn broken_output_and_diagnostic_pipes_do_not_panic() {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_p12-replay"))
        .arg("3")
        .stdout(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_p12-replay"))
        .arg("invalid")
        .stderr(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}
