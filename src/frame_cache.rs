//! Screen frames kept in RAM between game starts, so a game that starts again does not draw its frame again
//! (`bezel::Layer`, docs/FRAMES.md "Cache").
//!
//! A built-in frame is drawn in code at the screen's size (`bezel::generate`, 5 to 60 ms on the Mini V2), a pack
//! frame is a PNG to decode; both give the same picture every time for the same inputs. One file per frame in a
//! folder in RAM (`$XDG_RUNTIME_DIR/pocket-ui-frames`, else `/tmp/pocket-ui-frames-<uid>`; `POCKETUI_FRAME_CACHE`
//! moves it or turns it off), named after a hash of its [`Recipe::key`]: the style with its finish or colour, the
//! screen's size, the picture's rectangle and scale (which also fixes the Nintendo DS gap the Twin frame covers),
//! for a pack its file's path, size and time, and a hash of the frame code itself ([`CODE`]), so a new build never
//! reads a frame an older one drew.
//!
//! A file holds the key again (a hash collision is a miss, not a wrong frame), the frame's size, hole, inlay and
//! power light, the pixels with runs of transparent ones left out (the hole and the black panel edge, about two
//! thirds of the screen) and a checksum of everything. A file that is cut short, damaged or from another key is
//! deleted and the frame drawn again. Writes go to a temporary file that is renamed over the name
//! (`fsutil::replace_file`), so two processes or threads writing the same frame, or one reading while another
//! writes, only ever see whole files. The folder is kept under [`LIMIT`] bytes: the files used longest ago go
//! first (a hit touches its file).
//!
//! Nothing goes to the card: it is lost at power-off, and the first start after a boot draws its frame on a thread
//! while the game's first frames show without it (`Layer::set_async`), so a miss costs no start time.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::bezel::{Bezel, Style};
use crate::frame_led::{Led, Spot};
use crate::video::Rect;

/// The most the folder may hold, in bytes (a typical frame is 1 to 2 MB).
pub const LIMIT: u64 = 24 << 20;

const MAGIC: &[u8; 8] = b"PUIFRM01";
/// Larger frames are not read (a damaged size field must not allocate gigabytes).
const MAX_SIDE: u32 = 8192;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

const fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    let mut i = 0;
    while i < bytes.len() {
        h = (h ^ bytes[i] as u64).wrapping_mul(FNV_PRIME);
        i += 1;
    }
    h
}

/// A hash of the code that draws frames and of this file's format, made by the compiler: any change to one of
/// these files is a different hash, so frames drawn by another build are never used.
pub const CODE: u64 = {
    let mut h = FNV_OFFSET;
    h = fnv(h, include_bytes!("bezel.rs"));
    h = fnv(h, include_bytes!("frames.rs"));
    h = fnv(h, include_bytes!("frames_more.rs"));
    h = fnv(h, include_bytes!("frames_tv.rs"));
    h = fnv(h, include_bytes!("frame_led.rs"));
    h = fnv(h, include_bytes!("frame_cache.rs"));
    h
};

/// What a frame is made from.
#[derive(Clone, Debug, PartialEq)]
pub enum Recipe {
    /// A built-in style drawn for a picture at `game` on a `view` screen, `px` output pixels per game pixel.
    Builtin { style: Style, view: (u32, u32), game: Rect, px: f32 },
    /// A pack's PNG with its hole.
    Pack { path: PathBuf, hole: Rect },
}

impl Recipe {
    /// Everything the frame depends on, as text; None when a pack's file can't be looked at (it is then read
    /// every time, and fails as before).
    pub fn key(&self) -> Option<String> {
        match self {
            Recipe::Builtin { style, view, game, px } => Some(format!("{CODE:016x} builtin {style:?} {}x{} {game:?} px={:08x}", view.0, view.1, px.to_bits())),
            Recipe::Pack { path, hole } => {
                use std::os::unix::fs::MetadataExt;
                let m = fs::metadata(path).ok()?;
                Some(format!("{CODE:016x} pack {} {} {}.{} {hole:?}", path.display(), m.len(), m.mtime(), m.mtime_nsec()))
            }
        }
    }

