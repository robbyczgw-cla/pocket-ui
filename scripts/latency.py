#!/usr/bin/env python3
"""Speed guard: latency of pocket-ui and pocket-core, measured from their trace (POCKETUI_TRACE, src/trace.rs).

Two halves, one file (Python standard library only, so it runs on ROCKNIX's Python and on your computer):

  On the target (the virtual device or a Mini V2), started by scripts/latency.sh:
    latency.py measure --out FILE [--runs N] [--moves N] [--cycles N] [--systems gbc,gba,snes] ...
    latency.py measure --wake --out FILE [--wakes N] [--asleep S]   (root: suspends the device, docs/LATENCY.md#waking)
  It starts pocket-ui itself with the trace on, drives it through its command FIFO (/tmp/pocket-ui.in) like
  `vdev.sh send` / `dev.sh send`, reads the trace lines and writes the numbers as JSON. pocket-ui must not be
  running (latency.sh stops it first and starts it again the normal way afterwards).

  On your computer:
    latency.py table FILE                         the numbers as a table
    latency.py compare FILE BASELINE [--threshold 15] [--min-ms 3] [--min-cpu 0.3]
                                                  exit 1 when a number got more than threshold % slower
    latency.py merge OUT FILE...                  pools the runs of several files (medians recomputed)
    latency.py phases TRACE                       where the time of each Quit, pause menu and switcher went: the
                                                  steps both programs logged (`step` lines), from the button on
    latency.py selftest                           tests of the statistics and the comparison (no device needed)

What each number means: docs/LATENCY.md.
"""

import argparse
import errno
import json
import math
import os
import random
import signal
import statistics
import subprocess
import sys
import threading
import time

SCHEMA = 1
FIFO = "/tmp/pocket-ui.in"
SHELF = ("shelf", "games", "favorites")
# Lower is better for all of them. Unit and a short description for the table and docs/LATENCY.md.
METRICS = {
    "start.cold": ("ms", "pocket-ui started -> first shelf frame"),
    "start.process": ("ms", "pocket-ui main() -> first shelf frame"),
    "start.pads": ("ms", "pocket-ui main() -> its pads are open: a press from then on counts (before the first frame)"),
    "start.nocache": ("ms", "--cold: page cache dropped, pocket-ui started -> first shelf frame"),
    "start.nocache_process": ("ms", "--cold: the same from pocket-ui's main()"),
    "step.*": ("ms", "--cold: one step of pocket-ui's start, from its 'first frame after' log line"),
    "nav.p50": ("ms", "shelf move: input -> frame (median)"),
    "nav.p95": ("ms", "shelf move: input -> frame (95th pct)"),
    "open.settings": ("ms", "START on the shelf -> Settings frame"),
    "open.switcher": ("ms", "SELECT on the shelf -> switcher frame"),
    "open.pause_menu": ("ms", "HOME in a game -> pause menu frame"),
    "open.pause_menu_again": ("ms", "HOME after Continue -> pause menu frame"),
    "open.switcher_game": ("ms", "SELECT in the pause menu -> switcher frame"),
    "launch.*": ("ms", "A on a game -> first game frame (pocket-core)"),
    "quit.to_shelf": ("ms", "Quit in the pause menu -> shelf frame"),
    "switch.game": ("ms", "Switch in the switcher -> first frame of the other game"),
    "cpu.shelf_idle": ("%", "pocket-ui CPU, shelf, no input (% of one core)"),
    "cpu.behind_game": ("%", "pocket-ui CPU behind a running game (% of one core)"),
    # --wake (docs/LATENCY.md#waking): from the moment the system is back (the kernel's suspend counter went up), or
    # for light sleep from the button.
    "wake.*": ("ms", "--wake: system back from deep sleep -> ..."),
    "light.*": ("ms", "--wake: button in light sleep -> ..."),
}
for _w in ("shelf", "game"):
    METRICS.update({
        f"wake.{_w}.screen_on": ("ms", f"deep sleep, {_w}: system back -> backlight on"),
        f"wake.{_w}.frame": ("ms", f"deep sleep, {_w}: system back -> " + ("first shelf frame" if _w == "shelf" else "first game frame after the wake")),
        f"wake.{_w}.input": ("ms", f"deep sleep, {_w}: system back -> a pad press reaches pocket-ui (InputPlumber)"),
        f"light.{_w}.screen_on": ("ms", f"light sleep, {_w}: button -> backlight on"),
        f"light.{_w}.frame": ("ms", f"light sleep, {_w}: button -> " + ("first shelf frame" if _w == "shelf" else "first game frame after the wake")),
    })
METRICS.update({
    "wake.shelf.acts": ("ms", "deep sleep, shelf: system back -> the shelf shows a press (after the 600 ms guard)"),
    "wake.game.game_on": ("ms", "deep sleep, game: system back -> pocket-core 'awake' (the game runs)"),
    "wake.game.sound": ("ms", "deep sleep, game: system back -> the sound fades in"),
    "light.game.game_on": ("ms", "light sleep, game: button -> pocket-core 'awake' (the game runs)"),
})


def unit_of(name):
    return METRICS.get(name, METRICS.get(name.split(".")[0] + ".*", ("ms", "")))[0]


def describe(name):
    return METRICS.get(name, METRICS.get(name.split(".")[0] + ".*", ("ms", "")))[1]


def percentile(values, p):
    """Nearest-rank percentile (p in 0..100) of a non-empty list."""
    v = sorted(values)
    k = max(0, min(len(v) - 1, math.ceil(p / 100.0 * len(v)) - 1))
    return v[k]


def now_us():
    return int(time.monotonic() * 1_000_000)


def log(msg):
    print("latency: " + msg, file=sys.stderr, flush=True)


class Fail(Exception):
    pass


# --- target side -----------------------------------------------------------------------------------------


class Trace:
    """Follows the trace file. Lines: `<mono us> <program> <pid> <event> [detail]`."""

    def __init__(self, path, fresh=True):
        self.path = path
        self.pos = 0
        self.buf = ""
        self.events = []
        if fresh:
            open(path, "w").close()

    def poll(self):
        try:
            with open(self.path) as f:
                f.seek(self.pos)
                data = f.read()
                self.pos = f.tell()
        except FileNotFoundError:
            return
        self.buf += data
        *lines, self.buf = self.buf.split("\n")
        for line in lines:
            parts = line.split(" ", 4)
            if len(parts) < 4:
                continue
            try:
                ev = {"t": int(parts[0]), "prog": parts[1], "pid": int(parts[2]), "event": parts[3], "detail": parts[4] if len(parts) > 4 else ""}
            except ValueError:
                continue
            if ev["event"] == "frame":
                # "<screen> in=<n> sel=<entry, may have spaces>"
                d = ev["detail"]
                screen, _, rest = d.partition(" ")
                ev["screen"] = screen
                inp, _, sel = rest.partition(" ")
                ev["in"] = int(inp[3:]) if inp.startswith("in=") and inp[3:].isdigit() else 0
                ev["sel"] = sel[4:] if sel.startswith("sel=") else ""
            elif ev["event"] == "paint":
                # "<ms> <w>x<h>": how long the frame before it took, and the window's size then.
                f = ev["detail"].split()
                try:
                    w, h = f[1].split("x")
                    ev["size"] = (int(w), int(h))
                except (IndexError, ValueError):
                    ev["size"] = None
            elif ev["event"] == "input":
                f = ev["detail"].split()
                ev["n"] = int(f[0]) if f and f[0].isdigit() else 0
                ev["button"] = f[1] if len(f) > 1 else ""
                ev["down"] = len(f) > 2 and f[2] == "down"
            self.events.append(ev)

    def wait(self, pred, after_us, timeout, what):
        """The first event after `after_us` that `pred` accepts; polls every millisecond."""
        end = time.monotonic() + timeout
        i = 0
        while True:
            self.poll()
            while i < len(self.events):
                ev = self.events[i]
                i += 1
                if ev["t"] >= after_us and pred(ev):
                    return ev
            if time.monotonic() > end:
                raise Fail(f"timed out after {timeout} s waiting for {what}")
            time.sleep(0.001)

    def last(self, pred):
        self.poll()
        for ev in reversed(self.events):
            if pred(ev):
                return ev
        return None


def send(cmd, timeout=3.0):
    """One command into pocket-ui's FIFO. Returns the monotonic time (us) just before the write."""
    end = time.monotonic() + timeout
    while True:
        try:
            fd = os.open(FIFO, os.O_WRONLY | os.O_NONBLOCK)
            break
        except OSError as e:
            # No reader at this moment (pocket-ui reopens the FIFO after every writer).
            if e.errno not in (errno.ENXIO, errno.ENOENT) or time.monotonic() > end:
                raise Fail(f"cannot write to {FIFO}: {e}")
            time.sleep(0.001)
    try:
        t = now_us()
        os.write(fd, (cmd + "\n").encode())
    finally:
        os.close(fd)
    return t


