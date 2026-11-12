#!/usr/bin/env python3
"""Regression for mistaking an attract -> title transition for dead input."""
import shlex
import unittest
from title_entry import start_probe


class TimedGame:
    """Attract consumes Start; the title accepts the next edge, then expires.

    The old Odin probes left 16-19 seconds between presses. The captured first
    images show titles, not a failed read; later presses again cancel attract.
    Exercise that distinction on both 50 Hz and 60 Hz polling clocks.
    """
    def __init__(self, phase, hz):
        self.now, self.phase, self.hz = 0.0, phase, hz
        self.paused = False
        self.expires = 8.0 if phase == "title" else float("inf")
        self.after = []

    def advance(self, seconds):
        if self.paused:
            return
        self.now += seconds
        if self.phase in ("title", "menu") and self.now >= self.expires:
            self.phase = "attract"

    def run(self, script):
        for line in script.splitlines():
            words = shlex.split(line)
            if words[:2] == ["input", "swipe"]:
                duration = int(words[-1]) / 1000
                if duration >= 1 / self.hz:
                    if self.phase == "attract":
                        self.phase, self.expires = "title", self.now + 8
                    elif self.phase == "title":
                        self.phase, self.expires = "menu", self.now + 8
                self.advance(duration)
            elif words == ["input", "keyevent", "110"]:
                self.paused = not self.paused
            elif words[:1] == ["sleep"]:
                self.advance(float(words[1]))
            elif words[:2] == ["screencap", "-p"] and words[-1].endswith("-after.png"):
                self.after.append(self.phase)


class TitleEntryTests(unittest.TestCase):
    def test_attract_reaches_the_menu_before_the_title_expires(self):
        for hz in (50, 60):
            game = TimedGame("attract", hz)
            game.run(start_probe("attract", 1619, 923, "/sdcard/own-qa/probe"))
            self.assertEqual(game.phase, "menu", f"{hz} Hz: a title is not the game menu")
            self.assertEqual(game.after[0], "title", "keep evidence of the first press's actual response")

    def test_a_visible_title_needs_only_one_start(self):
        game = TimedGame("title", 60)
        game.run(start_probe("title", 1619, 923, "/sdcard/own-qa/probe"))
        self.assertEqual(game.after, ["menu"])

    def test_old_widely_spaced_attempts_only_reopen_the_title(self):
        game = TimedGame("attract", 60)
        for _ in range(2):
            game.run(start_probe("title", 1619, 923, "/sdcard/own-qa/probe"))
            self.assertEqual(game.phase, "title")
            game.advance(16)
        self.assertEqual(game.phase, "attract")

    def test_no_start_probe_on_an_uninspected_or_playing_screen(self):
        for phase in ("playing", "unknown", "launcher"):
            with self.assertRaises(ValueError):
                start_probe(phase, 1, 1, "/sdcard/own-qa/probe")

    def test_inspecting_a_paused_menu_does_not_return_to_attract(self):
        game = TimedGame("attract", 60)
        game.run(start_probe("attract", 1619, 923, "/sdcard/own-qa/probe", pause_after=True))
        game.advance(30)
        self.assertEqual(game.phase, "menu")
        self.assertEqual(game.after, ["title", "menu"])


if __name__ == "__main__":
    unittest.main()
