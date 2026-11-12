#!/usr/bin/env python3
"""Take every trace of test games out of a pocket-ui data folder (the owner's own history).

A game a test starts through the owner's pocket-ui (`play <system> <file>` below `.pocket-ui-test`, or any helper that
puts a ROM there) lands in Zuletzt gespielt / Continue (recents.json), the shelf and its play time (library.json),
Curator -> History (play-log.bin) and Settings -> Playing -> Battery (battery-games.json). Restoring a backup under a
running pocket-ui does not help: it writes its memory back at the next change. So the data is cleaned while the UI is
stopped, and the UI is started again afterwards (the caller's job).

    device_scrub.py DATA_DIR [--dry-run] [--marker TEXT ...]    -> prints what it takes out, one line per file

A path counts as a test path when it contains one of the markers (default: `.pocket-ui-test`). Names of a removed
game's file (`pocketcheck.gba`) are taken out of battery-games.json too. Nothing else is touched: every file is
rewritten only when something in it matched, atomically (write next to it, rename), and a copy of the original stays as
`<name>.scrub-orig` unless --no-keep. The file formats: docs/DEVELOPING.md / src/play_log.rs (frames: u8 kind, u16 length,
payload, u32 CRC-32 of kind, length and payload).
"""
import json
import os
import struct
import sys
import zlib
from pathlib import Path

DEFAULT_MARKERS = ('.pocket-ui-test',)
HEADER = 8
# frame kinds whose payload carries the path as a name id, and where (byte offset in the payload)
PATH_AT = {2: 6, 3: 10, 4: 6, 5: 0}
NAME_KINDS = {2: (6, 10, 14, 18), 3: (10, 14, 18), 4: (6, 10, 14), 5: (0, 4, 8)}  # path, name, system, emulator ids
SESSION_END = {2: 22, 3: 22}  # offset of the session number (frames from 2026-10-24 on)


def is_test(text, markers):
    return any(m in text for m in markers)


def frames_of(data):
    """(offset, kind, payload) for every intact frame; stops at a torn tail."""
    out = []
    i = HEADER
    while i + 3 <= len(data):
        kind = data[i]
        length = struct.unpack_from('<H', data, i + 1)[0]
        end = i + 3 + length + 4
        if end > len(data):
            break
        crc = struct.unpack_from('<I', data, i + 3 + length)[0]
        if zlib.crc32(data[i:i + 3 + length]) & 0xffffffff == crc:
            out.append((i, kind, data[i + 3:i + 3 + length]))
        i = end
    return out


def pack(kind, payload):
    head = bytes([kind]) + struct.pack('<H', len(payload)) + payload
    return head + struct.pack('<I', zlib.crc32(head) & 0xffffffff)


def scrub_play_log(data, markers):
    """The log without the frames of test games. Returns (new bytes, removed session count, removed frame count)."""
    frames = frames_of(data)
    names = {}
    for _, kind, p in frames:
        if kind == 1 and len(p) >= 4:
            names[struct.unpack_from('<I', p)[0]] = p[4:].decode('utf-8', 'replace')
    test_ids = {i for i, t in names.items() if is_test(t, markers)}
    if not test_ids:
        return data, 0, 0
    test_sessions = set()
    for _, kind, p in frames:
        if kind in PATH_AT and len(p) >= PATH_AT[kind] + 4 and struct.unpack_from('<I', p, PATH_AT[kind])[0] in test_ids:
            if kind in SESSION_END and len(p) >= SESSION_END[kind] + 4:
                test_sessions.add(struct.unpack_from('<I', p, SESSION_END[kind])[0])
    keep, removed, sessions = [], 0, 0
    for _, kind, p in frames:
        drop = False
        if kind in PATH_AT and len(p) >= PATH_AT[kind] + 4 and struct.unpack_from('<I', p, PATH_AT[kind])[0] in test_ids:
            drop = True
            sessions += kind == 2
        elif kind in (7, 8) and len(p) >= 4 and struct.unpack_from('<I', p)[0] in test_sessions:
            drop = True
        if drop:
            removed += 1
        else:
            keep.append((kind, p))
    # A name that no kept frame refers to any more goes too (the test game's path and file name).
    used = set()
    for kind, p in keep:
        if kind in NAME_KINDS:
            for off in NAME_KINDS[kind]:
                if len(p) >= off + 4:
                    used.add(struct.unpack_from('<I', p, off)[0])
    out = bytearray(data[:HEADER])
    for kind, p in keep:
        if kind == 1 and len(p) >= 4:
            ident = struct.unpack_from('<I', p)[0]
            if ident in names and (ident in test_ids or (ident not in used and is_test_name(names[ident], names, test_ids))):
                removed += 1
                continue
        out += pack(kind, p)
    return bytes(out), sessions, removed


