#!/usr/bin/env python3
"""Prepare a small app tar and immutable asset packages. Never uploads or signs anything."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import tarfile

HELPERS = {'bin/' + name for name in ('brick-egl', 'curl', 'dropbearmulti', 'syncthing', 'tailscale', 'tailscaled')}

LIMIT = 80_000_000  # margin below Hobby's upload limit, also enforced by the client


def sha(path):
    with path.open('rb') as f:
        h = hashlib.sha256()
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
        return h.hexdigest()


def archive(root, files, dest, extra=None, cheats=None):
    with dest.open('wb') as raw, gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0, compresslevel=9) as gz:
        # Upstream cheat filenames can exceed ustar's 100-byte basename limit.
        # The Brick's checked extractor accepts GNU long-name headers.
        with tarfile.open(fileobj=gz, mode='w|', format=tarfile.GNU_FORMAT) as out:
            for path in files:
                info = out.gettarinfo(str(path), str(path.relative_to(root)))
                if not info.isfile() and not info.isdir():
                    raise ValueError(f'unsupported link or special file: {path}')
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ''
                info.mode = 0o755 if info.isdir() or info.mode & 0o111 else 0o644
                if info.isfile():
                    with path.open('rb') as src:
                        out.addfile(info, src)
                else:
                    out.addfile(info)
            if extra is not None:
                import io
                data = json.dumps(extra, sort_keys=True, indent=2).encode()
                info = tarfile.TarInfo('asset-packages.json')
                info.size = len(data)
                info.mode = 0o644
                out.addfile(info, io.BytesIO(data))
            if cheats is not None:
                import io
                data = json.dumps(cheats, sort_keys=True, indent=2).encode()
                info = tarfile.TarInfo('database/cheats-download.json')
                info.size = len(data)
                info.mode = 0o644
                out.addfile(info, io.BytesIO(data))
    if dest.stat().st_size > LIMIT:
        raise ValueError(f'{dest.name} exceeds the 80 MB hosting budget')


def optional_cheats(app, release, channel, optional=None):
    """Publish immutable raw data separately. The trusted app carries only its size/hash/URL."""
    descriptor = app / 'database/cheats-download.json'
    legacy = app / 'cores/cht.pack'
    if legacy.is_symlink() or descriptor.is_symlink():
        raise ValueError('unsupported cheat link')
    if legacy.is_file():
        digest = sha(legacy)
        info = {'version': digest, 'size': legacy.stat().st_size, 'sha256': digest,
                'url': f'https://pocketui.xyz/updates/brick/assets/cheats-{digest}.pack'}
        source = legacy
    elif descriptor.is_file():
        info = json.loads(descriptor.read_text())
        source = (optional or app.parents[2] / 'optional') / f"cheats-{info['sha256']}.pack"
    else:
        return None
    if not 0 < info['size'] <= LIMIT or len(info['sha256']) != 64 or any(c not in '0123456789abcdef' for c in info['sha256']):
        raise ValueError('invalid optional cheat description')
    name = f"cheats-{info['sha256']}.pack"
    if info['url'] != f'https://pocketui.xyz/updates/brick/assets/{name}':
        raise ValueError('unexpected optional cheat URL')
    if source.stat().st_size != info['size'] or sha(source) != info['sha256']:
        raise ValueError('optional cheats differ from their description')
    import shutil
    shutil.copyfile(source, release / name)
    shutil.copyfile(source, channel / 'assets' / name)
    return info


def prepare(app, release, channel, github, version, optional=None):
    if app.resolve() in release.resolve().parents or app.resolve() in channel.resolve().parents:
        raise ValueError('output must be outside the input app')
    release.mkdir(parents=True, exist_ok=True)
    (channel / 'assets').mkdir(parents=True, exist_ok=True)
    cheats = optional_cheats(app, release, channel, optional)
    assets, regular = [], []
    for path in sorted(app.rglob('*')):
        relative = path.relative_to(app)
        if relative.as_posix() in ('cores/cht.pack', 'database/cheats-download.json'):
            if path.is_symlink():
                raise ValueError('unsupported cheat link')
            continue
        if relative.parts[0] in ('cores', 'bios-data') or relative.as_posix() in HELPERS:
            if path.is_symlink():
                raise ValueError(f'unsupported asset link: {path}')
            if path.is_dir():
                continue
            if not path.is_file() or path.is_symlink():
                raise ValueError(f'unsupported asset: {path}')
            temp = release / 'asset.part'
            archive(app, [path], temp)
            digest = sha(temp)
            name = f'brick-asset-{digest}.tar.gz'
            target = release / name
            temp.replace(target)
            import shutil
            shutil.copyfile(target, channel / 'assets' / name)
            assets.append({'path': str(relative), 'size': path.stat().st_size, 'sha256': sha(path),
                           'package': {'version': version, 'url': f'{github}/{name}',
                                       'mirrors': [f'https://pocketui.xyz/updates/brick/assets/{name}'], 'size': target.stat().st_size, 'sha256': digest}})
        else:
            if relative.as_posix() == 'asset-packages.json':
                raise ValueError('input already contains asset-packages.json')
            regular.append(path)
    if not assets:
        raise ValueError('card has no cores or data assets')
    name = f'pocket-ui-brick-{version}.tar.gz'
    archive(app, regular, release / name, {'format': 1, 'assets': assets}, cheats)
    import shutil
    shutil.copyfile(release / name, channel / name)
    print(json.dumps({'app_bytes': (release / name).stat().st_size, 'asset_files': len(assets),
                      'asset_bytes': sum(a['package']['size'] for a in assets),
                      'largest_asset_bytes': max(a['package']['size'] for a in assets),
                      'unpacked_asset_bytes': sum(a['size'] for a in assets)}, indent=2))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('app', type=Path)
    p.add_argument('release', type=Path)
    p.add_argument('channel', type=Path)
    p.add_argument('github')
    p.add_argument('version')
    p.add_argument('--optional', type=Path)
    a = p.parse_args()
    prepare(a.app, a.release, a.channel, a.github.rstrip('/'), a.version, a.optional)
