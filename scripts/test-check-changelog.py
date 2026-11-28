#!/usr/bin/env python3
"""Tests of scripts/check-changelog.py (python3 -I scripts/test-check-changelog.py; scripts/gate.sh's fast checks run it when the guard or this file changed).

The cases are the 20.11.2026 incident: the queue's union merge of CHANGELOG.md put copies of [Unreleased] lines back under
the published [0.5.1] and a rejected feature's line back under [Unreleased], and nothing noticed.
"""
import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("check_changelog", HERE / "check-changelog.py")
cc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cc)

PUBLISHED_TEXT = """# Changelog

Intro.

## [0.5.1] - 2026-11-18

### Added

- **Old feature.** Shipped in 0.5.1.

### Fixed

- **Old fix.** Shipped in 0.5.1.

## [0.1.9] - 2026-11-01

- README: a table of the devices.

## [0.1.8] - 2026-10-31

- README: a table of the devices.
"""

UNRELEASED = """## [Unreleased]

### Added

- **New thing.** Only on main.

### Fixed

- **New fix.** Only on main.

"""


def with_unreleased(published_text, unreleased=UNRELEASED):
    head, _, rest = published_text.partition("## [0.5.1]")
    return head + unreleased + "## [0.5.1]" + rest


class Tree:
    """A temporary folder with CHANGELOG.md, a record of the published tag and a vetoed list."""

    def __init__(self, changelog, tag_text=PUBLISHED_TEXT, vetoed="window in the room|Fenster raus|2026-11-20\n"):
        self.dir = tempfile.TemporaryDirectory()
        self.root = Path(self.dir.name)
        self.tag = self.root / "tag-CHANGELOG.md"
        self.tag.write_text(tag_text, encoding="utf-8")
        self.changelog = self.root / "CHANGELOG.md"
        self.changelog.write_text(changelog, encoding="utf-8")
        self.record = self.root / "published.txt"
        self.vetoed = self.root / "VETOED.txt"
        self.vetoed.write_text(vetoed, encoding="utf-8")
        rc, _ = self.run("--record", "v0.5.1", "--from-file", str(self.tag))
        assert rc == 0

    def run(self, *args):
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(out):
            rc = cc.main([str(self.changelog), "--published", str(self.record), *args])
        return rc, out.getvalue()

    def check(self):
        rc, out = self.run("--vetoed", str(self.vetoed))
        return rc, [line.split("\t")[1:3] for line in out.splitlines() if line.startswith("FINDING")]

    def close(self):
        self.dir.cleanup()


