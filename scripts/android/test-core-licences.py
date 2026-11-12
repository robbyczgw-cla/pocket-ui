#!/usr/bin/env python3
"""Regression test of scripts/android/core_licences.py: the build stops when a core lacks its licence, source or patches.

    python3 -I scripts/android/test-core-licences.py

The first 0.4 preview had licences for the bundled cores only by accident of the build; nothing asserted that every core,
the optional downloads too, carries licence texts, a pinned revision, a source archive and its patches.
"""
import copy
import gzip
import hashlib
import importlib.util
import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('core_licences', Path(__file__).with_name('core_licences.py'))
cl = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cl)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def tar_gz(files):
    raw = io.BytesIO()
    with gzip.GzipFile(fileobj=raw, mode='wb', mtime=0) as z, tarfile.open(fileobj=z, mode='w', format=tarfile.PAX_FORMAT) as tar:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
    return raw.getvalue()


class Fixture:
    """A tiny repository root and core cache with one patched core (`demo`) and one plain one (`plain`)."""

    def __init__(self, d):
        self.root, self.cache, self.sources, self.downloads = (d / n for n in ('root', 'cache', 'sources', 'downloads'))
        for p in (self.root / 'android/patches', self.cache / 'licences', self.sources, self.downloads):
            p.mkdir(parents=True)
        patch = b'--- a\n+++ b\n'
        (self.root / 'android/patches/demo.patch').write_bytes(patch)
        self.patch = {'path': 'android/patches/demo.patch', 'sha256': sha(patch)}
        cores = []
        for cid, patches in (('demo', [self.patch]), ('plain', [])):
            text = ('licence of ' + cid).encode()
            (self.cache / 'licences' / (cid + '--LICENSE')).write_bytes(text)
            archive = ('%s-%s.tar.gz' % (cid, 'a' * 40))
            (self.sources / archive).write_bytes(b'source of ' + cid.encode())
            cores.append({'id': cid, 'systems': ['x'], 'source': 'https://github.com/example/' + cid, 'revision': 'a' * 40, 'licence': 'GPL-2.0-or-later',
                          'evidence': [{'path': 'LICENSE', 'sha256': sha(text)}], 'patches': patches,
                          'notices': [{'path': 'LICENSE', 'sha256': sha(text), 'asset': cid + '--LICENSE'}],
                          'corresponding_source': archive, 'source_sha256': sha(b'source of ' + cid.encode())})
        self.manifest = {'cores': cores}
        self.lock = {'cores': [{'id': c['id'], 'patches': c['patches']} for c in cores]}
        # demo is an optional download, packed the way package-cores.py does.
        pkg = tar_gz({'libdemo_libretro.so': b'lib', 'core.json': b'{}', 'licences/demo--LICENSE': b'licence of demo', 'licences/PATCH--demo.patch': patch})
        (self.downloads / 'demo.tar.gz').write_bytes(pkg)
        (self.downloads / 'catalog.json').write_text(json.dumps({'packages': [
            {'core': 'demo', 'url': 'demo.tar.gz', 'sha256': sha(pkg), 'license': 'GPL-2.0-or-later', 'corresponding_source': cores[0]['corresponding_source']}]}))

    def problems(self, manifest=None, sources=True, downloads=True):
        return cl.check(manifest or self.manifest, self.lock, self.cache, self.sources if sources else None,
                        self.downloads if downloads else None, root=self.root)


