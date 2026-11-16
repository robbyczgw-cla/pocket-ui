#!/bin/sh
# Library (sourced by build.sh and check-h700.sh in the tools container): the changes to ROCKNIX's partition table that come with
# the bigger STORAGE (pins.sh P2_SIZE), and the check that nothing else changed.
#
#   MBR (H700): p2's size is one 32-bit field of sector 0's second table entry. It is the only thing patched (offset 474), so the
#     image's table is ROCKNIX's byte for byte but for that field; sfdisk would also have rewritten both entries' CHS values.
#   GPT (SM8250, SM8550, SM4450, RK3566): sfdisk writes the table again with p2's size and the last usable sector changed (both copies,
#     the second at the new end); UUIDs, names and attributes stay. build.sh's grow_image checks the dumps differ in only those.
#
# loader_md5 IMAGE: md5 of the first 16 MiB (P1_START sectors: partition table, U-Boot, idbloader) of a finished image as ROCKNIX
# shipped them, to compare with pins.sh H700_LOADER_MD5 / H700_DDR3_LOADER_MD5 / RK3566_LOADER_MD5: those hash the image's own bytes,
# which the grown table changes. The inverse of the change is made on a scratch copy and that is hashed, so the check still proves
# that nothing but p2's size differs. Needs P1_START, P2_ROCKNIX_SIZE, IMG_ROCKNIX_BYTES (pins.sh).
# By hand: sh -c '. scripts/image/pins.sh; . scripts/image/p2-table.sh; loader_md5 IMAGE'. Only defines functions: it runs nothing when sourced.

# $1 as the four bytes of a little-endian 32-bit number.
le32() { printf "$(printf '\\%03o\\%03o\\%03o\\%03o' $(($1 & 255)) $(($1 >> 8 & 255)) $(($1 >> 16 & 255)) $(($1 >> 24 & 255)))"; }

# mbr_p2_size IMAGE [NEW]: p2's size in sectors from sector 0's table; with NEW, writes it (IMAGE keeps its length).
mbr_p2_size() {
  if [ $# -ge 2 ]; then le32 "$2" | dd of="$1" bs=1 seek=474 conv=notrunc status=none; fi
  dd if="$1" bs=1 skip=474 count=4 status=none | od -An -tu4 | tr -d ' '
}

loader_md5() {
  _img=$1; _t=$(mktemp -d) || return 1; _s=$_t/s.img
  head -c $((P1_START * 512)) "$_img" >"$_s" || { rm -rf "$_t"; return 1; }
  if [ "$(dd if="$_img" bs=1 skip=510 count=2 status=none | od -An -tx1 | tr -d ' ')" = 55aa ] && ! sfdisk -d "$_img" 2>/dev/null | grep -q '^label: gpt'; then
    mbr_p2_size "$_s" "$P2_ROCKNIX_SIZE" >/dev/null
  else
    truncate -s "$IMG_ROCKNIX_BYTES" "$_s"
    sfdisk -d "$_img" 2>/dev/null |
      sed -E -e "s#$(printf '%s' "$_img" | sed 's/[.\/]/\\&/g')#$_s#g" -e "s/^(last-lba: )[0-9]+/\1$((IMG_ROCKNIX_BYTES / 512 - 34))/" \
             -e "s/^($(printf '%s' "$_s" | sed 's/[.\/]/\\&/g')2 *: *start= *[0-9]+, size= *)[0-9]+/\1$P2_ROCKNIX_SIZE/" >"$_t/table"
    # The scratch copy holds the image's first 16 MiB (primary table), not a second GPT; sfdisk writes both at the old end.
    sfdisk -q --no-reread --no-tell-kernel --wipe never --wipe-partitions never "$_s" <"$_t/table" >/dev/null 2>&1 || { echo "p2-table: sfdisk failed" >&2; rm -rf "$_t"; return 1; }
    head -c 446 "$_img" | dd of="$_s" conv=notrunc status=none
  fi
  head -c $((P1_START * 512)) "$_s" | md5sum | cut -d' ' -f1
  rm -rf "$_t"
}
