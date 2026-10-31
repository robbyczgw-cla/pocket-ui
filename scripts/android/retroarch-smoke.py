#!/usr/bin/env python3
"""Separately installed RetroArch fixture; test-only externalPlayer extra, actual SAF grant and SMS frame."""
import json,re,runpy,subprocess,sys,time,xml.etree.ElementTree as ET
from pathlib import Path
adb,serial,destination=sys.argv[1:];out=Path(destination);out.mkdir(parents=True,exist_ok=True)
root=Path(__file__).resolve().parents[2]
def run(*args,**kw):return subprocess.check_output([adb,'-s',serial,*args],**kw)
def shell(*args):return run('shell',*args,text=True)
assert serial.startswith('emulator-')and shell('getprop','ro.kernel.qemu').strip()=='1','Own emulator required'
assert 'package:'in shell('pm','path','com.retroarch'),'Prepare the separate official RetroArch fixture first'
rom=out/'sms-test.sms';rom.write_bytes(runpy.run_path(str(root/'scripts/testroms.py'))['sega_rom']('sms'))
shell('mkdir','-p','/sdcard/Download/PocketMvp/sms');run('push',str(rom),'/sdcard/Download/PocketMvp/sms/')
shell('am','force-stop','com.retroarch');shell('am','force-stop','dev.pocket.spike');shell('logcat','-c')
shell('am','start','-W','-n','dev.pocket.spike/.PocketActivity','--ez','externalPlayer','true');time.sleep(2)
shell('input','keyevent','34');time.sleep(1)
subprocess.check_call([sys.executable,str(root/'scripts/android/select-demo-folder.py'),adb,serial,str(out)]);time.sleep(2)
sequence=0
def tree():
    global sequence
    raw=run('exec-out','uiautomator','dump','/dev/tty',text=True)
    (out/f'ui-{sequence:03}.xml').write_text(raw);sequence+=1
    return list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>')+12]).iter('node'))
def tap_node(n):
    x1,y1,x2,y2=map(int,re.findall(r'\d+',n.get('bounds')));shell('input','tap',str((x1+x2)//2),str((y1+y2)//2));time.sleep(.4)
for _ in range(40):
    nodes=tree();node=next((n for n in nodes if n.get('text','').casefold()=='sms-test'),None)
    if node is not None:
        if node.get('selected')!='true':tap_node(node)
        break
    shell('input','keyevent','22');time.sleep(.3)
else:raise AssertionError('SMS fixture not in library')
nodes=tree();tap_node(next(n for n in nodes if n.get('text')=='Play'))
time.sleep(6)
(out/'retroarch-sms.png').write_bytes(run('exec-out','screencap','-p'))
log=shell('logcat','-d');activities=shell('dumpsys','activity','activities')
(out/'logcat.txt').write_text(log);(out/'activities.txt').write_text(activities)
assert 'launch profile=retroarch:gearsystem'in log,'Launcher did not select the external recipe'
assert 'Auto-start game "saf://'in log and 'sms-test.sms'in log,'RetroArch did not receive SAF content'
assert any('RetroActivityFuture'in line for line in activities.splitlines()if 'topResumedActivity='in line or 'mResumedActivity'in line),'RetroArch is not active'
assert 'FATAL EXCEPTION'not in log and 'Fatal signal'not in log
# Returning via our ordinary launcher intent must retain the shelf selection and clear the test-only override.
shell('am','start','-W','-n','dev.pocket.spike/.PocketActivity');time.sleep(2)
(out/'shelf-returned.png').write_bytes(run('exec-out','screencap','-p'))
assert any(n.get('text','').casefold()=='sms-test'and n.get('selected')=='true'for n in tree()),'Selection lost after external launch'
(out/'device.txt').write_text(shell('getprop')+shell('wm','size')+shell('wm','density'))
print('RetroArch SAF intent and preserved selection passed. Inspect the frame; receiver config/save persistence is separate evidence.')
