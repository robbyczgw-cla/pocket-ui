#!/usr/bin/env python3
"""Expected image inventory from the verified DSperate payload manifest."""
from pathlib import Path
import sys

core, version = Path(sys.argv[1]), sys.argv[2]
root = Path(f'./pocket-ui/releases/{version}/cores')
files = [Path(line.split('  ', 1)[1]) for line in (core/'dsperate/SHA256SUMS').read_text().splitlines()]
files.append(Path('dsperate/SHA256SUMS'))
dirs = {Path('.')}
for file in files:
    assert not file.is_absolute() and '..' not in file.parts
    dirs.update(file.parents)
for directory in sorted(dirs):
    print(f'755 0:0 d ./{root/directory}')
for file in sorted(files):
    mode = '755' if file == Path('dsperate_libretro.so') else '644'
    print(f'{mode} 0:0 f ./{root/file}')
