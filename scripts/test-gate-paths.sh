#!/bin/sh
# Exercise the real gate's path selection in an isolated tree, with stand-in cargo and shell suites. No builds.
set -eu
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
cat >"$T/bin/cargo" <<'CARGO'
#!/bin/sh
printf '%s\n' "$*" >>"$GATE_FIXTURE_CARGO"
case " $* " in *" doc_checks:: "*) [ "${GATE_FIXTURE_DOC_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" i18n:: "*) [ "${GATE_FIXTURE_I18N_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" timing_ "*) [ "${GATE_FIXTURE_TIMING_FAIL:-0}" = 0 ] || exit 1 ;; esac
case " $* " in *" doc_checks:: "*) if [ "${GATE_FIXTURE_DOC_EMPTY:-0}" = 1 ]; then echo 'test result: ok. 0 passed'; exit 0; fi ;; esac
case " $* " in *" i18n:: "*) if [ "${GATE_FIXTURE_I18N_EMPTY:-0}" = 1 ]; then echo 'test result: ok. 0 passed'; exit 0; fi ;; esac
case " $* " in *" --test brick_settings_walk "*) [ "${GATE_FIXTURE_BRICK_FAIL:-0}" = 0 ] || exit 1 ;; esac
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
cat >"$T/tree/scripts/image/test-receipts.py" <<'RECEIPTS'
import os, sys
with open(os.environ['GATE_FIXTURE_CALLS'], 'a') as calls:
    calls.write(sys.argv[0] + '\n')
