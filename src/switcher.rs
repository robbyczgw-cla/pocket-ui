//! Game switcher: recently played games as a row of screens, each showing where the game was left
//! (its auto-resume picture), NextUI/Onion style. SELECT on the shelf opens it; in a game HOME + SELECT
//! (or SELECT in the pause menu) does, with the running game first. A resumes, X takes a game off the
//! list, Y stars it, B goes back.
//!
//! Pictures, first found wins (`lastframe::rank`): the preview of the auto-resume state (pocket-core's
//! `<game>.pstate.auto.png`, RetroArch's `<game>.state.auto.png`), the frame pocket-ui kept when the game
//! was quit from the pause menu, the preview of a manual save newer than that, the downloaded box art, else
//! a card in the system's colour with the picture of the console. The selected game's picture, blurred and
//! dimmed, lights the whole background.
//!
//! Pictures are read and scaled to card size on a worker thread, selected card first, so scrolling never
//! waits for a file; the cards show the system card until their picture is in. Scaled pictures are kept in a
//! small cache (`lastframe::Lru`) that outlives the switcher, so opening it again is instant.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::gfx::{mix, Canvas};
use crate::image::{self, Image};
use crate::lastframe::{self, Candidates, FileKey, Lru, Source};
use crate::marquee::Marquee;
use crate::motion::Glide;
use crate::pad::Button;
use crate::palette::*;
use crate::slots;
use crate::text::{Face, Text};
use crate::video::Scaling;

/// The carousel slides this long after a move.
/// The row slides with an ease-out that settles in about 200 ms (a card is wide: a little slower than a row).
const SLIDE_TAU: Duration = Duration::from_millis(34);
/// The box every picture is fitted into (4:3), the selected one at full size.
const BOX_W: i32 = 760;
const BOX_H: i32 = 570;
const TOP: i32 = 140;

/// Shorter screens give up picture height so title, metadata and hints retain separate rows.
fn card_height(screen_h: i32) -> i32 {
    (BOX_H - (1080 - screen_h).max(0)).max(200)
}

/// Distance between card centres; the neighbours peek in at the screen edges.
pub const STEP: f32 = 700.0;
/// Neighbours are this much smaller.
const SHRINK: f32 = 0.34;
/// Cards this far from the selected one (and nearer) have their pictures read ahead.
const PREFETCH: f32 = 3.5;

/// NextUI's pill metrics at three times its scale, as in the pause menu.
const PAD: i32 = 30;
const PILL: i32 = 70;
const PILL_PAD: i32 = 36;
const SMALL: u32 = 30;

/// Handheld screens have square pixels; for the others a raw core frame is shown at 4:3.
const HANDHELDS: &[&str] = &[
    "gb",
    "gbc",
    "gba",
    "gamegear",
    "ngp",
    "ngpc",
    "wonderswan",
    "wonderswancolor",
    "pokemini",
    "virtualboy",
    "nds",
    "psp",
    "ports",
];

pub struct Picture {
    pub img: Rc<Image>,
    /// Width / height as the picture should appear.
    pub aspect: f32,
    /// A screenshot of the game fills its card; a cover sits on a dark ground.
    pub shot: bool,
}

/// Where an entry's picture is: not asked for yet, being read by the worker, or there (None: no file to
/// show, the system card stays).
enum Load {
    Idle,
    Loading(Option<FileKey>),
    Ready(Option<Picture>),
}

pub struct Entry {
    pub system: String,
    pub name: String,
    pub path: PathBuf,
    /// System name as people say it ("Game Boy Color").
    pub label: String,
    /// Line under the title: system and when it was played.
    pub meta: String,
    /// The system's stripe colour.
    pub color: u32,
    /// The game that is running right now (switcher opened in a game).
    pub current: bool,
    pub favorite: bool,
    /// Resume and slot previews, best first (their size tells `now_playing` the core's frame size).
    shots: Vec<PathBuf>,
    cands: Candidates,
    load: Load,
}

impl Entry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(system: &str, name: &str, path: &Path, label: String, meta: String, color: u32, favorite: bool, cover: Option<PathBuf>) -> Self {
        let (auto, slots) = previews(&slots::base(system, path.file_stem().and_then(|s| s.to_str()).unwrap_or_default()));
        Entry {
            system: system.to_string(),
            name: name.to_string(),
            path: path.to_path_buf(),
            label,
            meta,
            color,
            current: false,
            favorite,
            shots: auto.iter().cloned().chain(slots.iter().map(|(p, _)| p.clone())).collect(),
            cands: Candidates {
                auto,
                slots,
                frame: None,
                cover: cover.filter(|p| p.exists()),
            },
            load: Load::Idle,
        }
    }

    /// The frame pocket-ui kept when the game was last quit (`lastframe::frame_path`), if it is there.
    pub fn with_frame(mut self, file: &Path) -> Self {
        self.cands.frame = std::fs::metadata(file).and_then(|m| m.modified()).ok().map(|t| (file.to_path_buf(), t));
        self
    }

    /// The running game, with the screen as it was when the switcher opened. For a pocket-core game,
    /// `scaling` (its Picture setting) finds the game's picture inside the screen, leaving out the frame
    /// around it; the size of the core's frame comes from one of its save-state pictures.
    pub fn now_playing(mut self, screen: Option<&Image>, scaling: Option<Scaling>) -> Self {
        self.current = true;
        // Only the size of the core's frame is wanted: the PNG's header, not the picture (a decode on the UI thread).
        let frame = scaling.and_then(|s| self.shots.iter().find_map(|p| image::png_size(p)).map(|(w, h)| (s, w, h)));
        let cropped = screen.map(|img| match frame {
            Some((s, fw, fh)) => {
                let r = crate::video::dest_rect(s, img.w, img.h, fw, fh, display_aspect(&self.system, fw, fh));
                crop(img, r.x, r.y, r.w, r.h)
            }
            None => crop_black(img),
        });
        if let Some(img) = cropped {
            let aspect = img.w as f32 / img.h.max(1) as f32;
            self.load = Load::Ready(Some(Picture {
                img: Rc::new(img),
                aspect,
                shot: true,
            }));
        }
        self
    }

    /// The picture, once it has been read (never reads anything itself).
    fn picture(&self) -> Option<&Picture> {
        match &self.load {
            Load::Ready(p) => p.as_ref(),
            _ => None,
        }
    }

    fn loading(&self) -> bool {
        matches!(self.load, Load::Loading(_))
    }

    /// First ask for the picture: answered at once from the cache or when there is no file to read at all,
    /// otherwise the job for the worker.
    fn request(&mut self) -> Option<Job> {
        if !matches!(self.load, Load::Idle) {
            return None;
        }
        let sources = lastframe::rank(&self.cands);
        // The cache entry is named after the best file there is; if that one doesn't open, the next is shown.
        let key = sources.iter().filter_map(source_file).find_map(FileKey::of);
        let Some(key) = key else {
            // Nothing but the system's own picture to show.
            self.load = Load::Ready(None);
            return None;
        };
        if let Some(hit) = CACHE.with(|c| c.borrow_mut().get(&key).cloned()) {
            self.load = Load::Ready(Some(hit.picture()));
            return None;
        }
        self.load = Load::Loading(Some(key));
        Some(Job {
            id: self.path.clone(),
            system: self.system.clone(),
            sources,
        })
    }
}

