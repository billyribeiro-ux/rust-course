use log_detective::{Limits, analyze, search};
use std::io::{self, BufReader, Cursor, Read, Write};

#[test]
fn unicode_literal_matching_is_independent_of_chunk_boundaries() {
    let input = "1 INFO api café 🦀\r\n2 WARN db cafe\u{301}\n3 ERROR api CAFÉ\n4 INFO api café";
    for capacity in 1..=32 {
        let mut output = Vec::new();
        let result = search(
            BufReader::with_capacity(capacity, Cursor::new(input)),
            Limits::default(),
            "café",
            &mut output,
        )
        .unwrap();
        assert_eq!(result.matched, 2);
        assert_eq!(result.summary.valid, 4);
        assert_eq!(output, "1 INFO api café 🦀\n4 INFO api café\n".as_bytes());
    }
    let mut output = Vec::new();
    let result = search(input.as_bytes(), Limits::default(), "🦀", &mut output).unwrap();
    assert_eq!(result.matched, 1);
    let result = search(
        input.as_bytes(),
        Limits::default(),
        "absent",
        &mut io::sink(),
    )
    .unwrap();
    assert_eq!(result.matched, 0);
    assert_eq!(
        result.summary,
        analyze(input.as_bytes(), Limits::default()).unwrap()
    );
}

#[test]
fn rejected_records_are_counted_but_never_emitted() {
    let input =
        b"bad needle\n1 INFO x needle\n\xff needle\n12345678901234567890 needle\n2 INFO x needle";
    let mut output = Vec::new();
    let result = search(
        BufReader::with_capacity(2, Cursor::new(input)),
        Limits {
            max_line_bytes: 16,
            max_total_bytes: input.len() as u64,
        },
        "needle",
        &mut output,
    )
    .unwrap();
    assert_eq!(
        (
            result.summary.lines,
            result.summary.valid,
            result.summary.malformed,
            result.summary.oversized,
            result.matched
        ),
        (5, 2, 2, 1, 2)
    );
    assert_eq!(output, b"1 INFO x needle\n2 INFO x needle\n");
}

#[test]
fn exact_budget_counts_delimiters_and_rejected_bytes() {
    let input = b"bad\n1 INFO x ok\r\n";
    let exact = Limits {
        max_line_bytes: 4096,
        max_total_bytes: input.len() as u64,
    };
    assert_eq!(
        search(&input[..], exact, "ok", &mut io::sink())
            .unwrap()
            .matched,
        1
    );
    let mut output = Vec::new();
    let error = search(
        &input[..],
        Limits {
            max_total_bytes: exact.max_total_bytes - 1,
            ..exact
        },
        "ok",
        &mut output,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(output.is_empty());
    for query in ["", "\n", "\u{1b}"] {
        assert_eq!(
            search(&input[..], exact, query, &mut io::sink())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert!(search(&input[..], exact, &"x".repeat(4097), &mut io::sink()).is_err());
}

#[test]
fn output_failure_returns_error_instead_of_successful_report() {
    struct FailsAfter(usize);
    impl Write for FailsAfter {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            if self.0 == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "injected output failure",
                ));
            }
            let n = input.len().min(self.0);
            self.0 -= n;
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    // Partial writes are legal: write_all must continue and observe the failure.
    let error = search(
        &b"1 INFO x ready\n"[..],
        Limits::default(),
        "ready",
        &mut FailsAfter(5),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn read_error_after_a_match_preserves_error_and_documents_prefix_output() {
    struct Fails;
    impl Read for Fails {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected read failure"))
        }
    }
    let reader = Cursor::new(b"1 INFO x ready\n").chain(Fails);
    let mut output = Vec::new();
    let error = search(
        BufReader::with_capacity(3, reader),
        Limits::default(),
        "ready",
        &mut output,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert_eq!(output, b"1 INFO x ready\n");
}

#[test]
fn interrupted_read_is_retried() {
    struct InterruptedOnce {
        first: bool,
        source: Cursor<&'static [u8]>,
    }
    impl Read for InterruptedOnce {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if self.first {
                self.first = false;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.source.read(output)
        }
    }
    let reader = InterruptedOnce {
        first: true,
        source: Cursor::new(b"1 INFO x ready\n"),
    };
    assert_eq!(
        search(
            BufReader::with_capacity(2, reader),
            Limits::default(),
            "ready",
            &mut io::sink()
        )
        .unwrap()
        .matched,
        1
    );
}

#[test]
fn generated_stream_exceeds_default_budget_without_whole_input_storage() {
    // Only this 1KiB record exists in memory; the reader repeats it on demand.
    // 65,537 records produce 67,109,888 bytes, beyond the 64MiB default.
    // This is behavioral/structural evidence, not an OS RSS measurement.
    struct Repeated<'a> {
        record: &'a [u8],
        bytes_left: u64,
        offset: usize,
        max_request: usize,
    }
    impl Read for Repeated<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            self.max_request = self.max_request.max(output.len());
            let n = output
                .len()
                .min(self.record.len() - self.offset)
                .min(self.bytes_left as usize);
            output[..n].copy_from_slice(&self.record[self.offset..self.offset + n]);
            self.offset = (self.offset + n) % self.record.len();
            self.bytes_left -= n as u64;
            Ok(n)
        }
    }
    let mut record = [b'x'; 1024];
    record[..9].copy_from_slice(b"1 INFO x ");
    record[1023] = b'\n';
    let total = 65_537 * record.len() as u64;
    assert!(total > Limits::default().max_total_bytes);
    let mut source = Repeated {
        record: &record,
        bytes_left: total,
        offset: 0,
        max_request: 0,
    };
    let result = search(
        BufReader::with_capacity(257, &mut source),
        Limits {
            max_line_bytes: 1023,
            max_total_bytes: total,
        },
        "INFO",
        &mut io::sink(),
    )
    .unwrap();
    assert_eq!(
        (result.summary.lines, result.summary.valid, result.matched),
        (65_537, 65_537, 65_537)
    );
    assert_eq!(source.bytes_left, 0);
    assert!(source.max_request <= 257);
}
