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

use pocket_ui::cabinet::{self, Cabinet, Case, FormFactor, Pull, ShelfKey, Tween};
use pocket_ui::media::{self, ajar_peek, ajar_spine, Medium, Prop};
use pocket_ui::case_stickers::CaseStickers;
use pocket_ui::cabinet_art::{self as art, Look, Pic, Turn};
use pocket_ui::gfx::{mix, Canvas};

use pocket_ui::image::{self, Image};
use pocket_ui::insert_scene::Scene;
use pocket_ui::notice::Notice;
use pocket_ui::palette::*;
use pocket_ui::text::{Face, Text};
use pocket_ui::title_block;
use pocket_ui::touch::Area;

use super::{stripe, App, Button, Item, Screen, CANCEL, CONFIRM, REPEAT_DELAY};

/// The cabinet's sides on screen.
pub(crate) const CAB_X: i32 = 16;
/// Where the rows of the cabinet start and end; above is the title, below the hint bar. The bottom is measured from
/// the screen's bottom edge, over the bar (`kit::content_bottom`): 952 on the Mini V2's 1080 lines, 832 on the Mini
/// V1's 960 (docs/DEVICES.md).
const SCENE_TOP: i32 = 214;
/// On a screen under this many lines (1024x768, 1280x800) the title block moves up this much, and the scene with it: the
/// shelf then sinks less behind the top board (`showcase`).
const SHORT_H: i32 = media::SHORT_SCREEN_H;
const SHORT_LIFT: i32 = 18;

/// How far up the title block and the scene start on a screen `h` lines high. Under 768 lines (the Smart Pro's 720, painted
/// natively) 15 more (the title still clears the system's name over it), with `show_min_share` 0.17 there: the
/// showcase keeps 122 lines and its TV, and the SNES and Mega Drive shelves stand whole under the top board (the SNES
/// one sank 29 lines behind it).
fn lift(h: i32) -> i32 {
    if h < 768 { SHORT_LIFT + 15 } else if h < SHORT_H { SHORT_LIFT } else { 0 }
}

/// Where the bar of button hints (`paint_hint_row`) begins on a screen `h` high: a scene's console stays above it.
pub(crate) fn hint_band_top(h: i32) -> i32 {
    pocket_ui::kit::hint_bar_top(h)
}

fn scene_bottom(h: i32) -> i32 {
    // On a short screen (1024x768) the rows need the room (a SNES-box shelf would sink behind the top board): the cabinet's
    // plinth, which is only its base, ends 12 px under the hint bar's top edge there (the rows and their name tags stay over it).
    // Measured from where the bar stood before it took 960's share of the Brick's lines (`kit::chrome`, 0.6.1): the scene
    // stays as tuned (its TV and console in the Plus's share), the flatter bar leaves its plinth a little more air.
    if h < SHORT_H {
        pocket_ui::kit::design_hint_bar_top(h) + 12
    } else {
        pocket_ui::kit::content_bottom(h)
    }
}

/// The plinth can reach under the hints. Complete shelves and their name plates need the actual hint-bar edge.
fn shelves_bottom(h: i32) -> i32 {
    scene_bottom(h).min(hint_band_top(h))
}

/// The cabinet's top board and its plinth.
const TOP_H: i32 = 40;
const PLINTH_H: i32 = 56;
/// The least of the next shelf that looks in under the last whole one (less is a sliver: the plinth closes the cabinet
/// instead), and the band over which it fades into the hints' ground.
const PEEK_MIN: i32 = 72;
const PEEK_FADE: i32 = 40;
/// The largest a case's front is shown: what fits between the title and the tallest shelf's top.
const SHOW_W: f32 = 400.0;
const SHOW_H: f32 = 354.0;
const SHOW_GROW: f32 = 1.4;

/// How much bigger than the base size a showcase with `room` lines is used (the case, and the TV beside it): by the room's
/// share over `SHOW_H`, at most `SHOW_GROW`.
pub(crate) fn show_grow(room: i32) -> f32 {
    (room as f32 / SHOW_H).clamp(1.0, SHOW_GROW)
}

/// The least a case is shown at, against the scale it would get by itself: a small box (GBA) is never shown so small that
/// its cover is hard to see.
const SHOW_LEAST: f32 = 0.6;

/// How much bigger a case of this kind is shown when it is out, with `room` lines for it: never taller than that, so
/// its cover is never cut (the Mini V1's 960 lines, a 720-line TV). One scale for every case (the one the largest
/// needs), so they keep their real sizes to each other (a PS2 case taller than a GBA box), but never under `SHOW_LEAST`
/// of the case's own.
fn show_scale(m: Medium, room: i32) -> f32 {
    // A shelf with more room over it (a short shelf, or a taller screen) shows its cases bigger.
    let g = show_grow(room);
    let own = |m: Medium| {
        let s = m.size();
        (SHOW_W * g / s.front as f32).min(SHOW_H * g / s.front_h as f32)
    };
    let common = Medium::ALL.into_iter().map(own).fold(f32::MAX, f32::min);
    let s = m.size();
    // The bounds are in the normal shelf's scale: a smaller shelf (short screens) shows its case as big in pixels.
    let r = media::SHELF_SCALE / media::shelf_scale();
    common.max(own(m) * SHOW_LEAST).clamp(1.2 * r, 2.1 * g * r).min(room.max(40) as f32 / s.front_h as f32)
}

/// The showcase is never smaller than this share of the screen's height (a case needs it to be seen, and the TV beside
/// it to be there): on a short screen with a tall shelf (1024x768, 1280x800: no scaled layout, `scaled_layout`) the shelf
/// gives way, not the case (the polish review 2026-11-04: a 40-line chip and no TV on the Brick's 768 lines).
const SHOW_MIN_SHARE: f32 = 0.2;
/// `SHOW_MIN_SHARE`, and 0.17 under 768 lines (`lift`).
fn show_min_share(h: i32) -> f32 {
    if h < 768 { 0.17 } else { SHOW_MIN_SHARE }
}
/// How much of the selected shelf may sink behind the top board to make room for it: of its height, so the spines
/// keep their titles in view.
const SINK_MAX_SHARE: f32 = 0.45;

/// The showcase over the selected shelf (Alex's photo, 2026-10-28: a case held up in front of the shelf above hid
/// the spines of two shelves): from the title down to the cabinet's top board, as high as the case out of the shelf
/// needs, so the selected shelf stands whole right under it. `pitch`: the selected shelf with its board.
/// (room for the case, bottom of the top board over the selected shelf, how far the shelf's top sinks behind that
/// board). Where the room under the title is short of `SHOW_MIN_SHARE` of the height, the shelf sinks behind the board by
/// the rest (at most `SINK_MAX_SHARE` of it): its spines are cut at the top, the case keeps its room.
fn showcase(h: i32, m: Medium, pitch: i32, extra: i32) -> (i32, i32, i32) {
    let base = scene_bottom(h) - pitch - TOP_H - (SCENE_TOP - lift(h)) - if h < SHORT_H { 4 } else { 24 } + extra;
    let want = (h as f32 * show_min_share(h)).round() as i32;
    let sink = (want - base).clamp(0, (pitch as f32 * SINK_MAX_SHARE) as i32);
    let room = base + sink;
    let k = show_scale(m, room);
    let need = (m.size().front_h as f32 * k).round() as i32;
    (room, SCENE_TOP - lift(h) + 12 + need + 12 + TOP_H, sink)
}

/// How far a frozen game stands out beside its front: of its own width out of the case, at most this share of the front's
/// width (walk 2026-11-02: at 0.22 and 40 px a sliver of it read as a drawing fault, and with the title 48 px from the front
/// it almost touched the name). The name keeps clear of it by [`AJAR_FRONT`] and its own gap.
const AJAR: f32 = 0.5;
const AJAR_FRONT: f32 = 0.45;
/// Easy: the medium stands this much of the case's width out, whatever the medium is.
const AJAR_BIG: f32 = 0.17;
/// The time it takes to slide out with the case's turn (shared with Android's way back).
use pocket_ui::eject::AJAR_IN;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Kind {
    /// The cover's colours for a case of that medium (taken without its system's printed strip, `media::trim_system_band`:
    /// a US N64 box's red one would make every N64 spine red).
    Color(Medium),
    /// The cover cut for a front of that medium this many pixels high.
    Cover(Medium, i32),
    /// ScreenScraper's photo of the real spine (`boxsides/`), stood up and kept small (`media::upright_spine`).
    Spine,
    /// ScreenScraper's logo of a game (`wheels/`) for its series' spine, cut, stood up and kept small
    /// (`series_look::upright_logo`).
    Logo,
}

/// What the reader thread brings back.
enum Got {
    /// The cover's main colour and its accent (`media::cover_colors`).
    Color(Option<(u32, u32)>),
    Cover(Option<Image>),
    Spine(Option<Image>),
    Logo(Option<Pic>),
}

#[derive(Clone)]
enum Slot<T> {
    Pending,
    Ready(T),
    Missing,
}

/// The reader thread: covers asked for first, spine colours after, one at a time; then the covers the selection is
/// heading for (`soon`, `App::cover_prefetch`), when nothing else waits and no game runs.
#[derive(Clone)]
struct Reader {
    queue: Arc<(Mutex<Jobs>, Condvar)>,
    /// What was read, and whether it was a prefetch (not counted in `pending`).
    done: Arc<Mutex<Vec<(PathBuf, Kind, Got, bool)>>>,
}

#[derive(Default)]
struct Jobs {
    now: VecDeque<(PathBuf, Kind)>,
    soon: VecDeque<(PathBuf, Kind)>,
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
                    let (job, soon) = {
                        let (lock, cv) = &*worker.queue;
                        let mut q = lock.lock().unwrap();
                        loop {
                            if pocket_ui::background_io::paused() {
                                q = if q.now.is_empty() && q.soon.is_empty() {
                                    cv.wait(q).unwrap()
                                } else {
                                    cv.wait_timeout(q, Duration::from_millis(150)).unwrap().0
                                };
                                continue;
                            }
                            if let Some(job) = q.now.pop_front() {
                                break (job, false);
                            }
                            if let Some(job) = q.soon.pop_front() {
                                break (job, true);
                            }
                            q = cv.wait(q).unwrap();
                        }
                    };
                    if let Some(got) = pocket_ui::background_io::job(|| read(&job.0, job.1, None)) {
                        worker.done.lock().unwrap().push((job.0, job.1, got, soon));
                    } else if !soon {
                        worker.queue.0.lock().unwrap().now.push_back(job);
                    }
                }
            })
            .expect("cabinet reader thread");
        r
    }

    fn ask(&self, path: PathBuf, kind: Kind) {
        let (lock, cv) = &*self.queue;
        let mut q = lock.lock().unwrap();
        q.soon.retain(|(p, k)| !(*p == path && *k == kind));
        match kind {
            Kind::Cover(..) => q.now.push_front((path, kind)),
            Kind::Color(_) | Kind::Spine | Kind::Logo => q.now.push_back((path, kind)),
        }
        cv.notify_one();
    }

    /// The covers to read in the background, nearest first, in place of the last window's not read yet.
    fn prefetch(&self, jobs: Vec<(PathBuf, Kind)>) {
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap().soon = jobs.into();
        cv.notify_one();
    }

    fn take(&self) -> Vec<(PathBuf, Kind, Got, bool)> {
        std::mem::take(&mut *self.done.lock().unwrap())
    }
}

/// Reads one cover: its spine colour, or the picture cut for a case's front.
fn read(path: &Path, kind: Kind, loaded: Option<&Image>) -> Got {
    if kind == Kind::Logo {
        // With its alpha: a logo is see-through round its letters (`image` would put them on black).
        let logo = pocket_ui::real_label::load_rgba(path).and_then(|r| pocket_ui::series_look::upright_logo(r.w, r.h, &r.px, pocket_ui::series_look::LOGO_EDGE));
        return Got::Logo(logo);
    }
    let owned;
    let img = match loaded {
        Some(img) => Some(img),
        None => {
            owned = image::load_image(path);
            owned.as_ref()
        }
    };
    match kind {
        Kind::Color(m) => Got::Color(img.and_then(|i| media::cover_colors(&media::trim_system_band(i, m.game()).px))),
        Kind::Cover(m, h) => Got::Cover(img.map(|i| {
            let (_, _, lw, lh) = media::label_rect(m, h as f32 / m.size().h as f32);
            art::cover_fit_case(i, lw, lh, m)
        })),
        Kind::Spine => Got::Spine(img.and_then(media::upright_spine)),
        Kind::Logo => unreachable!("a logo is read with its alpha"),
    }
}

/// A series' cover and its spine's logo (`series_look`).
#[path = "app_series_look.rs"]
mod series_look;
use series_look::SeriesFront;

/// A case's pictures, made once: by game, case and colours.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CaseKey {
    rom: PathBuf,
    medium: Medium,
    look: Look,
    /// The stickers on the case (`app_case_stickers`): putting one on makes its pictures again.
    case: CaseStickers,
    /// A frozen game: its spine has its game a little out at the top (`ajar_spine`).
    ajar: bool,
    /// A game never played, in the shop's film (`app_shrink_wrap`): tearing it off makes its pictures again.
    sealed: bool,
    /// Its real spine's photo is read (`App::spine_photo`): the spine is made again with it once it is there.
    photo: bool,
}

/// A case's big spine and front, whether the front has its cover yet, and its last-use stamp.
type Shown = (Rc<Pic>, Rc<Pic>, bool, u64);

/// Lived-in shelf: how far a lying case selected slides out of its stack, a share of its length (and the room a stack
/// keeps on its left for it, `lay_row`).
const LIVED_SLIDE: f32 = 0.2;

/// Lived-in shelf: how far lying case `len` long is out of its stack at `open` (0 in its place, 1 selected).
fn lived_slide(len: i32, open: f32) -> i32 {
    (LIVED_SLIDE * len as f32 * open.clamp(0.0, 1.0)).round() as i32
}

/// Lived-in shelf: how long after the last button the row's posed spines are made ahead (`App::lived_warm_tick`):
/// between the presses of a calm walk along the row, not under a held button's repeats.
const LIVED_WARM_AFTER: Duration = Duration::from_millis(90);

/// Lived-in shelf: the most of the UI thread one turn of making them ahead takes (a turn makes at least one).
const LIVED_WARM_BUDGET: Duration = Duration::from_millis(3);

/// The Cabinet: how long after the last button the shelves next to the selected one are made ahead
/// (`App::cabinet_warm_tick`), and the most of the UI thread one turn takes (a turn makes at least one spine).
const CABINET_WARM_AFTER: Duration = Duration::from_millis(120);
const CABINET_WARM_BUDGET: Duration = Duration::from_millis(3);
const CABINET_WARM_COLOURS: Duration = Duration::from_secs(2);

/// Large fronts shared by Shelf, the game box and its idle warmer. Bound entries independently of rendered frames:
/// Cabinet uses these for box warming too, and never paints the Shelf that used to be their only cleanup path.
const FRONT_KEEP: usize = 12;

/// Cover cuts kept before the cache is trimmed (`App::prune_labels`).
const LABELS_KEEP: usize = 48;

/// The case going into the console while the game starts (the launch animation, `App::launch_animation`).
pub(crate) struct Inserting {
    /// When the game was started: the animation's clock (after the film's tear, `lead`, when a sealed game starts).
    at: Instant,
    pub(crate) item: Item,
    /// What it shows, shared with the Android launcher.
    scene: Scene,
    /// How long the scene waits for a sealed game's film to tear off first (`app_shrink_wrap`); zero for the others.
    lead: Duration,
    /// The film tearing off, painted over the scene's first moments.
    tear: Option<super::app_shrink_wrap::Tear>,
    /// The Shelf or Cabinet page under the launch once static (`App::launch_shelf_restore`).
    kept: Option<LaunchShelf>,
    /// The Cabinet's static shelves/walls/spines behind the showcase while the console beside the TV fades out
    /// (`paint_cabinet`), reused for the first ~120 ms before `kept` takes over the whole page.
    cab_body: Option<(CabinetBodyKey, Vec<u32>, Vec<(usize, usize)>)>,
    /// The case's shadow where it stood on the shelf (x, y, w, h, the floor it is cut at, how far it had turned): it
    /// goes with the case, so it is painted over the kept page each frame (`App::launch_shelf_shadow`).
    shadow: Option<(i32, i32, i32, i32, i32, f32)>,
}

