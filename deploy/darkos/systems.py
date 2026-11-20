#!/usr/bin/python3
"""Read dArkOS's own ES inventory; write only the port's shelf and mount plan.

Host commands stay host commands. The shelf adds pocket-core only where a known
64-bit core is installed; archives it cannot open retain the dArkOS route.
"""
import json
import os
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as ET

ALIASES = {'ppsspp': 'psp', 'lynx': 'atarilynx', 'gc': 'gamecube',
           'sega atomiswave': 'atomiswave', 'famicom disk system': 'fds',
           'pokemonmini': 'pokemini', 'jaguar': 'atarijaguar',
           'turbografx16': 'tg16', 'turbografxcd': 'tg16cd',
           'snes-hacks': 'snesh', 'msdos': 'pc'}
CORES = {
    'nds': ('dsperate', 'melondsds'),
    'gb': ('gambatte',), 'gbc': ('gambatte',), 'gba': ('gpsp', 'mgba'),
    'nes': ('fceumm', 'nestopia'), 'fds': ('fceumm', 'nestopia'),
    'snes': ('snes9x',), 'megadrive': ('genesis_plus_gx', 'picodrive'),
    'mastersystem': ('genesis_plus_gx', 'gearsystem', 'picodrive'),
    'gamegear': ('genesis_plus_gx', 'gearsystem', 'picodrive'),
    'sg-1000': ('genesis_plus_gx', 'gearsystem'), 'sega32x': ('picodrive',),
    'segacd': ('genesis_plus_gx', 'picodrive'), 'psx': ('pcsx_rearmed',),
    'pcengine': ('mednafen_pce_fast', 'beetle_pce_fast'),
    'pcenginecd': ('mednafen_pce_fast', 'beetle_pce_fast'),
    'supergrafx': ('mednafen_supergrafx', 'beetle_supergrafx'),
    'atari2600': ('stella2014', 'stella'), 'atari7800': ('prosystem',),
    'atarilynx': ('handy',), 'ngp': ('mednafen_ngp', 'race', 'beetle_ngp'),
    'ngpc': ('mednafen_ngp', 'race', 'beetle_ngp'),
    'wonderswan': ('mednafen_wswan', 'beetle_wswan'),
    'wonderswancolor': ('mednafen_wswan', 'beetle_wswan'),
    'pokemini': ('pokemini',), 'virtualboy': ('mednafen_vb', 'beetle_vb'),
    'arcade': ('fbneo',), 'fbneo': ('fbneo',), 'fbn': ('fbneo',),
    'neogeo': ('fbneo',), 'cps1': ('fbneo',), 'cps2': ('fbneo',), 'cps3': ('fbneo',),
    'n64': ('mupen64plus_next',),
    # The 3D systems as on ROCKNIX and the Brick: pocket-core with the image's own 64-bit cores (GLES, read back).
    'dreamcast': ('flycast',), 'naomi': ('flycast',), 'atomiswave': ('flycast',), 'psp': ('ppsspp',),
}
SKIP = {'options', 'tools', 'music', 'videos', 'imageviewer', 'kodi', 'moonlight', 'screenshots'}


def arm64_core(path):
    try:
        with path.open('rb') as f:
            head = f.read(20)
        return head[:6] == b'\x7fELF\x02\x01' and head[18:20] == b'\xb7\x00'
    except OSError:
        return False


def core_choices(sid, node, cores):
    # GameCube/Wii have no libretro route here: dArkOS's Dolphin. PSP minis keep dArkOS's PPSSPP (not one of
    # pocket-ui's systems). Dreamcast and PSP use pocket-core when the core is installed, dArkOS's command otherwise.
    if sid in ('pspminis', 'gamecube', 'wii'):
        return []
    if sid in ('dreamcast', 'psp'):
        # Only the plain 64-bit cores ROCKNIX uses; never a 32-bit or "xtreme"/rumble variant from the ES list.
        return [c for c in CORES[sid] if arm64_core(cores / (c + '_libretro.so'))]
    candidates = list(CORES.get(sid, ()))
    candidates += [c.text or '' for c in node.findall("emulators/emulator[@name='retroarch']/cores/core")]
    candidates += re.findall(r'/([a-zA-Z0-9_]+)_libretro\.so', node.findtext('command', ''))
    return list(dict.fromkeys(c for c in candidates if re.fullmatch(r'[a-zA-Z0-9_]+', c)
                             and arm64_core(cores / (c + '_libretro.so'))))


def systems(source, roms, cores):
    """Yield a canonical shelf node and its original host node/path. No symlinks created."""
    seen = set()
    for node in ET.parse(source).getroot().findall('system'):
        host = node.findtext('name', '').strip()
        sid = ALIASES.get(host.lower(), host.lower())
        if sid == 'snes' and Path(node.findtext('path', '')).name == 'sufami':
            sid = 'sufami'
        if host in SKIP or not re.fullmatch(r'[a-zA-Z0-9_-]+', sid) or sid in seen:
            continue
        original = Path(node.findtext('path', ''))
        # The selected card is authoritative, including when host XML still says /roms.
        if len(original.parts) < 3 or original.parts[1] not in ('roms', 'roms2'):
            continue
        relative = Path(*original.parts[2:])
        if '..' in relative.parts:
            continue
        real = roms / relative
        if not real.is_dir() or not node.findtext('command', '').strip():
            continue
        seen.add(sid)
        shelf = ET.Element('system')
        for tag, value in [('name', sid), ('fullname', node.findtext('fullname', sid)),
                           ('path', '/storage/roms/' + sid), ('extension', node.findtext('extension', ''))]:
            ET.SubElement(shelf, tag).text = value
        emus = ET.SubElement(shelf, 'emulators')
        ET.SubElement(ET.SubElement(emus, 'emulator', name='darkos'), 'cores')
        own = core_choices(sid, node, cores)
        if own:
            group = ET.SubElement(ET.SubElement(emus, 'emulator', name='pocket-core'), 'cores')
            for c in own:
                ET.SubElement(group, 'core').text = c
        yield sid, real, shelf, node


def prepare(source, roms, cores, config, run):
    root = ET.Element('systemList')
    inventory = {}
    mounts = []
    for sid, real, shelf, host in systems(source, roms, cores):
        root.append(shelf)
        inventory[sid] = {'path': str(real), 'system': ET.tostring(host, encoding='unicode')}
        mounts.append(sid + '\t' + str(real))
    config.mkdir(parents=True, exist_ok=True)
    ET.ElementTree(root).write(config / 'es_systems.cfg', encoding='unicode')
    (run / 'systems.json').write_text(json.dumps(inventory))
    (run / 'systems.mounts').write_text('\n'.join(mounts) + '\n')
    print('systems: %d folders, %d with pocket-core' %
          (len(inventory), sum(n.find("emulators/emulator[@name='pocket-core']") is not None for n in root)))


if __name__ == '__main__':
    prepare(*(Path(p) for p in sys.argv[1:]))
