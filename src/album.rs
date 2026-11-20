//! The photo album in a game's case (docs/user/library.md, "Photos of big moments"): when the game's own save shows a
//! big step (a new badge, a boss's remains, a Star Spirit, a gold cup, a level of ten), pocket-ui keeps a small photo of
//! the game's picture at that moment, with the date and a short note ("Badge 3 · 4 h 12 min"). Opening the case shows
//! them, lying in it like real instant photos (`app_album.rs`).
//!
//! This module is everything but the painting:
//! * **What a moment is** ([`moment`]): two reads of the same save (`case_knows::read`) compared, one rule per kind of
//!   game. Only steps a player would put in an album count; a clock that ticks or a level that rises by one never does.
//! * **Where photos are** ([`dir`]): `<data>/album/<system>/<rom stem>/`, one JPEG per photo and `album.tsv`, one line
//!   each (file, time, the note's parts in English: the interface's language is applied when it is painted).
//! * **How many** ([`PER_GAME`], [`PER_GAME_BYTES`], [`PER_SESSION`]): a game keeps its newest photos within both
//!   limits (the oldest go first), a session adds a few at most, and a step that is in the album already never comes
//!   twice (a save loaded from before it and played again).
//! * **The camera** ([`Camera`]): a thread of the player (pocket-core on Linux, the Android player) that compares the
//!   battery save each time the game writes it. The game's thread hands over the bytes it writes anyway and, only when a
//!   moment was found, one copy of its last picture; scaling and encoding happen on the camera's thread at the lowest
//!   priority, so a weak device (H700) never drops a frame for it.
//! * **The table on the case** ([`layout`]): where each photo lies in the open case, so the painter and the physics test
//!   (`tests/scene_rests.rs`) agree that every photo rests on the case and none floats.

use std::io;
use std::path::{Path, PathBuf};

use crate::case_knows::Progress;

/// Settings → Screen & sound → Photos of big moments (settings.json): Off unless switched on (19.11.: optional, never in the way).
pub const KEY: &str = "moment_photos";
/// Settings → Screen & sound → Photos of big moments as the players read it at a game's start: Off unless settings.json in
/// `data_dir` says `moment_photos` (pocket-core reads pocket-ui's, the Android player the handheld view's; neither has
/// the settings module's whole model). Off: no camera, no thread, no file.
/// Cozy extras → Progress from saves off (`cozy_extras`) means no save is read at all, so no photo is taken either.
pub fn on(data_dir: &Path) -> bool {
    let v = std::fs::read(data_dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
    let yes = |x: &serde_json::Value| *x == serde_json::Value::Bool(true) || x.as_str().is_some_and(|s| matches!(s, "yes" | "on" | "true"));
    let no = |x: &serde_json::Value| *x == serde_json::Value::Bool(false) || x.as_str().is_some_and(|s| matches!(s, "no" | "off" | "false"));
    let on = v.as_ref().and_then(|v| v.get(KEY)).is_some_and(yes);
    let saves_off = v.as_ref().and_then(|v| v.get("extras")).and_then(|e| e.get("progress_from_saves")).is_some_and(no);
    on && !saves_off
}

/// Settings → Screen & sound → Photos of big moments → Also in play (`album_eject` over the running game): Off unless settings.json
/// says `moment_in_play` (Variant A, 19.11.: the photo waits for the way back into the case instead).
pub fn moment_in_play(data_dir: &Path) -> bool {
    std::fs::read(data_dir.join("settings.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("moment_in_play").cloned())
        .is_some_and(|v| v == serde_json::Value::Bool(true) || v.as_str().is_some_and(|s| matches!(s, "yes" | "on" | "true")))
}

/// When the case was last opened (Unix seconds, 0: never): photos taken after it carry a small "New" mark.
pub fn seen(dir: &Path) -> i64 {
    std::fs::read_to_string(dir.join(SEEN)).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

/// The case was opened at `at`: what is in it now is seen.
pub fn mark_seen(dir: &Path, at: i64) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    crate::fsutil::write_atomic(&dir.join(SEEN), at.to_string().as_bytes())
}

const SEEN: &str = "seen";

/// The most photos a game keeps; a new one beyond replaces the oldest.
pub const PER_GAME: usize = 30;
/// The most bytes a game's photos take (about 30 photos of a GBA game at 30 KB): the oldest go first beyond it.
pub const PER_GAME_BYTES: u64 = 1_500_000;
/// The most photos one session adds: a speedrun through a save editor never fills the album.
pub const PER_SESSION: usize = 5;
/// The widest a kept photo is; the game's picture is halved until it fits (a PS1 game's 640 becomes 320).
pub const MAX_W: u32 = 320;
/// The JPEG quality the photos are kept at.
pub const QUALITY: u8 = 82;

const INDEX: &str = "album.tsv";

/// One photo in a game's album.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Photo {
    /// The picture's file, beside the index.
    pub file: PathBuf,
    /// When it was taken, Unix seconds.
    pub at: i64,
    /// The note's parts in English ("Badge 3", "4 h 12 min"), each translated on its own when painted.
    pub note: Vec<String>,
    /// The picture's size, as kept (its frame takes its shape: a Game Boy Advance's photo is a wide one).
    pub size: (u32, u32),
}

impl Photo {
    /// The note in the interface's language: its parts, each looked up, joined with " · " (as `case_knows_read::line`).
    pub fn note_line(&self) -> String {
        self.note.iter().map(|p| crate::i18n::tr(p)).collect::<Vec<_>>().join(" · ")
    }
}

/// The folder of one game's album.
pub fn dir(data_dir: &Path, system: &str, rom: &Path) -> PathBuf {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
    data_dir.join("album").join(clean(system)).join(clean(stem))
}

/// A name as a folder: no separators, never "." or "..".
fn clean(s: &str) -> String {
    let c: String = s.chars().map(|c| if matches!(c, '/' | '\\' | '\0' | '\t' | '\n') { '_' } else { c }).collect();
    if c.is_empty() || c == "." || c == ".." {
        "_".to_string()
    } else {
        c
    }
}

/// The album's photos, oldest first. Lines whose picture is gone are left out; an album that was never made is empty.
pub fn list(dir: &Path) -> Vec<Photo> {
    let text = std::fs::read_to_string(dir.join(INDEX)).unwrap_or_default();
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let (file, at) = (f.next()?, f.next()?.parse().ok()?);
            if file.is_empty() || file.contains('/') || file.starts_with('.') {
                return None;
            }
            let size = f.next().and_then(|s| s.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))).unwrap_or((4, 3));
            let note: Vec<String> = f.filter(|p| !p.is_empty()).map(str::to_string).collect();
            let file = dir.join(file);
            file.is_file().then_some(Photo { file, at, note, size })
        })
        .collect()
}

