# Project architecture

## Purpose and boundary

`tiny-wb` is a single-process desktop terminal application. Its job is narrow:
collect an amplitude-versus-frequency sweep from a tinySA through the USB
serial console, persist the sweep as CSV, and show the result in a terminal
chart.

It is not currently a general tinySA controller, live spectrum display,
firmware updater, calibration utility, or Rust protocol library. It changes a
small amount of temporary instrument state during acquisition and restores
continuous sweeping with `resume`; it does not restore every preceding device
setting.

## Runtime data flow

```text
keyboard
   |
   v
Ratatui event loop (AppInfo)
   |  selected port, baud, MHz range, points, repeat
   v
ConnectionInfo (shared Arc<Mutex<...>> state)
   |
   +---- UI thread: render progress, device info, errors, chart
   |
   `---- scan worker thread
           |
           v
      serialport / USB CDC
           |
           v
        tinySA shell ---- text responses ending in "ch>"
           |
           `------------ scanraw binary frame
                              |
                              v
                    parser -> Vec<(MHz, dBm)>
                              |
                              +-> timestamped CSV
                              `-> final Ratatui chart
```

The UI remains responsive because scanning runs in a worker thread. Clones of
`ConnectionInfo` share progress, status, returned data, messages, file name,
and error state through `Arc<Mutex<_>>` values. The selected connection and
scan parameters themselves are copied into the worker.

## Source map

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Start file logging and error reporting, initialize/restore the terminal, enumerate serial ports, and run the application. |
| `src/ui.rs` | Construct defaults, drive the event/render loop, transition screens, spawn the scan worker, plot results, and map errors to user messages. |
| `src/ui_struct.rs` | Define UI state, selectable fields, baud-rate and port lists, numeric bounds, colors, and navigation helpers. |
| `src/structs.rs` | Define configuration persistence, shared connection state, status values, and application/protocol error types. |
| `src/scan.rs` | Open the serial port, issue shell commands, find prompts, request `scanraw`, parse binary samples, convert levels, and write CSV. |
| `build.rs` | Embed the Git short hash and, on Windows, compile the application icon resource. |
| `Jenkinsfile` | Run formatting, checks, tests, Clippy, release build, and a non-fatal interactive-binary smoke diagnostic. |

## UI state machine

`AppState` advances in one direction during a normal run:

```text
InitialConfig -> FinalConfig -> Finished -> EndWindow -> exit
      |               |             |
      `---------------+-------------+-> Error -> exit
```

- `InitialConfig` selects a serial port and baud rate.
- `FinalConfig` selects start/stop MHz, sample count, and repeat count.
- Enter from `FinalConfig` writes `config.toml`, fills `ConnectionInfo`, and
  spawns the worker.
- `Finished` is somewhat counter-intuitively the in-progress screen. It polls
  shared status while the worker runs.
- `EndWindow` plots the collected samples and reports success.
- `Error` presents the mapped failure and exits on confirmation.

Defaults are 240–960 MHz, 10,001 points, one measurement per frequency, and
115,200 baud. The UI permits 0–960 MHz in 10 MHz steps, 101–1,000,001 points in
100-point steps, and repeat values from 1–1,000. A valid `config.toml` overrides
the defaults. Frequencies are converted from UI MHz to protocol Hz only when
`ConnectionInfo` is filled.

## Acquisition pipeline

`scan_with_progress` performs these operations:

1. Validate the requested range and choose Basic `low` or `high` input mode.
2. Open the selected port with a 3 s read timeout.
3. Drain stale input until the shell prompt is seen or reading times out.
4. Query `version`, `info`, `leveloffset`, `vbat`, `usart_cfg`, and `threads`.
5. Send `repeat N` and `zero 128`.
6. Query markers, pause the normal sweep, and set the input mode.
7. Set sweep start/stop, query the sweep, then run a 290-point textual `scan`.
8. Send `scanraw START STOP POINTS` without immediately consuming a text reply.
9. Read and parse its binary frame, update progress, and write CSV.
10. Send `resume`, set progress to 100%, and expose the samples to the UI.

