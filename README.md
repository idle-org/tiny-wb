# tiny-wb

![tiny-wb version](https://assets.dandaedre.com/coverage/idle-org/tiny-wb/main/version.svg)
![Test status](https://assets.dandaedre.com/coverage/idle-org/tiny-wb/main/test.svg)
![Line coverage](https://assets.dandaedre.com/coverage/idle-org/tiny-wb/main/coverage.svg)

`tiny-wb` is an interactive terminal application for acquiring a frequency
sweep from a tinySA over its USB serial console, plotting the result, and
exporting it as CSV. It is intended for tinySA users who want a repeatable scan
workflow and for Rust applications that need its reusable acquisition library.

## Support status

- tinySA Basic low and high inputs are supported through 960 MHz, subject to
  the instrument's path limits.
- tinySA Ultra-family detection and acquisition are implemented for the shared
  0.1–800 MHz normal range. A detected Ultra+ ZS406 additionally supports its
  0.1–900 MHz normal range and explicit opt-in Ultra mode through 5.4 GHz.
- ZS407 extended mode and harmonic-mode reception remain unsupported. Hardware
  validation proves communications and framing, not RF frequency/level accuracy.

See the [compatibility matrix](docs/tiny-wb-operations.md#compatibility-matrix)
and [Ultra support scope](docs/ultra-support.md) before relying on a particular
model, range, or mode.

## Requirements

- Rust 1.88.0 or newer
- For acquisition, a powered tinySA Basic, Ultra, or Ultra+ and a data-capable
  USB cable
- For acquisition, permission to open the instrument's serial port

Do not connect an unknown RF source directly to a tinySA. Use suitable external
attenuation for transmitters and a DC block where a DC component may be present.
Read the [safe-operation guidance](docs/tinysa-field-guide.md#safe-operation-read-before-connecting-a-signal)
before connecting a signal.

## Build and run

From the repository root:

```console
cargo run --release
```

For Apple Silicon and Intel macOS binaries built from Linux, use the
[release workflow](docs/releases.md): `./release.sh --compile-only` packages
both macOS architectures alongside Linux and Windows. See
[macOS requirements and validation](docs/macos.md).

Use `cargo run --release -- --live` to confirm the usual connection and scan
settings, then display repeated complete sweeps until stopped. Number keys select
average/hold/decay traces, `s` saves the visible buffer, and `q`, Escape, or
Enter stops, resumes the instrument, saves once more, and exits.

On Linux the serial device is commonly `/dev/ttyACM0`; on Windows it appears as
a COM port. If the port cannot be opened, follow the
[serial-permission guidance](docs/tiny-wb-operations.md#serial-permissions).

Use Left/Right or `h`/`l` to select a field, Up/Down or `k`/`j` to change it,
or type digits to edit a selected start/stop frequency in MHz or the point count.
Enter applies a typed value (press it again to advance), Backspace removes a
digit, and Escape cancels an active edit. Otherwise Enter advances and `q` or
Escape quits.
A successful scan displays a trace
and writes a collision-safe `scan_*.csv` file in the current directory. CSV
frequencies are in MHz and levels are in dBm.

Open an existing tiny-wb CSV directly in the interactive renderer without
enumerating or connecting to a device:

```console
cargo run --release -- --view scan.csv
# The shorter `cargo run --release -- scan.csv` is equivalent.
```

On a plot, use `+`/`=` and `-` to zoom, Left/Right or `h`/`l` to pan, and
`0` or Home to restore the full frequency range. These controls also work during
live capture. Press `m` to toggle a box showing the strongest visible sample's
level and frequency. Plot bindings are configurable
under `[ui.bindings]`. After an acquired scan, `r` immediately repeats the same
scan and `n` returns to the prefilled scan-parameter editor.

## Configuration and documentation

The selected settings are saved to `config.toml`, or to the path in
`TINY_WB_CONFIG`. Start with the commented [default configuration](config/default.toml)
and consult the [configuration guide](docs/configuration.md) for validation,
units, output, and logging settings.

- [Compatibility, operation, and troubleshooting](docs/tiny-wb-operations.md)
- [Hardware validation checklist](docs/hardware-validation.md)
- [USB console and `scanraw` protocol](docs/usb-protocol.md)
- [Documentation index](docs/README.md)

## Development

The core protocol, transport, acquisition, and CSV code can be tested without
a terminal or physical instrument:

```console
cargo fmt --all -- --check
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Run the complete local CI sequence in
[Security, dependency policy, and MSRV](docs/security-and-ci.md#local-verification).
See the [architecture guide](docs/architecture.md),
[testing guide](docs/testing.md), and [contributor instructions](AGENTS.md)
before changing behavior.

## Support

Report bugs in the [project issue tracker](https://proxy.dandaedre.com/gitea/idle-org/tiny-wb/issues).
Include the details from the
[reproducible bug-report checklist](docs/tiny-wb-operations.md#reproducible-bug-report-checklist).

## License

tiny-wb is available under the [MIT License](LICENSE).