def is_test_name(text, names, test_ids):
    """A short name (`pocketcheck`) belongs to a test game when the path of a removed game ends in it."""
    base = {os.path.splitext(os.path.basename(names[i]))[0] for i in test_ids}
    return text in base or text.title() in {b.title() for b in base}


def scrub_json_list(path, markers, key_of):
    """A JSON list: the entries whose key_of(entry) is a test path go. Returns (text or None, removed names)."""
    data = json.loads(path.read_text())
    keep = [e for e in data if not is_test(key_of(e), markers)]
    if len(keep) == len(data):
        return None, []
    gone = [e for e in data if is_test(key_of(e), markers)]
    return json.dumps(keep, indent=2) + '\n', [key_of(e) for e in gone]


def atomic_write(path, data, keep):
    if keep:
        orig = path.with_name(path.name + '.scrub-orig')
        if not orig.exists():
            orig.write_bytes(path.read_bytes())
    tmp = path.with_name(path.name + '.scrub-tmp')
    tmp.write_bytes(data if isinstance(data, bytes) else data.encode())
    os.replace(tmp, path)


def scrub(data_dir, markers=DEFAULT_MARKERS, dry_run=False, keep=True):
    """Clean a data folder. Returns a list of (file, what) lines for the report."""
    d = Path(data_dir)
    report = []
    gone_names = set()

    def note(f, what):
        report.append((f, what))

    for name in ('recents.json', 'recents.json.bak'):
        p = d / name
        if p.exists():
            text, gone = scrub_json_list(p, markers, lambda e: e.get('path', ''))
            if text is not None:
                gone_names.update(os.path.basename(g) for g in gone)
                note(name, '%d entr%s: %s' % (len(gone), 'y' if len(gone) == 1 else 'ies', ', '.join(sorted(set(gone)))))
                if not dry_run:
                    atomic_write(p, text, keep)
    for name in ('library.json', 'library.json.bak'):
        p = d / name
        if p.exists():
            doc = json.loads(p.read_text())
            games = doc.get('games', {})
            gone = [k for k in games if is_test(k, markers)]
            if gone:
                gone_names.update(os.path.basename(g) for g in gone)
                for k in gone:
                    del games[k]
                note(name, '%d game%s: %s' % (len(gone), '' if len(gone) == 1 else 's', ', '.join(sorted(gone))))
                if not dry_run:
                    atomic_write(p, json.dumps(doc, indent=2) + '\n', keep)
    for name in ('battery-games.json', 'battery-games.json.bak'):
        p = d / name
        if p.exists():
            doc = json.loads(p.read_text())
            games = doc.get('games', [])
            gone = [g for g in games if g.get('game') in gone_names]
            if gone:
                doc['games'] = [g for g in games if g.get('game') not in gone_names]
                note(name, '%d entr%s: %s' % (len(gone), 'y' if len(gone) == 1 else 'ies', ', '.join(sorted({g['game'] for g in gone}))))
                if not dry_run:
                    atomic_write(p, json.dumps(doc, indent=2) + '\n', keep)
    p = d / 'play-log.bin'
    if p.exists():
        data = p.read_bytes()
        new, sessions, removed = scrub_play_log(data, markers)
        if removed:
            note('play-log.bin', '%d session%s (%d frames)' % (sessions, '' if sessions == 1 else 's', removed))
            if not dry_run:
                atomic_write(p, new, keep)
    return report


def main(argv):
    args = [a for a in argv if not a.startswith('--')]
    if len(args) != 1:
        print(__doc__)
        return 2
    markers = []
    it = iter(argv)
    for a in it:
        if a == '--marker':
            markers.append(next(it))
    rep = scrub(args[0], tuple(markers) or DEFAULT_MARKERS, dry_run='--dry-run' in argv, keep='--no-keep' not in argv)
    for f, what in rep:
        print('%s: %s' % (f, what))
    if not rep:
        print('nothing to take out')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
