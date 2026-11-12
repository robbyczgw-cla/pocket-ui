#!/bin/sh
# Test of the H700, SM8550 and RK3566 image inputs that need no Docker and no download (scripts/make-image-family.sh, pins.sh).
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

hex "$H700_DDR3_GZ_SHA256" 64 && hex "$H700_DDR3_IMG_SHA256" 64 && hex "$H700_LOADER_MD5" 32 && hex "$H700_DDR3_LOADER_MD5" 32 && hex "$H700_DDR3_EXTLINUX_MD5" 32 && ok "DDR3 image, loader and extlinux pins are hashes" || bad "a DDR3 pin is not a hash"
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
dupd=$(echo "$H700_MODELS" | awk 'NF && $5 != "ddr3" { print $2 }' | sort | uniq -d)
[ -z "$dupd" ] && ok "each DDR4 model has its own device tree" || bad "two models share a tree: $dupd"
# A DDR3 line is a DDR4 model's twin: same tree, same md5, same model string, the name = that model's + -ddr3.
echo "$H700_MODELS" | awk 'NF && $5 == "ddr3" { n = $1; sub(/-ddr3$/, "", n); print n, $2, $3, $4 }' | while read -r twin dtb md5 model; do
  echo "$H700_MODELS" | awk -v t="$twin" -v d="$dtb" -v m="$md5" -v s="$model" '$1 == t && $2 == d && $3 == m && $4 == s && $5 != "ddr3" { f = 1 } END { exit !f }' || { echo "FAIL: $twin-ddr3 has no DDR4 twin with the same tree" >&2; exit 1; }
done && ok "every DDR3 model has a DDR4 twin with the same tree" || bad "a DDR3 model without its twin"
is_ddr3 H700 rg40xx-h-ddr3 && ! is_ddr3 H700 rg40xx-h && ! is_ddr3 H700 "" && ! is_ddr3 SM8550 odin2 && ok "is_ddr3 tells the DDR3 models" || bad "is_ddr3"
[ "$(echo "$H700_MODELS" | awk 'NF { print $1 }' | tr '\n' ' ')" = "rg35xx-plus rg35xx-sp rg28xx rg35xx-h rg35xx-pro rg35xx-2024 rg40xx-h rg40xx-v rg34xx rg34xx-sp rg-sp rgcubexx rg35xx-plus-r6 rg35xx-sp-v2 rg40xx-h-ddr3 " ] && ok "the three models with a unit to test first, then the untested ones" || bad "model list changed"
# The GAMES drive goes by model, and only where there's no Wi-Fi chip: the notes of all others don't name it.
has_games_drive H700 rg28xx && has_games_drive H700 rg35xx-2024 && ! has_games_drive H700 rg35xx-h && ! has_games_drive H700 rg35xx-plus && ! has_games_drive H700 rg35xx-sp && ! has_games_drive H700 "" && ! has_games_drive SM8250 rg28xx \
  && [ -z "$SM8550_GAMES_DRIVE_MODELS" ] && ok "GAMES drive on the RG28XX and RG35XX 2024 only" || bad "GAMES drive models: $H700_GAMES_DRIVE_MODELS / $SM8550_GAMES_DRIVE_MODELS"
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
[ "$(echo "$SM8550_MODELS" | awk 'NF { print $1 }' | tr '\n' ' ')" = "odin2-mini odin2 odin2-portal rp6 rp6-top-dpad nova " ] && ok "SM8550 models as listed" || bad "SM8550 model list changed"
# The Retroid cards that also start through UEFI and OpenABL (pins.sh SM8550_EFI_MODELS): exactly the three Retroids, each an
# SM8550 model with the same model string, two md5s, and never an Odin (OpenABL would start an Odin with a Retroid tree).
[ "$(echo "$SM8550_EFI_MODELS" | awk -F '|' 'NF { print $1 }' | tr '\n' ' ')" = "rp6 rp6-top-dpad nova " ] && ok "SM8550 EFI cards: rp6, rp6-top-dpad, nova" || bad "SM8550_EFI_MODELS list changed"
echo "$SM8550_EFI_MODELS" | while IFS='|' read -r m name orig noufs; do
  [ -n "$m" ] || continue
  [ "$(echo "$SM8550_MODELS" | awk -v m="$m" '$1 == m { print $4 }' | tr _ ' ')" = "$name" ] && hex "$orig" 32 && hex "$noufs" 32 && [ "$orig" != "$noufs" ] || { echo "FAIL: bad EFI line $m" >&2; exit 1; }
