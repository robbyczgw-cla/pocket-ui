#!/usr/bin/env python3
"""Brick release journey, run by device-check.sh --journey (host Python, real device).

Uses the existing FIFO, latency trace, nav-probe and grim seams. Each invocation
owns the shared host flock for at most nine minutes, with a device dead-man.
The installed UI is stopped by its exact PID; an isolated UI uses scratch data,
ROMs, SRAM and moments. Its original FIFO is parked and put back, inode intact.
No installer, service-row activation, reboot or power-off command is used.
"""
import argparse
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shlex
import signal
import struct
import subprocess
import sys
import tarfile
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import latency
import device_transfer
from nav import Frame

Q = shlex.quote
CARD = '/mnt/SDCARD/pocket-ui'
CFG = CARD + '/storage/.config/pocket-ui'
APP = CARD + '/app'
HW = '/tmp/pocket-ui/hwroot'
FIFO = '/tmp/pocket-ui.in'
CLOCK_SNAPSHOT = '''for p in /sys/devices/system/cpu/cpufreq/policy*; do
 for n in scaling_min_freq scaling_max_freq scaling_governor; do
  f=$p/$n; [ -r "$f" ] || continue
  printf '%s\t%s\n' "$f" "$(cat "$f")"
 done
done'''
CLOCK_RESTORE = '''while IFS="$(printf '\\t')" read -r f value; do
 [ "$(cat "$f")" = "$value" ] || printf '%s\\n' "$value" >"$f"
done <"$W/clocks-before"
'''
SILENT_AMIXER = '''#!/bin/sh
case "$*" in
 *cset*"name=HpSpeaker Switch"*) exec amixer -q cset 'name=HpSpeaker Switch' off ;;
 *cset*"name=Headphone Switch"*) exec amixer -q cset 'name=Headphone Switch' off ;;
 *cset*"name=DAC volume"*) exec amixer -q cset 'name=DAC volume' 0,0 ;;
 *) exec amixer "$@" ;;
esac
'''


class DeviceBusy(RuntimeError):
    """No mutation is authorized while somebody else's game is running."""


def events(code, down=True, kind=1):
    return b''.join(struct.pack('<qqHHi', 0, 0, t, c, v) for t, c, v in
                    [(kind, code, int(down)), (0, 0, 0)])


def file_map(blob):
    with tarfile.open(fileobj=io.BytesIO(blob)) as ar:
        return {m.name: ar.extractfile(m).read() for m in ar if m.isfile()}




def is_idle_shelf(frame, theme):
    # Cabinet has one real shelf per system; B has no level above that shelf.
    return frame['screen']=='shelf' or (theme=='cabinet' and frame['screen']=='games')


def mixer_differences(before, after):
    def controls(text):
        return {block.splitlines()[0]:block for block in re.split(r'(?=^numid=)',text,flags=re.M)
                if block.startswith('numid=') and "name='DAC volume'" not in block.splitlines()[0]}
    a,b=controls(before),controls(after)
    return sorted(name for name in a.keys()|b.keys() if a.get(name)!=b.get(name))


def seed_read_caches(remote, source, target):
    # A new scratch data directory must not turn a steady-shelf benchmark into
    # a background checksum scan of the entire ROM collection. Existing game
    # identity and picture caches keep their timestamps; settings stay private.
    folders=('covers','snaps','titles','identify','wikipedia','game-descriptions')
    for folder in folders:
        src=source+'/'+folder
        remote.run('if [ -d '+Q(src)+' ]; then cp -pR '+Q(src)+' '+Q(target)+'/; fi',timeout=45)


def percentile(values, p):
    return latency.percentile(values, p) if values else None


def process_ticks(stat):
    fields=stat.rsplit(') ',1)[1].split()
    return int(fields[11])+int(fields[12])


def clock_ticks(auxv):
    return next(value for kind,value in struct.iter_unpack('<QQ',auxv) if kind==17)


def fatal_log_lines(log):
    # Same distinction as device-check.sh: a ROM named Crash Bandicoot is not
    # a crash, but a terminated core must fail the journey as well as a UI panic.
    pattern=r'panicked at|panic:|segmentation fault|sigsegv|sigabrt|stack overflow|fatal (?:runtime )?error|\bcrashed\b|pocket-ui ended with [1-9]'
    return [line for line in log.splitlines() if re.search(pattern,line,re.I)]


def held_metrics(events, ui, button, after):
    down=next(e for e in events if e['pid']==ui and e['event']=='input' and e.get('button')==button and e.get('down') and e['t']>=after)
    up=next(e for e in events if e['pid']==ui and e['event']=='input' and e.get('button')==button and not e.get('down') and e['t']>down['t'])
    frames=[e for e in events if e['event']=='frame' and e['pid']==ui and down['t']<=e['t']<=up['t']]
    paints=[float(e['detail'].split()[0]) for e in events if e['event']=='paint' and e['pid']==ui and down['t']<=e['t']<=up['t']]
    intervals=[(b['t']-a['t'])/1000 for a,b in zip(frames,frames[1:])]
    seconds=(up['t']-down['t'])/1_000_000
    return {'frames':len(frames),'seconds':seconds,'fps':len(frames)/seconds,
            'interval_p50_ms':percentile(intervals,50),'interval_p95_ms':percentile(intervals,95),
            'interval_p99_ms':percentile(intervals,99),'interval_max_ms':max(intervals,default=None),
            'paint_p50_ms':percentile(paints,50),'paint_p95_ms':percentile(paints,95)}


def game_frame(events, after):
    # The Brick software-present path reports fast_switch's real first-frame
    # acknowledgement as a step; the GL path also emits core-frame.
    return next((e for e in events if e['t']>after and e.get('prog')=='pocket-core'
                 and (e['event']=='core-frame' or
                      (e['event']=='step' and e.get('detail')=='first frame'))),None)


def verify_volume_steps(values):
    # A displayed percentage changing is insufficient: the codec must follow,
    # and returning to a percentage must restore the same real attenuation.
    known={}
    for item in values:
        pct,raw=item['percent'],int(item['codec_raw'])
        assert 0<=raw<=63, f'invalid codec attenuation {raw}'
        for old_pct,old_raw in known.items():
            if pct==old_pct:
                assert raw==old_raw,'returning to a volume changed its gain'
            else:
                assert (pct-old_pct)*(raw-old_raw)<0,'volume keys did not change codec gain monotonically'
        known[pct]=raw
    return values


# Only enter subpages explicitly identified by the displayed A hint as Open.
# Changing a service is never justified by a translated/name substring match.
def may_open(frame, control):
    hint = dict(frame.hints).get('A', '')
    forbidden = re.search(r'reset|delete|remove|forget|install|restore|back.?up|sign|log.?out|password|pin\b|sleep|power|restart', control, re.I)
    return hint == 'Open' and not forbidden


class Remote:
    def __init__(self, host):
        self.temp = tempfile.TemporaryDirectory(prefix='brick-journey-ssh-')
        self.ssh = ['sshpass', '-f', str(Path.home()/'.config/pocket-brick/remote-password'),
                    'ssh', '-o', 'ConnectTimeout=8', '-o', 'ServerAliveInterval=5',
                    '-o', 'ServerAliveCountMax=3', '-o', 'PreferredAuthentications=password',
                    '-o', 'PubkeyAuthentication=no', '-o', 'ControlMaster=auto',
                    '-o', 'ControlPersist=30', '-o', 'ControlPath='+self.temp.name+'/sock', host]

    def run(self, command, data=None, timeout=20, check=True):
        p = subprocess.run(self.ssh+[command], input=data, capture_output=True, timeout=timeout)
        if check and p.returncode:
            raise subprocess.CalledProcessError(p.returncode, command, output=p.stdout, stderr=p.stderr)
        return p.stdout

    def text(self, command, **kw):
        return self.run(command, **kw).decode(errors='replace')

    def put(self, path, data):
        # Receive into RAM only; do not stream network data onto the SD card.
        device_transfer.upload(self.run, path, data)

    def get(self, path, destination, timeout=120):
        device_transfer.download(self.run, path, destination, timeout)

    def background(self, command, log, pidfile, errors=None):
        # Firmware BusyBox has no nohup applet. Ignore HUP before exec instead.
        stderr='2>'+Q(errors) if errors else '2>&1'
        self.run('(trap "" HUP; exec '+command+') </dev/null >'+Q(log)+' '+stderr+' & echo $! >'+Q(pidfile))

    def close(self):
        subprocess.run(self.ssh[:-1]+['-O', 'exit', self.ssh[-1]], capture_output=True, timeout=10)
        self.temp.cleanup()


