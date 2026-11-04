#!/bin/sh
# Offline checks of a pocket-ui SD image of a family that boots ROCKNIX's own way: FAMILY=H700 or SM8550 (the other half
# of scripts/image/check.sh). Runs in the pocket-image-tools container (scripts/check-image-family.sh starts it), with the
# repository at /repo. Only reads the image: the FAT partition through mtools, STORAGE as a copy.
#   FAMILY=H700 check-h700.sh IMG VERSION WORKDIR MODEL
set -u
. /repo/scripts/image/pins.sh
IMG=$1
VERSION=$2
W=$3
MODEL=$4
FAMILY=${FAMILY:-H700}
FAM=$(echo "$FAMILY" | tr 'A-Z' 'a-z')
family_pins "$FAMILY"
# This model's card makes a GAMES drive (pins.sh); every other one is ROCKNIX's layout, with none of its files on it.
GAMES=0
has_games_drive "$FAMILY" "$MODEL" && GAMES=1
set -- $(echo "$FAM_MODELS" | awk -v m="$MODEL" '$1 == m { print $2, $3, $4 }')
[ $# = 3 ] || { echo "FAIL $MODEL is not in ${FAMILY}_MODELS"; exit 1; }
DTB_NAME=$1
DTB_MD5_WANT=$2
DT_MODEL=$(echo "$3" | tr _ ' ')
# The family's pins under the names the shared checks use.
SYSTEM_REMOVE=$FAM_SYSTEM_REMOVE
SYSTEM_ZIP_REMOVE=$FAM_SYSTEM_ZIP_REMOVE
POCKET_SYSTEM_MD5=$FAM_POCKET_SYSTEM_MD5
KERNEL_MD5=$FAM_KERNEL_MD5
FAT="$IMG@@$((P1_START * 512))"
fails=0
warns=0

check() {
  if eval "$2"; then
    echo "ok   $1"
  else
    echo "FAIL $1"
    fails=$((fails + 1))
    return 1
  fi
}
warn() {
  if eval "$2"; then
    echo "ok   $1"
  else
    echo "WARN $1"
    warns=$((warns + 1))
  fi
}
md5() { md5sum "$1" 2>/dev/null | cut -d' ' -f1; }
same() { cmp -s "$1" "$2"; }

rm -rf "$W/fat" "$W/releases" "$W/p2.ext4" "$W/lists"
mkdir -p "$W/fat" "$W/lists"

echo "== partition table"
sfdisk -d "$IMG" >"$W/lists/sfdisk" 2>/dev/null
part() { grep "^$IMG$1 " "$W/lists/sfdisk" | sed 's/^[^:]*: *//; s/, uuid=[^,]*//'; }
check "exactly two partitions" '[ "$(grep -c "^$IMG[0-9]" "$W/lists/sfdisk")" = 2 ]'
if [ "$FAMILY" = H700 ]; then
check "MBR (dos), as ROCKNIX's H700 image" 'grep -qx "label: dos" "$W/lists/sfdisk"'
check "p1 FAT32 (LBA), bootable, as in ROCKNIX $ROCKNIX_TAG" \
  '[ "$(part 1)" = "start=       $P1_START, size=     $P1_SIZE, type=c, bootable" ]'
check "p2 Linux, as in ROCKNIX $ROCKNIX_TAG" '[ "$(part 2)" = "start=     $P2_START, size=       $P2_SIZE, type=83" ]'
else
check "GPT, as ROCKNIX's $FAMILY image" 'grep -qx "label: gpt" "$W/lists/sfdisk"'
check "p1 \"system\", as in ROCKNIX $ROCKNIX_TAG" \
  '[ "$(part 1)" = "start=       $P1_START, size=     $P1_SIZE, type=EBD0A0A2-B9E5-4433-87C0-68B6B72699C7, name=\"system\", attrs=\"LegacyBIOSBootable\"" ]'
check "p2 \"storage\", as in ROCKNIX $ROCKNIX_TAG" '[ "$(part 2)" = "start=     $P2_START, size=       $P2_SIZE, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, name=\"storage\"" ]'
fi
check "image size unchanged (2,198,863,872 bytes)" '[ "$(stat -c %s "$IMG")" = 2198863872 ]'
# ROCKNIX's U-Boot sits before the first partition at 8 KiB (dd seek=8, bootloader/update.sh): the card's boot chain.
tail -c +8193 "$IMG" | head -c 666105 >"$W/lists/uboot"
[ "$FAMILY" != H700 ] || check "ROCKNIX's DDR4 U-Boot is 8 KiB into the card, unchanged" '[ "$(md5 "$W/lists/uboot")" = f7008775f3e7b5fa45f869288752d3a3 ]'

echo "== FAT partition (/flash)"
check "label ROCKNIX (boot=LABEL=ROCKNIX)" 'mlabel -i "$FAT" -s :: 2>/dev/null | grep -q "Volume label is ROCKNIX"'
# Bare recursive listing: one path per line, directories with a trailing slash.
mdir -/ -b -i "$FAT" ::/ 2>/dev/null | sed 's#^::/##' | sort >"$W/lists/fat"
cat >"$W/lists/fat.expected" <<EOF
KERNEL
KERNEL.md5
SYSTEM
SYSTEM.md5
device_trees/
dtb.img
extlinux/
extlinux/extlinux.conf
mount-storage.sh
overlays/
overlays/sun50i-h700-anbernic-rg35xx-2024-ddr3.dtbo
pocket-ui-info/
pocket-ui-info/README.txt
pocket-ui-info/SOURCE-OFFER.txt
pocket-ui-info/pocket-ui-os.txt
post-flash.sh
EOF
mdir -b -i "$FAT" ::/device_trees 2>/dev/null | sed 's#.*/#device_trees/#' >>"$W/lists/fat.expected"
# SM8550: ROCKNIX's ABL in the handheld starts KERNEL, so the card has none of the H700's boot files (and no rocknix_abl).
if [ "$FAMILY" = SM8550 ]; then
  grep -Ev '^(device_trees/|dtb\.img|extlinux/|overlays/|games-first-start\.sh)' "$W/lists/fat.expected" >"$W/lists/fat.expected.new"
  mv "$W/lists/fat.expected.new" "$W/lists/fat.expected"
fi
# The firmware's terms, as in the repository.
echo "pocket-ui-info/firmware-licences/" >>"$W/lists/fat.expected"
for f in $(ls "/repo/deploy/image/firmware-licences-$FAM") $FAM_FW_SHARED; do echo "pocket-ui-info/firmware-licences/$f" >>"$W/lists/fat.expected"; done
# pocket-ui's LICENSE and NOTICE.md belong on the card as soon as the repository has them.
for f in LICENSE NOTICE.md; do [ ! -f "/repo/$f" ] || echo "pocket-ui-info/$f" >>"$W/lists/fat.expected"; done
# The crates' licence texts and the fonts' OFL (scripts/licence-files.sh); mdir lists the directory too.
echo "pocket-ui-info/licences/" >>"$W/lists/fat.expected"
for f in LICENSE NOTICE.md FONTS-OFL.txt THIRD-PARTY-CRATES.md THIRD-PARTY-LICENSES.txt; do echo "pocket-ui-info/licences/$f" >>"$W/lists/fat.expected"; done
[ $GAMES = 0 ] || echo games-first-start.sh >>"$W/lists/fat.expected"
sort -o "$W/lists/fat.expected" "$W/lists/fat.expected"
# ROCKNIX's LICENSE.md and licenses folder come from the tag at build time; counted and spot-checked below.
grep -v '^pocket-ui-info/rocknix-licenses/' "$W/lists/fat" >"$W/lists/fat.own"
check "exactly the expected files (ROCKNIX's, dtb.img, the hooks, pocket-ui-info/; nothing else)" 'same "$W/lists/fat.own" "$W/lists/fat.expected"' || diff "$W/lists/fat.expected" "$W/lists/fat.own" | sed 's/^/     /'
check "rocknix-licenses/: ROCKNIX's LICENSE.md and its licenses folder ($ROCKNIX_LICENSE_FILES files)" \
  '[ "$(grep -c "^pocket-ui-info/rocknix-licenses/.*[^/]$" "$W/lists/fat")" = "$ROCKNIX_LICENSE_FILES" ] && grep -qx "pocket-ui-info/rocknix-licenses/LICENSE.md" "$W/lists/fat" && grep -qx "pocket-ui-info/rocknix-licenses/licenses/GPL-2.0-only.txt" "$W/lists/fat"'
for f in KERNEL KERNEL.md5 SYSTEM.md5 dtb.img extlinux/extlinux.conf mount-storage.sh post-flash.sh games-first-start.sh \
  pocket-ui-info/README.txt pocket-ui-info/pocket-ui-os.txt; do
  mkdir -p "$W/fat/$(dirname "$f")"
  mcopy -n -i "$FAT" "::/$f" "$W/fat/$f" 2>/dev/null
done
check "KERNEL is ROCKNIX $ROCKNIX_TAG (md5 $KERNEL_MD5)" '[ "$(md5 "$W/fat/KERNEL")" = "$KERNEL_MD5" ]'
check "KERNEL.md5 agrees" '[ "$(cut -d" " -f1 "$W/fat/KERNEL.md5")" = "$KERNEL_MD5" ]'
mcopy -n -i "$FAT" ::/SYSTEM "$W/SYSTEM" 2>/dev/null
check "SYSTEM is ROCKNIX $ROCKNIX_TAG's without SYSTEM_REMOVE (md5 $POCKET_SYSTEM_MD5)" '[ "$(md5 "$W/SYSTEM")" = "$POCKET_SYSTEM_MD5" ]'
check "SYSTEM.md5 agrees, in ROCKNIX's format" '[ "$(mtype -i "$FAT" ::/SYSTEM.md5)" = "$POCKET_SYSTEM_MD5  target/SYSTEM" ]'

check "FAT free space is all zeros (no remnant of ROCKNIX's original SYSTEM)" \
  'python3 -I /repo/scripts/image/fat-free-zero.py "$IMG" "$((P1_START * 512))" --check >/dev/null'
dd if="$IMG" of="$W/p1.fat" bs=512 skip="$P1_START" count="$P1_SIZE" status=none
check "FAT file system clean (fsck.vfat -n)" 'fsck.vfat -n "$W/p1.fat" >"$W/lists/fsck.vfat" 2>&1'
rm -f "$W/p1.fat"

echo "== inside SYSTEM"
unsquashfs -lln "$W/SYSTEM" 2>/dev/null | sed -n 's#^.* squashfs-root/##p' >"$W/lists/system"
check "SYSTEM lists its files ($(wc -l <"$W/lists/system") entries)" '[ "$(wc -l <"$W/lists/system")" -gt 50000 ]'
for p in $SYSTEM_REMOVE; do
  check "not in SYSTEM: $p" '! grep -q "^$p\\( \\|/\\|$\\)" "$W/lists/system"'
done
syscat() { unsquashfs -cat "$W/SYSTEM" "$1" 2>/dev/null; }
echo "$SYSTEM_ZIP_REMOVE" | while read -r z glob; do
  [ -n "$z" ] || continue
  syscat "$z" >"$W/lists/zip"
  check "$z: intact, without $glob" \
    'unzip -tq "$W/lists/zip" >/dev/null && [ -z "$(unzip -Z1 "$W/lists/zip" | grep "^$(echo "$glob" | sed "s/\\*/[^\\/]*/g")\$")" ]' || echo "FAILED-ZIP" >>"$W/lists/zipfail"
done
check "every SYSTEM_ZIP_REMOVE archive checked out" '[ ! -e "$W/lists/zipfail" ]'
rm -f "$W/lists/zip" "$W/lists/zipfail"
# Remote access stays off until the player turns it on in pocket-ui (docs/SECURITY-HARDENING.md).
for k in ssh.enabled samba.enabled syncthing.enabled; do
  check "ROCKNIX's default $k=0 (nothing listens until it is turned on)" 'syscat usr/config/system/configs/system.cfg | grep -qx "$k=0"'
done
check "no SSH keys or host keys in SYSTEM (usr/config/ssh/authorized_keys, ssh_host_*)" \
  '! grep -Eq "(^usr/config/ssh/authorized_keys|ssh_host_.*key)" "$W/lists/system"'
if [ "$FAMILY" = H700 ]; then
check "the kernel command line does not turn SSH on (no ssh word)" '! grep -Eq "^ *APPEND .* ssh( |$)" "$W/fat/extlinux/extlinux.conf"'
check "the only BIOS files in SYSTEM are DraStic's open replacement pair (BSD, drastic_bios_arm7/arm9.bin)" \
  '[ "$(grep -i "bios.*\\.bin$" "$W/lists/system" | sort | tr "\n" " ")" = "usr/config/drastic/system/drastic_bios_arm7.bin usr/config/drastic/system/drastic_bios_arm9.bin " ]'
else
check "the only BIOS files in SYSTEM are DraStic's open pair and ReGBA's open GBA BIOS (GPL, md5 $REGBA_BIOS_MD5)" \
  '[ "$(grep -i "bios.*\\.bin$" "$W/lists/system" | sort | tr "\n" " ")" = "usr/config/drastic/system/drastic_bios_arm7.bin usr/config/drastic/system/drastic_bios_arm9.bin usr/config/nanoboyadvance/bios/gba_bios.bin " ] && [ "$(syscat usr/config/nanoboyadvance/bios/gba_bios.bin | md5sum | cut -d" " -f1)" = "$REGBA_BIOS_MD5" ]'
# ROCKNIX's update.sh runs updateabl, which flashes the ABL from this folder: without the file it cannot (exit 1, "ABL update not found").
check "no ABL anywhere in SYSTEM: nothing on this card can flash the handheld's bootloader" '! grep -q "abl_signed\|rocknix_abl" "$W/lists/system"'
fi
check "DraStic is there unchanged with its permission texts (usr/config/drastic: drastic, LICENSE.pdf)" \
  'grep -qx "usr/config/drastic/drastic" "$W/lists/system" && grep -qx "usr/config/drastic/LICENSE.pdf" "$W/lists/system"'
check "ROCKNIX's licenses/DRASTIC.pdf is on the card" '[ -n "$(mdir -b -i "$FAT" ::/pocket-ui-info/rocknix-licenses/licenses 2>/dev/null | grep DRASTIC.pdf)" ]'
FW=usr/lib/kernel-overlays/base/lib/firmware
unsquashfs -lln "$W/SYSTEM" "$FW" 2>/dev/null | grep '^-' | sed -n "s#^.* squashfs-root/$FW/##p" | sort >"$W/lists/firmware"
check "every firmware file in SYSTEM ($(wc -l <"$W/lists/firmware")) is listed with its terms in firmware-licences-$FAM/README.txt" \
  '( while read -r f; do grep -q " $f\\( \\|\$\\)" /repo/deploy/image/firmware-licences-$FAM/README.txt || { echo "     not listed: $f"; exit 1; }; done <"$W/lists/firmware" )'
if [ $GAMES = 1 ]; then
  # The GAMES drive's first start (deploy/image/games-first-start.sh) replaces this file for one boot and goes back to it.
  check "SYSTEM has ROCKNIX's fs-resize, its service, and what the first start and the drive's mount run" \
    'for p in usr/lib/rocknix/fs-resize usr/lib/systemd/system/fs-resize.service usr/lib/systemd/system/rocknix-automount.service usr/bin/automount usr/config/system-dirs.conf usr/sbin/parted usr/sbin/partprobe usr/sbin/mkfs.exfat usr/sbin/fsck.exfat usr/sbin/e2fsck usr/sbin/resize2fs usr/sbin/tune2fs usr/sbin/blkid usr/bin/timeout; do grep -q "^$p\\( \\|\$\\)" "$W/lists/system" || { echo "     not in SYSTEM: $p"; exit 1; }; done'
  syscat usr/bin/automount >"$W/lists/automount"
  check "ROCKNIX's automount takes an unmounted partition of a card over 8 GB, highest device first (a second card wins over GAMES)" \
    'grep -qF "\${SIZE} <= 8388608" "$W/lists/automount" && grep -qF "sort -r" "$W/lists/automount" && grep -qF "/fat/" "$W/lists/automount"'
fi
rm -f "$W/SYSTEM"

echo "== boot files"
boot_sm8550() {
  # ROCKNIX's ABL starts KERNEL, an Android boot image: the words after boot= are its kernel command line.
  head -c 2048 "$W/fat/KERNEL" | tr -d '\000' | grep -a -o 'boot=LABEL[ -~]*' >"$W/lists/cmdline"
  check "KERNEL is an Android boot image: boot=LABEL=ROCKNIX disk=LABEL=STORAGE" '[ "$(head -c 8 "$W/fat/KERNEL")" = "ANDROID!" ] && grep -q "^boot=LABEL=ROCKNIX disk=LABEL=STORAGE " "$W/lists/cmdline"'
  check "the kernel command line does not turn SSH on (no ssh word)" '! grep -Eq "( |^)ssh( |$)" "$W/lists/cmdline"'
  check "no GRUB, no EFI folder, no boot/ folder, no rocknix_abl/ (nothing on the card flashes or boots but KERNEL)" '! grep -Eq "^(EFI|boot|rocknix_abl)/" "$W/lists/fat"'
  check "KERNEL holds the tree of the $DT_MODEL" 'python3 -I /repo/scripts/make-test-bootimg.py "$W/fat/KERNEL" "$W/lists/boot.img" "$DT_MODEL" >/dev/null 2>&1 && [ "$(fdtget -t s "$W/lists/boot.img.orig.dtb" / model)" = "$DT_MODEL" ]'
  # What that tree asks for by name: if the list of removed files took one, the handheld loses its sound or battery.
  dtc -q -I dtb -O dts "$W/lists/boot.img.orig.dtb" 2>/dev/null | sed -n '/firmware-name/,/;/p' | grep -o '"[^"]*"' | tr -d '"' | sed 's/\\0/\n/g' | grep . | sort -u >"$W/lists/tree-firmware"
  check "every firmware the tree asks for ($(wc -l <"$W/lists/tree-firmware") files) is in SYSTEM" \
    '[ -s "$W/lists/tree-firmware" ] && ( while read -r f; do grep -qx "usr/lib/kernel-overlays/base/lib/firmware/$f" "$W/lists/system" || { echo "     missing: $f"; exit 1; }; done <"$W/lists/tree-firmware" )'
  rm -f "$W/lists/boot.img"*
}
boot_h700() {
X=$W/fat/extlinux/extlinux.conf
# ROCKNIX's U-Boot reads extlinux/extlinux.conf: kernel /KERNEL, tree /dtb.img. It is ROCKNIX's file, unchanged.
check "extlinux.conf: LINUX /KERNEL, FDT /dtb.img, boot=LABEL=ROCKNIX disk=LABEL=STORAGE" \
  'grep -qx "  LINUX /KERNEL" "$X" && grep -qx "  FDT /dtb.img" "$X" && grep -Eq "^  APPEND boot=LABEL=ROCKNIX disk=LABEL=STORAGE " "$X"'
check "no GRUB, no EFI folder, no boot/ folder (ROCKNIX's U-Boot boots it)" '! grep -Eq "^(EFI|boot)/" "$W/lists/fat"'
check "dtb.img is device_trees/$DTB_NAME.dtb (md5 $DTB_MD5_WANT)" '[ "$(md5 "$W/fat/dtb.img")" = "$DTB_MD5_WANT" ]'
mcopy -n -i "$FAT" "::/device_trees/$DTB_NAME.dtb" "$W/fat/tree.dtb" 2>/dev/null
check "dtb.img is byte for byte that tree on the card" 'same "$W/fat/dtb.img" "$W/fat/tree.dtb"'
check "dtb model is $DT_MODEL" '[ "$(fdtget -t s "$W/fat/dtb.img" / model)" = "$DT_MODEL" ]'
check "dtb.img is a device tree ROCKNIX ships and this model's only: no other model's dtb.img would match" \
  '[ "$(for d in $(mdir -b -i "$FAT" ::/device_trees | sed "s#.*/##"); do mcopy -n -i "$FAT" "::/device_trees/$d" - | cmp -s - "$W/fat/dtb.img" && echo "$d"; done)" = "$DTB_NAME.dtb" ]'
}
case $FAMILY in H700) boot_h700 ;; SM8550) boot_sm8550 ;; esac
check "KERNEL.md5 is KERNEL's" '[ "$(cut -d" " -f1 "$W/fat/KERNEL.md5")" = "$(md5 "$W/fat/KERNEL")" ]'
check "initramfs hook mount-storage.sh is deploy/image/mount-storage.sh" 'same "$W/fat/mount-storage.sh" /repo/deploy/image/mount-storage.sh'
check "initramfs hook post-flash.sh is deploy/image/post-flash.sh" 'same "$W/fat/post-flash.sh" /repo/deploy/image/post-flash.sh'
if [ $GAMES = 1 ]; then
  check "first start's games-first-start.sh is deploy/image/games-first-start.sh, a shell script" \
    'same "$W/fat/games-first-start.sh" /repo/deploy/image/games-first-start.sh && [ "$(head -c 9 "$W/fat/games-first-start.sh")" = "#!/bin/sh" ]'
