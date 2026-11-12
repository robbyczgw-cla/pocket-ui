#!/usr/bin/env python3
"""Host-only service tests. No mount, radio, audio or Rust build; all firmware tools are stubs."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / 'deploy/brick/hwroot/usr/bin'


class BluetoothService(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='brick-bt-')
        self.addCleanup(self.tmp.cleanup)
        self.d = Path(self.tmp.name)
        self.env = dict(os.environ, BRICK_DATA=str(self.d / 'card'),
                        BRICK_RUN=str(self.d / 'run'), BRICK_STATE=str(self.d / 'state'),
                        BRICK_BT_STORE=str(self.d / 'store'), BRICK_BT_HCI=str(self.d / 'hci'),
                        BRICK_RFKILL=str(self.d / 'rfkill'), BRICK_BT_BUS=str(self.d / 'bus'),
                        BRICK_BT_PIDOF='/bin/false', BRICK_HCICONFIG='/bin/true')
        for p in ['hci', 'state', 'rfkill/rfkill0', 'rfkill/rfkill1']:
            (self.d / p).mkdir(parents=True)
        (self.d / 'rfkill/rfkill0/type').write_text('bluetooth\n')
        (self.d / 'rfkill/rfkill0/state').write_text('0\n')
        (self.d / 'rfkill/rfkill1/type').write_text('wlan\n')
        (self.d / 'rfkill/rfkill1/state').write_text('1\n')
        self.bus = socket.socket(socket.AF_UNIX)
        self.bus.bind(self.env['BRICK_BT_BUS'])
        self.addCleanup(self.bus.close)
        self.stub('BRICK_BT_MOUNT', 'mount', "import os\nopen(os.environ['BRICK_DATA']+'.mount','a').write('mount\\n')\n")
        self.stub('BRICK_BT_UMOUNT', 'umount', "pass\n")
        self.stub('BRICK_BT_DAEMON', 'bluetoothd', 'import time\nwhile True: time.sleep(0.1)\n')
        self.stub('BRICK_BT_CTL', 'bluetoothctl', '''import os, sys
from pathlib import Path
args = sys.argv[1:]
if args[:1] == ['--agent']:
    for line in sys.stdin:
        if line.strip() == 'default-agent':
            print('Default agent request successful', flush=True)
elif args == ['power', 'on'] or args == ['power', 'off']:
    print('Changing power on/off succeeded')
elif args == ['show']:
    print('Controller 00:11:22:33:44:55\\n    Powered: yes')
elif args[:1] == ['trust']:
    key = Path(os.environ['BRICK_RUN']) / 'bluetooth/keys/test/info'
    key.parent.mkdir(parents=True, exist_ok=True)
    key.write_text('Trusted=true')
    print('Changing trust succeeded')
elif args == ['devices']:
    print('Device AA:BB:CC:DD:EE:FF Headset\\nDevice 11:22:33:44:55:66 Pad')
elif args[:1] == ['info']:
    headset = args[1] == 'AA:BB:CC:DD:EE:FF'
    on = (Path(os.environ['BRICK_DATA'] + '.connected').exists() and headset)
    print('Device %s\\n\\tConnected: %s' % (args[1], 'yes' if on else 'no'))
    print('\\tUUID: Audio Sink                (0000110b-0000-1000-8000-00805f9b34fb)' if headset else '\\tUUID: Human Interface Device    (00001124-0000-1000-8000-00805f9b34fb)')
elif args[:1] == ['connect']:
    open(os.environ['BRICK_DATA'] + '.connects', 'a').write(args[1] + '\\n')
    print('Connection successful')
elif args[:1] == ['remove']:
    (Path(os.environ['BRICK_RUN']) / 'bluetooth/keys/test/info').unlink()
    print('Device has been removed')
else:
    sys.exit(1)
''')
        self.addCleanup(lambda: self.call('stop', check=False))

    def stub(self, key, name, body):
        p = self.d / name
        p.write_text('#!/usr/bin/env python3\n' + body)
        p.chmod(0o755)
        self.env[key] = str(p)
        return p

    def call(self, *args, check=True):
        return subprocess.run([str(BIN / 'brick-bluetooth'), *args], env=self.env,
                              capture_output=True, timeout=65, check=check)

    def test_lifecycle_persistence_and_only_bluetooth_rfkill(self):
        self.call('start')
        self.call('status')
        pid = (self.d / 'run/bluetooth/service.pid').read_text()
        self.call('start')
        self.assertEqual(pid, (self.d / 'run/bluetooth/service.pid').read_text())
        self.assertEqual((self.d / 'card.mount').read_text(), 'mount\n')
        self.assertEqual((self.d / 'rfkill/rfkill0/state').read_text(), '1\n')
        self.assertEqual((self.d / 'rfkill/rfkill1/state').read_text(), '1\n')
        subprocess.run([str(BIN / 'brick-bluetoothctl'), 'trust', '00:11:22:33:44:55'],
                       env=self.env, check=True, capture_output=True, timeout=5)
        with tarfile.open(self.d / 'state/bluetooth.tar') as archive:
            self.assertEqual(archive.extractfile('./test/info').read(), b'Trusted=true')
        subprocess.run([str(BIN / 'brick-bluetoothctl'), 'remove', '00:11:22:33:44:55'],
                       env=self.env, check=True, capture_output=True, timeout=5)
        with tarfile.open(self.d / 'state/bluetooth.tar') as archive:
            self.assertNotIn('./test/info', archive.getnames())
        self.call('stop')
        self.assertNotEqual(self.call('status', check=False).returncode, 0)
        self.assertFalse((self.d / 'state/bluetooth-on').exists())
        self.call('start')
        self.assertFalse((self.d / 'run/bluetooth/keys/test/info').exists())

    def test_a_chip_that_answers_nothing_is_attached_once_more(self):
        # 08.11.: after the first sleep helper's rfkill cycles hci0 stayed but answered nothing (HCI timeouts): the attach that
        # holds it (exactly hciattach -n ttyS1 xradio) ends, the Bluetooth chip's power is reset, a new attach comes up.
        (self.d / 'rfkill/rfkill0/name').write_text('sunxi-bt\n')
        (self.d / 'rfkill/rfkill1/name').write_text('phy0\n')
        self.env['BRICK_HCICONFIG'] = '/bin/false'
        hci = self.d / 'hci'
        stale = subprocess.Popen([sys.executable, '-c', 'import os,signal,sys,time\n'
                                  'signal.signal(signal.SIGTERM, lambda *_: (os.rmdir(sys.argv[1]), sys.exit(0)))\n'
                                  'time.sleep(60)', str(hci)])
        self.addCleanup(lambda: stale.poll() is None and stale.kill())
        attach = self.stub('BRICK_BT_ATTACH', 'hciattach', """import os, sys, time
