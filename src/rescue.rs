//! The way back when pocket-ui won't start, and EmulationStation only when asked for (docs/ARCHITECTURE.md, "Rescue
//! path"). Three parts, all without painting (that is `app_rescue.rs` in the binary), so every path is a plain test:
//!
//! * The contract with the launcher `deploy/run-pocket-ui.sh`: pocket-ui's exit codes (a test reads the script and
//!   checks that the numbers match), the first-picture mark of its black screen guard, the note it leaves when it
//!   handed over to EmulationStation by itself, and which other version "Use previous version" goes to (the same
//!   rule as the script's `other_good` / `other_previous`).
//! * Safe mode (`--safe` or `POCKETUI_SAFE=1`): the rescue page. What happened (the launcher's reason, the last line of
//!   `crash.log`, the launcher's last lines), and Restart normally · Use previous version · Reset pocket-ui settings ·
//!   Open EmulationStation… · Power off.
//! * The dialog in front of EmulationStation (SELECT + START held, Escape, Settings → About, the rescue page), with
//!   the warning about the two things in it that harm this device.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::pad::Button;

/// Exit code: start pocket-ui again, normally (the rescue page's Restart, the dialog's Restart pocket-ui, the command
/// FIFO's `quit`, a closed window). Never EmulationStation.
pub const RESTART_CODE: i32 = 0;
/// Exit code: the player asked for EmulationStation (after the warning). The launcher hands over to it.
pub const OPEN_ES_CODE: i32 = 77;
/// Exit code: the rescue page's "Use previous version". The launcher switches `current`, then starts normally.
pub const PREVIOUS_CODE: i32 = 78;

/// The argument the launcher starts safe mode with.
pub const SAFE_ARG: &str = "--safe";
/// The same as an environment variable (`POCKETUI_SAFE=1`), for dev.env and tests.
pub const SAFE_ENV: &str = "POCKETUI_SAFE";
/// Why the launcher started safe mode: one sentence.
pub const REASON_ENV: &str = "POCKETUI_SAFE_REASON";
/// The file the launcher's black screen guard waits for: pocket-ui creates it once its first picture is on screen.
pub const FRAME_FILE_ENV: &str = "POCKETUI_FRAME_FILE";
/// In the app's root: why the launcher handed over to EmulationStation by itself; shown once at the next start.
pub const ES_NOTE_FILE: &str = "es-fallback";
/// The launcher's link to the last version that ran long enough with a picture.
pub const KNOWN_GOOD: &str = "known-good";

/// Safe mode was asked for, by the argument or the environment.
pub fn safe_mode_requested(args: &[String], env: Option<&str>) -> bool {
    args.iter().any(|a| a == SAFE_ARG) || env.is_some_and(|v| v.trim() == "1")
}

static FRAME_NOTED: AtomicBool = AtomicBool::new(false);

/// The first picture is on screen: tells the launcher's black screen guard (`POCKETUI_FRAME_FILE`). Once per process,
/// on a thread of its own: nothing for the UI thread to wait for. Nothing when the launcher didn't ask.
pub fn note_first_frame() {
    if FRAME_NOTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let Some(path) = std::env::var_os(FRAME_FILE_ENV).filter(|p| !p.is_empty()) else { return };
    std::thread::spawn(move || {
        if let Err(e) = std::fs::write(&path, b"") {
            eprintln!("rescue: couldn't note the first picture in {}: {e}", Path::new(&path).display());
        }
    });
}

/// The note the launcher left when it handed the screen to EmulationStation by itself, as a sentence for the notice;
/// removed, so it is shown once.
pub fn take_es_note(root: &Path) -> Option<String> {
    let file = root.join(ES_NOTE_FILE);
    let text = std::fs::read_to_string(&file).ok()?;
    let _ = std::fs::remove_file(&file);
    let why = text.lines().next().unwrap_or_default().trim().trim_end_matches('.').to_string();
    Some(if why.is_empty() {
        "EmulationStation opened last time because pocket-ui couldn't start.".to_string()
    } else {
        format!("EmulationStation opened last time: {why}.")
    })
}

