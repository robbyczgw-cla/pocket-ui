#!/usr/bin/env python3
"""Virtual gamepad for testing pocket-core on the device without touching the buttons (runs on the device).

Start it before the game, so the game finds the pad when it enumerates devices:
    python3 vpad.py [fifo] &    (the FIFO defaults to /tmp/vpad.cmd)
Then write commands to /tmp/vpad.cmd, one per line:
    press A 150      hold A for 150 ms (default 100)
    down A / up A    hold / release
    seq A 100 B 100  presses in a row
    stick 1 0        left stick fully right (x y in -1..1; "stick 0 0" centres it); "stick LX LY RX RY" also moves the right one
    combo HOME R 150   hold the first button, press the second for 150 ms, release both (a shortcut)
Button names: A B X Y L R L2 R2 SELECT START HOME UP DOWN LEFT RIGHT (libretro names; A is the right face button).

VPAD_INJECT=/dev/input/eventN writes the same input into that existing device instead of creating a pad: given the
Mini V2's own "Retroid Pocket Gamepad", it takes the real path (InputPlumber's virtual pad), which every
emulator reads, also SDL ones (standalone PPSSPP, Flycast) that ignore an unknown pad.
"""
import fcntl, os, struct, sys, time

BTN = {"B": 304, "A": 305, "Y": 308, "X": 307, "L": 310, "R": 311, "L2": 312, "R2": 313, "SELECT": 314, "START": 315, "HOME": 316,
       "UP": 544, "DOWN": 545, "LEFT": 546, "RIGHT": 547}
AXES = {"LX": 0, "LY": 1, "RX": 3, "RY": 4}  # ABS_X, ABS_Y, ABS_RX, ABS_RY
FIFO = sys.argv[1] if len(sys.argv) > 1 else "/tmp/vpad.cmd"  # another FIFO lets two sessions each have a pad

INJECT = os.environ.get("VPAD_INJECT")
if INJECT:
    fd = os.open(INJECT, os.O_WRONLY)
    # The device's own axis ranges (EVIOCGABS: value, min, max, fuzz, flat, resolution); its triggers are axes.
    RANGE = {}
    for code in (0, 1, 2, 3, 4, 5):
        info = bytearray(24)
        fcntl.ioctl(fd, 0x80184540 + code, info)
        RANGE[code] = struct.unpack("6i", info)[1:3]
    TRIGGER = {312: 2, 313: 5}  # L2, R2 -> ABS_Z, ABS_RZ


def setup_uinput():
    global fd
    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    fcntl.ioctl(fd, 0x40045564, 1)  # UI_SET_EVBIT EV_KEY
    for code in BTN.values():
        fcntl.ioctl(fd, 0x40045565, code)  # UI_SET_KEYBIT
    fcntl.ioctl(fd, 0x40045564, 3)  # UI_SET_EVBIT EV_ABS
    for code in AXES.values():
        fcntl.ioctl(fd, 0x40045567, code)  # UI_SET_ABSBIT
    absmax, absmin = [0] * 64, [0] * 64
    for code in AXES.values():
        absmax[code], absmin[code] = 32767, -32768
    os.write(fd, struct.pack("80sHHHHI" + "i" * 256, b"pocket-ui test pad", 3, 0x1234, 0x5678, 1, 0, *absmax, *absmin, *([0] * 128)))
    fcntl.ioctl(fd, 0x5501)  # UI_DEV_CREATE
    time.sleep(0.3)


if not INJECT:
    setup_uinput()


def abs_value(code, value):
    """-1..1 in the axis' range (0..1 for triggers)."""
    if not INJECT:
        return int(max(-1.0, min(1.0, value)) * 32767)
    lo, hi = RANGE[code]
    if code in (2, 5):
        return int(lo + (hi - lo) * max(0.0, min(1.0, value)))
    return int((lo + hi) / 2 + (hi - lo) / 2 * max(-1.0, min(1.0, value)))


def move(name, value):
    os.write(fd, struct.pack("llHHi", 0, 0, 3, AXES[name], abs_value(AXES[name], value)))


def emit(code, value):
    if INJECT and code in TRIGGER:
        os.write(fd, struct.pack("llHHi", 0, 0, 3, TRIGGER[code], abs_value(TRIGGER[code], value)))
    else:
        os.write(fd, struct.pack("llHHi", 0, 0, 1, code, value))
    os.write(fd, struct.pack("llHHi", 0, 0, 0, 0, 0))  # SYN_REPORT


if os.path.exists(FIFO):
    os.remove(FIFO)
os.mkfifo(FIFO)
print("ready", flush=True)
while True:
    with open(FIFO) as f:
        for line in f:
            w = line.split()
            if not w:
                continue
            if w[0] == "press":
                emit(BTN[w[1].upper()], 1)
                time.sleep(int(w[2] if len(w) > 2 else 100) / 1000)
                emit(BTN[w[1].upper()], 0)
                time.sleep(0.08)
            elif w[0] == "stick":
                for name, v in zip(("LX", "LY", "RX", "RY"), w[1:]):
                    move(name, float(v))
                os.write(fd, struct.pack("llHHi", 0, 0, 0, 0, 0))
            elif w[0] in ("down", "up"):
                emit(BTN[w[1].upper()], 1 if w[0] == "down" else 0)
            elif w[0] == "combo":
                emit(BTN[w[1].upper()], 1)
                time.sleep(0.15)
                emit(BTN[w[2].upper()], 1)
                time.sleep(int(w[3] if len(w) > 3 else 100) / 1000)
                emit(BTN[w[2].upper()], 0)
                time.sleep(0.1)
                emit(BTN[w[1].upper()], 0)
                time.sleep(0.08)
            elif w[0] == "seq":
                for name, ms in zip(w[1::2], w[2::2]):
                    emit(BTN[name.upper()], 1)
                    time.sleep(int(ms) / 1000)
                    emit(BTN[name.upper()], 0)
                    time.sleep(0.08)
