#!/system/bin/sh
# Back up the stock ABL (abl_a, abl_b) and verify the copies.
# Run as root from Android (Handheld Settings > Advanced > Run Script as Root).
# Never overwrites an existing backup, so a stock backup cannot be replaced by a flashed ABL.

DIR=/sdcard/rocknix_abl
ELF="$DIR/abl_signed-SM8250.elf"
LOG="$DIR/safe_backup.log"
SUMS="$DIR/abl_backup.sha256"

exec >"$LOG" 2>&1
fail() { echo "FAILED: $*"; echo "Nothing was written to the device."; exit 1; }

echo "safe_backup_abl $(date)"
echo "model: $(getprop ro.product.model)  build: $(getprop ro.build.display.id)  slot: $(getprop ro.boot.slot_suffix)"

[ "$(id -u)" = "0" ] || fail "not running as root"
[ -f "$ELF" ] || fail "$ELF missing (copy the whole rocknix_abl folder to internal storage)"
[ -e "$DIR/abl_a.img" ] || [ -e "$DIR/abl_b.img" ] && fail "a backup already exists in $DIR. Move it to your computer first; it is never overwritten"

ELF_SIZE=$(stat -c %s "$ELF")
ELF_SUM=$(sha256sum "$ELF" | cut -d' ' -f1)

for s in a b; do
  P=/dev/block/by-name/abl_$s
  [ -b "$P" ] || fail "$P not found"
  # The current ABL must not already be the ROCKNIX one, or the "backup" would not be stock.
  CUR=$(dd if="$P" bs="$ELF_SIZE" count=1 2>/dev/null | sha256sum | cut -d' ' -f1)
  [ "$CUR" = "$ELF_SUM" ] && fail "abl_$s already contains the ROCKNIX ABL; this is not a stock backup"
done

for s in a b; do
  P=/dev/block/by-name/abl_$s
  OUT="$DIR/abl_$s.img"
  PSIZE=$(blockdev --getsize64 "$P")
  dd if="$P" of="$OUT" bs=1M || fail "reading $P"
  sync
  FSIZE=$(stat -c %s "$OUT")
  [ "$FSIZE" = "$PSIZE" ] || fail "abl_$s.img is $FSIZE bytes, partition is $PSIZE"
  PSUM=$(sha256sum "$P" | cut -d' ' -f1)
  FSUM=$(sha256sum "$OUT" | cut -d' ' -f1)
  [ "$PSUM" = "$FSUM" ] || fail "abl_$s.img does not match the partition"
  echo "$FSUM  abl_$s.img" >>"$SUMS"
  echo "abl_$s: $PSIZE bytes, sha256 $FSUM  OK"
done

echo "BACKUP OK. Copy abl_a.img, abl_b.img and abl_backup.sha256 to your computer before flashing."
