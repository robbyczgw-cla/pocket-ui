#!/bin/sh
# Merge gate, two tiers (docs/dev/merge-gate.md "Merge gate"). Default: the fast tier (target 8-10 min on the train host) - every
# program compiles, unit tests, doctests, the integration suites except the slow layout matrices and the time budgets, plus what
# the change touches (scripts/gate-parts.py), the layout matrices as their sample (POCKETUI_TEST_QUICK). GATE_FULL=1 (nightly on
# main, scripts/release-check.sh): the tests at release level with their whole matrices (exit code, not grep), the time budgets
# alone, the serial timing tier, the Brick corpus and UI walk and the German/Spanish matrix,
# then the launcher tests, the verify-device test, the
# core-licence check's test, the install package's test and the quiet start script's test.
# Usage: scripts/gate.sh [--quick] [worktree]      (default: the repository this script is in)
#   GATE: ok (N passed)  exit 0     everything passed
#   GATE: ... FAILED     exit 1     the line says which part; the log is named in it
#   exit 2                          the worktree does not exist
# First step, both tiers (about 5 s): scripts/check-leaks.sh, the private-data scan of the export, on the tracked tree. A new
# leak (owner name, home path, host name, MAC or private address) fails here with the file:line list, before the long build.
# The shared compile-first stage then checks Linux all-targets, Brick and Android before any tests.
# Logs: /tmp/gate-test-<worktree>.log, /tmp/gate-launcher-<worktree>.log and /tmp/gate-verify-<worktree>.log (one set per worktree, so gates that run at
# the same time don't overwrite each other's), copied to /tmp/gate-test.log, /tmp/gate-launcher.log and
# /tmp/gate-verify.log at the end.
#
# What is different from a plain `cargo test --release` (docs/dev/merge-gate.md, "Merge gate"):
#  - the tests are built with [profile.gate] in Cargo.toml: release's optimisation level and checks (opt-level 3, no
#    debug assertions, no overflow checks), but no LTO, 16 codegen units and incremental compilation. Nothing that
#    ships is built with it: the app and the release builds keep [profile.release].
#  - the shell tests (launcher test in one lane, the small ones in another) run beside cargo on hosts with 8+ cores
#    after compilation, on separate SMT groups of an exclusive train host. Timing starts after all lanes join. They never retry a red verdict.
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
# The runner guarantees one train per host only when EXCLUSIVE is set. CPU caps
# apply to every tier, including quick/docs; parallel lanes use whole SMT groups.
allocation=$(python3 scripts/gate-cpus.py) || exit 1
eval "$allocation"
if [ "${GATE_CPUSET+x}" = x ]; then
    taskset -pc "$gate_allowed" $$ >/dev/null || { echo "GATE: cannot enforce CPU cap"; exit 1; }
fi
# This run's own TMPDIR, removed when the gate ends (scripts/run-tmp.sh): a test that leaves folders behind can't fill /tmp
# across runs (14.11.: 8,700 leftovers, "Disk quota exceeded" in a train). Under 2 GB free it cleans old run folders first.
[ ! -f scripts/run-tmp.sh ] || { . scripts/run-tmp.sh; run_tmp_begin gate; }
# Refresh source and embedded inputs after rsync, while the remote caller holds the shared target slot lock.
# GATE_SOURCES_STAMPED=1 (tools/remote-gate.sh): its rsync --checksum --no-times already stamped exactly the files whose
# content differs, so cargo sees every changed file as new and every other one as built. Touching them all too would
# recompile the library and every test program for a change that only touched one test or one asset.
if [ -z "${GATE_SOURCES_STAMPED:-}" ]; then
    for input in src tests examples assets; do [ ! -d "$input" ] || find "$input" -type f -exec touch {} + || exit 1; done
