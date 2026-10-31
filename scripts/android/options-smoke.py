#!/usr/bin/env python3
"""Own rooted emulator: Gambatte options survive restart without touching mGBA's per-system options."""
import json,os,runpy,subprocess,sys,time
from pathlib import Path
adb,serial,destination=sys.argv[1:];out=Path(destination);out.mkdir(parents=True,exist_ok=True)
assert serial.startswith('emulator-')
def shell(*args,**kw):return subprocess.check_output([adb,'-s',serial,'shell',*args],text=True,**kw)
assert shell('getprop','ro.kernel.qemu').strip()=='1'and shell('id','-u').strip()=='0','Own rooted emulator required'
os.environ['ANDROID_CLASSIC_CASES']='ucity';os.environ['ANDROID_GAMEBOY_CORE']='Gambatte'
sys.argv=[str(Path(__file__).with_name('classic-smoke.py')),adb,serial,str(out/'fixture')]
qa=runpy.run_path(sys.argv[0]);tap,key,tree,shot=map(qa.get,['tap','key','tree','shot'])
base='/data/user/0/dev.pocket.spike/files/core-options/'
def read(name):
    try:return json.loads(shell('cat',base+name+'.json',stderr=subprocess.DEVNULL))
    except subprocess.CalledProcessError:return None
mgba=read('gbc');tap('Play');time.sleep(2);tap('Pause');tap('Options')
def first():return next(n.get('text')for n in tree()if n.get('text')and n.get('text')!='Back')
before=first();tap(before);changed=first();assert changed!=before,'Core option did not change'
time.sleep(.5);saved=read('gbc-gambatte');assert saved,'Alternative core options were not written to their own file'
assert read('gbc')==mgba,'Gambatte overwrote mGBA options'
shot('gambatte-options-changed');key(4);tap('Quit');time.sleep(1)
tap('Play');time.sleep(2);tap('Pause');tap('Options')
assert first()==changed,'Alternative core option did not survive restart'
assert read('gbc-gambatte')==saved and read('gbc')==mgba
shot('gambatte-options-restored')
for _ in range(32):
    value=first()
    if value==before:break
    tap(value)
else:raise AssertionError('Could not restore the fixture option')
key(4);tap('Quit');time.sleep(1)
(out/'result.json').write_text(json.dumps({'gambatte_option_survived_restart':True,'mgba_options_unchanged':True,'restored_fixture_option':True},indent=2)+'\n')
print('Gambatte option persistence and unchanged mGBA options passed.')
