#!/usr/bin/env python3
"""Merge gate (fast tier): pick the slow parts a change list needs; print shell assignments for scripts/gate.sh.

stdin: changed paths, one per line. argv[1]: the tree. Environment: GATE_I18N_KEYS, a file with the English text of the
translation lines the change touched (optional; without it a translation change always counts for Android). Output, one per line:
  tier=fast|full           full: the change can affect the gate itself or every test (Cargo, gate scripts, test harness): every
                           suite runs (the layout matrices as their sample, see below), the Brick stage too
  heavy='a b'              slow layout suites (HEAVY) to run anyway: their own file or their inputs changed
  skip='c d'               the slow suites and the time budgets (BUDGET) this gate leaves to the nightly run
  nightly_tests='a::b c'   single tests this gate leaves to the nightly run (NIGHTLY_TESTS), as nextest names them after the binary
  shell_tests=0|1 why_shell=<path>   the shell suites: only for deploy/, scripts/ and the licence texts the package carries
  brick_full=0|1           Brick corpus/UI walk, the Brick text-fit cases and the Brick navigation; otherwise only `cargo check --features brick`
  brick_text=0|1           at least the Brick text-fit cases (the same as brick_full since 14.11.: both follow Brick paths only)
  i18n_matrix=0|1          German/Spanish snapshot matrix
  android=0|1              the Android page tests of the library (android_02_tests, android_eject_tests); android_skip='a b' names
                           the ones this gate leaves to the nightly run
The merge gate runs every layout matrix it picks as a sample (scripts/gate.sh sets POCKETUI_TEST_QUICK: a dozen states, one size,
the longest language, plus the states the change adds). The nightly run and the release check use GATE_FULL=1: every suite with its
whole matrix, the time budgets alone, the timing tier, Brick and Android (docs/dev/merge-gate.md, "What runs where").
"""
from pathlib import Path
import os
import re
import shlex
import sys

# The slow layout/matrix suites (about 70 % of the test seconds in a full run, 2026-11-09 junit).
HEAVY = ["connections", "home_panel_labels", "info_page", "list_space", "long_titles", "meta_line", "names_never_translated",
         "nav_explorer", "small_screen", "spine_names", "spine_titles", "stickers_tv", "text_size"]
# Each slow suite's own layout inputs: the painters of the pages it renders (18.11.: 21b457727 grew the Shelf look's box in
# app_cabinet.rs, left the words beside it less width, and only the nightly run's connections saw "gespielt" alone on a line).
# A change to one of these runs the suite in the merge gate (as its sample). text_size and small_screen sweep every page: their
# inputs are the text sources (TEXT_SOURCE, below) and the state lists; every other page is theirs in the nightly run.
_SHELF = r"app_cabinet|app_cabinet_tv|cabinet|cabinet_art|cabinet_tv|cabinet_tv_art|media|shelf|app_big_shelf|big_shelf|title_block|tape_label|showcase|app_frozen|frozen|app_case_knows|case_knows_art|app_share_shelf|share_shelf"
_INFO = r"game_info|game_box|game_card|game_card_model|app_card|app_games_card|games_card|app_connections|connections|app_case_stickers|case_stickers|sticker_press"
_CURATOR = r"app_curator|curator_page|curatordb|app_home_panel|home_panel"
_LOOKS = r"look|look_sample|app_look"
_SPINES = r"media|shelf|app_cabinet|cabinet|cabinet_art|big_shelf|app_big_shelf|tape_label"
_STICKERS_TV = r"app_cabinet|app_cabinet_tv|cabinet|cabinet_art|cabinet_tv|cabinet_tv_art|case_stickers|app_case_stickers|sticker_press|game_box|media"
_NAMES = r"title_block|media|tape_label|game_card|game_box"
# The lists' room under them and the hint bar on every page (tests/list_space.rs): the pause menu's and Settings' list pages,
# the hint bar and the footer beside it (kit, also a text source), the explanations' wrap.
_LISTS = r"pause_page|app_settings_groups|settings_values|kit|options|emu_menu|cheats|nav_probe"
_NAV = r"app_settings_groups|settings_values|quick|quick_paint|power_menu|nav_probe|app_nav_probe"
# What the meta line under a game's name says and how it is laid out (tests/meta_line.rs, 18.11.: "6 h …" on the Brick's
# Favourites, "under a …" in the Android handheld view): its painters, the times and play times in it, the status, the tally
# and the save's progress that take its place.
_META = r"play_time|recents|library_meta|app_case_knows|case_knows_read|app_achievements|app_connections|connections"


