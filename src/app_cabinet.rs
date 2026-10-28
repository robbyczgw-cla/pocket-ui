//! Part of the pocket-ui binary: the Cabinet look (`pocket_ui::cabinet`, painted with `pocket_ui::cabinet_art`).
//!
//! The cabinet's cursor is the app's own selection: a shelf is a screen (the top shelf is Home's recent games,
//! then Favorites, then each system's games) and the case is its selected row. So Play, Star, Details, the game
//! switcher and the sounds work as in every other look; this file only moves that selection in two directions
//! and paints it.
//!
//! Box art is read on a thread of its own: the spines' colours, and the cover of the case being pulled out.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use pocket_ui::cabinet::{self, Cabinet, Case, FormFactor, Media, Pull, ShelfKey, Tween};
use pocket_ui::media::{self, Medium};
use pocket_ui::cabinet_art::{self as art, Look, Pic, Turn};
use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::image::{self, Image};
use pocket_ui::palette::*;
use pocket_ui::text::Face;

use super::{stripe, App, Button, Item, Screen, CANCEL, CONFIRM, REPEAT_DELAY};

/// The cabinet's sides on screen.
const CAB_X: i32 = 16;
/// Where the rows of the cabinet start and end; above is the title, below the hints. The bottom and the floor below
/// are measured from the screen's bottom edge, where the hints are: 992 and 800 on the Mini V2's 1080 lines, 872 and
/// 680 on the Mini V1's 960 (docs/DEVICES.md).
const SCENE_TOP: i32 = 214;
const SCENE_BOTTOM_GAP: i32 = 1080 - 992;
/// The selected shelf's floor stays this far above the screen's bottom edge; the others move past.
const FLOOR_GAP: i32 = 1080 - 800;

fn scene_bottom(h: i32) -> i32 {
    h - SCENE_BOTTOM_GAP
}

fn floor_y(h: i32) -> i32 {
    h - FLOOR_GAP
}
/// The cabinet's top board and its plinth.
const TOP_H: i32 = 40;
const PLINTH_H: i32 = 56;
/// A case out of its shelf is held up in front of the shelf above, its foot this far below the top of its own
/// shelf: the row it came from stays in view.
const OUT_FOOT: i32 = 50;
/// The largest a case's front is shown: what fits between the title and the tallest shelf's top.
const SHOW_W: f32 = 400.0;
const SHOW_H: f32 = 354.0;

/// How much bigger a case of this kind is shown when it is out.
fn show_scale(m: Medium) -> f32 {
    let s = m.size();
    (SHOW_W / s.front as f32).min(SHOW_H / s.h as f32).clamp(1.2, 2.1)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Kind {
    Color,
    /// The cover cut for a front of that medium this many pixels high.
    Cover(Medium, i32),
}

/// What the reader thread brings back.
enum Got {
    /// The cover's main colour and its accent (`media::cover_colors`).
    Color(Option<(u32, u32)>),
    Cover(Option<Image>),
}

#[derive(Clone)]
enum Slot<T> {
    Pending,
    Ready(T),
    Missing,
}

/// The reader thread: covers asked for first, spine colours after, one at a time.
#[derive(Clone)]
struct Reader {
    queue: Arc<(Mutex<VecDeque<(PathBuf, Kind)>>, Condvar)>,
    done: Arc<Mutex<Vec<(PathBuf, Kind, Got)>>>,
}

impl Reader {
    fn start() -> Reader {
        let r = Reader {
            queue: Default::default(),
            done: Default::default(),
        };
        let worker = r.clone();
        std::thread::Builder::new()
            .name("cabinet-art".into())
            .spawn(move || {
                // The media's templates first: the first case of each kind then costs no decoding in a frame.
                media::warm();
                loop {
                    let job = {
                        let (lock, cv) = &*worker.queue;
                        let mut q = lock.lock().unwrap();
                        loop {
                            if let Some(job) = q.pop_front() {
                                break job;
                            }
                            q = cv.wait(q).unwrap();
                        }
                    };
                    let got = read(&job.0, job.1, None);
                    worker.done.lock().unwrap().push((job.0, job.1, got));
                }
            })
            .expect("cabinet reader thread");
        r
    }

    fn ask(&self, path: PathBuf, kind: Kind) {
        let (lock, cv) = &*self.queue;
        let mut q = lock.lock().unwrap();
        match kind {
            Kind::Cover(..) => q.push_front((path, kind)),
            Kind::Color => q.push_back((path, kind)),
        }
        cv.notify_one();
    }

    fn take(&self) -> Vec<(PathBuf, Kind, Got)> {
        std::mem::take(&mut *self.done.lock().unwrap())
    }
}

/// Reads one cover: its spine colour, or the picture cut for a case's front.
fn read(path: &Path, kind: Kind, loaded: Option<&Image>) -> Got {
    let owned;
    let img = match loaded {
        Some(img) => Some(img),
        None => {
            owned = image::load_image(path);
            owned.as_ref()
        }
    };
    match kind {
        Kind::Color => Got::Color(img.and_then(|i| media::cover_colors(&i.px))),
        Kind::Cover(m, h) => Got::Cover(img.map(|i| {
            let (_, _, lw, lh) = media::label_rect(m, h as f32 / m.size().h as f32);
            art::cover_fill(i, lw, lh)
        })),
    }
}

/// A case's pictures, made once: by game, case and colours.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CaseKey {
    rom: PathBuf,
    medium: Medium,
    look: Look,
}

/// A case's big spine and front, whether the front has its cover yet, and the frame it was last used in.
type Shown = (Rc<Pic>, Rc<Pic>, bool, u64);

/// The case going into the console while the game starts.
struct Inserting {
    /// When A was pressed, and whether the game has been started since (at `cabinet::LAUNCH_AT`).
    at: Instant,
    launched: bool,
    item: Item,
    system: String,
    ff: FormFactor,
    media: Media,
    front: Rc<Pic>,
    from: (i32, i32, i32, i32),
    tint: u32,
}

#[derive(Default)]
struct Timing {
    frames: u32,
    total: Duration,
    worst: Duration,
    /// The same in CPU time of the UI thread: what the frame cost, without waiting for a busy machine.
    cpu_total: Duration,
    cpu_worst: Duration,
    /// What was made during the frame being painted, and how long it took.
    made: Vec<(&'static str, Duration)>,
}

pub(crate) struct CabinetUi {
    cab: Cabinet,
    /// The library, recents or favourites changed: the shelves are made again before the next use.
    pub(crate) dirty: bool,
    pull: Pull,
    scroll_y: Tween,
    scroll_x: HashMap<ShelfKey, Tween>,
    placed: bool,
    /// The wood, per (width, inner height, grain, page colour: walnut when dark, oak when light).
    walls: HashMap<(i32, i32, u32, u32), Rc<Pic>>,
    boards: Option<(u32, Rc<Pic>, Rc<Pic>)>,
    spines: HashMap<CaseKey, (Rc<Pic>, u64)>,
    /// The big spine and front of a case being turned, and whether that front has its cover yet.
    shown: HashMap<CaseKey, (Rc<Pic>, Rc<Pic>, bool, u64)>,
    /// Per ROM: its box art file, if it has one.
    cover_files: HashMap<PathBuf, Option<PathBuf>>,
    colors: HashMap<PathBuf, Slot<(u32, u32)>>,
    labels: HashMap<(PathBuf, Medium, i32), Slot<Rc<Image>>>,
    /// The Shelf look's cases lying in the stack, per (medium, plastic, length, thickness): the template only, with
    /// where its label and band are (the game's colours are painted over them).
    lying: HashMap<(Medium, u32, i32, i32), LyingPic>,
    /// The Shelf look's case on the pulled-out label: its spine and front, whether that front has its cover yet.
    shelf_shown: HashMap<(CaseKey, i32), Shown>,
    reader: Option<Reader>,
    pending: usize,
    /// Read covers right away instead of on the thread (snapshots, the look step's previews).
    pub(crate) inline: bool,
    /// Where each case was painted, for taps: (rectangle, shelf, case); the pulled-out one last.
    drawn: Vec<(i32, i32, i32, i32, usize, usize)>,
    insert: Option<Inserting>,
    /// The front of the selected case as last painted: where an insert starts from.
    last_out: Option<(Item, (i32, i32, i32, i32))>,
    consoles: HashMap<String, Option<Rc<pocket_ui::system_art::Sprite>>>,
    frame: u64,
    timing: Timing,
    /// Curator's shelves as last made (`App::cabinet_curator_keys`): one coming or going makes the shelves again.
    curator_keys: Vec<ShelfKey>,
    /// The case out of its shelf and the one going back, with their shelves: a game on two shelves (Favorites and
    /// its system's, a smart shelf) comes out of the one the cursor is on only.
    out_on: Option<(Item, ShelfKey)>,
    back_on: Option<(Item, ShelfKey)>,
}

impl Default for CabinetUi {
    fn default() -> Self {
        CabinetUi {
            cab: Cabinet::default(),
            dirty: true,
            pull: Pull::default(),
            scroll_y: Tween::new(0.0, cabinet::SCROLL),
            scroll_x: HashMap::new(),
            placed: false,
            walls: HashMap::new(),
            boards: None,
            spines: HashMap::new(),
            shown: HashMap::new(),
            cover_files: HashMap::new(),
            colors: HashMap::new(),
            labels: HashMap::new(),
            lying: HashMap::new(),
            shelf_shown: HashMap::new(),
            reader: None,
            pending: 0,
            inline: false,
            drawn: Vec::new(),
            insert: None,
            last_out: None,
            consoles: HashMap::new(),
            frame: 0,
            timing: Timing::default(),
            curator_keys: Vec::new(),
            out_on: None,
            back_on: None,
        }
    }
}

impl CabinetUi {
    /// New box art arrived: look again for games that had none.
    pub(crate) fn covers_changed(&mut self) {
        self.cover_files.retain(|_, f| f.is_some());
        self.colors.retain(|_, c| !matches!(c, Slot::Missing));
        self.labels.retain(|_, c| !matches!(c, Slot::Missing));
    }

