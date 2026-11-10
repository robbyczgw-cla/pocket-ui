#!/usr/bin/env python3
"""Build the pinned libretro cheat subset; --optional keeps data outside the card and app archive."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import struct
import tarfile
import urllib.request
import zlib

COMMIT = '4de37d2c8042d9d0f608c43006c9b35438c536ce'  # same database as the ROCKNIX image
ARCHIVE_SHA256 = 'cd8d09c2408e193d2ea4925de60afcf01523fa8cf269c38b5ae5c2580cd56c1f'
SOURCE = f'https://github.com/libretro/libretro-database/archive/{COMMIT}.tar.gz'
# Only systems with a core on the Brick card. Other upstream systems never enter the card.
SYSTEMS = {
    'atari2600': 'Atari - 2600', 'atari7800': 'Atari - 7800', 'atarilynx': 'Atari - Lynx',
    'pc': 'DOS', 'fbn': 'FBNeo - Arcade Games',
    'pcengine': 'NEC - PC Engine - TurboGrafx 16', 'pcenginecd': 'NEC - PC Engine CD - TurboGrafx-CD',
    'supergrafx': 'NEC - PC Engine SuperGrafx', 'fds': 'Nintendo - Family Computer Disk System',
    'gb': 'Nintendo - Game Boy', 'gbc': 'Nintendo - Game Boy Color', 'gba': 'Nintendo - Game Boy Advance',
    'n64': 'Nintendo - Nintendo 64', 'nds': 'Nintendo - Nintendo DS',
    'nes': 'Nintendo - Nintendo Entertainment System', 'snes': 'Nintendo - Super Nintendo Entertainment System',
    'sega32x': 'Sega - 32X', 'dreamcast': 'Sega - Dreamcast', 'gamegear': 'Sega - Game Gear',
    'mastersystem': 'Sega - Master System - Mark III', 'megadrive': 'Sega - Mega Drive - Genesis',
    'segacd': 'Sega - Mega-CD - Sega CD', 'saturn': 'Sega - Saturn',
    'psx': 'Sony - PlayStation', 'psp': 'Sony - PlayStation Portable', 'tic-80': 'TIC-80',
}


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


PACK_MAGIC = b'PUICHT01'
# Staging path and legacy card location. With --optional the builder moves this file OUTSIDE card/ and writes
# database/cheats-download.json; neither the card ZIP nor the OTA's required assets contains the pack.
PACK = 'cores/cht.pack'


def write_pack(path, folders):
    """One file for the whole database; src/cheat_pack.rs documents the layout. folders: {folder: {file name: bytes}}.

    The Brick card is FAT with 32 KB clusters: 23,000 small files took 837 MB and ten minutes to install. Every entry
    is the unchanged .cht text, deflated on its own, so a lookup reads one folder's index and the few entries it needs."""
    names = sorted(folders)
    table = sum(2 + len(n.encode()) + 16 for n in names)
    blobs, indexes = [], []
    for folder in names:
        rows = []
        for name in sorted(folders[folder]):
            text = folders[folder][name]
            packed = zlib.compress(text, 9)
            rows.append((name.encode(), sum(len(b) for b in blobs), len(packed), len(text), zlib.crc32(text) & 0xffffffff))
            blobs.append(packed)
        indexes.append(rows)
    index_len = [sum(2 + len(r[0]) + 20 for r in rows) for rows in indexes]
    blob_base = 12 + table + sum(index_len)
    out = bytearray(PACK_MAGIC + struct.pack('<I', len(names)))
    at = 12 + table
    for folder, rows, length in zip(names, indexes, index_len):
        out += struct.pack('<H', len(folder.encode())) + folder.encode() + struct.pack('<IIQ', len(rows), length, at)
        at += length
    for rows in indexes:
        for name, offset, packed, size, crc in rows:
            out += struct.pack('<H', len(name)) + name + struct.pack('<QIII', blob_base + offset, packed, size, crc)
    for blob in blobs:
        out += blob
    assert len(out) == blob_base + sum(len(b) for b in blobs)
    path.write_bytes(bytes(out))


