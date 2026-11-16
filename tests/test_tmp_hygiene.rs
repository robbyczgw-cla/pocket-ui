//! Tests leave nothing in the shared temp folder.
//!
//! 14.11.2026 the merge trains went red ("write png: Disk quota exceeded") because /tmp of the build machine was full of
//! thousands of folders the tests had made and never removed (`pui-rigs-<name>-<pid>`, `pui-sticker-…`, `pocketui-…`): a
//! folder named by hand from the process id is not removed when the test fails, and every run adds a new one.
//!
//! A test that needs a folder takes it from `testutil::TestDir::new("label")` (unit tests) or `tempfile::Builder` /
//! `tempfile::tempdir()` (`tests/`), kept alive as long as the test uses it (docs/dev/tests.md). This test reads the sources
//! and fails on the two ways that leave one behind: `temp_dir().join(..process::id()..)` in test code and a `tempdir()`
//! that is `keep()`-ed. A line that has to (a folder that must outlive the test on purpose, with a reason) says
//! `test-tmp-ok` in a comment on it. The gate also runs every test with its own TMPDIR that is removed at the end
//! (scripts/run-tmp.sh), so a leak that gets past this still can't pile up; it is named in the gate output.

use std::path::{Path, PathBuf};

struct Finding {
    line: usize,
    text: String,
    why: &'static str,
}

/// The findings in the test code of one source file. `whole_file`: the file is a test file (everything counts); else only the
/// `#[cfg(test)] mod .. {` blocks (each ends at the next line that is just `}`).
fn findings(source: &str, whole_file: bool) -> Vec<Finding> {
    let lines: Vec<&str> = source.lines().collect();
    let mut in_test = whole_file;
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !whole_file {
            if !in_test {
                // A `#[cfg(test)]` on a single item or on `mod x;` is no test block.
                let opens = line.trim_start().starts_with("#[cfg(test)]")
                    && lines[i + 1..].iter().map(|l| l.trim_start()).find(|l| !l.starts_with("#[")).is_some_and(|l| (l.starts_with("mod ") || l.starts_with("pub mod ")) && l.trim_end().ends_with('{'));
                in_test = opens;
                continue;
            }
            if *line == "}" {
                in_test = false;
                continue;
            }
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || line.contains("test-tmp-ok") || (i > 0 && lines[i - 1].contains("test-tmp-ok")) {
            continue;
        }
        // The statement may continue on the next line (`format!("..", std::process::id(),\n now)`).
        let two = format!("{line} {}", lines.get(i + 1).unwrap_or(&""));
        if line.contains("temp_dir()") && two.contains("process::id()") {
            found.push(Finding { line: i + 1, text: line.trim().to_string(), why: "a folder in the temp dir named by hand from the process id stays behind when the test fails or is killed" });
        }
        if line.contains("tempdir()") && (line.contains(".keep()") || line.contains(".into_path()")) {
            found.push(Finding { line: i + 1, text: line.trim().to_string(), why: "a kept tempdir is never removed" });
        }
        if line.contains("mem::forget") && two.contains("temp") {
            found.push(Finding { line: i + 1, text: line.trim().to_string(), why: "a forgotten temp folder is never removed" });
        }
    }
    found
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !matches!(path.file_name().and_then(|n| n.to_str()), Some("target" | "vendor" | ".git")) {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The red-before part: what the old tests did is found, what they do now is not.
#[test]
fn the_check_finds_what_left_folders_in_tmp() {
    let old_world = r#"
        let tmp = std::env::temp_dir().join(format!("pui-rigs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
    "#;
    assert_eq!(findings(old_world, true).len(), 1, "the pid-named folder of tests/insert_rigs.rs");
    let two_lines = "let dir = std::env::temp_dir().join(format!(\"h700-volume-{}-{}\", std::process::id(),\n    nanos));";
    assert_eq!(findings(two_lines, true).len(), 1, "a name that goes on to the next line");
    let kept = r#"let dir = tempfile::Builder::new().prefix("pocketui-exit-guard-").tempdir().unwrap().keep();"#;
    assert_eq!(findings(kept, true).len(), 1, "a kept tempdir");
    let unit = "fn product() { let p = std::env::temp_dir().join(format!(\"x-{}\", std::process::id())); }\n#[cfg(test)]\nfn helper() {}\nfn product2() { let q = std::env::temp_dir().join(format!(\"z-{}\", std::process::id())); }\n#[cfg(test)]\nmod tests {\n fn t() { let d = std::env::temp_dir().join(format!(\"y-{}\", std::process::id())); }\n}\nfn product3() { let r = std::env::temp_dir().join(format!(\"w-{}\", std::process::id())); }";
    assert_eq!(findings(unit, false).len(), 1, "in a source file only the test part counts (the program may name its own files)");
    let good = r#"
        let dir = tempfile::Builder::new().prefix("pui-rigs-").tempdir().unwrap();
        let ok = std::env::temp_dir().join("fixed-name-is-bounded"); // a fixed name does not pile up
        let kept = tempfile::tempdir().unwrap().keep(); // test-tmp-ok: removed at exit
    "#;
    assert!(findings(good, true).is_empty(), "{:?}", findings(good, true).iter().map(|f| &f.text).collect::<Vec<_>>());
}

#[test]
fn no_test_makes_a_folder_in_tmp_that_it_does_not_remove() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string()));
    let mut files = Vec::new();
    rust_files(&root.join("tests"), &mut files);
    rust_files(&root.join("src"), &mut files);
    rust_files(&root.join("examples"), &mut files);
    let mut problems = Vec::new();
    for file in files {
        let rel = file.strip_prefix(&root).unwrap_or(&file).display().to_string();
        if rel == "tests/test_tmp_hygiene.rs" {
            continue; // its own examples of what is wrong
        }
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let whole = rel.starts_with("tests/") && !rel.starts_with("tests/common/") || name == "tests.rs" || name.ends_with("_tests.rs") || name == "testutil.rs";
        let Ok(source) = std::fs::read_to_string(&file) else { continue };
        for f in findings(&source, whole) {
            problems.push(format!("{rel}:{}: {}\n    {}", f.line, f.text, f.why));
        }
    }
    assert!(
        problems.is_empty(),
        "Tests that leave folders behind (use `testutil::TestDir::new(\"label\")` in unit tests, `tempfile::Builder::new().prefix(\"label-\").tempdir()` in tests/, \
         held for as long as the test uses the folder; docs/dev/tests.md):\n{}",
        problems.join("\n")
    );
}
