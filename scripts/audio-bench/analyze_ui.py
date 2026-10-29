#!/usr/bin/env python3
"""Gaps in UI sounds, per phase (ui-after-game.py's OUT.wav and OUT.events; README.md "UI sounds after a game").

A UI sound is short and not a sine, so analyze.py's measures don't apply. What a buffer shorter than the graph's cycle
does to it is plain to see: runs of exact digital zeros inside the sound, where PipeWire found nothing to play (a sound
has no such run of its own: it rings down through its end). Counted: runs of at least MIN_RUN zero frames (both
channels), no longer than a cycle (MAX_RUN), with sound on both sides within EDGE ms and the wave cut off at an edge
(above CUT; a quiet tail passes through zero smoothly). Exits 1 when any phase has a gap.
Usage: analyze_ui.py OUT   (reads OUT.wav and OUT.events)
"""
import sys
import wave

import numpy as np

MIN_RUN = 16  # frames: 0.3 ms; a gap left by a short buffer is a part of a cycle (hundreds of frames)
MAX_RUN = 2048  # frames: a gap is at most one cycle; longer zeros are the silence between two sounds
EDGE = 0.015  # s: sound within this before and after the run
LOUD = 30  # LSB that count as sound
CUT = 100  # LSB right before or after the run: the sound was cut off there, not ringing quietly through zero


def zero_runs(x):
    """(start, length) of each run of frames that are zero in both channels."""
    z = np.all(x == 0, axis=1).astype(np.int8)
    d = np.diff(np.concatenate([[0], z, [0]]))
    starts, ends = np.flatnonzero(d == 1), np.flatnonzero(d == -1)
    return list(zip(starts, ends - starts))


def main():
    out = sys.argv[1]
    with wave.open(out + ".wav") as w:
        rate = w.getframerate()
        x = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").reshape(-1, w.getnchannels()).astype(np.int32)
    phases = []
    for line in open(out + ".events"):
        t, what = line.rstrip("\n").split("\t", 1)
        phases.append((float(t), what))
    loud = np.any(np.abs(x) > LOUD, axis=1)
    edge = int(EDGE * rate)
    gaps = []
    for start, length in zero_runs(x):
        if length < MIN_RUN or length > MAX_RUN:
            continue
        before = loud[max(0, start - edge) : start].any()
        after = loud[start + length : start + length + edge].any()
        # A quiet tail at a low volume holds zero for a few dozen frames on its way through it, smoothly; a gap cuts
        # the wave where it stands.
        edge_level = max(np.abs(x[start - 1]).max() if start else 0, np.abs(x[start + length]).max() if start + length < len(x) else 0)
        if before and after and edge_level > CUT:
            gaps.append((start / rate, length))
    ok = True
    for i, (t, what) in enumerate(phases):
        end = phases[i + 1][0] if i + 1 < len(phases) else len(x) / rate
        inside = [g for g in gaps if t <= g[0] < end]
        sound_s = loud[int(t * rate) : int(end * rate)].sum() / rate
        frames = sum(g[1] for g in inside)
        print(f"{t:7.2f}-{end:7.2f} s  {what:<36} sound {sound_s:5.2f} s  gaps {len(inside):4d}  ({frames / rate * 1000:.1f} ms)")
        ok &= not inside
    print(f"total: {len(gaps)} gaps, {sum(g[1] for g in gaps) / rate * 1000:.1f} ms" + ("" if ok else "  FAIL"))
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
