#!/usr/bin/env python3
"""Cozy extras on the X300 emulator: every phone row is there and toggles, and each extra's effect shows in the player.

Usage: extras-smoke.py ADB emulator-5582 OUTPUT GAME [PSP_GAME [LINK_GAME]]
Needs the fixtures of main-refresh-smoke.py (a homebrew game by name, Download/PocketMvp chosen). Emulator only.
Checks: the Cozy extras sheet lists the rows below with On/Off, and Settings itself the four extras it keeps as rows of
their own (as Linux's Sound & feel and Sleep & power); "Memory card in the pause menu" On shows the slot picker
with the memory card (the player's pause sheet still has Save/Load · Slot N and its arrows move the slot).
"""
import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

adb, serial, destination, game = sys.argv[1:5]
PSP_GAME = sys.argv[5] if len(sys.argv) > 5 else 'Psp2048'
out = Path(destination)
out.mkdir(parents=True, exist_ok=True)
ROWS = ['TV on the cabinet', 'Stickers on cases', 'Printed discs', 'Memory card in the pause menu', 'Light by time of day', 'Decorations', 'Game connections', 'Rediscover']
SETTINGS_ROWS = ['Playful cases', 'Old-TV switch-on', 'Case sounds', 'Slideshow before screen off']

def run(*args):
    return subprocess.check_output([adb, '-s', serial, *args])

def sh(*args):
    return run('shell', *args).decode()

assert serial.startswith('emulator-') and sh('getprop', 'ro.kernel.qemu').strip() == '1'

def nodes():
    raw = run('exec-out', 'uiautomator', 'dump', '/dev/tty').decode()
    return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>') + 12]).iter('node'))

def texts():
    return [n.get('text') or '' for n in nodes()]

