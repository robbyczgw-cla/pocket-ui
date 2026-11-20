//! Part of the pocket-ui binary: the CRT TV on the Cabinet's top board (`pocket_ui::cabinet_tv`), showing the selected
//! game's screenshot. It stands at the board's right end, in the showcase; the case out of the shelf keeps clear of
//! it (`paint_out` reads [`TvState::left`]). Its size follows the screen, not the case, so it doesn't change as the
//! selection moves: about a fifth of the screen's height, a whole number of screen pixels a template pixel where it is
//! two or more, left out where its glass would be under 60 px high or the showcase has no room for it.
//! Its place is the cabinet's (`App::cabinet_plan`), the same on every shelf; a handheld game's handheld stands in it
//! instead (`cabinet_tv::handheld`), the game on its display.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use pocket_ui::cabinet_art::{self as art, Pic};
use pocket_ui::cabinet_tv::{self, Body, Loader, Want};
use pocket_ui::gfx::Canvas;

use super::App;

/// The TV's height as a share of the screen's.
const SHARE: f32 = 0.21;
/// From the cabinet's right inner wall.
const GAP: i32 = 30;
/// The warm light from above on the dark page, caught by a console of dark plastic (`cabinet_art::light_from_above`).
const ROOM_LIGHT: u32 = 0xffe9cc;

#[derive(Default)]
pub(crate) struct TvState {
    loader: Option<Loader>,
    /// The TV's body at a scale (hundredths) and the screen's position on it; or the handheld's (`cabinet_tv::handheld`,
    /// the key's second part its picture) in the TV's place.
    body: Option<((i32, usize), Rc<Body>)>,
    /// What the glass shows, the picture before it while the crossfade runs, and when it began.
    shown: Option<(Want, Option<Rc<Pic>>)>,
    /// Files changed at the same paths: ask the worker again, keeping the old picture until its replacement arrives.
    refresh: bool,
    prev: Option<Option<Rc<Pic>>>,
    since: Option<Instant>,
    /// The TV with its screen as painted last (the same picture, at rest, is not made again).
    frame: Option<(usize, usize, u32, Rc<Pic>)>,
    /// The TV's left edge this frame (None: no TV): the case out of the shelf stays left of it.
    pub(crate) left: Option<i32>,
    /// The TV's place this frame (x, y, w, h): the same on every shelf; a handheld stands in it instead of the TV.
    pub(crate) place: Option<(i32, i32, i32, i32)>,
    /// What stands there: true for a handheld.
    pub(crate) handheld: bool,
    /// The evening TV glow for where the TV stands now (`tv_glow`), kept until it moves or the darkness steps.
    glow: Option<pocket_ui::tv_glow::Glow>,
    /// The showcase (prototype, `pocket_ui::showcase`): the game on the glass and since when it is chosen.
    rest: Option<(Want, Instant)>,
    /// Whether the game resting on the glass may play (checked once per game: a few file lookups).
    plays: Option<(Want, bool)>,
    /// The run playing on the glass now.
    live: Option<Live>,
    /// The game whose run ended: it doesn't start again until another game was chosen.
    played: Option<Want>,
    /// Runs told to stop, waited for off the frame.
    reap: Vec<(std::process::Child, Instant)>,
    /// The redraw at the end of the rest was asked for.
    dwell_asked: bool,
    /// Where the TV stood in the last whole frame (its top left), for frames that paint only the TV.
    at: Option<(i32, i32)>,
    /// A run being started off the frame (copying pocket-core into memory, `showcase::command`): the game, its frame
    /// file, and the run once it is there. Runs that come for a game no longer resting are stopped as they arrive.
    starting: Option<(Want, PathBuf, Receiver<Started>)>,
    orphans: Vec<Receiver<Started>>,
    /// A pressed on the playing TV: the picture growing out of it into the game (`showcase_hand_over`).
    pub(crate) grow: Option<Grow>,
    /// The selected game's console beside the TV (`console_beside`): its picture made at a size, lit for the dark page
    /// where it is dark plastic, and its shadow (key: the system's picture, the box it fits, the page's light).
    /// The last few, newest last: going back to a shelf finds its console made.
    console: Vec<((usize, i32, i32, bool), Rc<Pic>, Rc<art::ShapeShadow>)>,
    /// The console's place this frame (x, y, w, h): left of the TV, the same on every shelf (a narrower console stands
    /// at its right end, by the TV); None where it has no room (or no TV).
    pub(crate) console_place: Option<(i32, i32, i32, i32)>,
    /// Where the console stands this frame (None: a handheld's game, or one without a console picture).
    pub(crate) console_at: Option<(i32, i32, i32, i32)>,
    /// The shadow of the handheld standing in the TV's place, in its shape (key: the body's).
    hand_shadow: Option<((i32, usize), Rc<art::ShapeShadow>)>,
    /// The power light of the handheld in the TV's place: the battery it shows (`frame_led`, read every 10 s).
    hand_light: Option<pocket_ui::frame_led::Clock>,
}

/// A run started off the frame: the process, its core, and whether A may go on from it (`showcase::may_continue`).
type Started = Option<(std::process::Child, PathBuf, bool)>;
use std::sync::mpsc::Receiver;

/// A on the playing TV (`App::showcase_hand_over`): the TV's picture grows to where pocket-core draws the game, which
/// goes on from that very frame.
pub(crate) struct Grow {
    rom: PathBuf,
    /// The state the run handed over, for pocket-core (`showcase::STATE_ENV`).
    pub(crate) state: PathBuf,
    /// The glass on the screen at A, and the TV's picture on it then.
    from: (i32, i32, i32, i32),
    tv: Rc<Pic>,
    /// The game's own picture: as the run stopped at it, and scaled to where the game will draw it.
    rx: Option<std::sync::mpsc::Receiver<Option<(pocket_ui::image::Image, pocket_ui::launch_picture::Prepared)>>>,
    game: Option<(pocket_ui::image::Image, pocket_ui::launch_picture::Prepared)>,
    asked: Instant,
    /// When it began to grow (once the game's picture was there, or given up on), and how long it takes.
    began: Option<Instant>,
    long: std::time::Duration,
    /// pocket-core was told the picture has grown (`startup::skip_hold`).
    told: bool,
    /// The shelf as the grow's first frame painted it (its size, its pixels): later frames start from it.
    kept: Option<(i32, i32, Vec<u32>)>,
}

/// A frame was asked for by anything but the TV (`App::request_redraw`): it paints everything.
pub(crate) static FULL_REDRAW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// A frame was asked for by the TV's showcase only (`App::request_tv_redraw`).
pub(crate) static TV_REDRAW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A showcase run: the second pocket-core and the screens it writes.
pub(crate) struct Live {
    want: Want,
    child: std::process::Child,
    /// The core the run plays: A goes on from it only when the game starts on the same one.
    core: PathBuf,
    /// The game has nothing of its own yet (`showcase::may_continue`), checked as the run started.
    may: bool,
    reader: pocket_ui::showcase::Reader,
    file: PathBuf,
    started: Instant,
    pic: Option<Rc<Pic>>,
    /// Screens taken from the run.
    pub(crate) frames: u32,
}

impl TvState {
    pub(crate) fn pictures_changed(&mut self) {
        if let Some(loader) = &mut self.loader {
            loader.invalidate();
        }
        self.refresh = true;
    }

    /// Drain replies even when a page hides the Cabinet; a completed job must no longer keep frames running.
    pub(crate) fn poll(&mut self) -> bool {
        self.loader.as_mut().is_some_and(Loader::poll)
    }

    fn select(&mut self, want: Want, data_dir: &Path, inline: bool, now: Instant) {
        if self.rest.as_ref().is_none_or(|(w, _)| !same_game(w, &want)) {
            self.showcase_stop();
            self.rest = Some((want.clone(), now));
            self.played = None;
            self.dwell_asked = false;
        }
        let loader = self.loader.get_or_insert_with(|| Loader::new(data_dir));
        loader.poll();
        if inline && loader.get(&want).is_none() {
            loader.settle(&want);
        }
        if self.refresh || self.shown.as_ref().is_none_or(|(w, _)| *w != want) {
            if let Some(pic) = loader.get(&want) {
                let before = self.shown.take().map(|(_, p)| p);
                let fade = before.is_some() && pocket_ui::motion::allowed(pocket_ui::motion::Kind::Fade) && !inline;
                self.prev = if fade { before } else { None };
                self.since = fade.then_some(now);
                self.shown = Some((want, pic));
                self.refresh = false;
            }
        }
    }

