# Upstream source catalogue

This catalogue records the authoritative references used to build the local
guide. It was last checked on **2026-10-04**. Prefer these primary sources over
retailer listings, reposted manuals, videos, and forum recollections.

## Official overview and model data

- [tinySA official wiki home](https://www.tinysa.org/wiki/) — product-family
  overview and navigation.
- [Model comparison](https://www.tinysa.org/wiki/pmwiki.php?n=TinySA4.Comparison)
  — identify ZS405/ZS406/ZS407 and compare frequency, LNA, RBW, scan, generator,
  storage, and phase-noise capabilities.
- [tinySA Basic specification](https://tinysa.org/wiki/pmwiki.php?n=Main.Specification)
  — low/high input and output paths, level limits, filters, speed, and battery.
- [tinySA Ultra/Ultra+ specification](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.Specification)
  — analyzer/generator ranges, attenuation, LNA, filters, accuracy, and hardware
  revisions.

## Architecture, limitations, and operation

- [Ultra technical description](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.TechnicalDescription)
  — mixer, IF, filter, detector, generator, and calibration-source signal paths.
- [Ultra mode](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.Ultra) — low-pass
  bypass, mirror/spur elimination, sensitivity and bandwidth limitations, LO
  leakage, and unlock behavior.
- [Ultra limitations](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.Limitations)
  — RBW, speed, images, low-frequency sensitivity, LNA overload, and 48 MHz MCU
  radiation.
- [Ultra menu tree](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.MenuTree) —
  front-panel settings, presets, SD, sweep controls, calibration, expert
  settings, and generator modes.
- [Ultra best-performance settings](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.BestSettings)
  — recommended automatic expert controls and cautions.
- [RBW](https://tinysa.org/wiki/pmwiki.php?n=Main.RBW) — coverage, noise-floor,
  resolution, and sweep-time trade-offs.
- [Attenuation](https://www.tinysa.org/wiki/pmwiki.php?n=Main.ATTENUATION) —
  protection and overload/spur diagnosis.

## Safety, first use, and maintenance

- [Safe operation](https://tinysa.org/wiki/pmwiki.php?n=Main.SafeOperation) —
  current DC-block, transmitter attenuation, and static precautions.
- [First use](https://tinysa.org/wiki/pmwiki.php?n=Main.FirstUse) — charging,
  authenticity, self-test, calibration, and connection warnings.
- [Safe transmitter measurement](https://tinysa.org/wiki/pmwiki.php?n=Main.Transmitters)
  — dBm examples and external attenuation strategy.
- [Firmware update](https://tinysa.org/wiki/pmwiki.php?n=Main.UpdatingTheFirmware)
  — correct download trees, DFU tools/modes, recovery, and post-update steps.
- [Ultra battery guidance](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.Battery)
  — charging current and the voltage needed for best RF performance.

## USB protocol and firmware implementation

- [Official USB interface](https://tinysa.org/wiki/pmwiki.php?n=Main.USBInterface)
  — console syntax, command descriptions, `scan`, `scanraw`, framing, options,
  and documented raw offsets.
- [Ultra console command example](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.ConsoleCommands)
  — Ultra build command inventory and example outputs; treat it as a snapshot,
  not a promise for every firmware.
- [Upstream tinySA firmware repository](https://github.com/Ho-Ro/tinySA) —
  implementation authority for a matching firmware revision.
- [`sa_cmd.c` raw source](https://raw.githubusercontent.com/Ho-Ro/tinySA/main/sa_cmd.c)
  — mode parsing, `zero`, RBW, Ultra commands, and exact `scanraw` byte order and
  step calculation.
- [`main.c` raw source](https://raw.githubusercontent.com/Ho-Ro/tinySA/main/main.c)
  — shell command registration, prompt behavior context, and build-conditional
  command availability.

## Source-conflict policy

When sources disagree:

1. protect hardware using the most conservative official safety value;
2. use the comparison/specification page for the exact physical revision;
3. use on-device `version`, `info`, and `help` for the installed build;
4. use source at the matching firmware revision for byte-level behavior;
5. verify ambiguous scan semantics with a low-level known signal and preserve
   the transcript as a regression fixture.

The local docs intentionally record two known conflicts: the wiki's textual
“MSB LSB” description versus firmware's little-endian byte writes, and
endpoint-inclusive intuition versus the reviewed firmware's `span / points`
`scanraw` step. Both are explained in [the protocol guide](usb-protocol.md).
