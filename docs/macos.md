# Building macOS binaries from Linux

Run from a Linux checkout with a working Podman or Docker installation:

```console
./scripts/build-macos.sh
```

The script prefers Podman when available. Select Docker explicitly with
`CONTAINER_ENGINE=docker ./scripts/build-macos.sh`. It works from another
directory too, using the checkout containing the script. Network access is
needed for the initial container and crate downloads. No Mac, Xcode installation
on the host, interactive container session, or publication token is required.

## Toolchain and dependencies

The script pins the upstream
[cargo-zigbuild container](https://github.com/rust-cross/cargo-zigbuild)
by SHA-256 manifest digest. The inspected image contains Rust 1.93.0,
cargo-zigbuild 0.23.4, Zig 0.16.0, both Apple Rust targets, and the macOS 11.3 SDK
at `/opt/MacOSX11.3.sdk` (`SDKROOT`). See the upstream
[Dockerfile](https://github.com/rust-cross/cargo-zigbuild/blob/main/Dockerfile)
for SDK provenance and image construction; the digest in the script, rather
than the moving upstream branch, identifies our actual build environment.

The SDK supplies IOKit and CoreFoundation for the existing `serialport`
dependency. No additional Cargo dependency or Homebrew package was necessary.
Linux's libudev and Windows's resource compiler are not needed for Apple
targets. The normal default features include both the terminal UI and serial
transport; these builds do not disable serial support to avoid linking it.

The declared deployment minimum is **macOS 13.0**, for both architectures.
During verification, requesting 11.0 still produced a 13.0 Mach-O minimum
with this toolchain. The script therefore explicitly requests 13.0. Supporting
older systems requires a separately verified toolchain and runtime testing;
the SDK directory's version is not the executable's deployment minimum.

The image pin fixes the compiler/SDK inputs, and `--locked` preserves the
dependency graph. This is not a claim of bit-for-bit reproducibility.
Container updates should rerun the checks below and inspect deployment
versions and dynamic dependencies before changing the pin.

## Outputs

Both release binaries are produced in one invocation:

| Architecture | Executable |
| --- | --- |
| Apple Silicon | `target/apple/aarch64-apple-darwin/release/tiny-wb` |
| Intel | `target/apple/x86_64-apple-darwin/release/tiny-wb` |

These are separate Mach-O executables, not a universal binary or `.app` bundle.
Cargo downloads, compilation outputs, and Zig caches live under ignored
`target/apple/`. Generated files belong to the invoking user. The container
mounts the checkout to compile it; use a checkout without concurrent builds
writing into the same Apple target directory.

The script only compiles. The existing [`release.sh` publisher](releases.md)
still handles its Linux/Windows matrix; it does not package or upload these
Apple artifacts. They have no Developer ID signature or notarization.

## Verification

Wrapper regression tests run without containers or network access:

```console
sh -n scripts/build-macos.sh
python3 scripts/test-macos-build.py
```

Run the normal [local quality gates](security-and-ci.md#local-verification)
and the real build script. On Linux with LLVM tools installed, inspect:

```console
file target/apple/*-apple-darwin/release/tiny-wb
llvm-objdump --macho --dylibs-used target/apple/aarch64-apple-darwin/release/tiny-wb
llvm-objdump --macho --private-headers target/apple/aarch64-apple-darwin/release/tiny-wb
llvm-objdump --macho --dylibs-used target/apple/x86_64-apple-darwin/release/tiny-wb
llvm-objdump --macho --private-headers target/apple/x86_64-apple-darwin/release/tiny-wb
```

Check the architecture, `LC_BUILD_VERSION` minimum of 13.0, and that loaded
libraries are Apple system paths rather than container/build-machine paths.

Observed on **2026-10-20**: both default-feature release binaries cross-compiled
on Linux using rootless Podman. Their dynamic dependencies were IOKit,
CoreFoundation, libSystem, libobjc, libiconv, and libcharset, all under
`/System/Library/Frameworks/` or `/usr/lib/`. A separate
`cargo zigbuild --locked --release --all-targets --all-features` invocation
with both Apple targets also compiled and linked the tests and examples.
The native Linux formatting, MSRV, feature-matrix checks/tests, Rustdoc,
Clippy, and release build passed. Docker execution is covered only by wrapper
argument tests, not a real Docker build.

Execution on macOS, terminal interaction, serial enumeration, actual USB
acquisition, Gatekeeper behavior, Jenkins execution, and publication remain
unverified. Cross-compiling tests does not run them on macOS.

On a Mac, first run `./tiny-wb --help`, then use the
[mock-device workflow](mock-device.md) for simulated acquisition and the
[hardware validation guide](hardware-validation.md) for a physical instrument.
Choose the actual enumerated macOS serial device in the UI; Linux/Windows
preferred port names in the defaults are preferences, not required devices.
