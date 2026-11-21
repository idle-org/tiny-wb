#!/bin/sh
set -eu
mkdir -p coverage
summary=coverage/summary.json
log=coverage/coverage.log
trap 'rm -f "$summary"' 0
trap 'exit 1' HUP INT TERM
# Alias and honest initial floor live in .cargo/config.toml.
rm -f "$summary"
coverage_status=0
cargo coverage >"$log" 2>&1 || coverage_status=$?
# Preserve the explicit status check even when Cargo writes a valid report.
# Print the measurement before cleanup, but never mask a failed Cargo run.
if [ ! -s "$summary" ]; then
    echo "ERROR: cargo coverage produced no summary (exit $coverage_status); see coverage/coverage.log." >&2
    [ "$coverage_status" -ne 0 ] || coverage_status=1
    exit "$coverage_status"
fi
if ! percentage=$(jq -er '.data | if type == "array" and length == 1 then .[0].totals.lines.percent else error("expected one overall total") end | select(type == "number" and . >= 0 and . <= 100)' "$summary"); then
    echo "ERROR: cargo coverage produced an invalid line-coverage summary (exit $coverage_status); see coverage/coverage.log." >&2
    [ "$coverage_status" -ne 0 ] || coverage_status=1
    exit "$coverage_status"
fi
LC_ALL=C awk -v percentage="$percentage" 'BEGIN { printf "Final line coverage: %.2f%%\n", percentage }'
if [ "$coverage_status" -ne 0 ]; then
    echo "ERROR: cargo coverage failed (exit $coverage_status); check the measurement against --fail-under-lines and coverage/coverage.log." >&2
    exit "$coverage_status"
fi
scripts/ci-coverage-badges.sh coverage --value "$percentage"
rm -f "$summary"
