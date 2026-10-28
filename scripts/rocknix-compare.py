#!/usr/bin/env python3
"""pocket-ui against stock ROCKNIX (EmulationStation + RetroArch), measured from outside the same way for both.

pocket-ui's own trace (scripts/latency.py) does not exist in EmulationStation, so this tool looks only at what both
have in common: a button press goes into the Pocket's pad, and the screen changes. Docs: docs/COMPARE-ROCKNIX.md.

  - Input: written into the pad device InputPlumber reads (the "Retroid Pocket Gamepad", in
    /dev/inputplumber/sources once InputPlumber holds it), so it takes the path a finger's press takes: InputPlumber,
    its virtual DualSense, then the UI (EmulationStation through SDL, pocket-ui through evdev, RetroArch through udev).
    The time of the write (CLOCK_MONOTONIC) is the time of the press.
  - Screen: a Wayland screencopy client (zwlr_screencopy_manager_v1, pure Python) records every frame sway commits,
    each with the compositor's own CLOCK_MONOTONIC time stamp, a few pixels of every 8th row and column, to a file.
  - Afterwards, per press: the first frame that differs from the screen at the press ("react") and the first frame
    from which the screen shows the end state of that step ("done").

Device side (Python standard library only, ROCKNIX's Python; started by scripts/rocknix-compare-device.sh):
  rocknix-compare.py record --out FILE [--step 8]        the recorder (SIGUSR1 pauses it, SIGUSR2 resumes)
  rocknix-compare.py drive --ui es|pocket-ui --plan PLAN --out DIR [--scenarios ...] [--reps N]
  rocknix-compare.py probe                               which source button gives which DualSense button
  rocknix-compare.py press SEQ                           presses, e.g. "RIGHT RIGHT A" or "HOME+X" (calibration)
On your computer:
  rocknix-compare.py analyze DIR [--out FILE]            numbers of one run (DIR from drive) as JSON
  rocknix-compare.py table FILE...                       markdown table, EmulationStation against pocket-ui
  rocknix-compare.py thumbs DIR EVENT...                 JPEG strips around presses, to check what was measured
  rocknix-compare.py selftest                            Wayland client against a fake compositor, analysis on
                                                         synthetic recordings (no device)
"""

import argparse
import errno
import fcntl
import json
import mmap
import os
import select
import signal
import socket
import struct
import subprocess
import sys
import threading
import time
import zlib

# --- buttons ---------------------------------------------------------------------------------------------------

# Named by their place on the Pocket (Nintendo layout: A right, B bottom, X top, Y left), which is the place of the
# button InputPlumber's DualSense reports. Values: the code written into the source device (Mini V2, ROCKNIX
# 20261001, checked with `probe` on 2026-10-26: X is BTN_NORTH, Y BTN_WEST, as vpad.py has them).
# `probe` checks them on the device and the run uses what it found.
EV_SYN, EV_KEY, EV_ABS = 0, 1, 3
SOURCE = {"A": 305, "B": 304, "X": 307, "Y": 308, "L": 310, "R": 311, "SELECT": 314, "START": 315, "HOME": 316,
          "UP": 544, "DOWN": 545, "LEFT": 546, "RIGHT": 547}
# What InputPlumber's DualSense (hid-playstation) reports for each: a key, or the D-pad as a hat (ABS_HAT0X/Y).
TARGET = {"A": ("key", 305), "B": ("key", 304), "X": ("key", 307), "Y": ("key", 308), "L": ("key", 310),
          "R": ("key", 311), "SELECT": ("key", 314), "START": ("key", 315), "HOME": ("key", 316),
          "UP": ("hat", 17, -1), "DOWN": ("hat", 17, 1), "LEFT": ("hat", 16, -1), "RIGHT": ("hat", 16, 1)}
KEY_POWER = 116
CLOCK_MONOTONIC = 1
EVIOCSCLOCKID = 0x400445A0
EVENT = struct.Struct("llHHi")  # struct input_event on 64-bit Linux

SOURCE_NAME = "Retroid Pocket Gamepad"
DUALSENSE_NAME = "DualSense Wireless Controller"


def mono_ns():
    return time.monotonic_ns()


def log(msg):
    print("compare: " + msg, file=sys.stderr, flush=True)


class Fail(Exception):
    pass


# COMPARE_INPUT_DEVICES, COMPARE_INPUT_DIR and COMPARE_ROOT are for the self-test's simulation (`selftest --sim`).
def input_devices(path=None):
    """[{name, phys, handlers, keys:set}] from /proc/bus/input/devices."""
    path = path or os.environ.get("COMPARE_INPUT_DEVICES", "/proc/bus/input/devices")
    out = []
    try:
        text = open(path).read()
    except OSError:
        return out
    for block in text.split("\n\n"):
        d = {"name": "", "phys": "", "handlers": [], "keys": set()}
        for line in block.splitlines():
            if line.startswith("N: Name="):
                d["name"] = line[8:].strip().strip('"')
            elif line.startswith("P: Phys="):
                d["phys"] = line[8:].strip()
            elif line.startswith("H: Handlers="):
                d["handlers"] = line[12:].split()
            elif line.startswith("B: KEY="):
                words = line[7:].split()
                bits = 0
                for w in words:
                    bits = (bits << 64) | int(w, 16)
                d["keys"] = {i for i in range(bits.bit_length()) if bits >> i & 1}
        if d["name"]:
            out.append(d)
    return out


def event_node(dev):
    ev = next((h for h in dev["handlers"] if h.startswith("event")), None)
    if not ev:
        return None
    dirs = [os.environ["COMPARE_INPUT_DIR"]] if os.environ.get("COMPARE_INPUT_DIR") else ["/dev/input", "/dev/inputplumber/sources"]
    for p in (f"{d}/{ev}" for d in dirs):
        if os.path.exists(p):
            return p
    return None


class Pad:
    """Presses go into the source device InputPlumber reads; a reader on its DualSense notes when they come out.

    inject="target" writes into InputPlumber's DualSense instead (its buttons, the D-pad as a hat): for a device on which
    InputPlumber grabs its source, so that the kernel drops what others write into it (`probe` says so). That skips
    InputPlumber, for both UIs alike."""

    def __init__(self, codes=None, inject="source", check=True):
        devs = input_devices()
        src = next((d for d in devs if d["name"] == SOURCE_NAME), None)
        ds = next((d for d in devs if DUALSENSE_NAME in d["name"] and event_node(d) and 316 in d["keys"]), None)
        self.inject = inject
        self.codes = dict(SOURCE)
        if codes:
            self.codes.update(codes)
        if inject == "target":
            if not ds:
                raise Fail(f"no '{DUALSENSE_NAME}' input device to write into")
            self.source = event_node(ds)
            self.source_keys = ds["keys"]
        else:
            if not src or not event_node(src):
                raise Fail(f"no '{SOURCE_NAME}' input device (is this a Retroid Pocket with InputPlumber?)")
            self.source = event_node(src)
            self.source_keys = src["keys"]
            missing = [b for b, c in self.codes.items() if c not in self.source_keys]
            if missing and check:
                # The kernel drops events of keys a device doesn't have, silently.
                raise Fail(f"{self.source} has no key for {missing}: check the mapping (probe)")
        self.fd = os.open(self.source, os.O_WRONLY)
        self.out = None
        self.out_events = []  # (mono ns, type, code, value) as InputPlumber's DualSense reports them
        if ds and inject == "source":
            self.out = os.open(event_node(ds), os.O_RDONLY)
            try:
                fcntl.ioctl(self.out, EVIOCSCLOCKID, struct.pack("i", CLOCK_MONOTONIC))
            except OSError:
                pass
            threading.Thread(target=self._read_out, daemon=True).start()

    def _read_out(self):
        while True:
            try:
                data = os.read(self.out, EVENT.size * 64)
            except OSError:
                return
            for i in range(0, len(data) - EVENT.size + 1, EVENT.size):
                sec, usec, typ, code, value = EVENT.unpack_from(data, i)
                if typ in (EV_KEY, EV_ABS):
                    self.out_events.append((sec * 1_000_000_000 + usec * 1000, typ, code, value))

    def _emit(self, typ, code, value):
        data = EVENT.pack(0, 0, typ, code, value) + EVENT.pack(0, 0, EV_SYN, 0, 0)
        try:
            os.write(self.fd, data)
        except OSError:
            # The node went away (InputPlumber made it again after a deep sleep): the new one.
            os.close(self.fd)
            end = time.monotonic() + 5
            name = DUALSENSE_NAME if self.inject == "target" else SOURCE_NAME
            while True:
                src = next((d for d in input_devices() if name in d["name"] and event_node(d) and (self.inject != "target" or 316 in d["keys"])), None)
                if src or time.monotonic() > end:
                    break
                time.sleep(0.05)
            if not src:
                raise Fail(f"'{name}' is gone")
            self.source = event_node(src)
            self.fd = os.open(self.source, os.O_WRONLY)
            os.write(self.fd, data)

    def _button(self, btn, down):
        if self.inject == "target":
            want = TARGET[btn]
            if want[0] == "key":
                self._emit(EV_KEY, want[1], 1 if down else 0)
            else:
                self._emit(EV_ABS, want[1], want[2] if down else 0)
        else:
            self._emit(EV_KEY, self.codes[btn], 1 if down else 0)

    def down(self, btn):
        t = mono_ns()
        self._button(btn, True)
        return t

    def up(self, btn):
        t = mono_ns()
        self._button(btn, False)
        return t

    def tap(self, btn, hold_ms=100, mod=None):
        """One press like a finger's: `hold_ms` down. With `mod` (HOME+X), `mod` is held around it. Returns the
        times of the last button's down and up (the press that completes the gesture)."""
        if mod:
            self.down(mod)
            time.sleep(0.06)
        t_down = self.down(btn)
        time.sleep(hold_ms / 1000.0)
        t_up = self.up(btn)
        if mod:
            time.sleep(0.04)
            self.up(mod)
        return t_down, t_up

    def arrival(self, btn, after_ns, timeout=0.5):
        """When InputPlumber's DualSense reported `btn` going down after `after_ns` (ns), or None."""
        want = TARGET[btn]
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            for t, typ, code, value in list(self.out_events):
                if t < after_ns:
                    continue
                if want[0] == "key" and typ == EV_KEY and code == want[1] and value == 1:
                    return t
                if want[0] == "hat" and typ == EV_ABS and code == want[1] and value == want[2]:
                    return t
            time.sleep(0.005)
        return None


def power_key_node():
    """The input device of the power key: the PMIC's (pwrkey, pon), else any other that has KEY_POWER and is not a
    keyboard or InputPlumber's."""
    cands = [d for d in input_devices() if KEY_POWER in d["keys"] and event_node(d)
             and "inputplumber" not in d["name"].lower() and "keyboard" not in d["name"].lower()]
    cands.sort(key=lambda d: 0 if any(w in d["name"].lower() for w in ("pwrkey", "pon", "power")) else 1)
    return (event_node(cands[0]), cands[0]["name"]) if cands else (None, None)


def parse_seq(seq):
    """'RIGHT RIGHT A' / 'HOME+X' / 'A:300' (hold) / 'w500' (wait ms) -> [(mod, btn, hold_ms) | ('wait', ms)]."""
    out = []
    for tok in seq.split():
        if tok[0] == "w" and tok[1:].isdigit():
            out.append(("wait", int(tok[1:])))
            continue
        hold = 100
        if ":" in tok:
            tok, h = tok.split(":", 1)
            hold = int(h)
        mod, _, btn = tok.rpartition("+")
        for b in (mod, btn):
            if b and b not in SOURCE:
                raise Fail(f"unknown button '{b}' in '{seq}' (known: {' '.join(SOURCE)})")
        out.append((mod or None, btn, hold))
    return out


# --- the recording: a Wayland screencopy client ------------------------------------------------------------------

WL_SHM_FORMAT_ARGB8888, WL_SHM_FORMAT_XRGB8888 = 0, 1
# Formats whose 4th byte (in memory) is padding or alpha: B,G,R,X and R,G,B,X.
X_LAST = {0, 1, 0x34325241, 0x34325258, 0x34324241, 0x34324258}  # AR24 XR24 AB24 XB24
X_FIRST = {0x34324152, 0x34325852, 0x34324142, 0x34325842}  # RA24 RX24 BA24 BX24


def wl_str(s):
    b = s.encode() + b"\0"
    return struct.pack("<I", len(b)) + b + b"\0" * (-len(b) % 4)


def wl_msg(obj, opcode, payload=b""):
    return struct.pack("<II", obj, ((8 + len(payload)) << 16) | opcode) + payload


def read_wl_str(data, off):
    n = struct.unpack_from("<I", data, off)[0]
    s = data[off + 4:off + 4 + n - 1].decode(errors="replace")
    return s, off + 4 + n + (-n % 4)


class Wayland:
    """Just enough of the Wayland wire protocol for wl_shm and zwlr_screencopy_manager_v1."""

    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.next_id = 2
        self.objects = {1: "wl_display"}
        self.handlers = {}
        self.buf = b""

    def new_id(self, iface):
        i = self.next_id
        self.next_id += 1
        self.objects[i] = iface
        return i

    def send(self, data, fds=()):
        if fds:
            self.sock.sendmsg([data], [(socket.SOL_SOCKET, socket.SCM_RIGHTS, struct.pack(f"{len(fds)}i", *fds))])
        else:
            self.sock.sendall(data)

    def dispatch(self, timeout):
        """Reads what is there (waits up to `timeout` s) and calls handlers[(iface, opcode)](obj, payload)."""
        r, _, _ = select.select([self.sock], [], [], timeout)
        if not r:
            return False
        data, anc, _, _ = self.sock.recvmsg(1 << 20, socket.CMSG_SPACE(16 * 4))
        for level, typ, fdata in anc:
            if level == socket.SOL_SOCKET and typ == socket.SCM_RIGHTS:
                for fd in struct.unpack(f"{len(fdata) // 4}i", fdata[:len(fdata) - len(fdata) % 4]):
                    os.close(fd)
        if not data:
            raise Fail("the compositor closed the connection")
        self.buf += data
        while len(self.buf) >= 8:
            obj, szop = struct.unpack_from("<II", self.buf)
            size = szop >> 16
            if len(self.buf) < size:
                break
            payload, self.buf = self.buf[8:size], self.buf[size:]
            iface = self.objects.get(obj)
            if iface == "wl_display" and (szop & 0xFFFF) == 0:
                oid, code = struct.unpack_from("<II", payload)
                msg, _ = read_wl_str(payload, 8)
                raise Fail(f"Wayland error on object {oid} ({self.objects.get(oid)}), code {code}: {msg}")
            h = self.handlers.get((iface, szop & 0xFFFF))
            if h:
                h(obj, payload)
        return True

    def roundtrip(self, timeout=5.0):
        cb = self.new_id("wl_callback")
        done = []
        self.handlers[("wl_callback", 0)] = lambda o, p: done.append(o)
        self.send(wl_msg(1, 0, struct.pack("<I", cb)))  # wl_display.sync
        end = time.monotonic() + timeout
        while cb not in done:
            if time.monotonic() > end:
                raise Fail("the compositor does not answer")
            self.dispatch(0.1)


