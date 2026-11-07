#!/usr/bin/env python3
"""Opt-in Linux/uinput proof; only creates/destroys its own test pads. Run as root with a pocket-ui binary.
Emulates two identical SN30 D-input pads and checks the actual standalone helper through SDL2.
"""
import ctypes
import fcntl
import json
import os
from pathlib import Path
import signal
import struct
import subprocess
import sys
import tempfile
import threading
import re
import time


def probe(output):
    sdl = ctypes.CDLL('libSDL2-2.0.so.0')
    sdl.SDL_GameControllerOpen.restype = ctypes.c_void_p
    sdl.SDL_GameControllerGetButton.argtypes = [ctypes.c_void_p, ctypes.c_int]
    sdl.SDL_GameControllerGetAxis.argtypes = [ctypes.c_void_p, ctypes.c_int]
    sdl.SDL_GameControllerGetAxis.restype = ctypes.c_int16
    sdl.SDL_JoystickNameForIndex.restype = ctypes.c_char_p
    assert sdl.SDL_Init(0x2200) == 0
    count = sdl.SDL_NumJoysticks()
    assert count == 4, f'SDL saw {count} joysticks instead of the four stable seats'
    names = [sdl.SDL_JoystickNameForIndex(i).decode() for i in range(count)]
    assert names == ['pocket-ui Controller'] * 4, names
    pads = [sdl.SDL_GameControllerOpen(i) for i in range(count)]
    assert all(pads), 'a bridge pad has no SDL mapping'
    output = Path(output)
    for _ in range(800):
        if output.with_suffix('.stop').exists():
            return
        sdl.SDL_GameControllerUpdate()
        state = [{'a': sdl.SDL_GameControllerGetButton(p, 0),
                  'l2': sdl.SDL_GameControllerGetAxis(p, 4),
                  'rx': sdl.SDL_GameControllerGetAxis(p, 2)} for p in pads]
        temp = output.with_suffix('.new')
        temp.write_text(json.dumps(state))
        temp.replace(output)
        time.sleep(.025)
    raise AssertionError('probe deadline exceeded')


class Pad:
    def __init__(self, identity):
        self.created = False
        self.node = None
        self.fd = os.open('/dev/uinput', os.O_RDWR | os.O_NONBLOCK)
        fcntl.ioctl(self.fd, 0x40045564, 1)
        for key in range(304, 319):
            fcntl.ioctl(self.fd, 0x40045565, key)
        fcntl.ioctl(self.fd, 0x40045564, 3)
        axes = [0, 1, 2, 5, 16, 17]
        lo, hi = [0] * 64, [0] * 64
        for axis in axes:
            fcntl.ioctl(self.fd, 0x40045567, axis)
            lo[axis], hi[axis] = (-1, 1) if axis >= 16 else (0, 255)
        phys = ctypes.create_string_buffer(identity.encode())
        assert ctypes.CDLL(None).ioctl(self.fd, ctypes.c_ulong(0x4008556C), ctypes.cast(phys, ctypes.c_void_p)) == 0
        os.write(self.fd, struct.pack('80sHHHHI' + 'i' * 256,
                                     b'pocket-bt-proof', 5, 0x2dc8, 0x6101, 0x100, 0,
                                     *hi, *lo, *([0] * 128)))
        fcntl.ioctl(self.fd, 0x5501)
        name = bytearray(64)
        fcntl.ioctl(self.fd, 0x8040552C, name)
        base = Path('/sys/devices/virtual/input') / name.split(b'\0')[0].decode()
        for _ in range(100):
            events = list(base.glob('event*'))
            if events:
                event = events[0]
                self.node = Path('/dev/input') / event.name
                if not self.node.exists():
                    major, minor = map(int, (event / 'dev').read_text().strip().split(':'))
                    os.mknod(self.node, 0o20600, os.makedev(major, minor))
                    self.created = True
                break
            time.sleep(.01)
        assert self.node is not None, 'test pad has no evdev node'

    def emit(self, kind, code, value):
        os.write(self.fd, struct.pack('llHHi', 0, 0, kind, code, value))
        os.write(self.fd, struct.pack('llHHi', 0, 0, 0, 0, 0))

    def close(self):
        if self.fd is not None:
            fcntl.ioctl(self.fd, 0x5502)
            os.close(self.fd)
            self.fd = None
        if self.created and self.node.exists():
            self.node.unlink()


