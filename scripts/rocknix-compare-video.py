#!/usr/bin/env python3
"""The side-by-side video of the comparison (docs/COMPARE-ROCKNIX.md, "The video"): EmulationStation + RetroArch on
the left, pocket-ui on the right, the same scripted steps of a showcase run (`drive --scenarios showcase`), each step
cut so that both start at the same press. The numbers in the captions come only from the measurement runs' results
(`<ui>.json` of `rocknix-compare.sh run`); a number that is not there says "not measured".

  rocknix-compare-video.py SHOWCASE_DIR --numbers MEASURE_DIR [--out FILE] [--audio none|es|pocket-ui] [--no-slowmo]

SHOWCASE_DIR and MEASURE_DIR are folders of compare-results/ (out/<ui>/ with events.jsonl, frames*.pfr and, for the
showcase, showcase.mp4; <ui>.json). Runs on this computer: ffmpeg, numpy, the Barlow fonts of assets/fonts.

Lining up: wf-recorder's frame times count from its own first frame. The sampling recorder ran alongside on the
presses' clock; the brightness of both, frame by frame, is compared at every shift within a few seconds of the
recorder's start, and the best fit gives the video's start on that clock (to about a frame).
"""
import argparse
import importlib.util
import json
import os
import subprocess
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
spec = importlib.util.spec_from_file_location("rc", os.path.join(HERE, "rocknix-compare.py"))
rc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rc)

FONT_T = os.path.join(ROOT, "assets/fonts/BarlowCondensed-SemiBold.ttf")
FONT_B = os.path.join(ROOT, "assets/fonts/Barlow-Medium.ttf")
BG = "0x0b0d10"
GREY = "0xb9c0c9"
DIM = "0x7d8590"
ENC = ["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p", "-r", "60"]
AENC = ["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2"]
NAMES = {"es": "EmulationStation + RetroArch", "pocket-ui": "pocket-ui"}

# The steps of a showcase run, in order: (key, press name, title, what the number is, metric, longest cut in s).
# {a}/{b}: the first and second game's system.
STEPS = [
    ("start", "showcase.start", "Cold start", "service started until the start screen", "start.screen", 5.0),
    ("nav", "showcase.nav", "Moving in a list", "press until the move is finished", "nav.done", 8.0),
    ("launch", "showcase.launch", "Starting a game", "from the last press (EmulationStation: A, Right, A in its Savestate manager; pocket-ui: A) until the game is on screen", "launch.{a}.done", 4.5),
    ("pause", "showcase.pause", "Pause menu", "RetroArch: HOME+X, pocket-ui: HOME, until the menu is there", "pause.{a}.done", 2.4),
    ("back", "showcase.back", "Back to the game", "", None, 2.4),
    ("quit", "showcase.quit", "Quit to the list", "from the confirming press (RetroArch: HOME+START twice; pocket-ui: Quit in its menu) until the list is there", "quit.{a}.done", 2.8),
    ("launch2", "showcase.launch2", "Another game", "from the last press until the game is on screen", "launch.{b}.done", 4.5),
]


def esc(t):
    return t.replace("\\", "\\\\").replace(":", "\\:").replace("'", "’").replace(",", "\\,")


def text(t, font, size, x, y, color="white", start=None, end=None, box=None):
    en = f":enable='between(t,{start:.3f},{end:.3f})'" if start is not None else ""
    bx = f":box=1:boxcolor={box}:boxborderw=12" if box else ""
    return f"drawtext=fontfile={font}:expansion=none:text='{esc(t)}':fontsize={size}:fontcolor={color}:x={x}:y={y}{bx}{en}"


def run(args):
    subprocess.run(["ffmpeg", "-v", "error", "-y", *args], check=True)


def duration(path):
    return float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path],
                                capture_output=True, text=True, check=True).stdout)


def has_audio(path):
    out = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "a", "-show_entries", "stream=index", "-of", "csv=p=0", path],
                         capture_output=True, text=True).stdout
    return bool(out.strip())


# --- lining up a clip with the presses -----------------------------------------------------------------------------

