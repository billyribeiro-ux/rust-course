use std::process::Command;
#[test]
fn cli_separates_success_domain_and_usage() {
    let exe = env!("CARGO_BIN_EXE_workshop-calculator");
    let success = Command::new(exe).args(["12m", "+", "3m"]).output().unwrap();
    assert!(success.status.success());
    assert_eq!(success.stdout, b"15m\n");
    assert!(success.stderr.is_empty());
    let domain = Command::new(exe).args(["1", "/", "0"]).output().unwrap();
    assert_eq!(domain.status.code(), Some(1));
    assert!(domain.stdout.is_empty());
    let usage = Command::new(exe).output().unwrap();
    assert_eq!(usage.status.code(), Some(2));
}

#[test]
fn conversion_cli_accepts_fractional_signed_and_scientific_values() {
    let exe = env!("CARGO_BIN_EXE_workshop-calculator");
    for (input, target, expected) in [
        ("1.5m", "cm", "approximately 150cm\n"),
        ("-2.5km", "m", "approximately -2500m\n"),
        ("1.25e3mm", "m", "approximately 1.25m\n"),
    ] {
        let output = Command::new(exe)
            .args(["convert", input, target])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn conversion_cli_reports_validation_and_range_errors_without_success_output() {
    let exe = env!("CARGO_BIN_EXE_workshop-calculator");
    for (input, target, diagnostic) in [
        ("NaNm", "m", "finite"),
        ("infm", "cm", "finite"),
        ("1.5s", "cm", "unit"),
        ("1.5m", "in", "unit"),
        ("badm", "cm", "number"),
        ("1e308km", "mm", "exceeds"),
        ("5e-324mm", "km", "rounded to zero"),
        ("1e-999m", "cm", "rounded to zero"),
    ] {
        let output = Command::new(exe)
            .args(["convert", input, target])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{input} -> {target}");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains(diagnostic), "{stderr}");
        assert!(!stderr.contains("panicked"));
    }
    let usage = Command::new(exe)
        .args(["convert", "1.5m"])
        .output()
        .unwrap();
    assert_eq!(usage.status.code(), Some(2));
    assert!(usage.stdout.is_empty());
}

#[test]
fn adding_conversion_does_not_silently_change_integer_arithmetic() {
    let exe = env!("CARGO_BIN_EXE_workshop-calculator");
    let truncated = Command::new(exe).args(["7m", "/", "2"]).output().unwrap();
    assert_eq!(truncated.status.code(), Some(0));
    assert_eq!(truncated.stdout, b"3m\n");
    let fractional = Command::new(exe)
        .args(["1.5m", "+", "2m"])
        .output()
        .unwrap();
    assert_eq!(fractional.status.code(), Some(1));
    assert!(fractional.stdout.is_empty());
}

#[cfg(unix)]
#[test]
fn non_unicode_operand_is_a_usage_error() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let output = Command::new(env!("CARGO_BIN_EXE_workshop-calculator"))
        .arg(OsString::from_vec(vec![0xff]))
        .args(["+", "1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("UTF-8"));
    assert!(!error.contains("panicked"));
}
