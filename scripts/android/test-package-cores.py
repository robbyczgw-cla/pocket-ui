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

    def test_every_core_of_the_lock_is_in_the_apk_and_none_is_a_download(self):
        """0.5.x: PSP with its system files, DS, N64 and Dreamcast are in the APK like the rest."""
        import hashlib, json, os, sys, tempfile
        lock = json.loads((Path(__file__).resolve().parents[2] / 'android/cores.lock.json').read_text())
        with tempfile.TemporaryDirectory() as d:
            out = Path(d) / 'build'
            cache = out / 'cores'
            (cache / 'lib').mkdir(parents=True)
            (out / 'apk/lib/arm64-v8a').mkdir(parents=True)
            (out / 'core-downloads').mkdir(parents=True)
            (out / 'core-downloads/ppsspp-old.tar.gz').write_bytes(b'a download of an earlier build')
            manifest = dict(abi=lock['abi'], ndk=lock['ndk'], cores=[])
            assets = {'compat.ini': b'pinned compat', 'flash0/font/ltn0.pgf': b'pinned font'}
            for core in lock['cores']:
                lib = aligned_elf(core['id'].encode())
                (cache / 'lib' / ('lib%s_libretro.so' % core['id'])).write_bytes(lib)
                entry = {**core, 'sha256': hashlib.sha256(lib).hexdigest(), 'notices': [],
                         'evidence': [{'path': e} for e in core['evidence']]}
                if core['id'] == 'ppsspp':
                    entry['system_assets'] = [{'path': k, 'sha256': hashlib.sha256(v).hexdigest(), 'bytes': len(v)} for k, v in assets.items()]
                    for k, v in assets.items():
                        f = cache / 'source/ppsspp/assets' / k
                        f.parent.mkdir(parents=True, exist_ok=True)
                        f.write_bytes(v)
                manifest['cores'].append(entry)
            (cache / 'manifest.json').write_text(json.dumps(manifest))
            os.environ['ANDROID_CORE_CACHE'] = str(cache)
            try:
                builder.package(out)
            finally:
                del os.environ['ANDROID_CORE_CACHE']
            ids = {c['id'] for c in lock['cores']}
            self.assertTrue({'ppsspp', 'melondsds', 'mupen64plus_next', 'flycast'} <= ids)
            packed = {p.name[3:-len('_libretro.so')] for p in (out / 'apk/lib/arm64-v8a').glob('lib*_libretro.so')}
            self.assertEqual(packed, ids, 'every core of the lock is in the APK')
            self.assertEqual({c['id'] for c in json.loads((out / 'apk/assets/cores.json').read_text())['cores']}, ids)
            self.assertEqual(json.loads((out / 'apk/assets/core-downloads.json').read_text())['packages'], [], 'no core is a download')
            self.assertEqual(json.loads((out / 'core-downloads/catalog.json').read_text())['packages'], [])
            self.assertEqual(list((out / 'core-downloads').glob('*.tar.gz')), [], 'no package of an earlier build stays')
            for k, v in assets.items():
                self.assertEqual((out / 'apk/assets/system/PPSSPP' / k).read_bytes(), v, 'PSP system files beside the core')


def aligned_elf(tag):
    """A minimal AArch64 ELF64 whose LOAD segments are 16 KB aligned (as build-cores.py links them), `tag` after it."""
    import struct
    header = b"\x7fELF" + bytes([2, 1, 1, 0]) + bytes(8) + struct.pack("<HHIQQQIHHHHHH", 3, 183, 1, 0, 64, 0, 0, 64, 56, 2, 0, 0, 0)
    phdrs = b"".join(struct.pack("<IIQQQQQQ", 1, 5, i * 16384, i * 16384, i * 16384, 100, 100, 16384) for i in range(2))
    return header + phdrs + bytes(64) + tag


if __name__ == '__main__':
    unittest.main()
