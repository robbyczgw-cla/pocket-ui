#!/usr/bin/env python3
"""Exercise first-run onboarding on our X300 emulator, using fresh semantic bounds.

Usage: onboarding-smoke.py ADB emulator-5582 OUTPUT [--all-combinations]
Clears ONLY dev.pocket.spike on the explicitly verified emulator. Existing ROM
fixtures in Download/PocketMvp are retained; PocketEmpty is a separate empty root.
Default: seven English portrait and seven German landscape screenshots. The flag
also checks English landscape/German portrait. No hardware targets are accepted.
"""
import argparse
import json
import re
import subprocess
import struct
import time
import xml.etree.ElementTree as ET
from pathlib import Path

PACKAGE = 'dev.pocket.spike'
HEADINGS = [
    'Welcome to pocket-ui', 'Language', 'Where your games are', 'Your look',
    'Covers and pictures', 'Your controls', 'Ready to play',
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('adb')
    parser.add_argument('serial')
    parser.add_argument('output', type=Path)
    parser.add_argument('--all-combinations', action='store_true')
    args = parser.parse_args()
    out = args.output
    out.mkdir(parents=True, exist_ok=True)
    sequence = 0
    lang = 'en'
    translations = {}
    table = Path(__file__).resolve().parents[2] / 'assets/i18n/de.tsv'
    for line in table.read_text().splitlines():
        if '\t' in line:
            source, translated = line.split('\t', 1)
            translations[source] = translated

    def tr(label):
        return translations.get(label, label) if lang == 'de' else label

    def run(*cmd, text=True):
        return subprocess.check_output([args.adb, '-s', args.serial, *cmd], text=text, timeout=30)

    def shell(*cmd):
        return run('shell', *cmd)

    assert args.serial == 'emulator-5582', 'This destructive fresh-start test is restricted to our X300 emulator-5582'
    assert shell('getprop', 'ro.kernel.qemu').strip() == '1', 'Refusing a real device'
    assert shell('getprop', 'sys.boot_completed').strip() == '1', 'Emulator has not booted'
    shell('mkdir', '-p', '/sdcard/Download/PocketEmpty')
    assert not shell('ls', '-A', '/sdcard/Download/PocketEmpty').strip(), 'PocketEmpty must contain no files'
    assert shell('ls', '/sdcard/Download/PocketMvp').strip(), 'Prepare existing PocketMvp ROM fixtures first'
    model = shell('getprop', 'ro.product.model').strip()

    def tree():
        nonlocal sequence
        raw = run('exec-out', 'uiautomator', 'dump', '/dev/tty')
        (out / f'ui-{sequence:04}.xml').write_text(raw)
        sequence += 1
        start, end = raw.find('<?xml'), raw.find('</hierarchy>')
        assert start >= 0 and end >= start, 'UIAutomator returned no hierarchy'
        return list(ET.fromstring(raw[start:end + len('</hierarchy>')]).iter('node'))

    def labels(nodes):
        return {value for n in nodes for value in (n.get('text'), n.get('content-desc')) if value}

    def find(nodes, label, prefix=False):
        for node in reversed(nodes):
            if node.get('enabled') != 'true':
                continue
            values = (node.get('text', ''), node.get('content-desc', ''))
            if any(value.startswith(label) if prefix else value == label for value in values):
                return node
        return None

    def touch(node):
        bounds = list(map(int, re.findall(r'-?\d+', node.get('bounds', ''))))
        assert len(bounds) == 4, 'Target has no current bounds'
        x1, y1, x2, y2 = bounds
        assert x2 > x1 and y2 > y1, 'Target has empty bounds'
        shell('input', 'tap', str((x1 + x2) // 2), str((y1 + y2) // 2))
        time.sleep(.35)

    def tap(label, prefix=False, translate=True):
        for _ in range(8):
            node = find(tree(), tr(label) if translate else label, prefix)
            if node is not None:
                touch(node)
                return
            time.sleep(.35)
        raise AssertionError(f'Missing enabled live target: {tr(label)!r}')

    def wait_step(number):
        expected = tr('Step {n} of {m}').replace('{n}', str(number)).replace('{m}', '7')
        heading = tr(HEADINGS[number - 1])
        for _ in range(12):
            current = labels(tree())
            if expected in current and heading in current:
                return
            time.sleep(.35)
        raise AssertionError(f'Expected step {number}: {expected!r}, {heading!r}; got {sorted(current)!r}')

    def screenshot(name):
        time.sleep(.25)
        png = run('exec-out', 'screencap', '-p', text=False)
        width, height = struct.unpack('>II', png[16:24])
        assert (width < height) == ('-portrait-' in name), f'Wrong screen orientation: {width}x{height}'
        (out / f'{name}.png').write_bytes(png)

    def start():
        shell('am', 'start', '-W', '-n', PACKAGE + '/.PocketActivity')
        time.sleep(.7)

    def restart_on(number):
        shell('am', 'force-stop', PACKAGE)
        start()
        wait_step(number)

    def pick_root(folder):
        # Android's system picker is deliberately not localized by the app's own
        # language choice, so accept the actual system-language button variants.
        entered = False
        for _ in range(24):
            nodes = tree()
            current = labels(nodes)
            app_nodes = [n for n in nodes if n.get('package') == PACKAGE]
            if entered and app_nodes and not any(n.get('package') == 'com.google.android.documentsui' for n in nodes):
                return
            target = None
            for label in ('ALLOW', 'Allow', 'ZULASSEN', 'Zulassen', 'Got it', 'OK'):
                target = find(nodes, label)
                if target is not None:
                    break
            if target is None and folder in current:
                for label in ('USE THIS FOLDER', 'Use this folder', 'DIESEN ORDNER VERWENDEN', 'Diesen Ordner verwenden'):
                    target = find(nodes, label)
                    if target is not None:
                        entered = True
                        break
            if target is None:
                # A folder row must be tapped, not the breadcrumb with its name.
                target = next((n for n in reversed(nodes) if n.get('text') == folder
                               and n.get('resource-id', '').endswith('android:id/title') and n.get('enabled') == 'true'), None)
            if target is None:
                # Open roots to leave a previously selected empty directory.
                for label in ('Show roots', 'Wurzeln anzeigen', 'Navigation drawer', 'Download', 'Downloads', model):
                    target = find(nodes, label)
                    if target is not None:
                        break
            if target is None:
                # Some picker breadcrumb ancestors are disabled in the semantic tree.
                # Android Back goes to the parent folder; re-read its actual rows.
                if any(n.get('package', '').endswith('documentsui') for n in nodes):
                    shell('input', 'keyevent', '4')
                    time.sleep(.35)
                    continue
                raise AssertionError(f'No SAF navigation target for {folder}: {sorted(current)!r}')
            touch(target)
        raise AssertionError('SAF selection timed out: ' + folder)

    # Skipping every optional step must reach a usable empty library offline.
    shell('am', 'force-stop', PACKAGE)
    assert 'Success' in shell('pm', 'clear', PACKAGE)
    run('emu', 'sensor', 'set', 'acceleration', '0:9.8:0')
    time.sleep(1)
    start()
    for number in range(1, 7):
        wait_step(number)
        tap('Skip')
    wait_step(7)
    screenshot('skip-portrait-07-ready')
    tap('Continue')
    time.sleep(.7)
    current = labels(tree())
    assert not any(tr('Step {n} of {m}').replace('{n}', str(n)).replace('{m}', '7') in current for n in range(1, 8))
    assert tr('Choose your ROM folder') in current or tr('No games') in current, 'Skipped setup did not open an empty library'
    screenshot('skip-portrait-08-empty-library')
    shell('am', 'force-stop', PACKAGE)
    start()
    assert tr(HEADINGS[0]) not in labels(tree()), 'Skipped setup completion was not persisted'

    combinations = [('en', 'portrait'), ('de', 'landscape')]
    if args.all_combinations:
        combinations += [('en', 'landscape'), ('de', 'portrait')]
    results = []
    for chosen_lang, orientation in combinations:
        lang = 'en'
        shell('am', 'force-stop', PACKAGE)
        assert 'Success' in shell('pm', 'clear', PACKAGE), 'App data reset failed'
        shell('logcat', '-c')
        sensor = '0:9.8:0' if orientation == 'portrait' else '9.8:0:0'
        run('emu', 'sensor', 'set', 'acceleration', sensor)
        time.sleep(1)
        start()
        prefix = f'{chosen_lang}-{orientation}'
        wait_step(1)
        screenshot(prefix + '-01-welcome')
        tap('Continue')
        wait_step(2)
        restart_on(2)
        tap('Deutsch' if chosen_lang == 'de' else 'English', translate=False)
        lang = chosen_lang
        wait_step(2)
        screenshot(prefix + '-02-language')
        tap('Continue')
        wait_step(3)
        restart_on(3)
        # Cancelling the system picker keeps onboarding and the unchosen root intact.
        tap('Choose your ROM folder')
        time.sleep(.7)
        # Back may first go to the picker's parent directory, rather than dismiss
        # DocumentsUI. Keep using live trees until the app's same step returns.
        expected_step = tr('Step {n} of {m}').replace('{n}', '3').replace('{m}', '7')
        for _ in range(8):
            shell('input', 'keyevent', '4')
            time.sleep(.35)
            current = labels(tree())
            if expected_step in current and tr(HEADINGS[2]) in current:
                break
        else:
            raise AssertionError('Back did not dismiss the SAF picker to onboarding step 3')
        wait_step(3)
        tap('Choose your ROM folder')
        pick_root('PocketEmpty')
        wait_step(3)
        screenshot(prefix + '-03-empty-folder')
        # Empty folders must be replaceable without restarting onboarding.
        tap('Choose another folder')
        pick_root('PocketMvp')
        wait_step(3)
        restart_on(3)
        screenshot(prefix + '-03-rom-folder')
        tap('Continue')
        wait_step(4)
        current = labels(tree())
        for choice in ('Shelf', 'Minimal', 'Split', 'Cards', 'Cabinet'):
            label = ('• ' if choice == 'Shelf' else '') + tr(choice)
            assert label in current, 'Missing look choice: ' + label
        for setting, value in (('Animations', 'Cozy'), ('Appearance', 'Dark')):
            label = tr(setting) + ' · ' + tr(value)
            assert label in current, 'Missing sensible default: ' + label
        screenshot(prefix + '-04-look')
        tap('Back')
        wait_step(3)
        tap('Continue')
        wait_step(4)
        tap('Continue')
        wait_step(5)
        current = labels(tree())
        assert any(label in current for label in (tr('Download on Wi-Fi'), '• ' + tr('Download on Wi-Fi')))
        screenshot(prefix + '-05-covers')
        tap('Not now')
        # Choosing the option may select it or advance; both are allowed.
        if tr('Step {n} of {m}').replace('{n}', '5').replace('{m}', '7') in labels(tree()):
            assert '• ' + tr('Not now') in labels(tree()), 'Not now was not selected'
            tap('Continue')
        wait_step(6)
        assert any(tr(label) in labels(tree()) for label in ('Touch', 'Controller detected'))
        screenshot(prefix + '-06-controls')
        tap('Continue')
        wait_step(7)
        screenshot(prefix + '-07-ready')
        tap('Continue')
        time.sleep(1)
        assert not any(tr('Step {n} of {m}').replace('{n}', str(n)).replace('{m}', '7') in labels(tree()) for n in range(1, 8)), 'Did not finish onboarding'
        shell('am', 'force-stop', PACKAGE)
        start()
        assert tr(HEADINGS[0]) not in labels(tree()), 'Finished onboarding reappeared after relaunch'
        screenshot(prefix + '-08-library')
        log = shell('logcat', '-d')
        (out / f'{prefix}-logcat.txt').write_text(log)
        assert 'FATAL EXCEPTION' not in log and 'Fatal signal' not in log, 'App crashed during onboarding'
        results.append({'language': chosen_lang, 'orientation': orientation, 'steps': 7, 'resumed': [2, 3], 'folders': ['PocketEmpty', 'PocketMvp']})
    (out / 'results.json').write_text(json.dumps({'skip_all': True, 'picker_cancel': True, 'combinations': results}, indent=2) + '\n')
    print('Onboarding: seven steps, EN/DE, portrait/landscape, resume and real/empty SAF roots passed. Inspect the PNGs.')


if __name__ == '__main__':
    main()