def pids_of(name):
    try:
        out = subprocess.run(["pidof", name], capture_output=True, text=True).stdout
    except FileNotFoundError:
        return []
    return [int(p) for p in out.split()]


def cpu_seconds(pid, proc="/proc"):
    """CPU time of `pid` and all its threads in seconds, and where it came from: (seconds, source).

    Sources, in this order (docs/LATENCY.md, "CPU"):
    1. `stat`: utime + stime of /proc/<pid>/stat (fields 14 and 15, in clock ticks: 10 ms on the Mini V2). Every Linux
       kernel has it, and it counts the threads that ended too.
    2. `schedstat`: the first field of /proc/<pid>/task/*/schedstat (ns), only with CONFIG_SCHEDSTATS. The Mini V2's
       kernel has it off: reading only this, the speed guard took 0.00 % for every CPU number before 2026-10-26.
    (None, None) when the process is gone; (None, "none") when it is there but neither can be read. Never 0 for
    "unknown": the caller reports n/a and the comparison fails."""
    base = f"{proc}/{pid}"
    try:
        with open(f"{base}/stat") as f:
            fields = f.read().rsplit(")", 1)[1].split()
        # After the name in brackets (which may hold spaces and brackets) field 3 is fields[0]: utime is field 14.
        return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK"), "stat"
    except FileNotFoundError:
        if not os.path.isdir(base):
            return None, None
    except (OSError, ValueError, IndexError):
        pass
    try:
        tasks = os.listdir(f"{base}/task")
    except FileNotFoundError:
        return (None, None) if not os.path.isdir(base) else (None, "none")
    total, found = 0, False
    for tid in tasks:
        try:
            with open(f"{base}/task/{tid}/schedstat") as f:
                total += int(f.read().split()[0])
                found = True
        except (OSError, ValueError, IndexError):
            pass
    return (total / 1e9, "schedstat") if found else (None, "none")


def cpu_percent(pid, secs, proc="/proc"):
    """`pid`'s CPU over the next `secs` seconds in % of one core, and its source; (None, why) when it can't be had."""
    (a, src_a), ta = cpu_seconds(pid, proc), time.monotonic()
    time.sleep(secs)
    (b, src_b), tb = cpu_seconds(pid, proc), time.monotonic()
    if src_a is None or src_b is None:
        raise Fail("pocket-ui went away during the CPU measurement")
    if a is None or b is None or src_a != src_b:
        return None, f"no CPU source for pid {pid}: neither {proc}/{pid}/stat nor {proc}/{pid}/task/*/schedstat could be read"
    return (b - a) / (tb - ta) * 100.0, src_a


def dev_env(path):
    """POCKETUI_*, POCKET_*, GALLIUM_* lines of dev.env, read like deploy/run-pocket-ui.sh does."""
    env = {}
    try:
        lines = open(path).read().splitlines()
    except OSError:
        return env
    for line in lines:
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, v = line.split("=", 1)
        if not k.startswith(("POCKETUI_", "POCKET_", "GALLIUM_")):
            continue
        if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
            v = v[1:-1]
        env[k] = v
    return env


class Ui:
    """pocket-ui started by us, with the trace on.

    It runs outside the essway unit, with POCKETUI_TRACE and POCKETUI_NO_HISTORY set: left behind it blocks the next
    deploy ("text busy") and keeps real play from being recorded (2026-10-24, scripts/verify-device.sh). So every Ui
    that has a process is in LIVE until stop() has ended that process, and cmd_measure ends what is left however it
    ends (an error, SIGTERM, SIGHUP)."""

    LIVE = []

    def __init__(self, a, trace):
        self.a = a
        self.trace = trace
        self.proc = None
        self.started_us = None  # pocket-ui's `ui-start` (trace clock) of the last start
        self.quit_timeout = 2  # s to wait for the FIFO when asking pocket-ui to quit
        self.kill_cores = True  # stop() also ends a pocket-core a game left behind
        self.log = open(a.ui_log, "a")
        # More variables for this pocket-ui (--wake: the suspend command with its alarm, short sleep minutes).
        self.extra_env = {}

    def start(self):
        env = dict(os.environ)
        env.setdefault("XDG_RUNTIME_DIR", "/var/run/0-runtime-dir")
        env.setdefault("WAYLAND_DISPLAY", "wayland-1")
        env.setdefault("SWAYSOCK", "/var/run/0-runtime-dir/sway-ipc.0.sock")
        env.update(dev_env(os.path.join(self.a.root, "dev.env")))
        env["POCKETUI_TRACE"] = self.a.trace
        # The games this measurement starts are no play: nothing goes into the player's history (docs/HISTORY.md).
        env["POCKETUI_NO_HISTORY"] = "1"
        # Switching off after sleeping writes the RTC's wake alarm before every suspend (src/auto_off.rs) and would
        # replace the alarm --wake sets, leaving the Pocket asleep until someone presses the power key (2026-10-26).
        # A measured pocket-ui writes into a file of its own instead, and never switches the Pocket off.
        env["POCKETUI_RTC_ALARM"] = "/tmp/pocket-latency-rtc-alarm"
        env.update(self.extra_env)
        t0 = now_us()
        self.proc = subprocess.Popen([os.path.join(self.a.root, "pocket-ui")], cwd=self.a.root, env=env, stdout=self.log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL)
        Ui.LIVE.append(self)
        pid = self.proc.pid
        first = self.trace.wait(lambda e: e["event"] == "frame" and e["pid"] == pid and e["screen"] in SHELF, t0, 30, "the first shelf frame")
        started = self.trace.last(lambda e: e["event"] == "ui-start" and e["pid"] == pid)
        self.started_us = started["t"] if started else None
        return (first["t"] - t0) / 1000.0, ((first["t"] - started["t"]) / 1000.0 if started else None)

    def pads_open_ms(self, timeout=1.0):
        """From pocket-ui's main() to its pads being open (its `pad-open` line), for the start `start` just measured;
        None when the line didn't come (an older pocket-ui)."""
        pid = self.proc.pid if self.proc else None
        try:
            ev = self.trace.wait(lambda e: e["event"] == "pad-open" and e["pid"] == pid, 0, timeout, "the pads open")
        except Fail:
            return None
        return (ev["t"] - self.started_us) / 1000.0 if self.started_us else None

    def stop(self):
        """Ends our pocket-ui: asks it to quit, kills it when it does not, and waits until it is gone."""
        if not self.proc:
            return
        if self.proc.poll() is None:
            try:
                send("quit", timeout=self.quit_timeout)
                self.proc.wait(timeout=5)
            except (Fail, subprocess.TimeoutExpired):
                self.proc.kill()
                self.proc.wait()
        self.proc = None
        if self in Ui.LIVE:
            Ui.LIVE.remove(self)
        # A game it left behind (it should not, pocket-core dies with it).
        if self.kill_cores:
            for p in pids_of("pocket-core"):
                try:
                    os.kill(p, 15)
                except OSError:
                    pass

    @staticmethod
    def stop_all():
        """Ends every pocket-ui this process started and has not stopped (the safety net under Run.go's finally)."""
        for ui in list(Ui.LIVE):
            try:
                ui.stop()
            except Exception as e:  # never let one failure leave the others running
                log(f"could not stop pocket-ui cleanly: {e}")
                if ui.proc and ui.proc.poll() is None:
                    ui.proc.kill()

    @property
    def pid(self):
        return self.proc.pid


