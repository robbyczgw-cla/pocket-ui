#!/usr/bin/env python3
"""Builds assets/data/console-credits.json, the console page's "Credits & links", from Wikidata (CC0) and the
consoles' English Wikipedia articles (facts only: names in infobox fields and in the article's lead).

    scripts/build-console-credits.py review          fetch what is not cached yet, print every candidate fact
    scripts/build-console-credits.py build           check scripts/console-credits.in.json against the cached
                                                     sources and write assets/data/console-credits.json
    scripts/build-console-credits.py build --refresh fetch everything again first

The facts are chosen by hand in scripts/console-credits.in.json (which people, which chips, which models), but
the script decides whether each one may ship: every fact names its source, and `build` checks it there.

    "wikidata:P880"       the console's Wikidata item has this statement pointing at the fact's `qid`
    "wikidata:^P155"      the other way round: the fact's item has this statement pointing at the console
    "wikidata:family"     the fact's item is in the console's family item (P361/P179 "Game Boy family")
    "infobox:<field>"     the infobox field (wikitext, markup removed) contains the fact's `match` (else its name)
    "lead"                a sentence of the article's first section (markup removed) has all of the fact's `words`,
                          short phrases; one of them has the name and what was done (`ROLE_WORDS`)
    chip makers           "wikidata:P176" / "wikidata:P178" on the chip's own item

A fact that fails its check is dropped with a warning and never written; `build --strict` fails instead. No prose
from Wikipedia is stored: the input keeps only the few short phrases that prove a lead fact, the output only names,
figures and our own short role text ("Led the design team"). The `summary` of each console is written by us.

Politeness as scripts/build-gamedb.py: one request at a time, a pause between requests, a User-Agent naming the
tool, Retry-After honoured. Answers are cached in ~/.cache/pocket-consoles (CONSOLES_CACHE).
"""
import argparse
import html
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
IN = ROOT / "scripts" / "console-credits.in.json"
OUT = ROOT / "assets" / "data" / "console-credits.json"
CONSOLES = ROOT / "assets" / "data" / "consoles.json"
CACHE = Path(os.environ.get("CONSOLES_CACHE", Path.home() / ".cache" / "pocket-consoles"))
SPARQL = "https://query.wikidata.org/sparql"
WIKI_API = "https://en.wikipedia.org/w/api.php"
USER_AGENT = "pocket-ui-consoles/0.1 (offline console facts for a handheld game launcher)"
PAUSE = 2.0

# The ROCKNIX system ids (es_systems.cfg of ROCKNIX 20261001, the image the virtual device unpacks). A link may only
# point at one of these; src/console_credits.rs keeps the same list for its test.
ROCKNIX = set("""3do 3ds amiga amigacd32 amstradcpc arcade arduboy atari2600 atari5200 atari7800 atari800 atarijaguar
atarilynx atarist atomiswave bbcmicro bk c128 c16 c64 cdi channelf chip-8 colecovision cps1 cps2 cps3 daphne doom
dreamcast easyrpg famicom fbn fds gameandwatch gamecube gamegear gb gba gbah gbav gbc gbch gbh genesis genh ggh gp32
heroic idtech imageviewer intellivision ios j2me macintosh mame mastersystem megacd megadrive-japan megadrive megadriveh
megaduck moonlight moto mplayer msx msx2 music n64 n64dd naomi nds ndsiware neocd neogeo nes nesh ngp ngpc odyssey2
openbor palm pc-8800 pc-9800 pc pcengine pcenginecd pcfx pet pico-8 pokemini ports ps2 ps3 psp pspminis psvita psx
satellaview saturn scummvm scv sega32x segacd segamodel3 sfc sg-1000 snes snesh snesmsu1 st-v steam sufami supergrafx
supervision tg16 tg16cd tic-80 tools triforce uzebox vectrex vic20 videopac vircon32 virtualboy wasm4 wii wiiu wiiware
windows wonderswan wonderswancolor x1 x68000 xbox zmachine zx81 zxspectrum""".split())

