#!/usr/bin/env python3
"""Cuts assets/ui-sounds/disc-in.wav in two for the launch rigs (src/insert_rig.rs): the disc coming down on the spindle
and the lid or tray shutting, played apart with the pause between them. Cut at the quiet point before the clunk, with
short raised-cosine fades on both sides. Nothing else changes; run from the repository's root."""
import math
import struct
import wave

CUT_MS, FADE_OUT_MS, FADE_IN_MS = 150, 10, 3


def main():
    with wave.open("assets/ui-sounds/disc-in.wav") as w:
        rate, ch, width = w.getframerate(), w.getnchannels(), w.getsampwidth()
        assert ch == 1 and width == 2
        pcm = list(struct.unpack(f"<{w.getnframes()}h", w.readframes(w.getnframes())))
    cut = rate * CUT_MS // 1000
    drop, shut = pcm[:cut], pcm[cut:]
    for i in range(rate * FADE_OUT_MS // 1000):
        k = 0.5 - 0.5 * math.cos(math.pi * i / (rate * FADE_OUT_MS / 1000))
        drop[-1 - i] = round(drop[-1 - i] * k)
    for i in range(rate * FADE_IN_MS // 1000):
        k = 0.5 - 0.5 * math.cos(math.pi * i / (rate * FADE_IN_MS / 1000))
        shut[i] = round(shut[i] * k)
    for name, part in (("disc-drop", drop), ("disc-shut", shut)):
        with wave.open(f"assets/ui-sounds/{name}.wav", "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(rate)
            w.writeframes(struct.pack(f"<{len(part)}h", *part))


main()