    /// Draws or reads the frame (no cache).
    pub fn make(&self) -> Option<Bezel> {
        match self {
            Recipe::Builtin { style, view, game, px } => crate::bezel::generate(*style, view.0, view.1, *game, *px),
            Recipe::Pack { path, hole } => {
                let img = crate::bezel::load_png_rgba(path);
                if img.is_none() {
                    eprintln!("bezel: can't read {}", path.display());
                }
                img.map(|(w, h, rgba)| Bezel::new(w, h, rgba, *hole))
            }
        }
    }
}

/// The frame for `recipe`: from the cache in `dir` when it is there, else made. The key comes back when the frame
/// was made now, for [`store`] (after it has been put on screen: keeping it is no reason to wait).
pub fn fetch(recipe: &Recipe, dir: Option<&Path>) -> (Option<Bezel>, Option<String>) {
    let t0 = std::time::Instant::now();
    let key = dir.and_then(|_| recipe.key());
    if let (Some(dir), Some(key)) = (dir, key.as_deref()) {
        if let Some(b) = load(dir, key) {
            eprintln!("bezel: prepared in {:.1} ms (from the cache)", t0.elapsed().as_secs_f32() * 1000.0);
            return (Some(b), None);
        }
    }
    let b = recipe.make();
    eprintln!("bezel: prepared in {:.1} ms (drawn{})", t0.elapsed().as_secs_f32() * 1000.0, if key.is_some() { ", kept in the cache" } else { "" });
    let keep = key.filter(|_| b.is_some());
    (b, keep)
}

/// The cache's folder:`POCKETUI_FRAME_CACHE` (`off` or empty: no cache), else a folder in `$XDG_RUNTIME_DIR`
/// (RAM, this user's only), else in `/tmp`. Made on first use, readable by this user only.
pub fn dir() -> Option<PathBuf> {
    let dir = match std::env::var_os("POCKETUI_FRAME_CACHE") {
        Some(v) if v.is_empty() || v == "off" || v == "0" => return None,
        Some(v) => PathBuf::from(v),
        None => match std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
            Some(run) => Path::new(&run).join("pocket-ui-frames"),
            None => PathBuf::from(format!("/tmp/pocket-ui-frames-{}", unsafe { libc::geteuid() })),
        },
    };
    usable(&dir).then_some(dir)
}

/// Makes `dir` (mode 0700) if needed; true when it is a real folder of ours (not a link someone left in `/tmp`).
fn usable(dir: &Path) -> bool {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let _ = fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
    fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && m.uid() == unsafe { libc::geteuid() })
}

fn file_for(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{:016x}.frame", fnv(FNV_OFFSET, key.as_bytes())))
}

/// The frame kept for `key`, if there is a whole, undamaged one. A damaged file or one for another key (a hash
/// collision) is deleted.
pub fn load(dir: &Path, key: &str) -> Option<Bezel> {
    let path = file_for(dir, key);
    let mut file = fs::File::open(&path).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    match decode(&bytes, key) {
        Some(b) => {
            // Used now: the last to go when the folder is full.
            let _ = file.set_modified(std::time::SystemTime::now());
            Some(b)
        }
        None => {
            eprintln!("bezel: cached frame {} unreadable, drawing it again", path.display());
            let _ = fs::remove_file(&path);
            None
        }
    }
}

/// Keeps `bezel` for `key`, then trims the folder to [`LIMIT`]. Errors are only logged: the cache is a shortcut.
pub fn store(dir: &Path, key: &str, bezel: &Bezel) {
    let path = file_for(dir, key);
    if let Err(e) = crate::fsutil::replace_file(&path, &encode(key, bezel)) {
        eprintln!("bezel: can't keep the frame in {}: {e}", dir.display());
        return;
    }
    trim(dir, &path, LIMIT);
}

