#!/bin/bash
# Test for the text-only release path (docs/dev/release.md, "Text-only fixes"): scripts/text-only-diff.py, and the gates of
# scripts/pre-release.sh --since, scripts/export-public.sh --text-only-since and RELEASE_CHECK_TEXT_ONLY in
# scripts/release-check.sh. Throw-away git repositories and stand-in build commands only; no real cargo or devices.
#   sh scripts/test-release-fast.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=fixture GIT_AUTHOR_EMAIL=fixture@example.invalid GIT_COMMITTER_NAME=fixture GIT_COMMITTER_EMAIL=fixture@example.invalid
export POCKETUI_PUBLIC_EMAIL=contributors@example.invalid
pass=0 fail=0
ok() { printf 'ok    %s\n' "$1"; pass=$((pass + 1)); }
bad() { printf 'FAIL  %s\n' "$1"; [ -z "${2:-}" ] || printf '%s\n' "$2" | head -12 | sed 's/^/      | /'; fail=$((fail + 1)); }
# expect NAME RC-WANTED PATTERN: the last run's status and output
expect() { if [ "$RC" = "$2" ] && printf '%s\n' "$OUT" | grep -Eq -- "$3"; then ok "$1"; else bad "$1 (rc $RC, want $2, /$3/)" "$OUT"; fi; }

R=$TMP/repo
mkdir -p "$R/scripts" "$R/assets/i18n" "$R/docs/user" "$R/src"
cp "$ROOT/scripts/text-only-diff.py" "$ROOT/scripts/release-check.sh" "$ROOT/scripts/release-text-tests.txt" "$R/scripts/"
printf 'docs/private/  # notes\n' > "$R/scripts/public-exclude.txt"
printf 'Hello\tHallo\n' > "$R/assets/i18n/de.tsv"; printf '# Manual\n' > "$R/docs/user/start.md"
printf '# demo\n' > "$R/README.md"; printf 'pub fn f() -> u32 { 1 }\n' > "$R/src/lib.rs"
git -C "$R" init -q -b main && git -C "$R" add -A && git -C "$R" commit -q -m base
BASE=$(git -C "$R" rev-parse HEAD)
d() { OUT=$(python3 -I "$ROOT/scripts/text-only-diff.py" "$@" --repo "$R" 2>&1); RC=$?; }

# --- text-only-diff.py -------------------------------------------------------------------------------------------
d "$BASE" HEAD; expect "nothing changed is text only" 0 '^text-only: yes, 0 files'
printf 'Hello\tHallo!\n' > "$R/assets/i18n/de.tsv"; printf '# Manual\nmore\n' > "$R/docs/user/start.md"; printf '# demo!\n' > "$R/README.md"
git -C "$R" commit -qam text; TEXT1=$(git -C "$R" rev-parse HEAD)
d "$BASE" HEAD; expect "a translation, a manual page and README are text only" 0 'text-only: yes, 3 files .*translations: yes, manual: yes'
printf 'pub fn f() -> u32 { 2 }\n' > "$R/src/lib.rs"; git -C "$R" commit -qam code; CODE=$(git -C "$R" rev-parse HEAD)
d "$BASE" HEAD; expect "a source change is not text only" 1 'text-only: NO, 1 of 4 files are not text: src/lib.rs'
d "$BASE" HEAD; expect "every file is listed with its class" 1 '^CODE  src/lib.rs'
mkdir -p "$R/docs/dev"; git -C "$R" mv README.md docs/dev/README.md && git -C "$R" commit -qm move
d "$CODE" HEAD; expect "a move between text places is text only (both paths named)" 0 'text-only: yes, 2 files'
mkdir -p "$R/docs/sub"; printf 'x\n' > "$R/scripts/x.sh"; git -C "$R" add -A && git -C "$R" commit -qm script
d "$CODE" HEAD; expect "a script is not text" 1 'not text: scripts/x.sh'
d nosuchref HEAD; expect "a bad ref is a usage error" 2 'git diff'
git -C "$R" reset -q --hard "$TEXT1"

