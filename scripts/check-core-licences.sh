#!/bin/bash
# Which libretro cores does pocket-core host, and is each one's licence compatible with the rest of the binary?
#
# pocket-ui is GPL-2.0-or-later and its binaries link four Apache-2.0-only crates (winit, glutin, glutin_egl_sys,
# dpi; see scripts/third-party.sh). Apache-2.0 is compatible with GPLv3 but not with GPLv2-only, so the combination
# with the cores pocket-core loads at run time is clean only if no core is GPL-2.0-only. This script reads the cores
# out of src/hostsys.rs (`HANDHELD_CORES` and `system(...)`), looks each one up in scripts/core-licences.txt
# and reports.
#
#   scripts/check-core-licences.sh              report: WARN for a v2-only / mixed-v2 / unlisted core, NOTE for a
#                                               non-commercial one; always exit 0 (owner's decision, 2026-10-22: known, accepted)
#   scripts/check-core-licences.sh --strict     the same, but exit 1 if there is a WARN (for when the decision changes)
#   scripts/check-core-licences.sh --root DIR   another source tree (the tests use fixtures); --table FILE another table
#   scripts/check-core-licences.sh --list       print the whole table with the hosted/not-hosted state
#
# Exit 2: the table or the sources could not be read, or a verdict word is unknown.
# Test: scripts/test-check-core-licences.sh
set -u
here="$(cd "$(dirname "$0")" && pwd)"
exec python3 -I - "$here" "$@" <<'PY'
import re, sys
from pathlib import Path

here = Path(sys.argv[1])
args = sys.argv[2:]
root = here.parent
table = None
strict = listing = False
i = 0
while i < len(args):
    a = args[i]
    if a == "--strict": strict = True
    elif a == "--list": listing = True
    elif a == "--root": i += 1; root = Path(args[i])
    elif a == "--table": i += 1; table = Path(args[i])
    else:
        print(f"unknown argument {a} (see the comment at the top of scripts/check-core-licences.sh)", file=sys.stderr)
        sys.exit(2)
    i += 1
table = table or (root / "scripts" / "core-licences.txt")

FINE = {"or-later", "v3", "permissive"}
PROBLEM = {"v2-only", "mixed-v2"}
NOTE = {"non-commercial"}
KNOWN = FINE | PROBLEM | NOTE

try:
    rows = {}
    for n, line in enumerate(table.read_text().splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        parts = [p.strip() for p in line.split("|", 3)]
        if len(parts) < 4:
            sys.exit(f"{table}:{n}: expected core|verdict|commit|evidence")
        core, verdict, commit, evidence = parts
        if verdict not in KNOWN:
            print(f"{table}:{n}: unknown verdict '{verdict}' for {core} (allowed: {', '.join(sorted(KNOWN))})", file=sys.stderr)
            sys.exit(2)
        rows[core] = (verdict, commit, evidence)
    hostsys_rs = (root / "src" / "hostsys.rs").read_text()
except (OSError, SystemExit) as e:
    if isinstance(e, SystemExit) and not isinstance(e.code, str):
        raise
    print(f"cannot read the inputs: {e}", file=sys.stderr)
    sys.exit(2)

# where each hosted core comes from
hosted = {}  # core -> set of places
# HANDHELD_CORES: the handheld systems pocket-core always hosts (src/main.rs HOST_CORES is an alias of it)
m = re.search(r"const\s+HANDHELD_CORES\s*:[^=]*=\s*&\[(.*?)\]\s*;", hostsys_rs, re.S)
if not m:
    print("HANDHELD_CORES not found in src/hostsys.rs", file=sys.stderr)
    sys.exit(2)
for _sys, core in re.findall(r'\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)', m.group(1)):
    hosted.setdefault(core, set()).add("hostsys.rs HANDHELD_CORES")
# system(&["name", ...], "core", ...)  -- the macro-like constructor in hostsys.rs
for core in re.findall(r'\bsystem\(\s*&\[[^\]]*\]\s*,\s*"([^"]+)"', hostsys_rs):
    hosted.setdefault(core, set()).add("hostsys.rs SYSTEMS")
if not hosted:
    print("no hosted core found", file=sys.stderr)
    sys.exit(2)

if listing:
    for core in sorted(set(rows) | set(hosted)):
        verdict = rows.get(core, ("UNLISTED", "", ""))[0]
        where = ", ".join(sorted(hosted.get(core, []))) or "not hosted"
        print(f"{core:18} {verdict:15} {where}")
    sys.exit(0)

warns = notes = 0
for core in sorted(hosted):
    where = ", ".join(sorted(hosted[core]))
    if core not in rows:
        print(f"WARN  {core} ({where}): not in {table.name}; add it with its verdict and evidence")
        warns += 1
        continue
    verdict, commit, evidence = rows[core]
    if verdict in PROBLEM:
        print(f"WARN  {core} ({where}): {verdict} at {commit}. {evidence}")
        warns += 1
    elif verdict in NOTE:
        print(f"NOTE  {core} ({where}): {verdict}, a separate issue (NOTICE.md)")
        notes += 1
stale = sorted(set(rows) - set(hosted))
for core in stale:
    print(f"NOTE  {core}: in the table but no longer hosted")

if warns:
    print(f"{warns} hosted core(s) are GPL-2.0-only (or contain GPL-2.0-only files): "
          "not combinable with the Apache-2.0 crates under GPLv2" + (" -- strict mode: failing" if strict else " -- report mode, exit 0"))
else:
    print(f"all hosted cores are GPLv3-compatible ({len(hosted)} cores checked, {notes} non-commercial)")
sys.exit(1 if (strict and warns) else 0)
PY
