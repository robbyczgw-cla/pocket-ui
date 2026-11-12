#!/usr/bin/env python3
"""Host regression checks for the real-device release journey's safety seams."""
import io
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tarfile
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parent/'brick'))
import journey
from nav import Frame


class NightDevice:
    """Recorded interfaces, with a real helper process for both LCD readbacks."""
    def __init__(self, root):
        self.root=root
        self.real=root/'real';self.logical=root/'logical'
        self.real.write_text('3');self.logical.write_text('3')
        self.status='v=1\tpid=44\tplace=pause:Main\tcover=none\tsleep=awake\tgame=paused\n'
        self.kernel='[1.0] boot\n';self.pstore='';self.capacity='80\n'
        self.boot='same';self.route=True;self.calls=[]

    def text(self, command, **kw):
        self.calls.append(command)
        if command=='cat /proc/sys/kernel/random/boot_id':return self.boot
        if command=='dmesg':return self.kernel
        if command=='md5sum /sys/fs/pstore/*':return self.pstore
        if command=='cat /proc/net/route':
            return 'Iface Destination\nwlan0 00000000\n' if self.route else 'Iface Destination\n'
        if command.endswith('/ui.status'):return self.status
        if command=='cat /sys/class/disp/disp/attr/sys':return 'lcd output backlight('+self.real.read_text()+')\n'
        if command==journey.APP+'/bin/brick-hw bl-get':
            return subprocess.check_output(['sh','-c','cat "$1"','fixture',str(self.real)]).decode()
        if command=='cat '+journey.HW+'/sys/class/backlight/panel/brightness':
            return subprocess.check_output(['sh','-c','cat "$1"','fixture',str(self.logical)]).decode()
        if '/capacity' in command:return self.capacity
        if command=='cat /sys/power/pm_wakeup_irq':return '344'
        if command=='cat /proc/interrupts':return '344: 0 0 0 0 wakeupgen 108 Level rtc\n'
        if command=='cat /sys/class/rtc/rtc0/since_epoch':return '1000'
        raise AssertionError('unexpected device read: '+command)

    def run(self, command, **kw):
        self.calls.append(command)
        if command in ('dmesg','md5sum /sys/fs/pstore/*','tar -cf - -C /sys/fs/pstore .'):return b'crash evidence'
        if '/wakealarm' in command:return b''
        raise AssertionError('unexpected device write: '+command)


class NightJourney(journey.Journey):
    def __init__(self, root):
        super().__init__(SimpleNamespace(out=root/'proof',label='night',phase='sleep',night_soak=True,
                         alarm_seconds=20,physical=False,bt=False,cycles=5,owner_watch_binary=Path(__file__)),NightDevice(root))
        self.ui=44;self.boot_id='same';self.cycles=[];self.last={key:0 for key,_ in self.COUNTERS}
        self.kernel_mark=0;self.commands=[];self.stage='pause';self.error_count=0

    def read(self, name):
        return '[1.0] boot\n' if name=='kernel-before' else ''

    def log(self):
        return ('sleep: suspending\nstate saved for Journey\nawake (still paused)\n'
                'sleep: awake on the Brick; the pause menu\nplay resumed\n')*len(self.commands)

    def reach(self,*args):return self.log()

    def cmd(self, key):
        self.commands.append(key)
        self.stage='power' if key=='powermenu' else 'pause'
        self.r.status=self.r.status.replace('cover=none','cover=power') if key=='powermenu' else self.r.status.replace('cover=power','cover=none')
        if key in ('dark','wake'):
            value='0' if key=='dark' else '3'
            self.r.real.write_text(value);self.r.logical.write_text(value)
            self.r.status=self.r.status.replace('sleep=awake','sleep=light') if key=='dark' else self.r.status.replace('sleep=light','sleep=awake')

    def record(self):return SimpleNamespace(text=lambda:'Sleep' if self.stage=='power' else 'Continue')
    def focus(self,*args):pass
    def shot(self,name):return name+'.jpg'
    def until(self,predicate,seconds=8):
        value=predicate()
        if not value:raise TimeoutError('fixture state absent')
        return value
    def audio_sample(self):
        self.audio_was_dark=self.r.real.read_text()=='0'
        return {'backend':{'alsa':{'xrun_errors':self.error_count,'write_errors':0,'recover_calls':0},'write_failures':0},
                'queue':{'playing_underruns':0,'playing_missing_frames':0,'dropped_frames':0}}


