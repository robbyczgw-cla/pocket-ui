#!/usr/bin/env python3
"""Builds assets/curatordb.bin, the feel tags behind Curator (docs/CURATOR.md, layer B).

    scripts/build-curatordb.py golden [--model luna|sol|deepseek]   label the golden set, report precision/recall
    scripts/build-curatordb.py label [--library LIST] [--limit N]   label every game in scope (resumable)
    scripts/build-curatordb.py cross [--library LIST] [--sample N]  label a sample again with the other model family
    scripts/build-curatordb.py import --model claude FILE...        add hand/second-family labels (compact lines, see `import_labels`)
    scripts/build-curatordb.py write                                write assets/curatordb.bin from the cache
    scripts/build-curatordb.py vocab                                only the vocabulary section again (a genre_map change,
                                                                    no new tags): every label section kept byte for byte
    scripts/build-curatordb.py report                               coverage, agreement and example tag sets

**What it does.** For every game in assets/gamedb.bin with sitelinks >= 3 (plus every game of a library
list, which goes first, and a fixed random sample of lesser-known games as privacy chaff, see `scope`),
a language model is asked, in batches of about 40 games, which tags of the
vocabulary in assets/data/curator-tags.json describe the game, with a weight of 1-3, and for its length,
session, difficulty, whether it is text-heavy, and how sure it is. The model sees only the vocabulary
and the Wikidata facts of the game (CC0: title, systems, year, developer, genres, series, modes,
mechanics, input methods). It is used as a pure labelling function: no tools, no files, strict JSON out.

- Labeller: `codex exec -m gpt-6-luna -c model_reasoning_effort=medium --sandbox read-only` (4 batches in
  parallel); `--model sol` retries with GPT-6 Sol where Luna is weak.
- Cross-check: another model family labels a sample a second time: `pi --model
  opencode-go/deepseek-v4.1-flash:high` (`cross`), or labels written by Claude and added with `import`
  (2026-10-23: the OpenCode Go key had expired, so the 300-game sample was labelled by Claude Opus 5.5
  without seeing Luna's labels, and again by Muse Wonder Full (Meta), which also labelled the golden set:
  96.2 % precision / 94.8 % recall there against Luna's 92.6 / 91.4). With two checkers a tag ships when two of
  the three labellers gave it (`cross_labels`). In the final data a tag of a cross-checked game is kept only when both
  gave it (see `merge`), and the agreement rates are reported.
- Golden set: src/testdata/curator-golden.json, about 120 games labelled by hand. `golden` measures tag
  precision (tags given that the golden set has as "must" or "ok") and recall (of the "must" tags).

**Cache.** Every answer is kept in ~/.cache/pocket-curator/<model>-v<PROMPT_VERSION>.jsonl (one batch
answer per line), so an interrupted run carries on where it stopped and `write` needs no network.

**Licence.** Wikidata facts are CC0. The tags are our own derived data (pocket-ui, GPL-2.0-or-later),
written by the build from the vocabulary; no Wikipedia or other copyrighted text is sent, stored or
shipped. A library list (file names of someone's games) is private: it is read, matched to QIDs by the
device's own rules (examples/gamedb_match.rs), and neither it nor its file names are written anywhere.
The local cache holds answers in the order they were made (library first): keep it private.

**File** (read by src/curatordb.rs), the same pattern as gamedb.bin:

    PCDB1\\n
    _vocab <offset> <length>\\n     the vocabulary (assets/data/curator-tags.json, compact)
    <system> <offset> <length>\\n    one line per system; offsets count from the end of the header
    \\n
    <zlib stream per section>       per system one game per line, sorted by QID, tab-separated:
                                    qid, tags ("id:weight,id:weight"), length, session, difficulty,
                                    text_heavy (1/0/empty), confidence (1-3), checked (1 = cross-checked)
"""
import argparse
import concurrent.futures as cf
import hashlib
import json
import os
import random
import re
import subprocess
import sys
import tempfile
import threading
import time
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GAMEDB = ROOT / "assets" / "gamedb.bin"
VOCAB = ROOT / "assets" / "data" / "curator-tags.json"
GOLDEN = ROOT / "src" / "testdata" / "curator-golden.json"
OUT = ROOT / "assets" / "curatordb.bin"
CACHE = Path(os.environ.get("CURATOR_CACHE", Path.home() / ".cache" / "pocket-curator"))
PROMPT_VERSION = 5
BATCH = 40
PARALLEL = 4
MIN_SITELINKS = 3

GAMEDB_FIELDS = ["qid", "label", "year", "developers", "publishers", "genres", "series", "modes", "players", "scores", "aliases",
                 "enwiki", "sitelinks", "redump", "work", "mechanics", "inputs", "languages", "related", "follows", "followed"]