    /// The TV with its screen now: the picture on the glass, crossfading from the one before.
    fn compose(&mut self, body: &Body, now: Instant) -> Rc<Pic> {
        let a = match self.since {
            Some(t) if now.duration_since(t) < cabinet_tv::FADE => {
                let x = now.duration_since(t).as_secs_f32() / cabinet_tv::FADE.as_secs_f32();
                (x * x * (3.0 - 2.0 * x) * 255.0) as u32
            }
            _ => {
                self.since = None;
                self.prev = None;
                255
            }
        };
        let ptr = |p: &Option<Rc<Pic>>| p.as_ref().map_or(0, |p| Rc::as_ptr(p) as usize);
        let live = self.live.as_ref().and_then(|l| l.pic.clone()).or_else(|| self.grow.as_ref().map(|g| g.tv.clone()));
        let cur = live.or_else(|| self.shown.as_ref().map(|(_, p)| p.clone()).unwrap_or(None));
        let prev = self.prev.clone().unwrap_or(None);
        let key = (ptr(&cur), ptr(&prev), a);
        let pic = match &self.frame {
            Some((k1, k2, k3, p)) if (*k1, *k2, *k3) == key => p.clone(),
            _ => {
                let mut p = body.pic.clone();
                // A picture made for another glass (the TV's, as a handheld game's handheld comes) is not shown on this one.
                let fits = |s: &Pic| (s.w, s.h) == (body.glass.2, body.glass.3);
                if let Some(old) = prev.as_ref().filter(|p| fits(p)) {
                    cabinet_tv::show(&mut p, body.glass, &body.mask, old, 255);
                }
                if let Some(new) = cur.as_ref().filter(|p| fits(p)) {
                    cabinet_tv::show(&mut p, body.glass, &body.mask, new, a);
                }
                let p = Rc::new(p);
                self.frame = Some((key.0, key.1, key.2, p.clone()));
                p
            }
        };
        pic
    }

    /// A showcase run is playing on the glass.
    pub(crate) fn is_live(&self) -> bool {
        self.live.is_some()
    }

    /// Stops the run on the glass (if any); the still picture comes back with a crossfade.
    pub(crate) fn showcase_stop(&mut self) {
        if let Some((_, file, rx)) = self.starting.take() {
            pocket_ui::showcase::clear(&[&file]);
            self.orphans.push(rx);
        }
        let Some(mut live) = self.live.take() else { return };
        unsafe { libc::kill(live.child.id() as libc::pid_t, libc::SIGTERM) };
        if live.pic.is_some() {
            self.prev = Some(live.pic.take());
            self.since = Some(Instant::now());
        }
        pocket_ui::showcase::clear(&[&live.file]);
        self.reap.push((live.child, Instant::now()));
    }

    /// The run being started is there: it plays from now (its game is still the one resting, `select` stops it
    /// otherwise). None from the thread: it couldn't start, and the TV keeps its still picture.
    fn poll_starting(&mut self, now: Instant) {
        let Some((_, _, rx)) = &self.starting else { return };
        match rx.try_recv() {
            Ok(Some((child, core, may))) => {
                let (want, file, _) = self.starting.take().unwrap();
                self.live = Some(Live { want, child, core, may, reader: pocket_ui::showcase::Reader::new(&file), file, started: now, pic: None, frames: 0 });
            }
            Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.played = self.starting.take().map(|(w, ..)| w);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Waits for stopped runs without blocking; one that doesn't end within 2 s is killed.
    fn reap(&mut self, now: Instant) {
        // A run that came after its game was left: stopped as it arrives.
        let mut late = Vec::new();
        self.orphans.retain(|rx| match rx.try_recv() {
            Ok(Some((child, ..))) => {
                unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
                late.push((child, now));
                false
            }
            Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
        });
        self.reap.extend(late);
        self.reap.retain_mut(|(child, since)| {
            if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
                return false;
            }
            if now.duration_since(*since) > std::time::Duration::from_secs(2) {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            true
        });
    }

    /// A screen is being made or crossfading: the frames go on.
    pub(crate) fn busy(&self, now: Instant) -> bool {
        self.loader.as_ref().is_some_and(Loader::waiting) || self.since.is_some_and(|t| now.duration_since(t) < cabinet_tv::FADE)
    }
}

/// The TV's scale for a screen `w` x `h` with `room` px of showcase above the board: None where it is left out.
pub(crate) fn tv_scale(w: i32, h: i32, room: i32) -> Option<f32> {
    let t = cabinet_tv::template();
    let want = (h as f32 * SHARE * super::app_cabinet::show_grow(room)).min(room as f32 - 12.0);
    let f = want / t.body.h as f32;
    // Whole template pixels from 2 up. A screen laid out at twice its size (640 x 480) rounds up: its TV stood a third
    // smaller than the case beside it, the game on its glass hard to make out; one step more is the case's height, and
    // 1.5 screen pixels a template pixel look even after the halving (step 2 showcase, the H700).
    // A 4:3 screen under 900 lines painted natively (the Brick's 1024x768, step 1) rounds up to the next half step: its
    // TV is 0.231 of the height, the Plus's 0.222, and the console beside it (`CONSOLE_H` of it) as large (photos of the
    // Brick, 0.6: rounded down to whole steps the TV was 0.185 and the console 60 lines, with empty room over both).
    // Other natively painted screens (the Smart Pro's 1280x720, 1280x800, 1080 lines) still round down.
    let brick_like = pocket_ui::scaled_layout::step() < 2 && h < pocket_ui::media::SHORT_SCREEN_H && w * 3 == h * 4;
    let f = if f < 2.0 {
        f
    } else if pocket_ui::scaled_layout::step() >= 2 {
        f.ceil()
    } else if brick_like {
        (f * 2.0).ceil() / 2.0
    } else {
        f.floor()
    };
    // Never more than a quarter of the width: the case out of the shelf needs the rest.
    cabinet_tv::scale_for(w / 4, room, f)
}

/// The console beside the TV: its height as a share of the TV's, the gap between them, and how wide its place is against
/// its height (a SNES or PlayStation with its pad stands whole in it; wider ones, the PC Engine, a little lower).
const CONSOLE_H: f32 = 0.42;
const CONSOLE_GAP: i32 = 14;
const CONSOLE_WIDE: f32 = 2.35;
/// Consoles kept made (`TvState::console`).
const CONSOLES_KEPT: usize = 8;
/// The TV's shadow (also the console's and a handheld's, in their shapes): how far right and down, how soft, how dark.
const SHADOW: (i32, i32, i32, f32) = (10, 12, 18, 0.45);

/// The console a game is played on, shown beside the TV: a home console with a picture of its own (`system_art`, the
/// player's colour). Not a handheld (it stands in the TV's place), not a computer, an arcade cabinet or a fantasy console.
fn console_beside(system: &str) -> Option<&'static pocket_ui::system_art::Sprite> {
    const NOT: &[&str] = &["arcade", "pc", "ports", "scummvm", "streaming", "pico-8", "tic-80", "j2me", "amiga", "c64", "zxspectrum", "atari800", "vectrex"];
    if system.is_empty() || cabinet_tv::handheld(system).is_some() || NOT.contains(&pocket_ui::system_art::canonical(system)) {
        return None;
    }
    pocket_ui::system_art::get(system)
}

impl App {
    /// The TV for the selected entry (`system`, `rom`: empty for what is not a game; `cover` its box art), standing on
    /// the top board whose top is at `board_y`; `room` the showcase's height. Sets `left` for `paint_out`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn paint_cabinet_tv(&mut self, c: &mut Canvas, system: &str, rom: &Path, cover: Option<PathBuf>, board_y: i32, room: i32, now: Instant) {
        let case_room = self.case_room(room);
        self.cabinet.tv.left = None;
        self.cabinet.tv.place = None;
        self.cabinet.tv.at = None;
        self.cabinet.tv.console_place = None;
        self.cabinet.tv.console_at = None;
        // The look step's sample cabinet has no TV: it shows how the cases look. Nor with its Cozy extra off.
        if self.sample_cover.is_some() || !pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::CabinetTv) {
            return;
        }
        let Some(f) = tv_scale(c.w, c.h, room) else { return };
        let fkey = (f * 100.0).round() as i32;
        // The TV's place: its size at this scale, at the board's right end. A handheld game's handheld stands in it.
        let t = cabinet_tv::template();
        let (pw, ph) = (((t.body.w as f32 * f).round() as i32).max(1), ((t.body.h as f32 * f).round() as i32).max(1));
        let hand = cabinet_tv::handheld(system);
        let key = (fkey, hand.map_or(0, |(sp, _)| sp as *const _ as usize));
        if self.cabinet.tv.body.as_ref().is_none_or(|(k, _)| *k != key) {
            let made = hand.and_then(|(sp, d)| cabinet_tv::handheld_body(sp, d.at, d.fuzz, d.lit, pw, ph));
            let lcd = made.is_some();
            self.cabinet.tv.body = Some((key, Rc::new(made.unwrap_or_else(|| cabinet_tv::body(f)))));
            self.cabinet.tv.handheld = lcd;
            self.cabinet.tv.frame = None;
        }
        let body = self.cabinet.tv.body.as_ref().map(|(_, b)| b.clone()).unwrap();
        let (_, _, gw, gh) = body.glass;
        let lcd = self.cabinet.tv.handheld;
        let want = Want { system: system.to_string(), rom: rom.to_path_buf(), cover, w: gw, h: gh, lcd };
        let inline = self.cabinet.inline;
        let data_dir = self.data_dir.clone();
        let unplayed = !self.play_evidence(rom).ever_played();
        // After dark: the TV's warm light on the board, the shelves and their cases round it (`tv_glow`), under the TV.
        let glow = self.cabinet.room.tv_dark * if pocket_ui::palette::pal().is_light() { 0.5 } else { 1.0 };
        // The console beside the TV, or the handheld in its place, while the Cozy scene has it in front: dark and out.
        let shows = self.console_beside_shows(now);
        let tv = &mut self.cabinet.tv;
        // The new game's picture once it is made; until then the last one stays on the glass.
        tv.select(want.clone(), &data_dir, inline, now);
        // The showcase: the real game on the glass after a rest (`pocket_ui::showcase`).
        if !inline {
            if !unplayed { tv.showcase_stop(); }
            showcase_step_with(tv, &want, &data_dir, now, pocket_ui::showcase::enabled() && unplayed);
        }
        let pic = tv.compose(&body, now);
        let px = c.w - super::app_cabinet::CAB_X - art::WALL - GAP - pw;
        let (x, y) = (px + (pw - pic.w) / 2, board_y - pic.h);
        tv.left = Some(px);
        tv.place = Some((px, board_y - ph, pw, ph));
        tv.at = Some((x, y));
        if glow > 0.0 && !lcd {
            let (gx, gy, _, _) = body.glass;
            let centre = (x + gx + gw / 2, y + gy + gh / 2);
            if tv.glow.as_ref().is_none_or(|g| !g.is_for(centre, pw, (c.w, c.h), 0, glow)) {
                tv.glow = Some(pocket_ui::tv_glow::Glow::new(centre, pw, (c.w, c.h), 0, glow));
            }
            if let Some(g) = tv.glow.as_ref() {
                g.paint(c);
            }
        }
        // The console's place beside the TV: the same on every shelf, so the cases out of the shelf keep clear of it
        // there too (a handheld's shelf leaves it empty); left out where the widest case would no longer have its room.
        let (bh, bw) = ((ph as f32 * CONSOLE_H).round() as i32, (ph as f32 * CONSOLE_H * CONSOLE_WIDE).round() as i32);
        let cx = px - CONSOLE_GAP - bw;
        if cx >= case_room {
            tv.console_place = Some((cx, board_y - bh, bw, bh));
            tv.left = Some(cx);
        }
        let (sdx, sdy, blur, dark) = SHADOW;
        if let (Some(sprite), Some((cx, _, bw, bh))) = (console_beside(system).filter(|_| !lcd && shows > 0.0), tv.console_place) {
            let lit = !pocket_ui::palette::pal().is_light();
            let key = (sprite as *const _ as usize, bw, bh, lit);
            let (_, cp, shadow) = match tv.console.iter().position(|(k, _, _)| *k == key) {
                Some(i) => {
                    let made = tv.console.remove(i);
                    tv.console.push(made);
                    tv.console.last().cloned().unwrap()
                }
                None => {
                    let s = sprite.fitted(bw, bh);
                    let mut p = Pic::of(s.w as i32, s.h as i32, s.px);
                    if lit {
                        art::light_from_above(&mut p, ROOM_LIGHT);
                    }
                    let shadow = art::ShapeShadow::of(&p, sdx, sdy, blur);
                    if tv.console.len() >= CONSOLES_KEPT {
                        tv.console.remove(0);
                    }
                    tv.console.push((key, Rc::new(p), Rc::new(shadow)));
                    tv.console.last().cloned().unwrap()
                }
            };
            // At the right end of its place, by the TV, standing on the board.
            let (x, y) = (cx + bw - cp.w, board_y - cp.h);
            shadow.paint(c, x, y, dark * shows, (0, c.h));
            art::blit(c, &cp, x, y, (256.0 * shows) as u32, (0, c.h));
            // Recorded while it stands there fully; going dark it is on its way into the scene.
            if shows >= 1.0 {
                pocket_ui::scene_check::put_pic("the console beside the TV", &cp, x, y, pocket_ui::scene_check::Rests::Settled, true);
            }
            tv.console_at = Some((x, y, cp.w, cp.h));
        }
        if lcd && shows <= 0.0 {
            // The handheld is in the scene, in front: its place by the board stays empty meanwhile.
            return;
        }
        if lcd {
            // A handheld is no box: its shadow in its shape.
            if tv.hand_shadow.as_ref().is_none_or(|(k, _)| *k != key) {
                tv.hand_shadow = Some((key, Rc::new(art::ShapeShadow::of(&body.pic, sdx, sdy, blur))));
            }
            let shadow = tv.hand_shadow.as_ref().map(|(_, s)| s.clone()).unwrap();
            shadow.paint(c, x, y, dark * shows, (0, c.h));
        } else {
            art::soft_shadow(c, x + sdx, y + sdy, pic.w - 4, pic.h - sdy, blur, dark, (0, c.h));
        }
        art::blit(c, &pic, x, y, if lcd { (256.0 * shows) as u32 } else { 256 }, (0, c.h));
        // Its power light once it stands there whole (`frame_led`).
        if lcd && shows >= 1.0 {
            paint_hand_light(c, tv, system, &pic, x, y, now);
        }
        if !lcd || shows >= 1.0 {
            pocket_ui::scene_check::put_pic(if lcd { "the handheld in the TV's place" } else { "the TV" }, &pic, x, y, pocket_ui::scene_check::Rests::Settled, true);
        }
    }
}

