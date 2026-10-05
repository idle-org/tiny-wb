#!/bin/sh
# Project-local rust_realease entry point. See docs/releases.md before publishing.
set -eu
# Do not expose the first token-file line if a caller enables shell tracing.
set +x
unset GITEA_TOKEN

usage() {
    echo "usage: $0 <gitea-token-file> [rust_realease options...]" >&2
    echo "       $0 --compile-only [rust_realease options...]" >&2
    exit 2
}

[ "$#" -ge 1 ] || usage
token_file=
if [ "$1" = "--compile-only" ]; then
    shift
    set -- --compile-only "$@"
else
    token_file=$1
    shift
    if [ ! -f "$token_file" ] || [ ! -r "$token_file" ] || [ ! -s "$token_file" ]; then
        echo "token file must be readable, regular and nonempty" >&2
        exit 2
    fi
fi
if ! command -v rust_realease >/dev/null 2>&1; then
    echo "rust_realease is not installed or not available in PATH; see docs/releases.md" >&2
    exit 2
fi

repository_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
# This repository has one root package with a literal version. Fail closed if
# it is changed to an inherited/workspace version instead of guessing a value.
manifest_version=$(awk '
    /^[[:space:]]*\[/ { package = ($0 ~ /^[[:space:]]*\[package\][[:space:]]*(#.*)?$/) }
    package && /^[[:space:]]*version[[:space:]]*=[[:space:]]*"[^"]+"[[:space:]]*(#.*)?$/ {
        value = $0
        sub(/^[^"]*"/, "", value)
        sub(/".*$/, "", value)
        count++
    }
    END {
        if (count != 1 || value !~ /^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.+-]+)?$/) exit 1
        print value
    }
' "$repository_dir/Cargo.toml") || {
    echo "could not determine the tiny-wb package version from Cargo.toml" >&2
    exit 2
}

set -- "$repository_dir" --package tiny-wb --artifact-format both "$@"
# A hyphen in build metadata alone does not make a SemVer prerelease.
case "${manifest_version%%+*}" in
    *-*) set -- "$@" --prerelease ;;
esac

# Read after subprocess-based discovery, and export only when publishing or
# performing an authenticated dry run. Compile-only mode needs no token.
if [ -n "$token_file" ]; then
    GITEA_TOKEN=
    IFS= read -r GITEA_TOKEN <"$token_file" || :
    if [ -z "$GITEA_TOKEN" ]; then
        echo "the token file's first line must not be empty" >&2
        exit 2
    fi
    export GITEA_TOKEN
fi

# Cross builds in an isolated Debian/Ubuntu image, so native libraries from the
# maintainer's host are unavailable. Keep this explicitly selected file away
# from the root Cross.toml path validated by rust_realease.
CROSS_CONFIG=$repository_dir/release/Cross.toml
if [ ! -f "$CROSS_CONFIG" ] || [ ! -r "$CROSS_CONFIG" ]; then
    echo "release Cross configuration is missing or unreadable: $CROSS_CONFIG" >&2
    exit 2
fi
export CROSS_CONFIG
exec rust_realease "$@"
