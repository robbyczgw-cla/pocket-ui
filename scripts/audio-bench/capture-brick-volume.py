#!/usr/bin/env python3
"""Private hardware acquisition; run only on a handed-over, idle Brick. Password stays in its existing config file."""
import sys,os,signal,time,json,fcntl,shlex
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts/brick'))
from journey import Remote,SILENT_AMIXER,HW,APP
from owner_watch import OwnerWatch
import argparse
p=argparse.ArgumentParser(description='Measure the Brick codec curve silently, with exact mixer restore (no SPL calibration).')
p.add_argument('--host',required=True)
p.add_argument('--out',type=Path,required=True)
p.add_argument('--owner-watch-binary',type=Path,required=True)
a=p.parse_args()
out=a.out;out.mkdir(parents=True,exist_ok=False)
def interrupt(*_): raise RuntimeError('input or session limit: restoring')
for s in [signal.SIGALRM,signal.SIGTERM,signal.SIGUSR2]:signal.signal(s,interrupt)
with open('/tmp/brick-device.lock','w') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
 r=Remote(a.host)
 work='/tmp/brick-journey-volume-'+str(os.getpid());watch=None;armed=False
 signal.alarm(300)
 try:
  identity=r.text("cat /proc/device-tree/model; cat /sys/class/graphics/fb0/virtual_size; strings /usr/trimui/bin/MainUI | grep -i '^Trimui Brick$'; test ! -e /dev/i2c-3")
  assert 'sun50iw10' in identity and '1024,16384' in identity and 'Trimui Brick' in identity,identity
  assert not r.text('pidof pocket-core retroarch',check=False).strip(),'owner is playing'
  status=r.text('cat /tmp/pocket-ui.status');assert 'sleep=light' in status and 'game=none' in status,status
  r.run('mkdir '+work)
  watch=OwnerWatch(r,work,300);watch.start()
  snapshot="alsactl --file "+work+"/before store; cat /sys/class/speaker/mute >"+work+"/speaker; cat /proc/sys/kernel/random/boot_id; amixer cget 'name=digital volume'; amixer cget 'name=DAC volume'; cat /tmp/pocket-ui.status"
  (out/'before.txt').write_text(r.text(snapshot))
  r.get(work+'/before',out/'mixer-before.state')
  restore='''#!/bin/sh
W=$1
[ ! -e "$W/restored" ] || exit 0
touch "$W/stop"
alsactl --file "$W/before" restore
cat "$W/speaker" >/sys/class/speaker/mute
touch "$W/restored"
'''
  r.put(work+'/restore.sh',restore.encode())
  guard='''#!/bin/sh
W=$1;n=0
while [ ! -e "$W/stop" ] && [ "$n" -lt 260 ]; do
 [ "$(cat /sys/class/speaker/mute)" = 1 ] || echo 1 >/sys/class/speaker/mute
 amixer -q cset 'name=DAC volume' 0,0
 amixer -q cset 'name=HpSpeaker Switch' off
 amixer -q cset 'name=Headphone Switch' off
 sleep 1;n=$((n+1))
done
[ -e "$W/stop" ] || sh "$W/restore.sh" "$W"
'''
  r.put(work+'/guard.sh',guard.encode());r.put(work+'/silent-amixer',SILENT_AMIXER.encode())
  for name in ['volume','brick-volume-profile']:
   r.put(work+'/'+name,(ROOT/'deploy/brick/hwroot/usr/bin'/name).read_bytes())
  r.run('chmod 700 '+work+'/volume '+work+'/silent-amixer; mkdir '+work+'/state; echo 50 >'+work+'/state/brick-volume-curve3; echo 50 >'+work+'/state/brick-volume-curve2; echo 50 >'+work+'/state/brick-volume; echo new >'+work+'/state/brick-volume-compat')
  r.put(work+'/owner-watch',a.owner_watch_binary.read_bytes())
  r.run('chmod 700 '+work+'/owner-watch')
  r.background(work+'/owner-watch '+work+' 260',work+'/input-watch.log',work+'/input-watch.pid')
  r.abort_path=work+'/owner-abort'
  r.run("echo 1 >/sys/class/speaker/mute; amixer -q cset 'name=DAC volume' 0,0; amixer -q cset 'name=HpSpeaker Switch' off; amixer -q cset 'name=Headphone Switch' off")
  armed=True;r.background('sh '+work+'/guard.sh '+work,work+'/guard.log',work+'/guard.pid')
  samples=[]
  for pct in range(5,101,5):
   before=time.monotonic()
   cmd='BRICK_STATE='+work+'/state BRICK_AMIXER='+work+'/silent-amixer BRICK_VOLUME_COMPAT_CURVE=new '+work+'/volume '+str(pct)+"; amixer cget 'name=digital volume'; amixer cget 'name=DAC volume'; cat /sys/class/speaker/mute"
   raw=r.text(cmd,timeout=15)
   import re
   values=re.findall(r': values=([^\n]+)',raw)
   assert values[1]=='0,0' and raw.rstrip().endswith('1'),raw
   samples.append({'percent':pct,'codec_raw':int(values[0]),'dac_raw':values[1],'elapsed_ms':round((time.monotonic()-before)*1000,2)})
   print(samples[-1],flush=True)
  (out/'readbacks.json').write_text(json.dumps({'codec_step_db':1.16,'basis':'device codec readback, inverted vendor TLV step; DAC=0 and output switches off throughout, no SPL measurement','samples':samples},indent=2)+'\n')
 finally:
  r.abort_path=None
  if watch:watch.stop()
  if armed:
   r.run('touch '+work+'/stop; p=$(cat '+work+'/guard.pid); if [ -r /proc/$p/cmdline ] && tr "\\000" " " </proc/$p/cmdline | grep -Fq '+work+'/guard.sh; then kill "$p"; fi; sh '+work+'/restore.sh '+work)
   r.run('alsactl --file '+work+'/after store')
   r.get(work+'/after',out/'mixer-after.state')
   assert (out/'mixer-before.state').read_bytes()==(out/'mixer-after.state').read_bytes(),'mixer changed'
   (out/'after.txt').write_text(r.text('cat /sys/class/speaker/mute; cat '+work+'/speaker; cat /tmp/pocket-ui.status; p=$(cat '+work+'/guard.pid); test ! -e /proc/$p && echo guard-gone; ls /mnt/SDCARD/pocket-ui/storage/.config/pocket-ui/dev-ssh /mnt/SDCARD/pocket-ui/storage/.config/pocket-ui/no-autosuspend'))
  listing=r.text('ls -la '+work);(out/'scratch-inventory.txt').write_text(listing)
  r.run('rm -rf '+work)
  r.close();signal.alarm(0)
