//! The screenshots on the card (HOME + A, `screenshot.rs`) as a calm gallery: which files there are, what the page
//! shows and does, the small cache of their pictures. The App's side (the scan thread, painting, the loader) is
//! `app_gallery.rs`.
//!
//! - **What is a screenshot**: a file `<screenshots>/<system>/<game> <YYYY-MM-DD HH-MM-SS>[ (n)].png`, the names
//!   `screenshot::file_name` makes. The game is the part in front of the time, read from the end, so "Game" and
//!   "Game 2" never mix. Other files in the folder (a PNG copied there by hand, a link) are not listed.
//! - **Two entry points**, one page: Settings → Games → Screenshots lists every screenshot (`Scope::All`), the game
//!   card's "Screenshots (N)" row only that game's (`Scope::Game`). Newest first, in groups by day.
//! - **Keys**: A opens the picture full screen (Left and Right browse, B comes back), X asks to delete and the second
//!   A does it, Y makes the picture the game's "In the game" picture on the shelf (`use_on_shelf`).
//! - **Nothing on the UI thread**: the folder is read on a thread ([`Index::load`]), pictures are decoded on the
//!   picture loader's thread into a [`PictureCache`] that keeps a few of them. A file that is not a picture shows a
//!   placeholder.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::image::Image;
use crate::pad::Button;
use crate::picture_loader::Key;
use crate::play_history::{civil, day_of, MONTHS};
use crate::screenshot::{clean, valid_at};

/// A date and time as the file name has it: the device's local time when the picture was taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct When {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
}

impl When {
    /// "2026-10-24 18-05-09" (what `valid_at` accepts); None when a part is not a date or a time.
    fn parse(at: &str) -> Option<When> {
        let part = |r: std::ops::Range<usize>| at.get(r)?.parse::<u32>().ok();
        let w = When { year: at.get(0..4)?.parse().ok()?, month: part(5..7)?, day: part(8..10)?, hour: part(11..13)?, min: part(14..16)?, sec: part(17..19)? };
        let ok = (1..=12).contains(&w.month) && (1..=31).contains(&w.day) && w.hour < 24 && w.min < 60 && w.sec < 61;
        ok.then_some(w)
    }

    pub fn date(&self) -> (i32, u32, u32) {
        (self.year, self.month, self.day)
    }

    /// "18:05".
    pub fn clock(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.min)
    }
}

/// One screenshot on the card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shot {
    pub path: PathBuf,
    /// The system's folder (its id: "gba").
    pub system: String,
    /// The game part of the file name ("Tetris (World) (Rev 1)").
    pub game: String,
    pub when: When,
    /// 1, or the number in " (2)" for a second one in the same second.
    pub n: u32,
}

impl Shot {
    /// The game as the shelf names it ("Tetris").
    pub fn title(&self) -> String {
        let name = crate::library::shelf_name(&self.game);
        if name.is_empty() {
            self.game.clone()
        } else {
            name
        }
    }
}

/// The game, the time and the repeat number of a file name; None for any other file.
fn split_name(file: &str) -> Option<(&str, When, u32)> {
    let stem = file.strip_suffix(".png")?;
    let (body, n) = match stem.strip_suffix(')').and_then(|s| s.rsplit_once(" (")) {
        Some((body, num)) if !num.is_empty() && num.len() <= 3 && num.bytes().all(|c| c.is_ascii_digit()) => (body, num.parse().ok()?),
        _ => (stem, 1),
    };
    let cut = body.len().checked_sub(19)?;
    if cut < 2 || !body.is_char_boundary(cut) || body.as_bytes()[cut - 1] != b' ' || !valid_at(&body[cut..]) {
        return None;
    }
    Some((&body[..cut - 1], When::parse(&body[cut..])?, n))
}

/// Newest first; the same time by its repeat number, then by name, so the order never depends on the folder's.
fn newest_first(a: &Shot, b: &Shot) -> std::cmp::Ordering {
    (b.when, b.n, &b.path).cmp(&(a.when, a.n, &a.path))
}

/// Every screenshot under `dir`, newest first: `dir/<system>/<file>`. A folder that is not there, or cannot be read,
/// is no screenshots.
pub fn scan(dir: &Path) -> Vec<Shot> {
    let mut found = Vec::new();
    let Ok(systems) = std::fs::read_dir(dir) else { return found };
    for sys in systems.filter_map(|e| e.ok()).filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
        let system = sys.file_name().to_string_lossy().into_owned();
        let Ok(files) = std::fs::read_dir(sys.path()) else { continue };
        for file in files.filter_map(|e| e.ok()).filter(|e| e.file_type().is_ok_and(|t| t.is_file())) {
            let name = file.file_name().to_string_lossy().into_owned();
            if let Some((game, when, n)) = split_name(&name) {
                found.push(Shot { path: file.path(), system: system.clone(), game: game.to_string(), when, n });
            }
        }
    }
    found.sort_by(newest_first);
    found
}

