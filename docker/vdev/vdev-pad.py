#!/usr/bin/env python3
"""Virtual gamepad for the virtual Mini V2: a uinput pad plus its /dev/input node.

Derived from scripts/vpad.py (same commands, same FIFO /tmp/vpad.cmd), with two additions for containers:
- the pad's /dev/input/eventN node is created here, because a container's /dev does not follow the kernel's;
- a MODE button (HOME).
Commands, one per line:  press A 150 | down A | up A | seq A 100 B 100 | stick LX LY [RX RY]  (-1..1, "stick 0 0" centres)
Buttons: A B X Y L R L2 R2 SELECT START MODE UP DOWN LEFT RIGHT (libretro names; A is the right face button),
VOLUP VOLDOWN (the volume keys of the device, KEY_VOLUMEUP/DOWN).

Touch screen: a second uinput device, "generic ft5x06 (8d)", made like the Mini V2's: multitouch protocol B,
direct input, portrait-native 1080x1240 (another device's with VDEV_PANEL). The commands take positions on the 1240x1080
*screen* (VDEV_SCREEN), as a finger sees it,
and turn them into panel coordinates the way sway's output transform does (VDEV_TOUCH_ROT, default 90 like the device):
  touch X Y [ms]            a tap (finger down for ms, default 80)
  swipe X1 Y1 X2 Y2 [ms]    finger down at 1, moves in a straight line to 2 in ms (default 300), lifts
  fdown X Y | fmove X Y | fup   a finger by hand, for holding
`vdev.sh pad touch 620 540`, `vdev.sh pad swipe 600 800 600 300`.

Lid (`vdev.sh up --device flip2`, VDEV_LID=1): a third device, "gpio-keys-lid", with the Flip 2's SW_LID switch:
  lid close | lid open      `vdev.sh lid close`

Force feedback: the pad accepts FF_RUMBLE (and sine FF_PERIODIC) like the Mini V2's vibration motor and writes
what it receives to /tmp/vpad-ff.log, one line each: "upload id=0 rumble strong=.. weak=.. length=..",
"play id=0", "stop id=0", "erase id=0". `vdev.sh sh cat /tmp/vpad-ff.log` shows what a game (or pocket-ui) asked for.
"""
import fcntl, os, struct, sys, threading, time

BTN = {"B": 304, "A": 305, "Y": 308, "X": 307, "L": 310, "R": 311, "L2": 312, "R2": 313, "SELECT": 314, "START": 315,
       "MODE": 316, "UP": 544, "DOWN": 545, "LEFT": 546, "RIGHT": 547, "VOLUP": 115, "VOLDOWN": 114}
AXES = {"LX": 0, "LY": 1, "RX": 3, "RY": 4}  # ABS_X, ABS_Y, ABS_RX, ABS_RY
FIFO = "/tmp/vpad.cmd"

fd = os.open("/dev/uinput", os.O_RDWR | os.O_NONBLOCK)
fcntl.ioctl(fd, 0x40045564, 1)  # UI_SET_EVBIT EV_KEY
for code in BTN.values():
    fcntl.ioctl(fd, 0x40045565, code)  # UI_SET_KEYBIT
fcntl.ioctl(fd, 0x40045564, 0x15)  # UI_SET_EVBIT EV_FF
for code in (0x50, 0x51, 0x5A, 0x60):  # FF_RUMBLE, FF_PERIODIC, FF_SINE, FF_GAIN
    fcntl.ioctl(fd, 0x4004556B, code)  # UI_SET_FFBIT
fcntl.ioctl(fd, 0x40045564, 3)  # UI_SET_EVBIT EV_ABS
for code in AXES.values():
    fcntl.ioctl(fd, 0x40045567, code)  # UI_SET_ABSBIT
absmax, absmin = [0] * 64, [0] * 64
for code in AXES.values():
    absmax[code], absmin[code] = 32767, -32768
