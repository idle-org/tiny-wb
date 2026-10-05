# USB console and `scanraw` protocol

This document is the implementation reference for talking to tinySA firmware.
It combines the official USB-interface description with the behavior visible
in the upstream C source and in `src/protocol.rs`, `src/transport.rs`, and `src/acquisition.rs`.

## Transport

Connecting a powered tinySA by a data-capable USB cable exposes a virtual
serial port (USB CDC): commonly `/dev/ttyACM*` on Linux and `COM*` on Windows.
Drivers are built into most Linux systems and are normally installed
automatically on Windows.

The on-device shell is a command stream, not a packet protocol:

- commands are ASCII and are normally terminated by carriage return (`\r`);
- terminal input is commonly echoed, so a reply can begin with the command;
- ordinary output is text;
- the ready prompt is `ch>`;
- some commands (`capture`, display bulk data, and `scanraw`) place binary data
  in the same byte stream;
- firmware performs limited parameter validation, so a prompt does not prove
  that the intended operation occurred.

The baud selector in tiny-wb controls how the host opens the serial device.
USB CDC and the optional internal TTL USART are different transports;
`usart_cfg` configures/reports the internal serial interface in firmware. Do
not infer the physical USB transfer speed from that value.

## Shell grammar

Common scalar syntax is:

- frequency: integer/floating value, optionally suffixed `k`, `M`, or `G`;
- level: floating-point dB/dBm value;
- time: seconds, optionally suffixed `m` (milliseconds) or `u`
  (microseconds).

For software, plain integer Hz is the least ambiguous frequency representation.
Commands and availability vary by build; issue `help`, then record `version`
and `info` before relying on optional commands.

## Commands relevant to tiny-wb

| Command | Meaning and notes |
| --- | --- |
| `version` | Firmware version/build text. |
| `info` | Model/platform information when enabled in the build. |
| `help` | Commands compiled into this firmware. Prefer this over a hard-coded global list. |
| `mode low input` | Select Basic low input; on Ultra, firmware also accepts `mode input` and has no Basic high path. |
| `mode high input` | Select Basic high input. Not an Ultra input mode. |
| `pause` / `resume` | Stop/restart normal display sweeping. |
| `repeat 1..1000` | Measurements per frequency. More repeats reduce random noise and increase time. |
| `zero N` | Set the additive raw-sample encoding offset used by `scanraw`; not an RF zero/calibration operation. |
| `sweep start F`, `sweep stop F` | Change display sweep boundaries. `sweep` alone reports settings. |
| `scan START STOP [POINTS] [OUTMASK]` | Perform a scan with at most 290 points and optional textual output. Outmask bits are 1 frequencies, 2 measured data, and 4 stored data. |
| `scanraw START STOP [POINTS] [OPTIONS]` | Perform an arbitrary-point binary scan. Option bit 0 requests unbuffered writes; bit 1 repeats continuously. |
| `rbw auto` or `rbw VALUE` | Set resolution bandwidth in kHz in current firmware syntax. Valid ranges differ by model. |
| `attenuate auto` or `attenuate 0..31` | Select automatic/manual front-end attenuation where supported. |
| `spur off\|on[\|auto]` | Control spur removal; `auto` is Ultra-specific. |
| `lna off\|on` | Ultra optional LNA; reduces overload headroom and disables its step attenuator path. |
| `ultra off\|on\|auto\|start\|harm ...` | Ultra-family build only; control extended-frequency behavior. |
| `leveloffset`, `vbat`, `threads`, `usart_cfg`, `marker` | Diagnostic/status queries used by tiny-wb; exact output is firmware-dependent. |

Avoid sending mutating calibration, output, correction, or configuration-save
commands until the model and active port are known.

## `scanraw` request

The request is:

```text
scanraw START_HZ STOP_HZ POINTS [OPTIONS]\r
```

Constraints visible in upstream firmware:

- at least start and stop are required;
- start must not exceed stop;
- points defaults to the current display sweep-point count;
- option values are bit flags: `1` unbuffered, `2` continuous, `3` both;
- waterfall is disabled before acquisition in current firmware;
- a continuous request must be explicitly aborted and needs a streaming parser.

