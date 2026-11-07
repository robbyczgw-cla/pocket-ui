#!/usr/bin/env python3
"""Copy ScummVM's declared engine/theme assets without host-specific grep/zip tools."""
from pathlib import Path
import re
import shutil
import sys

source = Path(sys.argv[1]).resolve()
destination = Path(sys.argv[2]).resolve() / 'bios' / 'scummvm'
assets = {}
for manifest in (source / 'dists/engine-data').glob('engine_data*.mk'):
    for relative in re.findall(r'^DIST_FILES_LIST\s*\+=\s*(\S+)', manifest.read_text(), re.MULTILINE):
        path = source / relative
        assets[Path('extra') / path.name] = path
for relative in re.findall(r'\bFILE\s+"([^"]+)"', (source / 'dists/scummvm.rc').read_text()):
    path = source / relative
    if relative.startswith('gui/themes/'):
        assets[Path('theme') / path.name] = path
    elif relative.startswith('dists/soundfonts/'):
        assets[Path('extra') / path.name] = path
    elif '/shaders/' in relative:
        assets[Path('extra/shaders') / path.name] = path
assets[Path('extra/vkeybd_default.zip')] = source / 'backends/vkeybd/packs/vkeybd_default.zip'
for relative, path in sorted(assets.items()):
    target = destination / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(path, target)
for required in ['extra/sky.cpt', 'theme/scummmodern.zip']:
    if not (destination / required).is_file():
        raise SystemExit('Missing ScummVM asset: ' + required)
print(f'ScummVM: {len(assets)} engine, theme and font files')
