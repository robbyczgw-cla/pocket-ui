#!/usr/bin/env python3
"""On the device: the media sounds as a player gets them, from the button. Runs under capture.py (which records) with
pocket-ui stopped; device-ui.sh sets it up and puts everything back.

pocket-ui runs with the trace (scripts/latency.py, copied next to this file), its own data folder (POCKETUI_DATA: the
Shelf look, Cozy, menu sounds at their default) and a library of two self-made test ROMs (POCKETUI_ES_SYSTEMS: a GBA
cartridge in its cardboard box, a Master System cartridge in its plastic case), so no game of the player's is started
and nothing of theirs is written. It presses X on each (the case opens), X again after the output has closed, A on
the GBA one in Cozy (box, cartridge, then pocket-core's CRT, `CrtOn@core`) and, after a restart in Quick, A on it
again (the quiet click), quitting each game through the pause menu. Prints what should sound, as capture.py's events: the monotonic µs of the
button, the sound, how long after the button it is due (µs), its dB, and how many ms of it to compare (0: all).
B after each opening plays the Back sound of the ElevenLabs set: the same path, for comparison.
Usage: device-ui.py ROOT DATA ES_SYSTEMS
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import latency as L  # noqa: E402


# Frames drawn at another size don't matter here (no frame latency is measured), and on an H700 the first frames come
# at 800x600 before the 640x480 ones, which latency.py's check would then take for frames drawn before sway's resize.
L.Run.small_frame = lambda self, fr: False


class Args:
    root = "/storage/pocket-ui"
    trace = "/tmp/media-ui-trace.log"
    ui_log = "/tmp/media-ui-pocket-ui.log"
    settle = 2.0
    look = ""


def expect(t_us, sound, after_ms=0, db=0.0, head_ms=0):
    print(f"{t_us}\t{sound}\t{int(after_ms * 1000)}\t{db}\t{head_ms}", flush=True)


def main():
    Args.root, data, es = sys.argv[1], sys.argv[2], sys.argv[3]
    settings = os.path.join(data, "settings.json")
    trace = L.Trace(Args.trace)

    def start(animations):
        s = json.load(open(settings))
        s["animations"] = animations
        json.dump(s, open(settings, "w"))
        run = L.Run(Args, trace)
        run.ui.extra_env = {"POCKETUI_DATA": data, "POCKETUI_ES_SYSTEMS": es, "POCKETUI_SOUND_DEBUG": "1"}
        run.ui.start()
        time.sleep(2)
        return run

    def press(run, cmd):
        """The button, once pocket-ui has taken it (so a dark screen's wake can't swallow it unnoticed)."""
        t, _, _ = run.press(cmd)
        return t

    def launch_and_quit(run, sounds, crt=False):
        t = L.send("a")
        for s in sounds:
            expect(t, *s)
        run.trace.wait(lambda e: e["event"] == "core-frame", t, 60, "the test ROM's first frame")
        if crt:
            # pocket-core's old-TV power-on: its sound starts with it, through the game's own sound (`crt_on::start`).
            ev = run.trace.wait(lambda e: e["event"] == "crt-on", t, 10, "pocket-core's crt-on")
            expect(ev["t"], "CrtOn@core")
        time.sleep(3)
        run.until_screen("home", ("menu",), timeout=20)
        time.sleep(1)
        run.quit_game()
        time.sleep(2.5)

    run = start("cozy")
    try:
        run.open_game("gba:gba-test")
        time.sleep(2.5)
        expect(press(run, "x"), "BoxOpen")
        time.sleep(1.5)
        expect(press(run, "b"), "Back")
        time.sleep(1.0)
        run.open_game("mastersystem:sms-test")
        time.sleep(2.5)
        expect(press(run, "x"), "CaseOpen")
        time.sleep(1.5)
        expect(press(run, "b"), "Back")
        # The output has closed (2 s of quiet): the opening as the first sound after a while.
        time.sleep(3.0)
        expect(press(run, "x"), "CaseOpen")
        time.sleep(1.5)
        expect(press(run, "b"), "Back")
        time.sleep(1.0)
        run.open_game("gba:gba-test")
        time.sleep(3.0)
        # Cozy: the box opens at A, the cartridge seats (604 ms into its file, `hit_at`) at IN_AT_COZY (800 ms) less SOUND_LEAD, and pocket-core's CRT
        # comes with its picture. The GBA test ROM is silent: nothing of the game's own under it.
        launch_and_quit(run, [("BoxOpen",), ("CartIn", 790 - 604)], crt=True)
    finally:
        run.ui.stop()
    run = start("quick")
    try:
        run.open_game("gba:gba-test")
        time.sleep(3.0)
        # Quick: the click at IN_AT (180 ms) less SOUND_LEAD, 6 dB under the rest (`hit_at` 30 ms before).
        launch_and_quit(run, [("Insert", 170 - 30, -6.0)])
    finally:
        run.ui.stop()


if __name__ == "__main__":
    main()
