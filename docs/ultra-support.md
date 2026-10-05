# Basic and Ultra acquisition support

Implemented 2026-10-05; synthetic verification is separate from hardware qualification.

The acquisition path reads `version`, `info`, and `help` before changing scan
settings. Recognized Basic (`tinySA_v…`) and Ultra-family (`tinySA4_v…`) identities
select different plans. Unknown or conflicting identities fail. Explicit
`scan.device_model = "basic"` or `"ultra"` requires a matching detected family;
the default is `"auto"`. Family detection does not identify a hardware revision
or certify that an arbitrary firmware build implements every command correctly.

| Family | Implemented acquisition range | Input command | Automatic raw bias |
| --- | --- | --- | --- |
| Basic | Existing low/high policy through 960 MHz | `mode low input` or `mode high input` | 128 dB |
| Ultra/Ultra+ | Common normal range 0.1–800 MHz | `mode low input` | 174 dB |

The [official comparison](https://tinysa.org/wiki/pmwiki.php?n=TinySA4.Comparison)
lists 800 MHz normal range for ZS405 and 900 MHz for Ultra+ revisions. This
initial implementation deliberately uses their shared 800 MHz ceiling.
Extended/Ultra/harmonic ranges, revision detection, LNA control and mode-state
restoration are not implemented. Existing instrument processing settings and
Ultra-enable state are retained; no unlock or persistent-save command is sent.
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

The terminal uses integer MHz inputs and a shared 960 MHz entry ceiling.
Model-specific validation runs after identification in the acquisition worker;
therefore out-of-profile input is rejected before mutation, but after the UI
saves the selection. Use the library for sub-MHz settings. Dynamic UI bounds
and detection before the selection screen remain future work.

## Verification and qualification

Regression tests cover model detection, conflicts, range/path selection,
automatic native offsets, command inventory, readback failures, partial I/O and
resume after scan failure. Run the [local gates](security-and-ci.md#local-verification)
and `cargo coverage`. The hardware validator now requires its selected family
to match detection before changing settings and exercises Ultra below 800 MHz.

This is initial software support, not a claim of hardware-qualified first-tier
support. Existing Basic evidence predates the new `help`/`zero` readback flow;
rerun [hardware validation](hardware-validation.md) on both instruments. Ultra
RF level/frequency accuracy, Windows/macOS, Jenkins and publication have not
been verified by these synthetic tests.
