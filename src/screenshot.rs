//! Screenshots while playing (HOME + A): the game's picture as a PNG on the card.
//!
//! - **Where**: `/storage/roms/screenshots/<system>/<game> <YYYY-MM-DD HH-MM-SS>.png` (`POCKETUI_SCREENSHOTS` moves
//!   the folder, for tests and the virtual device). It sits under `roms`, so the file share (Samba) shows it without
//!   a rule of its own, next to the games; `savestates` and `bios` are the folders the share hides, this one is not.
//!   The name has no colons (the card is vfat/exFAT) and sorts by time. The game part goes through [`clean`].
//! - **What**: pocket-core's own last frame, at the size the core delivered it (a Game Boy picture is 160 x 144),
//!   without the HUD, a screen frame or the scaling. For the standalone emulators the screen as `grim` sees it.
//! - **Never on the game's thread**: pocket-core copies the frame buffer (a few hundred KB at most) and hands it to
//!   the [`Saver`]'s thread, which converts, encodes and writes it. The result comes back through the `done`
//!   callback. One screenshot is made at a time; a press while one is being written is ignored.
//! - **Atomic**: the PNG is written through a temporary file and renamed (`fsutil::write_atomic`), so the folder
//!   never holds half a picture. A card that is full or a folder that can't be written to is named in the toast
//!   ([`Failure::toast`]) and leaves nothing behind.
//! - **Listing** ([`list`], [`find`]): the web companion shows a game's screenshots from the same names. The
//!   companion hands the browser the time part of the name ([`Entry::at`]), never a path.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::time::Duration;

use crate::libretro::Frame;

/// The folder on the device.
pub const DEFAULT_DIR: &str = "/storage/roms/screenshots";

/// How long `grim` gets to answer (standalone emulators).
const GRIM_TIMEOUT: Duration = Duration::from_secs(5);

/// The longest game part of a file name, in bytes; with the time and ".png" it stays far under the 255 of the card.
const NAME_MAX: usize = 120;

/// The screenshots folder (`POCKETUI_SCREENSHOTS` overrides it).
pub fn dir() -> PathBuf {
    std::env::var_os("POCKETUI_SCREENSHOTS").filter(|d| !d.is_empty()).map_or_else(|| PathBuf::from(DEFAULT_DIR), PathBuf::from)
}

/// A name as one safe path part: no slash or backslash (a game called "AC/DC" must not make a folder), none of the
/// characters vfat/exFAT refuse (`: * ? " < > |`), no control characters, no leading dot (it would hide the file), no
/// trailing dot or space, at most [`NAME_MAX`] bytes. Everything refused becomes an underscore; nothing left becomes
/// "game". Unicode stays as it is.
pub fn clean(name: &str) -> String {
    let mut out: String = name.chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c }).collect();
    if out.starts_with('.') {
        out.replace_range(..1, "_");
    }
    while out.len() > NAME_MAX {
        out.pop();
    }
    let kept = out.trim_end_matches(['.', ' ']).len();
    out.truncate(kept);
    let kept = out.trim_start().len();
    out.drain(..out.len() - kept);
    if out.is_empty() {
        "game".into()
    } else {
        out
    }
}

/// A local date and time, to the second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
}

impl Stamp {
    /// Now, in the device's time zone.
    pub fn now() -> Stamp {
        // SAFETY: time(NULL) only reads the clock.
        Stamp::at(unsafe { libc::time(std::ptr::null_mut()) } as i64)
    }

    /// A Unix time in the device's time zone (the report's file name, `report::file_name`).
    pub fn at(unix: i64) -> Stamp {
        let t = unix as libc::time_t;
        // SAFETY: localtime_r only writes into the struct handed in.
        let tm = unsafe {
            let mut tm: libc::tm = std::mem::zeroed();
            libc::localtime_r(&t, &mut tm);
            tm
        };
        if t <= 0 {
            return Stamp { year: 1970, month: 1, day: 1, hour: 0, min: 0, sec: 0 };
        }
        Stamp { year: tm.tm_year + 1900, month: (tm.tm_mon + 1) as u32, day: tm.tm_mday as u32, hour: tm.tm_hour as u32, min: tm.tm_min as u32, sec: tm.tm_sec as u32 }
    }

