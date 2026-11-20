#!/usr/bin/env python3
"""Paint a real page against a fault-injected card, without snapshot seed data."""
import argparse
import ctypes
import errno
import sys
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ui', required=True)
    parser.add_argument('--card', type=Path, required=True)
    parser.add_argument('--page', required=True)
    parser.add_argument('--contains', action='append', required=True)
    parser.add_argument('--load-target', choices=['slot', 'auto'], default='slot')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    d = args.card
    (d / "empty-roms").mkdir(exist_ok=True)
    (d / "wakealarm").write_text("0")
    env = dict(os.environ, POCKETUI_MOCK='1', POCKETUI_LANG='en',
               POCKETUI_DATA=str(d / 'data'), POCKETUI_ES_SYSTEMS=str(d / 'es_systems.cfg'),
               POCKETUI_VISIBLE_TEXT=str(d / 'live-words.txt'), POCKETUI_NO_AUTOSUSPEND='1',
               POCKETUI_BUILT_IN_ROMS=str(d / 'empty-roms'),
               POCKETUI_RTC_ALARM=str(d / 'wakealarm'), POCKETUI_SUSPEND_CMD='false',
               POCKETUI_POWEROFF_CMD='false', TMPDIR=str(d), POCKET_CORE_TEST='1',
               POCKETUI_STATES=str(d / 'states'), POCKETUI_NAV_PROBE=str(d / 'live-nav.txt'),
               POCKET_CHEATS_DB=str(d / 'cht'))
    env.pop('POCKETUI_NO_HISTORY', None)
    env.pop('SWAYSOCK', None)
    env.pop('POCKETUI_TEST_REAL_CORE', None)
    if args.page.startswith('game-'):
        # Only the UI's file readers are under test here. The separate game-start
        # rule exercises the real core and its audio output. This bounded guest
        # lets the normal pause-menu path read this card's states/options/cheats.
        host = d / 'live-host'
        host.write_bytes(b'')
        runner = d / 'run-game'
        runner.write_text('#!/bin/sh\nexec sleep 12\n')
        runner.chmod(0o755)
        env['POCKETUI_HOST_BIN'] = str(host)
    socket = d / 'live-wl'
    env['WAYLAND_DISPLAY'] = str(socket)
    def parent_death():
        if sys.platform.startswith('linux'):
            if ctypes.CDLL(None).prctl(1, 9, 0, 0, 0) != 0:
                os._exit(1)
            if os.getppid() == 1:
                os._exit(1)

    children = []
    deadline = time.monotonic() + 12
    words = ''
    with (d / 'live-ui.log').open('w') as log:
        def wait(condition, what):
            while not condition():
                if time.monotonic() >= deadline:
                    raise AssertionError(f'{args.page}: timed out waiting for {what}; words:\n{words}')
                if any(child.poll() is not None for child in children):
                    raise AssertionError(f'{args.page}: child exited while waiting for {what}')
                time.sleep(.01)

        try:
            children.append(subprocess.Popen(['python3', '-I', str(root / 'scripts/wayland-stub.py'), str(socket)], stdout=log, stderr=log, preexec_fn=parent_death))
            wait(socket.exists, 'compositor')
            children.append(subprocess.Popen([args.ui, '--test-driver', str(d)], env=env, stdout=log, stderr=log, preexec_fn=parent_death))
            fifo = d / 'ui.in'
            wait(fifo.exists, 'UI input')

            def send(text):
                fd = None
                def listening():
                    nonlocal fd
                    try:
                        fd = os.open(fifo, os.O_WRONLY | os.O_NONBLOCK)
                        return True
                    except OSError as error:
                        if error.errno not in (errno.ENXIO, errno.ENOENT):
                            raise
                        return False
                wait(listening, 'UI listening')
                try:
                    os.write(fd, (text + '\n').encode())
                finally:
                    os.close(fd)

            png = d / 'frame.png'
            nav = d / 'live-nav.txt'
            def capture():
                nonlocal words
                png.unlink(missing_ok=True)
                nav.unlink(missing_ok=True)
                send('frame')
                def complete():
                    try:
                        return png.read_bytes().endswith(b'IEND\xaeB`\x82') and nav.read_text().endswith('@end\n')
                    except FileNotFoundError:
                        return False
                wait(complete, 'painted frame')
                words = (d / 'live-words.txt').read_text()
                return words

            def select(row, activate=True):
                for _ in range(20):
                    capture()
                    boxes = []
                    for line in nav.read_text().splitlines():
                        fields = line.split('\t', 4)
                        if len(fields) != 5:
                            continue
                        try:
                            boxes.append((tuple(map(int, fields[:4])), fields[4]))
                        except ValueError:
                            continue
                    focus = next((box for box, text in boxes if text == '‹nav:focus›'), None)
                    if focus is None:
                        time.sleep(.02)
                        continue
                    text = [text for box, text in boxes if not text.startswith('‹nav:')
                            and box[0] >= focus[0] - 2 and box[2] <= focus[2] + 2
                            and focus[1] <= (box[1] + box[3]) / 2 <= focus[3]]
                    if row in text:
                        if activate:
                            send('a')
                        return
                    send('down')
                raise AssertionError(f'{args.page}: row {row!r} is unreachable; words:\n{words}')

            if args.page.startswith('game-'):
                send('a')
                def shelf():
                    try:
                        capture()
                        return 'place=shelf:Games' in (d / 'ui.status').read_text()
                    except FileNotFoundError:
                        return False
                wait(shelf, 'game shelf')
                send('a')
                def playing():
                    capture()
                    return 'game=playing' in (d / 'ui.status').read_text()
                wait(playing, 'guest started')
                send('home')
                wait(lambda: 'Continue' in capture(), 'pause menu')
                if args.page == 'game-load':
                    select('Load', activate=False)
                    if args.load_target == 'auto':
                        send('left')
                elif args.page == 'game-earlier':
                    select('Earlier', activate=False)
                elif args.page in ('game-options', 'game-cheats'):
                    select('Options')
                    if args.page == 'game-cheats':
                        select('Cheats')
                else:
                    raise AssertionError(f'unknown game page {args.page}')
            else:
                send('page ' + args.page)
            while time.monotonic() < deadline:
                capture()
                painted = ' '.join(words.split())
                if any(' '.join(expected.split()) in painted for expected in args.contains):
                    print(words)
                    return
                time.sleep(.04)
            raise AssertionError(f'{args.page}: no data or readable fallback {args.contains!r}; words:\n{words}')
        except BaseException:
            log.flush()
            print((d / 'live-ui.log').read_text())
            raise
        finally:
            for child in reversed(children):
                if child.poll() is None:
                    child.kill()
                child.wait()


if __name__ == '__main__':
    main()
