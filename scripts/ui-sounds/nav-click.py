#!/usr/bin/env python3
"""On the device: the menu's Nav click after rests of different length, from the button (H700-MENU-FEEL, 2026-11-15).

Runs under capture.py like device-ui.py (device-ui.sh with JOURNEY=nav-click.py sets it up, records and puts everything
back). pocket-ui runs with its own data folder (the Shelf look, sounds at their default) and device-ui.sh's two test
ROMs; Down/Up on Home move between the two systems, each move a Nav click. The rests show whether the output was still
warm: up to `ui_sound::WARM_LEASE` after a press the click starts within a few ms, after that the output reopens first
(100 to 220 ms on an H700). Nothing is launched. Prints capture.py's events (button µs, sound, wait, dB).
Usage: nav-click.py ROOT DATA ES_SYSTEMS
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import latency as L  # noqa: E402

# The H700's first frames are 800x600 before sway's resize (latency.py's small_frame would wait for nothing else).
L.Run.small_frame = lambda self, fr: False

# Seconds of rest before each press: warm browsing, short pauses to read, a long one past the old 2 s lease and the
# speaker's 5 s suspend.
RESTS = [1.0, 0.7, 0.7, 3.0, 3.0, 8.0, 8.0, 15.0]


class Args:
    root = "/storage/pocket-ui"
    trace = "/tmp/media-ui-trace.log"
    ui_log = "/tmp/media-ui-pocket-ui.log"
    settle = 2.0
    look = ""


def main():
    Args.root, data, es = sys.argv[1], sys.argv[2], sys.argv[3]
    # No "new in this version" card over Home (device-ui.sh's data says 0.1.3): its Down/Up would scroll the card, silently.
    settings = os.path.join(data, "settings.json")
    s = json.load(open(settings))
    s["seen_version"] = "99.0.0"
    json.dump(s, open(settings, "w"))
    run = L.Run(Args, L.Trace(Args.trace))
    run.ui.extra_env = {"POCKETUI_DATA": data, "POCKETUI_ES_SYSTEMS": es, "POCKETUI_SOUND_DEBUG": "1"}
    run.ui.start()
    time.sleep(3)
    try:
        run.to_home()
        for i, rest in enumerate(RESTS):
            time.sleep(rest)
            t, fr, _ = run.press("down" if i % 2 == 0 else "up")
            print(f"{t}\tNav\t0\t0.0", flush=True)
            print(f"# rest {rest} s: frame {(fr['t'] - t) / 1000:.1f} ms after the press", file=sys.stderr, flush=True)
        time.sleep(1.0)
    finally:
        run.ui.stop()


if __name__ == "__main__":
    main()
