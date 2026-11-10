#!/usr/bin/env python3
"""Real loop-mounted FAT32/exFAT/ext4 archive matrix; no device or game execution.

Requires passwordless sudo mount/umount, mkfs.vfat, mkfs.exfat, mkfs.ext4,
and an already-built Linux pocket-ui with --unpack-runtime. Never builds Cargo.
Only creates images/mounts below a dedicated temporary folder and always unmounts.
"""
import argparse
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PYTHON_SHA256 = '6547ff0fe1d6b9c22dd1fbf4bfb676533a2541e294e43345806f9861552bad77'
PORTMASTER_SHA256 = '85c9cf08b187574e83f11af6826ccb035a5fa989d1621c66dd50095a12cd1244'
PORTMASTER_REVISION = '9ed29285ecbb0610b7c86a0b77d0d9d1c74efd49'
CCELESTE_MD5 = '2952660321ffb17f151c4a3d4d59d84c'


def digest(path, algorithm='sha256'):
    result = hashlib.new(algorithm)
    with Path(path).open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            result.update(chunk)
    return result.hexdigest()


def command(args, expected=0):
    started = time.monotonic()
    result = subprocess.run(list(map(str, args)), capture_output=True, text=True, timeout=180)
    record = {'command': list(map(str, args)), 'seconds': round(time.monotonic() - started, 3),
              'exit': result.returncode, 'stdout': result.stdout[-8192:], 'stderr': result.stderr[-8192:]}
    if expected is not None and ((expected == 0 and result.returncode != 0) or (expected != 0 and result.returncode == 0)):
        raise RuntimeError(json.dumps(record, indent=2))
    return record


def make_fixtures(root):
    entries = [('game/lib/lib.so', 'link', 'lib.so.1'),
               ('game/lib/lib.so.1', 'file', 'actual library contents'),
               ('game/data/file', 'file', 'asset contents'),
               ('game/copy', 'link', 'data'),
               ('game/chain', 'link', 'copy/file')]
    good_zip, bad_zip = root / 'portable.zip', root / 'unsafe.zip'
    with zipfile.ZipFile(good_zip, 'w', zipfile.ZIP_DEFLATED) as out:
        for name, kind, value in entries:
            info = zipfile.ZipInfo(name)
            info.create_system = 3
            info.external_attr = ((stat.S_IFLNK if kind == 'link' else stat.S_IFREG) | 0o755) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            out.writestr(info, value)
    with zipfile.ZipFile(bad_zip, 'w') as out:
        out.writestr('first', 'must not appear')
        out.writestr('bad:name', 'unsupported')
    good_tar, bad_tar = root / 'portable.tar.gz', root / 'unsafe.tar.gz'
    with tarfile.open(good_tar, 'w:gz') as out:
        for name, kind, value in entries + [('hard', 'hardlink', 'game/lib/lib.so.1')]:
            info = tarfile.TarInfo(name)
            info.mode = 0o755
            if kind in ('link', 'hardlink'):
                info.type = tarfile.SYMTYPE if kind == 'link' else tarfile.LNKTYPE
                info.linkname = value
                out.addfile(info)
            else:
                data = value.encode()
                info.size = len(data)
                out.addfile(info, io.BytesIO(data))
    with tarfile.open(bad_tar, 'w:gz') as out:
        info = tarfile.TarInfo('bad:name')
        info.size = 3
        out.addfile(info, io.BytesIO(b'bad'))
    return good_zip, bad_zip, good_tar, bad_tar


def check_fixture(root, hard=False):
    expected = {'game/lib/lib.so': 'actual library contents',
                'game/lib/lib.so.1': 'actual library contents',
                'game/data/file': 'asset contents', 'game/copy/file': 'asset contents',
                'game/chain': 'asset contents'}
    if hard:
        expected['hard'] = 'actual library contents'
    for name, data in expected.items():
        path = root / name
        if path.is_symlink() or path.read_text() != data:
            raise AssertionError('link materialization failed: ' + str(path))