RECEIPTS
# A transitive shell helper must keep the Rust source it names in the shell test set.
printf '# helper reads src/hostsys.rs\n' >"$T/tree/scripts/nested/helpers/check.sh"
printf '# helper reads tests/shell_input.rs\n' >>"$T/tree/scripts/nested/helpers/check.sh"
export GATE_FIXTURE_BIN="$T/bin" TMPDIR="$T"
export GATE_FIXTURE_CARGO="$T/cargo" GATE_FIXTURE_CALLS="$T/calls"
pass=0
check() {
    label=$1 want_shell=$2 want_docs=$3 paths=$4 full=${5:-} bad_package=${6:-0}
    : >"$T/cargo"; : >"$T/calls"
    printf '%s\n' "$paths" >"$T/tree/.gate-changed"
    rc=0
    GATE_FULL="$full" GATE_PARALLEL=0 GATE_RUNNER=cargo GATE_PROFILE=gate GATE_FIXTURE_FAIL="$bad_package" \
        sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
    actual_shell=$(wc -l <"$T/calls" | tr -d ' ')
    actual_docs=0
    if grep -q -- '--lib --bins' "$T/cargo"; then actual_docs=1; fi
    expected_rc=0
    [ "$want_shell" = 0 ] || [ "$bad_package" = 0 ] || expected_rc=1
    if [ "$actual_shell" != "$want_shell" ] || [ "$actual_docs" != "$want_docs" ] || [ "$rc" != "$expected_rc" ]; then
        echo "FAIL $label: shell=$actual_shell docs=$actual_docs exit=$rc (wanted $want_shell $want_docs $expected_rc)"
        cat "$T/out"
        exit 1
    fi
    if [ "$want_docs" = 1 ]; then
        grep -q 'doc_checks::' "$T/cargo" && grep -q 'i18n::' "$T/cargo" || { echo 'FAIL short path lost doc/i18n checks'; exit 1; }
        if grep -q timing_ "$T/cargo"; then echo 'FAIL short path ran player timing tests'; exit 1; fi
    else
        grep -q 'timing_ -- --ignored --test-threads=1' "$T/cargo" || { echo 'FAIL timing tier missing'; exit 1; }
    fi
    [ "$(wc -l <"$T/cargo")" -eq 2 ] || { echo "FAIL timing tier retried or regular tests skipped"; exit 1; }
    pass=$((pass + 1))
    echo "ok $label"
}
check 'LICENSE still runs package tests' 12 0 LICENSE
check 'NOTICE still runs package tests' 12 0 NOTICE.md
check 'font licence runs package tests' 12 0 assets/fonts/OFL.txt
check 'deleted or empty licence is rejected by required package suite' 12 0 LICENSE '' 1
check 'ordinary docs use only doc and i18n checks' 0 1 docs/DEVELOPING.md
check 'root Markdown skips shell and integration tests' 0 1 README.md
check 'manual changes retain the full gate' 12 0 docs/user/settings.md
check 'Markdown outside docs is unknown' 12 0 assets/new-input.md
check 'unknown doc configuration runs all tests' 12 0 docs/new-input.conf
check 'one Rust source selects the full gate' 12 0 src/unreferenced_feature.rs
check 'transitive shell source input runs shell tests' 12 0 src/hostsys.rs
check 'one Rust test selects the full gate' 12 0 tests/layout_contract.rs
check 'Brick text regression changes retain the full gate' 12 0 tests/text_size.rs
check 'transitive shell test input runs shell tests' 12 0 tests/shell_input.rs
check 'unknown test data still runs shell tests' 12 0 tests/new-input.txt
check 'Rust test path with spaces remains unknown' 12 0 'tests/new input.rs'
check 'Rust test changes cannot cancel deploy tests' 12 0 'tests/layout_contract.rs
deploy/run-pocket-ui.sh'
check 'full override retains shell tests after Rust test changes' 12 0 tests/layout_contract.rs 1
check 'non-Rust source input is unknown' 12 0 src/new-input.c
check 'unknown assets run shell tests' 12 0 assets/new-input.txt
check 'unknown root files run shell tests' 12 0 new-input
check 'deploy changes run shell tests' 12 0 deploy/run-pocket-ui.sh
check 'script changes run shell tests' 12 0 scripts/test-gate-paths.sh
check 'Cargo metadata changes run shell tests' 12 0 Cargo.toml
check 'docs cannot cancel licence tests' 12 0 'LICENSE
README.md'
check 'docs cannot cancel unknown input tests' 12 0 'assets/new-input
README.md'
check 'path with spaces remains one unknown path' 12 0 'assets/new input.md'
check 'Markdown path with spaces remains one document' 0 1 'docs/new document.md'
check 'unembedded translation audit notes use short path' 0 1 assets/i18n/skip.txt
check 'runtime translation tables retain full gate' 12 0 assets/i18n/de.tsv
check 'mixed docs and Rust retain full gate' 12 0 'docs/DEVELOPING.md
src/main.rs'
check 'no change list runs everything' 12 0 ''
check 'full override runs everything' 12 0 docs/DEVELOPING.md 1
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
check 'on-disk player contract outside doc tier remains full path' 12 0 docs/player-contract.md
rm "$T/tree/src/runtime.rs"
printf 'include_str!("../docs/embedded.md");\n' >"$T/tree/src/embedded.rs"
check 'compiled document cannot use short path' 12 0 docs/embedded.md
printf 'include_str! { "../docs/embedded.md" };\n' >"$T/tree/src/embedded.rs"
check 'alternate embed delimiter remains full path' 12 0 docs/embedded.md
printf 'include_str!(concat!("../docs/", env!("NAME")));\n' >"$T/tree/src/embedded.rs"
check 'unknown dynamic embed remains full path' 12 0 docs/DEVELOPING.md
printf 'include_str!("docs/embedded.md");\n' >"$T/tree/build.rs"
rm "$T/tree/src/embedded.rs"
check 'build script embedded document remains full path' 12 0 docs/embedded.md
rm "$T/tree/build.rs"
mkdir -p "$T/tree/tests"
printf 'include_str!("../docs/embedded.md");\n' >"$T/tree/tests/embed.rs"
check 'integration fixture embedded document remains full path' 12 0 docs/embedded.md
rm "$T/tree/tests/embed.rs"
mkdir -p "$T/tree/docs"
ln -s ../README.md "$T/tree/docs/link.md"
check 'document symlink remains full path' 12 0 docs/link.md
rm "$T/tree/docs/link.md"
printf '%s\n' src/main.rs >"$T/tree/.gate-changed"
# A red timing result cannot be retried into green.
rc=0
GATE_FIXTURE_TIMING_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
[ "$rc" = 1 ] && grep -q 'GATE: tests FAILED' "$T/out" || { echo 'FAIL red timing tier'; exit 1; }
pass=$((pass + 1))
echo 'ok red timing tier fails the gate'
rc=0
GATE_FIXTURE_TIMING_EMPTY=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
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
    GATE_FIXTURE_BRICK_FAIL=$broken GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
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
GATE_FIXTURE_BRICK_TEXT_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
[ "$rc" = 1 ] && grep -q -- '--features brick --test text_size textfit_brick_031_' "$T/cargo" || {
    echo 'FAIL red Brick text regressions did not fail the gate'; exit 1;
}
pass=$((pass + 1))
echo 'ok red Brick text regressions fail the gate'
: >"$T/cargo"
rc=0
GATE_FIXTURE_INFO_TITLE_FAIL=1 GATE_RUNNER=cargo sh "$T/tree/scripts/gate.sh" "$T/tree" >"$T/out" 2>&1 || rc=$?
[ "$rc" = 1 ] && grep -q -- '--features brick --test long_titles info_titles_031_' "$T/cargo" || {
    echo 'FAIL red Brick info title regression did not fail the gate'; exit 1;
}
pass=$((pass + 1))
echo 'ok red Brick info title regression fails the gate'

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