/// The power light of the handheld standing in the TV's place, where its picture has one (`insert_scene::PlainDisplay::led`):
/// lit as the original's for the battery now (Power lights, `frame_led`), steadily, as the Cabinet is painted only when
/// something changes (a low PSP's light glows orange here, it blinks on the game's frame); Power lights off: steadily on.
fn paint_hand_light(c: &mut Canvas, tv: &mut TvState, system: &str, pic: &art::Pic, x: i32, y: i32, now: Instant) {
    use pocket_ui::frame_led;
    let shown = pocket_ui::insert_scene::scene_system(system);
    let Some((fx, fy)) = pocket_ui::insert_scene::plain_display(shown).and_then(|d| d.led) else { return };
    let model = frame_led::Model::of_system(pocket_ui::system_art::picture_id(shown));
    let look = if frame_led::live() {
        let clock = tv.hand_light.get_or_insert_with(|| frame_led::Clock::new(pocket_ui::battery::dir(), model));
        clock.set_model(model);
        clock.still(now)
    } else {
        model.steady()
    };
    let r = (pic.w as f32 / 55.0).clamp(2.0, 6.0);
    frame_led::paint_dot(c, x as f32 + fx * pic.w as f32, y as f32 + fy * pic.h as f32, r, look);
}

/// The same game on the same glass, whatever its box art: box art found while the TV plays must not restart it.
fn same_game(a: &Want, b: &Want) -> bool {
    a.system == b.system && a.rom == b.rom && a.w == b.w && a.h == b.h
}

