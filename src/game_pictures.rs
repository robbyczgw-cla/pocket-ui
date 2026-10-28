//! In-game screenshots and title screens from the libretro thumbnail collection, next to the box art
//! (`covers.rs`): the same site, the same free access, the same name matching (the checksum's name, then
//! `covers::best_match` on the file name: `covers::pick`), the same rules for when it may run (`auto_covers.rs`).
//!
//! Where they live: parallel to `<data>/covers/<system>/<rom stem>.png`, one folder per kind,
//!
//! * `<data>/snaps/<system>/<rom stem>.png`: the game running (the collection's `Named_Snaps`),
//! * `<data>/titles/<system>/<rom stem>.png`: its title screen (`Named_Titles`),
//!
//! each with a `.missing` list of the games the collection has no picture for (looked up once, not again). A PNG
//! you put there under the right name is used like a downloaded one. A picture is only kept when its header says
//! it is a PNG of a size pocket-ui is willing to decode (`image::size_ok`, review N2), so an absurd file never
//! reaches the shelf; the download is cut off at [`MAX_BYTES`].
//!
//! `run_until` is the whole job (box art first, then the pictures), for the automatic download and for
//! Settings > Games > Extras > Cover art alike.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::covers;

/// The largest picture file that is downloaded (the collection's biggest are a few hundred KB).
pub const MAX_BYTES: u64 = 3_000_000;

/// Which picture of a game.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    /// The game running.
    Snap,
    /// The title screen.
    Title,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Snap, Kind::Title];

    /// The folder in the data directory.
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Snap => "snaps",
            Kind::Title => "titles",
        }
    }

    /// The directory of the collection.
    pub fn remote(self) -> &'static str {
        match self {
            Kind::Snap => "Named_Snaps",
            Kind::Title => "Named_Titles",
        }
    }
}

/// Where the picture of a game is kept, whether or not it is there yet. The same stem rule as the box art
/// (`covers::cover_path`).
pub fn path(data_dir: &Path, kind: Kind, system: &str, rom: &Path) -> PathBuf {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
    data_dir.join(kind.folder()).join(system).join(format!("{stem}.png"))
}

fn missing_file(data_dir: &Path, kind: Kind, system: &str) -> PathBuf {
    data_dir.join(kind.folder()).join(system).join(".missing")
}

/// The games a `.missing` list of this kind names, by ROM stem.
pub fn listed_missing(data_dir: &Path, kind: Kind, system: &str) -> HashSet<String> {
    std::fs::read_to_string(missing_file(data_dir, kind, system)).unwrap_or_default().lines().map(str::to_string).collect()
}

/// A downloaded file is kept only when it is a PNG whose size may be decoded.
pub fn acceptable(file: &Path) -> bool {
    crate::image::png_size(file).is_some()
}

/// The games (system id, ROM path) that still lack a picture of some kind the collection may have: a known
/// system, no file, not in that system's `.missing` list, and not looked up already (`tried`, this session).
pub fn missing(data_dir: &Path, games: &[(String, PathBuf)], tried: &HashSet<PathBuf>) -> Vec<(String, PathBuf)> {
    let mut listed: HashMap<(Kind, &str), HashSet<String>> = HashMap::new();
    games
        .iter()
        .filter(|(sys, rom)| {
            if covers::playlist(sys).is_none() || tried.contains(rom) {
                return false;
            }
            let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            Kind::ALL.iter().any(|&kind| {
                !path(data_dir, kind, sys, rom).exists() && !listed.entry((kind, sys.as_str())).or_insert_with(|| listed_missing(data_dir, kind, sys)).contains(stem)
            })
        })
        .cloned()
        .collect()
}

/// What asking the collection for one picture came to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fetched {
    Got,
    /// The file came but is not one pocket-ui will decode: it counts as not there.
    Unusable,
    /// No answer (offline, a timeout): not the collection's "no", so ask again next time.
    Failed,
}

/// The collection, as far as the job needs it (replaced in the tests).
pub trait Remote {
    /// The names listed for a playlist's directory; empty when it could not be read.
    fn index(&self, playlist: &str, kind: Kind) -> Vec<String>;
    fn download(&self, playlist: &str, kind: Kind, name: &str, dest: &Path) -> Fetched;
}

/// thumbnails.libretro.com through the system's curl.
pub struct Libretro;

impl Remote for Libretro {
    fn index(&self, playlist: &str, kind: Kind) -> Vec<String> {
        covers::fetch_index_in(playlist, kind.remote())
    }

