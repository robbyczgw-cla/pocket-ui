#!/usr/bin/env python3
"""A second controller for the virtual Mini V2, as a Bluetooth pad looks to pocket-ui (couch co-op, src/players.rs).
A uinput pad on bus 0x05 (Bluetooth) with its own phys, so `pad::controller_key` makes it a controller of its own
("bt:045e:0b13:<phys>"); force feedback is accepted and logged to /tmp/vpad2-ff.log. Run it inside the container:
  docker cp scripts/vpad-bt.py vdev-<name>:/tmp/ && docker exec -d vdev-<name> python3 /tmp/vpad-bt.py [name] [phys]
Commands on the FIFO /tmp/vpad2.cmd: press A 150 | down A | up A | stick LX LY [RX RY] | quit (the pad goes away)
  docker exec vdev-<name> sh -c 'echo "press MODE 120" > /tmp/vpad2.cmd'
"""
import fcntl, os, struct, sys, threading, time

BTN = {"B": 304, "A": 305, "Y": 308, "X": 307, "L": 310, "R": 311, "L2": 312, "R2": 313, "SELECT": 314, "START": 315,
       "MODE": 316, "UP": 544, "DOWN": 545, "LEFT": 546, "RIGHT": 547}
AXES = {"LX": 0, "LY": 1, "RX": 3, "RY": 4}
NAME = (sys.argv[1] if len(sys.argv) > 1 else "Xbox Wireless Controller").encode()
PHYS = (sys.argv[2] if len(sys.argv) > 2 else "vdev-bt-1").encode()
FIFO, FFLOG = "/tmp/vpad2.cmd", "/tmp/vpad2-ff.log"

fd = os.open("/dev/uinput", os.O_RDWR | os.O_NONBLOCK)
fcntl.ioctl(fd, 0x40045564, 1)
for code in BTN.values():
    fcntl.ioctl(fd, 0x40045565, code)
fcntl.ioctl(fd, 0x40045564, 0x15)
for code in (0x50, 0x51, 0x5A, 0x60):
    fcntl.ioctl(fd, 0x4004556B, code)
fcntl.ioctl(fd, 0x40045564, 3)
for code in AXES.values():
    fcntl.ioctl(fd, 0x40045567, code)
import ctypes
phys_c = ctypes.create_string_buffer(PHYS + b"\0")
fcntl.ioctl(fd, 0x4008556C, ctypes.addressof(phys_c))  # UI_SET_PHYS
absmax, absmin = [0] * 64, [0] * 64
for code in AXES.values():
    absmax[code], absmin[code] = 32767, -32768
os.write(fd, struct.pack("80sHHHHI" + "i" * 256, NAME, 0x05, 0x045E, 0x0B13, 1, 16, *absmax, *absmin, *([0] * 128)))
fcntl.ioctl(fd, 0x5501)


def make_node(ufd):
    b = bytearray(64)
    fcntl.ioctl(ufd, 0x8040552C, b)
    sysname = b.split(b"\0")[0].decode()
    for _ in range(50):
        base = f"/sys/devices/virtual/input/{sysname}"
        events = [d for d in (os.listdir(base) if os.path.isdir(base) else []) if d.startswith("event")]
        if events:
            major, minor = map(int, open(f"{base}/{events[0]}/dev").read().strip().split(":"))
            node = f"/dev/input/{events[0]}"
            if not os.path.exists(node):
                os.mknod(node, 0o660 | 0o020000, os.makedev(major, minor))
            return node
        time.sleep(0.1)
    return None


node = make_node(fd)
UI_BEGIN_FF_UPLOAD, UI_END_FF_UPLOAD = 0xC06855C8, 0x406855C9
UI_BEGIN_FF_ERASE, UI_END_FF_ERASE = 0xC00C55CA, 0x400C55CB


def ff_log(text):
    with open(FFLOG, "a") as f:
        f.write(f"{time.time():.3f} {text}\n")


def ff_loop():
    import select
    while True:
        select.select([fd], [], [], 1.0)
        try:
            data = os.read(fd, 24 * 16)
        except (BlockingIOError, OSError):
            continue
        for i in range(0, len(data) - 23, 24):
            _, _, etype, code, value = struct.unpack("llHHi", data[i:i + 24])
            if etype == 0x0101 and code == 1:
                b = bytearray(struct.pack("Ii", value, 0) + bytes(96))
                fcntl.ioctl(fd, UI_BEGIN_FF_UPLOAD, b, True)
                eff = bytes(b[8:56])
                kind, eid, _ = struct.unpack("HhH", eff[:6])
                if kind == 0x50:
                    strong, weak = struct.unpack("HH", eff[16:20])
                    ff_log(f"upload id={eid} rumble strong={strong} weak={weak}")
                struct.pack_into("i", b, 4, 0)
                fcntl.ioctl(fd, UI_END_FF_UPLOAD, b, True)
            elif etype == 0x0101 and code == 2:
                b = bytearray(struct.pack("Iii", value, 0, 0))
                fcntl.ioctl(fd, UI_BEGIN_FF_ERASE, b, True)
                ff_log("erase")
                struct.pack_into("i", b, 4, 0)
                fcntl.ioctl(fd, UI_END_FF_ERASE, b, True)
            elif etype == 0x15 and code != 0x60:
                ff_log(f"{'play' if value else 'stop'} id={code}")


open(FFLOG, "w").close()
threading.Thread(target=ff_loop, daemon=True).start()


def emit(t, code, value):
    os.write(fd, struct.pack("llHHi", 0, 0, t, code, value))
    os.write(fd, struct.pack("llHHi", 0, 0, 0, 0, 0))


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
            if w[0] == "quit":
                fcntl.ioctl(fd, 0x5502)  # UI_DEV_DESTROY
                os.close(fd)
                if node and os.path.exists(node):
                    os.remove(node)
                os.remove(FIFO)
                print("gone", flush=True)
                sys.exit(0)
            try:
                if w[0] == "press":
                    emit(1, BTN[w[1].upper()], 1)
                    time.sleep(int(w[2] if len(w) > 2 else 100) / 1000)
                    emit(1, BTN[w[1].upper()], 0)
                elif w[0] in ("down", "up"):
                    emit(1, BTN[w[1].upper()], 1 if w[0] == "down" else 0)
                elif w[0] == "stick":
                    for name, v in zip(("LX", "LY", "RX", "RY"), w[1:]):
                        emit(3, AXES[name], int(max(-1.0, min(1.0, float(v))) * 32767))
            except (KeyError, ValueError) as e:
                print("bad command:", line.strip(), e, flush=True)