class Run:
    """One pass over everything, on a pocket-ui it starts itself. Samples go into `self.samples`."""

    def __init__(self, a, trace, warm=False):
        self.a = a
        self.trace = trace
        self.ui = Ui(a, trace)
        self.samples = {}
        # CPU numbers that were to be measured but had no source: name -> why (they end up as n/a, never as 0).
        self.unmeasured = {}
        self.cpu_sources = set()
        self.warm = warm

    def add(self, name, value):
        if value is not None and not self.warm:
            self.samples.setdefault(name, []).append(round(value, 3))

    def add_cpu(self, name, measured):
        value, source = measured
        if value is None:
            log(f"{name}: NOT MEASURED, {source}")
            if not self.warm:
                self.unmeasured[name] = source
            return
        self.cpu_sources.add(source)
        self.add(name, value)

    def frame_now(self):
        ev = self.trace.last(lambda e: e["event"] == "frame" and e["pid"] == self.ui.pid)
        if not ev:
            raise Fail("no frame from pocket-ui yet")
        return ev

    def press(self, cmd, timeout=5.0):
        """Sends a button; returns (send time, the first frame that shows it, its input number)."""
        t = send(cmd)
        inp = self.trace.wait(lambda e: e["event"] == "input" and e["down"] and e["pid"] == self.ui.pid, t, timeout, f"pocket-ui to take '{cmd}'")
        fr = self.trace.wait(lambda e: e["event"] == "frame" and e["pid"] == self.ui.pid and e["in"] >= inp["n"], inp["t"], timeout, f"a frame after '{cmd}'")
        return t, fr, inp

    def until_screen(self, cmd, screens, timeout=10.0):
        """Sends a button and waits for the first frame on one of `screens`; returns (latency ms, frame).

        A frame at less than the window's full size (the `paint` line after it says so) doesn't count: drawn
        before sway's resize came in, it is not what the screen shows (docs/LATENCY.md)."""
        t = send(cmd)
        after = t
        while True:
            fr = self.trace.wait(lambda e: e["event"] == "frame" and e["pid"] == self.ui.pid and e["screen"] in screens, after, timeout, f"{'/'.join(screens)} after '{cmd}'")
            if not self.small_frame(fr):
                return (fr["t"] - t) / 1000.0, fr
            after = fr["t"] + 1

    def small_frame(self, fr):
        """True when the `paint` line after frame `fr` gives a size smaller than the largest seen so far."""
        try:
            # Written right after the frame line, in the same microsecond or so.
            paint = self.trace.wait(lambda e: e["event"] in ("paint", "frame") and e["pid"] == self.ui.pid and e is not fr, fr["t"], 0.05, "the frame's paint line")
        except Fail:
            return False
        size = paint.get("size") if paint["event"] == "paint" else None
        if not size:
            return False  # a pocket-ui without paint lines: every frame counts, as before
        sizes = [e["size"] for e in self.trace.events if e["event"] == "paint" and e["pid"] == self.ui.pid and e.get("size")]
        full = max(sizes, key=lambda s: s[0] * s[1])
        return size[0] * size[1] < full[0] * full[1]

    def settle(self, secs=None):
        time.sleep(self.a.settle if secs is None else secs)

    # --- the parts ---

    def nocache_starts(self):
        """--cold: each start after `drop_caches`, as at power-on: every file pocket-ui reads comes from the card.

        Also keeps the steps of pocket-ui's own 'first frame after ... (logs 0.6, ..., drawn 24.1 ms)' line
        (src/startup.rs) as step.<name>, so a change shows which step it moved."""
        for i in range(self.a.starts):
            drop_caches()
            log_at = os.path.getsize(self.a.ui_log) if os.path.exists(self.a.ui_log) else 0
            cold, proc = self.ui.start()
            self.add("start.nocache", cold)
            self.add("start.nocache_process", proc)
            # The log line comes right after the frame; give it a moment.
            for _ in range(20):
                steps = startup_steps(self.a.ui_log, log_at)
                if steps:
                    break
                time.sleep(0.05)
            for name, ms in (steps or {}).items():
                self.add("step." + name, ms)
            # The scan after the first frame (library cache) and the jobs put off until then run before the next start.
            self.settle(1.5)
            self.ui.stop()
            time.sleep(0.5)

    def cold_starts(self):
        for i in range(self.a.starts):
            cold, proc = self.ui.start()
            self.add("start.cold", cold)
            self.add("start.process", proc)
            self.add("start.pads", self.ui.pads_open_ms())
            if i < self.a.starts - 1:
                self.ui.stop()
                time.sleep(0.5)
        self.settle(1.0)

    def to_home(self):
        for _ in range(4):
            if self.frame_now()["screen"] == "shelf":
                return
            self.until_screen("b", SHELF)
            self.settle(0.3)
        raise Fail(f"cannot get back to the shelf (on '{self.frame_now()['screen']}')")

    def goto(self, want, limit=200):
        """Moves the selection until `want(sel)`: down to the end of the list, then up to its start."""
        direction = "down"
        for _ in range(limit):
            sel = self.frame_now()["sel"]
            if want(sel):
                return sel
            _, fr, _ = self.press(direction)
            time.sleep(0.03)
            if fr["sel"] == sel:
                if direction == "up":
                    break
                direction = "up"
        raise Fail(f"no such entry in the list (last: {self.frame_now()['sel']})")

    def navigation(self):
        self.to_home()
        # Up and down over the whole list, so every move changes the selection.
        n = 0
        direction = "down"
        lat = []
        stuck = 0
        while n < self.a.moves:
            before = self.frame_now()["sel"]
            t, fr, _ = self.press(direction)
            if fr["sel"] == before:
                # The end of the list: turn round (twice in a row means a list of one).
                stuck += 1
                if stuck > 2:
                    raise Fail("the shelf has a single entry: nothing to move between")
                direction = "up" if direction == "down" else "down"
                time.sleep(self.a.gap / 1000.0)
                continue
            stuck = 0
            lat.append((fr["t"] - t) / 1000.0)
            n += 1
            time.sleep((self.a.gap + random.uniform(-20, 20)) / 1000.0)
        if not self.warm and lat:
            self.samples.setdefault("nav", []).extend(lat)
        self.settle(0.5)

    def menus_on_shelf(self):
        self.to_home()
        for _ in range(self.a.opens):
            ms, _ = self.until_screen("start", ("settings",))
            self.add("open.settings", ms)
            self.settle(0.4)
            self.until_screen("b", SHELF)
            self.settle(0.4)
        for _ in range(self.a.opens):
            ms, _ = self.until_screen("select", ("switcher",))
            self.add("open.switcher", ms)
            self.settle(0.4)
            self.until_screen("b", SHELF)
            self.settle(0.4)

    def cpu_shelf(self):
        self.to_home()
        self.settle(2.0)
        self.add_cpu("cpu.shelf_idle", cpu_percent(self.ui.pid, self.a.cpu_secs))

    def open_game(self, spec):
        """Shelf -> the system -> the game named by `spec` (system[:part of the file name]). Returns its sel."""
        system, _, part = spec.partition(":")
        self.to_home()
        self.goto(lambda s: s == f"system:{system}")
        self.until_screen("a", ("games",))
        self.settle(0.3)
        return self.goto(lambda s: s.startswith(f"game:{system}/") and part.lower() in s.lower())

    def launch(self, spec):
        system = spec.partition(":")[0]
        self.open_game(spec)
        self.settle(0.3)
        t = send("a")
        fr = self.trace.wait(lambda e: e["event"] == "core-frame", t, 60, f"the first frame of the {system} game")
        self.add(f"launch.{system}", (fr["t"] - t) / 1000.0)
        return fr["pid"]

    def pause_menu(self, metric="open.pause_menu"):
        ms, _ = self.until_screen("home", ("menu",))
        self.add(metric, ms)
        self.settle(0.5)

    def pause_menu_again(self):
        """Continue, play a moment, HOME again: the pause menu as it opens in play, after its window was moved
        behind the game once (the first one after a start never was)."""
        send("a")
        self.settle(1.0)
        self.pause_menu("open.pause_menu_again")

    def quit_game(self):
        # The pause menu is open on Continue; Quit is the last entry (Down stops there). Pixel Path and Achievements add
        # rows (seven with Pixel Path), so press more than there can be rather than counting them.
        for _ in range(12):
            self.press("down")
            time.sleep(0.05)
        t = send("a")
        ex = self.trace.wait(lambda e: e["event"] == "game-exited" and e["pid"] == self.ui.pid, t, 60, "the game to end")
        fr = self.trace.wait(lambda e: e["event"] == "frame" and e["pid"] == self.ui.pid and e["screen"] in SHELF, ex["t"], 10, "the shelf after the game")
        self.add("quit.to_shelf", (fr["t"] - t) / 1000.0)
        self.settle(1.0)

    def game_cycle(self, with_cpu):
        systems = self.a.systems
        first_core = None
        for i, spec in enumerate(systems):
            core = self.launch(spec)
            first_core = first_core or core
            self.settle()
            if with_cpu and i == 0:
                self.add_cpu("cpu.behind_game", cpu_percent(self.ui.pid, self.a.cpu_secs))
            self.pause_menu()
            if i == 0:
                self.pause_menu_again()
            if i < len(systems) - 1 or len(systems) < 2:
                self.quit_game()
                continue
            # The last game: the switcher from its pause menu, back to the game before it.
            ms, _ = self.until_screen("select", ("switcher",))
            self.add("open.switcher_game", ms)
            self.settle(0.5)
            self.press("right")
            self.settle(0.5)
            t = send("a")
            fr = self.trace.wait(lambda e: e["event"] == "core-frame" and e["pid"] != core, t, 60, "the first frame of the game switched to")
            self.add("switch.game", (fr["t"] - t) / 1000.0)
            self.settle()
            self.pause_menu()
            self.quit_game()

    def go(self):
        if self.a.cold:
            try:
                self.nocache_starts()
            finally:
                self.ui.stop()
            return {k: statistics.median(v) for k, v in self.samples.items()}
        try:
            self.cold_starts()
            if not self.warm:
                self.navigation()
                self.menus_on_shelf()
                self.cpu_shelf()
            cycles = 1 if self.warm else self.a.cycles
            for c in range(cycles):
                if self.a.systems:
                    self.game_cycle(with_cpu=c == 0 and not self.warm)
        finally:
            self.ui.stop()
        out = {}
        for k, v in self.samples.items():
            if k == "nav":
                out["nav.p50"] = percentile(v, 50)
                out["nav.p95"] = percentile(v, 95)
            else:
                out[k] = statistics.median(v)
        return out