LENGTHS = ["short", "medium", "long", "epic", "endless"]
SESSIONS = ["pickup", "sitdown"]
DIFFICULTIES = ["easy", "moderate", "hard", "brutal"]

# Hardware a control or multiplayer tag needs. A game on several systems is labelled once; these keep a
# tag off the systems that cannot have it (no touch screen on the SNES release of Chrono Trigger).
HANDHELD_LINK = {"gb", "gbc", "gba", "nds", "3ds", "psp", "ngp", "ngpc", "gamegear", "atarilynx", "wonderswan", "wonderswancolor", "psx", "saturn"}
NO_ANALOG = {"nes", "fds", "snes", "gb", "gbc", "gba", "megadrive", "mastersystem", "gamegear", "sg-1000", "pcengine", "pcenginecd", "ngp",
             "ngpc", "atarilynx", "atari2600", "atari7800", "wonderswan", "wonderswancolor", "virtualboy", "segacd", "sega32x", "nds",
             "pokemini"}
HOME = {"nes", "fds", "snes", "n64", "gamecube", "wii", "megadrive", "mastersystem", "segacd", "sega32x", "saturn", "dreamcast",
        "sg-1000", "psx", "ps2", "pcengine", "pcenginecd", "atari2600", "atari7800",
        "naomi", "atomiswave"}  # arcade cabinets: several players at one screen
SYSTEM_ONLY = {
    # Several players on one screen need a home console (handhelds play together over link-play).
    "couch-coop": HOME,
    "couch-versus": HOME,
    "party-4p": HOME,
    "touch-screen": {"nds", "3ds"},
    "two-screens": {"nds", "3ds"},
    "motion-controls": {"wii", "3ds", "ps2", "gba"},  # Wii remote, 3DS gyro, EyeToy, GBA tilt cartridges
    "link-play": HANDHELD_LINK,
}

MODELS = {
    "luna": ["codex", "exec", "-m", "gpt-6-luna", "-c", "model_reasoning_effort=medium"],
    "sol": ["codex", "exec", "-m", "gpt-6-sol", "-c", "model_reasoning_effort=medium"],
    "deepseek": ["pi", "--model", "opencode-go/deepseek-v4.1-flash:high"],
    "claude": None,  # no command: labels come in through `import`
}
CROSS = ["muse", "claude", "deepseek"]  # second-family label sources (Muse Wonder first: 96 % golden precision)

lock = threading.Lock()


# ---------------------------------------------------------------- inputs

def read_gamedb():
    """{system: {qid: row}} from assets/gamedb.bin."""
    b = GAMEDB.read_bytes()
    end = b.index(b"\n\n")
    header = b[:end].decode().split("\n")
    assert header[0] == "PGDB1"
    body = end + 2
    out = {}
    for line in header[1:]:
        system, off, length = line.split()
        text = zlib.decompress(b[body + int(off):body + int(off) + int(length)]).decode()
        rows = {}
        for l in text.splitlines():
            f = l.split("\t")
            f += [""] * (len(GAMEDB_FIELDS) - len(f))
            rows[f[0]] = dict(zip(GAMEDB_FIELDS, f))
        out[system] = rows
    return out


def games_by_qid(db):
    """One entry per QID with the facts the labeller sees and the systems it is on."""
    games = {}
    for system, rows in db.items():
        for q, r in rows.items():
            g = games.setdefault(q, {"qid": q, "title": r["label"], "systems": [], "sitelinks": int(r["sitelinks"] or 0), "row": r})
            g["systems"].append(system)
    return games


def facts(g):
    r = g["row"]
    f = {"qid": g["qid"], "title": r["label"], "systems": g["systems"]}
    for key, field in [("year", "year"), ("developer", "developers"), ("publisher", "publishers"), ("genres", "genres"), ("series", "series"),
                       ("modes", "modes"), ("players", "players"), ("mechanics", "mechanics"), ("inputs", "inputs"), ("also_known_as", "aliases")]:
        v = r[field]
        if not v:
            continue
        if field in ("developers", "publishers", "genres", "series", "modes", "mechanics", "inputs", "aliases"):
            v = v.split("|")[:4 if field == "aliases" else 6]
        f[key] = v
    return f


def library_qids(path):
    """QIDs of a library list (system/file name lines), matched by examples/gamedb_match.rs; private input,
    nothing of it is written out."""
    if not path:
        return []
    lines = [l for l in Path(path).read_text().splitlines() if l.split("/", 1)[0] not in ("ports", "bios", "savestates", "backups")]
    res = subprocess.run(["cargo", "run", "--quiet", "--release", "--example", "gamedb_match"], cwd=ROOT, input="\n".join(lines),
                         capture_output=True, text=True, check=True)
    seen, out = set(), []
    for l in res.stdout.splitlines():
        _, q, _ = l.split("\t")
        if q not in seen:
            seen.add(q)
            out.append(q)
    print(f"library: {len(lines)} files, {len(out)} games matched", file=sys.stderr)
    return out


