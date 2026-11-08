#!/bin/sh
# Merge gate: the tests at release level (exit code, not grep), the Steam Deck and Android compile checks (scripts/check-platforms.sh), then the launcher tests, the verify-device test, the
# core-licence check's test, the install package's test and the quiet start script's test.
# Usage: scripts/gate.sh [--quick] [worktree]      (default: the repository this script is in)
#   GATE: ok (N passed)  exit 0     everything passed
#   GATE: ... FAILED     exit 1     the line says which part; the log is named in it
#   exit 2                          the worktree does not exist
# First step, both tiers (about 5 s): scripts/check-leaks.sh, the private-data scan of the export, on the tracked tree. A new
# leak (owner name, home path, host name, MAC or private address) fails here with the file:line list, before the long build.
# Logs: /tmp/gate-test-<worktree>.log, /tmp/gate-launcher-<worktree>.log and /tmp/gate-verify-<worktree>.log (one set per worktree, so gates that run at
# the same time don't overwrite each other's), copied to /tmp/gate-test.log, /tmp/gate-launcher.log and
# /tmp/gate-verify.log at the end.
#
# What is different from a plain `cargo test --release` (docs/DEVELOPING.md, "Merge gate"):
#  - the tests are built with [profile.gate] in Cargo.toml: release's optimisation level and checks (opt-level 3, no
#    debug assertions, no overflow checks), but no LTO, 16 codegen units and incremental compilation. Nothing that
#    ships is built with it: the app and the release builds keep [profile.release].
#  - the shell tests (launcher test in one lane, the small ones in another) run beside cargo on hosts with 8+ cores
#    after compilation and the serial Rust timing tier. They never retry a red verdict.
#  - paths changed against the merge base with main (GATE_BASE=<ref>; a remote gate: the list comes from tools/remote-gate.sh)
#    select a short gate only for strictly allowlisted, unembedded documents (scripts/gate-select.py).
#    Source, scripts, runtime assets/translations/manual, Cargo, unknown and mixed changes run every tier.
#    No list, or GATE_FULL=1:
#    everything runs. GATE_LEAKS_DONE=1 (set by remote-gate.sh, which scans where .git is; the remote host has no .git) skips the scan.
#  - GATE_PROFILE=release runs the tests with the production profile (thin LTO, one codegen unit, panic=abort for the
#    binaries): slow, use it when you changed [profile.*] or want the production binaries under test.
export PATH=$HOME/.cargo/bin:$HOME/opt/zig:$PATH
quick=0
if [ "${1:-}" = --quick ]; then quick=1; shift; fi
case ${1:-} in --*) echo "usage: scripts/gate.sh [--quick] [worktree]" >&2; exit 2 ;; esac
dir=${1:-$(cd "$(dirname "$0")/.." && pwd)}
cd "$dir" || exit 2
# Refresh source and embedded inputs after rsync, while the remote caller holds the shared target slot lock.
for input in src tests assets; do [ ! -d "$input" ] || find "$input" -type f -exec touch {} + || exit 1; done
name=$(basename "$(pwd)")
T=/tmp/gate-test-$name.log L=/tmp/gate-launcher-$name.log V=/tmp/gate-verify-$name.log C=/tmp/gate-licences-$name.log
P=/tmp/gate-package-$name.log Q=/tmp/gate-grub-$name.log G=/tmp/gate-image-grub-$name.log F=/tmp/gate-fat-hook-$name.log
# Private-data scan (fails fast, before any compile). The script is maintainer tooling and is not in the public export.
if [ -f scripts/check-leaks.sh ] && [ -z "${GATE_LEAKS_DONE:-}" ]; then
    leaks_start=$(date +%s)
    leaks=$(sh scripts/check-leaks.sh "$(pwd)" 2>&1) || {
        echo "GATE: leak scan FAILED (scripts/check-leaks.sh; docs/DEVELOPING.md \"Private-data scan\")"
        echo "$leaks"
        exit 1
    }
    [ -z "$leaks" ] || echo "$leaks"
    echo "gate: leak scan ok ($(( $(date +%s) - leaks_start )) s)"
fi
if [ "$quick" -eq 1 ]; then profile=${GATE_PROFILE:-quick}; else profile=${GATE_PROFILE:-gate}; fi
case $profile in release) pflag=--release ;; *) pflag="--profile $profile" ;; esac

