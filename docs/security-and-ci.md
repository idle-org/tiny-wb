# Security, dependency policy, and MSRV

This project applies a deliberately small supply-chain hardening policy. It
tracks the application lockfile, verifies a stable compatibility floor, and
runs `cargo-audit` plus `cargo-deny`. No other hardening tools are in scope.

## Rust compatibility floor

The declared and CI-tested minimum supported Rust version (MSRV) is
**Rust 1.88.0**. This is an LTS-like project policy rather than an upstream Rust
LTS designation: Rust itself does not publish LTS toolchains.

The application code and the former Ratatui 0.28 dependency could be made to
compile with older compilers, but that graph contained the unmaintained `paste`
crate and `lru` releases covered by RustSec soundness advisories. Ratatui 0.30.0
still locked an affected optional `lru` and a vulnerable `time`. Ratatui 0.30.2
is the first tested upstream line that clears those findings, and it declares
Rust 1.88.0. The project therefore treats 1.88.0 as the lowest supported clean
graph rather than adding advisory exceptions or local dependency patches.

`Cargo.toml` declares `rust-version = "1.88.0"`. CI installs that exact
toolchain and runs:

```console
cargo +1.88.0 check --locked --all-targets --all-features
```

The normal CI jobs use current stable Rust, while the MSRV job prevents source
or dependency changes from silently raising the floor. `.cargo/config.toml`
asks modern Cargo to prefer dependency releases compatible with the declared
MSRV when the lockfile is deliberately refreshed.

## Reproducible dependency graph

`Cargo.lock` is tracked because tiny-wb is an executable. All CI compilation,
test, Clippy, and release-build commands use `--locked`; an uncommitted
resolution change is therefore a CI error. Do not update dependencies by
deleting the lockfile casually. After an intentional update, rerun every gate
in this document and review the lockfile diff.

Unused direct dependencies were removed while establishing the lock:
`clap`, `once_cell`, `ratatui-splash-screen`, and `tui-textarea`. Removing them
reduces download/build surface and makes policy results describe code that the
application actually uses. The acquisition refactor also removed direct
`packed_struct` and `memchr` dependencies: standard-library little-endian
decoding and buffered framing now provide those functions.

## Pinned policy tools

CI pins:

| Tool | Version | CI command |
| --- | ---: | --- |
| `cargo-audit` | 0.22.2 | `cargo audit --deny warnings` |
| `cargo-deny` | 0.20.2 | `cargo deny check` |

The setup stage checks the installed version and installs only when it does not
match exactly:

```console
cargo install --locked --version 0.22.2 cargo-audit
cargo install --locked --version 0.20.2 cargo-deny
```

Installing the tools and running both gates requires network access. The audit
jobs must refresh advisory and registry information; CI must fail rather than
silently using a stale or unavailable database.

## RustSec audit policy

`cargo audit --deny warnings` evaluates the complete tracked lockfile against
the current RustSec database and crates.io yanked status. Warnings such as
unmaintained or unsound advisories are promoted to failures, as are
vulnerabilities, yanked packages, database refresh failures, and tool errors.

There are no advisory ignores. If a future finding has no upgrade path, adding
an ignore requires an explicit maintainer decision documenting why tiny-wb is
unaffected and the condition that will remove the ignore.

`cargo-audit` is dependency/advisory matching. It does not prove exploit
reachability, review application logic, scan native system libraries, or
replace timely dependency maintenance.

## Dependency policy

`deny.toml` is evaluated with all features and enforces:

- license confidence of 0.8;
- only licenses currently needed by the resolved graph: Apache-2.0, MIT,
  MPL-2.0, Unicode-3.0, and Zlib;
- denial of wildcard dependency requirements;
- warnings for duplicate crate versions;
- denial of unknown registries and Git sources;
- crates.io as the only allowed registry and no allowed Git sources;
- RustSec advisory checks with no ignores;
- empty crate bans, duplicate skips, license exceptions, and license
  clarifications.

The MPL-2.0 allowance is needed by `serialport`. It is a graph-wide allowed
license, not an exception for one crate.

Duplicate warnings remain visible. The reviewed graph currently contains
transitive version splits for `bitflags`, `hashbrown`, `syn`, `toml`,
`toml_edit`, `windows-sys`, and `winnow`. They come from incompatible upstream
constraints in the serial, Ratatui, packed-struct, TOML, and Windows build
paths. No `skip` entries hide them. Direct `strum` and `thiserror` versions were
aligned with the graph where the duplicates were avoidable.

`cargo-deny` relies on package metadata and configured source URLs. It does not
perform source-code review, establish legal advice, verify downloaded artifact
provenance, or find vulnerabilities outside its advisory check.

## Local verification

Run from the repository root, in this order:

```console
cargo audit --deny warnings
cargo deny check
cargo fmt --all -- --check
cargo +1.88.0 check --locked --all-targets --all-features
cargo check --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
cargo test --locked --no-default-features --lib
cargo check --locked --no-default-features --features serial --lib
cargo test --locked --no-default-features --features mock --lib --test mock_device
cargo test --locked --doc --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release --all-features
```

The first two commands require the network. Duplicate-version warnings from
`cargo deny check` are expected to stay visible and should be reviewed whenever
the graph changes.

## Scope limit

This is intentionally “semi-hardened” CI. The repository does not add
`cargo-vet`, SBOM generation, dependency vendoring, registry proxies,
provenance/signing systems, or additional security scanners. Expanding that
scope needs a separate design and maintenance decision.

## CI badges and coverage

The README references version, tests and line coverage at these mutable URLs:

- https://assets.dandaedre.com/coverage/idle-org/tiny-wb/main/version.svg
- https://assets.dandaedre.com/coverage/idle-org/tiny-wb/main/test.svg
- https://assets.dandaedre.com/coverage/idle-org/tiny-wb/main/coverage.svg

Repository identity and `main` were verified against the origin remote, its
HEAD, and Gitea repository metadata. The version comes from `Cargo.toml` via
`cargo metadata --no-deps --locked --format-version 1`, never a Jenkins constant.

Coverage uses pinned `cargo-llvm-cov` 0.6.16 and Rust's `llvm-tools-preview`
component. The worker also needs `jq`. Install the prerequisites and run:

```console
rustup component add llvm-tools-preview
cargo install --locked --version 0.6.16 cargo-llvm-cov
mkdir -p coverage
cargo coverage
```

The exact alias is tracked in `.cargo/config.toml` and expands to:

```console
cargo llvm-cov --locked --all-targets --all-features --json --summary-only --fail-under-lines 85 --output-path coverage/summary.json
```

The enforced line-coverage floor is **85%**, measured across the library and
application without excluding production sources. Local measurement on 2026-10-04 was **1,852 of 2,038 lines (90.87%)**.
The suite includes library, application, mock/loopback integration and startup
tests, plus separately run Rustdoc examples. Tests
cover configuration, protocol, transport, acquisition, export, UI, startup and
mock device behavior. The floor leaves some headroom for
platform differences; it is not evidence of device/firmware correctness.

`scripts/collect-coverage.sh` runs that command, validates the overall line
percentage from temporary `coverage/summary.json`, renders the coverage badge,
and deletes the summary, including on failure. Jenkins then creates the current
build's test and coverage markers only after unit/integration tests, Rustdoc tests, and the coverage test run succeed.
The release smoke test remains a nonfatal diagnostic.

At checkout, CI recreates ignored `coverage/` to remove stale workspace state.
In `post { cleanup { ... } }`, it uses only current-build markers to choose test
status and retain measured coverage, falling back to `failing` and neutral `-%`
when markers are absent. Cleanup reads the version, removes the summary and
markers, validates and archives exactly `coverage/version.svg`,
`coverage/test.svg` and `coverage/coverage.svg` with fingerprints and
`allowEmptyArchive: false`. A later lint/build failure can retain successful
test/coverage results; these badges report those individual stages.

Only `main` publishes to `/mnt/assets/coverage/idle-org/tiny-wb/main/`, with
`/mnt/assets/coverage` passed as the complete publication root. Publication
runs in cleanup even after failure. Each file is replaced atomically, with
0755 new directories and 0644 files; the script never prunes shared assets.
Concurrent builds of this job are disabled to avoid workspace/marker races.

The publication predicate requires a Jenkins **multibranch** job's `BRANCH_NAME`
and excludes change requests and tags. The actual job configuration is not in
this repository and remains unverified. When `BRANCH_NAME` is absent,
publication is skipped with a diagnostic; generation and archival still run.
Verify the job type and its branch variable before deployment. Real Jenkins
execution, access to the asset mount, and public URL retrieval are separate
unverified deployment checkpoints; README links alone do not make badges live.

Server configuration is external: mount public assets read-only, disable
directory listing, serve SVG with a restrictive Content-Security-Policy, and
revalidate mutable URLs or give them a short cache lifetime.

Run isolated badge checks with `python3 scripts/test-badges.py`. The renderer
was copied from `Dandaedre/agent-tasks` at revision
`fb9d8f7d16f100c2d1377f23394a2302eac9ab18` (retrieved 2026-10-04), with local
fixes for literal newlines in width measurement, oversized integer validation,
unique temporary files, directory-output rejection and signal termination.

The library feature matrix runs before coverage markers are issued. Mock tests
require localhost listener permission and never connect to physical hardware.