else
  check "ROCKNIX's own first start: no games-first-start.sh on the card" '! grep -qx games-first-start.sh "$W/lists/fat"'
fi
check "pocket-ui-info/pocket-ui-os.txt: $POCKET_OS_NAME $VERSION, for Settings > About" \
  '[ "$(cat "$W/fat/pocket-ui-info/pocket-ui-os.txt")" = "$(printf "NAME=%s\nVERSION=%s" "$POCKET_OS_NAME" "$VERSION")" ]'

echo "== notes on the card (pocket-ui-info/README.txt)"
R=$W/fat/pocket-ui-info/README.txt
if [ "$FAMILY" = H700 ]; then
check "names the model: $DT_MODEL, DDR4, dtb.img from device_trees/$DTB_NAME.dtb" 'grep -q "for the $DT_MODEL: SD card image" "$R" && grep -q "DDR4" "$R" && grep -q "device_trees/$DTB_NAME.dtb" "$R"'
else
check "names the model: $DT_MODEL" 'grep -q "for the $DT_MODEL: SD card image" "$R"'
check "says plainly what the handheld needs first: ROCKNIX's boot helper (ABL), once, from Android, and that this card doesn't do it" \
  'grep -q "needs one thing first" "$R" && grep -q "ABL" "$R" && grep -q "from Android" "$R" && grep -q "does not flash" "$R"'