# Wikidata items that are a ROCKNIX system of their own: a family entry with one of these becomes a link. A model of
# a console (Game Boy Pocket, Genesis Nomad) plays that console's games and is not a system of its own, so it isn't
# here. Platform items as in scripts/build-gamedb.py, checked against Wikidata on 2026-10-23 (the last seven on 2026-10-27).
SYSTEM_OF = {
    "Q172742": "nes", "Q491640": "nes", "Q135321": "fds", "Q183259": "snes", "Q186437": "gb", "Q203992": "gbc",
    "Q188642": "gba", "Q184839": "n64", "Q170323": "nds", "Q164651": "virtualboy", "Q182172": "gamecube", "Q8079": "wii",
    "Q56942": "wiiu", "Q203597": "3ds", "Q215034": "gameandwatch", "Q10676": "megadrive", "Q209868": "mastersystem",
    "Q751719": "gamegear", "Q1047516": "segacd", "Q1063978": "sega32x", "Q200912": "saturn", "Q184198": "dreamcast",
    "Q1136956": "sg-1000", "Q10677": "psx", "Q10680": "ps2", "Q10683": "ps3", "Q170325": "psp", "Q188808": "psvita",
    "Q1057377": "pcengine", "Q202375": "supergrafx", "Q10854461": "pcenginecd", "Q1136902": "pcfx",
    "Q939881": "ngp", "Q1977455": "ngpc", "Q753657": "atarilynx", "Q206261": "atari2600", "Q743222": "atari5200",
    "Q753600": "atari7800", "Q1065792": "wonderswan", "Q1048035": "wonderswancolor", "Q253044": "n64dd",
    "Q1324477": "triforce", "Q1369174": "naomi", "Q757617": "atomiswave",
    "Q1054350": "neogeo", "Q2703883": "neocd", "Q650601": "atarijaguar", "Q229429": "3do", "Q1046862": "colecovision",
    "Q1061441": "intellivision", "Q767631": "vectrex",
}

# Statements read from each console's item.
CONSOLE_PROPS = ["P178", "P176", "P287", "P61", "P170", "P880", "P2560", "P7501", "P527", "P155", "P156", "P1365",
                 "P1366", "P361", "P179", "P144", "P4969"]
REVERSE_PROPS = ["P155", "P156", "P144", "P279", "P1365", "P1366"]

# What a "lead" fact's phrase with the name must also say.
ROLE_WORDS = re.compile(r"\b(design\w*|led|lead\w*|develop\w*|supervis\w*|advice|prototyp\w*|company|under|creat\w*|invent\w*|built|seek)\b", re.I)

last_request = 0.0


def get(url, data=None, accept="application/json"):
    """One polite request; waits as long as the server asks on 429/503."""
    global last_request
    for attempt in range(6):
        wait = PAUSE - (time.monotonic() - last_request)
        if wait > 0:
            time.sleep(wait)
        req = urllib.request.Request(url, data=data, headers={"User-Agent": USER_AGENT, "Accept": accept})
        last_request = time.monotonic()
        try:
            with urllib.request.urlopen(req, timeout=90) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            if e.code in (429, 502, 503, 504):
                retry = e.headers.get("Retry-After")
                delay = int(retry) if retry and retry.isdigit() else 30 * (attempt + 1)
                print(f"  server says {e.code}, waiting {delay} s", file=sys.stderr)
                time.sleep(delay)
                continue
            raise
        except (urllib.error.URLError, TimeoutError) as e:
            print(f"  {e}, trying again in {10 * (attempt + 1)} s", file=sys.stderr)
            time.sleep(10 * (attempt + 1))
    raise SystemExit("gave up after repeated errors")


def sparql(query):
    body = urllib.parse.urlencode({"query": query}).encode()
    return get(SPARQL, body, "application/sparql-results+json")["results"]["bindings"]


def cached(name, fetch, refresh):
    path = CACHE / name
    if path.exists() and not refresh:
        return json.loads(path.read_text())
    print(f"fetching {name}", file=sys.stderr)
    value = fetch()
    CACHE.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(value))
    tmp.replace(path)
    return value


def qid(uri):
    return uri.rsplit("/", 1)[-1]


# ---- the consoles and where their facts are --------------------------------------------------------------------

def consoles():
    """(id, QID, English Wikipedia title) for every console of consoles.json, from its sources."""
    out = []
    data = json.loads(CONSOLES.read_text())
    for c in data["systems"]:
        q = next((qid(s) for s in c["sources"] if "wikidata.org/wiki/Q" in s), None)
        title = next((urllib.parse.unquote(s.split("/wiki/", 1)[1]) for s in c["sources"]
                      if "en.wikipedia.org/wiki/" in s and " " not in s), None)
        out.append((c["id"], q, title, c["name"]))
    return out


