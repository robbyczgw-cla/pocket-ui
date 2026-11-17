#!/bin/sh
# Exercise the real gate's path selection in an isolated tree, with stand-in cargo and shell suites. No builds.
set -eu
# The fixtures decide every selection themselves: a caller's GATE_* settings (GATE_FULL=1, GATE_BASE, a runner or profile
# choice, a train's CPU plan) must not change what they prove.
for gate_variable in $(env | sed -n 's/^\(GATE_[A-Za-z0-9_]*\)=.*/\1/p'); do unset "$gate_variable"; done
export GATE_EXCLUSIVE=0
ROOT=$(cd "$(dirname "$0")/.." && pwd)
T=$(mktemp -d "${TMPDIR:-/tmp}/gate-paths.XXXXXX")
trap 'rm -rf "$T"' EXIT HUP INT TERM
mkdir -p "$T/tree/scripts/nested/helpers" "$T/bin" "$T/logs"
# Relocate logs and isolate executable lookup; the gate's selection and execution otherwise run unchanged.
sed -e "s|/tmp/gate-|$T/logs/gate-|g" \
    -e 's|^export PATH=.*|export PATH="$GATE_FIXTURE_BIN:$PATH"|' \
    "$ROOT/scripts/gate.sh" >"$T/tree/scripts/gate.sh"
cp "$ROOT/scripts/check-compile.sh" "$T/tree/scripts/check-compile.sh"
printf "[features]\n" >"$T/tree/Cargo.toml"
cp "$ROOT/scripts/test-timing.sh" "$T/tree/scripts/test-timing.sh"
cp "$ROOT/scripts/gate-select.py" "$T/tree/scripts/gate-select.py"
cp "$ROOT/scripts/gate-parts.py" "$T/tree/scripts/gate-parts.py"
cp "$ROOT/scripts/gate-cpus.py" "$T/tree/scripts/gate-cpus.py"
sed "s|/tmp/gate-|$T/logs/gate-|g" "$ROOT/scripts/gate-shell.sh" >"$T/tree/scripts/gate-shell.sh"
cat >"$T/bin/cargo" <<'CARGO'
#!/bin/sh
printf '%s\n' "$*" >>"$GATE_FIXTURE_CARGO"
[ -z "${POCKETUI_TEST_QUICK:-}" ] || printf '%s\n' "$*" >>"$GATE_FIXTURE_CARGO.quick"
case " $* " in *"check "*) [ "${GATE_FIXTURE_COMPILE_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" doc_checks:: "*) [ "${GATE_FIXTURE_DOC_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" i18n:: "*) [ "${GATE_FIXTURE_I18N_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" timing_ "*) [ "${GATE_FIXTURE_TIMING_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" doc_checks:: "*) if [ "${GATE_FIXTURE_DOC_EMPTY:-0}" = 1 ]; then echo 'test result: ok. 0 passed'; exit 0; fi ;; esac
case " $* " in *" i18n:: "*) if [ "${GATE_FIXTURE_I18N_EMPTY:-0}" = 1 ]; then echo 'test result: ok. 0 passed'; exit 0; fi ;; esac
case " $* " in *" --features brick --test brick_settings_walk "*) [ "${GATE_FIXTURE_BRICK_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" --test text_size textfit_brick_031_ "*) [ "${GATE_FIXTURE_BRICK_TEXT_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" --test nav_explorer -q -- brick "*) [ "${GATE_FIXTURE_BRICK_NAV_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" --test long_titles info_titles_031_ "*) [ "${GATE_FIXTURE_INFO_TITLE_FAIL:-0}" = 0 ] || exit 1 ;; esac
if [ "${GATE_FIXTURE_TIMING_EMPTY:-0}" = 1 ]; then
    case " $* " in *" timing_ "*) echo 'test result: ok. 0 passed'; exit 0 ;; esac
fi
echo 'test result: ok. 1 passed'
CARGO
chmod +x "$T/bin/cargo"
# Prevent core count or caller settings from changing the scheduling of this selection test.
printf '#!/bin/sh\necho 1\n' >"$T/bin/nproc"
chmod +x "$T/bin/nproc"
for suite in deploy/test-run-pocket-ui.sh scripts/test-verify-device.sh scripts/test-check-core-licences.sh \
    deploy/package/test-package.sh deploy/test-grub-quiet.sh scripts/image/test-grub-checks.sh scripts/image/test-rocknix-source.sh scripts/image/test-h700-pins.sh scripts/image/test-storage-budget.sh deploy/fat/test-hook.sh \
    deploy/image/test-mount-storage.sh; do
    mkdir -p "$T/tree/$(dirname "$suite")"
    cat >"$T/tree/$suite" <<'SUITE'
#!/bin/sh
printf '%s\n' "$0" >>"$GATE_FIXTURE_CALLS"
case $0 in */test-package.sh) [ "${GATE_FIXTURE_FAIL:-0}" = 0 ] || exit 1 ;; esac
SUITE
done
for suite in scripts/image/test-receipts.py scripts/test-device-portability.py scripts/test-device-transfer.py scripts/android/test-release-check.py; do
    mkdir -p "$T/tree/$(dirname "$suite")"
    cat >"$T/tree/$suite" <<'PYTEST'
import os, sys
with open(os.environ['GATE_FIXTURE_CALLS'], 'a') as calls:
    calls.write(sys.argv[0] + '\n')
if os.environ.get('GATE_FIXTURE_DEVICE_FAIL') == os.path.basename(sys.argv[0]):
    sys.exit(1)