# --- --wake: deep and light sleep, and how fast everything is back -----------------------------------------------

RTC_ALARM = "/sys/class/rtc/rtc0/wakealarm"
SUSPEND_OK = "/sys/power/suspend_stats/success"


def read_int(path):
    try:
        with open(path) as f:
            return int(f.read().strip())
    except (OSError, ValueError):
        return None


def backlight_file():
    """The backlight sleep dims, as src/sleep.rs finds it (POCKETUI_BACKLIGHT, else the first one)."""
    d = os.environ.get("POCKETUI_BACKLIGHT")
    if d:
        return os.path.join(d, "brightness")
    try:
        for n in sorted(os.listdir("/sys/class/backlight")):
            f = os.path.join("/sys/class/backlight", n, "brightness")
            if os.path.exists(f):
                return f
    except OSError:
        pass
    return None


def wake_metrics(where, start, ev, light=False):
    """The numbers of one wake from what was seen (`ev`: name -> monotonic us), in ms after `start`.

    Deep: `start` is when the system was back. Light: when the button was sent. Missing observations are left out
    (an older build writes fewer trace lines; the virtual device has no backlight)."""
    names = {
        "screen_on": "screen_on",
        "frame": "frame",
        "game_on": "game_on",
        "input": "input",
        "acts": "acts",
        "sound": "sound",
    }
    # Not numbers to compare: ROCKNIX's wake hooks (`hooks`) are not ours, the log line (`log_woke`) moved on purpose.
    # Both stay in the line each wake logs.
    prefix = ("light." if light else "wake.") + where + "."
    out = {}
    for k, name in names.items():
        if k in ev and ev[k] >= start:
            out[prefix + name] = (ev[k] - start) / 1000.0
    return out


class LogTail:
    """The lines added to a log file since `at`, a few at a time."""

    def __init__(self, path, at):
        self.path, self.pos, self.buf = path, at, ""

    def new_lines(self):
        try:
            with open(self.path, errors="replace") as f:
                f.seek(self.pos)
                data = f.read()
                self.pos = f.tell()
        except OSError:
            return []
        self.buf += data
        *lines, self.buf = self.buf.split("\n")
        return lines


class WakeRun(Run):
    """--wake: deep sleep (the power key, a suspend with an RTC alarm) and light sleep (idle), on the shelf and in a game.

    What is watched works with older builds too, for a before/after: the kernel's suspend counter for "back", the
    backlight for "screen on", pocket-core's "awake" line in the log for "game on", and an InputPlumber button for
    "input" (its `input` line in the trace). Newer builds add their own trace lines: the first game frame after the
    wake (`core-frame wake`), the sound fading in (`audio: playing again`), ROCKNIX's wake hooks done."""

    def __init__(self, a, trace):
        super().__init__(a, trace)
        self.bl = backlight_file()
        self.minute = a.minute_secs
        if a.mock_suspend:
            a.suspend_cmd = f"sleep {a.mock_suspend}"
        cmd = a.suspend_cmd or f"echo 0 > {RTC_ALARM}; echo +{a.asleep} > {RTC_ALARM}; systemctl suspend"
        self.ui.extra_env = {
            "POCKETUI_SUSPEND_CMD": cmd,
            # The harness's `power` (a short press through the FIFO) is pocket-ui's, with or without logind's inhibitor.
            "POCKETUI_POWER_KEY": "1",
            # Light sleep after a few seconds without input instead of minutes; idle never goes on to a suspend.
            "POCKETUI_SLEEP_MINUTE_SECS": str(self.minute),
            "POCKETUI_NO_AUTOSUSPEND": "1",
        }
        self.inject = None if a.no_inject or not os.path.exists("/usr/bin/busctl") else a.wake_button
        self.light_after = self.light_minutes()

    def light_minutes(self):
        """The player's "Screen off after" in minutes (None: Never), from sleep.json (`screen_off_secs`, 0 = never; a
        file from before Settings → System → Sleep & power has `light_after` in minutes); 2 when there is none."""
        data = os.environ.get("POCKETUI_DATA", "/storage/.config/pocket-ui")
        try:
            with open(os.path.join(data, "sleep.json")) as f:
                cfg = json.load(f)
        except (OSError, ValueError):
            return 2
        if "screen_off_secs" in cfg:
            secs = cfg["screen_off_secs"]
            return secs / 60 if secs else None
        return cfg.get("light_after", 2) if "light_after" in cfg else 2

    def level(self):
        return read_int(self.bl) if self.bl else None

    def log_size(self):
        try:
            return os.path.getsize(self.a.ui_log)
        except OSError:
            return 0

    def wait_log(self, at, pred, timeout, what):
        end = time.monotonic() + timeout
        tail = LogTail(self.a.ui_log, at)
        while time.monotonic() < end:
            if any(pred(line) for line in tail.new_lines()):
                return now_us()
            time.sleep(0.005)
        raise Fail(f"timed out after {timeout} s waiting for {what}")

    def ensure_awake(self):
        """A light sleep that came meanwhile (idle minutes are seconds here) would take the next `power` as a wake."""
        if self.level() == 0:
            send("b")
            end = time.monotonic() + 5
            while self.level() == 0 and time.monotonic() < end:
                time.sleep(0.01)
            time.sleep(1.0)

    def press_button(self):
        """One InputPlumber button press, the way a finger's goes in (`SendButtonChord`, 80 ms)."""
        try:
            subprocess.run(
                ["busctl", "call", "org.shadowblip.InputPlumber", "/org/shadowblip/InputPlumber/CompositeDevice0",
                 "org.shadowblip.Input.CompositeDevice", "SendButtonChord", "as", "1", self.inject],
                capture_output=True, timeout=2,
            )
        except (OSError, subprocess.TimeoutExpired):
            pass

    def watch(self, start, where, log_at, pre_sel, light=False, window=None):
        """Polls every 5 ms from `start`: backlight, log and trace. Returns what was seen (name -> us)."""
        ev = {}
        window = window or self.a.wake_window
        end = time.monotonic() + window
        stop = threading.Event()
        button = {"Gamepad:Button:DPadDown": "Down", "Gamepad:Button:DPadUp": "Up"}.get(self.inject or "", "")
        injector = None
        if self.inject and not light:
            def inject():
                while not stop.is_set():
                    self.press_button()
                    stop.wait(0.1)
            injector = threading.Thread(target=inject, daemon=True)
            injector.start()
        need = {"screen_on", "game_on" if where == "game" else "frame"}
        if self.inject and not light:
            need |= {"input"} | ({"acts"} if where == "shelf" else set())
        done_at = None
        # Only what is new since the last look (the trace of a run has thousands of lines; this runs 200 times a second).
        self.trace.poll()
        i = len(self.trace.events)
        while i > 0 and self.trace.events[i - 1]["t"] >= start:
            i -= 1
        tail = LogTail(self.a.ui_log, log_at)
        try:
            while time.monotonic() < end:
                t = now_us()
                lvl = self.level()
                if "screen_on" not in ev and lvl is not None and lvl > 0:
                    ev["screen_on"] = t
                for line in tail.new_lines():
                    if "game_on" not in ev and where == "game" and line.startswith("awake"):
                        ev["game_on"] = t
                    if "log_woke" not in ev and line.startswith("sleep: woke after"):
                        ev["log_woke"] = t
                self.trace.poll()
                new, i = self.trace.events[i:], len(self.trace.events)
                for e in new:
                    if e["t"] < start:
                        continue
                    if e["event"] == "step" and e["detail"] == "sleep: screen on" and "screen_on" not in ev and lvl is None:
                        ev["screen_on"] = e["t"]
                    elif e["event"] == "step" and e["detail"] == "sleep: wake hooks done":
                        ev.setdefault("hooks", e["t"])
                    elif e["event"] == "step" and e["detail"] == "audio: playing again" and where == "game":
                        ev.setdefault("sound", e["t"])
                    elif e["event"] == "core-frame" and e["detail"] == "wake" and where == "game":
                        ev.setdefault("frame", e["t"])
                    elif e["event"] == "frame" and e["pid"] == self.ui.pid and e.get("screen") in SHELF and where == "shelf":
                        ev.setdefault("frame", e["t"])
                        if pre_sel is not None and e.get("sel") != pre_sel:
                            ev.setdefault("acts", e["t"])
                    elif e["event"] == "input" and e["pid"] == self.ui.pid and e.get("down") and e.get("button") == button:
                        ev.setdefault("input", e["t"])
                if "input" in ev and (where == "game" or "acts" in ev):
                    stop.set()
                if need <= ev.keys():
                    # The lines only newer builds write (sound, hooks) get a little longer.
                    done_at = done_at or time.monotonic()
                    if time.monotonic() - done_at > 1.5 or {"sound", "hooks"} <= ev.keys() or light:
                        break
                time.sleep(0.005)
        finally:
            stop.set()
            if injector:
                injector.join(timeout=3)
        missing = sorted(need - ev.keys())
        if missing:
            log(f"  {'light' if light else 'deep'} sleep, {where}: not seen within {window} s: {', '.join(missing)}")
        return ev

    def deep_cycle(self, where):
        self.ensure_awake()
        pre_sel = self.frame_now()["sel"] if where == "shelf" else None
        # A suspend command of its own (virtual device: `sleep 4`) suspends nothing: the counter would never move.
        ok0 = None if self.a.suspend_cmd else read_int(SUSPEND_OK)
        log_at = self.log_size()
        t_ask = send("power")
        self.wait_log(log_at, lambda l: l.startswith("sleep: suspending"), 30, "pocket-ui to suspend")
        # Back: the kernel's counter of suspends (any build); without one (virtual device) pocket-ui's own line.
        end = time.monotonic() + self.a.asleep + 60
        back = None
        tail = LogTail(self.a.ui_log, log_at)
        self.trace.poll()
        i = len(self.trace.events)
        while back is None and time.monotonic() < end:
            if ok0 is not None:
                v = read_int(SUSPEND_OK)
                if v is not None and v > ok0:
                    back = now_us()
            else:
                self.trace.poll()
                new, i = self.trace.events[i:], len(self.trace.events)
                st = [e for e in new if e["t"] >= t_ask and e["event"] == "step" and e["detail"] == "sleep: resumed"]
                if st:
                    back = st[0]["t"]
                elif any(l.startswith("sleep: woke after") for l in tail.new_lines()):
                    back = now_us()
            time.sleep(0.005)
        if back is None:
            raise Fail("the system did not come back from the suspend")
        ev = self.watch(back, where, log_at, pre_sel)
        for k, v in wake_metrics(where, back, ev).items():
            self.add(k, v)
        log("  deep, " + where + ": " + "  ".join(f"{k}={(v - back) / 1000:.0f}" for k, v in sorted(ev.items(), key=lambda kv: kv[1])))
        time.sleep(1.0)

    def light_cycle(self, where):
        if self.light_after is None:
            log("  light sleep: 'Screen off after' is Never on this device, not measured")
            return
        self.ensure_awake()
        log_at = self.log_size()
        wait = self.light_after * self.minute + 15
        self.wait_log(log_at, lambda l: l.startswith("sleep: light after"), wait, "light sleep")
        time.sleep(1.0)
        log_at = self.log_size()
        t = send("a")
        ev = self.watch(t, where, log_at, None, light=True, window=5)
        for k, v in wake_metrics(where, t, ev, light=True).items():
            self.add(k, v)
        log("  light, " + where + ": " + "  ".join(f"{k}={(v - t) / 1000:.0f}" for k, v in sorted(ev.items(), key=lambda kv: kv[1])))
        time.sleep(1.0)

    def go(self):
        try:
            self.ui.start()
            self.to_home()
            self.settle(2.0)
            for _ in range(self.a.wakes):
                self.deep_cycle("shelf")
            for _ in range(self.a.wakes):
                self.light_cycle("shelf")
            if self.a.systems:
                self.ensure_awake()
                self.launch(self.a.systems[0])
                self.settle()
                for _ in range(self.a.wakes):
                    self.deep_cycle("game")
                for _ in range(self.a.wakes):
                    self.light_cycle("game")
                self.ensure_awake()
                self.pause_menu()
                self.quit_game()
        finally:
            self.ui.stop()
        return {k: statistics.median(v) for k, v in self.samples.items()}


