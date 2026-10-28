//! Helpers shared by the unit tests.

use std::io::Write;
use std::ops::Deref;
use std::path::Path;
use std::process::{Command, Stdio};

/// A scratch folder for one test: it exists as long as the `TestDir` does and is removed, with everything in it, when
/// it is dropped, also when the test panics. Use it instead of `std::env::temp_dir().join(..)` + `create_dir_all`, which
/// left a folder in /tmp on every run (tens of thousands on a build machine).
///
/// It derefs to [`Path`]: `&dir` goes where a `&Path` is wanted, `dir.join("x")` works. For an owned `PathBuf` (a
/// field, a call that takes one) use `dir.to_path_buf()`. The unique name is `pocketui-<label>-XXXXXX`; the label only
/// tells a leftover (after a kill or abort) which test it came from. Never `keep()` it.
pub(crate) struct TestDir(tempfile::TempDir);

impl TestDir {
    pub(crate) fn new(label: &str) -> TestDir {
        let dir = tempfile::Builder::new().prefix(&format!("pocketui-{label}-")).tempdir().expect("a scratch folder in the temp dir");
        TestDir(dir)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        // Background writers of the code under test (the gamelist cache) make their folder again when they are late.
        crate::gamelist::join_writers();
    }
}

impl Deref for TestDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        self.0.path()
    }
}

impl From<&TestDir> for std::path::PathBuf {
    fn from(dir: &TestDir) -> std::path::PathBuf {
        dir.0.path().to_path_buf()
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.0.path()
    }
}

/// Writes `contents` to `path` as an executable (mode 0755), for fake programs (`curl`, `bluetoothctl`, ROCKNIX's
/// scripts ...) that the code under test then runs. Use it instead of `std::fs::write` + `set_permissions`.
///
/// Why: Linux refuses to exec a file that is open for writing anywhere (ETXTBSY, "Text file busy"). The tests run
/// on many threads, and many of them start processes. If one thread forks while another holds the script's write
/// descriptor, the forked child gets a copy of that descriptor and keeps it until it execs (close-on-exec only
/// takes effect there), so for that moment *any* exec of the script fails, even though our own thread closed the
/// file long ago. Under load the moment gets long enough to hit now and then (tests failing with "Text file busy",
/// or code that treats a failed start as "off" quietly doing nothing). Retrying the exec only papers over it.
///
/// So the file is written by a child process (`sh`, contents on its stdin): no descriptor that can write to the
/// script ever exists in this process, so no fork of ours can carry one, and once the child has exited no one
/// holds the file open. The pipe to the child is close-on-exec and isn't the script, so it can't cause the error.
pub(crate) fn write_executable(path: &Path, contents: impl AsRef<[u8]>) {
    let mut child = Command::new("sh")
        .args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("sh to write {}: {e}", path.display()));
    // Taken out of `child` so that it is closed (end of input for `cat`) before the wait.
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(contents.as_ref()).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success(), "writing {} failed: {status}", path.display());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn writes_a_program_that_runs_and_replaces_an_older_one() {
        let dir = TestDir::new("testutil");
        let prog = dir.join("prog");
        write_executable(&prog, "#!/bin/sh\necho first\n");
        write_executable(&prog, format!("#!/bin/sh\necho {}\n", "second"));
        assert_eq!(std::fs::metadata(&prog).unwrap().permissions().mode() & 0o7777, 0o755);
        let out = Command::new(&prog).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "second\n");
    }

    #[test]
    fn a_test_dir_is_unique_exists_and_goes_with_its_contents_also_when_the_test_panics() {
        let (a, b) = (TestDir::new("same"), TestDir::new("same"));
        assert_ne!(a.to_path_buf(), b.to_path_buf());
        assert!(a.is_dir() && a.file_name().unwrap().to_string_lossy().starts_with("pocketui-same-"));
        std::fs::create_dir_all(a.join("deep/er")).unwrap();
        std::fs::write(a.join("deep/er/file"), b"x").unwrap();
        let gone = a.to_path_buf();
        drop(a);
        assert!(!gone.exists(), "removed with what was in it");
        // The folder of a test that panics goes too: the value unwinds with the closure.
        let seen = std::sync::Mutex::new(None);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let dir = TestDir::new("panics");
            std::fs::write(dir.join("file"), b"x").unwrap();
            *seen.lock().unwrap() = Some(dir.to_path_buf());
            panic!("the test fails");
        }));
        assert!(result.is_err());
        let path = seen.lock().unwrap().take().unwrap();
        assert!(!path.exists(), "removed although the test panicked");
    }
}
