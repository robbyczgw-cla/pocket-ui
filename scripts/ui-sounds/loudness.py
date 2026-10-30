#!/usr/bin/env python3
"""Loudness, true peak and edges of short sounds (mono or stereo 16-bit WAV), numpy only.

    python3 scripts/ui-sounds/loudness.py assets/ui-sounds/*.wav

- M max: the loudest 400 ms momentary loudness (EBU R128 / ITU-R BS.1770-4 K-weighting), in LUFS, 10 ms steps. Short
  sounds are compared by it: a 100 ms click and a 600 ms whir of the same M max sound about as loud.
- M spk: the same after a 400 Hz high-pass, about what a handheld's speaker plays of it.
- TP: true peak in dBTP, 4x oversampled (BS.1770 Annex 2: windowed-sinc interpolation, 48 taps per phase).
- peak: the sample peak in dBFS; DC: the mean in dBFS; first/last: the first and last sample, and the largest step
  between two samples in the first and last 2 ms (a click at the edges shows there).
"""
import sys
import wave

import numpy as np


def read(path):
    with wave.open(path, "rb") as w:
        rate, ch, width = w.getframerate(), w.getnchannels(), w.getsampwidth()
        assert width == 2, f"{path}: {8 * width}-bit"
        x = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(np.float64) / 32768.0
    return x.reshape(-1, ch), rate


def biquad(x, b, a):
    y = np.zeros_like(x)
    x1 = x2 = y1 = y2 = 0.0
    b0, b1, b2 = b
    _, a1, a2 = a
    for i, v in enumerate(x):
        o = b0 * v + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        x2, x1, y2, y1 = x1, v, y1, o
        y[i] = o
    return y


def k_weight(x, rate):
    """BS.1770's two stages, designed for `rate` (the published 48 kHz coefficients at 48 kHz)."""
    import math

    f0, g, q = 1681.974450955533, 3.999843853973347, 0.7071752369554196
    k = math.tan(math.pi * f0 / rate)
    vh = 10 ** (g / 20)
    vb = vh ** 0.4996667741545416
    a0 = 1 + k / q + k * k
    b = ((vh + vb * k / q + k * k) / a0, 2 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0)
    a = (1.0, 2 * (k * k - 1) / a0, (1 - k / q + k * k) / a0)
    y = biquad(x, b, a)
    f0, q = 38.13547087602444, 0.5003270373238773
    k = math.tan(math.pi * f0 / rate)
    a0 = 1 + k / q + k * k
    a = (1.0, 2 * (k * k - 1) / a0, (1 - k / q + k * k) / a0)
    return biquad(y, (1.0, -2.0, 1.0), a)


def m_max(x, rate):
    """The loudest 400 ms window, LUFS (mono counts as one channel, as BS.1770 does)."""
    e = sum(k_weight(x[:, c], rate) ** 2 for c in range(x.shape[1]))
    win, hop = int(0.4 * rate), int(0.01 * rate)
    e = np.concatenate([e, np.zeros(win)])
    c = np.concatenate([[0.0], np.cumsum(e)])
    best = max((c[i + win] - c[i]) / win for i in range(0, len(e) - win + 1, hop))
    return -0.691 + 10 * np.log10(max(best, 1e-12))


def speaker(x, rate):
    """What a handheld's small speaker leaves of it, roughly: nothing much below 400 Hz (two 2nd-order high-passes)."""
    import math

    w = 2 * math.pi * 400 / rate
    al, c = math.sin(w) / (2 * 0.707), math.cos(w)
    a0 = 1 + al
    b, a = ((1 + c) / 2 / a0, -(1 + c) / a0, (1 + c) / 2 / a0), (1.0, -2 * c / a0, (1 - al) / a0)
    return np.stack([biquad(biquad(x[:, ch], b, a), b, a) for ch in range(x.shape[1])], axis=1)


def true_peak(x):
    taps, over = 48, 4
    n = np.arange(-taps // 2, taps // 2)
    peak = np.abs(x).max()
    for p in range(1, over):
        t = n + p / over
        h = np.sinc(t) * np.kaiser(len(t), 8.0)
        for c in range(x.shape[1]):
            peak = max(peak, np.abs(np.convolve(x[:, c], h, mode="same")).max())
    return 20 * np.log10(max(peak, 1e-12))


def db(v):
    return 20 * np.log10(max(abs(v), 1e-12))


def report(path):
    x, rate = read(path)
    edge = rate // 500
    s = x[:, 0]
    return {
        "file": path.split("/")[-1],
        "ms": round(len(x) * 1000 / rate),
        "rate": rate,
        "M_max": round(m_max(x, rate), 1),
        "M_spk": round(m_max(speaker(x, rate), rate), 1),
        "TP": round(true_peak(x), 2),
        "peak": round(db(np.abs(x).max()), 2),
        "DC": round(db(s.mean()), 1),
        "first": int(round(s[0] * 32768)),
        "last": int(round(s[-1] * 32768)),
        "step_in": int(round(np.abs(np.diff(s[:edge])).max() * 32768)),
        "step_out": int(round(np.abs(np.diff(s[-edge:])).max() * 32768)),
    }


if __name__ == "__main__":
    print(f"{'file':22} {'ms':>5} {'M max':>7} {'M spk':>7} {'TP':>7} {'peak':>7} {'DC':>7} {'first':>6} {'last':>6} {'step in/out (LSB, 2 ms)':>24}")
    for p in sys.argv[1:]:
        r = report(p)
        print(f"{r['file']:22} {r['ms']:5} {r['M_max']:7.1f} {r['M_spk']:7.1f} {r['TP']:7.2f} {r['peak']:7.2f} {r['DC']:7.1f} {r['first']:6} {r['last']:6} {r['step_in']:>12}/{r['step_out']}")
