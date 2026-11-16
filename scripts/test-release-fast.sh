#!/bin/bash
# Test for the text-only release path (docs/dev/release.md, "Text-only fixes"): scripts/text-only-diff.py, and the gates of
# scripts/pre-release.sh --since, scripts/export-public.sh --text-only-since and RELEASE_CHECK_TEXT_ONLY in
# scripts/release-check.sh. Throw-away git repositories only; cargo never runs (every case stops before it or is refused).
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

echo "test-release-fast: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
