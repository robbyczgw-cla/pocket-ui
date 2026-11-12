#!/usr/bin/env python3
"""A failed session setup must preserve the owner's FIFO, status and inode."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from device_parking import SavedFile, ParkedFiles, write_if_reader, restore_steps


class ParkingTests(unittest.TestCase):
    def test_the_restore_target_survives_a_loop_over_game_systems(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'system.cfg'; path.write_bytes(b'owner settings')
            system = SavedFile(path)
            restore = system.restore
            for system in ('snes', 'psx', 'gba'):
                path.write_text(system)
            self.assertEqual(restore_steps([restore]), [])
            self.assertEqual(path.read_bytes(), b'owner settings')

    def test_an_exited_core_does_not_prevent_the_owner_from_returning(self):
        with tempfile.TemporaryDirectory() as tmp:
            fifo = Path(tmp) / 'core.cmd'; os.mkfifo(fifo)
            resumed = []
            errors = restore_steps([lambda: write_if_reader(fifo, b'quit nosave\n'), lambda: resumed.append(True)])
            self.assertEqual(errors, [])
            self.assertEqual(resumed, [True])
            self.assertTrue(fifo.exists())

    def test_one_failed_restore_still_resumes_the_owner(self):
        resumed = []
        def broken(): raise OSError('restore failed')
        errors = restore_steps([broken, lambda: resumed.append(True)])
        self.assertEqual(len(errors), 1)
        self.assertEqual(resumed, [True])

    def test_failed_first_move_never_removes_the_owner_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); park = root / 'park'; park.mkdir()
            fifo = root / 'ui.in'; os.mkfifo(fifo)
            status = root / 'ui.status'; status.write_text('owner asleep')
            inode = fifo.stat().st_ino
            files = ParkedFiles(park)
            with patch.object(Path, 'rename', side_effect=OSError('cross-device link')):
                with self.assertRaises(OSError): files.park([fifo, status])
            files.restore()
            self.assertEqual(fifo.stat().st_ino, inode)
            self.assertEqual(status.read_text(), 'owner asleep')

    def test_partial_setup_restores_only_the_successfully_parked_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); park = root / 'park'; park.mkdir()
            fifo = root / 'ui.in'; os.mkfifo(fifo)
            status = root / 'ui.status'; status.write_text('owner asleep')
            inode = fifo.stat().st_ino
            files = ParkedFiles(park)
            files.park([fifo])
            with patch.object(Path, 'rename', side_effect=OSError('interrupted')):
                with self.assertRaises(OSError): files.park([status])
            files.restore()
            self.assertEqual(fifo.stat().st_ino, inode)
            self.assertEqual(status.read_text(), 'owner asleep')

    def test_restore_refuses_an_unknown_replacement(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); park = root / 'park'; park.mkdir()
            status = root / 'ui.status'; status.write_text('owner asleep')
            files = ParkedFiles(park); files.park([status])
            status.write_text('another owner')
            with self.assertRaises(FileExistsError): files.restore()
            self.assertEqual(status.read_text(), 'another owner')
            self.assertEqual((park / status.name).read_text(), 'owner asleep')


if __name__ == '__main__': unittest.main()