    /// Every glide as if it had started `by` earlier.
    fn age(&mut self, by: Duration) {
        self.scroll_y.age(by);
        for t in self.scroll_x.values_mut() {
            t.age(by);
        }
    }

    pub(crate) fn animating(&self, now: Instant) -> bool {
        self.pull.animating(now)
            || self.scroll_y.moving(now)
            || self.scroll_x.values().any(|t| t.moving(now))
            || self.pending > 0
            || self.insert.as_ref().is_some_and(|i| !i.launched)
    }

    /// The pause before the launch fade: the rest of the case going into the console.
    pub(crate) fn launch_delay(&self) -> Duration {
        if self.insert.as_ref().is_some_and(|i| i.launched) {
            cabinet::fade_delay()
        } else {
            Duration::ZERO
        }
    }

    /// When the game waiting for its cartridge to reach the slot is to be started.
    pub(crate) fn launch_due(&self) -> Option<Instant> {
        self.insert.as_ref().filter(|i| !i.launched).map(|i| i.at + cabinet::LAUNCH_AT)
    }

    fn log_frame(&mut self, took: Duration, cpu: Duration) {
        static LOG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if !*LOG.get_or_init(|| std::env::var("POCKETUI_FRAME_LOG").is_ok_and(|v| v != "0")) {
            return;
        }
        let t = &mut self.timing;
        t.frames += 1;
        t.total += took;
        t.worst = t.worst.max(took);
        t.cpu_total += cpu;
        t.cpu_worst = t.cpu_worst.max(cpu);
        let made = std::mem::take(&mut t.made);
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        if cpu > Duration::from_millis(6) {
            let parts: Vec<String> = made.iter().map(|(what, d)| format!("{what} {:.1}", ms(*d))).collect();
            println!("cabinet: slow frame {:.2} ms, cpu {:.2} ms (made: {})", ms(took), ms(cpu), parts.join(", "));
        }
        if t.frames == 60 {
            println!(
                "cabinet: 60 frames painted, average {:.2} ms (cpu {:.2}), worst {:.2} ms (cpu {:.2})",
                ms(t.total) / 60.0,
                ms(t.cpu_total) / 60.0,
                ms(t.worst),
                ms(t.cpu_worst)
            );
            *t = Timing::default();
        }
    }

    fn made(&mut self, what: &'static str, since: Instant) {
        self.timing.made.push((what, since.elapsed()));
    }
}

/// CPU time this thread has used.
fn thread_cpu() -> Duration {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime only writes the timespec it is given.
    if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) } != 0 {
        return Duration::ZERO;
    }
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

/// Title, system id and ROM of something on a shelf.
pub(crate) struct Info {
    title: String,
    system: String,
    rom: PathBuf,
}

