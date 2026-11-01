#!/usr/bin/env python3
"""A game's start on the X300 emulator: the case going into its console, on a fresh install with its default settings.

Usage: insert-smoke.py ADB emulator-NNNN OUTPUT STEP... (emulator only; ffmpeg for the strips)
Steps:
  fresh APK ROMS   uninstall, install, ROMS/<system>/* into Download/PocketMvp, first start with Skip on every step but the folder
  run NAME         Play on the game NAME: a strip of the frames (NAME.jpg), asserts the player came with Rust's log
  rehearse NAME    the same without a game (`insertRehearsal`): any system, no core or emulator needed
  skip NAME        Play, a tap 0.5 s later: how long until the player's first picture
  nogame NAME      a system with no core and no emulator: a message at once, the page back
  home NAME        Home 0.4 s after Play: the player doesn't come up later
  homecopy NAME    Home during a copy held slow (`slowCopy`), then the same game again: no player from the first, the second comes
  setting LABEL VALUE      a row of Settings to VALUE
  rotate portrait|landscape
  sync NAME        rehearse with the audio diagnostic and print insert-sync.py's numbers (sync-warm: right after a tap)
Every run leaves NAME.jpg (≤1280 px, a frame each ~130 ms from the press) and NAME-key.jpg (the moment the medium is in).
"""
import json
import re
import shutil
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

PACKAGE = 'dev.pocket.spike'
NEIGHBOURS = ('Sample Card', 'Sample CD', 'Sample DVD')
adb, serial, destination = sys.argv[1:4]
steps = sys.argv[4:]
out = Path(destination)
out.mkdir(parents=True, exist_ok=True)
ROOT = Path(__file__).resolve().parents[2]
FFMPEG = shutil.which('ffmpeg') or '/opt/homebrew/bin/ffmpeg'


def run(*args, timeout=60):
    return subprocess.check_output([adb, '-s', serial, *args], timeout=timeout)


def sh(*args, timeout=60):
    return run('shell', *args, timeout=timeout).decode(errors='replace')


assert serial.startswith('emulator-') and sh('getprop', 'ro.kernel.qemu').strip() == '1', 'emulator only'


def nodes():
    for _ in range(6):
        try:
            raw = run('exec-out', 'uiautomator', 'dump', '/dev/tty').decode()
            return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>') + 12]).iter('node'))
        except (ValueError, ET.ParseError, subprocess.SubprocessError):
            time.sleep(.5)
    raise AssertionError('no UI tree')


def find(label, prefix=False):
    for n in reversed(nodes()):
        text = n.get('text') or ''
        if n.get('enabled') == 'true' and (text.startswith(label) if prefix else text == label):
            return n
    return None


