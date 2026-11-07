#!/usr/bin/env python3
"""Scheduling allowance for host test hang guards (same policy as src/test_clock.rs)."""
import math
import os
import sys


def budget(seconds):
    cores = len(os.sched_getaffinity(0)) if hasattr(os, 'sched_getaffinity') else (os.cpu_count() or 1)
    load = os.getloadavg()[0] if hasattr(os, 'getloadavg') else 0
    extra = float(os.environ.get('POCKETUI_TEST_TIME_FACTOR', '1'))
    if not math.isfinite(extra) or not 1 <= extra <= 64:
        raise ValueError('POCKETUI_TEST_TIME_FACTOR must be in 1..64')
    return seconds * max(1, min(64, math.ceil(load / cores))) * extra


if __name__ == '__main__':
    print(math.ceil(budget(float(sys.argv[1]))))
