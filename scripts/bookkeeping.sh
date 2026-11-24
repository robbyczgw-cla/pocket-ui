#!/bin/sh
# Bookkeeping (friction audit, 14.11.2026): checks no player notices, so they never block a merge. They are
# #[ignore = "release check: bookkeeping ..."] tests; the nightly run on main and scripts/release-check.sh print the red
# ones as warnings, and one commit fixes them in bulk:  sh scripts/bookkeeping.sh [tree]
#   the code-shape caps, every environment variable and source module documented, docs/README.md lists every document,
#   the manual's pages not too long for the screen (the Brick's too), and every language has every line (es/pt gaps;
#   the release check fails on that one itself).
# Prints "bookkeeping: WARNING <test>" with its first failure lines per red test, then "BOOKKEEPING: N warnings (M tests run)".
# Exit 0 (with warnings too); 2 bad usage, the tests did not build or none ran.
set -u
# failure_lines LOG TEST: the failure message of TEST in a cargo test log, up to 20 lines, then how many more there are.
# 20, not 5 (21.11.): the code-shape test prints one line per count over its cap after three lines of its own, so the
# 0.6.0 warning showed two of four counts (`let _ = fs::` and `Result<_, String>` were over as well).
failure_lines() {
    awk -v t="---- $2 stdout ----" '$0 == t { on = 1; next } on && /^(---- |failures:$)/ { exit } on && NF { n++; if (n <= 20) print; }
        END { if (n > 20) print "... " n - 20 " more lines" }' "$1" | sed 's/^/    /'
}
# Only the printing of a failure, for scripts/test-bookkeeping.sh: --failure-lines LOG TEST.
[ "${1:-}" = --failure-lines ] && { failure_lines "$2" "$3"; exit 0; }
case ${1:-} in -*) echo "usage: sh scripts/bookkeeping.sh [tree]" >&2; exit 2 ;; esac
cd "${1:-$(dirname "$0")/..}" || exit 2
export PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
log=$(mktemp "${TMPDIR:-/tmp}/bookkeeping.XXXXXX") || exit 2
trap 'rm -f "$log"' EXIT
warnings=0 ran=0
# check LABEL "TESTS" CARGO-ARGS...: one cargo run of these ignored tests, each red one a warning.
check() {
    label=$1 tests=$2; shift 2
    # shellcheck disable=SC2086
    cargo test --profile "${GATE_PROFILE:-gate}" --no-fail-fast "$@" -- --include-ignored $tests >"$log" 2>&1
    grep -q '^test result:' "$log" || { tail -20 "$log"; echo "BOOKKEEPING: the tests did not build ($label)"; exit 2; }
    ran=$((ran + $(awk '/^test result:/ { n += $4 + $6 } END { print n + 0 }' "$log")))
    for t in $(sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' "$log"); do
        warnings=$((warnings + 1))
        echo "bookkeeping: WARNING $t$label"
        failure_lines "$log" "$t"
    done
}
check "" "the_code_shape_counts_never_go_up every_environment_variable_is_documented developing_md_names_every_source_module \
the_documentation_index_lists_every_document manual_pages_are_not_too_long_for_the_screen every_language_has_every_line" --lib --bins
# The Brick build has manual pages and translation lines of its own.
if grep -Eq '^[[:space:]]*brick[[:space:]]*=' Cargo.toml; then
    check " (Brick)" "manual_pages_are_not_too_long_for_the_screen every_language_has_every_line" --features brick --lib
fi
if [ "$warnings" -gt 0 ]; then
    echo "bookkeeping: documentation gaps: python3 scripts/doc-missing.py prints the missing entries; a code-shape count: bring it back under its cap"
    echo "bookkeeping: missing es/pt/fr/it lines: fill them in one commit (docs/dev/translations.md); a long manual page: split it"
fi
# A renamed test must not pass silently: none ran is a broken check, not a clean tree.
[ "$ran" -gt 0 ] || { echo "BOOKKEEPING: no bookkeeping test ran (renamed?)"; exit 2; }
echo "BOOKKEEPING: $warnings warnings ($ran tests run)"
