#!/bin/sh
# Test of the H700 and SM8550 image inputs that need no Docker and no download (scripts/make-image-family.sh, pins.sh).
# Part of the gate (scripts/gate.sh).
set -u
cd "$(dirname "$0")/../.." || exit 1
. scripts/image/pins.sh
ROOT=$PWD
. scripts/image/tools.sh
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
# The GAMES drive goes by model, and only where there's no Wi-Fi chip: the notes of all others don't name it.
has_games_drive H700 rg28xx && ! has_games_drive H700 rg35xx-plus && ! has_games_drive H700 rg35xx-sp && ! has_games_drive H700 "" && ! has_games_drive SM8250 rg28xx \
  && [ -z "$SM8550_GAMES_DRIVE_MODELS" ] && ok "GAMES drive on the RG28XX only" || bad "GAMES drive models: $H700_GAMES_DRIVE_MODELS / $SM8550_GAMES_DRIVE_MODELS"
notes_blocks 1 <deploy/image/README-h700.txt.in | grep -q "drive called GAMES" && ok "the RG28XX's notes explain the drive" || bad "the RG28XX's notes don't name GAMES"
! notes_blocks 0 <deploy/image/README-h700.txt.in | grep -Eq "GAMES|^#(if|else|endif)" && notes_blocks 0 <deploy/image/README-h700.txt.in | grep -q "file sharing on that page" \
  && ok "the notes of a Wi-Fi model name file sharing, not GAMES" || bad "the notes of a Wi-Fi model name GAMES"
! grep -q GAMES deploy/image/README-sm8550.txt.in && ok "the SM8550 notes don't name GAMES" || bad "the SM8550 notes name GAMES"
# Placeholders in the notes must all be ones scripts/make-image-h700.sh fills in.
known=$(grep -o 's|@[A-Z_]*@|' scripts/make-image-family.sh | sed 's/^s|//; s/|$//' | sort -u)
pf=0
for t in README SOURCE-OFFER; do
  for ph in $(grep -o '@[A-Z_]*@' "deploy/image/$t-h700.txt.in" | sort -u); do
    echo "$known" | grep -qx "$ph" || { bad "$t-h700.txt.in: $ph is not filled in by make-image-family.sh"; pf=1; }
  done
done
[ $pf = 0 ] && ok "placeholders of the H700 notes are all filled in (checked against make-image-family.sh)"
# The removal lists name only paths that exist in the H700 SYSTEM (not available here) and none from the SM8250 list
# that cannot be there (rocknix_abl, AYN/Mangmi firmware).
echo "$H700_SYSTEM_REMOVE" | grep -Eq 'rocknix_abl|/ayn|/mangmi' && bad "H700_SYSTEM_REMOVE names SM8250-only paths" || ok "H700_SYSTEM_REMOVE has no SM8250-only paths"
# Firmware list: the 15 files of the H700 SYSTEM, each with an md5.
c=$(grep -Ec '^ +[0-9a-f]{32} +[a-z_0-9,]+/?[A-Za-z0-9_,./-]*' deploy/image/firmware-licences-h700/README.txt)
[ "$c" -ge 15 ] && ok "firmware-licences-h700/README.txt lists $c files with md5" || bad "firmware list has only $c md5 lines"
[ -s deploy/image/firmware-licences/LICENCE.rtlwifi_firmware.txt ] && [ -s deploy/image/firmware-licences/wireless-regdb-LICENSE ] && ok "the two shared licence texts exist" || bad "shared licence texts missing"