/// The file behind a source, if it has one.
fn source_file(s: &Source) -> Option<&Path> {
    match s {
        Source::AutoPreview(p) | Source::Frame(p) | Source::SlotPreview(p) | Source::BoxArt(p) => Some(p),
        Source::SystemArt => None,
    }
}

/// A picture as the worker hands it over: scaled to card size, with how it should appear.
struct Loaded {
    img: Image,
    aspect: f32,
    shot: bool,
}

/// What stays in the cache: the scaled picture, shared by every switcher that shows it.
#[derive(Clone)]
struct Cached {
    img: Rc<Image>,
    aspect: f32,
    shot: bool,
}

impl Cached {
    fn picture(&self) -> Picture {
        Picture {
            img: self.img.clone(),
            aspect: self.aspect,
            shot: self.shot,
        }
    }
}

/// Scaled pictures, by the file they came from and when it was written. About a dozen cards' worth.
const CACHE_ENTRIES: usize = 16;
const CACHE_BYTES: usize = 24 << 20;

thread_local! {
    static CACHE: RefCell<Lru<FileKey, Cached>> = RefCell::new(Lru::new(CACHE_ENTRIES, CACHE_BYTES));
    /// The glows behind the cards made last (`Switcher::paint_backdrop`), with the picture each was made from.
    static BACKDROPS: RefCell<Vec<Backdrop>> = const { RefCell::new(Vec::new()) };
}

/// Glows kept (5 MB each at the screen's size): two, so a crossfade between dark and light, which paints both, makes
/// each once, and the switcher opened again on the same game makes none.
const BACKDROPS_KEPT: usize = 2;

/// A glow kept: what it was made from, the picture (held, so its address in the key stays its own), the pixels.
type Backdrop = (BackdropKey, Option<Rc<Image>>, Rc<Vec<u32>>);

/// What a glow is made from.
#[derive(Clone, PartialEq)]
struct BackdropKey {
    game: PathBuf,
    /// The picture's address (0: none yet, the system's colour is used).
    picture: usize,
    color: u32,
    w: i32,
    h: i32,
    page: u32,
}

/// A picture to read: the game's sources, best first; the first that opens wins.
#[derive(Clone)]
struct Job {
    /// The game's file, which names the entry the answer is for.
    id: PathBuf,
    system: String,
    sources: Vec<Source>,
}

struct Done {
    id: PathBuf,
    pic: Option<Loaded>,
}

/// Reads the first of `sources` that opens, scaled down to card size. None: the system card.
fn load_sources(system: &str, sources: &[Source]) -> Option<Loaded> {
    sources.iter().find_map(|s| load_source(system, s))
}

fn load_source(system: &str, source: &Source) -> Option<Loaded> {
    // Never larger than the card needs; small pictures (160 x 144) stay small and are enlarged on screen.
    let card = |img: Image| {
        let (w, h) = img.fit(BOX_W, BOX_H);
        if img.w as i32 > w {
            img.resized(w, h)
        } else {
            img
        }
    };
    match source {
        Source::AutoPreview(p) | Source::SlotPreview(p) => {
            // Raw core frames: handhelds keep their shape, TV systems are shown at 4:3.
            let img = image::load_png(p)?;
            let aspect = display_aspect(system, img.w, img.h);
            Some(Loaded { img: card(img), aspect, shot: true })
        }
        Source::Frame(p) => {
            // Kept from the screen: already the shape it is shown in.
            let img = image::load_png(p)?;
            let aspect = img.w as f32 / img.h.max(1) as f32;
            Some(Loaded { img: card(img), aspect, shot: true })
        }
        Source::BoxArt(p) => {
            let img = image::load_image(p)?;
            let aspect = img.w as f32 / img.h.max(1) as f32;
            Some(Loaded { img: card(img), aspect, shot: false })
        }
        Source::SystemArt => None,
    }
}

/// The worker's queue: what is waiting, what it is busy with, and whether the switcher is gone.
#[derive(Default)]
struct Queue {
    jobs: Vec<Job>,
    current: Option<PathBuf>,
    closed: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
}

/// Puts `new` jobs into the queue and arranges it to `order` (most wanted first): jobs for games that are
/// not in `order` any more are removed (their ids are returned), a job already waiting or in progress
/// (`current`) is not added twice.
fn arrange(queue: &mut Vec<Job>, new: Vec<Job>, current: Option<&PathBuf>, order: &[PathBuf]) -> Vec<PathBuf> {
    for job in new {
        if current != Some(&job.id) && !queue.iter().any(|j| j.id == job.id) {
            queue.push(job);
        }
    }
    let (keep, drop): (Vec<Job>, Vec<Job>) = std::mem::take(queue).into_iter().partition(|j| order.contains(&j.id));
    *queue = keep;
    queue.sort_by_key(|j| order.iter().position(|o| *o == j.id));
    drop.into_iter().map(|j| j.id).collect()
}

/// Reads pictures on a thread of its own (started with the first job), one at a time, in the order the
/// switcher last asked for.
struct Loader {
    shared: Arc<Shared>,
    results: mpsc::Receiver<Done>,
    sender: Option<mpsc::Sender<Done>>,
    started: bool,
}

impl Loader {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Loader {
            shared: Arc::new(Shared {
                queue: Mutex::new(Queue::default()),
                wake: Condvar::new(),
            }),
            results: rx,
            sender: Some(tx),
            started: false,
        }
    }

    /// Adds `new` jobs and arranges the queue to `order` (most wanted first). Jobs that are not in `order`
    /// any more (scrolled past) are dropped; their ids are returned. The job in progress is left alone.
    fn sync(&mut self, new: Vec<Job>, order: &[PathBuf]) -> Vec<PathBuf> {
        let mut dropped = Vec::new();
        let waiting;
        {
            let Ok(mut q) = self.shared.queue.lock() else { return dropped };
            let current = q.current.clone();
            dropped = arrange(&mut q.jobs, new, current.as_ref(), order);
            waiting = !q.jobs.is_empty();
        }
        if waiting {
            if !self.started {
                self.start();
            }
            self.shared.wake.notify_one();
        }
        dropped
    }

    fn start(&mut self) {
        let Some(tx) = self.sender.take() else { return };
        self.started = true;
        let shared = self.shared.clone();
        let spawned = std::thread::Builder::new().name("switcher-pictures".into()).spawn(move || loop {
            let job = {
                let Ok(mut q) = shared.queue.lock() else { return };
                loop {
                    if q.closed {
                        return;
                    }
                    if !q.jobs.is_empty() {
                        break;
                    }
                    q = match shared.wake.wait(q) {
                        Ok(q) => q,
                        Err(_) => return,
                    };
                }
                let job = q.jobs.remove(0);
                q.current = Some(job.id.clone());
                job
            };
            let pic = load_sources(&job.system, &job.sources);
            if let Ok(mut q) = shared.queue.lock() {
                q.current = None;
            }
            if tx.send(Done { id: job.id, pic }).is_err() {
                return;
            }
        });
        if spawned.is_err() {
            self.started = false;
            self.sender = None;
        }
    }

    /// What the worker has finished since the last call, and whether it can still answer (jobs waiting for a
    /// worker that is gone would wait for ever).
    fn poll(&mut self) -> (Vec<Done>, bool) {
        let mut done = Vec::new();
        loop {
            match self.results.try_recv() {
                Ok(d) => done.push(d),
                Err(mpsc::TryRecvError::Empty) => return (done, true),
                Err(mpsc::TryRecvError::Disconnected) => return (done, false),
            }
        }
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        if let Ok(mut q) = self.shared.queue.lock() {
            q.closed = true;
            q.jobs.clear();
        }
        self.shared.wake.notify_all();
    }
}

