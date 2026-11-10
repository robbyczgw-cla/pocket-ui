#!/usr/bin/env python3
"""Interrupted/corrupt transfers must recover before backups or restores are used."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from importlib.machinery import SourceFileLoader
firmware_path = SourceFileLoader('portability', str(Path(__file__).with_name('test-device-portability.py'))).load_module().firmware_path
import device_transfer as transfer


class Transfers(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.env = dict(os.environ, PATH=firmware_path(self.root, ['/usr/bin/busybox', 'ash']))
        self.shell = str(self.root/'bin/sh')
        self.body = bytes(range(256))*8193
        self.source = self.root/"BIOS archive's source.tar"
        self.source.write_bytes(self.body)
        self.local = self.root/'backup.tar'
        self.target = self.root/"restore's file.tar"
        self.target.write_bytes(b'original must survive failure')
        self.writes = []
        self.reads = []
        self.faults = 0
        self.corrupt = False

    def tearDown(self):
        self.tmp.cleanup()

    def remote(self, command, data=None, timeout=120):
        if command.startswith('cat >>'):
            self.writes.append(len(data))
            if self.faults:
                self.faults -= 1
                partial = data[:65536]
                if self.corrupt: partial = b'x'*len(data)
                subprocess.run([self.shell, '-c', command], input=partial, env=self.env, check=True)
                if not self.corrupt: raise subprocess.TimeoutExpired(command, timeout)
                return b''
        if command.startswith('tail -c'):
            self.reads.append(command)
            if self.faults:
                self.faults -= 1
                result = subprocess.check_output([self.shell, '-c', command], env=self.env)
                if self.corrupt: return b'x'*len(result)
                raise subprocess.TimeoutExpired(command, timeout, output=result[:65536])
        return subprocess.run([self.shell, '-c', command], input=data, env=self.env,
                              capture_output=True, timeout=timeout, check=True).stdout

    def test_interrupted_backup_resumes_and_verifies_before_publishing(self):
        self.faults = 1
        transfer.download(self.remote, str(self.source), self.local)
        self.assertEqual(self.local.read_bytes(), self.body)
        self.assertEqual(len(self.reads), 2)
        self.assertIn('+65537', self.reads[1])
        self.assertFalse(self.local.with_name(self.local.name+'.part').exists())

    def test_interrupted_restore_resumes_and_verifies_before_replacing(self):
        self.faults = 1
        transfer.upload(self.remote, str(self.target), self.body)
        self.assertEqual(self.target.read_bytes(), self.body)
        self.assertEqual(self.writes, [len(self.body), len(self.body)-65536])

    def test_corrupt_full_size_transfer_is_retried_in_both_directions(self):
        for direction in ('upload', 'download'):
            with self.subTest(direction=direction):
                self.faults = 1; self.corrupt = True
                if direction == 'upload':
                    transfer.upload(self.remote, str(self.target), self.body)
                    self.assertEqual(self.target.read_bytes(), self.body)
                else:
                    transfer.download(self.remote, str(self.source), self.local)
                    self.assertEqual(self.local.read_bytes(), self.body)

    def test_two_failures_stop_and_preserve_existing_destination(self):
        for direction in ('upload', 'download'):
            with self.subTest(direction=direction):
                self.faults = 2; self.corrupt = True
                self.local.write_bytes(b'known good backup')
                with self.assertRaises(transfer.TransferError):
                    if direction == 'upload': transfer.upload(self.remote, str(self.target), self.body)
                    else: transfer.download(self.remote, str(self.source), self.local)
                self.assertEqual(self.target.read_bytes(), b'original must survive failure')
                self.assertEqual(self.local.read_bytes(), b'known good backup')

    def test_empty_files_and_cli_preserve_names_with_spaces_and_apostrophes(self):
        transfer.upload(self.remote, str(self.target), b'')
        self.assertEqual(self.target.read_bytes(), b'')
        self.source.write_bytes(b'')
        transfer.download(self.remote, str(self.source), self.local)
        self.assertEqual(self.local.read_bytes(), b'')
        self.source.write_bytes(self.body)
        result = subprocess.run(['/usr/bin/python3', str(Path(transfer.__file__)), '--seconds', '5',
                                 'get', str(self.source), str(self.local), '--', self.shell, '-c'],
                                env=self.env, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.local.read_bytes(), self.body)


if __name__ == '__main__': unittest.main()
