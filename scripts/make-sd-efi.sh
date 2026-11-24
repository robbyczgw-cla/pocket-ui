#!/bin/sh
# Builds the SD boot files for the dual boot: Boot mode: Loader -> Retroid u-boot (loader) -> GRUB -> pocket-ui.
# Any other UEFI firmware that starts EFI/BOOT/BOOTAA64.EFI from the card takes the same GRUB path; OpenABL
# ("Boot mode: Linux") reads loader/entries/pocket-ui.conf first and starts the same kernel, tree and command line.
#   scripts/make-sd-efi.sh KERNEL miniv2-noufs.dtb OUTDIR
# KERNEL is ROCKNIX's Android boot image from the SD (/flash/KERNEL); the arm64 Image inside it is unpacked.
# GRUB comes from Debian's grub-efi-arm64-bin and grub-common in the versions pinned in scripts/image/pins.sh (GRUB_DEB_*,
# Docker), downloaded once into GRUB_DEB_CACHE (default ~/.cache/pocket-ui/grub-<version>) and checked by sha256 before
# use. Nothing is written to the device.
# OUTDIR gets the SD layout: EFI/BOOT/BOOTAA64.EFI, boot/grub/{grub.cfg,grub-efi-arm64-bin.version},
# boot/pocket/{Image,miniv2-noufs.dtb,KERNEL.source.md5}, loader/entries/pocket-ui.conf. scripts/make-image.sh uses it for
# the whole SD image.
set -eu
[ $# -eq 3 ] || { echo "usage: $0 KERNEL DTB OUTDIR" >&2; exit 2; }
KERNEL=$1 DTB=$2 OUT=$3
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/image/pins.sh"
die() { echo "make-sd-efi: $*" >&2; exit 1; }
sha256() { shasum -a 256 "$1" 2>/dev/null | cut -d' ' -f1; }

# The container runs on the build machine's own architecture: an arm64 one does not start on an x86_64 machine without
# qemu ("exec format error"). grub-mkimage (grub-common) builds arm64-efi images on any architecture, from the arm64
# modules of Debian's grub-efi-arm64-bin, unpacked here rather than installed. Both architectures give the same
# BOOTAA64.EFI, byte for byte the device's (2.12-9+deb13u2, checked on x86_64 and arm64 on 2026-10-26).
ARCH=$(docker version -f '{{.Server.Arch}}')
case $ARCH in
  amd64) GC_SHA256=$GRUB_COMMON_AMD64_DEB_SHA256 ;;
  arm64) GC_SHA256=$GRUB_COMMON_ARM64_DEB_SHA256 ;;
  *) die "no pinned grub-common for the Docker architecture '$ARCH' (scripts/image/pins.sh)" ;;
esac