def drop_caches():
    """Writes the dirty pages out and empties the page cache, dentries and inodes (root only, Linux)."""
    os.sync()
    try:
        with open("/proc/sys/vm/drop_caches", "w") as f:
            f.write("3\n")
    except OSError as e:
        raise Fail(f"--cold needs root to write /proc/sys/vm/drop_caches ({e})")


def startup_steps(path, offset=0):
    """The steps of the first 'first frame after' line of pocket-ui's log after `offset`: {"event_loop": 45.4, ...}."""
    try:
        with open(path, errors="replace") as f:
            f.seek(offset)
            text = f.read()
    except OSError:
        return None
    for line in text.splitlines():
        if line.startswith("first frame after ") and "(" in line and line.rstrip().endswith("ms)"):
            inner = line[line.index("(") + 1 : line.rstrip().rindex("ms)")]
            steps = {}
            for part in inner.split(","):
                name, _, ms = part.strip().rpartition(" ")
                try:
                    steps[name.replace(" ", "_").replace("+", "_")] = float(ms)
                except ValueError:
                    pass
            return steps
    return None


def summarize(runs, samples):
    """Per metric: the median over the runs of each run's value."""
    metrics = {}
    names = sorted({k for r in runs for k in r})
    for k in names:
        vals = [r[k] for r in runs if k in r]
        metrics[k] = {
            "value": round(statistics.median(vals), 3),
            "unit": unit_of(k),
            "runs": [round(v, 3) for v in vals],
            "spread": round(max(vals) - min(vals), 3),
        }
        if k in samples:
            metrics[k]["samples"] = samples[k]
    return metrics


def with_unmeasured(metrics, unmeasured):
    """Adds the numbers that had no source as n/a (value None and why), unless some run did measure them."""
    for k, why in unmeasured.items():
        metrics.setdefault(k, {"value": None, "unit": unit_of(k), "runs": [], "spread": None, "unmeasured": why})
    return metrics


def _terminate(signum, _frame):
    # SIGTERM and SIGHUP (the SSH connection went away) would end Python without running any `finally`: turn them into
    # an exit that does, so Ui.stop ends the pocket-ui this run started.
    raise SystemExit(128 + signum)


def cmd_measure(a):
    for sig in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, _terminate)
    try:
        return _measure(a)
    finally:
        Ui.stop_all()


def _measure(a):
    if pids_of("pocket-ui"):
        log("pocket-ui is running: stop it first (scripts/latency.sh does)")
        return 2
    a.systems = [s for s in a.systems.split(",") if s] if a.systems else []
    trace = Trace(a.trace)
    random.seed(a.seed)
    runs, samples, unmeasured, cpu_sources = [], {}, {}, set()
    started = time.time()
    if a.cold or a.wake:
        a.warmup = False
    if a.wake:
        if a.suspend_cmd is None and not a.mock_suspend and not (os.path.exists(RTC_ALARM) and os.geteuid() == 0):
            log(f"--wake needs root and {RTC_ALARM} to wake the device by itself (or --suspend-cmd; docs/LATENCY.md#waking)")
            return 2
        a.runs = 1
    if a.warmup:
        log("warm-up (not counted)")
        Run(a, trace, warm=True).go()
    for i in range(a.runs):
        log(f"run {i + 1}/{a.runs}")
        r = WakeRun(a, trace) if a.wake else Run(a, trace)
        runs.append(r.go())
        for k, v in r.samples.items():
            samples.setdefault(k, []).extend(v)
        unmeasured.update(r.unmeasured)
        cpu_sources |= r.cpu_sources
        log("  " + "  ".join(f"{k} {v:.1f}" for k, v in sorted(runs[-1].items())))
    meta = dict(m.split("=", 1) for m in a.meta)
    meta.update({"date": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "seconds": round(time.time() - started)})
    try:
        meta["kernel"] = os.uname().release
    except AttributeError:
        pass
    config = {k: getattr(a, k) for k in ("runs", "starts", "moves", "opens", "cycles", "systems", "gap", "settle", "cpu_secs", "warmup", "cold", "wake", "wakes", "asleep")}
    if cpu_sources:
        meta["cpu_source"] = ",".join(sorted(cpu_sources))
    data = {"schema": SCHEMA, "meta": meta, "config": config, "metrics": with_unmeasured(summarize(runs, {}), unmeasured)}
    with open(a.out, "w") as f:
        json.dump(data, f, indent=1, sort_keys=True)
        f.write("\n")
    log(f"wrote {a.out}")
    return 0


# --- host side --------------------------------------------------------------------------------------------


def load(path):
    with open(path) as f:
        d = json.load(f)
    if d.get("schema") != SCHEMA:
        raise SystemExit(f"{path}: unknown schema {d.get('schema')}")
    return d


def fmt(v, unit):
    if v is None:
        return "n/a"
    return f"{v:.2f} %" if unit == "%" else f"{v:.1f} ms"


def order(names):
    keys = list(METRICS)

    def rank(n):
        k = n if n in METRICS else n.split(".")[0] + ".*"
        return (keys.index(k) if k in keys else len(keys), n)

    return sorted(names, key=rank)