    /// "2026-10-24 18-05-09": dashes in the time, because the card refuses a colon in a name.
    pub fn text(&self) -> String {
        format!("{:04}-{:02}-{:02} {:02}-{:02}-{:02}", self.year, self.month, self.day, self.hour, self.min, self.sec)
    }
}

/// The file name of a screenshot: `<game> <stamp>.png`; `n` > 1 adds " (n)" for a second one in the same second.
pub fn file_name(game: &str, stamp: &Stamp, n: u32) -> String {
    let more = if n > 1 { format!(" ({n})") } else { String::new() };
    format!("{} {}{more}.png", clean(game), stamp.text())
}

/// Where a screenshot goes: `<dir>/<system>/<game> <stamp>.png`; if that name is taken (two presses in one second),
/// the first free ` (2)`, ` (3)` ....
pub fn target(dir: &Path, system: &str, game: &str, stamp: &Stamp) -> PathBuf {
    let folder = dir.join(clean(system));
    let mut path = folder.join(file_name(game, stamp, 1));
    let mut n = 1;
    while path.symlink_metadata().is_ok() && n < 999 {
        n += 1;
        path = folder.join(file_name(game, stamp, n));
    }
    path
}

/// 8-bit RGB as a PNG; None when the buffer doesn't match the size.
pub fn encode_rgb(width: u32, height: u32, rgb: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgb.len() != width as usize * height as usize * 3 {
        return None;
    }
    let mut out = Vec::with_capacity(rgb.len() / 2 + 1024);
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut w| w.write_image_data(rgb)).ok()?;
    Some(out)
}

/// Why a screenshot wasn't saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The card has no room left.
    Full,
    /// The folder can't be written to (read-only card, a file where the folder should be, no permission).
    Unwritable,
    /// The core has not shown a picture yet.
    NoPicture,
    /// The screen could not be read (standalone emulators).
    NoScreen,
    Other,
}

impl Failure {
    /// The words on screen; each is a line of `assets/i18n/de.tsv`.
    pub fn toast(self) -> &'static str {
        match self {
            Failure::Full => "Screenshot not saved: the SD card is full",
            Failure::Unwritable => "Screenshot not saved: the folder can't be written to",
            Failure::NoPicture => "Screenshot not saved: no picture yet",
            Failure::NoScreen => "Screenshot not saved: the screen couldn't be read",
            Failure::Other => "Screenshot not saved",
        }
    }
}

/// What an error of the card means to the player.
pub fn classify(e: &io::Error) -> Failure {
    match e.raw_os_error() {
        Some(c) if c == libc::ENOSPC || c == libc::EDQUOT || c == libc::EFBIG => return Failure::Full,
        Some(c) if [libc::EACCES, libc::EPERM, libc::EROFS, libc::ENOTDIR, libc::EEXIST, libc::EISDIR].contains(&c) => return Failure::Unwritable,
        _ => {}
    }
    match e.kind() {
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => Failure::Full,
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem | io::ErrorKind::NotADirectory | io::ErrorKind::IsADirectory | io::ErrorKind::AlreadyExists => Failure::Unwritable,
        _ => Failure::Other,
    }
}

/// Puts `png` in place for the game: a temporary file in the folder, then a rename. Returns the file's path.
pub fn save_png(dir: &Path, system: &str, game: &str, stamp: &Stamp, png: &[u8]) -> Result<PathBuf, Failure> {
    let path = target(dir, system, game, stamp);
    match crate::fsutil::write_atomic(&path, png) {
        Ok(()) => Ok(path),
        Err(e) => {
            eprintln!("screenshot: can't write {}: {e}", path.display());
            Err(classify(&e))
        }
    }
}

