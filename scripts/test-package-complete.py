#!/usr/bin/env python3
"""Tests of scripts/package-complete.py with tiny fake packages; no build, no device: python3 -I scripts/test-package-complete.py

Red before (20.11.2026): the 0.6 Brick card had ppsspp_libretro.so without PPSSPP's system files (every PSP game ended
after 1 s); the same check on the real card and its card-update zip reported the 5 files missing."""
import importlib.util
import io
import json
import tarfile
import tempfile
import unittest
import zipfile
from contextlib import redirect_stdout
from pathlib import Path

spec = importlib.util.spec_from_file_location('complete', Path(__file__).with_name('package-complete.py'))
complete = importlib.util.module_from_spec(spec)
spec.loader.exec_module(complete)

SCUMM = ['scummvm/extra/sky.cpt', 'scummvm/theme/scummmodern.zip']


def tree(root, names, data=b'x'):
    for name in names:
        p = Path(root) / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)


def run(*args):
    out = io.StringIO()
    with redirect_stdout(out):
        rc = complete.main(['package-complete.py', *map(str, args)])
    return rc, out.getvalue()


class Brick(unittest.TestCase):
    def card(self, root, cores, data):
        app = 'pocket-ui/app/'
        tree(root, [app + f'cores/{c}_libretro.so' for c in cores] + [app + 'bios-data/' + d for d in data])
        return Path(root)

    def test_psp_without_its_system_files_fails_like_the_060_card(self):
        with tempfile.TemporaryDirectory() as t:
            card = self.card(t, ['gambatte', 'ppsspp', 'scummvm'], SCUMM)
            rc, out = run('brick', card)
            self.assertEqual(rc, 1)
            self.assertIn('MISSING ppsspp: 5 of 5', out)
            self.assertIn('PPSSPP/ppge_atlas.zim', out)

    def test_card_with_every_core_s_files_is_complete(self):
        with tempfile.TemporaryDirectory() as t:
            card = self.card(t, ['gambatte', 'ppsspp', 'scummvm'], SCUMM + complete.PSP)
            self.assertEqual(run('brick', card)[0], 0)

    def test_an_empty_system_file_counts_as_missing(self):
        with tempfile.TemporaryDirectory() as t:
            card = self.card(t, ['ppsspp'], complete.PSP)
            (card / 'pocket-ui/app/bios-data/PPSSPP/ppge_atlas.zim').write_bytes(b'')
            rc, out = run('brick', card)
            self.assertEqual(rc, 1)
            self.assertIn('1 of 5', out)

    def test_files_only_in_roms_bios_do_not_count(self):
        # A Wi-Fi update replaces app/ only; Roms/bios is seeded from app/bios-data by start.sh.
        with tempfile.TemporaryDirectory() as t:
            card = self.card(t, ['ppsspp'], [])
            tree(t, ['Roms/bios/' + f for f in complete.PSP])
            self.assertEqual(run('brick', card)[0], 1)

    def test_a_new_core_must_say_what_it_needs(self):
        with tempfile.TemporaryDirectory() as t:
            card = self.card(t, ['gambatte', 'brandnew'], [])
            rc, out = run('brick', card)
            self.assertEqual(rc, 1)
            self.assertIn('brandnew: new core without an entry in NEEDS', out)

    def test_a_core_es_systems_offers_must_be_on_the_card(self):
        with tempfile.TemporaryDirectory() as t:
            card = self.card(t, ['gpsp'], [])
            (card / 'pocket-ui/app/es_systems.cfg').write_text(
                '<systemList><system><name>gba</name><emulators><emulator name="pocket-core"><cores>'
                '<core default="true">gpsp</core><core>mgba</core></cores></emulator></emulators></system></systemList>')
            rc, out = run('brick', card)
            self.assertEqual(rc, 1)
            self.assertIn('mgba: es_systems.cfg offers it', out)

    def test_card_zip_and_wifi_update_are_read_like_the_folder(self):
        with tempfile.TemporaryDirectory() as t:
            card = self.card(Path(t) / 'card', ['ppsspp'], [])
            with zipfile.ZipFile(Path(t) / 'card.zip', 'w') as z:
                for p in card.rglob('*'):
                    if p.is_file():
                        z.write(p, p.relative_to(card).as_posix())
            self.assertEqual(run('brick', Path(t) / 'card.zip')[0], 1)
            # The Wi-Fi update: cores and bios-data are separate packages listed in asset-packages.json.
            assets = [{'path': 'cores/ppsspp_libretro.so', 'size': 9}] + [{'path': 'bios-data/' + f, 'size': 9} for f in complete.PSP]
            for listed, want in ((assets, 0), (assets[:1], 1)):
                buf = io.BytesIO()
                with tarfile.open(fileobj=buf, mode='w:gz') as tar:
                    body = json.dumps({'format': 1, 'assets': listed}).encode()
                    info = tarfile.TarInfo('asset-packages.json')
                    info.size = len(body)
                    tar.addfile(info, io.BytesIO(body))
                update = Path(t) / 'update.zip'
                with zipfile.ZipFile(update, 'w') as z:
                    z.writestr('pocket-ui-brick-9.9.9.tar.gz', buf.getvalue())
                    z.writestr('update.json', '{}')
                self.assertEqual(run('brick', update)[0], want, listed)


