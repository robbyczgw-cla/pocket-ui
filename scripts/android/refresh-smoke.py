#!/usr/bin/env python3
"""0.1.14 refresh on an isolated, disposable emulator (root required).
Resets launcher preferences, adds two generated GB fixtures through the real SAF
picker, checks Easy in both appearances/orientations, save/continue and Just Play.
Never run against a shared emulator: its private preferences are changed.
"""
import json, os, re, subprocess, sys, time, xml.etree.ElementTree as ET
from pathlib import Path

adb, serial, output = sys.argv[1:]
out = Path(output); out.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
pkg = 'dev.pocket.spike'

def run(*args, **kw):
    return subprocess.check_output([adb, '-s', serial, *args], **kw)

def shell(*args):
    return run('shell', *args, text=True)

assert serial.startswith('emulator-') and shell('getprop', 'ro.kernel.qemu').strip() == '1', 'Emulator required'
run('root'); time.sleep(1)
assert shell('id').startswith('uid=0'), 'Disposable rootable emulator required'
sequence = 0

def tree():
    global sequence
    for _ in range(6):
        raw = run('exec-out', 'uiautomator', 'dump', '/dev/tty', text=True)
        if '<?xml' in raw and '</hierarchy>' in raw:
            (out / f'ui-{sequence:03}.xml').write_text(raw); sequence += 1
            return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>') + 12]).iter('node'))
        time.sleep(.5)
    raise AssertionError('No UI tree after six attempts: ' + raw)