# Keep shared builders responsive, even when the caller forgot the environment.
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
# Don't double the priority adjustment when the caller already used `nice -n 10`.
current_nice=$(ps -o nice= -p $$ 2>/dev/null | tr -d ' ')
case $current_nice in ''|*[!0-9-]*) current_nice=0 ;; esac
nice_increment=$((10 - current_nice))
[ "$nice_increment" -ge 0 ] || nice_increment=0
if [ "$quick" -eq 1 ]; then
    T=/tmp/gate-quick-$name.log
    start=$(date +%s)
    # Full unit/doc-drift coverage. No snapshot matrix or wall-clock shell watchdogs here.
    nice -n "$nice_increment" cargo test $pflag --lib --bins -q >"$T" 2>&1 || {
        echo "QUICK: unit tests FAILED ($T)"; tail -20 "$T"; exit 1;
    }
    # All branch changes plus local edits; override the comparison base explicitly when needed.
    suites=$(python3 scripts/quick-tests.py "${GATE_BASE:-main}") || exit 1
    # Only this tier narrows render matrices; the merge gate never sets this variable.
    if [ -n "$suites" ]; then
        extra_states=$(python3 scripts/quick-tests.py --states "${GATE_BASE:-main}") || exit 1
        POCKETUI_TEST_QUICK=1 POCKETUI_TEST_STATES="$extra_states" nice -n "$nice_increment" cargo test $pflag $suites -q >>"$T" 2>&1 || {
            echo "QUICK: integration tests FAILED ($T)"; tail -30 "$T"; exit 1;
        }
    fi
    shell_suites=$(python3 scripts/quick-tests.py --shell "${GATE_BASE:-main}") || exit 1
    for suite in $shell_suites; do
        case $suite in *licences*) interpreter=bash ;; *) interpreter=sh ;; esac
        nice -n "$nice_increment" "$interpreter" "$suite" >>"$T" 2>&1 || {
            echo "QUICK: $suite FAILED ($T)"; tail -20 "$T"; exit 1;
        }
    done
    echo "QUICK: ok ($(( $(date +%s) - start )) s; full merge gate still required)"
    exit 0
fi

# Run selection regressions even when this branch would skip the shell suites. Fixture trees omit this script,
# preventing recursion; its cargo and shell commands are stand-ins and never build or access a device.
if [ -f scripts/test-gate-paths.sh ]; then
    paths_log=/tmp/gate-paths-$name.log
    sh scripts/test-gate-paths.sh >"$paths_log" 2>&1 || {
        echo "GATE: path-selection tests FAILED ($paths_log)"; tail -10 "$paths_log"; exit 1;
    }
fi
unset POCKETUI_TEST_QUICK
start=$(date +%s)

# What did the branch change? Against the merge base with main (committed, uncommitted and new files). A remote gate has no .git:
# tools/remote-gate.sh writes the list to .gate-changed there. Nothing known or an empty list: run everything.
changed=
if [ -f .gate-changed ]; then changed=$(cat .gate-changed)
elif base=$(git merge-base "${GATE_BASE:-main}" HEAD 2>/dev/null); then
    changed=$( { git diff --name-only "$base"; git ls-files -o --exclude-standard; } | sort -u )
fi
# The short path is a strict allowlist. Embedded manual/translation inputs and
# every unknown, source, script, image or Cargo change retain the complete gate.
docs_only=0 shell_tests=1 why_shell=full
if [ -n "$changed" ] && [ -z "${GATE_FULL:-}" ] && [ -f scripts/gate-select.py ]; then
    mode=$(printf '%s\n' "$changed" | python3 scripts/gate-select.py "$(pwd)") || exit 1
    [ "$mode" != docs ] || docs_only=1
fi
if [ "$docs_only" -eq 1 ]; then
    # These are the same active document, link/index and translation contracts
    # as the full gate. No player test is ignored or moved to another tier.
    : >"$T"
    for filter in doc_checks:: i18n::; do
        part=$(mktemp) || exit 1
        if nice -n "$nice_increment" cargo test $pflag --lib --bins "$filter" -q >"$part" 2>&1 &&
            awk '/^test result: ok\./ { passed += $4 } END { exit !(passed > 0) }' "$part"; then
            cat "$part" >>"$T"
            rm -f "$part"
        else
            cat "$part" >>"$T"
            rm -f "$part"
            echo "GATE: documentation/i18n checks FAILED ($filter; $T; empty checks also fail)"
            tail -20 "$T"
            exit 1
        fi
    done
    echo "gate: docs-only; document/link/index+i18n checks $(( $(date +%s) - start )) s; leak scan checked before compilation"
    echo "gate: SKIPPED player, Brick, timing, platform and shell tiers: strictly unembedded documentation only"
    short_passed=$(awk '/^test result: ok\./ { passed += $4 } END { print passed + 0 }' "$T") || exit 1
    echo "GATE: ok ($short_passed passed)"
    exit 0