done && ok "every SM8550_EFI_MODELS line is an SM8550 model with its exact model string and two different md5s" || bad "SM8550_EFI_MODELS"
! is_efi_card SM8550 odin2 && ! is_efi_card SM8550 odin2-mini && ! is_efi_card SM8550 odin2-portal && is_efi_card SM8550 rp6 && is_efi_card SM8550 nova && ! is_efi_card SM8250 rp6 && ! is_efi_card SM8550 "" \
  && ok "is_efi_card: the Retroids only, never an Odin" || bad "is_efi_card"
sm8550_efi_pins nova && [ "$EFI_DTB_FILE" = nova-noufs.dtb ] && [ "$EFI_MODEL_NAME" = "Retroid Pocket Nova" ] && ! sm8550_efi_pins odin2 && ok "sm8550_efi_pins gives the tree's file name and model" || bad "sm8550_efi_pins"
hex "$SM8550_IMAGE_MD5" 32 && [ "$SM8550_UFS_NODES" = "/soc@0/ufshc@1d84000 /soc@0/phy@1d80000" ] && ok "SM8550 Image md5 pinned; UFS host and phy are the nodes switched off" || bad "SM8550_IMAGE_MD5 / SM8550_UFS_NODES"
# GRUB and OpenABL start the same thing, and that is KERNEL's own command line (read from ROCKNIX 20261001's SM8550 KERNEL).
. scripts/image/grub-checks.sh
G8=deploy/sd-efi-sm8550/grub.cfg
B8=deploy/sd-efi-sm8550/loader/entries/pocket-ui.conf
bls_matches_grub "$B8" "$G8" && ok "SM8550: the BLS entry starts GRUB's kernel, tree and command line" || bad "SM8550 BLS entry and grub.cfg differ"
grub_cfg_quiet_clean "$G8" && grub_cfg_quiet_for_boot_quiet "$G8" deploy/grub-quiet.sh && ok "SM8550 grub.cfg is quiet (no echo, no fbcon=, boot-quiet.sh changes nothing)" || bad "SM8550 grub.cfg not quiet"
k8="boot=LABEL=ROCKNIX disk=LABEL=STORAGE quiet rootwait console=tty0 allow_mismatched_32bit_el0 fw_devlink.strict=1 pcie_ports=compat irqaffinity=0-2 cgroup.memory=nokmem,nosocket nosoftlockup usbcore.interrupt_interval_override=045e:028e:2"
g8=$(sed -n 's/^ *linux \/boot\/pocket\/Image //p' "$G8")
miss=; for w in $k8; do case " $g8 " in *" $w "*) ;; *) miss="$miss $w" ;; esac; done
[ -z "$miss" ] && ok "SM8550 GRUB passes every word of KERNEL's command line" || bad "SM8550 grub.cfg drops KERNEL words:$miss"
[ "$(grep -c 'rp6-noufs.dtb' "$G8")" -ge 2 ] && [ "$(grep -c '^devicetree /boot/pocket/rp6-noufs.dtb$' "$B8")" = 1 ] && ok "the SM8550 templates name rp6-noufs.dtb, which the build replaces" || bad "SM8550 templates' tree name"
# The notes: the Odin cards get exactly what they had before the Retroid blocks; the Retroid cards the UEFI start and GRUB.
for t in README SOURCE-OFFER; do
  notes_blocks 0 0 <"deploy/image/$t-sm8550.txt.in" | grep -Eq '^#(if|else|endif)|UEFI|GRUB|Secure Boot' && bad "$t-sm8550: an Odin card's notes name UEFI or GRUB" || ok "$t-sm8550: the Odin cards' notes have no UEFI or GRUB"
  notes_blocks 0 1 <"deploy/image/$t-sm8550.txt.in" | grep -Eq '^#(if|else|endif)' && bad "$t-sm8550: a block line is left" || ok "$t-sm8550: no block lines left on a Retroid card"