def wait_for(state, predicate, label):
    deadline = time.monotonic() + 5
    got = None
    while time.monotonic() < deadline:
        if state.exists():
            got = json.loads(state.read_text())
            if predicate(got):
                print('PASS:', label, flush=True)
                return
        time.sleep(.025)
    raise AssertionError(f'{label}: last SDL state {got}')


def main(binary):
    assert os.geteuid() == 0, 'requires root for uinput (no physical pads are used)'
    with tempfile.TemporaryDirectory(prefix='pocket-bt-proof-') as folder:
        data = Path(folder)
        state = data / 'sdl.json'
        roles = ['SOUTH','EAST','NORTH','WEST','L1','R1','L2','R2','L3','R3','SELECT','START','UP','DOWN','LEFT','RIGHT']
        roles[0], roles[6] = roles[6], roles[0]
        key = lambda name: f'bt:2dc8:6101:pocket-bt-proof-{name}'
        (data / 'controllers.json').write_text(json.dumps({'devices': {key('a'): {'roles': roles}}}))
        (data / 'players.json').write_text(json.dumps({'order': ['builtin', key('a'), key('b')]}))
        pads = []
        process = None
        nodes = []
        watch_stop = threading.Event()
        watcher = None
        try:
            a, b = Pad('pocket-bt-proof-a'), Pad('pocket-bt-proof-b')
            pads.extend([a, b])
            env = dict(os.environ, POCKETUI_DATA=str(data), SDL_VIDEODRIVER='dummy', SDL_AUDIODRIVER='dummy', SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS='1')
            with (data / 'bridge.log').open('w') as log:
                process = subprocess.Popen([str(Path(binary).resolve()), '--controller-bridge', sys.executable, str(Path(__file__).resolve()), '--probe', str(state)], env=env, stdout=log, stderr=log, start_new_session=True)
                def make_bridge_nodes():
                    # This container has no udev. Create only this helper's four nodes, by its
                    # reserved product IDs; never change a node that already exists.
                    while not watch_stop.wait(.01):
                        text = (data / 'bridge.log').read_text()
                        match = re.search(r'controller bridge: ids (\d+)\.\.(\d+)', text)
                        if not match:
                            continue
                        products = range(int(match[1]), int(match[2]) + 1)
                        for event in Path('/sys/devices/virtual/input').glob('input*/event*'):
                            parent = event.parent
                            try:
                                if (parent / 'name').read_text().strip() != 'pocket-ui Controller':
                                    continue
                                if int((parent / 'id/vendor').read_text().strip(), 16) != 0x1209 or int((parent / 'id/product').read_text().strip(), 16) not in products:
                                    continue
                                node = Path('/dev/input') / event.name
                                if not node.exists():
                                    major, minor = map(int, (event / 'dev').read_text().strip().split(':'))
                                    os.mknod(node, 0o20600, os.makedev(major, minor))
                                    nodes.append((node, parent))
                            except FileNotFoundError:
                                continue
                watcher = threading.Thread(target=make_bridge_nodes)
                watcher.start()
                try:
                    wait_for(state, lambda s: len(s) == 4, 'SDL sees only stable virtual seats')
                    a.emit(1, 312, 1)
                    b.emit(1, 312, 1)
                    wait_for(state, lambda s: s[1]['a'] and s[2]['l2'] > 30000 and not s[1]['l2'], 'two identical pads keep different trigger mappings')
                    b.emit(1, 312, 0)
                    wait_for(state, lambda s: s[1]['a'] and s[2]['l2'] == 0, 'releasing pad B keeps pad A held')
                    a.close()
                    wait_for(state, lambda s: not s[1]['a'] and s[2]['l2'] == 0, 'disconnect clears only the departed pad')
                    a = Pad('pocket-bt-proof-a')
                    pads.append(a)
                    time.sleep(1.2)
                    a.emit(1, 312, 1)
                    wait_for(state, lambda s: s[1]['a'] and not s[2]['a'], 'reconnect retains identity, player and mapping')
                    a.emit(3, 2, 255)
                    wait_for(state, lambda s: s[1]['rx'] > 30000 and s[1]['l2'] == 0, 'D-input right stick stays a stick')
                    state.with_suffix('.stop').touch()
                    assert process.wait(timeout=5) == 0
                except BaseException:
                    print((data / 'bridge.log').read_text(), file=sys.stderr)
                    raise
        finally:
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=5)
            watch_stop.set()
            if watcher is not None:
                watcher.join()
            for pad in pads:
                pad.close()
            for node, parent in nodes:
                if node.exists() and not parent.exists():
                    node.unlink()


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--probe':
        probe(sys.argv[2])
    else:
        main(sys.argv[1])
