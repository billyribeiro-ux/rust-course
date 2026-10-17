#[test]
fn checked_in_examples() {
    for row in include_str!("../fixtures/cases.tsv").lines() {
        let (source, expected) = row.split_once('\t').unwrap();
        assert_eq!(
            expression_language::evaluate(source).unwrap(),
            expected.parse::<i64>().unwrap(),
            "{source}"
        );
    }
}