/// The release a link of the app's root points at ("releases/1.2.3"), if it is a link.
fn link(root: &Path, name: &str) -> Option<String> {
    std::fs::read_link(root.join(name)).ok().map(|p| p.to_string_lossy().into_owned())
}

/// The version "Use previous version" switches to: `known-good` if it isn't `current` and has a pocket-ui, else
/// `previous` on the same terms (run-pocket-ui.sh, `other_good` and `other_previous`). Its name ("1.2.3", "dev").
pub fn other_version(root: &Path) -> Option<String> {
    let current = link(root, "current")?;
    [KNOWN_GOOD, "previous"]
        .iter()
        .filter_map(|name| link(root, name))
        .find(|target| *target != current && is_executable(&root.join(target).join("pocket-ui")))
        .map(|target| target.trim_start_matches("releases/").to_string())
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// What the rescue page says happened.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WhatHappened {
    /// The launcher's sentence (`POCKETUI_SAFE_REASON`); None when safe mode was started by hand.
    pub reason: Option<String>,
    /// The last line of `crash.log`.
    pub last_crash: Option<String>,
    /// The launcher's last lines in its log, oldest first.
    pub launcher: Vec<String>,
}

/// How many of the launcher's lines the page shows.
const LAUNCHER_LINES: usize = 3;
/// Only the end of a log is read.
const TAIL_BYTES: u64 = 32 * 1024;

/// The last `TAIL_BYTES` of a file, as text (the first, cut line dropped).
fn tail(path: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else { return String::new() };
    let len = f.metadata().map_or(0, |m| m.len());
    let from = len.saturating_sub(TAIL_BYTES);
    if f.seek(SeekFrom::Start(from)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    let _ = f.take(TAIL_BYTES).read_to_end(&mut bytes);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if from > 0 {
        text.split_once('\n').map_or(String::new(), |(_, rest)| rest.to_string())
    } else {
        text
    }
}

impl WhatHappened {
    /// From the launcher's reason, `<data dir>/crash.log` and the launcher's log.
    pub fn read(reason: Option<String>, data_dir: &Path, log: &Path) -> WhatHappened {
        let last_crash = tail(&data_dir.join(crate::crash::LOG_FILE)).lines().rev().map(str::trim).find(|l| !l.is_empty()).map(str::to_string);
        let log = tail(log);
        let mut launcher: Vec<String> = log.lines().rev().filter_map(|l| l.strip_prefix("launcher: ")).take(LAUNCHER_LINES).map(str::to_string).collect();
        launcher.reverse();
        WhatHappened {
            reason: reason.map(|r| r.trim().to_string()).filter(|r| !r.is_empty()),
            last_crash,
            launcher,
        }
    }

    /// The headline: the launcher's reason, or that safe mode was asked for.
    pub fn headline(&self) -> String {
        self.reason.clone().unwrap_or_else(|| "Safe mode was started by hand.".to_string())
    }
}

/// What "Reset pocket-ui settings" leaves where it is: the logs and crash.log (what happened), and the file sharing
/// and remote access passwords, which belong to this Pocket and must not change.
pub const KEPT_ON_RESET: [&str; 4] = ["logs", crate::crash::LOG_FILE, "share", "remote"];

/// Where `reset_settings` moves the data folder's contents: `<data dir>.rescue-<stamp>` beside it.
pub fn backup_dir(data_dir: &Path, stamp: &str) -> PathBuf {
    let mut name = data_dir.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".rescue-{stamp}"));
    data_dir.with_file_name(name)
}

/// "20261023-140307" for a Unix time (UTC), for the backup's name.
pub fn stamp(unix_secs: u64) -> String {
    let t = crate::crash::timestamp(unix_secs);
    // "2026-10-23 14:03:07 UTC"
    t.chars().filter(|c| c.is_ascii_digit() || *c == ' ').collect::<String>().split_whitespace().collect::<Vec<_>>().join("-")
}