PYTEST
done
# A transitive shell helper must keep the Rust source it names in the shell test set.
printf '# helper reads src/hostsys.rs\n' >"$T/tree/scripts/nested/helpers/check.sh"
printf '# helper reads tests/shell_input.rs\n' >>"$T/tree/scripts/nested/helpers/check.sh"
export GATE_FIXTURE_BIN="$T/bin" TMPDIR="$T"
export GATE_FIXTURE_CARGO="$T/cargo" GATE_FIXTURE_CALLS="$T/calls"
pass=0
# check LABEL SHELL_CALLS TIER PATHS [GATE_FULL] [BAD_PACKAGE]; TIER: docs (short path), fast (merge gate) or full.
check() {
    label=$1 want_shell=$2 want_tier=$3 paths=$4 full=${5:-} bad_package=${6:-0}
    : >"$T/cargo"; : >"$T/calls"
    printf '%s\n' "$paths" >"$T/tree/.gate-changed"
    rc=0
    GATE_FULL="$full" GATE_PARALLEL=0 GATE_RUNNER=cargo GATE_PROFILE=gate GATE_FIXTURE_FAIL="$bad_package" \
        sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
    actual_shell=$(wc -l <"$T/calls" | tr -d ' ')
    if grep -q -- 'doc_checks::' "$T/cargo"; then actual_tier=docs
    elif grep -q -- '--no-run' "$T/cargo"; then actual_tier=fast
    else actual_tier=full; fi
    expected_rc=0
    [ "$want_shell" = 0 ] || [ "$bad_package" = 0 ] || expected_rc=1
    if [ "$actual_shell" != "$want_shell" ] || [ "$actual_tier" != "$want_tier" ] || [ "$rc" != "$expected_rc" ]; then
        echo "FAIL $label: shell=$actual_shell tier=$actual_tier exit=$rc (wanted $want_shell $want_tier $expected_rc)"
        cat "$T/out"
        exit 1
    fi
    case $want_tier in
    docs)
        grep -q 'doc_checks::' "$T/cargo" && grep -q 'i18n::' "$T/cargo" || { echo 'FAIL short path lost doc/i18n checks'; exit 1; }
        if grep -q timing_ "$T/cargo"; then echo 'FAIL short path ran player timing tests'; exit 1; fi
        calls=2 ;;
    fast)
        # Compile everything, then unit + selected integration tests and doctests; the timing tier is nightly.
        if grep -q timing_ "$T/cargo"; then echo "FAIL $label: fast tier ran the timing tier"; exit 1; fi
        grep -q -- '--lib --bins' "$T/cargo" && grep -q -- '--doc' "$T/cargo" || { echo "FAIL $label: fast tier lost unit tests or doctests"; exit 1; }
        calls=4 ;;
    full)
        grep -q 'timing_ -- --ignored --test-threads=1' "$T/cargo" || { echo 'FAIL timing tier missing'; exit 1; }
        calls=3 ;;
    esac
    [ "$(wc -l <"$T/cargo")" -eq "$calls" ] || { echo "FAIL $label: timing tier retried or regular tests skipped"; cat "$T/cargo"; exit 1; }
    pass=$((pass + 1))
    echo "ok $label"
}
# A broken example must stop before codegen, tests or shell workers.
: >"$T/cargo"; : >"$T/calls"
printf '%s\n' src/main.rs >"$T/tree/.gate-changed"
GATE_FIXTURE_COMPILE_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 && { echo 'FAIL broken compilation passed'; exit 1; }
grep -q 'GATE: compile-first FAILED' "$T/out" && [ "$(wc -l <"$T/cargo")" -eq 1 ] && [ ! -s "$T/calls" ] || { cat "$T/out"; exit 1; }
pass=$((pass + 1))
echo 'ok compile error stops before tests or shell lanes'
# The cheap checks of scripts/preflight.sh run right after the compile stage and before the long test build; a red one stops there.
sed 's|^export PATH=.*|export PATH="$GATE_FIXTURE_BIN:$PATH"|' "$ROOT/scripts/preflight.sh" >"$T/tree/scripts/preflight.sh"
printf 'import sys\nsys.exit(0)\n' >"$T/tree/scripts/i18n-extract.py"
: >"$T/cargo"; : >"$T/calls"
printf '%s\n' src/main.rs >"$T/tree/.gate-changed"
GATE_PARALLEL=0 GATE_RUNNER=cargo GATE_PROFILE=gate sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo 'FAIL gate with the cheap checks'; cat "$T/out"; exit 1; }
cheap_at=$(grep -n -- 'doc_checks::' "$T/cargo" | head -1 | cut -d: -f1)
build_at=$(grep -n -- '--no-run' "$T/cargo" | head -1 | cut -d: -f1)
[ -n "$cheap_at" ] && [ -n "$build_at" ] && [ "$cheap_at" -lt "$build_at" ] || { echo 'FAIL cheap checks do not run before the test build'; cat "$T/cargo"; exit 1; }
grep -q 'gate: cheap checks ok' "$T/out" || { echo 'FAIL cheap checks not reported'; cat "$T/out"; exit 1; }
for filter in doc_checks:: document_check:: manual:: i18n::; do grep -q -- " $filter " "$T/cargo" || { echo "FAIL the gate lost the cheap check $filter"; exit 1; }; done
pass=$((pass + 1))
echo 'ok cheap checks run right after the compile stage'
: >"$T/cargo"; : >"$T/calls"
GATE_FIXTURE_DOC_FAIL=1 GATE_PARALLEL=0 GATE_RUNNER=cargo GATE_PROFILE=gate sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 && { echo 'FAIL a red cheap check passed the gate'; exit 1; }
grep -q 'GATE: cheap checks FAILED' "$T/out" && ! grep -q -- '--no-run' "$T/cargo" && [ ! -s "$T/calls" ] || { echo 'FAIL a red cheap check must stop before the test build and the shell lanes'; cat "$T/out"; cat "$T/cargo"; exit 1; }
pass=$((pass + 1))
echo 'ok a red cheap check stops the gate before the long build'
# The comment-only lane: a remote gate has no .git, so .gate-lane (written by tools/remote-gate.sh from scripts/gate-comments.py)
# carries the proof. With "comments" the gate is compile stage + cheap checks + self-tests; anything else keeps the normal tiers.
cp "$ROOT/scripts/gate-comments.py" "$T/tree/scripts/gate-comments.py"
lane_gate() {  # LANE [GATE_FULL]  -> rc in $rc, output in $T/out, cargo calls in $T/cargo
    : >"$T/cargo"; : >"$T/calls"
    printf '%s\n' scripts/some-script.sh >"$T/tree/.gate-changed"
    if [ -n "$1" ]; then printf '%s\n' "$1" >"$T/tree/.gate-lane"; else rm -f "$T/tree/.gate-lane"; fi
    rc=0
    GATE_FULL="${2:-}" GATE_PARALLEL=0 GATE_RUNNER=cargo GATE_PROFILE=gate sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
}
lane_gate comments
[ "$rc" = 0 ] && grep -q 'gate: comment-only lane' "$T/out" && grep -q '^GATE: ok (4 passed)' "$T/out" || { echo 'FAIL comment-only lane did not finish the gate'; cat "$T/out"; exit 1; }
! grep -q -- '--no-run' "$T/cargo" && [ ! -s "$T/calls" ] || { echo 'FAIL comment-only lane built the tests or ran shell suites'; cat "$T/cargo"; exit 1; }
grep -q 'check ' "$T/cargo" && for filter in doc_checks:: document_check:: manual:: i18n::; do grep -q -- " $filter " "$T/cargo" || { echo "FAIL the lane lost $filter"; exit 1; }; done
pass=$((pass + 1))
echo 'ok a comment-only change runs the compile stage and the cheap checks only'
for variant in "no" ""; do
    lane_gate "$variant"
    [ "$rc" = 0 ] && grep -q -- '--no-run' "$T/cargo" && ! grep -q 'comment-only lane' "$T/out" || { echo "FAIL lane '$variant' must keep the normal tiers"; cat "$T/out"; exit 1; }