def tap(label):
    for _ in range(8):
        node = next((n for n in reversed(tree()) if n.get('text') == label and n.get('enabled') == 'true'), None)
        if node is not None:
            x1, y1, x2, y2 = map(int, re.findall(r'\d+', node.get('bounds')))
            shell('input', 'tap', str((x1+x2)//2), str((y1+y2)//2)); time.sleep(.5)
            return
        time.sleep(.3)
    raise AssertionError('Missing target: ' + label)

def wait(label):
    for _ in range(12):
        if any(n.get('text') == label for n in tree()): return
        time.sleep(.4)
    raise AssertionError('Missing: ' + label)

def shot(name):
    (out / (name + '.png')).write_bytes(run('exec-out', 'screencap', '-p'))

def prefs(**changes):
    shell('am', 'force-stop', pkg)
    raw = shell('cat', '/data/data/' + pkg + '/files/launcher.json')
    value = json.loads(raw) if raw.strip() else {}
    value.update(changes)
    path = out / 'launcher.json'; path.write_text(json.dumps(value))
    run('push', str(path), '/data/local/tmp/android-0114-prefs.json')
    shell('cp', '/data/local/tmp/android-0114-prefs.json', '/data/data/' + pkg + '/files/launcher.json')
    shell('am', 'start', '-W', '-n', pkg + '/.PocketActivity'); time.sleep(1)

if os.environ.get('ANDROID_REFRESH_SETUP_ONLY') != '1':
    rom = out / 'Pocket Audio.gb'
    subprocess.check_call([sys.executable, str(root/'scripts/android/audio-test-rom.py'), str(rom)])
    shell('mkdir', '-p', '/sdcard/Download/PocketMvp/gb')
    run('push', str(rom), '/sdcard/Download/PocketMvp/gb/Pocket Audio.gb')
    run('push', str(rom), '/sdcard/Download/PocketMvp/gb/Pocket Other.gb')
    prefs(onboarding_done=True, language='en', theme='shelf', light=False)
    shell('input', 'keyevent', '34'); time.sleep(1)
    subprocess.check_call([sys.executable, str(root/'scripts/android/select-demo-folder.py'), adb, serial, str(out)])
    if any(n.get('text') == 'Not now' for n in tree()): tap('Not now')
    wait('Just Play')
    for width, height, orientation in [(720,1280,'portrait'), (1280,720,'landscape')]:
        shell('wm', 'size', f'{width}x{height}')
        for light in [False, True]:
            prefs(theme='big', light=light, selected_key='com.android.externalstorage.documents:primary:Download/PocketMvp/gb/Pocket Audio.gb', selected_uri='')
            wait('Pocket Audio')
            shot('easy-' + orientation + ('-light' if light else '-dark'))
            tap('›'); wait('Pocket Other')
            tap('‹'); wait('Pocket Audio')
            shell('input', 'keyevent', '22'); wait('Pocket Other')
            shell('input', 'keyevent', '21'); wait('Pocket Audio')
    # Direct A/Play, manual slot, automatic moment and return to Easy.
    tap('Continue' if any(n.get('text') == 'Continue' for n in tree()) else 'Play'); wait('Pause')
    shell('input', 'keyevent', '4'); wait('Save & quit')
    shot('player-pause')
    tap('Save · Slot 1'); tap('Load · Slot 1')
    # Deny the app atomic state replacement: the error must leave the sheet open.
    state_dirs = shell('find', '/data/data/' + pkg + '/files/states', '-mindepth', '1', '-maxdepth', '1', '-type', 'd').splitlines()
    assert state_dirs, 'No player state directory'
    auto_paths = shell('find', '/data/data/' + pkg + '/files/states', '-name', '*.pstate.auto*').splitlines()
    auto_before = {path: run('exec-out', 'cat', path) for path in auto_paths}
    for path in state_dirs: shell('chmod', '500', path)
    try:
        tap('Save & quit'); wait('Save & quit'); shot('player-save-error')
        assert all(run('exec-out', 'cat', path) == data for path, data in auto_before.items()), 'Failed save changed old moment'
    finally:
        for path in state_dirs: shell('chmod', '700', path)
    tap('Save & quit')
    wait('Continue'); shot('easy-continue')
    tap('Continue'); wait('Pause')
    shell('input', 'keyevent', '4'); tap('Save & quit'); wait('Continue')
    prefs(theme='shelf')
    wait('Continue')
    tap('Just Play'); wait('Pause')
    shell('input', 'keyevent', '4'); tap('Save & quit'); wait('Just Play')
    shot('games-just-play')
    shell('input', 'keyevent', '109'); wait('Quick settings'); shot('quick-just-play')
    tap('Just Play'); wait('Pause')
    shell('input', 'keyevent', '4'); tap('Save & quit'); wait('Just Play')
# The sixth look must not share an action ID with Animations or Appearance.
for width, height, orientation in [(720,1280,'portrait'), (1280,720,'landscape')]:
    shell('wm', 'size', f'{width}x{height}')
    prefs(onboarding_done=False, onboarding_step=3, cozy=None, light=True)
    tap('Easy'); shot('setup-easy-' + orientation)
    value = json.loads(shell('cat', '/data/data/' + pkg + '/files/launcher.json'))
    assert value['theme'] == 'big'
    tap('Cabinet')
    value = json.loads(shell('cat', '/data/data/' + pkg + '/files/launcher.json'))
    assert value['theme'] == 'cabinet'
    for prefix in ['Animations · ', 'Appearance · ']:
        node = next(n for n in tree() if n.get('text', '').startswith(prefix))
        x1,y1,x2,y2 = map(int, re.findall(r'\d+', node.get('bounds')))
        assert x2-x1 > 150 and y2-y1 > 40, 'Clipped setup control: ' + prefix
        tap(node.get('text'))
    value = json.loads(shell('cat', '/data/data/' + pkg + '/files/launcher.json'))
    assert value['cozy'] is False and value['light'] is False
    shot('setup-controls-' + orientation)
print('PASS: six setup looks, unclipped Animations/Appearance, single-case Easy preview, portrait/landscape' if os.environ.get('ANDROID_REFRESH_SETUP_ONLY') == '1' else 'PASS: Easy, slots, failed-save recovery, Continue shelf, Just Play and setup in both orientations')
