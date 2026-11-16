#!/usr/bin/env python3
"""Crash sweep: the real pocket-ui and the real pocket-core, headless, through what a player does with a game in every
look and Cozy setting, looking for anything that stops a program (no device needed; release recipe, before an export).

    python3 -I scripts/crash-sweep.py --ui target/.../pocket-ui --core target/.../pocket-core --out DIR
        [--quick] [--jobs 2] [--only TAG-SUBSTRING] [--roms DIR] [--real-cores DIR] [--keep] [--self-test]

What it does, per run (one card, one game, one combination of settings):
  start fresh → play → pause menu and back → screen dark and on again → Quit (the moment is kept) → wait on the shelf
  (the Cabinet's TV plays the game there) → A: Continue from the kept moment (the TV's hand-over when it played) →
  play → Save from the pause menu → Quit.
After every step: did pocket-ui or pocket-core panic or die (`panicked at`, a signal, an exit nobody asked for), does the
screen say "stopped unexpectedly", did the game come up at all.

The games run on pocket-core with `scripts/crash-sweep/stub-core.c` under every core name pocket-ui routes to
(src/hostsys.rs): a core with a picture, sound in batches, input, a battery save and save states, so pocket-core's whole
loop runs (the audio path too, which no other test reached: 14.11.2026, the Cabinet hand-over's divide by zero).
`--real-cores DIR` uses real `<core>_libretro.so` files from DIR where there are some (the free homebrew in `--roms`).

The matrix: every look (Shelf, Cabinet, Easy, Split) × Retro/Barlow × Cozy/Quick × Motion Full/Reduced × the Cabinet's
TV on/off × sealed new games on/off × "the case knows" on/off, as a pairwise covering set; every look with the TV on and
off on the Cabinet's game; every system family once in the Cabinet and once on the Shelf. One game is a Pokémon Red with
a ROM made to have the release's CRC-32 (zeros and four bytes, not the game) and a save the reader believes, so the
"case knows" line is drawn; the rest have no art (the tape label). `--quick` is the release-recipe cut (about a third).

Needs: cc, weston (headless backend), bwrap on a host (each run gets its own /tmp: pocket-ui talks to pocket-core
through /tmp/pocket-core.cmd, and other programs on the machine may use theirs), python3. In a container without bwrap
or a mount namespace (a typical build container) the runs go one at a time: parallel runs on one /tmp answer each other's commands
(`private_tmp`). Everything runs at nice 19. Findings go to DIR/findings.tsv and DIR/report.md with each run's logs in
DIR/runs/<tag>/ (kept for runs with findings; --keep keeps all). Exit 1 when there is a finding.
"""
import argparse, itertools, json, os, re, shutil, signal, struct, subprocess, sys, tempfile, time, zlib
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PANIC = re.compile(r"panicked at|attempt to (?:divide|calculate the remainder) (?:by|with a divisor of) zero|attempt to (?:add|subtract|multiply|negate|shift) with overflow|RUST_BACKTRACE|SIGSEGV|SIGABRT|core dumped")
STOPPED = re.compile(r"(?i)stopped unexpectedly|start failed")
# What fails the sweep: a program that panicked or died (pocket-core also without a panic line: EXIT, ENDED), a
# "stopped unexpectedly", a paused game without its menu.
# Everything else (a row not reached, a slot not written) is a note on the runner's reach, not a product bug.
REAL = ("PANIC", "DIED", "STOPPED", "MENU", "ENDED", "EXIT")
# pocket-ui couldn't reach its compositor (the run's wayland-stub): the sweep's own trouble, a note (15.11.2026: a
# socket path over the 107 bytes a Unix socket may have made all 25 nightly runs "panic" at the UI's start).
NO_DISPLAY = re.compile(r"event loop: .*WaylandError\(Connect")
# A Unix socket's path: at most 107 bytes (sun_path). The sockets live in short folders of their own (`sock_path`).
SOCK_MAX = 107


def sock_path(out, n):
    """The n-th run's wayland-stub socket: DIR/wl/<n>, whatever the run's (long) tag."""
    return os.path.join(out, "wl", str(n))

# System id (es_systems.cfg) -> (extensions, a file in --roms/<dir> or a stand-in name, the --roms folder).
FAMILIES = {
    "gb": (".gb", "Tobu Tobu Girl.gb", "gb"),
    "gbc": (".gbc", "uCity.gbc", "gbc"),
    "gba": (".gba", "Attack on Voxelburg.gba", "gba"),
    "nes": (".nes", "Thwaite.nes", "nes"),
    "fds": (".fds", "fds-test.fds", None),
    "snes": (".sfc .smc", "Pipes.sfc", "snes"),
    "megadrive": (".md .bin .gen", "KleleAtoms MD.bin", "megadrive"),
    "mastersystem": (".sms", "sms-test.sms", None),
    "gamegear": (".gg", "gg-test.gg", None),
    "sg-1000": (".sg", "sg-test.sg", None),
    "pcengine": (".pce", "PceHello.pce", "pce"),
    "ngp": (".ngp .ngc", "NgpAsteroids.ngp", "ngp"),
    "wonderswan": (".ws .wsc", "ws-test.ws", None),
    "atarilynx": (".lnx", "LynxPlasma.lnx", "lynx"),
    "atari2600": (".a26 .bin", "a2600-test.a26", None),
    "virtualboy": (".vb", "vb-test.vb", None),
    "n64": (".n64 .z64 .v64", "N64Hello.n64", "n64"),
    "nds": (".nds", "Wolveslayer.nds", "nds"),
    "psp": (".pbp .iso", "Psp2048.pbp", "psp"),
    "psx": (".cue .chd .pbp .m3u", "Sweep Disc (USA).cue", None),
    "dreamcast": (".chd .cdi .gdi", "Sweep Disc (USA).chd", None),
    "pico-8": (".p8 .png", "sweep.p8", None),
    "tic-80": (".tic", "sweep.tic", None),
}
# Pokémon Red (USA, Europe) (SGB Enhanced): the CRC-32 its ROM has as No-Intro lists it (src/case_knows.rs RELEASES).
RED_CRC = 0x9F7FDD53
LOOKS = ["shelf", "cabinet", "big", "split"]
LOOK_NAMES = {"shelf": "Shelf", "cabinet": "Cabinet", "big": "Easy", "split": "Split"}


