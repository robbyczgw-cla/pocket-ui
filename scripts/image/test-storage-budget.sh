#!/bin/sh
# Test of scripts/image/storage-budget.sh and of what build.sh puts into STORAGE (no Docker, no download, seconds).
# Part of the gate (scripts/gate.sh). The 0.3.1 programs did not fit into ROCKNIX's 32 MiB STORAGE (2026-11-08): both image builds
# stopped with "Could not allocate block"; the licence texts were stored there a second time. STORAGE is 128 MiB in the image now.
set -u
cd "$(dirname "$0")/../.." || exit 1
. scripts/image/pins.sh
fail=0
ok() { echo "ok: $1"; }
bad() { echo "FAIL: $1" >&2; fail=1; }
T=$(mktemp -d) || exit 1
trap 'rm -rf "$T"' EXIT
mk() { # dir ui core splash (bytes)
  mkdir -p "$1"; truncate -s "$2" "$1/pocket-ui"; truncate -s "$3" "$1/pocket-core"; truncate -s "$4" "$1/pocket-splash"
}
budget() { sh scripts/image/storage-budget.sh "$1" >"$T/out" 2>&1; }

# STORAGE is 128 MiB in the image (pins.sh P2_SIZE; ROCKNIX's 32 MiB grown by build.sh): the numbers must agree with each other.
[ "$P2_SIZE" = 262144 ] && ok "STORAGE is 128 MiB (262144 sectors)" || bad "P2_SIZE is $P2_SIZE sectors, not 128 MiB"
[ "$P2_FREE_BLOCKS" -gt "$P2_ROCKNIX_FREE_BLOCKS" ] && [ "$P2_FREE_BLOCKS" -lt $((P2_SIZE / 8)) ] && ok "the free blocks ($P2_FREE_BLOCKS) lie between ROCKNIX's ($P2_ROCKNIX_FREE_BLOCKS) and the partition's ($((P2_SIZE / 8)))" || bad "P2_FREE_BLOCKS $P2_FREE_BLOCKS is not plausible"
[ "$IMG_BYTES" = $((IMG_ROCKNIX_BYTES + (P2_SIZE - P2_ROCKNIX_SIZE) * 512)) ] && ok "the image is ROCKNIX's plus what p2 grew by" || bad "IMG_BYTES does not follow from P2_SIZE"

# The real sizes of the releases (aarch64 release builds): all fit, with the reserve and far more.
mk "$T/v030" 19278504 6057800 1989008
mk "$T/v031" 19543168 6066736 1991712
for version in v030 v031; do
  budget "$T/$version"; r=$?
  [ $r = 0 ] && ok "$version: fits with the growth reserve" || { bad "$version: exit $r"; cat "$T/out" >&2; }
done
# main of 14.11.2026 (the 0.5 work, thin of ROCKNIX's 32 MiB: 27 blocks short there, 0.4.2 left 307 of the 512 asked for).
mk "$T/main1114" 20140616 6222048 2418144
budget "$T/main1114"; r=$?
left=$(sed -n 's/.* \([0-9]*\) left = .*/\1/p' "$T/out")
[ $r = 0 ] && [ "${left:-0}" -ge 20000 ] && ok "main of 14.11. fits with ${left:-0} blocks to spare" || { bad "main of 14.11.: exit $r, ${left:-0} blocks left"; cat "$T/out" >&2; }
[ $((P2_ROCKNIX_FREE_BLOCKS - 4918 - 1520 - 591 - 14 - 11)) -lt 0 ] && ok "in ROCKNIX's own 32 MiB those programs would have been short (why STORAGE grew)" || bad "arithmetic of the 32 MiB failure"

# Grow the programs to the exact 512-block reserve. Launcher/helper changes may move this boundary; the reserve itself stays
# fixed. One extra byte then needs another block, so a byte-count-only budget must not let it pass.
boundary_ui_bytes=$(( ((20140616 + 4095) / 4096 + left - 512) * 4096 ))
mk "$T/boundary" "$boundary_ui_bytes" 6222048 2418144
budget "$T/boundary"; r=$?
boundary_left=$(sed -n 's/.* \([0-9]*\) left = .*/\1/p' "$T/out")
[ $r = 0 ] && [ "${boundary_left:-0}" = 512 ] && ok "exactly the growth reserve fits" || bad "growth reserve boundary: exit $r, ${boundary_left:-0} blocks left"
truncate -s $((boundary_ui_bytes + 1)) "$T/boundary/pocket-ui"
budget "$T/boundary"; r=$?; [ $r = 0 ] && grep -q 'STORAGE BUDGET: NOTE' "$T/out" && ok "one block below the reserve is a note, not a stop" || bad "one block below the reserve: exit $r"

