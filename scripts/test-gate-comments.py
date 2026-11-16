#!/usr/bin/env python3
# (Lane measurement, 13.11.: a comment only.)
# (Lane check 3, 13.11.: a comment only.)
"""scripts/gate-comments.py: what counts as a comment-only change, and what never does. Temporary git repository, no builds."""
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("gate_comments", HERE / "gate-comments.py")
gc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gc)


class Strippers(unittest.TestCase):
    def same(self, strip, a, b):
        self.assertIsNotNone(strip(a))
        self.assertEqual(strip(a), strip(b))

    def differ(self, strip, a, b):
        self.assertNotEqual(strip(a), strip(b))

    def test_rust_plain_comments_are_ignored(self):
        self.same(gc.strip_rust, "fn a() { 1 }\n", "// note\nfn a() {\n    1 // tail\n}\n/* block\n /* nested */ */\n")
        self.same(gc.strip_rust, "let x = 1;", "let x = /* c */ 1;")
        self.same(gc.strip_rust, "a b", "a/**/b")

    def test_rust_code_text_and_literals_are_never_comments(self):
        self.differ(gc.strip_rust, 'let s = "a // b";', 'let s = "a";')
        self.differ(gc.strip_rust, 'let s = "a /* b */ c";', 'let s = "a  c";')
        self.differ(gc.strip_rust, 'let s = r#"x // y "quoted""#;', 'let s = r#"x"#;')
        self.differ(gc.strip_rust, "let c = '\"'; f();", "let c = '\"'; g();")
        self.differ(gc.strip_rust, "fn f<'a>(x: &'a str) { a() }", "fn f<'a>(x: &'a str) { b() }")
        self.differ(gc.strip_rust, "ab", "a b")
        self.differ(gc.strip_rust, "let x = 1;", "let x = 2;")

    def test_rust_doc_comments_and_directives_count_as_code(self):
        self.differ(gc.strip_rust, "/// doc\nfn a() {}", "/// other doc\nfn a() {}")
        self.differ(gc.strip_rust, "//! module\nfn a() {}", "fn a() {}")
        self.differ(gc.strip_rust, "/** doc */ fn a() {}", "/** other */ fn a() {}")
        self.same(gc.strip_rust, "/**/ fn a() {}", "fn a() {}")

    def test_rust_unbalanced_source_fails_closed(self):
        self.assertIsNone(gc.strip_rust('let s = "open;'))
        self.assertIsNone(gc.strip_rust("/* never closed"))
        self.assertIsNone(gc.strip_rust('let s = r#"open"#x'.replace('"#x', "")))

    def test_shell_comments_are_ignored(self):
        self.same(gc.strip_shell, "#!/bin/sh\necho hi\n", "#!/bin/sh\n# why\n\n  echo hi   # tail\n")
        self.same(gc.strip_shell, "echo $# ${#x} a#b\n", "echo $# ${#x} a#b # c\n")

    def test_shell_quoted_heredoc_and_continued_text_is_data(self):
        self.differ(gc.strip_shell, "echo 'a\n# b\nc'\n", "echo 'a\nc'\n")
        self.differ(gc.strip_shell, 'echo "a\n# b"\n', 'echo "a"\n')
        self.differ(gc.strip_shell, "cat <<EOF\n# line of the file\nEOF\n", "cat <<EOF\nEOF\n")
        self.differ(gc.strip_shell, "cat <<-'X'\n\t# kept\n\tX\necho a\n", "cat <<-'X'\n\tX\necho a\n")
        self.differ(gc.strip_shell, "echo a #b\n", "echo a #b\necho c\n")
        self.differ(gc.strip_shell, 'x="$(echo "a # b")"\n', 'x="$(echo "a")"\n')
        self.differ(gc.strip_shell, "echo a \\\n# continued\n", "echo a \\\n")
        self.differ(gc.strip_shell, "x=`echo a # b`\n", "x=`echo a`\n")
        self.assertIsNone(gc.strip_shell("echo 'open\n"))
        self.assertIsNone(gc.strip_shell("cat <<EOF\nnever closed\n"))

    def test_python_comments_are_ignored_but_directives_and_docstrings_are_not(self):
        self.same(gc.strip_python, "x = 1\n", "# why\nx = 1  # tail\n\n")
        self.differ(gc.strip_python, "x = 1\n", "x = 1  # noqa\n")
        self.differ(gc.strip_python, "x = 1\n", "# -*- coding: latin-1 -*-\nx = 1\n")
        self.differ(gc.strip_python, "def f():\n    '''doc'''\n", "def f():\n    '''other'''\n")
        self.differ(gc.strip_python, "x = '# not'\n", "x = ''\n")
        self.assertIsNone(gc.strip_python("def f(:\n  (\n"))