os.write(fd, struct.pack("80sHHHHI" + "i" * 256, b"pocket-ui test pad", 3, 0x1234, 0x5678, 1, 16, *absmax, *absmin, *([0] * 128)))
fcntl.ioctl(fd, 0x5501)  # UI_DEV_CREATE


def make_node(ufd):
    """UI_GET_SYSNAME -> /sys/devices/virtual/input/<sysname>/event*/dev -> mknod; returns the node's path."""
    buf = bytearray(64)
    fcntl.ioctl(ufd, 0x8040552C, buf)
    sysname = buf.split(b"\0")[0].decode()
    for _ in range(50):
        base = f"/sys/devices/virtual/input/{sysname}"
        events = [d for d in (os.listdir(base) if os.path.isdir(base) else []) if d.startswith("event")]
        if events:
            major, minor = map(int, open(f"{base}/{events[0]}/dev").read().strip().split(":"))
            os.makedirs("/dev/input", exist_ok=True)
            node = f"/dev/input/{events[0]}"
            if not os.path.exists(node):
                os.mknod(node, 0o660 | 0o020000, os.makedev(major, minor))
            return node
        time.sleep(0.1)
    return None


node = make_node(fd)
if node is None:
    print("no event node for the pad", file=sys.stderr, flush=True)
    sys.exit(1)
time.sleep(0.2)

# ---- the touch screen ------------------------------------------------------------------------------------
# The device's panel (portrait) and screen (landscape): the Mini V2's unless the container was started with another
# device (`vdev.sh up --device`, which sets VDEV_PANEL and VDEV_SCREEN, docs/VIRTUAL-DEVICE.md).
PANEL_W, PANEL_H = map(int, os.environ.get("VDEV_PANEL", "1080x1240").split("x"))
SCREEN_W, SCREEN_H = map(int, os.environ.get("VDEV_SCREEN", "1240x1080").split("x"))
TOUCH_ROT = int(os.environ.get("VDEV_TOUCH_ROT", "90"))
ABS_X, ABS_Y, ABS_MT_SLOT, ABS_MT_X, ABS_MT_Y, ABS_MT_ID = 0, 1, 0x2F, 0x35, 0x36, 0x39
BTN_TOUCH = 330
tfd = os.open("/dev/uinput", os.O_RDWR | os.O_NONBLOCK)
fcntl.ioctl(tfd, 0x40045564, 1)  # EV_KEY
fcntl.ioctl(tfd, 0x40045565, BTN_TOUCH)
fcntl.ioctl(tfd, 0x40045564, 3)  # EV_ABS
for code in (ABS_X, ABS_Y, ABS_MT_SLOT, ABS_MT_X, ABS_MT_Y, ABS_MT_ID):
    fcntl.ioctl(tfd, 0x40045567, code)
fcntl.ioctl(tfd, 0x4004556E, 1)  # UI_SET_PROPBIT INPUT_PROP_DIRECT
tmax = [0] * 64
tmax[ABS_X], tmax[ABS_Y], tmax[ABS_MT_SLOT] = PANEL_W - 1, PANEL_H - 1, 4
tmax[ABS_MT_X], tmax[ABS_MT_Y], tmax[ABS_MT_ID] = PANEL_W - 1, PANEL_H - 1, 65535
os.write(tfd, struct.pack("80sHHHHI" + "i" * 256, b"generic ft5x06 (8d)", 0x18, 0, 0, 0, 0, *tmax, *([0] * 64), *([0] * 128)))
fcntl.ioctl(tfd, 0x5501)  # UI_DEV_CREATE
touch_node = make_node(tfd)
print(f"touch {touch_node} rotation {TOUCH_ROT}", flush=True)
finger_id = 0


def to_panel(x, y):
    """A position on the landscape screen to panel coordinates: the inverse of sway's `output transform`."""
    fx, fy = min(max(x / SCREEN_W, 0.0), 1.0), min(max(y / SCREEN_H, 0.0), 1.0)
    u, v = {0: (fx, fy), 90: (fy, 1 - fx), 180: (1 - fx, 1 - fy), 270: (1 - fy, fx)}[TOUCH_ROT]
    return round(u * (PANEL_W - 1)), round(v * (PANEL_H - 1))