/// Deletes the least recently used frames until the folder holds at most `limit` bytes (never `keep`), and
/// temporary files a crashed writer left (older than a minute).
fn trim(dir: &Path, keep: &Path, limit: u64) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let now = std::time::SystemTime::now();
    let mut frames = Vec::new();
    for e in entries.flatten() {
        let (path, Ok(meta)) = (e.path(), e.metadata()) else { continue };
        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        match path.extension().and_then(|x| x.to_str()) {
            Some("frame") => frames.push((modified, meta.len(), path)),
            Some("tmp") if now.duration_since(modified).is_ok_and(|age| age.as_secs() > 60) => {
                let _ = fs::remove_file(&path);
            }
            _ => {}
        }
    }
    let mut total: u64 = frames.iter().map(|f| f.1).sum();
    frames.sort();
    for (_, len, path) in frames {
        if total <= limit {
            break;
        }
        if path != keep && fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

// --- The file -----------------------------------------------------------------------------------------------------

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_rect(out: &mut Vec<u8>, r: Rect) {
    for v in [r.x, r.y, r.w, r.h] {
        put_u32(out, v);
    }
}

fn encode(key: &str, b: &Bezel) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.rgba.len() / 2 + 4096);
    out.extend_from_slice(MAGIC);
    put_u32(&mut out, key.len() as u32);
    out.extend_from_slice(key.as_bytes());
    put_u32(&mut out, b.w);
    put_u32(&mut out, b.h);
    put_rect(&mut out, b.hole);
    out.push(b.inlay.is_some() as u8);
    put_rect(&mut out, b.inlay.unwrap_or(Rect { x: 0, y: 0, w: 0, h: 0 }));
    out.push(b.led.is_some() as u8);
    if let Some(led) = &b.led {
        for v in [led.spot.x, led.spot.y, led.spot.r] {
            put_u32(&mut out, v.to_bits());
        }
        put_rect(&mut out, led.rect);
        put_u32(&mut out, led.base.len() as u32);
        out.extend_from_slice(&led.base);
    }
    // The pixels as runs: (transparent pixels, then pixels as they are, those pixels), to the end.
    let px: Vec<u32> = b.rgba.chunks_exact(4).map(|p| u32::from_ne_bytes([p[0], p[1], p[2], p[3]])).collect();
    let mut i = 0;
    while i < px.len() {
        let z = px[i..].iter().take_while(|&&p| p == 0).count();
        let l = px[i + z..].iter().take_while(|&&p| p != 0).count();
        put_u32(&mut out, z as u32);
        put_u32(&mut out, l as u32);
        out.extend_from_slice(&b.rgba[(i + z) * 4..(i + z + l) * 4]);
        i += z + l;
    }
    let sum = fnv(FNV_OFFSET, &out);
    out.extend_from_slice(&sum.to_le_bytes());
    out
}

/// Reads what `encode` wrote, checking every length against what is left; None if anything is off.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len())?;
        let s = &self.bytes[self.at..end];
        self.at = end;
        Some(s)
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn rect(&mut self) -> Option<Rect> {
        Some(Rect { x: self.u32()?, y: self.u32()?, w: self.u32()?, h: self.u32()? })
    }
}