check "says an internal ROCKNIX install wins over this card, and what to use instead" 'grep -q "internal storage, this card" "$R" && grep -q "install package" "$R"'
check "says the firmware without a stated licence stays, and comes out on request" 'grep -q "states no licence" "$R" && grep -q "on request" "$R"'
fi
check "DraStic: closed source, unchanged like ROCKNIX, 2017 statement, not a formal licence, credit Exophase, removed on request" 'grep -q "DraStic by Exophase" "$R" && grep -q "closed-source" "$R" && grep -q "not a formal licence" "$R" && grep -q "on request" "$R"'
check "names version $VERSION" 'grep -q "pocket-ui $VERSION" "$R"'
check "$POCKET_OS_NAME, built on ROCKNIX, not made or endorsed by its team, with link" 'grep -q "$POCKET_OS_NAME $VERSION" "$R" && grep -q "built on ROCKNIX" "$R" && grep -q "not made or" "$R" && grep -q "https://github.com/ROCKNIX/distribution" "$R"'
check "source offer: ROCKNIX tag $ROCKNIX_TAG and commit" 'grep -q "tag $ROCKNIX_TAG" "$R" && grep -q "$ROCKNIX_COMMIT" "$R"'
check "artwork under CC BY-NC-SA 4.0" 'grep -q "CC BY-NC-SA 4.0" "$R"'
check "free, non-commercial distribution only" 'grep -qi "non-commercial" "$R"'
check "says: no games, no console maker's BIOS files" 'grep -q "no games and no console maker" "$R"'
check "says what was removed from SYSTEM and why (SKick, DOOM shareware)" 'grep -q "skick346.lha" "$R" && grep -q "DOOM shareware" "$R"'
if [ $GAMES = 1 ]; then
  check "says how to copy games: the GAMES drive, never format" 'grep -q "GAMES" "$R" && grep -q "never say yes" "$R"'
