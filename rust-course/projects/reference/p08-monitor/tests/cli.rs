use std::process::Command;

#[test]
fn demo_runs_and_invalid_or_excess_endpoints_are_rejected() {
    let demo = Command::new(env!("CARGO_BIN_EXE_p08-monitor"))
        .output()
        .unwrap();
    assert!(demo.status.success(), "{demo:?}");
    assert!(String::from_utf8_lossy(&demo.stdout).contains("Healthy(200)"));
    for args in [
        vec!["not-an-endpoint"],
        vec!["192.0.2.1:80"],
        vec!["127.0.0.1:1"; 129],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_p08-monitor"))
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
fn non_utf8_endpoint_is_an_input_error() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let output = Command::new(env!("CARGO_BIN_EXE_p08-monitor"))
        .arg(OsString::from_vec(vec![0xff]))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("UTF-8"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

#[test]
fn invalid_request_is_validated_before_any_network_request() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_p08-monitor"))
        .arg(listener.local_addr().unwrap().to_string())
        .arg("invalid")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn broken_output_and_diagnostic_pipes_do_not_panic() {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_p08-monitor"))
        .stdout(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_p08-monitor"))
        .arg("invalid")
        .stderr(writer)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}