def fetch_all(ids, refresh):
    items = " ".join(f"wd:{q}" for q in ids.values())
    claims = cached("claims.json", lambda: sparql("""SELECT ?item ?p ?v ?vl WHERE {
  VALUES ?item { %s } VALUES ?p { %s }
  ?item ?p ?v . OPTIONAL { ?v rdfs:label ?vl FILTER(LANG(?vl) IN ("en", "mul")) }
}""" % (items, " ".join(f"wdt:{p}" for p in CONSOLE_PROPS))), refresh)
    reverse = cached("reverse.json", lambda: sparql("""SELECT ?item ?p ?x ?xl WHERE {
  VALUES ?item { %s } VALUES ?p { %s }
  ?x ?p ?item . OPTIONAL { ?x rdfs:label ?xl FILTER(LANG(?xl) IN ("en", "mul")) }
}""" % (items, " ".join(f"wdt:{p}" for p in REVERSE_PROPS))), refresh)
    labels = cached("labels.json", lambda: sparql("""SELECT ?item ?l WHERE {
  VALUES ?item { %s } ?item rdfs:label ?l FILTER(LANG(?l) IN ("en", "mul"))
}""" % items), refresh)
    families = {qid(r["v"]["value"]) for r in claims if qid(r["p"]["value"]) in ("P361", "P179")}
    members = cached("families.json", lambda: sparql("""SELECT ?family ?x ?xl WHERE {
  VALUES ?family { %s } { ?x wdt:P361 ?family } UNION { ?x wdt:P179 ?family }
  OPTIONAL { ?x rdfs:label ?xl FILTER(LANG(?xl) IN ("en", "mul")) }
}""" % " ".join(f"wd:{f}" for f in sorted(families))), refresh) if families else []
    chips = {qid(r["v"]["value"]) for r in claims if qid(r["p"]["value"]) in ("P880", "P2560", "P7501", "P527")}
    chip_claims = cached("chips.json", lambda: sparql("""SELECT ?chip ?p ?v ?vl WHERE {
  VALUES ?chip { %s } VALUES ?p { wdt:P176 wdt:P178 }
  ?chip ?p ?v . OPTIONAL { ?v rdfs:label ?vl FILTER(LANG(?vl) IN ("en", "mul")) }
}""" % " ".join(f"wd:{c}" for c in sorted(chips))), refresh) if chips else []
    return claims, reverse, labels, members, chip_claims


def fetch_article(title, refresh):
    def go():
        url = WIKI_API + "?" + urllib.parse.urlencode({"action": "parse", "page": title, "prop": "wikitext", "section": 0,
                                                       "format": "json", "redirects": 1})
        return get(url)["parse"]["wikitext"]["*"]
    return cached("wiki-" + re.sub(r"[^A-Za-z0-9_-]", "_", title) + ".json", go, refresh)


# ---- wikitext to plain text (for checking names only) ------------------------------------------------------------

def strip_refs(w):
    w = re.sub(r"<!--.*?-->", "", w, flags=re.S)
    w = re.sub(r"<ref[^>]*/>", "", w)
    return re.sub(r"<ref[^>]*>.*?</ref>", "", w, flags=re.S)


def infobox_fields(w):
    """The top-level `| name = value` fields of the first {{Infobox ...}}."""
    w = strip_refs(w)
    start = w.find("{{Infobox")
    if start < 0:
        return {}
    depth, i, parts, cur = 0, start, [], []
    while i < len(w):
        two = w[i:i + 2]
        if two in ("{{", "[["):
            depth += 1
            cur.append(two)
            i += 2
            continue
        if two in ("}}", "]]"):
            depth -= 1
            if depth == 0:
                parts.append("".join(cur))
                break
            cur.append(two)
            i += 2
            continue
        if w[i] == "|" and depth == 1:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(w[i])
        i += 1
    fields = {}
    for p in parts[1:]:
        if "=" in p:
            k, v = p.split("=", 1)
            fields[k.strip().lower()] = v.strip()
    return fields


