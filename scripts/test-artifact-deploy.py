#!/usr/bin/env python3
"""Hermetic deployment checks: never connects to a device or builds Rust."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

DEV = Path(__file__).with_name('dev.sh')
HOST = DEV.with_name('host.sh')


class ArtifactDeployTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='artifact-deploy-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / 'worktree'
        (self.repo / 'scripts').mkdir(parents=True)
        shutil.copy2(DEV, self.repo / 'scripts/dev.sh')
        shutil.copy2(HOST, self.repo / 'scripts/host.sh')
        self.sha = '1234567890abcdef' * 2 + '12345678'
        self.artifacts = self.root / 'artifacts'
        self.package = self.artifacts / self.sha
        self.package.mkdir(parents=True)
        self.files = {
            'bin/pocket-ui': b'ui binary\n',
            'bin/pocket-core': b'core binary\n',
            'bin/pocket-splash': b'splash binary\n',
            'deploy/portmaster-setup.sh': b'#!/bin/sh\n',
            'deploy/run-pocket-ui.sh': b'#!/bin/sh\n',
            'deploy/pocket-splash.service': b'[Unit]\n',
            'scripts/host.sh': HOST.read_bytes(),
            'scripts/verify-device.sh': b'''#!/bin/sh
python3 - "$VERIFY_BUILD_DIR" "$@" <<'PY'
import json, os, pathlib, sys
b = pathlib.Path(sys.argv[1])
assert (b/'pocket-ui').read_bytes() == b'ui binary\\n'
assert (b/'pocket-core').read_bytes() == b'core binary\\n'
assert (b/'pocket-splash').read_bytes() == b'splash binary\\n'
with open(os.environ['MOCK_LOG'], 'a') as f: f.write(json.dumps(['verify', *sys.argv[1:]])+'\\n')
print('VERIFY: ok')
PY
''',
            'manifest.json': json.dumps({'format': 1, 'commit': self.sha,
                'target': 'aarch64-unknown-linux-gnu.2.38', 'profile': 'release',
                'bins': ['pocket-ui', 'pocket-core', 'pocket-splash']}).encode(),
        }
        for name, data in self.files.items():
            file = self.package / name
            file.parent.mkdir(exist_ok=True, parents=True)
            file.write_bytes(data)
            file.chmod(0o755)
        self.sums()
        (self.artifacts / 'latest-main').symlink_to(self.sha)
        mockbin = self.root / 'mockbin'
        mockbin.mkdir()
        program = '''#!/usr/bin/env python3
import json, os, pathlib, sys
with open(os.environ['MOCK_LOG'], 'a') as f: f.write(json.dumps([pathlib.Path(sys.argv[0]).name, *sys.argv[1:]])+'\\n')
if pathlib.Path(sys.argv[0]).name == 'cargo': sys.exit(97)
if pathlib.Path(sys.argv[0]).name == 'scp':
    for arg in sys.argv[1:-1]:
        if not arg.startswith('-'): assert pathlib.Path(arg).is_file(), arg
'''
        for name in ('ssh', 'scp', 'cargo'):
            file = mockbin / name
            file.write_text(program)
            file.chmod(0o755)
        self.log = self.root / 'calls.jsonl'
        self.env = dict(os.environ, PATH=str(mockbin)+os.pathsep+os.environ['PATH'],
                        MOCK_LOG=str(self.log), POCKETUI_HOST='device.invalid',
                        POCKETUI_ARTIFACT_DIR=str(self.artifacts))

    def sums(self):
        (self.package / 'SHA256SUMS').write_text(''.join(
            hashlib.sha256((self.package/name).read_bytes()).hexdigest()+'  '+name+'\n'
            for name in self.files))

    def deploy(self, *args):
        return subprocess.run(['sh', str(self.repo/'scripts/dev.sh'), 'deploy', *args],
                              env=self.env, text=True, capture_output=True)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def test_latest_installs_three_binaries_and_matching_verifier_without_cargo(self):
        p = self.deploy('--artifact', 'latest-main')
        self.assertEqual(p.returncode, 0, p.stdout+p.stderr)
        calls = self.calls()
        self.assertFalse(any(call[0] == 'cargo' for call in calls))
        self.assertEqual(calls[-1][0], 'verify')
        self.assertEqual(calls[-1][-1], 'device.invalid')
        upload = next(call for call in calls if call[0] == 'scp')
        self.assertEqual([Path(arg).name for arg in upload[2:-1]],
                         ['pocket-ui', 'pocket-core', 'pocket-splash', 'portmaster-setup.sh'])
        self.assertIn('releases/dev-good', calls[0][-1])
        self.assertIn('systemctl stop', calls[0][-1])

    def test_short_and_full_sha_are_supported(self):
        for key in (self.sha[:12], self.sha):
            p = self.deploy('--artifact', key)
            self.assertEqual(p.returncode, 0, p.stdout+p.stderr)

    def test_corrupt_binary_is_rejected_before_device_access(self):
        (self.package/'bin/pocket-core').write_bytes(b'wrong')
        p = self.deploy('--artifact', 'latest-main')
        self.assertNotEqual(p.returncode, 0)
        self.assertIn('checksum mismatch', p.stderr)
        self.assertEqual(self.calls(), [])

    def test_missing_checksum_is_rejected_before_device_access(self):
        sums = self.package/'SHA256SUMS'
        sums.write_text('\n'.join(sums.read_text().splitlines()[1:])+'\n')
        p = self.deploy('--artifact', self.sha)
        self.assertNotEqual(p.returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_non_main_paths_and_unknown_options_are_refused(self):
        for key in ('../../elsewhere', 'main', 'deadbee'):
            self.assertNotEqual(self.deploy('--artifact', key).returncode, 0)
        self.assertNotEqual(self.deploy('--artifact').returncode, 0)
        self.assertNotEqual(self.deploy('--unknown').returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_symlinked_binary_is_rejected_even_with_matching_content(self):
        binary = self.package/'bin/pocket-ui'
        other = self.root/'other'
        other.write_bytes(binary.read_bytes())
        binary.unlink()
        binary.symlink_to(other)
        self.assertNotEqual(self.deploy('--artifact', self.sha).returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_latest_must_resolve_inside_artifact_directory(self):
        (self.artifacts/'latest-main').unlink()
        external = self.root/self.sha
        shutil.copytree(self.package, external)
        (self.artifacts/'latest-main').symlink_to(external)
        self.assertNotEqual(self.deploy('--artifact', 'latest-main').returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_plain_deploy_still_runs_own_build(self):
        (self.repo/'scripts/release-rustflags.sh').write_text(':\n')
        p = self.deploy()
        self.assertEqual(p.returncode, 97, p.stdout+p.stderr)
        self.assertEqual([call[0] for call in self.calls()], ['cargo'])
        self.assertIn('zigbuild', self.calls()[0])
        self.assertIn('--bin', self.calls()[0])


if __name__ == '__main__':
    unittest.main(verbosity=2)
