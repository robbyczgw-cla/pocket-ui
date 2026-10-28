#!/usr/bin/env python3
"""Builds assets/gamedb.bin, the offline game facts behind the game card, from Wikidata (CC0).

    scripts/build-gamedb.py                 query what is not cached yet, then write assets/gamedb.bin
    scripts/build-gamedb.py --refresh       query everything again
    scripts/build-gamedb.py --only gb,nes   only these systems (for trying the queries)

For every system pocket-ui knows, the Wikidata SPARQL endpoint is asked for video games whose platform
(P400) is that console: English label and aliases (or the language-neutral "mul" ones, which Wikidata
uses for names that are the same everywhere, such as "Super Mario World"), release year (P577),
developer (P178), publisher (P123), genre (P136), series (P179), game mode (P404), number of players
(P1872/P1873), review scores (P444 with "review by", P447), the English Wikipedia article's title (only
its *name*, to fetch the lead text on the device on demand: that text is CC BY-SA and is never bundled),
the number of sitelinks (how well known a game is, to choose between same-named entries) and the Redump
ID (P5290), which links a disc to its checksums.

For Curator (docs/CURATOR.md, layer A) also: game mechanics (P4151), input methods (P479), language of
work (P407, as ISO 639-1 codes where Wikidata has one), follows / followed by (P155/P156), and the links
between versions of a game: edition of (P629) / has edition (P747), based on (P144) / derivative work
(P4969), plus the item's own classes (P31) to recognise remakes, remasters, reboots and ROM hacks.
From these every game gets a **work**: the QID of the work it is a version of, so the same game on
several systems (or filed as an edition item for one system) groups together. See `resolve_works`.

Politeness (https://www.mediawiki.org/wiki/Wikimedia_APIs/Rate_limits): one request at a time, a pause
between requests, User-Agent `pocket-ui-gamedb/0.1`, and a 429/503 makes the script wait as long as the
server asks (Retry-After) before trying again. A query the endpoint keeps cutting off is asked again in
batches of games. Answers are cached in ~/.cache/pocket-gamedb, so a second run only writes the file.

The file (read by src/gamedb.rs):

    PGDB1\\n
    <system> <offset> <length>\\n      one line per system; offsets count from the end of the header
    \\n
    <zlib stream per system>          tab-separated lines, see `FIELDS`

Fields with several values separate them with `|`; a review score is `<reviewer>=<score>`.
"""
import argparse
import http.client
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zlib
from pathlib import Path

ENDPOINT = "https://query.wikidata.org/sparql"
USER_AGENT = "pocket-ui-gamedb/0.1"
PAUSE = 2.0  # seconds between requests
CACHE = Path(os.environ.get("GAMEDB_CACHE", Path.home() / ".cache" / "pocket-gamedb"))
OUT = Path(__file__).resolve().parent.parent / "assets" / "gamedb.bin"

# pocket-ui system id -> Wikidata items used as platform (P400). Checked against Wikidata on 2026-10-22.
SYSTEMS = {
    "nes": ["Q172742"],  # Nintendo Entertainment System (the Famicom is the same item)
    "fds": ["Q135321"],  # Famicom Disk System
    "snes": ["Q183259"],  # Super Nintendo Entertainment System
    "gb": ["Q186437"],  # Game Boy
    "gbc": ["Q203992"],  # Game Boy Color
    "gba": ["Q188642"],  # Game Boy Advance
    "n64": ["Q184839"],  # Nintendo 64
    "nds": ["Q170323"],  # Nintendo DS
    "virtualboy": ["Q164651"],
    "gamecube": ["Q182172"],
    "wii": ["Q8079"],
    "3ds": ["Q203597"],
    "megadrive": ["Q10676"],  # Sega Genesis/Mega Drive
    "mastersystem": ["Q209868"],
    "gamegear": ["Q751719"],
    "segacd": ["Q1047516"],
    "sega32x": ["Q1063978"],
    "saturn": ["Q200912"],
    "dreamcast": ["Q184198"],
    "sg-1000": ["Q1136956"],
    "psx": ["Q10677"],
    "ps2": ["Q10680"],
    "psp": ["Q170325"],
    "pcengine": ["Q1057377", "Q202375"],  # TurboGrafx-16 / PC Engine, SuperGrafx
    "pcenginecd": ["Q10854461"],  # TurboGrafx-CD
    "ngp": ["Q939881"],
    "ngpc": ["Q1977455"],
    "atarilynx": ["Q753657"],
    "atari2600": ["Q206261"],
    "atari7800": ["Q753600"],
    "wonderswan": ["Q1065792"],
    "wonderswancolor": ["Q1048035"],
    # Added 2026-10-23 with the systems pocket-core took on (docs/HOST.md "More systems"). Wikidata files few arcade
    # games under their board (most only under "arcade game"), so these two stay small.
    "naomi": ["Q1369174", "Q843916"],  # Sega NAOMI, NAOMI 2
    "atomiswave": ["Q757617"],
    "pokemini": ["Q1759168"],
}