# --- pre-release.sh --since ----------------------------------------------------------------------------------------
receipt() { printf 'kind=%s\nresult=%s\nmode=full\ncommit=%s\ntree=%s\nfull=%s\n' "$1" "$2" "$3" "$(git -C "$R" rev-parse "$3^{tree}")" "$3" > "$4"; }
receipt pre-release ok "$BASE" "$TMP/pr-base.txt"
receipt pre-release red "$BASE" "$TMP/pr-red.txt"
pr() { OUT=$(cd "$TMP" && bash "$ROOT/scripts/pre-release.sh" "$@" --logs "$TMP/logs" "$R" 2>&1); RC=$?; }
pr --since "$TMP/pr-red.txt"; expect "pre-release refuses a red receipt" 2 'not a green receipt'
pr --since "$TMP/none.txt"; expect "pre-release refuses a missing receipt" 2 'no receipt'
git -C "$R" checkout -q "$CODE"
pr --since "$TMP/pr-base.txt"; expect "pre-release refuses a code change after the full run" 2 'REFUSED text-only since .*not text only'
pr --since "$TMP/pr-base.txt"; expect "pre-release prints the changed file" 2 'CODE  src/lib.rs'
git -C "$R" checkout -q "$TEXT1"; printf 'dirty\n' >> "$R/README.md"
pr --receipt "$TMP/new.txt"; expect "pre-release writes no receipt for uncommitted changes" 2 'uncommitted changes'
[ ! -e "$TMP/new.txt" ] && ok "no receipt file was made" || bad "a receipt was made for a dirty tree"
git -C "$R" checkout -q -- README.md

# --- export-public.sh --text-only-since -----------------------------------------------------------------------------
receipt export ok "$BASE" "$TMP/exp-base.txt"
ex() { OUT=$(bash "$ROOT/scripts/export-public.sh" "$TMP/out-$RANDOM" --repo "$R" --version 1.2.3 "$@" 2>&1); RC=$?; }
receipt pre-release ok "$TEXT1" "$TMP/pr-text1.txt"
ex --ref "$TEXT1" --text-only-since "$TMP/exp-base.txt"; expect "export: text-only needs a pre-release receipt" 2 'needs --pre-release'
ex --ref "$TEXT1" --text-only-since "$TMP/pr-base.txt" --pre-release "$TMP/pr-text1.txt"; expect "export: the base must be an export receipt" 2 'not the receipt of a green export'
ex --ref "$TEXT1" --text-only-since "$TMP/exp-base.txt" --pre-release "$TMP/pr-base.txt"; expect "export: the pre-release receipt must be of this tree" 2 'checked tree .*not this commit'
receipt pre-release ok "$CODE" "$TMP/pr-code.txt"
ex --ref "$CODE" --text-only-since "$TMP/exp-base.txt" --pre-release "$TMP/pr-code.txt"; expect "export: a code change is refused" 1 'not a text-only change since'
ex --ref "$TEXT1" --text-only-since "$TMP/exp-base.txt" --pre-release "$TMP/pr-text1.txt" --skip-project-checks; expect "export: text-only and skip-project-checks exclude each other" 2 'do not go together'
ex --ref "$TEXT1" --text-only-since "$TMP/exp-base.txt" --pre-release "$TMP/pr-text1.txt"
# The fixture is no Rust project, so the licence step stops it; what matters is that the text-only decision came first and was printed.
expect "export: a text-only change is accepted and its files are printed" 1 'text  assets/i18n/de.tsv'
expect "export: the pre-release steps are named" 1 'checks of this tree: pre-release full'

# --- release-check.sh RELEASE_CHECK_TEXT_ONLY ------------------------------------------------------------------------
receipt release-check ok "$BASE" "$TMP/rc-base.txt"; echo "volume_sha256=$(printf '0%.0s' $(seq 64))" >> "$TMP/rc-base.txt"
rc() { OUT=$(cd "$R" && env RELEASE_CHECK_TEXT_ONLY="$1" sh scripts/release-check.sh 2>&1); RC=$?; }
rc "$TMP/exp-base.txt"; expect "release check: text-only needs a release-check receipt" 1 'not the receipt of a green release check'
git -C "$R" checkout -q "$CODE"
rc "$TMP/rc-base.txt"; expect "release check: a code change is refused" 1 'FAILED \(not a text-only change'
git -C "$R" checkout -q "$TEXT1"
rc "$TMP/rc-base.txt"
# No AArch64 programs in the fixture: the check stops at the STORAGE step, after the text-only decision.
expect "release check: a text-only change passes the gate of the mode and is printed" 1 'TEXT-ONLY since the full release check'
expect "release check: the changed files are printed" 1 'text  docs/user/start.md'
grep -v -e TEXT_ONLY -e RECEIPT "$TMP/rc-base.txt" | sed 's/^kind=.*/kind=release-check/' | grep -v volume > "$TMP/rc-novol.txt"
rc "$TMP/rc-novol.txt"; expect "release check: a receipt without a verified volume binary is refused" 1 'names no verified volume binary'