def read_pack(path):
    """Everything in a pack, for tests: {folder: {file name: bytes}}."""
    data = path.read_bytes()
    assert data[:8] == PACK_MAGIC
    (count,), at, result = struct.unpack_from('<I', data, 8), 12, {}
    table = []
    for _ in range(count):
        (n,) = struct.unpack_from('<H', data, at)
        folder = data[at + 2:at + 2 + n].decode()
        entries, length, offset = struct.unpack_from('<IIQ', data, at + 2 + n)
        table.append((folder, entries, offset))
        at += 2 + n + 16
    for folder, entries, offset in table:
        result[folder] = {}
        for _ in range(entries):
            (n,) = struct.unpack_from('<H', data, offset)
            name = data[offset + 2:offset + 2 + n].decode()
            blob, packed, size, crc = struct.unpack_from('<QIII', data, offset + 2 + n)
            text = zlib.decompress(data[blob:blob + packed])
            assert len(text) == size and zlib.crc32(text) & 0xffffffff == crc
            result[folder][name] = text
            offset += 2 + n + 20
    return result


def install(archive, app, expected_sha=ARCHIVE_SHA256):
    if sha(archive) != expected_sha:
        raise ValueError('libretro-database archive SHA256 mismatch; no data installed')
    database = app / 'database'
    licences = app / 'licences/libretro-database'
    pack = app / PACK
    if database.exists() or licences.exists() or pack.exists():
        raise ValueError('destination database or notices already exist; use a fresh app folder')
    counts, sizes, notices = {}, {}, set()
    packed = {}
    try:
        with tarfile.open(archive, 'r|gz') as upstream:
            for entry in upstream:
                path = PurePosixPath(entry.name)
                parts = path.parts
                if not parts or parts[0] != f'libretro-database-{COMMIT}':
                    raise ValueError('unexpected archive root')
                relative = parts[1:]
                notice = len(relative) == 1 and relative[0] in ('LICENSE', 'README.md')
                cheat = len(relative) >= 3 and relative[0] == 'cht' and relative[1] in SYSTEMS.values() and relative[-1].endswith('.cht')
                if not (notice or cheat):
                    continue
                if path.is_absolute() or '..' in parts or '\\' in entry.name or not entry.isfile():
                    raise ValueError('unsafe cheat or licence entry')
                if notice:
                    target = licences / relative[0]
                    if target.exists():
                        raise ValueError('duplicate cheat or licence entry')
                    target.parent.mkdir(parents=True, exist_ok=True)
                    with upstream.extractfile(entry) as source, target.open('wb') as out:
                        shutil.copyfileobj(source, out)
                    target.chmod(0o644)
                    notices.add(relative[0])
                else:
                    # The pack keeps the file name only; no system has subfolders upstream, and a repeat would hide a file.
                    if len(relative) != 3:
                        raise ValueError('unexpected cheat folder depth')
                    files = packed.setdefault(relative[1], {})
                    if relative[2] in files:
                        raise ValueError('duplicate cheat or licence entry')
                    with upstream.extractfile(entry) as source:
                        files[relative[2]] = source.read()
                    counts[relative[1]] = counts.get(relative[1], 0) + 1
                    sizes[relative[1]] = sizes.get(relative[1], 0) + entry.size
        if notices != {'LICENSE', 'README.md'} or set(counts) != set(SYSTEMS.values()):
            raise ValueError('upstream archive is missing a system or licence notice')
        database.mkdir(parents=True)
        pack.parent.mkdir(parents=True, exist_ok=True)
        write_pack(pack, packed)
        pack.chmod(0o644)
        manifest = {'upstream': 'libretro/libretro-database', 'commit': COMMIT, 'source': SOURCE,
                    'archive_sha256': expected_sha, 'licence': 'CC-BY-SA-4.0',
                    'files': sum(counts.values()), 'bytes': sum(sizes.values()), 'pack': PACK, 'pack_bytes': pack.stat().st_size,
                    'systems': {s: {'folder': d, 'files': counts[d], 'bytes': sizes[d]} for s, d in sorted(SYSTEMS.items())}}
        (database / 'MANIFEST.json').write_text(json.dumps(manifest, sort_keys=True, indent=2) + '\n')
        (licences / 'ATTRIBUTION.txt').write_text(
            f'libretro-database, by the libretro-database contributors.\n'
            f'https://github.com/libretro/libretro-database/tree/{COMMIT}\n'
            f'Upstream commit: {COMMIT}\nSource archive: {SOURCE}\n'
            f'Archive SHA256: {expected_sha}\n'
            'Licence: Creative Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0).\n'
            'https://creativecommons.org/licenses/by-sa/4.0/\n'
            'Full licence and upstream credits/source notes: LICENSE and README.md next to this file.\n'
            'Changes: selected only the systems with a core on the Brick card; each .cht file is unchanged and stored,\n'
            'compressed on its own, in the single file cores/cht.pack.\n'
            'The selected database is provided under CC BY-SA 4.0. No endorsement is implied.\n'
            'Inventory: ../../database/MANIFEST.json; cheat files: ../../cores/cht.pack.\n')
        return manifest
    except Exception:
        pack.unlink(missing_ok=True)
        if database.exists():
            shutil.rmtree(database)
        if licences.exists():
            shutil.rmtree(licences)
        raise