thread_local! {
    /// Home's page painted (not copied) during a launch: the launch bench's count (`bench_playful`, tests/launch_frames.rs).
    pub(crate) static LAUNCH_PAGES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// Cartridge/disc pictures built during launch (`launch_game_pic`), for focus-warm tests (`tests/brick_insert_warm.rs`).
    pub(crate) static LAUNCH_GAME_BUILDS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// Lived-in shelf: thread CPU of `paint_lived_feet` and of building the row (`shelf_row`) for the
    /// bench (`bench_playful` prints them): total time and calls.
    pub(crate) static LIVED_FEET: std::cell::Cell<(Duration, u32)> = const { std::cell::Cell::new((Duration::ZERO, 0)) };
    pub(crate) static LIVED_ROW: std::cell::Cell<(Duration, u32)> = const { std::cell::Cell::new((Duration::ZERO, 0)) };
    /// Lived-in shelf: how many leaning or lying spines were made (`posed_pic`), for the bench's walk.
    pub(crate) static LIVED_POSED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// The Shelf or Cabinet page under a launch: what the frame shows before the scene goes over it, and what it was painted
/// for. Nothing on it moves while the case goes into its console (in Cabinet once the console beside the TV has faded),
/// so the frames after copy it instead of painting the whole page again.
pub(crate) struct LaunchShelf {
    key: LaunchShelfKey,
    px: Vec<u32>,
    /// Once the scene's dim ramp reaches its plateau (`rise >= 1.0`), `px` is shaded in place to this `keep` factor so
    /// every remaining frame is a single copy with zero extra buffer allocation or full-screen RMW.
    dimmed_keep: Option<u32>,
}

#[derive(PartialEq)]
struct LaunchShelfKey {
    /// The canvas: design size, screen row length, pixels.
    size: (i32, i32, usize, usize),
    /// The page colour: the appearance and the colour set, also both pictures of a change between dark and light.
    void: u32,
    /// The cases made again (a sealed game's film off, `cabinet_frozen_changed`).
    gen: u64,
    /// Room lighting/decor rebuild count (`ROOM_REBUILDS`).
    room_gen: usize,
    /// Shelf vs Cabinet look.
    cabinet: bool,
    /// The status corner's minute.
    clock: Option<String>,
    sel: usize,
    with_hints: bool,
}

/// What the cached Cabinet shelves/walls/spines buffer (`Inserting::cab_body`) was painted for.
#[derive(PartialEq)]
struct CabinetBodyKey {
    size: (i32, i32, usize, usize),
    void: u32,
    gen: u64,
    room_gen: usize,
    sel: (usize, usize),
    show_bottom: i32,
    sy: i32,
    cut: bool,
    peek: bool,
}

/// What the cached launch cartridge/disc picture (`CabinetUi::launch_game`) was built for.
struct LaunchGameKey {
    front: Rc<Pic>,
    system: String,
    game: media::Game,
    h: i32,
    has_cover: bool,
    look: Look,
    /// The game's own label printed on it (`App::real_label`): it is made again once the label is there.
    label: bool,
}

impl Inserting {
    /// The case's front the scene starts from (the sealed one, until `unwrap`).
    pub(crate) fn scene_front(&self) -> Rc<Pic> {
        self.scene.front.clone()
    }

    /// `rom`'s film is still flying off this launch's case (`app_shrink_wrap::Tear`).
    pub(crate) fn tearing(&self, rom: &Path, now: Instant) -> bool {
        self.tear.as_ref().is_some_and(|t| t.tearing(rom, now))
    }

    /// Where the scene's case starts.
    pub(crate) fn scene_from(&self) -> (i32, i32, i32, i32) {
        self.scene.from
    }

    /// A sealed game's first start: the scene shows the case without its film and begins `lead` later, after the tear.
    pub(crate) fn unwrap(&mut self, bare: Rc<Pic>, at: Instant, lead: Duration, tear: super::app_shrink_wrap::Tear) {
        self.scene.front = bare;
        self.at = at + lead;
        self.lead = lead;
        self.tear = Some(tear);
    }
}

/// How fast the console beside the Cabinet's TV goes as a Cozy start begins (`App::console_beside_shows`): the scene's
/// console rises in front meanwhile.
const BESIDE_AWAY: Duration = Duration::from_millis(120);
/// How long it takes to come back at the end of the way back.
const BESIDE_BACK: Duration = Duration::from_millis(220);

/// Cozy's way back from a game (`pocket_ui::eject`, `App::begin_eject`): the last picture switches off, then the launch
/// animation plays backwards over the live shelf. Nothing waits for it; any button ends it.
pub(crate) struct Ejecting {
    /// When the game ended: the way back's clock.
    at: Instant,
    /// What shows when (a TV's picture or a handheld's display, switching off first or not, a lid to open).
    way: pocket_ui::eject::Way,
    /// The game's last picture, as the pause menu grabbed it (shared with the switcher's keep, never copied).
    frame: Option<Arc<Image>>,
    /// How a handheld's display switches off (Handheld switch-on, `lcd_on`); `Plain` simply goes dark.
    panel: pocket_ui::lcd_on::Panel,
    /// The played game's case on the shelf.
    item: Item,
    /// The case's way back was asked of `begin_insert` (once, on the first frame after the picture is off).
    scene_asked: bool,
    /// The lid check and the case's sound, on the first frame (nothing of it in `game_exited`).
    ready: bool,
    /// A start that failed (`begin_failed_start`): the way back is the launch backwards from where it had got to, and the
    /// notice waits here until it is over.
    failed: Option<FailedStart>,
    /// The photos the album's camera kept this session (`app_album::album_tucked`): they slide into the case with the game.
    photos: Vec<Image>,
}

/// What a failed start's way back carries: how far the launch had run and the sentence that comes when it is over.
struct FailedStart {
    ran: Duration,
    notice: Notice,
}

impl Ejecting {
    /// With the session's photos (and not a failed start): where the way back holds for them to slide into the case.
    fn hold(&self) -> Option<pocket_ui::eject::PhotoHold> {
        (!self.photos.is_empty() && self.failed.is_none()).then(|| pocket_ui::eject::PhotoHold::of(self.way))
    }

    /// What shows `since` the way back began.
    fn moment(&self, since: Duration) -> pocket_ui::eject::Moment {
        match &self.failed {
            Some(f) => self.way.failed(f.ran).moment(since),
            None => self.way.moment(self.hold().map_or(since, |h| h.clock(since))),
        }
    }

    /// How far the session's photos are in their case at `since` (None: no photos).
    fn tuck(&self, since: Duration) -> Option<f32> {
        self.hold().map(|h| h.tuck(since))
    }

    /// How long the way back takes.
    fn len(&self) -> Duration {
        match &self.failed {
            Some(f) => self.way.failed(f.ran).len(),
            None => self.way.len() + if self.hold().is_some() { pocket_ui::eject::PHOTO_HOLD } else { Duration::ZERO },
        }
    }

    /// When the case's sound is due.
    fn case_shut_at(&self) -> Duration {
        match &self.failed {
            Some(f) => self.way.failed(f.ran).case_shut_at(),
            None => self.way.case_shut_at() + if self.hold().is_some() { pocket_ui::eject::PHOTO_HOLD } else { Duration::ZERO },
        }
    }
}

#[derive(Default)]
pub(crate) struct Timing {
    frames: u32,
    total: Duration,
    worst: Duration,
    /// The same in CPU time of the UI thread: what the frame cost, without waiting for a busy machine.
    cpu_total: Duration,
    cpu_worst: Duration,
    /// What was made during the frame being painted, and how long it took.
    made: Vec<(&'static str, Duration)>,
    /// The frame's parts in CPU time (`App::paint_cabinet`), with `POCKETUI_FRAME_LOG` only.
    parts: Vec<(&'static str, Duration)>,
}

impl Timing {
    /// With the frame log on: the CPU time since `lap` as part `what` of this frame, and `lap` moved on.
    pub(crate) fn part(&mut self, what: &'static str, lap: &mut Duration) {
        if frame_log().is_some() {
            let now = thread_cpu();
            self.parts.push((what, now.saturating_sub(*lap)));
            *lap = now;
        }
    }
}

/// `POCKETUI_FRAME_LOG` set (not 0): the Cabinet logs its slow frames, those over its value in ms if it is a number
/// (0.5 on a fast computer), else over 6 ms; with the time of each part of the frame.
fn frame_log() -> Option<Duration> {
    static LOG: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
    *LOG.get_or_init(|| {
        let v = std::env::var("POCKETUI_FRAME_LOG").ok().filter(|v| v != "0")?;
        Some(Duration::from_secs_f64(v.parse::<f64>().ok().filter(|ms| *ms > 0.0).unwrap_or(6.0) / 1000.0))
    })
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
    /// Light by time of day and the decorations earned (`app_room_extras`); the wood and the Shelf look's tiles in
    /// these caches are made in its light.
    pub(crate) room: super::app_room_extras::Room,
    boards: Option<(u32, Rc<Pic>, Rc<Pic>)>,
    spines: HashMap<CaseKey, (Rc<Pic>, u64)>,
    /// The big spine and front of a case being turned, and whether that front has its cover yet.
    shown: HashMap<CaseKey, (Rc<Pic>, Rc<Pic>, bool, u64)>,
    /// Per ROM: its box art file, if it has one.
    cover_files: HashMap<PathBuf, Option<PathBuf>>,
    /// Per ROM: ScreenScraper's photo of its real spine, if there is one (`screenscraper::existing`, `boxsides/`).
    side_files: HashMap<PathBuf, Option<PathBuf>>,
    /// The spine photos read, stood up (`media::upright_spine`), per file.
    photos: HashMap<PathBuf, Slot<Rc<Image>>>,
    /// Per series (by name): the logo file its spine shows, if one is on the card (`App::series_logo`).
    logo_files: HashMap<String, Option<PathBuf>>,
    /// Whether the card has a folder of logos at all (None: not looked yet): without one no series looks further.
    logo_dir: Option<bool>,
    /// The logos read, stood up (`series_look::upright_logo`), per file.
    logos: HashMap<PathBuf, Slot<Rc<Pic>>>,
    /// Per series (by name) and front size: its cover of its games' cases (`series_look::collage`), made once; None: no
    /// game of it has box art (its plain front).
    series_fronts: HashMap<(String, i32), Option<Rc<Pic>>>,
    /// Series covers being stacked on a thread of their own (`App::series_front`), and the ones it finished.
    series_composing: std::collections::HashSet<(String, i32)>,
    /// Counts the changes that make series' spines and fronts again (`series_changed`): part of their cases' keys.
    pub(crate) series_gen: u64,
    /// The cover cuts of a series' cases while the rest are read: held here, so trimming `labels` never drops one
    /// before the stack is made (`App::series_front`).
    series_labels: HashMap<(PathBuf, Medium, i32), Rc<Image>>,
    /// (series and size, `series_gen` when asked, the stack; None: it could not be made).
    series_composed: Arc<Mutex<Vec<((String, i32), u64, Option<Pic>)>>>,
    /// The home-made front of a game without box art for Split's preview, per ROM and room (`homemade_front`).
    homemade: Option<((PathBuf, i32, i32), Rc<Pic>)>,
    colors: HashMap<PathBuf, Slot<(u32, u32)>>,
    labels: HashMap<(PathBuf, Medium, i32), Slot<Rc<Image>>>,
    /// The height of the selected case's cover cut, per medium: the size the neighbours' covers are read ahead at
    /// (`app_cover_prefetch`).
    label_h: HashMap<Medium, i32>,
    /// The cover files of the entries around the selection (`app_cover_prefetch::NEAR`): their cuts stay when the
    /// cache is trimmed.
    pub(crate) near: Vec<PathBuf>,
    /// The Shelf look's spines standing on the shelf, per case and (scale, title size), with the frame last used in.
    stand: HashMap<(CaseKey, i32), (Rc<Pic>, u64)>,
    /// The Shelf look's selected case shown above the shelf: its big spine and front per case and size, whether that
    /// front is final (has its cover, or there is none to wait for), and its last use on `front_use`'s clock.
    shelf_shown: HashMap<(CaseKey, i32), Shown>,
    front_use: u64,
    /// The Shelf look's row as laid out last, how far it is scrolled, its board, and the front as painted last
    /// (it stays lit through the launch fade).
    shelf_row: Option<ShelfRow>,
    pub(crate) shelf_scroll: Tween,
    /// The Shelf's spines leaning while it scrolls fast (Playful animations).
    pub(crate) lean: pocket_ui::playful::Lean,
    /// A snapshot's clock for the Shelf's frames (`shelf_playful_snapshot`); None: the real one.
    pub(crate) shelf_clock: Option<Instant>,
    shelf_board: Option<(i32, i32, u32, Rc<Pic>)>,
    pub(crate) shelf_lit: Option<(Rc<Pic>, i32, i32)>,
    /// The Shelf look's strip of design pixels painted natively (`paint_shelf`), kept between frames.
    strip_px: Vec<u32>,
    /// The big spine of a case whose front is made with the next frame (`stand_front`'s `later`), the entry whose
    /// front was asked for last, and whether a front is still to come.
    shelf_spine_only: Option<((CaseKey, i32), Rc<Pic>)>,
    shelf_front_for: Option<Item>,
    shelf_waiting: bool,
    shelf_props: HashMap<(Prop, i32), Rc<Pic>>,
    /// Counts the covers' colours that came in: a spine or tile kept by the Shelf look is made again after one.
    colors_gen: u64,
    /// The library's case artwork last seen by the Shelf's row/tile caches (also changes after sync or a reload).
    case_generation: Option<u64>,
    /// The room for the case out of its shelf (`showcase`), and where the selected shelf's top is, gliding with the
    /// cabinet when the room changes.
    show_room: i32,
    show_top: Tween,
    reader: Option<Reader>,
    pub(crate) pending: usize,
    /// Read covers right away instead of on the thread (snapshots, the look step's previews).
    pub(crate) inline: bool,
    /// Cover and spine files read on the spot (`inline`) since the cabinet was made: the Layout page's previews by
    /// series read none (`app_look`).
    pub(crate) read_inline: usize,
    /// The cover file of the game being started: its colours and cuts are read right away by the first frame that wants
    /// them (once, `App::warm_launch_cover`), so the insert scene never opens on a plain stand-in front.
    warm: Option<PathBuf>,
    /// Where each case was painted, for taps: (rectangle, shelf, case); the pulled-out one last.
    drawn: Vec<(i32, i32, i32, i32, usize, usize)>,
    /// The selected shelf's name plate was painted this frame (`cabinet-outcome`'s `plate`).
    plate_shown: bool,
    pub(crate) insert: Option<Inserting>,
    /// The way back from a game, while it plays (`App::begin_eject`); its case uses `insert` going backwards.
    pub(crate) eject: Option<Ejecting>,
    /// When the way back ended: an open case's game slides out of it from then (`paint_ajar_game`), on its own clock,
    /// so that the shelf's move (the case turning in, the rock) does not start again over the case just put back.
    pub(crate) ajar_at: Option<Instant>,
    /// The game box's cartridge or card, made for the case front it came out of (`box_game`), and whether it carries the
    /// game's own label (it is made again when the label arrives).
    box_game: Option<(Rc<Pic>, bool, Rc<Pic>)>,
    /// The games' own labels from ScreenScraper, made on their thread (`real_label`, `App::real_label`).
    real_labels: Option<pocket_ui::real_label::Labels>,
    /// The focused or launching game's cartridge/disc (`launch_game_pic`), pre-warmed while resting on a case.
    launch_game: Option<(LaunchGameKey, Rc<Pic>)>,
    /// Scratch buffer for keeping the hint bar pixels across `paint_insert` without allocating every frame.
    hint_scratch: Vec<u32>,
    /// Pre-faulted pixel buffer reused by `Inserting::cab_body` and `LaunchShelf::px` so Frame 0 and Frame 8 never
    /// trigger 3 MB `mmap` page-fault spikes mid-animation.
    launch_px: Vec<u32>,
    /// Focused game whose insert assets are being pre-warmed (`insert_warm_tick`), and how many passes ran.
    insert_warm: Option<(Item, u8)>,
    /// The cabinet's width and the shelves' width inside its walls at the last frame (`cabinet_warm`).
    warm_geometry: Option<(i32, i32)>,
    /// Fronts with case stickers and the same without (`app_case_stickers`).
    pub(crate) bare_fronts: super::app_case_stickers::BareFronts,
    /// Sealed fronts and the same without their film (`app_shrink_wrap`): the launch tears the film off the one shown.
    pub(crate) wrap_fronts: super::app_shrink_wrap::SealedFronts,
    /// A sticker landing on or lifting off the box's case (`app_case_stickers::Press`).
    pub(crate) press: Option<super::app_case_stickers::Press>,
    /// The game's stickers when its Stickers sheet opened: what the press at its closing is measured against.
    pub(crate) stickers_opened: pocket_ui::case_stickers::CaseStickers,
    /// The TV on the top board (`app_cabinet_tv`).
    pub(crate) tv: super::app_cabinet_tv::TvState,
    /// The front of the selected case as last painted: where an insert starts from.
    last_out: Option<(Item, (i32, i32, i32, i32))>,
    /// Where the selected case's game stood out of it this frame (an open case, `paint_ajar_game`): with the case, what
    /// must keep clear of the TV (`cabinet_snapshot_outcome`).
    last_ajar: Option<(i32, i32, i32, i32)>,
    /// Where the window hung this frame (`paint_cabinet_window`), for the snapshots' outcome line.
    pub(crate) window_at: Option<(i32, i32, i32, i32)>,
    consoles: HashMap<String, Option<Rc<pocket_ui::system_art::Sprite>>>,
    /// The consoles of the start scene (`scene_sprite`), by system and the shelf's scale in percent.
    scene_consoles: HashMap<(String, i32), Option<Rc<pocket_ui::system_art::Sprite>>>,
    frame: u64,
    pub(crate) timing: Timing,
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
            room: Default::default(),
            boards: None,
            spines: HashMap::new(),
            shown: HashMap::new(),
            cover_files: HashMap::new(),
            side_files: HashMap::new(),
            photos: HashMap::new(),
            logo_files: HashMap::new(),
            logo_dir: None,
            logos: HashMap::new(),
            series_fronts: HashMap::new(),
            series_composing: Default::default(),
            series_gen: 0,
            series_labels: HashMap::new(),
            series_composed: Default::default(),
            homemade: None,
            colors: HashMap::new(),
            labels: HashMap::new(),
            label_h: HashMap::new(),
            near: Vec::new(),
            stand: HashMap::new(),
            shelf_shown: HashMap::new(),
            front_use: 0,
            shelf_row: None,
            shelf_scroll: Tween::new(0.0, cabinet::SCROLL),
            lean: Default::default(),
            shelf_clock: None,
            shelf_board: None,
            shelf_lit: None,
            strip_px: Vec::new(),
            shelf_spine_only: None,
            shelf_front_for: None,
            shelf_waiting: false,
            shelf_props: HashMap::new(),
            show_room: 360,
            show_top: Tween::new(0.0, cabinet::SCROLL),
            colors_gen: 0,
            case_generation: None,
            reader: None,
            pending: 0,
            inline: false,
            read_inline: 0,
            warm: None,
            drawn: Vec::new(),
            insert: None,
            eject: None,
            ajar_at: None,
            box_game: None,
            real_labels: None,
            launch_game: None,
            hint_scratch: Vec::new(),
            launch_px: Vec::new(),
            insert_warm: None,
            warm_geometry: None,
            bare_fronts: Default::default(),
            wrap_fronts: Default::default(),
            press: None,
            stickers_opened: Default::default(),
            tv: Default::default(),
            last_out: None,
            last_ajar: None,
            window_at: None,
            consoles: HashMap::new(),
            scene_consoles: HashMap::new(),
            frame: 0,
            timing: Timing::default(),
            curator_keys: Vec::new(),
            plate_shown: false,
            out_on: None,
            back_on: None,
        }
    }
}

/// How often the room's cached wood and tiles were thrown away (`CabinetUi::room_changed`): `bench-shelf` prints it,
/// tests/ritual_tv.rs holds it down while moving between a handheld and a console game in the evening.
pub(crate) static ROOM_REBUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl CabinetUi {
    /// The room's light or the decorations changed (`app_room_extras::room_sync`): the wood and the Shelf look's tiles
    /// are made again with the next frame.
    pub(crate) fn room_changed(&mut self) {
        ROOM_REBUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.walls.clear();
        self.boards = None;
        self.shelf_board = None;
        if let Some(row) = self.shelf_row.as_mut() {
            row.tiles.clear();
        }
    }

    fn cached_front(&mut self, key: &(CaseKey, i32), reading: bool) -> Option<(Rc<Pic>, Rc<Pic>)> {
        let (spine, front, done, used) = self.shelf_shown.get_mut(key)?;
        if !*done && !reading {
            return None;
        }
        self.front_use += 1;
        *used = self.front_use;
        Some((spine.clone(), front.clone()))
    }

    fn cache_front(&mut self, key: (CaseKey, i32), spine: Rc<Pic>, front: Rc<Pic>, done: bool) {
        self.front_use += 1;
        self.shelf_shown.insert(key, (spine, front, done, self.front_use));
        self.prune_fronts();
    }

    fn prune_fronts(&mut self) {
        while self.shelf_shown.len() > FRONT_KEEP {
            let key = self.shelf_shown.iter().min_by_key(|(_, (_, _, _, used))| *used).map(|(key, _)| key.clone()).unwrap();
            self.shelf_shown.remove(&key);
        }
        // Dropping a cache owner must not lose the bare label of a still-active box or launch. BareFronts keeps
        // those by Rc ownership, and releases evicted inactive fronts even when no new sticker is painted.
        self.bare_fronts.prune();
        self.wrap_fronts.prune();
    }

    /// Every picture carrying translated words, including the Shelf's outer row/tile caches.
    pub(crate) fn language_changed(&mut self) {
        self.spines.clear();
        self.shown.clear();
        self.stand.clear();
        self.shelf_shown.clear();
        self.shelf_row = None;
        self.shelf_lit = None;
        self.shelf_spine_only = None;
        self.shelf_front_for = None;
        self.shelf_waiting = false;
        self.homemade = None;
        self.box_game = None;
        self.launch_game = None;
        self.insert_warm = None;
        self.colors_gen += 1;
        self.dirty = true;
        self.bare_fronts.prune();
        self.wrap_fronts.prune();
    }

    /// New box art arrived: look again for games that had none.
    pub(crate) fn covers_changed(&mut self) {
        self.launch_game = None;
        self.insert_warm = None;
        self.cover_files.retain(|_, f| f.is_some());
        // A series' cover is made again from its games' new box art.
        self.series_changed();
        self.spines_changed();
        pocket_ui::dir_index::changed();
        self.colors.retain(|_, c| !matches!(c, Slot::Missing));
        self.labels.retain(|_, c| !matches!(c, Slot::Missing));
        // The games' own labels too: a run may have brought a label scan where only the cartridge's photo was.
        if let Some(labels) = self.real_labels.as_mut() {
            labels.forget();
        }
        self.box_game = None;
    }

    /// New spine photos may be on disk. Keep the ones already loaded and retry cached misses.
    pub(crate) fn spines_changed(&mut self) {
        // Repaint cached shelf tiles so visible cases ask the reader for their photos again.
        self.colors_gen += 1;
        self.side_files.retain(|_, f| f.is_some());
        self.photos.retain(|_, c| !matches!(c, Slot::Missing));
        // New logos may be there too (the run's logo pass).
        self.logo_dir = None;
        self.logo_files.clear();
        self.logos.retain(|_, c| !matches!(c, Slot::Missing));
        self.series_gen += 1;
    }

    /// The series, their games, covers or logos changed: their covers are stacked again, their logos looked up again,
    /// and their spines and fronts made again (their keys carry `series_gen`).
    pub(crate) fn series_changed(&mut self) {
        self.series_fronts.clear();
        self.series_labels.clear();
        self.logo_files.clear();
        self.series_gen += 1;
        // The shelf's painted tiles hold the old spines: painted again (Sol's recheck).
        self.colors_gen += 1;
    }

    /// A player's cover changed. Existing case art and colors of this game have to be read again.
    pub(crate) fn cover_replaced(&mut self, rom: &Path) {
        self.cover_files.remove(rom);
        pocket_ui::dir_index::changed();
        self.colors.remove(rom);
        self.labels.retain(|(path, _, _), _| path != rom);
        self.series_changed();
        self.language_changed();
    }

    /// Port metadata may replace a fallback picture with its real cover. Rebuild those cases after publication.
    pub(crate) fn port_art_changed(&mut self) {
        let port = |rom: &Path| rom.starts_with(pocket_ui::ports::PORTS_DIR);
        self.cover_files.retain(|rom, _| !port(rom));
        self.spines.retain(|key, _| !port(&key.rom));
        self.shown.retain(|key, _| !port(&key.rom));
        self.stand.retain(|(key, _), _| !port(&key.rom));
        self.shelf_shown.retain(|(key, _), _| !port(&key.rom));
        self.covers_changed();
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
            || (pocket_ui::playful::on()
                && self.pull.selected().and_then(|i| self.pull.since(i, now)).is_some_and(|d| d < pocket_ui::playful::span(cabinet::PULL + pocket_ui::playful::WIGGLE)))
            || self.scroll_y.moving(now)
            || self.show_top.moving(now)
            || self.scroll_x.values().any(|t| t.moving(now))
            || self.pending > 0
            || self.insert_playing(now)
            || self.tv.busy(now)
    }

    /// The launch animation is still moving (the game is loading behind it).
    pub(crate) fn insert_playing(&self, now: Instant) -> bool {
        // `at` is in the future while a sealed game's film tears (`lead`): the scene is waiting, so still playing.
        self.insert.as_ref().is_some_and(|i| now.saturating_duration_since(i.at) < i.scene.len() || i.tear.as_ref().is_some_and(|t| t.moving(now)))
    }

    /// A rig's lid or tray shuts after the medium is in: the launch fade keeps its end and runs after the clunk.
    pub(crate) fn launch_fade_at(&self, p: f32) -> f32 {
        self.insert.as_ref().map_or(p, |i| i.scene.rig_fade(p))
    }

    /// The launch fade `p` (0-1) over the screen: to black (`Scene::veil`).
    pub(crate) fn launch_veil(&self, c: &mut pocket_ui::gfx::Canvas, p: f32) {
        match self.insert.as_ref() {
            Some(i) => i.scene.veil(c, p),
            None => c.fade((p * 255.0) as u8),
        }
    }

    /// The pause before the launch fade: the cartridge or disc going into the console.
    pub(crate) fn launch_delay(&self) -> Duration {
        match self.insert.as_ref() {
            Some(i) => cabinet::fade_delay(i.scene.cozy) + i.scene.turn_back() + i.lead,
            None => Duration::ZERO,
        }
    }

    /// A button while the launch animation plays: it is skipped (true when there was one to skip).
    pub(crate) fn skip_insert(&mut self, now: Instant) -> bool {
        if !self.insert_playing(now) {
            return false;
        }
        self.insert = None;
        pocket_ui::ui_sound::forget_waiting();
        true
    }

    fn log_frame(&mut self, took: Duration, cpu: Duration) {
        let Some(slow) = frame_log() else { return };
        let t = &mut self.timing;
        t.frames += 1;
        t.total += took;
        t.worst = t.worst.max(took);
        t.cpu_total += cpu;
        t.cpu_worst = t.cpu_worst.max(cpu);
        let made = std::mem::take(&mut t.made);
        let frame_parts = std::mem::take(&mut t.parts);
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        if cpu > slow {
            let list = |l: &[(&str, Duration)]| l.iter().map(|(what, d)| format!("{what} {:.2}", ms(*d))).collect::<Vec<_>>().join(", ");
            println!("cabinet: slow frame {:.2} ms, cpu {:.2} ms (made: {}) (parts: {})", ms(took), ms(cpu), list(&made), list(&frame_parts));
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

    pub(crate) fn made(&mut self, what: &'static str, since: Instant) {
        self.timing.made.push((what, since.elapsed()));
    }

    /// [`Timing::part`] of this frame.
    fn part(&mut self, what: &'static str, lap: &mut Duration) {
        self.timing.part(what, lap);
    }
}

/// CPU time this thread has used.
pub(crate) fn thread_cpu() -> Duration {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime only writes the timespec it is given.
    if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) } != 0 {
        return Duration::ZERO;
    }
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

/// Title, system id and ROM of something on a shelf.
pub(crate) struct Info {
    pub(crate) title: String,
    pub(crate) system: String,
    pub(crate) rom: PathBuf,
}

/// `system`'s disc (or UMD) `h` high, printed with the cover's art and the title (`media::disc_pic`); with Cozy extras
/// → Printed discs off a plain one, its print in the system's colour without art or title.
/// A real face (`label`, `App::real_label`) is printed in place of the cover's art.
fn printed_disc(text: &mut pocket_ui::text::Text, system: &str, h: i32, look: Look, title: &str, cover: Option<&Image>, label: Option<&Image>) -> Pic {
    let kind = media::DiscKind::of(system);
    if pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::PrintedDiscs) {
        media::disc_pic_labelled(text, kind, h, look, title, cover, label)
    } else {
        media::disc_pic(text, kind, h, Look { color: stripe(system), ..look }, "", None)
    }
}

/// How often `case_info` copied an entry's title, system and ROM (`bench-bigcard`: a shelf switch must not do it for
/// every game of the system).
pub(crate) static CASE_INFOS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl App {
    fn case_info(&self, item: Item) -> Info {
        CASE_INFOS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
            // A series is a shelf in the cabinet, never a case (`app_series`); here for completeness.
            Item::Series(i) => Info {
                title: self.series_describe(i).0,
                system: "series".into(),
                rom: PathBuf::new(),
            },
            Item::Favorites => Info {
                title: "Favourites".into(),
                system: "favorites".into(),
                rom: PathBuf::new(),
            },
            // Curator's boxes on its shelf (the Lens, smart shelves, stickers), coloured as Curator's rows are.
            // Their "ROM" only tells their pictures apart (and a renamed one from its old name); they have no box art.
            Item::Curator | Item::Lens | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) | Item::Rediscover => {
                let title = self.curator_describe(item).0;
                Info {
                    rom: PathBuf::from(format!("curator/{item:?}/{title}/")),
                    title,
                    system: if matches!(item, Item::Sticker(_)) { "sticker" } else { "curator" }.into(),
                }
            }
            // Shelf space's crate on Home: a box of the crate's colour, its emblem the cardboard box (`app_crate`).
            Item::Crate => Info {
                title: "Crate".into(),
                system: "crate".into(),
                rom: PathBuf::from("crate"),
            },
            // Not on the cabinet's shelves (Up on the top shelf opens the search); here for completeness.
            Item::Search => Info {
                title: "Search".into(),
                system: "search".into(),
                rom: PathBuf::new(),
            },
            // Just Play's box on the Continue shelf, coloured as Curator's boxes are; its emblem is the cartridge with dice.
            Item::JustPlay => Info {
                title: pocket_ui::shelf::JUST_PLAY.into(),
                system: "curator".into(),
                rom: PathBuf::from("just-play"),
            },
        }
    }

    /// `case_info(item).system`, borrowed: a row of 22,000 games is laid out without copying a string per game
    /// (`shelf_row`, `lay_row`; a player's R36S with systems of 13,500 and 22,000 games).
    pub(crate) fn item_system(&self, item: Item) -> &str {
        match item {
            Item::Recent(i) => &self.recents.list[i].system,
            Item::Game(s, _) | Item::System(s) => &self.systems[s].name,
            Item::Streaming => "streaming",
            Item::Favorites => "favorites",
            Item::Sticker(_) => "sticker",
            Item::Curator | Item::Lens | Item::SmartShelf(_) | Item::History | Item::Review(_) | Item::Rediscover | Item::JustPlay => "curator",
            Item::Crate => "crate",
            Item::Search => "search",
            // A series in the colours of its first game's console (`app_series::series_stripe_id`).
            Item::Series(i) => self.series_first_system(i).unwrap_or("curator"),
        }
    }

    /// The ROM of a game on the shelf (Continue's or a system's), borrowed (`case_info(item).rom`).
    pub(crate) fn item_path(&self, item: Item) -> Option<&Path> {
        match item {
            Item::Recent(i) => self.recents.list.get(i).map(|r| r.path.as_path()),
            Item::Game(s, g) => self.systems.get(s).and_then(|s| s.games.get(g)).map(|g| g.path.as_path()),
            _ => None,
        }
    }

    /// The case a game (or one of Curator's boxes) stands in on the shelf (`media`).
    pub(crate) fn medium(&self, item: Item) -> Medium {
        match item {
            Item::Streaming | Item::JustPlay | Item::Crate | Item::Curator | Item::Lens | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) | Item::Rediscover => Medium::Keep,
            _ => Medium::of(self.item_system(item)),
        }
    }

    fn form_factor(&self, item: Item) -> FormFactor {
        match item {
            Item::Streaming | Item::JustPlay | Item::Crate | Item::Curator | Item::Lens | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) | Item::Rediscover => FormFactor::DvdCase,
            _ => FormFactor::of(self.item_system(item)),
        }
    }

    /// A new library gives every numeric case, row and shelf position a new meaning.
    pub(crate) fn cabinet_library_changed(&mut self) {
        self.cover_prefetch_stop();
        self.end_eject();
        self.recycle_insert();
        let cab = &mut self.cabinet;
        cab.cab = Cabinet::default();
        cab.dirty = true;
        cab.pull = Pull::default();
        cab.placed = false;
        cab.scroll_x.clear();
        cab.drawn.clear();
        cab.out_on = None;
        cab.back_on = None;
        cab.last_out = None;
        cab.last_ajar = None;
        cab.shelf_row = None;
        cab.shelf_front_for = None;
        cab.shelf_spine_only = None;
        cab.shelf_waiting = false;
        cab.shelf_lit = None;
        cab.insert_warm = None;
        cab.box_game = None;
    }

    /// The shelves, made again when something on them changed.
    fn cabinet_shelves(&mut self) {
        // Looking around an empty card (`app_starter`): no Curator shelf, which needs games to mean anything.
        let curator = if self.empty_home_around() { Vec::new() } else { self.cabinet_curator_keys() };
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
            .filter(|i| matches!(i, Item::Recent(_) | Item::Streaming | Item::JustPlay))
            .map(|i| case(self, i))
            .collect();
        let favorites: Vec<Case> = self.favorite_items().into_iter().map(|i| case(self, i)).collect();
        let systems: Vec<(ShelfKey, Vec<Case>)> = if self.series_on() {
            // By series (`app_series`): a shelf per series, each game in its own console's case.
            (0..self.series.shelves.len()).map(|i| (ShelfKey::Series(i), self.series_items(i).into_iter().map(|it| case(self, it)).collect())).collect()
        } else {
            pocket_ui::shelf::visible_systems(self.systems.iter().map(|s| s.name.as_str()), &self.hidden)
                .into_iter()
                .map(|s| {
                    let (ff, medium) = (FormFactor::of(&self.systems[s].name), Medium::of(&self.systems[s].name));
                    (ShelfKey::System(s), (0..self.systems[s].games.len()).map(|g| Case { item: Item::Game(s, g), ff, medium }).collect())
                })
                .collect()
        };
        let curator_shelves = curator.iter().map(|&k| (k, self.cabinet_curator_items(k).into_iter().map(|i| case(self, i)).collect())).collect();
        self.cabinet.curator_keys = curator;
        self.cabinet.cab = Cabinet::build_keyed(recent, favorites, curator_shelves, systems);
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
                (_, Item::Curator | Item::History | Item::Review(_) | Item::Rediscover) => ShelfKey::Curator,
                (_, Item::SmartShelf(i)) => ShelfKey::SmartShelf(i),
                (_, Item::Sticker(id)) => ShelfKey::Sticker(id),
                (_, Item::Recent(_) | Item::Streaming | Item::Search | Item::JustPlay | Item::Lens | Item::Crate) => ShelfKey::Continue,
                (Screen::Favorites, _) | (_, Item::Favorites) => ShelfKey::Favorites,
                (Screen::Series(i), _) | (_, Item::Series(i)) => ShelfKey::Series(i),
                (_, Item::Game(s, _) | Item::System(s)) => ShelfKey::System(s),
            };
            match item {
                Item::System(_) | Item::Series(_) | Item::Favorites | Item::Curator => shelf(key).map(|s| (s, 0)),
                Item::SmartShelf(_) | Item::Sticker(_) if curator.is_none() => shelf(key).map(|s| (s, 0)),
                _ => cab.find(key, item),
            }
        });
        found.or_else(|| match self.screen {
            Screen::Games(s) => shelf(ShelfKey::System(s)).map(|i| (i, 0)),
            Screen::Series(i) => shelf(ShelfKey::Series(i)).map(|x| (x, 0)),
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
            ShelfKey::Series(x) => Screen::Series(x),
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
                self.moved_at = pocket_ui::frame_clock::now();
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
                // A card's list of games goes back to the card (`app_connections`).
                if !self.connection_back() {
                    self.curator_back();
                }
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
    /// A new frame: the cases drawn in the last one are gone until the Cabinet paints them again, so a tap on a page
    /// over it (Settings, the card, a dialog) never lands on a case that is no longer on screen.
    pub(crate) fn cabinet_frame_begin(&mut self) {
        self.cabinet.drawn.clear();
    }

    /// Whether the Cabinet's cases are on screen and nothing in front takes touches: the tap and swipe of
    /// `app_touch` go to the shelf only then.
    pub(crate) fn cabinet_in_front(&self) -> bool {
        // The cabinet with nothing over it (`app_where`): a tap on a dialog, the search or a page in front of the cases is
        // that page's, never a case's behind it.
        self.theme == pocket_ui::settings::Theme::Cabinet && !self.cabinet.drawn.is_empty() && self.shelf_alone()
    }

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

    /// The launch animation (Settings → Look → Game start animation), started by `launch` just before the game's process:
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
        self.cabinet.eject = None;
        if !matches!(self.theme, super::Theme::Cabinet | super::Theme::Shelf) && !self.big_live() {
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
        // The sounds, all queued now with their waits: the sound thread plays them on time and then leaves the device
        // to the game (`ui_sound::close_when_done`). Quick: a quiet click as it goes in (`cabinet::IN_AT`). Cozy: the
        // case opens, and the game goes into the console with its own sound, seated as it is in (`IN_AT_COZY`). The
        // CRT's sound is pocket-core's, with its picture (`crt_on`).
        use pocket_ui::ui_sound;
        // Motion Reduced or Off starts straight away: the quick way, whatever Animations says (`cabinet::starts_cozy`).
        // An open case plays Cozy's scene as well; its moment stands still behind it (`startup::HOLD_PAUSE`).
        let cozy = cabinet::starts_cozy(self.cozy(), pocket_ui::motion::level());
        if let Some(i) = self.cabinet.insert.as_mut() {
            i.scene.cozy = cozy;
        }
        // A sealed game: unwrapped for good, its film torn off before the scene (Cozy, Motion Full), which waits for it.
        let at = self.cabinet.insert.as_ref().map_or_else(pocket_ui::frame_clock::now, |i| i.at);
        let lead = self.tear_wrap(path, cozy, at);
        let power_on = self.apply_power_on(cozy, hosted);
        let medium = self.medium(item);
        let info = self.case_info(item);
        let kind = media::Game::of(&info.system, &info.rom.to_string_lossy());
        let playful = pocket_ui::playful::on();
        let mechanism = (cozy && playful).then(|| pocket_ui::insert_rig::moving_kind(&info.system, kind)).flatten();
        for cue in pocket_ui::insert_scene::cues(cozy, medium, kind, FormFactor::of(&info.system), mechanism, playful) {
            self.play_sound_after(cue.sound, cue.after + lead, cue.db);
        }
        // A handheld that lights its display: its power switch with its power light.
        if power_on {
            let cue = pocket_ui::insert_scene::switch_on_cue(pocket_ui::insert_scene::scene_system(&info.system), kind);
            self.play_sound_after(cue.sound, cue.after + lead, cue.db);
        }
        ui_sound::played();
        ui_sound::close_when_done(!hosted);
        // A PSP turns back to its front to light its display: its scene's hold is that much longer (`Scene::hold`).
        let hold = self.cabinet.insert.as_ref().map_or_else(|| pocket_ui::insert_scene::hold(cozy, power_on), |i| i.scene.hold());
        hold.map(|h| h + lead).filter(|_| hosted)
    }

    /// A handheld's start ends in its display's light instead of the fade (a pocket-core game: its window follows the light,
    /// `crt_on`), on the tube's own conditions (Cozy, Motion Full, the old-TV switch-on extra). True when it does.
    pub(crate) fn apply_power_on(&mut self, cozy: bool, hosted: bool) -> bool {
        let motion_full = pocket_ui::motion::level() == pocket_ui::motion::Motion::Full;
        let crt_extra = pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::CrtOn);
        let Some(i) = self.cabinet.insert.as_mut() else { return false };
        // The Brick too: the scene lights the display itself (`insert_scene`); its software picture has no overlay, so the game
        // comes in over the lit display instead of clearing the dark off it (tonight's check 14.11.: only the power light lit).
        i.scene.power_on = pocket_ui::insert_scene::power_on_light(&i.scene.system, cozy, motion_full, crt_extra).filter(|_| hosted);
        i.scene.power_on.is_some()
    }

    /// The set of frozen games changed: the spines are made again.
    pub(crate) fn cabinet_frozen_changed(&mut self) {
        self.cabinet.colors_gen += 1;
        self.cabinet.shelf_row = None;
        self.cabinet.dirty = true;
    }

    /// A frozen game's front, shown large: the game inside (the open box's own picture, `box_game`) a little out at its
    /// side, painted before the front. Slides out with the case's turn; with Motion off it is simply there.
    /// Returns where the game was painted (None: the case is shut).
    pub(crate) fn paint_ajar_game(&mut self, c: &mut Canvas, item: Item, m: Medium, front: &Rc<Pic>, (x, y): (i32, i32), clip: (i32, i32)) -> Option<(i32, i32, i32, i32)> {
        let (game, reach) = self.ajar_game(item, m, front)?;
        let gw = game.w;
        let gh = game.h;
        let from = self.cabinet.ajar_at.map_or(self.moved_at, |t| t.max(self.moved_at));
        let at = pocket_ui::motion::progress(pocket_ui::motion::Kind::Move, from, pocket_ui::frame_clock::now(), AJAR_IN);
        // From the middle of its case, where the way back leaves it (`insert_scene`: a game rests there before it comes
        // out), to beside it: no step when the shelf takes over (a clear case shows the game through it).
        let (inside, out_x) = ((front.w - gw) as f32 / 2.0, (front.w - gw) as f32 + reach);
        let gx = x + (inside + (out_x - inside) * at).round() as i32;
        let gy = y + (front.h - gh) / 2;
        art::blit(c, &game, gx, gy, 256, clip);
        Some((gx, gy, gw, gh))
    }

    /// A frozen game's game out of its case `front` (`paint_ajar_game`) and how far it stands out of the case's right
    /// side once it is all the way out; None for a shut case.
    fn ajar_game(&mut self, item: Item, m: Medium, front: &Rc<Pic>) -> Option<(Rc<Pic>, f32)> {
        if !self.stands_open(item) {
            return None;
        }
        let game = self.box_game(item, front)?;
        let reach = ajar_reach_of(self.big_on(), m, (front.w, front.h), Some(&game));
        Some((game, reach))
    }

    /// Whether `item`'s case stands a little open. Easy: every usable moment, whatever the Frozen games extra says
    /// (`big_has_moment`).
    fn stands_open(&self, item: Item) -> bool {
        if self.big_on() { self.big_has_moment(item) } else { self.is_frozen_item(item) }
    }

    /// How far past its case's right side an open case's game stands once it is out (0: a shut case), from the medium's
    /// sizes (no picture is made): the room the Cabinet keeps beside the case, so the game never reaches the TV (a Brick
    /// hand test, 17.11.).
    pub(crate) fn ajar_reach(&self, item: Item, m: Medium, front: &Rc<Pic>) -> i32 {
        if !self.stands_open(item) {
            return 0;
        }
        ajar_reach_of(self.big_on(), m, (front.w, front.h), None).ceil() as i32 + 1
    }

    /// How far right the showcase must reach for the cabinet's widest case out of its shelf, its game standing out of it
    /// (`paint_out`, `show_centre`): the TV's left edge (or the console's beside it) may not stand left of this.
    pub(crate) fn case_room(&self, room: i32) -> i32 {
        let cab = &self.cabinet.cab;
        let media: &[Medium] = if cab.media.is_empty() { &[Medium::Gb] } else { &cab.media };
        let widest = media.iter().map(|&m| {
            let k = show_scale(m, room);
            let (fw, fh) = (m.size().front as f32 * k, m.size().front_h as f32 * k);
            fw + ajar_reach_of(self.big_on(), m, (fw.round() as i32, fh.round() as i32), None).ceil() + 1.0
        }).fold(0.0, f32::max);
        // `paint_out`'s margin each side; the showcase reaches 12 px under the TV's left edge.
        (2.0 * 40.0 + widest).ceil() as i32 - 12
    }

    /// The game box's look and its case's front `h` pixels tall, made as the Shelf look makes its fronts.
    pub(crate) fn box_front(&mut self, item: Item, h: i32) -> (Medium, Look, Option<Rc<Pic>>) {
        let m = self.medium(item);
        let info = self.case_info(item);
        let look = self.case_look(&info, m);
        let k = h as f32 / m.size().front_h.max(1) as f32;
        let (_, front) = self.stand_front(item, m, k, false);
        (m, look, front)
    }

    /// The cartridge, card or printed disc out of the game box's case `front` (`box_front`), at its real size beside it,
    /// with the cover taken off the case's front; made once per case and size.
    pub(crate) fn box_game(&mut self, item: Item, front: &Rc<Pic>) -> Option<Rc<Pic>> {
        let m = self.medium(item);
        let info = self.case_info(item);
        let game = media::Game::of(&info.system, &info.rom.to_string_lossy());
        let label = self.real_label(&info.system, &info.rom);
        if let Some((f, labelled, pic)) = &self.cabinet.box_game {
            if Rc::ptr_eq(f, front) && *labelled == label.is_some() {
                return Some(pic.clone());
            }
        }
        let look = self.case_look(&info, m);
        let k = front.h as f32 / m.size().front_h.max(1) as f32;
        // The label from the case's front without its stickers (`app_case_stickers`).
        let cover = media::cover_of(&self.bare_front(front), m, k);
        let console = self.console_sprite(&info.system);
        let h = game.size_beside(m, front.h).1;
        let t0 = Instant::now();
        // Without box art the front shows a title card: the label gets the title written to fit it instead.
        let has_cover = self.cover_file(&info.system, &info.rom).is_some();
        let pic = Rc::new(if game.is_round() {
            // A disc or a UMD, printed with the cover's art and the title (`media::disc_pic`), or its own face.
            printed_disc(&mut self.text, &info.system, h, look, &info.title, has_cover.then_some(&cover), label.as_deref())
        } else {
            media::game_pic_labelled(&mut self.text, game, h, look, &info.title, has_cover.then_some(&cover), console.as_deref(), label.as_deref())?
        });
        self.cabinet.made("box game", t0);
        self.cabinet.box_game = Some((front.clone(), label.is_some(), pic.clone()));
        Some(pic)
    }

    /// The game ended (or never came up): the case is back on the shelf. The Shelf look paints the insert only while
    /// `launch_at` is set, so nothing else would clear it, and the played game's case stayed blank in its own list.
    pub(crate) fn end_insert(&mut self) {
        self.recycle_insert();
        // The just-played game's resume preview or kept frame may have replaced the file at the same path.
        self.cabinet.tv.pictures_changed();
    }

    /// Drops the active insert scene while returning its reusable framebuffer backing to `CabinetUi::launch_px`.
    fn recycle_insert(&mut self) {
        if let Some(ins) = self.cabinet.insert.take() {
            if let Some(kept) = ins.kept {
                self.cabinet.launch_px = kept.px;
            } else if let Some((_, px, _)) = ins.cab_body {
                self.cabinet.launch_px = px;
            }
        }
    }

    /// How much of the console beside the Cabinet's TV (or of the handheld standing in its place) shows, 0 to 1: it is the
    /// console the Cozy scene brings to the front, so it is never on the screen twice (ritual inventory 19.11.: every
    /// Cabinet start showed it small beside the TV and big in front). It goes dark and out as the scene starts, and comes
    /// back up out of the dark as the way back ends. Quick has no console in its scene: it stays.
    pub(crate) fn console_beside_shows(&self, now: Instant) -> f32 {
        let ease = |t: f32| {
            let t = t.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        if let Some(ej) = self.cabinet.eject.as_ref() {
            let left = ej.len().saturating_sub(now.saturating_duration_since(ej.at));
            return 1.0 - ease(left.as_secs_f32() / BESIDE_BACK.as_secs_f32());
        }
        match self.cabinet.insert.as_ref().filter(|i| i.scene.cozy) {
            Some(ins) => 1.0 - ease(now.saturating_duration_since(ins.at).as_secs_f32() / BESIDE_AWAY.as_secs_f32()),
            None => 1.0,
        }
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
            let key = CaseKey { rom: info.rom.clone(), medium, look, case: self.case_stickers_of(&info.rom), ajar: self.is_frozen_item(item), sealed: self.sealed(&info.rom), photo: self.spine_photo(&info.system, &info.rom).is_some() };
            let Some((_, front, _, _)) = self.cabinet.shown.get(&key).cloned() else { return false };
            (front, from)
        } else {
            let Some((front, x, y)) = self.cabinet.shelf_lit.clone() else { return false };
            let rect = (x, y, front.w, front.h);
            (front, rect)
        };
        // A handheld without a picture of its own starts in the closest handheld that has one (`insert_scene::scene_system`).
        let shown = pocket_ui::insert_scene::scene_system(&info.system).to_string();
        let kind = media::Game::of(&shown, &info.rom.to_string_lossy());
        let has_cover = self.cover_file(&info.system, &info.rom).is_some();
        self.cabinet.insert = Some(Inserting {
            at: pocket_ui::frame_clock::now(),
            item,
            scene: Scene {
                system: shown,
                ff,
                medium,
                look,
                kind,
                has_cover,
                title: info.title,
                front,
                from,
                game: None, game_back: None,
                // Motion below Full: no hold and no rig (`launch_animation`).
                cozy: cabinet::starts_cozy(self.cozy(), pocket_ui::motion::level()),
                at_drive: None,
                rig: None,
                rig_asked: false,
                home: None,
                power_on: None,
                plain_lit: None,
                plain_foot: None,
                tucked: Vec::new(),
                tuck_p: None,
            },
            lead: Duration::ZERO,
            tear: None,
            kept: None,
            cab_body: None,
            shadow: None,
        });
        true
    }

    // ---- art -------------------------------------------------------------------------------------------

    fn reader(&mut self) -> Reader {
        self.cabinet.reader.get_or_insert_with(Reader::start).clone()
    }

    /// The game's own label from ScreenScraper for its cartridge or disc (`real_label`), when it is made; asked of the
    /// labels' thread the first time, so a frame never reads or decodes it. None for a game without one, a system whose
    /// medium takes none, and a game whose label is still being made (the cover's art stands in).
    pub(crate) fn real_label(&mut self, system: &str, rom: &Path) -> Option<std::sync::Arc<Image>> {
        // Real labels Off: the cover's art on the label, exactly as without ScreenScraper.
        if rom.as_os_str().is_empty() || system == "curator" || system == "sticker" || !self.screenscraper_shows(pocket_ui::screenscraper::Face::SupportTexture) {
            return None;
        }
        let target = pocket_ui::real_label::Target::of(system, &rom.to_string_lossy())?;
        let sync = self.pictures.sync;
        let proxy = self.proxy.clone();
        let labels = self.cabinet.real_labels.get_or_insert_with(|| {
            // A label made on its thread asks for a frame: a cartridge at rest on the screen shows it at once.
            let wake: pocket_ui::real_label::Wake = std::sync::Arc::new(move || {
                let _ = proxy.send_event(super::UserEvent::RealLabel);
            });
            pocket_ui::real_label::Labels::new(&self.data_dir, Some(wake))
        });
        labels.sync = sync;
        labels.get(system, rom, target)
    }

    /// The box's real back from ScreenScraper (`Face::BoxBack`) fitted into `w` x `h`, from the picture thread (None
    /// until it is read, and for a game without one: its back stays as pocket-ui lays it out). Nothing is looked up in the
    /// frame (review 26.11., P2: a slow card made the box's front wait): the picture thread finds the file or not, and a
    /// ScreenScraper run that brings one clears the picture cache (`app_screenscraper`).
    pub(crate) fn real_back(&mut self, system: &str, rom: &Path, w: i32, h: i32) -> Option<Rc<Image>> {
        // Real box backs Off (the usual): pocket-ui's own back, with the player's progress.
        if rom.as_os_str().is_empty() || system == "curator" || system == "sticker" || !self.screenscraper_shows(pocket_ui::screenscraper::Face::BoxBack) {
            return None;
        }
        let file = pocket_ui::screenscraper::path(&self.data_dir, pocket_ui::screenscraper::Face::BoxBack, system, rom);
        self.picture(&file, w, h)
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
        let mut candidates = vec![pocket_ui::covers::active_cover_path(&self.data_dir, system, rom)];
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

    /// ScreenScraper's photo of the game's real spine, stood up (docs/SCREENSCRAPER.md): asked for the first time a
    /// spine of the game is made (read on the thread), None until it is there and for a game without one.
    fn spine_photo(&mut self, system: &str, rom: &Path) -> Option<Rc<Image>> {
        // Real spines Off: the painted spine, exactly as without ScreenScraper.
        if rom.as_os_str().is_empty() || system == "curator" || system == "sticker" || !self.screenscraper_shows(pocket_ui::screenscraper::Face::BoxSide) {
            return None;
        }
        let file = match self.cabinet.side_files.get(rom) {
            Some(f) => f.clone(),
            None => {
                let f = pocket_ui::screenscraper::existing(&self.data_dir, pocket_ui::screenscraper::Face::BoxSide, system, rom);
                self.cabinet.side_files.insert(rom.to_path_buf(), f.clone());
                f
            }
        }?;
        if !self.cabinet.photos.contains_key(&file) {
            self.want(&file, Kind::Spine);
        }
        match self.cabinet.photos.get(&file) {
            Some(Slot::Ready(img)) => Some(img.clone()),
            _ => None,
        }
    }

    /// A spine with the game's real spine photo when there is one (`media::photo_spine`: the photo alone, whole), else
    /// the painted one with its title (`media::spine_with`).
    fn spine_or_photo(&mut self, photo: Option<&Image>, m: Medium, k: f32, look: Look, title: &str, want: f32) -> Pic {
        photo
            .and_then(|ph| media::photo_spine(m, k, look, ph))
            .unwrap_or_else(|| media::spine_with(&mut self.text, m, k, look, title, want))
    }

    /// Reads now or asks the thread, for a cover file.
    fn want(&mut self, file: &Path, kind: Kind) {
        let sample = self.sample_cover.as_ref().filter(|(p, _)| p == file).map(|(_, img)| img.clone());
        if self.cabinet.inline || sample.is_some() || self.cabinet.warm.as_deref() == Some(file) {
            self.cabinet.read_inline += usize::from(sample.is_none());
            let got = read(file, kind, sample.as_deref());
            self.cabinet_got(file.to_path_buf(), kind, got);
            return;
        }
        match kind {
            Kind::Color(_) => {
                self.cabinet.colors.insert(file.to_path_buf(), Slot::Pending);
            }
            Kind::Cover(m, h) => {
                self.cabinet.labels.insert((file.to_path_buf(), m, h), Slot::Pending);
            }
            Kind::Spine => {
                self.cabinet.photos.insert(file.to_path_buf(), Slot::Pending);
            }
            Kind::Logo => {
                self.cabinet.logos.insert(file.to_path_buf(), Slot::Pending);
            }
        }
        self.cabinet.pending += 1;
        self.reader().ask(file.to_path_buf(), kind);
    }

    fn cabinet_got(&mut self, file: PathBuf, kind: Kind, got: Got) {
        match (kind, got) {
            (Kind::Color(_), Got::Color(c)) => {
                self.cabinet.colors_gen += 1;
                self.cabinet.colors.insert(file, c.map_or(Slot::Missing, Slot::Ready));
            }
            (Kind::Cover(m, h), Got::Cover(img)) => {
                self.cabinet.labels.insert((file, m, h), img.map_or(Slot::Missing, |i| Slot::Ready(Rc::new(i))));
            }
            (Kind::Spine, Got::Spine(img)) => {
                if img.is_some() {
                    self.cabinet.colors_gen += 1;
                }
                self.cabinet.photos.insert(file, img.map_or(Slot::Missing, |i| Slot::Ready(Rc::new(i))));
            }
            (Kind::Logo, Got::Logo(logo)) => {
                if logo.is_some() {
                    self.cabinet.colors_gen += 1;
                }
                self.cabinet.logos.insert(file, logo.map_or(Slot::Missing, |l| Slot::Ready(Rc::new(l))));
            }
            _ => {}
        }
    }

    /// A game is about to start: if it has box art, the first frame that shows its case reads the cover (colours and
    /// cut, at whatever size that scene asks for) right away rather than on the thread. Started from the search or at
    /// boot its cover is in no cabinet cache yet, and the insert scene showed the plain stand-in front for a few frames
    /// before the cover came (search-start-check). `POCKETUI_LAUNCH_COVER=0` turns it off (the check's "before").
    pub(crate) fn warm_launch_cover(&mut self, system: &str, rom: &Path) {
        if std::env::var("POCKETUI_LAUNCH_COVER").is_ok_and(|v| v == "0") {
            return;
        }
        self.cabinet.warm = self.cover_file(system, rom);
    }

    pub(crate) fn cabinet_poll(&mut self) {
        self.series_fronts_poll();
        // Over with the start: the game runs (or the start was dropped). Not when the first cut is in: the scene reads
        // its own sizes, and the shelf's small cut may be there from before.
        if self.cabinet.warm.is_some() && (self.running.is_some() || self.launch_at.is_none()) {
            self.cabinet.warm = None;
        }
        self.cabinet.prune_fronts();
        // Here, where every look takes its covers in: Easy never paints the Shelf or the Cabinet, and its cuts were
        // never trimmed (one per game passed, 15.11.).
        self.prune_labels();
        let Some(reader) = self.cabinet.reader.clone() else { return };
        for (file, kind, got, soon) in reader.take() {
            if !soon {
                self.cabinet.pending = self.cabinet.pending.saturating_sub(1);
            } else if matches!(kind, Kind::Cover(m, h) if matches!(self.cabinet.labels.get(&(file.clone(), m, h)), Some(Slot::Ready(_) | Slot::Missing))) {
                continue;
            }
            self.cabinet_got(file, kind, got);
        }
    }

    /// A case's colours: its label and band from its cover (asked for when first needed, read on the thread), its
    /// system's otherwise; a cartridge's plastic is its system's.
    pub(crate) fn case_look(&mut self, info: &Info, m: Medium) -> Look {
        let file = self.cover_file(&info.system, &info.rom);
        // A game with no box art at all: home-made, masking tape and marker (`tape_label`). Not Curator's boxes or a
        // system's own case, which have pictures of their own.
        if file.is_none() && is_game_file(info) {
            return media::homemade(media::case_look(&info.system, stripe(&info.system), m, None), m);
        }
        let colors = match file {
            Some(file) => {
                if !self.cabinet.colors.contains_key(&file) {
                    self.want(&file, Kind::Color(m));
                }
                match self.cabinet.colors.get(&file) {
                    Some(Slot::Ready(c)) => Some(*c),
                    _ => None,
                }
            }
            None => None,
        };
        media::case_look(&info.system, stripe(&info.system), m, colors)
    }

    fn spine_pic(&mut self, item: Item, m: Medium) -> Rc<Pic> {
        let info = self.case_info(item);
        let look = self.case_look(&info, m);
        let photo = self.spine_photo(&info.system, &info.rom);
        let key = CaseKey {
            case: self.case_stickers_of(&info.rom),
            rom: info.rom.clone(),
            medium: m,
            look,
            ajar: self.is_frozen_item(item),
            sealed: self.sealed(&info.rom),
            photo: photo.is_some(),
        };
        let frame = self.cabinet.frame;
        if let Some((pic, used)) = self.cabinet.spines.get_mut(&key) {
            *used = frame;
            return pic.clone();
        }
        let t0 = Instant::now();
        // As large as the label takes (37 px at most): readable at arm's length on the Mini V2 (Alex's photo,
        // 2026-10-28: "Legend of…" at 21 px was not).
        let pic = self.spine_or_photo(photo.as_deref(), m, 1.0, look, &media::printed_title(&info.title, item.title_is_name()), 37.0);
        let pic = self.stuck_spine(pic, m, 1.0, &info.rom, key.case);
        let pic = self.sealed_spine(pic, 1.0, &info.rom, key.sealed);
        let pic = Rc::new(if key.ajar { ajar_spine(&mut self.text, pic, &info.system, &info.rom, media::shelf_scale(), look) } else { pic });
        self.cabinet.made("spine", t0);
        self.cabinet.spines.insert(key, (pic.clone(), frame));
        pic
    }

    /// The home-made front of a game that has no box art at all (masking tape and marker, `tape_label`), as large as
    /// fits in `w` x `h`; None for a game with box art (it shows that) and for anything that is not a game.
    pub(crate) fn homemade_front(&mut self, item: Item, w: i32, h: i32) -> Option<Rc<Pic>> {
        if !matches!(item, Item::Game(..) | Item::Recent(_)) {
            return None;
        }
        let info = self.case_info(item);
        if self.cover_file(&info.system, &info.rom).is_some() || !is_game_file(&info) {
            return None;
        }
        let key = (info.rom.clone(), w, h);
        if let Some((k, pic)) = &self.cabinet.homemade {
            if *k == key {
                return Some(pic.clone());
            }
        }
        let m = self.medium(item);
        let look = self.case_look(&info, m);
        let (fw, fh) = media::front_size(m, 1.0);
        let k = (w as f32 / fw.max(1) as f32).min(h as f32 / fh.max(1) as f32);
        let name = super::system_short(&self.systems, &info.system);
        let printed = media::printed_title(&info.title, item.title_is_name());
        let pic = Rc::new(media::front(&mut self.text, m, k, look, &printed, None, None, &name));
        self.cabinet.homemade = Some((key, pic.clone()));
        Some(pic)
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

    /// Cover cuts are many and big: over [`LABELS_KEEP`] the cache keeps only those being read and those of the entries
    /// around the selection (the ones in view among them), so reading ahead never pushes out a cover in view.
    fn prune_labels(&mut self) {
        if self.cabinet.labels.len() > LABELS_KEEP {
            let near = &self.cabinet.near;
            self.cabinet.labels.retain(|(file, _, _), slot| matches!(slot, Slot::Pending) || near.contains(file));
        }
    }

    /// The selected entry's cover cut is `h` high: its neighbours' are read ahead at that size.
    fn note_label_h(&mut self, item: Item, m: Medium, h: i32) {
        if self.items.get(self.sel) == Some(&item) {
            self.cabinet.label_h.insert(m, h);
        }
    }

    /// The cover cut `item` needs as the selected case, when it is not read or being read yet (`app_cover_prefetch`).
    pub(crate) fn cabinet_prefetch_job(&mut self, item: Item, file: PathBuf) -> Option<(PathBuf, Kind)> {
        let m = self.medium(item);
        let h = *self.cabinet.label_h.get(&m)?;
        (!self.cabinet.labels.contains_key(&(file.clone(), m, h))).then_some((file, Kind::Cover(m, h)))
    }

    /// The covers for the reader thread to read when it has nothing else to do, in place of the last ones.
    pub(crate) fn cabinet_prefetch(&mut self, jobs: Vec<(PathBuf, Kind)>) {
        if self.cabinet.inline {
            return;
        }
        if jobs.is_empty() && self.cabinet.reader.is_none() {
            return;
        }
        self.reader().prefetch(jobs);
    }

    /// The cover cut `item` would show as the selected case is read (None: it has no box art, or no size is known yet).
    pub(crate) fn cover_cut_ready(&mut self, item: Item) -> Option<bool> {
        let info = self.case_info(item);
        let file = self.cover_file(&info.system, &info.rom)?;
        let m = self.medium(item);
        let h = *self.cabinet.label_h.get(&m)?;
        Some(matches!(self.cabinet.labels.get(&(file, m, h)), Some(Slot::Ready(_))))
    }

    /// Cover cuts in the cache (the benches).
    pub(crate) fn cover_cuts(&self) -> usize {
        self.cabinet.labels.len()
    }

    /// The cover of an entry is read and ready for its front (a game without one counts as ready: it has nothing to wait for).
    pub(crate) fn kids_cover_ready(&mut self, item: Item) -> bool {
        let info = self.case_info(item);
        let Some(file) = self.cover_file(&info.system, &info.rom) else { return true };
        self.cabinet.labels.iter().any(|((f, _, _), slot)| *f == file && matches!(slot, Slot::Ready(_)))
    }

    /// The big spine and front of a case coming out; its cover is asked for the first time it is needed.
    fn shown_pics(&mut self, item: Item, m: Medium) -> (Rc<Pic>, Rc<Pic>) {
        let info = self.case_info(item);
        let look = self.case_look(&info, m);
        let file = self.cover_file(&info.system, &info.rom);
        let k = show_scale(m, self.cabinet.show_room);
        let lh = (m.size().h as f32 * k).round() as i32;
        self.note_label_h(item, m, lh);
        let label = file.and_then(|f| self.cover_label(&f, m, lh));
        let photo = self.spine_photo(&info.system, &info.rom);
        let key = CaseKey {
            case: self.case_stickers_of(&info.rom),
            rom: info.rom.clone(),
            medium: m,
            look,
            ajar: self.is_frozen_item(item),
            sealed: self.sealed(&info.rom),
            photo: photo.is_some(),
        };
        let frame = self.cabinet.frame;
        let fh = media::front_size(m, k).1;
        if let Some((spine, front, has, used)) = self.cabinet.shown.get_mut(&key) {
            // Made for this room (another screen size or a taller shelf makes it again).
            if (*has || label.is_none()) && front.h == fh {
                *used = frame;
                return (spine.clone(), front.clone());
            }
        }
        let sprite = pocket_ui::system_art::get(&info.system).or_else(|| emblem(item));
        let t0 = Instant::now();
        let spine = self.spine_or_photo(photo.as_deref(), m, k, look, &media::printed_title(&info.title, item.title_is_name()), media::spine_size(m, k));
        let spine = self.stuck_spine(spine, m, k, &info.rom, key.case);
        let spine = Rc::new(self.sealed_spine(spine, k, &info.rom, key.sealed));
        self.cabinet.made("big spine", t0);
        let t0 = Instant::now();
        let name = super::system_short(&self.systems, &info.system);
        let printed = media::printed_title(&info.title, item.title_is_name());
        let front = media::front(&mut self.text, m, k, look, &printed, label.as_deref(), sprite, &name);
        let front = self.stuck_front(front, m, k, &info.rom, key.case, (&printed, label.is_some(), sprite));
        let front = self.sealed_front(front, k, &info.rom, key.sealed);
        self.cabinet.made(if label.is_some() { "front" } else { "plain front" }, t0);
        self.cabinet.shown.insert(key, (spine.clone(), front.clone(), label.is_some(), frame));
        (spine, front)
    }

    fn wall_pic(&mut self, inner: i32, seed: u32, w: i32) -> Rc<Pic> {
        if let Some(p) = self.cabinet.walls.get(&(w, inner, seed, pal().void)) {
            return p.clone();
        }
        let t0 = Instant::now();
        let mut p = art::compartment(w, inner, seed);
        self.cabinet.room.lit.tint_all(&mut p.px);
        let p = Rc::new(p);
        self.cabinet.made("wood", t0);
        self.cabinet.walls.insert((w, inner, seed, pal().void), p.clone());
        p
    }

    /// How tall the start scene's console may be: `high`, and an arcade cabinet, which stands upright where the consoles
    /// lie wide, half as tall again. Fitted into the consoles' box it was a third as big as them, and nothing else happens
    /// in its start (ritual review 14.11., finding 10).
    fn scene_high(system: &str, high: i32) -> i32 {
        if pocket_ui::system_art::canonical(system) == "arcade" {
            high * 3 / 2
        } else {
            high
        }
    }

    fn console_sprite(&mut self, system: &str) -> Option<Rc<pocket_ui::system_art::Sprite>> {
        let high = Self::scene_high(system, 330);
        self.cabinet
            .consoles
            .entry(system.to_string())
            .or_insert_with(|| pocket_ui::system_art::get(system).map(|s| Rc::new(s.fitted(560, high))))
            .clone()
    }

    /// The console of the start scene on a screen `h` lines high. The case it takes in is shown at the shelf's scale, which is
    /// smaller on a short screen (1024x768: 1.7 against 2.5), and the console is made in the same proportion: at 560
    /// pixels it was 55 % of the width there (47 % at 1240x1080) and stood over the title and the case (ritual review 14.11.).
    fn scene_sprite(&mut self, system: &str, h: i32) -> Option<Rc<pocket_ui::system_art::Sprite>> {
        let r = media::shelf_scale_for(h) / media::SHELF_SCALE;
        if r >= 1.0 {
            return self.console_sprite(system);
        }
        let (w, high) = ((560.0 * r).round() as i32, (Self::scene_high(system, 330) as f32 * r).round() as i32);
        self.cabinet.scene_consoles.entry((system.to_string(), w)).or_insert_with(|| pocket_ui::system_art::get(system).map(|s| Rc::new(s.fitted(w, high)))).clone()
    }

    /// The width `scene_sprite(system, h)` (or its stand-in) would have, without fitting/copying its pixels.
    fn scene_sprite_width(system: &str, h: i32) -> f32 {
        let fitted_w = |sys: &str| -> Option<i32> {
            let s = pocket_ui::system_art::get(sys)?;
            let r = media::shelf_scale_for(h) / media::SHELF_SCALE;
            let (max_w, max_h) = if r >= 1.0 {
                (560, Self::scene_high(sys, 330))
            } else {
                ((560.0 * r).round() as i32, (Self::scene_high(sys, 330) as f32 * r).round() as i32)
            };
            if s.w == 0 || s.h == 0 || max_w <= 0 || max_h <= 0 {
                return Some(0);
            }
            let scale = (max_w as f32 / s.w as f32).min(max_h as f32 / s.h as f32);
            if scale >= 1.0 {
                let k = if scale.floor() / scale >= 0.75 { scale.floor() } else { scale };
                Some((s.w as f32 * k) as i32)
            } else {
                Some(((s.w as f32 * scale) as u32).max(1) as i32)
            }
        };
        fitted_w(system).or_else(|| fitted_w(pocket_ui::insert_scene::STAND_IN)).unwrap_or(360) as f32
    }

    /// The shelf's scale changed (the screen's height did: HDMI plugged in): the shelves and their pictures are made again.
    pub(crate) fn cabinet_scale_changed(&mut self) {
        self.cabinet.dirty = true;
        self.cabinet.spines.clear();
        self.cabinet.shown.clear();
        self.cabinet.labels.clear();
        self.cabinet.shelf_shown.clear();
        self.cabinet.walls.clear();
        self.cabinet.launch_game = None;
        self.cabinet.insert_warm = None;
        self.cabinet.placed = false;
    }

    // ---- painting ----------------------------------------------------------------------------------------

    /// The cabinet's frame on a screen `h` lines high: (room for the case, bottom of the top board, how far its tallest
    /// shelf sinks behind that board). One frame for the whole cabinet, whatever shelf or case is selected: laid out for
    /// its tallest shelf and its tallest kind of case, so every shelf stands whole under the top board and every case has
    /// its room above it (the hand test 16.11.2026: laid out for each shelf by itself, the top board and the TV on it
    /// jumped from system to system). Only the shelves move under it. A cabinet short enough to stand whole stands on the
    /// hints' line, its showcase taking the room above (the Mini V2 photo, 2026-11-03: no strip of nothing under it).
    fn cabinet_plan(&self, w: i32, h: i32) -> (i32, i32, i32) {
        let cab = &self.cabinet.cab;
        let tall = cab.tallest_pitch();
        let media: &[Medium] = if cab.media.is_empty() { &[Medium::Gb] } else { &cab.media };
        // The tallest case's showcase over the tallest shelf, `hide` lines of that shelf's empty headroom behind the board:
        // the room is the same for every case, how high it needs to be is not.
        let plan = |hide: i32, extra: i32| {
            let (room, top, sink) = media.iter().map(|&m| showcase(h, m, tall - hide, extra)).max_by_key(|f| f.1).unwrap_or((0, 0, 0));
            (room, top, sink + hide)
        };
        // A TV left out for want of a few lines (1280 x 720 with a PS2 shelf): the space over that shelf's tallest case
        // goes behind the board instead, never a spine.
        let tv = |room: i32| super::app_cabinet_tv::tv_scale(w, h, room).is_some();
        let hide = if tv(plan(0, 0).0) { 0 } else { (2..=cabinet::HEADROOM - 6).step_by(2).find(|&s| tv(plan(s, 0).0)).unwrap_or(0) };
        let (room, top, sink) = plan(hide, 0);
        let rest = scene_bottom(h) - (top - sink) - (cab.height() + PLINTH_H);
        if rest <= 0 {
            return (room, top, sink);
        }
        let (room, top, sink) = plan(hide, rest);
        let rest = scene_bottom(h) - (top - sink) - (cab.height() + PLINTH_H);
        (room, top + rest.max(0), sink)
    }

    /// The scroll that shows shelf `cs` right under the top board ending at `top` (sunk by its part of `sink`, the tallest
    /// shelf's); near the cabinet's end the plinth stays on the hints' line and the selection goes on down instead, as a
    /// list stops at its end.
    fn cabinet_scroll_for(cab: &cabinet::Cabinet, h: i32, cs: usize, top: i32, sink: i32) -> i32 {
        let own_sink = (sink - (cab.tallest_pitch() - cab.pitch(cs))).max(0);
        // A shelf taller than the room under the showcase (a shelf of DVD cases, Curator's, on the Brick's 768 lines) goes
        // that much further behind the top board, by its empty headroom only (`HEADROOM` less 6 lines, the spines stay
        // whole): its board and name plate stand over the hints instead of fading out under them (Brick review 20.11.: the
        // Curator plate was missing).
        let over = top - own_sink + cab.pitch(cs) - shelves_bottom(h);
        let tuck = over.clamp(0, (cabinet::HEADROOM - 6 - own_sink).max(0));
        let end = cab.height() + PLINTH_H - scene_bottom(h);
        (cab.y(cs) - top + own_sink + tuck).min(end).max(-top)
    }

    /// The last shelf standing whole over the hints at scroll `sy` (its board's bottom; 0: none) and whether more follow.
    fn cabinet_stack(&self, h: i32, sy: i32) -> (i32, bool) {
        let cab = &self.cabinet.cab;
        let mut lowest = 0;
        for s in 0..cab.shelves.len() {
            let bottom = cab.y(s) - sy + cab.pitch(s);
            if bottom > shelves_bottom(h) {
                return (lowest, true);
            }
            lowest = bottom;
        }
        (lowest, false)
    }

    pub(crate) fn paint_cabinet(&mut self, c: &mut Canvas) {
        let started = Instant::now();
        let cpu_started = thread_cpu();
        let mut lap = cpu_started;
        let now = pocket_ui::frame_clock::now();
        self.cabinet.frame += 1;
        self.cabinet_poll();
        self.cabinet_shelves();
        let (light, earned) = self.room_sync();
        self.cabinet.drawn.clear();
        self.cabinet.plate_shown = false;
        self.cabinet.last_ajar = None;
        self.cabinet.window_at = None;
        // The app's selection becomes a case (after the look was changed on a system card, say); the look step's
        // previews paint a sample and leave the selection alone.
        let at = if self.sample_cover.is_none() {
            self.cabinet_sync()
        } else {
            self.cabinet_preview_cursor()
        };
        let Some((cs, ci)) = at else {
            c.clear(pal().void);
            return;
        };
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
        self.cabinet.warm_geometry = Some((cab_w, view));
        // The frame (top board, showcase, TV) is the cabinet's, the same on every shelf; the shelves scroll under it.
        let (room, top, sink) = self.cabinet_plan(c.w, c.h);
        self.cabinet.show_room = room;
        if !self.cabinet.placed || !pocket_ui::motion::allowed(pocket_ui::motion::Kind::Move) {
            self.cabinet.show_top.jump(top as f32);
        } else {
            self.cabinet.show_top.set(top as f32, now);
        }
        let show_bottom = self.cabinet.show_top.value(now).round() as i32;
        let want_sy = Self::cabinet_scroll_for(&self.cabinet.cab, c.h, cs, top, sink);
        let want_y = want_sy as f32;
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
        let (mut lowest, _) = self.cabinet_stack(c.h, sy);
        // The selected shelf alone is taller than the room (a tiny screen): as much of it as fits.
        let cut = lowest <= show_bottom;
        if cut {
            lowest = shelves_bottom(c.h);
        }
        // Room left under the last whole shelf where the scroll goes: the next one's top looks in (it fades out at the
        // hints), no plinth; a sliver of it would read as a fault, the plinth closes the cabinet there instead.
        let peek = !cut && {
            let (end, more) = self.cabinet_stack(c.h, want_sy);
            more && shelves_bottom(c.h) - end >= PEEK_MIN
        };
        let clip = (show_bottom, if peek { shelves_bottom(c.h) } else { lowest });
        let floor_y = self.cabinet.cab.y(cs) - sy + self.cabinet.cab.shelves[cs].height + 3;

        // During the first ~120 ms of a Cabinet launch (while the console beside the TV fades out and before the whole
        // page can be kept in `LaunchShelf`), the shelves/walls/spines below the showcase are already completely static.
        let can_cache_body = self.launch_at.is_some()
            && self.cabinet.eject.is_none()
            && self.sample_cover.is_none()
            && self.cabinet.placed
            && !pocket_ui::scene_check::recording()
            && !pocket_ui::text::boxes::on()
            && !self.cabinet.scroll_y.moving(now)
            && !self.cabinet.show_top.moving(now)
            && !self.cabinet.scroll_x.values().any(|t| t.moving(now))
            && !self.cabinet.pull.animating(now);
        let body_key = can_cache_body.then(|| CabinetBodyKey {
            size: (c.w, c.h, c.stride(), c.pixels().len()),
            void: pal().void,
            gen: self.cabinet.colors_gen,
            room_gen: ROOM_REBUILDS.load(std::sync::atomic::Ordering::Relaxed),
            sel: (cs, ci),
            show_bottom,
            sy,
            cut,
            peek,
        });
        let mut out: Vec<(usize, usize)> = Vec::new();
        let body_hit = body_key.as_ref().is_some_and(|key| {
            if let Some((saved_key, px, saved_out)) = self.cabinet.insert.as_ref().and_then(|i| i.cab_body.as_ref()) {
                if saved_key == key {
                    c.pixels_mut().copy_from_slice(px);
                    out.extend_from_slice(saved_out);
                    return true;
                }
            }
            false
        });

        if !body_hit {
            c.clear(pal().void);
            // The cabinet's top board and plinth, at its ends.
            if self.cabinet.boards.as_ref().is_none_or(|(page, b, _)| b.w != cab_w || *page != pal().void) {
                self.cabinet.walls.retain(|(w, _, _, _), _| *w == cab_w);
                let lit = |mut p: Pic| {
                    self.cabinet.room.lit.tint_all(&mut p.px);
                    Rc::new(p)
                };
                let boards = (lit(art::top_board(cab_w, TOP_H)), lit(art::top_board(cab_w, PLINTH_H)));
                self.cabinet.boards = Some((pal().void, boards.0, boards.1));
            }
            let (_, top_board, plinth) = self.cabinet.boards.clone().unwrap();
            // The top board closes the cabinet under the showcase, the plinth under the last whole shelf in view: the
            // cabinet always stands whole.
            art::blit(c, &top_board, CAB_X, show_bottom - TOP_H, 256, (0, c.h));
            if pocket_ui::scene_check::recording() {
                // What stands on the top board (the TV, its console, a case out in the showcase) stands on its top, 12
                // lines into it as it is seen a little from above (`paint_cabinet_tv`'s `board_y`).
                pocket_ui::scene_check::set_origin(0, 0);
                pocket_ui::scene_check::surface("the cabinet's top board", show_bottom - TOP_H - 12, 12, CAB_X, CAB_X + cab_w);
            }
            if !cut && !peek {
                art::blit(c, &plinth, CAB_X, lowest, 230, (0, scene_bottom(c.h)));
            }

            // A bookend, a plant or a lamp on a shelf with room beside its games: four in view at most.
            let mut props = 0;
            // The trophy and the framed "100 %" stand on the first shelf with room for them (`app_room_extras::deco_for`).
            let awards_on = self.awards_shelf(&earned, view);
            self.cabinet.part("setup", &mut lap);
            for s in 0..self.cabinet.cab.shelves.len() {
                let top = self.cabinet.cab.y(s) - sy;
                let (inner, pitch) = (self.cabinet.cab.shelves[s].height, self.cabinet.cab.pitch(s));
                if top + pitch < clip.0 || top > clip.1 {
                    continue;
                }
                let floor = top + inner + 3;
                if pocket_ui::scene_check::recording() {
                    // The shelf's board goes on past the walls where the shelf scrolls.
                    let sc = self.cabinet.scroll_x.get(&self.cabinet.cab.shelves[s].key).map_or(0.0, |t| t.value(now)).round() as i32;
                    let (from, to) = (inner_x - sc, inner_x - sc + self.cabinet.cab.shelves[s].width.max(view));
                    pocket_ui::scene_check::set_origin(0, 0);
                    pocket_ui::scene_check::surface("a shelf of the cabinet", floor, 0, from.min(inner_x), to.max(inner_x + view));
                }
                // The shelves away from the selected one are a little darker.
                let away = (floor - floor_y).abs() as f32;
                let shade = 256 - (away * 0.22).min(70.0) as u32;
                let skey = self.cabinet.cab.shelves[s].key;
                let scroll = self.cabinet.scroll_x.get(&skey).map_or(0.0, |t| t.value(now)).round() as i32;
                // Its props (`prop_plan`); a glowing lamp's light is in the back wall's picture (`lit_wall`), made once.
                let plan = if props < 4 { self.prop_plan(s, inner_x, view, scroll, (light, &earned, awards_on == Some(s))) } else { Vec::new() };
                let wall = match plan.iter().find(|p| p.2) {
                    Some((lamp, x, _)) if light.lamp > 0 => self.lit_wall(s, cab_w, (lamp.clone(), x - CAB_X), light.lamp),
                    _ => self.wall_pic(inner, s as u32 % 3, cab_w),
                };
                art::blit(c, &wall, CAB_X, top, shade, clip);
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
                    // Inside the walls only; in the room's light (half its dimming, no tint: `Light::case_shade`).
                    self.blit_between(c, &pic, x, y, light.shaded(shade), clip, inner_x, inner_x + view);
                    pocket_ui::scene_check::put_pic_in(format!("shelf {s} case {i}"), &pic, x, y, pocket_ui::scene_check::Rests::Settled, true, clip);
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
                props += self.paint_props(c, &plan, floor, light.shaded(shade), clip);
                let shown = self.paint_plate(c, s, inner_x + 18, floor + 5, shade, clip);
                if s == cs {
                    self.cabinet.plate_shown = shown;
                }
            }

            // A shelf taller than the room is cut: it fades out over the hints' edge.
            if cut {
                art::fade_band_to(c, lowest - 34, lowest, pal().void);
            }
            if peek {
                art::fade_band_to(c, clip.1 - PEEK_FADE, clip.1, pal().void);
            }
            if let Some(key) = body_key {
                let mut px = self.cabinet.insert.as_mut().and_then(|i| i.cab_body.take()).map(|(_, p, _)| p).unwrap_or_else(|| std::mem::take(&mut self.cabinet.launch_px));
                px.clear();
                px.extend_from_slice(c.pixels());
                if let Some(ins) = self.cabinet.insert.as_mut() {
                    ins.cab_body = Some((key, px, out.clone()));
                } else {
                    self.cabinet.launch_px = px;
                }
            }
        }

        // The TV on the top board, showing the selected game (`app_cabinet_tv`); the cases out of the shelf keep clear of it.
        self.cabinet.part("shelves", &mut lap);
        let info = self.case_info(item);
        let cover = self.cover_file(&info.system, &info.rom);
        self.paint_cabinet_tv(c, &info.system, &info.rom, cover, show_bottom - TOP_H - 12, room, now);
        self.cabinet.part("tv", &mut lap);
        // The window on the wall over the top board, left of the TV: the same on every shelf; the cases out of the
        // shelf pass in front of it.
        self.paint_cabinet_window(c, show_bottom - TOP_H - 12, room);

        // The cases out of the shelf, up in the showcase: the one going back first, the selected one over it.
        out.sort_by_key(|&(s, i)| (s, i) == (cs, ci));
        self.cabinet.part("window", &mut lap);
        for (s, i) in out {
            self.paint_out(c, s, i, sy, now, inner_x, show_bottom);
        }
        self.cabinet.part("out", &mut lap);

        // The way into the console (and back out) is painted last of all, over the title and the hints, so that it dims
        // them with the rest like every look (`App::paint`, `paint_eject`; polish review 12.11.).
        self.paint_cabinet_title(c, cs, item);
        self.cabinet.part("title", &mut lap);
        // Taps are handled by `cabinet_tap`; swipes move by a case sideways and by a shelf up and down.
        let len = self.cabinet.cab.shelves[cs].cases.len();
        self.hits.row(pocket_ui::touch::Area::new(0, SCENE_TOP - lift(c.h), c.w, scene_bottom(c.h) - SCENE_TOP + lift(c.h)), ci);
        self.hits.list(ci, len, 44.0, 250.0);

        // Pictures not used for a while go.
        let frame = self.cabinet.frame;
        if self.cabinet.spines.len() > 360 {
            self.cabinet.spines.retain(|_, (_, used)| frame - *used < 90);
        }
        if self.cabinet.shown.len() > 8 {
            self.cabinet.shown.retain(|_, (_, _, _, used)| frame - *used < 30);
        }
        self.cabinet.log_frame(started.elapsed(), thread_cpu().saturating_sub(cpu_started));
    }

    /// The window (`pocket_ui::room_window`, Cozy extras → Window) on the wall over the top board whose top is at
    /// `board`, in the showcase `room` high: at the right of the wall left of the TV (or its console), clear of them,
    /// where the cases out of the shelf come less often; left out where it doesn't fit whole. Not in the look step's
    /// sample cabinet.
    fn paint_cabinet_window(&mut self, c: &mut Canvas, board: i32, room: i32) {
        if self.sample_cover.is_some() {
            return;
        }
        let Some((win, shadow)) = self.room_window_pic(media::shelf_scale() * pocket_ui::room_window::SCALE) else { return };
        let (left, right) = (CAB_X + art::WALL, self.cabinet.tv.left.unwrap_or(c.w - CAB_X - art::WALL));
        let top = board - room;
        let gap = (media::shelf_scale() * 10.0).round() as i32;
        let Some((x, y)) = pocket_ui::room_window::cabinet_spot(left, right, top, board, (win.pic.w, win.pic.h), gap) else { return };
        if pal().void != 0 {
            shadow.paint(c, x, y, 0.3, (0, board));
        }
        art::blit(c, &win.pic, x, y, self.cabinet.room.lit.case_shade(), (0, c.h));
        self.cabinet.window_at = Some((x, y, win.pic.w, win.pic.h));
        if pocket_ui::scene_check::recording() {
            use pocket_ui::scene_check::{self as sc, Rests};
            sc::set_origin(0, 0);
            sc::wall("the wall over the cabinet", left, top, right, board, Some(", out"));
            let (fx, fy, fw, fh) = win.frame;
            sc::put("the window", x + fx, y + fy, fw, fh, Rests::Hung, true);
            sc::surface_here("the window's sill", y + win.sill_y, 0, x + win.sill.0, x + win.sill.1);
            let (px, py, pw, ph) = win.piece;
            sc::put("the piece on the window's sill", x + px, y + py, pw, ph, Rests::Settled, false);
        }
    }

    /// A spine, cut off at the cabinet's inner walls.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn blit_between(&self, c: &mut Canvas, pic: &Pic, x: i32, y: i32, shade: u32, clip: (i32, i32), left: i32, right: i32) {
        if x >= left && x + pic.w <= right {
            art::blit(c, pic, x, y, shade, clip);
            return;
        }
        art::blit_cols(c, pic, x, y, shade, clip, (left - x, right - x));
    }

    /// The props of shelf `s` where there is room beside its games, each with its x and whether it is the lamp: a
    /// bookend right after the last case holding the row up, what the player earned (`room`: its figurine on a system's
    /// shelf; the trophy and the framed "100 %" on the `awards` shelf) in a row after it, and the shelf's own prop
    /// (`media::Prop::beside`, the same every time) further along in what is left, well clear of the games. Never over
    /// a case, never cut at the shelf's end, and only on a shelf that doesn't scroll.
    #[allow(clippy::too_many_arguments)]
    fn prop_plan(
        &mut self,
        s: usize,
        inner_x: i32,
        view: i32,
        scroll: i32,
        room: (pocket_ui::room_light::Light, &pocket_ui::room_deco::Earned, bool),
    ) -> Vec<(Rc<Pic>, i32, bool)> {
        let (_, earned, awards) = room;
        let shelf = &self.cabinet.cab.shelves[s];
        if scroll != 0 || shelf.width > view || shelf.cases.is_empty() {
            return Vec::new();
        }
        let (key, shelf_h, shelf_w) = (shelf.key, shelf.height, shelf.width);
        let seed = self.prop_seed(key);
        let system = match key {
            ShelfKey::System(i) => self.systems.get(i).map(|sys| sys.name.clone()),
            _ => None,
        };
        let room = shelf_h - cabinet::HEADROOM / 2;
        let games_end = inner_x + shelf_w - cabinet::INSET;
        let right = inner_x + view - 36;
        let mut plan = Vec::new();
        let bookend = self.room_prop_pic(Prop::Bookend, earned, media::shelf_scale());
        let mut free_from = games_end + 60;
        let mut after = games_end + 2;
        if bookend.h <= room && games_end + 2 + bookend.w <= inner_x + view {
            free_from += bookend.w;
            after += bookend.w;
            plan.push((bookend, games_end + 2, false));
        }
        // What was earned, in a row after the bookend: whatever fits whole.
        let deco: Vec<Rc<Pic>> = super::app_room_extras::deco_for(earned, system.as_deref(), awards)
            .into_iter()
            .map(|p| self.room_prop_pic(p, earned, media::shelf_scale()))
            .collect();
        let widths: Vec<i32> = deco.iter().map(|p| if p.h <= room { p.w } else { 0 }).collect();
        for (i, x) in pocket_ui::room_deco::place(after, right, 22, &widths) {
            free_from = free_from.max(x + deco[i].w + 30);
            plan.push((deco[i].clone(), x, false));
        }
        if let Some(prop) = media::Prop::beside(seed, room) {
            let pic = self.room_prop_pic(prop, earned, media::shelf_scale());
            // Somewhere in the free part of the shelf, the same place every time.
            let free = right - pic.w - free_from;
            if free >= 0 && pic.h <= room {
                let x = free_from + (seed >> 3) as i32 % (free + 1).min(240);
                plan.push((pic, x, prop == Prop::Lamp));
            }
        }
        plan
    }

    /// The props of a `prop_plan` standing on `floor`, in `shade`, each with its shadow on the back panel. How many.
    fn paint_props(&self, c: &mut Canvas, plan: &[(Rc<Pic>, i32, bool)], floor: i32, shade: u32, clip: (i32, i32)) -> u32 {
        for (k, (pic, x, _)) in plan.iter().enumerate() {
            let y = floor - pic.h;
            art::blit(c, pic, *x, y, shade, clip);
            pocket_ui::scene_check::put_pic_in(format!("prop {k} on the shelf at {floor}"), pic, *x, y, pocket_ui::scene_check::Rests::Settled, true, clip);
            art::soft_shadow(c, x + pic.w, y + pic.h / 3, 3, pic.h * 2 / 3, 6, 0.3 * shade as f32 / 256.0, (clip.0.max(y), clip.1.min(floor)));
        }
        plan.len() as u32
    }

    /// Shelf `s`'s back wall with its lamp's glow (`lamp`: the lamp's picture and its x on the wall), made once per
    /// shelf, place and strength and kept with the wood (`CabinetUi::room_changed` clears it with the light).
    fn lit_wall(&mut self, s: usize, cab_w: i32, lamp: (Rc<Pic>, i32), strength: u8) -> Rc<Pic> {
        let inner = self.cabinet.cab.shelves[s].height;
        // In the plain walls' cache: the grain, the lamp's place and strength in the third slot, the page colour turned
        // over in the fourth (no page colour is that high, so it never meets a plain wall).
        let key = (cab_w, inner, s as u32 % 3 | (lamp.1 as u32) << 8 | (strength as u32) << 24, u32::MAX - pal().void);
        if let Some(p) = self.cabinet.walls.get(&key) {
            return p.clone();
        }
        let mut pic = (*self.wall_pic(inner, s as u32 % 3, cab_w)).clone();
        let (w, h) = (pic.w, pic.h);
        {
            let mut c = Canvas::new(&mut pic.px, w as u32, h as u32);
            let floor = inner + 3;
            super::app_room_extras::lamp_glow(&mut c, &lamp.0, lamp.1, floor - lamp.0.h, strength, (art::WALL, 0, w - art::WALL, floor));
        }
        let pic = Rc::new(pic);
        self.cabinet.walls.insert(key, pic.clone());
        pic
    }

    /// The number a shelf keeps for its props (which one, where): the same every time.
    fn prop_seed(&self, key: ShelfKey) -> u32 {
        match key {
            ShelfKey::System(i) => self.systems.get(i).map_or(7, |sys| sys.name.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32)),
            ShelfKey::Series(i) => self.series.shelves.get(i).map_or(11, |sh| sh.name.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32)),
            ShelfKey::Continue => 1,
            ShelfKey::Favorites => 2,
            ShelfKey::Curator => 3,
            ShelfKey::Lens => 4,
            ShelfKey::SmartShelf(i) => 100 + i as u32,
            ShelfKey::Sticker(id) => 1000 + id,
            ShelfKey::Connection => 5,
            ShelfKey::Rediscover => 6,
        }
    }

    /// The first shelf (from the top) with room after its games for its bookend, its own figurine, the trophy and the
    /// framed "100 %": where those two stand. None when neither was earned or no shelf has the room.
    fn awards_shelf(&mut self, earned: &pocket_ui::room_deco::Earned, view: i32) -> Option<usize> {
        if !earned.trophy && !earned.hundred {
            return None;
        }
        let mut need = media::Prop::Bookend.pic().w + 60 + 36;
        let mut tall = 0;
        for p in super::app_room_extras::deco_for(earned, None, true) {
            let pic = self.room_prop_pic(p, earned, media::shelf_scale());
            need += pic.w + 22;
            tall = tall.max(pic.h);
        }
        let shelves = &self.cabinet.cab.shelves;
        (0..shelves.len()).find(|&s| {
            let sh = &shelves[s];
            let figure = match sh.key {
                ShelfKey::System(i) => self.systems.get(i).and_then(|sys| earned.figure(&sys.name)).map_or(0, |f| media::Prop::Figure(f).pic().w + 22),
                _ => 0,
            };
            !sh.cases.is_empty() && sh.height - cabinet::HEADROOM / 2 >= tall && sh.width - cabinet::INSET + need + figure <= view
        })
    }

    /// The shelf's name on a small paper card in a holder on the board's edge.
    /// The name plate of shelf `s` on its board's edge, if the board is in view (`clip`): whether it was painted.
    fn paint_plate(&mut self, c: &mut Canvas, s: usize, x: i32, y: i32, shade: u32, clip: (i32, i32)) -> bool {
        if y < clip.0 || y + 26 > clip.1 {
            return false;
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
            return true;
        }
        // A brass-coloured holder, the card in it (in the colours of Settings → Look → Theme: `palette::Scheme`).
        let plate = pocket_ui::palette::scheme();
        c.round_rect(x - 3, y - 3, w + 6, 30, 4, dim(plate.holder));
        c.round_rect(x, y, w, 24, 2, dim(plate.card));
        self.text.draw(c, Face::Body, 20, x + 11, y + 19, dim(plate.card_ink), &name);
        true
    }

    /// A case out of its shelf: sliding out and turning (`openness` 0 to 1).
    #[allow(clippy::too_many_arguments)]
    fn paint_out(&mut self, c: &mut Canvas, s: usize, i: usize, sy: i32, now: Instant, inner_x: i32, show_bottom: i32) {
        let case = self.cabinet.cab.shelves[s].cases[i];
        // The case going into the console is drawn by `paint_insert`. Its place is still worked out once (on its way back
        // from a game every frame), for it to go back to, and nothing drawn there.
        let hidden = self.cabinet.insert.as_ref().is_some_and(|ins| ins.item == case.item);
        // Once: where it goes back to (`insert_scene::Home`), below.
        if hidden && self.cabinet.eject.is_none() && self.cabinet.insert.as_ref().is_some_and(|ins| ins.scene.home.is_some()) {
            return;
        }
        let o = self.cabinet.pull.openness(case.item, now);
        let size = case.medium.size();
        let skey = self.cabinet.cab.shelves[s].key;
        let scroll = self.cabinet.scroll_x.get(&skey).map_or(0.0, |t| t.value(now));
        let floor = (self.cabinet.cab.y(s) - sy + self.cabinet.cab.shelves[s].height + 3) as f32;
        let slot_cx = inner_x as f32 + self.cabinet.cab.shelves[s].x(i) as f32 - scroll + size.spine as f32 / 2.0;
        let k = show_scale(case.medium, self.cabinet.show_room);
        // Out, it shows its front (a SNES box lies down from its short side to it as it turns: `draw_turning`).
        let (fw, fh) = (size.front as f32 * k, size.front_h as f32 * k);
        let margin = 40.0;
        let right = self.cabinet.tv.left.map_or(c.w as f32, |x| x as f32 + 12.0);
        let (spine, front) = self.shown_pics(case.item, case.medium);
        // An open case's game stands out at its right side: that keeps clear of the TV too (it reached over the TV's
        // console when the game stood at the shelf's right end, a Brick hand test 17.11.).
        let reach = self.ajar_reach(case.item, case.medium, &front) as f32;
        let show_cx = show_centre(slot_cx, fw, reach, right, margin, self.cabinet.cab.shelves[s].width, c.w - 2 * CAB_X - 2 * art::WALL);
        // Up in the showcase over its shelf, standing on the cabinet's top board: nothing of the shelves under it.
        let show_cy = (show_bottom - TOP_H - 12) as f32 - fh / 2.0;
        let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
        let mut mv = smooth(o.clamp(0.0, 1.0));
        let turned = smooth(((o - 0.12) / 0.88).clamp(0.0, 1.0));
        // Playful animations: it springs out, a touch past its place and back (`playful::back_out`); once there it
        // rocks once on its bottom edge (`playful::wiggle`).
        let playful = pocket_ui::playful::on();
        let mut rock = 0.0;
        if playful {
            if let Some((start, p)) = self.cabinet.pull.linear(case.item, now) {
                let from = smooth(start.clamp(0.0, 1.0));
                mv = from + (1.0 - from) * pocket_ui::playful::back_out(p);
            }
            if let Some(since) = self.cabinet.pull.since(case.item, now) {
                let after = since.as_secs_f32() / pocket_ui::motion::time_scale() - cabinet::PULL.as_secs_f32();
                rock = pocket_ui::playful::wiggle(after);
            }
        }
        let scale = 1.0 + (k - 1.0) * mv;
        let cx = slot_cx + (show_cx - slot_cx) * mv;
        let cy = (floor - size.h as f32 / 2.0) + (show_cy - (floor - size.h as f32 / 2.0)) * mv;
        // Its shadow on what is behind and under it.
        let h_case = size.h as f32 + (size.front_h - size.h) as f32 * turned;
        let (w_now, h_now) = ((size.spine as f32 + (size.front - size.spine) as f32 * turned) * scale, h_case * scale);
        if hidden {
            let r = ((cx - front.w as f32 / 2.0).round() as i32, (cy - front.h as f32 / 2.0).round() as i32, front.w, front.h);
            if (s, i) == self.cabinet_cursor().unwrap_or((usize::MAX, 0)) {
                self.cabinet.last_out = Some((case.item, r));
            }
            // Its place on the shelf, for the case to go back to once its game is out (`insert_scene::Home`): its spine
            // standing where the shelf shows it at rest.
            if let Some(ins) = self.cabinet.insert.as_mut().filter(|ins| ins.item == case.item) {
                ins.scene.home = Some(pocket_ui::insert_scene::Home { spine, at: (slot_cx, floor - size.h as f32 / 2.0), scale: 1.0 });
            }
            return;
        }
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
            let ajar = self.paint_ajar_game(c, case.item, case.medium, &front, (x, y), (0, scene_bottom(c.h)));
            if (s, i) == self.cabinet_cursor().unwrap_or((usize::MAX, 0)) {
                self.cabinet.last_ajar = ajar;
            }
            art::blit_sheared(c, &front, x, y, rock * front.h as f32, 256, (0, scene_bottom(c.h)));
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
        if pocket_ui::scene_check::recording() && r.2 > 0 {
            // Out, it stands in the showcase on the top board; on its way it is in the player's hand.
            let rests = if mv >= 1.0 && turned >= 1.0 {
                pocket_ui::scene_check::Rests::Settled
            } else {
                pocket_ui::scene_check::Rests::Held("on its way between its shelf and the showcase")
            };
            pocket_ui::scene_check::put(format!("shelf {s} case {i}, out"), r.0, r.1, r.2, r.3, rests, true);
        }
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
            ShelfKey::Series(i) => (self.series.shelves.get(i).map_or_else(String::new, |sh| sh.name.clone()), false),
            key => self.cabinet_curator_plate(key).unwrap_or_default(),
        }
    }

    fn paint_cabinet_title(&mut self, c: &mut Canvas, s: usize, item: Item) {
        let (shelf, _) = self.shelf_name(self.cabinet.cab.shelves[s].key);
        self.paint_top_line(c, Some(&shelf), pal().dust);
        let (title, _, _, meta) = self.describe(item);
        let fav = self.is_favorite(item);
        // Whole shelves below the showcase can be off screen even on a large panel. Keep the current
        // position and both movement directions visible, instead of making the plinth look like the end.
        // With larger text (1280x720, Large) the title a lift higher would stand in the top line's letters: it keeps its
        // cap height clear of them, the meta row following at the same distance.
        let lift_t = if pocket_ui::type_scale::grow(50) > 0 {
            lift(c.h).min(150 - (super::app_status_corner::top_line(c.h) + 12 + (pocket_ui::type_scale::px(50) as f32 * 0.8) as i32))
        } else {
            lift(c.h)
        };
        let meta_y = 192 - lift_t;
        let total = self.cabinet.cab.shelves.len();
        let mut right = c.w - MARGIN;
        if total > 1 {
            let position = format!("{} / {}", s + 1, total);
            let width = self.text.width(Face::Body, 24, &position);
            self.text.draw_right(c, Face::Body, 24, right - 24, meta_y, pal().dust, &position);
            for (x, up, available) in [(right - width - 42, true, s > 0), (right - 8, false, s + 1 < total)] {
                if available {
                    for d in 0..7 {
                        let y = if up { meta_y - 18 + d } else { meta_y - 6 - d };
                        c.rect(x - d, y, 2 * d + 1, 2, pal().dust);
                    }
                }
            }
            right -= width + 80;
        }
        // A game's name whole. One line stays where it always was. A name that needs a second (a subtitle, smaller) has
        // no room above the showcase for it, so it takes the meta row, and the meta line follows it on that row: the
        // type goes a step smaller before the name is cut with "…" (`title_block`).
        let mut meta_x = MARGIN;
        // An open game's word ("Continue") stands at the right end of the meta row: neither the subtitle nor the meta line
        // may run into it.
        let mut word_right = right;
        if self.is_frozen_item(item) {
            right -= self.frozen_word_width() + 24;
        }
        if matches!(item, Item::Game(..) | Item::Recent(_)) {
            let star = if fav { super::app_switcher::star_room(50) } else { 0 };
            // The type goes smaller while a second line would leave the first no room under the top line (Large text, the
            // wider faces).
            let mut size = 50;
            let (lines, y) = loop {
                let mut lines = title_block::name_layout(&mut self.text, Face::Title, size, &title, c.w - 2 * MARGIN - star, 2);
                if lines.len() < 2 {
                    break (lines, 150 - lift_t);
                }
                lines = title_block::name_layout(&mut self.text, Face::Title, size, &title, right - MARGIN - star - 140, 2);
                let up = (title_block::step(&self.text, lines[0].px) + title_block::step(&self.text, lines.last().map_or(0, |l| l.px))) / 2;
                let y = (150 - lift_t).min(meta_y - up);
                let least = super::app_status_corner::top_line(c.h) + 12 + (pocket_ui::type_scale::px(lines[0].px) as f32 * 0.8) as i32;
                // The meta line keeps its size and place beside the second line: the name goes a step smaller first
                // (Retro's even-width letters left "Not played yet" too little room at 1024x768 in Large text, and the
                // meta line a step smaller stood lower than the other games').
                let meta_fits = lines.get(1).is_none_or(|second| {
                    let x = MARGIN + self.text.width_plain(Face::Title, second.px, &second.text) + 40 + if fav { super::app_switcher::star_room(second.px) } else { 0 };
                    self.text.width(Face::Body, 28, &meta) <= right - x
                });
                if lines.len() < 2 || (y >= least && meta_fits) || size <= 30 {
                    break (lines, y);
                }
                size = size * 5 / 6;
            };
            let first = &lines[0];
            let w = title_block::paint_line(&mut self.text, c, Face::Title, first, MARGIN, y, pal().chalk);
            title_block::note(first, y);
            if fav && lines.len() == 1 {
                App::paint_title_star(c, MARGIN + w, y, 50, pal().chalk);
            }
            if let Some(second) = lines.get(1) {
                let w = title_block::paint_line(&mut self.text, c, Face::Title, second, MARGIN, meta_y, pal().chalk);
                title_block::note(second, meta_y);
                if fav {
                    App::paint_title_star(c, MARGIN + w, meta_y, second.px, pal().chalk);
                }
                meta_x = MARGIN + w + 40 + if fav { super::app_switcher::star_room(second.px) } else { 0 };
            }
        } else {
            self.draw_title(c, Face::Title, 50, MARGIN, 150 - lift_t, c.w - 2 * MARGIN, pal().chalk, &title, true, fav);
        }
        // Preserve both old sizes wherever the whole line already fits. If it would be cut, use the spare space beside
        // the shelf counter, Continue and a second title line first; then try smaller letters before losing any words.
        let size = if self.text.width(Face::Body, 28, &meta) > right - meta_x { 24 } else { 28 };
        let (size, lines) = if self.text.width(Face::Body, size, &meta) <= right - meta_x {
            (size, vec![self.text.fit(Face::Body, size, &meta, right - meta_x)])
        } else {
            if total > 1 { right += 20; word_right += 20; }
            if self.is_frozen_item(item) { right += 8; }
            if meta_x > MARGIN { meta_x -= 16; }
            let room = right - meta_x;
            let px = [24, 22, 20, 18].into_iter().find(|&px| self.text.width(Face::Body, px, &meta) <= room).unwrap_or(18);
            // Retro has a minimum of two screen pixels per font pixel. Below that, ever smaller nominal sizes do
            // not make its letters narrower: use the spare line above the showcase before cutting the last words.
            // Break between facts when possible, so a play time stays together instead of leaving "de jeu" alone.
            let whole = self.text.translated(&meta).to_string();
            let parts = whole.rsplit_once(", ").filter(|(first, last)| {
                self.text.width_plain(Face::Body, px, &format!("{first},")) <= room && self.text.width_plain(Face::Body, px, last) <= room
            });
            let lines = if self.text.width(Face::Body, px, &meta) > room {
                parts.map(|(first, last)| vec![format!("{first},"), last.to_string()])
                    .unwrap_or_else(|| self.text.wrap_prose_at_most(Face::Body, px, &meta, room, 2))
            } else { vec![whole] };
            (px, lines)
        };
        let whole = self.text.translated(&meta).to_string();
        pocket_ui::text::boxes::note_meta(&whole, &lines);
        for (i, line) in lines.iter().enumerate() {
            self.text.draw(c, Face::Body, size, meta_x, meta_y + i as i32 * pocket_ui::type_scale::line(size, 4), pal().dust, line);
        }
        if self.is_frozen_item(item) {
            self.paint_frozen_word(c, word_right, meta_y, pal().chalk);
        }
    }

    /// Starting a game: the console comes up, the case opens and the cartridge, card or disc slides out of it, the case
    /// fades and the game goes into the console (every system the same way, 0.1.4).
    /// What the kept launch page (`LaunchShelf`) is painted for, this frame; None where nothing is kept: not Shelf or
    /// settled Cabinet, not a launch, no scene, or scene/text checks recording (a copy draws no words/surfaces).
    fn launch_shelf_key(&self, c: &Canvas, big: bool, with_hints: bool) -> Option<LaunchShelfKey> {
        let cabinet = self.theme == super::Theme::Cabinet;
        if big
            || (!cabinet && self.theme != super::Theme::Shelf)
            || self.launch_at.is_none()
            || self.cabinet.eject.is_some()
            || pocket_ui::text::boxes::on()
            || pocket_ui::scene_check::recording()
        {
            return None;
        }
        let ins = self.cabinet.insert.as_ref()?;
        if cabinet {
            let now = pocket_ui::frame_clock::now();
            if self.cabinet.dirty
                || !self.cabinet.placed
                || self.sample_cover.is_some()
                || self.cabinet.scroll_y.moving(now)
                || self.cabinet.show_top.moving(now)
                || self.cabinet.scroll_x.values().any(|t| t.moving(now))
                || self.cabinet.pull.animating(now)
                || self.cabinet.tv.busy(now)
                || self.cabinet.tv.is_live()
                || (ins.scene.cozy && self.console_beside_shows(now) > 0.0)
            {
                return None;
            }
        }
        Some(LaunchShelfKey {
            size: (c.w, c.h, c.stride(), c.pixels().len()),
            void: pal().void,
            gen: self.cabinet.colors_gen,
            room_gen: ROOM_REBUILDS.load(std::sync::atomic::Ordering::Relaxed),
            cabinet,
            clock: pocket_ui::status_corner::clock(),
            sel: self.sel,
            with_hints,
        })
    }

    /// A launch frame after the page is static in Shelf or Cabinet: copies the kept page and, at `n == 1` when no
    /// shrink-wrap tear is running, fuses or reuses the scene's full-screen dim pass. Returns `(restored, already_dimmed)`.
    pub(crate) fn launch_shelf_restore(&mut self, c: &mut Canvas, big: bool, with_hints: bool) -> (bool, bool) {
        let Some(key) = self.launch_shelf_key(c, big, with_hints) else { return (false, false) };
        let now = pocket_ui::frame_clock::now();
        let Some(ins) = self.cabinet.insert.as_mut() else { return (false, false) };
        let Some(kept) = ins.kept.as_mut().filter(|k| k.key == key) else { return (false, false) };
        let pre_dim = c.n() == 1 && c.stride() == c.w as usize && c.pixels().len() == (c.w * c.h) as usize && ins.tear.is_none();
        if !pre_dim {
            if kept.dimmed_keep.is_some() {
                return (false, false);
            }
            c.pixels_mut().copy_from_slice(&kept.px);
            return (true, false);
        }
        let elapsed = now.saturating_duration_since(ins.at);
        let st = cabinet::insert_at(elapsed, ins.scene.cozy);
        let keep = 256 - (st.dim * 256.0) as u32;
        if let Some(done) = kept.dimmed_keep {
            if done == keep {
                c.pixels_mut().copy_from_slice(&kept.px);
                return (true, true);
            }
            return (false, false);
        }
        if keep >= 256 {
            c.pixels_mut().copy_from_slice(&kept.px);
            return (true, true);
        }
        if st.rise >= 1.0 {
            // Dim ramp has reached its constant plateau (`dim == 0.6`): shade `kept.px` in place once so every
            // remaining frame of the launch is a single pure copy with zero extra 3 MB allocation.
            for (dst, src) in c.pixels_mut().iter_mut().zip(&mut kept.px) {
                let s = ((*src & 0xff00ff) * keep >> 8 & 0xff00ff) | ((*src & 0x00ff00) * keep >> 8 & 0x00ff00);
                *src = s;
                *dst = s;
            }
            kept.dimmed_keep = Some(keep);
            return (true, true);
        }
        for (dst, &src) in c.pixels_mut().iter_mut().zip(&kept.px) {
            *dst = ((src & 0xff00ff) * keep >> 8 & 0xff00ff) | ((src & 0x00ff00) * keep >> 8 & 0x00ff00);
        }
        (true, true)
    }

    /// The page just painted under a launch, kept for the frames after (`launch_shelf_restore`).
    pub(crate) fn launch_shelf_keep(&mut self, c: &Canvas, big: bool, with_hints: bool) {
        let Some(key) = self.launch_shelf_key(c, big, with_hints) else { return };
        let recycled = std::mem::take(&mut self.cabinet.launch_px);
        if let Some(ins) = self.cabinet.insert.as_mut() {
            // Reuse `cab_body`'s already-faulted pixel buffer when handing off from Cabinet's beside-TV fade to `kept`.
            let mut px = ins
                .kept
                .take()
                .map(|k| k.px)
                .or_else(|| ins.cab_body.take().map(|(_, p, _)| p))
                .unwrap_or(recycled);
            ins.cab_body = None;
            px.clear();
            px.extend_from_slice(c.pixels());
            ins.kept = Some(LaunchShelf { key, px, dimmed_keep: None });
        } else {
            self.cabinet.launch_px = recycled;
        }
    }

    /// The case's shadow on the shelf going with it (`case_shown`), over the page, painted or kept.
    pub(crate) fn launch_shelf_shadow(&self, c: &mut Canvas) {
        let Some(ins) = self.cabinet.insert.as_ref().filter(|_| self.theme == super::Theme::Shelf) else { return };
        let Some((x, y, w, h, floor, turned)) = ins.shadow else { return };
        let elapsed = pocket_ui::frame_clock::now().saturating_duration_since(ins.at);
        let shown = ins.scene.case_shown(elapsed);
        if shown > 0.0 {
            // It goes with the case on its way back to the row (a block of shadow left where the case had stood looked like a hole).
            let (x, y, w, h) = ins.scene.with_case(elapsed, (x, y, w, h));
            art::soft_shadow(c, x, y, w, h, 24, 0.45 * turned * shown, (0, floor));
        }
    }

    /// Builds or reuses the launch cartridge, card or printed disc for `front` (`CabinetUi::launch_game`).
    #[allow(clippy::too_many_arguments)]
    fn launch_game_pic(
        &mut self,
        system: &str,
        medium: Medium,
        game: media::Game,
        front: &Rc<Pic>,
        from_h: i32,
        look: Look,
        title: &str,
        has_cover: bool,
        screen_h: i32,
        stage_k: f32,
        seated: i32,
        (own_system, rom): (&str, &Path),
    ) -> Option<Rc<Pic>> {
        let target_h = if game.is_round() {
            let sw = Self::scene_sprite_width(system, screen_h);
            game.size_beside(medium, from_h).1.max(2 * pocket_ui::insert_scene::over_drive(game, sw, stage_k) as i32).max(seated)
        } else {
            game.size_beside(medium, from_h).1
        };
        // The label is the game's own (its system's folder, not the console the scene shows).
        let label = self.real_label(own_system, rom);
        if let Some((cached, pic)) = &self.cabinet.launch_game {
            if Rc::ptr_eq(&cached.front, front)
                && cached.system == system
                && cached.game == game
                && cached.h == target_h
                && cached.has_cover == has_cover
                && cached.look == look
                && cached.label == label.is_some()
            {
                return Some(pic.clone());
            }
        }
        LAUNCH_GAME_BUILDS.with(|n| n.set(n.get() + 1));
        let k = from_h as f32 / medium.size().front_h.max(1) as f32;
        let cover = media::cover_of(&self.bare_front(front), medium, k);
        let t0 = Instant::now();
        let cover = has_cover.then_some(&cover);
        let pic = if game.is_round() {
            Some(printed_disc(&mut self.text, system, target_h, look, title, cover, label.as_deref()))
        } else {
            media::game_pic_labelled(&mut self.text, game, target_h, look, title, cover, None, label.as_deref())
        };
        self.cabinet.made("game", t0);
        let pic = pic.map(Rc::new)?;
        self.cabinet.launch_game = Some((
            LaunchGameKey { front: front.clone(), system: system.to_string(), game, h: target_h, has_cover, look, label: label.is_some() },
            pic.clone(),
        ));
        Some(pic)
    }

    /// Pre-decodes/scales the focused game's console rig (or plain sprite) and cartridge/disc while resting on a case,
    /// and pre-faults the reusable launch background buffer so Frame 0 after A has no asset-load or page-fault spike.
    pub(crate) fn warm_focused_insert(&mut self, w: i32, h: i32) -> bool {
        if w <= 0 || h <= 0 || !matches!(self.theme, super::Theme::Cabinet | super::Theme::Shelf | super::Theme::Big) {
            return false;
        }
        let step = pocket_ui::scaled_layout::step().max(1) as usize;
        let need = (w as usize / step).saturating_mul(h as usize / step);
        if self.cabinet.launch_px.capacity() < need {
            self.cabinet.launch_px.resize(need, 0);
            // Touch one element per 4 KiB page during idle so the OS faults the backing pages before A is pressed.
            for i in (0..need).step_by(1024) {
                self.cabinet.launch_px[i] = 1;
            }
            self.cabinet.launch_px.clear();
        }
        let Some(item) = self.items.get(self.sel).copied().filter(|i| matches!(i, Item::Recent(_) | Item::Game(..))) else {
            return false;
        };
        let info = self.case_info(item);
        let medium = self.medium(item);
        let look = self.case_look(&info, medium);
        let shown = pocket_ui::insert_scene::scene_system(&info.system).to_string();
        let game = media::Game::of(&shown, &info.rom.to_string_lossy());
        let stage_k = media::shelf_scale_for(h) / media::SHELF_SCALE;
        let cozy = cabinet::starts_cozy(self.cozy(), pocket_ui::motion::level());
        let sp = pocket_ui::system_art::picture_id(&shown) == "gba-sp";
        let rig = if (cozy && pocket_ui::playful::on()) || sp {
            pocket_ui::insert_scene::Rigged::cached_scaled(&shown, game, pocket_ui::insert_rig::SCALE * stage_k)
        } else {
            None
        };
        if rig.is_none() {
            let _ = self.scene_sprite(&shown, h).or_else(|| self.scene_sprite(pocket_ui::insert_scene::STAND_IN, h));
        }
        let front = if self.theme == super::Theme::Cabinet {
            let key = CaseKey { rom: info.rom.clone(), medium, look, case: self.case_stickers_of(&info.rom), ajar: self.is_frozen_item(item), sealed: self.sealed(&info.rom), photo: self.spine_photo(&info.system, &info.rom).is_some() };
            self.cabinet.shown.get(&key).map(|(_, f, _, _)| f.clone())
        } else {
            self.cabinet.shelf_lit.as_ref().map(|(f, _, _)| f.clone())
        };
        let Some(front) = front else { return rig.is_some() };
        let has_cover = self.cover_file(&info.system, &info.rom).is_some();
        let seated = rig.as_ref().map_or(0, |r| r.rig.seated_h(r.scale));
        let _ = self.launch_game_pic(&shown, medium, game, &front, front.h, look, &info.title, has_cover, h, stage_k, seated, (&info.system, &info.rom));
        true
    }

    /// Idle pre-warm of the focused game's insert assets (150 ms after input, and once more at 500 ms once covers settle).
    pub(crate) fn insert_warm_tick(&mut self, now: Instant) -> Option<Instant> {
        const AFTER_MS: [u64; 2] = [150, 500];
        let idle = matches!(self.theme, super::Theme::Cabinet | super::Theme::Shelf | super::Theme::Big)
            && self.launch_at.is_none()
            && self.cabinet.eject.is_none()
            && self.card.is_none()
            && self.running.is_none()
            && self.menu.is_none()
            && self.switcher.is_none()
            && self.settings_page.is_none()
            && self.window.is_some();
        let item = self.items.get(self.sel).copied().filter(|i| idle && matches!(i, Item::Game(..) | Item::Recent(_)));
        let Some(item) = item else {
            self.cabinet.insert_warm = None;
            return None;
        };
        let tries = match self.cabinet.insert_warm {
            Some((i, n)) if i == item => n,
            _ => 0,
        };
        let Some(&after) = AFTER_MS.get(tries as usize) else { return None };
        let due = self.last_input + Duration::from_millis(after);
        if now < due {
            return Some(due);
        }
        let (w, h) = self.screen_size();
        self.cabinet_poll();
        pocket_ui::mark!("shelf-warm", "insert");
        self.warm_focused_insert(w as i32, h as i32);
        self.cabinet.insert_warm = Some((item, tries + 1));
        AFTER_MS.get(tries as usize + 1).map(|a| self.last_input + Duration::from_millis(*a))
    }

    pub(crate) fn paint_insert(&mut self, c: &mut Canvas, now: Instant) {
        self.paint_insert_with(c, now, false);
    }

    pub(crate) fn paint_insert_with(&mut self, c: &mut Canvas, now: Instant, already_dimmed: bool) {
        // Back from the game (or it never came up): the cabinet as it was, unless the case is on its way back (`eject`).
        if self.launch_at.is_none() && self.cabinet.eject.is_none() {
            self.recycle_insert();
        }
        // A sealed game's film is off (`app_shrink_wrap`): the rest of its case, its spine on the shelf, is made again
        // without it, once.
        if self.cabinet.insert.as_ref().is_some_and(|i| i.tear.as_ref().is_some_and(|t| !t.moving(now))) {
            if let Some(ins) = self.cabinet.insert.as_mut() {
                ins.tear = None;
            }
            self.cabinet_frozen_changed();
        }
        let Some(ins) = self.cabinet.insert.as_ref() else { return };
        let (system, medium, front, from, game) = (ins.scene.system.clone(), ins.scene.medium, ins.scene.front.clone(), ins.scene.from, ins.scene.kind);
        // On a short screen (1024x768) the console is as much smaller as the shelf's cases are (`scene_sprite`).
        let stage_k = media::shelf_scale_for(c.h) / media::SHELF_SCALE;
        // Its console's real mechanism where we have it (`insert_rig`): Cozy only, today's animation everywhere else.
        if !ins.scene.rig_asked {
            let t0 = Instant::now();
            if let Some(ins) = self.cabinet.insert.as_mut() {
                ins.scene.load_rig_scaled(pocket_ui::insert_rig::SCALE * stage_k);
            }
            self.cabinet.made("rig", t0);
        }
        let Some(ins) = self.cabinet.insert.as_ref() else { return };
        // The cartridge, card or disc, once: the game's process is already starting, this frame is the first of the
        // animation.
        if ins.scene.game.is_none() {
            let seated = ins.scene.rig.as_ref().map_or(0, |r| r.rig.seated_h(r.scale));
            let (look, title, has_cover, item) = (ins.scene.look, ins.scene.title.clone(), ins.scene.has_cover, ins.item);
            let info = self.case_info(item);
            let pic = self.launch_game_pic(&system, medium, game, &front, from.3, look, &title, has_cover, c.h, stage_k, seated, (&info.system, &info.rom));
            if let Some(ins) = self.cabinet.insert.as_mut() {
                ins.scene.game = pic;
            }
        }
        let has_rig = self.cabinet.insert.as_ref().is_some_and(|i| i.scene.rig.is_some());
        // Only plain (non-rig) scenes use the fitted console sprite; a system without a picture gets the stand-in crate.
        let sprite = if has_rig {
            None
        } else {
            self.scene_sprite(&system, c.h).or_else(|| self.scene_sprite(pocket_ui::insert_scene::STAND_IN, c.h))
        };
        // The Shelf look has no scene of its own: the console comes up onto its board, as the cases stand on it (18.11.:
        // on the screen's floor it hung over the board on some screens and stood in front of it on others).
        let floor = match self.cabinet.shelf_row.as_ref().filter(|_| self.theme == super::Theme::Shelf) {
            Some(row) => shelf_layout(c.w, c.h, row.tallest).floor,
            // In front of the cabinet, on its bottom board's top, a little back from its front edge over the hints: its
            // contact shadow shows on the board (tests/polish_1.rs).
            // On the Brick's 768 lines the scene's plinth reaches 12 lines past the board's (`scene_bottom`): the console stands
            // on the board as on the Plus, over the hints by the Plus's share, so the HuCard going into a PC Engine's front
            // clears the hint bar (Brick review 20.11.: it was cut by it).
            None => scene_bottom(c.h).min(pocket_ui::kit::content_bottom(c.h)) - (10.0 * stage_k) as i32,
        };
        // The console comes up from below the screen's edge, and the hint bar is not part of the scene: it stays where
        // it is, dimmed with everything else, and the console never passes over it (polish review 12.11.).
        // The band's screen rows are kept (at `n` = 2 from the screen row its top edge rounds to, as `rect` rounds).
        let band = hint_band_top(c.h).clamp(0, c.h);
        // For tests/brick_look_proportions.rs: the room between the stage's floor and the hint bar.
        pocket_ui::shares::mark("insert-stage", 0, floor, c.w, band - floor);
        let at = (c.screen_y(band).max(0) as usize * c.stride()).min(c.pixels().len());
        let mut kept = std::mem::take(&mut self.cabinet.hint_scratch);
        kept.clear();
        kept.extend_from_slice(&c.pixels()[at..]);
        let Some(ins) = self.cabinet.insert.as_mut() else {
            self.cabinet.hint_scratch = kept;
            return;
        };
        let elapsed = now.saturating_duration_since(ins.at);
        let dim = cabinet::insert_at(elapsed, ins.scene.cozy).dim;
        if already_dimmed {
            ins.scene.paint_dimmed(c, elapsed, sprite.as_deref(), pocket_ui::insert_scene::Stage { floor, k: stage_k });
        } else {
            ins.scene.paint(c, elapsed, sprite.as_deref(), pocket_ui::insert_scene::Stage { floor, k: stage_k });
        }
        // A sealed game's film flying off over it (`app_shrink_wrap`).
        if let Some(tear) = ins.tear.as_ref() {
            tear.paint(c, now);
        }
        c.pixels_mut()[at..].copy_from_slice(&kept);
        self.cabinet.hint_scratch = kept;
        if !already_dimmed {
            art::dim(c, band, c.h, 256 - (dim * 256.0) as u32);
        }
    }
}

