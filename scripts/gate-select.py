#!/usr/bin/env python3
"""Fail closed: only unembedded documentation and translation audit notes use the short gate."""
from pathlib import Path, PurePosixPath
import re
import sys


def selection(root, names):
    if not names:
        return "full"
    embedded = set()
    sources = [source for folder in ("src", "tests", "android", "examples")
               for source in root.joinpath(folder).rglob("*.rs")]
    sources += [root / "build.rs"] if root.joinpath("build.rs").exists() else []
    for source in sources:
        if source.is_symlink():
            return "full"
        text = source.read_text()
        # A document read by a player contract outside the short tier is also
        # an input, even when Rust reads it from disk rather than embedding it.
        if source.name not in ("doc_checks.rs", "i18n.rs"):
            embedded.update(re.findall(r'"(docs/[^"\n]+\.md)"', text))
        # Unknown macro-generated embed paths cannot prove a document harmless.
        for call in re.finditer(r'include_(?:str|bytes)!\s*([({\[])', text):
            if call.group(1) != "(":
                return "full"
            tail = text[call.end():].lstrip()
            if tail.startswith('"'):
                continue
            # The local two-literal image macro: expand every call rather than
            # assuming that a concat prefix can never escape into a document.
            image = re.match(r'concat!\(\s*"([^"]+)"\s*,\s*\$(\w+)\s*,\s*"/"\s*,\s*\$(\w+)\s*\)', tail)
            definition = re.search(r'macro_rules!\s+(\w+)\s*\{\s*\(\$(\w+):literal,\s*\$(\w+):literal\)\s*=>\s*\{\s*$', text[:call.start()])
            if not image or not definition or definition.groups()[1:] != image.groups()[1:]:
                return "full"
            macro = definition.group(1)
            uses = re.findall(r'\b' + re.escape(macro) + r'!\s*\(', text)
            arguments = re.findall(r'\b' + re.escape(macro) + r'!\s*\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)', text)
            if len(uses) != len(arguments) or "#[macro_export]" in text:
                return "full"
            for first, second in arguments:
                path = (source.parent / (image.group(1) + first + "/" + second)).resolve()
                if path.is_relative_to(root):
                    embedded.add(path.relative_to(root).as_posix())
        for name in re.findall(r'include_(?:str|bytes)!\s*\(\s*"([^"]+)"', text):
            if "\\" in name:
                return "full"
            path = (source.parent / name).resolve()
            if path.is_relative_to(root):
                embedded.add(path.relative_to(root).as_posix())
    for name in names:
        path = PurePosixPath(name)
        if not name or path.is_absolute() or ".." in path.parts or "\n" in name or "\r" in name:
            return "full"
        if name in embedded or name in ("LICENSE", "NOTICE.md") or name.startswith("docs/user/"):
            return "full"
        target = root.joinpath(name)
        if any(p.is_symlink() for p in (target, *target.parents)) or not target.resolve().is_relative_to(root):
            return "full"
        doc = name.endswith(".md") and (len(path.parts) == 1 or name.startswith("docs/"))
        # skip.txt is extraction bookkeeping, never a translation table or a painted string.
        if not doc and name != "assets/i18n/skip.txt":
            return "full"
    return "docs"


if __name__ == "__main__":
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    print(selection(root, sys.stdin.read().splitlines()))