/// How many photos the album holds (`list`'s length; a few hundred bytes read).
pub fn count(dir: &Path) -> usize {
    list(dir).len()
}

fn write_index(dir: &Path, photos: &[Photo]) -> io::Result<()> {
    let text: String = photos
        .iter()
        .filter_map(|p| {
            let name = p.file.file_name()?.to_str()?;
            let parts: Vec<String> = p.note.iter().map(|s| s.replace(['\t', '\n'], " ")).collect();
            Some(format!("{name}\t{}\t{}x{}\t{}\n", p.at, p.size.0, p.size.1, parts.join("\t")))
        })
        .collect();
    crate::fsutil::write_atomic(&dir.join(INDEX), text.as_bytes())
}

/// Puts a photo into the album: the picture's file first (written whole or not at all), then the index. Beyond
/// [`PER_GAME`] photos or [`PER_GAME_BYTES`] the oldest are taken out (index first, then their files).
pub fn add(dir: &Path, at: i64, note: &[String], size: (u32, u32), jpeg: &[u8]) -> io::Result<Photo> {
    std::fs::create_dir_all(dir)?;
    let mut photos = list(dir);
    let mut n = 0u32;
    let file = loop {
        let name = if n == 0 { format!("{at}.jpg") } else { format!("{at}-{n}.jpg") };
        let path = dir.join(name);
        if !path.exists() {
            break path;
        }
        n += 1;
    };
    crate::fsutil::write_atomic(&file, jpeg)?;
    let photo = Photo { file, at, note: note.to_vec(), size };
    photos.push(photo.clone());
    let size = |p: &Photo| std::fs::metadata(&p.file).map_or(0, |m| m.len());
    let mut total: u64 = photos.iter().map(size).sum();
    let mut gone = Vec::new();
    while photos.len() > 1 && (photos.len() > PER_GAME || total > PER_GAME_BYTES) {
        let old = photos.remove(0);
        total = total.saturating_sub(size(&old));
        gone.push(old.file);
    }
    write_index(dir, &photos)?;
    for f in gone {
        let _ = std::fs::remove_file(f);
    }
    Ok(photo)
}

