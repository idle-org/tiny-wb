# tiny-wb compatibility and operations

This guide describes the application as it exists today. It is intentionally
more conservative than “the firmware probably accepts it”: a range is called
supported only when tiny-wb selects the right path, decodes it knowingly, and
can represent it in the UI.

## Compatibility matrix

| Instrument/use | Status | Reason |
| --- | --- | --- |
| tinySA Basic low input, range wholly below 350 MHz | Best-supported path | The client sends `mode low input`, uses the Basic-oriented range, and normalizes raw samples with `zero 128`. |
| tinySA Basic high input, range wholly from 350 to 960 MHz | Implemented | The client sends `mode high input`; the RF path itself is less accurate and more overload-prone than low input. |
| tinySA Basic crossing from 240–<350 MHz into 350–960 MHz | Implemented as high input | The whole scan is forced onto the high path; the low path is not stitched to it. |
| Basic span starting below 240 MHz and ending at/above 350 MHz | Rejected | One physical path cannot cover it under the client's policy. Split it into scans. |
| tinySA Ultra/Ultra+ 0.1–800 MHz | Implemented; awaiting hardware qualification | Family detection, `mode low input`, automatic `zero 174` and readback verification. |
| Ultra+ ZS406 0.1–900 MHz | Hardware communications validated | Explicit ZS406/V0.4.6 identity permits the full normal-mode range; RF accuracy is not qualified. |
| Ultra+ ZS406 above 900 MHz through 5.4 GHz | Opt-in; hardware communications validated | Requires `scan.enable_ultra_mode = true`, sends `ultra on`, and verifies sweep readback. Expect slower scans, aliases/spurs and RF-port LO leakage. |
| ZS407 extended mode or any harmonic mode | Rejected | Their different ceilings and measurement behavior are not implemented. |
| Generator control, live streaming, capture, calibration, firmware update | Out of scope | No corresponding application workflow exists. |

At the exact 350 MHz boundary, the implementation's matching rules matter:
low accepts frequencies strictly below 350 MHz, while high accepts 350 MHz
through 960 MHz. A scan from 0 to exactly 350 MHz is rejected, but a scan from
240 to 350 MHz is assigned to the high path.

## Required equipment

- A genuine, powered tinySA Basic for the currently supported workflow.
- A data-capable USB cable. Charge-only cables do not enumerate a serial port.
- A host with permission to open the USB serial device.
- For conducted RF measurements: known-good 50-ohm coax/adapters, correctly
  rated fixed attenuation, and a DC block when needed.
