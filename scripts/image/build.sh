#!/bin/sh
# The container half of scripts/make-image.sh. Runs in pocket-image-tools with the repository at /repo (read-only)
# and the work directory at /w. Writes only to files in /w; never to a block device (there are none in here).
#   build.sh extract              /w/base.img -> /w/KERNEL, /w/miniv2-noufs.dtb
#   build.sh assemble VERSION EPOCH
#       /w/image.img (a copy of base.img) gets the boot files from /w/sd-efi, the initramfs hooks, the notes from
#       /w/info, and pocket-ui from /w/bin on STORAGE. EPOCH (seconds) is the time stamp of every added file.
set -eu
. /repo/scripts/image/pins.sh
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

assemble() {
  VERSION=$1
  EPOCH=$2
  # mtools (new directories) and e2fsprogs (inode and superblock times) take their clock from these, so a
  # rebuild of the same commit gives the same image.
  export SOURCE_DATE_EPOCH=$EPOCH E2FSPROGS_FAKE_TIME=$EPOCH
  IMG=$W/image.img
  FAT="$IMG@@$((P1_START * 512))"
  E=$W/sd-efi
  [ "$(md5 "$E/boot/pocket/Image")" = "$IMAGE_MD5" ] || die "boot/pocket/Image is not the expected kernel"

  # Everything that goes in, staged here so owners (root), modes and times are exact.
  S=/tmp/stage
  rm -rf "$S"
  mkdir -p "$S/fat/EFI/BOOT" "$S/fat/boot/grub" "$S/fat/boot/pocket" "$S/fat/pocket-ui-info/licences" "$S/storage/licences"
  cp "$E/EFI/BOOT/BOOTAA64.EFI" "$S/fat/EFI/BOOT/"
  cp "$E/boot/grub/grub.cfg" "$E/boot/grub/grub-efi-arm64-bin.version" "$S/fat/boot/grub/"
  cp "$E/boot/pocket/Image" "$E/boot/pocket/miniv2-noufs.dtb" "$E/boot/pocket/KERNEL.source.md5" "$S/fat/boot/pocket/"
  cp /repo/deploy/image/post-flash.sh /repo/deploy/image/mount-storage.sh "$S/fat/"
  cp "$W/info/README.txt" "$S/fat/pocket-ui-info/"
  for f in LICENSE NOTICE.md; do [ ! -f "$W/info/$f" ] || cp "$W/info/$f" "$S/fat/pocket-ui-info/"; done
  # The same texts on the card and in STORAGE (/storage/pocket-ui/licences, where Settings > About points).
  cp "$W/info/licences/"* "$S/fat/pocket-ui-info/licences/"
  cp "$W/info/licences/"* "$S/storage/licences/"
  for f in pocket-ui pocket-core pocket-splash; do install -m 755 "$W/bin/$f" "$S/storage/$f"; done
  install -m 755 /repo/deploy/portmaster-setup.sh "$S/storage/portmaster-setup.sh"
  install -m 755 /repo/deploy/run-pocket-ui.sh "$S/storage/run-pocket-ui.sh"
  install -m 644 /repo/deploy/essway-pocket-ui.conf "$S/storage/10-pocket-ui.conf"
  install -m 644 /repo/deploy/pocket-splash.service "$S/storage/pocket-splash.service"
  chown -R 0:0 "$S"
  find "$S" -exec touch -h -d "@$EPOCH" {} +

  echo "== FAT partition"
  # rocknix_abl/ holds ROCKNIX's replacement for Retroid's bootloader (abl) and scripts that flash it. This boot
  # path never uses it, and flashing it is what this setup avoids, so it is not on the card.
  mdeltree -i "$FAT" ::/rocknix_abl
  for d in EFI EFI/BOOT boot boot/grub boot/pocket pocket-ui-info pocket-ui-info/licences; do
    mmd -i "$FAT" "::/$d"
  done
  (cd "$S/fat" && find . -type f | sed 's#^\./##' | sort) | while read -r f; do
    mcopy -m -i "$FAT" "$S/fat/$f" "::/$f"
  done
  mdir -/ -b -i "$FAT" ::/ | sed 's#^::/#  #'

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
    for p in / /pocket-ui /pocket-ui/releases "/$R" "/$R/pocket-ui" "/$R/pocket-core" "/$R/pocket-splash" "/$R/portmaster-setup.sh" \
      /pocket-ui/run-pocket-ui.sh /pocket-ui/current /pocket-ui/pocket-ui /pocket-ui/pocket-core \
      /pocket-ui/portmaster-setup.sh /pocket-ui/licences $(for f in $(ls "$S/storage/licences"); do echo "/pocket-ui/licences/$f"; done) /.pocket-seed /.pocket-seed/.config /.pocket-seed/.config/system.d "/$SEED" \
      "/$SEED/10-pocket-ui.conf" "/$UNITS/pocket-splash.service" "/$UNITS/sysinit.target.wants" \
      "/$UNITS/sysinit.target.wants/pocket-splash.service"; do
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
  echo "  pocket-ui/ ($R), .pocket-seed/.config/system.d/: essway.service.d/10-pocket-ui.conf, pocket-splash.service"
}

case ${1:-} in
  extract) extract ;;
  assemble) shift; assemble "$@" ;;
  *) die "usage: build.sh extract | assemble VERSION EPOCH" ;;
esac