CHAFF = 400


def scope(games, library):
    """Library games first, then everything with sitelinks >= 3, best known first, then a fixed random
    sample of lesser-known games.

    The sample is privacy chaff: a library game below the threshold would otherwise be in the shipped file
    only because someone owns it, and a public binary would give the library away. With a few hundred
    random lesser-known games beside it (drawn without looking at the library, the same every run) and one
    rule for all of them (`write`: shipped only when the labeller knew the game), it can't be told apart."""
    first = [q for q in library if q in games]
    seen = set(first)
    rest = sorted((q for q, g in games.items() if g["sitelinks"] >= MIN_SITELINKS and q not in seen),
                  key=lambda q: (-games[q]["sitelinks"], q))
    low = sorted(q for q, g in games.items() if g["sitelinks"] < MIN_SITELINKS)
    chaff = [q for q in random.Random(7).sample(low, min(CHAFF, len(low))) if q not in seen]
    return first + rest + chaff


# ---------------------------------------------------------------- prompt

def vocab():
    return json.loads(VOCAB.read_text())


def prompt(v, batch):
    by_axis = {}
    for t in v["tags"]:
        by_axis.setdefault(t["axis"], []).append(t)
    lines = []
    for a in v["axes"]:
        if a["id"] in ("session", "length", "difficulty", "text"):
            continue  # asked as fields
        lines.append(f"[{a['id']}] {a['hint']}")
        for t in by_axis[a["id"]]:
            lines.append(f"  {t['id']}: {t['label']}" + (f" ({t['hint']})" if t.get("hint") else ""))
    games = "\n".join(json.dumps(facts(g), ensure_ascii=False) for g in batch)
    return f"""You are a data-labelling function for an offline game recommender. Do not use tools. Answer with JSON only.

For each game below, pick tags from the vocabulary that truly describe how THE ACTUAL GAME plays and feels, from
your own knowledge of it. The Wikidata facts are only there to identify the game. Precision matters most: a wrong
tag is worse than a missing one. Do not add a tag just because it is loosely possible. Use each tag exactly in
the meaning given in brackets; when a tag only half fits, leave it out.

Rules:
- family: 1 tag (2 only for a true hybrid). subgenre: 0-3. mechanics: 0-4, only distinctive ones.
- pace: exactly 1. mood: 1-3. setting: 0-2. story: "story-driven" only if the story matters (weight = how much);
  "cinematic" and "lore" only when notable. grinding: only if repetition is needed to progress.
- controls: only what clearly applies to the listed systems' versions (touch-screen/two-screens need DS/3DS use,
  needs-analog for 3D movement on systems with sticks, dpad-friendly for games that play fully on a d-pad).
- multiplayer: only modes on ONE system (couch-coop, couch-versus, party-4p, pass-and-play) or with linked
  handhelds (link-play). Nothing for single-player games. Online-only modes do not count.
- weight: 3 = defining, 2 = clearly present, 1 = present but minor.
- length (main game, typical player): short (<5 h) | medium (5-15 h) | long (15-40 h) | epic (40 h+) |
  endless (no real end: score attack, sports, versus, sandbox) | unknown
- session: pickup (fun in 15 minutes or less) | sitdown (needs longer sittings) | unknown
- difficulty (typical player): easy | moderate | hard | brutal | unknown
- text_heavy: true if enjoying it needs reading a lot of text (RPG dialogue, visual novels, adventure), false if
  not, null if unknown.
- confidence: 3 = you know this game well, 2 = roughly, 1 = you do not really know it. With confidence 1 give
  only family/subgenre that the facts make obvious and "unknown"/null for the rest. Never guess details.

Output: ONE JSON object, no prose, no code fences, every qid of the input exactly once:
{{"Q123": {{"tags": {{"tag-id": 3, "other-tag": 1}}, "length": "medium", "session": "pickup", "difficulty": "moderate", "text_heavy": false, "confidence": 3}}}}

Vocabulary (tag id: label (meaning)):
{chr(10).join(lines)}

Games (one JSON object per line):
{games}
"""


# ---------------------------------------------------------------- calling a model

