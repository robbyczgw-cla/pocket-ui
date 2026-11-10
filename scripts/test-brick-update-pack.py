#!/usr/bin/env python3
"""Player-facing packaging regressions, with a tiny fake card; no build machines or signing key."""
import importlib.util
import json
import subprocess
import sys
from pathlib import Path
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('pack', Path(__file__).with_name('brick-update-pack.py'))
pack = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pack)


class Packaging(unittest.TestCase):
    def test_regular_update_excludes_cores_and_packages_are_stable(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            app = root / 'app'
            for name, data in [('bin/pocket-ui', b'program'), ('cores/core.so', b'core'), ('bios-data/scummvm/theme', b'data')]:
                file = app / name
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_bytes(data)
            snapshots = []
            for version in ['1', '2']:
                release, channel = root / version / 'release', root / version / 'channel'
                pack.prepare(app, release, channel, 'https://github.com/owner/repo/releases/download/v' + version, version)
                with tarfile.open(channel / f'pocket-ui-brick-{version}.tar.gz') as archive:
                    self.assertEqual(set(archive.getnames()), {'bin', 'bin/pocket-ui', 'asset-packages.json'})
                    inventory = json.load(archive.extractfile('asset-packages.json'))
                snapshots.append({p.name: p.read_bytes() for p in (channel / 'assets').iterdir()})
                for asset in inventory['assets']:
                    package = asset['package']
                    self.assertTrue(package['url'].startswith('https://github.com/owner/repo/releases/download/v' + version + '/'))
                    self.assertEqual(package['mirrors'], ['https://pocketui.xyz/updates/brick/assets/' + package['url'].rsplit('/', 1)[1]])
                    file = release / asset['package']['url'].rsplit('/', 1)[1]
                    self.assertEqual(pack.sha(file), asset['package']['sha256'])
                    with tarfile.open(file) as archive:
                        self.assertEqual(archive.getnames(), [asset['path']])
            self.assertEqual(snapshots[0], snapshots[1], 'app release numbers never cause unchanged cores to be downloaded again')

    def test_legacy_card_pack_is_published_only_as_an_optional_download(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            app = root / 'app'
            (app / 'cores').mkdir(parents=True)
            (app / 'cores/core.so').write_bytes(b'core')
            (app / 'cores/cht.pack').write_bytes(b'legacy cheats')
            pack.prepare(app, root / 'release', root / 'channel', 'https://example.org/release', '1')
            with tarfile.open(root / 'channel/pocket-ui-brick-1.tar.gz') as archive:
                self.assertNotIn('cores/cht.pack', archive.getnames())
                inventory = json.load(archive.extractfile('asset-packages.json'))
                self.assertEqual([asset['path'] for asset in inventory['assets']], ['cores/core.so'])
                info = json.load(archive.extractfile('database/cheats-download.json'))
            self.assertEqual((root / 'channel/assets' / info['url'].rsplit('/', 1)[1]).read_bytes(), b'legacy cheats')
            self.assertEqual((app / 'cores/cht.pack').read_bytes(), b'legacy cheats', 'preparing OTA never changes the build input')
            listed = subprocess.check_output([sys.executable, str(Path(__file__).with_name('list-update-assets.py')),
                str(root / 'channel/pocket-ui-brick-1.tar.gz')], text=True)
            self.assertIn('optional-cheats\t', listed, 'release preflight checks that the optional download is actually hosted')
            self.assertIn(info['url'], listed)

    def test_oversized_file_and_symlinks_refuse_the_package(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'cores').mkdir()
            file = root / 'cores/core.so'
            file.write_bytes(bytes(range(256)) * 10)
            previous = pack.LIMIT
            try:
                pack.LIMIT = 10
                with self.assertRaises(ValueError):
                    pack.archive(root, [file], root / 'oversize.tar.gz')
            finally:
                pack.LIMIT = previous
            (root / 'cores/link.so').symlink_to(file)
            with self.assertRaises(ValueError):
                pack.prepare(root, root / 'release', root / 'channel', 'https://github.com/owner/repo/releases/download/v1', '1')


if __name__ == '__main__':
    unittest.main()
