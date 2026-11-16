#!/usr/bin/env python3
"""Silent, reversible volume-curve check against the real codec signal path.

Host: --family h700 --host root@HOST [--jump HOST] --out evidence.json
Run only on an explicitly allocated, idle device. No PCM, keys, restart or mute guard.
Brick/Retroid are explicit TODO profiles; they never claim an H700 topology or pass.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import select
import shlex
import signal
import struct
import subprocess
import sys
import time

CONTROLS = ('Speaker Switch', 'DAC Playback Volume', 'DAC Playback Switch',
            'Line Out Playback Volume', 'Line Out Playback Switch')
CEILING_DB = -73.08 + 44 * 1.16
TOLERANCE_DB = 1.5


def gain_row(percent, controls, props):
    dac = int(controls['DAC Playback Volume'])
    line = int(controls['Line Out Playback Volume'])
    soft = props['softVolumes']
    assert len(soft) == 2 and all(0 < v <= 1.001 for v in soft), 'invalid software residual'
    assert 0 <= dac <= 44 and 2 <= line <= 31, 'codec gain outside speaker limits'
    assert all(controls[n] == ('on' if n == 'Speaker Switch' else 'on,on')
               for n in ('Speaker Switch', 'DAC Playback Switch', 'Line Out Playback Switch')), 'playback switch off'
    measured = [-73.08 + dac * 1.16 - 46.5 + line * 1.5 + 20 * math.log10(v) for v in soft]
    expected = CEILING_DB + 60 * math.log10(percent / 100)
    return dict(percent=percent, controls=controls, soft=soft, measured_db=measured,
                expected_db=expected, error_db=[v-expected for v in measured],
                passed=all(abs(v-expected) <= TOLERANCE_DB for v in measured))


def validate_topology(speaker, lineout, mixer):
    assert 'in  "static" "LINEOUT" "5096000.codec"' in speaker, 'unknown speaker DAPM path'
    assert 'in  "static" "Line Out Ramp Controller" "5096000.codec"' in lineout, 'unknown LINEOUT DAPM path'
    for name, limits, scale in [('DAC Playback Volume','min=0,max=63','dBscale-min=-73.08dB,step=1.16dB'),
                                ('Line Out Playback Volume','min=0,max=31','dBscale-min=-43.50dB,step=1.50dB')]:
        section = next(s for s in mixer.split('numid=') if s.splitlines() and s.splitlines()[0].endswith("name='"+name+"'"))
        assert limits in section and scale in section, 'unknown mixer TLV: '+name


class H700:
    def __init__(self):
        os.environ['XDG_RUNTIME_DIR'] = '/var/run/0-runtime-dir'
        self.inputs = [os.open(str(p), os.O_RDONLY | os.O_NONBLOCK) for p in Path('/dev/input').glob('event*')]
        assert self.inputs, 'no physical input monitor'

    def run(self, *args):
        return subprocess.check_output(args, text=True, timeout=5).strip()

    def idle(self):
        batteries = [int(p.read_text()) for p in Path('/sys/class/power_supply').glob('*/capacity')
                     if (p.parent/'type').read_text().strip() == 'Battery']
        assert batteries and min(batteries) >= 20, 'battery below 20% or unreadable'
        status = Path('/tmp/pocket-ui.status').read_text()
        assert '\tgame=none\t' in status, 'game running; device not allocated'
        assert not json.loads(self.run('pactl','-f','json','list','sink-inputs')), 'audio stream active'
        assert Path('/proc/asound/card0/pcm0p/sub0/status').read_text().strip() == 'closed', 'PCM is open'
        for fd in select.select(self.inputs, [], [], 0)[0]:
            data = os.read(fd,4096)
            size = struct.calcsize('llHHi')
            for offset in range(0,len(data)-size+1,size):
                _,_,kind,_,value = struct.unpack_from('llHHi',data,offset)
                assert not ((kind == 1 and value > 0) or kind == 5), 'physical input: stop and restore'

    def controls(self):
        return {n: re.search(r': values=(.*)',self.run('amixer','-c','Codec','cget','name='+n))[1].strip() for n in CONTROLS}

    def snapshot(self):
        self.idle()
        sink_name = self.run('pactl','get-default-sink')
        sink = next(s for s in json.loads(self.run('pactl','-f','json','list','sinks')) if s['name'] == sink_name)
        assert sink_name.endswith('.HiFi__Speaker__sink') and '5096000.codec' in sink_name, 'not the internal speaker'
        assert not sink['mute'], 'sink muted'
        controls = self.controls()
        assert controls['Speaker Switch'] == 'on' and controls['DAC Playback Switch'] == 'on,on', 'hardware muted'
        base = Path('/sys/kernel/debug/asoc/H616 Audio Codec')
        speaker = (base/'dapm/Speaker').read_text()
        lineout = (base/'5096000.codec/dapm/LINEOUT').read_text()
        mixer = self.run('amixer','-c','Codec','contents')
        validate_topology(speaker,lineout,mixer)
        self.sink, self.node = sink_name, str(sink['properties']['object.id'])
        status = Path('/tmp/pocket-ui.status').read_text()
        pid = re.search(r'(?:^|\t)pid=(\d+)',status)[1]
        with (Path('/proc')/pid/'exe').open('rb') as f:
            executable_sha = hashlib.file_digest(f,'sha256').hexdigest()
        wp = {p:p.read_bytes() for p in Path('/storage/.local/state/wireplumber').iterdir() if p.is_file()}
        return dict(controls=controls, sink=sink, wp=wp, executable_sha256=executable_sha,
                    topology=dict(speaker=speaker,lineout=lineout,mixer=mixer), status=status)

    def set_volume(self, percent):
        self.run('pactl','set-sink-volume',self.sink,str(percent)+'%')

    def sample(self, percent):
        self.idle()
        assert self.run('pactl','get-default-sink') == self.sink, 'output route changed'
        props = json.loads(self.run('pw-dump',self.node))[0]['info']['params']['Props'][0]
        row = gain_row(percent,self.controls(),props)
        row['pcm'] = 'closed'
        return row

    def restore(self, before):
        errors = []
        if self.run('pactl','get-default-sink') != self.sink:
            return dict(exact=False,errors=['output route changed: preserving the new route instead of forcing old speaker switches'])
        try:
            self.run('pactl','set-sink-volume',self.sink,*[str(v['value']) for v in before['sink']['volume'].values()])
            time.sleep(2)
        except Exception as error:
            errors.append(str(error))
        # Attempt every independent control even if one restore fails. Speaker is last.
        for name in ('DAC Playback Volume','DAC Playback Switch','Line Out Playback Volume','Line Out Playback Switch','Speaker Switch'):
            try:
                self.run('amixer','-q','-c','Codec','cset','name='+name,before['controls'][name])
            except Exception as error:
                errors.append(str(error))
        time.sleep(2)
        for path,data in before['wp'].items():
            try:
                if path.read_bytes() != data: path.write_bytes(data)
            except Exception as error:
                errors.append(str(error))
        sink = next(s for s in json.loads(self.run('pactl','-f','json','list','sinks')) if s['name'] == self.sink)
        exact = self.controls() == before['controls'] and sink['volume'] == before['sink']['volume'] and sink['mute'] == before['sink']['mute']
        exact = exact and all(p.read_bytes() == b for p,b in before['wp'].items())
        for fd in self.inputs: os.close(fd)
        return dict(exact=exact and not errors, errors=errors)


def probe(device, wait=time.sleep):
    before = device.snapshot()  # No mutation before a complete snapshot.
    result = dict(format=1, family='h700', timestamp=time.time(), tolerance_db=TOLERANCE_DB,
                  executable_sha256=before['executable_sha256'], topology=before['topology'], rows=[])
    try:
        for percent in range(10,101,10):
            device.idle()
            device.set_volume(percent)
            wait(2)  # bounded keeper poll + at most 44 one-step/20-ms DAC changes
            result['rows'].append(device.sample(percent))
    except Exception as error:
        result['error'] = str(error)
    finally:
        try:
            result['restore'] = device.restore(before)
        except Exception as error:
            result['restore'] = dict(exact=False,errors=[str(error)])
    result['status'] = 'pass' if len(result['rows']) == 10 and all(r['passed'] for r in result['rows']) and result['restore']['exact'] and 'error' not in result else 'fail'
    return result


def verify(result, expected_sha=None):
    assert result['format'] == 1 and result['family'] == 'h700' and result['status'] == 'pass'
    assert result['restore']['exact'], 'restore incomplete'
    assert 0 <= time.time()-result['timestamp'] < 86400, 'device evidence older than 24 hours'
    if expected_sha: assert result['executable_sha256'] == expected_sha, 'device binary differs from release binary'
    validate_topology(**result['topology'])
    assert [r['percent'] for r in result['rows']] == list(range(10,101,10)), 'incomplete curve'
    for row in result['rows']:
        assert row['pcm'] == 'closed', 'not a silent, stream-free measurement'
        assert gain_row(row['percent'],row['controls'],dict(softVolumes=row['soft']))['passed'], 'volume curve outside ±1.5 dB'


def remote_command(lock):
    """The measurement under the device's own lock: a lock file on the host guards nothing (two hosts, one handheld)."""
    take = 'exec 9>' + shlex.quote(lock) + '; flock -n 9 || { echo ' + shlex.quote('device lock busy: ' + lock) + ' >&2; exit 75; }; '
    return 'sh -c ' + shlex.quote(take + shlex.join(['exec','python3','-','--on-device','--family','h700']))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--family',choices=['h700','brick','retroid'],default='h700')
    parser.add_argument('--host')
    parser.add_argument('--jump')
    parser.add_argument('--lock',default='/tmp/plus-device.lock',help='lock file ON the device (flock -n), held for the whole measurement')
    parser.add_argument('--out',type=Path)
    parser.add_argument('--on-device',action='store_true')
    parser.add_argument('--verify-evidence',type=Path)
    parser.add_argument('--release-binary',type=Path)
    parser.add_argument('--release-sha',help='the sha256 the evidence must name instead of --release-binary (release-check.sh text-only)')
    args = parser.parse_args()
    if args.verify_evidence:
        sha = hashlib.sha256(args.release_binary.read_bytes()).hexdigest() if args.release_binary else args.release_sha
        verify(json.loads(args.verify_evidence.read_text()),sha)
        print('volume device check: verified H700 curve, exact restore, matching release binary' if sha else 'volume device check: verified H700 curve and exact restore')
        return 0
    if args.family != 'h700':
        print(json.dumps(dict(format=1,family=args.family,status='todo',reason='TODO: capture this family\'s real mixer/DAPM path and define its volume conversion before enabling the check')))
        return 3
    if args.on_device:
        def interrupted(*_): raise RuntimeError('measurement interrupted; restoring')
        for sig in (signal.SIGTERM,signal.SIGHUP,signal.SIGINT,signal.SIGALRM): signal.signal(sig,interrupted)
        signal.alarm(90)  # Leave two minutes for every restore before the host SSH deadline.
        result = probe(H700())
        signal.alarm(0)
    else:
        if not args.host: parser.error('--host required; device must be allocated explicitly')
        ssh = ['ssh','-o','BatchMode=yes','-o','ConnectTimeout=10']
        if args.jump: ssh += ['-J',args.jump]
        ssh += [args.host,remote_command(args.lock)]
        completed = subprocess.run(ssh,input=Path(__file__).read_bytes(),capture_output=True,timeout=210)
        if completed.returncode not in (0,1) or not completed.stdout.strip():
            raise RuntimeError(completed.stderr.decode(errors='replace'))
        result = json.loads(completed.stdout)
    if args.out:
        args.out.parent.mkdir(parents=True,exist_ok=True)
        args.out.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result))
    return 0 if result['status'] == 'pass' else 1


if __name__ == '__main__':
    sys.exit(main())