done
notes_blocks 0 1 <deploy/image/README-sm8550.txt.in | grep -q 'Pick "UEFI"' && notes_blocks 0 1 <deploy/image/README-sm8550.txt.in | grep -q '@EFI_DTB_FILE@' \
  && ! notes_blocks 0 1 <deploy/image/README-sm8550.txt.in | grep -q 'needs one thing first' && ok "a Retroid card's notes start with UEFI and name its tree, not the ABL first" || bad "Retroid notes"
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
# Classic Rev Q only: selecting a family must never inherit another model or release.
family_pins SM4450
[ "$FAM_IMG" = "$SM4450_IMG" ] && [ "$ROCKNIX_TAG" = 20261101 ] && ok "Classic uses its SM4450 20261101 base" || bad "Classic base"
[ "$(echo "$FAM_MODELS" | awk 'NF { print $1, $4 }')" = "rp-classic Retroid_Pocket_Classic_Rev_Q" ] && ok "Classic beta accepts only Rev Q" || bad "Classic models"
echo "$FAM_SYSTEM_REMOVE" | grep -qx usr/share/bootloader/openabl && ok "Classic drops the ABL flash payload" || bad "Classic keeps ABL"
! echo "$FAM_SYSTEM_REMOVE" | grep -q RPClassic && ok "Classic keeps its audio/battery firmware" || bad "Classic firmware removed"
for t in README SOURCE-OFFER; do
  for ph in $(grep -o '@[A-Z_]*@' "deploy/image/$t-sm4450.txt.in" | sort -u); do
    echo "$known" | grep -qx "$ph" || bad "$t-sm4450.txt.in: unresolved $ph"
  done
done
# The RK3566 pins and notes are private (scripts/public-exclude.txt): in the public tree there is nothing of them to test.
if [ -f scripts/image/pins-rk3566.sh ]; then
# RK3566 (RG DS Plus test image): its own ROCKNIX release, one model, extlinux.conf names its tree.
hex "$RK3566_GZ_SHA256" 64 && hex "$RK3566_IMG_SHA256" 64 && hex "$RK3566_KERNEL_MD5" 32 && hex "$RK3566_SYSTEM_MD5" 32 \
  && hex "$RK3566_LOADER_MD5" 32 && hex "$RK3566_EXTLINUX_MD5" 32 && hex "$RK3566_COMMIT" 40 && ok "RK3566 pins are hashes" || bad "an RK3566 pin is not a hash"
case $RK3566_POCKET_SYSTEM_MD5 in unset) ok "RK3566_POCKET_SYSTEM_MD5 is unset (first build)" ;; *) hex "$RK3566_POCKET_SYSTEM_MD5" 32 && ok "RK3566_POCKET_SYSTEM_MD5 is a hash" || bad "RK3566_POCKET_SYSTEM_MD5" ;; esac
[ "$(echo "$RK3566_MODELS" | awk 'NF { print $1, $2, $4 }')" = "rg-ds-plus rk3568-anbernic-rg-ds-plus Anbernic_RG_DS_Plus" ] \
  && ok "RK3566: the RG DS Plus with its own tree (not the plain RG DS's)" || bad "RK3566_MODELS changed"