class Recorder:
    """Records every frame the compositor commits (copy_with_damage: a still screen gives no frames).

    File: b'PFR1\\n', one JSON line (the frames' size, format, step), then one record per frame:
    b'FRM1', <ready ns> <received ns> u64, flags u32, number of damage rectangles u16, then those (4 x u16), the
    length of the pixels u32, the pixels (zlib): every `step`th pixel of every `step`th row, 4 bytes each as in the
    compositor's buffer. A b'MARK' record (same head, no pixels) notes a pause or a resume."""

    def __init__(self, out, step=8, output_name=None, display=None, region=None):
        rt = os.environ.get("XDG_RUNTIME_DIR", "/var/run/0-runtime-dir")
        display = display or os.environ.get("WAYLAND_DISPLAY", "wayland-1")
        self.wl = Wayland(display if display.startswith("/") else os.path.join(rt, display))
        self.step = step
        self.region = region  # (x, y, w, h) in the output's logical coordinates: capture_output_region
        self.out = open(out, "wb")
        self.globals = []
        self.outputs = {}
        self.paused = False
        self.want_pause = False
        self.want_resume = False
        self.stop = False
        self.header_written = False
        self.buffers = []
        self.frame = None
        self.frames = 0
        self.output_name = output_name
        wl = self.wl
        wl.handlers[("wl_registry", 0)] = self._global
        self.registry = wl.new_id("wl_registry")
        wl.send(wl_msg(1, 1, struct.pack("<I", self.registry)))
        wl.roundtrip()
        names = {i: (n, v) for n, i, v in self.globals}
        if "zwlr_screencopy_manager_v1" not in names:
            raise Fail("the compositor has no zwlr_screencopy_manager_v1 (not sway/wlroots?)")
        self.shm = self._bind("wl_shm", 1, names)
        self.manager = self._bind("zwlr_screencopy_manager_v1", 3, names)
        self.manager_version = min(3, names["zwlr_screencopy_manager_v1"][1])
        wl.handlers[("wl_output", 1)] = self._mode
        wl.handlers[("wl_output", 4)] = self._name
        for n, iface, v in self.globals:
            if iface == "wl_output":
                oid = wl.new_id("wl_output")
                wl.send(wl_msg(self.registry, 0, struct.pack("<I", n) + wl_str("wl_output") + struct.pack("<II", min(v, 4), oid)))
                self.outputs[oid] = {"name": None, "mode": None}
        wl.roundtrip()
        if not self.outputs:
            raise Fail("no output")
        pick = [o for o, d in self.outputs.items() if output_name and d["name"] == output_name]
        self.output = (pick or sorted(self.outputs))[0]
        for ev, fn in ((0, self._buffer), (1, self._flags), (2, self._ready), (3, self._failed), (4, self._damage), (6, self._buffer_done)):
            wl.handlers[("zwlr_screencopy_frame_v1", ev)] = fn

    def _global(self, _o, p):
        name = struct.unpack_from("<I", p)[0]
        iface, off = read_wl_str(p, 4)
        ver = struct.unpack_from("<I", p, off)[0]
        self.globals.append((name, iface, ver))

    def _bind(self, iface, ver, names):
        oid = self.wl.new_id(iface)
        n, v = names[iface]
        self.wl.send(wl_msg(self.registry, 0, struct.pack("<I", n) + wl_str(iface) + struct.pack("<II", min(v, ver), oid)))
        return oid

    def _mode(self, o, p):
        flags, w, h, refresh = struct.unpack_from("<Iiii", p)
        if flags & 1:
            self.outputs[o]["mode"] = (w, h, refresh)

    def _name(self, o, p):
        self.outputs[o]["name"] = read_wl_str(p, 0)[0]

    # One frame at a time: request -> buffer (+ buffer_done) -> copy -> ready; the next request goes out the moment
    # a frame is ready, into the other buffer, before this one is sampled.
    def _request(self, damage):
        fid = self.wl.new_id("zwlr_screencopy_frame_v1")
        self.frame = {"id": fid, "damage_wanted": damage and self.manager_version >= 2, "spec": None, "flags": 0,
                      "rects": [], "copied": False}
        if self.region:
            self.wl.send(wl_msg(self.manager, 1, struct.pack("<IiIiiii", fid, 0, self.output, *self.region)))  # capture_output_region
        else:
            self.wl.send(wl_msg(self.manager, 0, struct.pack("<IiI", fid, 0, self.output)))  # capture_output
        if self.manager_version < 3:
            pass  # buffer is the last event before the copy in v1/v2: copied from _buffer

    def _buffer(self, o, p):
        fr = self.frame
        if not fr or fr["id"] != o:
            return
        fr["spec"] = struct.unpack_from("<IIII", p)  # format, width, height, stride
        if self.manager_version < 3:
            self._copy(fr)

    def _buffer_done(self, o, _p):
        fr = self.frame
        if fr and fr["id"] == o:
            self._copy(fr)

    def _copy(self, fr):
        if fr["copied"]:
            return
        if not fr["spec"]:
            raise Fail("the compositor offers no shm buffer for screencopy")
        self._ensure_buffers(fr["spec"])
        fr["buf"] = self.buffers[self.frames % 2]
        fr["copied"] = True
        self.wl.send(wl_msg(fr["id"], 2 if fr["damage_wanted"] else 0, struct.pack("<I", fr["buf"]["id"])))

    def _flags(self, o, p):
        if self.frame and self.frame["id"] == o:
            self.frame["flags"] = struct.unpack_from("<I", p)[0]

    def _damage(self, o, p):
        if self.frame and self.frame["id"] == o:
            self.frame["rects"].append(struct.unpack_from("<IIII", p))

    def _failed(self, o, _p):
        self.wl.send(wl_msg(o, 1))  # destroy
        raise Fail("screencopy failed (the output went away?)")

    def _ready(self, o, p):
        fr = self.frame
        if not fr or fr["id"] != o:
            return
        hi, lo, ns = struct.unpack_from("<III", p)
        t_ready = ((hi << 32) | lo) * 1_000_000_000 + ns
        t_recv = mono_ns()
        self.wl.send(wl_msg(o, 1))  # destroy
        self.frames += 1
        self.frame = None
        if not self.paused:
            self._request(damage=True)  # the next one while this one is sampled
        self._write_frame(t_ready, t_recv, fr)

    def _ensure_buffers(self, spec):
        if self.buffers:
            if self.buffers[0]["spec"] != spec:
                raise Fail(f"the output changed size or format: {self.buffers[0]['spec']} -> {spec}")
            return
        fmt, w, h, stride = spec
        size = stride * h
        fd = os.memfd_create("pocket-compare", 0)
        os.ftruncate(fd, size * 2)
        self.mm = mmap.mmap(fd, size * 2)
        pool = self.wl.new_id("wl_shm_pool")
        self.wl.send(wl_msg(self.shm, 0, struct.pack("<Ii", pool, size * 2)), fds=[fd])
        os.close(fd)
        for i in range(2):
            bid = self.wl.new_id("wl_buffer")
            self.wl.send(wl_msg(pool, 0, struct.pack("<IiiiiI", bid, i * size, w, h, stride, fmt)))
            self.buffers.append({"id": bid, "off": i * size, "spec": spec})
        self.sw = (w + self.step - 1) // self.step
        self.sh = (h + self.step - 1) // self.step
        if not self.header_written:
            o = self.outputs[self.output]
            head = {"format": fmt, "width": w, "height": h, "stride": stride, "step": self.step, "sw": self.sw, "region": self.region,
                    "sh": self.sh, "output": o["name"], "mode": o["mode"], "clock": "CLOCK_MONOTONIC ns",
                    "manager_version": self.manager_version, "started_ns": mono_ns()}
            self.out.write(b"PFR1\n" + json.dumps(head).encode() + b"\n")
            self.header_written = True
            self._mark(2)  # running (a recorder started again after a stop: see `recorded`)

    def _sample(self, buf):
        fmt, w, h, stride = buf["spec"]
        mv = memoryview(self.mm)
        rows = []
        for y in range(0, h, self.step):
            off = buf["off"] + y * stride
            rows.append(mv[off:off + w * 4].cast("I")[::self.step].tobytes())
        return b"".join(rows)

    def _write_frame(self, t_ready, t_recv, fr):
        px = zlib.compress(self._sample(fr["buf"]), 1)
        rects = fr["rects"][:64]
        rec = struct.pack("<4sQQIH", b"FRM1", t_ready, t_recv, fr["flags"], len(rects))
        rec += b"".join(struct.pack("<4H", *(min(v, 65535) for v in r)) for r in rects)
        rec += struct.pack("<I", len(px)) + px
        self.out.write(rec)
        self.out.flush()

    def _mark(self, what):
        if not self.header_written:
            return
        self.out.write(struct.pack("<4sQQIH", b"MARK", mono_ns(), 0, what, 0) + struct.pack("<I", 0))
        self.out.flush()

    def run(self, secs=None):
        end = time.monotonic() + secs if secs else None
        self._request(damage=False)  # the screen as it is now, also when nothing moves
        while not self.stop and (end is None or time.monotonic() < end):
            if self.want_pause:
                self.want_pause = False
                self.paused = True
                self._mark(1)
            if self.want_resume:
                self.want_resume = False
                self.paused = False
                self._mark(2)
                if self.frame is None:
                    self._request(damage=False)
            self.wl.dispatch(0.05)
        self._mark(1)
        self.out.close()


def cmd_record(a):
    rec = Recorder(a.out, step=a.step, output_name=a.output, region=tuple(int(v) for v in a.region.split(",")) if a.region else None)

    def pause(*_):
        rec.want_pause = True

    def resume(*_):
        rec.want_resume = True

    def stop(*_):
        rec.stop = True

    signal.signal(signal.SIGUSR1, pause)
    signal.signal(signal.SIGUSR2, resume)
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    if a.ready:
        open(a.ready, "w").write(str(os.getpid()))
    rec.run(a.secs)
    return 0


# --- reading recordings -------------------------------------------------------------------------------------------

class Frame:
    __slots__ = ("t", "recv", "flags", "rects", "z", "_px")

    def __init__(self, t, recv, flags, rects, z):
        self.t, self.recv, self.flags, self.rects, self.z = t, recv, flags, rects, z
        self._px = None

    @property
    def px(self):
        if self._px is None:
            self._px = zlib.decompress(self.z)
        return self._px


class Recording:
    """A recording file, read whole or followed while it grows (`poll`). Frames sorted by their ready time; `marks`
    are (ns, 1 paused | 2 resumed)."""

    def __init__(self, path):
        self.path = path
        self.head = None
        self.frames = []
        self.marks = []
        self.pos = 0
        self.buf = b""
        self.poll()

    def poll(self):
        try:
            with open(self.path, "rb") as f:
                f.seek(self.pos)
                data = f.read()
                self.pos = f.tell()
        except FileNotFoundError:
            return 0
        self.buf += data
        n = 0
        if self.head is None:
            nl = self.buf.find(b"\n", 5)
            if not self.buf.startswith(b"PFR1\n") or nl < 0:
                return 0
            self.head = json.loads(self.buf[5:nl])
            self.buf = self.buf[nl + 1:]
        b = self.buf
        off = 0
        while len(b) - off >= 26:
            tag, t, recv, flags, nr = struct.unpack_from("<4sQQIH", b, off)
            need = 26 + nr * 8 + 4
            if len(b) - off < need:
                break
            rects = [struct.unpack_from("<4H", b, off + 26 + i * 8) for i in range(nr)]
            ln = struct.unpack_from("<I", b, off + 26 + nr * 8)[0]
            if len(b) - off < need + ln:
                break  # the last record is still being written (or the recorder was killed in it)
            z = b[off + need:off + need + ln]
            off += need + ln
            if tag == b"FRM1":
                self.frames.append(Frame(t, recv, flags, rects, z))
                n += 1
            elif tag == b"MARK":
                self.marks.append((t, flags))
        self.buf = b[off:]
        if n and len(self.frames) > n and self.frames[-n - 1].t > self.frames[-n].t:
            self.frames.sort(key=lambda f: f.t)
        return n

    def at(self, t):
        """The last frame at or before t (the screen at that moment), or None."""
        lo, hi = 0, len(self.frames)
        while lo < hi:
            mid = (lo + hi) // 2
            if self.frames[mid].t <= t:
                lo = mid + 1
            else:
                hi = mid
        return self.frames[lo - 1] if lo else None

    def between(self, t0, t1):
        return [f for f in self.frames if t0 < f.t <= t1]


# --- comparing pictures -------------------------------------------------------------------------------------------
# Pixels are compared as Python integers (one bit per pixel), so the standard library is enough and it is fast: a
# picture of 155x135 pixels is one 84 KB integer.

def _table(fn):
    return bytes(fn(v) for v in range(256))


Q16 = _table(lambda v: v >> 4)  # 16 levels per channel: what counts as "the same colour"
NONBLACK = _table(lambda v: 1 if v >= 40 else 0)


class Pics:
    def __init__(self, sw, sh, fmt):
        self.n = sw * sh
        self.sw, self.sh = sw, sh
        # Bit 0 of every pixel's first byte: one bit per pixel.
        self.ones = int.from_bytes(b"\x01\x00\x00\x00" * self.n, "little")
        if fmt in X_LAST:
            ch = 0x000F0F0F
        elif fmt in X_FIRST:
            ch = 0x0F0F0F00
        else:
            ch = 0x0F0F0F0F
        self.chmask = self.ones * ch  # the colour channels' bytes, no padding byte
        self.all = self.ones
        self.cache = {}

    def q(self, fr):
        key = id(fr)
        v = self.cache.get(key)
        if v is None or v[0] is not fr:
            v = (fr, int.from_bytes(fr.px.translate(Q16), "little") & self.chmask)
            if len(self.cache) > 128:
                self.cache.clear()
            self.cache[key] = v
        return v[1]

    def fold(self, x):
        """Nibble-wise differences -> one bit per pixel (bit 0 of its first byte)."""
        x |= x >> 2
        x |= x >> 1
        x &= self.ones * 0x01010101
        x |= x >> 8
        x |= x >> 16
        return x & self.ones

    def diff(self, a, b):
        """Pixels (bits) where a and b differ by a colour level or more."""
        return self.fold(self.q(a) ^ self.q(b))

    def nonblack(self, fr):
        v = int.from_bytes(fr.px.translate(NONBLACK), "little") & (self.chmask & (self.ones * 0x01010101))
        v |= v >> 8
        v |= v >> 16
        return v & self.ones

    @staticmethod
    def count(bits):
        return bin(bits).count("1") if not hasattr(int, "bit_count") else bits.bit_count()

    def look(self, ignore):
        """All pixels but the plan's `ignore` rectangles ([x0, y0, x1, y1], fractions of the recorded picture as
        `thumbs` shows it: a status bar with a clock, say)."""
        m = self.all
        for r in ignore or ():
            m &= ~self.rect(*r)
        return m

    def rect(self, x0, y0, x1, y1):
        """Bits of the pixels in a rectangle (fractions 0..1 of the recorded picture)."""
        m = 0
        row = 0
        c0, c1 = int(x0 * self.sw), max(int(x0 * self.sw) + 1, int(x1 * self.sw))
        for c in range(c0, min(c1, self.sw)):
            row |= 1 << (c * 32)
        for r in range(int(y0 * self.sh), min(self.sh, max(int(y0 * self.sh) + 1, int(y1 * self.sh)))):
            m |= row << (r * self.sw * 32)
        return m