def call(model, text, timeout=1500):
    """Runs one batch through `model`; returns the raw answer text or None."""
    model = model.removesuffix("-golden")
    with tempfile.TemporaryDirectory(prefix="cdat-label-") as d:
        if model in ("luna", "sol"):
            out = Path(d) / "answer.txt"
            cmd = MODELS[model] + ["--sandbox", "read-only", "--skip-git-repo-check", "--ephemeral", "-C", d, "-o", str(out), "-"]
            try:
                r = subprocess.run(cmd, input=text, capture_output=True, text=True, timeout=timeout)
            except subprocess.TimeoutExpired:
                return None
            if r.returncode != 0 or not out.exists():
                print(f"  {model} failed ({r.returncode}): {r.stderr[-400:]}", file=sys.stderr)
                return None
            return out.read_text()
        p = Path(d) / "prompt.md"
        p.write_text(text)
        cmd = MODELS[model] + ["-p", "--no-tools", "--no-session", "--no-extensions", "--no-mcp", "--no-skills", "--no-context-files",
                               "--no-prompt-templates", "--system-prompt", "You are a strict JSON data-labelling function.", f"@{p}"]
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=d)
        except subprocess.TimeoutExpired:
            return None
        if r.returncode != 0 and model == "deepseek":
            # Fallback named in the routing rules: the same model through opencode.
            cmd = ["opencode", "run", "--model", "opencode-go/deepseek-v4.1-flash", "--variant", "high", text]
            try:
                r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=d)
            except subprocess.TimeoutExpired:
                return None
        if r.returncode != 0:
            print(f"  {model} failed ({r.returncode}): {r.stderr[-400:]}", file=sys.stderr)
            return None
        return r.stdout


def parse(answer, v, qids):
    """The answer as {qid: clean label}; tags not in the vocabulary, odd weights and unknown values go."""
    if not answer:
        return {}
    s = answer.strip()
    s = re.sub(r"^```(?:json)?\s*|\s*```$", "", s)
    # One object, or (some answers) one object per game on its own line: read every object in turn.
    data, dec, i = {}, json.JSONDecoder(), s.find("{")
    while 0 <= i < len(s):
        try:
            obj, end = dec.raw_decode(s, i)
        except json.JSONDecodeError:
            break
        if isinstance(obj, dict):
            data.update(obj)
        i = s.find("{", end)
    ids = {t["id"]: t["axis"] for t in v["tags"]}
    out = {}
    for q, e in data.items():
        if q not in qids or not isinstance(e, dict):
            continue
        tags = {}
        raw = e.get("tags", {})
        if isinstance(raw, list):
            raw = {t: 2 for t in raw if isinstance(t, str)}
        if not isinstance(raw, dict):
            raw = {}
        for t, w in raw.items():
            if t in ids and ids[t] not in ("session", "length", "difficulty", "text"):
                try:
                    w = int(w)
                except (TypeError, ValueError):
                    continue
                if 1 <= w <= 3:
                    tags[t] = w
        def pick(key, allowed):
            x = e.get(key)
            return x if x in allowed else None
        th = e.get("text_heavy")
        conf = e.get("confidence")
        out[q] = {
            "tags": tags,
            "length": pick("length", LENGTHS),
            "session": pick("session", SESSIONS),
            "difficulty": pick("difficulty", DIFFICULTIES),
            "text_heavy": th if isinstance(th, bool) else None,
            "confidence": conf if conf in (1, 2, 3) else 1,
        }
    return out


def cache_path(model):
    return CACHE / f"{model}-v{PROMPT_VERSION}.jsonl"


def eval_model(model):
    """The cache name for measuring `model` on the golden set. Luna's golden answers are the pipeline's own
    labels; another model's go to a separate cache, so they never end up in the shipped data and the golden
    test keeps measuring what ships."""
    return model if model == "luna" else f"{model}-golden"


def load_cache(model):
    path = cache_path(model)
    out = {}
    if path.exists():
        for line in path.read_text().splitlines():
            try:
                out.update(json.loads(line))
            except json.JSONDecodeError:
                pass
    return out


def label(model, qids, games, v, done=None, batch_size=BATCH, parallel=PARALLEL):
    """Labels `qids` not in the cache yet; appends every batch answer to the cache. Returns all labels."""
    done = load_cache(model) if done is None else done
    todo = [q for q in qids if q not in done]
    batches = [todo[i:i + batch_size] for i in range(0, len(todo), batch_size)]
    print(f"{model}: {len(qids)} games, {len(qids) - len(todo)} cached, {len(batches)} batches to run", file=sys.stderr)
    CACHE.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()

    def run(i, batch):
        qs = set(batch)
        got = {}
        for attempt in range(3):
            missing = [q for q in batch if q not in got]
            if not missing:
                break
            answer = call(model, prompt(v, [games[q] for q in missing]))
            parsed = parse(answer, v, qs)
            if not parsed and answer:
                # Keep what could not be read, to see why (a cut-off answer, prose around the JSON ...).
                bad = CACHE / "failed"
                bad.mkdir(parents=True, exist_ok=True)
                (bad / f"{model}-{hashlib.sha1(answer.encode()).hexdigest()[:10]}.txt").write_text(answer)
            got.update(parsed)
        with lock:
            with cache_path(model).open("a") as f:
                f.write(json.dumps(got, ensure_ascii=False) + "\n")
            done.update(got)
            n = sum(1 for q in batch if q in got)
            el = time.monotonic() - started
            print(f"  {model} batch {i + 1}/{len(batches)}: {n}/{len(batch)} labelled ({el / 60:.1f} min)", file=sys.stderr)
        return got

    with cf.ThreadPoolExecutor(parallel) as ex:
        list(ex.map(lambda ib: run(*ib), enumerate(batches)))
    return done


