#!/usr/bin/env python3
"""Host test of device_scrub.py: a test game's traces leave recents, library, battery-games and the play log; nothing else changes."""
import json, os, struct, sys, tempfile, unittest, zlib
from pathlib import Path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import device_scrub as ds

TEST = '/mnt/SDCARD/.pocket-ui-test/device-check/rom/pocketcheck.gba'
REAL = '/storage/roms/gba/Real Game.gba'


def name(i, text): return ds.pack(1, struct.pack('<I', i) + text.encode())
def start(t, path, nm, system, emu, session): return ds.pack(2, struct.pack('<IhIIIII', t, 60, path, nm, system, emu, session))
def end(t, secs, path, nm, system, session): return ds.pack(3, struct.pack('<IhIIIII', t, 60, secs, path, nm, system, session))
def pause(session): return ds.pack(7, struct.pack('<IIII', session, 0, 5, 3))


class Scrub(unittest.TestCase):
    def fixture(self, d):
        d = Path(d)
        (d / 'recents.json').write_text(json.dumps([{'system': 'gba', 'name': 'Real Game', 'path': REAL}, {'system': 'gba', 'name': 'Pocketcheck', 'path': TEST}], indent=2) + '\n')
        (d / 'library.json').write_text(json.dumps({'version': 2, 'games': {REAL: {'secs': 9}, TEST: {'secs': 132}}}, indent=2) + '\n')
        (d / 'battery-games.json').write_text(json.dumps({'games': [{'game': 'Real Game.gba', 'secs': 5.0}, {'game': 'pocketcheck.gba', 'secs': 60.0}]}, indent=2) + '\n')
        log = bytearray(b'PPL1\x01\x00\x00\x00')
        for f in (name(1, 'gba'), name(2, 'pocket-core'), name(3, REAL), name(4, 'Real Game'), name(5, TEST), name(6, 'pocketcheck'),
                  start(100, 3, 4, 1, 2, 1), end(100, 50, 3, 4, 1, 1),
                  start(200, 5, 6, 1, 2, 2), pause(2), end(200, 40, 5, 6, 1, 2)):
            log += f
        (d / 'play-log.bin').write_bytes(bytes(log))
        return d

    def test_a_test_game_leaves_every_list_and_the_real_game_stays(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.fixture(t)
            before_real = json.loads((d / 'library.json').read_text())['games'][REAL]
            report = ds.scrub(d, keep=False)
            self.assertEqual(sorted(f for f, _ in report), ['battery-games.json', 'library.json', 'play-log.bin', 'recents.json'])
            self.assertEqual([e['path'] for e in json.loads((d / 'recents.json').read_text())], [REAL])
            self.assertEqual(json.loads((d / 'library.json').read_text())['games'], {REAL: before_real})
            self.assertEqual([g['game'] for g in json.loads((d / 'battery-games.json').read_text())['games']], ['Real Game.gba'])
            data = (d / 'play-log.bin').read_bytes()
            frames = ds.frames_of(data)
            self.assertEqual([k for _, k, _ in frames if k in (2, 3, 7, 8)], [2, 3], 'the real session stays, the test session and its pause go')
            self.assertNotIn(b'pocketcheck', data)
            self.assertNotIn(b'pocket-ui-test', data)
            self.assertIn(b'Real Game', data)

    def test_nothing_to_remove_touches_nothing(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.fixture(t)
            ds.scrub(d, keep=False)
            before = {p.name: p.read_bytes() for p in d.iterdir()}
            self.assertEqual(ds.scrub(d, keep=False), [])
            self.assertEqual(before, {p.name: p.read_bytes() for p in d.iterdir()})

    def test_dry_run_changes_nothing_and_a_copy_of_the_original_is_kept(self):
        with tempfile.TemporaryDirectory() as t:
            d = self.fixture(t)
            before = {p.name: p.read_bytes() for p in d.iterdir()}
            self.assertTrue(ds.scrub(d, dry_run=True))
            self.assertEqual(before, {p.name: p.read_bytes() for p in d.iterdir()})
            ds.scrub(d)
            self.assertEqual((d / 'recents.json.scrub-orig').read_bytes(), before['recents.json'])


if __name__ == '__main__':
    unittest.main()