def compare(cur, base, threshold, min_ms, min_cpu):
    """Rows (name, cur, base, change %, verdict) and whether anything regressed. A number the run tried to measure but
    could not (`unmeasured`: a CPU number without a source) fails too: n/a is not a pass."""
    rows, bad = [], False
    names = order(set(cur["metrics"]) | set(base["metrics"]) if base else set(cur["metrics"]))
    for n in names:
        c = cur["metrics"].get(n)
        b = base["metrics"].get(n) if base else None
        cv = c["value"] if c else None
        bv = b["value"] if b else None
        unit = (c or b)["unit"]
        verdict, change = "", None
        first_frame = (cur["metrics"].get("start.process") or {}).get("value")
        if c and c.get("unmeasured"):
            verdict, bad = "NOT MEASURED", True
            log(f"{n}: n/a, not measured ({c['unmeasured']})")
        elif n == "start.pads" and cv is not None and first_frame is not None and cv >= first_frame:
            # Against any baseline: a press between the first frame and the pads' open is lost (docs/LATENCY.md, First
            # press after a start). 2026-10-26: 220 ms against a first frame at 60 ms, unnoticed, as no baseline had it.
            verdict, bad = "AFTER FRAME", True
            log(f"{n}: the pads open {cv:.1f} ms after main(), after the first frame ({first_frame:.1f} ms): a press in between is lost")
        elif cv is not None and bv is not None:
            change = (cv - bv) / bv * 100.0 if bv > 0 else (0.0 if cv == 0 else float("inf"))
            floor = min_cpu if unit == "%" else min_ms
            if cv > bv * (1 + threshold / 100.0) and cv - bv > floor:
                verdict, bad = "SLOWER", True
            elif cv < bv * (1 - threshold / 100.0) and bv - cv > floor:
                verdict = "faster"
            else:
                verdict = "ok"
            if unit == "%" and bv == 0 and not base.get("meta", {}).get("cpu_source"):
                log(f"{n}: the baseline says 0.00 % without a CPU source: likely no measurement (latency.py before "
                    "2026-10-26 on a kernel without schedstat); save a new baseline")
        elif base is not None:
            verdict = "new" if bv is None else "missing"
        rows.append((n, cv, bv, change, verdict, unit))
    return rows, bad


def print_table(rows, with_base):
    w = max([len(r[0]) for r in rows] + [6])
    if with_base:
        print(f"{'metric':<{w}}  {'now':>10}  {'baseline':>10}  {'change':>8}  verdict")
    else:
        print(f"{'metric':<{w}}  {'value':>10}  meaning")
    for n, cv, bv, change, verdict, unit in rows:
        if with_base:
            ch = "-" if change is None else ("inf" if change == float("inf") else f"{change:+.1f}%")
            print(f"{n:<{w}}  {fmt(cv, unit):>10}  {fmt(bv, unit):>10}  {ch:>8}  {verdict}")
        else:
            print(f"{n:<{w}}  {fmt(cv, unit):>10}  {describe(n)}")


def cmd_table(a):
    d = load(a.file)
    rows, _ = compare(d, None, 0, 0, 0)
    m = d.get("meta", {})
    print(f"# {m.get('target', '?')} {m.get('commit', '')} {m.get('date', '')}, {d['config'].get('runs')} runs".rstrip())
    print_table(rows, False)
    return 0


def cmd_compare(a):
    cur, base = load(a.file), load(a.baseline)
    if cur["meta"].get("target") and base["meta"].get("target") and cur["meta"]["target"] != base["meta"]["target"]:
        log(f"warning: comparing {cur['meta']['target']} against a {base['meta']['target']} baseline")
    rows, bad = compare(cur, base, a.threshold, a.min_ms, a.min_cpu)
    print(f"# {a.file} against {a.baseline}: slower than +{a.threshold:g} % (and more than {a.min_ms:g} ms / {a.min_cpu:g} CPU points) fails")
    print_table(rows, True)
    if bad:
        print("RESULT: slower than the baseline")
        return 1
    print("RESULT: ok")
    return 0


def cmd_merge(a):
    ds = [load(p) for p in a.files]
    runs_by = {}
    for d in ds:
        for k, m in d["metrics"].items():
            runs_by.setdefault(k, []).extend(m["runs"])
    out = dict(ds[0])
    out["config"] = dict(out["config"], runs=sum(d["config"].get("runs", 0) for d in ds))
    out["meta"] = dict(out["meta"], merged=len(ds))
    out["metrics"] = {}
    unmeasured = {k: m["unmeasured"] for d in ds for k, m in d["metrics"].items() if m.get("unmeasured")}
    for k, vals in runs_by.items():
        if vals:
            out["metrics"][k] = {"value": round(statistics.median(vals), 3), "unit": unit_of(k), "runs": vals, "spread": round(max(vals) - min(vals), 3)}
    with_unmeasured(out["metrics"], unmeasured)
    with open(a.out, "w") as f:
        json.dump(out, f, indent=1, sort_keys=True)
        f.write("\n")
    return 0


# --- phases: where the time went --------------------------------------------------------------------------

# The moments `phases` takes apart: name, what starts one, what ends it. Starts and ends are tested on trace events.
def _is_input(button):
    return lambda e: e["event"] == "input" and e.get("button") == button and e.get("down")


def _frame_on(*screens):
    return lambda e: e["event"] == "frame" and e.get("screen") in screens


MOMENTS = [
    # Quit in the pause menu: from pocket-ui's step to the first shelf frame after the game ended.
    ("quit.to_shelf", lambda e: e["event"] == "step" and e["detail"] == "pause menu: quit", _frame_on(*SHELF), "game-exited"),
    # HOME (its press) in a game to the first pause menu frame.
    ("open.pause_menu", _is_input("Mode"), _frame_on("menu"), None),
    # SELECT (its press) to the first switcher frame, on the shelf or in the pause menu.
    ("open.switcher", _is_input("Select"), _frame_on("switcher"), None),
]
# Steps after a moment's end still shown (the pause menu's picture comes after its first frame), in ms.
AFTER_MS = 300


def phases(events):
    """Per moment: a list of samples, each a list of (offset ms, label) from the start."""
    out = {name: [] for name, *_ in MOMENTS}
    # Each frame's size, from the paint line right after it; frames smaller than the largest don't end a moment
    # (drawn before sway's resize came in).
    size_of, full = {}, {}
    for k, e in enumerate(events):
        if e["event"] == "paint" and e.get("size") and k > 0 and events[k - 1]["event"] == "frame" and events[k - 1]["pid"] == e["pid"]:
            size_of[id(events[k - 1])] = e["size"]
            if e["size"][0] * e["size"][1] > full.get(e["pid"], (0, 0))[0] * full.get(e["pid"], (0, 0))[1]:
                full[e["pid"]] = e["size"]

    def small(f):
        sz, fu = size_of.get(id(f)), full.get(f["pid"])
        return bool(sz and fu and sz[0] * sz[1] < fu[0] * fu[1])

    for name, starts, ends, needs in MOMENTS:
        i = 0
        while i < len(events):
            e = events[i]
            i += 1
            if e["prog"] != "pocket-ui" or not starts(e):
                continue
            t0 = e["t"]
            # The end: the first pocket-ui event that ends it (after `needs`, if given), before the next start.
            seen_needed = needs is None
            end = None
            for f in events[i:]:
                if f["prog"] == "pocket-ui" and starts(f):
                    break
                if f["t"] - t0 > 10_000_000:
                    break
                if needs and f["prog"] == "pocket-ui" and f["event"] == needs:
                    seen_needed = True
                if seen_needed and f["prog"] == "pocket-ui" and ends(f) and not small(f):
                    end = f
                    break
            if end is None:
                continue
            steps = []
            painted = None
            for f in events[i:]:
                if f["t"] > end["t"] + AFTER_MS * 1000:
                    break
                if f["prog"] == "pocket-ui" and f["event"] == "input" and f["t"] > end["t"]:
                    break
                after = "+" if f["t"] > end["t"] else ""
                ms = (f["t"] - t0) / 1000.0
                if f["event"] == "step":
                    steps.append((ms, f"{after}{f['prog']}: {f['detail']}"))
                elif f["event"] in ("game-exited", "launch") and f["prog"] == "pocket-ui":
                    steps.append((ms, f"{after}pocket-ui: {f['event']}"))
                elif f is end:
                    steps.append((ms, f"pocket-ui: first {f['screen']} frame"))
                elif f["event"] == "paint" and f["prog"] == "pocket-ui" and painted is None and f["t"] >= end["t"]:
                    painted = f["detail"]
                elif f["event"] == "frame" and f["prog"] == "pocket-ui" and f["t"] > end["t"]:
                    steps.append((ms, f"+pocket-ui: next {f['screen']} frame"))
            if painted:
                ms_, _, size = painted.partition(" ")
                steps.append((-1.0, f"(first frame painted in {ms_} ms at {size})"))
            out[name].append(steps)
    return out


def print_phases(out):
    for name, samples in out.items():
        if not samples:
            continue
        print(f"# {name}: {len(samples)} times, ms from the button (median, min-max); + = after the first frame")
        by = {}
        order = []
        notes = []
        for steps in samples:
            seen = set()
            for ms, label in steps:
                if ms < 0:
                    notes.append(label)
                    continue
                if label in seen:
                    continue
                seen.add(label)
                if label not in by:
                    by[label] = []
                    order.append(label)
                by[label].append(ms)
        rows = sorted(order, key=lambda k: statistics.median(by[k]))
        w = max(len(r) for r in rows)
        for r in rows:
            v = by[r]
            print(f"  {r:<{w}}  {statistics.median(v):8.1f}  ({min(v):.1f}-{max(v):.1f}, {len(v)}x)")
        if notes:
            sizes = sorted(set(n.split(" at ")[-1] for n in notes))
            paint = [float(n.split(" in ")[1].split(" ms")[0]) for n in notes]
            print(f"  first frame painted in {statistics.median(paint):.1f} ms (median), window {', '.join(sizes)}")
        print()