/// Takes one photo out of the album: the index first, then the file.
pub fn delete(dir: &Path, file: &Path) -> io::Result<()> {
    let photos: Vec<Photo> = list(dir).into_iter().filter(|p| p.file != file).collect();
    write_index(dir, &photos)?;
    match std::fs::remove_file(file) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Whether a step is in the album already (its first part, "Badge 3"): a save loaded from before it and played again
/// doesn't take the same photo twice.
pub fn has(photos: &[Photo], note: &[String]) -> bool {
    note.first().is_some_and(|first| photos.iter().any(|p| p.note.first() == Some(first)))
}

/// The game's own clock as the note's second part ("4 h 12 min"), where the save keeps one.
fn clock(p: &Progress) -> Option<String> {
    let (h, m) = match p {
        Progress::Pokemon { hours, minutes, .. }
        | Progress::Items { hours, minutes, .. }
        | Progress::StarSpirits { hours, minutes, .. }
        | Progress::Hero { hours, minutes, .. }
        | Progress::Chapter { hours, minutes, .. }
        | Progress::Jiggies { hours, minutes, .. } => (*hours, *minutes),
        Progress::Complete { clock: Some((h, m)), .. } => (*h, *m),
        _ => return None,
    };
    (h > 0 || m > 0).then(|| crate::play_time::duration(h as u64 * 3600 + m as u64 * 60))
}

/// Whether `to` reached a new multiple of `step` (or the end, `of`) coming from `from`.
fn milestone(from: u16, to: u16, step: u16, of: u16) -> bool {
    to > from && (to / step > from / step || (of > 0 && to >= of))
}

/// A level of ten reached by the same leader ("Charizard reached Lv. 50").
fn level_up(before: &Option<(String, u8)>, after: &Option<(String, u8)>) -> Option<String> {
    match (before, after) {
        (Some((a, la)), Some((b, lb))) if a == b && milestone(*la as u16, *lb as u16, 10, 100) => Some(format!("{b} reached Lv. {lb}")),
        _ => None,
    }
}

/// Super Mario 64's stars that open something: the first, the doors of 8, 30, 50 and 70, and all 120.
const STAR_DOORS: [u8; 6] = [1, 8, 30, 50, 70, 120];

/// What the save says happened between two reads of it, as the note's parts in English (the step first, then the
/// game's clock where it has one); None when nothing an album would keep happened. One step per change: the biggest
/// (a badge before the level its leader reached on the way).
pub fn moment(before: &Progress, after: &Progress) -> Option<Vec<String>> {
    use Progress as P;
    let step: Option<String> = match (before, after) {
        (P::Pokemon { badges: a, lead: la, .. }, P::Pokemon { badges: b, lead: lb, .. }) => {
            if b > a {
                Some(format!("Badge {b}"))
            } else {
                level_up(la, lb)
            }
        }
        (P::Items { percent: a, .. }, P::Items { percent: b, .. }) => milestone(*a as u16, *b as u16, 10, 100).then(|| format!("{b}% items")),
        (P::Exits { done: a, .. }, P::Exits { done: b, of }) => milestone(*a as u16, *b as u16, 10, *of as u16).then(|| format!("{b} of {of} exits")),
        (P::Stars { done: a, .. }, P::Stars { done: b, of }) => {
            (b > a && STAR_DOORS.iter().any(|d| a < d && b >= d)).then(|| format!("{b} of {of} stars"))
        }
        (P::GoldCups { done: a, .. }, P::GoldCups { done: b, of }) => (b > a).then(|| format!("{b} of {of} gold cups")),
        (P::Medals { normal: a, expert: ea }, P::Medals { normal: b, expert: eb }) => {
            if eb > ea {
                Some(format!("{eb} in Expert"))
            } else {
                (b > a).then(|| if *b == 1 { "1 medal".to_string() } else { format!("{b} medals") })
            }
        }
        (P::Balloons { done: a, .. }, P::Balloons { done: b, of }) => milestone(*a as u16, *b as u16, 5, *of as u16).then(|| format!("{b} of {of} golden balloons")),
        (P::Spirit { stones: sa, medallions: ma, .. }, P::Spirit { stones: sb, medallions: mb, .. }) => {
            if mb > ma {
                Some(format!("{mb} of {} medallions", 6))
            } else {
                (sb > sa).then(|| format!("{sb} of {} spiritual stones", 3))
            }
        }
        (P::StarSpirits { rescued: a, .. }, P::StarSpirits { rescued: b, .. }) => (b > a).then(|| format!("{b} of {} Star Spirits", 7)),
        (P::Hero { lead: la, .. }, P::Hero { lead: lb, .. }) => level_up(la, lb),
        (P::Party { lead: la, friends: fa }, P::Party { lead: lb, friends: fb }) => {
            if fb > fa {
                Some(if *fb == 1 { "1 friend".to_string() } else { format!("{fb} friends") })
            } else {
                level_up(&Some(la.clone()), &Some(lb.clone()))
            }
        }
        (P::Quest { pendants: pa, crystals: ca, dark_world: da, .. }, P::Quest { pendants: pb, crystals: cb, dark_world: db, .. }) => {
            if *db && !*da {
                Some("Agahnim beaten".to_string())
            } else if *db && cb > ca {
                Some(format!("{cb} of {} crystals", 7))
            } else {
                (!*db && pb > pa).then(|| format!("{pb} of {} pendants", 3))
            }
        }
        (P::Instruments { done: a, .. }, P::Instruments { done: b, .. }) => (b > a).then(|| format!("{b} of {} instruments", 8)),
        (P::Elements { done: a, .. }, P::Elements { done: b, .. }) => (b > a).then(|| format!("{b} of {} elements", 4)),
        (P::Remains { done: a, .. }, P::Remains { done: b, .. }) => (b > a).then(|| format!("{b} of {} remains", 4)),
        (P::Essences { done: a, .. }, P::Essences { done: b, .. }) => (b > a).then(|| format!("{b} of {} essences", 8)),
        (P::Shards { done: a }, P::Shards { done: b }) => (b > a).then(|| format!("{b} of {} mirror shards", 8)),
        (P::Chapter { chapter: a, .. }, P::Chapter { chapter: Some(b), .. }) => (a.as_ref() != Some(b)).then(|| b.clone()),
        (P::Complete { percent: a, .. }, P::Complete { percent: b, of, .. }) => milestone(*a as u16, *b as u16, 10, *of as u16).then(|| format!("{b}% complete")),
        (P::Jiggies { done: a, .. }, P::Jiggies { done: b, of, .. }) => milestone(*a as u16, *b as u16, 10, *of as u16).then(|| format!("{b} Jiggies")),
        (P::Mission { mission: a, difficulty: da }, P::Mission { mission: Some(b), difficulty: db }) => {
            (a.as_ref() != Some(b) || da != db).then(|| format!("{b} · {db}"))
        }
        _ => None,
    };
    let step = step?;
    // A step that holds two parts already ("Mission 2.i: Surface · Agent") keeps them apart.
    let mut out: Vec<String> = step.split(" · ").map(str::to_string).collect();
    out.extend(clock(after));
    Some(out)
}

/// A photo's place in the open case, in the case's own units (0..1 across its width and height): the middle, its turn
/// in degrees (+ clockwise), its size as a share of the case's height, and its height in the pile (0 the bottom).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub index: usize,
    pub cx: f32,
    pub cy: f32,
    pub deg: f32,
    pub z: usize,
}

