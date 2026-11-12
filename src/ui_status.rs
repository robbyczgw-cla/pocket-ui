//! The status file: one line saying where the UI is, for agents, test helpers and support (docs/DEVICE-TESTS.md "Where the
//! UI is"). `/tmp/pocket-ui.status` (tmpfs, never the card), `POCKETUI_STATUS` for a probe, or `<dir>/ui.status` under `--test-driver`.
//!
//! The line is TAB-separated `key=value` fields: first `v` (the schema, `VERSION`), `rev` (one more with every write, from 1
//! in each process) and `pid`, then the fields the App gives (`place`, `cover`, `sleep`, `game`, later `update`…). Values
//! are machine codes, never translated text and never secrets (no network names, keys, addresses or game names). A reader
//! ignores keys it doesn't know. The file is replaced whole (a temporary file renamed over it, no `fsync`) and only when a
//! field changed: an unchanged screen never writes.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// The schema of the line: a new number when a field changes its meaning or goes away (new fields keep it).
pub const VERSION: u32 = 1;
/// Where a pocket-ui on a device writes it.
pub const FILE: &str = "/tmp/pocket-ui.status";

/// The App's fields, in order.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Line {
    fields: Vec<(&'static str, String)>,
}

/// A key: lowercase letters, digits and `.` `_` `-` (`update.version`).
fn key_ok(key: &str) -> bool {
    !key.is_empty() && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'))
}

impl Line {
    pub fn new() -> Line {
        Line::default()
    }

    /// Adds `key=value`. A TAB or line break in the value becomes a space, so the line always reads back.
    pub fn field(&mut self, key: &'static str, value: impl Into<String>) {
        debug_assert!(key_ok(key) && !matches!(key, "v" | "rev" | "pid"), "status key {key:?}");
        let value: String = value.into().chars().map(|c| if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c }).collect();
        self.fields.push((key, value));
    }

    /// The fields without the head: what decides whether the file is written again.
    fn body(&self) -> String {
        self.fields.iter().map(|(k, v)| format!("\t{k}={v}")).collect()
    }

    /// The whole line, with the head.
    pub fn render(&self, rev: u64, pid: u32) -> String {
        format!("v={VERSION}\trev={rev}\tpid={pid}{}\n", self.body())
    }
}

/// The fields of a line as written, in order (for tests and helpers; `scripts/ui-status.sh` reads it with awk).
pub fn parse(line: &str) -> Vec<(String, String)> {
    line.trim_end_matches('\n')
        .split('\t')
        .filter_map(|f| f.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The UI status destination; the test driver otherwise keeps it beside its FIFO.
pub fn output_path(driver: Option<&Path>, requested: Option<OsString>) -> PathBuf {
    requested.filter(|p| !p.is_empty()).map(PathBuf::from)
        .unwrap_or_else(|| driver.map_or_else(|| PathBuf::from(FILE), |d| d.join("ui.status")))
}

/// Writes the file when the line changed.
pub struct Writer {
    path: PathBuf,
    last: Option<String>,
    rev: u64,
}

impl Writer {
    pub fn new(path: PathBuf) -> Writer {
        Writer { path, last: None, rev: 0 }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes `line` when it differs from the last one written; true when it wrote. A failed write is tried again with
    /// the next offer.
    pub fn offer(&mut self, line: &Line) -> io::Result<bool> {
        let body = line.body();
        if self.last.as_deref() == Some(body.as_str()) {
            return Ok(false);
        }
        let rev = self.rev + 1;
        crate::fsutil::replace_file(&self.path, line.render(rev, std::process::id()).as_bytes())?;
        self.rev = rev;
        self.last = Some(body);
        Ok(true)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn line(place: &str) -> Line {
        let mut l = Line::new();
        l.field("place", place);
        l.field("cover", "none");
        l
    }

    #[test]
    fn a_line_starts_with_its_schema_revision_and_pid_and_reads_back() {
        let text = line("settings:List Look").render(7, 42);
        assert!(text.starts_with("v=1\trev=7\tpid=42\t") && text.ends_with('\n'), "{text:?}");
        let fields = parse(&text);
        assert_eq!(fields[3], ("place".into(), "settings:List Look".into()));
        assert_eq!(fields.len(), 5);
    }

    #[test]
    fn a_tab_or_line_break_in_a_value_cannot_break_the_line() {
        let mut l = Line::new();
        l.field("place", "a\tb\nc");
        assert_eq!(parse(&l.render(1, 1))[3].1, "a b c");
    }

    #[test]
    fn the_file_is_written_only_when_a_field_changes_and_each_write_counts_up() {
        let dir = crate::testutil::TestDir::new("ui-status");
        let mut w = Writer::new(dir.join("ui.status"));
        assert!(w.offer(&line("shelf:Home")).unwrap());
        assert!(!w.offer(&line("shelf:Home")).unwrap(), "same line: no write");
        assert!(w.offer(&line("settings:Groups")).unwrap());
        let text = std::fs::read_to_string(w.path()).unwrap();
        assert!(text.contains("\trev=2\t") && text.contains("place=settings:Groups"), "{text}");
    }

    #[test]
    fn a_probe_status_override_never_replaces_the_owner_status() {
        let dir = crate::testutil::TestDir::new("probe-status");
        let owner = dir.join("ui.status");
        let private = dir.join("probe.status");
        std::fs::write(&owner, "owner status\n").unwrap();
        let mut probe = Writer::new(output_path(Some(&dir.join("")), Some(private.clone().into_os_string())));
        probe.offer(&line("settings:Groups")).unwrap();
        assert_eq!(std::fs::read_to_string(&owner).unwrap(), "owner status\n", "probe replaced the owner's status");
        assert!(std::fs::read_to_string(&private).unwrap().contains("place=settings:Groups"));
        assert_eq!(output_path(None, Some(private.clone().into_os_string())), private);
        assert_eq!(output_path(None, None), PathBuf::from(FILE));
        assert_eq!(output_path(None, Some(OsString::new())), PathBuf::from(FILE));
    }

    #[test]
    fn keys_are_machine_codes() {
        assert!(key_ok("update.version") && key_ok("place") && key_ok("game"));
        assert!(!key_ok("Place") && !key_ok("a b") && !key_ok("") && !key_ok("x=y"));
    }
}
