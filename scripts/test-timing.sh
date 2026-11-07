#!/bin/sh
# Mandatory timing tier: run real scheduler/deadline scenarios alone, after the
# ordinary tests. The timing_ prefix is reserved for these explicit ignores.
# Usage: scripts/test-timing.sh [gate|release|quick] [cargo flags ...]
set -eu
profile=${1:-gate}
[ "$#" -eq 0 ] || shift
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
log=$(mktemp)
trap 'rm -f "$log"' EXIT HUP INT TERM
rc=0
cargo test --profile "$profile" "$@" timing_ -- --ignored --test-threads=1 >"$log" 2>&1 || rc=$?
cat "$log"
[ "$rc" -eq 0 ] || exit "$rc"
# A typo in the filter or accidentally lost ignores must never become a green
# empty tier. Existing manual/release ignores have no timing_ prefix.
awk '/^test result: ok\./ { passed += $4 } END { exit !(passed > 0) }' "$log" || {
    echo 'timing tier FAILED: no timing tests ran' >&2
    exit 1
}
