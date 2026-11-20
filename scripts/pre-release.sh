#!/bin/bash
# Pre-release: every text, document and bookkeeping check of a release, run on the release branch BEFORE the public export,
# so the export and the release check find nothing new (0.4.2 lost ~3 h: each small text find came after a 55-min export).
#
#   sh scripts/pre-release.sh [--receipt FILE] [--logs DIR] [tree]                    the full step (first, on the release tree)
#   sh scripts/pre-release.sh --since RECEIPT [--receipt FILE] [--logs DIR] [tree]    after a text-only fix
#   sh scripts/pre-release.sh --only text-fit,nav [tree]       only these steps (to iterate on a fix; never a receipt)
#
# Steps (each prints "pre-release: <step> ok|RED|WARN|skipped (N s)"; a red step does not stop the others):
#   docs          scripts/i18n-extract.py --check (every string has its German line) and scripts/doc-missing.py (note)
#   full-suite    GATE_FULL=1 scripts/gate.sh: every export/RC test, doctests, serial timing, Brick and shell checks
#   unit          --lib --bins only on the text-only path (the full suite already includes them)
#   release-text  the release check's text tests (scripts/release-text-tests.txt): Settings paths, category lines, every
#                 language complete, the German/Spanish snapshot words
#   text-fit      the text-fit suites with their release matrices (--include-ignored): Large text on every panel size in
#                 every language (text_size), long titles, spines, small screens, panel labels, info pages, pt/es suites
#   brick         with the Brick feature: the Settings walk (Smart Pro 1280x720 text fit too), the Brick text-fit cases, the
#                 Brick and Smart Pro D-pad explorer
#   nav           the D-pad explorer at every size and in Large text (tests/nav_explorer.rs, --ignored)
#   bookkeeping   scripts/bookkeeping.sh: warnings only (doc lists, code-shape counts, page length)
#
# --since RECEIPT (a text-only fix): RECEIPT is an "ok" receipt of this script (or of export-public.sh / release-check.sh)
# for an earlier commit of the same branch. The change since its full run must be text only (scripts/text-only-diff.py:
# translation tables, docs, top-level Markdown), else this stops and says so. Then docs, unit, release-text and bookkeeping
# run always; text-fit and brick only when a translation table or a manual page (docs/user) changed; nav only when a
# translation table changed. Every skipped step is printed with its reason.
#
# --receipt FILE writes, when every step is green, "kind=pre-release result=ok commit=… tree=… mode=full|text-only full=…"; a full run adds nav_* evidence for the release check (scripts/nav-evidence.py).
# scripts/export-public.sh --pre-release FILE checks that its tree is the exported one. Needs a clean git checkout.
# Exit 0 green (warnings allowed), 1 a step red, 2 usage, not a clean checkout, or --since refused.
set -u
SINCE="" RECEIPT="" LOGS="" TREE="" ONLY=""
while [ $# -gt 0 ]; do
    case $1 in
        --since) SINCE=${2:?--since needs a receipt}; shift ;;
        --receipt) RECEIPT=${2:?--receipt needs a file}; shift ;;
        --logs) LOGS=${2:?--logs needs a directory}; shift ;;
        --only) ONLY=",${2:?--only needs steps},"; shift ;;
        -h|--help) sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) echo "pre-release: unknown option $1" >&2; exit 2 ;;
        *) TREE=$1 ;;
    esac
    shift