# Room for many releases: each may add what 0.4.2 did (430 KiB); 20 MiB more in every program is still fine.
mk "$T/grown" $((20140616 + 20 * 1024 * 1024)) 6222048 2418144
budget "$T/grown"; r=$?; [ $r = 0 ] && ok "programs 20 MiB bigger still fit with the reserve" || { bad "20 MiB growth: exit $r"; cat "$T/out" >&2; }
mk "$T/huge" $((20140616 + 128 * 1024 * 1024)) 6222048 2418144
budget "$T/huge"; r=$?; [ $r = 1 ] && grep -q "does NOT fit" "$T/out" && ok "programs that do not fit at all are red" || { bad "128 MiB growth: exit $r"; cat "$T/out" >&2; }
mkdir -p "$T/empty"; budget "$T/empty"; r=$?; [ $r = 2 ] && ok "programs missing: usage error (exit 2), not a pass" || bad "missing programs: exit $r"

# build.sh: the licence texts are on the FAT partition (pocket-ui-info/licences); STORAGE has a link to them, no second copy.
! grep -q 'cp "\$W/info/licences/"\* "\$S/storage/licences/"' scripts/image/build.sh && ok "build.sh does not copy the licence texts into the STORAGE stage" || bad "build.sh still copies the licence texts into STORAGE"
grep -q 'symlink /pocket-ui/licences /flash/pocket-ui-info/licences' scripts/image/build.sh && ok "build.sh links /pocket-ui/licences to /flash/pocket-ui-info/licences" || bad "build.sh has no licences link"
grep -q 'P2_ROCKNIX_FREE_BLOCKS' scripts/image/build.sh && grep -q 'P2_FREE_BLOCKS' scripts/image/build.sh && ok "build.sh checks the free blocks of ROCKNIX's STORAGE and of the grown one against pins.sh" || bad "build.sh does not check the free blocks"
grep -q 'resize2fs "\$P2"' scripts/image/build.sh && grep -q '^grow_image()' scripts/image/build.sh && ok "build.sh grows the file system and the partition" || bad "build.sh does not grow STORAGE"
grep -q 'IMG_BYTES' scripts/image/check.sh && grep -q 'IMG_BYTES' scripts/image/check-h700.sh && ok "both image checks expect the grown image size" || bad "an image check still expects ROCKNIX's size"
grep -q 'P2_MIN_HEADROOM_BLOCKS' scripts/image/check.sh && ok "check.sh fails a finished image with too little room (not a warning)" || bad "check.sh has no hard headroom check"
! grep -q 'warn "STORAGE keeps a quarter free' scripts/image/check.sh && ok "the 'quarter free' warning is gone" || bad "the 'quarter free' warning is still a warning"
grep -q 'storage-budget.sh' scripts/release-check.sh && ok "release-check.sh runs the budget" || bad "release-check.sh does not run the budget"