/// What the folder held at the last look: kept in memory, so the game card and the Settings row know a count without
/// reading anything.
#[derive(Clone, Debug, Default)]
pub struct Index {
    shots: Vec<Shot>,
    counts: HashMap<(String, String), usize>,
    /// The first look is through (until then nothing is known, which is not the same as nothing there).
    loaded: bool,
}

impl Index {
    pub fn new(mut shots: Vec<Shot>) -> Index {
        shots.sort_by(newest_first);
        let mut counts: HashMap<(String, String), usize> = HashMap::new();
        for s in &shots {
            *counts.entry((s.system.clone(), s.game.clone())).or_default() += 1;
        }
        Index { shots, counts, loaded: true }
    }

    /// Reads the folder (on a thread).
    pub fn load(dir: &Path) -> Index {
        Index::new(scan(dir))
    }

    pub fn loaded(&self) -> bool {
        self.loaded
    }

    pub fn len(&self) -> usize {
        self.shots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.shots.is_empty()
    }

    pub fn all(&self) -> &[Shot] {
        &self.shots
    }

    /// The key a game's screenshots are filed under: its system's folder and the game part of the file name.
    fn key(system: &str, stem: &str) -> (String, String) {
        (clean(system), clean(stem))
    }

    /// How many screenshots a game has (`system` its id, `stem` the ROM's file name without the extension).
    pub fn count(&self, system: &str, stem: &str) -> usize {
        self.counts.get(&Index::key(system, stem)).copied().unwrap_or(0)
    }

    /// A game's screenshots, newest first.
    pub fn of_game(&self, system: &str, stem: &str) -> Vec<Shot> {
        let key = Index::key(system, stem);
        self.shots.iter().filter(|s| (&s.system, &s.game) == (&key.0, &key.1)).cloned().collect()
    }

    /// The newest screenshot of a game.
    pub fn latest(&self, system: &str, stem: &str) -> Option<&Shot> {
        let key = Index::key(system, stem);
        self.shots.iter().find(|s| (&s.system, &s.game) == (&key.0, &key.1))
    }

    /// A screenshot was deleted.
    pub fn remove(&mut self, path: &Path) {
        let Some(at) = self.shots.iter().position(|s| s.path == path) else { return };
        let s = self.shots.remove(at);
        let key = (s.system, s.game);
        if let Some(n) = self.counts.get_mut(&key) {
            *n -= 1;
            if *n == 0 {
                self.counts.remove(&key);
            }
        }
    }
}

/// "Today", "Yesterday" or "24 October 2026" for a day, as seen on `today`.
pub fn day_label(day: (i32, u32, u32), today: (i32, u32, u32)) -> String {
    let n = day_of(day.0, day.1, day.2);
    let t = day_of(today.0, today.1, today.2);
    if n == t {
        "Today".into()
    } else if n == t - 1 {
        "Yesterday".into()
    } else {
        format!("{} {} {}", day.2, MONTHS[(day.1 as usize).clamp(1, 12) - 1], day.0)
    }
}

/// The date `days` after `date` (negative: before).
pub fn add_days(date: (i32, u32, u32), days: i64) -> (i32, u32, u32) {
    civil(day_of(date.0, date.1, date.2) + days)
}

/// A row of the list: a day's heading, or the screenshot at this place of the page's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Day((i32, u32, u32)),
    Shot(usize),
}

/// The rows of a list of screenshots (newest first): a heading before the first of each day.
pub fn rows(shots: &[Shot]) -> Vec<Row> {
    let mut out = Vec::with_capacity(shots.len() + 8);
    let mut day = None;
    for (i, s) in shots.iter().enumerate() {
        if day != Some(s.when.date()) {
            day = Some(s.when.date());
            out.push(Row::Day(s.when.date()));
        }
        out.push(Row::Shot(i));
    }
    out
}

/// Whose screenshots the page lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Settings → Games → Screenshots: every game's.
    All,
    /// The game card's row: this game's (its title for the page's heading).
    Game(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    List,
    Full,
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    None,
    Close,
    /// The second A: delete this file.
    Delete(PathBuf),
    /// Y: this file becomes the game's picture on the shelf.
    UseOnShelf(Shot),
}