class Android(unittest.TestCase):
    def apk(self, path, cores, system, listed=None):
        with zipfile.ZipFile(path, 'w') as z:
            for c in cores:
                z.writestr(f'lib/arm64-v8a/lib{c}_libretro.so', b'x')
            for f in system:
                z.writestr('assets/system/' + f, b'x')
            if listed is not None:
                z.writestr('assets/cores.json', json.dumps({'cores': [{'id': c} for c in listed]}))
        return path

    def test_apk_with_psp_files_is_complete_and_without_fails(self):
        with tempfile.TemporaryDirectory() as t:
            self.assertEqual(run('android', self.apk(Path(t) / 'a.apk', ['mgba', 'ppsspp'], complete.PSP))[0], 0)
            self.assertEqual(run('android', self.apk(Path(t) / 'b.apk', ['mgba', 'ppsspp'], []))[0], 1)

    def test_a_listed_core_must_be_in_the_apk(self):
        with tempfile.TemporaryDirectory() as t:
            rc, out = run('android', self.apk(Path(t) / 'a.apk', ['mgba'], [], ['mgba', 'dsperate']))
            self.assertEqual(rc, 1)
            self.assertIn('dsperate: assets/cores.json lists it', out)


class DarkOS(unittest.TestCase):
    def test_own_cores_pass_and_host_psp_is_a_device_check(self):
        with tempfile.TemporaryDirectory() as t:
            tree(t, ['pocket-ui/app/bin/cores/dsperate_libretro.so'])
            rc, out = run('darkos', t)
            self.assertEqual(rc, 0)
            self.assertIn("device check (dArkOS's own ppsspp): roms/bios/PPSSPP/ppge_atlas.zim", out)

    def test_a_shipped_psp_core_without_a_bios_folder_fails(self):
        with tempfile.TemporaryDirectory() as t:
            tree(t, ['pocket-ui/app/bin/cores/ppsspp_libretro.so'])
            self.assertEqual(run('darkos', t)[0], 1)


class RocknixOnly(unittest.TestCase):
    def test_brick_card_and_darkos_port_reject_hakux(self):
        for platform, core in [('brick', 'app/cores/gpsp_libretro.so'), ('darkos', 'app/bin/cores/gpsp_libretro.so')]:
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as t:
                tree(t, [core, 'app/hakux/hakux_libretro.so'])
                rc, out = run(platform, t)
                self.assertEqual(rc, 1)
                self.assertIn('ROCKNIX hakuX bundle', out)