    fn download(&self, playlist: &str, kind: Kind, name: &str, dest: &Path) -> Fetched {
        let unusable = std::cell::Cell::new(false);
        let accept = |f: &Path| {
            let ok = acceptable(f);
            unusable.set(!ok);
            ok
        };
        let max = MAX_BYTES.to_string();
        if covers::download_from(playlist, kind.remote(), name, dest, &["--max-filesize", &max], &accept) {
            Fetched::Got
        } else if unusable.get() {
            Fetched::Unusable
        } else {
            Fetched::Failed
        }
    }
}

/// What `scrape_until` got through.
pub struct Scraped {
    pub found: usize,
    /// The games it looked at (found or not); the rest were left for later.
    pub looked_up: Vec<PathBuf>,
}

fn note_missing(data_dir: &Path, kind: Kind, system: &str, stem: &str) {
    let file = missing_file(data_dir, kind, system);
    let now = std::fs::read_to_string(&file).unwrap_or_default();
    if !now.lines().any(|l| l == stem) {
        let _ = crate::fsutil::write_atomic(&file, format!("{now}{stem}\n").as_bytes());
    }
}

/// Downloads the missing pictures of `games` (system id, ROM path). `stop` is asked before every file (a game was
/// started): the job ends after the file it is on. `progress(done, total, found)` runs after each game.
pub fn scrape_until(
    remote: &dyn Remote,
    data_dir: &Path,
    games: &[(String, PathBuf)],
    progress: impl Fn(usize, usize, usize),
    stop: impl Fn() -> bool,
) -> Scraped {
    let total = games.len();
    let (mut done, mut found) = (0, 0);
    let mut looked_up = Vec::new();
    // The listing of the playlist being worked through, per kind: games come system by system.
    let mut index: HashMap<Kind, (String, Vec<String>)> = HashMap::new();
    // The proper names the games' checksums gave (`rom_id`).
    let names = crate::rom_id::known(data_dir, games);
    'games: for (sys, rom) in games {
        let Some(list) = covers::playlist(sys) else { continue };
        let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        for kind in Kind::ALL {
            let dest = path(data_dir, kind, sys, rom);
            if dest.exists() || listed_missing(data_dir, kind, sys).contains(stem) {
                continue;
            }
            if stop() {
                break 'games;
            }
            if index.get(&kind).is_none_or(|(p, _)| p != list) {
                index.insert(kind, (list.to_string(), remote.index(list, kind)));
            }
            let list_names = &index[&kind].1;
            match covers::pick(stem, names.get(rom).map(String::as_str), list_names) {
                Some(name) => match remote.download(list, kind, name, &dest) {
                    Fetched::Got => found += 1,
                    Fetched::Unusable => note_missing(data_dir, kind, sys, stem),
                    Fetched::Failed => {}
                },
                // An empty listing means it didn't load; don't mark the game as missing then.
                None if !list_names.is_empty() => note_missing(data_dir, kind, sys, stem),
                None => {}
            }
        }
        looked_up.push(rom.clone());
        done += 1;
        progress(done, total, found);
    }
    Scraped { found, looked_up }
}

/// What `run_until` got through.
pub struct Outcome {
    /// New files, box art and pictures.
    pub found: usize,
    /// The games whose box art was looked up, and those whose pictures were; the rest were left for later.
    pub covers_looked_up: Vec<PathBuf>,
    pub pictures_looked_up: Vec<PathBuf>,
    /// How many games each part had to do (the progress counts both).
    pub covers_total: usize,
    pub pictures_total: usize,
}

impl Outcome {
    /// A game cut it short.
    pub fn stopped(&self) -> bool {
        self.covers_looked_up.len() < self.covers_total || self.pictures_looked_up.len() < self.pictures_total
    }
}