class Journey:
    def __init__(self, args, remote):
        self.a, self.r = args, remote
        self.work = '/tmp/brick-journey-'+str(os.getpid())+'-'+str(int(time.time()))
        self.home = self.work+'/home'
        self.out = args.out
        self.out.mkdir(parents=True, exist_ok=False)
        (self.out/'host.pid').write_text(str(os.getpid())+'\n')
        self.results, self.metrics = [], {}
        self.original = None
        self.pre_files = {}
        self.armed = False
        self.created = False
        self.start = time.monotonic()
        self.trace_path = self.out/'trace.log'

    def read(self, name):
        return self.r.text('cat '+Q(self.work+'/'+name)+' 2>/dev/null', check=False)

    def trace(self):
        self.trace_path.write_text(self.read('trace.log'))
        t = latency.Trace(str(self.trace_path), fresh=False)
        t.poll()
        return t.events

    def log(self):
        return self.read('ui.log')

    def record(self):
        # The live probe uses truncate+write. A concurrent read can see an empty
        # file; that is no rendered empty page and cannot prove missing focus.
        def complete():
            raw=self.read('nav.txt')
            return Frame(raw) if raw.strip() and raw.endswith('\n') else None
        return self.until(complete)

    def until(self, predicate, seconds=8):
        end = time.monotonic()+seconds
        while time.monotonic() < end:
            if time.monotonic()-self.start > 480:
                raise TimeoutError('journey budget reached: restoring with a minute reserved')
            value = predicate()
            if value:
                return value
            time.sleep(.08)
        raise TimeoutError('expected state did not arrive')

    def cmd(self, key):
        # A reboot replaces both the original PID and FIFO. Never send the next
        # journey key to the freshly started public UI.
        guard='test "$(cat /proc/sys/kernel/random/boot_id)" = '+Q(self.boot_id)+' || exit 125; '
        self.r.run(guard+'printf "%s\\n" '+Q(key)+' >'+FIFO)

    def press(self, key, gap=.18):
        self.cmd(key)
        time.sleep(gap)
        return self.record()

    def shot(self, name):
        raw = self.r.run('SWAYSOCK=/tmp/pocket-ui/sway.sock BRICK_FB='+APP+'/bin/pocket-fb '+HW+'/usr/bin/grim -', timeout=8)
        target = self.out/(name+'.jpg')
        subprocess.run(['convert', '-', '-resize', '768x', '-quality', '82', str(target)], input=raw, check=True, capture_output=True, timeout=8)
        return target.name

    def step(self, name, action):
        result = {'step': name, 'status': 'FAIL'}
        try:
            detail = action()
            result.update(status='PASS', detail=detail)
        except Exception as e:
            result['detail'] = str(e)
        try:
            result['jpeg'] = self.shot(name)
        except Exception as e:
            result.update(status='FAIL', capture_error=str(e))
        self.results.append(result)
        self.flush()
        print(result['status'], name, result.get('detail', ''), flush=True)
        return result['status'] == 'PASS'

    def flush(self):
        (self.out/'result.json').write_text(json.dumps({'label': self.a.label, 'phase': self.a.phase,
            'metrics': self.metrics, 'steps': self.results}, indent=2, ensure_ascii=False)+'\n')

    def prepare(self):
        # The shared host lock is held by main before the first device contact.
        version=self.r.text('cat '+APP+'/RELEASE').strip()
        if self.a.expect_version and version!=self.a.expect_version:
            raise DeviceBusy(f'installed {version}, expected {self.a.expect_version}; no device files changed')
        self.original = int(self.r.text('pidof pocket-ui').strip())
        if self.r.text('pidof pocket-core retroarch', check=False).strip():
            raise DeviceBusy('owner game running; no device files changed')
        self.r.run('mkdir '+Q(self.work))
        self.created = True
        self.r.run('if [ ! -e '+CFG+'/no-autosuspend ]; then touch '+CFG+'/no-autosuspend; touch '+self.work+'/owned-inhibitor; fi')
        self.r.run('if [ ! -e '+CFG+'/dev-ssh ]; then touch '+CFG+'/dev-ssh; touch '+self.work+'/owned-ssh; fi')
        self.env = dict(x.split('=', 1) for x in self.r.run('cat /proc/'+str(self.original)+'/environ').decode().split('\0') if '=' in x)
        self.identity = self.r.text('cat '+APP+'/VERSION; cat '+APP+'/RELEASE; cat /proc/sys/kernel/random/boot_id; cat /proc/uptime')
        self.metrics['tested_build']={'installed_release':version,
            'ui_sha256':hashlib.sha256(self.a.binary.read_bytes()).hexdigest() if self.a.binary else None,
            'core_sha256':hashlib.sha256(self.a.core.read_bytes()).hexdigest() if self.a.core else None,
            'source':'local ARM executables in RAM' if self.a.binary else 'installed application'}
        (self.out/'identity.txt').write_text(self.identity)
        self.boot_id=self.identity.splitlines()[2]
        self.owner_audio=self.env['HOME'].rstrip('/')+'/.asoundrc'
        self.protected=self.r.text('md5sum '+APP+'/bin/pocket-ui '+APP+'/bin/pocket-core '+Q(self.owner_audio))
        (self.out/'protected-before.md5').write_text(self.protected)
        (self.out/'power-before.txt').write_text(self.r.text('cat /sys/class/power_supply/*/uevent; cat /sys/class/thermal/thermal_zone*/temp; cat /proc/meminfo; command -v perf; command -v strace',check=False))
        # Save the full config except downloaded art. Never copy the owner's ROMs.
        backup = self.config_archive(self.out/'config-before.tar')
        (self.out/'config-before.tar').chmod(0o600)
        self.pre_files = file_map(backup)
        self.r.run('set -e; alsactl --file '+self.work+'/mixer-before store; cat /sys/class/speaker/mute >'+self.work+'/speaker-before; dmesg >'+self.work+'/kernel-before; md5sum /sys/fs/pstore/* >'+self.work+'/pstore-before')
        for name in ('kernel-before','pstore-before'):
            (self.out/name).write_text(self.read(name))
        (self.out/'mixer-before.txt').write_text(self.r.text('amixer -c 0 contents'))
        clocks=self.r.text(CLOCK_SNAPSHOT)
        (self.out/'clocks-before.txt').write_text(clocks)
        self.r.put(self.work+'/clocks-before',clocks.encode())
        # Restore belongs to a device-side dead-man too, so losing SSH cannot leave
        # the owner's UI stopped or a guard alive. No wildcard cleanup of /tmp.
        restore = '''#!/bin/sh
W=$1
[ ! -e "$W/restored" ] || exit 0
[ ! -f "$W/ftrace-instance" ] || {
 t=$(cat "$W/ftrace-instance")
 echo 0 >"$t/tracing_on"
 echo 0 >"$t/events/sched/sched_wakeup/enable"
 rmdir "$t"
}
[ ! -f "$W/debugfs-mounted" ] || umount "$W/debug"
[ ! -f "$W/wifi.pid" ] || {
 p=$(cat "$W/wifi.pid")
 if [ -r /proc/$p/cmdline ] && tr '\\000' ' ' </proc/$p/cmdline | grep -Fq "$W/wifi.sh"; then kill -TERM "$p" 2>/dev/null; fi
}
[ ! -f "$W/wifi-radio" ] || {
 r=$(cat "$W/wifi-radio")
 [ ! -r "$W/wifi-soft-before" ] || cat "$W/wifi-soft-before" >"$r/soft"
}
# A dead-man also ends a private game and slider watcher if the host vanished.
# Only this session's exact private paths establish ownership.
for p in $(pidof pocket-core); do
 if [ -r /proc/$p/cmdline ] && tr '\\000' '\\n' </proc/$p/cmdline | grep -Fq "$W/.pocket-ui-test/"; then
  kill -TERM "$p" 2>/dev/null
 fi
done
[ ! -e "$W/slider.pid" ] || {
 p=$(cat "$W/slider.pid")
 if [ -r /proc/$p/environ ] && tr '\\000' '\\n' </proc/$p/environ | grep -Fxq "BRICK_STATE=$W/data"; then kill -TERM "$p" 2>/dev/null; fi
}
[ ! -e "$W/ui.pid" ] || {
 p=$(cat "$W/ui.pid")
 if [ -r /proc/$p/environ ] && tr '\\000' '\\n' </proc/$p/environ | grep -Fxq "POCKETUI_DATA=$W/data"; then kill -TERM "$p" 2>/dev/null; fi
}
sleep 1
if [ -p "$W/owner.fifo" ]; then rm -f /tmp/pocket-ui.in; mv "$W/owner.fifo" /tmp/pocket-ui.in; fi
mkdir -p "$W/discard"
for f in /tmp/pocket-core* /tmp/pocket-perf.lock; do
 [ -f "$f" ] || [ -p "$f" ] || continue
 mv "$f" "$W/discard/"
done
for f in "$W/parked/"*; do
 [ -e "$f" ] || [ -p "$f" ] || continue
 mv "$f" /tmp/
done
alsactl --file "$W/mixer-before" restore
# CPU profiles can change while a game runs. Restore only the known policy
# files captured before our UI started, and only when their value changed.
''' + CLOCK_RESTORE + '''
# Session ends silent even if it began with sound enabled.
amixer -q cset 'name=DAC volume' 0,0
echo 1 >/sys/class/speaker/mute
''' + 'kill -CONT '+str(self.original)+''' 2>/dev/null
''' + APP+'''/bin/brick-hw bl-set 0
''' + APP+'''/bin/brick-hw led-off
echo 0 >''' + HW+'''/sys/class/backlight/panel/brightness
[ ! -e "$W/owned-inhibitor" ] || rm -f '''+CFG+'''/no-autosuspend
[ ! -e "$W/owned-ssh" ] || rm -f '''+CFG+'''/dev-ssh
touch "$W/restored"
'''
        self.r.put(self.work+'/restore.sh', restore.encode())
        guard = '''#!/bin/sh
W=$1; n=0
while [ ! -e "$W/stop" ] && [ "$n" -lt 500 ]; do
 echo 1 >/sys/class/speaker/mute
 amixer -q cset 'name=DAC volume' 0,0
 '''+APP+'''/bin/brick-hw bl-set 3
 '''+APP+'''/bin/brick-hw led-off
 sleep 1; n=$((n+1))
done
[ -e "$W/stop" ] || sh "$W/restore.sh" "$W"
'''
        if self.a.phase=='game':
            # Both paths off stop the codec clock, which blocks a silent dmix writer.
            guard=guard.replace(' sleep 1;', " amixer -q cset 'name=Headphone Switch' on\n amixer -q cset 'name=HpSpeaker Switch' off\n sleep 1;")
        self.r.put(self.work+'/guard.sh', guard.encode())
        self.r.run('echo 1 >/sys/class/speaker/mute; amixer -q cset "name=DAC volume" 0,0')
        self.r.background('sh '+self.work+'/guard.sh '+self.work,self.work+'/guard.log',self.work+'/guard.pid')
        self.armed = True
        self.r.run('kill -0 $(cat '+self.work+'/guard.pid)')
        self.r.run('set -e; kill -STOP '+str(self.original)+'; mv '+FIFO+' '+self.work+'/owner.fifo')
        # Leave the owner's sleep state alone. The scratch process starts with a
        # dim, awake hardware view; no Power event or original-UI restart needed.
        self.r.run('echo 3 >'+HW+'/sys/class/backlight/panel/brightness; '+APP+'/bin/brick-hw bl-set 3')
        # Preserve pre-existing core FIFOs/files by rename, including their inode.
        # They go back BEFORE any session-created scratch directory is removed.
        self.r.run('mkdir '+self.work+'/parked; for f in /tmp/pocket-core* /tmp/pocket-perf.lock; do [ -f "$f" ] || [ -p "$f" ] || continue; mv "$f" '+self.work+'/parked/; done')
        self.r.run('mkdir -p '+self.work+'/data/console-settings '+self.work+'/states '+self.work+'/sram '+self.work+'/home '+self.work+'/.pocket-ui-test/gb')
        rom = self.a.rom.read_bytes()
        assert 16384 <= len(rom) <= 4*1024*1024, 'supply a small, redistributable GB test cartridge'
        self.r.put(self.work+'/.pocket-ui-test/gb/Journey.gb', rom)
        self.r.run('cp '+Q(self.owner_audio)+' '+Q(self.home+'/.asoundrc')+'; touch '+self.work+'/data/no-autosuspend')
        self.r.put(self.work+'/data/console-settings/gb.json', b'{"rumble":"off"}\n')
        self.r.put(self.work+'/data/brick-keys.json',b'{"f1":"save","f2":"load"}\n')
        self.r.put(self.work+'/data/haptics.json', b'{"menu":false}\n')
        self.r.put(self.work+'/data/sounds.json', b'{"on":false}\n')
        self.r.put(self.work+'/data/leds.json',b'{"mode":"off"}\n')
        # The inhibitor only blocks deep sleep. A six-minute screen-on sample
        # must also disable light sleep in this private data directory.
        self.r.put(self.work+'/data/sleep.json',b'{"screen_off_secs":0,"sleep_after_min":0,"off_after_min":0}\n')
        # Release builds check after 60 s; development builds do not. Keep the
        # benchmark on the shelf in either case. The Updates page still performs
        # its real manual check in the network phase.
        self.r.put(self.work+'/data/update-hint.json',b'{"off":true}\n')
        self.r.put(self.work+'/systems.cfg', ('<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>'+self.work+'/.pocket-ui-test/gb</path><extension>.gb</extension><command>pocket-core</command><platform>gb</platform><theme>gb</theme></system></systemList>').encode())
        core_target=APP+'/bin/pocket-core'
        if self.a.core:
            core_target=self.work+'/pocket-core'
            with self.a.core.open('rb') as stream:
                while chunk := stream.read(256*1024):
                    self.r.run('cat >>'+core_target,chunk)
                    time.sleep(.125)
            self.r.run('chmod 700 '+core_target)
        if self.a.binary:
            # Local ARM file -> RAM. Small bounded chunks avoid the WLAN/SD hazard.
            target=self.work+'/pocket-ui'
            with self.a.binary.open('rb') as stream:
                while chunk := stream.read(256*1024):
                    self.r.run('cat >>'+target, chunk)
                    time.sleep(.125)
            self.r.run('chmod 700 '+target)
        else:
            target=APP+'/bin/pocket-ui'
        env = {k:v for k,v in self.env.items() if k in ('PATH','LD_LIBRARY_PATH','WAYLAND_DISPLAY','SWAYSOCK','XDG_RUNTIME_DIR','POCKETUI_DEVICE','POCKETUI_HW_ROOT','POCKETUI_BACKLIGHT','POCKETUI_BATTERY','POCKETUI_POWER_KEY','POCKETUI_BRICK_STANDALONE','POCKETUI_TAILSCALED_SOCKET','POCKETUI_TAILSCALE','POCKETUI_TS_PROXY','BRICK_HW','BRICK_SLIDER_CFG','BRICK_SLIDER_MUTED','POCKET_CORE_SOFTWARE','BRICK_FB')}
        env.update(HOME=self.home, BRICK_STATE=self.work+'/data', POCKETUI_DATA=self.work+'/data',
                   POCKETUI_ES_SYSTEMS=self.work+'/systems.cfg', POCKETUI_STATES=self.work+'/states',
                   POCKET_CORE_SAVE_DIR=self.work+'/sram', POCKETUI_HOST_BIN=core_target,
                   POCKETUI_LANG='en', POCKETUI_TRACE=self.work+'/trace.log', TMPDIR=self.work,
                   POCKETUI_LOG=self.work+'/ui.log', POCKETUI_UPDATE_URL='https://pocketui.xyz/updates/brick/update.json')
        if self.a.phase not in ('perf','idle','perf-idle'):
            env['POCKETUI_NAV_PROBE']=self.work+'/nav.txt'
        if self.a.phase=='controls':
            self.r.put(self.work+'/silent-amixer',SILENT_AMIXER.encode())
            self.r.run('chmod 700 '+self.work+'/silent-amixer')
            env['BRICK_AMIXER']=self.work+'/silent-amixer'
            self.r.run('BRICK_STATE='+self.work+'/data BRICK_AMIXER='+self.work+'/silent-amixer '+HW+'/usr/bin/volume 20')
        if self.a.phase != 'first-start':
            self.r.put(self.work+'/data/settings.json', b'{"onboarded":true,"theme":"split","animations":"quick","motion":"off","art":"off","seen_version":"9.9.9"}\n')
        if self.a.phase in ('perf','idle','perf-idle'):
            # Same real library and look as the installed version, isolated writes.
            env['POCKETUI_ES_SYSTEMS']='/storage/.config/emulationstation/es_systems.cfg'
            settings=json.loads(self.pre_files.get('./settings.json', b'{}'))
            settings.update(onboarded=True,on_start='home',seen_version='9.9.9')
            self.theme=settings.get('theme','shelf')
            self.r.put(self.work+'/data/settings.json',(json.dumps(settings)+'\n').encode())
            seed_read_caches(self.r,CFG,self.work+'/data')
            self.metrics['read_caches']='existing identity, cover, screenshot, title and description caches; original timestamps preserved'
        self.r.put(self.work+'/launch.sh', ('#!/bin/sh\n'+''.join('export '+k+'='+Q(v)+'\n' for k,v in env.items())+'exec '+Q(target)+'\n').encode())
        if self.a.cold:
            self.r.run('sync; echo 3 >/proc/sys/vm/drop_caches')
        self.r.background('sh '+self.work+'/launch.sh',self.work+'/ui.log',self.work+'/ui.pid')
        self.until(lambda: any(e['event']=='frame' for e in self.trace()), 20)
        time.sleep(.7)
        self.ui=int(self.read('ui.pid'))
        (self.out/'clocks-running.txt').write_text(self.r.text(CLOCK_SNAPSHOT))

    def config_archive(self, destination):
        # Materialize once: retries must resume the same archive, not a fresh tar
        # stream whose contents/headers may have changed during a disconnect.
        path = self.work+'/config-transfer.tar'
        self.r.run('set -e; cd '+CFG+'; find . -type f ! -path "./covers/*" ! -path "./game-descriptions/*" >'+Q(self.work+'/config-files')+'; tar -cf '+Q(path)+' -T '+Q(self.work+'/config-files'), timeout=60)
        self.r.get(path, destination)
        self.r.run('rm -f '+Q(path))
        return destination.read_bytes()

    def cleanup(self):
        if not self.armed:
            if self.created:
                self.r.run('[ ! -e '+self.work+'/owned-inhibitor ] || rm -f '+CFG+'/no-autosuspend; [ ! -e '+self.work+'/owned-ssh ] || rm -f '+CFG+'/dev-ssh; rm -rf '+Q(self.work))
                self.created=False
            return
        self.check_boot()
        # A game is ours only when /proc names this session's private ROM.
        self.r.run('for p in $(pidof pocket-core); do if tr "\\000" "\\n" </proc/$p/cmdline | grep -Fq '+Q(self.work+'/.pocket-ui-test/')+'; then [ ! -p /tmp/pocket-core.cmd ] || echo "quit nosave" >/tmp/pocket-core.cmd; fi; done; true', timeout=10)
        time.sleep(1)
        # Restore changed original config files while its UI is still stopped.
        # No directory is removed and no unknown "extra" file is ever deleted.
        restored=[]
        try:
            current=file_map(self.config_archive(self.out/'config-compare.tar'))
            for name,body in self.pre_files.items():
                if current.get(name)!=body and name not in ('./dev-ssh','./no-autosuspend'):
                    self.r.put(self.work+'/restore-file',body)
                    self.r.run('cp '+self.work+'/restore-file '+Q(CFG+'/'+name))
                    restored.append(name)
            after_files=file_map(self.config_archive(self.out/'config-compare.tar'))
            marker_names={'./no-autosuspend','./dev-ssh'}
            changed=[k for k in self.pre_files.keys()|after_files.keys() if self.pre_files.get(k)!=after_files.get(k) and k not in marker_names]
        finally:
            self.r.run('touch '+self.work+'/stop; p=$(cat '+self.work+'/guard.pid); if [ -r /proc/$p/cmdline ] && tr "\\000" " " </proc/$p/cmdline | grep -Fq '+Q(self.work+'/guard.sh')+'; then kill "$p"; fi; sh '+self.work+'/restore.sh '+self.work, timeout=15)
        self.r.run('kill -0 '+str(self.original)+'; test -p '+FIFO+'; test -e '+self.work+'/restored')
        for name in ('ui.log','trace.log','nav.txt','guard.log','kernel-before','pstore-before'):
            (self.out/name).write_text(self.read(name))
        self.metrics['startup_steps_ms']=latency.startup_steps(str(self.out/'ui.log'))
        (self.out/'kernel-after').write_text(self.r.text('dmesg'))
        (self.out/'power-after.txt').write_text(self.r.text('cat /sys/class/power_supply/*/uevent; cat /sys/class/thermal/thermal_zone*/temp; cat /proc/meminfo',check=False))
        guard=int(self.read('guard.pid'))
        alive=self.r.text('test ! -e /proc/'+str(guard)+' && echo gone', check=False).strip()=='gone'
        restore={'comparison':'restored files hashed while original UI stopped; normal logging resumes afterwards','config_differences':changed,'guard_pid':guard,'guard_gone':alive,
                 'restored_files':restored,
                 'original_ui_pid':self.original,'installed_version':self.r.text('cat '+APP+'/RELEASE').strip()}
        protected_after=self.r.text('md5sum '+APP+'/bin/pocket-ui '+APP+'/bin/pocket-core '+Q(self.owner_audio))
        (self.out/'protected-after.md5').write_text(protected_after)
        restore['protected_files_unchanged']=protected_after==self.protected
        clocks_after=self.r.text(CLOCK_SNAPSHOT)
        (self.out/'clocks-after.txt').write_text(clocks_after)
        restore['cpu_policy_unchanged']=clocks_after==(self.out/'clocks-before.txt').read_text()
        end_state=self.r.text("cat /proc/"+str(self.original)+"/stat; cat /sys/class/speaker/mute; "+APP+"/bin/brick-hw bl-get; amixer cget 'name=DAC volume'; amixer cget 'name=HpSpeaker Switch'; amixer cget 'name=Headphone Switch'; pidof pocket-core retroarch; "+APP+"/bin/brick-hw led-info",check=False)
        (self.out/'end-state.txt').write_text(end_state)
        restore['owner_ui_running']=end_state.split(') ',1)[1].split()[0] not in ('T','Z','X')
        mixer_before=self.out/'mixer-before.txt'
        if mixer_before.exists():
            mixer_after=self.r.text('amixer -c 0 contents')
            (self.out/'mixer-after.txt').write_text(mixer_after)
            restore['mixer_control_differences_except_night_DAC']=mixer_differences(mixer_before.read_text(),mixer_after)
        (self.out/'restore.json').write_text(json.dumps(restore,indent=2)+'\n')
        assert not restore.get('mixer_control_differences_except_night_DAC',[]),'ALSA route/gain controls were not restored'
        assert not changed, f'configuration changed: {changed}; backup kept, scratch retained'
        assert alive, 'guard still alive'
        assert restore['owner_ui_running'],'original UI did not resume'
        assert restore['protected_files_unchanged'],'installed binaries or audio configuration changed'
        assert restore['cpu_policy_unchanged'],'CPU policy was not restored'
        assert not self.r.text('ls -A '+self.work+'/parked').strip(),'a pre-existing file is still parked; refusing cleanup'
        trace_left=self.r.text('if [ -f '+self.work+'/ftrace-instance ]; then t=$(cat '+self.work+'/ftrace-instance); [ ! -d "$t" ] || echo trace-present; fi; awk '+Q('$2=="'+self.work+'/debug" {print "debugfs-mounted"}')+' /proc/mounts')
        assert not trace_left.strip(),'private trace/debugfs still present; scratch retained for cleanup'
        # This exact directory was created by this instance. Never delete extras
        # based on a comparison with an inventory of somebody else's files.
        self.r.run('rm -rf '+Q(self.work))
        self.armed=False
        self.created=False

    def perf(self):
        ev=self.trace()
        first=next(e for e in ev if e['event']=='frame')
        start=next(e for e in ev if e['event']=='ui-start')
        self.metrics['process_to_first_frame_ms']=(first['t']-start['t'])/1000
        self.step('01-shelf',lambda: 'real scratch shelf presented')
        # A held D-pad creates the normal repeat events in the input supervisor.
        # Evdev ABS_HAT0Y, release always follows in finally.
        t0=int(self.r.text("awk '{printf \"%.0f\",$1*1000000}' /proc/uptime"))
        axis=16 if self.theme in ('cabinet','shelf','cards','big') else 17
        try:
            self.r.run('cat >/dev/input/event3', events(axis, True, 3))
            time.sleep(self.a.hold_seconds)
        finally:
            self.r.run('cat >/dev/input/event3', events(axis, False, 3))
        button='Right' if axis==16 else 'Down'
        def measured():
            try: return held_metrics(self.trace(),self.ui,button,t0)
            except StopIteration: return None
        self.metrics['held_dpad']=self.until(measured,3)
        self.metrics['held_dpad'].update(direction=button.lower(),look=self.theme)
        def held():
            assert self.metrics['held_dpad']['frames']>=3,'held D-pad produced too few real frames to measure'
            return self.metrics['held_dpad']
        self.step('02-held-dpad',held)
        def open_settings():
            self.timed_press('start',('settings',),'settings_open_ms')
            self.timed_press('b',latency.SHELF,'settings_close_ms')
            return self.metrics['settings_open_ms']
        self.step('03-settings-time',open_settings)
        def details():
            item=self.select_trace(lambda s:s.startswith(('game:','system:')))
            if item.get('sel','').startswith('system:'):
                self.timed_press('a',('games',),'open_system_ms')
                self.select_trace(lambda s:s.startswith('game:'))
            self.timed_press('x',('card',),'details_open_ms')
            return self.metrics['details_open_ms']
        self.step('04-details-time',details)

    def timed_press(self, key, screen, name):
        count=max((e.get('n',0) for e in self.trace() if e['event']=='input'),default=0)
        self.cmd(key)
        def arrived():
            ev=self.trace()
            inputs=[e for e in ev if e['event']=='input' and e.get('down') and e.get('n',0)>count and e['pid']==self.ui]
            if not inputs:
                return None
            inp=inputs[0]
            frames=[e for e in ev if e['event']=='frame' and e['pid']==self.ui and e.get('in',0)>=inp['n'] and e['screen'] in screen]
            if frames:
                self.metrics[name]=(frames[0]['t']-inp['t'])/1000
                return frames[0]
        return self.until(arrived,12)

    def first_start(self):
        def sees(text):
            return self.until(lambda: text in self.record().text())
        def advance(key,text):
            self.press(key)
            sees(text)
            return self.record().title()
        self.step('01-welcome',lambda: sees('Welcome'))
        self.step('02-wifi',lambda: advance('a','Wi-Fi'))
        # D-pad only to the displayed Continue row, never activate a network row.
        def wifi_continue():
            self.focus('Continue')
            self.press('a')
            self.until(lambda:any(s in self.record().text() for s in ('Your games','Several time zones here','Settings › System › Time has the rest.')),15)
            f=self.record()
            if 'Several time zones here' in f.text():
                self.press('b')
                self.focus('Continue')
                self.press('a')
            elif 'Your games' not in f.text():
                self.press('a')
            sees('Your games')
            return 'Wi-Fi service left unchanged; first start advanced'
        self.step('03-your-games',wifi_continue)
        self.step('04-look',lambda: advance('a','How your games stand'))
        def finish():
            self.press('a')
            self.until(lambda: any(e['event']=='frame' and e.get('screen') in latency.SHELF for e in self.trace()))
            saved=json.loads(self.read('data/settings.json'))
            assert saved['onboarded'], 'first start did not persist completion'
            return 'scratch setup finished and persisted'
        self.step('05-first-shelf',finish)

    def focus(self, name, limit=50):
        wanted=re.compile(name,re.I)
        # Enumerate full list, including offscreen rows. No A/Left/Right here.
        for _ in range(limit):
            f=self.record()
            if f.focused() and wanted.search(f.focused()):
                return f
            self.press('down')
        raise AssertionError(f'row {name!r} not reachable; focus {self.record().focused()!r}')

    def game(self):
        self.step('01-shelf',lambda: 'scratch Game Boy cartridge on shelf')
        def launch():
            item=self.select_trace(lambda selected:selected.startswith(('system:gb','game:gb/')))
            if item.get('sel','').startswith('system:'):
                self.timed_press('a',('games',),'game_system_open_ms')
                self.select_trace(lambda selected:selected.startswith('game:gb/'))
            before=max((e['t'] for e in self.trace()),default=0)
            mark=len(self.log())
            self.cmd('a')
            time.sleep(.3)
            if 'controls card: first start' in self.log()[mark:]:
                self.metrics['first_controls_card']=True
                self.shot('02-first-controls-card')
                self.cmd('a')  # The displayed first-console card: any button continues.
            fr=self.until(lambda: game_frame(self.trace(),before),25)
            confirmations=[e for e in self.trace() if e['pid']==self.ui and e['event']=='input' and e.get('down') and e['t']>before]
            if confirmations:self.metrics['game_confirm_to_frame_ms']=(fr['t']-confirmations[-1]['t'])/1000
            launches=[e for e in self.trace() if e['t']>before and e['event']=='launch']
            if launches:
                self.metrics['game_start_ms']=(fr['t']-launches[0]['t'])/1000
            return 'actual pocket-core first game frame'
        if not self.step('02-game',launch):
            return
        def pause():
            self.timed_press('home',('menu',),'pause_open_ms')
            return self.metrics['pause_open_ms']
        if not self.step('03-pause',pause):
            return
        def save():
            # Non-Retro pause rows have no nav focus marker on public 0.3.0.
            # A freshly opened menu selects Continue; one Down is the painted Save.
            assert 'Save' in self.record().text(),'pause has no Save row'
            self.press('down')
            self.press('a')
            self.until(lambda: 'saved '+self.work+'/states/gb/Journey.pstate' in self.log())
            return 'menu Save wrote a private moment'
        if not self.step('04-save',save):
            return
        def quit_game():
            self.timed_press('home',('menu',),'pause_again_ms')
            assert 'Save & quit' in self.record().text(),'pause has no Save & quit row'
            # Up wraps from the freshly opened Continue row to Quit.
            self.press('up')
            self.timed_press('a',latency.SHELF,'return_to_shelf_ms')
            self.until(lambda: 'pstate.auto' in self.log() and 'saved ' in self.log())
            return self.metrics['return_to_shelf_ms']
        if not self.step('05-quit',quit_game):
            return
        def resume():
            # Continue is the newly created recent at the top of Home.
            for _ in range(4): self.press('b')
            self.select_trace(lambda s:s.startswith('recent:') and 'Journey' in s)
            before=max(e['t'] for e in self.trace())
            self.cmd('a')
            fr=self.until(lambda: game_frame(self.trace(),before),20)
            inputs=[e for e in self.trace() if e['event']=='input' and e.get('down') and e['t']>before]
            if inputs: self.metrics['continue_ms']=(fr['t']-inputs[0]['t'])/1000
            self.until(lambda: '\nresumed' in self.log())
            return 'Continue loaded the auto moment and resumed'
        if not self.step('06-continue',resume):
            return
        def hotkey(code,pattern):
            mark=len(self.log())
            self.r.run('cat >/dev/input/event3',events(code)+events(code,False))
            self.until(lambda: pattern in self.log()[mark:])
            return f'evdev {code}: {pattern}'
        self.step('07-f1-save',lambda:hotkey(317,'saved '))
        self.step('08-f2-load',lambda:hotkey(318,'loaded '))

    def select_trace(self, predicate, limit=60):
        # List looks clamp at their ends. After returning from a console the
        # cursor stays on that system, below Continue, so Down alone cannot find it.
        for direction in ('down','up'):
            seen=set()
            for _ in range(limit):
                frame=next((e for e in reversed(self.trace()) if e['event']=='frame' and e['pid']==self.ui),None)
                if frame and predicate(frame.get('sel','')):
                    return frame
                where=(frame.get('screen'),frame.get('sel')) if frame else None
                if where in seen:break
                seen.add(where)
                self.press(direction)
        raise AssertionError('requested shelf entry not reachable')

    def settings_press(self,key):
        # Page transitions can finish after the nominal key gap. Read the probe
        # only after the frame that acknowledges this exact input was presented.
        self.timed_press(key,('settings','setup','manual'),'last_settings_navigation_ms')
        if key=='a' and self.record().title()=='Wi-Fi':
            # Initial scan results replace the provisional tail rows and cursor.
            # Let that bounded worker settle before collecting reachability.
            time.sleep(1)
        return self.record()

    def settings(self):
        self.timed_press('start',('settings',),'settings_open_ms')
        visited=[]
        def walk(path, depth=0):
            f=self.record()
            info_only=(dict(f.hints).get('B')=='Back' and 'A' not in dict(f.hints))
            if depth>=2 and (info_only or f.title() in ('Test buttons','Battery','The basics in 7 cards','Welcome')) and not f.focused() and not f.controls():
                # Informational/empty pages and the first Tour card have no rows. Their parent
                # still verifies the FIFO B returns to its original row; this
                # does not claim a physical controller test. Other pages without
                # focus fail, including layouts with missing navigation marks.
                shot=self.shot('settings-'+str(len(visited)+1))
                visited.append({'path':path,'rows':[],'status':'PASS','jpeg':shot})
                return
            reached=set()
            drawn=set()
            # Only this known system grid uses horizontal keys for focus. On
            # service/editor pages Left/Right could change a real setting.
            directions=('down','up','right','left') if f.title()=='Add your games' else ('down','up')
            for direction in directions:
                seen=set()
                for _ in range(60):
                    f=self.record()
                    controls=f.controls()
                    drawn.update(c['name'] for c in controls)
                    focused=f.focused()
                    assert focused, f'no D-pad focus at {path}'
                    # A network scan can insert a row above the cursor while
                    # Down keeps the same label selected. Only a repeated
                    # focus AND visible row set establish the end of a walk.
                    state=(focused,tuple(c['name'] for c in controls))
                    if state in seen:
                        break
                    seen.add(state)
                    fresh=focused not in reached
                    reached.add(focused)
                    if fresh and depth<2 and may_open(f,focused):
                        before=f.sig()
                        self.settings_press('a')
                        child=self.record()
                        if child.sig()!=before:
                            walk(path+'/'+focused,depth+1)
                            self.settings_press('b')
                            back=self.record()
                            assert back.focused()==focused and back.title()==f.title(), f'B did not restore {path}/{focused}'
                    self.timed_press(direction,('settings','setup','manual'),'last_settings_navigation_ms')
            assert not drawn-reached, f'unreachable rows at {path}: {sorted(drawn-reached)}'
            shot=self.shot('settings-'+str(len(visited)+1))
            visited.append({'path':path,'rows':sorted(reached),'status':'PASS','jpeg':shot})
        self.step('01-settings-walk',lambda: (walk('Settings'),visited)[1])
        (self.out/'settings-walk.json').write_text(json.dumps(visited,indent=2)+'\n')

    def perf_idle(self):
        self.perf()
        self.timed_press('b',latency.SHELF,'details_close_ms')
        frame=next(e for e in reversed(self.trace()) if e['event']=='frame' and e['pid']==self.ui)
        if not is_idle_shelf(frame,self.theme):
            self.timed_press('b',('shelf',),'home_for_idle_ms')
        self.idle()

    def idle(self):
        helper=Path(__file__).resolve().parents[1]/'baseline-device.sh'
        self.r.put(self.work+'/baseline.sh',helper.read_bytes())
        def on_shelf():
            frame=next(e for e in reversed(self.trace()) if e['event']=='frame' and e['pid']==self.ui)
            assert is_idle_shelf(frame,self.theme),'idle must measure the real shelf, not Details or a list submenu'
            self.metrics['idle_shelf']={'look':self.theme,'screen':frame['screen'],'selection':frame.get('sel')}
            return 'visible shelf, screen on at 3/255, sound off, no game'
        assert self.step('01-idle-shelf',on_shelf),'not on Home for idle measurement'
        time.sleep(15)  # Let startup workers settle before measuring steady idle.
        self.metrics['idle_temperature_before_mC']=self.temperatures()
        hz=clock_ticks(self.r.run('cat /proc/'+str(self.ui)+'/auxv'))
        trace=self.idle_trace_start()
        before=self.r.text('cat /proc/'+str(self.ui)+'/stat; cat /proc/uptime').splitlines()
        command='BASELINE_PID='+str(self.ui)+' TICKS='+str(hz)+' UILOG='+self.work+'/ui.log DATA_DIRS='+self.work+'/data BL='+HW+'/sys/class/backlight/panel/brightness sh '+self.work+'/baseline.sh idle '+str(self.a.idle_seconds)+' '+Q(self.a.label)
        raw=self.r.text(command,timeout=self.a.idle_seconds+20)
        after=self.r.text('cat /proc/'+str(self.ui)+'/stat; cat /proc/uptime').splitlines()
        self.metrics['idle_temperature_after_mC']=self.temperatures()
        seconds=float(after[1].split()[0])-float(before[1].split()[0])
        self.metrics['idle_cpu_pct']=(process_ticks(after[0])-process_ticks(before[0]))/hz/seconds*100
        self.metrics['idle_cpu_seconds']=seconds
        self.metrics['clock_ticks_per_s']=hz
        if trace:
            self.r.run('echo 0 >'+trace+'/tracing_on')
            wakeups=self.r.text('cat '+trace+'/trace')
            (self.out/'idle-wakeups.trace').write_text(wakeups)
            lost=re.search(r'entries-in-buffer/entries-written:\s*(\d+)/(\d+)',wakeups)
            assert not lost or lost[1]==lost[2],'wakeup trace overflowed'
            count=sum('sched_wakeup:' in line for line in wakeups.splitlines() if not line.startswith('#'))
            self.metrics['idle_wakeups_per_s']=count/seconds
            self.metrics['idle_wakeup_count']=count
        (self.out/'idle.txt').write_text(raw)
        self.metrics['idle']={k:v for line in raw.splitlines() if '=' in line for k,v in [line.split('=',1)]}
        assert self.metrics['idle']['pid']==str(self.ui),'idle helper measured the wrong UI process'
        assert self.metrics['idle']['window_state']=='awake-whole-window','idle sample crossed screen-off'
        assert self.metrics['idle']['backlight']=='3','idle screen was not at the requested level'
        status=self.metrics['idle'].get('battery_status')
        self.metrics['battery_discharge_pct_per_h']=(float(self.metrics['idle']['battery_pct_per_h'])
            if status=='Discharging' and self.metrics['idle'].get('battery_pct_per_h') not in (None,'n/a') else None)
        if status!='Discharging':
            self.metrics['battery_unmeasured_reason']='battery is '+str(status)+'; a charging percentage is not an idle discharge rate'
        self.step('02-idle-sample',lambda:self.metrics['idle'])

    def temperatures(self):
        raw=self.r.text('for d in /sys/class/thermal/thermal_zone*; do [ -r "$d/temp" ] || continue; printf "%s\\t%s\\t%s\\n" "${d##*/}" "$(cat "$d/type")" "$(cat "$d/temp")"; done')
        return [{'zone':zone,'type':kind,'mC':int(value)} for line in raw.splitlines()
                for zone,kind,value in [line.split('\t')]]

    def idle_trace_start(self):
        # A private trace instance preserves every pre-existing global trace and
        # buffer. Mount debugfs only below our own scratch directory if needed.
        command='''set -e
W='''+Q(self.work)+'''
T=/sys/kernel/debug/tracing
if [ ! -d "$T/instances" ]; then
 mkdir "$W/debug"
 mount -t debugfs none "$W/debug" || exit 1
 touch "$W/debugfs-mounted"
 T="$W/debug/tracing"
fi
test -d "$T/instances"
T="$T/instances/brick-journey-'''+str(os.getpid())+'''"
mkdir "$T"
echo "$T" >"$W/ftrace-instance"
echo 0 >"$T/tracing_on"
echo 256 >"$T/buffer_size_kb"
filter=
for p in /proc/'''+str(self.ui)+'''/task/*; do
 [ -z "$filter" ] || filter="$filter || "
 filter="${filter}pid == ${p##*/}"
done
echo "$filter" >"$T/events/sched/sched_wakeup/filter"
echo 1 >"$T/events/sched/sched_wakeup/enable"
echo 1 >"$T/tracing_on"
echo "$T"
'''
        try:
            trace=self.r.text(command).strip().splitlines()[-1]
            self.metrics['idle_wakeup_scope']='UI threads alive at sample start; later-created threads are excluded'
            return trace
        except RuntimeError as e:
            self.metrics['idle_wakeups_per_s']=None
            self.metrics['idle_wakeups_unmeasured_reason']=str(e)
            return None

    def controls(self):
        # Replay the GPIO seam through the REAL installed slider watcher. The
        # physical switch cannot be moved over SSH; BRICK_SYS redirects only its
        # input files, while the production volume helper drives the real mixer.
        self.r.run('mkdir -p '+self.work+'/gpio/sys/class/gpio/gpio243; echo 0 >'+self.work+'/gpio/sys/class/gpio/gpio243/value')
        self.press('home')
        prefix='BRICK_STATE='+self.work+'/data BRICK_AMIXER='+self.work+'/silent-amixer '
        volume=prefix+HW+'/usr/bin/volume '
        self.r.run(volume+'20')
        watcher=prefix+'BRICK_SYS='+self.work+'/gpio BRICK_SLIDER_CMD='+Q('env '+prefix+HW+'/usr/bin/volume')+' '+APP+'/bin/brick-hw slider-watch'
        self.r.background('env '+watcher,self.work+'/slider.log',self.work+'/slider.pid')
        try:
            def pin(value):
                self.r.run('echo '+str(value)+' >'+self.work+'/gpio/sys/class/gpio/gpio243/value')
            def mute():
                pin(1)
                self.until(lambda:self.r.text('test -e '+self.work+'/data/brick-muted && echo muted',check=False).strip()=='muted')
                before=self.read('data/brick-volume-curve3')
                self.press('volup')
                time.sleep(.3)
                assert self.read('data/brick-volume-curve3')==before,'keys changed remembered volume while muted'
                return 'real slider watcher/helper: mute holds remembered level; GPIO input replay'
            self.step('01-slider-mute',mute)
            def unmute():
                pin(0)
                self.until(lambda:self.r.text('test ! -e '+self.work+'/data/brick-muted && echo unmuted',check=False).strip()=='unmuted')
                assert self.read('data/brick-volume-curve3').strip()=='20'
                return 'remembered 20% restored; physical output remains inaudible'
            self.step('02-slider-unmute',unmute)
            def steps():
                values=[]
                for key,pct in (('volup',25),('volup',30),('voldown',25),('voldown',20),('voldown',15),('voldown',10),('voldown',5)):
                    self.press(key)
                    self.until(lambda:self.read('data/brick-volume-curve3').strip()==str(pct),3)
                    saved=self.read('data/brick-volume-curve3').strip()
                    assert saved==str(pct),(pct,saved)
                    raw=self.r.text("amixer -c 0 cget 'name=digital volume' | sed -n 's/^ *: values=//p'").strip()
                    values.append({'percent':pct,'codec_raw':raw})
                self.metrics['volume_steps']=values
                return verify_volume_steps(values)
            self.step('03-volume-steps',steps)
        finally:
            self.r.run('p=$(cat '+self.work+'/slider.pid); if tr "\\000" " " </proc/$p/cmdline | grep -q "brick-hw slider-watch"; then kill "$p"; fi; true')

    def network(self):
        if not self.a.wifi_cycle:
            # A real run reset the firmware kernel. A release run must stay red
            # rather than repeat that hazardous experiment without an opt-in.
            self.results.append({'step':'01-wifi-off-on','status':'BLOCKED',
                'detail':'known firmware reset during rfkill cycle; --wifi-cycle is an explicit hazardous opt-in',
                'jpeg':self.shot('01-wifi-off-on')})
            self.flush()
            print('BLOCKED 01-wifi-off-on known firmware reset; radio left unchanged',flush=True)
            self.step('02-ota-check-only',self.ota_check)
            return
        # Drop the radio briefly with an autonomous restore, rather than activating
        # arbitrary service rows. Existing network daemons keep their exact PIDs.
        script='''#!/bin/sh
W=$1
r=
for f in /sys/class/rfkill/rfkill*; do [ "$(cat "$f/type")" != wlan ] || r=$f; done
[ -n "$r" ] || exit 2
echo "$r" >"$W/wifi-radio"
cat "$r/soft" >"$W/wifi-soft-before"
[ "$(cat "$r/soft")" = 0 ] || exit 3
trap 'cat "$W/wifi-soft-before" >"$r/soft"' EXIT HUP INT TERM
echo 1 >"$r/soft"
sleep 4
cat "$r/soft" >"$W/wifi-off-result"
SWAYSOCK=/tmp/pocket-ui/sway.sock BRICK_FB='''+APP+'''/bin/pocket-fb '''+HW+'''/usr/bin/grim "$W/wifi-off.png"
echo 0 >"$r/soft"
for p in $(pidof udhcpc); do kill -USR1 "$p"; done
sleep 5
cat "$r/soft" >"$W/wifi-on-result"
'''
        self.r.put(self.work+'/wifi.sh',script.encode())
        before_daemons=self.r.text('pidof wpa_supplicant udhcpc',check=False).split()
        self.r.background('sh '+self.work+'/wifi.sh '+self.work,self.work+'/wifi.log',self.work+'/wifi.pid')
        def online():
            try:
                return self.read('wifi-on-result').strip()=='0'
            except (RuntimeError,subprocess.TimeoutExpired):
                return False
        def cycle():
            self.until(online,65)
            assert self.read('wifi-off-result').strip()=='1'
            routes=self.r.text('cat /proc/net/route').splitlines()[1:]
            assert any(line.split()[1]=='00000000' for line in routes),'default route did not return'
            assert sorted(self.r.text('pidof wpa_supplicant udhcpc',check=False).split())==sorted(before_daemons),'network daemon identity changed'
            raw=self.r.run('cat '+self.work+'/wifi-off.png')
            subprocess.run(['convert','-','-resize','768x','-quality','82',str(self.out/'wifi-off.jpg')],input=raw,check=True,timeout=8,capture_output=True)
            return 'radio soft-block 1 then 0; route returned; existing daemons retained'
        if not self.step('01-wifi-off-on',cycle):
            return  # No later input when the radio cycle may have reset the UI.
        self.step('02-ota-check-only',self.ota_check)

    def ota_check(self):
        self.press('start')
        self.focus('^System')
        self.press('a')
        self.focus('^Updates')
        self.press('a')
        self.until(lambda: any(text in self.record().text() for text in ('up to date','newest','available','published','Couldn’t',"Couldn't",'Could not','could not','No updates')),30)
        text=self.record().text()
        assert not any(x in text for x in ('Couldn’t',"Couldn't",'Could not','could not')),text
        assert 'up to date' in text or 'available' in text,'update check returned no usable manifest: '+text
        return {'channel':'https://pocketui.xyz/updates/brick/update.json','display':text,'installed':False}

    def check_boot(self):
        expected=getattr(self,'boot_id',None)
        if expected and self.r.text('cat /proc/sys/kernel/random/boot_id').strip()!=expected:
            # RAM backups/guards disappear on reboot. Preserve read-only evidence
            # on the host and leave recovery to the saved config-before.tar.
            for name,command in [('reset-dmesg.txt','dmesg'),('reset-pstore.tar','tar -cf - -C /sys/fs/pstore .'),
                                 ('reset-pstore-md5.txt','md5sum /sys/fs/pstore/*')]:
                (self.out/name).write_bytes(self.r.run(command,check=False))
            raise RuntimeError('device rebooted; RAM restore unavailable; recover from host config-before.tar; no further device writes')

    def crash_check(self):
        self.check_boot()
        now=self.r.text('dmesg')
        before=self.read('kernel-before')
        delta=now[len(before):] if now.startswith(before) else '\n'.join(line for line in now.splitlines() if line not in before.splitlines())
        assert not re.search(r'Kernel panic|Oops:|BUG:|mmc.*(?:error|CRC)|sunxi-mmc.*error',delta,re.I),delta[-1000:]
        assert not fatal_log_lines(self.log()),'application crash in session log: '+str(fatal_log_lines(self.log()))
        assert self.r.text('md5sum /sys/fs/pstore/*')==self.read('pstore-before'),'pstore changed'
        return 'no new app panic, kernel Oops/MMC error or pstore change'


