# Mock tinySA device

The optional `mock` feature provides a deterministic in-memory duplex stream
and a TCP server backed by the same device model. Standard `Read + Write`
keeps this portable without pseudo-terminal pairs, virtual COM drivers or
privileged serial setup. TCP is a mock/bridge transport, not a native tinySA
network feature. Simulated success is not hardware validation.

## Run the server and application

From the checkout, start the server:

```console
cargo run --locked --no-default-features --features mock --example mock-tinysa-server -- --config config/mock-basic.toml
```

In a second terminal, copy the client template to a writable settings file
outside the checkout, then point `TINY_WB_CONFIG` at that copy:

```sh
cp config/mock-client.toml /tmp/tiny-wb-mock-client.toml
TINY_WB_CONFIG=/tmp/tiny-wb-mock-client.toml cargo run --locked --bin tiny-wb
```

On Windows, copy the template to a chosen temporary file and set
`$env:TINY_WB_CONFIG` in PowerShell before running Cargo. The app saves settings
to this path; do not use the tracked template as the runtime file. The connection
screen offers `tcp://127.0.0.1:7355` without enumerating USB devices. Serial port
and baud selections have no effect on a TCP connection. Successful scans still
write CSV to the configured export directory.

Options: `--bind IP:port` (default `127.0.0.1:7355`), `--config FILE`, `--once`,
`--help`. Port zero asks the OS for a free port; the actual endpoint is printed.
Each accepted connection gets fresh state. The example serves one client at a
time; disconnected/error sessions allow the next client. `--once` exits after
one session, returning failure for session I/O errors. The library's
`serve_connection` lets integrations supply their own listener and concurrency.
Keep this unauthenticated test service on loopback.

## In-memory integration

Enable `mock` alongside `default-features = false` in the dependency entry:

```rust
use tiny_wb::{mock::{MockConfig, MockStream}, transport::Shell};
use std::time::Duration;
let mut device = MockStream::new(MockConfig::default()).unwrap();
{
    let mut shell = Shell::new(&mut device, 65_536);
    shell.synchronize().unwrap();
    shell.command_checked("pause", Duration::ZERO, Duration::ZERO).unwrap();
}
assert!(device.state().paused);
```

`tests/mock_device.rs` runs production acquisition with one-byte reads/writes,
both echo modes, legacy preparation enabled/disabled, recovery faults, and a
real loopback TCP connection. Run without terminal or native serial dependencies:

```console
cargo test --locked --no-default-features --features mock --lib --test mock_device
```

## Supported synthetic behavior

| Commands | Behavior |
| --- | --- |
| `version`, `info`, `help` | Explicitly synthetic identity and implemented subset |
| `pause`, `resume` | Observable sweep state |
| `mode low/high input` | Basic input paths; Basic low stop below 350 MHz, high start at least 240 MHz |
| `mode input`, `mode low input` | Ultra mock input syntax; high mode rejected |
| `repeat N`, `zero N` | Stored count and fixed-point offset |
| `sweep start/stop Hz`, `sweep` | Stored bounds and query |
| `scanraw start stop points` | Bounded deterministic binary spectrum |
| `scan start stop 290 101` | Current app's compatibility preparation only; no trace data |
| `leveloffset`, `vbat`, `usart_cfg`, `threads`, `marker` | Fixed diagnostic fixtures, not measurement implementations |
| `lna on/off`, `ultra on/off` | Ultra-only state toggles with no RF effect |

Other commands or invalid arguments return an explicit error. No calibration,
RF output, display capture, arbitrary firmware behavior or complete console
emulation is claimed. The Ultra profile exercises automatic detection, native
offset readback and low-input acquisition. Production Ultra scans are limited
to its detected model/revision policy; the mock's larger synthetic range is not
a production capability. ZS406 limits are covered by identity-policy transcript
tests rather than inferred from this generic Ultra mock.
See [support scope](ultra-support.md).

Samples use the reviewed firmware's integer-divided, single-precision
`scanraw` point frequencies, a triangular peak above a configured floor, and
`round((dBm + zero_offset) * 32)` clamped to `u16`.
Frames contain `{`, repeated `x` plus little-endian `u16`, then `}` and a prompt.
Use the firmware frequency-axis setting for matching labels. There is no noise,
RF physics, timing model or amplitude change from LNA/repeat/Ultra toggles.
The mock Ultra ceiling is adjustable synthetic policy, not a model specification.

## Configuration and failures

[Basic](../config/mock-basic.toml) and [Ultra](../config/mock-ultra.toml) profiles
are partial TOML overrides of `MockConfig::default()`, separate from application
settings. Unknown keys and invalid bounds fail before accepting a session.
Public Rustdoc lists every field and unit. `session_timeout_ms` defaults to
30,000 and bounds server socket reads/writes; client timeouts are independent.
`read_chunk_bytes` and `write_chunk_bytes` exercise partial I/O. TCP may coalesce
chunks. Commands, frame allocations, pending responses and command history are
bounded; integrations must read responses instead of pipelining indefinitely.

`fault` is one of `none`, `reject`, `timeout`, `disconnect`, `missing_prompt`,
`truncated_frame`, `bad_alignment`. It fires once per session on the first
`fault_command` (default `scanraw`). Frame corruption requires `scanraw`.
Rejection/timeout/disconnect do not execute the command. Missing prompt and
frame corruption occur after execution. Timeout drops the response: an empty
in-memory read immediately returns `TimedOut`; a TCP client waits for its own
socket deadline. Disconnect yields EOF, then `BrokenPipe` on later writes.

Example fault override:

```toml
fault = "truncated_frame"
fault_command = "scanraw"
read_chunk_bytes = 1
write_chunk_bytes = 1
```

Faults verify error handling and resume attempts, not reliable recovery from
all possible real firmware stream states. Hardware transcripts and per-firmware
acceptance tests remain [roadmap work](roadmap.md).