# Thresholds (fractions of the pixels looked at). Chosen with the self-test and the first device runs; the JSON of
# every run keeps them, so the numbers can be computed again with others.
CHANGE_FRAC = 0.003   # "the screen changed": more than 0.3 % of the pixels outside what was moving anyway
MATCH_FRAC = 0.01     # "shows that screen" (the start screen): at most 1 % of the compared pixels differ from it
DONE_FRAC = 0.10      # "done": at most 10 % of the step's change still to come (a notice over the game doesn't hold it)
GAME_DONE_FRAC = 0.5  # for steps that end in a running game: its picture is there (the game moves on by itself)
MOTION_MS = 1000      # what moved in this long before the press does not count as a reaction
TAIL_MS = 300         # the end state: the last frames of the step's window


def react(rec, pics, t_press, t_end, roi=None, motion_ms=MOTION_MS):
    """First frame after the press that differs from the screen at the press, outside the pixels that were moving
    anyway in `motion_ms` before it (a running game, an animated background). (frame, how) or (None, why).
    motion_ms=0 for the presses right after a start or a wake, when the screen has only just come."""
    ref = rec.at(t_press)
    if ref is None:
        return None, "no frame before the press"
    moving = 0
    prev = None
    for f in (rec.between(t_press - motion_ms * 1_000_000, t_press) if motion_ms else ()):
        if prev is not None:
            moving |= pics.diff(prev, f)
        prev = f
    look = (roi if roi is not None else pics.all) & ~moving
    n = pics.count(look)
    if n < pics.n * 0.02:
        return None, "almost everything was moving before the press"
    for f in rec.between(t_press, t_end):
        if pics.count(pics.diff(ref, f) & look) > CHANGE_FRAC * n:
            return f, "changed"
    return None, "no change"


def end_state(rec, pics, t_press, t_end, roi=None):
    """The step's end state: the last frame of its window, the pixels that stay still in its last TAIL_MS (a game
    keeps moving: only its still parts, the borders or the frame around it, count), and of those the ones that
    differ from the screen at the press and are not black, when there are enough of them."""
    last = rec.at(t_end)
    ref = rec.at(t_press)
    if last is None or ref is None:
        return None, 0, "no frames"
    still = pics.all if roi is None else roi
    prev = rec.at(t_end - TAIL_MS * 1_000_000)
    for f in rec.between(t_end - TAIL_MS * 1_000_000, t_end):
        if prev is not None:
            still &= ~pics.diff(prev, f)
        prev = f
    changed = still & pics.diff(ref, last)
    lit = changed & pics.nonblack(last)
    floor = pics.n * 0.002
    for mask, how in ((lit, "changed+lit"), (changed, "changed"), (still, "still")):
        if pics.count(mask) >= floor:
            return last, mask, how
    return last, still, "still (few)"


def done(rec, pics, t_press, t_end, roi=None, frac=DONE_FRAC):
    """First frame from which on the screen stays within DONE_FRAC of the step's change from its end state: 90 % of
    the change is on the screen, in that frame and every later one of the window. A slide or a fade ends at its 90 %
    point; a small notice over a game that started (RetroArch's "state loaded") doesn't hold it up."""
    last, mask, how = end_state(rec, pics, t_press, t_end, roi)
    if last is None:
        return None, how
    n = max(1, pics.count(mask))
    frames = rec.between(t_press, t_end)
    if not frames:
        return None, "no frame after the press"
    total = pics.count(pics.diff(rec.at(t_press), last) & mask)
    if total <= MATCH_FRAC * n:
        return None, "nothing changed"
    tol = max(MATCH_FRAC * n, frac * total)
    first = None
    for f in reversed(frames):
        if pics.count(pics.diff(f, last) & mask) <= tol:
            first = f
        else:
            break
    if first is None:
        return None, "the end state was not reached"
    return first, how


def matches(pics, f, ref, mask):
    n = max(1, pics.count(mask))
    return pics.count(pics.diff(f, ref) & mask) <= MATCH_FRAC * n


# --- device side: the UIs, the scenarios ---------------------------------------------------------------------------

ES_ONLY = os.environ.get("COMPARE_ROOT", "") + "/tmp/pocket-ui.es-only"
# The full-size recorder for the showcase video, and the speaker's monitor for its sound (the speaker itself is muted).
WF_RECORDER = os.environ.get("COMPARE_WF_RECORDER", "/storage/rec-tools/rec/wf-recorder")
SPEAKER_MONITOR = "alsa_output._sys_devices_platform_sound_sound_card0.HiFi__Speaker__sink.monitor"
UI_PROC = {"es": "emulationstation", "pocket-ui": "pocket-ui"}
GAME_PROC = {"es": "retroarch", "pocket-ui": "pocket-core"}


def pids_of(name):
    try:
        out = subprocess.run(["pidof", name], capture_output=True, text=True).stdout
    except FileNotFoundError:
        return []
    return [int(p) for p in out.split()]


def cmdline(pid):
    try:
        return open(f"/proc/{pid}/cmdline", "rb").read().replace(b"\0", b" ").decode(errors="replace")
    except OSError:
        return ""


def cpu_ns(pid):
    """CPU time of the process (all its threads) in ns, or None when it is gone: utime + stime of /proc/<pid>/stat,
    in clock ticks (10 ms). Not schedstat: the Mini V2's kernel has no CONFIG_SCHEDSTATS, its schedstat files are not
    there (scripts/latency.py read 0 % from them)."""
    try:
        with open(f"/proc/{pid}/stat") as f:
            fields = f.read().rsplit(")", 1)[1].split()
    except (OSError, IndexError):
        return None
    hz = os.sysconf("SC_CLK_TCK")
    return (int(fields[11]) + int(fields[12])) * 1_000_000_000 // hz


def proc_stat():
    """(busy, total) jiffies of all CPUs."""
    with open("/proc/stat") as f:
        v = [int(x) for x in f.readline().split()[1:]]
    idle = v[3] + (v[4] if len(v) > 4 else 0)
    return sum(v) - idle, sum(v)


def read(path, default=None):
    try:
        with open(path) as f:
            return f.read().strip()
    except OSError:
        return default


def battery_dir():
    base = "/sys/class/power_supply"
    try:
        for d in sorted(os.listdir(base)):
            if read(f"{base}/{d}/type") == "Battery":
                return f"{base}/{d}"
    except OSError:
        pass
    return None


def backlight_dir():
    base = "/sys/class/backlight"
    try:
        for d in sorted(os.listdir(base)):
            return f"{base}/{d}"
    except OSError:
        pass
    return None


def sh(cmd, timeout=30):
    return subprocess.run(["sh", "-c", cmd], capture_output=True, text=True, timeout=timeout)


