#!/bin/sh
# Test of deploy/image/r36s-stock-panel.sh (the R36S card's screen overlay from a stock tree) with stand-ins for python3, mount and
# reboot. Run by scripts/image/test-h700-pins.sh when the RK3326 pins are there (the gate).
set -u
cd "$(dirname "$0")/../.." || exit 1
H=deploy/image/r36s-stock-panel.sh
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fail=0
ok() { echo "ok: $1"; }
bad() { echo "FAIL: $1" >&2; fail=1; }
mkdir -p "$T/bin"
# python3 IMPORTPANEL -O dtbo -o OUT STOCK: writes a tree (magic d00dfeed) unless the stock file says "broken".
cat >"$T/bin/python3" <<'EOF'
#!/bin/sh
echo "python3 $*" >>"$PU_CALLS"
out=$5 src=$6
grep -q broken "$src" && { echo "AttributeError: no panel" >&2; exit 1; }
printf '\320\015\376\355rest' >"$out"
EOF
# mount -o ro -t vfat /dev/X DIR: "mounts" the folder $T/dev/X (a card partition) by copying it; a remount is only logged.
cat >"$T/bin/mount" <<'EOF2'
#!/bin/sh
echo "mount $*" >>"$PU_CALLS"
[ "$4" = vfat ] || exit 0
[ -d "$PU_DEVS/${5#/dev/}" ] || exit 32
cp -R "$PU_DEVS/${5#/dev/}/." "$6/"
EOF2
printf '#!/bin/sh\necho "umount $*" >>"$PU_CALLS"\nfind "$1" -mindepth 1 -delete\n' >"$T/bin/umount"
printf '#!/bin/sh\necho reboot >>"$PU_CALLS"\n' >"$T/bin/reboot"
printf '#!/bin/sh\ncat "$PU_DEVS.blkid" 2>/dev/null\n' >"$T/bin/blkid"
chmod 755 "$T/bin/"*
run() { PATH="$T/bin:$PATH" PU_CALLS="$T/calls" PU_FLASH="$T/flash" PU_IMPORTPANEL=/x/importpanel.py PU_DEVS="$T/dev" \
  PU_BLKID="$T/bin/blkid" PU_MOUNTS="$T/mounts" PU_PARTITIONS="$T/partitions" sh "$H"; }
reset() {
  rm -rf "$T/flash" "$T/calls" "$T/dev" "$T/dev.blkid"; mkdir -p "$T/flash/stock" "$T/flash/overlays" "$T/dev"; : >"$T/calls"
  echo "/dev/mmcblk0p1 $T/flash vfat ro 0 0" >"$T/mounts"; : >"$T/partitions"
}
# A partition: name, type, size in KiB, then files as name=content.
part() {
  p=$1 type=$2 size=$3; shift 3
  echo "/dev/$p: LABEL=\"X\" UUID=\"1\" TYPE=\"$type\"" >>"$T/dev.blkid"
  echo " 179 1 $size $p" >>"$T/partitions"
  mkdir -p "$T/dev/$p"
  for f; do printf '%s\n' "${f#*=}" >"$T/dev/$p/${f%%=*}"; done
}

reset
run
[ ! -s "$T/calls" ] && [ ! -e "$T/flash/overlays/mipi-panel.dtbo" ] && ok "no stock tree: nothing runs, no overlay, no restart" || bad "no stock tree"

reset
echo stock >"$T/flash/stock/rk3326-r35s-linux.dtb"
run
[ "$(od -An -tx1 -N4 "$T/flash/overlays/mipi-panel.dtbo" | tr -d ' ')" = d00dfeed ] && ok "a stock tree gives overlays/mipi-panel.dtbo" || bad "no overlay written"
grep -q "^python3 /x/importpanel.py -O dtbo -o .* $T/flash/stock/rk3326-r35s-linux.dtb\$" "$T/calls" && ok "ROCKNIX's importpanel.py, screen only (-O dtbo)" || bad "importpanel call: $(cat "$T/calls")"
[ -f "$T/flash/stock/rk3326-r35s-linux.dtb.used" ] && [ ! -e "$T/flash/stock/rk3326-r35s-linux.dtb" ] && ok "the stock tree is renamed .used" || bad "stock tree not renamed"
[ "$(tail -1 "$T/calls")" = reboot ] && grep -q "mount -o remount,rw $T/flash" "$T/calls" && grep -q "mount -o remount,ro $T/flash" "$T/calls" && ok "/flash rw for the write, ro again, then one restart" || bad "calls: $(cat "$T/calls")"
grep -q "screen overlay written" "$T/flash/stock/panel-log.txt" && ok "stock/panel-log.txt says what happened" || bad "no log"
: >"$T/calls"
rm "$T/flash/overlays/mipi-panel.dtbo"
run
[ ! -e "$T/flash/overlays/mipi-panel.dtbo" ] && ! grep -q reboot "$T/calls" && ok "overlay deleted by the player: the original screen stays, no new overlay, no restart" || bad "overlay came back"

reset
echo stock >"$T/flash/stock/a.dtb"
printf 'mine' >"$T/flash/overlays/mipi-panel.dtbo"
run
[ "$(cat "$T/flash/overlays/mipi-panel.dtbo")" = mine ] && [ -f "$T/flash/stock/a.dtb" ] && [ ! -s "$T/calls" ] && ok "an overlay already there is never replaced" || bad "existing overlay touched"

