#!/bin/sh
set -eu

case "${1:-}" in
    basic|ultra) ;;
    *)
        echo "usage: $0 <basic|ultra> [--port DEVICE] [--output DIRECTORY]" >&2
        exit 2
        ;;
esac

repository=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repository"
exec cargo run --locked --release --example validate-hardware -- "$@"
