#!/usr/bin/env python3
"""Repeat snapshot scenarios for perf/pprof (timings come from latency.sh, not this runner).

Set POCKETUI_DATA, POCKETUI_ES_SYSTEMS and, on headless Linux, WAYLAND_DISPLAY to
the disposable fixture and wayland-stub.py socket. Build with symbols retained, then:
  perf record -F 499 --call-graph dwarf,16384 -- python3 scripts/perf-profile.py list-cold
The five scenarios match docs/latency/perf-night-2026-10-29-profiles.txt.
"""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('scenario', choices=['start-cold', 'list-cold', 'cabinet', 'box', 'pause-quick'])
    parser.add_argument('--binary', type=Path, default=Path(__file__).resolve().parents[1] / 'target/gate/pocket-ui')
    parser.add_argument('--repeats', type=int)
    args = parser.parse_args()
    if not all(os.environ.get(key) for key in ['POCKETUI_DATA', 'POCKETUI_ES_SYSTEMS']):
        parser.error('set POCKETUI_DATA and POCKETUI_ES_SYSTEMS to a disposable performance fixture')
    states = {
        'start-cold': ['home'],
        'list-cold': ['shelf:gbc:split:games:15'],
        'cabinet': ['bench-shelf:cabinet'],
        'box': ['box:gbc:Perf'],
        'pause-quick': ['menu:pocket-core', 'game-quick'],
    }[args.scenario]
    repeats = args.repeats if args.repeats is not None else (2 if args.scenario in ['cabinet', 'pause-quick'] else 12)
    env = os.environ | {'POCKETUI_MOCK': '1', 'POCKETUI_LANG': 'en'}
    env.pop('POCKETUI_FRAME_BENCH', None)
    if args.scenario == 'pause-quick':
        env['POCKETUI_FRAME_BENCH'] = '1'
    with tempfile.TemporaryDirectory(prefix='pocket-perf-') as tmp:
        for _ in range(repeats):
            for state in states:
                subprocess.run([str(args.binary), '--snapshot', state, str(Path(tmp) / 'frame.png')],
                               env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)


if __name__ == '__main__':
    main()