else
  check "says nothing of a GAMES drive, and how to copy games: file sharing, a second card, never format" '! grep -q "GAMES" "$R" && grep -q "file sharing" "$R" && grep -q "never say yes" "$R"'
fi
check "no #if lines of the notes' template left" '! grep -Eq "^#(if|else|endif)" "$R"'
[ "$FAMILY" != H700 ] || check "going back: the original card, into the first slot (TF1)" 'grep -q "original card back" "$R"'
check "says remote access is off, and that pocket-ui makes the password" 'grep -q "Remote access (SSH), file sharing and Syncthing are off" "$R"'
check "points to the source next to the image and to SOURCE-OFFER.txt" 'grep -q "pocket-ui-info/SOURCE-OFFER.txt" "$R"'
warn "names a clean pocket-ui source (no -dirty commit)" '! grep -q -- "-dirty" "$R"'
mcopy -n -i "$FAT" ::/pocket-ui-info/SOURCE-OFFER.txt "$W/fat/pocket-ui-info/SOURCE-OFFER.txt" 2>/dev/null
O=$W/fat/pocket-ui-info/SOURCE-OFFER.txt
check "SOURCE-OFFER.txt: written offer for three years, how to ask, ROCKNIX tag and commit" \
  'grep -q "^Written offer" "$O" && grep -q "at least three years" "$O" && grep -q "https://github.com/.*/issues" "$O" && grep -q "$ROCKNIX_COMMIT" "$O" && grep -q "pocket-ui $VERSION" "$O"'
