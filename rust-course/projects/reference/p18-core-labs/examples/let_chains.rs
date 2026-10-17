//! Edition 2024 let chains: a binding is available only after its pattern matches.
use core_rust_labs::conversions::Percent;

fn positive_percentage(text: Option<&str>) -> Option<Percent> {
    if let Some(text) = text
        && let Ok(value) = text.parse::<Percent>()
        && u16::from(value) > 0
    {
        Some(value)
    } else {
        None
    }
}

fn main() {
    // This example deliberately treats absence, malformed input, and zero alike.
    // A user-facing validator should retain a Result when those causes matter.
    assert_eq!(positive_percentage(Some("25")).map(u16::from), Some(25));
    for text in [None, Some("0"), Some("101"), Some("bad")] {
        assert_eq!(positive_percentage(text), None);
    }
}
