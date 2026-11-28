#!/usr/bin/env python3
"""Guards CHANGELOG.md against what merges do to it unnoticed (fast, no build).

    scripts/check-changelog.py [CHANGELOG]                  the checks below; exit 0 ok, 1 findings, 2 could not run
    scripts/check-changelog.py --vetoed FILE [CHANGELOG]    also the vetoed patterns of FILE
    scripts/check-changelog.py --no-vetoed [CHANGELOG]      never the vetoed patterns (the gate: same verdict on every host)
    scripts/check-changelog.py --diff X.Y.Z                 network: the section against the public tag, as a diff
    scripts/check-changelog.py --restore X.Y.Z              network: put the public tag's section back, byte for byte
    scripts/check-changelog.py --record vX.Y.Z              network, after a release is published: record every released
                                                            section of that tag in scripts/changelog-published.txt
    scripts/check-changelog.py --fix [--good-ref REF]...    offline repair of what merges and picks do: drops the copies of a
                                                            duplicate line and puts a changed published section back from a
                                                            git version whose section matches the record (default refs:
                                                            origin/main, main, HEAD); then the checks; exit 0 when clean

The checks (findings as `FINDING<TAB>kind<TAB>CHANGELOG.md:line<TAB>what`):
  published   a section listed in scripts/changelog-published.txt ([0.5.1] ...) must stay byte-equal to the public tag
              it was recorded from (sha256 of its bytes, from its "## [" line up to the next one). Fix: --restore X.Y.Z.
  duplicate   a line of a section that is not published yet ([Unreleased], a release still being cut) must occur only
              once in the whole file. Lines only inside published sections are history and not checked.
  vetoed      a line of a section that is not published yet must not match a pattern of the vetoed list (`pattern<TAB>reason<TAB>date`
              per line, grep -iE style, `#` comments; in the older `pattern|reason|date` lines the pattern is everything before
              the last two `|`). The list is the
              --vetoed FILE, else $CHANGELOG_VETOED, else release-kit/VETOED.txt beside the repository's folder; none = skipped.

--fix (25.11.2026, gate false reds): a duplicate or a revived published line is the merge's doing, not the author's; the queue
runs --fix after its union (one queue commit) and tools/fast-checks.sh after a release pick, so only a vetoed line or a
published section no git version can restore is still red.

Why (20.11.2026): the merge queue unions CHANGELOG.md on conflicts. One train put nine old lines back under the
published [0.5.1] (copies of [Unreleased] lines) and a rejected feature's line back under [Unreleased]; the gate was green.
"""
import argparse
import difflib
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
PUBLISHED = ROOT / "scripts" / "changelog-published.txt"
PUBLIC_REPO = os.environ.get("CHANGELOG_PUBLIC_REPO", "robbyczgw-cla/pocket-ui")
HEADER = re.compile(rb"^## \[([^\]]+)\]")


def sections(data):
    """[(name, first line number, bytes)] for every '## [name]' section; the text before the first one is left out."""
    out = []
    lines = data.splitlines(keepends=True)
    for number, line in enumerate(lines, 1):
        m = HEADER.match(line)
        if m:
            out.append([m.group(1).decode("utf-8", "replace"), number, []])
        elif not out:
            continue
        out[-1][2].append(line)
    return [(name, first, b"".join(body)) for name, first, body in out]


def digest(body):
    return hashlib.sha256(body).hexdigest()


def read_published(path):
    """({version: sha256}, tag) from the record file; ({}, None) when it is missing."""
    if not path.is_file():
        return {}, None
    sums, tag = {}, None
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line.startswith("# from "):
            tag = line[len("# from "):].strip()
        if not line or line.startswith("#"):
            continue
        version, _, value = line.partition(" ")
        sums[version] = value.strip()
    return sums, tag


def read_vetoed(path):
    """[(compiled pattern, source, reason)]; raises ValueError on a pattern Python's re cannot read."""
    out = []
    for number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        if "\t" in raw:
            # pattern<TAB>reason<TAB>date (since 20.11.2026): a pattern keeps every "|" of its alternatives
            pattern, _, rest = raw.partition("\t")
            pattern, reason = pattern.strip(), rest.partition("\t")[0].strip()
        else:
            # the older pattern|reason|date: the pattern is everything before the last two "|"
            parts = raw.rsplit("|", 2)
            pattern = parts[0].strip() if len(parts) == 3 else raw.strip()
            reason = parts[1].strip() if len(parts) == 3 else ""
        try:
            out.append((re.compile(pattern, re.IGNORECASE), pattern, reason))
        except re.error as exc:
            raise ValueError(f"{path}:{number}: pattern {pattern!r} is not a valid regular expression ({exc})") from None
    return out


