#!/usr/bin/env python3
"""Evaluation half of scripts/big-emu-bench.sh (docs/BIG-EMULATORS.md "Measuring Wii and 3DS on the device").
    big-emu-bench.py report RUNDIR...   one table and a plain verdict per game (RUNDIR: what the device half wrote)
    big-emu-bench.py thresholds         the limits behind the verdicts
    big-emu-bench.py selftest           the statistics and verdicts on made-up runs

A RUNDIR holds `meta` (key=value lines), `samples.csv` (one line per second) and `frames.txt` (CLOCK_MONOTONIC ns of
every presented frame, deploy/frametime.c)."""
import csv
import os
import statistics
import sys

# The limits. A game is judged against its target frame rate (60 unless the game is given as ROM@30 or ROM@50).
FULL_MEAN = 0.95    # full speed: average fps at least this share of the target ...
FULL_LOW = 0.80     # ... the 1 % low (the slowest 1 % of the frames, as fps) at least this share ...
FULL_LONG = 0.005   # ... at most this share of the frames long (slower than 1.5 frames) ...
FULL_XRUNS = 0      # ... and no audio underruns
PLAY_MEAN = 0.80    # playable: average at least this share, the 1 % low at least PLAY_LOW; below: too slow
PLAY_LOW = 0.50
LONG_FACTOR = 1.5   # a long frame takes longer than this many target frames (25 ms at 60 fps)
FREEZE_MS = 100.0   # a freeze: no new picture for this long
HOT_C = 70.0        # a note when the CPU or GPU got hotter than this


def thresholds_text():
    return "\n".join([
        "Verdicts, against the target frame rate (60 fps unless the game is given as ROM@30 or ROM@50):",
        f"  full speed  average >= {FULL_MEAN:.0%}, 1 % low >= {FULL_LOW:.0%}, long frames (> {LONG_FACTOR} frames) <= {FULL_LONG:.1%}, "
        f"audio underruns = {FULL_XRUNS}",
        f"  playable    average >= {PLAY_MEAN:.0%} and 1 % low >= {PLAY_LOW:.0%}, but something above is missed (the reason is named)",
        "  too slow    below that",
        f"  freeze      no new picture for {FREEZE_MS:.0f} ms; 1 % low = fps of the slowest 1 % of the frames",
    ])


def read_kv(path):
    d = {}
    with open(path) as f:
        for line in f:
            if "=" in line:
                k, v = line.rstrip("\n").split("=", 1)
                d[k] = v
    return d


def num(x, default=None):
    try:
        return float(x)
    except (TypeError, ValueError):
        return default


def frame_intervals(ts_ns, t0, t1):
    """Frame-to-frame times in ms of the frames presented in [t0, t1] (CLOCK_MONOTONIC seconds)."""
    t = [x / 1e9 for x in ts_ns if t0 <= x / 1e9 <= t1]
    return [(b - a) * 1000.0 for a, b in zip(t, t[1:])], t