/// One frame's turn of the showcase for the game on the glass: start a run after the rest, take its newest screen,
/// end it after `showcase::live()`.
/// `showcase_step` with the switch given (`showcase::enabled`): off, no run is started, now or later.
fn showcase_step_with(tv: &mut TvState, want: &Want, data_dir: &Path, now: Instant, enabled: bool) {
    use pocket_ui::showcase;
    if let Some(live) = &mut tv.live {
        if !same_game(&live.want, want) {
            tv.showcase_stop();
        } else if now.duration_since(live.started) >= showcase::live() {
            tv.showcase_stop();
            tv.played = Some(want.clone());
        } else if let Some(pic) = live.reader.take() {
            let first = live.pic.is_none();
            let pic = Rc::new(pic);
            live.pic = Some(pic.clone());
            live.frames += 1;
            // The TV turns on: the still picture crossfades into the running game.
            if first && pocket_ui::motion::allowed(pocket_ui::motion::Kind::Fade) {
                tv.prev = Some(tv.shown.as_ref().and_then(|(_, p)| p.clone()));
                tv.since = Some(now);
            }
        }
        return;
    }
    if !enabled || tv.played.as_ref().is_some_and(|p| same_game(p, want)) {
        return;
    }
    let Some((_, since)) = tv.rest.as_ref().filter(|(w, _)| same_game(w, want)) else { return };
    if now.duration_since(*since) < showcase::DWELL {
        return;
    }
    if tv.plays.as_ref().is_none_or(|(w, _)| !same_game(w, want)) {
        // A game already played shows the player's own last moment, as before.
        let plays = showcase::system_plays(&want.system)
            && pocket_ui::launch_picture::source(data_dir, &want.system, &want.rom).is_none()
            && Path::new(super::host_bin()).is_file()
            && showcase::core_for(&want.system, &want.rom).is_some();
        tv.plays = Some((want.clone(), plays));
    }
    if tv.plays.as_ref().is_some_and(|(_, p)| *p) {
        // The run comes from a thread (pocket-core is copied into memory first, `showcase::command`).
        if tv.starting.is_some() {
            tv.poll_starting(now);
            return;
        }
        let Some(core) = showcase::core_for(&want.system, &want.rom) else { return };
        let file = showcase::frame_file(std::process::id());
        showcase::clear(&[&file, &showcase::hand_state(&file), &showcase::hand_frame(&file)]);
        let (tx, rx) = std::sync::mpsc::channel();
        let (bin, out, w) = (PathBuf::from(super::host_bin()), file.clone(), want.clone());
        std::thread::spawn(move || {
            let log = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("pocket-showcase.log"));
            let mut spawn = showcase::command(&bin);
            let cmd = &mut spawn.cmd;
            cmd.arg("--showcase").arg(&core).arg(&w.rom).arg("--system").arg(&w.system).arg("--out").arg(&out);
            cmd.arg("--size").arg(format!("{}x{}", w.w, w.h)).arg("--seconds").arg(format!("{}", showcase::live().as_secs() + 2));
            cmd.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            cmd.stdout(log.map_or_else(|_| std::process::Stdio::null(), std::process::Stdio::from));
            // Not a game: the trace and the frame dump stay pocket-ui's and the game's.
            cmd.env_remove("POCKETUI_TRACE").env_remove("POCKETUI_FRAME_DUMP");
            let child = cmd.spawn().map_err(|e| eprintln!("showcase: {e}")).ok();
            drop(spawn);
            let may = showcase::may_continue(&w.system, &w.rom);
            let _ = tx.send(child.map(|c| (c, core, may)));
        });
        tv.starting = Some((want.clone(), file, rx));
    }
}

impl App {
    /// A frame for the TV's showcase alone: painted over the last whole frame when nothing else asked for one
    /// (`paint_tv_partial`).
    pub(crate) fn request_tv_redraw(&self) {
        TV_REDRAW.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Only the TV may be painted this frame: the showcase plays on a TV the last whole frame showed, and nothing
    /// else asked for a frame. Takes the requests either way.
    pub(crate) fn tv_partial_frame(&self) -> bool {
        let full = FULL_REDRAW.swap(false, std::sync::atomic::Ordering::Relaxed);
        let tv = TV_REDRAW.swap(false, std::sync::atomic::Ordering::Relaxed);
        let t = &self.cabinet.tv;
        tv && !full && t.at.is_some() && t.body.is_some() && t.live.as_ref().is_some_and(|l| t.rest.as_ref().is_some_and(|(w, _)| same_game(w, &l.want)))
    }

    /// The TV alone over the last whole frame: its newest screen, where it stood.
    pub(crate) fn paint_tv_partial(&mut self, c: &mut Canvas) {
        let now = Instant::now();
        let data_dir = self.data_dir.clone();
        let unplayed = self.cabinet.tv.live.as_ref().is_some_and(|l| !self.play_evidence(&l.want.rom).ever_played());
        let tv = &mut self.cabinet.tv;
        let (Some((x, y)), Some(body), Some(want)) = (tv.at, tv.body.as_ref().map(|(_, b)| b.clone()), tv.live.as_ref().map(|l| l.want.clone())) else { return };
        if !unplayed { tv.showcase_stop(); }
        showcase_step_with(tv, &want, &data_dir, now, pocket_ui::showcase::enabled() && unplayed);
        let pic = tv.compose(&body, now);
        art::blit(c, &pic, x, y, 256, (0, c.h));
        if tv.handheld {
            paint_hand_light(c, tv, &want.system, &pic, x, y, now);
        }
    }

    /// When the event loop must look at the showcase again (`about_to_wait`): the end of a rest on a game, or the
    /// next screen of a run. Stops the run when the Cabinet is not what the player sees.
    pub(crate) fn showcase_tick(&mut self, now: Instant) -> Option<Instant> {
        let tv = &mut self.cabinet.tv;
        tv.reap(now);
        let seen = self.theme == super::Theme::Cabinet
            && self.shelf_in_front()
            && self.running.is_none()
            && self.pending_launch.is_none()
            && self.launch_at.is_none()
            && self.sleep.state() == pocket_ui::sleep::State::Awake;
        let tick = now + std::time::Duration::from_millis(16);
        // The picture growing out of the TV after A: a frame each tick until it has grown; gone with the start.
        if self.launch_at.is_none() {
            self.cabinet.tv.grow = None;
        }
        let growing = self.cabinet.tv.grow.as_mut().is_some_and(|g| g.poll(pocket_ui::frame_clock::now()));
        if growing {
            self.request_redraw();
        }
        let tv = &mut self.cabinet.tv;
        if !seen {
            tv.showcase_stop();
            let waiting = !tv.reap.is_empty() || !tv.orphans.is_empty();
            return (growing || waiting).then(|| if growing { tick } else { now + std::time::Duration::from_millis(100) });
        }
        if tv.starting.is_some() {
            tv.poll_starting(now);
            if tv.starting.is_some() {
                return Some(tick);
            }
        }
        if let Some(live) = &mut tv.live {
            // A new screen, or the end of the run: one frame.
            if now.duration_since(live.started) >= pocket_ui::showcase::live() {
                self.request_redraw();
            } else if live.reader.fresh() {
                self.request_tv_redraw();
            }
            return Some(tick);
        }
        if !pocket_ui::showcase::enabled() {
            return None;
        }
        let (want, since) = tv.rest.clone()?;
        if tv.played.as_ref().is_some_and(|p| same_game(p, &want)) || tv.plays.as_ref().is_some_and(|(w, p)| same_game(w, &want) && !p) {
            return None;
        }
        let due = since + pocket_ui::showcase::DWELL;
        if now >= due {
            if !tv.dwell_asked {
                tv.dwell_asked = true;
                self.request_redraw();
            }
            return None;
        }
        Some(due)
    }
}

/// How long A waits for the game's own picture before the TV's picture grows on its own (the run ended meanwhile).
const HAND_WAIT: std::time::Duration = std::time::Duration::from_millis(400);
/// How long pocket-core holds the game still at most: the grow tells it sooner (`startup::skip_hold`).
pub(crate) const HAND_HOLD: std::time::Duration = std::time::Duration::from_millis(1500);

impl Grow {
    /// Takes the game's picture once it is there; true while frames are wanted (waiting for it, or growing).
    fn poll(&mut self, now: Instant) -> bool {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(game) => {
                    self.game = game;
                    self.rx = None;
                    self.began = Some(now);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.began = Some(now);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) if now.duration_since(self.asked) >= HAND_WAIT => {
                    self.rx = None;
                    self.began = Some(now);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        // Grown: pocket-core may show the game now, whether or not a frame was painted at the very end (13.11.2026 on
        // the H700 a grow whose last frame fell before its end left pocket-core held to its 1.5 s limit).
        let grown = self.began.is_some_and(|b| now.duration_since(b) >= self.long);
        if grown && !self.told {
            self.told = true;
            pocket_ui::startup::skip_hold();
            pocket_ui::trace::step("showcase grown");
        }
        !self.over(now)
    }

    /// The grow has run its course (grown, and the last frames painted): the game is on screen, and whatever pocket-ui
    /// paints now (the pause menu on HOME) is not the kept shelf with the grow over it. False until it has begun.
    pub(crate) fn over(&self, now: Instant) -> bool {
        self.began.is_some_and(|b| now.duration_since(b) >= self.long + std::time::Duration::from_millis(40))
    }

    /// How far it has grown, 0 (the glass) to 1 (where the game draws), eased.
    pub(crate) fn at(&self, now: Instant) -> f32 {
        let Some(b) = self.began else { return 0.0 };
        if self.long.is_zero() {
            return 1.0;
        }
        let x = (now.duration_since(b).as_secs_f32() / self.long.as_secs_f32()).min(1.0);
        x * x * (3.0 - 2.0 * x)
    }

    /// The picture's rectangle at `g` (`at`): from the glass to the game's place (the glass, without one).
    pub(crate) fn rect(&self, g: f32) -> (i32, i32, i32, i32) {
        let to = self.game.as_ref().map_or(self.from, |(_, p)| (p.x, p.y, p.img.w as i32, p.img.h as i32));
        let l = |a: i32, b: i32| a + ((b - a) as f32 * g).round() as i32;
        (l(self.from.0, to.0), l(self.from.1, to.1), l(self.from.2, to.2).max(1), l(self.from.3, to.3).max(1))
    }
}

/// `px` (`w` x `h`, 0RGB) at the rectangle `r` on `c`, nearest neighbour, `alpha` 0-255.
fn blit_scaled(c: &mut Canvas, px: &[u32], w: u32, h: u32, r: (i32, i32, i32, i32), alpha: u32) {
    if alpha == 0 || w == 0 || h == 0 {
        return;
    }
    let (x0, x1) = (r.0.max(0), (r.0 + r.2).min(c.w));
    if c.n() != 1 {
        // Painted natively (docs/dev/native-paint.md): the four design pixels of each screen pixel as below, averaged.
        let (y0, y1) = (r.1.max(0), (r.1 + r.3).min(c.h));
        return art::cells_over(c, x0, x1, y0, y1, |x, y, bg| {
            let sy = ((y - r.1) as u64 * h as u64 / r.3 as u64).min(h as u64 - 1) as usize;
            let sx = ((x - r.0) as u64 * w as u64 / r.2 as u64).min(w as u64 - 1) as usize;
            let s = px[sy * w as usize + sx];
            if alpha >= 255 { s } else { pocket_ui::gfx::mix(bg, s, alpha) }
        });
    }
    for y in r.1.max(0)..(r.1 + r.3).min(c.h) {
        let sy = ((y - r.1) as u64 * h as u64 / r.3 as u64).min(h as u64 - 1) as usize;
        let src = &px[sy * w as usize..(sy + 1) * w as usize];
        let row = (y * c.w) as usize;
        for x in x0..x1 {
            let sx = ((x - r.0) as u64 * w as u64 / r.2 as u64).min(w as u64 - 1) as usize;
            let d = &mut c.pixels_mut()[row + x as usize];
            *d = if alpha >= 255 { src[sx] | 0xff00_0000 } else { pocket_ui::gfx::mix(*d, src[sx], alpha) };
        }
    }
}

impl App {
    /// A on the Cabinet's playing TV, for `path` about to start on pocket-core with `core`: the run hands over (its
    /// state and its picture, `showcase::headless`) and the picture starts to grow out of the TV. False when the game
    /// can't go on from the TV (nothing playing yet, another core, a save of its own): it starts the usual way.
    pub(crate) fn showcase_hand_over(&mut self, path: &Path, core: &str) -> bool {
        let (data_dir, view) = (self.data_dir.clone(), self.screen_size());
        // A game still in the shop's film starts with the film tearing off, whatever the TV played (`tear_wrap`).
        let sealed = self.sealed(path);
        let tv = &mut self.cabinet.tv;
        let (Some(live), Some((x, y)), Some((_, body))) = (tv.live.as_ref(), tv.at, tv.body.as_ref()) else { return false };
        let Some(pic) = live.pic.clone() else { return false };
        if live.want.rom != path {
            return false;
        }
        if let Some(why) = pocket_ui::showcase::usual_way(live.may, pocket_ui::store::core_path(core) == live.core, sealed) {
            println!("showcase: A starts {} the usual way ({why})", path.display());
            return false;
        }
        let (gx, gy, gw, gh) = body.glass;
        let from = (x + gx, y + gy, gw, gh);
        let Some(Live { want, child, file, .. }) = tv.live.take() else { return false };
        // The run writes its state and picture and ends (`showcase::headless`); not TERM, which would end it bare.
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGUSR1) };
        pocket_ui::showcase::clear(&[&file]);
        let (state, frame) = (pocket_ui::showcase::hand_state(&file), pocket_ui::showcase::hand_frame(&file));
        tv.reap.push((child, Instant::now()));
        // Back on the shelf the game shows the player's own last moment.
        tv.played = Some(want.clone());
        // The game's picture where pocket-core will draw it (`launch_picture::placement`), off the frame.
        let (tx, rx) = std::sync::mpsc::channel();
        let (system, rom) = (want.system.clone(), path.to_path_buf());
        std::thread::spawn(move || {
            let t = Instant::now();
            while !frame.exists() && t.elapsed() < HAND_WAIT {
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            let img = pocket_ui::showcase::read_hand_frame(&frame);
            pocket_ui::showcase::clear(&[&frame]);
            let game = img.map(|img| {
                let v = pocket_ui::video::VideoSettings::load(&data_dir, &system, &rom);
                let turns = v.orientation.turns() & 3;
                let src = pocket_ui::launch_picture::Source::Resume(PathBuf::new());
                let r = pocket_ui::launch_picture::placement(&src, &system, img.w, img.h, Some((v.scaling, turns)), view.0, view.1);
                let img = if turns != 0 {
                    let (w, h, px) = pocket_ui::video::turn_pixels(img.w, img.h, &img.px, turns);
                    pocket_ui::image::Image { w, h, px }
                } else {
                    img
                };
                let scaled = pocket_ui::launch_picture::scaled(&img, r.w, r.h);
                (img, pocket_ui::launch_picture::Prepared { rom: rom.clone(), x: r.x as i32, y: r.y as i32, img: scaled })
            });
            let _ = tx.send(game);
        });
        let long = if pocket_ui::motion::allowed(pocket_ui::motion::Kind::Fade) { pocket_ui::showcase::GROW } else { std::time::Duration::ZERO };
        let now = pocket_ui::frame_clock::now();
        tv.grow = Some(Grow { rom: path.to_path_buf(), state, from, tv: pic, rx: Some(rx), game: None, asked: now, began: None, long, told: false, kept: None });
        println!("showcase: A goes on with {} from the TV", path.display());
        true
    }