class RocknixBundle(unittest.TestCase):
    """Red before (25.11.2026, AUDIT-071): the real 0.7.1 install zip, card zip, OTA tarball and offline update had no hakuX
    and no bsnes-hd, and nothing said so; the check on them names both (the 0.7.2 ones: bsnes-hd)."""
    PARTS = [rel for rel, _ in complete.ROCKNIX_BUNDLE]

    def layouts(self, t, skip=None):
        """The four ROCKNIX payloads with everything but `skip` (a ROCKNIX_BUNDLE path) in them."""
        def files(home, hakux_home):
            names = {home + 'pocket-ui': b'x', home + 'pocket-core': b'x'}
            for rel in self.PARTS:
                if rel != skip:
                    names[(hakux_home if rel.startswith('hakux/') else home) + rel] = b'x'
            return names

        def tgz(names):
            buf = io.BytesIO()
            with tarfile.open(fileobj=buf, mode='w:gz') as tf:
                for name, data in names.items():
                    info = tarfile.TarInfo('./' + name)
                    info.size = len(data)
                    tf.addfile(info, io.BytesIO(data))
            return buf.getvalue()

        out = {}
        install = Path(t) / 'pocket-ui-9.8.7-rocknix-install.zip'
        with zipfile.ZipFile(install, 'w') as z:
            for name, data in files('pocket-ui-9.8.7-rocknix-install/files/', 'pocket-ui-9.8.7-rocknix-install/').items():
                z.writestr(name, data)
        out['install zip'] = install
        seed = tgz(files('pocket-ui/releases/9.8.7/', 'pocket-ui/releases/9.8.7/'))
        card = Path(t) / 'pocket-ui-9.8.7-rocknix-card.zip'
        with zipfile.ZipFile(card, 'w') as z:
            z.writestr('mount-storage.sh', 'hook')
            z.writestr('pocket-ui-seed.tar.gz', seed)
        out['card zip'] = card
        ota = Path(t) / 'pocket-ui-9.8.7-aarch64.tar.gz'
        ota.write_bytes(tgz(files('', '')))
        out['OTA tarball'] = ota
        offline = Path(t) / 'pocket-ui-update-9.8.7-rocknix.zip'
        with zipfile.ZipFile(offline, 'w') as z:
            z.writestr('update.json', '{}')
            z.writestr(ota.name, ota.read_bytes())
        out['offline update'] = offline
        return out

    def test_every_rocknix_package_with_all_four_is_complete(self):
        with tempfile.TemporaryDirectory() as t:
            for what, path in self.layouts(t).items():
                with self.subTest(package=what):
                    rc, out = run('rocknix', path)
                    self.assertEqual(rc, 0, out)
                    self.assertIn('hakuX, gpSP, DSperate and bsnes-hd', out)

    def test_each_missing_part_is_red_in_every_package(self):
        for skip, what in complete.ROCKNIX_BUNDLE:
            with tempfile.TemporaryDirectory() as t:
                for package, path in self.layouts(t, skip).items():
                    with self.subTest(missing=skip, package=package):
                        rc, out = run('rocknix', path)
                        self.assertEqual(rc, 1, out)
                        self.assertIn(f'MISSING {what}: ', out)
                        self.assertIn(skip, out)

    def test_an_empty_file_counts_as_missing_and_a_non_package_is_red(self):
        with tempfile.TemporaryDirectory() as t:
            tree(t, ['pocket-ui', 'pocket-core'] + [rel for rel in self.PARTS if rel != 'cores/gpsp_libretro.so'])
            tree(t, ['cores/gpsp_libretro.so'], data=b'')
            rc, out = run('rocknix', t)
            self.assertEqual(rc, 1)
            self.assertIn('gpSP', out)
        with tempfile.TemporaryDirectory() as t:
            tree(t, ['cores/gpsp_libretro.so'])
            rc, out = run('rocknix', t)
            self.assertEqual(rc, 1)
            self.assertIn('not a ROCKNIX package', out)


class Tree(unittest.TestCase):
    def test_unpacked_storage_tree(self):
        with tempfile.TemporaryDirectory() as t:
            tree(t, ['pocket-ui/releases/0.6.0/cores/ppsspp_libretro.so'] + ['roms/bios/' + f for f in complete.PSP])
            args = ('tree', t, '--cores', 'pocket-ui/releases/*/cores', '--system', 'roms/bios')
            self.assertEqual(run(*args)[0], 0)
            (Path(t) / 'roms/bios/PPSSPP/compat.ini').unlink()
            self.assertEqual(run(*args)[0], 1)

    def test_wrong_call_is_2(self):
        self.assertEqual(run('nope', '.')[0], 2)
        self.assertEqual(run('brick', '/does/not/exist')[0], 2)


if __name__ == '__main__':
    unittest.main()
