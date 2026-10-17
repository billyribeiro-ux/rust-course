#![no_main]
use expression_language::{MAX_DEPTH, Span, parse};
use libfuzzer_sys::fuzz_target;

fn valid_span(source: &str, span: Span) {
    assert!(span.start <= span.end && span.end <= source.len());
    assert!(source.is_char_boundary(span.start));
    assert!(source.is_char_boundary(span.end));
    // Rendering this range is safe when the diagnostic contract is satisfied.
    let _highlight = &source[span.start..span.end];
}
fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    match parse(source) {
        Ok(tree) => {
            valid_span(source, tree.span());
            assert!((1..=MAX_DEPTH).contains(&tree.depth()));
            let first = tree.evaluate();
            assert_eq!(first, tree.evaluate());
            if let Err(error) = first {
                valid_span(source, error.span);
            }
        }
        Err(error) => valid_span(source, error.span),
    }
});