FIELDS = ["qid", "label", "year", "developers", "publishers", "genres", "series", "modes", "players", "scores", "aliases", "enwiki", "sitelinks", "redump",
          # Curator layer A (added 2026-10-23; older readers ignore extra columns):
          "work", "mechanics", "inputs", "languages", "related", "follows", "followed"]

# Video games, and anything Wikidata files as a kind of video game (remakes, ports, compilations ...).
GAME = "?item wdt:P400 ?platform ; wdt:P31/wdt:P279* wd:Q7889 . VALUES ?platform { %s }"

QUERIES = {
    "items": """SELECT ?item ?label ?date ?article ?links WHERE {
  %s
  ?item rdfs:label ?label FILTER(LANG(?label) IN ("en", "mul"))
  OPTIONAL { ?item wdt:P577 ?date }
  OPTIONAL { ?article schema:about ?item ; schema:isPartOf <https://en.wikipedia.org/> }
  OPTIONAL { ?item wikibase:sitelinks ?links }
}""",
    "relations": """SELECT ?item ?p ?v ?vl WHERE {
  %s
  VALUES ?p { wdt:P178 wdt:P123 wdt:P136 wdt:P179 wdt:P404 }
  ?item ?p ?v . ?v rdfs:label ?vl FILTER(LANG(?vl) IN ("en", "mul"))
}""",
    "scores": """SELECT ?item ?score ?by ?byl ?plat WHERE {
  %s
  ?item p:P444 ?st . ?st ps:P444 ?score .
  OPTIONAL { ?st pq:P447 ?by . ?by rdfs:label ?byl FILTER(LANG(?byl) IN ("en", "mul")) }
  OPTIONAL { ?st pq:P400 ?plat }
}""",
    "aliases": """SELECT ?item ?alias WHERE {
  %s
  ?item skos:altLabel ?alias FILTER(LANG(?alias) IN ("en", "mul"))
}""",
    "ids": """SELECT ?item ?redump WHERE {
  %s
  ?item wdt:P5290 ?redump .
}""",
    "players": """SELECT ?item ?min ?max WHERE {
  %s
  { ?item wdt:P1872 ?min } UNION { ?item wdt:P1873 ?max }
}""",
    # Curator layer A. ?vl is the value's name (mechanics, inputs, classes, titles of linked games), ?code
    # a language's ISO 639-1 code.
    "links": """SELECT ?item ?p ?v ?vl ?code WHERE {
  %s
  VALUES ?p { wdt:P4151 wdt:P479 wdt:P407 wdt:P155 wdt:P156 wdt:P629 wdt:P747 wdt:P144 wdt:P4969 wdt:P31 }
  ?item ?p ?v .
  OPTIONAL { ?v rdfs:label ?vl FILTER(LANG(?vl) IN ("en", "mul")) }
  OPTIONAL { ?v wdt:P218 ?code }
}""",
}

PROPS = {"P178": "developers", "P123": "publishers", "P136": "genres", "P179": "series", "P404": "modes"}

last_request = 0.0


class CutOff(Exception):
    """The endpoint keeps running out of time on this query."""


