# Project architecture

`tiny-wb` has a reusable acquisition library and a thin terminal application.
The core accepts typed values and I/O interfaces; it does not depend on UI
widgets, port discovery, worker threads, or the application's working directory.

## Source map

| File | Responsibility |
| --- | --- |
| `src/lib.rs` | Public core module boundary. |
| `src/config.rs` | Typed settings, recursive partial overrides, legacy-key compatibility and validation. |
| `config/default.toml` | Authoritative embedded defaults, with units and comments. |
| `src/storage.rs` | Same-directory atomic replacement for saved configuration. |
| `src/protocol.rs` | Structural scan validation, wire invariants and pure raw-frame decoding. |
| `src/device.rs` | Family identification, model-specific ranges, input paths and native offsets. |
| `src/transport.rs` | Buffered `Read + Write` shell I/O, complete command writes, text prompts and length-delimited raw frames. |
| `src/connection.rs` | Runtime serial/TCP adapter; native serial is optional. |
| `src/mock/` | Opt-in bounded device simulation and stream server. |
| `examples/mock-tinysa-server.rs` | Portable TCP listener for the mock, outside release binaries. |
| `src/acquisition.rs` | Command ordering, progress events, frame decoding and resume attempts. |
| `src/export.rs` | CSV parsing/serialization over I/O traits and collision-safe file export. |
| `src/main.rs` | Select acquisition/direct-view mode, load configuration, start diagnostics, enumerate ports when needed and manage terminal lifetime. |
| `src/error.rs` | Application/UI boundary errors with original context. |
| `src/scan.rs` | Open the configured connection, adapt core events to shared progress, and export successful acquisitions. |
| `src/ui/mod.rs` | Screen transitions, keyboard handling, worker startup and progress reconciliation. |
| `src/ui/state.rs` | UI state, configured port/baud choices and bounded numeric input. |
| `src/ui/view.rs` | Ratatui rendering and selection styling. |
| `build.rs` | Embed Git revision and compile the Windows icon resource. |
| `Jenkinsfile` | Required quality gates, coverage, badge cleanup/archival/publication and diagnostic smoke execution. |

## Data flow

```text
embedded defaults + runtime overrides -> validated Config
                                            |
                             startup / terminal controller
                                            |
                       ScanRequest (Hz) + selected Config
                                            |
                                      connection worker
                                            |
                          Shell<Connection> -> acquisition
                                            |
                                decode_frame -> Vec<Sample>
                                            |
                           CSV export + completed UI snapshot
```

`ScanRequest` carries Hz, sample count and repeat count. `Sample` carries MHz
and dBm. Units are converted at the UI/core boundary, not inside widgets or
file naming. The selected zero offset is both sent to the instrument and used
by the decoder. Frequency labeling is an explicit policy; the default mirrors
the reviewed firmware's integer/single-precision `scanraw` calculation, while
legacy endpoint-inclusive labels remain available for compatibility.
Protocol constants record the native raw-level biases (Basic 128, Ultra 174).
`device.rs` detects the family and validates its acquisition plan. Both the
application and validator select the native bias automatically and confirm
the readback. See [support scope](ultra-support.md).

The core reports `ScanEvent` values. `ScanState` adapts them to one
`Arc<Mutex<Progress>>`; completion publishes samples, filename, status and 100%
progress together only after export succeeds. The UI transfers the completed
sample vector once, rather than cloning it every frame. Error reporting uses
the original error's message instead of a second exhaustive mapping.

## UI state machine

```text
SelectingConnection -> SelectingScan -> Scanning -> Complete -> exit
          |                  |             |
          +------------------+-------------+-> Error -> exit
CSV file -------------------------------------> Complete
```

The constructor takes configuration and enumerated ports as values, without
reading files. Selection is clamped before rendering, including empty lists.
Before a scan starts, the controller validates the complete selected settings
and request, atomically saves settings, then starts a worker. The controller
reconciles worker status before drawing; rendering does not consume samples.
The complete screen owns one named plot with an independent frequency viewport.
Zooming and panning mutate only that viewport, not sample data. This plot
boundary can grow into a tabbed collection without coupling CSV parsing or
acquisition to Ratatui.

