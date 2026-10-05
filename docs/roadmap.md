# tinySA feature completion roadmap

For implementation subtasks, dependencies, acceptance gates and selectable
stretch goals, use the detailed [task backlog](../task.md). Shared task IDs
refer to the same features; this document is the concise overview.

This is the task list for reaching documented feature coverage on tinySA Basic
and tinySA Ultra. It is a plan, not a claim that either model is fully supported.
Reviewed 2026-10-04 against the official console/menu documentation and firmware
sources linked below. No physical instrument was available for this change.

“Complete” means every feature of an explicitly supported hardware/firmware
combination has either a tested library API and user interface or a documented
remote-control limitation. A feature present on the touchscreen is not necessarily
available over USB. Ultra+, hardware revisions and firmware build options must
be distinct capability profiles, not inferred from the name “Ultra”.

## Foundations delivered

- [x] F01: Build a Rust library without terminal/native serial dependencies;
  retain optional serial and existing default application builds.
- [x] F02: Share `Read + Write` framing across native serial, TCP and in-memory
  adapters; keep acquisition and export independent of UI/filesystem ownership.
- [x] F03: Add bounded, stateful mock Basic/Ultra profiles, deterministic raw
  samples, one-shot faults and a runnable TCP server example.
- [x] F04: Exercise production Basic acquisition over fragmented mock I/O and
  real loopback TCP, including malformed responses and resume attempts.

These foundations do not complete Ultra acquisition or firmware emulation.
See [library integration](library.md) and [mock scope](mock-device.md).

## Priority 0 — trustworthy model-aware acquisition

Initial family detection, native-offset readback and common Ultra normal-range
planning are implemented; see [support status](ultra-support.md). The broader
P01–P04 acceptance gates below remain open pending revision-specific profiles
and hardware qualification.

- [ ] P01 **Inventory and capability negotiation** (depends F01–F04). Capture
  `version`, `info`, `help` and matching firmware commit/build options for Basic
  and each supported Ultra revision. Create a versioned command/capability
  manifest, including argument forms, units, readback and unsupported features.
  Unknown firmware must expose uncertainty and permit explicit profile selection.
  Acceptance: recorded fixtures cover both families and unknown variants; no
  model selection based only on a shared substring or successful prompt.
- [ ] P02 **Separate model rules from raw framing** (P01). Replace Basic-only
  range/input validation with per-model input paths, ranges, zero offsets,
  sweep/point limits and frequency-axis semantics. Add Ultra acquisition without
  sending Basic `mode high input`. Acceptance: boundary tests for each declared
  profile, hardware scans below/above 960 MHz where supported, and independently
  verified frequency/dBm values. Do not use the mock's synthetic ceiling as evidence.
- [ ] P03 **Robust command sessions** (P01). Typed commands/replies, explicit
  rejection handling, bounded binary capture readers, deadlines, cancellation,
  reconnect and resynchronization; preserve context and partial results policy.
  Acceptance: echoed/un-echoed and fragmented real transcripts, timeout/EOF at
  each phase, stale prompts, rejected commands, and no silent partial success.
- [ ] P04 **State ownership and recovery** (P02–P03). Query and restore supported
  settings, model pause/resume and generator state explicitly, define exclusive
  session ownership. Acceptance: failure/cancellation tests for every mutation
  plus hardware readback; report when full restoration is unavailable.

## Priority 1 — spectrum analyzer controls and data

Each item depends on P01–P04. Implement library API first, then mock behavior,
CLI/TUI access, documentation and hardware acceptance fixtures in the same task.

- [ ] A01 **Frequency and sweeps:** start/stop, center/span, CW/zero span, point
  count, sweep time, continuous/single acquisition, repeat, frequency tables,
  external frequency reference/offset, wideband and multiband where exposed.
  Acceptance: requested versus actual frequencies and sweep completion verified.
- [ ] A02 **RF input chain:** per-model paths, attenuation/auto attenuation,
  LNA and other gain/AGC controls, Ultra/auto/harmonic mode, IF choices and
  overload state where queryable. Acceptance: legal combinations enforced by
  profile and actual device readback; explain expected RF measurement limitations.
- [ ] A03 **Bandwidth and processing:** RBW/automatic RBW, VBW where available,
  averaging, min/max hold, subtraction/reference traces, spur suppression,
  detector/calculation modes and reset behavior. Acceptance: trace provenance
  includes processing settings and corresponding firmware behavior is verified.
- [ ] A04 **Amplitude and units:** reference level, scale, displayed units,
  external gain/loss, level correction tables and offsets. Acceptance: documented
  conversion semantics and tests prevent double application of corrections.
- [ ] A05 **Trace acquisition/export:** ASCII scans, raw scans, trace/data and
  frequencies queries, multiple/stored traces, streaming/repeated scans and
  timestamped metadata. Acceptance: bounded memory/backpressure, aligned axes,
  round-trip datasets and explicit firmware/model/settings metadata.
- [ ] A06 **Markers and measurements:** enable/select/move, peak search/tracking,
  delta/reference markers and firmware-supported measurement readouts.
  Acceptance: typed units and host-computed versus firmware-computed provenance;
  values checked against the device display or matching trace.
- [ ] A07 **Triggers:** supported sources, level, polarity, position and
  single-shot/auto modes. Acceptance: trigger waiting can be cancelled and is
  distinguished from transport failure, with captured hardware trigger cases.

## Priority 2 — generator and complete instrument workflows

- [ ] G01 **RF generator** (P01–P04): supported output paths, CW/swept frequency,
  level/attenuation, output enable, level sweep and available modulation modes
  (AM/FM/narrow FM and other per-build modes). Acceptance: explicit output state,
  validated limits, predictable stop/disconnect handling and external measurement
  of supported settings. Never enable RF output as a probing side effect.