def _src(alternatives, dirs=""):
    return re.compile(rf"^src/(?:(?:{alternatives})\.rs$" + (rf"|(?:{dirs})/)" if dirs else ")"))


LAYOUT_INPUTS = {
    "connections": _src(f"{_SHELF}|{_INFO}|{_CURATOR}", "curator"),
    "info_page": _src(f"{_SHELF}|{_INFO}|{_LOOKS}"),
    "meta_line": _src(f"{_SHELF}|{_META}", "achievements"),
    "home_panel_labels": _src(_CURATOR, "curator"),
    "long_titles": _src(f"{_SHELF}|{_INFO}|{_LOOKS}"),
    "spine_names": _src(_SPINES),
    "spine_titles": _src(_SPINES),
    "stickers_tv": _src(_STICKERS_TV),
    "names_never_translated": _src(_NAMES),
    "nav_explorer": _src(_NAV),
    "list_space": _src(_LISTS),
}
# Absolute CPU/frame-time budgets: each runs alone (threads-required) and goes red on an overloaded host (4 red trains in 3 days,
# 12.-14.11., none a real slowdown). They are measurements like the timing tier: nightly and before a release, never in the merge
# gate, not even when their own file changes (preflight-air.sh runs a changed one on the Air).
BUDGET = ["launch_frames", "shelf_moves"]
# Single tests the merge gate leaves to the nightly run and the release check: the Android twin of a check the Linux suite makes
# in every merge gate (eject::the_way_back_hands_over_…, 31 of 59 flake entries 12.-14.11.), and the case's gap on the way back,
# both 130+ s frame-by-frame walks.
# Also the library's pure time budgets (5,000 games indexed and searched within a time): they run alone (nextest.toml), so in
# the merge gate they also held up every other test while they measured.
# Settings keeps its timing case here, while the deterministic glyph/pattern check runs in every merge gate.
NIGHTLY_TESTS = ["android_eject_tests::the_way_back_hands_over_to_the_still_shelf_without_a_jump",
                 "settings_first_frame_is_not_much_slower_than_the_next",
                 "android_eject_tests::the_case_s_place_stays_an_empty_gap_until_the_case_comes_home",
                 "connections::tests::speed_of_the_index_for_5000_games",
                 "curator::tests::speed_index_build_lens_keypress_and_echoes_for_5000_games",
                 "curator::tests::speed_catalog_from_gamedb_for_a_real_library",
                 "search::tests::five_thousand_games_index_and_search_in_time"]