The terminal guard restores terminal modes on ordinary returns and unwinding.
Configuration, logging and enumeration errors occur before terminal takeover.
Process termination or release-mode panic abort cannot be recovered by a guard.

## Acquisition and protocol boundary

1. Validate structural request and sample/repeat limits before using a port.
2. Synchronize with stale input, allowing an idle timeout only during this step.
3. Query `version`/`info`, validate the detected model's range, check `help`, and run advertised diagnostics.
4. Set `repeat` and the selected raw zero offset, then verify its readback.
5. Attempt `pause`, select input mode, and configure the display sweep.
6. Optionally retain the historical `scan START STOP 290 101` preparation.
7. Request `scanraw`, consume exactly the expected framed record count, then
   consume the trailing shell prompt.
8. Decode/validate all records and attempt `resume` on success or failure after
   pause was attempted. Preserve both failures if resume also fails.
9. Export validated samples and mark the application scan complete.

The transport uses a persistent buffered stream. Prompt matching spans serial
reads; bytes beyond a prompt remain available. Binary records are read by
length, so sample bytes equal to braces or text characters are not delimiters.
EOF, disconnect, oversized text and missing prompts are errors rather than
successful partial reads. Writers use `write_all`, and protocol sequencing does
not require `unsafe`.

See [USB protocol](usb-protocol.md) for firmware evidence and the unresolved
frequency-label compatibility decision. Device facts are not inferred from
our synthetic test transcripts.

## Configuration and persistence

The six legacy selection keys remain top-level. Additional sections cover
serial timing/choices, scan policy, event timing, chart bounds, output and
logging. Partial files merge with embedded defaults; unknown or invalid values
fail explicitly. Saving selection values retains all recognized sections.

CSV formatting is independent of file creation. The worker does not create a
CSV until a complete frame has been validated and acquisition has returned
success. Filenames contain microseconds plus a collision counter; exclusive
creation avoids overwriting earlier scans. Failed writes remove incomplete
files when the filesystem permits it. A file may be visible while being
written; the returned path/completed status is the success indicator.

See [configuration](configuration.md) for settings, paths and exact behavior.

## Extension points and remaining limits

- Add model/capability recognition ahead of request planning. The current
  Basic mode rules must not be stretched to imply Ultra support.
- Add new output formats beside `write_csv`, consuming `Sample` values rather
  than parsing the wire format again.
- Inject transcript streams into `Shell` and writers into export tests. Keep
  UI tests on Ratatui's in-memory backend. See [testing](testing.md).
- Settings change policy and timing; wire delimiters, fixed-point scaling and
  command grammar remain protocol invariants.
- Known error/usage replies reject configuration commands, but responses are
  not a complete typed acknowledgement protocol. An echo-only identity can
  appear nonempty; prompt arrival alone is not proof of acceptance.
- Serial timeouts are per-read idle timeouts, not a whole-transaction deadline.
  Cancellation/abort and complete device-state restoration remain future work.
  Quitting during a scan can terminate the worker before its resume attempt.
- Synthetic tests do not establish the connected firmware's frequency stepping,
  raw-offset acceptance, or need for the legacy preliminary scan. Defaults retain
  previous behavior until hardware evidence supports changing it.

## Dependency policy

The core uses standard-library byte decoding instead of `packed_struct`, and
buffered framing instead of a separate direct `memchr` dependency. Transitive
`memchr` use elsewhere in the graph is unrelated. Serde/TOML handle settings;
serialport, Ratatui, chrono, log/flexi_logger, color-eyre and thiserror retain
their device, presentation, timestamp and error-reporting roles.

Library consumers can omit terminal and serial dependencies; see [feature selection](library.md).
The [mock](mock-device.md) uses the same shell/acquisition code over memory or TCP.
