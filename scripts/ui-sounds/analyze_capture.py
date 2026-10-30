#!/usr/bin/env python3
"""Measures the media sounds in a device recording (capture.py's OUT.wav, .start, .events), against the files.

    analyze_capture.py OUT [VOLUME_STEP]

Per request (an `.events` line: monotonic µs it was asked, the sound, its wait in µs, dB, and optionally how many ms
of it to compare, when other sound starts under the rest): where it starts in the
recording (cross-correlation with the file at its gain, ±0.1 ms), so how late against when it was due (asked + wait;
the monitor is before the DAC, whose own buffer comes on top), M max and true peak as recorded, the difference to the
file (D+N, dB under the sound, with the sounds that overlap it taken out where they were found: dropouts, clicks or
a broken resample show there; TP is of the recording as it is, overlaps included), runs of padded silence (32+ zero
frames where the file has sound) and clipped samples. Exit 1 when a sound is missing, has a gap, clips, or D+N is
above -40 dB (-25 for a sound `NAME@core`, which pocket-core resamples to its core's rate and back and plays at the
game's pace: compared with the file stretched by the factor that fits best, reported in cents).
"""
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import loudness  # noqa: E402

FILES = {
    "CaseOpen": "case-open",
    "BoxOpen": "box-open",
    "CartIn": "cart-in",
    "DiscIn": "disc-in",
    "CardIn": "card-in",
    "Insert": "seat",
    "CrtOn": "crt-on",
    # The ElevenLabs set, for comparison on the same path.
    "Back": "back",
    "Confirm": "confirm",
    "Nav": "nav",
}
ASSETS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../assets/ui-sounds")
RATE = 48000


def gain_of(step):
    return 10 ** (-(10 - step) * 2.5 / 20)


def main():
    out = sys.argv[1]
    step = int(sys.argv[2]) if len(sys.argv) > 2 else 10
    rec, rate = loudness.read(out + ".wav")
    assert rate == RATE
    mono = rec[:, 0]
    start = int(open(out + ".start").read().strip())
    refs = {k: loudness.read(os.path.join(ASSETS, v + ".wav"))[0][:, 0] for k, v in FILES.items()}
    bad = 0
    # First where each one is (cross-correlation with its file, around when it was due), then each against the
    # recording less every other sound placed where it was found: the Cozy launch's sounds overlap.
    found = []
    for line in open(out + ".events"):
        t_us, name, wait_us, db, *head = line.rstrip("\n").split("\t")
        head_ms = int(head[0]) if head else 0
        ref = refs[name.split("@")[0]] * gain_of(step) * 10 ** (float(db) / 20)
        due = (int(t_us) + int(wait_us) - start) / 1e6
        lo = max(0, int((due - 0.02) * RATE))
        hi = min(len(mono), int((due + 0.35) * RATE) + len(ref))
        seg = mono[lo:hi]
        if len(seg) < len(ref):
            found.append((name, wait_us, db, due, None, ref, head_ms))
            continue
        n = 1 << int(np.ceil(np.log2(len(seg) + len(ref))))
        xc = np.fft.irfft(np.fft.rfft(seg, n) * np.conj(np.fft.rfft(ref, n)), n)[: len(seg) - len(ref) + 1]
        found.append((name, wait_us, db, due, lo + int(np.argmax(xc)), ref, head_ms))
    placed = np.zeros(len(mono))
    for _, _, _, _, at, ref, _ in found:
        if at is not None:
            placed[at : at + len(ref)] += ref
    print(f"{'sound':11} {'late ms':>8} {'M max':>7} {'TP':>7} {'D+N dB':>7} {'gaps':>5} {'clip':>5}")
    for name, wait_us, db, due, at, ref, head_ms in found:
        if at is None:
            print(f"{name:11} not in the recording")
            bad += 1
            continue
        stretch = 1.0
        if name.endswith("@core"):
            # pocket-core plays the game's sound (and what its chime mixes in) a little faster or slower to keep pace
            # with the display (a 59.73 fps core at 60 Hz: +0.45 %): the file stretched by the factor that fits best.
            t = np.arange(len(ref))
            best = None
            for k in np.linspace(0.99, 1.01, 81):
                r2 = np.interp(t / k, t, ref, right=0.0)
                seg = mono[at - 200 : at + len(ref) + 200]
                xc = np.correlate(seg, r2, "valid")
                lag = int(np.argmax(xc))
                g2 = mono[at - 200 + lag : at - 200 + lag + len(ref)]
                d = np.dot(g2 - r2, g2 - r2)
                if best is None or d < best[0]:
                    best = (d, k, r2, at - 200 + lag)
            _, stretch, ref, at = best
        got = mono[at : at + len(ref)]
        others = placed[at : at + len(ref)] - ref if not name.endswith("@core") else np.zeros(len(ref))
        alone = got - others
        g = float(np.dot(alone, ref) / np.dot(ref, ref))
        res = alone - ref
        # Only the head when asked (a game's own sound starts under the rest of it).
        k = int(RATE * head_ms / 1000) if head_ms else len(ref)
        dn = 10 * np.log10(max(np.dot(res[:k], res[:k]), 1e-20) / np.dot(ref[:k], ref[:k]))
        late = (at / RATE - due) * 1000
        # Padded silence: 32+ exact zeros in a row where the file is not near zero.
        z = (np.abs(got) < 0.5 / 32768) & (np.abs(ref) > 64 / 32768)
        runs, cur = 0, 0
        for v in z:
            cur = cur + 1 if v else 0
            if cur == 32:
                runs += 1
        clip = int(np.sum(np.abs(got) >= 32767 / 32768))
        m = loudness.m_max(alone[:, None], RATE)
        tp = loudness.true_peak(got[:, None])
        flag = ""
        # `@core`: played by pocket-core into the game's sound, resampled to the core's rate and back: not bit for bit.
        if runs or clip or dn > (-25 if name.endswith("@core") else -40) or g < 0.5:
            flag = "  <-- FAIL"
            bad += 1
        print(f"{name:11} {late:8.1f} {m:7.1f} {tp:7.2f} {dn:7.1f} {runs:5} {clip:5}{flag}   (gain fit {g:.3f}, wait {int(wait_us) // 1000} ms, {db.strip()} dB{f", pitch {-1200 * np.log2(stretch):+.1f} cents" if stretch != 1.0 else ""})")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