impl App {
    fn case_info(&self, item: Item) -> Info {
        match item {
            Item::Recent(i) => {
                let r = &self.recents.list[i];
                Info {
                    title: r.name.clone(),
                    system: r.system.clone(),
                    rom: r.path.clone(),
                }
            }
            Item::Game(s, g) => {
                let (sys, game) = (&self.systems[s], &self.systems[s].games[g]);
                Info {
                    title: game.name.clone(),
                    system: sys.name.clone(),
                    rom: game.path.clone(),
                }
            }
            Item::Streaming => Info {
                title: "PC streaming".into(),
                system: "streaming".into(),
                rom: PathBuf::new(),
            },
            Item::System(s) => Info {
                title: self.systems[s].fullname.clone(),
                system: self.systems[s].name.clone(),
                rom: PathBuf::new(),
            },
            Item::Favorites => Info {
                title: "Favourites".into(),
                system: "favorites".into(),
                rom: PathBuf::new(),
            },
            // Curator's boxes on its shelf (the Lens, smart shelves, stickers), coloured as Curator's rows are.
            // Their "ROM" only tells their pictures apart (and a renamed one from its old name); they have no box art.
            Item::Curator | Item::Lens | Item::Surprise | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) => {
                let title = self.curator_describe(item).0;
                // Surprise me's box shows the day's pick on its front (`surprise_front`): a new pick, a new front.
                let pick = if item == Item::Surprise { self.surprise_front().map(|(_, rom)| rom.to_string_lossy().into_owned()).unwrap_or_default() } else { String::new() };
                Info {
                    rom: PathBuf::from(format!("curator/{item:?}/{title}/{pick}")),
                    title,
                    system: if matches!(item, Item::Sticker(_)) { "sticker" } else { "curator" }.into(),
                }
            }
            // Not on the cabinet's shelves (Up on the top shelf opens the search); here for completeness.
            Item::Search => Info {
                title: "Search".into(),
                system: "search".into(),
                rom: PathBuf::new(),
            },
        }
    }

    /// The case a game (or one of Curator's boxes) stands in on the shelf (`media`).
    pub(crate) fn medium(&self, item: Item) -> Medium {
        match item {
            Item::Streaming | Item::Curator | Item::Lens | Item::Surprise | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) => Medium::Keep,
            _ => Medium::of(&self.case_info(item).system),
        }
    }

    fn form_factor(&self, item: Item) -> FormFactor {
        match item {
            Item::Streaming | Item::Curator | Item::Lens | Item::Surprise | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) => FormFactor::DvdCase,
            _ => FormFactor::of(&self.case_info(item).system),
        }
    }

    /// The shelves, made again when something on them changed.
    fn cabinet_shelves(&mut self) {
        let curator = self.cabinet_curator_keys();
        if curator != self.cabinet.curator_keys {
            self.cabinet.dirty = true;
        }
        if !self.cabinet.dirty {
            return;
        }
        self.cabinet.dirty = false;
        let case = |app: &App, item: Item| Case {
            item,
            ff: app.form_factor(item),
            medium: app.medium(item),
        };
        let recent: Vec<Case> = self
            .home_items()
            .into_iter()
            .filter(|i| matches!(i, Item::Recent(_) | Item::Streaming))
            .map(|i| case(self, i))
            .collect();
        let favorites: Vec<Case> = self.favorite_items().into_iter().map(|i| case(self, i)).collect();
        let systems = pocket_ui::shelf::visible_systems(self.systems.iter().map(|s| s.name.as_str()), &self.hidden)
            .into_iter()
            .map(|s| {
                let (ff, medium) = (FormFactor::of(&self.systems[s].name), Medium::of(&self.systems[s].name));
                (s, (0..self.systems[s].games.len()).map(|g| Case { item: Item::Game(s, g), ff, medium }).collect())
            })
            .collect();
        let curator_shelves = curator.iter().map(|&k| (k, self.cabinet_curator_items(k).into_iter().map(|i| case(self, i)).collect())).collect();
        self.cabinet.curator_keys = curator;
        self.cabinet.cab = Cabinet::build(recent, favorites, curator_shelves, systems);
    }

    /// Which case the app's selection is: a shelf per screen, the selected row on it.
    pub(crate) fn cabinet_cursor(&self) -> Option<(usize, usize)> {
        let cab = &self.cabinet.cab;
        if cab.shelves.is_empty() {
            return None;
        }
        let shelf = |key: ShelfKey| cab.shelves.iter().position(|s| s.key == key);
        // A Curator list is the shelf of its own (`app_curator::screen_shelf`).
        let curator = super::app_curator::screen_shelf(self.screen);
        let found = self.items.get(self.sel).copied().and_then(|item| {
            let key = match (self.screen, item) {
                _ if curator.is_some() => curator.unwrap_or(ShelfKey::Curator),
                // Home's Curator rows: the shelf they stand for.
                (_, Item::Curator | Item::History | Item::Review(_)) => ShelfKey::Curator,
                (_, Item::SmartShelf(i)) => ShelfKey::SmartShelf(i),
                (_, Item::Sticker(id)) => ShelfKey::Sticker(id),
                (_, Item::Recent(_) | Item::Streaming | Item::Search | Item::Lens | Item::Surprise) => ShelfKey::Continue,
                (Screen::Favorites, _) | (_, Item::Favorites) => ShelfKey::Favorites,
                (_, Item::Game(s, _) | Item::System(s)) => ShelfKey::System(s),
            };
            match item {
                Item::System(_) | Item::Favorites | Item::Curator => shelf(key).map(|s| (s, 0)),
                Item::SmartShelf(_) | Item::Sticker(_) if curator.is_none() => shelf(key).map(|s| (s, 0)),
                _ => cab.find(key, item),
            }
        });
        found.or_else(|| match self.screen {
            Screen::Games(s) => shelf(ShelfKey::System(s)).map(|i| (i, 0)),
            Screen::Favorites => shelf(ShelfKey::Favorites).map(|i| (i, 0)),
            _ => curator.and_then(shelf).map(|i| (i, 0)).or(Some((0, 0))),
        })
    }

    /// The look step's preview: the sample game on its system's shelf, which is fuller than the recent games'.
    fn cabinet_preview_cursor(&self) -> Option<(usize, usize)> {
        let rom = match self.items.get(self.sel) {
            Some(Item::Recent(i)) => self.recents.list.get(*i).map(|r| r.path.clone()),
            _ => None,
        };
        let on_system = rom.and_then(|rom| {
            let s = self.systems.iter().position(|s| s.games.iter().any(|g| g.path == rom))?;
            let g = self.systems[s].games.iter().position(|g| g.path == rom)?;
            self.cabinet.cab.find(ShelfKey::System(s), Item::Game(s, g))
        });
        on_system.or_else(|| self.cabinet_cursor())
    }

    /// Moves the app's selection to a case: its shelf's screen, its row.
    pub(crate) fn cabinet_go(&mut self, (s, i): (usize, usize)) {
        let Some(shelf) = self.cabinet.cab.shelves.get(s) else { return };
        let Some(item) = shelf.cases.get(i).map(|c| c.item) else { return };
        let screen = match shelf.key {
            ShelfKey::Continue => Screen::Home,
            ShelfKey::Favorites => Screen::Favorites,
            ShelfKey::System(x) => Screen::Games(x),
            key => super::app_curator::shelf_screen(key).unwrap_or(Screen::Home),
        };
        let moved = self.screen != screen || self.items.get(self.sel) != Some(&item);
        if self.screen != screen {
            self.curator_left();
            self.screen = screen;
            self.rebuild();
            // Same library, same shelves: nothing to make again.
            self.cabinet.dirty = false;
        }
        if let Some(pos) = self.items.iter().position(|it| *it == item) {
            if moved {
                self.prev_sel = self.sel;
                self.sel = pos;
                self.moved_at = Instant::now();
            }
        }
    }

    /// The selection where the cabinet shows it (after the look was changed on a system card, say).
    pub(crate) fn cabinet_sync(&mut self) -> Option<(usize, usize)> {
        self.cabinet_shelves();
        let at = self.cabinet_cursor()?;
        let shown = self.cabinet.cab.shelves[at.0].cases[at.1].item;
        if self.items.get(self.sel) != Some(&shown) {
            self.cabinet_go(at);
        }
        Some(at)
    }

    /// A button on the cabinet: true when it was a move of the cabinet's own (the rest goes the usual way).
    pub(crate) fn cabinet_press(&mut self, b: Button) -> bool {
        // The cartridge is on its way into the console: nothing else until the game has started.
        if self.cabinet.launch_due().is_some() {
            return true;
        }
        if self.cabinet_sync().is_none() {
            return false;
        }
        match b {
            Button::Up | Button::Down | Button::Left | Button::Right | Button::L1 | Button::R1 => {
                self.cabinet_step(b);
                self.held = Some((b, Instant::now() + REPEAT_DELAY));
                true
            }
            // There is no level above the cabinet to go back to, but a Curator list opened from the Curator shelf
            // or the Lens goes back there (`app_curator`).
            b if b == CANCEL => {
                self.curator_back();
                true
            }
            // A game goes into the console first and starts on its way in (`cabinet_launch`); anything else
            // (PC streaming, a game without its case out) starts at once as in every look.
            b if b == CONFIRM => self.cabinet_begin_insert(),
            _ => false,
        }
    }

    /// One step of the cursor (also a held button repeating).
    pub(crate) fn cabinet_step(&mut self, b: Button) {
        let Some(at) = self.cabinet_sync() else { return };
        let scroll = |s: usize| {
            self.cabinet
                .cab
                .shelves
                .get(s)
                .and_then(|sh| self.cabinet.scroll_x.get(&sh.key))
                .map_or(0.0, |t| t.target())
        };
        let initial = |item: Item| cabinet::initial(&self.case_info(item).title);
        if let Some(to) = cabinet::step(&self.cabinet.cab, at, b, &scroll, &initial) {
            self.cabinet_go(to);
        }
    }

    /// The case under a tap: Some(true) when it was the selected one (play it), Some(false) when it is
    /// selected now.
    pub(crate) fn cabinet_tap(&mut self, x: f32, y: f32) -> Option<bool> {
        if self.cabinet.launch_due().is_some() {
            return Some(false);
        }
        let (x, y) = (x as i32, y as i32);
        let hit = self
            .cabinet
            .drawn
            .iter()
            .rev()
            .find(|(rx, ry, rw, rh, _, _)| x >= *rx && y >= *ry && x < rx + rw && y < ry + rh)
            .map(|d| (d.4, d.5))?;
        if self.cabinet_cursor() == Some(hit) {
            return Some(true);
        }
        self.cabinet_go(hit);
        Some(false)
    }

    /// A on a game: its case starts towards the console; true when it did (the game starts on the way).
    fn cabinet_begin_insert(&mut self) -> bool {
        self.cabinet.insert = None;
        let Some(item) = self.items.get(self.sel).copied() else { return false };
        if !matches!(item, Item::Recent(_) | Item::Game(..)) {
            return false;
        }
        let info = self.case_info(item);
        let (ff, medium) = (self.form_factor(item), self.medium(item));
        let Some(from) = self.cabinet.last_out.filter(|(i, _)| *i == item).map(|(_, r)| r) else {
            return false;
        };
        let look = self.case_look(&info, medium);
        let key = CaseKey {
            rom: info.rom.clone(),
            medium,
            look,
        };
        let Some((_, front, _, _)) = self.cabinet.shown.get(&key).cloned() else {
            return false;
        };
        self.cabinet.insert = Some(Inserting {
            at: Instant::now(),
            launched: false,
            item,
            system: info.system,
            ff,
            media: ff.media(),
            front,
            from,
            tint: look.color,
        });
        true
    }

    /// The cartridge has reached the slot: the game starts, as A does in every look.
    pub(crate) fn cabinet_launch(&mut self) {
        if let Some(ins) = self.cabinet.insert.as_mut() {
            ins.launched = true;
        }
        self.activate();
        // Nothing was started (the system is gone): the cabinet comes back.
        if self.launch_at.is_none() {
            self.cabinet.insert = None;
        }
    }

    // ---- art -------------------------------------------------------------------------------------------

    fn reader(&mut self) -> Reader {
        self.cabinet.reader.get_or_insert_with(Reader::start).clone()
    }

    /// The box art file of a ROM, looked for once.
    pub(crate) fn cover_file(&mut self, system: &str, rom: &Path) -> Option<PathBuf> {
        if rom.as_os_str().is_empty() || system == "curator" || system == "sticker" {
            return None;
        }
        if let Some(found) = self.cabinet.cover_files.get(rom) {
            return found.clone();
        }
        if let Some((path, _)) = self.sample_cover.as_ref().filter(|(p, _)| p == rom) {
            let found = Some(path.clone());
            self.cabinet.cover_files.insert(rom.to_path_buf(), found.clone());
            return found;
        }
        let mut candidates = vec![pocket_ui::covers::cover_path(&self.data_dir, system, rom)];
        if system == "ports" {
            let launcher = rom.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            candidates.extend(pocket_ui::ports::launcher_images(&launcher));
        }
        // Then the picture in the player's EmulationStation gamelist (`app_gamelist`).
        let found = match candidates.into_iter().find(|p| p.exists()) {
            Some(p) => Some(p),
            None => self.gamelist_cover(system, rom),
        };
        self.cabinet.cover_files.insert(rom.to_path_buf(), found.clone());
        found
    }

    /// Reads now or asks the thread, for a cover file.
    fn want(&mut self, file: &Path, kind: Kind) {
        let sample = self.sample_cover.as_ref().filter(|(p, _)| p == file).map(|(_, img)| img.clone());
        if self.cabinet.inline || sample.is_some() {
            let got = read(file, kind, sample.as_deref());
            self.cabinet_got(file.to_path_buf(), kind, got);
            return;
        }
        match kind {
            Kind::Color => {
                self.cabinet.colors.insert(file.to_path_buf(), Slot::Pending);
            }
            Kind::Cover(m, h) => {
                self.cabinet.labels.insert((file.to_path_buf(), m, h), Slot::Pending);
            }
        }
        self.cabinet.pending += 1;
        self.reader().ask(file.to_path_buf(), kind);
    }

    fn cabinet_got(&mut self, file: PathBuf, kind: Kind, got: Got) {
        match (kind, got) {
            (Kind::Color, Got::Color(c)) => {
                self.cabinet.colors.insert(file, c.map_or(Slot::Missing, Slot::Ready));
            }
            (Kind::Cover(m, h), Got::Cover(img)) => {
                self.cabinet.labels.insert((file, m, h), img.map_or(Slot::Missing, |i| Slot::Ready(Rc::new(i))));
            }
            _ => {}
        }
    }

    fn cabinet_poll(&mut self) {
        let Some(reader) = self.cabinet.reader.clone() else { return };
        for (file, kind, got) in reader.take() {
            self.cabinet.pending = self.cabinet.pending.saturating_sub(1);
            self.cabinet_got(file, kind, got);
        }
    }

    /// A case's colours: its label and band from its cover (asked for when first needed, read on the thread), its
    /// system's otherwise; a cartridge's plastic is its system's.
    pub(crate) fn case_look(&mut self, info: &Info, m: Medium) -> Look {
        let stripe_c = stripe(&info.system);
        let fallback = (mix(stripe_c, 0x101010, 70), stripe_c);
        let (color, accent) = match self.cover_file(&info.system, &info.rom) {
            Some(file) => {
                if !self.cabinet.colors.contains_key(&file) {
                    self.want(&file, Kind::Color);
                }
                match self.cabinet.colors.get(&file) {
                    Some(Slot::Ready(c)) => *c,
                    _ => fallback,
                }
            }
            None => fallback,
        };
        let plastic = if m.is_cart() { art::plastic(&info.system) } else { 0x141416 };
        Look {
            color,
            band: media::apart(accent, color),
            plastic,
        }
    }

    fn spine_pic(&mut self, item: Item, m: Medium) -> Rc<Pic> {
        let info = self.case_info(item);
        let look = self.case_look(&info, m);
        let key = CaseKey {
            rom: info.rom.clone(),
            medium: m,
            look,
        };
        let frame = self.cabinet.frame;
        if let Some((pic, used)) = self.cabinet.spines.get_mut(&key) {
            *used = frame;
            return pic.clone();
        }
        let t0 = Instant::now();
        let pic = Rc::new(media::spine(&mut self.text, m, 1.0, look, &info.title));
        self.cabinet.made("spine", t0);
        self.cabinet.spines.insert(key, (pic.clone(), frame));
        pic
    }

    /// A cover cut for the front of a case `h` pixels high: asked for the first time it is needed (read on the
    /// thread), None until it is there.
    fn cover_label(&mut self, file: &Path, m: Medium, h: i32) -> Option<Rc<Image>> {
        let key = (file.to_path_buf(), m, h);
        if !self.cabinet.labels.contains_key(&key) {
            self.want(file, Kind::Cover(m, h));
        }
        match self.cabinet.labels.get(&key) {
            Some(Slot::Ready(img)) => Some(img.clone()),
            _ => None,
        }
    }

    /// The day's pick (system, file) whose box art is on Surprise me's front (docs/DESIGN.md "Home panels").
    fn surprise_front(&self) -> Option<(String, PathBuf)> {
        self.curator.index.as_ref()?;
        let pick = self.curator.today.pick.as_ref()?;
        let tile = self.catalog_tile(pick.game)?;
        Some((tile.system, tile.rom))
    }

    /// The big spine and front of a case coming out; its cover is asked for the first time it is needed.
    fn shown_pics(&mut self, item: Item, m: Medium) -> (Rc<Pic>, Rc<Pic>) {
        let info = self.case_info(item);
        let look = self.case_look(&info, m);
        // Surprise me's box keeps Curator's colours and shows the day's pick's cover on its front.
        let file = match (item, self.surprise_front()) {
            (Item::Surprise, Some((system, rom))) => self.cover_file(&system, &rom),
            _ => self.cover_file(&info.system, &info.rom),
        };
        let k = show_scale(m);
        let label = file.and_then(|f| self.cover_label(&f, m, (m.size().h as f32 * k).round() as i32));
        let key = CaseKey {
            rom: info.rom.clone(),
            medium: m,
            look,
        };
        let frame = self.cabinet.frame;
        if let Some((spine, front, has, used)) = self.cabinet.shown.get_mut(&key) {
            if *has || label.is_none() {
                *used = frame;
                return (spine.clone(), front.clone());
            }
        }
        let sprite = pocket_ui::system_art::get(&info.system);
        let t0 = Instant::now();
        let spine = Rc::new(media::spine(&mut self.text, m, k, look, &info.title));
        self.cabinet.made("big spine", t0);
        let t0 = Instant::now();
        let front = Rc::new(media::front(&mut self.text, m, k, look, &info.title, label.as_deref(), sprite));
        self.cabinet.made(if label.is_some() { "front" } else { "plain front" }, t0);
        self.cabinet.shown.insert(key, (spine.clone(), front.clone(), label.is_some(), frame));
        (spine, front)
    }

    fn wall_pic(&mut self, inner: i32, seed: u32, w: i32) -> Rc<Pic> {
        if let Some(p) = self.cabinet.walls.get(&(w, inner, seed, pal().void)) {
            return p.clone();
        }
        let t0 = Instant::now();
        let p = Rc::new(art::compartment(w, inner, seed));
        self.cabinet.made("wood", t0);
        self.cabinet.walls.insert((w, inner, seed, pal().void), p.clone());
        p
    }

    fn console_sprite(&mut self, system: &str) -> Option<Rc<pocket_ui::system_art::Sprite>> {
        self.cabinet
            .consoles
            .entry(system.to_string())
            .or_insert_with(|| pocket_ui::system_art::get(system).map(|s| Rc::new(s.fitted(560, 330))))
            .clone()
    }

    // ---- painting ----------------------------------------------------------------------------------------

    pub(crate) fn paint_cabinet(&mut self, c: &mut Canvas) {
        let started = Instant::now();
        let cpu_started = thread_cpu();
        let now = started;
        self.cabinet.frame += 1;
        self.cabinet_poll();
        self.cabinet_shelves();
        c.clear(pal().void);
        self.cabinet.drawn.clear();
        // The app's selection becomes a case (after the look was changed on a system card, say); the look step's
        // previews paint a sample and leave the selection alone.
        let at = if self.sample_cover.is_none() {
            self.cabinet_sync()
        } else {
            self.cabinet_preview_cursor()
        };
        let Some((cs, ci)) = at else { return };
        let item = self.cabinet.cab.shelves[cs].cases[ci].item;
        self.cabinet.pull.follow(item, self.moved_at);
        let on = Some((item, self.cabinet.cab.shelves[cs].key));
        if self.cabinet.out_on != on {
            self.cabinet.back_on = std::mem::replace(&mut self.cabinet.out_on, on);
        }

        // Where the view is: the selected shelf's floor at `floor_y`, its case in view along it.
        let cab_w = c.w - 2 * CAB_X;
        let view = cab_w - 2 * art::WALL;
        let inner_x = CAB_X + art::WALL;
        let want_y = (self.cabinet.cab.y(cs) + self.cabinet.cab.shelves[cs].height - floor_y(c.h)) as f32;
        let key = self.cabinet.cab.shelves[cs].key;
        let current_x = self.cabinet.scroll_x.get(&key).map_or(0.0, |t| t.target());
        let want_x = self.cabinet.cab.shelves[cs].scroll_to_show(ci, view, current_x);
        if !self.cabinet.placed {
            self.cabinet.placed = true;
            self.cabinet.scroll_y.jump(want_y);
            self.cabinet
                .scroll_x
                .entry(key)
                .or_insert_with(|| Tween::new(want_x, cabinet::SCROLL))
                .jump(want_x);
        }
        self.cabinet.scroll_y.set(want_y, now);
        self.cabinet
            .scroll_x
            .entry(key)
            .or_insert_with(|| Tween::new(current_x, cabinet::SCROLL))
            .set(want_x, now);
        let sy = self.cabinet.scroll_y.value(now).round() as i32;
        let clip = (SCENE_TOP, scene_bottom(c.h));

        // The cabinet's top board and plinth, at its ends.
        if self.cabinet.boards.as_ref().is_none_or(|(page, b, _)| b.w != cab_w || *page != pal().void) {
            self.cabinet.walls.retain(|(w, _, _, _), _| *w == cab_w);
            self.cabinet.boards = Some((pal().void, Rc::new(art::top_board(cab_w, TOP_H)), Rc::new(art::top_board(cab_w, PLINTH_H))));
        }
        let (_, top_board, plinth) = self.cabinet.boards.clone().unwrap();
        let cab_bottom = self.cabinet.cab.height() - sy;
        art::blit(c, &top_board, CAB_X, -sy - TOP_H, 256, clip);
        art::blit(c, &plinth, CAB_X, cab_bottom, 230, clip);

        let mut out: Vec<(usize, usize)> = Vec::new();
        // A plant, a lamp or a bookend on a shelf with room beside its games: two in view at most.
        let mut props = 0;
        for s in 0..self.cabinet.cab.shelves.len() {
            let top = self.cabinet.cab.y(s) - sy;
            let (inner, pitch) = (self.cabinet.cab.shelves[s].height, self.cabinet.cab.pitch(s));
            if top + pitch < SCENE_TOP || top > scene_bottom(c.h) {
                continue;
            }
            let floor = top + inner + 3;
            // The shelves away from the selected one are a little darker.
            let away = (floor - floor_y(c.h)).abs() as f32;
            let shade = 256 - (away * 0.22).min(70.0) as u32;
            let wall = self.wall_pic(inner, s as u32 % 3, cab_w);
            art::blit(c, &wall, CAB_X, top, shade, clip);
            let skey = self.cabinet.cab.shelves[s].key;
            let scroll = self.cabinet.scroll_x.get(&skey).map_or(0.0, |t| t.value(now)).round() as i32;
            let first = self.cabinet.cab.shelves[s].nearest(scroll).saturating_sub(1);
            for i in first..self.cabinet.cab.shelves[s].cases.len() {
                let case = self.cabinet.cab.shelves[s].cases[i];
                let x = inner_x + self.cabinet.cab.shelves[s].x(i) - scroll;
                if x > inner_x + view {
                    break;
                }
                let here = Some((case.item, skey));
                if self.cabinet.pull.openness(case.item, now) > 0.0 && (here == self.cabinet.out_on || here == self.cabinet.back_on) {
                    out.push((s, i));
                    continue;
                }
                let pic = self.spine_pic(case.item, case.medium);
                let y = floor - pic.h;
                // Inside the walls only.
                self.blit_between(c, &pic, x, y, shade, clip, inner_x, inner_x + view);
                // A little shadow on the back panel at the right of each case.
                art::soft_shadow(
                    c,
                    x + pic.w,
                    y + 6,
                    3,
                    pic.h - 6,
                    5,
                    0.35 * shade as f32 / 256.0,
                    (clip.0.max(top), clip.1.min(floor)),
                );
                self.cabinet.drawn.push((x, y, pic.w, pic.h, s, i));
            }
            if props < 2 && self.paint_prop(c, s, inner_x, view, scroll, floor, shade, clip) {
                props += 1;
            }
            self.paint_plate(c, s, inner_x + 18, floor + 5, shade, clip);
        }

        // The cases out of the shelf: the one going back first, the selected one over it.
        out.sort_by_key(|&(s, i)| (s, i) == (cs, ci));
        for (s, i) in out {
            self.paint_out(c, s, i, sy, now, inner_x);
        }

        // Black above and below the cabinet, rising softly at its edges.
        art::fade_band_to(c, SCENE_TOP + 34, SCENE_TOP, pal().void);
        art::fade_band_to(c, scene_bottom(c.h) - 34, scene_bottom(c.h), pal().void);
        for row in [(0, SCENE_TOP + 1), (scene_bottom(c.h), c.h)] {
            c.rect(0, row.0, c.w, row.1 - row.0, pal().void);
        }

        self.paint_insert(c, now);
        self.paint_cabinet_title(c, cs, item);
        // Taps are handled by `cabinet_tap`; swipes move by a case sideways and by a shelf up and down.
        let len = self.cabinet.cab.shelves[cs].cases.len();
        self.hits.row(pocket_ui::touch::Area::new(0, SCENE_TOP, c.w, scene_bottom(c.h) - SCENE_TOP), ci);
        self.hits.list(ci, len, 44.0, 250.0);

        // Pictures not used for a while go.
        let frame = self.cabinet.frame;
        if self.cabinet.spines.len() > 360 {
            self.cabinet.spines.retain(|_, (_, used)| frame - *used < 90);
        }
        if self.cabinet.shown.len() > 8 {
            self.cabinet.shown.retain(|_, (_, _, _, used)| frame - *used < 30);
        }
        if self.cabinet.labels.len() > 48 {
            self.cabinet.labels.clear();
        }
        self.cabinet.log_frame(started.elapsed(), thread_cpu().saturating_sub(cpu_started));
    }

    /// A spine, cut off at the cabinet's inner walls.
    #[allow(clippy::too_many_arguments)]
    fn blit_between(&self, c: &mut Canvas, pic: &Pic, x: i32, y: i32, shade: u32, clip: (i32, i32), left: i32, right: i32) {
        if x >= left && x + pic.w <= right {
            art::blit(c, pic, x, y, shade, clip);
            return;
        }
        let (from, to) = ((left - x).max(0), (right - x).min(pic.w));
        if from >= to {
            return;
        }
        let mut part = Pic::new(to - from, pic.h, 0);
        for row in 0..pic.h {
            let src = &pic.px[(row * pic.w + from) as usize..(row * pic.w + to) as usize];
            part.px[(row * part.w) as usize..((row + 1) * part.w) as usize].copy_from_slice(src);
        }
        art::blit(c, &part, x + from, y, shade, clip);
    }

    /// The prop of shelf `s` (`media::Prop::for_shelf`), if it has one and there is room for it beside the games:
    /// a bookend right after the last case, the others further along. Never over a case, and only on a shelf that
    /// doesn't scroll. True when one was painted.
    #[allow(clippy::too_many_arguments)]
    fn paint_prop(&mut self, c: &mut Canvas, s: usize, inner_x: i32, view: i32, scroll: i32, floor: i32, shade: u32, clip: (i32, i32)) -> bool {
        let shelf = &self.cabinet.cab.shelves[s];
        if scroll != 0 || shelf.width > view || shelf.cases.is_empty() {
            return false;
        }
        let seed = match shelf.key {
            ShelfKey::System(i) => self.systems.get(i).map_or(7, |sys| sys.name.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32)),
            ShelfKey::Continue => 1,
            ShelfKey::Favorites => 2,
            ShelfKey::Curator => 3,
            ShelfKey::Lens => 4,
            ShelfKey::SmartShelf(i) => 100 + i as u32,
            ShelfKey::Sticker(id) => 1000 + id,
        };
        let Some(prop) = media::Prop::for_shelf(seed, shelf.height - cabinet::HEADROOM / 2) else { return false };
        let pic = prop.pic();
        let games_end = inner_x + shelf.width - cabinet::INSET;
        let x = if prop == media::Prop::Bookend {
            games_end + 2
        } else {
            // Somewhere in the free part of the shelf, the same place every time; well clear of the games.
            let free = inner_x + view - 36 - pic.w - (games_end + 60);
            if free < 0 {
                return false;
            }
            games_end + 60 + (seed >> 3) as i32 % (free + 1).min(240)
        };
        if x + pic.w > inner_x + view {
            return false;
        }
        let y = floor - pic.h;
        art::blit(c, pic, x, y, shade, clip);
        art::soft_shadow(c, x + pic.w, y + pic.h / 3, 3, pic.h * 2 / 3, 6, 0.3 * shade as f32 / 256.0, (clip.0.max(y), clip.1.min(floor)));
        true
    }

    /// The shelf's name on a small paper card in a holder on the board's edge.
    fn paint_plate(&mut self, c: &mut Canvas, s: usize, x: i32, y: i32, shade: u32, clip: (i32, i32)) {
        if y < clip.0 || y + 26 > clip.1 {
            return;
        }
        let (name, sticker) = self.shelf_name(self.cabinet.cab.shelves[s].key);
        // A card in a brass holder as high as the board's edge: the design's size whatever Settings → Text size says.
        let _fixed = pocket_ui::type_scale::fixed();
        let name = self.text.fit(Face::Body, 20, &name, 420);
        let w = self.text.width(Face::Body, 20, &name) + 22;
        let dim = |col: u32| {
            let k = shade.min(256);
            ((col >> 16 & 0xff) * k >> 8) << 16 | ((col >> 8 & 0xff) * k >> 8) << 8 | (col & 0xff) * k >> 8
        };
        if sticker {
            // A sticker's shelf: the sticker itself on the board's edge, in the stickers' colour.
            let colour = stripe("sticker");
            c.round_rect(x, y - 2, w + 4, 28, 13, dim(colour));
            self.text.draw(c, Face::Body, 20, x + 13, y + 18, dim(cabinet::ink_on(colour)), &name);
            return;
        }
        // A brass-coloured holder, the card in it.
        c.round_rect(x - 3, y - 3, w + 6, 30, 4, dim(0x8a7350));
        c.round_rect(x, y, w, 24, 2, dim(0xe6dfcf));
        self.text.draw(c, Face::Body, 20, x + 11, y + 19, dim(0x3a3026), &name);
    }

    /// A case out of its shelf: sliding out and turning (`openness` 0 to 1).
    fn paint_out(&mut self, c: &mut Canvas, s: usize, i: usize, sy: i32, now: Instant, inner_x: i32) {
        let case = self.cabinet.cab.shelves[s].cases[i];
        // The case going into the console is drawn by `paint_insert`.
        if self.cabinet.insert.as_ref().is_some_and(|ins| ins.item == case.item) {
            return;
        }
        let o = self.cabinet.pull.openness(case.item, now);
        let size = case.medium.size();
        let skey = self.cabinet.cab.shelves[s].key;
        let scroll = self.cabinet.scroll_x.get(&skey).map_or(0.0, |t| t.value(now));
        let floor = (self.cabinet.cab.y(s) - sy + self.cabinet.cab.shelves[s].height + 3) as f32;
        let slot_cx = inner_x as f32 + self.cabinet.cab.shelves[s].x(i) as f32 - scroll + size.spine as f32 / 2.0;
        let k = show_scale(case.medium);
        let (fw, fh) = (size.front as f32 * k, size.h as f32 * k);
        let margin = 40.0;
        let show_cx = slot_cx.clamp(margin + fw / 2.0, c.w as f32 - margin - fw / 2.0);
        // Held up in front of the shelf above, never over the title.
        let row_top = floor - 3.0 - self.cabinet.cab.shelves[s].height as f32;
        let show_cy = (row_top + OUT_FOOT as f32 - fh / 2.0).max(SCENE_TOP as f32 + 6.0 + fh / 2.0);
        let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
        let mv = smooth(o.clamp(0.0, 1.0));
        let turned = smooth(((o - 0.12) / 0.88).clamp(0.0, 1.0));
        let scale = 1.0 + (k - 1.0) * mv;
        let cx = slot_cx + (show_cx - slot_cx) * mv;
        let cy = (floor - size.h as f32 / 2.0) + (show_cy - (floor - size.h as f32 / 2.0)) * mv;
        let (spine, front) = self.shown_pics(case.item, case.medium);
        // Its shadow on what is behind and under it.
        let (w_now, h_now) = ((size.spine as f32 + (size.front - size.spine) as f32 * turned) * scale, size.h as f32 * scale);
        art::soft_shadow(
            c,
            (cx - w_now / 2.0) as i32 + 14,
            (cy - h_now / 2.0) as i32 + 18,
            w_now as i32,
            h_now as i32,
            30,
            0.55 * mv,
            (0, scene_bottom(c.h)),
        );
        art::soft_shadow(
            c,
            (cx - w_now / 2.0) as i32,
            (cy + h_now / 2.0) as i32 - 6,
            w_now as i32,
            10,
            16,
            0.5 * mv,
            (0, scene_bottom(c.h)),
        );
        // All the way out it faces you square: the picture as it is, sharp.
        let r = if turned >= 1.0 && (scale - k).abs() < 0.001 {
            let (x, y) = ((cx - front.w as f32 / 2.0).round() as i32, (cy - front.h as f32 / 2.0).round() as i32);
            art::blit(c, &front, x, y, 256, (0, scene_bottom(c.h)));
            (x, y, front.w, front.h)
        } else {
            art::draw_turning(
                c,
                &spine,
                &front,
                size,
                Turn {
                    cx,
                    cy,
                    scale,
                    turned,
                    shade: 256,
                },
                (0, scene_bottom(c.h)),
            )
        };
        if r.2 > 0 {
            self.cabinet.drawn.push((r.0, r.1, r.2, r.3, s, i));
            if (s, i) == self.cabinet_cursor().unwrap_or((usize::MAX, 0)) {
                self.cabinet.last_out = Some((case.item, r));
            }
        }
    }

    /// The game's name and what is known about it, above the cabinet; the shelf's name and the battery above that.
    /// A shelf's name (its plate, the title line), and whether it is a sticker's.
    fn shelf_name(&self, key: ShelfKey) -> (String, bool) {
        match key {
            ShelfKey::Continue => ("Continue".to_string(), false),
            ShelfKey::Favorites => ("Favourites".to_string(), false),
            ShelfKey::System(i) => (self.systems[i].fullname.clone(), false),
            key => self.cabinet_curator_plate(key).unwrap_or_default(),
        }
    }

    fn paint_cabinet_title(&mut self, c: &mut Canvas, s: usize, item: Item) {
        let (shelf, _) = self.shelf_name(self.cabinet.cab.shelves[s].key);
        self.paint_top_line(c, Some(&shelf), pal().dust);
        let (title, _, _, meta) = self.describe(item);
        let fav = self.is_favorite(item);
        self.draw_title(c, Face::Title, 50, MARGIN, 150, c.w - 2 * MARGIN, pal().chalk, &title, true, fav);
        let meta = self.text.fit(Face::Body, 28, &meta, c.w - 2 * MARGIN);
        self.text.draw(c, Face::Body, 28, MARGIN, 192, pal().dust, &meta);
    }

    /// Starting a game: the console comes up, the case goes to it, the cartridge or disc goes in.
    fn paint_insert(&mut self, c: &mut Canvas, now: Instant) {
        // Back from the game (or it never came up): the cabinet as it was.
        if self.cabinet.insert.as_ref().is_some_and(|i| i.launched) && self.launch_at.is_none() {
            self.cabinet.insert = None;
        }
        let Some(ins) = self.cabinet.insert.as_ref() else { return };
        let (system, ff, media, front, from, tint) = (ins.system.clone(), ins.ff, ins.media, ins.front.clone(), ins.from, ins.tint);
        let st = cabinet::insert_at(now.saturating_duration_since(ins.at));
        art::dim(c, 0, c.h, 256 - (st.dim * 256.0) as u32);
        // The console, up from below the screen.
        let sprite = self.console_sprite(&system);
        let (sw, sh) = sprite.as_ref().map_or((360, 120), |s| (s.w as i32, s.h as i32));
        let base = scene_bottom(c.h) - 10;
        let sx = (c.w - sw) / 2;
        let sy_final = base - sh;
        let sy = sy_final + ((1.0 - st.rise) * (sh + 160) as f32) as i32;
        if let Some(sp) = &sprite {
            c.blit_sprite(sp, sx, sy, ((1.0 - st.rise) * 200.0) as u8);
        } else {
            c.round_rect(sx, sy, sw, sh, 16, mix(0x2c2c30, 0x000000, ((1.0 - st.rise) * 200.0) as u32));
        }
        // The slot: a handheld's at its top edge, a console's on its top, a disc drive's in its middle.
        let slot_y = sy
            + match ff {
                FormFactor::HandheldCart | FormFactor::CardBox => sh / 14,
                FormFactor::ConsoleCart => sh / 3,
                _ => sh * 2 / 5,
            };
        let slot_cx = c.w / 2;
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let (fx, fy, fw, fh) = (from.0 as f32, from.1 as f32, from.2 as f32, from.3 as f32);
        match media {
            Media::Cart | Media::Card => {
                // The cartridge itself shrinks onto the slot and sinks in.
                let end_w = (sw as f32 * if media == Media::Card { 0.2 } else { 0.36 }).clamp(90.0, 230.0);
                let end_h = end_w * fh / fw.max(1.0);
                let w = lerp(fw, end_w, st.carry);
                let h = lerp(fh, end_h, st.carry);
                let x = lerp(fx, slot_cx as f32 - end_w / 2.0, st.carry);
                // A cartridge stays partly out of its slot, as it does in the real console.
                let depth = match ff {
                    FormFactor::ConsoleCart => 0.55,
                    FormFactor::HandheldCart => 0.65,
                    _ => 0.85,
                };
                let y = lerp(fy, slot_y as f32 - end_h, st.carry) + st.sink * end_h * depth;
                if st.rise > 0.4 {
                    c.round_rect(slot_cx - end_w as i32 / 2 - 8, slot_y - 4, end_w as i32 + 16, 9, 4, 0x0c0c0d);
                }
                let pic = front.scaled(w as i32, h as i32);
                art::blit(c, &pic, x as i32, y as i32, 256, (0, slot_y));
            }
            Media::Disc | Media::Umd => {
                // The disc slides out from behind its case, then the case fades and the disc goes to the console and in.
                let case_fade = 256 - (((st.carry - 0.3) / 0.4).clamp(0.0, 1.0) * 256.0) as u32;
                let r0 = fw.min(fh) * 0.42;
                let r1 = (sw as f32 * 0.16).clamp(46.0, 90.0);
                let out = (st.carry / 0.35).min(1.0);
                let fly = ((st.carry - 0.35) / 0.65).clamp(0.0, 1.0);
                let start = (fx + fw / 2.0 + out * fw * 0.55, fy + fh / 2.0);
                let end = (slot_cx as f32, slot_y as f32 - r1);
                let r = lerp(r0, r1, fly);
                let (x, y) = (lerp(start.0, end.0, fly), lerp(start.1, end.1, fly) + st.sink * 2.0 * r1);
                if st.rise > 0.4 {
                    c.round_rect(slot_cx - r1 as i32 - 10, slot_y - 4, 2 * r1 as i32 + 20, 9, 4, 0x0c0c0d);
                }
                let clip_bottom = if fly > 0.5 { slot_y } else { c.h };
                let draw_disc = |c: &mut Canvas| {
                    if media == Media::Umd {
                        art::umd(c, x as i32, y as i32, r as i32, tint, clip_bottom);
                    } else {
                        art::disc(c, x as i32, y as i32, r as i32, tint, clip_bottom);
                    }
                };
                // Behind the case while it comes out, in front once it is on its way.
                if fly == 0.0 {
                    draw_disc(c);
                }
                if case_fade > 0 {
                    art::blit(c, &front, from.0, from.1, case_fade, (0, c.h));
                }
                if fly > 0.0 {
                    draw_disc(c);
                }
            }
        }
    }
}

