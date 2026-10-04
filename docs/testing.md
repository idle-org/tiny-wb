# Testing without a tinySA

The library exposes configuration, protocol, transport, acquisition and CSV
boundaries independently of the terminal. Tests use small synthetic byte
transcripts rather than opening a serial device. They are regression checks,
not proof of behavior on a particular model or firmware.

```console
cargo test --locked --all-targets --all-features
cargo test --locked --doc
```

| Boundary | Regression cases |
| --- | --- |
| Configuration | Embedded defaults, legacy flat keys, partial nested overrides, unknown/malformed values, numeric and path bounds, round trips, and failed replacement preservation. |
| Protocol | Basic mode boundaries, point/repeat validation, truncated/extra records, alignment, closing brace, binary delimiter bytes, little-endian levels, offset and both frequency-axis conventions. |
| Transport | One-byte/fragmented reads, multiple responses in one read, retained bytes after prompts, partial writes, invalid command lines, EOF, timeout, response size limits and raw-body framing. |
| Acquisition | Exact command ordering, optional legacy pre-scan, progress completion, successful decoding, resume after malformed frames, and preservation of acquisition plus recovery errors. |
| Export | Header/units, deterministic CSV values, propagated write failures and collision-safe output files. |

`Shell<T>` accepts `T: Read + Write`. To add a regression, provide a transcript
reader and capture its writes; vary read chunk size so a token may cross any
read boundary. Test both expected output and error behavior. Keep command side
effects outside logging macros because disabled log levels do not evaluate
format arguments.

`decode_frame` receives exactly the framed body (`{`, records, `}`), without
shell echo or prompt. Use raw bytes in tests: converting binary through UTF-8
can destroy the failure being reproduced. Check every truncated prefix when a
parser changes, and include payload bytes that equal framing characters.

`write_csv` accepts a writer; use an in-memory vector or a writer that fails
partway through. Filesystem tests use separate temporary directories and clean
up after themselves. They must never write shared public assets.

Run the coverage alias and badge tests as described in
[security and CI](security-and-ci.md). Keep the minimum tied to an actual
measurement over production code. Report test counts and hardware limitations
alongside percentages; a high line percentage does not test RF correctness.

Hardware validation still requires a known model/version, safe RF input, and a
bounded capture of the real command/response sequence. In particular, verify
frequency-axis policy, the legacy preparatory scan, zero-offset acceptance and
resume behavior against the connected firmware before changing their defaults.