def crc_forge(data, target):
    """`data` with four bytes appended so that its CRC-32 is `target` (the CRC's linearity: no game bytes needed)."""
    # Work backwards through the table: the four bytes that take the register from crc(data) to target.
    table = []
    for i in range(256):
        c = i
        for _ in range(8):
            c = (c >> 1) ^ 0xEDB88320 if c & 1 else c >> 1
        table.append(c)
    top = {t >> 24: i for i, t in enumerate(table)}
    reg = target ^ 0xFFFFFFFF
    idx = []
    for _ in range(4):
        i = top[reg >> 24]
        idx.append(i)
        reg = ((reg ^ table[i]) << 8) & 0xFFFFFFFF
    idx.reverse()
    cur = zlib.crc32(data) ^ 0xFFFFFFFF
    out = bytearray()
    for i in idx:
        b = (cur ^ i) & 0xFF
        out.append(b)
        cur = (cur >> 8) ^ table[(cur ^ b) & 0xFF]
    forged = bytes(data) + bytes(out)
    assert zlib.crc32(forged) == target, hex(zlib.crc32(forged))
    return forged


def gb_text(s, n):
    out = bytearray()
    for ch in s:
        out.append(0x80 + ord(ch) - 65 if "A" <= ch <= "Z" else 0x7F)
    out.append(0x50)
    return bytes(out.ljust(n, b"\x50"))


def gen1_save(team, badges, hours, minutes):
    """A Red/Blue save the reader believes (src/case_knows.rs tests `gen1_save`): name, badges, clock, team, sum."""
    s = bytearray(0x8000)
    s[0x2598:0x2598 + 11] = gb_text("SWEEP", 11)
    s[0x2602] = badges
    s[0x2CED], s[0x2CEF], s[0x2CF0] = hours, minutes, 30
    p = 0x2F2C
    s[p] = len(team)
    for i, (name, level) in enumerate(team):
        s[p + 1 + i] = 0xB4
        s[p + 8 + i * 44 + 0x21] = level
        s[p + 8 + i * 44 + 0x03] = level
        s[p + 0x152 + i * 11:p + 0x152 + (i + 1) * 11] = gb_text(name, 11)
    s[p + 1 + len(team)] = 0xFF
    s[0x3523] = (~sum(s[0x2598:0x3523])) & 0xFF
    return bytes(s)


def core_names():
    """Every core name pocket-ui starts on pocket-core, by system, from src/hostsys.rs."""
    src = open(os.path.join(ROOT, "src/hostsys.rs")).read()
    by_system = {}
    for names, core in re.findall(r'system\(\s*&\[([^\]]*)\],\s*"([a-z0-9_]+)"', src):
        for n in re.findall(r'"([^"]+)"', names):
            by_system.setdefault(n, core)
    for s, c in re.findall(r'\("([a-z0-9-]+)", "([a-z0-9_]+)"\)', src[src.find("HANDHELD_CORES"):src.find("HANDHELD_CORES") + 400]):
        by_system[s] = c
    return by_system


def pairwise(factors):
    """A small set of combinations in which every pair of two factors' values occurs (greedy, deterministic)."""
    names = list(factors)
    want = set()
    for a, b in itertools.combinations(range(len(names)), 2):
        for va in factors[names[a]]:
            for vb in factors[names[b]]:
                want.add((a, va, b, vb))
    rows = []
    every = list(itertools.product(*factors.values()))
    while want:
        best = max(every, key=lambda row: sum((a, row[a], b, row[b]) in want for a, b in itertools.combinations(range(len(names)), 2)))
        rows.append(dict(zip(names, best)))
        for a, b in itertools.combinations(range(len(names)), 2):
            want.discard((a, best[a], b, best[b]))
    return rows


