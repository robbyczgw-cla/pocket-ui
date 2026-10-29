#!/bin/sh
# The source archives that go next to the SD card image (deploy/image/SOURCE-OFFER.txt.in lists them for players).
#
#   scripts/image/source-bundle.sh OUT --version V --rocknix-source DIR --pocket-ui GIT_DIR REF
#     OUT               output directory (about 10 GB, most of it ROCKNIX's sources)
#     --rocknix-source  the output of scripts/image/rocknix-source.sh (must have packed, with an empty failed.txt)
#     --pocket-ui       the repository and the commit or tag whose tree is the image's pocket-ui source
#                       (for a public image: the public repository and the release's commit)
#
# Writes into OUT: pocket-ui-V-source.tar.gz, rocknix-<tag>-distribution.tar.xz, rocknix-<tag>-SM8250-sources.tar.xz.part-*,
# packages.txt, regba-bios-source.tar.gz, grub2_<version>.dsc with its .orig and .debian tarballs, SOURCE-SHA256SUMS.
# Every download is checked: Debian's files against the .dsc (sha256), ReGBA's BIOS against REGBA_BIOS_MD5.
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
. "$ROOT/scripts/image/pins.sh"
# GRUB's Debian source as in BOOTAA64.EFI (boot/grub/grub-efi-arm64-bin.version), and ReGBA's commit with the BIOS.
GRUB_DEB=2.12-9+deb13u2
REGBA_COMMIT=7f379052dc14366e622a96c7cf7fb6b5a7e918a3

[ $# -ge 1 ] || { sed -n '4,8p' "$0" >&2; exit 2; }
OUT=$1
shift
VERSION=
RS=
PU_GIT=
PU_REF=
while [ $# -gt 0 ]; do
  case $1 in
    --version) VERSION=$2; shift ;;
    --rocknix-source) RS=$2; shift ;;
    --pocket-ui) PU_GIT=$2; PU_REF=$3; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
die() { echo "source-bundle: $*" >&2; exit 1; }
[ -n "$VERSION" ] && [ -n "$RS" ] && [ -n "$PU_GIT" ] || die "--version, --rocknix-source and --pocket-ui are needed"
[ -f "$RS/SHA256SUMS" ] || die "$RS has no SHA256SUMS: run scripts/image/rocknix-source.sh first"
# Packages whose source scripts/get cannot fetch, allowed only because nothing of them is on the card:
#   rocknix-abl  ROCKNIX's ABL binary (its release asset no longer matches the pinned sha256); removed from SYSTEM
#                (pins.sh SYSTEM_REMOVE), so the image does not distribute it.
NOT_ON_CARD="rocknix-abl"
for p in $(cat "$RS/failed.txt"); do
  case " $NOT_ON_CARD " in *" $p "*) echo "not fetched, and not on the card: $p" ;; *) die "$RS/failed.txt: $p could not be fetched" ;; esac
done
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)

echo "== ROCKNIX $ROCKNIX_TAG (from $RS)"
(cd "$RS" && sha256sum -c --quiet SHA256SUMS) || die "$RS/SHA256SUMS does not match"
for f in "$RS/rocknix-$ROCKNIX_TAG-distribution.tar.xz" "$RS/rocknix-$ROCKNIX_TAG-SM8250-sources.tar.xz.part-"* "$RS/packages.txt"; do
  ln -f "$f" "$OUT/" 2>/dev/null || cp "$f" "$OUT/"
done

echo "== pocket-ui ($PU_REF)"
git -C "$PU_GIT" archive --format=tar.gz --prefix="pocket-ui-$VERSION/" "$PU_REF" >"$OUT/pocket-ui-$VERSION-source.tar.gz"

echo "== ReGBA's GBA BIOS ($REGBA_COMMIT)"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
git clone -q --filter=blob:none --no-checkout https://github.com/Nebuleon/ReGBA.git "$T/regba"
git -C "$T/regba" checkout -q "$REGBA_COMMIT"
[ "$(md5sum "$T/regba/bios/gba_bios.bin" | cut -d' ' -f1)" = "$REGBA_BIOS_MD5" ] || die "ReGBA's bios/gba_bios.bin is not the one in SYSTEM"
git -C "$T/regba" archive --format=tar.gz --prefix=regba-bios/ "$REGBA_COMMIT" bios README.md >"$OUT/regba-bios-source.tar.gz"

echo "== GRUB, Debian source package grub2 $GRUB_DEB"
POOL=https://snapshot.debian.org/archive/debian/20260601T000000Z/pool/main/g/grub2
DSC=grub2_$GRUB_DEB.dsc
for base in https://deb.debian.org/debian/pool/main/g/grub2 "$POOL"; do
  curl -sfL -o "$OUT/$DSC" "$base/$DSC" && break
done
[ -s "$OUT/$DSC" ] || die "could not download $DSC"
# The .dsc's Checksums-Sha256 lines: sha256 size name.
sed -n '/^Checksums-Sha256:/,/^[^ ]/p' "$OUT/$DSC" | sed -n 's/^ \([0-9a-f]\{64\}\) [0-9]* \(.*\)$/\1 \2/p' >"$T/grub.sums"
[ -s "$T/grub.sums" ] || die "$DSC lists no files"
while read -r sum name; do
  for base in https://deb.debian.org/debian/pool/main/g/grub2 "$POOL"; do
    [ "$(sha256sum "$OUT/$name" 2>/dev/null | cut -d' ' -f1)" = "$sum" ] && break
    curl -sfL -o "$OUT/$name" "$base/$name" || true
  done
  [ "$(sha256sum "$OUT/$name" | cut -d' ' -f1)" = "$sum" ] || die "$name does not match $DSC"
done <"$T/grub.sums"

cd "$OUT"
rm -f SOURCE-SHA256SUMS
sha256sum ./* | sed 's# \./# #' >"$T/sums"
mv "$T/sums" SOURCE-SHA256SUMS
ls -l "$OUT"
du -sh "$OUT"
