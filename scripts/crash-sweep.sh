#!/bin/sh
# The crash sweep in one command, for the release recipe (docs/dev/tests.md "Crash sweep"): builds pocket-ui and
# pocket-core with overflow checks on (Cargo.toml `[profile.sweep]`: a debug build's panics in a fast build), runs the
# case-knows fuzzer and the card update file's fuzzer long (CASE_KNOWS_FUZZ_ROUNDS, default 20), then scripts/crash-sweep.py over every look and Cozy setting.
#
#   sh scripts/crash-sweep.sh [--quick] [--jobs N] [out]      (out: default target/crash-sweep)
#
# Needs cc, python3, weston (its headless backend) and, outside a container, bwrap. CRASH_SWEEP_APT=1 installs
# weston with apt-get when it is missing (a throwaway build container).
# A shared computer: CARGO_BUILD_JOBS=2 and an own CARGO_TARGET_DIR; a throwaway container: CRASH_SWEEP_APT=1.
# Exit 1 when the fuzzer or the sweep found something (DIR/report.md), 2 when a tool is missing.
set -eu
cd "$(dirname "$0")/.."
# Its own TMPDIR for the build, the fuzzer and the sweep, removed at the end: scripts/run-tmp.sh
if [ -f scripts/run-tmp.sh ]; then . scripts/run-tmp.sh; run_tmp_begin sweep; fi
quick=""
jobs=2
out=""
while [ $# -gt 0 ]; do
    case "$1" in
        --quick) quick="--quick" ;;
        --jobs) jobs="$2"; shift ;;
        -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
        *) out="$1" ;;
    esac
    shift
done
target="${CARGO_TARGET_DIR:-target}"
out="${out:-$target/crash-sweep}"
if ! command -v weston >/dev/null 2>&1; then
    if [ "${CRASH_SWEEP_APT:-}" = 1 ] && command -v apt-get >/dev/null 2>&1; then
        (apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq weston >/dev/null) 2>&1 | tail -3
    fi
    command -v weston >/dev/null 2>&1 || { echo "crash-sweep: needs weston (or CRASH_SWEEP_APT=1 in a container)" >&2; exit 2; }
fi
profile="--profile sweep"
echo "crash-sweep: build ($profile)"
# shellcheck disable=SC2086
cargo build $profile --bin pocket-ui --bin pocket-core
echo "crash-sweep: case-knows and card update fuzzers, ${CASE_KNOWS_FUZZ_ROUNDS:-20} rounds"
fuzz=0
# shellcheck disable=SC2086
CASE_KNOWS_FUZZ_ROUNDS="${CASE_KNOWS_FUZZ_ROUNDS:-20}" cargo test $profile --lib case_knows -- --test-threads=4 || fuzz=1
# The update file on the card (`update::card`), cut, changed and random: the look always answers and never offers a changed app.
# shellcheck disable=SC2086
CASE_KNOWS_FUZZ_ROUNDS="${CASE_KNOWS_FUZZ_ROUNDS:-20}" cargo test $profile --lib card_update_fuzz -- --test-threads=4 || fuzz=1
sweep=0
python3 -I scripts/crash-sweep.py --ui "$target/sweep/pocket-ui" --core "$target/sweep/pocket-core" --out "$out" --jobs "$jobs" $quick || sweep=$?
[ "$sweep" -le 1 ] || exit "$sweep"
cat "$out/report.md"
if [ "$fuzz" -ne 0 ] || [ "$sweep" -ne 0 ]; then
    echo "crash-sweep: FOUND something (fuzzer exit $fuzz, sweep exit $sweep; $out/report.md)"
    exit 1
fi
echo "crash-sweep: nothing found ($out/report.md)"
