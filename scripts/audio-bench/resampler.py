#!/usr/bin/env python3
"""Analyses the files of examples/resampler_bench.rs (usage: resampler.py DIR [MODE...]; MODE: linear sinc).
Per core rate and mode: tone lines (strongest foreign line and THD+N, 20 Hz-20 kHz), noise (flatness to 0.45 of the
core's rate, level of the image band above half of it), sweep (strongest line away from the sweep's own frequency),
latency. Numbers in dB. The 16-bit input and output set a floor of about -92 dB THD+N (printed as `floor`)."""
import sys, os, numpy as np

OUT = 48000.0
RATES = [32768, 32040, 44100, 48000]
TONES = [1000, 5000, 10000, 14000]

def load(d, name):
    return np.fromfile(f"{d}/{name}.s16", dtype="<i2").astype(np.float64)

def kaiser_spec(x, beta=18):
    w = np.kaiser(len(x), beta)
    return np.abs(np.fft.rfft(x * w)) / w.sum() * 2

def tone_metrics(y, f_in, skip=0.5, secs=2.0):
    y = y[int(skip * OUT):int((skip + secs) * OUT)]
    mag = kaiser_spec(y)
    freqs = np.fft.rfftfreq(len(y), 1 / OUT)
    k0 = int(np.argmax(mag))
    guard = 14                                   # bins of the window's main lobe
    sig = np.zeros(len(mag), bool)
    sig[max(0, k0 - guard):k0 + guard + 1] = True
    band = (freqs >= 20) & (freqs <= 20000)
    spur = np.where(sig, 0, mag)
    ks = int(np.argmax(spur))
    spur_db = 20 * np.log10(spur[ks] / mag[k0] + 1e-30)
    noise = np.sum((mag[band & ~sig]) ** 2)
    thdn = 10 * np.log10(noise / np.sum(mag[sig] ** 2))
    return spur_db, freqs[ks], thdn, freqs[k0], 20 * np.log10(mag[k0] / (0.7 * 32767))