def plan(quick):
    """The runs: (tag, settings, system, game kind)."""
    base = dict(look="cabinet", style="barlow", anim="cozy", motion="full", tv=True, sealed=True, knows=True)
    runs = []
    factors = {"look": LOOKS, "style": ["barlow", "retro"], "anim": ["cozy", "quick"], "motion": ["full", "reduced"],
               "tv": [True, False], "sealed": [True, False], "knows": [True, False]}
    for row in pairwise(factors):
        runs.append((row, "gb", "red"))
    # The Cabinet's game with a kept moment in every look, TV on and off (tonight's crash was here).
    # A new game without a battery save (kind "new": the stub without one) rests on the shelf first, so the Cabinet's TV
    # plays it and A hands that run over (`showcase::may_continue`).
    # Not sealed: a sealed game's first start is its tear scene, never the TV's run.
    for look in LOOKS:
        for tv in (True, False):
            runs.append((dict(base, look=look, tv=tv, sealed=False), "snes", "new"))
    for sysid in ("nes", "megadrive", "mastersystem", "gb", "gba"):
        runs.append((dict(base, sealed=False), sysid, "new"))
    fams = list(FAMILIES)
    if quick:
        # The pairwise set, the Cabinet's game with the TV on and off, and the families whose cores pocket-core treats
        # in a way of their own (3D, discs, PSP's exit guard, the fantasy consoles).
        fams = ["gb", "gba", "nes", "snes", "megadrive", "psx", "n64", "nds", "psp", "pico-8"]
        runs = [r for r in runs if r[1] == "gb" or r[0]["look"] == "cabinet"]
    for sysid in fams:
        runs.append((base, sysid, "plain"))
        if not quick:
            runs.append((dict(base, look="shelf", anim="quick", motion="reduced", tv=False, sealed=False), sysid, "plain"))
    out, seen = [], set()
    for s, sysid, kind in runs:
        tag = f"{sysid}-{kind}-{s['look']}-{s['style']}-{s['anim']}-{s['motion']}-tv{int(s['tv'])}-seal{int(s['sealed'])}-knows{int(s['knows'])}"
        if tag not in seen:
            seen.add(tag)
            out.append((tag, s, sysid, kind))
    return out


def settings_json(s):
    # `tv_plays_game`: the Cabinet TV playing the game (off at the start); POCKETUI_SHOWCASE forces it as well.
    extras = {"cabinet_tv": s["tv"], "tv_plays_game": s["tv"], "sealed_new_games": s["sealed"], "progress_from_saves": s["knows"]}
    return {"onboarded": True, "seen_version": "9.9.9", "theme": s["look"], "font": s["style"],
            "animations": s["anim"], "motion": s["motion"], "extras": extras}


