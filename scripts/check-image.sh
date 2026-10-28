#!/bin/sh
# Checks a pocket-ui SD card image offline, without writing anything to a disk (docs/SD-IMAGE.md).
#   scripts/check-image.sh build/image/pocket-ui-<version>-rp-mini-v2.img.xz [--version <version>]
# Partition table and labels, GRUB config and the files it names, kernel and device tree against ROCKNIX 20261001
# and the author's device, the initramfs hooks, every file on STORAGE with owner and mode, and that nothing from a
# developer device is on the card (settings, dev-ssh, keys, Wi-Fi, recents). The binaries must answer --version
# with the image's version (run in the virtual device's base image if it exists).
# The work files go to build/image/check/ (CHECK_WORK). Docker is needed.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
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
[ -n "$VERSION" ] || VERSION=$(version_of "$IN")
[ -n "$VERSION" ] || { echo "FAIL cannot tell the version from the name $(basename "$IN"); pass --version" >&2; exit 1; }
W=${CHECK_WORK:-$ROOT/build/image/check}
mkdir -p "$W"
W=$(cd "$W" && pwd)
TOOLS=$(tools_image)
fails=0

echo "== $(basename "$IN") (version $VERSION)"
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
docker run --rm $MOUNT_IN -v "$W:/w" -v "$ROOT:/repo:ro" -e CHECK_OWNER="$(id -u):$(id -g)" "$TOOLS" sh /repo/scripts/image/check.sh "$RAW" "$VERSION" /w ||
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
  # The base image is made by scripts/vdev.sh image and needs a host that runs aarch64 containers (Apple silicon, or
  # qemu-user binfmt); the x86_64 build host has neither, so there the programs' --version stays unchecked on purpose.
  echo "WARN $BASE missing (scripts/vdev.sh image, needs an aarch64-capable Docker host): --version not checked"
fi

[ -z "$MOUNT_IN" ] && rm -f "$W/check.img"
echo
if [ $fails -eq 0 ]; then echo "IMAGE OK: $(basename "$IN")"; else echo "IMAGE NOT OK: $(basename "$IN")"; exit 1; fi