def sparql(query, cutoff_tries=8):
    """One polite request; waits as long as the server asks on 429/503 and tries again."""
    global last_request
    body = urllib.parse.urlencode({"query": query}).encode()
    for attempt in range(8):
        wait = PAUSE - (time.monotonic() - last_request)
        if wait > 0:
            time.sleep(wait)
        req = urllib.request.Request(ENDPOINT, data=body, headers={
            "User-Agent": USER_AGENT,
            "Accept": "application/sparql-results+json",
            "Content-Type": "application/x-www-form-urlencoded",
        })
        last_request = time.monotonic()
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                return json.load(r)["results"]["bindings"]
        except urllib.error.HTTPError as e:
            if e.code in (429, 503, 502, 504):
                retry = e.headers.get("Retry-After")
                delay = int(retry) if retry and retry.isdigit() else 30 * (attempt + 1)
                print(f"  server says {e.code}, waiting {delay} s", file=sys.stderr)
                time.sleep(delay)
                continue
            raise
        except (json.JSONDecodeError, http.client.IncompleteRead):
            # The endpoint ran out of time while streaming and cut the answer off.
            if attempt + 1 >= cutoff_tries:
                raise CutOff()
            print(f"  answer cut off, trying again in {20 * (attempt + 1)} s", file=sys.stderr)
            time.sleep(20 * (attempt + 1))
        except (urllib.error.URLError, TimeoutError) as e:
            print(f"  {e}, trying again in {10 * (attempt + 1)} s", file=sys.stderr)
            time.sleep(10 * (attempt + 1))
    raise SystemExit("gave up after repeated errors")


BATCH = 300


def cached(system, name, query, refresh, items=None):
    """`query` holds one `%s` for the games: the platform clause, or, when the endpoint keeps cutting the
    answer off, `VALUES ?item { ... }` for batches of the games the items query found."""
    path = CACHE / f"{system}-{name}.json"
    if path.exists() and not refresh:
        return json.loads(path.read_text())
    print(f"{system}: {name}", file=sys.stderr)
    try:
        rows = sparql(query[0] % query[1], cutoff_tries=2 if items else 8)
    except CutOff:
        print(f"  {system}: {name} in batches of {BATCH}", file=sys.stderr)
        rows = []
        for i in range(0, len(items), BATCH):
            values = "VALUES ?item { %s }" % " ".join(f"wd:{q}" for q in items[i:i + BATCH])
            rows += sparql(query[0] % values)
    CACHE.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(rows))
    tmp.replace(path)
    return rows


def qid(uri):
    return uri.rsplit("/", 1)[-1]


def clean(s):
    """No tabs, line breaks or the list separator inside a field."""
    return re.sub(r"[\t\r\n|=]+", " ", s).strip()


def year(date):
    m = re.match(r"^\+?(\d{4})-", date)
    return int(m.group(1)) if m else None