/// Moves everything in the data folder except `KEPT_ON_RESET` into `backup_dir` (renames on the same card: instant,
/// nothing deleted). Games and saves live elsewhere (`/storage/roms`) and are not touched. pocket-ui starts like new
/// afterwards (first-start setup included). Returns the backup folder.
pub fn reset_settings(data_dir: &Path, stamp: &str) -> io::Result<PathBuf> {
    let backup = backup_dir(data_dir, stamp);
    std::fs::create_dir(&backup)?;
    let mut entries: Vec<_> = std::fs::read_dir(data_dir)?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name();
        if KEPT_ON_RESET.iter().any(|k| name == *k) {
            continue;
        }
        std::fs::rename(e.path(), backup.join(&name))?;
    }
    Ok(backup)
}

/// A row of the rescue page, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    RestartNormally,
    PreviousVersion,
    ResetSettings,
    OpenEs,
    PowerOff,
}

/// The dialogs: in front of EmulationStation, and in front of a reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    /// SELECT + START held or Escape on the shelf: Cancel, Restart pocket-ui, Report a problem, Open EmulationStation.
    Leave,
    /// Settings → About and the rescue page: Cancel, Open EmulationStation.
    OpenEs,
    /// The rescue page's reset: Cancel, Reset.
    Reset,
}

/// A choice in a dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Cancel,
    RestartUi,
    OpenEs,
    Reset,
    /// "Report a problem": the dialog is where a player looks when something is wrong (`app_report`).
    Report,
}

impl Choice {
    pub fn label(self) -> &'static str {
        match self {
            Choice::Cancel => "Cancel",
            Choice::RestartUi => "Restart pocket-ui",
            Choice::OpenEs => "Open EmulationStation",
            Choice::Reset => "Reset settings",
            Choice::Report => crate::report::ROW,
        }
    }
}

/// The warning before EmulationStation, one paragraph each (docs/ES-REPLACEMENT-GAPS, 5.3).
pub const ES_WARNING: [&str; 3] = [
    "EmulationStation is ROCKNIX's own menu. pocket-ui stays installed: restart the Pocket to come back to it.",
    "In EmulationStation, do not use Updates. On this Pocket a system update there also rewrites the bootloader that Android needs.",
    "Do not use Factory reset either. It deletes pocket-ui and its settings.",
];

/// A dialog over the page: a title, what to know, and a short list of choices; Cancel first and selected, so A
/// pressed by mistake does nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Dialog {
    pub kind: DialogKind,
    pub sel: usize,
    /// Reset: where the settings will go, for the text.
    pub backup: Option<String>,
}

/// What a press in a dialog asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogOut {
    Stay,
    /// B, or Cancel chosen.
    Close,
    Chose(Choice),
}

impl Dialog {
    pub fn new(kind: DialogKind) -> Dialog {
        Dialog { kind, sel: 0, backup: None }
    }

    pub fn title(&self) -> &'static str {
        match self.kind {
            DialogKind::Leave => "Leave pocket-ui?",
            DialogKind::OpenEs => "Open EmulationStation?",
            DialogKind::Reset => "Reset pocket-ui settings?",
        }
    }

    /// The paragraphs under the title.
    pub fn body(&self) -> Vec<String> {
        match self.kind {
            DialogKind::OpenEs => ES_WARNING.iter().map(|s| s.to_string()).collect(),
            DialogKind::Leave => std::iter::once(format!("Restart pocket-ui starts it afresh. {}", ES_WARNING[0])).chain(ES_WARNING[1..].iter().map(|s| s.to_string())).collect(),
            DialogKind::Reset => vec![
                format!(
                    "Your look, Curator, favourites, play history, covers and what the Store installed move to {}. Nothing is deleted.",
                    self.backup.as_deref().unwrap_or("a backup folder beside them")
                ),
                "Games, saves and the passwords for file sharing and remote access stay. pocket-ui then starts like new, with the first-start setup."
                    .to_string(),
            ],
        }
    }

    pub fn choices(&self) -> &'static [Choice] {
        match self.kind {
            DialogKind::Leave => &[Choice::Cancel, Choice::RestartUi, Choice::Report, Choice::OpenEs],
            DialogKind::OpenEs => &[Choice::Cancel, Choice::OpenEs],
            DialogKind::Reset => &[Choice::Cancel, Choice::Reset],
        }
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> DialogOut {
        let n = self.choices().len();
        match b {
            Button::Up | Button::Left => self.sel = (self.sel + n - 1) % n,
            Button::Down | Button::Right => self.sel = (self.sel + 1) % n,
            b if b == cancel => return DialogOut::Close,
            b if b == confirm => {
                return match self.choices()[self.sel.min(n - 1)] {
                    Choice::Cancel => DialogOut::Close,
                    c => DialogOut::Chose(c),
                }
            }
            _ => {}
        }
        DialogOut::Stay
    }
}