class Driver:
    """Runs the scenarios on one UI. Every press, mark and number goes into events.jsonl; the frames into frames.pfr."""

    def __init__(self, a):
        self.a = a
        self.ui = a.ui
        self.plan = json.load(open(a.plan))
        self.p = self.plan[self.ui]
        os.makedirs(a.out, exist_ok=True)
        self.events = open(os.path.join(a.out, "events.jsonl"), "a")
        codes = self.plan.get("source_codes") or {}
        if a.codes:
            codes.update(json.load(open(a.codes)))
        self.pad = Pad(codes, inject=a.inject)
        self.rec_path = os.path.join(a.out, "frames.pfr")
        self.rec_proc = None
        self.rec = None
        self.paused = False
        self.blind = False  # CPU and power: no recorder (its own work would be in the numbers)
        self.pics = None
        self.hold = a.hold
        self.where = "start"
        self.visited = set()
        self.nav_gap_s = 0.35  # between unmeasured presses

    # -- bookkeeping
    def ev(self, kind, **kw):
        kw.update(kind=kind, ui=self.ui, t=kw.get("t", mono_ns()))
        self.events.write(json.dumps(kw) + "\n")
        self.events.flush()
        return kw

    # -- the recorder
    def rec_start(self):
        if self.rec_proc and self.rec_proc.poll() is None:
            return
        path = self.rec_path
        if os.path.exists(path):
            # A recorder that died: a new file next to it, the analysis reads all.
            k = 1
            while os.path.exists(f"{self.rec_path[:-4]}.{k}.pfr"):
                k += 1
            path = f"{self.rec_path[:-4]}.{k}.pfr"
        region = self.p.get("region", self.a.region)
        self.rec_proc = subprocess.Popen([sys.executable, os.path.abspath(__file__), "record", "--out", path, "--step", str(self.a.step)]
                                         + (["--region", region] if region else []),
                                         stdout=subprocess.DEVNULL, stderr=open(os.path.join(self.a.out, "recorder.log"), "a"))
        self.rec = Recording(path)
        self.paused = False
        end = time.monotonic() + 5
        while not self.rec.frames:
            if self.rec_proc.poll() is not None or time.monotonic() > end:
                raise Fail("the recorder did not start (recorder.log)")
            time.sleep(0.02)
            self.rec.poll()
        h = self.rec.head
        self.pics = Pics(h["sw"], h["sh"], h["format"])
        self.ev("recorder", path=os.path.basename(path), head=h)

    def rec_pause(self):
        if self.rec_proc and self.rec_proc.poll() is None and not self.paused:
            self.rec_proc.send_signal(signal.SIGUSR1)
            self.paused = True
            self.ev("rec-pause")

    def rec_resume(self):
        """The recorder running, with a frame of the screen as it is now."""
        if not self.rec_proc or self.rec_proc.poll() is not None:
            if self.rec_proc:
                self.ev("recorder-died", rc=self.rec_proc.returncode)
            self.rec_start()
            return
        if not self.paused:
            return
        self.paused = False
        n = len(self.rec.frames)
        self.rec_proc.send_signal(signal.SIGUSR2)
        self.ev("rec-resume")
        end = time.monotonic() + 3
        while len(self.rec.frames) == n and time.monotonic() < end:
            time.sleep(0.01)
            self.rec.poll()

    def rec_stop(self):
        if self.rec_proc and self.rec_proc.poll() is None:
            self.rec_proc.terminate()
            try:
                self.rec_proc.wait(5)
            except subprocess.TimeoutExpired:
                self.rec_proc.kill()
        self.rec_proc = None
        self.paused = False

    # -- waiting on the screen (the same comparisons the analysis makes, on the frames so far)
    def poll(self):
        if self.rec:
            self.rec.poll()
            # Online only the last seconds matter (the analysis reads the file): keep the memory of a long run small.
            fr = self.rec.frames
            if len(fr) > 2000:
                cut = mono_ns() - 20_000_000_000
                keep = next((i for i, f in enumerate(fr) if f.t >= cut), len(fr))
                del fr[:max(0, keep - 1)]  # and the frame before: the screen at `cut`

    def wait_change(self, t, timeout):
        """Until the screen differs from what it was at t (nothing masked). Returns the frame or None."""
        end = time.monotonic() + timeout
        self.poll()
        ref = self.rec.at(t)
        while time.monotonic() < end:
            self.poll()
            for f in self.rec.between(t, mono_ns()):
                if ref is None or self.pics.count(self.pics.diff(ref, f)) > CHANGE_FRAC * self.pics.n:
                    return f
            time.sleep(0.01)
        return None

    def wait_quiet(self, quiet_ms=600, timeout=8.0):
        """Until nothing changed for quiet_ms (a game never is: then after `timeout`). True if quiet."""
        if self.blind:
            time.sleep(min(timeout, 2 * quiet_ms / 1000.0))
            return True
        if self.rec_proc is None or self.paused:
            self.rec_resume()  # a paused recorder sees no change
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            self.poll()
            last = self.rec.frames[-1] if self.rec.frames else None
            now = mono_ns()
            if last is None or now - last.t >= quiet_ms * 1_000_000:
                return True
            # Frames that change nothing visible (a cursor, a clock elsewhere) still count as quiet.
            recent = self.rec.between(now - quiet_ms * 1_000_000, now)
            base = self.rec.at(now - quiet_ms * 1_000_000)
            if base and all(self.pics.count(self.pics.diff(base, f)) <= CHANGE_FRAC * self.pics.n for f in recent):
                return True
            time.sleep(0.05)
        return False

    # -- input
    def seq(self, seq, gap=None):
        """Presses that are not measured (getting somewhere)."""
        gap = self.nav_gap_s if gap is None else gap
        for item in parse_seq(seq):
            if item[0] == "wait":
                time.sleep(item[1] / 1000.0)
                continue
            mod, btn, hold = item
            td, tu = self.pad.tap(btn, hold, mod)
            self.ev("nav", btn=btn, mod=mod, t=td, t_up=tu)
            time.sleep(gap)

    def measure(self, name, gesture, window, rep=0, **kw):
        """One measured press: the recorder runs from MOTION_MS before it to the end of `window` seconds after it."""
        self.mute()
        self.rec_resume()
        time.sleep(MOTION_MS / 1000.0 + 0.1)
        items = parse_seq(gesture)
        for it in items[:-1]:
            if it[0] == "wait":
                time.sleep(it[1] / 1000.0)
            else:
                td, tu = self.pad.tap(it[1], it[2], it[0])
                self.ev("nav", btn=it[1], mod=it[0], t=td, t_up=tu)
                time.sleep(0.3)
        mod, btn, hold = items[-1]
        td, tu = self.pad.tap(btn, hold, mod)
        arrival = self.pad.arrival(btn, td - 1_000_000) if self.pad.out else None
        e = self.ev("press", name=name, btn=btn, mod=mod, t=td, t_up=tu, rep=rep, window_ms=int(window * 1000),
                    inputplumber_ms=(arrival - td) / 1e6 if arrival else None, **kw)
        time.sleep(window)
        self.ev("window-end", name=name, rep=rep, press=td)
        return e

    def mute(self):
        sh("pactl set-sink-mute @DEFAULT_SINK@ 1 2>/dev/null")

    # -- the UI's service
    def ui_pid(self):
        p = pids_of(UI_PROC[self.ui])
        return p[0] if p else None

    def game_pid(self):
        p = pids_of(GAME_PROC[self.ui])
        return p[0] if p else None

    def stop_ui(self):
        sh("systemctl stop essway", timeout=60)
        end = time.monotonic() + 10
        while (pids_of("emulationstation") or pids_of("pocket-ui")) and time.monotonic() < end:
            time.sleep(0.05)
        for name in ("retroarch", "pocket-core"):
            for p in pids_of(name):
                try:
                    os.kill(p, signal.SIGTERM)
                except OSError:
                    pass

    def start_ui(self):
        """essway with the mark (EmulationStation) or without (pocket-ui); returns the time of the start."""
        if self.ui == "es":
            open(ES_ONLY, "w").close()
        elif os.path.exists(ES_ONLY):
            os.unlink(ES_ONLY)
        sh("systemctl reset-failed essway 2>/dev/null")
        self.where = "start"
        self.visited = set()
        t0 = mono_ns()
        subprocess.run(["systemctl", "start", "--no-block", "essway"])
        return t0

    # -- scenarios
    # Where the UI is, so that the plan's relative moves work: "start" (the start screen, the cursor on its first
    # entry), ("list", system) (that system's game list, the cursor on the test game) or ("game", system). Both UIs
    # remember the cursor in a list until they are started again: `visited` are the lists entered since.
    def home(self):
        """Back to the start screen with the cursor on its first entry (the plan's way), settled."""
        if self.where != "start":
            kind, system = self.where
            if kind == "game":
                self.seq(self.p["quit"])
                self.wait_game_gone()
            self.seq(self.p["from_system"][system])
            self.where = "start"
        self.wait_quiet(800, 6)

    def to_game(self, system):
        """Start screen -> the system's list with the cursor on the test game (not measured)."""
        if self.where == ("list", system):
            return
        self.home()
        self.seq(self.p["to_system"][system])
        self.wait_quiet(600, 4)
        # pocket-ui opens a list at its first game every time; EmulationStation keeps the cursor until it restarts.
        if system not in self.visited or not self.p.get("remembers_cursor", True):
            self.seq(self.p.get("to_game_first", {}).get(system, ""))
            self.visited.add(system)
        self.where = ("list", system)
        self.wait_quiet(800, 6)

    def wait_game_gone(self, secs=15):
        end = time.monotonic() + secs
        while self.game_pid() and time.monotonic() < end:
            time.sleep(0.05)

    def sc_start(self):
        """Cold start: essway started -> the start screen, and -> the first press it acts on."""
        reps = self.a.reps_start
        ref = None
        for rep in range(-1, reps):
            self.stop_ui()
            time.sleep(1.5)
            self.rec_resume()
            time.sleep(0.3)
            t0 = self.start_ui()
            self.ev("service-start", rep=rep, t=t0)
            proc_seen = None
            end = time.monotonic() + 40
            listed = None
            while time.monotonic() < end:
                if proc_seen is None and self.ui_pid():
                    proc_seen = mono_ns()
                    self.ev("ui-process", rep=rep, t=proc_seen)
                self.poll()
                if ref is not None:
                    for f in self.rec.between(t0, mono_ns()):
                        if matches(self.pics, f, ref[0], ref[1]):
                            listed = f
                            break
                    if listed:
                        break
                elif proc_seen and self.wait_quiet(1500, 0.01) and mono_ns() - t0 > 3e9:
                    break
                time.sleep(0.005)
            if ref is None:
                # The warm-up start: its settled screen is the start screen of the others.
                self.wait_quiet(2000, 20)
                f = self.rec.at(mono_ns())
                now = mono_ns()
                last, mask, how = end_state(self.rec, self.pics, t0, now, self.pics.look(self.p.get("ignore")))
                ref = (f, mask)
                self.ev("start-screen", rep=rep, frame_t=f.t, how=how, mask_from=t0, mask_to=now)
                continue
            if not listed:
                self.ev("start-timeout", rep=rep)
                continue
            # Accepts input: press `move` until the screen reacts (each press measured on its own).
            got = None
            for k in range(20):
                td, tu = self.pad.tap(self.p["move"], self.hold)
                self.ev("press", name="start.input_try", btn=self.p["move"], t=td, t_up=tu, rep=rep, k=k, start=t0)
                f = self.wait_change(td, 0.25)
                if f:
                    got = (td, f)
                    break
            self.ev("start-input", rep=rep, start=t0, press=got[0] if got else None, frame_t=got[1].t if got else None)
            time.sleep(0.6)
            self.seq(self.p["move_back"])
            self.wait_quiet(800, 5)
        self.rec_pause()

    def sc_nav(self):
        """Moves in a game list (down and up again, so the cursor ends where it was)."""
        system = self.p.get("list_system", self.a.systems[0])
        self.to_game(system)
        moves = [self.p["list_move"], self.p["list_back"]]
        for i in range(self.a.moves):
            self.measure("nav", moves[i % 2], self.a.nav_gap, rep=i)
        if self.a.moves % 2:
            self.seq(self.p["list_back"])
        self.rec_pause()

    def check_game(self, system):
        """The game that runs is the one of the plan (its process and its command line)."""
        part = self.plan["games"][system]
        end = time.monotonic() + 20
        while time.monotonic() < end:
            for pid in pids_of(GAME_PROC[self.ui]):
                if part.lower() in cmdline(pid).lower():
                    return pid
            time.sleep(0.1)
        raise Fail(f"{self.ui}: no {GAME_PROC[self.ui]} with '{part}' in its command line after the launch of {system} (check the plan's sequence)")

    def launch(self, system, rep=None, window=None):
        """From the game's list: A (measured when rep is given), and check that the plan's game runs."""
        gesture = self.p.get("launch", "A")
        if rep is None:
            self.seq(gesture)
        else:
            self.measure("launch", gesture, window or self.a.launch_window, rep=rep, system=system)
        self.where = ("game", system)
        return self.check_game(system)

    def quit_game(self, rep, system, measured=True):
        """From the running game back to its list (the plan's `quit`: RetroArch's hotkey, pocket-ui's pause menu)."""
        q = self.p["quit"]
        if measured:
            self.measure("quit", q, self.a.window, rep=rep, system=system)
        else:
            self.seq(q)
        self.wait_game_gone()
        self.where = ("list", system)
        self.wait_quiet(800, 6)

    def sc_launch(self):
        for system in self.a.systems:
            for rep in range(-1, self.a.reps):  # -1: warm-up, not counted (writes the resume states, fills caches)
                self.to_game(system)
                self.launch(system, rep)
                time.sleep(1.0)
                self.measure("pause", self.p["pause"], self.a.window, rep=rep, system=system)
                self.seq(self.p["pause_close"])
                time.sleep(1.5)
                self.quit_game(rep, system)
                self.rec_pause()

    def sc_switch(self):
        """From the first game to the second: every press of the UI's way, each measured to its end state."""
        s1, s2 = self.a.systems[0], self.a.systems[1]
        for rep in range(self.a.reps):
            # The second game played just before the first: pocket-ui's switcher offers the game played before
            # (and both lists are visited, so EmulationStation's way finds the cursor on the game).
            self.to_game(s2)
            self.launch(s2)
            time.sleep(2)
            self.quit_game(0, s2, measured=False)
            self.to_game(s1)
            self.launch(s1)
            time.sleep(3)
            steps = self.p["switch"][f"{s1}>{s2}"]
            for k, step in enumerate(steps):
                last = k == len(steps) - 1
                self.measure("switch", step, self.a.launch_window if last else self.a.window, rep=rep, k=k, steps=len(steps))
                if not last:
                    self.wait_quiet(500, 2.5)
            self.where = ("game", s2)
            self.check_game(s2)
            time.sleep(1.5)
            self.quit_game(rep, s2, measured=False)
            self.rec_pause()
            # Where a quit after a switch lands differs between the UIs: start again from a known screen.
            self.stop_ui()
            self.start_ui()
            time.sleep(3)
            self.wait_quiet(1500, 20)

    def sc_wake(self):
        """Deep sleep: the power key (written into its input device), the RTC alarm wakes it; then the screen and the
        pad. Every suspend has its alarm before the key goes down."""
        node, name = power_key_node()
        if not node:
            self.ev("wake-skipped", why="no power key input device")
            return
        succ = "/sys/power/suspend_stats/success"
        bl = backlight_dir()
        self.home()
        fd = os.open(node, os.O_WRONLY)
        try:
            for rep in range(self.a.reps):
                self.rec_resume()
                time.sleep(1.0)
                before = int(read(succ, "0"))
                arm_alarm(self.a.asleep)
                t = mono_ns()
                os.write(fd, EVENT.pack(0, 0, EV_KEY, KEY_POWER, 1) + EVENT.pack(0, 0, EV_SYN, 0, 0))
                time.sleep(0.1)
                os.write(fd, EVENT.pack(0, 0, EV_KEY, KEY_POWER, 0) + EVENT.pack(0, 0, EV_SYN, 0, 0))
                self.ev("power-key", rep=rep, t=t, node=node, device=name)
                # Asleep and back: the kernel's counter goes up when the system is back (this process thaws too).
                back = wait_resume(succ, before, 6)
                how = "power key"
                if back is None:
                    # The key was not acted on (a reader that grabs it?). The UI's own way to the same suspend; the
                    # alarm first, again, so that no suspend can happen without it.
                    arm_alarm(self.a.asleep)
                    how = "fallback"
                    if self.ui == "pocket-ui":
                        sh("timeout 2 sh -c 'echo power > /tmp/pocket-ui.in'")
                    else:
                        subprocess.Popen(["systemctl", "suspend"])
                    self.ev("power-fallback", rep=rep, t=mono_ns())
                    back = wait_resume(succ, before, self.a.asleep + 40)
                if back is None:
                    self.ev("wake-none", rep=rep, why="no suspend (the alarm runs out harmlessly)")
                    time.sleep(self.a.asleep)
                    continue
                self.ev("resumed", rep=rep, t=back, how=how)
                screen_on = None
                if bl:
                    e2 = time.monotonic() + 5
                    while time.monotonic() < e2:
                        if read(f"{bl}/brightness", "0") not in ("0", ""):
                            screen_on = mono_ns()
                            break
                        time.sleep(0.005)
                self.ev("screen-on", rep=rep, t=screen_on or 0, ok=screen_on is not None)
                # The pad works: `move` every 100 ms until the screen reacts.
                for k in range(40):
                    td, tu = self.pad.tap(self.p["move"], 60)
                    self.ev("press", name="wake.input_try", btn=self.p["move"], t=td, t_up=tu, rep=rep, k=k, resumed=back)
                    if self.wait_change(td, 0.1):
                        break
                time.sleep(1.0)
                self.seq(self.p["move_back"])
                self.wait_quiet(800, 5)
                self.rec_pause()
                time.sleep(2)
        finally:
            os.close(fd)

    def cpu_window(self, name, secs, **kw):
        ui, game = self.ui_pid(), self.game_pid()
        a_ui, a_game = cpu_ns(ui) if ui else None, cpu_ns(game) if game else None
        s0, t0 = proc_stat(), time.monotonic()
        time.sleep(secs)
        s1, t1 = proc_stat(), time.monotonic()
        b_ui, b_game = cpu_ns(ui) if ui else None, cpu_ns(game) if game else None
        pct = lambda a, b: (b - a) / 1e9 / (t1 - t0) * 100.0 if a is not None and b is not None else None
        ncpu = os.cpu_count() or 1
        sys_pct = (s1[0] - s0[0]) / max(1, s1[1] - s0[1]) * 100.0 * ncpu
        return self.ev("cpu", name=name, secs=round(t1 - t0, 2), ui_pct=pct(a_ui, b_ui), game_pct=pct(a_game, b_game),
                       system_pct=sys_pct, ui_pid=ui, game_pid=game, **kw)

    def sc_cpu(self):
        self.rec_stop()
        self.blind = True
        self.home()
        time.sleep(3)
        for rep in range(self.a.reps):
            self.cpu_window("cpu.list_idle", self.a.cpu_secs, rep=rep)
        s = self.a.systems[0]
        self.to_game(s)
        self.launch(s)
        time.sleep(8)
        for rep in range(self.a.reps):
            self.cpu_window("cpu.behind_game", self.a.cpu_secs, rep=rep, system=s)
        self.quit_game(0, s, measured=False)

    def sc_play(self):
        """Each game for --play-secs from its start (resume states set aside, so both start from power-on and show
        the same title and attract sequence), no input, screen on at the brightness of the run's start."""
        self.rec_stop()
        self.blind = True
        bat = battery_dir()
        bl = backlight_dir()
        level = self.plan.get("brightness") or (read(f"{bl}/brightness") if bl else None)
        for s in self.a.systems:
            self.set_aside_states(s)
            self.to_game(s)
            time.sleep(1.5)
            if bl and level:
                sh(f"echo {level} > {bl}/brightness")
            pid = self.launch(s)
            time.sleep(self.a.play_boot)
            samples = []
            g0, u0 = cpu_ns(pid), cpu_ns(self.ui_pid()) if self.ui_pid() else None
            s0, t0 = proc_stat(), time.monotonic()
            end = t0 + self.a.play_secs
            while time.monotonic() < end:
                time.sleep(min(5, max(0.1, end - time.monotonic())))
                smp = {"t": round(time.monotonic() - t0, 1)}
                if bat:
                    for k in ("status", "capacity", "current_now", "voltage_now", "power_now"):
                        v = read(f"{bat}/{k}")
                        if v is not None:
                            smp[k] = v
                smp["temp_mC"] = max_temp()
                smp["brightness"] = read(f"{bl}/brightness") if bl else None
                smp["governor"] = read("/sys/devices/system/cpu/cpufreq/policy0/scaling_governor")
                smp["fan"] = fan_state()
                samples.append(smp)
            s1, t1 = proc_stat(), time.monotonic()
            g1, u1 = cpu_ns(pid), cpu_ns(self.ui_pid()) if self.ui_pid() else None
            ncpu = os.cpu_count() or 1
            self.ev("play", system=s, secs=round(t1 - t0, 1), boot_secs=self.a.play_boot,
                    game_pct=(g1 - g0) / 1e9 / (t1 - t0) * 100 if g0 is not None and g1 is not None else None,
                    ui_pct=(u1 - u0) / 1e9 / (t1 - t0) * 100 if u0 is not None and u1 is not None else None,
                    system_pct=(s1[0] - s0[0]) / max(1, s1[1] - s0[1]) * 100.0 * ncpu, samples=samples, brightness=level)
            self.quit_game(0, s, measured=False)
            # From a fresh start of the UI for the next game: after a long session one press of the way back went
            # astray once (pocket-ui started another GBA game instead of the SNES one, 2026-10-26).
            self.stop_ui()
            self.start_ui()
            time.sleep(8)

    # -- the showcase: the same scripted steps on both UIs, filmed at full size by wf-recorder for the side-by-side video
    # (scripts/rocknix-compare-video.py). The numbers in that video come from the measurement runs, not from this one;
    # the sampling recorder runs alongside only to put the video on the presses' clock (`video_anchor`).
    def wf_start(self, path):
        cmd = [WF_RECORDER, "-D", "-c", "libx264", "-p", "preset=veryfast", "-p", "crf=18", "-r", "60"]
        if self.a.showcase_audio:
            cmd += [f"--audio={SPEAKER_MONITOR}", "-C", "aac"]
        cmd += ["-f", path]
        env = dict(os.environ, LD_LIBRARY_PATH=os.path.dirname(WF_RECORDER) + "/lib")
        t = mono_ns()
        self.wf = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=open(os.path.join(self.a.out, "wf-recorder.log"), "a"))
        self.ev("wf-start", t=t, path=os.path.basename(path), cmd=cmd)
        time.sleep(1.0)
        if self.wf.poll() is not None:
            raise Fail(f"wf-recorder ended at once ({self.wf.returncode}; wf-recorder.log)")

    def wf_stop(self):
        wf = getattr(self, "wf", None)
        if wf and wf.poll() is None:
            wf.send_signal(signal.SIGINT)
            try:
                wf.wait(20)
            except subprocess.TimeoutExpired:
                wf.kill()
            self.ev("wf-stop")
        self.wf = None

    def sc_showcase(self):
        sa, sb = self.a.showcase_systems.split(",")[:2]
        # Warm-up, not filmed: each game started and quit once, so that both resume their own state when filmed, as
        # in the measurements (without it RetroArch booted F-Zero from its logo: no auto save yet, 2026-10-26).
        for system in (sa, sb):
            self.to_game(system)
            self.launch(system)
            time.sleep(3)
            self.quit_game(0, system, measured=False)
        try:
            # From a dark screen: the UI stopped, both recorders on, then the service started.
            self.stop_ui()
            self.rec_stop()
            time.sleep(1.0)
            self.rec_start()
            self.wf_start(os.path.join(self.a.out, "showcase.mp4"))
            time.sleep(1.5)
            t0 = self.start_ui()
            self.ev("press", name="showcase.start", btn="service", t=t0, t_up=t0, rep=0, window_ms=8000)
            self.wait_quiet(1500, 25)
            time.sleep(4.0)
            # Slower than in the measurements: with wf-recorder's load EmulationStation dropped presses that came
            # during a carousel slide (it went to the N64 list instead of GBA's, 2026-10-26).
            self.nav_gap_s = 0.8
            self.mute()
            self.to_game(sa)
            time.sleep(1.0)
            moves = [self.p["list_move"], self.p["list_back"]]
            for i in range(4):
                self.measure("showcase.nav", moves[i % 2], 0.9, rep=i)
            self.measure("showcase.launch", self.p.get("launch", "A"), 5.0, rep=0, system=sa)
            self.where = ("game", sa)
            self.check_game(sa)
            self.measure("showcase.pause", self.p["pause"], 2.5, rep=0, system=sa)
            self.measure("showcase.back", self.p["pause_close"], 2.5, rep=0, system=sa)
            self.measure("showcase.quit", self.p["quit"], 3.0, rep=0, system=sa)
            self.wait_game_gone()
            self.where = ("list", sa)
            self.to_game(sb)
            time.sleep(1.0)
            self.measure("showcase.launch2", self.p.get("launch", "A"), 5.0, rep=0, system=sb)
            self.where = ("game", sb)
            self.check_game(sb)
            self.quit_game(0, sb, measured=False)
            time.sleep(1.0)
        finally:
            self.nav_gap_s = 0.35
            self.wf_stop()

    def set_aside_states(self, system):
        """Resume states of the test game out of the way (the device script has a copy and puts everything back)."""
        part = self.plan["games"][system].lower()
        d = f"/storage/roms/savestates/{system}"
        try:
            names = os.listdir(d)
        except OSError:
            return
        for n in names:
            if part in n.lower() and (".state.auto" in n or ".pstate.auto" in n):
                os.unlink(os.path.join(d, n))
                self.ev("state-set-aside", system=system, file=n)

    def go(self):
        self.ev("run-start", plan=self.plan, args={k: v for k, v in vars(self.a).items() if k != "func"},
                pad_source=self.pad.source, dualsense=self.pad.out is not None, codes=self.pad.codes)
        self.mute()
        self.rec_start()
        self.rec_pause()
        # The UI of this run, started the way the Pocket starts it.
        self.stop_ui()
        self.start_ui()
        time.sleep(3)
        self.rec_resume()
        self.wait_quiet(1500, 30)
        self.rec_pause()
        for sc in self.a.scenarios:
            log(f"{self.ui}: {sc}")
            self.ev("scenario", name=sc)
            self.blind = False
            try:
                getattr(self, "sc_" + sc)()
            except Fail as e:
                log(f"{self.ui}: {sc} failed: {e}")
                self.ev("scenario-failed", name=sc, why=str(e))
                self.recover()
        self.rec_stop()
        self.ev("run-end")

    def recover(self):
        """After a failed scenario: no game, the UI on its start screen (restarted if need be)."""
        for name in (GAME_PROC[self.ui],):
            for p in pids_of(name):
                try:
                    os.kill(p, signal.SIGTERM)
                except OSError:
                    pass
        time.sleep(2)
        self.stop_ui()
        self.start_ui()
        time.sleep(6)


