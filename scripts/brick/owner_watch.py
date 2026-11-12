"""Bounded, non-grabbing evdev watch for a private Brick session (host Python)."""
import os
import signal
import struct
import subprocess
import threading

EVENT = struct.Struct('<qqHHi')


def pressed(record):
    _, _, kind, code, value = EVENT.unpack(record)
    return (kind == 1 and value == 1) or (kind == 3 and value != 0)


class OwnerWatch:
    def __init__(self, remote, work, lifetime=530):
        self.remote, self.work, self.lifetime = remote, work, lifetime
        self.ending = threading.Event()
        self.children = []
        self.threads = []

    def start(self):
        for node in (0, 1, 3):
            # timeout owns the same PID recorded by the shell. Never grab the input node.
            command = f'echo $$ >{self.work}/input-{node}.pid; exec timeout {self.lifetime} cat /dev/input/event{node}'
            child = subprocess.Popen(self.remote.ssh + [command], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            self.children.append(child)
            thread = threading.Thread(target=self.read, args=(child,), daemon=True)
            self.threads.append(thread)
            thread.start()

    def read(self, child):
        while not self.ending.is_set():
            record = child.stdout.read(EVENT.size)
            if len(record) != EVENT.size:
                return
            if pressed(record) and not self.ending.is_set():
                os.kill(os.getpid(), signal.SIGUSR2)
                return

    def stop(self):
        self.ending.set()
        for node in (0, 1, 3):
            self.remote.run(f'p=$(cat {self.work}/input-{node}.pid 2>/dev/null); '
                            f'if [ -n "$p" ] && [ -r /proc/$p/cmdline ] && '
                            f'tr "\\000" " " </proc/$p/cmdline | grep -Fq "cat /dev/input/event{node}"; '
                            'then kill "$p"; fi; true', check=False)
        for child in self.children:
            if child.poll() is None:
                child.terminate()
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        for thread in self.threads:
            thread.join(timeout=1)