def find_vetoed(args):
    if args.no_vetoed:
        return None
    if args.vetoed:
        return Path(args.vetoed)
    if os.environ.get("CHANGELOG_VETOED"):
        return Path(os.environ["CHANGELOG_VETOED"])
    beside = ROOT.parent / "release-kit" / "VETOED.txt"
    return beside if beside.is_file() else None


def check(data, published, vetoed, label="CHANGELOG.md"):
    """The findings as (kind, line number, text), in file order."""
    findings = []
    parts = sections(data)
    names = {name for name, _, _ in parts}
    for version, want in published.items():
        if version not in names:
            findings.append(("published", 1, f"[{version}] is published but missing here"))
    open_lines = []          # (line number, text, section) of sections not published yet
    where = {}               # text -> [(line number, section)] over the whole file
    for name, first, body in parts:
        if name in published and digest(body) != published[name]:
            findings.append(("published", first, f"[{name}] is not byte-equal to the published tag any more; "
                             f"see `scripts/check-changelog.py --diff {name}`, fix `--restore {name}`"))
        for offset, raw in enumerate(body.decode("utf-8", "replace").splitlines()):
            text = raw.rstrip()
            if not text.strip() or text.lstrip().startswith("#"):
                continue
            number = first + offset
            where.setdefault(text, []).append((number, name))
            if name not in published:
                open_lines.append((number, text, name))
    for number, text, name in open_lines:
        others = [f":{n} [{s}]" for n, s in where[text] if n != number]
        if others:
            findings.append(("duplicate", number, f"[{name}] line also at {', '.join(others)}: {text.strip()[:90]}"))
        for pattern, source, reason in vetoed or ():
            if pattern.search(text):
                findings.append(("vetoed", number, f"[{name}] matches {source!r} ({reason}): {text.strip()[:90]}"))
    findings.sort(key=lambda f: f[1])
    return findings


def fix(data, published, good=()):
    """(repaired bytes, [(kind, what)]): duplicate copies dropped, published sections put back from `good` (other versions of
    the file, as bytes) where one of them has the recorded section. Vetoed lines and unrestorable sections stay as they are."""
    done = []
    parts = sections(data)
    for name, _, body in parts:
        if name not in published or digest(body) == published[name]:
            continue
        for other in good:
            want = next((b for n, _, b in sections(other) if n == name and digest(b) == published[name]), None)
            if want is not None:
                data = data.replace(body, want, 1)
                done.append(("published", f"[{name}] put back as published"))
                break
    # Duplicates, on the file with its published sections back. A line a published section has is history: every copy in an
    # open section goes. Else the first copy stays ([Unreleased] is on top) and later ones go.
    lines = data.splitlines(keepends=True)
    section = None
    owner = []  # section name per line (None before the first one)
    for raw in lines:
        m = HEADER.match(raw)
        if m:
            section = m.group(1).decode("utf-8", "replace")
        owner.append(section)
    history = {raw.decode("utf-8", "replace").rstrip() for raw, name in zip(lines, owner) if name in published}
    seen, drop = set(), set()
    for i, (raw, name) in enumerate(zip(lines, owner)):
        text = raw.decode("utf-8", "replace").rstrip()
        if name is None or name in published or not text.strip() or text.lstrip().startswith("#"):
            continue
        if text in history or text in seen:
            drop.add(i)
            done.append(("duplicate", f"[{name}] copy dropped: {text.strip()[:90]}"))
        seen.add(text)
    if drop:
        data = b"".join(raw for i, raw in enumerate(lines) if i not in drop)
    return data, done


def git_versions(path, refs):
    """The file at each of refs that exists, as bytes (path relative to its git work tree)."""
    out = []
    try:
        top = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=path.parent, capture_output=True, text=True, timeout=30)
        if top.returncode:
            return out
        rel = path.resolve().relative_to(Path(top.stdout.strip()).resolve()).as_posix()
        for ref in refs:
            p = subprocess.run(["git", "show", f"{ref}:{rel}"], cwd=path.parent, capture_output=True, timeout=30)
            if not p.returncode:
                out.append(p.stdout)
    except (OSError, ValueError, subprocess.TimeoutExpired):
        pass
    return out


