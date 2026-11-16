#!/usr/bin/env python3
"""Comment-only proof for the merge gate's fast lane (13.11.2026, docs/dev/merge-gate.md "Comment-only lane").

Usage: gate-comments.py <worktree> <base-ref>   (changed paths on stdin, one per line, as scripts/gate-select.py gets them)
Prints `comments` when every changed file is either
  - a Rust, shell or Python file that differs from <base-ref>'s version only in plain comments, whitespace and blank lines
    (the file with its comments removed is identical to the old one; strings, doc comments, doctests and directives count
    as code), or
  - a document the short path already accepts (unembedded Markdown, scripts/gate-select.py's rule),
and at least one file is a comment-only code file. Anything else prints `no`: new, deleted, renamed or symlinked files,
other file types, unreadable or unbalanced sources. The check fails closed; a bug here must never lead to `comments`.
"""
from pathlib import Path, PurePosixPath
import importlib.util
import io
import re
import subprocess
import sys
import tokenize


def strip_rust(text):
    """Token-ish form of Rust source without plain comments: whitespace runs collapse to one space outside literals,
    literals stay verbatim. `///`, `//!`, `/**` and `/*!` stay (doc comments are attributes and doctests). None if unbalanced."""
    out, i, n = [], 0, len(text)

    def literal_end(j, hashes):
        close = '"' + "#" * hashes
        k = text.find(close, j)
        return -1 if k < 0 else k + len(close)

    while i < n:
        c = text[i]
        if c == "/" and text.startswith("//", i):
            doc = text.startswith("///", i) or text.startswith("//!", i)
            j = text.find("\n", i)
            j = n if j < 0 else j
            if doc:
                out.append(text[i:j])
            else:
                out.append(" ")
            i = j
        elif c == "/" and text.startswith("/*", i):
            doc = (text.startswith("/**", i) and not text.startswith("/**/", i) and not text.startswith("/***", i)) \
                or text.startswith("/*!", i)
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            if depth:
                return None
            out.append(text[i:j] if doc else " ")
            i = j
        elif c in "rbc" and re.match(r'(br|cr|r)#*"|b"|c"|b\'', text[i:i + 12]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            m = re.match(r'(br|cr|r)(#*)"', text[i:i + 40])
            if m:
                end = literal_end(i + m.end(), len(m.group(2)))
                if end < 0:
                    return None
                out.append(text[i:end])
                i = end
            elif text[i] in "bc" and text[i + 1] == '"':
                j = i + 2
                while j < n and text[j] != '"':
                    j += 2 if text[j] == "\\" else 1
                if j >= n:
                    return None
                out.append(text[i:j + 1])
                i = j + 1
            else:   # b'x'
                m = re.match(r"b'(\\.[^']*|[^\\'])'", text[i:i + 16])
                if not m:
                    return None
                out.append(m.group(0))
                i += m.end()
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            if j >= n:
                return None
            out.append(text[i:j + 1])
            i = j + 1
        elif c == "'":
            m = re.match(r"'(\\u\{[0-9a-fA-F_]+\}|\\x[0-9a-fA-F]{2}|\\.|[^\\'\n])'", text[i:i + 14])
            if m:
                out.append(m.group(0))
                i += m.end()
            else:   # a lifetime or loop label
                out.append("'")
                i += 1
        elif c.isspace():
            while i < n and text[i].isspace():
                i += 1
            out.append(" ")
        else:
            out.append(c)
            i += 1
    return re.sub(r" +", " ", "".join(out)).strip()


def strip_shell(text):
    """Lines of a shell script without comments (whole-line and trailing `#` at a word start), blank lines and indentation.
    Quoted text, heredoc bodies, backtick spans and the line after a backslash stay verbatim. None if a quote or heredoc is open."""
    lines, kept = text.split("\n"), []
    state = []          # stack of 'sq', 'dq', 'n' (command substitution inside double quotes), 'bt'
    heredoc = None      # (word, strip_tabs)
    cont = False
    for line in lines:
        literal_start = bool(state) or heredoc is not None
        if heredoc is not None:
            body = line.lstrip("\t") if heredoc[1] else line
            if body == heredoc[0]:
                heredoc = None
            kept.append(line)
            continue
        out, i, pending = [], 0, None
        shebang = not kept and line.startswith("#!")
        while i < len(line):
            ch = line[i]
            top = state[-1] if state else "n0"
            if top == "sq":
                if ch == "'":
                    state.pop()
            elif top == "bt":
                if ch == "`":
                    state.pop()
            elif top == "dq":
                if ch == "\\":
                    out.append(line[i:i + 2])
                    i += 2
                    continue
                if ch == '"':
                    state.pop()
                elif ch == "$" and line.startswith("$(", i):
                    state.append("n")
                    out.append("$(")
                    i += 2
                    continue
                elif ch == "`":
                    state.append("bt")
            else:   # normal code (n0 top level, n inside $( ) in double quotes)
                if ch == "\\":
                    out.append(line[i:i + 2])
                    i += 2
                    continue
                if ch == "'":
                    state.append("sq")
                elif ch == '"':
                    state.append("dq")
                elif ch == "`":
                    state.append("bt")
                elif ch == ")" and top == "n":
                    state.pop()
                elif ch == "#" and not shebang and (i == 0 or line[i - 1] in " \t") and not (cont and i == 0):
                    break
                elif line.startswith("<<", i) and not line.startswith("<<<", i):
                    m = re.match(r"<<(-?)\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\2", line[i:])
                    if m:
                        pending = (m.group(3), m.group(1) == "-")
            out.append(ch)
            i += 1
        cont = line.endswith("\\") and not line.endswith("\\\\")
        if pending:
            heredoc = pending
        text_out = "".join(out)
        if literal_start or state:
            kept.append(text_out.rstrip() if not literal_start else text_out)
        elif text_out.strip():
            kept.append(text_out.strip())
    if state or heredoc is not None:
        return None
    return "\n".join(kept)


DIRECTIVE = re.compile(r"(?i)(type:|noqa|pragma|pylint|fmt:|isort:|mypy|coding[:=])")


def strip_python(text):
    """Token list without comments and layout-only tokens. Comments that act as directives (type:, noqa, coding) count as code."""
    try:
        tokens = []
        for tok in tokenize.generate_tokens(io.StringIO(text).readline):
            if tok.type == tokenize.COMMENT and not DIRECTIVE.search(tok.string) and not tok.string.startswith("#!"):
                continue
            if tok.type in (tokenize.NL, tokenize.ENDMARKER):
                continue
            tokens.append((tok.type, tok.string))
        return tokens
    except (tokenize.TokenError, IndentationError, SyntaxError):
        return None


STRIPPERS = {".rs": strip_rust, ".sh": strip_shell, ".py": strip_python}


def load_selector(root):
    path = Path(__file__).resolve().with_name("gate-select.py")
    spec = importlib.util.spec_from_file_location("gate_select", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def old_text(root, base, name):
    p = subprocess.run(["git", "-C", str(root), "show", f"{base}:{name}"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return p.stdout.decode("utf-8") if p.returncode == 0 else None


def comment_only(root, base, name):
    suffix = PurePosixPath(name).suffix
    strip = STRIPPERS.get(suffix)
    path = root / name
    if strip is None or path.is_symlink() or not path.is_file():
        return False
    try:
        new = path.read_text(encoding="utf-8")
        old = old_text(root, base, name)
    except (OSError, UnicodeDecodeError):
        return False
    if old is None:
        return False
    a, b = strip(old), strip(new)
    return a is not None and b is not None and a == b and old != new


def verdict(root, base, names):
    names = [n for n in names if n]
    if not names:
        return "no"
    code, docs = [], []
    for name in names:
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts or "\r" in name:
            return "no"
        (code if path.suffix in STRIPPERS else docs).append(name)
    if not code:
        return "no"      # documents alone are the short path's business
    if not all(comment_only(root, base, n) for n in code):
        return "no"
    if docs and load_selector(root).selection(root, docs) != "docs":
        return "no"
    return "comments"


if __name__ == "__main__":
    root = Path(sys.argv[1]).resolve()
    base_ref = sys.argv[2] if len(sys.argv) > 2 else "main"
    merge_base = subprocess.run(["git", "-C", str(root), "merge-base", base_ref, "HEAD"], stdout=subprocess.PIPE,
                                stderr=subprocess.DEVNULL, text=True)
    if merge_base.returncode:
        print("no")
        sys.exit(0)
    try:
        print(verdict(root, merge_base.stdout.strip(), sys.stdin.read().split("\n")))
    except Exception:       # fail closed
        print("no")