fi
name=$(basename "$(pwd)")
T=/tmp/gate-test-$name.log L=/tmp/gate-launcher-$name.log V=/tmp/gate-verify-$name.log C=/tmp/gate-licences-$name.log
P=/tmp/gate-package-$name.log Q=/tmp/gate-grub-$name.log G=/tmp/gate-image-grub-$name.log F=/tmp/gate-fat-hook-$name.log
# Private-data scan (fails fast, before any compile). The script is maintainer tooling and is not in the public export.
if [ -f scripts/check-leaks.sh ] && [ -z "${GATE_LEAKS_DONE:-}" ]; then
    leaks_start=$(date +%s)
    leaks=$(sh scripts/check-leaks.sh "$(pwd)" 2>&1) || {
        echo "GATE: leak scan FAILED (scripts/check-leaks.sh; docs/dev/merge-gate.md \"Private-data scan\")"
        echo "$leaks"
        exit 1
    }
    [ -z "$leaks" ] || echo "$leaks"
    echo "gate: leak scan ok ($(( $(date +%s) - leaks_start )) s)"
fi
# Fast checks (19.11.2026, fail-fast): the script checks that turned the most trains red, before any build - seconds, not the
# 20 minutes until the suites reach the same verdict. The translation audit (a painted string without a German line) and the
# reader inventory (tests/data_sources.rs runs the same script later). Each only when the tree has it; all at once.
# 20.11.2026: the CHANGELOG guard (scripts/check-changelog.py): the queue's union merge had put copies of [Unreleased] lines
# back under the published [0.5.1] with a green gate. Here it checks the train's tree after that merge; --no-vetoed keeps
# its verdict the same on every gate host (the vetoed list is the maintainer's, the queue client checks it before queueing).
fast_started=$(date +%s) fast_pids= fast_names=
for fast_check in i18n-extract.py reader-inventory.py check-changelog.py test-check-changelog.py; do
    [ -f "scripts/$fast_check" ] || continue
    case $fast_check in i18n-extract.py) fast_args=--check ;; check-changelog.py) fast_args=--no-vetoed ;; *) fast_args= ;; esac
    # shellcheck disable=SC2086
    python3 -I "scripts/$fast_check" $fast_args >"/tmp/gate-fast-$name.$fast_check.log" 2>&1 &
    fast_pids="$fast_pids $!" fast_names="$fast_names $fast_check"
done
fast_failed=
set -- $fast_names
for fast_pid in $fast_pids; do
    wait "$fast_pid" || fast_failed="$fast_failed $1"
    shift
done
if [ -n "$fast_failed" ]; then
    for fast_check in $fast_failed; do
        echo "GATE: fast checks FAILED: scripts/$fast_check ($(( $(date +%s) - fast_started )) s; /tmp/gate-fast-$name.$fast_check.log)"
        tail -20 "/tmp/gate-fast-$name.$fast_check.log"
    done
    exit 1
fi
[ -z "$fast_names" ] || echo "gate: fast checks ok:$fast_names ($(( $(date +%s) - fast_started )) s)"
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
    # The render matrices as their sample, as in the merge gate (GATE_FULL=1 runs them whole).
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
# The comment-only lane (13.11.): a change whose every file differs from main only in plain comments (Rust, shell, Python) and
# unembedded documents needs no suites - the compile stage, the cheap checks and the gate's own self-tests are the whole gate.
# scripts/gate-comments.py proves it against the base versions. A remote gate has no .git: tools/remote-gate.sh runs the proof
# where .git is and writes .gate-lane next to .gate-changed. Anything but "comments" keeps the normal tiers.
comments_only=0
if [ "$docs_only" -eq 0 ] && [ -n "$changed" ] && [ -z "${GATE_FULL:-}" ] && [ -f scripts/gate-comments.py ]; then
    if [ -f .gate-changed ]; then lane=$(cat .gate-lane 2>/dev/null)
    else
        # Proved against the tree this train was built on (GATE_LANE_BASE): a train behind another one in flight carries that
        # one's changes, which its own gate tests. Without it: the same base as the change list.
        lbase=${GATE_LANE_BASE:-${GATE_BASE:-main}}
        lane=$( { git diff --name-only "$(git merge-base "$lbase" HEAD)"; git ls-files -o --exclude-standard; } 2>/dev/null | sort -u \
                | python3 -I scripts/gate-comments.py "$(pwd)" "$lbase" 2>/dev/null) || lane=
    fi
    [ "$lane" != comments ] || comments_only=1
