#!/bin/sh
# Run from the repository root. The root already includes /coverage.
set -eu
fail() { echo "$*" >&2; exit 1; }
[ "$#" -ge 5 ] || fail 'usage: publish-badges.sh ROOT OWNER PROJECT BRANCH NAME...'
root=$1 owner=$2 project=$3 branch=$4
shift 4
case "$root" in /*) ;; *) fail 'publication root must be absolute' ;; esac
[ -d "$root" ] && [ -w "$root" ] || fail 'publication root must exist and be writable'
for component in "$owner" "$project" "$branch"; do
    case "$component" in ''|.|..|*[!a-zA-Z0-9_.-]*) fail 'invalid path component' ;; esac
done
seen=' '
for name do
    case "$name" in coverage|test|version) ;; *) fail 'invalid badge name' ;; esac
    case "$seen" in *" $name "*) fail 'duplicate badge name' ;; esac
    seen="$seen$name "
    source="coverage/$name.svg"
    [ ! -L "$source" ] && [ -f "$source" ] && [ -s "$source" ] || fail "invalid source: $source"
done
# Explicit chmod makes newly created directories readable even under umask 077.
destination=${root%/}
for component in "$owner" "$project" "$branch"; do
    destination="$destination/$component"
    if [ "$destination" = "${root%/}/$owner/$project/$branch" ] && [ -L "$destination" ]; then
        unlink "$destination"
    fi
    [ ! -L "$destination" ] || fail "symlink destination: $destination"
    if [ ! -e "$destination" ]; then
        mkdir -m 0755 "$destination"
        chmod 0755 "$destination"
    fi
    [ -d "$destination" ] || fail "not a directory: $destination"
done
temporary=
trap '[ -z "$temporary" ] || rm -f "$temporary"' 0
trap 'exit 1' HUP INT TERM
for name do
    # Never let mv interpret an existing directory (or link to one) as a target.
    [ ! -d "$destination/$name.svg" ] || fail 'badge destination is a directory'
    temporary=$(mktemp "$destination/.$name.svg.XXXXXX")
    cp "coverage/$name.svg" "$temporary"
    chmod 0644 "$temporary"
    mv -f "$temporary" "$destination/$name.svg"
    temporary=
done
