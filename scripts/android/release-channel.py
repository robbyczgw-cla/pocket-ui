#!/usr/bin/env python3
"""Prepare/check an Android channel locally. Nothing is uploaded or deployed.

The existing store-tool owns signing and checks the same compiled PUBLIC_KEY as the app.
Release-check verifies APK identity/version/certificate before invoking this tool.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
from urllib.parse import urlsplit


def digest(path):
    sha = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            sha.update(chunk)
    return sha.hexdigest()


def run(tool, *args):
    result = subprocess.run([tool, *map(str, args)], capture_output=True)
    if result.returncode:
        raise ValueError('Signing/verification failed: the key must match the compiled PUBLIC_KEY. No channel was prepared.')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--apk', type=Path, required=True)
    p.add_argument('--version', required=True, help='exact versionName: X.Y.Z-android.N')
    p.add_argument('--url', help='immutable HTTPS APK URL (file:// for offline channels)')
    p.add_argument('--out', type=Path, help='prepared root, containing updates/android/')
    p.add_argument('--key', type=Path, default=Path.home() / '.config/pocket-ui/store-signing.key')
    p.add_argument('--notes', default='')
    p.add_argument('--check', type=Path, help='verify an existing manifest against APK and PUBLIC_KEY')
    p.add_argument('--allow-foreign-key', action='store_true', help='offline tests only: verify against --key instead of PUBLIC_KEY')
    a = p.parse_args()
    tool = os.environ.get('STORE_TOOL') or shutil.which('store-tool')
    if not tool:
        raise ValueError('Set STORE_TOOL to a store-tool built from this checkout.')
    if not re.fullmatch(r'\d+\.\d+\.\d+-android\.[1-9]\d?', a.version):
        raise ValueError('Android version must be X.Y.Z-android.N, with N from 1 to 99.')
    if not a.apk.is_file() or not 0 < a.apk.stat().st_size <= 100000000:
        raise ValueError('APK is missing, empty or exceeds the download limit.')
    if a.allow_foreign_key and not a.key.is_file():
        raise ValueError('Test signing key is missing.')
    if (not a.check or a.allow_foreign_key) and (not a.key.is_file() or a.key.stat().st_size != 32):
        raise ValueError('Signing key must be a 32-byte store-tool key; its contents are never displayed.')
    sha, size = digest(a.apk), a.apk.stat().st_size

    def verify(path):
        run(tool, 'verify', path, *([a.key] if a.allow_foreign_key else []))

    def valid_url(url):
        parsed = urlsplit(url)
        if parsed.username or parsed.password or parsed.fragment:
            raise ValueError('APK URL must not contain credentials or a fragment.')
        if not ((parsed.scheme == 'https' and parsed.hostname) or (parsed.scheme == 'file' and parsed.path.startswith('/'))):
            raise ValueError('APK URL must use HTTPS (file:// for offline channels).')
        if '/latest/' in parsed.path or parsed.path.endswith('/latest'):
            raise ValueError('APK URL must identify an immutable release, not latest.')

    if a.check:
        verify(a.check)
        data = json.loads(a.check.read_bytes())
        app = data.get('app') or {}
        if data.get('format') != 1 or app.get('version') != a.version or app.get('sha256') != sha or app.get('size') != size:
            raise ValueError('Signed channel does not match this APK/version. Nothing was changed.')
        valid_url(app.get('url', ''))
        print('Android channel: signature, version, APK size and SHA-256 verified.')
        return
    if not a.out or not a.url:
        raise ValueError('--out and --url are required when preparing a channel.')
    valid_url(a.url)
    if len(a.notes.encode()) > 4096:
        raise ValueError('Release notes exceed the Android channel limit.')
    # First sign/verify a probe in scratch. An untrusted key never creates an output tree.
    with tempfile.TemporaryDirectory(prefix='android-channel-') as scratch:
        probe = Path(scratch) / 'probe'
        probe.write_text('pocket-ui Android channel trust check\n')
        run(tool, 'sign', a.key, probe)
        verify(probe)
        channel = a.out / 'updates/android'
        saved = channel / a.version / 'pocket-ui-android.apk'
        if saved.exists() and (saved.stat().st_size != size or digest(saved) != sha):
            raise ValueError('An APK for this version already exists with different bytes; increase the build number.')
        manifest = Path(scratch) / 'update.json'
        manifest.write_text(json.dumps({'format': 1, 'app': {'version': a.version, 'url': a.url,
                            'sha256': sha, 'size': size, 'notes': a.notes}}, indent=2) + '\n')
        run(tool, 'sign', a.key, manifest)
        verify(manifest)
        saved.parent.mkdir(parents=True, exist_ok=True)
        if not saved.exists():
            shutil.copyfile(a.apk, saved)
        # Only local preparation. Publish the complete directory atomically together, after approval.
        for name in ('update.json', 'update.json.sig'):
            temporary = channel / (name + '.new')
            shutil.copyfile(Path(scratch) / name, temporary)
            temporary.replace(channel / name)
        print('Prepared Android channel: ' + str(channel / 'update.json'))
        print('Nothing was uploaded or deployed.')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, TypeError) as e:
        print('android-channel: ' + str(e), file=sys.stderr)
        sys.exit(1)
