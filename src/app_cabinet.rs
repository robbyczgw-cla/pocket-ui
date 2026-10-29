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
use pocket_ui::media::{self, Medium, Prop};
use pocket_ui::cabinet_art::{self as art, Look, Pic, Turn};
use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::image::{self, Image};
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{stripe, App, Button, Item, Screen, CANCEL, CONFIRM, REPEAT_DELAY};

/// The cabinet's sides on screen.
const CAB_X: i32 = 16;
/// Where the rows of the cabinet start and end; above is the title, below the hints. The bottom and the floor below
/// are measured from the screen's bottom edge, where the hints are: 992 and 800 on the Mini V2's 1080 lines, 872 and
/// 680 on the Mini V1's 960 (docs/DEVICES.md).
const SCENE_TOP: i32 = 214;
const SCENE_BOTTOM_GAP: i32 = 1080 - 992;

fn scene_bottom(h: i32) -> i32 {
    h - SCENE_BOTTOM_GAP
}

/// The cabinet's top board and its plinth.
const TOP_H: i32 = 40;
const PLINTH_H: i32 = 56;
/// The largest a case's front is shown: what fits between the title and the tallest shelf's top.
const SHOW_W: f32 = 400.0;
const SHOW_H: f32 = 354.0;

/// How much bigger a case of this kind is shown when it is out, with `room` lines for it: never taller than that, so
/// its cover is never cut (the Mini V1's 960 lines, a 720-line TV).
fn show_scale(m: Medium, room: i32) -> f32 {
    let s = m.size();
    (SHOW_W / s.front as f32).min(SHOW_H / s.h as f32).clamp(1.2, 2.1).min(room.max(40) as f32 / s.h as f32)
}

