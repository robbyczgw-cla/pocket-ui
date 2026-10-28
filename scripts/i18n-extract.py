#!/usr/bin/env python3
"""Lists the user-visible strings in src/ and which of them assets/i18n/de.tsv translates.

    scripts/i18n-extract.py               coverage, then every string without a translation (file:line)
    scripts/i18n-extract.py --summary     only the coverage line
    scripts/i18n-extract.py --stub        the missing ones as table lines to fill in ("English<TAB>")
    scripts/i18n-extract.py FILE...       only these files

A heuristic, not a parser: it reads string literals outside tests, logs, panics and attributes, keeps the ones
that read like words for a person (a capital letter or a space, not a path, key or identifier) and turns format
strings into patterns ("{} games" -> "{n} games"). The authoritative check is tests/i18n_snapshots.rs, which
renders the snapshot states in German and fails on what is shown untranslated. See docs/DEVELOPING.md
"Translations".
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "src")
TABLE = os.path.join(ROOT, "assets", "i18n", "de.tsv")
KEEP = os.path.join(ROOT, "assets", "i18n", "keep.txt")
SKIP = os.path.join(ROOT, "assets", "i18n", "skip.txt")

# Files that show nothing on the screen (tools, tests, the docs check).
SKIP_FILES = {"doc_checks.rs", "bin/hello_pad.rs", "bin/store_tool.rs", "manual.rs", "curator/tests.rs", "web/tests.rs", "testutil.rs"}
# Calls whose strings are for a log, a developer or the machine.
SKIP_CALLS = {
    "println", "eprintln", "print", "eprint", "panic", "assert", "assert_eq", "assert_ne", "debug_assert",
    "debug_assert_eq", "unreachable", "todo", "unimplemented", "expect", "mark", "include_str", "include_bytes",
    "env", "option_env", "concat", "log", "trace", "debug", "warn", "info", "error", "matches", "var", "var_os",
    "join", "arg", "args", "Command", "new_command", "starts_with", "ends_with", "strip_prefix", "strip_suffix",
    "split", "split_once", "contains", "find", "replace", "get", "set", "value", "with_setting", "remove", "insert",
    "entry", "open", "read", "write", "create", "write_all", "status", "spawn", "output", "file", "setting",
    "position", "parse", "trim_start_matches", "trim_end_matches", "rsplit_once", "lines", "with_extension",
    "is_some_and", "eq", "eq_ignore_ascii_case", "send", "log_line", "sh", "run", "systemctl", "swaymsg",
}


def lex(src):
    """(kind, text, line) tokens: 'str' literals (content unescaped roughly), 'id' identifiers, 'p' punctuation."""
    i, n, line = 0, len(src), 1
    out = []
    while i < n:
        c = src[i]
        if c == "\n":
            line += 1
            i += 1
        elif c.isspace():
            i += 1
        elif src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
        elif src.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif src.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    line += src[i] == "\n"
                    i += 1
        elif c == "r" and re.match(r'r#*"', src[i:i + 10]) and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            m = re.match(r'r(#*)"', src[i:])
            end = '"' + m.group(1)
            j = src.find(end, i + len(m.group(0)))
            body = src[i + len(m.group(0)):j]
            out.append(("str", body, line))
            line += body.count("\n")
            i = j + len(end)
        elif c == "b" and i + 1 < n and src[i + 1] in "\"'" and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            # Byte strings and bytes: never shown.
            q = src[i + 1]
            j = i + 2
            while j < n and src[j] != q:
                j += 2 if src[j] == "\\" else 1
            line += src[i:j].count("\n")
            i = j + 1
        elif c == '"':
            j, buf = i + 1, []
            while j < n and src[j] != '"':
                if src[j] == "\\":
                    nxt = src[j + 1]
                    if nxt == "\n":
                        j += 2
                        while j < n and src[j] in " \t\n":
                            line += src[j] == "\n"
                            j += 1
                        line += 1
                        continue
                    m = re.match(r"\\u\{([0-9a-fA-F]+)\}", src[j:j + 12])
                    if m:
                        buf.append(chr(int(m.group(1), 16)))
                        j += len(m.group(0))
                        continue
                    buf.append({"n": "\n", "t": "\t", '"': '"', "\\": "\\", "'": "'", "0": ""}.get(nxt, "\\" + nxt))
                    j += 2
                else:
                    line += src[j] == "\n"
                    buf.append(src[j])
                    j += 1
            out.append(("str", "".join(buf), line))
            i = j + 1
        elif c == "'":
            # A char literal ('a', '\n', '\u{2026}') or a lifetime ('a).
            m = re.match(r"'(\\u\{[0-9a-fA-F]+\}|\\.|[^\\'])'", src[i:])
            if m:
                i += len(m.group(0))
            else:
                i += 1
                while i < n and (src[i].isalnum() or src[i] == "_"):
                    i += 1
        elif c.isalpha() or c == "_":
            j = i
            while j < n and (src[j].isalnum() or src[j] == "_"):
                j += 1
            out.append(("id", src[i:j], line))
            i = j
        elif c.isdigit():
            j = i
            while j < n and (src[j].isalnum() or src[j] in "_."):
                j += 1
            i = j
        else:
            out.append(("p", c, line))
            i += 1
    return out


def drop_tests(tokens):
    """Leaves out `#[cfg(test)]` items (test modules and test-only functions)."""
    out, i = [], 0
    while i < len(tokens):
        t = tokens[i]
        if t == ("p", "#", t[2]) and [x[1] for x in tokens[i + 1:i + 6]] == ["[", "cfg", "(", "test", ")"]:
            # Skip the attribute, then the item up to its closing brace or semicolon.
            j = i + 7
            depth = 0
            while j < len(tokens):
                k, v = tokens[j][0], tokens[j][1]
                if k == "p" and v == "{":
                    depth += 1
                elif k == "p" and v == "}":
                    depth -= 1
                    if depth == 0:
                        break
                elif k == "p" and v == ";" and depth == 0:
                    break
                j += 1
            i = j + 1
            continue
        out.append(t)
        i += 1
    return out


def literals(path):
    with open(path, encoding="utf-8") as f:
        tokens = drop_tests(lex(f.read()))
    stack = []  # (callee, is_format)
    attr = 0
    prev = [None, None]
    for k, v, line in tokens:
        if k == "p" and v in "([{":
            callee = None
            if prev[1] and prev[1][0] == "id":
                callee = prev[1][1]
            elif prev[1] and prev[1][1] == "!" and prev[0] and prev[0][0] == "id":
                callee = prev[0][1]
            if v == "[" and prev[1] and prev[1][1] == "#":
                attr += 1
                stack.append(("#attr", False))
            else:
                stack.append((callee, callee in ("format", "write", "writeln", "format_args")))
        elif k == "p" and v in ")]}":
            if stack:
                c, _ = stack.pop()
                if c == "#attr":
                    attr -= 1
        elif k == "str":
            skip = attr > 0 or any(c in SKIP_CALLS for c, _ in stack)
            fmt = bool(stack) and stack[-1][1] and prev[1] and prev[1][1] == "("
            if not skip:
                yield line, v, fmt
        prev = [prev[1], (k, v, line)]


WORDY = re.compile(r"[A-Za-zÀ-ÿ]{2,}")


def visible(s):
    t = s.strip()
    if len(WORDY.findall(t)) == 0:
        return False
    if re.search(r"https?://|\.(json|cfg|sh|so|png|txt|tsv|bin|zip|7z|chd|iso|log|toml|conf|service|desktop|xml)\b|^/|^\.|^-|::|=>|==|\{:\?\}|\{\w*:\?\}", t):
        return False
    if re.fullmatch(r"[a-z0-9_.\-:/+@%]+", t):  # ids, keys, file names, core names
        return False
    if re.fullmatch(r"[A-Z0-9_]+", t) and "_" in t:  # constants, env names
        return False
    if t.startswith("POCKET") or "=" in t and " " not in t:
        return False
    if re.fullmatch(r"[A-Za-z0-9_]+(\.[A-Za-z0-9_]+)+", t):  # system.cfg keys
        return False
    if not re.search(r"[A-Z]", t) and " " not in t:
        return False
    if re.fullmatch(r"[A-Z]{2,3}", t):  # country and region codes
        return False
    if re.fullmatch(r"[A-Z][a-z0-9]+(?:[A-Z][A-Za-z0-9]*)+", t):  # CamelCase keys of other programs' files and APIs
        return False
    if " " not in t and re.search(r"[:/`]", t):  # Gamepad:Button:South, EmuCore/GS, zone names, `Button N`
        return False
    if t.startswith("#") or "`" in t:  # shader and config text, binding expressions
        return False
    if re.fullmatch(r"(?:\{s\}\s*)*[^A-Za-z]*(?:\{s\}\s*)*(?:Hz|kHz|MHz|GHz|ms|fps|KB|MB|GB|mA|W|V|bytes?)?", t):  # units
        return False
    return True


def to_pattern(s, fmt):
    """A format string as the table writes it: {} and {name} become {s} (the table may say {n} or {t})."""
    if not fmt:
        return s
    s = re.sub(r"\{\{", "\x00", s)
    s = re.sub(r"\}\}", "\x01", s)
    s = re.sub(r"\{[^{}]*\}", "{s}", s)
    return s.replace("\x00", "{").replace("\x01", "}")


def table_lines():
    """(English, German) of every line of the table, as the app reads them (`i18n::Table::parse`)."""
    out = []
    with open(TABLE, encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n")
            if not line.strip() or line.startswith("#") or "\t" not in line:
                continue
            en, de = line.split("\t", 1)
            # `!` in front marks a careful pattern (src/i18n.rs); the English is what follows.
            out.append((en.strip().lstrip("!").replace("\\n", "\n").replace("\\t", "\t"), de.strip()))
    return out


HOLE = re.compile(r"\{\w+\}")


def hole_rx(p, i=0):
    """A table placeholder as a regex: {n} {m} {k} {n2}... a number, {t}... a group checked in turn, {s} any text."""
    if re.fullmatch(r"\{[nmk]\d*\}", p):
        return r"[-+]?\d+(?:[.,]\d+)*"
    return f"(?P<t{i}>.+?)" if p.startswith("{t") else ".+?"


def load_table():
    """The exact lines, and the patterns as (normalised English, regex, an instance with made-up values)."""
    exact, patterns = {}, []
    for en, _ in table_lines():
        if "{" in en:
            parts = [p for p in re.split(r"(\{\w+\})", en) if p]
            rx = "^" + "".join(hole_rx(p, i) if HOLE.fullmatch(p) else re.escape(p) for i, p in enumerate(parts)) + "$"
            sample = "".join(("7" if re.fullmatch(r"\{[nmk]\d*\}", p) else "Xy") if HOLE.fullmatch(p) else p for p in parts)
            patterns.append((HOLE.sub("{s}", en), re.compile(rx, re.S), sample))
        else:
            exact[en] = True
    return exact, patterns


def glob_rx(p):
    """keep.txt's and skip.txt's `re:` lines (the same small language as tests/i18n_snapshots.rs `keep_match`):
    `^` `$` anchor, `.*` any text, `\\d` one or more digits, `\\` before another character takes it literally."""
    out, i = "", 0
    if p.startswith("^"):
        out, i = "^", 1
    while i < len(p):
        if p.startswith(".*", i):
            out, i = out + ".*", i + 2
        elif p.startswith("\\d", i):
            out, i = out + r"\d+", i + 2
        elif p[i] == "\\" and i + 1 < len(p):
            out, i = out + re.escape(p[i + 1]), i + 2
        elif p[i] == "$" and i == len(p) - 1:
            out, i = out + "$", i + 1
        else:
            out, i = out + re.escape(p[i]), i + 1
    return re.compile(out, re.S)


def load_list(path):
    """keep.txt or skip.txt: exact lines and `re:` patterns."""
    exact, rx = set(), []
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.rstrip("\n")
                if not line.strip() or line.startswith("#"):
                    continue
                if line.startswith("re:"):
                    rx.append(glob_rx(line[3:]))
                else:
                    exact.add(line.strip().replace("\\n", "\n"))
    return exact, rx


def listed(key, lst):
    exact, rx = lst
    return key in exact or any(r.search(key) for r in rx)


def translated(s, exact, patterns, depth=0):
    """Whether the painter finds `s` (a literal, or a format string with {s} holes) in the table."""
    t = s.strip()
    if not t or t in exact:
        return True
    # Drawn line by line: each line is looked up on its own.
    if "\n" in t:
        return all(translated(l, exact, patterns, depth) for l in t.split("\n") if l.strip())
    if re.sub(r"\{s\}", "", t).strip() == "":
        return True
    norm = re.sub(r"\{\w+\}", "{s}", t)
    for p_norm, p_rx, _ in patterns:
        if norm == p_norm:
            return True
        m = p_rx.match(t)
        # A {t} takes only what the table translates in turn (or a hole of the source: "Set to {s}").
        if m and depth < 6 and all(not re.search(r"[A-Za-z]{2}", re.sub(r"\{s\}", "", v)) or translated(v, exact, patterns, depth + 1) for v in m.groupdict().values()):
            return True
    # Put together from sentences the table has one by one, as the painter splits a paragraph (`i18n::by_sentences`);
    # a hole glued to a sentence's end ("renders.{s}") is the next sentence.
    glued = re.sub(r"\.(\{s\})+$", ".", t)
    parts = [p for p in re.split(r"(?<=\.\)) |(?<=\.) ", glued) if p.strip()]
    if len(parts) > 1 and all(translated(p, exact, patterns, depth + 1) for p in parts):
        return True
    if glued != t and translated(glued, exact, patterns, depth + 1):
        return True
    if "{s}" in t:
        # A format string: some line of the table has to fit it ("{s} game{s}" is "{n} games" or "1 game").
        parts = [p for p in re.split(r"(\{s\})", t) if p]
        rx = re.compile("^" + "".join(".+" if p == "{s}" else re.escape(p) for p in parts) + "$", re.S)
        longest = max((p for p in parts if p != "{s}"), key=len, default="")
        for en in exact:
            if longest in en and rx.match(en):
                return True
        for _, _, sample in patterns:
            if longest in sample and rx.match(sample):
                return True
    return False


def strings(files):
    """{shown string: [file:line, ...]} of everything that reads like words for a person."""
    seen = {}
    for path in sorted(files):
        rel = os.path.relpath(path, SRC)
        if rel in SKIP_FILES:
            continue
        for line, s, fmt in literals(path):
            if not visible(s):
                continue
            key = to_pattern(s, fmt).strip()
            if key:
                seen.setdefault(key, []).append(f"src/{rel}:{line}")
    return seen


def main(argv):
    flags = {a for a in argv if a.startswith("--")}
    files = [a for a in argv if not a.startswith("--")]
    if not files:
        for d, _, names in os.walk(SRC):
            for name in names:
                if name.endswith(".rs"):
                    files.append(os.path.join(d, name))
    exact, patterns = load_table()
    keep, skip = load_list(KEEP), load_list(SKIP)
    seen = strings(files)
    kept = {k for k in seen if listed(k, keep)}
    skipped = {k for k in seen if k not in kept and listed(k, skip)}
    todo = {k: v for k, v in seen.items() if k not in kept and k not in skipped}
    missing = {k: v for k, v in todo.items() if not translated(k, exact, patterns)}
    done = len(todo) - len(missing)
    print(
        f"i18n: {len(seen)} strings found in src/: {done} translated, {len(kept)} shown in English on purpose (keep.txt), "
        f"{len(skipped)} never shown (skip.txt), {len(missing)} missing"
    )
    if "--stale" in flags:
        # Table lines whose English the source no longer has (a string changed or went away). Patterns made for a
        # value that is built at run time ("{n} games" for "{} game{}") can't be told apart and are listed too.
        source = set(seen)
        for en, _ in table_lines():
            if "{" not in en and en not in source and not any(en in "\n".join(k.split("\n")) for k in source if "\n" in k):
                print(f"stale?\t{en!r}")
        return 0
    if "--summary" in flags:
        return 0
    for k in sorted(missing, key=lambda k: missing[k][0]):
        if "--stub" in flags:
            print(k.replace("\n", "\\n") + "\t")
        else:
            print(f"{missing[k][0]}\t{k!r}")
    if "--check" in flags and missing:
        print(
            f"\n{len(missing)} strings have no German line: translate them in assets/i18n/de.tsv (scripts/i18n-extract.py "
            "--stub prints them as lines to fill in), or add a name to assets/i18n/keep.txt, or something never shown "
            "(a log line, a key) to assets/i18n/skip.txt. docs/DEVELOPING.md \"Translations\".",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except BrokenPipeError:
        sys.exit(0)
