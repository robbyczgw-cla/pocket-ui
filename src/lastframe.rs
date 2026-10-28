//! The last frame you saw of each game, for the game switcher.
//!
//! Where the picture of a recent game comes from, best first:
//! 1. the preview next to the auto-resume state (`<game>.pstate.auto.png`, RetroArch's `.state.auto.png`),
//!    written by the emulator on every clean exit;
//! 2. the frame pocket-ui kept at Quit (`<data>/frames/<system>/<rom stem>.png`): the paused picture the pause
//!    menu grabbed with grim anyway, cropped to the game and shrunk to 640 x 480 at most. This is what games
//!    of the standalone emulators (their previews hide behind disc-specific names) and any game without a
//!    resume preview get;
//! 3. the preview of a manual save slot, if it is newer than the kept frame;
//! 4. the downloaded box art;
//! 5. the picture of the system.
//!
//! A game that ended any other way (crash, power loss, hotkey exit) has no kept frame: the one of an earlier
//! session is deleted when a session ends without a new one, so the picture is never older than the
//! last clean quit.

use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::image::Image;

/// Folder of the kept frames inside the data directory.
pub const FRAMES_DIR: &str = "frames";

/// The largest a kept frame gets (the switcher shows it at 760 x 570 at most; a handheld has 4 inches).
pub const MAX_W: i32 = 640;
pub const MAX_H: i32 = 480;

/// Where a game's picture comes from. Listed best first by [`rank`].
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Preview of the auto-resume state.
    AutoPreview(PathBuf),
    /// The frame kept at Quit.
    Frame(PathBuf),
    /// Preview of a manual save slot.
    SlotPreview(PathBuf),
    BoxArt(PathBuf),
    /// The pixel-art picture of the console (always there to fall back on).
    SystemArt,
}

/// What exists for one game; the paths are known to exist.
#[derive(Default)]
pub struct Candidates {
    /// Auto-resume previews, best first.
    pub auto: Vec<PathBuf>,
    /// Manual slot previews and when they were written.
    pub slots: Vec<(PathBuf, SystemTime)>,
    /// The frame kept at Quit and when it was written.
    pub frame: Option<(PathBuf, SystemTime)>,
    pub cover: Option<PathBuf>,
}

/// `<data>/frames/<system>/<rom stem>.png`.
pub fn frame_path(data_dir: &Path, system: &str, rom: &Path) -> PathBuf {
    // Same stem rule as the box art (`covers::cover_path`); a name that is not a plain file name can't leave the folder.
    let stem = rom.file_stem().and_then(|s| s.to_str()).filter(|s| !s.is_empty() && *s != "..").unwrap_or("unknown");
    data_dir.join(FRAMES_DIR).join(system).join(format!("{stem}.png"))
}

/// The sources to try for a game, best first; ends with the system's picture.
pub fn rank(c: &Candidates) -> Vec<Source> {
    let mut out: Vec<Source> = c.auto.iter().cloned().map(Source::AutoPreview).collect();
    // The kept frame and the manual saves: the newest picture first; the same second goes to the save.
    let mut pictures: Vec<(SystemTime, u8, Source)> = c.slots.iter().map(|(p, t)| (*t, 0, Source::SlotPreview(p.clone()))).collect();
    pictures.extend(c.frame.iter().map(|(p, t)| (*t, 1, Source::Frame(p.clone()))));
    pictures.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    out.extend(pictures.into_iter().map(|(_, _, s)| s));
    out.extend(c.cover.iter().cloned().map(Source::BoxArt));
    out.push(Source::SystemArt);
    out
}

/// Whether the session needs a frame of its own: false when the emulator wrote an auto-resume preview during it.
pub fn needs_frame(auto_preview_written: Option<SystemTime>, session_start: SystemTime) -> bool {
    auto_preview_written.is_none_or(|t| t < session_start)
}