/// Save-state pictures of a game, best first: the auto-resume ones, then slot previews, newest first.
pub fn screenshots(system: &str, rom: &Path) -> Vec<PathBuf> {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    screenshots_at(&slots::base(system, stem))
}

/// When the emulator last wrote the preview of the auto-resume state of this game, if it has one.
pub fn resume_preview_written(system: &str, rom: &Path) -> Option<SystemTime> {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let (auto, _) = previews(&slots::base(system, stem));
    auto.iter().filter_map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok()).max()
}

/// `screenshots` for the files starting with `base` (`<states>/<system>/<stem>`).
fn screenshots_at(base: &Path) -> Vec<PathBuf> {
    let (auto, slot_pics) = previews(base);
    auto.into_iter().chain(slot_pics.into_iter().map(|(p, _)| p)).collect()
}

/// The previews that exist for the files starting with `base`: the auto-resume ones, and the slot ones with
/// their time, newest first.
fn previews(base: &Path) -> (Vec<PathBuf>, Vec<(PathBuf, SystemTime)>) {
    let with = |suffix: &str| {
        let mut p = base.as_os_str().to_owned();
        p.push(suffix);
        PathBuf::from(p)
    };
    let auto: Vec<PathBuf> = [slots::preview_path(&slots::auto_path(base)), with(".state.auto.png")]
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    let mut slot_pics: Vec<(PathBuf, SystemTime)> = (0..slots::SLOTS)
        .map(|i| slots::preview_path(&slots::state_path(base, i)))
        .chain(std::iter::once(with(".state.png")))
        .chain((1..10).map(|i| with(&format!(".state{i}.png"))))
        .filter_map(|p| {
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((p, t))
        })
        .collect();
    slot_pics.sort_by(|a, b| b.1.cmp(&a.1));
    (auto, slot_pics)
}

/// How wide a raw core frame of `w` x `h` appears on screen.
pub(crate) fn display_aspect(system: &str, w: u32, h: u32) -> f32 {
    let raw = w as f32 / h.max(1) as f32;
    if HANDHELDS.contains(&system) || !(1.0..=2.0).contains(&raw) {
        raw
    } else {
        4.0 / 3.0
    }
}

/// The game's picture without the black around it (pocket-core centres it on a 1240 x 1080 screen).
pub fn crop_black(img: &Image) -> Image {
    let (w, h) = (img.w as usize, img.h as usize);
    let lit = |p: u32| (p >> 16 & 0xff).max(p >> 8 & 0xff).max(p & 0xff) > 10;
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        let row = &img.px[y * w..(y + 1) * w];
        if let (Some(a), Some(b)) = (row.iter().position(|p| lit(*p)), row.iter().rposition(|p| lit(*p))) {
            x0 = x0.min(a);
            x1 = x1.max(b + 1);
            y0 = y0.min(y);
            y1 = y + 1;
        }
    }
    // Nearly all black (a fade, a dark scene): keep the whole screen.
    if x1 <= x0 || y1 <= y0 || (x1 - x0) * (y1 - y0) < w * h / 20 {
        return Image {
            w: img.w,
            h: img.h,
            px: img.px.clone(),
        };
    }
    crop(img, x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32)
}

/// The part of `img` at (x, y, w, h), clamped to the image.
fn crop(img: &Image, x: u32, y: u32, w: u32, h: u32) -> Image {
    let (x0, y0) = (x.min(img.w), y.min(img.h));
    let (x1, y1) = ((x + w).min(img.w), (y + h).min(img.h));
    let stride = img.w as usize;
    let px = (y0..y1)
        .flat_map(|y| img.px[y as usize * stride + x0 as usize..y as usize * stride + x1 as usize].iter().copied())
        .collect();
    Image { w: x1 - x0, h: y1 - y0, px }
}

/// What a button press asks of the app.
#[derive(Debug, PartialEq)]
pub enum Action {
    None,
    Close,
    /// Resume this entry (switch to it in a game).
    Play(usize),
    Remove(usize),
    Favorite(usize),
}

pub struct Switcher {
    pub entries: Vec<Entry>,
    pub sel: usize,
    /// Where the row is, in cards: glides to `sel` and is retargeted from wherever it is (`motion::Glide`).
    pos: Glide,
    /// Opened in a game: the running game is the first entry.
    pub in_game: bool,
    /// Opened from the pause menu (SELECT there): B goes back to it instead of the game.
    pub from_menu: bool,
    /// A game was chosen in a game: the running one is saving and quitting.
    pub switching: bool,
    /// Where each card was painted last, (x, y, w, h, entry), for taps.
    pub card_areas: Vec<(i32, i32, i32, i32, usize)>,
    /// There are recent games, but only of systems taken off the shelf (Settings > Look > Systems on the shelf).
    pub only_hidden: bool,
    loader: Loader,
}

impl Switcher {
    pub fn new(entries: Vec<Entry>, in_game: bool) -> Self {
        Switcher {
            entries,
            sel: 0,
            pos: Glide::rows().with_tau(SLIDE_TAU),
            in_game,
            from_menu: false,
            switching: false,
            card_areas: Vec::new(),
            only_hidden: false,
            loader: Loader::new(),
        }
    }

    /// True while the row slides or a picture is on its way (the app repaints until it is in).
    pub fn animating(&self) -> bool {
        self.pos.moving(Instant::now()) || self.entries.iter().any(Entry::loading)
    }

