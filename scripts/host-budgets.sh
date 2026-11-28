#!/bin/sh
# Host timing budgets as warnings (25.11.2026, gate false reds): runs the tests of scripts/host-budgets.txt one at a time, so
# nothing else competes while they measure, and prints each red one as a warning. scripts/gate.sh never runs them; the nightly
# run on main does (tools/nightly-gate.sh counts the warnings in NIGHTLY.md). A real slowdown is there every night by name.
#   sh scripts/host-budgets.sh [tree]
# Prints "host-budget: WARNING <test>" with its first failure lines, then "HOST-BUDGETS: N warnings (M tests run)".
# Exit 0 (with warnings too); 2 bad usage, the tests did not build or none ran (a renamed test must not pass silently).
set -u
case ${1:-} in -*) echo "usage: sh scripts/host-budgets.sh [tree]" >&2; exit 2 ;; esac
cd "${1:-$(dirname "$0")/..}" || exit 2
[ -f scripts/host-budgets.txt ] || { echo "HOST-BUDGETS: no scripts/host-budgets.txt"; exit 2; }
export PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
log=$(mktemp "${TMPDIR:-/tmp}/host-budgets.XXXXXX") || exit 2
trap 'rm -f "$log"' EXIT
warnings=0 ran=0
# run LABEL CARGO-ARGS... -- TEST-ARGS: one cargo run, each red test a warning.
run() {
    label=$1; shift
    # shellcheck disable=SC2086
    ${HOST_BUDGETS_CARGO:-cargo} test --profile "${GATE_PROFILE:-gate}" --no-fail-fast "$@" >"$log" 2>&1
    grep -q '^test result:' "$log" || { tail -20 "$log"; echo "HOST-BUDGETS: the tests did not build ($label)"; exit 2; }
    ran=$((ran + $(awk '/^test result:/ { n += $4 + $6 } END { print n + 0 }' "$log")))
    for t in $(sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' "$log"); do
        warnings=$((warnings + 1))
        echo "host-budget: WARNING $t ($label)"
        sh scripts/bookkeeping.sh --failure-lines "$log" "$t" 2>/dev/null | head -8
    done
}
lib_tests=$(sed -n 's/^test lib \([^ ]*\)$/\1/p' scripts/host-budgets.txt | tr '\n' ' ')
# shellcheck disable=SC2086
[ -z "$lib_tests" ] || run "library" --lib -- --exact --test-threads=1 $lib_tests
for target in $(sed -n 's/^test \([^ ]*\) [^ ]*$/\1/p' scripts/host-budgets.txt | grep -vx lib | sort -u); do
    names=$(sed -n "s/^test $target \\([^ ]*\\)$/\\1/p" scripts/host-budgets.txt | tr '\n' ' ')
    # shellcheck disable=SC2086
    run "$target" --test "$target" -- --exact --test-threads=1 $names
done
for suite in $(sed -n 's/^suite \([^ ]*\)$/\1/p' scripts/host-budgets.txt); do
    run "$suite" --test "$suite" -- --test-threads=1
done
[ "$ran" -gt 0 ] || { echo "HOST-BUDGETS: no budget test ran (renamed?)"; exit 2; }
echo "HOST-BUDGETS: $warnings warnings ($ran tests run)"
