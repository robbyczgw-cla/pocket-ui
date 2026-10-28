#!/system/bin/sh
# Read-only: report what abl_a / abl_b contain (stock backup, ROCKNIX ABL, or unknown).
# Run as root from Android (Handheld Settings > Advanced > Run Script as Root). Writes only the log.

DIR=/sdcard/rocknix_abl
ELF="$DIR/abl_signed-SM8250.elf"
LOG="$DIR/verify_abl.log"
SUMS="$DIR/abl_backup.sha256"

exec >"$LOG" 2>&1
echo "verify_abl $(date)"
echo "model: $(getprop ro.product.model)  build: $(getprop ro.build.display.id)  slot: $(getprop ro.boot.slot_suffix)"
[ "$(id -u)" = "0" ] || { echo "not running as root"; exit 1; }

ELF_SIZE=$(stat -c %s "$ELF")
ELF_SUM=$(sha256sum "$ELF" | cut -d' ' -f1)
echo "ROCKNIX ELF: $ELF_SUM ($ELF_SIZE bytes)"

for s in a b; do
  P=/dev/block/by-name/abl_$s
  FULL=$(sha256sum "$P" | cut -d' ' -f1)
  HEAD=$(dd if="$P" bs="$ELF_SIZE" count=1 2>/dev/null | sha256sum | cut -d' ' -f1)
  STOCK=$(grep " abl_$s.img\$" "$SUMS" 2>/dev/null | cut -d' ' -f1)
  if [ "$HEAD" = "$ELF_SUM" ]; then STATE="ROCKNIX ABL"
  elif [ -n "$STOCK" ] && [ "$FULL" = "$STOCK" ]; then STATE="STOCK (matches backup)"
  else STATE="UNKNOWN"
  fi
  echo "abl_$s: $STATE"
done