def temit(*events):
    for etype, code, value in events:
        os.write(tfd, struct.pack("llHHi", 0, 0, etype, code, value))
    os.write(tfd, struct.pack("llHHi", 0, 0, 0, 0, 0))  # SYN_REPORT


def finger(kind, x=0, y=0):
    global finger_id
    if kind == "up":
        temit((3, ABS_MT_SLOT, 0), (3, ABS_MT_ID, -1), (1, BTN_TOUCH, 0))
        return
    px, py = to_panel(x, y)
    pos = [(3, ABS_MT_X, px), (3, ABS_MT_Y, py), (3, ABS_X, px), (3, ABS_Y, py)]
    if kind == "down":
        finger_id += 1
        temit((3, ABS_MT_SLOT, 0), (3, ABS_MT_ID, finger_id), *pos, (1, BTN_TOUCH, 1))
    else:
        temit((3, ABS_MT_SLOT, 0), *pos)



# ---- the lid (the Flip 2, VDEV_LID=1) -----------------------------------------------------------------------
# A third uinput device like the Flip 2's hall sensor: gpio-keys, one switch, SW_LID. `lid close` / `lid open`.
lfd = None
if os.environ.get("VDEV_LID") == "1":
    lfd = os.open("/dev/uinput", os.O_RDWR | os.O_NONBLOCK)
    fcntl.ioctl(lfd, 0x40045564, 5)  # UI_SET_EVBIT EV_SW
    fcntl.ioctl(lfd, 0x4004556D, 0)  # UI_SET_SWBIT SW_LID
    os.write(lfd, struct.pack("80sHHHHI" + "i" * 256, b"gpio-keys-lid", 0x19, 1, 1, 0x100, 0, *([0] * 256)))
    fcntl.ioctl(lfd, 0x5501)  # UI_DEV_CREATE
    print(f"lid {make_node(lfd)}", flush=True)


def lid(closed):
    if lfd is None:
        print("no lid on this device (VDEV_LID=1, vdev.sh up --device flip2)", flush=True)
        return
    os.write(lfd, struct.pack("llHHi", 0, 0, 5, 0, 1 if closed else 0))  # EV_SW SW_LID
    os.write(lfd, struct.pack("llHHi", 0, 0, 0, 0, 0))  # SYN_REPORT


FFLOG = "/tmp/vpad-ff.log"
UI_BEGIN_FF_UPLOAD, UI_END_FF_UPLOAD = 0xC06855C8, 0x406855C9  # struct uinput_ff_upload is 104 bytes
UI_BEGIN_FF_ERASE, UI_END_FF_ERASE = 0xC00C55CA, 0x400C55CB  # struct uinput_ff_erase is 12 bytes


def ff_log(text):
    with open(FFLOG, "a") as f:
        f.write(f"{time.time():.3f} {text}\n")


