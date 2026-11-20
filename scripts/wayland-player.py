#!/usr/bin/env python3
"""Headless software-EGL endpoint for the real pocket-core journey.

Unlike wayland-stub.py's registry-only snapshot endpoint, this accepts real
xdg-shell windows and Mesa wl_shm buffers. Rendering stays in pocket-core's
ordinary GL renderer. No input, display server, GPU or Weston installation is
needed. Frame callbacks acknowledge commits; the player's own clock sets pace.
"""
import array
import importlib.util
import mmap
import os
import socket
import struct
import sys
import time
import zlib

spec = importlib.util.spec_from_file_location("registry", os.path.join(os.path.dirname(__file__), "wayland-stub.py"))
registry = importlib.util.module_from_spec(spec)
spec.loader.exec_module(registry)
message, wl_string = registry.message, registry.wl_string


def u32(*values):
    return struct.pack("<" + "I" * len(values), *values)


def serve(conn):
    objects = {1: "wl_display"}
    surfaces, shells, pools, buffers = {}, {}, {}, {}
    pending_fds, received = [], b""
    serial = 0
    try:
        while True:
            data, controls, _, _ = conn.recvmsg(65536, socket.CMSG_SPACE(64 * 4))
            if not data:
                return
            for level, kind, payload in controls:
                if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
                    fds = array.array("i")
                    fds.frombytes(payload[:len(payload) - len(payload) % fds.itemsize])
                    pending_fds.extend(fds)
            received += data
            out = b""
            while len(received) >= 8:
                obj, opcode, size = struct.unpack_from("<IHH", received)
                if size < 8:
                    raise ValueError("invalid Wayland message size")
                if len(received) < size:
                    break
                body, received = received[8:size], received[size:]
                kind = objects.get(obj)
                values = struct.unpack("<" + "I" * (len(body) // 4), body)
                if kind == "wl_display" and opcode == 0:
                    serial += 1
                    out += message(values[0], 0, u32(serial)) + message(1, 1, u32(values[0]))
                elif kind == "wl_display" and opcode == 1:
                    reg = values[0]
                    objects[reg] = "wl_registry"
                    for name, iface, version in registry.GLOBALS:
                        out += message(reg, 0, u32(name) + wl_string(iface) + u32(version))
                elif kind == "wl_registry" and opcode == 0:
                    n = values[1]
                    iface = body[8:8 + n - 1].decode()
                    new_id = struct.unpack_from("<I", body, 12 + n + (-n % 4))[0]
                    objects[new_id] = iface
                    if iface == "wl_shm":
                        out += message(new_id, 0, u32(0)) + message(new_id, 0, u32(1))
                elif kind == "wl_compositor":
                    objects[values[0]] = "wl_surface" if opcode == 0 else "wl_region"
                    if opcode == 0:
                        surfaces[values[0]] = {"callbacks": [], "buffer": 0, "configured": False}
                elif kind == "xdg_wm_base" and opcode == 2:
                    objects[values[0]] = "xdg_surface"
                    shells[values[0]] = values[1]
                    surfaces[values[1]]["shell"] = values[0]
                elif kind == "xdg_surface" and opcode == 1:
                    objects[values[0]] = "xdg_toplevel"
                    surfaces[shells[obj]]["toplevel"] = values[0]
                elif kind == "wl_shm" and opcode == 0:
                    objects[values[0]] = "wl_shm_pool"
                    pools[values[0]] = pending_fds.pop(0)
                elif kind == "wl_shm_pool" and opcode == 0:
                    buffer_id, offset, width, height, stride, fmt = values
                    objects[buffer_id] = "wl_buffer"
                    buffers[buffer_id] = (os.dup(pools[obj]), offset, height * stride)
                elif kind == "wl_shm_pool" and opcode == 1:
                    os.close(pools.pop(obj))
                elif kind == "wl_buffer" and opcode == 0:
                    os.close(buffers.pop(obj)[0])
                elif kind == "wl_surface":
                    surface = surfaces[obj]
                    if opcode == 1:
                        surface["buffer"] = values[0]
                    elif opcode == 3:
                        surface["callbacks"].append(values[0])
                    elif opcode == 6:
                        if not surface["configured"] and "toplevel" in surface:
                            serial += 1
                            # Activated, compositor chooses the client's requested dimensions.
                            out += message(surface["toplevel"], 0, u32(0, 0, 4, 4))
                            out += message(surface["shell"], 0, u32(serial))
                            surface["configured"] = True
                        buffer_id = surface["buffer"]
                        if buffer_id:
                            fd, offset, length = buffers[buffer_id]
                            with mmap.mmap(fd, offset + length, access=mmap.ACCESS_READ) as pixels:
                                checksum = zlib.crc32(pixels[offset:offset + length])
                            print(f"buffer {time.monotonic():.6f} {length} {checksum}", flush=True)
                            out += message(buffer_id, 0)
                            surface["buffer"] = 0
                        for callback in surface["callbacks"]:
                            out += message(callback, 0, u32(int(time.monotonic() * 1000) & 0xffffffff))
                            out += message(1, 1, u32(callback))
                        surface["callbacks"].clear()
            if out:
                conn.sendall(out)
    except (BrokenPipeError, ConnectionResetError):
        pass
    finally:
        for fd in pending_fds + list(pools.values()) + [buffer[0] for buffer in buffers.values()]:
            os.close(fd)
        conn.close()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: wayland-player.py <socket path>")
    registry.serve = serve
    registry.main(sys.argv[1])
