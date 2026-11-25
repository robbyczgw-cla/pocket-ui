#!/bin/sh
# Every fixed sentence the card checks look for in a card's notes (check.sh, check-h700.sh: grep -q "TEXT" "$R") must be in at
# least one notes template (deploy/image/README*.txt.in). 24.11.2026: a docs pass rewrote "Nothing needs flashing" in
# README-sm8550.txt.in and the 0.7 release's RP6 and Nova cards failed their check; only a real image build showed it.
#   sh scripts/image/test-check-phrases.sh        exit 1 names each sentence no template carries
cd "$(dirname "$0")/../.." || exit 2
bad=0; n=0
for c in scripts/image/check.sh scripts/image/check-h700.sh; do
  # the plain sentences only: no shell variables, no patterns (grep -E / -F lines and $ or \ are skipped)
  grep -o 'grep -q "[^"$\\]*" "\$R"' "$c" | sed 's/^grep -q "//; s/" "\$R"$//' | while IFS= read -r s; do printf '%s\t%s\n' "$c" "$s"; done
done > /tmp/check-phrases.$$
while IFS="$(printf '\t')" read -r c s; do
  n=$((n+1))
  if ! grep -qF -- "$s" deploy/image/README*.txt.in; then echo "FAIL $c looks for \"$s\" but no deploy/image/README*.txt.in says it"; bad=1; fi
done < /tmp/check-phrases.$$
rm -f /tmp/check-phrases.$$
[ $bad = 0 ] && echo "ok   every sentence the card checks look for is in a notes template" || exit 1