from pathlib import Path
Path(os.environ['BRICK_BT_HCI']).mkdir(exist_ok=True)
open(os.environ['BRICK_DATA'] + '.attached', 'a').write(' '.join(sys.argv[1:]) + '\\n')
time.sleep(60)
""")
        proc = self.d / 'attach-proc' / str(stale.pid)
        proc.mkdir(parents=True)
        (proc / 'cmdline').write_bytes(f'{attach}\0-n\0ttyS1\0xradio\0'.encode())
        self.env['BRICK_BT_ATTACH_PROC'] = str(self.d / 'attach-proc')
        self.stub('BRICK_BT_ATTACH_PIDOF', 'attach-pidof', f'print({stale.pid})\n')
        wifi = (self.d / 'rfkill/rfkill1/state').stat().st_mtime_ns
        self.call('start')
        self.assertIsNotNone(stale.wait(timeout=5), 'the old attach ended')
        self.assertEqual(Path(self.env['BRICK_DATA'] + '.attached').read_text(), '-n ttyS1 xradio\n', 'one new attach')
        self.assertEqual((self.d / 'rfkill/rfkill0/state').read_text(), '1\n')
        self.assertEqual((self.d / 'rfkill/rfkill1/state').stat().st_mtime_ns, wifi, 'never the Wi-Fi rfkill')
        self.call('status')

    def test_sleep_keeps_bluetooth_on_and_asks_the_headphones_back(self):
        # The system's sleep (brick-suspend): Settings' "on" stays, and the headphones connected before are asked to
        # connect again when the service is back; a plain stop forgets them.
        self.call('start')
        (self.d / 'run/bt-audio').write_text('AA:BB:CC:DD:EE:FF\n')
        self.call('sleep')
        self.assertTrue((self.d / 'state/bluetooth-on').exists(), 'still on in Settings')
        self.assertNotEqual(self.call('status', check=False).returncode, 0, 'the service ended for the suspend')
        self.call('start')
        connects = Path(self.env['BRICK_DATA'] + '.connects')
        deadline = time.monotonic() + 10
        while not connects.exists() and time.monotonic() < deadline:
            time.sleep(0.1)
        self.assertEqual(connects.read_text(), 'AA:BB:CC:DD:EE:FF\n')
        self.call('stop')
        self.assertFalse((self.d / 'run/bluetooth/reconnect').exists())

    def test_a_chip_that_answers_is_left_attached(self):
        self.stub('BRICK_BT_ATTACH_PIDOF', 'attach-pidof', 'raise SystemExit("asked for the attach")\n')
        self.call('start')
        self.assertFalse(Path(self.env['BRICK_DATA'] + '.attached').exists())

    def test_foreign_daemon_is_refused_without_mount(self):
        self.env['BRICK_BT_PIDOF'] = '/bin/true'
        self.assertNotEqual(self.call('start', check=False).returncode, 0)
        self.assertFalse((self.d / 'card.mount').exists())
        self.assertFalse((self.d / 'state/bluetooth-on').exists())

    def factory_service(self, *, foreign=False):
        """TrimUI's original menu starts procd BlueZ and links its persistent keys."""
        keys = self.d / 'firmware-keys'
        keys.mkdir()
        (keys / 'stock-pairing').write_text('keep the stock pairing')
        Path(self.env['BRICK_BT_STORE']).symlink_to(keys)
        self.env['BRICK_BT_FIRMWARE_KEYS'] = str(keys)
        proc = self.d / 'firmware-proc/424242'
        proc.mkdir(parents=True)
        self.env['BRICK_BT_FIRMWARE_PROC'] = str(proc.parent)
        (proc / 'exe').symlink_to(self.env['BRICK_BT_DAEMON'])
        daemon = self.env['BRICK_BT_DAEMON']
        (proc / 'cmdline').write_bytes((daemon + '\0' + ('--other' if foreign else '-nd') + '\0').encode())
        (self.d / 'firmware-running').touch()
        self.stub('BRICK_BT_PIDOF', 'pidof', '''import os, sys
from pathlib import Path
if Path(os.environ['BRICK_BT_FIRMWARE_PROC']).parent.joinpath('firmware-running').exists():
    print('424242')
else: sys.exit(1)
''')
        self.stub('BRICK_BT_FIRMWARE_INIT', 'firmware-init', '''import os, sys
from pathlib import Path
assert sys.argv[1:] == ['stop']
root = Path(os.environ['BRICK_BT_FIRMWARE_PROC']).parent
(root / 'firmware-stopped').touch()
(root / 'firmware-running').unlink()
''')
        return keys

    def test_factory_boot_can_enable_bluetooth_without_changing_stock_pairings(self):
        keys = self.factory_service()
        self.call('start')
        self.call('status')
        self.assertTrue((self.d / 'firmware-stopped').exists())
        self.assertFalse(Path(self.env['BRICK_BT_STORE']).is_symlink())
        self.assertEqual(list(keys.iterdir()), [keys / 'stock-pairing'])
        self.assertEqual((keys / 'stock-pairing').read_text(), 'keep the stock pairing')
        self.call('stop')
        self.assertTrue(Path(self.env['BRICK_BT_STORE']).is_symlink())
        self.assertEqual(Path(self.env['BRICK_BT_STORE']).resolve(), keys)
        self.call('start')
        self.call('status')

    def test_factory_handover_refuses_a_daemon_from_another_session(self):
        keys = self.factory_service(foreign=True)
        self.assertNotEqual(self.call('start', check=False).returncode, 0)
        self.assertFalse((self.d / 'firmware-stopped').exists())
        self.assertTrue(Path(self.env['BRICK_BT_STORE']).is_symlink())
        self.assertEqual((keys / 'stock-pairing').read_text(), 'keep the stock pairing')

    def test_failed_factory_mount_restores_the_stock_storage_link(self):
        keys = self.factory_service()
        self.env['BRICK_BT_MOUNT'] = '/bin/false'
        self.assertNotEqual(self.call('start', check=False).returncode, 0)
        self.assertTrue(Path(self.env['BRICK_BT_STORE']).is_symlink())
        self.assertEqual(Path(self.env['BRICK_BT_STORE']).resolve(), keys)

    def test_absent_bus_does_not_start_or_mount(self):
        self.env['BRICK_BT_BUS'] = str(self.d / 'missing-bus')
        self.assertNotEqual(self.call('start', check=False).returncode, 0)
        self.assertFalse((self.d / 'card.mount').exists())

    def test_failed_mount_preserves_the_previous_archive(self):
        old = self.d / 'state/bluetooth.tar'
        with tarfile.open(old, 'w'):
            pass
        before = old.read_bytes()
        self.env['BRICK_BT_MOUNT'] = '/bin/false'
        self.assertNotEqual(self.call('start', check=False).returncode, 0)
        self.assertEqual(old.read_bytes(), before)

    def test_stale_pid_file_never_signals_an_unrelated_process(self):
        foreign = subprocess.Popen(['sleep', '30'])
        self.addCleanup(foreign.wait)
        self.addCleanup(foreign.terminate)
        run = self.d / 'run/bluetooth'
        run.mkdir(parents=True)
        (run / 'service.pid').write_text(str(foreign.pid))
        self.call('stop')
        self.assertIsNone(foreign.poll())

    def test_systemctl_dispatches_bluetooth_unit_names(self):
        self.call('start')
        for unit in ['bluetooth', 'bluetooth.service']:
            out = subprocess.run([str(BIN / 'systemctl'), '--quiet', 'is-active', unit],
                                 env=self.env, capture_output=True, timeout=5, check=True)
            self.assertEqual(out.stdout.strip(), b'active')


    def wait_for(self, what, done, seconds=12):
        import time
        until = time.time() + seconds
        while time.time() < until:
            if done():
                return
            time.sleep(0.2)
        self.fail(what)

    def test_a_connected_headset_takes_the_sound_and_gives_it_back(self):
        """Headphones (A2DP): BlueALSA runs beside BlueZ; a connected device with an Audio Sink becomes the route (its address
        in run/bt-audio, which pocket-core and pocket-ui read at every open), its level is set once, and the route goes when
        it disconnects or the service stops. A pad is never a route."""
        self.stub('BRICK_BLUEALSA', 'bluealsa', 'import time\nwhile True: time.sleep(0.1)\n')
        log = self.volume_mixer()
        self.volume('40')
        # Opening pocket_bt creates its softvol control. Simulate its initial
        # maximum: the real service must invoke the real helper to lower it.
        values = self.mixer_values()
        values['BT Volume'] = '255'
        (self.d / 'mixer.json').write_text(json.dumps(values))
        log.write_text('')
        self.stub('BRICK_VOLUME', 'volume', "import os, sys\nopen(os.environ['BRICK_DATA']+'.volume','a').write(' '.join(sys.argv[1:])+'\\n')\nos.execv(%r, [%r, *sys.argv[1:]])\n" % (str(BIN / 'volume'), str(BIN / 'volume')))
        route = self.d / 'run/bt-audio'
        self.call('start')
        self.assertTrue((self.d / 'run/bluetooth/bluealsa.pid').exists(), 'BlueALSA started')
        (self.d / 'hci:12').mkdir()
        self.wait_for('no headset connected: no route', lambda: not route.exists() and (self.d / 'run/bluetooth/service.log').exists(), 4)
        Path(self.env['BRICK_DATA'] + '.connected').touch()
        self.wait_for('the headset becomes the route', lambda: route.exists() and route.read_text().strip() == 'AA:BB:CC:DD:EE:FF')
        self.wait_for('its real softvol level is set', lambda: self.mixer_values().get('BT Volume') == '125')
        self.assertIn('cset name=BT Volume 125', log.read_text())
        self.assertEqual(Path(self.env['BRICK_DATA'] + '.volume').read_text(), 'bt\n', 'once, not every 3 s')
        self.assertEqual(self.call('audio').stdout.strip(), b'AA:BB:CC:DD:EE:FF')
        Path(self.env['BRICK_DATA'] + '.connected').unlink()
        self.wait_for('disconnected: back to the speaker', lambda: not route.exists())
        Path(self.env['BRICK_DATA'] + '.connected').touch()
        self.wait_for('connected again', lambda: route.exists())
        pid = int((self.d / 'run/bluetooth/bluealsa.pid').read_text())
        self.call('stop')
        self.assertFalse(route.exists(), 'stopped: the speaker')
        self.assertFalse(os.path.exists('/proc/%d' % pid) and open('/proc/%d/stat' % pid).read().split()[2] != 'Z', 'BlueALSA ended')

    def volume(self, *args):
        return subprocess.run([str(BIN / 'volume'), *args], env=self.env,
                              capture_output=True, timeout=5, check=True)

    def mixer_values(self):
        return json.loads((self.d / 'mixer.json').read_text())

    def volume_mixer(self):
        log = self.d / 'amixer.log'
        self.env['BRICK_VOLUME_RAMP_DELAY'] = '0'
        self.env['BRICK_AMIXER'] = str(self.stub('BRICK_AMIXER', 'amixer', """import sys, json, shlex
from pathlib import Path
state = Path(%r)
values = json.loads(state.read_text()) if state.exists() else {'digital volume': '0', 'BT Volume': '255'}
def control(args):
    name = next(a[5:] for a in args if a.startswith('name='))
    if 'cget' in args:
        if name not in values: raise SystemExit(1)
        print('  : values=' + values[name])
    else:
        values[name] = args[-1]
        pending = state.with_suffix('.new')
        pending.write_text(json.dumps(values))
        pending.replace(state)
        with open(%r, 'a') as out: out.write(' '.join(args) + '\\n')
if '-s' in sys.argv:
    for line in sys.stdin: control(shlex.split(line))
else: control(sys.argv[1:])
""" % (str(self.d / 'mixer.json'), str(log))))
        return log

    def test_volume_sets_the_headphones_level_and_the_mute_slider_silences_them(self):
        log = self.volume_mixer()
        self.volume('40')
        # Curve 3's 40% is codec 22 / BT 125; 116 was curve 2's BT endpoint.
        self.assertEqual(self.mixer_values()['digital volume'], '22')
        self.assertEqual(self.mixer_values()['BT Volume'], '125')
        prefix = 'cset name=BT Volume '
        steps = [int(line[len(prefix):]) for line in log.read_text().splitlines() if line.startswith(prefix)]
        self.assertTrue(steps, 'the headset control must actually receive commands')
        self.assertEqual(steps[-1], 125)
        self.assertTrue(all(0 < a - b <= 5 for a, b in zip([255] + steps, steps)), 'headphone ramp: at most 1 dB per write')
        self.volume('mute')
        self.assertTrue(log.read_text().splitlines()[-1].endswith('BT Volume 0'), 'muted: silent headphones too')
        muted = self.mixer_values()
        muted_trace = log.read_text()
        self.volume('60')
        self.assertTrue(log.read_text().splitlines()[-1].endswith('BT Volume 0'), 'a level change while muted stays silent')
        self.assertEqual(self.mixer_values(), muted, 'muted keys do not change either actual output')
        self.assertEqual(log.read_text(), muted_trace, 'muted keys do not write mixer controls')
        self.assertEqual((Path(self.env['BRICK_STATE']) / 'brick-volume-curve3').read_text().strip(), '40')
        self.volume('unmute')
        self.assertEqual(self.mixer_values()['digital volume'], '22')
        self.assertEqual(self.mixer_values()['BT Volume'], '125', 'exact pre-mute headphone level restored')
        self.assertTrue(log.read_text().splitlines()[-1].endswith('BT Volume 125'), 'unmuted: the exact pre-mute level, with hidden changes ignored')

if __name__ == '__main__':
    unittest.main()