def video_brightness(path, w=48, h=48):
    """[(t_s, mean)] of the video's frames (their own pts), from a small grey picture of each."""
    probe = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "frame=best_effort_timestamp_time",
                            "-of", "csv=p=0", path], capture_output=True, text=True, check=True).stdout.split()
    pts = [float(x.strip(",")) for x in probe if x.strip(",")]
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-vf", f"scale={w}:{h},format=gray", "-f", "rawvideo", "-"],
                         capture_output=True, check=True).stdout
    a = np.frombuffer(raw, dtype=np.uint8)
    n = min(len(pts), len(a) // (w * h))
    means = a[:n * w * h].reshape(n, w * h).mean(axis=1)
    return np.array(pts[:n]) - pts[0], means


def rec_brightness(rec):
    out_t, out_m = [], []
    for f in rec.frames:
        px = np.frombuffer(f.px, dtype=np.uint8).reshape(-1, 4)[:, :3]
        out_t.append(f.t)
        out_m.append(px.mean())
    return np.array(out_t, dtype=np.int64), np.array(out_m)


def on_grid(t_s, v, t0, t1, step=1 / 60):
    """Step-hold resampling onto a grid from t0 to t1 (s)."""
    grid = np.arange(t0, t1, step)
    idx = np.searchsorted(t_s, grid, side="right") - 1
    return grid, np.where(idx >= 0, v[np.clip(idx, 0, len(v) - 1)], v[0])


def video_anchor(rec, video, guess_ns, search_s=(-1.0, 4.0)):
    """The video's t=0 on CLOCK_MONOTONIC (ns): the shift at which the brightness changes of the video and of the
    sampling recording fit best. Returns (anchor_ns, score 0..1)."""
    vt, vm = video_brightness(video)
    rt, rm = rec_brightness(rec)
    rts = (rt - guess_ns) / 1e9  # recording on the guess's scale
    end = min(vt[-1], 60.0)
    _, gv = on_grid(vt, vm, 0, end)
    dv = np.abs(np.diff(gv))
    best = (None, -1.0)
    for k in range(int(search_s[0] * 60), int(search_s[1] * 60)):
        shift = k / 60
        _, gr = on_grid(rts - shift, rm, 0, end)
        dr = np.abs(np.diff(gr))
        den = np.linalg.norm(dv) * np.linalg.norm(dr)
        score = float(dv @ dr / den) if den else 0.0
        if score > best[1]:
            best = (shift, score)
    shift, score = best
    # Refine to the millisecond around the best frame.
    for ms in range(-16, 17):
        sh = shift + ms / 1000
        _, gr = on_grid(rts - sh, rm, 0, end, step=1 / 240)
        _, gv2 = on_grid(vt, vm, 0, end, step=1 / 240)
        a, b = np.abs(np.diff(gv2)), np.abs(np.diff(gr))
        den = np.linalg.norm(a) * np.linalg.norm(b)
        sc = float(a @ b / den) if den else 0.0
        if sc > score:
            shift, score = sh, sc
    return guess_ns + int(shift * 1e9), score


# --- the cut -------------------------------------------------------------------------------------------------------

def load_showcase(d, ui):
    events, recs = rc.load_run(os.path.join(d, "out", ui))
    rec = rc.Merged(recs)
    video = os.path.join(d, "out", ui, "showcase.mp4")
    if not os.path.exists(video):
        raise SystemExit(f"{video}: no showcase clip (run with --scenarios showcase)")
    wf = [e for e in events if e["kind"] == "wf-start"]
    if not wf:
        raise SystemExit(f"{d}/out/{ui}: no wf-start in events.jsonl")
    anchor, score = video_anchor(rec, video, wf[-1]["t"])
    presses = [e for e in events if e["kind"] == "press" and e["name"].startswith("showcase.") and e["t"] >= wf[-1]["t"]]
    args = next((e["args"] for e in events if e["kind"] == "run-start"), {})
    return {"video": video, "anchor": anchor, "score": score, "presses": presses, "args": args}


def number(results, ui, metric):
    m = (results.get(ui) or {}).get("metrics", {}).get(metric) if metric else None
    if not m or m.get("median") is None:
        return None, 0
    return m["median"], m["n"]


def fmt_ms(v, metric=""):
    if v is None:
        return "not measured"
    if metric.startswith(("cpu.", "play.")):
        return f"{v:.0f} %"
    return f"{v / 1000:.2f} s" if v >= 1000 else f"{v:.0f} ms"


def segment(i, sc, step, results, systems, out, lead, slow=1.0, title_override=None, cap=None):
    """One step side by side: both clips from `lead` s before their press, as long as the shorter gap allows."""
    key, pname, title, what, metric, longest = step
    metric = metric.format(a=systems[0], b=systems[1]) if metric else None
    starts, gaps = {}, []
    for ui in ("es", "pocket-ui"):
        ps = sc[ui]["presses"]
        idx = [k for k, e in enumerate(ps) if e["name"] == pname]
        if not idx:
            return None
        k = idx[0]
        t = ps[k]["t"]
        nxt = next((e["t"] for e in ps[k + 1:] if e["name"] != pname), None)
        starts[ui] = (t - sc[ui]["anchor"]) / 1e9
        if nxt:
            gaps.append((nxt - t) / 1e9)
    dur = min([cap or longest] + [g - 0.15 for g in gaps])
    dur = max(1.0, dur)
    total = (lead + dur) * slow
    vals = {ui: number(results, ui, metric) for ui in ("es", "pocket-ui")}
    ns = sorted({n for _, n in vals.values() if n})
    runs = "/".join(str(n) for n in ns)
    note = (what + "  ·  " if what else "") + (f"median of {runs} measured run{'' if ns == [1] else 's'}" if ns else "")
    vf = []
    # Both clips upright (transpose=2), 900 wide, from their press minus the lead.
    inputs = []
    for k, ui in enumerate(("es", "pocket-ui")):
        ss = max(0.0, starts[ui] - lead)
        inputs += ["-ss", f"{ss:.3f}", "-t", f"{(lead + dur):.3f}", "-i", sc[ui]["video"]]
        vf.append(f"[{k}:v]transpose=2,scale=900:784,setsar=1,setpts={slow}*(PTS-STARTPTS)[v{k}]")
    vf.append(f"color=c={BG}:s=1920x1080:r=60:d={total:.3f}[bg]")
    vf.append("[bg][v0]overlay=40:130:eof_action=pass[b0]")
    vf.append("[b0][v1]overlay=980:130:eof_action=pass[b1]")
    draw = [
        text(title_override or title, FONT_T, 52, "(w-text_w)/2", 28),
        text(NAMES["es"], FONT_B, 28, "490-text_w/2", 88, color=GREY),
        text(NAMES["pocket-ui"], FONT_B, 28, "1430-text_w/2", 88, color=GREY),
    ]
    if metric:
        for x, ui in ((490, "es"), (1430, "pocket-ui")):
            draw.append(text(fmt_ms(vals[ui][0]), FONT_T, 54, f"{x}-text_w/2", 930, start=lead * slow, end=total))
    if note:
        draw.append(text(note, FONT_B, 26, "(w-text_w)/2", 1010, color=DIM))
    # The press, on both, for half a second.
    badge = "pressed" if key != "start" else "service started"
    for x in (60, 1000):
        draw.append(text(badge, FONT_B, 26, x, 860, color="black", start=lead * slow, end=(lead + 0.5) * slow, box="0xffffff@0.85"))
    vf.append("[b1]" + ",".join(draw) + "[v]")
    path = os.path.join(out, f"{i:02d}-{key}{'-slow' if slow != 1 else ''}.mp4")
    audio = []
    aud = sc.get("audio_from")
    if aud and slow == 1 and has_audio(sc[aud]["video"]):
        vf.append(f"[{0 if aud == 'es' else 1}:a]asetpts=PTS-STARTPTS,apad,atrim=0:{total:.3f}[a]")
        audio = ["-map", "[a]"]
    else:
        inputs += ["-f", "lavfi", "-t", f"{total:.3f}", "-i", "anullsrc=r=48000:cl=stereo"]
        audio = ["-map", "2:a"]
    run([*inputs, "-filter_complex", ";".join(vf), "-map", "[v]", *audio, "-t", f"{total:.3f}", *ENC, *AENC, path])
    return path


def card(path, secs, title, lines, small=""):
    vf = [text(title, FONT_T, 84, "(w-text_w)/2", 300 if len(lines) > 3 else 380)]
    y0 = 430 if len(lines) > 3 else 510
    for k, line in enumerate(lines):
        vf.append(text(line, FONT_B, 34 if len(lines) > 3 else 40, "(w-text_w)/2", y0 + k * (50 if len(lines) > 3 else 56), color=GREY))
    if small:
        vf.append(text(small, FONT_B, 26, "(w-text_w)/2", 960, color=DIM))
    run(["-f", "lavfi", "-i", f"color=c={BG}:s=1920x1080:r=60:d={secs}", "-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo",
         "-shortest", "-vf", ",".join(vf), *ENC, *AENC, path])
    return path


SUMMARY = [
    ("Cold start", "start.screen"), ("List move", "nav.done"), ("Game start ({a})", "launch.{a}.done"),
    ("Game start ({b})", "launch.{b}.done"), ("Pause menu", "pause.{a}.done"), ("Quit to the list", "quit.{a}.done"),
    ("Switch to another game (8 / 4 presses)", "switch.total"),
    ("Back from deep sleep: a press acted on *", "wake.input"),
    ("Playing {a}: the emulator's CPU **", "play.{a}.game_cpu"),
    ("Playing {c}: the emulator's CPU **", "play.{c}.game_cpu"),
]
SUMMARY_NOTES = ["* pocket-ui takes no button for 0.6 s after a wake, on purpose.   ** One core = 100 %, five minutes from the game's power-on."]


def summary_rows(results, systems):
    rows = []
    for label, metric in SUMMARY:
        metric = metric.format(a=systems[0], b=systems[1], c="snes")
        es, _ = number(results, "es", metric)
        pu, _ = number(results, "pocket-ui", metric)
        if es is None and pu is None:
            continue
        rows.append((label.format(a=systems[0].upper(), b=systems[1].upper(), c="SNES"), fmt_ms(es, metric), fmt_ms(pu, metric)))
    return rows


def summary_card(path, secs, rows, small, device=""):
    """The measured medians as a table: what, EmulationStation + RetroArch, pocket-ui."""
    vf = [text("Measured on the device", FONT_T, 68, "(w-text_w)/2", 90)]
    y = 210
    vf += [text(NAMES["es"], FONT_B, 28, "1270-text_w/2", y, color=GREY), text(NAMES["pocket-ui"], FONT_B, 28, "1640-text_w/2", y, color=GREY)]
    for k, (label, es, pu) in enumerate(rows):
        yy = y + 60 + k * 56
        vf += [text(label, FONT_B, 32, 180, yy), text(es, FONT_T, 40, "1270-text_w/2", yy - 5), text(pu, FONT_T, 40, "1640-text_w/2", yy - 5)]
    yy = y + 60 + len(rows) * 56 + 20
    for note in SUMMARY_NOTES:
        vf.append(text(note, FONT_B, 24, 180, yy, color=GREY))
        yy += 36
    vf.append(text(small, FONT_B, 24, "(w-text_w)/2", 980, color=DIM))
    if device:
        vf.append(text(device, FONT_B, 22, "(w-text_w)/2", 1020, color=DIM))
    run(["-f", "lavfi", "-i", f"color=c={BG}:s=1920x1080:r=60:d={secs}", "-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo",
         "-shortest", "-vf", ",".join(vf), *ENC, *AENC, path])
    return path


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("showcase")
    ap.add_argument("--numbers", required=True, help="the measurement run's folder (es.json, pocket-ui.json)")
    ap.add_argument("--out", default=os.path.join(os.getcwd(), "pocket-ui-vs-rocknix-speed.mp4"))
    ap.add_argument("--work", default="/tmp/compare-video")
    ap.add_argument("--audio", choices=("none", "es", "pocket-ui"), default="none")
    ap.add_argument("--no-slowmo", dest="slowmo", action="store_false")
    a = ap.parse_args()
    os.makedirs(a.work, exist_ok=True)
    os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
    results = {}
    for ui in ("es", "pocket-ui"):
        p = os.path.join(a.numbers, f"{ui}.json")
        if os.path.exists(p):
            results[ui] = json.load(open(p))
    if not results:
        raise SystemExit(f"{a.numbers}: no es.json or pocket-ui.json (the numbers come only from a measurement run)")
    sc = {ui: load_showcase(a.showcase, ui) for ui in ("es", "pocket-ui")}
    for ui in sc:
        print(f"video: {ui}: clip starts at {sc[ui]['anchor']} ns on the presses' clock (fit {sc[ui]['score']:.2f})", file=sys.stderr)
        if sc[ui]["score"] < 0.5:
            print(f"video: {ui}: the fit is poor; check the lining up in the strips (thumbs)", file=sys.stderr)
    sc["audio_from"] = None if a.audio == "none" else a.audio
    systems = sc["pocket-ui"]["args"].get("showcase_systems", "gba,gbc").split(",")
    parts = [card(os.path.join(a.work, "00-title.mp4"), 4, "pocket-ui and stock ROCKNIX",
                  ["The same presses on the same Retroid Pocket Mini V2"],
                  "Recorded on the device in real time.  Left: EmulationStation + RetroArch.  Right: pocket-ui.")]
    for i, step in enumerate(STEPS, 1):
        p = segment(i, sc, step, results, systems, a.work, lead=1.0 if step[0] == "start" else 0.6)
        if p:
            parts.append(p)
        if step[0] == "launch" and a.slowmo:
            p = segment(i, sc, step, results, systems, a.work, lead=0.25, slow=4.0, cap=1.4,
                        title_override="Starting a game, four times slower")
            if p:
                parts.append(p)
    meta = results.get("pocket-ui", {}).get("dir", "")
    dev = {}
    p = os.path.join(a.numbers, "out", "device.json")
    if os.path.exists(p):
        try:
            dev = json.load(open(p))
        except ValueError:
            pass
    device = "  ·  ".join(x for x in (f"ROCKNIX {dev['rocknix']}" if dev.get("rocknix") else "",
                                       f"EmulationStation theme {dev['es_theme']}" if dev.get("es_theme") else
                                       "EmulationStation's default theme" if dev else "") if x)
    parts.append(summary_card(os.path.join(a.work, "90-summary.mp4"), 9, summary_rows(results, systems),
                              "Medians of the measurement runs, from the press to the first frame showing the result (16.7 ms resolution).",
                              device))
    parts.append(card(os.path.join(a.work, "99-end.mp4"), 3, "pocket-ui", ["Built on ROCKNIX for the Retroid Pocket Mini V2"]))
    faded = []
    for p in parts:
        d = duration(p)
        f = p[:-4] + "-f.mp4"
        run(["-i", p, "-vf", f"fade=t=in:st=0:d=0.3,fade=t=out:st={d - 0.3:.3f}:d=0.3",
             "-af", f"afade=t=in:st=0:d=0.3,afade=t=out:st={d - 0.3:.3f}:d=0.3", *ENC, *AENC, f])
        faded.append(f)
    lst = os.path.join(a.work, "list.txt")
    with open(lst, "w") as f:
        f.writelines(f"file '{p}'\n" for p in faded)
    run(["-f", "concat", "-safe", "0", "-i", lst, "-c", "copy", "-movflags", "+faststart", a.out])
    # What the video was made from, next to it: where each clip was placed on the presses' clock, the numbers shown.
    prov = {"showcase": os.path.abspath(a.showcase), "numbers": os.path.abspath(a.numbers),
            "clips": {ui: {"anchor_ns": sc[ui]["anchor"], "fit": sc[ui]["score"]} for ui in ("es", "pocket-ui")},
            "shown": {f"{label}": {ui: number(results, ui, metric.format(a=systems[0], b=systems[1], c="snes")) for ui in ("es", "pocket-ui")}
                      for label, metric in SUMMARY}}
    json.dump(prov, open(os.path.splitext(a.out)[0] + ".json", "w"), indent=1)
    print(f"video: {a.out} ({duration(a.out):.0f} s; numbers from {meta or a.numbers})", file=sys.stderr)


if __name__ == "__main__":
    main()
