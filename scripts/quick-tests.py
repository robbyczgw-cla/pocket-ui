#!/usr/bin/env python3
"""Choose integration suites conservatively from changed paths; print cargo --test arguments."""
import pathlib
import subprocess
import sys

shell = '--shell' in sys.argv
new_states = '--states' in sys.argv
args = [arg for arg in sys.argv[1:] if arg not in {'--shell', '--states'}]
base = args[0] if args else 'main'
# Include committed branch changes, staged and unstaged edits, and untracked tests.
changed = set(subprocess.check_output(['git', 'diff', '--name-only', base + '...HEAD'], text=True).splitlines())
changed.update(subprocess.check_output(['git', 'diff', '--name-only', 'HEAD'], text=True).splitlines())
changed.update(subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'], text=True).splitlines())
if new_states:
    def states(text):
        return {line.strip().removeprefix('safe ') for line in text.splitlines()
                if line.strip() and not line.lstrip().startswith('#')}
    before = subprocess.check_output(['git', 'show', base + ':tests/i18n-states.txt'], text=True)
    current = pathlib.Path('tests/i18n-states.txt').read_text()
    print('\n'.join(sorted(states(current) - states(before))))
    raise SystemExit(0)
if shell:
    if any(p.startswith(('scripts/', 'deploy/')) for p in changed):
        print('deploy/test-run-pocket-ui.sh')
        print('scripts/test-verify-device.sh')
        print('scripts/test-check-core-licences.sh')
        print('deploy/package/test-package.sh')
        print('deploy/test-grub-quiet.sh')
    raise SystemExit(0)
all_tests = {p.stem for p in pathlib.Path('tests').glob('*.rs')}
selected = {'first_day', 'user_journeys'} & all_tests
broad = any(p in {'Cargo.toml', 'Cargo.lock', 'build.rs', 'src/lib.rs', 'src/main.rs'} or
            p.startswith(('src/app_', 'assets/', 'testdata/', 'src/text', 'src/gfx', 'src/palette', 'src/settings', 'src/type_scale'))
            or (p.startswith('tests/') and p.count('/') > 1) for p in changed)
if broad:
    selected.update(all_tests)
else:
    for p in changed:
        if p in {'tests/i18n-states.txt', 'tests/layout-risk-states.txt'}:
            selected.update({'i18n_snapshots', 'text_size'} & all_tests)
        if p.startswith('tests/') and pathlib.Path(p).stem in all_tests:
            selected.add(pathlib.Path(p).stem)
        if p.startswith('src/'):
            # A library module can feed several UI surfaces; keep all integration contracts.
            selected.update(all_tests)
# Absolute CPU/frame-time limits describe the gate/shipping optimisation level, not the development profile.
# They remain mandatory in the full merge gate, with nextest's narrowly scoped load retries when selected.
selected.difference_update({'settings_open', 'shelf_moves'})
for suite in sorted(selected):
    print('--test')
    print(suite)
