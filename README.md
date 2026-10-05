# tiny-wb

![Version](https://assets.dandaedre.com/coverage/idle-org/tiny-wb/main/version.svg)
![Tests](https://assets.dandaedre.com/coverage/idle-org/tiny-wb/main/test.svg)
![Coverage](https://assets.dandaedre.com/coverage/idle-org/tiny-wb/main/coverage.svg)

`tiny-wb` is an interactive terminal application that acquires a frequency
sweep from a tinySA over its USB serial console and exports the result as CSV.
It is written in Rust and uses Ratatui for the interface.

The current implementation is primarily shaped around the original **tinySA
Basic** command set and its 0–960 MHz low/high input ranges. The tinySA Ultra
family uses much of the same console protocol, but model detection and the
Ultra frequency ranges are not implemented yet; see the compatibility notes
before connecting an Ultra or Ultra+.

## Documentation

- [Implementation tasks and stretch goals](task.md) — detailed Basic/Ultra backlog and optional features to choose
- [Feature completion roadmap](docs/roadmap.md) — prioritized Basic/Ultra tasks and acceptance criteria
- [Library integration](docs/library.md) — core-only builds and optional adapters
- [Mock device](docs/mock-device.md) — portable TCP server and deterministic testing

- [Documentation index](docs/README.md) — where to start and source policy
- [Configuration](docs/configuration.md) — defaults, partial overrides, units, validation, and saved selections
- [Release guide](docs/releases.md) — wrapper, target/artifact policy, token handling, and publication readiness
- [Testing guide](docs/testing.md) — hardware-free regression tests and test boundaries
- [Hardware validation](docs/hardware-validation.md) — guided Basic and Ultra acceptance checks
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

The Basic acquisition path was validated on physical hardware on 2026-10-05
with firmware `tinySA_v1.4-175-g1419a93`. Both the 240–300 MHz low-input scan
and the 350–400 MHz high-input scan returned 101 valid samples, exercised the
fixed-record binary decoder, produced CSV output, and resumed the live sweep.
This is communications and workflow evidence, not frequency or level-accuracy
calibration; see the [hardware validation checklist](docs/hardware-validation.md)
for the tested scope and remaining limitations.

## Build and run

Install Rust 1.88.0 or newer, connect and power on the instrument, then run:

```console
cargo run --release
```

On Linux the USB CDC device is commonly `/dev/ttyACM0`; on Windows it appears
as a COM port. The user running `tiny-wb` must have permission to open it.

### Linux serial permissions

If `tiny-wb` or `scripts/test-hardware.sh` reports `serial connection:
Permission denied`, inspect the device ownership after connecting the tinySA:

```console
ls -l /dev/ttyACM* /dev/ttyUSB* 2>/dev/null
```

Add your user to the group shown for the device. This is usually `dialout` on
Debian, Ubuntu, and Fedora, or `uucp` on Arch Linux. For example:

```console
sudo usermod -aG dialout "$(id -un)"
```

Log out completely and back in for the new group membership to take effect,
then confirm that the port is readable and writable:

```console
id -nG
test -r /dev/ttyACM0 && test -w /dev/ttyACM0 && echo "serial access OK"
```

Close serial terminals and other tinySA software before retrying. Run the
application or hardware validator as your normal user, not with `sudo`; avoid
making the device world-writable with `chmod 666`, because that permission is
overly broad and is normally lost when the device reconnects. See the
[operations guide](docs/tiny-wb-operations.md#serial-permissions) for more
platform setup information.

The interface uses arrow keys or Vim-style `h`, `j`, `k`, `l`. Left/right
selects a field, up/down changes its value, Enter advances, and `q` or Escape
quits. Scan frequencies shown in the UI are MHz. The selected settings are
stored in `config.toml` (or `TINY_WB_CONFIG`); scan results default to the current
directory with collision-safe timestamped names. Timing, scan defaults, chart
bounds, CSV output, and logging are configurable. See the commented
[defaults](config/default.toml) and [configuration guide](docs/configuration.md).

## Current scan flow

At a high level, `tiny-wb`:

1. opens the selected serial port;
2. identifies the instrument with `version` and `info`;
3. configures repeat count, raw-value zero offset, input mode, and sweep;
4. requests a binary `scanraw` acquisition;
5. decodes each level sample into dBm and writes `frequency_MHz,level_dBm`;
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

The Jenkins pipeline checks formatting, compilation, tests, documentation,
coverage, Clippy, and a release build. Run the equivalent local checks with:

```console
cargo fmt --all -- --check
cargo audit --deny warnings
cargo deny check
cargo +1.88.0 check --locked --all-targets --all-features
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo test --locked --doc --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
```

The binary builds on a reusable library for configuration, protocol decoding,
transport, acquisition, and CSV export. UI control flow, state, and rendering
live in `src/ui/`. See [architecture](docs/architecture.md) for the source map
and [AGENTS.md](AGENTS.md) for code, documentation, and commit conventions.

## License

See [LICENSE](LICENSE).
