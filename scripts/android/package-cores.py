#!/usr/bin/env python3
"""Split the measured heavy cores from the APK into normal pocket-store tar packages.

Writes core-downloads/catalog.json; sign its exact bytes with store-tool sign before serving.
No publishing or key access happens here. Corresponding sources stay beside the packages.
"""
import gzip
import hashlib
import io
import os
import time
import json
import shutil
import sys
import tarfile
from pathlib import Path
from apk_audit import check_elf, problems

# Measured deflated libraries: PSP 7.33 MB (+ assets), DS 1.84 MB, N64 1.53 MB,
# Flycast 4.75 MB. Saturn is only 0.69 MB and remains bundled, like PS1.
CANDIDATES = {'ppsspp', 'melondsds', 'mupen64plus_next', 'flycast', 'yabasanshiro'}
MIN_DOWNLOAD_BYTES = 1_500_000
NAMES = {'ppsspp': 'PSP', 'melondsds': 'DS', 'mupen64plus_next': 'N64',
         'flycast': 'Dreamcast', 'yabasanshiro': 'Saturn'}


def make_archive(files):
    archive = io.BytesIO()
    with gzip.GzipFile(fileobj=archive, mode='wb', mtime=0) as zipped:
        # Licence basenames can exceed USTAR's 100-byte limit. The shared extractor accepts PAX path records.
        with tarfile.open(fileobj=zipped, mode='w', format=tarfile.PAX_FORMAT) as tar:
            for name, data in sorted(dict(files).items()):
                entry = tarfile.TarInfo(name)
                entry.size, entry.mode, entry.mtime = len(data), 0o644, 0
                tar.addfile(entry, io.BytesIO(data))
    return archive.getvalue()


def package(out):
    cores = Path(os.environ.get('ANDROID_CORE_CACHE', str(out / 'cores')))
    manifest = json.loads((cores / 'manifest.json').read_text())
    lock = json.loads((Path(__file__).resolve().parents[2] / 'android/cores.lock.json').read_text())
    assert manifest['abi'] == lock['abi'] and manifest['ndk'] == lock['ndk']
    assert {c['id'] for c in manifest['cores']} == {c['id'] for c in lock['cores']}, 'Stale core cache'
    for recorded in lock['cores']:
        actual = next(c for c in manifest['cores'] if c['id'] == recorded['id'])
        assert all(actual.get(k) == v for k,v in recorded.items() if k != 'evidence'), 'Core cache does not match lock: '+recorded['id']
        assert [e['path'] for e in actual['evidence']] == recorded['evidence'], 'Core licence evidence differs from lock'
    target = out / 'core-downloads'
    target.mkdir(exist_ok=True)
    packages, bundled = [], []
    for core in manifest['cores']:
        cid = core['id']
        source = cores / 'lib' / ('lib' + cid + '_libretro.so')
        assert hashlib.sha256(source.read_bytes()).hexdigest() == core['sha256'], cid
        assert check_elf(source.name, source.read_bytes()) and not problems, cid
        large = len(gzip.compress(source.read_bytes(), compresslevel=6, mtime=0)) >= MIN_DOWNLOAD_BYTES
        if cid not in CANDIDATES or (not large and not core.get('system_assets')):
            shutil.copyfile(source, out / 'apk/lib/arm64-v8a' / source.name)
            bundled.append(core)
            continue
        assert core.get('corresponding_source') and core.get('source_sha256'), 'Build with ANDROID_RELEASE_SOURCES=1'
        files = [(source.name, source.read_bytes()), ('core.json', (json.dumps(core, indent=2)+'\n').encode())]
        for notice in core['notices']:
            path = cores / 'licences' / notice['asset']
            files.append(('licences/' + path.name, path.read_bytes()))
        # The patches that were applied to this core's source sit beside its licences, so a downloaded core is complete by itself.
        for patch in core.get('patches', []):
            data = (Path(__file__).resolve().parents[2] / patch['path']).read_bytes()
            assert hashlib.sha256(data).hexdigest() == patch['sha256'], 'Changed recorded core patch'
            files.append(('licences/PATCH--' + Path(patch['path']).name, data))
        if cid == 'ppsspp':
            assert core.get('system_assets'), 'PSP system files missing'
            for asset in core['system_assets']:
                rel = Path(asset['path'])
                assert not rel.is_absolute() and '..' not in rel.parts
                data = (cores / 'source/ppsspp/assets' / rel).read_bytes()
                assert hashlib.sha256(data).hexdigest() == asset['sha256']
                files.append(('system/PPSSPP/' + rel.as_posix(), data))
        data = make_archive(files)
        archive_hash = hashlib.sha256(data).hexdigest()
        filename = cid + '-' + archive_hash[:16] + '.tar.gz'
        (target / filename).write_bytes(data)
        source_archive = cores / 'corresponding-source' / core['corresponding_source']
        assert hashlib.sha256(source_archive.read_bytes()).hexdigest() == core['source_sha256']
        # Keep a reference, not a second copy of a large source tree.
        packages.append(dict(id='android-core-'+cid, name=NAMES[cid], category='android-cores',
            version=os.environ.get('ANDROID_CORE_VERSION', '1.'+str(int(time.time()))), description='Additional system for the built-in player',
            size=len(data), url=filename, sha256=archive_hash, kind='tar', dest='/android-cores',
            core=cid, systems=core['systems'], library_sha256=core['sha256'], license=core['licence'],
            corresponding_source=core['corresponding_source'], source_sha256=core['source_sha256']))
    catalog = dict(version=1, serial=int(os.environ.get('ANDROID_CORE_SERIAL', str(int(time.time())))), packages=packages)
    (target / 'catalog.json').write_text(json.dumps(catalog, indent=2)+'\n')
    (out / 'apk/assets').mkdir(exist_ok=True)
    (out / 'apk/assets/core-downloads.json').write_text(json.dumps(catalog, indent=2)+'\n')
    (out / 'apk/assets/cores.json').write_text(json.dumps({**manifest, 'cores':bundled}, indent=2)+'\n')
    print('APK cores:', ', '.join(c['id'] for c in bundled))
    for p in packages:
        print('Download:', p['name'], p['size'], 'bytes')


if __name__ == '__main__':
    package(Path(sys.argv[1]))
