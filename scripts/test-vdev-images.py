#!/usr/bin/env python3
"""Run the real vdev commands against a private Docker stand-in; no engine or builds."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
MOCK = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
statefile = pathlib.Path(os.environ['VDEV_TEST_STATE'])
# Import consumes the extraction stream before changing state (as Docker does).
if args[0] == 'import': sys.stdin.buffer.read()
state = json.loads(statefile.read_text())
with open(os.environ['VDEV_TEST_LOG'], 'a') as log: log.write(json.dumps(args)+'\n')
images = state['images']
def save(): statefile.write_text(json.dumps(state))
def image(ref):
    if ref not in images: sys.exit(1)
    return images[ref]
if args == ['info']: pass
elif args[0] == 'images':
    repo = args[-1]
    print('\n'.join(ref for ref in images if ref.rsplit(':', 1)[0] == repo))
elif args[:2] == ['image', 'inspect']:
    item = image(args[-1])
    print(item['id'] if '-f' in args or '--format' in args else json.dumps([item]))
elif args[0] == 'inspect': print('false')
elif args[0] == 'build':
    tag = args[args.index('-t')+1]
    if ':' not in tag: tag += ':latest'
    if '--build-arg' in args:
        base = args[args.index('--build-arg')+1].removeprefix('BASE=')
        images[tag] = dict(id='overlay-'+str(len(images)), base=base, base_id=image(base)['id'])
    else: images[tag] = dict(id='extract')
    save()
elif args[0] == 'import':
    images[args[-1]] = dict(id='new-import-'+str(len(images)))
    save()
elif args[0] == 'run':
    if '--rm' in args: print('rootfs stream')
    else:
        state['runs'].append(image(args[-1]))
        save()
elif args[0] == 'exec': pass
elif args[0] == 'rm': pass
else: sys.exit('unexpected Docker command: '+repr(args))
'''


class VirtualDeviceImageTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory(prefix='vdev-images-')
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.repo = self.root / 'tree'
        (self.repo / 'scripts').mkdir(parents=True)
        shutil.copy2(ROOT / 'scripts/vdev.sh', self.repo / 'scripts/vdev.sh')
        shutil.copytree(ROOT / 'docker', self.repo / 'docker')
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        docker = self.bin / 'docker'
        docker.write_text(MOCK)
        docker.chmod(0o755)
        self.statefile = self.root / 'state.json'
        self.log = self.root / 'calls.jsonl'
        self.statefile.write_text(json.dumps(dict(images={}, runs=[])))
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith('VDEV_') and key != 'ROCKNIX_IMG'}
        self.env.update(PATH=str(self.bin)+os.pathsep+os.environ['PATH'],
                        VDEV_TEST_STATE=str(self.statefile), VDEV_TEST_LOG=str(self.log),
                        VDEV_NAME='image-regression', VDEV_CACHE=str(self.root / 'cache'))

    def state(self):
        return json.loads(self.statefile.read_text())

    def add_base(self, ref, identity):
        state = self.state()
        state['images'][ref] = dict(id=identity)
        self.statefile.write_text(json.dumps(state))

    def run_vdev(self, *args, ok=True):
        result = subprocess.run(['sh', str(self.repo / 'scripts/vdev.sh'), *args],
                                env=self.env, text=True, capture_output=True, timeout=10)
        if ok:
            self.assertEqual(result.returncode, 0, result.stdout+result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout+result.stderr)
        return result

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def source_image(self):
        img = self.root / 'ROCKNIX-SM8250.aarch64-20261101.img'
        img.write_bytes(b'private fixture image')
        self.env['ROCKNIX_IMG'] = str(img)

    def test_start_after_new_base_uses_november_userland(self):
        self.add_base('pocket-vdev-base:20261001', 'october')
        self.run_vdev('up', '--device', 'mini1')
        self.add_base('pocket-vdev-base:20261101', 'november')
        self.run_vdev('up', '--device', 'rp5')
        self.assertEqual([run['base_id'] for run in self.state()['runs']], ['october', 'november'])

    def test_reimported_base_does_not_reuse_old_userland(self):
        ref = 'pocket-vdev-base:20261101'
        self.add_base(ref, 'old-import')
        self.run_vdev('up')
        self.add_base(ref, 'new-import')
        self.run_vdev('up')
        self.assertEqual([run['base_id'] for run in self.state()['runs']], ['old-import', 'new-import'])

    def test_exact_private_base_wins_over_newest_shared_version(self):
        self.add_base('pocket-vdev-base:20261201', 'shared-future')
        self.add_base('pocket-vdev-base-mine:20261101', 'private-november')
        self.env['VDEV_BASE_IMAGE'] = 'pocket-vdev-base-mine:20261101'
        self.run_vdev('up', '--device', 'flip2')
        self.assertEqual(self.state()['runs'][-1]['base_id'], 'private-november')

    def test_missing_private_base_is_imported_without_changing_shared_images(self):
        self.add_base('pocket-vdev-base:20261001', 'shared-october')
        self.add_base('pocket-vdev-extract:latest', 'shared-extractor')
        before = self.state()['images'].copy()
        self.source_image()
        self.env['VDEV_BASE_IMAGE'] = 'pocket-vdev-base-mine:20261101'
        self.run_vdev('up')
        images = self.state()['images']
        for ref, data in before.items():
            self.assertEqual(images[ref], data)
        self.assertEqual(self.state()['runs'][-1]['base'], self.env['VDEV_BASE_IMAGE'])
        self.assertTrue(any(call[0] == 'import' and call[-1] == self.env['VDEV_BASE_IMAGE']
                            for call in self.calls()))

    def test_rebuild_refuses_to_overwrite_explicit_base(self):
        self.source_image()
        ref = 'pocket-vdev-base-mine:20261101'
        self.env['VDEV_BASE_IMAGE'] = ref
        self.add_base(ref, 'keep-this-userland')
        result = self.run_vdev('image', '--rebuild-base', ok=False)
        self.assertIn('already exists', result.stderr)
        self.assertEqual(self.state()['images'][ref]['id'], 'keep-this-userland')
        self.assertFalse(any(call[0] in ('build', 'import', 'run') for call in self.calls()))

    def test_default_rebuild_still_imports_version_from_rocknix_filename(self):
        self.source_image()
        self.run_vdev('image', '--rebuild-base')
        self.assertIn('pocket-vdev-base:20261101', self.state()['images'])

    def test_unchanged_base_reuses_overlay_but_changed_boot_files_rebuild(self):
        self.add_base('pocket-vdev-base:20261001', 'october')
        self.run_vdev('up')
        self.run_vdev('up')
        builds = lambda: [call for call in self.calls() if call[0] == 'build']
        self.assertEqual(len(builds()), 1)
        with (self.repo / 'docker/vdev/entrypoint.sh').open('a') as script:
            script.write('\n# changed boot configuration\n')
        self.run_vdev('up')
        self.assertEqual(len(builds()), 2)

    def test_private_selection_requires_an_explicit_local_namespaced_tag(self):
        self.add_base('pocket-vdev-base:20261001', 'october')
        for ref in ('foreign:latest', 'registry.invalid/pocket-vdev-base-mine:20261101',
                    'pocket-vdev-base-mine', 'pocket-vdev-base:20261101',
                    'pocket-vdev-base-mine:bad tag'):
            with self.subTest(ref=ref):
                self.env['VDEV_BASE_IMAGE'] = ref
                self.run_vdev('up', ok=False)
        self.assertFalse(any(call[0] in ('build', 'import', 'run') for call in self.calls()))


if __name__ == '__main__':
    unittest.main(verbosity=2)
