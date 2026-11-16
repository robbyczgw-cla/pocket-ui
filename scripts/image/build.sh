#!/bin/sh
# The container half of scripts/make-image.sh. Runs in pocket-image-tools with the repository at /repo (read-only)
# and the work directory at /w. Writes only to files in /w; never to a block device (there are none in here).
#   build.sh extract              /w/base.img -> /w/KERNEL, /w/miniv2-noufs.dtb
#   build.sh extract-efi          FAMILY=SM8550 MODEL=<a Retroid>: /w/base.img -> /w/KERNEL, /w/<model>-noufs.dtb
#   build.sh strip-system         /w/base.img's SYSTEM -> /w/SYSTEM, repacked without pins.sh SYSTEM_REMOVE
#   build.sh assemble VERSION EPOCH
#       /w/image.img (a copy of base.img) gets the boot files from /w/sd-efi, the initramfs hooks, the notes from
#       /w/info, and pocket-ui from /w/bin on STORAGE. EPOCH (seconds) is the time stamp of every added file.
# FAMILY=H700, SM8550, RK3566 or RK3326 (scripts/make-image-family.sh): that family's pins instead; strip-system works on its SYSTEM,
# and assemble takes MODEL (and no boot files from /w/sd-efi): no GRUB; on the H700 the FAT root gets dtb.img, a copy of
# that model's tree from device_trees/, and on the RK3566 extlinux.conf names that model's tree instead of the X55's. The
# SM8550 Retroids (pins.sh SM8550_EFI_MODELS) are the exception: they take /w/sd-efi as the SM8250 cards do, next to KERNEL.
# On the RK3326 extlinux.conf's commented FDT line names the R36S tree, the FAT gets r36s-panels/ (/w/panels) and STORAGE the
# screen hook; the device-test card (pins.sh is_devtest) also the USB network preset, SSH and /w/devtest/authorized_keys.
set -eu
. /repo/scripts/image/pins.sh
FAMILY=${FAMILY:-SM8250}
if [ "$FAMILY" != SM8250 ]; then
  family_pins "$FAMILY"
  SYSTEM_MD5=$FAM_SYSTEM_MD5
  SYSTEM_REMOVE=$FAM_SYSTEM_REMOVE
  SYSTEM_ZIP_REMOVE=$FAM_SYSTEM_ZIP_REMOVE
  POCKET_SYSTEM_MD5=$FAM_POCKET_SYSTEM_MD5
fi
if [ "$FAMILY" = SM8250 ]; then sm8250_pins "${MODEL:-rp-mini-v2}"; fi
W=/w
die() { echo "build.sh: $*" >&2; exit 1; }
md5() { md5sum "$1" | cut -d' ' -f1; }

extract() {
  FAT="$W/base.img@@$((P1_START * 512))"
  rm -f "$W/KERNEL" "$W/$SM8250_DTB_FILE"
  mcopy -n -i "$FAT" ::/KERNEL "$W/KERNEL"
  [ "$(md5 "$W/KERNEL")" = "$KERNEL_MD5" ] || die "KERNEL md5 $(md5 "$W/KERNEL") is not ROCKNIX $ROCKNIX_TAG's $KERNEL_MD5"
  # The Mini V2 tree among the device trees appended to KERNEL, then the internal UFS switched off in place.
  # fdtput changes only the two properties, so the result is byte for byte the tree that booted on the device.
  python3 -I /repo/scripts/make-test-bootimg.py "$W/KERNEL" /tmp/kernel-test.img "$SM8250_MODEL_NAME" >/dev/null
  [ "$(md5 /tmp/kernel-test.img.orig.dtb)" = "$SM8250_ORIG_MD5" ] || die "original device tree hash mismatch"
  cp /tmp/kernel-test.img.orig.dtb /tmp/noufs.dtb
  fdtput -t s /tmp/noufs.dtb /soc@0/ufshc@1d84000 status disabled
  fdtput -t s /tmp/noufs.dtb /soc@0/phy@1d87000 status disabled
  [ "$(md5 /tmp/noufs.dtb)" = "$DTB_MD5" ] || die "device tree md5 $(md5 /tmp/noufs.dtb), expected $DTB_MD5"
  [ "$(fdtget -t s /tmp/noufs.dtb /soc@0/ufshc@1d84000 status)" = disabled ] &&
    [ "$(fdtget -t s /tmp/noufs.dtb /soc@0/phy@1d87000 status)" = disabled ] || die "internal UFS is not disabled"
  cp /tmp/noufs.dtb "$W/$SM8250_DTB_FILE"
  echo "KERNEL and $SM8250_DTB_FILE extracted, both as expected"
}

# An SM8550 Retroid card (pins.sh SM8550_EFI_MODELS): KERNEL, and the model's tree from it with the internal UFS switched
# off in place, as extract does for the SM8250 cards (only the status properties change).
extract_efi() {
  FAT="$W/base.img@@$((P1_START * 512))"
  sm8550_efi_pins "${MODEL:-}" || die "$MODEL is not in SM8550_EFI_MODELS"
  rm -f "$W/KERNEL" "$W/$EFI_DTB_FILE"
  mcopy -n -i "$FAT" ::/KERNEL "$W/KERNEL"
  [ "$(md5 "$W/KERNEL")" = "$FAM_KERNEL_MD5" ] || die "KERNEL md5 $(md5 "$W/KERNEL") is not ROCKNIX $ROCKNIX_TAG's $FAM_KERNEL_MD5"
  python3 -I /repo/scripts/make-test-bootimg.py "$W/KERNEL" /tmp/kernel-test.img "$EFI_MODEL_NAME" >/dev/null
  [ "$(md5 /tmp/kernel-test.img.orig.dtb)" = "$EFI_ORIG_MD5" ] || die "original device tree of the $EFI_MODEL_NAME: hash mismatch"
  cp /tmp/kernel-test.img.orig.dtb /tmp/noufs.dtb
  for node in $SM8550_UFS_NODES; do fdtput -t s /tmp/noufs.dtb "$node" status disabled; done
  [ "$(md5 /tmp/noufs.dtb)" = "$EFI_DTB_MD5" ] || die "device tree md5 $(md5 /tmp/noufs.dtb), expected $EFI_DTB_MD5"
  for node in $SM8550_UFS_NODES; do
    [ "$(fdtget -t s /tmp/noufs.dtb "$node" status)" = disabled ] || die "internal UFS ($node) is not disabled"
  done
  cp /tmp/noufs.dtb "$W/$EFI_DTB_FILE"
  rm -f /tmp/kernel-test.img*
  echo "KERNEL and $EFI_DTB_FILE extracted, both as expected"
}

