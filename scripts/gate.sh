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
#  - the shell tests (launcher test in one lane, the small ones in another) run beside cargo on hosts with 8+ cores
#    (GATE_PARALLEL=0 or fewer cores: after it). Several launcher tests (Smooth fan's watchdog) time real intervals of
#    0.1-0.6 s and failed under the load of the compile (2026-10-24); if any shell test fails beside cargo, all of them
#    run again alone and that result counts. They take about 50 s on a quiet machine, about 3 min on the remote host.
#  - paths changed against the merge base with main (GATE_BASE=<ref>; server2: the list comes from tools/remote-gate.sh)
#    skip what cannot be affected, and say so ("gate: SKIPPED ..."): shell tests may skip ordinary Markdown and Rust
#    sources no shell script names. Licence files and every unknown path run them. The integration tests (unit tests and
#    doc checks still run) skip when only docs/ Markdown (not docs/user/, the manual), root *.md or LICENSE changed.
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

# What did the branch change? Against the merge base with main (committed, uncommitted and new files). Server2 has no .git:
# tools/remote-gate.sh writes the list to .gate-changed there. Nothing known or an empty list: run everything.
changed=
if [ -f .gate-changed ]; then changed=$(cat .gate-changed)
elif base=$(git merge-base "${GATE_BASE:-main}" HEAD 2>/dev/null); then
    changed=$( { git diff --name-only "$base"; git ls-files -o --exclude-standard; } | sort -u )
fi
# Every skipped path must be known harmless. Source references include all shell scripts, not just the test entry
# points: e.g. check-core-licences.sh reads src/hostsys.rs. Other assets, configuration and unknown paths run the suite.
# The Rust-only sources are safe here because shell tests use stand-in binaries rather than compiling/running them.
docs_only=0 shell_tests=1 why_shell=
if [ -n "$changed" ] && [ -z "${GATE_FULL:-}" ]; then
    srcrefs=$(find deploy scripts -type f -name '*.sh' -exec grep -hoE 'src/[a-zA-Z0-9_./-]+\.rs' {} + 2>/dev/null | sort -u)
    docs_only=1 shell_tests=0
    while IFS= read -r f; do
        case $f in
            docs/user/*) docs_only=0 ;;
            docs/*.md|LICENSE) ;;
            */*) docs_only=0 ;;
            *.md) ;;
            *) docs_only=0 ;;
        esac
        case $f in
            LICENSE|NOTICE.md|assets/fonts/OFL.txt) shell_tests=1; why_shell=$f ;;
            docs/*.md) ;;
            src/*.rs) case "
$srcrefs
" in *"
$f
"*) shell_tests=1; why_shell=$f ;; esac ;;
            */*) shell_tests=1; why_shell=$f ;;
            *.md) ;;
            *) shell_tests=1; why_shell=$f ;;
        esac
    done <<EOF
$changed
EOF
fi

# The shell tests: the launcher test and the small ones. Each has its own log and exit-code file in $RCD.
RCD=$(mktemp -d "${TMPDIR:-/tmp}/gate-rc.XXXXXX") || { echo "GATE: cannot create shell test result directory"; exit 1; }
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
    sh scripts/image/test-grub-checks.sh >"$G" 2>&1; echo $? >"$RCD/g"
    # The FAT-card install hook against a fake /flash and /storage (about 2 s): deploy/fat/mount-storage.sh.in.
    sh deploy/fat/test-hook.sh >"$F" 2>&1; echo $? >"$RCD/f"
}
shell_tests_run() { shell_launcher & shell_small; wait; }   # two lanes: the launcher (the longest) and all the small ones
shell_rc() {
    # A skipped suite has no result by design; a selected suite without a complete result never passed.
    if [ "$shell_tests" -eq 0 ]; then echo 0; return; fi
    result=$(cat "$RCD/$1" 2>/dev/null)
    case $result in ''|*[!0-9]*) echo 125 ;; *) echo "$result" ;; esac
}
shell_failed() { for k in l v c p q g f; do [ "$(shell_rc $k)" = 0 ] || return 0; done; return 1; }

# The shell tests build nothing and use no port or shared directory (mktemp, fakes), so they run beside cargo when the host has
# cores to spare (GATE_PARALLEL=0 or fewer than 8 cores: one after the other, as before). If they fail beside the compile
# (the launcher's watchdog tests time real 0.1-0.6 s intervals), they run again alone after cargo, and that result counts.
cores=$(nproc 2>/dev/null || echo 1)
parallel=0
[ "$shell_tests" -eq 1 ] && [ "${GATE_PARALLEL:-1}" != 0 ] && [ "$cores" -ge 8 ] && parallel=1
if [ "$parallel" -eq 1 ]; then shell_tests_run & shell_pid=$!; fi   # the launcher and the small tests are independent of each other too

# shellcheck disable=SC2086  # pflag is one or two words
runner=${GATE_RUNNER:-cargo}
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
        nice -n "$nice_increment" cargo nextest run --cargo-profile "$profile" >"$T" 2>&1
        trc=$?
        # nextest doesn't execute Rust documentation examples. Keep the complete gate's doctest protection.
        nice -n "$nice_increment" cargo test $pflag --doc -q >>"$T" 2>&1 || trc=1
        ;;
    *) echo "GATE: unknown GATE_RUNNER (use cargo or nextest)"; exit 2 ;;
esac
tdone=$(date +%s)
sdone=$tdone
shell_run_rc=0
if [ "$parallel" -eq 1 ]; then
    wait "$shell_pid" || shell_run_rc=$?
    sdone=$(date +%s)
    # An interrupted coordinator can leave a lane alive. Fail this run; do not race a retry against it.
    if [ "$shell_run_rc" -eq 0 ] && shell_failed; then
        echo "gate: shell tests failed beside cargo, running them again alone (first logs: $L.parallel ...)"
        cp "$L" "$L.parallel"
        shell_tests_run
        sdone=$(date +%s)
    fi
elif [ "$shell_tests" -eq 1 ]; then
    shell_tests_run
    sdone=$(date +%s)
fi
lrc=$(shell_rc l) vrc=$(shell_rc v) crc=$(shell_rc c) prc=$(shell_rc p) qrc=$(shell_rc q) grc=$(shell_rc g) frc=$(shell_rc f)
rm -rf "$RCD"
end=$(date +%s)
cp "$T" /tmp/gate-test.log
if [ "$shell_tests" -eq 1 ]; then
    cp "$L" /tmp/gate-launcher.log
    cp "$V" /tmp/gate-verify.log
fi
if [ "$shell_tests" -eq 1 ]; then
    echo "gate: profile $profile; tests $((tdone - start)) s, shell tests (launcher etc.) $((sdone - start)) s$([ "$parallel" -eq 1 ] && echo ' beside cargo')$([ -n "$why_shell" ] && echo "; run because of $why_shell"), total $((end - start)) s"
else
    echo "gate: profile $profile; tests $((tdone - start)) s, total $((end - start)) s"
fi
if [ "$docs_only" -eq 1 ]; then
    echo "gate: SKIPPED integration tests: only documents changed ($(echo "$changed" | wc -l) files, none read by the Rust build or the manual); unit tests with the doc checks ran"
fi
if [ "$shell_tests" -eq 0 ]; then
    echo "gate: SKIPPED launcher and other shell tests: only Markdown or Rust sources not named by shell scripts changed (GATE_FULL=1 runs everything)"
fi

fail=0
if [ "$shell_run_rc" -ne 0 ]; then
  echo "GATE: shell test coordinator FAILED (exit $shell_run_rc; incomplete run)"
  fail=1
fi
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
