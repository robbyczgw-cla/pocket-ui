#!/bin/sh
# (Lane check 2, 13.11.: a comment only.)
# scripts/preflight.sh in an isolated git repository with stand-in compile, leak, audit and cargo steps. No builds.
# Proves: rebase on main, the step order (leaks, compile, audit, cheap tests), that every failure stops the later steps and says
# which one failed, that an empty cheap check fails, and that --cheap-tests (what the merge gate calls) skips the first steps.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
T=$(mktemp -d "${TMPDIR:-/tmp}/preflight-test.XXXXXX")
trap 'rm -rf "$T"' EXIT HUP INT TERM
H=$T/home; W=$T/repo
mkdir -p "$H/.cargo/bin" "$W/scripts"
export HOME=$H GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null TRACE=$T/trace
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid
unset ANDROID_HOME PREFLIGHT_NO_REBASE GATE_PROFILE CARGO_BUILD_JOBS
cp "$ROOT/scripts/preflight.sh" "$W/scripts/preflight.sh"
cat >"$H/.cargo/bin/cargo" <<'CARGO'
#!/bin/sh
printf 'cargo %s\n' "$*" >>"$TRACE"
for f in doc_checks:: document_check:: manual:: i18n::; do
    case " $* " in *" $f "*)
        [ ! -e "$FIXTURE/red-$f" ] || { echo "test $f ... FAILED"; exit 101; }
        [ ! -e "$FIXTURE/empty-$f" ] || { echo 'test result: ok. 0 passed'; exit 0; } ;;
    esac
done
echo 'test result: ok. 3 passed'
CARGO
cat >"$W/scripts/check-leaks.sh" <<'S'
#!/bin/sh
echo leaks >>"$TRACE"; [ ! -e "$FIXTURE/red-leaks" ] || { echo 'FINDING	owner	a.txt:1'; exit 1; }
S
cat >"$W/scripts/check-compile.sh" <<'S'
#!/bin/sh
echo compile >>"$TRACE"; [ ! -e "$FIXTURE/red-compile" ] || { echo 'error[E0599]: example'; exit 1; }
echo 'compile-first: ok (1 s)'; [ ! -e "$FIXTURE/skipped" ] || echo 'check-platforms: android SKIPPED (no Android NDK here)'
S
cat >"$W/scripts/i18n-extract.py" <<'S'
import os, sys
open(os.environ["TRACE"], "a").write("audit %s\n" % " ".join(sys.argv[1:]))
if os.path.exists(os.environ["FIXTURE"] + "/red-audit"):
    print("MISSING German line: New text"); sys.exit(1)
if os.path.exists(os.environ["FIXTURE"] + "/es-gap"):
    print("i18n: WARNING es.tsv lacks 1 lines de.tsv has")
S
chmod +x "$H/.cargo/bin/cargo" "$W/scripts/check-leaks.sh" "$W/scripts/check-compile.sh"
export FIXTURE=$T/flags; mkdir -p "$FIXTURE"
git -C "$W" init -q -b main
echo base >"$W/shared"; echo one >"$W/changelog"
git -C "$W" add -A; git -C "$W" commit -qm base
git -C "$W" checkout -q -b topic
echo topic >"$W/topic"; git -C "$W" add -A; git -C "$W" commit -qm topic
git -C "$W" checkout -q main
echo main >"$W/main-file"; git -C "$W" add -A; git -C "$W" commit -qm "main moved"
git -C "$W" checkout -q topic
pass=0
run() { : >"$TRACE"; rc=0; ( cd "$W" && sh scripts/preflight.sh "$@" ) >"$T/out" 2>&1 || rc=$?; }
ok() { pass=$((pass + 1)); echo "ok $1"; }
bad() { echo "FAIL $1"; cat "$T/out"; echo "--- trace"; cat "$TRACE"; exit 1; }
want_rc() { [ "$rc" = "$1" ] || bad "$2 (exit $rc, wanted $1)"; }
has() { grep -q -- "$1" "$T/out" || bad "$2 (output lacks: $1)"; }
traced() { grep -q -- "$1" "$TRACE" || bad "$2 (trace lacks: $1)"; }
untraced() { ! grep -q -- "$1" "$TRACE" || bad "$2 (trace has: $1)"; }

run
want_rc 0 'clean run'; has 'PREFLIGHT: ok' 'clean run'
git -C "$W" merge-base --is-ancestor main HEAD || bad 'the branch was not rebased onto main'
[ -f "$W/main-file" ] || bad 'main file missing after rebase'
# order: leaks, compile, audit, then the four cheap test filters
order=$(sed -e 's/^cargo test .* \(doc_checks\|document_check\|manual\|i18n\)::.*/cheap-\1/' -e 's/^audit.*/audit/' "$TRACE" | tr '\n' ' ')
[ "$order" = 'leaks compile audit cheap-doc_checks cheap-document_check cheap-manual cheap-i18n ' ] || bad "order was: $order"
traced 'audit --check' 'audit uses --check'; traced 'cargo test --profile gate --lib --bins' 'gate profile, lib and bins'
ok 'rebase, order and profile'