# The pinned packages first, never the newest the archive has: on 2026-11-22 trixie had 2.12-9+deb13u4, whose BOOTAA64.EFI
# is not the device's. A cached file is used only with the pinned sha256; anything else is fetched again and checked.
GE=grub-efi-arm64-bin_${GRUB_DEB_VERSION}_arm64.deb
GC=grub-common_${GRUB_DEB_VERSION}_$ARCH.deb
DEBS=${GRUB_DEB_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/pocket-ui/grub-$GRUB_DEB_VERSION}
mkdir -p "$DEBS"
DEBS=$(cd "$DEBS" && pwd)
for pair in "$GE $GRUB_EFI_ARM64_DEB_SHA256" "$GC $GC_SHA256"; do
  f=${pair% *} want=${pair#* }
  [ "$(sha256 "$DEBS/$f")" != "$want" ] || continue
  rm -f "$DEBS/$f.part"
  if curl -fsSL --retry 3 -A pocket-ui-image-build/0.1 -o "$DEBS/$f.part" "$GRUB_DEB_URL/$(echo "$f" | sed 's/+/%2B/g')" &&
    [ "$(sha256 "$DEBS/$f.part")" = "$want" ]; then
    mv "$DEBS/$f.part" "$DEBS/$f"
  else
    rm -f "$DEBS/$f.part"
    die "$f: no file with the pinned sha256 $want (scripts/image/pins.sh) in $DEBS or at $GRUB_DEB_URL"
  fi
done

mkdir -p "$OUT/EFI/BOOT" "$OUT/boot/grub" "$OUT/boot/pocket" "$OUT/loader/entries"
OUT=$(cd "$OUT" && pwd)

python3 - "$KERNEL" "$OUT/boot/pocket/Image" <<'EOF'
import struct, sys, zlib
d = open(sys.argv[1], 'rb').read()
if d[:8] != b'ANDROID!':
    sys.exit('KERNEL is not an Android boot image')
size, page = struct.unpack('<I', d[8:12])[0], struct.unpack('<I', d[36:40])[0]
payload = d[page:page + size]
# A gzip member followed by the appended device trees: decompress only the first member.
image = zlib.decompressobj(31).decompress(payload) if payload[:2] == b'\x1f\x8b' else payload
if image[:2] != b'MZ' or image[56:60] != b'ARM\x64':
    sys.exit('no arm64 EFI-stub Image inside KERNEL')
open(sys.argv[2], 'wb').write(image)
EOF
cp "$DTB" "$OUT/boot/pocket/miniv2-noufs.dtb"
md5 -q "$KERNEL" 2>/dev/null >"$OUT/boot/pocket/KERNEL.source.md5" || md5sum "$KERNEL" | cut -d' ' -f1 >"$OUT/boot/pocket/KERNEL.source.md5"
cp "$HERE/../deploy/sd-efi/grub.cfg" "$OUT/boot/grub/grub.cfg"
cp "$HERE/../deploy/sd-efi/loader/entries/pocket-ui.conf" "$OUT/loader/entries/pocket-ui.conf"

docker run --rm --platform "linux/$ARCH" -v "$DEBS:/debs:ro" -v "$OUT/EFI/BOOT:/out" -e V="$GRUB_DEB_VERSION" -e GE="$GE" -e GC="$GC" \
  debian:trixie-slim sh -c '
  set -e
  # The pinned grub-common from its file; only the libraries it needs come from the archive.
  apt-get update -qq >/dev/null && apt-get install -y -qq --no-install-recommends "/debs/$GC" >/dev/null 2>&1
  [ "$(dpkg-query -W -f "\${Version}" grub-common)" = "$V" ] || { echo "grub-common is not version $V" >&2; exit 1; }
  deb=/debs/$GE
  [ "$(dpkg-deb -f "$deb" Version)" = "$V" ] || { echo "$GE is not version $V" >&2; exit 1; }
  dpkg -x "$deb" /tmp/grub
  grub-mkimage -d /tmp/grub/usr/lib/grub/arm64-efi -O arm64-efi -p /boot/grub -o /out/BOOTAA64.EFI \
    boot linux ext2 fat part_msdos part_gpt normal search search_fs_file search_fs_uuid search_label \
    reboot loadenv test fdt echo sleep efi_gop gfxterm
  # Source is "grub2", or "grub2 (<version>)" when the binary version differs from the source version.
  src=$(dpkg-deb -f "$deb" Source)
  sv=$(echo "$src" | sed -n "s/.*(\\(.*\\)).*/\\1/p")
  echo "grub-efi-arm64-bin $V (source ${src%% *} ${sv:-$V})" >/out/grub-version'
# Which GRUB this is: GPL source offer (scripts/make-image.sh writes it into the image notes).
mv "$OUT/EFI/BOOT/grub-version" "$OUT/boot/grub/grub-efi-arm64-bin.version"
(cd "$OUT" && find . -type f ! -name SHA256SUMS | sort | xargs shasum -a 256 >SHA256SUMS)
cat "$OUT/SHA256SUMS"