mkdir -p "$W/fat/pocket-ui-info/firmware-licences"
for f in README.txt $FAM_FW_SHARED; do
  mcopy -n -i "$FAT" "::/pocket-ui-info/firmware-licences/$f" "$W/fat/pocket-ui-info/firmware-licences/$f" 2>/dev/null
done
check "firmware-licences/: the family's list and the licence texts it names" \
  '( same "$W/fat/pocket-ui-info/firmware-licences/README.txt" /repo/deploy/image/firmware-licences-$FAM/README.txt && for f in $FAM_FW_SHARED; do same "$W/fat/pocket-ui-info/firmware-licences/$f" "/repo/deploy/image/firmware-licences/$f" || exit 1; done )'
check "pocket-ui licensed GPL-2.0-or-later, credits in NOTICE.md" \
  'grep -q "GNU GPL version 2 or later (pocket-ui-info/LICENSE)" "$R" && grep -q "pocket-ui-info/NOTICE.md" "$R"'
for f in LICENSE NOTICE.md; do
  if [ -f "/repo/$f" ]; then
    mcopy -n -i "$FAT" "::/pocket-ui-info/$f" "$W/fat/pocket-ui-info/$f" 2>/dev/null
    check "pocket-ui-info/$f is the repository's" 'same "$W/fat/pocket-ui-info/$f" "/repo/$f"'
  else
    warn "pocket-ui-info/$f: the repository has no $f yet (the image is not releasable without it)" false
  fi
