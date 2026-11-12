#!/usr/bin/env python3
"""Android 0.3 QA after 02-smoke's matrix. Disposable rooted emulator only; homebrew fixtures.

03-smoke.py ADB emulator-N OUTPUT [names|literal-names|starts|faults]
Pictures are JPEG <=150 KB. Optical PS1 scenes use the production rehearsal seam because no PS1 core is bundled;
PSP and GB journeys run actual bundled cores. No external app, upload, key or physical device is touched.
"""
import hashlib, io, json, os, re, shlex, subprocess, sys, time, urllib.request, xml.etree.ElementTree as ET
from pathlib import Path
from PIL import Image, ImageDraw
adb,serial,destination=sys.argv[1:4];phase=sys.argv[4] if len(sys.argv)>4 else 'all'
root=Path(__file__).resolve().parents[2];out=Path(destination);out.mkdir(parents=True,exist_ok=True)
pkg='xyz.pocketui.app';private='/data/data/'+pkg+'/files';base='/sdcard/Download/PocketMvp'
def run(*args,**kw): return subprocess.check_output([adb,'-s',serial,*args],timeout=kw.pop('timeout',30),**kw)
def sh(*args): return run('shell',shlex.join(args),text=True)
def read(path): return run('exec-out','cat '+shlex.quote(path))
def states(pattern):
 return sh('sh','-c','if [ -d "$1" ]; then find "$1" -type f -name "$2"; fi','qa',private+'/states',pattern).splitlines()
assert serial.startswith('emulator-') and sh('getprop','ro.kernel.qemu').strip()=='1'
run('root');time.sleep(1);assert sh('id').startswith('uid=0')
sh('cmd','media_session','volume','--stream','3','--set','0');sh('settings','put','system','haptic_feedback_enabled','0')
seq=0
def nodes():
 global seq
 for _ in range(5):
  raw=run('exec-out','uiautomator dump /dev/tty',text=True);(out/f'ui-{seq:04}.xml').write_text(raw);seq+=1
  if '<?xml' in raw and '</hierarchy>' in raw:
   return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>')+12]).iter('node'))
  time.sleep(.5)
 raise AssertionError('Android did not provide a complete accessibility hierarchy')
def find(label,prefix=False): return next((n for n in reversed(nodes()) if n.get('enabled')=='true' and (n.get('text','').startswith(label) if prefix else n.get('text')==label)),None)
def wait(label,prefix=False):
 for _ in range(10):
  n=find(label,prefix)
  if n is not None:return n
  time.sleep(.25)
 raise AssertionError('Missing '+label)