# --- Full pre-release and pre-cut barrier (real entry points, stand-in gate/cargo) -----------------------------------
F=$TMP/full
mkdir -p "$F/scripts" "$F/src" "$TMP/bin"
cp "$ROOT/scripts/full-suite.sh" "$ROOT/scripts/pre-cut-check.py" "$F/scripts/"
sed 's|^export PATH=.*|export PATH="$RELEASE_FIXTURE_BIN:$PATH"|' "$ROOT/scripts/pre-release.sh" > "$F/scripts/pre-release.sh"
printf '[features]\n' > "$F/Cargo.toml"
printf 'pub fn f() {}\n' > "$F/src/lib.rs"
cp "$ROOT/scripts/release-text-tests.txt" "$F/scripts/"
printf 'import sys\nsys.exit(0)\n' > "$F/scripts/i18n-extract.py"
cp "$F/scripts/i18n-extract.py" "$F/scripts/doc-missing.py"
printf '#!/bin/sh\necho "BOOKKEEPING: 0 warnings"\n' > "$F/scripts/bookkeeping.sh"
cat > "$F/scripts/gate.sh" <<'GATE'
#!/bin/sh
[ "${GATE_FULL:-}" = 1 ] || { echo 'fixture: full selection missing'; exit 1; }
echo full-gate >> "$RELEASE_FIXTURE_CALLS"
if [ "${RELEASE_FIXTURE_RED:-0}" = 1 ]; then
    echo 'test settings_open::release_only_regression ... FAILED'
    exit 1
fi
[ "${RELEASE_FIXTURE_MUTATE:-0}" = 0 ] || echo '// changed while running' >> src/lib.rs
echo 'GATE: ok (1 passed)'
GATE
cat > "$TMP/bin/cargo" <<'CARGO'
#!/bin/sh
[ -z "${POCKETUI_TEST_QUICK+x}${POCKETUI_TEST_STATES+x}${POCKETUI_SNAPSHOT_CACHE_DIR+x}" ] || { echo 'fixture: inherited release sample/cache'; exit 1; }
echo "$*" >> "$RELEASE_FIXTURE_CALLS"
echo 'test result: ok. 1 passed'
CARGO
chmod +x "$TMP/bin/cargo"
export RELEASE_FIXTURE_BIN="$TMP/bin" RELEASE_FIXTURE_CALLS="$TMP/full-calls"
git -C "$F" init -q -b main && git -C "$F" add -A && git -C "$F" commit -qm fixture
SHA=$(git -C "$F" rev-parse HEAD)
fullpre() { OUT=$(RELEASE_FIXTURE_RED="$1" bash "$F/scripts/pre-release.sh" --receipt "$TMP/full-pre.txt" --logs "$TMP/full-logs" "$F" 2>&1); RC=$?; }
fullpre 1; expect "pre catches a failing release-only test before any export" 1 'PRE-RELEASE: RED: full-suite'
[ ! -e "$TMP/full-pre.txt" ] && ok "red full suite issues no pre receipt" || bad "red full suite issued a receipt"
grep -q 'release_only_regression' "$TMP/full-logs/full-suite.log" && ok "pre keeps the failing test in its log" || bad "pre lost failing test"
fullpre 0; expect "pre runs full suite and all release extras successfully" 0 'PRE-RELEASE: ok'
grep -q '^steps=.*full-suite:ok' "$TMP/full-pre.txt" && ok "pre receipt records the full suite" || bad "pre receipt misses full suite"
grep -q "^nav_carry=" "$TMP/full-pre.txt" && ok "pre receipt says whether the nav cases can be carried" || bad "pre receipt has no nav_carry line"
POCKETUI_TEST_QUICK=1 POCKETUI_TEST_STATES=sample POCKETUI_SNAPSHOT_CACHE_DIR="$TMP/foreign-cache" fullpre 0
expect "pre clears inherited samples/cache for all release extras" 0 'PRE-RELEASE: ok'