def make_optional(app, output):
    pack = app / PACK
    digest = sha(pack)
    name = f'cheats-{digest}.pack'
    output.mkdir(parents=True, exist_ok=True)
    target = output / name
    pack.replace(target)
    descriptor = {'version': digest, 'size': target.stat().st_size, 'sha256': digest,
                  'url': f'https://pocketui.xyz/updates/brick/assets/{name}'}
    (app / 'database/cheats-download.json').write_text(json.dumps(descriptor, sort_keys=True, indent=2) + '\n')
    inventory = app / 'database/MANIFEST.json'
    manifest = json.loads(inventory.read_text())
    manifest.update({'optional': True, 'pack': 'storage/pocket-ui/cheats/cht.pack', 'pack_base': 'pocket-ui card folder'})
    inventory.write_text(json.dumps(manifest, sort_keys=True, indent=2) + '\n')
    attribution = app / 'licences/libretro-database/ATTRIBUTION.txt'
    attribution.write_text(attribution.read_text().replace('in the single file cores/cht.pack.', 'in one optional pack.')
        .replace('cheat files: ../../cores/cht.pack.', 'download: ../../database/cheats-download.json.\n'
                 'Installed on the card at pocket-ui/storage/pocket-ui/cheats/cht.pack.'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', type=Path)
    parser.add_argument('--optional', type=Path, help='keep raw pack here, outside the card; app gets a download description only')
    parser.add_argument('--archive', type=Path, help='offline copy of the pinned upstream tar.gz')
    args = parser.parse_args()
    archive = args.archive
    if archive is None:
        archive = Path(__file__).resolve().parent.parent / 'downloads' / f'libretro-database-{COMMIT}.tar.gz'
        if not archive.exists():
            archive.parent.mkdir(parents=True, exist_ok=True)
            partial = archive.with_suffix('.part')
            try:
                with urllib.request.urlopen(SOURCE, timeout=120) as source, partial.open('wb') as target:
                    shutil.copyfileobj(source, target)
                if sha(partial) != ARCHIVE_SHA256:
                    raise ValueError('download SHA256 mismatch')
                partial.replace(archive)
            finally:
                partial.unlink(missing_ok=True)
    manifest = install(archive, args.app)
    if args.optional is not None:
        make_optional(args.app, args.optional)
    print(f'Brick cheats: {manifest["files"]} files, {manifest["bytes"]} bytes, {len(SYSTEMS)} systems; {COMMIT}')


if __name__ == '__main__':
    main()
