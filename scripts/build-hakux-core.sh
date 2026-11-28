#!/bin/sh
# Fetches the hakuX Xbox core pocket-ui ships: the hakuX-lr release asset by portare-ch (based on hakuX by rfandango,
# GPL-3.0-or-later), built by that project's CI. Nothing is compiled here: the archive and the core are checked against the
# sha256 sums in deploy/hakux/SOURCE.txt, the core is only renamed from hakux-lr_libretro.so to hakux_libretro.so, and the
# result is left where `make-install-zip.sh` / `make-fat-install.sh` / `release-channel.sh --hakux DIR` take it from
# (scripts/hakux-files.sh, docs/XBOX.md "Shipping the core"):
#
#   scripts/build-hakux-core.sh [out-dir] [archive]
#     out-dir  default ./hakux-core; gets lib/libretro/hakux_libretro.so, share/licenses/hakux/, BUILD-INFO and the archive
#     archive  a hakux-lr-<version>-aarch64.tar.gz already downloaded (checked the same way); default: download it
#
# A new hakuX-lr release: update the Release, Commit and both sha256 lines of deploy/hakux/SOURCE.txt (and the docs that name
# the version), then run this.
set -eu
bad() { echo "build-hakux-core: $*" >&2; exit 1; }
ROOT=$(cd "$(dirname "$0")/.." && pwd)
SRC=$ROOT/deploy/hakux/SOURCE.txt
OUT=${1:-./hakux-core}
# Release:   https://github.com/portare-ch/hakuX-lr/releases/tag/<tag>, asset <name>,
#            sha256 <64 hex> (built by ...)
TAG_URL=$(awk '$1 == "Release:" { print $2; exit }' "$SRC" | tr -d ',')
ASSET=$(awk '$1 == "Release:" { print $4; exit }' "$SRC" | tr -d ',')
ASSET_SHA=$(awk '$1 == "Release:" { getline; print $2; exit }' "$SRC")
CORE_SHA=$(awk '$1 == "Core:" && $3 == "sha256" { print $4; exit }' "$SRC")
case $TAG_URL in */releases/tag/*) ;; *) bad "no release URL in $SRC" ;; esac
[ ${#ASSET_SHA} -eq 64 ] && [ ${#CORE_SHA} -eq 64 ] && [ -n "$ASSET" ] || bad "no asset name or sha256 in $SRC"
URL=$(echo "$TAG_URL" | sed 's#/releases/tag/#/releases/download/#')/$ASSET

mkdir -p "$OUT"
TAR=$OUT/$ASSET
if [ $# -ge 2 ]; then cp "$2" "$TAR"
elif [ ! -s "$TAR" ]; then curl -fL --retry 3 -o "$TAR.part" "$URL" && mv "$TAR.part" "$TAR"; fi
echo "$ASSET_SHA  $TAR" | sha256sum -c --quiet - || bad "$TAR is not the pinned asset (sha256 $ASSET_SHA)"

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
tar -xzf "$TAR" -C "$TMP" --strip-components=1
[ -s "$TMP/lib/libretro/hakux-lr_libretro.so" ] || bad "no lib/libretro/hakux-lr_libretro.so in $ASSET"
echo "$CORE_SHA  $TMP/lib/libretro/hakux-lr_libretro.so" | sha256sum -c --quiet - || bad "the core in $ASSET is not the pinned one"
rm -rf "$OUT/lib" "$OUT/share"
mkdir -p "$OUT/lib/libretro" "$OUT/share/licenses"
install -m 755 "$TMP/lib/libretro/hakux-lr_libretro.so" "$OUT/lib/libretro/hakux_libretro.so"
[ ! -f "$TMP/lib/libretro/hakux-lr_libretro.info" ] || install -m 644 "$TMP/lib/libretro/hakux-lr_libretro.info" "$OUT/upstream-hakux-lr_libretro.info"
cp -R "$TMP/share/licenses/hakux-lr" "$OUT/share/licenses/hakux"
[ ! -f "$TMP/BUILD-INFO" ] || install -m 644 "$TMP/BUILD-INFO" "$OUT/BUILD-INFO"
echo "build-hakux-core: $(sha256sum "$OUT/lib/libretro/hakux_libretro.so")"
