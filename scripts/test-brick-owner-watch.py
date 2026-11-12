#!/usr/bin/env python3
"""The real device-side dead-man hands control back on input, without SSH."""
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
EVENT = struct.Struct('<qqHHi')


class Ownership(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='brick-journey-watch-fixture-')
        cls.root = Path(cls.temp.name)
        cls.input = cls.root / 'input'
        cls.input.mkdir()
        cls.writers = []
        for n in (0, 1, 3):
            node = cls.input / f'event{n}'
            os.mkfifo(node)
            cls.writers.append(os.open(node, os.O_RDWR | os.O_NONBLOCK))
        cls.binary = cls.root / 'watch'
        subprocess.run(['cc', '-O2', '-Wall', '-Wextra',
                        f'-DBRICK_SESSION_INPUT_ROOT="{cls.input}"',
                        str(ROOT / 'scripts/brick/owner-watch.c'), '-o', str(cls.binary)],
                       check=True, timeout=20)

    @classmethod
    def tearDownClass(cls):
        for fd in cls.writers:
            os.close(fd)
        cls.temp.cleanup()

    def run_watch(self, lifetime=5):
        work = Path(tempfile.mkdtemp(prefix='brick-journey-watch-case-'))
        self.addCleanup(lambda: __import__('shutil').rmtree(work))
        (work / 'restore.sh').write_text('#!/bin/sh\necho restored >"$1/restored"\n')
        proc = subprocess.Popen([str(self.binary), str(work), str(lifetime)])
        self.addCleanup(lambda: proc.poll() is None and proc.kill())
        time.sleep(.05)
        self.assertIsNone(proc.poll(), 'watcher must be reading private fake input nodes')
        return work, proc

    def test_real_press_or_dpad_restores_even_without_a_host_connection(self):
        for kind, code, value in [(1, 305, 1), (1, 116, 1), (3, 16, -1)]:
            with self.subTest(kind=kind, code=code):
                work, proc = self.run_watch()
                os.write(self.writers[2], EVENT.pack(0, 0, kind, code, value))
                self.assertEqual(proc.wait(timeout=1), 0)
                self.assertTrue((work / 'owner-abort').exists())
                self.assertTrue((work / 'stop').exists(), 'all mute/light guards stop before handback')
                self.assertEqual((work / 'restored').read_text().strip(), 'restored')

    def test_neutral_input_does_not_interrupt_and_normal_cleanup_never_restores_twice(self):
        work, proc = self.run_watch()
        for kind, code, value in [(0, 0, 0), (1, 305, 0), (1, 305, 2), (3, 16, 0)]:
            os.write(self.writers[2], EVENT.pack(0, 0, kind, code, value))
        time.sleep(.1)
        self.assertIsNone(proc.poll())
        (work / 'stop').touch()
        self.assertEqual(proc.wait(timeout=1), 0)
        self.assertFalse((work / 'restored').exists())

    def test_lost_host_is_bounded_and_restores_the_owner(self):
        work, proc = self.run_watch(lifetime=1)
        self.assertEqual(proc.wait(timeout=2), 0)
        self.assertTrue((work / 'restored').exists())


if __name__ == '__main__':
    unittest.main()
