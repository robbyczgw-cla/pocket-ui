#!/bin/sh
# The container half of scripts/make-image.sh. Runs in pocket-image-tools with the repository at /repo (read-only)
# and the work directory at /w. Writes only to files in /w; never to a block device (there are none in here).
#   build.sh extract              /w/base.img -> /w/KERNEL, /w/miniv2-noufs.dtb
#   build.sh strip-system         /w/base.img's SYSTEM -> /w/SYSTEM, repacked without pins.sh SYSTEM_REMOVE
#   build.sh assemble VERSION EPOCH
#       /w/image.img (a copy of base.img) gets the boot files from /w/sd-efi, the initramfs hooks, the notes from
#       /w/info, and pocket-ui from /w/bin on STORAGE. EPOCH (seconds) is the time stamp of every added file.
# FAMILY=H700 (scripts/make-image-h700.sh): the H700_ pins instead; strip-system works on the H700 SYSTEM, and
# assemble takes MODEL (and no boot files from /w/sd-efi): the FAT root gets dtb.img, a copy of that model's tree
# from device_trees/, and no GRUB.
set -eu
. /repo/scripts/image/pins.sh
FAMILY=${FAMILY:-SM8250}
if [ "$FAMILY" = H700 ]; then
  SYSTEM_MD5=$H700_SYSTEM_MD5
  SYSTEM_REMOVE=$H700_SYSTEM_REMOVE
  SYSTEM_ZIP_REMOVE=$H700_SYSTEM_ZIP_REMOVE
  POCKET_SYSTEM_MD5=$H700_POCKET_SYSTEM_MD5
fi
W=/w
die() { echo "build.sh: $*" >&2; exit 1; }
md5() { md5sum "$1" | cut -d' ' -f1; }

extract() {
  FAT="$W/base.img@@$((P1_START * 512))"
  rm -f "$W/KERNEL" "$W/miniv2-noufs.dtb"
  mcopy -n -i "$FAT" ::/KERNEL "$W/KERNEL"
  [ "$(md5 "$W/KERNEL")" = "$KERNEL_MD5" ] || die "KERNEL md5 $(md5 "$W/KERNEL") is not ROCKNIX $ROCKNIX_TAG's $KERNEL_MD5"
  # The Mini V2 tree among the device trees appended to KERNEL, then the internal UFS switched off in place.
  # fdtput changes only the two properties, so the result is byte for byte the tree that booted on the device.
  python3 -I /repo/scripts/make-test-bootimg.py "$W/KERNEL" /tmp/kernel-test.img "Retroid Pocket Mini V2" >/dev/null
  cp /tmp/kernel-test.img.orig.dtb /tmp/noufs.dtb
  fdtput -t s /tmp/noufs.dtb /soc@0/ufshc@1d84000 status disabled
  fdtput -t s /tmp/noufs.dtb /soc@0/phy@1d87000 status disabled
  [ "$(md5 /tmp/noufs.dtb)" = "$DTB_MD5" ] || die "device tree md5 $(md5 /tmp/noufs.dtb), expected $DTB_MD5"
  cp /tmp/noufs.dtb "$W/miniv2-noufs.dtb"
  echo "KERNEL and miniv2-noufs.dtb extracted, both as expected"
}

