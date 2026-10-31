#!/usr/bin/env python3
"""One launch and pause per selected new core on our own emulator; no release proof bundle."""
import json,subprocess,sys,time,re,os,hashlib,urllib.request,xml.etree.ElementTree as ET
from pathlib import Path
adb,serial,destination=sys.argv[1:];out=Path(destination);out.mkdir(parents=True,exist_ok=True)
root=Path(__file__).resolve().parents[2]
def run(*a):return subprocess.check_output([adb,'-s',serial,*a])
def sh(*a):return run('shell',*a).decode()
manifest=json.loads((root/'scripts/android/core-smoke-roms.json').read_text())
selected=set(os.environ.get('ANDROID_CORE_CASES','pce,ngp,lynx,psp,nds,n64').split(','))
cases=[r for r in manifest if r['system'] in selected];assert cases,'No matching core cases'
for r in cases:
 path=out/r['name']
 if not path.exists():path.write_bytes(urllib.request.urlopen(r['url'],timeout=40).read())
 assert hashlib.sha256(path.read_bytes()).hexdigest()==r['sha256'],r['name']+' checksum mismatch'
(out/'roms.json').write_text(json.dumps(cases,indent=2)+'\n')
assert sh('getprop','ro.kernel.qemu').strip()=='1'
for r in cases:
 folder='/sdcard/Download/PocketMvp/'+r['system'];sh('mkdir','-p',folder);run('push',str(out/r['name']),folder+'/')
sh('am','force-stop','dev.pocket.spike');sh('am','start','-W','-n','dev.pocket.spike/.PocketActivity');time.sleep(2);sh('input','keyevent','34');time.sleep(1)
subprocess.check_call([sys.executable,str(root/'scripts/android/select-demo-folder.py'),adb,serial,str(out)]);time.sleep(2)
def nodes():
 raw=run('exec-out','uiautomator','dump','/dev/tty').decode();return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>')+12]).iter('node'))
def tap(label):
 ns=nodes();n=next(n for n in reversed(ns) if n.get('text')==label and n.get('enabled')=='true');a,b,c,d=map(int,re.findall(r'\d+',n.get('bounds')));sh('input','tap',str((a+c)//2),str((b+d)//2));time.sleep(.5)
for r in cases:
 name=Path(r['name']).stem;sh('input','keyevent',*(['21']*40));time.sleep(.4)
 for _ in range(45):
  n=next((n for n in nodes() if n.get('text','').casefold()==name.casefold()),None)
  if n is not None:
   if n.get('selected')!='true':tap(n.get('text'))
   break
  sh('input','keyevent','22');time.sleep(.15)
 else:raise AssertionError('Missing game '+name)
 sh('logcat','-c');tap('Play');time.sleep(5)
 log=sh('logcat','-d');(out/(name+'.log')).write_text(log)
 assert 'pocket-core in-app: '+r['core'] in log and 'Fatal signal' not in log,(name,'core launch failed')
 if r['system'] in ('psp','n64'):assert 'Android core renderer: GLES 3' in log,(name,'hardware context missing')
 (out/(name+'.png')).write_bytes(run('exec-out','screencap','-p'));sh('input','keyevent','4');time.sleep(.5)
 labels=[n.get('text') for n in nodes()];assert 'Continue' in labels,(name,'pause missing',labels)
 (out/(name+'-pause.png')).write_bytes(run('exec-out','screencap','-p'));tap('Quit');time.sleep(1)
 print(name,'QUICK LAUNCH + PAUSE PASS',flush=True)
