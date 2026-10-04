# tiny-wb

`tiny-wb` is an interactive terminal application that acquires a frequency
sweep from a tinySA over its USB serial console and exports the result as CSV.
It is written in Rust and uses Ratatui for the interface.

The current implementation is primarily shaped around the original **tinySA
Basic** command set and its 0–960 MHz low/high input ranges. The tinySA Ultra
family uses much of the same console protocol, but model detection and the
Ultra frequency ranges are not implemented yet; see the compatibility notes
before connecting an Ultra or Ultra+.

## Documentation

- [Documentation index](docs/README.md) — where to start and source policy
- [Project architecture](docs/architecture.md) — components, state, data flow,
  files, dependencies, and extension points
- [tinySA field guide](docs/tinysa-field-guide.md) — models, RF architecture,
  specifications, safe operation, measurement practice, calibration, and
  firmware
- [USB console and `scanraw` protocol](docs/usb-protocol.md) — framing,
  commands, binary decoding, synchronization, and a worked example
- [tiny-wb compatibility and operations](docs/tiny-wb-operations.md) — current
  behavior, setup, controls, files, troubleshooting, and known limitations
- [Security, dependency policy, and MSRV](docs/security-and-ci.md) — Rust 1.88
  compatibility floor, locked builds, RustSec, licenses, and CI gates
- [Upstream source catalogue](docs/sources.md) — authoritative links and what
  each source is used for

The documentation was checked against the official tinySA wiki and firmware
source on 2026-10-04. Instrument firmware evolves, so `help`, `version`, and
`info` on the connected unit remain the final authority for its command set.

## Build and run

Install Rust 1.88.0 or newer, connect and power on the instrument, then run:

```console
cargo run --release
```

On Linux the USB CDC device is commonly `/dev/ttyACM0`; on Windows it appears
as a COM port. The user running `tiny-wb` must have permission to open it.

The interface uses arrow keys or Vim-style `h`, `j`, `k`, `l`. Left/right
selects a field, up/down changes its value, Enter advances, and `q` or Escape
quits. Scan frequencies shown in the UI are MHz. The selected settings are
stored in `config.toml`; scan results are written in the current directory as
`scan_YYYYMMDD HH_MM_SS.csv`.

## Current scan flow

At a high level, `tiny-wb`:

1. opens the selected serial port;
2. identifies the instrument with `version` and `info`;
3. configures repeat count, raw-value zero offset, input mode, and sweep;
4. requests a binary `scanraw` acquisition;
5. decodes each level sample into dBm and writes `frequency_MHz,dBm`;
6. resumes the instrument sweep and plots the acquired points.

For the exact command sequence and compatibility implications, read
[USB console and `scanraw` protocol](docs/usb-protocol.md) and
[tiny-wb compatibility and operations](docs/tiny-wb-operations.md).

## Safety

Do not connect an unknown RF source directly to a tinySA. Use external
attenuation for transmitters and a DC block where a DC component may be
present. The Ultra family has an absolute RF input limit of +6 dBm at 0 dB
internal attenuation; the Basic limit is +10 dBm. These are damage limits,
not recommended measurement levels. See the [safe-operation section](docs/tinysa-field-guide.md#safe-operation-read-before-connecting-a-signal).

## Development

The Jenkins pipeline checks formatting, compilation, tests, Clippy, and a
release build. Run the equivalent local checks with:

```console
cargo fmt --all -- --check
cargo audit --deny warnings
cargo deny check
cargo +1.88.0 check --locked --all-targets --all-features
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
```

The application is currently a binary crate rather than a reusable library.
Protocol work is concentrated in `src/scan.rs`; UI state and rendering are in
`src/ui.rs`, `src/ui_struct.rs`, and `src/structs.rs`.

## License

See [LICENSE](LICENSE).
