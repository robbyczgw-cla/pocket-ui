#!/bin/sh
# Fixture test for scripts/bookkeeping.sh's warning lines (21.11.2026: the 0.6.0 warning cut the code-shape test after
# 5 lines and showed two of its four counts over their caps).  sh scripts/test-bookkeeping.sh
set -u
S=$(cd "$(dirname "$0")" && pwd); T=$(mktemp -d); trap 'rm -rf "$T"' EXIT; fail=0
cat > "$T/log" <<'LOG'
---- doc_checks::the_code_shape_counts_never_go_up stdout ----
code shape: pub fn 6324 in src/ (product code; shown, not capped)
thread 'doc_checks::the_code_shape_counts_never_go_up' (1) panicked at src/doc_checks.rs:1416:5:
a code-shape count went up (docs/DEVELOPING.md, "Code shape"); use a type, return the error or keep the item private instead:
== "pocket-core": 13 in src/ (product code), the cap is 11
== "ports": 15 in src/ (product code), the cap is 14
let _ = fs::: 275 in src/ (product code), the cap is 235
Result<_, String>: 468 in src/ (product code), the cap is 443

---- doc_checks::every_language_has_every_line stdout ----
LOG
i=1; while [ $i -le 30 ]; do echo "missing line $i" >> "$T/log"; i=$((i+1)); done
printf '\nfailures:\n    doc_checks::the_code_shape_counts_never_go_up\n' >> "$T/log"
out=$(sh "$S/bookkeeping.sh" --failure-lines "$T/log" doc_checks::the_code_shape_counts_never_go_up)
for want in '"pocket-core": 13' '"ports": 15' 'let _ = fs::: 275' 'Result<_, String>: 468'; do
    case $out in *"$want"*) echo "ok   shows $want" ;; *) echo "FAIL the warning leaves out $want"; fail=1 ;; esac
done
case $out in *every_language*) echo "FAIL the next test's lines leak in"; fail=1 ;; *) echo "ok   stops at the next test" ;; esac
out=$(sh "$S/bookkeeping.sh" --failure-lines "$T/log" doc_checks::every_language_has_every_line)
[ "$(printf '%s\n' "$out" | grep -c 'missing line')" = 20 ] && echo "ok   a long failure: 20 lines" || { echo "FAIL a long failure is not cut at 20"; fail=1; }
case $out in *"... 10 more lines"*) echo "ok   says how many more" ;; *) echo "FAIL no '... 10 more lines'"; fail=1 ;; esac
[ $fail = 0 ] && echo "test-bookkeeping: ok" || { echo "test-bookkeeping: FAILED"; exit 1; }