impl App {
    /// Cozy's way back from a game (`pocket_ui::eject`), from `game_exited` once the shelf is made again: the last
    /// picture (`frame`, the pause menu's) switches off and the case comes back out of the console, over the live shelf.
    /// Only with Animations on Cozy and Motion on Full, in the insertion looks (including Easy and Kids), on the
    /// game just played; nothing waits for it, and any button ends it (`skip_eject`).
    pub(crate) fn begin_eject(&mut self, rom: &Path, system: &str, frame: Option<Arc<Image>>) {
        self.cabinet.eject = None;
        let why = if !matches!(self.theme, super::Theme::Cabinet | super::Theme::Shelf | super::Theme::Big) {
            "look"
        } else if !self.cozy() || pocket_ui::motion::level() != pocket_ui::motion::Motion::Full {
            "not Cozy with Motion Full"
        } else if !self.shelf_in_front() && self.kids_view() != Some(super::app_kids::KidsView::Shelf) {
            "the shelf is not in front"
        } else {
            ""
        };
        let item = self.items.get(self.sel).copied().filter(|i| matches!(i, Item::Recent(_) | Item::Game(..)));
        let why = match item {
            _ if !why.is_empty() => why,
            Some(i) if self.item_path(i) == Some(rom) => "",
            _ => "the shelf is not on the game",
        };
        let Some(item) = item.filter(|_| why.is_empty()) else {
            println!("eject: not shown ({why})");
            return;
        };
        let screen = pocket_ui::eject::Screen::of(FormFactor::of(system));
        let crt = pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::CrtOff);
        let off = frame.is_some() && (screen == pocket_ui::eject::Screen::Lcd || crt);
        // The whole picture switches off, as the screen showed it: the screen frame and the black round the game fall
        // together with the game, so the first picture of the way back is the last one of the game (11/13: the frame
        // vanished at once and the game stood smaller for a tick).
        let panel = if pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::HandheldOn) { pocket_ui::lcd_on::Panel::of(system) } else { pocket_ui::lcd_on::Panel::Plain };
        let photos = self.album_tucked(system, rom);
        let way = pocket_ui::eject::Way::new(screen, off, false);
        self.cabinet.eject = Some(Ejecting { at: pocket_ui::frame_clock::now(), way, frame, panel, item, scene_asked: false, ready: false, failed: None, photos });
    }

    /// Whether a start that failed is shown by the way back: Cozy with Motion Full, in the Cabinet or the Shelf look (the
    /// Easy shelf has its own page for it, Kids Mode its safe screen).
    pub(crate) fn failed_start_shown(&self) -> bool {
        self.cozy() && pocket_ui::motion::level() == pocket_ui::motion::Motion::Full && matches!(self.theme, super::Theme::Cabinet | super::Theme::Shelf) && !self.big_live() && !self.kids_live()
    }

    /// A game didn't start (`ran` after A: how far the launch animation had got): the medium comes back out of the console
    /// and goes into its case, the case back on the shelf, and the sentence (`start_failed`) comes when it is over, with
    /// the error sound. Nothing waits for it: any button ends it at once, uses the press up, and shows the sentence.
    /// Without it (another look, Motion below Full, the shelf not on the game) the notice stays as it was, at once.
    pub(crate) fn begin_failed_start(&mut self, rom: &Path, system: &str, name: &str, ran: Duration) {
        if !self.failed_start_shown() || self.notice.is_none() {
            return;
        }
        // Tests and snapshots (POCKETUI_MOCK) read the notice the moment a start fails, as the journeys and the navigation
        // explorer do; only the failed-start snapshot (`eject_snapshot`) plays the way back there.
        if pocket_ui::proc::mock() && std::env::var_os("POCKETUI_SNAPSHOT_EJECT_FAILED").is_none() {
            return;
        }
        // The shelf stands on the game again (a game's exit leaves it on the first row).
        if let Some(at) = self.items.iter().position(|i| self.item_path(*i) == Some(rom)) {
            self.select(at);
        }
        self.begin_eject(rom, system, None);
        let Some(notice) = self.notice.take().filter(|_| self.cabinet.eject.is_some()) else { return };
        let text = pocket_ui::start_failed::sentence(name, notice.text());
        let notice = notice.reworded(text);
        println!("start failed: {name}: {}", notice.text());
        if let Some(ej) = self.cabinet.eject.as_mut() {
            ej.failed = Some(FailedStart { ran, notice });
        }
    }

    /// The first frame of the way back: whether its console opens a lid or tray first (`insert_rig`, it starts shut), and
    /// the case's sound as the medium is back in it. Here, not in `begin_eject`: the shelf after Quit comes first.
    fn eject_ready(&mut self) {
        let Some((item, way)) = self.cabinet.eject.as_ref().filter(|e| !e.ready).map(|e| (e.item, e.way)) else { return };
        let info = self.case_info(item);
        let kind = media::Game::of(&info.system, &info.rom.to_string_lossy());
        let mechanism = pocket_ui::playful::on().then(|| pocket_ui::insert_rig::moving_kind(&info.system, kind)).flatten();
        let way = pocket_ui::eject::Way { lid: mechanism.is_some(), ..way };
        if let Some(ej) = self.cabinet.eject.as_mut() {
            ej.way = way;
            ej.ready = true;
        }
        let medium = self.medium(item);
        let Some(ej) = self.cabinet.eject.as_ref() else { return };
        let since = pocket_ui::frame_clock::now().saturating_duration_since(ej.at);
        // The way back's sounds, each queued so that its hit falls on its moment (`eject::Way::sounds`); a failed start's
        // way back has only the case's. With the session's photos the way back holds before its case shuts
        // (`eject::PhotoHold`): what comes after the hold comes that much later, and a soft click as the photos are in.
        let sounds = match ej.failed.as_ref() {
            Some(_) => vec![(pocket_ui::ui_sound::closing(medium), ej.case_shut_at())],
            None => ej.way.sounds(medium, kind, FormFactor::of(&info.system), mechanism),
        };
        let hold = ej.hold();
        for (sound, at) in sounds {
            let at = match hold {
                Some(h) if ej.failed.is_none() && at > h.from => at + pocket_ui::eject::PHOTO_HOLD,
                _ => at,
            };
            self.play_sound_after(sound, at.saturating_sub(since + pocket_ui::ui_sound::hit_at(sound)), 0.0);
        }
        if let Some(hold) = hold {
            let (click, at) = (pocket_ui::ui_sound::Sound::CardIn, hold.from + pocket_ui::eject::PHOTO_HOLD - Duration::from_millis(60));
            self.play_sound_after(click, at.saturating_sub(since + pocket_ui::ui_sound::hit_at(click)), 0.0);
        }
        // A failed start's sentence comes with its error sound, queued for the moment the sentence shows; a button that ends
        // the way back sooner drops it (`skip_eject`).
        if let Some(f) = ej.failed.as_ref() {
            let (sound, when) = (pocket_ui::ui_sound::Sound::Error, ej.way.failed(f.ran).len());
            self.play_sound_after(sound, when.saturating_sub(since), 0.0);
        }
    }

    /// Before a frame: where the way back is. The case goes backwards through the launch animation (`insert`, its clock
    /// set to the moment), following where the shelf paints it now; at the end it is the shelf's again.
    pub(crate) fn eject_tick(&mut self, now: Instant) {
        self.eject_ready();
        let Some(ej) = self.cabinet.eject.as_ref() else { return };
        let (item, asked) = (ej.item, ej.scene_asked);
        match ej.moment(now.saturating_duration_since(ej.at)) {
            pocket_ui::eject::Moment::Done => {
                self.end_eject();
                // Back in its place: an open case's game comes a little out of it (`paint_ajar_game`). Not `moved_at`: that
                // would turn the case in from its edge again and rock it, a jump right after the way back (11.11.).
                self.cabinet.ajar_at = Some(now);
            }
            pocket_ui::eject::Moment::Off(_) => {}
            pocket_ui::eject::Moment::Shelf { scene, .. } => {
                if !asked {
                    if let Some(ej) = self.cabinet.eject.as_mut() {
                        ej.scene_asked = true;
                    }
                    // From where the shelf painted the case last (the picture switching off was over it).
                    if self.begin_insert(item) {
                        let photos = self.cabinet.eject.as_ref().map(|e| e.photos.clone()).unwrap_or_default();
                        if let Some(ins) = self.cabinet.insert.as_mut() {
                            // Back from a game is always the Cozy way, a frozen game too (it goes back in quickly).
                            ins.scene.cozy = true;
                            // The session's photos go into the case with it, as instant photos at the case's size.
                            let s = ((ins.scene.front.h as f32 * 0.62 / pocket_ui::album::FRAME.1 as f32).round() as i32).max(1);
                            ins.scene.tucked = photos.iter().map(|img| std::rc::Rc::new(pocket_ui::album_paint::make_photo(s, (img.w, img.h), Some(img)))).collect();
                        }
                    }
                }
                let target = if self.theme == super::Theme::Cabinet {
                    self.cabinet.last_out.filter(|(i, _)| *i == item).map(|(_, r)| r)
                } else {
                    self.cabinet.shelf_lit.as_ref().map(|(f, x, y)| (*x, *y, f.w, f.h))
                };
                let tuck = self.cabinet.eject.as_ref().and_then(|e| e.tuck(now.saturating_duration_since(e.at)));
                if let Some(ins) = self.cabinet.insert.as_mut().filter(|i| i.item == item) {
                    ins.at = now.checked_sub(scene).unwrap_or(now);
                    ins.scene.tuck_p = tuck;
                    if let Some(r) = target {
                        ins.scene.from = r;
                    }
                }
            }
        }
    }

    /// Over the shelf, after it is painted: the last picture switching off, then black lifting off the shelf (the Shelf
    /// look's case is painted here; the Cabinet paints it with the cabinet).
    pub(crate) fn paint_eject(&mut self, c: &mut Canvas, now: Instant) {
        let Some(ej) = self.cabinet.eject.as_ref() else { return };
        let since = now.saturating_duration_since(ej.at);
        match ej.moment(since) {
            pocket_ui::eject::Moment::Off(p) => {
                let (frame, whole) = (ej.frame.as_deref(), (0, 0, c.w, c.h));
                match ej.way.screen {
                    // As dark as the pause menu showed it at first, not the bright picture (14.11. review, Fund 2).
                    pocket_ui::eject::Screen::Tv => pocket_ui::eject::paint_tv_off_from(c, frame, None, whole, p, pocket_ui::eject::PAUSE_FADE),
                    // A handheld's pause menu leaves its picture as it was (tests/eject.rs): its display goes off from it.
                    pocket_ui::eject::Screen::Lcd => pocket_ui::eject::paint_lcd_off_as(c, frame, None, whole, p, ej.panel),
                }
            }
            pocket_ui::eject::Moment::Shelf { black, .. } => {
                self.paint_insert(c, now);
                pocket_ui::eject::paint_black(c, black);
            }
            pocket_ui::eject::Moment::Done => {}
        }
    }

    /// The way back is playing (the shelf keeps drawing frames).
    pub(crate) fn ejecting(&self) -> bool {
        self.cabinet.eject.is_some()
    }

    /// Runs before trigger gestures can consume an input (L2/R2 open overlays themselves). True when the input is used up:
    /// the press that ends a failed start's way back is that and nothing else, or it would close the sentence it
    /// brings before it was read. The power and volume keys go on as always.
    pub(crate) fn eject_input(&mut self, event: &super::UserEvent) -> bool {
        use pocket_ui::pad::{Button, PadEvent};
        if matches!(event, super::UserEvent::Touch(_)) || matches!(event, super::UserEvent::Pad(pe) if pocket_ui::pad::counts_as_input(pe)) {
            let failed = self.cabinet.eject.as_ref().is_some_and(|e| e.failed.is_some());
            self.skip_eject();
            return failed && !matches!(event, super::UserEvent::Pad(PadEvent::Button { button: Button::Power | Button::VolUp | Button::VolDown, .. }));
        }
        false
    }

    /// A button or a touch during the way back: it ends at once (true when there was one), the press goes on to the
    /// shelf as always.
    pub(crate) fn skip_eject(&mut self) -> bool {
        if self.cabinet.eject.is_none() {
            return false;
        }
        self.end_eject();
        pocket_ui::ui_sound::forget_waiting();
        self.request_redraw();
        true
    }

    /// `POCKETUI_SNAPSHOT_EJECT_MS=<ms>` on `shelf:<system>:<look>:…`: the way back from the selected game that long after
    /// it ended, with `POCKETUI_SNAPSHOT_EJECT_FRAME` (a PNG or JPEG of the game) as its last picture. With
    /// `POCKETUI_SNAPSHOT_EJECT_SERIES=<folder>` every 60 Hz frame from the end of the game to a moment after the way back
    /// (`<folder>/0000.png` …, for a video). `POCKETUI_SNAPSHOT_EJECT_BENCH=1`: no picture, the line `bench-eject …` with
    /// what starting it costs, its frames and what a button does on its way (any moment: it ends, the shelf moves).
    /// True when it wrote what was asked.
    pub(crate) fn eject_snapshot(&mut self, buf: &mut [u32], w: u32, h: u32, out: &str) -> bool {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let bench = var("POCKETUI_SNAPSHOT_EJECT_BENCH").is_some();
        let series = var("POCKETUI_SNAPSHOT_EJECT_SERIES");
        let Some(ms) = var("POCKETUI_SNAPSHOT_EJECT_MS").and_then(|v| v.parse::<u64>().ok()).or((bench || series.is_some()).then_some(0)) else {
            return false;
        };
        let Some(item) = self.items.get(self.sel).copied() else { return false };
        let info = self.case_info(item);
        // The raw game frame as the screen showed it: a TV game at 4:3, a handheld's at its own shape, in the middle.
        let tv = pocket_ui::eject::Screen::of(FormFactor::of(&info.system)) == pocket_ui::eject::Screen::Tv;
        let frame = var("POCKETUI_SNAPSHOT_EJECT_FRAME").and_then(|p| image::load_image(Path::new(&p))).map(|raw| {
            let shape = if tv { 4.0 / 3.0 } else { raw.w as f32 / raw.h.max(1) as f32 };
            let (gw, gh) = if w as f32 / h as f32 > shape { ((h as f32 * shape) as u32, h) } else { (w, (w as f32 / shape) as u32) };
            let rect = ((w - gw) / 2, (h - gh) / 2, gw, gh);
            let mut px = vec![0u32; (w * h) as usize];
            Canvas::new(&mut px, w, h).blit_scaled(&raw, rect.0 as i32, rect.1 as i32, gw as i32, gh as i32);
            Arc::new(Image { w, h, px })
        });
        // `POCKETUI_SNAPSHOT_EJECT_SCREEN=<png>`: the pause menu's grab as it is, the whole screen with whatever frame and black
        // borders the game stood in (`write_shot`), instead of a game picture placed here.
        let frame = var("POCKETUI_SNAPSHOT_EJECT_SCREEN").and_then(|p| image::load_image(Path::new(&p))).map(Arc::new).or(frame);
        // `POCKETUI_SNAPSHOT_EJECT_FAILED=<ms>`: not a way back from a game but a start that failed that long after A (a missing
        // core), with the notice it leaves (`begin_failed_start`).
        let failed = var("POCKETUI_SNAPSHOT_EJECT_FAILED").and_then(|v| v.parse::<u64>().ok());
        let start = |app: &mut App, ago: Duration| {
            if let Some(ran) = failed {
                app.notice = Some(Notice::new(pocket_ui::fallback::core_missing_here(&info.title, "mgba")));
                app.begin_failed_start(&info.rom, &info.system, &info.title, Duration::from_millis(ran));
            } else {
                app.begin_eject(&info.rom, &info.system, frame.clone());
            }
            if let Some(ej) = app.cabinet.eject.as_mut() {
                ej.at = pocket_ui::frame_clock::now().checked_sub(ago).unwrap_or(ej.at);
            }
        };
        if bench {
            // What it costs, on the real clock.
            pocket_ui::frame_clock::thaw();
            self.eject_bench(item, start, buf, w, h);
            return true;
        }
        // Every frame the moment it stands for, however long the one before took (`frame_clock`).
        let t0 = pocket_ui::frame_clock::hold_settled();
        if let Some(dir) = series {
            if let Err(error) = std::fs::create_dir_all(&dir) {
                eprintln!("{error}");
                return false;
            }
            start(self, Duration::ZERO);
            let Some(ej) = self.cabinet.eject.as_ref() else { return false };
            let len = ej.len() + Duration::from_millis(700);
            let frames = (len.as_secs_f64() * 60.0).ceil() as u32;
            let mut done = None;
            for k in 0..frames {
                // The clock steps by exactly one 60 Hz frame; the way back, and after it the game's way out of its case
                // (`eject_tick` starts it), follow it.
                pocket_ui::frame_clock::freeze(t0 + pocket_ui::frame_clock::frames(k));
                if self.cabinet.eject.is_none() {
                    done.get_or_insert(k);
                }
                self.paint(&mut Canvas::new(buf, w, h));
                super::write_png(buf, w, h, &format!("{dir}/{k:04}.png"));
            }
            // Where the way back ended: the test of the hand-over to the still shelf compares the frames round it.
            println!("eject-series frames={frames} done={} notice={}", done.map_or(-1, |d| d as i64), self.notice.is_some());
            return true;
        }
        start(self, Duration::from_millis(ms));
        // Its first frame makes the case's way back from where the shelf painted it; the second is the moment asked.
        self.paint(&mut Canvas::new(buf, w, h));
        if let Some(ej) = self.cabinet.eject.as_mut() {
            ej.at = t0.checked_sub(Duration::from_millis(ms)).unwrap_or(ej.at);
        }
        self.paint(&mut Canvas::new(buf, w, h));
        super::write_png(buf, w, h, out);
        println!("eject-snapshot ms={ms} playing={} inserting={} notice={}", self.ejecting(), self.cabinet_inserting(), self.notice.is_some());
        true
    }

    /// The numbers of `eject_snapshot`'s bench: the start, each frame, and a button at moments along the way.
    fn eject_bench(&mut self, item: Item, start: impl Fn(&mut App, Duration), buf: &mut [u32], w: u32, h: u32) {
        // Costs as the UI thread's CPU time: what the way back takes, not how busy the computer is (a wall clock made
        // the gate's check of it depend on the load, 11.11.).
        let paint = |app: &mut App, buf: &mut [u32]| {
            let t = thread_cpu();
            app.paint(&mut Canvas::new(buf, w, h));
            thread_cpu().saturating_sub(t)
        };
        let mut shelf = Vec::new();
        for _ in 0..20 {
            shelf.push(paint(self, buf));
        }
        let t = thread_cpu();
        start(self, Duration::ZERO);
        let begin = thread_cpu().saturating_sub(t);
        let Some(ej) = self.cabinet.eject.as_ref() else {
            println!("bench-eject started=false");
            return;
        };
        let len = ej.way.len();
        let mut frames = Vec::new();
        let mut k = 0u64;
        let mut way = None;
        while self.ejecting() {
            let at = Duration::from_nanos(k * 16_666_667);
            if let Some(ej) = self.cabinet.eject.as_mut() {
                ej.at = Instant::now().checked_sub(at).unwrap_or(ej.at);
            }
            frames.push(paint(self, buf));
            way = way.or(self.cabinet.eject.as_ref().map(|e| e.way));
            k += 1;
            if k > 600 {
                break;
            }
        }
        // A button at moments along the way: the way back ends with it, and the press moves the shelf in the same
        // frame (the cursor on another case), nothing waits.
        let mut presses = Vec::new();
        for ms in [0u64, 100, 300, 700, 1200] {
            self.sel = self.items.iter().position(|i| *i == item).unwrap_or(self.sel);
            self.prev_sel = self.sel;
            self.cabinet_sync();
            paint(self, buf);
            start(self, Duration::from_millis(ms));
            paint(self, buf);
            let before = (self.sel, self.cabinet_cursor());
            let was_playing = self.ejecting();
            let t = thread_cpu();
            self.press(Button::Right);
            let handled = thread_cpu().saturating_sub(t);
            let frame = paint(self, buf);
            let moved = (self.sel, self.cabinet_cursor()) != before;
            presses.push(format!("{ms}:{}:{}:{}:{:.0}:{:.0}", was_playing, !self.ejecting() && !self.cabinet_inserting(), moved, handled.as_secs_f64() * 1e6, frame.as_secs_f64() * 1e6));
        }
        let mut aborts = Vec::new();
        use pocket_ui::pad::{Axis, PadEvent};
        for ms in [0u64, 700, 1200] {
            for button in Button::ALL {
                self.sel = self.items.iter().position(|i| *i == item).unwrap_or(self.sel);
                self.prev_sel = self.sel;
                start(self, Duration::from_millis(ms));
                self.eject_input(&super::UserEvent::Pad(PadEvent::Button { source: usize::MAX, button, pressed: true }));
                aborts.push(!self.ejecting() && !self.cabinet_inserting());
            }
            for axis in Axis::ALL {
                start(self, Duration::from_millis(ms));
                self.eject_input(&super::UserEvent::Pad(PadEvent::Axis { source: usize::MAX, axis, value: 0.9 }));
                aborts.push(!self.ejecting() && !self.cabinet_inserting());
            }
            start(self, Duration::from_millis(ms));
            self.eject_input(&super::UserEvent::Touch(pocket_ui::touch::TouchEvent { phase: pocket_ui::touch::Phase::Down, x: 0.5, y: 0.5 }));
            aborts.push(!self.ejecting() && !self.cabinet_inserting());
        }
        assert!(aborts.iter().all(|ended| *ended), "every input ends ejection before another overlay can consume it");
        println!(
            "{} screen={} off={} lid={} len_ms={} begin_us={:.0} shelf_p50_us={:.0} presses={}",
            bench_line("bench-eject", &frames),
            way.map_or_else(|| "none".to_string(), |w| format!("{:?}", w.screen)),
            way.is_some_and(|w| w.off),
            way.is_some_and(|w| w.lid),
            way.map_or(len, |w| w.len()).as_millis(),
            begin.as_secs_f64() * 1e6,
            {
                let mut s: Vec<f64> = shelf.iter().map(|d| d.as_secs_f64() * 1e6).collect();
                s.sort_by(|a, b| a.total_cmp(b));
                s[s.len() / 2]
            },
            presses.join(",")
        );
    }

    fn end_eject(&mut self) {
        if let Some(ej) = self.cabinet.eject.take() {
            if self.cabinet.insert.as_ref().is_some_and(|i| i.item == ej.item) && self.launch_at.is_none() {
                self.cabinet.insert = None;
            }
            // A failed start's sentence comes now, when the case is back (or the way back was ended): the notice, which
            // stays until A or B closes it.
            if let Some(f) = ej.failed {
                if self.notice.is_none() {
                    self.notice = Some(f.notice);
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
        // `POCKETUI_SNAPSHOT_OUTCOME_EACH=1`: the outcome line after every key, not only at the end (a walk along a shelf
        // in one run, `tests/cabinet_case_clear_of_tv.rs`); the open cases (`POCKETUI_SNAPSHOT_FROZEN`) from the start.
        let each = std::env::var_os("POCKETUI_SNAPSHOT_OUTCOME_EACH").is_some();
        if each {
            self.frozen_snapshot();
        }
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
            // This offline snapshot records the settled outcome (including an empty/missing-game notice).
            // Its first response was just painted above; live launches continue through the event loop.
            self.launch_response_painted();
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.pending_launch.is_some() && Instant::now() < deadline {
                self.launch_poll();
                if self.pending_launch.is_some() { std::thread::sleep(Duration::from_millis(1)); }
            }
            settle(self, ago);
            if each {
                self.cabinet_snapshot_outcome(h as i32, (w * h) as u64);
            }
        }
        if let Some(ms) = insert_ms {
            settle(self, rest);
            self.launch_animation_snapshot(Duration::from_millis(ms));
        }
    }

    /// Report the final painted case and shelf for host snapshot outcome checks. Uses the same
    /// rectangles as touch/launch, so a case clipped out of the picture cannot pass as visible.
    pub(crate) fn cabinet_snapshot_outcome(&self, h: i32, page_px: u64) {
        let Some((s, _)) = self.cabinet_cursor() else { return };
        let Some((_, (x, y, w, case_h))) = self.cabinet.last_out else { return };
        let top = self.cabinet.cab.y(s) - self.cabinet.scroll_y.value(Instant::now()).round() as i32;
        let rect = |r: Option<(i32, i32, i32, i32)>| r.map_or("none".to_string(), |(x, y, w, h)| format!("{x},{y},{w},{h}"));
        let place = rect(self.cabinet.tv.place);
        // The open case's game, where it stood out of the case (none: a shut case).
        let ajar = self.cabinet.last_ajar.map_or("none".to_string(), |(x, y, w, h)| format!("{x},{y},{w},{h}"));
        // The pixels the frame cleared, against the page's own (`page_px`, screen pixels; `gfx::take_cleared`): the whole
        // page once, not twice.
        let cleared = pocket_ui::gfx::take_cleared();
        println!("cabinet-outcome case={x},{y},{w},{case_h} shelf={top},{} showcase={} tv={} hints={} board={} place={place} ajar={ajar} handheld={} cleared={cleared}/{} console={} console_place={} window={} plate={}",
            top + self.cabinet.cab.shelves[s].height, self.cabinet.show_room,
            self.cabinet.tv.left.is_some(), pocket_ui::kit::hint_bar_top(h), self.cabinet.show_top.target().round() as i32, self.cabinet.tv.handheld, page_px,
            rect(self.cabinet.tv.console_at), rect(self.cabinet.tv.console_place), rect(self.cabinet.window_at), self.cabinet.plate_shown);
    }

    /// Playful animations in a Shelf snapshot: `POCKETUI_SNAPSHOT_HELD=<steps>:<ms>` paints Right held for `steps`
    /// steps 70 ms apart (each frame on its own clock, `shelf_clock`), then the frame `ms` after the last step: the
    /// spines leaning or swinging back. `POCKETUI_SNAPSHOT_MOVED_MS=<ms>`: the selection that long after it moved (the
    /// spine springing up, the front's wiggle).
    pub(crate) fn shelf_playful_snapshot(&mut self, c: &mut Canvas) {
        let env = |k: &str| std::env::var(k).ok();
        if let Some(ms) = env("POCKETUI_SNAPSHOT_MOVED_MS").and_then(|v| v.parse::<u64>().ok()) {
            self.moved_at = Instant::now() - Duration::from_millis(ms);
            self.paint(c);
        }
        let Some((steps, ms)) = env("POCKETUI_SNAPSHOT_HELD").and_then(|v| {
            let (a, b) = v.split_once(':')?;
            Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?))
        }) else {
            return;
        };
        let step = Duration::from_millis(70);
        let base = Instant::now() - step * steps as u32 - Duration::from_millis(ms);
        for k in 0..steps {
            let at = base + step * k as u32;
            if self.sel + 1 < self.items.len() {
                self.sel += 1;
            }
            self.moved_at = at;
            // A frame every 35 ms of the hold, as the device paints (the glide is set on the first).
            for f in 0..2 {
                self.cabinet.shelf_clock = Some(at + Duration::from_millis(35 * f));
                self.paint(c);
            }
        }
        self.cabinet.shelf_clock = None;
        self.paint(c);
    }

    /// `--snapshot bench-playful:<system>:<launch|pull|lean|walk|walk-warm>` (docs/LATENCY.md "Playful animations"): every frame of
    /// one animation in the Shelf look painted as the device would, 16 ms apart, on the system's list of games; prints
    /// the frames' thread CPU time (`bench_line`). Run with the extra on and off (POCKETUI_DATA's settings.json) to
    /// compare.
    pub(crate) fn bench_playful(&mut self, rest: &str, w: u32, h: u32) {
        let (system, what) = rest.split_once(':').unwrap_or((rest, "launch"));
        self.shelf_snapshot(&format!("{system}:shelf:games:0"));
        // A small panel's frame as on the device (`ScaledFrame::render`: painted natively, or the design canvas and its
        // reduction with POCKETUI_PAINT=scaled), with what `draw` paints over the page. Before 14.11. this painted the
        // design canvas alone, so the numbers were neither way's (docs/LATENCY.md "Launch frames on the H700").
        let panel = crate::snapshot_panel_dims();
        let direct = pocket_ui::scaled_layout::logical_size(panel) != (w, h);
        let mut scaler = pocket_ui::scaled_layout::ScaledFrame::default();
        let mut buf = vec![0u32; if direct { (w * h) as usize } else { panel.0 as usize * panel.1 as usize }];
        let mut times = Vec::new();
        let frame_ms = Duration::from_millis(16);
        let mut paint = |app: &mut App, times: &mut Vec<Duration>| {
            let c0 = thread_cpu();
            app.device_frame(&mut scaler, &mut buf, panel, direct.then_some((w, h)));
            times.push(thread_cpu().saturating_sub(c0));
        };
        // The reference: the shelf itself, a frame of it standing still, on the same computer (tests/launch_frames.rs
        // holds the launch's frames to a budget of these).
        let mut still = Vec::new();
        for _ in 0..60 {
            paint(self, &mut still);
        }
        let pages = LAUNCH_PAGES.get();
        match what {
            "pull" => {
                // Ten moves to the right, every frame of each one's spring, turn and wiggle (700 ms).
                for _ in 0..10 {
                    self.sel = (self.sel + 1).min(self.items.len().saturating_sub(1));
                    let at = Instant::now();
                    for k in 0..44u32 {
                        self.moved_at = at.checked_sub(frame_ms * k).unwrap_or(at);
                        paint(self, &mut times);
                    }
                }
            }
            "rests" | "rests-cabinet" => {
                // Nothing floats (`pocket_ui::scene_check`): every selection of the row (Cabinet: every case of every
                // shelf), each at moments of its animation (the case before going back, this one coming out), every
                // object checked in the model the painters recorded. Prints how many frames and every fault
                // (`rests frames=… faults=…`).
                let cabinet = what == "rests-cabinet";
                let stops: Vec<(usize, usize)> = if cabinet {
                    self.shelf_snapshot(&format!("{system}:cabinet"));
                    self.cabinet_shelves();
                    let shelves = &self.cabinet.cab.shelves;
                    (0..shelves.len()).flat_map(|s| (0..shelves[s].cases.len()).map(move |i| (s, i))).collect()
                } else {
                    (0..self.items.len()).map(|k| (0, k)).collect()
                };
                // Open cases too, where the snapshot's env asks for them (`POCKETUI_SNAPSHOT_FROZEN`).
                self.frozen_snapshot();
                pocket_ui::scene_check::record(true);
                let (mut frames, mut faults) = (0, Vec::new());
                let mut kinds = std::collections::BTreeSet::new();
                let base = Instant::now() - Duration::from_secs(36_000);
                for (k, &(s, i)) in stops.iter().enumerate() {
                    let at = base + Duration::from_secs(k as u64);
                    pocket_ui::frame_clock::freeze(at);
                    if cabinet {
                        self.cabinet_go((s, i));
                    } else {
                        self.prev_sel = self.sel;
                        self.sel = i;
                    }
                    self.moved_at = at;
                    for ms in [0u64, 30, 60, 100, 160, 250, 400, 800] {
                        let t = at + Duration::from_millis(ms);
                        self.cabinet.shelf_clock = Some(t);
                        pocket_ui::frame_clock::freeze(t);
                        pocket_ui::scene_check::take();
                        paint(self, &mut times);
                        frames += 1;
                        let (objs, surfaces) = pocket_ui::scene_check::take();
                        // What was seen, without the numbers ("shelf # case #, out").
                        kinds.extend(objs.iter().map(|o| o.what.replace(|c: char| c.is_ascii_digit(), "#").replace("##", "#").replace("##", "#")));
                        kinds.extend(surfaces.iter().map(|s| format!("surface: {}", s.what)));
                        for f in pocket_ui::scene_check::check(&objs, &surfaces) {
                            faults.push(format!("shelf {s}, case {i} selected, {ms} ms: {f}"));
                        }
                    }
                }
                self.cabinet.shelf_clock = None;
                pocket_ui::scene_check::record(false);
                println!("rests frames={frames} faults={}", faults.len());
                println!("rests seen: {}", kinds.into_iter().collect::<Vec<_>>().join(" | "));
                for f in faults.iter().take(40) {
                    println!("rests fault: {f}");
                }
            }
            "rests-launch" | "rests-launch-cabinet" => {
                // Nothing floats while a game goes in (`scene_check`): every game of the row started, ten moments of the
                // way into its console each.
                pocket_ui::scene_check::record(true);
                let (mut frames, mut faults) = (0, Vec::new());
                let mut kinds = std::collections::BTreeSet::new();
                let stops = self.rests_stops(system, what.ends_with("-cabinet"));
                for (k, &at) in stops.iter().enumerate() {
                    self.rests_select(at);
                    if what.ends_with("-cabinet") {
                        // The Cabinet starts a game from its case out of the shelf, where a frame painted it (`last_out`):
                        // without that frame no scene began and only the shelf was checked.
                        paint(self, &mut times);
                    }
                    self.launch_animation_snapshot(Duration::ZERO);
                    pocket_ui::frame_clock::thaw();
                    let len = self.cabinet.insert.as_ref().map_or(cabinet::INSERT_COZY, |i| i.scene.len());
                    let start = Instant::now();
                    for step in 0..=10u32 {
                        let at = start.checked_sub(len.mul_f32(step as f32 / 10.0)).unwrap_or(start);
                        if let Some(ins) = self.cabinet.insert.as_mut() {
                            ins.at = at;
                        }
                        self.launch_at = Some(at);
                        pocket_ui::scene_check::take();
                        paint(self, &mut times);
                        frames += 1;
                        let (objs, surfaces) = pocket_ui::scene_check::take();
                        kinds.extend(objs.iter().map(|o| o.what.replace(|c: char| c.is_ascii_digit(), "#").replace("##", "#").replace("##", "#")));
                        for f in pocket_ui::scene_check::check(&objs, &surfaces).into_iter().chain(pocket_ui::scene_check::console_twice(&objs, &surfaces)) {
                            faults.push(format!("game {k} going in, {}0 %: {f}", step));
                        }
                    }
                    self.cabinet.insert = None;
                    self.launch_at = None;
                }
                pocket_ui::scene_check::record(false);
                println!("rests frames={frames} faults={}", faults.len());
                println!("rests seen: {}", kinds.into_iter().collect::<Vec<_>>().join(" | "));
                for f in faults.iter().take(40) {
                    println!("rests fault: {f}");
                }
            }
            "rests-eject" | "rests-eject-cabinet" => {
                // Nothing floats on the way back from a game (`scene_check`): every game of the row, its way out of the
                // console and back into its case on the shelf, every 50 ms of it.
                pocket_ui::scene_check::record(true);
                let (mut frames, mut faults) = (0, Vec::new());
                let mut kinds = std::collections::BTreeSet::new();
                let stops = self.rests_stops(system, what.ends_with("-cabinet"));
                for (k, &at) in stops.iter().enumerate() {
                    self.rests_select(at);
                    let info = self.case_info(self.items[self.sel]);
                    let t0 = pocket_ui::frame_clock::hold_settled();
                    self.begin_eject(&info.rom, &info.system, None);
                    let len = self.cabinet.eject.as_ref().map_or(Duration::ZERO, |e| e.len()) + Duration::from_millis(700);
                    for step in 0..=(len.as_millis() / 50) as u32 {
                        pocket_ui::frame_clock::freeze(t0 + Duration::from_millis(step as u64 * 50));
                        pocket_ui::scene_check::take();
                        paint(self, &mut times);
                        frames += 1;
                        let (objs, surfaces) = pocket_ui::scene_check::take();
                        kinds.extend(objs.iter().map(|o| o.what.replace(|c: char| c.is_ascii_digit(), "#").replace("##", "#").replace("##", "#")));
                        for f in pocket_ui::scene_check::check(&objs, &surfaces).into_iter().chain(pocket_ui::scene_check::console_twice(&objs, &surfaces)) {
                            faults.push(format!("game {k} coming back, {} ms: {f}", step * 50));
                        }
                    }
                    self.end_eject();
                    self.cabinet.insert = None;
                }
                pocket_ui::scene_check::record(false);
                println!("rests frames={frames} faults={}", faults.len());
                println!("rests seen: {}", kinds.into_iter().collect::<Vec<_>>().join(" | "));
                for f in faults.iter().take(40) {
                    println!("rests fault: {f}");
                }
            }
            "walk" | "walk-warm" => {
                // Lived-in shelf: a calm walk along the row, Right every 200 ms to its end, a frame every 16 ms; with
                // "walk-warm" the row's posed spines were made ahead first, as the idle moments after opening the row do
                // (`lived_warm_tick`). Prints how many were made during the walk and its worst frame.
                if what == "walk-warm" {
                    let t0 = thread_cpu();
                    let made = LIVED_POSED.get();
                    self.lived_warm(Duration::MAX);
                    println!("lived warm posed={} warm_us={:.0}", LIVED_POSED.get() - made, thread_cpu().saturating_sub(t0).as_secs_f64() * 1e6);
                }
                let made = LIVED_POSED.get();
                let n = self.items.len().saturating_sub(1);
                let base = Instant::now() - Duration::from_secs(60);
                for k in 0..(n as u32 * 200 + 600) / 16 {
                    let t = base + frame_ms * k;
                    let want = ((k * 16 / 200) as usize).min(n);
                    if want != self.sel {
                        self.sel = want;
                        self.moved_at = t;
                    }
                    self.cabinet.shelf_clock = Some(t);
                    paint(self, &mut times);
                }
                self.cabinet.shelf_clock = None;
                let worst = times.iter().max().copied().unwrap_or_default();
                println!("lived walk posed_in_walk={} worst_us={:.0}", LIVED_POSED.get() - made, worst.as_secs_f64() * 1e6);
            }
            "lean" => {
                // Right held for 20 steps 70 ms apart, then the swing back: a frame every 16 ms of it.
                let base = Instant::now() - Duration::from_millis(3000);
                for k in 0..(20 * 70 + 1000) / 16 {
                    let t = base + frame_ms * k;
                    let step = (k * 16 / 70).min(20) as usize;
                    let want = step.min(self.items.len().saturating_sub(1));
                    if want != self.sel {
                        self.sel = want;
                        self.moved_at = t;
                    }
                    self.cabinet.shelf_clock = Some(t);
                    paint(self, &mut times);
                }
                self.cabinet.shelf_clock = None;
            }
            _ => {
                // The launch: from A to the end, five times.
                for _ in 0..5 {
                    self.launch_animation_snapshot(Duration::ZERO);
                    // A bench: what painting costs, on the real clock.
                    pocket_ui::frame_clock::thaw();
                    let len = self.cabinet.insert.as_ref().map_or(cabinet::INSERT_COZY, |i| i.scene.len());
                    let start = Instant::now();
                    for k in 0..=(len.as_millis() / 16) as u32 {
                        let at = start.checked_sub(frame_ms * k).unwrap_or(start);
                        if let Some(ins) = self.cabinet.insert.as_mut() {
                            ins.at = at;
                        }
                        self.launch_at = Some(at);
                        paint(self, &mut times);
                    }
                    // More of the reference between the launches, on the computer as it is now (19.11.: taken only
                    // before the first launch, the shelf's frame went from 91 to 227 us between runs on one machine,
                    // and tests/launch_frames.rs's ratio with it).
                    self.cabinet.insert = None;
                    self.launch_at = None;
                    for _ in 0..12 {
                        paint(self, &mut still);
                    }
                }
                self.cabinet.insert = None;
                self.launch_at = None;
            }
        }
        let mut sorted = still.clone();
        sorted.sort();
        let shelf_us = sorted.get(sorted.len() / 2).map_or(0.0, |d| d.as_secs_f64() * 1e6);
        let (feet, feet_n) = LIVED_FEET.get();
        let (rowt, row_n) = LIVED_ROW.get();
        println!(
            "lived first_frame_us={:.0} still_p50_us={shelf_us:.0} row_builds={row_n} row_build_us={:.0} feet_frames={feet_n} feet_us_per_frame={:.2} feet_max_total_us={:.0}",
            still.first().map_or(0.0, |d| d.as_secs_f64() * 1e6),
            rowt.as_secs_f64() * 1e6,
            if feet_n > 0 { feet.as_secs_f64() * 1e6 / feet_n as f64 } else { 0.0 },
            feet.as_secs_f64() * 1e6
        );
        println!("{} shelf_p50_us={shelf_us:.0} pages={}", bench_line(&format!("bench-playful {system}:{what} playful={}", pocket_ui::playful::on()), &times), LAUNCH_PAGES.get() - pages);
    }

    /// The places `rests-launch` and `rests-eject` start a game from: each of the row's games, or in the Cabinet look
    /// (`cabinet`) the first eight cases of each shelf.
    fn rests_stops(&mut self, system: &str, cabinet: bool) -> Vec<(usize, usize)> {
        self.frozen_snapshot();
        if !cabinet {
            return (0..self.items.len()).map(|k| (0, k)).collect();
        }
        self.shelf_snapshot(&format!("{system}:cabinet"));
        self.cabinet_shelves();
        let shelves = &self.cabinet.cab.shelves;
        (0..shelves.len()).flat_map(|s| (0..shelves[s].cases.len().min(8)).map(move |i| (s, i))).collect()
    }

    /// Selects stop `(s, i)` of `rests_stops`, settled (pulled out long ago).
    fn rests_select(&mut self, (s, i): (usize, usize)) {
        if self.theme == super::Theme::Cabinet {
            self.cabinet_go((s, i));
        } else {
            self.sel = i;
        }
        self.prev_sel = self.sel;
        self.moved_at = Instant::now() - Duration::from_secs(5);
    }

    /// The launch animation `ago` after A, for a snapshot: the game itself isn't started.
    pub(crate) fn launch_animation_snapshot(&mut self, ago: Duration) {
        let Some(item) = self.items.get(self.sel).copied() else { return };
        if self.begin_insert(item) {
            self.apply_power_on(self.cabinet.insert.as_ref().is_some_and(|i| i.scene.cozy), true);
            // One moment: the clock stands while it is painted (`frame_clock`).
            let now = pocket_ui::frame_clock::hold_settled();
            if let Some(ins) = self.cabinet.insert.as_mut() {
                ins.at = now - ago;
            }
            self.launch_at = Some(now - ago);
            // A sealed game's film tears off first, as at a real start.
            let (rom, cozy) = (self.case_info(item).rom, self.cabinet.insert.as_ref().is_some_and(|i| i.scene.cozy));
            self.tear_wrap(&rom, cozy, now - ago);
            println!("launch-wrap tearing={}", self.cabinet.insert.as_ref().is_some_and(|i| i.tear.is_some()));
        }
    }

    /// `POCKETUI_SNAPSHOT_LAUNCH_SERIES=<folder>` on `shelf:<system>:<look>:…`: every 60 Hz frame of the launch animation
    /// from A to its end (`<folder>/0000.png` …), and the line `launch-series frames=… ` with the paint times
    /// (`bench_line`, PNG writing not counted). For the frame-sequence tests (no jump over a threshold) and contact sheets.
    /// True when it wrote the series.
    pub(crate) fn launch_series_snapshot(&mut self, buf: &mut [u32], w: u32, h: u32) -> bool {
        let Some(dir) = std::env::var("POCKETUI_SNAPSHOT_LAUNCH_SERIES").ok().filter(|v| !v.is_empty()) else { return false };
        if let Err(error) = std::fs::create_dir_all(&dir) {
            eprintln!("{error}");
            return false;
        }
        let Some(item) = self.items.get(self.sel).copied() else { return false };
        if std::env::var_os("POCKETUI_SNAPSHOT_WARM_FOCUS").is_some_and(|v| !v.is_empty() && v != "0") {
            let canvas = super::snap_canvas(buf, w, h);
            let (cw, ch) = (canvas.w, canvas.h);
            self.warm_focused_insert(cw, ch);
        }
        if !self.begin_insert(item) {
            println!("launch-series started=false");
            return true;
        }
        let cozy = self.cabinet.insert.as_ref().is_some_and(|i| i.scene.cozy);
        // As a hosted game's start: a handheld ends in its display's light (`POCKETUI_SNAPSHOT_NO_POWER_ON=1`: the old fade).
        let power_on = std::env::var_os("POCKETUI_SNAPSHOT_NO_POWER_ON").is_none() && self.apply_power_on(cozy, true);
        let hold = self.cabinet.insert.as_ref().map_or_else(|| pocket_ui::insert_scene::hold(cozy, power_on), |i| i.scene.hold());
        let len = hold.unwrap_or(cabinet::insert_len(cozy) + pocket_ui::insert_scene::FADE);
        // A at `t0`; the clock steps by exactly one 60 Hz frame (`frame_clock`).
        let t0 = pocket_ui::frame_clock::hold_settled();
        if let Some(ins) = self.cabinet.insert.as_mut() {
            ins.at = t0;
        }
        self.launch_at = Some(t0);
        // A sealed game's film tears off first, as at a real start; the scene and its fade come that much later.
        let rom = self.case_info(item).rom;
        let len = len + self.tear_wrap(&rom, cozy, t0);
        let frames = (len.as_secs_f64() * 60.0).ceil() as u32;
        let pages_before = LAUNCH_PAGES.get();
        let rigs_before = pocket_ui::insert_scene::rig_loads();
        let games_before = LAUNCH_GAME_BUILDS.get();
        let print_frames = std::env::var_os("POCKETUI_LAUNCH_SERIES_TIMES").is_some_and(|v| !v.is_empty() && v != "0");
        let skip_png = std::env::var_os("POCKETUI_LAUNCH_SERIES_NO_PNG").is_some_and(|v| !v.is_empty() && v != "0");
        let mut times = Vec::new();
        for k in 0..frames {
            pocket_ui::frame_clock::freeze(t0 + pocket_ui::frame_clock::frames(k));
            let t = Instant::now();
            let cpu0 = if print_frames { thread_cpu() } else { Duration::ZERO };
            // Painted as the shelf shot before it and as a real start: natively on a small panel (`snap_canvas`). A design
            // canvas here was written out as if it were the panel's pixels (2x text, every other row).
            self.paint(&mut super::snap_canvas(buf, w, h));
            let dt = t.elapsed();
            times.push(dt);
            if print_frames {
                let cpu = thread_cpu().saturating_sub(cpu0);
                println!("launch-frame k={k} wall_us={:.0} cpu_us={:.0}", dt.as_secs_f64() * 1e6, cpu.as_secs_f64() * 1e6);
            }
            if !skip_png {
                super::write_png(buf, w, h, &format!("{dir}/{k:04}.png"));
            }
        }
        let pages = LAUNCH_PAGES.get().saturating_sub(pages_before);
        let rig_loads = pocket_ui::insert_scene::rig_loads().saturating_sub(rigs_before);
        let game_builds = LAUNCH_GAME_BUILDS.get().saturating_sub(games_before);
        println!("launch-series {} cozy={cozy} pages={pages} rig_loads={rig_loads} game_builds={game_builds}", bench_line("paint", &times));
        true
    }
}