fi
# The merge gate's fast tier (2026-11-10, target <= 8 min on the train host): every program compiles, the unit tests, every
# integration suite except the slow layout matrices, the ones the change touches (scripts/gate-parts.py), a Brick and
# Android compile checks, and the shell suites when a script or a path they read changed. The timing tier, the full Brick
# tier, the German/Spanish matrix and the other slow suites run when the change touches them, every night on
# main (tools/nightly-gate.sh) and before a release (scripts/release-check.sh): GATE_FULL=1 runs all of it.
tier=full heavy= skip= nightly_tests= parts_ran=0 brick_full=1 brick_text=1 i18n_matrix=1 android=1 android_skip=
if [ "$docs_only" -eq 0 ] && [ "$comments_only" -eq 0 ] && [ -n "$changed" ] && [ -z "${GATE_FULL:-}" ] && [ -f scripts/gate-parts.py ]; then
    # The English text of the translation lines the change touched: scripts/gate-parts.py sends a translation-only change to
    # the Android page tests only when one of those texts is drawn from a source Android uses. A remote gate gets the file
    # from tools/remote-gate.sh (.gate-i18n-changed); without it, or without git, every translation change counts.
    i18n_keys=
    if printf '%s\n' "$changed" | grep -Eq '^assets/i18n/(de|es|pt|fr|it)\.tsv$'; then
        if [ -f .gate-i18n-changed ]; then i18n_keys=$(pwd)/.gate-i18n-changed
        elif [ -n "${base:-}" ]; then
            i18n_keys=$(mktemp "${TMPDIR:-/tmp}/gate-i18n-keys.XXXXXX") || exit 1
            git diff -U0 "$base" -- assets/i18n/de.tsv assets/i18n/es.tsv assets/i18n/pt.tsv assets/i18n/fr.tsv assets/i18n/it.tsv | sed -n '/^+++/d; /^---/d; s/^[-+]\([^#\t][^\t]*\)\t.*/\1/p' | sort -u >"$i18n_keys"
        fi
    fi
    selection=$(printf '%s\n' "$changed" | GATE_I18N_KEYS="$i18n_keys" python3 scripts/gate-parts.py "$(pwd)") || exit 1
    eval "$selection"
    parts_ran=1
    case $i18n_keys in ''|"$(pwd)"/.gate-i18n-changed) ;; *) rm -f "$i18n_keys" ;; esac
fi
# The layout matrices (text_size, long_titles, the name and search suites, the German/Spanish snapshots) run as their sample in
# the merge gate: a dozen states, one size, the longest language, plus the states the change adds to tests/i18n-states.txt
# (a remote gate: .gate-states from tools/remote-gate.sh). Their whole matrix runs nightly and in the release check (GATE_FULL=1).
unset POCKETUI_TEST_QUICK POCKETUI_TEST_STATES
if [ -z "${GATE_FULL:-}" ] && [ "$docs_only" -eq 0 ] && [ "$comments_only" -eq 0 ] && [ -n "$changed" ]; then
    if [ -f .gate-states ]; then new_states=$(cat .gate-states)
    elif [ -n "${base:-}" ] && [ -f scripts/quick-tests.py ]; then new_states=$(python3 scripts/quick-tests.py --states "$base" 2>/dev/null) || new_states=
    else new_states=; fi
    export POCKETUI_TEST_QUICK=1 POCKETUI_TEST_STATES="$new_states"
    echo "gate: layout matrices as their sample (POCKETUI_TEST_QUICK; $(printf '%s' "$new_states" | grep -c .) added states too; the whole matrix runs nightly)"
fi
if [ "$tier" = fast ] || [ "$parts_ran" -eq 1 ]; then
    brick_what=check
    [ "$brick_text" -ne 1 ] || brick_what="text-fit cases"
    [ "$brick_full" -ne 1 ] || brick_what=tests
    echo "gate: $tier tier (GATE_FULL=1 runs everything); slow suites run: ${heavy:-none}; Brick $brick_what; Android page tests $([ "$android" -eq 1 ] && echo yes || echo "no (nightly)"); shells $([ "$shell_tests" -eq 1 ] && echo "yes ($why_shell)" || echo no)"
    echo "gate: left to the nightly run: the time budgets, the timing tier, the layout matrices beyond their sample${skip:+; suites $skip}${nightly_tests:+; $(echo $nightly_tests | wc -w) single tests (gate-parts.py NIGHTLY_TESTS)}"
