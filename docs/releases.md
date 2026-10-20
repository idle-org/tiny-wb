# Releasing tiny-wb

For compile-only Apple Silicon and Intel macOS binaries using a pinned Zig
container, see [macOS cross-compilation](macos.md). That separate build command
does not add Apple artifacts to the publisher's Linux/Windows matrix below.

`release.sh` delegates to the deliberately named
[`rust_realease`](https://proxy.dandaedre.com/gitea/idle-org/rust_realease).
It is based on the
[john-slop wrapper](https://proxy.dandaedre.com/gitea/idle-org/john-slop/src/branch/main/release.sh),
with this project's package, targets, tag convention and repository mapping.
The wrapper does not edit metadata, commit, tag, push, or repair a release.
A normal invocation **does publish** through the delegated publisher; begin
with a dry run and obtain publication authorization separately.

## Release contract

| Property | Source/policy |
| --- | --- |
| Cargo package | `tiny-wb`, the single root package in `Cargo.toml`. |
| Binary | `tiny-wb` (`tiny-wb.exe` on Windows); the library is not a release binary. |
| Version | Literal `[package].version` in `Cargo.toml`; never duplicated in the wrapper. |
| Tag | Package version without a prefix, preserving existing tags such as `0.1.1`. |
| Profile | `release`, for both targets; the older custom `windows` profile is not selected. |
| Targets | `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-gnu`. |
| Artifacts | Both cargo-binstall `.tgz` archives and standalone executables, plus `SHA256SUMS`. |
| Builds | Locked dependencies; tracked `Cargo.lock`; review toolchain/container inputs for repeatability. |
| Public repository | `https://proxy.dandaedre.com/gitea/idle-org/tiny-wb`. |
| Development origin | `git@git.dandaedre.com:idle-org/tiny-wb.git`. |
| Prerelease | A hyphen in the version before any `+build` suffix adds `--prerelease`. |

`rust-release.toml` sets the target matrix, profile, tag prefix, lock policy,
`GITEA_TOKEN` variable and exact origin-to-public-URL fallback. This avoids
copying the publisher's musl/ARM defaults: tiny-wb uses system libudev on Linux,
and its documented desktop platforms are Linux and Windows. FreeBSD remains
a CI host, not an additional promised release artifact. Target builds are not
yet verified. Do not infer compatibility merely from the configured matrix.

The fallback does not rewrite the Git origin. `Cargo.toml` records the public
repository and the publisher-compatible binstall archive layout. Tag URLs lack
`v`; archive names retain `-v{version}`. For example, version `0.1.1` plans:

```text
tiny-wb-x86_64-unknown-linux-gnu-v0.1.1.tgz
tiny-wb-x86_64-unknown-linux-gnu-v0.1.1
tiny-wb-x86_64-pc-windows-gnu-v0.1.1.tgz
tiny-wb-x86_64-pc-windows-gnu-v0.1.1.exe
SHA256SUMS
```

Each archive contains a matching stem directory and its one executable.
Public release/download verification and cargo-binstall checks are supported;
they remain unverified until a separately authorized real release succeeds.
Use cargo-binstall's `--git` mode for this repository rather than assuming it
is published to crates.io. This workflow does not run `cargo publish`.

## Prerequisites

Required: Git, Cargo/Rust (at least the declared MSRV), `rust_realease`, Cross,
and a usable Docker or Podman backend. Archives additionally need cargo-binstall
for installation verification. Local wrapper tests use Python 3.11+ (`tomllib`).
There is currently no repository pin for the publisher; the inspected upstream
source identifies itself as version 0.1.4. Confirm its installed version and
help before use. Tool installation is a maintainer action, not performed by
the wrapper. Upstream's preferred installation command is:

```console
cargo binstall --git https://proxy.dandaedre.com/gitea/idle-org/rust_realease rust_realease
rust_realease --version
rust_realease --help
```

For Cross installation and image setup, follow the
[Cross project](https://github.com/cross-rs/cross). Select a working backend with
`CROSS_CONTAINER_ENGINE=podman` or `docker` when necessary. Check `cross --version`,
`cargo binstall -V`, and the selected engine's `info` command before building.

The Linux target needs libudev development files and pkg-config configuration;
the Windows target needs the GNU resource compiler used by `build.rs`
(`windres`). Native host compilation alone does not verify those container
prerequisites. [`release/Cross.toml`](../release/Cross.toml) installs
`libudev-dev` and `pkg-config` in the Linux image. `release.sh` exports its
absolute path as `CROSS_CONFIG` for the publisher process. The file is kept away
from the repository root because the publisher rejects root `pre-build`,
`dockerfile`, and `runner` hooks. Package installation requires access to the
image's Debian/Ubuntu repositories. A caller-supplied `CROSS_CONFIG` is
intentionally replaced so releases use the reviewed, tracked configuration.

## Token handling and commands

Store a token in a readable regular file outside the checkout, with only the
token on its first line; restrict its permissions to its owner. The account
needs repository read access and release write access. Do not commit tokens or
token paths. The wrapper reads the first line verbatim (also without a trailing
newline), rejects an empty first line, and exports it as `GITEA_TOKEN` only to
the publisher. It never passes the token in arguments or prints it. The publisher
is responsible for keeping it out of child builds and for download verification.

Compile-only builds do not access Gitea and need no token:

```console
./release.sh --compile-only
```

`--compile-only` builds local artifacts in ignored `dist/release-*/`, without
contacting Gitea. Inspect `SHA256SUMS`, archive members, executables and the
binstall layout before publication.

For authenticated dry runs and publication, set `TOKEN_FILE` in your shell to
the existing token file's path; it is not a checked-in setting:

```console
./release.sh "$TOKEN_FILE" --dry-run
```

A dry run must report the correct package, version, unprefixed tag, both targets,
asset names and public HTTPS destination without changing files, refs or releases.
A successful dry run is required but does not replace collision inspection.

After all readiness checks and separate publication authorization:

```console
./release.sh "$TOKEN_FILE"
```

You can invoke the script by its absolute path from another working directory.
The repository resolves from the script location; relative token and forwarded
file paths remain relative to your invoking directory. Additional options are
forwarded exactly, including spaces. For example, use `--message-file` with a
reviewed release-notes file. The publisher can update an existing release body
when notes are explicitly supplied. Do not add preparation/push/replacement flags
without deliberate authorization for those operations.

## Readiness, collisions and repeatability

Before publication:

1. Run the [local quality gates](security-and-ci.md#local-verification),
   `python3 scripts/test-release.py`, `sh -n release.sh`, and ShellCheck when
   available. Run `cargo package --locked` to verify the source package; this is
   a local build/check, not registry publication.
2. Review release notes/changelog and set the intended package version. There
   was no tracked changelog when this wrapper was introduced. Do not invent
   historical entries or bump versions automatically. Record changes for the
   next chosen version before publishing.
3. Require a clean checkout, a local unprefixed version tag at `HEAD`, and the
   same tag/commit on origin. Inspect `git status --short`, `git rev-parse HEAD`,
   `git rev-parse '<version>^{commit}'`, and `git ls-remote --tags origin`.
   Tag creation and pushing are separate maintainer operations; never move an
   existing tag to make this check pass.
4. Inspect the release-by-tag API at
   `https://proxy.dandaedre.com/gitea/api/v1/repos/idle-org/tiny-wb/releases/tags/<version>`.
   A 404 means missing; authentication, network and server errors are blockers,
   not absence. Check draft/prerelease status and **all** asset names.
5. Compare the planned archive, binary and `SHA256SUMS` names to existing assets.
   Any collision stops publication. `--replace-assets` requires inspection and
   explicit approval to replace those exact assets; replacement is not atomic.
   An existing empty release can be reused only after tag/version checks pass.
6. Run the real wrapper dry run. Resolve every prerequisite or metadata error.
   Repeat tag and release/asset inspection immediately before live publication.

The publisher normalizes archive metadata, but this is not a bit-for-bit build
reproducibility claim. Rust toolchains and Cross image digests are not pinned
here, and `build.rs` embeds a Git revision. Record those inputs per release.
Use `--recompile` if a matching publisher build cache must not be reused.

## Inspection recorded on 2026-10-04

- Package and binary: `tiny-wb` 0.1.1. Expected tag: `0.1.1`, not `v0.1.1`.
- Both local and public-HTTPS remote `0.1.1` point to
  `82aa266e18454905c01287035c377b8edf9f9c60`, not the current refactored checkout.
  This is a publication blocker. The repository already has a later `0.1.2`
  release, so choose the next version deliberately rather than retagging 0.1.1.
- The `0.1.1` release API returned HTTP 200: public, non-draft, non-prerelease,
  **no assets**. None of the five planned names collide in that snapshot.
  The `v0.1.1` endpoint returned 404. These checks do not authorize reuse.
- Git 2.56.0, Cargo 1.93.0, cargo-binstall 1.17.5 and Podman 6.1.3 were present.
  Podman reported a usable `amd64` backend. The SSH origin query failed
  (connection closed), so the remote tag was checked through the public HTTPS
  Git endpoint; SSH push access remains unverified.
  `rust_realease`, Cross and ShellCheck were unavailable and were not installed.
  Publisher CLI/configuration and artifact names were inspected from upstream
  source, not confirmed by a runnable local publisher.
- The real publisher dry run could not start (`command not found`). A real-token
  wrapper dry run, Cross `--compile-only`, built-asset/checksum inspection and
  cargo-binstall download verification remain **not verified**. No token file
  was supplied, and no release or asset was created or replaced.
