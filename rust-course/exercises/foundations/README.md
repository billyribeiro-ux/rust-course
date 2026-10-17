# Deliberately failing compiler exercises

These files are not reference build failures. From `rust-course/exercises/foundations`, create `/tmp/rust-course-diagnostics`, then run the command for one file in `expected-errors.json`. Rust 1.99 should exit nonzero and include the listed primary error code. Compiler wording can change. Write the violated ownership/type relationship in your own words; save a corrected copy in your own scratch directory. Do not “repair” the original fixture or the expected-error runner loses its purpose.

Record the version, diagnostic, hypothesis, smallest experiment, fix, and remaining uncertainty using the course investigation template. A successful compile establishes type correctness of that program, not persistence correctness, arithmetic correctness, or complete requirements coverage.
