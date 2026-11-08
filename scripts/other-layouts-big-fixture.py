#!/usr/bin/env python3
"""13,000-ROM mixed fixture: 'python3 big_fixture.py OUT'. Empty fake ROMs + one small PNG linked as every cover (no copyrighted files).
Systems and layouts (the card of someone who switched from several systems):
  gba 2500  device folder, gamelist.xml with 2,500 games, images/<game>-thumb.png
  snes 2500 'Super Nintendo Entertainment System (SFC)', .media/<game>.png
  megadrive 1800 'MD' (Anbernic), Imgs/<game>.png
  nes 2200  device folder, no pictures
  psx 800   'Sony PlayStation (PS)', one folder per disc (.cue + .bin), .media/<folder>.png
  gb 900 / gbc 600 device folders, no pictures
  n64 700   device folder, no pictures
  arcade 1000 device folder, zips, no pictures"""
import os, sys, subprocess
out = os.path.abspath(sys.argv[1]); roms = out + '/card/roms'
png = out + '/cover.png'
os.makedirs(roms, exist_ok=True); os.makedirs(out + '/data', exist_ok=True)
subprocess.run(['convert', '-size', '240x336', 'gradient:#335-#8ab', png], check=True)
def put(path, link=False):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if link:
        os.link(png, path)
    else:
        open(path, 'w').write('made up game')
def name(i): return f'Game number {i:05d} (USA)'
sysdefs = [('gba','Game Boy Advance','.gba .zip'),('snes','Super Nintendo','.sfc .smc .zip'),('megadrive','Sega Mega Drive','.md .bin .zip'),
           ('nes','Nintendo Entertainment System','.nes .zip'),('psx','Sony PlayStation','.cue .chd .m3u'),('gb','Game Boy','.gb .zip'),
           ('gbc','Game Boy Color','.gbc .zip'),('n64','Nintendo 64','.z64 .n64 .zip'),('arcade','Arcade','.zip')]
with open(out + '/es_systems.cfg', 'w') as f:
    f.write('<systemList>')
    for i, n, e in sysdefs:
        f.write(f'<system><name>{i}</name><fullname>{n}</fullname><path>{roms}/{i}</path><extension>{e}</extension><command>true</command></system>')
    f.write('</systemList>')
open(out + '/data/settings.json', 'w').write('{"onboarded":true,"seen_version":"9.9.9","motion":"off"}')
xml = ['<gameList>']
for i in range(2500):
    put(f'{roms}/gba/{name(i)}.gba'); put(f'{roms}/gba/images/{name(i)}-thumb.png', True)
    xml.append(f'<game><path>./{name(i)}.gba</path><name>Game number {i}</name><desc>A made-up game.</desc><thumbnail>./images/{name(i)}-thumb.png</thumbnail><rating>0.6</rating></game>')
open(roms + '/gba/gamelist.xml', 'w').write(''.join(xml) + '</gameList>')
for i in range(2500):
    d = roms + '/Super Nintendo Entertainment System (SFC)'
    put(f'{d}/{name(i)}.sfc'); put(f'{d}/.media/{name(i)}.png', True)
for i in range(1800):
    put(f'{roms}/MD/{name(i)}.md'); put(f'{roms}/MD/Imgs/{name(i)}.png', True)
for i in range(2200): put(f'{roms}/nes/{name(i)}.nes')
for i in range(800):
    d = roms + '/Sony PlayStation (PS)'
    put(f'{d}/{name(i)}/{name(i)}.cue'); put(f'{d}/{name(i)}/{name(i)}.bin'); put(f'{d}/.media/{name(i)}.png', True)
for i in range(900): put(f'{roms}/gb/{name(i)}.gb')
for i in range(600): put(f'{roms}/gbc/{name(i)}.gbc')
for i in range(700): put(f'{roms}/n64/{name(i)}.z64')
for i in range(1000): put(f'{roms}/arcade/{name(i)}.zip')
print(out)
