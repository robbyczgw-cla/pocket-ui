#!/usr/bin/env python3
"""Frozen games on the X300 emulator, from a fresh install with default settings. Emulator only.

Usage: frozen-smoke.py ADB emulator-5582 APK OUTPUT PHASE...
Phases (each starts from a fresh install, so another agent's session in between doesn't matter):
  flow     first start with real SAF picker, play, Quit, the case stands open, tap, the same moment is back
  five     seven games played: five stand open, an external game never does; off switch; every look, both orientations
  safety   changed file (same and other size), file gone, state gone, broken state, app reinstalled, kill
  strip    the same start on film
  measure  tap to first game frame: frozen against off, no moment and cold (the launcher's and the player's log lines)
  lost     the folder's grant taken away (root, emulator only), chosen again
  big      a 1.8 GB PSP ISO: homebrew PSP2048 in a generated ISO, padded (sparse): first start with its bar, Cancel, killed
           mid-copy, the second start without a copy and its moment, a game with no room, lost grant, changed and missing file
Fixtures: /sdcard/Download/PocketMvp from main-refresh-smoke.py's homebrew, copied to PocketFrozen. Screens go to OUTPUT as PNG.
"""
import hashlib, json, os, re, shlex, struct, subprocess, sys, tempfile, time, urllib.request, xml.etree.ElementTree as ET
from pathlib import Path

adb, serial, apk, destination, *phases = sys.argv[1:]
out = Path(destination)
out.mkdir(parents=True, exist_ok=True)
PACKAGE = 'xyz.pocketui.app'
ROOT = '/sdcard/Download/PocketFrozen'
# (fixture under PocketMvp, the name the shelf gives it)
GAMES = json.loads((Path(__file__).parent / 'frozen-smoke-games.json').read_text())
EXTERNAL = 'ps2/Outside Emulator.iso'
# Fixtures made longer after their copy, in MiB (`phase_big`).
PADDED = {}
results = []

def run(*args, text=True, check=True):
    p = subprocess.run([adb, '-s', serial, *args], capture_output=True, text=text, timeout=120)
    if check and p.returncode:
        raise RuntimeError(f'{args}: {p.stderr}')
    return p.stdout

def sh(*args, check=True):
    return run('shell', ' '.join(shlex.quote(a) for a in args), check=check)

assert serial.startswith('emulator-') and sh('getprop', 'ro.kernel.qemu').strip() == '1', 'Emulator only'

def nodes():
    for _ in range(6):
        raw = run('exec-out', 'uiautomator', 'dump', '/dev/tty', check=False)
        if '</hierarchy>' in raw:
            return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>') + 12]).iter('node'))
        time.sleep(.5)
    raise AssertionError('no UI hierarchy')

def texts():
    return [n.get('text') or n.get('content-desc') or '' for n in nodes()]

def find(label, prefix=False):
    for n in reversed(nodes()):
        for v in (n.get('text') or '', n.get('content-desc') or ''):
            if (v.startswith(label) if prefix else v == label) and n.get('enabled') == 'true':
                return n
    return None

