#!/usr/bin/env python3
"""Monkey test: long seeded random-but-valid button sequences through pocket-ui's test-driver FIFO.

    python3 -I scripts/monkey.py --bin target/release/pocket-ui --out DIR [--seed N] [--minutes M]
        [--look shelf|minimal|split|cards|cabinet] [--lang en|de] [--text normal|medium|large] [--library demo|empty]

Per run it builds a throwaway card (a messy demo library: very long titles, accents and CJK, systems with one game,
Xbox / Wii U / PS3 entries, no covers; or `empty`: systems without games, the starter-pack state), starts
`pocket-ui --test-driver` (docs/DEVELOPING.md) at nice 19 and presses random buttons. The same seed is the same
button sequence; every press is also written to DIR/<tag>.cmds. Findings (DIR/<tag>.findings, one per line):

  PANIC / LOGERR   the UI died, or its log has a panic / ERROR line
  HANG             `frame` was not answered within 2 s
  STUCK            after B presses (and, when a stand-in game runs, the pause menu's Quit) the shelf did not come back
  TEXT O|T|E       the painter's text-box record: text on text, touching text, text cut by an edge (src/text.rs `boxes`)
  TOAST            an error-looking string on screen
  RSS              resident memory grew by more than 25 MB over the run
  IDLE             more than 1 % of a CPU while nothing is pressed
Exit status 1 when there are findings.
"""
import argparse, os, random, re, signal, subprocess, sys, tempfile, time, shutil

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

LONG = ("The Legend of Zelda - The Minish Cap - Super Extended Special Collector's Edition with Bonus Content and "
        "Director's Commentary")
SYSTEMS = [
    ("gb", "Game Boy", ".gb .zip", ["Tobu Tobu Girl (World).gb"]),
    ("gba", "Game Boy Advance", ".gba .zip", [
        f"{LONG} (USA, Europe) (Rev 2) (Beta).gba", "ポケットモンスター ルビー (Japan).gba", "Ōkami Den (Japan).gba",
        "Advance Wars (USA) (Rev 1).gba", "Golden Sun (USA, Europe).gba", "Metroid Fusion (USA).gba",
        "Mother 3 (Japan) [T-En by Chewy v1.3].gba", "Mario Kart - Super Circuit (USA).gba", "WarioWare, Inc. (USA).gba",
        "A.gba", "Zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz (World).gba"]),
    ("snes", "Super Nintendo", ".sfc .smc .zip", [f"Game {i:02d} - Chapter {i} of the Long Saga (USA).sfc" for i in range(14)]),
    ("psx", "PlayStation", ".chd .cue .m3u", ["Crash Team Racing (USA).chd", "Wipeout 2097 (Europe).chd", "Chrono Cross (USA).m3u"]),
    ("ps3", "PlayStation 3", ".ps3 .iso", ["Demon's Souls (USA).iso", "Journey (Europe).iso"]),
    ("xbox", "Xbox", ".iso .xbe", ["Halo - Combat Evolved (USA).iso", "Project Gotham Racing 2 (Europe).iso"]),
    ("wiiu", "Wii U", ".wua .wup .rpx", ["Super Mario 3D World (USA).wua", "Mario Kart 8 (Europe).wua"]),
    ("segacd", "Sega CD", ".chd .cue .iso", ["Pocket CD Demo (USA).cue", "Pocket CD Demo (USA).bin"]),
]
# What the shelf shows when it is the shelf: a system's name (it is in the library list of every look).
SHELF_NAMES = [s[1] for s in SYSTEMS]
TOAST = re.compile(r"(?i)\b(error|failed|couldn.t|could not|went wrong|panic|unwrap|undefined|NaN|None)\b")
STANDIN = """#!/bin/sh
printf 'started\\n' > "$0.started"
trap 'printf "quit\\n" > "$0.quit"; exit 0' TERM INT
while :; do sleep 0.05; done
"""
KEYS = [("down", 22), ("up", 18), ("right", 14), ("left", 12), ("a", 12), ("b", 9), ("x", 2), ("y", 2), ("l1", 2),
        ("r1", 2), ("l2", 1), ("r2", 1), ("select", 1), ("start", 1), ("home", 1)]


def build_card(tmp, lib, look, text):
    data = os.path.join(tmp, "data")
    os.makedirs(data)
    cfg = '<?xml version="1.0"?>\n<systemList>\n'
    for sid, name, exts, files in SYSTEMS:
        folder = os.path.join(tmp, sid)
        os.makedirs(folder)
        if lib == "demo":
            for f in files:
                body = 'FILE "Pocket CD Demo (USA).bin" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n' if f.endswith(".cue") else ""
                with open(os.path.join(folder, f), "w") as h:
                    h.write(body)
        cfg += (f"  <system><name>{sid}</name><fullname>{name}</fullname><path>{folder}</path>"
                f"<extension>{exts}</extension><command>true</command></system>\n")
    cfg += "</systemList>\n"
    with open(os.path.join(tmp, "es_systems.cfg"), "w") as h:
        h.write(cfg)
    with open(os.path.join(data, "settings.json"), "w") as h:
        h.write('{"onboarded":true,"theme":"%s"}' % look)
    run = os.path.join(tmp, "run-game")
    with open(run, "w") as h:
        h.write(STANDIN)
    os.chmod(run, 0o755)
    with open(os.path.join(tmp, "wakealarm"), "w") as h:
        h.write("+3600")
    return data


