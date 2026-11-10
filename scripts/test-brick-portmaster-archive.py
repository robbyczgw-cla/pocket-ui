#!/usr/bin/env python3
"""Portable PortMaster archives must install actual data on FAT and reject unsafe trees."""
import importlib.util
import io
from pathlib import Path
import stat
import sys
import tarfile
import tempfile
import unittest
import types
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('portable_archive', ROOT / 'deploy/brick/portmaster-archive.py')
archive = importlib.util.module_from_spec(spec)
spec.loader.exec_module(archive)


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='brick-pm-archive-')
        self.root = Path(self.tmp.name)
        self.dest = self.root / 'out'

    def tearDown(self):
        self.tmp.cleanup()

    def make_zip(self, entries):
        target = self.root / 'test.zip'
        with zipfile.ZipFile(target, 'w', compression=zipfile.ZIP_DEFLATED) as out:
            for name, kind, value in entries:
                info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
                info.create_system = 3
                info.external_attr = ((stat.S_IFLNK if kind == 'link' else stat.S_IFREG) | 0o755) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                out.writestr(info, value)
        return target

    def make_tar(self, entries):
        target = self.root / 'test.tar.gz'
        with tarfile.open(target, 'w:gz') as out:
            for name, kind, value in entries:
                info = tarfile.TarInfo(name)
                info.mode = 0o755
                if kind in ('link', 'hardlink'):
                    info.type = tarfile.SYMTYPE if kind == 'link' else tarfile.LNKTYPE
                    info.linkname = value
                    out.addfile(info)
                else:
                    value = value.encode()
                    info.size = len(value)
                    out.addfile(info, io.BytesIO(value))
        return target

    def assert_regular(self, name, data):
        target = self.dest / name
        self.assertFalse(target.is_symlink(), 'archive link survived on the card')
        self.assertEqual(target.read_text(), data)

    def assert_rejected(self, source, **kwargs):
        extract = archive.extract_zip if source.suffix == '.zip' else archive.extract_tar
        with self.assertRaises((ValueError, zipfile.BadZipFile)):
            extract(source, self.dest, **kwargs)
        self.assertFalse(self.dest.exists() and any(self.dest.iterdir()), 'unsafe archive wrote data before rejection')

    def test_zip_file_links_become_real_files_even_when_target_is_later(self):
        source = self.make_zip([('game/lib/lib.so', 'link', 'lib.so.1'), ('game/lib/lib.so.1', 'file', 'ELF')])
        archive.extract_zip(source, self.dest)
        self.assert_regular('game/lib/lib.so', 'ELF')
        self.assert_regular('game/lib/lib.so.1', 'ELF')

    def test_zip_directory_and_link_chain_are_materialized(self):
        source = self.make_zip([('alias', 'link', 'real'), ('real/data', 'file', 'payload'),
                                ('copy', 'link', 'alias/data')])
        archive.extract_zip(source, self.dest)
        self.assert_regular('alias/data', 'payload')
        self.assert_regular('copy', 'payload')

    def test_extract_member_uses_safe_link_target_and_extractall_respects_members(self):
        source = self.make_zip([('game/alias', 'link', 'data'), ('game/data', 'file', 'yes'), ('other', 'file', 'no')])
        with archive.PortableZipFile(source) as out:
            returned = out.extract(out.getinfo('game/alias'), self.dest)
        self.assertEqual(Path(returned), self.dest / 'game/alias')
        self.assert_regular('game/alias', 'yes')
        self.assertFalse((self.dest / 'game/data').exists())

    def test_zip_internal_parent_reference_is_allowed(self):
        source = self.make_zip([('game/data', 'file', 'yes'), ('game/sub/alias', 'link', '../data')])
        archive.extract_zip(source, self.dest)
        self.assert_regular('game/sub/alias', 'yes')

    def test_external_and_missing_links_are_rejected(self):
        for target in ('/etc/passwd', '../../outside', 'C:/outside', 'missing'):
            with self.subTest(target=target):
                self.assert_rejected(self.make_zip([('game/link', 'link', target)]))

    def test_link_and_directory_cycles_are_rejected(self):
        for entries in ([('a', 'link', 'b'), ('b', 'link', 'a')],
                        [('game/data', 'file', 'yes'), ('game/again', 'link', '..')]):
            with self.subTest(entries=entries):
                self.assert_rejected(self.make_zip(entries))

    def test_unsafe_portable_names_are_rejected_before_any_file_is_written(self):
        names = ('../escape', '/absolute', 'a/../../b', 'C:/drive', 'a\\b', 'a//b', 'a/./b',
                 'a/name.', 'a/name ', 'a/b:c', 'a/b*c', 'a/b?c', 'a/b"c', 'a/b<c', 'a/b>c', 'a/b|c', 'a/\x01')
        for name in names:
            with self.subTest(name=name):
                self.assert_rejected(self.make_zip([('valid', 'file', 'first'), (name, 'file', 'bad')]))

    def test_case_aliases_at_any_component_are_rejected(self):
        for names in (('A/x', 'a/y'), ('game/Data', 'game/data'), ('game/é', 'game/e\u0301')):
            with self.subTest(names=names):
                self.assert_rejected(self.make_zip([(name, 'file', 'bad') for name in names]))

    def test_duplicate_files_and_file_directory_collisions_are_rejected(self):
        for names in (('same', 'same'), ('game', 'game/data')):
            with self.subTest(names=names):
                self.assert_rejected(self.make_zip([(name, 'file', 'bad') for name in names]))

    def test_existing_destination_symlink_cannot_escape(self):
        outside = self.root / 'outside'
        outside.mkdir()
        self.dest.mkdir()
        (self.dest / 'game').symlink_to(outside, target_is_directory=True)
        source = self.make_zip([('game/payload', 'file', 'bad')])
        with self.assertRaises((ValueError, zipfile.BadZipFile)):
            archive.extract_zip(source, self.dest)
        self.assertFalse((outside / 'payload').exists())

    def test_existing_case_alias_is_rejected(self):
        self.dest.mkdir()
        (self.dest / 'Game').mkdir()
        with self.assertRaises((ValueError, zipfile.BadZipFile)):
            archive.extract_zip(self.make_zip([('game/payload', 'file', 'bad')]), self.dest)
        self.assertFalse((self.dest / 'Game/payload').exists())

    def test_zip_size_ratio_entry_and_materialization_budgets(self):
        cases = [([('data', 'file', '12345')], dict(max_file=4)),
                 ([('data', 'file', 'A' * 512)], dict(ratio=2, slack=4)),
                 ([('a', 'file', 'x'), ('b', 'file', 'y')], dict(max_entries=1)),
                 ([('data', 'file', '1234'), ('a', 'link', 'data')], dict(max_total=7)),
                 ([('names-one', 'file', 'x'), ('names-two', 'file', 'y')], dict(max_names=12))]
        for entries, limits in cases:
            with self.subTest(limits=limits):
                self.assert_rejected(self.make_zip(entries), limits=archive.Limits(**limits))

    def test_central_directory_metadata_bomb_is_rejected_without_reading_it(self):
        source = self.make_zip([('file', 'file', 'data')])
        data = bytearray(source.read_bytes())
        end = data.rfind(b'PK\x05\x06')
        data[end + 12:end + 16] = (33 * 1024**2).to_bytes(4, 'little')
        source.write_bytes(data)
        with self.assertRaisesRegex(zipfile.BadZipFile, 'central directory exceeds'):
            archive.extract_zip(source, self.dest)
        self.assertFalse(self.dest.exists())

    def test_timestamp_is_ignored(self):
        archive.extract_zip(self.make_zip([('data', 'file', 'yes')]), self.dest)
        self.assertGreater((self.dest / 'data').stat().st_mtime, 1600000000)

    def test_fat_limit_applies_per_file_and_not_to_exfat_or_volume_total(self):
        gib = 1024**3
        large = archive.Tree([archive.Entry('large', 'file', size=5 * gib)], archive.Limits())
        split = archive.Tree([archive.Entry('one', 'file', size=3 * gib),
                              archive.Entry('two', 'file', size=3 * gib)], archive.Limits())
        with patch.object(archive, 'available_bytes', return_value=16 * gib):
            with patch.object(archive, 'filesystem_file_limit', return_value=2**32 - 1):
                with self.assertRaisesRegex(ValueError, 'FAT32 per-file'):
                    archive.destination_budget(large.full_plan, self.dest)
                archive.destination_budget(split.full_plan, self.dest)
            with patch.object(archive, 'filesystem_file_limit', return_value=None):
                archive.destination_budget(large.full_plan, self.dest)
        self.assertFalse(self.dest.exists(), 'metadata budget check unexpectedly wrote a payload')

    def test_free_space_is_checked_before_any_extracted_file_is_written(self):
        source = self.make_zip([('data', 'file', '12345')])
        with patch.object(archive, 'available_bytes', return_value=4):
            self.assert_rejected(source)

    def test_member_installs_do_not_reserve_completed_files_twice(self):
        source = self.make_zip([('one', 'file', '12345'), ('two', 'file', '67890')])
        with archive.PortableZipFile(source) as zf, patch.object(archive, 'available_bytes', side_effect=[11, 6]):
            zf.extract('one', self.dest)
            zf.extract('two', self.dest)
        self.assert_regular('one', '12345')
        self.assert_regular('two', '67890')

    def test_tar_symlinks_and_hardlinks_become_regular_files(self):
        source = self.make_tar([('game/data', 'file', 'yes'), ('game/link', 'link', 'data'),
                                ('hard', 'hardlink', 'game/data'), ('alias', 'link', 'game')])
        archive.extract_tar(source, self.dest)
        for name in ('game/link', 'hard', 'alias/data'):
            self.assert_regular(name, 'yes')

    def test_tar_links_can_resolve_below_directory_aliases(self):
        for prefix in ('', 'upstream/PortMaster/'):
            with self.subTest(prefix=prefix):
                source = self.make_tar([(prefix + 'game/data/file', 'file', 'yes'),
                                        (prefix + 'game/copy', 'link', 'data'),
                                        (prefix + 'game/chain', 'link', 'copy/file')])
                target = self.dest / ('selected' if prefix else 'all')
                (archive.extract_portmaster_tar if prefix else archive.extract_tar)(source, target)
                self.assertEqual((target / ('PortMaster/' if prefix else '') / 'game/chain').read_text(), 'yes')

    def test_tar_invalid_links_names_and_cycles_are_rejected(self):
        for entries in ([('link', 'link', '../outside')], [('link', 'hardlink', '/outside')],
                        [('a', 'link', 'b'), ('b', 'link', 'a')], [('Data/a', 'file', 'x'), ('data/b', 'file', 'y')],
                        [('bad.', 'file', 'x')]):
            with self.subTest(entries=entries):
                self.assert_rejected(self.make_tar(entries))

    def test_portmaster_source_selection_retains_safe_links_and_root_licence(self):
        source = self.make_tar([('upstream/LICENSE', 'file', 'MIT'), ('upstream/README', 'file', 'unused'),
                                ('upstream/PortMaster/lib/lib.so.1', 'file', 'ELF'),
                                ('upstream/PortMaster/lib/lib.so', 'link', 'lib.so.1')])
        archive.extract_portmaster_tar(source, self.dest)
        self.assert_regular('PortMaster/lib/lib.so', 'ELF')
        self.assert_regular('PortMaster/LICENSE-PortMaster.txt', 'MIT')
        self.assertFalse((self.dest / 'README').exists())

    def test_harbourmaster_policy_does_not_replace_global_zipfile(self):
        bridge_spec = importlib.util.spec_from_file_location('bridge', ROOT / 'deploy/brick/portmaster-bridge.py')
        bridge = importlib.util.module_from_spec(bridge_spec)
        bridge_spec.loader.exec_module(bridge)
        module_name = 'harbourmaster._archive_fixture'
        fixture = types.ModuleType(module_name)
        fixture.zipfile = zipfile
        fixture.ZipFile = zipfile.ZipFile
        standard = zipfile.ZipFile
        sys.modules[module_name] = fixture
        try:
            portable = bridge.install_archive_policy()
            self.assertIs(zipfile.ZipFile, standard)
            self.assertIs(fixture.zipfile.ZipFile, portable)
            self.assertIs(fixture.ZipFile, portable)
            source = self.make_zip([('game/data', 'file', 'ELF'), ('game/link', 'link', 'data')])
            with fixture.zipfile.ZipFile(source) as out:
                out.extract(out.getinfo('game/link'), self.dest)
            self.assert_regular('game/link', 'ELF')
        finally:
            del sys.modules[module_name]


if __name__ == '__main__':
    unittest.main()