fresh() { OUT=$(RELEASE_FIXTURE_RED="$1" sh "$F/scripts/full-suite.sh" --receipt "$TMP/fresh.json" "$F" 2>&1); RC=$?; }
cutcheck() { OUT=$(python3 "$ROOT/scripts/pre-cut-check.py" --repo "$F" --main "$1" --receipt "$2" 2>&1); RC=$?; }
cutcheck "$SHA" "$TMP/missing.json"; expect "cut refused without receipt" 2 'PRE-CUT: REFUSED'
fresh 0; expect "fresh full run writes receipt" 0 'full-suite: green receipt'
cutcheck "$SHA" "$TMP/fresh.json"; expect "cut accepts exact green full-suite commit" 0 'PRE-CUT: ok'
cp "$TMP/fresh.json" "$TMP/good.json"
fresh 1; expect "fresh red run fails" 1 'release_only_regression'
[ ! -e "$TMP/fresh.json" ] && ok "fresh red run invalidates old green output" || bad "old green output survived red run"
git -C "$F" commit --allow-empty -qm next
NEXT=$(git -C "$F" rev-parse HEAD)
cutcheck "$NEXT" "$TMP/good.json"; expect "same tree at different commit needs its own receipt" 2 'sha='
fresh 0
python3 - "$TMP/fresh.json" "$TMP/nightly.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1])); d['kind'] = 'nightly'
json.dump(d, open(sys.argv[2], 'w'))
PY
cutcheck "$NEXT" "$TMP/nightly.json"; expect "cut accepts exact green nightly receipt" 0 'PRE-CUT: ok'
for field in verdict:red mode:text-only suite:fixture tree:wrong profile:quick kind:export; do
    python3 - "$TMP/fresh.json" "$TMP/bad.json" "$field" <<'PY'
import json, sys
d = json.load(open(sys.argv[1])); k,v = sys.argv[3].split(':'); d[k]=v
json.dump(d, open(sys.argv[2], 'w'))
PY
    cutcheck "$NEXT" "$TMP/bad.json"; expect "cut refuses altered receipt $field" 2 'PRE-CUT: REFUSED'
done
printf '{broken\n' > "$TMP/bad.json"
cutcheck "$NEXT" "$TMP/bad.json"; expect "cut refuses malformed receipt" 2 'PRE-CUT: REFUSED'
printf '[]\n' > "$TMP/bad.json"
cutcheck "$NEXT" "$TMP/bad.json"; expect "cut refuses nonobject receipt with clear message" 2 'receipt must be a JSON object'
printf '// untracked\n' > "$F/src/untracked.rs"
fresh 0; expect "fresh receipt refuses untracked source" 2 'clean checkout'
rm "$F/src/untracked.rs"
git -C "$F" checkout -qb feature
git -C "$F" commit --allow-empty -qm feature
fresh 0
cutcheck HEAD "$TMP/fresh.json"; expect "feature branch receipt cannot authorize a main cut" 2 'PRE-CUT: REFUSED'
git -C "$F" checkout -q main
RELEASE_FIXTURE_MUTATE=1 fresh 0; expect "source mutation during full run issues no receipt" 2 'checkout changed during run'
git -C "$F" checkout -q -- src/lib.rs

# The nightly driver checks this same frozen-source guard after synchronisation and tests.
sourcecheck() { OUT=$(python3 "$ROOT/scripts/pre-cut-check.py" --repo "$F" --main "$NEXT" --verify-source 2>&1); RC=$?; }
sourcecheck; expect "nightly source guard accepts unchanged clean commit" 0 '^$'
printf '// dirty\n' >> "$F/src/lib.rs"
sourcecheck; expect "nightly source guard refuses changed tracked source" 2 'source changed during full-suite'
git -C "$F" checkout -q -- src/lib.rs
printf '// untracked\n' > "$F/src/untracked.rs"
sourcecheck; expect "nightly source guard refuses new untracked source" 2 'source changed during full-suite'
rm "$F/src/untracked.rs"
git -C "$F" checkout -q "$SHA"
sourcecheck; expect "nightly source guard refuses moved checkout" 2 'source changed during full-suite'
git -C "$F" checkout -q main