impl App {
    /// `--snapshot cabinet:<keys>[:<ms since the last key>[:<ms since A>]]`: the cabinet after the keys
    /// (r l u d, L R for L1 R1, a b for A and B), at rest, or that long into the last move, or into the start of a game.
    pub(crate) fn cabinet_snapshot(&mut self, rest: &str) {
        let mut parts = rest.split(':');
        let keys: Vec<Button> = parts
            .next()
            .unwrap_or_default()
            .chars()
            .filter_map(|k| match k {
                'r' => Some(Button::Right),
                'l' => Some(Button::Left),
                'u' => Some(Button::Up),
                'd' => Some(Button::Down),
                'L' => Some(Button::L1),
                'R' => Some(Button::R1),
                // A and B as on the shelf: Curator's boxes open their lists (and B goes back).
                'a' => Some(CONFIRM),
                'b' => Some(CANCEL),
                _ => None,
            })
            .collect();
        let pull_ms: Option<u64> = parts.next().and_then(|n| n.parse().ok());
        let insert_ms: Option<u64> = parts.next().and_then(|n| n.parse().ok());
        self.theme = super::Theme::Cabinet;
        self.cabinet.inline = true;
        // The list looks' Home has Curator and Search on top: the cabinet starts on the same entry (Continue).
        let was = self.items.get(self.sel).copied();
        self.rebuild();
        if self.screen == super::Screen::Home {
            self.sel = was.and_then(|it| self.items.iter().position(|i| *i == it)).unwrap_or_else(|| Item::first_row(&self.items));
            self.prev_sel = self.sel;
        }
        let (w, h) = super::snapshot_dims();
        let mut buf = vec![0u32; (w * h) as usize];
        let mut settle = |app: &mut App, ago: Duration| {
            app.moved_at = Instant::now() - ago;
            app.paint(&mut Canvas::new(&mut buf, w, h));
            app.cabinet.age(ago);
        };
        let rest = Duration::from_secs(2);
        settle(self, rest);
        let n = keys.len();
        for (k, b) in keys.into_iter().enumerate() {
            if b == CONFIRM || b == CANCEL {
                self.press(b);
            } else {
                self.cabinet_press(b);
            }
            self.held = None;
            let ago = pull_ms.filter(|_| k + 1 == n).map_or(rest, Duration::from_millis);
            settle(self, ago);
        }
        if let Some(ms) = insert_ms {
            settle(self, rest);
            // As if A had been pressed `ms` ago; the game itself isn't started.
            self.cabinet_begin_insert();
            let ago = Duration::from_millis(ms);
            if let Some(ins) = self.cabinet.insert.as_mut() {
                ins.at = Instant::now() - ago;
                if ago >= cabinet::LAUNCH_AT {
                    ins.launched = true;
                    self.launch_at = Some(Instant::now() - (ago - cabinet::LAUNCH_AT));
                }
            }
        }
    }
}