class Verdict(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="gate-comments-")
        self.root = Path(self.tmp.name)
        self.env = dict(os.environ, GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1",
                        GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@example.invalid",
                        GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@example.invalid")
        self.git("init", "-q", "-b", "main")
        self.write("src/lib.rs", "pub fn a() -> u8 {\n    1\n}\n")
        self.write("scripts/run.sh", "#!/bin/sh\necho hi\n")
        self.write("tools/job.py", "print('x')\n")
        self.write("docs/note.md", "note\n")
        self.write("docs/user/page.md", "page\n")
        self.write("src/embed.rs", 'pub const P: &str = include_str!("../docs/embedded.md");\n')
        self.write("docs/embedded.md", "embedded\n")
        self.write("assets/data.json", "{}\n")
        self.git("add", "-A")
        self.git("commit", "-qm", "base")
        shutil_src = HERE / "gate-select.py"
        (self.root / "scripts/gate-select.py").write_text(shutil_src.read_text())
        self.git("add", "-A")
        self.git("commit", "-qm", "selector")

    def tearDown(self):
        self.tmp.cleanup()

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.root), *args], env=self.env, check=True,
                              stdout=subprocess.PIPE, text=True).stdout

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def verdict(self, *names):
        base = self.git("merge-base", "main", "HEAD").strip()
        return gc.verdict(self.root, base, list(names))

    def test_a_comment_in_a_script_is_the_lane(self):
        self.write("scripts/run.sh", "#!/bin/sh\n# say hello (13.11.)\necho hi # greeting\n")
        self.assertEqual(self.verdict("scripts/run.sh"), "comments")

    def test_comments_in_rust_python_and_unembedded_docs_together(self):
        self.write("src/lib.rs", "// the number\npub fn a() -> u8 {\n    1 // one\n}\n")
        self.write("tools/job.py", "# job\nprint('x')\n")
        self.write("docs/note.md", "note, reworded\n")
        self.assertEqual(self.verdict("src/lib.rs", "tools/job.py", "docs/note.md"), "comments")

    def test_one_changed_value_leaves_the_lane(self):
        self.write("src/lib.rs", "// the number\npub fn a() -> u8 {\n    2\n}\n")
        self.assertEqual(self.verdict("src/lib.rs"), "no")
        self.write("scripts/run.sh", "#!/bin/sh\n# c\necho hello\n")
        self.assertEqual(self.verdict("scripts/run.sh"), "no")

    def test_a_string_or_doc_comment_change_leaves_the_lane(self):
        self.write("src/lib.rs", 'pub fn a() -> u8 {\n    println!("new text");\n    1\n}\n')
        self.assertEqual(self.verdict("src/lib.rs"), "no")
        self.write("src/lib.rs", "/// now documented\npub fn a() -> u8 {\n    1\n}\n")
        self.assertEqual(self.verdict("src/lib.rs"), "no")

    def test_embedded_or_manual_documents_leave_the_lane(self):
        self.write("scripts/run.sh", "#!/bin/sh\n# c\necho hi\n")
        self.write("docs/user/page.md", "page, changed\n")
        self.assertEqual(self.verdict("scripts/run.sh", "docs/user/page.md"), "no")
        self.git("checkout", "--", "docs/user/page.md")
        self.write("docs/embedded.md", "embedded, changed\n")
        self.assertEqual(self.verdict("scripts/run.sh", "docs/embedded.md"), "no")

    def test_other_files_new_deleted_and_unchanged_files_leave_the_lane(self):
        self.write("scripts/run.sh", "#!/bin/sh\n# c\necho hi\n")
        self.write("assets/data.json", '{"a": 1}\n')
        self.assertEqual(self.verdict("scripts/run.sh", "assets/data.json"), "no")
        self.git("checkout", "--", "assets/data.json")
        self.write("scripts/new.sh", "# only a comment\n")
        self.assertEqual(self.verdict("scripts/run.sh", "scripts/new.sh"), "no")
        (self.root / "scripts/run.sh").unlink()
        self.assertEqual(self.verdict("scripts/run.sh"), "no")
        self.git("checkout", "--", "scripts/run.sh")
        self.assertEqual(self.verdict("scripts/run.sh"), "no", "a listed file that did not change is no proof")

    def test_symlinks_documents_alone_and_empty_lists_are_no_lane(self):
        self.assertEqual(self.verdict(), "no")
        self.write("docs/note.md", "reworded\n")
        self.assertEqual(self.verdict("docs/note.md"), "no", "documents alone are the short path")
        (self.root / "scripts/run.sh").unlink()
        os.symlink("/etc/passwd", self.root / "scripts/run.sh")
        self.assertEqual(self.verdict("scripts/run.sh"), "no")

    def test_command_line_prints_no_when_anything_goes_wrong(self):
        out = subprocess.run(["python3", str(HERE / "gate-comments.py"), str(self.root), "no-such-ref"], input="scripts/run.sh\n",
                             stdout=subprocess.PIPE, text=True).stdout.strip()
        self.assertEqual(out, "no")
        self.write("scripts/run.sh", "#!/bin/sh\n# c\necho hi\n")
        out = subprocess.run(["python3", str(HERE / "gate-comments.py"), str(self.root), "main"], input="scripts/run.sh\n",
                             stdout=subprocess.PIPE, text=True).stdout.strip()
        self.assertEqual(out, "comments")


if __name__ == "__main__":
    unittest.main()
