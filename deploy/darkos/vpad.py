#!/usr/bin/env python3
# A virtual dArkOS pad for the container test (docs/DARKOS.md, "What is tested"): a uinput device that looks like the RG353M's
# `retrogame_joypad` (same name, bus/vendor/product/version, button codes), driven from a FIFO. Written for this project.
#   vpad.py create FIFO     makes the device, makes its /dev/input node (a container has no udev), writes the node's path to
#                           FIFO.node and then runs: each line of FIFO is `tap CODE [ms]`, `down CODE`, `up CODE` or `quit`
#   vpad.py power FIFO      the separate PMIC power key (rk805 pwrkey, KEY_POWER only)
# Needs /dev/uinput (the container is privileged). Run as root.
import ctypes, fcntl, os, struct, sys, time

EV_SYN, EV_KEY, EV_ABS = 0, 1, 3
NAME = "rk805 pwrkey" if len(sys.argv) > 1 and sys.argv[1] == "power" else "retrogame_joypad"
KEYS = [116] if NAME == "rk805 pwrkey" else [304, 305, 307, 308, 310, 311, 312, 313, 314, 315, 316, 317, 318, 544, 545, 546, 547]
AXES = [] if NAME == "rk805 pwrkey" else [0, 1, 2, 3]
UI_SET_EVBIT, UI_SET_KEYBIT, UI_SET_ABSBIT, UI_DEV_CREATE, UI_DEV_DESTROY = 0x40045564, 0x40045565, 0x40045567, 0x5501, 0x5502

def create():
    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    for ev in ((EV_KEY, EV_ABS, EV_SYN) if AXES else (EV_KEY, EV_SYN)):
        fcntl.ioctl(fd, UI_SET_EVBIT, ev)
    for k in KEYS:
        fcntl.ioctl(fd, UI_SET_KEYBIT, k)
    for a in AXES:
        fcntl.ioctl(fd, UI_SET_ABSBIT, a)
    absmax, absmin = [0] * 64, [0] * 64
    for a in AXES:
        absmax[a], absmin[a] = 1800, -1800
    ids = (0x19, 0, 0, 0) if NAME == "rk805 pwrkey" else (0x19, 0x484b, 0x1101, 0x100)
    dev = struct.pack("80sHHHHi", NAME.encode(), *ids, 0)
    dev += struct.pack("64i", *absmax) + struct.pack("64i", *absmin) + struct.pack("64i", *([0] * 64)) + struct.pack("64i", *([0] * 64))
    os.write(fd, dev)
    fcntl.ioctl(fd, UI_DEV_CREATE)
    return fd

def emit(fd, kind, code, value):
    os.write(fd, struct.pack("llHHi", 0, 0, kind, code, value))
    os.write(fd, struct.pack("llHHi", 0, 0, EV_SYN, 0, 0))

def node():
    for _ in range(50):
        for d in sorted(os.listdir("/sys/class/input")):
            if d.startswith("event") and open(f"/sys/class/input/{d}/device/name").read().strip() == NAME:
                major, minor = open(f"/sys/class/input/{d}/dev").read().strip().split(":")
                path = f"/dev/input/{d}"
                os.makedirs("/dev/input", exist_ok=True)
                if not os.path.exists(path):
                    os.mknod(path, 0o660 | 0o020000, os.makedev(int(major), int(minor)))
                os.chmod(path, 0o666)
                return path
        time.sleep(0.1)
    raise SystemExit("the device did not show up")

def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("create", "power"):
        raise SystemExit(__doc__)
    fifo = sys.argv[2]
    if os.path.exists(fifo):
        os.unlink(fifo)
    os.mkfifo(fifo)
    fd = create()
    open(fifo + ".node", "w").write(node() + "\n")
    while True:
        with open(fifo) as f:
            for line in f:
                w = line.split()
                if not w:
                    continue
                if w[0] == "quit":
                    fcntl.ioctl(fd, UI_DEV_DESTROY)
                    return
                code = int(w[1])
                if w[0] in ("tap", "down"):
                    emit(fd, EV_KEY, code, 1)
                if w[0] == "tap":
                    time.sleep(int(w[2] if len(w) > 2 else 120) / 1000)
                if w[0] in ("tap", "up"):
                    emit(fd, EV_KEY, code, 0)

main()
