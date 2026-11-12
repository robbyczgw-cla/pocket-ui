#!/bin/sh
# Against the pinned ROCKNIX KERNEL (SM8250, or SM8550 for the Retroid cards): prove that each selected tree changes only
# the two UFS status properties, and that both are disabled in the pinned result.
# Requires python3, fdtget, fdtput and dtc (the image-tools container has them).
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
. "$ROOT/scripts/image/pins.sh"
[ $# = 1 ] || { echo "usage: $0 KERNEL" >&2; exit 2; }
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT HUP INT TERM
md5() { md5sum "$1" | cut -d' ' -f1; }
# The SM8550 KERNEL (pins.sh SM8550_EFI_MODELS, the Retroid cards) goes through the same proof with its own trees and nodes.
if [ "$(md5 "$1")" = "$SM8550_KERNEL_MD5" ]; then
  MODELS=$(echo "$SM8550_EFI_MODELS" | awk -F '|' 'NF { print $1 }') NODES=$SM8550_UFS_NODES
else
  [ "$(md5 "$1")" = "$KERNEL_MD5" ]
  MODELS="rp-mini-v2 rp5 flip2" NODES="/soc@0/ufshc@1d84000 /soc@0/phy@1d87000"
fi
for model in $MODELS; do
  if [ "$NODES" = "$SM8550_UFS_NODES" ]; then
    sm8550_efi_pins "$model"
    SM8250_MODEL_NAME=$EFI_MODEL_NAME SM8250_ORIG_MD5=$EFI_ORIG_MD5 DTB_MD5=$EFI_DTB_MD5
  else
    sm8250_pins "$model"
  fi
  python3 -I "$ROOT/scripts/make-test-bootimg.py" "$1" "$W/boot.img" "$SM8250_MODEL_NAME" >/dev/null
  original=$W/boot.img.orig.dtb
  [ "$(md5 "$original")" = "$SM8250_ORIG_MD5" ]
  cp "$original" "$W/noufs.dtb"
  for node in $NODES; do
    fdtput -t s "$W/noufs.dtb" "$node" status disabled
  done
  [ "$(md5 "$W/noufs.dtb")" = "$DTB_MD5" ]
  cp "$W/noufs.dtb" "$W/reversed.dtb"
  for node in $NODES; do
    [ "$(fdtget -t s "$W/noufs.dtb" "$node" status)" = disabled ]
    fdtput -t s "$W/reversed.dtb" "$node" status "$(fdtget -t s "$original" "$node" status)"
    # A tree that makes either UFS node visible must lose the accepted hash.
    cp "$W/noufs.dtb" "$W/unsafe.dtb"
    fdtput -t s "$W/unsafe.dtb" "$node" status okay
    [ "$(md5 "$W/unsafe.dtb")" != "$DTB_MD5" ]
  done
  dtc -q -I dtb -O dts "$original" >"$W/original.dts" 2>/dev/null
  dtc -q -I dtb -O dts "$W/reversed.dtb" >"$W/reversed.dts" 2>/dev/null
  cmp "$W/original.dts" "$W/reversed.dts"
  echo "ok: $model: exact tree, only two UFS statuses changed, both disabled; unsafe trees rejected"
done