def fetch(tag):
    url = f"https://raw.githubusercontent.com/{PUBLIC_REPO}/{tag}/CHANGELOG.md"
    with urllib.request.urlopen(url, timeout=30) as response:
        return response.read()


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("changelog", nargs="?", default=str(ROOT / "CHANGELOG.md"))
    ap.add_argument("--published", default=str(PUBLISHED), help="record file (default scripts/changelog-published.txt)")
    ap.add_argument("--vetoed")
    ap.add_argument("--no-vetoed", action="store_true")
    ap.add_argument("--diff", metavar="VERSION")
    ap.add_argument("--restore", metavar="VERSION")
    ap.add_argument("--record", metavar="TAG")
    ap.add_argument("--from-file", help="with --record/--diff/--restore: read the tag's CHANGELOG from this file (offline)")
    ap.add_argument("--fix", action="store_true", help="repair duplicates and published sections (offline), then check")
    ap.add_argument("--good-ref", action="append", default=[], help="with --fix: a git ref whose file may restore a published "
                    "section (repeatable; default origin/main, main, HEAD)")
    ap.add_argument("--good-file", action="append", default=[], help="with --fix: a file that may restore a published section")
    args = ap.parse_args(argv)
    path = Path(args.changelog)
    try:
        data = path.read_bytes()
    except OSError as exc:
        print(f"check-changelog: cannot read {path}: {exc}", file=sys.stderr)
        return 2
    published, published_tag = read_published(Path(args.published))
    tag_data = (lambda tag: Path(args.from_file).read_bytes()) if args.from_file else fetch

    if args.record:
        tagged = tag_data(args.record)
        lines = [f"# CHANGELOG.md sections as the public tag has them: version, sha256 of the section's bytes (its \"## [\" line",
                 "# up to the next one). Checked by scripts/check-changelog.py; written by its --record vX.Y.Z after a release.",
                 f"# from {args.record}"]
        lines += [f"{name} {digest(body)}" for name, _, body in sections(tagged) if name.lower() != "unreleased"]
        Path(args.published).write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"check-changelog: recorded {len(lines) - 3} published sections of {args.record} in {args.published}")
        return 0
    if args.diff or args.restore:
        version = args.diff or args.restore
        if not published_tag:
            print(f"check-changelog: {args.published} names no tag (`# from vX.Y.Z`)", file=sys.stderr)
            return 2
        want = None
        for name, _, body in sections(tag_data(published_tag)):
            if name == version:
                want = body
        if want is None:
            print(f"check-changelog: {published_tag} has no [{version}] section", file=sys.stderr)
            return 2
        if version in published and digest(want) != published[version]:
            print(f"check-changelog: {published_tag}'s [{version}] does not match the record; nothing changed", file=sys.stderr)
            return 2
        here = {name: body for name, _, body in sections(data)}
        if version not in here:
            print(f"check-changelog: no [{version}] section in {path}", file=sys.stderr)
            return 2
        if args.diff:
            sys.stdout.writelines(difflib.unified_diff(want.decode().splitlines(keepends=True),
                                                       here[version].decode().splitlines(keepends=True),
                                                       f"{published_tag} [{version}]", f"{path.name} [{version}]"))
            return 0 if want == here[version] else 1
        path.write_bytes(data.replace(here[version], want, 1))
        print(f"check-changelog: [{version}] in {path} is {published_tag}'s again")
        return 0

    vetoed_path = find_vetoed(args)
    vetoed = []
    if vetoed_path is not None:
        try:
            vetoed = read_vetoed(vetoed_path)
        except (OSError, ValueError) as exc:
            print(f"check-changelog: vetoed list: {exc}", file=sys.stderr)
            return 2
    if args.fix:
        good = [Path(f).read_bytes() for f in args.good_file] + git_versions(path, args.good_ref or ["origin/main", "main", "HEAD"])
        repaired, done = fix(data, published, good)
        for kind, text in done:
            print(f"FIXED\t{kind}\t{path.name}\t{text}")
        if repaired != data:
            path.write_bytes(repaired)
            data = repaired
    findings = check(data, published, vetoed)
    for kind, number, text in findings:
        print(f"FINDING\t{kind}\t{path.name}:{number}\t{text}")
    veto_note = f"{len(vetoed)} vetoed patterns from {vetoed_path}" if vetoed_path else "vetoed patterns skipped (no list)"
    if findings:
        print(f"check-changelog: {len(findings)} findings in {path} ({len(published)} published sections, {veto_note}). "
              "Duplicates and a changed published section: --fix repairs them offline (else --restore X.Y.Z); "
              "vetoed: take the line out.")
        return 1
    print(f"check-changelog: ok ({len(published)} published sections unchanged, no duplicate lines, {veto_note})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
