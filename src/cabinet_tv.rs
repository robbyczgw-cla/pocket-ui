//! The TV on the cabinet (0.1.4, Cabinet look only): a small 1990s CRT television standing on the cabinet's top board
//! beside the case out of the shelf, showing the selected game: its downloaded screenshot (`game_pictures`, Snap),
//! else the newest picture of the player's own play (the resume state's preview or the frame kept at Quit,
//! `launch_picture::source`), else its cover. The picture gets a light CRT look (a slightly curved glass, scanlines, a
//! darker rim and a warm glow) and crossfades in [`FADE`] when the selection changes (Motion Off: at once). No video.
//!
//! Cost: the TV's body is scaled once per size on the UI thread (a few thousand pixels). Finding the picture, decoding
//! it and the CRT look run on the `cabinet-tv` thread ([`Loader`]); the UI thread only asks and copies a finished
//! screen (`Pic`) in. The last [`KEEP`] screens are kept, so scrolling back is instant. The picture is PixelLab pixel
//! art (`assets/media/tv`, generated from text: Wikimedia Commons has no clean public-domain photo of a 90s portable
//! CRT; prompt and seed in `SOURCES.md` there), its glass painted one flat magenta that marks where the screen goes.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use crate::cabinet_art::Pic;
use crate::image::Image;
use crate::system_art::Sprite;

/// The crossfade from one game's picture to the next.
pub const FADE: Duration = Duration::from_millis(220);
/// Screens kept, by game and size.
pub const KEEP: usize = 12;
/// The glass's marker colour in `tv.png`.
const GLASS: u32 = 0xff00ff;
/// The glass where there is no picture (yet): a dark, slightly green-grey tube.
pub const TUBE: u32 = 0x1c211e;

const PNG: &[u8] = include_bytes!("../assets/media/tv/tv.png");

/// The TV's picture as drawn: `body` with the glass marked, `glass` its bounding box (x, y, w, h).
pub struct Template {
    pub body: Sprite,
    pub glass: (i32, i32, i32, i32),
}

pub fn template() -> &'static Template {
    static T: std::sync::OnceLock<Template> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let body = crate::system_art::decode(PNG).unwrap_or(Sprite { w: 1, h: 1, px: vec![0] });
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, -1, -1);
        for y in 0..body.h as i32 {
            for x in 0..body.w as i32 {
                if is_glass(body.px[(y * body.w as i32 + x) as usize]) {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        let glass = if x1 < 0 { (0, 0, 0, 0) } else { (x0, y0, x1 - x0 + 1, y1 - y0 + 1) };
        Template { body, glass }
    })
}

fn is_glass(p: u32) -> bool {
    p >> 24 != 0 && p & 0xffffff == GLASS
}

/// The TV `f` screen pixels a template pixel (nearest): its body with the glass as the dark tube, and a mask of the
/// glass (true where the screen shows) over the glass's box, scaled the same way.
pub struct Body {
    pub pic: Pic,
    /// The glass's box on `pic`.
    pub glass: (i32, i32, i32, i32),
    /// `glass.2 * glass.3` entries: whether that pixel of the box is glass.
    pub mask: Vec<bool>,
}

pub fn body(f: f32) -> Body {
    let t = template();
    let s = &t.body;
    let (w, h) = (((s.w as f32 * f).round() as i32).max(1), ((s.h as f32 * f).round() as i32).max(1));
    let mut pic = Pic::new(w, h, 0);
    let src = |x: i32, y: i32| {
        let sx = ((x as f32 / f) as u32).min(s.w - 1);
        let sy = ((y as f32 / f) as u32).min(s.h - 1);
        s.px[(sy * s.w + sx) as usize]
    };
    for y in 0..h {
        for x in 0..w {
            let p = src(x, y);
            pic.px[(y * w + x) as usize] = if is_glass(p) { 0xff00_0000 | TUBE } else { p };
        }
    }
    let (gx, gy, gw, gh) = t.glass;
    let glass = ((gx as f32 * f).ceil() as i32, (gy as f32 * f).ceil() as i32, ((gx + gw) as f32 * f).ceil() as i32, ((gy + gh) as f32 * f).ceil() as i32);
    let glass = (glass.0, glass.1, (glass.2 - glass.0).max(1), (glass.3 - glass.1).max(1));
    let mut mask = vec![false; (glass.2 * glass.3) as usize];
    for y in 0..glass.3 {
        for x in 0..glass.2 {
            mask[(y * glass.2 + x) as usize] = is_glass(src(glass.0 + x, glass.1 + y));
        }
    }
    Body { pic, glass, mask }
}