# ---------------------------------------------------------------- measuring

def tagset(e, min_weight=1):
    return {t for t, w in e["tags"].items() if w >= min_weight}


def score_golden(labels, golden, title=""):
    tp = fp = must_hit = must_all = 0
    scal = {"length": [0, 0], "session": [0, 0], "difficulty": [0, 0], "text_heavy": [0, 0]}
    per_game = []
    wrong = {}
    for q, gd in golden.items():
        e = labels.get(q)
        if not e:
            continue
        good = set(gd["must"]) | set(gd["ok"])
        # Scored on what ships: the tags as they apply on the golden game's system.
        pred = set(system_tags(e, gd["system"]))
        t = len(pred & good)
        tp += t
        fp += len(pred - good)
        for x in pred - good:
            wrong[x] = wrong.get(x, 0) + 1
        must_hit += len(pred & set(gd["must"]))
        must_all += len(gd["must"])
        per_game.append((t / max(1, len(pred)), gd["title"], sorted(pred - good), sorted(set(gd["must"]) - pred)))
        for k in scal:
            want = gd.get(k)
            if want is None:
                continue
            got = e.get(k)
            if got is None:
                continue
            scal[k][1] += 1
            wants = want if isinstance(want, list) else [want]
            scal[k][0] += got in wants
    n = sum(1 for q in golden if q in labels)
    p = tp / max(1, tp + fp)
    r = must_hit / max(1, must_all)
    print(f"{title}: {n}/{len(golden)} golden games labelled; tag precision {100 * p:.1f} %, recall (must) {100 * r:.1f} %, "
          f"{(tp + fp) / max(1, n):.1f} tags per game")
    print("  scalars right: " + ", ".join(f"{k} {100 * a / max(1, b):.0f} % ({b})" for k, (a, b) in scal.items()))
    print("  most frequent wrong tags: " + ", ".join(f"{t} {c}" for t, c in sorted(wrong.items(), key=lambda kv: -kv[1])[:15]))
    for prec, t, bad, miss in sorted(per_game)[:8]:
        print(f"  worst: {t}: {100 * prec:.0f} % wrong {bad} missing {miss}")
    return p, r


def merge(a, b, axis_of):
    """The cross-checked label of `a` (the labeller) with `b` (the second family).

    Tags: on an axis where `b` gave tags, only tags both gave stay (with the lower weight): the two
    disagree on the rest. On an axis `b` left out (a sparse second label says nothing there) `a`'s tags of
    weight >= MIN_WEIGHT stay as they are, unconfirmed. Weight-1 tags stay only when confirmed.
    Scalars: kept where both agree within one step (the first model's value), else unknown. Confidence is
    the lower of the two."""
    b_axes = {axis_of[t] for t in b["tags"] if t in axis_of}
    tags = {}
    for t, w in a["tags"].items():
        if t in b["tags"]:
            tags[t] = min(w, b["tags"][t])
        elif axis_of.get(t) not in b_axes and w >= MIN_WEIGHT:
            tags[t] = w
    out = {"tags": tags, "confidence": min(a["confidence"], b["confidence"])}
    for k, order in [("length", LENGTHS[:4]), ("difficulty", DIFFICULTIES), ("session", SESSIONS)]:
        x, y = a.get(k), b.get(k)
        if x is None or y is None:
            out[k] = x if y is None else None
        elif x == y:
            out[k] = x
        elif x in order and y in order and abs(order.index(x) - order.index(y)) <= 1 and k != "session":
            out[k] = x
        else:
            out[k] = None
    th_a, th_b = a.get("text_heavy"), b.get("text_heavy")
    out["text_heavy"] = th_a if th_a == th_b or th_b is None else None
    out["checked"] = True
    return out


def agreement(a, b, qids, axis_of=None):
    """How far two labellers agree on `qids`: Jaccard of the tag sets, how many of each one's tags the
    other gave too, the scalars, and (with `axis_of`) the share of `a`'s shipped tags (weight >= 2) that
    `b` confirms on the axes `b` labelled at all."""
    j = []
    inter = na = nb = 0
    on_axes = [0, 0]
    scal = {"length": [0, 0], "session": [0, 0], "difficulty": [0, 0], "text_heavy": [0, 0]}
    for q in qids:
        if q not in a or q not in b:
            continue
        x, y = tagset(a[q]), tagset(b[q])
        if x | y:
            j.append(len(x & y) / len(x | y))
        inter += len(x & y)
        na += len(x)
        nb += len(y)
        if axis_of:
            b_axes = {axis_of.get(t) for t in y}
            shipped = {t for t, w in a[q]["tags"].items() if w >= MIN_WEIGHT and axis_of.get(t) in b_axes}
            on_axes[0] += len(shipped & y)
            on_axes[1] += len(shipped)
        for k in scal:
            if a[q].get(k) is not None and b[q].get(k) is not None:
                scal[k][1] += 1
                scal[k][0] += a[q][k] == b[q][k]
    out = {
        "games": len(j),
        "jaccard": round(sum(j) / max(1, len(j)), 3),
        "a_confirmed": round(inter / max(1, na), 3),
        "b_confirmed": round(inter / max(1, nb), 3),
        "scalars": {k: round(s / max(1, t), 3) for k, (s, t) in scal.items()},
    }
    if axis_of:
        out["a_shipped_confirmed_on_b_axes"] = round(on_axes[0] / max(1, on_axes[1]), 3)
    return out