/// The whole download: box art for `games` first (`covers::scrape_until`), then, when `pictures` is given, those
/// games' screenshots and title screens. `progress(done, total, found)` counts the games of both parts.
pub fn run_until(
    remote: &dyn Remote,
    data_dir: &Path,
    covers_for: &[(String, PathBuf)],
    pictures: Option<&[(String, PathBuf)]>,
    progress: impl Fn(usize, usize, usize),
    stop: impl Fn() -> bool,
) -> Outcome {
    let covers_total = covers::todo(data_dir, covers_for).len();
    let pictures = pictures.unwrap_or_default();
    let pictures_total = pictures.len();
    let total = covers_total + pictures_total;
    let first = covers::scrape_until(data_dir, covers_for, |done, _, found| progress(done, total, found), &stop);
    let mut out = Outcome { found: first.found, covers_looked_up: first.looked_up, pictures_looked_up: Vec::new(), covers_total, pictures_total };
    if out.covers_looked_up.len() < covers_total {
        return out;
    }
    let second = scrape_until(remote, data_dir, pictures, |done, _, found| progress(covers_total + done, total, first.found + found), stop);
    out.found += second.found;
    out.pictures_looked_up = second.looked_up;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::cell::RefCell;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("pictures-{name}"))
    }

    fn game(sys: &str, stem: &str) -> (String, PathBuf) {
        (sys.to_string(), PathBuf::from(format!("/storage/roms/{sys}/{stem}.rom")))
    }

    /// A PNG header of `w` x `h` (what `png_size` reads), and nothing after it.
    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b
    }

    /// A made-up collection: these names exist, in both directories; each download is `size` big.
    struct Fake {
        names: Vec<String>,
        size: (u32, u32),
        online: bool,
        asked: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new(names: &[&str]) -> Fake {
            Fake { names: names.iter().map(|s| s.to_string()).collect(), size: (256, 224), online: true, asked: RefCell::default() }
        }
    }

    impl Remote for Fake {
        fn index(&self, playlist: &str, kind: Kind) -> Vec<String> {
            self.asked.borrow_mut().push(format!("index {playlist} {}", kind.remote()));
            if self.online {
                self.names.clone()
            } else {
                Vec::new()
            }
        }

        fn download(&self, _playlist: &str, kind: Kind, name: &str, dest: &Path) -> Fetched {
            self.asked.borrow_mut().push(format!("{} {name}", kind.remote()));
            if !self.online {
                return Fetched::Failed;
            }
            let tmp = dest.with_extension("part");
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::write(&tmp, png_header(self.size.0, self.size.1)).unwrap();
            if !acceptable(&tmp) {
                let _ = std::fs::remove_file(&tmp);
                return Fetched::Unusable;
            }
            std::fs::rename(&tmp, dest).unwrap();
            Fetched::Got
        }
    }

    #[test]
    fn pictures_are_kept_next_to_the_covers_by_the_same_stem() {
        let d = Path::new("/data");
        let rom = Path::new("/storage/roms/gba/Advance Wars (USA).gba");
        assert_eq!(covers::cover_path(d, "gba", rom), Path::new("/data/covers/gba/Advance Wars (USA).png"));
        assert_eq!(path(d, Kind::Snap, "gba", rom), Path::new("/data/snaps/gba/Advance Wars (USA).png"));
        assert_eq!(path(d, Kind::Title, "gba", rom), Path::new("/data/titles/gba/Advance Wars (USA).png"));
        assert_eq!(Kind::Snap.remote(), "Named_Snaps");
        assert_eq!(Kind::Title.remote(), "Named_Titles");
    }

    #[test]
    fn they_are_matched_by_the_names_the_box_art_is() {
        let d = dir("match");
        // A scene-numbered file name and a short region still find the No-Intro entry, as for the box art.
        let games = vec![game("gba", "0902 - MegaMan & Bass (U)(Venom)")];
        let fake = Fake::new(&["Mega Man & Bass (Japan)", "Mega Man & Bass (USA)"]);
        let done = scrape_until(&fake, &d, &games, |_, _, _| {}, || false);
        assert_eq!(done.found, 2, "a snap and a title");
        assert!(fake.asked.borrow().contains(&"Named_Snaps Mega Man & Bass (USA)".to_string()));
        assert!(path(&d, Kind::Snap, "gba", &games[0].1).is_file() && path(&d, Kind::Title, "gba", &games[0].1).is_file());
    }

    #[test]
    fn a_game_recognised_by_its_checksum_is_asked_for_under_that_name() {
        let d = dir("checksum");
        let rom = d.join("roms/gba/mmb.gba");
        std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
        std::fs::write(&rom, b"mmbs").unwrap();
        let games = vec![("gba".to_string(), rom.clone())];
        let dat = "game (\n\tname \"Mega Man & Bass (USA)\"\n\trom ( name \"x.gba\" size 4 crc 36C08112 )\n)\n";
        crate::rom_id::run_until(&d, &games, &|_| Some(dat.to_string()), true, || false);
        let fake = Fake::new(&["Mega Man & Bass (USA)", "Mega Man Zero (USA)"]);
        let done = scrape_until(&fake, &d, &games, |_, _, _| {}, || false);
        assert_eq!(done.found, 2);
        assert!(fake.asked.borrow().contains(&"Named_Titles Mega Man & Bass (USA)".to_string()), "{:?}", fake.asked.borrow());
        // Kept under the file's own stem, like every picture.
        assert!(path(&d, Kind::Snap, "gba", &rom).ends_with("snaps/gba/mmb.png"));
    }

    #[test]
    fn a_game_the_collection_does_not_have_is_remembered_and_not_asked_for_again() {
        let d = dir("not-found");
        let games = vec![game("snes", "Homebrew Thing"), game("snes", "Tetris Attack (USA)")];
        let fake = Fake::new(&["Tetris Attack (USA)"]);
        let done = scrape_until(&fake, &d, &games, |_, _, _| {}, || false);
        assert_eq!(done.found, 2);
        assert_eq!(done.looked_up.len(), 2, "looked at both");
        for kind in Kind::ALL {
            assert_eq!(std::fs::read_to_string(d.join(kind.folder()).join("snes/.missing")).unwrap(), "Homebrew Thing\n");
        }
        // Next time: only what is new. The remembered one and the one on the card are left alone.
        assert!(missing(&d, &games, &HashSet::new()).is_empty());
        let more = vec![game("snes", "Homebrew Thing"), game("snes", "Brand New (USA)")];
        assert_eq!(missing(&d, &more, &HashSet::new()), vec![game("snes", "Brand New (USA)")]);
        let again = Fake::new(&[]);
        scrape_until(&again, &d, &games, |_, _, _| {}, || false);
        assert!(again.asked.borrow().is_empty(), "no request for a game that is remembered or has its pictures");
    }

    #[test]
    fn a_listing_that_did_not_load_does_not_mark_games_missing() {
        let d = dir("offline");
        let games = vec![game("gba", "Advance Wars (USA)")];
        let mut fake = Fake::new(&["Advance Wars (USA)"]);
        fake.online = false;
        let done = scrape_until(&fake, &d, &games, |_, _, _| {}, || false);
        assert_eq!(done.found, 0);
        assert!(!d.join("snaps/gba/.missing").exists() && !d.join("titles/gba/.missing").exists());
        assert_eq!(missing(&d, &games, &HashSet::new()).len(), 1, "tried again at the next start");
        // Only this session skips it.
        assert!(missing(&d, &games, &done.looked_up.iter().cloned().collect()).is_empty());
    }

    #[test]
    fn an_absurd_picture_is_never_kept_and_not_asked_for_again() {
        let d = dir("absurd");
        let games = vec![game("gba", "Advance Wars (USA)")];
        let mut fake = Fake::new(&["Advance Wars (USA)"]);
        // A 196-byte PNG claiming 100 x 50,000,000 pixels (review N2).
        fake.size = (100, 50_000_000);
        let done = scrape_until(&fake, &d, &games, |_, _, _| {}, || false);
        assert_eq!(done.found, 0);
        assert!(!path(&d, Kind::Snap, "gba", &games[0].1).exists());
        assert!(!d.join("snaps/gba/Advance Wars (USA).part").exists(), "no leftovers");
        assert_eq!(listed_missing(&d, Kind::Snap, "gba").len(), 1);
    }

    #[test]
    fn a_game_stops_the_job_after_the_file_it_is_on() {
        let d = dir("stop");
        let games = vec![game("nes", "A (USA)"), game("nes", "B (USA)"), game("nes", "C (USA)")];
        let fake = Fake::new(&["A (USA)", "B (USA)", "C (USA)"]);
        let fetched = || fake.asked.borrow().iter().filter(|a| a.starts_with("Named_")).count();
        // Two files may be fetched, then a game starts.
        let done = scrape_until(&fake, &d, &games, |_, _, _| {}, || fetched() >= 2);
        assert_eq!(done.found, 2);
        assert_eq!(done.looked_up, vec![games[0].1.clone()], "A is through, B is left for after the game");
        assert_eq!(missing(&d, &games, &done.looked_up.iter().cloned().collect()).len(), 2);
        // The file already fetched for the cut-off game is not fetched twice later.
        let fake = Fake::new(&["A (USA)", "B (USA)", "C (USA)"]);
        scrape_until(&fake, &d, &games, |_, _, _| {}, || false);
        assert_eq!(fake.asked.borrow().iter().filter(|a| a.starts_with("Named_")).count(), 4, "B and C, two pictures each");
    }

    #[test]
    fn systems_without_a_collection_are_left_out() {
        let d = dir("unknown-system");
        let games = vec![game("tools", "Calculator"), game("gba", "A")];
        assert_eq!(missing(&d, &games, &HashSet::new()), vec![game("gba", "A")]);
    }

    #[test]
    fn the_job_counts_box_art_and_pictures_and_does_the_box_art_first() {
        let d = dir("run");
        let games = vec![game("gba", "Advance Wars (USA)")];
        // The box art needs the network; with none, only the order and the totals are checked.
        let fake = Fake::new(&["Advance Wars (USA)"]);
        let seen = RefCell::new(Vec::new());
        let todo = missing(&d, &games, &HashSet::new());
        let out = run_until(&fake, &d, &games, Some(&todo), |done, total, _| seen.borrow_mut().push((done, total)), || true);
        assert_eq!((out.covers_total, out.pictures_total), (1, 1));
        assert!(out.stopped() && seen.borrow().is_empty(), "stopped before the first file");
        let none = run_until(&fake, &d, &games, None, |_, _, _| {}, || true);
        assert_eq!(none.pictures_total, 0, "Box art only: no pictures");
    }
}