/// The CRT look over `img`, `w` x `h`: the picture cut to fill the glass (its middle kept), on a slightly curved tube
/// (the corners pulled in), scanlines every second row, darker towards the rim, a warm tint and a soft glow.
pub fn crt(img: &Image, w: i32, h: i32) -> Pic {
    let (w, h) = (w.max(1), h.max(1));
    let mut out = Pic::new(w, h, 0xff00_0000 | TUBE);
    if img.w == 0 || img.h == 0 {
        return out;
    }
    // The part of the picture the glass shows: as much as fills it, from the middle.
    let (iw, ih) = (img.w as f32, img.h as f32);
    let k = (w as f32 / iw).max(h as f32 / ih);
    let (cw, ch) = (w as f32 / k, h as f32 / k);
    let (ox, oy) = ((iw - cw) / 2.0, (ih - ch) / 2.0);
    // Bilinear sample of the picture at (u, v) in 0..1 of the cut.
    let sample = |u: f32, v: f32| -> (f32, f32, f32) {
        let x = (ox + u * cw - 0.5).clamp(0.0, iw - 1.0);
        let y = (oy + v * ch - 0.5).clamp(0.0, ih - 1.0);
        let (x0, y0) = (x as u32, y as u32);
        let (x1, y1) = ((x0 + 1).min(img.w - 1), (y0 + 1).min(img.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let px = |x: u32, y: u32| img.px[(y * img.w + x) as usize];
        let ch = |p: u32, s: u32| ((p >> s) & 0xff) as f32;
        let mut c = [0.0f32; 3];
        for (i, s) in [16u32, 8, 0].into_iter().enumerate() {
            let top = ch(px(x0, y0), s) * (1.0 - fx) + ch(px(x1, y0), s) * fx;
            let bot = ch(px(x0, y1), s) * (1.0 - fx) + ch(px(x1, y1), s) * fx;
            c[i] = top * (1.0 - fy) + bot * fy;
        }
        (c[0], c[1], c[2])
    };
    // Scanlines at the picture's own scale on a big screen, every second screen row on a small one.
    let line = ((h as f32 / 120.0).round() as i32).max(1);
    let mut lit = vec![(0.0f32, 0.0f32, 0.0f32); (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            // -1..1 across the glass, then the barrel: further out is pulled in a little more.
            let (nx, ny) = (2.0 * (x as f32 + 0.5) / w as f32 - 1.0, 2.0 * (y as f32 + 0.5) / h as f32 - 1.0);
            let r2 = nx * nx + ny * ny;
            let bend = 1.0 + 0.045 * r2;
            let (bx, by) = (nx * bend, ny * bend);
            if bx.abs() > 1.0 || by.abs() > 1.0 {
                continue;
            }
            let (r, g, b) = sample((bx + 1.0) / 2.0, (by + 1.0) / 2.0);
            let scan = if (y / line) % 2 == 1 { 0.74 } else { 1.0 };
            let vignette = (1.0 - 0.30 * r2.min(2.0) / 2.0).max(0.55);
            let k = scan * vignette;
            // A warm tube: a little more red, a little less blue.
            lit[(y * w + x) as usize] = (r * k * 1.04, g * k, b * k * 0.90);
        }
    }
    // The glow: each pixel takes a fifth of its neighbours' light (a 3 x 3 box), so bright parts bleed softly.
    for y in 0..h {
        for x in 0..w {
            let (mut sr, mut sg, mut sb, mut n) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (xx, yy) = (x + dx, y + dy);
                if xx >= 0 && yy >= 0 && xx < w && yy < h {
                    let (r, g, b) = lit[(yy * w + xx) as usize];
                    (sr, sg, sb, n) = (sr + r, sg + g, sb + b, n + 1.0);
                }
            }
            let (r, g, b) = lit[(y * w + x) as usize];
            let glow = |c: f32, s: f32| (c * 0.85 + s / n.max(1.0) * 0.25).min(255.0) as u32;
            let tube = |c: u32, t: u32| c.max(t / 2);
            let rgb = tube(glow(r, sr), TUBE >> 16 & 0xff) << 16 | tube(glow(g, sg), TUBE >> 8 & 0xff) << 8 | tube(glow(b, sb), TUBE & 0xff);
            out.px[(y * w + x) as usize] = 0xff00_0000 | rgb;
        }
    }
    out
}

/// Which picture the TV shows for a game, best first: the downloaded screenshot, the player's newest own picture
/// (the resume state's preview, the frame kept at Quit), the cover. Reads the disk: the loader's thread calls it.
pub fn source(data_dir: &Path, system: &str, rom: &Path, cover: Option<&Path>) -> Option<PathBuf> {
    let snap = crate::game_pictures::path(data_dir, crate::game_pictures::Kind::Snap, system, rom);
    if snap.is_file() {
        return Some(snap);
    }
    match crate::launch_picture::source(data_dir, system, rom) {
        Some(crate::launch_picture::Source::Resume(p) | crate::launch_picture::Source::Kept(p)) => return Some(p),
        None => {}
    }
    cover.filter(|c| c.is_file()).map(Path::to_path_buf)
}

/// The screen of a game with no picture at all (or of a system's box): its console's pixel art on the dark tube,
/// through the same CRT look, so the TV never stands there blank.
pub fn console_screen(system: &str, w: i32, h: i32) -> Option<Pic> {
    let s = crate::system_art::get(system)?;
    // A 4:3 picture, the console about half its height, as a menu would show it.
    let (iw, ih) = (160u32, 120u32);
    let mut img = Image { w: iw, h: ih, px: vec![0x14191a; (iw * ih) as usize] };
    let f = (ih as f32 * 0.55 / s.h as f32).min(iw as f32 * 0.6 / s.w as f32);
    let (sw, sh) = ((s.w as f32 * f) as u32, (s.h as f32 * f) as u32);
    let (ox, oy) = ((iw - sw.min(iw)) / 2, (ih - sh.min(ih)) / 2);
    for y in 0..sh.min(ih) {
        for x in 0..sw.min(iw) {
            let p = s.px[(((y as f32 / f) as u32).min(s.h - 1) * s.w + ((x as f32 / f) as u32).min(s.w - 1)) as usize];
            if p >> 24 != 0 {
                img.px[((oy + y) * iw + ox + x) as usize] = p & 0xffffff;
            }
        }
    }
    Some(crt(&img, w, h))
}

/// What the screen shows for `w`: its picture through the CRT look, else its console. Reads the disk.
fn make(dir: &Path, w: &Want) -> Option<Pic> {
    source(dir, &w.system, &w.rom, w.cover.as_deref())
        .and_then(|p| crate::image::load_image(&p))
        .map(|img| crt(&img, w.w, w.h))
        .or_else(|| console_screen(&w.system, w.w, w.h))
}

/// What the UI asks the thread for: a game's screen at a size.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Want {
    pub system: String,
    pub rom: PathBuf,
    pub cover: Option<PathBuf>,
    pub w: i32,
    pub h: i32,
}

