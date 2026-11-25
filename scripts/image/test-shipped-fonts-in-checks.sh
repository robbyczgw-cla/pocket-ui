#!/bin/sh
# What scripts/image/build.sh puts on a card from scripts/shipped-fonts.sh, the card checks must expect too (25.11.2026: the
# 0.7.1 cards failed "exactly the expected files" on releases/<V>/fonts/, which build.sh had added and the checks did not know).
#   sh scripts/image/test-shipped-fonts-in-checks.sh
cd "$(dirname "$0")/../.." || exit 2
bad=0
grep -q 'scripts/shipped-fonts.sh' scripts/image/build.sh || { echo "ok   build.sh ships no font files"; exit 0; }
for c in scripts/image/check.sh scripts/image/check-h700.sh; do
  if grep -q 'shipped-fonts.sh' "$c" && grep -q 'releases/$V/fonts' "$c"; then echo "ok   $c expects the shipped fonts"
  else echo "FAIL $c does not expect releases/<V>/fonts/ (scripts/shipped-fonts.sh), but build.sh puts it on the card"; bad=1; fi
done
exit $bad
