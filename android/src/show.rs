//! The idle slideshow on a phone (Settings → Slideshow before screen off, off until switched on): after a quiet minute
//! on the Games or Favourites page the phone shows the player's games one after another, the cover, a picture of the
//! game, the player's own save pictures and screenshots, each with a slow pan and zoom, fading into the next, the title
//! and system small at the bottom. Any touch, key or Back ends it and does nothing else. It ends by itself after ten
//! minutes, and while it can run the screen is kept on (Android's own screen-off time would come first otherwise).
//! The timing and painting are `pocket_ui::slideshow`, the same as the handheld's; this is the phone's part: which
//! pictures there are (the launcher keeps them by the games' hashes, `collect`), the thread that prepares them, and
//! when it starts and stops.
use pocket_ui::{
    gfx::Canvas,
    slideshow::{self, Rng, Scene, Slide, Source},
    text::{Face, Text},
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(PartialEq, Clone, Copy)]
enum Phase {
    Waiting,
    Collecting,
    Playing,
}

/// A game as the show needs it, taken from the library when the show starts.
pub struct Entry {
    pub name: String,
    pub system: String,
    /// The hash the covers and pictures are kept under, and the key of its saves and screenshots (`ui::game_key`).
    pub art: String,
    pub key: String,
    pub stem: String,
}

pub struct Show {
    last_input: Instant,
    phase: Phase,
    slides: Vec<Slide>,
    scenes: Vec<Option<Scene>>,
    asked: Vec<bool>,
    started: Instant,
    generation: u64,
    slides_rx: Option<mpsc::Receiver<(u64, Vec<Slide>)>>,
    scene_tx: mpsc::Sender<(u64, usize, Option<Scene>)>,
    scene_rx: mpsc::Receiver<(u64, usize, Option<Scene>)>,
    size: (i32, i32),
    /// It ran its ten minutes or found nothing to show: no new start until the player touches the phone.
    done: bool,
}

/// What `Show::tick` asks of the host.
pub struct Wake {
    /// When to look again (None: nothing to wait for).
    pub at: Option<Instant>,
    /// Keep the screen on: the show can start or is running.
    pub keep_screen_on: bool,
}

