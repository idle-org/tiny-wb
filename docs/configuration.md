# Application configuration

`config/default.toml` is the authoritative, commented set of defaults embedded
in the binary at compile time. To customize them, copy it to `config.toml` in
the working directory, or supply a partial file containing only overrides.
Rebuild after changing the embedded file; it is not read at runtime.

At startup the application reads `config.toml`, or the path named by the
`TINY_WB_CONFIG` environment variable. For example:

```console
TINY_WB_CONFIG=/path/to/session.toml cargo run --release
```

Settings and logging are validated before entering the terminal interface.
Saving a scan selection writes to this same path; unrelated configuration
sections are retained. An invalid file is reported rather than silently reset.

The configuration API is in `src/config.rs`. It recursively merges supplied
TOML tables with the embedded defaults, deserializes into typed settings, and
validates before use. Arrays replace their defaults. Unknown keys, invalid
numbers, malformed files and I/O failures are errors; only an absent file uses
defaults. The original six top-level keys remain compatible:

```toml
port = "/dev/ttyACM0"
baudrate = 115200
start_frequency = 240
stop_frequency = 960
points = 10001
repeat = 1
```

Start/stop frequencies are **MHz**. The Basic envelope remains 0–960 MHz,
with stop at least 10 MHz; the protocol separately rejects spans that cannot
use one input path. Values must be ordered. The selected baud rate must be in
`serial.baudrates`; points and repeat must fit the configured bounds.

| Section | Settings and units |
| --- | --- |
| `serial` | Read timeout and command/settling/pre-scan delays in milliseconds; bounded text response size in bytes; baud-rate choices and preferred ports. |
| `scan` | Raw zero offset in dB, frequency/sample keyboard steps, sample/repeat limits, frequency-axis convention, and legacy pre-scan toggle. |
| `ui` | Event polling interval in milliseconds and finite increasing chart bounds in dBm. |
| `output` | CSV directory, filename prefix, and optional column header. |
| `logging` | Logger filter expression and destination directory. |

For example, this partial file changes the serial timeout and output format
while retaining the default scan selection:

```toml
[serial]
timeout_ms = 5000

[output]
directory = "scans"
filename_prefix = "measurement"
include_header = true
```

All relative paths are relative to the process working directory. Saving a
selection preserves every recognized section, using a same-directory temporary
file, flush, and rename. The old configuration survives validation or write
failure. TOML formatting and comments are not preserved; retain comments in a
separate template if needed. The configuration file's parent must exist.

`scan.frequency_axis = "inclusive"` preserves the historical CSV frequency
labels (`span / (points - 1)`). `"firmware"` uses `span / points`, matching the
reviewed upstream implementation. Neither changes the request or proves which
firmware is connected. See [frequency assignment](usb-protocol.md#frequency-assignment).

The preparatory `scan START STOP 290 101` remains enabled by default with
`scan.legacy_pre_scan = true` for compatibility. Its undocumented output mask
is not a tunable protocol contract. Disable the step only after checking the
target instrument; hardware validation remains necessary.

Protocol delimiters, fixed-point scaling, command grammar and Basic input-path
limits remain code invariants. Configuration cannot turn a Basic-only client
into an Ultra-compatible one by increasing a numeric limit.

## Connection backends

`[connection] backend = "serial"` preserves USB behavior. Select `"tcp"` with
`tcp_address = "127.0.0.1:7355"` and `tcp_timeout_ms = 3000` for an explicit
mock/bridge. The address must be numeric IPv4/IPv6 with a nonzero port; timeout
is 1–600,000 ms and applies to connect/read/write operations. All backend fields
are validated, including inactive ones. TCP bypasses USB enumeration and ignores
serial port/baud selections; protocol pacing still uses `[serial]` timing fields.
See the [mock client template](../config/mock-client.toml) and [mock guide](mock-device.md).
