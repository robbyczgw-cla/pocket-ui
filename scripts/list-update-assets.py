#!/usr/bin/env python3
"""List sources embedded in an already SHA-256 verified app archive for release preflight."""
import json
import sys
import tarfile

# Stream: even a legacy full card archive is never held in memory or extracted to disk.
with tarfile.open(sys.argv[1], 'r|*') as archive:
    for member in archive:
        if member.name.lstrip('./') != 'asset-packages.json':
            continue
        if not member.isfile() or member.size > 1 << 20:
            raise ValueError('invalid asset list')
        data = json.load(archive.extractfile(member))
        if data['format'] != 1:
            raise ValueError('unsupported asset list')
        for asset in data['assets']:
            rel = asset['package']
            for url in [rel['url']] + rel.get('mirrors', []):
                if not url.startswith(('https://', 'file://')) or any(c.isspace() for c in url):
                    raise ValueError('unsafe asset URL')
                print('asset', rel['version'], rel['size'], rel['sha256'], url, sep='\t')
        break