def touch(n):
    x1, y1, x2, y2 = map(int, re.findall(r'-?\d+', n.get('bounds')))
    sh('input', 'tap', str((x1 + x2) // 2), str((y1 + y2) // 2))


def tap(label, prefix=False):
    for _ in range(12):
        n = find(label, prefix)
        if n is not None:
            touch(n)
            time.sleep(.5)
            return
        # Android's own hint the first time a window is full screen.
        got_it = find('Got it')
        if got_it is not None:
            touch(got_it)
        time.sleep(.4)
    raise AssertionError('missing ' + label + ': ' + repr(sorted({n.get('text') for n in nodes() if n.get('text')})))


def start(*extras):
    sh('am', 'force-stop', PACKAGE)
    sh('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity', *extras)
    time.sleep(2.5)


def fresh(apk, roms):
    sh('am', 'force-stop', PACKAGE)
    subprocess.call([adb, '-s', serial, 'uninstall', PACKAGE], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    run('install', apk, timeout=180)
    sh('rm', '-rf', '/sdcard/Download/PocketMvp')
    for path in sorted(Path(roms).glob('*/*')):
        folder = '/sdcard/Download/PocketMvp/' + path.parent.name
        sh('mkdir', '-p', folder)
        run('push', str(path), folder + '/', timeout=120)
    start()
    # The first start, as a new player goes through it: only the folder is chosen, every other step keeps its default.
    for step in range(1, 7):
        for _ in range(16):
            if find('Step %d of 7' % step) is not None:
                break
            got_it = find('Got it')
            if got_it is not None:
                touch(got_it)
            time.sleep(.4)
        if step == 3:
            tap('Choose your ROM folder')
            time.sleep(1)
            subprocess.check_call([sys.executable, str(ROOT / 'scripts/android/select-demo-folder.py'), adb, serial, str(out)])
            time.sleep(1.5)
            tap('Continue')
        else:
            tap('Skip')
        time.sleep(.3)
    tap('Continue')
    time.sleep(3)
    prefs = json.loads(sh('cat', '/data/data/%s/files/launcher.json' % PACKAGE))
    print('defaults:', {k: prefs.get(k) for k in ('theme', 'cozy', 'motion', 'muted', 'light')}, 'playful', prefs.get('extras', {}).get('playful_animations'), flush=True)


def choose(name):
    """The game by its name on the page; the page says it is selected before anything goes on."""
    sh('input', 'keyevent', *(['21'] * 40))
    time.sleep(1)
    for _ in range(45):
        n = next((n for n in nodes() if (n.get('text') or '').casefold() == name.casefold()), None)
        if n is not None:
            for _ in range(5):
                if n.get('selected') == 'true':
                    return
                touch(n)
                time.sleep(.6)
                n = next(m for m in nodes() if (m.get('text') or '').casefold() == name.casefold())
            raise AssertionError('not selected: ' + name)
        sh('input', 'keyevent', '22')
        time.sleep(.3)
    raise AssertionError('missing game ' + name)


def size():
    w, h = re.findall(r'\d+', sh('wm', 'size').splitlines()[-1])
    land = 'ROTATION_90' in sh('dumpsys', 'input') or 'ROTATION_270' in sh('dumpsys', 'input')
    return (int(h), int(w)) if land else (int(w), int(h))


def record(name, action, seconds=7):
    """Screen video around `action`; the strip counts from the first frame that differs."""
    sh('rm', '-f', '/sdcard/insert.mp4')
    w, h = size()
    # Half the screen: the emulator's encoder refuses its whole size (an empty file).
    rec = subprocess.Popen([adb, '-s', serial, 'shell', 'screenrecord', '--bit-rate', '16000000', '--time-limit', str(seconds), '--size', '%dx%d' % (w // 2 // 2 * 2, h // 2 // 2 * 2), '/sdcard/insert.mp4'])
    time.sleep(1.2)
    sh('logcat', '-c')
    action()
    rec.wait(timeout=seconds + 15)
    time.sleep(1)
    video = out / (name + '.mp4')
    run('pull', '/sdcard/insert.mp4', str(video))
    log = sh('logcat', '-d', '-v', 'monotonic', '-s', 'PocketMvp:I', 'ActivityTaskManager:I', timeout=30)
    (out / (name + '.log')).write_text(log)
    strip(video, name, (w, h))
    return log


def frames_gray(video, w=76):
    """Small grey frames at 50 per second."""
    h = 0
    probe = subprocess.check_output([FFMPEG, '-v', 'error', '-i', str(video), '-vf', 'fps=50,scale=%d:-2,format=gray' % w, '-f', 'rawvideo', '-'])
    info = subprocess.run([FFMPEG, '-v', 'error', '-i', str(video), '-vf', 'scale=%d:-2' % w, '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'gray', '-'], capture_output=True)
    h = len(info.stdout) // w
    return [probe[i:i + w * h] for i in range(0, len(probe), w * h)]


def strip(video, name, wh):
    gray = frames_gray(video)
    def diff(a, b):
        return sum(1 for x, y in zip(a, b) if abs(x - y) > 12)
    start_at = next((i for i in range(1, len(gray)) if diff(gray[0], gray[i]) > 12), 0)
    # Before the press only the press' ripple can differ: from here on it is the start's own.
    t0 = start_at / 50.0
    print(name, 'first change at %.2f s of the video' % t0, flush=True)
    # A column of frames each 130 ms over 2.4 s, the screen's whole picture small.
    cols = 9 if wh[0] < wh[1] else 5
    rows = 2 if wh[0] < wh[1] else 3
    tile_w = 1280 // cols
    pts = 't0'
    n = cols * rows
    every = 2.6 / n
    vf = "select='gte(t,%.3f)',fps=%.3f,scale=%d:-2,tile=%dx%d:padding=0" % (t0, 1 / every, tile_w, cols, rows)
    subprocess.check_call([FFMPEG, '-y', '-v', 'error', '-i', str(video), '-vf', vf, '-frames:v', '1', '-q:v', '5', str(out / (name + '.jpg'))])
    # The moment after it is in, large enough to read.
    subprocess.check_call([FFMPEG, '-y', '-v', 'error', '-ss', '%.3f' % (t0 + 1.15), '-i', str(video), '-vf', 'scale=%s' % ('720:-2' if wh[0] < wh[1] else '1280:-2'), '-frames:v', '1', '-q:v', '6', str(out / (name + '-key.jpg'))])
    video_brightness(gray, name, start_at)


def video_brightness(gray, name, start_at):
    """A white flash would show as a frame much brighter than both neighbours after the fade."""
    mean = [sum(f) / max(1, len(f)) for f in gray]
    worst = max((mean[i] for i in range(start_at, len(mean))), default=0)
    print(name, 'brightest mean after the press: %.0f; mean over time (each 100 ms): %s' % (worst, ' '.join('%.0f' % m for m in mean[start_at::5][:40])), flush=True)


def press_play():
    tap('Play')


def do_run(name):
    choose(name)
    log = record(re.sub(r'\W+', '_', name), press_play)
    assert 'insert: ' in log, 'no start: ' + name
    assert 'PlayerActivity' in log, 'no player for ' + name
    print(name, 'frames', log.count('insert-frame'), flush=True)
    time.sleep(.5)
    sh('input', 'keyevent', '4')
    time.sleep(1)
    tap('Quit')
    time.sleep(1.5)


def do_rehearse(name):
    choose(name)
    log = record(re.sub(r'\W+', '_', name), press_play, 5)
    assert 'insert: ' in log, 'no start: ' + name
    time.sleep(.5)


def do_skip(name):
    choose(name)
    def act():
        press_play()
        time.sleep(.5)
        sh('input', 'tap', '600', '1300')
    log = record(re.sub(r'\W+', '_', name) + '-skip', act)
    (out / 'skip.log').write_text(log)
    assert 'insert: skipped' in log, 'not skipped'
    assert 'PlayerActivity' in log, 'no player'
    stamp = lambda word: float(re.search(r'%s at ([\d.]+)' % word, log).group(1))
    print(name, 'skip: tapped %.0f ms after the start' % (stamp('skipped') - stamp('start cozy=true')), flush=True)
    sh('input', 'keyevent', '4')
    time.sleep(1)
    tap('Quit')
    time.sleep(1.5)


def do_nogame(name):
    """No core and no emulator app for this system: the player's told why at once and the page is back."""
    choose(name)
    sh('logcat', '-c')
    press_play()
    for _ in range(12):
        if find("Emulator couldn't start") is not None:
            break
        time.sleep(.4)
    assert find("Emulator couldn't start") is not None, 'no message for ' + name
    log = sh('logcat', '-d', '-v', 'monotonic', '-s', 'PocketMvp:*', timeout=30)
    print(name, 'no emulator:', re.findall(r"([\d.]+) .*(?:insert: \S+ start|Emulator couldn't start)", log), flush=True)
    png = out / (re.sub(r'\W+', '_', name) + '-nogame.png')
    png.write_bytes(run('exec-out', 'screencap', '-p'))
    tap('OK')
    time.sleep(.5)
    assert find('Play') is not None, 'page not back'


def do_home(name):
    """Home while the case is on its way: nothing comes up later."""
    choose(name)
    sh('logcat', '-c')
    press_play()
    time.sleep(.4)
    sh('input', 'keyevent', '3')
    time.sleep(3)
    log = sh('logcat', '-d', '-s', 'ActivityTaskManager:I', timeout=30)
    assert 'PlayerActivity' not in log, 'the player came after Home'
    sh('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
    time.sleep(1.5)
    assert find('Play') is not None, 'page not back'


def do_homecopy(name):
    """Home while the game is still being copied: the worker stops, and a new start of the same game isn't in its way."""
    start('--ei', 'slowCopy', '400')
    choose(name)
    sh('logcat', '-c')
    press_play()
    time.sleep(4.5)  # Past the fade, so the start is released and only the copy holds it.
    sh('input', 'keyevent', '3')
    time.sleep(6)
    log = sh('logcat', '-d', '-s', 'ActivityTaskManager:I', 'PocketMvp:I', timeout=30)
    assert 'PlayerActivity' not in log, 'the player came after Home'
    assert 'cancelled start' in log, 'the worker went on: ' + log[-800:]
    sh('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
    for _ in range(20):
        if find('Play') is not None:
            break
        time.sleep(.5)
    assert find('Play') is not None, 'page not back: ' + repr(sorted({n.get('text') for n in nodes() if n.get('text')}))
    # The same process, as a player comes back: the cancelled worker's part isn't in this one's way.
    choose(name)
    sh('logcat', '-c')
    press_play()
    for _ in range(30):
        time.sleep(1)
        if 'PlayerActivity' in sh('logcat', '-d', '-s', 'ActivityTaskManager:I', timeout=30):
            return
    raise AssertionError('the second start did not come')


def do_setting(label, value):
    tap('Settings')
    for _ in range(8):
        n = find(label + ' ·', True)
        if n is None:
            sh('input', 'swipe', '600', '1800', '600', '900', '200')
            time.sleep(.5)
            continue
        if (n.get('text') or '').endswith(value):
            break
        touch(n)
        time.sleep(.5)
    assert find(label + ' · ' + value) is not None, label + value
    tap('Games')
    time.sleep(1)


def do_sync(name, warm=False):
    """Cold: the output has closed since the last sound. Warm: Play right after the tap that chose the game, as a player does."""
    start('--ez', 'diagnosticAudio', 'true', '--ez', 'insertRehearsal', 'true')
    sh('rm', '-rf', '/data/data/%s/files/audio-diagnostics' % PACKAGE)
    choose(name)
    if not warm:
        time.sleep(6)
    sh('logcat', '-c')
    if warm:
        # A tap on a neighbour and back, then Play at once: the bounds come first, a tree takes longer than the output stays open.
        def centre(n):
            x1, y1, x2, y2 = map(int, re.findall(r'-?\d+', n.get('bounds')))
            return str((x1 + x2) // 2), str((y1 + y2) // 2)
        here = next(n for n in nodes() if (n.get('text') or '').casefold() == name.casefold() and n.get('selected') == 'true')
        other = next(n for n in nodes() if n.get('text') in NEIGHBOURS and n.get('text') != here.get('text'))
        sh('input', 'tap', *centre(other))
        time.sleep(.3)
        sh('input', 'tap', *centre(here))
        time.sleep(.3)
        play = centre(find('Play'))
        sh('input', 'tap', *centre(other))
        time.sleep(.2)
        sh('input', 'tap', *centre(here))
        time.sleep(.2)
        sh('logcat', '-c')
        sh('input', 'tap', *play)
    else:
        press_play()
    time.sleep(7)
    log = sh('logcat', '-d', '-v', 'monotonic', '-s', 'PocketMvp:I', timeout=30)
    tag = re.sub(r'\W+', '_', name)
    (out / (tag + '-sync.log')).write_text(log)
    remote = '/data/data/%s/files/audio-diagnostics' % PACKAGE
    target = out / (tag + '-diag')
    shutil.rmtree(target, ignore_errors=True)
    target.mkdir()
    for f in sh('ls', remote).split():
        run('pull', remote + '/' + f, str(target / f))
    subprocess.check_call([sys.executable, str(ROOT / 'scripts/android/insert-sync.py'), str(out / (tag + '-sync.log')), str(target)])


i = 0
sh('settings', 'put', 'system', 'accelerometer_rotation', '1')
while i < len(steps):
    step = steps[i]
    if step == 'fresh':
        fresh(steps[i + 1], steps[i + 2])
        i += 3
    elif step == 'restart':
        start()
        i += 1
    elif step == 'rehearsal':
        start('--ez', 'insertRehearsal', 'true')
        i += 1
    elif step == 'run':
        do_run(steps[i + 1])
        i += 2
    elif step == 'rehearse':
        do_rehearse(steps[i + 1])
        i += 2
    elif step == 'skip':
        do_skip(steps[i + 1])
        i += 2
    elif step == 'nogame':
        do_nogame(steps[i + 1])
        i += 2
    elif step == 'homecopy':
        do_homecopy(steps[i + 1])
        i += 2
    elif step == 'home':
        do_home(steps[i + 1])
        i += 2
    elif step == 'setting':
        do_setting(steps[i + 1], steps[i + 2])
        i += 3
    elif step == 'rotate':
        run('emu', 'sensor', 'set', 'acceleration', '0:9.8:0' if steps[i + 1] == 'portrait' else '9.8:0:0')
        time.sleep(2)
        i += 2
    elif step == 'sync':
        do_sync(steps[i + 1])
        i += 2
    elif step == 'sync-warm':
        do_sync(steps[i + 1], True)
        i += 2
    else:
        raise SystemExit('unknown step ' + step)
    print('ok', step, flush=True)
