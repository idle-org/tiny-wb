# README screenshots

`scan-settings.png` and `scan-result.png` show the real tiny-wb terminal UI,
captured on 2026-10-20 from a Linux release binary. Input comes from the
[Basic mock device](../mock-device.md), not a physical instrument. The screenshot
binary identifies itself as `0.1.4-8ed07cb`; later release/documentation changes
did not change this UI.

To reproduce the screens, follow the mock-device walkthrough using a temporary
copy of the client settings. Set start to 280 MHz, stop to 320 MHz, points to
1001 and repeat to 1. Capture the scan-settings screen after confirming the
connection; start the scan and press `m` on the completed plot to show its peak.
The mock's peak is near 300 MHz. The displayed CSV filename varies with time.

These PNGs were rendered from the application's captured pseudo-terminal output
using pyte 0.8.2 and Pillow 13.0.0, with DejaVu Sans Mono and a dark terminal
palette. The setup screen is 132 columns by 13 rows; the result is 132 by 26.
Colors were enabled (`NO_COLOR` unset). No UI labels or trace values were drawn
by hand. The mock run completed acquisition and exported one CSV in a temporary
directory. Screenshot tooling is not an application or build dependency.
