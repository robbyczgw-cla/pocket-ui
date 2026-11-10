#!/usr/bin/env python3
"""Merge gate (fast tier): pick the slow parts a change list needs; print shell assignments for scripts/gate.sh.

stdin: changed paths, one per line. argv[1]: the tree. Output, one per line:
  tier=fast|full           full: the change can affect the gate itself or every test (Cargo, gate scripts, test harness)
  heavy='a b'              slow layout suites (HEAVY) to run anyway: their own file or their inputs changed
  skip='c d'               the slow suites this gate leaves to the nightly run
  shell_tests=0|1 why_shell=<path>
  brick_full=0|1           Brick corpus/UI walk; otherwise only `cargo check --features brick`
  i18n_matrix=0|1          German/Spanish snapshot matrix
  android=0|1              Android compile check (the Deck check is a few seconds and always runs)
The nightly run and release check use GATE_FULL=1: every suite, the timing tier, Brick and both platforms.
"""
from pathlib import Path
import re
import shlex
import sys

# The slow layout/matrix suites (about 70 % of the test seconds in a full run, 2026-11-09 junit).
HEAVY = ["connections", "home_panel_labels", "info_page", "long_titles", "names_never_translated",
         "nav_explorer", "small_screen", "spine_names", "spine_titles", "stickers_tv", "text_size"]
# Absolute CPU/frame-time budgets: each runs alone (threads-required) and goes red on an overloaded host. They are
# timing checks like the timing tier: nightly and before a release, or when their own file changes.
BUDGET = ["settings_open", "shelf_moves"]
TEXT = ["text_size", "long_titles", "spine_titles", "spine_names", "small_screen", "home_panel_labels", "info_page"]
# Paths that define the gate, the build or every test: these run the complete gate.
FULL = re.compile(r"^(Cargo\.(toml|lock)|build\.rs|rust-toolchain(\.toml)?|\.cargo/|\.config/|tests/common/|"
                  r"scripts/(gate|test-gate|test-timing)[^/]*$)")


def parts(root, names):
    heavy, why_shell, shell = set(), "", 0
    brick = i18n = android = 0
    shell_scripts = list(root.joinpath("scripts").rglob("*.sh")) + list(root.joinpath("deploy").rglob("*.sh"))
    srcrefs = set()
    for script in shell_scripts:
        # These two name invented Rust paths as fixture data; neither is a gate shell suite.
        if script.name in ("test-gate-paths.sh", "test-export-public.sh"):
            continue
        try:
            srcrefs.update(re.findall(r"(?:src|tests)/[a-zA-Z0-9_./-]+\.rs", script.read_text(errors="replace")))
        except OSError:
            pass
    for name in names:
        if not name:
            continue
        if FULL.match(name):
            return {"tier": "full"}
        stem = Path(name).stem
        # Shell suites use stand-in binaries, not the Rust test executables: Rust files are safe unless a script names them.
        if name.startswith("docs/") and name.endswith(".md"):
            pass
        elif re.fullmatch(r"(src|tests)/[a-zA-Z0-9_./-]+\.rs", name):
            if name in srcrefs:
                shell, why_shell = 1, name
        elif name.endswith(".md") and "/" not in name and name != "NOTICE.md":
            pass
        else:
            shell, why_shell = 1, name
        if name.startswith("tests/") and name.endswith(".rs") and stem in HEAVY + BUDGET:
            heavy.add(stem)
        if name.startswith(("tests/snapshots/", "tests/fixtures/")):
            heavy.update(HEAVY)
        if name in ("tests/i18n-states.txt", "tests/layout-risk-states.txt"):
            heavy.add("text_size")
            i18n = 1
        if name.startswith("assets/fonts/") or re.match(r"src/(text|textfit|type_scale|font|letters|name_lines)", name):
            heavy.update(TEXT)
        if (name.startswith(("assets/i18n/", "docs/user/", "tests/i18n")) and name != "assets/i18n/skip.txt") \
                or name == "src/i18n.rs":
            heavy.update({"text_size", "names_never_translated"})
            i18n = 1
        if name in ("src/nav_probe.rs", "src/app_nav_probe.rs"):
            heavy.add("nav_explorer")
        if "brick" in name or name.startswith(("src/input_corpus", "src/fn_keys", "src/manual")):
            brick = 1
        if name.startswith("android/") or name == "scripts/check-platforms.sh":
            android = 1
        elif name.endswith(".rs"):
            try:
                if 'target_os = "android"' in root.joinpath(name).read_text(errors="replace"):
                    android = 1
            except OSError:
                pass  # a deleted file
    present = {p.stem for p in root.joinpath("tests").glob("*.rs")}
    return {"tier": "fast", "heavy": " ".join(sorted(heavy)), "skip": " ".join(sorted(present & set(HEAVY + BUDGET) - heavy)), "shell_tests": shell, "why_shell": why_shell,
            "brick_full": brick, "i18n_matrix": i18n, "android": android}


if __name__ == "__main__":
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    for key, value in parts(root, sys.stdin.read().splitlines()).items():
        print(f"{key}={shlex.quote(str(value))}")
