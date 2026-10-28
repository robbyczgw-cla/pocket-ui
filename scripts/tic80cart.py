#!/usr/bin/env python3
"""Turns a TIC-80 text cart (a .lua file with -- <TILES> ... sections, as the demos in TIC-80's own repository are
kept) into a binary .tic cartridge, which is what the tic80 libretro core loads.

    scripts/tic80cart.py tetris.lua tetris.tic

Only what the demos use: code, tiles, sprites, map, waves, sfx, patterns, tracks, palette (bank 0).
Chunk layout as in TIC-80's src/cart.c: 1 byte type | bank << 5, 2 bytes size (little endian), 1 reserved byte.
"""
import re
import sys

CHUNK_CODE = 5
# section tag -> (chunk type, bytes per row, swap nibbles), as TIC-80's studio/project.c writes them
SECTIONS = {
    "TILES": (1, 32, True),
    "SPRITES": (2, 32, True),
    "MAP": (4, 240, True),
    "SFX": (9, 66, True),
    "WAVES": (10, 16, True),
    "PALETTE": (12, 48, False),
    "PATTERNS": (15, 192, True),
    "TRACKS": (14, 51, True),
}


def chunk(kind, data):
    assert 0 < len(data) <= 0xFFFF, (kind, len(data))
    return bytes([kind & 0x1F, len(data) & 0xFF, len(data) >> 8, 0]) + data


def convert(text):
    code_lines, out, section, rows = [], b"", None, {}
    for line in text.splitlines():
        m = re.match(r"^-- <(/?)([A-Z0-9]+)>\s*$", line)
        if m and m.group(2) in SECTIONS:
            if m.group(1):
                kind, size, swap = SECTIONS[section]
                buf = bytearray(size * (max(rows) + 1))
                for i, hexrow in rows.items():
                    raw = bytes.fromhex(hexrow.ljust(size * 2, "0")[: size * 2])
                    if swap:
                        raw = bytes(((b & 0xF) << 4) | (b >> 4) for b in raw)
                    buf[i * size : (i + 1) * size] = raw
                out += chunk(kind, bytes(buf).rstrip(b"\0") or b"\0")
                section, rows = None, {}
            else:
                section = m.group(2)
            continue
        if section:
            r = re.match(r"^-- (\d{3}):([0-9a-fA-F]+)", line)
            if r:
                rows[int(r.group(1))] = r.group(2)
            continue
        code_lines.append(line)
    code = "\n".join(code_lines).rstrip() + "\n"
    return chunk(CHUNK_CODE, code.encode()) + out


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    with open(sys.argv[1], encoding="utf-8") as f:
        data = convert(f.read())
    with open(sys.argv[2], "wb") as f:
        f.write(data)
    print(f"{sys.argv[2]}: {len(data)} bytes")
