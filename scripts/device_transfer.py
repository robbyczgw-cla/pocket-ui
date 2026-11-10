"""Verified host/device file transfers; one retry, resuming a verified prefix.

The remote callable accepts (command, data=None, timeout=SECONDS) and returns
stdout bytes, raising CalledProcessError or TimeoutExpired with partial output.
Only firmware commands are used remotely; no stat, timeout, rsync or Python.
Destinations become visible only after size and SHA-256 both match.
"""
import hashlib
import os
from pathlib import Path
import re
import shlex
import subprocess

Q = shlex.quote


class TransferError(RuntimeError):
    pass


def metadata(remote, path, timeout=120):
    result = remote('set -e; wc -c < '+Q(path)+'; sha256sum '+Q(path), timeout=timeout)
    lines = result.decode().splitlines()
    if len(lines) != 2 or not lines[0].strip().isdigit() or not re.match(r'^[0-9a-f]{64} ', lines[1]):
        raise TransferError('invalid size/SHA-256 response for '+path)
    return int(lines[0]), lines[1][:64]


def upload(remote, path, data, timeout=120):
    """Upload to path.part, resume once, then atomically replace path."""
    expected = len(data), hashlib.sha256(data).hexdigest()
    part = path+'.part'
    failure = None
    for _ in range(2):
        try:
            remote('[ -f '+Q(part)+' ] || : > '+Q(part), timeout=timeout)
            size, digest = metadata(remote, part, timeout)
            if size > len(data) or hashlib.sha256(data[:size]).hexdigest() != digest:
                remote(': > '+Q(part), timeout=timeout)
                size = 0
            if size < len(data):
                remote('cat >> '+Q(part), data=data[size:], timeout=timeout)
            if metadata(remote, part, timeout) != expected:
                raise TransferError('uploaded size/SHA-256 mismatch: '+path)
            remote('mv -f '+Q(part)+' '+Q(path), timeout=timeout)
            return
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, TransferError) as error:
            failure = error
    raise TransferError('upload failed after two attempts: '+path) from failure


def download(remote, path, destination, timeout=120):
    """Download to destination.part and publish only the verified complete file."""
    destination = Path(destination)
    part = destination.with_name(destination.name+'.part')
    expected = metadata(remote, path, timeout)
    failure = None
    for _ in range(2):
        try:
            size = part.stat().st_size if part.exists() else 0
            if size:
                digest = remote('head -c '+str(size)+' '+Q(path)+' | sha256sum', timeout=timeout).decode()[:64]
                with part.open('rb') as stream:
                    valid = hashlib.file_digest(stream, 'sha256').hexdigest() == digest
                if size > expected[0] or not valid:
                    part.unlink(); size = 0
            # Keep every received byte even when SSH timed out or disconnected.
            # The next attempt verifies this prefix before resuming it.
            with part.open('ab') as stream:
                part.chmod(0o600)
                try:
                    stream.write(remote('tail -c +'+str(size+1)+' '+Q(path), timeout=timeout))
                except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
                    stream.write(error.output or b'')
                    raise
            with part.open('rb') as stream:
                actual = part.stat().st_size, hashlib.file_digest(stream, 'sha256').hexdigest()
            if actual != expected or metadata(remote, path, timeout) != expected:
                raise TransferError('downloaded size/SHA-256 mismatch: '+path)
            os.replace(part, destination)
            return
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired, TransferError) as error:
            failure = error
    raise TransferError('download failed after two attempts: '+path) from failure


def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('direction', choices=('get', 'put'))
    parser.add_argument('remote_path')
    parser.add_argument('local_path')
    parser.add_argument('--seconds', type=float, default=120)
    parser.add_argument('ssh', nargs=argparse.REMAINDER, help='-- ssh [options] host (or sh -c for fixtures)')
    args = parser.parse_args()
    command = args.ssh
    if command[:1] == ['--']: command = command[1:]
    if not command: parser.error('missing SSH command')
    def remote(script, data=None, timeout=120):
        return subprocess.run(command+[script], input=data, capture_output=True,
                              timeout=timeout, check=True).stdout
    if args.direction == 'get': download(remote, args.remote_path, args.local_path, args.seconds)
    else: upload(remote, args.remote_path, Path(args.local_path).read_bytes(), args.seconds)


if __name__ == '__main__': main()
