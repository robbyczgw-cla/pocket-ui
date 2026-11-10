#!/usr/bin/env python3
"""Host regression checks for the real-device release journey's safety seams."""
import io
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


class Safety(unittest.TestCase):
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


if __name__=='__main__':
    unittest.main()