/// A photo's turn: a few degrees either way, the same for the same photo every time (its time decides).
pub fn tilt(at: i64) -> f32 {
    let h = (at as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 59; // 0..31
    (h as f32 / 31.0 - 0.5) * 9.0
}

/// The frame's art (`assets/media/album/polaroid.png`): its size and the picture's window in it (x, y, w, h).
pub const FRAME: (i32, i32) = (84, 99);
pub const WINDOW: (i32, i32, i32, i32) = (5, 5, 74, 72);

/// A photo's window width at the art's scale for a picture of `size`: as wide as the picture's shape at the window's
/// height, as instant film comes square and wide (a Game Boy's picture fills a square one, a Game Boy Advance's a wide one).
pub fn window_w(size: (u32, u32)) -> i32 {
    let a = size.0.max(1) as f32 / size.1.max(1) as f32;
    ((WINDOW.3 as f32 * a).round() as i32).clamp(56, 120)
}

/// The frame's width at the art's scale for a picture of `size`.
pub fn frame_w(size: (u32, u32)) -> i32 {
    FRAME.0 - WINDOW.2 + window_w(size)
}

/// How tall a photo lies in the case at most, as a share of the case's height (a real instant photo on a DVD case's
/// inside is about 0.56); the painter picks the nearest whole multiple of the frame's art (`album_paint::plan`).
pub const PHOTO_H: f32 = 0.62;

/// Where the photos lie in the open case with `sel` on top: the chosen one in the middle, its two neighbours each side
/// fanned out under it, the rest of the pile hidden under those (only the five nearest are painted). Sorted bottom to
/// top. `aspect`: the case's width over its height; `photo_h`: a photo's height as a share of the case's.
pub fn layout(photos: &[Photo], sel: usize, aspect: f32, photo_h: f32) -> Vec<Placed> {
    layout_at(photos, sel as f32, aspect, photo_h)
}

/// [`layout`] on the way from one photo to the next: `sel` between two places (a Left or Right sliding).
pub fn layout_at(photos: &[Photo], sel: f32, aspect: f32, photo_h: f32) -> Vec<Placed> {
    if photos.is_empty() {
        return Vec::new();
    }
    let sel = sel.clamp(0.0, (photos.len() - 1) as f32);
    // The widest photo decides the spread.
    let widest = photos.iter().map(|p| frame_w(p.size)).max().unwrap_or(FRAME.0) as f32 / FRAME.1 as f32;
    let w = photo_h * widest / aspect;
    // The spread: neighbours under a third of a photo apart, never past the case's edge (with room for a turn).
    let step = (w * 0.42).min((1.0 - w * 1.12) / 4.0).max(0.0);
    let lo = (sel.floor() as i64 - 2).max(0) as usize;
    let hi = ((sel.ceil() as usize) + 3).min(photos.len());
    let mut out: Vec<Placed> = (lo..hi)
        .filter_map(|i| {
            let off = i as f32 - sel;
            let dist = off.abs();
            (dist <= 2.0).then(|| Placed {
                index: i,
                cx: 0.5 + off * step,
                cy: 0.5 + 0.02 * dist,
                deg: tilt(photos[i].at) * (0.4 + 0.6 * dist.min(1.0)),
                z: ((2.0 - dist) * 100.0) as usize,
            })
        })
        .collect();
    out.sort_by_key(|p| p.z);
    out
}

/// The four corners of a placed photo in the case's units (`aspect` its width over height, `photo_h` the photo's
/// height as a share of the case's), for the physics test and the painter's bounds.
pub fn corners(p: &Placed, photo: &Photo, aspect: f32, photo_h: f32) -> [(f32, f32); 4] {
    let (hw, hh) = (photo_h * frame_w(photo.size) as f32 / FRAME.1 as f32 / 2.0, photo_h / 2.0);
    let (sin, cos) = p.deg.to_radians().sin_cos();
    [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|(x, y)| {
        // Turned in a square space (the case's height as the unit), then back to the case's width.
        let (rx, ry) = (x * cos - y * sin, x * sin + y * cos);
        (p.cx + rx / aspect, p.cy + ry)
    })
}

/// What the card knows of a game's album: its folder, the photos (read when the card opens: a few hundred bytes), whether
/// its save is one pocket-ui reads (else the empty case says so) and whether the switch is on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlbumData {
    pub dir: PathBuf,
    pub photos: Vec<Photo>,
    pub reads_save: bool,
    pub on: bool,
    /// Cozy extras → Progress from saves: off, no save is read (and no photo taken).
    pub saves_read: bool,
    /// When the case was last opened (`seen`): newer photos are marked "New".
    pub seen: i64,
}

impl AlbumData {
    /// The album of `rom`, read now.
    pub fn read(data_dir: &Path, system: &str, rom: &Path, reads_save: bool, on: bool, saves_read: bool) -> AlbumData {
        let dir = dir(data_dir, system, rom);
        AlbumData { photos: list(&dir), seen: seen(&dir), dir, reads_save, on, saves_read }
    }

    /// The sentence an empty case shows (never an empty case without a word).
    pub fn empty_line(&self) -> &'static str {
        if !self.saves_read {
            EMPTY_NO_SAVES
        } else if !self.on {
            EMPTY_OFF
        } else if self.reads_save {
            EMPTY
        } else {
            EMPTY_UNREAD
        }
    }
}

/// An empty case of a game whose save pocket-ui reads.
pub const EMPTY: &str = "No photos yet. When your save shows a new badge, a boss beaten or a new level, a photo of that moment lands here.";
/// An empty case of a game whose save pocket-ui doesn't read (yet).
pub const EMPTY_UNREAD: &str = "No photos yet. pocket-ui takes them for games whose save it can read, and this one isn't among them yet.";
/// An empty case while Cozy extras → Progress from saves is off (no save is read).
pub const EMPTY_NO_SAVES: &str = "No photos yet. They come from your saves, and Progress from saves is off: Settings › Look › Cozy extras.";
/// An empty case while Settings → Screen & sound → Photos of big moments is off.
pub const EMPTY_OFF: &str = "No photos yet. Photos of big moments are off: Settings › Screen & sound.";

/// What the open case shows: the photos lying in it, one held up close, or the question before one is thrown away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Case,
    Close,
    Delete { from_close: bool },
}

/// The open case: which photo is on top, what is shown, and the last move (for the slide).
#[derive(Clone, Debug)]
pub struct AlbumView {
    pub sel: usize,
    pub mode: Mode,
    /// When the case was opened (it lies down and opens: `album_paint::OPEN`).
    pub opened_at: Option<std::time::Instant>,
    /// The last Left or Right: when, and which way (the photos slide from there).
    pub moved: Option<(std::time::Instant, i32)>,
}

/// What a press in the open case asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlbumOut {
    None,
    /// B in the case: shut it, back to the box.
    Shut,
    /// The second A of a delete: this photo goes.
    Delete(PathBuf),
}

