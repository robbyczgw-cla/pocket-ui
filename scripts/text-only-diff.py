#!/usr/bin/env python3
"""Is the change from BASE to TARGET text only? The test behind the text-only release steps (docs/dev/release.md).

    python3 scripts/text-only-diff.py BASE [TARGET] [--repo DIR]      (TARGET default HEAD)

Text = what no compiler, test harness or script reads as code: the translation tables and their lists (assets/i18n/*),
the documents and their pictures (docs/**), and the Markdown files at the top (README.md, CHANGELOG.md, ...). The manual
under docs/user is shown in the app, so a text-only change still needs the text checks (scripts/pre-release.sh) and new
programs; it does not need the full test run again.

Prints every changed file as `text  <path>` or `CODE  <path>`, then one summary line:
    text-only: yes, N files since BASE (translations: yes|no, manual: yes|no)
    text-only: NO, M of N files are not text: <first ones>
Exit 0 text only (also: nothing changed), 1 not text only, 2 usage or git error.
"""
import re
import subprocess
import sys

TEXT = re.compile(r"^(assets/i18n/[^/]+|docs/.+|[^/]+\.md)$")


def main(argv):
    args, repo = [], "."
    it = iter(argv)
    for a in it:
        if a == "--repo":
            repo = next(it, None)
            if repo is None:
                return usage()
        elif a.startswith("-"):
            return usage()
        else:
            args.append(a)
    if not 1 <= len(args) <= 2:
        return usage()
    base, target = args[0], args[1] if len(args) == 2 else "HEAD"
    try:
        out = subprocess.run(["git", "-C", repo, "diff", "--name-only", "--no-renames", base, target],
                             check=True, capture_output=True, text=True).stdout
    except (subprocess.CalledProcessError, OSError) as e:
        print(f"text-only-diff: git diff {base} {target} failed: {getattr(e, 'stderr', '') or e}".strip(), file=sys.stderr)
        return 2
    files = [f for f in out.splitlines() if f]
    code = [f for f in files if not TEXT.match(f)]
    for f in files:
        print(("CODE  " if f in code else "text  ") + f)
    if code:
        print(f"text-only: NO, {len(code)} of {len(files)} files are not text: {' '.join(code[:8])}{' ...' if len(code) > 8 else ''}")
        return 1
    tr = any(f.startswith("assets/i18n/") for f in files)
    manual = any(f.startswith("docs/user/") for f in files)
    print(f"text-only: yes, {len(files)} files since {base} (translations: {'yes' if tr else 'no'}, manual: {'yes' if manual else 'no'})")
    return 0


def usage():
    print(__doc__.strip().splitlines()[2].strip(), file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