# What the shell suites read (launcher, package, image, device scripts): their own directories and the licence texts the package
# carries. A Rust source a script mentions by name is not an input: the suites run stand-in binaries (14.11., test audit).
SHELL = re.compile(r"^(deploy|scripts)/|(^|/)(LICEN[CS]E[^/]*|COPYING[^/]*|NOTICE\.md|OFL\.txt)$")
# Scripts no shell suite runs or reads: only their own Rust test calls them (19.11.: the reader inventory, the queue's most
# touched file that day, started the 4-7 min shell lanes in every gate).
NOT_SHELL = {"scripts/reader-inventory.py"}
# Fixtures only tests/data_sources.rs (through scripts/reader-inventory.py) reads: no layout suite draws from them, so they
# don't start every slow suite as other fixtures do.
READER_FIXTURES = re.compile(r"^tests/fixtures/reader-(inventory|coverage|audit)\.tsv$")
# The Brick's own code: its corpus/UI walk, F keys, text-fit and navigation cases run in the merge gate only for these.
BRICK = re.compile(r"brick|^src/(input_corpus|fn_keys)")
TEXT = ["text_size", "long_titles", "meta_line", "spine_titles", "spine_names", "small_screen", "home_panel_labels", "info_page"]
# Library tests that draw the Android pages (src/android_*_tests.rs include android/src/*.rs). 240 s for one of them on a busy host.
ANDROID_TESTS = ["android_02_tests", "android_eject_tests"]
ANDROID_SOURCES = ["src/android_02_tests.rs", "src/android_phones_tests.rs", "src/android_drawer_tests.rs", "src/android_eject_tests.rs"]
# Text and layout sources: what the text-fit suites and the Android pages both read.
TEXT_SOURCE = re.compile(r"^(assets/fonts/|assets/i18n/(?!skip\.txt$)|src/(text|textfit|type_scale|font|letters|name_lines|kit|layouts|scaled_layout|wiki_text|auto_texts|i18n)|tests/i18n)")
# Paths that define the gate, the build or every test: these run the complete gate.
FULL = re.compile(r"^(Cargo\.(toml|lock)|build\.rs|rust-toolchain(\.toml)?|\.cargo/|\.config/|tests/common/|"
                  r"scripts/(gate|test-gate|test-timing)[^/]*$)")


def android_modules(root):
    """The library modules the Android page tests and android/src use directly (`pocket_ui::<module>`): a change to one of
    them can change what those tests see. Read from the sources, so a new dependency joins the list without an edit here."""
    mods = set()
    files = [root / f for f in ANDROID_SOURCES] + sorted(root.joinpath("android", "src").glob("*.rs"))
    for path in files:
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        mods.update(re.findall(r"pocket_ui::([a-z_0-9]+)", text))
        for group in re.findall(r"pocket_ui::\{([^}]*)\}", text):
            mods.update(re.findall(r"\b([a-z_0-9]+)\b", group))
    return {m for m in mods if root.joinpath("src", m + ".rs").exists() or root.joinpath("src", m).is_dir()}


def changed_keys(root):
    """The English source strings of the translation lines this change adds, edits or removes (one per line in the file
    named by GATE_I18N_KEYS; gate.sh writes it from the diff, tools/remote-gate.sh uploads it). None: not known."""
    path = os.environ.get("GATE_I18N_KEYS", "")
    try:
        return [k for k in Path(path).read_text(errors="replace").split("\n") if k] if path else None
    except OSError:
        return None


def android_corpus(root, modules):
    """The text of every source an Android page can draw a string from: android/, src/android*.rs (not their tests) and the
    library modules the Android code uses directly."""
    files = [p for p in root.joinpath("android").rglob("*.rs")]
    files += [p for p in root.joinpath("src").glob("android*.rs") if not p.name.endswith("_tests.rs")]
    for m in modules:
        single = root.joinpath("src", m + ".rs")
        files += [single] if single.exists() else list(root.joinpath("src", m).rglob("*.rs"))
    text = []
    for f in files:
        try:
            text.append(f.read_text(errors="replace"))
        except OSError:
            pass
    return "\n".join(text).replace('\\"', '"').replace("\\'", "'")


def key_can_reach_android(key, corpus):
    """Does the English text of a translation line occur in a source Android draws from? Placeholders ({s}, {n2}) are cut
    out and the longest remaining piece is searched. Anything unclear (no piece of 6 characters, a backslash, a tab) says yes."""
    pieces = [p.strip() for p in re.split(r"\{[a-z0-9]*\}", key)]
    longest = max(pieces, key=len, default="")
    if len(longest) < 6 or "\\" in longest or "\t" in longest:
        return True
    return longest in corpus


def touches_android(name, modules):
    """Android, text, translations, fonts, assets, or a library module the Android tests use."""
    if name == "assets/i18n/skip.txt":
        return False
    if name.startswith(("android/", "src/android", "assets/")) or TEXT_SOURCE.match(name):
        return True
    if name.startswith("docs/user/"):
        return False
    m = re.match(r"src/([a-z_0-9]+)(?:\.rs|/)", name)
    return bool(m and m.group(1) in modules)


