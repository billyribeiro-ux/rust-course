use std::process::Command;
#[test]
fn cli_override_beats_environment_and_redacts() {
    let result = Command::new(env!("CARGO_BIN_EXE_typed-config"))
        .env_clear()
        .env("APP_PORT", "9001")
        .env("APP_TOKEN", "test-only-secret-marker")
        .args(["--set", "port=9002"])
        .output()
        .unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("9002"));
    assert!(!output.contains("test-only-secret-marker"));
    assert!(output.contains("REDACTED"));
}