class Run:
    def __init__(self, a, tag, s, sysid, kind, env_base, wrap):
        self.a, self.tag, self.s, self.sysid, self.kind = a, tag, s, sysid, kind
        self.dir = os.path.join(a.out, "runs", tag)
        self.env_base, self.wrap = env_base, wrap
        self.findings, self.steps = [], []
        self.ui = None

    def cores_dir(self):
        """The stub with a battery save, or (kind "new") the one without; real cores where --real-cores has them."""
        return os.path.join(self.a.out, "cores-nosram" if self.kind == "new" else "cores")

    # -- the card --------------------------------------------------------------------------------------------------
    def card(self):
        shutil.rmtree(self.dir, ignore_errors=True)
        os.makedirs(os.path.join(self.dir, "data"))
        os.makedirs(os.path.join(self.dir, "tmp"))
        exts, name, romdir = FAMILIES[self.sysid]
        folder = os.path.join(self.dir, "roms", self.sysid)
        os.makedirs(folder)
        if self.kind == "red":
            name = "Pokemon - Red Version (USA, Europe) (SGB Enhanced).gb"
            rom = os.path.join(folder, name)
            with open(rom, "wb") as h:
                h.write(crc_forge(bytes(1 << 20)[:-4], RED_CRC))
            with open(os.path.join(folder, os.path.splitext(name)[0] + ".srm"), "wb") as h:
                h.write(gen1_save([("CHARIZARD", 45), ("PIKACHU", 30)], 0x7F, 31, 12))
        else:
            src = os.path.join(self.a.roms, romdir, name) if romdir and self.a.roms else None
            gen = os.path.join(self.a.out, "testroms", name)
            rom = os.path.join(folder, name)
            if src and os.path.exists(src):
                shutil.copy(src, rom)
            elif os.path.exists(gen):
                shutil.copy(gen, rom)
            else:
                with open(rom, "wb") as h:
                    h.write(b"crash-sweep stand-in\n" * 64)
            if name.endswith(".cue"):
                with open(rom, "w") as h:
                    h.write('FILE "Sweep Disc (USA).bin" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n')
                with open(os.path.join(folder, "Sweep Disc (USA).bin"), "wb") as h:
                    h.write(bytes(2352 * 16))
        self.rom = rom
        with open(os.path.join(self.dir, "es_systems.cfg"), "w") as h:
            h.write(f'<?xml version="1.0"?>\n<systemList>\n  <system><name>{self.sysid}</name><fullname>{self.sysid}</fullname>'
                    f'<path>{folder}</path><extension>{exts}</extension><command>true</command></system>\n</systemList>\n')
        with open(os.path.join(self.dir, "data", "settings.json"), "w") as h:
            json.dump(settings_json(self.s), h)
        # Played before: the game on Home's Continue row, every look starts it with A from there. Sealed new games on,
        # or a new game (kind "new"): never played, A goes through the shelf (and the wrap) to it.
        if not self.s["sealed"] and self.kind != "new":
            with open(os.path.join(self.dir, "data", "recents.json"), "w") as h:
                json.dump([{"system": self.sysid, "name": os.path.splitext(os.path.basename(rom))[0], "path": rom,
                            "last_played": int(time.time()) - 600, "play_secs": 600}], h)
        with open(os.path.join(self.dir, "wakealarm"), "w") as h:
            h.write("+3600")
        # The game command of the test driver: pocket-core on this system's core, in the headless compositor.
        run_game = os.path.join(self.dir, "run-game")
        with open(run_game, "w") as h:
            h.write("#!/bin/sh\n"
                    'printf "%s\\n" "$*" >> "$0.started"\n'
                    f'export WAYLAND_DISPLAY="{self.a.wayland}" XDG_RUNTIME_DIR="{self.a.runtime}"\n'
                    f'"{self.a.core}" "{self.cores_dir()}/{os.path.basename(self.a.cores_map[self.sysid])}" "$1" --system "{self.sysid}" >> "$0.core.log" 2>&1\n'
                    # How pocket-core ended (a signal: 128 + its number): a death without a panic line is found too.
                    'rc=$?\nprintf "%s\\n" "$rc" >> "$0.exit"\nexit "$rc"\n')
        os.chmod(run_game, 0o755)

    # -- driving -----------------------------------------------------------------------------------------------------
    def start(self):
        env = dict(self.env_base)
        env.update(POCKETUI_DATA=os.path.join(self.dir, "data"), POCKETUI_ES_SYSTEMS=os.path.join(self.dir, "es_systems.cfg"),
                   POCKETUI_VISIBLE_TEXT=os.path.join(self.dir, "frame.txt"), POCKETUI_NAV_PROBE=os.path.join(self.dir, "nav.tsv"),
                   POCKETUI_RTC_ALARM=os.path.join(self.dir, "wakealarm"), POCKETUI_STATES=os.path.join(self.dir, "states"),
                   POCKETUI_SHOWCASE="1" if self.s["tv"] else "0", TMPDIR=os.path.join(self.dir, "tmp"),
                   POCKETUI_CORES=self.cores_dir())
        self.log = open(os.path.join(self.dir, "ui.log"), "w")
        cmd = self.wrap + ["nice", "-n", "19", self.a.ui, "--test-driver", self.dir]
        self.ui = subprocess.Popen(cmd, env=env, stdout=self.log, stderr=subprocess.STDOUT, start_new_session=True)
        self.wait(lambda: os.path.exists(os.path.join(self.dir, "ui.in")), 20, "the UI's command FIFO")

    def send(self, *cmds):
        for c in cmds:
            fd = os.open(os.path.join(self.dir, "ui.in"), os.O_WRONLY | os.O_NONBLOCK)
            os.write(fd, (c + "\n").encode())
            os.close(fd)
            time.sleep(0.25)

    def status(self):
        try:
            return dict(f.split("=", 1) for f in open(os.path.join(self.dir, "ui.status")).read().strip().split("\t") if "=" in f)
        except OSError:
            return {}

    def frame(self):
        png = os.path.join(self.dir, "frame.png")
        if os.path.exists(png):
            os.remove(png)
        self.send("frame")
        self.wait(lambda: os.path.exists(png), 15, "a frame")
        time.sleep(0.05)
        try:
            return open(os.path.join(self.dir, "frame.txt")).read()
        except OSError:
            return ""

    def wait(self, cond, secs, what):
        t0 = time.time()
        while time.time() - t0 < secs:
            if cond():
                return True
            if self.ui and self.ui.poll() is not None:
                return False
            time.sleep(0.05)
        self.note("WAIT", f"no {what} within {secs} s")
        return False

    def note(self, kind, detail):
        self.findings.append((kind, detail, self.steps[-1] if self.steps else "start"))

    def started_count(self):
        try:
            return len(open(os.path.join(self.dir, "run-game.started")).read().splitlines())
        except OSError:
            return 0

    def focused(self):
        """The strings inside the focus mark of the last frame (`POCKETUI_NAV_PROBE`)."""
        try:
            lines = [l.rstrip("\n").split("\t") for l in open(os.path.join(self.dir, "nav.tsv"))]
        except OSError:
            return ""
        boxes = [(tuple(map(int, l[:4])), l[4]) for l in lines if len(l) >= 5 and all(re.fullmatch(r"-?\d+", x) for x in l[:4])]
        marks = [b for b, s in boxes if s == "‹nav:focus›"]
        out = []
        for (x0, y0, x1, y1), s in boxes:
            if s.startswith("‹nav:"):
                continue
            cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
            if any(m[0] <= cx <= m[2] and m[1] <= cy <= m[3] for m in marks):
                out.append(s)
        return " ".join(out)

    # The pause menu's rows, top to bottom (src/main.rs `MainItem`); it opens on Continue and its list wraps.
    MENU = ["Continue", "Save", "Load", "Earlier", "Disc", "Options", "Achievements", "Pixel Path", "Players", "Zoom", "Save & quit", "Quit"]

    def choose(self, label):
        """In the open pause menu: down to the row `label` (counted among the rows the frame shows), then A."""
        lines = [l.strip() for l in self.frame().splitlines()]
        rows = [m for m in self.MENU if m in lines]
        want = [m for m in rows if m == label or (label == "Quit" and m in ("Save & quit", "Quit"))]
        if not want:
            # Easy's menu is Continue and Save & quit only, on purpose.
            if not (self.s["look"] == "big" and label != "Quit"):
                self.note("NAV", f"no '{label}' row in the menu ({rows})")
            return False
        self.send(*(["down"] * rows.index(want[0])), "a")
        return True

    def game(self):
        return self.status().get("game", "")

    def play_from_home(self, why):
        """A on Home's first game until the game runs (a card or a question between: A again)."""
        before = self.started_count()
        for _ in range(4):
            self.send("a")
            if self.wait(lambda: self.started_count() > before, 6, f"game start ({why})") if False else self._started(before, 6):
                break
        else:
            self.note("NAV", f"the game did not start ({why}); screen: {self.frame()[:300]!r}")
            return False
        self.wait(lambda: self.game() in ("playing", "starting"), 15, f"game=playing ({why})")
        time.sleep(3)  # a few seconds of picture and sound
        return True

    def _started(self, before, secs):
        t0 = time.time()
        while time.time() - t0 < secs:
            if self.started_count() > before:
                return True
            if self.ui.poll() is not None:
                return False
            time.sleep(0.05)
        return False

    def ended(self, why):
        """The game should be running: one that ended by itself (no Quit asked for) is an ENDED finding."""
        if self.started_count() and self.game() in ("", "none", "off"):
            tail = ""
            try:
                tail = " | ".join(l.strip() for l in open(os.path.join(self.dir, "run-game.core.log"), errors="replace").read().splitlines()[-6:])
            except OSError:
                pass
            self.note("ENDED", f"the game ended by itself before {why} (exits {self.exits()}): {tail[:400]}")
            return True
        return False

    def exits(self):
        try:
            return [int(x) for x in open(os.path.join(self.dir, "run-game.exit")).read().split()]
        except (OSError, ValueError):
            return []

    def quit_game(self, why):
        if self.ended(f"Quit ({why})"):
            return
        if self.game() != "paused":
            self.send("home")
        self.wait(lambda: self.game() == "paused", 10, f"the pause menu ({why})")
        self.menu_seen(why)
        if self.game() != "paused":
            self.note("NAV", f"HOME did not open the pause menu ({why}): {self.frame()[:200]!r}")
            return
        if self.choose("Quit"):
            # A question (saves failing) or the Quit itself; the shelf after it.
            t0 = time.time()
            while time.time() - t0 < 20 and self.game() not in ("", "none", "off"):
                if self.ui.poll() is not None:
                    break
                time.sleep(0.2)

    def handed_over(self):
        """A note (the sweep's reach, not a product bug) when the TV's run was not handed over: then this run did
        not cover the hand-over (`showcase: A goes on with … from the TV` in pocket-ui's log)."""
        try:
            log = open(os.path.join(self.dir, "ui.log"), errors="replace").read()
        except OSError:
            log = ""
        if "from the TV" not in log:
            self.note("NOHANDOVER", "A started the game the usual way, not from the TV's run")

    def rest_for_tv(self):
        for _ in range(14):
            self.frame()
            time.sleep(0.5)

    def menu_seen(self, why):
        """Paused means the pause menu is on the screen: a paused game whose menu isn't painted is a MENU finding
        (14.11.2026: after the Cabinet TV's hand-over every paint of the game was the grow over the kept shelf)."""
        if self.game() != "paused":
            return
        for _ in range(6):
            if "Continue" in self.frame().splitlines():
                return
            time.sleep(0.5)
        self.note("MENU", f"paused, but no pause menu painted ({why}): {self.frame()[:160]!r}")

    def slots(self):
        d = os.path.join(self.dir, "states", self.sysid)
        # The manual slots (`slots::state_path`): `<game>.pstate`, `<game>.pstate1` …, with their size and time.
        if not os.path.isdir(d):
            return []
        return sorted((f, os.stat(os.path.join(d, f)).st_mtime_ns) for f in os.listdir(d) if re.search(r"\.pstate\d*$", f))

    def save_slot(self):
        if self.ended("Save"):
            return
        before = self.slots()
        self.send("home")
        if not self.wait(lambda: self.game() == "paused", 10, "the pause menu (save)"):
            return
        if self.choose("Save"):
            # Save fills the slot's picture first (`app_save_media`, a few frames); the test driver paints on request only.
            t0 = time.time()
            while time.time() - t0 < 12 and self.slots() == before and self.ui.poll() is None:
                self.frame()
                time.sleep(0.2)
            if self.slots() == before:
                self.note("WAIT", "no slot file after Save within 12 s")
            time.sleep(1)
        self.frame()

    def step(self, name, fn):
        if getattr(self, "_dead", False):
            return
        self.steps.append(name)
        try:
            fn()
        except Exception as e:  # the runner's own trouble is a finding of the run, not a stop of the sweep
            self.note("RUNNER", f"{type(e).__name__}: {e}")
        self.check()

    def check(self):
        logs = {"ui.log": os.path.join(self.dir, "ui.log"), "core.log": os.path.join(self.dir, "run-game.core.log")}
        seen = getattr(self, "_seen", {})
        for name, path in logs.items():
            try:
                text = open(path, errors="replace").read()
            except OSError:
                continue
            new = text[seen.get(name, 0):]
            seen[name] = len(text)
            for line in new.splitlines():
                if name == "ui.log" and NO_DISPLAY.search(line):
                    self.note("HARNESS", f"harness: no display ({line.strip()[:200]})")
                elif PANIC.search(line):
                    self.note("PANIC", f"{name}: {line.strip()[:300]}")
                elif STOPPED.search(line):
                    self.note("STOPPED", f"{name}: {line.strip()[:300]}")
        self._seen = seen
        for rc in self.exits()[getattr(self, "_exits_seen", 0):]:
            # 143: SIGTERM, ours or pocket-ui's own stop of a game that didn't answer (the UI says so itself then).
            if rc not in (0, 143):
                self.note("EXIT", f"pocket-core ended with {rc}" + (f" (signal {rc - 128})" if rc > 128 else ""))
        self._exits_seen = len(self.exits())
        if self.ui and self.ui.poll() is not None and not getattr(self, "_dead", False):
            self._dead = True
            self.note("DIED", f"pocket-ui exited with {self.ui.returncode}")
        try:
            text = open(os.path.join(self.dir, "frame.txt")).read()
            if STOPPED.search(text):
                self.note("STOPPED", "on screen: " + STOPPED.search(text).group(0))
        except OSError:
            pass

    def journey(self):
        self.step("card", self.card)
        self.step("ui start", self.start)
        self.step("home", lambda: self.frame())
        if self.s["look"] == "cabinet" and self.s["tv"]:
            # Rest on the game: the TV comes on after `showcase::DWELL` and plays; A then goes on from it.
            # The test driver paints on request only: a frame every half second lets the shelf's clock run.
            self.step("rest, the TV plays", self.rest_for_tv)
        self.step("start fresh", lambda: self.play_from_home("fresh"))
        if self.kind == "new" and self.s["look"] == "cabinet" and self.s["tv"]:
            self.step("went on from the TV", self.handed_over)
        self.step("pause and back", lambda: (self.send("home"), self.wait(lambda: self.game() == "paused", 10, "pause menu"), self.menu_seen("pause"), self.send("home"), time.sleep(1.5)))
        self.step("dark and on", lambda: (self.send("dark"), time.sleep(1.5), self.send("wake"), time.sleep(1), self.frame()))
        self.step("quit (moment kept)", lambda: self.quit_game("first"))
        # On the shelf: the Cabinet's TV plays the game after a few seconds; its A hands over (showcase.rs).
        self.step("shelf, TV plays", lambda: (self.frame(), time.sleep(7), self.frame()))
        self.step("continue the kept moment", lambda: self.play_from_home("continue"))
        # Save: the moment goes into the slot and the game plays on (`SaveAction::Save`).
        self.step("save", self.save_slot)
        self.step("quit again", lambda: self.quit_game("second"))
        self.step("shelf after", lambda: (time.sleep(2), self.frame()))

    def finish(self):
        self.check()
        self._dead = True  # what follows is our own stop, not a crash
        if self.ui and self.ui.poll() is None:
            try:
                os.killpg(self.ui.pid, signal.SIGTERM)
                self.ui.wait(10)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                try:
                    os.killpg(self.ui.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        self.check()
        if hasattr(self, "log"):
            self.log.close()
        real = [f for f in self.findings if f[0] in REAL]
        if not self.findings and not self.a.keep:
            shutil.rmtree(self.dir, ignore_errors=True)
        return real


# Each run's own /tmp. pocket-ui and pocket-core meet in fixed files there (/tmp/pocket-core.cmd, .status, the menu's
# picture …): two runs on one /tmp answer each other's commands. 15.11.2026: in an aarch64 build container (no bwrap, "a
# container's /tmp is its own") three parallel runs did, one run's Quit ended another's game a second after its first
# frame ("the game ended by itself", 2 of 7 hand-over runs), and the run whose Quit went astray was ended by pocket-ui's
# SIGTERM 5 s later. No product bug: with a /tmp per run (bwrap) never.
BWRAP = ["bwrap", "--dev-bind", "/", "/", "--tmpfs", "/tmp", "--die-with-parent"]
# A container's root may have its own mount namespace (CAP_SYS_ADMIN) where bwrap has no user namespace.
UNSHARE = ["unshare", "--mount", "--propagation", "private", "sh", "-c", 'mount -t tmpfs tmpfs /tmp && exec "$@"', "sh"]


def probe(cmd):
    try:
        return subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=20).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def private_tmp(out, bwrap_ok, unshare_ok):
    """The wrapper that gives every run its own /tmp, and why there is none (None: there is none here)."""
    if bwrap_ok:
        # The sweep's own folder stays visible when it is under /tmp itself.
        return BWRAP + ["--bind", out, out], ""
    under_tmp = os.path.realpath(out) == "/tmp" or os.path.realpath(out).startswith("/tmp/")
    if unshare_ok and not under_tmp:
        return list(UNSHARE), ""
    if unshare_ok:
        return None, f"the output {out} is under /tmp, which each run's own /tmp would hide (give --out elsewhere)"
    return None, "no bwrap or mount namespace here for a /tmp per run, and parallel runs would answer each other's commands"