fi

# The shell tests: the launcher test and the small ones. Each has its own log and exit-code file in $RCD.
RCD=$(mktemp -d "${TMPDIR:-/tmp}/gate-rc.XXXXXX") || { echo "GATE: cannot create shell test result directory"; exit 1; }
# A fresh scope per gate; no render is reused across runs or different binaries.
export POCKETUI_SNAPSHOT_CACHE_DIR="$RCD/snapshots"
shell_launcher() { sh deploy/test-run-pocket-ui.sh >"$L" 2>&1; echo $? >"$RCD/l"; }
shell_small() {
    # The device check's shell test (fake ssh and /proc, about 3 s): scripts/verify-device.sh, docs/DEVELOPING.md "Deploy".
    sh scripts/test-verify-device.sh >"$V" 2>&1; echo $? >"$RCD/v"
    # The core-licence check's shell test (fixture trees, about 1 s): scripts/check-core-licences.sh must keep reading src/hostsys.rs.
    bash scripts/test-check-core-licences.sh >"$C" 2>&1; echo $? >"$RCD/c"
    # The install package's scripts against a fake root (about 8 s): deploy/package/install.sh and uninstall.sh.
    sh deploy/package/test-package.sh >"$P" 2>&1; echo $? >"$RCD/p"
    # The quiet start's script against a fake /flash (about 1 s): deploy/grub-quiet.sh.
    sh deploy/test-grub-quiet.sh >"$Q" 2>&1; echo $? >"$RCD/q"
    # The SD image check's reading of grub.cfg (about 1 s): scripts/image/grub-checks.sh against deploy/sd-efi/grub.cfg.
    # Plus the family parameter of scripts/image/rocknix-source.sh (--dry-run, nothing fetched).
    { sh scripts/image/test-grub-checks.sh && sh scripts/image/test-rocknix-source.sh && sh scripts/image/test-h700-pins.sh; } >"$G" 2>&1; echo $? >"$RCD/g"
    # The FAT-card install hook against a fake /flash and /storage (about 2 s): deploy/fat/mount-storage.sh.in; then the
    # SD image's boot hook with its update guard (about 1 s): deploy/image/mount-storage.sh.
    { sh deploy/fat/test-hook.sh && sh deploy/image/test-mount-storage.sh; } >"$F" 2>&1; echo $? >"$RCD/f"
}
shell_tests_run() { shell_launcher; shell_small; }
shell_rc() {
    # A skipped suite has no result by design; a selected suite without a complete result never passed.
    if [ "$shell_tests" -eq 0 ]; then echo 0; return; fi
    result=$(cat "$RCD/$1" 2>/dev/null)
    case $result in ''|*[!0-9]*) echo 125 ;; *) echo "$result" ;; esac
}

# Deadline and watchdog scenarios run after cargo, in a separate serial tier.
# A retry would conceal a scheduling assumption; a red verdict stays red.
cores=$(nproc 2>/dev/null || echo 1)

# shellcheck disable=SC2086  # pflag is one or two words
runner=${GATE_RUNNER:-auto}
# auto: nextest when it is installed (one process per test, all test binaries side by side, one retry), else cargo test.
if [ "$runner" = auto ]; then
    if cargo nextest --version >/dev/null 2>&1 && [ -f .config/nextest.toml ]; then runner=nextest; else runner=cargo; fi
fi
case $runner in
    cargo)
        if [ "$docs_only" -eq 1 ]; then
            # Unit tests only: they hold the doc-drift checks (src/doc_checks.rs); no source changed, so cargo has nothing to build.
            nice -n "$nice_increment" cargo test $pflag --lib --bins -q >"$T" 2>&1
        else
            nice -n "$nice_increment" cargo test $pflag -q >"$T" 2>&1
        fi
        trc=$?
        ;;
    nextest)
        # .config/nextest.toml: exactly ONE retry. A test that passes on the retry is "flaky": listed below, the gate stays
        # green. A test that fails twice makes the gate red. The test threads come from GATE_TEST_THREADS (default: the cores, at most 8).
        nthreads=$cores; [ "$nthreads" -le 8 ] || nthreads=8   # a shared computer: remote-gate.sh passes GATE_TEST_THREADS
        export NEXTEST_TEST_THREADS=${GATE_TEST_THREADS:-${NEXTEST_TEST_THREADS:-$nthreads}}
        if [ "$docs_only" -eq 1 ]; then ntargets="--lib --bins"; else ntargets=; fi
        nice -n "$nice_increment" cargo nextest run --cargo-profile "$profile" $ntargets --no-fail-fast \
            --status-level retry --final-status-level flaky >"$T" 2>&1
        trc=$?
        # nextest doesn't execute Rust documentation examples. Keep the complete gate's doctest protection.
        if [ "$docs_only" -eq 0 ]; then nice -n "$nice_increment" cargo test $pflag --doc -q >>"$T" 2>&1 || trc=1; fi
        ;;
    *) echo "GATE: unknown GATE_RUNNER (use auto, cargo or nextest)"; exit 2 ;;
