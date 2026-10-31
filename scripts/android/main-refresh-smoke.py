#!/usr/bin/env python3
"""Four X300 screenshots plus touch, slots and warm return; prepare classic/core-smoke fixtures first."""
import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

adb, serial, destination = sys.argv[1:]
out = Path(destination)
out.mkdir(parents=True, exist_ok=True)

def run(*args):
    return subprocess.check_output([adb, '-s', serial, *args])

def sh(*args):
    return run('shell', *args).decode()

assert serial.startswith('emulator-') and sh('getprop', 'ro.kernel.qemu').strip() == '1'

def nodes():
    raw = run('exec-out', 'uiautomator', 'dump', '/dev/tty').decode()
    return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>') + 12]).iter('node'))

def tap(label):
    n = next(n for n in reversed(nodes()) if n.get('text') == label and n.get('enabled') == 'true')
    x1, y1, x2, y2 = map(int, re.findall(r'\d+', n.get('bounds')))
    sh('input', 'tap', str((x1 + x2) // 2), str((y1 + y2) // 2))
    time.sleep(.5)

def activate(label):
    node = next(n for n in nodes() if n.get('text') == label and n.get('enabled') == 'true')
    if node.get('selected') != 'true':
        tap(label)
    tap(label)

def key(code):
    sh('input', 'keyevent', str(code))
    time.sleep(.4)

def choose(label):
    sh('input', 'keyevent', *(['21'] * 40))
    for _ in range(45):
        n = next((n for n in nodes() if n.get('text', '').casefold() == label.casefold()), None)
        if n is not None:
            if n.get('selected') != 'true':
                tap(n.get('text'))
            return
        key(22)
    raise AssertionError('Missing fixture ' + label)

def look(theme):
    tap('Settings')
    sh('input', 'keyevent', *(['19'] * 8))
    for _ in range(6):
        label = next(n.get('text') for n in nodes() if n.get('text', '').startswith('Look ·'))
        if label.endswith(theme):
            break
        tap(label)
    else:
        raise AssertionError('Look did not change')
    key(4)

def rotate(portrait):
    run('emu', 'sensor', 'set', 'acceleration', '0:9.8:0' if portrait else '9.8:0:0')
    time.sleep(2)

def shot(name):
    (out / (name + '.png')).write_bytes(run('exec-out', 'screencap', '-p'))

sh('am', 'force-stop', 'dev.pocket.spike')
sh('am', 'start', '-W', '-n', 'dev.pocket.spike/.PocketActivity')
time.sleep(2)
rotate(False)
look('Cabinet')
choose('Attack on Voxelburg')
key(100)  # Favourite sticker; the fixture AVD is ours.
shot('01-cabinet-landscape')
tap('Info')
time.sleep(.8)
shot('02-open-game-box')
key(4)
rotate(True)
shot('03-cabinet-portrait')
# Both physical touch and controller input must keep the same selected game.
game = next(n for n in nodes() if n.get('selected') == 'true' and n.get('text') == 'Attack on Voxelburg')
x1, y1, x2, y2 = map(int, re.findall(r'\d+', game.get('bounds')))
sh('input', 'swipe', str((x1 + x2) // 2), str((y1 + y2) // 2), str(x1 + 12), str((y1 + y2) // 2), '300')
time.sleep(.5)
choose('Psp2048')
tap('Play')
time.sleep(3)
key(4)
assert 'Continue' in {n.get('text') for n in nodes()}
sh('logcat', '-c')
activate('Save · Slot 1')
assert 'player state saved:' in sh('logcat', '-d')
activate('Load · Slot 1')
assert 'player state loaded:' in sh('logcat', '-d')
shot('04-player-save-row')
activate('Continue')
time.sleep(1)
key(4)
activate('Quit')
time.sleep(1)
# Warm start keeps provider identity and reads the separate automatic state.
sh('logcat', '-c')
tap('Play')
time.sleep(3)
log = sh('logcat', '-d')
assert 'resume=Resumed' in log and 'Android core renderer: GLES 3' in log
key(4)
activate('Quit')
assert 'Fatal signal' not in log
print('X300 landscape/portrait, touch, box, slots, warm return PASS', flush=True)