fi
# Check every Rust configuration before any test or lane can start. In particular,
# examples and Android cfg errors must not wait for codegen or the player suites.
compile_check_started=$(date +%s)
platforms_log=/tmp/gate-platforms-$name.log
: >"$platforms_log"
if [ "$docs_only" -eq 0 ]; then
    # The remote queue already collects this log, including an early compiler failure.
    compile_log=$T
    nice -n "$nice_increment" sh scripts/check-compile.sh >"$compile_log" 2>&1 || {
        echo "GATE: compile-first FAILED ($(( $(date +%s) - start )) s; $compile_log)"
        tail -40 "$compile_log"
        exit 1
    }
    grep -E '^(compile-first:|check-platforms:)' "$compile_log"
fi
compile_check_done=$(date +%s)
# Cheap checks first (13.11.): the list scripts/preflight.sh gives every agent before queueing - translation audit and the
# doc_checks/document_check/manual/i18n unit tests - runs right after the compile stage, so the cheap reds that cost most trains
# show up here (minutes) and not after the full build and the suites. Same script, same list.
if [ "$docs_only" -eq 0 ] && [ -f scripts/preflight.sh ]; then
    cheap_started=$(date +%s)
    cheap_log=/tmp/gate-cheap-$name.log
    nice -n "$nice_increment" sh scripts/preflight.sh --cheap-tests "$(pwd)" >"$cheap_log" 2>&1 || {
        echo "GATE: cheap checks FAILED ($(( $(date +%s) - start )) s; $cheap_log; the list of scripts/preflight.sh)"
        tail -30 "$cheap_log"
        exit 1
    }
    echo "gate: cheap checks ok ($(( $(date +%s) - cheap_started )) s)"
fi

# Run selection regressions even when this branch would skip the shell suites. Fixture trees omit this script,
# preventing recursion; its cargo and shell commands are stand-ins and never build or access a device.
if [ -f scripts/test-gate-paths.sh ]; then
    paths_log=/tmp/gate-paths-$name.log
    sh scripts/test-gate-paths.sh >"$paths_log" 2>&1 || {
        echo "GATE: path-selection tests FAILED ($paths_log)"; tail -10 "$paths_log"; exit 1;
    }
fi
if [ -f scripts/test-preflight.sh ]; then
    preflight_log=/tmp/gate-preflight-$name.log
    sh scripts/test-preflight.sh >"$preflight_log" 2>&1 || {
        echo "GATE: preflight script tests FAILED ($preflight_log)"; tail -10 "$preflight_log"; exit 1;
    }
fi
if [ -f scripts/test-run-tmp.sh ]; then
    runtmp_log=/tmp/gate-run-tmp-$name.log
    sh scripts/test-run-tmp.sh >"$runtmp_log" 2>&1 || {
        echo "GATE: run-folder script tests FAILED ($runtmp_log)"; tail -10 "$runtmp_log"; exit 1;
    }
fi
if [ -f scripts/test-gate-schedule.py ]; then
    schedule_log=/tmp/gate-schedule-$name.log
    python3 scripts/test-gate-schedule.py >"$schedule_log" 2>&1 || {
        echo "GATE: scheduling fixtures FAILED ($schedule_log)"; tail -20 "$schedule_log"; exit 1;
    }
    tail -1 "$schedule_log"
fi
if [ -f scripts/test-gate-comments.py ]; then
    comments_log=/tmp/gate-comments-$name.log
    python3 -I scripts/test-gate-comments.py >"$comments_log" 2>&1 || {
        echo "GATE: comment-only proof tests FAILED ($comments_log)"; tail -20 "$comments_log"; exit 1;
    }