# ---------------------------------------------------------------- final data

def cross_labels():
    """The second-family labels per game. A game labelled by more than one checker (2026-10-23: Claude and Muse
    Wonder on the same 300 games) gets the union of their tags, with `_agree` holding the tags all of them gave:
    `final_labels` then keeps a labeller tag that at least one checker confirms and adds the tags the checkers
    agree on — a majority of the three. Scalars come from the first checker in `CROSS` that has one."""
    srcs = [load_cache(m) for m in CROSS]
    out = {}
    for q in set().union(*(s.keys() for s in srcs)):
        es = [s[q] for s in srcs if q in s]
        if len(es) == 1:
            out[q] = es[0]
            continue
        tags = {}
        for e in es:
            for t, w in e["tags"].items():
                tags[t] = max(tags.get(t, 0), w)
        u = {"tags": tags, "confidence": max(e["confidence"] for e in es)}
        for k in ("length", "difficulty", "session", "text_heavy"):
            u[k] = next((e.get(k) for e in es if e.get(k) is not None), None)
        u["_agree"] = {t: min(e["tags"][t] for e in es) for t in tags if all(t in e["tags"] for e in es)}
        out[q] = u
    return out


def final_labels(games, v):
    """Luna's labels (Sol's where Luna's were retried), cross-checked ones merged with the second family's."""
    luna = load_cache("luna")
    sol = load_cache("sol")
    other = cross_labels()
    out = {}
    for q, e in luna.items():
        out[q] = dict(e)
    for q, e in sol.items():
        out[q] = dict(e)  # a retry with the stronger model replaces Luna's
    axis_of = {t["id"]: t["axis"] for t in v["tags"]}
    for q in list(out):
        if q in other:
            out[q] = merge(out[q], other[q], axis_of)
            for t, w in other[q].get("_agree", {}).items():
                out[q]["tags"].setdefault(t, w)
    return out


def import_labels(model, files, v):
    """Adds labels written outside the batch labeller (a second model family, by hand) to the cache of
    `model`. One game per line: `QID|tag tag* tag|length|session|difficulty|text_heavy`, where `*` marks a
    defining tag (weight 3, else 2), empty fields are unknown and text_heavy is 1 or 0. A line with two tags
    or fewer counts as confidence 1 (barely known), else 2."""
    ids = {t["id"] for t in v["tags"]}
    got, unknown = {}, set()
    for f in files:
        for line in Path(f).read_text().splitlines():
            if not line.strip():
                continue
            q, tags, length, session, difficulty, text = (line.split("|") + [""] * 6)[:6]
            t = {}
            for x in tags.split():
                w = 3 if x.endswith("*") else 2
                x = x.rstrip("*")
                if x in ids:
                    t[x] = w
                else:
                    unknown.add(x)
            got[q] = {"tags": t, "length": length or None, "session": session or None, "difficulty": difficulty or None,
                      "text_heavy": {"1": True, "0": False}.get(text.strip()), "confidence": 1 if len(t) <= 2 else 2}
    CACHE.mkdir(parents=True, exist_ok=True)
    with cache_path(model).open("a") as fh:
        fh.write(json.dumps(got, ensure_ascii=False) + "\n")
    print(f"{model}: {len(got)} games imported; unknown tags dropped: {sorted(unknown)}", file=sys.stderr)


# Tags of weight 1 ("present but minor") were right only about a third of the time on the golden set
# (weight 2: 86 %, weight 3: 98 %), so they ship only when the cross-check model gave the tag too.
MIN_WEIGHT = 2
# Pace is one value per game, so its weight says nothing about how sure the labeller is (weight-1 pace tags
# were right 92 % of the time on the golden set): it ships at any weight.
SINGLE_AXIS_TAGS = {"pace-calm", "pace-measured", "pace-fast", "pace-frantic"}


