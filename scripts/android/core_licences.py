#!/usr/bin/env python3
"""Licence and source evidence of the Android emulator cores: the check, the written offer and the in-app list.

    core_licences.py check   --cache DIR [--sources DIR] [--downloads DIR]
    core_licences.py offer   --cache DIR --version V --out FILE [--release-url URL] [--downloads DIR]
    core_licences.py list    --cache DIR --out FILE [--downloads DIR]
    core_licences.py patches --cache DIR --out DIR
    core_licences.py channel FILE

DIR (--cache) is the core build's output (android/build/cores or $ANDROID_CORE_CACHE): manifest.json, licences/ and
corresponding-source/. The check is what scripts/android/build-apk.sh runs before it packs anything, as the Linux images do
for ROCKNIX's packages: a core without its licence, its pinned source revision, its source archive or its patches stops the
build. It fails (exit 1) on the first missing piece and prints every problem. Test: scripts/android/test-core-licences.py.
"""
import gzip
import hashlib
import io
import json
import re
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HEX40 = re.compile(r'^[0-9a-f]{40}$')
HEX64 = re.compile(r'^[0-9a-f]{64}$')
ARCHIVE_NAME = 'pocket-ui-android-core-sources.tar'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(cache):
    return json.loads((Path(cache) / 'manifest.json').read_text())


def lock():
    return json.loads((ROOT / 'android' / 'cores.lock.json').read_text())


def patch_name(patch):
    return Path(patch['path']).name


def check(manifest, lock_data, cache, sources=None, downloads=None, root=ROOT):
    """Every problem found (an empty list: every core has its licence, source revision, source archive and patches)."""
    cache, problems = Path(cache), []
    cores = manifest.get('cores', [])
    locked = {c['id']: c for c in lock_data['cores']}
    built = {c['id'] for c in cores}
    if built != set(locked):
        problems.append('cores built %s differ from android/cores.lock.json %s' % (sorted(built), sorted(locked)))
    for c in cores:
        cid = c.get('id', '?')
        say = lambda why: problems.append('%s: %s' % (cid, why))
        if not isinstance(c.get('licence'), str) or not c['licence'].strip():
            say('no licence named')
        if not str(c.get('source', '')).startswith('https://'):
            say('no https source address')
        if not HEX40.match(str(c.get('revision', ''))):
            say('the source is not pinned to a full commit')
        evidence = c.get('evidence') or []
        if not evidence or not all(HEX64.match(str(e.get('sha256', ''))) and e.get('path') for e in evidence):
            say('no licence evidence (file and hash) from the pinned source')
        notices = c.get('notices') or []
        if not notices:
            say('no licence texts were collected')
        for n in notices:
            f = cache / 'licences' / n.get('asset', '?')
            if not f.is_file():
                say('licence text %s is missing' % n.get('asset'))
            elif sha(f.read_bytes()) != n.get('sha256'):
                say('licence text %s changed after the build' % n.get('asset'))
        want = locked.get(cid, {}).get('patches', [])
        if [(p['path'], p['sha256']) for p in c.get('patches', [])] != [(p['path'], p['sha256']) for p in want]:
            say('the patches built in differ from the lock')
        for p in want:
            f = root / p['path']
            if not f.is_file():
                say('patch %s is missing' % p['path'])
            elif sha(f.read_bytes()) != p['sha256']:
                say('patch %s differs from the recorded one' % p['path'])
        if not c.get('corresponding_source') or not HEX64.match(str(c.get('source_sha256', ''))):
            say('no corresponding-source archive recorded (build with ANDROID_RELEASE_SOURCES=1)')
        elif sources is not None:
            f = Path(sources) / c['corresponding_source']
            if not f.is_file() or sha(f.read_bytes()) != c['source_sha256']:
                say('source archive %s is missing or changed' % c['corresponding_source'])
    if downloads is not None:
        problems += check_downloads(cores, Path(downloads), cache)
    return problems


def check_downloads(cores, downloads, cache):
    """The packages people load later carry the same evidence as the APK: the licence texts and the patches next to the library."""
    problems = []
    catalog = downloads / 'catalog.json'
    if not catalog.is_file():
        return ['%s is missing' % catalog]
    by_id = {c['id']: c for c in cores}
    for pkg in json.loads(catalog.read_text()).get('packages', []):
        c = by_id.get(pkg.get('core'))
        say = lambda why, pkg=pkg: problems.append('download %s: %s' % (pkg.get('core'), why))
        if c is None:
            say('not a built core')
            continue
        f = downloads / pkg.get('url', '?')
        if not f.is_file() or sha(f.read_bytes()) != pkg.get('sha256'):
            say('the package file is missing or does not match the catalog')
            continue
        with tarfile.open(fileobj=io.BytesIO(gzip.decompress(f.read_bytes()))) as tar:
            names = set(tar.getnames())
        need = ['lib%s_libretro.so' % c['id'], 'core.json']
        need += ['licences/' + n['asset'] for n in c.get('notices', [])]
        need += ['licences/PATCH--' + patch_name(p) for p in c.get('patches', [])]
        gone = [n for n in need if n not in names]
        if gone:
            say('the package lacks %s' % gone[:4])
        if pkg.get('license') != c.get('licence') or pkg.get('corresponding_source') != c.get('corresponding_source'):
            say('the catalog entry names another licence or source archive than the build')
    return problems


