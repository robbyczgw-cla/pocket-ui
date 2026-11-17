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

use crate::cabinet_art::Pic;
use crate::system_art::Sprite;
#[cfg(test)]
use crate::image::Image;

#[path = "cabinet_tv_art.rs"]
mod art;
pub use art::*;
#[cfg(test)]
use art::GLASS;

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

/// What the screen shows for `w`: its picture through the CRT look (a handheld's display: the LCD look), else on the
/// TV its console (a handheld's display stays lit without a picture). Reads the disk.
fn make(dir: &Path, w: &Want) -> Option<Pic> {
    let img = source(dir, &w.system, &w.rom, w.cover.as_deref()).and_then(|p| crate::image::load_image(&p));
    if w.lcd {
        return img.map(|img| lcd(&img, w.w, w.h));
    }
    img.map(|img| crt(&img, w.w, w.h)).or_else(|| console_screen(&w.system, w.w, w.h))
}

/// The handheld a game is shown on instead of the TV (hand test idea, 16.11.2026: a Game Boy game on a Game Boy, a PSP
/// game on a PSP, the game on its little display): the plain picture and display of the handheld its start shows
/// (`insert_scene::scene_system`, `plain_display`: the Game Boy for the Mega Duck, the GBA SP when that is the chosen
/// GBA). None for a game played on a TV, and for what is not a game.
pub fn handheld(system: &str) -> Option<(&'static Sprite, &'static crate::insert_scene::PlainDisplay)> {
    if system.is_empty() {
        return None;
    }
    let shown = crate::insert_scene::scene_system(system);
    Some((crate::system_art::get(shown)?, crate::insert_scene::plain_display(shown)?))
}

/// What the UI asks the thread for: a game's screen at a size.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Want {
    pub system: String,
    pub rom: PathBuf,
    pub cover: Option<PathBuf>,
    pub w: i32,
    pub h: i32,
    /// For a handheld's display (`handheld`): the LCD look instead of the CRT's.
    pub lcd: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidation_reuses_the_worker_and_ignores_old_replies_for_the_same_picture() {
        let dir = crate::testutil::TestDir::new("tv-generation");
        let want = Want { system: "snes".into(), rom: PathBuf::from("Game.sfc"), cover: None, w: 4, h: 4, lcd: false };
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

    /// The hand test's idea (16.11.2026): a handheld system's game shows on its handheld in the TV's place, a console's on the TV.
    /// Every handheld gets its own picture with a display big enough to see the game, inside the TV's size.
    #[test]
    fn handheld_games_show_on_their_handheld_with_the_game_on_its_display() {
        for system in ["gb", "gbc", "gba", "gamegear", "ngp", "ngpc", "atarilynx", "wonderswan", "wonderswancolor", "nds", "3ds", "psp", "megaduck", "gbav", "j2me"] {
            let (sprite, d) = handheld(system).unwrap_or_else(|| panic!("{system}: a handheld"));
            // The TV's size on the Mini V2 (3 screen pixels a template pixel).
            let t = template();
            let (w, h) = ((t.body.w * 3) as i32, (t.body.h * 3) as i32);
            let body = handheld_body(sprite, d.at, d.fuzz, d.lit, w, h).unwrap_or_else(|| panic!("{system}: its display is found"));
            assert!(body.pic.w <= w && body.pic.h <= h, "{system}: inside the TV's place");
            assert!(body.pic.w >= w * 3 / 5 || body.pic.h >= h * 3 / 5, "{system}: as big as the place allows");
            let (gx, gy, gw, gh) = body.glass;
            assert!(gx >= 0 && gy >= 0 && gx + gw <= body.pic.w && gy + gh <= body.pic.h, "{system}: the display on the handheld");
            let lit = body.mask.iter().filter(|m| **m).count();
            assert!(gw >= 30 && gh >= 24 && lit * 3 >= body.mask.len(), "{system}: a display to see the game on ({gw} x {gh}, {lit} lit)");
            assert!(lit * 2 <= (body.pic.w * body.pic.h) as usize, "{system}: the display, not the whole handheld");
        }
        for system in ["snes", "nes", "psx", "ps2", "n64", "megadrive", "arcade", ""] {
            assert!(handheld(system).is_none(), "{system}: on the TV");
        }
    }

    #[test]
    fn a_handhelds_display_shows_the_game_sharp_and_flat() {
        let img = Image { w: 4, h: 4, px: vec![0xff0000; 16] };
        let p = lcd(&img, 10, 8);
        assert_eq!((p.w, p.h), (10, 8));
        // No scanlines, no darker rim: every pixel is the picture's own (a touch dimmer).
        assert!(p.px.iter().all(|&c| c == p.px[0]) && p.px[0] & 0xff0000 > 0xe00000, "{:x}", p.px[0]);
    }
}