def plain(w):
    """Wikitext as plain text, good enough to find names in: links as their text, list templates as "; "."""
    w = strip_refs(w)
    w = re.sub(r"\{\{(?:efn|refn|sfn|citation needed|cn)[^{}]*\}\}", "", w, flags=re.I)
    for _ in range(6):
        w = re.sub(r"\{\{\s*(?:nbsp|snd|spaced ndash)\s*\}\}", " ", w, flags=re.I)
        # {{nowrap|x}}, {{nobold|x}}, {{ubl|a|b}}, {{plainlist|* a}}, {{Unbulleted list|a|b}}: their contents.
        w = re.sub(r"\{\{\s*(?:nowrap|nobold|small|nihongo foot|nihongo)\s*\|([^{}]*)\}\}", lambda m: m.group(1).split("|")[0], w, flags=re.I)
        w = re.sub(r"\{\{\s*(?:ubl|ubli|unbulleted list|plainlist|plain list|flatlist|indented plainlist|collapsible list)\s*\|([^{}]*)\}\}",
                   lambda m: "; ".join(x.strip(" *") for x in m.group(1).split("|") if x.strip(" *") and "=" not in x), w, flags=re.I)
        w = re.sub(r"\[\[(?:[^|\]]*\|)?([^\]]*)\]\]", r"\1", w)
    w = re.sub(r"\{\{[^{}]*\}\}", "", w)
    w = re.sub(r"<br\s*/?>", "; ", w)
    w = re.sub(r"<[^>]+>", "", w)
    w = w.replace("'''", "").replace("''", "")
    w = html.unescape(w).replace("\xa0", " ")
    w = re.sub(r"\n\s*\*\s*", "; ", w)
    return re.sub(r"\s+", " ", w).strip()


# ---- review: everything a fact could be chosen from -------------------------------------------------------------

def index(claims, reverse, labels, members, chip_claims):
    by = {}
    for r in claims:
        it = qid(r["item"]["value"])
        v = r["v"]["value"]
        if not v.startswith("http://www.wikidata.org/entity/Q"):
            continue
        by.setdefault(it, {}).setdefault(qid(r["p"]["value"]), {})[qid(v)] = r.get("vl", {}).get("value", qid(v))
    rev = {}
    for r in reverse:
        it = qid(r["item"]["value"])
        rev.setdefault(it, {}).setdefault(qid(r["p"]["value"]), {})[qid(r["x"]["value"])] = r.get("xl", {}).get("value", "")
    fam = {}
    for r in members:
        fam.setdefault(qid(r["family"]["value"]), {})[qid(r["x"]["value"])] = r.get("xl", {}).get("value", "")
    chip = {}
    for r in chip_claims:
        chip.setdefault(qid(r["chip"]["value"]), {}).setdefault(qid(r["p"]["value"]), {})[qid(r["v"]["value"])] = r.get("vl", {}).get("value", "")
    label = {}
    for r in labels:
        it = qid(r["item"]["value"])
        if r["l"].get("xml:lang") == "en" or it not in label:
            label[it] = r["l"]["value"]
    return by, rev, fam, chip, label


def load(refresh):
    cs = consoles()
    missing = [c[0] for c in cs if not c[1] or not c[2]]
    extra = json.loads(IN.read_text()) if IN.exists() else {}
    # consoles.json has no Wikidata or Wikipedia address for a few: the input file names them.
    fixed = []
    for cid, q, title, name in cs:
        e = extra.get("ids", {}).get(cid, {})
        fixed.append((cid, e.get("qid", q), e.get("enwiki", title), name))
    missing = [c[0] for c in fixed if not c[1] or not c[2]]
    if missing:
        raise SystemExit(f"no Wikidata item or Wikipedia article for: {', '.join(missing)} (set 'ids' in {IN.name})")
    ids = {c[0]: c[1] for c in fixed}
    wd = index(*fetch_all(ids, refresh))
    articles = {c[0]: fetch_article(c[2], refresh) for c in fixed}
    return fixed, wd, articles, extra


def review(refresh):
    fixed, (by, rev, fam, chip, label), articles, _ = load(refresh)
    for cid, q, title, name in fixed:
        print(f"\n== {cid}  {name}  {q}  {title}")
        for p, vals in sorted(by.get(q, {}).items()):
            print(f"  wikidata:{p:6} " + "; ".join(f"{l} ({v})" for v, l in vals.items()))
        for p, vals in sorted(rev.get(q, {}).items()):
            print(f"  wikidata:^{p:5} " + "; ".join(f"{l} ({v})" for v, l in vals.items()))
        for f in sorted(set(by.get(q, {}).get("P361", {})) | set(by.get(q, {}).get("P179", {}))):
            if f in fam:
                print(f"  family {f}: " + "; ".join(f"{l} ({v})" for v, l in fam[f].items()))
        for c in set(by.get(q, {}).get("P880", {})) | set(by.get(q, {}).get("P2560", {})) | set(by.get(q, {}).get("P7501", {})):
            for p, vals in chip.get(c, {}).items():
                print(f"  chip {c} {p}: " + "; ".join(vals.values()))
        box = infobox_fields(articles[cid])
        for k in ["developer", "manufacturer", "designer", "cpu", "gpu", "graphics", "sound", "predecessor", "successor",
                  "related", "family", "top_game", "compatibility"]:
            if k in box:
                print(f"  infobox:{k:12} {plain(box[k])[:240]}")