def touch(n):
    x1, y1, x2, y2 = map(int, re.findall(r'\d+', n.get('bounds')))
    sh('input', 'tap', str((x1 + x2) // 2), str((y1 + y2) // 2))

def tap(label, prefix=False, wait=.5, tries=10):
    for _ in range(tries):
        n = find(label, prefix)
        if n is not None:
            touch(n)
            time.sleep(wait)
            return
        time.sleep(.5)
    raise AssertionError(f'No target {label!r}: {texts()}')

def wait_text(label, prefix=True, seconds=20):
    end = time.time() + seconds
    while time.time() < end:
        if any(t.startswith(label) if prefix else t == label for t in texts()):
            return True
        time.sleep(.4)
    raise AssertionError(f'No {label!r} after {seconds} s: {texts()}')

def key(code):
    sh('input', 'keyevent', str(code))
    time.sleep(.4)

def shot(name):
    (out / (name + '.png')).write_bytes(run('exec-out', 'screencap', '-p', text=False))

def check(name, ok, detail=''):
    results.append((name, bool(ok), detail))
    print(('ok   ' if ok else 'FAIL ') + name + (f': {detail}' if detail else ''), flush=True)

def orientation(portrait):
    run('emu', 'sensor', 'set', 'acceleration', '0:9.8:0' if portrait else '9.8:0:0')
    time.sleep(2)
    size = re.findall(r'cur=(\d+)x(\d+)', sh('dumpsys', 'window', 'displays'))
    if size:
        w, h = map(int, size[0])
        assert (w < h) == portrait, f'orientation {w}x{h}'

def fixtures():
    sh('rm', '-rf', ROOT)
    for source, _name in GAMES:
        folder = source.rsplit('/', 1)[0]
        sh('mkdir', '-p', f'{ROOT}/{folder}')
        sh('cp', f"/sdcard/Download/PocketMvp/{source}", f'{ROOT}/{source}')
        if source in PADDED:
            sh('truncate', '-s', f'{PADDED[source]}M', f'{ROOT}/{source}')
    sh('mkdir', '-p', f'{ROOT}/ps2')
    sh('sh', '-c', f"dd if=/dev/zero of='{ROOT}/{EXTERNAL}' bs=1024 count=8 2>/dev/null")

def pick_folder():
    model = sh('getprop', 'ro.product.model').strip()
    entered = False
    for _ in range(30):
        ns = nodes()
        if entered and not any(n.get('package', '').endswith('documentsui') for n in ns):
            return
        have = {n.get('text') or n.get('content-desc') for n in ns}
        target = next((n for n in reversed(ns) if (n.get('text') or '') in ('ALLOW', 'Allow', 'Got it', 'OK') and n.get('enabled') == 'true'), None)
        if target is None and 'PocketFrozen' in have:
            target = next((n for n in reversed(ns) if (n.get('text') or '') in ('USE THIS FOLDER', 'Use this folder') and n.get('enabled') == 'true'), None)
            entered = entered or target is not None
        if target is None:
            target = next((n for n in reversed(ns) if n.get('text') == 'PocketFrozen' and n.get('resource-id', '').endswith('android:id/title')), None)
        if target is None:
            for label in ('Download', 'Downloads', model, 'Show roots'):
                target = next((n for n in reversed(ns) if label in (n.get('text'), n.get('content-desc')) and n.get('enabled') == 'true'), None)
                if target is not None:
                    break
        if target is None:
            key(4)
            continue
        touch(target)
        time.sleep(.8)
    raise AssertionError(f'SAF picker: {sorted(have, key=str)}')

def fresh():
    """A fresh install: default settings, the real first start, the real folder picker."""
    sh('am', 'force-stop', PACKAGE)
    sh('pm', 'uninstall', PACKAGE, check=False)
    run('install', '-r', apk)
    fixtures()
    orientation(True)
    sh('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
    time.sleep(2)
    # The system's own "Viewing full screen" hint, once on a new phone.
    if find('Got it') is not None:
        tap('Got it')
    wait_text('Step 1 of 8')
    tap('Continue')
    wait_text('Step 2 of 8')
    tap('Continue')
    wait_text('Step 3 of 8')
    tap('Choose your ROM folder')
    time.sleep(1)
    pick_folder()
    wait_text('Step 3 of 8')
    for step in (3, 4, 5, 6, 7):
        wait_text(f'Step {step} of 8')
        tap('Skip' if step == 5 else 'Continue')
    wait_text('Step 8 of 8')
    tap('Continue')
    wait_text('Games')
    time.sleep(2)

def restart(*extras):
    sh('am', 'force-stop', PACKAGE)
    sh('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity', *extras)
    wait_text('Games')
    time.sleep(2.5)

def choose(name):
    """Selects the game on the shelf: by a tap on its case or row where it is on screen, else with the arrow keys."""
    # Shelves hold one game per system row here: the page is searched top to bottom with Down, then (from the end) with Up,
    # and along a shelf with Right and Left when a row holds several games.
    for step in [19] * 12 + [20] * 40 + [19] * 40:
        found = [n for n in nodes() if (n.get('text') or '').casefold() == name.casefold()]
        if found:
            if not any(n.get('selected') == 'true' for n in found):
                touch(found[0])
                time.sleep(.6)
            return
        key(step)
    for step in [22] * 6 + [21] * 6:
        found = [n for n in nodes() if (n.get('text') or '').casefold() == name.casefold()]
        if found:
            touch(found[0])
            time.sleep(.6)
            return
        key(step)
    raise AssertionError('Missing fixture ' + name)

def status():
    """The line under the selected game's name."""
    return next((t for t in texts() if t.startswith('Frozen · ')), '')

def play():
    tap('Play')

def in_game(seconds=25):
    wait_text('Pause', prefix=False, seconds=seconds)

def pause_menu():
    if 'Save & quit' not in texts():
        key(4)
    wait_text('Save & quit', prefix=False)

def quit_game():
    pause_menu()
    tap('Save & quit', wait=1)
    wait_text('Games')
    time.sleep(1.5)

def open_count():
    logs = run('logcat', '-d', '-s', 'PocketMvp:I')
    found = re.findall(r'frozen games standing open: (\d+)', logs)
    return int(found[-1]) if found else None

def set_setting(label, value):
    tap('Settings')
    time.sleep(.5)
    for _ in range(12):
        row = next((t for t in texts() if t.startswith(label + ' ·')), None)
        if row:
            break
        sh('input', 'swipe', '600', '1800', '600', '900', '300')
        time.sleep(.5)
    assert row, f'No Settings row {label}: {texts()}'
    if not row.endswith(value):
        tap(row)
    row = next(t for t in texts() if t.startswith(label + ' ·'))
    assert row.endswith(value), row
    tap('Games')
    time.sleep(1.5)

def first_frame_ms(act, seconds=30):
    """logcat from the launcher's launch line to the player's first frame, in ms."""
    run('logcat', '-c')
    act()
    end = time.time() + seconds
    while time.time() < end:
        logs = run('logcat', '-d', '-v', 'epoch', '-s', 'PocketMvp:I')
        start = re.search(r'^\s*(\d+\.\d+).*launch: ', logs, re.M)
        frame = re.search(r'^\s*(\d+\.\d+).*player first game frame', logs, re.M)
        if start and frame:
            return round((float(frame[1]) - float(start[1])) * 1000)
        time.sleep(.2)
    raise AssertionError('no first frame: ' + logs[-800:])

def is_frozen_label(name):
    choose(name)
    time.sleep(.5)
    return status()

def phase_flow():
    fresh()
    run('logcat', '-c')
    shot('flow-1-fresh-shelf')
    name = GAMES[0][1]
    choose(name)
    check('never played: closed, no Frozen line', status() == '', status())
    play()
    in_game()
    time.sleep(3)
    # Leave the title: a moment a fresh start isn't at.
    for label in ('Start', 'A', 'A', 'Start', 'A'):
        tap(label, wait=.8)
    time.sleep(1.5)
    shot('flow-2-playing')
    key(4)
    wait_text('Save & quit', prefix=False)
    shot('flow-3-paused')
    tap('Save & quit', wait=1)
    wait_text('Games')
    time.sleep(2)
    shot('flow-4-stands-open')
    choose(name)
    check('after Quit: Frozen line', status().startswith('Frozen · '), status())
    shot('flow-5-selected')
    check('one stands open', open_count() == 1, str(open_count()))
    ms = first_frame_ms(play)
    time.sleep(.3)
    shot('flow-6-back-at-once')
    in_game()
    logs = run('logcat', '-d', '-s', 'PocketMvp:I')
    check('the start was frozen', f'{name} frozen=true'.casefold() in logs.casefold())
    check('core resumed', 'resume=Resumed' in logs, re.findall(r'resume=\w+', logs)[-1:] and re.findall(r'resume=\w+', logs)[-1])
    check('no note on a good resume', not any('started fresh' in t for t in texts()))
    print(f'frozen start: {ms} ms', flush=True)
    time.sleep(1.5)
    shot('flow-7-back')
    quit_game()
    check('open again after the second Quit', status().startswith('Frozen · '), status())

def phase_five():
    fresh()
    names = [n for _, n in GAMES]
    for name in names[:7]:
        choose(name)
        play()
        in_game(40)
        time.sleep(1.5)
        quit_game()
        print('played', name, open_count(), flush=True)
    time.sleep(1.5)
    check('seven played: five stand open', open_count() == 5, str(open_count()))
    shot('five-portrait-shelf')
    for name in names[:7]:
        choose(name)
        print('  ', name, repr(status()), flush=True)
    newest = names[6]
    oldest = names[0]
    check('newest stands open', is_frozen_label(newest) != '')
    check('oldest of seven closed', is_frozen_label(oldest) == '')
    # An external game: no emulator app here, so its start says so, and it never stands open.
    ext = 'Outside Emulator'
    choose(ext)
    check('external game closed before', status() == '')
    play()
    time.sleep(2)
    shot('five-external-dialog')
    if find('OK') is not None:
        tap('OK')
    time.sleep(1)
    restart()
    choose(ext)
    check('external game closed after its start', status() == '')
    check('still five', open_count() == 5, str(open_count()))
    # Looks and orientations, a game open and selected.
    for look in ('Shelf', 'Cabinet', 'Split'):
        for _ in range(6):
            tap('Settings')
            row = next(t for t in texts() if t.startswith('Look ·'))
            if row.endswith(look):
                break
            tap(row)
        tap('Games')
        time.sleep(1)
        choose(newest)
        shot(f'look-{look.lower()}-portrait')
        orientation(False)
        time.sleep(1)
        shot(f'look-{look.lower()}-landscape')
        check(f'{look} landscape: Frozen line', status().startswith('Frozen · '), status())
        orientation(True)
    tap('Settings')
    while not next(t for t in texts() if t.startswith('Look ·')).endswith('Shelf'):
        tap(next(t for t in texts() if t.startswith('Look ·')))
    tap('Games')
    # The switch: every case closed, Play still goes on from the state as it always did; back on, they stand open.
    set_setting('Frozen games', 'Off')
    time.sleep(1)
    check('off: none open', open_count() == 0, str(open_count()))
    choose(newest)
    check('off: no Frozen line', status() == '', status())
    shot('off-shelf')
    run('logcat', '-c')
    play()
    in_game(40)
    logs = run('logcat', '-d', '-s', 'PocketMvp:I')
    check('off: not a frozen start', f'{newest} frozen=false'.casefold() in logs.casefold())
    check('off: starts as before, from its moment', 'resume=Resumed' in logs, ' '.join(re.findall(r'resume=\w+', logs)))
    time.sleep(1)
    quit_game()
    set_setting('Frozen games', 'On')
    time.sleep(1)
    check('on again: five', open_count() == 5, str(open_count()))
    orientation(True)

def note_seen(fragment, seconds=15):
    end = time.time() + seconds
    while time.time() < end:
        shown = [t for t in texts() if fragment in t]
        if shown:
            return shown[0]
        time.sleep(.3)
    return ''

def played_game(name):
    choose(name)
    play()
    in_game(40)
    time.sleep(2)
    quit_game()
    choose(name)
    assert status().startswith('Frozen · '), status()

def root():
    run('root', check=False)
    time.sleep(2)
    run('wait-for-device')
    assert sh('id', '-u').strip() == '0', 'adb root refused'

def game_dir(name):
    source = next(s for s, n in GAMES if n == name)
    base = source.rsplit('/', 1)[1].rsplit('.', 1)[0]
    found = sh('sh', '-c', f"ls -d /data/data/{PACKAGE}/files/states/*/ | while read d; do ls \"$d\" | grep -qF \"{base}.pstate\" && echo \"$d\"; done; true").split('\n')
    return found[0].strip(), base

def phase_safety():
    fresh()
    (name_a, name_b, name_c, name_d, name_e, name_f) = [n for _, n in GAMES[:6]]
    src_a = next(s for s, n in GAMES if n == name_a)
    src_b = next(s for s, n in GAMES if n == name_b)
    # The file changed, the same size: the shelf can't tell; the start does, and the moment goes to slot 1.
    played_game(name_a)
    path = f'{ROOT}/{src_a}'
    sh('cp', path, '/sdcard/Download/frozen-keep.bin')
    sh('sh', '-c', f"printf 'ZZZZ' | dd of='{path}' bs=1 seek=4096 conv=notrunc 2>/dev/null")
    restart()
    choose(name_a)
    check('same size patched: shelf can\'t tell yet', status().startswith('Frozen · '), status())
    run('logcat', '-c')
    play()
    note = note_seen("file changed")
    shot('safety-changed-note')
    check('changed file: says so', 'file changed' in note, note)
    check('changed file: says where the moment is', 'slot 1' in note, note)
    in_game()
    logs = run('logcat', '-d', '-s', 'PocketMvp:I')
    check('changed file: state not loaded', 'resume=NoState' in logs, ' '.join(re.findall(r'resume=\w+', logs)))
    pause_menu()
    time.sleep(.5)
    shot('safety-changed-pause')
    tap('Load', prefix=True, wait=1.5)
    t = texts()
    time.sleep(1)
    shot('safety-changed-loaded')
    logs = run('logcat', '-d', '-s', 'PocketMvp:I')
    check('changed file: Load finds the moment in slot 1', 'player state loaded' in logs and '.pstate:' not in t, re.findall(r'player state loaded: .*', logs)[-1:] and re.findall(r'player state loaded: .*', logs)[-1][-40:])
    quit_game()
    # Its new moment is for the new file; the old one is still in slot 1.
    time.sleep(1.5)
    choose(name_a)
    check('after that Quit it stands open for the new file', status().startswith('Frozen · '), status())
    sh('cp', '/sdcard/Download/frozen-keep.bin', path)
    # Another size: the shelf already knows.
    played_game(name_b)
    path_b = f'{ROOT}/{src_b}'
    sh('sh', '-c', f"printf 'more bytes' >> '{path_b}'")
    restart()
    choose(name_b)
    check('longer file: closed on the shelf', status() == '', status())
    shot('safety-longer-closed')
    run('logcat', '-c')
    play()
    note = note_seen('file changed')
    check('longer file: says so at the start', 'file changed' in note, note)
    in_game()
    quit_game()
    # File gone, the launcher's list still has it.
    played_game(name_c)
    src_c = next(s for s, n in GAMES if n == name_c)
    sh('mv', f'{ROOT}/{src_c}', '/sdcard/Download/frozen-away.bin')
    play()
    note = note_seen("isn't in its folder")
    shot('safety-file-gone')
    check('file gone: plain message', "isn't in its folder" in note, note)
    tap('OK')
    sh('mv', '/sdcard/Download/frozen-away.bin', f'{ROOT}/{src_c}')
    restart()
    choose(name_c)
    check('file back: open again', status().startswith('Frozen · '), status())
    run('logcat', '-c')
    play()
    in_game()
    logs = run('logcat', '-d', '-s', 'PocketMvp:I')
    check('file back: the moment is back', 'resume=Resumed' in logs, ' '.join(re.findall(r'resume=\w+', logs)))
    quit_game()
    # Kill: the state of the last Quit or Home; here force-stop while playing, the game in the foreground.
    choose(name_c)
    play()
    in_game()
    time.sleep(2)
    sh('am', 'force-stop', PACKAGE)
    time.sleep(1)
    restart()
    choose(name_c)
    check('killed in the game: still open', status().startswith('Frozen · '), status())
    run('logcat', '-c')
    play()
    note = note_seen("didn't close normally")
    shot('safety-killed-note')
    check('killed: says so, back at the last saved moment', "didn't close normally" in note, note)
    in_game()
    quit_game()
    # App reinstalled (an update keeps its data).
    played_game(name_d)
    run('install', '-r', apk)
    restart()
    choose(name_d)
    check('after an update: open', status().startswith('Frozen · '), status())
    run('logcat', '-c')
    play()
    in_game()
    logs = run('logcat', '-d', '-s', 'PocketMvp:I')
    check('after an update: same moment', 'resume=Resumed' in logs, ' '.join(re.findall(r'resume=\w+', logs)))
    quit_game()
    root()
    # State gone while the shelf shows it open.
    played_game(name_e)
    d, base = game_dir(name_e)
    sh('sh', '-c', f"rm -f '{d}{base}'.pstate.auto*")
    play()
    note = note_seen('moment is gone')
    shot('safety-state-gone')
    check('state gone: says so', 'moment is gone' in note, note)
    in_game()
    quit_game()
    # Broken state: the core refuses it.
    played_game(name_f)
    d, base = game_dir(name_f)
    sh('sh', '-c', f"head -c 512 /dev/urandom > '{d}{base}.pstate.auto'")
    run('logcat', '-c')
    play()
    note = note_seen("Couldn't go back")
    shot('safety-broken-note')
    check('broken state: says so, game runs', "Couldn't go back" in note and 'slot' in note, note)
    in_game()
    time.sleep(1)
    logs = run('logcat', '-d', '-s', 'PocketMvp:I')
    check('broken state: refused', 'resume=Failed' in logs, ' '.join(re.findall(r'resume=\w+', logs)))
    shot('safety-broken-running')
    quit_game()

def phase_lost():
    """URI permission lost: the persisted grant is taken away by editing the system's list, emulator root only."""
    fresh()
    name = GAMES[2][1]
    played_game(name)
    root()
    sh('am', 'force-stop', PACKAGE)
    grants = '/data/system/urigrants.xml'
    print(sh('grep', '-c', PACKAGE, grants, check=False), flush=True)
    sh('stop')
    sh('sed', '-i', f'/{PACKAGE}/d', grants)
    sh('start')
    for _ in range(60):
        if sh('getprop', 'sys.boot_completed', check=False).strip() == '1' and sh('pidof', 'system_server', check=False).strip():
            break
        time.sleep(2)
    time.sleep(25)
    orientation(True)
    sh('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
    time.sleep(5)
    shot('lost-after-restart')
    print(texts(), flush=True)
    check('grant gone: says what to do', any('Could not read' in t or 'Choose' in t for t in texts()), '')
    # The same folder chosen again: same identity, the moment is still there.
    tap('Choose your ROM folder')
    time.sleep(1)
    pick_folder()
    wait_text('Games')
    time.sleep(3)
    choose(name)
    check('folder chosen again: open again', status().startswith('Frozen · '), status())
    shot('lost-chosen-again')
    run('logcat', '-c')
    play()
    in_game()
    logs = run('logcat', '-d', '-s', 'PocketMvp:I')
    check('folder chosen again: the moment is back', 'resume=Resumed' in logs, ' '.join(re.findall(r'resume=\w+', logs)))
    quit_game()

def phase_strip():
    """The same start on film: what the screen shows from the tap on, for a look and for the transition's time."""
    fresh()
    name = GAMES[0][1]
    choose(name)
    play()
    in_game()
    time.sleep(3)
    for label in ('Start', 'A', 'A', 'Start', 'A'):
        tap(label, wait=.8)
    time.sleep(1.5)
    quit_game()
    choose(name)
    time.sleep(1)
    n = find('Play')
    x1, y1, x2, y2 = map(int, re.findall(r'\d+', n.get('bounds')))
    rec = subprocess.Popen([adb, '-s', serial, 'shell', 'screenrecord', '--time-limit', '6', '--bit-rate', '6000000', '/sdcard/Download/fz.mp4'])
    time.sleep(2)
    sh('input', 'tap', str((x1 + x2) // 2), str((y1 + y2) // 2))
    rec.wait()
    time.sleep(1)
    run('pull', '/sdcard/Download/fz.mp4', str(out / 'strip.mp4'))
    sh('rm', '/sdcard/Download/fz.mp4')
    quit_game()

def phase_measure():
    fresh()
    report = {}
    for source, name in [g for g in GAMES if g[0].split('/')[0] in ('gb', 'gba', 'snes', 'psp')]:
        choose(name)
        fresh_ms = first_frame_ms(play)
        in_game(60)
        time.sleep(1.5)
        quit_game()
        frozen = []
        for _ in range(5):
            choose(name)
            assert status().startswith('Frozen · '), status()
            frozen.append(first_frame_ms(play))
            in_game(60)
            time.sleep(1)
            quit_game()
        report[name] = {'system': source.split('/')[0], 'first_start_no_moment': fresh_ms, 'frozen': frozen}
        print(name, report[name], flush=True)
    set_setting('Frozen games', 'Off')
    for source, name in [g for g in GAMES if g[0].split('/')[0] in ('gb', 'gba', 'snes', 'psp')]:
        off = []
        for _ in range(3):
            choose(name)
            off.append(first_frame_ms(play))
            in_game(60)
            time.sleep(1)
            quit_game()
        report[name]['off'] = off
        print(name, 'off', off, flush=True)
    set_setting('Frozen games', 'On')
    time.sleep(1)
    # Cold: the app was closed in between, nothing of the game's process left.
    for source, name in [g for g in GAMES if g[0].split('/')[0] in ('gb', 'gba', 'snes', 'psp')]:
        cold = []
        for _ in range(3):
            restart()
            choose(name)
            cold.append(first_frame_ms(play))
            in_game(60)
            time.sleep(1)
            quit_game()
        report[name]['frozen_cold'] = cold
        print(name, 'cold', cold, flush=True)
    (out / 'measure.json').write_text(json.dumps(report, indent=1))

def psp_iso(pbp):
    """A bootable PSP ISO of a homebrew EBOOT.PBP: PSP_GAME/PARAM.SFO and PSP_GAME/SYSDIR/EBOOT.BIN, nothing else."""
    at = struct.unpack('<8I', pbp[8:40])
    sfo, eboot = pbp[at[0]:at[1]], pbp[at[6]:at[7]]
    both = lambda n: struct.pack('<I', n) + struct.pack('>I', n)
    def record(name, lba, size, folder):
        r = bytes([0, 0]) + both(lba) + both(size) + bytes(7) + bytes([2 if folder else 0, 0, 0]) + struct.pack('<H', 1) + struct.pack('>H', 1) + bytes([len(name)]) + name
        r += bytes(len(r) % 2)
        return bytes([len(r)]) + r[1:]
    ROOT_AT, GAME_AT, SYS_AT, SFO_AT = 20, 21, 22, 23
    EBOOT_AT = SFO_AT + (len(sfo) + 2047) // 2048
    total = EBOOT_AT + (len(eboot) + 2047) // 2048
    def folder(me, up, *entries):
        return (record(b'\0', me, 2048, True) + record(b'\1', up, 2048, True) + b''.join(entries)).ljust(2048, b'\0')
    pvd = bytearray(2048)
    pvd[0:7] = b'\x01CD001\x01'
    pvd[8:72] = b' ' * 32 + b'BIG TEST'.ljust(32)
    pvd[80:88] = both(total)
    pvd[120:124] = struct.pack('<H', 1) + struct.pack('>H', 1)
    pvd[124:128] = struct.pack('<H', 1) + struct.pack('>H', 1)
    pvd[128:132] = struct.pack('<H', 2048) + struct.pack('>H', 2048)
    pvd[156:190] = record(b'\0', ROOT_AT, 2048, True)
    end = b'\xffCD001\x01'.ljust(2048, b'\0')
    return b''.join([bytes(16 * 2048), bytes(pvd), end, bytes(4096),
        folder(ROOT_AT, ROOT_AT, record(b'PSP_GAME', GAME_AT, 2048, True)),
        folder(GAME_AT, ROOT_AT, record(b'PARAM.SFO', SFO_AT, len(sfo), False), record(b'SYSDIR', SYS_AT, 2048, True)),
        folder(SYS_AT, GAME_AT, record(b'EBOOT.BIN', EBOOT_AT, len(eboot), False)),
        sfo.ljust((EBOOT_AT - SFO_AT) * 2048, b'\0'), eboot.ljust((total - EBOOT_AT) * 2048, b'\0')])

def copies():
    return sh('sh', '-c', f'ls -la /data/data/{PACKAGE}/files/content/*/ 2>/dev/null; true')

def copy_logs():
    return re.findall(r'(copy: .*|FDTEST .*|launch: .*|resume=\w+)', run('logcat', '-d', '-s', 'PocketMvp:I'))

def starts_copying(seconds=30):
    """By the log: the moving bar keeps UIAutomator from ever finding the window idle."""
    end = time.time() + seconds
    while time.time() < end:
        if 'copy: bar shown' in run('logcat', '-d', '-s', 'PocketMvp:I'):
            time.sleep(1)
            return
        time.sleep(.5)
    raise AssertionError('No bar')

def tap_cancel():
    """Where the window's Cancel is in portrait (seen on big-bar): UIAutomator can't read it, as above."""
    w, h = map(int, re.search(r'(\d+)x(\d+)', sh('wm', 'size')).groups())
    sh('input', 'tap', str(w * 335 // 414), str(h * 514 // 900))

def phase_big():
    global GAMES
    fixture = next(g for g in json.loads((Path(__file__).parent / 'core-smoke-roms.json').read_text()) if g['system'] == 'psp')
    pbp = urllib.request.urlopen(fixture['url'], timeout=60).read()
    assert hashlib.sha256(pbp).hexdigest() == fixture['sha256'], 'Fixture changed'
    big, none, later, real = 'psp/Big Test.iso', 'psp/No Room.iso', 'psp/Later Test.iso', 'psp/Real Bytes.iso'
    # Later Test: the faults after the first start, at a size a small host disk can copy again. BIG_REAL_BYTES=1: the copy's real speed.
    measure = os.environ.get('BIG_REAL_BYTES') == '1'
    GAMES = [[big, 'Big Test'], [none, 'No Room'], [later, 'Later Test']] + ([[real, 'Real Bytes']] if measure else [])
    PADDED.update({big: 1843, none: 5120, later: 300, real: 600})
    with tempfile.NamedTemporaryFile(suffix='.iso') as iso:
        iso.write(psp_iso(pbp))
        iso.flush()
        sh('mkdir', '-p', '/sdcard/Download/PocketMvp/psp')
        for source, _ in GAMES:
            run('push', iso.name, f'/sdcard/Download/PocketMvp/{source}')
    root()
    fresh()
    document = f'{ROOT}/{later}'

    # What Game check says on a fresh install.
    tap('Settings')
    for _ in range(12):
        if find('Game check') is not None:
            break
        sh('input', 'swipe', '600', '1800', '600', '900', '300')
        time.sleep(.7)
    tap('Game check')
    words = ''
    for _ in range(60):
        words = ' '.join(texts())
        if 'games ready' in words:
            break
        time.sleep(1)
    check('check: no 128 MB line', '128 MB' not in words, words[:300])
    tap("Can't start in this app yet", prefix=True)
    time.sleep(1)
    tap('No Room', prefix=True)
    time.sleep(1)
    words = ' '.join(texts())
    shot('big-game-check')
    check('check: only the game with no room, with its numbers', 'free on your phone' in words and 'Big Test' not in words, words[:400])
    restart()

    # The real speed: padding that is really on the storage, unlike the others' holes, nothing of it in the page cache.
    if measure:
        sh('dd', 'if=/dev/urandom', f'of={ROOT}/{real}', 'bs=1048576', 'seek=1', 'count=599', 'conv=notrunc')
        sh('sh', '-c', 'sync; echo 3 > /proc/sys/vm/drop_caches')
        run('logcat', '-c')
        choose('Real Bytes')
        play()
        in_game(120)
        logs = copy_logs()
        print('600 MiB of real bytes', logs, flush=True)
        (out / 'big-real-bytes.txt').write_text('\n'.join(logs) + '\n')
        time.sleep(2)
        quit_game()

    restart('--ei', 'slowCopy', '20')

    # Cancel: back on the shelf, nothing left behind.
    run('logcat', '-c')
    choose('Big Test')
    play()
    starts_copying()
    time.sleep(4)
    shot('big-bar')
    tap_cancel()
    time.sleep(3)
    check('cancel: no part, no copy', '.part' not in copies() and 'Big Test.iso' not in copies(), copies())
    check('cancel: no player', 'Pause' not in texts(), str(texts()[:8]))

    # Killed mid-copy: the part is never played and goes at the next start.
    run('logcat', '-c')
    play()
    starts_copying()
    time.sleep(3)
    sh('am', 'force-stop', PACKAGE)
    time.sleep(1)
    check('killed: a part stays for now', '.part' in copies(), copies())
    restart()
    sh('sh', '-c', 'sync; echo 3 > /proc/sys/vm/drop_caches')

    # First start, measured.
    run('logcat', '-c')
    choose('Big Test')
    began = time.time()
    play()
    in_game(400)
    waited = time.time() - began
    print(f'first start {waited:.1f} s', copy_logs(), flush=True)
    (out / 'big-first-start.txt').write_text(f'{waited:.1f} s\n' + '\n'.join(copy_logs()) + '\n')
    check('first start plays', True, f'{waited:.1f} s')
    check('kill: part gone', '.part' not in copies(), copies())
    time.sleep(6)
    shot('big-playing')
    quit_game()
    the_copy = lambda: next(l for l in copies().splitlines() if l.endswith('.iso'))
    before = the_copy()
    choose('Big Test')
    check('moment: the case stands open', status().startswith('Frozen · '), status())

    # Second start: no copy, the same file, back at the moment.
    run('logcat', '-c')
    began = time.time()
    play()
    in_game(60)
    logs = copy_logs()
    (out / 'big-second-start.txt').write_text(f'{time.time() - began:.1f} s\n' + '\n'.join(logs) + '\n')
    check('second start: no bar', not any('copy: window' in l for l in logs), str(logs))
    check('second start: back at the moment', any('resume=Resumed' in l for l in logs), str(logs))
    check('second start: same copy', before == the_copy(), copies())
    quit_game()

    # No room: the game that can't fit says so, the other's copy stays.
    run('logcat', '-c')
    choose('No Room')
    play()
    said = note_seen('free on your phone', 30)
    shot('big-no-room')
    check('no room: says the numbers', 'GB' in said or 'MB' in said, said)
    tap('OK')
    check('no room: nothing left, the other copy stays', '.part' not in copies() and 'Big Test.iso' in copies(), copies())

    # File gone: said, the copy kept, not played.
    played_game('Later Test')
    run('logcat', '-c')
    sh('mv', document, document + '.away')
    choose('Later Test')
    play()
    said = note_seen("isn't in its folder", 30)
    check('gone: said', bool(said), str(texts()[:8]))
    tap('OK')
    sh('mv', document + '.away', document)
    sh('touch', '-r', document, document)

    # Grant lost, folder chosen again: the same identity, no new copy (the rename above didn't change size or time).
    stamp = sh('sh', '-c', f'ls -la /data/data/{PACKAGE}/files/content/*/*.iso')
    sh('am', 'force-stop', PACKAGE)
    grants = '/data/system/urigrants.xml'
    sh('stop')
    sh('sed', '-i', f'/{PACKAGE}/d', grants)
    sh('start')
    for _ in range(60):
        if sh('getprop', 'sys.boot_completed', check=False).strip() == '1' and sh('pidof', 'system_server', check=False).strip():
            break
        time.sleep(2)
    time.sleep(25)
    sh('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
    time.sleep(5)
    shot('big-lost')
    check('lost: says what to do', any('Could not read' in t or 'Choose' in t for t in texts()), str(texts()[:8]))
    tap('Choose your ROM folder')
    time.sleep(1)
    pick_folder()
    tap('Not now')  # The folder chosen again offers the check.
    tap('Games')
    time.sleep(3)
    run('logcat', '-c')
    choose('Later Test')
    play()
    in_game(60)
    logs = copy_logs()
    check('lost, chosen again: no bar, the moment is back', not any('copy: window' in l for l in logs) and any('resume=Resumed' in l for l in logs), str(logs))
    check('lost, chosen again: same copy', stamp == sh('sh', '-c', f'ls -la /data/data/{PACKAGE}/files/content/*/*.iso'), copies())
    quit_game()

    # Changed between the samples, the size kept: copied again, and the moment isn't loaded for another file.
    time.sleep(1)
    sh('dd', 'if=/dev/urandom', f'of={document}', 'bs=4096', 'seek=115200', 'count=1', 'conv=notrunc')
    run('logcat', '-c')
    restart('--ei', 'slowCopy', '20')
    choose('Later Test')
    play()
    starts_copying()
    in_game(400)
    logs = copy_logs()
    (out / 'big-changed.txt').write_text('\n'.join(logs) + '\n')
    check('changed: copied again', any('copy: window' in l for l in logs), str(logs))
    check('changed: moment not loaded', not any('resume=Resumed' in l for l in logs), str(logs))
    states = sh('sh', '-c', f'ls /data/data/{PACKAGE}/files/states/*/')
    check('changed: the moment is in slot 1', re.search(r'\.pstate\s', states) is not None, states)
    time.sleep(2)
    quit_game()

try:
    for phase in phases:
        globals()['phase_' + phase]()
finally:
    run('emu', 'sensor', 'set', 'acceleration', '0:9.8:0', check=False)
    bad = [r for r in results if not r[1]]
    print(f'{len(results) - len(bad)} ok, {len(bad)} failed', flush=True)
    (out / 'results.json').write_text(json.dumps(results, indent=1))
sys.exit(1 if bad else 0)
