#!/usr/bin/env python3
"""Frame-time stats from Dolphin's render_times.txt (one presented frame per line, in ms). Dolphin writes it to
/storage/.config/dolphin-emu/Logs/ when GFX `LogRenderTimeToFile = True` (set it in the game INI under
[Video_Settings], since ROCKNIX rewrites GFX.ini on every start) and the Logs folder exists.
    dolphin-frametimes.py FROM TO FILE...   only frames between FROM and TO seconds into the log"""
import os, statistics, sys

a, b = float(sys.argv[1]), float(sys.argv[2])
for f in sys.argv[3:]:
    t, ft = 0.0, []
    for line in open(f):
        try:
            ms = float(line)
        except ValueError:
            continue
        t += ms / 1000
        if a <= t <= b:
            ft.append(ms)
    if not ft:
        print(os.path.basename(f), "no frames in the window; log is", round(t), "s")
        continue
    s = sorted(ft)
    total = sum(ft) / 1000
    print(f"{os.path.basename(f)}: {len(ft)/total:.2f} fps | frame {statistics.mean(ft):.2f} ms sd {statistics.pstdev(ft):.2f} "
          f"p99 {s[int(len(s) * 0.99) - 1]:.1f} max {max(ft):.1f} | over 25 ms: {sum(x > 25 for x in ft)}")
