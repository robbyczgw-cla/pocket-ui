#!/usr/bin/env python3
"""Prepare source sidecars in throwaway release/channel folders; no real key or upload."""
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parent.parent


class PublishSources(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='pocket-publish-sources-')
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        self.app = self.work / 'build/card/pocket-ui/app'
        self.version = '0.2.1'
        self.helpers = self.work / 'helpers'
        self.helpers.mkdir()
        self.key = self.work / 'fixture-key'
        self.key.write_text('fixture-only-key')
        self.tool = self.helpers / 'store-tool'
        self.executable(self.tool, '''#!/usr/bin/env python3
import json, pathlib, sys
command, *args = sys.argv[1:]
if command == 'update':
    key, target, kind, version, asset, url, notes = args
    target = pathlib.Path(target)
    data = json.loads(target.read_text()) if target.exists() else {}
    data[kind] = {'version':version,'url':url,'asset':pathlib.Path(asset).name}
    target.write_text(json.dumps(data)); pathlib.Path(str(target)+'.sig').write_text('fixture signature')
elif command == 'list':
    for kind, item in json.loads(pathlib.Path(args[0]).read_text()).items(): print(kind+'\\t'+item['version'])
elif command not in ('verify', 'mirror'): raise SystemExit('unexpected fixture command')
''')
        self.executable(self.helpers / 'curl', '''#!/usr/bin/env python3
import pathlib, shutil, sys
args = sys.argv[1:]
urls = [a for a in args if a.startswith('file://')]
if len(urls) != 1: raise SystemExit('fixture forbids network access')
shutil.copyfile(pathlib.Path(urls[0][7:]), args[args.index('-o')+1])
''')
        for name in ('gh', 'ssh', 'scp', 'tailscale'):
            self.executable(self.helpers / name, '#!/bin/sh\necho forbidden >"$FIXTURE_EXTERNAL_CALL"\nexit 88\n')
        self.env = dict(os.environ, STORE_TOOL=str(self.tool), POCKETUI_SIGNING_KEY=str(self.key),
                        PATH=str(self.helpers)+os.pathsep+os.environ['PATH'],
                        BRICK_CHANNEL_BASE='file://'+str(self.work / 'web'),
                        FIXTURE_EXTERNAL_CALL=str(self.work / 'external-call'))
        self.build(self.version)

    def executable(self, path, text):
        path.write_text(text)
        path.chmod(0o755)

    def build(self, version):
        self.version = version
        files = ('start.sh', 'RELEASE', 'bin/pocket-ui', 'bin/pocket-core', 'bin/pocket-fb', 'bin/brick-hw',
                 'msg/starting.png', 'bin/tailscale', 'bin/tailscaled', 'bin/curl', 'bin/dropbearmulti',
                 'bin/syncthing', 'hwroot/usr/bin/brick-ssh', 'hwroot/usr/bin/brick-tailscale',
                 'hwroot/usr/bin/brick-syncthing', 'bin/bash', 'cores/test_libretro.so', 'licences/SOURCES.txt')
        for name in files:
            path = self.app / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('fixture')
        (self.app / 'RELEASE').write_text(version)
        (self.app / 'start.sh').write_text('#!/bin/sh\nexit 0\n')
        for name in ('pocket-ui', 'pocket-core', 'pocket-fb'):
            (self.app / 'bin' / name).write_bytes(b'\x7fELF\x02\x01\x01'+b'\0'*11+b'\xb7\0'+b'\0'*40+version.encode()+b' .asset-moves .restore-assets')
        (self.app / 'licences/SOURCES.txt').write_text('Complete source: '+self.source_name+' beside the card zip.\n')

    @property
    def source_name(self):
        return 'pocket-ui-trimui-sources-'+self.version+'.zip'

    def sources(self, parent=None, name=None):
        path = (parent or self.work / 'build') / (name or self.source_name)
        path.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(path, 'w') as archive:
            archive.writestr('sources/portmaster/bash-5.2.37.tar.gz', b'fixture source only')
        return path

    def run_publish(self, *args, public=True):
        command = ['bash', str(ROOT / 'scripts/brick-publish.sh'), str(self.app)]
        if public: command += ['--public', 'owner/pocket-ui', '--out', str(self.work / 'public')]
        else: command += ['--web', str(self.work / 'web'), '--channel', 'test']
        result = subprocess.run(command + list(args), env=self.env, capture_output=True, text=True, timeout=20)
        self.assertFalse((self.work / 'external-call').exists(), 'preparation must never upload')
        return result

    def test_public_preparation_discovers_maker_sources_and_keeps_them_out_of_ota(self):
        sources = self.sources()
        result = self.run_publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        published = self.work / 'public/release' / self.source_name
        self.assertTrue(published.is_file(), 'the release needs the matching source sidecar')
        self.assertEqual(published.read_bytes(), sources.read_bytes())
        listing = json.loads((self.work / 'public/updates/brick/update.json').read_text())
        self.assertNotIn('sources', listing, 'source archives are not an installable update')
        with tarfile.open(self.work / 'public/release/pocket-ui-brick-0.2.1.tar.gz') as archive:
            self.assertFalse(any(self.source_name in n or n.startswith('sources/portmaster/') for n in archive.getnames()))
            self.assertIn(self.source_name.encode(), archive.extractfile('licences/SOURCES.txt').read())

    def test_beta_channel_serves_the_sidecar_before_the_bash_update_is_visible(self):
        self.build('0.2.1-brick.1')
        sources = self.sources()
        result = self.run_publish('--full-check', public=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        published = self.work / 'web/brick-updates/test' / self.source_name
        self.assertTrue(published.is_file(), 'a beta with Bash also needs accessible source')
        self.assertEqual(published.read_bytes(), sources.read_bytes())
        with tarfile.open(self.work / 'web/brick-updates/test/pocket-ui-brick-0.2.1-brick.1.tar.gz') as archive:
            self.assertFalse(any(self.source_name in n for n in archive.getnames()))

    def test_missing_sources_refuse_public_preparation_without_destroying_existing_files(self):
        marker = self.work / 'public/release/previous'
        marker.parent.mkdir(parents=True)
        marker.write_text('keep')
        result = self.run_publish()
        self.assertNotEqual(result.returncode, 0, 'Bash must not be prepared without its source archive')
        self.assertEqual(marker.read_text(), 'keep')
        self.assertIn(self.source_name, result.stderr)

    def test_explicit_sources_must_match_the_card_version(self):
        wrong = self.sources(self.work / 'elsewhere', 'pocket-ui-trimui-sources-0.2.0.zip')
        result = self.run_publish('--sources', str(wrong))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(self.source_name, result.stderr)
        right = self.sources(self.work / 'elsewhere')
        result = self.run_publish('--sources', str(right))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.work / 'public/release' / self.source_name).read_bytes(), right.read_bytes())


if __name__ == '__main__':
    unittest.main()