done
lane_gate comments 1
[ "$rc" = 0 ] && ! grep -q 'comment-only lane' "$T/out" || { echo 'FAIL GATE_FULL=1 must ignore the lane'; cat "$T/out"; exit 1; }
pass=$((pass + 1))
echo 'ok without the proof, or with GATE_FULL=1, the normal tiers run'
: >"$T/cargo"; printf 'comments\n' >"$T/tree/.gate-lane"
GATE_FIXTURE_I18N_FAIL=1 GATE_PARALLEL=0 GATE_RUNNER=cargo GATE_PROFILE=gate sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 && { echo 'FAIL the lane passed a red cheap check'; exit 1; }
grep -q 'GATE: cheap checks FAILED' "$T/out" || { echo 'FAIL the lane lost its cheap checks'; cat "$T/out"; exit 1; }
pass=$((pass + 1))
echo 'ok a red cheap check fails the comment-only lane'
# A local gate (git available, no .gate-changed) proves the lane itself, against GATE_LANE_BASE when the train has one: the commit
# before holds a code change of an earlier train, only the last commit is a comment.
rm -f "$T/tree/.gate-lane" "$T/tree/.gate-changed"
(
    cd "$T/tree" && export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid
    git init -q -b main && printf '#!/bin/sh\necho a\n' >scripts/some.sh && printf '#!/bin/sh\necho b\n' >scripts/other.sh
    printf '.gate-*\n' >.git/info/exclude
    git add -A && git commit -qm base && git tag lane-main
    printf '#!/bin/sh\necho b changed\n' >scripts/other.sh && git commit -qam "earlier train: code" && git tag lane-earlier
    printf '#!/bin/sh\n# only a comment\necho a\n' >scripts/some.sh && git commit -qam "this train: comment"
)
local_gate() {  # extra env as arguments
    : >"$T/cargo"; : >"$T/calls"; rc=0
    env "$@" GATE_BASE=lane-main GATE_PARALLEL=0 GATE_RUNNER=cargo GATE_PROFILE=gate sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
}
local_gate GATE_LANE_BASE=lane-earlier
[ "$rc" = 0 ] && grep -q 'gate: comment-only lane' "$T/out" && ! grep -q -- '--no-run' "$T/cargo" || { echo 'FAIL local comment-only lane against the train base'; cat "$T/out"; exit 1; }
local_gate GATE_UNUSED=1
[ "$rc" = 0 ] && ! grep -q 'comment-only lane' "$T/out" && grep -q -- '--no-run' "$T/cargo" || { echo 'FAIL the earlier code change must keep the normal tiers without GATE_LANE_BASE'; cat "$T/out"; exit 1; }
pass=$((pass + 1))
echo 'ok a local gate proves the lane against the train base'
rm -rf "$T/tree/.git" "$T/tree/scripts/gate-comments.py" "$T/tree/scripts/some.sh" "$T/tree/scripts/other.sh"
rm -f "$T/tree/scripts/preflight.sh" "$T/tree/scripts/i18n-extract.py"
: >"$T/cargo"
check 'LICENSE still runs package tests' 15 fast LICENSE
check 'NOTICE still runs package tests' 15 fast NOTICE.md
check 'font licence runs package tests' 15 fast assets/fonts/OFL.txt
check 'deleted or empty licence is rejected by required package suite' 15 fast LICENSE '' 1
check 'ordinary docs use only doc and i18n checks' 0 docs docs/DEVELOPING.md
check 'root Markdown skips shell and integration tests' 0 docs README.md
check 'manual changes leave the short path' 0 fast docs/user/settings.md
check 'Markdown outside docs runs the merge gate without shell tests' 0 fast assets/new-input.md
check 'unknown doc configuration runs the merge gate without shell tests' 0 fast docs/new-input.conf
check 'one Rust source runs the merge gate without shell tests' 0 fast src/unreferenced_feature.rs
check 'a Rust source a script names runs no shell tests (they use stand-ins)' 0 fast src/hostsys.rs
check 'one Rust test runs the merge gate' 0 fast tests/layout_contract.rs
check 'Brick text regression changes run the merge gate' 0 fast tests/text_size.rs
check 'a Rust test a script names runs no shell tests' 0 fast tests/shell_input.rs
check 'unknown test data runs the merge gate without shell tests' 0 fast tests/new-input.txt
check 'Rust test path with spaces runs the merge gate without shell tests' 0 fast 'tests/new input.rs'
check 'Rust test changes cannot cancel deploy tests' 15 fast 'tests/layout_contract.rs
deploy/run-pocket-ui.sh'
check 'full override retains shell tests after Rust test changes' 15 full tests/layout_contract.rs 1
check 'non-Rust source input runs the merge gate without shell tests' 0 fast src/new-input.c
check 'assets run the merge gate without shell tests' 0 fast assets/new-input.txt
check 'unknown root files run the merge gate without shell tests' 0 fast new-input
check 'deploy changes run shell tests' 15 fast deploy/run-pocket-ui.sh
check 'gate script changes run every suite but the time budgets, with the shell tests' 15 fast scripts/test-gate-paths.sh
check 'other script changes run shell tests' 15 fast scripts/device-check.sh
check 'Cargo metadata changes run every suite but the time budgets' 0 fast Cargo.toml
check 'test harness changes run every suite but the time budgets' 0 fast tests/common/mod.rs
check 'docs cannot cancel licence tests' 15 fast 'LICENSE
README.md'
check 'docs and an asset run the merge gate without shell tests' 0 fast 'assets/new-input
README.md'
check 'path with spaces remains one path' 0 fast 'assets/new input.md'
check 'Markdown path with spaces remains one document' 0 docs 'docs/new document.md'
check 'unembedded translation audit notes use short path' 0 docs assets/i18n/skip.txt
check 'runtime translation tables leave the short path' 0 fast assets/i18n/de.tsv
check 'mixed docs and Rust leave the short path' 0 fast 'docs/DEVELOPING.md
src/main.rs'
check 'no change list runs everything' 15 full ''
check 'full override runs everything' 15 full docs/DEVELOPING.md 1
# Firmware and transfer failures must fail the real shell tier.
printf '%s\n' scripts/device-check.sh >"$T/tree/.gate-changed"
for suite in test-device-portability.py test-device-transfer.py; do
    rc=0
    GATE_FIXTURE_DEVICE_FAIL="$suite" GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
    [ "$rc" = 1 ] && grep -q 'verify-device tests FAILED' "$T/out" || { echo "FAIL $suite failure hidden"; exit 1; }
    pass=$((pass + 1))
    echo "ok $suite failure stays red"