/// A screen grab as it is kept: black borders cropped, at most `MAX_W` x `MAX_H`.
pub fn shrink(screen: &Image) -> Image {
    let game = crate::switcher::crop_black(screen);
    if game.w == 0 || game.h == 0 || (game.w as i32 <= MAX_W && game.h as i32 <= MAX_H) {
        return game;
    }
    let scale = (MAX_W as f32 / game.w as f32).min(MAX_H as f32 / game.h as f32);
    game.resized((game.w as f32 * scale).round() as i32, (game.h as f32 * scale).round() as i32)
}

/// Keeps `screen` as the game's last frame. False when it could not be written.
pub fn keep(data_dir: &Path, system: &str, rom: &Path, screen: &Image) -> bool {
    let small = shrink(screen);
    if small.w == 0 || small.h == 0 {
        return false;
    }
    let rgb: Vec<u8> = small.px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    let mut png_bytes = Vec::new();
    let encoded = {
        let mut enc = png::Encoder::new(&mut png_bytes, small.w, small.h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().and_then(|mut w| w.write_image_data(&rgb)).is_ok()
    };
    encoded && crate::fsutil::write_atomic(&frame_path(data_dir, system, rom), &png_bytes).is_ok()
}

/// Deletes the kept frame of a game (its session ended without a new one).
pub fn forget(data_dir: &Path, system: &str, rom: &Path) {
    let _ = std::fs::remove_file(frame_path(data_dir, system, rom));
}

/// A decoded file's identity: a new file under the same name is a new key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileKey {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub len: u64,
}

impl FileKey {
    pub fn of(path: &Path) -> Option<FileKey> {
        let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
        Some(FileKey {
            path: path.to_path_buf(),
            modified: meta.modified().ok(),
            len: meta.len(),
        })
    }
}

/// Least recently used cache with a limit on entries and on bytes. The newest entry always stays.
pub struct Lru<K, V> {
    map: HashMap<K, (V, usize, u64)>,
    max_entries: usize,
    max_bytes: usize,
    bytes: usize,
    tick: u64,
}

