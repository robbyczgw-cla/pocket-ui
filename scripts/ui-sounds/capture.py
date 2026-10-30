#!/usr/bin/env python3
"""On the device: records a sink's monitor while a command plays, and notes when the recording's first sample was.

    capture.py OUT TARGET -- COMMAND...

Writes OUT.wav (s16, stereo, 48 kHz), OUT.start (CLOCK_MONOTONIC µs of the first sample, estimated from how fast the
file grows: the earliest `now - frames written / rate` over the first 1.5 s, so late writes only make it later) and
OUT.events (the command's stdout). device-capture.sh runs it; analyze_capture.py reads it.
"""
import os
import subprocess
import sys
import threading
import time


def main():
    out, target = sys.argv[1], sys.argv[2]
    cmd = sys.argv[sys.argv.index("--") + 1 :]
    wav = out + ".wav"
    rec = subprocess.Popen(
        ["pw-record", "--target", target, "-P", "{ stream.capture.sink = true node.name = media_sounds_rec }", "--rate", "48000", "--channels", "2", "--format", "s16", wav]
    )
    best = [None]

    def poll():
        end = time.monotonic() + 3.0
        while time.monotonic() < end:
            now = time.monotonic()
            try:
                size = os.path.getsize(wav)
            except OSError:
                size = 0
            if size > 44:
                est = now - (size - 44) / 4 / 48000
                if best[0] is None or est < best[0]:
                    best[0] = est
            time.sleep(0.002)

    th = threading.Thread(target=poll)
    th.start()
    time.sleep(1.0)
    with open(out + ".events", "w") as ev:
        subprocess.run(cmd, stdout=ev, check=False)
    th.join()
    time.sleep(0.3)
    rec.terminate()
    rec.wait()
    with open(out + ".start", "w") as f:
        f.write(f"{int(best[0] * 1_000_000) if best[0] else 0}\n")


if __name__ == "__main__":
    main()