/// Frame times as one line: count, p50, p95, max (microseconds) and how many took longer than a 60 Hz frame.
pub(crate) fn bench_line(label: &str, times: &[Duration]) -> String {
    let mut us: Vec<f64> = times.iter().map(|d| d.as_secs_f64() * 1e6).collect();
    us.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f64| us.get(((us.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(0.0);
    let over = us.iter().filter(|u| **u > 16_667.0).count();
    format!("{label} frames={} p50_us={:.0} p95_us={:.0} max_us={:.0} over_16ms={over}", us.len(), at(0.5), at(0.95), at(1.0))
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
    // Under the top line and over the hints, both in 960's share of a shorter screen (`kit::chrome`: the Brick's 768).
    let top = if h < 600 { 80 } else { pocket_ui::kit::chrome(104, h) };
    // The hint bar starts `kit::hint_bar_top` (72 px over the bottom edge at Normal text): the board stands over it. The size of the
    // spines is still the one the room before the bar gave (112 px), so the Mini V2 keeps its three pixels a template pixel.
    let bottom = pocket_ui::kit::content_bottom(h);
    let content = (h - pocket_ui::kit::chrome(112, h) - top).max(200);
    // The Brick's 1024x768 gets 2.0, the Plus's 1280x960 2.5: the same share of the height (0.31). Until 0.6.1 the
    // Brick got 1.5, spines a quarter smaller, because at 2.0 the room above the shelf shrank under the frame made for
    // 960 lines and the shown case lost its facts and Curator's tile its console's name (tests/home_panel_labels.rs);
    // the frame's lines in 960's share leave that room.
    let f = ((content as f32 * 0.42 / TALLEST * 2.0).floor() / 2.0).clamp(1.0, 4.0);
    let board_h = (8.0 * f).round() as i32;
    let floor = bottom - board_h;
    // 16 template pixels: 10 (30 px at 1080 lines, 12 on 640x480) was too quiet beside cases of one colour.
    let lift = (16.0 * f).round() as i32;
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
/// of cartridges, Just Play's cartridge and dice, the monitor of PC streaming, a sticker's sheet,
/// and from the empty pages' vignettes Favourites' star on its cushion, the search's glass, a smart shelf's row of
/// cartridges and the history's notebook.
fn emblem(item: Item) -> Option<&'static pocket_ui::system_art::Sprite> {
    use pocket_ui::ui_art::{get, Art, Emblem, Sticker};
    get(match item {
        Item::Curator => Art::Emblem(Emblem::Curator),
        Item::JustPlay => Art::Emblem(Emblem::JustPlay),
        Item::Streaming => Art::Emblem(Emblem::Streaming),
        Item::Sticker(_) => Art::Emblem(Emblem::Sticker),
        Item::Favorites => Art::NoFavorites,
        Item::Search | Item::Lens => Art::Search,
        Item::SmartShelf(_) => Art::Sticker(Sticker::TenGames),
        Item::History | Item::Review(_) | Item::Rediscover => Art::History,
        Item::Crate => return Some(super::app_crate::crate_art()),
        Item::Recent(_) | Item::Game(..) | Item::System(_) | Item::Series(_) => return None,
    })
}

/// How much larger than up to 0.5 the Shelf look shows the selected case, where the room over the shelf lets it (a
/// player on a TrimUI Brick asked for larger covers, r/SBCGaming 17.11.2026; the owner chose 1.4 of the old size).
const SHOW_BIGGER: f32 = 1.4;
/// The most of the room's width the grown front may take: the words beside it keep the other half.
const SHOW_WIDE: f32 = 0.5;
/// The least a front grows by at all (`show_k`); under that it keeps its old size.
const SHOW_GROW_LEAST: f32 = 1.08;
/// The least of the room's width the words beside a grown front keep when its case stands open, its game out at its
/// side (`AJAR_FRONT`, Continue).
const WORDS_LEAST: f32 = 0.25;

/// The `k` (`media::front`) of a case of `m` shown above the shelf whose row holds the cases `row`: `SHOW_BIGGER` times
/// the size it had up to 0.5 (`show_k_before`), as far as the room lets the largest case of the row (its height, and
/// `SHOW_WIDE` of its width, less where an open case's game beside it would leave the words under `WORDS_LEAST`), never
/// smaller than that old size, and not at all where that is under `SHOW_GROW_LEAST`. One scale for the cases of a row, so the ones you see
/// side by side keep their real sizes to each other; a row of cartridge boxes (a system's page) grows more than one
/// with DVD cases in it (Home).
pub(crate) fn show_k(lay: &ShelfLayout, m: Medium, row: &[Medium]) -> f32 {
    let before = show_k_before(lay, m) * media::shelf_scale();
    let fit = row
        .iter()
        .chain(std::iter::once(&m))
        .map(|m| {
            let t = m.template_size();
            (lay.show.3 as f32 / t.front_h as f32).min(lay.show.2 as f32 * SHOW_WIDE / t.front as f32)
        })
        .fold(f32::MAX, f32::min);
    // The case, the 48 px gap and the game out at its side leave the words their share.
    let words = |m: Medium| (lay.show.2 as f32 * (1.0 - WORDS_LEAST) - 48.0) / (1.0 + AJAR_FRONT) / m.template_size().front as f32;
    let fit = row.iter().chain(std::iter::once(&m)).map(|m| words(*m)).fold(fit, f32::min);
    // In tenths (a hair over, so 3.0 * 1.4 stays 4.2 and doesn't fall to 4.1): the same size for every case of the row.
    let grown = ((before * SHOW_BIGGER).min(fit) * 10.0 + 0.001).floor() / 10.0;
    // A few per cent larger is not worth the words' room: on 720x720 Continue 2 % made the name a size smaller.
    if grown < before * SHOW_GROW_LEAST {
        return before / media::shelf_scale();
    }
    grown / media::shelf_scale()
}

/// The `k` of a case of `m` shown above the shelf as it was up to 0.5: as large as the room lets the largest case of
/// any kind (at most 45 % of the room's width and `SHOW_MAX` template pixels a pixel), in half steps, the same for every
/// case; below one (a tiny screen) as large as fits. `show_k` grows it.
fn show_k_before(lay: &ShelfLayout, m: Medium) -> f32 {
    // One scale for every case, the one the largest needs to fit the room, so they keep their real sizes to each other;
    // never under `SHOW_LEAST` of a case's own. A taller spine (a SNES box standing on its short side) is laid down to
    // the front's height as it turns (`cabinet_art::draw_turning`).
    let own = |m: Medium| {
        let t = m.template_size();
        (lay.show.3 as f32 / t.front_h as f32).min(lay.show.2 as f32 * 0.45 / t.front as f32)
    };
    let common = Medium::ALL.into_iter().map(own).fold(f32::MAX, f32::min);
    let s = common.max(own(m) * SHOW_LEAST);
    let s = if s >= 1.0 { ((s * 2.0).floor() / 2.0).min(SHOW_MAX) } else { s.max(0.2) };
    s / media::shelf_scale()
}

/// The Shelf look's meta line ("Game Boy Advance, played a year ago today") in at most `fit` lines `width` wide, and
/// whether they are translated already: whole where it can be (`meta_whole`), else wrapped, the last line shown ending with
/// "…" (`meta_choice` tries the short line first).
fn meta_wrap(text: &mut Text, meta: &str, width: i32, fit: usize) -> (Vec<String>, bool) {
    if fit >= 2 {
        if let Some(whole) = meta_whole(text, meta, width, 2) {
            return whole;
        }
    }
    (text.wrap_at_most(Face::Body, 28, meta, width, fit), false)
}

/// The meta line whole in at most `k` lines `width` wide, and whether they are translated already; None where it takes more.
/// Two lines break at its first ", " or " · " where both parts fit: the system on the first line, why the game is there
/// whole on the second. Wrapped at the width it left a word alone ("Game Boy Advance, heute vor einem Jahr" over
/// "gespielt", 640x480, Large German, once the shown box grew 1.4 times, 17.11.2026); else wrapped by words.
fn meta_whole(text: &mut Text, meta: &str, width: i32, k: usize) -> Option<(Vec<String>, bool)> {
    let shown = text.translated(meta).to_string();
    if text.width_plain(Face::Body, 28, &shown) <= width {
        return Some((vec![shown], true));
    }
    if k < 2 {
        return None;
    }
    if k > 2 {
        // Its parts as they come, as many on a line as fit, a line broken only between them ("Super Nintendo" /
        // "Durchgespielt" / "vor 9 Tagen · 8 h", not "vor 9" / "Tagen"); by words where a part alone is too wide. A break
        // at " · " leaves the dot out, as the two-line break does (review 19.11. #15: some lines ended "·", their
        // neighbours not); a comma stays.
        let mut lines: Vec<String> = Vec::new();
        let mut rest = shown.as_str();
        while !rest.is_empty() {
            let mut cuts: Vec<(usize, &str)> = rest.match_indices(", ").chain(rest.match_indices(" · ")).collect();
            cuts.sort();
            let end = |(at, sep): (usize, &str)| if sep == ", " { at + 1 } else { at };
            let fits = cuts.iter().rev().copied().find(|c| text.width_plain(Face::Body, 28, rest[..end(*c)].trim_end()) <= width);
            let (line, next) = match fits {
                _ if text.width_plain(Face::Body, 28, rest) <= width => (rest, ""),
                Some((at, sep)) => (rest[..end((at, sep))].trim_end(), rest[at + sep.len()..].trim_start()),
                None => break,
            };
            lines.push(line.to_string());
            rest = next;
        }
        if rest.is_empty() && lines.len() <= k {
            return Some((lines, true));
        }
        let lines = text.wrap_prose(Face::Body, 28, meta, width);
        return (lines.len() <= k && !lines.iter().any(|l| l.ends_with('…'))).then_some((lines, false));
    }
    let mut cuts: Vec<(usize, &str)> = shown.match_indices(", ").chain(shown.match_indices(" · ")).collect();
    cuts.sort();
    for (at, sep) in cuts {
        let first = if sep == ", " { format!("{},", &shown[..at]) } else { shown[..at].to_string() };
        let second = &shown[at + sep.len()..];
        if text.width_plain(Face::Body, 28, &first) <= width && text.width_plain(Face::Body, 28, second) <= width {
            return Some((vec![first, second.to_string()], true));
        }
    }
    let lines = text.wrap_prose(Face::Body, 28, meta, width);
    (lines.len() <= k && !lines.iter().any(|l| l.ends_with('…'))).then_some((lines, false))
}

/// What the Shelf look paints as the meta line, with `fit` lines of room where it stands and up to `most` where it starts
/// higher (the name above it leaves that free): the whole line on one line, on two, the short one (`App::meta_short`:
/// "Super Nintendo · Completed · 3 days ago · 6 h") on one, on two, and on three where the room above has them; only when
/// none fits, cut with "…" (18.11.2026: the Brick's Favourites showed "Super Nintendo, last played 3 days ago, 6 h …"
/// with two free lines over it). The lines, whether they are translated, how many lines higher it starts, and which line
/// it is.
fn meta_choice<'a>(text: &mut Text, meta: &'a str, short: Option<&'a str>, width: i32, fit: usize, most: usize) -> (Vec<String>, bool, usize, &'a str) {
    let most = most.max(fit).min(3);
    // One line or two, the whole line before the short one; three only where the room above has them, the short first.
    let tries = [(meta, 1), (meta, 2), (short.unwrap_or(meta), 1), (short.unwrap_or(meta), 2), (short.unwrap_or(meta), 3), (meta, 3)];
    for (line, k) in tries {
        if k <= most {
            if let Some((lines, translated)) = meta_whole(text, line, width, k) {
                return (lines, translated, k.saturating_sub(fit), line);
            }
        }
    }
    // Nothing fits whole (a panel narrower than any we have): the shorter line, as much of it as the room has.
    let line = short.unwrap_or(meta);
    let (lines, translated) = meta_wrap(text, line, width, most.clamp(1, 2));
    (lines, translated, most.clamp(1, 2).saturating_sub(fit), line)
}

/// How many of the Shelf look's meta line's two lines fit between `y` (the last line above it) and `bottom`, `step`
/// apart (`App::paint_shelf_words`).
fn meta_lines(y: i32, bottom: i32, step: i32) -> usize {
    ((bottom - y) / step.max(1)).clamp(0, 2) as usize
}

/// The spines' title size at scale `f` for the text size (`media::spine_px` picks the size that fits the label): 30 px
/// on the Mini V2 in Normal, 37 in Large.
fn spine_want(f: f32) -> f32 {
    pocket_ui::type_scale::px(24) as f32 * f / 2.4
}

/// How far in from the shelf's left end a row of `len` pixels stands on a shelf `view` wide: a row of only a few games
/// (a single one, under half the shelf) starts a third of the free room in, so the shelf is not one thin spine at the wall and
/// black everywhere else (the new-user test, 2026-10-30); a fuller row starts at the end.
fn shelf_lead(len: i32, view: i32) -> i32 {
    if len * 2 < view {
        (view - len) / 3
    } else {
        0
    }
}

/// How far an open case's game (`game`: its picture; None: the size its medium gives it) stands out of the right side
/// of its case `case` (w, h) once it is out. Easy's one big case: well out, so an open case shows at a glance, the same for a
/// cartridge, a card and a disc.
fn ajar_reach_of(big: bool, m: Medium, case: (i32, i32), game: Option<&Pic>) -> f32 {
    let (gw, _, out) = pocket_ui::game_box::game_beside(m, case, game);
    if big {
        (case.0 as f32 * AJAR_BIG).min(gw as f32 * 0.8)
    } else {
        (out as f32 * AJAR).min(case.0 as f32 * AJAR_FRONT)
    }
}

/// Where the centre of a case shown out of its shelf stands: over its slot, unless the shelf holds only a few cases
/// (their row under a third of the cabinet's width): then in the middle of the cabinet, so a lone game is not tucked in
/// its corner above an empty shelf. Kept `margin` clear of the screen's edges (`screen_w`: the room up to the TV, when one stands there),
/// with `reach` more on the right: an open case's game stands out there (`App::ajar_reach`).
fn show_centre(slot_cx: f32, fw: f32, reach: f32, screen_w: f32, margin: f32, shelf_w: i32, view: i32) -> f32 {
    let cx = if shelf_w * 3 < view { (screen_w - reach) / 2.0 } else { slot_cx };
    cx.clamp(margin + fw / 2.0, (screen_w - margin - fw / 2.0 - reach).max(margin + fw / 2.0))
}

/// Case `i` of the Shelf look's row painted again in this frame, out of its tile (`scene_check`): its place in the
/// tile is taken back and this one recorded instead.
#[allow(clippy::too_many_arguments)]
fn scene_replace(i: usize, x: i32, y: i32, w: i32, h: i32, rests: pocket_ui::scene_check::Rests, solid: bool) {
    if pocket_ui::scene_check::recording() {
        let what = format!("case {i}");
        pocket_ui::scene_check::remove(&what);
        pocket_ui::scene_check::put(what, x, y, w, h, rests, solid);
    }
}

thread_local! {
    /// Where the row's (0, 0) and the tiles' top are on the screen this frame, while `scene_check` records.
    static SHELF_ORIGIN: std::cell::Cell<(i32, i32)> = const { std::cell::Cell::new((0, 0)) };
}

/// Lived-in shelf: how much lower a case standing a little forward stands, at the shelf's scale.
fn forward_drop(f: f32) -> i32 {
    (5.0 * f).round() as i32
}

/// How wide an entry stands in the Shelf look's row at scale `f`, in its case `m` (`App::row_media`): its case's spine,
/// or Shelf space's crate as the box (`app_crate::home_box`).
fn row_width_of(item: Item, m: Medium, f: f32) -> i32 {
    if item == Item::Crate {
        return super::app_crate::home_box(f).0;
    }
    (m.template_size().spine as f32 * f).round() as i32
}

/// Which group of Home an entry belongs to: the shelf leaves a wider gap where one ends (Curator and Search, the recent
/// games, Favorites and the pinned rows, the systems).
fn shelf_group(item: Item) -> u8 {
    match item {
        Item::Curator | Item::Search => 0,
        Item::Recent(_) => 1,
        Item::System(_) | Item::Series(_) => 3,
        Item::Game(..) => 4,
        _ => 2,
    }
}

/// The Shelf look's row as laid out last: for these entries at this scale, each spine's place along the shelf, its
/// width and its case; and how long the row is.
pub(crate) struct ShelfRow {
    items: Vec<Item>,
    f: f32,
    /// The shelf's width it was laid out for, and how far in from its left end the first spine stands (`shelf_lead`).
    view: i32,
    lead: i32,
    /// The spines' title size (`media::spine_px`'s `want`): Settings → Text size.
    want: f32,
    /// Each entry's spine as painted last, with the colours' generation it was made in (`CabinetUi::colors_gen`): a
    /// frame copies it without looking anything up.
    pics: Vec<Option<(u64, Rc<Pic>)>>,
    /// The row painted in tiles (`shelf_tile`), with the colours' generation and the page colour they were made for.
    tiles: HashMap<i32, (u64, u32, Rc<Vec<u32>>)>,
    /// Painted natively (docs/dev/native-paint.md): each tile at half size for a place within a screen pixel
    /// (`cabinet_art::half_tile`), with the tile it was made from (a new tile makes a new one).
    halves: HashMap<(i32, u8, u8), (Rc<Vec<u32>>, Rc<Vec<u32>>)>,
    spines: Vec<(i32, i32, Medium)>,
    len: i32,
    /// The tallest case on it, in template pixels.
    tallest: f32,
    /// Each kind of case on it once: the shown front's scale fits the largest of them (`show_k`).
    kinds: Vec<Medium>,
    /// Lived-in shelf (`pocket_ui::lived_in`): each entry's pose, how high above the board its outline
    /// starts (a case lying on the stack) and how high the outline is; empty when the row stands straight.
    lived: Vec<(pocket_ui::lived_in::Plan, i32, i32)>,
    /// The leaning and lying spines as painted last, with the colours' generation they were made in.
    posed: Vec<Option<(u64, Rc<Pic>)>>,
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

    /// The game is still sliding a little out of the case just put back (`ajar_at`): keep painting.
    pub(crate) fn ajar_sliding(&self, now: Instant) -> bool {
        pocket_ui::motion::allowed(pocket_ui::motion::Kind::Move) && self.cabinet.ajar_at.is_some_and(|t| now < t + AJAR_IN + pocket_ui::motion::SETTLE)
    }

    /// Covers still being read, the selected case still turning or the shelf still gliding: keep painting.
    pub(crate) fn shelf_media_animating(&self, now: Instant) -> bool {
        self.cabinet.pending > 0
            || self.cabinet.shelf_waiting
            || self.cabinet.shelf_scroll.moving(now)
            || self.ajar_sliding(now)
            || self.crate_animating(now)
            || (pocket_ui::motion::allowed(pocket_ui::motion::Kind::Move) && now < self.moved_at + SHELF_TURN + pocket_ui::motion::SETTLE)
            || (pocket_ui::playful::on()
                && (self.cabinet.lean.moving(now) || now < self.moved_at + pocket_ui::playful::span(SHELF_TURN + pocket_ui::playful::WIGGLE) + pocket_ui::motion::SETTLE))
    }

    /// The cases are to look different (Cozy extras: stickers, printed discs): the Shelf's rows and tiles and the box's
    /// disc are made again; the fronts follow by themselves (`CaseKey` holds the stickers).
    pub(crate) fn cases_look_changed(&mut self) {
        self.cabinet.colors_gen += 1;
        self.cabinet.box_game = None;
    }

    /// A hardware variant changes cached console icons and the plain covers that carry them.
    pub(crate) fn console_look_changed(&mut self) {
        self.art.clear();
        self.look_previews = None;
        self.cabinet.consoles.clear();
        self.cabinet.scene_consoles.clear();
        self.cabinet.tv.pictures_changed();
        // A system's plain case front embeds the console. Its key stays the ROM system,
        // so these already-painted fronts must be dropped as well as the fitted sprites.
        self.cabinet.shown.clear();
        self.cabinet.shelf_shown.clear();
        self.cabinet.shelf_row = None;
        self.cabinet.shelf_lit = None;
        self.cabinet.shelf_spine_only = None;
        self.cabinet.shelf_front_for = None;
        self.cabinet.shelf_waiting = false;
        self.cabinet.bare_fronts.prune();
        self.cabinet.wrap_fronts.prune();
        self.cases_look_changed();
    }

    /// Covers read on the thread since the last frame, and pictures made for entries long gone.
    pub(crate) fn shelf_media_poll(&mut self) {
        self.cabinet_poll();
        let generation = self.library.case_generation();
        if self.cabinet.case_generation.replace(generation) != Some(generation) {
            // Both row.pics and row.tiles use this generation; changing only CaseKey leaves those outer caches old.
            self.cabinet.colors_gen += 1;
        }
        let frame = self.cabinet.frame;
        if self.cabinet.stand.len() > 400 {
            self.cabinet.stand.retain(|_, (_, used)| frame - *used < 60);
        }
        self.cabinet.frame += 1;
    }

    /// What tells an entry's pictures apart: a game by its ROM, the rest (which have none) by what they are.
    fn shelf_key(&mut self, item: Item, m: Medium) -> (Info, CaseKey) {
        let mut info = self.case_info(item);
        let look = self.case_look(&info, m);
        // Only a game can be sealed: asked before an entry without a ROM gets its stand-in key.
        let sealed = self.sealed(&info.rom);
        if let Item::Series(_) = item {
            // A series by its name and the cabinet's series generation (`CabinetUi::series_changed`): a new list of series,
            // new covers or logos make its spine and front again, and a series that took another's place never shows its.
            info.rom = PathBuf::from(format!("shelf/series/{}/{}", info.title, self.cabinet.series_gen));
        } else if info.rom.as_os_str().is_empty() {
            info.rom = PathBuf::from(format!("shelf/{item:?}"));
        }
        // A series' spine: its logo is its photo (`series_logo`).
        let photo = match item {
            Item::Series(i) => self.series_logo(i).is_some(),
            _ => self.spine_photo(&info.system, &info.rom).is_some(),
        };
        let key = CaseKey {
            case: self.case_stickers_of(&info.rom),
            rom: info.rom.clone(),
            medium: m,
            look,
            ajar: self.is_frozen_item(item),
            sealed,
            photo,
        };
        (info, key)
    }

    /// Each entry of the row's case (`shelf_medium`), worked out once per system along the row.
    fn row_media(&self) -> Vec<Medium> {
        let mut last: Option<(&str, Medium)> = None;
        self.items
            .iter()
            .map(|&item| {
                if !matches!(item, Item::Game(..) | Item::Recent(_)) {
                    return self.shelf_medium(item);
                }
                let system = self.item_system(item);
                match last {
                    Some((s, m)) if s == system => m,
                    _ => {
                        let m = self.shelf_medium(item);
                        last = Some((system, m));
                        m
                    }
                }
            })
            .collect()
    }

    /// An entry's spine standing on the shelf at `f` screen pixels a template pixel, its title at the text size's
    /// spine size (`media::spine_px`); a system's spine carries its short name ("SNES"). Made once and kept.
    pub(crate) fn stand_pic(&mut self, item: Item, m: Medium, f: f32) -> Rc<Pic> {
        // Shelf space's crate stands on the board as the box itself, not a case (19.11.2026).
        if item == Item::Crate {
            return self.home_box_pic(f);
        }
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
        let photo = if key.0.photo { self.spine_photo(&info.system, &info.rom) } else { None };
        let logo = if key.0.photo { self.series_logo_of(item) } else { None };
        let pic = match logo {
            // A series with its logo: the logo is its name (`series_look::logo_spine`).
            Some(logo) => pocket_ui::series_look::logo_spine(&mut self.text, m, f / media::shelf_scale(), key.0.look, &logo, want),
            None => self.spine_or_photo(photo.as_deref(), m, f / media::shelf_scale(), key.0.look, &media::printed_title(&title, item.title_is_name()), want),
        };
        let pic = self.stuck_spine(pic, m, f / media::shelf_scale(), &info.rom, key.0.case);
        let pic = self.sealed_spine(pic, f / media::shelf_scale(), &info.rom, key.0.sealed);
        let pic = Rc::new(if key.0.ajar { ajar_spine(&mut self.text, pic, &info.system, &info.rom, f, key.0.look) } else { pic });
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
    pub(crate) fn stand_front(&mut self, item: Item, m: Medium, k: f32, later: bool) -> (Rc<Pic>, Option<Rc<Pic>>) {
        // Shelf space's crate, selected: the box large, standing on the front's bottom edge, not a case's front.
        if item == Item::Crate {
            let (fw, fh) = media::front_size(m, k);
            let pic = self.home_box_front(fw, fh);
            return (pic.clone(), Some(pic));
        }
        let (info, key) = self.shelf_key(item, m);
        let file = self.cover_file(&info.system, &info.rom);
        let fh = (m.size().h as f32 * k).round() as i32;
        self.note_label_h(item, m, fh);
        let label = file.as_ref().and_then(|f| self.cover_label(f, m, fh));
        // A series: its games' cases stacked (`series_front`), while their covers are read its plain front.
        // Not on the first frame of a turn (`later`): only the spine shows then.
        let series = match item {
            Item::Series(i) if !later => Some(self.series_front(i, m, k)),
            _ => None,
        };
        let reading = matches!(series, Some(SeriesFront::Reading))
            || label.is_none()
                && file
                    .as_ref()
                    .is_some_and(|f| matches!(self.cabinet.labels.get(&(f.clone(), m, fh)), Some(Slot::Pending)));
        let key = (key, (k * 1000.0) as i32);
        if let Some((spine, front)) = self.cabinet.cached_front(&key, reading) {
            return (spine, Some(front));
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
        let title = media::printed_title(if reading { "" } else { info.title.as_str() }, item.title_is_name());
        let t0 = Instant::now();
        let photo = if key.0.photo { self.spine_photo(&info.system, &info.rom) } else { None };
        let spine = match if key.0.photo { self.series_logo_of(item) } else { None } {
            Some(logo) => pocket_ui::series_look::logo_spine(&mut self.text, m, k, key.0.look, &logo, media::spine_size(m, k)),
            None => self.spine_or_photo(photo.as_deref(), m, k, key.0.look, "", media::spine_size(m, k)),
        };
        let spine = self.stuck_spine(spine, m, k, &info.rom, key.0.case);
        let spine = Rc::new(self.sealed_spine(spine, k, &info.rom, key.0.sealed));
        if let Some(SeriesFront::Cases(front)) = series {
            self.cabinet.made("shown front", t0);
            self.cabinet.cache_front(key, spine.clone(), front.clone(), true);
            return (spine, Some(front));
        }
        let front = media::front(
            &mut self.text,
            m,
            k,
            key.0.look,
            &title,
            label.as_deref(),
            if reading { None } else { sprite },
            &super::system_short(&self.systems, &info.system),
        );
        let front = self.stuck_front(front, m, k, &info.rom, key.0.case, (&title, label.is_some(), if reading { None } else { sprite }));
        let front = self.sealed_front(front, k, &info.rom, key.0.sealed);
        self.cabinet.made("shown front", t0);
        self.cabinet.cache_front(key, spine.clone(), front.clone(), !reading);
        (spine, Some(front))
    }

    /// The row laid out again when the entries or the scale changed; the shelf then jumps to the selection.
    fn shelf_row(&mut self, f: f32, view: i32) -> bool {
        let want = spine_want(f);
        if self
            .cabinet
            .shelf_row
            .as_ref()
            .is_some_and(|r| r.f == f && r.view == view && r.want == want && r.items == self.items)
        {
            return false;
        }
        // Every spine starts on a multiple of four pixels: a small screen's picture is laid out larger and scaled down
        // (`scaled_layout`: 2:1 on 640x480 and the other H700 panels), and a spine starting between two of its pixels came out a
        // pixel wider than the one beside it (the owner, 2026-10-29: "same platform, different sizes?").
        let up4 = |v: i32| (v + 3) / 4 * 4;
        let gap = (2.0 * f).round() as i32;
        let group_gap = up4((12.0 * f).round() as i32);
        // A few games: the row stands a little in from the shelf's end, not squeezed against it.
        // Each entry's case once (a system's shelf of 22,000 games asks one system, not 22,000 copies of its name).
        let media = self.row_media();
        let natural: i32 = {
            let mut end = 0;
            for (i, &item) in self.items.iter().enumerate() {
                if i > 0 && shelf_group(item) != shelf_group(self.items[i - 1]) {
                    end += group_gap;
                }
                let w = row_width_of(item, media[i], f);
                end = up4(end + w + gap);
            }
            end - gap
        };
        // Only a lone game: a few are read from the shelf's end (Curator's seven in tests/shelf_end.rs, two games in
        // tests/user_journeys.rs' sticker journey), one alone on a wide shelf is not a spine pressed against the wall.
        let lead = up4(if self.items.len() == 1 { shelf_lead(natural, view) } else { 0 });
        // Settings → Look → Lived-in shelf: the row's poses (`pocket_ui::lived_in`). A row that fits the shelf standing
        // straight (its props after its end) never grows past it: no stack then, and if it still would, no wider spaces,
        // and last no leaning either (0.5.2, mixed rows at 1280x720 had to scroll only for their stacks).
        let mut plans = self.lived_in_plans(true, &media);
        let (mut spines, mut lived, mut tallest) = self.lay_row(f, lead, &plans, &media);
        let fits_straight = lead + natural <= view;
        let past = |spines: &[(i32, i32, Medium)]| spines.iter().map(|&(x, w, _)| x + w).max().unwrap_or(0) > view;
        if !plans.is_empty() && fits_straight && past(&spines) {
            plans = self.lived_in_plans(false, &media);
            (spines, lived, tallest) = self.lay_row(f, lead, &plans, &media);
            if past(&spines) {
                plans.iter_mut().for_each(|p| p.extra = 0);
                (spines, lived, tallest) = self.lay_row(f, lead, &plans, &media);
            }
            if past(&spines) {
                (spines, lived, tallest) = self.lay_row(f, lead, &[], &media);
            }
        }
        let spines_end = spines.iter().map(|&(x, w, _)| x + w).max().unwrap_or(0);
        let mut kinds: Vec<Medium> = Vec::new();
        for &(_, _, m) in &spines {
            if !kinds.contains(&m) {
                kinds.push(m);
            }
        }
        self.cabinet.shelf_row = Some(ShelfRow {
            items: self.items.clone(),
            f,
            view,
            lead,
            want,
            pics: vec![None; spines.len()],
            tiles: HashMap::new(),
            halves: HashMap::new(),
            posed: vec![None; spines.len()],
            spines,
            len: spines_end,
            tallest,
            kinds,
            lived,
        });
        true
    }

    /// The row's spines laid out from `lead` on at scale `f` (`shelf_row`): each entry's left end, width across and case,
    /// with its plan's pose where `plans` has one (Lived-in shelf; empty: every case standing straight), and the tallest
    /// case in template pixels. `media`: each entry's case (`row_media`).
    #[allow(clippy::type_complexity)]
    fn lay_row(&self, f: f32, lead: i32, plans: &[pocket_ui::lived_in::Plan], media: &[Medium]) -> (Vec<(i32, i32, Medium)>, Vec<(pocket_ui::lived_in::Plan, i32, i32)>, f32) {
        let up4 = |v: i32| (v + 3) / 4 * 4;
        let gap = (2.0 * f).round() as i32;
        let group_gap = up4((12.0 * f).round() as i32);
        let mut spines = Vec::with_capacity(self.items.len());
        let mut lived = Vec::with_capacity(if plans.is_empty() { 0 } else { self.items.len() });
        // The stack being laid: where it starts, its right end so far and how high it is.
        let mut stack: Option<(i32, i32, i32, i32)> = None;
        // The case before, when it leans left: its left end, width, angle and outline's width (`Lean` dominoes).
        let mut prev_lean: Option<(i32, i32, f32, i32)> = None;
        let (mut x, mut tallest) = (lead, 0.0f32);
        for (i, &item) in self.items.iter().enumerate() {
            if i > 0 && shelf_group(item) != shelf_group(self.items[i - 1]) {
                x += group_gap;
            }
            let m = media[i];
            let w = row_width_of(item, m, f);
            let h = (m.template_size().h as f32 * f).round() as i32;
            tallest = tallest.max(m.template_size().h as f32);
            let Some(plan) = plans.get(i).copied() else {
                spines.push((x, w, m));
                x += up4(w + gap);
                continue;
            };
            use pocket_ui::lived_in::{footprint, lean_height, Pose};
            x += up4((plan.extra as f32 * f).round() as i32);
            let was_lean = prev_lean.take();
            match plan.pose {
                Pose::Up => {
                    spines.push((x, w, m));
                    lived.push((plan, 0, h));
                    x += up4(w + gap);
                }
                Pose::Lean(deg) => {
                    let bw = footprint(plan.pose, w as f32, h as f32).ceil() as i32;
                    // A domino: its top on the top corner of the one before (that one's left end and its width across),
                    // its foot clear of that one's foot.
                    if let Some((px, pw, pdeg, pbw)) = was_lean.filter(|_| deg < 0.0) {
                        let top_corner = px + (pw as f32 * pdeg.to_radians().cos()).ceil() as i32 + 1;
                        let foot_clear = px + pbw + gap - (h as f32 * deg.abs().to_radians().sin()).floor() as i32;
                        x = top_corner.max(foot_clear);
                    }
                    spines.push((x, bw, m));
                    lived.push((plan, 0, lean_height(deg, w as f32, h as f32).ceil() as i32));
                    if deg < 0.0 {
                        prev_lean = Some((x, w, deg.abs(), bw));
                    }
                    x += up4(bw + gap);
                }
                Pose::Flat { level, nudge } => {
                    // Lying one on another, each a few pixels off the one under it; a new stack at level 0.
                    // In a mixed row a shorter case lies centred on the longer one under it (`plan_sized`).
                    // A stack keeps room on its left for a case pulled out of it (`lived_slide`): it slides out sideways
                    // and stays on what it lay on (18.11.: a lying case lifted like a standing one hung in the air).
                    if level == 0 {
                        stack = Some((x + up4(2 * gap) + up4((LIVED_SLIDE * h as f32).ceil() as i32), 0, 0, h));
                    }
                    let (sx, end, below, base) = stack.unwrap_or((x, 0, 0, h));
                    let at = sx + (base - h).max(0) / 2 + ((6 + nudge as i32) as f32 * f).round() as i32;
                    spines.push((at, h, m));
                    lived.push((plan, below, w));
                    let end = end.max(at + h);
                    stack = Some((sx, end, below + w, base));
                    x = up4(end + 2 * gap);
                }
            }
        }
        (spines, lived, tallest)
    }

    /// Lived-in shelf: lying case `i`, selected, `slide` pixels out of its stack to the left (the stack keeps that room,
    /// `lay_row`), on the case or the board it lay on. Its place is cleared (its own layer of the stack only: the ones
    /// over and under it stay), with the little shadow on its right but never into the case after the stack.
    #[allow(clippy::too_many_arguments)]
    fn pull_from_stack(&mut self, c: &mut Canvas, row: &mut ShelfRow, i: usize, lay: &ShelfLayout, scroll: i32, slide: i32, shadows: bool) {
        use pocket_ui::lived_in::Pose;
        let level = |j: usize| match row.lived.get(j).map(|l| l.0.pose) {
            Some(Pose::Flat { level, .. }) => Some(level),
            _ => None,
        };
        let mut end = i + 1;
        while level(end).is_some_and(|l| l > 0) {
            end += 1;
        }
        let next = row.spines.get(end).map_or(lay.right, |s| lay.left + s.0 - scroll);
        let (x0, len, _) = row.spines[i];
        let (bottom, outline) = (row.lived[i].1, row.lived[i].2);
        let (x, top) = (lay.left + x0 - scroll, lay.floor - bottom - outline);
        let (l, r) = ((x - 1).max(lay.left), (x + len + 9).min(next).min(lay.right));
        if r > l {
            c.rect(l, top, r - l, outline, pal().void);
        }
        let Some(pic) = self.posed_pic(row, i) else { return };
        let at = x - slide + (len - pic.w) / 2;
        self.blit_between(c, &pic, at, top, 256, (0, lay.floor), lay.left, lay.right);
        if shadows {
            art::soft_shadow(c, at + pic.w, top + 2, 3, outline - 2, 5, 0.3, (top, lay.floor - bottom));
        }
        pocket_ui::scene_check::set_origin(0, 0);
        scene_replace(i, at, top, pic.w, outline, pocket_ui::scene_check::Rests::Settled, true);
    }

    /// Lived-in shelf: the feet of the cases standing a little forward, over the board's front edge,
    /// with a little shadow on the board (the rest of them is in the row's tiles). The selected one is out of the row.
    fn paint_lived_feet(&mut self, c: &mut Canvas, row: &mut ShelfRow, lay: &ShelfLayout, scroll: i32, sel: usize) {
        let drop = forward_drop(row.f);
        let shade = self.cabinet.room.lit.case_shade();
        for i in 0..row.lived.len() {
            if !row.lived[i].0.forward || i == sel {
                continue;
            }
            let (x0, w, _) = row.spines[i];
            let x = lay.left + x0 - scroll;
            // Where the row fades into the page at an end that goes on (`fade_cols`, 48 wide) the foot is left out too.
            let (fade_l, fade_r) = (scroll > 0, scroll < row.len - (lay.right - lay.left));
            if x + w <= lay.left || x >= lay.right || (fade_l && x < lay.left + 48) || (fade_r && x + w > lay.right - 48) {
                continue;
            }
            let pic = self.row_pic(row, i, row.f);
            if pal().void != 0 {
                art::soft_shadow(c, x + 1, lay.floor + drop - 1, pic.w, 3, 4, 0.45, (lay.floor, c.h));
            }
            self.blit_between(c, &pic, x, lay.floor - pic.h + drop, shade, (lay.floor, lay.floor + drop), lay.left, lay.right);
        }
    }

    /// Lived-in shelf: leaning case `i` taken out of the row: cleared along its own outline, row by row
    /// (a domino partner keeps its corner), and a leaning neighbour painted again over the edge where they touched.
    fn erase_leaning(&mut self, c: &mut Canvas, row: &mut ShelfRow, i: usize, lay: &ShelfLayout, scroll: i32) {
        let at = |row: &ShelfRow, j: usize, pic: &Pic| {
            let (x, w, _) = row.spines[j];
            let (bottom, outline) = (row.lived[j].1, row.lived[j].2);
            (lay.left + x - scroll + w / 2 - pic.w / 2, lay.floor - bottom - outline / 2 - pic.h / 2)
        };
        let Some(pic) = self.posed_pic(row, i) else { return };
        let (px, py) = at(row, i, &pic);
        for yy in 0..pic.h {
            let line = &pic.px[(yy * pic.w) as usize..((yy + 1) * pic.w) as usize];
            let (Some(a), Some(b)) = (line.iter().position(|p| p >> 24 != 0), line.iter().rposition(|p| p >> 24 != 0)) else { continue };
            let (l, r) = ((px + a as i32).max(lay.left), (px + b as i32 + 1).min(lay.right));
            if r > l && py + yy < lay.floor {
                c.rect(l, py + yy, r - l, 1, pal().void);
            }
        }
        let shade = self.cabinet.room.lit.case_shade();
        for j in [i.wrapping_sub(1), i + 1] {
            if !row.lived.get(j).is_some_and(|l| matches!(l.0.pose, pocket_ui::lived_in::Pose::Lean(_))) {
                continue;
            }
            if let Some(p) = self.posed_pic(row, j) {
                let (x, y) = at(row, j, &p);
                self.blit_between(c, &p, x, y, shade, (0, lay.floor), lay.left, lay.right);
            }
        }
    }

    /// Settings → Look → Lived-in shelf: the row's leaning and lying spines made ahead (`posed_pic`), in idle moments
    /// after the row opened, so scrolling along it never turns a spine in a frame (the prototype's first frame on a tile of
    /// them: 5-6 ms on a desktop core, estimated 40-55 ms on the Brick, a stutter of 2-3 frames). From
    /// `LIVED_WARM_AFTER` after the last button, the nearest to the selection first, for at most `LIVED_WARM_BUDGET` of
    /// the UI thread at a time; when to come back while some are left.
    pub(crate) fn lived_warm_tick(&mut self, now: Instant) -> Option<Instant> {
        let idle = self.lived_in
            && self.theme == pocket_ui::settings::Theme::Shelf
            && self.window.is_some()
            && self.card.is_none()
            && self.running.is_none()
            && self.menu.is_none()
            && self.switcher.is_none()
            && self.settings_page.is_none();
        if !idle || self.lived_left() == 0 {
            return None;
        }
        let due = self.last_input + LIVED_WARM_AFTER;
        if now < due {
            return Some(due);
        }
        (self.lived_warm(LIVED_WARM_BUDGET) > 0).then(|| now + Duration::from_millis(16))
    }

    /// The Cabinet: the spines and back walls of the two shelves over and under the selected one made ahead, in idle
    /// moments after a move, so a Down or Up to a shelf not seen yet does not make twenty spines and a wall in one frame
    /// (a big card's first walk through its systems: 12-23 ms on the build computer,
    /// 21.11.2026). From `CABINET_WARM_AFTER` after the last button, for at most `CABINET_WARM_BUDGET` at a time; when to come
    /// back while some are left or covers' colours are still being read.
    pub(crate) fn cabinet_warm_tick(&mut self, now: Instant) -> Option<Instant> {
        let idle = self.theme == pocket_ui::settings::Theme::Cabinet
            && self.window.is_some()
            && self.card.is_none()
            && self.running.is_none()
            && self.menu.is_none()
            && self.switcher.is_none()
            && self.settings_page.is_none()
            && self.cabinet.eject.is_none()
            && self.cabinet.insert.is_none();
        if !idle || self.cabinet.warm_geometry.is_none() {
            return None;
        }
        let due = self.last_input + CABINET_WARM_AFTER;
        if now < due {
            return Some(due);
        }
        // Covers' colours still on their way remake those spines: looked at again for a while after the move.
        let colours = self.cabinet.pending > 0 && now < self.last_input + CABINET_WARM_COLOURS;
        pocket_ui::mark!("shelf-warm", "cabinet");
        (self.cabinet_warm(CABINET_WARM_BUDGET) > 0 || colours).then(|| now + Duration::from_millis(16))
    }

    /// Makes the spines (and walls) of the shelves next to the selected one that are not made yet, the nearest shelf
    /// first, until `budget` of wall time is used (at least one); how many were made.
    pub(crate) fn cabinet_warm(&mut self, budget: Duration) -> usize {
        let Some((cab_w, view)) = self.cabinet.warm_geometry else { return 0 };
        self.cabinet_poll();
        self.cabinet_shelves();
        let Some((cs, _)) = self.cabinet_cursor() else { return 0 };
        let t0 = Instant::now();
        let mut made = 0;
        for s in [cs + 1, cs.wrapping_sub(1), cs + 2, cs.wrapping_sub(2)] {
            let Some(shelf) = self.cabinet.cab.shelves.get(s) else { continue };
            let scroll = self.cabinet.scroll_x.get(&shelf.key).map_or(0.0, |t| t.target()).round() as i32;
            let first = shelf.nearest(scroll).saturating_sub(1);
            let cases: Vec<cabinet::Case> = (first..shelf.cases.len()).take_while(|&i| shelf.x(i) - scroll <= view).map(|i| shelf.cases[i]).collect();
            let inner = shelf.height;
            let walls = self.cabinet.walls.len();
            self.wall_pic(inner, s as u32 % 3, cab_w);
            made += self.cabinet.walls.len() - walls;
            for case in cases {
                if made > 0 && t0.elapsed() >= budget {
                    return made;
                }
                let spines = self.cabinet.spines.len();
                self.spine_pic(case.item, case.medium);
                made += self.cabinet.spines.len() - spines;
            }
        }
        made
    }

    /// Reproduce the idle warm pass between a card rescan and the next full redraw.
    pub(crate) fn cabinet_card_swap_check(&mut self) {
        self.theme = super::Theme::Cabinet;
        self.cabinet.inline = true;
        self.screen = super::Screen::Home;
        self.rebuild();
        let last = self.systems.len() - 1;
        self.sel = self.items.iter().position(|&i| i == Item::System(last - 1)).unwrap();
        self.cabinet_shelves();
        self.cabinet.warm_geometry = Some((600, 500));
        self.cabinet.last_out = Some((Item::Game(last, 0), (0, 0, 1, 1)));
        let fresh = self.systems[..last].to_vec();
        self.library_rescanned(fresh, None);
        assert_eq!(self.systems.len(), last);
        self.cabinet_warm(Duration::from_secs(2));
        assert!(self.cabinet.last_out.is_none(), "the old case position must not survive the swap");
        println!("cabinet card swap warmed the new library");
    }

    /// The Shelf look's row laid out again at the next frame (Lived-in shelf switched: with or without its poses).
    pub(crate) fn forget_shelf_row(&mut self) {
        self.cabinet.shelf_row = None;
    }

    /// How many of the shelf row's posed spines are still to make (0 without a row, or one of other entries).
    pub(crate) fn lived_left(&self) -> usize {
        let gen = self.cabinet.colors_gen;
        let Some(row) = self.cabinet.shelf_row.as_ref().filter(|r| r.items == self.items) else { return 0 };
        (0..row.lived.len()).filter(|&i| row.lived[i].0.pose != pocket_ui::lived_in::Pose::Up && !matches!(&row.posed[i], Some((g, _)) if *g == gen)).count()
    }

    /// Makes the row's posed spines not made yet, nearest to the selection first, until `budget` of wall time is
    /// used (at least one); how many are left.
    pub(crate) fn lived_warm(&mut self, budget: Duration) -> usize {
        let gen = self.cabinet.colors_gen;
        let Some(mut row) = self.cabinet.shelf_row.take() else { return 0 };
        if row.items != self.items {
            self.cabinet.shelf_row = Some(row);
            return 0;
        }
        let mut todo: Vec<usize> = (0..row.lived.len())
            .filter(|&i| row.lived[i].0.pose != pocket_ui::lived_in::Pose::Up && !matches!(&row.posed[i], Some((g, _)) if *g == gen))
            .collect();
        todo.sort_by_key(|&i| i.abs_diff(self.sel));
        let t0 = Instant::now();
        let total = todo.len();
        let mut left = total;
        for i in todo {
            if left < total && t0.elapsed() >= budget {
                break;
            }
            self.posed_pic(&mut row, i);
            left -= 1;
        }
        self.cabinet.shelf_row = Some(row);
        left
    }

    /// Settings → Look → Lived-in shelf: the row's plan (`pocket_ui::lived_in::plan_sized`) from its games' ROM names,
    /// with or without stacks; none (the row stands straight) when it is off or the row is not only games (Home).
    fn lived_in_plans(&self, stacks: bool, media: &[Medium]) -> Vec<pocket_ui::lived_in::Plan> {
        if !self.lived_in || !self.items.iter().all(|i| matches!(i, Item::Game(..))) {
            return Vec::new();
        }
        let seeds: Vec<u64> = self
            .items
            .iter()
            .map(|&i| pocket_ui::lived_in::seed(&self.item_path(i).and_then(|p| p.file_name()).map(|n| n.to_string_lossy()).unwrap_or_default()))
            .collect();
        let fixed: Vec<bool> = self.items.iter().map(|&i| self.is_frozen_item(i)).collect();
        // A mixed row (Favourites, a smart shelf): the cases' lengths, so a stack has the small ones on top.
        let sizes: Vec<f32> = media.iter().map(|m| m.template_size().h as f32).collect();
        pocket_ui::lived_in::plan_sized(&seeds, &fixed, &sizes, stacks)
    }

    /// Lived-in shelf: entry `i`'s spine leaning or lying as its plan says (made once and kept); None standing up.
    fn posed_pic(&mut self, row: &mut ShelfRow, i: usize) -> Option<Rc<Pic>> {
        use pocket_ui::lived_in::{lying, turned, Pose};
        let pose = row.lived.get(i)?.0.pose;
        if pose == Pose::Up {
            return None;
        }
        let gen = self.cabinet.colors_gen;
        if let Some((g, pic)) = &row.posed[i] {
            if *g == gen {
                return Some(pic.clone());
            }
        }
        let t0 = Instant::now();
        let up = self.row_pic(row, i, row.f);
        let pic = Rc::new(match pose {
            Pose::Lean(deg) => turned(&up, deg),
            _ => lying(&up, true),
        });
        self.cabinet.made("posed spine", t0);
        LIVED_POSED.set(LIVED_POSED.get() + 1);
        row.posed[i] = Some((gen, pic.clone()));
        Some(pic)
    }

    /// The shelf board, as wide as the shelf (walnut on the dark page, oak on the light one).
    pub(crate) fn shelf_board(&mut self, w: i32, h: i32) -> Rc<Pic> {
        match &self.cabinet.shelf_board {
            Some((bw, bh, page, pic)) if (*bw, *bh, *page) == (w, h, pal().void) => pic.clone(),
            _ => {
                let mut pic = art::top_board(w, h);
                self.cabinet.room.lit.tint_all(&mut pic.px);
                let pic = Rc::new(pic);
                self.cabinet.shelf_board = Some((w, h, pal().void, pic.clone()));
                pic
            }
        }
    }

    /// Shelf: every entry stands on one shelf as its case's spine (`media`), the same light and baseline for all,
    /// only the case's shape changing; the selected one rises out of the row and is shown large above the shelf,
    /// turned to its front with the cover, its name and what is known about it beside it.
    /// The Shelf look with nothing on it: just its board along the bottom, empty on purpose (`paint_empty_home_around`).
    pub(crate) fn paint_empty_shelf(&mut self, c: &mut Canvas) {
        let lay = shelf_layout(c.w, c.h, TALLEST);
        let view = lay.right - lay.left;
        let board = self.shelf_board(view + 32, lay.board_h);
        art::blit(c, &board, lay.left - 16, lay.floor, 256, (0, c.h));
        if pal().void != 0 {
            art::soft_shadow(c, lay.left - 16, lay.floor + lay.board_h, view + 32, 10, 12, 0.5, (0, c.h));
        }
    }

    pub(crate) fn paint_shelf(&mut self, c: &mut Canvas) {
        self.shelf_media_poll();
        self.room_sync();
        self.cabinet.shelf_lit = None;
        let n = self.items.len();
        if n == 0 {
            return;
        }
        // The spines' scale is the screen's; the room above them follows the row's tallest case.
        let f = shelf_layout(c.w, c.h, TALLEST).f;
        let now = self.cabinet.shelf_clock.unwrap_or_else(pocket_ui::frame_clock::now);
        let t0 = thread_cpu();
        let new_row = self.shelf_row(f, c.w - 2 * MARGIN);
        if new_row {
            let (d, n) = LIVED_ROW.get();
            LIVED_ROW.set((d + thread_cpu().saturating_sub(t0), n + 1));
        }
        let Some(mut row) = self.cabinet.shelf_row.take() else { return };
        let lay = shelf_layout(c.w, c.h, row.tallest);
        let sel = self.sel.min(n - 1);
        let view = lay.right - lay.left;

        // The shelf glides so that the selection keeps a fifth of the shelf beside it; a row that fits stands still.
        let max_scroll = (row.len - view).max(0) as f32;
        let (sx, sw, sm) = row.spines[sel];
        let slot = (sx, sw);
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
        // Playful animations: scrolling fast, the spines lean against the move and swing back when it stops
        // (`playful::Lean`), up to about 2 degrees: each row of the strip is copied a little sideways.
        let playful = pocket_ui::playful::on();
        if new_row || !playful {
            self.cabinet.lean.reset();
        }
        // Frozen games: their game stands a little out of the case's top (`ajar_spine`).
        let room = (row.tallest * lay.f).round() as i32 + if self.frozen.is_empty() { 0 } else { ajar_peek(lay.f) };
        let lean_px = if playful { self.cabinet.lean.at(self.cabinet.shelf_scroll.segment(), lay.f, now) * room as f32 * 0.035 } else { 0.0 };

        // The board, and the spines standing on it.
        let board = self.shelf_board(view + 32, lay.board_h);
        art::blit(c, &board, lay.left - 16, lay.floor, 256, (0, c.h));
        if pal().void != 0 {
            art::soft_shadow(c, lay.left - 16, lay.floor + lay.board_h, view + 32, 10, 12, 0.5, (0, c.h));
        }
        // The row is painted into tiles once (spines, their shadows, the props); a frame copies what is in view and
        // paints only the lifted spines on their own (a move costs no more than the list looks', tests/shelf_moves.rs).
        let shadows = pal().void != 0;
        let strip = lay.floor - room;
        let seed = (n as u32).wrapping_mul(31) ^ self.items.first().map_or(0, |i| shelf_group(*i) as u32);
        // How far row `yy` of the strip is pushed sideways: the top by the whole lean, the floor not at all.
        let shift = |yy: i32| (lean_px * (1.0 - yy as f32 / room.max(1) as f32)).round() as i32;
        let reach = lean_px.abs().ceil() as i32;
        let first = (scroll - reach).max(0) / TILE_W;
        if pocket_ui::scene_check::recording() {
            // Every tile painted again, so its cases and props are recorded where this frame shows them; the board is
            // what they stand on, a case standing a little forward on its front (`forward_drop`).
            row.tiles.clear();
            row.halves.clear();
            SHELF_ORIGIN.set((lay.left - scroll, strip));
            // The board goes on under the row past the view's ends.
            let ends = (lay.left - scroll - 16, lay.left - scroll + row.len.max(view) + 16);
            pocket_ui::scene_check::surface("the shelf's board", lay.floor, forward_drop(row.f), ends.0.min(lay.left - 16), ends.1.max(lay.right + 16));
            // The whole row, also what is out of view: a case in view may lie on one that is not.
            for t in 0..=(row.len.max(view) / TILE_W) {
                self.shelf_tile(&mut row, t, room, view, seed);
            }
            row.tiles.clear();
        }
        let tiles: Vec<Rc<Vec<u32>>> = (first..=(scroll + view + reach - 1) / TILE_W).map(|t| self.shelf_tile(&mut row, t, room, view, seed)).collect();
        if c.n() == 1 {
            let c_w = c.w;
            for (t, tile) in (first..).zip(&tiles) {
                for yy in 0..room {
                    let cy = strip + yy;
                    // The row's columns this screen row shows: the view, moved back by the lean.
                    let from = scroll - shift(yy);
                    let (a, b) = ((t * TILE_W).max(from), ((t + 1) * TILE_W).min(from + view));
                    if (0..c.h).contains(&cy) && b > a {
                        let dx = lay.left + a - from;
                        let src = &tile[(yy * TILE_W + a - t * TILE_W) as usize..][..(b - a) as usize];
                        c.pixels_mut()[(cy * c_w + dx) as usize..][..(b - a) as usize].copy_from_slice(src);
                    }
                }
            }
        } else if lean_px == 0.0 {
            // Painted natively (docs/dev/native-paint.md), standing still: the tiles at half size, made once per place
            // within a screen pixel, copied row by row (the strip is not put together in design pixels).
            let fx = (lay.left - scroll).rem_euclid(2) as u8;
            let fy = c.cell(lay.left, strip).3;
            if row.halves.len() > 24 {
                row.halves.clear();
            }
            let halves: Vec<Rc<Vec<u32>>> = (first..)
                .zip(&tiles)
                .map(|(t, tile)| match row.halves.get(&(t, fx, fy)) {
                    Some((of, half)) if Rc::ptr_eq(of, tile) => half.clone(),
                    _ => {
                        let half = Rc::new(art::half_tile(tile, TILE_W as usize, fx, fy));
                        row.halves.insert((t, fx, fy), (tile.clone(), half.clone()));
                        half
                    }
                })
                .collect();
            let (tiles, halves): (Vec<&[u32]>, Vec<&[u32]>) = (tiles.iter().map(|t| t.as_slice()).collect(), halves.iter().map(|h| h.as_slice()).collect());
            art::paint_strip(c, (lay.left, strip), (view, room), scroll, first, TILE_W, &tiles, &halves);
        } else {
            // Painted natively (docs/dev/native-paint.md), leaning: the strip's rows put together from the same columns of
            // the same tiles, then copied as the 2:1 filter would show them. A column no tile has leaves what is there.
            const NONE: u32 = 0xff00_0000;
            let w = view.max(0) as usize;
            let mut px = std::mem::take(&mut self.cabinet.strip_px);
            px.resize(w * room.max(0) as usize, NONE);
            let end = (first + tiles.len() as i32) * TILE_W;
            // Only a leaning row has columns no tile covers; without them the copy needs no see-through test.
            let mut holes = false;
            for yy in 0..room {
                let from = scroll - shift(yy);
                let (mut col, hi) = (from.max(first * TILE_W), (from + view).min(end));
                let line = &mut px[yy as usize * w..(yy as usize + 1) * w];
                let (a, b) = ((col - from).clamp(0, view) as usize, (hi - from).clamp(0, view) as usize);
                holes |= a > 0 || b < w;
                line[..a].fill(NONE);
                line[b..].fill(NONE);
                while col < hi {
                    let t = col.div_euclid(TILE_W);
                    let stop = ((t + 1) * TILE_W).min(hi);
                    let src = &tiles[(t - first) as usize][(yy * TILE_W + col - t * TILE_W) as usize..][..(stop - col) as usize];
                    px[yy as usize * w + (col - from) as usize..][..src.len()].copy_from_slice(src);
                    col = stop;
                }
            }
            if holes {
                c.copy_design(lay.left, strip, w, &px, Some(NONE));
            } else {
                art::copy_solid(c, lay.left, strip, w, &px);
            }
            self.cabinet.strip_px = px;
        }
        if !row.lived.is_empty() {
            let t0 = thread_cpu();
            self.paint_lived_feet(c, &mut row, &lay, scroll, sel);
            let (d, n) = LIVED_FEET.get();
            LIVED_FEET.set((d + thread_cpu().saturating_sub(t0), n + 1));
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
            let mut h = (m.template_size().h as f32 * lay.f).round() as i32;
            // Lived-in shelf: a leaning or lying case (its outline), and whether it lies on the stack.
            let posed = row.lived.get(i).filter(|l| l.0.pose != pocket_ui::lived_in::Pose::Up).map(|&(p, bottom, outline)| (p.pose, bottom, outline));
            let mut open = self.openness(i);
            if playful && i == sel {
                // Playful animations: it springs up, a touch past and back.
                open = pocket_ui::playful::back_out(pocket_ui::playful::secs(self.moved_at, now) / super::PULL.as_secs_f32());
            }
            // Shelf space's crate is a box on the board: it is not pulled up out of the row like a case (it hung over
            // the board, 19.11.2026); the room above shows it large.
            if self.items.get(i) == Some(&Item::Crate) {
                open = 0.0;
            }
            let rise = (open * lay.lift as f32).round() as i32;
            if rise > 0 {
                // Out of its place in the tile, up out of the row (leaning with it: the hole is where its picture was).
                let pic = self.row_pic(&mut row, i, lay.f);
                let (hx, hw) = (x.max(lay.left), (x + w).min(lay.right) - x.max(lay.left));
                let mut from_stack = false;
                if let Some((pose, bottom, outline)) = posed {
                    if matches!(pose, pocket_ui::lived_in::Pose::Flat { .. }) {
                        // A lying case is pulled out of its stack sideways, like a book, and stays on what it lies on;
                        // the rest of the stack stays as it is.
                        self.pull_from_stack(c, &mut row, i, &lay, scroll, lived_slide(w, open), shadows);
                        from_stack = true;
                    } else {
                        // A leaning case comes out standing straight, as any other; where it leaned is cleared.
                        let _ = (bottom, outline);
                        self.erase_leaning(c, &mut row, i, &lay, scroll);
                        let (px, py) = (x + (w - pic.w) / 2, lay.floor - pic.h - rise);
                        self.blit_between(c, &pic, px, py, 256, (0, lay.floor), lay.left, lay.right);
                        pocket_ui::scene_check::set_origin(0, 0);
                        scene_replace(i, px, py, pic.w, pic.h, pocket_ui::scene_check::Rests::Raised("the selected case, out of the row"), true);
                    }
                } else if lean_px == 0.0 {
                    c.rect(hx, lay.floor - pic.h, hw, pic.h, pal().void);
                } else {
                    for y in (lay.floor - pic.h).max(strip)..lay.floor {
                        let d = shift(y - strip);
                        let (l, r) = ((hx + d).max(lay.left), (hx + hw + d).min(lay.right));
                        c.rect(l, y, r - l, 1, pal().void);
                    }
                }
                if posed.is_none() {
                    self.blit_between(c, &pic, x, lay.floor - pic.h - rise, 256, (0, lay.floor), lay.left, lay.right);
                    pocket_ui::scene_check::set_origin(0, 0);
                    scene_replace(i, x, lay.floor - pic.h - rise, pic.w, pic.h, pocket_ui::scene_check::Rests::Raised("the selected case, out of the row"), true);
                }
                if shadows && !from_stack {
                    art::soft_shadow(c, x, lay.floor - 4, pic.w, 6, 6, 0.4 * rise as f32 / lay.lift as f32, (0, c.h));
                }
                // A strip of light on the board where it stood, in the label's paper: the selection reads even
                // among cases of one colour and on 480 lines (the visual review 2026-11-04, 4).
                let alpha = (255.0 * rise as f32 / lay.lift as f32).min(255.0) as u8;
                for y in lay.floor..lay.floor + (2.0 * lay.f).round() as i32 {
                    for x in hx..hx + hw {
                        c.blend(x, y, pal().label, alpha);
                    }
                }
            }
            // A lying case selected is out of its stack to the left (`pull_from_stack`).
            let slid = match posed {
                Some((pocket_ui::lived_in::Pose::Flat { .. }, _, _)) => lived_slide(w, open),
                _ => 0,
            };
            let (hx, hw) = ((x - slid).max(lay.left), (x - slid + w).min(lay.right) - (x - slid).max(lay.left));
            let mut foot = 0;
            if let Some((_, bottom, outline)) = posed {
                (h, foot) = (outline, bottom);
            }
            self.hits.row(Area::new(hx, lay.floor - foot - h - rise, hw, h + rise), i);
        }
        // Where the row goes on past an end, it fades into the page.
        if scroll > 0 {
            fade_cols(c, lay.left + 48, lay.left, lay.floor - (TALLEST * lay.f) as i32 - lay.lift, lay.floor);
        }
        if (scroll as f32) < max_scroll {
            fade_cols(c, lay.right - 48, lay.right, lay.floor - (TALLEST * lay.f) as i32 - lay.lift, lay.floor);
        }
        let pitch = (row.len - row.lead) as f32 / n as f32;
        // The selected case, large, turned to its front.
        let k = show_k(&lay, sm, &row.kinds);
        self.cabinet.shelf_row = Some(row);
        let item = self.items[sel];
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
        // On its way into the console its shadow goes with it, or Light keeps a dark block where it stood.
        // Going into its console, the shadow is painted over the page each frame instead (`launch_shelf_shadow`): the
        // page under a launch is kept.
        if let Some(ins) = self.cabinet.insert.as_mut().filter(|i| i.item == item) {
            ins.shadow = (shadows && front.is_some()).then_some((fx + 10, fy + 12, fw, fh, lay.floor, turned));
        } else if shadows && front.is_some() && item != Item::Crate && !self.series_stacked(item, fh) {
            // (Not the crate's: its box stands at the bottom of the front's place, a case's shadow round it was a dark frame.
            // Nor a series' stack of cases: each has its own shadow, `series_look::collage`.)
            art::soft_shadow(c, fx + 10, fy + 12, fw, fh, 24, 0.45 * turned, (0, lay.floor));
        }
        match front {
            Some(front) if turned >= 1.0 => {
                // On its way into the console (`paint_insert`): not here as well.
                if !self.cabinet.insert.as_ref().is_some_and(|i| i.item == item) {
                    // Playful animations: turned to you, it rocks once on its bottom edge.
                    let rock = if playful { pocket_ui::playful::wiggle(pocket_ui::playful::secs(self.moved_at, now) - SHELF_TURN.as_secs_f32()) } else { 0.0 };
                    self.paint_ajar_game(c, item, sm, &front, (fx, fy), show_clip);
                    art::blit_sheared(c, &front, fx, fy, rock * front.h as f32, 256, show_clip);
                }
                // Going into the console, the case turns its spine to you again where it stands (`insert_scene::Home`), as it
                // turned its front to you coming out.
                // Its place is the row's: where the shelf shows its spine, lifted out of the row, in the row's own scale (it used to
                // stay up where its front had been, a bare grey spine hanging over the shelf, ritual review 14.11.).
                if self.cabinet.insert.as_ref().is_some_and(|ins| ins.item == item) {
                    let rise = (self.openness(sel) * lay.lift as f32).round();
                    // Lived-in shelf: a lying case goes home lying, out of its stack as `pull_from_stack` paints it (18.11.:
                    // it came home standing over the row while it lay in its stack as well).
                    let mut row = self.cabinet.shelf_row.take();
                    let lying = row.as_ref().and_then(|r| r.lived.get(sel)).filter(|l| matches!(l.0.pose, pocket_ui::lived_in::Pose::Flat { .. })).map(|l| (l.1, l.2));
                    let posed = row.as_mut().filter(|_| lying.is_some()).and_then(|r| self.posed_pic(r, sel));
                    self.cabinet.shelf_row = row;
                    let home = match (lying, posed) {
                        (Some((bottom, outline)), Some(pic)) => pocket_ui::insert_scene::Home {
                            spine: pic,
                            at: (
                                (lay.left + slot.0 - scroll - lived_slide(slot.1, self.openness(sel))) as f32 + slot.1 as f32 / 2.0,
                                (lay.floor - bottom) as f32 - outline as f32 / 2.0,
                            ),
                            scale: lay.f / media::shelf_scale(),
                        },
                        _ => pocket_ui::insert_scene::Home {
                            spine: self.stand_pic(item, sm, lay.f),
                            at: ((lay.left + slot.0 - scroll) as f32 + slot.1 as f32 / 2.0, lay.floor as f32 - sm.template_size().h as f32 * lay.f / 2.0 - rise),
                            // Relative to the case's own size, as `stand_pic` makes the spine.
                            scale: lay.f / media::shelf_scale(),
                        },
                    };
                    if let Some(ins) = self.cabinet.insert.as_mut() {
                        ins.scene.home = Some(home);
                    }
                }
                self.cabinet.shelf_lit = Some((front, fx, fy));
            }
            Some(front) => {
                // On its way into the console (`paint_insert`): not here as well.
                if !self.cabinet.insert.as_ref().is_some_and(|i| i.item == item) {
                    let turn = Turn {
                        cx: (fx + fw / 2) as f32,
                        cy: (fy + fh / 2) as f32,
                        scale: k,
                        turned,
                        shade: 256,
                    };
                    art::draw_turning(c, &spine, &front, sm.size(), turn, show_clip);
                }
                // A pressed while it still turns: the scene starts from its front all the same (until 14.11. there was
                // no scene and no hold then, and a quick player saw the game come straight up).
                self.cabinet.shelf_lit = Some((front, fx, fy));
            }
            // Its spine to you, as at the start of the turn: the front comes with the next frame.
            None => art::blit(c, &spine, fx + (fw - spine.w) / 2, fy + (fh - spine.h) / 2, 256, show_clip),
        }
        self.hits.row(Area::new(fx, fy, fw, fh), sel);
        self.motion.sel_y = Some(fy);
        self.hits.list(sel, n, pitch, 0.0);

        // Beside it, from its top edge: what it is. An open case has its game out beside it: the words start clear of it.
        let tx = fx + fw + 48 + if self.is_frozen_item(item) { (fw as f32 * AJAR_FRONT) as i32 } else { 0 };
        self.paint_shelf_words(c, item, tx, fy.max(sy), sx + sw - tx, sy + sh);
    }

    /// The selected entry's words beside its front, from `top` down to `bottom`: "Continue" over the last game played,
    /// the name whole (up to three lines, `title_block`), the meta line, and a system's fact sheet or a Curator or Favorites row's covers
    /// under them.
    pub(crate) fn paint_shelf_words(&mut self, c: &mut Canvas, item: Item, x: i32, top: i32, w: i32, bottom: i32) {
        use pocket_ui::type_scale::{line, px};
        if w < 120 {
            return;
        }
        let (title, _, _, meta) = self.describe(item);
        let mut panel = self.home_panel(item);
        let kicker = if self.screen == Screen::Home && item == Item::Recent(0) {
            Some("Continue")
        } else if self.is_frozen_item(item) {
            Some(super::app_frozen::FROZEN)
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
        // The baseline of the name's last line: the meta line may start higher, under it, where it needs a second line.
        let name_end;
        if matches!(item, Item::Game(..) | Item::Recent(_)) {
            // A game's name whole: a big main title and a smaller subtitle, up to three lines (two where the room under
            // them is short) before the type goes one step smaller and only then "…". The lines below stand where the
            // most lines would end, so they do not move from one game to the next (`title_block`).
            // A meta line that takes two lines here, the whole one and the short one alike, keeps two under the name (Home's
            // Continue on the Brick, Retro, Large text: the name's two lines left it one, "Super Nintendo · just now …",
            // 18.11.2026): the name goes a step smaller where it must.
            let meta_two = panel.is_none() && !self.meta_fits_one(item, &meta, w);
            let (step, meta_room) = (title_block::step(&self.text, 67), line(28, 10) * if meta_two { 2 } else { 1 } + 10);
            let room = bottom - y - meta_room;
            let mut n = ((room / step.max(1)).clamp(2, 3)) as usize;
            let star = if fav { super::app_switcher::star_room(67) } else { 0 };
            let mut lines = title_block::name_layout(&mut self.text, Face::Title, 67, &title, w - 40 - star, n);
            let mut step = step;
            if n < 3 && !title_block::is_whole(&lines, &title) {
                // Room for two lines only, and the name needs three: the type goes smaller until three fit.
                let px = title_block::px_for(&self.text, 67, 3, room);
                lines = title_block::name_layout(&mut self.text, Face::Title, px, &title, w - 40 - star, 3);
                (n, step) = (3, title_block::step(&self.text, px));
            }
            if meta_two && lines.len() as i32 * step > room {
                let px = title_block::px_for(&self.text, 67, lines.len(), room);
                lines = title_block::name_layout(&mut self.text, Face::Title, px, &title, w - 40 - star, lines.len());
                step = title_block::step(&self.text, px);
            }
            let first = y + line(67, 0) - 6;
            let (end, base) = title_block::paint(&mut self.text, c, Face::Title, &lines, x, first, pal().chalk, pal().dust, false);
            if fav {
                App::paint_title_star(c, end, base, lines[0].px, pal().chalk);
            }
            name_end = first + (lines.len() as i32 - 1) * step;
            // Where the room is short, the meta line stands at the end of it for every game, so it does not move between a
            // name of one line and one that needed the room's three (`title_block::px_for`).
            y = if room < 3 * title_block::step(&self.text, 67) { y + room } else { first + (n as i32 - 1) * step };
        } else {
            let lines = self.text.wrap_at_most(Face::Title, 67, &title, w - 40, 2);
            let last = lines.len().saturating_sub(1);
            for (k, l) in lines.iter().enumerate() {
                y += line(67, 0) - 6;
                self.draw_title(c, Face::Title, 67, x, y, w, pal().chalk, l, false, fav && k == last);
            }
            name_end = y;
        }
        // A system's short facts (its headline, Released, Sold, Inside…) under its name, as Split's panel has them.
        if matches!(item, Item::System(_)) && self.paint_system_facts(c, item, x, y + 48, w, bottom) {
            return;
        }
        let (meta, short) = match &panel {
            // A panel's own line (the day's pick and why; how to fill an empty list), shortened by its parts.
            Some(p) => (self.panel_line(&p.line, w * 2), None),
            None => (meta, self.meta_short(item)),
        };
        if !meta.is_empty() {
            y += 10;
            // Playing, paused, completed or dropped: its mark in front of the line.
            let status = self.item_status(item);
            let indent = if status != pocket_ui::library_meta::Status::NotStarted { 40 } else { 0 };
            // As many of its two lines as the room has: the last one shown ends with "…" (a second line used to be left
            // off without a mark, Large text in German on Continue, 17.11.2026).
            let step = line(28, 10);
            let fit = meta_lines(y, bottom, step);
            // Lines it may start higher: as far as the name's last line leaves it the gap it has under a name of the most lines.
            let most = fit + ((y - 10 - name_end) / step).max(0) as usize;
            if most == 0 {
                let whole = self.text.translated(&meta).to_string();
                pocket_ui::text::boxes::note_meta(&whole, &[]);
                return;
            }
            let (lines, translated, up, said) = meta_choice(&mut self.text, &meta, short.as_deref(), w - indent, fit, most);
            let said = said.to_string();
            y -= up as i32 * step;
            let whole = self.text.translated(&said).to_string();
            pocket_ui::text::boxes::note_meta(&whole, &lines);
            for (k, l) in lines.iter().enumerate() {
                y += line(28, 10);
                if k == 0 && indent > 0 {
                    pocket_ui::game_card::paint_mark(c, x + 12, y - px(28) as i32 * 7 / 20, status, pal().dust);
                }
                // Lines split at the comma are in the interface's language already: painted as they are.
                let translating = self.text.translating();
                self.text.set_translate(translating && !translated);
                self.text.draw(c, Face::Body, 28, x + indent, y, pal().dust, l);
                self.text.set_translate(translating);
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

    /// Whether the meta line of `item` (`meta`, or its short one) fits one line of the words' width `w` beside the box,
    /// after the status mark in front of it.
    fn meta_fits_one(&mut self, item: Item, meta: &str, w: i32) -> bool {
        if meta.is_empty() {
            return true;
        }
        let indent = if self.item_status(item) != pocket_ui::library_meta::Status::NotStarted { 40 } else { 0 };
        if meta_whole(&mut self.text, meta, w - indent, 1).is_some() {
            return true;
        }
        self.meta_short(item).is_some_and(|short| meta_whole(&mut self.text, &short, w - indent, 1).is_some())
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
            let checking = pocket_ui::scene_check::recording();
            if checking {
                let (ox, oy) = SHELF_ORIGIN.get();
                pocket_ui::scene_check::set_origin(ox + x0, oy);
            }
            for i in 0..row.spines.len() {
                let (x, w, _) = row.spines[i];
                if x + w + 8 <= x0 || x >= x0 + TILE_W {
                    continue;
                }
                let mut shade = self.cabinet.room.lit.case_shade();
                let mut drop = 0;
                // Lived-in shelf: leaning or lying (`posed_pic`), or a little further back or forward in the
                // shelf (forward: lower, its foot over the board's edge, painted each frame by `paint_lived_feet`).
                if let Some(&(plan, bottom, outline)) = row.lived.get(i) {
                    if plan.forward {
                        drop = forward_drop(row.f);
                    }
                    if plan.back {
                        shade = shade * 7 / 8;
                    }
                    if let Some(pic) = self.posed_pic(row, i) {
                        // Centred on its outline (`lived_in::turned`), the outline's foot on the board or on the case under it.
                        let (cx, cy) = (x - x0 + w / 2, room - bottom - outline / 2);
                        let (px, py) = (cx - pic.w / 2, cy - pic.h / 2);
                        art::blit(&mut c, &pic, px, py, shade, (0, room));
                        if checking {
                            // Its outline: lying, the picture; leaning, the footprint, its foot on the board.
                            let flat = matches!(plan.pose, pocket_ui::lived_in::Pose::Flat { .. });
                            pocket_ui::scene_check::put(format!("case {i}"), x - x0, room - bottom - outline, w, outline, pocket_ui::scene_check::Rests::Settled, flat);
                        }
                        // Lying, the little shadow on its right; leaning, none (its right edge is not upright).
                        if page != 0 && matches!(plan.pose, pocket_ui::lived_in::Pose::Flat { .. }) {
                            art::soft_shadow(&mut c, x - x0 + w, room - bottom - outline + 2, 3, outline - 2, 5, 0.3, (0, room));
                        }
                        continue;
                    }
                }
                let pic = self.row_pic(row, i, row.f);
                let (x, y) = (x - x0, room - pic.h + drop);
                art::blit(&mut c, &pic, x, y, shade, (0, room));
                if checking {
                    pocket_ui::scene_check::put_pic(format!("case {i}"), &pic, x, y, pocket_ui::scene_check::Rests::Settled, true);
                }
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

    /// A bookend right after the last case at `end`, what the player earned after it (`app_room_extras::deco_for`:
    /// the row's system's figurine, the trophy, the framed "100 %"), and one of the other props (`media::Prop::beside`,
    /// picked by `seed`, the same every time) where there is room further along before `right`, the lamp glowing in
    /// the evening and night light; whatever doesn't fit whole is left out. At the shelf's scale `f`, standing on the
    /// bottom of `c` (`room` high).
    fn paint_shelf_props(&mut self, c: &mut Canvas, end: i32, right: i32, room: i32, f: f32, seed: u32) {
        let (light, earned) = (self.cabinet.room.lit, self.room_earned());
        let shade = light.case_shade();
        let bookend = self.prop_pic(Prop::Bookend, f);
        let mut x = end + 6;
        if x + bookend.w > right || bookend.h > room {
            return;
        }
        art::blit(c, &bookend, x, room - bookend.h, shade, (0, room));
        pocket_ui::scene_check::put_pic("the bookend after the row", &bookend, x, room - bookend.h, pocket_ui::scene_check::Rests::Settled, true);
        x += bookend.w;
        let system = match self.items.first() {
            Some(Item::Game(s, _)) => self.systems.get(*s).map(|sys| sys.name.clone()),
            _ => None,
        };
        let deco: Vec<Rc<Pic>> =
            super::app_room_extras::deco_for(&earned, system.as_deref(), true).into_iter().map(|p| self.room_prop_pic(p, &earned, f)).collect();
        let widths: Vec<i32> = deco.iter().map(|p| if p.h <= room * 2 / 3 { p.w } else { 0 }).collect();
        let gap = (f * 6.0) as i32;
        for (i, at) in pocket_ui::room_deco::place(x, right, gap, &widths) {
            art::blit(c, &deco[i], at, room - deco[i].h, shade, (0, room));
            pocket_ui::scene_check::put_pic(format!("decoration {i} after the row"), &deco[i], at, room - deco[i].h, pocket_ui::scene_check::Rests::Settled, true);
            x = at + deco[i].w;
        }
        // The shelf's own prop, centred in what is left; the window hangs on the wall over the free part, clear of it,
        // painted first so the lamp's light falls on it too.
        let prop = Prop::beside(seed, room * 2 / 3).map(|prop| (prop, self.room_prop_pic(prop, &earned, f))).and_then(|(prop, pic)| {
            let at = x + ((right - x - pic.w) / 2).max(0);
            (at + pic.w <= right && pic.h <= room).then_some((prop, pic, at))
        });
        let below: Vec<pocket_ui::room_window::Below> = prop.iter().map(|(_, pic, at)| (*at, at + pic.w, room - pic.h)).collect();
        self.paint_shelf_window(c, (x, right), room, f, &below, shade);
        if let Some((prop, pic, at)) = prop {
            if prop == Prop::Lamp {
                super::app_room_extras::lamp_glow(c, &pic, at, room - pic.h, light.lamp, (0, 0, c.w, room));
            }
            art::blit(c, &pic, at, room - pic.h, shade, (0, room));
            pocket_ui::scene_check::put_pic("the prop after the row", &pic, at, room - pic.h, pocket_ui::scene_check::Rests::Settled, true);
        }
    }

    /// The window (`pocket_ui::room_window`, Cozy extras → Window) on the wall over the free part of the shelf from
    /// `free.0` to `free.1`, in the tile `room` high down to the board, clear of what stands there (`below`); left out
    /// where it doesn't fit whole. At the shelf's scale `f`, in the room's light (`shade`).
    fn paint_shelf_window(&mut self, c: &mut Canvas, free: (i32, i32), room: i32, f: f32, below: &[pocket_ui::room_window::Below], shade: u32) {
        let Some((win, shadow)) = self.room_window_pic(f * pocket_ui::room_window::SCALE) else { return };
        let gap = (f * 6.0).round() as i32;
        let Some((x, y)) = pocket_ui::room_window::shelf_spot(free.0, free.1, room, below, (win.pic.w, win.pic.h), gap) else { return };
        if pal().void != 0 {
            shadow.paint(c, x, y, 0.3, (0, room));
        }
        art::blit(c, &win.pic, x, y, shade, (0, room));
        if pocket_ui::scene_check::recording() {
            use pocket_ui::scene_check::{self as sc, Rests};
            sc::wall("the wall beside the row", free.0, 0, free.1, room, None);
            let (fx, fy, fw, fh) = win.frame;
            sc::put("the window", x + fx, y + fy, fw, fh, Rests::Hung, true);
            sc::surface_here("the window's sill", y + win.sill_y, 0, x + win.sill.0, x + win.sill.1);
            let (px, py, pw, ph) = win.piece;
            // In front of the glass on purpose: its box is not one the window may not be behind (not solid).
            sc::put("the piece on the window's sill", x + px, y + py, pw, ph, Rests::Settled, false);
        }
    }

    /// A prop at the shelf's scale `f` (`Prop::pic` is at `media::shelf_scale()`), made once.
    fn prop_pic(&mut self, prop: Prop, f: f32) -> Rc<Pic> {
        let key = (prop, (f * 100.0) as i32);
        if let Some(p) = self.cabinet.shelf_props.get(&key) {
            return p.clone();
        }
        let p = Rc::new(media::rescaled(prop.pic(), f / media::shelf_scale()));
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
        // Above the hints by 112 lines, 960's share of them under 960 lines (`kit::chrome`).
        Some((middle - size / 2).min(h - pocket_ui::kit::chrome(112, h) - size).max(0))
    }

    /// The top of the tallest spine in the Shelf look (`shelf_flash_top`'s spines); None in the other looks.
    pub(crate) fn shelf_spines_top(&self, w: i32, h: i32) -> Option<i32> {
        if self.theme != super::Theme::Shelf {
            return None;
        }
        let tallest = self.cabinet.shelf_row.as_ref().map_or(TALLEST, |r| r.tallest);
        let lay = shelf_layout(w, h, tallest);
        Some(lay.floor - (tallest * lay.f) as i32)
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
    if c.n() != 1 {
        // Painted natively (docs/dev/native-paint.md): each design column's share as below, two of a screen pixel averaged.
        let lo = if x1 > x0 { x0 } else { x0 - n + 1 };
        let share: Vec<u32> = (lo..lo + n)
            .map(|col| {
                let a = ((if x1 > x0 { col - x0 } else { x0 - col }) * 255 / n) as u32;
                a * a / 255
            })
            .collect();
        // One mix per screen pixel with the mean share of its four design pixels (within a step of the old picture,
        // a quarter of the work: the fade covered both ends of every shelf move).
        let at = |col: i32| if (lo..lo + n).contains(&col) && (0..c.w).contains(&col) { share[(col - lo) as usize] } else { 0 };
        let (sc0, sc1) = (lo.max(0) / 2, ((lo + n).min(c.w) - 1) / 2 + 1);
        if y0 >= y1 || sc0 >= sc1 {
            return;
        }
        let cols: Vec<u32> = (sc0..sc1).map(|sc| at(2 * sc) + at(2 * sc + 1)).collect();
        let (_, sr0, _, gy) = c.cell(0, y0);
        let (_, sr1, _, _) = c.cell(0, y1 - 1);
        let stride = c.stride();
        let rows = (c.pixels().len() / stride.max(1)) as i32;
        let px = c.pixels_mut();
        for sr in sr0.max(0)..(sr1 + 1).min(rows) {
            let top = y0 - gy as i32 + 2 * (sr - sr0);
            let inside = (top >= y0) as u32 + (top + 1 < y1) as u32;
            for (p, k) in px[sr as usize * stride + sc0 as usize..][..cols.len()].iter_mut().zip(&cols) {
                let a = (k * inside + 2) / 4;
                if a > 0 {
                    *p = mix(*p, page, a);
                }
            }
        }
        return;
    }
    for k in 0..n {
        let col = if x1 > x0 { x0 + k } else { x0 - k };
        if col < 0 || col >= c.w {
            continue;
        }
        let a = (k * 255 / n) as u32;
        let a = a * a / 255;
        for row in y0..y1 {
            let i = (row * c.w + col) as usize;
            c.pixels_mut()[i] = mix(c.pixels_mut()[i], page, a);
        }
    }
}

#[cfg(test)]
mod shelf_tests {
    use super::*;

    #[test]
    fn r36s_readers_cabinet_waits_for_the_game_before_reading_a_requested_cover() {
        let _guard = super::super::READER_GAME_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let r = Reader::start();
        pocket_ui::covers::set_game_on(true);
        r.ask(PathBuf::from("no-cabinet-cover.png"), Kind::Cover(Medium::N64, 60));
        std::thread::sleep(Duration::from_millis(300));
        let early = r.take();
        pocket_ui::covers::set_game_on(false);
        assert!(early.is_empty(), "a foreground-priority cover job still belongs to the background reader");
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until {
            if !r.take().is_empty() { return; }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the cover request was lost");
    }

    /// The meta line that takes two lines breaks at its comma where both parts fit: the system, then why the game is there
    /// whole, never one word of it alone on the second line (640x480 German Large, Rediscover, 18.11.2026). At every
    /// width: the words in order, nothing cut, and the second line no lone word while the reason fits a line of its own.
    #[test]
    fn a_shelf_meta_line_breaks_at_its_comma_not_before_a_lone_word() {
        let mut text = Text::new();
        text.set_translate(false);
        let meta = "Game Boy Advance, heute vor einem Jahr gespielt";
        let why = "heute vor einem Jahr gespielt";
        for width in (150..1200).step_by(7) {
            let (lines, _) = meta_wrap(&mut text, meta, width, 2);
            if lines.len() == 2 && !lines.iter().any(|l| l.ends_with('…')) {
                assert_eq!(lines.join(" "), meta, "width {width}: {lines:?}");
                if text.width_plain(Face::Body, 28, why) <= width && text.width_plain(Face::Body, 28, "Game Boy Advance,") <= width {
                    assert_eq!(lines[1], why, "width {width}: the reason whole on its own line: {lines:?}");
                }
            }
            let (one, _) = meta_wrap(&mut text, meta, width, 1);
            assert!(one.len() <= 1, "width {width}: one line asked, {one:?}");
        }
    }

    #[test]
    fn the_selected_shelf_board_stays_above_the_hints_when_there_is_headroom() {
        use pocket_ui::type_scale::{self, TextSize};
        for size in [TextSize::Normal, TextSize::Medium, TextSize::Large] {
            type_scale::set(size);
            for h in [720, 768, 800, 960, 1080, 1440] {
                for medium in Medium::ALL {
                    let shelves = (0..3).map(|s| {
                        (s, vec![cabinet::Case { item: Item::Game(s, 0), ff: FormFactor::of("snes"), medium }])
                    }).collect();
                    let cab = cabinet::Cabinet::build(Vec::new(), Vec::new(), Vec::new(), shelves);
                    let pitch = cab.pitch(1);
                    // The showcase leaves exactly enough room for the selected shelf down to the plinth line.
                    let top = scene_bottom(h) - pitch;
                    let scroll = App::cabinet_scroll_for(&cab, h, 1, top, 0);
                    let board_bottom = cab.y(1) - scroll + pitch;
                    assert!(board_bottom <= hint_band_top(h), "{medium:?}, {size:?}, {h} lines: the shelf board reaches {board_bottom}");
                }
            }
        }
    }

    #[test]
    fn downloaded_spines_wake_a_shelf_with_cached_placeholders() {
        let mut cabinet = CabinetUi::default();
        let pending = PathBuf::from("new-rom.gba");
        let ready = PathBuf::from("old-rom.gba");
        let existing_photo = PathBuf::from("old-rom.png");
        cabinet.side_files.insert(pending.clone(), None);
        cabinet.side_files.insert(ready.clone(), Some(existing_photo.clone()));
        let painted = cabinet.colors_gen;

        cabinet.spines_changed();

        assert_ne!(cabinet.colors_gen, painted, "already painted shelf tiles must ask for the new photo");
        assert!(!cabinet.side_files.contains_key(&pending), "a previously missing spine is looked up again");
        assert_eq!(cabinet.side_files.get(&ready), Some(&Some(existing_photo)), "ready spines stay cached");
    }

    /// On every supported size the showcase has its room without the selected shelf sinking behind the top board (its
    /// spines cut at the top lose the start of their titles), for the tallest shelf of every kind of case, and the room is
    /// at least what the TV and the case need (`SHOW_MIN_SHARE`).
    #[test]
    fn the_showcase_never_cuts_the_spines_on_the_supported_sizes() {
        // The layout's heights: 640x480 and 720x720 are laid out at 960 and 1440 lines, the others natively (720: the Smart
        // Pro); 1280 stays as a tall in-between size.
        for h in [960, 1280, 1440, 768, 720, 1080, 800] {
            let f = media::shelf_scale_for(h);
            for m in Medium::ALL {
                let pitch = (m.template_size().h as f32 * f).round() as i32 + cabinet::HEADROOM + cabinet::BOARD;
                let (room, _, sink) = showcase(h, m, pitch, 0);
                assert_eq!(sink, 0, "{m:?} at {h} lines: the shelf would sink {sink} lines behind the board");
                // Rounded as `showcase` rounds what it wants (0.15 of 720 is 108.00001 in f32).
                assert!(room >= (h as f32 * show_min_share(h)).round() as i32, "{m:?} at {h} lines: the showcase has {room}");
            }
        }
    }

    #[test]
    fn warming_many_box_fronts_without_a_shelf_painter_releases_old_pixels_but_keeps_active_sources() {
        let mut cabinet = CabinetUi::default();
        let look = Look { color: 0x777777, band: 0x555555, plastic: 0x333333, tape: false };
        let key = |i| (CaseKey {
            rom: PathBuf::from(format!("/roms/gb/{i}.gb")), medium: Medium::Gb, look,
            case: CaseStickers { completed: true, ..Default::default() },
            ajar: false,
            sealed: false,
            photo: false,
        }, 3000);
        let mut probes = Vec::new();
        for i in 0..100 {
            // The productive stand_front/box_front path caches these same owned Pic/Rc pairs. No paint_shelf or
            // frame advancement is needed for cache lifetime; the idle warmer may be the only thing making fronts.
            let spine = Rc::new(Pic::new(24, 200, 0xff10_0000 + i));
            let front = Rc::new(Pic::new(140, 200, 0xffa0_0000 + i));
            cabinet.bare_fronts.remember(&front, Pic::new(140, 200, 0xffb0_0000 + i));
            let bare = cabinet.bare_fronts.bare(&front);
            probes.push((Rc::downgrade(&spine), Rc::downgrade(&front), Rc::downgrade(&bare)));
            if i == 0 {
                cabinet.box_game = Some((front.clone(), false, Rc::new(Pic::new(80, 90, 0xff00_ff00))));
            } else if i == 1 {
                cabinet.insert = Some(Inserting {
                    at: Instant::now(), item: Item::Game(0, 1),
                    scene: Scene {
                        rig: None,
                        rig_asked: false,
                        system: "gb".into(), ff: FormFactor::of("gb"),
                        medium: Medium::Gb, look, title: "Active launch".into(), kind: media::Game::of("gb", "1.gb"),
                        has_cover: true, front: front.clone(), from: (0, 0, 140, 200), game: None, game_back: None, cozy: false, at_drive: None,
                        home: None,
                        power_on: None,
                        plain_lit: None,
                        plain_foot: None,
                        tucked: Vec::new(),
                        tuck_p: None,
                    },
                    lead: Duration::ZERO,
                    tear: None,
                    kept: None,
                    cab_body: None,
                    shadow: None,
                });
            }
            cabinet.cache_front(key(i), spine, front, true);
            assert!(cabinet.shelf_shown.len() <= 12, "actual cache entries remain bounded during 100 warm misses");
            if i >= 2 {
                let (_, held) = cabinet.cached_front(&key(2), false).expect("a repeatedly used front stays hot");
                assert!(Rc::ptr_eq(&held, &probes[2].1.upgrade().unwrap()));
            }
        }
        assert_eq!(cabinet.frame, 0, "the bound does not depend on either look's frame counter");
        assert!(probes[0].0.upgrade().is_none(), "evicted spine pixels have no other owner");
        for i in [0, 1] {
            let front = probes[i].1.upgrade().expect("box and launch retain their own decorated front");
            let bare = cabinet.bare_fronts.bare(&front);
            assert_eq!(bare.px[0], 0xffb0_0000 + i as u32, "the cartridge still has the correct bare label after eviction");
            assert!(Rc::ptr_eq(&bare, &probes[i].2.upgrade().unwrap()));
        }
        for (_, front, bare) in &probes[3..80] {
            assert!(front.upgrade().is_none() && bare.upgrade().is_none(), "old front and bare pixel buffers are released together");
        }
        cabinet.box_game = None;
        cabinet.insert = None;
        cabinet.prune_fronts();
        for i in [0, 1] {
            assert!(probes[i].1.upgrade().is_none() && probes[i].2.upgrade().is_none(), "finished box/launch sources are released too");
        }
        // The same owner-cache is also used while the box's sticker sheet changes GOTY years, with neither look
        // painting underneath it. CaseKey changes at a fixed ROM must obey the same lifetime bound.
        let old = probes[2].1.clone();
        for year in 1970..2070 {
            let mut changed = key(0);
            changed.0.case.goty = Some(year);
            cabinet.cache_front(changed, Rc::new(Pic::new(24, 200, 0xff11_1111)), Rc::new(Pic::new(140, 200, 0xff22_2222)), true);
        }
        assert!(old.upgrade().is_none(), "the formerly hot entry is released when sheet edits replace it");
        assert!(cabinet.shelf_shown.len() <= 12);
    }

    /// At every screen size pocket-ui runs on, the shelf, the lifted spine and the shown front stay inside the screen,
    /// between the top line and the hints, and the front never runs into the shelf.
    #[test]
    fn the_shelf_and_the_shown_front_fit_every_screen() {
        for (w, h) in [(1240, 1080), (1920, 1080), (1280, 960), (1280, 720), (640, 480), (1024, 768)] {
            let lay = shelf_layout(w, h, TALLEST);
            assert!(lay.floor + lay.board_h <= pocket_ui::kit::content_bottom(h), "{w}x{h}: the board over the hints");
            let tallest = (TALLEST * lay.f).round() as i32;
            assert!(
                lay.floor - tallest - lay.lift >= lay.show.1 + lay.show.3,
                "{w}x{h}: the lifted DVD case under the shown front"
            );
            assert!(lay.show.1 >= 62 && lay.show.3 > 0, "{w}x{h}: under the top line");
            for m in Medium::ALL {
                let k = show_k(&lay, m, &Medium::ALL);
                let (fw, fh) = media::front_size(m, k);
                assert!(fh <= lay.show.3 && fw <= lay.show.2 / 2, "{w}x{h} {m:?}: front {fw}x{fh} in {:?}", lay.show);
                assert!(
                    k * media::shelf_scale() >= lay.f || fh as f32 >= lay.show.3 as f32 * 0.6,
                    "{w}x{h} {m:?}: shown smaller than on the shelf"
                );
            }
        }
        let big = shelf_layout(1240, 1080, TALLEST);
        let carts = shelf_layout(1240, 1080, Medium::Snes.template_size().h as f32);
        assert!(
            carts.show.3 > big.show.3 && carts.f == big.f,
            "a shelf of cartridge boxes: the same spines, more room for the front"
        );
        assert_eq!(big.f, 3.0, "the Mini V2: three screen pixels a template pixel");
        // The Brick's spines take the Plus's share of the height (0.6.1: they were 1.5, a quarter smaller).
        let (brick, plus) = (shelf_layout(1024, 768, TALLEST), shelf_layout(1280, 960, TALLEST));
        assert_eq!((brick.f / 768.0, plus.f / 960.0), (2.0 / 768.0, 2.5 / 960.0), "the Brick's spines, the Plus's");
        // The SNES box (0.1.4) lies wide when shown (on the shelf it stands on its short side): its real size, wider
        // than a GBA box's.
        let (snes_w, _) = media::front_size(Medium::Snes, show_k(&big, Medium::Snes, &Medium::ALL));
        let (gba_w, gba_h) = media::front_size(Medium::Gba, show_k(&big, Medium::Gba, &Medium::ALL));
        assert!(snes_w as f32 >= big.show.2 as f32 * 0.3, "a SNES box is shown large on the Mini V2: {snes_w} of {}", big.show.2);
        // One scale for all (the owner, 2026-10-29): a SNES box half as wide again as a GBA box (184 to 118 mm), a PS2
        // case taller than both.
        assert!(snes_w as f32 >= gba_w as f32 * 1.4, "SNES {snes_w}, GBA {gba_w}");
        let (_, ps2_h) = media::front_size(Medium::Ps2, show_k(&big, Medium::Ps2, &Medium::ALL));
        assert!(ps2_h as f32 >= gba_h as f32 * 1.5, "PS2 {ps2_h}, GBA {gba_h}");
    }

    /// The shown case 1.4 times its old size (a player's wish, r/SBCGaming 17.11.2026), at every panel pocket-ui runs on
    /// (the H700 panels at their layout size, `scaled_layout`) and on every kind of row: a system's page of one kind of
    /// case and Home's mix. Where it stands (`paint_shelf`: at the room's left, two thirds down its free height) it never
    /// reaches the top line (status bar, system name), the lifted spine, the shelf or the hints, its words beside it
    /// keep a quarter of the room even with an open case's game out at its side, it is never smaller than before, and
    /// on the 4:3 panels a row of cartridge boxes gets the whole 1.4.
    #[test]
    fn the_shown_case_grows_by_two_fifths_where_the_room_lets_it_and_never_runs_into_anything() {
        let home = [Medium::Keep, Medium::Gba, Medium::Snes, Medium::Ps1, Medium::Ps2, Medium::Ds];
        let mut rows: Vec<Vec<Medium>> = Medium::ALL.iter().map(|&m| vec![m]).collect();
        rows.push(home.to_vec());
        rows.push(Medium::ALL.to_vec());
        // 640x480 and 720x480, 720x720, Brick, Mini V1, Mini V2, a 720-line TV, 1280x800, Pocket 5 / Flip 2.
        for (w, h) in [(1280, 960), (1440, 960), (1440, 1440), (1024, 768), (1240, 1080), (1280, 720), (1280, 800), (1920, 1080), (1920, 1152)] {
            for row in &rows {
                let tallest = row.iter().map(|m| m.template_size().h as f32).fold(0.0, f32::max);
                let lay = shelf_layout(w, h, tallest);
                let (sx, sy, sw, sh) = lay.show;
                for &m in row {
                    let k = show_k(&lay, m, row);
                    let (fw, fh) = media::front_size(m, k);
                    let (bw, bh) = media::front_size(m, show_k_before(&lay, m));
                    let at = format!("{w}x{h} {m:?} in {row:?}: front {fw}x{fh} (was {bw}x{bh}) in {:?}", lay.show);
                    assert!(fw >= bw && fh >= bh, "{at}: smaller than before");
                    assert!(fw as f32 <= bw as f32 * SHOW_BIGGER + 1.0, "{at}: more than 1.4 times");
                    assert!(fw == bw || fw as f32 >= bw as f32 * 1.07, "{at}: a step too small to be worth the words' room");
                    let fy = sy + (sh - fh) * 2 / 3;
                    assert!(fy >= sy && sy >= 62, "{at}: into the top line");
                    let spine_top = lay.floor - (tallest * lay.f).round() as i32 - lay.lift;
                    assert!(fy + fh <= spine_top, "{at}: into the lifted spine");
                    assert!(lay.floor + lay.board_h <= pocket_ui::kit::hint_bar_top(h), "{at}: shelf into the hints");
                    let words = sx + sw - (sx + fw + 48 + (fw as f32 * AJAR_FRONT) as i32);
                    assert!(words * 4 >= sw, "{at}: the words beside an open case keep {words} px");
                }
                let same: Vec<f32> = row.iter().map(|&m| show_k(&lay, m, row)).collect();
                if row.iter().all(|m| show_k_before(&lay, *m) == show_k_before(&lay, row[0])) {
                    assert!(same.iter().all(|k| *k == same[0]), "{w}x{h} {row:?}: one scale for the row, {same:?}");
                }
            }
        }
        // The 4:3 panels (Brick, 640x480 and the Mini V1): a system's page of cartridge boxes, the full 1.4.
        for (w, h) in [(1024, 768), (1280, 960)] {
            for m in [Medium::Snes, Medium::N64, Medium::Gba, Medium::Gb, Medium::Ps1] {
                let tallest = m.template_size().h as f32;
                let lay = shelf_layout(w, h, tallest);
                let (fw, _) = media::front_size(m, show_k(&lay, m, &[m]));
                let (bw, _) = media::front_size(m, show_k_before(&lay, m));
                assert!(fw as f32 >= bw as f32 * 1.36, "{w}x{h} {m:?}: {fw} against {bw} before");
            }
        }
    }

    #[test]
    fn the_meta_line_shows_only_the_lines_that_fit_so_its_last_ends_with_an_ellipsis() {
        assert_eq!(meta_lines(100, 200, 40), 2);
        assert_eq!(meta_lines(100, 179, 40), 1, "room for one: one line, with its \"…\", not a second one left off");
        assert_eq!(meta_lines(100, 139, 40), 0);
        assert_eq!(meta_lines(300, 200, 40), 0);
    }

    #[test]
    fn a_row_of_few_games_stands_in_from_the_shelf_end_and_a_full_one_does_not() {
        assert_eq!(shelf_lead(60, 600), 180, "one spine: a third of the free room in");
        assert_eq!(shelf_lead(299, 600), (600 - 299) / 3);
        assert_eq!(shelf_lead(300, 600), 0, "half the shelf or more: from the end");
        assert_eq!(shelf_lead(2000, 600), 0, "a long row scrolls from the end");
    }

    #[test]
    fn a_frozen_games_spine_keeps_its_width_and_its_place_on_the_board() {
        let mut text = pocket_ui::text::Text::new();
        let look = Look { color: 0x3a2a6a, band: 0x7a6cb0, plastic: 0x9c99a2, tape: false };
        for (system, f) in [("gba", 3.0), ("ps2", 3.5), ("nds", 2.0), ("snes", 1.5)] {
            let spine = Pic::new(40, 200, 0xff11_2233);
            let bottom: Vec<u32> = spine.px.clone();
            let ajar = ajar_spine(&mut text, spine, system, Path::new("/roms/Game (USA).rom"), f, look);
            assert_eq!(ajar.w, 40, "{system}: the same width, so the row's sizes stay equal");
            assert_eq!(ajar.h, 200 + ajar_peek(f), "{system}");
            assert_eq!(&ajar.px[(ajar_peek(f) * 40) as usize..], &bottom[..], "{system}: the case itself as it was");
            assert!(ajar.px[..(ajar_peek(f) * 40) as usize].iter().any(|p| p >> 24 != 0), "{system}: the game shows");
        }
    }

    #[test]
    fn a_lone_case_is_shown_in_the_middle_of_the_cabinet_not_in_its_corner() {
        // A slot at the far left of a 608 px cabinet on a 640 px screen: a one-game shelf (60 px) shows it centred...
        assert_eq!(show_centre(70.0, 100.0, 0.0, 640.0, 40.0, 60, 608), 320.0);
        // ...a full shelf keeps the case over its own slot, clamped to the screen.
        assert_eq!(show_centre(70.0, 100.0, 0.0, 640.0, 40.0, 900, 608), 90.0);
        assert_eq!(show_centre(300.0, 100.0, 0.0, 640.0, 40.0, 900, 608), 300.0);
    }

    #[test]
    fn an_open_case_s_game_keeps_clear_of_the_tv_from_every_slot() {
        // The room up to the TV ends at 760; the case is 200 wide and its game stands 60 out at its right side.
        let (fw, reach, right, margin) = (200.0, 60.0, 760.0, 40.0);
        for slot in (0..=1024).step_by(8) {
            let cx = show_centre(slot as f32, fw, reach, right, margin, 900, 960);
            assert!(cx + fw / 2.0 + reach <= right - margin, "slot {slot}: the game reaches {} past {}", cx + fw / 2.0 + reach, right - margin);
            assert!(cx - fw / 2.0 >= margin, "slot {slot}: the case left the screen");
        }
        // A lone case in the middle: the case and its game together.
        assert_eq!(show_centre(70.0, 100.0, 60.0, 640.0, 40.0, 60, 608), 290.0);
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

/// A game's own file (a game on a system's shelf, a recent game), not one of the shelf's boxes (Curator, a sticker,
/// Just Play, a system, PC streaming): only those are made home-made when they have no box art.
fn is_game_file(info: &Info) -> bool {
    !info.rom.as_os_str().is_empty() && !matches!(info.system.as_str(), "curator" | "sticker" | "streaming" | "favorites" | "search")
}

#[cfg(test)]
mod file_source_tests {
    use super::*;
    #[test]
    fn file_source_rule_cover_workers() {
        let d = tempfile::tempdir().unwrap(); let file = d.path().join("cover.png");
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.write_header().unwrap().write_image_data(&[60, 120, 220]).unwrap();
        }
        for (label, bytes) in crate::file_faults::faults(&png) {
            let _ = std::fs::remove_file(&file);
            if let Some(bytes) = bytes { std::fs::write(&file, bytes).unwrap(); }
            for medium in Medium::ALL {
                let _ = read(&file, Kind::Color(medium), None);
                let cover = read(&file, Kind::Cover(medium, 240), None);
                if label == "valid" { assert!(matches!(cover, Got::Cover(Some(ref img)) if img.w > 0 && img.h > 0), "{medium:?}: valid cover did not reach the worker"); }
                if label == "missing" { assert!(matches!(cover, Got::Cover(None))); }
            }
        }
    }
}
