#!/usr/bin/env python3
"""Run device code with the firmware's missing applets, without SSH."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest

HERE = Path(__file__).resolve().parent
BUSYBOX = shutil.which('busybox')


def firmware_path(root, shell):
    bindir = root / 'bin'
    bindir.mkdir(exist_ok=True)
    # The host BusyBox is standalone: PATH alone cannot hide its built-in applets.
    # Rename just these equal-length entries (keeping the sorted applet table)
    # in a fixture copy, so ash really has the firmware's missing commands.
    firmware = root / 'busybox'
    blob = Path(BUSYBOX).read_bytes()
    for old, new in ((b'timeout', b'timeouu'), (b'stat', b'stau'), (b'nohup', b'nohuq')):
        blob = blob.replace(b'\0'+old+b'\0', b'\0'+new+b'\0')
    firmware.write_bytes(blob); firmware.chmod(0o755)
    for name in subprocess.check_output([BUSYBOX, '--list'], text=True).split():
        if name not in ('timeout', 'stat', 'nohup', 'sh'):
            (bindir / name).symlink_to(firmware)
    if shell[0] == BUSYBOX:
        shell = [str(firmware), *shell[1:]]
    (bindir / 'sh').write_text('#!/bin/sh\nexec ' + ' '.join(shell) + ' "$@"\n')
    (bindir / 'sh').chmod(0o755)
    return str(bindir)


class Firmware(unittest.TestCase):
    def test_device_check_ui_and_core_writes_without_device_timeout(self):
        for shell in ([shutil.which('dash')], [BUSYBOX, 'ash']):
            with self.subTest(shell=shell), tempfile.TemporaryDirectory() as temp:
                root=Path(temp); work=root/'work'; work.mkdir()
                env=dict(os.environ, PATH=firmware_path(root,shell), DC_LOCAL='1', DC_SOURCE_ONLY='1',
                         DC_WORK=str(work), DC_FIFO=str(root/'fifo'), DC_CORECMD=str(root/'core'))
                shutil.copy(HERE/'ui-status.sh',work/'ui-status.sh')
                readers=[]
                try:
                    for name in ('fifo','core'):
                        os.mkfifo(root/name)
                        with (root/(name+'.txt')).open('wb') as output:
                            readers.append(subprocess.Popen([BUSYBOX,'cat',str(root/name)],stdout=output))
                    result=subprocess.run(['/bin/bash','-c',
                        '. "$1/device-check.sh" --device brick --out "$2/out"; dev() { sh -c "pidof() { return 0; }; $1"; }; fifo down && qcmd pause',
                        'fixture',str(HERE),str(root)],env=env,capture_output=True,timeout=30)
                    self.assertEqual(result.returncode,0,result.stderr)
                    for reader in readers:reader.wait(timeout=2)
                    self.assertEqual((root/'fifo.txt').read_bytes(),b'down\n')
                    self.assertEqual((root/'core.txt').read_bytes(),b'pause\n')
                finally:
                    for reader in readers:
                        if reader.poll() is None:reader.terminate();reader.wait(timeout=2)

    def test_fifo_writes_work_and_unread_fifo_is_bounded(self):
        for shell in ([shutil.which('dash')], [BUSYBOX, 'ash']):
            with self.subTest(shell=shell), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                env = dict(os.environ, PATH=firmware_path(root, shell),
                           POCKETUI_STATUS=str(root/'status'), POCKETUI_FIFO=str(root/'fifo'))
                (root/'status').write_text(f'pid={os.getpid()}\tplace=settings:Sound\tsleep=awake\tcover=none\n')
                os.mkfifo(root/'fifo')
                device_shell = [str(root/'bin/sh')]
                missing = subprocess.run([*device_shell, '-c', 'command -v timeout || command -v stat || command -v nohup'], env=env, capture_output=True)
                self.assertNotEqual(missing.returncode, 0, missing.stdout)
                command = [*device_shell, str(HERE/'ui-status.sh'), 'send', '--at', 'settings:*', 'down']
                with (root/'received').open('wb') as output:
                    reader = subprocess.Popen([BUSYBOX, 'cat', str(root/'fifo')], stdout=output)
                    try:
                        result = subprocess.run(command, env=env, capture_output=True, timeout=5)
                        self.assertEqual(result.returncode, 0, result.stderr)
                        reader.wait(timeout=2)
                    finally:
                        if reader.poll() is None:
                            reader.terminate(); reader.wait(timeout=2)
                self.assertEqual((root/'received').read_bytes(), b'down\n')
                started = time.monotonic()
                blocked = subprocess.run(command, env=env, capture_output=True, timeout=5)
                self.assertEqual(blocked.returncode, 1, blocked.stderr)
                self.assertGreater(time.monotonic()-started, 1.8)
                self.assertNotIn(b'not found', blocked.stderr)

    def test_timeout_status_and_arguments_match_native_timeout(self):
        source = (HERE/'ui-status.sh').read_text().split('cmd=${1:-}')[0]
        for shell in ([shutil.which('dash')], [BUSYBOX, 'ash']):
            with self.subTest(shell=shell), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                env = dict(os.environ, PATH=firmware_path(root, shell))
                device_shell = [str(root/'bin/sh')]
                for script, expected in [('exit 7', 7), ('exit 0', 0),
                                         ('kill -TERM $$', 143), ('exec sleep 3', 124)]:
                    result = subprocess.run([*device_shell, '-c', source+'\nst_timeout 0.1 sh -c "$1"', 'test', script],
                                            env=env, capture_output=True, timeout=3)
                    self.assertEqual(result.returncode, expected, result.stderr)
                result = subprocess.run([*device_shell, '-c', source+'\nst_timeout 1 missing-command'],
                                        env=env, capture_output=True, timeout=3)
                self.assertEqual(result.returncode, 127)
                # If timeout exists, preserve its behaviour rather than using the fallback.
                native = root/'bin/timeout'
                native.write_text('#!/bin/sh\nexit 42\n'); native.chmod(0o755)
                result = subprocess.run([*device_shell, '-c', source+'\nst_timeout 1 true'], env=env, timeout=3)
                self.assertEqual(result.returncode, 42)


if __name__ == '__main__':
    unittest.main()