/// The page: the list, the full-screen view, the delete question.
#[derive(Debug)]
pub struct Gallery {
    pub scope: Scope,
    shots: Vec<Shot>,
    /// The folder has been read (false: "Looking for screenshots…").
    pub loaded: bool,
    /// The selected screenshot (an index into the list); 0 with none.
    pub sel: usize,
    pub view: View,
    /// X was pressed: the next A deletes.
    pub armed: bool,
    /// The first row on screen (an index into `rows`).
    top: usize,
    /// What the last delete or Y came to, until the next press.
    pub message: Option<(String, bool)>,
    /// The day the page calls "Today".
    pub today: (i32, u32, u32),
    /// Y can do something: the shelf shows game pictures (Settings → Games → Extras → Game pictures).
    pub shelf: bool,
}

/// How many rows L1 and R1 move.
const PAGE: usize = 5;

impl Gallery {
    pub fn new(scope: Scope, shots: Vec<Shot>, loaded: bool, today: (i32, u32, u32), shelf: bool) -> Gallery {
        Gallery { scope, shots, loaded, sel: 0, view: View::List, armed: false, top: 0, message: None, today, shelf }
    }

    pub fn shots(&self) -> &[Shot] {
        &self.shots
    }

    pub fn current(&self) -> Option<&Shot> {
        self.shots.get(self.sel)
    }

    /// The list was read again: the page takes it and keeps the selected picture if it is still there.
    pub fn refresh(&mut self, shots: Vec<Shot>) {
        let keep = self.current().map(|s| s.path.clone());
        self.shots = shots;
        self.loaded = true;
        self.sel = keep.and_then(|p| self.shots.iter().position(|s| s.path == p)).unwrap_or(self.sel.min(self.shots.len().saturating_sub(1)));
        if self.shots.is_empty() {
            self.view = View::List;
            self.armed = false;
        }
    }

    /// A press. `confirm` and `cancel` are the buttons for A and B (Settings → Playing → Button layout).
    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Out {
        let before = (self.sel, self.view);
        let out = self.act(b, confirm, cancel);
        if (self.sel, self.view) != before {
            self.armed = false;
            self.message = None;
        }
        out
    }

    fn act(&mut self, b: Button, confirm: Button, cancel: Button) -> Out {
        if self.armed {
            return match b {
                b if b == confirm => {
                    self.armed = false;
                    self.current().map_or(Out::None, |s| Out::Delete(s.path.clone()))
                }
                // B, X again: no. Moving on leaves the question behind (the caller clears it with the move).
                b if b == cancel || b == Button::North => {
                    self.armed = false;
                    Out::None
                }
                Button::Up | Button::Down | Button::Left | Button::Right | Button::L1 | Button::R1 => {
                    self.armed = false;
                    self.act(b, confirm, cancel)
                }
                _ => Out::None,
            };
        }
        let last = self.shots.len().saturating_sub(1);
        let full = self.view == View::Full;
        match b {
            b if b == cancel => {
                if full {
                    self.view = View::List;
                    Out::None
                } else {
                    Out::Close
                }
            }
            b if b == confirm => {
                if !full && !self.shots.is_empty() {
                    self.view = View::Full;
                }
                Out::None
            }
            Button::North if !self.shots.is_empty() => {
                self.armed = true;
                self.message = None;
                Out::None
            }
            Button::West if self.shelf => self.current().cloned().map_or(Out::None, Out::UseOnShelf),
            Button::Up => self.go(-1, last),
            Button::Down => self.go(1, last),
            Button::Left if full => self.go(-1, last),
            Button::Right if full => self.go(1, last),
            Button::L1 if !full => self.go(-(PAGE as isize), last),
            Button::R1 if !full => self.go(PAGE as isize, last),
            _ => Out::None,
        }
    }

    fn go(&mut self, by: isize, last: usize) -> Out {
        self.sel = (self.sel as isize + by).clamp(0, last as isize) as usize;
        Out::None
    }

    /// The file is gone: the page forgets it, and says so.
    pub fn deleted(&mut self, path: &Path) {
        if let Some(at) = self.shots.iter().position(|s| s.path == path) {
            self.shots.remove(at);
            if at < self.sel {
                self.sel -= 1;
            }
        }
        self.sel = self.sel.min(self.shots.len().saturating_sub(1));
        self.armed = false;
        if self.shots.is_empty() {
            self.view = View::List;
        }
        self.message = Some(("Screenshot deleted".into(), false));
    }

    pub fn failed(&mut self, why: &str) {
        self.armed = false;
        self.message = Some((why.to_string(), true));
    }

    pub fn said(&mut self, text: &str) {
        self.message = Some((text.to_string(), false));
    }