fn decode(bytes: &[u8], key: &str) -> Option<Bezel> {
    let body_len = bytes.len().checked_sub(8)?;
    let (body, sum) = bytes.split_at(body_len);
    if !body.starts_with(MAGIC) || fnv(FNV_OFFSET, body).to_le_bytes() != sum {
        return None;
    }
    let mut r = Reader { bytes: body, at: MAGIC.len() };
    let key_len = r.u32()? as usize;
    if r.take(key_len)? != key.as_bytes() {
        return None;
    }
    let (w, h) = (r.u32()?, r.u32()?);
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let hole = r.rect()?;
    let (has_inlay, inlay) = (r.u8()?, r.rect()?);
    let led = match r.u8()? {
        0 => None,
        _ => {
            let f = |v: u32| f32::from_bits(v);
            let spot = Spot { x: f(r.u32()?), y: f(r.u32()?), r: f(r.u32()?) };
            let rect = r.rect()?;
            let n = r.u32()? as usize;
            if n != rect.w as usize * rect.h as usize * 4 {
                return None;
            }
            Some(Led { spot, rect, base: r.take(n)?.to_vec() })
        }
    };
    let total = w as usize * h as usize;
    let mut rgba = vec![0u8; total * 4];
    let mut at = 0usize;
    while at < total {
        let (z, l) = (r.u32()? as usize, r.u32()? as usize);
        let start = at.checked_add(z)?;
        let end = start.checked_add(l).filter(|&e| e <= total)?;
        if z == 0 && l == 0 {
            return None;
        }
        rgba[start * 4..end * 4].copy_from_slice(r.take(l * 4)?);
        at = end;
    }
    if r.at != body.len() {
        return None;
    }
    let mut b = Bezel::new(w, h, rgba, hole);
    b.inlay = (has_inlay != 0).then_some(inlay);
    b.led = led;
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bezel::{BrickColour, Colourway, SlimFinish};
    use crate::testutil::TestDir;

    const GB_7X: Rect = Rect { x: 60, y: 36, w: 1120, h: 1008 };
    const GBA_5X: Rect = Rect { x: 20, y: 140, w: 1200, h: 800 };

    fn temp_dir(name: &str) -> crate::testutil::TestDir {
        let dir = TestDir::new(&format!("frame-cache-{name}"));
        assert!(usable(&dir));
        dir
    }

    fn brick() -> Recipe {
        Recipe::Builtin { style: Style::Brick(BrickColour::Grey), view: (1240, 1080), game: GB_7X, px: 7.0 }
    }

    fn same(a: &Bezel, b: &Bezel) -> bool {
        (a.w, a.h, a.hole, &a.led, a.inlay) == (b.w, b.h, b.hole, &b.led, b.inlay) && a.rgba == b.rgba
    }

    #[test]
    fn a_kept_frame_comes_back_exactly_and_smaller() {
        let dir = temp_dir("round");
        // With a power light (Brick), with an inlay (Twin over a DS gap), plain (Monitor).
        let px = 1080.0 / 448.0;
        let w = (256.0 * px) as u32;
        let twin = Recipe::Builtin { style: Style::Twin, view: (1240, 1080), game: Rect { x: (1240 - w) / 2, y: 0, w, h: 1080 }, px };
        let monitor = Recipe::Builtin { style: Style::Monitor, view: (1240, 1080), game: GBA_5X, px: 5.0 };
        for recipe in [brick(), twin, monitor] {
            let key = recipe.key().unwrap();
            let made = recipe.make().unwrap();
            assert!(load(&dir, &key).is_none(), "nothing kept yet");
            store(&dir, &key, &made);
            let back = load(&dir, &key).expect("kept");
            assert!(same(&made, &back), "{recipe:?}");
            let on_disk = fs::metadata(file_for(&dir, &key)).unwrap().len();
            assert!(on_disk < made.rgba.len() as u64 / 2, "{recipe:?}: {on_disk} bytes");
        }
        let b = load(&dir, &brick().key().unwrap()).unwrap();
        assert!(b.led.is_some());
    }

    #[test]
    fn changing_anything_the_frame_depends_on_is_another_key() {
        let base = brick().key().unwrap();
        let Recipe::Builtin { style, view, game, px } = brick() else { unreachable!() };
        let changed = [
            Recipe::Builtin { style: Style::Brick(BrickColour::Red), view, game, px },
            Recipe::Builtin { style: Style::PocketSlim(SlimFinish::Clear), view, game, px },
            Recipe::Builtin { style: Style::Shell(Colourway::Grey), view, game, px },
            Recipe::Builtin { style, view: (1080, 1240), game, px },
            Recipe::Builtin { style, view: (1240, 1081), game, px },
            Recipe::Builtin { style, view, game: Rect { x: 61, ..game }, px },
            Recipe::Builtin { style, view, game: Rect { h: 1007, ..game }, px },
            Recipe::Builtin { style, view, game, px: 7.0001 },
        ];
        let mut keys = vec![base.clone()];
        for r in &changed {
            let k = r.key().unwrap();
            assert!(!keys.contains(&k), "{r:?} has the key of an earlier recipe");
            keys.push(k);
        }
        assert_eq!(brick().key().unwrap(), base, "the same inputs, the same key");
        assert!(base.contains(&format!("{CODE:016x}")), "the code's hash is part of the key");

        // A pack: its path, and its file's size and time.
        let dir = temp_dir("pack");
        let png = dir.join("gbc.png");
        crate::bezel::write_pack(&dir, "Test", "gbc", &brick().make().unwrap()).unwrap();
        let pack = Recipe::Pack { path: png.clone(), hole: GB_7X };
        let k1 = pack.key().unwrap();
        assert_ne!(Recipe::Pack { path: png.clone(), hole: GBA_5X }.key().unwrap(), k1, "another hole");
        let t = fs::metadata(&png).unwrap().modified().unwrap() + std::time::Duration::from_secs(5);
        fs::File::options().write(true).open(&png).unwrap().set_modified(t).unwrap();
        assert_ne!(pack.key().unwrap(), k1, "the pack's file changed");
        assert!(Recipe::Pack { path: dir.join("missing.png"), hole: GB_7X }.key().is_none());
        // A frame kept for one key is not found for another.
        let made = brick().make().unwrap();
        store(&dir, &keys[0], &made);
        for k in &keys[1..] {
            assert!(load(&dir, k).is_none());
        }
    }

    #[test]
    fn a_damaged_or_foreign_file_is_a_miss_and_goes() {
        let dir = temp_dir("damaged");
        let key = brick().key().unwrap();
        let made = brick().make().unwrap();
        let path = file_for(&dir, &key);
        let good = encode(&key, &made);
        let flipped = {
            let mut b = good.clone();
            let i = b.len() / 2;
            b[i] ^= 0x40;
            b
        };
        let damaged: Vec<Vec<u8>> = vec![
            Vec::new(),
            good[..7].to_vec(),
            good[..good.len() / 3].to_vec(),
            good[..good.len() - 1].to_vec(),
            flipped,
            [good.clone(), vec![0]].concat(),
            // Another key under this name (a hash collision).
            encode("something else", &made),
        ];
        for bytes in damaged {
            fs::write(&path, &bytes).unwrap();
            assert!(load(&dir, &key).is_none(), "{} bytes", bytes.len());
            assert!(!path.exists(), "the bad file is deleted");
        }
        // A file that passes the checksum but whose runs lie about the size is refused too.
        let mut b = Bezel::new(4, 1, vec![1; 16], Rect { x: 0, y: 0, w: 0, h: 0 });
        b.rgba[0] = 0;
        let mut bytes = encode(&key, &b);
        bytes.truncate(bytes.len() - 8);
        let n = bytes.len();
        bytes[n - 16 - 4..n - 16].copy_from_slice(&9u32.to_le_bytes()); // the literal count of the only run
        let sum = fnv(FNV_OFFSET, &bytes);
        bytes.extend_from_slice(&sum.to_le_bytes());
        assert!(decode(&bytes, &key).is_none());
        // And a good one still loads.
        fs::write(&path, &good).unwrap();
        assert!(same(&load(&dir, &key).unwrap(), &made));
    }

    #[test]
    fn the_folder_stays_under_its_limit_dropping_the_oldest() {
        let dir = temp_dir("trim");
        let b = Bezel::new(64, 64, (0..64 * 64 * 4).map(|i| (i % 251) as u8 | 1).collect(), Rect { x: 0, y: 0, w: 0, h: 0 });
        let one = encode("k0", &b).len() as u64;
        let mut paths = Vec::new();
        for i in 0..5 {
            let key = format!("k{i}");
            fs::write(file_for(&dir, &key), encode(&key, &b)).unwrap();
            let p = file_for(&dir, &key);
            fs::File::options().write(true).open(&p).unwrap().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000 + i)).unwrap();
            paths.push(p);
        }
        // k0 was used just now: it stays, k1 and k2 (the oldest after it) go.
        assert!(load(&dir, "k0").is_some());
        let stale = dir.join("x.frame.1-1.tmp");
        fs::write(&stale, b"half").unwrap();
        fs::File::options().write(true).open(&stale).unwrap().set_modified(std::time::UNIX_EPOCH).unwrap();
        trim(&dir, &paths[4], one * 3);
        let left: Vec<bool> = paths.iter().map(|p| p.exists()).collect();
        assert_eq!(left, [true, false, false, true, true]);
        assert!(!stale.exists(), "a crashed writer's temporary file goes");
    }

    #[test]
    fn threads_writing_and_reading_one_frame_only_see_whole_files() {
        let dir = temp_dir("threads");
        let key = brick().key().unwrap();
        let made = std::sync::Arc::new(brick().make().unwrap());
        std::thread::scope(|s| {
            for t in 0..6 {
                let (dir, key, made) = (&dir, &key, made.clone());
                s.spawn(move || {
                    for _ in 0..4 {
                        if t % 2 == 0 {
                            store(dir, key, &made);
                        } else if let Some(b) = load(dir, key) {
                            assert!(same(&b, &made), "a reader saw a frame that isn't the one written");
                        }
                    }
                });
            }
        });
        assert!(same(&load(&dir, &key).unwrap(), &made));
        let names: Vec<String> = fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.len(), 1, "no temporary files left: {names:?}");
    }

    #[test]
    fn the_folder_is_not_used_through_a_link() {
        let dir = temp_dir("link");
        let link = dir.with_extension("link");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        assert!(!usable(&link));
        let _ = fs::remove_file(&link);
    }
}