class NightSoak(unittest.TestCase):
    def test_early_input_restore_preserves_owner_core_fifo_inode_and_afterpark_discards_only_probe(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);device_tmp=root/'tmp';work=root/'session'
            device_tmp.mkdir();work.mkdir()
            owner=device_tmp/'pocket-core.cmd';os.mkfifo(owner);inode=owner.stat().st_ino
            script='W='+journey.Q(str(work))+'\n'+journey.core_restore(str(device_tmp))
            # Physical input can run restore before the park directory exists.
            subprocess.run(['sh'],input=script.encode(),check=True,capture_output=True)
            self.assertTrue(owner.is_fifo());self.assertEqual(owner.stat().st_ino,inode)
            parked=work/'parked';parked.mkdir();owner.rename(parked/owner.name)
            # Even a partial park is returned, without discarding unparked files.
            perf=device_tmp/'pocket-perf.lock';perf.write_text('owner lock')
            subprocess.run(['sh'],input=script.encode(),check=True,capture_output=True)
            self.assertEqual(owner.stat().st_ino,inode);self.assertEqual(perf.read_text(),'owner lock')
            owner.rename(parked/owner.name);perf.rename(parked/perf.name)
            (work/'parked-ready').touch();os.mkfifo(owner);probe_inode=owner.stat().st_ino
            subprocess.run(['sh'],input=script.encode(),check=True,capture_output=True)
            self.assertEqual(owner.stat().st_ino,inode)
            self.assertEqual((work/'discard/pocket-core.cmd').stat().st_ino,probe_inode)
            self.assertEqual(perf.read_text(),'owner lock')

    def test_owner_config_change_after_input_restore_is_preserved_and_reported(self):
        def archive():
            stream=io.BytesIO()
            with tarfile.open(fileobj=stream,mode='w') as ar:
                body=b'owner changed settings';item=tarfile.TarInfo('./settings.json');item.size=len(body)
                ar.addfile(item,io.BytesIO(body))
            return stream.getvalue()
        class Device:
            def text(self,command,**kw):
                if command.endswith('/owner-abort && echo abort'):return 'abort'
                if command.endswith('/restored && echo restored'):return 'restored'
                if command.startswith('test ! -e /proc/88'):return 'gone'
                if command.startswith('cat /proc/77/stat;'):return '77 (pocket-ui) S 0\n'
                if command.startswith('md5sum '):return 'protected\n'
                if command==journey.CLOCK_SNAPSHOT:return 'owner game clocks\n'
                if command.startswith('for f in ') and '/lights-before/' in command:return 'effect_lr\n'
                if command=='cat '+journey.APP+'/RELEASE':return '0.3.2'
                return ''
            def run(self,command,*args,**kw):
                if command.startswith('cp '):raise AssertionError('owner settings were overwritten')
                return b''
            def put(self,*args):raise AssertionError('owner settings were overwritten')
            def get(self,path,destination,**kw):destination.write_bytes(archive())
        with tempfile.TemporaryDirectory() as temp:
            j=journey.Journey(SimpleNamespace(out=Path(temp)/'proof',label='owner',phase='sleep',night_soak=True),Device())
            j.original=77;j.armed=True;j.created=True;j.input_watch_started=True
            j.owner_audio='/home/user/.asoundrc';j.protected='protected\n';j.pre_files={'./settings.json':b'old settings'}
            j.read=lambda name:'88' if name=='guard.pid' else ''
            (j.out/'clocks-before.txt').write_text('clocks\n')
            with patch.object(journey.time,'sleep'):j.cleanup()
            restore=json.loads((j.out/'restore.json').read_text())
            self.assertTrue(restore['owner_interrupted'])
            self.assertEqual(restore['owner_activity_config_changes'],['./settings.json'])
            self.assertEqual(restore['restored_files'],[])
            self.assertFalse(restore['cpu_policy_unchanged'])
            self.assertEqual(restore['light_control_differences'],['effect_lr'])

    def test_owner_waking_during_backup_blocks_takeover_without_a_control_write(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);status=root/'owner.status';real=root/'real';logical=root/'logical'
            helper=root/'app/bin/brick-hw';helper.parent.mkdir(parents=True)
            helper.write_text('#!/bin/sh\ncat '+journey.Q(str(real))+'\n');helper.chmod(0o700)
            pidof=root/'pidof';pidof.write_text('#!/bin/sh\n[ "$*" != pocket-ui ] || echo '+str(os.getpid())+'\n');pidof.chmod(0o700)
            real.write_text('0');logical.write_text('0')
            quiet=f'v=1\tpid={os.getpid()}\tplace=shelf:Home\tcover=none\tsleep=light\tgame=none\n'
            status.write_text(quiet)
            j=object.__new__(journey.Journey);j.work=str(root);j.original=os.getpid()
            def run_guard():
                command=j.night_owner_guard().replace('/tmp/pocket-ui.status',str(status)).replace(
                    journey.HW+'/sys/class/backlight/panel/brightness',str(logical))
                return subprocess.run(['sh','-c',command+'echo controlled >'+str(root/'control-write')],
                                      env=dict(os.environ,PATH=str(root)+':'+os.environ['PATH']),capture_output=True)
            with patch.object(journey,'APP',str(root/'app')):
                self.assertEqual(run_guard().returncode,0)
                (root/'control-write').unlink()
                # The first check passed; a real press wakes the owner during backup.
                for changed in (quiet.replace('sleep=light','sleep=awake'),quiet.replace('game=none','game=playing'),
                                quiet.replace('place=shelf:Home','place=settings:Groups'),quiet.replace('pid='+str(os.getpid()),'pid=1')):
                    status.write_text(changed)
                    self.assertNotEqual(run_guard().returncode,0)
                    self.assertFalse((root/'control-write').exists())
                status.write_text(quiet)
                for file in (real,logical):
                    file.write_text('3')
                    self.assertNotEqual(run_guard().returncode,0)
                    self.assertFalse((root/'control-write').exists())
                    file.write_text('0')
                (root/'owner-abort').touch()
                self.assertNotEqual(run_guard().returncode,0)
                self.assertFalse((root/'control-write').exists())

    def test_failed_prearm_stops_its_local_watcher_before_removing_scratch(self):
        class Local:
            abort_path=None
            def run(self,command,**kw):return subprocess.run(['sh','-c',command],capture_output=True,check=True).stdout
            def text(self,command,**kw):return self.run(command,**kw).decode()
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'brick-journey-fixture';root.mkdir()
            watcher=root/'owner-watch';watcher.write_text('import time\nfrom pathlib import Path\nPath(__file__).with_name("ready").touch()\ntime.sleep(60)\n')
            child=subprocess.Popen([sys.executable,str(watcher)])
            try:
                for _ in range(100):
                    if (root/'ready').exists():break
                    time.sleep(.01)
                self.assertTrue((root/'ready').exists())
                (root/'input-watch.pid').write_text(str(child.pid))
                j=object.__new__(journey.Journey);j.r=Local();j.work=str(root);j.armed=False
                j.start=time.monotonic()
                j.created=True;j.input_watch_started=True;j.owner_watch=None
                j.cleanup()
                child.wait(timeout=2)
                self.assertFalse(root.exists())
            finally:
                if child.poll() is None:child.terminate();child.wait(timeout=2)

    def test_owner_abort_restore_finishes_before_host_cleanup_can_change_files(self):
        class Restoring:
            abort_path=None;polls=0;finished=False
            def text(self,command,**kw):
                if command.endswith('/owner-abort && echo abort'):return 'abort'
                if command.endswith('/restored && echo restored'):
                    self.polls+=1;self.finished=self.polls==2
                    return 'restored' if self.finished else ''
                return 'gone'
            def run(self,command,**kw):
                assert self.finished,'host wrote while the input watcher was still restoring'
        j=object.__new__(journey.Journey);j.a=SimpleNamespace(night_soak=True)
        j.r=Restoring();j.work='/tmp/brick-journey-test';j.start=time.monotonic()
        j.armed=False;j.created=True;j.input_watch_started=True;j.owner_watch=None
        with patch.object(journey.time,'sleep'):j.cleanup()
        self.assertEqual(j.r.polls,2)
        self.assertFalse(j.created)

    def test_night_restore_keeps_zone_scales_colours_and_ignores_read_only_driver_info(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);before=root/'lights-before';lights=root/'leds'
            before.mkdir();lights.mkdir()
            saved={'effect_enable':b'0\n','anim_frames_enable':b'0\n','max_scale':b'22\n',
                   'max_scale_lr':b'17\n','max_scale_f1f2':b'13\n','effect_rgb_hex_l':b'010203 \n',
                   'effect_rgb_hex_lr':b'040506 \n','frame_hex':b'010203\0 040506\0 \n'}
            for name,value in saved.items():
                (before/name).write_bytes(value);(lights/name).write_bytes(b'0\n')
            (before/'effect_names').write_text('old information')
            (lights/'effect_names').write_text('current read-only driver information')
            script='W='+journey.Q(str(root))+'\n'+journey.night_led_restore(str(lights))
            result=subprocess.run(['sh'],input=script.encode(),check=True,capture_output=True)
            self.assertEqual(result.stderr,b'')
            self.assertEqual({name:(lights/name).read_bytes() for name in saved},
                             {name:value.replace(b'\0',b'') for name,value in saved.items()})
            self.assertEqual((lights/'effect_names').read_text(),'current read-only driver information')

    def test_black_lcd_fails_before_any_brightness_repair_and_keeps_evidence(self):
        with tempfile.TemporaryDirectory() as temp:
            j=NightJourney(Path(temp));j.r.real.write_text('0')
            with self.assertRaisesRegex(AssertionError,'backlight'):j.night_wake(1)
            self.assertEqual(j.r.real.read_text(),'0')
            self.assertFalse(j.commands)
            self.assertIn('backlight(0)',(j.out/'cycle-1-lcd.txt').read_text())
            self.assertIn('sleep=awake',(j.out/'cycle-1-status.txt').read_text())
            j.r.real.write_text('3');j.r.logical.write_text('0')
            with self.assertRaisesRegex(AssertionError,'backlight'):j.night_wake(2)

    def test_oversized_light_and_wrong_pause_state_are_rejected_without_repair(self):
        with tempfile.TemporaryDirectory() as temp:
            j=NightJourney(Path(temp));j.r.real.write_text('26')
            with self.assertRaisesRegex(AssertionError,'10%'):j.night_wake(1)
            j.r.real.write_text('3');j.r.status=j.r.status.replace('place=pause:Main','place=game')
            with self.assertRaisesRegex(AssertionError,'pause|shelf'):j.night_wake(2)

    def test_every_cycle_checks_route_sound_and_darkens_before_the_audio_wait(self):
        with tempfile.TemporaryDirectory() as temp, patch.object(journey.time,'sleep'):
            j=NightJourney(Path(temp));j.sleep_cycle(1)
            self.assertTrue(j.audio_was_dark)
            self.assertEqual(j.r.real.read_text(),'0')
            row=j.cycles[0]
            self.assertEqual(row['backlight'],{'real':3,'logical':3})
            self.assertTrue(row['route'])
            self.assertEqual(row['boot_id'],'same')
            self.assertEqual(row['battery_percent'],80)
            self.assertIn('dark',j.commands)
            self.assertTrue((j.out/'cycle-1-kernel-after.txt').exists())
            j.r.route=False
            with self.assertRaisesRegex((TimeoutError,AssertionError),'route|state'):j.sleep_cycle(2)

    def test_audio_failure_stops_the_cycle_instead_of_waiting_for_batch_end(self):
        with tempfile.TemporaryDirectory() as temp, patch.object(journey.time,'sleep'):
            j=NightJourney(Path(temp));j.error_count=1
            with self.assertRaisesRegex(AssertionError,'sound'):j.sleep_cycle(1)
            self.assertTrue(j.audio_was_dark)
            self.assertEqual(j.cycles,[])
            self.assertEqual(json.loads((j.out/'cycle-1-measurements.json').read_text())['delta']['xrun'],1)

    def test_missing_sound_counters_do_not_count_as_a_clean_cycle(self):
        with tempfile.TemporaryDirectory() as temp:
            j=NightJourney(Path(temp))
            with self.assertRaisesRegex(AssertionError,'sound counter unavailable'):j.counters({'backend':{}})

    def test_low_battery_pstore_and_real_oops_abort_but_known_wifi_warnings_are_counted(self):
        with tempfile.TemporaryDirectory() as temp:
            j=NightJourney(Path(temp))
            j.r.kernel+='[2.0] WARNING: CPU: 2 PID: 20 at net/core/skbuff.c:4383 skb_try_coalesce+0x1bc/0x2f8\n[2.1] mmc2: error -22 whilst initialising SDIO card\n'
            health=j.night_health(1,'before')
            self.assertEqual(health['known_wifi_warn'],1)
            self.assertEqual(health['known_wifi_sdio'],1)
            for field,value,reason in [('capacity','19\n','battery'),('pstore','new crash','pstore'),
                                       ('kernel','[2.0] Oops: bad memory','kernel'),
                                       ('kernel','[2.0] mmc1: error -22 reading SD card','kernel'),
                                       ('kernel','[2.0] WARNING: CPU: 1 at another_driver.c:44','kernel warning'),
                                       ('boot','rebooted','rebooted')]:
                previous=getattr(j.r,field);setattr(j.r,field,value)
                with self.assertRaisesRegex((AssertionError,RuntimeError),reason):j.night_health(2,'after')
                setattr(j.r,field,previous)
            self.assertTrue((j.out/'reset-pstore.tar').exists())

    def test_night_session_rejects_missing_guard_and_overlong_cycle_budget(self):
        for options in ({'owner_watch_binary':None},{'physical':True},{'phase':'game'}, {'cycles':100}):
            args=dict(night_soak=True,phase='sleep',physical=False,owner_watch_binary=Path(__file__),cycles=5,alarm_seconds=20)
            args.update(options)
            with self.assertRaises(ValueError):journey.validate_night_args(SimpleNamespace(**args))

    def test_reach_retries_only_transport_disconnect_not_owner_abort_or_command_errors(self):
        for code in (125,1):
            r=SimpleNamespace(text=lambda *a,code=code,**kw:(_ for _ in ()).throw(subprocess.CalledProcessError(code,'ssh')))
            j=SimpleNamespace(r=r)
            with self.assertRaises(subprocess.CalledProcessError):journey.Journey.reach(j,'read',10)
        replies=iter([subprocess.CalledProcessError(255,'ssh'),'back'])
        def reconnect(*args,**kw):
            value=next(replies)
            if isinstance(value,Exception):raise value
            return value
        with patch.object(journey.time,'sleep'):
            self.assertEqual(journey.Journey.reach(SimpleNamespace(r=SimpleNamespace(text=reconnect)),'read',10),'back')
        replies=iter([subprocess.TimeoutExpired('ssh',10),'back after timeout'])
        with patch.object(journey.time,'sleep'):
            self.assertEqual(journey.Journey.reach(SimpleNamespace(r=SimpleNamespace(text=reconnect)),'read',10),'back after timeout')
        with self.assertRaisesRegex(RuntimeError,'real failure'):
            journey.Journey.reach(SimpleNamespace(r=SimpleNamespace(text=lambda *a,**k:(_ for _ in ()).throw(RuntimeError('real failure')))),'read',0)
        with self.assertRaises(subprocess.TimeoutExpired):
            journey.Journey.reach(SimpleNamespace(r=SimpleNamespace(text=lambda *a,**k:(_ for _ in ()).throw(subprocess.TimeoutExpired('ssh',10)))),'read',0)


