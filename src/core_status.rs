//! Read-only projection of pocket-core's control flags (docs/DEVICE-TESTS.md).
//! Uses the UI's v1 status writer; never acknowledges a command or promises a completed save.
use std::path::PathBuf;
use crate::{core_command::Quit, disc::Discs, save_trouble::Trouble, ui_status::{Line, Writer}};

pub const FILE: &str = "/tmp/pocket-core.status";

/// One dominant state, with orthogonal flags retained in the status for combinations such as sleep behind a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Play { Playing, Held, MenuPaused, Asleep, DarkPlaying, Exiting }
impl Play {
    pub fn code(self) -> &'static str {
        match self { Self::Playing => "playing", Self::Held => "held", Self::MenuPaused => "menu-paused",
            Self::Asleep => "asleep", Self::DarkPlaying => "dark-playing", Self::Exiting => "exiting" }
    }
}

/// Cheap to compare between frames: no core calls, files, strings or allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub paused: bool,
    pub sleeping: bool,
    pub dark: bool,
    pub held: bool,
    pub quit: Option<Quit>,
    /// Zero-based internally, one-based on the wire like the existing slot commands.
    pub slot: usize,
    pub discs: Option<Discs>,
    pub save_trouble: Option<Trouble>,
}
impl Snapshot {
    pub fn play(self) -> Play {
        if self.quit.is_some() { Play::Exiting }
        else if self.sleeping { Play::Asleep }
        else if self.paused { Play::MenuPaused }
        else if self.dark { Play::DarkPlaying }
        else if self.held { Play::Held }
        else { Play::Playing }
    }
    fn line(self) -> Line {
        let mut line = Line::new();
        line.field("play", self.play().code());
        line.field("quit", match self.quit { None => "none", Some(Quit::Save) => "save",
            Some(Quit::SkipResume) => "skip-resume", Some(Quit::ForSwitch) => "for-switch" });
        line.field("menu", if self.paused { "paused" } else { "none" });
        line.field("asleep", if self.sleeping { "on" } else { "off" });
        line.field("screen", if self.dark { "dark" } else { "on" });
        line.field("hold", if self.held { "on" } else { "off" });
        line.field("slot", (self.slot + 1).to_string());
        line.field("disc", self.discs.map_or_else(|| "none".into(), |d|
            format!("{}/{}", d.current.map_or(0, |c| c + 1), d.count)));
        line.field("save-trouble", match self.save_trouble { None => "none", Some(Trouble::Full) => "full",
            Some(Trouble::Unwritable) => "unwritable", Some(Trouble::Unreadable) => "unreadable" });
        line
    }
}

/// Only changed snapshots are formatted. No writes, heap allocation or core calls on an unchanged frame.
/// No Drop cleanup: an old process must never remove its successor's file.
pub struct Publisher {
    writer: Writer,
    last: Option<Snapshot>,
    warned: bool,
}
impl Publisher {
    pub fn new(path: PathBuf) -> Self { Self { writer: Writer::new(path), last: None, warned: false } }
    /// A failure leaves the revision and cached snapshot unchanged; the next event retries it.
    pub fn offer(&mut self, now: Snapshot) -> std::io::Result<bool> {
        if self.last == Some(now) { return Ok(false); }
        match self.writer.offer(&now.line()) {
            Ok(wrote) => { self.last = Some(now); self.warned = false; Ok(wrote) }
            Err(e) => {
                if !self.warned { eprintln!("core status: {e}"); self.warned = true; }
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot { Snapshot { paused: false, sleeping: false, dark: false, held: false,
        quit: None, slot: 0, discs: None, save_trouble: None } }
    #[test]
    fn core_status_all_flag_combinations_have_one_dominant_state() {
        for flags in 0..16 {
            let s = Snapshot { paused: flags & 1 != 0, sleeping: flags & 2 != 0, dark: flags & 4 != 0,
                held: flags & 8 != 0, ..snapshot() };
            let expected = if flags & 2 != 0 { Play::Asleep } else if flags & 1 != 0 { Play::MenuPaused }
                else if flags & 4 != 0 { Play::DarkPlaying } else if flags & 8 != 0 { Play::Held } else { Play::Playing };
            assert_eq!(s.play(), expected);
            assert_eq!(Snapshot { quit: Some(Quit::ForSwitch), ..s }.play(), Play::Exiting);
        }
    }
    #[test]
    fn core_status_revision_changes_only_after_successful_changed_snapshots() {
        let dir = crate::testutil::TestDir::new("core-status");
        let blocked = dir.join("blocked");
        std::fs::write(&blocked, b"a file cannot contain status").unwrap();
        let file = blocked.join("status");
        let mut p = Publisher::new(file.clone());
        assert!(p.offer(snapshot()).is_err());
        std::fs::remove_file(&blocked).unwrap();
        std::fs::create_dir(&blocked).unwrap();
        assert!(p.offer(snapshot()).unwrap());
        let first = std::fs::read_to_string(&file).unwrap();
        assert!(first.starts_with("v=1\trev=1\tpid="));
        assert!(!p.offer(snapshot()).unwrap());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), first);
        assert!(p.offer(Snapshot { paused: true, sleeping: true, ..snapshot() }).unwrap());
        let second = std::fs::read_to_string(&file).unwrap();
        assert!(second.contains("\trev=2\t") && second.contains("\tplay=asleep\t") && second.contains("\tmenu=paused\t"));
    }
    #[test]
    fn core_status_dropping_an_old_publisher_keeps_its_successors_file() {
        let dir = crate::testutil::TestDir::new("core-status-owner");
        let file = dir.join("status");
        let mut old = Publisher::new(file.clone()); old.offer(snapshot()).unwrap();
        let mut new = Publisher::new(file.clone());
        new.offer(Snapshot { held: true, ..snapshot() }).unwrap();
        let newer = std::fs::read_to_string(&file).unwrap(); drop(old);
        assert_eq!(std::fs::read_to_string(file).unwrap(), newer);
    }
    #[test]
    fn core_status_codes_keep_slots_discs_and_save_health_separate_from_exit() {
        let s = Snapshot { slot: 7, discs: Some(Discs { current: None, count: 3 }),
            save_trouble: Some(Trouble::Unreadable), quit: Some(Quit::SkipResume), ..snapshot() };
        let text = s.line().render(1,42);
        for field in ["play=exiting", "quit=skip-resume", "slot=8", "disc=0/3", "save-trouble=unreadable"] {
            assert!(text.contains(field), "{text}");
        }
    }
}