// ---- the Shelf look's games ----------------------------------------------------------------------------

/// A case lying in the Shelf look's stack: the template stretched to the row, where its label and band are.
#[derive(Clone)]
struct LyingPic {
    pic: Rc<Pic>,
    label: (i32, i32, i32, i32),
    band: Option<(i32, i32, i32, i32)>,
}

/// How long the case on the Shelf look's pulled-out label takes to turn its cover to you (Motion off: at once).
const SHELF_TURN: Duration = Duration::from_millis(220);
/// How far a case slides out of the stack while its label opens.
const SHELF_SLIDE: f32 = 40.0;

impl App {
    /// The case a row of the Shelf look stands for: a game's (`media`); None for a system, Curator or the like,
    /// which stay plain spines.
    pub(crate) fn shelf_medium(&self, item: Item) -> Option<Medium> {
        matches!(item, Item::Game(..) | Item::Recent(_)).then(|| self.medium(item))
    }

    /// The Shelf look's case still turning to show its cover, or covers still being read: keep painting.
    pub(crate) fn shelf_media_animating(&self, now: Instant) -> bool {
        self.cabinet.pending > 0 || (pocket_ui::motion::allowed(pocket_ui::motion::Kind::Move) && now < self.moved_at + SHELF_TURN + pocket_ui::motion::SETTLE)
    }

