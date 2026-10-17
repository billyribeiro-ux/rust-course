#![no_main]
// INTENTIONAL TEST-ORACLE BUG. The parser supports UTF-8 diagnostic byte spans.
// A Unicode character may occupy more than one byte. Do not adopt this assertion.
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    if let Err(error) = expression_language::parse(source) {
        if error.message == "unexpected character" {
            assert_eq!(
                error.span.end - error.span.start,
                1,
                "INTENTIONALLY WRONG: each character occupies one byte"
            );
        }
    }
});