def arm_alarm(secs):
    """The RTC alarm `secs` from now; Fail if the kernel did not take it (no suspend may happen without one)."""
    alarm = "/sys/class/rtc/rtc0/wakealarm"
    with open(alarm, "w") as f:
        f.write("0")
    with open(alarm, "w") as f:
        f.write(f"+{secs}")
    if not read(alarm):
        raise Fail("the RTC alarm could not be set: no wake test without it")


def wait_resume(succ, before, secs):
    end = time.monotonic() + secs
    while time.monotonic() < end:
        if int(read(succ, "0")) > before:
            return mono_ns()
        time.sleep(0.005)
    return None


def max_temp():
    best = None
    base = "/sys/class/thermal"
    try:
        for d in os.listdir(base):
            if d.startswith("thermal_zone"):
                v = read(f"{base}/{d}/temp")
                if v and v.lstrip("-").isdigit():
                    best = max(best or -10**9, int(v))
    except OSError:
        pass
    return best


def fan_state():
    base = "/sys/class/hwmon"
    try:
        for d in os.listdir(base):
            v = read(f"{base}/{d}/pwm1")
            if v is not None:
                return v
    except OSError:
        pass
    return None


def cmd_drive(a):
    a.systems = a.systems.split(",")
    a.scenarios = a.scenarios.split(",")
    d = Driver(a)

    def stop(*_):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGHUP, stop)
    try:
        d.go()
    except KeyboardInterrupt:
        log("stopped")
        return 3
    finally:
        d.wf_stop()
        d.rec_stop()
    return 0


def cmd_probe(a):
    """Every key of the source device once, and what InputPlumber's DualSense made of it. Run with no UI on the screen
    (the device script stops essway for it): each press would act."""
    devs = input_devices()
    src = next((d for d in devs if d["name"] == SOURCE_NAME), None)
    if not src:
        raise Fail(f"no '{SOURCE_NAME}'")
    pad = Pad(check=False)
    if not pad.out:
        raise Fail("no DualSense from InputPlumber to read")
    found = {}
    for code in sorted(src["keys"]):
        t = mono_ns()
        pad._emit(EV_KEY, code, 1)
        time.sleep(0.06)
        pad._emit(EV_KEY, code, 0)
        time.sleep(0.12)
        outs = [(typ, c, v) for tt, typ, c, v in list(pad.out_events) if tt >= t and v not in (0,)]
        for btn, want in TARGET.items():
            if any((want[0] == "key" and typ == EV_KEY and c == want[1]) or (want[0] == "hat" and typ == EV_ABS and c == want[1] and v == want[2]) for typ, c, v in outs):
                found.setdefault(btn, code)
    out = {"source": pad.source, "codes": found, "missing": [b for b in TARGET if b not in found],
           "inject": "source" if found else "target (nothing came out of InputPlumber: it grabs its source?)",
           "differs_from_default": {b: [SOURCE[b], c] for b, c in found.items() if SOURCE[b] != c}}
    print(json.dumps(out, indent=1))
    if a.out:
        json.dump(found, open(a.out, "w"))
    return 0 if not out["missing"] else 1


def cmd_press(a):
    pad = Pad(json.load(open(a.codes)) if a.codes else None, inject=a.inject)
    for item in parse_seq(a.seq):
        if item[0] == "wait":
            time.sleep(item[1] / 1000.0)
            continue
        pad.tap(item[1], item[2], item[0])
        time.sleep(a.gap)
    return 0


# --- analysis -------------------------------------------------------------------------------------------------------

def load_run(d):
    events = []
    with open(os.path.join(d, "events.jsonl")) as f:
        for line in f:
            line = line.strip()
            if line:
                events.append(json.loads(line))
    recs = []
    for n in sorted(os.listdir(d)):
        if n.endswith(".pfr"):
            r = Recording(os.path.join(d, n))
            if r.head:
                recs.append(r)
    return events, recs


class Merged:
    """Several recordings (a recorder that was restarted) as one."""

    def __init__(self, recs):
        self.frames = sorted((f for r in recs for f in r.frames), key=lambda f: f.t)
        self.marks = sorted(m for r in recs for m in r.marks)
        self.head = recs[0].head if recs else None

    at = Recording.at
    between = Recording.between


def recorded(rec, t0, t1):
    """Was the recorder running (not paused) over [t0, t1]?"""
    state = True
    for t, what in rec.marks:
        if t > t1:
            break
        if t <= t0:
            state = what == 2
        elif what == 1:
            return False
    return state


def median(v):
    v = sorted(x for x in v if x is not None)
    if not v:
        return None
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2


def analyze(d):
    events, recs = load_run(d)
    rec = Merged(recs)
    if not rec.frames:
        raise Fail(f"{d}: no frames recorded")
    pics = Pics(rec.head["sw"], rec.head["sh"], rec.head["format"])
    ui = next((e["ui"] for e in events if "ui" in e), "?")
    start = next((e for e in events if e["kind"] == "run-start"), {})
    plan = (start.get("plan") or {}).get(ui, {})
    roi = pics.look(plan.get("ignore"))
    samples = {}
    detail = []

    def add(name, v, **kw):
        samples.setdefault(name, []).append(v)
        detail.append(dict(metric=name, value=v, **kw))

    # Measured presses: react and done.
    for i, e in enumerate(events):
        if e["kind"] != "press" or e.get("rep", 0) < 0 or e["name"].endswith("_try") or e["name"].startswith("showcase."):
            continue
        t = e["t"]
        t_end = t + e["window_ms"] * 1_000_000
        if not recorded(rec, t - MOTION_MS * 1_000_000, t_end):
            f_r = f_d = None
            why_r = why_d = "the recorder was paused"
        else:
            f_r, why_r = react(rec, pics, t, t_end, roi)
            # A step that ends in a running game is done when the game's picture is there: a game that scrolls or
            # animates by itself never settles (uCity's city kept moving for 5 s after it was on the screen).
            to_game = e["name"] == "launch" or (e["name"] == "switch" and e.get("k") == e.get("steps", 0) - 1)
            f_d, why_d = done(rec, pics, t, t_end, roi, GAME_DONE_FRAC if to_game else DONE_FRAC)
        base = e["name"] + (f".{e['system']}" if e.get("system") else "")
        r_ms = (f_r.t - t) / 1e6 if f_r else None
        d_ms = (f_d.t - t) / 1e6 if f_d else None
        if e["name"] == "switch":
            add(f"switch.step{e['k']}.done", d_ms, rep=e["rep"], how=why_d)
        else:
            add(base + ".react", r_ms, rep=e["rep"], how=why_r)
            add(base + ".done", d_ms, rep=e["rep"], how=why_d)
        if e.get("inputplumber_ms") is not None:
            samples.setdefault("input.inputplumber", []).append(e["inputplumber_ms"])
    # Switching: the sum of each rep's steps (the waiting a player does; not their own time between presses).
    # A press whose answer is outside the recorded band ("nothing changed": EmulationStation's Right in its Savestate
    # manager only moves a highlight below it) counts as 0 and is noted; any other missing step leaves the total out.
    reps = {}
    for x in detail:
        if x["metric"].startswith("switch.step"):
            reps.setdefault(x["rep"], []).append(x)
    for rep, steps in reps.items():
        unseen = [x for x in steps if x["value"] is None and x["how"] == "nothing changed"]
        if steps and all(x["value"] is not None or x in unseen for x in steps):
            add("switch.total", sum(x["value"] or 0 for x in steps), rep=rep, presses=len(steps), unseen=len(unseen))
    # Cold start.
    starts = {e["rep"]: e for e in events if e["kind"] == "service-start" and e["rep"] >= 0}
    ref_ev = next((e for e in events if e["kind"] == "start-screen"), None)
    if ref_ev:
        ref = next((f for f in rec.frames if f.t == ref_ev["frame_t"]), None)
        _, mask, _ = end_state(rec, pics, ref_ev["mask_from"], ref_ev["mask_to"], roi) if ref else (None, 0, None)
        for rep, s in sorted(starts.items()):
            t0 = s["t"]
            first = next((f for f in rec.frames if f.t > t0), None)
            listed = next((f for f in rec.frames if f.t > t0 and ref is not None and matches(pics, f, ref, mask)), None)
            add("start.first_frame", (first.t - t0) / 1e6 if first else None, rep=rep)
            add("start.screen", (listed.t - t0) / 1e6 if listed else None, rep=rep)
            proc = next((e for e in events if e["kind"] == "ui-process" and e["rep"] == rep), None)
            add("start.process_seen", (proc["t"] - t0) / 1e6 if proc else None, rep=rep)
    for e in events:
        if e["kind"] == "start-input" and e.get("frame_t") and e["rep"] >= 0:
            # Recomputed from the frames: the first press that got a reaction, its first changed frame.
            tries = [x for x in events if x["kind"] == "press" and x["name"] == "start.input_try" and x["rep"] == e["rep"]]
            hit = None
            for tr in tries:
                f, _ = react(rec, pics, tr["t"], tr["t"] + 250_000_000, roi, motion_ms=0)
                if f:
                    hit = (tr, f)
                    break
            if hit:
                add("start.input", (hit[1].t - e["start"]) / 1e6, rep=e["rep"], presses=len(tries))
                add("start.input_press", (hit[1].t - hit[0]["t"]) / 1e6, rep=e["rep"])
    # Waking.
    for e in events:
        if e["kind"] == "resumed":
            rep = e["rep"]
            back = e["t"]
            so = next((x for x in events if x["kind"] == "screen-on" and x["rep"] == rep), None)
            add("wake.screen_on", (so["t"] - back) / 1e6 if so and so.get("ok") else None, rep=rep)
            first = next((f for f in rec.frames if f.t > back), None)
            add("wake.first_frame", (first.t - back) / 1e6 if first else None, rep=rep)
            tries = [x for x in events if x["kind"] == "press" and x["name"] == "wake.input_try" and x["rep"] == rep]
            for tr in tries:
                f, _ = react(rec, pics, tr["t"], tr["t"] + 100_000_000, roi, motion_ms=0)
                if f:
                    add("wake.input", (f.t - back) / 1e6, rep=rep, presses=tries.index(tr) + 1)
                    break
            else:
                add("wake.input", None, rep=rep)
    # CPU and power.
    for e in events:
        if e["kind"] == "cpu":
            add(e["name"] + ".ui", e["ui_pct"], rep=e["rep"])
            add(e["name"] + ".system", e["system_pct"], rep=e["rep"])
            if e["name"] == "cpu.behind_game":
                add(e["name"] + ".game", e["game_pct"], rep=e["rep"])
        if e["kind"] == "play":
            s = e["system"]
            add(f"play.{s}.game_cpu", e["game_pct"])
            add(f"play.{s}.ui_cpu", e["ui_pct"])
            add(f"play.{s}.system_cpu", e["system_pct"])
            watts = []
            on_battery = True
            for smp in e["samples"]:
                if smp.get("status") not in ("Discharging",):
                    on_battery = False
                if "power_now" in smp:
                    watts.append(abs(int(smp["power_now"])) / 1e6)
                elif "current_now" in smp and "voltage_now" in smp:
                    watts.append(abs(int(smp["current_now"])) * int(smp["voltage_now"]) / 1e12)
            add(f"play.{s}.watts", (sum(watts) / len(watts)) if watts and on_battery else None,
                on_battery=on_battery, n=len(watts), temp_max_mC=max((x.get("temp_mC") or 0) for x in e["samples"]) if e["samples"] else None)
    # Diagnostics: how late frames reach the recorder (the compositor's time stamp is what counts, not this).
    lag = [(f.recv - f.t) / 1e6 for f in rec.frames if f.recv]
    out = {
        "schema": 1, "ui": ui, "dir": os.path.basename(os.path.abspath(d)),
        "recording": {"frames": len(rec.frames), "head": rec.head, "lag_ms_median": median(lag),
                      "lag_ms_max": max(lag) if lag else None},
        "thresholds": {"change_frac": CHANGE_FRAC, "match_frac": MATCH_FRAC, "done_frac": DONE_FRAC, "game_done_frac": GAME_DONE_FRAC, "motion_ms": MOTION_MS, "tail_ms": TAIL_MS},
        "run": {k: start.get(k) for k in ("args", "pad_source", "dualsense", "codes")},
        "metrics": {k: {"median": median(v), "n": len([x for x in v if x is not None]), "missing": len([x for x in v if x is None]),
                        "min": min((x for x in v if x is not None), default=None), "max": max((x for x in v if x is not None), default=None)}
                    for k, v in samples.items()},
        "samples": detail,
        "failures": [e for e in events if e["kind"] in ("scenario-failed", "start-timeout", "wake-none", "wake-skipped")],
    }
    return out


