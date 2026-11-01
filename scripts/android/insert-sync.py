#!/usr/bin/env python3
"""How far a start's sound is from its picture, from one diagnostic launch (scripts/android/insert-smoke.py sync).

Usage: insert-sync.py LOG DIAG_DIR
LOG has the launcher's `insert-frame <monotonic ms> <ms since A>` lines; DIAG_DIR the launcher's capture (`Sink`): the PCM it
gave AAudio and, a few times a second, which frame AAudio says it presented when. Picture: the first frame presented
past the moment. Sound: the capture's onset and loudest moment around it, on AAudio's presentation clock. Positive: the
sound comes after the picture. What neither shows is the display's and the speaker's own delay past that point.
"""
import array
import json
import re
import sys
from pathlib import Path

RATE = 48000
log, diag = Path(sys.argv[1]).read_text(), Path(sys.argv[2])
frames = [(float(a), float(b)) for a, b in re.findall(r'insert-frame ([\d.]+) ([\d.]+)', log)]
assert frames, 'no insert-frame lines: start the launcher with --ez diagnosticAudio true'
press = float(re.search(r'insert: \S+ start cozy=\S+ at ([\d.]+)', log).group(1))
gaps = sorted(b[0] - a[0] for a, b in zip(frames, frames[1:]))
print('frames %d, gap median %.1f ms, worst %.1f ms' % (len(frames), gaps[len(gaps) // 2], gaps[-1]))

best = None
for meta_path in sorted(diag.glob('*.json')):
    meta = json.loads(meta_path.read_text())
    trace = meta.get('trace') or []
    if trace and trace[0][1] <= press + 1000 and trace[-1][1] >= press + 1000:
        best = (meta_path, meta, trace)
assert best, 'no capture covers the start'
meta_path, meta, trace = best
pcm = array.array('h')
pcm.frombytes(meta_path.with_suffix('.s16le').read_bytes())
left = pcm[::2]
stamped = [t for t in trace if t[2] >= 0 and t[3] > 0]
print('capture %s: %d frames, burst %d, buffer %d, xruns %s, AAudio timestamps %d of %d' % (meta_path.stem, len(left), meta['frames_per_burst'], meta['buffer_frames'], meta['xruns'], len(stamped), len(trace)))


def written_at(frame):
    near = min(trace, key=lambda t: abs(t[0] - frame))
    return near[1] + (frame - near[0]) * 1000.0 / RATE


def presented_at(frame):
    """AAudio's clock when it reports one; else when it was written, which is earlier by the buffer."""
    if not stamped:
        return written_at(frame)
    near = min(stamped, key=lambda t: abs(t[2] - frame))
    return near[3] + (frame - near[2]) * 1000.0 / RATE


def frame_for(ms):
    """The frame presented at `ms`: a write runs ahead of that by the output's buffer."""
    if not stamped:
        near = min(trace, key=lambda t: abs(t[1] - ms))
        return int(near[0] + (ms - near[1]) * RATE / 1000.0)
    near = min(stamped, key=lambda t: abs(t[3] - ms))
    return int(near[2] + (ms - near[3]) * RATE / 1000.0)


def event(name, moment, before, after):
    """The picture's first frame past `moment`, and the sound's in the window around it."""
    pic = next((f[0] for f in frames if f[1] >= moment), None)
    a, b = max(0, frame_for(press + moment - before)), min(len(left), frame_for(press + moment + after))
    window = left[a:b]
    if not window or pic is None:
        print(name, 'nothing to measure')
        return
    peak = max(range(len(window)), key=lambda i: abs(window[i]))
    level = abs(window[peak])
    # Onset: where a 2 ms window first reaches a tenth of what follows, going back from the loudest moment.
    step = 96
    onset = peak
    while onset - step > 0 and max(abs(s) for s in window[onset - step:onset]) > level * 0.1:
        onset -= step
    print('%-22s picture %8.1f (late %+5.1f) | sound onset %+6.1f ms, loudest %+6.1f ms after the picture, peak %d' % (
        name, pic - press, pic - press - moment, presented_at(a + onset) - pic, presented_at(a + peak) - pic, level))


event('medium in (1200 ms)', 1200, 120, 120)
event('lid/tray shut (1480 ms)', 1480, 120, 120)