# Test the prepared real driver too, using a temporary HOME, with no contact to the held release host.
driver=$ROOT/maintainer/release-prevention/build-all.sh
if [ -f "$driver" ]; then
    mkdir -p "$TMP/home/pocket-review" "$TMP/driver/scripts"
    ln -s "$F" "$TMP/home/pocket-review/pocket-ui"
    cp "$driver" "$TMP/driver/scripts/build-all.sh"
    cat > "$TMP/driver/scripts/0-bump.sh" <<'BUMP'
#!/bin/sh
echo bump >> "$RELEASE_FIXTURE_CALLS"
BUMP
    : > "$RELEASE_FIXTURE_CALLS"
    OUT=$(HOME="$TMP/home" STOP_AFTER=bump FULL_SUITE_RECEIPT="$TMP/missing.json" \
        bash "$TMP/driver/scripts/build-all.sh" 9.9.9 fixture "$NEXT" 2099-01-01 2>&1); RC=$?
    expect "build-all refuses cut before creating worktree or bump" 2 'PRE-CUT: REFUSED'
    [ ! -e "$TMP/home/pocket-review/wt-relfixture" ] && [ ! -s "$RELEASE_FIXTURE_CALLS" ] && ok "refused driver has no bump side effects" || bad "refused driver changed release state"
    fresh 0
    OUT=$(HOME="$TMP/home" STOP_AFTER=bump FULL_SUITE_RECEIPT="$TMP/fresh.json" \
        bash "$TMP/driver/scripts/build-all.sh" 9.9.9 fixture "$NEXT" 2099-01-01 2>&1); RC=$?
    expect "build-all accepts receipt and reaches bump" 0 'stopped after bump'
    grep -qx bump "$RELEASE_FIXTURE_CALLS" && ok "approved driver invokes bump" || bad "approved driver lost bump"
    work=$TMP/home/pocket-review/wt-relfixture
    git -C "$work" checkout -q --detach "$SHA"
    : > "$RELEASE_FIXTURE_CALLS"
    OUT=$(HOME="$TMP/home" STOP_AFTER=bump FULL_SUITE_RECEIPT="$TMP/fresh.json" \
        bash "$TMP/driver/scripts/build-all.sh" 9.9.9 fixture "$NEXT" 2099-01-01 2>&1); RC=$?
    expect "build-all refuses an existing worktree at a different commit" 2 'PRE-CUT: REFUSED: existing'
    [ ! -s "$RELEASE_FIXTURE_CALLS" ] && ok "wrong existing worktree cannot reach bump" || bad "wrong existing worktree reached bump"
    git -C "$work" checkout -q "release/9.9.9-fixture"
    printf '// dirty\n' >> "$work/src/lib.rs"
    OUT=$(HOME="$TMP/home" STOP_AFTER=bump FULL_SUITE_RECEIPT="$TMP/fresh.json" \
        bash "$TMP/driver/scripts/build-all.sh" 9.9.9 fixture "$NEXT" 2099-01-01 2>&1); RC=$?
    expect "build-all refuses an existing dirty release worktree" 2 'PRE-CUT: REFUSED: existing'
    git -C "$work" checkout -q -- src/lib.rs
    git -C "$work" checkout -qb wrong-branch
    OUT=$(HOME="$TMP/home" STOP_AFTER=bump FULL_SUITE_RECEIPT="$TMP/fresh.json" \
        bash "$TMP/driver/scripts/build-all.sh" 9.9.9 fixture "$NEXT" 2099-01-01 2>&1); RC=$?
    expect "build-all refuses an existing worktree on the wrong branch" 2 'PRE-CUT: REFUSED: existing'
fi