    /// Takes in the pictures the worker has finished and asks for the ones the cards need: the cards on
    /// screen first, the selected one before its neighbours, then the next ones either way so a scroll finds
    /// them in. Pictures that are not wanted any more are not read.
    fn load_pictures(&mut self, pos: f32) {
        let (done, alive) = self.loader.poll();
        for d in done {
            let Some(e) = self.entries.iter_mut().find(|e| e.path == d.id) else { continue };
            let key = match std::mem::replace(&mut e.load, Load::Idle) {
                Load::Loading(k) => k,
                Load::Idle => None,
                other => {
                    // Answered already (the entry was asked again after being dropped); keep what is there.
                    e.load = other;
                    continue;
                }
            };
            e.load = Load::Ready(d.pic.map(|l| {
                let cached = Cached {
                    img: Rc::new(l.img),
                    aspect: l.aspect,
                    shot: l.shot,
                };
                if let Some(k) = key {
                    let bytes = cached.img.px.len() * 4;
                    CACHE.with(|c| c.borrow_mut().put(k, cached.clone(), bytes));
                }
                cached.picture()
            }));
        }
        if !alive {
            for e in self.entries.iter_mut().filter(|e| e.loading()) {
                e.load = Load::Ready(None);
            }
            return;
        }
        let mut wanted: Vec<(usize, f32)> = (0..self.entries.len()).map(|i| (i, (i as f32 - pos).abs())).filter(|(_, d)| *d < PREFETCH).collect();
        wanted.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (mut jobs, mut order) = (Vec::new(), Vec::new());
        for (i, _) in wanted {
            let e = &mut self.entries[i];
            let job = e.request();
            if e.loading() {
                order.push(e.path.clone());
            }
            jobs.extend(job);
        }
        let dropped = self.loader.sync(jobs, &order);
        for id in dropped {
            if let Some(e) = self.entries.iter_mut().find(|e| e.path == id && e.loading()) {
                e.load = Load::Idle;
            }
        }
    }

    fn select(&mut self, to: usize) {
        if to != self.sel {
            // Mid-slide, the new slide starts from where the row is now.
            self.pos.set(self.sel as f32, Instant::now());
            self.sel = to;
            self.pos.set(to as f32, Instant::now());
        }
    }

    pub fn input(&mut self, b: Button) -> Action {
        if self.switching {
            return Action::None;
        }
        let last = self.entries.len().saturating_sub(1);
        match b {
            Button::Left | Button::Up => self.select(self.sel.saturating_sub(1)),
            Button::Right | Button::Down => self.select((self.sel + 1).min(last)),
            Button::L1 => self.select(0),
            Button::R1 => self.select(last),
            // A is on the right face button, B at the bottom, X on top, Y on the left (Nintendo layout).
            Button::East if !self.entries.is_empty() => return Action::Play(self.sel),
            Button::South | Button::Select | Button::Mode => return Action::Close,
            Button::North if self.entries.get(self.sel).is_some_and(|e| !e.current) => return Action::Remove(self.sel),
            Button::West if !self.entries.is_empty() => return Action::Favorite(self.sel),
            _ => {}
        }
        Action::None
    }

    /// Takes entry `i` off the row.
    pub fn remove(&mut self, i: usize) {
        if i < self.entries.len() {
            self.entries.remove(i);
        }
        self.sel = self.sel.min(self.entries.len().saturating_sub(1));
        self.pos.jump(self.sel as f32);
    }

