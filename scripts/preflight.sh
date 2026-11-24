#!/bin/sh
# (Lane check 13.11.: this line is a comment only.)
# One command before you queue (13.11.2026; the merge gate runs the same cheap list right after its compile stage).
#   sh scripts/preflight.sh [worktree]                all four steps below, stops at the first failure
#   sh scripts/preflight.sh --cheap-tests [worktree]  only step 4 (what scripts/gate.sh calls after compile-first)
#   sh scripts/preflight.sh --git-only|--build-only [worktree]  steps 1-2 / steps 3-4: the build steps can run on another machine
#                  from a copy of the tree without .git while the git steps run where the repository is
# Steps:
#   1 rebase       `git rebase main` (a clean tree and a branch are required; PREFLIGHT_NO_REBASE=1 skips this step).
#                  CHANGELOG.md and the translation tables conflict most; the queue unions them itself, so on a conflict there
#                  resolve by keeping both sides or run with PREFLIGHT_NO_REBASE=1 and let the queue rebase.
#   2 leaks        scripts/check-leaks.sh, the private-data scan (about 5 s).
#   3 compile      scripts/check-compile.sh: Linux --all-targets (examples too), Brick, and Android with android/player and
#                  android/handheld. Says "android SKIPPED" where the Rust target or an NDK is missing: then the gate is the first
#                  to compile it, so run this where they exist (the build machine or a Mac with the NDK) when you touched src/ or android/.
#   4 cheap tests  the translation audit (scripts/i18n-extract.py --check: every string painted from src/ has a German line;
#                  missing es.tsv/pt.tsv/fr.tsv/it.tsv lines are only a warning, filled before a release), then the unit tests doc_checks::,
#                  document_check::, manual::, i18n:: (links and anchors resolve, settings tables match, placeholders match,
#                  German complete; bookkeeping such as the docs index or page length is scripts/bookkeeping.sh, nightly).
#                  The tests run in seconds; the build of the unit tests is the cost (measured 13.11. on the shared builder, two jobs, sources touched:
#                  compile stage 2 min warm, unit-test build 8 min, 10 min in all; a cold target takes longer than 15 min).
# Run it as your last step: `~/bin/heavy timeout 1500 sh scripts/preflight.sh` on the shared builder (CARGO_BUILD_JOBS defaults to 2;
# 900 s is enough only with a warm target), or on a machine with more cores.
# A red gate costs the whole train; most red gates of 12.-13.11. failed in exactly these checks.
# Exit 0 ok, 1 a step failed (it says which and prints the first lines), 2 bad usage.
set -u
cheap_only=0 git_part=1 build_part=1
case ${1:-} in
    --cheap-tests) cheap_only=1; shift ;;
    --git-only) build_part=0; shift ;;     # steps 1-2 only (rebase, leak scan): needs .git, takes seconds
    --build-only) git_part=0; shift ;;     # steps 3-4 only (compile, cheap tests): a copy of the tree without .git is enough
esac
case ${1:-} in --*) echo "usage: scripts/preflight.sh [--cheap-tests | --git-only | --build-only] [worktree]" >&2; exit 2 ;; esac
dir=${1:-$(cd "$(dirname "$0")/.." && pwd)}
cd "$dir" || { echo "preflight: no such worktree: $dir" >&2; exit 2; }
# Its own TMPDIR (the cheap tests leave nothing in the shared /tmp): scripts/run-tmp.sh
[ ! -f scripts/run-tmp.sh ] || { . scripts/run-tmp.sh; run_tmp_begin pre; }
export PATH=$HOME/.cargo/bin:$HOME/opt/zig:$PATH
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
[ -n "${ANDROID_HOME:-}" ] || [ ! -d "$HOME/android/sdk" ] || export ANDROID_HOME=$HOME/android/sdk
profile=${GATE_PROFILE:-gate}
case $profile in release) pflag=--release ;; *) pflag="--profile $profile" ;; esac
began=$(date +%s)
step() { echo "preflight: $1"; stage_started=$(date +%s); }
took() { echo "preflight: $1 ok ($(( $(date +%s) - stage_started )) s)"; }
fail() { echo "PREFLIGHT: $1 FAILED ($(( $(date +%s) - began )) s)"; shift; for line in "$@"; do printf '%s\n' "$line"; done; exit 1; }

