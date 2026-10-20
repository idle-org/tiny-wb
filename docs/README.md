# tiny-wb technical documentation

This directory is the local reference for the project and the instruments it
controls. It is deliberately split by concern so that maintainers normally do
not need to reread the upstream wiki before changing the code.

## Reading order

1. [Project architecture](architecture.md) explains what `tiny-wb` does and
   how control and data move through the Rust application.
2. [tinySA field guide](tinysa-field-guide.md) identifies the hardware,
   explains the RF signal paths, and covers safe and accurate use.
3. [USB console and `scanraw` protocol](usb-protocol.md) is the implementation
   reference for serial commands and binary acquisition.
4. [tiny-wb compatibility and operations](tiny-wb-operations.md) maps those
   facts onto the behavior, restrictions, and failure modes of this repository.
5. [Security, dependency policy, and MSRV](security-and-ci.md) documents the
   reproducible-build and supply-chain gates.
6. [Upstream source catalogue](sources.md) records the primary references and
   their scope.
7. [Hardware validation](hardware-validation.md) provides guided Basic and Ultra
   checklists and the interactive validator command.

## Scope and authority

There are three different kinds of statement in these documents:

- **Instrument facts** come from the official tinySA wiki or the upstream
  firmware source.
- **Application facts** come from the code in this repository.
- **Engineering conclusions** are called out as implications, risks, or
  recommendations.

The material was verified on **2026-10-04**. Specifications and console
commands can change with firmware and hardware revisions. When a connected
device disagrees with this documentation, record its `version`, `info`, and
`help` output and treat those results plus the matching firmware source as
authoritative.

## Why this is not an mdBook yet

The documentation is a small, navigable set of Markdown files with direct
cross-links. Adding mdBook configuration and a CI publication stage now would
introduce a new build dependency without improving navigation. If the guide
grows into many chapters, the files are already organized so that a future
`SUMMARY.md` can include them with little restructuring.

## Contributor references

- [Configuration](configuration.md): embedded defaults, partial overrides and persistence.
- [Testing](testing.md): regression seams and hardware verification boundaries.
- [Agent/contributor guidance](../AGENTS.md): code structure, comments, documentation and atomic commits.
- [Releases](releases.md): release wrapper, prerequisites, collision checks and publication policy.
- [macOS cross-compilation](macos.md): automated Apple Silicon and Intel builds from Linux.
- [Basic and Ultra support](ultra-support.md): implemented range, model detection and qualification status.
- [Feature roadmap](roadmap.md): model-aware Basic/Ultra completion tasks.
- [Implementation task backlog](../task.md): detailed milestones and optional stretch goals.
- [Library integration](library.md): feature selection and public interfaces.
- [Mock device](mock-device.md): in-memory and TCP simulation.
