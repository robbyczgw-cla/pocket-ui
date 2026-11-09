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

    def test_inputd_is_paused_at_kernel_entry_and_resumed_before_services(self):
        for shell in (['dash'], ['busybox', 'ash']):
            for failed in (False, True):
                with self.subTest(shell=shell, failed=failed), tempfile.TemporaryDirectory() as d:
                    root = Path(d)
                    marker = root/'system_suspend'
                    state = root/'power'
                    if failed:
                        # Opening a directory for output refuses suspend, without touching host /sys.
                        state.mkdir()
                    else:
                        os.mkfifo(state)
                    wpa = root/'wpa'
                    wpa.write_text('#!/bin/sh\nif [ "$1" = start ]; then test ! -e "$BRICK_INPUTD_SUSPEND" || exit 8; echo resumed >"$BRICK_STATE/resumed"; fi\n')
                    wpa.chmod(0o755)
                    env = dict(os.environ, BRICK_INPUTD_SUSPEND=str(marker), BRICK_POWER_STATE=str(state),
                               BRICK_STATE=d, BRICK_HW='/bin/true', BRICK_WPA_INIT=str(wpa),
                               BRICK_PIDOF='/bin/false', BRICK_UDHCPC='/bin/true', BRICK_RFKILL=str(root/'none'),
                               BRICK_SUSPEND_RETRY='0', BRICK_INPUTD_DRAIN='0')
                    proc = subprocess.Popen(shell + [str(ROOT/'deploy/brick/hwroot/usr/bin/systemctl'), 'suspend'], env=env,
                                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    try:
                        if not failed:
                            # FIFO open unblocks the kernel-write stand-in. The flag must already exist.
                            deadline = time.monotonic() + 5
                            while not marker.exists() and proc.poll() is None and time.monotonic() < deadline:
                                time.sleep(0.01)
                            self.assertTrue(marker.exists(), 'inputd was still polling at kernel entry')
                            fd = os.open(state, os.O_RDONLY | os.O_NONBLOCK)
                            import select
                            self.assertTrue(select.select([fd], [], [], 5)[0], 'suspend never reached kernel entry')
                            self.assertEqual(os.read(fd, 64), b'mem\n')
                            os.close(fd)
                        result = proc.wait(timeout=5)
                        self.assertEqual(result, 1 if failed else 0)
                        self.assertFalse(marker.exists(), 'input left paused after suspend returned/refused')
                        self.assertTrue((root/'resumed').exists(), 'input must resume before network services')
                    finally:
                        if proc.poll() is None:
                            proc.terminate()
                            proc.wait(timeout=5)

    def test_pro_poweroff_uses_its_own_zones_and_leaves_the_brick_raw_frame_alone(self):
        for shell in (['dash'], ['busybox', 'ash']):
            with self.subTest(shell=shell), tempfile.TemporaryDirectory() as d:
                root = Path(d)
                (root/'frame_hex').write_text('original-pro-frame')
                env = dict(os.environ, POCKETUI_DEVICE='brickpro', BRICK_LED_ANIM=d, BRICK_POWEROFF='/bin/true')
                subprocess.run(shell + [str(ROOT/'deploy/brick/hwroot/usr/bin/systemctl'), 'poweroff'], env=env, check=True, timeout=5)
                for zone in ['f1','f2','m','lr','rear']:
                    self.assertEqual((root/f'effect_{zone}').read_text().strip(), '0')
                self.assertFalse((root/'effect_l').exists())
                self.assertEqual((root/'frame_hex').read_text(), 'original-pro-frame')

    def test_marker_failure_refuses_kernel_entry(self):
        for shell in (['dash'], ['busybox', 'ash']):
            with self.subTest(shell=shell), tempfile.TemporaryDirectory() as d:
                root = Path(d)
                state = root/'power'
                env = dict(os.environ, BRICK_INPUTD_SUSPEND=str(root/'missing/system_suspend'), BRICK_POWER_STATE=str(state))
                out = subprocess.run(shell + [str(ROOT/'deploy/brick/hwroot/usr/bin/systemctl'), 'suspend'], env=env,
                                     capture_output=True, timeout=5)
                self.assertNotEqual(out.returncode, 0)
                self.assertFalse(state.exists(), 'must refuse sleep when inputd cannot be paused')

    def test_interrupted_suspend_releases_inputd(self):
        import signal
        for shell in (['dash'], ['busybox', 'ash']):
            with self.subTest(shell=shell), tempfile.TemporaryDirectory() as d:
                root = Path(d)
                marker = root/'system_suspend'
                wpa = root/'wpa'
                wpa.write_text('#!/bin/sh\ntouch "$BRICK_STATE/reached"\nexec sleep 20\n')
                wpa.chmod(0o755)
                env = dict(os.environ, BRICK_INPUTD_SUSPEND=str(marker), BRICK_INPUTD_DRAIN='0',
                           BRICK_HW='/bin/true', BRICK_RFKILL=str(root/'none'), BRICK_WPA_INIT=str(wpa), BRICK_STATE=d)
                proc = subprocess.Popen(shell + [str(ROOT/'deploy/brick/hwroot/usr/bin/systemctl'), 'suspend'], env=env,
                                        start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                try:
                    deadline = time.monotonic() + 5
                    while not (root/'reached').exists() and time.monotonic() < deadline:
                        time.sleep(0.01)
                    self.assertTrue(marker.exists())
                    os.killpg(proc.pid, signal.SIGTERM)  # this test created the entire process group
                    self.assertNotEqual(proc.wait(timeout=5), 0)
                    self.assertFalse(marker.exists(), 'input must not remain paused after termination')
                finally:
                    if proc.poll() is None:
                        os.killpg(proc.pid, signal.SIGKILL)
                        proc.wait(timeout=5)


if __name__ == '__main__':
    unittest.main()
