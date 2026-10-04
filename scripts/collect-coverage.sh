#!/bin/sh
set -eu
mkdir -p coverage
summary=coverage/summary.json
trap 'rm -f "$summary"' 0
trap 'exit 1' HUP INT TERM
# Alias and honest initial floor live in .cargo/config.toml.
cargo coverage
percentage=$(jq -er '.data | if length == 1 then .[0].totals.lines.percent else error("expected one overall total") end | select(type == "number" and . >= 0 and . <= 100)' "$summary")
scripts/ci-coverage-badges.sh coverage --value "$percentage"
rm -f "$summary"