def cpu_ticks(pid):
    f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    return int(f[11]) + int(f[12])


def rss_kb(pid):
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmRSS:"):
            return int(line.split()[1])
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=os.path.join(ROOT, "target/release/pocket-ui"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--minutes", type=float, default=4)
    ap.add_argument("--look", default="shelf")
    ap.add_argument("--lang", default="en")
    ap.add_argument("--text", default="normal")
    ap.add_argument("--library", default="demo")
    ap.add_argument("--step-ms", type=int, default=35)
    ap.add_argument("--keys", default="", help="only these buttons, comma separated (bisecting what a button leaks)")
    a = ap.parse_args()
    tag = f"{a.library}-{a.look}-{a.lang}-{a.text}-s{a.seed}"
    os.makedirs(a.out, exist_ok=True)
    findings, seen = [], set()
    cmds = open(os.path.join(a.out, tag + ".cmds"), "w")

    def find(kind, detail, step):
        key = (kind, detail)
        if key not in seen:
            seen.add(key)
            findings.append(f"{kind}\tstep {step}\t{detail}")

    tmp = tempfile.mkdtemp(prefix="pocketui-monkey-")
    data = build_card(tmp, a.library, a.look, a.text)
    log_path = os.path.join(tmp, "ui.log")
    env = dict(os.environ, POCKETUI_MOCK="1", POCKETUI_LANG=a.lang, POCKETUI_TEXT_SIZE=a.text,
               POCKETUI_VISIBLE_TEXT=os.path.join(tmp, "frame.txt"), POCKETUI_TEXT_BOXES=os.path.join(tmp, "frame.box"),
               POCKETUI_DATA=data, POCKETUI_ES_SYSTEMS=os.path.join(tmp, "es_systems.cfg"),
               POCKETUI_RTC_ALARM=os.path.join(tmp, "wakealarm"), POCKETUI_SUSPEND_CMD="false",
               POCKETUI_POWEROFF_CMD="true", POCKETUI_NO_AUTOSUSPEND="1", POCKETUI_NO_HISTORY="1")
    env.pop("SWAYSOCK", None)
    stub = None
    if not os.environ.get("WAYLAND_DISPLAY"):
        sock = os.path.join(tmp, "wayland-stub")
        stub = subprocess.Popen(["python3", "-I", os.path.join(ROOT, "scripts/wayland-stub.py"), sock])
        for _ in range(300):
            if os.path.exists(sock):
                break
            time.sleep(0.01)
        env["WAYLAND_DISPLAY"] = sock
    log = open(log_path, "w")
    ui = subprocess.Popen(["nice", "-n", "19", a.bin, "--test-driver", tmp], env=env, stdout=log, stderr=log)
    fifo = os.path.join(tmp, "ui.in")
    for _ in range(500):
        if os.path.exists(fifo):
            break
        time.sleep(0.01)
    rng = random.Random(a.seed)
    names, weights = zip(*[k for k in KEYS if not a.keys or k[0] in a.keys.split(",")])
    logpos = [0]

    def send(cmd):
        cmds.write(cmd + "\n")
        fd = os.open(fifo, os.O_WRONLY | os.O_NONBLOCK)
        os.write(fd, (cmd + "\n").encode())
        os.close(fd)

    def frame(step, why=""):
        png = os.path.join(tmp, "frame.png")
        try:
            os.remove(png)
        except FileNotFoundError:
            pass
        t0 = time.time()
        send("frame")
        while not os.path.exists(png):
            if ui.poll() is not None:
                return None
            if time.time() - t0 > 2.0:
                find("HANG", f"no frame within 2 s (load {os.getloadavg()[0]:.1f}) {why}", step)
                if time.time() - t0 > 10:
                    return None
            time.sleep(0.005)
        time.sleep(0.02)
        try:
            return open(os.path.join(tmp, "frame.txt")).read()
        except OSError:
            return ""

    def check_text(step, text):
        if text is None:
            return
        try:
            for line in open(os.path.join(tmp, "frame.box")).read().splitlines():
                if line[:1] in "OTE":
                    n = len(findings)
                    find("TEXT " + line[0], line[2:].replace("\t", " | "), step)
                    if len(findings) > n:
                        try:
                            shutil.copy(os.path.join(tmp, "frame.png"), os.path.join(a.out, f"{tag}-step{step}.png"))
                        except OSError:
                            pass
        except OSError:
            pass
        for line in text.splitlines():
            if TOAST.search(line) and not line.startswith("‹"):
                find("TOAST", line.strip()[:100], step)

    def check_log(step):
        try:
            with open(log_path) as h:
                h.seek(logpos[0])
                new = h.read()
                logpos[0] = h.tell()
        except OSError:
            return
        for line in new.splitlines():
            if re.search(r"panicked|PANIC|\bERROR\b|\bWARN\b.*(fail|error)", line):
                find("LOGERR", line.strip()[:160], step)

    def game_running():
        s, q = os.path.join(tmp, "run-game.started"), os.path.join(tmp, "run-game.quit")
        return os.path.exists(s) and (not os.path.exists(q) or os.path.getmtime(q) < os.path.getmtime(s))

    def recover(step):
        """B presses, and out of a running game through the pause menu's Quit: the shelf must come back."""
        for _try in range(4):
            if game_running():
                send("home")
                time.sleep(0.6)
                # The guest's menu has four rows and wraps (Down x12 is Continue again): Up from Continue is Quit; a
                # hosted game's longer menu clamps, so the second try goes down to the end.
                if _try % 2 == 0:
                    send("up")
                else:
                    for _ in range(12):
                        send("down")
                time.sleep(0.2)
                send("a")
                # The stand-in ends with the ordinary SIGTERM fallback after 5 s when the core does not answer.
                for _ in range(120):
                    if not game_running():
                        break
                    time.sleep(0.1)
                time.sleep(0.5)
            for _ in range(10):
                send("b")
                time.sleep(0.02)
            time.sleep(0.4)
            t = frame(step, "recover")
            if t and not game_running() and (any(n in t for n in SHELF_NAMES + ["No games", "starter", "Starter"])
                                             or ("START" in t and ("Settings" in t or "Einstellungen" in t))):
                return True
        find("STUCK", "shelf not back after B x10 (+ pause menu Quit)\n" + (t or "").replace("\n", " / ")[:300], step)
        return False

    time.sleep(1.0)
    step, end = 0, time.time() + a.minutes * 60
    rss0 = None
    idle_next = time.time() + a.minutes * 30
    try:
        while time.time() < end:
            if ui.poll() is not None:
                find("PANIC", f"the UI exited with {ui.returncode}", step)
                break
            send(rng.choices(names, weights)[0])
            step += 1
            time.sleep(a.step_ms / 1000)
            if step % 6 == 0:
                check_text(step, frame(step))
                check_log(step)
            if step % 150 == 0:
                recover(step)
                if rss0 is None:
                    rss0 = rss_kb(ui.pid)
                with open(os.path.join(a.out, tag + ".rss"), "a") as h:
                    h.write(f"{step}\t{rss_kb(ui.pid) // 1024}\n")
            if time.time() > idle_next and idle_next > 0:
                idle_next = -1
                recover(step)
                time.sleep(2)
                t1, c1 = time.time(), cpu_ticks(ui.pid)
                time.sleep(10)
                pct = (cpu_ticks(ui.pid) - c1) / os.sysconf("SC_CLK_TCK") / (time.time() - t1) * 100
                if pct > 1.0:
                    find("IDLE", f"{pct:.1f} % of a CPU for 10 s without input (shelf)", step)
        if ui.poll() is None:
            recover(step)
            check_log(step)
            grown = (rss_kb(ui.pid) - (rss0 or 0)) / 1024
            if rss0 and grown > 25:
                find("RSS", f"{rss0 / 1024:.0f} MB -> {rss_kb(ui.pid) / 1024:.0f} MB", step)
    finally:
        cmds.close()
        if ui.poll() is None:
            ui.terminate()
            try:
                ui.wait(5)
            except subprocess.TimeoutExpired:
                ui.kill()
        for line in open(log_path).read().splitlines():
            m = re.match(r"test game pid (\d+)", line)
            if m:
                try:
                    os.killpg(int(m.group(1)), signal.SIGKILL)
                except OSError:
                    pass
        if stub:
            stub.kill()
        check_log(step)
        # What the random presses saved: data that the UI keeps in memory too (a named shelf is a shelf of the Cabinet).
        try:
            sizes = {f: os.path.getsize(os.path.join(data, f)) for f in sorted(os.listdir(data)) if os.path.isfile(os.path.join(data, f))}
            with open(os.path.join(a.out, tag + ".data"), "w") as h:
                h.write(repr(sizes) + "\n")
        except OSError:
            pass
        shutil.copy(log_path, os.path.join(a.out, tag + ".log"))
        shutil.rmtree(tmp, ignore_errors=True)
    with open(os.path.join(a.out, tag + ".findings"), "w") as h:
        h.write("\n".join(findings) + ("\n" if findings else ""))
    print(f"{tag}: {step} presses, {len(findings)} findings")
    for f in findings:
        print("  " + f)
    sys.exit(1 if findings else 0)


main()
