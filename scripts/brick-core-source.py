#!/usr/bin/env python3
"""Archive tracked corresponding source, including build-fetched dependency repos.

Run after a successful build; git's index excludes object files. CMake FetchContent
and ScummVM dependencies are nested repositories rather than git submodules.
"""
import gzip
import os
from pathlib import Path
import subprocess
import sys
import tarfile

root = Path(sys.argv[1]).resolve()
out = Path(sys.argv[2]).resolve()
repos = {root}
for directory, dirs, files in os.walk(root):
    if '.git' in dirs or '.git' in files:
        repos.add(Path(directory))
    dirs[:] = [d for d in dirs if d not in {'.git', 'CMakeFiles', 'node_modules'}]
names = set()
for repo in sorted(repos):
    tracked = subprocess.check_output(['git', '-C', str(repo), 'ls-files', '-z', '--recurse-submodules'])
    for name in tracked.split(b'\0'):
        if name:
            path = repo / os.fsdecode(name)
            if path.is_file() or path.is_symlink():
                names.add(path.relative_to(root).as_posix())
with out.open('wb') as stream, gzip.GzipFile(filename='', fileobj=stream, mode='wb', mtime=0) as gz, tarfile.open(fileobj=gz, mode='w') as archive:
    for name in sorted(names):
        path = root / name
        info = archive.gettarinfo(str(path), arcname=name)
        info.mtime = 1577836800
        info.uid = info.gid = 0
        info.uname = info.gname = ''
        if info.isreg():
            with path.open('rb') as source:
                archive.addfile(info, source)
        else:
            archive.addfile(info)
