#!/usr/bin/env python3
"""UI sounds before, in and after a game, recorded off the speaker (README.md, "UI sounds after a game").

Runs on a Mini V2 with pocket-ui stopped (ui-after-game.sh does that and puts it back): starts pocket-ui with the
trace like scripts/latency.py (imported from the same folder), records the speaker sink's monitor with pw-record
(before volume and mute: silent with the speaker muted) and
follows `clock.force-quantum` with `pw-metadata -m`, while it
  1. moves on the shelf (menu sounds),
  2. starts GAME (system[:part of the file name], e.g. xbox:jade) and lets it run PLAY seconds,
  3. opens the pause menu and moves in it (the sounds over a game),
  4. quits through it and moves on the shelf at once, every 0.3 s for 6 s (the stream stays open: "a while"),
  5. waits 3 s (the output closes) and moves again.
Writes OUT.wav, OUT.events (seconds into the recording, what happened) and OUT.quantum (seconds, forced quantum).
Usage: ui-after-game.py GAME PLAY OUT
"""

import os
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import latency as L  # noqa: E402  (scripts/latency.py, copied next to this file)

# The sink recorded: the speaker, or the null sink ui-after-game.sh makes the default for a silent run (UAG_TARGET).
SPEAKER = os.environ.get("UAG_TARGET", "alsa_output._sys_devices_platform_sound_sound_card0.HiFi__Speaker__sink")


class Args:
    root = "/storage/pocket-ui"
    trace = "/tmp/pocket-trace.log"
    ui_log = "/tmp/ui-after-game-pocket-ui.log"
    settle = 2.0
    look = ""  # latency.py's --look: the shelf's system lists, not the Cabinet


def main():
    game, play, out = sys.argv[1], float(sys.argv[2]), sys.argv[3]
    events = open(out + ".events", "w")
    rec = subprocess.Popen(
        ["pw-record", "--target", SPEAKER, "-P", "{ stream.capture.sink = true node.name = ui_after_game_rec }", "--rate", "48000", "--channels", "2", "--format", "s16", out + ".wav"]
    )
    t0 = time.monotonic()

    def mark(what):
        events.write(f"{time.monotonic() - t0:.3f}\t{what}\n")
        events.flush()
        L.log(what)

    quantum = open(out + ".quantum", "w")
    watch = subprocess.Popen(["pw-metadata", "-m", "-n", "settings", "0", "clock.force-quantum"], stdout=subprocess.PIPE, text=True)

    def follow():
        for line in watch.stdout:
            if "clock.force-quantum" in line:
                value = line.split("value:'")[1].split("'")[0]
                quantum.write(f"{time.monotonic() - t0:.3f}\t{value}\n")
                quantum.flush()

    threading.Thread(target=follow, daemon=True).start()

    def moves(n, gap, label):
        mark(f"{label}: {n} moves")
        for i in range(n):
            L.send("down" if i % 2 == 0 else "up")
            time.sleep(gap)

    def top(label):
        with open(f"{out}.pwtop-{label}", "w") as f:
            subprocess.run(["pw-top", "-b", "-n", "2"], stdout=f, stderr=subprocess.STDOUT, timeout=10)

    trace = L.Trace(Args.trace)
    run = L.Run(Args, trace)
    # What the UI sound mixer does: when it opens, what is queued (POCKETUI_SOUND_DEBUG, ui_sound.rs).
    run.ui.extra_env = {"POCKETUI_SOUND_DEBUG": "1"}
    try:
        run.ui.start()
        mark("shelf")
        time.sleep(2)
        top("shelf")
        moves(10, 0.35, "shelf before")
        time.sleep(3)
        run.open_game(game)
        mark(f"start {game}")
        L.send("a")
        time.sleep(play)
        top("game")
        run.until_screen("home", ("menu",), timeout=20)
        mark("pause menu")
        time.sleep(1)
        moves(6, 0.35, "pause menu")
        mark("quit")
        # The moves above end on Continue (down, up, ...); Quit is one Up from it (the menu wraps around).
        run.press("up")
        time.sleep(0.05)
        t = L.send("a")
        trace.wait(lambda e: e["event"] == "game-exited" and e["pid"] == run.ui.pid, t, 60, "the game to end")
        mark("game exited")
        # Back from the game's list (Xbox has one game: nothing moves there, so nothing sounds) to the shelf.
        run.until_screen("b", L.SHELF, timeout=10)
        mark("back on the shelf")
        moves(20, 0.3, "shelf right after")
        top("after")
        # Where pocket-ui's sound threads wait (a mixer stuck in a write plays nothing more).
        with open(f"{out}.threads", "w") as f:
            task = f"/proc/{run.ui.pid}/task"
            for tid in sorted(os.listdir(task)):
                def rd(name):
                    try:
                        return open(f"{task}/{tid}/{name}").read().strip()
                    except OSError:
                        return "?"
                comm = rd("comm")
                if "sound" in comm or "audio" in comm or comm.startswith("pw-") or "data-loop" in comm:
                    f.write(f"{tid} {comm} state={rd('stat').split(') ')[1][0]} wchan={rd('wchan')}\n{rd('stack')}\n")
        time.sleep(3)
        moves(10, 0.35, "shelf after the output closed")
        time.sleep(1)
    finally:
        mark("end")
        run.ui.stop()
        rec.terminate()
        rec.wait()
        watch.terminate()


if __name__ == "__main__":
    main()
