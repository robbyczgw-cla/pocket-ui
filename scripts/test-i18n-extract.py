#!/usr/bin/env python3
"""Tests of scripts/i18n-extract.py --check (python3 -I scripts/test-i18n-extract.py; scripts/gate.sh's fast checks run it when
the extractor or this file changed).

25.11.2026 (gate false reds): between 17. and 25.11. about thirty commits only added skip.txt lines for strings nobody sees:
test modules behind `#[cfg(all(test, ...))]`, key=value log lines, font and picture file names, HTTP headers, other programs'
option keys. Each is now recognised; an English UI sentence without a German line is still red.
"""
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent

# Strings that each cost a skip.txt line (commit in brackets), in the shape their source had.
FALSE_ALARMS = r'''
pub fn header() -> &'static str { "Accept-Language: ko-KR,ko;q=0.9,en;q=0.7" }            // bdb4e31bb
pub fn font() -> &'static str { "FusionPixel12-zh_hans-CJK.otf" }                           // c2a47dd54
pub fn key() -> &'static str { "bsnes_mode7_wsMode" }                                       // 16967fd18
pub fn key2() -> &'static str { "mupen64plus-ThreadedRenderer" }                            // 2213e748b
pub fn pacing(a: u32, b: u32) -> String { format!("calls={a} repeated={b} missed={a}") }   // 0a34966e7
pub fn phases(a: u32, b: u32) -> String { format!("main={a} rchar={b} read_bytes={a}") }   // 1c2ad21e1
'''
REAL = r'''
pub fn shown() -> &'static str { "Load the game again" }
pub fn wifi() -> &'static str { "Wi-Fi" }
pub fn saves() -> &'static str { "The .srm files your games write." }
'''
TEST_MODULE = r'''
pub fn picture() -> &'static str { "HEIC" }                                                 // 7c79cfe74
pub fn shader() -> &'static str { "uniform int effect" }                                    // ec825fca6
pub fn fixture() -> &'static str { "Reader jump" }                                          // f98e50391
'''


class Tree:
    def __init__(self, lib, files):
        self.dir = tempfile.TemporaryDirectory(prefix="i18n-extract-")
        self.root = Path(self.dir.name)
        (self.root / "scripts").mkdir()
        shutil.copy2(HERE / "i18n-extract.py", self.root / "scripts/i18n-extract.py")
        (self.root / "assets/i18n").mkdir(parents=True)
        (self.root / "assets/i18n/de.tsv").write_text("# test table\nWi-Fi\tWLAN\n")
        (self.root / "src").mkdir()
        (self.root / "src/lib.rs").write_text(lib)
        for name, text in files.items():
            (self.root / "src" / name).parent.mkdir(parents=True, exist_ok=True)
            (self.root / "src" / name).write_text(text)

    def check(self):
        p = subprocess.run([sys.executable, "-I", str(self.root / "scripts/i18n-extract.py"), "--check"],
                           capture_output=True, text=True, timeout=120)
        return p.returncode, p.stdout + p.stderr

    def close(self):
        self.dir.cleanup()


class Extract(unittest.TestCase):
    def tree(self, lib, files):
        tree = Tree(lib, files)
        self.addCleanup(tree.close)
        return tree

    def test_the_false_alarms_pass_without_a_skip_line(self):
        lib = 'pub mod machine;\n#[cfg(all(test, target_os = "linux"))]\nmod android_video_tests;\n'
        rc, out = self.tree(lib, {"machine.rs": FALSE_ALARMS, "android_video_tests.rs": TEST_MODULE}).check()
        self.assertEqual(rc, 0, out)

    def test_a_test_module_behind_a_path_attribute_and_its_children_pass(self):
        lib = 'pub mod covers;\n'
        covers = '#[cfg(test)]\n#[path = "custom_tests.rs"]\nmod custom_tests;\n'
        child = 'mod deeper;\n' + TEST_MODULE
        rc, out = self.tree(lib, {"covers.rs": covers, "custom_tests.rs": child, "custom_tests/deeper.rs": TEST_MODULE}).check()
        self.assertEqual(rc, 0, out)

    def test_an_english_ui_sentence_without_german_stays_red(self):
        rc, out = self.tree("pub mod page;\n", {"page.rs": REAL}).check()
        self.assertEqual(rc, 1, out)
        self.assertIn("'Load the game again'", out)
        self.assertIn("'The .srm files your games write.'", out)
        self.assertNotIn("\t'Wi-Fi'", out)  # translated: counted, never listed as missing

    def test_a_tests_named_file_the_app_compiles_stays_checked(self):
        # Named like a test, declared without a test cfg: the app builds it, so its strings are UI text.
        rc, out = self.tree("pub mod page_tests;\n", {"page_tests.rs": REAL}).check()
        self.assertEqual(rc, 1, out)
        self.assertIn("'Load the game again'", out)


if __name__ == "__main__":
    unittest.main(verbosity=1)
