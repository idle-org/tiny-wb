# tinySA and tinySA Ultra field guide

This is the instrument-side reference for tiny-wb maintainers and users. It
summarizes the official documentation needed to identify, protect, configure,
and interpret the supported hardware. It does not replace the safety labels on
the device, and limits are not targets.

## Names and model identification

The product name is **tinySA** (tiny spectrum analyzer). The larger device is
**tinySA Ultra**—not “tinySAultra”—and newer variants are **tinySA Ultra+**.
Firmware and source code often use hardware-family names:

| Product | How to identify it | Firmware/source shorthand |
| --- | --- | --- |
| tinySA Basic | 2.8-inch, 320×240 display; separate `LOW` and `HIGH` SMA ports | tinySA / `TINYSA3` |
| tinySA Ultra ZS405 | Front says `Ultra`; 4-inch, 480×320 display | tinySA4 / `TINYSA4` |
| tinySA Ultra+ ZS406 | Front says `Ultra+`; rear label says ZS406 | tinySA4 / `TINYSA4` |
| tinySA Ultra+ ZS407 | Front says `Ultra+`; rear label says ZS407 | tinySA4 / `TINYSA4` |

“Ultra mode” is also the name of a high-frequency measurement mode on Ultra
hardware. Do not confuse the physical Ultra model with whether Ultra mode is
currently enabled.

Counterfeits exist. Prefer the sellers listed by the official project. The
official first-use guidance recommends authentic firmware and the built-in
self-test as checks; a clone can be electrically unsafe or behave differently
at the console.

## What the instrument is

A tinySA is a swept, scalar spectrum analyzer and a signal generator. In
analyzer mode it steps a local oscillator across a range, passes the selected
mixing product through a resolution filter, measures power, and reports level
at each step. It measures magnitude versus frequency, not phase, and it is not
a real-time analyzer. Signals shorter than the time spent away from their
frequency may be missed.

In generator mode the RF path is reconfigured to produce an output. Never
assume an SMA connector is an input merely because it was one earlier: confirm
the mode before connecting a device under test.

## Model capability summary

Values below summarize the official model comparison and specification pages.
Some limits depend on firmware, mode, frequency, attenuation, LNA state, and
hardware revision.

| Capability | Basic | Ultra ZS405 | Ultra+ ZS406 | Ultra+ ZS407 |
| --- | --- | --- | --- | --- |
| Display | 2.8 in, 320×240 | 4 in, 480×320 | same | same |
| Normal analyzer range | low 100 kHz–350 MHz; high 240–960 MHz | 100 kHz–800 MHz | 100 kHz–900 MHz | 100 kHz–900 MHz |
| Extended analyzer range | none | Ultra mode to about 5.3 GHz; calibrated to 6 GHz | Ultra mode to about 5.4 GHz; calibrated to 6 GHz | Ultra mode and calibrated range to 7.3 GHz |
| Harmonic observation | none | about 10 GHz | about 10 GHz | about 11 GHz |
| RBW selections | roughly 3–600 kHz | 0.2–850 kHz | same | same |
| Maximum displayed points | 290 | 450 | 450 | 450 |
| Step attenuator | 0–31 dB on low path | 0–31 dB when LNA is off | same | same |
| Internal LNA | none on standard low path | nominal 20 dB to 4 GHz | same | 20 dB to 7.3 GHz, about 15 dB to 9 GHz |
| Sine generator | 100 kHz–350 MHz | 100 kHz–800 MHz | 100 kHz–900 MHz | 100 kHz–900 MHz |
| Higher-frequency generator | square 240–960 MHz | square to 4.4 GHz / RF test to 5.3 GHz | square to 4.4 GHz / RF test to 5.4 GHz | square around 6 GHz / RF test to 7.3 GHz |
| Storage/audio | no microSD; audio requires modification | microSD up to 32 GB; headphone output | same | same |

The official Ultra specification lists ±2 dB absolute level accuracy after
level calibration over its main calibrated region, detector resolution of
0.5 dB, more than 1,000 points/s with the widest filters, and selectable
200 Hz, 1, 3, 10, 30, 100, 300, 600, and 850 kHz filters. Those headline values
do not mean every signal at every frequency is measured to ±2 dB: mismatch,
compression, images, correction tables, RBW, battery voltage, and Ultra-mode
limitations still apply.

## RF architecture

### tinySA Basic

The Basic exposes two distinct bidirectional RF ports and four broad modes:

- **Low input:** the higher-quality 100 kHz–350 MHz analyzer path with a
  0–31 dB step attenuator and input filtering.
- **High input:** a 240–960 MHz path with frequency-dependent impedance, no
  input band filter, no adjustable step attenuator, lower intercept point, and
  greater susceptibility to overload, images, and out-of-band signals.
- **Low output:** a filtered sine source over roughly 100 kHz–350 MHz.
- **High output:** a square-wave source over roughly 240–960 MHz.

