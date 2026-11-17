#!/usr/bin/env python3
"""Fail closed before a version bump unless the exact main commit has a full-suite receipt."""
import argparse
import json
from pathlib import Path
import subprocess
import sys


def check(repo, main, receipt=None, verify_source=False):
    def git(*args):
        return subprocess.check_output(['git', '-C', str(repo), *args], text=True, stderr=subprocess.PIPE).strip()

    sha = git('rev-parse', '--verify', f'{main}^{{commit}}')
    tree = git('rev-parse', f'{sha}^{{tree}}')
    if verify_source:
        if git('rev-parse', 'HEAD') != sha or git('status', '--porcelain', '--untracked-files=normal'):
            raise ValueError('source changed during full-suite run; no green receipt')
        return
    # A frozen earlier main is allowed; a feature-branch receipt cannot authorize a cut.
    git('merge-base', '--is-ancestor', sha, 'refs/heads/main')
    data = json.loads(Path(receipt).read_text())
    if not isinstance(data, dict):
        raise ValueError('receipt must be a JSON object')
    expected = dict(suite='gate-full-v1', verdict='green', mode='full', sha=sha, tree=tree)
    for key, value in expected.items():
        if data.get(key) != value:
            raise ValueError(f'{key}={data.get(key)!r}, expected {value!r}')
    if data.get('kind') not in ('full-suite', 'nightly') or data.get('profile') not in ('gate', 'release'):
        raise ValueError('not a supported full-suite/nightly receipt at gate/release optimisation')
    print(f'PRE-CUT: ok {sha} (exact main commit, full suite green; {receipt})')


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo', default='.')
    p.add_argument('--main', required=True)
    p.add_argument('--receipt')
    p.add_argument('--verify-source', action='store_true', help='Verify a frozen clean checkout before/after a full run')
    a = p.parse_args()
    if not a.verify_source and not a.receipt:
        p.error('--receipt is required before a cut')
    try:
        check(a.repo, a.main, a.receipt, a.verify_source)
    except (OSError, ValueError, subprocess.CalledProcessError) as e:
        print(f'PRE-CUT: REFUSED: no valid green full-suite receipt for exact main {a.main}: {e}', file=sys.stderr)
        print('Wait for a green nightly on that exact commit, or in a clean checkout of it run:\n'
              '  sh scripts/full-suite.sh --receipt /path/to/full-suite.json\n'
              'Then pass FULL_SUITE_RECEIPT=/path/to/full-suite.json to build-all.sh. No bump was made.', file=sys.stderr)
        sys.exit(2)
