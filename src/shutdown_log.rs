//! The shutdown path's own log (`<data dir>/logs/shutdown.log`, 2026-10-26: a "Power off" that rebooted left nothing
//! to read). Every power off and restart writes, and fsyncs line by line, what it did: which action and why, the
//! RTC alarm read back (`auto_off::Rtc::disarm_for_shutdown`), the command it runs and how that ended, plus the last
//! lines of pocket-ui's own log (still only in RAM, the logger copies it a second later) and of the kernel's.
//!
//! The file is one of the logger's streams (`logs::STREAMS`): it moves into `boot-<n>-…/` with the other logs when the
//! next boot starts, the newest [`logs::KEEP_BOOTS`] boots stay, and Report a problem sends the current one and the
//! previous boot's (`report.rs`). Size: [`CAP`] for a boot's file and its `.old`, 3 boots = 384 KB.

use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::logs;

/// The most one boot's `shutdown.log` and its `.old` take together (three boots: 384 KB, far below 2 MB).
pub const CAP: u64 = 128 * 1024;
const _: () = assert!(3 * CAP < 2 * 1024 * 1024);
/// Lines of pocket-ui's log and of the kernel's kept with a power off.
const UI_LINES: usize = 40;
const KERNEL_LINES: usize = 25;

/// Where the data folder is (the same default as `main`).
pub fn default_dir() -> PathBuf {
    PathBuf::from(std::env::var("POCKETUI_DATA").unwrap_or_else(|_| "/storage/.config/pocket-ui".into()))
}

fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(logs::DIR_NAME).join(logs::SHUTDOWN_LOG)
}

/// One line, with the uptime and the clock, on the card before this returns. Failing to write costs the line, never
/// the power off.
pub fn event(data_dir: &Path, text: &str) {
    append(data_dir, &format!("{} {text}\n", stamp()));
}

fn stamp() -> String {
    let uptime = logs::read_uptime(Path::new("/proc/uptime"));
    format!("[{uptime:>9.3}] {}", crate::crash::timestamp(crate::recents::now_secs()))
}

fn append(data_dir: &Path, text: &str) {
    // One writer at a time: the rtc-check thread and a shutdown thread, and two rolls of the file never overlap.
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let file = path(data_dir);
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut log = logs::Rolling::new(file, CAP);
    // A single entry never takes more than a segment, so the cap holds however much is asked for.
    let max = (CAP / 2) as usize;
    if text.len() > max {
        let mut cut = max;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        log.append(&format!("{}\n", &text[..cut]));
    } else {
        log.append(text);
    }
    log.sync();
}

/// `event`, and the last lines of pocket-ui's log (`POCKETUI_LOG`) and of the kernel's ring buffer under it: what
/// was going on when the Pocket went down.
pub fn snapshot(data_dir: &Path, text: &str) {
    let ui_log = std::env::var_os("POCKETUI_LOG").map_or_else(|| PathBuf::from("/var/log/pocket-ui.log"), PathBuf::from);
    snapshot_from(data_dir, text, &ui_log, Path::new("/dev/kmsg"));
}

fn snapshot_from(data_dir: &Path, text: &str, ui_log: &Path, kmsg: &Path) {
    let mut out = format!("{} {text}\n", stamp());
    let ui = match (ui_log.parent(), ui_log.file_name().and_then(|n| n.to_str())) {
        (Some(dir), Some(name)) => logs::tail_lines(dir, name, UI_LINES),
        _ => Vec::new(),
    };
    out.push_str(&format!("  --- pocket-ui log, last {} lines ---\n", ui.len()));
    for l in &ui {
        out.push_str(&format!("  | {l}\n"));
    }
    let kernel = kernel_tail(kmsg, KERNEL_LINES);
    out.push_str(&format!("  --- kernel log, last {} lines ---\n", kernel.len()));
    for l in &kernel {
        out.push_str(&format!("  | {l}\n"));
    }
    append(data_dir, &out);
}

/// The last `n` records of the kernel's ring buffer (`/dev/kmsg` read without waiting), formatted like dmesg.
fn kernel_tail(kmsg: &Path, n: usize) -> Vec<String> {
    let Ok(mut f) = OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(kmsg) else { return Vec::new() };
    let mut lines: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut buf = [0u8; 8192];
    for _ in 0..4096 {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(len) => {
                for line in String::from_utf8_lossy(&buf[..len]).lines() {
                    if line.is_empty() || line.starts_with(' ') {
                        continue;
                    }
                    if let Some(m) = logs::parse_kmsg(line) {
                        lines.push_back(logs::format_kmsg(&m).trim_end().to_string());
                        if lines.len() > n {
                            lines.pop_front();
                        }
                    }
                }
            }
            // Records overwritten while reading: the next read goes on with the oldest left.
            Err(e) if e.raw_os_error() == Some(libc::EPIPE) => continue,
            Err(_) => break,
        }
    }
    lines.into()
}

