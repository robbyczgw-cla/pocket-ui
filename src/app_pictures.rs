//! Part of the pocket-ui binary: the games' screenshots and title screens on the shelf (`pocket_ui::game_pictures`
//! downloads them next to the box art, `pocket_ui::picture_cycle` times the cycle, `pocket_ui::picture_loader`
//! decodes them). The same worker also reads and fits the ordinary box-art covers (`App::cover`).
//!
//! * The Split look's preview shows the box art; after a short dwell on the selection it fades to the game
//!   running, then to the title screen, and round again (`paint_preview`). Motion: Off keeps the box art still.
//! * The game card shows the same two pictures under the facts (`card_pictures`).
//! * Nothing is read while the selection is moving: the files are looked at (24 bytes of header each) once it has
//!   rested [`REST`], and decoded on the loader thread into the same image cache as the box art (`App::covers`),
//!   at most [`KEEP`] of them at a time. A game without a picture shows its box art only; with neither it is the
//!   console's picture as before.
//! * One wake-up per change from `about_to_wait` (`pictures_tick`); nothing between.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_ui::game_card::{self, CardPicture};
use pocket_ui::game_pictures::{self, Kind};
use pocket_ui::gfx::Canvas;
use pocket_ui::image::{self, Image};
use pocket_ui::motion::{self, Kind as Motion};
use pocket_ui::picture_cycle::{self, Slide};
use pocket_ui::picture_loader::{self, Key, Loader};

use super::{App, Item};

/// The selection has to rest this long before its pictures are looked for.
pub(crate) const REST: Duration = Duration::from_millis(400);
/// Decoded covers, screenshots and title screens kept together. Scrolling through a large library must
/// not keep one decoded image per game and size. Cabinet case artwork has its own caches.
const KEEP: usize = 16;

pub(crate) struct PicturesUi {
    /// Settings > Games > Extras > Game pictures: on, or "Box art only".
    wanted: bool,
    /// What the header of a picture file said: its size; `None` when the file is not there (or not a PNG that may
    /// be decoded). Looked at once per file, until the files change.
    sizes: HashMap<PathBuf, Option<(u32, u32)>>,
    loader: Option<Loader>,
    /// Asked for, not here yet.
    pending: HashSet<Key>,
    /// Decoded, oldest first: the cache is trimmed from the front.
    loaded: VecDeque<Key>,
    /// The game the preview is on, since when its cycle runs, and when to draw it again.
    focus: Option<PathBuf>,
    since: Instant,
    wake: Option<Instant>,
    /// `--snapshot`: pictures are decoded on the spot instead of on the loader thread.
    pub(crate) sync: bool,
}

impl PicturesUi {
    pub fn new(wanted: bool) -> Self {
        PicturesUi { wanted, sizes: HashMap::new(), loader: None, pending: HashSet::new(), loaded: VecDeque::new(), focus: None, since: Instant::now(), wake: None, sync: false }
    }

    pub fn wanted(&self) -> bool {
        self.wanted
    }

    pub fn set_wanted(&mut self, on: bool) {
        self.wanted = on;
        self.focus = None;
        self.wake = None;
    }
}

/// A picture with its place, as the canvas wants it.
fn placed(p: &Option<(Rc<Image>, i32, i32)>) -> Option<(&Image, i32, i32)> {
    p.as_ref().map(|(i, x, y)| (&**i, *x, *y))
}

impl App {
    /// Files came or went (a download ended, the library was read again): look again.
    pub(crate) fn pictures_changed(&mut self) {
        self.cabinet.tv.pictures_changed();
        self.pictures.sizes.clear();
        self.pictures.loaded.clear();
        self.pictures.pending.clear();
        self.pictures.focus = None;
    }

    /// The existing screenshot and title screen of a game with the size their headers give, in the order they
    /// cycle (the game running, then the title). Empty for box art only.
    fn picture_files(&mut self, item: Item) -> Vec<(Kind, PathBuf, (u32, u32))> {
        if !self.pictures.wanted {
            return Vec::new();
        }
        let Some((system, rom)) = self.item_rom(item) else { return Vec::new() };
        let mut found = Vec::new();
        for kind in Kind::ALL {
            let file = game_pictures::path(&self.data_dir, kind, &system, &rom);
            let size = *self.pictures.sizes.entry(file.clone()).or_insert_with(|| image::png_size(&file));
            if let Some(size) = size {
                found.push((kind, file, size));
            }
        }
        found
    }

    /// A picture fitted into `w` x `h`, from the image cache; asks the loader thread for it when it is not there
    /// and gives `None` until it is.
    pub(crate) fn picture(&mut self, file: &Path, w: i32, h: i32) -> Option<Rc<Image>> {
        let key: Key = (file.to_path_buf(), w, h);
        if let Some(have) = self.covers.get(&key) {
            return have.clone();
        }
        if self.pictures.sync {
            let img = picture_loader::load_fitted(&key).map(Rc::new);
            self.covers.insert(key, img.clone());
            return img;
        }
        if self.pictures.pending.insert(key.clone()) {
            self.request_picture(key);
        }
        None
    }