/// Just before EmulationStation opens (asked for): its own update check off (`updates.enabled=0` in system.cfg, under
/// ROCKNIX's lock), so it doesn't offer the system update that rewrites the bootloader on this device
/// (EmulationStation's NetworkThread checks only while it is 1). The player can switch it on again there.
pub fn before_es() {
    if Path::new(crate::system_cfg::PATH).exists() && crate::system_cfg::get("updates.enabled").is_none_or(|v| v.trim() != "0") {
        let ok = crate::system_cfg::set("updates.enabled", "0");
        println!("rescue: EmulationStation's update check switched off: {}", if ok { "done" } else { "failed" });
    }
}

/// The exit code for a choice that leaves pocket-ui.
pub fn exit_code(c: Choice) -> Option<i32> {
    match c {
        Choice::RestartUi => Some(RESTART_CODE),
        Choice::OpenEs => Some(OPEN_ES_CODE),
        Choice::Cancel | Choice::Reset | Choice::Report => None,
    }
}

/// What a press on the rescue page asks the safe-mode app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    /// End with this code (the launcher takes it from there).
    Exit(i32),
    /// Move the settings aside (`reset_settings`), then say where they went.
    ResetSettings,
    PowerOff,
}

/// The rescue page: the rows, the selection and an open dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct RescuePage {
    pub sel: usize,
    pub what: WhatHappened,
    /// The version "Use previous version" goes to; no row without one.
    pub previous: Option<String>,
    pub dialog: Option<Dialog>,
    /// What the last action did ("Settings moved to …").
    pub message: Option<String>,
    /// Where the settings would go, for the reset dialog.
    pub backup: String,
    /// The settings were moved: Reset is done.
    pub reset_done: bool,
}

