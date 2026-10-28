#!/bin/sh
# Builds the SD boot files for the dual boot: Boot mode: Loader -> Retroid u-boot (loader) -> GRUB -> pocket-ui.
#   scripts/make-sd-efi.sh KERNEL miniv2-noufs.dtb OUTDIR
# KERNEL is ROCKNIX's Android boot image from the SD (/flash/KERNEL); the arm64 Image inside it is unpacked.
# GRUB comes from Debian's grub-efi-arm64-bin (Docker, arm64). Nothing is written to the device.
# OUTDIR gets the SD layout: EFI/BOOT/BOOTAA64.EFI, boot/grub/{grub.cfg,grub-efi-arm64-bin.version},
# boot/pocket/{Image,miniv2-noufs.dtb,KERNEL.source.md5}. scripts/make-image.sh uses it for the whole SD image.
set -eu
[ $# -eq 3 ] || { echo "usage: $0 KERNEL DTB OUTDIR" >&2; exit 2; }
KERNEL=$1 DTB=$2 OUT=$3
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$OUT/EFI/BOOT" "$OUT/boot/grub" "$OUT/boot/pocket"
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

# The container runs on the build machine's own architecture: an arm64 one does not start on an x86_64 machine without
# qemu ("exec format error"). grub-mkimage (grub-common) builds arm64-efi images on any architecture, from the arm64
# modules of Debian's grub-efi-arm64-bin, unpacked here rather than installed. Both architectures give the same
# BOOTAA64.EFI, byte for byte the device's (2.12-9+deb13u2, checked on x86_64 and arm64 on 2026-10-26).
ARCH=$(docker version -f '{{.Server.Arch}}')
docker run --rm --platform "linux/$ARCH" -v "$OUT/EFI/BOOT:/out" debian:trixie-slim sh -c '
  set -e
  dpkg --add-architecture arm64
  apt-get update -qq >/dev/null && apt-get install -y -qq --no-install-recommends grub-common >/dev/null 2>&1
  cd /tmp && apt-get download -qq grub-efi-arm64-bin:arm64 >/dev/null 2>&1
  deb=$(ls /tmp/grub-efi-arm64-bin_*_arm64.deb)
  dpkg -x "$deb" /tmp/grub
  v=$(dpkg-deb -f "$deb" Version)
  [ "$(dpkg-query -W -f "\${Version}" grub-common)" = "$v" ] || { echo "grub-common is not version $v" >&2; exit 1; }
  grub-mkimage -d /tmp/grub/usr/lib/grub/arm64-efi -O arm64-efi -p /boot/grub -o /out/BOOTAA64.EFI \
    boot linux ext2 fat part_msdos part_gpt normal search search_fs_file search_fs_uuid search_label \
    reboot loadenv test fdt echo sleep efi_gop gfxterm
  # Source is "grub2", or "grub2 (<version>)" when the binary version differs from the source version.
  src=$(dpkg-deb -f "$deb" Source)
  sv=$(echo "$src" | sed -n "s/.*(\\(.*\\)).*/\\1/p")
  echo "grub-efi-arm64-bin $v (source ${src%% *} ${sv:-$v})" >/out/grub-version'
# Which GRUB this is: GPL source offer (scripts/make-image.sh writes it into the image notes).
mv "$OUT/EFI/BOOT/grub-version" "$OUT/boot/grub/grub-efi-arm64-bin.version"
(cd "$OUT" && find . -type f ! -name SHA256SUMS | sort | xargs shasum -a 256 >SHA256SUMS)
cat "$OUT/SHA256SUMS"