impl<K: Eq + Hash + Clone, V> Lru<K, V> {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Lru {
            map: HashMap::new(),
            max_entries,
            max_bytes,
            bytes: 0,
            tick: 0,
        }
    }

    /// The picture, which is then the most recently used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.tick += 1;
        let tick = self.tick;
        self.map.get_mut(key).map(|(v, _, used)| {
            *used = tick;
            &*v
        })
    }

    /// Adds (or replaces) a picture of `bytes` and drops the least recently used ones until both limits hold again.
    pub fn put(&mut self, key: K, value: V, bytes: usize) {
        self.tick += 1;
        if let Some((_, old, _)) = self.map.insert(key, (value, bytes, self.tick)) {
            self.bytes -= old;
        }
        self.bytes += bytes;
        while self.map.len() > 1 && (self.map.len() > self.max_entries || self.bytes > self.max_bytes) {
            let Some(oldest) = self.map.iter().min_by_key(|(_, (_, _, used))| *used).map(|(k, _)| k.clone()) else { break };
            if let Some((_, b, _)) = self.map.remove(&oldest) {
                self.bytes -= b;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::time::Duration;

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    // ---- file naming ----

    #[test]
    fn a_frame_is_named_after_the_system_and_the_rom_stem_in_the_data_dir() {
        let d = Path::new("/storage/.config/pocket-ui");
        assert_eq!(frame_path(d, "gbc", Path::new("/storage/roms/gbc/uCity.gbc")), p("/storage/.config/pocket-ui/frames/gbc/uCity.png"));
        // Dots, brackets and spaces of No-Intro names stay in the name.
        assert_eq!(
            frame_path(d, "snes", Path::new("/r/Super Mario World (U) [!].v1.1.sfc")),
            p("/storage/.config/pocket-ui/frames/snes/Super Mario World (U) [!].v1.1.png")
        );
        // The same layout as the box art, so both are found by the same stem.
        assert_eq!(
            frame_path(d, "ps2", Path::new("/r/ps2/Okami.iso")).file_name(),
            crate::covers::cover_path(d, "ps2", Path::new("/r/ps2/Okami.iso")).file_name()
        );
    }

    #[test]
    fn a_rom_without_a_usable_name_cannot_climb_out_of_the_folder() {
        let d = Path::new("/data");
        let got = frame_path(d, "gb", Path::new("/"));
        assert!(got.starts_with("/data/frames/gb"), "{got:?}");
        assert!(!got.components().any(|c| c.as_os_str() == ".."));
    }

    // ---- which picture a game gets ----

    #[test]
    fn the_resume_preview_comes_first_then_the_kept_frame_then_box_art_then_the_system() {
        let all = Candidates {
            auto: vec![p("/s/g.pstate.auto.png")],
            slots: vec![(p("/s/g.pstate3.png"), t(100))],
            frame: Some((p("/f/g.png"), t(200))),
            cover: Some(p("/c/g.png")),
        };
        assert_eq!(
            rank(&all),
            [
                Source::AutoPreview(p("/s/g.pstate.auto.png")),
                Source::Frame(p("/f/g.png")),
                Source::SlotPreview(p("/s/g.pstate3.png")),
                Source::BoxArt(p("/c/g.png")),
                Source::SystemArt
            ]
        );
        let no_auto = Candidates { auto: vec![], ..all };
        assert_eq!(rank(&no_auto)[0], Source::Frame(p("/f/g.png")));
    }

    #[test]
    fn a_game_never_quit_cleanly_shows_its_box_art_and_without_that_the_system() {
        let never = Candidates {
            cover: Some(p("/c/g.png")),
            ..Default::default()
        };
        assert_eq!(rank(&never), [Source::BoxArt(p("/c/g.png")), Source::SystemArt]);
        assert_eq!(rank(&Candidates::default()), [Source::SystemArt]);
    }

    #[test]
    fn an_older_kept_frame_does_not_hide_a_newer_manual_save() {
        let c = Candidates {
            slots: vec![(p("/s/new.png"), t(500)), (p("/s/old.png"), t(50))],
            frame: Some((p("/f/g.png"), t(300))),
            ..Default::default()
        };
        assert_eq!(
            rank(&c),
            [Source::SlotPreview(p("/s/new.png")), Source::Frame(p("/f/g.png")), Source::SlotPreview(p("/s/old.png")), Source::SystemArt]
        );
        // The same second: the state preview is the older rule and wins the tie.
        let tie = Candidates {
            slots: vec![(p("/s/a.png"), t(300))],
            frame: Some((p("/f/g.png"), t(300))),
            ..Default::default()
        };
        assert_eq!(rank(&tie)[0], Source::SlotPreview(p("/s/a.png")));
    }

    #[test]
    fn a_session_needs_its_own_frame_unless_the_emulator_wrote_a_resume_preview_in_it() {
        let start = t(1000);
        assert!(needs_frame(None, start), "no resume preview at all");
        assert!(needs_frame(Some(t(400)), start), "the preview is from an earlier session");
        assert!(!needs_frame(Some(t(1005)), start), "written during this one");
        assert!(!needs_frame(Some(t(1000)), start));
    }

    // ---- what is kept ----

    fn screen(w: u32, h: u32, lit: (u32, u32, u32, u32)) -> Image {
        let mut px = vec![0u32; (w * h) as usize];
        for y in lit.1..lit.1 + lit.3 {
            for x in lit.0..lit.0 + lit.2 {
                px[(y * w + x) as usize] = 0x4080c0;
            }
        }
        Image { w, h, px }
    }

    #[test]
    fn a_kept_frame_loses_its_black_borders_and_fits_640_by_480() {
        // The Mini V2's screen with a GBA picture in the middle.
        let small = shrink(&screen(1240, 1080, (140, 200, 960, 640)));
        assert!(small.w as i32 <= MAX_W && small.h as i32 <= MAX_H, "{}x{}", small.w, small.h);
        assert_eq!(small.w as i32, MAX_W, "as large as the box allows");
        assert!((small.w as f32 / small.h as f32 - 1.5).abs() < 0.02, "still 3:2: {}x{}", small.w, small.h);
        assert!(small.px.iter().all(|p| *p != 0), "no black border left");
        // A picture already smaller than the box is not blown up.
        let tiny = shrink(&screen(400, 300, (0, 0, 400, 300)));
        assert_eq!((tiny.w, tiny.h), (400, 300));
    }

    #[test]
    fn keeping_a_frame_writes_a_png_that_reads_back_and_forgetting_deletes_it() {
        let dir = TestDir::new("lastframe");
        let rom = Path::new("/roms/gba/Advance Wars.gba");
        assert!(keep(&dir, "gba", rom, &screen(1240, 1080, (140, 200, 960, 640))));
        let file = frame_path(&dir, "gba", rom);
        let back = crate::image::load_png(&file).expect("a png");
        assert_eq!((back.w, back.h), (640, 427));
        assert!(std::fs::metadata(&file).unwrap().len() < 300_000, "small file");
        let names: Vec<_> = std::fs::read_dir(file.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "no temporary file left: {names:?}");
        forget(&dir, "gba", rom);
        assert!(!file.exists());
        forget(&dir, "gba", rom); // nothing there: no complaint
    }

    // ---- cache eviction ----

    #[test]
    fn a_file_that_changed_is_a_new_key() {
        let dir = TestDir::new("filekey");
        let f = dir.join("a.png");
        assert_eq!(FileKey::of(&f), None, "missing file");
        std::fs::write(&f, b"one").unwrap();
        let first = FileKey::of(&f).unwrap();
        assert_eq!(first, FileKey::of(&f).unwrap());
        std::fs::write(&f, b"three").unwrap();
        assert_ne!(first, FileKey::of(&f).unwrap());
    }

    #[test]
    fn the_cache_drops_the_least_recently_used_picture_when_the_bytes_run_out() {
        let mut c: Lru<&str, u32> = Lru::new(100, 1000);
        c.put("a", 1, 400);
        c.put("b", 2, 400);
        assert_eq!(c.get(&"a"), Some(&1), "touching a makes b the oldest");
        c.put("c", 3, 400);
        assert_eq!(c.get(&"b"), None, "b was the least recently used");
        assert_eq!(c.get(&"a"), Some(&1));
        assert_eq!(c.get(&"c"), Some(&3));
        assert_eq!((c.len(), c.bytes()), (2, 800));
    }

    #[test]
    fn the_cache_drops_the_oldest_when_there_are_too_many_entries() {
        let mut c: Lru<u32, u32> = Lru::new(3, usize::MAX);
        for i in 0..5 {
            c.put(i, i, 1);
        }
        assert_eq!(c.len(), 3);
        assert!(c.get(&0).is_none() && c.get(&1).is_none());
        assert!(c.get(&2).is_some() && c.get(&3).is_some() && c.get(&4).is_some());
    }

    #[test]
    fn the_newest_picture_stays_even_when_it_alone_is_over_the_limit_and_replacing_does_not_leak_bytes() {
        let mut c: Lru<&str, u32> = Lru::new(10, 100);
        c.put("a", 1, 60);
        c.put("big", 2, 500);
        assert_eq!(c.get(&"big"), Some(&2));
        assert_eq!(c.get(&"a"), None);
        assert_eq!((c.len(), c.bytes()), (1, 500));
        c.put("big", 3, 200);
        assert_eq!(c.get(&"big"), Some(&3));
        assert_eq!((c.len(), c.bytes()), (1, 200));
        assert!(!c.is_empty());
    }
}