    /// Asks the loader thread for a picture (starting it at the first request); the answer is `UserEvent::Picture`.
    /// The gallery's pictures go through the same thread (`app_gallery`).
    pub(crate) fn request_picture(&mut self, key: Key) {
        let proxy = self.proxy.clone();
        let loader = self.pictures.loader.get_or_insert_with(|| {
            Loader::start(move |key, img| {
                let _ = proxy.send_event(super::UserEvent::Picture(key, img));
            })
        });
        loader.request(key);
    }

    /// The loader thread has a picture: into the image cache, and a frame to show it.
    pub(crate) fn picture_arrived(&mut self, key: Key, img: Option<Image>) {
        // A picture of the gallery has caches of its own (`app_gallery`).
        let Some(img) = self.gallery_picture_arrived(&key, img) else { return };
        self.pictures.pending.remove(&key);
        while self.pictures.loaded.len() >= KEEP {
            if let Some(old) = self.pictures.loaded.pop_front() {
                self.covers.remove(&old);
            }
        }
        self.covers.insert(key.clone(), img.map(Rc::new));
        self.pictures.loaded.push_back(key);
        self.request_redraw();
    }

    /// Split's preview: box art, then the screenshot, then the title screen, each fading into the next. Paints
    /// into the box (x, y, w, h) over `bg`; false when the game has none of them (the caller shows the console).
    pub(crate) fn paint_preview(&mut self, c: &mut Canvas, item: Item, (x, y, w, h): (i32, i32, i32, i32), bg: u32) -> bool {
        let now = Instant::now();
        // Systems and the like have no pictures; the look step's sample game keeps its box art (it is not a game of the
        // library, and the previews are painted once).
        let rom = self.item_rom(item).map(|(_, rom)| rom).filter(|_| self.sample_cover.is_none());
        if rom.is_none() {
            self.pictures.wake = None;
            return self.cover(item, w, h).is_some_and(|img| {
                c.blit_scaled(&img, x + (w - img.w as i32) / 2, y + (h - img.h as i32) / 2, img.w as i32, img.h as i32);
                true
            });
        }
        let box_art = self.cover(item, w, h);
        // The same game as the last frame goes on with its cycle; another one starts it.
        if self.pictures.focus != rom {
            self.pictures.focus = rom;
            self.pictures.since = now;
        }
        let elapsed = now.saturating_duration_since(self.pictures.since);
        // Nothing but the box art while the selection is still moving.
        let files = if elapsed >= REST { self.picture_files(item) } else { Vec::new() };
        let slides = picture_cycle::slides(box_art.is_some(), files.len());
        if slides.is_empty() {
            self.pictures.wake = rest_wake(now, elapsed, self.pictures.wanted);
            return false;
        }
        let step = picture_cycle::step(slides.len(), elapsed, picture_cycle::cycling(), motion::span(Motion::Fade, picture_cycle::FADE));
        // The pictures are asked for as soon as the selection has rested, so they are there when the fade starts.
        let image_of = |app: &mut App, slide: &Slide| match slide {
            Slide::BoxArt => box_art.clone(),
            Slide::Picture(i) => app.picture(&files[*i].1, w, h),
        };
        let (from, to) = (image_of(self, &slides[step.from]), image_of(self, &slides[step.to]));
        if slides.len() > 1 {
            for slide in &slides {
                image_of(self, slide);
            }
        }
        let place = |img: &Rc<Image>| (img.clone(), x + (w - img.w as i32) / 2, y + (h - img.h as i32) / 2);
        let (from, to) = (from.as_ref().map(place), to.as_ref().map(place));
        // A fade into a picture that has not arrived yet waits for it.
        let showing = if step.mix >= 1.0 || to.is_none() { to.as_ref().or(from.as_ref()) } else { None };
        match showing {
            Some((img, ix, iy)) => c.blit_scaled(img, *ix, *iy, img.w as i32, img.h as i32),
            None if from.is_none() && to.is_none() => return false,
            None => {
                c.crossfade(placed(&from), placed(&to), bg, (step.mix * 255.0) as u32);
            }
        }
        // The next look: when the fade ends frames are drawn until then; later one wake-up per change.
        let mut wake = step.next.map(|d| now + d);
        if elapsed < REST && slides.len() < 3 && self.pictures.wanted {
            wake = Some(wake.map_or(now + (REST - elapsed), |w| w.min(now + (REST - elapsed))));
        }
        if let Some(left) = step.fading {
            motion::keep_alive(now + left);
        }
        self.pictures.wake = wake;
        true
    }

