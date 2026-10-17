use log_detective::{Limits, analyze};
#[test]
fn fixture_totals() {
    let report = analyze(
        &include_bytes!("../fixtures/sample.log")[..],
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        (report.lines, report.valid, report.malformed, report.errors),
        (5, 4, 1, 1)
    );
    let report = analyze(
        &include_bytes!("../fixtures/invalid-utf8.log")[..],
        Limits::default(),
    )
    .unwrap();
    assert_eq!((report.valid, report.malformed), (1, 1));
}
