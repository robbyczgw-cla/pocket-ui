#!/bin/sh
# Test of scripts/image/rocknix-source.sh's family parameter (--dry-run: nothing is cloned or fetched).
# Part of the gate (scripts/gate.sh).
set -u
cd "$(dirname "$0")/../.." || exit 1
S=scripts/image/rocknix-source.sh
fail=0
ok() { echo "ok: $1"; }
bad() { echo "FAIL: $1" >&2; fail=1; }
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

o=$(sh $S "$T/o" --dry-run) && echo "$o" | grep -q '^device=SM8250 ' && echo "$o" | grep -q 'rocknix-20261001-SM8250-sources.tar.xz.part-NN' &&
  ok "default family is SM8250, file names unchanged" || bad "default family: $o"
o=$(sh $S "$T/o" --device H700 --dry-run) && echo "$o" | grep -q '^device=H700 ' && echo "$o" | grep -q 'rocknix-20261001-H700-sources.tar.xz.part-NN' &&
  ok "--device H700" || bad "--device H700: $o"
o=$(sh $S "$T/o" --dry-run --device H700) && echo "$o" | grep -q '^device=H700 ' && ok "option order does not matter" || bad "option order: $o"
sh $S "$T/o" --device AMD64 --dry-run >/dev/null 2>&1 && bad "an unsupported family was accepted" || ok "unsupported family refused"
sh $S "$T/o" --device >/dev/null 2>&1 && bad "--device without a value was accepted" || ok "--device needs a value"
sh $S "$T/o" --frobnicate >/dev/null 2>&1 && bad "unknown option accepted" || ok "unknown option refused"
[ ! -e "$T/o" ] && ok "dry run creates nothing" || bad "dry run created $T/o"
# The script must not name SM8250 as the device any more except as default / in comments.
! grep -n 'DEVICE=SM8250' $S && ok "no hard-coded DEVICE=SM8250" || bad "DEVICE=SM8250 still hard-coded"
# The H700's U-Boot, TF-A and suspend stub are on the card but not in the image build plan: the script adds them.
grep -q "u-boot-DDR4 atf h700-suspend-stub" $S && ok "H700 extras (U-Boot, atf, suspend stub) are fetched" || bad "H700 extras missing"
exit $fail
