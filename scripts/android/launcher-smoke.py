#!/usr/bin/env python3
"""Screenshot/UI evidence for shared launcher screens and both appearances; taps use current semantic bounds."""
import re,subprocess,sys,time,xml.etree.ElementTree as ET
from pathlib import Path
adb,serial,destination=sys.argv[1:];out=Path(destination);out.mkdir(parents=True,exist_ok=True)
def run(*args,**kwargs):return subprocess.check_output([adb,'-s',serial,*args],**kwargs)
def shell(*args):return run('shell',*args,text=True)
assert serial.startswith('emulator-')and shell('getprop','ro.kernel.qemu').strip()=='1'
shell('am','force-stop','dev.pocket.spike');shell('logcat','-c');shell('am','start','-W','-n','dev.pocket.spike/.PocketActivity');time.sleep(2)
sequence=0
def tree():
    global sequence
    raw=run('exec-out','uiautomator','dump','/dev/tty',text=True);(out/f'ui-{sequence:03}.xml').write_text(raw);sequence+=1
    return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>')+12]).iter('node'))
def tap(label,prefix=False):
    for _ in range(5):
        n=next((n for n in reversed(tree())if (n.get('text','').startswith(label)if prefix else n.get('text')==label)),None)
        if n is not None:
            x1,y1,x2,y2=map(int,re.findall(r'\d+',n.get('bounds')));shell('input','tap',str((x1+x2)//2),str((y1+y2)//2));time.sleep(.4);return
        time.sleep(.4)
    raise AssertionError('Missing live UI target: '+label)
def activate(prefix):
    # A sheet row acts at the first tap on phones.
    tap(prefix,True)
def key(code):shell('input','keyevent',str(code));time.sleep(.45)
def shot(name):time.sleep(.3);(out/(name+'.png')).write_bytes(run('exec-out','screencap','-p'))
def choose_game(name):
    # Navigation clamps at the last game. Start at the beginning before searching a narrow shelf.
    shell('input','keyevent',*(['21']*40));time.sleep(.5)
    for _ in range(30):
        n=next((n for n in tree()if n.get('text')==name),None)
        if n is not None:
            if n.get('selected')!='true':tap(name)
            return
        key(22)
    raise AssertionError('Missing game: '+name)
# Restore English through real settings when a previous pass ended in German.
key(108)
if any(n.get('text','').startswith('Sprache ·')for n in tree()):activate('Sprache ·')
for appearance in ['Dark','Light']:
    n=next(n for n in tree()if n.get('text','').startswith('Appearance ·'))
    if not n.get('text').endswith(appearance):activate('Appearance ·')
    shot(appearance.lower()+'-settings')
    looks=set()
    for _ in range(5):
        n=next(n for n in tree()if n.get('text','').startswith('Look ·'));look=n.get('text').split(' · ')[1];looks.add(look)
        tap('Games');shot(appearance.lower()+'-'+look.lower());tap('Settings');activate('Look')
    assert len(looks)==5,looks
    tap('Games');choose_game('Libbet and the Magic Floor')
    key(99);shot(appearance.lower()+'-box-front');key(99);shot(appearance.lower()+'-box-back');key(99);key(4)
    tap('Favourites')
    if not any(n.get('text')=='Libbet and the Magic Floor'for n in tree()):
        tap('Games');choose_game('Libbet and the Magic Floor');key(100);tap('Favourites')
    assert any(n.get('text')=='Libbet and the Magic Floor'for n in tree()),'Favourite missing after starring'
    shot(appearance.lower()+'-favourites');tap('Games');choose_game('Libbet and the Magic Floor');key(100)
    tap('Search')
    if appearance=='Light':tap('Delete');tap('Delete')
    tap('l');tap('i');shot(appearance.lower()+'-search-keyboard');tap('Done')
    assert any(n.get('text')=='Libbet and the Magic Floor'for n in tree()),'Touch search lost the expected match'
    shot(appearance.lower()+'-search-results');key(4)
    tap('Curator');shot(appearance.lower()+'-curator')
    n=next((n for n in tree()if ' · All'in n.get('text','')),None)
    if n is not None:activate(n.get('text'));shot(appearance.lower()+'-curator-facet');key(4)
    key(4);key(109);shot(appearance.lower()+'-quick-settings');key(4)
    choose_game('Libbet and the Magic Floor');tap('Play');time.sleep(2);tap('Pause');shot(appearance.lower()+'-player-pause')
    activate('Options');shot(appearance.lower()+'-player-options');key(4);activate('Quit');time.sleep(1)
    tap('Settings')
# Check translated settings, media and player menu, then leave the preview in English/dark on the shelf.
activate('Language ·');shot('de-settings');key(4);shot('de-library');key(99);shot('de-box');key(4)
key(108);activate('Sprache ·')
n=next(n for n in tree()if n.get('text','').startswith('Appearance ·'))
if not n.get('text').endswith('Dark'):activate('Appearance ·')
key(4)
log=shell('logcat','-d');(out/'logcat.txt').write_text(log)
assert 'FATAL EXCEPTION'not in log and 'Fatal signal'not in log
(out/'device.txt').write_text(shell('getprop')+shell('wm','size')+shell('wm','density'))
print('Five looks and launcher/player screens captured in dark/light; EN/DE captured. Inspect every PNG.')
