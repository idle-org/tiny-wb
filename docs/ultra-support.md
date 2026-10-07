# Basic and Ultra acquisition support

Implemented 2026-10-05; synthetic verification is separate from hardware qualification.

The acquisition path reads `version`, `info`, and `help` before changing scan
settings. Recognized Basic (`tinySA_v…`) and Ultra-family (`tinySA4_v…`) identities
select different plans. Unknown or conflicting identities fail. Explicit
`scan.device_model = "basic"` or `"ultra"` requires a matching detected family;
the default is `"auto"`. Explicit identity text also distinguishes ZS405,
ZS406 and ZS407; an unrecognized revision retains the conservative family
fallback. Detection does not certify that arbitrary firmware implements every
command correctly.

| Family | Implemented acquisition range | Input command | Automatic raw bias |
| --- | --- | --- | --- |
| Basic | Existing low/high policy through 960 MHz | `mode low input` or `mode high input` | 128 dB |
| Ultra family without identified revision | Common normal range 0.1–800 MHz | `mode low input` | 174 dB |
| Ultra+ ZS406 | Normal 0.1–900 MHz; opt-in Ultra through 5.4 GHz | `mode low input`; `ultra on` above 900 MHz | 174 dB |

The [official comparison](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.Comparison)
lists 800 MHz normal range for ZS405 and 900 MHz for Ultra+ revisions. ZS406 is
recognized only from explicit `ZS406` or `HW Version:V0.4.6` evidence. Above
900 MHz, `scan.enable_ultra_mode = true` is required; acquisition sends
`ultra on` and verifies that `sweep` reads back the requested boundaries. The
client stops at 5.4 GHz before harmonic mixing. It does not send `saveconfig`,
but firmware may retain Ultra mode as device state. ZS407 extended mode,
harmonic mode, LNA control and complete mode-state restoration are not implemented.
The [official Ultra console snapshot](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.ConsoleCommands)
documents the family version prefix and zero readback. The reviewed
[mode handler](https://github.com/Ho-Ro/tinySA/blob/main/sa_cmd.c)
accepts `mode low input` for Ultra. Sources checked 2026-10-05.

Required commands must appear in `help`. Optional diagnostics run only when
advertised. After setting the selected bias with `zero N`, acquisition queries
`zero` and requires a matching `NdBm` readback. A missing or mismatched readback
fails before scanning. The decoder uses this confirmed value. Both families
share the same binary record layout; see [protocol](usb-protocol.md).

## Configuration migration

New defaults select `device_model = "auto"`, `auto_zero_offset = true`, and
240–800 MHz, usable by both families. Saved frequencies remain unchanged;
an old 960 MHz stop is rejected on Ultra with an actionable range error.
To preserve a custom raw bias, set `auto_zero_offset = false` alongside the
existing `zero_offset` value. Previously saved offsets remain parseable but
are ignored while automatic selection is enabled. No configuration is rewritten
until the normal save workflow runs.

The terminal uses integer MHz inputs and a shared 5,400 MHz entry ceiling.
Model-specific validation runs after identification in the acquisition worker;
therefore out-of-profile input is rejected before mutation, but after the UI
saves the selection. Use the library for sub-MHz settings. Dynamic UI bounds
and detection before the selection screen remain future work. Existing partial
configuration files adopt `scan.enable_ultra_mode = false`, so this change does
not opt them into persistent instrument-state changes.

## Verification and qualification

Regression tests cover model detection, conflicts, range/path selection,
automatic native offsets, command inventory, readback failures, partial I/O and
resume after scan failure. Run the [local gates](security-and-ci.md#local-verification)
and `cargo coverage`. The hardware validator requires its selected family to
match detection before changing settings. Its `zs406` profile separately checks
0.1–900 MHz normal reception and 900 MHz–5.4 GHz opt-in Ultra reception.

The ZS406 normal and Ultra command paths were physically validated on 2026-10-07
with firmware `tinySA4_v1.4-197-gaa78ccc`; see
[hardware validation](hardware-validation.md#recorded-zs406-evidence). This is
not RF accuracy qualification. Generic Ultra-family variants still need their
own hardware runs, and Ultra RF level/frequency accuracy, Windows/macOS, Jenkins
and publication have not been verified for this change.