# ROCKNIX's SYSTEM without the files in SYSTEM_REMOVE. Unpacked as root (owners, modes, device nodes and times
# stay), the files removed, the directories that held them given back their old time, and packed again with the
# settings ROCKNIX used (unsquashfs -s: zstd level 19, 1 MiB blocks, no xattrs) and its creation time. The listing of the new SYSTEM must be
# the old one minus exactly the removed paths.
strip_system() {
  # Linux ownership, modes and symlinks need the container filesystem, not a
  # macOS bind mount (e.g. usr/cache/shadow cannot be unpacked there).
  SYSROOT=/tmp/pocket-image-sysroot
  FAT="$W/base.img@@$((P1_START * 512))"
  rm -rf "$W/SYSTEM.orig" "$W/SYSTEM" "$SYSROOT"
  mcopy -n -i "$FAT" ::/SYSTEM "$W/SYSTEM.orig"
  [ "$(md5 "$W/SYSTEM.orig")" = "$SYSTEM_MD5" ] || die "SYSTEM md5 is not ROCKNIX $ROCKNIX_TAG's $SYSTEM_MD5"
  # The new SYSTEM keeps ROCKNIX's creation time (superblock bytes 8-11), so its md5 does not depend on our commit.
  MKFS_TIME=$(od -An -t u4 -j 8 -N 4 "$W/SYSTEM.orig" | tr -d ' ')
  unsquashfs -q -n -d "$SYSROOT" "$W/SYSTEM.orig" >/dev/null || die "unsquashfs failed"
  for p in $SYSTEM_REMOVE; do
    [ -e "$SYSROOT/$p" ] || die "$p is not in ROCKNIX's SYSTEM"
    parent=$(dirname "$SYSROOT/$p")
    stamp=$(stat -c %Y "$parent")
    rm -rf "${SYSROOT:?}/$p"
    touch -d "@$stamp" "$parent"
  done
  echo "$SYSTEM_ZIP_REMOVE" | while read -r z glob; do
    [ -n "$z" ] || continue
    [ -f "$SYSROOT/$z" ] || die "$z is not in ROCKNIX's SYSTEM"
    n=$(unzip -Z1 "$SYSROOT/$z" | grep -c "^$(echo "$glob" | sed 's/\./\\./g; s/\*/[^\/]*/g')\$")
    [ "$n" -gt 0 ] || die "$z holds nothing like $glob"
    # zip -d writes a new file and renames it: the archive's and its folder's times are put back afterwards.
    stamp=$(stat -c %Y "$SYSROOT/$z")
    dstamp=$(stat -c %Y "$(dirname "$SYSROOT/$z")")
    zip -q -d "$SYSROOT/$z" "$glob" || die "zip -d $z $glob failed"
    touch -d "@$stamp" "$SYSROOT/$z"
    touch -d "@$dstamp" "$(dirname "$SYSROOT/$z")"
    ! unzip -Z1 "$SYSROOT/$z" | grep -q "^$(echo "$glob" | sed 's/\./\\./g; s/\*/[^\/]*/g')\$" || die "$glob still in $z"
    unzip -tq "$SYSROOT/$z" >/dev/null || die "$z is damaged after zip -d"
    echo "$z: $n entries $glob removed"
  done
  mksquashfs "$SYSROOT" "$W/SYSTEM" -comp zstd -Xcompression-level 19 -b 1048576 -no-xattrs -noappend \
    -mkfs-time "$MKFS_TIME" -processors "${SQUASH_CPUS:-4}" -quiet -no-progress >/dev/null || die "mksquashfs failed"
  rm -rf "$SYSROOT"
  unsquashfs -lln "$W/SYSTEM.orig" | sed -n 's#^\([^ ]*\) \([^ ]*\) .* squashfs-root#\1 \2 #p' | sort >/tmp/list.orig
  unsquashfs -lln "$W/SYSTEM" | sed -n 's#^\([^ ]*\) \([^ ]*\) .* squashfs-root#\1 \2 #p' | sort >/tmp/list.new
  # Everything that went: the removed paths and whatever was below them.
  comm -23 /tmp/list.orig /tmp/list.new | sed 's#^[^ ]* [^ ]* ##' >/tmp/gone
  while read -r g; do
    ok=0
    for p in $SYSTEM_REMOVE; do case $g in "/$p" | "/$p"/*) ok=1 ;; esac; done
    [ $ok = 1 ] || die "repacked SYSTEM lost $g, which is not in SYSTEM_REMOVE"
  done </tmp/gone
  [ -z "$(comm -13 /tmp/list.orig /tmp/list.new)" ] || die "repacked SYSTEM has entries ROCKNIX's has not: $(comm -13 /tmp/list.orig /tmp/list.new | head -3)"
  rm -f "$W/SYSTEM.orig"
  echo "SYSTEM repacked without $(wc -l </tmp/gone) entries: $(stat -c %s "$W/SYSTEM") bytes, md5 $(md5 "$W/SYSTEM")"
}

assemble() {
  VERSION=$1
  EPOCH=$2
  # mtools (new directories) and e2fsprogs (inode and superblock times) take their clock from these, so a
  # rebuild of the same commit gives the same image.
  export SOURCE_DATE_EPOCH=$EPOCH E2FSPROGS_FAKE_TIME=$EPOCH
  IMG=$W/image.img
  FAT="$IMG@@$((P1_START * 512))"
  E=$W/sd-efi
  if [ "$FAMILY" != SM8250 ]; then
    [ -n "${MODEL:-}" ] || die "FAMILY=$FAMILY needs MODEL"
    set -- $(echo "$FAM_MODELS" | awk -v m="$MODEL" '$1 == m { print $2, $3 }')
    [ $# = 2 ] || die "$MODEL is not in ${FAMILY}_MODELS"
    DTB_NAME=$1
    DTB_MD5_WANT=$2
  else
    [ "$(md5 "$E/boot/pocket/Image")" = "$IMAGE_MD5" ] || die "boot/pocket/Image is not the expected kernel"
  fi

  # Everything that goes in, staged here so owners (root), modes and times are exact.
  S=/tmp/stage
  rm -rf "$S"
  mkdir -p "$S/fat/pocket-ui-info" "$S/storage"
  if is_efi_card "$FAMILY" "${MODEL:-}"; then
    # KERNEL stays for ROCKNIX's ABL; GRUB, the Image, the model's tree and the BLS entry for UEFI and OpenABL (as SM8250).
    sm8550_efi_pins "$MODEL"
    [ "$(md5 "$E/boot/pocket/Image")" = "$SM8550_IMAGE_MD5" ] || die "boot/pocket/Image is not the kernel unpacked from KERNEL"
    [ "$(md5 "$E/boot/pocket/$EFI_DTB_FILE")" = "$EFI_DTB_MD5" ] || die "boot/pocket/$EFI_DTB_FILE is not the pinned tree"
    mkdir -p "$S/fat/EFI/BOOT" "$S/fat/boot/grub" "$S/fat/boot/pocket" "$S/fat/loader/entries"
    cp "$E/EFI/BOOT/BOOTAA64.EFI" "$S/fat/EFI/BOOT/"
    cp "$E/boot/grub/grub.cfg" "$E/boot/grub/grub-efi-arm64-bin.version" "$S/fat/boot/grub/"
    cp "$E/boot/pocket/Image" "$E/boot/pocket/$EFI_DTB_FILE" "$E/boot/pocket/KERNEL.source.md5" "$S/fat/boot/pocket/"
    cp "$E/loader/entries/pocket-ui.conf" "$S/fat/loader/entries/"
  elif [ "$FAMILY" = SM8550 ] || [ "$FAMILY" = SM4450 ]; then
    : # KERNEL carries the trees, ROCKNIX's ABL starts it: no boot files to add.
  elif [ "$FAMILY" = RK3566 ] && is_generic RK3566 "$MODEL"; then
    # The Generic card: the Specific card with the Generic image's first 16 MiB (GPT, idbloader, the U-Boot that detects the
    # handheld) and its extlinux.conf (FDTDIR, no FDT line), both unchanged. KERNEL, SYSTEM and the trees are the same bytes in
    # both images (the pins and the check below), so nothing else differs.
    [ -f "$W/base-generic.img" ] || die "$MODEL needs the Generic image at /w/base-generic.img"
    [ "$(head -c $((P1_START * 512)) "$W/base-generic.img" | md5sum | cut -d' ' -f1)" = "$RK3566_GENERIC_LOADER_MD5" ] || die "the Generic image's first 16 MiB are not the pinned ones"
    G="$W/base-generic.img@@$((P1_START * 512))"
    for f in KERNEL SYSTEM; do
      got=$(mcopy -n -i "$G" "::/$f" - | md5sum | cut -d' ' -f1)
      want=$RK3566_KERNEL_MD5
      [ "$f" = SYSTEM ] && want=$RK3566_SYSTEM_MD5
      [ "$got" = "$want" ] || die "the Generic image's $f (md5 $got) is not the Specific image's ($want): it needs its own build"
    done
    [ "$(mdir -/ -b -i "$G" ::/ | sed 's#^::/##' | grep -v '^extlinux/' | sort | md5sum)" = "$(mdir -/ -b -i "$FAT" ::/ | sed 's#^::/##' | grep -v '^extlinux/' | sort | md5sum)" ] \
      || die "the Generic image's FAT partition has other files than the Specific one's"
    for d in $(mdir -b -i "$G" ::/device_trees | sed 's#.*/##'); do
      [ "$(mcopy -n -i "$G" "::/device_trees/$d" - | md5sum)" = "$(mcopy -n -i "$FAT" "::/device_trees/$d" - | md5sum)" ] || die "device_trees/$d differs between the Generic and the Specific image"
    done
    mcopy -n -i "$G" ::/extlinux/extlinux.conf "$W/extlinux.conf"
    [ "$(md5 "$W/extlinux.conf")" = "$RK3566_GENERIC_EXTLINUX_MD5" ] || die "the Generic image's extlinux.conf is not the pinned one"
    grep -qx '  FDTDIR /device_trees' "$W/extlinux.conf" && ! grep -q '^ *FDT ' "$W/extlinux.conf" || die "the Generic extlinux.conf has no FDTDIR line or an FDT line"
    dd if="$W/base-generic.img" of="$IMG" bs=512 count="$P1_START" conv=notrunc status=none
    [ "$(head -c $((P1_START * 512)) "$IMG" | md5sum | cut -d' ' -f1)" = "$RK3566_GENERIC_LOADER_MD5" ] || die "the Generic loader did not land on the image"
  elif [ "$FAMILY" = RK3566 ]; then
    # ROCKNIX's extlinux.conf with its one FDT line (the Powkiddy X55's tree) naming this model's tree; the rest unchanged.
    mcopy -n -i "$FAT" "::/device_trees/$DTB_NAME.dtb" /tmp/tree.dtb
    [ "$(md5 /tmp/tree.dtb)" = "$DTB_MD5_WANT" ] || die "device_trees/$DTB_NAME.dtb is not the expected tree (md5 $DTB_MD5_WANT)"
    mcopy -n -i "$FAT" ::/extlinux/extlinux.conf /tmp/extlinux.orig
    [ "$(md5 /tmp/extlinux.orig)" = "$RK3566_EXTLINUX_MD5" ] || die "extlinux/extlinux.conf is not ROCKNIX's (md5 $RK3566_EXTLINUX_MD5)"
    [ "$(grep -c "^  FDT $RK3566_DEFAULT_FDT\$" /tmp/extlinux.orig)" = 1 ] || die "extlinux.conf has no single FDT $RK3566_DEFAULT_FDT line"
    sed "s#^  FDT $RK3566_DEFAULT_FDT\$#  FDT /device_trees/$DTB_NAME.dtb#" /tmp/extlinux.orig >"$W/extlinux.conf"
    [ "$(diff /tmp/extlinux.orig "$W/extlinux.conf" | grep -c '^[<>]')" = 2 ] || die "extlinux.conf: more than the FDT line changed"
  elif [ "$FAMILY" = RK3326 ]; then
    # ROCKNIX's extlinux.conf (FDTDIR /, the trees in the FAT root) with its commented "# FDT" line made into the model's FDT line,
    # what ROCKNIX's own extlinux.conf.r36s says (boot.scr has no R36S branch). The rest, FDTOVERLAYS /overlays/mipi-panel.dtbo
    # included, unchanged; boot.scr too. No overlay in overlays/: the 2023 screen needs none, the newer ones are in r36s-panels/.
    mcopy -n -i "$FAT" "::/$DTB_NAME.dtb" /tmp/tree.dtb
    [ "$(md5 /tmp/tree.dtb)" = "$DTB_MD5_WANT" ] || die "$DTB_NAME.dtb is not the expected tree (md5 $DTB_MD5_WANT)"
    mcopy -n -i "$FAT" ::/boot.scr /tmp/boot.scr
    [ "$(md5 /tmp/boot.scr)" = "$RK3326_BOOTSCR_MD5" ] || die "boot.scr is not ROCKNIX's (md5 $RK3326_BOOTSCR_MD5)"
    mcopy -n -i "$FAT" ::/extlinux/extlinux.conf /tmp/extlinux.orig
    [ "$(md5 /tmp/extlinux.orig)" = "$RK3326_EXTLINUX_MD5" ] || die "extlinux/extlinux.conf is not ROCKNIX's (md5 $RK3326_EXTLINUX_MD5)"
    [ "$(grep -cxF "$RK3326_FDT_COMMENT" /tmp/extlinux.orig)" = 1 ] && ! grep -q '^ *FDT ' /tmp/extlinux.orig || die "extlinux.conf has no single '$RK3326_FDT_COMMENT' line, or an FDT line"
    awk -v c="$RK3326_FDT_COMMENT" -v n="  FDT /$DTB_NAME.dtb" '$0 == c { print n; next } { print }' /tmp/extlinux.orig >"$W/extlinux.conf"
    [ "$(diff /tmp/extlinux.orig "$W/extlinux.conf" | grep -c '^[<>]')" = 2 ] || die "extlinux.conf: more than the FDT line changed"
    grep -qx '  FDTOVERLAYS /overlays/mipi-panel.dtbo' "$W/extlinux.conf" || die "extlinux.conf lost its FDTOVERLAYS line"
    ! mdir -b -i "$FAT" ::/overlays/mipi-panel.dtbo >/dev/null 2>&1 || die "ROCKNIX's image already has overlays/mipi-panel.dtbo"
    mkdir -p "$S/fat/r36s-panels"
    echo "$RK3326_PANELS" | while read -r p want; do
      [ -n "$p" ] || continue
      [ "$(sha256sum "$W/panels/$p.dtbo" | cut -d' ' -f1)" = "$want" ] || die "r36s-panels/$p.dtbo is not the pinned file"
      cp "$W/panels/$p.dtbo" "$S/fat/r36s-panels/"
    done
    cmp -s "$W/panels/README.txt" /repo/deploy/image/r36s-panels-README.txt || die "r36s-panels/README.txt is not deploy/image/r36s-panels-README.txt"
    cp "$W/panels/README.txt" "$S/fat/r36s-panels/README.txt"
    install -m 755 /repo/deploy/image/r36s-stock-panel.sh "$S/storage/pocket-ui-r36s-panel"
    if is_devtest RK3326 "$MODEL"; then
      [ -s "$W/devtest/authorized_keys" ] || die "$MODEL needs /w/devtest/authorized_keys (make-image-family.sh --authorized-keys)"
      ! grep -q PRIVATE "$W/devtest/authorized_keys" || die "authorized_keys holds a private key"
      install -m 600 "$W/devtest/authorized_keys" "$S/storage/authorized_keys"
      install -m 755 /repo/deploy/image/r36s-devtest.sh "$S/storage/pocket-ui-devtest"
      printf 'USB_MODE=cdc\n' >"$S/storage/usbgadget.conf"
      chmod 644 "$S/storage/usbgadget.conf"
      : >"$S/storage/dev-ssh"
      chmod 644 "$S/storage/dev-ssh"
    fi
  elif [ "$FAMILY" = H700 ]; then
    # The model's tree from ROCKNIX's own device_trees/, as ROCKNIX's install page has the player copy it by hand.
    mcopy -n -i "$FAT" "::/device_trees/$DTB_NAME.dtb" "$S/fat/dtb.img"
    [ "$(md5 "$S/fat/dtb.img")" = "$DTB_MD5_WANT" ] || die "device_trees/$DTB_NAME.dtb is not the expected tree (md5 $DTB_MD5_WANT)"
    if is_ddr3 H700 "$MODEL"; then
      # A DDR3 unit: the DDR4 card with the DDR3 image's first 16 MiB (partition table, U-Boot with the DDR3 RAM set-up) and its
      # extlinux.conf (one more line, FDTOVERLAYS). KERNEL and SYSTEM are the same bytes in both images (the pins above).
      [ -f "$W/base-ddr3.img" ] || die "$MODEL needs the DDR3 image at /w/base-ddr3.img"
      [ "$(head -c $((P1_START * 512)) "$W/base-ddr3.img" | md5sum | cut -d' ' -f1)" = "$H700_DDR3_LOADER_MD5" ] || die "the DDR3 image's first 16 MiB are not the pinned ones"
      D3="$W/base-ddr3.img@@$((P1_START * 512))"
      for f in KERNEL SYSTEM; do
        got=$(mcopy -n -i "$D3" "::/$f" - | md5sum | cut -d' ' -f1)
        want=$H700_KERNEL_MD5
        [ "$f" = SYSTEM ] && want=$H700_SYSTEM_MD5
        [ "$got" = "$want" ] || die "the DDR3 image's $f (md5 $got) is not the DDR4 image's ($want): it needs its own build"
      done
      mcopy -n -i "$D3" ::/extlinux/extlinux.conf "$W/extlinux.conf"
      [ "$(md5 "$W/extlinux.conf")" = "$H700_DDR3_EXTLINUX_MD5" ] || die "the DDR3 image's extlinux.conf is not the pinned one"
      dd if="$W/base-ddr3.img" of="$IMG" bs=512 count="$P1_START" conv=notrunc status=none
      [ "$(head -c $((P1_START * 512)) "$IMG" | md5sum | cut -d' ' -f1)" = "$H700_DDR3_LOADER_MD5" ] || die "the DDR3 loader did not land on the image"
    fi
  else
    mkdir -p "$S/fat/EFI/BOOT" "$S/fat/boot/grub" "$S/fat/boot/pocket" "$S/fat/loader/entries"
    cp "$E/EFI/BOOT/BOOTAA64.EFI" "$S/fat/EFI/BOOT/"
    cp "$E/boot/grub/grub.cfg" "$E/boot/grub/grub-efi-arm64-bin.version" "$S/fat/boot/grub/"
    cp "$E/boot/pocket/Image" "$E/boot/pocket/$SM8250_DTB_FILE" "$E/boot/pocket/KERNEL.source.md5" "$S/fat/boot/pocket/"
    # OpenABL's Boot Loader Specification entry (no extlinux.conf or boot.scr: Retroid's u-boot would take those before GRUB).
    cp "$E/loader/entries/pocket-ui.conf" "$S/fat/loader/entries/"
  fi
  cp /repo/deploy/image/post-flash.sh /repo/deploy/image/mount-storage.sh "$S/fat/"
  # The GAMES drive (docs/SD-IMAGE.md, "The GAMES drive"), only on the models pins.sh names: the first start's script, and at
  # every start the drive's mount. All other cards have neither: ROCKNIX's fs-resize, STORAGE over the whole card.
  # Every card: an ArkOS/dArkOS/R36S stock games card in the second slot is used as it is (docs/SD-IMAGE.md, the hooks).
  install -m 755 /repo/deploy/image/games-card.sh "$S/storage/games-card.sh"
  install -m 644 /repo/deploy/image/rocknix-automount-games-card.conf "$S/storage/rocknix-automount-games-card.conf"
  if has_games_drive "$FAMILY" "${MODEL:-}"; then
    cp /repo/deploy/image/games-first-start.sh "$S/fat/"
    install -m 755 /repo/deploy/image/games-mount.sh "$S/storage/games-mount.sh"
    install -m 644 /repo/deploy/image/rocknix-automount-games.conf "$S/storage/rocknix-automount-games.conf"
  fi
  # The notes, the source offer, pocket-ui's LICENSE and NOTICE.md, licences/ (crates, fonts), rocknix-licenses/
  # (ROCKNIX's LICENSE.md and licenses folder at the tag) and firmware-licences/ (the firmware's terms).
  cp -r "$W/info/." "$S/fat/pocket-ui-info/"
  # The crates' and fonts' texts live once on the card, in pocket-ui-info/licences on the FAT partition (readable on any computer).
  # /storage/pocket-ui/licences, where Settings > About points, is a link to them: a second 1.4 MB copy no longer fits into the
  # 32 MiB STORAGE next to the programs (0.3.1 was 27 blocks short, scripts/image/storage-budget.sh). /flash is the FAT partition.
  for f in pocket-ui pocket-core pocket-splash; do install -m 755 "$W/bin/$f" "$S/storage/$f"; done
  install -m 755 /repo/deploy/portmaster-setup.sh "$S/storage/portmaster-setup.sh"
  install -m 755 /repo/deploy/run-pocket-ui.sh "$S/storage/run-pocket-ui.sh"
  install -m 644 /repo/deploy/essway-pocket-ui.conf "$S/storage/10-pocket-ui.conf"
  install -m 644 /repo/deploy/pocket-splash.service "$S/storage/pocket-splash.service"
  # The model's hostname on the NAME line (see the file); the checks expect exactly this.
  HOST=$(image_hostname "${MODEL:-}")
  sed "s/^NAME=pocket-ui\$/NAME=$HOST/" /repo/deploy/image/image-defaults.sh >"$S/storage/pocket-ui-image-defaults"
  chmod 755 "$S/storage/pocket-ui-image-defaults"
  grep -qx "NAME=$HOST" "$S/storage/pocket-ui-image-defaults" || die "image-defaults.sh has no NAME=pocket-ui line to give $HOST"
  chown -R 0:0 "$S"
  find "$S" -exec touch -h -d "@$EPOCH" {} +

  echo "== FAT partition"
  # rocknix_abl/ holds ROCKNIX's replacement for Retroid's bootloader (abl) and scripts that flash it. This boot
  # path never uses it, and flashing it is what this setup avoids, so it is not on the card.
  case $FAMILY in H700 | RK3566 | RK3326) ;; SM4450)
    mdeltree -i "$FAT" ::/openabl
    # This private beta is for the four-button Classic Rev Q only.
    for tree in $(mdir -b -i "$FAT" ::/dtbs/ | sed 's#^.*/##'); do
      [ "$tree" = "$DTB_NAME.dtb" ] || mdel -i "$FAT" "::/dtbs/$tree"
    done
    ;; *) mdeltree -i "$FAT" ::/rocknix_abl ;; esac
  # SYSTEM without the files in pins.sh SYSTEM_REMOVE (strip-system), and its .md5 in ROCKNIX's format.
  # A new SYSTEM_REMOVE (or a new mksquashfs) gives a new md5: then pins.sh is "unset" for one build, which prints it.
  sysmd5=$(md5 "$W/SYSTEM")
  if [ "$POCKET_SYSTEM_MD5" = unset ]; then
    echo "WARNING: POCKET_SYSTEM_MD5 is unset; this SYSTEM's md5 is $sysmd5 (pin it in pins.sh and build again)" >&2
  elif [ "$sysmd5" != "$POCKET_SYSTEM_MD5" ]; then
    die "SYSTEM md5 $sysmd5 is not POCKET_SYSTEM_MD5 ($POCKET_SYSTEM_MD5)"
  fi
  printf '%s  target/SYSTEM\n' "$sysmd5" >"$S/SYSTEM.md5"
  touch -d "@$EPOCH" "$S/SYSTEM.md5" "$W/SYSTEM"
  mdel -i "$FAT" ::/SYSTEM ::/SYSTEM.md5
  mcopy -m -i "$FAT" "$W/SYSTEM" ::/SYSTEM
  mcopy -m -i "$FAT" "$S/SYSTEM.md5" ::/SYSTEM.md5
  if [ "$FAMILY" = RK3566 ] || [ "$FAMILY" = RK3326 ] || { [ "$FAMILY" = H700 ] && is_ddr3 H700 "$MODEL"; }; then
    touch -d "@$EPOCH" "$W/extlinux.conf"
    mdel -i "$FAT" ::/extlinux/extlinux.conf
    mcopy -m -i "$FAT" "$W/extlinux.conf" ::/extlinux/extlinux.conf
  fi
  (cd "$S/fat" && find . -mindepth 1 -type d | sed 's#^\./##' | sort) | while read -r d; do
    mmd -i "$FAT" "::/$d"
  done
  (cd "$S/fat" && find . -type f | sed 's#^\./##' | sort) | while read -r f; do
    mcopy -m -i "$FAT" "$S/fat/$f" "::/$f"
  done
  mdir -/ -b -i "$FAT" ::/ | sed 's#^::/#  #'
  if is_efi_card "$FAMILY" "${MODEL:-}"; then
    # As on the Pocket 5: Retroid's "UEFI" is reported to start only from an EFI system partition. Offsets, names and
    # LegacyBIOSBootable stay.
    sfdisk -q --part-type "$IMG" 1 "$SM8250_ESP_TYPE" || die "could not mark p1 as EFI system partition"
    [ "$(sfdisk --part-type "$IMG" 1)" = "$SM8250_ESP_TYPE" ] || die "p1 is not an EFI system partition"
    echo "p1 marked as EFI system partition"
  fi
  if [ "$FAMILY" = SM8250 ] && [ "$(sm8250_p1_type "${MODEL:-}")" != "$SM8250_DATA_TYPE" ]; then
    # Only p1's type GUID changes (sfdisk rewrites both GPT copies); offsets, names and LegacyBIOSBootable stay.
    sfdisk -q --part-type "$IMG" 1 "$(sm8250_p1_type "$MODEL")" || die "could not mark p1 as EFI system partition"
    [ "$(sfdisk --part-type "$IMG" 1)" = "$SM8250_ESP_TYPE" ] || die "p1 is not an EFI system partition"
    echo "p1 marked as EFI system partition"
  fi
  # The clusters of ROCKNIX's original SYSTEM are free now but still hold its bytes, with the files SYSTEM_REMOVE
  # leaves out: zero every free cluster (scripts/image/fat-free-zero.py), then let fsck.vfat look at the result.
  python3 -I /repo/scripts/image/fat-free-zero.py "$IMG" "$((P1_START * 512))" || die "zeroing the free FAT clusters failed"
  dd if="$IMG" of=/tmp/p1.fat bs=512 skip="$P1_START" count="$P1_SIZE" status=none
  fsck.vfat -n /tmp/p1.fat >/tmp/fsck.log 2>&1 || { cat /tmp/fsck.log >&2; die "fsck.vfat found errors on the FAT partition"; }
  rm -f /tmp/p1.fat

  echo "== STORAGE partition"
  P2=$W/p2.ext4
  dd if="$IMG" of="$P2" bs=512 skip="$P2_START" count="$P2_ROCKNIX_SIZE" status=none
  [ "$(e2label "$P2")" = STORAGE ] || die "partition 2 is not STORAGE"
  [ -n "$(debugfs -R 'stat /.please_resize_me' "$P2" 2>/dev/null)" ] || die "STORAGE has no .please_resize_me"
  # ROCKNIX's 32 MiB STORAGE with the room it leaves (pins.sh P2_ROCKNIX_FREE_BLOCKS), then grown to P2_SIZE (128 MiB) before
  # anything is written: the programs have room for many releases. The file system is grown here, offline, with everything
  # still in it clean; the partition table and the image follow when it is written back (grow_image below).
  [ "$(dumpe2fs -h "$P2" 2>/dev/null | sed -n 's/^Free blocks: *//p')" = "$P2_ROCKNIX_FREE_BLOCKS" ] ||
    die "ROCKNIX's STORAGE has not $P2_ROCKNIX_FREE_BLOCKS free blocks (pins.sh P2_ROCKNIX_FREE_BLOCKS)"
  e2fsck -fn "$P2" >/dev/null 2>&1 || die "ROCKNIX's STORAGE is not clean before it is grown"
  truncate -s $((P2_SIZE * 512)) "$P2"
  resize2fs "$P2" >/tmp/resize2fs.log 2>&1 || { cat /tmp/resize2fs.log >&2; die "resize2fs could not grow STORAGE to $P2_SIZE sectors"; }
  e2fsck -fn "$P2" >/dev/null 2>&1 || die "STORAGE is not clean after it is grown"
  [ "$(dumpe2fs -h "$P2" 2>/dev/null | sed -n 's/^Free blocks: *//p')" = "$P2_FREE_BLOCKS" ] ||
    die "the grown STORAGE has $(dumpe2fs -h "$P2" 2>/dev/null | sed -n 's/^Free blocks: *//p') free blocks, not $P2_FREE_BLOCKS (pins.sh P2_FREE_BLOCKS): work out the new room, storage-budget.sh"
  R=pocket-ui/releases/$VERSION
  UNITS=.pocket-seed/.config/system.d
  SEED=$UNITS/essway.service.d
  GAMES_UNIT=$UNITS/rocknix-automount.service.d
  {
    echo "mkdir /pocket-ui"
    echo "mkdir /pocket-ui/releases"
    echo "mkdir /$R"
    for f in pocket-ui pocket-core pocket-splash portmaster-setup.sh; do
      echo "write $S/storage/$f /$R/$f"
    done
    [ ! -d "$W/bin/cores" ] || sh /repo/scripts/image/gba-debugfs.sh "$W/bin/cores" "$R" "$EPOCH"
    echo "write $S/storage/run-pocket-ui.sh /pocket-ui/run-pocket-ui.sh"
    # The layout scripts/dev.sh and the updater use (docs/UPDATES.md): fixed paths through current.
    echo "symlink /pocket-ui/current releases/$VERSION"
    # Licence texts of the crates and the fonts (scripts/licence-files.sh), read from Settings > About.
    echo "symlink /pocket-ui/licences /flash/pocket-ui-info/licences"
    for f in pocket-ui pocket-core portmaster-setup.sh; do
      echo "symlink /pocket-ui/$f current/$f"
    done
    # Not .config itself: ROCKNIX would refuse to grow STORAGE (deploy/image/mount-storage.sh).
    echo "mkdir /.pocket-seed"
    echo "mkdir /.pocket-seed/.config"
    echo "mkdir /.pocket-seed/.config/system.d"
    echo "mkdir /$SEED"
    echo "write $S/storage/10-pocket-ui.conf /$SEED/10-pocket-ui.conf"
    # The boot screen (docs/BOOT-SPLASH.md), put in place with the drop-in at the second boot.
    echo "write $S/storage/pocket-splash.service /$UNITS/pocket-splash.service"
    echo "mkdir /$UNITS/sysinit.target.wants"
    echo "symlink /$UNITS/sysinit.target.wants/pocket-splash.service ../pocket-splash.service"
    # ROCKNIX settings that differ on this card (updates off, hostname), run once by ROCKNIX's autostart.
    echo "mkdir /.pocket-seed/.config/autostart"
    echo "write $S/storage/pocket-ui-image-defaults /.pocket-seed/.config/autostart/pocket-ui-image-defaults"
    # ROCKNIX's automount sees an ArkOS card's own layout first (deploy/image/games-card.sh), on every card.
    echo "write $S/storage/games-card.sh /pocket-ui/games-card.sh"
    echo "mkdir /$GAMES_UNIT"
    echo "write $S/storage/rocknix-automount-games-card.conf /$GAMES_UNIT/20-pocket-games-card.conf"
    GAMES_PATHS="/pocket-ui/games-card.sh /$GAMES_UNIT /$GAMES_UNIT/20-pocket-games-card.conf"
    if has_games_drive "$FAMILY" "${MODEL:-}"; then
      echo "write $S/storage/games-mount.sh /pocket-ui/games-mount.sh"
      echo "write $S/storage/rocknix-automount-games.conf /$GAMES_UNIT/10-pocket-games.conf"
      GAMES_PATHS="$GAMES_PATHS /pocket-ui/games-mount.sh /$GAMES_UNIT/10-pocket-games.conf"
    fi
    # RK3326: the screen hook (both cards); the device-test card's USB network preset, key, dev-ssh marker and SSH script.
    if [ "$FAMILY" = RK3326 ]; then
      echo "write $S/storage/pocket-ui-r36s-panel /.pocket-seed/.config/autostart/pocket-ui-r36s-panel"
      GAMES_PATHS="$GAMES_PATHS /.pocket-seed/.config/autostart/pocket-ui-r36s-panel"
    fi
    if [ "$FAMILY" = RK3326 ] && is_devtest RK3326 "$MODEL"; then
      echo "write $S/storage/pocket-ui-devtest /.pocket-seed/.config/autostart/pocket-ui-devtest"
      echo "mkdir /.pocket-seed/.cache"
      echo "mkdir /.pocket-seed/.cache/usbgadget"
      echo "write $S/storage/usbgadget.conf /.pocket-seed/.cache/usbgadget/usbgadget.conf"
      echo "mkdir /.pocket-seed/.ssh"
      echo "set_inode_field /.pocket-seed/.ssh mode 040700"
      echo "write $S/storage/authorized_keys /.pocket-seed/.ssh/authorized_keys"
      echo "mkdir /.pocket-seed/.config/pocket-ui"
      echo "write $S/storage/dev-ssh /.pocket-seed/.config/pocket-ui/dev-ssh"
      GAMES_PATHS="$GAMES_PATHS /.pocket-seed/.config/autostart/pocket-ui-devtest /.pocket-seed/.cache /.pocket-seed/.cache/usbgadget
        /.pocket-seed/.cache/usbgadget/usbgadget.conf /.pocket-seed/.ssh /.pocket-seed/.ssh/authorized_keys /.pocket-seed/.config/pocket-ui
        /.pocket-seed/.config/pocket-ui/dev-ssh"
    fi
    for p in / /pocket-ui /pocket-ui/releases "/$R" "/$R/pocket-ui" "/$R/pocket-core" "/$R/pocket-splash" "/$R/portmaster-setup.sh" \
      /pocket-ui/run-pocket-ui.sh /pocket-ui/current /pocket-ui/pocket-ui /pocket-ui/pocket-core \
      /pocket-ui/portmaster-setup.sh /pocket-ui/licences /.pocket-seed /.pocket-seed/.config /.pocket-seed/.config/system.d "/$SEED" \
      "/$SEED/10-pocket-ui.conf" "/$UNITS/pocket-splash.service" "/$UNITS/sysinit.target.wants" \
      "/$UNITS/sysinit.target.wants/pocket-splash.service" /.pocket-seed/.config/autostart \
      /.pocket-seed/.config/autostart/pocket-ui-image-defaults $GAMES_PATHS; do
      for field in uid gid; do echo "set_inode_field $p $field 0"; done
      for field in atime mtime ctime crtime; do echo "set_inode_field $p $field @$EPOCH"; done
    done
  } >/tmp/debugfs.cmds
  debugfs -w -f /tmp/debugfs.cmds "$P2" >/tmp/debugfs.log 2>&1
  # -f echoes every command as "debugfs: ..."; write adds "Allocated inode: N". Anything else is an error.
  if grep -v -e '^debugfs[ :]' -e '^Allocated inode: [0-9]*$' /tmp/debugfs.log | grep -q .; then
    cat /tmp/debugfs.log >&2
    die "debugfs reported errors"
  fi
  # No resize_inode. It only serves growing a mounted file system, which nothing here does, and with it resize2fs breaks
  # STORAGE at the first boot on a card over about 36 GB once more than about 18 MB of the 32 MB is in use (resize2fs 1.47:
  # "Illegal doubly indirect block", and it has already moved the root directory). scripts/image/grow-check.sh.
  tune2fs -O ^resize_inode "$P2" >/dev/null 2>&1 || die "tune2fs could not take resize_inode off STORAGE"
  e2fsck -fy "$P2" >/dev/null 2>&1 || [ $? = 1 ] || die "e2fsck failed after taking resize_inode off STORAGE"
  e2fsck -fn "$P2" >/dev/null || die "STORAGE file system not clean after writing"
  grow_image
  dd if="$P2" of="$IMG" bs=512 seek="$P2_START" conv=notrunc status=none
  rm -f "$P2"
  echo "  pocket-ui/ ($R, games-card.sh), .pocket-seed/.config/: system.d/essway.service.d/10-pocket-ui.conf, system.d/pocket-splash.service, system.d/rocknix-automount.service.d/20-pocket-games-card.conf, autostart/pocket-ui-image-defaults"
}