The 240–350 MHz overlap is useful: compare paths when a result is suspicious.
The low path is generally preferable where it covers the frequency.

### tinySA Ultra and Ultra+

Ultra uses one main RF SMA path and a separate calibration/output connector.
In analyzer mode the signal path is conceptually:

```text
RF SMA
  -> select LNA or 0–31 dB attenuator
  -> select 800 MHz low-pass filter, bypass, or direct path
  -> swept first LO + mixer
  -> ~980 MHz high-IF band-pass filter
  -> receiver / second mixer
  -> ~870 kHz low IF
  -> selectable RBW filter
  -> power detector
```

The low-pass filter suppresses aliases in the normal range. In Ultra mode it
is bypassed above its useful passband, allowing higher-frequency mixing but
also allowing many more images and spurs. Firmware makes multiple measurements
with shifted conditions to reject candidates that do not behave like a real
input signal.

In generator mode a nominal 980 MHz source, filter/mixer network, switching,
and attenuation are reused to create the requested output. Above the normal
sine range the user trades spectral cleanliness, frequency granularity, level
accuracy, and maximum frequency between “cleanest” square-wave and “highest
accuracy” RF-test modes.

The CAL source derives 30, 15, 10, 4, 3, 2, or 1 MHz from a temperature-
controlled 30 MHz oscillator. It is a useful internal reference, not a general
laboratory-grade calibrated generator across the analyzer's entire range.

## Ultra mode and harmonic mode

Ultra mode bypasses the input low-pass filter when needed and applies a
multi-measurement mirror/spur-removal algorithm. On the ZS405, the filter is
normally retained below roughly 750 MHz. Above that point scans slow down.

It works best for stable, narrowband signals. Important failure modes are:

- short bursts may be absent because repeated observations do not coincide;
- swept or hopping signals may be rejected or placed incorrectly;
- complex or wideband signals (official guidance flags roughly 600 kHz–1 MHz
  width, depending on the page/firmware) can create false results;
- sensitivity deteriorates with frequency;
- bypassing the filter increases LO leakage at the RF connector, reported as
  high as approximately −10 dBm;
- results beyond the level-calibrated range lack trustworthy
  frequency-dependent amplitude correction.

A band-limited antenna or external preselector can reject unwanted mixing
products before they enter the analyzer. This is often better than asking the
algorithm to disambiguate every possible image.

Harmonic mode extends observation beyond the calibrated Ultra range by mixing
on a selected LO harmonic. Treat it as signal discovery, not calibrated level
metrology. The official best-settings guidance says harmonic selection should
normally remain at 3 and key expert settings should remain automatic.

## Safe operation: read before connecting a signal

Absolute maximum ratings prevent immediate damage; they do not guarantee
linearity or accurate measurement. Use the most conservative applicable limit.

### Core rules

- Keep Ultra/Ultra+ below **+6 dBm at 0 dB internal attenuation**. Basic's
  published absolute RF limit at 0 dB attenuation is **+10 dBm**. The official
  recommendation for automatic attenuation is no more than **0 dBm**, and best
  accuracy is generally obtained with signal level below about **−25 dBm** at
  the analyzer input stage.
- The latest official safe-operation table says to use a **DC block plus at
  least 3 dB external attenuation above 5 V DC offset**. When the DC content is
  unknown, use a suitably rated DC block.
- Do not trust the internal attenuator as the sole protection from a powerful
  source. Place external attenuation directly at the analyzer so a failed
  cable or incorrect mode cannot expose the input.
- Never connect a transmitter directly. The official minimum examples are
  40 dB attenuation for 1 W, 50 dB for 10 W, and 60 dB for 100 W. Real setups
  must also account for peak/envelope power, attenuator power rating, mismatch,
  transients, and harmonics.
- Avoid the supplied telescopic antenna where static discharge or a nearby
  transmitter is possible. Do not touch objects with it.
- After Basic self-test/calibration, disconnect the cable between LOW and HIGH
  before enabling an output. Driving a connected input can destroy the input
  attenuator.
- Confirm whether the RF port is in analyzer or generator mode before every
  connection change.

### A practical transmitter setup

Use a rated dummy load or directional/attenuating sampler, followed by
power-rated coarse attenuation, then a sacrificial fixed attenuator (the
official example uses 30 dB) immediately at the tinySA. Calculate the expected
level in dBm and leave margin for modulation peaks. For harmonic measurements,
an external high-pass/low-pass/band-pass filter may be required to prevent the
fundamental from generating analyzer distortion that looks like DUT harmonics.

## Measurement controls and trade-offs

### Frequency span and sample points

Displayed or requested points are not identical to analog resolution. Point
spacing is approximately span divided by the number of intervals. Firmware may
insert hidden measurements so the RBW covers the span without gaps. More
points improve trace sampling but increase time and do not substitute for a
narrower RBW.

