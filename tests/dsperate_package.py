"""Exercise package integrity failures without downloading or running a ROM."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import subprocess
import xml.etree.ElementTree as ET

root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('ds_files',root/'scripts/dsperate-files.py')
package=importlib.util.module_from_spec(spec);spec.loader.exec_module(package)

with tempfile.TemporaryDirectory(prefix='pocket-ds-package-') as tmp:
    build,dest=Path(tmp)/'build',Path(tmp)/'staged'
    (build/'cores').mkdir(parents=True);(build/'sources').mkdir()
    binary=bytearray(64);binary[:6]=b'\x7fELF\x02\x01';binary[18:20]=(183).to_bytes(2,'little')
    binary.extend(b'GLIBC_2.17\0')
    (build/'cores/dsperate_libretro.so').write_bytes(binary)
    (build/'CORES.txt').write_text(f'dsperate {package.PIN} dsperate_libretro.so {hashlib.sha256(binary).hexdigest()}\n')
    source=build/f'sources/dsperate-{package.PIN[:7]}.tar.gz';source.write_bytes(b'corresponding source fixture')
    for notice in package.NOTICES:
        path=package.notice_path(build/'licences/dsperate',notice);path.parent.mkdir(parents=True,exist_ok=True);path.write_text('notice fixture')
    package.record(build,'2.17');package.stage(build,dest);package.check(dest)
    for path in [dest/'dsperate_libretro.so',dest/'dsperate/source.tar.gz',dest/'dsperate/licences/LICENSE']:
        original=path.read_bytes();path.write_bytes(b'changed')
        try:package.check(dest)
        except (AssertionError,FileNotFoundError):pass
        else:raise AssertionError(f'accepted changed {path.name}')
        path.write_bytes(original)
    manifest=dest/'dsperate/SHA256SUMS';original=manifest.read_text()
    manifest.write_text(''.join(line+'\n' for line in original.splitlines() if not line.endswith('dsperate/licences/LICENSE')))
    try:package.check(dest)
    except AssertionError:pass
    else:raise AssertionError('accepted a manifest without the licence')
    source.unlink()
    try:package.stage(build,Path(tmp)/'no-source')
    except (AssertionError,FileNotFoundError):pass
    else:raise AssertionError('accepted a build without source')

    # The port sees the card's DS folder through its normal /roms mapping.
    card=Path(tmp)/'card';(card/'nds').mkdir(parents=True)
    run=Path(tmp)/'run';run.mkdir();config=Path(tmp)/'config'
    source=Path(tmp)/'systems.xml'
    source.write_text('<systemList><system><name>nds</name><path>/roms/nds</path><extension>.nds</extension><command>host command</command></system></systemList>')
    subprocess.run(['python3','-I',str(root/'deploy/darkos/systems.py'),str(source),str(card),str(dest),str(config),str(run)],check=True)
    choices=ET.parse(config/'es_systems.cfg').findall("system/emulators/emulator[@name='pocket-core']/cores/core")
    assert [c.text for c in choices]==['dsperate']
    assert 'host command' in (run/'systems.json').read_text()
