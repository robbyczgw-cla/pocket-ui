#!/usr/bin/env python3
"""Compare buffer/quantum settings with the existing waveform and session limits.

Requires an aarch64 ROCKNIX test image and a prebuilt audio_tone. Never touches a physical device.
Each case has its own files, including failures. --start-ms is a benchmark-only first-push delay;
keep 100 for the production-like start, and label slower-host explorations explicitly.
"""
import argparse
from pathlib import Path
import subprocess
import time
import tempfile
import json

CASES = {
    'default256': ('speaker', {}),
    'h700': ('speaker', {'POCKETUI_DEVICE': 'rg35xx-plus'}),
    'h700-pauses': ('speaker', {'POCKETUI_DEVICE': 'rg35xx-plus', 'AUDIO_TONE_PAUSES': '5:6,12:12.3'}),
    'buffered256': ('speaker', {'POCKET_CORE_ALSA_US': '32000', 'POCKET_CORE_AUDIO_MS': '32'}),
    'buffered512': ('speaker', {'POCKET_CORE_QUANTUM': '512', 'POCKET_CORE_ALSA_US': '32000', 'POCKET_CORE_AUDIO_MS': '32'}),
    'cycle1024': ('speaker', {'POCKET_CORE_QUANTUM': '0'}),
    'bluetooth': ('bluetooth', {}),
    'h700-bluetooth': ('bluetooth', {'POCKETUI_DEVICE': 'rg35xx-plus'}),
    'bluetooth-skew': ('bluetooth', {'AUDIO_TONE_REFRESH': '60.06'}),
    'pauses-spk': ('speaker', {'AUDIO_TONE_PAUSES': '5:6,12:12.3'}),
    'pauses-bt': ('bluetooth', {'AUDIO_TONE_PAUSES': '5:6,12:12.3'}),
}


