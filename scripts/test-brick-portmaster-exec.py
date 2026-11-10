#!/usr/bin/env python3
"""Exercise the real scoped process applets on Linux without chroot or root."""
import ctypes
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path(os.environ.get("BRICK_PORTMASTER_EXEC_SOURCE", ROOT / "deploy/brick/portmaster-exec.c"))
SCRIPT = Path(__file__).resolve()


def process_state(pid):
    try:
        return Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[0]
    except FileNotFoundError:
        return None


def wait_state(pid, stopped):
    until = time.monotonic() + 3
    while time.monotonic() < until:
        state = process_state(pid)
        if state is not None and ((state in ("T", "t")) == stopped):
            return state
        time.sleep(0.01)
    raise AssertionError(f"fixture process {pid}: expected stopped={stopped}, got {process_state(pid)}")


def read_json_line(process):
    readable, _, _ = select.select([process.stdout], [], [], 5)
    if not readable:
        raise AssertionError("fixture did not report readiness within five seconds")
    line = process.stdout.readline()
    if not line:
        raise AssertionError(f"fixture ended before readiness: {process.stderr.read()}")
    return json.loads(line)


def stop_owned(process):
    if process.poll() is None:
        # A failed STOP/CONT assertion must not leave an unresponsive fixture behind.
        try:
            os.kill(process.pid, signal.SIGCONT)
        except ProcessLookupError:
            pass
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=3)
    for stream in (process.stdin, process.stdout, process.stderr):
        if stream is not None:
            stream.close()


def spawn_helper(name, token, **kwargs):
    process = subprocess.Popen([sys.executable, str(SCRIPT), "--fixture-helper", name, token],
                               stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               text=True, **kwargs)
    try:
        info = read_json_line(process)
        assert info["pid"] == process.pid and info["name"] == name, info
        return process, info
    except BaseException:
        stop_owned(process)
        raise


def helper_main():
    name = sys.argv[2]
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(15, name.encode(), 0, 0, 0):  # PR_SET_NAME, used by /proc/PID/comm.
        raise OSError(ctypes.get_errno(), "cannot name fixture process")
    print(json.dumps({"pid": os.getpid(), "session": os.getsid(0), "group": os.getpgrp(),
                      "name": Path("/proc/self/comm").read_text().strip()}), flush=True)
    while True:
        signal.pause()


def worker_main():
    def interrupted(number, _frame):
        raise SystemExit(128 + number)

    signal.signal(signal.SIGTERM, interrupted)
    request = json.loads(sys.stdin.readline())
    helpers = []
    try:
        first, first_info = spawn_helper(request["name"], request["token"])
        helpers.append(first)
        # Both helpers belong to this port session, but only the first shares its group.
        second, second_info = spawn_helper(request["name"], request["token"], process_group=0)
        helpers.append(second)
        print(json.dumps({"session": os.getsid(0), "group": os.getpgrp(),
                          "helpers": [first_info, second_info]}), flush=True)

        def applet(mode, *args):
            result = subprocess.run([str(Path(request["bin"]) / mode), *args],
                                    text=True, capture_output=True, timeout=5)
            return {"code": result.returncode, "out": result.stdout, "err": result.stderr}

        mode = request["mode"]
        if mode == "pidof":
            result = applet("pidof", request["name"])
        elif mode == "killall":
            result = applet("killall", request["name"])
            result["exit"] = [process.wait(timeout=3) for process in helpers]
            result["after"] = applet("pidof", request["name"])
        elif mode == "pkill-full":
            result = applet("pkill", "-f", request["token"])
            result["exit"] = [process.wait(timeout=3) for process in helpers]
        elif mode == "signals":
            result = {"stop": applet("pkill", "-STOP", request["name"])}
            result["stopped"] = [wait_state(process.pid, True) for process in helpers]
            result["outside_state_during_stop"] = process_state(request["outside_pid"])
            result["pidof"] = applet("pidof", request["name"])
            result["continue"] = applet("pkill", "-CONT", request["name"])
            result["running"] = [wait_state(process.pid, False) for process in helpers]
            result["kill"] = applet("killall", request["kill_signal"], request["name"])
            result["exit"] = [process.wait(timeout=3) for process in helpers]
        elif mode == "missing":
            # A same-named process in another session must not count as a match.
            for process in helpers:
                stop_owned(process)
            result = {applet_name: applet(applet_name, request["name"])
                      for applet_name in ("pidof", "pkill", "killall")}
            result["no-argument"] = applet("pidof")
        else:
            raise AssertionError(f"unknown fixture operation {mode}")
        print(json.dumps(result), flush=True)
    finally:
        for process in helpers:
            stop_owned(process)


class ScopedProcessTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if sys.platform != "linux":
            raise unittest.SkipTest("scoped process applets use Linux /proc")
        cls.tmp = tempfile.TemporaryDirectory(prefix="brick-pm-exec-")
        cls.addClassCleanup(cls.tmp.cleanup)
        cls.bin = Path(cls.tmp.name)
        binary = cls.bin / "portmaster-exec"
        subprocess.run(["cc", "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", str(SOURCE), "-o", str(binary)],
                       check=True, text=True, capture_output=True, timeout=30)
        for name in ("pidof", "pkill", "killall"):
            (cls.bin / name).symlink_to(binary)
        # Call the production root setup without privileges. Only mount(2) is
        # replaced: the device bind exposes the same absolute shm symlink as stock.
        fixture = cls.bin / "setup-fixture.c"
        fixture.write_text(r'''#define main portmaster_production_main
#define mount fixture_mount
#include SOURCE
#undef mount
#undef main
int fixture_mount(const char *from, const char *to, const char *type, unsigned long flags, const void *data) {
    (void)type; (void)flags; (void)data;
    if (!strcmp(from, "/dev")) {
        char shm[PATH_MAX];
        if (path(shm, to, "shm")) return -1;
        return symlink("/tmp/shm", shm);
    }
    return 0;
}
int main(int argc, char **argv) {
    if (argc != 3) return 2;
    setenv("BRICK_CARD", argv[1], 1);
    setenv("BRICK_PORTMASTER_HOME", argv[2], 1);
    if (setup(argv[1], argv[2])) { perror("fixture setup"); return 3; }
    char shm[PATH_MAX], target[PATH_MAX], inside[PATH_MAX];
    if (path(shm, argv[1], "dev/shm")) return 4;
    ssize_t n = readlink(shm, target, sizeof target - 1);
    if (n < 1 || (size_t)n >= sizeof target - 1) return 5;
    target[n] = 0;
    if (strcmp(target, "/tmp/shm")) return 6;
    // Resolve the absolute target against the private root, as chroot(2) does.
    if (path(inside, argv[1], target + 1) || path(shm, inside, "portmaster") || make(shm)) {
        perror("private /dev/shm/portmaster"); return 7;
    }
    if (path(inside, shm, "probe")) return 8;
    FILE *out = fopen(inside, "w");
    if (!out) { perror("private shared-memory write"); return 9; }
    return fputs("private-portmaster-shm", out) < 0 || fclose(out);
}
'''.replace("#include SOURCE", "#include " + json.dumps(str(SOURCE))))
        cls.setup_fixture = cls.bin / "setup-fixture"
        subprocess.run(["cc", "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", str(fixture), "-o", str(cls.setup_fixture)],
                       check=True, text=True, capture_output=True, timeout=30)

    def exercise(self, mode, **options):
        name = "pmx" + uuid.uuid4().hex[:9]  # Linux comm is limited to 15 bytes.
        token = "port-helper-fixture-" + uuid.uuid4().hex
        outside, outside_info = spawn_helper(name, token, start_new_session=True)
        worker = None
        try:
            worker = subprocess.Popen([sys.executable, str(SCRIPT), "--session-worker"],
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                      text=True, start_new_session=True)
            worker.stdin.write(json.dumps(dict(bin=str(self.bin), mode=mode, name=name, token=token,
                                               outside_pid=outside.pid, **options)) + "\n")
            worker.stdin.flush()
            initial = read_json_line(worker)
            self.assertNotEqual(initial["session"], outside_info["session"])
            self.assertTrue(all(info["session"] == initial["session"] for info in initial["helpers"]))
            self.assertEqual(initial["helpers"][0]["group"], initial["group"])
            self.assertNotEqual(initial["helpers"][1]["group"], initial["group"])
            result = read_json_line(worker)
            worker.wait(timeout=15)
            errors = worker.stderr.read()
            self.assertEqual(worker.returncode, 0, errors)
            self.assertIsNone(outside.poll(), "scoped cleanup killed the other session's same-named helper")
            self.assertNotIn(process_state(outside.pid), (None, "T", "t", "Z"), "other session's helper was stopped")
            return result, {info["pid"] for info in initial["helpers"]}, outside.pid
        finally:
            if worker is not None:
                stop_owned(worker)
            stop_owned(outside)

    def test_stock_shared_memory_link_is_writable_only_inside_the_private_root(self):
        with tempfile.TemporaryDirectory(prefix="brick-pm-root-") as scratch:
            root = Path(scratch) / "root"
            root.mkdir()
            result = subprocess.run([str(self.setup_fixture), str(root), str(self.bin)],
                                    text=True, capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "dev/shm").readlink(), Path("/tmp/shm"))
            self.assertEqual((root / "tmp/shm").stat().st_mode & 0o7777, 0o1777)
            self.assertEqual((root / "tmp/shm/portmaster/probe").read_text(), "private-portmaster-shm")

    def test_pidof_finds_same_session_across_groups_and_excludes_other_session(self):
        result, expected, outside = self.exercise("pidof")
        self.assertEqual(result["code"], 0, result)
        self.assertEqual({int(pid) for pid in result["out"].split()}, expected)
        self.assertNotIn(str(outside), result["out"].split())

    def test_killall_terminates_same_session_helpers_in_both_groups(self):
        result, _, _ = self.exercise("killall")
        self.assertEqual(result["code"], 0, result)
        self.assertEqual(result["exit"], [-signal.SIGTERM, -signal.SIGTERM])
        self.assertEqual(result["after"]["code"], 1)
        self.assertEqual(result["after"]["out"], "")

    def test_pkill_full_command_matches_helpers_without_killing_other_session(self):
        result, _, _ = self.exercise("pkill-full")
        self.assertEqual(result["code"], 0, result)
        self.assertEqual(result["exit"], [-signal.SIGTERM, -signal.SIGTERM])

    def test_stop_continue_and_kill_stay_within_session(self):
        for kill_signal in ("-9", "-KILL"):
            with self.subTest(signal=kill_signal):
                result, expected, _ = self.exercise("signals", kill_signal=kill_signal)
                for command in ("stop", "continue", "kill", "pidof"):
                    self.assertEqual(result[command]["code"], 0, result)
                self.assertEqual({int(pid) for pid in result["pidof"]["out"].split()}, expected)
                self.assertNotIn(result["outside_state_during_stop"], (None, "T", "t", "Z"))
                self.assertEqual(result["exit"], [-signal.SIGKILL, -signal.SIGKILL])

    def test_other_session_alone_and_missing_argument_return_no_match(self):
        result, _, _ = self.exercise("missing")
        for command, value in result.items():
            self.assertEqual(value["code"], 1, (command, value))
            self.assertEqual(value["out"], "")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--fixture-helper":
        helper_main()
    elif len(sys.argv) > 1 and sys.argv[1] == "--session-worker":
        worker_main()
    else:
        unittest.main()
