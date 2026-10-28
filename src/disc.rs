//! Changing discs of a multi-disc game from the pause menu (pocket-ui's own host, docs/HOST.md "Discs").
//!
//! pocket-core learns the discs from the core's disk control interface (`libretro::Core::disc`: an `.m3u` playlist,
//! a multi-disc `.pbp`) and writes what it knows to [`FILE`] once the game has loaded, when the pause menu opens and
//! after every change. pocket-ui reads that file when the pause menu opens (`app_disc.rs`, microseconds on tmpfs) and
//! shows a **Disc** row only for a game with more than one disc. No file, a file of another game, a core without disc
//! control or a game with one disc (a single `.cue`): no row.
//!
//! The row sends `disc <n>` (1-based) on pocket-core's command FIFO; pocket-core opens the tray, sets the disc and
//! closes it (`Core::set_disc`), then writes the file again.

use std::path::Path;

/// What pocket-core knows about the running game's discs: "<disc in the drive, 1-based; 0 unknown> <count>", then the
/// game's path on the next line, so a file left by another game is never read as this one's.
pub const FILE: &str = "/tmp/pocket-core.disc";

/// The discs of the running game, as the core reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Discs {
    /// The disc in the drive, 0-based; None when the core names none of its discs (an index past the last).
    pub current: Option<u32>,
    pub count: u32,
}

impl Discs {
    /// From the core's disk control: `index` 0-based, `count` images.
    pub fn from_core(index: u32, count: u32) -> Discs {
        Discs { current: (index < count).then_some(index), count }
    }

    /// The file's text for the game `rom`.
    pub fn to_text(self, rom: &Path) -> String {
        format!("{} {}\n{}\n", self.current.map_or(0, |c| c + 1), self.count, rom.display())
    }

    /// The file's text, if it is about the game `rom` and makes sense.
    pub fn parse(text: &str, rom: &Path) -> Option<Discs> {
        let mut lines = text.lines();
        let mut nums = lines.next()?.split_whitespace().map(|w| w.parse::<u32>().ok());
        let (at, count) = (nums.next()??, nums.next()??);
        if lines.next()? != rom.to_str()? || count == 0 {
            return None;
        }
        Some(Discs { current: (1..=count).contains(&at).then(|| at - 1), count })
    }

    /// Writes [`FILE`] for the game `rom` (pocket-core). Never half written; tmpfs, so nothing waits for the card.
    pub fn write(self, rom: &Path) {
        self.write_to(Path::new(FILE), rom);
    }

    fn write_to(self, file: &Path, rom: &Path) {
        if let Err(e) = crate::fsutil::replace_file(file, self.to_text(rom).as_bytes()) {
            eprintln!("disc: {e}");
        }
    }

    /// What [`FILE`] says about the game `rom`; None when there is no file or it is about another game.
    pub fn read(rom: &Path) -> Option<Discs> {
        Discs::read_from(Path::new(FILE), rom)
    }

    fn read_from(file: &Path, rom: &Path) -> Option<Discs> {
        Discs::parse(&std::fs::read_to_string(file).ok()?, rom)
    }
}

/// Removes [`FILE`] when it is about the game `rom` (pocket-core's start and end; a game started meanwhile keeps its own).
pub fn forget(rom: &Path) {
    forget_at(Path::new(FILE), rom);
}

fn forget_at(file: &Path, rom: &Path) {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    if text.lines().nth(1).is_some_and(|p| Some(p) == rom.to_str()) {
        let _ = std::fs::remove_file(file);
    }
}

/// The pause menu's Disc row: the disc chosen with Left/Right, and what A does with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscRow {
    pub discs: Discs,
    /// The disc the row shows, 0-based. It starts on the one in the drive.
    pub chosen: u32,
}

impl DiscRow {
    /// A row for a game with two discs or more; None otherwise.
    pub fn new(discs: Discs) -> Option<DiscRow> {
        (discs.count > 1).then(|| DiscRow { discs, chosen: discs.current.unwrap_or(0) })
    }

    /// Left/Right: the previous or next disc, round the end.
    pub fn step(&mut self, delta: i32) {
        self.chosen = (self.chosen as i64 + delta as i64).rem_euclid(self.discs.count as i64) as u32;
    }

    /// The disc A puts in (0-based): the chosen one, or the next when the chosen one is already in the drive (A on
    /// the row as it opens changes to the next disc, as a "next disc" button would).
    pub fn target(&self) -> u32 {
        match self.discs.current {
            Some(c) if c == self.chosen => (c + 1) % self.discs.count,
            _ => self.chosen,
        }
    }