impl RescuePage {
    pub fn new(what: WhatHappened, previous: Option<String>, backup: String) -> RescuePage {
        RescuePage { sel: 0, what, previous, dialog: None, message: None, backup, reset_done: false }
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::RestartNormally];
        if self.previous.is_some() {
            rows.push(Row::PreviousVersion);
        }
        if !self.reset_done {
            rows.push(Row::ResetSettings);
        }
        rows.extend([Row::OpenEs, Row::PowerOff]);
        rows
    }

    pub fn row(&self) -> Row {
        let rows = self.rows();
        rows[self.sel.min(rows.len() - 1)]
    }

    pub fn label(&self, row: Row) -> String {
        match row {
            Row::RestartNormally => "Restart normally".into(),
            Row::PreviousVersion => format!("Use previous version ({})", self.previous.as_deref().unwrap_or("?")),
            Row::ResetSettings => "Reset pocket-ui settings…".into(),
            Row::OpenEs => "Open EmulationStation…".into(),
            Row::PowerOff => "Power off".into(),
        }
    }

    /// The line under the list for the selected row.
    pub fn describe(&self, row: Row) -> &'static str {
        match row {
            Row::RestartNormally => "Start pocket-ui as usual. If it fails again, this page comes back.",
            Row::PreviousVersion => "Go back to the version that ran before this one. Your settings and games stay.",
            Row::ResetSettings => "Move pocket-ui's settings aside and start like new. Games and saves stay.",
            Row::OpenEs => "ROCKNIX's own menu, with a warning first.",
            Row::PowerOff => "Switch the Pocket off.",
        }
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        if let Some(d) = self.dialog.as_mut() {
            return match d.press(b, confirm, cancel) {
                DialogOut::Stay => Action::Nothing,
                DialogOut::Close => {
                    self.dialog = None;
                    Action::Nothing
                }
                DialogOut::Chose(Choice::Reset) => {
                    self.dialog = None;
                    Action::ResetSettings
                }
                DialogOut::Chose(c) => {
                    self.dialog = None;
                    exit_code(c).map_or(Action::Nothing, Action::Exit)
                }
            };
        }
        let n = self.rows().len();
        match b {
            Button::Up => self.sel = (self.sel + n - 1) % n,
            Button::Down => self.sel = (self.sel + 1) % n,
            b if b == confirm => {
                return match self.row() {
                    Row::RestartNormally => Action::Exit(RESTART_CODE),
                    Row::PreviousVersion => Action::Exit(PREVIOUS_CODE),
                    Row::ResetSettings => {
                        self.dialog = Some(Dialog { backup: Some(self.backup.clone()), ..Dialog::new(DialogKind::Reset) });
                        Action::Nothing
                    }
                    Row::OpenEs => {
                        self.dialog = Some(Dialog::new(DialogKind::OpenEs));
                        Action::Nothing
                    }
                    Row::PowerOff => Action::PowerOff,
                }
            }
            _ => {}
        }
        Action::Nothing
    }

    /// The reset has run: the row goes, the page says where the settings are.
    pub fn reset_finished(&mut self, result: Result<PathBuf, String>) {
        match result {
            Ok(dir) => {
                self.reset_done = true;
                self.sel = 0;
                self.message = Some(format!("Settings moved to {}. Restart normally to start like new.", dir.display()));
            }
            Err(e) => self.message = Some(format!("Couldn't move the settings: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn tmp(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("rescue-{name}"))
    }

    #[test]
    fn the_exit_codes_are_the_launchers() {
        let launcher = include_str!("../deploy/run-pocket-ui.sh");
        let code = |name: &str| launcher.lines().find_map(|l| l.strip_prefix(&format!("CODE_{name}="))?.trim().parse::<i32>().ok());
        assert_eq!(code("OPEN_ES"), Some(OPEN_ES_CODE));
        assert_eq!(code("PREVIOUS"), Some(PREVIOUS_CODE));
        assert_eq!(code("UPDATE"), Some(crate::update::RESTART_CODE));
        assert_eq!(RESTART_CODE, 0);
        // Exit 0 means start again; it may never hand over to EmulationStation.
        assert!(launcher.contains("    0)\n      if [ $framed = 1 ]; then"), "exit 0 with a picture restarts");
        let distinct = [RESTART_CODE, OPEN_ES_CODE, PREVIOUS_CODE, crate::update::RESTART_CODE, crate::update::WATCHDOG_EXIT, 126, 127];
        for (i, a) in distinct.iter().enumerate() {
            assert!(!distinct[i + 1..].contains(a), "{a} twice");
        }
        assert!(launcher.contains("POCKETUI_FRAME_FILE") && launcher.contains("POCKETUI_SAFE_REASON") && launcher.contains("run_ui --safe"));
        assert!(launcher.contains(&format!("\"$ROOT/{ES_NOTE_FILE}\"")) && launcher.contains(KNOWN_GOOD));
    }

    #[test]
    fn safe_mode_by_argument_or_environment() {
        assert!(safe_mode_requested(&["pocket-ui".into(), "--safe".into()], None));
        assert!(safe_mode_requested(&["pocket-ui".into()], Some("1")));
        assert!(!safe_mode_requested(&["pocket-ui".into()], Some("0")));
        assert!(!safe_mode_requested(&["pocket-ui".into(), "--snapshot".into()], None));
    }

    #[test]
    fn the_es_note_is_shown_once() {
        let d = tmp("note");
        assert_eq!(take_es_note(&d), None);
        std::fs::write(d.join(ES_NOTE_FILE), "pocket-ui could not start, not even in safe mode (exit 139)\n").unwrap();
        assert_eq!(
            take_es_note(&d).as_deref(),
            Some("EmulationStation opened last time: pocket-ui could not start, not even in safe mode (exit 139).")
        );
        assert_eq!(take_es_note(&d), None);
    }

    fn release(root: &Path, v: &str, with_ui: bool) {
        let dir = root.join("releases").join(v);
        std::fs::create_dir_all(&dir).unwrap();
        if with_ui {
            crate::testutil::write_executable(&dir.join("pocket-ui"), "#!/bin/sh\n");
        }
    }

    #[test]
    fn the_previous_version_follows_the_launchers_rule() {
        let root = tmp("previous");
        let ln = |target: &str, name: &str| {
            let _ = std::fs::remove_file(root.join(name));
            std::os::unix::fs::symlink(target, root.join(name)).unwrap();
        };
        assert_eq!(other_version(&root), None, "no current: a flat install");
        release(&root, "2.0", true);
        release(&root, "1.0", true);
        release(&root, "0.9", true);
        release(&root, "broken", false);
        ln("releases/2.0", "current");
        assert_eq!(other_version(&root), None);
        ln("releases/1.0", "previous");
        assert_eq!(other_version(&root).as_deref(), Some("1.0"));
        ln("releases/0.9", KNOWN_GOOD);
        assert_eq!(other_version(&root).as_deref(), Some("0.9"), "known-good first");
        ln("releases/2.0", KNOWN_GOOD);
        assert_eq!(other_version(&root).as_deref(), Some("1.0"), "known-good is current: previous");
        ln("releases/broken", "previous");
        assert_eq!(other_version(&root), None, "one without a pocket-ui is no way back");
    }

    #[test]
    fn what_happened_reads_the_ends_of_the_logs() {
        let d = tmp("what");
        std::fs::write(d.join("crash.log"), "2026-10-22 10:00:00 UTC pocket-ui panicked at a.rs:1:1: old\n2026-10-23 10:00:00 UTC pocket-ui panicked at b.rs:2:2: new\n\n").unwrap();
        let log = d.join("pocket-ui.log");
        let mut text = "x\n".repeat(20_000);
        text.push_str("launcher: one\nexit code 139\nlauncher: two\nlauncher: three\nsomething else\nlauncher: four\n");
        std::fs::write(&log, text).unwrap();
        let w = WhatHappened::read(Some(" pocket-ui stopped 4 times in a row (exit 139). ".into()), &d, &log);
        assert_eq!(w.reason.as_deref(), Some("pocket-ui stopped 4 times in a row (exit 139)."));
        assert_eq!(w.last_crash.as_deref(), Some("2026-10-23 10:00:00 UTC pocket-ui panicked at b.rs:2:2: new"));
        assert_eq!(w.launcher, ["two", "three", "four"]);
        assert_eq!(w.headline(), "pocket-ui stopped 4 times in a row (exit 139).");
        let none = WhatHappened::read(None, &d.join("nothing"), &d.join("nothing.log"));
        assert_eq!(none, WhatHappened::default());
        assert_eq!(none.headline(), "Safe mode was started by hand.");
    }

    #[test]
    fn a_reset_moves_the_settings_aside_and_keeps_logs_and_passwords() {
        let d = tmp("reset");
        let data = d.join("pocket-ui");
        for dir in ["logs", "share", "remote", "covers/nes", "store"] {
            std::fs::create_dir_all(data.join(dir)).unwrap();
        }
        for f in ["settings.json", "crash.log", "library.json", "covers/nes/a.png", "share/password"] {
            std::fs::write(data.join(f), f).unwrap();
        }
        assert_eq!(stamp(1_792_764_187), "20261023-140307");
        let backup = reset_settings(&data, "20261023-140307").unwrap();
        assert_eq!(backup, d.join("pocket-ui.rescue-20261023-140307"));
        for kept in ["logs", "share/password", "remote", "crash.log"] {
            assert!(data.join(kept).exists(), "{kept} stays");
        }
        for moved in ["settings.json", "library.json", "covers/nes/a.png", "store"] {
            assert!(!data.join(moved).exists() && backup.join(moved).exists(), "{moved} moved");
        }
        assert!(reset_settings(&data, "20261023-140307").is_err(), "never over an earlier backup");
    }

    #[test]
    fn dialogs_start_on_cancel_and_say_what_harms_the_device() {
        let mut d = Dialog::new(DialogKind::Leave);
        assert_eq!(d.choices(), [Choice::Cancel, Choice::RestartUi, Choice::Report, Choice::OpenEs]);
        assert_eq!(d.press(A, A, B), DialogOut::Close, "A at once is Cancel");
        assert_eq!(d.press(Button::Down, A, B), DialogOut::Stay);
        assert_eq!(d.press(A, A, B), DialogOut::Chose(Choice::RestartUi));
        d.press(Button::Down, A, B);
        assert_eq!(d.press(A, A, B), DialogOut::Chose(Choice::Report), "something is wrong: the report is one row from Restart");
        assert_eq!(exit_code(Choice::Report), None, "the report doesn't leave pocket-ui");
        d.press(Button::Down, A, B);
        assert_eq!(d.press(A, A, B), DialogOut::Chose(Choice::OpenEs));
        assert_eq!(d.press(Button::Down, A, B), DialogOut::Stay, "wraps around");
        assert_eq!(d.sel, 0);
        assert_eq!(d.press(B, A, B), DialogOut::Close);
        let body = Dialog::new(DialogKind::OpenEs).body().join(" ");
        assert!(body.contains("Updates") && body.contains("bootloader") && body.contains("Factory reset") && body.contains("deletes pocket-ui"));
        assert_eq!(Dialog::new(DialogKind::OpenEs).choices(), [Choice::Cancel, Choice::OpenEs]);
        assert_eq!(exit_code(Choice::OpenEs), Some(77));
        assert_eq!(exit_code(Choice::RestartUi), Some(0));
    }

    #[test]
    fn the_rescue_page_rows_and_what_they_do() {
        let mut p = RescuePage::new(WhatHappened::default(), Some("1.0".into()), "/x.rescue".into());
        assert_eq!(p.rows(), [Row::RestartNormally, Row::PreviousVersion, Row::ResetSettings, Row::OpenEs, Row::PowerOff]);
        assert_eq!(p.label(Row::PreviousVersion), "Use previous version (1.0)");
        assert_eq!(p.press(A, A, B), Action::Exit(RESTART_CODE));
        p.press(Button::Down, A, B);
        assert_eq!(p.press(A, A, B), Action::Exit(PREVIOUS_CODE));
        p.press(Button::Down, A, B);
        assert_eq!(p.press(A, A, B), Action::Nothing, "the reset asks first");
        assert_eq!(p.dialog.as_ref().map(|d| d.kind), Some(DialogKind::Reset));
        assert!(p.dialog.as_ref().unwrap().body()[0].contains("/x.rescue"));
        assert_eq!(p.press(B, A, B), Action::Nothing);
        assert!(p.dialog.is_none(), "B closes the dialog, nothing done");
        p.press(A, A, B);
        p.press(Button::Down, A, B);
        assert_eq!(p.press(A, A, B), Action::ResetSettings);
        p.reset_finished(Ok("/x.rescue".into()));
        assert_eq!(p.rows(), [Row::RestartNormally, Row::PreviousVersion, Row::OpenEs, Row::PowerOff], "done once");
        assert!(p.message.as_deref().unwrap().contains("/x.rescue"));
        p.sel = 2;
        p.press(A, A, B);
        assert_eq!(p.dialog.as_ref().map(|d| d.kind), Some(DialogKind::OpenEs));
        p.press(Button::Down, A, B);
        assert_eq!(p.press(A, A, B), Action::Exit(OPEN_ES_CODE));
        p.sel = 3;
        assert_eq!(p.press(A, A, B), Action::PowerOff);
        assert_eq!(p.press(B, A, B), Action::Nothing, "B on the page itself goes nowhere: there is nothing behind it");
        let without = RescuePage::new(WhatHappened::default(), None, String::new());
        assert!(!without.rows().contains(&Row::PreviousVersion));
    }
}
