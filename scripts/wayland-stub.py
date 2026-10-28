#!/usr/bin/env python3
"""A Wayland "compositor" that only says hello, so `pocket-ui --snapshot` runs on a Linux computer without one.

    python3 -I scripts/wayland-stub.py /tmp/wayland-stub &
    WAYLAND_DISPLAY=/tmp/wayland-stub POCKETUI_MOCK=1 target/release/pocket-ui --snapshot settings out.png

A snapshot paints into memory and never opens a window, but winit's event loop (which the app's proxy comes from)
connects to the compositor first and binds wl_compositor, wl_shm and xdg_wm_base. This answers exactly that: the
registry, binds, sync callbacks; every other request is read and ignored. Nothing is ever shown. macOS and a Linux
desktop with a compositor don't need it. Used by tests/i18n_snapshots.rs and scripts/i18n-shots.sh
(docs/DEVELOPING.md "Snapshots").
"""
import os
import socket
import struct
import sys
import threading

GLOBALS = [(1, "wl_compositor", 4), (2, "wl_shm", 1), (3, "xdg_wm_base", 2)]


def wl_string(s):
    b = s.encode() + b"\0"
    return struct.pack("<I", len(b)) + b + b"\0" * (-len(b) % 4)


def message(obj, opcode, payload=b""):
    return struct.pack("<IHH", obj, opcode, 8 + len(payload)) + payload


def serve(conn):
    objects = {1: "wl_display"}
    buf = b""
    serial = 0
    try:
        while True:
            # Plain recv: the file descriptors some requests carry (wl_shm.create_pool) are closed by the kernel.
            data = conn.recv(65536)
            if not data:
                return
            buf += data
            out = b""
            while len(buf) >= 8:
                obj, opcode, size = struct.unpack_from("<IHH", buf)
                if size < 8 or len(buf) < size:
                    break
                body, buf = buf[8:size], buf[size:]
                kind = objects.get(obj)
                if kind == "wl_display" and opcode == 0:  # sync -> callback.done, display.delete_id
                    (cb,) = struct.unpack_from("<I", body)
                    serial += 1
                    out += message(cb, 0, struct.pack("<I", serial)) + message(1, 1, struct.pack("<I", cb))
                elif kind == "wl_display" and opcode == 1:  # get_registry -> the globals
                    (reg,) = struct.unpack_from("<I", body)
                    objects[reg] = "wl_registry"
                    for name, iface, version in GLOBALS:
                        out += message(reg, 0, struct.pack("<I", name) + wl_string(iface) + struct.pack("<I", version))
                elif kind == "wl_registry" and opcode == 0:  # bind(name, interface, version, new_id)
                    n = struct.unpack_from("<I", body, 4)[0]
                    iface = body[8:8 + n - 1].decode()
                    at = 8 + n + (-n % 4)
                    new_id = struct.unpack_from("<I", body, at + 4)[0]
                    objects[new_id] = iface
                    if iface == "wl_shm":  # the two formats every compositor has
                        out += message(new_id, 0, struct.pack("<I", 0)) + message(new_id, 0, struct.pack("<I", 1))
            if out:
                conn.sendall(out)
    except OSError:
        return
    finally:
        conn.close()


def main(path):
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(path)
    srv.listen(64)
    while True:
        conn, _ = srv.accept()
        threading.Thread(target=serve, args=(conn,), daemon=True).start()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: wayland-stub.py <socket path>")
    main(sys.argv[1])