def cmd_analyze(a):
    out = analyze(a.dir)
    path = a.out or os.path.join(a.dir, "result.json")
    json.dump(out, open(path, "w"), indent=1)
    log(f"{path}: {len(out['metrics'])} numbers, {len(out['failures'])} failures")
    return 0


ROWS = [
    ("start.screen", "Cold start: service start -> start screen", "ms"),
    ("start.input", "Cold start: service start -> first press acted on", "ms"),
    ("nav.react", "List move: press -> screen changes", "ms"),
    ("nav.done", "List move: press -> move finished", "ms"),
    ("launch.*.react", "Game launch: A -> screen changes", "ms"),
    ("launch.*.done", "Game launch: A -> game shown", "ms"),
    ("pause.*.react", "Pause menu: press -> screen changes", "ms"),
    ("pause.*.done", "Pause menu: press -> menu shown", "ms"),
    ("quit.*.done", "Quit to the list: confirming press -> list shown", "ms"),
    ("switch.total", "Switch game: sum of the waits of every press", "ms"),
    ("wake.screen_on", "Deep sleep: system back -> backlight on", "ms"),
    ("wake.first_frame", "Deep sleep: system back -> first frame", "ms"),
    ("wake.input", "Deep sleep: system back -> a press acted on", "ms"),
    ("cpu.list_idle.ui", "CPU, list, idle: the UI", "%"),
    ("cpu.list_idle.system", "CPU, list, idle: whole system", "%"),
    ("cpu.behind_game.ui", "CPU behind a running game: the UI", "%"),
    ("cpu.behind_game.game", "CPU behind a running game: the emulator", "%"),
    ("play.*.game_cpu", "Playing (5 min by default): emulator CPU", "%"),
    ("play.*.system_cpu", "Playing: whole system CPU", "%"),
    ("play.*.watts", "Playing: battery power (on battery only)", "W"),
    ("input.inputplumber", "(press -> InputPlumber's pad, both)", "ms"),
]


def expand(metrics, pattern):
    if "*" not in pattern:
        return [pattern] if pattern in metrics else []
    pre, post = pattern.split("*")
    return sorted(k for k in metrics if k.startswith(pre) and k.endswith(post) and "." not in k[len(pre):len(k) - len(post)])


def fmtv(m, unit):
    if not m or m["median"] is None:
        return "–"
    v = m["median"]
    s = f"{v:.0f}" if unit == "ms" and v >= 100 else f"{v:.1f}" if unit != "W" else f"{v:.2f}"
    extra = f" ({m['n']}" + (f", {m['missing']} missed" if m["missing"] else "") + ")"
    return f"{s} {unit}{extra}"


def table(results):
    by_ui = {}
    for r in results:
        by_ui.setdefault(r["ui"], r)
    uis = [u for u in ("es", "pocket-ui") if u in by_ui] + [u for u in by_ui if u not in ("es", "pocket-ui")]
    names = {"es": "EmulationStation + RetroArch", "pocket-ui": "pocket-ui + pocket-core"}
    lines = ["| What | " + " | ".join(names.get(u, u) for u in uis) + " |", "|---|" + "---|" * len(uis)]
    for pat, label, unit in ROWS:
        keys = set()
        for u in uis:
            keys.update(expand(by_ui[u]["metrics"], pat))
        for k in sorted(keys):
            suffix = ""
            if "*" in pat:
                pre, post = pat.split("*")
                suffix = f" ({k[len(pre):len(k) - len(post)]})"
            lines.append(f"| {label}{suffix} | " + " | ".join(fmtv(by_ui[u]["metrics"].get(k), unit) for u in uis) + " |")
    lines.append("")
    lines.append("Medians; in brackets the number of measurements. Times end at the frame the compositor committed "
                 "(the panel shows it at its next refresh, up to 16.7 ms later, the same for both); the compositor "
                 "commits once per refresh, so each time is on a 16.7 ms grid.")
    return "\n".join(lines)


def cmd_table(a):
    res = [json.load(open(p)) for p in a.files]
    print(table(res))
    return 0


def cmd_thumbs(a):
    """JPEG strips (needs ffmpeg on this computer): for each named press, the frame at the press, react, done."""
    events, recs = load_run(a.dir)
    rec = Merged(recs)
    h = rec.head
    pics = Pics(h["sw"], h["sh"], h["format"])
    os.makedirs(a.out, exist_ok=True)
    n = 0
    for e in events:
        if e["kind"] != "press" or (a.names and e["name"] not in a.names):
            continue
        t = e["t"]
        t_end = t + e.get("window_ms", 1000) * 1_000_000
        fr = [rec.at(t), react(rec, pics, t, t_end)[0], done(rec, pics, t, t_end)[0]]
        raw = b"".join(rgb(f.px if f else b"\0" * (h["sw"] * h["sh"] * 4), h["format"]) for f in fr)
        # Three pictures side by side: rows interleaved.
        row = h["sw"] * 3
        strip = bytearray()
        for y in range(h["sh"]):
            for i in range(3):
                strip += raw[i * h["sw"] * h["sh"] * 3 + y * row:i * h["sw"] * h["sh"] * 3 + (y + 1) * row]
        name = f"{e['name']}-{e.get('system', '')}-{e.get('rep', 0)}-{e.get('k', 0)}.jpg"
        subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{h['sw'] * 3}x{h['sh']}",
                        "-i", "-", "-vf", "transpose=2" if h.get("region") or h.get("height", 0) > h.get("width", 0) else "null",
                        "-q:v", "5", os.path.join(a.out, name)], input=bytes(strip), check=True)
        n += 1
        if n >= a.limit:
            break
    log(f"{n} strips in {a.out} (at the press | react | done)")
    return 0


def cmd_import_video(a):
    """A video (wf-recorder's, or a synthetic one) as a recording: each frame at --start-ms + its own pts. A video has
    no clock of its own, so the result is only as good as that start; the self-test knows it, a device recording
    does not (docs/COMPARE-ROCKNIX.md, "Why not wf-recorder")."""
    probe = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height",
                            "-show_entries", "frame=best_effort_timestamp_time", "-of", "json", a.video], capture_output=True, text=True, check=True)
    info = json.loads(probe.stdout)
    w, h = info["streams"][0]["width"], info["streams"][0]["height"]
    pts = [float(f["best_effort_timestamp_time"]) for f in info["frames"]]
    sw, sh = (w + a.step - 1) // a.step, (h + a.step - 1) // a.step
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", a.video, "-vf", f"scale={sw}:{sh}:flags=neighbor", "-f", "rawvideo",
                          "-pix_fmt", "bgr0", "-"], capture_output=True, check=True).stdout
    size = sw * sh * 4
    t0 = int(a.start_ms * 1e6)
    with open(a.out, "wb") as f:
        f.write(b"PFR1\n" + json.dumps({"format": WL_SHM_FORMAT_XRGB8888, "width": w, "height": h, "stride": w * 4, "step": a.step,
                                        "sw": sw, "sh": sh, "source": os.path.basename(a.video)}).encode() + b"\n")
        for i in range(min(len(pts), len(raw) // size)):
            z = zlib.compress(raw[i * size:(i + 1) * size], 1)
            t = t0 + int(round(pts[i] * 1e9))
            f.write(struct.pack("<4sQQIH", b"FRM1", t, 0, 0, 0) + struct.pack("<I", len(z)) + z)
    log(f"{a.out}: {min(len(pts), len(raw) // size)} frames")
    return 0


def cmd_fake_wf(a):
    """The simulation's wf-recorder: records like `record` (every pixel), and at SIGINT writes the -f file as a 60 fps
    video whose t=0 is its first frame, as wf-recorder's is, turned a quarter clockwise (the cut turns it back), and
    the true start on CLOCK_MONOTONIC next to it (<file>.t0) for the test to check the lining up."""
    args = a.rest
    out = args[args.index("-f") + 1]
    pfr = out + ".pfr"
    rec = Recorder(pfr, step=1)
    signal.signal(signal.SIGINT, lambda *_: setattr(rec, "stop", True))
    signal.signal(signal.SIGTERM, lambda *_: setattr(rec, "stop", True))
    rec.run()
    r = Recording(pfr)
    h = r.head
    fr = r.frames
    t0 = fr[0].t
    with open(out + ".t0", "w") as f:
        f.write(str(t0))
    raw = bytearray()
    k, i = 0, 0
    while t0 + k * 1e9 / 60 <= fr[-1].t + 1e9 / 60:
        while i + 1 < len(fr) and fr[i + 1].t <= t0 + k * 1e9 / 60:
            i += 1
        raw += fr[i].px
        k += 1
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "bgr0", "-s", f"{h['sw']}x{h['sh']}", "-r", "60",
                    "-i", "-", "-vf", "transpose=1,scale=trunc(iw/2)*2:trunc(ih/2)*2", "-c:v", "libx264", "-crf", "18",
                    "-pix_fmt", "yuv420p", out], input=bytes(raw), check=True)
    os.unlink(pfr)
    return 0