/// What a request is made from.
pub enum Source {
    /// pocket-core: a copy of the core's last frame.
    Frame(Frame),
    /// A standalone emulator: the screen, read with `grim`.
    Screen,
}

/// One screenshot to make.
pub struct Request {
    pub system: String,
    /// The game's file name without extension (the web companion looks for the same).
    pub game: String,
    pub stamp: Stamp,
    pub source: Source,
}

/// What became of a request.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Saved(PathBuf),
    Failed(Failure),
}

/// The PNG of the request, then the file. `screen` reads the screen (a PNG) for [`Source::Screen`].
pub fn make(dir: &Path, req: Request, screen: &dyn Fn() -> Option<Vec<u8>>) -> Outcome {
    let png = match &req.source {
        Source::Frame(f) => {
            let (w, h) = (f.width as usize, f.height as usize);
            let bpp = if f.format == crate::libretro::PIXEL_XRGB8888 { 4 } else { 2 };
            // A frame whose buffer is shorter than its size says is not read past its end.
            if w == 0 || h == 0 || f.pitch < w * bpp || f.data.len() < f.pitch * (h - 1) + w * bpp {
                return Outcome::Failed(Failure::NoPicture);
            }
            match encode_rgb(f.width, f.height, &f.to_rgb()) {
                Some(p) => p,
                None => return Outcome::Failed(Failure::NoPicture),
            }
        }
        Source::Screen => match screen().filter(|p| p.starts_with(b"\x89PNG")) {
            Some(p) => p,
            None => return Outcome::Failed(Failure::NoScreen),
        },
    };
    match save_png(dir, &req.system, &req.game, &req.stamp, &png) {
        Ok(path) => Outcome::Saved(path),
        Err(f) => Outcome::Failed(f),
    }
}

/// The screen as `grim` writes it: a PNG on stdout.
pub fn grim_png() -> Option<Vec<u8>> {
    let out = crate::proc::run_with_timeout(std::process::Command::new("grim").args(["-t", "png", "-"]), GRIM_TIMEOUT).ok()?;
    out.status.success().then_some(out.stdout)
}

/// The thread that makes screenshots, one at a time.
pub struct Saver {
    tx: SyncSender<Request>,
}

impl Saver {
    /// Starts the thread; `done` is called on it with every outcome.
    pub fn start(dir: PathBuf, done: impl Fn(Outcome) + Send + 'static) -> io::Result<Saver> {
        let (tx, rx) = sync_channel::<Request>(1);
        std::thread::Builder::new().name("screenshot".into()).spawn(move || {
            for req in rx {
                let outcome = make(&dir, req, &grim_png);
                if let Outcome::Saved(p) = &outcome {
                    println!("screenshot: saved {}", p.display());
                }
                done(outcome);
            }
        })?;
        Ok(Saver { tx })
    }