def ff_loop():
    """Answers the kernel's force-feedback uploads and logs plays, like a motor that is only watched."""
    import select
    while True:
        select.select([fd], [], [], 1.0)
        try:
            data = os.read(fd, 24 * 16)
        except BlockingIOError:
            continue
        for i in range(0, len(data) - 23, 24):
            _, _, etype, code, value = struct.unpack("llHHi", data[i:i + 24])
            if etype == 0x0101 and code == 1:  # EV_UINPUT, UI_FF_UPLOAD
                buf = bytearray(struct.pack("Ii", value, 0) + bytes(96))
                fcntl.ioctl(fd, UI_BEGIN_FF_UPLOAD, buf, True)
                eff = bytes(buf[8:56])
                kind, eid, _dir = struct.unpack("HhH", eff[:6])
                length = struct.unpack("H", eff[10:12])[0]
                if kind == 0x50:
                    strong, weak = struct.unpack("HH", eff[16:20])
                    ff_log(f"upload id={eid} rumble strong={strong} weak={weak} length={length}")
                else:
                    ff_log(f"upload id={eid} type=0x{kind:x} length={length}")
                struct.pack_into("i", buf, 4, 0)
                fcntl.ioctl(fd, UI_END_FF_UPLOAD, buf, True)
            elif etype == 0x0101 and code == 2:  # UI_FF_ERASE
                buf = bytearray(struct.pack("Iii", value, 0, 0))
                fcntl.ioctl(fd, UI_BEGIN_FF_ERASE, buf, True)
                ff_log(f"erase id={struct.unpack('I', buf[8:12])[0]}")
                struct.pack_into("i", buf, 4, 0)
                fcntl.ioctl(fd, UI_END_FF_ERASE, buf, True)
            elif etype == 0x15:  # EV_FF
                if code == 0x60:
                    ff_log(f"gain {value}")
                else:
                    ff_log(f"{'play' if value else 'stop'} id={code}" + (f" count={value}" if value else ""))


open(FFLOG, "w").close()
threading.Thread(target=ff_loop, daemon=True).start()


def move(name, value):
    os.write(fd, struct.pack("llHHi", 0, 0, 3, AXES[name], int(max(-1.0, min(1.0, value)) * 32767)))


def emit(code, value):
    os.write(fd, struct.pack("llHHi", 0, 0, 1, code, value))
    os.write(fd, struct.pack("llHHi", 0, 0, 0, 0, 0))  # SYN_REPORT


if os.path.exists(FIFO):
    os.remove(FIFO)
os.mkfifo(FIFO)
os.chmod(FIFO, 0o666)
print(f"ready {node}", flush=True)
while True:
    with open(FIFO) as f:
        for line in f:
            w = line.split()
            if not w:
                continue
            try:
                if w[0] == "press":
                    emit(BTN[w[1].upper()], 1)
                    time.sleep(int(w[2] if len(w) > 2 else 100) / 1000)
                    emit(BTN[w[1].upper()], 0)
                    time.sleep(0.08)
                elif w[0] == "stick":
                    for name, v in zip(("LX", "LY", "RX", "RY"), w[1:]):
                        move(name, float(v))
                    os.write(fd, struct.pack("llHHi", 0, 0, 0, 0, 0))
                elif w[0] == "touch":
                    x, y = float(w[1]), float(w[2])
                    finger("down", x, y)
                    time.sleep(int(w[3] if len(w) > 3 else 80) / 1000)
                    finger("up")
                    time.sleep(0.08)
                elif w[0] == "swipe":
                    x1, y1, x2, y2 = map(float, w[1:5])
                    ms = int(w[5] if len(w) > 5 else 300)
                    steps = max(2, ms // 16)
                    finger("down", x1, y1)
                    for i in range(1, steps + 1):
                        time.sleep(ms / 1000 / steps)
                        finger("move", x1 + (x2 - x1) * i / steps, y1 + (y2 - y1) * i / steps)
                    time.sleep(0.03)
                    finger("up")
                    time.sleep(0.08)
                elif w[0] == "lid" and len(w) > 1 and w[1] in ("close", "open"):
                    lid(w[1] == "close")
                elif w[0] == "fdown":
                    finger("down", float(w[1]), float(w[2]))
                elif w[0] == "fmove":
                    finger("move", float(w[1]), float(w[2]))
                elif w[0] == "fup":
                    finger("up")
                elif w[0] in ("down", "up"):
                    emit(BTN[w[1].upper()], 1 if w[0] == "down" else 0)
                elif w[0] == "seq":
                    for name, ms in zip(w[1::2], w[2::2]):
                        emit(BTN[name.upper()], 1)
                        time.sleep(int(ms) / 1000)
                        emit(BTN[name.upper()], 0)
                        time.sleep(0.08)
            except (KeyError, ValueError) as e:
                print("bad command:", line.strip(), e, flush=True)
