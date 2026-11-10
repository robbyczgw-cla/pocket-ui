#!/usr/bin/env python3
"""Android 0.2 journeys on a disposable rootable AVD only.
Needs the APK installed, test/Gestures.java as /data/local/tmp/classes.dex and
NDK-built test/gamepad.c as /data/local/tmp/qa-gamepad. Uses generated GB fixtures
through the actual SAF picker. No real-device use, network downloads or release.
"""
import json, os, re, shlex, subprocess, sys, time, xml.etree.ElementTree as ET
from pathlib import Path
adb, serial, destination = sys.argv[1:]
out = Path(destination); out.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
pkg = 'dev.pocket.spike'; private = '/data/data/' + pkg + '/files'
def run(*args, **kwargs):
    kwargs.setdefault('timeout',30)
    return subprocess.check_output([adb, '-s', serial, *args], **kwargs)
def shell(*args): return run('shell', shlex.join(args), text=True)
def read(path): return run('exec-out', 'cat ' + shlex.quote(path))
assert serial.startswith('emulator-') and shell('getprop', 'ro.kernel.qemu').strip() == '1', 'Disposable AVD required'
run('root'); time.sleep(1); assert shell('id').startswith('uid=0')
shell('logcat','-c')
shell('cmd','media_session','volume','--stream','3','--set','0')
shell('settings','put','system','haptic_feedback_enabled','0')
sequence = 0
def tree():
    global sequence
    for _ in range(5):
        raw=run('exec-out','uiautomator dump /dev/tty',text=True)
        if '<?xml' in raw and '</hierarchy>' in raw:
            (out/f'ui-{sequence:03}.xml').write_text(raw); sequence += 1
            nodes=list(ET.fromstring(raw[raw.index('<?xml'):raw.index('</hierarchy>')+12]).iter('node'))
            got=next((n for n in nodes if n.get('text')=='Got it'),None)
            if got is not None:
                x1,y1,x2,y2=map(int,re.findall(r'\d+',got.get('bounds')))
                shell('input','tap',str((x1+x2)//2),str((y1+y2)//2));time.sleep(.3);continue
            return nodes
        time.sleep(.3)
    raise AssertionError('No semantic tree: '+raw)
def find(label, prefix=False):
    return next((n for n in reversed(tree()) if (n.get('text','').startswith(label) if prefix else n.get('text')==label) and n.get('enabled')=='true'),None)
def wait(label,prefix=False):
    for _ in range(12):
        n=find(label,prefix)
        if n is not None:return n
        time.sleep(.25)
    raise AssertionError('Missing '+label)
def tap(label,prefix=False):
    n=wait(label,prefix);x1,y1,x2,y2=map(int,re.findall(r'\d+',n.get('bounds')))
    shell('input','tap',str((x1+x2)//2),str((y1+y2)//2));time.sleep(.25)
def key(code):shell('input','keyevent',str(code));time.sleep(.25)
def shot_file(name): return out/(name+('.jpg' if os.environ.get('ANDROID_SMOKE_JPEG') == '1' else '.png'))
def shot(name):
    data=run('exec-out','screencap -p')
    if os.environ.get('ANDROID_SMOKE_JPEG') == '1':
        import io
        from PIL import Image
        picture=Image.open(io.BytesIO(data)).convert('RGB');picture.thumbnail((1280,1280))
        for quality in [80,65,50,35]:
            buffer=io.BytesIO();picture.save(buffer,format='JPEG',quality=quality,optimize=True)
            if buffer.tell()<=150_000:break
        assert buffer.tell()<=150_000
        shot_file(name).write_bytes(buffer.getvalue())
    else: shot_file(name).write_bytes(data)
def prefs(**changes):
    shell('am','force-stop',pkg)
    try: value=json.loads(read(private+'/launcher.json'))
    except (subprocess.CalledProcessError,ValueError): value={}
    value.update(changes)
    if os.environ.get('ANDROID_SMOKE_STYLE'):value['font']=os.environ['ANDROID_SMOKE_STYLE']
    p=out/'prefs.json';p.write_text(json.dumps(value));run('push',str(p),'/data/local/tmp/android-02-prefs.json')
    shell('cp','/data/local/tmp/android-02-prefs.json',private+'/launcher.json')
    shell('am','start','-W','-n',pkg+'/.PocketActivity');time.sleep(.6)
def settings_row(label):
    shell('input','swipe','500','300','500','1100','200')
    for _ in range(10):
        if find(label,True) is not None:tap(label,True);return
        shell('input','swipe','500','1050','500','350','250');time.sleep(.2)
    raise AssertionError('Missing setting '+label)
def play():
    tap('Continue' if find('Continue') is not None else 'Play')
    for _ in range(10):
        if find('Pause') is not None:return
        if find('Back') is not None:key(4)
        time.sleep(.3)
    raise AssertionError('No running game')
def pause():key(4);wait('Save & quit')
def quit():
    tap('Save & quit')
    for _ in range(30):
        activity=shell('dumpsys','activity','activities')
        if any('topResumedActivity=' in l and '/.PocketActivity' in l for l in activity.splitlines()):break
        time.sleep(.3)
    else:raise AssertionError('Save & quit did not return to the launcher')
    wait('Continue')
if os.environ.get('ANDROID02_COMPRESSED') == '1':
    selected='com.android.externalstorage.documents:primary:Download/PocketMvp/gb/Pocket Audio.gb'
    prefs(language='en',theme='big',light=False,selected_key=selected,selected_uri='',remind_where=False)
    play();pause();tap('Save ·',True);quit()
    auto=next(iter(shell('find',private+'/states','-type','f','-name','Pocket Audio.pstate.auto.z').splitlines()),None)
    assert auto is not None,'The Android writer did not create compressed Auto'
    encoded=read(auto);assert encoded.startswith(b'PCKSTZ\x01\0')
    slot=auto.removesuffix('.auto.z');raw=read(slot)
    assert raw and not raw.startswith(b'PCKSTZ\x01\0'),'Manual slots must remain raw'
    prefs();play();pause();assert read(auto+'.taken')==encoded,'Compressed Auto did not resume after process restart'
    shot('compressed-resumed');quit()
    shell('am','force-stop',pkg)
    # Use the player's own raw manual slot as an authentic old-version Auto fixture.
    # Keep app ownership: a root-owned 0600 copy is unreadable by the player.
    legacy=auto.removesuffix('.z');shell('cp','-p',slot,legacy)
    for suffix in ['.mark','.png']:
        shell('cp','-p',auto+suffix,legacy+suffix)
    assert shell('stat','-c','%u:%g',legacy)==shell('stat','-c','%u:%g',slot),'Legacy fixture lost app ownership'
    for p in shell('find',auto.rsplit('/',1)[0],'-type','f','-name','Pocket Audio.pstate.auto.z*').splitlines():shell('rm',p)
    shell('am','start','-W','-n',pkg+'/.PocketActivity');wait('Pocket Audio');play();pause()
    assert read(legacy+'.taken')==raw,'The upgrade did not resume the old raw Auto'
    shot('legacy-auto-resumed');quit()
    assert read(auto).startswith(b'PCKSTZ\x01\0'),'The next quit must use shared compressed Auto'
    assert read(slot)==raw,'Upgrading Auto changed the manual slot'
    (out/'result.txt').write_text('PASS: Android compressed Auto write/resume, old raw Auto upgrade and raw manual slot preservation.\n')
    print((out/'result.txt').read_text());sys.exit(0)
if os.environ.get('ANDROID02_EXTRA') == '1':
    selected='com.android.externalstorage.documents:primary:Download/PocketMvp/gb/Pocket Audio.gb'
    shell('wm','size','720x1280');shell('settings','put','system','font_scale','1.0')
    prefs(language='en',theme='big',selected_key=selected,selected_uri='',remind_where=False)
    tap('Settings');launcher_pid=shell('pidof',pkg).strip();shot('live-font-launcher-100')
    shell('settings','put','system','font_scale','2.0');time.sleep(.8);shot('live-font-launcher-200')
    assert launcher_pid==shell('pidof',pkg).strip(),'Font scale restarted the launcher'
    assert shot_file('live-font-launcher-100').read_bytes()!=shot_file('live-font-launcher-200').read_bytes(),'Idle launcher did not redraw for the font change'
    key(4);play();pause();player_pid=shell('pidof',pkg+':player').strip();shot('live-font-player-200')
    shell('settings','put','system','font_scale','1.0');time.sleep(.8);shot('live-font-player-100')
    assert player_pid==shell('pidof',pkg+':player').strip(),'Font scale restarted the player'
    assert shot_file('live-font-player-100').read_bytes()!=shot_file('live-font-player-200').read_bytes(),'Paused player did not redraw for the font change'
    quit()
    for width,height,orientation in [(720,1280,'portrait'),(1280,720,'landscape')]:
        shell('wm','size',f'{width}x{height}');shell('settings','put','system','font_scale','2.0')
        shell('cmd','overlay','enable','com.android.internal.display.cutout.emulation.tall')
        prefs(language='en',theme='big',selected_key=selected,selected_uri='',remind_where=False)
        play();pause();shot('cutout-'+orientation)
        for n in tree():
            if n.get('package')==pkg and n.get('text'):
                x1,y1,x2,y2=map(int,re.findall(r'\d+',n.get('bounds')))
                assert 0<=x1<x2<=width and 0<=y1<y2<=height,(n.attrib,width,height)
        tap('Zoom');wait('Zoom ·',True)
        shell('input','swipe','0',str(height//2),str(width//3),str(height//2),'300');wait('Save & quit')
        shell('input','swipe','0',str(height//2),str(width//3),str(height//2),'300');wait('Pause')
        pause();quit();shell('cmd','overlay','disable','com.android.internal.display.cutout.emulation.tall')
        for lang in ['de','es']:
            table=dict(l.split('\t',1) for l in (root/'assets/i18n'/f'{lang}.tsv').read_text().splitlines() if '\t' in l and not l.startswith('#'))
            for light in [False,True]:
                prefs(language=lang,light=light,theme='big',selected_key=selected,selected_uri='')
                tap(table['Continue']);wait(table['Pause']) if 'Pause' in table else wait('Pause')
                key(4);wait(table['Save & quit']);shot(f'{lang}-pause-'+orientation+('-light' if light else '-dark'))
                tap(table['Controls']);wait(table['Back']);shot(f'{lang}-controls-'+orientation+('-light' if light else '-dark'))
                key(4);tap(table['Zoom']);wait(table['Zoom']+' ·',True);shot(f'{lang}-zoom-'+orientation+('-light' if light else '-dark'))
                key(4);tap(table['Save & quit']);wait(table['Continue'])
    shell('settings','put','system','font_scale','1.0')
    (out/'result.txt').write_text('PASS: live font changes, real Back gestures, emulated cutouts and DE/ES pages at 200% fonts.\n')
    print((out/'result.txt').read_text());sys.exit(0)

if os.environ.get('ANDROID02_PSP_ROM'):
    import hashlib
    fixture=Path(os.environ['ANDROID02_PSP_ROM'])
    expected=next(g['sha256'] for g in json.loads((root/'scripts/android/core-smoke-roms.json').read_text()) if g['system']=='psp')
    assert hashlib.sha256(fixture.read_bytes()).hexdigest()==expected
    shell('wm','size','720x1280');shell('settings','put','system','font_scale',os.environ.get('ANDROID02_FONT_SCALE','1.0'))
    shell('mkdir','-p','/sdcard/Download/PocketMvp/psp');run('push',str(fixture),'/sdcard/Download/PocketMvp/psp/Psp2048.pbp')
    selected='com.android.externalstorage.documents:primary:Download/PocketMvp/psp/Psp2048.pbp'
    shell('am','force-stop',pkg)
    # Only this script's PSP fixture: a prior failed run may have left its short Auto behind.
    for p in shell('find',private+'/states','-type','f','-name','Psp2048.pstate*').splitlines():shell('rm',p)
    prefs(language='en',theme='big',light=os.environ.get('ANDROID02_LIGHT')=='1',selected_key=selected,selected_uri='',cozy=False,remind_where=False)
    wait('Psp2048');play();pause();quit()
    # The launcher must recover the real core's names from disk after a process restart.
    prefs();wait('Psp2048')
    tap('Psp2048');tap('Controls');wait('Back')
    assert any('Cross' in n.get('text','') for n in tree()),'The game card lost the running PSP core\'s button names'
    shot('psp-game-card-controls');key(4);key(4)
    shell('am','force-stop',pkg)
    states=shell('find',private+'/states','-type','f','-name','Psp2048.pstate.auto*').splitlines()
    state=next(p for p in states if not any(p.endswith(s) for s in ['.png','.mark','.battery-unsaved']))
    # The historical raw filename is distinct from the shared compressed namespace.
    folder=state.rsplit('/',1)[0]
    for p in states:shell('rm',p)
    legacy=folder+'/Psp2048.pstate.auto'
    tiny=out/'short-psp-moment';tiny.write_bytes(b'old short PSP moment')
    run('push',str(tiny),'/data/local/tmp/android-02-short-state');shell('cp','/data/local/tmp/android-02-short-state',legacy)
    shell('am','start','-W','-n',pkg+'/.PocketActivity');wait('Psp2048');tap('Continue' if find('Continue') is not None else 'Play')
    for _ in range(12):
        if any('This PSP moment could not be loaded safely.' in n.get('text','') for n in tree()):break
        time.sleep(.4)
    else:raise AssertionError('No clear PSP recovery explanation')
    shot('psp-recovery')
    key(4);wait('Save & quit');shot('psp-recovery-load-slot')
    kept=shell('find',folder,'-type','f','-name','*.pstate*').splitlines()
    assert any(read(p)==tiny.read_bytes() for p in kept),'The old PSP moment was not preserved'
    # Begin the fresh PSP game before its core can serialize a new moment.
    tap('Continue');wait('Pause');time.sleep(2);pause()
    # Save & quit must not replace that manual recovery slot.
    quit();assert any(read(p)==tiny.read_bytes() for p in kept if '.auto' not in p)
    (out/'result.txt').write_text('PASS: cached PSP game-card names survive process restart; old short PSP moment explained and retained by the actual Android player.\n')
    print((out/'result.txt').read_text());sys.exit(0)

# Sound and vibration stay disabled throughout; no real hardware is accessed.
shell('cmd','media_session','volume','--stream','3','--set','0');shell('settings','put','system','haptic_feedback_enabled','0')
shell('settings','put','system','font_scale','1.0')
shell('wm','size','720x1280');shell('wm','density','240')
shell('am','start','-W','-n',pkg+'/.PocketActivity');time.sleep(1)
rom=out/'Pocket Audio.gb';subprocess.check_call([sys.executable,str(root/'scripts/android/audio-test-rom.py'),str(rom)])
shell('mkdir','-p','/sdcard/Download/PocketMvp/gb');run('push',str(rom),'/sdcard/Download/PocketMvp/gb/Pocket Audio.gb')
prefs(onboarding_done=True,language='en',theme='big',light=False,cozy=False,haptics=False,remind_where=True,selected_key='com.android.externalstorage.documents:primary:Download/PocketMvp/gb/Pocket Audio.gb',selected_uri='')
# This disposable app has only QA games. Start their per-game toggles from Off.
shell('rm','-rf',private+'/android-controls')
key(34);subprocess.check_call([sys.executable,str(root/'scripts/android/select-demo-folder.py'),adb,serial,str(out)])
if find('Not now') is not None:tap('Not now')
tap('Games')
wait('Pocket Audio')
if os.environ.get('ANDROID02_SETUP_ONLY') == '1':
    (out/'result.txt').write_text('PASS: generated GB fixture selected through the real SAF folder picker.\n')
    sys.exit(0)
play();pause();quit()
# Every new page in portrait/landscape, dark/light, and Android's 200% font setting.
if os.environ.get('ANDROID02_SKIP_MATRIX') != '1':
    for width,height,orientation in [(720,1280,'portrait'),(1280,720,'landscape')]:
        shell('wm','size',f'{width}x{height}')
        for light in [False,True]:
            for scale in ['1.0','2.0']:
                shell('settings','put','system','font_scale',scale)
                prefs(light=light,theme='big');play();pause()
                suffix=orientation+('-light' if light else '-dark')+'-font'+scale
                shot('pause-'+suffix)
                tap('Controls');wait('Back');shot('controls-'+suffix);key(4);wait('Save & quit')
                tap('Zoom');wait('Zoom ·',True);shot('zoom-'+suffix)
                key(22);key(96);shot('zoom-pad-'+suffix);key(4);wait('Save & quit')
                tap('Options');wait('Back');shot('options-'+suffix);key(4);wait('Save & quit')
                # Nested Back never finishes or saves-and-quits by itself.
                assert 'PlayerActivity' in shell('dumpsys','activity','activities')
                quit()
print('PASS: pages, fonts, orientations and nested Back',flush=True)
if os.environ.get('ANDROID02_MATRIX_ONLY') == '1':
    (out/'result.txt').write_text('PASS: portrait/landscape × dark/light × 100%/200% fonts, Controls, Zoom, Options and nested Back.\n')
    sys.exit(0)
# Pinch opens the frozen picture and changes its crop; running frames never respond to it.
shell('wm','size','720x1280');shell('settings','put','system','font_scale','1.0');prefs(light=False);play();pause()
shell('env','CLASSPATH=/data/local/tmp/classes.dex','app_process','/system/bin','dev.pocket.qa.Gestures','360','350')
wait('Zoom ·',True);shot('pinch-zoom');key(4);wait('Save & quit');quit()
print('PASS: paused-frame pinch',flush=True)
# Backgrounding writes a moment and the game stays paused when returned through Recents.
play()
def fresh_auto():
    return [p for p in shell('find',private+'/states','-type','f','-name','Pocket Audio.pstate.auto*').splitlines()
            if p.endswith('.pstate.auto') or p.endswith('.pstate.auto.z')]
assert not fresh_auto(),'A resumed moment should already be taken before backgrounding'
key(3);time.sleep(.7)
assert fresh_auto(),'Home did not write a new moment; a taken moment from the previous start does not count'
key(187);time.sleep(.3)
# The task's thumbnail in Recents: centre, below its title.
shell('input','tap','360','620');wait('Save & quit');shot('background-paused');quit()
print('PASS: background pause and moment save',flush=True)
# A real Android input device appears/disappears: touch comes back and held input releases.
play();shell('mkfifo','/data/local/tmp/android-02-pad.cmd')
proc=subprocess.Popen([adb,'-s',serial,'shell','sh -c '+shlex.quote('/data/local/tmp/qa-gamepad <> /data/local/tmp/android-02-pad.cmd')],stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
    time.sleep(.5)
    if proc.poll() is not None:raise AssertionError('Gamepad probe failed: '+proc.stdout.read().decode())
    shell('sh','-c','echo "key 304 1" > /data/local/tmp/android-02-pad.cmd');time.sleep(.6)
    assert 'pocket-android-02 QA gamepad' in shell('dumpsys','input')
    shell('sh','-c','echo quit > /data/local/tmp/android-02-pad.cmd');proc.wait(timeout=10)
    wait('Save & quit');shot('gamepad-disconnected');tap('Continue');wait('Pause');pause();quit()
finally:
    if proc.poll() is None:
        shell('sh','-c','echo quit > /data/local/tmp/android-02-pad.cmd');proc.wait(timeout=10)
    shell('rm','/data/local/tmp/android-02-pad.cmd')
print('PASS: gamepad connect/disconnect',flush=True)
# The real Settings choice survives process restart; the player uses its per-game setting too.
prefs(theme='big');play();pause();tap('Options');
for _ in range(8):
    if find('Hold to toggle',True) is not None:break
    shell('input','swipe','500','1000','500','350','250')
wait('Hold to toggle',True)
tap('Hold to toggle',True);shot('hold-choice');key(4);quit()
# Recreate both launcher and player before checking the saved per-game choice.
prefs();play()
# A quick B tap on the touch pad leaves a visible held badge until tapped again.
tap('B');wait('B held');shot('hold-active');pause();tap('Continue');assert find('B held') is None;shot('hold-cleared-on-pause');pause();quit()
prefs(light=True);play();tap('B');wait('B held');shot('hold-active-light');pause();quit()
# The last played date is old only for this return. A successful resume holds the reminder for reading.
for light in [False,True]:
    value=json.loads(read(private+'/launcher.json'));played={k:int(time.time())-23*86400 for k in value.get('played',{})}
    prefs(played=played,remind_where=True,light=light);tap('Continue');time.sleep(.3);shot('where-was-i'+('-light' if light else '-dark'))
    assert 'Android reminder: Last played 3 weeks ago' in shell('logcat','-d','-s','PocketMvp:I')
    time.sleep(4)
    assert find('Pause') is None,'The reminder must wait for a press instead of timing out'
    key(4);wait('Pause');pause();quit()
# Battery saver suspends optional shelf animation/downloads, without changing the stored consent.
shell('dumpsys','battery','unplug');shell('cmd','power','set-mode','1');time.sleep(.6)
assert 'Android battery saver=true' in shell('logcat','-d','-s','PocketMvp:I')
shell('cmd','power','set-mode','0');shell('dumpsys','battery','reset')
log=shell('logcat','-d');(out/'logcat.txt').write_text(log)
assert 'FATAL EXCEPTION' not in log and 'Fatal signal' not in log
(out/'device.txt').write_text(shell('getprop')+shell('wm','size')+shell('wm','density'))
(out/'result.txt').write_text('PASS: Android 0.2 emulator journeys; inspect the screenshots. Hardware feel/performance remain untested.\n')
print((out/'result.txt').read_text())
