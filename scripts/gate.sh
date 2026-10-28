#!/bin/sh
# Merge gate: the tests at release level (exit code, not grep), then the launcher tests, the verify-device test, the
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
#  - the launcher tests run after cargo, not alongside it: several of them (Smooth fan's watchdog) time real
#    intervals of 0.1-0.6 s and failed under the load of the compile (2026-10-24). They take about 50 s.
#  - GATE_PROFILE=release runs the tests with the production profile (thin LTO, one codegen unit, panic=abort for the
#    binaries): slow, use it when you changed [profile.*] or want the production binaries under test.
export PATH=$HOME/.cargo/bin:$HOME/opt/zig:$PATH
quick=0
if [ "${1:-}" = --quick ]; then quick=1; shift; fi
case ${1:-} in --*) echo "usage: scripts/gate.sh [--quick] [worktree]" >&2; exit 2 ;; esac
dir=${1:-$(cd "$(dirname "$0")/.." && pwd)}
cd "$dir" || exit 2
name=$(basename "$(pwd)")
T=/tmp/gate-test-$name.log L=/tmp/gate-launcher-$name.log V=/tmp/gate-verify-$name.log C=/tmp/gate-licences-$name.log
P=/tmp/gate-package-$name.log Q=/tmp/gate-grub-$name.log G=/tmp/gate-image-grub-$name.log F=/tmp/gate-fat-hook-$name.log
# Private-data scan (fails fast, before any compile). The script is maintainer tooling and is not in the public export.
if [ -f scripts/check-leaks.sh ]; then
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

unset POCKETUI_TEST_QUICK
start=$(date +%s)
# shellcheck disable=SC2086  # pflag is one or two words
runner=${GATE_RUNNER:-cargo}
case $runner in
    cargo)
        nice -n "$nice_increment" cargo test $pflag -q >"$T" 2>&1
        trc=$?
        ;;
    nextest)
        nice -n "$nice_increment" cargo nextest run --cargo-profile "$profile" >"$T" 2>&1
        trc=$?
        # nextest doesn't execute Rust documentation examples. Keep the complete gate's doctest protection.
        nice -n "$nice_increment" cargo test $pflag --doc -q >>"$T" 2>&1 || trc=1
        ;;
    *) echo "GATE: unknown GATE_RUNNER (use cargo or nextest)"; exit 2 ;;
esac
tdone=$(date +%s)
sh deploy/test-run-pocket-ui.sh >"$L" 2>&1
lrc=$?
# The device check's shell test (fake ssh and /proc, about 3 s): scripts/verify-device.sh, docs/DEVELOPING.md "Deploy".
sh scripts/test-verify-device.sh >"$V" 2>&1
vrc=$?
# The core-licence check's shell test (fixture trees, about 1 s): scripts/check-core-licences.sh must keep reading src/hostsys.rs.
bash scripts/test-check-core-licences.sh >"$C" 2>&1
crc=$?
# The install package's scripts against a fake root (about 8 s): deploy/package/install.sh and uninstall.sh.
sh deploy/package/test-package.sh >"$P" 2>&1
prc=$?
# The quiet start's script against a fake /flash (about 1 s): deploy/grub-quiet.sh.
sh deploy/test-grub-quiet.sh >"$Q" 2>&1
qrc=$?
# The SD image check's reading of grub.cfg (about 1 s): scripts/image/grub-checks.sh against deploy/sd-efi/grub.cfg.
sh scripts/image/test-grub-checks.sh >"$G" 2>&1
grc=$?
# The FAT-card install hook against a fake /flash and /storage (about 2 s): deploy/fat/mount-storage.sh.in.
sh deploy/fat/test-hook.sh >"$F" 2>&1
frc=$?
end=$(date +%s)
cp "$T" /tmp/gate-test.log
cp "$L" /tmp/gate-launcher.log
cp "$V" /tmp/gate-verify.log
echo "gate: profile $profile; tests $((tdone - start)) s, launcher tests $((end - tdone)) s, total $((end - start)) s"

fail=0
if [ $trc -ne 0 ]; then
  echo "GATE: tests FAILED ($T)"
  grep -E "FAILED|panicked|^error" "$T" | head -10
  fail=1
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
  echo "GATE: FAT install hook tests FAILED ($F)"
  grep FAIL "$F" | head -5
  fail=1
fi
[ $fail -eq 0 ] || exit 1
if [ "$runner" = nextest ]; then
    summary=$(sed -n 's/.*tests run: \([0-9][0-9]*\) passed.*/\1 passed/p' "$T" | tail -1)
else
    summary=$(grep -o '[0-9]* passed' "$T" | head -1)
fi
echo "GATE: ok (${summary:-tests passed})"