    /// Covers read on the thread since the last frame (the Shelf look; the Cabinet does it in its own paint).
    pub(crate) fn shelf_media_poll(&mut self) {
        self.cabinet_poll();
        // Pictures made for rows long gone.
        if self.cabinet.shelf_shown.len() > 12 {
            let frame = self.cabinet.frame;
            self.cabinet.shelf_shown.retain(|_, (_, _, _, used)| frame - *used < 4);
        }
        if self.cabinet.lying.len() > 64 {
            self.cabinet.lying.clear();
        }
        self.cabinet.frame += 1;
    }

    /// A game's row of the Shelf look while it is not pulled out (`o` below one half): its case lying in the stack,
    /// spine to you, the label in the cover's colour with the title on it. A case is as long as it is tall on the
    /// Cabinet's shelf: a DVD case spans the row, a Game Boy cartridge three fifths of it, all from the same left
    /// edge. True when the row was painted (it is a game's).
    pub(crate) fn paint_shelf_case(&mut self, c: &mut Canvas, i: usize, y: i32, thick: i32, o: f32) -> bool {
        let item = self.items[i];
        let Some(m) = self.shelf_medium(item) else { return false };
        let info = self.case_info(item);
        let look = self.case_look(&info, m);
        let width = c.w - 2 * MARGIN;
        let len = lying_len(m, width);
        let key = (m, if m.is_cart() { look.plastic } else { 0 }, len, thick);
        let lying = match self.cabinet.lying.get(&key) {
            Some(l) => l.clone(),
            None => {
                let t0 = Instant::now();
                let (pic, label, band) = media::lying(m, look.plastic, len, thick);
                self.cabinet.made("lying case", t0);
                let l = LyingPic { pic: Rc::new(pic), label, band };
                self.cabinet.lying.insert(key, l.clone());
                l
            }
        };
        // It slides out to the right as its label opens.
        let x = MARGIN + (SHELF_SLIDE * (o * 2.0).min(1.0)).round() as i32;
        // Calm: the plastic a little dimmed on the dark page, the label in a quieter shade of the cover's colour.
        art::blit(c, &lying.pic, x, y, pal().pick(180, 256), (0, c.h));
        let color = media::muted(look.color, pal().spine);
        // On the dark page no row is a bright bar: a white cover's label is a quiet grey with light letters.
        let color = if pal().pick(1, 0) == 1 && cabinet::luma(color) > 120 { mix(color, 0x000000, (cabinet::luma(color) - 120) * 2) } else { color };
        if let Some((bx, by, bw, bh)) = lying.band {
            c.rect(x + bx, y + by, bw, bh, media::muted(look.band, pal().spine));
        }
        let (lx, ly, lw, lh) = lying.label;
        let (lx, ly) = (x + lx, y + ly);
        c.rect(lx, ly, lw, lh, color);
        c.rect(lx, ly, lw, 1, mix(color, 0xffffff, 40));
        c.rect(lx, ly + lh - 1, lw, 1, mix(color, 0x000000, 60));
        let look = Look { color, ..look };
        let ink = cabinet::ink_on(look.color);
        // Its system (on Home and mixed shelves) after the case, or at the label's end when the case is long.
        let (_, _, side, _) = self.describe(item);
        let side_w = if side.is_empty() { 0 } else { self.text.width(Face::Body, 28, &side) };
        let after = x + lying.pic.w + 24;
        let mut title_w = lw - 40;
        let mid = y + thick / 2;
        if side_w > 0 {
            if after + side_w <= MARGIN + width {
                self.text.draw(c, Face::Body, 28, after, mid + pocket_ui::type_scale::base(10, 28), pal().dust, &side);
            } else {
                self.text.draw_right(c, Face::Body, 28, lx + lw - 20, mid + pocket_ui::type_scale::base(10, 28), mix(ink, look.color, 90), &side);
                title_w -= side_w + 24;
            }
        }
        // Playing, paused, completed or dropped: a small mark at the label's right end.
        let status = self.item_status(item);
        if status != pocket_ui::library_meta::Status::NotStarted {
            let right = lx + lw - 20 - if title_w < lw - 40 { side_w + 24 } else { 0 };
            pocket_ui::game_card::paint_mark(c, right - 12, mid, status, ink);
            title_w -= 44;
        }
        let fav = self.is_favorite(item);
        self.draw_title(c, Face::Title, 37, lx + 20, mid + pocket_ui::type_scale::base(13, 37), title_w, ink, &info.title, false, fav);
        true
    }