impl Show {
    pub fn new() -> Self {
        let (scene_tx, scene_rx) = mpsc::channel();
        Self {
            last_input: Instant::now(),
            phase: Phase::Waiting,
            slides: Vec::new(),
            scenes: Vec::new(),
            asked: Vec::new(),
            started: Instant::now(),
            generation: 0,
            slides_rx: None,
            scene_tx,
            scene_rx,
            size: (0, 0),
            done: false,
        }
    }
    /// A picture is on the screen.
    pub fn visible(&self) -> bool {
        self.phase == Phase::Playing && self.scenes.iter().any(Option::is_some)
    }
    /// Any input: the quiet minute starts again. True when the show was on screen: that input only ended it.
    pub fn input(&mut self, now: Instant) -> bool {
        self.last_input = now;
        self.done = false;
        let was = self.visible();
        if self.phase != Phase::Waiting {
            self.stop();
        }
        was
    }
    fn stop(&mut self) {
        self.phase = Phase::Waiting;
        self.generation += 1;
        self.slides.clear();
        self.scenes.clear();
        self.asked.clear();
        self.slides_rx = None;
    }
    /// One look from the event loop. `games` is asked for only when the show starts; `size` is the canvas it paints on.
    pub fn tick(&mut self, eligible: bool, size: (i32, i32), games: impl FnOnce() -> Vec<Entry>, data: &Path, now: Instant) -> Wake {
        if !eligible {
            if self.phase != Phase::Waiting {
                self.stop();
            }
            return Wake { at: None, keep_screen_on: false };
        }
        self.size = size;
        let keep = !self.done;
        match self.phase {
            Phase::Waiting => {
                if self.done {
                    return Wake { at: None, keep_screen_on: false };
                }
                let due = self.last_input + slideshow::IDLE_AFTER;
                if now >= due {
                    let (tx, rx) = mpsc::channel();
                    let (generation, data, games) = (self.generation, data.to_path_buf(), games());
                    let seed = now.duration_since(self.last_input).as_nanos() as u64 ^ games.len() as u64;
                    std::thread::spawn(move || {
                        let _ = tx.send((generation, collect(&games, &data, seed)));
                    });
                    self.slides_rx = Some(rx);
                    self.phase = Phase::Collecting;
                    return Wake { at: Some(now + Duration::from_millis(200)), keep_screen_on: keep };
                }
                Wake { at: Some(due), keep_screen_on: keep }
            }
            Phase::Collecting => {
                if let Some((generation, slides)) = self.slides_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
                    self.slides_rx = None;
                    if generation != self.generation || slides.is_empty() {
                        // Nothing real to show: no pictures downloaded yet. Quiet until the next touch.
                        self.phase = Phase::Waiting;
                        self.done = true;
                        return Wake { at: None, keep_screen_on: false };
                    }
                    self.scenes = (0..slides.len()).map(|_| None).collect();
                    self.asked = vec![false; slides.len()];
                    self.slides = slides;
                    self.phase = Phase::Playing;
                    self.started = now;
                }
                Wake { at: Some(now + Duration::from_millis(200)), keep_screen_on: keep }
            }
            Phase::Playing => {
                if now.duration_since(self.started) >= slideshow::MAX_RUN {
                    self.stop();
                    self.done = true;
                    return Wake { at: None, keep_screen_on: false };
                }
                while let Ok((generation, index, scene)) = self.scene_rx.try_recv() {
                    if generation == self.generation && index < self.scenes.len() {
                        self.scenes[index] = scene;
                    }
                }
                self.ask(now);
                Wake { at: Some(now + slideshow::FRAME), keep_screen_on: true }
            }
        }
    }
    /// The slide on screen and the next one are prepared on a thread; the ones far from here are let go.
    fn ask(&mut self, now: Instant) {
        let len = self.slides.len();
        let n = (now.duration_since(self.started).as_millis() / slideshow::HOLD.as_millis()) as usize;
        let want = [(n + len - 1) % len, n % len, (n + 1) % len];
        for i in 0..len {
            if !want.contains(&i) {
                self.scenes[i] = None;
                self.asked[i] = false;
            }
        }
        for &i in &want {
            if !self.asked[i] {
                self.asked[i] = true;
                let (tx, generation, file, (w, h)) = (self.scene_tx.clone(), self.generation, self.slides[i].file.clone(), self.size);
                std::thread::spawn(move || {
                    let _ = tx.send((generation, i, slideshow::prepare(&file, w, h)));
                });
            }
        }
    }
    /// The frame at `now`: the slide before it moving on under the new one fading in, then its title.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, now: Instant, still: bool) {
        c.clear(0);
        let len = self.slides.len();
        if len == 0 {
            return;
        }
        let elapsed = now.duration_since(self.started);
        let n = (elapsed.as_millis() / slideshow::HOLD.as_millis()) as usize;
        let local = elapsed - slideshow::HOLD * n as u32;
        if n > 0 {
            if let Some(scene) = &self.scenes[(n - 1) % len] {
                slideshow::paint(c, scene, slideshow::view(n - 1, slideshow::progress(local + slideshow::HOLD), still), 255);
            }
        }
        if let Some(scene) = &self.scenes[n % len] {
            let alpha = (slideshow::fade_mix(local) * 255.0) as u8;
            slideshow::paint(c, scene, slideshow::view(n, slideshow::progress(local), still), alpha);
        }
        let slide = &self.slides[n % len];
        let alpha = (slideshow::title_alpha(local) * 255.0) as u32;
        if alpha > 0 {
            let size = (c.h / 34).clamp(16, 44) as u32;
            let margin = c.w / 14;
            // A dark band under the words so they read on any picture.
            let band = size as i32 * 3;
            for y in c.h - band..c.h {
                let a = ((y - (c.h - band)) * alpha as i32 / band.max(1) / 2).clamp(0, 140) as u8;
                c.shade_rect(0, y, c.w, 1, 0, a);
            }
            let title = text.fit(Face::Title, size, &slide.title, c.w - 2 * margin);
            let ink = fade(0xffffff, alpha);
            text.draw(c, Face::Title, size, margin, c.h - size as i32 * 3 / 2, ink, &title);
            let line = match slide.source.caption() {
                Some(caption) => {
                    let caption = text.translated(caption).to_string();
                    format!("{} · {caption}", slide.system)
                }
                None => slide.system.clone(),
            };
            let line = text.fit(Face::Body, size * 3 / 4, &line, c.w - 2 * margin);
            text.draw(c, Face::Body, size * 3 / 4, margin, c.h - size as i32 / 3, fade(0xdddddd, alpha), &line);
        }
    }
}