def tap(label,prefix=False):
 n=wait(label,prefix);a,b,c,d=map(int,re.findall(r'\d+',n.get('bounds')));sh('input','tap',str((a+c)//2),str((b+d)//2));time.sleep(.15)
def key(code):sh('input','keyevent',str(code));time.sleep(.2)
def shot(name):
 picture=Image.open(io.BytesIO(run('exec-out','screencap -p'))).convert('RGB');picture.thumbnail((1280,1280))
 for q in [80,65,50,35]:
  buffer=io.BytesIO();picture.save(buffer,format='JPEG',quality=q,optimize=True)
  if buffer.tell()<=150_000:break
 assert buffer.tell()<=150_000;(out/(name+'.jpg')).write_bytes(buffer.getvalue())
def prefs(relative,extra=(),**changes):
 sh('am','force-stop',pkg);value=json.loads(read(private+'/launcher.json'))
 value.update(onboarding_done=True,language='en',font='retro',remind_where=False,download_media=False,haptics=False,muted=True,selected_uri='',selected_key='com.android.externalstorage.documents:primary:Download/PocketMvp/'+relative)
 value.update(changes);p=out/'prefs.json';p.write_text(json.dumps(value));run('push',str(p),'/data/local/tmp/android-03-prefs.json')
 sh('cp','/data/local/tmp/android-03-prefs.json',private+'/launcher.json');sh('am','start','-W','-n',pkg+'/.PocketActivity',*extra);time.sleep(.5)
def started(back="Back"):
 for _ in range(8):
  if find('Pause') is not None:return
  if find(back) is not None:key(4)
  time.sleep(.2)
 raise AssertionError('The real player did not start')
def play():tap('Play' if find('Play') is not None else 'Continue');started()
def pause():key(4);wait('Save & quit')
def quit():
 tap('Save & quit')
 for _ in range(15):
  activity=sh('dumpsys','activity','activities')
  if any('topResumedActivity=' in line and '/.PocketActivity' in line for line in activity.splitlines()):break
  time.sleep(.2)
 else:raise AssertionError('Save & quit did not return to the launcher')
 wait('Play' if find('Play') is not None else 'Continue')
long='The Legend of Zelda - Links Awakening DX';relative='Nintendo Game Boy (GB)/'+long+'.gb'
if phase in ['all','names']:
 fixture=out/(long+'.gb');subprocess.check_call([sys.executable,str(root/'scripts/android/audio-test-rom.py'),str(fixture)])
 sh('mkdir','-p',base+'/Nintendo Game Boy (GB)/Imgs',base+'/PlayStation',base+'/psp')
 run('push',str(fixture),base+'/'+relative)
 picture=Image.new('RGB',(360,500),'#33687a');draw=ImageDraw.Draw(picture);draw.rectangle((15,15,344,484),outline='white',width=5);draw.text((45,160),'QA HOMEBREW',fill='white');draw.text((45,200),'SAF BOX ART',fill='white')
 cover=out/'sample-cover.jpg';picture.save(cover,quality=85);run('push',str(cover),base+'/Nintendo Game Boy (GB)/Imgs/'+long+'.jpg')
 cheats=out/(long+'.cht');cheats.write_text('cheat0_desc = "QA RAM marker"\ncheat0_code = "010301C0"\n')
 run('push',str(cheats),base+'/Nintendo Game Boy (GB)/'+cheats.name)
 for w,h,orientation in [(720,1280,'portrait'),(1280,720,'landscape')]:
  sh('wm','size',f'{w}x{h}')
  for light in [False,True]:
   for scale in ['1.0','2.0']:
    sh('settings','put','system','font_scale',scale)
    for theme in ['shelf','big']:
     if os.environ.get('ANDROID03_NAME_FOCUS')=='1' and (orientation,light,scale,theme)!=('landscape',False,'2.0','shelf'):continue
     prefs(relative,theme=theme,light=light,cozy=True);wait(long)
     if orientation=='landscape' and theme=='shelf':
      painted=nodes();name_node=next(n for n in reversed(painted) if n.get('text')==long)
      just=next(n for n in painted if n.get('text')=='Just Play')
      bounds=lambda n:list(map(int,re.findall(r'\d+',n.get('bounds'))))
      shot(f'name-{orientation}-{light}-{scale}-{theme}')
      assert bounds(name_node)[1]>=bounds(just)[3],'The selected title overlaps Just Play at large text'
     else:shot(f'name-{orientation}-{light}-{scale}-{theme}')
 print('PASS: focused landscape 200% title/Just Play regression' if os.environ.get('ANDROID03_NAME_FOCUS')=='1' else 'PASS: long-name homebrew and local JPEG art, dark/light × portrait/landscape × 100%/200%',flush=True)
if phase in ['all','literal-names']:
 fixture=out/'Literal homebrew.gb';subprocess.check_call([sys.executable,str(root/'scripts/android/audio-test-rom.py'),str(fixture)])
 folder='Nintendo Game Boy (GB)';sh('mkdir','-p',base+'/'+folder)
 for title in ['Game Boy Color','Golden Sun']:
  run('push',str(fixture),base+'/'+folder+'/'+title+'.gb')
 sh('settings','put','system','font_scale','2.0')
 for w,h,orientation in [(720,1280,'portrait'),(1280,720,'landscape')]:
  sh('wm','size',f'{w}x{h}')
  for lang in ['de','es']:
   table=dict(l.split('\t',1) for l in (root/'assets/i18n'/f'{lang}.tsv').read_text().splitlines() if '\t' in l and not l.startswith('#'))
   for title in ['Game Boy Color','Golden Sun']:
    for theme in ['minimal','shelf']:
     prefs(folder+'/'+title+'.gb',language=lang,theme=theme,light=False,cozy=False)
     wait(title);wait(table['Settings'])
     shot(f'literal-{orientation}-{lang}-{title.replace(" ","-")}-{theme}')
     if theme=='shelf':
      tap(table['Info']);wait(title);shot(f'literal-box-{orientation}-{lang}-{title.replace(" ","-")}')
      key(4);wait(table['Play'])
    # The pause title is data too; surrounding actions must still use this language.
    prefs(folder+'/'+title+'.gb',language=lang,theme='shelf',light=False,cozy=False)
    wait(title);tap(table['Play']);started(table['Back']);key(4);wait(table['Save & quit']);wait(table['Options'])
    shot(f'literal-pause-{orientation}-{lang}-{title.replace(" ","-")}')
    tap(table['Controls']);wait(table['Back']);shot(f'literal-controls-{orientation}-{lang}-{title.replace(" ","-")}')
    key(4);wait(table['Save & quit'])
    tap(table['Save & quit'])
    for _ in range(15):
     if any('topResumedActivity=' in line and '/.PocketActivity' in line for line in sh('dumpsys','activity','activities').splitlines()):break
     time.sleep(.2)
    else:raise AssertionError('Localized Save & quit did not return to the launcher')
    wait(table['Play'])
 print('PASS: literal DE/ES phone rows, title bands, box and pause headers at 200%, portrait/landscape',flush=True)
if phase in ['all','starts']:
 sh('wm','size','720x1280');sh('settings','put','system','font_scale','1.0')
 prefs(relative,theme='shelf',light=False,cozy=True);wait(long)
 # A fresh cartridge start; no old Auto of this owned QA fixture.
 for p in states(long+'.pstate.auto*'):sh('rm',p)
 prefs(relative,theme='shelf',light=False,cozy=True);tap('Play')
 for i in range(7):shot(f'cart-start-{i:02}');time.sleep(.15)
 started();pause();shot('cart-playing-paused');tap('Options')
 for _ in range(12):
  if find('QA RAM marker',True) is not None:break
  sh('input','swipe','500','1050','500','350','180')
 if find('QA RAM marker · On') is not None:tap('QA RAM marker',True)
 wait('QA RAM marker · Off');tap('QA RAM marker',True);wait('QA RAM marker · On');shot('cheat-on');key(4);quit()
 prefs(relative,theme='shelf',cozy=False);play();pause();tap('Options')
 for _ in range(12):
  if find('QA RAM marker',True) is not None:break
  sh('input','swipe','500','1050','500','350','180')
 wait('QA RAM marker · On');tap('QA RAM marker',True);wait('QA RAM marker · Off');key(4);quit()
 # The optical console scene is real production drawing with the explicit non-emulating rehearsal seam.
 cue=out/'Sample Disc.cue';cue.write_text('FILE "Sample Disc.bin" BINARY\n TRACK 01 MODE2/2352\n  INDEX 01 00:00:00\n');track=out/'Sample Disc.bin';track.write_bytes(bytes(2352))
 run('push',str(cue),base+'/PlayStation/Sample Disc.cue');run('push',str(track),base+'/PlayStation/Sample Disc.bin')
 prefs('PlayStation/Sample Disc.cue',extra=('--ez','insertRehearsal','true'),theme='shelf',cozy=True);wait('Sample Disc');tap('Play')
 for i in range(10):shot(f'disc-scene-{i:02}');time.sleep(.12)
 # A supported larger-system start also reaches a real core, using the pinned homebrew hash.
 spec=next(g for g in json.loads((root/'scripts/android/core-smoke-roms.json').read_text()) if g['system']=='psp')
 psp=out/'Psp2048.pbp';psp.write_bytes(urllib.request.urlopen(spec['url'],timeout=30).read());assert hashlib.sha256(psp.read_bytes()).hexdigest()==spec['sha256']
 run('push',str(psp),base+'/psp/Psp2048.pbp')
 for p in states('Psp2048.pstate.auto*'):sh('rm',p)
 prefs('psp/Psp2048.pbp',theme='shelf',cozy=True);wait('Psp2048');tap('Play')
 for i in range(5):shot(f'psp-start-{i:02}');time.sleep(.15)
 started();pause();shot('psp-real-core-paused');quit()
 print('PASS: real GB cartridge, persisted cheats, optical PS1 scene rehearsal and real PSP core',flush=True)
if phase in ['all','faults']:
 prefs(relative,theme='big',cozy=False,light=False);play();pause();tap('Save ·',True)
 wait('Saved to slot 1');shot('manual-save-success')
 auto=next(p for p in sh('find',private+'/states','-type','f','-name',long+'.pstate.auto.z.taken').splitlines());folder=auto.rsplit('/',1)[0]
 # A missing optional thumbnail must not turn a durably saved state into a false failure.
 slot=auto.removesuffix('.auto.z.taken');preview=slot+'.png';parked=preview+'.qa-parked'
 sh('mv',preview,parked);sh('mkdir',preview)
 try:tap('Save ·',True);wait('Saved to slot 1');shot('state-saved-preview-unavailable')
 finally:sh('rmdir',preview);sh('mv',parked,preview)
 before={p:hashlib.sha256(read(p)).hexdigest() for p in sh('find',folder,'-type','f').splitlines()}
 sh('chmod','500',folder)
 try:
  tap('Load ·',True);wait("Couldn't load: Before load couldn't be saved. Your game is unchanged.");shot('load-undo-write-refused')
  tap('Save & quit');assert 'PlayerActivity' in sh('dumpsys','activity','activities');wait('Save & quit');shot('save-quit-write-refused')
  assert all(hashlib.sha256(read(p)).hexdigest()==digest for p,digest in before.items()),'A failed write changed an earlier moment'
 finally:sh('chmod','700',folder)
 quit();print('PASS: failed Before load and Save & quit keep the player open and preserve all earlier files',flush=True)
log=sh('logcat','-d');(out/'logcat.txt').write_text(log);assert 'Fatal signal' not in log and 'FATAL EXCEPTION' not in log
(out/('result-'+phase+'.txt')).write_text('PASS: '+phase+'; optical PS1 is scene rehearsal, GB and PSP use bundled cores.\n')