def capture_health(path):
    """Conservatively reject a recorder with a known error, even when its WAV happens to look clean.

    The isolated graph has exactly one recorder with our name. Retain the raw counter and its limited
    attribution: a single final snapshot never establishes gap-free coverage of the entire recording.
    """
    counter = None
    try:
        for line in path.read_text().splitlines():
            fields = line.split()
            if fields[:3] == ["S", "ID", "QUANT"]:
                counter = None
            if line.endswith(" + bench_rec") and len(fields) >= 11 and fields[0] == "R":
                try:
                    counter = int(fields[8])
                except ValueError:
                    counter = None
    except OSError:
        pass
    if counter is None or counter < 0:
        return {"status": "unavailable", "raw_recorder_err": None, "reason": "active recorder row unavailable"}, False
    return {"status": "errors_observed" if counter else "partial", "raw_recorder_err": counter,
            "reason": "final raw recorder snapshot; no complete capture-coverage claim"}, counter == 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin', type=Path, required=True)
    parser.add_argument('--image', default='pocket-vdev-base:20261001')
    parser.add_argument('--recorder-image', help='optional native image with pw-record, sharing the test server socket')
    parser.add_argument('--seconds', type=int, default=30)
    parser.add_argument('--start-ms', type=int, default=100)
    parser.add_argument('--cases', nargs='+', choices=CASES, default=list(CASES))
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    if args.seconds < 8:
        parser.error('--seconds must be at least 8')
    root = Path(__file__).resolve().parent
    args.output.mkdir(parents=True, exist_ok=False)
    name = f'audio-path-matrix-{time.time_ns()}'
    def docker(*cmd, **kwargs):
        return subprocess.run(['docker', *cmd], check=True, **kwargs)
    failed = False
    recorder = None
    runtime = tempfile.TemporaryDirectory(prefix='audio-path-runtime-')
    try:
        docker('run', '-d', '--name', name, '-e', 'PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin',
               '-v', runtime.name + ':/tmp/xdg', '--entrypoint', '/bin/sh', args.image, '-c', 'sleep 86400')
        docker('exec', name, 'mkdir', '-p', '/etc/pipewire/pipewire.conf.d', '/tmp/xdg', '/tmp/out')
        for source, destination in [(args.bin, '/usr/local/bin/audio_tone'),
                                    (root / 'run-case.sh', '/usr/local/bin/run-case.sh'),
                                    (root / 'bench-sink.conf', '/etc/pipewire/pipewire.conf.d/90-bench.conf')]:
            docker('cp', str(source), name + ':' + destination)
        docker('exec', name, 'sh', '-c', '''export XDG_RUNTIME_DIR=/tmp/xdg DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/xdg/bus
            dbus-daemon --session --fork --address=$DBUS_SESSION_BUS_ADDRESS
            pipewire >/tmp/pw.log 2>&1 & sleep 1
            wireplumber >/tmp/wp.log 2>&1 & sleep 2''')
        for case in args.cases:
            mode, env = CASES[case]
            env = dict(env, AUDIO_TONE_START_MS=str(args.start_ms))
            print('CASE', case, 'mode', mode, 'seconds', args.seconds, 'environment', env, flush=True)
            if args.recorder_image:
                recorder = name + '-rec'
                docker('run', '-d', '--name', recorder, '--network', 'none',
                       '-e', 'XDG_RUNTIME_DIR=/tmp/xdg', '-v', runtime.name + ':/tmp/xdg',
                       '-v', str(args.output.resolve()) + ':/out', '--entrypoint', 'pw-record',
                       args.recorder_image, '--target', 'bench_sink', '-P',
                       '{ stream.capture.sink = true node.name = bench_rec }', '--rate', '48000',
                       '--channels', '2', '--format', 's16', '/out/' + case + '.wav')
                time.sleep(1)
            with (args.output / (case + '.run.log')).open('w') as log:
                result = subprocess.run(['docker', 'exec', '-e', 'BENCH_EXTERNAL_RECORDER=' + ('1' if args.recorder_image else '0'), name, '/usr/local/bin/run-case.sh', mode,
                                         str(args.seconds), '/tmp/out/' + case,
                                         *[k + '=' + v for k, v in env.items()]], stdout=log, stderr=subprocess.STDOUT)
                failed |= result.returncode != 0
            if recorder:
                docker('stop', '-s', 'SIGINT', '-t', '5', recorder, stdout=subprocess.DEVNULL)
                with (args.output / (case + '.recorder.log')).open('w') as log:
                    docker('logs', recorder, stdout=log, stderr=subprocess.STDOUT)
                docker('rm', recorder, stdout=subprocess.DEVNULL)
                recorder = None
            suffixes = ['tone.log', 'pwtop', 'pwtop.end', 'pwtop.short']
            if not args.recorder_image:
                suffixes.append('wav')
            for suffix in suffixes:
                try:
                    docker('cp', name + ':/tmp/out/' + case + '.' + suffix, str(args.output))
                except subprocess.CalledProcessError:
                    failed = True
            capture, capture_ok = capture_health(args.output / (case + '.pwtop.end'))
            (args.output / (case + '.capture.json')).write_text(json.dumps(capture, indent=2) + '\n')
            failed |= not capture_ok
            for analyzer, files in [('session.py', [args.output / (case + '.tone.log')]),
                                    ('analyze.py', ['--expect', '1004.5619', args.output / (case + '.wav')])]:
                with (args.output / (case + '.' + analyzer + '.log')).open('w') as log:
                    result = subprocess.run(['python3', str(root / analyzer), *map(str, files)], stdout=log, stderr=subprocess.STDOUT)
                failed |= result.returncode != 0
            print('FINISHED', case, flush=True)
        for source in ['pw.log', 'wp.log']:
            docker('cp', name + ':/tmp/' + source, str(args.output))
    finally:
        if recorder:
            docker('rm', '-f', recorder, stdout=subprocess.DEVNULL)
        docker('rm', '-f', name, stdout=subprocess.DEVNULL)
        runtime.cleanup()
    return int(failed)


if __name__ == '__main__':
    raise SystemExit(main())