    /// The grow over the shelf while the game starts (in place of the launch fade): the shelf goes dark, the picture
    /// moves from the glass to the game's place, the TV's look giving way to the game's pixels. Once grown, pocket-core
    /// may show the game (`startup::skip_hold`), which covers this same picture. False: no grow for this start.
    pub(crate) fn paint_showcase_grow(&mut self, c: &mut Canvas) -> bool {
        let now = pocket_ui::frame_clock::now();
        let running = self.running.as_ref().map(|r| r.path.clone()).or_else(|| self.pending_launch.as_ref().map(|p| p.run.path.clone()));
        if !self.cabinet.tv.grow.as_ref().is_some_and(|g| running.as_ref() == Some(&g.rom) && !g.over(now)) {
            return false;
        }
        // The screen's pixels as they are (a quarter of the design pixels when painted natively), put back as they are.
        if let Some(g) = self.cabinet.tv.grow.as_mut().filter(|g| g.kept.is_none()) {
            g.kept = Some((c.w, c.h, c.pixels().to_vec()));
        }
        self.paint_grow_over(c, now);
        true
    }

    /// A grow frame over the kept shelf (`paint`'s first step); false when there is none to start from yet.
    pub(crate) fn paint_grow_on_kept(&mut self, c: &mut Canvas) -> bool {
        let running = self.running.as_ref().map(|r| r.path.clone()).or_else(|| self.pending_launch.as_ref().map(|p| p.run.path.clone()));
        let now = pocket_ui::frame_clock::now();
        // Not once it is over: the game runs, and the pause menu (HOME) must be painted, not the shelf kept for the grow.
        let Some(g) = self.cabinet.tv.grow.as_ref().filter(|g| running.as_ref() == Some(&g.rom) && !g.over(now)) else { return false };
        let Some((_, _, px)) = g.kept.as_ref().filter(|(w, h, px)| (*w, *h) == (c.w, c.h) && px.len() == c.pixels().len()) else { return false };
        c.pixels_mut().copy_from_slice(px);
        self.paint_grow_over(c, now);
        self.paint_loading(c);
        true
    }

