#!/bin/bash
# Lists every Rust crate that ends up in pocket-ui / pocket-core (name, version, licence), checks each licence
# against GPL-2.0-or-later, and can collect the licence texts that have to ship with a binary.
#
#   scripts/third-party.sh                       write docs/THIRD-PARTY-CRATES.md (the list NOTICE.md points to)
#   scripts/third-party.sh --check               print the compatibility verdict only; exit 1 on a problem
#   scripts/third-party.sh --texts FILE          also write the licence / notice files of all crates into FILE
#   scripts/third-party.sh --target TRIPLE       platform to resolve for (default aarch64-unknown-linux-gnu, the device)
#   scripts/third-party.sh --manifest-path PATH   resolve another package (Android APK)
#   scripts/third-party.sh --all-platforms       every target, e.g. to see what a macOS dev build would add
#   scripts/third-party.sh --out FILE            list somewhere else (use - for stdout)
#
# The libretro cores pocket-core loads are checked separately: scripts/check-core-licences.sh (table:
# scripts/core-licences.txt, report mode). The Apache-2.0 crates flagged here only matter if a hosted core is GPL-2.0-only.
#
# It reads `cargo metadata --locked`, so it needs the crates in the local cargo cache (cargo fetch) but no extra
# tools. cargo-deny and cargo-about do the same job if you prefer them; this script has no dependency beyond
# cargo and python3. Dev-dependencies are not part of a release and are left out.
set -eu
cd "$(dirname "$0")/.."
exec python3 -I - "$@" <<'PY'
import json, os, re, subprocess, sys
from pathlib import Path

args = sys.argv[1:]
target = "aarch64-unknown-linux-gnu"
all_platforms = check_only = False
out_path = "docs/THIRD-PARTY-CRATES.md"
texts_path = None
manifest = None
i = 0
while i < len(args):
    a = args[i]
    if a == "--check": check_only = True
    elif a == "--all-platforms": all_platforms = True
    elif a == "--target": i += 1; target = args[i]
    elif a == "--out": i += 1; out_path = args[i]
    elif a == "--manifest-path": i += 1; manifest = args[i]
    elif a == "--texts": i += 1; texts_path = args[i]
    else: sys.exit(f"unknown argument {a} (see the comment at the top of scripts/third-party.sh)")
    i += 1

cmd = ["cargo", "metadata", "--format-version", "1", "--locked"]
if manifest:
    cmd += ["--manifest-path", manifest]
if not all_platforms:
    cmd += ["--filter-platform", target]
r = subprocess.run(cmd + ["--offline"], capture_output=True, text=True)
if r.returncode != 0:
    r = subprocess.run(cmd, capture_output=True, text=True)
if r.returncode != 0:
    sys.exit("cargo metadata failed:\n" + r.stderr)
meta = json.loads(r.stdout)
pk = {p["id"]: p for p in meta["packages"]}
nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
root = meta["resolve"]["root"]

# reachable through normal and build dependencies
seen, todo = set(), [root]
while todo:
    cur = todo.pop()
    if cur in seen:
        continue
    seen.add(cur)
    for d in nodes[cur]["deps"]:
        if any(k["kind"] in (None, "build") for k in d["dep_kinds"]):
            todo.append(d["pkg"])
crates = sorted((pk[i] for i in seen if i != root), key=lambda p: (p["name"], p["version"]))

# linked into the binaries: reachable through normal dependencies only. The rest is build-time only (build
# scripts and what they use): the code they generate is compiled in, the tool itself is not.
linked, todo = set(), [root]
while todo:
    cur = todo.pop()
    if cur in linked:
        continue
    linked.add(cur)
    for d in nodes[cur]["deps"]:
        if any(k["kind"] is None for k in d["dep_kinds"]):
            todo.append(d["pkg"])

# --- GPL-2.0-or-later compatibility of an SPDX expression ------------------------------------------------------
# pocket-core loads GPL-2.0-only cores into its own process, so the binary has to be distributable under GPLv2:
# every crate needs at least one option that is GPLv2-compatible. Apache-2.0 alone is not (patent and
# indemnity terms, per the FSF); GPL-3.0-only crates are not either. "MIT OR Apache-2.0" is fine: take MIT.
OK = {  # may be combined into a GPL-2.0-or-later work that is also shipped next to GPL-2.0-only code
    "MIT", "MIT-0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unlicense", "CC0-1.0", "0BSD",
    "MPL-2.0", "Unicode-3.0", "Unicode-DFS-2016", "BSL-1.0", "NCSA", "Libpng", "bzip2-1.0.6",
    "GPL-2.0", "GPL-2.0-only", "GPL-2.0-or-later", "LGPL-2.0", "LGPL-2.0-or-later", "LGPL-2.1", "LGPL-2.1-only",
    "LGPL-2.1-or-later",
    # the LLVM exception was written to make Apache-2.0 code usable with GPLv2
    "Apache-2.0 WITH LLVM-exception",
}
WHY = {
    "Apache-2.0": "Apache-2.0 alone is not GPLv2-compatible (a problem next to GPL-2.0-only cores)",
    "GPL-3.0": "GPL-3.0-only crate cannot ship next to GPL-2.0-only cores",
    "GPL-3.0-only": "GPL-3.0-only crate cannot ship next to GPL-2.0-only cores",
    "GPL-3.0-or-later": "GPL-3.0-or-later crate cannot ship next to GPL-2.0-only cores",
    "LGPL-3.0": "LGPL-3.0 is not GPLv2-compatible", "LGPL-3.0-or-later": "LGPL-3.0 is not GPLv2-compatible",
}
CAUTION = {"AGPL-3.0", "AGPL-3.0-only", "AGPL-3.0-or-later", "EPL-2.0"}  # legal read needed
tokens = lambda e: re.findall(r"\(|\)|AND|OR|WITH|[A-Za-z0-9.+\-]+", e)