done

for f in FONTS-OFL.txt THIRD-PARTY-CRATES.md THIRD-PARTY-LICENSES.txt; do
  mkdir -p "$W/fat/pocket-ui-info/licences"
  mcopy -n -i "$FAT" "::/pocket-ui-info/licences/$f" "$W/fat/pocket-ui-info/licences/$f" 2>/dev/null
  check "pocket-ui-info/licences/$f is not empty" '[ -s "$W/fat/pocket-ui-info/licences/$f" ]'
done
check "pocket-ui-info/licences/FONTS-OFL.txt is assets/fonts/OFL.txt" 'same "$W/fat/pocket-ui-info/licences/FONTS-OFL.txt" /repo/assets/fonts/OFL.txt'

echo "== STORAGE partition"
dd if="$IMG" of="$W/p2.ext4" bs=512 skip="$P2_START" count="$P2_SIZE" status=none
check "label STORAGE (disk=LABEL=STORAGE)" '[ "$(e2label "$W/p2.ext4")" = STORAGE ]'
check "file system clean (e2fsck -fn)" 'e2fsck -fn "$W/p2.ext4" >"$W/lists/e2fsck" 2>&1'
check "STORAGE survives the first boot's resize on any card, 8 GB to 2 TB (grow-check.sh)" 'P2_START=$P2_START sh /repo/scripts/image/grow-check.sh "$W/p2.ext4"'
# Dumped inside the container, not to the bind mount, so owners and modes are the ones in the image.
S=/tmp/storage
rm -rf "$S"
mkdir -p "$S"
debugfs -R "rdump / $S" "$W/p2.ext4" >/dev/null 2>&1
(cd "$S" && find . -printf '%m %U:%G %y %p %l\n' | sed 's/ $//' | sort) >"$W/lists/storage"
V=$VERSION
sort >"$W/lists/storage.expected" <<EOF
755 0:0 d .
700 0:0 d ./lost+found
644 0:0 f ./.please_resize_me
755 0:0 d ./.pocket-seed
755 0:0 d ./.pocket-seed/.config
755 0:0 d ./.pocket-seed/.config/autostart
755 0:0 f ./.pocket-seed/.config/autostart/pocket-ui-image-defaults
755 0:0 d ./.pocket-seed/.config/system.d
755 0:0 d ./.pocket-seed/.config/system.d/essway.service.d
644 0:0 f ./.pocket-seed/.config/system.d/essway.service.d/10-pocket-ui.conf
644 0:0 f ./.pocket-seed/.config/system.d/pocket-splash.service
755 0:0 d ./.pocket-seed/.config/system.d/sysinit.target.wants
777 0:0 l ./.pocket-seed/.config/system.d/sysinit.target.wants/pocket-splash.service ../pocket-splash.service
755 0:0 d ./pocket-ui
755 0:0 f ./pocket-ui/run-pocket-ui.sh
777 0:0 l ./pocket-ui/current releases/$V
777 0:0 l ./pocket-ui/pocket-ui current/pocket-ui
777 0:0 l ./pocket-ui/pocket-core current/pocket-core
777 0:0 l ./pocket-ui/portmaster-setup.sh current/portmaster-setup.sh
755 0:0 d ./pocket-ui/licences
644 0:0 f ./pocket-ui/licences/FONTS-OFL.txt
644 0:0 f ./pocket-ui/licences/LICENSE
644 0:0 f ./pocket-ui/licences/NOTICE.md
644 0:0 f ./pocket-ui/licences/THIRD-PARTY-CRATES.md
644 0:0 f ./pocket-ui/licences/THIRD-PARTY-LICENSES.txt
755 0:0 d ./pocket-ui/releases
755 0:0 d ./pocket-ui/releases/$V
755 0:0 f ./pocket-ui/releases/$V/pocket-ui
755 0:0 f ./pocket-ui/releases/$V/pocket-core
755 0:0 f ./pocket-ui/releases/$V/pocket-splash
755 0:0 f ./pocket-ui/releases/$V/portmaster-setup.sh
EOF
if [ $GAMES = 1 ]; then
  sort -o "$W/lists/storage.expected" - "$W/lists/storage.expected" <<GAMESFILES