def build_system(system, platforms, refresh):
    values = " ".join(f"wd:{p}" for p in platforms)
    game = GAME % values
    games = {}
    for row in cached(system, "items", (QUERIES["items"], game), refresh):
        q = qid(row["item"]["value"])
        # Names that are the same in every language are "mul" labels; an English one wins.
        label, lang = clean(row["label"]["value"]), row["label"].get("xml:lang")
        g = games.setdefault(q, {"qid": q, "label": label, "lang": lang, "year": None, "developers": [], "publishers": [],
                                 "genres": [], "series": [], "modes": [], "scores": [], "aliases": [], "enwiki": "", "sitelinks": 0, "redump": [], "players": "", "pmin": None, "pmax": None})
        if lang == "en" and g["lang"] != "en":
            g["label"], g["lang"] = label, lang
        if "date" in row:
            y = year(row["date"]["value"])
            if y and (g["year"] is None or y < g["year"]):
                g["year"] = y
        if "article" in row:
            g["enwiki"] = clean(urllib.parse.unquote(row["article"]["value"].rsplit("/wiki/", 1)[-1]).replace("_", " "))
        if "links" in row:
            g["sitelinks"] = int(row["links"]["value"])
    names = {}  # value item -> its name, English before "mul"
    order = []  # (game, field, value item) in the order Wikidata lists them
    for row in cached(system, "relations", (QUERIES["relations"], game), refresh, list(games)):
        g = games.get(qid(row["item"]["value"]))
        field = PROPS.get(qid(row["p"]["value"]))
        if not g or not field:
            continue
        v = qid(row["v"]["value"])
        name, lang = clean(row["vl"]["value"]), row["vl"].get("xml:lang")
        if name and (v not in names or lang == "en"):
            names[v] = name
        order.append((g, field, v))
    for g, field, v in order:
        name = names.get(v)
        if name and name not in g[field]:
            g[field].append(name)
    reviewers = {}  # reviewer item -> its name, English before "mul"
    reviews = []  # (game, reviewer item, score)
    for row in cached(system, "scores", (QUERIES["scores"], game), refresh, list(games)):
        g = games.get(qid(row["item"]["value"]))
        # A score given for another platform is not this game's; a score without a reviewer says little.
        if not g or "by" not in row or "byl" not in row or ("plat" in row and qid(row["plat"]["value"]) not in platforms):
            continue
        by = qid(row["by"]["value"])
        name, lang = clean(row["byl"]["value"]), row["byl"].get("xml:lang")
        if name and (by not in reviewers or lang == "en"):
            reviewers[by] = name
        reviews.append((g, by, clean(row["score"]["value"])))
    seen = set()
    for g, by, score in reviews:
        # One score per reviewer and game (the first Wikidata lists), under one name.
        if score and (g["qid"], by) not in seen:
            seen.add((g["qid"], by))
            g["scores"].append(f"{reviewers[by]}={score}")
    for row in cached(system, "aliases", (QUERIES["aliases"], game), refresh, list(games)):
        g = games.get(qid(row["item"]["value"]))
        a = clean(row["alias"]["value"])
        if g and a and a != g["label"] and a not in g["aliases"]:
            g["aliases"].append(a)
    for row in cached(system, "ids", (QUERIES["ids"], game), refresh, list(games)):
        g = games.get(qid(row["item"]["value"]))
        if g:
            g["redump"].append(clean(row["redump"]["value"]))
    for row in cached(system, "players", (QUERIES["players"], game), refresh, list(games)):
        g = games.get(qid(row["item"]["value"]))
        if not g:
            continue
        for key in ("min", "max"):
            if key in row:
                try:
                    n = int(float(row[key]["value"]))
                except ValueError:
                    continue
                g["p" + key] = n if g["p" + key] is None else (min if key == "min" else max)(g["p" + key], n)
    for g in games.values():
        lo, hi = g["pmin"], g["pmax"]
        if lo is not None or hi is not None:
            lo, hi = lo or 1, hi or lo
            g["players"] = str(lo) if lo == hi else f"{lo}-{hi}"
    for g in games.values():
        g.update({"work": "", "mechanics": [], "inputs": [], "languages": [], "related": [], "follows": [], "followed": [],
                  "classes": [], "links": []})
    for row in cached(system, "links", (QUERIES["links"], game), refresh, list(games)):
        g = games.get(qid(row["item"]["value"]))
        if not g:
            continue
        p, v = qid(row["p"]["value"]), row["v"]["value"]
        if not v.startswith("http://www.wikidata.org/entity/Q"):
            continue
        v = qid(v)
        name = clean(row["vl"]["value"]) if "vl" in row else ""
        field = {"P4151": "mechanics", "P479": "inputs", "P155": "follows", "P156": "followed", "P31": "classes"}.get(p)
        if p == "P407":
            # Languages as ISO 639-1 codes; one without a code (a dialect, "Simplified Chinese") by name.
            lang = clean(row["code"]["value"]).lower() if "code" in row else name
            if lang and lang not in g["languages"]:
                g["languages"].append(lang)
        elif field in ("follows", "followed"):
            if v not in g[field]:
                g[field].append(v)
        elif field:
            if name and name not in g[field]:
                g[field].append(name)
        else:
            # Links between versions, sorted out across all systems in resolve_works.
            if (p, v, name) not in g["links"]:
                g["links"].append((p, v, name))
    return games