reset
echo broken >"$T/flash/stock/b.dtb"
run
[ ! -e "$T/flash/overlays/mipi-panel.dtbo" ] && [ -f "$T/flash/stock/b.dtb.failed" ] && ! grep -q reboot "$T/calls" && grep -q "no screen found" "$T/flash/stock/panel-log.txt" \
  && ok "an unreadable tree: no overlay, renamed .failed, logged, no restart (no restart loop)" || bad "broken tree: $(cat "$T/calls")"
: >"$T/calls"
run
[ ! -s "$T/calls" ] && ok "and the next start does nothing" || bad "the failed tree runs again"
# A card flashed from a phone: the stock card in TF2 gives the tree, read only.
reset
part mmcblk0p1 vfat 1048575 "boot.ini=load mmc 0:1 x rk3326-r35s-linux.dtb" "rk3326-r35s-linux.dtb=own"
part mmcblk1p1 vfat 102400 "boot.ini=load mmc 1:1 \${dtb_loadaddr} rk3326-r35s-linux.dtb" "rk3326-r35s-linux.dtb=stock" "rk3326-rg351mp-linux.dtb=other"
part mmcblk1p3 exfat 60000000 "x=games"
before=$(cd "$T/dev" && find . -type f -exec md5sum {} + | sort)
run
[ "$(od -An -tx1 -N4 "$T/flash/overlays/mipi-panel.dtbo" | tr -d ' ')" = d00dfeed ] && [ "$(tail -1 "$T/calls")" = reboot ] \
  && ok "stock card in TF2: overlay from the tree its boot.ini loads, one restart" || bad "TF2 stock card: $(cat "$T/calls")"
grep -q "^python3 /x/importpanel.py -O dtbo -o .*/rk3326-r35s-linux.dtb\$" "$T/calls" && [ "$(cat "$T/flash/stock/rk3326-r35s-linux.dtb.used")" = stock ] \
  && ok "the TF2 tree is the one boot.ini names, kept in stock/ as .used" || bad "wrong tree: $(cat "$T/calls"); $(ls "$T/flash/stock")"
grep -q "mount -o ro -t vfat /dev/mmcblk1p1" "$T/calls" && ! grep -q "mmcblk0p1\|mmcblk1p3" "$T/calls" \
  && ok "only the small FAT partition of the other card is mounted, read only; never the own card, never the games partition" || bad "mounts: $(cat "$T/calls")"
[ "$before" = "$(cd "$T/dev" && find . -type f -exec md5sum {} + | sort)" ] && ok "the stock card is unchanged" || bad "stock card changed"
grep -q "read from the stock card in the second slot (mmcblk1p1, read only)" "$T/flash/stock/panel-log.txt" && ok "panel-log.txt names the stock card" || bad "log: $(cat "$T/flash/stock/panel-log.txt")"
: >"$T/calls"; rm "$T/flash/overlays/mipi-panel.dtbo"
run
[ ! -e "$T/flash/overlays/mipi-panel.dtbo" ] && ! grep -q "reboot\|vfat" "$T/calls" && ok "overlay deleted by the player, stock card still in TF2: original screen stays, the card is not read again" || bad "TF2 overlay came back: $(cat "$T/calls")"

# boot.ini that names two trees present there: nothing guessed, said once.
reset
part mmcblk1p1 vfat 102400 "boot.ini=if a; then load a.dtb; else load b.dtb; fi" "a.dtb=x" "b.dtb=y"
run
[ ! -e "$T/flash/overlays/mipi-panel.dtbo" ] && ! grep -q reboot "$T/calls" && grep -q "names no single tree there (a.dtb b.dtb)" "$T/flash/stock/panel-log.txt" && [ -e "$T/flash/stock/tf2-looked" ] \
  && ok "two trees named: no guess, no restart, logged" || bad "ambiguous: $(cat "$T/calls")"
: >"$T/calls"
run
! grep -q vfat "$T/calls" && [ "$(grep -c "names no single" "$T/flash/stock/panel-log.txt")" = 1 ] && ok "and not looked at again (once)" || bad "ambiguous again: $(cat "$T/calls")"

# A big FAT games card or a FAT card without boot.ini in TF2: never looked into / nothing done.
reset
part mmcblk1p1 vfat 31000000 "boot.ini=x.dtb" "x.dtb=games card"
part mmcblk2p1 vfat 50000 "readme=no boot.ini"
run
! grep -q "mmcblk1p1" "$T/calls" && [ ! -e "$T/flash/overlays/mipi-panel.dtbo" ] && [ ! -e "$T/flash/stock/panel-log.txt" ] && [ ! -e "$T/flash/stock/tf2-looked" ] \
  && ok "a big games card is never mounted; a small card without boot.ini changes nothing" || bad "games card: $(cat "$T/calls")"

# A partition already mounted (automount got it first) is left alone.
reset
part mmcblk1p1 vfat 102400 "boot.ini=s.dtb" "s.dtb=stock"
echo "/dev/mmcblk1p1 /storage/games-external vfat rw 0 0" >>"$T/mounts"
run
! grep -q "vfat /dev/mmcblk1p1" "$T/calls" && ok "an already mounted partition is not mounted again" || bad "mounted twice: $(cat "$T/calls")"

# A tree copied into stock/ by hand wins over the card in TF2.
reset
echo stock >"$T/flash/stock/mine.dtb"
part mmcblk1p1 vfat 102400 "boot.ini=s.dtb" "s.dtb=stock"
run
grep -q "importpanel.py .*$T/flash/stock/mine.dtb\$" "$T/calls" && ! grep -q "vfat" "$T/calls" && ok "stock/ by hand wins, TF2 not read" || bad "hand tree: $(cat "$T/calls")"
exit $fail