    /// From `about_to_wait`: the frame for the preview's next change, once, when it is due.
    pub(crate) fn pictures_tick(&mut self, now: Instant) -> Option<Instant> {
        let at = self.pictures.wake?;
        if self.running.is_some() || self.sleep.state() != pocket_ui::sleep::State::Awake {
            self.pictures.wake = None;
            return None;
        }
        if now >= at {
            self.pictures.wake = None;
            self.request_redraw();
            return None;
        }
        Some(at)
    }

    /// The pictures under the game card's facts: what the game has, in the places their headers give, with the
    /// decoded image once the loader has it. Empty for box art only or a game without any.
    pub(crate) fn card_pictures(&mut self, item: Item, screen_w: i32) -> Vec<CardPicture> {
        let (bw, bh) = game_card::picture_box(screen_w);
        self.picture_files(item)
            .into_iter()
            .map(|(kind, file, (w, h))| CardPicture {
                label: match kind {
                    Kind::Snap => "In the game",
                    Kind::Title => "Title screen",
                },
                size: picture_loader::fitted(w, h, bw, bh),
                image: self.picture(&file, bw, bh),
            })
            .collect()
    }
}

impl App {
    /// `--snapshot pictures:<which>[:<system>]`: Split's list of a system whose first game has a screenshot and a title
    /// screen in `POCKETUI_DATA/snaps` and `titles`, with the preview at a point of its cycle: `box` (before the
    /// first change), `snap`, `title`, `fade` (halfway from the box art to the screenshot), `off` (Motion: Off, long
    /// after the dwell: still the box art), `none` (a game without pictures: box art only), and `card` (that
    /// game's card, with both pictures under the facts). The pictures are decoded at once.
    pub(crate) fn pictures_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("pictures:") else { return false };
        let (which, system) = rest.split_once(':').map_or((rest, None), |(w, s)| (w, Some(s)));
        self.pictures.sync = true;
        self.pictures.wanted = true;
        self.theme = pocket_ui::settings::Theme::Split;
        // The first game with a screenshot (or, for `none`, the first without), in the wanted system.
        let has_snap = |app: &App, s: usize, g: usize| {
            let sys = &app.systems[s];
            game_pictures::path(&app.data_dir, Kind::Snap, &sys.name, &sys.games[g].path).exists()
        };
        let wanted_none = which == "none";
        let found = (0..self.systems.len())
            .filter(|&s| system.is_none_or(|n| self.systems[s].name == n))
            .find_map(|s| (0..self.systems[s].games.len()).find(|&g| has_snap(self, s, g) != wanted_none).map(|g| (s, g)));
        let Some((s, g)) = found else {
            eprintln!("snapshot pictures: no game {} a screenshot in the data folder", if wanted_none { "without" } else { "with" });
            return true;
        };
        self.screen = super::Screen::Games(s);
        self.rebuild();
        if let Some(i) = self.items.iter().position(|it| *it == Item::Game(s, g)) {
            self.sel = i;
            self.prev_sel = i;
        }
        let ago = match which {
            "snap" => picture_cycle::DWELL + picture_cycle::FADE + Duration::from_millis(100),
            "title" => picture_cycle::DWELL + picture_cycle::HOLD + picture_cycle::FADE + Duration::from_millis(100),
            "fade" => picture_cycle::DWELL + Duration::from_millis(120),
            "off" => Duration::from_secs(30),
            _ => REST + Duration::from_millis(100),
        };
        if which == "off" {
            motion::set_level(pocket_ui::settings::Motion::Off);
        }
        self.pictures.focus = self.item_rom(Item::Game(s, g)).map(|(_, rom)| rom);
        self.pictures.since = Instant::now() - ago;
        self.moved_at = Instant::now() - Duration::from_secs(2);
        if which.starts_with("card") {
            self.open_card();
        }
        true
    }
}

/// When to look again for a resting selection's pictures: at the end of the rest, if they are wanted and the rest
/// isn't over. Lazy on purpose: `then_some(now + (REST - elapsed))` computed `REST - elapsed` even after the rest and
/// panicked ("overflow when subtracting durations") on a game without pictures that stayed selected (2026-10-24).
fn rest_wake(now: Instant, elapsed: std::time::Duration, wanted: bool) -> Option<Instant> {
    (wanted && elapsed < REST).then(|| now + (REST - elapsed))
}

#[cfg(test)]
mod rest_wake_tests {
    use super::*;

    #[test]
    fn a_selection_resting_past_the_rest_needs_no_wake_and_never_panics() {
        let now = Instant::now();
        assert_eq!(rest_wake(now, REST + std::time::Duration::from_secs(5), true), None);
        assert_eq!(rest_wake(now, REST * 3, false), None);
        assert_eq!(rest_wake(now, std::time::Duration::ZERO, true), Some(now + REST));
        assert_eq!(rest_wake(now, std::time::Duration::ZERO, false), None);
    }
}
