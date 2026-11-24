#!/usr/bin/env python3
"""Kern-Abnahme C before the devices: the moments that make pocket-ui, walked on each device's real drawing path, as a
picture series with three numbers per moment, against the RG35XX Plus and a reference (the release checklist's "Kern-Abnahme", part C).

    python3 -I scripts/kern-walk.py --bin DIR --out DIR [--devices brick,plus,miniv2] [--ref DIR] [--make-ref]
                                    [--core FILE] [--label TEXT]
    python3 -I scripts/kern-walk.py --self-test

--bin holds pocket-ui, pocket-core and pocket-fb of one x86_64 Linux build with the `brick` feature (the merge gate's
own build folder is one, no build needed). --core takes another pocket-core (the red-before proof).

Why this way (20.11.2026): the Brick shipped without its TV switch-on after inserting a game (pocket-core's software
picture never marked its first frame shown) and with a Cabinet too small; ~7000 tests stayed green because none looked at
the finished frames on the Brick's own way of drawing. Here every device runs its real programs on its real path:
  brick   1024x768, pocket-core's software picture (POCKET_CORE_SOFTWARE=1, the CPU copy)
  plus    640x480, pocket-core on GLES (Mesa's software GL here, the H700's GPU on the device)
  miniv2  1240x1080, as plus
all inside pocket-fb (the Brick's own small compositor) on a plain file as the screen. The file is read 60 times a second,
as `fbrec` reads the Brick's fb0: what is in it is what the panel would show. pocket-ui and pocket-core are unchanged,
started as on a device (no test driver, which paints only on request), with a made-up card (two SNES games with covers of
our own) and scripts/crash-sweep/stub-core.c under every core name (a picture that moves, sound, saves). Each walk has its
own /tmp (bwrap), the FIFO is its /tmp/pocket-ui.in.

The walk (about a minute per device): the Cabinet with its TV (Schrank), the case turning in (Huelle), A: insert, the
console on, the TV switching on, the game 5 s, Save & quit (eject), Continue; then each other look once: shelf, insert,
TV on. Three numbers per moment instead of equal pixels (pixel equality is too strict for a pre-check):
  motion   distinct pictures in the moment (a missing animation gives 0-1); for the TV: pictures of the switch-on, those
           far from the settled game picture in its first second (0 without a switch-on: the 20.11. Brick bug)
  size     the share of the screen the moment's figure takes: the box of what moves (insert, eject), the lit share of a
           still, the game picture's box; Brick against the Plus within 5 points (both 4:3) and every device against its
           reference within 5 points
  cover    our cover's own magenta is on the case (not the tape-label stand-in)
plus "not empty" for every picture. Exit 0 = all green, 1 = a red line (report.md, page/index.html), 2 = the harness.

Output: OUT/page/index.html (the contact sheets: per moment beginning, middle and end, reference left, now right),
OUT/report.md, OUT/numbers.json, OUT/<device>/ (logs). --make-ref writes this run as a reference folder (OUT/ref/) for
the maintainer to look at and keep as --ref once the owner says it is right.
"""
import argparse
import html
import json
import os
import re
import shutil
import signal
import socket
import struct
import subprocess
import sys
import threading
import time
import zlib

try:
    import numpy as np
except ImportError:  # the analysis needs it; say so instead of a traceback
    np = None

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEVICES = {
    "brick": {"size": (1024, 768), "env": {"POCKETUI_DEVICE": "brick", "POCKET_CORE_SOFTWARE": "1"},
              "path": "Brick: software picture (POCKET_CORE_SOFTWARE), 1024x768"},
    "plus": {"size": (640, 480), "env": {"POCKETUI_DEVICE": "rg35xx-plus"},
             "path": "RG35XX Plus: pocket-core on GLES, 640x480"},
    "miniv2": {"size": (1240, 1080), "env": {"POCKETUI_DEVICE": "miniv2"},
               "path": "Mini V2 (ROCKNIX): pocket-core on GLES, 1240x1080"},
    # --android: the Handheld view of the APK with its Java player, on a device or emulator (screenrecord).
    "android": {"size": (1240, 1080), "env": {}, "path": "Android Handheld view with the APK's player (screenrecord), 1240x1080"},
}
GAMES = ["Pocket Quest (USA)", "Pocket Rally (USA)"]
# Our own cover: magenta and cyan, colours no shelf, case or stand-in label has.
MAGENTA = (255, 0, 255)
LOOKS = [("shelf", "Regal"), ("big", "Easy"), ("split", "Split")]
# Moments in sheet order: key, German name for the lead's page, what is checked.
MOMENTS = [
    ("schrank", "Schrank", "still"),
    ("huelle", "Hülle", "case"),
    ("einlegen", "Einlegen", "anim"),
    ("konsole", "Konsole an", "anim"),
    ("tv", "TV an", "tv"),
    ("spiel", "Spiel läuft (5 s)", "game"),
    ("auswerfen", "Auswerfen", "anim"),
    ("continue", "Continue", "tv"),
] + [m for look, name in LOOKS for m in (
    ((f"regal-{look}", f"Look {name}: Regal", "still"), (f"einlegen-{look}", f"Look {name}: Einlegen", "anim"),
     (f"tv-{look}", f"Look {name}: TV an", "tv")) if look != "split" else
    # Split has no launch scene, so no insert and no switch-on (by design): the game must come, with a picture.
    ((f"regal-{look}", f"Look {name}: Regal", "still"), (f"einlegen-{look}", f"Look {name}: Start (ohne Szene, wie gebaut)", "start")))]
# The least a moment must show (the build computer's software GL draws fewer pictures a second than a device; these are far below
# what a working moment gives and far above a missing one).
MIN_MOTION = {"anim": 8, "case": 3, "game": 30}
MIN_TV = 4          # pictures of the switch-on in its first second (a working one gives 10-30, a missing one 0)
TV_FAR = 18.0       # mean difference (0-255) from the settled game picture that makes a picture part of the switch-on
SHARE_POINTS = 5.0  # size: Brick vs Plus and now vs reference, in points of the screen
COVER_RATIO = (0.7, 1.45)


# --- pictures --------------------------------------------------------------------------------------------------------
def lum(f):
    return f.max(axis=2)


def lit_share(f):
    """Share (0-100) of the screen that is not black."""
    return float((lum(f) > 40).mean() * 100)


def empty(f):
    """Nothing to see: almost all black, or one flat colour."""
    return lit_share(f) < 2.0 or float(f.astype(np.float32).std()) < 3.0


def diff(a, b):
    return float(np.abs(a.astype(np.int16) - b.astype(np.int16)).mean())


def distinct(frames, eps=0.15):
    """How many different pictures follow one another (a step smaller than eps counts as the same picture)."""
    n, last = 0, None
    for f in frames:
        if last is None or diff(f, last) > eps:
            n += 1
            last = f
    return n


def box_share(mask):
    """The bounding box of a mask as a share (0-100) of the picture, and the box (x, y, w, h)."""
    ys, xs = np.nonzero(mask)
    if len(xs) == 0:
        return 0.0, (0, 0, 0, 0)
    x0, x1, y0, y1 = int(xs.min()), int(xs.max()) + 1, int(ys.min()), int(ys.max()) + 1
    return (x1 - x0) * (y1 - y0) * 100.0 / mask.size, (x0, y0, x1 - x0, y1 - y0)


def motion_share(frames):
    """The box of everything that changes during a moment, as a share of the screen."""
    if len(frames) < 2:
        return 0.0
    base = frames[0].astype(np.int16)
    moved = np.zeros(frames[0].shape[:2], bool)
    for f in frames[1:]:
        moved |= np.abs(f.astype(np.int16) - base).max(axis=2) > 40
    return box_share(moved)[0]


