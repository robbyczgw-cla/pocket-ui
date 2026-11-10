#!/usr/bin/env python3
"""The port sees its controls, keeps saves, and returns the display after an error."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
ENV = ROOT / "deploy/brick/portmaster/device-env.sh"
RUN = ROOT / "deploy/brick/portmaster-run.sh"


class PortLaunch(unittest.TestCase):
    def test_controls_match_the_physical_board(self):
        for board, name, sticks, width in [("brick", "trimui-brick", "0", "1024"),
                                           ("brickpro", "trimui-brick-pro", "2", "1024"),
                                           ("smartpro", "trimui-smart-pro", "2", "1280")]:
            with self.subTest(board=board):
                cmd = '. "$1" || exit; printf "%s|%s|%s|%s" "$DEVICE_NAME" "$ANALOG_STICKS" "$DISPLAY_WIDTH" "$CFW_GLIBC"'
                out = subprocess.run(["sh", "-c", cmd, "sh", str(ENV)], env={**os.environ, "POCKETUI_DEVICE": board}, capture_output=True, text=True)
                self.assertEqual(out.returncode, 0, out.stderr)
                self.assertEqual(out.stdout, f"{name}|{sticks}|{width}|233")

    def test_other_hardware_never_receives_a_trimui_controller_profile(self):
        out = subprocess.run(["sh", "-c", '. "$1"', "sh", str(ENV)], env={**os.environ, "POCKETUI_DEVICE": "smartpros"}, capture_output=True)
        self.assertNotEqual(out.returncode, 0)

    def test_upstream_reads_each_device_capability_as_a_separate_word(self):
        for board, sticks in [("brick", "analog_0"), ("brickpro", "analog_2"), ("smartpro", "analog_2")]:
            out = subprocess.run(["sh", "-c", '. "$1"; printf "%s" "$DEVICE_CAPABILITIES"', "sh", str(ENV)],
                                 env={**os.environ, "POCKETUI_DEVICE": board}, capture_output=True, text=True, check=True)
            capabilities = out.stdout.split()
            self.assertIn("aarch64", capabilities)
            self.assertIn(sticks, capabilities)
            self.assertNotIn("opengl", capabilities)

    def test_a_port_with_spaces_gets_persistent_saves_and_returns_the_screen_on_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            app, runtime, pm, home = [base / p for p in ["app", "runtime", "pm", "home"]]
            for path in [app / "bin", app / "portmaster", runtime / "bin", pm, home]:
                path.mkdir(parents=True)
            (app / "portmaster/device-env.sh").write_text(ENV.read_text())
            # Fake only the native launch boundary; the real port is executed by Bash.
            binary = app / "bin/portmaster-exec"
            binary.write_text('#!/bin/sh\nexec "$BRICK_APP/bin/bash" "$@"\n')
            binary.chmod(0o755)
            (app / "bin/bash").symlink_to("/bin/bash")
            fb = app / "bin/pocket-fb"
            fb.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >>"$EVENTS"\n')
            fb.chmod(0o755)
            game = base / "Test Port.sh"
            game.write_text('printf "%s|%s|%s|%s" "$HOME" "$ANALOG_STICKS" "$SDL_GAMECONTROLLERCONFIG_FILE" "$XDG_DATA_HOME" >"$RESULT"; echo progress >"$HOME/save"; exit 7\n')
            env = {**os.environ, "BRICK_APP": str(app), "BRICK_DATA": str(base / "data"),
                   "BRICK_RUN": str(base / "run"), "BRICK_PORTMASTER_HOME": str(pm),
                   "BRICK_PORTMASTER_RUNTIME": str(runtime), "POCKETUI_DEVICE": "brick",
                   "HOME": str(home), "RESULT": str(base / "result"), "EVENTS": str(base / "events"), "SWAYSOCK": "test-socket"}
            out = subprocess.run(["sh", str(RUN), str(game)], env=env, capture_output=True, text=True)
            self.assertEqual(out.returncode, 7, out.stderr)
            self.assertEqual((home / "save").read_text(), "progress\n")
            values = (base / "result").read_text().split("|")
            self.assertEqual(values[:3], [str(home), "0", str(pm / "pocket-gamecontrollerdb.txt")])
            self.assertTrue((Path(values[3]) / "PortMaster").is_dir(), "standard port scripts must find control.txt")
            self.assertTrue((pm / "pocket-gamecontrollerdb.txt").read_text().startswith("0300a3845e0400008e02000014010000,"),
                            "firmware SDL 2.30 includes the controller name checksum in its GUID")
            events = (base / "events").read_text().splitlines()
            self.assertIn("pocket_port begin", events[0])
            self.assertIn("pocket_port end", events[-1])


if __name__ == "__main__":
    unittest.main()
