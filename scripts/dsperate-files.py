#!/usr/bin/env python3
"""Stage the verified portable core with its notices and corresponding source.

This only writes a local release folder. It never publishes or accesses keys.
"""
import hashlib
import json
from pathlib import Path
import re
import shutil
import sys

ROOT=Path(__file__).resolve().parents[1]
PIN='27b9d76fb4c900d29f9629806f316527b5da90c2'
ENGINE='99618eb956c66083618395dbae9bb5cbf0241d16'
NOTICES=['LICENSE','libretro.h','DSperate/src/core/bios/LICENSE.freebios',
         'DSperate/src/core/io/dsi_font/LICENSE-NotoSans-OFL-1.1.txt',
         'DSperate/src/core/io/dsi_font/LICENSE-WenQuanYi-MicroHei.txt']

def notice_path(root, relative):
    path=Path(relative)
    return root/str(path.parent).replace('/','_')/path.name if path.parent!=Path('.') else root/path.name

def validate_binary(binary):
    data=binary.read_bytes()
    assert data[:6]==b'\x7fELF\x02\x01' and int.from_bytes(data[18:20],'little')==183,'DSperate requires ARM64 ELF'
    versions={tuple(map(int,v.split(b'.'))) for v in re.findall(rb'GLIBC_([0-9.]+)',data)}
    assert versions and max(versions)<=(2,17),'DSperate release must run on the oldest Linux target'
    return hashlib.sha256(data).hexdigest()

def patches():
    return {name:hashlib.sha256((ROOT/'scripts/dsperate'/name).read_bytes()).hexdigest()
            for name in ['portability.patch','libretro.patch']}

def record(source,glibc):
    row=next(line.split() for line in (source/'CORES.txt').read_text().splitlines() if line.startswith('dsperate '))
    assert row[1]==PIN
    archive=source/f'sources/dsperate-{PIN[:7]}.tar.gz'
    data=dict(core='dsperate',wrapper=PIN,engine=ENGINE,sha256=row[3],glibc=glibc,
              patches=patches(),source_sha256=hashlib.sha256(archive.read_bytes()).hexdigest())
    (source/'dsperate-build.json').write_text(json.dumps(data,indent=2)+'\n')


def stage(source,dest):
    built=json.loads((source/'dsperate-build.json').read_text())
    assert built['patches']==patches(),'Core was built with different DSperate patches'
    assert built['wrapper']==PIN and built['engine']==ENGINE
    binary=source/'cores/dsperate_libretro.so'
    digest=validate_binary(binary)
    row=next(line.split() for line in (source/'CORES.txt').read_text().splitlines() if line.startswith('dsperate '))
    assert built['sha256']==digest
    assert row==['dsperate',PIN,'dsperate_libretro.so',digest],'Core differs from its build record'
    archive=source/f'sources/dsperate-{PIN[:7]}.tar.gz'
    assert archive.is_file(),'Corresponding source missing'
    assert built['source_sha256']==hashlib.sha256(archive.read_bytes()).hexdigest(),'Source archive differs from its build'
    notices=source/'licences/dsperate'
    # brick-build-cores flattens each notice's parent into a directory name.
    for relative in NOTICES:
        flat=notice_path(notices,relative)
        assert flat.is_file(),f'Missing DSperate notice: {relative}'
    dest.mkdir(parents=True,exist_ok=True)
    payload=dest/'dsperate'
    if payload.exists():shutil.rmtree(payload)
    payload.mkdir()
    shutil.copyfile(binary,dest/'dsperate_libretro.so')
    (dest/'dsperate_libretro.so').chmod(0o755)
    shutil.copytree(notices,payload/'licences',dirs_exist_ok=True)
    shutil.copyfile(archive,payload/'source.tar.gz')
    (payload/'scripts').mkdir(exist_ok=True)
    for name in ['brick-build-cores.sh','brick-core-source.py','brick-clear-cache.h','build-dsperate.sh','dsperate-files.py']:
        shutil.copyfile(ROOT/'scripts'/name,payload/'scripts'/name)
    shutil.copytree(ROOT/'scripts/dsperate',payload/'scripts/dsperate',dirs_exist_ok=True)
    description=dict(core='dsperate',wrapper=PIN,engine=ENGINE,sha256=digest,glibc='2.17',
                source_sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),
                origin=['https://github.com/beebono/DSperate','https://github.com/thwonp/dsperate-lr'],
                patches=built['patches'],licence='GPL-3.0-or-later; libretro MIT; FreeBIOS BSD-2-Clause; fonts retain their licences')
    (payload/'SOURCE.json').write_text(json.dumps(description,indent=2)+'\n')
    (payload/'BUILD.txt').write_text('The source archive includes all applied engine and wrapper changes.\n'
        'From its extracted root, configure CMake for aarch64 Linux with glibc <=2.17, PIC and the supplied clear-cache header; build dsperate_libretro with two jobs.\n'
        'The normal pinned fetch/build path is brick-build-cores.sh with BRICK_CORES=dsperate, BRICK_GLIBC=2.17 and JOBS=2.\n'
        'Use the dsperate/ patches and the accompanying source archiver. No ROM or proprietary BIOS is required to build.\n')
    files=[dest/'dsperate_libretro.so',*sorted(p for p in payload.rglob('*') if p.is_file())]
    (payload/'SHA256SUMS').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.relative_to(dest).as_posix()+'\n' for p in files if p.name!='SHA256SUMS'))
    print('Staged DSperate',digest)

def check(dest):
    payload=dest/'dsperate'
    data=json.loads((payload/'SOURCE.json').read_text())
    assert data['wrapper']==PIN and data['engine']==ENGINE
    assert data['patches']==patches(),'Packaged DSperate uses different patches'
    assert validate_binary(dest/'dsperate_libretro.so')==data['sha256']
    assert hashlib.sha256((payload/'source.tar.gz').read_bytes()).hexdigest()==data['source_sha256']
    listed=set()
    for line in (payload/'SHA256SUMS').read_text().splitlines():
        digest,name=line.split('  ',1);relative=Path(name)
        assert not relative.is_absolute() and '..' not in relative.parts
        path=dest/relative
        listed.add(relative.as_posix())
        assert path.is_file() and not path.is_symlink() and hashlib.sha256(path.read_bytes()).hexdigest()==digest,name
    required=[dest/'dsperate_libretro.so',payload/'SOURCE.json',payload/'source.tar.gz',payload/'BUILD.txt',
              *(notice_path(payload/'licences',name) for name in NOTICES),
              *(payload/'scripts'/name for name in ['brick-build-cores.sh','build-dsperate.sh','brick-core-source.py','brick-clear-cache.h','dsperate-files.py']),
              *(payload/'scripts/dsperate'/name for name in patches())]
    assert all(p.relative_to(dest).as_posix() in listed for p in required),'Package manifest omits required files'
    print('DSperate package verified')

if __name__=='__main__':
    if len(sys.argv)==4 and sys.argv[1]=='--record':record(Path(sys.argv[2]),sys.argv[3])
    elif len(sys.argv)==3 and sys.argv[1]=='--check':check(Path(sys.argv[2]))
    else:
        assert len(sys.argv)==3,'usage: dsperate-files.py BUILD_DIR DEST_CORES, or --check DEST_CORES'
        stage(Path(sys.argv[1]),Path(sys.argv[2]))
