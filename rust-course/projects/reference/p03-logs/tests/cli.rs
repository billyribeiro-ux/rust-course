use std::process::{Command, Output};
fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_log-detective"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(arguments)
        .output()
        .unwrap()
}
#[test]
fn existing_summary_and_streaming_search_have_separate_output_contracts() {
    let output = run(&["fixtures/sample.log"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "lines=5 valid=4 malformed=1 oversized=0 info=2 warnings=1 errors=1\n"
    );
    let output = run(&[
        "--contains",
        "api",
        "--max-bytes",
        "1000000000",
        "fixtures/sample.log",
    ]);
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"100 INFO api ready\n102 ERROR api upstream unavailable\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "lines=5 valid=4 malformed=1 oversized=0 info=2 warnings=1 errors=1 matched=2\n"
    );
}
#[test]
fn invalid_arguments_and_budget_exhaustion_exit_nonzero_without_success_summary() {
    for args in [
        vec!["--max-bytes", "0", "fixtures/sample.log"],
        vec!["--contains", "", "fixtures/sample.log"],
        vec!["--max-bytes", "5", "fixtures/sample.log"],
        vec!["missing-fixture.log"],
    ] {
        let output = run(&args);
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .starts_with("logs: ")
        );
    }
    let output = run(&["--help"]);
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("usage:")
    );
}