impl AlbumView {
    /// Opened on the newest photo (the last in the list).
    pub fn new(n: usize) -> AlbumView {
        let dur = crate::motion::span(crate::motion::Kind::Move, OPEN);
        AlbumView { sel: n.saturating_sub(1), mode: Mode::Case, opened_at: (!dur.is_zero()).then(crate::frame_clock::now), moved: None }
    }

    /// How far the case has opened, 0 to 1.
    pub fn openness(&self, now: std::time::Instant) -> f32 {
        let dur = crate::motion::span(crate::motion::Kind::Move, OPEN);
        match self.opened_at {
            Some(at) if !dur.is_zero() => (now.saturating_duration_since(at).as_secs_f32() / dur.as_secs_f32()).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    /// How far the last move has come, 0 to 1 (1: at rest), and its way.
    pub fn slide(&self, now: std::time::Instant) -> (f32, i32) {
        let dur = crate::motion::span(crate::motion::Kind::Move, SLIDE);
        match self.moved {
            Some((at, way)) if !dur.is_zero() => ((now.saturating_duration_since(at).as_secs_f32() / dur.as_secs_f32()).clamp(0.0, 1.0), way),
            _ => (1.0, 0),
        }
    }

    pub fn animating(&self, now: std::time::Instant) -> bool {
        self.openness(now) < 1.0 || self.slide(now).0 < 1.0
    }

    /// A button in the open case with `photos` in it.
    pub fn press(&mut self, b: crate::pad::Button, photos: &[Photo]) -> AlbumOut {
        use crate::pad::Button as B;
        let n = photos.len();
        self.sel = self.sel.min(n.saturating_sub(1));
        match (self.mode, b) {
            (Mode::Delete { from_close }, B::East) => {
                let file = photos.get(self.sel).map(|p| p.file.clone());
                self.mode = if from_close && n > 1 { Mode::Close } else { Mode::Case };
                // The one after it comes on top; the last one's neighbour before it.
                if self.sel + 1 >= n {
                    self.sel = self.sel.saturating_sub(1);
                }
                file.map_or(AlbumOut::None, AlbumOut::Delete)
            }
            (Mode::Delete { from_close }, B::South) => {
                self.mode = if from_close { Mode::Close } else { Mode::Case };
                AlbumOut::None
            }
            (Mode::Delete { .. }, _) => AlbumOut::None,
            (_, B::Left | B::Right) if n > 0 => {
                let way = if b == B::Right { 1 } else { -1 };
                let to = (self.sel as i32 + way).clamp(0, n as i32 - 1) as usize;
                if to != self.sel {
                    self.sel = to;
                    let dur = crate::motion::span(crate::motion::Kind::Move, SLIDE);
                    self.moved = (!dur.is_zero()).then(|| (crate::frame_clock::now(), way));
                }
                AlbumOut::None
            }
            (Mode::Case, B::East) if n > 0 => {
                self.mode = Mode::Close;
                AlbumOut::None
            }
            (Mode::Close, B::East | B::South) => {
                self.mode = Mode::Case;
                AlbumOut::None
            }
            (m, B::North) if n > 0 => {
                self.mode = Mode::Delete { from_close: m == Mode::Close };
                AlbumOut::None
            }
            (Mode::Case, B::South | B::Up) => AlbumOut::Shut,
            _ => AlbumOut::None,
        }
    }

    /// The button hints of what is shown: only keys that do something here.
    pub fn hints(&self, n: usize) -> Vec<(&'static str, &'static str)> {
        match self.mode {
            Mode::Delete { .. } => vec![("A", "Delete"), ("B", "Keep")],
            Mode::Close => {
                let mut h = vec![("B", "Back"), ("X", "Delete")];
                if n > 1 {
                    h.insert(0, ("LEFT RIGHT", "Browse"));
                }
                h
            }
            Mode::Case if n == 0 => vec![("B", "Close")],
            Mode::Case => {
                let mut h = vec![("A", "Look closer"), ("B", "Close"), ("X", "Delete")];
                if n > 1 {
                    h.insert(0, ("LEFT RIGHT", "Browse"));
                }
                h
            }
        }
    }
}

/// How long the case takes to lie down and open (Settings → Look → Motion: Off at once).
pub const OPEN: std::time::Duration = std::time::Duration::from_millis(260);
/// How long a photo takes to slide onto the pile's top.
pub const SLIDE: std::time::Duration = std::time::Duration::from_millis(180);

/// A picture for the camera: pocket-core's last frame as the core drew it, or the Android player's (0RGB).
pub enum Picture {
    Frame(crate::libretro::Frame),
    Image(std::sync::Arc<crate::image::Image>),
}

impl Picture {
    /// Packed 8-bit RGB and its size; None for an empty or short one.
    fn rgb(&self) -> Option<(u32, u32, Vec<u8>)> {
        match self {
            Picture::Frame(f) => {
                let bpp = if f.format == crate::libretro::PIXEL_XRGB8888 { 4 } else { 2 };
                let (w, h) = (f.width as usize, f.height as usize);
                (w > 0 && h > 0 && f.pitch >= w * bpp && f.data.len() >= f.pitch * (h - 1) + w * bpp).then(|| (f.width, f.height, f.to_rgb()))
            }
            Picture::Image(i) => (i.w > 0 && i.h > 0 && i.px.len() >= (i.w * i.h) as usize)
                .then(|| (i.w, i.h, i.px.iter().take((i.w * i.h) as usize).flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect())),
        }
    }
}

/// The picture as the album keeps it: halved (each pixel the mean of four) until it is at most [`MAX_W`] wide, as JPEG.
pub fn encode(w: u32, h: u32, rgb: &[u8]) -> Option<(Vec<u8>, (u32, u32))> {
    let (mut w, mut h, mut px) = (w as usize, h as usize, rgb.to_vec());
    while w > MAX_W as usize && w >= 2 && h >= 2 {
        let (nw, nh) = (w / 2, h / 2);
        let mut out = vec![0u8; nw * nh * 3];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..3 {
                    let at = |xx: usize, yy: usize| px[(yy * w + xx) * 3 + c] as u32;
                    out[(y * nw + x) * 3 + c] = ((at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1) + 2) / 4) as u8;
                }
            }
        }
        (w, h, px) = (nw, nh, out);
    }
    let mut jpeg = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut jpeg, QUALITY);
    enc.encode(&px, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).ok()?;
    Some((jpeg, (w as u32, h as u32)))
}