    /// The hint bar.
    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.armed {
            return vec![("A", "Delete now"), ("B", "Keep it")];
        }
        if self.shots.is_empty() {
            return vec![("B", "Back")];
        }
        let mut h = match self.view {
            View::List => vec![("A", "Open"), ("B", "Back"), ("X", "Delete")],
            View::Full => vec![("B", "Back"), ("LEFT RIGHT", "Browse"), ("X", "Delete")],
        };
        if self.shelf {
            h.push(("Y", "Shelf picture"));
        }
        h
    }

    /// The rows of the list with their day headings.
    pub fn rows(&self) -> Vec<Row> {
        rows(&self.shots)
    }

    /// The first row on screen after the selection moved: as little scrolling as will do, the selected row whole,
    /// and its day's heading with it when it is the day's first. `heights` are a heading's and a screenshot's, `avail`
    /// the room for the list. Returns the index into [`Gallery::rows`].
    pub fn follow(&mut self, avail: i32, (day_h, shot_h): (i32, i32)) -> usize {
        let rows = self.rows();
        let height = |r: &Row| if matches!(r, Row::Day(_)) { day_h } else { shot_h };
        let Some(at) = rows.iter().position(|r| *r == Row::Shot(self.sel)) else {
            self.top = 0;
            return 0;
        };
        // The heading over the first picture of a day belongs to it.
        let want = if at > 0 && matches!(rows[at - 1], Row::Day(_)) { at - 1 } else { at };
        self.top = self.top.min(rows.len().saturating_sub(1));
        if want < self.top {
            self.top = want;
        }
        while self.top < want && rows[self.top..=at].iter().map(height).sum::<i32>() > avail {
            self.top += 1;
        }
        self.top
    }

    /// A short line for the page's heading area: "3 of 12" in the full view.
    pub fn place(&self) -> String {
        format!("{} of {}", self.sel + 1, self.shots.len())
    }
}

/// A few decoded pictures, the oldest let go when there are too many; what was asked for and is not here yet.
/// `Some(None)` is a file that could not be read.
pub struct PictureCache {
    cap: usize,
    map: HashMap<Key, Option<Rc<Image>>>,
    order: VecDeque<Key>,
    pending: HashSet<Key>,
}

/// What the cache says about a picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    /// It is here (or known to be unreadable).
    Have,
    /// Asked for already, or the loader has enough to do: not now.
    Wait,
    /// Ask the loader for it.
    Ask,
}

impl PictureCache {
    pub fn new(cap: usize) -> PictureCache {
        PictureCache { cap: cap.max(1), map: HashMap::new(), order: VecDeque::new(), pending: HashSet::new() }
    }

    pub fn get(&self, key: &Key) -> Option<&Option<Rc<Image>>> {
        self.map.get(key)
    }

    /// Whether to ask the loader for `key`; at most `room` requests are out at a time, so a fast scroll never queues
    /// pictures that are long off screen.
    pub fn want(&mut self, key: &Key, room: usize) -> Want {
        if self.map.contains_key(key) {
            Want::Have
        } else if self.pending.contains(key) || self.pending.len() >= room {
            Want::Wait
        } else {
            self.pending.insert(key.clone());
            Want::Ask
        }
    }

