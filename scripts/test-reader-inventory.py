#!/usr/bin/env python3
"""Tests of scripts/reader-inventory.py (python3 -I scripts/test-reader-inventory.py; scripts/gate.sh's fast checks run it when
the inventory script or this file changed).

25.11.2026 (gate false reds): a moved or renamed read line used to stop merges and release picks. Now it is a note; a new reader
in a module without a fault case is a warning; a registered fault case or test that went missing is still red.
"""
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
SKIP = shutil.ignore_patterns("target", "build", ".gradle", ".git", "*.png", "*.jpg", "*.ttf", "*.otf", "*.so")


class Tree:
    """The files the inventory reads, copied into a temporary folder (the gate host has no .git: plain copies)."""

    def __init__(self):
        self.dir = tempfile.TemporaryDirectory(prefix="reader-inventory-")
        self.root = Path(self.dir.name)
        for name in ("src", "android", "tests/common", "scripts/android"):
            if (ROOT / name).is_dir():
                shutil.copytree(ROOT / name, self.root / name, ignore=SKIP)
        (self.root / "tests/fixtures").mkdir(parents=True, exist_ok=True)
        for name in ("tests/data_sources.rs", "tests/emulator_routes.rs", "scripts/reader-inventory.py",
                     *[f"tests/fixtures/reader-{k}.tsv" for k in ("inventory", "coverage", "audit")]):
            if (ROOT / name).is_file():
                shutil.copy2(ROOT / name, self.root / name)

    def run(self, *args):
        p = subprocess.run([sys.executable, "-I", str(self.root / "scripts/reader-inventory.py"), *args],
                           capture_output=True, text=True, timeout=300)
        return p.returncode, p.stdout + p.stderr

    def edit(self, name, old, new):
        path = self.root / name
        text = path.read_text()
        assert old in text, f"{name} has no {old!r}"
        path.write_text(text.replace(old, new, 1))

    def close(self):
        self.dir.cleanup()


def a_read_line(name):
    """A line of the reviewed list in file `name` (the inventory's own view of a real read site)."""
    for line in (ROOT / "tests/fixtures/reader-inventory.tsv").read_text().splitlines():
        if line.startswith(name + "\t"):
            return line.split("\t", 2)[2]
    raise AssertionError(f"no read site of {name} in the reviewed list")


class ReaderInventory(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.base = Tree()
        rc, out = cls.base.run("--write")
        assert rc == 0, out

    @classmethod
    def tearDownClass(cls):
        cls.base.close()

    def tree(self):
        tree = Tree.__new__(Tree)
        tree.dir = tempfile.TemporaryDirectory(prefix="reader-inventory-")
        tree.root = Path(tree.dir.name) / "t"
        shutil.copytree(self.base.root, tree.root)
        self.addCleanup(tree.close)
        return tree

    def test_the_reviewed_tree_is_clean(self):
        rc, out = self.tree().run()
        self.assertEqual(rc, 0, out)
        self.assertNotIn("WARNING new reader", out)
        self.assertNotIn("differ", out)

    def test_a_moved_line_in_a_tested_reader_is_a_note_not_red(self):
        # The false red: a reader with its fault case gets a new variable name; only the line's text changed.
        tree = self.tree()
        line = a_read_line("src/settings.rs")
        tree.edit("src/settings.rs", line, line.replace("(", "( ", 1))
        rc, out = tree.run()
        self.assertEqual(rc, 0, out)
        self.assertIn("read site(s) differ", out)
        self.assertNotIn("WARNING new reader", out)
        rc, out = tree.run("--strict")
        self.assertEqual(rc, 1, out)

    def test_a_new_reader_without_a_fault_case_is_a_warning_and_in_the_report(self):
        tree = self.tree()
        (tree.root / "src/brand_new_reader.rs").write_text('pub fn load() -> Vec<u8> { std::fs::read("/tmp/x").unwrap_or_default() }\n')
        rc, out = tree.run()
        self.assertEqual(rc, 0, out)
        self.assertIn("WARNING new reader without a fault case: src/brand_new_reader.rs", out)
        rc, out = tree.run("--report")
        self.assertEqual(rc, 0, out)
        self.assertIn("untested reader: src/brand_new_reader.rs", out)

    def test_a_registered_fault_test_that_went_missing_stays_red(self):
        tree = self.tree()
        tree.edit("src/shelf_space.rs", "fn file_source_rule_shelf_space", "fn renamed_away")
        rc, out = tree.run()
        self.assertEqual(rc, 1, out)
        self.assertIn("Inventoried private fault test missing", out)

    def test_write_follows_the_code(self):
        tree = self.tree()
        (tree.root / "src/brand_new_reader.rs").write_text('pub fn load() -> Vec<u8> { std::fs::read("/tmp/x").unwrap_or_default() }\n')
        self.assertEqual(tree.run("--write")[0], 0)
        rc, out = tree.run()
        self.assertEqual(rc, 0, out)
        self.assertNotIn("differ", out)
        # The untested reader stays visible after the list followed the code.
        self.assertIn("untested reader: src/brand_new_reader.rs", tree.run("--report")[1])


if __name__ == "__main__":
    unittest.main(verbosity=1)
