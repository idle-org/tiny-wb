# tinySA Basic and Ultra implementation tasks

Planning baseline: repository state after `b4d1462`, reviewed 2026-10-05.
This file is the detailed implementation backlog. The
[roadmap](docs/roadmap.md) remains the short feature overview; shared IDs below
refer to the same work. All unchecked items are proposals, not implemented
features or certified device support. Stretch goals are unselected until the
user chooses them.

## What complete coverage means

Coverage here means instrument features, not Rust line coverage. Qualify each
exact model, hardware revision, firmware version, and host platform separately.
Basic and Ultra are the primary targets; Ultra+ and other revisions need
explicit profiles and evidence rather than inheriting support by name.

For every console command and front-panel feature, record one disposition:
implemented and verified; implemented but awaiting hardware; front-panel-only;
not present on this model/build; or blocked with a concrete upstream dependency.
No feature may disappear silently from the inventory. A documented limitation
completes the audit entry, but does not make that feature remotely implemented.
Do not advertise full remote coverage while remote-capable entries remain open.

Use the existing [source catalogue](docs/sources.md),
[protocol evidence](docs/usb-protocol.md), and
[compatibility matrix](docs/tiny-wb-operations.md#compatibility-matrix).
Their firmware observations are dated; this planning change does not reverify
upstream firmware. Command names below are investigation candidates until P01
confirms their syntax and availability on the selected build.

## Current baseline

Update 2026-10-05: automatic family detection, offset readback and common Ultra
normal-range acquisition are now implemented. The table below is the planning
baseline; [current support status](docs/ultra-support.md) supersedes its missing
model-detection entry. Hardware and extended-range gates remain open.

| Area | Delivered | Still missing |
| --- | --- | --- |
| Library | Terminal-independent config, framing, Basic acquisition, CSV; optional serial/mock features | Model/session API and the remaining device operations |
| Basic | Low/high selection, one-shot raw sweep, repeat, configurable zero offset, resume attempt | Hardware qualification, processing/input controls, generator and complete workflows |
| Ultra | Synthetic profile; experimental physical validator at 240–300 MHz | Model detection, normal/extended range planning, Ultra-specific controls and qualification |
| Transport | Buffered mixed text/binary I/O, complete writes, bounded responses, serial/TCP adapters | Overall deadlines, abort, reconnect, typed replies and full recovery |
| Interface | One scan per TUI launch, plot and CSV | Headless commands, repeat/live operation, capability-aware controls |
| Verification | Regression/mock tests, CI gates, guided hardware validator | Captured real-device fixtures, RF reference measurements and platform matrix |

- [x] F01–F04: reusable library, injected I/O, bounded mock profiles and loopback tests.
- [x] H00: initial [hardware checklist](docs/hardware-validation.md),
  [launcher](scripts/test-hardware.sh), and [validator](examples/validate-hardware.rs).
- [ ] H00 qualification: execute on real Basic and Ultra units and retain evidence.
  Providing the validator has not completed hardware acceptance.

## Implementation rules and common completion gate

Keep the boundaries in [architecture](docs/architecture.md). Candidate new
modules such as `device`, `commands`, `session`, and `measurements` below are
proposed responsibilities, not existing APIs. Introduce them only with the
first concrete consumer; do not recreate a miscellaneous `structs` module.

Every feature slice must include:

- [ ] A typed library operation, validation before mutation, and an explicit
  supported-model/firmware capability entry.
- [ ] Protocol parsing over bytes or `Read`/`Write`, with no hidden port/file
  opening; settings use MHz/dBm/milliseconds, wire frequencies use Hz.
- [ ] Mock/transcript tests for success, rejection, boundaries, fragmentation,
  timeouts, partial writes, disconnects and recovery where applicable.
- [ ] CLI/TUI exposure appropriate to the operation, including unsupported-state
  feedback; diagnostics must not turn a rejected command into success.
- [ ] Configuration compatibility or a tested migration, Rustdoc, user guidance,
  source links and hardware evidence status updated together.
- [ ] The [required verification commands](docs/security-and-ci.md#local-verification)
  pass; run `cargo coverage` for Rust behavior and security gates for dependency
  changes. Keep production coverage exclusions and the floor policy unchanged.

Hardware results must name the device/build, settings, RF setup and references,
expected result, observed result, tolerances and artifact paths. Mock results
must remain labeled synthetic. Publish or run shared Jenkins jobs only as a
separately authorized action.

## Delivery order

| Milestone | Work | Exit condition |
| --- | --- | --- |
| M0: reliable evidence | H01, P01, initial Q01/Q02 | Identity and command fixtures for each target; failed tests retain reports |
| M1: trustworthy acquisition | P02–P04, C01, initial I01/I02 | Basic and Ultra scans use verified profiles; timeout/cancel/recovery work |
| M2: analyzer parity | A01–A07, continuing I01/I02/Q01/Q02 | Analyzer controls and traces exposed and qualified per profile |
| M3: remaining device workflows | G01, D01–D04 | Generator, display, persistence, calibration and management classified/implemented |
| M4: supported release | I03, Q02, Q03 | Complete feature ledger and tested releases for explicitly supported combinations |

Start the small `list`/`inspect` CLI in M0 and extend it feature by feature.
Do not postpone all interface work until every analyzer control exists.
Hardware evidence collection can run alongside implementation; it is a gate
for support claims, not a reason to stop developing transcript tests.

## M0 — evidence and device identity

### H01 — harden the guided hardware validator

Depends on H00; extend incrementally with P01/P03/P04. Start in
`examples/validate-hardware.rs`, `scripts/test-hardware.sh`, and the checklist.

- [ ] Treat stdin EOF as cancellation. Do not interpret a closed input stream
  as confirmation to start a scan.
- [ ] Retry enumeration for a bounded interval; require explicit selection if
  no newly attached port is found. The current sole-port fallback can select an
  unrelated serial device. Handle multiple devices and permissions explicitly.
- [ ] Query identity and `help` before any mutating acquisition. Confirm the
  selected model/profile before scanning; current confirmation happens afterward.
- [ ] Create a unique session directory with exclusive creation and write the
  initial report immediately. Append/checkpoint each stage so early I/O failures
  preserve identity, completed work and the original failure.
- [ ] Record app revision/version, OS, model label, firmware, raw identity/help,
  test settings including zero offset and frequency-axis policy, and timestamps.
  Preserve bounded raw transcripts separately from sanitized terminal text.
- [ ] Give separate pass/fail/skipped/unverified results for command acceptance,
  data framing, export, operator identity and front-panel recovery. A no-signal
  scan cannot establish RF amplitude or frequency accuracy or prove zero readback.
- [ ] Run paired legacy/direct scans with identical settings before attributing
  differences to the preparatory scan. Keep Basic high-path coverage as its own
  case; currently Basic changes path and preparation in the same comparison.
- [ ] Add portable invocation instructions using `cargo run --example` on
  Windows, an optional prebuilt validator, and concise `--help`/argument errors.
- [ ] Add optional reference-source and unplug/reconnect stages after P03/P04;
  skip them explicitly when equipment is absent. Prompt once per physical wiring
  change, automate all software checks, and retain a useful final summary.

Acceptance: test EOF, ambiguous ports, wrong model, second-scan failure, failed
report writes and negative operator confirmation without hardware. A real failed
run must still leave a report; a skipped check must never become a pass.

### P01 — inventory, identification and capabilities

Depends on F01–F04; H01 supplies evidence. Touch acquisition identification and
add a concrete `device` module plus versioned fixture metadata.

- [ ] Build a ledger per exact device/build: feature, command/arguments, units,
  limits, query/readback form, mutation/persistence effects, source revision,
  test fixture and support status.
- [ ] Inventory both official menu trees and command registration at the matching
  firmware commit. Resolve the differing upstream repository references in local
  docs against each captured firmware; do not treat a moving branch as evidence.
- [ ] Parse `version`/`info` after removing only known echo/prompt framing. Reject
  empty and echo-only replies; retain unknown fields verbatim.
- [ ] Parse command availability from `help`, but distinguish advertised commands
  from verified argument forms and successful readback.
- [ ] Represent identity confidence and unknown firmware explicitly. Permit a
  recorded profile override without pretending it proves hardware identity.
- [ ] Gate optional diagnostic queries (`threads`, `usart_cfg`, etc.) on
  availability; an absent debug command must not invalidate a supported scan.
- [ ] Keep inspection free of calibration, RF-output and persistent-write probes.

Acceptance: Basic, Ultra, unknown build, conflicting identity, echo-only and
missing-optional-command fixtures produce the expected profile or an explicit
unsupported/uncertain result before settings change.

## M1 — reliable Basic and Ultra acquisition

### P02 — separate model policy from raw decoding

Depends on P01. Main files: `src/protocol.rs`, `src/acquisition.rs`,
`src/config.rs`, `src/ui/state.rs`, `src/ui/mod.rs` and `src/mock/`.

- [ ] Move Basic physical-path/range selection out of generic frame-length and
  sample-decoding rules. Keep delimiters, little-endian representation and raw
  scale in the protocol module.
- [ ] Introduce a validated acquisition plan containing selected profile/path,
  command sequence, sample count, axis policy and confirmed encoding offset.
- [ ] Preserve the current Basic overlap/boundary policy unless hardware evidence
  justifies a documented change; test spans below, at and across each boundary.
- [ ] Add Ultra normal/extended/harmonic mode plans only for evidenced ranges.
  Never send Basic `mode high input` on Ultra. Distinguish hardware revisions,
  unlocked state and firmware limitations; the mock's ceiling is not a spec.
- [ ] Remove Basic-only UI/config ceilings when a resolved profile supplies the
  range. Split structural config validation from post-identification validation
  so detection can occur before rejecting a legitimate Ultra request.
- [ ] Verify `zero` acceptance/readback where available, bind the decoder to the
  actual encoding, and expose uncertainty when a setting cannot be confirmed.
- [ ] Resolve frequency assignment using matching firmware and a reference
  signal; preserve legacy inclusive behavior through an explicit migration.
- [ ] Qualify removal/replacement of the legacy `scan ... 290 101` preparation;
  explain its undocumented mask and observed effects before changing defaults.

Acceptance: Basic path boundaries and Ultra boundaries reject invalid plans
without writes; valid Ultra plans above 960 MHz work on declared hardware.
Known-source measurements verify frequency/level conversion, including the
potential 46 dB offset mismatch. No synthetic result alone closes this task.

### P03 — typed replies, deadlines, abort and reconnect

Depends on P01; develop alongside P02. Extend `src/transport.rs` and
`src/connection.rs`; add small command-specific parsers as needed.

- [ ] Define typed replies/errors for implemented commands. Handle known firmware
  rejection variants, optional echoes and readback mismatches; a prompt alone is
  not acknowledgement of a setting change.
- [ ] Track an overall transaction deadline separately from idle read timeout.
  Bound each blocking adapter call by remaining time; checking a clock only
  between indefinitely blocking reads is insufficient.
- [ ] Add a cancellation token checked at bounded intervals, verified firmware
  abort syntax, and a drain/resynchronization state before another command.
- [ ] Decode streaming/aborted frames with explicit complete/partial states.
  Never search arbitrary sample bytes for a text prompt.
- [ ] Include command, stage, expected/received bytes and bounded diagnostics in
  errors. Keep binary captures as bytes; escape terminal-control characters.
- [ ] Retry only operations known to be safe/idempotent. Reconnect must reidentify
  the unit and reset stale buffers; never replay output/calibration mutations.
- [ ] Introduce a clock/adapter test seam only where needed to simulate elapsed
  deadlines and blocking behavior deterministically.

Acceptance: fragmented prompts, slow trickle, total silence, late replies,
unplug mid-frame, aborted short frames and reconnect to another device all
terminate predictably without reporting partial data as a complete acquisition.

### P04 — session ownership and state restoration

Depends on P02/P03. A concrete session should own connection, identified profile,
operation state and restorable settings; the UI owns its worker lifetime.

- [ ] Snapshot queryable settings before mutation and track only successful
  changes. Record settings whose previous values cannot be queried.
- [ ] Restore settings in a verified order on success, failure and cancellation;
  preserve the primary failure together with every recovery failure.
- [ ] Replace detached worker exit with cancel-and-join under a bounded timeout.
  Ensure `q`, Escape and process interrupts do not silently abandon acquisition.
- [ ] Expose recovery as events/status, not just a `Drop` side effect. Abort-mode
  panics, unplugged devices and power loss cannot guarantee remote restoration.
- [ ] Prevent simultaneous commands from UI, scanning and diagnostics. Document
  limits of OS port exclusivity rather than assuming universal locking.
- [ ] Do not automatically re-enable RF output when restoring a preset/session;
  generator recovery needs a separate, explicit state policy in G01.

Acceptance: inject failure after each mutation and assert restoration attempts,
error preservation and worker termination. Verify final device state physically;
mark unavailable restoration as incomplete, never silently successful.

### C01 — typed settings and compatibility migration

Depends on P01/P02; accompanies every new control. Extend `src/config.rs`,
`config/default.toml`, persistence and configuration examples.

- [ ] Design explicit profile selection, analyzer settings and operation timeout
  sections; distinguish requested settings from observed device state.
- [ ] Preserve six legacy selection keys and existing partial overrides, or
  provide a tested migration with backup and actionable errors.
- [ ] Validate units, finite values, supported enum choices, mode combinations,
  allocation limits and empty collections before commands/output creation.
- [ ] Keep model/wire invariants in code/profile evidence; changing a config
  number must not authorize an unsupported physical range.
- [ ] Persist only intended user settings; do not silently save transient
  generator enablement or hardware identity as permanent truth.

Acceptance: old configs still load, unknown keys fail, invalid combinations
cause no mutations, and failed persistence leaves the previous config usable.

## M2 — analyzer feature coverage

Each A-task depends on P01–P04/C01 and must extend I01/I02/Q01/Q02 in the same
feature slice. Firmware-only controls are conditional on the capability ledger.

### A01 — frequency, sweep and timing controls

- [ ] Add start/stop and center/span conversion with checked arithmetic; separate
  frequency sweeps from CW/zero-span time traces so the axis cannot be mislabeled.
- [ ] Add supported point counts, repeat, sweep time, single/continuous operation
  and actual frequency tables/readback rather than assuming requested settings.
- [ ] Inventory external reference, frequency offset, wideband/multiband and
  segment behavior; implement supported forms with explicit metadata.
- [ ] Estimate progress from known phases without presenting byte count as a
  reliable prediction of measurement duration.

Acceptance: boundary and overflow tests; compare requested/actual sweep and time
axis on hardware. Unsupported sweep forms fail before commands are sent.

### A02 — input paths, attenuation, gain and Ultra modes

- [ ] Model Basic low/high input separately from Ultra normal/extended/harmonic
  modes; surface the physical connector and selected mode to the user.
- [ ] Add manual/automatic attenuation, available AGC/LNA stages and legal
  combinations using per-profile ranges and mutual-exclusion rules.
- [ ] Inventory exposed IF and expert controls; retain automatic defaults until
  an explicit user choice and verified syntax justify changing them.
- [ ] Capture overload/readback indicators where available; distinguish measured
  overload status from a host heuristic.

Acceptance: illegal combinations are rejected, transitions/readback match the
panel, and low-level reference measurements show expected gain changes within
documented tolerances. Do not use damage-limit signals as test inputs.

### A03 — bandwidth, detector and trace processing

- [ ] Implement auto/manual RBW and available VBW with explicit unit conversion
  at command boundaries; profile limits may differ.
- [ ] Add supported averaging, detector/calculation modes, min/max hold,
  reference/subtraction traces and spur controls, including reset semantics.
- [ ] Record firmware processing separately from host processing. Changing span,
  RBW or mode must invalidate incompatible accumulated/reference traces.
- [ ] Keep state and bounded trace buffers outside terminal rendering code.

Acceptance: deterministic sequences verify averaging/hold/reset; real-device
fixtures verify command forms, and RF tests record their bandwidth/settings.

### A04 — amplitude, display units and correction

- [ ] Add supported reference level, scale and display units; keep canonical
  exported levels explicit and do not change values merely to fit a chart.
- [ ] Separate raw encoding offset, external loss/gain and calibration correction
  in types and metadata; prevent correction from being applied twice.
- [ ] Define conversions requiring impedance or bandwidth only when that context
  is available. Never label arbitrary dBm conversions as noise density.
- [ ] Validate correction tables for ordering, units, interpolation and range
  policy; device-persistent calibration writes belong to D03.

Acceptance: numeric tests cover zero/negative/non-finite inputs and round trips;
reference measurements verify exported amplitude and visible correction state.

### A05 — trace APIs, streaming and export provenance

- [ ] Add typed ASCII scan, binary raw, stored trace and frequency-query decoders
  only for advertised forms. Verify frequency/value lengths and record counts.
- [ ] Introduce trace metadata: device/build/profile, requested and actual range,
  axis policy, offset, RF path, processing, timestamps and completion status.
- [ ] Support repeated snapshots before native continuous frames; both need
  bounded queues, cancellation, backpressure and explicit dropped-frame counts.
- [ ] Preserve current two-column CSV compatibility. Add opt-in metadata sidecars
  or a documented versioned format; keep serialization independent of paths.
- [ ] Define atomic completion for trace plus metadata and a clearly labeled
  partial-output policy. Disk-full/cancel must not leave a success-looking file.

Acceptance: transcript replay, multiple frames per read, truncated streaming
frames, full queues and disk failures preserve correct ordering and status.

### A06 — markers and measurement readouts

- [ ] Implement available marker enable/select/move, peak, tracking, reference
  and delta controls with typed frequency/level results.
- [ ] Inventory model-specific readouts and measurement modes; implement each
  exposed form or record a precise front-panel limitation.
- [ ] Distinguish firmware readouts from calculations over exported samples;
  include algorithm and resolution when the host computes a result.
- [ ] Handle flat traces, tied peaks, nonuniform frequency tables and stale traces.

Acceptance: known trace fixtures and device-panel comparisons agree within
specified resolution/tolerance; unsupported readouts are never fabricated.

### A07 — trigger workflows

- [ ] Inventory trigger sources, level, polarity, position and auto/single modes
  per build; add validated operations and readback where available.
- [ ] Represent armed, waiting, triggered, complete, cancelled and failed states.
  Trigger waiting must have its own configurable deadline/status.
- [ ] Combine cancellation with P03 abort/resynchronization and P04 restoration.

Acceptance: no-trigger timeout, delayed trigger, cancel while armed and repeated
arming are covered synthetically and with a controlled hardware stimulus.

## M3 — generator and remaining device workflows

### G01 — RF generator

Depends on P01–P04/C01. Add a dedicated generator state/operation module rather
than reusing analyzer input-mode booleans.

- [ ] Inventory Basic output paths and Ultra output modes, frequency/level
  bounds, step sizes, swept frequency/level and supported modulation variants.
- [ ] Add configure, explicitly enable, query and disable operations; opening
  a connection, identifying a unit or loading a host profile must not emit RF.
- [ ] Validate modulation type/depth/rate and sweep parameters per profile before
  switching modes. Record which limits cannot be remotely read back.
- [ ] Implement orderly shutdown/output-off attempts and report unknown state
  after disconnect; never promise software can turn off an unreachable device.
- [ ] Extend CLI/TUI with visible output state and a direct stop control.

Acceptance: command-order/failure tests plus frequency/level/modulation checks
using suitable external equipment and a documented terminated/attenuated setup.
Analyzer-only or no-signal tests cannot qualify this task.

### D01 — display capture and remote panel

Depends on P01/P03; use A05 metadata conventions.

- [ ] Implement profile-specific capture dimensions/pixel format and exact byte
  counts in a pure decoder; test truncated and oversized image responses.
- [ ] Export screenshots with firmware/settings provenance; keep image encoding
  separate from serial I/O and add a dependency only if justified.
- [ ] Implement supported refresh, color/display options and touch coordinates
  with bounds checks and known panel dimensions/orientation.
- [ ] Classify front-panel-only features explicitly. Touch navigation sequences
  are firmware-specific automation, not a replacement for a typed command API.

Acceptance: capture fixtures decode correctly; hardware screenshots match the
panel; malformed captures cannot desynchronize the following command.

### D02 — presets, traces and storage

Depends on P04/C01/A05.

- [ ] Separate host profiles, instrument preset slots and removable storage in
  types and UI. Inventory query/save/recall/delete support per firmware.
- [ ] Prevent accidental overwrite, path traversal and invalid names; use atomic
  replacement for host files and explicit intent for device persistence.
- [ ] Read back settings after recall and refresh capabilities/trace provenance
  if a preset changes mode. Recalled output state follows G01 policy.
- [ ] Implement SD trace/screenshot/file access only when a documented remote
  interface exists; otherwise supply the verified manual transfer procedure.

Acceptance: full storage, missing media, interrupted writes, wrong profile and
recall failures preserve existing data and expose actual resulting state.

### D03 — calibration and self-test

Depends on P01–P04 and G01 when calibration output is involved.

- [ ] Inventory read-only calibration/status queries separately from self-test,
  calibration-output enablement and persistent correction writes.
- [ ] Provide model-specific wiring/equipment instructions and explicit entry
  into mutating procedures; do not probe service commands automatically.
- [ ] Capture preconditions, firmware result codes, before/after status and
  operator observations; use staged progress with a defined abort procedure.
- [ ] Document recovery if interruption can leave calibration incomplete; never
  claim rollback where firmware provides none.

Acceptance: parser/state tests plus operator-performed calibration/self-test
on each supported revision. Skipped equipment or failed firmware results remain
visible; a mock success never constitutes calibration evidence.

### D04 — device information and maintenance

Depends on P01/P03/P04.

- [ ] Add typed supported battery, device ID, build, clock/RTC and diagnostic
  queries with unknown-field preservation and bounded output.
- [ ] Distinguish USB CDC settings from internal USART controls; implement only
  verified forms and explain changes that can break the active connection.
- [ ] Add explicit reset/reconnect lifecycle handling and fresh identification.
- [ ] Inventory bootloader/update capabilities; where remote updating is not
  supported, provide an exact manual workflow and classify the limitation.
  If implemented, verify target image/device matching and recovery prerequisites.

Acceptance: restart, re-enumeration, changed port names and incompatible firmware
are handled without silently reusing an old profile or repeating mutations.

## M4 — interfaces, tests and release qualification

### I01 — scriptable command interface

Start with P01 inspection; extend alongside each feature family.

- [ ] Separate CLI parsing from the small startup entry point; keep ordinary TUI
  startup compatible. Provide real `--help`/`--version` that never initialize a
  terminal, enumerate/open a port or start an RF operation.
- [ ] Add discover/inspect/acquire first, then typed analyzer, generator, capture
  and preset subcommands as their library APIs arrive.
- [ ] Define explicit model/port selection, machine-readable output, stable exit
  categories and stdout/stderr separation; noninteractive mode must never prompt.
- [ ] Return failures for acquisition/export/recovery errors and propagate
  cancellation/interrupt status. Validate config before opening output.

Acceptance: subprocess tests cover invalid arguments, no terminal, missing port,
unknown profile, timeout, cancellation and successful machine-readable output.

### I02 — capability-aware terminal UI

Depends incrementally on P01/P02 and each exposed feature.

- [ ] Display model/build/connection state and derive field limits/choices from
  capabilities. Explain unavailable features rather than offering illegal values.
- [ ] Add rescan and return-to-settings without restarting, live plots, marker
  controls and appropriate analyzer/generator/preset screens.
- [ ] Keep worker events/state updates outside rendering; add cancel, recovery,
  trigger-wait and reconnect states without blocking the event loop.
- [ ] Label MHz/dBm/time axes, selected path, corrections, output state and dropped
  frames. Keep rendering bounded on small terminals and long acquisitions.

Acceptance: in-memory rendering/event tests plus keyboard testing on real
terminals; every implemented end-user feature has a documented access path.

### I03 — public library and compatibility policy

Depends on stable P02–P04 interfaces, then evolves with feature slices.

- [ ] Expose session/profile/trace/error types with Rustdoc for units, invariants,
  side effects, cancellation and restoration guarantees.
- [ ] Keep native serial and TUI optional; prove core-only downstream builds and
  caller-provided streams remain usable.
- [ ] Document breaking changes and migration for config, exported metadata and
  library calls. Add small downstream examples for Basic and Ultra inspection
  and acquisition rather than speculative framework abstractions.

Acceptance: MSRV and feature matrix pass; examples build independently and public
APIs do not leak terminal types into device/protocol layers.

### Q01 — feature contracts, fixtures and fault coverage

Continuous task accompanying each feature.

- [ ] Add stateful mock behavior only for implemented commands; unsupported forms
  continue to reject rather than return universal `ok` responses.
- [ ] Store approved bounded real transcripts with model/build/date/settings,
  source provenance and byte-exact binary fixtures; redact identifying details
  without altering protocol behavior.
- [ ] Replay identical feature contracts against mocks, captured transcripts and
  explicitly enabled hardware sessions. Label their evidence levels separately.
- [ ] Add delayed responses, interrupted/partial writes, buffer limits, disconnect
  at each mutation, rejected settings and recovery failures.
- [ ] Use property/exhaustive boundary tests for pure codecs where valuable;
  never exclude production code to improve the reported coverage percentage.

Acceptance: each capability-ledger row links to a relevant test, and simulation
tests assert semantic outcomes as well as the actual command sequence.

### Q02 — hardware, host and distribution matrix

Depends on H01 and the feature being qualified.

- [ ] Maintain rows for exact Basic/Ultra revision + firmware + host OS + transport
  + build/artifact version, with pass/fail/skipped/unverified per feature group.
- [ ] Execute no-signal checks first; add calibrated/reference measurements for
  frequency, amplitude, processing, triggers and generator as equipment permits.
- [ ] Measure tolerances from the chosen instrument specification and reference
  uncertainty before running tests; do not retrofit thresholds to observed data.
- [ ] Test permissions, multiple devices, sleep/wake, USB unplug, reconnect and
  extended acquisition memory/runtime behavior on declared platforms.
- [ ] Verify packaged binaries as well as source builds; reconcile supported
  targets with `rust-release.toml` and [release guidance](docs/releases.md).
- [ ] Investigate the Jenkinsfile's documented FreeBSD release-startup SIGILL;
  obtain a reproducible trace/build comparison before declaring that runtime
  supported or making its diagnostic smoke stage a required gate.
- [ ] Keep local verification, physical-device runs, Jenkins execution and
  publication evidence separate. Do not infer runtime support from compilation.

Acceptance: every claimed support combination has retained results and no
unexplained critical failure; unavailable hardware/platforms remain unverified.

### Q03 — final feature audit and support declaration

Depends on all baseline tasks for the declared profiles.

- [ ] Reconcile every P01 command/menu entry with implementation, interface,
  regression tests, hardware evidence and any remote-control limitation.
- [ ] Resolve unclassified entries and stale compatibility/README claims.
  Maintain separate Basic and Ultra support tables, including firmware ranges.
- [ ] Record residual limitations with consequences and evidence needed to
  remove them; do not close tasks using vague future-work notes.
- [ ] Produce a release acceptance record linking artifacts and measurements,
  migration notes and known issues. Publishing is a separate requested action.

Acceptance: no open remote-capable feature in the declared completeness scope;
any front-panel/upstream limitation is explicit in the public support statement.

## Optional stretch goals — choose individually

These add host-side capabilities beyond device feature parity. None is selected
or required for M0–M4. Size is relative implementation effort, not a delivery
estimate: M = a contained feature with several components; L = multiple feature
slices; XL = an additional subsystem. Pick by ID, for example `S01, S03, S06`.

| Pick | ID | Feature | Depends on | Size |
| --- | --- | --- | --- | --- |
| [ ] | S01 | Waterfall, history and offline replay | A05, I02 | L |
| [ ] | S02 | Scheduled unattended measurement sessions | P03/P04, A05, I01 | L |
| [ ] | S03 | Spectrum masks, alarms and change detection | A04/A05/A06 | M |
| [ ] | S04 | Occupancy and channel-power analysis | A01/A03/A04/A05 | L |
| [ ] | S05 | Wide-span segmented survey planner | P02/P04, A01/A02/A05 | L |
| [ ] | S06 | Measurement comparison and report bundles | A04/A05, D01 | M |
| [ ] | S07 | Local web dashboard and remote-control service | P04, I01/I03 | XL |
| [ ] | S08 | Multiple instruments and comparison sessions | P04, A05, I03 | XL |
| [ ] | S09 | Generator/analyzer characterization recipes | G01, A04/A05, P04 | XL |
| [ ] | S10 | Python analysis bindings | I03, A05 | L |
| [ ] | S11 | Guided overload and measurement-quality assistant | A02–A05, Q02 | L |
| [ ] | S12 | Additional export formats and integrations | A05, I01 | M |

### S01 — waterfall, history and offline replay

- [ ] Define a versioned recording containing timestamped traces and immutable
  settings snapshots; use a bounded ring buffer and configurable disk retention.
- [ ] Render waterfall/time-history with pause, zoom and color scale; replay
  recordings through the same view without opening a device.
- [ ] Test missing frames, changing axes, bounded memory, truncated recordings
  and replay speed. Acceptance: a recorded session reproduces its trace order,
  timing and settings; gaps remain visible rather than interpolated silently.

### S02 — scheduled unattended sessions

- [ ] Define recipes with profile, span, interval, duration, output retention,
  retry limits and collision policy; use monotonic time for intervals.
- [ ] Run one owner per connection, checkpoint progress, detect disk exhaustion,
  and recover only safe analyzer operations after reconnect.
- [ ] Integrate through the headless CLI before adding a scheduler UI. Test clock
  jumps, missed intervals and process restart. Acceptance: bounded, resumable
  sessions report missed measurements and do not reactivate RF output.

### S03 — spectrum masks and alerts

- [ ] Represent frequency-dependent limits with units, interpolation, hysteresis,
  debounce and an explicit response to missing/out-of-range samples.
- [ ] Compare compatible traces against masks or saved baselines and retain the
  triggering trace plus settings as evidence.
- [ ] Start with local events/files; outbound notifications require a selected
  integration. Acceptance: synthetic edge cases verify no alert chatter, while
  dropped/incompatible data creates uncertainty rather than a false pass.

### S04 — occupancy and channel-power analysis

- [ ] Define channel plans, level thresholds, observation duration and denominators
  that account for scan gaps and unequal dwell time.
- [ ] Implement linear-power integration only with defensible bandwidth/sample
  assumptions; report RBW, detector and uncertainty with every result.
- [ ] Test analytic synthetic spectra and controlled signals. Acceptance: label
  sweep-based estimates explicitly; do not claim simultaneous real-time capture,
  demodulation or IQ information that the source trace does not contain.

### S05 — segmented surveys

- [ ] Plan overlapping segments constrained by model/path/RBW/point limits;
  estimate duration and show any required physical connector change.
- [ ] Preserve each segment's settings and timing; define overlap/deduplication
  and level-discontinuity handling before constructing a combined trace.
- [ ] Test cancellation/resume and segment-boundary peaks. Acceptance: no missing
  frequencies or hidden cross-path correction; never assume software can switch
  a Basic cable between physical connectors.

### S06 — comparison and reproducible reports

- [ ] Compare recordings only after checking axes, RF path, correction and
  bandwidth compatibility; make interpolation explicit and retain originals.
- [ ] Generate a portable HTML report first: plots, settings, identity, reference
  setup, uncertainty, pass/fail criteria and links to original trace data.
- [ ] Add deterministic fixtures for differences and missing metadata.
  Acceptance: another user can recompute the comparison from the exported bundle.

### S07 — local web dashboard and control service

- [ ] Wrap the typed session API with one device owner, bounded requests and
  streamed telemetry; begin bound to localhost with view-only operation.
- [ ] Add explicit control ownership, authentication for network exposure and
  operation authorization; never expose arbitrary shell/serial command execution.
- [ ] Provide cancellation and disconnect behavior matching the native client.
  Acceptance: concurrent browsers cannot interleave commands, exhaust queues or
  leave output state misrepresented. Deployment is a separate scope decision.

### S08 — multiple instruments

- [ ] Maintain independent sessions/capabilities and stable device selection;
  record host/device timestamps and measured scheduling skew.
- [ ] Coordinate start/stop and comparison while isolating a failed unit; model
  hardware synchronization only where an actual supported interface exists.
- [ ] Test two mocks with different timing plus paired hardware.
  Acceptance: do not label host-scheduled USB scans phase-coherent or simultaneous.

### S09 — characterization recipes

- [ ] Define typed generator/analyzer steps, expected wiring, settling, source
  limits and a dry-run plan; require explicit output enablement at execution.
- [ ] Capture scalar gain/loss versus frequency with fixture/reference corrections
  and preserve raw measurements. Do not label scalar data vector S-parameters.
- [ ] Add stop/recovery for failure on either instrument and operator checkpoints
  for cabling. Acceptance: reference fixtures reproduce known scalar behavior
  within stated uncertainty; disconnected output state is reported as unknown.

### S10 — Python analysis bindings

- [ ] Select a concrete use case first; compare invoking the stable CLI with a
  native binding before adding packaging and interpreter dependencies.
- [ ] Expose profiles, traces, metadata and structured exceptions; document
  ownership/cancellation if direct device access is included.
- [ ] Test wheels/imports on chosen platforms and version the binding contract.
  Acceptance: notebooks can inspect/replay/analyze traces without hidden device
  access and without requiring terminal dependencies.

### S11 — measurement-quality guidance

- [ ] Detect suspicious overload/image patterns using documented heuristics and
  explain confidence; never equate a heuristic with calibrated diagnosis.
- [ ] Offer guided attenuation/span/RBW comparison steps with operator approval
  for physical changes, keeping original measurements and settings.
- [ ] Validate suggestions against reference setups and known false positives.
  Acceptance: advice is tied to recorded evidence and cannot silently alter
  calibration, RF output or user-selected pass/fail criteria.

### S12 — additional exports and integrations

- [ ] Choose actual consumers and formats before adding dependencies: versioned
  JSON/JSONL, analysis-tool imports, compressed archives or selected databases.
- [ ] Serialize the common A05 trace model with units, identity, corrections and
  completion status; stream large datasets and document precision loss.
- [ ] Add round-trip/schema compatibility tests and interrupted-write recovery.
  Acceptance: data retains provenance across formats; external publication or
  account writes require a separately selected integration and destination.

## Suggested first implementation slice

Choose H01 + P01 first: identify before mutation, preserve failure reports,
capture `help`, and create the per-firmware capability ledger. Then implement
one end-to-end M1 slice: verified Basic/Ultra profile, one-shot acquisition,
bounded cancellation/recovery, minimal inspect/acquire CLI and its hardware test.
After that, prioritize analyzer tasks or select stretch goals by ID; selecting a
stretch goal brings its listed prerequisites into scope but does not select the
other stretch goals.
