#!/bin/sh
# Test of the H700 image inputs that need no Docker and no download (scripts/make-image-h700.sh, pins.sh H700 block).
# Part of the gate (scripts/gate.sh).
set -u
cd "$(dirname "$0")/../.." || exit 1
. scripts/image/pins.sh
fail=0
ok() { echo "ok: $1"; }
bad() { echo "FAIL: $1" >&2; fail=1; }
hex() { printf '%s' "$1" | grep -Eq "^[0-9a-f]{$2}\$"; }

hex "$H700_GZ_SHA256" 64 && hex "$H700_IMG_SHA256" 64 && hex "$H700_KERNEL_MD5" 32 && hex "$H700_SYSTEM_MD5" 32 && ok "image, KERNEL and SYSTEM pins are hashes" || bad "a pin is not a hash"
case $H700_POCKET_SYSTEM_MD5 in unset) ok "H700_POCKET_SYSTEM_MD5 is unset (first build)" ;; *) hex "$H700_POCKET_SYSTEM_MD5" 32 && ok "H700_POCKET_SYSTEM_MD5 is a hash" || bad "H700_POCKET_SYSTEM_MD5" ;; esac
n=0
names=
echo "$H700_MODELS" | while read -r name dtb md5 model; do
  [ -n "$name" ] || continue
  [ -n "$dtb" ] && hex "$md5" 32 && [ -n "$model" ] || { echo "FAIL: bad model line '$name $dtb $md5 $model'" >&2; exit 1; }
done && ok "every H700_MODELS line has name, dtb, md5 and model" || bad "H700_MODELS"
dup=$(echo "$H700_MODELS" | awk 'NF { print $1 }' | sort | uniq -d)
[ -z "$dup" ] && ok "image names are unique" || bad "duplicate image name: $dup"
dupd=$(echo "$H700_MODELS" | awk 'NF { print $2 }' | sort | uniq -d)
[ -z "$dupd" ] && ok "each model has its own device tree" || bad "two models share a tree: $dupd"
[ "$(echo "$H700_MODELS" | awk 'NF { print $1 }' | tr '\n' ' ')" = "rg35xx-plus rg35xx-sp rg28xx " ] && ok "the three models with a unit to test: RG35XX Plus, SP, RG28XX" || bad "model list changed"
# Placeholders in the notes must all be ones scripts/make-image-h700.sh fills in.
known=$(grep -o 's|@[A-Z_]*@|' scripts/make-image-h700.sh | sed 's/^s|//; s/|$//' | sort -u)
pf=0
for t in README SOURCE-OFFER; do
  for ph in $(grep -o '@[A-Z_]*@' "deploy/image/$t-h700.txt.in" | sort -u); do
    echo "$known" | grep -qx "$ph" || { bad "$t-h700.txt.in: $ph is not filled in by make-image-h700.sh"; pf=1; }
  done
done
[ $pf = 0 ] && ok "placeholders of the H700 notes are all filled in (checked against make-image-h700.sh)"
# The removal lists name only paths that exist in the H700 SYSTEM (not available here) and none from the SM8250 list
# that cannot be there (rocknix_abl, AYN/Mangmi firmware).
echo "$H700_SYSTEM_REMOVE" | grep -Eq 'rocknix_abl|/ayn|/mangmi' && bad "H700_SYSTEM_REMOVE names SM8250-only paths" || ok "H700_SYSTEM_REMOVE has no SM8250-only paths"
# Firmware list: the 15 files of the H700 SYSTEM, each with an md5.
c=$(grep -Ec '^ +[0-9a-f]{32} +[a-z_0-9,]+/?[A-Za-z0-9_,./-]*' deploy/image/firmware-licences-h700/README.txt)
[ "$c" -ge 15 ] && ok "firmware-licences-h700/README.txt lists $c files with md5" || bad "firmware list has only $c md5 lines"
[ -s deploy/image/firmware-licences/LICENCE.rtlwifi_firmware.txt ] && [ -s deploy/image/firmware-licences/wireless-regdb-LICENSE ] && ok "the two shared licence texts exist" || bad "shared licence texts missing"
exit $fail