class Safety(unittest.TestCase):
    def test_legacy_ui_is_refused_before_any_device_mutation(self):
        class Device:
            def text(self,command,**kw):
                if command=='cat '+journey.APP+'/RELEASE':return '0.3.2'
                if command=='pidof pocket-ui':return '15502'
                if command=='pidof pocket-core retroarch':return ''
                if command.startswith('if grep -Fq POCKETUI_STATUS '):return ''
                raise AssertionError('unexpected read: '+command)
            def run(self,*args,**kw):raise AssertionError('legacy probe changed the device')
        with tempfile.TemporaryDirectory() as temp:
            old=Path(temp)/'old-ui';old.write_bytes(b'ELF old UI with global status only')
            status_only=Path(temp)/'status-only-ui';status_only.write_bytes(b'ELF POCKETUI_STATUS')
            for i,binary in enumerate((old,status_only,None)):
                j=journey.Journey(SimpleNamespace(out=Path(temp)/str(i),expect_version='0.3.2',phase='lights',binary=binary),Device())
                with self.assertRaisesRegex(journey.DeviceBusy,'private status'):j.prepare()
                self.assertFalse(j.created)
                self.assertFalse(j.armed)

    def test_current_local_and_installed_ui_can_use_private_status(self):
        class Device:
            def text(self,command):
                assert command.startswith('if grep -Fq POCKETUI_STATUS ')
                return 'supported\n'
        with tempfile.TemporaryDirectory() as temp:
            current=Path(temp)/'current-ui';current.write_bytes(b'ELF POCKETUI_STATUS POCKETUI_DARK_FILE')
            for binary in (current,None):
                j=object.__new__(journey.Journey);j.a=SimpleNamespace(binary=binary);j.r=Device()
                j.require_private_status()

    def test_probe_launch_cannot_overwrite_an_inherited_owner_status(self):
        with tempfile.TemporaryDirectory(prefix='brick status ') as temp:
            root=Path(temp);owner=root/'owner.status';owner.write_text('owner status\n')
            ui=root/'fake-ui'
            ui.write_text('#!/usr/bin/env python3\nimport os\nfrom pathlib import Path\nPath(os.environ["POCKETUI_STATUS"]).write_text("probe status\\n")\n')
            ui.chmod(0o700)
            j=object.__new__(journey.Journey);j.work=str(root)
            script=j.launch_script({'POCKETUI_STATUS':str(owner)},str(ui))
            subprocess.run(['sh'],input=script,check=True)
            self.assertEqual(owner.read_text(),'owner status\n')
            self.assertEqual((root/'ui.status').read_text(),'probe status\n')

    def test_probe_launch_cannot_consume_the_owners_brightness_note(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);owner=root/'owner.brightness';owner.write_text('150')
            ui=root/'fake-ui'
            ui.write_text('#!/usr/bin/env python3\nimport os\nfrom pathlib import Path\nPath(os.environ["POCKETUI_DARK_FILE"]).unlink(missing_ok=True)\n')
            ui.chmod(0o700)
            j=object.__new__(journey.Journey);j.work=str(root)
            (root/'ui.brightness').write_text('3')
            subprocess.run(['sh'],input=j.launch_script({'POCKETUI_DARK_FILE':str(owner)},str(ui)),check=True)
            self.assertEqual(owner.read_text(),'150')
            self.assertFalse((root/'ui.brightness').exists())

    def test_probe_key_guard_reads_only_its_own_status(self):
        calls=[]
        class Remote:
            def text(self,command):
                calls.append(command)
                return 'v=1\trev=1\tpid=8044\tplace=settings:Groups\n' if command=="cat '/tmp/private status/ui.status'" else 'v=1\trev=1\tpid=15502\t'
            def run(self,command):calls.append(command)
        j=object.__new__(journey.Journey);j.work='/tmp/private status';j.ui=8044;j.boot_id='same-boot';j.r=Remote()
        j.cmd('b')
        self.assertEqual(calls[0],"cat '/tmp/private status/ui.status'")
        self.assertEqual(len(calls),2)

    def test_explorer_does_not_mix_a_delayed_child_frame_into_the_parent_page(self):
        class Page:
            def __init__(self,name,at):self.name,self.at=name,at
            def controls(self):return [{'name':v} for v in {'root':['Look'],'look':['Layout'],'editor':['Split','Appearance']}[self.name]]
            def focused(self):return self.controls()[self.at]['name']
            def title(self):return {'root':'Settings','look':'Look','editor':'Choose a look'}[self.name]
            @property
            def hints(self):return [('A','Open')] if self.name!='editor' else [('A','Keep')]
            def sig(self):return self.name,self.at
        class UI:
            name='root';at=0;pending=None
            settings_press=journey.Journey.settings_press
            def record(self):return Page(self.name,self.at)
            def press(self,key):
                if key=='a':self.pending={'root':'look','look':'editor'}[self.name]
                elif key=='b':self.pending={'editor':'look','look':'root'}[self.name]
                elif key in ('up','down'):self.pending_at=max(0,min(len(self.record().controls())-1,self.at+(1 if key=='down' else -1)))
                else:assert key=='start'
            def timed_press(self,key,*args):
                self.press(key)
                if hasattr(self,'pending_at'):self.at=self.pending_at;del self.pending_at
                if self.pending:
                    self.name=self.pending;self.pending=None;self.at=int(self.name=='editor')
            def shot(self,name):return name+'.jpg'
            def step(self,name,action):return action()
        with tempfile.TemporaryDirectory() as temp:
            ui=UI();ui.out=Path(temp)
            journey.Journey.settings(ui)
            visits=__import__('json').loads((ui.out/'settings-walk.json').read_text())
            self.assertEqual({v['path'] for v in visits},{'Settings','Settings/Look','Settings/Look/Layout'})
            self.assertEqual(ui.name,'root')

    def test_reboot_guard_never_sends_a_key_to_the_replacement_ui(self):
        class Local:
            def run(self,command):return subprocess.run(['sh','-c',command],check=True,capture_output=True)
        with tempfile.TemporaryDirectory() as temp:
            target=Path(temp)/'input';target.write_text('owner input unchanged')
            ui=SimpleNamespace(r=Local(),boot_id='different-boot')
            with patch.object(journey,'FIFO',str(target)):
                with self.assertRaises(subprocess.CalledProcessError):journey.Journey.cmd(ui,'down')
                self.assertEqual(target.read_text(),'owner input unchanged')
                ui.boot_id=Path('/proc/sys/kernel/random/boot_id').read_text().strip()
                journey.Journey.cmd(ui,'b')
                self.assertEqual(target.read_text(),'b\n')

    def test_rebooted_cleanup_collects_evidence_without_device_mutations(self):
        class Remote:
            calls=[]
            def text(self,command):return 'new-boot\n'
            def run(self,command,**kw):
                self.calls.append(command)
                assert command in ('dmesg','tar -cf - -C /sys/fs/pstore .','md5sum /sys/fs/pstore/*')
                return b'evidence'
        with tempfile.TemporaryDirectory() as temp:
            j=object.__new__(journey.Journey);j.r=Remote();j.boot_id='old-boot';j.armed=True;j.out=Path(temp)
            with self.assertRaisesRegex(RuntimeError,'device rebooted'):j.cleanup()
            self.assertEqual(len(j.r.calls),3)
            self.assertEqual((j.out/'reset-pstore.tar').read_bytes(),b'evidence')

    def test_default_network_check_blocks_radio_cycle_and_only_checks_ota(self):
        calls=[]
        ui=SimpleNamespace(a=SimpleNamespace(wifi_cycle=False),results=[],
            shot=lambda name:name+'.jpg',flush=lambda:None,ota_check=lambda:'checked')
        ui.step=lambda name,action:calls.append((name,action()))
        journey.Journey.network(ui)
        self.assertEqual(ui.results[0]['status'],'BLOCKED')
        self.assertEqual(ui.results[0]['jpeg'],'01-wifi-off-on.jpg')
        self.assertEqual(calls,[('02-ota-check-only','checked')])

    def test_failed_radio_cycle_never_attempts_ota_navigation(self):
        class Remote:
            def put(self,*args):pass
            def text(self,*args,**kw):return ''
            def background(self,*args):pass
        calls=[]
        ui=SimpleNamespace(a=SimpleNamespace(wifi_cycle=True),r=Remote(),work='/tmp/private-test',
            step=lambda name,action:calls.append(name) or False)
        journey.Journey.network(ui)
        self.assertEqual(calls,['01-wifi-off-on'])

    def test_live_probe_truncation_is_retried_instead_of_reporting_an_empty_page(self):
        class UI:
            start=time.monotonic()
            until=journey.Journey.until
            replies=iter(['','partial','0\t0\t100\t20\tSettings\n'])
            def read(self,name):return next(self.replies)
        self.assertIn('Settings',journey.Journey.record(UI()).text())

    def test_scan_inserting_a_row_above_the_cursor_does_not_end_the_walk(self):
        class Page:
            hints=[]
            def __init__(self,ui):self.ui=ui
            def title(self):return 'Wi-Fi'
            def controls(self):return [{'name':v} for v in self.ui.rows]
            def focused(self):return self.ui.rows[self.ui.at]
        class UI:
            at=0;inserted=False
            rows=['Other network','Wi-Fi country','Scan again']
            def record(self):return Page(self)
            def timed_press(self,key,*args):
                if key=='down' and not self.inserted:
                    self.rows=['New network',*self.rows];self.inserted=True;self.at+=1
                elif key in ('up','down'):
                    self.at=max(0,min(len(self.rows)-1,self.at+(1 if key=='down' else -1)))
            def shot(self,name):return name+'.jpg'
            def step(self,name,action):return action()
        with tempfile.TemporaryDirectory() as temp:
            ui=UI();ui.out=Path(temp)
            journey.Journey.settings(ui)
            rows=__import__('json').loads((ui.out/'settings-walk.json').read_text())[0]['rows']
            self.assertEqual(rows,['New network','Other network','Scan again','Wi-Fi country'])

    def test_games_grid_reaches_the_adjacent_system_tile_without_service_keys(self):
        class Page:
            hints=[]
            def __init__(self,at):self.at=at
            def title(self):return 'Add your games'
            def controls(self):return [{'name':v} for v in ('Game Boy','All systems','From your phone')]
            def focused(self):return self.controls()[self.at]['name']
        class UI:
            at=0
            def timed_press(self,key,*args):
                if key!='start':self.press(key)
            def record(self):return Page(self.at)
            def press(self,key):
                assert key in ('down','up','left','right'),'a service was changed'
                self.at=({'down':2,'up':0,'right':1,'left':0}[key])
            def shot(self,name):return name+'.jpg'
            def step(self,name,action):return action()
        with tempfile.TemporaryDirectory() as temp:
            ui=UI();ui.out=Path(temp)
            journey.Journey.settings(ui)
            rows=__import__('json').loads((ui.out/'settings-walk.json').read_text())[0]['rows']
            self.assertEqual(rows,['All systems','From your phone','Game Boy'])

    def test_status_leaves_need_no_fake_focus_but_unknown_pages_still_fail(self):
        class Page:
            hints=[('A','Open')]
            def __init__(self,name):self.name=name
            def controls(self):return [{'name':'Playing' if self.name=='Settings' else 'Battery'}] if self.name in ('Settings','Playing') else []
            def focused(self):return self.controls()[0]['name'] if self.controls() else None
            def title(self):return self.name
            def sig(self):return self.name
        class UI:
            name='Settings';leaf='Battery'
            def record(self):return Page(self.name)
            def timed_press(self,key,*args):
                if key!='start':self.press(key)
            def settings_press(self,key):
                self.name=({'Settings':'Playing','Playing':self.leaf}[self.name] if key=='a' else {self.leaf:'Playing','Playing':'Settings'}[self.name])
            def press(self,key):assert key in ('up','down')
            def shot(self,name):return name+'.jpg'
            def step(self,name,action):return action()
        with tempfile.TemporaryDirectory() as temp:
            ui=UI();ui.out=Path(temp)
            journey.Journey.settings(ui)
            self.assertEqual(ui.name,'Settings')
            ui.leaf='Broken page'
            with self.assertRaisesRegex(AssertionError,'no D-pad focus'):
                journey.Journey.settings(ui)

    def test_settings_walk_reaches_rows_above_the_initial_choice_without_changing_them(self):
        class Page:
            hints=[]
            def __init__(self,at):self.at=at
            def focused(self):return ['Split','Appearance','Motion','Animations'][self.at]
            def controls(self):return [{'name':name} for name in ['Split','Appearance','Motion','Animations']]
            def title(self):return 'Choose a look'
        class UI:
            at=1
            def timed_press(self,key,*args):
                if key!='start':self.press(key)
            def record(self):return Page(self.at)
            def press(self,key):
                assert key in ('down','up'),'a choice was changed'
                self.at=max(0,min(3,self.at+(1 if key=='down' else -1)))
            def shot(self,name):return name+'.jpg'
            def step(self,name,action):return action()
        with tempfile.TemporaryDirectory() as temp:
            ui=UI();ui.out=Path(temp)
            journey.Journey.settings(ui)
            rows=__import__('json').loads((ui.out/'settings-walk.json').read_text())[0]['rows']
            self.assertEqual(rows,['Animations','Appearance','Motion','Split'])

    def test_empty_information_page_with_back_and_no_action_is_a_leaf(self):
        class Page:
            hints=[('B','Back')]
            def title(self):return 'Screenshots'
            def controls(self):return []
            def focused(self):return None
        # Exercise it as a child reached through the same nested walk helper.
        class RootPage:
            hints=[('A','Open')]
            def __init__(self,depth):self.depth=depth
            def title(self):return 'Settings' if self.depth==0 else 'Games'
            def controls(self):return [{'name':'Games' if self.depth==0 else 'Screenshots'}]
            def focused(self):return self.controls()[0]['name']
            def sig(self):return self.depth
        class UI:
            depth=0
            def timed_press(self,key,*args):
                if key!='start':self.press(key)
            def record(self):
                p=Page() if self.depth==2 else RootPage(self.depth)
                if self.depth==2:p.sig=lambda:2
                return p
            def settings_press(self,key):self.depth+=1 if key=='a' else -1
            def press(self,key):assert key in ('up','down')
            def shot(self,name):return name+'.jpg'
            def step(self,name,action):return action()
        with tempfile.TemporaryDirectory() as temp:
            ui=UI();ui.out=Path(temp)
            journey.Journey.settings(ui)
            self.assertEqual(ui.depth,0)

    def test_first_start_style_heading_is_recognized_at_its_lower_page_position(self):
        f=Frame('74\t92\t400\t141\tChoose a look\n73\t409\t150\t456\tSplit\n72\t218\t952\t458\t‹nav:row›\n72\t520\t198\t542\tAppearance\n72\t509\t560\t549\t‹nav:row›\n44\t509\t952\t549\t‹nav:focus›\n85\t699\t180\t720\tLeft/Right')
        self.assertEqual(f.title(),'Choose a look')
        self.assertEqual(f.focused(),'Appearance')

    def test_continue_above_the_returned_system_is_reachable_on_a_clamped_list(self):
        class UI:
            ui=1;at=3
            rows=['curator','search','recent:gb/Journey.gb','system:gb']
            def trace(self):return [dict(event='frame',pid=1,screen='shelf',sel=self.rows[self.at])]
            def press(self,key):self.at=max(0,min(len(self.rows)-1,self.at+(1 if key=='down' else -1)))
        ui=UI()
        frame=journey.Journey.select_trace(ui,lambda selected:selected.startswith('recent:'))
        self.assertEqual(frame['sel'],'recent:gb/Journey.gb')

    def test_cabinet_system_shelf_is_a_real_idle_shelf_but_a_details_card_is_not(self):
        self.assertTrue(journey.is_idle_shelf({'screen':'games'},'cabinet'))
        self.assertTrue(journey.is_idle_shelf({'screen':'shelf'},'split'))
        self.assertFalse(journey.is_idle_shelf({'screen':'games'},'split'))
        self.assertFalse(journey.is_idle_shelf({'screen':'card'},'cabinet'))
        class UI:
            ui=1;theme='cabinet';screen='card'
            def perf(self):pass
            def timed_press(self,key,screens,name):
                assert self.screen=='card','Cabinet B cannot go above its system shelf'
                self.screen='games'
            def trace(self):return [dict(event='frame',pid=1,screen=self.screen)]
            def idle(self):assert self.screen=='games'
        journey.Journey.perf_idle(UI())

    def test_restore_preserves_output_routes_while_allowing_the_night_dac_mute(self):
        before="numid=1,iface=MIXER,name='Headphone Switch'\n  : values=on\nnumid=2,iface=MIXER,name='DAC volume'\n  : values=160,160\n"
        silent=before.replace('160,160','0,0')
        self.assertEqual(journey.mixer_differences(before,silent),[])
        changed=journey.mixer_differences(before,silent.replace('values=on','values=off'))
        self.assertEqual(len(changed),1)
        self.assertIn('Headphone Switch',changed[0])

    def test_shelf_benchmark_keeps_existing_identity_and_picture_cache_without_settings(self):
        class Local:
            def run(self,command,**kw):subprocess.run(['sh','-c',command],check=True)
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);source=root/'owner';target=root/'scratch'
            source.mkdir();target.mkdir()
            (source/'identify').mkdir();(source/'snaps').mkdir()
            checksum=source/'identify/checksums.tsv';checksum.write_text('known ROM identity')
            __import__('os').utime(checksum,(1000000000,1000000000))
            (source/'snaps/game.png').write_bytes(b'original picture')
            (source/'settings.json').write_text('owner settings')
            journey.seed_read_caches(Local(),str(source),str(target))
            self.assertEqual((target/'identify/checksums.tsv').read_bytes(),checksum.read_bytes())
            self.assertEqual((target/'identify/checksums.tsv').stat().st_mtime,checksum.stat().st_mtime)
            self.assertEqual((target/'snaps/game.png').read_bytes(),b'original picture')
            self.assertFalse((target/'settings.json').exists())
            self.assertEqual((source/'settings.json').read_text(),'owner settings')

    def test_combined_measurement_leaves_details_and_the_game_list_for_home(self):
        class UI:
            ui=1
            theme='split'
            screen='card'
            def perf(self):pass
            def timed_press(self,key,screens,name):
                self.screen={'card':'games','games':'shelf'}[self.screen]
                assert self.screen in screens
            def trace(self):return [dict(event='frame',pid=1,screen=self.screen)]
            def idle(self):assert self.screen=='shelf','idle measured a game list'
        journey.Journey.perf_idle(UI())

    def test_restore_is_verified_before_the_owner_battery_writer_resumes(self):
        def archive(body):
            result=io.BytesIO()
            with tarfile.open(fileobj=result,mode='w') as ar:
                item=tarfile.TarInfo('./battery-history.bin');item.size=len(body)
                ar.addfile(item,io.BytesIO(body))
            return result.getvalue()
        class Device:
            body=b'before';running=False
            def get(self,path,destination,**kw):
                assert not self.running, 'backup read after owner resumed'
                destination.write_bytes(archive(self.body))
            def run(self,command,*args,**kw):
                if command.startswith('cd '+journey.CFG):return archive(self.body)
                if '/restore.sh ' in command:
                    self.running=True
                    self.body=b'normal new battery sample'
                return b''
            def text(self,command,**kw):
                if command==journey.CLOCK_SNAPSHOT:return 'clock-policy\n'
                if command.startswith('md5sum '):return 'protected\n'
                if command.startswith('cat /proc/77/stat;'):return '77 (pocket-ui) S 0\n'
                if command.startswith('test ! -e /proc/88'):return 'gone\n'
                if command=='cat '+journey.APP+'/RELEASE':return '0.3.0\n'
                return ''
        with tempfile.TemporaryDirectory() as temp:
            device=Device()
            j=journey.Journey(SimpleNamespace(out=Path(temp)/'result',label='restore',phase='idle'),device)
            j.original=77;j.armed=True;j.created=True;j.owner_audio='/home/user/.asoundrc';j.protected='protected\n'
            j.pre_files={'./battery-history.bin':b'before'}
            j.read=lambda name:'88' if name=='guard.pid' else ''
            (j.out/'clocks-before.txt').write_text('clock-policy\n')
            with patch.object(journey.time,'sleep'):
                j.cleanup()
            self.assertTrue(device.running)
            self.assertEqual(device.body,b'normal new battery sample')
            self.assertEqual(__import__('json').loads((j.out/'restore.json').read_text())['config_differences'],[])

    def test_game_cpu_profile_is_restored_without_touching_other_files(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            profile={'scaling_min_freq':'300000','scaling_max_freq':'1800000','scaling_governor':'ondemand'}
            (root/'clocks-before').write_text(''.join(f'{root/name}\t{value}\n' for name,value in profile.items()))
            for name in profile:(root/name).write_text('performance' if name.endswith('governor') else '1800000')
            (root/'unrelated').write_text('owner')
            subprocess.run(['sh','-c','W='+str(root)+'\n'+journey.CLOCK_RESTORE],check=True)
            self.assertEqual({name:(root/name).read_text().strip() for name in profile},profile)
            self.assertEqual((root/'unrelated').read_text(),'owner')

    def test_crashed_core_fails_but_a_game_named_crash_does_not(self):
        self.assertEqual(journey.fatal_log_lines('launching Crash Bandicoot\nloaded Crash Bandicoot.pstate\n'),[])
        for crash in ('core crashed with signal 11','SIGABRT','panic: renderer','fatal runtime error: stack overflow'):
            self.assertEqual(journey.fatal_log_lines(crash),[crash])

    def test_idle_cpu_uses_group_ticks_and_the_kernel_clock_rate(self):
        # /proc/stat's comm may contain spaces and parentheses. CPU is fields
        # 14+15, not the neighbouring fault counters or child CPU fields.
        fields=['S']+['0']*10+['30','7','999','888']+['0']*8
        stat='12 (pocket ui (worker)) '+' '.join(fields)
        self.assertEqual(journey.process_ticks(stat),37)
        for hz in (64,100,250):
            self.assertEqual(journey.clock_ticks(struct.pack('<QQQQ',17,hz,0,0)),hz)

    def test_foreign_game_or_wrong_release_blocks_before_device_mutation(self):
        class Preflight:
            def __init__(self,version,game):self.version,self.game=version,game
            def text(self,command,**kw):
                if command=='cat '+journey.APP+'/RELEASE':return self.version
                if command=='pidof pocket-ui':return '12'
                if command=='pidof pocket-core retroarch':return self.game
                raise AssertionError('unexpected device contact: '+command)
            def run(self,*args,**kw):raise AssertionError('device mutation before preflight')
        with tempfile.TemporaryDirectory() as temp:
            for i,(version,game) in enumerate((('0.3.0.1',''),('0.3.0','13'))):
                j=journey.Journey(SimpleNamespace(out=Path(temp)/str(i),expect_version='0.3.0'),Preflight(version,game))
                with self.assertRaises(journey.DeviceBusy):j.prepare()
                self.assertFalse(j.created)
                self.assertFalse(j.armed)

    def test_page_identity_ignores_the_top_right_size_and_battery_controls(self):
        f=Frame('900\t20\t970\t45\tAa\n820\t20\t890\t45\tFull\n60\t50\t500\t95\tYour games')
        self.assertEqual(f.title(),'Your games')

    def test_volume_changes_real_gain_and_restores_a_revisited_level(self):
        valid=[{'percent':20,'codec_raw':'27'},{'percent':25,'codec_raw':'26'},
               {'percent':30,'codec_raw':'25'},{'percent':25,'codec_raw':'26'}]
        self.assertEqual(journey.verify_volume_steps(valid),valid)
        for broken in ([{'percent':20,'codec_raw':'27'},{'percent':25,'codec_raw':'27'}],
                       valid+[{'percent':20,'codec_raw':'28'}]):
            with self.assertRaises(AssertionError):journey.verify_volume_steps(broken)

    def test_brick_software_present_uses_the_real_core_first_frame_ack(self):
        events=[dict(t=10,prog='pocket-ui',event='step',detail='first frame'),
                dict(t=20,prog='pocket-core',event='step',detail='process start'),
                dict(t=30,prog='pocket-core',event='step',detail='first frame')]
        self.assertIsNone(journey.game_frame(events[:2],0))
        self.assertEqual(journey.game_frame(events,0)['t'],30)
        self.assertIsNone(journey.game_frame(events,30))

    def test_scroll_rate_excludes_frames_outside_the_real_device_hold(self):
        events=[dict(pid=7,event='input',button='Right',down=True,t=1_000_000),
                dict(pid=7,event='input',button='Right',down=False,t=3_000_000)]
        for pid,t in ((7,500_000),(7,1_100_000),(7,2_100_000),(7,3_500_000),(8,2_500_000)):
            events.append(dict(pid=pid,event='frame',t=t))
            events.append(dict(pid=pid,event='paint',t=t,detail='12.5 1024x768'))
        result=journey.held_metrics(sorted(events,key=lambda e:e['t']),7,'Right',900_000)
        self.assertEqual(result['frames'],2)
        self.assertEqual(result['seconds'],2)
        self.assertEqual(result['fps'],1)
        self.assertEqual(result['interval_p95_ms'],1000)
        self.assertEqual(result['paint_p50_ms'],12.5)

    def test_python_and_shell_share_the_same_device_lock(self):
        with tempfile.NamedTemporaryFile() as lock:
            journey.fcntl.flock(lock,journey.fcntl.LOCK_EX|journey.fcntl.LOCK_NB)
            contender=subprocess.run(['flock','-n',lock.name,'true'])
            self.assertEqual(contender.returncode,1,'a shell device session overlapped a Python session')
            journey.fcntl.flock(lock,journey.fcntl.LOCK_UN)
            self.assertEqual(subprocess.run(['flock','-n',lock.name,'true']).returncode,0)

    def test_background_helper_survives_hup_without_a_nohup_applet(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            remote=journey.Remote.__new__(journey.Remote)
            remote.run=lambda command:subprocess.run(['sh','-c',command],capture_output=True,check=True).stdout
            # A delayed file is evidence that the child actually ran past HUP.
            remote.background("sh -c 'sleep .15; echo survived >"+str(root/'done')+"'",str(root/'log'),str(root/'pid'))
            pid=int((root/'pid').read_text())
            os=__import__('os'); signal=__import__('signal')
            os.kill(pid,signal.SIGHUP)
            for _ in range(40):
                if (root/'done').exists(): break
                time.sleep(.02)
            self.assertEqual((root/'done').read_text().strip(),'survived')

    def test_service_and_destructive_rows_are_never_activated_as_changes(self):
        for hint in ('Change','Connect','On','Off','Sign in','Toggle','Install',''):
            f=Frame('0\t0\t10\t10\t‹nav:hint›A\t'+hint)
            for row in ('Wi-Fi','Tailscale','Remote access','Bluetooth','Save sync','Previous version','Reset'):
                self.assertFalse(journey.may_open(f,row),(hint,row))
        f=Frame('0\t0\t10\t10\t‹nav:hint›A\tOpen')
        self.assertTrue(journey.may_open(f,'Look'))
        for row in ('Reset settings','Restore backup','Install update','Sign out','Password','Forget network'):
            self.assertFalse(journey.may_open(f,row),row)

    def test_evdev_press_has_one_syn_and_releases_the_same_key(self):
        for code in (116,317,318):
            rows=list(struct.iter_unpack('<qqHHi',journey.events(code)+journey.events(code,False)))
            self.assertEqual([(t,c,v) for _,_,t,c,v in rows],[(1,code,1),(0,0,0),(1,code,0),(0,0,0)])

    def test_backup_preserves_empty_files_and_names_with_spaces(self):
        data=io.BytesIO()
        with tarfile.open(fileobj=data,mode='w') as ar:
            for name,body in (('./pre-existing/hold','do not delete'),('./settings with spaces.json',''),('./moment.pstate','private progress')):
                member=tarfile.TarInfo(name); member.size=len(body.encode())
                ar.addfile(member,io.BytesIO(body.encode()))
        self.assertEqual(journey.file_map(data.getvalue()),{'./pre-existing/hold':b'do not delete','./settings with spaces.json':b'','./moment.pstate':b'private progress'})

    def test_rendered_but_unfocused_row_is_visible_to_the_explorer(self):
        f=Frame('''0\t0\t400\t40\tSettings
10\t100\t400\t150\t‹nav:row›
10\t100\t400\t150\t‹nav:focus›
20\t110\t90\t135\tAppearance
10\t160\t400\t210\t‹nav:row›
20\t170\t90\t195\tMotion
''')
        self.assertEqual(f.focused(),'Appearance')
        self.assertEqual({c['name'] for c in f.controls()},{'Appearance','Motion'})
        self.assertFalse(next(c for c in f.controls() if c['name']=='Motion')['focused'])

    def test_a_real_button_or_dpad_stops_the_session_before_any_further_guard_write(self):
        import owner_watch
        for kind, code, value in [(1, 305, 1), (1, 116, 1), (3, 16, -1), (3, 17, 1)]:
            watch = owner_watch.OwnerWatch(None, '/tmp/private')
            stream = io.BytesIO(owner_watch.EVENT.pack(0, 0, kind, code, value))
            with patch.object(owner_watch.os, 'kill') as stop:
                watch.read(SimpleNamespace(stdout=stream))
                stop.assert_called_once_with(owner_watch.os.getpid(), owner_watch.signal.SIGUSR2)
        watch.ending.set()
        with patch.object(owner_watch.os, 'kill') as stop:
            watch.read(SimpleNamespace(stdout=io.BytesIO(owner_watch.EVENT.pack(0, 0, 1, 305, 1))))
            stop.assert_not_called()

    def test_sync_release_and_neutral_axes_do_not_claim_the_owner_pressed(self):
        import owner_watch
        for kind, code, value in [(0, 0, 0), (1, 305, 0), (1, 305, 2), (3, 16, 0)]:
            self.assertFalse(owner_watch.pressed(owner_watch.EVENT.pack(0, 0, kind, code, value)))


if __name__=='__main__':
    unittest.main()