### Resolution bandwidth (RBW)

RBW is the bandwidth of the measurement filter:

- narrower RBW separates nearby carriers and lowers displayed noise;
- narrower RBW takes longer, particularly below 30 kHz;
- wider RBW scans quickly and captures more energy from modulated signals, but
  merges close signals and raises the displayed noise floor.

Auto RBW is a good default. A wide span with a narrow fixed RBW can take a very
long time because firmware adds steps to maintain coverage.

### Attenuation and overload diagnosis

Internal attenuation trades sensitivity for headroom. Automatic mode aims to
keep the strongest signal at a safe internal level. To test whether a harmonic
or spur is generated inside the analyzer, increase external or internal
attenuation:

- a real input signal, after compensation, should remain near the same
  reported level;
- an internally generated intermodulation product often falls or disappears
  disproportionately;
- the noise floor rises because attenuation reduces signal before the
  receiver.

The Basic high path has no normal adjustable attenuator and is easy to
overload. Strong out-of-band signals also reach it because it lacks an input
band filter.

### LNA

Ultra's LNA improves sensitivity for weak signals but disables the step
attenuator and greatly reduces overload headroom. Do not enable it around
unknown or strong signals. The model comparison reports a roughly −26.5 dBm
1 dB compression level with the LNA on for the ZS405, versus about +0.5 dBm
with it off. If a trace changes unexpectedly when the LNA is toggled, suspect
compression or intermodulation before assuming a real RF change.

### Repeat and averaging

The `repeat` console setting takes multiple measurements at each frequency and
reduces random noise at the cost of proportional scan time. Trace calculations
such as min hold, max hold, and averaging are separate firmware features. For
bursty signals, max hold across repeated sweeps is often more informative than
per-point averaging.

### Spur and image checks

When a line is questionable:

1. change attenuation and see whether its corrected amplitude behaves like the
   wanted signal;
2. change span/RBW and repeat the sweep;
3. switch the Basic path in the overlap region or use below/above-IF controls;
4. enable automatic spur removal;
5. add an external preselector or filter;
6. compare against a known source or another analyzer.

Remember that the instrument's own 48 MHz MCU clock and harmonics can be
visible, especially with an antenna or low RBW.

## First use, calibration, and health checks

1. Charge the device. Ultra RF performance can degrade at low supply voltage;
   official guidance recommends more than 3.9 V for best performance.
2. Record the hardware label and current `version`/`info` output.
3. If updating firmware, first run the self-test so pre-existing damage is not
   confused with an update problem.
4. For the Basic self-test, connect LOW and HIGH with the supplied SMA cable
   and run `CONFIG -> SELF TEST`.
5. Perform input level calibration as directed by the on-device menu. Basic low
   level calibration uses the two ports connected; high-path calibration is a
   separate procedure. Ultra correction/calibration varies by range and model.
6. Disconnect any calibration loop before enabling generator output.

Calibration corrects systematic level error; it cannot remove front-end
overload, RF mismatch, an image, a damaged switch, insufficient shielding, or
an invalid Ultra-mode measurement.

## Firmware update and recovery

Use the firmware file for the exact family:

- Basic: the `tinySA/DFU` tree;
- Ultra/Ultra+: the `tinySA4/DFU` tree.

Do not flash a similarly named file from the other family. Configuration data
may be reset or become structurally incompatible, so record unusual correction
settings and expect to redo calibration when appropriate.

Ultra enters recovery/DFU mode by holding the jog control while powering on;
the screen remains black. The Basic normally enters DFU from its menu, with a
documented PCB BOOT0 recovery procedure if an update is interrupted. The
official guide supports `dfu-util` and vendor tools. After updating, power-cycle
the unit, verify normal USB serial enumeration, check the version, clear stale
configuration when advised, and rerun self-test.

## Settings baseline for Ultra

For general use, leave these expert controls automatic unless a documented
experiment requires otherwise: spur removal, IF frequency, mixer drive, scan
speed, and below-IF choice. Keep harmonic at 3. The official best-settings page
also warns against casual AGC/LNA expert changes and against its then-current
FAST sweep mode. Because firmware evolves, confirm that advice against the
version installed on the instrument.

## How this maps to tiny-wb

tiny-wb detects Basic/Ultra families and applies model-specific input commands
and native encoding offsets. Generic Ultra support covers 0.1–800 MHz; a
detected ZS406 covers normal mode through 900 MHz and explicit opt-in Ultra mode
through 5.4 GHz. Harmonic reception, ZS407 extended mode and other extended
ranges remain unsupported; RF accuracy qualification remains pending. See
[support scope](ultra-support.md) and the [compatibility matrix](tiny-wb-operations.md#compatibility-matrix).

For the wire format and exact command behavior, continue with
[USB console and `scanraw` protocol](usb-protocol.md).
