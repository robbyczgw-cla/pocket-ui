#!/usr/bin/env python3
"""Talks to ARMSX2's PINE socket (runs on the device; EnablePINE must be on).
    pine.py status                 Running / Paused / Shutdown
    pine.py stats                  performance JSON (fps, frame times, thread load)
    pine.py save N | load N        save state to / load from slot N
    pine.py get SECTION KEY        a setting as stored
    pine.py set SECTION KEY VALUE  write a setting and apply it to the running game
    pine.py version | title | id
    pine.py watch SECONDS          one stats line per second, then min/avg of fps and frame time
"""
import json, os, socket, struct, sys, time

SOCK = os.path.join(os.environ.get("XDG_RUNTIME_DIR", "/var/run/0-runtime-dir"), "pcsx2.sock")
OPS = {"version": 8, "save": 9, "load": 10, "title": 11, "id": 12, "status": 15, "stats": 16, "get": 17, "set": 18}


def lp(s):
    b = s.encode()
    return struct.pack("<I", len(b)) + b


def call(op, args=b""):
    body = bytes([op]) + args
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(5)
        s.connect(SOCK)
        s.sendall(struct.pack("<I", len(body) + 4) + body)
        head = s.recv(4)
        size = struct.unpack("<I", head)[0]
        data = b""
        while len(data) < size - 4:
            data += s.recv(size - 4 - len(data))
    if data[0] != 0:
        raise SystemExit("PINE: command failed")
    return data[1:]


def string(reply):
    return reply[4:].split(b"\0")[0].decode()


def main(a):
    cmd = a[0]
    if cmd == "status":
        print(["Running", "Paused", "Shutdown"][struct.unpack("<I", call(15))[0]])
    elif cmd in ("save", "load"):
        call(OPS[cmd], bytes([int(a[1])]))
    elif cmd in ("stats", "version", "title", "id"):
        print(string(call(OPS[cmd])))
    elif cmd == "get":
        print(string(call(17, lp(a[1]) + lp(a[2]))))
    elif cmd == "set":
        print(string(call(18, lp(a[1]) + lp(a[2]) + lp(a[3]))))
    elif cmd == "watch":
        fps, worst = [], []
        for _ in range(int(a[1])):
            st = json.loads(string(call(16)))
            fps.append(st["fps"])
            worst.append(st["frame_ms_max"])
            print("fps %.2f  speed %.1f%%  frame %.2f ms (max %.2f)  ee %.0f%%  gs %.0f%%  vu %.0f%%  gpu %.0f%%" % (
                st["fps"], st["speed"], st["frame_ms_avg"], st["frame_ms_max"], st["cpu_thread_pct"], st["gs_thread_pct"],
                st["vu_thread_pct"], st["gpu_pct"]), flush=True)
            time.sleep(1)
        print("fps min %.2f avg %.2f, worst frame %.1f ms" % (min(fps), sum(fps) / len(fps), max(worst)))
    else:
        print(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:] or ["help"])
