#!/bin/sh
# Quick self-test of the monkey runner itself (no UI, no build): profiles, seeded
# actions with taps, card variants. Not part of the merge gate's shell list, so it
# never slows the gate; run it by hand after changing scripts/monkey.py.
set -e
cd "$(dirname "$0")/.."
python3 -I scripts/monkey.py --out /tmp/monkey-self-test --self-test