def main():
    p=argparse.ArgumentParser(description=__doc__)
    host_file=Path.home()/'.config/pocket-ui/brick-host'
    p.add_argument('--host', default=os.environ.get('DC_HOST') or (host_file.read_text().strip() if host_file.exists() else None))
    p.add_argument('--rom',type=Path,default=Path.home()/'.config/pocket-ui/brick-smoke.gb')
    p.add_argument('--binary',type=Path)
    p.add_argument('--core',type=Path)
    p.add_argument('--out',type=Path,required=True)
    p.add_argument('--label',required=True)
    p.add_argument('--expect-version',help='refuse before mutation if the installed release differs')
    p.add_argument('--phase',choices=['all','first-start','perf','perf-idle','game','settings','network','controls','idle'],default='all')
    p.add_argument('--wifi-cycle',action='store_true',help='hazardous opt-in: a real rfkill cycle reset the Brick kernel; default stays BLOCKED')
    p.add_argument('--cold',action='store_true',help='drop file caches before process start; not a firmware cold boot')
    p.add_argument('--hold-seconds',type=float,default=10)
    p.add_argument('--idle-seconds',type=int,default=60)
    a=p.parse_args()
    for flag,path in (('--rom',a.rom),('--binary',a.binary),('--core',a.core)):
        if path is not None and not path.is_file():
            p.error(f'{flag}: file does not exist: {path}')
    if not a.host:
        p.error('set --host, DC_HOST, or ~/.config/pocket-ui/brick-host')
    if not 5<=a.idle_seconds<=360 or not 1<=a.hold_seconds<=30:
        p.error('idle must be 5..360 seconds, D-pad hold 1..30 seconds')
    if a.phase=='all':
        # The controller owns no lock. Each bounded child takes it independently,
        # allowing the other device users between phases of a release check.
        a.out.mkdir(parents=True,exist_ok=False)
        rows=[]
        for phase in ('first-start','perf','game','controls','settings','network','idle'):
            command=[sys.executable,__file__,'--host',a.host,'--rom',str(a.rom),
                     '--out',str(a.out/phase),'--label',a.label,'--phase',phase,
                     '--idle-seconds',str(a.idle_seconds),'--hold-seconds',str(a.hold_seconds)]
            for flag,value in (('--binary',a.binary),('--core',a.core),('--expect-version',a.expect_version)):
                if value: command.extend([flag,str(value)])
            if a.cold: command.append('--cold')
            if a.wifi_cycle: command.append('--wifi-cycle')
            result=subprocess.run(command,timeout=1500)
            rows.append({'phase':phase,'exit':result.returncode})
            (a.out/'summary.json').write_text(json.dumps(rows,indent=2)+'\n')
            report=a.out/phase/'result.json'
            if not report.exists() or any(step['status']=='BLOCKED' or (step['step']=='restore' and step['status']=='FAIL')
                    for step in json.loads(report.read_text())['steps']):
                break  # An uncertain restoration blocks every later device phase.
        return int(any(row['exit'] for row in rows))
    if os.environ.get('BRICK_JOURNEY_FLOCK')!='1':
        # Use the same visible utility invocation as the other Brick harnesses.
        # Python's flock is also regression-tested against it, but this removes
        # ambiguity when operators inspect the process tree during a session.
        os.execvpe('flock',['flock','-n','/tmp/brick-device.lock','env',
                   'BRICK_JOURNEY_FLOCK=1',sys.executable,__file__,*sys.argv[1:]],os.environ)
    def interrupt(signum, frame):
        raise TimeoutError('session interrupted; restoring the device')
    signal.signal(signal.SIGTERM,interrupt)
    signal.signal(signal.SIGALRM,interrupt)
    with open('/tmp/brick-device.lock','w') as lock:
        r=Remote(a.host)
        journey=Journey(a,r)
        signal.alarm(530)
        try:
            journey.prepare()
            getattr(journey,a.phase.replace('-','_'))()
            journey.step('99-crash-free',journey.crash_check)
        except Exception as e:
            status='BLOCKED' if isinstance(e,DeviceBusy) else 'FAIL'
            journey.results.append({'step':a.phase+'-run','status':status,'detail':str(e)})
            print(status,a.phase,str(e),flush=True)
        finally:
            try:
                journey.cleanup()
            except Exception as e:
                journey.results.append({'step':'restore','status':'FAIL','detail':str(e)})
                raise
            finally:
                journey.flush()
                r.close()
                signal.alarm(0)
        return int(any(s['status']!='PASS' for s in journey.results))


if __name__=='__main__':
    sys.exit(main())