def cmd_phases(a):
    tr = Trace(a.trace, fresh=False)
    tr.poll()
    out = phases(tr.events)
    if not any(out.values()):
        log(f"no Quit, pause menu or switcher in {a.trace}")
        return 3
    print_phases(out)
    return 0


def selftest_ui_stop():
    """Ui.stop and Ui.stop_all end the process the harness started, whether it ends by asking or by force."""
    global FIFO
    saved = FIFO
    FIFO = os.path.join(os.environ.get("TMPDIR", "/tmp"), f"latency-selftest-no-fifo-{os.getpid()}")  # none: quit cannot be sent
    try:
        class A:
            ui_log = os.devnull

        def started():
            ui = Ui(A(), None)
            ui.quit_timeout = 0.05
            ui.kill_cores = False  # never touch a real pocket-core in a test
            ui.proc = subprocess.Popen(["sleep", "30"])
            Ui.LIVE.append(ui)
            return ui

        ui = started()
        pid = ui.proc.pid
        ui.stop()
        assert ui.proc is None and ui not in Ui.LIVE
        try:
            os.kill(pid, 0)
            raise AssertionError("Ui.stop left its process running")
        except ProcessLookupError:
            pass
        # stop_all: every Ui still in LIVE, e.g. after an exception between start and stop.
        a, b = started(), started()
        procs = [a.proc, b.proc]
        Ui.stop_all()
        assert not Ui.LIVE and all(p.poll() is not None for p in procs)
        Ui.stop_all()  # nothing left: no error
    finally:
        FIFO = saved


def selftest_cpu(doc):
    """CPU from /proc/<pid>/stat, schedstat only as the fallback, and n/a (a failed comparison) instead of 0."""
    import tempfile

    tick = os.sysconf("SC_CLK_TCK")
    with tempfile.TemporaryDirectory() as proc:
        def put(pid, stat=None, schedstat=None):
            os.makedirs(f"{proc}/{pid}/task/{pid}", exist_ok=True)
            if stat is not None:
                with open(f"{proc}/{pid}/stat", "w") as f:
                    f.write(stat)
            for i, ns in enumerate(schedstat or []):
                os.makedirs(f"{proc}/{pid}/task/{pid + i}", exist_ok=True)
                with open(f"{proc}/{pid}/task/{pid + i}/schedstat", "w") as f:
                    f.write(f"{ns} 0 0\n")

        # A name with spaces and brackets; utime 300 and stime 50 ticks (fields 14 and 15).
        put(10, stat="10 (pocket ui (x)) S 1 10 10 0 -1 4194560 100 0 0 0 300 50 0 0 20 0 9 0 50 0 0\n", schedstat=[7_000_000_000])
        assert cpu_seconds(10, proc) == (350 / tick, "stat"), cpu_seconds(10, proc)
        # The Mini V2 before: no schedstat, no stat either here; schedstat alone (a kernel with it, stat unreadable).
        put(11, schedstat=[1_500_000_000, 500_000_000])
        assert cpu_seconds(11, proc) == (2.0, "schedstat")
        put(12)
        assert cpu_seconds(12, proc) == (None, "none"), "no source is not 0"
        assert cpu_seconds(13, proc) == (None, None), "gone"
        assert cpu_percent(12, 0.01, proc)[0] is None and "no CPU source" in cpu_percent(12, 0.01, proc)[1]
        try:
            cpu_percent(13, 0.01, proc)
            raise AssertionError("a process that is gone must fail")
        except Fail:
            pass
    # The real /proc: this process, after spending some CPU.
    if os.path.exists(f"/proc/{os.getpid()}/stat"):
        a, src = cpu_seconds(os.getpid())
        end = time.process_time() + 0.05
        while time.process_time() < end:
            pass
        b, _ = cpu_seconds(os.getpid())
        assert src == "stat" and b > a, (a, b, src)

    # n/a in the table and a failed comparison, whatever the baseline says.
    class A:
        warm = False
        unmeasured = {}
        cpu_sources = set()
        samples = {}
        add = Run.add

    r = A()
    Run.add_cpu(r, "cpu.shelf_idle", (None, "no CPU source for pid 12"))
    Run.add_cpu(r, "cpu.behind_game", (0.1, "stat"))
    assert r.unmeasured == {"cpu.shelf_idle": "no CPU source for pid 12"} and r.samples == {"cpu.behind_game": [0.1]} and r.cpu_sources == {"stat"}
    cur = doc({"nav.p50": 20.0, "cpu.behind_game": 0.1})
    cur["metrics"] = with_unmeasured(cur["metrics"], r.unmeasured)
    assert cur["metrics"]["cpu.shelf_idle"]["value"] is None and fmt(None, "%") == "n/a"
    rows, bad = compare(cur, doc({"nav.p50": 20.0, "cpu.shelf_idle": 0.0, "cpu.behind_game": 0.1}), 15, 3, 0.3)
    assert bad and {r[0]: r[4] for r in rows}["cpu.shelf_idle"] == "NOT MEASURED"
    _, bad = compare(cur, None, 15, 3, 0.3)
    assert bad, "also without a baseline"
    # A merge keeps it n/a unless another file measured it.
    with tempfile.TemporaryDirectory() as d:
        paths = []
        for i, metrics in enumerate([cur["metrics"], doc({"nav.p50": 22.0, "cpu.behind_game": 0.2})["metrics"]]):
            paths.append(os.path.join(d, f"{i}.json"))
            with open(paths[-1], "w") as f:
                json.dump({"schema": SCHEMA, "meta": {}, "config": {"runs": 1}, "metrics": metrics}, f)

        class M:
            files = paths
            out = os.path.join(d, "m.json")

        cmd_merge(M())
        merged = load(M.out)["metrics"]
        assert merged["cpu.shelf_idle"]["unmeasured"] and merged["cpu.behind_game"]["value"] == 0.15 and merged["nav.p50"]["value"] == 21.0


