# Building macOS binaries from Linux

Use the [release workflow](releases.md) with a working Podman or Docker backend
and a publisher supporting automated Apple builds:

```console
./release.sh --compile-only
```

This builds and packages Apple Silicon and Intel macOS alongside Linux and
Windows. To build only the Apple targets:

```console
./release.sh --compile-only --target aarch64-apple-darwin --target x86_64-apple-darwin
```

Select the backend with `CROSS_CONTAINER_ENGINE=podman` or `docker`. Network
access is needed for initial container and crate downloads. No Mac, host Xcode
installation or publication token is required. The standalone Apple build
wrapper has been removed; the publisher owns compilation and packaging.

## Toolchain and outputs

[`rust-release.toml`](../rust-release.toml) declares the immutable cargo-zigbuild
image digest, SDK path and **macOS 13.0** deployment minimum for both Apple
architectures. See the [release contract and prerequisites](releases.md) for
tool installation, artifact naming and the compiler/SDK policy. The upstream
[cargo-zigbuild Dockerfile](https://github.com/rust-cross/cargo-zigbuild/blob/main/Dockerfile)
documents SDK provenance; our configured digest identifies the actual image.

The SDK supplies IOKit and CoreFoundation for the existing `serialport`
dependency. The normal default features include terminal UI and serial transport.
Linux's libudev and Windows's resource compiler are not needed for Apple targets.

The publisher prints its output directory under ignored `dist/release-*/`.
Each Apple target produces a standalone executable and a cargo-binstall `.tgz`
archive, covered by the common `SHA256SUMS`. These are separate Mach-O binaries;
there is no universal binary, `.app` bundle, Developer ID signature or notarization.

## Verification

Run the normal [local quality gates](security-and-ci.md#local-verification),
`python3 scripts/test-release.py`, and the compile-only command above.
The publisher checks Apple architecture, `LC_BUILD_VERSION` minimum of 13.0,
system dynamic dependencies, archive layout and checksums. Inspect the printed
output directory and run `sha256sum -c SHA256SUMS` from that directory.
Linux cannot execute the Apple binaries, so their runtime checks are skipped.

The [recorded four-target verification](releases.md#macos-migration-verification-on-2026-10-20)
used rootless Podman and passed these checks for both Apple architectures.
Docker execution, native macOS terminal interaction, serial enumeration, actual
USB acquisition, Gatekeeper behavior, Jenkins and publication remain unverified.

On a Mac, first run `./tiny-wb --help`, then use the
[mock-device workflow](mock-device.md) for simulated acquisition and the
[hardware validation guide](hardware-validation.md) for a physical instrument.
Choose the actual enumerated macOS serial device in the UI; Linux/Windows
preferred port names in the defaults are preferences, not required devices.
