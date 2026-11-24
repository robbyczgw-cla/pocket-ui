#!/bin/bash
# The small 7z files of the 7z tests (src/seven_zip.rs), made with 7-Zip (7-Zip 23.01). No real game is in them: the
# "game" is FAKE_ROM's pattern (byte i = (i*31 + i/7) mod 256, 4096 bytes), which the tests compute again.
#   testdata/7z/make.sh      (from the repo root; writes the .7z files next to this script)
set -euo pipefail
out=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
cd "$work"
python3 -c 'import sys; sys.stdout.buffer.write(bytes(((i*31 + i//7) & 255) for i in range(4096)))' > rom.bin
mk() { rm -f "$out/$1"; shift; 7z a -bd -bso0 "$@"; }
mkdir -p solid separate
cp rom.bin "solid/Pocket Demo (World).gba"; printf 'Pocket Demo\nA test file, no game.\n' > solid/About.txt
cp -r solid/. separate/
# Solid (7-Zip's default): one stream, the readme before the game (7-Zip orders by name: About.txt comes first).
(cd solid && mk game.7z "$out/game.7z" About.txt "Pocket Demo (World).gba")
# One stream per file.
(cd separate && mk separate.7z -ms=off "$out/separate.7z" About.txt "Pocket Demo (World).gba")
cp rom.bin Demo.gba
mk lzma.7z -m0=LZMA "$out/lzma.7z" Demo.gba
mk ppmd.7z -m0=PPMd "$out/ppmd.7z" Demo.gba
mk bzip2.7z -m0=BZip2 "$out/bzip2.7z" Demo.gba
mk locked.7z -pPocket -mhe=off "$out/locked.7z" Demo.gba
mk locked-names.7z -pPocket -mhe=on "$out/locked-names.7z" Demo.gba
cp rom.bin a.gba; cp rom.bin b.gba
mk two.7z "$out/two.7z" a.gba b.gba
mk text.7z "$out/text.7z" solid/About.txt
# Over archive::MAX_ROM (64 MB) unpacked: 65 MB of zeros, a few KB packed.
head -c $((65 << 20)) /dev/zero > Big.gba
mk big.7z "$out/big.7z" Big.gba
# A 128 MiB dictionary for a 4 KB game (seven_zip::memory_limit_kb): 7-Zip makes it no bigger than the files, so the
# header's dictionary byte is set by hand (LZMA2:12 -> LZMA2:27) and its two CRCs written again. 7-Zip tests it Ok.
mk big-dictionary.7z -m0=LZMA2 -mhc=off "$work/x.7z" Demo.gba
python3 - "$work/x.7z" "$out/big-dictionary.7z" <<'PY'
import struct, sys, zlib
b = bytearray(open(sys.argv[1], 'rb').read())
offset, size, _ = struct.unpack_from('<QQI', b, 12)
start = 32 + offset
h = bytearray(b[start:start + size])
i = h.find(bytes([0x21, 0x21, 0x01, 0x00]))  # one LZMA2 coder, 1 property byte: 0 (4 KiB)
h[i + 3] = 30                                # (2 | 0) << (30 / 2 + 11) = 128 MiB
b[start:start + size] = h
struct.pack_into('<I', b, 28, zlib.crc32(bytes(h)))
struct.pack_into('<I', b, 8, zlib.crc32(bytes(b[12:32])))
open(sys.argv[2], 'wb').write(b)
PY
ls -l "$out"/*.7z