/// How a command ended, for the log: "exit 0", "exit 1", "killed by signal 15", "not started: …".
pub fn run_and_describe(cmd: &str) -> (bool, String) {
    match std::process::Command::new("sh").arg("-c").arg(cmd).status() {
        Ok(s) if s.success() => (true, "exit 0".into()),
        Ok(s) => match s.code() {
            Some(c) => (false, format!("exit {c}")),
            None => (false, format!("{s}")),
        },
        Err(e) => (false, format!("not started: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> crate::testutil::TestDir {
        crate::testutil::TestDir::new(&format!("shutdownlog-{name}"))
    }

    #[test]
    fn an_event_is_a_stamped_line_in_the_logs_folder() {
        let data = tmp("event");
        event(&data, "PowerOff asked");
        event(&data, "systemctl poweroff: exit 0");
        let text = std::fs::read_to_string(path(&data)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].starts_with('[') && lines[0].ends_with("PowerOff asked") && lines[0].contains(" UTC "), "{text}");
        assert!(lines[1].ends_with("systemctl poweroff: exit 0"));
    }

    #[test]
    fn the_snapshot_keeps_the_last_lines_of_both_logs() {
        let data = tmp("snapshot");
        let ui = data.join("ram-pocket-ui.log");
        let lines: Vec<String> = (0..100).map(|i| format!("[{i}.000] ui line {i}")).collect();
        std::fs::write(&ui, lines.join("\n") + "\n").unwrap();
        let kmsg = data.join("kmsg");
        let records: Vec<String> = (0..60).map(|i| format!("6,{i},{}000000,-;kernel line {i}\n SUBSYSTEM=x", 5000 + i)).collect();
        std::fs::write(&kmsg, records.join("\n") + "\n").unwrap();
        snapshot_from(&data, "PowerOff: running systemctl poweroff", &ui, &kmsg);
        let text = std::fs::read_to_string(path(&data)).unwrap();
        assert!(text.contains("PowerOff: running systemctl poweroff"), "{text}");
        assert!(text.contains("| [99.000] ui line 99") && text.contains(&format!("| [{}.000] ui line {}", 100 - UI_LINES, 100 - UI_LINES)), "{text}");
        assert!(!text.contains("ui line 10\n"), "only the last lines");
        assert!(text.contains("kernel line 59") && text.contains(&format!("kernel line {}", 60 - KERNEL_LINES)), "{text}");
        assert!(!text.contains("kernel line 3\n") && !text.contains("SUBSYSTEM"), "{text}");
    }

    #[test]
    fn missing_sources_leave_a_snapshot_with_empty_sections_not_a_failure() {
        let data = tmp("nosources");
        snapshot_from(&data, "Reboot", &data.join("no-log"), &data.join("no-kmsg"));
        let text = std::fs::read_to_string(path(&data)).unwrap();
        assert!(text.contains("last 0 lines") && text.contains("Reboot"), "{text}");
    }

    #[test]
    fn the_file_and_its_old_segment_stay_under_the_cap() {
        let data = tmp("cap");
        for i in 0..3000 {
            event(&data, &format!("event {i} {}", "x".repeat(100)));
        }
        let size = |n: &str| std::fs::metadata(path(&data).with_file_name(n)).map_or(0, |m| m.len());
        let total = size(logs::SHUTDOWN_LOG) + size(&format!("{}.old", logs::SHUTDOWN_LOG));
        assert!(total <= CAP, "{total} bytes");
        assert!(total > CAP / 4, "it keeps a good stretch: {total}");
        let text = std::fs::read_to_string(path(&data)).unwrap();
        assert!(text.contains("event 2999"), "the newest is always there");
        // One huge entry never breaks the cap either.
        event(&data, &"y".repeat(1_000_000));
        let total = size(logs::SHUTDOWN_LOG) + size(&format!("{}.old", logs::SHUTDOWN_LOG));
        assert!(total <= CAP + 1, "{total} bytes");
    }

    #[test]
    fn three_boots_stay_and_the_total_is_far_below_two_megabytes() {
        let data = tmp("boots");
        for boot in 0..5u64 {
            logs::begin_boot(&data, &format!("boot-{boot}"), 1_791_900_000 + boot * 10_000);
            for i in 0..2000 {
                event(&data, &format!("boot {boot} event {i} {}", "z".repeat(80)));
            }
            snapshot_from(&data, &format!("boot {boot} last"), &data.join("none"), &data.join("none"));
        }
        let logs_dir = data.join(logs::DIR_NAME);
        let mut dirs: Vec<String> = std::fs::read_dir(&logs_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        dirs.sort();
        assert_eq!(dirs.len(), logs::KEEP_BOOTS - 1, "{dirs:?}: two archived boots and the current one");
        assert!(dirs[0].starts_with("boot-0003-") && dirs[1].starts_with("boot-0004-"), "{dirs:?}");
        // The newest archived boot kept its own shutdown log, with its last line.
        let last = std::fs::read_to_string(logs_dir.join(&dirs[1]).join(logs::SHUTDOWN_LOG)).unwrap();
        assert!(last.contains("boot 3 last"), "{last}");
        let mut total = 0;
        for e in walk(&logs_dir) {
            total += std::fs::metadata(&e).map_or(0, |m| m.len());
        }
        assert!(total <= 3 * CAP + 4096, "{total} bytes in all");
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.path().is_dir() {
                out.extend(walk(&e.path()));
            } else {
                out.push(e.path());
            }
        }
        out
    }

    #[test]
    fn a_command_is_described_by_how_it_ended() {
        assert_eq!(run_and_describe("true"), (true, "exit 0".into()));
        assert_eq!(run_and_describe("exit 3"), (false, "exit 3".into()));
        let (ok, text) = run_and_describe("kill -9 $$");
        assert!(!ok && text.contains("signal"), "{text}");
    }
}