def rgb(px, fmt):
    """4-byte pixels -> RGB."""
    out = bytearray(len(px) // 4 * 3)
    if fmt in (0, 1, 0x34325241, 0x34325258):  # B G R X (ARGB/XRGB8888)
        out[0::3], out[1::3], out[2::3] = px[2::4], px[1::4], px[0::4]
    elif fmt in X_FIRST:
        out[0::3], out[1::3], out[2::3] = px[3::4], px[2::4], px[1::4]
    else:  # R G B X
        out[0::3], out[1::3], out[2::3] = px[0::4], px[1::4], px[2::4]
    return bytes(out)


# --- self-test -----------------------------------------------------------------------------------------------------

class FakeCompositor:
    """A Wayland server with wl_shm, one wl_output and zwlr_screencopy_manager_v1 (v3), answering copies with
    pictures it is given: `show(pixels)` commits a new frame (copy_with_damage completes on the next one, a plain copy
    at once). Clients one after another or at the same time (each recorder of a run connects anew)."""

    def __init__(self, path, w=64, h=48, fmt=1):
        self.path, self.w, self.h, self.fmt = path, w, h, fmt
        self.stride = w * 4
        self.picture = bytes(self.stride * h)
        self.commits = []  # the ns time stamp of each commit
        self.lock = threading.Lock()
        self.cv = threading.Condition(self.lock)
        self.conns = []
        if os.path.exists(path):
            os.unlink(path)
        self.srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.srv.bind(path)
        self.srv.listen(4)
        self.stop = False
        threading.Thread(target=self._accept, daemon=True).start()
        threading.Thread(target=self._copier, daemon=True).start()

    def show(self, picture):
        with self.cv:
            self.picture = picture
            t = mono_ns()
            self.commits.append(t)
            self.cv.notify_all()
        return t

    def _accept(self):
        while not self.stop:
            try:
                conn, _ = self.srv.accept()
            except OSError:
                return
            c = {"sock": conn, "pending": [], "buffers": {}}
            with self.lock:
                self.conns.append(c)
            threading.Thread(target=self._serve, args=(c,), daemon=True).start()

    def _send(self, c, data):
        try:
            c["sock"].sendall(data)
        except OSError:
            pass

    def _serve(self, c):
        conn = c["sock"]
        objs = {1: "wl_display"}
        pools = {}
        buf = b""
        fds = []
        while not self.stop:
            try:
                data, anc, _, _ = conn.recvmsg(65536, socket.CMSG_SPACE(16))
            except OSError:
                data = b""
            if not data:
                with self.lock:
                    self.conns.remove(c)
                return
            for _lv, _ty, fd in anc:
                fds += list(struct.unpack(f"{len(fd) // 4}i", fd))
            buf += data
            while len(buf) >= 8:
                obj, szop = struct.unpack_from("<II", buf)
                size, op = szop >> 16, szop & 0xFFFF
                if len(buf) < size:
                    break
                p, buf = buf[8:size], buf[size:]
                iface = objs.get(obj)
                out = b""
                if iface == "wl_display" and op == 1:
                    rid = struct.unpack_from("<I", p)[0]
                    objs[rid] = "wl_registry"
                    for n, (name, ver) in enumerate((("wl_shm", 1), ("wl_output", 4), ("zwlr_screencopy_manager_v1", 3)), 1):
                        out += wl_msg(rid, 0, struct.pack("<I", n) + wl_str(name) + struct.pack("<I", ver))
                elif iface == "wl_display" and op == 0:
                    cb = struct.unpack_from("<I", p)[0]
                    out += wl_msg(cb, 0, struct.pack("<I", 0)) + wl_msg(1, 1, struct.pack("<I", cb))
                elif iface == "wl_registry" and op == 0:
                    iname, off = read_wl_str(p, 4)
                    _ver, nid = struct.unpack_from("<II", p, off)
                    objs[nid] = iname
                    if iname == "wl_output":
                        out += wl_msg(nid, 1, struct.pack("<Iiii", 3, self.w, self.h, 60000)) + wl_msg(nid, 4, wl_str("FAKE-1")) + wl_msg(nid, 2)
                elif iface == "wl_shm" and op == 0:
                    pid, size = struct.unpack_from("<Ii", p)
                    fd = fds.pop(0)
                    pools[pid] = mmap.mmap(fd, size)
                    os.close(fd)
                    objs[pid] = "wl_shm_pool"
                elif iface == "wl_shm_pool" and op == 0:
                    bid, off, _w, h, stride, _fmt = struct.unpack_from("<IiiiiI", p)
                    c["buffers"][bid] = (pools[obj], off, stride, h)
                    objs[bid] = "wl_buffer"
                elif iface == "zwlr_screencopy_manager_v1" and op == 0:
                    fid = struct.unpack_from("<I", p)[0]
                    objs[fid] = "zwlr_screencopy_frame_v1"
                    out += wl_msg(fid, 0, struct.pack("<IIII", self.fmt, self.w, self.h, self.stride)) + wl_msg(fid, 6)
                elif iface == "zwlr_screencopy_frame_v1" and op in (0, 2):
                    bid = struct.unpack_from("<I", p)[0]
                    with self.cv:
                        c["pending"].append((obj, bid, op == 2, len(self.commits)))
                        self.cv.notify_all()
                if out:
                    with self.lock:
                        self._send(c, out)

    def _copier(self):
        while not self.stop:
            with self.cv:
                self.cv.wait(0.05)
                pic = self.picture
                t = self.commits[-1] if self.commits else mono_ns()
                for c in self.conns:
                    for item in list(c["pending"]):
                        fid, bid, dmg, seen = item
                        if dmg and len(self.commits) <= seen:
                            continue
                        c["pending"].remove(item)
                        mm, off, stride, h = c["buffers"][bid]
                        mm[off:off + stride * h] = pic
                        ts = t if dmg else mono_ns()
                        sec, ns = divmod(ts, 1_000_000_000)
                        msg = wl_msg(fid, 1, struct.pack("<I", 0))
                        if dmg:
                            msg += wl_msg(fid, 4, struct.pack("<IIII", 0, 0, self.w, self.h))
                        msg += wl_msg(fid, 2, struct.pack("<III", sec >> 32, sec & 0xFFFFFFFF, ns))
                        self._send(c, msg)


def solid(w, h, color, rects=()):
    """A picture (B G R X) of one colour with rectangles (x0, y0, x1, y1, colour) on it."""
    px = bytearray(bytes((color[2], color[1], color[0], 0)) * (w * h))
    for x0, y0, x1, y1, c in rects:
        for y in range(y0, y1):
            for x in range(x0, x1):
                i = (y * w + x) * 4
                px[i:i + 4] = bytes((c[2], c[1], c[0], 0))
    return bytes(px)


class Synth:
    """A recording made up in memory: frames at given times."""

    def __init__(self, w, h, step=1):
        self.head = {"sw": w, "sh": h, "format": 1, "step": step}
        self.frames = []
        self.marks = []

    def add(self, t_ms, px):
        self.frames.append(Frame(int(t_ms * 1e6), int(t_ms * 1e6) + 2_000_000, 0, [], zlib.compress(px, 1)))
        self.frames.sort(key=lambda f: f.t)

    at = Recording.at
    between = Recording.between


def selftest_wayland():
    import tempfile
    tmp = tempfile.mkdtemp()
    path = os.path.join(tmp, "wl")
    fake = FakeCompositor(path, w=64, h=48)
    out = os.path.join(tmp, "rec.pfr")
    rec = Recorder(out, step=4, display=path)
    th = threading.Thread(target=rec.run, args=(3.0,), daemon=True)
    th.start()
    time.sleep(0.3)
    shown = []
    for i in range(5):
        shown.append(fake.show(solid(64, 48, (10 * i, 200, 30), [(8, 8, 24, 24, (255, 255, 255))])))
        time.sleep(0.1)
    time.sleep(0.3)
    rec.want_pause = True
    time.sleep(0.15)
    fake.show(solid(64, 48, (0, 0, 0)))  # while paused: at most the one frame already asked for
    time.sleep(0.2)
    rec.want_resume = True
    time.sleep(0.3)
    rec.stop = True
    th.join(2)
    r = Recording(out)
    assert r.head and r.head["sw"] == 16 and r.head["sh"] == 12 and r.head["output"] == "FAKE-1", r.head
    ts = [f.t for f in r.frames]
    assert len(r.frames) >= 6, f"{len(r.frames)} frames"
    for t in shown:
        assert t in ts, f"commit {t} not recorded: {ts}"
    f = next(f for f in r.frames if f.t == shown[2])
    px = f.px
    assert len(px) == 16 * 12 * 4 and px[0:4] == bytes((30, 200, 20, 0)), px[0:4]
    # Pixel (2,2) of the sampled picture is (8, 8) of the real one: inside the white square.
    assert px[(2 * 16 + 2) * 4:(2 * 16 + 2) * 4 + 3] == b"\xff\xff\xff"
    # Started, paused, resumed, stopped.
    assert [m[1] for m in r.marks] == [2, 1, 2, 1], r.marks
    assert recorded(r, shown[0], shown[-1]) and not recorded(r, r.marks[1][0] + 1, r.marks[2][0] - 1)
    fake.stop = True
    __import__("shutil").rmtree(tmp, ignore_errors=True)
    print("selftest: Wayland screencopy client ok (%d frames, ready time stamps = commit times)" % len(r.frames))


def selftest_analysis():
    W, H = 40, 30
    pics = Pics(W, H, 1)
    grey = (60, 60, 60)
    # 1. A list: the highlight moves 33 ms after the press, a 50 ms slide, then still.
    s = Synth(W, H)
    s.add(0, solid(W, H, grey, [(2, 2, 38, 8, (200, 200, 0))]))
    s.add(1033, solid(W, H, grey, [(2, 4, 38, 10, (200, 200, 0))]))
    s.add(1050, solid(W, H, grey, [(2, 6, 38, 12, (200, 200, 0))]))
    s.add(1083, solid(W, H, grey, [(2, 8, 38, 14, (200, 200, 0))]))
    f, how = react(s, pics, int(1000e6), int(1700e6))
    assert f and f.t == int(1033e6), (f and f.t, how)
    f, how = done(s, pics, int(1000e6), int(1700e6))
    assert f and f.t == int(1083e6), (f and f.t, how)
    # 2. A clock ticking elsewhere before the press does not count; a status-bar change alone is no reaction.
    s = Synth(W, H)
    for k in range(10):
        s.add(k * 100, solid(W, H, grey, [(30, 0, 30 + (k % 3) + 1, 1, (255, 255, 255))]))
    s.add(1010, solid(W, H, grey, [(30, 0, 31, 1, (255, 255, 255))]))
    f, how = react(s, pics, int(1000e6), int(1500e6))
    assert f is None, how
    # 3. A running game (everything moving inside its picture), then the pause menu covers the borders too.
    s = Synth(W, H)
    import random
    rnd = random.Random(1)
    game = lambda: [(10, 5, 30, 25, (rnd.randrange(256), rnd.randrange(256), rnd.randrange(256)))] + \
        [(10 + rnd.randrange(20), 5 + rnd.randrange(20), 12 + rnd.randrange(18), 7 + rnd.randrange(18), (rnd.randrange(256),) * 3) for _ in range(4)]
    t = 0.0
    while t < 1000:
        s.add(t, solid(W, H, (0, 0, 0), game()))
        t += 16.7
    menu = solid(W, H, (30, 30, 90), [(4, 4, 36, 10, (240, 240, 240))])
    s.add(1000 + 16.7 * 3, solid(W, H, (15, 15, 45), [(10, 5, 30, 25, (90, 30, 30))]))  # a fade
    s.add(1000 + 16.7 * 4, menu)
    f, how = react(s, pics, int(1001e6), int(1800e6))
    assert f and abs(f.t / 1e6 - (1000 + 16.7 * 3)) < 0.01, (f and f.t, how)
    f, how = done(s, pics, int(1001e6), int(1800e6))
    assert f and abs(f.t / 1e6 - (1000 + 16.7 * 4)) < 0.01, (f and f.t, how)
    # 4. A launch: list -> black (the emulator's window) -> the game in a frame (bezel), the game keeps moving.
    s = Synth(W, H)
    s.add(0, solid(W, H, grey, [(2, 2, 38, 8, (200, 200, 0))]))
    s.add(1100, solid(W, H, (0, 0, 0)))
    bezel = [(0, 0, W, 4, (120, 40, 40)), (0, 26, W, H, (120, 40, 40))]
    t = 1400.0
    while t < 3000:
        s.add(t, solid(W, H, (0, 0, 0), bezel + game()))
        t += 16.7
    f, how = react(s, pics, int(1000e6), int(3000e6))
    assert f and f.t == int(1100e6), (f and f.t, how)
    f, how = done(s, pics, int(1000e6), int(3000e6))
    assert f and f.t == int(1400e6), (f and f.t, how)
    # 5. A launch without a frame around the game: the black screen before it must not count as the game.
    s = Synth(W, H)
    s.add(0, solid(W, H, grey, [(2, 2, 38, 8, (200, 200, 0))]))
    s.add(1100, solid(W, H, (0, 0, 0)))
    hud = [(12, 6, 28, 8, (250, 250, 250))]  # still parts of the game picture (a score)
    t = 1300.0
    while t < 3000:
        s.add(t, solid(W, H, (0, 0, 0), game() + hud))
        t += 16.7
    f, how = done(s, pics, int(1000e6), int(3000e6))
    assert f and f.t == int(1300e6), (f and f.t, how)
    # 6. Nothing happens: no react, no done.
    s = Synth(W, H)
    s.add(0, solid(W, H, grey))
    assert react(s, pics, int(1000e6), int(2000e6))[0] is None
    assert done(s, pics, int(1000e6), int(2000e6))[0] is None
    # 7. A fade-in of the end state: done is the first frame within a colour level of it, after which all are.
    s = Synth(W, H)
    s.add(0, solid(W, H, (0, 0, 0)))
    for k, v in enumerate((40, 120, 196, 200)):
        s.add(1020 + k * 16.7, solid(W, H, (v, v, v)))
    f, _ = done(s, pics, int(1000e6), int(2000e6))
    assert f and abs(f.t / 1e6 - (1020 + 2 * 16.7)) < 0.01, f and f.t / 1e6
    # 8. Bit tricks: diff counts pixels, not channels, and ignores the padding byte.
    a = Frame(0, 0, 0, [], zlib.compress(bytes((10, 10, 10, 0, 10, 10, 10, 0)), 1))
    b = Frame(0, 0, 0, [], zlib.compress(bytes((200, 200, 200, 0, 10, 10, 10, 255)), 1))
    p2 = Pics(2, 1, 1)
    assert p2.count(p2.diff(a, b)) == 1
    # 9. Pause marks: frames of a paused stretch are not "the screen at the press".
    s = Synth(W, H)
    s.marks = [(int(500e6), 1), (int(900e6), 2)]
    assert recorded(s, int(950e6), int(990e6)) and not recorded(s, int(400e6), int(600e6))
    print("selftest: analysis ok (list move, clock, game + pause menu, launch with and without a frame, fade)")


def selftest_run():
    """A whole run in a folder: events + a recording file, analysed and tabled like a real one."""
    import tempfile
    d = tempfile.mkdtemp()
    W, H = 40, 30
    grey = (60, 60, 60)
    path = os.path.join(d, "frames.pfr")
    with open(path, "wb") as f:
        f.write(b"PFR1\n" + json.dumps({"format": 1, "width": W, "height": H, "stride": W * 4, "step": 1, "sw": W, "sh": H}).encode() + b"\n")

        def frame(t_ms, px):
            z = zlib.compress(px, 1)
            f.write(struct.pack("<4sQQIH", b"FRM1", int(t_ms * 1e6), int(t_ms * 1e6) + 1_000_000, 0, 0) + struct.pack("<I", len(z)) + z)

        frame(0, solid(W, H, grey, [(2, 2, 38, 8, (200, 200, 0))]))
        for k in range(5):
            t0 = 2000 + k * 1000
            y = 2 + 2 * ((k + 1) % 2)
            frame(t0 + 20, solid(W, H, grey, [(2, y, 38, y + 6, (200, 200, 0))]))
        frame(8000 + 40, solid(W, H, (0, 0, 0)))
        frame(8000 + 300, solid(W, H, (90, 20, 20), [(10, 5, 30, 25, (0, 200, 0))]))
    ev = [{"kind": "run-start", "ui": "pocket-ui", "t": 0}]
    for k in range(5):
        ev.append({"kind": "press", "ui": "pocket-ui", "name": "nav", "btn": "DOWN", "t": int((2000 + k * 1000) * 1e6), "t_up": 0, "rep": k, "window_ms": 800})
    ev.append({"kind": "press", "ui": "pocket-ui", "name": "launch", "system": "gba", "btn": "A", "t": int(8000e6), "t_up": 0, "rep": 0, "window_ms": 2000})
    with open(os.path.join(d, "events.jsonl"), "w") as f:
        for e in ev:
            f.write(json.dumps(e) + "\n")
    out = analyze(d)
    m = out["metrics"]
    assert m["nav.react"]["median"] == 20 and m["nav.react"]["n"] == 5, m["nav.react"]
    assert m["launch.gba.react"]["median"] == 40 and m["launch.gba.done"]["median"] == 300, m
    es = json.loads(json.dumps(out))
    es["ui"] = "es"
    es["metrics"]["nav.react"]["median"] = 51.0
    t = table([es, out])
    assert "| List move: press -> screen changes | 51.0 ms (5) | 20.0 ms (5) |" in t, t
    assert "Game launch: A -> game shown (gba)" in t, t
    __import__("shutil").rmtree(d, ignore_errors=True)
    print("selftest: a run analysed and tabled ok")


def selftest_seq():
    assert parse_seq("RIGHT HOME+X w200 A:300") == [(None, "RIGHT", 100), ("HOME", "X", 100), ("wait", 200), (None, "A", 300)]
    try:
        parse_seq("JUMP")
        raise AssertionError("unknown button accepted")
    except Fail:
        pass
    print("selftest: button sequences ok")


class FakeUi:
    """The simulated Pocket of `selftest --sim`: a start screen with three systems, game lists that remember their
    cursor, games (a frame around a picture that changes every refresh), a pause menu and a switcher, with
    EmulationStation's buttons (HOME+X, HOME+START twice) or pocket-ui's (HOME on release, its menu's Quit). It reads
    the presses from the FIFO the driver takes for the pad, answers 30 ms later, and starts and stops when the stand-in
    systemctl says so. Processes are stand-ins too (sleeping Pythons with the game in their command line)."""

    SYSTEMS = ["gbc", "gba", "snes"]
    W, H = 64, 48

    def __init__(self, comp, d, games):
        self.comp, self.d, self.games = comp, d, games
        self.lock = threading.RLock()
        self.flavour = "pocket-ui"
        self.screen = "off"
        self.cursor = 0
        self.lists = {}
        self.system = None
        self.playing = None
        self.prev = None
        self.menu = 0
        self.sw = 0
        self.held = set()
        self.combo = False
        self.armed = 0.0
        self.procs = {}
        self.gen = 0
        self.rnd = __import__("random").Random(7)
        os.makedirs(os.path.join(d, "pids"), exist_ok=True)
        self.render()
        for fn in (self._ticker, self._control, self._input):
            threading.Thread(target=fn, daemon=True).start()

    # -- processes
    def spawn(self, name, tag):
        self.kill(name)
        p = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(600)", tag])
        self.procs[name] = p
        with open(os.path.join(self.d, "pids", name), "w") as f:
            f.write(str(p.pid))

    def kill(self, name):
        p = self.procs.pop(name, None)
        if p:
            p.kill()
            p.wait()
        try:
            os.unlink(os.path.join(self.d, "pids", name))
        except OSError:
            pass

    def kill_all(self):
        for n in list(self.procs):
            self.kill(n)

    def later(self, secs, fn):
        gen = self.gen

        def run():
            with self.lock:
                if gen == self.gen:  # not across a stop
                    fn()
                    self.render()
        t = threading.Timer(secs, run)
        t.daemon = True
        t.start()

    # -- the service
    def _control(self):
        path = os.path.join(self.d, "ctl.log")
        pos = 0
        while True:
            time.sleep(0.01)
            try:
                with open(path) as f:
                    f.seek(pos)
                    lines = f.read().splitlines()
                    pos = f.tell()
            except OSError:
                continue
            for line in lines:
                w = line.split()
                if "essway" not in w:
                    continue
                if "stop" in w or "restart" in w:
                    self.stop()
                if "start" in w or "restart" in w:
                    self.start()

    def stop(self):
        with self.lock:
            self.gen += 1
            self.kill_all()
            self.screen = "off"
            self.playing = None
            self.render()

    def start(self):
        with self.lock:
            self.flavour = "es" if os.path.exists(os.path.join(self.d, "root/tmp/pocket-ui.es-only")) else "pocket-ui"
            self.cursor, self.lists, self.prev = 0, {}, None
            self.later(0.05, lambda: self.spawn(UI_PROC[self.flavour], "ui"))
            self.later(0.2, lambda: setattr(self, "screen", "splash"))
            self.later(0.5, lambda: setattr(self, "screen", "start"))

    # -- the pad
    def _input(self):
        path = os.path.join(self.d, "event0")
        inv = {v: k for k, v in SOURCE.items()}
        while True:
            with open(path, "rb") as f:
                while True:
                    data = f.read(EVENT.size)
                    if len(data) < EVENT.size:
                        break
                    _s, _u, typ, code, value = EVENT.unpack(data)
                    if typ == EV_KEY and code in inv:
                        with self.lock:
                            (self.press if value else self.release)(inv[code])

    def press(self, b):
        self.held.add(b)
        if self.screen in ("off", "splash", "black"):
            return
        if b == "HOME":
            self.combo = False
            return
        if "HOME" in self.held:
            self.combo = True
            if self.flavour == "es" and self.screen in ("game", "menu"):
                if b == "X":
                    self.later(0.03, lambda: setattr(self, "screen", "menu" if self.screen == "game" else "game"))
                elif b == "START" and self.screen == "game":
                    now = time.monotonic()
                    if now - self.armed < 2.5:
                        self.later(0.05, self.quit)
                    else:
                        self.armed = now
            return
        self.later(0.03, lambda: self.act(b))

    def release(self, b):
        self.held.discard(b)
        if b == "HOME" and not self.combo and self.flavour == "pocket-ui" and self.screen == "game":
            self.later(0.03, lambda: (setattr(self, "screen", "menu"), setattr(self, "menu", 0)))

    def act(self, b):
        sc = self.screen
        if sc == "start":
            if b in ("DOWN", "UP"):
                self.cursor = max(0, min(2, self.cursor + (1 if b == "DOWN" else -1)))
            elif b == "A":
                self.system = self.SYSTEMS[self.cursor]
                self.lists.setdefault(self.system, 0)
                self.screen = "list"
        elif sc == "list":
            if b in ("DOWN", "UP"):
                self.lists[self.system] = max(0, min(4, self.lists[self.system] + (1 if b == "DOWN" else -1)))
            elif b == "B":
                self.screen = "start"
            elif b == "A":
                self.launch(self.system, self.lists[self.system] == 2)
        elif sc == "menu" and self.flavour == "pocket-ui":
            if b in ("DOWN", "UP"):
                self.menu = max(0, min(3, self.menu + (1 if b == "DOWN" else -1)))
            elif b == "B" or (b == "A" and self.menu < 3):
                self.screen = "game"
            elif b == "A":
                self.quit()
            elif b == "SELECT":
                self.screen, self.sw = "switcher", 0
        elif sc == "switcher":
            if b in ("LEFT", "RIGHT"):
                self.sw = 1 if b == "RIGHT" else 0
            elif b == "A" and self.sw == 1 and self.prev:
                self.launch(self.prev, True)
            elif b in ("A", "B"):
                self.screen = "game"

    def launch(self, system, right_game):
        if self.playing and self.playing != system:
            self.prev = self.playing
        elif not self.playing and getattr(self, "last", None) and self.last != system:
            self.prev = self.last
        self.spawn(GAME_PROC[self.flavour], f"/roms/{system}/" + (self.games[system] if right_game else "Other game"))
        self.screen = "black"
        self.system = system

        def on():
            self.screen, self.playing = "game", system
        self.later(0.15, on)

    def quit(self):
        self.kill(GAME_PROC[self.flavour])
        self.last = self.playing
        self.system = self.playing or self.system
        self.playing = None
        self.screen = "list"

    # -- the picture
    def _ticker(self):
        while True:
            time.sleep(1 / 60)
            with self.lock:
                if self.screen == "game":
                    self.render()

    def render(self):
        W, H = self.W, self.H
        sc = self.screen
        if sc in ("off", "black"):
            px = solid(W, H, (0, 0, 0))
        elif sc == "splash":
            px = solid(W, H, (40, 40, 120), [(10, 20, 54, 28, (200, 200, 255))])
        elif sc == "start":
            px = solid(W, H, (60, 60, 60), [(4, 6 + 12 * i, 60, 14 + 12 * i, (220, 200, 0) if i == self.cursor else (110, 110, 110)) for i in range(3)])
        elif sc == "list":
            cur = self.lists.get(self.system, 0)
            px = solid(W, H, (30, 60, 30), [(4, 4 + 8 * i, 60, 10 + 8 * i, (230, 230, 230) if i == cur else (70, 100, 70)) for i in range(5)])
        elif sc == "game":
            r = self.rnd
            px = solid(W, H, (120, 40, 40), [(12, 6, 52, 42, (0, 0, 0))] + [(12 + r.randrange(36), 6 + r.randrange(32), 16 + r.randrange(36), 10 + r.randrange(32), (r.randrange(256), r.randrange(256), 90)) for _ in range(6)])
        elif sc == "menu":
            px = solid(W, H, (20, 20, 70), [(8, 6 + 9 * i, 56, 12 + 9 * i, (240, 240, 240) if i == self.menu else (60, 60, 110)) for i in range(4)])
        else:  # switcher
            px = solid(W, H, (70, 20, 20), [(4 + 30 * i, 10, 30 + 30 * i, 38, (250, 200, 120) if i == self.sw else (120, 60, 60)) for i in range(2)])
        self.comp.show(px)


def sim_plan(games):
    common = {
        "move": "DOWN", "move_back": "UP",
        "to_system": {"gbc": "A", "gba": "DOWN A", "snes": "DOWN DOWN A"},
        "from_system": {"gbc": "B", "gba": "B UP", "snes": "B UP UP"},
        "to_game_first": {"gbc": "DOWN DOWN", "gba": "DOWN DOWN", "snes": "DOWN DOWN"},
        "list_system": "gba", "list_move": "DOWN", "list_back": "UP", "ignore": [],
    }
    es = dict(common, pause="HOME+X", pause_close="HOME+X", quit="HOME+START w300 HOME+START",
              switch={"gbc>gba": ["HOME+START w300 HOME+START", "B", "DOWN", "A", "A"]})
    pu = dict(common, pause="HOME", pause_close="B", quit="HOME w300 DOWN DOWN DOWN A",
              switch={"gbc>gba": ["HOME", "SELECT", "RIGHT", "A"]})
    return {"calibrated": True, "games": games, "es": es, "pocket-ui": pu}


def selftest_sim():
    """Both UIs through every scenario but the wake, with the real driver (its own process, as on the device), the
    real recorder and the analysis; the Pocket is FakeUi."""
    import tempfile
    d = tempfile.mkdtemp()
    os.makedirs(os.path.join(d, "root/tmp"))
    os.makedirs(os.path.join(d, "bin"))
    games = {"gbc": "uCity", "gba": "Maximum", "snes": "Stars"}
    json.dump(sim_plan(games), open(os.path.join(d, "plan.json"), "w"))
    keys = 0
    for c in SOURCE.values():
        keys |= 1 << c
    words = []
    while keys:
        words.append(f"{keys & (2**64 - 1):x}")
        keys >>= 64
    with open(os.path.join(d, "devices"), "w") as f:
        f.write(f'I: Bus=0019\nN: Name="{SOURCE_NAME}"\nP: Phys=retroid-pocket-gamepad/input0\nH: Handlers=event0\nB: KEY={" ".join(reversed(words))}\n\n')
    os.mkfifo(os.path.join(d, "event0"))
    stubs = {
        "systemctl": f'echo "$*" >>"{d}/ctl.log"',
        "pidof": f'p=$(cat "{d}/pids/$1" 2>/dev/null) && [ -n "$(tr -d "\\0" </proc/$p/cmdline 2>/dev/null)" ] && echo $p',
        "pactl": "exit 0",
        "wf-recorder": f'exec "{sys.executable}" -I "{os.path.abspath(__file__)}" fake-wf "$@"',
    }
    for name, body in stubs.items():
        path = os.path.join(d, "bin", name)
        with open(path, "w") as f:
            f.write("#!/bin/sh\n" + body + "\n")
        os.chmod(path, 0o755)
    comp = FakeCompositor(os.path.join(d, "wl"), FakeUi.W, FakeUi.H)
    ui = FakeUi(comp, d, games)
    env = dict(os.environ, PATH=os.path.join(d, "bin") + ":" + os.environ["PATH"], WAYLAND_DISPLAY=os.path.join(d, "wl"),
               COMPARE_INPUT_DEVICES=os.path.join(d, "devices"), COMPARE_INPUT_DIR=d, COMPARE_ROOT=os.path.join(d, "root"),
               COMPARE_WF_RECORDER=os.path.join(d, "bin", "wf-recorder"))
    results = []
    try:
        for flavour in ("es", "pocket-ui"):
            out = os.path.join(d, "out", flavour)
            cmd = [sys.executable, "-I", os.path.abspath(__file__), "drive", "--ui", flavour, "--plan", os.path.join(d, "plan.json"),
                   "--out", out, "--scenarios", "start,nav,launch,switch,cpu,play,showcase", "--systems", "gbc,gba", "--reps", "1",
                   "--showcase-systems", "gba,gbc", "--showcase-audio", "0", "--region", "",
                   "--reps-start", "2", "--moves", "4", "--nav-gap", "0.5", "--window", "0.8", "--launch-window", "1.2",
                   "--cpu-secs", "0.3", "--play-secs", "0.6", "--play-boot", "0.2", "--hold", "60", "--step", "1"]
            r = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=600)
            ev = [json.loads(x) for x in open(os.path.join(out, "events.jsonl"))]
            failed = [e for e in ev if e["kind"] == "scenario-failed"]
            assert r.returncode == 0 and not failed, (flavour, r.returncode, r.stderr[-2000:], failed)
            res = analyze(out)
            results.append(res)
            m = res["metrics"]

            def val(k):
                assert k in m and m[k]["median"] is not None, (flavour, k, m.get(k), [x for x in res["samples"] if x["metric"] == k])
                return m[k]["median"]
            # The fake answers 30 ms after a press (pocket-ui's pause menu: after HOME's release, 60 ms later).
            for k in ("nav.react", "nav.done", "launch.gbc.react", "launch.gba.react", "quit.gbc.done", "quit.gba.done", "pause.gbc.done"):
                assert 20 <= val(k) <= 150 + (60 if k.startswith("pause") and flavour == "pocket-ui" else 0), (flavour, k, val(k))
            for k in ("launch.gbc.done", "launch.gba.done"):
                assert 150 <= val(k) <= 400, (flavour, k, val(k))  # black, then the game 150 ms later
            assert 450 <= val("start.screen") <= 900, (flavour, val("start.screen"))
            assert val("start.input") >= val("start.screen"), flavour
            assert val("switch.total") > 0 and m["switch.total"]["n"] == 1, (flavour, m.get("switch.total"))
            for k in ("cpu.list_idle.ui", "cpu.behind_game.game", "play.gbc.game_cpu"):
                assert val(k) is not None
            shows = [e["name"] for e in ev if e["kind"] == "press" and e["name"].startswith("showcase.")]
            assert shows == ["showcase.start"] + ["showcase.nav"] * 4 + ["showcase.launch", "showcase.pause", "showcase.back",
                                                                         "showcase.quit", "showcase.launch2"], shows
            assert os.path.getsize(os.path.join(out, "showcase.mp4")) > 1000
            assert not any(k.startswith("showcase") for k in m), [k for k in m if k.startswith("showcase")]
    finally:
        ui.stop()
        comp.stop = True
        if os.environ.get("COMPARE_SIM_KEEP"):
            for ui, res in zip(("es", "pocket-ui"), results):
                json.dump(res, open(os.path.join(d, f"{ui}.json"), "w"))
            print(f"selftest: the simulated run is in {d}")
        else:
            __import__("shutil").rmtree(d, ignore_errors=True)
    print(table(results))
    print("selftest: simulated run of both UIs ok (every scenario but the wake, with the real driver and recorder)")