- For transmitter work: a suitable dummy load or sampler and power-rated
  attenuation. Read the [safe-operation guide](tinysa-field-guide.md#safe-operation-read-before-connecting-a-signal)
  before connecting a source.

Do initial software/serial testing with no external RF source connected. This
separates communications failures from unsafe or overloaded RF setups.

## Build and launch

Use a current stable Rust toolchain:

```console
cargo build --release
./target/release/tiny-wb
```

With no arguments the program starts the acquisition workflow. To inspect a
previous export without enumerating a serial device or starting acquisition,
open its renderer directly:

```console
./target/release/tiny-wb --view scan.csv
# A positional path is equivalent: ./target/release/tiny-wb scan.csv
```

Run it from the working directory for your settings and output, or set
`TINY_WB_CONFIG` to another configuration path. See [configuration](configuration.md).

### Serial permissions

On Linux, identify the device after plugging it in:

```console
ls -l /dev/ttyACM*
```

The exact permission mechanism is distribution-specific (often a `dialout` or
`uucp` group, or a udev rule). Do not solve it by running the RF application as
root when a scoped device permission is available.

On Windows, use Device Manager to find the COM port. Close serial terminals and
other tinySA applications before launching tiny-wb; only one process can
normally own the port.

## User interface

### Connection screen

The application enumerates ports at startup. Selection priority is:

1. the port saved in `config.toml`, if present;
2. the configured `serial.preferred_ports` (by default `/dev/ttyACM0`, `COM3`, `COM5`, `/dev/ttyS16`);
3. the first enumerated port.

Default baud choices range from 19,200 to 3,000,000, with 115,200 selected.
`serial.baudrates` defines the choices; the selected rate must belong to that
list. No available ports produces an actionable error rather than an index panic.

### Scan screen

The embedded defaults are shown below. Steps and sample/repeat bounds are
configurable; Basic frequency limits remain fixed protocol rules.

| Field | Unit | Default | Step | Bounds |
| --- | --- | ---: | ---: | ---: |
| Start frequency | MHz | 240 | 10 | 0–5,400 (model checked after identity) |
| Stop frequency | MHz | 800 | 10 | 10–5,400 (model checked after identity) |
| Points | samples | 10,001 | 100 | 101–1,000,001 |
| Repeat | measurements/point | 1 | 1 | 1–1,000 |

Left/right or `h`/`l` changes the selected field. Up/down or `k`/`j` changes
the selection/value. On either frequency field or the Points field, typing a
digit opens an inline editor: digits append, Backspace removes a digit, Enter
applies a value within the displayed bounds, and Escape cancels the edit.
Frequencies are entered in MHz; points are entered as a sample count. Applying
a typed value does not advance, so press Enter again to start the scan. Outside
the editor, Enter advances to the next screen and `q` or Escape exits.

Instrument identification sends the read-only `version` and `info` queries. If
the first complete pair is empty or cannot identify a supported family, tiny-wb
repeats that pair once before reporting the identification error. Transport
timeouts and incomplete prompts are not replayed on a potentially unsynchronized
stream.

Large point and repeat values can produce extremely long scans. Total time also
depends on RBW and instrument mode; the progress bar is based mainly on bytes
received, not a predictive time estimate.

### Progress and result

The scan runs on a worker thread while the UI displays shared progress and the
concatenated `version`/`info` response. On success, the final screen plots a
scatter trace. Its Y axis defaults to −128–0 dBm and uses the configured `ui.chart_min_dbm`
and `ui.chart_max_dbm` bounds. Values outside those bounds are clipped visually
even though CSV data may contain them.

The frequency axis is labeled in MHz. `+` or `=` zooms in, `-` zooms out,
Left/Right or `h`/`l` pans by a quarter of the visible span, and `0` or Home
restores the full range. Zoom and pan stay within the frequencies present in
the plot. Maximum zoom follows the finest positive sample spacing in the loaded
data, and frequency labels add decimal places as the visible range narrows.
After an acquired scan, `r` reconnects and repeats the exact request and settings
without reopening the parameter screen; each successful repeat creates a new
collision-safe CSV. `n` returns to the scan screen with the previous parameters
prefilled so they can be edited before starting another scan. These restart keys
are not available when viewing a CSV directly. Plot bindings are configurable
under `[ui.bindings]`; on acquired results, `r` and `n` take precedence over any
matching plot binding. `q`, Escape, or Enter exits the renderer.

## Files

### `config.toml`

The current selection is saved before acquisition starts:

```toml
port = "/dev/ttyACM0"
baudrate = 115200
start_frequency = 240
stop_frequency = 960
points = 10001
repeat = 1
```

Frequencies in this file are MHz, unlike protocol frequencies (Hz). Saving a selection atomically replaces this file while preserving every
recognized settings section. Missing files use embedded defaults; malformed,
unknown or invalid settings are reported before terminal startup. Override the
path with `TINY_WB_CONFIG`. See [configuration](configuration.md) for the schema.

### CSV

By default the scanner writes `scan_YYYYMMDD HH_MM_SS_microseconds_counter.csv`
in the working directory. The directory and filename prefix are configurable;
exclusive creation prevents overwriting earlier scans. The default has no header:

```csv
240,-92.4375
240.072,-91.96875
```

Column one is MHz and column two is dBm. The file is created only after complete frame validation and a successful
resume response. Failed writes are reported and incomplete files are removed
when possible. Enable `output.include_header` for named columns. Choose the
frequency-label convention explicitly after reviewing the
[frequency assignment notes](usb-protocol.md#frequency-assignment).
The direct renderer accepts this header or headerless files, ignores blank
lines, and reports malformed, non-finite, empty, or decreasing-frequency data
with its CSV line number before initializing logging or taking over the terminal.

### Logs

`flexi_logger` uses `logging.filter` and `logging.directory`; defaults are
`info` and the working directory. The filter also applies in release builds. Logs
can contain instrument version/configuration details and port names. Attach the
relevant bounded excerpt—not an uncontrolled archive—when reporting a bug.

## Instrument state changed by a run

tiny-wb changes repeat count, raw zero offset, input mode, display sweep start
and stop, and pause/resume state. By default it also runs the legacy auxiliary textual scan before
the raw scan; `scan.legacy_pre_scan` can disable it after hardware validation. It does not snapshot and restore previous values. If preserving a
carefully tuned front-panel configuration matters, save it as an instrument
preset and reload it after tiny-wb exits.

After attempting `pause`, acquisition always attempts `resume`, including on
frame or I/O errors. This is best-effort recovery, not confirmation that a
disconnected device resumed or that its previous settings were restored. Quitting
during acquisition can still terminate the worker before recovery. Send `resume`, use the UI, or
power-cycle/reload a preset after making the RF setup safe.

## Troubleshooting

### No serial port is listed

- Confirm the device is powered on and the cable supports data.
- Try a direct USB port or known USB 2.0 hub/cable.
- Check OS device enumeration before debugging tiny-wb.
- On Linux, inspect `/dev/ttyACM*` and kernel logs; on Windows, Device Manager.

### Port cannot be opened

- Close other serial terminals and tinySA software.
- Check the selected COM/TTY name and user permissions.
- Unplug/reconnect if the device was just in DFU mode; DFU and CDC serial are
  different USB personalities.

### “No version found” or a timeout

- Verify the selected port is the tinySA, not another CDC device.
- Wake/power the instrument and close auto-refreshing software.
- Manually send `version` in a serial terminal and confirm a `ch>` prompt.
- Record firmware version. The client relies on prompt timing and may fail on a
  slow command when the configured read timeout is too short. Fragmented prompts
are handled by the buffered transport.

### Parse beginning/end/alignment error

- Reduce point count and repeat to minimize transfer time.
- Disable any other process reading the port.
- Ensure the firmware implements the documented `scanraw` framing.
- Preserve a raw byte capture if possible. Do not convert it through UTF-8.
- The decoder now rejects truncated, extra, misaligned and unterminated records.
  Preserve the exact requested point count and firmware version with the capture.

### Trace is shifted in frequency

The default `firmware` labeling reproduces the reviewed `scanraw` integer and
single-precision arithmetic. If a saved configuration selects `inclusive`, it
uses `points - 1` intervals and can shift later labels by nearly one firmware
step. Remove that override or choose `scan.frequency_axis = "firmware"`, then
verify with a known narrow carrier and record the firmware version. The setting
changes CSV/chart labels; it does not change the instrument request or calibrate
the analyzer.

### Levels are wrong by about 46 dB

That is the difference between Ultra's documented default raw offset (174) and
Basic's (128). The application and hardware validator now select the detected
family's native offset automatically and verify its readback. A 46 dB shift
suggests that the configured command/decoder offset does not match the firmware
value. Stop treating the result as calibrated data and capture `version`,
`info`, and the `zero` command response. This bias is numeric encoding, not
model-specific record padding.

### Unexpected harmonics or spurs

Assume overload or an image until disproved. Add attenuation, reduce span,
change RBW, use a filter/preselector, and compare paths where possible. Consult
the [measurement controls](tinysa-field-guide.md#measurement-controls-and-trade-offs).

## Reproducible bug report checklist

Include:

- tiny-wb commit hash shown in the title;
- host OS and serial port type;
- exact tinySA model/rear label;
- `version`, `info`, and relevant `help` output;
- start/stop, points, repeat, and selected baud;
- RF setup, attenuation, DC block, expected maximum level, RBW, LNA, and Ultra
  mode state where relevant;
- the smallest CSV/log excerpt that demonstrates the problem;
- whether the front panel was paused and whether `resume` recovered it.

Never include a dangerous live-transmitter reproduction just to demonstrate a
software bug; substitute a low-level known source or recorded serial fixture.

## Remaining engineering work

ZS407 extended detection, harmonic ranges above 5.4 GHz, overall
transaction deadlines, cancellation/abort, full instrument-state restoration,
and richer output metadata remain open. Buffered framing, strict record-count
validation, reusable core interfaces and hardware-free regression tests are
implemented. Real Basic/Ultra transcript fixtures and hardware checks are still
needed; see [architecture](architecture.md#extension-points-and-remaining-limits).

## Hardware-free operation

Use the [mock server](mock-device.md) with the TCP connection backend to run
the normal acquisition and CSV workflow without USB hardware. The production
app supports the common Ultra normal range; see [support scope](ultra-support.md).
