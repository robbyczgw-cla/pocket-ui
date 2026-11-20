#!/bin/sh
# pocket-ui R36S card (RK3326): makes the screen overlay from the handheld's stock tree, once. The image puts this file in
# /storage/.pocket-seed/.config/autostart/; deploy/image/mount-storage.sh moves it to /storage/.config/autostart at the second
# boot, and ROCKNIX's autostart runs it at every start, before the user interface. It stays (unlike image-defaults.sh).
#
# The player (or we, on day one) copies the .dtb of the R36S's stock card into stock/ on the card's ROCKNIX partition. When
# there is such a file and no overlays/mipi-panel.dtbo yet, this runs ROCKNIX's own importpanel.py (package generic-dsi: the
# stock tree's panel init sequence and timings for ROCKNIX's generic-dsi driver, nothing else), writes the result as
# overlays/mipi-panel.dtbo and starts the handheld again once. The stock tree is then renamed to <name>.dtb.used, so deleting
# overlays/mipi-panel.dtbo goes back to the original screen for good and never makes a new one; a tree it can't read becomes
# <name>.dtb.failed. What happened is written to stock/panel-log.txt (readable on any computer).
# Not ROCKNIX's EE-clone quirk (002-generate_dtbo, rocknix_dtbo.py): that one also rewrites the pad, sticks and headphones for
# the EE clone tree, and reinstalls the boot loader; on an R36S tree only the screen is wanted (docs/SD-IMAGE-RK3326.md).
# Without a computer (a card flashed from a phone): the stock card itself in the second slot (TF2) at the first start. Its small
# FAT partition with boot.ini (BOOT, under 1 GiB; never the card this runs from, never a games partition) is mounted read-only,
# and the one tree its boot.ini loads is taken as if it had been copied into stock/ (nothing is written on the stock card).
# Not when stock/ already holds a used or failed tree (the player's choice stands), nor when boot.ini names no single tree there
# (then stock/panel-log.txt says so once, in stock/tf2-looked; delete that file to look again).
# PU_FLASH, PU_IMPORTPANEL, PU_MOUNTS, PU_PARTITIONS, PU_BLKID are for the tests only (deploy/image/test-r36s-stock-panel.sh).
F=${PU_FLASH:-/flash}
IMPORTPANEL=${PU_IMPORTPANEL:-/usr/libexec/generic-dsi/importpanel.py}
MOUNTS=${PU_MOUNTS:-/proc/mounts}
PARTITIONS=${PU_PARTITIONS:-/proc/partitions}
BLKID=${PU_BLKID:-blkid}
O=$F/overlays/mipi-panel.dtbo
[ -e "$O" ] && exit 0
S=
for f in "$F"/stock/*.dtb; do
  if [ -f "$f" ]; then S=$f; break; fi
done
T=$(mktemp -d) || exit 0
note= done_before=
for f in "$F"/stock/*.dtb.used "$F"/stock/*.dtb.failed "$F/stock/tf2-looked"; do
  [ -e "$f" ] && done_before=1
done
if [ -z "$S" ] && [ -z "$done_before" ]; then
  own=$(grep " $F " "$MOUNTS" 2>/dev/null | head -n 1 | cut -d" " -f1)
  own=${own%p[0-9]*}
  mkdir -p "$T/card"
  for part in $("$BLKID" 2>/dev/null | sed -n 's/^\(\/dev\/mmcblk[0-9]*p[0-9]*\):.* TYPE="vfat".*/\1/p'); do
    [ -n "$S" ] && break
    case $part in "$own"p*) continue ;; esac
    grep -q "^$part " "$MOUNTS" 2>/dev/null && continue
    size=$(awk -v r="${part#/dev/}" '$4 == r {print $3}' "$PARTITIONS" 2>/dev/null)
    [ "${size:-0}" -gt 0 ] && [ "${size:-0}" -lt 1048576 ] 2>/dev/null || continue
    mount -o ro -t vfat "$part" "$T/card" >/dev/null 2>&1 || continue
    if [ -f "$T/card/boot.ini" ]; then
      names=$(grep -v '^[[:space:]]*#' "$T/card/boot.ini" | grep -o '[A-Za-z0-9_.-]*\.dtb' | sort -u)
      pick= n=0
      for d in $names; do
        [ -f "$T/card/$d" ] || continue
        pick=$d n=$((n + 1))
      done
      if [ $n = 1 ] && cp "$T/card/$pick" "$T/$pick"; then
        S=$T/$pick
        note="$pick: read from the stock card in the second slot (${part#/dev/}, read only)"
      elif [ -z "$note" ]; then
        note="${part#/dev/}: a stock card with boot.ini, but it names no single tree there ($(echo $names)); copy the right .dtb into stock/"
      fi
    fi
    umount "$T/card" >/dev/null 2>&1 || umount -l "$T/card" >/dev/null 2>&1
  done
  if [ -z "$S" ]; then
    if [ -n "$note" ]; then
      mount -o remount,rw "$F" 2>/dev/null
      mkdir -p "$F/stock" && echo "$note" >>"$F/stock/panel-log.txt" && : >"$F/stock/tf2-looked"
      sync
      mount -o remount,ro "$F" 2>/dev/null
    fi
    rm -rf "$T"
    exit 0
  fi
fi
[ -n "$S" ] || { rm -rf "$T"; exit 0; }
python3 "$IMPORTPANEL" -O dtbo -o "$T/mipi-panel.dtbo" "$S" >"$T/log" 2>&1
rc=$?
magic=$(od -An -tx1 -N4 "$T/mipi-panel.dtbo" 2>/dev/null | tr -d ' \n')
mount -o remount,rw "$F" 2>/dev/null
case $S in
  "$T"/*) mkdir -p "$F/stock" && cp "$S" "$F/stock/" && S=$F/stock/${S##*/} && echo "$note" >>"$F/stock/panel-log.txt" ;;
esac
if [ $rc = 0 ] && [ "$magic" = d00dfeed ]; then
  mkdir -p "$F/overlays"
  if cp "$T/mipi-panel.dtbo" "$O.new" && sync && mv "$O.new" "$O"; then
    mv "$S" "$S.used"
    echo "$(basename "$S"): screen overlay written to overlays/mipi-panel.dtbo; starting again. Delete that file to go back to the original screen." >>"$F/stock/panel-log.txt"
    ok=1
  else
    rm -f "$O.new"
    echo "$(basename "$S"): could not write overlays/mipi-panel.dtbo" >>"$F/stock/panel-log.txt"
    ok=0
  fi
else
  mv "$S" "$S.failed"
  { echo "$(basename "$S"): no screen found in this file (importpanel.py exit $rc); renamed to $(basename "$S").failed"; tail -3 "$T/log"; } >>"$F/stock/panel-log.txt"
  ok=0
fi
sync
mount -o remount,ro "$F" 2>/dev/null
rm -rf "$T"
[ $ok = 1 ] && reboot
exit 0