# SM8550: the same pins, in the same shape; one more family must not need its own script.
hex "$SM8550_GZ_SHA256" 64 && hex "$SM8550_IMG_SHA256" 64 && hex "$SM8550_KERNEL_MD5" 32 && hex "$SM8550_SYSTEM_MD5" 32 && ok "SM8550 image, KERNEL and SYSTEM pins are hashes" || bad "an SM8550 pin is not a hash"
hex "$SM8550_POCKET_SYSTEM_MD5" 32 && ok "SM8550_POCKET_SYSTEM_MD5 is a hash" || bad "SM8550_POCKET_SYSTEM_MD5"
[ "$(echo "$SM8550_MODELS" | awk 'NF { print $1 }' | tr '\n' ' ')" = "odin2-mini odin2 odin2-portal rp6 nova " ] && ok "SM8550 models as listed" || bad "SM8550 model list changed"
echo "$SM8550_MODELS" | awk 'NF && NF != 4 { exit 1 }' && ok "every SM8550_MODELS line has four fields" || bad "SM8550_MODELS"
# The card must never be able to flash a bootloader: ROCKNIX's update.sh runs updateabl, which needs this folder.
echo "$SM8550_SYSTEM_REMOVE" | grep -qx 'usr/share/bootloader/rocknix_abl' && ok "SM8550 SYSTEM goes without rocknix_abl" || bad "rocknix_abl stays in the SM8550 SYSTEM"
# The firmware the five models' trees name must not be in the removal list (sound and battery depend on it).
echo "$SM8550_SYSTEM_REMOVE" | grep -Eq 'ayn/(odin2|odin2mini|odin2portal|cdsp)' && bad "SM8550_SYSTEM_REMOVE takes a listed model's firmware" || ok "SM8550_SYSTEM_REMOVE keeps the listed models' firmware"
for t in README SOURCE-OFFER; do
  for ph in $(grep -o '@[A-Z_]*@' "deploy/image/$t-sm8550.txt.in" | sort -u); do
    echo "$known" | grep -qx "$ph" || bad "$t-sm8550.txt.in: $ph is not filled in by make-image-family.sh"
  done
done
family_pins SM8550
[ "$FAM_IMG" = "$SM8550_IMG" ] && [ -n "$FAM_FW_SHARED" ] && ok "family_pins gives the family's pins" || bad "family_pins"
for f in $FAM_FW_SHARED; do [ -s "deploy/image/firmware-licences/$f" ] || bad "shared licence text missing: $f"; done
sh scripts/make-image-family.sh --family AMD64 >/dev/null 2>&1 && bad "an unknown family was accepted" || ok "an unknown family is refused"
# Every card gives its handheld its own name on the network (system.hostname): lowercase ASCII, at most 24 characters, never
# ROCKNIX's default, different on each model, and image-defaults.sh has the one NAME line the build and the checks replace.
hosts=$(for m in "" $(echo "$H700_MODELS $SM8550_MODELS" | awk 'NF { print $1 }'); do image_hostname "$m"; done)
bad_host=$(echo "$hosts" | grep -Ev '^[a-z][a-z0-9-]{0,23}$|^$')
[ -z "$bad_host" ] && ! echo "$hosts" | grep -qix 'rocknix\|sm8250\|sm8550\|h700' && ok "every image has a valid hostname of its own kind" || bad "hostname not valid: $bad_host"
[ -z "$(echo "$hosts" | sort | uniq -d)" ] && [ "$(image_hostname "")" = pocket-mini-v2 ] && [ "$(image_hostname rg35xx-plus)" = pocket-rg35xx-plus ] \
  && ok "hostnames are unique: pocket-mini-v2, pocket-rg35xx-plus, ..." || bad "hostnames clash or changed: $(echo $hosts)"
[ "$(grep -c '^NAME=pocket-ui$' deploy/image/image-defaults.sh)" = 1 ] && grep -q 'set_setting system.hostname "\$NAME"' deploy/image/image-defaults.sh \
  && ok "image-defaults.sh has one NAME line and sets system.hostname from it" || bad "image-defaults.sh: NAME line"
sed "s/^NAME=pocket-ui\$/NAME=$(image_hostname rg28xx)/" deploy/image/image-defaults.sh | grep -qx 'NAME=pocket-rg28xx' && ok "the build's sed puts the model's name on the NAME line" || bad "sed does not set NAME"
exit $fail
