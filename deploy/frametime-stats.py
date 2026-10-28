#!/usr/bin/env python3
"""Frame-time statistics from deploy/frametime.c's logs (CLOCK_MONOTONIC ns per presented frame).
    frametime-stats.py FROM TO FILE...   only frames presented between FROM and TO (CLOCK_MONOTONIC seconds,
                                          e.g. `python3 -c 'import time; print(time.monotonic())'` on the device)"""
import os, statistics, sys

a, b = float(sys.argv[1]), float(sys.argv[2])
for f in sys.argv[3:]:
    t = [int(x) / 1e9 for x in open(f) if x.strip().isdigit()]
    t = [x for x in t if a <= x <= b]
    if len(t) < 10:
        continue
    ft = [(y - x) * 1000 for x, y in zip(t, t[1:])]
    s = sorted(ft)
    print(f"{os.path.basename(f)}: {len(ft) / (t[-1] - t[0]):.2f} fps | frame {statistics.mean(ft):.2f} ms sd {statistics.pstdev(ft):.2f} "
          f"p99 {s[int(len(s) * 0.99) - 1]:.1f} max {max(ft):.1f} | over 25 ms: {sum(x > 25 for x in ft)} | {len(ft)} frames")
