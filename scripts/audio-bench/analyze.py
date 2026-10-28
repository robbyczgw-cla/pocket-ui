#!/usr/bin/env python3
"""Measures a recording of `audio_tone` (a steady sine through pocket-core's audio path), see README.md.

Prints one line per file and exits 1 when a limit is broken:
- glitches: places where the waveform leaves the sine (a dropout, a padded or repeated block, a click), over the whole
  tone including its start, the pauses' fades and its end. A pure sine obeys x[n+1] + x[n-1] = 2cos(w) x[n]; the
  residual stays at a few LSB for a clean tone, about 60 at the corner of a 5 ms fade, and jumps at any gap or step.
- THD+N: residual after a least-squares sine fit per 50 ms block, worst block reported.
- pitch: frequency from zero crossings over 20-period windows, deviation from the median in cents (RMS and max).
THD+N and pitch only count where the tone has been playing for 1 s and goes on for 0.2 s (the rate control settles
after a start or a pause by design).
Usage: analyze.py [--expect HZ] FILE.wav ...   (16-bit, 48 kHz, as run-case.sh records)
"""
import sys
import wave

import numpy as np

GLITCH_LSB = 200
LIMITS = {"glitches": 0, "thdn_db": -60.0, "cents_rms": 1.0, "cents_max": 3.0}
SETTLE_S, TAIL_S = 1.0, 0.2


def load(path):
    with wave.open(path) as w:
        assert w.getsampwidth() == 2 and w.getframerate() == 48000, "need s16, 48 kHz"
        x = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(np.float64)
        return x.reshape(-1, w.getnchannels())[:, 0], 48000


def zero_crossings(x):
    i = np.flatnonzero((x[:-1] < 0) & (x[1:] >= 0))
    return i + (-x[i] / (x[i + 1] - x[i]))


def sine_fit_residual(x, rate, f):
    # The t*sin and t*cos columns (and t^2) take up a small frequency or level drift within the block (the rate
    # control's slow steering), so only what is not the tone stays in the residual.
    t = np.arange(len(x)) / rate
    s, c = np.sin(2 * np.pi * f * t), np.cos(2 * np.pi * f * t)
    tc = t - t.mean()
    a = np.column_stack([s, c, tc * s, tc * c, tc * tc * s, tc * tc * c, np.ones_like(t)])
    coef, *_ = np.linalg.lstsq(a, x, rcond=None)
    return x - a @ coef


def steady_mask(x, rate):
    """Per sample: the tone has played for SETTLE_S and goes on for TAIL_S (10 ms blocks with a peak above 1000)."""
    blk = rate // 100
    n = len(x) // blk
    on = np.abs(x[: n * blk]).reshape(n, blk).max(axis=1) > 1000
    run_before = np.zeros(n, int)
    for i in range(n):
        run_before[i] = run_before[i - 1] + 1 if on[i] and i else int(on[i])
    run_after = np.zeros(n, int)
    for i in range(n - 1, -1, -1):
        run_after[i] = run_after[i + 1] + 1 if on[i] and i < n - 1 else int(on[i])
    ok = (run_before >= SETTLE_S * 100) & (run_after >= TAIL_S * 100)
    return np.repeat(ok, blk), on


def analyze(path, expect):
    x, rate = load(path)
    steady, on = steady_mask(x, rate)
    active = np.flatnonzero(on)
    if not len(active):
        return None
    if not steady.any():
        # Broken all through (the dropouts): the glitches still count, pitch and THD+N over all of it.
        steady = np.repeat(on, rate // 100)
    a, b = active[0] * (rate // 100), (active[-1] + 1) * (rate // 100)
    zc_all = zero_crossings(x[: len(steady)])
    zc = zc_all[steady[zc_all.astype(int)]]
    f_med = rate / np.median(np.diff(zc))
    w = 2 * np.pi * f_med / rate
    # Glitches over the whole tone, start, fades and end included.
    seg = x[a:b]
    resid = seg[2:] + seg[:-2] - 2 * np.cos(w) * seg[1:-1]
    bad = np.flatnonzero(np.abs(resid) > GLITCH_LSB)
    events = int(np.sum(np.diff(bad) > rate // 100)) + 1 if len(bad) else 0
    # Pitch over 20 periods, only windows that lie in one steady stretch.
    n = 20
    ends = (zc_all[n:], zc_all[:-n])
    f = n * rate / (ends[0] - ends[1])
    keep = steady[ends[0].astype(int)] & steady[ends[1].astype(int)] & ((ends[0] - ends[1]) < 1.5 * n * rate / f_med)
    cents = 1200 * np.log2(f[keep] / f_med)
    thdn = []
    blen = rate // 20
    for s in range(0, len(steady) - blen, blen):
        if not steady[s : s + blen].all():
            continue
        blk = x[s : s + blen]
        z = zero_crossings(blk)
        fb = rate * (len(z) - 1) / (z[-1] - z[0]) if len(z) > 2 else f_med
        r = sine_fit_residual(blk, rate, fb)
        thdn.append(20 * np.log10(np.sqrt(np.mean(r**2)) / np.sqrt(max(np.mean(blk**2) - np.mean(r**2), 1e-9))))
    return {
        "seconds": (b - a) / rate,
        "steady_s": steady.sum() / rate,
        "glitches": events,
        "glitch_samples": len(bad),
        "first_glitch_s": round((bad[0] + a) / rate, 3) if len(bad) else None,
        "thdn_db": max(thdn),
        "thdn_median_db": float(np.median(thdn)),
        "cents_rms": float(np.sqrt(np.mean(cents**2))),
        "cents_max": float(np.max(np.abs(cents))),
        "freq": f_med,
        "offset_cents": 1200 * np.log2(f_med / expect) if expect else 0.0,
    }


def main():
    args = sys.argv[1:]
    expect = None
    if args[:1] == ["--expect"]:
        expect, args = float(args[1]), args[2:]
    ok = True
    for path in args:
        r = analyze(path, expect)
        if r is None:
            print(f"{path}: no tone  FAIL")
            ok = False
            continue
        fails = [k for k, lim in LIMITS.items() if r[k] > lim]
        ok &= not fails
        print(
            f"{path}: tone {r['seconds']:.1f} s ({r['steady_s']:.1f} s steady)  glitches {r['glitches']} "
            f"({r['glitch_samples']} samples, first at {r['first_glitch_s']} s)  "
            f"THD+N worst {r['thdn_db']:.1f} dB (median {r['thdn_median_db']:.1f})  "
            f"pitch {r['freq']:.3f} Hz, wobble rms {r['cents_rms']:.3f} / max {r['cents_max']:.3f} cents"
            + (f", {r['offset_cents']:+.2f} cents off" if expect else "")
            + ("  FAIL " + ",".join(fails) if fails else "  ok")
        )
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
