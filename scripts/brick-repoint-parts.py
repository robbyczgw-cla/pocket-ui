#!/usr/bin/env python3
"""Brick asset packages for a release: only NEW packages go to a new pre-release (brick-parts-VERSION); packages that already exist in the
previous parts release keep pointing there. The GitHub source of every package moves out of the release tag (clean asset list, 0.3.0.1 rule).
Everything else in the OTA tarball stays byte-identical (only asset-packages.json changes), the list is re-signed.

  brick-repoint-parts.py CHANNEL_DIR VERSION OLD_PARTS_TAG  [--live-names FILE]      (CHANNEL_DIR = what brick-publish.sh --public --out made)
  Writes: CHANNEL_DIR/parts-new/<new package files>  (upload these to the pre-release brick-parts-VERSION),
          CHANNEL_DIR/parts-report.json, a changed release/pocket-ui-brick-VERSION.tar.gz and updates/brick/ copy, re-signed update.json.
OLD_PARTS_TAG's package names come from `gh release view` (or --live-names FILE with one name per line, for tests). Nothing is uploaded."""
import gzip, hashlib, io, json, os, shutil, subprocess, sys, tarfile
from pathlib import Path

ch = Path(sys.argv[1]); ver = sys.argv[2]; old_tag = sys.argv[3]
names_file = sys.argv[sys.argv.index('--live-names') + 1] if '--live-names' in sys.argv else None
R = Path(__file__).resolve().parents[1]
ST = os.environ.get('STORE_TOOL', str(R / 'target/release/store-tool')); KEY = os.environ.get('POCKETUI_SIGNING_KEY', str(Path.home() / '.config/pocket-ui/store-signing.key'))
repo = os.environ.get('POCKETUI_PUBLIC_REPO', 'robbyczgw-cla/pocket-ui'); new_tag = f'brick-parts-{ver}'
if names_file:
    have = set(Path(names_file).read_text().split())
else:
    out = subprocess.check_output(['gh', 'release', 'view', old_tag, '--repo', repo, '--json', 'assets', '--jq', '.assets[].name'], text=True)
    have = set(out.split())
assert have, f'{old_tag} lists no assets'

# Packages are one file each. brick-update-pack.py writes GNU headers (needed only for the OTA tar's long cheat names), which changes the
# bytes - and so the name - of every package even though the file is the same. Rebuild each package the way 0.3.0 did (USTAR, the same
# gzip) so an unchanged file gets exactly its old package and name (kept on brick-parts-<old>); only really changed files get a new one.
import hashlib as _h, io as _io
def _ustar(src):
    with tarfile.open(src) as t:
        m = t.getmembers(); assert len(m) == 1, src; info = m[0]; data = t.extractfile(info).read()
    buf = _io.BytesIO()
    with gzip.GzipFile(filename='', fileobj=buf, mode='wb', mtime=0, compresslevel=9) as gz, tarfile.open(fileobj=gz, mode='w|', format=tarfile.USTAR_FORMAT) as out:
        out.addfile(info, _io.BytesIO(data))
    return buf.getvalue()
remap = {}   # old (GNU) package name -> (new bytes, new name)
for f in sorted((ch / 'updates/brick/assets').iterdir()):
    try: b = _ustar(f)
    except ValueError: continue          # name too long for ustar: keep the GNU package as it is
    remap[f.name] = (b, 'brick-asset-' + _h.sha256(b).hexdigest() + '.tar.gz')
for old, (b, nn) in remap.items():
    (ch / 'updates/brick/assets' / old).unlink()
    (ch / 'updates/brick/assets' / nn).write_bytes(b)
    if (ch / 'release' / old).exists(): (ch / 'release' / old).unlink()

tar_path = ch / 'release' / f'pocket-ui-brick-{ver}.tar.gz'
old = ch / 'before-parts-repoint.tar.gz'; shutil.copyfile(tar_path, old)   # kept next to the channel, NOT in release/
proof = {}; changed = []; moved = {'old': 0, 'new': 0}; newfiles = []
with tarfile.open(old, 'r:gz') as src, tar_path.with_suffix('.new').open('wb') as raw, \
     gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0, compresslevel=9) as gz, \
     tarfile.open(fileobj=gz, mode='w|', format=tarfile.GNU_FORMAT) as out:
    for m in src:
        data = src.extractfile(m).read() if m.isfile() else None
        if m.name == 'asset-packages.json':
            j = json.loads(data)
            for a in j['assets']:
                p = a['package']; fname = p['url'].rsplit('/', 1)[1]
                if fname in remap:
                    b, nn = remap[fname]
                    p['url'] = p['url'].rsplit('/', 1)[0] + '/' + nn; p['size'] = len(b); p['sha256'] = _h.sha256(b).hexdigest(); fname = nn
                    p['mirrors'] = [u.rsplit('/', 1)[0] + '/' + nn for u in p.get('mirrors', [])]
                sources = [p['url']] + p.get('mirrors', [])
                github = next(u for u in sources if u.startswith(f'https://github.com/{repo}/releases/download/v{ver}/'))
                website = next(u for u in sources if u.startswith('https://pocketui.xyz/updates/brick/assets/'))
                tag = old_tag if fname in have else new_tag
                moved['old' if tag == old_tag else 'new'] += 1
                if tag == new_tag: newfiles.append(fname)
                p['url'] = github.replace(f'/v{ver}/', f'/{tag}/')
                p['mirrors'] = [website]
            data = json.dumps(j, sort_keys=True, indent=2).encode(); m.size = len(data); changed.append(m.name)
        elif data is not None:
            proof[m.name] = hashlib.sha256(data).hexdigest()
        out.addfile(m, io.BytesIO(data) if data is not None else None)
tar_path.with_suffix('.new').replace(tar_path)
with tarfile.open(tar_path) as t:
    for m in t:
        if m.isfile() and m.name != 'asset-packages.json':
            assert hashlib.sha256(t.extractfile(m).read()).hexdigest() == proof.pop(m.name)
assert not proof and changed == ['asset-packages.json'], (proof, changed)
sha = hashlib.sha256(tar_path.read_bytes()).hexdigest(); size = tar_path.stat().st_size
shutil.copyfile(tar_path, ch / 'updates/brick' / tar_path.name)
lst = ch / 'updates/brick/update.json'; j = json.loads(lst.read_text()); j['app']['sha256'] = sha; j['app']['size'] = size
lst.write_text(json.dumps(j, indent=2) + '\n')
subprocess.run([ST, 'sign', KEY, str(lst)], check=True); subprocess.run([ST, 'verify', str(lst)] + ([os.environ['BRICK_PUBLISH_VERIFY_KEY']] if os.environ.get('BRICK_PUBLISH_VERIFY_KEY') else []), check=True)
# the packages that are new go to one folder for the pre-release; the release folder must not carry any package
pn = ch / 'parts-new'; shutil.rmtree(pn, ignore_errors=True); pn.mkdir()
for f in newfiles:
    src = ch / 'updates/brick/assets' / f; assert src.is_file(), f; shutil.copyfile(src, pn / f)
for f in (ch / 'release').iterdir():
    if f.name.startswith('brick-asset-'): f.unlink()
rep = {'version': ver, 'assets_total': moved['old'] + moved['new'], 'stay_in': {old_tag: moved['old']}, 'new_in': {new_tag: moved['new']},
       'new_files': newfiles, 'changed_members': changed, 'app_sha256': sha, 'app_size': size,
       'release_dir': sorted(p.name for p in (ch / 'release').iterdir())}
(ch / 'parts-report.json').write_text(json.dumps(rep, indent=2)); print(json.dumps(rep, indent=2))
