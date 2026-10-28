#!/system/bin/sh
# Write the verified stock ABL backups back to abl_a and abl_b and compare after writing.
# Run as root from Android (Handheld Settings > Advanced > Run Script as Root).

DIR=/sdcard/rocknix_abl
LOG="$DIR/safe_restore.log"
SUMS="$DIR/abl_backup.sha256"

exec >"$LOG" 2>&1
fail() { echo "FAILED: $*"; exit 1; }

echo "safe_restore_abl $(date)"
[ "$(id -u)" = "0" ] || fail "not running as root. Nothing written"
[ -f "$SUMS" ] || fail "abl_backup.sha256 missing. Nothing written"

for s in a b; do
  P=/dev/block/by-name/abl_$s
  B="$DIR/abl_$s.img"
  [ -b "$P" ] && [ -f "$B" ] || fail "abl_$s partition or backup missing. Nothing written"
  WANT=$(grep " abl_$s.img\$" "$SUMS" | cut -d' ' -f1)
  [ "$(sha256sum "$B" | cut -d' ' -f1)" = "$WANT" ] || fail "$B damaged (checksum). Nothing written"
  [ "$(stat -c %s "$B")" = "$(blockdev --getsize64 "$P")" ] || fail "$B size differs from partition. Nothing written"
done

for s in a b; do
  P=/dev/block/by-name/abl_$s
  dd if="$DIR/abl_$s.img" of="$P" bs=1M conv=fsync || fail "writing abl_$s"
  sync
  WANT=$(grep " abl_$s.img\$" "$SUMS" | cut -d' ' -f1)
  [ "$(sha256sum "$P" | cut -d' ' -f1)" = "$WANT" ] || fail "abl_$s read-back mismatch"
  echo "abl_$s restored and verified"
done
echo "RESTORE OK. Stock bootloader is back."
