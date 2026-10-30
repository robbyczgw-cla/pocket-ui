#!/usr/bin/env python3
"""A raw sound effect -> a media UI sound in assets/ui-sounds: mono, 48 kHz, cut at the event's real onset, 30 Hz
high-pass, a gentle 10 kHz low-pass, DC out, faded in (2 ms) and out to exactly zero, levelled like the rest of the set
(M max, also as a small speaker plays it, `loudness.py`; true peak at most -1.5 dBTP), 16 bit with TPDF dither.

    EXACT=1 python3 scripts/ui-sounds/prep_media_sounds.py RAW OUT KIND START_MS END_MS

RAW: what ElevenLabs returned for `media-sounds.tsv`'s prompt (s16 stereo 48 kHz `.pcm`) or anything ffmpeg reads;
KIND and the cut: the same table. Without EXACT the window is found around the loudest moment (for a first look)."""
import math, os, subprocess, sys, wave
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import loudness
R = 48000
MAX_MS = {"seat": 300, "card-in": 500, "case-open": 800, "box-open": 1000, "cart-in": 1300, "disc-in": 1800, "crt-on": 2200}
# Around the loudest moment (the click, the snap, the thunk): how much before and after it is kept at most.
AROUND = {"seat": (20, 280), "card-in": (250, 250), "case-open": (250, 450), "box-open": (400, 500), "cart-in": (800, 400), "disc-in": (300, 1500), "crt-on": (150, 2000)}
TARGET = {"case-open": -26.0, "box-open": -27.0, "cart-in": -25.5, "disc-in": -25.5, "card-in": -27.5, "seat": -26.5, "crt-on": -27.0}
FULL_MAX, TP_MAX = -24.5, -1.5
def load(p):
    if p.endswith(".pcm"):
        x = np.fromfile(p, "<i2").astype(np.float64) / 32768
        return x.reshape(-1, 2).mean(axis=1)
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", p, "-ac", "1", "-ar", str(R), "-f", "f64le", "-"], capture_output=True, check=True).stdout
    return np.frombuffer(raw, "<f8").copy()
def hp(x, f=30):
    w = 2 * math.pi * f / R
    for q in (0.54, 1.31):
        al, c = math.sin(w) / (2 * q), math.cos(w); a0 = 1 + al
        x = loudness.biquad(x, ((1 + c) / 2 / a0, -(1 + c) / a0, (1 + c) / 2 / a0), (1.0, -2 * c / a0, (1 - al) / a0))
    return x
def env_db(x, ms=5):
    n = int(R * ms / 1000); k = np.ones(n) / n
    return 10 * np.log10(np.convolve(x ** 2, k, "same") + 1e-14)
def main():
    src, out, kind = sys.argv[1:4]
    x = hp(load(src))
    if len(sys.argv) > 5:
        x = x[int(R * float(sys.argv[4]) / 1000): int(R * float(sys.argv[5]) / 1000)]
    e = env_db(x); pk = int(np.argmax(e))
    if not os.environ.get("EXACT"):
        pre, post = AROUND[kind]
        lo = max(0, pk - int(R * pre / 1000)); x = x[lo: pk + int(R * post / 1000)]; e = e[lo: lo + len(x)]
    top = e.max()
    on = np.where(e > top - 45)[0]
    a = max(0, on[0] - int(R * 0.004)); b = on[-1]
    # Tail: on until it is 55 dB under the top, at most MAX_MS from the onset.
    later = np.where(e[b:] > top - 55)[0]
    b = min(len(x), b + (later[-1] if len(later) else 0) + int(R * 0.01), a + int(R * MAX_MS[kind] / 1000))
    x = x[a:b].copy()
    # Nothing sharp up top for a small speaker: a gentle 10 kHz low-pass (2nd order).
    w = 2 * math.pi * 10000 / R; al, c = math.sin(w) / (2 * 0.707), math.cos(w); a0 = 1 + al
    x = loudness.biquad(x, ((1 - c) / 2 / a0, (1 - c) / a0, (1 - c) / 2 / a0), (1.0, -2 * c / a0, (1 - al) / a0))
    x -= x.mean()
    fade = {"disc-in": 0.25, "crt-on": 0.12}.get(kind, 0.04)
    n_in, n_out = int(R * 0.002), min(int(R * fade), len(x) // 4)
    x[:n_in] *= 0.5 - 0.5 * np.cos(np.linspace(0, math.pi, n_in))
    x[-n_out:] *= 0.5 + 0.5 * np.cos(np.linspace(0, math.pi, n_out))
    x[0] = x[-1] = 0
    spk, full = loudness.m_max(loudness.speaker(x[:, None], R), R), loudness.m_max(x[:, None], R)
    x *= 10 ** (min(TARGET[kind] - spk, FULL_MAX - full) / 20)
    tp = loudness.true_peak(x[:, None])
    if tp > TP_MAX:
        x *= 10 ** ((TP_MAX - 0.05 - tp) / 20)
    rng = np.random.default_rng(1)
    d = (rng.uniform(-.5, .5, len(x)) + rng.uniform(-.5, .5, len(x))) * (np.abs(x) > 2 / 32768)
    q = np.clip(np.round(x * 32767 + d), -32767, 32767).astype("<i2"); q[0] = q[-1] = 0
    with wave.open(out, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(R); w.writeframes(q.tobytes())
    hit = int(np.argmax(np.abs(q))) * 1000 // R
    print(f"{os.path.basename(out):22} cut {a*1000//R:5}-{b*1000//R:5} ms  len {len(q)*1000//R:5} ms  loudest at {hit} ms")
main()