# --- nav explorer evidence: the release check carries pre-release's nav cases only on an exact match ---------------------
N=$TMP/nav
mkdir -p "$N/scripts/image" "$TMP/navbin" "$TMP/navbins"
cp "$ROOT/scripts/nav-evidence.py" "$ROOT/scripts/release-check.sh" "$ROOT/scripts/release-text-tests.txt" "$ROOT/scripts/full-suite.sh" "$N/scripts/"
cp "$F/scripts/gate.sh" "$F/scripts/doc-missing.py" "$F/scripts/bookkeeping.sh" "$N/scripts/"
printf '#!/bin/sh\nexit 0\n' > "$N/scripts/image/storage-budget.sh"
printf 'import sys\nsys.exit(0)\n' > "$N/scripts/volume-device-check.py"
printf 'pub fn f() {}\n' > "$N/lib.rs"; printf 'arm\n' > "$TMP/navbins/pocket-ui"
cat > "$TMP/navbin/cargo" <<'CARGO'
#!/bin/sh
echo "$*" >> "$NAV_CALLS"
case " $* " in *" --list "*) cat "$NAV_LIST"; exit 0 ;; esac
echo 'test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'
CARGO
chmod +x "$TMP/navbin/cargo"
printf 'walk::a: test\nwalk::b: test\n2 tests, 0 benchmarks\n' > "$TMP/nav-list.txt"
printf '   Compiling x\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out\n' > "$TMP/nav.log"
git -C "$N" init -q -b main && git -C "$N" add -A && git -C "$N" commit -qm nav
NC=$(git -C "$N" rev-parse HEAD) NT=$(git -C "$N" rev-parse 'HEAD^{tree}')
export NAV_CALLS="$TMP/nav-calls" NAV_LIST="$TMP/nav-list.txt" RUST_TEST_THREADS=6
ne() { OUT=$(cd "$N" && python3 scripts/nav-evidence.py "$@" 2>&1); RC=$?; }
navreceipt() { # mode steps -> $TMP/pre-nav.txt (the receipt pre-release.sh writes)
    { printf 'kind=pre-release\nresult=ok\nmode=%s\ncommit=%s\ntree=%s\nfull=%s\nsteps=%s\n' "$1" "$NC" "$NT" "$NC" "$2"
      (cd "$N" && python3 scripts/nav-evidence.py record --log "$TMP/nav.log" --list "$TMP/nav-list.txt"); } > "$TMP/pre-nav.txt"; }
