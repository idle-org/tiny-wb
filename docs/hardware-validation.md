# tinySA hardware validation checklist

This checklist qualifies one physical instrument and firmware build. A passing
mock or CI run is not hardware evidence. Run the validator once with a tinySA
Basic and once with each supported tinySA Ultra profile; keep reports separate.

## Before each run

- [ ] Identify the rear-label model and record the firmware shown by the device.
- [ ] Disconnect every RF source. This test needs only the analyzer and a
  data-capable USB cable.
- [ ] Close serial terminals and other tinySA software. Confirm the current user
  can open the USB serial port without running the validator as root.
- [ ] If the front-panel configuration matters, save a preset. The validator
  changes repeat, raw zero offset, input mode, sweep limits, and pause state.
  The ZS406 profile also enables Ultra mode, which firmware may retain.

From the repository root, run exactly one command:

```console
scripts/test-hardware.sh basic
```

For an Ultra, use:

```console
scripts/test-hardware.sh ultra
```

For the ZS406 normal and non-harmonic Ultra ranges, use:

```console
scripts/test-hardware.sh zs406
```

The script asks when to unplug and plug the instrument so it can detect the new
serial port. If discovery is ambiguous, choose from the displayed ports. To skip
discovery, pass `--port /dev/ttyACM0` or `--port COM7`. It builds the validator,
runs two 101-point scans, validates the returned values, writes CSV files and an
identity report under ignored `hardware-validation/`, and asks whether the
front-panel sweep resumed.

## Basic acceptance

- [ ] The automatically selected port belongs to the Basic.
- [ ] Both `legacy` (240–300 MHz low input) and `direct` (350–400 MHz high input)
  scans report `PASS` and create CSV files with 101 data rows plus a units header.
- [ ] The printed `version`/`info` text matches the connected Basic and its
  recorded firmware.
- [ ] The live front-panel sweep resumes after the scans; answer `y` only after
  checking it.
- [ ] The final `basic-report.txt` records both CSV paths and
  `operator_confirmed=y` (or `yes`).
- [ ] Each report entry records `raw_level_offset_db=128`.

### Recorded Basic evidence

The Basic acquisition path was validated on physical hardware on 2026-10-05
with firmware `tinySA_v1.4-175-g1419a93`. Both the 240–300 MHz low-input scan
and the 350–400 MHz high-input scan returned 101 valid samples, exercised the
fixed-record binary decoder, produced CSV output, and resumed the live sweep.
This is communications and workflow evidence, not frequency or level-accuracy
calibration.

## Ultra acceptance and limit

- [ ] The automatically selected port belongs to the Ultra.
- [ ] Both `legacy` and `direct` scans report `PASS` at 240–300 MHz and create
  CSV files with 101 data rows plus a units header.
- [ ] The printed `version`/`info` text identifies the exact connected Ultra
  variant and firmware.
- [ ] The live front-panel sweep resumes after the scans; answer `y` only after
  checking it.
- [ ] The final `ultra-report.txt` records both CSV paths and
  `operator_confirmed=y` (or `yes`).
- [ ] Each report entry records `raw_level_offset_db=174`.

A passing Ultra run validates only the shared low-input command path below
350 MHz, including the native `zero 174` encoding, binary framing, CSV export,
and best-effort
resume. It does not validate Ultra mode, harmonic mode, LNA behavior, ranges at
or above 350 MHz, level accuracy, or frequency accuracy. Those require
model-aware acquisition and a controlled reference source; do not infer them
from this communications test.

## ZS406 range acceptance

- [ ] The rear label says ZS406 and identity reports `ZS406` or hardware V0.4.6.
- [ ] `normal` (0.1–900 MHz) and `ultra` (900 MHz–5.4 GHz) each report `PASS`
  and create 101-row CSV files with units headers.
- [ ] The report records `enable_ultra_mode=false` for `normal`, `true` for
  `ultra`, and `raw_level_offset_db=174` for both.
- [ ] The live front-panel sweep resumes and the final `zs406-report.txt` records
  affirmative operator confirmation.

This validates identity, accepted sweep bounds, command ordering, binary
framing and finite decoded values across both ranges. With no controlled RF
source it does not establish frequency accuracy, level accuracy, sensitivity,
spur rejection or the authenticity of displayed signals.

### Recorded ZS406 evidence

The ZS406 acquisition profile was validated on physical hardware on 2026-10-07
with hardware V0.4.6 and firmware `tinySA4_v1.4-197-gaa78ccc`. The 100 kHz–900
MHz normal scan and 900 MHz–5.4 GHz Ultra scan each returned 101 finite samples,
confirmed the requested sweep boundaries, produced CSV output, and resumed the
front-panel sweep. The RF input was unconnected. This is communications,
range-acceptance and workflow evidence, not RF frequency or level calibration.

## Failure record

- [ ] Keep the generated report and any completed CSV; do not edit them.
- [ ] Record the command, host OS, selected port, exact model, firmware, and the
  complete `FAIL:` line.
- [ ] If the sweep did not resume, disconnect any RF setup and use `resume`,
  reload the saved preset, or power-cycle the instrument.
- [ ] Do not connect a transmitter merely to reproduce a communications failure.
