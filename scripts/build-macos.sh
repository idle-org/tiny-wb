#!/bin/sh
# Compile-only Apple builds. See docs/macos.md for prerequisites and validation.
set -eu

if [ "$#" -ne 0 ]; then
    echo "usage: $0 (set CONTAINER_ENGINE=podman or docker)" >&2
    exit 2
fi

engine=${CONTAINER_ENGINE:-}
if [ -z "$engine" ]; then
    if command -v podman >/dev/null 2>&1; then
        engine=podman
    else
        engine=docker
    fi
fi
case "$engine" in
    podman|docker) ;;
    *) echo "CONTAINER_ENGINE must be podman or docker" >&2; exit 2 ;;
esac
command -v "$engine" >/dev/null 2>&1 || {
    echo "Install Podman or Docker before building macOS binaries." >&2
    exit 1
}

repository_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
# Pin the complete compiler/SDK environment, not the mutable latest tag.
image=ghcr.io/rust-cross/cargo-zigbuild@sha256:9b4f6b3eb9e8f9fefb3960fe288ddba576627d66d9eff851ce7f62d2455564a5
mkdir -p "$repository_dir/target/apple"

# Keep generated files owned by the invoking user on both container engines.
set -- run --rm --user "$(id -u):$(id -g)"
if [ "$engine" = podman ]; then
    set -- "$@" --userns=keep-id
fi
"$engine" "$@" \
    -v "$repository_dir:/io" -w /io \
    -e HOME=/tmp \
    -e CARGO_HOME=/io/target/apple/cargo-home \
    -e CARGO_TARGET_DIR=/io/target/apple \
    -e ZIG_GLOBAL_CACHE_DIR=/io/target/apple/zig-cache \
    -e MACOSX_DEPLOYMENT_TARGET=13.0 \
    "$image" \
    cargo zigbuild --locked --release --bin tiny-wb \
    --target aarch64-apple-darwin --target x86_64-apple-darwin

echo "Apple Silicon: $repository_dir/target/apple/aarch64-apple-darwin/release/tiny-wb"
echo "Intel: $repository_dir/target/apple/x86_64-apple-darwin/release/tiny-wb"