/// The `cabinet-tv` thread and the screens it made. `get` never waits: it asks once and answers None until the
/// screen is there (or Some(None): the game has no picture at all, the TV shows the tube).
pub struct Loader {
    tx: Option<Sender<(u64, Want)>>,
    /// What the thread did with a wish: None skipped (a newer one came), Some(None) the game has no picture.
    rx: Option<Receiver<(u64, Want, Option<Option<Pic>>)>>,
    /// Invalidating pictures keeps this worker; replies from earlier generations cannot refill its cache.
    generation: u64,
    data_dir: PathBuf,
    done: HashMap<Want, Option<std::rc::Rc<Pic>>>,
    order: VecDeque<Want>,
    asked: std::collections::HashSet<Want>,
}

impl Loader {
    pub fn new(data_dir: &Path) -> Loader {
        Loader { tx: None, rx: None, generation: 0, data_dir: data_dir.to_path_buf(), done: HashMap::new(), order: VecDeque::new(), asked: Default::default() }
    }

    /// Picture files changed, possibly at the same paths. Keep one decoder thread, discard its stale results.
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.done.clear();
        self.order.clear();
        self.asked.clear();
    }

    fn start(&mut self) {
        let (tx, jobs) = channel::<(u64, Want)>();
        let (back, rx) = channel();
        let dir = self.data_dir.clone();
        let started = std::thread::Builder::new().name("cabinet-tv".into()).spawn(move || {
            while let Ok((mut generation, mut w)) = jobs.recv() {
                // Only the newest wish counts: a held direction asks for every game it passes.
                while let Ok((new_generation, newer)) = jobs.try_recv() {
                    let _ = back.send((generation, w, None));
                    generation = new_generation;
                    w = newer;
                }
                let pic = make(&dir, &w);
                if back.send((generation, w, Some(pic))).is_err() {
                    break;
                }
            }
        });
        if started.is_ok() {
            self.tx = Some(tx);
            self.rx = Some(rx);
        }
    }

    /// Takes in what the thread finished; true when something new is there (the frame repaints).
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else { return false };
        let mut any = false;
        while let Ok((generation, w, made)) = rx.try_recv() {
            // In particular, an old reply for the same Want must not clear the new generation's pending request.
            if generation != self.generation {
                continue;
            }
            self.asked.remove(&w);
            // Skipped for a newer wish: asked again when it is wanted again.
            let Some(pic) = made else { continue };
            self.order.push_back(w.clone());
            self.done.insert(w, pic.map(std::rc::Rc::new));
            while self.order.len() > KEEP {
                if let Some(old) = self.order.pop_front() {
                    self.done.remove(&old);
                }
            }
            any = true;
        }
        any
    }

    /// The screen for `w`: Some(Some) ready, Some(None) the game has no picture, None not made yet (asked now).
    pub fn get(&mut self, w: &Want) -> Option<Option<std::rc::Rc<Pic>>> {
        if let Some(p) = self.done.get(w) {
            return Some(p.clone());
        }
        if !self.asked.contains(w) {
            if self.tx.is_none() {
                self.start();
            }
            if let Some(tx) = &self.tx {
                if tx.send((self.generation, w.clone())).is_ok() {
                    self.asked.insert(w.clone());
                }
            }
        }
        None
    }

    /// Something asked for is still being made (the frames go on until it is there).
    pub fn waiting(&self) -> bool {
        !self.asked.is_empty()
    }

    /// Made synchronously, for snapshots (no thread, no wait).
    pub fn settle(&mut self, w: &Want) {
        let pic = make(&self.data_dir, w);
        self.order.push_back(w.clone());
        self.done.insert(w.clone(), pic.map(std::rc::Rc::new));
    }
}

