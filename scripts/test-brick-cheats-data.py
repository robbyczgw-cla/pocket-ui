#!/usr/bin/env python3
"""Cheats must reach a new card and OTA unchanged, without unrelated systems."""
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
import xml.etree.ElementTree as ET

spec = importlib.util.spec_from_file_location('data', Path(__file__).with_name('brick-cheats-data.py'))
data = importlib.util.module_from_spec(spec)
spec.loader.exec_module(data)
pack_spec = importlib.util.spec_from_file_location('pack', Path(__file__).with_name('brick-update-pack.py'))
pack = importlib.util.module_from_spec(pack_spec)
pack_spec.loader.exec_module(pack)


class BrickCheats(unittest.TestCase):
    def fixture(self, root):
        archive = root / 'upstream.tar.gz'
        self.code = b'cheats = 1\ncheat0_desc = "Infinite lives"\ncheat0_code = "0123ABCD"\n'
        # Real data includes filenames over ustar's 100-byte basename limit.
        self.game = 'A long game title ' + 'x' * 100 + ' (USA).cht'
        with tarfile.open(archive, 'w:gz') as out:
            files = {'LICENSE': b'upstream licence', 'README.md': b'upstream credits',
                     'cht/Sony - PlayStation 2/Unrelated.cht': b'not supported by this card'}
            files.update({f'cht/{folder}/{self.game}': self.code for folder in data.SYSTEMS.values()})
            for relative, content in files.items():
                entry = tarfile.TarInfo(f'libretro-database-{data.COMMIT}/{relative}')
                entry.size = len(content)
                out.addfile(entry, io.BytesIO(content))
        return archive

    def test_card_payload_and_ota_retain_cheats_and_notices(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            archive, app = self.fixture(root), root / 'card/pocket-ui/app'
            manifest = data.install(archive, app, data.sha(archive))
            self.assertEqual(manifest['files'], len(data.SYSTEMS))
            config = Path(__file__).resolve().parent.parent / 'deploy/brick/es_systems.cfg'
            card_systems = {s.findtext('name') for s in ET.parse(config).getroot().findall('system')}
            self.assertFalse(set(data.SYSTEMS) - card_systems, 'every shipped database belongs to a playable card system')
            self.assertEqual((app / 'licences/libretro-database/README.md').read_bytes(), b'upstream credits')
            # One file instead of a file per cheat list (23,000 small files were 837 MB on the card's FAT).
            self.assertFalse((app / 'database/cht').exists())
            packed = data.read_pack(app / data.PACK)
            self.assertEqual(set(packed), set(data.SYSTEMS.values()))
            for folder in data.SYSTEMS.values():
                self.assertEqual(packed[folder], {self.game: self.code})
            self.assertEqual(manifest['pack_bytes'], (app / data.PACK).stat().st_size)
            (app / 'cores/test.so').write_bytes(b'core')
            pack.prepare(app, root / 'release', root / 'channel', 'https://example.org/release', '1')
            with tarfile.open(root / 'channel/pocket-ui-brick-1.tar.gz') as ota:
                names = ota.getnames()
                # The database is an asset package of its own: downloaded once, not part of the app tarball.
                self.assertNotIn(data.PACK, names)
                assets = {a['path']: a for a in json.load(ota.extractfile('asset-packages.json'))['assets']}
                self.assertEqual(assets[data.PACK]['size'], (app / data.PACK).stat().st_size)
                self.assertEqual(assets[data.PACK]['sha256'], data.sha(app / data.PACK))
                self.assertFalse([n for n in names if n.startswith('database/cht')])
                self.assertEqual(ota.extractfile('licences/libretro-database/LICENSE').read(), b'upstream licence')
                self.assertIn('CC BY-SA 4.0', ota.extractfile('licences/libretro-database/ATTRIBUTION.txt').read().decode())
                self.assertEqual(json.load(ota.extractfile('database/MANIFEST.json'))['commit'], data.COMMIT)

    def test_pack_is_byte_for_byte_the_files_and_stable(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / 'cht.pack'
            files = {'Nintendo - Game Boy': {'B (USA).cht': b'b' * 5000, 'A.cht': 'cheat0_desc = "Pok\u00e9"\n'.encode(), 'Empty.cht': b''},
                     'Sega - Saturn': {'N.cht': b'n'}}
            data.write_pack(path, files)
            first = path.read_bytes()
            self.assertEqual(data.read_pack(path), files)
            data.write_pack(path, {k: dict(reversed(list(v.items()))) for k, v in reversed(list(files.items()))})
            self.assertEqual(path.read_bytes(), first, 'same input, same bytes, whatever the order')

    def test_damaged_source_does_not_install_partial_cheats(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            archive, app = self.fixture(root), root / 'app'
            with self.assertRaisesRegex(ValueError, 'SHA256 mismatch'):
                data.install(archive, app)
            self.assertFalse((app / 'database').exists())
            with tarfile.open(archive, 'w:gz') as out:
                entry = tarfile.TarInfo(f'libretro-database-{data.COMMIT}/LICENSE')
                entry.size = 1
                out.addfile(entry, io.BytesIO(b'x'))
            with self.assertRaisesRegex(ValueError, 'missing a system or licence'):
                data.install(archive, app, data.sha(archive))
            self.assertFalse((app / 'database').exists())
            self.assertFalse((app / data.PACK).exists())
            self.assertFalse((app / 'licences/libretro-database').exists())


if __name__ == '__main__':
    unittest.main()