tiny-wb omits OPTIONS, so it requests one buffered acquisition. The firmware
buffers in small blocks internally; “buffered” does not mean the entire sweep
is delivered in a single USB transfer.

## Binary response framing

Ignoring command echo, line endings, and the final prompt, the firmware emits:

```text
0x7b                                      # '{'
0x78 LOW_BYTE HIGH_BYTE                  # 'x', sample 0, little-endian
0x78 LOW_BYTE HIGH_BYTE                  # 'x', sample 1
...
0x7d                                      # '}'
```

Thus the framed body length is `1 + 3 * points + 1` bytes. Each level is a
16-bit raw value. Although the official USB page describes the bytes in one
place as “MSB LSB,” the firmware source writes `val & 0xff` first and
`val >> 8` second. The repository correctly unpacks the value as little-endian.

The `x` byte is a per-record alignment marker, not padding. Basic and Ultra use
the same braces, marker, two-byte value and exact body length; there is no
model-specific padding in the documented or reviewed `scanraw` format. Their
model-specific difference is the numeric level bias described below.

Payload bytes are arbitrary. A payload byte can equal `{`, `}`, `x`, `c`, `h`,
or `>`. Framing logic must only interpret `{` before the records, `x` at every
three-byte record boundary, and `}` after exactly the requested record count
(or after an explicitly aborted short frame). Searching binary payload for
text tokens is unsafe.

## Level conversion and `zero`