done
[ -z "$ONLY" ] || [ -z "$RECEIPT" ] || { echo "pre-release: --only runs a part, so it writes no receipt"; exit 2; }
case $SINCE in /*|"") ;; *) SINCE=$(pwd)/$SINCE ;; esac
case $RECEIPT in /*|"") ;; *) RECEIPT=$(pwd)/$RECEIPT ;; esac
cd "${TREE:-$(dirname "$0")/..}" || exit 2
if git rev-parse --git-dir >/dev/null 2>&1; then
    dirty=$(git status --porcelain --untracked-files=no)
    [ -z "$dirty" ] || [ -z "$RECEIPT" ] || { echo "pre-release: uncommitted changes; commit them first (a receipt must name the tree it checked):"; echo "$dirty" | head -5; exit 2; }
    COMMIT=$(git rev-parse HEAD) TREEH=$(git rev-parse 'HEAD^{tree}')
else
    # A copy without .git (the Air's build copy): the checks run, but nothing can be named, so no receipt and no --since.
    [ -z "$RECEIPT" ] && [ -z "$SINCE" ] || { echo "pre-release: not a git checkout: --receipt and --since need the commit"; exit 2; }
    COMMIT=no-git TREEH=no-git
fi
export PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
# A caller's merge-gate sample/cache must not shorten the release-only matrices after the full-gate child exits.
unset POCKETUI_TEST_QUICK POCKETUI_TEST_STATES POCKETUI_SNAPSHOT_CACHE_DIR
# Keep the diagnostic logs outside the temporary test sandbox so red runs remain inspectable.
LOGS=${LOGS:-${TMPDIR:-/tmp}/pre-release-$(printf %.12s "$COMMIT")}
# Inherit one temporary run directory across the gate and all extra release checks.
[ ! -f scripts/run-tmp.sh ] || { . scripts/run-tmp.sh; run_tmp_begin pre; }
P="--profile ${GATE_PROFILE:-gate}"
mkdir -p "$LOGS" || exit 2
rm -f "$LOGS/nav-evidence.txt"   # only this run's nav step may write it
rv() { sed -n "s/^$2=//p" "$1" | head -1; }   # a value of a receipt

MODE=full FULL=$COMMIT tr=1 manual=1
if [ -n "$SINCE" ]; then
    [ -s "$SINCE" ] || { echo "pre-release: no receipt $SINCE"; exit 2; }
    [ "$(rv "$SINCE" result)" = ok ] || { echo "pre-release: $SINCE is not a green receipt (result=$(rv "$SINCE" result))"; exit 2; }
    FULL=$(rv "$SINCE" full); [ -n "$FULL" ] || FULL=$(rv "$SINCE" commit)
    git cat-file -e "$FULL^{commit}" 2>/dev/null || { echo "pre-release: the receipt's full run $FULL is not in this repository"; exit 2; }
    python3 -I scripts/text-only-diff.py "$FULL" HEAD > "$LOGS/text-only-diff.txt"; d=$?
    sed 's/^/pre-release:   /' "$LOGS/text-only-diff.txt"
    if [ $d -ne 0 ]; then
        echo "PRE-RELEASE: REFUSED text-only since $(printf %.12s "$FULL"): the change is not text only (above); run the full step (no --since)"
        exit 2
    fi
    MODE=text-only
    grep -q '^text  assets/i18n/' "$LOGS/text-only-diff.txt" || tr=0
    grep -q '^text  docs/user/' "$LOGS/text-only-diff.txt" || manual=0
fi
echo "pre-release: $MODE on $(printf %.12s "$COMMIT") (tree $(printf %.12s "$TREEH"))$([ "$MODE" = full ] || echo ", text only since the full run $(printf %.12s "$FULL")"); logs $LOGS"

RED="" WARN="" DONE="" START=$(date +%s)
step_end() { # name rc start [warn]
    s=$(( $(date +%s) - $3 ))
    if [ "$2" -eq 0 ]; then echo "pre-release: $1 ok ($s s)"; DONE="$DONE $1:ok"
    elif [ "${4:-}" = warn ]; then echo "pre-release: $1 WARN ($s s, $LOGS/$1.log)"; WARN="$WARN $1"; DONE="$DONE $1:warn"
    else echo "pre-release: $1 RED ($s s, $LOGS/$1.log)"; RED="$RED $1"; DONE="$DONE $1:red"; fi
}
skip() { echo "pre-release: $1 skipped ($2)"; DONE="$DONE $1:skipped"; }
want() { [ -z "$ONLY" ] && return 0; case $ONLY in *",$1,"*) return 0 ;; esac; echo "pre-release: $1 not run (--only)"; return 1; }
# ct LOG CARGO-ARGS... -- TEST-ARGS...: one cargo test run; a red test gets one more run alone in a process of its own (the
# language/font tests share a global and fail only side by side, export-public.sh does the same). At most 15 retries.
ct() {
    log=$1; shift
    pre=() post=() seen=0
    for a in "$@"; do
        if [ $seen = 0 ] && [ "$a" = -- ]; then seen=1; elif [ $seen = 0 ]; then pre+=("$a"); else post+=("$a"); fi
    done
    # shellcheck disable=SC2086
    nice -n 10 cargo test $P --no-fail-fast "${pre[@]}" -- "${post[@]}" >>"$log" 2>&1 && return 0
    grep -q '^test result:' "$log" || { echo "    (did not build: $log)"; return 1; }
    failed=$(awk '/^failures:$/ {f=1; next} /^test result/ {f=0} f && /^    [A-Za-z0-9_:]+$/ {print $1}' "$log" | sort -u)
    n=$(printf '%s\n' "$failed" | grep -c .)
    [ "$n" -ge 1 ] && [ "$n" -le 15 ] || { echo "    $n red tests (no retry above 15)"; return 1; }
    keep=""
    for t in $failed; do
        inc=; case " ${post[*]} " in *" --include-ignored "*|*" --ignored "*) inc=--include-ignored ;; esac
        # shellcheck disable=SC2086
        one=$(mktemp "${TMPDIR:-/tmp}/pre-release-retry.XXXXXX")
        if nice -n 10 cargo test $P "${pre[@]}" -- --exact "$t" --test-threads 1 $inc >"$one" 2>&1 \
            && grep -q '^test result: ok. [1-9]' "$one"; then echo "    flaked side by side, green alone: $t"
        else keep="$keep $t"; fi
        { echo "== retry alone: $t"; cat "$one"; } >>"$log.retry"; rm -f "$one"
    done
    [ -z "$keep" ] || { echo "    red alone too:$keep"; return 1; }
    return 0
}

if want docs; then
t0=$(date +%s); : >"$LOGS/docs.log"; rc=0
python3 scripts/i18n-extract.py --check >>"$LOGS/docs.log" 2>&1 || rc=1
python3 scripts/doc-missing.py >>"$LOGS/docs.log" 2>&1 || echo "    doc-missing.py lists missing entries ($LOGS/docs.log)"
step_end docs $rc "$t0"
fi

if [ "$MODE" = full ] && want full-suite; then
t0=$(date +%s); rc=0
# The same entry point as nightly and RC. GATE_FULL disables every path, quick-matrix and flaky exclusion.
# Do not overlap feature builds or deadline tests with other cargo processes; gate.sh parallelises its safe shell lanes.
sh scripts/full-suite.sh "$(pwd)" >"$LOGS/full-suite.log" 2>&1 || rc=1
step_end full-suite $rc "$t0"
fi

if { [ "$MODE" = text-only ] || [ -n "$ONLY" ]; } && want unit; then
t0=$(date +%s); : >"$LOGS/unit.log"; rc=0
ct "$LOGS/unit.log" --lib --bins -- || rc=1
step_end unit $rc "$t0"
fi

if want release-text; then
t0=$(date +%s); : >"$LOGS/release-text.log"; rc=0
# shellcheck disable=SC2046
ct "$LOGS/release-text.log" --lib --bins --test i18n_snapshots -- --include-ignored $(grep -v '^#' scripts/release-text-tests.txt) || rc=1
step_end release-text $rc "$t0"
fi

TEXT_SUITES="text_size long_titles spine_titles spine_names small_screen home_panel_labels info_page portuguese french italian spanish names_never_translated settings_value_text i18n_snapshots where_states no_wifi_walk"
if ! want text-fit; then :
elif [ $tr = 0 ] && [ $manual = 0 ]; then skip text-fit "no translation table or manual page changed since the full run"
else
    t0=$(date +%s); : >"$LOGS/text-fit.log"; rc=0; args=()
    for s in $TEXT_SUITES; do [ -f "tests/$s.rs" ] && args+=(--test "$s"); done
    # Full gate ran every ordinary case. Only release ignores remain; text-only/--only still need both.
    ignored=--include-ignored
    [ "$MODE" != full ] || [ -n "$ONLY" ] || ignored=--ignored
    ct "$LOGS/text-fit.log" "${args[@]}" -- "$ignored" || rc=1
    step_end text-fit $rc "$t0"
fi

if ! want brick; then :
elif ! grep -Eq '^[[:space:]]*brick[[:space:]]*=' Cargo.toml; then skip brick "no Brick feature in this tree"
elif [ $tr = 0 ] && [ $manual = 0 ]; then skip brick "no translation table or manual page changed since the full run"
else
    t0=$(date +%s); : >"$LOGS/brick.log"; rc=0
    if [ "$MODE" = full ] && [ -z "$ONLY" ]; then
        # Additional Brick Large-text cases, beyond the full gate's brick_031_ selection.
        ct "$LOGS/brick.log" --features brick --test text_size -- textfit_extras || rc=1
    else
        ct "$LOGS/brick.log" --features brick --test brick_settings_walk -- || rc=1
        ct "$LOGS/brick.log" --features brick --test text_size -- textfit_brick_031_ textfit_extras || rc=1
        ct "$LOGS/brick.log" --features brick --test long_titles -- info_titles_031_ || rc=1
        ct "$LOGS/brick.log" --features brick --test nav_explorer -- brick on_the_smart_pro || rc=1
    fi
    step_end brick $rc "$t0"
fi

if ! want nav; then :
elif [ $tr = 0 ]; then skip nav "no translation table changed since the full run"
else
    t0=$(date +%s); : >"$LOGS/nav.log"; rm -f "$LOGS/nav.log.retry"; rc=0
    ignored=--include-ignored
    [ "$MODE" != full ] || [ -n "$ONLY" ] || ignored=--ignored
    ct "$LOGS/nav.log" --test nav_explorer -- "$ignored" || rc=1
    # Evidence for the release check of this same tree (scripts/nav-evidence.py): it carries these cases instead of a second
    # ~27-min run only when the list, the clean log and the environment match exactly. Full mode with a receipt only.
    if [ $rc = 0 ] && [ "$ignored" = --ignored ] && [ -n "$RECEIPT" ]; then
        { nice -n 10 cargo test $P --test nav_explorer -q -- --ignored --list >"$LOGS/nav-list.txt" 2>/dev/null \
            && python3 scripts/nav-evidence.py record --log "$LOGS/nav.log" --list "$LOGS/nav-list.txt"; } >"$LOGS/nav-evidence.txt" \
            || echo 'nav_carry=no (the case list did not build)' >"$LOGS/nav-evidence.txt"
        sed -n 's/^nav_carry=no /    nav evidence: the release check runs these cases again /p' "$LOGS/nav-evidence.txt"
    fi
    step_end nav $rc "$t0"
fi

if want bookkeeping; then
t0=$(date +%s); rc=0
sh scripts/bookkeeping.sh . >"$LOGS/bookkeeping.log" 2>&1; b=$?
grep -E '^(bookkeeping: WARNING|BOOKKEEPING:)' "$LOGS/bookkeeping.log" | sed 's/^/    /'
if [ $b -ge 2 ]; then step_end bookkeeping 1 "$t0"
elif grep -q '^bookkeeping: WARNING' "$LOGS/bookkeeping.log"; then step_end bookkeeping 1 "$t0" warn
else step_end bookkeeping 0 "$t0"; fi
fi

if [ -n "$RED" ]; then
    echo "PRE-RELEASE: RED:$RED (logs $LOGS). Fix on the release branch, then run this again; nothing is exported yet."
    exit 1
fi
if [ -n "$RECEIPT" ]; then
    { echo "kind=pre-release"; echo "result=ok"; echo "mode=$MODE"; echo "commit=$COMMIT"; echo "tree=$TREEH"; echo "full=$FULL"
      echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"; echo "steps=${DONE# }"
      [ ! -s "$LOGS/nav-evidence.txt" ] || cat "$LOGS/nav-evidence.txt"; } >"$RECEIPT"
    echo "pre-release: receipt $RECEIPT"
fi
echo "PRE-RELEASE: ok ($MODE${ONLY:+, only$(echo "$ONLY" | tr , ' ')}${WARN:+, warnings:$WARN}; wall $(( $(date +%s) - START )) s)"