done
# Short-path failures must stay red, including a link/index or translation failure.
printf '%s\n' docs/DEVELOPING.md >"$T/tree/.gate-changed"
for which in DOC_FAIL I18N_FAIL DOC_EMPTY I18N_EMPTY; do
    rc=0
    env "GATE_FIXTURE_${which}=1" GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
    [ "$rc" = 1 ] && grep -q 'documentation/i18n checks FAILED' "$T/out" || { echo "FAIL short-path $which failure hidden"; exit 1; }
    pass=$((pass + 1))
    echo "ok short-path $which failure stays red"
done
mkdir -p "$T/tree/src"
printf 'std::fs::read_to_string("docs/player-contract.md");\n' >"$T/tree/src/runtime.rs"
check 'on-disk player contract outside doc tier leaves the short path' 0 fast docs/player-contract.md
rm "$T/tree/src/runtime.rs"
printf 'include_str!("../docs/embedded.md");\n' >"$T/tree/src/embedded.rs"
check 'compiled document leaves the short path' 0 fast docs/embedded.md
printf 'include_str! { "../docs/embedded.md" };\n' >"$T/tree/src/embedded.rs"
check 'alternate embed delimiter leaves the short path' 0 fast docs/embedded.md
printf 'include_str!(concat!("../docs/", env!("NAME")));\n' >"$T/tree/src/embedded.rs"
check 'unknown dynamic embed leaves the short path' 0 fast docs/DEVELOPING.md
printf 'include_str!("docs/embedded.md");\n' >"$T/tree/build.rs"
rm "$T/tree/src/embedded.rs"
check 'build script embedded document leaves the short path' 0 fast docs/embedded.md
rm "$T/tree/build.rs"
mkdir -p "$T/tree/tests"
printf 'include_str!("../docs/embedded.md");\n' >"$T/tree/tests/embed.rs"
check 'integration fixture embedded document leaves the short path' 0 fast docs/embedded.md
rm "$T/tree/tests/embed.rs"
mkdir -p "$T/tree/docs"
ln -s ../README.md "$T/tree/docs/link.md"
check 'document symlink leaves the short path' 0 fast docs/link.md
rm "$T/tree/docs/link.md"
printf '%s\n' src/main.rs >"$T/tree/.gate-changed"
# The German/Spanish snapshot matrix runs when the manual or the translations change, and in every full gate
# (nightly, release); not for other merge-gate changes and not in a docs-only one.
mkdir -p "$T/tree/tests"; : >"$T/tree/tests/i18n_snapshots.rs"
for changed_path in docs/user/options.md assets/i18n/keep.txt src/main.rs docs/DEVELOPING.md full:src/main.rs; do
    full=
    case $changed_path in full:*) full=1; changed_path=${changed_path#full:} ;; esac
    printf '%s\n' "$changed_path" >"$T/tree/.gate-changed"
    : >"$T/cargo"
    GATE_FULL="$full" GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo "FAIL gate with $changed_path"; cat "$T/out"; exit 1; }
    if grep -q -- '--test i18n_snapshots -q -- --include-ignored' "$T/cargo"; then ran=1; else ran=0; fi
    case $full:$changed_path in :docs/DEVELOPING.md|:src/main.rs) want=0 ;; *) want=1 ;; esac
    [ "$ran" = "$want" ] || { echo "FAIL German/Spanish snapshot matrix ran=$ran for $changed_path"; exit 1; }
    pass=$((pass + 1))
    echo "ok German/Spanish snapshot matrix ran=$ran for $changed_path"