/// What the camera's thread is handed.
enum Msg {
    /// The battery save as the game just wrote it (only when it changed), and its number.
    Save(Vec<u8>, usize),
    Picture(Picture),
}

/// What the camera's thread has done: the number of the last save compared, the photos kept.
#[derive(Default)]
struct Done {
    compared: usize,
    taken: usize,
    /// The photo just kept, made up for the moment over the game (`album_eject`), until the game's thread takes it.
    moment: Option<std::sync::Arc<crate::album_eject::Stages>>,
}

type Shared = std::sync::Arc<(std::sync::Mutex<Done>, std::sync::Condvar)>;

/// The album's camera in a player (pocket-core, the Android player): started with the game, it compares each battery
/// save the game writes with the one before (`moment`) and, at a moment, asks the game's thread for one picture
/// ([`Camera::wants_picture`]), which it shrinks, encodes and puts into the album on its own thread.
pub struct Camera {
    tx: std::sync::mpsc::SyncSender<Msg>,
    want: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The thread found the game is one it reads: until then (and for every other game) nothing is copied for it.
    armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    sent: std::sync::atomic::AtomicUsize,
    done: Shared,
    /// The screen's height, for the moment's size (`set_screen`); 0: no moment is made up.
    screen_h: std::sync::Arc<std::sync::atomic::AtomicI32>,
}

/// How long the camera waits for its picture before it lets the moment go.
const PICTURE_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

impl Camera {
    /// The camera for `rom`, or None when Settings → Screen & sound → Photos of big moments is off (`on`: pocket-core reads
    /// `settings::moment_photos`, the Android player the handheld view's). Its thread reads the game's release from the
    /// caches only (never the ROM through while the game runs) and the save as it is now; `data_dir` holds those caches
    /// and the album.
    pub fn start(data_dir: &Path, system: &str, rom: &Path, on: bool) -> Option<Camera> {
        if !on {
            return None;
        }
        let (tx, rx) = std::sync::mpsc::sync_channel::<Msg>(4);
        let want = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done: Shared = Default::default();
        let (data_dir, system, rom) = (data_dir.to_path_buf(), system.to_string(), rom.to_path_buf());
        let screen_h = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));
        let (w2, a2, d2, h2) = (want.clone(), armed.clone(), done.clone(), screen_h.clone());
        std::thread::Builder::new()
            .name("album".into())
            .spawn(move || {
                lowest_priority();
                run(rx, &data_dir, &system, &rom, &w2, &a2, &d2, &h2);
            })
            .ok()?;
        Some(Camera { tx, want, armed, sent: Default::default(), done, screen_h })
    }

    /// The game is one whose save is read: the player keeps a recent picture for it (`wants_picture` may come).
    pub fn armed(&self) -> bool {
        self.armed.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The game wrote its battery save and it changed: a copy for the comparison. Never waits: a full queue drops it
    /// (the next write is compared with the last one read, so a step is still found then).
    pub fn saved(&self, bytes: &[u8]) {
        if self.armed() {
            let n = self.sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let _ = self.tx.try_send(Msg::Save(bytes.to_vec(), n));
        }
    }

    /// A moment was found: the thread waits for a picture (`picture`).
    pub fn wants_picture(&self) -> bool {
        self.want.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn picture(&self, p: Picture) {
        if self.want.swap(false, std::sync::atomic::Ordering::Relaxed) {
            let _ = self.tx.try_send(Msg::Picture(p));
        }
    }

    /// The screen the game is shown on is `h` tall: a kept photo is made up for the moment over the game at its size.
    pub fn set_screen(&self, h: i32) {
        self.screen_h.store(h, std::sync::atomic::Ordering::Relaxed);
    }

    /// A photo was just kept: its stages for the moment over the game (once).
    pub fn take_moment(&self) -> Option<std::sync::Arc<crate::album_eject::Stages>> {
        self.done.0.lock().unwrap_or_else(|e| e.into_inner()).moment.take()
    }

    /// How many photos the camera put into the album so far.
    pub fn taken(&self) -> usize {
        self.done.0.lock().unwrap_or_else(|e| e.into_inner()).taken
    }

    /// At the game's end: the last save compared and its photo written before the process goes, waiting at most
    /// `wait`; the picture is made only if a moment asks for it (the game's thread can't be asked again). Returns as soon
    /// as the save is compared and nothing waits.
    pub fn finish(&self, last_save: &[u8], picture: impl FnOnce() -> Option<Picture>, wait: std::time::Duration) {
        if !self.armed() {
            return;
        }
        let until = std::time::Instant::now() + wait;
        let n = self.sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if self.tx.send(Msg::Save(last_save.to_vec(), n)).is_err() {
            return;
        }
        let mut picture = Some(picture);
        let (lock, cv) = &*self.done;
        let mut d = lock.lock().unwrap_or_else(|e| e.into_inner());
        while std::time::Instant::now() < until {
            if self.wants_picture() {
                drop(d);
                match picture.take().and_then(|f| f()) {
                    Some(p) => self.picture(p),
                    None => return,
                }
                d = lock.lock().unwrap_or_else(|e| e.into_inner());
            }
            if d.compared >= n && !self.wants_picture() {
                return;
            }
            d = cv.wait_timeout(d, std::time::Duration::from_millis(20)).map(|(g, _)| g).unwrap_or_else(|e| e.into_inner().0);
        }
    }
}

/// The calling thread at the lowest CPU priority (Linux and Android alike): the camera only gets what the game leaves.
fn lowest_priority() {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    // SAFETY: setpriority on this thread's own id only changes its scheduling.
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, tid, 19);
    }
}