def delivery(downloads):
    """Core ids that are optional downloads (from a core-downloads catalog), else an empty set."""
    if downloads is None or not (Path(downloads) / 'catalog.json').is_file():
        return set()
    return {p['core'] for p in json.loads((Path(downloads) / 'catalog.json').read_text()).get('packages', [])}


def repo_name(url):
    return re.sub(r'^https://(www\.)?', '', url).removesuffix('.git')


def offer(manifest, version, release_url, optional=frozenset()):
    """SOURCE-OFFER.txt for the APK (the Linux images' deploy/image/SOURCE-OFFER.txt.in, for the cores)."""
    where = release_url or 'the pocket-ui release page for version %s' % version
    issues = (release_url.split('/releases')[0] + '/issues') if release_url and '/releases' in release_url else 'the pocket-ui issue tracker'
    template = (ROOT / 'android' / 'SOURCE-OFFER.txt.in').read_text()
    lines = []
    for c in manifest['cores']:
        how = 'optional download' if c['id'] in optional else 'in the APK'
        lines.append('%s (%s), %s' % (c['id'], ', '.join(c.get('systems', [])), how))
        lines.append('    licence:  %s' % c['licence'])
        lines.append('    upstream: %s, commit %s' % (repo_name(c['source']), c['revision']))
        lines.append('    source:   %s' % c['corresponding_source'])
        lines.append('              sha256 %s' % c['source_sha256'])
        for p in c.get('patches', []):
            lines.append('    patch:    %s (sha256 %s)' % (patch_name(p), p['sha256']))
        if not c.get('patches'):
            lines.append('    patches:  none')
        lines.append('    licence texts: licences/%s--* in this app' % c['id'])
        lines.append('')
    return (template.replace('@VERSION@', version).replace('@RELEASE_URL@', where).replace('@ISSUES_URL@', issues)
            .replace('@ARCHIVE@', ARCHIVE_NAME).replace('@CORES@', '\n'.join(lines).rstrip()) + '\n')


def listing(manifest, optional=frozenset()):
    """The tab-separated list the app's Licences page reads: id, systems, licence, upstream, commit, delivery, patches."""
    rows = []
    for c in manifest['cores']:
        rows.append('\t'.join([c['id'], ','.join(c.get('systems', [])), c['licence'], repo_name(c['source']), c['revision'],
                               'download' if c['id'] in optional else 'apk', ','.join(patch_name(p) for p in c.get('patches', []))]))
    return '\n'.join(rows) + '\n'


def channel(path):
    """core-channel.url: comments, then at most one https address (or nothing: the systems read "Coming soon")."""
    urls = [l.strip() for l in Path(path).read_text().splitlines() if l.strip() and not l.strip().startswith('#')]
    if len(urls) > 1:
        return ['core-channel.url has %d addresses, expected at most one' % len(urls)]
    if urls and not re.fullmatch(r'https://[^\s]+/catalog\.json', urls[0]):
        return ['core-channel.url must be an https address ending in /catalog.json, got %r' % urls[0]]
    return []


def arg(args, name, default=None):
    return args[args.index(name) + 1] if name in args else default


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, args = argv[0], argv[1:]
    if cmd == 'channel':
        problems = channel(args[0])
    else:
        cache = arg(args, '--cache')
        if not cache:
            print(__doc__)
            return 2
        manifest = load(cache)
        downloads = arg(args, '--downloads')
        if cmd == 'check':
            problems = check(manifest, lock(), cache, arg(args, '--sources'), downloads)
        elif cmd == 'offer':
            problems = check(manifest, lock(), cache)
            if not problems:
                Path(arg(args, '--out')).write_text(offer(manifest, arg(args, '--version'), arg(args, '--release-url', ''), delivery(downloads)))
        elif cmd == 'list':
            problems = check(manifest, lock(), cache)
            if not problems:
                Path(arg(args, '--out')).write_text(listing(manifest, delivery(downloads)))
        elif cmd == 'patches':
            problems = check(manifest, lock(), cache)
            if not problems:
                out = Path(arg(args, '--out'))
                out.mkdir(parents=True, exist_ok=True)
                for c in manifest['cores']:
                    for p in c.get('patches', []):
                        (out / patch_name(p)).write_bytes((ROOT / p['path']).read_bytes())
        else:
            print(__doc__)
            return 2
    for p in problems:
        print('FAIL: ' + p)
    if not problems:
        print('ok: core licences (%s)' % cmd)
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
