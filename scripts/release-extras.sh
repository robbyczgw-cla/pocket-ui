#!/bin/bash
# The release-only checks, run in the nightly full suite (tools/nightly-gate.sh) after scripts/full-suite.sh, so a green night means
# scripts/pre-release.sh finds nothing new at the cut (0.5.1, 18.11.: two wrong Settings paths in the release-text tests were
# found by Pre, not by main, because none of these ran anywhere before the cut).
#
#   bash scripts/release-extras.sh [tree]            every step
#   bash scripts/release-extras.sh --only a,b [tree] only these steps (docs, release-text, text-fit, brick, nav)
#
# Steps (the same commands as the full run of scripts/pre-release.sh, which runs them after its own full suite):
#   docs          scripts/i18n-extract.py --check: every string of src/ has its German line
#   release-text  scripts/release-text-tests.txt (--lib --bins --test i18n_snapshots, --include-ignored), each name must have run
#   text-fit      the text-fit suites' release matrices (--ignored; the full suite ran their ordinary cases), suites as in pre-release
#   brick         with the Brick feature: the extra Large-text cases (text_size textfit_extras)
#   nav           the D-pad explorer's release-only cases (tests/nav_explorer.rs --ignored)
# A red test gets one more run alone in a process of its own (as pre-release.sh's `ct`: language/font tests share a global).
# Output: "release-extras: <step> ok|RED (N s)" per step; a red run ends with a "GATE: release extras FAILED ..." line (the nightly
# reads the first GATE: line as its reason). Every step's test output is appended to /tmp/gate-test-<tree name>.log, where the
# nightly looks for failing tests. A step that ran no test at all is red. Exit 0 green, 1 a step red, 2 usage.
# scripts/doc_checks keeps this file and pre-release.sh in step (src/doc_checks.rs: the release-text names, the suite list).
set -u
ONLY=""
while [ $# -gt 0 ]; do
    case $1 in
        --only) ONLY=",${2:?--only needs steps},"; shift ;;
        -h|--help) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) echo "release-extras: unknown option $1" >&2; exit 2 ;;
        *) TREE=$1 ;;
    esac
    shift
done
cd "${TREE:-$(dirname "$0")/..}" || exit 2
export PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
# A caller's merge-gate sample/cache must not shorten the release-only matrices.
unset POCKETUI_TEST_QUICK POCKETUI_TEST_STATES POCKETUI_SNAPSHOT_CACHE_DIR
NAME=$(basename "$(pwd)")
[ ! -f scripts/run-tmp.sh ] || { . scripts/run-tmp.sh; run_tmp_begin extras; }
# Logs outside the run folder, so a red run stays inspectable.
LOGS=${PUI_TMP_BASE:-/tmp}/release-extras-$NAME
T=/tmp/gate-test-$NAME.log
P="--profile ${GATE_PROFILE:-gate}"
rm -rf "$LOGS"; mkdir -p "$LOGS" || exit 2