esac
runner_done=$(date +%s)
# The ordinary library tier covers every captured public controller. The private
# Brick feature must run the SAME corpus too: otherwise a default gate can miss an
# ID collision that only changes the built-in Brick's F keys. Fixture gate trees and
# public exports without this feature keep their ordinary integration tier.
if [ "$trc" -eq 0 ] && [ "$docs_only" -eq 0 ] && [ -f src/input_corpus_tests.rs ] && grep -Eq '^[[:space:]]*brick[[:space:]]*=' Cargo.toml; then
    nice -n "$nice_increment" cargo test $pflag --features brick --lib input_corpus_tests -q >>"$T" 2>&1 || trc=1
    # The ordinary integration tier excludes the Brick settings walk and the
    # Brick-only text regressions. Run those cases, keeping the public text matrix in its usual tier. The Smart Pro's
    # D-pad explorer (same card, 1280x720; its text fit is in brick_settings_walk) runs here too.
    for suite in brick_settings_walk text_size long_titles search_titles nav_explorer; do
        [ "$trc" -eq 0 ] || break
        [ -f "tests/$suite.rs" ] || continue
        case $suite in text_size) filter=textfit_brick_031_ ;; long_titles) filter=info_titles_031_ ;; nav_explorer) filter=on_the_smart_pro ;; *) filter= ;; esac
        nice -n "$nice_increment" cargo test $pflag --features brick --test "$suite" $filter -q >>"$T" 2>&1 || trc=1
    done
fi
brick_done=$(date +%s)
# Timing cases must measure a real renderer, not a stored static layout probe.
unset POCKETUI_SNAPSHOT_CACHE_DIR
# The ignored timing tier is mandatory for both cargo and nextest. It gets one
# attempt, one test thread, and runs after compilation; failure stays red.
if [ "$trc" -eq 0 ]; then
    if [ "$docs_only" -eq 1 ]; then timing_targets="--lib --bins"; else timing_targets=; fi
    nice -n "$nice_increment" sh scripts/test-timing.sh "$profile" $timing_targets >>"$T" 2>&1 || trc=1
fi
timing_done=$(date +%s)
# The Steam Deck's and Android's own code (scripts/check-platforms.sh): checks, not builds; after the tests. Android is
# skipped where no NDK is installed; GATE_ANDROID_TARGET_DIR (set by the remote gate) keeps its files out of the host target dir.
prc_platforms=0 platforms_log=/tmp/gate-platforms-$name.log
if [ "$docs_only" -eq 0 ] && [ -f scripts/check-platforms.sh ]; then
    nice -n "$nice_increment" sh scripts/check-platforms.sh >"$platforms_log" 2>&1 || prc_platforms=1
fi
tdone=$(date +%s)
sdone=$tdone
shell_run_rc=0
if [ "$shell_tests" -eq 1 ]; then
    # Supervise a child even in the serial tier: an interrupted coordinator must
    # not turn missing exit-code files into success.
    shell_tests_run & shell_pid=$!
    wait "$shell_pid" || shell_run_rc=$?
    sdone=$(date +%s)
fi
lrc=$(shell_rc l) vrc=$(shell_rc v) crc=$(shell_rc c) prc=$(shell_rc p) qrc=$(shell_rc q) grc=$(shell_rc g) frc=$(shell_rc f)
cache_rc=0
if [ -f "$RCD/snapshots/stats.jsonl" ]; then
    cp "$RCD/snapshots/stats.jsonl" "/tmp/gate-snapshots-$name.jsonl" || cache_rc=1
    python3 - "$RCD/snapshots/stats.jsonl" <<'CACHE' || cache_rc=1
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1])]
hits = sum(bool(row.get("hit")) for row in rows)
print(f"gate: shared snapshots {len(rows)} requests, {hits} reused, {len(rows)-hits} renderer starts")
CACHE
fi
rm -rf "$RCD"
end=$(date +%s)
cp "$T" /tmp/gate-test.log
if [ "$shell_tests" -eq 1 ]; then
    cp "$L" /tmp/gate-launcher.log
    cp "$V" /tmp/gate-verify.log