def parse(e):
    """-> (compatible: bool, note). Evaluates AND / OR; WITH exceptions stay part of the identifier."""
    t = tokens(e.replace("/", " OR "))
    pos = 0
    def ident():
        nonlocal pos
        if t[pos] == "(":
            pos += 1
            v = expr()
            pos += 1  # ')'
            return v
        name = t[pos]; pos += 1
        if pos < len(t) and t[pos] == "WITH":
            name += " WITH " + t[pos + 1]; pos += 2
        base = name.removesuffix("+")
        if name in OK or base in OK: return (True, "")
        if name in CAUTION or base in CAUTION: return (False, f"{name}: needs a legal read")
        return (False, f"{name}: " + WHY.get(name, "not on the GPL-2.0-or-later compatible list"))
    def conj():
        nonlocal pos
        ok, note = ident()
        while pos < len(t) and t[pos] == "AND":
            pos += 1
            o2, n2 = ident()
            ok, note = ok and o2, note or n2
        return ok, note
    def expr():
        nonlocal pos
        ok, note = conj()
        while pos < len(t) and t[pos] == "OR":
            pos += 1
            o2, n2 = conj()
            ok, note = (ok or o2), ("" if ok or o2 else note or n2)
        return ok, note
    return expr()

problems = []
build_only = []  # problems in crates that only run at build time
rows = []
for p in crates:
    lic = p.get("license")
    if not lic and p.get("license_file"):
        lic = f"see {p['license_file']}"
    if not lic:
        problems.append((p, "no licence field"))
        lic = "UNKNOWN"
    else:
        if lic.startswith("see "):
            problems.append((p, "licence only as a file, read it"))
        else:
            ok, note = parse(lic)
            if not ok:
                (problems if p["id"] in linked else build_only).append((p, note))
    rows.append((p["name"], p["version"], lic.replace("|", "\\|"), "yes" if p["id"] in linked else "build only"))

scope = "every platform" if all_platforms else target
by_lic = {}
for n, v, l, _k in rows:
    by_lic[l] = by_lic.get(l, 0) + 1

if not check_only:
    L = []
    L.append("# Rust crates in pocket-ui and pocket-core")
    L.append("")
    L.append("Generated by `scripts/third-party.sh` from `Cargo.lock`; do not edit by hand. Resolved for: " + scope + ". "
             "Dev-dependencies are not included. Every crate is used under the licence shown (for `A OR B`, under either).")
    L.append("")
    L.append(f"{len(rows)} crates. Licences (count): " + ", ".join(f"{k} ({v})" for k, v in sorted(by_lic.items(), key=lambda kv: (-kv[1], kv[0]))) + ".")
    L.append("")
    L.append("pocket-ui is GPL-2.0-or-later and pocket-core loads GPL-2.0-only cores into its process, so each crate "
             "linked into a binary needs a GPLv2-compatible option (for `A OR B`, take the compatible one). Result: "
             + ("all compatible." if not problems else f"**{len(problems)} linked crates are a problem:** "
                + ", ".join(f"{p['name']} {p['version']} ({p.get('license')})" for p, _ in problems) + "."))
    if build_only:
        L.append("")
        L.append("Build time only, not linked into any binary (they run in build scripts; the code they generate is compiled in): "
                 + ", ".join(f"{p['name']} {p['version']} ({p.get('license')})" for p, _ in build_only) + ".")
    L.append("")
    L.append("| Crate | Version | Licence | In the binary |")
    L.append("|---|---|---|---|")
    for n, v, l, k in rows:
        L.append(f"| [{n}](https://crates.io/crates/{n}/{v}) | {v} | {l} | {k} |")
    L.append("")
    text = "\n".join(L)
    if out_path == "-":
        print(text)
    else:
        Path(out_path).write_text(text)
        print(f"wrote {out_path} ({len(rows)} crates)")

if texts_path:
    NAMES = re.compile(r"^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICENSE)([-_. ].*)?$", re.I)
    out = []
    missing = []
    for p in crates:
        d = Path(p["manifest_path"]).parent
        files = sorted(f for f in d.iterdir() if f.is_file() and NAMES.match(f.name))
        if not files:
            missing.append(f"{p['name']} {p['version']}")
            continue
        out.append("=" * 100)
        out.append(f"{p['name']} {p['version']}  ({p.get('license')})  {p.get('repository') or ''}")
        for f in files:
            out.append("-" * 100)
            out.append(f"[{f.name}]")
            out.append(f.read_text(errors="replace").rstrip())
        out.append("")
    Path(texts_path).write_text("\n".join(out))
    print(f"wrote {texts_path}: licence files of {len(crates) - len(missing)} crates"
          + (f"; no licence file in the package for: {', '.join(missing)}" if missing else ""))

print(f"GPL-2.0-or-later compatibility ({scope}): " + ("all {} linked crates compatible".format(len(linked) - 1) if not problems else f"{len(problems)} problem(s) in crates linked into the binaries"))
for p, why in problems:
    print(f"  PROBLEM  {p['name']} {p['version']} [{p.get('license')}]: {why}")
for p, why in build_only:
    print(f"  build-time only, not linked  {p['name']} {p['version']} [{p.get('license')}]: {why}")
sys.exit(1 if problems else 0)
PY