/// The camera's thread.
#[allow(clippy::too_many_arguments)]
fn run(rx: std::sync::mpsc::Receiver<Msg>, data_dir: &Path, system: &str, rom: &Path, want: &std::sync::atomic::AtomicBool, armed: &std::sync::atomic::AtomicBool, done: &Shared, screen_h: &std::sync::atomic::AtomicI32) {
    use std::sync::atomic::Ordering;
    let mut reading = crate::case_knows_read::Reading::new(data_dir);
    reading.no_hashing = true;
    let Some(release) = reading.verified_release(system, rom) else { return };
    let mut memory = reading.memory(rom);
    let mut before = reading.progress(system, rom);
    let dir = dir(data_dir, system, rom);
    armed.store(true, Ordering::Relaxed);
    let mark = |n: usize, kept: Option<&Path>| {
        // The moment over the game: the photo as the album keeps it, its three stages at the screen's size.
        let h = screen_h.load(Ordering::Relaxed);
        let moment = kept.filter(|_| h > 0).and_then(crate::image::load_jpeg).map(|img| std::sync::Arc::new(crate::album_eject::Stages::new(&img, h)));
        let (lock, cv) = &**done;
        let mut d = lock.lock().unwrap_or_else(|e| e.into_inner());
        d.compared = d.compared.max(n);
        d.taken += kept.is_some() as usize;
        if moment.is_some() {
            d.moment = moment;
        }
        cv.notify_all();
    };
    let mut taken = 0usize;
    let mut next: Option<(Vec<u8>, usize)> = None;
    loop {
        let (bytes, n) = match next.take() {
            Some(s) => s,
            None => match rx.recv() {
                Ok(Msg::Save(b, n)) => (b, n),
                Ok(Msg::Picture(_)) => continue,
                Err(_) => break,
            },
        };
        let (now, mem) = crate::case_knows::read(release, &bytes, memory.as_ref());
        memory = Some(mem);
        let Some(now) = now else {
            mark(n, None);
            continue;
        };
        let note = before.as_ref().and_then(|b| moment(b, &now));
        before = Some(now);
        let Some(note) = note.filter(|note| taken < PER_SESSION && !has(&list(&dir), note)) else {
            mark(n, None);
            continue;
        };
        want.store(true, Ordering::Relaxed);
        let picture = loop {
            match rx.recv_timeout(PICTURE_WAIT) {
                Ok(Msg::Picture(p)) => break Some(p),
                // A newer save while the picture is on its way: compared next, after this photo.
                Ok(Msg::Save(b, m)) => next = Some((b, m)),
                Err(_) => break None,
            }
        };
        want.store(false, Ordering::Relaxed);
        let kept = picture.and_then(|p| p.rgb()).and_then(|(w, h, rgb)| encode(w, h, &rgb)).and_then(|(jpeg, size)| {
            let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
            match add(&dir, at, &note, size, &jpeg) {
                Ok(p) => {
                    println!("album: {} ({})", p.file.display(), note.join(" · "));
                    Some(p.file)
                }
                Err(e) => {
                    eprintln!("album: not kept: {e}");
                    None
                }
            }
        });
        taken += kept.is_some() as usize;
        mark(n, kept.as_deref());
    }
}

