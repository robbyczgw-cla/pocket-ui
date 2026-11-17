#!/usr/bin/env python3
"""Evidence for the D-pad explorer's release-only cases (tests/nav_explorer.rs, `-- --ignored`, ~27 min on the release host).

scripts/pre-release.sh runs them on the release tree; scripts/release-check.sh would run them again on the SAME tree. With
the pre-release receipt of that exact tree the release check carries them instead, but only when everything matches:

  nav-evidence.py record --log NAV.LOG --list LIST         receipt lines (pre-release.sh, full mode, after a green nav step)
  nav-evidence.py check  --receipt R --list LIST           exit 0: carry (one line to print); 1: run them here (the reason)
  nav-evidence.py verify --receipt R --tree T --commit C --list-sha X
                                                           8-publish check: the carried receipt again, without cargo

What must match: kind=pre-release result=ok mode=full, the nav step ok, commit and tree of this checkout, the cargo profile,
`rustc -V`, the host, RUST_TEST_THREADS, the CJK test font (sha256), the exact list of ignored nav cases (sha256 of
`cargo test --test nav_explorer -- --ignored --list`), and the log: one clean `test result: ok. N passed; 0 failed` with N =
the number of listed cases, no side-by-side retry, its sha256 unchanged. Anything missing, unknown or different: run them.
There is no switch that skips them.
"""
import argparse
import hashlib
import os
import re
import socket
import subprocess
import sys
from pathlib import Path

OK_LINE = re.compile(r'^test result: ok\. (\d+) passed; 0 failed;', re.M)


def sha_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for b in iter(lambda: f.read(1 << 20), b''):
            h.update(b)
    return h.hexdigest()


def listed(path):
    """The case names of a libtest --list output, sorted (lines `name: test`)."""
    names = sorted(l[:-len(': test')] for l in Path(path).read_text().splitlines() if l.endswith(': test'))
    return names, hashlib.sha256('\n'.join(names).encode()).hexdigest()


def environment():
    try:
        rustc = subprocess.run(['rustc', '-V'], capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        rustc = 'unknown'
    font = os.environ.get('POCKETUI_TEST_CJK_FONT', '')
    return {
        'nav_profile': os.environ.get('GATE_PROFILE', 'gate'),
        'nav_rustc': rustc,
        'nav_host': socket.gethostname(),
        'nav_threads': os.environ.get('RUST_TEST_THREADS', ''),
        'nav_cjk_font': sha_file(font) if font and os.path.isfile(font) else 'none',
    }


def clean_log(log):
    """N when the log is one clean green run, else a reason string."""
    p = Path(log)
    if not p.is_file():
        return f'no log {log}'
    if Path(str(log) + '.retry').exists():
        return 'a case was retried alone (flaky side by side)'
    text = p.read_text(errors='replace')
    results = [l for l in text.splitlines() if l.startswith('test result:')]
    m = OK_LINE.findall(text)
    if len(results) != 1 or len(m) != 1:
        return f'{len(results)} test results in the log, want exactly one green'
    return int(m[0])


def git(*args):
    return subprocess.run(['git', *args], capture_output=True, text=True, check=True).stdout.strip()


def read_receipt(path):
    out = {}
    for line in Path(path).read_text().splitlines():
        if '=' in line:
            k, v = line.split('=', 1)
            out.setdefault(k, v)
    return out


def verify(r, tree, commit, list_sha, env=None):
    """None when the receipt carries the nav cases for this tree, else the reason."""
    want = {'kind': 'pre-release', 'result': 'ok', 'mode': 'full', 'commit': commit, 'tree': tree,
            'nav_carry': 'yes', 'nav_list_sha256': list_sha}
    if env:
        want.update(env)
    for k, v in want.items():
        if r.get(k) != v:
            return f'{k}={r.get(k)!r} in the pre-release receipt, here {v!r}'
    if 'nav:ok' not in r.get('steps', '').split():
        return 'the receipt has no green nav step'
    n = clean_log(r.get('nav_log', ''))
    if isinstance(n, str):
        return n
    if str(n) != r.get('nav_tests') or not r.get('nav_log_sha256'):
        return f'the log has {n} passed, the receipt {r.get("nav_tests")!r}'
    if sha_file(r['nav_log']) != r['nav_log_sha256']:
        return 'the nav log changed since the receipt was written'
    return None


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest='cmd', required=True)
    a = sub.add_parser('record'); a.add_argument('--log', required=True); a.add_argument('--list', required=True)
    a = sub.add_parser('check'); a.add_argument('--receipt', required=True); a.add_argument('--list', required=True)
    a = sub.add_parser('verify'); a.add_argument('--receipt', required=True); a.add_argument('--tree', required=True)
    a.add_argument('--commit', required=True); a.add_argument('--list-sha', required=True)
    a = p.parse_args()
    try:
        if a.cmd == 'record':
            names, list_sha = listed(a.list)
            n = clean_log(a.log)
            if isinstance(n, str) or not names or n != len(names):
                why = n if isinstance(n, str) else f'{n} passed but {len(names)} listed'
                print(f'nav_carry=no ({why})')
                return 0
            print('nav_carry=yes')
            print(f'nav_tests={n}')
            print(f'nav_list_sha256={list_sha}')
            print(f'nav_log={os.path.abspath(a.log)}')
            print(f'nav_log_sha256={sha_file(a.log)}')
            for k, v in environment().items():
                print(f'{k}={v}')
            return 0
        if a.cmd == 'check':
            names, list_sha = listed(a.list)
            if not names:
                print('nav explorer (--ignored) runs here: the test list is empty')
                return 1
            tree, commit = git('rev-parse', 'HEAD^{tree}'), git('rev-parse', 'HEAD')
            if git('status', '--porcelain', '--untracked-files=no'):
                print('nav explorer (--ignored) runs here: uncommitted changes in this checkout')
                return 1
            r = read_receipt(a.receipt)
            why = verify(r, tree, commit, list_sha, environment())
            if why:
                print(f'nav explorer (--ignored) runs here: {why}')
                return 1
            print(f'nav explorer (--ignored): carried from pre-release receipt {os.path.abspath(a.receipt)}: tree {tree[:12]}, '
                  f'{r["nav_tests"]} passed, list sha256 {list_sha}, log {r["nav_log"]} sha256 {r["nav_log_sha256"][:16]}')
            return 0
        r = read_receipt(a.receipt)
        why = verify(r, a.tree, a.commit, a.list_sha)
        if why:
            print(f'FAIL carried nav explorer evidence: {why}')
            return 1
        print(f'ok carried nav explorer evidence: {r["nav_tests"]} passed on tree {a.tree[:12]} ({a.receipt})')
        return 0
    except (OSError, ValueError, subprocess.CalledProcessError) as e:
        print(f'FAIL carried nav explorer evidence: {e}' if a.cmd == 'verify' else f'nav explorer (--ignored) runs here: {e}')
        return 1


if __name__ == '__main__':
    sys.exit(main())