    /// The grow at `now` over what `c` shows (`paint_showcase_grow`; the bench paints it over the shelf itself).
    pub(crate) fn paint_grow_over(&mut self, c: &mut Canvas, now: Instant) {
        let Some(g) = self.cabinet.tv.grow.as_mut() else { return };
        g.poll(now);
        let k = g.at(now);
        c.fade((k * 255.0) as u8);
        let r = g.rect(k);
        if let Some((img, _)) = &g.game {
            blit_scaled(c, &img.px, img.w, img.h, r, 255);
        }
        // The TV's look (its glass, scanlines) gives way in the first part of the grow.
        let tv_alpha = if g.game.is_none() { 255 } else { ((1.0 - k / 0.45).max(0.0) * 255.0) as u32 };
        blit_scaled(c, &g.tv.px, g.tv.w as u32, g.tv.h as u32, r, tv_alpha);
    }
}

impl App {
    /// The hand-over part of `bench_showcase` (POCKETUI_SHOWCASE_HAND).
    fn bench_hand(&mut self, w: u32, h: u32, scaler: &mut pocket_ui::scaled_layout::ScaledFrame, buf: &mut [u32], output: &mut [u32], dir: Option<&Path>) {
        let panel = super::snapshot_panel_dims();
        let Some((rom, system, core)) = self.cabinet.tv.live.as_ref().map(|l| {
            let name = l.core.file_stem().and_then(|s| s.to_str()).unwrap_or_default().trim_end_matches("_libretro").to_string();
            (l.want.rom.clone(), l.want.system.clone(), name)
        }) else {
            println!("bench-showcase-hand none: the run had ended");
            return;
        };
        let asked = Instant::now();
        if !self.showcase_hand_over(&rom, &core) {
            println!("bench-showcase-hand refused: {} on {core}", rom.display());
            return;
        }
        let cpu = || pocket_ui::run_ahead::thread_cpu_ms();
        let (mut times, mut shots, mut waited_ms) = (Vec::new(), 0, None);
        while asked.elapsed() < std::time::Duration::from_secs(3) {
            let now = Instant::now();
            let t = cpu();
            // As on the device: the shelf painted once, then each frame from the one kept (`paint_grow_on_kept`).
            let painted = |app: &mut App, c: &mut Canvas| {
                app.paint_in_appearance(c, &mut |app, c| {
                    let kept = app.cabinet.tv.grow.as_ref().and_then(|g| g.kept.as_ref()).filter(|(w, h, p)| (*w, *h) == (c.w, c.h) && p.len() == c.pixels().len()).map(|(_, _, p)| p.clone());
                    match kept {
                        Some(p) => c.pixels_mut().copy_from_slice(&p),
                        None => {
                            app.paint(c);
                            if let Some(g) = app.cabinet.tv.grow.as_mut() {
                                g.kept = Some((c.w, c.h, c.pixels().to_vec()));
                            }
                        }
                    }
                    app.paint_grow_over(c, now);
                });
            };
            scaler.render(output, panel, &mut |c| painted(self, c));
            times.push(cpu() - t);
            let Some((began, k, told)) = self.cabinet.tv.grow.as_ref().map(|g| (g.began.is_some(), g.at(now), g.told)) else { break };
            if waited_ms.is_none() && began {
                waited_ms = Some(asked.elapsed().as_secs_f64() * 1000.0);
            }
            if let Some(d) = dir.filter(|_| k >= shots as f32 * 0.25 && began) {
                painted(self, &mut Canvas::new(buf, w, h));
                super::write_png(buf, w, h, &d.join(format!("grow-{system}-{shots}.png")).to_string_lossy());
                shots += 1;
            }
            if told {
                break;
            }
        }
        let grow_ms = asked.elapsed().as_secs_f64() * 1000.0;
        let state = self.cabinet.tv.grow.as_ref().map(|g| g.state.clone()).unwrap_or_default();
        times.sort_by(f64::total_cmp);
        let at = |q: f64| times.get(((times.len() as f64 - 1.0).max(0.0) * q).round() as usize).copied().unwrap_or(0.0);
        println!(
            "bench-showcase-hand system={system} picture_after_ms={:.0} grown_after_ms={grow_ms:.0} frames={} paint_p50_ms={:.1} paint_p95_ms={:.1} paint_max_ms={:.1} state_bytes={}",
            waited_ms.unwrap_or(-1.0),
            times.len(),
            at(0.5),
            at(0.95),
            at(1.0),
            std::fs::metadata(&state).map_or(0, |m| m.len())
        );
        // pocket-core goes on from the state, as `launch_prepared` starts it (no hold: the picture has grown).
        let Some(d) = dir else { return };
        let log = std::fs::File::create(d.join(format!("pocket-core-{system}.log")));
        let mut cmd = std::process::Command::new(super::host_bin());
        cmd.arg(pocket_ui::store::core_path(&core)).arg(&rom).arg("--system").arg(&system);
        cmd.env(pocket_ui::showcase::STATE_ENV, &state).env(pocket_ui::showcase::FADE_IN_ENV, "500");
        cmd.env(pocket_ui::startup::LAUNCHED_AT, format!("{:.1}", pocket_ui::startup::clock_ms()));
        if let Ok(l) = log {
            cmd.stdout(l);
        }
        let started = Instant::now();
        match cmd.spawn() {
            Ok(mut child) => {
                std::thread::sleep(std::time::Duration::from_secs(4));
                unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
                let _ = child.wait();
                println!("bench-showcase-hand pocket-core ran {:.1} s (its log: pocket-core-{system}.log)", started.elapsed().as_secs_f64());
            }
            Err(e) => println!("bench-showcase-hand pocket-core: {e}"),
        }
    }
}

/// CPU time a process has used (user + system), from /proc/<pid>/stat, in ms.
fn proc_cpu_ms(pid: u32) -> Option<f64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    let ticks = f.get(11)?.parse::<f64>().ok()? + f.get(12)?.parse::<f64>().ok()?;
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    Some(ticks * 1000.0 / hz.max(1.0))
}