/// The TV's size for a screen: `f` (screen pixels a template pixel) or None when there is no room for it. It stands
/// on the top board in `room` = (width beside the case out of the shelf, height of the showcase); below 60 px of glass
/// height (a 640 x 480 screen with a tall case out) it is left out rather than shown as a smudge.
pub fn scale_for(room_w: i32, room_h: i32, wanted: f32) -> Option<f32> {
    let t = template();
    let (tw, th) = (t.body.w as f32, t.body.h as f32);
    let f = wanted.min(room_w as f32 / tw).min(room_h as f32 / th);
    let glass_h = t.glass.3 as f32 * f;
    (glass_h >= 60.0).then_some(f)
}

/// Copies `screen` (the glass box's size) into the glass of `body` at `alpha` (0-255) over what is there.
pub fn show(body: &mut Pic, glass: (i32, i32, i32, i32), mask: &[bool], screen: &Pic, alpha: u32) {
    let (gx, gy, gw, gh) = glass;
    for y in 0..gh.min(screen.h) {
        for x in 0..gw.min(screen.w) {
            if !mask[(y * gw + x) as usize] {
                continue;
            }
            let s = screen.px[(y * screen.w + x) as usize];
            body.blend(gx + x, gy + y, s & 0xffffff, alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidation_reuses_the_worker_and_ignores_old_replies_for_the_same_picture() {
        let dir = crate::testutil::TestDir::new("tv-generation");
        let want = Want { system: "snes".into(), rom: PathBuf::from("Game.sfc"), cover: None, w: 4, h: 4 };
        let mut loader = Loader::new(&dir);
        // Use the worker's real channels, controlling when it answers instead of racing the filesystem decoder.
        let (tx, jobs) = channel();
        let (back, rx) = channel();
        loader.tx = Some(tx);
        loader.rx = Some(rx);
        let cached = Want { rom: PathBuf::from("Cached.sfc"), ..want.clone() };
        loader.done.insert(cached.clone(), None);
        loader.order.push_back(cached);
        assert!(loader.get(&want).is_none());
        let (old_generation, old_want) = jobs.recv().unwrap();

        loader.invalidate();
        assert!(loader.done.is_empty() && loader.order.is_empty() && !loader.waiting());
        assert!(loader.get(&want).is_none());
        let (new_generation, new_want) = jobs.recv().unwrap();
        assert_ne!(old_generation, new_generation, "new work still uses the existing worker's channel");

        back.send((old_generation, old_want.clone(), Some(Some(Pic::new(4, 4, 0xffff0000))))).unwrap();
        back.send((old_generation, old_want, None)).unwrap();
        assert!(!loader.poll(), "old completed and skipped replies request no repaint");
        assert!(loader.done.is_empty(), "an old image cannot refill the new cache");
        assert!(loader.waiting(), "nor clear the same Want's new pending request");

        back.send((new_generation, new_want, Some(Some(Pic::new(4, 4, 0xff0000ff))))).unwrap();
        assert!(loader.poll());
        assert!(!loader.waiting());
        let pic = loader.get(&want).unwrap().unwrap();
        assert_eq!(pic.px[0], 0xff0000ff, "only the new generation's blue image is published");
        assert!(matches!(jobs.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)), "no duplicate job after the stale reply");
    }

    #[test]
    fn the_tv_has_a_glass_and_the_body_is_solid_or_clear() {
        let t = template();
        let (_, _, gw, gh) = t.glass;
        assert!(gw > 30 && gh > 25, "{:?}", t.glass);
        assert!(t.body.px.iter().all(|p| p >> 24 == 0 || p >> 24 == 0xff));
        let b = body(2.5);
        assert_eq!(b.mask.len(), (b.glass.2 * b.glass.3) as usize);
        assert!(b.mask.iter().filter(|m| **m).count() as i32 > b.glass.2 * b.glass.3 * 3 / 4);
        assert!(!b.pic.px.iter().any(|p| p & 0xffffff == GLASS && p >> 24 != 0), "no magenta left");
    }

    #[test]
    fn the_crt_look_keeps_the_picture_recognisable() {
        // Left half red, right half blue: after the look the left is still red, the right blue, rows alternate.
        let img = Image { w: 64, h: 48, px: (0..64 * 48).map(|i| if i % 64 < 32 { 0xd02020 } else { 0x2020d0 }).collect() };
        let p = crt(&img, 100, 80);
        let at = |x: i32, y: i32| p.px[(y * 100 + x) as usize];
        let red = |c: u32| c >> 16 & 0xff;
        let blue = |c: u32| c & 0xff;
        assert!(red(at(25, 40)) > 150 && blue(at(25, 40)) < 60);
        assert!(blue(at(75, 40)) > 120 && red(at(75, 40)) < 60);
        assert!(red(at(25, 41)) < red(at(25, 40)) || red(at(25, 40)) < red(at(25, 41)), "scanlines");
        assert!(red(at(2, 2)) < red(at(25, 40)), "darker at the rim");
        assert_eq!(crt(&Image { w: 0, h: 0, px: vec![] }, 10, 10).px[0] & 0xffffff, TUBE);
    }

    #[test]
    fn a_game_without_pictures_shows_its_console_on_the_tube() {
        let p = console_screen("snes", 90, 70).expect("the SNES has a picture");
        let tube = p.px.iter().filter(|c| **c & 0xffffff == TUBE).count();
        let lit = p.px.iter().filter(|c| (*c >> 16 & 0xff) > 90 || (*c >> 8 & 0xff) > 90 || (*c & 0xff) > 90).count();
        assert!(lit > 300, "the console shows: {lit} light pixels, {tube} of the tube");
        assert!(console_screen("no-such-system", 90, 70).is_none());
    }

    #[test]
    fn a_tv_too_small_to_read_is_left_out() {
        assert!(scale_for(400, 300, 2.5).is_some());
        assert!(scale_for(60, 50, 2.5).is_none());
        let f = scale_for(150, 1000, 2.5).unwrap();
        assert!(template().body.w as f32 * f <= 150.0 + 0.01);
    }

    #[test]
    fn the_picture_is_the_screenshot_then_the_players_then_the_cover() {
        let d = crate::testutil::TestDir::new("tv-source");
        let rom = Path::new("/roms/snes/Game (USA).sfc");
        let cover = d.join("cover.png");
        assert_eq!(source(&d, "snes", rom, Some(&cover)), None, "nothing at all");
        std::fs::write(&cover, b"x").unwrap();
        assert_eq!(source(&d, "snes", rom, Some(&cover)), Some(cover.clone()));
        let frame = crate::lastframe::frame_path(&d, "snes", rom);
        std::fs::create_dir_all(frame.parent().unwrap()).unwrap();
        std::fs::write(&frame, b"x").unwrap();
        assert_eq!(source(&d, "snes", rom, Some(&cover)), Some(frame));
        let snap = crate::game_pictures::path(&d, crate::game_pictures::Kind::Snap, "snes", rom);
        std::fs::create_dir_all(snap.parent().unwrap()).unwrap();
        std::fs::write(&snap, b"x").unwrap();
        assert_eq!(source(&d, "snes", rom, Some(&cover)), Some(snap));
    }
}