done
printf '%s\n' src/main.rs >"$T/tree/.gate-changed"
# A red timing result cannot be retried into green.
rc=0
GATE_FULL=1 GATE_FIXTURE_TIMING_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
[ "$rc" = 1 ] && grep -q 'GATE: tests FAILED' "$T/out" || { echo 'FAIL red timing tier'; exit 1; }
pass=$((pass + 1))
echo 'ok red timing tier fails the gate'
rc=0
GATE_FULL=1 GATE_FIXTURE_TIMING_EMPTY=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
[ "$rc" = 1 ] && grep -q 'GATE: tests FAILED' "$T/out" || { echo 'FAIL empty timing tier'; exit 1; }
pass=$((pass + 1))
echo 'ok empty timing tier fails the gate'
# The Brick's UI walk has cfg(feature = "brick"): the ordinary run sees zero
# tests. A broken real walk must fail the private gate, not silently disappear.
mkdir -p "$T/tree/src" "$T/tree/tests"
printf '[features]\nbrick = []\n' >"$T/tree/Cargo.toml"
: >"$T/tree/src/input_corpus_tests.rs"
for suite in brick_settings_walk text_size long_titles search_titles nav_explorer; do : >"$T/tree/tests/$suite.rs"; done
printf '%s\n' src/sleep.rs >"$T/tree/.gate-changed"
for broken in 1 0; do
    : >"$T/cargo"
    rc=0
    GATE_FULL=1 GATE_FIXTURE_BRICK_FAIL=$broken GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
    [ "$rc" = "$broken" ] || { echo 'FAIL red Brick UI walk did not fail the gate'; exit 1; }
    grep -q -- '--features brick --lib input_corpus_tests' "$T/cargo" &&
        grep -q -- '--features brick --test brick_settings_walk' "$T/cargo" || {
        echo 'FAIL private gate omitted the Brick controller or UI walk'; exit 1;
    }
    pass=$((pass + 1))
    echo "ok private Brick UI walk verdict $broken"
done
# Every Brick-named navigation case needs a feature-enabled stage, including future names.
# Check actual gate invocations against the real suite, rather than trusting a comment or a fixed list of today's cases.
python3 -I - "$ROOT/tests/nav_explorer.rs" "$T/cargo" <<'NAV_COVERAGE'
from pathlib import Path
import re, shlex, sys
names = re.findall(r"fn\s+(\w*brick\w*)\s*\(", Path(sys.argv[1]).read_text())
names += ["future_brick_navigation_regression", "brick_future_shortcut"]
filters = []
for line in Path(sys.argv[2]).read_text().splitlines():
    args = shlex.split(line)
    if "--features" not in args or "brick" not in args[args.index("--features")+1].split(","):
        continue
    if "--test" not in args or args[args.index("--test")+1] != "nav_explorer":
        continue
    tail = args[args.index("--test")+2:]
    filters.extend(a for a in tail if not a.startswith("-") and a != "--")
missing = [name for name in names if not any(f in name for f in filters)]
assert not missing, "Brick navigation omitted from every feature-enabled stage: " + ", ".join(missing)
print(f"ok {len(names)} current/future Brick navigation names have a feature-enabled stage")
NAV_COVERAGE
pass=$((pass + 1))
: >"$T/cargo"
rc=0
GATE_FULL=1 GATE_FIXTURE_BRICK_NAV_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
[ "$rc" = 1 ] || { echo 'FAIL red Brick navigation did not fail the gate'; exit 1; }
pass=$((pass + 1))
echo 'ok red Brick navigation fails the gate'
: >"$T/cargo"
rc=0
GATE_FULL=1 GATE_FIXTURE_BRICK_TEXT_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
[ "$rc" = 1 ] && grep -q -- '--features brick --test text_size textfit_brick_031_' "$T/cargo" || {
    echo 'FAIL red Brick text regressions did not fail the gate'; exit 1;
}
pass=$((pass + 1))
echo 'ok red Brick text regressions fail the gate'
: >"$T/cargo"
rc=0
GATE_FULL=1 GATE_FIXTURE_INFO_TITLE_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
[ "$rc" = 1 ] && grep -q -- '--features brick --test long_titles info_titles_031_' "$T/cargo" || {
    echo 'FAIL red Brick info title regression did not fail the gate'; exit 1;
}
pass=$((pass + 1))
echo 'ok red Brick info title regression fails the gate'