if [ "$cheap_only" -eq 0 ] && [ "$git_part" -eq 1 ]; then
    if [ -z "${PREFLIGHT_NO_REBASE:-}" ]; then
        step "1/4 rebase on main"
        branch=$(git symbolic-ref --short -q HEAD) || fail "rebase" "not on a branch (detached HEAD)"
        [ -z "$(git status --porcelain)" ] || fail "rebase" "the tree has uncommitted changes; commit them first:" "$(git status --short | head -10)"
        if ! out=$(git rebase main 2>&1); then
            files=$(git diff --name-only --diff-filter=U | head -10)
            git rebase --abort >/dev/null 2>&1
            fail "rebase" "$branch does not rebase onto main; conflicts in:" "$files" "resolve with: git rebase main (CHANGELOG.md / assets/i18n tables: keep both sides)"
        fi
        took "rebase ($branch on $(git rev-parse --short main))"
    else
        echo "preflight: 1/4 rebase skipped (PREFLIGHT_NO_REBASE)"
    fi
    if [ -f scripts/check-leaks.sh ]; then
        step "2/4 leak scan"
        out=$(sh scripts/check-leaks.sh "$(pwd)" 2>&1) || fail "leak scan (scripts/check-leaks.sh)" "$out"
        took "leak scan"
    fi
fi
if [ "$git_part" -eq 1 ] && [ "$build_part" -eq 0 ]; then
    echo "PREFLIGHT: git part ok ($(( $(date +%s) - began )) s); the build part is still to run (--build-only)"
    exit 0
fi
if [ "$cheap_only" -eq 0 ]; then
    step "3/4 compile: Linux, Brick, Android (android/, android/player, android/handheld)"
    log=$(mktemp "${TMPDIR:-/tmp}/preflight-compile.XXXXXX") || exit 1
    if ! sh scripts/check-compile.sh "$(pwd)" >"$log" 2>&1; then
        out=$(tail -40 "$log"); rm -f "$log"; fail "compile (scripts/check-compile.sh)" "$out"
    fi
    grep -E '^(compile-first|check-platforms):' "$log"
    if grep -q 'SKIPPED' "$log"; then
        echo "preflight: WARNING a platform was not compiled here (see SKIPPED above); the gate is the first to compile it"
    fi
    rm -f "$log"
    took "compile"
fi

step "4/4 cheap tests: translation audit, doc_checks::, document_check::, manual::, i18n::"
if [ -f scripts/i18n-extract.py ]; then
    out=$(python3 -I scripts/i18n-extract.py --check 2>&1) || fail "translation audit (scripts/i18n-extract.py --check)" "$out" \
        "new UI text needs a line in assets/i18n/de.tsv; es.tsv, pt.tsv, fr.tsv and it.tsv are filled before a release (docs/dev/translations.md)"
    printf '%s\n' "$out" | grep 'WARNING' || true
fi
# The reader inventory (19.11.2026: the most frequent red test of the day, tests/data_sources.rs runs this script); seconds.
if [ -f scripts/reader-inventory.py ]; then
    out=$(python3 -I scripts/reader-inventory.py 2>&1) || fail "reader inventory (scripts/reader-inventory.py)" "$out" \
        "a new or changed file reader: add its fault case and run scripts/reader-inventory.py --write (tests/data_sources.rs)"
fi
# The CHANGELOG guard (20.11.2026): published sections as their tag, no line twice, no vetoed line (the list beside the repo).
if [ -f scripts/check-changelog.py ]; then
    out=$(python3 -I scripts/check-changelog.py 2>&1) || fail "CHANGELOG guard (scripts/check-changelog.py)" "$out" \
        "new lines go under ## [Unreleased]; a published section stays as its tag (--restore X.Y.Z); no line twice"
fi
total=0
for filter in doc_checks:: document_check:: manual:: i18n::; do
    part=$(mktemp "${TMPDIR:-/tmp}/preflight-test.XXXXXX") || exit 1
    # An empty filter (a renamed module) must not pass silently: at least one test has to run.
    if cargo test $pflag --lib --bins "$filter" -q >"$part" 2>&1 &&
        awk '/^test result: ok\./ { passed += $4 } END { exit !(passed > 0) }' "$part"; then
        total=$(( total + $(awk '/^test result: ok\./ { passed += $4 } END { print passed + 0 }' "$part") ))
        rm -f "$part"
    else
        out=$(tail -30 "$part"); rm -f "$part"
        fail "cheap tests ($filter; an empty check also fails)" "$out"
    fi
done
echo "preflight: cheap tests ok ($(( $(date +%s) - stage_started )) s, $total passed)"
echo "PREFLIGHT: ok ($(( $(date +%s) - began )) s)"