def tap(label):
    n = next(n for n in reversed(nodes()) if (n.get('text') or '').startswith(label))
    x1, y1, x2, y2 = map(int, re.findall(r'\d+', n.get('bounds')))
    sh('input', 'tap', str((x1 + x2) // 2), str((y1 + y2) // 2))
    time.sleep(.6)

def key(code):
    sh('input', 'keyevent', str(code))
    time.sleep(.5)

def shot(name):
    (out / (name + '.png')).write_bytes(run('exec-out', 'screencap', '-p'))

def row(label):
    return next(t for t in texts() if t.startswith(label + ' ·'))

def choose(label):
    sh('input', 'keyevent', *(['21'] * 40))
    for _ in range(45):
        n = next((n for n in nodes() if (n.get('text') or '').casefold() == label.casefold()), None)
        if n is not None:
            if n.get('selected') != 'true':
                tap(n.get('text'))
            return
        key(22)
    raise AssertionError('Missing fixture ' + label)

def set_extra(label, value):
    tap('Settings')
    sheet = label not in SETTINGS_ROWS
    if sheet:
        tap('Cozy extras')
    if not row(label).endswith(value):
        tap(label)
    assert row(label).endswith(value), row(label)
    if sheet:
        key(4)

sh('am', 'force-stop', 'dev.pocket.spike')
sh('am', 'start', '-W', '-n', 'dev.pocket.spike/.PocketActivity')
time.sleep(2)
tap('Settings')
shown = texts()
for label in SETTINGS_ROWS:
    assert any(t.startswith(label + ' ·') for t in shown), f'Missing Settings row {label}: {shown}'
tap('Cozy extras')
shown = texts()
for label in ROWS:
    assert any(t.startswith(label + ' ·') for t in shown), f'Missing Cozy extras row {label}: {shown}'
shot('extras')
key(4)
for state in ('On', 'Off'):
    set_extra('Memory card in the pause menu', state)
    tap('Games')
    choose(game)
    tap('Play')
    time.sleep(4)
    key(4)
    time.sleep(1)
    t = texts()
    assert any(x.startswith('Save · Slot 1') for x in t), t
    key(22)
    assert any(x.startswith('Save · Slot 2') for x in t + texts()), texts()
    shot('pause-card-' + state.lower())
    tap('Quit')
    time.sleep(1)
# Printed discs: the open box of a PSP game (a UMD) shows the disc either way and the page keeps its buttons.
for state in ('On', 'Off'):
    set_extra('Printed discs', state)
    tap('Games')
    choose(PSP_GAME)
    tap('Info')
    time.sleep(1.5)
    assert {'Play', 'Back', 'Turn over'} <= set(texts()), texts()
    shot('box-discs-' + state.lower())
    key(4)
# Connections: a fact other games share is a tap target that opens their list; B comes back to the card. Needs two
# games with one developer in the offline list (fixtures named after them, e.g. "Advance Wars" and "Fire Emblem").
LINK_GAME = sys.argv[6] if len(sys.argv) > 6 else 'Advance Wars'
for state in ('On', 'Off'):
    set_extra('Game connections', state)
    tap('Games')
    choose(LINK_GAME)
    tap('Info')
    time.sleep(1.5)
    facts = [t for t in texts() if t.startswith('Developer:')]
    assert bool(facts) == (state == 'On'), (state, texts())
    if state == 'On':
        tap('Developer:')
        time.sleep(1)
        assert any(t.startswith('More from') for t in texts()), texts()
        shot('connection-list')
        key(4)
        time.sleep(1)
        assert any(t.startswith('Developer:') for t in texts()), texts()
    key(4)
set_extra('Game connections', 'On')
# Rediscover: Curator has its row, with a visible answer when nothing is there yet.
set_extra('Rediscover', 'On')
tap('Curator')
sh('input', 'swipe', '600', '2000', '600', '500', '300')
time.sleep(1)
assert any(t.startswith('Rediscover ·') for t in texts()), texts()
tap('Rediscover')
time.sleep(1)
assert any(t in ('Nothing to rediscover yet', 'Rediscover') for t in texts()), texts()
shot('rediscover')
key(4)
# Decorations and Light by time of day show on the Shelf: the end of the shelf looks different with each switched
# off. The fixtures need play time for the decorations (launcher.json "play_secs", written with adb root by
# main-refresh fixtures or by hand); without it only the plant stands there, which is a picture of its own too.
def shelf_end():
    tap('Games')
    sh('input', 'keyevent', *(['20'] * 30))
    for _ in range(3):
        sh('input', 'swipe', '600', '2000', '600', '300', '300')
    time.sleep(1.2)
    return run('exec-out', 'screencap', '-p')

for extra in ('Decorations', 'Light by time of day'):
    shots = {}
    for state in ('On', 'Off'):
        set_extra(extra, state)
        shots[state] = shelf_end()
        (out / f"shelf-end-{extra.split()[0].lower()}-{state.lower()}.png").write_bytes(shots[state])
    set_extra(extra, 'On')
    if extra == 'Decorations':
        assert shots['On'] != shots['Off'], 'Decorations changed nothing on the shelf'
    else:
        print('Light by time of day: on/off differ' if shots['On'] != shots['Off'] else 'Light by time of day: same picture (daytime afternoon is neutral)', flush=True)
# The slideshow (SLOW=1: it needs a quiet minute and covers or pictures in the app's data folder, `covers/<hash>.png`):
# after a minute without touch the page is replaced by a slide with the game's title; a touch ends it and does nothing else.
import os
if os.environ.get('SLOW'):
    set_extra('Slideshow before screen off', 'On')
    tap('Games')
    before = run('exec-out', 'screencap', '-p')
    time.sleep(70)
    during = run('exec-out', 'screencap', '-p')
    (out / 'slideshow.png').write_bytes(during)
    assert during != before and not [t for t in texts() if t in ('Games', 'Favourites')], 'The slideshow did not start'
    assert 'KEEP_SCREEN_ON' in sh('dumpsys', 'window', 'windows'), 'The screen is not kept on while the slideshow runs'
    sh('input', 'tap', '600', '1200')
    time.sleep(1.2)
    assert 'Games' in texts() and 'Favourites' in texts(), texts()
    set_extra('Slideshow before screen off', 'Off')
print('Cozy extras rows + memory card + printed discs + connections + rediscover + decorations + light PASS', flush=True)
