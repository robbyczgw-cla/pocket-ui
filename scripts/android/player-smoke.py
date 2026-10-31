#!/usr/bin/env python3
"""Own emulator only: real SAF, starter homebrew, our player, touch targets from live UI nodes, save/continue."""
import hashlib,json,os,re,subprocess,sys,time,urllib.request,xml.etree.ElementTree as ET
from pathlib import Path
adb,serial,out=sys.argv[1:];out=Path(out);out.mkdir(parents=True,exist_ok=True)
def run(*args,**kw):return subprocess.check_output([adb,'-s',serial,*args],**kw)
def shell(*args):return run('shell',*args,text=True)
assert serial.startswith('emulator-') and shell('getprop','ro.kernel.qemu').strip()=='1','Emulator required'
root=Path(__file__).resolve().parents[2]
entry=next(g for g in json.loads((root/'assets/starter-pack.json').read_text())['games'] if g['id']=='libbet')
rom=out/entry['file']['name_on_disk']
if not rom.exists():rom.write_bytes(urllib.request.urlopen(entry['file']['url'],timeout=30).read())
assert hashlib.sha256(rom.read_bytes()).hexdigest()==entry['file']['sha256']
shell('mkdir','-p','/sdcard/Download/PocketMvp/gb')
run('push',str(rom),'/sdcard/Download/PocketMvp/gb/')
subprocess.check_call([sys.executable,str(root/'scripts/android/audio-test-rom.py'),str(out/'Pocket Audio.gb')])
run('push',str(out/'Pocket Audio.gb'),'/sdcard/Download/PocketMvp/gb/')
shell('am','force-stop','dev.pocket.spike');shell('logcat','-c')
shell('am','start','-W','-n','dev.pocket.spike/.PocketActivity','--ez','diagnosticAudio','true');time.sleep(2)
shell('input','keyevent','34');time.sleep(1)
subprocess.check_call([sys.executable,str(root/'scripts/android/select-demo-folder.py'),adb,serial,str(out)])
time.sleep(2)
sequence=0
def tree():
 global sequence
 raw=run('exec-out','uiautomator','dump','/dev/tty',text=True)
 (out/f'ui-{sequence:03}.xml').write_text(raw);sequence+=1
 return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>')+12]).iter('node'))
def tap(label,selected=None):
 for _ in range(6):
  nodes=tree()
  node=next((n for n in reversed(nodes) if n.get('text')==label and n.get('enabled')=='true' and (selected is None or n.get('selected')==str(selected).lower())),None)
  if node is not None:
   x1,y1,x2,y2=map(int,re.findall(r'\d+',node.get('bounds')))
   shell('input','tap',str((x1+x2)//2),str((y1+y2)//2));time.sleep(.35);return
  time.sleep(.5)
 raise AssertionError('No live UI target: '+label)
def shot(name): (out/(name+'.png')).write_bytes(run('exec-out','screencap','-p'))
def key(*keys):shell('input','keyevent',*(str(k)for k in keys));time.sleep(.35)
def choose_game(label):
 key(*([21]*40))
 for _ in range(40):
  node=next((n for n in tree()if n.get('text')==label),None)
  if node is not None:
   if node.get('selected')!='true':tap(label)
   return
  key(22)
 raise AssertionError('Game not found on the shelf: '+label)
def confirm():
 if os.environ.get('ANDROID_TOUCH_ONLY')=='1':tap('Play')
 else:key(96)
def back():
 if os.environ.get('ANDROID_TOUCH_ONLY')=='1':
  node=tree()[0];x1,y1,x2,y2=map(int,re.findall(r'\d+',node.get('bounds')))
  shell('input','swipe',str(x1+1),str((y1+y2)//2),str(x1+(x2-x1)//4),str((y1+y2)//2),'300');time.sleep(.5)
 else:key(4)
# Select only when needed: touching an already-selected game correctly launches it.
# The primary-player/audio proof deliberately uses mGBA, independent of earlier alternative-core tests.
tap('Settings');key(*([19]*8));key(*([20]*5))
setting=next(n for n in tree()if n.get('text','').startswith('Game Boy core ·'))
if setting.get('text').endswith('Gambatte'):tap(setting.get('text'))
key(4)
choose_game('Libbet and the Magic Floor');confirm();time.sleep(3);shot('player-game')
(tap('Start') if os.environ.get('ANDROID_TOUCH_ONLY')=='1' else key(108));time.sleep(1)
# Inject Android GAMEPAD-source events, then return to touch from the pause menu.
shell('input','gamepad','keyevent','96');time.sleep(.5)
assert not any(n.get('text')=='Pause'for n in tree()),'Controller did not hide touch controls'
shot('player-controller-fullscreen')
shell('input','gamepad','keyevent','110');time.sleep(.5)
assert any(n.get('text')=='Continue'for n in tree()),'Controller Menu did not pause'
# A first touch after the controller only reveals the touch menu; the second one taps.
shot('player-controller-pause');tap('Continue');tap('Continue');time.sleep(.5)
assert any(n.get('text')=='Pause'for n in tree()),'Touch did not restore controls'
shot('player-touch-restored')
(tap('Pause') if os.environ.get('ANDROID_TOUCH_ONLY')=='1' else key(4));time.sleep(.4);shot('player-pause')
tap('Save · Slot 1');shot('player-saved')
tap('Load · Slot 1');shot('player-loaded')
tap('Options');shot('player-options');(back() if os.environ.get('ANDROID_TOUCH_ONLY')=='1' else key(97))
tap('Screenshot');shot('player-screenshot-saved')
tap('Quit');time.sleep(1);shot('launcher-after-quit')
confirm();time.sleep(2);shot('player-auto-resumed');back();tap('Quit');time.sleep(1)
choose_game('Pocket Audio');confirm();time.sleep(22);shot('player-audio-test');back();tap('Quit');time.sleep(1)
log=shell('logcat','-d');(out/'player-logcat.txt').write_text(log)
assert 'pocket-core in-app: mGBA' in log and 'player EGL:' in log
assert 'player state saved:' in log and 'player state loaded:' in log
assert 'resume=Resumed' in log, 'Auto state did not resume'
assert 'FATAL EXCEPTION' not in log and 'Fatal signal' not in log
assert 'AAudio write failed:' not in log and 'audio: the stream failed' not in log, 'Audio stream restarted during the smoke run'
(out/'device.txt').write_text(shell('getprop')+'\n'+shell('wm','size')+shell('wm','density'))
print('Own player, touch pause/save/load/options/screenshot, quit and auto-resume passed. Inspect PNGs and audio evidence separately.')