    /// The loader answered. False when this cache did not ask (the picture is somebody else's).
    pub fn arrived(&mut self, key: &Key, img: Option<Image>) -> bool {
        if !self.pending.remove(key) {
            return false;
        }
        while self.map.len() >= self.cap {
            match self.order.pop_front() {
                Some(old) => {
                    self.map.remove(&old);
                }
                None => break,
            }
        }
        self.map.insert(key.clone(), img.map(Rc::new));
        self.order.push_back(key.clone());
        true
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Whether this cache asked for `key` and is waiting for it (the loader's answers come to one place).
    pub fn owns(&self, key: &Key) -> bool {
        self.pending.contains(key)
    }

    /// Every picture let go. What was asked for stays asked for: its answer still belongs here and is not mistaken
    /// for somebody else's.
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    /// Forgets one file's pictures, whatever their size.
    pub fn forget(&mut self, path: &Path) {
        self.map.retain(|k, _| k.0 != path);
        self.order.retain(|k| k.0 != path);
    }
}

/// The box a picture of `src` pixels is shown in on a screen with `room` free: a whole multiple of its size when it is
/// small (a Game Boy picture, so no pixel is wider than its neighbour), else the room, which `Image::fit` fits it to.
pub fn full_box(src: (u32, u32), room: (i32, i32)) -> (i32, i32) {
    let (w, h) = (src.0.max(1) as i32, src.1.max(1) as i32);
    let k = (room.0 / w).min(room.1 / h);
    if k >= 2 {
        (w * k, h * k)
    } else {
        room
    }
}

/// Deletes a screenshot: one `unlink`. Only a plain file of the folder's own naming two levels under `dir` goes (never
/// a link, a folder or a file outside), and one that is gone already counts as deleted. `Err`: why it stayed.
pub fn delete(dir: &Path, file: &Path) -> Result<(), std::io::Error> {
    use std::io::{Error, ErrorKind};
    let inside = file.parent().and_then(|p| p.parent()) == Some(dir) && file.file_name().and_then(|n| n.to_str()).is_some_and(|n| split_name(n).is_some());
    if !inside {
        return Err(Error::new(ErrorKind::InvalidInput, "not a screenshot of the folder"));
    }
    match file.symlink_metadata() {
        Ok(m) if m.is_file() => std::fs::remove_file(file),
        Ok(_) => Err(Error::new(ErrorKind::InvalidInput, "not a plain file")),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// The largest screenshot made the shelf's picture: far above what a screen makes.
const SHELF_MAX_BYTES: u64 = 8_000_000;

/// Copies a screenshot to where the shelf looks for the game's "In the game" picture (`game_pictures::path`). Whole or
/// not at all; an unreadable or oversized file is refused with the reason in a few words.
pub fn use_on_shelf(shot: &Path, dest: &Path) -> Result<(), String> {
    use std::io::Read;
    if crate::image::png_size(shot).is_none() {
        return Err("That file is not a picture the Pocket can read".into());
    }
    let mut bytes = Vec::new();
    let read = std::fs::File::open(shot).and_then(|f| f.take(SHELF_MAX_BYTES + 1).read_to_end(&mut bytes));
    if read.is_err() {
        return Err("Could not read the screenshot".into());
    }
    if bytes.len() as u64 > SHELF_MAX_BYTES {
        return Err("That screenshot is too large".into());
    }
    crate::fsutil::write_atomic(dest, &bytes).map_err(|_| "Could not save it: is the SD card full?".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn scratch(tag: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("gallery-{tag}"))
    }

    fn put(dir: &Path, system: &str, file: &str) -> PathBuf {
        let p = dir.join(system).join(file);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, crate::screenshot::encode_rgb(2, 2, &[7; 12]).unwrap()).unwrap();
        p
    }

    fn shot(system: &str, game: &str, stamp: &str, n: u32) -> Shot {
        let when = When::parse(stamp).unwrap();
        Shot { path: PathBuf::from(format!("/s/{system}/{game} {stamp}{}.png", if n > 1 { format!(" ({n})") } else { String::new() })), system: system.into(), game: game.into(), when, n }
    }

    fn page(shots: Vec<Shot>) -> Gallery {
        Gallery::new(Scope::All, shots, true, (2026, 10, 24), true)
    }

    fn three() -> Vec<Shot> {
        vec![shot("gba", "Alpha", "2026-10-24 18-05-09", 1), shot("gba", "Alpha", "2026-10-24 17-00-00", 1), shot("gb", "Tetris", "2026-10-23 09-00-00", 1)]
    }

    #[test]
    fn file_names_are_read_from_the_end() {
        let (g, w, n) = split_name("Tetris (World) (Rev 1) 2026-10-24 18-05-09.png").unwrap();
        assert_eq!((g, w.date(), w.clock(), n), ("Tetris (World) (Rev 1)", (2026, 10, 24), "18:05".to_string(), 1));
        assert_eq!(split_name("Game 2 2026-10-24 18-05-09 (3).png").map(|(g, _, n)| (g, n)), Some(("Game 2", 3)));
        assert_eq!(split_name("Pokémon – Rot 2026-01-02 03-04-05.png").map(|(g, _, _)| g), Some("Pokémon – Rot"));
        for bad in ["Game.png", "2026-10-24 18-05-09.png", " 2026-10-24 18-05-09.png", "Game 2026-10-24 18-05-09.jpg", "Game 2026-13-24 18-05-09.png", "Game 2026-10-24 25-05-09.png", "Game 2026-10-24 18-05-09 (x).png", "Game 2026-10-24 18-05-09 ().png", "é.png"] {
            assert!(split_name(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_folder_is_listed_newest_first_and_only_by_names_pocket_ui_makes() {
        let dir = scratch("scan");
        put(&dir, "gba", "Alpha 2026-10-24 18-05-09.png");
        put(&dir, "gba", "Alpha 2026-10-24 18-05-09 (2).png");
        put(&dir, "gba", "Alpha 2026-10-23 09-00-00.png");
        put(&dir, "gb", "Tetris 2026-10-24 19-00-00.png");
        put(&dir, "gb", "notes.png");
        put(&dir, "gb", "Tetris 2026-10-24 19-00-00.txt");
        std::fs::write(dir.join("loose 2026-10-24 19-00-00.png"), b"x").unwrap();
        let shots = scan(&dir);
        let names: Vec<String> = shots.iter().map(|s| format!("{}/{} {} {}", s.system, s.game, s.when.clock(), s.n)).collect();
        assert_eq!(names, ["gb/Tetris 19:00 1", "gba/Alpha 18:05 2", "gba/Alpha 18:05 1", "gba/Alpha 09:00 1"]);
        assert!(scan(&dir.join("nowhere")).is_empty(), "no folder: no screenshots");
    }

    #[test]
    fn a_game_counts_only_its_own_pictures() {
        let ix = Index::new(vec![shot("gba", "Game", "2026-10-24 10-00-00", 1), shot("gba", "Game 2", "2026-10-24 11-00-00", 1), shot("gba", "Game", "2026-10-23 10-00-00", 1), shot("gb", "Game", "2026-10-22 10-00-00", 1)]);
        assert_eq!((ix.count("gba", "Game"), ix.count("gba", "Game 2"), ix.count("gb", "Game"), ix.count("gb", "Other")), (2, 1, 1, 0));
        assert_eq!(ix.of_game("gba", "Game").len(), 2);
        assert_eq!(ix.latest("gba", "Game").map(|s| s.when.day), Some(24));
        // The game part goes through the same cleaning as the file name: a name with a colon is filed with an underscore.
        let odd = Index::new(vec![shot("gba", "A_B", "2026-10-24 10-00-00", 1)]);
        assert_eq!(odd.count("gba", "A:B"), 1);
        let mut ix = ix;
        ix.remove(Path::new("/s/gba/Game 2026-10-24 10-00-00.png"));
        assert_eq!((ix.count("gba", "Game"), ix.len()), (1, 3));
        assert!(!Index::default().loaded() && Index::new(Vec::new()).loaded(), "not read yet is not the same as none");
    }

    #[test]
    fn the_list_is_grouped_by_day_with_a_heading_each() {
        let r = rows(&three());
        assert_eq!(r, [Row::Day((2026, 10, 24)), Row::Shot(0), Row::Shot(1), Row::Day((2026, 10, 23)), Row::Shot(2)]);
        assert!(rows(&[]).is_empty());
        let today = (2026, 10, 24);
        assert_eq!(day_label((2026, 10, 24), today), "Today");
        assert_eq!(day_label((2026, 10, 23), today), "Yesterday");
        assert_eq!(day_label((2026, 10, 1), today), "1 October 2026");
        assert_eq!(day_label((2025, 12, 31), (2026, 1, 1)), "Yesterday", "over the new year");
        assert_eq!(day_label((2026, 10, 25), today), "25 October 2026", "a clock that is ahead is not 'Today'");
        assert_eq!(add_days((2026, 3, 1), -1), (2026, 2, 28));
    }

    #[test]
    fn a_opens_b_goes_back_and_the_list_stops_at_its_ends() {
        let mut p = page(three());
        assert_eq!(p.press(Button::Up, A, B), Out::None);
        assert_eq!(p.sel, 0);
        p.press(Button::Down, A, B);
        p.press(Button::Down, A, B);
        p.press(Button::Down, A, B);
        assert_eq!(p.sel, 2, "no wrapping round to the oldest");
        assert_eq!(p.press(A, A, B), Out::None);
        assert_eq!(p.view, View::Full);
        assert_eq!(p.press(Button::Left, A, B), Out::None);
        assert_eq!(p.sel, 1, "Left: the one before in the list");
        p.press(Button::Right, A, B);
        p.press(Button::Right, A, B);
        assert_eq!(p.sel, 2, "Right stops at the last");
        assert_eq!(p.press(B, A, B), Out::None);
        assert_eq!(p.view, View::List, "B leaves the full view first");
        assert_eq!(p.press(B, A, B), Out::Close);
        p.press(Button::R1, A, B);
        assert_eq!(p.sel, 2);
        p.press(Button::L1, A, B);
        assert_eq!(p.sel, 0, "L1 and R1 move five");
    }

    #[test]
    fn delete_wants_a_second_a_and_anything_else_keeps_the_picture() {
        let mut p = page(three());
        p.press(Button::Down, A, B);
        assert_eq!(p.press(Button::North, A, B), Out::None);
        assert!(p.armed);
        assert_eq!(p.hints(), [("A", "Delete now"), ("B", "Keep it")]);
        assert_eq!(p.press(B, A, B), Out::None, "B keeps it and does not leave the page");
        assert!(!p.armed);
        p.press(Button::North, A, B);
        p.press(Button::North, A, B);
        assert!(!p.armed, "X again is a no");
        p.press(Button::North, A, B);
        p.press(Button::Down, A, B);
        assert!(!p.armed && p.sel == 2, "moving on drops the question");
        p.press(Button::North, A, B);
        let path = p.current().unwrap().path.clone();
        assert_eq!(p.press(A, A, B), Out::Delete(path.clone()));
        assert!(!p.armed, "one A, one file");
        p.deleted(&path);
        assert_eq!((p.shots().len(), p.sel), (2, 1));
        assert_eq!(p.message, Some(("Screenshot deleted".into(), false)));
        // The last one goes: an empty list, not a blank or a full screen of nothing.
        let mut one = page(vec![shot("gb", "Tetris", "2026-10-23 09-00-00", 1)]);
        one.press(A, A, B);
        one.press(Button::North, A, B);
        let Out::Delete(path) = one.press(A, A, B) else { panic!("delete") };
        one.deleted(&path);
        assert_eq!((one.shots().len(), one.view), (0, View::List));
        assert_eq!(one.hints(), [("B", "Back")]);
        assert_eq!(one.press(Button::North, A, B), Out::None);
        assert!(!one.armed, "nothing to delete");
    }

    #[test]
    fn deleting_before_the_selection_keeps_the_same_picture_selected() {
        let mut p = page(three());
        p.sel = 2;
        let first = p.shots()[0].path.clone();
        let last = p.current().unwrap().path.clone();
        p.deleted(&first);
        assert_eq!(p.current().unwrap().path, last);
    }

    #[test]
    fn y_offers_the_shelf_picture_only_when_the_shelf_shows_them() {
        let mut p = page(three());
        p.press(Button::Down, A, B);
        let Out::UseOnShelf(s) = p.press(Button::West, A, B) else { panic!("Y") };
        assert_eq!(s.when.clock(), "17:00");
        assert!(p.hints().contains(&("Y", "Shelf picture")));
        p.shelf = false;
        assert_eq!(p.press(Button::West, A, B), Out::None);
        assert!(!p.hints().iter().any(|h| h.0 == "Y"));
    }

    #[test]
    fn every_state_has_hints_and_a_selection_that_exists() {
        for n in 0..3 {
            for view in [View::List, View::Full] {
                let mut p = page(three().into_iter().take(n).collect());
                p.view = view;
                assert!(!p.hints().is_empty());
                p.press(Button::Down, A, B);
                p.press(Button::Right, A, B);
                assert_eq!(p.current().is_some(), n > 0);
            }
        }
    }

    #[test]
    fn a_new_reading_keeps_the_picture_you_are_on() {
        let mut p = Gallery::new(Scope::All, Vec::new(), false, (2026, 10, 24), true);
        assert!(!p.loaded && p.hints() == [("B", "Back")]);
        p.refresh(three());
        assert!(p.loaded);
        p.sel = 1;
        let mut newer = three();
        newer.insert(0, shot("gba", "Alpha", "2026-10-24 19-00-00", 1));
        newer.insert(0, shot("gba", "Alpha", "2026-10-24 20-00-00", 1));
        let on = p.current().unwrap().path.clone();
        p.refresh(newer);
        assert_eq!(p.current().unwrap().path, on, "two new ones came in above it");
        p.refresh(Vec::new());
        assert_eq!((p.sel, p.view), (0, View::List));
    }

    #[test]
    fn scrolling_shows_the_selected_row_whole_and_its_day_heading() {
        let many: Vec<Shot> = (0..40).map(|i| shot("gba", "Alpha", &format!("2026-10-{:02} 10-00-00", 24 - i / 4), 1 + (i % 4) as u32)).collect();
        let mut p = page(many);
        // Room for exactly 4 rows of 100 and nothing else.
        let tops: Vec<usize> = (0..40)
            .map(|i| {
                p.sel = i;
                let top = p.follow(400, (40, 100));
                let rows = p.rows();
                let at = rows.iter().position(|r| *r == Row::Shot(i)).unwrap();
                let used: i32 = rows[top..=at].iter().map(|r| if matches!(r, Row::Day(_)) { 40 } else { 100 }).sum();
                assert!(used <= 400, "row {i} is on screen: {used}");
                top
            })
            .collect();
        assert_eq!(tops[0], 0, "the first heading is shown");
        assert!(tops.windows(2).all(|w| w[1] >= w[0]), "going down never scrolls back up");
        // Back up to the first picture of a later day: its heading comes with it.
        p.sel = 4;
        let top = p.follow(400, (40, 100));
        assert!(matches!(p.rows()[top], Row::Day(_)) || top < p.rows().iter().position(|r| *r == Row::Shot(4)).unwrap());
        assert_eq!(page(Vec::new()).follow(400, (40, 100)), 0);
    }

    #[test]
    fn the_picture_cache_keeps_a_few_and_lets_go_of_the_oldest() {
        let key = |n: i32| -> Key { (PathBuf::from(format!("/s/{n}.png")), 100, 75) };
        let img = || Some(Image { w: 1, h: 1, px: vec![0] });
        let mut c = PictureCache::new(3);
        assert_eq!(c.want(&key(1), 8), Want::Ask);
        assert_eq!(c.want(&key(1), 8), Want::Wait, "asked for already");
        assert!(!c.arrived(&key(9), img()), "not ours");
        for n in 1..=5 {
            if n > 1 {
                assert_eq!(c.want(&key(n), 8), Want::Ask);
            }
            assert!(c.arrived(&key(n), img()));
            assert!(c.len() <= 3, "the cap holds at {n}");
        }
        assert_eq!(c.want(&key(5), 8), Want::Have);
        assert_eq!(c.want(&key(1), 8), Want::Ask, "the first one was let go");
        assert!(c.get(&key(4)).is_some() && c.get(&key(2)).is_none());
        // An unreadable file is remembered as such, and asked for once.
        assert!(c.arrived(&key(1), None));
        assert!(matches!(c.get(&key(1)), Some(None)));
        assert_eq!(c.want(&key(1), 8), Want::Have);
        // At most `room` requests out at a time.
        let mut c = PictureCache::new(8);
        let asked = (0..20).filter(|n| c.want(&key(*n), 4) == Want::Ask).count();
        assert_eq!((asked, c.pending()), (4, 4));
        c.arrived(&key(0), img());
        assert_eq!(c.want(&key(10), 4), Want::Ask, "an answer makes room");
        c.forget(Path::new("/s/0.png"));
        assert!(c.get(&key(0)).is_none());
        assert!(c.owns(&key(10)));
        c.clear();
        assert!(c.is_empty() && c.owns(&key(10)), "an answer on its way is still ours");
    }

    #[test]
    fn small_pictures_are_shown_at_a_whole_multiple() {
        assert_eq!(full_box((160, 144), (1240, 850)), (800, 720));
        assert_eq!(full_box((240, 160), (1240, 850)), (1200, 800));
        assert_eq!(full_box((640, 480), (1240, 850)), (1240, 850), "less than twice: fitted");
        assert_eq!(full_box((1240, 1080), (1240, 850)), (1240, 850));
        assert_eq!(full_box((0, 0), (100, 100)), (100, 100));
    }

    #[test]
    fn delete_takes_one_plain_screenshot_of_the_folder_and_nothing_else() {
        let dir = scratch("delete");
        let one = put(&dir, "gba", "Alpha 2026-10-24 18-05-09.png");
        let other = put(&dir, "gba", "Alpha 2026-10-24 18-05-10.png");
        let notes = put(&dir, "gba", "notes.png");
        let outside_dir = TestDir::new("gallery-outside");
        let outside = outside_dir.join("outside.png");
        std::fs::write(&outside, b"x").unwrap();
        assert!(delete(&dir, &one).is_ok());
        assert!(!one.exists() && other.exists(), "one file");
        assert!(delete(&dir, &one).is_ok(), "gone already is gone");
        assert!(delete(&dir, &notes).is_err() && notes.exists(), "a file that is not named like a screenshot stays");
        assert!(delete(&dir, &outside).is_err() && outside.exists(), "nothing outside the folder");
        assert!(delete(&dir, &dir.join("gba/../gba/Alpha 2026-10-24 18-05-10.png")).is_err() && other.exists(), "no way round the folder");
        let folder = dir.join("gba/Folder 2026-10-24 18-05-11.png");
        std::fs::create_dir_all(&folder).unwrap();
        assert!(delete(&dir, &folder).is_err() && folder.is_dir(), "a folder with a screenshot's name stays");
        #[cfg(unix)]
        {
            let link = dir.join("gba/Link 2026-10-24 18-05-12.png");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            assert!(delete(&dir, &link).is_err() && link.symlink_metadata().is_ok() && outside.exists(), "a link is left alone");
        }
        let _ = std::fs::remove_file(outside);
    }

    #[test]
    fn a_screenshot_becomes_the_shelf_picture_whole_or_not_at_all() {
        let dir = scratch("shelf");
        let shot_file = put(&dir, "gba", "Alpha 2026-10-24 18-05-09.png");
        let dest = dir.join("data/snaps/gba/Alpha.png");
        assert_eq!(use_on_shelf(&shot_file, &dest), Ok(()));
        assert_eq!(std::fs::read(&dest).unwrap(), std::fs::read(&shot_file).unwrap());
        // A damaged file and a missing one leave the picture that is there alone.
        let bad = dir.join("gba/Bad 2026-10-24 18-05-10.png");
        std::fs::write(&bad, b"not a png at all, just words").unwrap();
        assert!(use_on_shelf(&bad, &dest).is_err());
        assert!(use_on_shelf(&dir.join("gba/None 2026-10-24 18-05-11.png"), &dest).is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), std::fs::read(&shot_file).unwrap());
    }
}