echo dirty >>"$W/topic"
run; want_rc 1 'dirty tree'; has 'rebase FAILED' 'dirty tree'; has 'uncommitted' 'dirty tree'
[ ! -s "$TRACE" ] || bad 'a dirty tree must stop before every other step'
git -C "$W" checkout -q -- topic
ok 'a dirty tree stops first'

git -C "$W" checkout -q main; echo conflict-main >"$W/shared"; git -C "$W" commit -qam "main shared"
git -C "$W" checkout -q -b clash "$(git -C "$W" rev-list --max-parents=0 main)"
echo conflict-topic >"$W/shared"; git -C "$W" commit -qam "topic shared"
run; want_rc 1 'rebase conflict'; has 'rebase FAILED' 'conflict'; has 'shared' 'conflicts name the file'
[ ! -e "$W/.git/rebase-merge" ] && [ ! -e "$W/.git/rebase-apply" ] || bad 'a failed rebase must be aborted'
[ ! -s "$TRACE" ] || bad 'a conflict must stop before leaks and compile'
git -C "$W" checkout -q topic
ok 'a rebase conflict is aborted and named'

PREFLIGHT_NO_REBASE=1 run; want_rc 0 'no rebase'; has 'rebase skipped' 'no rebase'
ok 'PREFLIGHT_NO_REBASE skips the rebase'

touch "$FIXTURE/red-leaks"; run; want_rc 1 'leak'; has 'leak scan' 'leak'; untraced compile 'leak red stops before compile'; rm "$FIXTURE/red-leaks"
ok 'a leak stops before the compile'

touch "$FIXTURE/red-compile"; run; want_rc 1 'compile'; has 'compile (scripts/check-compile.sh) FAILED' 'compile'; has 'E0599' 'compile prints the error'
untraced audit 'compile red stops before the audit'; untraced 'cargo test' 'compile red stops before the tests'; rm "$FIXTURE/red-compile"
ok 'a compile error stops before the tests'

touch "$FIXTURE/red-audit"; run; want_rc 1 'audit'; has 'translation audit' 'audit'; has 'de.tsv; es.tsv, pt.tsv, fr.tsv and it.tsv are filled before a release' 'audit hint'; untraced 'cargo test' 'audit red stops before the tests'; rm "$FIXTURE/red-audit"
ok 'a missing translation stops with a hint'

touch "$FIXTURE/es-gap"; run; want_rc 0 'es gap'; has 'WARNING es.tsv lacks' 'es gap warning shown'; rm "$FIXTURE/es-gap"
ok 'a missing Spanish line only warns'

for f in doc_checks:: document_check:: manual:: i18n::; do
    touch "$FIXTURE/red-$f"; run; want_rc 1 "red $f"; has "$f" "red $f named"; rm "$FIXTURE/red-$f"
    touch "$FIXTURE/empty-$f"; run; want_rc 1 "empty $f"; has "empty check" "empty $f"; rm "$FIXTURE/empty-$f"
done
ok 'each cheap test fails when red or when it matches nothing'

touch "$FIXTURE/skipped"; run; want_rc 0 'skipped platform'; has 'WARNING a platform was not compiled' 'skipped warning'; rm "$FIXTURE/skipped"
ok 'a skipped platform is a visible warning'

run --cheap-tests; want_rc 0 'cheap only'
untraced leaks 'cheap-only skips leaks'; untraced compile 'cheap-only skips compile'; traced 'audit --check' 'cheap-only runs the audit'
[ "$(git -C "$W" rev-parse HEAD)" = "$(git -C "$W" rev-parse topic)" ] || bad 'cheap-only must not rebase'
ok 'the gate mode runs only the cheap list'

# the two halves: git steps where the repository is, build steps from a copy without .git (another machine)
run --git-only; want_rc 0 'git only'; has 'git part ok' 'git only'
traced leaks 'git-only runs the leak scan'; untraced compile 'git-only stops before the compile'; untraced 'cargo test' 'git-only runs no tests'
ok '--git-only: rebase and leak scan only'
cp -r "$W" "$T/copy"; rm -rf "$T/copy/.git"
: >"$TRACE"; rc=0; ( cd "$T/copy" && sh scripts/preflight.sh --build-only ) >"$T/out" 2>&1 || rc=$?
want_rc 0 'build only without .git'; has 'PREFLIGHT: ok' 'build only'
untraced leaks 'build-only skips the leak scan'; traced compile 'build-only compiles'; traced 'audit --check' 'build-only runs the audit'
ok '--build-only works from a copy without .git'
touch "$FIXTURE/red-compile"; : >"$TRACE"; rc=0; ( cd "$T/copy" && sh scripts/preflight.sh --build-only ) >"$T/out" 2>&1 || rc=$?
want_rc 1 'build only compile red'; has 'compile (scripts/check-compile.sh) FAILED' 'build-only red'; rm "$FIXTURE/red-compile"
ok '--build-only fails on a compile error'

run --bogus; want_rc 2 'usage'
echo "preflight tests: $((pass + 1)) groups passed"