fi
if [ "$shell_tests" -eq 1 ]; then
    echo "gate: profile $profile; tests $((tdone - start)) s, shell tests (launcher etc.) $((sdone - start)) s$([ -n "$why_shell" ] && echo "; run because of $why_shell"), total $((end - start)) s"
else
    echo "gate: profile $profile; tests $((tdone - start)) s, total $((end - start)) s"
fi
echo "gate: stages runner+doc $((runner_done - start)) s, brick corpus+UI $((brick_done - runner_done)) s, serial timing $((timing_done - brick_done)) s, platforms $((tdone - timing_done)) s, shells $((sdone - tdone)) s"
if [ "$docs_only" -eq 1 ]; then
    echo "gate: SKIPPED integration tests: only documents changed ($(echo "$changed" | wc -l) files, none read by the Rust build or the manual); unit tests with the doc checks ran"
fi
if [ "$shell_tests" -eq 0 ]; then
    echo "gate: SKIPPED launcher and other shell tests: only Markdown or Rust sources not named by shell scripts changed (GATE_FULL=1 runs everything)"
fi

fail=0
if [ "$cache_rc" -ne 0 ]; then
  echo "GATE: snapshot cache accounting FAILED"
  fail=1
fi
if [ "$shell_run_rc" -ne 0 ]; then
  echo "GATE: shell test coordinator FAILED (exit $shell_run_rc; incomplete run)"
  fail=1
fi
if [ $trc -ne 0 ]; then
  echo "GATE: tests FAILED ($T)"
  grep -E "FAILED|panicked|^error|^ *(TRY 2 )?FAIL " "$T" | head -10
  fail=1
fi
if [ $prc_platforms -ne 0 ]; then
  echo "GATE: platform check FAILED ($platforms_log)"
  head -20 "$platforms_log"
  fail=1
elif [ -s "$platforms_log" ] && [ "$docs_only" -eq 0 ]; then
  sed 's/^check-platforms:/gate:/' "$platforms_log"
fi
if [ $lrc -ne 0 ]; then
  echo "GATE: launcher tests FAILED ($L)"
  tail -5 "$L"
  fail=1
fi
if [ $vrc -ne 0 ]; then
  echo "GATE: verify-device tests FAILED ($V)"
  tail -5 "$V"
  fail=1
fi
if [ $crc -ne 0 ]; then
  echo "GATE: core-licence check tests FAILED ($C)"
  tail -5 "$C"
  fail=1
fi
if [ $prc -ne 0 ]; then
  echo "GATE: package tests FAILED ($P)"
  grep FAIL "$P" | head -5
  fail=1
fi
if [ $qrc -ne 0 ]; then
  echo "GATE: grub-quiet tests FAILED ($Q)"
  grep FAIL "$Q" | head -5
  fail=1
fi
if [ $grc -ne 0 ]; then
  echo "GATE: image grub.cfg check tests FAILED ($G)"
  grep FAIL "$G" | head -5
  fail=1
fi
if [ $frc -ne 0 ]; then
  echo "GATE: FAT install / SD image boot hook tests FAILED ($F)"
  grep FAIL "$F" | head -5
  fail=1
fi
# Tests that failed once and passed on the retry (nextest runner): green, but listed, so they get fixed.
flaky=$(grep -E '^ *FLAKY ' "$T" 2>/dev/null | sed 's/^ *FLAKY *[0-9]*\/[0-9]* *\[[^]]*\] *([^)]*) *//' | sort -u)
if [ -n "$flaky" ]; then
  echo "gate: flaky (failed once, passed on the one retry): $(echo "$flaky" | wc -l)"
  echo "$flaky" | sed 's/^/gate:   flaky: /'
fi
[ $fail -eq 0 ] || exit 1
if [ "$runner" = nextest ]; then
    summary=$(sed -n 's/.*tests run: \([0-9][0-9]*\) passed.*/\1 passed/p' "$T" | tail -1)
else
    summary=$(awk '/^test result: ok\./ { n += $4 } END { print n " passed" }' "$T")
fi
echo "GATE: ok (${summary:-tests passed})"