# Merge gate: an ordinary source change only compiles the Brick build; a Brick path runs its corpus and UI walk.
printf '%s\n' src/sleep.rs >"$T/tree/.gate-changed"
: >"$T/cargo"
GATE_FIXTURE_BRICK_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || {
    echo 'FAIL merge gate ran the Brick UI walk for an ordinary source change'; cat "$T/out"; exit 1;
}
grep -q -- '^check --profile gate --locked --all-targets --features brick' "$T/cargo" || { echo 'FAIL merge gate lost the Brick compile check'; exit 1; }
if grep -q -- '--features brick --test' "$T/cargo"; then echo 'FAIL merge gate ran Brick suites'; exit 1; fi
pass=$((pass + 1))
echo 'ok merge gate compiles the Brick build for ordinary changes'
printf '%s\n' src/brick_hw.rs >"$T/tree/.gate-changed"
: >"$T/cargo"
rc=0
GATE_FIXTURE_BRICK_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
[ "$rc" = 1 ] && grep -q -- '--features brick --test brick_settings_walk' "$T/cargo" || {
    echo 'FAIL a Brick change in the merge gate did not run the red Brick UI walk'; exit 1;
}
pass=$((pass + 1))
echo 'ok a Brick change runs the Brick UI walk in the merge gate'
# Merge gate: slow layout suites wait for the nightly run unless the change touches them; all programs still compile.
: >"$T/tree/tests/user_journeys.rs"
for changed_path in src/sleep.rs tests/text_size.rs assets/fonts/new.ttf; do
    printf '%s\n' "$changed_path" >"$T/tree/.gate-changed"
    : >"$T/cargo"
    GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo "FAIL gate with $changed_path"; cat "$T/out"; exit 1; }
    # Compile-first (cargo check --all-targets) covers every program; the build before the tests makes only the programs it runs.
    grep -q -- '^check --profile gate --locked --all-targets' "$T/cargo" || { echo "FAIL $changed_path: compile-first lost"; exit 1; }
    grep -q -- '^test --profile gate --no-run.* --test user_journeys' "$T/cargo" || { echo "FAIL $changed_path: journeys not built"; exit 1; }
    grep -q -- '--test user_journeys' "$T/cargo" || { echo "FAIL $changed_path: journeys skipped"; exit 1; }
    case $changed_path in
    src/sleep.rs) if grep -q -- '^test --profile gate --no-run.* --test text_size' "$T/cargo"; then echo "FAIL $changed_path: the fast tier built a suite it leaves to the nightly run"; exit 1; fi ;;
    *) grep -q -- '^test --profile gate --no-run.* --test text_size' "$T/cargo" || { echo "FAIL $changed_path: text_size not built"; exit 1; } ;;
    esac
    if grep -q -- '--lib --bins.* --test text_size' "$T/cargo"; then ran=1; else ran=0; fi
    case $changed_path in src/sleep.rs) want=0 ;; *) want=1 ;; esac
    [ "$ran" = "$want" ] || { echo "FAIL slow suite text_size ran=$ran for $changed_path"; cat "$T/cargo"; exit 1; }
    pass=$((pass + 1))
    echo "ok slow suite text_size ran=$ran for $changed_path"
done
rm "$T/tree/tests/user_journeys.rs"
# The Brick stage follows Brick paths only (14.11., test audit): a font, a layout or text source, a translation line, the manual or a
# text suite leave every Brick test to the nightly run and the release check. The compile check of the Brick build stays.
for changed_path in assets/fonts/new.ttf src/font_style.rs src/kit.rs src/layouts.rs src/text_size.rs src/i18n.rs docs/user/settings.md \
        tests/text_size.rs tests/search_titles.rs src/manual.rs assets/i18n/de.tsv assets/i18n/es.tsv assets/i18n/pt.tsv assets/i18n/fr.tsv assets/i18n/it.tsv; do
    printf '%s\n' "$changed_path" >"$T/tree/.gate-changed"
    : >"$T/cargo"
    GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo "FAIL $changed_path gate"; cat "$T/out"; exit 1; }
    grep -q -- '^check --profile gate --locked --all-targets --features brick' "$T/cargo" || { echo "FAIL $changed_path: the Brick compile check is gone"; exit 1; }
    if grep -q -- '--features brick --\(lib\|test\)' "$T/cargo"; then echo "FAIL $changed_path: ran a Brick test without a Brick path"; cat "$T/cargo"; exit 1; fi
    grep -q 'Brick check' "$T/out" || { echo "FAIL $changed_path: the gate does not say the Brick tests wait"; cat "$T/out"; exit 1; }
    pass=$((pass + 1))
    echo "ok $changed_path leaves the Brick tests to the nightly run"
done
# A Brick path runs the whole Brick stage: corpus, slider, UI walk, text-fit cases, navigation, title search.
for changed_path in src/brick_hw.rs deploy/brick/start.sh scripts/brick-card.sh src/input_corpus.rs src/fn_keys.rs \
        'assets/i18n/de.tsv
src/brick_settings.rs'; do
    printf '%s\n' "$changed_path" >"$T/tree/.gate-changed"
    : >"$T/cargo"
    GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo "FAIL $changed_path gate"; cat "$T/out"; exit 1; }
    for wanted in '--features brick --lib input_corpus_tests' '--features brick --test brick_settings_walk' \
                  '--features brick --test text_size textfit_brick_031_' '--features brick --test nav_explorer' '--features brick --test search_titles'; do
        grep -q -- "$wanted" "$T/cargo" || { echo "FAIL $changed_path: the Brick stage lost '$wanted'"; cat "$T/cargo"; exit 1; }
    done
    pass=$((pass + 1))
    echo "ok $(echo "$changed_path" | tail -1) runs the whole Brick stage"
done
# Time budgets never run in the merge gate, not even when their own file changes; the layout matrices run as their sample there
# (POCKETUI_TEST_QUICK), whole under GATE_FULL=1. The single nightly-only tests are skipped in the merge gate only.
: >"$T/tree/tests/shelf_moves.rs"; : >"$T/tree/tests/launch_frames.rs"; : >"$T/tree/tests/user_journeys.rs"
: >"$T/tree/tests/settings_open.rs"
for changed_path in tests/shelf_moves.rs tests/launch_frames.rs src/sleep.rs scripts/gate.sh Cargo.toml; do
    printf '%s\n' "$changed_path" >"$T/tree/.gate-changed"
    : >"$T/cargo"; : >"$T/cargo.quick"
    GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo "FAIL $changed_path gate"; cat "$T/out"; exit 1; }
    for budget in shelf_moves launch_frames; do
        if grep -q -- "--test $budget\b" "$T/cargo"; then echo "FAIL $changed_path: the merge gate ran the time budget $budget"; cat "$T/cargo"; exit 1; fi
    done
    grep -q -- '--test user_journeys' "$T/cargo" || { echo "FAIL $changed_path: journeys skipped"; cat "$T/cargo"; exit 1; }
    grep -q -- '--test settings_open' "$T/cargo" || { echo "FAIL $changed_path: deterministic Settings check skipped"; cat "$T/cargo"; exit 1; }
    grep -q -- '--skip settings_first_frame_is_not_much_slower_than_the_next' "$T/cargo" || { echo "FAIL $changed_path: Settings time budget ran in merge gate"; cat "$T/cargo"; exit 1; }
    if grep -q timing_ "$T/cargo"; then echo "FAIL $changed_path: the merge gate ran the timing tier"; exit 1; fi
    grep -q -- '--lib --bins' "$T/cargo.quick" || { echo "FAIL $changed_path: the merge gate ran the matrices whole (no POCKETUI_TEST_QUICK)"; exit 1; }
    grep -q -- '--skip android_eject_tests::the_way_back_hands_over_to_the_still_shelf_without_a_jump' "$T/cargo" || { echo "FAIL $changed_path: a nightly-only test ran"; cat "$T/cargo"; exit 1; }
    pass=$((pass + 1))
    echo "ok $changed_path: no time budget, no timing tier, matrices as their sample"