    /// Button hints for the selected game, for the pill at the bottom right.
    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let Some(e) = self.entries.get(self.sel) else { return vec![("B", "Back")] };
        let play = match (self.in_game, e.current) {
            (true, true) => "Continue",
            (true, false) => "Switch",
            _ => "Resume",
        };
        let mut h = vec![("B", "Back")];
        if !e.current {
            h.push(("X", "Remove"));
        }
        h.push(("Y", if e.favorite { "Unstar" } else { "Star" }));
        h.push(("A", play));
        h
    }

    /// The row's position, between the last selection and the new one while it slides.
    fn pos(&self) -> f32 {
        if !self.pos.is_set() {
            return self.sel as f32;
        }
        self.pos.value(Instant::now())
    }

    /// Paints everything but the status and hint pills, which the app draws in its own style.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, marquee: &mut Marquee) {
        self.card_areas.clear();
        self.load_pictures(self.pos());
        self.paint_backdrop(c);
        // Heading pill at the top left, with where in the row we are.
        let heading = if self.in_game { "Switch game" } else { "Recently played" };
        let count = if self.entries.len() > 1 {
            format!("{} / {}", self.sel + 1, self.entries.len())
        } else {
            String::new()
        };
        let hw = text.width(Face::Body, SMALL, heading);
        let cw = if count.is_empty() { 0 } else { text.width(Face::Body, SMALL, &count) + 24 };
        c.round_rect(PAD, PAD, hw + cw + 2 * PILL_PAD, PILL, PILL / 2, pal().pill);
        let base = PAD + PILL / 2 + crate::type_scale::mid(SMALL);
        text.draw(c, Face::Body, SMALL, PAD + PILL_PAD, base, pal().soft, heading);
        if !count.is_empty() {
            text.draw(c, Face::Body, SMALL, PAD + PILL_PAD + hw + 24, base, pal().pick(0x8a8780, 0x6a665e), &count);
        }

        if self.entries.is_empty() {
            let (t, s) = if self.only_hidden {
                ("No recent games on the shelf", "Games from systems taken off the shelf stay out of this list (Settings › Look & feel › Look).")
            } else {
                ("Nothing played yet", "The games you play show up here, ready to pick up where you left off.")
            };
            // A handheld asleep under a blanket above the words (`ui_art`), the words moved down if it needs room.
            let dy = crate::ui_art::above_words(c, crate::ui_art::Art::NothingPlayed, c.w / 2, true, PAD + PILL + 24, c.h / 2 - 10, 50);
            let tw = text.width(Face::Title, 50, t);
            text.draw(c, Face::Title, 50, (c.w - tw) / 2, c.h / 2 - 10 + dy, pal().chalk, t);
            let sw = text.width(Face::Body, 28, s);
            text.draw(c, Face::Body, 28, (c.w - sw) / 2, c.h / 2 + 44 + dy, pal().dust, s);
            return;
        }

        // Cards, farthest first so the selected one lies on top.
        let pos = self.pos();
        let mut order: Vec<(usize, f32)> = (0..self.entries.len()).map(|i| (i, i as f32 - pos)).filter(|(_, d)| d.abs() < 1.9).collect();
        order.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
        for (i, d) in order {
            self.paint_card(c, text, i, d);
        }

        // Title (scrolling when it is too long) and what it is.
        let e = &self.entries[self.sel];
        let (name, meta) = (e.name.clone(), e.meta.clone());
        let y = TOP + card_height(c.h) + 108;
        marquee.draw_centered(text, c, Face::Title, 67, MARGIN, y, c.w - 2 * MARGIN, pal().pick(0xf4f2ec, 0x2b2925), &name);
        let meta = text.fit(Face::Body, 28, &meta, c.w - 2 * MARGIN);
        let mw = text.width(Face::Body, 28, &meta);
        text.draw(c, Face::Body, 28, (c.w - mw) / 2, y + 50, pal().pick(0x9a978f, 0x57534c), &meta);

        // One dot per game; the selected one is lit and wider.
        let n = self.entries.len() as i32;
        if n > 1 {
            let (dot, gap, wide) = (10, 12, 30);
            let total = n * dot + (n - 1) * gap + (wide - dot);
            let mut x = (c.w - total) / 2;
            let dy = y + 94;
            for i in 0..n {
                let selected = i as usize == self.sel;
                let w = if selected { wide } else { dot };
                c.round_rect(x, dy, w, dot, dot / 2, if selected { pal().pick(0xf4f2ec, 0x2b2925) } else { pal().pick(0x4a4a4e, 0xc9c3b7) });
                x += w + gap;
            }
        }

        if self.switching {
            let current = self.entries.iter().find(|e| e.current).map_or(String::new(), |e| e.name.clone());
            let msg = text.fit(Face::Body, SMALL, &format!("Saving {current}…"), c.w / 2);
            text.draw_right(c, Face::Body, SMALL, c.w - PAD - PILL_PAD, c.h - PAD - 34, pal().soft, &msg);
        }
    }

    /// The selected game's picture as a soft glow behind everything, darker towards the bottom. The last glows
    /// made are kept (`BACKDROPS`), also for the next switcher: opened again on the same game, it costs a copy.
    fn paint_backdrop(&mut self, c: &mut Canvas) {
        let (w, h) = (c.w, c.h);
        let page = pal().void;
        let e = self.entries.get(self.sel);
        let pic = e.and_then(|e| e.picture()).map(|p| p.img.clone());
        let key = BackdropKey {
            game: e.map(|e| e.path.clone()).unwrap_or_default(),
            picture: pic.as_ref().map_or(0, |p| Rc::as_ptr(p) as usize),
            color: e.map_or(0, |e| e.color),
            w,
            h,
            page,
        };
        let px = BACKDROPS.with(|b| b.borrow().iter().find(|(k, _, _)| *k == key).map(|(_, _, px)| px.clone()));
        let px = px.unwrap_or_else(|| {
            let px = Rc::new(match (e, &pic) {
                (Some(_), Some(p)) => glow(&glow_source(p), w, h, page),
                (Some(e), None) => glow(
                    &Image {
                        w: 1,
                        h: 2,
                        px: vec![mix(e.color, 0, 40), mix(e.color, 0, 160)],
                    },
                    w,
                    h,
                    page,
                ),
                (None, _) => vec![page; (w * h) as usize],
            });
            BACKDROPS.with(|b| {
                let mut b = b.borrow_mut();
                if b.len() >= BACKDROPS_KEPT {
                    b.remove(0);
                }
                // The picture is held with its glow, so its address (in the key) can't be another picture's.
                b.push((key, pic, px.clone()));
            });
            px
        });
        if px.len() == c.buf.len() {
            c.buf.copy_from_slice(&px);
        }
    }

    fn paint_card(&mut self, c: &mut Canvas, text: &mut Text, i: usize, d: f32) {
        let text_mark = crate::text::boxes::mark();
        let near = d.abs().min(1.0);
        let s = 1.0 - SHRINK * near;
        let e = &mut self.entries[i];
        let (color, current, favorite) = (e.color, e.current, e.favorite);
        let (name, label, system) = (e.name.clone(), e.label.clone(), e.system.clone());
        let pic = e.picture().map(|p| (p.img.clone(), p.aspect, p.shot));
        let aspect = pic.as_ref().map_or(4.0 / 3.0, |p| p.1).clamp(0.5, 2.5);
        let box_h = card_height(c.h);
        let box_w = BOX_W * box_h / BOX_H;
        let (bw, bh) = (box_w as f32 * s, box_h as f32 * s);
        let (w, h) = if aspect > bw / bh { (bw, bw / aspect) } else { (bh * aspect, bh) };
        let (w, h) = (w as i32, h as i32);
        let cx = c.w / 2 + (d * STEP) as i32;
        let cy = TOP + box_h / 2;
        let (x, y) = (cx - w / 2, cy - h / 2);
        let hit = (x - 18, y - 18, w + 36, h + 36, i);
        self.card_areas.push(hit);

        // A bezel like the handheld's own screen: a thin light edge, a dark frame, the picture.
        let pad = (16.0 * s) as i32;
        let r = (26.0 * s) as i32;
        c.round_rect(
            x - pad - 2,
            y - pad - 2,
            w + 2 * pad + 4,
            h + 2 * pad + 4,
            r + 2,
            if near < 0.5 { 0x45454b } else { 0x2c2c30 },
        );
        c.round_rect(x - pad, y - pad, w + 2 * pad, h + 2 * pad, r, 0x111113);
        match &pic {
            Some((img, _, true)) => c.blit_scaled(img, x, y, w, h),
            Some((img, _, false)) => {
                c.rect(x, y, w, h, 0x0b0b0c);
                let (iw, ih) = img.fit(w - 24, h - 24);
                c.blit_scaled(img, x + (w - iw) / 2, y + (h - ih) / 2, iw, ih);
            }
            None => title_card(c, text, x, y, w, h, color, &system, &name, &label, s > 0.83, if current && near < 0.5 { BADGE_ROOM } else { 0 }),
        }
        // Neighbour cards deliberately peek through the screen edge; their internal title still fits
        // the card. Keep testing the selected card and every text overlap normally.
        if i != self.sel && (x < 0 || x + w > c.w) {
            crate::text::boxes::cut_on_purpose(text_mark);
        }
        if near > 0.01 {
            c.shade_rect(x - pad - 2, y - pad - 2, w + 2 * pad + 4, h + 2 * pad + 4, pal().void, (near * 150.0) as u8);
        }
        if near < 0.5 {
            if current {
                // "Now playing" in a white pill on the picture's top left corner.
                let t = "Now playing";
                let tw = text.width(Face::Body, 24, t);
                c.round_rect(x + 18, y + 18, tw + 40, 44, 22, 0xf4f2ec);
                text.draw(c, Face::Body, 24, x + 38, y + 18 + 22 + 8, INK, t);
            }
            if favorite {
                c.disc(x + w - 40, y + 40, 24, 0x151515);
                star(c, x + w - 40, y + 41, 15, 0xf2c14e);
            }
        }
    }
}

/// How far a title card's system name moves down when the "Now playing" pill sits in the same corner (the pill ends 62 px
/// below the card's top; the smaller card's name starts 30 px below it). Found by the monkey test: they were painted on
/// each other for a running game without a cover.
const BADGE_ROOM: i32 = 34;

/// A card for a game with no picture yet: its system's colour, the system's name, the title set large. `badge_room`:
/// pixels the system name moves down for the "Now playing" pill.
#[allow(clippy::too_many_arguments)]
fn title_card(c: &mut Canvas, text: &mut Text, x: i32, y: i32, w: i32, h: i32, color: u32, system: &str, name: &str, label: &str, big: bool, badge_room: i32) {
    // Lighter at the top, like light falling on a label.
    for row in 0..h {
        c.rect(x, y + row, w, 1, mix(mix(color, 0, 70), mix(color, 0, 170), (row * 255 / h.max(1)) as u32));
    }
    c.rect(x, y, STRIPE + 4, h, color);
    let (tp, lp, inset) = if big { (67u32, 28u32, 44) } else { (46, 21, 30) };
    let label = text.fit(Face::Body, lp, label, w - 2 * inset);
    text.draw(c, Face::Body, lp, x + inset, y + inset + badge_room + lp as i32, mix(0xffffff, color, 70), &label);
    let mut lines = text.wrap(Face::Title, tp, name, w - 2 * inset);
    if lines.len() > 3 {
        lines.truncate(3);
        let last = lines[2].clone() + "…";
        lines[2] = text.fit(Face::Title, tp, &last, w - 2 * inset);
    }
    let line_h = crate::type_scale::line(tp, 4);
    // The console between the system's name and the title.
    if let Some(art) = crate::system_art::get(system) {
        let (top, bottom) = (y + inset + badge_room + lp as i32 + 24, y + h - inset - tp as i32 - (lines.len() as i32 - 1) * line_h - 20);
        let (room_w, room_h) = ((w - 2 * inset) * 7 / 10, bottom - top);
        if room_h > 40 {
            let sprite = art.fitted(room_w, room_h);
            c.blit_sprite(&sprite, x + (w + STRIPE - sprite.w as i32) / 2, top + (room_h - sprite.h as i32) / 2, 0);
        }
    }
    let mut by = y + h - inset - (lines.len() as i32 - 1) * line_h;
    for l in &lines {
        text.draw(c, Face::Title, tp, x + inset, by, 0xf4f2ec, l);
        by += line_h;
    }
}