RED=""
want() { [ -z "$ONLY" ] || case $ONLY in *",$1,"*) return 0 ;; *) return 1 ;; esac; }
step_end() { # name rc start
    s=$(( $(date +%s) - $3 ))
    if [ "$2" -eq 0 ]; then echo "release-extras: $1 ok ($s s)"
    else echo "release-extras: $1 RED ($s s, $LOGS/$1.log)"; RED="$RED $1"; fi
    [ ! -s "$LOGS/$1.log" ] || cat "$LOGS/$1.log" "$LOGS/$1.log.retry" >>"$T" 2>/dev/null
}
# ct LOG CARGO-ARGS... -- TEST-ARGS...: one cargo test run; a red test gets one more run alone (at most 15 retries). As pre-release.sh.
ct() {
    log=$1; shift
    pre=() post=() seen=0
    for a in "$@"; do
        if [ $seen = 0 ] && [ "$a" = -- ]; then seen=1; elif [ $seen = 0 ]; then pre+=("$a"); else post+=("$a"); fi
    done
    # shellcheck disable=SC2086
    nice -n 10 cargo test $P --no-fail-fast "${pre[@]}" -- "${post[@]}" >>"$log" 2>&1 && return 0
    grep -q '^test result:' "$log" || { echo "    (did not build: $log)" >>"$log"; return 1; }
    failed=$(awk '/^failures:$/ {f=1; next} /^test result/ {f=0} f && /^    [A-Za-z0-9_:]+$/ {print $1}' "$log" | sort -u)
    n=$(printf '%s\n' "$failed" | grep -c .)
    [ "$n" -ge 1 ] && [ "$n" -le 15 ] || { echo "    $n red tests (no retry above 15)" >>"$log"; return 1; }
    keep=""
    for t in $failed; do
        inc=; case " ${post[*]} " in *" --include-ignored "*|*" --ignored "*) inc=--include-ignored ;; esac
        one=$(mktemp "${TMPDIR:-/tmp}/extras-retry.XXXXXX")
        # shellcheck disable=SC2086
        if nice -n 10 cargo test $P "${pre[@]}" -- --exact "$t" --test-threads 1 $inc >"$one" 2>&1 \
            && grep -q '^test result: ok. [1-9]' "$one"; then echo "    flaked side by side, green alone: $t" >>"$log"
        else keep="$keep $t"; fi
        { echo "== retry alone: $t"; cat "$one"; } >>"$log.retry"; rm -f "$one"
    done
    [ -z "$keep" ] || { echo "    red alone too:$keep" >>"$log"; return 1; }
    return 0
}
# A green step that ran no test is red (a typo in a filter or a lost suite must never be a green empty step).
ran_some() { awk '/^test result: ok\./ { n += $4 } END { exit !(n > 0) }' "$1" || { echo "    no test ran" >>"$1"; return 1; }; }

if want docs; then
t0=$(date +%s); : >"$LOGS/docs.log"; rc=0
python3 scripts/i18n-extract.py --check >>"$LOGS/docs.log" 2>&1 || rc=1
step_end docs $rc "$t0"
fi

if want release-text; then
t0=$(date +%s); : >"$LOGS/release-text.log"; rc=0
names=$(grep -v '^#' scripts/release-text-tests.txt | grep .)
# shellcheck disable=SC2086
ct "$LOGS/release-text.log" --lib --bins --test i18n_snapshots -- --include-ignored $names || rc=1
# Each name is a test that ran (libtest filters match substrings: a dead name matches nothing and stays silent).
for n in $names; do
    grep -Eq "^test .*\b$n \.\.\. ok" "$LOGS/release-text.log" || { echo "    release-text-tests.txt: $n did not run (a dead name?)" >>"$LOGS/release-text.log"; rc=1; }
done
step_end release-text $rc "$t0"
fi

TEXT_SUITES="text_size long_titles spine_titles spine_names small_screen home_panel_labels info_page portuguese french italian spanish names_never_translated settings_value_text i18n_snapshots where_states no_wifi_walk"
if want text-fit; then
t0=$(date +%s); : >"$LOGS/text-fit.log"; rc=0; args=()
for s in $TEXT_SUITES; do [ -f "tests/$s.rs" ] && args+=(--test "$s"); done
ct "$LOGS/text-fit.log" "${args[@]}" -- --ignored || rc=1
[ $rc != 0 ] || ran_some "$LOGS/text-fit.log" || rc=1
step_end text-fit $rc "$t0"
fi

if want brick && grep -Eq '^[[:space:]]*brick[[:space:]]*=' Cargo.toml; then
t0=$(date +%s); : >"$LOGS/brick.log"; rc=0
ct "$LOGS/brick.log" --features brick --test text_size -- textfit_extras || rc=1
[ $rc != 0 ] || ran_some "$LOGS/brick.log" || rc=1
step_end brick $rc "$t0"
fi

if want nav; then
t0=$(date +%s); : >"$LOGS/nav.log"; rc=0
ct "$LOGS/nav.log" --test nav_explorer -- --ignored || rc=1
[ $rc != 0 ] || ran_some "$LOGS/nav.log" || rc=1
step_end nav $rc "$t0"
fi

if [ -n "$RED" ]; then
    echo "GATE: release extras FAILED:$RED (logs $LOGS, tests in $T)"
    exit 1
fi
echo "release-extras: ok"