impl App {
    /// `--snapshot bench-showcase:<system>:<row>[:<seconds>[:<dir>]]`: the Cabinet with that system's game at `row`
    /// selected, the TV playing it as on the device (a real `pocket-core --showcase`), painted whenever a new screen is
    /// there, at the panel's size (the scaled layout's downscale included). Prints the paint time per frame (thread CPU),
    /// both processes' CPU and the run's own line; with `<dir>`, a screen every 2 s as PNG there (a contact sheet).
    pub(crate) fn bench_showcase(&mut self, rest: &str, w: u32, h: u32) {
        let mut parts = rest.split(':');
        let system = parts.next().unwrap_or("gb").to_string();
        let row = parts.next().unwrap_or("0").to_string();
        let seconds: f64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(20.0);
        let dir = parts.next().filter(|d| !d.is_empty()).map(PathBuf::from);
        self.shelf_snapshot(&format!("{system}:cabinet:games:{row}"));
        self.cabinet.inline = false;
        self.pictures.sync = true;
        pocket_ui::status_corner::start(|| {});
        self.text.prewarm();
        self.text.finish_prewarm();
        let panel = super::snapshot_panel_dims();
        let mut buf = vec![0u32; (w * h) as usize];
        let mut output = vec![0u32; (panel.0 * panel.1) as usize];
        let mut scaler = pocket_ui::scaled_layout::ScaledFrame::default();
        scaler.retain = true;
        let full_only = std::env::var_os("POCKETUI_SHOWCASE_FULL").is_some();
        let mut check = pocket_ui::scaled_layout::ScaledFrame::default();
        let mut check_out = vec![0u32; (panel.0 * panel.1) as usize];
        let (mut partials, mut checked, mut differing, mut worst_diff) = (0u32, 0u32, 0usize, 0u32);
        let cpu = || pocket_ui::run_ahead::thread_cpu_ms() * 1000.0;
        // A frame of the TV alone over the last whole one, as the device paints when only the TV asked; every 30th is
        // checked against a whole frame painted afresh (the pixels that differ).
        let mut partial = |app: &mut App, scaler: &mut pocket_ui::scaled_layout::ScaledFrame, output: &mut Vec<u32>, verify: bool| -> Option<f64> {
            let t = cpu();
            if !scaler.render_partial(output, panel, &mut |p, lw, lh| app.paint_tv_partial(&mut Canvas::new(p, lw, lh))) {
                return None;
            }
            let took = cpu() - t;
            if verify {
                check.render(&mut check_out, panel, &mut |c| {
                    app.paint_in_appearance(c, &mut |app, c| {
                        app.paint(c);
                        app.motion_after_paint(c);
                        app.sys.paint(c, &mut app.text, app.menu.is_none());
                        app.paint_leave(c);
                    })
                });
                let mut n = 0;
                for (a, b) in output.iter().zip(&check_out) {
                    let d = [16u32, 8, 0].iter().map(|s| ((a >> s & 0xff) as i32 - (b >> s & 0xff) as i32).unsigned_abs()).max().unwrap_or(0);
                    if d > 2 {
                        n += 1;
                        worst_diff = worst_diff.max(d);
                    }
                }
                checked += 1;
                differing += n;
            }
            partials += 1;
            Some(took)
        };
        let frame = |app: &mut App, scaler: &mut pocket_ui::scaled_layout::ScaledFrame, buf: &mut Vec<u32>, output: &mut Vec<u32>| -> f64 {
            let t = cpu();
            let paint = |app: &mut App, c: &mut Canvas| {
                app.motion_frame_begin(Instant::now());
                app.paint_in_appearance(c, &mut |app, c| {
                    app.paint(c);
                    app.motion_after_paint(c);
                    app.sys.paint(c, &mut app.text, app.menu.is_none());
                    app.paint_leave(c);
                });
            };
            scaler.render(output, panel, &mut |c| paint(app, c));
            // The logical picture too, for the PNGs (not timed).
            let took = cpu() - t;
            if dir.is_some() {
                paint(app, &mut Canvas::new(buf, w, h));
            }
            took
        };
        // The rest on the game: the still picture comes from its thread, the TV turns on after DWELL.
        let begun = Instant::now();
        while self.cabinet.tv.live.as_ref().is_none_or(|l| l.pic.is_none()) && begun.elapsed() < std::time::Duration::from_secs(12) {
            self.cabinet.tv.poll();
            frame(self, &mut scaler, &mut buf, &mut output);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let Some(pid) = self.cabinet.tv.live.as_ref().map(|l| l.child.id()) else {
            let item = self.items.get(self.sel).map(|i| format!("{i:?}")).unwrap_or_default();
            println!("bench-showcase none: the TV did not start ({item}; plays={:?})", self.cabinet.tv.plays.as_ref().map(|(w, p)| (w.rom.display().to_string(), *p)));
            return;
        };
        let to_live = begun.elapsed();
        let rom = self.cabinet.tv.live.as_ref().map(|l| l.want.rom.display().to_string()).unwrap_or_default();
        let (ui0, core0) = (proc_cpu_ms(std::process::id()).unwrap_or(0.0), proc_cpu_ms(pid).unwrap_or(0.0));
        let t0 = Instant::now();
        let mut us = Vec::new();
        let mut next_shot = 0.0f64;
        let mut shots = 0;
        while t0.elapsed().as_secs_f64() < seconds && self.cabinet.tv.live.is_some() {
            let fresh = self.cabinet.tv.live.as_mut().is_some_and(|l| l.reader.fresh());
            if !fresh {
                std::thread::sleep(std::time::Duration::from_millis(2));
                continue;
            }
            let verify = us.len() % 30 == 29;
            let took = if full_only { None } else { partial(self, &mut scaler, &mut output, verify) };
            us.push(match took {
                Some(t) => t,
                None => frame(self, &mut scaler, &mut buf, &mut output),
            });
            if dir.is_some() && t0.elapsed().as_secs_f64() >= next_shot && took.is_some() {
                // The PNG shows the logical frame: paint it whole (not timed).
                frame(self, &mut scaler, &mut buf, &mut output);
            }
            if let Some(d) = &dir {
                if t0.elapsed().as_secs_f64() >= next_shot {
                    if let Err(e) = std::fs::create_dir_all(d) {
                        println!("bench-showcase: {}: {e}", d.display());
                    }
                    super::write_png(&buf, w, h, &d.join(format!("showcase-{system}-{:02}.png", shots)).to_string_lossy());
                    shots += 1;
                    next_shot += 2.0;
                }
            }
        }
        let secs = t0.elapsed().as_secs_f64();
        let (ui1, core1) = (proc_cpu_ms(std::process::id()).unwrap_or(0.0), proc_cpu_ms(pid).unwrap_or(0.0));
        let frames = self.cabinet.tv.live.as_ref().map_or(0, |l| l.frames);
        // POCKETUI_SHOWCASE_HAND=1: then A, as on the device: the run hands over, the picture grows (each frame painted
        // whole and timed, PNGs at its steps into `<dir>`), and pocket-core goes on from the state (its log in `<dir>`).
        if std::env::var_os("POCKETUI_SHOWCASE_HAND").is_some() {
            self.bench_hand(w, h, &mut scaler, &mut buf, &mut output, dir.as_deref());
            return;
        }
        self.cabinet.tv.showcase_stop();
        let end = Instant::now();
        while !self.cabinet.tv.reap.is_empty() && end.elapsed() < std::time::Duration::from_secs(3) {
            self.cabinet.tv.reap(Instant::now());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        us.sort_by(f64::total_cmp);
        let at = |q: f64| us.get(((us.len() as f64 - 1.0).max(0.0) * q).round() as usize).copied().unwrap_or(0.0);
        let core_line = std::fs::read_to_string(std::env::temp_dir().join("pocket-showcase.log"))
            .ok()
            .and_then(|l| l.lines().rev().find(|l| l.starts_with("showcase-core")).map(str::to_string))
            .unwrap_or_default();
        println!(
            "bench-showcase system={system} rom={rom} panel={}x{} logical={w}x{h} to_live_ms={:.0} secs={secs:.1} painted={} painted_per_s={:.1} tv_frames={frames} paint_p50_us={:.0} paint_p95_us={:.0} paint_max_us={:.0} ui_cpu_pct={:.1} core_cpu_pct={:.1}",
            panel.0,
            panel.1,
            to_live.as_secs_f64() * 1000.0,
            us.len(),
            us.len() as f64 / secs.max(0.001),
            at(0.5),
            at(0.95),
            at(1.0),
            (ui1 - ui0) / (secs * 10.0),
            (core1 - core0) / (secs * 10.0),
        );
        println!("bench-showcase-partial mode={} partial_frames={partials} checked={checked} differing_px_total={differing} worst_channel_diff={worst_diff}", if full_only { "full" } else { "partial" });
        println!("{core_line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_snap(dir: &Path, want: &Want, rgb: [u8; 3]) {
        let file = pocket_ui::game_pictures::path(dir, pocket_ui::game_pictures::Kind::Snap, &want.system, &want.rom);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let mut encoder = png::Encoder::new(std::fs::File::create(file).unwrap(), 4, 4);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&rgb.repeat(16)).unwrap();
    }

    /// Settings → Look → Cozy extras → TV plays the game, off at the start: a game resting on the glass long enough
    /// to play starts no player while it is off (red before the switch: the showcase was on for everyone), and does
    /// with it on.
    /// The grow is over once grown: from then on the shelf kept for it must not be painted over the game's pause menu
    /// (crash sweep, 14.11.2026: after A on the playing TV, HOME paused the game but the kept shelf and grow stayed
    /// on screen for the whole game; red before: nothing ended the grow while `launch_at` was set).
    #[test]
    fn a_grow_is_over_once_it_has_grown() {
        let t0 = Instant::now();
        let long = std::time::Duration::from_millis(360);
        let mut g = Grow {
            rom: PathBuf::from("Game.gb"),
            state: PathBuf::new(),
            from: (0, 0, 10, 10),
            tv: Rc::new(Pic::new(2, 2, 0)),
            rx: None,
            game: None,
            asked: t0,
            began: None,
            long,
            told: true,
            kept: None,
        };
        assert!(!g.over(t0 + std::time::Duration::from_secs(60)), "not begun (waiting for the picture): not over");
        g.began = Some(t0);
        assert!(!g.over(t0 + long), "the last frames are still to come");
        assert!(g.over(t0 + long + std::time::Duration::from_millis(40)), "over");
        assert!(g.over(t0 + std::time::Duration::from_secs(60)), "over for the whole game");
        assert!(!g.poll(t0 + std::time::Duration::from_secs(60)), "no more frames wanted");
    }

    #[test]
    fn the_tv_starts_no_player_unless_tv_plays_the_game_is_on() {
        let dir = tempfile::tempdir().unwrap();
        let cores = dir.path().join("cores");
        std::fs::create_dir_all(&cores).unwrap();
        std::fs::write(cores.join("gambatte_libretro.so"), b"x").unwrap();
        std::env::set_var("POCKETUI_CORES", &cores);
        let want = Want { system: "gb".into(), rom: dir.path().join("Never Played.gb"), cover: None, w: 90, h: 70, lcd: false };
        assert!(pocket_ui::showcase::core_for("gb", &want.rom).is_some(), "the test has a core to start");
        let rested = Instant::now();
        let later = rested + pocket_ui::showcase::DWELL + std::time::Duration::from_millis(100);
        // A game that was never played, rested on past the rest, able to play.
        let fresh = || {
            let mut tv = TvState::default();
            tv.rest = Some((want.clone(), rested));
            tv.plays = Some((want.clone(), true));
            tv
        };
        let mut off = fresh();
        showcase_step_with(&mut off, &want, dir.path(), later, false);
        assert!(off.starting.is_none() && off.live.is_none() && off.orphans.is_empty(), "off: no player was started");
        assert!(pocket_ui::cozy_extras::Extras::default().get(pocket_ui::cozy_extras::Extra::TvPlaysGame) == false, "off at the start");
        let mut on = fresh();
        showcase_step_with(&mut on, &want, dir.path(), later, true);
        assert!(on.starting.is_some(), "on: the player is being started");
        on.showcase_stop();
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_replacing_a_picture_at_the_same_path_refreshes_the_selected_tv() {
        let dir = tempfile::tempdir().unwrap();
        let want = Want { system: "snes".into(), rom: PathBuf::from("Game.sfc"), cover: None, w: 90, h: 70, lcd: false };
        write_snap(dir.path(), &want, [230, 10, 10]);
        let mut tv = TvState::default();
        tv.select(want.clone(), dir.path(), true, Instant::now());
        let old = tv.shown.as_ref().unwrap().1.clone().unwrap();
        let center = 35 * 90 + 45;
        assert!((old.px[center] >> 16 & 0xff) > (old.px[center] & 0xff) * 3);

        // The initial inline selection can still have its original worker reply queued. Its generation must not
        // refill the decoded cache; replacing the file wins without changing Want or starting a second worker.
        write_snap(dir.path(), &want, [10, 10, 230]);
        tv.pictures_changed();
        assert!(Rc::ptr_eq(&old, tv.shown.as_ref().unwrap().1.as_ref().unwrap()), "keep the old screen while loading");
        let started = Instant::now();
        while tv.refresh && started.elapsed() < crate::test_clock::timeout(std::time::Duration::from_secs(3)) {
            tv.select(want.clone(), dir.path(), false, Instant::now());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!tv.refresh, "the real loader delivered the replacement");
        let new = tv.shown.as_ref().unwrap().1.as_ref().unwrap();
        assert!(!Rc::ptr_eq(&old, new));
        assert!((new.px[center] & 0xff) > (new.px[center] >> 16 & 0xff) * 3, "the same path now shows the new blue picture");
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_a_hidden_tv_finishes_waiting_and_has_its_picture_when_the_cabinet_returns() {
        let dir = tempfile::tempdir().unwrap();
        let want = Want { system: "snes".into(), rom: PathBuf::from("Game.sfc"), cover: None, w: 90, h: 70, lcd: false };
        write_snap(dir.path(), &want, [20, 210, 20]);
        let mut tv = TvState::default();
        tv.select(want.clone(), dir.path(), false, Instant::now());
        assert!(tv.busy(Instant::now()));
        assert!(tv.shown.is_none(), "the initial frame asked the real worker");

        // Settings, a card or another look is painted now. Only the common event-loop poll runs; there is no
        // paint_cabinet_tv/select call to consume the answer. Finishing must stop the redraw timer anyway.
        let started = Instant::now();
        let mut repaint = false;
        while tv.busy(Instant::now()) && started.elapsed() < crate::test_clock::timeout(std::time::Duration::from_secs(3)) {
            repaint |= tv.poll();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(repaint, "the completed picture requests one frame");
        assert!(!tv.busy(Instant::now()), "a hidden TV no longer keeps the UI animating");
        assert!(tv.shown.is_none(), "the hidden Cabinet has not painted");

        // Returning to the Cabinet uses the already completed result on its first frame, without another wait.
        tv.select(want, dir.path(), false, Instant::now());
        assert!(tv.shown.as_ref().unwrap().1.is_some());
        assert!(!tv.busy(Instant::now()));
    }

    #[test]
    fn box_art_found_while_resting_on_a_game_does_not_restart_its_showcase() {
        // 13.11.2026 on the H700: the cover arrived 18 s into the run, the game's Want changed with it, and the TV
        // started the game over after another rest.
        let dir = tempfile::tempdir().unwrap();
        let want = Want { system: "snes".into(), rom: PathBuf::from("Game.sfc"), cover: None, w: 90, h: 70, lcd: false };
        let mut tv = TvState::default();
        let t0 = Instant::now();
        tv.select(want.clone(), dir.path(), true, t0);
        tv.played = Some(want.clone());
        let with_cover = Want { cover: Some(dir.path().join("cover.png")), ..want.clone() };
        tv.select(with_cover.clone(), dir.path(), true, t0 + std::time::Duration::from_secs(1));
        assert_eq!(tv.rest.as_ref().unwrap().1, t0, "the same game keeps its rest");
        assert!(tv.played.as_ref().is_some_and(|p| same_game(p, &with_cover)), "and a run that ended stays ended");
        let later = t0 + std::time::Duration::from_secs(2);
        tv.select(Want { rom: PathBuf::from("Other.sfc"), ..want }, dir.path(), true, later);
        assert_eq!(tv.rest.as_ref().unwrap().1, later, "another game rests anew");
        assert!(tv.played.is_none());
    }

    fn grow(rx: Receiver<Option<(pocket_ui::image::Image, pocket_ui::launch_picture::Prepared)>>, asked: Instant, long: std::time::Duration) -> Grow {
        Grow { rom: PathBuf::from("Game.gb"), state: PathBuf::new(), from: (1000, 100, 120, 90), tv: Rc::new(Pic::new(120, 90, 0)), rx: Some(rx), game: None, asked, began: None, long, told: false, kept: None }
    }

    #[test]
    fn a_on_the_playing_tv_grows_its_picture_to_where_the_game_draws() {
        let t0 = Instant::now();
        let ms = std::time::Duration::from_millis;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut g = grow(rx, t0, pocket_ui::showcase::GROW);
        assert!(g.poll(t0));
        assert_eq!(g.rect(g.at(t0)), g.from, "until the game's picture is there the TV's stays on the glass");
        let img = pocket_ui::image::Image { w: 160, h: 144, px: vec![0x336699; 160 * 144] };
        let scaled = pocket_ui::launch_picture::scaled(&img, 1067, 960);
        tx.send(Some((img, pocket_ui::launch_picture::Prepared { rom: PathBuf::from("Game.gb"), x: 107, y: 0, img: scaled }))).unwrap();
        let t1 = t0 + ms(30);
        assert!(g.poll(t1));
        assert_eq!(g.rect(g.at(t1)), g.from, "it starts on the glass: no jump at A");
        let half = g.rect(g.at(t1 + pocket_ui::showcase::GROW / 2));
        assert!(half.2 > 120 && half.2 < 1067 && half.0 < 1000 && half.0 > 107, "half grown: {half:?}");
        assert_eq!(g.rect(g.at(t1 + pocket_ui::showcase::GROW)), (107, 0, 1067, 960), "it ends where pocket-core draws the game's frame");
        assert!(!g.told, "not grown yet: pocket-core still holds the game");
        assert!(!g.poll(t1 + pocket_ui::showcase::GROW + ms(50)), "grown: no more frames for it");
        assert!(g.told, "grown: pocket-core is told, painted or not");
    }

    #[test]
    fn without_the_games_picture_the_tv_picture_stays_on_the_glass() {
        // The run ended before it could hand over: the shelf goes dark around the TV's last picture, and the game
        // starts (pocket-core finds no state) without anything jumping.
        let t0 = Instant::now();
        let (_tx, rx) = std::sync::mpsc::channel();
        let mut g = grow(rx, t0, pocket_ui::showcase::GROW);
        assert!(g.poll(t0 + HAND_WAIT / 2) && g.began.is_none(), "it waits a moment for the picture");
        g.poll(t0 + HAND_WAIT);
        assert!(g.began.is_some(), "then it goes on without");
        assert_eq!(g.rect(g.at(t0 + HAND_WAIT + pocket_ui::showcase::GROW)), g.from);
        // Motion off: no growing, the game's place at once.
        let (_tx, rx) = std::sync::mpsc::channel();
        let mut still = grow(rx, t0, std::time::Duration::ZERO);
        drop(_tx);
        still.poll(t0);
        assert_eq!(still.at(t0), 1.0);
    }

    #[test]
    fn the_tv_fits_every_screen_or_is_left_out() {
        // The Mini V2, 1280 x 720, 640 x 480 (laid out at 1280 x 960, `scaled_layout`) with a roomy and with a tight
        // showcase.
        for (w, h, room) in [(1240, 1080, 420), (1280, 720, 260), (1280, 960, 300), (1280, 960, 60)] {
            match tv_scale(w, h, room) {
                Some(f) => {
                    let t = cabinet_tv::template();
                    let (tw, th) = (t.body.w as f32 * f, t.body.h as f32 * f);
                    assert!(th <= room as f32 && tw <= w as f32 / 4.0 + 0.5, "{w}x{h} room {room}: {tw}x{th}");
                    assert!(t.glass.3 as f32 * f >= 60.0);
                }
                None => assert!(room < 120, "{w}x{h} room {room}: a TV was expected"),
            }
        }
        assert!(tv_scale(1240, 1080, 420).is_some_and(|f| f.fract() == 0.0), "whole pixels on the Mini V2");
    }

    #[test]
    fn on_640x480_the_tv_stands_as_tall_as_the_case() {
        // Step 2 of the showcase on the H700: at 2 template pixels a pixel of the 1280 x 960 layout the TV was 71 screen
        // lines of 480, its game hard to make out; one step more and it is as tall as the case out of the shelf.
        let t = cabinet_tv::template();
        pocket_ui::scaled_layout::set_step((1280, 960), (1280, 960));
        assert_eq!(tv_scale(1280, 960, 300), Some(2.0), "a 1280 x 960 screen keeps its size");
        pocket_ui::scaled_layout::set_step((1280, 960), (640, 480));
        let f = tv_scale(1280, 960, 300).expect("a TV on 640 x 480");
        assert_eq!(f, 3.0);
        assert!(t.body.h as f32 * f <= 300.0 - 12.0 && t.body.w as f32 * f <= 320.0, "still in the showcase, a quarter of the width at most");
        // A tall case out (little room): it shrinks to fit, as before.
        assert!(tv_scale(1280, 960, 180).is_some_and(|f| t.body.h as f32 * f <= 180.0));
        pocket_ui::scaled_layout::set_step((1280, 960), (1280, 960));
    }
}