done
printf '%s\n' tests/shelf_moves.rs >"$T/tree/.gate-changed"; : >"$T/cargo"; : >"$T/cargo.quick"
GATE_FULL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo "FAIL GATE_FULL gate"; cat "$T/out"; exit 1; }
[ ! -s "$T/cargo.quick" ] || { echo 'FAIL GATE_FULL=1 ran a matrix as its sample'; cat "$T/cargo.quick"; exit 1; }
grep -q 'timing_ -- --ignored --test-threads=1' "$T/cargo" || { echo 'FAIL GATE_FULL=1 lost the timing tier'; exit 1; }
if grep -q -- '--skip android_eject_tests::the_way_back' "$T/cargo"; then echo 'FAIL GATE_FULL=1 skipped a nightly-only test'; exit 1; fi
if grep -q -- '--skip settings_first_frame_is_not_much_slower_than_the_next' "$T/cargo"; then echo 'FAIL GATE_FULL=1 skipped Settings time budget'; exit 1; fi
grep -q -- '^test --profile gate -q$' "$T/cargo" || { echo 'FAIL GATE_FULL=1 picked suites instead of running all'; cat "$T/cargo"; exit 1; }
rm "$T/tree/tests/shelf_moves.rs" "$T/tree/tests/launch_frames.rs" "$T/tree/tests/user_journeys.rs" "$T/tree/tests/settings_open.rs"
pass=$((pass + 1))
echo 'ok GATE_FULL=1 runs the budgets, the timing tier, the whole matrices and the nightly-only tests'
# The Android page tests (android_02_tests, android_eject_tests) run when the change can reach them: Android sources, text,
# translations, fonts, assets, or a library module they use. Otherwise they wait for the nightly run; the filter names them.
mkdir -p "$T/tree/android/src"
printf 'use pocket_ui::eject::Case;\n' >"$T/tree/src/android_02_tests.rs"
: >"$T/tree/src/eject.rs"; : >"$T/tree/src/sleep.rs"; : >"$T/tree/src/hostsys.rs"; : >"$T/tree/android/src/ui.rs"
for changed_path in src/sleep.rs src/eject.rs src/hostsys.rs android/src/ui.rs src/android_core_run.rs src/text.rs assets/i18n/de.tsv assets/consoles/new.png docs/DEVELOPING.md+src/sleep.rs; do
    case $changed_path in docs/DEVELOPING.md+src/sleep.rs) printf 'docs/DEVELOPING.md\nsrc/sleep.rs\n' >"$T/tree/.gate-changed" ;; *) printf '%s\n' "$changed_path" >"$T/tree/.gate-changed" ;; esac
    : >"$T/cargo"
    GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo "FAIL $changed_path gate"; cat "$T/out"; exit 1; }
    if grep -q -- '--skip android_02_tests::' "$T/cargo"; then skipped=1; else skipped=0; fi
    case $changed_path in src/sleep.rs|src/hostsys.rs|docs/DEVELOPING.md+src/sleep.rs) want=1 ;; *) want=0 ;; esac
    [ "$skipped" = "$want" ] || { echo "FAIL Android page tests skipped=$skipped for $changed_path (wanted $want)"; cat "$T/cargo"; exit 1; }
    if [ "$skipped" = 1 ]; then grep -q -- '--skip android_eject_tests::' "$T/cargo" || { echo "FAIL $changed_path: android_eject_tests still run"; exit 1; }; fi
    pass=$((pass + 1))
    echo "ok Android page tests skipped=$skipped for $changed_path"
done
# A translation line reaches the Android pages only when its English text is drawn from a source they use (placeholders are
# cut out; anything unclear counts). Without the list of lines (a gate without git, an old caller) every translation counts.
printf 'tr("Help")\n' >"$T/tree/android/src/ui.rs"; printf 'tr("Deploying {} saves")\n' >"$T/tree/src/eject.rs"; printf 'tr("Only on the Linux shelf")\n' >"$T/tree/src/sleep.rs"
printf 'assets/i18n/de.tsv\n' >"$T/tree/.gate-changed"
for case_keys in 'Help:1' 'Deploying {n} saves:1' 'Only on the Linux shelf:0' 'Only on the Linux shelf|Help:1' 'Ab:1' '-:1' 'none:unknown'; do
    keys=${case_keys%:*}; want=${case_keys##*:}
    rm -f "$T/tree/.gate-i18n-changed"
    [ "$keys" = none ] || printf '%s\n' "$keys" | tr '|' '\n' >"$T/tree/.gate-i18n-changed"
    : >"$T/cargo"
    GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo "FAIL translation gate"; cat "$T/out"; exit 1; }
    if grep -q -- '--skip android_02_tests::' "$T/cargo"; then skipped=1; else skipped=0; fi
    [ "$want" = unknown ] && want=1 skipped_expect=0 || skipped_expect=$((1 - want))
    [ "$want" = 1 ] && skipped_expect=0
    [ "$skipped" = "$skipped_expect" ] || { echo "FAIL translation line '$keys': Android skipped=$skipped (wanted $skipped_expect)"; cat "$T/cargo"; exit 1; }
    pass=$((pass + 1))
    echo "ok translation line '$keys': Android page tests skipped=$skipped"