/// A filled five-pointed star centred on (cx, cy) with outer radius `r`, edges smoothed.
pub fn star(c: &mut Canvas, cx: i32, cy: i32, r: i32, color: u32) {
    let pts: Vec<(f32, f32)> = (0..10)
        .map(|k| {
            let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
            let rr = if k % 2 == 0 { r as f32 } else { r as f32 * 0.42 };
            (cx as f32 + rr * a.cos(), cy as f32 + rr * a.sin())
        })
        .collect();
    let inside = |px: f32, py: f32| {
        let mut odd = false;
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a.1 > py) != (b.1 > py) && px < a.0 + (py - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                odd = !odd;
            }
        }
        odd
    };
    for y in cy - r - 1..=cy + r + 1 {
        for x in cx - r - 1..=cx + r + 1 {
            // 4 x 4 samples per pixel.
            let n = (0..16)
                .filter(|k| inside(x as f32 + (k % 4) as f32 / 4.0 + 0.125, y as f32 + (k / 4) as f32 / 4.0 + 0.125))
                .count();
            if n > 0 {
                c.blend(x, y, color, (n * 255 / 16) as u8);
            }
        }
    }
}

/// The 16 x 12 colours a glow is made of: each the average of up to 8 x 8 pixels spread evenly over its part of
/// `img` (`Image::resized` would read every pixel: a whole screen's picture of the running game cost several
/// milliseconds of the in-game switcher's first frame; for a blur this soft the samples are as good).
fn glow_source(img: &Image) -> Image {
    const W: u32 = 16;
    const H: u32 = 12;
    const SAMPLES: u32 = 8;
    if img.w <= W * SAMPLES && img.h <= H * SAMPLES {
        return img.resized(W as i32, H as i32);
    }
    let mut px = Vec::with_capacity((W * H) as usize);
    for ty in 0..H {
        for tx in 0..W {
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for sy in 0..SAMPLES {
                let y = ((ty * SAMPLES + sy) as u64 * 2 + 1) * img.h as u64 / (2 * (H * SAMPLES) as u64);
                for sx in 0..SAMPLES {
                    let x = ((tx * SAMPLES + sx) as u64 * 2 + 1) * img.w as u64 / (2 * (W * SAMPLES) as u64);
                    let p = img.px.get((y * img.w as u64 + x) as usize).copied().unwrap_or(0);
                    r += p >> 16 & 0xff;
                    g += p >> 8 & 0xff;
                    b += p & 0xff;
                }
            }
            let n = SAMPLES * SAMPLES;
            px.push(((r / n) << 16) | ((g / n) << 8) | (b / n));
        }
    }
    Image { w: W, h: H, px }
}

