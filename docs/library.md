# Rust library integration

The package builds both `tiny_wb` (Rust library) and `tiny-wb` (terminal app).
Rust 1.91 or newer is required. To integrate a checkout without terminal, serial,
libudev or Windows resource dependencies:

```toml
[dependencies]
tiny-wb = { path = "../tiny-wb", default-features = false }
```

| Cargo feature | Purpose |
| --- | --- |
| no features | Configuration, protocol, shell, Basic acquisition, CSV, TCP adapter |
| `serial` | Native USB/serial adapter |
| `tui` | Terminal dependencies and application build resources |
| `mock` | In-memory simulator and stream server; no additional dependencies |
| default | `tui` + `serial`, preserving the existing application |

The application requires both default features. Build only the library with
`cargo build --locked --lib --no-default-features`. This is a Rust API, not a
stable C ABI, shared-library binding or published crates.io compatibility promise.
Mock support is opt-in; the server is a Cargo example, preserving the existing
single-binary release artifact contract.

`Shell<T>` accepts any synchronous `Read + Write` stream. Use
`connection::open_connection(&config)` for configured serial/TCP connections,
or inject your own adapter. Transport deadlines belong to the adapter; an
arbitrary blocking `Read` implementation cannot be interrupted by the shell.
`acquisition::acquire` accepts a validated Basic `ScanRequest`, scan/timing
settings and a progress callback, and returns samples without opening files.
`export::write_csv` writes those samples to any writer. `export::read_csv`
loads the same two-column, optionally headed format from any `BufRead` source;
it validates finite values and nondecreasing MHz frequencies without depending
on the terminal renderer.

```rust
use tiny_wb::{config::Config, connection::open_connection,
    acquisition::acquire, protocol::ScanRequest, transport::Shell};

fn scan(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    config.validate()?;
    let mut shell = Shell::new(open_connection(config)?, 65_536);
    let request = ScanRequest {
        start_hz: 240_000_000, stop_hz: 960_000_000, points: 101, repeat: 1,
    };
    let samples = acquire(&mut shell, &request, &config.scan, &config.serial, |_| {})?;
    tiny_wb::export::write_csv(&mut std::io::stdout(), &samples, true)?;
    Ok(())
}
```

Frequencies in requests are Hz; samples expose MHz and dBm. Keep the decoding
zero offset and frequency-axis convention explicit. Acquisition attempts
`resume` after every attempted pause and preserves both errors if recovery
fails. It does not restore all device settings. Known `error:`, `usage:` and
`unknown command` text replies reject configuration commands; this is not
universal firmware acknowledgement or automatic capability detection.

See [the mock guide](mock-device.md) for deterministic integration tests and
[the roadmap](roadmap.md) for the model-aware API still needed for Ultra.
Generate API documentation with `cargo doc --no-default-features --features mock --no-deps`.
