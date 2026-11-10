#!/usr/bin/env python3
"""Real port supervisor lifecycle with only privileged filesystem calls replaced."""
import ctypes
import json
import os
from pathlib import Path
import select
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = Path(__file__).resolve()


def identity(pid):
    try:
        fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        return {'pid': pid, 'state': fields[0], 'parent': int(fields[1]),
                'session': int(fields[3]), 'birth': fields[19]}
    except (FileNotFoundError, ProcessLookupError):
        return None


def processes():
    return {int(p.name): row for p in Path('/proc').iterdir()
            if p.name.isdigit() and (row := identity(int(p.name))) is not None}


def alive(row):
    current = identity(row['pid'])
    return current and current['birth'] == row['birth'] and current['state'] not in ('Z', 'X')


def signal_owned(row, number):
    if alive(row):
        try:
            os.kill(row['pid'], number)
        except ProcessLookupError:
            pass


def wait_for(predicate, message, seconds=4):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        result = predicate()
        if result:
            return result
        time.sleep(.02)
    raise AssertionError(message)


def fixture_main(mode):
    if mode == '--ui':
        child = subprocess.Popen(['sh', os.environ['FIXTURE_RUN'], os.environ['FIXTURE_LAUNCHER']],
                                 start_new_session=True)
        print(json.dumps({'port': child.pid}), flush=True)
        while True:
            signal.pause()
    elif mode == '--game':
        writer = subprocess.Popen([sys.executable, str(SCRIPT), '--writer'], process_group=0)
        Path(os.environ['FIXTURE_READY']).write_text(json.dumps({'game': os.getpid(), 'writer': writer.pid}))
        while True:
            signal.pause()
    elif mode == '--writer':
        while True:
            if Path(os.environ['FIXTURE_OWNER_DEAD']).exists():
                Path(os.environ['FIXTURE_LATE_WRITE']).write_text('writer resumed after UI death')
            time.sleep(.01)


class LifecycleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if sys.platform != 'linux':
            raise unittest.SkipTest('Linux process sessions and /proc required')
        if ctypes.CDLL(None).prctl(36, 1, 0, 0, 0):  # Adopt and reap only our orphaned fixtures.
            raise OSError('PR_SET_CHILD_SUBREAPER failed')
        cls.tmp = tempfile.TemporaryDirectory(prefix='brick-pm-lifecycle-')
        cls.addClassCleanup(cls.tmp.cleanup)
        cls.base = Path(cls.tmp.name)
        source = cls.base / 'fixture.c'
        source.write_text(r'''#define main production_main
#define mount fixture_mount
#define umount2 fixture_umount2
#define chroot fixture_chroot
#include SOURCE
#undef main
int fixture_mount(const char *from, const char *to, const char *type, unsigned long flags, const void *data) {
    (void)from; (void)to; (void)type; (void)flags; (void)data; return 0;
}
int fixture_chroot(const char *root) { (void)root; return 0; }
int fixture_umount2(const char *root, int flags) {
    (void)flags;
    FILE *out = fopen(getenv("FIXTURE_UNMOUNTS"), "a");
    if (!out) return -1;
    int result = fprintf(out, "%s\n", root) < 0 || fclose(out);
    return result ? -1 : 0;
}
int main(int argc, char **argv) {
    if (argc == 4 && !strcmp(argv[1], "--same-process"))
        return same_process((pid_t)atoi(argv[2]), strtoull(argv[3], NULL, 10)) ? 0 : 1;
    return production_main(argc, argv);
}
'''.replace('#include SOURCE', '#include ' + json.dumps(str(ROOT / 'deploy/brick/portmaster-exec.c'))))
        cls.binary = cls.base / 'portmaster-exec'
        subprocess.run(['cc', '-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', source, '-o', cls.binary],
                       check=True, capture_output=True, timeout=30)

    def exercise(self, crash):
        with tempfile.TemporaryDirectory(prefix='case-', dir=self.base) as scratch:
            root = Path(scratch)
            app = root / 'app'
            (app / 'bin').mkdir(parents=True)
            (app / 'portmaster').mkdir()
            (root / 'run').mkdir()
            (root / 'pm').mkdir()
            (app / 'bin/bash').symlink_to(shutil.which('bash'))
            (app / 'bin/portmaster-exec').symlink_to(self.binary)
            shutil.copy(ROOT / 'deploy/brick/portmaster-run.sh', app / 'bin/portmaster-run')
            (app / 'portmaster/device-env.sh').write_text('ANALOG_STICKS=0\n')
            launcher = root / 'launcher.sh'
            launcher.write_text('#!/bin/bash\nunset PYTHONHOME\n' + json.dumps(sys.executable) + ' ' + json.dumps(str(SCRIPT)) + ' --game\n')
            env = dict(os.environ, BRICK_APP=str(app), BRICK_CARD=str(root), BRICK_DATA=str(root),
                       BRICK_RUN=str(root / 'run'), BRICK_PORTMASTER_HOME=str(root / 'pm'),
                       BRICK_PORTMASTER_RUNTIME=str(root / 'runtime'), HOME=str(root / 'home'),
                       FIXTURE_RUN=str(app / 'bin/portmaster-run'), FIXTURE_LAUNCHER=str(launcher),
                       FIXTURE_READY=str(root / 'ready'), FIXTURE_UNMOUNTS=str(root / 'unmounts'),
                       FIXTURE_OWNER_DEAD=str(root / 'owner-dead'), FIXTURE_LATE_WRITE=str(root / 'late-write'))
            for name in ('SWAYSOCK', 'PYTHONHOME'):
                env.pop(name, None)
            outside = subprocess.Popen(['sleep', '30'], start_new_session=True)
            ui = subprocess.Popen([sys.executable, str(SCRIPT), '--ui'], env=env,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
            owned = {}
            try:
                self.assertTrue(select.select([ui.stdout], [], [], 4)[0], 'fake UI did not start port')
                port = json.loads(ui.stdout.readline())['port']
                wait_for(lambda: (root / 'ready').exists(), 'actual supervisor did not reach launcher')
                info = json.loads((root / 'ready').read_text())
                snapshot = processes()
                members = {pid: row for pid, row in snapshot.items() if row['session'] == port}
                owned.update(members)
                self.assertIn(info['game'], members)
                self.assertIn(info['writer'], members)
                self.assertNotEqual(os.getpgid(info['writer']), port, 'fixture needs a mapper in another group')
                c = next(row for row in members.values() if row['parent'] == port)
                # Record supervisor children outside the game session, including its watcher.
                owned.update({pid: row for pid, row in snapshot.items() if row['parent'] == c['pid']})
                watchers = [row for row in owned.values() if row['session'] != port]
                self.assertEqual(len(watchers), 1, 'watcher must have left the paused port session')
                outside_id = identity(outside.pid)
                if crash:
                    for row in members.values():
                        signal_owned(row, signal.SIGSTOP)
                    wait_for(lambda: all(identity(pid)['state'] == 'T' for pid in members),
                             'every port member, including C, must really be stopped')
                    (root / 'owner-dead').touch()
                    ui.kill()
                    ui.wait(timeout=3)
                else:
                    signal_owned(members[info['game']], signal.SIGTERM)
                wait_for(lambda: all(not alive(row) for row in members.values()),
                         'UI death left a paused port session alive' if crash else 'normal port exit leaked children')
                wait_for(lambda: all(not alive(row) for row in owned.values()), 'port watcher leaked')
                wait_for(lambda: all(identity(row['pid']) is None for row in watchers), 'watcher was not reaped')
                self.assertTrue(alive(outside_id), 'another session was killed')
                self.assertFalse((root / 'late-write').exists(), 'cleanup resumed a paused card writer')
                self.assertTrue((root / 'unmounts').read_text(), 'supervisor did not unmount its private root')
                self.assertFalse(list((root / 'run').glob('portmaster-root-*')), 'private root was not removed')
            finally:
                ui.kill() if ui.poll() is None else None
                ui.wait(timeout=3)
                for row in owned.values():
                    signal_owned(row, signal.SIGKILL)
                outside.kill()
                outside.wait(timeout=3)
                ui.stdout.close()
                ui.stderr.close()
                end = time.monotonic() + 2
                while time.monotonic() < end:
                    try:
                        pid, _ = os.waitpid(-1, os.WNOHANG)
                        if pid == 0:
                            time.sleep(.02)
                        else:
                            continue
                    except ChildProcessError:
                        break

    def test_owner_identity_requires_the_same_proc_starttime(self):
        row = identity(os.getpid())
        for birth, expected in ((row['birth'], 0), (str(int(row['birth']) + 1), 1)):
            result = subprocess.run([self.binary, '--same-process', str(row['pid']), birth], timeout=3)
            self.assertEqual(result.returncode, expected)

    def test_ui_sigkill_cleans_stopped_session_without_resuming_writers(self):
        self.exercise(crash=True)

    def test_normal_port_exit_reaps_watcher_and_leaves_other_session_alive(self):
        self.exercise(crash=False)


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] in ('--ui', '--game', '--writer'):
        fixture_main(sys.argv[1])
    else:
        unittest.main()
