#!/usr/bin/python3
"""Run the installed dArkOS ES command as ark, outside pocket-ui's chroot.

The command template comes from the firmware, never from a game's filename.
Arguments replacing its tokens are shell-quoted in one pass. Host scripts retain
normal write access to their own saves and configurations, like an ES launch.
"""
import json
import os
from pathlib import Path
import re
import pwd
import shlex
import signal
import subprocess
import sys
import xml.etree.ElementTree as ET


def read_xml(path):
    try:
        return ET.parse(path).getroot()
    except (OSError, ET.ParseError):
        return ET.Element('empty')


def choose(node, settings, game):
    sid = node.findtext('name', '')
    emus = node.findall('emulators/emulator')
    wanted = game.get('emulator') or settings.get(sid + '.emulator')
    emu = next((e for e in emus if e.get('name') == wanted), emus[0] if emus else None)
    name = emu.get('name', '') if emu is not None else ''
    cores = [c.text or '' for c in emu.findall('cores/core')] if emu is not None else []
    wanted = game.get('core') or settings.get(sid + '.core')
    core = wanted if wanted in cores else (node.findtext('defaultCore', '') or next(iter(cores), ''))
    governors = [g.text or '' for g in emu.findall('governors/governor')] if emu is not None else []
    governor = game.get('governor') or settings.get(sid + '.governor') or node.findtext('defaultGov', '') or next(iter(governors), '') or settings.get('GlobalPerformanceGovernor') or 'performance'
    command = emu.get('command') if emu is not None else None
    # Emulator/core names appear within command paths too; accept only a single simple component.
    if any(v and not re.fullmatch(r'[a-zA-Z0-9_.-]+', v) for v in (name, core, governor)):
        raise ValueError('invalid dArkOS emulator/core/governor')
    return command or node.findtext('command', ''), name, core, governor


def command_line(template, values):
    token = r'%(?:ROM|ROM_RAW|BASENAME|SYSTEM|EMULATOR|CORE|GOVERNOR)%'
    # Some user ES templates already quote a complete placeholder. Replace those
    # quotes along with the placeholder; do not double-quote an escaped filename.
    pattern = re.compile(r"'(" + token + r")'|\"(" + token + r')\"|(' + token + r')')
    def replace(m):
        key = next(v for v in m.groups() if v is not None)[1:-1]
        return shlex.quote(values[key])
    result = pattern.sub(replace, template)
    if re.search(r'%[A-Z_]+%', result):
        raise ValueError('unsupported ES command placeholder')
    return result


def launch(sid, shelf_path):
    root = Path(os.environ['DARKOS_ROOT'])
    run = Path(os.environ['DARKOS_STATE'])
    home = Path(os.environ['DARKOS_ARK_HOME'])
    data = json.loads((run / 'systems.json').read_text())[sid]
    node = ET.fromstring(data['system'])
    folder = Path(data['path'])
    relative = Path(shelf_path).relative_to('/storage/roms/' + sid)
    if '..' in relative.parts:
        raise ValueError('game is outside its system folder')
    rom = folder / relative
    if not rom.is_file() and not rom.is_dir():
        raise ValueError('game is missing')
    settings = {e.get('name'): e.get('value', '') for e in read_xml(home / '.emulationstation/es_settings.cfg')}
    game = {}
    for e in read_xml(folder / 'gamelist.xml').findall('game'):
        if os.path.normpath(e.findtext('path', '')) in (str(relative), './' + str(relative), str(rom)):
            game = {t: e.findtext(t, '') for t in ('emulator', 'core', 'governor')}
            break
    template, emulator, core, governor = choose(node, settings, game)
    line = command_line(template, dict(ROM=str(rom), ROM_RAW=str(rom), BASENAME=rom.stem,
                                       SYSTEM=node.findtext('name', ''), EMULATOR=emulator, CORE=core, GOVERNOR=governor))
    fb = str(root / 'bin/pocket-fb')
    sock = os.environ['SWAYSOCK']
    def display(action):
        subprocess.run([fb, '--ipc-send', sock, 'pocket_port ' + action + ' ' + str(os.getpid())], check=True,
                       stdout=subprocess.DEVNULL, timeout=3)
    child = None
    def stopped(signum, frame):
        if child is not None:
            child.terminate()
        raise SystemExit(128 + signum)
    for sig in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT):
        signal.signal(sig, stopped)
    display('begin')
    try:
        print('dArkOS launch: %s via %s/%s as ark' % (sid, emulator, core), flush=True)
        # Discard the port's fake system tools, chroot HOME, Wayland and preload/GL settings.
        child = subprocess.Popen(['/usr/bin/sudo', '-n', '-u', 'ark', '/usr/bin/env', '-i',
                                  'HOME=' + str(home), 'USER=ark', 'LOGNAME=ark', 'TERM=linux',
                                  'XDG_RUNTIME_DIR=/run/user/' + str(pwd.getpwnam('ark').pw_uid),
                                  'PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin',
                                  'SDL_GAMECONTROLLERCONFIG=' + os.environ.get('DARKOS_HOST_SDL', ''),
                                  '/bin/bash', '-c', line], cwd=str(home))
        return child.wait()
    finally:
        if child is not None and child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        display('end')


if __name__ == '__main__':
    try:
        sys.exit(launch(*sys.argv[1:]))
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print('dArkOS game could not start: ' + str(error), file=sys.stderr)
        sys.exit(1)