def parts(root, names):
    heavy, why_shell, shell = set(), "", 0
    brick = i18n = android = 0
    modules = android_modules(root)
    keys = changed_keys(root)
    corpus = None
    full = False
    for name in names:
        if not name:
            continue
        if FULL.match(name):
            full = True
        stem = Path(name).stem
        if SHELL.search(name) and name not in NOT_SHELL and not shell:
            shell, why_shell = 1, name
        if name.startswith("tests/") and name.endswith(".rs") and stem in HEAVY:
            heavy.add(stem)
        if name.startswith(("tests/snapshots/", "tests/fixtures/")) and not READER_FIXTURES.match(name):
            heavy.update(HEAVY)
        if name in ("tests/i18n-states.txt", "tests/layout-risk-states.txt"):
            heavy.add("text_size")
            i18n = 1
        if re.fullmatch(r"assets/i18n/(de|es|pt|fr|it)\.tsv", name) and keys is not None:
            # A translation line only matters to the Android pages when its English text is drawn from a source they use.
            corpus = corpus if corpus is not None else android_corpus(root, modules)
            if any(key_can_reach_android(k, corpus) for k in keys):
                android = 1
        elif touches_android(name, modules):
            android = 1
        if name.startswith("assets/fonts/") or re.match(r"src/(text|textfit|type_scale|font|letters|name_lines|kit|layouts|scaled_layout|wiki_text|auto_texts)", name):
            heavy.update(TEXT)
        if (name.startswith(("assets/i18n/", "docs/user/", "tests/i18n")) and name != "assets/i18n/skip.txt") \
                or name == "src/i18n.rs":
            heavy.update({"text_size", "names_never_translated", "meta_line"})
            i18n = 1
        # Where a console's name on a card or spine comes from (13.11.: a short-name table left the Atari 800 as "8-bit family"
        # and only the nightly run saw it): the two name suites run, 6 s together when the host is quiet.
        if re.match(r"src/(system_names|app_home_panel|home_panel|consoles|document_library|library|app_cabinet)\.rs$", name) \
                or name == "assets/data/consoles.json":
            heavy.update({"home_panel_labels", "spine_names"})
        for suite, inputs in LAYOUT_INPUTS.items():
            if inputs.match(name):
                heavy.add(suite)
        if BRICK.search(name):
            brick = 1
    # The Brick stage follows Brick paths only (14.11., both test audits: 30 % of all merge-gate seconds, in 74 % of the gates). A
    # font, a layout source, a translation or the manual leave it to the nightly run and the release check; an overlong text on the
    # Brick's screens is found there, before any release. `cargo check --features brick` stays in every gate (compile-first).
    present = {p.stem for p in root.joinpath("tests").glob("*.rs")}
    if full:
        # Every suite (their matrices as the sample), Android, the German/Spanish matrix and the Brick stage; the time budgets and
        # the timing tier stay with the nightly run.
        return {"tier": "full", "heavy": " ".join(sorted(present & set(HEAVY))), "skip": " ".join(sorted(present & set(BUDGET))),
                "nightly_tests": " ".join(NIGHTLY_TESTS), "shell_tests": shell, "why_shell": why_shell,
                "brick_full": 1, "brick_text": 1, "i18n_matrix": 1, "android": 1, "android_skip": ""}
    return {"tier": "fast", "heavy": " ".join(sorted(heavy)), "skip": " ".join(sorted(present & set(HEAVY + BUDGET) - heavy)),
            "nightly_tests": " ".join(NIGHTLY_TESTS), "shell_tests": shell, "why_shell": why_shell,
            "brick_full": brick, "brick_text": brick, "i18n_matrix": i18n, "android": android,
            "android_skip": "" if android else " ".join(n for n in ANDROID_TESTS)}


if __name__ == "__main__":
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    for key, value in parts(root, sys.stdin.read().splitlines()).items():
        print(f"{key}={shlex.quote(str(value))}")