class CheckChangelog(unittest.TestCase):
    def tree(self, changelog, **kw):
        tree = Tree(changelog, **kw)
        self.addCleanup(tree.close)
        return tree

    def test_clean_changelog_passes(self):
        rc, findings = self.tree(with_unreleased(PUBLISHED_TEXT)).check()
        self.assertEqual((rc, findings), (0, []))

    def test_union_copy_under_a_published_section_is_red(self):
        # The train: an [Unreleased] line came back under [0.5.1] ### Fixed.
        text = with_unreleased(PUBLISHED_TEXT).replace("- **Old fix.**", "- **New fix.** Only on main.\n- **Old fix.**")
        rc, findings = self.tree(text).check()
        self.assertEqual(rc, 1)
        kinds = [kind for kind, _ in findings]
        self.assertIn("published", kinds)
        self.assertIn("duplicate", kinds)

    def test_any_byte_in_a_published_section_is_red(self):
        rc, findings = self.tree(with_unreleased(PUBLISHED_TEXT).replace("Shipped in 0.5.1.", "Shipped in 0.5.1 .", 1)).check()
        self.assertEqual((rc, [kind for kind, _ in findings]), (1, ["published"]))

    def test_a_published_section_taken_out_is_red(self):
        text = with_unreleased(PUBLISHED_TEXT)
        text = text[:text.index("## [0.1.8]")]
        rc, findings = self.tree(text).check()
        self.assertEqual(rc, 1)
        self.assertIn(["published", "CHANGELOG.md:1"], findings)

    def test_twice_under_unreleased_is_red(self):
        text = with_unreleased(PUBLISHED_TEXT, UNRELEASED.replace("- **New fix.** Only on main.\n",
                                                                "- **New fix.** Only on main.\n- **New fix.** Only on main.\n"))
        rc, findings = self.tree(text).check()
        self.assertEqual(rc, 1)
        self.assertEqual([kind for kind, _ in findings], ["duplicate", "duplicate"])

    def test_history_duplicates_and_repeated_headings_pass(self):
        # 0.1.8 and 0.1.9 share a line in the public tags; ### Fixed repeats in every section.
        rc, findings = self.tree(with_unreleased(PUBLISHED_TEXT)).check()
        self.assertEqual((rc, findings), (0, []))

    def test_vetoed_line_under_unreleased_is_red_but_not_in_history(self):
        tag = PUBLISHED_TEXT.replace("- **Old feature.**", "- **A window in the room** was shipped once.\n- **Old feature.**")
        text = with_unreleased(tag, UNRELEASED.replace("### Fixed", "- **A window in the room** (Cozy extras).\n\n### Fixed"))
        rc, findings = self.tree(text, tag_text=tag).check()
        self.assertEqual(rc, 1)
        self.assertEqual([kind for kind, _ in findings], ["vetoed"])

    def test_vetoed_pattern_with_alternation_keeps_its_bars(self):
        # release-kit/check-vetoed.sh cut this pattern at its first "|" (IFS='|'), so "Switch games" passed there.
        vetoed = "Nintendo Switch|Switch (game|games|support)|nie Switch|immer\n"
        text = with_unreleased(PUBLISHED_TEXT, UNRELEASED.replace("### Fixed", "- Plays Switch games now.\n\n### Fixed"))
        rc, findings = self.tree(text, vetoed=vetoed).check()
        self.assertEqual((rc, [kind for kind, _ in findings]), (1, ["vetoed"]))

    def test_tab_separated_vetoed_list(self):
        # The list's format since 20.11.2026: pattern<TAB>reason<TAB>date, a pattern with "|" alternatives stays whole.
        vetoed = "# comment\nNintendo Switch|Switch (game|games|support)\tnie Switch\timmer\nhome screen\tnie\timmer\n"
        text = with_unreleased(PUBLISHED_TEXT, UNRELEASED.replace("### Fixed", "- Switch support.\n- The home screen.\n\n### Fixed"))
        rc, findings = self.tree(text, vetoed=vetoed).check()
        self.assertEqual((rc, [kind for kind, _ in findings]), (1, ["vetoed", "vetoed"]))

    def test_bad_vetoed_pattern_cannot_pass(self):
        tree = self.tree(with_unreleased(PUBLISHED_TEXT), vetoed="window (in|the room|why|date\n")
        rc, out = tree.run("--vetoed", str(tree.vetoed))
        self.assertEqual(rc, 2, out)

    def test_no_vetoed_skips_the_list(self):
        text = with_unreleased(PUBLISHED_TEXT, UNRELEASED.replace("### Fixed", "- A window in the room.\n\n### Fixed"))
        rc, out = self.tree(text).run("--no-vetoed")
        self.assertEqual(rc, 0, out)

    def test_restore_puts_the_tag_section_back_byte_for_byte(self):
        good = with_unreleased(PUBLISHED_TEXT)
        broken = good.replace("- **Old fix.**", "- **New fix.** Only on main.\n- **Old fix.**")
        tree = self.tree(broken)
        rc, out = tree.run("--restore", "0.5.1", "--from-file", str(tree.tag))
        self.assertEqual(rc, 0, out)
        self.assertEqual(tree.changelog.read_text(encoding="utf-8"), good)
        self.assertEqual(tree.check(), (0, []))

    def test_record_leaves_unreleased_out(self):
        tree = self.tree(with_unreleased(PUBLISHED_TEXT), tag_text=with_unreleased(PUBLISHED_TEXT))
        sums, tag = cc.read_published(tree.record)
        self.assertEqual((sorted(sums), tag), (["0.1.8", "0.1.9", "0.5.1"], "v0.5.1"))

    # --fix (25.11.2026): what a merge or a pick does is repaired, a real finding stays red.
    def test_fix_drops_the_second_copy_under_unreleased(self):
        good = with_unreleased(PUBLISHED_TEXT)
        tree = self.tree(good.replace("- **New fix.** Only on main.\n", "- **New fix.** Only on main.\n- **New fix.** Only on main.\n"))
        rc, out = tree.run("--fix", "--no-vetoed")
        self.assertEqual(rc, 0, out)
        self.assertIn("FIXED\tduplicate", out)
        self.assertEqual(tree.changelog.read_text(encoding="utf-8"), good)

    def test_fix_puts_a_union_damaged_published_section_back_and_drops_the_revived_copy(self):
        good = with_unreleased(PUBLISHED_TEXT)
        broken = good.replace("- **Old fix.**", "- **New fix.** Only on main.\n- **Old fix.**")
        tree = self.tree(broken)
        main = tree.root / "main-CHANGELOG.md"
        main.write_text(good, encoding="utf-8")
        rc, out = tree.run("--fix", "--no-vetoed", "--good-file", str(main))
        self.assertEqual(rc, 0, out)
        self.assertEqual(tree.changelog.read_text(encoding="utf-8"), good)

    def test_fix_drops_a_released_line_revived_under_unreleased(self):
        good = with_unreleased(PUBLISHED_TEXT)
        tree = self.tree(good.replace("- **New fix.** Only on main.\n", "- **New fix.** Only on main.\n- **Old fix.** Shipped in 0.5.1.\n"))
        rc, out = tree.run("--fix", "--no-vetoed")
        self.assertEqual(rc, 0, out)
        self.assertEqual(tree.changelog.read_text(encoding="utf-8"), good)

    def test_fix_keeps_a_published_section_red_when_no_version_has_it(self):
        tree = self.tree(with_unreleased(PUBLISHED_TEXT).replace("Shipped in 0.5.1.", "Shipped in 0.5.1 .", 1))
        rc, out = tree.run("--fix", "--no-vetoed", "--good-file", str(tree.changelog))
        self.assertEqual(rc, 1, out)
        self.assertIn("FINDING\tpublished", out)

    def test_fix_keeps_a_vetoed_line_red(self):
        text = with_unreleased(PUBLISHED_TEXT, UNRELEASED.replace("### Fixed", "- **A window in the room** (Cozy extras).\n\n### Fixed"))
        tree = self.tree(text)
        rc, out = tree.run("--fix", "--vetoed", str(tree.vetoed))
        self.assertEqual(rc, 1, out)
        self.assertIn("FINDING\tvetoed", out)
        self.assertEqual(tree.changelog.read_text(encoding="utf-8"), text)


if __name__ == "__main__":
    unittest.main(verbosity=1)