def noise_metrics(x, y, rate):
    def psd(v, fs):
        n = 8192
        w = np.hanning(n)
        segs = [v[i:i + n] * w for i in range(int(fs), len(v) - n, n // 2)]
        p = np.mean([np.abs(np.fft.rfft(s)) ** 2 for s in segs], axis=0) / (fs * np.sum(w ** 2))
        return np.fft.rfftfreq(n, 1 / fs), p
    fx, px = psd(x, rate)
    fy, py = psd(y, OUT)
    def mean_db(f, p, lo, hi): return 10 * np.log10(np.mean(p[(f >= lo) & (f <= hi)]))
    ref = mean_db(fx, px, 100, 0.2 * rate)
    # response: output density relative to the input's, per band of the input's rate
    resp = {}
    edges = [(0.02, 0.1), (0.1, 0.2), (0.2, 0.3), (0.3, 0.4), (0.4, 0.45)]
    for lo, hi in edges:
        resp[(lo, hi)] = mean_db(fy, py, lo * rate, hi * rate) - mean_db(fx, px, lo * rate, hi * rate)
    pass_dev = (max(resp.values()) - min(resp.values()))
    edge_gain = mean_db(fy, py, 0.44 * rate, 0.45 * rate) - mean_db(fx, px, 0.44 * rate, 0.45 * rate)
    img = None
    if rate < OUT and 0.56 * rate < OUT / 2:
        # image band: from 0.56 of the rate (the images of what is below 0.44) up to the output's Nyquist, relative to the passband
        img = mean_db(fy, py, 0.56 * rate, OUT / 2) - mean_db(fy, py, 0.02 * rate, 0.4 * rate)
    return resp, pass_dev, edge_gain, img

def sweep_metrics(y, rate):
    n = 4096
    w = np.kaiser(n, 18)
    worst, at = -999, 0
    f0, f1, T = 20.0, 0.49 * rate, 6.0
    kk = np.log(f1 / f0) / T
    for i in range(int(0.3 * OUT), len(y) - n, n // 2):
        t = (i + n / 2) / OUT
        f = f0 * np.exp(kk * t)
        if f > 0.45 * rate:
            break
        spec = np.abs(np.fft.rfft(y[i:i + n] * w)) / w.sum() * 2
        fr = np.fft.rfftfreq(n, 1 / OUT)
        peak = spec.max()
        # the sweep moves f*kk*n/OUT Hz during one frame; leave that and the window's lobes out
        excl = 400 + 2 * f * kk * n / OUT
        m = np.abs(fr - f) > excl
        m &= fr > 20
        v = 20 * np.log10(spec[m].max() / peak + 1e-30)
        if v > worst:
            worst, at = v, fr[m][np.argmax(spec[m])]
    return worst, at

def latency(d, mode, rate):
    for line in open(f"{d}/meta.txt"):
        p = line.split()
        if p[:3] == [mode, str(rate), "impulse"] and p[2] == "impulse":
            lag = float(p[4])
    y = load(d, f"{mode}_{rate}_impulse")
    c = np.arange(len(y))
    win = slice(max(0, int(4000 / rate * OUT) - 80), int(4000 / rate * OUT) + 80)
    e = y[win] ** 2
    centroid = np.sum(c[win] * e) / np.sum(e)
    delay = (centroid / OUT - 4000 / rate) * 1000
    return lag + delay

def main():
    d = sys.argv[1]
    modes = sys.argv[2:] or ["linear", "sinc"]
    rng = np.random.default_rng(1)
    # the floor: a perfect sine rounded to 16 bits, same analysis
    n = int(OUT * 3)
    ideal = np.round(0.7 * 32767 * np.sin(2 * np.pi * 1000 * np.arange(n) / OUT))
    print(f"floor (perfect 1 kHz sine, 16 bit): THD+N {tone_metrics(ideal, 1000)[2]:.1f} dB, strongest foreign line {tone_metrics(ideal, 1000)[0]:.1f} dB\n")
    for suffix, label in [("", "nominal ratio"), ("_s", "ratio 0.4 % off (rate control at its limit)")]:
        print(f"### {label}\n")
        print("| Core rate | Mode | " + " | ".join(f"{t//1000} kHz line / THD+N" for t in TONES) + " | Sweep worst line | Noise: flatness to 0.45 / level at 0.44-0.45 / image band 0.56-24k |")
        print("|---|---|" + "---|" * (len(TONES) + 2))
        for rate in RATES:
            for mode in modes:
                cells = []
                for t in TONES:
                    s, fs, th, fp, lv = tone_metrics(load(d, f"{mode}_{rate}_tone{t}{suffix}"), t)
                    cells.append(f"{s:.0f} dB @{fs/1000:.1f}k / {th:.0f}")
                sw, at = sweep_metrics(load(d, f"{mode}_{rate}_sweep{suffix}"), rate)
                resp, dev, edge, img = noise_metrics(load(d, f"in_{rate}_noise"), load(d, f"{mode}_{rate}_noise{suffix}"), rate)
                nz = f"±{dev/2:.2f} dB / {edge:+.2f} dB / " + (f"{img:.0f} dB" if img is not None else "-")
                print(f"| {rate} | {mode} | " + " | ".join(cells) + f" | {sw:.0f} dB @{at/1000:.1f}k | {nz} |")
        print()
    print("### Latency (ms, input sample in -> output stream, including the kernel's delay)\n")
    print("| Core rate | " + " | ".join(modes) + " |\n|---|" + "---|" * len(modes))
    for rate in RATES:
        print(f"| {rate} | " + " | ".join(f"{latency(d, m, rate):.2f}" for m in modes) + " |")
    print("\n### CPU (ms per second of audio, this machine, one core)\n")
    for line in open(f"{d}/meta.txt"):
        if "cpu_ms" in line:
            print("-", line.strip())

main()