family_pins RK3566
[ "$ROCKNIX_TAG" = 20261101 ] && [ "$ROCKNIX_COMMIT" = "$RK3566_COMMIT" ] && [ "$ROCKNIX_LICENSE_FILES" = 75 ] && [ "$FAM_IMG" = "$RK3566_IMG" ] \
  && ok "family_pins RK3566 gives ROCKNIX 20261101's tag, commit and licence count" || bad "family_pins RK3566: $ROCKNIX_TAG $ROCKNIX_COMMIT"
family_pins H700
[ "$ROCKNIX_TAG" = 20261001 ] && [ "$ROCKNIX_COMMIT" = "$ROCKNIX_BASE_COMMIT" ] && [ "$ROCKNIX_LICENSE_FILES" = 76 ] \
  && ok "the other families stay on 20261001 after an RK3566 lookup" || bad "family_pins H700 after RK3566: $ROCKNIX_TAG"
[ -z "$RK3566_GAMES_DRIVE_MODELS" ] && ! grep -q GAMES deploy/image/README-rk3566.txt.in && ok "RK3566: no GAMES drive, the notes don't name it" || bad "RK3566 GAMES drive"
for t in README SOURCE-OFFER; do
  for ph in $(grep -o '@[A-Z_]*@' "deploy/image/$t-rk3566.txt.in" | sort -u); do
    echo "$known" | grep -qx "$ph" || bad "$t-rk3566.txt.in: $ph is not filled in by make-image-family.sh"
  done
done
c=$(grep -Ec '^  [0-9a-f]{32} ' deploy/image/firmware-licences-rk3566/README.txt)
[ "$c" = 293 ] && ok "firmware-licences-rk3566/README.txt lists the 293 firmware files with md5" || bad "RK3566 firmware list has $c md5 lines"
for f in $RK3566_FW_SHARED; do [ -s "deploy/image/firmware-licences/$f" ] || bad "shared licence text missing: $f"; done
fi
sh scripts/make-image-family.sh --family AMD64 >/dev/null 2>&1 && bad "an unknown family was accepted" || ok "an unknown family is refused"
# Every card gives its handheld its own name on the network (system.hostname): lowercase ASCII, at most 24 characters, never
# ROCKNIX's default, different on each model, and image-defaults.sh has the one NAME line the build and the checks replace.
hosts=$(for m in "" $(echo "$H700_MODELS $SM8550_MODELS $SM4450_MODELS $RK3566_MODELS" | awk 'NF { print $1 }'); do image_hostname "$m"; done)
bad_host=$(echo "$hosts" | grep -Ev '^[a-z][a-z0-9-]{0,23}$|^$')
[ -z "$bad_host" ] && ! echo "$hosts" | grep -qix 'rocknix\|sm8250\|sm8550\|h700\|rk3566' && ok "every image has a valid hostname of its own kind" || bad "hostname not valid: $bad_host"
[ -z "$(echo "$hosts" | sort | uniq -d)" ] && [ "$(image_hostname "")" = pocket-mini-v2 ] && [ "$(image_hostname rg35xx-plus)" = pocket-rg35xx-plus ] && [ "$(image_hostname rg-ds-plus)" = pocket-rg-ds-plus ] \
  && ok "hostnames are unique: pocket-mini-v2, pocket-rg35xx-plus, ..." || bad "hostnames clash or changed: $(echo $hosts)"
[ "$(grep -c '^NAME=pocket-ui$' deploy/image/image-defaults.sh)" = 1 ] && grep -q 'set_setting system.hostname "\$NAME"' deploy/image/image-defaults.sh \
  && ok "image-defaults.sh has one NAME line and sets system.hostname from it" || bad "image-defaults.sh: NAME line"
sed "s/^NAME=pocket-ui\$/NAME=$(image_hostname rg28xx)/" deploy/image/image-defaults.sh | grep -qx 'NAME=pocket-rg28xx' && ok "the build's sed puts the model's name on the NAME line" || bad "sed does not set NAME"
python3 -I scripts/image/test-tree-firmware.py && ok "Classic speaker labels do not hide valid firmware" || bad "firmware path parser"
exit $fail
