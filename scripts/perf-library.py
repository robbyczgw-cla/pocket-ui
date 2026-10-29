#!/usr/bin/env python3
"""Deterministic large-library fixture for latency and frame benches (no copyrighted ROMs).

perf-library.py OUT COUNT [WIDTH HEIGHT]
Create COUNT empty GBC files and shared, generated PNG covers (default 1200x1700).
An existing ucity.gbc is retained and counts towards COUNT; copy the homebrew ROM there
before running latency's launch checks. OUT/es_systems.cfg and OUT/data are self-contained.
Never point OUT at a player's library: this is a disposable fixture, not a ROM manager.
"""
import json
from pathlib import Path
import struct
import sys
import zlib


def main():
    root = Path(sys.argv[1]).resolve()
    count = int(sys.argv[2])
    w, h = map(int, sys.argv[3:5]) if len(sys.argv) > 3 else (1200, 1700)
    if count < 2 or not (0 < w <= 4096 and 0 < h <= 4096):
        raise SystemExit('COUNT >= 2; cover dimensions 1..4096')
    roms, data = root / 'roms/gbc', root / 'data'
    covers = data / 'covers/gbc'
    roms.mkdir(parents=True, exist_ok=True)
    covers.mkdir(parents=True, exist_ok=True)
    if any(p.name != 'ucity.gbc' and not p.name.startswith(('Perf Game ', 'zz Perf Game ')) for p in roms.glob('*.gbc')):
        raise SystemExit('not an empty performance fixture')
    for old in list(roms.glob('Perf Game *')) + list(roms.glob('zz Perf Game *')):
        old.unlink()
    for old in covers.glob('*.png'):
        old.unlink()
    (roms / 'ucity.gbc').touch(exist_ok=True)
    for i in range(count - 1):
        (roms / f'zz Perf Game {i:04} (World).gbc').touch()
    def chunk(tag, body):
        return struct.pack('>I', len(body)) + tag + body + struct.pack('>I', zlib.crc32(tag + body))
    raw = b''.join(b'\0' + bytes([y % 256, 80, 160]) * w for y in range(h))
    png = (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>2I5B', w, h, 8, 2, 0, 0, 0))
           + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b''))
    source = root / 'cover.png'
    source.write_bytes(png)
    for rom in roms.glob('*.gbc'):
        (covers / (rom.stem + '.png')).hardlink_to(source)
    # No saved states, favorites or history. Settings are reproducible and disable network downloads.
    (data / 'settings.json').write_text(json.dumps({'theme': 'split', 'appearance': 'dark', 'onboarded': True, 'box_art_only': True}))
    # Drop only the fixture's cached library when changing its size.
    for cache in data.glob('library-cache*'):
        cache.unlink()
    (root / 'es_systems.cfg').write_text(
        f'<systemList><system><name>gbc</name><fullname>Game Boy Color</fullname>'
        f'<path>{roms}</path><extension>.gbc</extension><command>true</command></system></systemList>')
    print(f'{count} games; covers {w}x{h}; POCKETUI_ES_SYSTEMS={root}/es_systems.cfg POCKETUI_DATA={data}')


if __name__ == '__main__':
    main()