#[cfg(test)]
#[path = "album_save_tests.rs"]
mod save_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case_knows::TeamArt;

    fn poke(badges: u8, lead: Option<(&str, u8)>, h: u16, m: u8) -> Progress {
        Progress::Pokemon { badges, lead: lead.map(|(n, l)| (n.to_string(), l)), hours: h, minutes: m, art: TeamArt::default() }
    }

    #[test]
    fn a_new_badge_is_a_moment_with_the_games_clock() {
        let n = moment(&poke(2, Some(("Pikachu", 24)), 3, 50), &poke(3, Some(("Pikachu", 25)), 4, 12));
        assert_eq!(n, Some(vec!["Badge 3".to_string(), "4 h 12 min".to_string()]));
    }

    #[test]
    fn a_level_counts_only_at_ten() {
        assert_eq!(moment(&poke(3, Some(("Pikachu", 25)), 4, 0), &poke(3, Some(("Pikachu", 26)), 4, 5)), None);
        let n = moment(&poke(3, Some(("Pikachu", 29)), 4, 0), &poke(3, Some(("Pikachu", 30)), 4, 5)).unwrap();
        assert_eq!(n[0], "Pikachu reached Lv. 30");
        // Another leader at a higher level is a swap, not a level reached.
        assert_eq!(moment(&poke(3, Some(("Pikachu", 29)), 4, 0), &poke(3, Some(("Onix", 31)), 4, 5)), None);
    }

    #[test]
    fn the_clock_alone_is_never_a_moment() {
        assert_eq!(moment(&poke(3, Some(("Pikachu", 25)), 4, 0), &poke(3, Some(("Pikachu", 25)), 9, 0)), None);
        let items = |p, h| Progress::Items { percent: p, hours: h, minutes: 0 };
        assert_eq!(moment(&items(21, 1), &items(23, 2)), None);
        assert_eq!(moment(&items(29, 1), &items(31, 2)).unwrap()[0], "31% items");
    }

    #[test]
    fn star_doors_and_the_end() {
        let s = |d| Progress::Stars { done: d, of: 120 };
        assert_eq!(moment(&s(0), &s(1)).unwrap(), vec!["1 of 120 stars".to_string()]);
        assert_eq!(moment(&s(2), &s(3)), None);
        assert_eq!(moment(&s(29), &s(30)).unwrap()[0], "30 of 120 stars");
        assert_eq!(moment(&s(119), &s(120)).unwrap()[0], "120 of 120 stars");
    }

    #[test]
    fn going_back_is_never_a_moment() {
        assert_eq!(moment(&poke(4, None, 9, 0), &poke(3, None, 4, 0)), None);
        let s = |d| Progress::Stars { done: d, of: 120 };
        assert_eq!(moment(&s(31), &s(30)), None);
    }

    #[test]
    fn link_to_the_past_agahnim_then_crystals() {
        let q = |p, c, d| Progress::Quest { pendants: p, crystals: c, hearts: 6, dark_world: d };
        assert_eq!(moment(&q(2, 0, false), &q(3, 0, false)).unwrap()[0], "3 of 3 pendants");
        assert_eq!(moment(&q(3, 0, false), &q(3, 0, true)).unwrap()[0], "Agahnim beaten");
        assert_eq!(moment(&q(3, 0, true), &q(3, 1, true)).unwrap()[0], "1 of 7 crystals");
    }

    #[test]
    fn the_album_keeps_its_newest_within_both_limits() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().join("album/gba/Game");
        for i in 0..(PER_GAME as i64 + 4) {
            add(&d, 1_000 + i, &[format!("Badge {i}")], (240, 160), &[0u8; 100]).unwrap();
        }
        let l = list(&d);
        assert_eq!(l.len(), PER_GAME);
        assert_eq!(l[0].note[0], "Badge 4", "the oldest went first");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), PER_GAME + 1, "their files went too");
        // The bytes' limit: a few big photos push the small ones out.
        let big = vec![0u8; (PER_GAME_BYTES / 3) as usize];
        for i in 0..3 {
            add(&d, 5_000 + i, &[format!("Big {i}")], (240, 160), &big).unwrap();
        }
        let l = list(&d);
        let total: u64 = l.iter().map(|p| std::fs::metadata(&p.file).unwrap().len()).sum();
        assert!(total <= PER_GAME_BYTES, "{total}");
        assert_eq!(l.last().unwrap().note[0], "Big 2");
    }

    #[test]
    fn delete_takes_the_line_and_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = add(dir.path(), 10, &["Badge 1".into()], (160, 144), b"x").unwrap();
        add(dir.path(), 20, &["Badge 2".into()], (160, 144), b"y").unwrap();
        delete(dir.path(), &a.file).unwrap();
        let l = list(dir.path());
        assert_eq!(l.len(), 1);
        assert!(!a.file.exists());
        assert!(has(&l, &["Badge 2".to_string()]));
        assert!(!has(&l, &["Badge 1".to_string()]));
    }

    #[test]
    fn the_players_switch_follows_both_settings() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!on(dir.path()), "no settings: off (a new device)");
        let w = |j: &str| std::fs::write(dir.path().join("settings.json"), j).unwrap();
        w(r#"{"moment_photos": true}"#);
        assert!(on(dir.path()));
        w(r#"{"moment_photos": true, "extras": {"progress_from_saves": false}}"#);
        assert!(!on(dir.path()), "no save is read: no photo either");
        w(r#"{"extras": {"progress_from_saves": true}}"#);
        assert!(!on(dir.path()), "not switched on");
        w("{ broken");
        assert!(!on(dir.path()));
    }

    /// Off (as on every new device): no camera, so no thread and nothing copied, and no file or folder in the data folder.
    #[test]
    fn off_there_is_no_camera_and_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let rom = dir.path().join("Game.gba");
        assert!(Camera::start(dir.path(), "gba", &rom, on(dir.path())).is_none(), "off by default: no camera");
        let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert!(names.is_empty(), "nothing written: {names:?}");
    }

    /// An empty case always says why, in a sentence: never an empty case without a word.
    #[test]
    fn an_empty_case_always_has_a_sentence_for_its_reason() {
        let d = |reads_save, on, saves_read| AlbumData { reads_save, on, saves_read, ..Default::default() }.empty_line();
        assert_eq!(d(true, true, true), EMPTY);
        assert_eq!(d(false, true, true), EMPTY_UNREAD);
        assert_eq!(d(true, false, true), EMPTY_OFF);
        assert_eq!(d(true, true, false), EMPTY_NO_SAVES);
        for s in [EMPTY, EMPTY_UNREAD, EMPTY_OFF, EMPTY_NO_SAVES] {
            assert!(s.starts_with("No photos yet.") && s.len() > 40, "{s}");
        }
    }

    #[test]
    fn the_moment_during_play_is_off_until_switched_on() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!moment_in_play(dir.path()), "a new device: off");
        std::fs::write(dir.path().join("settings.json"), r#"{"moment_in_play": true}"#).unwrap();
        assert!(moment_in_play(dir.path()));
    }

    #[test]
    fn photos_after_the_last_opening_are_new() {
        let dir = tempfile::tempdir().unwrap();
        add(dir.path(), 100, &["Badge 1".into()], (160, 144), b"x").unwrap();
        assert_eq!(seen(dir.path()), 0, "never opened: everything is new");
        mark_seen(dir.path(), 150).unwrap();
        add(dir.path(), 200, &["Badge 2".into()], (160, 144), b"y").unwrap();
        let s = seen(dir.path());
        let new: Vec<_> = list(dir.path()).into_iter().filter(|p| p.at > s).map(|p| p.note[0].clone()).collect();
        assert_eq!(new, vec!["Badge 2".to_string()]);
        assert!(mark_seen(&dir.path().join("none"), 1).is_ok(), "an album never made: nothing to mark, no folder made");
        assert!(!dir.path().join("none").exists());
    }

    #[test]
    fn a_missing_or_damaged_index_is_an_empty_album() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list(dir.path()).is_empty());
        std::fs::write(dir.path().join(INDEX), "../../etc/passwd\t1\tx\n\tno\n\u{0}garbage").unwrap();
        assert!(list(dir.path()).is_empty());
    }
}