755 0:0 d ./.pocket-seed/.config/system.d/rocknix-automount.service.d
644 0:0 f ./.pocket-seed/.config/system.d/rocknix-automount.service.d/10-pocket-games.conf
755 0:0 f ./pocket-ui/games-mount.sh
GAMESFILES
fi
check "exactly the expected files, owners and modes" 'same "$W/lists/storage" "$W/lists/storage.expected"' ||
  diff "$W/lists/storage.expected" "$W/lists/storage" | sed 's/^/     /'
check ".please_resize_me there: the first boot splits the card (or ROCKNIX grows STORAGE)" '[ -f "$S/.please_resize_me" ]'
check "no /storage/.config or .cache yet (fs-resize would refuse to grow)" '[ ! -e "$S/.config" ] && [ ! -e "$S/.cache" ]'
check "drop-in waiting in .pocket-seed is deploy/essway-pocket-ui.conf" \
  'same "$S/.pocket-seed/.config/system.d/essway.service.d/10-pocket-ui.conf" /repo/deploy/essway-pocket-ui.conf'
check "boot screen unit waiting in .pocket-seed is deploy/pocket-splash.service" \
  'same "$S/.pocket-seed/.config/system.d/pocket-splash.service" /repo/deploy/pocket-splash.service'
[ $GAMES = 0 ] || check "the GAMES drive's mount is deploy/image/games-mount.sh, started by the seed's automount drop-in" \
  'same "$S/pocket-ui/games-mount.sh" /repo/deploy/image/games-mount.sh && same "$S/.pocket-seed/.config/system.d/rocknix-automount.service.d/10-pocket-games.conf" /repo/deploy/image/rocknix-automount-games.conf'
