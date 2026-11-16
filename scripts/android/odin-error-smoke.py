#!/usr/bin/env python3
"""Failed-start keys through the actual Android dialog. Disposable rootable emulator + QA package only."""
import argparse, json, re, subprocess, time
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('adb'); p.add_argument('serial'); p.add_argument('package'); p.add_argument('out', type=Path)
p.add_argument('--keys', default='110,4,82,97,111,23,66,96')
a = p.parse_args()
assert a.serial.startswith('emulator-') and a.package.startswith('xyz.pocketui.qa.'), 'Disposable emulator/QA package required'
def adb(*args): return subprocess.check_output([a.adb, '-s', a.serial, *args], timeout=40)
def shell(*args): return adb('shell', *args).decode()
assert shell('getprop', 'ro.kernel.qemu').strip() == '1'
assert shell('id', '-u').strip() == '0', 'Root is needed to explicitly launch the private PlayerActivity in an old QA APK'
a.out.mkdir(parents=True, exist_ok=True)
def tree(name):
    xml = adb('exec-out', 'uiautomator', 'dump', '/dev/tty').decode()
    (a.out / (name + '.xml')).write_text(xml)
    return xml
shell('settings', 'put', 'secure', 'immersive_mode_confirmations', 'confirmed')
results = []
for key in map(int, a.keys.split(',')):
    shell('am', 'force-stop', a.package)
    shell('am', 'start', '-W', '-n', a.package + '/xyz.pocketui.app.PocketActivity')
    shell('am', 'start', '-W', '-n', a.package + '/xyz.pocketui.app.PlayerActivity',
          '--es', 'core', '/missing/odin-core.so', '--es', 'rom', '/missing/Game.md',
          '--es', 'system', 'megadrive', '--es', 'key', 'odin-error', '--es', 'title', 'ErrorProbe')
    for _ in range(6):
        xml = tree(str(key) + '-before')
        if "Game couldn't start" in xml: break
        time.sleep(.3)
    assert "Game couldn't start" in xml, 'The start error must be visible before pressing a key'
    if not results: (a.out / 'error.png').write_bytes(adb('exec-out', 'screencap', '-p'))
    shell('input', 'gamepad', 'keyevent', str(key))
    time.sleep(.3)
    xml = tree(str(key) + '-after')
    front = shell('dumpsys', 'activity', 'activities')
    resumed = next((line for line in front.splitlines() if 'topResumedActivity=' in line or 'mResumedActivity:' in line), '')
    assert "Game couldn't start" not in xml and a.package in resumed and 'PlayerActivity' not in resumed, (
        f'key {key} must return to the library, got {resumed}')
    results.append({'key': key, 'returned': resumed.strip()})
    (a.out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print('PASS failed-start key', key, flush=True)