    /// The command for pocket-core's FIFO.
    pub fn command(&self) -> String {
        format!("disc {}", self.target() + 1)
    }

    /// "2 of 3", beside the row's name.
    pub fn chip(&self) -> String {
        format!("{} of {}", self.chosen + 1, self.discs.count)
    }

    /// The line beside the menu: which disc is in, and what A does.
    pub fn note(&self) -> String {
        match self.discs.current {
            Some(c) => format!("Disc {} is in. A puts in disc {} and goes back to the game.", c + 1, self.target() + 1),
            None => format!("A puts in disc {} and goes back to the game.", self.target() + 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn rom() -> &'static Path {
        Path::new("/storage/roms/psx/Final Fantasy VII/Final Fantasy VII.m3u")
    }

    #[test]
    fn the_file_round_trips_and_belongs_to_one_game() {
        let d = Discs::from_core(1, 3);
        assert_eq!(d, Discs { current: Some(1), count: 3 });
        let text = d.to_text(rom());
        assert!(text.starts_with("2 3\n"), "{text:?}");
        assert_eq!(Discs::parse(&text, rom()), Some(d));
        assert_eq!(Discs::parse(&text, Path::new("/storage/roms/psx/Other.cue")), None, "another game's file");
    }

    #[test]
    fn missing_or_broken_data_gives_no_row() {
        // No file, an empty one, a cut-off one, words instead of numbers, no discs at all.
        for text in ["", "2", "2 3", "x 3\n/r", "2 x\n/r", "1 0\n/r"] {
            let rom = Path::new("/r");
            assert!(Discs::parse(text, rom).and_then(DiscRow::new).is_none(), "{text:?}");
        }
        // One disc (a single .cue, or a core that reports one image): the menu stays as it is.
        assert_eq!(DiscRow::new(Discs::from_core(0, 1)), None);
        assert_eq!(DiscRow::new(Discs { current: None, count: 0 }), None);
    }

    #[test]
    fn an_unknown_disc_in_the_drive_still_offers_the_discs() {
        // A core that names none of its discs (index past the last) or a file that says 0.
        let d = Discs::from_core(3, 3);
        assert_eq!(d.current, None);
        assert_eq!(Discs::parse("0 3\n/r", Path::new("/r")), Some(Discs { current: None, count: 3 }));
        let mut row = DiscRow::new(d).unwrap();
        assert_eq!((row.chip(), row.command()), ("1 of 3".to_string(), "disc 1".to_string()));
        assert_eq!(row.note(), "A puts in disc 1 and goes back to the game.");
        row.step(-1);
        assert_eq!((row.chip(), row.command()), ("3 of 3".to_string(), "disc 3".to_string()));
    }

    #[test]
    fn left_right_choose_and_a_puts_in_the_chosen_or_the_next_disc() {
        let mut row = DiscRow::new(Discs::from_core(0, 3)).unwrap();
        assert_eq!(row.chip(), "1 of 3", "starts on the disc in the drive");
        assert_eq!(row.command(), "disc 2", "A on the disc that is in: the next one");
        assert_eq!(row.note(), "Disc 1 is in. A puts in disc 2 and goes back to the game.");
        row.step(1);
        row.step(1);
        assert_eq!((row.chip(), row.command()), ("3 of 3".to_string(), "disc 3".to_string()));
        row.step(1);
        assert_eq!(row.chip(), "1 of 3", "round the end");
        row.step(-1);
        assert_eq!(row.chip(), "3 of 3", "and back");
        // The last disc in the drive: A goes round to the first.
        let row = DiscRow::new(Discs::from_core(1, 2)).unwrap();
        assert_eq!(row.command(), "disc 1");
    }

    #[test]
    fn forget_leaves_another_games_file_alone() {
        let dir = TestDir::new("disc");
        let file = dir.join("pocket-core.disc");
        assert_eq!(Discs::read_from(&file, rom()), None, "no file: no row");
        let other = Path::new("/storage/roms/psx/Other.m3u");
        Discs::from_core(0, 2).write_to(&file, other);
        forget_at(&file, rom());
        assert_eq!(Discs::read_from(&file, other), Some(Discs::from_core(0, 2)), "a game started meanwhile keeps its file");
        assert_eq!(Discs::read_from(&file, rom()), None, "and it is not this game's");
        forget_at(&file, other);
        assert!(!file.exists());
    }
}