class CoreLicences(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.f = Fixture(Path(self.tmp.name))

    def tearDown(self):
        self.tmp.cleanup()

    def broken(self, edit):
        m = copy.deepcopy(self.f.manifest)
        edit(m['cores'][0])
        return self.f.problems(m)

    def test_a_complete_set_passes(self):
        self.assertEqual(self.f.problems(), [])

    def test_the_real_lock_and_patches_pass_with_a_synthetic_cache(self):
        # Every core of android/cores.lock.json names a licence, a full commit, evidence, and its patch files exist with the recorded hashes.
        lock = cl.lock()
        cores = []
        with tempfile.TemporaryDirectory() as d:
            cache = Path(d)
            (cache / 'licences').mkdir()
            for c in lock['cores']:
                (cache / 'licences' / (c['id'] + '--LICENSE')).write_bytes(b'x')
                cores.append({**c, 'evidence': [{'path': e, 'sha256': sha(b'x')} for e in c['evidence']],
                              'notices': [{'asset': c['id'] + '--LICENSE', 'sha256': sha(b'x')}],
                              'corresponding_source': c['id'] + '.tar.gz', 'source_sha256': sha(b'y')})
            self.assertEqual(cl.check({'cores': cores}, lock, cache), [])
            text = cl.offer({'cores': cores}, '0.4.0-android.1', '', {'ppsspp'})
            for c in lock['cores']:
                self.assertIn(c['revision'], text, c['id'])
                self.assertIn(c['source'].removeprefix('https://'), text)
                for p in c.get('patches', []):
                    self.assertIn(Path(p['path']).name, text)
            self.assertNotIn('@', text.replace('@@', ''), 'a placeholder of the template is left')
            self.assertIn('ppsspp (psp), optional download', text)

    def test_each_missing_piece_stops_the_build(self):
        cases = [
            ('no licence', lambda c: c.update(licence=''), 'no licence named'),
            ('moving branch instead of a commit', lambda c: c.update(revision='master'), 'full commit'),
            ('no source address', lambda c: c.update(source='git://x'), 'https source'),
            ('no evidence', lambda c: c.update(evidence=[]), 'licence evidence'),
            ('no licence texts', lambda c: c.update(notices=[]), 'no licence texts'),
            ('a text that is not in the cache', lambda c: c['notices'].append({'asset': 'gone', 'sha256': 'a' * 64}), 'gone is missing'),
            ('a text changed after the build', lambda c: c['notices'][0].update(sha256='b' * 64), 'changed after the build'),
            ('no source archive', lambda c: (c.pop('corresponding_source'), c.pop('source_sha256')), 'ANDROID_RELEASE_SOURCES=1'),
            ('patches the build did not apply', lambda c: c.update(patches=[]), 'differ from the lock'),
        ]
        for label, edit, why in cases:
            got = self.broken(edit)
            self.assertTrue(any(why in p for p in got), '%s: %s' % (label, got))
            self.assertTrue(all(p.startswith(('demo:', 'download demo:')) for p in got), got)

    def test_a_patch_file_that_is_gone_or_changed_is_refused(self):
        (self.f.root / 'android/patches/demo.patch').write_bytes(b'changed')
        self.assertTrue(any('differs from the recorded one' in p for p in self.f.problems()))
        (self.f.root / 'android/patches/demo.patch').unlink()
        self.assertTrue(any('demo.patch is missing' in p for p in self.f.problems()))

    def test_a_missing_or_changed_source_archive_is_refused(self):
        (self.f.sources / self.f.manifest['cores'][1]['corresponding_source']).write_bytes(b'moved branch')
        got = self.f.problems()
        self.assertTrue(any(p.startswith('plain: source archive') for p in got), got)
        (self.f.sources / self.f.manifest['cores'][0]['corresponding_source']).unlink()
        self.assertEqual(sum('source archive' in p for p in self.f.problems()), 2)

    def test_a_core_that_is_not_in_the_lock_or_not_built_is_refused(self):
        m = copy.deepcopy(self.f.manifest)
        m['cores'].pop()
        self.assertTrue(any('differ from android/cores.lock.json' in p for p in self.f.problems(m)))

    def test_a_download_package_must_carry_the_licences_and_patches_of_its_core(self):
        pkg = tar_gz({'libdemo_libretro.so': b'lib', 'core.json': b'{}', 'licences/demo--LICENSE': b'x'})
        (self.f.downloads / 'demo.tar.gz').write_bytes(pkg)
        catalog = json.loads((self.f.downloads / 'catalog.json').read_text())
        catalog['packages'][0]['sha256'] = sha(pkg)
        (self.f.downloads / 'catalog.json').write_text(json.dumps(catalog))
        got = self.f.problems()
        self.assertTrue(any('licences/PATCH--demo.patch' in p for p in got), got)
        catalog['packages'][0]['license'] = 'MIT'
        (self.f.downloads / 'catalog.json').write_text(json.dumps(catalog))
        self.assertTrue(any('another licence' in p for p in self.f.problems()))

    def test_the_offer_names_every_core_with_commit_source_hash_and_patches(self):
        text = cl.offer(self.f.manifest, '0.4.0-android.1', 'https://github.com/o/r/releases/tag/v1', {'demo'})
        for needle in ('github.com/example/demo, commit ' + 'a' * 40, 'demo-' + 'a' * 40 + '.tar.gz', 'sha256 ' + sha(b'source of demo'),
                       'patch:    demo.patch (sha256 ' + self.f.patch['sha256'], 'patches:  none', 'https://github.com/o/r/issues',
                       'https://github.com/o/r/releases/tag/v1', 'Written offer', 'Source request', 'demo (x), optional download', 'plain (x), in the APK'):
            self.assertIn(needle, text)
        rows = cl.listing(self.f.manifest, {'demo'}).splitlines()
        self.assertEqual([r.split('\t')[0] for r in rows], ['demo', 'plain'])
        self.assertEqual(rows[0].split('\t')[5:], ['download', 'demo.patch'])
        self.assertEqual(rows[1].split('\t')[5:], ['apk', ''])
        self.assertTrue(all(len(r.split('\t')) == 7 for r in rows))

    def test_the_channel_file_is_empty_by_default_and_checks_its_address(self):
        real = ROOT_CHANNEL
        self.assertEqual(cl.channel(real), [], 'the shipped core-channel.url is valid')
        urls = [l for l in real.read_text().splitlines() if l.strip() and not l.startswith('#')]
        self.assertEqual(urls, [], 'the channel stays closed until it is made public: the systems read "Coming soon"')
        f = Path(self.tmp.name) / 'c.url'
        for text, ok in (('', True), ('# only a comment\n', True), ('https://example.org/store/catalog.json\n', True),
                         ('http://example.org/catalog.json\n', False), ('https://a/catalog.json\nhttps://b/catalog.json\n', False),
                         ('https://example.org/other.json\n', False)):
            f.write_text(text)
            self.assertEqual(cl.channel(f) == [], ok, text)


ROOT_CHANNEL = Path(__file__).resolve().parents[2] / 'android' / 'core-channel.url'

if __name__ == '__main__':
    unittest.main()