def cover_share(f):
    """Our cover's magenta on the screen (the case, and the Cabinet's TV showing it): its share (0-100) of the screen,
    0 when it is not there. Shaded as the case is lit, so by hue: red and blue high, green far below both."""
    r, g, b = (f[:, :, i].astype(np.int16) for i in range(3))
    m = (r > 120) & (b > 120) & (g * 2 < np.minimum(r, b))
    return float(m.mean() * 100) if m.mean() > 0.0005 else 0.0


def tv_pictures(core_frames, settled):
    """Pictures of the switch-on: core pictures of the first second far from the settled game picture."""
    return sum(1 for f in core_frames if diff(f, settled) > TV_FAR)


# --- the screen as a file, read like fbrec -----------------------------------------------------------------------------
class Ipc:
    """sway's GET_TREE from pocket-fb: which window is on the screen (pocket-fb shows one at a time): "ui", "core" or ""."""

    def __init__(self, path):
        self.path, self.s = path, None

    def shown(self):
        try:
            if self.s is None:
                self.s = socket.socket(socket.AF_UNIX)
                self.s.settimeout(0.5)
                self.s.connect(self.path)
            self.s.sendall(b"i3-ipc" + struct.pack("<II", 0, 4))
            head = self._read(14)
            body = json.loads(self._read(struct.unpack("<I", head[6:10])[0]))
        except (OSError, ValueError, struct.error):
            self.s = None
            return ""
        todo = [body]
        while todo:
            n = todo.pop()
            if n.get("focused") and n.get("type") == "con":
                # pocket-core's window has no app_id: whatever is not pocket-ui is the game.
                return "ui" if n.get("app_id") == "pocket-ui" else "core"
            todo += n.get("nodes", []) + n.get("floating_nodes", [])
        return ""

    def _read(self, n):
        b = b""
        while len(b) < n:
            c = self.s.recv(n - len(b))
            if not c:
                raise OSError("closed")
            b += c
        return b


class Recorder(threading.Thread):
    """Reads the screen files about 60 times a second while a moment is open; keeps every new picture (half size above
    800 pixels wide) with its time and whose it is ("ui" or "core"). `sources`: (file, who) where who is a fixed name
    or a function asking the compositor which window is on that screen."""

    def __init__(self, sources, size):
        super().__init__(daemon=True)
        self.sources, (self.w, self.h) = sources, size
        self.fb = sources[0][0]
        self.step = 2 if self.w > 800 else 1
        self.lock = threading.Lock()
        self.frames = None
        self.stop_flag = False

    def read(self, path):
        raw = np.fromfile(path, dtype=np.uint8, count=self.w * self.h * 4)
        return raw.reshape(self.h, self.w, 4)[::self.step, ::self.step, 2::-1].copy()

    def run(self):
        last = [None] * len(self.sources)
        while not self.stop_flag:
            t0 = time.monotonic()
            with self.lock:
                on = self.frames is not None
            for i, (path, who) in enumerate(self.sources if on else []):
                try:
                    f = self.read(path)
                except (OSError, ValueError):
                    continue
                if last[i] is None or not np.array_equal(f, last[i]):
                    name = who() if callable(who) else who
                    with self.lock:
                        if self.frames is not None:
                            self.frames.append((t0, f, name))
                    last[i] = f
            if not on:
                last = [None] * len(self.sources)
            time.sleep(max(0.0, 1 / 60 - (time.monotonic() - t0)))

    def open(self):
        with self.lock:
            self.frames = []

    def close(self):
        with self.lock:
            got, self.frames = self.frames or [], None
        return got

    def now(self):
        """pocket-ui's screen as it is (for a still that may not change during its moment)."""
        return self.read(self.fb)