def system_tags(e, system):
    """A game's tags as they ship for `system`: weight >= MIN_WEIGHT unless cross-checked, and only tags the
    system's hardware allows (see SYSTEM_ONLY, NO_ANALOG)."""
    tags = {t: w for t, w in e["tags"].items() if w >= MIN_WEIGHT or e.get("checked") or t in SINGLE_AXIS_TAGS}
    for t, systems in SYSTEM_ONLY.items():
        if t in tags and system not in systems:
            del tags[t]
    if system in NO_ANALOG:
        tags.pop("needs-analog", None)
    return tags


def write_vocab(v):
    """`_vocab` of assets/curatordb.bin replaced by assets/data/curator-tags.json, the systems' sections kept as they are.
    Refused when the tags themselves changed: the labels name tags, so that needs `write`."""
    b = OUT.read_bytes()
    end = b.index(b"\n\n")
    header = b[:end].decode().split("\n")
    assert header[0] == "PCDB1", f"{OUT}: not a PCDB1 file"
    body = end + 2
    old = [(n, b[body + int(o):body + int(o) + int(ln)]) for n, o, ln in (l.split() for l in header[1:])]
    shipped = json.loads(zlib.decompress(dict(old)["_vocab"]).decode())
    if shipped["tags"] != v["tags"] or shipped["axes"] != v["axes"]:
        raise SystemExit("the tags or axes changed: run `write` (labels name tags)")
    new = zlib.compress(json.dumps(v, ensure_ascii=False, separators=(",", ":")).encode(), 9)
    blobs, header, offset = [], ["PCDB1"], 0
    for system, blob in old:
        blob = new if system == "_vocab" else blob
        header.append(f"{system} {offset} {len(blob)}")
        blobs.append(blob)
        offset += len(blob)
    data = ("\n".join(header) + "\n\n").encode() + b"".join(blobs)
    tmp = OUT.with_suffix(".tmp")
    tmp.write_bytes(data)
    tmp.replace(OUT)
    print(f"vocabulary written, {len(old) - 1} systems kept, {len(data) / 1e3:.0f} KB -> {OUT}", file=sys.stderr)


def write(games, labels, v):
    order = {t["id"]: i for i, t in enumerate(v["tags"])}
    by_system = {}
    for q, e in labels.items():
        g = games.get(q)
        if not g:
            continue
        # Lesser-known games (library or chaff, see `scope`) only when the labeller knew them.
        if g["sitelinks"] < MIN_SITELINKS and e["confidence"] < 2:
            continue
        for system in g["systems"]:
            tags = system_tags(e, system)
            if not tags and e["confidence"] < 2:
                continue
            cells = [
                q,
                ",".join(f"{t}:{w}" for t, w in sorted(tags.items(), key=lambda kv: order[kv[0]])),
                e.get("length") or "",
                e.get("session") or "",
                e.get("difficulty") or "",
                {True: "1", False: "0"}.get(e.get("text_heavy"), ""),
                str(e["confidence"]),
                "1" if e.get("checked") else "",
            ]
            by_system.setdefault(system, []).append("\t".join(cells))
    blobs, header, offset = [], ["PCDB1"], 0
    # The vocabulary travels in the same file (section "_vocab", the JSON without whitespace), so the reader
    # never sees tags from one build and labels from another.
    sections = [("_vocab", json.dumps(v, ensure_ascii=False, separators=(",", ":")))]
    sections += [(system, "\n".join(sorted(by_system[system])) + "\n") for system in sorted(by_system)]
    for system, text in sections:
        blob = zlib.compress(text.encode(), 9)
        header.append(f"{system} {offset} {len(blob)}")
        blobs.append(blob)
        offset += len(blob)
    data = ("\n".join(header) + "\n\n").encode() + b"".join(blobs)
    tmp = OUT.with_suffix(".tmp")
    tmp.write_bytes(data)
    tmp.replace(OUT)
    print(f"{sum(len(x) for x in by_system.values())} entries in {len(by_system)} systems, {len(data) / 1e3:.0f} KB -> {OUT}", file=sys.stderr)


# ---------------------------------------------------------------- main