def frame_stats(ts_ns, t0, t1, target):
    iv, t = frame_intervals(ts_ns, t0, t1)
    if len(iv) < 30:
        return None
    total = sum(iv) / 1000.0
    worst = sorted(iv, reverse=True)[: max(1, len(iv) // 100)]
    limit = LONG_FACTOR * 1000.0 / target
    # fps of each second of the window, to see slow stretches the average hides
    secs = {}
    for x in t:
        secs[int(x - t0)] = secs.get(int(x - t0), 0) + 1
    full_secs = [n for s, n in secs.items() if s < int(t1 - t0)]
    return {
        "frames": len(iv),
        "fps": len(iv) / total,
        "low1": 1000.0 / statistics.mean(worst),
        "long": sum(x > limit for x in iv),
        "long_share": sum(x > limit for x in iv) / len(iv),
        "freezes": sum(x > FREEZE_MS for x in iv),
        "max_ms": max(iv),
        "slow_secs": sum(n < 0.9 * target for n in full_secs),
        "secs": len(full_secs),
    }


def judge(target, fs, xruns, aborted):
    """(verdict, [reasons]) from the numbers. verdict: full speed | playable | too slow | aborted | no frame data."""
    if aborted:
        return "aborted", [aborted]
    if fs is None:
        return "no frame data", ["no frames were presented, or the frame logger did not reach the emulator"]
    miss = []
    if fs["fps"] < FULL_MEAN * target:
        miss.append(f"average {fs['fps']:.1f} fps is below {FULL_MEAN:.0%} of {target:g}")
    if fs["low1"] < FULL_LOW * target:
        miss.append(f"1 % low {fs['low1']:.1f} fps is below {FULL_LOW:.0%} of {target:g}")
    if fs["long_share"] > FULL_LONG:
        miss.append(f"{fs['long']} long frames ({fs['long_share']:.1%})")
    if xruns is not None and xruns > FULL_XRUNS:
        miss.append(f"{xruns} audio underruns")
    if not miss:
        return "full speed", []
    if fs["fps"] >= PLAY_MEAN * target and fs["low1"] >= PLAY_LOW * target:
        return "playable", miss
    return "too slow", miss


def read_samples(path):
    with open(path) as f:
        return list(csv.DictReader(f))


def sample_stats(rows):
    m = [r for r in rows if r.get("phase") == "m"]
    if len(m) < 2:
        return {}

    def col(name, scale=1.0):
        return [num(r[name]) / scale for r in m if num(r.get(name)) is not None]

    def mean(x):
        return statistics.mean(x) if x else None

    out = {}
    pt, at = col("proc_ticks"), col("all_ticks")
    if len(pt) > 1 and at[-1] > at[0]:
        out["cpu"] = (pt[-1] - pt[0]) * 100.0 / (at[-1] - at[0])  # all_ticks already divided by the number of CPUs
    for name, key, scale in (("big_khz", "big", 1e3), ("prime_khz", "prime", 1e3), ("gpu_hz", "gpu", 1e6)):
        c = col(name, scale)
        out[key + "_mean"], out[key + "_max"] = mean(c), (max(c) if c else None)
    for name, key in (("cpu_mc", "cpu_c"), ("gpu_mc", "gpu_c")):
        c = col(name, 1000.0)
        out[key + "_max"] = max(c) if c else None
    pwm = col("pwm")
    out["pwm_max"] = max(pwm) if pwm else None
    watts = [abs(num(r["cur_ua"])) * num(r["volt_uv"]) / 1e12 for r in m
             if r.get("bat_status") == "Discharging" and num(r.get("cur_ua")) is not None and num(r.get("volt_uv")) is not None]
    out["watts"] = mean(watts)
    # "went off": it was lit when the run began and is dark later (a screen that was dark on purpose is no finding)
    lit = [num(r.get("bright"), 1) for r in m]
    out["screen_off"] = bool(lit) and lit[0] != 0 and any(v == 0 for v in lit)
    return out


def evaluate(rundir):
    meta = read_kv(os.path.join(rundir, "meta"))
    target = num(meta.get("target"), 60.0)
    name = meta.get("game", os.path.basename(rundir))
    rows = read_samples(os.path.join(rundir, "samples.csv")) if os.path.exists(os.path.join(rundir, "samples.csv")) else []
    ts = []
    fp = os.path.join(rundir, "frames.txt")
    if os.path.exists(fp):
        with open(fp) as f:
            ts = [int(x) for x in f if x.strip().isdigit()]
    t0, t1 = num(meta.get("mono_start"), 0.0), num(meta.get("mono_end"), 0.0)
    fs = frame_stats(ts, t0, t1, target) if t1 > t0 else None
    ss = sample_stats(rows)
    xruns = int(num(meta.get("xruns"), 0)) if meta.get("xruns") not in (None, "") else None
    status = meta.get("status", "failed")
    reason = meta.get("reason") or status
    if status == "failed":
        verdict, why = "did not run", [reason]
    else:
        verdict, why = judge(target, fs, xruns, reason if status == "aborted" else None)
    notes = []
    if ss.get("screen_off"):
        notes.append("the screen went off during the run: frame pacing may be wrong")
    hot = max([x for x in (ss.get("cpu_c_max"), ss.get("gpu_c_max")) if x is not None], default=0)
    if hot > HOT_C:
        notes.append(f"got hot ({hot:.0f} °C)")
    if meta.get("saves_changed"):
        notes.append("the run changed files of the emulators, now put back: " + meta["saves_changed"])
    return {"name": name, "system": meta.get("system", "?"), "target": target, "meta": meta, "frames": fs, "samples": ss,
            "xruns": xruns, "verdict": verdict, "why": why, "notes": notes}


def f1(x, unit="", dash="-"):
    return dash if x is None else f"{x:.1f}{unit}"


def f0(x, dash="-"):
    return dash if x is None else f"{x:.0f}"


def table(results):
    head = ["game", "sys", "fps avg", "1% low", "long", "freeze", "xruns", "CPU%", "big/prime/GPU MHz", "°C cpu/gpu", "fan", "battery", "verdict"]
    rows = []
    for r in results:
        fs, ss, m = r["frames"], r["samples"], r["meta"]
        bat = "-"
        if m.get("bat_start") and m.get("bat_end"):
            bat = f"{m['bat_start']}→{m['bat_end']} %"
            if ss.get("watts") is not None:
                bat += f" {ss['watts']:.1f} W"
        rows.append([
            r["name"][:28], r["system"],
            f1(fs and fs["fps"]), f1(fs and fs["low1"]),
            "-" if not fs else f"{fs['long']} ({fs['long_share']:.1%})", "-" if not fs else str(fs["freezes"]),
            "-" if r["xruns"] is None else str(r["xruns"]), f0(ss.get("cpu")),
            f"{f0(ss.get('big_mean'))}/{f0(ss.get('prime_mean'))}/{f0(ss.get('gpu_mean'))}" if ss else "-",
            f"{f0(ss.get('cpu_c_max'))}/{f0(ss.get('gpu_c_max'))}" if ss else "-",
            "-" if ss.get("pwm_max") is None else f"{ss['pwm_max']:.0f}", bat, r["verdict"],
        ])
    w = [max(len(head[i]), *(len(row[i]) for row in rows)) if rows else len(head[i]) for i in range(len(head))]
    line = lambda cells: "  ".join(c.ljust(w[i]) for i, c in enumerate(cells)).rstrip()
    out = [line(head), line(["-" * x for x in w])] + [line(r) for r in rows]
    return "\n".join(out)


def sentence(r):
    t = f"{r['target']:g}"
    v = r["verdict"]
    who = f"{r['name']} ({r['system']})"
    if v == "full speed":
        s = f"{who}: full speed - {r['frames']['fps']:.1f} fps of {t}, 1 % low {r['frames']['low1']:.1f}, no audio underruns."
    elif v == "playable":
        s = f"{who}: playable, not full speed - " + "; ".join(r["why"]) + "."
    elif v == "too slow":
        s = f"{who}: too slow - " + "; ".join(r["why"]) + "."
    elif v == "aborted":
        s = f"{who}: aborted - {r['why'][0]}; the numbers up to there are in the table, no verdict."
    else:
        s = f"{who}: {v} - {r['why'][0]}."
    for n in r["notes"]:
        s += f"\n  note: {n}"
    return s


def report(dirs):
    results = [evaluate(d) for d in dirs]
    print(table(results))
    print()
    for r in results:
        print(sentence(r))
    print()
    print("Measured on what the game shows after its start (title, intro or demo): menus are lighter than play.")
    print(thresholds_text())
    return results


def selftest():
    import tempfile

    def make(root, name, fps, jitter=0.0, secs=60, target=60, xruns=0, status="ok", reason="", pwm=0, bright=100):
        d = os.path.join(root, name)
        os.makedirs(d)
        t, ns = 100.0, []
        n = 0
        end = t + secs
        while t < end:
            ns.append(int(t * 1e9))
            n += 1
            step = 1.0 / fps
            if jitter and n % 50 == 0:
                step += jitter
            t += step
        with open(os.path.join(d, "frames.txt"), "w") as f:
            f.write("\n".join(map(str, ns)) + "\n")
        with open(os.path.join(d, "meta"), "w") as f:
            f.write(f"game={name}\nsystem=wii\ntarget={target}\nmono_start=100\nmono_end={100 + secs}\nstatus={status}\nreason={reason}\n"
                    f"xruns={xruns}\nbat_start=80\nbat_end=79\n")
        with open(os.path.join(d, "samples.csv"), "w") as f:
            f.write("phase,t,proc_ticks,all_ticks,little_khz,big_khz,prime_khz,gpu_hz,cpu_mc,gpu_mc,pwm,bat_pct,bat_status,bright,cur_ua,volt_uv\n")
            for i in range(secs):
                f.write(f"m,{i},{i * 130},{i * 100},1000000,2000000,2400000,305000000,48000,45000,{pwm},80,Discharging,{bright},-500000,3900000\n")
        return d

    failed = []

    def check(what, got, want):
        if got != want:
            failed.append(f"{what}: got {got!r}, wanted {want!r}")

    with tempfile.TemporaryDirectory() as root:
        r = evaluate(make(root, "smooth", 60.0))
        check("smooth verdict", r["verdict"], "full speed")
        check("smooth fps", round(r["frames"]["fps"]), 60)
        check("smooth long", r["frames"]["long"], 0)
        check("cpu share", round(r["samples"]["cpu"]), 130)
        check("watts", round(r["samples"]["watts"], 2), 1.95)
        r = evaluate(make(root, "slow", 35.0))
        check("slow verdict", r["verdict"], "too slow")
        r = evaluate(make(root, "mid", 52.0))
        check("mid verdict", r["verdict"], "playable")
        r = evaluate(make(root, "thirty", 30.0, target=30))
        check("30 fps game at its target", r["verdict"], "full speed")
        r = evaluate(make(root, "thirty-as-sixty", 30.0))
        check("30 fps game against 60", r["verdict"], "too slow")
        r = evaluate(make(root, "audio", 60.0, xruns=3))
        check("audio verdict", r["verdict"], "playable")
        check("audio reason", any("underruns" in w for w in r["why"]), True)
        r = evaluate(make(root, "jitter", 60.0, jitter=0.012))
        check("jitter verdict", r["verdict"], "playable")
        check("jitter long frames", r["frames"]["long"] > 0, True)
        r = evaluate(make(root, "fan", 60.0, status="aborted", reason="the fan started (pwm 51)", pwm=51))
        check("aborted verdict", r["verdict"], "aborted")
        r = evaluate(make(root, "dark", 60.0, bright=0))
        check("dark on purpose: no note", any("screen went off" in n for n in r["notes"]), False)
        rows = [{"phase": "m", "bright": b, "proc_ticks": i, "all_ticks": i} for i, b in enumerate(["100", "100", "0"])]
        check("screen goes off midway: noted", sample_stats(rows).get("screen_off"), True)
        d = make(root, "empty", 60.0)
        open(os.path.join(d, "frames.txt"), "w").close()
        check("empty verdict", evaluate(d)["verdict"], "no frame data")
        out = table([evaluate(os.path.join(root, n)) for n in ("smooth", "slow", "fan")])
        check("table rows", len(out.splitlines()), 5)
        check("table has verdict header", out.splitlines()[0].rstrip().endswith("verdict"), True)
    # the 1 % low is the mean of the slowest 1 % of frames
    ts = [int(i * 1e9 / 60) for i in range(600)]
    ts += [ts[-1] + int(0.5e9)]
    fs = frame_stats(ts, 0, 1000, 60)
    check("one 500 ms frame in 600 is a freeze", fs["freezes"], 1)
    if failed:
        print("selftest FAILED:\n  " + "\n  ".join(failed))
        return 1
    print("selftest ok")
    return 0


def main(argv):
    if len(argv) >= 2 and argv[1] == "report" and len(argv) > 2:
        report(argv[2:])
        return 0
    if len(argv) == 2 and argv[1] == "thresholds":
        print(thresholds_text())
        return 0
    if len(argv) == 2 and argv[1] == "selftest":
        return selftest()
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