# deploy/image/image-defaults.sh with the model's hostname on its NAME line (image_hostname, pins.sh), a valid name that is not
# ROCKNIX's default.
HOST=$(image_hostname "$MODEL")
sed "s/^NAME=pocket-ui\$/NAME=$HOST/" /repo/deploy/image/image-defaults.sh >"$W/lists/image-defaults.expected"
check "first-start settings waiting in .pocket-seed are deploy/image/image-defaults.sh with the hostname $HOST" \
  'same "$S/.pocket-seed/.config/autostart/pocket-ui-image-defaults" "$W/lists/image-defaults.expected" && grep -qx "NAME=$HOST" "$S/.pocket-seed/.config/autostart/pocket-ui-image-defaults" && printf %s "$HOST" | grep -Eq "^[a-z][a-z0-9-]{0,23}\$" && [ "$HOST" != ROCKNIX ]'
check "licence texts in STORAGE: the repository's LICENSE and NOTICE.md, the fonts' OFL, the crates' texts (not empty)" \
  'same "$S/pocket-ui/licences/LICENSE" /repo/LICENSE && same "$S/pocket-ui/licences/NOTICE.md" /repo/NOTICE.md && same "$S/pocket-ui/licences/FONTS-OFL.txt" /repo/assets/fonts/OFL.txt && [ -s "$S/pocket-ui/licences/THIRD-PARTY-CRATES.md" ] && grep -q "^adler2 " "$S/pocket-ui/licences/THIRD-PARTY-LICENSES.txt"'
check "launcher is deploy/run-pocket-ui.sh" 'same "$S/pocket-ui/run-pocket-ui.sh" /repo/deploy/run-pocket-ui.sh'
check "portmaster-setup.sh is deploy/portmaster-setup.sh" 'same "$S/pocket-ui/releases/$V/portmaster-setup.sh" /repo/deploy/portmaster-setup.sh'
for b in pocket-ui pocket-core pocket-splash; do
  check "$b is an aarch64 Linux program" 'file -b "$S/pocket-ui/releases/$V/$b" | grep -q "ELF 64-bit LSB.*ARM aarch64"'
done
echo "-- first start, nothing from a developer device:"
check "no pocket-ui settings (first-start setup runs)" '[ -z "$(find "$S" -path "*/.config/pocket-ui*")" ]'
check "no dev-ssh marker, no SSH keys" '[ -z "$(find "$S" -name dev-ssh -o -name .ssh -o -name "*authorized_keys*" -o -name "ssh_host_*")" ]'
check "no Wi-Fi credentials (iwd, NetworkManager, wpa_supplicant)" \
  '[ -z "$(find "$S" -name "*.psk" -o -name "*.8021x" -o -name "*.nmconnection" -o -name "wpa_supplicant*" -o -path "*/iwd*")" ]'
check "the card has no GAMES drive yet (the first start makes it), and no state of a first start" '[ ! -e "$S/.pocket-games-state" ] && [ ! -e "$S/.pocket-games-tries" ]'
check "no recents, saves or games" '[ -z "$(find "$S" -name "recents*" -o -name roms -o -name "*.srm" -o -name "*.state*")" ]'
check "no BIOS files" '[ -z "$(find "$S" -iname "*bios*")" ] && ! grep -qi bios "$W/lists/fat"'
check "no update left pending (update-pending, previous, dev.env)" \
  '[ ! -e "$S/pocket-ui/update-pending" ] && [ ! -L "$S/pocket-ui/previous" ] && [ ! -e "$S/pocket-ui/dev.env" ]'
echo "STORAGE before the first boot: $(du -sh "$S" | cut -f1) used of 32 MB"
# The programs grow with every release; STORAGE stays 32 MB until the first boot grows it (debugfs fails when full).
warn "STORAGE keeps a quarter free for the programs to grow (under 24 MB used)" '[ "$(du -sk "$S" | cut -f1)" -lt 24576 ]'
# For the --version check outside (scripts/check-image.sh).
[ -d "$S/pocket-ui/releases" ] && cp -r "$S/pocket-ui/releases" "$W/releases"

# The work files back to the user who started the check (CHECK_OWNER, uid:gid): root's directories in a bind mount
# could not be removed without root on a Linux machine.
[ -z "${CHECK_OWNER:-}" ] || chown -R "$CHECK_OWNER" "$W" 2>/dev/null || true

echo
if [ $fails -eq 0 ]; then
  echo "all checks passed ($warns warnings)"
else
  echo "$fails checks failed ($warns warnings)"
  exit 1
fi