    /// Hands the request over and returns at once. False when the last screenshot is still being written (the press
    /// is ignored) or the thread is gone.
    pub fn submit(&self, req: Request) -> bool {
        match self.tx.try_send(req) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

/// One screenshot of a game, for the web companion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The time part of the name, with " (n)" when there were several in one second: "2026-10-24 18-05-09". This is
    /// what travels to the browser (never the path), and what [`find`] takes back.
    pub at: String,
}

/// The time part of a file name `<prefix><at>.png`, when it is one.
fn at_of(file: &str, prefix: &str) -> Option<String> {
    let rest = file.strip_prefix(prefix)?.strip_suffix(".png")?;
    valid_at(rest).then(|| rest.to_string())
}

/// "YYYY-MM-DD HH-MM-SS", then optionally " (n)".
pub fn valid_at(at: &str) -> bool {
    let b = at.as_bytes();
    if b.len() < 19 || !at.is_ascii() {
        return false;
    }
    let shape = b"dddd-dd-dd dd-dd-dd";
    let stamp_ok = b[..19].iter().zip(shape).all(|(c, s)| if *s == b'd' { c.is_ascii_digit() } else { c == s });
    let more = &at[19..];
    let more_ok = more.is_empty()
        || more.strip_prefix(" (").and_then(|m| m.strip_suffix(')')).is_some_and(|n| !n.is_empty() && n.len() <= 3 && n.bytes().all(|c| c.is_ascii_digit()));
    stamp_ok && more_ok
}

/// A game's screenshots, newest first. Only files named like [`file_name`] count, so "Game" never lists the pictures
/// of "Game 2".
pub fn list(dir: &Path, system: &str, game: &str) -> Vec<Entry> {
    let prefix = format!("{} ", clean(game));
    let Ok(entries) = std::fs::read_dir(dir.join(clean(system))) else { return Vec::new() };
    let mut found: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| at_of(&e.file_name().to_string_lossy(), &prefix))
        .collect();
    // Time first, then the number of a repeat in the same second; "(10)" after "(9)".
    let key = |s: &String| (s[..19].to_string(), s[19..].trim_matches([' ', '(', ')']).parse::<u32>().unwrap_or(1));
    found.sort_by_key(|s| std::cmp::Reverse(key(s)));
    found.into_iter().map(|at| Entry { at }).collect()
}