/// The showcase over the selected shelf (Alex's photo, 2026-10-28: a case held up in front of the shelf above hid
/// the spines of two shelves): from the title down to the cabinet's top board, as high as the case out of the shelf
/// needs, so the selected shelf stands whole right under it. `pitch`: the selected shelf with its board.
/// (room for the case, top of the selected shelf).
fn showcase(h: i32, m: Medium, pitch: i32) -> (i32, i32) {
    let room = scene_bottom(h) - pitch - TOP_H - SCENE_TOP - 24;
    let k = show_scale(m, room);
    let need = (m.size().h as f32 * k).round() as i32;
    (room, SCENE_TOP + 12 + need + 12 + TOP_H)
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
            art::cover_fit(i, lw, lh)
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

/// The case going into the console while the game starts (the launch animation, `App::launch_animation`).
struct Inserting {
    /// When the game was started: the animation's clock.
    at: Instant,
    item: Item,
    system: String,
    ff: FormFactor,
    media: Media,
    front: Rc<Pic>,
    from: (i32, i32, i32, i32),
    tint: u32,
    /// Cozy or Quick when it started (`App::cozy`): its length and when the cartridge is in.
    cozy: bool,
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
    /// The Shelf look's spines standing on the shelf, per case and (scale, title size), with the frame last used in.
    stand: HashMap<(CaseKey, i32), (Rc<Pic>, u64)>,
    /// The Shelf look's selected case shown above the shelf: its big spine and front per case and size, whether that
    /// front is final (has its cover, or there is none to wait for).
    shelf_shown: HashMap<(CaseKey, i32), Shown>,
    /// The Shelf look's row as laid out last, how far it is scrolled, its board, and the front as painted last
    /// (it stays lit through the launch fade).
    shelf_row: Option<ShelfRow>,
    pub(crate) shelf_scroll: Tween,
    shelf_board: Option<(i32, i32, u32, Rc<Pic>)>,
    shelf_lit: Option<(Rc<Pic>, i32, i32)>,
    /// The big spine of a case whose front is made with the next frame (`stand_front`'s `later`), the entry whose
    /// front was asked for last, and whether a front is still to come.
    shelf_spine_only: Option<((CaseKey, i32), Rc<Pic>)>,
    shelf_front_for: Option<Item>,
    shelf_waiting: bool,
    shelf_props: HashMap<(Prop, i32), Rc<Pic>>,
    /// Counts the covers' colours that came in: a spine or tile kept by the Shelf look is made again after one.
    colors_gen: u64,
    /// The room for the case out of its shelf (`showcase`), and where the selected shelf's top is, gliding with the
    /// cabinet when the room changes.
    show_room: i32,
    show_top: Tween,
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
            stand: HashMap::new(),
            shelf_shown: HashMap::new(),
            shelf_row: None,
            shelf_scroll: Tween::new(0.0, cabinet::SCROLL),
            shelf_board: None,
            shelf_lit: None,
            shelf_spine_only: None,
            shelf_front_for: None,
            shelf_waiting: false,
            shelf_props: HashMap::new(),
            show_room: 360,
            show_top: Tween::new(0.0, cabinet::SCROLL),
            colors_gen: 0,
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
        self.colors_gen += 1;
        self.cover_files.retain(|_, f| f.is_some());
        self.colors.retain(|_, c| !matches!(c, Slot::Missing));
        self.labels.retain(|_, c| !matches!(c, Slot::Missing));
    }

    /// Every glide as if it had started `by` earlier.
    fn age(&mut self, by: Duration) {
        self.scroll_y.age(by);
        self.show_top.age(by);
        for t in self.scroll_x.values_mut() {
            t.age(by);
        }
    }

    pub(crate) fn animating(&self, now: Instant) -> bool {
        self.pull.animating(now)
            || self.scroll_y.moving(now)
            || self.show_top.moving(now)
            || self.scroll_x.values().any(|t| t.moving(now))
            || self.pending > 0
            || self.insert_playing(now)
    }

    /// The launch animation is still moving (the game is loading behind it).
    pub(crate) fn insert_playing(&self, now: Instant) -> bool {
        self.insert.as_ref().is_some_and(|i| now.saturating_duration_since(i.at) < cabinet::insert_len(i.cozy))
    }

    /// The pause before the launch fade: the cartridge or disc going into the console.
    pub(crate) fn launch_delay(&self) -> Duration {
        match self.insert.as_ref() {
            Some(i) => cabinet::fade_delay(i.cozy),
            None => Duration::ZERO,
        }
    }

    /// A button while the launch animation plays: it is skipped (true when there was one to skip).
    pub(crate) fn skip_insert(&mut self, now: Instant) -> bool {
        if !self.insert_playing(now) {
            return false;
        }
        self.insert = None;
        true
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
            // A starts the game at once, as in every look; its case goes into the console while it loads
            // (`App::launch_animation`).
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

    /// The launch animation (Settings → Look & feel → Animations), started by `launch` just before the game's process:
    /// in the Cabinet and the Shelf look the case of the game being started goes into its console while the emulator
    /// loads, with a click (`Sound::Insert`, played by the sound thread) and a glow. Only when the case started from is
    /// on screen (A or a tap on it), not from the card, the switcher or the start.
    ///
    /// Quick: nothing waits for it, the game's window covers what is left of it. Cozy: a pocket-core game's window is
    /// held back until the cartridge is in and the fade has run, so the animation can be seen (`startup::HOLD_UNTIL`,
    /// at most `fade_delay + FADE` after A); the result is how long after A the window may come up, None for no hold.
    /// A game that needs longer to load than that is not held at all.
    pub(crate) fn launch_animation(&mut self, path: &std::path::Path, hosted: bool) -> Option<Duration> {
        self.cabinet.insert = None;
        if !matches!(self.theme, super::Theme::Cabinet | super::Theme::Shelf) {
            return None;
        }
        if self.card.is_some() || self.find.is_open() || self.switcher.is_some() || self.menu.is_some() || self.settings_page.is_some() || self.setup.is_some() {
            return None;
        }
        let Some(item) = self.items.get(self.sel).copied() else { return None };
        let rom = match item {
            Item::Recent(i) => self.recents.list.get(i).map(|r| r.path.clone()),
            Item::Game(s, g) => self.systems.get(s).and_then(|s| s.games.get(g)).map(|g| g.path.clone()),
            _ => None,
        };
        if rom.as_deref() != Some(path) {
            return None;
        }
        if !self.begin_insert(item) {
            return None;
        }
        // The click as it goes in (`cabinet::IN_AT`): queued now, the sound thread plays it on time and then
        // leaves the device to the game (`ui_sound::close_when_done`).
        let cozy = self.cozy();
        self.play_sound(if cozy { pocket_ui::ui_sound::Sound::InsertCozy } else { pocket_ui::ui_sound::Sound::Insert });
        pocket_ui::ui_sound::close_when_done(!hosted);
        (cozy && hosted).then(|| cabinet::fade_delay(true) + super::FADE)
    }

    /// The game box's look and its case's front `h` pixels tall (None for a cartridge, whose box is cardboard with the
    /// cover: `game_box`), made as the Shelf look makes its fronts.
    pub(crate) fn box_front(&mut self, item: Item, h: i32) -> (Medium, Look, Option<Rc<Pic>>) {
        let m = self.medium(item);
        let info = self.case_info(item);
        let look = self.case_look(&info, m);
        if pocket_ui::game_box::Hull::of(m) == pocket_ui::game_box::Hull::CartBox {
            return (m, look, None);
        }
        let k = h as f32 / m.size().h.max(1) as f32;
        let (_, front) = self.stand_front(item, m, k, false);
        (m, look, front)
    }

    /// The game ended (or never came up): the case is back on the shelf. The Shelf look paints the insert only while
    /// `launch_at` is set, so nothing else would clear it, and the played game's case stayed blank in its own list.
    pub(crate) fn end_insert(&mut self) {
        self.cabinet.insert = None;
    }

    /// The launch animation's case is on its way or in its console (until the game ends).
    pub(crate) fn cabinet_inserting(&self) -> bool {
        self.cabinet.insert.is_some()
    }

    /// The case of `item` starts towards its console, from where it was painted last; false when it wasn't.
    fn begin_insert(&mut self, item: Item) -> bool {
        if !matches!(item, Item::Recent(_) | Item::Game(..)) {
            return false;
        }
        let info = self.case_info(item);
        let (ff, medium) = (self.form_factor(item), self.medium(item));
        let look = self.case_look(&info, medium);
        let (front, from) = if self.theme == super::Theme::Cabinet {
            let Some(from) = self.cabinet.last_out.filter(|(i, _)| *i == item).map(|(_, r)| r) else { return false };
            let key = CaseKey { rom: info.rom.clone(), medium, look };
            let Some((_, front, _, _)) = self.cabinet.shown.get(&key).cloned() else { return false };
            (front, from)
        } else {
            let Some((front, x, y)) = self.cabinet.shelf_lit.clone() else { return false };
            let rect = (x, y, front.w, front.h);
            (front, rect)
        };
        self.cabinet.insert = Some(Inserting {
            at: Instant::now(),
            item,
            system: info.system,
            ff,
            media: ff.media(),
            front,
            from,
            tint: look.color,
            cozy: self.cozy(),
        });
        true
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
                self.cabinet.colors_gen += 1;
                self.cabinet.colors.insert(file, c.map_or(Slot::Missing, Slot::Ready));
            }
            (Kind::Cover(m, h), Got::Cover(img)) => {
                self.cabinet.labels.insert((file, m, h), img.map_or(Slot::Missing, |i| Slot::Ready(Rc::new(i))));
            }
            _ => {}
        }
    }

    pub(crate) fn cabinet_poll(&mut self) {
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
        // As large as the label takes (37 px at most): readable at arm's length on the Mini V2 (Alex's photo,
        // 2026-10-28: "Legend of…" at 21 px was not).
        let pic = Rc::new(media::spine_with(&mut self.text, m, 1.0, look, &info.title, 37.0));
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
        let k = show_scale(m, self.cabinet.show_room);
        let label = file.and_then(|f| self.cover_label(&f, m, (m.size().h as f32 * k).round() as i32));
        let key = CaseKey {
            rom: info.rom.clone(),
            medium: m,
            look,
        };
        let frame = self.cabinet.frame;
        let fh = (m.size().h as f32 * k).round() as i32;
        if let Some((spine, front, has, used)) = self.cabinet.shown.get_mut(&key) {
            // Made for this room (another screen size or a taller shelf makes it again).
            if (*has || label.is_none()) && front.h == fh {
                *used = frame;
                return (spine.clone(), front.clone());
            }
        }
        let sprite = pocket_ui::system_art::get(&info.system).or_else(|| emblem(item));
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

        // Where the view is: the selected shelf right under the showcase, its case in view along it.
        let cab_w = c.w - 2 * CAB_X;
        let view = cab_w - 2 * art::WALL;
        let inner_x = CAB_X + art::WALL;
        let (room, top) = showcase(c.h, self.cabinet.cab.shelves[cs].cases[ci].medium, self.cabinet.cab.pitch(cs));
        self.cabinet.show_room = room;
        if !self.cabinet.placed || !pocket_ui::motion::allowed(pocket_ui::motion::Kind::Move) {
            self.cabinet.show_top.jump(top as f32);
        } else {
            self.cabinet.show_top.set(top as f32, now);
        }
        let show_bottom = self.cabinet.show_top.value(now).round() as i32;
        let want_y = (self.cabinet.cab.y(cs) - top) as f32;
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
        // The shelves under the showcase, down to the last one that fits whole: none is cut by the hints (the photo:
        // the bottom row's spines under the hint bar).
        let mut lowest = 0;
        for s in cs..self.cabinet.cab.shelves.len() {
            let bottom = self.cabinet.cab.y(s) - sy + self.cabinet.cab.pitch(s);
            if bottom > scene_bottom(c.h) {
                break;
            }
            lowest = bottom;
        }
        // The selected shelf alone is taller than the room (a tiny screen): as much of it as fits.
        let cut = lowest <= show_bottom;
        if cut {
            lowest = scene_bottom(c.h);
        }
        let clip = (show_bottom, lowest);
        let floor_y = show_bottom + self.cabinet.cab.shelves[cs].height + 3;

        // The cabinet's top board and plinth, at its ends.
        if self.cabinet.boards.as_ref().is_none_or(|(page, b, _)| b.w != cab_w || *page != pal().void) {
            self.cabinet.walls.retain(|(w, _, _, _), _| *w == cab_w);
            self.cabinet.boards = Some((pal().void, Rc::new(art::top_board(cab_w, TOP_H)), Rc::new(art::top_board(cab_w, PLINTH_H))));
        }
        let (_, top_board, plinth) = self.cabinet.boards.clone().unwrap();
        // The top board closes the cabinet under the showcase, the plinth under the last whole shelf in view: the
        // cabinet always stands whole.
        art::blit(c, &top_board, CAB_X, show_bottom - TOP_H, 256, (0, c.h));
        if !cut {
            art::blit(c, &plinth, CAB_X, lowest, 230, (0, scene_bottom(c.h)));
        }

        let mut out: Vec<(usize, usize)> = Vec::new();
        // A bookend, a plant or a lamp on a shelf with room beside its games: four in view at most.
        let mut props = 0;
        for s in 0..self.cabinet.cab.shelves.len() {
            let top = self.cabinet.cab.y(s) - sy;
            let (inner, pitch) = (self.cabinet.cab.shelves[s].height, self.cabinet.cab.pitch(s));
            if top + pitch < clip.0 || top > clip.1 {
                continue;
            }
            let floor = top + inner + 3;
            // The shelves away from the selected one are a little darker.
            let away = (floor - floor_y).abs() as f32;
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
            if props < 4 {
                props += self.paint_prop(c, s, inner_x, view, scroll, floor, shade, clip);
            }
            self.paint_plate(c, s, inner_x + 18, floor + 5, shade, clip);
        }

        // A shelf taller than the room is cut: it fades out over the hints' edge.
        if cut {
            art::fade_band_to(c, lowest - 34, lowest, pal().void);
        }

        // The cases out of the shelf, up in the showcase: the one going back first, the selected one over it.
        out.sort_by_key(|&(s, i)| (s, i) == (cs, ci));
        for (s, i) in out {
            self.paint_out(c, s, i, sy, now, inner_x, show_bottom);
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

    /// The props of shelf `s` where there is room beside its games: a bookend right after the last case holding the
    /// row up, and the shelf's own prop (`media::Prop::beside`, the same every time) further along in the free part,
    /// well clear of the games. Never over a case, and only on a shelf that doesn't scroll. How many were painted.
    #[allow(clippy::too_many_arguments)]
    fn paint_prop(&mut self, c: &mut Canvas, s: usize, inner_x: i32, view: i32, scroll: i32, floor: i32, shade: u32, clip: (i32, i32)) -> u32 {
        let shelf = &self.cabinet.cab.shelves[s];
        if scroll != 0 || shelf.width > view || shelf.cases.is_empty() {
            return 0;
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
        let room = shelf.height - cabinet::HEADROOM / 2;
        let games_end = inner_x + shelf.width - cabinet::INSET;
        let mut painted = 0;
        let mut put = |c: &mut Canvas, pic: &Pic, x: i32| {
            let y = floor - pic.h;
            art::blit(c, pic, x, y, shade, clip);
            art::soft_shadow(
                c,
                x + pic.w,
                y + pic.h / 3,
                3,
                pic.h * 2 / 3,
                6,
                0.3 * shade as f32 / 256.0,
                (clip.0.max(y), clip.1.min(floor)),
            );
            painted += 1;
        };
        let bookend = media::Prop::Bookend.pic();
        let mut free_from = games_end + 60;
        if bookend.h <= room && games_end + 2 + bookend.w <= inner_x + view {
            put(c, bookend, games_end + 2);
            free_from += bookend.w;
        }
        if let Some(prop) = media::Prop::beside(seed, room) {
            let pic = prop.pic();
            // Somewhere in the free part of the shelf, the same place every time.
            let free = inner_x + view - 36 - pic.w - free_from;
            if free >= 0 {
                put(c, pic, free_from + (seed >> 3) as i32 % (free + 1).min(240));
            }
        }
        painted
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
    #[allow(clippy::too_many_arguments)]
    fn paint_out(&mut self, c: &mut Canvas, s: usize, i: usize, sy: i32, now: Instant, inner_x: i32, show_bottom: i32) {
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
        let k = show_scale(case.medium, self.cabinet.show_room);
        let (fw, fh) = (size.front as f32 * k, size.h as f32 * k);
        let margin = 40.0;
        let show_cx = slot_cx.clamp(margin + fw / 2.0, c.w as f32 - margin - fw / 2.0);
        // Up in the showcase over its shelf, standing on the cabinet's top board: nothing of the shelves under it.
        let show_cy = (show_bottom - TOP_H - 12) as f32 - fh / 2.0;
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
    pub(crate) fn paint_insert(&mut self, c: &mut Canvas, now: Instant) {
        // Back from the game (or it never came up): the cabinet as it was.
        if self.launch_at.is_none() {
            self.cabinet.insert = None;
        }
        let Some(ins) = self.cabinet.insert.as_ref() else { return };
        let (system, ff, media, front, from, tint) = (ins.system.clone(), ins.ff, ins.media, ins.front.clone(), ins.from, ins.tint);
        let st = cabinet::insert_at(now.saturating_duration_since(ins.at), ins.cozy);
        art::dim(c, 0, c.h, 256 - (st.dim * 256.0) as u32);
        // The Shelf look has no scene of its own: the console stands on the screen's floor.
        let floor = if self.theme == super::Theme::Cabinet { scene_bottom(c.h) } else { c.h - 110 };
        // The console, up from below the screen.
        let sprite = self.console_sprite(&system);
        let (sw, sh) = sprite.as_ref().map_or((360, 120), |s| (s.w as i32, s.h as i32));
        let base = floor - 10;
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
        // It is in: the slot lights up for a moment, in the label's colour lifted towards white.
        if st.glow > 0.0 {
            let light = mix(tint, 0xfff4d8, 170);
            art::glow(c, slot_cx, slot_y, (sw / 3).max(90), (sh / 5).max(40), light, st.glow * 0.8);
            art::glow(c, slot_cx, slot_y, (sw / 8).max(36), (sh / 14).max(14), 0xffffff, st.glow * 0.7);
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
            self.launch_animation_snapshot(Duration::from_millis(ms));
        }
    }

    /// The launch animation `ago` after A, for a snapshot: the game itself isn't started.
    pub(crate) fn launch_animation_snapshot(&mut self, ago: Duration) {
        let Some(item) = self.items.get(self.sel).copied() else { return };
        if self.begin_insert(item) {
            if let Some(ins) = self.cabinet.insert.as_mut() {
                ins.at = Instant::now() - ago;
            }
            self.launch_at = Some(Instant::now() - ago);
        }
    }
}

// ---- the Shelf look ----------------------------------------------------------------------------------------

/// How long the selected case takes to turn its cover to you (Motion off: at once).
const SHELF_TURN: Duration = Duration::from_millis(220);
/// How wide a tile of the Shelf look's row is (`shelf_tile`).
const TILE_W: i32 = 512;
/// The tallest case on a shelf in template pixels (the DVD cases): the shelf's room is made for it.
const TALLEST: f32 = 120.0;
/// The most template pixels a screen pixel the shown front gets: past that it is a blurry close-up, not a case.
const SHOW_MAX: f32 = 6.0;

/// Where the Shelf look's parts are on a screen (docs/DESIGN.md "Shelf look").
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct ShelfLayout {
    /// Screen pixels per template pixel of the spines on the shelf, in half steps so the pixel art stays even.
    pub(crate) f: f32,
    /// Where every case stands: the board's top edge.
    pub(crate) floor: i32,
    pub(crate) board_h: i32,
    /// The shelf's ends: the spines are painted between them.
    pub(crate) left: i32,
    pub(crate) right: i32,
    /// How far the selected spine rises out of the row.
    pub(crate) lift: i32,
    /// The room above the shelf where the selected case is shown turned to its front: x, y, w, h.
    pub(crate) show: (i32, i32, i32, i32),
}

/// The layout for a screen `w` x `h`: the shelf over the hints, the tallest case (a DVD case) about two fifths of the
/// room between the top line and the hints, the selected case's front above the tallest case of this row (`tallest`,
/// template pixels): a shelf of cartridges leaves the front more room. Every part stays inside the screen at any
/// size from 640x480 up (tests below).
pub(crate) fn shelf_layout(w: i32, h: i32, tallest: f32) -> ShelfLayout {
    let top = if h < 600 { 80 } else { 104 };
    // The hint pills start 98 px above the bottom edge (`console_page::paint_hints`).
    let bottom = h - 112;
    let content = (bottom - top).max(200);
    let f = ((content as f32 * 0.42 / TALLEST * 2.0).floor() / 2.0).clamp(1.0, 4.0);
    let board_h = (8.0 * f).round() as i32;
    let floor = bottom - board_h;
    let lift = (10.0 * f).round() as i32;
    let show_bottom = floor - (tallest.min(TALLEST) * f).round() as i32 - lift - 16;
    ShelfLayout {
        f,
        floor,
        board_h,
        left: MARGIN,
        right: w - MARGIN,
        lift,
        show: (MARGIN, top, w - 2 * MARGIN, (show_bottom - top).max(40)),
    }
}

/// The picture printed on the front of a box that is not a game (a game shows its console; `ui_art`): Curator's crate
/// of cartridges, Surprise me's gift (until its pick's cover is read), the monitor of PC streaming, a sticker's sheet,
/// and from the empty pages' vignettes Favourites' star on its cushion, the search's glass, a smart shelf's row of
/// cartridges and the history's notebook.
fn emblem(item: Item) -> Option<&'static pocket_ui::system_art::Sprite> {
    use pocket_ui::ui_art::{get, Art, Emblem, Sticker};
    get(match item {
        Item::Curator => Art::Emblem(Emblem::Curator),
        Item::Surprise => Art::Emblem(Emblem::Surprise),
        Item::Streaming => Art::Emblem(Emblem::Streaming),
        Item::Sticker(_) => Art::Emblem(Emblem::Sticker),
        Item::Favorites => Art::NoFavorites,
        Item::Search | Item::Lens => Art::Search,
        Item::SmartShelf(_) => Art::Sticker(Sticker::TenGames),
        Item::History | Item::Review(_) => Art::History,
        Item::Recent(_) | Item::Game(..) | Item::System(_) => return None,
    })
}

/// The `k` (`media::front`) of a case of `m` shown above the shelf: as large as the room lets it, at most 45 % of the
/// room's width and `SHOW_MAX` template pixels a pixel, in half steps; below one (a tiny screen) as large as fits.
pub(crate) fn show_k(lay: &ShelfLayout, m: Medium) -> f32 {
    let (fw, fh) = {
        let t = m.template_size();
        (t.front, t.h)
    };
    let s = (lay.show.3 as f32 / fh as f32).min(lay.show.2 as f32 * 0.45 / fw as f32);
    let s = if s >= 1.0 { ((s * 2.0).floor() / 2.0).min(SHOW_MAX) } else { s.max(0.2) };
    s / media::SHELF_SCALE
}

/// The spines' title size at scale `f` for the text size (`media::spine_px` picks the size that fits the label): 30 px
/// on the Mini V2 in Normal, 37 in Large.
fn spine_want(f: f32) -> f32 {
    pocket_ui::type_scale::px(24) as f32 * f / 2.4
}

/// Which group of Home an entry belongs to: the shelf leaves a wider gap where one ends (Curator and Search, the recent
/// games, Favorites and the pinned rows, the systems).
fn shelf_group(item: Item) -> u8 {
    match item {
        Item::Curator | Item::Search => 0,
        Item::Recent(_) => 1,
        Item::System(_) => 3,
        Item::Game(..) => 4,
        _ => 2,
    }
}

/// The Shelf look's row as laid out last: for these entries at this scale, each spine's place along the shelf, its
/// width and its case; and how long the row is.
pub(crate) struct ShelfRow {
    items: Vec<Item>,
    f: f32,
    /// The spines' title size (`media::spine_px`'s `want`): Settings → Text size.
    want: f32,
    /// Each entry's spine as painted last, with the colours' generation it was made in (`CabinetUi::colors_gen`): a
    /// frame copies it without looking anything up.
    pics: Vec<Option<(u64, Rc<Pic>)>>,
    /// The row painted in tiles (`shelf_tile`), with the colours' generation and the page colour they were made for.
    tiles: HashMap<i32, (u64, u32, Rc<Vec<u32>>)>,
    spines: Vec<(i32, i32, Medium)>,
    len: i32,
    /// The tallest case on it, in template pixels.
    tallest: f32,
}

impl App {
    /// The case an entry of the Shelf look stands in: a game's own (`media::Medium::of`); a plain DVD-size box for a
    /// system, Curator, Favorites and the rest, in their colours.
    pub(crate) fn shelf_medium(&self, item: Item) -> Medium {
        if matches!(item, Item::Game(..) | Item::Recent(_)) {
            self.medium(item)
        } else {
            Medium::Keep
        }
    }

    /// Covers still being read, the selected case still turning or the shelf still gliding: keep painting.
    pub(crate) fn shelf_media_animating(&self, now: Instant) -> bool {
        self.cabinet.pending > 0
            || self.cabinet.shelf_waiting
            || self.cabinet.shelf_scroll.moving(now)
            || (pocket_ui::motion::allowed(pocket_ui::motion::Kind::Move) && now < self.moved_at + SHELF_TURN + pocket_ui::motion::SETTLE)
    }

    /// Covers read on the thread since the last frame, and pictures made for entries long gone.
    pub(crate) fn shelf_media_poll(&mut self) {
        self.cabinet_poll();
        let frame = self.cabinet.frame;
        if self.cabinet.shelf_shown.len() > 12 {
            self.cabinet.shelf_shown.retain(|_, (_, _, _, used)| frame - *used < 4);
        }
        if self.cabinet.stand.len() > 400 {
            self.cabinet.stand.retain(|_, (_, used)| frame - *used < 60);
        }
        if self.cabinet.labels.len() > 48 {
            self.cabinet.labels.clear();
        }
        self.cabinet.frame += 1;
    }

    /// What tells an entry's pictures apart: a game by its ROM, the rest (which have none) by what they are.
    fn shelf_key(&mut self, item: Item, m: Medium) -> (Info, CaseKey) {
        let mut info = self.case_info(item);
        let look = self.case_look(&info, m);
        if info.rom.as_os_str().is_empty() {
            info.rom = PathBuf::from(format!("shelf/{item:?}"));
        }
        let key = CaseKey {
            rom: info.rom.clone(),
            medium: m,
            look,
        };
        (info, key)
    }

    /// An entry's spine standing on the shelf at `f` screen pixels a template pixel, its title at the text size's
    /// spine size (`media::spine_px`); a system's spine carries its short name ("SNES"). Made once and kept.
    fn stand_pic(&mut self, item: Item, m: Medium, f: f32) -> Rc<Pic> {
        let want = spine_want(f);
        let (info, key) = self.shelf_key(item, m);
        let key = (key, (f * 100.0) as i32 * 1000 + want as i32);
        let frame = self.cabinet.frame;
        if let Some((pic, used)) = self.cabinet.stand.get_mut(&key) {
            *used = frame;
            return pic.clone();
        }
        let title = match item {
            Item::System(s) => super::system_short(&self.systems, &self.systems[s].name),
            _ => info.title.clone(),
        };
        let t0 = Instant::now();
        let pic = Rc::new(media::spine_with(&mut self.text, m, f / media::SHELF_SCALE, key.0.look, &title, want));
        self.cabinet.made("standing spine", t0);
        self.cabinet.stand.insert(key, (pic.clone(), frame));
        pic
    }

    /// The big spine and front of the selected entry, `k` times its shelf size (`show_k`): the front with the cover in
    /// its label; a game without one, a system (its console) and the rest get a printed title card instead. While a
    /// cover is still being read the label stays plain (a move draws no letters of its own, tests/shelf_moves.rs).
    /// `later`: the case has only just started to turn (the first frame on it), so its spine is all there is to see; a
    /// front not made yet is then left for the next frame (None), and the first frame after a move costs about what
    /// the list looks' does (tests/shelf_moves.rs).
    fn stand_front(&mut self, item: Item, m: Medium, k: f32, later: bool) -> (Rc<Pic>, Option<Rc<Pic>>) {
        let (info, key) = self.shelf_key(item, m);
        let file = match (item, self.surprise_front()) {
            (Item::Surprise, Some((system, rom))) => self.cover_file(&system, &rom),
            _ => self.cover_file(&info.system, &info.rom),
        };
        let fh = (m.size().h as f32 * k).round() as i32;
        let label = file.as_ref().and_then(|f| self.cover_label(f, m, fh));
        let reading = label.is_none()
            && file
                .as_ref()
                .is_some_and(|f| matches!(self.cabinet.labels.get(&(f.clone(), m, fh)), Some(Slot::Pending)));
        let key = (key, (k * 1000.0) as i32);
        let frame = self.cabinet.frame;
        if let Some((spine, front, done, used)) = self.cabinet.shelf_shown.get_mut(&key) {
            if *done || reading {
                *used = frame;
                return (spine.clone(), Some(front.clone()));
            }
        }
        if later {
            if let Some((k2, spine)) = &self.cabinet.shelf_spine_only {
                if *k2 == key {
                    return (spine.clone(), None);
                }
            }
            let spine = Rc::new(media::spine(&mut self.text, m, k, key.0.look, ""));
            self.cabinet.shelf_spine_only = Some((key, spine.clone()));
            return (spine, None);
        }
        let sprite = pocket_ui::system_art::get(&info.system).or_else(|| emblem(item));
        let title = if reading { "" } else { info.title.as_str() };
        let t0 = Instant::now();
        let spine = Rc::new(media::spine(&mut self.text, m, k, key.0.look, ""));
        let front = Rc::new(media::front(
            &mut self.text,
            m,
            k,
            key.0.look,
            title,
            label.as_deref(),
            if reading { None } else { sprite },
        ));
        self.cabinet.made("shown front", t0);
        self.cabinet.shelf_shown.insert(key, (spine.clone(), front.clone(), !reading, frame));
        (spine, Some(front))
    }

    /// The row laid out again when the entries or the scale changed; the shelf then jumps to the selection.
    fn shelf_row(&mut self, f: f32) -> bool {
        let want = spine_want(f);
        if self
            .cabinet
            .shelf_row
            .as_ref()
            .is_some_and(|r| r.f == f && r.want == want && r.items == self.items)
        {
            return false;
        }
        let gap = (2.0 * f).round() as i32;
        let group_gap = (12.0 * f).round() as i32;
        let mut spines = Vec::with_capacity(self.items.len());
        let (mut x, mut tallest) = (0, 0.0f32);
        for (i, &item) in self.items.iter().enumerate() {
            if i > 0 && shelf_group(item) != shelf_group(self.items[i - 1]) {
                x += group_gap;
            }
            let m = self.shelf_medium(item);
            let w = (m.template_size().spine as f32 * f).round() as i32;
            tallest = tallest.max(m.template_size().h as f32);
            spines.push((x, w, m));
            x += w + gap;
        }
        self.cabinet.shelf_row = Some(ShelfRow {
            items: self.items.clone(),
            f,
            want,
            pics: vec![None; spines.len()],
            tiles: HashMap::new(),
            spines,
            len: (x - gap).max(0),
            tallest,
        });
        true
    }

    /// The shelf board, as wide as the shelf (walnut on the dark page, oak on the light one).
    fn shelf_board(&mut self, w: i32, h: i32) -> Rc<Pic> {
        match &self.cabinet.shelf_board {
            Some((bw, bh, page, pic)) if (*bw, *bh, *page) == (w, h, pal().void) => pic.clone(),
            _ => {
                let pic = Rc::new(art::top_board(w, h));
                self.cabinet.shelf_board = Some((w, h, pal().void, pic.clone()));
                pic
            }
        }
    }

    /// Shelf: every entry stands on one shelf as its case's spine (`media`), the same light and baseline for all,
    /// only the case's shape changing; the selected one rises out of the row and is shown large above the shelf,
    /// turned to its front with the cover, its name and what is known about it beside it.
    pub(crate) fn paint_shelf(&mut self, c: &mut Canvas) {
        self.shelf_media_poll();
        self.cabinet.shelf_lit = None;
        let n = self.items.len();
        if n == 0 {
            return;
        }
        // The spines' scale is the screen's; the room above them follows the row's tallest case.
        let f = shelf_layout(c.w, c.h, TALLEST).f;
        let now = Instant::now();
        let new_row = self.shelf_row(f);
        let Some(mut row) = self.cabinet.shelf_row.take() else { return };
        let lay = shelf_layout(c.w, c.h, row.tallest);
        let sel = self.sel.min(n - 1);
        let view = lay.right - lay.left;

        // The shelf glides so that the selection keeps a fifth of the shelf beside it; a row that fits stands still.
        let max_scroll = (row.len - view).max(0) as f32;
        let (sx, sw, sm) = row.spines[sel];
        let ctx = (view / 5) as f32;
        let mut want = self.cabinet.shelf_scroll.target();
        if (sx as f32) - want < ctx {
            want = sx as f32 - ctx;
        }
        if (sx + sw) as f32 - want > view as f32 - ctx {
            want = (sx + sw) as f32 - view as f32 + ctx;
        }
        let want = want.clamp(0.0, max_scroll).round();
        if new_row || !pocket_ui::motion::allowed(pocket_ui::motion::Kind::Move) {
            self.cabinet.shelf_scroll.jump(want);
        } else {
            self.cabinet.shelf_scroll.set(want, now);
        }
        let scroll = self.cabinet.shelf_scroll.value(now).round() as i32;

        // The board, and the spines standing on it.
        let board = self.shelf_board(view + 32, lay.board_h);
        art::blit(c, &board, lay.left - 16, lay.floor, 256, (0, c.h));
        if pal().void != 0 {
            art::soft_shadow(c, lay.left - 16, lay.floor + lay.board_h, view + 32, 10, 12, 0.5, (0, c.h));
        }
        // The row is painted into tiles once (spines, their shadows, the props); a frame copies what is in view and
        // paints only the lifted spines on their own (a move costs no more than the list looks', tests/shelf_moves.rs).
        let shadows = pal().void != 0;
        let room = (row.tallest * lay.f).round() as i32;
        let strip = lay.floor - room;
        let seed = (n as u32).wrapping_mul(31) ^ self.items.first().map_or(0, |i| shelf_group(*i) as u32);
        for t in scroll / TILE_W..=(scroll + view - 1) / TILE_W {
            let tile = self.shelf_tile(&mut row, t, room, view, seed);
            let (a, b) = ((t * TILE_W).max(scroll), ((t + 1) * TILE_W).min(scroll + view));
            let dx = lay.left + a - scroll;
            for yy in 0..room {
                let cy = strip + yy;
                if (0..c.h).contains(&cy) && b > a {
                    let src = &tile[(yy * TILE_W + a - t * TILE_W) as usize..][..(b - a) as usize];
                    c.buf[(cy * c.w + dx) as usize..][..(b - a) as usize].copy_from_slice(src);
                }
            }
        }
        for i in 0..n {
            let (x0, w, m) = row.spines[i];
            let x = lay.left + x0 - scroll;
            if x + w <= lay.left {
                continue;
            }
            if x >= lay.right {
                break;
            }
            let h = (m.template_size().h as f32 * lay.f).round() as i32;
            let rise = (self.openness(i) * lay.lift as f32).round() as i32;
            if rise > 0 {
                // Out of its place in the tile, up out of the row.
                let pic = self.row_pic(&mut row, i, lay.f);
                let (hx, hw) = (x.max(lay.left), (x + w).min(lay.right) - x.max(lay.left));
                c.rect(hx, lay.floor - h, hw, h, pal().void);
                self.blit_between(c, &pic, x, lay.floor - pic.h - rise, 256, (0, lay.floor), lay.left, lay.right);
                if shadows {
                    art::soft_shadow(c, x, lay.floor - 4, pic.w, 6, 6, 0.4 * rise as f32 / lay.lift as f32, (0, c.h));
                }
            }
            let (hx, hw) = (x.max(lay.left), (x + w).min(lay.right) - x.max(lay.left));
            self.hits.row(Area::new(hx, lay.floor - h - rise, hw, h + rise), i);
        }
        // Where the row goes on past an end, it fades into the page.
        if scroll > 0 {
            fade_cols(c, lay.left + 48, lay.left, lay.floor - (TALLEST * lay.f) as i32 - lay.lift, lay.floor);
        }
        if (scroll as f32) < max_scroll {
            fade_cols(c, lay.right - 48, lay.right, lay.floor - (TALLEST * lay.f) as i32 - lay.lift, lay.floor);
        }
        let pitch = row.len as f32 / n as f32;
        self.cabinet.shelf_row = Some(row);

        // The selected case, large, turned to its front.
        let item = self.items[sel];
        let k = show_k(&lay, sm);
        let turned = pocket_ui::motion::progress(pocket_ui::motion::Kind::Move, self.moved_at, now, SHELF_TURN);
        // The first frame after a move, while the case turns: its spine is what there is to see, its front can wait a
        // frame (with Motion off, and on a screen's first frame, it is there at once).
        let first = matches!(self.cabinet.shelf_front_for.replace(item), Some(was) if was != item);
        let (spine, front) = self.stand_front(item, sm, k, first && turned < 1.0);
        self.cabinet.shelf_waiting = front.is_none();
        let (sx, sy, sw, sh) = lay.show;
        let (fw, fh) = media::front_size(sm, k);
        // Low in the room, nearer the shelf it came from than the top line.
        let (fx, fy) = (sx, sy + (sh - fh) * 2 / 3);
        let show_clip = (sy, sy + sh);
        if shadows && front.is_some() {
            art::soft_shadow(c, fx + 10, fy + 12, fw, fh, 24, 0.45 * turned, (0, lay.floor));
        }
        match front {
            Some(front) if turned >= 1.0 => {
                // On its way into the console (`paint_insert`): not here as well.
                if !self.cabinet.insert.as_ref().is_some_and(|i| i.item == item) {
                    art::blit(c, &front, fx, fy, 256, show_clip);
                }
                self.cabinet.shelf_lit = Some((front, fx, fy));
            }
            Some(front) => {
                let turn = Turn {
                    cx: (fx + fw / 2) as f32,
                    cy: (fy + fh / 2) as f32,
                    scale: k,
                    turned,
                    shade: 256,
                };
                art::draw_turning(c, &spine, &front, sm.size(), turn, show_clip);
            }
            // Its spine to you, as at the start of the turn: the front comes with the next frame.
            None => art::blit(c, &spine, fx + (fw - spine.w) / 2, fy + (fh - spine.h) / 2, 256, show_clip),
        }
        self.hits.row(Area::new(fx, fy, fw, fh), sel);
        self.motion.sel_y = Some(fy);
        self.hits.list(sel, n, pitch, 0.0);

        // Beside it, from its top edge: what it is.
        let tx = fx + fw + 48;
        self.paint_shelf_words(c, item, tx, fy.max(sy), sx + sw - tx, sy + sh);
    }

    /// The selected entry's words beside its front, from `top` down to `bottom`: "Continue" over the last game played,
    /// the name in two lines at most, the meta line, and a system's fact sheet or a Curator or Favorites row's covers
    /// under them.
    fn paint_shelf_words(&mut self, c: &mut Canvas, item: Item, x: i32, top: i32, w: i32, bottom: i32) {
        use pocket_ui::type_scale::{line, px};
        if w < 120 {
            return;
        }
        let (title, _, _, meta) = self.describe(item);
        let mut panel = self.home_panel(item);
        let kicker = if self.screen == Screen::Home && item == Item::Recent(0) {
            Some("Continue")
        } else {
            panel.as_ref().and_then(|p| p.kicker)
        };
        let mut y = top;
        if let Some(kicker) = kicker {
            y += px(28) as i32 + 4;
            self.text.draw(c, Face::Body, 28, x, y, pal().dust, kicker);
            y += 10;
        }
        let fav = self.is_favorite(item);
        let lines = self.text.wrap_at_most(Face::Title, 67, &title, w - 40, 2);
        let last = lines.len().saturating_sub(1);
        for (k, l) in lines.iter().enumerate() {
            y += line(67, 0) - 6;
            self.draw_title(c, Face::Title, 67, x, y, w, pal().chalk, l, false, fav && k == last);
        }
        // A system's short facts (its headline, Released, Sold, Inside…) under its name, as Split's panel has them.
        if matches!(item, Item::System(_)) && self.paint_system_facts(c, item, x, y + 48, w, bottom) {
            return;
        }
        let meta = match &panel {
            // A panel's own line (the day's pick and why; how to fill an empty list), shortened by its parts.
            Some(p) => self.panel_line(&p.line, w * 2),
            None => meta,
        };
        if !meta.is_empty() {
            y += 10;
            // Playing, paused, completed or dropped: its mark in front of the line.
            let status = self.item_status(item);
            let indent = if status != pocket_ui::library_meta::Status::NotStarted { 40 } else { 0 };
            for (k, l) in self.text.wrap_at_most(Face::Body, 28, &meta, w - indent, 2).iter().enumerate() {
                y += line(28, 10);
                if y > bottom {
                    return;
                }
                if k == 0 && indent > 0 {
                    pocket_ui::game_card::paint_mark(c, x + 12, y - px(28) as i32 * 7 / 20, status, pal().dust);
                }
                self.text.draw(c, Face::Body, 28, x + indent, y, pal().dust, l);
            }
        }
        if let Some(panel) = panel.as_mut() {
            let h = (bottom - y - 28).min(super::LABEL_ART_H);
            if h > 80 {
                let ink = super::app_home_panel::Ink {
                    caption: pal().dust,
                    under: pal().spine,
                };
                self.panel_make_room(panel, w, h, w);
                self.paint_home_panel(c, panel, (x, y + 28, w, h), 0.0, &ink);
            }
        }
    }

    /// Entry `i`'s spine on the row, kept with the colours' generation it was made in: no look-up while it holds.
    fn row_pic(&mut self, row: &mut ShelfRow, i: usize, f: f32) -> Rc<Pic> {
        let gen = self.cabinet.colors_gen;
        match &row.pics[i] {
            Some((g, pic)) if *g == gen => pic.clone(),
            _ => {
                let pic = self.stand_pic(row.items[i], row.spines[i].2, f);
                row.pics[i] = Some((gen, pic.clone()));
                pic
            }
        }
    }

    /// Tile `t` of the row (`TILE_W` wide from `t * TILE_W` along it, `room` high down to the board, on the page's
    /// colour): the spines standing in it with their shadows, and where the row is shorter than the shelf (`view`)
    /// the props after its end. Made again when covers' colours came in or the page changed (Appearance).
    fn shelf_tile(&mut self, row: &mut ShelfRow, t: i32, room: i32, view: i32, seed: u32) -> Rc<Vec<u32>> {
        let (gen, page) = (self.cabinet.colors_gen, pal().void);
        if let Some((g, p, tile)) = row.tiles.get(&t) {
            if (*g, *p) == (gen, page) {
                return tile.clone();
            }
        }
        if row.tiles.len() > 8 {
            row.tiles.clear();
        }
        let t0 = Instant::now();
        let mut buf = vec![page; (TILE_W * room) as usize];
        {
            let mut c = Canvas::new(&mut buf, TILE_W as u32, room as u32);
            let x0 = t * TILE_W;
            for i in 0..row.spines.len() {
                let (x, w, _) = row.spines[i];
                if x + w + 8 <= x0 || x >= x0 + TILE_W {
                    continue;
                }
                let pic = self.row_pic(row, i, row.f);
                let (x, y) = (x - x0, room - pic.h);
                art::blit(&mut c, &pic, x, y, 256, (0, room));
                // A little shadow on the right of each case; on the black page there is nothing for it to darken.
                if page != 0 {
                    art::soft_shadow(&mut c, x + pic.w, y + 4, 3, pic.h - 4, 5, 0.3, (0, room));
                }
            }
            if view - row.len > 80 {
                self.paint_shelf_props(&mut c, row.len - x0, view - x0, room, row.f, seed);
            }
        }
        self.cabinet.made("shelf tile", t0);
        let tile = Rc::new(buf);
        row.tiles.insert(t, (gen, page, tile.clone()));
        tile
    }

    /// A bookend right after the last case at `end`, and one of the other props (`media::Prop::beside`, picked by
    /// `seed`, the same every time) where there is room further along before `right`; at the shelf's scale `f`,
    /// standing on the bottom of `c` (`room` high).
    fn paint_shelf_props(&mut self, c: &mut Canvas, end: i32, right: i32, room: i32, f: f32, seed: u32) {
        let mut x = end + 6;
        let mut picks = vec![Prop::Bookend];
        picks.extend(Prop::beside(seed, room * 2 / 3));
        for (k, prop) in picks.into_iter().enumerate() {
            let pic = self.prop_pic(prop, f);
            if k > 0 {
                x += ((right - x - pic.w) / 2).max(0);
            }
            if x + pic.w > right || pic.h > room {
                return;
            }
            art::blit(c, &pic, x, room - pic.h, 256, (0, room));
            x += pic.w;
        }
    }

    /// A prop at the shelf's scale `f` (`Prop::pic` is at `media::SHELF_SCALE`), made once.
    fn prop_pic(&mut self, prop: Prop, f: f32) -> Rc<Pic> {
        let key = (prop, (f * 100.0) as i32);
        if let Some(p) = self.cabinet.shelf_props.get(&key) {
            return p.clone();
        }
        let p = Rc::new(media::rescaled(prop.pic(), f / media::SHELF_SCALE));
        self.cabinet.shelf_props.insert(key, p.clone());
        p
    }

    /// Where the letter of an L2/R2 jump stands in the Shelf look (`app_search::paint_letter_flash`, `size` high): over
    /// the middle of the spines, above the hints; None in the other looks.
    pub(crate) fn shelf_flash_top(&self, w: i32, h: i32, size: i32) -> Option<i32> {
        if self.theme != super::Theme::Shelf {
            return None;
        }
        let tallest = self.cabinet.shelf_row.as_ref().map_or(TALLEST, |r| r.tallest);
        let lay = shelf_layout(w, h, tallest);
        let middle = lay.floor - (tallest * lay.f) as i32 / 2;
        Some((middle - size / 2).min(h - 112 - size).max(0))
    }

    /// The launch fade: the selected case's front stays lit while the rest of the shelf goes dark.
    pub(crate) fn shelf_relight(&self, c: &mut Canvas) {
        if let Some((pic, x, y)) = &self.cabinet.shelf_lit {
            art::blit(c, pic, *x, *y, 256, (0, c.h));
        }
    }
}

/// The page colour rising over columns `x0..x1` (none at `x0`, all at `x1`) of rows `y0..y1`: where the shelf goes
/// on past the screen.
fn fade_cols(c: &mut Canvas, x0: i32, x1: i32, y0: i32, y1: i32) {
    let page = pal().void;
    let n = (x1 - x0).abs().max(1);
    let (y0, y1) = (y0.max(0), y1.min(c.h));
    for k in 0..n {
        let col = if x1 > x0 { x0 + k } else { x0 - k };
        if col < 0 || col >= c.w {
            continue;
        }
        let a = (k * 255 / n) as u32;
        let a = a * a / 255;
        for row in y0..y1 {
            let i = (row * c.w + col) as usize;
            c.buf[i] = mix(c.buf[i], page, a);
        }
    }
}

#[cfg(test)]
mod shelf_tests {
    use super::*;

    /// At every screen size pocket-ui runs on, the shelf, the lifted spine and the shown front stay inside the screen,
    /// between the top line and the hints, and the front never runs into the shelf.
    #[test]
    fn the_shelf_and_the_shown_front_fit_every_screen() {
        for (w, h) in [(1240, 1080), (1920, 1080), (1280, 960), (1280, 720), (640, 480), (1024, 768)] {
            let lay = shelf_layout(w, h, TALLEST);
            assert!(lay.floor + lay.board_h <= h - 98, "{w}x{h}: the board over the hints");
            let tallest = (TALLEST * lay.f).round() as i32;
            assert!(
                lay.floor - tallest - lay.lift >= lay.show.1 + lay.show.3,
                "{w}x{h}: the lifted DVD case under the shown front"
            );
            assert!(lay.show.1 >= 62 && lay.show.3 > 0, "{w}x{h}: under the top line");
            for m in Medium::ALL {
                let k = show_k(&lay, m);
                let (fw, fh) = media::front_size(m, k);
                assert!(fh <= lay.show.3 && fw <= lay.show.2 / 2, "{w}x{h} {m:?}: front {fw}x{fh} in {:?}", lay.show);
                assert!(
                    k * media::SHELF_SCALE >= lay.f || fh as f32 >= lay.show.3 as f32 * 0.6,
                    "{w}x{h} {m:?}: shown smaller than on the shelf"
                );
            }
        }
        let big = shelf_layout(1240, 1080, TALLEST);
        let carts = shelf_layout(1240, 1080, Medium::Snes.template_size().h as f32);
        assert!(
            carts.show.3 > big.show.3 && carts.f == big.f,
            "a shelf of cartridges: the same spines, more room for the front"
        );
        assert_eq!(big.f, 3.0, "the Mini V2: three screen pixels a template pixel");
        assert!(
            show_k(&big, Medium::Snes) * media::SHELF_SCALE >= 4.5,
            "a SNES front is shown large on the Mini V2"
        );
    }

    #[test]
    fn spine_titles_fit_their_label_and_grow_with_the_text_size() {
        // The thin jewel case's label at three pixels a template pixel (33 px) and a SNES cartridge's (48 px).
        assert!(media::spine_px(33, 30.0) as f32 * 0.86 <= 27.0);
        assert!(
            media::spine_px(48, 37.5) > media::spine_px(48, 30.0),
            "Large text, larger titles where the label has room"
        );
        assert_eq!(media::spine_px(10, 30.0), 21, "never smaller than the shelf's smallest");
    }
}