The [official USB interface](https://tinysa.org/wiki/pmwiki.php?n=Main.USBInterface)
documents these native conversions:

```text
Basic dBm = raw_u16 / 32 - 128
Ultra dBm = raw_u16 / 32 - 174
```

This is a fixed-point value with 1/32 dB numeric steps. The detector itself has
coarser effective resolution, so the encoding precision is not the same as
measurement accuracy.

The reviewed [firmware defaults](https://github.com/Ho-Ro/tinySA/blob/main/main.c#L1099-L1200)
set `config.ext_zero_level` to 128 for Basic and 174 for Ultra. The
[`scanraw` implementation](https://github.com/Ho-Ro/tinySA/blob/main/sa_cmd.c#L1221-L1267)
adds that offset to the measured value, and the
[`zero` handler](https://github.com/Ho-Ro/tinySA/blob/main/sa_cmd.c#L602-L609)
lets `zero N` replace it. tiny-wb detects the family and selects its native
offset automatically, then requires matching `zero` readback before scanning.
For Basic this gives:

```text
dBm = raw_u16 / 32 - 128
```

Ultra uses 174. Manual normalization is still available with
`scan.auto_zero_offset = false`; readback must agree with `scan.zero_offset`.
A mismatch would shift levels by 46 dB between native defaults, so acquisition
now rejects missing or mismatched readback. See [support and migration](ultra-support.md).

### Worked record

Suppose the three bytes are:

```text
78 00 08
```

`0x0800` is 2048. With `zero 128`:

```text
2048 / 32 - 128 = -64 dBm
```

## Frequency assignment

The official USB command page specifies the `scanraw` request and record count,
but does not define a point-to-frequency formula. The upstream
[`cmd_scanraw` implementation](https://github.com/Ho-Ro/tinySA/blob/main/sa_cmd.c#L1221-L1263),
reviewed on 2026-10-05, is the byte-level authority used here. Its C expression
has three relevant conversions:

```text
integer_step_hz = integer_divide(stop_hz - start_hz, points)
firmware_step_hz = float32(integer_step_hz)
frequency_hz[i] = start_hz + truncate_to_freq_t(firmware_step_hz * float32(i))
                    for i = 0 .. points - 1
```

The denominator is `points`, not `points - 1`. Integer division discards any
remainder before the assignment to the 32-bit `float`; multiplication can then
round at large values before conversion back to `freq_t`. For example, a
0–10 Hz, three-point request measures nominally at 0, 3, and 6 Hz. The requested
stop is an upper boundary and the final point is normally roughly one step below
it. tiny-wb's `firmware` axis reproduces these conversions, including the
single-precision rounding.

The default is now `scan.frequency_axis = "firmware"`. The compatibility-only
`inclusive` setting retains labels produced by older tiny-wb releases:

```text
host_step = (stop - start) / (points - 1)
frequency[i] = start + host_step * i
```

`inclusive` labels both requested endpoints and disagrees with the reviewed
firmware. The discrepancy grows linearly through a sweep to almost one firmware
step at the last point. Existing saved configuration remains compatible because
an explicit `inclusive` value is still accepted. Record the connected device's
`version` when validating a narrow carrier: the synthetic tests establish source
parity, not RF frequency accuracy or behavior of every firmware revision.

## Prompt and synchronization behavior

A reliable request/response state machine should:

1. retain a persistent receive buffer across OS reads;
2. write the complete command plus `\r`;
3. parse optional echo and line endings;
4. for text commands, recognize `ch>` even when split across reads and retain
   any bytes after it;
5. for `scanraw`, find the frame start in shell text, then consume exactly the
   expected number of aligned records and the closing brace before looking for
   the prompt;
6. use an overall deadline plus idle deadlines rather than treating one serial
   timeout as a successfully completed short reply;
7. make cancellation explicit with the firmware's `abort` behavior;
8. return protocol errors that include command, stage, byte counts, and a
   bounded hex/text diagnostic.

Sleeping a fixed duration is not a protocol guarantee. Sweep time changes with
span, RBW, repeat, mode, signal-processing options, and firmware.

## Current tiny-wb transaction

The current code sends the following logical sequence (frequency placeholders
are decimal Hz):

```text
version
info
help
leveloffset
vbat
usart_cfg
threads
marker
repeat REPEAT
zero MODEL_OFFSET
zero
pause
mode LOW_OR_HIGH input
sweep start START
sweep stop STOP
sweep
scan START STOP 290 101
scanraw START STOP POINTS
resume
```

Every item except `scanraw` goes through the text helper and waits for `ch>`.
The transport retains unread bytes and recognizes prompts split across serial
reads. It consumes the complete length-delimited raw frame before returning to
text prompt parsing; arbitrary sample bytes cannot prematurely end the frame.
The pure decoder requires the exact record count, alignment and closing brace.

After attempting `pause`, acquisition attempts `resume` even if frame decoding
or a later command fails. A failed recovery is included with the original error.
There is no cancellation protocol or full state restoration, and process exit
can prevent recovery. The offset requires readback; other settings are checked
for recognizable command errors. Optional diagnostic queries require `help`
advertisement. See [model-specific planning](ultra-support.md).

The `scan` outmask value `101` remains outside the documented three-bit mask and
its returned data is discarded. The legacy preparation is enabled by default
for compatibility, controlled by `scan.legacy_pre_scan`. Establish its hardware
purpose before changing the default or copying it into another client.

## Manual diagnostic session

Use a serial terminal that can send carriage return, with every RF source
disconnected or safely attenuated. A minimally invasive identification session
is:

```text
version
info
help
sweep
vbat
```

Save the exact transcript. Do not test `scanraw` in a terminal that assumes all
input is UTF-8; it returns binary and may corrupt the display or its own log.
Use a byte-oriented program and a bounded point count instead.

## Compatibility rules for implementers

- Identify capabilities; do not infer Ultra merely from a version prefix if
  `info` gives an explicit model.
- Treat the command list as a firmware-build property.
- Keep Basic low/high path selection separate from Ultra normal/Ultra/harmonic
  range selection.
- Make raw offset, valid frequency range, RBW limits, and legal mode syntax
  model capabilities.
- Do not use UTF-8 strings for the mixed shell/binary receive buffer.
- Never use an unescaped textual prompt as a delimiter inside a binary frame.
- Restore or clearly report every instrument setting that the client changes.
- Include model, firmware version, configured offset, start/stop, point count,
  repeat, RBW, attenuation, LNA, and Ultra state in future output metadata.

See [Project architecture](architecture.md#extension-points-and-remaining-limits) for
remaining implementation limits and [tiny-wb operations](tiny-wb-operations.md)
for what users can safely expect today.

## Configuration command rejection

`Shell::command_checked` rejects text lines beginning with `error:`, `usage:`,
or `unknown command` (case-insensitive). Acquisition uses it for settings and
pause/resume; diagnostic queries retain raw replies. This catches known failure
forms, not every firmware-specific error or acknowledgement. Binary scan errors
still fail framing/timeout checks. Model-aware replies remain roadmap work.