def file_manifest(root):
    manifest = {}
    for path in sorted(root.rglob('*')):
        if path.is_symlink():
            raise AssertionError('archive left an actual link: ' + str(path))
        if path.is_file():
            manifest[str(path.relative_to(root))] = {'size': path.stat().st_size, 'sha256': digest(path)}
        elif not path.is_dir():
            raise AssertionError('archive left a special file: ' + str(path))
    return manifest


def elf_aarch64(path):
    with path.open('rb') as source:
        header = source.read(20)
    if header[:6] != b'\x7fELF\x02\x01' or int.from_bytes(header[18:20], 'little') != 183:
        raise AssertionError('real aarch64 payload missing: ' + str(path))


def filesystem_run(kind, root, args, fixtures):
    image, mount = root / (kind + '.img'), root / ('mount-' + kind)
    with image.open('wb') as out:
        out.truncate(args.image_mib * 1024 * 1024)
    mount.mkdir()
    formats = {'vfat': ['mkfs.vfat', '-F', '32', '-n', 'PMTEST'],
               'exfat': ['mkfs.exfat', '-L', 'PMTEST'],
               'ext4': ['mkfs.ext4', '-q', '-F', '-L', 'PMTEST']}
    record = {'filesystem': kind, 'format': command([*formats[kind], image]), 'operations': []}
    options = 'loop,nodev,nosuid'
    if kind != 'ext4':
        options += ',uid=%d,gid=%d,fmask=0022,dmask=0022' % (os.getuid(), os.getgid())
    mounted, loop_device = False, None
    try:
        mounted_result = command(['sudo', '-n', 'mount', '-t', kind, '-o', options, image, mount], expected=None)
        if mounted_result['exit'] != 0:
            if kind != 'exfat' or not shutil.which('mount.exfat-fuse') or 'unknown filesystem type' not in mounted_result['stderr']:
                raise RuntimeError(json.dumps(mounted_result, indent=2))
            # A real exFAT loop volume through its FUSE driver is still exFAT;
            # never label a plain-directory or ext4 substitute as an exFAT test.
            loop_device = command(['sudo', '-n', 'losetup', '--find', '--show', image])['stdout'].strip()
            mounted_result = command(['sudo', '-n', 'mount.exfat-fuse', '-o', options.removeprefix('loop,'), loop_device, mount])
            record['mount_driver'] = 'exfat-fuse'
        else:
            record['mount_driver'] = 'kernel'
        mounted = True
        if kind == 'ext4':
            command(['sudo', '-n', 'chown', '%d:%d' % (os.getuid(), os.getgid()), mount])
        mount_info = command(['findmnt', '-J', '--target', mount])
        record['mount'] = json.loads(mount_info['stdout'])['filesystems'][0]
        expected_driver_type = 'fuseblk' if record['mount_driver'] == 'exfat-fuse' else kind
        if record['mount']['fstype'] != expected_driver_type:
            raise AssertionError('not the requested real filesystem: ' + str(record['mount']))
        record['disk_format'] = command(['blkid', '-p', '-s', 'TYPE', '-o', 'value', image])['stdout'].strip()
        if record['disk_format'] != kind:
            raise AssertionError('mounted image has wrong real disk format')
        probe = mount / 'CaseProbe'
        probe.write_text('case')
        record['case_insensitive'] = (mount / 'caseprobe').exists()
        if record['case_insensitive'] != (kind != 'ext4'):
            raise AssertionError('unexpected case behavior for ' + kind)
        link = mount / 'NativeLink'
        try:
            link.symlink_to('CaseProbe')
            record['native_symlinks'] = True
            link.unlink()
        except OSError as error:
            record['native_symlinks'] = False
            record['native_symlink_errno'] = error.errno
        if record['native_symlinks'] != (kind == 'ext4'):
            raise AssertionError('unexpected native link behavior for ' + kind)
        probe.unlink()
        script = mount / 'exec-probe.sh'
        script.write_text('#!/bin/sh\nprintf FS_OK\n')
        script.chmod(0o755)
        if command([script])['stdout'] != 'FS_OK':
            raise AssertionError('mount cannot execute normal card files')
        script.unlink()
        spec = importlib.util.spec_from_file_location('matrix_archive', ROOT / 'deploy/brick/portmaster-archive.py')
        portable_policy = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(portable_policy)
        record['detected_file_limit'] = portable_policy.filesystem_file_limit(mount)
        if (record['detected_file_limit'] is not None) != (kind == 'vfat'):
            raise AssertionError('FAT file ceiling applied to wrong real filesystem')
        big_metadata = portable_policy.Tree([portable_policy.Entry('large', 'file', size=5 * 1024**3)], portable_policy.Limits())
        try:
            portable_policy.destination_budget(big_metadata.full_plan, mount)
        except ValueError as error:
            record['metadata_5gib_result'] = str(error)
            if ('FAT32 per-file' in str(error)) != (kind == 'vfat'):
                raise AssertionError('5 GiB metadata uses the wrong filesystem limit')
        else:
            raise AssertionError('5 GiB should exceed this small test image free space')
        if args.probe_only:
            record['status'] = 'mounted-and-probed'
            return record, None
        good_zip, bad_zip, good_tar, bad_tar = fixtures
        portable = ROOT / 'deploy/brick/portmaster-archive.py'
        for label, cmd, expected_hard in (
                ('rust-zip', [args.unpacker, '--unpack-runtime', good_zip], False),
                ('python-zip', [sys.executable, portable, 'zip', good_zip], False),
                ('python-tar', [sys.executable, portable, 'tar', good_tar], True)):
            target = mount / label
            record['operations'].append(command([*cmd, target]))
            check_fixture(target, hard=expected_hard)
        for label, cmd in (
                ('bad-rust', [args.unpacker, '--unpack-runtime', bad_zip]),
                ('bad-zip', [sys.executable, portable, 'zip', bad_zip]),
                ('bad-tar', [sys.executable, portable, 'tar', bad_tar])):
            target = mount / label
            record['operations'].append(command([*cmd, target], expected=1))
            if target.exists() and any(target.iterdir()):
                raise AssertionError('unsafe archive wrote before preflight rejection')
        payload = mount / 'payload'
        record['operations'].append(command([args.unpacker, '--unpack-runtime', args.python_zip, payload / 'runtime']))
        # The real runtime helper applies this after extraction; FAT mount masks
        # provide executable permissions independently of archive Unix metadata.
        (payload / 'runtime/bin/python3').chmod(0o755)
        record['operations'].append(command([sys.executable, ROOT / 'deploy/brick/portmaster-extract.py',
                                             args.portmaster_tar, payload]))
        record['operations'].append(command([sys.executable, ROOT / 'deploy/brick/portmaster-extract.py',
                                             '--prepare', payload, PORTMASTER_REVISION]))
        fonts = payload / 'PortMaster/pylibs/resources'
        expected_fonts = {'NotoSansHK-Regular.ttf', 'NotoSansJP-Regular.ttf', 'NotoSansKR-Regular.ttf',
                          'NotoSansSC-Regular.ttf', 'NotoSansTC-Regular.ttf'}
        if any(not (fonts / name).is_file() for name in expected_fonts) or (fonts / 'NotoSans.tar.xz').exists():
            raise AssertionError('real nested Unicode font preparation incomplete')
        if (payload / 'PortMaster/version').read_text() != 'git-' + PORTMASTER_REVISION + '\n':
            raise AssertionError('source version is not the pinned Git revision')
        all_fonts = list(fonts.glob('*.ttf'))
        for source in all_fonts:
            target = payload / 'PortMaster/resources' / source.name
            if not target.is_file() or digest(source) != digest(target):
                raise AssertionError('first-game font initialization remains incomplete')
        if (payload / 'PortMaster/resources/do_init').exists():
            raise AssertionError('first-game initialization marker was not removed')
        record['initialized_font_files'] = len(all_fonts)
        record['initialized_font_bytes'] = sum(source.stat().st_size for source in all_fonts)
        record['nested_font_files'] = len(expected_fonts)
        record['nested_font_bytes'] = sum((fonts / name).stat().st_size for name in expected_fonts)
        record['operations'].append(command([sys.executable, portable, 'zip', args.port_zip, payload / 'Roms/ports']))
        for name in ('runtime/bin/python3', 'PortMaster/gptokeyb', 'Roms/ports/ccleste/ccleste'):
            elf_aarch64(payload / name)
        if not (payload / 'Roms/ports/Celeste Classic.sh').read_text().startswith('#!/bin/bash'):
            raise AssertionError('real port launcher missing')
        manifest = file_manifest(payload)
        record.update(status='pass', real_payload_files=len(manifest),
                      real_payload_bytes=sum(row['size'] for row in manifest.values()))
        record['allocated_kib'] = int(command(['du', '-sk', payload])['stdout'].split()[0])
        return record, manifest
    finally:
        if mounted:
            command(['sudo', '-n', 'umount', mount])
            mounted = False
        if loop_device is not None:
            command(['sudo', '-n', 'losetup', '-d', loop_device])
        image.unlink()
        mount.rmdir()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--unpacker', type=Path, help='already built Linux pocket-ui binary')
    parser.add_argument('--python-zip', type=Path)
    parser.add_argument('--portmaster-tar', type=Path)
    parser.add_argument('--port-zip', type=Path)
    parser.add_argument('--work-root', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--image-mib', type=int, default=1024)
    parser.add_argument('--probe-only', action='store_true')
    args = parser.parse_args()
    for name in ('mkfs.vfat', 'mkfs.exfat', 'mkfs.ext4', 'findmnt', 'sudo'):
        if not shutil.which(name):
            raise SystemExit('required real-filesystem tool missing: ' + name)
    command(['sudo', '-n', 'true'])
    if args.image_mib < 512:
        raise SystemExit('use at least 512 MiB for genuine FAT32 and the pinned payload')
    provenance = {}
    if not args.probe_only:
        if any(getattr(args, name) is None for name in ('unpacker', 'python_zip', 'portmaster_tar', 'port_zip')):
            parser.error('full matrix needs --unpacker, --python-zip, --portmaster-tar and --port-zip')
        for path, expected, algorithm in ((args.python_zip, PYTHON_SHA256, 'sha256'),
                                           (args.portmaster_tar, PORTMASTER_SHA256, 'sha256'),
                                           (args.port_zip, CCELESTE_MD5, 'md5')):
            actual = digest(path, algorithm)
            if actual != expected:
                raise SystemExit('pinned archive verification failed: ' + str(path))
            provenance[str(path)] = {'bytes': path.stat().st_size, algorithm: actual}
        provenance['unpacker'] = {'path': str(args.unpacker), 'sha256': digest(args.unpacker)}
    args.work_root.mkdir(parents=True, exist_ok=True)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    report = {'real_loop_filesystems': True, 'probe_only': args.probe_only,
              'provenance': provenance, 'results': [],
              'scope': 'Real mounted filesystem extraction and harmless shell execution; no ARM game/Python execution.'}
    reference = None
    with tempfile.TemporaryDirectory(prefix='brick-pm-fs-', dir=args.work_root) as temp:
        root = Path(temp)
        fixtures = make_fixtures(root)
        for kind in ('vfat', 'exfat', 'ext4'):
            print('testing real ' + kind, flush=True)
            result, manifest = filesystem_run(kind, root, args, fixtures)
            if manifest is not None:
                if reference is not None and reference != manifest:
                    raise AssertionError('actual payload content differs across filesystems: ' + kind)
                reference = manifest
            report['results'].append(result)
            args.report.write_text(json.dumps(report, indent=2) + '\n')
            print(json.dumps({key: value for key, value in result.items() if key not in ('operations', 'format', 'mount')}), flush=True)
    report['status'] = 'pass'
    if reference is not None:
        report['identical_payload_manifest_sha256'] = hashlib.sha256(json.dumps(reference, sort_keys=True).encode()).hexdigest()
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    print('real filesystem matrix PASS: ' + str(args.report), flush=True)


if __name__ == '__main__':
    main()