/// The pictures the show cycles through, two scraped ones (box art, a picture of the game) to one memory (a save's
/// picture, a screenshot), as the handheld's `slideshow::collect`, over the phone's own folders: covers and pictures by
/// the games' hashes, saves under `states/<key>/`, screenshots as `screenshots/<key>-<time>.png`.
fn collect(games: &[Entry], data: &Path, seed: u64) -> Vec<Slide> {
    let mut rng = Rng(seed | 1);
    let mut order: Vec<usize> = (0..games.len()).collect();
    rng.shuffle(&mut order);
    let usable = |f: &Path| pocket_ui::image::png_size(f).is_some_and(|(w, h)| w >= slideshow::MIN_SIDE && h >= slideshow::MIN_SIDE);
    let mut scraped = Vec::new();
    let mut memories = Vec::new();
    for &i in order.iter().take(slideshow::MAX_GAMES_LOOKED_AT) {
        let g = &games[i];
        let mut tries = vec![(Source::Cover, data.join("covers").join(format!("{}.png", g.art))), (Source::Snap, data.join("pictures").join(format!("{}.png", g.art)))];
        if rng.next_u64() & 1 == 1 {
            tries.reverse();
        }
        if scraped.len() < slideshow::MAX_SLIDES * 2 / 3 {
            if let Some((source, file)) = tries.into_iter().find(|(_, f)| usable(f)) {
                scraped.push(Slide { file, title: g.name.clone(), system: g.system.clone(), source });
            }
        }
        if memories.len() < slideshow::MAX_SLIDES {
            let base = data.join("states").join(&g.key).join(&g.stem);
            let mut found: Vec<PathBuf> = (0..pocket_ui::slots::SLOTS)
                .map(|n| pocket_ui::slots::preview_path(&pocket_ui::slots::state_path(&base, n)))
                .chain([pocket_ui::slots::preview_path(&pocket_ui::slots::auto_path(&base))])
                .filter(|f| usable(f))
                .collect();
            if !found.is_empty() {
                let file = found.swap_remove(rng.below(found.len()));
                memories.push(Slide { file, title: g.name.clone(), system: g.system.clone(), source: Source::Save });
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(data.join("screenshots")) {
        let mut files: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        rng.shuffle(&mut files);
        for file in files.into_iter().take(12) {
            let Some(name) = file.file_name().and_then(|n| n.to_str()) else { continue };
            let Some(key) = name.strip_suffix(".png").and_then(|s| s.rsplit_once('-')).map(|(k, _)| k) else { continue };
            if let (Some(g), true) = (games.iter().find(|g| g.key == key), usable(&file)) {
                memories.push(Slide { file, title: g.name.clone(), system: g.system.clone(), source: Source::Screenshot });
            }
        }
    }
    rng.shuffle(&mut memories);
    memories.truncate(slideshow::MAX_SLIDES.saturating_sub(scraped.len()).max(slideshow::MAX_SLIDES / 3));
    let mut out = Vec::new();
    let (mut s, mut m) = (scraped.into_iter(), memories.into_iter());
    while out.len() < slideshow::MAX_SLIDES {
        let before = out.len();
        out.extend(s.next());
        out.extend(s.next());
        out.extend(m.next());
        if out.len() == before {
            break;
        }
    }
    out.truncate(slideshow::MAX_SLIDES);
    out
}

/// `colour` towards black by `alpha` of 255: the words come in and go out with the title's timing.
fn fade(colour: u32, alpha: u32) -> u32 {
    let ch = |shift: u32| ((colour >> shift & 0xff) * alpha.min(255) / 255) << shift;
    ch(16) | ch(8) | ch(0)
}