# p2 gets P2_SIZE sectors instead of ROCKNIX's P2_ROCKNIX_SIZE and the image grows by the difference; the tail after p2 (room for
# the second GPT) and everything else in the table stay as shipped (scripts/image/p2-table.sh): on MBR (H700) only p2's size field
# is patched, on GPT sfdisk writes the table again (both copies, the second at the new end; no signature wiping) and the boot code
# of sector 0 is put back byte for byte.
. /repo/scripts/image/p2-table.sh
grow_image() {
  [ "$(stat -c %s "$IMG")" = "$IMG_ROCKNIX_BYTES" ] || die "$IMG is not ROCKNIX's $IMG_ROCKNIX_BYTES bytes long"
  sfdisk -d "$IMG" >"$W/table.before" 2>/dev/null || die "no partition table to read"
  truncate -s "$IMG_BYTES" "$IMG"
  if grep -qx 'label: dos' "$W/table.before"; then
    [ "$(mbr_p2_size "$IMG")" = "$P2_ROCKNIX_SIZE" ] || die "sector 0's second table entry is not a $P2_ROCKNIX_SIZE-sector p2"
    [ "$(mbr_p2_size "$IMG" "$P2_SIZE")" = "$P2_SIZE" ] || die "p2's size did not land in sector 0"
    changed=2
  else
    dd if="$IMG" of="$W/sector0.before" bs=446 count=1 status=none
    sed -E -e "s/^(last-lba: )[0-9]+/\\1$((IMG_BYTES / 512 - 34))/" \
           -e "s/^($(printf '%s' "$IMG" | sed 's/[.\/]/\\&/g')2 *: *start= *[0-9]+, size= *)[0-9]+/\\1$P2_SIZE/" "$W/table.before" >"$W/table.after"
    grep -q "size= *$P2_SIZE," "$W/table.after" || die "the partition table dump has no p2 line to change"
    sfdisk -q --no-reread --no-tell-kernel --wipe never --wipe-partitions never "$IMG" <"$W/table.after" || die "sfdisk could not write the grown partition table"
    dd if="$W/sector0.before" of="$IMG" bs=446 count=1 conv=notrunc status=none
    changed=4
  fi
  sfdisk -d "$IMG" >"$W/table.now" 2>/dev/null || die "the grown partition table can't be read"
  # Only p2's size (and on GPT the last usable sector) may differ from what ROCKNIX shipped.
  [ "$(diff "$W/table.before" "$W/table.now" | grep -c '^[<>]')" = "$changed" ] ||
    { diff "$W/table.before" "$W/table.now" >&2; die "the grown partition table differs in more than p2's size"; }
  rm -f "$W/table.before" "$W/table.after" "$W/table.now" "$W/sector0.before"
}

case ${1:-} in
  extract) extract ;;
  extract-efi) extract_efi ;;
  strip-system) shift; strip_system "$@" ;;
  assemble) shift; assemble "$@" ;;
  *) die "usage: build.sh extract | extract-efi | strip-system | assemble VERSION EPOCH" ;;
esac