navreceipt full 'docs:ok full-suite:ok nav:ok'
grep -qx 'nav_carry=yes' "$TMP/pre-nav.txt" && grep -qx 'nav_tests=2' "$TMP/pre-nav.txt" && ok "nav evidence: a clean green log is recorded" || bad "nav evidence not recorded" "$(cat "$TMP/pre-nav.txt")"
ne check --receipt "$TMP/pre-nav.txt" --list "$TMP/nav-list.txt"; expect "nav evidence: exact match is carried" 0 'carried from pre-release receipt'
RUST_TEST_THREADS=12 ne check --receipt "$TMP/pre-nav.txt" --list "$TMP/nav-list.txt"; expect "nav evidence: other test threads run the cases" 1 'runs here: nav_threads='
printf 'walk::a: test\nwalk::b: test\nwalk::c: test\n' > "$TMP/nav-list2.txt"
ne check --receipt "$TMP/pre-nav.txt" --list "$TMP/nav-list2.txt"; expect "nav evidence: a changed case list runs the cases" 1 'runs here: nav_list_sha256='
ne check --receipt "$TMP/missing.txt" --list "$TMP/nav-list.txt"; expect "nav evidence: a missing receipt runs the cases" 1 'runs here'
navreceipt text-only 'docs:ok nav:ok'
ne check --receipt "$TMP/pre-nav.txt" --list "$TMP/nav-list.txt"; expect "nav evidence: a text-only pre receipt runs the cases" 1 "runs here: mode='text-only'"
navreceipt full 'docs:ok full-suite:ok nav:skipped'
ne check --receipt "$TMP/pre-nav.txt" --list "$TMP/nav-list.txt"; expect "nav evidence: no green nav step runs the cases" 1 'runs here: the receipt has no green nav step'
navreceipt full 'docs:ok full-suite:ok nav:ok'
cp "$TMP/pre-nav.txt" "$TMP/pre-nav-good.txt"
printf '\n' >> "$TMP/nav.log"
ne check --receipt "$TMP/pre-nav.txt" --list "$TMP/nav-list.txt"; expect "nav evidence: a changed log runs the cases" 1 'runs here: the nav log changed'
printf '   Compiling x\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out\n' > "$TMP/nav.log"
: > "$TMP/nav.log.retry"
ne check --receipt "$TMP/pre-nav.txt" --list "$TMP/nav-list.txt"; expect "nav evidence: a retried case runs the cases" 1 'runs here: a case was retried'
ne record --log "$TMP/nav.log" --list "$TMP/nav-list.txt"; expect "nav evidence: a retried case is never recorded" 0 '^nav_carry=no \(a case was retried'
rm "$TMP/nav.log.retry"
ne record --log "$TMP/nav.log" --list "$TMP/nav-list2.txt"; expect "nav evidence: passed != listed is never recorded" 0 '^nav_carry=no \(2 passed but 3 listed'
printf 'test result: FAILED. 1 passed; 1 failed;\n' > "$TMP/nav-red.log"
ne record --log "$TMP/nav-red.log" --list "$TMP/nav-list.txt"; expect "nav evidence: a red log is never recorded" 0 '^nav_carry=no'
ne verify --receipt "$TMP/pre-nav.txt" --tree "$NT" --commit "$NC" --list-sha "$(sed -n 's/^nav_list_sha256=//p' "$TMP/pre-nav.txt")"
expect "nav evidence: 8-publish verify accepts the carried receipt" 0 '^ok carried nav explorer evidence'
ne verify --receipt "$TMP/pre-nav.txt" --tree "$NC" --commit "$NC" --list-sha x; expect "nav evidence: 8-publish verify refuses another tree" 1 '^FAIL carried nav explorer evidence: tree='
# the real release check (stand-in cargo/gate/STORAGE/volume): carried = no nav run; anything else = the nav run
nrc() { : > "$NAV_CALLS"; OUT=$(cd "$N" && PATH="$TMP/navbin:$PATH" RELEASE_FIXTURE_CALLS="$TMP/nav-gate" POCKET_STORAGE_BIN_DIR="$TMP/navbins" \
    POCKET_VOLUME_CHECK_EVIDENCE="$TMP/nav.log" RELEASE_CHECK_RECEIPT="$TMP/rc-nav.txt" "$@" sh scripts/release-check.sh 2>&1); RC=$?; }
navran() { grep -v -- '--list' "$NAV_CALLS" | grep -q 'nav_explorer'; }
nrc env RELEASE_CHECK_PRE_RECEIPT="$TMP/pre-nav.txt"
expect "release check: carries the nav cases from an exact pre receipt" 0 'release check: nav explorer \(--ignored\): carried'
! navran && grep -q '^nav=carried ' "$TMP/rc-nav.txt" && ok "release check: carried nav cases do not run and the receipt says so" || bad "carried nav still ran" "$(cat "$NAV_CALLS")"
nrc env
expect "release check: without a pre receipt it runs as before" 0 'RELEASE CHECK: ok|release check: receipt'
navran && grep -q '^nav=ran' "$TMP/rc-nav.txt" && ok "release check: no pre receipt runs the nav cases" || bad "nav cases did not run" "$(cat "$NAV_CALLS")"
printf 'walk::a: test\n' > "$TMP/nav-list-short.txt"
nrc env RELEASE_CHECK_PRE_RECEIPT="$TMP/pre-nav.txt" NAV_LIST="$TMP/nav-list-short.txt"
expect "release check: a different case list is named" 0 'release check: nav explorer \(--ignored\) runs here: nav_list_sha256='
navran && ok "release check: a different case list runs the nav cases" || bad "nav cases did not run on a different list" "$(cat "$NAV_CALLS")"
git -C "$N" commit -q --allow-empty -m later
nrc env RELEASE_CHECK_PRE_RECEIPT="$TMP/pre-nav.txt"
expect "release check: another commit is named" 0 "runs here: commit="
navran && ok "release check: another commit runs the nav cases" || bad "nav cases did not run on another commit" "$(cat "$NAV_CALLS")"

echo "test-release-fast: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
