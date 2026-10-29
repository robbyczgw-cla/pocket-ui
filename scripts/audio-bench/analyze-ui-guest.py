#!/usr/bin/env python3
"""Align each UI click to its WAV reference; count padding and residual steps.

Clicks have no single fundamental: report reference-relative distortion+noise,
not a sine THD+N figure. Fit gain, but never stretch time or remove missing frames.
Exit 1 if any click exceeds -40 dB D+N or has padded audible frames. The first click after the stream opens (at the
start, in the guest's pause menu, after the quiet) is its own group: it plays while the shelf's quantum is still being
set (the "first-click transient").
"""
import json
import sys
from pathlib import Path

import numpy as np

from analyze import load


def analyze(path):
    x, rate = load(path)
    ref, _ = load(str(Path(__file__).resolve().parents[2] / "assets/ui-sounds/nav.wav"))
    results = {"first": [], "baseline": [], "guest_first": [], "guest": [], "post_60s": [], "recovery_first": [], "recovery": []}
    for n in range(312):
        if 16 <= n < 20 or 280 <= n < 292:
            continue
        # Recorder starts 0.3 s before the UI; allow stream-opening latency.
        lo = int((0.2 + n * .25) * rate)
        hi = min(len(x), lo + int(.35 * rate))
        seg = x[lo:hi]
        size = 1 << (len(seg) + len(ref) - 1).bit_length()
        corr = np.fft.irfft(np.fft.rfft(seg, size) * np.conj(np.fft.rfft(ref, size)), size)
        at = int(np.argmax(corr[:len(seg) - len(ref) + 1]))
        got = seg[at:at + len(ref)]
        gain = np.dot(got, ref) / np.dot(ref, ref)
        expected = ref * gain
        residual = got - expected
        dn = float(10 * np.log10(max(np.dot(residual, residual), 1e-9) / max(np.dot(expected, expected), 1e-9)))
        audible = np.abs(expected) > 200
        zeros = int(np.sum((got == 0) & audible))
        bad = np.flatnonzero(np.abs(np.diff(residual)) > 200)
        events = int(np.sum(np.diff(bad) > 1) + 1) if len(bad) else 0
        first = {0: "first", 20: "guest_first", 292: "recovery_first"}
        key = first.get(n) or ("baseline" if n < 20 else "guest" if n < 40 else "post_60s" if n < 280 else "recovery")
        results[key].append((dn, zeros, events))
    summary = {}
    for key, values in results.items():
        a = np.array(values)
        summary[key] = dict(clicks=len(values), dn_worst_db=round(float(a[:, 0].max()), 2),
                            dn_median_db=round(float(np.median(a[:, 0])), 2),
                            padded_audible_frames=int(a[:, 1].sum()), discontinuities=int(a[:, 2].sum()))
    print(json.dumps({"file": path, **summary}, indent=2))
    return all(v["dn_worst_db"] <= -40 and v["padded_audible_frames"] == 0 for v in summary.values())


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit("usage: analyze-ui-guest.py FILE.wav [...]")
    sys.exit(0 if all([analyze(p) for p in sys.argv[1:]]) else 1)
