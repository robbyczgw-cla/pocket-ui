#!/usr/bin/env python3
"""Capture every main launcher screen of the phone layout on our X300 emulator, using live semantic bounds.

Usage: phone-design-smoke.py ADB emulator-5582 OUTPUT [--lang en|de] [--orientation portrait|landscape]
Needs a finished first start with Download/PocketMvp chosen (onboarding-smoke.py leaves that state). Keeps the app's
data, but changes its look, language and favourites. Emulator only; no hardware targets are accepted.
Screens: Games in every look, a selected game, its card (long press) and info page, Favourites empty and filled,
Search (field, keyboard, results), Curator, a facet, Settings, and the controller hints after a key press.
--parity clears the app and goes through the first start: one system stays the plain shelves; several get a shelf each,
Recent games and Favourites on top, along and across by touch and keys; the card's Stickers sheet, kept over a restart.
"""
import argparse
import re
import subprocess
import struct
import time
import xml.etree.ElementTree as ET
from pathlib import Path

PACKAGE = 'xyz.pocketui.app'
LOOKS = ['Shelf', 'Split', 'Cabinet']
SYSTEMS = ['Recent games', 'Favourites', 'Game Boy', 'Game Boy Advance', 'Game Boy Color', 'Nintendo Entertainment System', 'Super Nintendo',
           'Atari 2600', 'Atari Lynx', 'Master System', 'Mega Drive', 'Neo Geo Pocket', 'PlayStation Portable', 'Game Gear', 'PC Engine']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('adb')
    parser.add_argument('serial')
    parser.add_argument('output', type=Path)
    parser.add_argument('--lang', default='en', choices=['en', 'de'])
    parser.add_argument('--orientation', default='portrait', choices=['portrait', 'landscape'])
    parser.add_argument('--looks', default=','.join(LOOKS))
    parser.add_argument('--parity', action='store_true', help='clear the app, go through the first start, check the shelves and stickers; needs Download/PocketMvp (5 systems) and Download/PocketOne (one system)')
    parser.add_argument('--empty', action='store_true', help='clear the app, skip the first start, capture every tab without games')
    args = parser.parse_args()
    out = args.output
    out.mkdir(parents=True, exist_ok=True)
    translations = {}
    for line in (Path(__file__).resolve().parents[2] / 'assets/i18n/de.tsv').read_text().splitlines():
        if '\t' in line:
            source, translated = line.split('\t', 1)
            translations[source] = translated
    lang = 'en'

    def tr(label):
        return translations.get(label, label) if lang == 'de' else label

    def run(*cmd, text=True):
        return subprocess.check_output([args.adb, '-s', args.serial, *cmd], text=text, timeout=30)

    def shell(*cmd):
        return run('shell', *cmd)

    assert args.serial.startswith('emulator-'), 'Emulator only'
    assert shell('getprop', 'ro.kernel.qemu').strip() == '1', 'Refusing a real device'

    def tree():
        raw = run('exec-out', 'uiautomator', 'dump', '/dev/tty')
        start, end = raw.find('<?xml'), raw.find('</hierarchy>')
        assert start >= 0 and end >= start, 'UIAutomator returned no hierarchy'
        return list(ET.fromstring(raw[start:end + len('</hierarchy>')]).iter('node'))

    def labels():
        return [value for n in tree() for value in (n.get('text'), n.get('content-desc')) if value]

    def find(label, prefix=False):
        for _ in range(8):
            matches = [n for n in tree() if n.get('enabled') == 'true'
                       and any(v.startswith(label) if prefix else v == label for v in (n.get('text', ''), n.get('content-desc', '')))]
            if matches:
                # A tab and the page's title share a name ("Games"): the lowest one is the tab.
                return max(matches, key=lambda n: int(re.findall(r'-?\d+', n.get('bounds', '0 0 0 0'))[1]))
            time.sleep(.35)
        raise AssertionError(f'Missing live target: {label!r}; have {labels()!r}')

    def centre(node):
        x1, y1, x2, y2 = map(int, re.findall(r'-?\d+', node.get('bounds', '')))
        assert x2 > x1 and y2 > y1, 'Empty target'
        return (x1 + x2) // 2, (y1 + y2) // 2

    def tap(label, prefix=False, translate=True):
        x, y = centre(find(tr(label) if translate else label, prefix))
        shell('input', 'tap', str(x), str(y))
        time.sleep(.6)

    def hold(label, translate=False):
        x, y = centre(find(tr(label) if translate else label))
        shell('input', 'swipe', str(x), str(y), str(x), str(y), '900')
        time.sleep(1.2)

    def shot(name):
        time.sleep(.5)
        png = run('exec-out', 'screencap', '-p', text=False)
        width, height = struct.unpack('>II', png[16:24])
        assert (width < height) == (args.orientation == 'portrait'), f'Wrong orientation {width}x{height} for {name}'
        (out / f'{args.lang}-{args.orientation}-{name}.png').write_bytes(png)

    def back():
        shell('input', 'keyevent', '4')
        time.sleep(.6)

    def touch_mode():
        # Any tap on a quiet label switches back to touch (no hints): the page title is one.
        node = next((n for n in reversed(tree()) if n.get('content-desc') or n.get('text')), None)
        if node is not None:
            x, y = centre(node)
            shell('input', 'tap', str(x), str(y))
            time.sleep(.4)

    def set_look(look):
        tap('Settings')
        for _ in range(5):  # the four phone looks (Easy too) and one more look at the row
            row = find(tr('Look'), prefix=True)
            value = (row.get('text') or row.get('content-desc') or '')[len(tr('Look')):].strip(' ·')
            if value == tr(look):
                break
            x, y = centre(row)
            shell('input', 'tap', str(x), str(y))
            time.sleep(.5)
        else:
            raise AssertionError('Look not reachable: ' + look)
        tap('Games')

    run('emu', 'sensor', 'set', 'acceleration', '0:9.8:0' if args.orientation == 'portrait' else '9.8:0:0')
    if args.empty:
        # A fresh install that skips every step: each tab must still show what is going on and what to do.
        assert args.lang == 'en', 'The fresh start begins in the emulator language (English)'
        shell('am', 'force-stop', PACKAGE)
        assert 'Success' in shell('pm', 'clear', PACKAGE)
        shell('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
        time.sleep(2)
        for _ in range(6):
            tap('Skip')
        tap('Continue')
        time.sleep(1)
        for tab, expected in (('Games', 'No games yet'), ('Favourites', 'No games yet'), ('Search', 'Search games'),
                              ('Curator', 'No games yet'), ('Settings', 'Look')):
            tap(tab)
            if tab == 'Search':
                back()
            assert any(v.startswith(expected) for v in labels()), f'{tab}: nothing visible ({labels()!r})'
            shot('empty-' + tab.lower())
        print('Empty tabs captured.')
        return
    if args.parity:
        def first_start(folder):
            shell('am', 'force-stop', PACKAGE)
            assert 'Success' in shell('pm', 'clear', PACKAGE)
            shell('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
            time.sleep(2)
            if 'Got it' in labels():
                tap('Got it')
            tap('Continue')
            tap('Continue')
            tap('Choose your ROM folder')
            time.sleep(2)
            subprocess.run(['python3', str(Path(__file__).with_name('select-demo-folder.py')), args.adb, args.serial, str(out), folder], check=True, stdout=subprocess.DEVNULL)
            time.sleep(3)
            for _ in range(5):
                tap('Continue')
            time.sleep(1.5)

        def swipe(x0, y0, x1, y1):
            shell('input', 'swipe', str(x0), str(y0), str(x1), str(y1), '350')
            time.sleep(1.2)

        def size():
            png = run('exec-out', 'screencap', '-p', text=False)
            return struct.unpack('>II', png[16:24])

        def strip_name():
            # The selected game's name: before Info in portrait's strip, last in landscape's panel.
            names = labels()
            return names[-1] if args.orientation == 'landscape' else names[names.index('Info') - 1]

        def play_and_quit(game):
            # The name of the selected game opens its card.
            if strip_name() != game:
                tap(game)
            tap('Play')
            time.sleep(4)
            back()
            tap('Save & quit')
            time.sleep(2.5)

        landscape = args.orientation == 'landscape'
        # One system and nothing played: the plain shelves as before, no names of shelves.
        first_start('PocketOne')
        # "Game Boy" stays the selected game's system line only.
        assert 'Recent games' not in labels() and sum(n.get('text') == 'Game Boy' for n in tree()) == 1, labels()
        shot('one-system')
        # A normal library from a fresh start, default settings.
        first_start('PocketMvp')
        shot('fresh')
        w, h = size()
        current = labels()
        if landscape:
            assert 'Shelf below' in current and 'Shelf above' in current, current
            # The arrows and a swipe across change the shelf; the page's title says which.
            titles = [t for t in current if t in SYSTEMS]
            assert titles, current
            tap('Shelf below')
            below = [t for t in labels() if t in SYSTEMS]
            assert below and below != titles, (titles, below)
            swipe(w // 3, h * 2 // 3, w // 3, h // 5)
            assert [t for t in labels() if t in SYSTEMS] != below, 'A swipe up goes to the shelf below'
            swipe(w // 3, h // 5, w // 3, h * 2 // 3)
            assert [t for t in labels() if t in SYSTEMS] == below, 'A swipe down comes back'
            shot('shelf-below')
            tap('Shelf above')
        else:
            # The fixture has a dozen systems: the page opens at the top (no shelf scrolled past) with two or more shelves on the
            # first screen, and the fresh start's selected game is the first one on the page.
            assert sum(t in SYSTEMS for t in current) >= 2, current
            assert not any(t in current for t in ('Neo Geo Pocket', 'PlayStation Portable')), ('A fresh start opens at the top', current)
            # Along a shelf: the shelf moves, the selection stays. The Game Boy shelf is the fixture's long one.
            selected = strip_name()
            # The Game Boy shelf may be below the first screen: scroll the page down to it.
            for _ in range(8):
                if any((n.get('text') or n.get('content-desc')) == 'Libbet 1' for n in tree()):
                    break
                swipe(w // 2, h * 3 // 4, w // 2, h // 4)
            bounds = centre(next(n for n in tree() if (n.get('text') or n.get('content-desc')) == 'Libbet 1'))
            swipe(w - 80, bounds[1], 80, bounds[1])
            after = labels()
            assert 'Libbet 1' not in after or 'Libbet 6' in after, after
            assert strip_name() == selected, 'A swipe along a shelf keeps the selection'
            shot('along-shelf')
            swipe(80, bounds[1], w - 80, bounds[1])
            swipe(w // 2, h * 3 // 4, w // 2, h // 4)
            shot('down-the-page')
            swipe(w // 2, h // 4, w // 2, h * 3 // 4)
            swipe(w // 2, h // 4, w // 2, h * 3 // 4)
        # A played game stands on Recent games, on top. Reaching it with Search keeps the test off the shelf's order.
        tap('Search')
        if any(k in labels() for k in ('Done',)):
            back()
        back()
        tap('Games')
        for _ in range(6):
            if 'Attack on Voxelburg' in labels():
                break
            shell('input', 'keyevent', '20')
            time.sleep(.5)
        play_and_quit('Attack on Voxelburg')
        now = labels()
        if landscape:
            for _ in range(8):
                if 'Recent games' in labels():
                    break
                tap('Shelf above')
            assert 'Recent games' in labels(), labels()
        else:
            shell('input', 'keyevent', *(['19'] * 8))
            time.sleep(1)
            assert 'Recent games' in labels(), (now, labels())
        shot('recent')
        # Favourite from the card: a shelf of its own, the card's game stays selected.
        tap('Info')
        selected = 'Attack on Voxelburg'
        tap('Favourite')
        shot('card')
        back()
        assert strip_name() == selected, labels()
        if not landscape:
            assert 'Favourites' in labels()[10:], labels()
        # Stickers: the card's button, the sheet, ticks, the year; kept over a restart.
        tap('Info')
        tap('Stickers')
        rows = labels()
        for row in ('Completed · Off', '100 % · Off', 'Favourite · On', 'Playing now · Off', 'Game of the year · Off'):
            assert row in rows, rows
        shot('stickers-sheet')
        tap('Completed', prefix=True)
        tap('100 %', prefix=True)
        tap('Game of the year', prefix=True)
        assert 'Completed · On' in labels() and 'Game of the year · On' in labels(), labels()
        if landscape:
            # The sheet is longer than a landscape page.
            swipe(w // 2, h // 2, w // 2, h // 5)
        year = next(t for t in labels() if t.startswith('Year · '))
        tap('Later')
        assert next(t for t in labels() if t.startswith('Year · ')) != year, 'The year moves'
        shot('stickers-on')
        back()
        assert 'Turn over' in labels(), 'Back from the sheet goes to the card'
        shot('card-stickers')
        back()
        shot('shelf-stickers')
        shell('am', 'force-stop', PACKAGE)
        shell('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
        time.sleep(3)
        assert 'Attack on Voxelburg' in labels(), labels()
        tap('Info')
        tap('Stickers')
        assert 'Completed · On' in labels() and 'Game of the year · On' in labels(), 'Stickers are kept over a restart'
        back()
        # Taking the star off on the sheet takes the game out of Favourites.
        tap('Stickers')
        tap('Favourite · On')
        assert 'Favourite · Off' in labels(), labels()
        back()
        back()
        # A key opens the sheet from the card, Up/Down change shelves.
        tap('Info')
        shell('input', 'keyevent', '109')
        time.sleep(.6)
        assert 'Completed · On' in labels(), 'SELECT on the card opens the stickers'
        shell('input', 'keyevent', '4')
        time.sleep(.5)
        shell('input', 'keyevent', '4')
        time.sleep(.5)
        selected = strip_name()
        shell('input', 'keyevent', '20')
        time.sleep(.7)
        assert strip_name() != selected, 'Down goes to the next shelf'
        shot('controller')
        tap('Games')
        # The other looks: Cabinet has the shelves, Split's list keeps its flat page and the sheet says why
        # stickers don't show.
        for look in ('Cabinet',):
            set_look(look)
            assert 'Recent games' in labels() or landscape, labels()
            shot('look-' + look.lower())
        set_look('Split')
        assert 'Shelf below' not in labels() and 'Recent games' not in labels(), labels()
        tap('Attack on Voxelburg')
        tap('Info')
        tap('Stickers')
        assert 'This look shows no cases' in labels(), labels()
        back()
        back()
        set_look('Shelf')
        # Stickers on cases off: they stay on the game, and the sheet says why they don't show.
        tap('Settings')
        if landscape:
            swipe(w // 2, h // 2, w // 2, h // 4)
        tap('Cozy extras')
        tap('Stickers on cases', prefix=True)
        tap('Games')
        tap('Info')
        tap('Stickers')
        assert 'Stickers on cases is off' in labels() and 'Completed · On' in labels(), labels()
        shot('stickers-off')
        back()
        back()
        tap('Settings')
        if landscape:
            swipe(w // 2, h // 2, w // 2, h // 4)
        tap('Cozy extras')
        tap('Stickers on cases', prefix=True)
        tap('Games')
        print('Shelves and stickers checked.', args.orientation)
        return
    shell('am', 'force-stop', PACKAGE)
    shell('logcat', '-c')
    shell('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
    time.sleep(2)
    # The app's language follows its own setting: switch it in Settings when it differs.
    lang = 'de' if 'Einstellungen' in labels() else 'en'
    tap('Settings')
    if lang != args.lang:
        tap('Language', prefix=True)
    lang = args.lang
    find(tr('Settings'))
    shot('settings')
    looks = [l for l in args.looks.split(',') if l]
    for look in looks:
        set_look(look)
        shot(f'games-{look.lower()}')
    set_look(looks[0])
    names = [n.get('content-desc') or n.get('text') for n in tree()]
    games = [n for n in names if n and n not in {tr(t) for t in ('Games', 'Favourites', 'Search', 'Curator', 'Settings', 'Play', 'Info')}]
    # A second game: tap selects (lifts) it; Play is on the strip.
    tap('Games')
    second = [n for n in tree() if (n.get('content-desc') or n.get('text')) and n.get('selected') == 'false']
    candidates = [n for n in second if (n.get('content-desc') or n.get('text')) in games]
    if candidates:
        x, y = centre(candidates[min(1, len(candidates) - 1)])
        shell('input', 'tap', str(x), str(y))
        time.sleep(.6)
    shot('games-selected')
    # Scroll the shelf with a fling, then come back to the top with the tab.
    if args.orientation == 'portrait':
        shell('input', 'swipe', '600', '1800', '600', '700', '150')
        time.sleep(1.5)
        shot('games-scrolled')
        tap('Games')
    # The card: long press on the selected game, Favourite on it, Back.
    selected = next((n for n in tree() if n.get('selected') == 'true' and (n.get('content-desc') or n.get('text')) in games), None)
    assert selected is not None, 'No selected game'
    x, y = centre(selected)
    shell('input', 'swipe', str(x), str(y), str(x), str(y), '900')
    time.sleep(1.5)
    shot('card')
    tap('Favourite')
    shot('card-favourite')
    tap('Turn over')
    time.sleep(1)
    shot('card-back')
    tap('Back')
    tap('Favourites')
    shot('favourites')
    # Favourites empty: remove it again through the card.
    tap('Games')
    set_look('Split')
    tap('Info')
    shot('info-page')
    tap('Favourite')
    tap('Back')
    tap('Favourites')
    shot('favourites-empty')
    tap('Search')
    shot('search-keyboard')
    for key in ('a', 't'):
        tap(key, translate=False)
    shot('search-typing')
    back()
    shot('search-results')
    tap('Curator')
    shot('curator')
    rows = [n for n in tree() if (n.get('content-desc') or n.get('text') or '').startswith(tr('System'))]
    if rows:
        x, y = centre(rows[0])
        shell('input', 'tap', str(x), str(y))
        time.sleep(.6)
        shot('facet')
        back()
    tap('Games')
    set_look(looks[0])
    # Controller: a D-pad key brings the hints and the selection's frame back; a touch hides them again.
    shell('input', 'keyevent', '22')
    time.sleep(.6)
    shot('games-controller')
    tap('Settings')
    shell('input', 'keyevent', '20')
    time.sleep(.5)
    shot('settings-controller')
    tap('Games')
    log = shell('logcat', '-d')
    (out / f'{args.lang}-{args.orientation}-logcat.txt').write_text(log)
    assert 'FATAL EXCEPTION' not in log and 'Fatal signal' not in log, 'App crashed'
    print(f'Phone screens captured: {args.lang} {args.orientation}. Inspect them.')


if __name__ == '__main__':
    main()
