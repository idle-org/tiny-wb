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

Start/stop frequencies are **MHz**. The shared entry envelope is 0–5,400 MHz,
with stop at least 10 MHz; detected model/revision policy applies before the
instrument is mutated. Basic remains limited to 960 MHz, generic Ultra to 800
MHz, and ZS406 normal mode to 900 MHz. Values must be ordered. The selected baud
rate must be in `serial.baudrates`; points and repeat must fit the configured bounds.

| Section | Settings and units |
| --- | --- |
| `serial` | Read timeout and command/settling/pre-scan delays in milliseconds; bounded text response size in bytes; baud-rate choices and preferred ports. |
| `scan` | Raw zero offset in dB, explicit ZS406 Ultra-mode opt-in, frequency/sample keyboard steps, sample/repeat limits, frequency-axis convention, and legacy pre-scan toggle. |
| `ui` | Event polling interval in milliseconds, finite increasing chart bounds in dBm, and plot navigation bindings. |
| `output` | CSV directory, filename prefix, and optional column header. |
| `logging` | Logger filter expression and destination directory. |

`scan.device_model = "auto"` detects the family before changing settings.
`scan.auto_zero_offset = true` selects Basic 128 or Ultra 174 and verifies the
`zero` readback. To retain a manual `scan.zero_offset`, set automatic selection
to false. `scan.enable_ultra_mode = true` explicitly permits a detected ZS406
to receive above 900 MHz through 5.4 GHz; it is false by default and never
enables generator output. See [support and migration](ultra-support.md). The wire record has no
model-specific padding; see [level conversion](usb-protocol.md#level-conversion-and-zero).

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

`scan.frequency_axis = "firmware"` is the default. It reproduces the reviewed
`scanraw` integer division, single-precision multiplication and conversion back
to integer Hz. `"inclusive"` preserves the legacy tiny-wb labels
(`span / (points - 1)`) and includes the requested stop frequency. Neither
setting changes the request or proves RF frequency accuracy. Existing saved
files that explicitly contain `frequency_axis = "inclusive"` keep their old
labels; partial configurations that omit the key adopt the corrected default.
See [frequency assignment](usb-protocol.md#frequency-assignment).

`[ui.bindings]` configures the result/CSV renderer. Each action is a nonempty
array of unique bindings. A binding is one printable character or `left`,
`right`, or `home`; bindings cannot overlap between actions and `q` remains
reserved for exit. The defaults are:

```toml
[ui.bindings]
zoom_in = ["+", "="]
zoom_out = ["-"]
pan_left = ["left", "h"]
pan_right = ["right", "l"]
reset_view = ["0", "home"]
```

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
