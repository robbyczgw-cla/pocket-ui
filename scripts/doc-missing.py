#!/usr/bin/env python3
"""Prints what the release check's bookkeeping tests would complain about, ready to fix.
    scripts/doc-missing.py            env variables and source modules missing from docs/dev/
German keys: scripts/i18n-extract.py --stub (the missing ones as table lines to fill in).
Writes nothing; descriptions and translations stay a human's job."""
import os, re, sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
dev = "\n".join(
    open(os.path.join(root, f), encoding="utf-8").read()
    for f in ["docs/DEVELOPING.md"] + sorted("docs/dev/" + n for n in os.listdir(os.path.join(root, "docs/dev")) if n.endswith(".md"))
)


def section(title):
    m = re.search(r"^##+ " + re.escape(title) + r"\s*$(.*?)(?=^##? |\Z)", dev, re.S | re.M)
    return m.group(1) if m else ""


env_doc, layout, building = section("Environment variables"), section("Repository layout"), section("Building")
src = os.path.join(root, "src")
variables = {}
for dirpath, _, files in os.walk(src):
    for f in files:
        if not f.endswith(".rs") or f == "doc_checks.rs":
            continue
        path = os.path.join(dirpath, f)
        for name in re.findall(r'"(POCKET[A-Z0-9_]*)"', open(path, encoding="utf-8").read()):
            if "_" in name and f"`{name}" not in env_doc:
                variables.setdefault(name, os.path.relpath(path, root))
print("Environment variables to add under 'Environment variables' in docs/dev/environment.md:")
for name, where in sorted(variables.items()):
    print(f"  | `{name}` | ... | {where}")
print("Source modules to name under 'Repository layout' (binaries: under 'Building'):")
missing = []
for e in sorted(os.listdir(src)):
    p = os.path.join(src, e)
    if e == "bin":
        missing += [f"bin/{b} (binaries table)" for b in sorted(os.listdir(p)) if b not in building]
    elif (e.endswith((".rs", ".c")) or os.path.isdir(p)) and (e + "/" if os.path.isdir(p) else e) not in layout:
        missing.append(e + "/" if os.path.isdir(p) else e)
for m in missing:
    print("  " + m)
if not variables and not missing:
    print("  (nothing missing)")
sys.exit(1 if variables or missing else 0)
