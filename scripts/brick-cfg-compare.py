#!/usr/bin/env python3
"""Compare two default (no `brick`) builds of the same binary: docs/BRICK-CFG.md "Binary comparison".

usage: brick-cfg-compare.py <reference-binary> <branch-binary>

Both must come from the same source path, toolchain, target and flags, with the build clock held equal.
Prints, per ELF section, how many bytes differ, and whether every differing 8-byte word in .data.rel.ro is a
core::panic::Location line/column pair whose column is unchanged (a moved source line, not changed code).
Exit 0 when only such line numbers differ (or nothing does), 1 otherwise.
"""
import re, struct, subprocess, sys

def sections(path):
    out = subprocess.run(["readelf", "-SW", path], capture_output=True, text=True, check=True).stdout
    rows = []
    for line in out.splitlines():
        m = re.match(r"\s*\[\s*\d+\]\s+(\S+)\s+\S+\s+[0-9a-f]+\s+([0-9a-f]+)\s+([0-9a-f]+)", line)
        if m and m.group(1) != "NULL":
            rows.append((m.group(1), int(m.group(2), 16), int(m.group(3), 16)))
    return rows

def main():
    ref, new = sys.argv[1:3]
    a, b = open(ref, "rb").read(), open(new, "rb").read()
    if len(a) != len(b):
        print(f"sizes differ: {len(a)} vs {len(b)}"); return 1
    secs = sections(ref)
    by_sec, words = {}, set()
    for i in range(len(a)):
        if a[i] != b[i]:
            name = next((n for n, o, z in secs if o <= i < o + z), "(header/none)")
            by_sec[name] = by_sec.get(name, 0) + 1
            if name == ".data.rel.ro":
                words.add(i // 8 * 8)
    print("differing bytes per section:", by_sec or "none (byte-identical)")
    if not by_sec:
        return 0
    if set(by_sec) != {".data.rel.ro"}:
        return 1
    # A Location is {file: &str (ptr, len), line: u32, col: u32}: the 8-byte word holding line+col.
    ok = all(a[w + 4:w + 8] == b[w + 4:w + 8] and struct.unpack("<I", a[w:w + 4])[0] < 200000
             and struct.unpack("<I", b[w:w + 4])[0] < 200000 for w in words)
    print(f"{len(words)} differing words in .data.rel.ro; all line numbers with unchanged column: {ok}")
    return 0 if ok else 1

sys.exit(main())
