#!/usr/bin/env python3
"""Hash-bound offline image checks and release manifests. Standard library only."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

RELEASE = 'image-release.receipt.json'
MANIFESTS = ('SHA256SUMS-images', 'SHA256SUMS-images-more', 'SHA256SUMS-images-sm8550')
HEX = re.compile(r'[0-9a-f]{64}')
ASSET = re.compile(r'pocket-ui-[A-Za-z0-9.+_-]+\.(?:img\.xz|txt)')
HEADER = re.compile(r'^== (\S+\.img(?:\.xz)?) \(version ([^,)]+)(?:, model [^)]*)?\)$')


class Refused(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise Refused(message)


def minimum():
    pins = Path(__file__).with_name('pins.sh').read_text()
    return int(re.search(r'^P2_MIN_HEADROOM_BLOCKS=(\d+)$', pins, re.M)[1])


def regular(path):
    require(path.is_file() and not path.is_symlink(), f'not a regular file: {path.name}')


def identity(path, cache=None):
    if cache is not None and path in cache:
        return cache[path]
    regular(path)
    h = hashlib.sha256()
    with path.open('rb') as f:
        before = os.fstat(f.fileno())
        for chunk in iter(lambda: f.read(4*1024*1024), b''):
            h.update(chunk)
        after = os.fstat(f.fileno())
    fields = ('st_dev','st_ino','st_size','st_mtime_ns','st_ctime_ns')
    require(all(getattr(before,k) == getattr(after,k) for k in fields), f'changed while hashing: {path.name}')
    result = {'file':path.name, 'sha256':h.hexdigest(), 'bytes':after.st_size}
    if cache is not None:
        cache[path] = result
    return result


def load(path):
    regular(path)
    require(path.stat().st_size <= 1024*1024, f'oversized receipt: {path.name}')
    value = json.loads(path.read_text())
    require(isinstance(value,dict), f'invalid receipt: {path.name}')
    return value


def save(path, value):
    value = dict(value, recorded_at=datetime.now(timezone.utc).isoformat())
    name = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, prefix=path.name+'.', delete=False) as f:
            name = f.name
            json.dump(value,f,indent=2,sort_keys=True)
            f.write('\n')
            f.flush()
            os.fsync(f.fileno())
        os.replace(name,path)
        name = None
    finally:
        if name is not None:
            os.unlink(name)


def notes(image):
    return image.with_name(image.name.removesuffix('.img.xz')+'.txt')


def sidecar(image):
    return image.with_name(image.name+'.receipt.json')


def assembled(image, version, cache=None):
    require(re.fullmatch(r'[A-Za-z0-9.+_-]+',version), 'invalid version')
    require(image.name.startswith('pocket-ui-'+version+'-') and image.name.endswith('.img.xz')
            and ASSET.fullmatch(image.name), 'image name does not match the version')
    return {'format':1, 'kind':'image', 'state':'assembled', 'version':version,
            'artifacts':[identity(image,cache),identity(notes(image),cache)]}


def sums(path):
    regular(path)
    result = {}
    for line in path.read_text().splitlines():
        if not line:
            continue
        match = re.fullmatch(r'([0-9a-f]{64}) [ *](.+)',line)
        require(match is not None and ASSET.fullmatch(match[2]), f'invalid checksum entry in {path.name}')
        require(match[2] not in result, f'duplicate checksum entry: {match[2]}')
        result[match[2]] = match[1]
    require(result, f'empty checksum file: {path.name}')
    return result


def evidence(raw, image, version, artifacts):
    require(len(raw) <= 16*1024*1024, 'check log is too large')
    lines = raw.decode('utf-8').splitlines()
    headers = [(i,HEADER.fullmatch(line)) for i,line in enumerate(lines) if HEADER.fullmatch(line)]
    matches = [(i,m) for i,m in headers if m[1] == image.name]
    require(matches, 'no check block for this image')
    start, header = matches[-1]
    require(header[2] == version, 'check log version does not match')
    end = next((i for i,_ in headers if i > start),len(lines))
    block = lines[start:end]
    bound = {}
    for line in lines[:start]:
        m = re.fullmatch(r'([0-9a-f]{64}) [ *](\S+)',line)
        if m:
            bound[m[2]] = m[1]
    require(all(bound.get(a['file']) == a['sha256'] for a in artifacts),
            'check log hashes do not identify these image and notes bytes')
    require(not any(re.match(r'^(FAIL(?:\s|$)|IMAGE NOT OK:|rc=(?!0$))',s) for s in block),
            'latest check contains a failure')
    verdicts = [s for s in block if s.startswith(('IMAGE OK:','IMAGE NOT OK:'))]
    require(verdicts == ['IMAGE OK: '+image.name], 'missing or ambiguous IMAGE OK')
    require('all checks passed (0 warnings)' in block, 'offline checks did not pass without warnings')
    require(any(re.fullmatch(r'ok +STORAGE survives .*\(grow-check\.sh\)',s) for s in block),
            'missing successful first-boot grow check')
    free = [int(m[1]) for s in block if (m := re.fullmatch(
        r'STORAGE before the first boot: .*?, (\d+) of \d+ blocks \(4 KiB\) free',s))]
    limits = [int(m[1]) for s in block if (m := re.fullmatch(
        r'ok +STORAGE keeps at least (\d+) blocks \(\d+ KiB\) free for the programs to grow',s))]
    require(len(free) == 1 and len(limits) == 1 and limits[0] >= minimum() and free[0] >= limits[0],
            'STORAGE headroom does not meet the landed image budget')
    warnings = [s for s in block if s.startswith('WARN ')]
    require(all(re.fullmatch(r'WARN pocket-vdev-base:[^ ]+ missing \(scripts/vdev.sh image, '
                            r'needs an aarch64-capable Docker host\): --version not checked',s) for s in warnings),
            'unexpected check warning')
    return {'scope':'offline-image-check', 'log_sha256':hashlib.sha256(raw).hexdigest(),
            'grow_check':True, 'storage':{'free_blocks':free[0],'minimum_blocks':limits[0]},
            'binary_version_checked':not warnings,
            'hardware_boot':'not_attested', 'game_smoke':'not_attested'}


def checked(image, version, before, raw, origin):
    after = assembled(image,version)
    require(after['artifacts'] == before['artifacts'], 'image or notes changed during the check')
    proof = evidence(raw,image,version,after['artifacts'])
    save(sidecar(image),dict(after,state='checked',check=proof,evidence_origin=origin))
    print('IMAGE CHECKED: '+image.name)


def import_log(image, version, log, checksum_file):
    before = assembled(image,version)
    save(sidecar(image),before)
    entries = sums(checksum_file)
    require(all(entries.get(f['file']) == f['sha256'] for f in before['artifacts']), 'SHA256SUMS mismatch')
    checked(image,version,before,log.read_bytes(),'legacy-log-import')


def verify_image(image, version, cache):
    expected = assembled(image,version,cache)
    receipt = load(sidecar(image))
    require(all(receipt.get(k) == expected[k] for k in ('format','kind','version','artifacts'))
            and receipt.get('state') == 'checked', f'no matching checked receipt: {image.name}')
    proof = receipt.get('check')
    require(isinstance(proof,dict) and proof.get('scope') == 'offline-image-check'
            and HEX.fullmatch(str(proof.get('log_sha256',''))) and proof.get('grow_check') is True,
            f'incomplete check evidence: {image.name}')
    storage = proof.get('storage',{})
    require(isinstance(storage,dict) and type(storage.get('free_blocks')) is int
            and type(storage.get('minimum_blocks')) is int
            and storage['minimum_blocks'] >= minimum() and storage['free_blocks'] >= storage['minimum_blocks'],
            f'STORAGE budget evidence is missing or insufficient: {image.name}')
    require(type(proof.get('binary_version_checked')) is bool, 'missing binary-check capability')
    return expected['artifacts'],identity(sidecar(image),cache)


def release_data(directory, tag):
    require(tag.startswith('v') and len(tag) > 1,'tag must be v<version>')
    version = tag[1:]
    cache, entries, manifests = {}, {}, []
    for name in MANIFESTS:
        path = directory/name
        if not path.exists():
            continue
        for asset,digest in sums(path).items():
            require(asset not in entries, f'asset listed more than once: {asset}')
            entries[asset] = digest
        manifests.append(identity(path,cache))
    require(manifests, 'no image release checksum lists')
    images = sorted(name for name in entries if name.endswith('.img.xz'))
    require(images, 'no images in release checksum lists')
    require(set(images) == {p.name for p in directory.glob('*.img.xz')},
            'release checksum lists omit an assembled image or name a missing image')
    artifacts, receipts = [], []
    for name in images:
        files, receipt = verify_image(directory/name,version,cache)
        for file in files:
            require(entries.get(file['file']) == file['sha256'], f'release checksum mismatch: {file["file"]}')
        artifacts.extend(files)
        receipts.append(receipt)
    require(set(entries) == {f['file'] for f in artifacts}, 'unpaired or unexpected release asset')
    return {'format':1, 'kind':'image-release', 'state':'releasable', 'tag':tag,
            'artifacts':artifacts, 'receipts':receipts, 'manifests':manifests}


def verify_release(directory, tag):
    recorded = load(directory/RELEASE)
    actual = release_data(directory,tag)
    require(all(recorded.get(k) == v for k,v in actual.items()),
            'release receipt does not match the current files and checked receipts')
    return actual


def run_check(args, command):
    require(command,'run-check needs -- COMMAND [ARG ...]')
    before = assembled(args.image,args.version)
    save(sidecar(args.image),before)
    log = args.image.with_name(args.image.name+'.check.log')
    with log.open('wb') as out:
        for item in before['artifacts']:
            out.write(f'{item["sha256"]}  {item["file"]}\n'.encode())
        out.flush()
        env = dict(os.environ,POCKET_IMAGE_RECEIPT_CHILD='1')
        with subprocess.Popen(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env) as process:
            for line in process.stdout:
                out.write(line)
                out.flush()
                sys.stdout.buffer.write(line)
                sys.stdout.buffer.flush()
            rc = process.wait()
    if rc:
        return rc
    checked(args.image,args.version,before,log.read_bytes(),'fresh-check')
    return 0


def main(argv):
    command = []
    if argv and argv[0] == 'run-check' and '--' in argv:
        split = argv.index('--')
        command, argv = argv[split+1:],argv[:split]
    parser = argparse.ArgumentParser(description=__doc__)
    subs = parser.add_subparsers(dest='action',required=True)
    for action in ('assemble','import-log','run-check'):
        sub = subs.add_parser(action)
        sub.add_argument('image',type=lambda s:Path(s).absolute())
        sub.add_argument('--version',required=True)
        if action == 'import-log':
            sub.add_argument('--log',type=Path,required=True)
            sub.add_argument('--sums',type=Path,required=True)
    for action in ('release','verify','import-release'):
        sub = subs.add_parser(action)
        sub.add_argument('directory',type=lambda s:Path(s).absolute())
        sub.add_argument('--tag',required=True)
        if action == 'import-release':
            sub.add_argument('--log',type=Path,action='append',required=True)
    upload = subs.add_parser('upload')
    upload.add_argument('tag')
    upload.add_argument('directory',type=lambda s:Path(s).absolute())
    upload.add_argument('--repo',required=True)
    upload.add_argument('--dry-run',action='store_true')
    args = parser.parse_args(argv)
    if args.action == 'assemble':
        save(sidecar(args.image),assembled(args.image,args.version))
        print('IMAGE ASSEMBLED: '+args.image.name)
    elif args.action == 'import-log':
        import_log(args.image,args.version,args.log,args.sums)
    elif args.action == 'run-check':
        return run_check(args,command)
    elif args.action in ('release','import-release'):
        if args.action == 'import-release':
            require(args.tag.startswith('v'), 'tag must be v<version>')
            images = sorted(args.directory.glob('*.img.xz'))
            require(images, 'no images to import')
            logs = {}
            for log in args.log:
                regular(log)
                require(log.stat().st_size <= 16*1024*1024,'check log is too large')
                logs[log] = {m[1] for line in log.read_text().splitlines() if (m := HEADER.fullmatch(line))}
            for image in images:
                matches = [log for log,names in logs.items() if image.name in names]
                require(len(matches) == 1,f'need exactly one supplied log for {image.name}')
                import_log(image,args.tag[1:],matches[0],args.directory/'SHA256SUMS')
        save(args.directory/RELEASE,release_data(args.directory,args.tag))
        print('IMAGE RELEASE RELEASABLE: '+args.tag)
    elif args.action == 'verify':
        verify_release(args.directory,args.tag)
        print('IMAGE RELEASE VERIFIED: '+args.tag)
    else:
        require(re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+',args.repo),'invalid repository')
        value = verify_release(args.directory,args.tag)
        assets = [str(args.directory/f['file']) for f in value['artifacts']+value['manifests']]
        cmd = ['gh','release','upload',args.tag,'--repo',args.repo,*assets]
        if args.dry_run:
            print(json.dumps({'tag':args.tag,'repo':args.repo,'assets':assets},indent=2))
        else:
            return subprocess.run(cmd,check=False).returncode
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv[1:]))
    except (OSError,ValueError,KeyError,TypeError) as error:
        print(f'image-receipt: REFUSED: {error}',file=sys.stderr)
        sys.exit(1)