# ROCKNIX's SYSTEM without the files in SYSTEM_REMOVE. Unpacked as root (owners, modes, device nodes and times
# stay), the files removed, the directories that held them given back their old time, and packed again with the
# settings ROCKNIX used (unsquashfs -s: zstd level 19, 1 MiB blocks, no xattrs) and its creation time. The listing of the new SYSTEM must be
# the old one minus exactly the removed paths.
strip_system() {
  FAT="$W/base.img@@$((P1_START * 512))"
  rm -rf "$W/SYSTEM.orig" "$W/SYSTEM" "$W/sysroot"
  mcopy -n -i "$FAT" ::/SYSTEM "$W/SYSTEM.orig"
  [ "$(md5 "$W/SYSTEM.orig")" = "$SYSTEM_MD5" ] || die "SYSTEM md5 is not ROCKNIX $ROCKNIX_TAG's $SYSTEM_MD5"
  # The new SYSTEM keeps ROCKNIX's creation time (superblock bytes 8-11), so its md5 does not depend on our commit.
  MKFS_TIME=$(od -An -t u4 -j 8 -N 4 "$W/SYSTEM.orig" | tr -d ' ')
  unsquashfs -q -n -d "$W/sysroot" "$W/SYSTEM.orig" >/dev/null || die "unsquashfs failed"
  for p in $SYSTEM_REMOVE; do
    [ -e "$W/sysroot/$p" ] || die "$p is not in ROCKNIX's SYSTEM"
    parent=$(dirname "$W/sysroot/$p")
    stamp=$(stat -c %Y "$parent")
    rm -rf "${W:?}/sysroot/$p"
    touch -d "@$stamp" "$parent"
  done
  echo "$SYSTEM_ZIP_REMOVE" | while read -r z glob; do
    [ -n "$z" ] || continue
    [ -f "$W/sysroot/$z" ] || die "$z is not in ROCKNIX's SYSTEM"
    n=$(unzip -Z1 "$W/sysroot/$z" | grep -c "^$(echo "$glob" | sed 's/\./\\./g; s/\*/[^\/]*/g')\$")
    [ "$n" -gt 0 ] || die "$z holds nothing like $glob"
    # zip -d writes a new file and renames it: the archive's and its folder's times are put back afterwards.
    stamp=$(stat -c %Y "$W/sysroot/$z")
    dstamp=$(stat -c %Y "$(dirname "$W/sysroot/$z")")
    zip -q -d "$W/sysroot/$z" "$glob" || die "zip -d $z $glob failed"
    touch -d "@$stamp" "$W/sysroot/$z"
    touch -d "@$dstamp" "$(dirname "$W/sysroot/$z")"
    ! unzip -Z1 "$W/sysroot/$z" | grep -q "^$(echo "$glob" | sed 's/\./\\./g; s/\*/[^\/]*/g')\$" || die "$glob still in $z"
    unzip -tq "$W/sysroot/$z" >/dev/null || die "$z is damaged after zip -d"
    echo "$z: $n entries $glob removed"
  done
  mksquashfs "$W/sysroot" "$W/SYSTEM" -comp zstd -Xcompression-level 19 -b 1048576 -no-xattrs -noappend \
    -mkfs-time "$MKFS_TIME" -processors "${SQUASH_CPUS:-4}" -quiet -no-progress >/dev/null || die "mksquashfs failed"
  rm -rf "$W/sysroot"
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
  if [ "$FAMILY" = H700 ]; then
    [ -n "${MODEL:-}" ] || die "FAMILY=H700 needs MODEL"
    set -- $(echo "$H700_MODELS" | awk -v m="$MODEL" '$1 == m { print $2, $3 }')
    [ $# = 2 ] || die "$MODEL is not in H700_MODELS"
    DTB_NAME=$1
    DTB_MD5_WANT=$2
  else
    [ "$(md5 "$E/boot/pocket/Image")" = "$IMAGE_MD5" ] || die "boot/pocket/Image is not the expected kernel"
  fi

  # Everything that goes in, staged here so owners (root), modes and times are exact.
  S=/tmp/stage
  rm -rf "$S"
  mkdir -p "$S/fat/pocket-ui-info" "$S/storage/licences"
  if [ "$FAMILY" = H700 ]; then
    # The model's tree from ROCKNIX's own device_trees/, as ROCKNIX's install page has the player copy it by hand.
    mcopy -n -i "$FAT" "::/device_trees/$DTB_NAME.dtb" "$S/fat/dtb.img"
    [ "$(md5 "$S/fat/dtb.img")" = "$DTB_MD5_WANT" ] || die "device_trees/$DTB_NAME.dtb is not the expected tree (md5 $DTB_MD5_WANT)"
  else
    mkdir -p "$S/fat/EFI/BOOT" "$S/fat/boot/grub" "$S/fat/boot/pocket"
    cp "$E/EFI/BOOT/BOOTAA64.EFI" "$S/fat/EFI/BOOT/"
    cp "$E/boot/grub/grub.cfg" "$E/boot/grub/grub-efi-arm64-bin.version" "$S/fat/boot/grub/"
    cp "$E/boot/pocket/Image" "$E/boot/pocket/miniv2-noufs.dtb" "$E/boot/pocket/KERNEL.source.md5" "$S/fat/boot/pocket/"
  fi
  cp /repo/deploy/image/post-flash.sh /repo/deploy/image/mount-storage.sh "$S/fat/"
  # The notes, the source offer, pocket-ui's LICENSE and NOTICE.md, licences/ (crates, fonts), rocknix-licenses/
  # (ROCKNIX's LICENSE.md and licenses folder at the tag) and firmware-licences/ (the firmware's terms).
  cp -r "$W/info/." "$S/fat/pocket-ui-info/"
  # The crates' and fonts' texts also in STORAGE (/storage/pocket-ui/licences, where Settings > About points).
  cp "$W/info/licences/"* "$S/storage/licences/"
  for f in pocket-ui pocket-core pocket-splash; do install -m 755 "$W/bin/$f" "$S/storage/$f"; done
  install -m 755 /repo/deploy/portmaster-setup.sh "$S/storage/portmaster-setup.sh"
  install -m 755 /repo/deploy/run-pocket-ui.sh "$S/storage/run-pocket-ui.sh"
  install -m 644 /repo/deploy/essway-pocket-ui.conf "$S/storage/10-pocket-ui.conf"
  install -m 644 /repo/deploy/pocket-splash.service "$S/storage/pocket-splash.service"
  install -m 755 /repo/deploy/image/image-defaults.sh "$S/storage/pocket-ui-image-defaults"
  chown -R 0:0 "$S"
  find "$S" -exec touch -h -d "@$EPOCH" {} +

  echo "== FAT partition"
  # rocknix_abl/ holds ROCKNIX's replacement for Retroid's bootloader (abl) and scripts that flash it. This boot
  # path never uses it, and flashing it is what this setup avoids, so it is not on the card.
  [ "$FAMILY" = H700 ] || mdeltree -i "$FAT" ::/rocknix_abl
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
  (cd "$S/fat" && find . -mindepth 1 -type d | sed 's#^\./##' | sort) | while read -r d; do
    mmd -i "$FAT" "::/$d"
  done
  (cd "$S/fat" && find . -type f | sed 's#^\./##' | sort) | while read -r f; do
    mcopy -m -i "$FAT" "$S/fat/$f" "::/$f"
  done
  mdir -/ -b -i "$FAT" ::/ | sed 's#^::/#  #'
  # The clusters of ROCKNIX's original SYSTEM are free now but still hold its bytes, with the files SYSTEM_REMOVE
  # leaves out: zero every free cluster (scripts/image/fat-free-zero.py), then let fsck.vfat look at the result.
  python3 -I /repo/scripts/image/fat-free-zero.py "$IMG" "$((P1_START * 512))" || die "zeroing the free FAT clusters failed"
  dd if="$IMG" of=/tmp/p1.fat bs=512 skip="$P1_START" count="$P1_SIZE" status=none
  fsck.vfat -n /tmp/p1.fat >/tmp/fsck.log 2>&1 || { cat /tmp/fsck.log >&2; die "fsck.vfat found errors on the FAT partition"; }
  rm -f /tmp/p1.fat

  echo "== STORAGE partition"
  P2=$W/p2.ext4
  dd if="$IMG" of="$P2" bs=512 skip="$P2_START" count="$P2_SIZE" status=none
  [ "$(e2label "$P2")" = STORAGE ] || die "partition 2 is not STORAGE"
  [ -n "$(debugfs -R 'stat /.please_resize_me' "$P2" 2>/dev/null)" ] || die "STORAGE has no .please_resize_me"
  R=pocket-ui/releases/$VERSION
  UNITS=.pocket-seed/.config/system.d
  SEED=$UNITS/essway.service.d
  {
    echo "mkdir /pocket-ui"
    echo "mkdir /pocket-ui/releases"
    echo "mkdir /$R"
    for f in pocket-ui pocket-core pocket-splash portmaster-setup.sh; do
      echo "write $S/storage/$f /$R/$f"
    done
    echo "write $S/storage/run-pocket-ui.sh /pocket-ui/run-pocket-ui.sh"
    # The layout scripts/dev.sh and the updater use (docs/UPDATES.md): fixed paths through current.
    echo "symlink /pocket-ui/current releases/$VERSION"
    # Licence texts of the crates and the fonts (scripts/licence-files.sh), read from Settings > About.
    echo "mkdir /pocket-ui/licences"
    for f in $(ls "$S/storage/licences"); do
      echo "write $S/storage/licences/$f /pocket-ui/licences/$f"
    done
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
    for p in / /pocket-ui /pocket-ui/releases "/$R" "/$R/pocket-ui" "/$R/pocket-core" "/$R/pocket-splash" "/$R/portmaster-setup.sh" \
      /pocket-ui/run-pocket-ui.sh /pocket-ui/current /pocket-ui/pocket-ui /pocket-ui/pocket-core \
      /pocket-ui/portmaster-setup.sh /pocket-ui/licences $(for f in $(ls "$S/storage/licences"); do echo "/pocket-ui/licences/$f"; done) /.pocket-seed /.pocket-seed/.config /.pocket-seed/.config/system.d "/$SEED" \
      "/$SEED/10-pocket-ui.conf" "/$UNITS/pocket-splash.service" "/$UNITS/sysinit.target.wants" \
      "/$UNITS/sysinit.target.wants/pocket-splash.service" /.pocket-seed/.config/autostart \
      /.pocket-seed/.config/autostart/pocket-ui-image-defaults; do
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
  e2fsck -fn "$P2" >/dev/null || die "STORAGE file system not clean after writing"
  dd if="$P2" of="$IMG" bs=512 seek="$P2_START" conv=notrunc status=none
  rm -f "$P2"
  echo "  pocket-ui/ ($R), .pocket-seed/.config/: system.d/essway.service.d/10-pocket-ui.conf, system.d/pocket-splash.service, autostart/pocket-ui-image-defaults"
}

case ${1:-} in
  extract) extract ;;
  strip-system) shift; strip_system "$@" ;;
  assemble) shift; assemble "$@" ;;
  *) die "usage: build.sh extract | strip-system | assemble VERSION EPOCH" ;;
esac
