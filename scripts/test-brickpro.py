#!/usr/bin/env python3
"""Host-only Brick Pro identity and inputd suspend regression tests; synthetic roots, no hardware."""
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parent.parent


class BrickPro(unittest.TestCase):
    def test_card_prefers_brickpro_identity_over_the_shared_screen(self):
        text = (ROOT / 'deploy/brick/start.sh').read_text()
        start = text.index('board() {')
        functions = text[start:text.index('\nBOARD=', start)]
        for shell in (['dash'], ['busybox', 'ash']):
            for model, dt, main, i2c, want in [
                ('', '', 'Trimui Brick Pro', False, 'brickpro'),
                ('', '', 'Trimui Brick', True, 'brickpro'),
                ('TG4040', '', '', False, 'brickpro'),
                ('', 'trimui,brick-pro\0allwinner,sun50iw10p1\0', '', False, 'brickpro'),
                ('TG3040', 'allwinner,sun50iw10p1', 'Trimui Brick', False, 'brick'),
            ]:
                with self.subTest(shell=shell, model=model, dt=dt, main=main, i2c=i2c), tempfile.TemporaryDirectory() as d:
                    root = Path(d)
                    (root / 'bin').mkdir()
                    for name, content in [('virtual_size', '1024,16384'), ('model', model), ('compatible', dt), ('bin/MainUI', main)]:
                        (root / name).write_text(content)
                    if i2c:
                        (root / 'i2c-3').touch()
                    env = dict(os.environ, FBSYS=d, FW=d, BRICK_MODEL_FILE=str(root/'model'),
                               BRICK_DT=d, BRICK_CPUINFO=str(root/'missing'), BRICK_I2C3=str(root/'i2c-3'))
                    out = subprocess.check_output(shell + ['-c', functions + '\nboard'], env=env, text=True)
                    self.assertEqual(out.strip(), want)

    def suspend_fixture(self, shell, outcome='success'):
        # Share the new dispatcher's harmless kernel capability/counter fixtures.
        import runpy
        fixture = runpy.run_path(str(ROOT/'scripts/test-brick-suspend.py'))['Suspend'].fixture
        root, env, run = fixture(self, shell, outcome)
        env['BRICK_INPUTD_SUSPEND'] = str(root/'system_suspend')
        return root, env, run

    def test_inputd_is_paused_at_kernel_entry_and_resumed_before_services(self):
        for shell in (['dash'], ['busybox', 'ash']):
            for failed in (False, True):
                with self.subTest(shell=shell, failed=failed):
                    root, env, run = self.suspend_fixture(shell)
                    marker=Path(env['BRICK_INPUTD_SUSPEND'])
                    stats=root/'sys/kernel/debug/suspend_stats'
                    Path(env['BRICK_SUSPEND_ENTER']).write_text(
                        '#!/bin/sh\n' + f'test -e "{marker}" || exit 8\necho entered >"{root}/entered"\n' +
                        ('exit 1\n' if failed else f'echo "success: 1" >"{stats}"\n'))
                    Path(env['BRICK_WPA_INIT']).write_text(
                        '#!/bin/sh\nif [ "$1" = start ]; then test ! -e "$BRICK_INPUTD_SUSPEND" || exit 8; echo resumed >"'+str(root)+'/resumed"; fi\n')
                    result=run()
                    self.assertEqual(result.returncode, 1 if failed else 0, result.stderr)
                    self.assertTrue((root/'entered').exists(), 'inputd must already be paused at kernel entry')
                    self.assertFalse(marker.exists(), 'input left paused after suspend returned/refused')
                    self.assertTrue((root/'resumed').exists(), 'input must resume before network services')

    def test_pro_poweroff_uses_its_own_zones_and_leaves_the_brick_raw_frame_alone(self):
        for shell in (['dash'], ['busybox', 'ash']):
            with self.subTest(shell=shell), tempfile.TemporaryDirectory() as d:
                root = Path(d)
                (root/'frame_hex').write_text('original-pro-frame')
                env = dict(os.environ, POCKETUI_DEVICE='brickpro', BRICK_LED_ANIM=d, BRICK_POWEROFF='/bin/true', BRICK_SYNC='/bin/true')
                subprocess.run(shell + [str(ROOT/'deploy/brick/hwroot/usr/bin/systemctl'), 'poweroff'], env=env, check=True, timeout=5)
                for zone in ['f1','f2','m','lr','rear']:
                    self.assertEqual((root/f'effect_{zone}').read_text().strip(), '0')
                self.assertFalse((root/'effect_l').exists())
                self.assertEqual((root/'frame_hex').read_text(), 'original-pro-frame')

    def test_marker_failure_refuses_kernel_entry(self):
        for shell in (['dash'], ['busybox', 'ash']):
            with self.subTest(shell=shell):
                root, env, run = self.suspend_fixture(shell)
                env['BRICK_INPUTD_SUSPEND'] = str(root/'missing/system_suspend')
                out=run()
                self.assertNotEqual(out.returncode, 0)
                self.assertNotIn('enter', (root/'calls').read_text() if (root/'calls').exists() else '',
                                 'must refuse sleep when inputd cannot be paused')

    def test_interrupted_suspend_releases_inputd(self):
        import signal
        for shell in (['dash'], ['busybox', 'ash']):
            with self.subTest(shell=shell):
                root, env, _ = self.suspend_fixture(shell)
                marker=Path(env['BRICK_INPUTD_SUSPEND'])
                Path(env['BRICK_UBUS']).write_text('#!/bin/sh\ntouch "'+str(root)+'/reached"\nexec sleep 20\n')
                proc = subprocess.Popen(shell + [str(ROOT/'deploy/brick/hwroot/usr/bin/systemctl'), 'suspend'], env=env,
                                        start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                try:
                    deadline = time.monotonic() + 5
                    while not (root/'reached').exists() and time.monotonic() < deadline:
                        time.sleep(0.01)
                    self.assertTrue((root/'reached').exists(), 'interruption must exercise the paused network helper')
                    self.assertTrue(marker.exists())
                    os.killpg(proc.pid, signal.SIGTERM)
                    self.assertNotEqual(proc.wait(timeout=5), 0)
                    self.assertFalse(marker.exists(), 'input must not remain paused after termination')
                    deadline=time.monotonic()+5
                    while (root/'tmp/brick-suspend.lock').exists() and time.monotonic()<deadline:
                        time.sleep(.01)
                finally:
                    if proc.poll() is None:
                        os.killpg(proc.pid, signal.SIGKILL)
                        proc.wait(timeout=5)


if __name__ == '__main__':
    unittest.main()
