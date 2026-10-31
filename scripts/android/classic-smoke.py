#!/usr/bin/env python3
"""Own emulator: pinned starter games / original SMS+GG fixtures, each bundled classic core, slots and warm resume."""
import hashlib,io,json,os,re,runpy,subprocess,sys,time,urllib.request,zipfile,xml.etree.ElementTree as ET
from pathlib import Path
adb,serial,destination=sys.argv[1:];out=Path(destination);out.mkdir(parents=True,exist_ok=True)
root=Path(__file__).resolve().parents[2]
def run(*args,**kwargs):return subprocess.check_output([adb,'-s',serial,*args],**kwargs)
def shell(*args):return run('shell',*args,text=True)
assert serial.startswith('emulator-')and shell('getprop','ro.kernel.qemu').strip()=='1','Own emulator required'
def digest(data):return hashlib.sha256(data).hexdigest()
entries=json.loads((root/'assets/starter-pack.json').read_text())['games'];provenance=[];cases=[]
for game_id,core in [('ucity','mGBA'),('attack-on-voxelburg','mGBA'),('thwaite','Nestopia'),('elevator-madness-dx','bsnes'),('kleleatoms','ClownMDEmu')]:
    g=next(x for x in entries if x['id']==game_id);f=g['file'];rom=out/f['name_on_disk']
    if not rom.exists():
        data=urllib.request.urlopen(f['url'],timeout=60).read();assert digest(data)==f['sha256']
        if f.get('archive')=='zip':
            with zipfile.ZipFile(io.BytesIO(data))as z:
                matches=[z.read(i)for i in z.infolist()if not i.is_dir()and i.file_size<=32*1024*1024 and i.filename.lower().endswith(('.sfc','.smc','.gba'))]
            data=next(b for b in matches if digest(b)==f['rom_sha256'])
        rom.write_bytes(data)
    assert digest(rom.read_bytes())==f.get('rom_sha256',f['sha256'])
    folder='/sdcard/Download/PocketMvp/'+g['system'];shell('mkdir','-p',folder);run('push',str(rom),folder+'/')
    cases.append((g['id'],rom.stem,core));provenance.append(g)
fixture=runpy.run_path(str(root/'scripts/testroms.py'))
for system in ['sms','gg','ws','atari2600']:
    ext={'atari2600':'a26'}.get(system,system)
    rom=out/(system+'-test.'+ext);rom.write_bytes(fixture['wonderswan']()if system=='ws'else fixture['a2600']()if system=='atari2600'else fixture['sega_rom'](system))
    folder='/sdcard/Download/PocketMvp/'+system;shell('mkdir','-p',folder);run('push',str(rom),folder+'/')
    cases.append((system,rom.stem,{'ws':'Beetle WonderSwan','atari2600':'Stella'}.get(system,'Gearsystem')));provenance.append({'id':system,'source':'scripts/testroms.py, original pocket-ui diagnostic','sha256':digest(rom.read_bytes())})
if os.environ.get('ANDROID_CLASSIC_CASES'):
    selected=set(os.environ['ANDROID_CLASSIC_CASES'].split(','));cases=[c for c in cases if c[0]in selected];assert cases,'No matching classic cases'
(out/'rom-provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
shell('am','force-stop','dev.pocket.spike');shell('logcat','-c');shell('am','start','-W','-n','dev.pocket.spike/.PocketActivity');time.sleep(2)
shell('input','keyevent','34');time.sleep(1)
subprocess.check_call([sys.executable,str(root/'scripts/android/select-demo-folder.py'),adb,serial,str(out)]);time.sleep(2)
sequence=0
def tree():
    global sequence
    raw=run('exec-out','uiautomator','dump','/dev/tty',text=True)
    (out/f'ui-{sequence:03}.xml').write_text(raw);sequence+=1
    return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>')+12]).iter('node'))
def tap(label):
    for _ in range(5):
        node=next((n for n in reversed(tree())if n.get('text')==label and n.get('enabled')=='true'),None)
        if node is not None:
            x1,y1,x2,y2=map(int,re.findall(r'\d+',node.get('bounds')));shell('input','tap',str((x1+x2)//2),str((y1+y2)//2));time.sleep(.4);return
        time.sleep(.5)
    raise AssertionError('Missing current touch target: '+label)
def key(code):shell('input','keyevent',str(code));time.sleep(.4)
def shot(name):(out/(name+'.png')).write_bytes(run('exec-out','screencap','-p'))
def choose(label):
    # Navigation clamps at the last game. Start at the beginning before searching a narrow shelf.
    shell('input','keyevent',*(['21']*40));time.sleep(.5)
    for _ in range(30):
        node=next((n for n in tree()if n.get('text','').casefold()==label.casefold()),None)
        if node is not None:
            if node.get('selected')!='true':tap(node.get('text'))
            return
        key(22)
    raise AssertionError('Game not found: '+label)
# Select the actual user-facing GB/GBC core preference; switching must not reuse incompatible states.
wanted=os.environ.get('ANDROID_GAMEBOY_CORE','mGBA');assert wanted in ('mGBA','Gambatte')
tap('Settings')
for _ in range(8):key(19)
for _ in range(5):key(20)
setting=next(n for n in tree()if n.get('text','').startswith('Game Boy core ·'))
if not setting.get('text').endswith(wanted):tap(setting.get('text'))
assert any(n.get('text')=='Game Boy core · '+wanted for n in tree()),'Core preference did not change'
key(4)
for cid,label,core in cases:
    if cid=='ucity':core=wanted
    shell('logcat','-c')
    choose(label);tap('Play');time.sleep(3)
    log=shell('logcat','-d');assert 'pocket-core in-app: '+core in log,(cid,'wrong core')
    if cid in ('attack-on-voxelburg','elevator-madness-dx'):
        labels={n.get('text')for n in tree()};assert {'L','R'}<=labels,(cid,'missing shoulder controls')
        tap('L');tap('R')
    if cid=='kleleatoms':assert {'A','B','C','X','Y','Z'}<={n.get('text')for n in tree()},'Missing Mega Drive touch buttons'
    tap('Start');time.sleep(2);shot(cid+'-playing');tap('Pause');shot(cid+'-pause')
    tap('Next slot')
    tap('Save · Slot 2');shot(cid+'-saved')
    tap('Load · Slot 2');shot(cid+'-loaded')
    tap('Previous slot');assert any(n.get('text')=='Save · Slot 1'for n in tree()),'Touch slot did not return to slot 1'
    tap('Earlier');shot(cid+'-earlier')
    tap('Quit');time.sleep(1);shot(cid+'-shelf')
    tap('Play');time.sleep(2);key(4);tap('Quit');time.sleep(1)
    log=shell('logcat','-d');(out/'logcat.txt').write_text(log)
    assert 'FATAL EXCEPTION'not in log and 'Fatal signal'not in log
    assert 'player state loaded:'in log and 'resume=Resumed'in log
    assert any('player state loaded:'in line and '.earlier0'in line for line in log.splitlines()),'Earlier point was not restored'
    starts=[line for line in log.splitlines()if 'pocket-core in-app:'in line]
    assert len(starts)==2 and all('pocket-core in-app: '+core in line for line in starts)
    (out/(cid+'-logcat.txt')).write_text(log)
    assert 'Could not load'not in log
    print(cid,core,'touch, save/load, Earlier and warm resume passed',flush=True)
(out/'device.txt').write_text(shell('getprop')+shell('wm','size')+shell('wm','density'))
print('Inspect PNGs. These functional checks do not replace the separate pitch/dropout diagnostic.')
