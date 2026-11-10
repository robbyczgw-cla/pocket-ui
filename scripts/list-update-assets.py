#!/usr/bin/env python3
"""List sources embedded in an already SHA-256 verified app archive for release preflight."""
import json
import sys
import tarfile

# Stream: even a legacy full card archive is never held in memory or extracted to disk.
with tarfile.open(sys.argv[1], 'r|*') as archive:
    for member in archive:
        name = member.name.lstrip('./')
        if name not in ('asset-packages.json', 'database/cheats-download.json'):
            continue
        if not member.isfile() or member.size > 1 << 20:
            raise ValueError('invalid asset list')
        data = json.load(archive.extractfile(member))
        if name == 'database/cheats-download.json':
            url = data['url']
            if not url.startswith(('https://', 'file://')) or any(c.isspace() for c in url):
                raise ValueError('unsafe optional cheat URL')
            print('optional-cheats', data['version'], data['size'], data['sha256'], url, sep='\t')
            continue
        if data['format'] != 1:
            raise ValueError('unsupported asset list')
        for asset in data['assets']:
            rel = asset['package']
            for url in dict.fromkeys(rel.get('preferred_sources', []) + [rel['url']] + rel.get('mirrors', [])):
                if not url.startswith(('https://', 'file://')) or any(c.isspace() for c in url):
                    raise ValueError('unsafe asset URL')
                print('asset', rel['version'], rel['size'], rel['sha256'], url, sep='\t')
