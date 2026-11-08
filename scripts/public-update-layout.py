#!/usr/bin/env python3
"""Extract small OTA layout fixtures from public rocknix-card.zip assets, without executing/extracting ARM programs.
Usage: public-update-layout.py <asset.zip>... > tests/fixtures/updates/public-layouts.json
Each asset's published SHA256SUMS must sit beside it. Keep the original assets outside the repository.
"""
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import zipfile

layouts = []
for arg in sys.argv[1:]:
    asset = Path(arg)
    checksum = hashlib.sha256(asset.read_bytes()).hexdigest()
    sums = dict((parts[1].lstrip('*'), parts[0]) for line in asset.with_name('SHA256SUMS').read_text().splitlines() if len(parts := line.split()) == 2)
    assert sums[asset.name] == checksum, f'published checksum mismatch: {asset.name}'
    version = asset.name.removeprefix('pocket-ui-').removesuffix('-rocknix-card.zip')
    with zipfile.ZipFile(asset) as archive:
        seed = archive.read('pocket-ui-seed.tar.gz')
    entries = []
    with tarfile.open(fileobj=io.BytesIO(seed), mode='r:gz') as archive:
        for member in archive:
            name = member.name.removeprefix('./')
            if not name.startswith('pocket-ui/'):
                continue
            name = name.removeprefix('pocket-ui/')
            if member.isdir():
                entry = dict(path=name, kind='directory')
            elif member.issym():
                entry = dict(path=name, kind='link', target=member.linkname)
            else:
                assert member.isfile(), member.name
                entry = dict(path=name, kind='file', size=member.size, mode=member.mode,
                    sha256=hashlib.sha256(archive.extractfile(member).read()).hexdigest())
            entries.append(entry)
    layouts.append(dict(version=version, source=f'https://github.com/robbyczgw-cla/pocket-ui/releases/download/v{version}/{asset.name}',
        sha256=checksum, seed_sha256=hashlib.sha256(seed).hexdigest(), entries=entries))
assert layouts, 'provide public card assets'
print(json.dumps(layouts, indent=2))
