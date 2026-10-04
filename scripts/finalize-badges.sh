#!/bin/sh
set -eu
: "${BUILD_NUMBER:?BUILD_NUMBER is required}"
case "$BUILD_NUMBER" in *[!0-9]*) exit 1 ;; esac
trap 'rm -f coverage/summary.json "coverage/.tests-passed-${BUILD_NUMBER}" "coverage/.coverage-ready-${BUILD_NUMBER}"' 0
trap 'exit 1' HUP INT TERM
status=failing
[ ! -f "coverage/.tests-passed-${BUILD_NUMBER}" ] || status=passing
scripts/ci-coverage-badges.sh test --value "$status"
if [ ! -f "coverage/.coverage-ready-${BUILD_NUMBER}" ]; then
    scripts/ci-coverage-badges.sh coverage --value -
fi
version=$(cargo metadata --no-deps --locked --format-version 1 | jq -er '.packages[] | select(.name == "tiny-wb") | .version')
scripts/ci-coverage-badges.sh version --value "$version"
rm -f coverage/summary.json "coverage/.tests-passed-${BUILD_NUMBER}" "coverage/.coverage-ready-${BUILD_NUMBER}"
for name in version test coverage; do
    file="coverage/$name.svg"
    [ ! -L "$file" ] && [ -f "$file" ] && [ -s "$file" ] || exit 1
done
