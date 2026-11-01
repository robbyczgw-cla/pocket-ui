#!/usr/bin/env python3
"""Settings → Game check on the X300 emulator, as a new user meets it: a fresh install, default settings, real folders through
the system picker, with the faults people have (a disc without its track, an empty file, a zip without a game, a game in the
wrong folder, games for emulator apps that aren't installed, a BIOS nothing here reads).

Usage: game-check-smoke.py ADB emulator-NNNN OUTPUT ROMS [--lang en|de] [--orientation portrait|landscape] [--phase faults,good,empty,lost]
ROMS holds legal homebrew: Libbet and the Magic Floor.gb, Attack on Voxelburg.gba, Thwaite.nes, Elevator Madness DX.sfc.
Clears the app's data. Emulator only: it refuses anything else. `lost` needs root (a Google APIs image) and restarts the emulator.
"""
import argparse
import io
import re
import struct
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

PACKAGE = 'dev.pocket.spike'
BASE = '/sdcard/Download/'
HERE = Path(__file__).resolve().parent
FORBIDDEN = ['SD card', 'SD-Karte', '/storage', 'roms/', 'Press ', 'Drück ', 'Web companion', 'handheld', 'Handheld', 'pocket-core']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('adb')
    parser.add_argument('serial')
    parser.add_argument('output', type=Path)
    parser.add_argument('roms', type=Path)
    parser.add_argument('--lang', default='en', choices=['en', 'de'])
    parser.add_argument('--orientation', default='portrait', choices=['portrait', 'landscape'])
    parser.add_argument('--phase', default='faults,good,empty,lost')
    parser.add_argument('--retroarch', help="the official RetroArch APK, for --phase retroarch: an emulator that is there is no fault")
    args = parser.parse_args()
    out = args.output
    out.mkdir(parents=True, exist_ok=True)
    translations = {}
    for line in (HERE.parents[1] / 'assets/i18n/de.tsv').read_text().splitlines():
        if '\t' in line:
            source, translated = line.split('\t', 1)
            translations[source] = translated

    def tr(label):
        return translations.get(label, label) if args.lang == 'de' else label

    def run(*cmd, text=True, timeout=60):
        return subprocess.check_output([args.adb, '-s', args.serial, *cmd], text=text, timeout=timeout)

    def shell(*cmd):
        return run('shell', *cmd)

    assert args.serial.startswith('emulator-'), 'Emulator only'
    assert shell('getprop', 'ro.kernel.qemu').strip() == '1', 'Refusing a real device'

    def tree():
        # A busy page can keep it from answering for a moment.
        for _ in range(8):
            raw = run('exec-out', 'uiautomator', 'dump', '/dev/tty')
            start, end = raw.find('<?xml'), raw.find('</hierarchy>')
            if start >= 0 and end >= start:
                break
            time.sleep(1)
        assert start >= 0 and end >= start, 'UIAutomator returned no hierarchy'
        return list(ET.fromstring(raw[start:end + len('</hierarchy>')]).iter('node'))

    def labels():
        return [value for n in tree() for value in (n.get('text'), n.get('content-desc')) if value]

    def find(label, prefix=False, tries=8):
        for _ in range(tries):
            matches = [n for n in tree() if n.get('enabled') == 'true' and any(v.startswith(label) if prefix else v == label for v in (n.get('text', ''), n.get('content-desc', '')))]
            if matches:
                return max(matches, key=lambda n: int(re.findall(r'-?\d+', n.get('bounds', '0 0 0 0'))[1]))
            time.sleep(.35)
        return None

    def centre(node):
        x1, y1, x2, y2 = map(int, re.findall(r'-?\d+', node.get('bounds', '')))
        assert x2 > x1 and y2 > y1, 'Empty target'
        return (x1 + x2) // 2, (y1 + y2) // 2

    def tap(label, prefix=False, translate=True):
        wanted = tr(label) if translate else label
        node = find(wanted, prefix)
        assert node is not None, f'Missing live target: {wanted!r}; have {labels()!r}'
        x, y = centre(node)
        shell('input', 'tap', str(x), str(y))
        time.sleep(.7)

    def key(code):
        shell('input', 'keyevent', str(code))
        time.sleep(.7)

    def shot(name):
        time.sleep(.5)
        png = run('exec-out', 'screencap', '-p', text=False)
        width, height = struct.unpack('>II', png[16:24])
        assert (width < height) == (args.orientation == 'portrait'), f'Wrong orientation {width}x{height} for {name}'
        (out / f'{args.lang}-{args.orientation}-{name}.png').write_bytes(png)

    def clean_words():
        for value in labels():
            for word in FORBIDDEN:
                assert word not in value, f'Handheld wording {word!r} on a phone: {value!r}'

    def see(*wanted, tries=30):
        """Every text is on the page; they may arrive a moment later."""
        for _ in range(tries):
            have = labels()
            if all(any(w in v for v in have) for w in wanted):
                clean_words()
                return have
            time.sleep(.5)
        raise AssertionError(f'Not on the page: {wanted!r}; have {have!r}')

    def settings_row(label):
        """Taps a Settings row, scrolling to it."""
        tap('Settings')
        for _ in range(12):
            node = find(tr(label), prefix=True, tries=1)
            if node is not None:
                x, y = centre(node)
                shell('input', 'tap', str(x), str(y))
                time.sleep(.8)
                return
            size = shell('wm', 'size').split()[-1].split('x')
            w, h = int(size[0]), int(size[1])
            if args.orientation == 'landscape':
                w, h = max(w, h), min(w, h)
            else:
                w, h = min(w, h), max(w, h)
            shell('input', 'swipe', str(w // 2), str(h * 6 // 10), str(w // 2), str(h * 3 // 10), '300')
            time.sleep(.5)
        raise AssertionError(f'No Settings row {label!r}')

    def pick(folder):
        with tempfile.TemporaryDirectory() as temp:
            subprocess.check_call([sys.executable, str(HERE / 'select-demo-folder.py'), args.adb, args.serial, temp, folder], timeout=120)
        time.sleep(1.5)

    def fresh():
        """A new install: default settings, the first start skipped."""
        shell('am', 'force-stop', PACKAGE)
        assert 'Success' in shell('pm', 'clear', PACKAGE)
        run('emu', 'sensor', 'set', 'acceleration', '0:9.8:0' if args.orientation == 'portrait' else '9.8:0:0')
        shell('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
        time.sleep(2)
        # Android's own note about full screen, the first time.
        node = find('Got it', tries=3)
        if node is not None:
            x, y = centre(node)
            shell('input', 'tap', str(x), str(y))
            time.sleep(.6)
        for _ in range(6):
            node = find('Skip', tries=6)
            assert node is not None, f'No Skip; have {labels()!r}'
            x, y = centre(node)
            shell('input', 'tap', str(x), str(y))
            time.sleep(.6)
        node = find('Continue')
        x, y = centre(node)
        shell('input', 'tap', str(x), str(y))
        time.sleep(1)
        if args.lang == 'de':
            settings_row_en('Language')
            see('Deutsch')

    def settings_row_en(label):
        # The language row, before the page speaks German.
        node = find('Settings')
        x, y = centre(node)
        shell('input', 'tap', str(x), str(y))
        time.sleep(.8)
        for _ in range(12):
            node = find(label, prefix=True, tries=1)
            if node is not None:
                x, y = centre(node)
                shell('input', 'tap', str(x), str(y))
                time.sleep(.8)
                return
            shell('input', 'swipe', '300', '900', '300', '500', '300')
            time.sleep(.5)
        raise AssertionError(label)

    def fixture():
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'GameCheckRoms'

            def put(rel, data=b''):
                path = root / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)

            libbet, voxel = (args.roms / 'Libbet and the Magic Floor.gb').read_bytes(), (args.roms / 'Attack on Voxelburg.gba').read_bytes()
            thwaite, elevator = (args.roms / 'Thwaite.nes').read_bytes(), (args.roms / 'Elevator Madness DX.sfc').read_bytes()
            readme = io.BytesIO()
            with zipfile.ZipFile(readme, 'w') as z:
                z.writestr('readme.txt', 'no game in here')
            zipped = io.BytesIO()
            with zipfile.ZipFile(zipped, 'w') as z:
                z.writestr('Thwaite.nes', thwaite)
            put('gb/Libbet and the Magic Floor.gb', libbet)
            put('gb/Libbet and the Magic Floor (1).gb', libbet)
            put('gba/Attack on Voxelburg.gba', voxel)
            put('gba/Only Readme.zip', readme.getvalue())
            put('nes/Thwaite.nes', thwaite)
            put('nes/Thwaite.zip', zipped.getvalue())
            put('nes/Stopped Copy.nes')
            put('snes/Elevator Madness DX.sfc', elevator)
            put('snes/Wrong Place.gba', voxel)
            put('snes/Half Copy.sfc.part', b'x' * 100)
            put('snes/Typo Name.sfcc', b'x' * 100)
            put('psx/Broken Disc.cue', b'FILE "Broken Disc.bin" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n')
            put('psx/Whole Disc.cue', b'FILE "Whole Disc.bin" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n')
            put('psx/Whole Disc.bin', b'\0' * 2352)
            put('segacd/Needs Bios.chd', b'\0' * 2352)
            put('pcengine/Cd Game.chd', b'\0' * 2352)
            put('bios/scph5501.bin', b'\0' * 16)
            put('Odd Folder/Mystery Disc.iso', b'\0' * 2352)
            good = Path(temp) / 'GameCheckGood'
            for rel, data in (('gb/Libbet and the Magic Floor.gb', libbet), ('nes/Thwaite.nes', thwaite), ('snes/Elevator Madness DX.sfc', elevator)):
                (good / rel).parent.mkdir(parents=True, exist_ok=True)
                (good / rel).write_bytes(data)
            nothing = Path(temp) / 'GameCheckEmpty'
            nothing.mkdir()
            (nothing / 'notes.txt').write_text('no games here')
            shell('rm', '-rf', BASE + 'GameCheckRoms', BASE + 'GameCheckGood', BASE + 'GameCheckEmpty')
            for folder in (root, good, nothing):
                run('push', str(folder), BASE, timeout=120)
        # A game over the in-app player's limit, sparse: nothing has to be copied.
        shell('mkdir', '-p', BASE + 'GameCheckRoms/psp')
        shell(f"truncate -s 200M '{BASE}GameCheckRoms/psp/Big Game.iso'")
        shell('am', 'broadcast', '-a', 'android.intent.action.MEDIA_SCANNER_SCAN_FILE', '-d', 'file://' + BASE)

    def faults():
        fresh()
        # No folder yet: something to read and the way on.
        settings_row('Game check')
        see(tr('No folder yet'), tr('Choose your ROM folder'))
        shot('01-no-folder')
        # The real picker; chosen from this page, it looks at once.
        tap('Choose your ROM folder')
        pick('GameCheckRoms')
        have = see(tr('{n} of {n2} games ready').replace('{n}', '6').replace('{n2}', '14'))
        print('groups:', [v for v in have if '·' in v])
        shot('04-groups')
        expected = ['Discs missing a file', 'Empty files', "Zips that won't start", 'Same game twice', 'Wrong folder or file type', 'Emulator apps to install', "Can't start in this app yet", 'Tips']
        for name in expected:
            # The list may be longer than the page: slowly down, then back up (a quick swipe would fling past it).
            mid = 800 if args.orientation == 'portrait' else 560
            for step in range(24):
                if find(tr(name), prefix=True, tries=1) is not None:
                    break
                up = step >= 10
                shell('input', 'swipe', '300', str(mid + 60 if not up else mid - 60), '300', str(mid - 60 if not up else mid + 60), '700')
                time.sleep(.3)
            tap(name, prefix=True)
            items = see(tr('Back'))
            print(name, '->', [v for v in items if v not in (tr('Back'),)][:8])
            slug = re.sub(r'[^a-z]+', '-', name.lower()).strip('-')
            shot('05-' + slug)
            # The first game: what is wrong, what to do. The tab bar's labels aren't rows.
            quiet = {tr(t) for t in ('Games', 'Favourites', 'Search', 'Curator', 'Settings', 'Back', name)}
            rows = [n for n in tree() if n.get('text') and n.get('text') not in quiet]
            x, y = centre(rows[0])
            shell('input', 'tap', str(x), str(y))
            time.sleep(.8)
            details = see(tr('Back'))
            print('  detail:', [v for v in details if v != tr('Back')][:4])
            shot('06-' + slug)
            tap('Back')
            tap('Back')
        # A controller walks the same page: Down, A, B, X (keyboard keycodes of a gamepad's buttons).
        key(20)
        shot('07-controller')
        see(tr('Check again'))
        key(96)
        key(97)
        key(99)
        see(tr('{n} of {n2} games ready').replace('{n}', '6').replace('{n2}', '14'))
        key(4)
        # Settings says what the last check found.
        have = see(tr('Game check'))
        verdict = next(v for v in have if v.startswith(tr('Game check')))
        assert '·' in verdict, verdict
        print('settings row:', verdict)
        shot('08-settings-row')
        # The games that can start do: the shelf's first ready game goes to the player.
        tap('Games')

    def good():
        fresh()
        settings_row('Choose your ROM folder')
        pick('GameCheckGood')
        # Chosen from Settings, with new games: the offer. Not now goes back, the row looks again.
        see(tr('{n} new games').replace('{n}', '3'), tr('Check'))
        shot('02-offer')
        tap('Not now')
        see(tr('Game check'))
        settings_row('Choose your ROM folder')
        pick('GameCheckGood')
        settings_row('Game check')
        see(tr('All good'), tr('{n} games ready').replace('{n}', '3'))
        shot('10-all-good')
        # What it calls ready starts: the shelf's game goes to the in-app player.
        tap('Games')
        tap('Play')
        for _ in range(40):
            if any('ResumedActivity' in l and 'PlayerActivity' in l for l in shell('dumpsys', 'activity', 'activities').splitlines()):
                break
            time.sleep(.5)
        else:
            raise AssertionError('The player did not open')
        time.sleep(3)
        shot('10-player')
        shell('am', 'force-stop', PACKAGE)

    def empty():
        fresh()
        settings_row('Choose your ROM folder')
        pick('GameCheckEmpty')
        settings_row('Game check')
        see(tr('No games found'), tr('Choose your ROM folder'))
        shot('11-no-games')

    def retroarch():
        run('install', '-r', args.retroarch, timeout=300)
        fresh()
        settings_row('Game check')
        tap('Choose your ROM folder')
        pick('GameCheckRoms')
        see(tr('{n} of {n2} games ready').replace('{n}', '7').replace('{n2}', '14'))
        shot('18-retroarch-installed')
        tap('Emulator apps to install', prefix=True)
        have = see('PlayStation')
        assert not any(v.startswith('Mega CD') for v in have), have
        shot('19-only-duckstation-left')
        tap('Back')
        run('uninstall', 'com.retroarch.aarch64')

    def big():
        """A big library through the real provider: progress shows, and the log says how long it took."""
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'GameCheckBig'
            zipped = io.BytesIO()
            with zipfile.ZipFile(zipped, 'w') as z:
                z.writestr('game.gba', b'x' * 1024)
            for system, ext, count in (('gba', 'zip', 2500), ('snes', 'sfc', 1500), ('nes', 'nes', 1000)):
                (root / system).mkdir(parents=True)
                for i in range(count):
                    (root / system / f'Game Number {i} (Europe).{ext}').write_bytes(zipped.getvalue() if ext == 'zip' else b'x' * 1024)
            shell('rm', '-rf', BASE + 'GameCheckBig')
            run('push', str(root), BASE, timeout=600)
        fresh()
        settings_row('Choose your ROM folder')
        pick('GameCheckBig')
        see(tr('Check'), tries=120)
        tap('Check')
        # Past the listing: the bar and the count.
        time.sleep(8)
        shot('16-running')
        see(tr('All good'), tries=240)
        shot('17-big-all-good')
        shell('rm', '-rf', BASE + 'GameCheckBig')

    def lost():
        """A grant Android lost: the file with the grants goes (root, emulator only), and the emulator restarts."""
        fresh()
        settings_row('Choose your ROM folder')
        pick('GameCheckGood')
        tap('Not now')
        run('root')
        time.sleep(2)
        run('wait-for-device')
        shell('rm', '-f', '/data/system/urigrants.xml')
        shell('setprop', 'ctl.restart', 'zygote')
        for _ in range(90):
            time.sleep(2)
            try:
                if shell('getprop', 'sys.boot_completed').strip() == '1' and 'mCurrentFocus' in shell('dumpsys', 'window', 'windows'):
                    break
            except subprocess.SubprocessError:
                pass
        time.sleep(4)
        run('unroot')
        time.sleep(3)
        run('wait-for-device')
        run('emu', 'sensor', 'set', 'acceleration', '0:9.8:0' if args.orientation == 'portrait' else '9.8:0:0')
        shell('input', 'keyevent', '82')
        shell('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
        time.sleep(3)
        settings_row('Game check')
        see(tr("Can't read your folder"), tr('Choose it again'))
        shot('12-lost')
        tap('Choose it again')
        pick('GameCheckGood')
        see(tr('All good'), tr('{n} games ready').replace('{n}', '3'))
        shot('13-lost-chosen-again')
        # A folder that is gone: it says so, and it can be forgotten.
        shell('mv', BASE + 'GameCheckGood', BASE + 'GameCheckGoodAway')
        tap('Check again')
        see(tr("Can't read your folder"), tr('Forget this folder'))
        shot('14-gone')
        tap('Forget this folder')
        see(tr('No folder yet'))
        shot('15-forgotten')
        shell('mv', BASE + 'GameCheckGoodAway', BASE + 'GameCheckGood')

    fixture()
    phases = {'faults': faults, 'good': good, 'empty': empty, 'lost': lost, 'big': big, 'retroarch': retroarch}
    for name in args.phase.split(','):
        print('==', name, args.lang, args.orientation, flush=True)
        phases[name]()
    print('OK', args.lang, args.orientation)


if __name__ == '__main__':
    main()