def game_lines(games):
    lines = []
    for g in sorted(games.values(), key=lambda g: (g["label"].lower(), g["qid"])):
        cells = []
        for f in FIELDS:
            v = g[f]
            if isinstance(v, list):
                v = "|".join(sorted(v) if f in ("genres", "modes", "mechanics", "inputs", "languages") else v)
            cells.append("" if v is None else str(v))
        lines.append("\t".join(cells))
    return "\n".join(lines) + "\n"


# A class (P31) or a title word that makes an item a new work built on an older one, not a version of it.
REMAKE_CLASSES = ("remake", "remaster", "reboot")
FAN_CLASSES = ("rom hack", "fan game", "fangame", "homebrew", "mod")
REMAKE_WORDS = {"3d", "hd", "remastered", "remaster", "remake", "reloaded", "redux", "reborn", "reimagined", "remix", "returns",
                "anniversary", "definitive", "ultimate", "director's", "directors", "cut"}
# Words a port adds to the title of the game it ports ("Final Fantasy VI Advance", "Link's Awakening DX").
PORT_WORDS = {"advance", "ds", "dx", "portable", "gba", "pocket", "color", "colour", "plus", "deluxe", "edition", "version", "for",
              "the", "game", "boy", "psp", "playstation", "sega", "cd", "mega", "genesis", "snes", "super", "famicom", "nes"}


def title_words(title):
    """Lowercase words of a title without punctuation or (…) groups; Roman numbers as digits."""
    t = re.sub(r"\([^)]*\)", " ", title.lower()).replace("&", " and ")
    words = [w for w in re.split(r"[^\w']+", t) if w]
    roman = {"ii": "2", "iii": "3", "iv": "4", "v": "5", "vi": "6", "vii": "7", "viii": "8", "ix": "9", "x": "10"}
    return [roman.get(w, w) for w in words]


def version_kind(g, target_title):
    """How the game `g`, linked by P144/P4969 to an item called `target_title`, relates to it: "port" (the
    same work: same title, or the title plus words a port adds), "remake" (its class or a title word says
    so) or "based" (a new work: sequel-like spin-off, fan game, adaptation)."""
    classes = " ".join(g["classes"]).lower()
    if any(c in classes for c in FAN_CLASSES):
        return "based"
    if any(c in classes for c in REMAKE_CLASSES):
        return "remake"
    mine, theirs = title_words(g["label"]), title_words(target_title)
    if not theirs:
        return "based"
    if mine == theirs:
        return "port"
    if mine[:len(theirs)] == theirs:
        extra = set(mine[len(theirs):])
        if extra & REMAKE_WORDS:
            return "remake"
        if extra <= PORT_WORDS:
            return "port"
    return "based"


def games_among(qids, refresh):
    """Which of `qids` (items linked from our games but on none of our systems) are video games. A game
    "based on" a film of the same name is an adaptation, not a port of it. Cached like the other answers."""
    path = CACHE / "link-targets.json"
    known = {} if refresh or not path.exists() else json.loads(path.read_text())
    todo = sorted(q for q in qids if q not in known)
    for i in range(0, len(todo), BATCH):
        chunk = todo[i:i + BATCH]
        print(f"link targets: {i + len(chunk)} of {len(todo)}", file=sys.stderr)
        rows = sparql("SELECT DISTINCT ?v WHERE { VALUES ?v { %s } ?v wdt:P31/wdt:P279* wd:Q7889 . }" % " ".join(f"wd:{q}" for q in chunk))
        games = {qid(r["v"]["value"]) for r in rows}
        known.update({q: q in games for q in chunk})
        CACHE.mkdir(parents=True, exist_ok=True)
        tmp = path.with_suffix(".tmp")
        tmp.write_text(json.dumps(known))
        tmp.replace(path)
    return {q for q in qids if known.get(q)}