/// `small` stretched to `w` x `h` with bilinear filtering (a cheap, very soft blur), mixed towards the page
/// colour `page` (black when dark, paper when light) from 70 % at the top to 92 % at the bottom so text stays readable.
///
/// Separable and in integers: per row, the few source columns are interpolated between their two rows and mixed
/// towards the page once (mixing is linear, so before or after the horizontal step comes to the same); per pixel
/// only the horizontal step is left, with the weights of each column worked out once. The float version this
/// replaces took 15 ms for the 1240 x 1080 screen on the Mac and about three times that on the device, on the
/// switcher's first frame and again when the selected picture came in.
fn glow(small: &Image, w: i32, h: i32, page: u32) -> Vec<u32> {
    let (sw, sh) = (small.w.max(1) as usize, small.h.max(1) as usize);
    let (w, h) = (w.max(0) as usize, h.max(0) as usize);
    let at = |x: usize, y: usize| small.px.get(y.min(sh - 1) * sw + x.min(sw - 1)).copied().unwrap_or(0);
    // Source position of a target pixel's centre in 1/256ths: (i + 0.5) * src / dst - 0.5, not below 0.
    let pos = |i: usize, src: usize, dst: usize| ((((2 * i + 1) * src * 256) / (2 * dst.max(1))).saturating_sub(128)) as u32;
    let cols: Vec<(usize, u32)> = (0..w).map(|x| pos(x, sw, w)).map(|p| ((p >> 8) as usize, p & 0xff)).collect();
    // Two channels at once: red and blue in one word, green in another, each with room for a 0..256 weight.
    let split = |c: u32| (c & 0x00ff_00ff, c & 0x0000_ff00);
    let lerp = |a: u32, b: u32, t: u32| {
        let ((arb, ag), (brb, bg)) = (split(a), split(b));
        let rb = ((arb * (256 - t) + brb * t) >> 8) & 0x00ff_00ff;
        let g = ((ag * (256 - t) + bg * t) >> 8) & 0x0000_ff00;
        rb | g
    };
    let mut out = Vec::with_capacity(w * h);
    let mut row = vec![0u32; sw + 1];
    for y in 0..h {
        let p = pos(y, sh, h);
        let (y0, ty) = ((p >> 8) as usize, p & 0xff);
        let dark = 180 + 55 * y as u32 / h.max(1) as u32;
        for (x, c) in row.iter_mut().enumerate() {
            *c = mix(lerp(at(x, y0), at(x, y0 + 1), ty), page, dark);
        }
        let row = &row;
        out.extend(cols.iter().map(|&(x0, tx)| {
            let x0 = x0.min(sw - 1);
            lerp(row[x0], row[x0 + 1], tx)
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn entry(name: &str, current: bool) -> Entry {
        let mut e = Entry::new(
            "gbc",
            name,
            Path::new(&format!("/nowhere/{name}.gbc")),
            "Game Boy Color".into(),
            String::new(),
            0x2e8c9a,
            false,
            None,
        );
        e.current = current;
        e
    }

    #[test]
    fn moving_stops_at_the_ends_and_buttons_map_to_actions() {
        let mut s = Switcher::new(vec![entry("a", true), entry("b", false), entry("c", false)], true);
        assert_eq!(s.input(Button::Left), Action::None);
        assert_eq!(s.sel, 0);
        assert_eq!(s.input(Button::North), Action::None, "the running game can't be removed");
        assert_eq!(s.input(Button::East), Action::Play(0));
        s.input(Button::Right);
        s.input(Button::Right);
        s.input(Button::Right);
        assert_eq!(s.sel, 2);
        assert_eq!(s.input(Button::North), Action::Remove(2));
        assert_eq!(s.input(Button::West), Action::Favorite(2));
        assert_eq!(s.input(Button::South), Action::Close);
        assert_eq!(s.input(Button::Select), Action::Close);
        s.remove(2);
        assert_eq!((s.sel, s.entries.len()), (1, 2));
        s.switching = true;
        assert_eq!(s.input(Button::East), Action::None, "no input while switching");
    }

    // ---- pictures ----

    fn scratch(tag: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("switcher-{tag}"))
    }

    fn png(path: &Path, w: u32, h: u32, color: u32) {
        let rgb: Vec<u8> = (0..w * h).flat_map(|_| [(color >> 16) as u8, (color >> 8) as u8, color as u8]).collect();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(path).unwrap()), w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    }

    fn job(name: &str) -> Job {
        Job {
            id: PathBuf::from(name),
            system: "gbc".into(),
            sources: vec![Source::SystemArt],
        }
    }

    fn ids(q: &[Job]) -> Vec<&str> {
        q.iter().map(|j| j.id.to_str().unwrap()).collect()
    }

    #[test]
    fn the_queue_follows_the_selection_and_forgets_cards_that_were_scrolled_past() {
        let order = |names: &[&str]| names.iter().map(PathBuf::from).collect::<Vec<_>>();
        let mut q = vec![job("a"), job("b"), job("c")];
        // The selection moved to d: d first, then its neighbour c; a and b are out of sight.
        let dropped = arrange(&mut q, vec![job("d")], None, &order(&["d", "c"]));
        assert_eq!(ids(&q), ["d", "c"]);
        assert_eq!(dropped, order(&["a", "b"]));
        // Asked again for a job that waits or is in progress: not queued twice.
        let dropped = arrange(&mut q, vec![job("d"), job("e")], Some(&PathBuf::from("e")), &order(&["c", "d", "e"]));
        assert_eq!((ids(&q), dropped.len()), (vec!["c", "d"], 0));
    }

    #[test]
    fn a_picture_that_does_not_open_gives_way_to_the_next_source_and_big_ones_are_scaled_to_card_size() {
        let dir = scratch("load");
        let (bad, big, cover) = (dir.join("bad.png"), dir.join("big.png"), dir.join("cover.png"));
        std::fs::write(&bad, b"not a png").unwrap();
        png(&big, 1520, 1140, 0x336699);
        png(&cover, 300, 400, 0x993366);

        let got = load_sources("snes", &[Source::AutoPreview(bad.clone()), Source::Frame(big.clone()), Source::BoxArt(cover.clone())]).expect("a picture");
        assert_eq!((got.img.w as i32, got.img.h as i32), (BOX_W, BOX_H), "scaled down to card size");
        assert!(got.shot && (got.aspect - 4.0 / 3.0).abs() < 1e-3);

        // A kept frame keeps its own shape (16:9 stays 16:9); a raw core frame of a TV system is shown at 4:3.
        let wide = dir.join("wide.png");
        png(&wide, 320, 180, 0x111111);
        assert!((load_sources("ps2", &[Source::Frame(wide.clone())]).unwrap().aspect - 16.0 / 9.0).abs() < 1e-3);
        assert!((load_sources("snes", &[Source::AutoPreview(wide)]).unwrap().aspect - 16.0 / 9.0).abs() > 0.1);

        let art = load_sources("snes", &[Source::BoxArt(cover.clone()), Source::SystemArt]).unwrap();
        assert!(!art.shot, "box art sits on a dark ground");
        // Nothing opens: the system's picture.
        assert!(load_sources("snes", &[Source::AutoPreview(bad), Source::SystemArt]).is_none());
        assert!(load_sources("snes", &[Source::SystemArt]).is_none());
    }

    #[test]
    fn something_that_is_not_a_file_is_no_picture_and_the_next_source_is_used() {
        let dir = scratch("notfile");
        let rom = Path::new("/nowhere/Game.gbc");
        let frame = dir.join("Game.png");
        png(&frame, 64, 48, 0x00aa55);
        let mut e = Entry::new("gbc", "Game", rom, String::new(), String::new(), 0x2e8c9a, false, None).with_frame(&frame);
        // A folder where the resume preview should be (the emulator couldn't write it).
        e.cands.auto = vec![dir.to_path_buf()];
        let job = e.request().expect("the frame is read");
        assert_eq!(job.sources[0], Source::AutoPreview(dir.to_path_buf()));
        let got = load_sources("gbc", &job.sources).expect("the next source opens");
        assert_eq!((got.img.w, got.img.h), (64, 48));
    }

    #[test]
    fn a_game_with_no_file_to_show_is_answered_at_once_and_one_with_a_frame_goes_to_the_worker_once() {
        let dir = scratch("request");
        let rom = Path::new("/nowhere/Game.gbc");
        let mut plain = entry("Game", false);
        assert!(plain.request().is_none() && plain.picture().is_none() && !plain.loading(), "system card, no waiting");

        let file = dir.join("Game.png");
        png(&file, 64, 48, 0x00aa55);
        let mut e = Entry::new("gbc", "Game", rom, String::new(), String::new(), 0x2e8c9a, false, None).with_frame(&file);
        let j = e.request().expect("a job for the worker");
        assert_eq!(j.sources[0], Source::Frame(file.clone()));
        assert!(e.loading());
        assert!(e.request().is_none(), "asked once");
    }

    #[test]
    fn pictures_arrive_from_the_worker_without_the_caller_waiting_and_a_second_switcher_gets_them_from_the_cache() {
        let dir = scratch("worker");
        let files: Vec<PathBuf> = (0..3).map(|i| dir.join(format!("g{i}.png"))).collect();
        for (i, f) in files.iter().enumerate() {
            png(f, 800, 600, 0x202020 * (i as u32 + 1));
        }
        let make = || {
            let entries = files
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let rom = format!("/nowhere/g{i}.gbc");
                    Entry::new("gbc", &format!("g{i}"), Path::new(&rom), String::new(), String::new(), 0x2e8c9a, false, None).with_frame(f)
                })
                .collect();
            Switcher::new(entries, false)
        };
        let mut sw = make();
        let t0 = Instant::now();
        sw.load_pictures(0.0);
        assert!(t0.elapsed() < Duration::from_millis(50), "asking does not read: {:?}", t0.elapsed());
        assert!(sw.animating(), "repaints until the pictures are in");
        let deadline = Instant::now() + Duration::from_secs(10);
        while sw.animating() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            sw.load_pictures(0.0);
        }
        assert!(!sw.animating());
        for e in &sw.entries {
            let p = e.picture().expect("picture");
            assert_eq!((p.img.w as i32, p.img.h as i32), (BOX_W, BOX_H));
        }
        // Opened again: the cache answers in the first call, no worker involved.
        let mut again = make();
        again.load_pictures(0.0);
        assert!(!again.animating() && again.entries.iter().all(|e| e.picture().is_some()));
        assert!(Rc::ptr_eq(&again.entries[0].picture().unwrap().img, &sw.entries[0].picture().unwrap().img), "the very same scaled image");
        // A frame written later is a new file for the cache.
        png(&files[0], 400, 300, 0xeeeeee);
        let mut changed = make();
        changed.load_pictures(0.0);
        assert!(changed.entries[0].loading(), "not served from the stale cache entry");
    }

    #[test]
    fn empty_switcher_only_closes() {
        let mut s = Switcher::new(Vec::new(), false);
        assert_eq!(s.input(Button::East), Action::None);
        assert_eq!(s.input(Button::West), Action::None);
        assert_eq!(s.input(Button::South), Action::Close);
        assert_eq!(s.hints(), vec![("B", "Back")]);
    }

    #[test]
    fn screenshots_prefer_the_auto_state_then_the_newest_slot() {
        let dir = TestDir::new("switcher");
        // `slots::base` points into /storage; test the ordering with the same names in a scratch folder.
        let base = dir.join("Game");
        std::fs::create_dir_all(&dir).unwrap();
        let file = |suffix: &str| {
            let mut p = base.as_os_str().to_owned();
            p.push(suffix);
            PathBuf::from(p)
        };
        for s in [".pstate.png", ".pstate3.png", ".state.png"] {
            std::fs::write(file(s), b"x").unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
        let got = screenshots_at(&base);
        assert_eq!(got, [file(".state.png"), file(".pstate3.png"), file(".pstate.png")], "newest first");
        std::fs::write(file(".state.auto.png"), b"x").unwrap();
        std::fs::write(file(".pstate.auto.png"), b"x").unwrap();
        assert_eq!(screenshots_at(&base)[..2], [file(".pstate.auto.png"), file(".state.auto.png")]);
    }

    #[test]
    fn black_borders_are_cropped_from_the_live_screen() {
        let mut px = vec![0u32; 100 * 80];
        for y in 10..70 {
            for x in 20..80 {
                px[y * 100 + x] = 0x336699;
            }
        }
        let img = crop_black(&Image { w: 100, h: 80, px });
        assert_eq!((img.w, img.h), (60, 60));
        let dark = crop_black(&Image {
            w: 100,
            h: 80,
            px: vec![0; 8000],
        });
        assert_eq!((dark.w, dark.h), (100, 80), "an all-black screen stays whole");
    }

    #[test]
    fn handheld_frames_keep_their_shape_and_tv_frames_become_4_3() {
        assert!((display_aspect("gbc", 160, 144) - 160.0 / 144.0).abs() < 1e-6);
        assert!((display_aspect("snes", 256, 224) - 4.0 / 3.0).abs() < 1e-6);
        assert!((display_aspect("gba", 240, 160) - 1.5).abs() < 1e-6);
    }

    /// The "Now playing" pill ends 62 px below the card's top (`paint_card`: 18 + 44); the system name of a title card
    /// starts below it at both sizes, instead of on it.
    #[test]
    fn title_card_system_name_starts_below_the_now_playing_pill() {
        for inset in [30, 44] {
            assert!(inset + BADGE_ROOM >= 18 + 44, "inset {inset}");
        }
    }
}
#[cfg(test)]
mod glow_tests {
    use super::*;