    /// The case on a game's pulled-out label (Shelf look): its front with the real cover, `max_h` high, its right
    /// edge at `right` and its middle at `cy`, turning from its spine to its front as the label opens (`SHELF_TURN`).
    /// Returns how wide it is; None for rows that are not a game.
    pub(crate) fn paint_shelf_front(&mut self, c: &mut Canvas, item: Item, right: i32, cy: i32, max_h: i32, selected: bool) -> Option<i32> {
        let m = self.shelf_medium(item)?;
        let info = self.case_info(item);
        let look = self.case_look(&info, m);
        let size = m.size();
        // As high as the label lets it, never wider than a square.
        let k = (max_h as f32 / size.h as f32).min(max_h as f32 / size.front as f32);
        let fh = (size.h as f32 * k).round() as i32;
        // A game without box art keeps its console on the label, as before.
        let file = self.cover_file(&info.system, &info.rom)?;
        let label = self.cover_label(&file, m, fh);
        let key = (CaseKey { rom: info.rom.clone(), medium: m, look }, fh);
        let frame = self.cabinet.frame;
        let made = match self.cabinet.shelf_shown.get_mut(&key) {
            Some((spine, front, has, used)) if *has || label.is_none() => {
                *used = frame;
                Some((spine.clone(), front.clone()))
            }
            _ => None,
        };
        let (spine, front) = match made {
            Some(p) => p,
            None => {
                let t0 = Instant::now();
                // No lettering on these: the spine is seen only while it turns, and until its cover is read the front is
                // the case with a label in the cover's colour (a move draws no letters of its own, tests/shelf_moves.rs).
                let spine = Rc::new(media::spine(&mut self.text, m, k, look, ""));
                let front = Rc::new(media::front(&mut self.text, m, k, look, "", label.as_deref(), None));
                self.cabinet.made("shelf front", t0);
                self.cabinet.shelf_shown.insert(key, (spine.clone(), front.clone(), label.is_some(), frame));
                (spine, front)
            }
        };
        let turned = if selected { pocket_ui::motion::progress(pocket_ui::motion::Kind::Move, self.moved_at, Instant::now(), SHELF_TURN) } else { 1.0 };
        let (fw, fh) = (front.w, front.h);
        let (x, y) = (right - fw, cy - fh / 2);
        art::soft_shadow(c, x + 6, y + 8, fw, fh, 14, 0.35 * turned, (0, c.h));
        if turned >= 1.0 {
            art::blit(c, &front, x, y, 256, (0, c.h));
        } else {
            let turn = Turn {
                cx: (right - fw / 2) as f32,
                cy: cy as f32,
                scale: k,
                turned,
                shade: 256,
            };
            art::draw_turning(c, &spine, &front, size, turn, (0, c.h));
        }
        Some(fw)
    }
}

/// How long a case lies in the Shelf look's row of `width`: as long as it is tall on the Cabinet's shelf, the
/// tallest (240 px, DVD) the whole row, the smallest (112 px, Game Boy Advance) three fifths of it.
fn lying_len(m: Medium, width: i32) -> i32 {
    let h = m.size().h.clamp(112, 240);
    width * (600 + 400 * (h - 112) / 128) / 1000
}
