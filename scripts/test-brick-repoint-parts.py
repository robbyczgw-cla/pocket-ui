#!/usr/bin/env python3
"""Only changed Brick components move to a new release; both hosts keep their bytes."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.environ.get('STORE_TOOL'), 'needs the built store-tool')
class RepointParts(unittest.TestCase):
    def test_unchanged_package_keeps_old_tag_hash_and_website_fallback(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            tool = os.environ['STORE_TOOL']
            key = root / 'test-key'
            subprocess.run([tool, 'keygen', str(key)], check=True, stdout=subprocess.DEVNULL)
            env = dict(os.environ, POCKETUI_SIGNING_KEY=str(key), BRICK_PUBLISH_VERIFY_KEY=str(key), POCKETUI_PUBLIC_REPO='owner/repo')
            app = root / 'app'
            for name, data in [('bin/pocket-ui', b'program'), ('cores/stable.so', b'stable'), ('cores/changed.so', b'old')]:
                path = app / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
            live_names = root / 'live-names'
            live_names.write_text('placeholder\n')
            prior = 'brick-parts-older'
            previous = None
            for version in ['9.9.8', '9.9.9']:
                if previous:
                    (app / 'cores/changed.so').write_bytes(b'new')
                channel = root / version
                base = f'https://github.com/owner/repo/releases/download/v{version}'
                release = channel / 'release'
                website = channel / 'updates/brick'
                subprocess.run(['python3', str(ROOT / 'scripts/brick-update-pack.py'), str(app), str(release), str(website), base, version], check=True, stdout=subprocess.DEVNULL)
                archive = release / f'pocket-ui-brick-{version}.tar.gz'
                manifest = website / 'update.json'
                subprocess.run([tool, 'update', str(key), str(manifest), 'app', version, str(archive), base + '/' + archive.name], check=True, stdout=subprocess.DEVNULL)
                subprocess.run([tool, 'mirror', str(key), str(manifest), 'app', 'https://pocketui.xyz/updates/brick/' + archive.name], check=True, stdout=subprocess.DEVNULL)
                subprocess.run(['python3', str(ROOT / 'scripts/brick-repoint-parts.py'), str(channel), version, prior, '--live-names', str(live_names)], check=True, env=env, stdout=subprocess.DEVNULL)
                with tarfile.open(archive) as tar:
                    inventory = {a['path']: a['package'] for a in json.load(tar.extractfile('asset-packages.json'))['assets']}
                    self.assertEqual(tar.extractfile('bin/pocket-ui').read(), b'program')
                for package in inventory.values():
                    name = package['url'].rsplit('/', 1)[1]
                    self.assertEqual(hashlib.sha256((website / 'assets' / name).read_bytes()).hexdigest(), package['sha256'])
                    self.assertEqual(package['mirrors'], ['https://pocketui.xyz/updates/brick/assets/' + name])
                subprocess.run(['python3', str(ROOT / 'scripts/check-channel-sources.py'), 'brick', str(manifest), 'owner/repo'], check=True)
                if previous:
                    self.assertEqual(inventory['cores/stable.so']['url'], previous['cores/stable.so']['url'])
                    self.assertEqual(inventory['cores/stable.so']['sha256'], previous['cores/stable.so']['sha256'])
                    self.assertIn('/brick-parts-9.9.9/', inventory['cores/changed.so']['url'])
                    report = json.loads((channel / 'parts-report.json').read_text())
                    self.assertEqual(report['stay_in'], {'brick-parts-9.9.8': 1})
                    self.assertEqual(report['new_in'], {'brick-parts-9.9.9': 1})
                    self.assertEqual(len(list((channel / 'parts-new').iterdir())), 1)
                previous = inventory
                prior = 'brick-parts-' + version
                live_names.write_text('\n'.join(p['url'].rsplit('/', 1)[1] for p in inventory.values()) + '\n')


if __name__ == '__main__':
    unittest.main()
