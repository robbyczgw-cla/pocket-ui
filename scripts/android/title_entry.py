#!/usr/bin/env python3
"""Record a bounded Start probe after inspecting a game's attract/title screen.

Prints an Android shell script; does not connect to a device, launch an app or
acquire a device session. Run the script only in an already guarded QA player.
The coordinates must come from that player's visible Start accessibility node.
"""
import argparse
from pathlib import PurePosixPath
import shlex


def start_probe(phase, x, y, output, pause_after=False):
    if phase not in ("attract", "title"):
        raise ValueError("Inspect the attract or title screen before probing Start")
    if not (0 <= x < 8192 and 0 <= y < 8192):
        raise ValueError("Start coordinates must be inside the visible display")
    directory = PurePosixPath(output)
    if not directory.is_absolute() or ".." in directory.parts:
        raise ValueError("Use an absolute session-owned evidence directory")
    lines = ["set -eu", "mkdir -p " + shlex.quote(str(directory))]
    # An attract movie and the actual title can both say "Press Start". The
    # first edge cancels attract; allow its transition to settle, then press
    # again while the title is still there. Returning to a human between these
    # steps previously added 16-19 seconds and let attract start over.
    for i in range(2 if phase == "attract" else 1):
        shot = lambda when: "screencap -p " + shlex.quote(str(directory / f"start-{i + 1}-{when}.png"))
        lines += [shot("before"), f"input swipe {x} {y} {x} {y} 180", "sleep 3", shot("after")]
    if pause_after:
        # Pocket's MODE key pauses emulation. Keep the captured menu in place
        # while an operator inspects it; otherwise idle menus can also expire.
        lines.append("input keyevent 110")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", required=True, choices=("attract", "title"))
    parser.add_argument("--pause-after", action="store_true", help="pause Pocket's player after the last evidence image")
    parser.add_argument("x", type=int)
    parser.add_argument("y", type=int)
    parser.add_argument("output", help="new evidence directory owned by the current device session")
    args = parser.parse_args()
    print(start_probe(args.phase, args.x, args.y, args.output, args.pause_after))


if __name__ == "__main__":
    main()