# ---- build --------------------------------------------------------------------------------------------------------

def build(refresh, strict):
    fixed, (by, rev, fam, chip, label), articles, extra = load(refresh)
    chosen = extra.get("consoles", {})
    problems = []
    out_systems = []

    def check(cid, q, fact, what):
        """None when `fact` is where its source says, else why not."""
        src = fact.get("source", "")
        texts = fact.get("match") or [t for t in (fact.get("name") or fact.get("title"), fact.get("figure")) if t]
        texts = [texts] if isinstance(texts, str) else texts
        if src.startswith("wikidata:"):
            p = src.split(":", 1)[1]
            fq = fact.get("qid")
            if not fq or fq == q:
                return f"{what}: {src} needs the qid of another item"
            if p == "family":
                # The console's family (P179 "part of the series", or a P361 group called a family), not its
                # generation, which is a P361 group too.
                mine = by.get(q, {})
                fams = set(mine.get("P179", {})) | {f for f, l in mine.get("P361", {}).items() if "family" in l.lower()}
                ok = any(fq in fam.get(f, {}) for f in fams)
            elif p.startswith("^"):
                ok = fq in rev.get(q, {}).get(p[1:], {})
            else:
                ok = fq in by.get(q, {}).get(p, {})
            if not ok:
                return f"{what}: {fq} not under {src}"
            return None
        if src.startswith("infobox:"):
            field = src.split(":", 1)[1]
            box = infobox_fields(articles[cid])
            if field not in box:
                return f"{what}: no infobox field {field}"
            for text in texts:
                if text.lower() not in plain(box[field]).lower():
                    return f"{what}: {text!r} not in infobox {field}: {plain(box[field])[:160]!r}"
            return None
        if src == "lead":
            # Groups of short phrases: each group within one sentence of the first section, and the name among the
            # phrases. Only these phrases are kept (in the input file), never the sentence.
            groups = fact.get("words") or []
            groups = [groups] if groups and isinstance(groups[0], str) else groups
            sentences = re.split(r"(?<=[.!?])\s+", plain(articles[cid]))
            for group in groups:
                if not any(all(w in sentence for w in group) for sentence in sentences):
                    return f"{what}: no sentence of the lead has all of {group!r}"
            # The name must stand in a phrase with what the person or company did ("team led by Hideki Sato"), not
            # alone: a sentence can name someone else beside the fact ("... after president Hiroshi Yamauchi ...").
            name = fact.get("name", "")
            if not groups or not any(name in w and ROLE_WORDS.search(w.replace(name, "")) for group in groups for w in group):
                return f"{what}: no phrase has {name!r} with what they did ({ROLE_WORDS.pattern})"
            return None
        return f"{what}: unknown source {src!r}"

    def keep(cid, q, facts, what):
        kept = []
        for i, f in enumerate(facts):
            why = check(cid, q, f, f"{cid} {what}[{i}] {f.get('name') or f.get('title')}")
            if why:
                problems.append(why)
                continue
            kept.append(f)
        return kept

    for cid, q, title, name in fixed:
        c = chosen.get(cid)
        if not c:
            problems.append(f"{cid}: not in {IN.name}")
            continue
        entry = {"id": cid, "qid": q}
        if c.get("summary"):
            entry["summary"] = c["summary"]
        people = []
        for f in keep(cid, q, c.get("people", []), "people"):
            people.append({k: f[k] for k in ("name", "role", "source") if k in f})
        makers = []
        for f in keep(cid, q, c.get("makers", []), "makers"):
            m = {k: f[k] for k in ("name", "role", "source") if k in f}
            if f.get("qid"):
                m["label"] = label_of(f["qid"], by, q, f, label)
            makers.append(m)
        chips = []
        for f in keep(cid, q, c.get("chips", []), "chips"):
            ch = {k: f[k] for k in ("role", "name", "source") if k in f}
            if f.get("qid"):
                ch["qid"] = f["qid"]
            # One maker or several ("Sony Computer Entertainment and Toshiba"), all under the same property of the
            # chip's item: P176 manufacturer is "made by", P178 developer "designed by".
            maker = f.get("maker")
            makers_of = [maker] if isinstance(maker, dict) else (maker or [])
            props = {m.get("source", "").removeprefix("wikidata:") for m in makers_of}
            ok = [m for m in makers_of if f.get("qid") and m.get("qid") in chip.get(f["qid"], {}).get(m.get("source", "").removeprefix("wikidata:"), {})]
            if makers_of and len(ok) == len(makers_of) and len(props) == 1:
                ch["maker"] = " and ".join(m["name"] for m in makers_of)
                ch["maker_by"] = "made" if props == {"P176"} else "designed"
            elif makers_of:
                problems.append(f"{cid} chip {f['name']}: makers {[m.get('name') for m in makers_of]} not all under {f.get('qid')} {props}")
            chips.append(ch)
        family = []
        for f in keep(cid, q, c.get("family", []), "family"):
            fe = {k: f[k] for k in ("relation", "name", "source") if k in f}
            fq = f.get("qid")
            if fq:
                fe["qid"] = fq
            system = f.get("system") or (SYSTEM_OF.get(fq) if fq else None)
            if system and system != cid:
                if system not in ROCKNIX:
                    problems.append(f"{cid} family {f['name']}: {system} is no ROCKNIX system")
                else:
                    fe["system"] = system
            family.append(fe)
        best = []
        for f in keep(cid, q, c.get("best_selling", []), "best_selling"):
            best.append({k: f[k] for k in ("title", "figure", "source") if k in f})
        makers_names = {m.get("label") or m["name"] for m in makers} | {m["name"] for m in makers}
        game_makers = []
        for g in c.get("game_makers", []):
            if g in makers_names:
                game_makers.append(g)
            else:
                problems.append(f"{cid}: game maker {g!r} is not one of its verified makers {sorted(makers_names)}")
        for k, v in (("people", people), ("makers", makers), ("chips", chips), ("family", family),
                     ("best_selling", best), ("game_makers", game_makers)):
            if v:
                entry[k] = v
        entry["sources"] = {
            "wikidata": f"https://www.wikidata.org/wiki/{q}",
            "wikipedia": "https://en.wikipedia.org/wiki/" + urllib.parse.quote(title.replace(" ", "_"), safe="()_-,.'!"),
        }
        out_systems.append(entry)
    for p in problems:
        print("dropped: " + p, file=sys.stderr)
    if problems and strict:
        raise SystemExit(f"{len(problems)} facts could not be checked")
    data = {
        "version": 1,
        "generated": time.strftime("%Y-%m-%d"),
        "license": "Facts from Wikidata (CC0) and the consoles' English Wikipedia articles (names only, CC BY-SA); "
                   "summaries and role texts written for pocket-ui",
        "schema": "Built by scripts/build-console-credits.py from scripts/console-credits.in.json; every fact was checked "
                  "against its source (wikidata:<P>, infobox:<field>, lead). Don't edit by hand.",
        "systems": out_systems,
    }
    tmp = OUT.with_suffix(".tmp")
    tmp.write_text(json.dumps(data, ensure_ascii=False, indent=1) + "\n")
    tmp.replace(OUT)
    n = sum(len(s.get(k, [])) for s in out_systems for k in ("people", "makers", "chips", "family", "best_selling"))
    print(f"{OUT}: {len(out_systems)} consoles, {n} facts, {len(problems)} dropped")


def label_of(fq, by, q, fact, label):
    """The Wikidata English label of a maker (what gamedb's developer field holds), else the fact's name."""
    for p in ("P178", "P176", "P287", "P170"):
        v = by.get(q, {}).get(p, {}).get(fq)
        if v:
            return v
    return label.get(fq, fact.get("name"))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("what", choices=["review", "build"])
    ap.add_argument("--refresh", action="store_true", help="fetch everything again")
    ap.add_argument("--strict", action="store_true", help="fail when a fact can't be checked")
    a = ap.parse_args()
    if a.what == "review":
        review(a.refresh)
    else:
        build(a.refresh, a.strict)


if __name__ == "__main__":
    main()
