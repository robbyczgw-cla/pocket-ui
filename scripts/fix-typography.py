#!/usr/bin/env python3
"""Writing rules of the translation tables: a warning list and an automatic fix. Never a merge blocker.

19.11.2026: an apostrophe no person sees must not stop anything. These rules were Rust tests (tests/french.rs,
tests/italian.rs, tests/portuguese.rs) and turned 20-minute merge gates red. Now they live here: the nightly run prints the
list (tools/nightly-gate.sh), and the nightly or release agent runs --write. What users really notice stays in the tests:
cut or overlapping text, missing translations, page names the code looks for, crashes.

    scripts/fix-typography.py              the warning list (exit 0)
    scripts/fix-typography.py --strict     the same, exit 1 when there is anything
    scripts/fix-typography.py --write      fix what can be fixed automatically; the rest stays on the list
    scripts/fix-typography.py --self-test  the rules on fixed examples (tests/french.rs runs it)

Rules (docs/i18n-glossary.md):
  fr  a no-break space (U+00A0) before ? ! : ; and inside « », the straight apostrophe, never U+202F    -> fixed
      "vous"/"votre"/"vos" instead of tu; words of Quebec ("courriel", "magasin", ...)                     -> listed
  it  "perche'", " e' ", "piu'" typed for an accent; a no-break space                                      -> fixed
      formal imperatives ("prema", "scelga", ...)                                                            -> listed
  pt  words of Portugal, not Brazil ("ecrã", "ficheiro", ...)                                              -> listed
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
NBSP = " "
QUEBEC = ["courriel", "magasin", "fin de semaine", "clavardage", "téléverser", "cellulaire", "fichier de journal"]
FORMAL_IT = ["prema", "scelga", "inserisca", "attenda", "riprovi", "verifichi", "si prega"]
PORTUGAL = ["ecrã", "ficheiro", "telemóvel", "descarreg", "utilizador", "partilh", "aceder", "consola"]


def words(text):
    return [w for w in re.split(r"[^\w]+", text.lower()) if w]


def fix_fr(text):
    text = text.replace("’", "'").replace(" ", NBSP)
    text = re.sub(r" ([?!;:])", NBSP + r"\1", text)
    text = re.sub(r"« ?(?! )", "«" + NBSP, text)
    text = re.sub(r"(?<! ) ?»", NBSP + "»", text)
    return text


def look_fr(text):
    out = []
    if any(w in ("vous", "votre", "vos") for w in words(text)):
        out.append("vous, not tu")
    lower = text.lower()
    out += [f"word of Quebec: {w!r}" for w in QUEBEC if w in lower]
    return out


def fix_it(text):
    text = text.replace(NBSP, " ")
    text = re.sub(r"\bperche'", "perché", text)
    text = re.sub(r"\bpiu'", "più", text)
    text = text.replace(" e' ", " è ")
    return text


def look_it(text):
    found = [w for w in words(text) if w in FORMAL_IT] + (["si prega"] if "si prega" in text.lower() else [])
    return [f"formal Italian: {w!r}" for w in found]


def look_pt(text):
    return [f"word of Portugal: {w!r}" for w in words(text) for p in PORTUGAL if w.startswith(p)]


RULES = {"fr": (fix_fr, look_fr), "it": (fix_it, look_it), "pt": (lambda t: t, look_pt)}


def lines(path):
    for n, line in enumerate(path.read_text(encoding="utf-8").split("\n")):
        if line and not line.startswith("#") and "\t" in line:
            en, _, rest = line.partition("\t")
            yield n, en, rest


def scan(root, write=False):
    """[(lang, english, translation, fixed or None, notes)]; with write=True the fixes are saved."""
    found = []
    for lang, (fix, look) in RULES.items():
        path = root / "assets/i18n" / f"{lang}.tsv"
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8").split("\n")
        changed = False
        for n, en, tr in lines(path):
            fixed = fix(tr)
            notes = look(fixed)
            if fixed != tr or notes:
                found.append((lang, en, tr, fixed if fixed != tr else None, notes))
            if write and fixed != tr:
                text[n] = en + "\t" + fixed
                changed = True
        if changed:
            path.write_text("\n".join(text), encoding="utf-8")
    return found


def self_test():
    assert fix_fr("Prêt ? Oui !") == "Prêt ? Oui !"
    assert fix_fr("Choisis : « Mario »") == "Choisis : « Mario »"
    assert fix_fr("«Mario»") == "« Mario »"
    assert fix_fr("l’écran !") == "l'écran !"
    assert fix_fr("Déjà bon : « x »") == "Déjà bon : « x »", "a correct line stays"
    assert fix_fr("{n}:{m} et http://x") == "{n}:{m} et http://x"
    assert look_fr("Choisissez votre jeu") == ["vous, not tu"] and look_fr("Choisis ton jeu") == []
    assert look_fr("Envoie un courriel") == ["word of Quebec: 'courriel'"]
    assert fix_it("perche' e' piu' bello !") == "perché è più bello !"
    assert look_it("Prema A") == ["formal Italian: 'prema'"] and look_it("Premi A") == []
    assert look_pt("Abra o ficheiro") == ["word of Portugal: 'ficheiro'"] and look_pt("compartilhar") == []
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        tree = Path(tmp)
        (tree / "assets/i18n").mkdir(parents=True)
        (tree / "assets/i18n/fr.tsv").write_text("# head : x\nReady?\tPrêt ?\nOK\tOK\nYour game\tVotre jeu\n", encoding="utf-8")
        found = scan(tree, write=True)
        assert [(f[1], f[3] is not None, f[4]) for f in found] == [("Ready?", True, []), ("Your game", False, ["vous, not tu"])], found
        assert (tree / "assets/i18n/fr.tsv").read_text(encoding="utf-8") == "# head : x\nReady?\tPrêt ?\nOK\tOK\nYour game\tVotre jeu\n"
        assert [f[1] for f in scan(tree)] == ["Your game"], "fixed lines are off the list; only the one to look at stays"
    print("fix-typography: self-test ok")


def main(argv):
    if argv == ["--self-test"]:
        self_test()
        return 0
    write = "--write" in argv
    found = scan(ROOT, write=write)
    fixable = [f for f in found if f[3] is not None]
    look = [f for f in found if f[4]]
    for lang, en, tr, fixed, notes in found:
        what = ([f"{'fixed' if write else 'fixable'}: {fixed!r}"] if fixed is not None else []) + notes
        print(f"typography {lang}: {en!r} -> {tr!r}: {'; '.join(what)}")
    print(f"typography: {len(fixable)} line(s) {'fixed' if write else 'fixable with --write'}, {len(look)} to look at "
          "(warnings, never a merge blocker)")
    return 1 if "--strict" in argv and found and not write else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
