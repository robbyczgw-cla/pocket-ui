#!/bin/sh
# Fixture test for scripts/host-budgets.sh (25.11.2026): a red budget is a warning with its numbers and exit 0; a budget
# list whose tests no longer exist is exit 2, never a silent pass.  sh scripts/test-host-budgets.sh
set -u
S=$(cd "$(dirname "$0")" && pwd); T=$(mktemp -d "${TMPDIR:-/tmp}/host-budgets-test.XXXXXX"); trap 'rm -rf "$T"' EXIT; fail=0
mkdir -p "$T/tree/scripts"
cp "$S/host-budgets.sh" "$S/bookkeeping.sh" "$T/tree/scripts/"
printf 'test lib glow::tests::the_glow_is_cheap\ntest settings_open first_frame_is_quick\nsuite launch_frames\n' >"$T/tree/scripts/host-budgets.txt"
cat >"$T/cargo" <<'CARGO'
#!/bin/sh
printf '%s\n' "$*" >>"$FIXTURE_CALLS"
case " $* " in
*" --lib "*) [ "${FIXTURE_NONE:-0}" = 1 ] && { echo 'test result: ok. 0 passed; 0 failed'; exit 0; }
    printf -- '---- glow::tests::the_glow_is_cheap stdout ----\nthe glow 900 us, the wood 300 us: 3.00x\n\nfailures:\n'
    echo 'test glow::tests::the_glow_is_cheap ... FAILED'; echo 'test result: FAILED. 0 passed; 1 failed'; exit 101 ;;
*" --test "*) [ "${FIXTURE_NONE:-0}" = 1 ] && { echo 'test result: ok. 0 passed; 0 failed'; exit 0; }
    echo 'test result: ok. 2 passed; 0 failed' ;;
esac
CARGO
chmod +x "$T/cargo"
export HOST_BUDGETS_CARGO="$T/cargo" FIXTURE_CALLS="$T/calls"
out=$(sh "$T/tree/scripts/host-budgets.sh" "$T/tree"); rc=$?
[ $rc = 0 ] && echo "ok   a red budget is no failure (exit 0)" || { echo "FAIL exit $rc: $out"; fail=1; }
case $out in *"host-budget: WARNING glow::tests::the_glow_is_cheap"*"3.00x"*) echo "ok   the warning names the test and its numbers" ;; *) echo "FAIL no warning: $out"; fail=1 ;; esac
case $out in *"HOST-BUDGETS: 1 warnings (5 tests run)"*) echo "ok   the count line" ;; *) echo "FAIL count line: $out"; fail=1 ;; esac
grep -q -- '--lib -- --exact --test-threads=1 glow::tests::the_glow_is_cheap' "$T/calls" && grep -q -- '--test settings_open -- --exact --test-threads=1 first_frame_is_quick' "$T/calls" \
    && grep -q -- '--test launch_frames -- --test-threads=1' "$T/calls" && echo "ok   each runs alone (one thread)" || { echo "FAIL calls:"; cat "$T/calls"; fail=1; }
FIXTURE_NONE=1 sh "$T/tree/scripts/host-budgets.sh" "$T/tree" >/dev/null 2>&1; rc=$?
[ $rc = 2 ] && echo "ok   no budget test ran: exit 2" || { echo "FAIL renamed budgets passed silently (exit $rc)"; fail=1; }
[ $fail = 0 ] && echo "test-host-budgets: ok" || { echo "test-host-budgets: FAILED"; exit 1; }