/// The file of the game's screenshot taken `at`, if it is there. `at` comes from the browser: it must be exactly a
/// time part, so it can't point anywhere else.
pub fn find(dir: &Path, system: &str, game: &str, at: &str) -> Option<PathBuf> {
    if !valid_at(at) {
        return None;
    }
    let path = dir.join(clean(system)).join(format!("{} {at}.png", clean(game)));
    path.symlink_metadata().is_ok_and(|m| m.is_file()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn stamp() -> Stamp {
        Stamp { year: 2026, month: 10, day: 24, hour: 18, min: 5, sec: 9 }
    }

    fn scratch(tag: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("shot-{tag}"))
    }

    fn frame(w: u32, h: u32, pixel: [u8; 4]) -> Frame {
        // XRGB8888 (libretro format 1), little endian: B, G, R, X.
        Frame { width: w, height: h, pitch: w as usize * 4, format: 1, data: pixel.repeat((w * h) as usize) }
    }

    fn request(game: &str, f: Frame) -> Request {
        Request { system: "gba".into(), game: game.into(), stamp: stamp(), source: Source::Frame(f) }
    }

    #[test]
    fn the_name_is_game_then_time_with_no_colon() {
        assert_eq!(stamp().text(), "2026-10-24 18-05-09");
        assert_eq!(file_name("Tetris (World)", &stamp(), 1), "Tetris (World) 2026-10-24 18-05-09.png");
        assert_eq!(file_name("Tetris", &stamp(), 2), "Tetris 2026-10-24 18-05-09 (2).png");
        let t = target(Path::new("/storage/roms/screenshots"), "gba", "Tetris", &stamp());
        assert_eq!(t, Path::new("/storage/roms/screenshots/gba/Tetris 2026-10-24 18-05-09.png"));
    }

    #[test]
    fn unicode_stays_and_slashes_and_card_forbidden_characters_go() {
        assert_eq!(clean("Pokémon Smaragd – 日本語 🎮"), "Pokémon Smaragd – 日本語 🎮");
        assert_eq!(clean("AC/DC Pinball"), "AC_DC Pinball");
        assert_eq!(clean("a\\b"), "a_b");
        assert_eq!(clean("Zelda: Link's \"Awakening\"?"), "Zelda_ Link's _Awakening__");
        assert_eq!(clean("tab\there\nnl"), "tab_here_nl");
        for empty in ["", "   ", "\t"] {
            assert_eq!(clean(empty), if empty == "\t" { "_" } else { "game" }, "{empty:?}");
        }
        // Dots alone can't be "." or "..": the first one becomes an underscore.
        for dots in ["..", ".", "...", " . "] {
            let c = clean(dots);
            assert!(c != "." && c != ".." && !c.is_empty() && !c.starts_with('.'), "{dots:?} -> {c:?}");
        }
        assert_eq!(clean(".hidden"), "_hidden");
        assert_eq!(clean("Name. "), "Name");
        assert_eq!(clean("../../etc/passwd"), "_._.._etc_passwd");
        // Whatever comes out is one plain path part.
        for name in ["a/b", "../x", "/abs", "x/", "..\\y", "a\0b"] {
            let c = clean(name);
            assert!(!c.contains(['/', '\\', '\0']) && c != ".." && c != ".", "{name:?} -> {c:?}");
            assert_eq!(Path::new(&c).components().count(), 1, "{c:?}");
        }
    }

    #[test]
    fn a_long_name_is_cut_on_a_character_boundary() {
        let long = "ä".repeat(200);
        let c = clean(&long);
        assert!(c.len() <= NAME_MAX && c.chars().all(|ch| ch == 'ä') && !c.is_empty());
        let whole = file_name(&long, &stamp(), 1);
        assert!(whole.len() < 255, "{} bytes", whole.len());
    }

    #[test]
    fn a_name_in_use_gets_a_number_not_overwritten() {
        let dir = scratch("unique");
        let first = save_png(&dir, "gba", "Tetris", &stamp(), b"one").unwrap();
        let second = save_png(&dir, "gba", "Tetris", &stamp(), b"two").unwrap();
        assert_ne!(first, second);
        assert!(second.to_string_lossy().ends_with("Tetris 2026-10-24 18-05-09 (2).png"));
        assert_eq!(std::fs::read(&first).unwrap(), b"one");
        assert_eq!(std::fs::read(&second).unwrap(), b"two");
    }

    #[test]
    fn a_failed_write_leaves_no_file_and_no_temporary_file() {
        let dir = scratch("atomic");
        crate::fsutil::fail_next_write();
        let r = save_png(&dir, "gba", "Tetris", &stamp(), b"png bytes");
        assert!(r.is_err());
        let left: Vec<_> = std::fs::read_dir(dir.join("gba")).map(|d| d.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
        assert!(left.is_empty(), "nothing half-written: {left:?}");
        // The next one works, and its file is whole.
        let ok = save_png(&dir, "gba", "Tetris", &stamp(), b"png bytes").unwrap();
        assert_eq!(std::fs::read(ok).unwrap(), b"png bytes");
        let names: Vec<_> = std::fs::read_dir(dir.join("gba")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["Tetris 2026-10-24 18-05-09.png"], "no .tmp beside it");
    }

    #[test]
    fn a_folder_that_cant_be_made_says_so() {
        let dir = scratch("blocked");
        // A file where the folder should be: the same answer as a read-only card gives.
        std::fs::write(dir.join("gba"), b"in the way").unwrap();
        assert_eq!(save_png(&dir, "gba", "Tetris", &stamp(), b"x"), Err(Failure::Unwritable));
        assert_eq!(std::fs::read(dir.join("gba")).unwrap(), b"in the way");
    }

    #[test]
    fn card_errors_become_words_a_player_understands() {
        let e = |c: i32| io::Error::from_raw_os_error(c);
        assert_eq!(classify(&e(libc::ENOSPC)), Failure::Full);
        assert_eq!(classify(&e(libc::EDQUOT)), Failure::Full);
        for c in [libc::EROFS, libc::EACCES, libc::EPERM, libc::ENOTDIR] {
            assert_eq!(classify(&e(c)), Failure::Unwritable, "{c}");
        }
        assert_eq!(classify(&io::Error::other("who knows")), Failure::Other);
        for f in [Failure::Full, Failure::Unwritable, Failure::NoPicture, Failure::NoScreen, Failure::Other] {
            assert!(f.toast().starts_with("Screenshot not saved"), "{f:?}");
        }
        let all: std::collections::HashSet<_> = [Failure::Full, Failure::Unwritable, Failure::NoPicture, Failure::NoScreen, Failure::Other].iter().map(|f| f.toast()).collect();
        assert_eq!(all.len(), 5, "each reason has its own words");
    }

    #[test]
    fn a_frame_becomes_a_png_at_the_cores_own_size() {
        let dir = scratch("frame");
        // BGRX bytes: blue 0x10, green 0x80, red 0xf0.
        let out = make(&dir, request("Tetris", frame(160, 144, [0x10, 0x80, 0xf0, 0])), &|| None);
        let Outcome::Saved(path) = out else { panic!("{out:?}") };
        let img = crate::image::load_png(&path).expect("a PNG we can read again");
        assert_eq!((img.w, img.h), (160, 144), "native resolution, not scaled");
        assert_eq!(img.px[0] & 0xff_ffff, 0xf0_80_10);
    }

    #[test]
    fn a_rgb565_frame_is_converted_like_the_menu_previews() {
        let dir = scratch("565");
        // Pure red in RGB565 is 0xf800.
        let f = Frame { width: 4, height: 2, pitch: 8, format: 2, data: 0xf800u16.to_le_bytes().repeat(8) };
        let Outcome::Saved(path) = make(&dir, request("R", f), &|| None) else { panic!() };
        let img = crate::image::load_png(&path).unwrap();
        assert_eq!(img.px[0] & 0xff_ffff, 0xff_00_00);
    }

    #[test]
    fn a_missing_or_damaged_frame_is_no_picture_and_writes_nothing() {
        let dir = scratch("nopic");
        let mut short = frame(16, 16, [0; 4]);
        short.data.truncate(100);
        assert_eq!(make(&dir, request("G", short), &|| None), Outcome::Failed(Failure::NoPicture));
        assert_eq!(make(&dir, request("G", frame(0, 0, [0; 4])), &|| None), Outcome::Failed(Failure::NoPicture));
        assert!(!dir.join("gba").exists(), "no folder made for nothing");
    }

    #[test]
    fn the_screen_of_a_standalone_emulator_is_saved_only_if_it_is_a_png() {
        let dir = scratch("screen");
        let shot = || Request { system: "ps2".into(), game: "Game".into(), stamp: stamp(), source: Source::Screen };
        let png = encode_rgb(2, 2, &[7; 12]).unwrap();
        let Outcome::Saved(path) = make(&dir, shot(), &|| Some(png.clone())) else { panic!() };
        assert_eq!(std::fs::read(path).unwrap(), png);
        assert_eq!(make(&dir, shot(), &|| None), Outcome::Failed(Failure::NoScreen), "grim failed");
        assert_eq!(make(&dir, shot(), &|| Some(b"not a png".to_vec())), Outcome::Failed(Failure::NoScreen));
    }

    #[test]
    fn encoding_and_writing_happen_on_the_savers_own_thread() {
        let dir = scratch("thread");
        let (tx, rx) = std::sync::mpsc::channel();
        let saver = Saver::start(dir.to_path_buf(), move |o| {
            let _ = tx.send((std::thread::current().id(), std::thread::current().name().map(String::from), o));
        })
        .unwrap();
        // A big frame (PlayStation size) so the encode takes a moment; submit must not wait for it.
        let t0 = std::time::Instant::now();
        assert!(saver.submit(request("Big", frame(640, 480, [1, 2, 3, 0]))));
        let handed_over = t0.elapsed();
        let (id, name, outcome) = rx.recv_timeout(Duration::from_secs(20)).expect("an outcome");
        assert_ne!(id, std::thread::current().id());
        assert_eq!(name.as_deref(), Some("screenshot"));
        assert!(matches!(outcome, Outcome::Saved(_)));
        assert!(handed_over < t0.elapsed() || handed_over < Duration::from_millis(50), "handing over is not the encode: {handed_over:?}");
    }

    #[test]
    fn a_press_while_one_is_being_written_is_ignored_not_queued_up() {
        let dir = scratch("busy");
        let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
        let gate_rx = std::sync::Mutex::new(gate_rx);
        let (tx, rx) = std::sync::mpsc::channel();
        // The first outcome waits for the test, so the thread stays busy.
        let saver = Saver::start(dir.to_path_buf(), move |o| {
            let _ = gate_rx.lock().unwrap().recv();
            let _ = tx.send(o);
        })
        .unwrap();
        assert!(saver.submit(request("A", frame(8, 8, [0; 4]))));
        // The thread takes the first request at once; the second waits in the one free place; a third finds it full.
        let mut accepted = 1;
        for _ in 0..5 {
            if saver.submit(request("A", frame(8, 8, [0; 4]))) {
                accepted += 1;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(accepted <= 2, "{accepted} accepted while busy");
        for _ in 0..accepted {
            gate_tx.send(()).unwrap();
        }
        for _ in 0..accepted {
            assert!(matches!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), Outcome::Saved(_)));
        }
    }

    #[test]
    fn a_games_screenshots_are_listed_newest_first_and_only_its_own() {
        let dir = scratch("list");
        let put = |name: &str| {
            std::fs::create_dir_all(dir.join("gba")).unwrap();
            std::fs::write(dir.join("gba").join(name), b"x").unwrap();
        };
        put("Game 2026-10-24 18-05-09.png");
        put("Game 2026-10-24 18-05-09 (2).png");
        put("Game 2026-10-24 18-05-09 (10).png");
        put("Game 2026-10-25 09-00-00.png");
        put("Game 2 2026-10-26 09-00-00.png"); // another game
        put("Game 2026-10-24 18-05-09.png.tmp"); // not finished
        put("Game notes.png");
        put("Game 2026-10-24 18-05-09.jpg");
        let got: Vec<String> = list(&dir, "gba", "Game").into_iter().map(|e| e.at).collect();
        assert_eq!(got, ["2026-10-25 09-00-00", "2026-10-24 18-05-09 (10)", "2026-10-24 18-05-09 (2)", "2026-10-24 18-05-09"]);
        assert_eq!(list(&dir, "gba", "Game 2").len(), 1);
        assert!(list(&dir, "snes", "Game").is_empty(), "other system");
        assert!(list(&dir, "gba", "Nothing").is_empty());
        assert!(list(&dir.join("nope"), "gba", "Game").is_empty(), "no folder yet is an empty list, not an error");
    }

    #[test]
    fn find_takes_only_a_time_part_and_never_leaves_the_folder() {
        let dir = scratch("find");
        std::fs::create_dir_all(dir.join("gba")).unwrap();
        std::fs::write(dir.join("gba/Game 2026-10-24 18-05-09.png"), b"x").unwrap();
        std::fs::write(dir.join("secret.png"), b"x").unwrap();
        assert!(find(&dir, "gba", "Game", "2026-10-24 18-05-09").is_some());
        assert!(find(&dir, "gba", "Game", "2026-10-24 18-05-10").is_none());
        for evil in ["../../secret", "2026-10-24 18-05-09/../../secret", "", "2026-10-24 18-05-09.png", "2026-10-24 18-05-09 (x)", "../secret"] {
            assert!(find(&dir, "gba", "Game", evil).is_none(), "{evil:?}");
        }
        assert!(find(&dir, "..", "Game", "2026-10-24 18-05-09").is_none());
    }

    #[test]
    fn the_folder_can_be_moved_for_tests() {
        // Not set in the test environment: the device's folder.
        if std::env::var_os("POCKETUI_SCREENSHOTS").is_none() {
            assert_eq!(dir(), Path::new(DEFAULT_DIR));
        }
    }
}