- [ ] D01 **Display/remote panel** (P03, A05): screenshot/capture decoding,
  display refresh, touch injection, remote navigation and color/trace settings
  where exposed. Acceptance: exact dimensions/pixel encoding per profile,
  truncated-image tests and documented controls unavailable through the console.
- [ ] D02 **Presets and storage** (P04): load/save/recall instrument presets,
  host profiles, saved traces/screenshots and SD features where remotely exposed.
  Acceptance: distinguish volatile state, instrument persistence and host files;
  prevent accidental overwrite and support versioned configuration migration.
- [ ] D03 **Calibration and self-test** (P01–P04): calibration output, self-test,
  calibration/correction readback and firmware-supported calibration workflows.
  Acceptance: guided prerequisites, explicit persistent changes, captured results
  and hardware verification; simulator results always identified as synthetic.
- [ ] D04 **Device management** (P03): battery, device identity, firmware/build,
  time/RTC, console/USART settings, reset and firmware-update/bootloader workflow
  if a documented safe interface exists. Acceptance: reconnect behavior tested;
  unsupported remote update is clearly documented rather than guessed.

## Priority 3 — access, integrations and qualification

- [ ] I01 **Headless command interface** (P02, A01–A07): scriptable discovery,
  inspect/configure/acquire/export commands, stable machine-readable errors and
  structured output. Acceptance: noninteractive workflows require no terminal,
  exit codes are tested, and long operations support cancellation.
- [ ] I02 **Capability-driven UI** (A01–D04 incrementally): model/firmware banner,
  units and supported control visibility, streaming plot, markers, generator and
  saved workflows. Acceptance: every implemented remotely accessible feature
  has a discoverable entry point; unavailable controls explain the reason.
- [ ] I03 **Library API lifecycle** (P02–P04): stable typed device/session facade,
  builder/configuration versioning, integration examples and a release policy.
  Decide on asynchronous adapters or foreign-language bindings only with actual
  integration requirements. Acceptance: downstream example crates compile on
  the MSRV and API compatibility is assessed before releases.
- [ ] Q01 **Expanded mock contracts** (each feature): add tested behavior only
  for newly implemented commands, transcript replay, delayed/stalled responses,
  noise fixtures and injected failures at transaction boundaries. Acceptance:
  shared contract tests exercise adapters and real recorded replies; unsupported
  commands continue to fail explicitly. Simulation does not certify RF accuracy.
- [ ] Q02 **Hardware and platform matrix** (all supported features): Basic and
  declared Ultra revisions/firmware, Linux/macOS/Windows serial operation,
  disconnects, permissions and extended acquisition. Acceptance: reproducible
  test reports with firmware identifiers and reference measurements, separate
  from mock coverage; Jenkins and packaged releases exercised on their hosts.
- [ ] Q03 **Completion audit** (all above): reconcile the capability manifest
  with the official console list and menu tree. For every entry record API,
  interface, tests, hardware evidence or precise remote-interface limitation.
  Acceptance: no unclassified feature or unsupported broad “all tinySA” claim.

## Coverage audit map

Use this map when splitting tasks into issues. Command names are discovery
candidates, not a universal command contract; firmware conditionals matter.

| Feature family | Console/menu evidence to inventory | Tasks |
| --- | --- | --- |
| Identity and build | `version`, `info`, `help`, `deviceid` | P01, D04 |
| Frequency/time | `freq`, `sweep`, `sweeptime`, `actual_freq`, `frequencies`, menu multiband | A01 |
| Input and gain | `mode`, `attenuate`, `agc`, `lna`, `lna2`, `ultra`, `if`, `if1` | A02 |
| Filtering/calculation | `rbw`, `calc`, `spur`, menu VBW | A03 |
| Level/units/corrections | `level`, `leveloffset`, `ext_gain`, `correction`, `zero`, display units | A04 |
| Data/traces | `scan`, `scanraw`, `data`, `trace`, `repeat`, `pause`, `resume` | A05 |
| Markers/triggers | `marker`, `trigger`, measurement menus | A06, A07 |
| Generator | output `mode`, `output`, `level`, `levelchange`, `modulation` | G01 |
| Display/touch | `capture`, `refresh`, `color`, `touch`, `touchcal`, `touchtest` | D01, D03 |
| Presets/files | `save`, `recall`, `load`, `saveconfig`, SD/menu storage | D02 |
| Calibration/service | `caloutput`, `selftest`, `test`, `dac`, `sweep_voltage`, `clearconfig` | D03 |
| Device lifecycle | `vbat`, `time`, `usart`, `reset`, `release`, build/debug diagnostics | D04 |

Front-panel-only features may require upstream firmware work. Record that as a
separate dependency after checking the matching source; touch emulation alone
is not a reliable typed API for every menu feature.

## Primary references

- [Official USB interface and commands](https://tinysa.org/wiki/pmwiki.php?n=Main.USBInterface).
- [Ultra console transcript](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.ConsoleCommands): historical snapshot, not a current universal schema.
- [Ultra menu tree](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.MenuTree): includes functions that may lack console access.
- [Basic specifications](https://tinysa.org/wiki/pmwiki.php?n=Main.Specification),
  [Ultra specifications](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.Specification),
  [Ultra hardware comparison](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.Comparison).
- [Upstream firmware](https://github.com/erikkaashoek/tinySA), especially
  [command handlers](https://github.com/erikkaashoek/tinySA/blob/main/sa_cmd.c)
  and [command registration](https://github.com/erikkaashoek/tinySA/blob/main/main.c).
  These links move; P01 must pin a commit matching each captured device firmware.
