//! What a crash leaves behind. pocket-ui and pocket-core are built with `panic = "abort"`, so a panic ends
//! the process, but the panic hook still runs first: it writes a timestamped report to the log, appends it
//! to `crash.log` and leaves a notice for the next screen (the internal review, K5).
//!
//! * pocket-ui: `<data dir>/crash.marker`, which the next start shows on the shelf (`take_marker`).
//! * pocket-core: `/tmp/pocket-core.error`, which pocket-ui already shows when the game ends (`notice`).

use std::io::Write;
use std::path::{Path, PathBuf};

/// Appended to on every crash, trimmed when it grows.
pub const LOG_FILE: &str = "crash.log";
/// Left by a crash of pocket-ui, taken (and removed) by the next start.
pub const MARKER_FILE: &str = "crash.marker";
/// `crash.log` is cut back to about half of this when it grows past it.
const LOG_LIMIT: usize = 64 * 1024;
/// "2026-10-22 14:03:07 UTC" for a Unix time.
pub fn timestamp(unix_secs: u64) -> String {
    let (days, rest) = ((unix_secs / 86_400) as i64, unix_secs % 86_400);
    // Days since 1970-01-01 to a calendar date (Howard Hinnant's civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + (month <= 2) as i64;
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// One line: when, who, what, where.
pub fn report(program: &str, unix_secs: u64, message: &str, location: &str) -> String {
    format!("{} {program} panicked at {location}: {}", timestamp(unix_secs), message.replace('\n', " | "))
}

/// The notice on screen: one plain sentence. The panic message and where it happened are for crash.log; on
/// the shelf they were a Rust error that ran off the screen.
fn notice_text(program: &str) -> String {
    match program {
        "pocket-ui" => "pocket-ui stopped unexpectedly and started again.".into(),
        _ => "The game stopped unexpectedly.".into(),
    }
}

/// The notice for a boot that ended without a clean shutdown (`logs::begin_boot`): the device froze and the
/// watchdog restarted it, the power went, or it was switched off by force. One plain sentence like the others.
const UNCLEAN_NOTICE: &str = "The device shut down unexpectedly last time.";

/// "<now> pocket-ui: previous boot ended without a clean shutdown at <last write>; last kernel lines: a | b | c".
/// One line, like every report in `crash.log`.
pub fn unclean_report(unix_secs: u64, last_alive: u64, kernel_tail: &[String], sensors_last: Option<&str>) -> String {
    let mut line = format!("{} pocket-ui: previous boot ended without a clean shutdown at {}; last kernel lines: ", timestamp(unix_secs), timestamp(last_alive));
    line.push_str(&if kernel_tail.is_empty() { "(none)".to_string() } else { kernel_tail.join(" | ") });
    if let Some(s) = sensors_last {
        line.push_str("; last sensors: ");
        line.push_str(s);
    }
    line.replace('\n', " | ")
}

/// Writes the report of a boot that ended without a clean shutdown to `crash.log` in `dir` and leaves the marker
/// for pocket-ui's next screen. Never fails, never panics.
pub fn record_unclean(dir: &Path, unix_secs: u64, last_alive: u64, kernel_tail: &[String], sensors_last: Option<&str>) {
    append_log(dir, &unclean_report(unix_secs, last_alive, kernel_tail, sensors_last));
    let _ = std::fs::write(dir.join(MARKER_FILE), UNCLEAN_NOTICE);
}

/// Appends one line to `crash.log`, trimming the file when it has grown.
fn append_log(dir: &Path, line: &str) {
    let _ = std::fs::create_dir_all(dir);
    let log = dir.join(LOG_FILE);
    if std::fs::metadata(&log).is_ok_and(|m| m.len() as usize > LOG_LIMIT) {
        trim(&log);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
        let _ = writeln!(f, "{line}");
    }
}

/// Writes the report to `crash.log` in `dir`. A crash of pocket-core (`error_file` given) also leaves its
/// reason in that file for the shelf; a crash of pocket-ui leaves the marker. Never fails, never panics.
pub fn record(dir: &Path, error_file: Option<&Path>, program: &str, unix_secs: u64, message: &str, location: &str) {
    append_log(dir, &report(program, unix_secs, message, location));
    let notice = notice_text(program);
    match error_file {
        Some(file) => {
            let _ = std::fs::write(file, notice);
        }
        None => {
            let _ = std::fs::write(dir.join(MARKER_FILE), notice);
        }
    }
}

/// Keeps the newer half of the log, from a line start.
fn trim(log: &Path) {
    let Ok(text) = std::fs::read_to_string(log) else { return };
    let mut at = text.len().saturating_sub(LOG_LIMIT / 2);
    while !text.is_char_boundary(at) {
        at += 1;
    }
    let keep = text[at..].split_once('\n').map_or("", |(_, rest)| rest);
    let _ = std::fs::write(log, keep);
}

/// Takes the marker a crash of pocket-ui left, if any: the text for a notice.
pub fn take_marker(dir: &Path) -> Option<String> {
    let file = dir.join(MARKER_FILE);
    let text = std::fs::read_to_string(&file).ok()?;
    let _ = std::fs::remove_file(&file);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Installs the panic hook for `program`. The report goes to stderr (pocket-ui's log) and `dir`;
/// `error_file` is pocket-core's reason-for-the-shelf file. The default hook runs afterwards.
pub fn install(program: &'static str, dir: PathBuf, error_file: Option<PathBuf>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = match info.payload().downcast_ref::<&str>() {
            Some(s) => (*s).to_string(),
            None => info.payload().downcast_ref::<String>().cloned().unwrap_or_else(|| "(no message)".into()),
        };
        let location = info
            .location()
            .map_or_else(|| "unknown location".to_string(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        eprintln!("{}", report(program, now, &message, &location));
        record(&dir, error_file.as_deref(), program, now, &message, &location);
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn tmp(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("crash-{name}"))
    }

    #[test]
    fn timestamps_are_utc_calendar_time() {
        assert_eq!(timestamp(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(timestamp(1_791_900_000), "2026-10-13 14:00:00 UTC");
        // a leap day
        assert_eq!(timestamp(1_709_164_800), "2024-02-29 00:00:00 UTC");
    }

    #[test]
    fn a_report_says_when_who_what_and_where() {
        let r = report("pocket-ui", 0, "buffer: lost", "src/main.rs:1674:30");
        assert!(r.contains("1970-01-01 00:00:00 UTC"), "{r}");
        assert!(r.contains("pocket-ui"), "{r}");
        assert!(r.contains("buffer: lost"), "{r}");
        assert!(r.contains("src/main.rs:1674:30"), "{r}");
        assert_eq!(report("p", 0, "a\nb", "l").lines().count(), 1, "one line per crash");
    }

    #[test]
    fn a_crash_of_pocket_core_leaves_a_log_and_its_error_file() {
        let dir = tmp("core");
        let err = dir.join("pocket-core.error");
        record(
            &dir,
            Some(&err),
            "pocket-core",
            1_791_900_000,
            "window surface: lost",
            "src/bin/pocket_core.rs:208:5",
        );
        record(&dir, Some(&err), "pocket-core", 1_791_900_060, "second", "x.rs:1:1");
        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(log.contains("window surface: lost") && log.contains("second"), "{log}");
        assert_eq!(log.lines().count(), 2);
        let shown = std::fs::read_to_string(&err).unwrap();
        assert_eq!(shown, "The game stopped unexpectedly.", "no Rust error text on the shelf");
        assert!(take_marker(&dir).is_none(), "pocket-core's notice is the error file, not the marker");
    }

    #[test]
    fn a_crash_of_pocket_ui_leaves_a_marker_that_shows_once() {
        let dir = tmp("ui");
        record(&dir, None, "pocket-ui", 1_791_900_000, "buffer: lost", "src/main.rs:1674:30");
        assert!(dir.join(LOG_FILE).exists());
        let text = take_marker(&dir).expect("marker");
        assert_eq!(text, "pocket-ui stopped unexpectedly and started again.", "a plain sentence for the player");
        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(log.contains("buffer: lost") && log.contains("main.rs"), "the details are in crash.log: {log}");
        assert!(take_marker(&dir).is_none(), "the marker shows once");
        assert!(dir.join(LOG_FILE).exists(), "the log stays");
    }

    #[test]
    fn the_log_does_not_grow_without_bound() {
        let dir = tmp("trim");
        for i in 0..2000 {
            record(&dir, None, "pocket-ui", i, &"x".repeat(200), "a.rs:1:1");
        }
        let size = std::fs::metadata(dir.join(LOG_FILE)).unwrap().len();
        assert!(size < 200_000, "{size}");
        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(log.contains("1970-01-01 00:33:19 UTC"), "the newest report is kept");
        assert!(log.lines().all(|l| l.starts_with("1970-")), "cut at a line start");
    }

    #[test]
    fn the_installed_hook_records_a_real_panic() {
        let dir = tmp("hook");
        install("pocket-ui", dir.to_path_buf(), None);
        let _ = std::panic::catch_unwind(|| panic!("the hook test panic"));
        let _ = std::panic::take_hook();
        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(log.contains("the hook test panic") && log.contains("crash.rs"), "{log}");
        assert!(take_marker(&dir).is_some_and(|t| !t.contains("the hook test panic")), "the details stay in the log");
    }

    #[test]
    fn a_boot_that_ended_unclean_leaves_one_line_and_a_plain_notice() {
        let dir = tmp("unclean");
        let tail = vec!["[ 1.000000] err    first".to_string(), "[ 2.000000] warn   second".to_string()];
        record_unclean(&dir, 1_791_990_000, 1_791_900_000, &tail, Some("2026-10-13 14:00:00 UTC up=5s temp=61.3C@z6"));
        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert_eq!(log.lines().count(), 1, "{log}");
        assert!(log.starts_with("2026-10-14 15:00:00 UTC pocket-ui: previous boot ended without a clean shutdown at 2026-10-13 14:00:00 UTC; last kernel lines: "), "{log}");
        assert!(log.contains("first | [ 2.000000] warn   second; last sensors: 2026-10-13 14:00:00 UTC up=5s"), "{log}");
        let notice = take_marker(&dir).expect("the shelf shows the usual crash notice");
        assert_eq!(notice, "The device shut down unexpectedly last time.");
        assert!(!notice.contains("kernel") && notice.ends_with('.') && !notice.contains('\n'), "a plain sentence: {notice}");
        // appended to what a crash left, not instead of it
        record(&dir, None, "pocket-ui", 1_791_990_100, "later", "a.rs:1:1");
        assert_eq!(std::fs::read_to_string(dir.join(LOG_FILE)).unwrap().lines().count(), 2);
    }

    #[test]
    fn an_unclean_report_without_kernel_lines_says_so() {
        let line = unclean_report(0, 0, &[], None);
        assert!(line.ends_with("last kernel lines: (none)"), "{line}");
    }

    #[test]
    fn a_crash_that_cannot_be_written_does_not_panic_again() {
        record(
            Path::new("/proc/definitely/not/here"),
            Some(Path::new("/proc/nope/err")),
            "pocket-ui",
            0,
            "m",
            "l",
        );
        record(Path::new("/proc/definitely/not/here"), None, "pocket-ui", 0, "m", "l");
        assert!(take_marker(Path::new("/proc/definitely/not/here")).is_none());
    }
}