fi
if [ "$comments_only" -eq 1 ]; then
    cheap_passed=$(sed -n 's/.*cheap tests ok.*, \([0-9][0-9]*\) passed).*/\1/p' "$cheap_log" 2>/dev/null | tail -1)
    echo "gate: comment-only lane ($(( $(date +%s) - start )) s): every changed file differs from main only in comments or unembedded documents (scripts/gate-comments.py)"
    echo "gate: SKIPPED player, Brick, timing, platform and shell tiers; compile stage, cheap checks and the gate self-tests ran"
    echo "GATE: ok (${cheap_passed:-0} passed)"
    exit 0
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
trap 'rc=$?; rm -rf "$RCD"; if command -v run_tmp_end >/dev/null 2>&1; then run_tmp_end "$rc"; fi' EXIT
trap 'exit 143' HUP INT TERM
shell_tests_run() {
    sh scripts/gate-shell.sh launcher "$name" "$RCD"
    sh scripts/gate-shell.sh small "$name" "$RCD"
}
shell_run_rc=0 launcher_pid= small_pid=
shell_wait() {
    [ -z "$launcher_pid" ] || wait "$launcher_pid" || shell_run_rc=$?
    [ -z "$small_pid" ] || wait "$small_pid" || shell_run_rc=$?
}
run_rust() {
    if [ "$gate_parallel" -eq 1 ]; then taskset -c "$gate_normal" nice -n "$nice_increment" "$@"
    else nice -n "$nice_increment" "$@"; fi
}
shell_rc() {
    # A skipped suite has no result by design; a selected suite without a complete result never passed.
    if [ "$shell_tests" -eq 0 ]; then echo 0; return; fi
    result=$(cat "$RCD/$1" 2>/dev/null)
    case $result in ''|*[!0-9]*) echo 125 ;; *) echo "$result" ;; esac
}

