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
| tinySA Ultra/Ultra+ below 350 MHz | Experimental, not declared supported | `mode low input` and `zero 128` are plausible, but the client neither detects nor tests the model. |
| tinySA Ultra/Ultra+ at or above 350 MHz | Not supported | The client sends Basic-only `mode high input`; Ultra has no such high input mode. |
| Ultra mode above the normal range | Not supported | UI stops at 960 MHz and never negotiates Ultra state/capabilities. |
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

The program does not currently expose meaningful command-line options; it
starts the interactive terminal immediately. Run it from the directory where
you want `config.toml`, logs, and scan CSV files to appear.

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
2. common candidates (`/dev/ttyACM0`, `COM3`, `COM5`, `/dev/ttyS16`);
3. the first enumerated port.

Baud choices range from 19,200 to 3,000,000; default is 115,200 unless a saved
value matches the built-in list.

### Scan screen

Fields and bounds are:

| Field | Unit | Default | Step | Bounds |
| --- | --- | ---: | ---: | ---: |
| Start frequency | MHz | 240 | 10 | 0–960 |
| Stop frequency | MHz | 960 | 10 | 10–960 |
| Points | samples | 10,001 | 100 | 101–1,000,001 |
| Repeat | measurements/point | 1 | 1 | 1–1,000 |

Left/right or `h`/`l` changes the selected field. Up/down or `k`/`j` changes
the selection/value. Enter advances to the next screen. `q` or Escape exits.

Large point and repeat values can produce extremely long scans. Total time also
depends on RBW and instrument mode; the progress bar is based mainly on bytes
received, not a predictive time estimate.

### Progress and result

The scan runs on a worker thread while the UI displays shared progress and the
concatenated `version`/`info` response. On success, the final screen plots a
scatter trace. Its Y axis is fixed to −128–0 dBm, so valid results outside that
range are clipped visually even though CSV data may contain them.

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

Frequencies in this file are MHz, unlike protocol frequencies (Hz). The file
is overwritten by the next run. A missing or invalid configuration falls back
to defaults; validate unexpected values before assuming they were loaded.

### CSV

The scanner writes `scan_YYYYMMDD HH_MM_SS.csv` in the working directory. It
has no header:

```csv
240,-92.4375
240.072,-91.96875
```

Column one is MHz and column two is dBm. The file is created before full frame
validation, so an error can leave a partial or empty CSV. Also note the
[known frequency-label mismatch](usb-protocol.md#frequency-assignment).

### Logs

`flexi_logger` writes diagnostic logs to a file in the working directory. Logs
can contain instrument version/configuration details and port names. Attach the
relevant bounded excerpt—not an uncontrolled archive—when reporting a bug.

## Instrument state changed by a run

tiny-wb changes repeat count, raw zero offset, input mode, display sweep start
and stop, and pause/resume state. It also runs an auxiliary textual scan before
the raw scan. It does not snapshot and restore previous values. If preserving a
carefully tuned front-panel configuration matters, save it as an instrument
preset and reload it after tiny-wb exits.

If acquisition fails after `pause`, error handling does not guarantee a final
`resume`; the front panel may remain paused. Send `resume`, use the UI, or
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
  slow command or fragmented prompt even when the instrument is healthy.

### Parse beginning/end/alignment error

- Reduce point count and repeat to minimize transfer time.
- Disable any other process reading the port.
- Ensure the firmware implements the documented `scanraw` framing.
- Preserve a raw byte capture if possible. Do not convert it through UTF-8.
- Be aware that the current reader can mistake binary bytes equal to `ch>` for
  the shell prompt or miss a prompt split between OS reads.

### Trace is shifted in frequency

The current host labels `points` samples over `points - 1` intervals, while the
reviewed firmware measures using a `span / points` step. Verify with a known
carrier, record firmware version, and use the firmware formula when correcting
the exported axis offline.

### Levels are wrong by about 46 dB

That is the difference between Ultra's documented default raw offset (174) and
Basic's (128). tiny-wb sends `zero 128`; a 46 dB shift suggests the firmware
did not accept/apply that command or the reply was not validated. Stop treating
the result as calibrated data and capture `version`, `info`, and the `zero`
command response.

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

## Current engineering backlog implied by the documentation

These are documentation-derived gaps, not changes made in this task:

1. Parse `version`/`info` into a model and capability structure.
2. Add Ultra/Ultra+ ranges and correct mode syntax, or explicitly reject them.
3. Replace prompt searches with a persistent mixed text/binary framing state
   machine.
4. Resolve the `scanraw` frequency denominator against real firmware fixtures.
5. Remove or justify the preliminary `scan ... 290 101` request.
6. Validate exact record count and closing delimiter before accepting/writing a
   completed CSV.
7. Restore changed instrument state on success, cancellation, and error.
8. Put model/firmware/settings and column names in output metadata.
9. Add protocol tests based on captured Basic and Ultra transcripts.