# build.sh's grow_image on synthetic images of ROCKNIX's geometry (sparse files, seconds): a GPT one (SM8250, SM8550, RK3566) and an MBR one
# (H700). Only p2's size (and on GPT the last usable sector) changes, sfdisk -V is clean, the image has the new length, the boot code of
# sector 0 and everything between it and p2 stay byte for byte, and the grown file system still checks out with the pinned free blocks.
if command -v sfdisk >/dev/null && command -v mke2fs >/dev/null && command -v resize2fs >/dev/null && command -v dumpe2fs >/dev/null; then
  for label in gpt dos; do
    IMG=$T/$label.img; W=$T/w-$label; mkdir -p "$W"
    truncate -s "$IMG_ROCKNIX_BYTES" "$IMG"
    if [ $label = gpt ]; then
      printf 'label: gpt\nfirst-lba: 34\n%s1 : start=%s, size=%s, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, name="system", attrs="LegacyBIOSBootable"\n%s2 : start=%s, size=%s, type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, name="storage"\n' \
        "$IMG" "$P1_START" "$P1_SIZE" "$IMG" "$P2_START" "$P2_ROCKNIX_SIZE" | sfdisk -q "$IMG" >/dev/null 2>&1
    else
      printf 'label: dos\n%s1 : start=%s, size=%s, type=c, bootable\n%s2 : start=%s, size=%s, type=83\n' \
        "$IMG" "$P1_START" "$P1_SIZE" "$IMG" "$P2_START" "$P2_ROCKNIX_SIZE" | sfdisk -q "$IMG" >/dev/null 2>&1
    fi
    printf 'BOOTCODE-OF-THE-LOADER' | dd of="$IMG" bs=1 seek=100 conv=notrunc status=none
    printf 'LOADER-AT-40000' | dd of="$IMG" bs=1 seek=40000 conv=notrunc status=none
    printf 'FAT-STARTS-HERE' | dd of="$IMG" bs=512 seek="$P1_START" conv=notrunc status=none
    mke2fs -q -F -t ext4 -b 4096 -L STORAGE -O resize_inode -J size=4 "$W/p2.ext4" 8192 >/dev/null 2>&1
    dd if="$W/p2.ext4" of="$IMG" bs=512 seek="$P2_START" conv=notrunc status=none
    cp --sparse=always "$IMG" "$T/$label.orig"
    head -c "$((P2_START * 512))" "$IMG" | tail -c +447 | md5sum >"$T/$label.before"
    head -c 446 "$IMG" | md5sum >"$T/$label.code"
    sfdisk -d "$IMG" >"$T/$label.dump.before" 2>/dev/null
    die() { echo "$*" >&2; exit 1; }
    . scripts/image/p2-table.sh
    eval "$(sed -n '/^grow_image() {/,/^}/p' scripts/image/build.sh)"
    if (grow_image) >"$T/$label.log" 2>&1; then
      [ "$(stat -c %s "$IMG")" = "$IMG_BYTES" ] && ok "$label: the image is $IMG_BYTES bytes" || bad "$label: wrong image length $(stat -c %s "$IMG")"
      sfdisk -d "$IMG" >"$T/$label.dump.after" 2>/dev/null
      [ "$(diff "$T/$label.dump.before" "$T/$label.dump.after" | grep -c '^[<>]')" = "$([ $label = gpt ] && echo 4 || echo 2)" ] && grep -q "size= *$P2_SIZE," "$T/$label.dump.after" \
        && ok "$label: only p2's size$([ $label = gpt ] && echo ' and the last usable sector') changed" || { bad "$label: the table changed in more than p2's size"; diff "$T/$label.dump.before" "$T/$label.dump.after" >&2; }
      sfdisk -V "$IMG" >/dev/null 2>&1 && ok "$label: sfdisk -V is clean" || bad "$label: sfdisk -V finds errors"
      [ "$(head -c 446 "$IMG" | md5sum)" = "$(cat "$T/$label.code")" ] && ok "$label: sector 0's boot code is as it was" || bad "$label: the boot code of sector 0 changed"
      # The check that nothing but p2's size differs: the first 16 MiB as ROCKNIX shipped them come back from the grown image (p2-table.sh).
      [ "$(loader_md5 "$IMG")" = "$(head -c $((P1_START * 512)) "$T/$label.orig" | md5sum | cut -d' ' -f1)" ] && ok "$label: the first 16 MiB hash as ROCKNIX's again once p2's size is put back (loader_md5)" || bad "$label: loader_md5 does not give ROCKNIX's first 16 MiB back"
      if [ $label = dos ]; then
        [ "$(cmp -l "$T/$label.orig" "$IMG" 2>/dev/null | awk '$1 <= 512' | wc -l)" -le 4 ] && cmp -l "$T/$label.orig" "$IMG" 2>/dev/null | awk '$1 <= 512 {print $1}' | grep -qvE '^47[5-8]$' && bad "mbr: sector 0 changed outside p2's size field" || ok "mbr: sector 0 differs from ROCKNIX's only in p2's four size bytes"
      fi
      # GPT keeps its primary header (sector 1) and entries (2..33) in front, which sfdisk rewrites: compare from sector 34 on, MBR from sector 1.
      from=$([ $label = gpt ] && echo 34 || echo 1)
      [ "$(dd if="$IMG" bs=512 skip=$from count=$((P2_START - from)) status=none | md5sum)" = "$(dd if="$T/$label.img" bs=512 skip=$from count=$((P2_START - from)) status=none | md5sum)" ] && ok "$label: everything between the table and p2 is untouched" || bad "$label: bytes before p2 changed"
      # The file system grown the way build.sh does it, in the grown partition, checks out with the pinned free blocks.
      truncate -s $((P2_SIZE * 512)) "$W/p2.ext4"
      e2fsck -fn "$W/p2.ext4" >/dev/null 2>&1 && resize2fs "$W/p2.ext4" >/dev/null 2>&1
      [ "$(dumpe2fs -h "$W/p2.ext4" 2>/dev/null | sed -n 's/^Free blocks: *//p')" -gt "$P2_ROCKNIX_FREE_BLOCKS" ] 2>/dev/null && e2fsck -fn "$W/p2.ext4" >/dev/null 2>&1 && ok "$label: STORAGE grows to $P2_SIZE sectors and stays clean" || bad "$label: the grown file system is not clean"
    else
      bad "$label: grow_image failed: $(cat "$T/$label.log")"
    fi
  done
else
  ok "grow_image test skipped: sfdisk/mke2fs/resize2fs are not here (the image checks run it for real)"
fi

[ $fail = 0 ] && echo "storage budget tests: ok" || echo "storage budget tests: FAILED" >&2
exit $fail