    /// The float version `glow` replaced, as the reference.
    fn glow_float(small: &Image, w: i32, h: i32, page: u32) -> Vec<u32> {
        let (sw, sh) = (small.w as usize, small.h as usize);
        let at = |x: usize, y: usize| small.px[y.min(sh - 1) * sw + x.min(sw - 1)];
        let lerp = |a: u32, b: u32, t: f32| {
            let ch = |s: u32| {
                let (x, y) = ((a >> s & 0xff) as f32, (b >> s & 0xff) as f32);
                ((x + (y - x) * t) as u32) << s
            };
            ch(16) | ch(8) | ch(0)
        };
        let mut out = Vec::new();
        for y in 0..h {
            let fy = ((y as f32 + 0.5) * sh as f32 / h as f32 - 0.5).max(0.0);
            let (y0, ty) = (fy as usize, fy.fract());
            let dark = 180 + 55 * y as u32 / h.max(1) as u32;
            for x in 0..w {
                let fx = ((x as f32 + 0.5) * sw as f32 / w as f32 - 0.5).max(0.0);
                let (x0, tx) = (fx as usize, fx.fract());
                let top = lerp(at(x0, y0), at(x0 + 1, y0), tx);
                let bottom = lerp(at(x0, y0 + 1), at(x0 + 1, y0 + 1), tx);
                out.push(mix(lerp(top, bottom, ty), page, dark));
            }
        }
        out
    }

    /// CPU time the calling thread has used.
    fn thread_cpu() -> Duration {
        Duration::from_secs_f64(crate::run_ahead::thread_cpu_ms() / 1000.0)
    }

    #[test]
    fn the_fast_glow_looks_like_the_float_one_and_is_fast() {
        let small = Image { w: 16, h: 12, px: (0..192u32).map(|i| (i * 37 % 256) << 16 | (i * 91 % 256) << 8 | (255 - i)).collect() };
        for (w, h, page) in [(1240, 1080, 0x000000), (1240, 1080, 0xf1ece2), (97, 53, 0x101010)] {
            let (fast, float) = (glow(&small, w, h, page), glow_float(&small, w, h, page));
            assert_eq!(fast.len(), float.len());
            let worst = fast
                .iter()
                .zip(&float)
                .flat_map(|(a, b)| [16, 8, 0].map(|s| ((a >> s & 0xff) as i32 - (b >> s & 0xff) as i32).abs()))
                .max()
                .unwrap();
            assert!(worst <= 2, "{w}x{h} on {page:06x}: a channel differs by {worst}");
        }
        // A big picture's glow colours come from samples; a flat one gives its colour exactly.
        let flat = Image { w: 1000, h: 900, px: vec![0x336699; 900_000] };
        let src = glow_source(&flat);
        assert_eq!((src.w, src.h), (16, 12));
        assert!(src.px.iter().all(|p| *p == 0x336699));
        let halves = Image { w: 1000, h: 900, px: (0..900_000).map(|i| if i % 1000 < 500 { 0xff0000 } else { 0x0000ff }).collect() };
        let src = glow_source(&halves);
        assert_eq!((src.px[0], src.px[15]), (0xff0000, 0x0000ff), "left red, right blue");
        let one = Image { w: 1, h: 2, px: vec![0x402010, 0x100804] };
        assert_eq!(glow(&one, 8, 8, 0).len(), 64, "the system colour's two-pixel glow");
        // The glow runs on this thread only; its CPU time is not stretched by other work on a busy machine.
        let t = thread_cpu();
        for _ in 0..5 {
            std::hint::black_box(glow(&small, 1240, 1080, 0));
        }
        // 15 ms each before (Mac, release); generous for a busy machine and debug builds.
        let each = (thread_cpu() - t) / 5;
        assert!(each < Duration::from_millis(if cfg!(debug_assertions) { 400 } else { 12 }), "{each:?}");
    }
}
