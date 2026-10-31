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
cat >"$T/bin/cargo" <<'CARGO'
#!/bin/sh
printf '%s\n' "$*" >>"$GATE_FIXTURE_CARGO"
echo 'test result: ok. 1 passed'
CARGO
chmod +x "$T/bin/cargo"
# Prevent core count or caller settings from changing the scheduling of this selection test.
printf '#!/bin/sh\necho 1\n' >"$T/bin/nproc"
chmod +x "$T/bin/nproc"
for suite in deploy/test-run-pocket-ui.sh scripts/test-verify-device.sh scripts/test-check-core-licences.sh \
    deploy/package/test-package.sh deploy/test-grub-quiet.sh scripts/image/test-grub-checks.sh scripts/image/test-rocknix-source.sh scripts/image/test-h700-pins.sh deploy/fat/test-hook.sh \
    deploy/image/test-mount-storage.sh; do
    mkdir -p "$T/tree/$(dirname "$suite")"
    cat >"$T/tree/$suite" <<'SUITE'
#!/bin/sh
printf '%s\n' "$0" >>"$GATE_FIXTURE_CALLS"
case $0 in */test-package.sh) [ "${GATE_FIXTURE_FAIL:-0}" = 0 ] || exit 1 ;; esac
SUITE
done
# A transitive shell helper must keep the Rust source it names in the shell test set.
printf '# helper reads src/hostsys.rs\n' >"$T/tree/scripts/nested/helpers/check.sh"
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
    pass=$((pass + 1))
    echo "ok $label"
}
check 'LICENSE still runs package tests' 10 1 LICENSE
check 'NOTICE still runs package tests' 10 1 NOTICE.md
check 'font licence runs package tests' 10 0 assets/fonts/OFL.txt
check 'deleted or empty licence is rejected by required package suite' 10 1 LICENSE '' 1
check 'ordinary docs skip shell and integration tests' 0 1 docs/DEVELOPING.md
check 'root Markdown skips shell and integration tests' 0 1 README.md
check 'manual changes retain Rust integration tests' 0 0 docs/user/settings.md
check 'Markdown outside docs is unknown' 10 0 assets/new-input.md
check 'unknown doc configuration runs all tests' 10 0 docs/new-input.conf
check 'unreferenced Rust source skips shell only' 0 0 src/unreferenced_feature.rs
check 'transitive shell source input runs shell tests' 10 0 src/hostsys.rs
check 'non-Rust source input is unknown' 10 0 src/new-input.c
check 'unknown assets run shell tests' 10 0 assets/new-input.txt
check 'unknown root files run shell tests' 10 0 new-input
check 'deploy changes run shell tests' 10 0 deploy/run-pocket-ui.sh
check 'script changes run shell tests' 10 0 scripts/test-gate-paths.sh
check 'Cargo metadata changes run shell tests' 10 0 Cargo.toml
check 'docs cannot cancel licence tests' 10 1 'LICENSE
README.md'
check 'docs cannot cancel unknown input tests' 10 0 'assets/new-input
README.md'
check 'path with spaces remains one unknown path' 10 0 'assets/new input.md'
check 'Markdown path with spaces remains one document' 0 1 'docs/new document.md'
check 'no change list runs everything' 10 0 ''
check 'full override runs everything' 10 0 docs/DEVELOPING.md 1
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
