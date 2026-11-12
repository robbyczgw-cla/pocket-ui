#!/usr/bin/env python3
"""A refused/failed Brick sleep must leave input, network and radio policy usable."""
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import unittest

HELPERS = Path(__file__).resolve().parents[1] / 'deploy/brick/hwroot/usr/bin'


class Suspend(unittest.TestCase):
    def fixture(self, shell, outcome='success', wifi=True, radio='1', marker=False, bt=False):
        tmp = tempfile.TemporaryDirectory(prefix='brick-suspend-test-')
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)

        def put(name, value):
            p = root / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(value)
            return p

        def executable(name, body):
            p = put(name, '#!/bin/sh\n' + body + '\n')
            p.chmod(0o755)
            return str(p)

        put('sys/power/state', 'freeze mem\n')
        put('sys/class/rtc/rtc0/wakealarm', '130\n')
        put('sys/class/rtc/rtc0/since_epoch', '100\n')
        put('proc/driver/rtc', 'alarm_IRQ\t: yes\n')
        put('sys/kernel/debug/suspend_stats', 'success: 0\nfail: 0\n')
        for number, kind in [(0, 'bluetooth'), (1, 'wlan')]:
            put(f'sys/class/rfkill/rfkill{number}/type', kind)
            put(f'sys/class/rfkill/rfkill{number}/state', radio + '\n')
        put('tmp/placeholder', '')
        put('brightness', '42\n')
        if marker:
            put('tmp/system_suspend', 'pre-existing owner\n')
        env = dict(os.environ, BRICK_SUSPEND_ROOT=str(root), BRICK_SUSPEND_SETTLE='0',
                   BRICK_SUSPEND_LOG=str(root / 'suspend.log'), BRICK_SYNC='/bin/true')
        env['BRICK_BT'] = executable('bt', f'''echo "bt $1" >>'{root}/calls'
[ "$1" != start ] || sed -n 's/^SigIgn:[[:space:]]*//p' /proc/$$/status >'{root}/bt-sigign'
case "$1" in status) {'exit 0' if bt else 'exit 1'};; esac''')
        env['BRICK_HW'] = executable('hw', f'''case "$1" in
bl-get) cat '{root}/brightness';;
bl-set) echo "$2" >'{root}/brightness';;
esac''')
        env['BRICK_PIDOF'] = executable('pidof', '''case "$1" in
wpa_supplicant) %s;;
*) exit 1;;
esac''' % ('echo 123' if wifi else 'exit 1'))
        env['BRICK_UBUS'] = executable('ubus', f'echo "ubus $*" >>"{root}/calls"')
        env['BRICK_WPA_INIT'] = executable('wpa', f'echo "wpa $*" >>"{root}/calls"')
        env['BRICK_IFCONFIG'] = executable('ifconfig', f'echo "ifconfig $*" >>"{root}/calls"')
        enter = f'''echo enter >>'{root}/calls'
test -e '{root}/tmp/system_suspend' || exit 4
test "$(cat '{root}/brightness')" = 0 || exit 5
'''
        if outcome in ('success', 'false-negative'):
            stats = root / 'sys/kernel/debug/suspend_stats'
            enter += f"n=$(sed -n 's/^success: *//p' '{stats}'); echo \"success: $((n+1))\" >'{stats}'\n"
        enter += 'exit ' + ('1' if outcome in ('failed', 'false-negative') else '0')
        env['BRICK_SUSPEND_ENTER'] = executable('enter', enter)

        def run():
            result = subprocess.run([*shell, str(HELPERS / 'systemctl'), 'suspend'], env=env,
                                    capture_output=True, text=True, timeout=5)
            # The radios come back in the background; the lock goes when they are.
            deadline = time.monotonic() + 5
            while (root / 'tmp/brick-suspend.lock').exists() and time.monotonic() < deadline:
                time.sleep(0.02)
            return result

        return root, env, run

    def test_transitions_restore_the_players_previous_radio_and_input_state(self):
        shells = [['dash']]
        if shutil.which('busybox'):
            shells.append(['busybox', 'ash'])
        for shell in shells:
            for outcome in ['success', 'failed', 'false-negative', 'no-suspend']:
                for wifi, radio, marker in [(True, '1', False), (False, '0', True)]:
                    with self.subTest(shell=shell, outcome=outcome, wifi=wifi, radio=radio):
                        root, _, run = self.fixture(shell, outcome, wifi, radio, marker)
                        result = run()
                        self.assertEqual(result.returncode, 0 if outcome in ('success', 'false-negative') else 1,
                                         result.stderr)
                        calls = (root / 'calls').read_text()
                        self.assertEqual(calls.count('enter\n'), 1, 'no surprise re-suspend after a wake')
                        self.assertEqual('wpa start' in calls, wifi, 'restore only an originally running client')
                        self.assertEqual('ubus call service delete' in calls, wifi)
                        self.assertNotIn('bt sleep', calls, 'Bluetooth off in Settings stays off')
                        self.assertEqual((root / 'brightness').read_text().strip(), '42')
                        for i in (0, 1):
                            self.assertEqual((root / f'sys/class/rfkill/rfkill{i}/state').read_text().strip(), radio)
                        self.assertNotIn('rfkill', calls)
                        self.assertFalse((root / 'tmp/brick-suspend.lock').exists())
                        self.assertEqual((root / 'tmp/system_suspend').exists(), marker)
                        if marker:
                            self.assertEqual((root / 'tmp/system_suspend').read_text(), 'pre-existing owner\n')

    def test_an_alarm_due_within_seconds_stops_nothing(self):
        # A switch-off alarm (or a test's safety alarm) about to ring would wake the Brick at once: no transition.
        for alarm in ['101', '105']:
            with self.subTest(alarm=alarm):
                root, env, run = self.fixture(['dash'])
                (root / 'sys/class/rtc/rtc0/wakealarm').write_text(alarm)
                self.assertNotEqual(run().returncode, 0)
                self.assertNotIn('enter', (root / 'calls').read_text() if (root / 'calls').exists() else '')
                self.assertEqual((root / 'brightness').read_text().strip(), '42')

    def test_no_alarm_at_all_is_the_players_normal_sleep(self):
        # The power button wakes it; only tests and Switch off after arm the RTC.
        for alarm in ['', '0']:
            with self.subTest(alarm=alarm):
                root, env, run = self.fixture(['dash'])
                (root / 'sys/class/rtc/rtc0/wakealarm').write_text(alarm)
                self.assertEqual(run().returncode, 0)
                self.assertEqual((root / 'calls').read_text().count('enter\n'), 1)

    def test_bluetooth_on_in_settings_sleeps_and_starts_again(self):
        root, env, run = self.fixture(['dash'], bt=True)
        self.assertEqual(run().returncode, 0)
        calls = (root / 'calls').read_text().splitlines()
        self.assertLess(calls.index('bt sleep'), calls.index('enter'), 'stopped before the suspend')
        self.assertGreater(calls.index('bt start'), calls.index('enter'), 'and started after it')

    def test_bluetooth_started_after_the_wake_can_be_stopped(self):
        # 08.11.: the service came back with SIGTERM ignored (inherited from the helper's own trap), so the next sleep's
        # `brick-bluetooth sleep` could not end it and the Brick stayed awake. TERM and HUP reach it as normal (INT is
        # ignored by every background job of a script, the service's own start included; its stop sends TERM).
        for shell in [['dash']] + ([['busybox', 'ash']] if shutil.which('busybox') else []):
            with self.subTest(shell=shell):
                root, env, run = self.fixture(shell, bt=True)
                self.assertEqual(run().returncode, 0)
                ignored = int((root / 'bt-sigign').read_text().strip(), 16)
                for sig in (signal.SIGHUP, signal.SIGTERM):
                    self.assertFalse(ignored & (1 << (sig - 1)), f'{sig.name} ignored by the restarted service')

    def test_the_caller_does_not_wait_for_the_radios(self):
        # pocket-ui turns the screen on when this returns; Wi-Fi and Bluetooth take seconds to come back.
        root, env, run = self.fixture(['dash'], bt=True)
        Path(env['BRICK_WPA_INIT']).write_text(f"#!/bin/sh\nsleep 2\necho \"wpa $*\" >>'{root}/calls'\n")
        t0 = time.monotonic()
        result = subprocess.run(['dash', str(HELPERS / 'systemctl'), 'suspend'], env=env, capture_output=True,
                                text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLess(time.monotonic() - t0, 1.5)
        deadline = time.monotonic() + 6
        while (root / 'tmp/brick-suspend.lock').exists() and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertIn('wpa start', (root / 'calls').read_text())
        self.assertIn('radios back', (root / 'suspend.log').read_text())

    def test_a_second_sleep_waits_for_the_radios_of_the_last_wake(self):
        root, env, run = self.fixture(['dash'])
        Path(env['BRICK_WPA_INIT']).write_text(f"#!/bin/sh\nsleep 1\necho \"wpa $*\" >>'{root}/calls'\n")
        subprocess.run(['dash', str(HELPERS / 'systemctl'), 'suspend'], env=env, capture_output=True, timeout=10)
        self.assertEqual(run().returncode, 0)
        calls = (root / 'calls').read_text()
        self.assertLess(calls.index('wpa start'), calls.rindex('ubus call service delete'), 'one transition at a time')

    def test_a_lock_left_by_a_killed_helper_is_taken_over(self):
        root, env, run = self.fixture(['dash'])
        (root / 'tmp/brick-suspend.lock').mkdir()
        (root / 'tmp/brick-suspend.lock/pid').write_text('999999\n')
        self.assertEqual(run().returncode, 0)
        self.assertFalse((root / 'tmp/brick-suspend.lock').exists())

    def test_alarm_expiring_during_preparation_fails_awake_and_restores_wifi(self):
        root, env, run = self.fixture(['dash'])
        Path(env['BRICK_IFCONFIG']).write_text(
            f"#!/bin/sh\necho 130 >'{root}/sys/class/rtc/rtc0/since_epoch'\n")
        self.assertNotEqual(run().returncode, 0)
        self.assertIn('wpa start', (root / 'calls').read_text())
        self.assertNotIn('enter\n', (root / 'calls').read_text())
        self.assertFalse((root / 'tmp/system_suspend').exists())

    def test_the_helper_never_touches_an_rfkill(self):
        # 08.11.: the helper's Bluetooth rfkill cycles left the XR829's Bluetooth answering nothing after the wake, and a
        # Wi-Fi rfkill cycle restarted the kernel. Every rfkill file is read-only for the helper, in every outcome; the
        # source names no rfkill path at all.
        source = (HELPERS / 'brick-suspend').read_text()
        self.assertNotRegex(''.join(l for l in source.splitlines(True) if not l.lstrip().startswith('#')), r'rfkill')
        for outcome in ['success', 'failed', 'false-negative', 'no-suspend']:
            for bt in (False, True):
                with self.subTest(outcome=outcome, bt=bt):
                    root, env, run = self.fixture(['dash'], outcome, bt=bt)
                    states = sorted((root / 'sys/class/rfkill').glob('rfkill*/state'))
                    for f in states:
                        f.chmod(0o444)
                    before = {f: (f.read_text(), f.stat().st_mtime_ns) for f in states}
                    run()
                    self.assertEqual({f: (f.read_text(), f.stat().st_mtime_ns) for f in states}, before)
                    self.assertNotIn('cannot create', (root / 'suspend.log').read_text() if (root / 'suspend.log').exists() else '')

    def test_existing_dhcp_client_is_asked_to_renew_after_resume(self):
        root, env, run = self.fixture(['dash'])
        renewals = []
        previous = signal.signal(signal.SIGUSR1, lambda *_: renewals.append(True))
        self.addCleanup(signal.signal, signal.SIGUSR1, previous)
        Path(env['BRICK_PIDOF']).write_text(
            '#!/bin/sh\ncase "$1" in\nwpa_supplicant) echo 123;;\n'
            f'udhcpc) echo {os.getpid()};;\n*) exit 1;;\nesac\n')
        self.assertEqual(run().returncode, 0)
        self.assertEqual(renewals, [True], 'a retained address alone does not restore the default route')


if __name__ == '__main__':
    unittest.main()
