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
cp "$ROOT/scripts/test-timing.sh" "$T/tree/scripts/test-timing.sh"
cp "$ROOT/scripts/gate-select.py" "$T/tree/scripts/gate-select.py"
cp "$ROOT/scripts/gate-parts.py" "$T/tree/scripts/gate-parts.py"
cp "$ROOT/scripts/gate-cpus.py" "$T/tree/scripts/gate-cpus.py"
sed "s|/tmp/gate-|$T/logs/gate-|g" "$ROOT/scripts/gate-shell.sh" >"$T/tree/scripts/gate-shell.sh"
cat >"$T/bin/cargo" <<'CARGO'
#!/bin/sh
printf '%s\n' "$*" >>"$GATE_FIXTURE_CARGO"
case " $* " in *" doc_checks:: "*) [ "${GATE_FIXTURE_DOC_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" i18n:: "*) [ "${GATE_FIXTURE_I18N_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" timing_ "*) [ "${GATE_FIXTURE_TIMING_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" doc_checks:: "*) if [ "${GATE_FIXTURE_DOC_EMPTY:-0}" = 1 ]; then echo 'test result: ok. 0 passed'; exit 0; fi ;; esac
case " $* " in *" i18n:: "*) if [ "${GATE_FIXTURE_I18N_EMPTY:-0}" = 1 ]; then echo 'test result: ok. 0 passed'; exit 0; fi ;; esac
case " $* " in *" --features brick --test brick_settings_walk "*) [ "${GATE_FIXTURE_BRICK_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" --test text_size textfit_brick_031_ "*) [ "${GATE_FIXTURE_BRICK_TEXT_FAIL:-0}" = 0 ] || exit 1 ;; esac
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
for suite in scripts/image/test-receipts.py scripts/test-device-portability.py scripts/test-device-transfer.py; do
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
        calls=3 ;;
    full)
        grep -q 'timing_ -- --ignored --test-threads=1' "$T/cargo" || { echo 'FAIL timing tier missing'; exit 1; }
        calls=2 ;;
    esac
    [ "$(wc -l <"$T/cargo")" -eq "$calls" ] || { echo "FAIL $label: timing tier retried or regular tests skipped"; cat "$T/cargo"; exit 1; }
    pass=$((pass + 1))
    echo "ok $label"
}
check 'LICENSE still runs package tests' 14 fast LICENSE
check 'NOTICE still runs package tests' 14 fast NOTICE.md
check 'font licence runs package tests' 14 fast assets/fonts/OFL.txt
check 'deleted or empty licence is rejected by required package suite' 14 fast LICENSE '' 1
check 'ordinary docs use only doc and i18n checks' 0 docs docs/DEVELOPING.md
check 'root Markdown skips shell and integration tests' 0 docs README.md
check 'manual changes leave the short path' 0 fast docs/user/settings.md
check 'Markdown outside docs is unknown' 14 fast assets/new-input.md
check 'unknown doc configuration runs the shell tests' 14 fast docs/new-input.conf
check 'one Rust source runs the merge gate without shell tests' 0 fast src/unreferenced_feature.rs
check 'transitive shell source input runs shell tests' 14 fast src/hostsys.rs
check 'one Rust test runs the merge gate' 0 fast tests/layout_contract.rs
check 'Brick text regression changes run the merge gate' 0 fast tests/text_size.rs
check 'transitive shell test input runs shell tests' 14 fast tests/shell_input.rs
check 'unknown test data still runs shell tests' 14 fast tests/new-input.txt
check 'Rust test path with spaces remains unknown' 14 fast 'tests/new input.rs'
check 'Rust test changes cannot cancel deploy tests' 14 fast 'tests/layout_contract.rs
deploy/run-pocket-ui.sh'
check 'full override retains shell tests after Rust test changes' 14 full tests/layout_contract.rs 1
check 'non-Rust source input is unknown' 14 fast src/new-input.c
check 'unknown assets run shell tests' 14 fast assets/new-input.txt
check 'unknown root files run shell tests' 14 fast new-input
check 'deploy changes run shell tests' 14 fast deploy/run-pocket-ui.sh
check 'gate script changes run the full gate' 14 full scripts/test-gate-paths.sh
check 'other script changes run shell tests' 14 fast scripts/device-check.sh
check 'Cargo metadata changes run the full gate' 14 full Cargo.toml
check 'test harness changes run the full gate' 14 full tests/common/mod.rs
check 'docs cannot cancel licence tests' 14 fast 'LICENSE
README.md'
check 'docs cannot cancel unknown input tests' 14 fast 'assets/new-input
README.md'
check 'path with spaces remains one unknown path' 14 fast 'assets/new input.md'
check 'Markdown path with spaces remains one document' 0 docs 'docs/new document.md'
check 'unembedded translation audit notes use short path' 0 docs assets/i18n/skip.txt
check 'runtime translation tables leave the short path' 14 fast assets/i18n/de.tsv
check 'mixed docs and Rust leave the short path' 0 fast 'docs/DEVELOPING.md
src/main.rs'
check 'no change list runs everything' 14 full ''
check 'full override runs everything' 14 full docs/DEVELOPING.md 1
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
for suite in brick_settings_walk text_size long_titles search_titles; do : >"$T/tree/tests/$suite.rs"; done
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
grep -q -- '^check --profile gate --features brick --lib --bins' "$T/cargo" || { echo 'FAIL merge gate lost the Brick compile check'; exit 1; }
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
    grep -q -- '^test --profile gate --no-run' "$T/cargo" || { echo "FAIL $changed_path: not every program compiled"; exit 1; }
    grep -q -- '--test user_journeys' "$T/cargo" || { echo "FAIL $changed_path: journeys skipped"; exit 1; }
    if grep -q -- '--lib --bins.* --test text_size' "$T/cargo"; then ran=1; else ran=0; fi
    case $changed_path in src/sleep.rs) want=0 ;; *) want=1 ;; esac
    [ "$ran" = "$want" ] || { echo "FAIL slow suite text_size ran=$ran for $changed_path"; cat "$T/cargo"; exit 1; }
    pass=$((pass + 1))
    echo "ok slow suite text_size ran=$ran for $changed_path"
done
rm "$T/tree/tests/user_journeys.rs"

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
echo "$pass gate path-selection checks passed"
