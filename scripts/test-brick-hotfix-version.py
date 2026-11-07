#!/usr/bin/env python3
"""Exercise the real publishers' version gates without a signing key or a build."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class HotfixVersions(unittest.TestCase):
    def test_four_numbers_are_reserved_for_the_brick_channel(self):
        with tempfile.TemporaryDirectory() as tmp:
            for channel, version, accepted in [
                ('brick', '0.3.0.1', True), ('brick', '0.3.1', True),
                ('stable', '0.3.0.1', False), ('beta', '0.3.0.1', False),
                ('stable', '0.3.1', True), ('brick', '0.3.0.1.2', False),
                ('brick', '0.3..1', False), ('brick', '0.3.0-rc1', False),
                ('brick', '0.3.0.', False), ('brick', '.3.0.1', False),
            ]:
                with self.subTest(channel=channel, version=version):
                    p = subprocess.run(['sh', str(ROOT / 'scripts/release-channel.sh'),
                        '--repo', 'example/project', '--in', tmp, '--out', tmp + '/out',
                        '--key', tmp + '/missing-key', '--channel', channel, '--version', version],
                        capture_output=True, text=True, timeout=5)
                    self.assertNotEqual(p.returncode, 0)
                    self.assertIn('no signing key' if accepted else 'bad or no version', p.stderr)

    def test_the_brick_publisher_accepts_the_hotfix_but_rejects_test_names(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            app = d / 'card'
            files = ['start.sh', 'RELEASE', 'bin/pocket-ui', 'bin/pocket-core', 'bin/pocket-fb',
                'bin/brick-hw', 'msg/starting.png', 'bin/tailscale', 'bin/tailscaled', 'bin/curl',
                'bin/dropbearmulti', 'bin/syncthing', 'hwroot/usr/bin/brick-ssh',
                'hwroot/usr/bin/brick-tailscale', 'hwroot/usr/bin/brick-syncthing']
            for name in files:
                p = app / name
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text('#!/bin/sh\nexit 0\n')
            (d / 'key').touch()
            # Reaching store-tool after packing proves that the actual public-version gate accepted it.
            tool = d / 'store-tool'
            tool.write_text('#!/bin/sh\necho VERSION_GATE_ACCEPTED >&2\nexit 1\n')
            tool.chmod(0o755)
            env = dict(os.environ, POCKETUI_SIGNING_KEY=str(d / 'key'), STORE_TOOL=str(tool))
            for version, accepted in [('0.3.0.1', True), ('0.3.1', True),
                    ('0.3.0-rc1', False), ('0.3.0.1.2', False), ('0.3..1', False)]:
                with self.subTest(version=version):
                    (app / 'RELEASE').write_text(version)
                    elf = b'\x7fELF\x02\x01' + bytes(12) + b'\xb7\x00' + version.encode() + b'.asset-moves.restore-assets'
                    for name in ['pocket-ui', 'pocket-core', 'pocket-fb']:
                        (app / 'bin' / name).write_bytes(elf)
                    p = subprocess.run(['bash', str(ROOT / 'scripts/brick-publish.sh'), str(app),
                        '--public', 'example/project', '--out', str(d / 'out')],
                        env=env, capture_output=True, text=True, timeout=10)
                    self.assertNotEqual(p.returncode, 0)
                    self.assertIn('VERSION_GATE_ACCEPTED' if accepted else 'plain X.Y.Z', p.stderr)


if __name__ == '__main__':
    unittest.main()