def cmd_selftest(a):
    if a.sim:
        selftest_sim()
        return 0
    selftest_seq()
    selftest_analysis()
    selftest_run()
    selftest_wayland()
    print("selftest: ok")
    return 0


# --- main -----------------------------------------------------------------------------------------------------------

def main():
    if sys.argv[1:2] == ["fake-wf"]:  # wf-recorder's own options, not ours
        return cmd_fake_wf(argparse.Namespace(rest=sys.argv[2:]))
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("record")
    r.add_argument("--out", required=True)
    r.add_argument("--step", type=int, default=8)
    r.add_argument("--output", help="the wl_output's name (default: the first)")
    r.add_argument("--secs", type=float)
    r.add_argument("--region", help="x,y,w,h: only that part of the output (logical coordinates)")
    r.add_argument("--ready", help="write the pid here when connected")
    r.set_defaults(func=cmd_record)
    d = sub.add_parser("drive")
    d.add_argument("--ui", required=True, choices=("es", "pocket-ui"))
    d.add_argument("--plan", required=True)
    d.add_argument("--codes", help="JSON from probe: source codes per button")
    d.add_argument("--inject", choices=("source", "target"), default="source", help="where presses go (Pad)")
    d.add_argument("--out", required=True)
    d.add_argument("--scenarios", default="start,nav,launch,switch,wake,cpu,play")
    d.add_argument("--systems", default="gbc,gba,snes")
    d.add_argument("--reps", type=int, default=3, help="repetitions of launch, pause, quit, switch, wake, cpu")
    d.add_argument("--reps-start", dest="reps_start", type=int, default=5)
    d.add_argument("--moves", type=int, default=30)
    d.add_argument("--nav-gap", dest="nav_gap", type=float, default=0.9, help="s between list moves (the window)")
    d.add_argument("--window", type=float, default=2.0, help="s after a press for its end state")
    d.add_argument("--launch-window", dest="launch_window", type=float, default=6.0)
    d.add_argument("--hold", type=int, default=100, help="ms a press is held")
    d.add_argument("--step", type=int, default=4)
    d.add_argument("--region", default="0,450,1240,180",
                   help="the part of the screen the recorder copies (x,y,w,h, logical): sway copies pocket-core's full "
                        "screen at ~17 a second (50 ms each), a band across the middle at 60 (docs/COMPARE-ROCKNIX.md); empty = all")
    d.add_argument("--asleep", type=int, default=15)
    d.add_argument("--cpu-secs", dest="cpu_secs", type=float, default=20)
    d.add_argument("--play-secs", dest="play_secs", type=float, default=300)
    d.add_argument("--play-boot", dest="play_boot", type=float, default=20)
    d.add_argument("--showcase-systems", dest="showcase_systems", default="gba,gbc", help="showcase: the game started, then the second")
    d.add_argument("--showcase-audio", dest="showcase_audio", type=int, default=1, help="showcase: record the speaker's monitor (1) or not (0)")
    d.set_defaults(func=cmd_drive)
    p = sub.add_parser("probe")
    p.add_argument("--out")
    p.set_defaults(func=cmd_probe)
    pr = sub.add_parser("press")
    pr.add_argument("seq")
    pr.add_argument("--codes")
    pr.add_argument("--inject", choices=("source", "target"), default="source")
    pr.add_argument("--gap", type=float, default=0.35)
    pr.set_defaults(func=cmd_press)
    an = sub.add_parser("analyze")
    an.add_argument("dir")
    an.add_argument("--out")
    an.set_defaults(func=cmd_analyze)
    t = sub.add_parser("table")
    t.add_argument("files", nargs="+")
    t.set_defaults(func=cmd_table)
    th = sub.add_parser("thumbs")
    th.add_argument("dir")
    th.add_argument("names", nargs="*")
    th.add_argument("--out", default="screenshots/compare")
    th.add_argument("--limit", type=int, default=12)
    th.set_defaults(func=cmd_thumbs)
    iv = sub.add_parser("import-video")
    iv.add_argument("video")
    iv.add_argument("out")
    iv.add_argument("--start-ms", dest="start_ms", type=float, default=0)
    iv.add_argument("--step", type=int, default=8)
    iv.set_defaults(func=cmd_import_video)
    s = sub.add_parser("selftest")
    s.add_argument("--sim", action="store_true", help="instead: both UIs through the scenarios on a simulated Pocket (about 5 min)")
    s.set_defaults(func=cmd_selftest)
    a = ap.parse_args()
    try:
        return a.func(a)
    except Fail as e:
        log(str(e))
        return 3


if __name__ == "__main__":
    sys.exit(main())