# --- the card -----------------------------------------------------------------------------------------------------------
def png(path, w, h, rows):
    raw = b"".join(b"\0" + bytes(r) for r in rows)
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    with open(path, "wb") as fh:
        fh.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                 + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def cover(path, seed):
    """Our own box art: a magenta field with cyan bands (no game's art), 300x420 like a case front."""
    w, h = 300, 420
    rows = []
    for y in range(h):
        row = bytearray()
        for x in range(w):
            band = ((x + y + seed * 37) // 30) % 3 == 0
            row += bytes((0, 220, 230) if band else MAGENTA)
        rows.append(row)
    png(path, w, h, rows)


def core_names():
    """Every core name pocket-ui may start for an SNES game (src/hostsys.rs), so the stub answers whichever it picks."""
    src = open(os.path.join(ROOT, "src/hostsys.rs")).read()
    names = set()
    for names_s, core in re.findall(r'system\(\s*&\[([^\]]*)\],\s*"([a-z0-9_]+)"', src):
        names.add(core)
    return sorted(names | {"snes9x", "snes9x2010", "bsnes", "mesen-s"})


def make_card(d, stub):
    for sub in ("data/covers/snes", "roms/snes", "states", "cores", "tmp"):
        os.makedirs(os.path.join(d, sub), exist_ok=True)
    for i, g in enumerate(GAMES):
        shutil.copy(stub["rom"], os.path.join(d, "roms/snes", g + ".sfc"))
        cover(os.path.join(d, "data/covers/snes", g + ".png"), i)
    for c in core_names():
        dst = os.path.join(d, "cores", f"{c}_libretro.so")
        if not os.path.lexists(dst):
            os.symlink(stub["core"], dst)
    with open(os.path.join(d, "es_systems.cfg"), "w") as fh:
        fh.write('<?xml version="1.0"?>\n<systemList>\n  <system><name>snes</name><fullname>Super Nintendo</fullname>'
                 f'<path>{d}/roms/snes</path><extension>.sfc .smc</extension><command>true</command></system>\n</systemList>\n')


def settings(d, look):
    s = {"onboarded": True, "seen_version": "9.9.9", "theme": look, "animations": "cozy", "motion": "full",
         "where_was_i": True, "extras": {"cabinet_tv": True, "tv_plays_game": False, "tv_switch_on": True,
                                         "sealed_new_games": False}}
    with open(os.path.join(d, "data", "settings.json"), "w") as fh:
        json.dump(s, fh)


# --- one walk on one device -----------------------------------------------------------------------------------------
class Session:
    """pocket-fb on a file and pocket-ui (with pocket-core) inside it, with their own /tmp."""

    def __init__(self, a, dev, d, look):
        self.a, self.dev, self.d, self.look = a, dev, d, look
        self.procs = []
        self.fb2 = None

    def __enter__(self):
        d, (w, h) = self.d, DEVICES[self.dev]["size"]
        settings(d, self.look)
        for f in ("tmp/pocket-ui.in", "tmp/pocket-ui.status", "frame.txt"):
            try:
                os.remove(os.path.join(d, f))
            except OSError:
                pass
        self.fb, self.wl, self.ipc_path = self.screen("")
        host = self.a.core
        gl = self.dev != "brick"
        if gl:
            # The GLES devices: pocket-core on a screen of its own. pocket-fb shows a window from its first picture,
            # and a GL window's first is the dark one before the game's (sway keeps pocket-ui in front then); on one
            # screen that would cut the insert short. pocket-ui does without seeing the game's window, as on the
            # crash sweep's two compositors. The Brick keeps both on one pocket-fb: that is how the Brick runs.
            self.fb2, wl2, ipc2 = self.screen("2")
            host = os.path.join(d, "core.sh")
            with open(host, "w") as fh:
                fh.write(f'#!/bin/sh\nexport WAYLAND_DISPLAY="{wl2}" SWAYSOCK="{ipc2}"\nexec "{self.a.core}" "$@"\n')
            os.chmod(host, 0o755)
        env = {k: v for k, v in os.environ.items() if k not in ("DISPLAY", "WAYLAND_DISPLAY", "SWAYSOCK")}
        env.update(WAYLAND_DISPLAY=self.wl, SWAYSOCK=self.ipc_path, XDG_RUNTIME_DIR=d, POCKETUI_MOCK="1", POCKETUI_LANG="en",
                   POCKETUI_TEXT_SIZE="normal", POCKETUI_DATA=os.path.join(d, "data"),
                   POCKETUI_ES_SYSTEMS=os.path.join(d, "es_systems.cfg"), POCKETUI_VISIBLE_TEXT=os.path.join(d, "frame.txt"),
                   POCKETUI_STATES=os.path.join(d, "states"), POCKETUI_CORES=os.path.join(d, "cores"),
                   POCKETUI_HOST_BIN=host, POCKETUI_NO_AUTOSUSPEND="1", POCKETUI_NO_HISTORY="1",
                   POCKETUI_SUSPEND_CMD="false", POCKETUI_POWEROFF_CMD="true", POCKETUI_REBOOT_CMD="true",
                   POCKET_CORE_ALSA_DEVICE="null",
                   POCKET_CORE_AUDIO="alsa", LIBGL_ALWAYS_SOFTWARE="1", RUST_BACKTRACE="1", TZ="UTC")
        env.update(DEVICES[self.dev]["env"])
        # Its own /tmp: pocket-ui and pocket-core meet in fixed files there (/tmp/pocket-ui.in, /tmp/pocket-core.cmd).
        self.uilog = open(os.path.join(d, f"ui-{self.look}.log"), "w")
        self.procs.append(subprocess.Popen(["bwrap", "--dev-bind", "/", "/", "--bind", os.path.join(d, "tmp"), "/tmp",
                                            "--die-with-parent", "nice", "-n", "19", self.a.ui], env=env, stdout=self.uilog,
                                           stderr=subprocess.STDOUT, start_new_session=True))
        self.ipc = Ipc(self.ipc_path)
        self.rec = Recorder([(self.fb, "ui"), (self.fb2, "core")] if gl else [(self.fb, self.ipc.shown)], (w, h))
        self.rec.start()
        self.wait(lambda: os.path.exists(os.path.join(d, "tmp/pocket-ui.in")) and lit_share(self.rec.now()) > 2, 30,
                  "pocket-ui's first picture")
        time.sleep(2.5)  # the shelf settles (fonts, covers, the case turning in)
        return self

    def screen(self, n):
        """A pocket-fb on a plain file as the screen: (file, Wayland socket, IPC socket)."""
        d, (w, h) = self.d, DEVICES[self.dev]["size"]
        fb, wl, ipc = os.path.join(d, "fb" + n), os.path.join(d, "w" + n), os.path.join(d, "i" + n)
        with open(fb, "wb") as fh:
            fh.write(bytes(w * h * 4))
        for p in (wl, ipc):
            if os.path.exists(p):
                os.remove(p)
        log = open(os.path.join(d, f"fb{n}-{self.look}.log"), "w")
        self.procs.append(subprocess.Popen(["nice", "-n", "19", self.a.fb, "--wayland", wl, "--ipc", ipc, "--fb", fb,
                                            "--mode", f"{w}x{h}x32"], stdout=log, stderr=subprocess.STDOUT,
                                           start_new_session=True))
        self.wait(lambda: os.path.exists(wl) and os.path.exists(ipc), 10, "pocket-fb's sockets")
        return fb, wl, ipc

    def __exit__(self, *_):
        self.rec.stop_flag = True
        for p in reversed(self.procs):
            try:
                os.killpg(p.pid, signal.SIGTERM)
                p.wait(8)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        self.uilog.close()

    def wait(self, cond, secs, what):
        t0 = time.monotonic()
        while time.monotonic() - t0 < secs:
            if cond():
                return
            if any(p.poll() is not None for p in self.procs):
                raise Harness(f"{what}: a program ended ({self.tail()})")
            time.sleep(0.05)
        raise Harness(f"no {what} within {secs} s ({self.tail()})")

    def tail(self):
        try:
            return " | ".join(open(self.uilog.name, errors="replace").read().strip().splitlines()[-3:])
        except OSError:
            return ""

    def send(self, *cmds, gap=0.35):
        for c in cmds:
            fd = os.open(os.path.join(self.d, "tmp/pocket-ui.in"), os.O_WRONLY | os.O_NONBLOCK)
            os.write(fd, (c + "\n").encode())
            os.close(fd)
            time.sleep(gap)

    def game(self):
        try:
            st = open(os.path.join(self.d, "tmp/pocket-ui.status")).read()
        except OSError:
            return ""
        m = re.search(r"(?:^|\t)game=([a-z]+)", st)
        return m.group(1) if m else ""

    def text(self):
        try:
            return [l.strip() for l in open(os.path.join(self.d, "frame.txt"), errors="replace")]
        except OSError:
            return []

    def record(self, secs, act=None, until=None):
        self.rec.open()
        if act:
            act()
        t0 = time.monotonic()
        while time.monotonic() - t0 < secs and not (until and until()):
            time.sleep(0.05)
        return self.rec.close()


class Harness(Exception):
    """The walk could not go on. `final`: never walk again (something else came to the front of a device: its owner
    may be using it)."""
    final = False


def to_game(s):
    """Rest on a game of the card: A starts it (a system's tile first: A opens it). Returns the launch's frames."""
    for _ in range(3):
        frames = s.record(9.0, lambda: s.send("a", gap=0), until=lambda: s.game() in ("playing",) and core_seen(s))
        if any(w == "core" and lit_any(f) for _, f, w in frames):
            more = s.record(1.6)  # the switch-on's first second and the picture settled after it
            return frames + more
        time.sleep(1.0)
    raise Harness(f"no game started (status game={s.game()!r}, screen: {' / '.join(s.text()[:8])})")


def core_seen(s):
    with s.rec.lock:
        return any(w == "core" and lit_any(f) for _, f, w in (s.rec.frames or [])[-3:])


def mid(tf):
    """The picture halfway through a moment by time ((time, picture) pairs): its own picture (an insert ends dark on
    purpose, the hand-over to the game)."""
    if not tf:
        return None
    half = (tf[0][0] + tf[-1][0]) / 2
    return min(tf, key=lambda p: abs(p[0] - half))[1]


def anim(tf):
    """The numbers of a moving moment: its distinct pictures, how much of the screen its middle lights, what moves."""
    fr = [f for _, f in tf]
    m = mid(tf)
    # The size: the median lit share over the moment, steady whichever frames a loaded machine happened to catch.
    lit = float(np.median([lit_share(f) for f in fr[::max(1, len(fr) // 24)]])) if fr else 0.0
    return dict(motion=distinct(fr), lit=lit, moved=motion_share(fr),
                empty=empty(m) if m is not None else True, secs=round(tf[-1][0] - tf[0][0], 2) if tf else 0.0)


def lit_any(f):
    """Anything lit at all: the switch-on's first dot counts, a black window before the game's picture does not."""
    return int((lum(f) > 40).sum()) > 8


def split_launch(frames):
    """A launch's frames: the UI's (insert, console on) before the game's first lit picture, and the game's
    (pocket-core's window) from it."""
    first = next((t for t, f, w in frames if w == "core" and lit_any(f)), None)
    ui = [(t, f) for t, f, w in frames if w != "core" and (first is None or t < first)]
    core = [(t, f) for t, f, w in frames if w == "core" and first is not None and t >= first]
    return ui, core


def walk(a, dev, out):
    m = {}
    try:
        return walk_linux(a, dev, out, m)
    except Harness as e:
        e.partial = m   # the moments walked before it stopped stay on the page
        raise


def walk_linux(a, dev, out, m):
    d = os.path.join(out, dev)
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(d)
    make_card(d, a.stub)

    def keep(key, tf, **numbers):
        """A moment's numbers and its first, middle and last picture ((time, picture) pairs or plain pictures)."""
        tf = [p if isinstance(p, tuple) else (i, p) for i, p in enumerate(tf)]
        m[key] = dict(numbers, frames=[tf[0][1], mid(tf), tf[-1][1]] if tf else [])

    with Session(a, dev, d, "cabinet") as s:
        still = s.rec.now()
        keep("schrank", [still], empty=empty(still), lit=lit_share(still))
        # To the other game and back: the case turns into its place, with our cover on it.
        fr = [(t, f) for t, f, _ in s.record(1.6, lambda: s.send("right", gap=0))]
        fr += [(t, f) for t, f, _ in s.record(1.6, lambda: s.send("left", gap=0))]
        end = fr[-1][1] if fr else s.rec.now()
        keep("huelle", fr or [end], motion=distinct([f for _, f in fr]), cover=cover_share(end), empty=empty(end))
        launch(s, m, keep, "", "einlegen", "konsole", "tv")
        tf = [(t, f) for t, f, w in s.record(5.0) if w == "core"]
        game = [f for _, f in tf]
        keep("spiel", tf, motion=distinct(game), picture=box_share(lum(game[-1]) > 40)[0] if game else 0.0,
             empty=any(empty(f) for f in game[::10]) if game else True)
        # Save & quit from the pause menu: the eject.
        s.send("home")
        s.wait(lambda: s.game() == "paused", 10, "the pause menu")
        time.sleep(0.8)
        rows = [r for r in ("Continue", "Save", "Load", "Earlier", "Options", "Save & quit", "Quit") if r in s.text()]
        quit_row = next((r for r in rows if r in ("Save & quit", "Quit")), None)
        if quit_row is None:
            raise Harness(f"no Save & quit in the pause menu: {s.text()[:12]}")
        s.send(*["down"] * rows.index(quit_row))
        ej = s.record(6.0, lambda: s.send("a", gap=0), until=lambda: s.game() in ("none", ""))
        ej += s.record(2.5)
        ui = [(t, f) for t, f, w in ej if w != "core"]
        keep("auswerfen", ui, **anim(ui))
        time.sleep(1.0)
        if s.fb2:
            # The game's own screen keeps its last picture after the game: dark again, as a panel would be.
            with open(s.fb2, "r+b") as fh:
                fh.write(bytes(os.path.getsize(s.fb2)))
        launch(s, m, keep, "", None, None, "continue")
    for look, _ in LOOKS:
        # A card of its own: a game played before would continue straight from its moment (no insert, by design).
        dl = os.path.join(d, look)
        os.makedirs(dl)
        make_card(dl, a.stub)
        with Session(a, dev, dl, look) as s:
            still = s.rec.now()
            keep(f"regal-{look}", [still], empty=empty(still), lit=lit_share(still))
            launch(s, m, keep, look, f"einlegen-{look}", None, f"tv-{look}")
    return m


def launch(s, m, keep, look, k_insert, k_console, k_tv):
    frames = to_game(s)
    ui, core = split_launch(frames)
    if k_insert and look == "split":
        # Split has no cases to watch: no launch scene and so no switch-on either (main.rs, "the same launch decision").
        # Its start is checked for the game coming, with a picture.
        keep(k_insert, ui + core[:1], kind="start", reached=bool(core), empty=empty(core[-1][1]) if core else True)
        return
    if k_insert:
        # The UI's part of the launch by time: the first 55 % the case going in, the rest the console coming on.
        cut = ui[0][0] + (ui[-1][0] - ui[0][0]) * (0.55 if k_console else 1.0) if ui else 0
        part = [p for p in ui if p[0] <= cut]
        keep(k_insert, part, **anim(part))
        if k_console:
            rest = [p for p in ui if p[0] > cut]
            keep(k_console, rest, **anim(rest))
    # The game's first second against its settled picture: the switch-on's pictures.
    t0 = core[0][0] if core else 0
    first = [f for t, f in core if t - t0 < 1.0]
    settled = [f for t, f in core if t - t0 >= 1.0]
    ref = settled[-1] if settled else (core[-1][1] if core else None)
    n = tv_pictures(first, ref) if ref is not None else 0
    keep(k_tv, first or ([ref] if ref is not None else []), tv=n, reached=bool(core),
         empty=empty(ref) if ref is not None else True)


# --- Android: the Handheld view (the Linux UI in the APK) with the APK's own Java player ------------------------------
# A QA copy of the APK beside the owner's app (its own package, provider and data; never the owner's app), its games in
# its own shared folder (Android/data/<package>/files: the player opens games by document id, so not in its private
# files; the owner's app reads only ROMs folders and its own kept ones), our covers and settings through run-as (the QA
# copy is debuggable). Its sound is off by appops for that package only (the owner's volume is not touched); the device
# needs 20 % battery; one session holds /tmp/rpmini-android.lock (fcntl, released with the process). At the end the QA
# package is uninstalled, which takes its data and games folder with it. Pictures by screenrecord, one per moment.
ANDROID_KEYS = {"a": "KEYCODE_BUTTON_A", "b": "KEYCODE_BUTTON_B", "home": "KEYCODE_BUTTON_MODE", "up": "KEYCODE_DPAD_UP",
                "down": "KEYCODE_DPAD_DOWN", "left": "KEYCODE_DPAD_LEFT", "right": "KEYCODE_DPAD_RIGHT"}
ANDROID_LOCK = "/tmp/rpmini-android.lock"
# Our games' system on Android: one the device's ROMs folders don't have (the QA copy reads them, the owner's games must
# never be the ones started), a TV console whose core the APK carries, with a test program of scripts/testroms.py.
ANDROID_SYSTEMS = [("megadrive", "md-test.md", ".md"), ("mastersystem", "sms-test.sms", ".sms")]


class Android:
    def __init__(self, a):
        self.adb = a.android.split()
        self.apk = a.android_apk
        self.pkg = a.android_pkg
        self.size = DEVICES["android"]["size"]
        self.games = f"/sdcard/Android/data/{self.pkg}/files/kern-roms"
        self.system = self.ext = None
        self.hidden = []
        self.tmp = "/data/local/tmp/kern-walk"
        self.lock = None
        self.lock_path = a.android_lock
        self.stop_watch, self.abort, self.running_before = True, None, set()

    def sh(self, cmd, check=True, timeout=60):
        r = subprocess.run(self.adb + ["shell", cmd], capture_output=True, text=True, timeout=timeout)
        if check and r.returncode != 0:
            raise Harness(f"adb shell {cmd[:80]}: {r.stderr.strip() or r.stdout.strip()}")
        return r.stdout

    def adb_run(self, *args, timeout=300):
        r = subprocess.run(self.adb + list(args), capture_output=True, text=True, timeout=timeout)
        if r.returncode != 0:
            raise Harness(f"adb {' '.join(args)[:80]}: {r.stderr.strip() or r.stdout.strip()}")
        return r.stdout

    def runas(self, cmd):
        return self.sh(f"run-as {self.pkg} sh -c '{cmd}'")

    def begin(self, work):
        import fcntl
        self.lock = open(self.lock_path, "w")
        try:
            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            raise Harness(f"{self.lock_path} is held by another session")
        level = re.search(r"level: (\d+)", self.sh("dumpsys battery"))
        if level and int(level.group(1)) < 20:
            raise Harness(f"battery {level.group(1)} % (needs 20 %)")
        # What runs before (an app the walk must never stop): the user's apps stay as they are.
        self.running_before = {l.split()[-1] for l in self.sh("ps -A -o NAME", check=False).split()[1:] if "." in l}
        if f"package:{self.pkg}\n" in self.sh("pm list packages") + "\n":
            raise Harness(f"{self.pkg} is installed already: not ours to replace (uninstall it by hand if it is a leftover)")
        self.adb_run("install", self.apk)
        self.sh(f"cmd appops set {self.pkg} PLAY_AUDIO ignore")   # silent before anything starts
        self.sh(f"appops set {self.pkg} MANAGE_EXTERNAL_STORAGE allow")
        # The device's own systems (every ROMs folder, any case): all hidden in the QA copy; ours is one they lack.
        found = self.sh("ls -d /sdcard/[Rr][Oo][Mm][Ss]/*/ /storage/*/[Rr][Oo][Mm][Ss]/*/ 2>/dev/null", check=False)
        self.hidden = sorted({l.rstrip("/").rsplit("/", 1)[-1].lower() for l in found.split() if l.strip()})
        pick = next((c for c in ANDROID_SYSTEMS if c[0] not in self.hidden), None)
        if pick is None:
            raise Harness(f"the device has every test system ({', '.join(c[0] for c in ANDROID_SYSTEMS)}): no system of our own")
        self.system, test_rom, self.ext = pick
        # Our card: a test program of scripts/testroms.py under two names, our covers, the walk's settings.
        rom = os.path.join(work, "game" + self.ext)
        subprocess.run([sys.executable, "-I", os.path.join(ROOT, "scripts/testroms.py"), os.path.join(work, "tr")], check=True, capture_output=True)
        shutil.copy(os.path.join(work, "tr", test_rom), rom)
        os.makedirs(os.path.join(work, "card"), exist_ok=True)
        for i, g in enumerate(GAMES):
            cover(os.path.join(work, "card", f"cover{i}.png"), i)
        # The games folder inside the package's own external folder, which Android makes (owned by the package) when the
        # view first starts; one made by the shell before that stays unreadable to the app ("no games").
        self.sh(f"am start -n {self.pkg}/xyz.pocketui.app.HandheldActivity --es updateUrl http://127.0.0.1:9/none.json")
        t0 = time.monotonic()
        while self.sh(f"ls -d /sdcard/Android/data/{self.pkg}/files", check=False).strip() == "":
            if time.monotonic() - t0 > 30:
                raise Harness(f"Android did not make /sdcard/Android/data/{self.pkg}/files")
            time.sleep(0.5)
        # Every system the copy knows (its es_systems.cfg, by id: "gamecube", not the folder "gc") is hidden but ours,
        # so none of the owner's games is on its shelf (21.11.2026: folder names hid nothing, a press started his
        # GameCube game in his Dolphin).
        cfg = self.sh(f"run-as {self.pkg} cat files/handheld/es_systems.cfg", check=False)
        names = re.findall(r"<name>([^<]+)</name>", cfg)
        if self.system not in names:
            raise Harness(f"the copy's es_systems.cfg does not list {self.system} ({len(names)} systems)")
        self.hidden = sorted(set(names) - {self.system})
        self.sh(f"am force-stop {self.pkg}")
        self.sh(f"rm -rf {self.tmp}; mkdir -p {self.tmp} {self.games}/{self.system}")
        self.adb_run("push", rom, os.path.join(work, "card", "cover0.png"), os.path.join(work, "card", "cover1.png"), self.tmp + "/")
        for i, g in enumerate(GAMES):
            self.sh(f"cp {self.tmp}/game{self.ext} '{self.games}/{self.system}/{g}{self.ext}'")

    def card(self, look):
        """A fresh card in the QA package for one look: no game played, our covers and settings."""
        self.sh(f"am force-stop {self.pkg}")
        s = json.dumps({"onboarded": True, "seen_version": "9.9.9", "theme": look, "animations": "cozy", "motion": "full",
                        "where_was_i": True, "extras": {"cabinet_tv": True, "tv_plays_game": False, "tv_switch_on": True,
                                                        "sealed_new_games": False},
                        "hidden_systems": self.hidden})
        self.sh(f"echo '{s}' > {self.tmp}/settings.json")
        h = "files/handheld"
        self.runas(f"rm -rf files/states files/saves {h}/recents.json {h}/recents.json.bak {h}/last-game.json {h}/play-log.bin; "
                   f"mkdir -p {h}/covers/{self.system}; cp {self.tmp}/settings.json {h}/; "
                   f"cp {self.tmp}/cover0.png \"{h}/covers/{self.system}/{GAMES[0]}.png\"; cp {self.tmp}/cover1.png \"{h}/covers/{self.system}/{GAMES[1]}.png\"; "
                   f"echo /storage/emulated/0/Android/data/{self.pkg}/files/kern-roms > {h}/games-folders")
        # No update offer from the real list (the QA copy is older than the release): a list nobody answers.
        self.sh(f"am start -n {self.pkg}/xyz.pocketui.app.HandheldActivity --es updateUrl http://127.0.0.1:9/none.json")
        t0 = time.monotonic()
        while time.monotonic() - t0 < 30:
            if "HandheldActivity" in self.focus() and lit_share(self.still()) > 5:
                break
            time.sleep(0.5)
        else:
            raise Harness(f"the Handheld view did not come ({self.focus().strip()})")
        time.sleep(3.0)

    def watch(self):
        """From the first picture to the end: every 0.4 s, what is in front. Anything but our package (the owner's apps,
        an emulator app such as Dolphin or RetroArch a start handed over to) stops the walk at once: our package is
        force-stopped, and so is that app when it was not running before the walk."""
        def run():
            while not self.stop_watch:
                front = self.focus()
                m = re.search(r"u0 ([\w.]+)/", front)
                if m and m.group(1) != self.pkg:
                    other = m.group(1)
                    self.sh(f"am force-stop {self.pkg}", check=False)
                    if other not in self.running_before and other not in ("com.android.systemui",):
                        self.sh(f"am force-stop {other}", check=False)
                    self.abort = f"{other} came to the front during the walk: stopped at once"
                    return
                time.sleep(0.4)
        self.stop_watch, self.abort = False, None
        threading.Thread(target=run, daemon=True).start()

    def check(self):
        if self.abort:
            e = Harness(self.abort)
            e.final = True
            raise e

    def started_ours(self):
        """The last game the Handheld view started is one of ours (its log line names the file)."""
        lines = [l for l in self.sh("logcat -d -s PocketHandheld:I", check=False).splitlines() if "app_android: launch " in l]
        return bool(lines) and f"/files/kern-roms/{self.system}/" in lines[-1]

    def focus(self):
        return next((l for l in self.sh("dumpsys window", check=False).splitlines() if "mCurrentFocus" in l), "")

    def key(self, *names, gap=0.35):
        for n in names:
            self.check()
            # Only into our package: a press never reaches the owner's app (or anything he opened meanwhile).
            front = self.focus()
            if f"{self.pkg}/" not in front:
                raise Harness(f"not pressing {n}: {front.strip() or 'nothing'} is in front, not {self.pkg}")
            self.sh(f"input gamepad keyevent {ANDROID_KEYS[n]}")
            time.sleep(gap)

    def still(self):
        png = subprocess.run(self.adb + ["exec-out", "screencap", "-p"], capture_output=True, timeout=30).stdout
        return self.decode(png, "png")[0][1]

    def decode(self, data, fmt):
        """Pictures (half size) with their times from a PNG or an MP4 (from a file: screenrecord's index is at its end)."""
        import tempfile
        w, h = self.size[0] // 2, self.size[1] // 2
        with tempfile.NamedTemporaryFile(suffix="." + ("mp4" if fmt == "mp4" else "png")) as f:
            f.write(data)
            f.flush()
            args = ["ffmpeg", "-loglevel", "info", "-i", f.name] + (["-vsync", "passthrough"] if fmt == "mp4" else []) + \
                   ["-vf", f"scale={w}:{h},showinfo", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]
            r = subprocess.run(args, capture_output=True)
        fr = np.frombuffer(r.stdout, np.uint8)
        n = len(fr) // (w * h * 3)
        fr = fr[:n * w * h * 3].reshape(n, h, w, 3)
        ts = [float(x) for x in re.findall(rb"pts_time:([0-9.]+)", r.stderr)][:n]
        return list(zip(ts, fr))

    def record(self, secs, act=None, lead=1.2):
        """screenrecord for `secs` (VFR, no frame lost), `act` after `lead` seconds; (time, picture) pairs."""
        self.sh("rm -f /sdcard/kern-walk.mp4")
        p = subprocess.Popen(self.adb + ["shell", f"screenrecord --bit-rate 20000000 --time-limit {int(secs + lead + 1)} /sdcard/kern-walk.mp4"],
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(lead)
        if act:
            act()
        p.wait(timeout=secs + lead + 30)
        time.sleep(0.5)
        data = subprocess.run(self.adb + ["exec-out", "cat /sdcard/kern-walk.mp4"], capture_output=True, timeout=120).stdout
        self.sh("rm -f /sdcard/kern-walk.mp4")
        tf = self.decode(data, "mp4")
        if not tf:
            raise Harness("screenrecord gave no pictures")
        return tf

    def end(self):
        self.stop_watch = True
        try:
            self.sh(f"am force-stop {self.pkg}", check=False)
            self.sh(f"pm uninstall {self.pkg}", check=False)
            self.sh(f"rm -rf {self.tmp} /sdcard/kern-walk.mp4", check=False)
        finally:
            if self.lock:
                self.lock.close()


def android_launch(tf):
    """A launch on Android: the Handheld view's scene from the first change, the dark hand-over, then the player's
    pictures from the first lit one after it. (ui, game) as (time, picture) pairs."""
    start = next((i for i in range(1, len(tf)) if diff(tf[i][1], tf[0][1]) > 0.5), None)
    if start is None:
        return [], []
    dark = next((i for i in range(start, len(tf)) if lit_share(tf[i][1]) < 1.0), None)
    if dark is None:
        return tf[start - 1:], []
    first = next((i for i in range(dark, len(tf)) if lit_any(tf[i][1])), None)
    return tf[start - 1:dark], (tf[first:] if first is not None else [])


def walk_android(a, out):
    d = os.path.join(out, "android")
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(d)
    m = {}

    def keep(key, tf, **numbers):
        tf = [p if isinstance(p, tuple) else (i, p) for i, p in enumerate(tf)]
        m[key] = dict(numbers, frames=[tf[0][1], mid(tf), tf[-1][1]] if tf else [])

    def launch_a(dev, k_insert, k_console, k_tv, look=""):
        step[0] = k_tv
        # A system's place first (the Shelf look starts on the systems): A opens it, the next A starts the game.
        for attempt in range(3):
            # A only on a picture of ours: our cover selected (or, once, the Shelf look's system shelf, which A
            # opens, Split's list of systems the same; our cover must be there after it).
            dev.check()
            if "HandheldActivity" in dev.focus() and cover_share(dev.still()) < 0.3 and not (look in ("shelf", "split") and attempt == 0):
                raise Harness(f"{k_tv}: not pressing A, no cover of ours on the screen")
            tf = dev.record(9.0, lambda: dev.key("a", gap=0))
            dev.check()
            if "PlayerActivity" in dev.focus():
                if not dev.started_ours():
                    dev.sh(f"am force-stop {dev.pkg}")
                    raise Harness(f"{k_tv}: a game not of ours started: stopped at once")
                break
            time.sleep(1.5)
        else:
            raise Harness(f"{k_tv}: no game started ({dev.focus().strip()})")
        ui, game = android_launch(tf)
        if k_insert and look == "split":
            keep(k_insert, ui + game[:1], kind="start", reached=bool(game), empty=empty(game[-1][1]) if game else True)
            return
        if k_insert:
            cut = ui[0][0] + (ui[-1][0] - ui[0][0]) * (0.55 if k_console else 1.0) if ui else 0
            part = [p for p in ui if p[0] <= cut]
            keep(k_insert, part, **anim(part))
            if k_console:
                rest = [p for p in ui if p[0] > cut]
                keep(k_console, rest, **anim(rest))
        t0 = game[0][0] if game else 0
        first = [f for t, f in game if t - t0 < 1.0]
        settled = [f for t, f in game if t - t0 >= 1.0]
        ref = settled[-1] if settled else (game[-1][1] if game else None)
        keep(k_tv, first or ([ref] if ref is not None else []), tv=tv_pictures(first, ref) if ref is not None else 0,
             reached=bool(game), empty=empty(ref) if ref is not None else True)

    def save_quit(dev):
        dev.key("home")
        time.sleep(1.5)
        # The player's menu: Continue, Save, Load, Earlier, Options, Save & quit (the 0.6 player; more rows: Screenshot,
        # Controls, Zoom before Save & quit on newer ones; ANDROID_QUIT_DOWNS says how many).
        dev.key(*["down"] * int(os.environ.get("ANDROID_QUIT_DOWNS", "5")))

    dev = Android(a)
    step = ["begin"]
    try:
        dev.begin(d)
        dev.card("cabinet")
        dev.watch()
        still = dev.still()
        keep("schrank", [still], empty=empty(still), lit=lit_share(still))
        # No D-pad on the shelf of a device (21.11.2026): the case and its cover from the still, its size against the reference.
        keep("huelle", [still], cover=cover_share(still), empty=empty(still))
        launch_a(dev, "einlegen", "konsole", "tv")
        tf = dev.record(5.0, lead=0.2)
        game = [f for _, f in tf]
        # The test program stands still: its picture and that it is there, not its motion.
        keep("spiel", tf, picture=box_share(lum(game[-1]) > 40)[0], empty=any(empty(f) for f in game[::10]))
        save_quit(dev)
        ej = dev.record(6.0, lambda: dev.key("a", gap=0))
        if "HandheldActivity" not in dev.focus():
            raise Harness(f"Save & quit did not come back to the shelf ({dev.focus().strip()})")
        start = next((i for i in range(1, len(ej)) if diff(ej[i][1], ej[0][1]) > 0.5), 0)
        keep("auswerfen", ej[start:], **anim(ej[start:]))
        time.sleep(1.0)
        launch_a(dev, None, None, "continue")
        save_quit(dev)
        dev.key("a")
        time.sleep(3)
        for look, _ in LOOKS:
            dev.stop_watch = True
            time.sleep(0.5)
            dev.card(look)
            dev.watch()
            still = dev.still()
            keep(f"regal-{look}", [still], empty=empty(still), lit=lit_share(still))
            launch_a(dev, f"einlegen-{look}", None, f"tv-{look}", look)
            save_quit(dev)
            dev.key("a")
            time.sleep(2)
    except Harness as e:
        e.partial = m
        try:
            m["-"] = {"frames": [dev.still()]}   # what the screen showed when the walk stopped
        except Exception:
            pass
        raise
    finally:
        dev.end()
    return m


# --- judging ------------------------------------------------------------------------------------------------------------
def judge(numbers, ref=None, plus=None):
    """Red lines for one device's moments: (moment, text)."""
    red = []
    for key, name, kind in MOMENTS:
        n = numbers.get(key)
        if n is None:
            red.append((key, "nicht erreicht"))
            continue
        if n.get("empty"):
            red.append((key, "leeres Bild"))
        if kind in MIN_MOTION and "motion" in n and n["motion"] < MIN_MOTION[kind]:
            red.append((key, f"Bewegung {n.get('motion', 0)} Bilder (mindestens {MIN_MOTION[kind]})"))
        if kind == "start" and not n.get("reached"):
            red.append((key, "das Spiel kam nicht"))
        if kind == "tv":
            if not n.get("reached"):
                red.append((key, "das Spiel kam nicht"))
            elif n.get("tv", 0) < MIN_TV:
                red.append((key, f"kein Einschalten: {n.get('tv', 0)} Bilder (mindestens {MIN_TV})"))
        if kind == "case" and n.get("cover", 0) <= 0:
            red.append((key, "kein Cover auf der Hülle (Platzhalter)"))
        for other, who in ((plus, "Plus"), (ref, "Referenz")):
            o = (other or {}).get(key)
            if not o:
                continue
            # The game's picture is scaled by whole numbers per screen (the Brick's 3x, the Plus's 2x): only against
            # the device's own reference.
            for field in ("lit",) if who == "Plus" else ("lit", "picture"):
                if field in n and field in o and abs(n[field] - o[field]) > SHARE_POINTS:
                    red.append((key, f"Größe {field} {n[field]:.1f} % gegen {who} {o[field]:.1f} % (±{SHARE_POINTS:.0f})"))
            if n.get("cover") and o.get("cover"):
                r = n["cover"] / o["cover"]
                if not COVER_RATIO[0] <= r <= COVER_RATIO[1]:
                    red.append((key, f"Hülle {n['cover']:.1f} % gegen {who} {o['cover']:.1f} %"))
            if who == "Referenz":
                for field in ("motion", "tv"):
                    if o.get(field, 0) >= 8 and n.get(field, 0) < o[field] / 2:
                        red.append((key, f"{field} {n.get(field, 0)} gegen Referenz {o[field]}"))
    return red


# --- the page -----------------------------------------------------------------------------------------------------------
def jpeg(frames, path, width=220):
    """Frames side by side as one small JPEG (ffmpeg; Pillow is not everywhere)."""
    if not frames:
        return False
    h0, w0 = frames[0].shape[:2]
    step = max(1, w0 // width)
    th = [f[::step, ::step] for f in frames]
    hh, ww = min(t.shape[0] for t in th), min(t.shape[1] for t in th)
    gap = np.full((hh, 6, 3), 255, np.uint8)
    parts = []
    for i, t in enumerate(th):
        parts += ([gap] if i else []) + [t[:hh, :ww]]
    strip = np.ascontiguousarray(np.concatenate(parts, axis=1))
    r = subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s",
                        f"{strip.shape[1]}x{strip.shape[0]}", "-i", "-", "-q:v", "5", path], input=strip.tobytes())
    return r.returncode == 0


def page(out, results, ref_dir, label):
    pg = os.path.join(out, "page")
    os.makedirs(pg, exist_ok=True)
    rows = [f"<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'><title>Kern-Abnahme {html.escape(label)}</title>",
            "<style>body{font:15px system-ui;margin:12px;background:#111;color:#eee}img{max-width:100%;display:block}"
            "td{vertical-align:top;padding:4px 8px;border-bottom:1px solid #333}.r{color:#f66}.g{color:#6d6}h2{margin-top:28px}"
            "small{color:#aaa}</style>",
            f"<h1>Kern-Abnahme {html.escape(label)}</h1><p>Pro Moment: Anfang · Mitte · Ende. Links Referenz, rechts jetzt."
            " Rot = eine der drei Zahlen (Bewegung, Größe, Cover) oder ein leeres Bild.</p>"]
    for dev, r in results.items():
        red = {k for k, _ in r["red"]}
        rows.append(f"<h2>{dev} <small>{html.escape(DEVICES[dev]['path'])}</small> "
                    f"<span class={'r' if red else 'g'}>{'ROT' if red else 'grün'}</span></h2><table>")
        for key, name, _ in ([("-", "Gang abgebrochen, letztes Bild", "")] if "-" in r["numbers"] else []) + MOMENTS:
            n = r["numbers"].get(key, {})
            img = f"{dev}-{key}.jpg"
            jpeg(n.get("frames", []), os.path.join(pg, img))
            refimg = os.path.join(ref_dir, dev, f"{key}.jpg") if ref_dir else ""
            cells = []
            if refimg and os.path.exists(refimg):
                shutil.copy(refimg, os.path.join(pg, f"ref-{dev}-{key}.jpg"))
                cells.append(f"<img src='ref-{dev}-{key}.jpg'>")
            cells.append(f"<img src='{img}'>" if n.get("frames") else "<b class=r>kein Bild</b>")
            nums = ", ".join(f"{k} {v:.1f}" if isinstance(v, float) else f"{k} {v}" for k, v in n.items() if k != "frames")
            why = "; ".join(t for k, t in r["red"] if k == key)
            rows.append(f"<tr><td><b class={'r' if key in red else 'g'}>{html.escape(name)}</b><br><small>{html.escape(nums)}</small>"
                        f"{'<br><span class=r>' + html.escape(why) + '</span>' if why else ''}</td><td>{''.join(cells)}</td></tr>")
        rows.append("</table>")
    with open(os.path.join(pg, "index.html"), "w") as fh:
        fh.write("\n".join(rows))
    return pg


def strip_frames(numbers):
    return {k: {f: v for f, v in n.items() if f != "frames"} for k, n in numbers.items()}


# --- self-test: the numbers on made-up pictures -------------------------------------------------------------------------
def self_test():
    if np is None:
        print("kern-walk: numpy is missing", file=sys.stderr)
        return 2
    H, W = 96, 128
    black = np.zeros((H, W, 3), np.uint8)
    game = np.zeros((H, W, 3), np.uint8)
    game[:, :, 1] = np.linspace(40, 255, W, dtype=np.uint8)[None, :]
    game[:, :, 0] = np.linspace(40, 255, H, dtype=np.uint8)[:, None]

    def bar(f, x):
        f = f.copy()
        f[:, x % W] = 255
        return f
    # A switch-on: dark, a line, the picture opening out of it; then the game.
    on = [black.copy() for _ in range(2)]
    for k in range(1, 9):
        f = black.copy()
        f[H // 2 - k * H // 18: H // 2 + k * H // 18 + 1] = game[H // 2 - k * H // 18: H // 2 + k * H // 18 + 1]
        on.append(f)
    settled = bar(game, 40)
    with_tv = on + [bar(game, x) for x in range(20)]
    without = [bar(game, x) for x in range(30)]
    assert tv_pictures(with_tv, settled) >= MIN_TV, tv_pictures(with_tv, settled)
    assert tv_pictures(without, settled) == 0, "a game picture with only its bar moving is no switch-on"
    assert empty(black) and not empty(game) and empty(np.full((H, W, 3), 90, np.uint8))
    assert distinct([black, black, game, game, bar(game, 3)]) == 3
    still = [black] * 20
    assert distinct(still) == 1 and motion_share(still) == 0.0
    box = black.copy()
    box[10:50, 20:84] = 200
    assert abs(motion_share([black, box]) - 40 * 64 * 100 / (H * W)) < 0.01
    cov = black.copy()
    cov[20:60, 30:60] = MAGENTA
    cov[20:40, 30:60] = (150, 40, 160)  # in the case's shade
    assert abs(cover_share(cov) - 40 * 30 * 100 / (H * W)) < 0.01 and cover_share(game) == 0.0
    # Judging: a missing switch-on, a shrunken figure, a missing cover, a blank picture are red; the same numbers green.
    ok = {k: {"empty": False, "motion": 40, "tv": 12, "reached": True, "cover": 4.0, "lit": 50.0,
              "picture": 80.0} for k, _, _ in MOMENTS}
    assert judge(ok, ok, ok) == [], judge(ok, ok, ok)
    bad = json.loads(json.dumps(ok))
    bad["tv"]["tv"] = 0
    bad["einlegen"]["lit"] = 41.0
    bad["huelle"]["cover"] = 0.0
    bad["spiel"]["empty"] = True
    keys = {k for k, _ in judge(bad, ok, ok)}
    assert keys == {"tv", "einlegen", "huelle", "spiel"}, judge(bad, ok, ok)
    assert any("Plus" in t for _, t in judge(bad, None, ok)) and any("Referenz" in t for _, t in judge(bad, ok, None))
    gone = {k: v for k, v in ok.items() if k != "auswerfen"}
    assert judge(gone) == [("auswerfen", "nicht erreicht")]
    # Android: the scene from its first change, the dark hand-over, the player's pictures from the first lit one.
    shelf = game.copy()
    seq = [(0.0, shelf), (0.1, shelf), (0.2, bar(shelf, 5)), (0.3, bar(shelf, 9)), (0.4, black), (0.5, black)] + \
          [(0.6 + k / 60, f) for k, f in enumerate(on)] + [(1.0 + k / 60, bar(game, k)) for k in range(30)]
    ui, played = android_launch(seq)
    assert [t for t, _ in ui] == [0.1, 0.2, 0.3], ui
    assert played and played[0][0] > 0.5 and lit_any(played[0][1]) and len(played) == len(on) - 2 + 30, len(played)
    assert android_launch([(0.0, shelf)] * 5) == ([], [])
    print(f"kern-walk self-test: ok ({len(MOMENTS)} moments)")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--bin", help="folder with pocket-ui, pocket-core, pocket-fb (x86_64, --features brick)")
    ap.add_argument("--core", help="another pocket-core than the one in --bin")
    ap.add_argument("--out")
    ap.add_argument("--devices", default="brick,plus,miniv2")
    ap.add_argument("--ref", default="", help="reference folder (<device>/numbers.json and pictures, from --make-ref)")
    ap.add_argument("--make-ref", action="store_true", help="also write this run as a reference folder OUT/ref")
    ap.add_argument("--label", default=time.strftime("%Y-%m-%d %H:%M"))
    ap.add_argument("--publish", metavar="DIR", help="also copy the page to DIR (a folder a web server shows)")
    ap.add_argument("--android", metavar="ADB", help='also the Android Handheld view: the adb command, e.g. "adb -s <serial>"')
    ap.add_argument("--android-apk", help="the QA copy of the APK to install beside the owner's app (its own package)")
    ap.add_argument("--android-pkg", help="that QA copy's package name")
    ap.add_argument("--android-lock", default=ANDROID_LOCK, help="the device's session lock (an emulator: a lock of its own)")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if np is None:
        print("kern-walk: needs numpy (python3-numpy)", file=sys.stderr)
        return 2
    if not (a.bin and a.out):
        ap.error("--bin and --out are needed")
    a.ui, a.fb = os.path.join(a.bin, "pocket-ui"), os.path.join(a.bin, "pocket-fb")
    a.core = os.path.abspath(a.core or os.path.join(a.bin, "pocket-core"))
    for p in (a.ui, a.fb, a.core):
        if not os.access(p, os.X_OK):
            print(f"kern-walk: {p} is missing", file=sys.stderr)
            return 2
    for tool in ("bwrap", "ffmpeg", "cc"):
        if not shutil.which(tool):
            print(f"kern-walk: needs {tool}", file=sys.stderr)
            return 2
    a.out = os.path.abspath(a.out)
    os.makedirs(a.out, exist_ok=True)
    # Unix sockets must fit 107 bytes: the walk's folder is short, under the home folder (its /tmp is the walk's own).
    work = os.path.join(os.path.expanduser("~"), f".kw{os.getpid()}")
    os.makedirs(work, exist_ok=True)
    stub = os.path.join(work, "stub_libretro.so")
    subprocess.run(["cc", "-shared", "-fPIC", "-O1", "-o", stub, os.path.join(ROOT, "scripts/crash-sweep/stub-core.c"), "-lm"], check=True)
    rom = os.path.join(work, "game.sfc")
    with open(rom, "wb") as fh:
        fh.write(b"kern-walk stand-in, no game\n" * 64)
    a.stub = {"core": stub, "rom": rom}
    devices = [x for x in a.devices.split(",") if x]
    if a.android and "android" not in devices:
        devices.append("android")
    if "android" in devices and not (a.android and a.android_apk and a.android_pkg and os.path.exists(a.android_apk)):
        print("kern-walk: android needs --android ADB, --android-apk FILE and --android-pkg NAME", file=sys.stderr)
        return 2
    for dev in devices:
        if dev not in DEVICES:
            print(f"kern-walk: unknown device {dev}", file=sys.stderr)
            return 2
    numbers, failed, final = {}, {}, set()
    t_all = time.monotonic()

    def walk_one(dev):
        t0 = time.monotonic()
        failed.pop(dev, None)
        try:
            numbers[dev] = walk_android(a, work) if dev == "android" else walk(a, dev, work)
        except Harness as e:
            failed[dev] = str(e)
            if e.final:
                final.add(dev)
            numbers[dev] = dict(getattr(e, "partial", {}))
            # What the screen showed when the walk stopped: on the page, never an empty gap.
            try:
                if "-" in numbers[dev]:
                    raise OSError("already there")
                numbers[dev]["-"] = {"frames": [Recorder([(os.path.join(work, dev, "fb"), "ui")], DEVICES[dev]["size"]).now()]}
            except (OSError, ValueError):
                pass
        finally:
            if os.path.isdir(os.path.join(work, dev)):
                dst = os.path.join(a.out, dev)
                shutil.rmtree(dst, ignore_errors=True)
                shutil.copytree(os.path.join(work, dev), dst, ignore=shutil.ignore_patterns("fb", "fb2", "w*", "i", "i2", "cores", "tmp", "*.so"))
        print(f"kern-walk: {dev} walked in {time.monotonic() - t0:.0f} s" + (f" (harness: {failed[dev]})" if dev in failed else ""), flush=True)

    ref_dir = a.ref if a.ref and os.path.isdir(a.ref) else ""
    refs = {dev: json.load(open(os.path.join(ref_dir, dev, "numbers.json"))) for dev in devices
            if ref_dir and os.path.exists(os.path.join(ref_dir, dev, "numbers.json"))}

    def reds():
        out = {}
        for dev in devices:
            plus = strip_frames(numbers["plus"]) if dev == "brick" and numbers.get("plus") and "plus" not in failed else None
            out[dev] = judge(strip_frames(numbers[dev]), refs.get(dev), plus) if dev not in failed else [("-", f"Gang abgebrochen: {failed[dev]}")]
        return out

    try:
        for dev in devices:
            walk_one(dev)
        first = reds()
        # The build computer is shared: a moment whose pictures came slowly once (another job's load) is walked again. Red is what is
        # red both times (the 20.11. switch-on is 0 pictures every time; findings only when they repeat).
        again = [dev for dev in devices if first[dev] and dev not in final]
        for dev in again:
            print(f"kern-walk: {dev} again ({len(first[dev])} red)", flush=True)
            walk_one(dev)
        second = reds() if again else first
    finally:
        shutil.rmtree(work, ignore_errors=True)
    results = {}
    for dev in devices:
        twice = {k for k, _ in first[dev]} & {k for k, _ in second[dev]}
        red = [(k, t) for k, t in second[dev] if k in twice]
        once = sorted({k for k, _ in first[dev]} ^ {k for k, _ in second[dev]})
        results[dev] = {"numbers": numbers[dev], "red": red, "ref": dev in refs, "once": once,
                        "first": [f"{k}: {t}" for k, t in first[dev] if k in once]}
    pg = page(a.out, results, ref_dir, a.label)
    if a.make_ref:
        for dev in devices:
            rd = os.path.join(a.out, "ref", dev)
            os.makedirs(rd, exist_ok=True)
            json.dump(strip_frames(numbers[dev]), open(os.path.join(rd, "numbers.json"), "w"), indent=1)
            for key, _, _ in MOMENTS:
                if os.path.exists(os.path.join(pg, f"{dev}-{key}.jpg")):
                    shutil.copy(os.path.join(pg, f"{dev}-{key}.jpg"), os.path.join(rd, f"{key}.jpg"))
    json.dump({d: {"numbers": strip_frames(r["numbers"]), "red": r["red"], "ref": r["ref"], "once": r["once"]} for d, r in results.items()},
              open(os.path.join(a.out, "numbers.json"), "w"), indent=1)
    lines = [f"# Kern-Abnahme {a.label}", "", f"{time.monotonic() - t_all:.0f} s, core {a.core}", ""]
    for dev, r in results.items():
        lines.append(f"- **{dev}**: {'ROT' if r['red'] else 'grün'}" + ("" if r["ref"] else " (ohne Referenz)"))
        lines += [f"  - {k}: {t}" for k, t in r["red"]]
        if r["once"]:
            lines.append(f"  - nur einmal rot, beim zweiten Gang nicht (Last auf dem Rechner): {'; '.join(r['first']) or ', '.join(r['once'])}")
    open(os.path.join(a.out, "report.md"), "w").write("\n".join(lines) + "\n")
    print("\n".join(lines))
    print(f"kern-walk: page {pg}/index.html")
    if a.publish:
        shutil.rmtree(a.publish, ignore_errors=True)
        shutil.copytree(pg, a.publish)
        print(f"kern-walk: page also in {a.publish}")
    return 1 if any(r["red"] for r in results.values()) else 0


if __name__ == "__main__":
    sys.exit(main())
