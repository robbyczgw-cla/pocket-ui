#!/bin/sh
# Checks a pocket-ui H700 SD card image offline, without writing anything to a disk (docs/SD-IMAGE.md, "H700").
#   scripts/check-image-h700.sh build/image-h700/pocket-ui-<version>-<model>.img.xz [--version <version>]
# Partition table, ROCKNIX's U-Boot before the first partition, the FAT files (nothing but ROCKNIX's, dtb.img, the
# hooks and the notes), dtb.img against the model, KERNEL and SYSTEM against ROCKNIX 20261001, the initramfs hooks,
# every file on STORAGE with owner and mode, and that nothing from a developer device is on the card. The model is
# read from the file name (pins.sh H700_MODELS). Work files go to build/image-h700/check/ (CHECK_WORK). Docker is needed.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/image/pins.sh"
. "$ROOT/scripts/image/tools.sh"

[ $# -ge 1 ] || { sed -n '2,3p' "$0" >&2; exit 2; }
IN=$1
shift
VERSION=
while [ $# -gt 0 ]; do
  case $1 in
    --version) VERSION=$2; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
[ -f "$IN" ] || { echo "FAIL no image at $IN"; exit 1; }
IN=$(cd "$(dirname "$IN")" && pwd)/$(basename "$IN")
# pocket-ui-<version>-<model>.img[.xz]
base=$(basename "$IN" | sed 's/\.xz$//; s/\.img$//')
MODEL=
for m in $(echo "$H700_MODELS" | awk 'NF { print $1 }'); do
  case $base in pocket-ui-*-"$m") MODEL=$m ;; esac
done
[ -n "$MODEL" ] || { echo "FAIL cannot tell the model from the name $(basename "$IN")" >&2; exit 1; }
[ -n "$VERSION" ] || VERSION=$(echo "$base" | sed "s/^pocket-ui-//; s/-$MODEL\$//")
W=${CHECK_WORK:-$ROOT/build/image-h700/check}
mkdir -p "$W"
W=$(cd "$W" && pwd)
TOOLS=$(tools_image)
fails=0

echo "== $(basename "$IN") (version $VERSION, model $MODEL)"
SUMS=$(dirname "$IN")/SHA256SUMS
if [ -f "$SUMS" ] && grep -q " $(basename "$IN")\$" "$SUMS"; then
  if (cd "$(dirname "$IN")" && grep " $(basename "$IN")\$" SHA256SUMS | shasum -a 256 -c - >/dev/null); then
    echo "ok   SHA256SUMS matches"
  else
    echo "FAIL SHA256SUMS does not match"
    fails=$((fails + 1))
  fi
else
  echo "WARN no SHA256SUMS entry next to the image"
fi

case $IN in
  *.xz)
    echo "unpacking to $W/check.img"
    docker run --rm -v "$(dirname "$IN"):/in:ro" -v "$W:/w" "$TOOLS" \
      sh -c 'xz -dc -T0 "/in/$1" >/w/check.img' sh "$(basename "$IN")"
    RAW=/w/check.img
    MOUNT_IN=
    ;;
  *)
    RAW=/in/$(basename "$IN")
    MOUNT_IN="-v $(dirname "$IN"):/in:ro"
    ;;
esac

# shellcheck disable=SC2086
docker run --rm $MOUNT_IN -v "$W:/w" -v "$ROOT:/repo:ro" -e CHECK_OWNER="$(id -u):$(id -g)" "$TOOLS" sh /repo/scripts/image/check-h700.sh "$RAW" "$VERSION" /w "$MODEL" ||
  fails=$((fails + 1))

echo "== the programs"
REL=$W/releases/$VERSION
BASE=${POCKETUI_TEST_IMAGE:-pocket-vdev-base:20261001}
if [ ! -d "$REL" ]; then
  echo "FAIL no releases/$VERSION on STORAGE"
  fails=$((fails + 1))
elif docker image inspect "$BASE" >/dev/null 2>&1; then
  for b in pocket-ui pocket-core pocket-splash; do
    got=$(docker run --rm -v "$REL:/b:ro" "$BASE" "/b/$b" --version 2>&1 || true)
    if [ "$got" = "$b $VERSION" ]; then
      echo "ok   $b --version: $got"
    else
      echo "FAIL $b --version: '$got', expected '$b $VERSION'"
      fails=$((fails + 1))
    fi
  done
else
  echo "WARN $BASE missing (scripts/vdev.sh image, needs an aarch64-capable Docker host): --version not checked"
fi

[ -z "$MOUNT_IN" ] && rm -f "$W/check.img"
echo
if [ $fails -eq 0 ]; then echo "IMAGE OK: $(basename "$IN")"; else echo "IMAGE NOT OK: $(basename "$IN")"; exit 1; fi