# Deadline and watchdog scenarios run after cargo, in a separate serial tier.
# A retry would conceal a scheduling assumption; a red verdict stays red.
case ${GATE_RUNNER:-auto} in auto|cargo|nextest) ;; *) echo "GATE: unknown GATE_RUNNER (use auto, cargo or nextest)"; exit 2 ;; esac
cores=$gate_cores
# The merge gate runs only some integration suites (a full-tier change: all but the time budgets), but every test program still
# compiles (compile-first). GATE_FULL=1 builds and runs them all.
rust_targets=
# merge_full: scripts/gate-parts.py said "full" (Cargo, the gate's own scripts, the test harness) in a merge gate.
merge_full=0
[ "$tier" != full ] || [ "$parts_ran" -eq 0 ] || merge_full=1
if [ "$tier" = fast ] || [ "$merge_full" -eq 1 ]; then
    rust_targets="--lib --bins"
    for suite in tests/*.rs; do
        [ -f "$suite" ] || continue
        suite=$(basename "$suite" .rs)
        case " $skip " in *" $suite "*) ;; *) rust_targets="$rust_targets --test $suite" ;; esac
    done
fi
if [ "$gate_parallel" -eq 1 ] || [ -n "$rust_targets" ]; then
    # Finish compilation before watchdogs, and finish default tests before any
    # Brick build can overwrite the unversioned CARGO_BIN_EXE_pocket-ui.
    # The fast tier builds only the programs it runs: compile-first has already checked every other target (examples,
    # the skipped suites), and the nightly run builds them all.
    nice -n "$nice_increment" cargo test $pflag --no-run $rust_targets >"$T" 2>&1 || {
        echo "GATE: compilation FAILED ($T)"; tail -20 "$T"; exit 1;
    }
fi
compiled=$(date +%s)
if [ "$gate_parallel" -eq 1 ] && [ "$shell_tests" -eq 1 ]; then
    echo "gate: parallel on ${GATE_HOST:-host}; Rust CPUs $gate_normal; launcher $gate_launcher; small shells $gate_small"
    taskset -c "$gate_launcher" nice -n "$nice_increment" sh scripts/gate-shell.sh launcher "$name" "$RCD" & launcher_pid=$!
    taskset -c "$gate_small" nice -n "$nice_increment" sh scripts/gate-shell.sh small "$name" "$RCD" & small_pid=$!
elif [ "$gate_parallel" -eq 1 ]; then
    echo "gate: parallel on ${GATE_HOST:-host}; Rust CPUs $gate_normal; no shell suites selected"
else
    echo "gate: serial schedule (no exclusive host, disabled, or insufficient separate cores)"
fi

# Known flaky tests the merge gate leaves to the nightly run (docs/dev/merge-gate.md, "Flaky tests"): GATE_SKIP_TESTS names
# them as nextest prints them after the binary (`eject::tests::x`, `the_way_back_hands_over_y`); GATE_FULL=1 runs them.
flaky_skip=
[ -n "${GATE_FULL:-}" ] || flaky_skip=${GATE_SKIP_TESTS:-}
[ -z "$flaky_skip" ] || echo "gate: left to the nightly run (flaky): $flaky_skip"
# Single tests scripts/gate-parts.py leaves to the nightly run (NIGHTLY_TESTS), skipped the same way.
[ -n "${GATE_FULL:-}" ] || flaky_skip="$flaky_skip${nightly_tests:+ $nightly_tests}"
cargo_flaky=
for skipped in $flaky_skip; do cargo_flaky="$cargo_flaky --skip $skipped"; done
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
            run_rust cargo test $pflag --lib --bins -q ${cargo_flaky:+-- $cargo_flaky} >"$T" 2>&1
        elif [ -n "$rust_targets" ]; then
            cargo_skips=$cargo_flaky
            for skipped in $android_skip; do cargo_skips="$cargo_skips --skip ${skipped}::"; done
            # shellcheck disable=SC2086
            run_rust cargo test $pflag $rust_targets -q -- $cargo_skips >>"$T" 2>&1 && run_rust cargo test $pflag --doc -q >>"$T" 2>&1
        else
            run_rust cargo test $pflag -q ${cargo_flaky:+-- $cargo_flaky} >"$T" 2>&1
        fi
        trc=$?
        ;;
    nextest)
        # .config/nextest.toml: exactly ONE retry. A test that passes on the retry is "flaky": listed below, the gate stays
        # green. A test that fails twice makes the gate red. The test threads come from GATE_TEST_THREADS (default: the cores, at most 8).
        nthreads=$cores; [ "$nthreads" -le 8 ] || nthreads=8   # a shared computer: remote-gate.sh passes GATE_TEST_THREADS
        export NEXTEST_TEST_THREADS=${GATE_TEST_THREADS:-${NEXTEST_TEST_THREADS:-$nthreads}}
        if [ "$docs_only" -eq 1 ]; then ntargets="--lib --bins"; else ntargets=$rust_targets; fi
        # Library tests this change cannot reach wait for the nightly run (gate-parts.py "android").
        nextest_filter=
        for skipped in $android_skip; do nextest_filter="${nextest_filter:+$nextest_filter | }test(~${skipped}::)"; done
        for skipped in $flaky_skip; do nextest_filter="${nextest_filter:+$nextest_filter | }test(=${skipped})"; done
        if [ -n "$nextest_filter" ]; then set -- -E "not ($nextest_filter)"; else set --; fi
        run_rust cargo nextest run --cargo-profile "$profile" $ntargets --no-fail-fast "$@" \
            --status-level retry --final-status-level flaky >"$T" 2>&1
        trc=$?
        # nextest doesn't execute Rust documentation examples. Keep the complete gate's doctest protection.
        if [ "$docs_only" -eq 0 ]; then run_rust cargo test $pflag --doc -q >>"$T" 2>&1 || trc=1; fi
        ;;
    *) echo "GATE: unknown GATE_RUNNER (use auto, cargo or nextest)"; exit 2 ;;
esac
runner_done=$(date +%s)
# Join watchdogs before the next feature build too: disjoint CPUs do not
# isolate compilation I/O and memory pressure. Timing remains alone afterward.
if [ "$gate_parallel" -eq 1 ]; then shell_wait; fi
[ "$shell_run_rc" -eq 0 ] || trc=1
# Run the real Java readers too on JVM-capable hosts. The host-only API doubles
# exercise files/ZIPs/descriptors, not Android Binder or package acceptance.
# Existing minimal Rust-only hosts keep an explicit platform skip, not a new JDK package requirement.
if [ "$trc" -eq 0 ] && [ "$docs_only" -eq 0 ] && [ -f scripts/android/test/file-reader-rule.sh ]; then
    jvm_rc=0
    (unset JAVA_READER_ONLY; run_rust bash scripts/android/test/file-reader-rule.sh --if-available) >>"$T" 2>&1 || jvm_rc=$?
    case $jvm_rc in
        0) echo "gate: JVM file-reader companion passed" ;;
        77) echo "gate: JVM file-reader companion SKIPPED: no compatible JVM host; Java platform probes were not run" ;;
        *) echo "gate: JVM file-reader companion FAILED ($T)"; trc=1 ;;
    esac
fi
brick_started=$(date +%s)
# The German and Spanish snapshot matrix (~45 s) is a release check, but any full-gate change (a new screen, a Brick state, a
# manual page) can turn it red: run it in every full gate, so a release candidate does not find it first.
if [ "$trc" -eq 0 ] && [ "$docs_only" -eq 0 ] && [ "$i18n_matrix" -eq 1 ] && [ -f tests/i18n_snapshots.rs ]; then
    nice -n "$nice_increment" cargo test $pflag --test i18n_snapshots -q -- --include-ignored \
        every_string_in_the_german_snapshots_is_translated_or_a_name \
        every_string_in_the_spanish_snapshots_is_translated_or_a_name >>"$T" 2>&1 || trc=1
fi
# The ordinary library tier covers every captured public controller. The private
# Brick feature must run the SAME corpus too: otherwise a default gate can miss an
# ID collision that only changes the built-in Brick's F keys. Fixture gate trees and
# public exports without this feature keep their ordinary integration tier.
has_brick=0
if [ -f src/input_corpus_tests.rs ] && grep -Eq '^[[:space:]]*brick[[:space:]]*=' Cargo.toml; then has_brick=1; fi
# brick_full=0 with brick_text=1 (only translations or manual text changed): the Brick's text-fit cases run, the
# corpus, slider, navigation and title search wait for the nightly run (gate-parts.py).
if [ "$trc" -eq 0 ] && [ "$docs_only" -eq 0 ] && [ "$has_brick" -eq 1 ] && { [ "$brick_full" -eq 1 ] || [ "$brick_text" -eq 1 ]; }; then
    # The library's unit tests with the Brick feature are the Brick stage's biggest build (measured with two build jobs: 390 s of the
    # ~700 s); the text-fit cases below are integration tests and do not need it.
    if [ "$brick_full" -eq 1 ]; then
        nice -n "$nice_increment" cargo test $pflag --features brick --lib input_corpus_tests -q >>"$T" 2>&1 || trc=1
    fi
    # The Settings writer and the card's hardware watcher must agree on the slider file, including legacy cards.
    if [ "$brick_full" -eq 1 ]; then
        nice -n "$nice_increment" cargo test $pflag --features brick --lib the_saved_slider_choice_reaches_the_card_watcher -q >>"$T" 2>&1 || trc=1
    fi
    # The ordinary integration tier excludes the Brick settings walk and the
    # Brick-only text regressions. Run those cases, keeping the public text matrix in its usual tier. The Smart Pro's
    # D-pad explorer (same card, 1280x720; its text fit is in brick_settings_walk) runs here too.
    for suite in brick_settings_walk text_size long_titles search_titles nav_explorer data_sources; do
        [ "$trc" -eq 0 ] || break
        [ -f "tests/$suite.rs" ] || continue
        [ "$brick_full" -eq 1 ] || case $suite in search_titles|nav_explorer|data_sources) continue ;; esac
        case $suite in
            nav_explorer)
                # libtest's filters are ORed: every present/future Brick case, plus the Smart Pro's walks.
                nice -n "$nice_increment" cargo test $pflag --features brick --test "$suite" -q -- brick on_the_smart_pro >>"$T" 2>&1 || trc=1
                continue ;;
            text_size) filter=textfit_brick_031_ ;;
            long_titles) filter=info_titles_031_ ;;
            *) filter= ;;
        esac
        nice -n "$nice_increment" cargo test $pflag --features brick --test "$suite" $filter -q >>"$T" 2>&1 || trc=1
    done
fi
brick_done=$(date +%s)
# No timing assertion runs while a lane or its compiler may still be active.
timing_started=$(date +%s)
# Timing cases must measure a real renderer, not a stored static layout probe.
unset POCKETUI_SNAPSHOT_CACHE_DIR
# The ignored timing tier is mandatory for both cargo and nextest. It gets one
# attempt, one test thread, and runs after compilation; failure stays red.
if [ -z "${GATE_FULL:-}" ] && { [ "$tier" = fast ] || [ "$merge_full" -eq 1 ]; }; then
    :   # the timing tier runs nightly and before a release (GATE_FULL=1)
elif [ "$trc" -eq 0 ] && [ "$shell_run_rc" -eq 0 ]; then
    if [ "$docs_only" -eq 1 ]; then timing_targets="--lib --bins"; else timing_targets=; fi
    nice -n "$nice_increment" sh scripts/test-timing.sh "$profile" $timing_targets >>"$T" 2>&1 || trc=1
elif [ "$shell_run_rc" -ne 0 ]; then
    echo "gate: timing not started: shell coordinator did not finish cleanly"
    trc=1
fi
timing_done=$(date +%s)
# Platform checks already passed in the compile-first stage.
prc_platforms=0
tdone=$(date +%s)
sdone=$tdone
if [ "$shell_tests" -eq 1 ] && [ "$gate_parallel" -eq 0 ]; then
    # Supervise a child even in the serial tier: an interrupted coordinator must
    # not turn missing exit-code files into success.
    shell_tests_run & shell_pid=$!
    wait "$shell_pid" || shell_run_rc=$?
    sdone=$(date +%s)
fi
shell_bounds=$(python3 - "$RCD" <<'BOUNDS'
from pathlib import Path
import sys
root=Path(sys.argv[1])
try:
    starts=[int((root/f"{lane}.start").read_text()) for lane in ("launcher","small")]
    ends=[int((root/f"{lane}.end").read_text()) for lane in ("launcher","small")]
    if any(end < start for start,end in zip(starts,ends)): raise ValueError("negative interval")
    print(f"shell_started={min(starts)} shell_finished={max(ends)} shell_bounds_ok=1")
except (OSError,ValueError):
    # Missing result files are reported below; no fabricated positive duration.
    print("shell_started=0 shell_finished=0 shell_bounds_ok=0")
BOUNDS
) || exit 1
eval "$shell_bounds"
if [ "$shell_bounds_ok" -ne 1 ] && [ "$shell_run_rc" -eq 0 ] && [ "$shell_tests" -eq 1 ]; then shell_run_rc=125; fi
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
    echo "gate: profile $profile; tests $((tdone - start)) s, shell tests (launcher etc.) $((shell_finished - shell_started)) s$([ -n "$why_shell" ] && echo "; run because of $why_shell"), total $((end - start)) s"
else
    echo "gate: profile $profile; tests $((tdone - start)) s, total $((end - start)) s"
fi
echo "gate: tier $tier; compile $((compiled - start)) s; compile-first $((compile_check_done - compile_check_started)) s"
echo "gate: stages runner+doc $((runner_done - start)) s, brick corpus+UI $((brick_done - brick_started)) s, serial timing $((timing_done - timing_started)) s, platforms $((tdone - timing_done)) s, shells $((shell_finished - shell_started)) s, join before Brick $((brick_started - runner_done)) s"
python3 - "$gate_parallel" "$gate_normal" "$gate_launcher" "$gate_small" "$start" "$runner_done" "$brick_started" "$brick_done" "$shell_started" "$shell_finished" "$timing_started" "$timing_done" "$tdone" <<'WINDOWS' || exit 1
import json, os, sys
parallel, normal, launcher, small = sys.argv[1:5]
start, runner, brick_start, brick, shell_start, shell_end, timing_start, timing_end, platforms = map(int, sys.argv[5:])
print("gate: schedule " + json.dumps({"mode": "parallel" if parallel == "1" else "serial", "host": os.environ.get("GATE_HOST", "host"), "cpus": {"rust": normal, "launcher": launcher, "small": small}, "windows": {"runner": [start, runner], "brick": [brick_start, brick], "shells": [shell_start, shell_end], "timing": [timing_start, timing_end], "platforms": [timing_end, platforms]}}))
WINDOWS
if [ "$docs_only" -eq 1 ]; then
    echo "gate: SKIPPED integration tests: only documents changed ($(echo "$changed" | wc -l) files, none read by the Rust build or the manual); unit tests with the doc checks ran"
fi
if [ "$shell_tests" -eq 0 ]; then
    echo "gate: SKIPPED launcher and other shell tests: nothing under deploy/ or scripts/ and no licence text changed (GATE_FULL=1 runs everything)"
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
