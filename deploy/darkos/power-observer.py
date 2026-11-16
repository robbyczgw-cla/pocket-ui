#!/usr/bin/env python3
"""Container test: an already-open host reader, like ogage, observes power events (no actions)."""
import fcntl
import os
import struct
import sys

node, log = sys.argv[1:]
fd = os.open(node, os.O_RDONLY)
open(log + '.ready', 'w').close()
with open(log, 'a', buffering=1) as out:
    while True:
        event = os.read(fd, struct.calcsize('llHHi'))
        if not event:
            break
        _, _, kind, code, value = struct.unpack('llHHi', event)
        if kind == 1 and code == 116:
            out.write(f'{value}\n')
