# Working on tiny-wb

## Structure and dependencies

- Keep the binary entry point small: configuration, diagnostics, terminal lifetime,
  and application startup belong there.
- Separate configuration/persistence, protocol decoding, transport, acquisition,
  CSV export, shared scan state, and terminal presentation by responsibility.
  Do not recreate a miscellaneous `structs` module.
- Protocol and export code must be usable without a terminal or physical device.
  Accept `Read`/`Write` implementations or explicit data instead of opening files
  or serial ports inside parsers. Keep model rules separate from transport rules.
- Pass typed settings explicitly. Put user-adjustable defaults in
  `config/default.toml`; keep wire-format invariants in the protocol module.
  Validate settings before connecting or creating output, including units,
  bounds, empty collections, and non-finite numbers.
- Preserve existing configuration compatibility or document a tested migration.
  Unknown keys and invalid supplied values must produce actionable errors.
- Prefer small concrete types and functions over speculative abstraction.
  Introduce a trait only when multiple implementations or an actual test seam
  justify it. Avoid adding dependencies for small standard-library operations.
- Propagate I/O errors, use complete writes, bound receive buffers, and never
  interpret binary payload as text. Do not use `unsafe` for protocol ordering.

## Documentation and comments

- Update user-visible behavior, configuration examples, architecture/source maps,
  protocol notes, and verification instructions in the same task as code changes.
  Link to the authoritative document instead of duplicating detailed guidance.
- Add module documentation explaining its responsibility and boundaries. Public
  APIs need Rustdoc covering units, invariants, errors, and meaningful examples.
  Private comments should explain intent, non-obvious constraints, or tradeoffs;
  do not narrate straightforward statements or retain commented-out code.
- Keep names and terminology consistent: MHz in user settings/CSV, Hz on the
  wire, dBm for levels, milliseconds for configurable delays and timeouts.
- Separate observed behavior, upstream protocol evidence, and unverified hardware
  assumptions. Preserve source links and verification dates for device facts.
  Never claim a simulated transcript is a hardware integration test.
- Document limitations candidly. Avoid vague TODOs; explain the missing behavior,
  its consequence, and what evidence would justify the next change.
- Keep README concise; use `docs/` for operational and architectural details.
  Generated reports and badge SVGs stay ignored.

## Verification

- Add focused regression tests for changed behavior, especially malformed input,
  boundary values, partial reads/writes, timeouts, absent ports, persistence, and
  failure cleanup. Use in-memory transcripts and isolated temporary directories.
- Run formatting, locked checks/tests, Clippy with warnings denied, documentation,
  the release build, and the declared MSRV check. Use the exact commands in
  `docs/security-and-ci.md`; run security gates when changing dependencies.
- Run `cargo coverage` after changing Rust behavior. Raise the documented floor
  only from measured results; never exclude production code to satisfy a target.
- Run `python3 scripts/test-badges.py` after CI/badge changes. Verify documentation
  links and configuration examples after moving files or changing settings.
- Report unavailable hardware, Jenkins, platform, or publication checks explicitly.
  Do not start Jenkins or publish shared assets as part of local verification.

## Commits

- Preserve unrelated work. Make one commit per coherent task with a concise
  imperative subject (`refactor:`, `feat:`, `fix:`, `test:`, `docs:`, or `ci:`).
- Include tests and directly related documentation with their implementation.
  Keep an independent CI change or contributor policy in its own commit.
- Inspect each staged diff; never commit generated files, local configuration,
  logs, credentials, or build products. Do not push unless requested.