done
rm -f "$T/tree/.gate-i18n-changed"
printf 'pub fn s() -> u8 { 1 }\n' >"$T/tree/src/sleep.rs"
# The nightly run and the release check (GATE_FULL=1) skip nothing.
printf '%s\n' src/sleep.rs >"$T/tree/.gate-changed"
: >"$T/cargo"
GATE_FULL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo 'FAIL full gate'; cat "$T/out"; exit 1; }
if grep -q -- '--skip' "$T/cargo"; then echo 'FAIL the full gate skipped tests'; exit 1; fi
pass=$((pass + 1))
echo 'ok the full gate skips no Android page test'
# GATE_SKIP_TESTS: known flaky tests wait for the nightly run in the merge gate; GATE_FULL=1 runs them.
: >"$T/cargo"
GATE_SKIP_TESTS='eject::tests::flaky_one the_way_back_x' GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo 'FAIL flaky skip gate'; cat "$T/out"; exit 1; }
grep -q -- '--skip eject::tests::flaky_one --skip the_way_back_x' "$T/cargo" && grep -q 'gate: left to the nightly run (flaky): eject::tests::flaky_one the_way_back_x' "$T/out" \
    || { echo 'FAIL GATE_SKIP_TESTS was not skipped'; cat "$T/cargo"; exit 1; }
: >"$T/cargo"
GATE_FULL=1 GATE_SKIP_TESTS='the_way_back_x' GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { echo 'FAIL flaky full gate'; cat "$T/out"; exit 1; }
if grep -q -- '--skip' "$T/cargo" || grep -q 'left to the nightly' "$T/out"; then echo 'FAIL the full gate skipped a flaky test'; exit 1; fi
pass=$((pass + 1))
echo 'ok GATE_SKIP_TESTS leaves flaky tests to the nightly run, GATE_FULL=1 runs them'
# Sources stamped by the caller (tools/remote-gate.sh: rsync --checksum --no-times) are not touched again; without the
# flag every source is (a local run, where file times say nothing about a shared target).
printf 'x\n' >"$T/tree/src/stamped.rs"; touch -d 2020-01-01 "$T/tree/src/stamped.rs"
printf '%s\n' docs/DEVELOPING.md >"$T/tree/.gate-changed"
GATE_SOURCES_STAMPED=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { cat "$T/out"; exit 1; }
[ "$(stat -c %Y "$T/tree/src/stamped.rs")" = "$(date -d 2020-01-01 +%s)" ] || { echo 'FAIL stamped sources were touched'; exit 1; }
GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || { cat "$T/out"; exit 1; }
[ "$(stat -c %Y "$T/tree/src/stamped.rs")" != "$(date -d 2020-01-01 +%s)" ] || { echo 'FAIL unstamped sources were not touched'; exit 1; }
rm "$T/tree/src/stamped.rs"
pass=$((pass + 1))
echo 'ok sources the caller stamped are not touched again, others are'

printf '%s\n' docs/DEVELOPING.md >"$T/tree/.gate-changed"
: >"$T/cargo"
GATE_FIXTURE_BRICK_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || {
    echo 'FAIL docs-only gate ran the Brick UI walk'; exit 1;
}
if grep -q -- '--features brick' "$T/cargo"; then echo 'FAIL docs-only gate built the Brick'; exit 1; fi
pass=$((pass + 1))
echo 'ok docs-only gate skips the private Brick tiers'
printf '[features]\n' >"$T/tree/Cargo.toml"
printf '%s\n' src/sleep.rs >"$T/tree/.gate-changed"
: >"$T/cargo"
GATE_FIXTURE_BRICK_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || {
    echo 'FAIL public gate required the private Brick feature'; exit 1;
}
if grep -q -- '--features brick' "$T/cargo"; then echo 'FAIL public gate built the Brick'; exit 1; fi
pass=$((pass + 1))
echo 'ok public gate needs no private Brick feature'
# A killed parallel coordinator used to leave missing result files which the gate read as success. Only fixture
# processes are signalled here; no cargo build or real suite runs.
printf '#!/bin/sh\necho 8\n' >"$T/bin/nproc"
cat >"$T/tree/scripts/test-verify-device.sh" <<'INTERRUPT'
#!/bin/sh
kill -KILL "$PPID"
INTERRUPT
rc=0
GATE_FULL=1 GATE_PARALLEL=1 GATE_RUNNER=cargo GATE_PROFILE=gate \
    sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
if [ "$rc" != 1 ] || ! grep -q 'shell test coordinator FAILED' "$T/out" || grep -q '^GATE: ok' "$T/out"; then
    echo "FAIL interrupted shell test coordinator: exit=$rc"
    cat "$T/out"
    exit 1
fi
pass=$((pass + 1))
echo 'ok interrupted shell test coordinator cannot pass the gate'
# The name suites run when a change touches where a console's name comes from (13.11.: the nightly run found "8-bit family").
mkdir -p "$T/names/tests"
for suite in home_panel_labels spine_names text_size; do : >"$T/names/tests/$suite.rs"; done
for changed_path in src/system_names.rs src/app_home_panel.rs assets/data/consoles.json src/library.rs; do
    picked=$(printf '%s\n' "$changed_path" | python3 "$ROOT/scripts/gate-parts.py" "$T/names" | sed -n "s/^heavy='\(.*\)'$/\1/p")
    [ "$picked" = "home_panel_labels spine_names" ] || { echo "FAIL $changed_path picked '$picked'"; exit 1; }
    pass=$((pass + 1))
    echo "ok $changed_path runs the console-name suites"
done
# The proof behind GATE_SOURCES_STAMPED: a copy by rsync --checksum --no-times is rebuilt by cargo wherever a source changed.
sh "$ROOT/scripts/test-gate-stamp.sh" >"$T/stamp.out" 2>&1 || { echo 'FAIL source stamping'; cat "$T/stamp.out"; exit 1; }
pass=$((pass + 1))
echo 'ok a changed source is always rebuilt after the caller stamped it (scripts/test-gate-stamp.sh)'
echo "$pass gate path-selection checks passed"
