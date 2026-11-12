#!/usr/bin/env python3
"""Real notice names must fit the downloadable archive without dropping licence files."""
import importlib.util
import io
from pathlib import Path
import tarfile
import unittest

spec = importlib.util.spec_from_file_location('package_cores', Path(__file__).with_name('package-cores.py'))
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class CorePackages(unittest.TestCase):
    def test_long_notices_and_psp_system_files_survive_roundtrip(self):
        files = [('licences/ppsspp-' + 'source-header-' * 14 + '.txt', b'licence text'),
                 ('system/PPSSPP/flash0/font/ltn0.pgf', b'pinned font'),
                 ('libppsspp_libretro.so', b'core')]
        data = builder.make_archive(files)
        with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as tar:
            self.assertEqual(sorted(tar.getnames()), sorted(name for name, _ in files))
            for name, contents in files:
                self.assertEqual(tar.extractfile(name).read(), contents)
        self.assertEqual(data, builder.make_archive(reversed(files)), 'the same payload has the same archive hash')


if __name__ == '__main__':
    unittest.main()