def cmd_selftest(_a):
    selftest_ui_stop()

    def doc(values):
        return {"schema": SCHEMA, "meta": {}, "config": {"runs": 1}, "metrics": {k: {"value": v, "unit": unit_of(k), "runs": [v], "spread": 0} for k, v in values.items()}}

    assert percentile([5, 1, 3, 2, 4], 50) == 3
    assert percentile(list(range(1, 101)), 95) == 95
    assert percentile([7], 95) == 7
    assert unit_of("launch.gba") == "ms" and unit_of("cpu.shelf_idle") == "%"
    base = doc({"nav.p50": 20.0, "launch.gbc": 400.0, "cpu.behind_game": 0.1, "open.settings": 4.0})
    # 14 % slower passes, 16 % slower fails.
    _, bad = compare(doc({"nav.p50": 22.8, "launch.gbc": 400.0, "cpu.behind_game": 0.1, "open.settings": 4.0}), base, 15, 1, 0.3)
    assert not bad
    rows, bad = compare(doc({"nav.p50": 23.3, "launch.gbc": 400.0, "cpu.behind_game": 0.1, "open.settings": 4.0}), base, 15, 1, 0.3)
    assert bad and [r[4] for r in rows if r[0] == "nav.p50"] == ["SLOWER"]
    # Tiny numbers: +50 % of 4 ms is 2 ms, under the 3 ms floor; CPU 0.1 -> 0.3 % is under 0.3 points.
    _, bad = compare(doc({"nav.p50": 20.0, "launch.gbc": 400.0, "cpu.behind_game": 0.3, "open.settings": 6.0}), base, 15, 3, 0.3)
    assert not bad
    _, bad = compare(doc({"nav.p50": 20.0, "launch.gbc": 400.0, "cpu.behind_game": 0.6, "open.settings": 4.0}), base, 15, 3, 0.3)
    assert bad
    # Faster is fine; a metric missing on one side is reported, not failed.
    rows, bad = compare(doc({"nav.p50": 10.0, "launch.gbc": 300.0, "launch.snes": 500.0}), base, 15, 3, 0.3)
    v = {r[0]: r[4] for r in rows}
    assert not bad and v["nav.p50"] == "faster" and v["launch.snes"] == "new" and v["open.settings"] == "missing"
    # The pads open after the first frame: fails whatever the baseline says (none had start.pads before 2026-10-26).
    rows, bad = compare(doc({"start.process": 60.0, "start.pads": 220.0}), doc({"start.process": 60.0}), 15, 3, 0.3)
    assert bad and {r[0]: r[4] for r in rows}["start.pads"] == "AFTER FRAME"
    _, bad = compare(doc({"start.process": 60.0, "start.pads": 220.0}), None, 15, 3, 0.3)
    assert bad, "also without a baseline"
    rows, bad = compare(doc({"start.process": 60.0, "start.pads": 12.0}), doc({"start.process": 60.0}), 15, 3, 0.3)
    assert not bad and {r[0]: r[4] for r in rows}["start.pads"] == "new"
    # summarize: median over runs.
    m = summarize([{"a.x": 1.0}, {"a.x": 9.0}, {"a.x": 2.0}], {})
    assert m["a.x"]["value"] == 2.0 and m["a.x"]["spread"] == 8.0
    # Trace lines, as src/trace.rs writes them.
    import tempfile

    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, "t.log")
        tr = Trace(p)
        with open(p, "a") as f:
            f.write("100 pocket-ui 7 input 3 Down down\n")
            f.write("250 pocket-ui 7 frame shelf in=3 sel=game:snes/Contra III - The Alien Wars.smc\n")
            f.write("300 pocket-core 9 core-frame first\n")
            f.write("garbage\n400 pocket-ui 7 fra")
        tr.poll()
        assert [e["event"] for e in tr.events] == ["input", "frame", "core-frame"]
        assert tr.events[0]["n"] == 3 and tr.events[0]["down"]
        assert tr.events[1]["in"] == 3 and tr.events[1]["sel"] == "game:snes/Contra III - The Alien Wars.smc"
        with open(p, "a") as f:
            f.write("me x=1 in=3 sel=-\n")
        tr.poll()
        assert tr.events[-1]["event"] == "frame" and tr.events[-1]["t"] == 400 and tr.events[-1]["sel"] == "-"
        assert tr.wait(lambda e: e["event"] == "core-frame", 0, 1, "x")["pid"] == 9
    # phases: a Quit, taken apart from its steps; the end is the first shelf frame after game-exited.
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, "t.log")
        with open(p, "w") as f:
            f.write("1000000 pocket-ui 7 step pause menu: quit\n")
            f.write("1002000 pocket-core 9 step quit received\n")
            f.write("1003000 pocket-ui 7 frame menu in=5 sel=-\n")
            f.write("1020000 pocket-core 9 step exit save done\n")
            f.write("1021000 pocket-ui 7 game-exited gba /x.gba\n")
            f.write("1030000 pocket-ui 7 frame shelf in=5 sel=-\n")
            f.write("1030001 pocket-ui 7 paint 8.50 1240x1080\n")
            f.write("1050000 pocket-core 9 step torn down\n")
        tr = Trace(p, fresh=False)
        tr.poll()
        q = phases(tr.events)["quit.to_shelf"]
        assert len(q) == 1, q
        labels = dict((l, ms) for ms, l in q[0])
        assert labels["pocket-core: exit save done"] == 20.0 and labels["pocket-ui: first shelf frame"] == 30.0, labels
        assert labels["+pocket-core: torn down"] == 50.0, labels
        assert "(first frame painted in 8.50 ms at 1240x1080)" in labels
    # --cold: the steps of pocket-ui's start line, after an offset into its log.
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, "ui.log")
        with open(p, "w") as f:
            f.write("first frame after 9.0ms (logs 1.0 ms)\n")
            at = f.tell()
            f.write("library: 23 systems, 332 games, read in 3.1ms (from the cache)\n")
            f.write("first frame after 318.6ms (logs 0.6, event loop 45.4, settings+hw 3.6, library 205.3, drawn 24.1 ms)\n")
        assert startup_steps(p, at) == {"logs": 0.6, "event_loop": 45.4, "settings_hw": 3.6, "library": 205.3, "drawn": 24.1}, startup_steps(p, at)
        assert startup_steps(p, os.path.getsize(p)) is None
        assert unit_of("step.event_loop") == "ms"
    # --wake: the numbers of one wake, in ms after the system was back; what wasn't seen (an older build) is left out.
    ev = {"screen_on": 1_004_000, "game_on": 1_002_000, "input": 1_090_000, "frame": 1_020_000, "sound": 1_060_000, "hooks": 1_900_000, "log_woke": 900_000}
    m = wake_metrics("game", 1_000_000, ev)
    assert m == {"wake.game.screen_on": 4.0, "wake.game.game_on": 2.0, "wake.game.input": 90.0, "wake.game.frame": 20.0, "wake.game.sound": 60.0}, m
    assert wake_metrics("shelf", 5_000, {"screen_on": 25_000, "frame": 30_000}, light=True) == {"light.shelf.screen_on": 20.0, "light.shelf.frame": 25.0}
    assert unit_of("wake.shelf.screen_on") == "ms" and unit_of("light.game.frame") == "ms"
    selftest_cpu(doc)
    print("latency.py selftest: ok")
    return 0


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    m = sub.add_parser("measure", help="on the target: measure and write JSON")
    m.add_argument("--out", required=True)
    m.add_argument("--runs", type=int, default=3, help="passes over everything; each number is the median over them")
    m.add_argument("--starts", type=int, default=5, help="cold starts per run")
    m.add_argument("--moves", type=int, default=100, help="shelf moves per run")
    m.add_argument("--opens", type=int, default=5, help="Settings and switcher opens per run")
    m.add_argument("--cycles", type=int, default=2, help="game cycles per run (launch, pause menu, quit, switch)")
    m.add_argument("--systems", default="gbc,gba,snes", help="games to start: system[:part of the file name], comma separated; empty: no games")
    m.add_argument("--gap", type=float, default=150, help="ms between shelf moves")
    m.add_argument("--settle", type=float, default=2.0, help="s to let a game run before the next step")
    m.add_argument("--cpu-secs", dest="cpu_secs", type=float, default=10)
    m.add_argument("--no-warmup", dest="warmup", action="store_false", help="skip the uncounted first pass")
    m.add_argument("--cold", action="store_true", help="only starts, each after drop_caches (root): start.nocache and the steps of pocket-ui's start (docs/LATENCY.md, Cold start)")
    m.add_argument("--wake", action="store_true", help="only deep and light sleep and the wake from them, on the shelf and in the first --systems game (root; suspends the device with an RTC alarm; docs/LATENCY.md#waking)")
    m.add_argument("--wakes", type=int, default=3, help="--wake: wakes of each kind on the shelf and in the game")
    m.add_argument("--asleep", type=int, default=15, help="--wake: seconds the device sleeps before the RTC alarm wakes it")
    m.add_argument("--suspend-cmd", dest="suspend_cmd", default=None, help="--wake: the suspend instead of the RTC alarm and systemctl suspend (virtual device: 'sleep 4')")
    m.add_argument("--mock-suspend", dest="mock_suspend", type=int, default=0, help="--wake: no real suspend, a command that blocks this many seconds (the virtual device; scripts/latency.sh --wake passes 4 there)")
    m.add_argument("--wake-window", dest="wake_window", type=float, default=8.0, help="--wake: seconds after the wake to wait for everything to be back")
    m.add_argument("--minute-secs", dest="minute_secs", type=int, default=4, help="--wake: a sleep 'minute' in seconds (POCKETUI_SLEEP_MINUTE_SECS), so light sleep comes after seconds")
    m.add_argument("--wake-button", dest="wake_button", default="Gamepad:Button:DPadDown", help="--wake: the InputPlumber button pressed after the wake for 'input'")
    m.add_argument("--no-inject", dest="no_inject", action="store_true", help="--wake: press no button through InputPlumber (no 'input' numbers)")
    m.add_argument("--seed", type=int, default=1)
    m.add_argument("--meta", action="append", default=[], help="KEY=VALUE stored with the result")
    m.add_argument("--root", default="/storage/pocket-ui")
    m.add_argument("--trace", default="/tmp/pocket-trace.log")
    m.add_argument("--ui-log", dest="ui_log", default="/tmp/latency-pocket-ui.log")
    t = sub.add_parser("table")
    t.add_argument("file")
    c = sub.add_parser("compare")
    c.add_argument("file")
    c.add_argument("baseline")
    c.add_argument("--threshold", type=float, default=15.0, help="percent slower that fails (default 15)")
    c.add_argument("--min-ms", dest="min_ms", type=float, default=3.0, help="and at least this many ms slower (default 3)")
    c.add_argument("--min-cpu", dest="min_cpu", type=float, default=0.3, help="and at least this many CPU percentage points (default 0.3)")
    g = sub.add_parser("merge")
    g.add_argument("out")
    g.add_argument("files", nargs="+")
    ph = sub.add_parser("phases", help="where the time of each Quit, pause menu and switcher went (from a trace)")
    ph.add_argument("trace", nargs="?", default="/tmp/pocket-trace.log")
    sub.add_parser("selftest")
    a = p.parse_args()
    try:
        return {"measure": cmd_measure, "table": cmd_table, "compare": cmd_compare, "merge": cmd_merge, "phases": cmd_phases, "selftest": cmd_selftest}[a.cmd](a)
    except Fail as e:
        log(f"failed: {e}")
        return 3


if __name__ == "__main__":
    sys.exit(main())