def resolve_works(systems, refresh=False):
    """Gives every game its `work` (the QID of the work it is a version of) and `related` links.

    - The same QID on several systems is one work.
    - X edition of (P629) Y, or Y has edition (P747) X: X is a version of Y.
    - X based on (P144) Y, or Y derivative work (P4969) X: a version of Y when X is a port (`version_kind`);
      a remake/remaster keeps its own work and is linked as `remake-of:Y` (Y gets `remade-as:X`); anything
      else (a fan game such as "Chrono Trigger: Crimson Echoes", an adaptation) is `based-on:Y` / `source-of:X`.
    - Chains are followed to their end (an edition of a port of Y belongs to Y); a loop stops at the item
      with the most sitelinks.
    The work's QID may be an item that is on none of our systems (a PC original); it is only a key."""
    items = {}  # qid -> one game dict (the same item on several systems shares its facts)
    for games in systems.values():
        for q, g in games.items():
            items.setdefault(q, g)
    parent, related = {}, {}
    outside = {v for g in items.values() for p, v, _ in g["links"] if p in ("P629", "P144", "P4969") and v not in items}
    is_game = set(items) | games_among(outside, refresh)

    def link(a, kind, b):
        related.setdefault(a, set()).add(f"{kind}:{b}")

    for q, g in items.items():
        for p, v, name in g["links"]:
            if v == q:
                continue
            if v not in is_game and p != "P747":
                continue  # a film, book or toy the game is based on: not a version of it
            if p == "P629":
                parent.setdefault(q, v)
            elif p == "P747":
                if v in items:
                    parent.setdefault(v, q)
            elif p in ("P144", "P4969"):
                # P144: q is based on v. P4969: v is derived from q.
                child, source = (q, v) if p == "P144" else (v, q)
                child_game = items.get(child)
                source_title = name if p == "P144" else g["label"]
                if child_game is None:
                    # A derivative on none of our systems: only the link.
                    link(source, "source-of", child)
                    continue
                kind = version_kind(child_game, source_title)
                if kind == "port":
                    parent.setdefault(child, source)
                elif kind == "remake":
                    link(child, "remake-of", source)
                    link(source, "remade-as", child)
                else:
                    link(child, "based-on", source)
                    link(source, "source-of", child)

    def root(q):
        seen = [q]
        while q in parent and parent[q] not in seen:
            q = parent[q]
            seen.append(q)
        if q in parent:  # a loop: the best known item of it
            loop = seen[seen.index(parent[q]):]
            return max(loop, key=lambda x: (items[x]["sitelinks"] if x in items else -1, -int(x[1:])))
        return q

    works = {q: root(q) for q in items}
    for games in systems.values():
        for q, g in games.items():
            w = works[q]
            g["work"] = "" if w == q else w  # empty: the game is its own work (the common case, saves space)
            g["related"] = sorted(r for r in related.get(q, ()) if r.split(":", 1)[1] != w)
    return works


def write(sections):
    blobs, header = [], ["PGDB1"]
    offset = 0
    for system, text in sections:
        blob = zlib.compress(text.encode(), 9)
        header.append(f"{system} {offset} {len(blob)}")
        blobs.append(blob)
        offset += len(blob)
    data = ("\n".join(header) + "\n\n").encode() + b"".join(blobs)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    tmp = OUT.with_suffix(".tmp")
    tmp.write_bytes(data)
    tmp.replace(OUT)
    return len(data)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--refresh", action="store_true", help="query everything again instead of using the cache")
    ap.add_argument("--only", help="comma-separated system ids")
    args = ap.parse_args()
    only = set(args.only.split(",")) if args.only else None
    sections, raw, games = [], 0, 0
    systems = {}
    for system, platforms in SYSTEMS.items():
        if only and system not in only:
            continue
        systems[system] = build_system(system, platforms, args.refresh)
    works = resolve_works(systems, args.refresh)
    grouped = sum(1 for q, w in works.items() if q != w)
    print(f"{grouped} items belong to another item's work", file=sys.stderr)
    for system, found in systems.items():
        text = game_lines(found)
        n = text.count("\n")
        raw += len(text.encode())
        games += n
        print(f"{system:16} {n:6} games", file=sys.stderr)
        sections.append((system, text))
    size = write(sections)
    print(f"{games} entries, {raw / 1e6:.2f} MB of text, {size / 1e6:.2f} MB compressed -> {OUT}", file=sys.stderr)


if __name__ == "__main__":
    main()