def jobs_for(asked, wrap, shared):
    """Runs at once: several only where each has its own /tmp (or the red-before demonstration asks for one /tmp)."""
    return asked if wrap or shared else 1


def self_test():
    forged = crc_forge(bytes(1000), RED_CRC)
    assert zlib.crc32(forged) == RED_CRC and len(forged) == 1004
    save = gen1_save([("CHARIZARD", 45)], 0xFF, 31, 12)
    assert save[0x3523] == (~sum(save[0x2598:0x3523])) & 0xFF
    cores = core_names()
    assert cores.get("gb") == "gambatte" and cores.get("psx") == "pcsx_rearmed" and cores.get("n64") == "mupen64plus_next", cores
    for s in FAMILIES:
        assert s in cores, f"{s}: no core in src/hostsys.rs"
    rows = pairwise({"a": [1, 2, 3, 4], "b": [0, 1], "c": [0, 1], "d": [0, 1]})
    assert all(any(r["a"] == x and r["b"] == y for r in rows) for x in [1, 2, 3, 4] for y in [0, 1])
    full, quick = plan(False), plan(True)
    assert len(quick) < len(full) and len({t for t, *_ in full}) == len(full)
    # The runs' sockets fit a Unix socket whatever the tag (15.11.2026: DIR/runs/<tag>-wl was 111 bytes in the nightly).
    assert len(sock_path("/" + "d" * 60, 66)) <= SOCK_MAX
    assert NO_DISPLAY.search('event loop: Os(OsError { line: 89, file: "x", error: WaylandError(Connect(NoCompositor)) })')
    # Parallel runs never share a /tmp (15.11.2026: one run's Quit ended another's game in a build container).
    assert private_tmp("/w/out", True, False)[0][:len(BWRAP)] == BWRAP
    assert private_tmp("/w/out", False, True)[0] == UNSHARE
    assert private_tmp("/tmp/out", False, True)[0] is None and private_tmp("/w/out", False, False)[0] is None
    assert jobs_for(3, None, False) == 1 and jobs_for(3, [], False) == 1 and jobs_for(3, BWRAP, False) == 3
    print(f"crash-sweep self-test: ok ({len(full)} runs, --quick {len(quick)})")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ui")
    ap.add_argument("--core")
    ap.add_argument("--out")
    ap.add_argument("--quick", action="store_true")
    ap.add_argument("--jobs", type=int, default=2)
    ap.add_argument("--only", default="", help="only runs whose tag contains this")
    ap.add_argument("--roms", default=os.path.join(os.path.dirname(ROOT), "android-test-kit", "roms"))
    ap.add_argument("--real-cores", default="")
    ap.add_argument("--keep", action="store_true")
    ap.add_argument("--shared-tmp", action="store_true", help="red-before demonstration: parallel runs on one /tmp (inside a container or an outer bwrap only)")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if not (a.ui and a.core and a.out):
        ap.error("--ui, --core and --out are needed")
    a.ui, a.core, a.out = os.path.abspath(a.ui), os.path.abspath(a.core), os.path.abspath(a.out)
    os.makedirs(a.out, exist_ok=True)
    if not os.path.isdir(a.roms):
        a.roms = ""
    # The stub core, under each core name; real cores where --real-cores has them.
    a.cores_map = {}
    for variant, flags in (("cores", []), ("cores-nosram", ["-DNO_SRAM"])):
        cores_dir = os.path.join(a.out, variant)
        os.makedirs(cores_dir, exist_ok=True)
        stub = os.path.join(cores_dir, "stub_libretro.so")
        subprocess.run(["cc", "-shared", "-fPIC", "-O1", *flags, "-o", stub, os.path.join(ROOT, "scripts/crash-sweep/stub-core.c"), "-lm"], check=True)
        for sysid, core in core_names().items():
            path = os.path.join(cores_dir, f"{core}_libretro.so")
            real = os.path.join(a.real_cores, f"{core}_libretro.so") if a.real_cores else ""
            if os.path.lexists(path):
                os.remove(path)
            if real and os.path.isfile(real) and variant == "cores":
                os.symlink(os.path.abspath(real), path)
            else:
                os.symlink("stub_libretro.so", path)
            a.cores_map[sysid] = path
    cores_dir = os.path.join(a.out, "cores")
    subprocess.run([sys.executable, "-I", os.path.join(ROOT, "scripts/testroms.py"), os.path.join(a.out, "testroms")], check=True, capture_output=True)
    # The BIOS stand-ins testroms.py makes (our own programs): the Famicom Disk System starts instead of asking for one.
    bios = os.path.join(a.out, "bios")
    os.makedirs(bios, exist_ok=True)
    for src, name in (("fds-bios-standin.rom", "disksys.rom"), ("naomi-bios-standin.zip", "naomi.zip")):
        if os.path.exists(os.path.join(a.out, "testroms", src)):
            shutil.copy(os.path.join(a.out, "testroms", src), os.path.join(bios, name))
    # The sockets' paths must fit a Unix socket, or nothing can connect (a deep --out: say so once, not per run).
    longest = max(len(sock_path(a.out, 999).encode()), len(os.path.join(a.out, "run", "wl-sweep").encode()))
    if longest > SOCK_MAX:
        print(f"crash-sweep: harness: --out {a.out} is too deep for a Unix socket ({longest} > {SOCK_MAX} bytes); give a shorter one", file=sys.stderr)
        return 2
    # One headless compositor for every pocket-core of the sweep.
    a.runtime = os.path.join(a.out, "run")
    os.makedirs(a.runtime, mode=0o700, exist_ok=True)
    a.wayland = os.path.join(a.runtime, "wl-sweep")
    # weston 13 (Ubuntu 24.04) and 10 (Debian 12, the build container) name the headless backend and its GL differently.
    weston = None
    for flags in (["--backend=headless", "--renderer=gl"], ["--backend=headless-backend.so", "--use-gl"], ["--backend=headless-backend.so"]):
        weston = subprocess.Popen(["nice", "-n", "19", "weston", *flags, "--socket=wl-sweep", "--width=1240", "--height=1080", "--idle-time=0"],
                                  env=dict(os.environ, XDG_RUNTIME_DIR=a.runtime), stdout=open(os.path.join(a.out, "weston.log"), "a"),
                                  stderr=subprocess.STDOUT, start_new_session=True)
        for _ in range(100):
            if os.path.exists(a.wayland) or weston.poll() is not None:
                break
            time.sleep(0.05)
        if os.path.exists(a.wayland) and weston.poll() is None:
            break
        if weston.poll() is None:
            os.killpg(weston.pid, signal.SIGTERM)
    else:
        print("crash-sweep: weston did not start (weston.log)", file=sys.stderr)
        return 2
    in_container = os.path.exists("/.dockerenv") or os.path.exists("/run/.containerenv")
    if a.shared_tmp:
        wrap, why = [], "--shared-tmp: every run on the one /tmp (only inside a container or an outer bwrap)"
    else:
        wrap, why = private_tmp(a.out, probe(BWRAP + ["true"]), in_container and probe(UNSHARE + ["true"]))
    if wrap is None and not in_container:
        print("crash-sweep: needs bwrap for a private /tmp (pocket-ui and pocket-core share /tmp/pocket-core.cmd)", file=sys.stderr)
        return 2
    jobs = jobs_for(a.jobs, wrap, a.shared_tmp)
    if jobs != a.jobs:
        print(f"crash-sweep: one run at a time, not {a.jobs}: {why}", flush=True)
    wrap = wrap or []
    env = {k: v for k, v in os.environ.items() if k not in ("SWAYSOCK", "WAYLAND_DISPLAY", "DISPLAY")}
    env.update(POCKETUI_MOCK="1", POCKETUI_LANG="en", POCKETUI_TEXT_SIZE="normal", POCKETUI_DEVICE="miniv2",
               POCKETUI_SNAPSHOT_SIZE="1240x1080", POCKETUI_SUSPEND_CMD="false", POCKETUI_POWEROFF_CMD="true",
               POCKETUI_REBOOT_CMD="true", POCKETUI_BOOTLOADER_CMD="true", POCKETUI_NO_AUTOSUSPEND="1", POCKETUI_NO_HISTORY="1",
               POCKETUI_HOST_BIN=a.core, POCKETUI_CORES=cores_dir, POCKET_UI_HOST_ALL="1", POCKET_CORE_ALSA_DEVICE="null",
               POCKET_CORE_AUDIO="alsa", RUST_BACKTRACE="1", POCKETUI_BIOS=bios)
    # pocket-ui itself paints into the test driver's stub compositor (its frames on request).
    stubs = []

    def one(numbered):
        n, (tag, s, sysid, kind) = numbered
        r = Run(a, tag, s, sysid, kind, env, wrap)
        os.makedirs(r.dir, exist_ok=True)
        sock = sock_path(a.out, n)
        os.makedirs(os.path.dirname(sock), exist_ok=True)
        stub_p = subprocess.Popen([sys.executable, "-I", os.path.join(ROOT, "scripts/wayland-stub.py"), sock])
        stubs.append(stub_p)
        for _ in range(500):
            if os.path.exists(sock) or stub_p.poll() is not None:
                break
            time.sleep(0.01)
        r.env_base = dict(env, WAYLAND_DISPLAY=sock)
        t0 = time.time()
        try:
            if not os.path.exists(sock):
                r.note("HARNESS", f"harness: no display (the wayland stub's socket {sock} did not come up)")
                r._dead = True
            else:
                r.journey()
        finally:
            real = r.finish()
            stub_p.kill()
            stub_p.wait()
            try:
                os.remove(sock)
            except OSError:
                pass
        mark = "FAIL" if [f for f in r.findings if f[0] in REAL] else ("note" if r.findings else "ok")
        print(f"{mark:4} {time.time() - t0:5.0f} s  {tag}" + "".join(f"\n       {k}\t{st}\t{d}" for k, d, st in r.findings), flush=True)
        return tag, r.findings

    # A stop from outside (timeout, Ctrl-C) still ends the compositor and the runs (`finally` below).
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    runs = [p for p in plan(a.quick) if a.only in p[0]]
    print(f"crash-sweep: {len(runs)} runs, {jobs} at a time, stub core {'+ real cores ' + a.real_cores if a.real_cores else ''}", flush=True)
    t0 = time.time()
    try:
        with ThreadPoolExecutor(jobs) as pool:
            results = list(pool.map(one, enumerate(runs)))
    finally:
        try:
            os.killpg(weston.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    with open(os.path.join(a.out, "findings.tsv"), "w") as h:
        for tag, fs in results:
            for kind, detail, st in fs:
                h.write(f"{tag}\t{kind}\t{st}\t{detail}\n")
    real = [(t, f) for t, fs in results for f in fs if f[0] in REAL]
    notes = [(t, f) for t, fs in results for f in fs if f[0] not in REAL]
    with open(os.path.join(a.out, "report.md"), "w") as h:
        h.write(f"# Crash sweep\n\n{len(runs)} runs in {time.time() - t0:.0f} s; {len(real)} crash findings, {len(notes)} notes.\n\n")
        for t, (k, d, st) in real + notes:
            h.write(f"- **{k}** `{t}` at {st}: {d}\n")
    print(f"crash-sweep: {len(runs)} runs, {len(real)} crash findings, {len(notes)} notes ({a.out}/report.md)")
    # No run reached its display: nothing was swept, which is no green either (exit 2, the harness's trouble).
    blind = sum(1 for _, fs in results if any(k == "HARNESS" for k, *_ in fs))
    if blind:
        print(f"crash-sweep: harness: {blind} of {len(runs)} runs had no display (notes in report.md)")
    if runs and blind == len(runs) and not real:
        return 2
    return 1 if real else 0


if __name__ == "__main__":
    sys.exit(main())