Step 7's textual `scan ... 290 101` does not contribute samples to the final
CSV; its response is read and discarded. It therefore needs a documented
reason or removal during a future protocol cleanup.

## Protocol boundary

Text commands are terminated with carriage return (`\r`). `send_cmd` sleeps,
then reads until it finds the ASCII prompt `ch>`, and sleeps again. This is a
timing-assisted, prompt-delimited request/response protocol. The application
does not currently:

- parse command echo separately from command output;
- validate an explicit success status;
- retain bytes after a prompt found in the middle of a serial read;
- account for a prompt split across two read buffers;
- preserve partial results after a read timeout;
- negotiate capabilities from `version`, `info`, or `help`.

The `scanraw` body is different: it is a binary `{...}` frame made of
three-byte sample records. See [USB console and `scanraw` protocol](usb-protocol.md).

## Persistence and outputs

`config.toml` is stored in the process working directory and contains:

```toml
port = "/dev/ttyACM0"
baudrate = 115200
start_frequency = 240
stop_frequency = 960
points = 10001
repeat = 1
```

Each acquisition immediately creates a timestamped CSV in the working
directory. It has no header. Column one is the application-computed frequency
in MHz and column two is level in dBm. Logs are produced by `flexi_logger` in
the working directory with its default file naming policy.

## Important invariants and risks

- A scan needs at least two points because host interpolation divides by
  `points - 1`; the UI minimum of 101 enforces this, but the parser itself does
  not.
- `read_port_with_progress` estimates `3 * points + 6` bytes. Progress is only
  an estimate because command echo and prompt bytes are also present.
- The parser finds the first `{`, requires each subsequent full triple to start
  with `x`, and stops on a triple whose first byte is `}`. End-frame validation
  is currently commented out, so a truncated frame can be accepted.
- The firmware emits sample bytes little-endian even though one wiki sentence
  labels them “MSB LSB.” The upstream C implementation and the Rust parser both
  use low byte followed by high byte.
- Firmware computes `scanraw`'s step as `(stop - start) / points`, while the
  host labels samples using `(stop - start) / (points - 1)`. The final host CSV
  therefore labels its last point at `stop`, but current firmware measures its
  last point one step below `stop`. This should be resolved in code, ideally by
  testing against the target firmware version.
- `zero 128` deliberately asks firmware to encode the level around a 128 dB
  offset and the host subtracts 128. Without that command, the documented
  default offset is 128 for Basic and 174 for Ultra.
- Input-mode selection encodes Basic-only assumptions. Ultra has no `high`
  input command, and the UI cannot request its ranges above 960 MHz.

## Dependency roles

- `ratatui` and `crossterm` (through Ratatui) provide terminal rendering and
  input.
- `serialport` enumerates and opens the device.
- `packed_struct` decodes the three-byte sample representation.
- `memchr` locates `{` and the `ch>` prompt.
- `serde` and `toml` persist settings.
- `chrono` names scan files.
- `flexi_logger`, `log`, `color-eyre`, and `thiserror` cover diagnostics and
  error propagation.
- `strum` derives iteration/counting for menu enums.

The manifest intentionally contains only dependencies used by the current
binary. In particular, the parser uses `std::sync::OnceLock`; no external
`once_cell` dependency is needed.

## Recommended evolution path

The cleanest route to reliable multi-model support is to separate four layers:

1. a byte-accurate transport that preserves unread bytes and uses deadlines;
2. a shell codec for echo, response, prompt, and binary frames;
3. a capability/model layer derived from `version`/`info` plus explicit
   firmware quirks;
4. the TUI and CSV application built on a reusable acquisition API.

Tests should cover prompt fragmentation, binary frames containing arbitrary
byte values, truncation, Basic versus Ultra offsets/modes, frequency labeling,
timeouts, and disconnects. Captured serial transcripts make good fixtures and
do not require hardware in CI.