def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("command", choices=["golden", "label", "cross", "import", "write", "vocab", "report"])
    ap.add_argument("files", nargs="*", help="label files for `import`")
    ap.add_argument("--model", default="luna", choices=list(MODELS))
    ap.add_argument("--library", help="system/file name list whose games go first (private, never written out)")
    ap.add_argument("--limit", type=int, help="only the first N games of the scope")
    ap.add_argument("--sample", type=int, default=300, help="games for the cross-check")
    ap.add_argument("--systems", help="only games on these systems (comma-separated)")
    args = ap.parse_args()
    v = vocab()
    if args.command == "vocab":
        write_vocab(v)
        return
    db = read_gamedb()
    games = games_by_qid(db)
    golden = json.loads(GOLDEN.read_text())

    if args.command == "import":
        import_labels(args.model, args.files, v)
        return

    if args.command == "golden":
        labels = label(eval_model(args.model), list(golden), games, v)
        score_golden(labels, golden, f"{args.model} v{PROMPT_VERSION}")
        if args.model != "luna":
            luna = load_cache("luna")
            if all(q in luna for q in golden) and args.model in CROSS:
                axis_of = {t["id"]: t["axis"] for t in v["tags"]}
                merged = {q: merge(luna[q], labels[q], axis_of) for q in golden if q in labels}
                score_golden(merged, golden, f"luna x {args.model} (tags both gave)")
                print("agreement on the golden set:", json.dumps(agreement(luna, labels, list(golden)), indent=None))
        return

    library = library_qids(args.library)
    qids = scope(games, library)
    if args.systems:
        want = set(args.systems.split(","))
        qids = [q for q in qids if want & set(games[q]["systems"])]
    if args.limit:
        qids = qids[:args.limit]

    if args.command == "label":
        label(args.model, qids, games, v)
    elif args.command == "cross":
        # The library's games and a random spread of the rest, the same every run.
        rest = qids[len([q for q in library if q in games]):]
        rnd = random.Random(23)
        sample = [q for q in library if q in games][:args.sample]
        sample += rnd.sample(rest, max(0, min(len(rest), args.sample - len(sample))))
        other = "deepseek" if args.model == "luna" else args.model
        b = label(other, sample, games, v, parallel=2)
        a = load_cache("luna")
        print("agreement:", json.dumps(agreement(a, b, sample)))
    elif args.command == "write":
        write(games, final_labels(games, v), v)
    elif args.command == "report":
        report(db, games, qids, golden, v)


def report(db, games, qids, golden, v):
    """Per system: games in gamedb, in scope, shipped with tags, cross-checked, golden precision and the
    agreement of the two labellers; then totals and the file size. Markdown, for docs/CURATOR-DATA.md."""
    labels = final_labels(games, v)
    luna, other = load_cache("luna"), cross_labels()
    axis_of = {t["id"]: t["axis"] for t in v["tags"]}
    shipped = {}  # (system, qid) -> tags as written
    for q, e in labels.items():
        g = games.get(q)
        if not g or (g["sitelinks"] < MIN_SITELINKS and e["confidence"] < 2):
            continue
        for s in g["systems"]:
            t = system_tags(e, s)
            if t or e["confidence"] >= 2:
                shipped[(s, q)] = t
    in_scope = set(qids)
    rows = []
    tot = [0, 0, 0, 0, 0, 0, 0, 0]
    for s in sorted(db):
        n = len(db[s])
        sc = sum(1 for q in db[s] if q in in_scope)
        tagged = sum(1 for q in db[s] if (s, q) in shipped)
        checked = sum(1 for q in db[s] if (s, q) in shipped and labels[q].get("checked"))
        gr = gt = 0
        for q, gd in golden.items():
            if gd["system"] == s and (s, q) in shipped:
                good = set(gd["must"]) | set(gd["ok"])
                gt += len(shipped[(s, q)])
                gr += sum(1 for t in shipped[(s, q)] if t in good)
        cq = [q for q in db[s] if q in other and q in luna]
        ag = agreement(luna, other, cq, axis_of) if cq else None
        rows.append((s, n, sc, tagged, checked, f"{100 * gr / gt:.0f} % ({gt})" if gt else "–",
                     f"{100 * ag['a_shipped_confirmed_on_b_axes']:.0f} % ({ag['games']})" if ag else "–"))
        for i, x in enumerate((n, sc, tagged, checked)):
            tot[i] += x
        tot[4] += gr
        tot[5] += gt
    print("| System | Games (gamedb) | In scope | Tagged | Cross-checked | Golden precision (tags) | Labellers agree (games) |")
    print("|---|---|---|---|---|---|---|")
    for r in rows:
        print("| " + " | ".join(str(x) for x in r) + " |")
    ag = agreement(luna, other, [q for q in other if q in luna], axis_of)
    print(f"| **All** | {tot[0]} | {tot[1]} | {tot[2]} | {tot[3]} | {100 * tot[4] / max(1, tot[5]):.1f} % ({tot[5]}) | "
          f"{100 * ag['a_shipped_confirmed_on_b_axes']:.1f} % ({ag['games']}) |")
    uniq = {q for _, q in shipped}
    print(f"\nUnique games tagged: {len(uniq)}; entries: {len(shipped)}; tags per entry: "
          f"{sum(len(t) for t in shipped.values()) / max(1, len(shipped)):.1f}")
    conf = {c: sum(1 for q in uniq if labels[q]["confidence"] == c) for c in (1, 2, 3)}
    print(f"Confidence: {conf}")
    print("Agreement (all cross-checked games):", json.dumps(ag))
    if OUT.exists():
        print(f"{OUT.name}: {OUT.stat().st_size / 1e3:.0f} KB")

if __name__ == "__main__":
    main()
