//! Part of the pocket-ui binary: the idle slideshow's App side (`pocket_ui::slideshow` has the logic, docs/SLEEP.md
//! "Idle slideshow").
//!
//! * **When**: the shelf (no page, card, menu or game) has had no input for [`slideshow::start_after`] (a minute, or half
//!   the screen-off time when that is shorter) and Settings → System → Sleep & power → Slideshow before screen off is on (`cozy_extras::on(Extra::IdleSlideshow)`; off in every look until the player switches it on). The show only
//!   reads `Sleep::quiet_for`; it touches no sleep timer and holds no wake lock, so the screen goes off exactly when
//!   it did before, and the show is over with it (`slideshow_tick` ends it as soon as the device is not awake).
//! * **What**: the pictures are found on a thread (`slideshow::collect`), then each is read and scaled for the screen on a
//!   second one, three of them at most in memory (the one shown, the one fading out, the next). No game with a real
//!   picture, no show: the shelf stays.
//! * **Frames**: 15 a second while something moves or fades (`slideshow::FRAME`), none between slides when Motion is
//!   Off or Reduced (then the pictures only cross-fade). `animating` is false while it runs, so the shelf's own
//!   animations (a marquee, a pull) do not set the pace.
//! * **Any input ends it** and does nothing else (`slideshow_input`): a button press wakes the shelf as it was, a touch
//!   too, the rest of the touch (move, lift) is swallowed. The idle timers see the input as usual.

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::cozy_extras::{on as extra, Extra};
use pocket_ui::gfx::Canvas;
use pocket_ui::motion::{self, Kind as Motion};
use pocket_ui::pad::{Button, PadEvent};
use pocket_ui::palette::MARGIN;
use pocket_ui::slideshow::{self, Dirs, GameRef, Scene, Slide};
use pocket_ui::text::Face;

use crate::app_status_corner::TOP_LINE;
use crate::{App, Screen, UserEvent};

const TITLE_PX: u32 = 44;
const SYSTEM_PX: u32 = 26;

struct Shown {
    scene: Rc<Scene>,
    slide: Slide,
    /// Which slide of the show this is (counted up, for the movement).
    seq: usize,
    start: Instant,
}

enum Phase {
    Off,
    /// The pictures are being looked for on a thread.
    Collecting,
    /// Nothing to show; looked at again after the next input (`quiet` was the quiet time at the look).
    Empty { quiet: Duration },
    Show(Box<Show>),
}

struct Show {
    slides: Vec<Slide>,
    /// The slide to read next (index into `slides`), and how many in a row could not be read.
    next: usize,
    failed: usize,
    /// Asked for (its index), or here.
    asking: Option<usize>,
    upcoming: Option<(usize, Rc<Scene>)>,
    cur: Option<Shown>,
    prev: Option<Shown>,
    seq: usize,
    /// When a frame was last painted, to keep the pace.
    frame_at: Instant,
    started: Instant,
}

struct Job {
    generation: u64,
    index: usize,
    file: std::path::PathBuf,
    size: (i32, i32),
}

pub(crate) struct SlideshowUi {
    phase: Phase,
    generation: Arc<AtomicU64>,
    worker: Option<Sender<Job>>,
    /// A finger went down on the show: the rest of its touch is not for the shelf.
    touch_held: bool,
    /// `--snapshot` and the journey driver: pictures are read on the spot, the clock is the caller's.
    pub(crate) sync: bool,
    /// The pictures' folders (the screenshots folder and the states folder move with the test environment).
    dirs_override: Option<Dirs>,
}

/// `slideshow::start_after`, except in the journey tests (`POCKETUI_MOCK` with `POCKETUI_SLIDESHOW_AFTER_MS`), which
/// cannot wait a minute.
fn start_after(screen_off: Option<u32>) -> Option<Duration> {
    if std::env::var_os("POCKETUI_MOCK").is_some() {
        if let Some(ms) = std::env::var("POCKETUI_SLIDESHOW_AFTER_MS").ok().and_then(|v| v.parse::<u64>().ok()) {
            return Some(Duration::from_millis(ms));
        }
    }
    slideshow::start_after(screen_off)
}

impl SlideshowUi {
    pub fn new() -> Self {
        SlideshowUi { phase: Phase::Off, generation: Arc::new(AtomicU64::new(0)), worker: None, touch_held: false, sync: false, dirs_override: None }
    }

    fn reset(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.phase = Phase::Off;
    }

    /// The show is on screen.
    pub fn visible(&self) -> bool {
        matches!(&self.phase, Phase::Show(s) if s.cur.is_some())
    }
}

impl App {
    /// Whether the shelf is the only thing there is: no game, page, card, menu, dialog or notice over it.
    fn slideshow_shelf(&self) -> bool {
        self.running.is_none()
            && self.menu.is_none()
            && self.switcher.is_none()
            && self.setup.is_none()
            && self.settings_page.is_none()
            && self.card.is_none()
            && self.leave.is_none()
            && !self.tour.is_open()
            && !self.sys.is_open()
            && self.screen == Screen::Home
            && !self.find.is_open()
            && self.gallery_row().is_none()
            && self.curator.page.is_none()
            && self.this_game.is_none()
            && self.console.is_none()
            && self.port_files.is_none()
            && self.launch_at.is_none()
            && self.exit_since.is_none()
            && self.notice.is_none()
            && self.toast.is_none()
            && self.sleep.state() == pocket_ui::sleep::State::Awake
            && self.sleep.overlay().is_none()
            && !self.on_tv()
    }

    fn slideshow_games(&self) -> Vec<GameRef> {
        self.systems
            .iter()
            .flat_map(|s| s.games.iter().map(|g| GameRef { system: s.name.clone(), system_name: s.fullname.clone(), name: g.name.clone(), path: g.path.clone() }))
            .collect()
    }

    fn slideshow_dirs(&self) -> Dirs {
        self.slideshow.dirs_override.clone().unwrap_or_else(|| Dirs {
            data: self.data_dir.clone(),
            screenshots: pocket_ui::screenshot::dir(),
            // The tests and snapshots (`POCKETUI_MOCK`) point it at a folder of their own.
            states: std::env::var_os("POCKETUI_MOCK")
                .and_then(|_| std::env::var_os("POCKETUI_SLIDESHOW_STATES"))
                .map_or_else(|| std::path::PathBuf::from(pocket_ui::slots::STATES_DIR), std::path::PathBuf::from),
            scraped_pictures: self.pictures.wanted(),
        })
    }

    /// From `about_to_wait`: starts the show after the quiet time, keeps it going, ends it when the shelf is no longer
    /// alone or the screen is going off. Returns when to look again.
    pub(crate) fn slideshow_tick(&mut self, now: Instant) -> Option<Instant> {
        if !extra(Extra::IdleSlideshow) || !self.slideshow_shelf() {
            if !matches!(self.slideshow.phase, Phase::Off) {
                self.slideshow_end();
            }
            return None;
        }
        let quiet = self.sleep.quiet_for();
        let after = start_after(self.sleep.config.screen_off)?;
        match &mut self.slideshow.phase {
            Phase::Off => {
                if quiet < after {
                    return Some(now + (after - quiet));
                }
                self.slideshow_collect();
                None
            }
            Phase::Collecting => None,
            Phase::Empty { quiet: at } => {
                if quiet < *at {
                    // Someone touched the Pocket since: the next quiet spell looks again.
                    self.slideshow.phase = Phase::Off;
                    return Some(now + after.saturating_sub(quiet));
                }
                *at = quiet;
                None
            }
            Phase::Show(_) => {
                // A button pressed since the last look ends the show (`slideshow_input` did, for pad and touch);
                // this is the safety net for input that did not pass there (a key from the keyboard).
                if quiet < after.saturating_sub(Duration::from_secs(1)) {
                    self.slideshow_end();
                    return Some(now + (after - quiet));
                }
                // Screen off "Never": the show does not play for ever; the shelf comes back and stays until the next input.
                if matches!(&self.slideshow.phase, Phase::Show(s) if now.duration_since(s.started) >= slideshow::MAX_RUN) {
                    self.slideshow_end();
                    self.slideshow.phase = Phase::Empty { quiet };
                    return None;
                }
                self.slideshow_step(now)
            }
        }
    }

    fn slideshow_collect(&mut self) {
        let games = self.slideshow_games();
        let dirs = self.slideshow_dirs();
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        let generation = self.slideshow.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if self.slideshow.sync {
            let slides = slideshow::collect(&games, &dirs, seed);
            self.slideshow_slides(generation, slides);
            return;
        }
        self.slideshow.phase = Phase::Collecting;
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("slides".into()).spawn(move || {
            let slides = slideshow::collect(&games, &dirs, seed);
            let _ = proxy.send_event(UserEvent::Slides(generation, slides));
        });
    }

    /// The pictures were found (`UserEvent::Slides`).
    pub(crate) fn slideshow_slides(&mut self, generation: u64, slides: Vec<Slide>) {
        if generation != self.slideshow.generation.load(Ordering::SeqCst) || !matches!(self.slideshow.phase, Phase::Collecting | Phase::Off) {
            return;
        }
        if slides.is_empty() || !extra(Extra::IdleSlideshow) {
            println!("slideshow: no pictures, the shelf stays");
            self.slideshow.phase = Phase::Empty { quiet: self.sleep.quiet_for() };
            return;
        }
        println!("slideshow: {} pictures", slides.len());
        let show = Show { slides, next: 0, failed: 0, asking: None, upcoming: None, cur: None, prev: None, seq: 0, frame_at: Instant::now() - slideshow::FRAME, started: Instant::now() };
        self.slideshow.phase = Phase::Show(Box::new(show));
        self.slideshow_ask();
    }

    /// Asks for the next slide's picture when none is here or on its way.
    fn slideshow_ask(&mut self) {
        let size = self.screen_size();
        let size = (size.0 as i32, size.1 as i32);
        let generation = self.slideshow.generation.load(Ordering::SeqCst);
        let sync = self.slideshow.sync;
        let Phase::Show(show) = &mut self.slideshow.phase else { return };
        if show.asking.is_some() || show.upcoming.is_some() {
            return;
        }
        let index = show.next % show.slides.len();
        show.asking = Some(index);
        let file = show.slides[index].file.clone();
        if sync {
            let scene = slideshow::prepare(&file, size.0, size.1).map(Box::new);
            self.slideshow_scene(generation, index, scene);
            return;
        }
        let proxy = self.proxy.clone();
        let current = self.slideshow.generation.clone();
        let tx = self.slideshow.worker.get_or_insert_with(|| {
            let (tx, rx) = channel::<Job>();
            let _ = std::thread::Builder::new().name("slideshow".into()).spawn(move || {
                for job in rx {
                    // A job of a show that has ended is not worth reading.
                    if job.generation != current.load(Ordering::SeqCst) {
                        continue;
                    }
                    let scene = slideshow::prepare(&job.file, job.size.0, job.size.1).map(Box::new);
                    let _ = proxy.send_event(UserEvent::SlideScene(job.generation, job.index, scene));
                }
            });
            tx
        });
        let _ = tx.send(Job { generation, index, file, size });
    }

    /// A picture was read (`UserEvent::SlideScene`), or could not be.
    pub(crate) fn slideshow_scene(&mut self, generation: u64, index: usize, scene: Option<Box<Scene>>) {
        if generation != self.slideshow.generation.load(Ordering::SeqCst) {
            return;
        }
        let now = Instant::now();
        let Phase::Show(show) = &mut self.slideshow.phase else { return };
        if show.asking != Some(index) {
            return;
        }
        show.asking = None;
        show.next += 1;
        match scene {
            Some(scene) => {
                show.failed = 0;
                let scene = Rc::new(*scene);
                if show.cur.is_none() {
                    let seq = show.seq;
                    show.seq += 1;
                    show.cur = Some(Shown { scene, slide: show.slides[index].clone(), seq, start: now });
                    show.frame_at = now - slideshow::FRAME;
                } else {
                    show.upcoming = Some((index, scene));
                }
            }
            None => {
                show.failed += 1;
                if show.failed >= show.slides.len() {
                    println!("slideshow: no picture could be read, the shelf stays");
                    self.slideshow.phase = Phase::Empty { quiet: self.sleep.quiet_for() };
                    return;
                }
            }
        }
        self.slideshow_ask();
        self.request_redraw();
    }

    /// Moves the show on to `now`: the next slide when this one's time is up and its picture is here, the old one away
    /// once it has faded out. Asks for a frame when something moves. Returns when to look again.
    fn slideshow_step(&mut self, now: Instant) -> Option<Instant> {
        self.slideshow_ask();
        let moves = motion::allowed(Motion::Move);
        let Phase::Show(show) = &mut self.slideshow.phase else { return None };
        let Some(cur) = &show.cur else { return None };
        if now.duration_since(cur.start) >= slideshow::HOLD && show.slides.len() > 1 {
            if let Some((index, scene)) = show.upcoming.take() {
                let seq = show.seq;
                show.seq += 1;
                show.prev = show.cur.take();
                show.cur = Some(Shown { scene, slide: show.slides[index].clone(), seq, start: now });
            }
        }
        let cur = show.cur.as_ref()?;
        let local = now.duration_since(cur.start);
        if local >= slideshow::FADE {
            show.prev = None;
        }
        let fading = local < slideshow::FADE;
        // Moving: the picture still has its path to go. Still: a plain fade, then nothing until the next slide.
        let moving = moves && slideshow::progress(local) < 1.0;
        if fading || moving || show.frame_at < cur.start {
            let due = show.frame_at + slideshow::FRAME;
            if now >= due {
                self.request_redraw();
                return Some(now + slideshow::FRAME);
            }
            return Some(due);
        }
        // Between slides without movement: the next change, and the title's fade-out before it.
        let next = cur.start + slideshow::HOLD;
        if now < next {
            let title_edge = cur.start + Duration::from_secs_f32(slideshow::HOLD.as_secs_f32() - 1.0);
            // The title fades near the end of the slide: frames for that too.
            if now >= title_edge {
                self.request_redraw();
                return Some(now + slideshow::FRAME);
            }
            return Some(title_edge.min(next));
        }
        // Time is up but the next picture is not here yet (or there is only one): look again soon.
        (show.slides.len() > 1).then(|| now + Duration::from_millis(250))
    }

    /// Ends the show: the shelf is as it was (nothing of it was changed), and a frame draws it.
    pub(crate) fn slideshow_end(&mut self) {
        let was_visible = self.slideshow.visible();
        self.slideshow.reset();
        if was_visible {
            self.request_redraw();
        }
    }

    /// For `animating`: no frame at the shelf's pace while the show runs (it sets its own).
    pub(crate) fn slideshow_visible(&self) -> bool {
        self.slideshow.visible()
    }

    /// Input while the show is on screen. True when the event was the show's: a button press ends it and does nothing
    /// else, a touch ends it and its move and lift are swallowed. The idle timers have seen it already
    /// (`Sleep::on_pad` / `touched`), so the screen-off time starts again from this press.
    pub(crate) fn slideshow_input(&mut self, ev: &UserEvent) -> bool {
        match ev {
            UserEvent::Pad(PadEvent::Button { button, pressed, .. }) if self.slideshow.visible() => {
                // The volume keys belong to ROCKNIX and do not wake anything; a release changes nothing.
                if matches!(button, Button::VolUp | Button::VolDown) || !*pressed {
                    return !*pressed;
                }
                self.slideshow_end();
                self.held = None;
                true
            }
            UserEvent::Touch(t) => {
                use pocket_ui::touch::Phase;
                if self.slideshow.visible() && t.phase == Phase::Down {
                    self.sleep.touched();
                    self.slideshow.touch_held = true;
                    self.slideshow_end();
                    return true;
                }
                if self.slideshow.touch_held {
                    if t.phase == Phase::Up {
                        self.slideshow.touch_held = false;
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Paints the show over the canvas (over the shelf, which the caller painted first, while the first slide fades in);
    /// false when it is not on screen, then the shelf paints as always.
    pub(crate) fn paint_slideshow(&mut self, c: &mut Canvas, now: Instant) -> bool {
        let still = !motion::allowed(Motion::Move);
        let Phase::Show(show) = &self.slideshow.phase else { return false };
        let Some(cur) = &show.cur else { return false };
        let local = now.saturating_duration_since(cur.start);
        if let Some(prev) = &show.prev {
            let v = slideshow::view(prev.seq, slideshow::progress(now.saturating_duration_since(prev.start)), still);
            slideshow::paint(c, &prev.scene, v, 255);
        }
        let mix = (slideshow::fade_mix(local) * 255.0).round() as u8;
        let v = slideshow::view(cur.seq, slideshow::progress(local), still);
        let (scene, slide, title_a) = (cur.scene.clone(), cur.slide.clone(), slideshow::title_alpha(local));
        slideshow::paint(c, &scene, v, mix);
        self.slideshow_overlay(c, &slide, title_a, mix);
        if let Phase::Show(show) = &mut self.slideshow.phase {
            show.frame_at = now;
        }
        true
    }

    /// A frame of the show alone (the shelf is not painted under it): what the shelf's last frame left behind is not on
    /// screen any more, as `paint` makes sure for its own.
    pub(crate) fn slideshow_begin_frame(&mut self) {
        self.hits.clear();
        pocket_ui::text::boxes::reset();
        self.corner_minute = None;
    }

    /// Whether the shelf has to be painted under this frame: the first slide is still fading in.
    pub(crate) fn slideshow_needs_shelf(&self, now: Instant) -> bool {
        matches!(&self.slideshow.phase, Phase::Show(s) if s.cur.as_ref().is_some_and(|c| s.prev.is_none() && c.seq == 0 && now.saturating_duration_since(c.start) < slideshow::FADE))
    }

    /// `--snapshot slideshow[:<ms>|:fade[:<title>[:<source>[:<system>]]]]`: the show with the library's real pictures, `ms`
    /// milliseconds into a slide (default: well into it, the title there; `fade` is 800 ms, halfway in over the shelf).
    /// With a `title` (part of a game's name), a `source` (`cover`, `snap`, `title`, `screenshot`, `save`) and a `system`
    /// (part of its name: "Super") the slide is that game's picture of that kind. Pictures are read on the spot.
    pub(crate) fn slideshow_snapshot(&mut self, state: &str) -> bool {
        use pocket_ui::slideshow::Source;
        let Some(rest) = state.strip_prefix("slideshow") else { return false };
        let mut parts = rest.trim_start_matches(':').split(':');
        let ms = match parts.next().unwrap_or("") {
            "" => 6000,
            "fade" => 800,
            n => n.parse().unwrap_or(6000),
        };
        let wanted = parts.next().filter(|t| !t.is_empty()).map(|t| t.to_lowercase());
        let source = parts.next().and_then(|s| match s {
            "cover" => Some(Source::Cover),
            "snap" => Some(Source::Snap),
            "title" => Some(Source::Title),
            "screenshot" => Some(Source::Screenshot),
            "save" => Some(Source::Save),
            _ => None,
        });
        let system = parts.next().filter(|t| !t.is_empty()).map(|t| t.to_lowercase());
        self.slideshow.sync = true;
        self.pictures.sync = true;
        let (games, dirs) = (self.slideshow_games(), self.slideshow_dirs());
        // Another shuffle until the wanted game shows the wanted kind of picture first.
        let mut slides = Vec::new();
        for seed in 1..400u64 {
            slides = slideshow::collect(&games, &dirs, seed);
            let hit = slides.iter().position(|s| wanted.as_ref().is_none_or(|t| s.title.to_lowercase().contains(t))
                && source.is_none_or(|k| s.source == k)
                && system.as_ref().is_none_or(|t| s.system.to_lowercase().contains(t)));
            if let Some(i) = hit {
                slides.rotate_left(i);
                break;
            }
            if wanted.is_none() && source.is_none() && system.is_none() {
                break;
            }
        }
        let generation = self.slideshow.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.slideshow_slides(generation, slides);
        let now = Instant::now();
        if let Phase::Show(show) = &mut self.slideshow.phase {
            if let Some(cur) = &mut show.cur {
                cur.start = now - Duration::from_millis(ms);
            }
        } else {
            eprintln!("snapshot slideshow: no pictures in the data folder");
        }
        true
    }

    /// The title and system at the bottom, the clock and battery at the top right, each on a soft dark band.
    fn slideshow_overlay(&mut self, c: &mut Canvas, slide: &Slide, title_alpha: f32, mix: u8) {
        let (w, h) = (c.w, c.h);
        // Bands: dark at the edge, clear by the time they reach the picture.
        let band = |c: &mut Canvas, y0: i32, y1: i32, strongest: u32, top_down: bool| {
            let n = (y1 - y0).max(1);
            for row in 0..n {
                let t = if top_down { 1.0 - row as f32 / n as f32 } else { row as f32 / n as f32 };
                let amount = (strongest as f32 * t.powf(1.6) * (mix as f32 / 255.0)) as u8;
                c.darken_rect(0, y0 + row, w, 1, amount);
            }
        };
        band(c, h - h / 3, h, 225, false);
        band(c, 0, h / 8, 120, true);
        let corner = self.status_corner();
        corner.paint(c, &mut self.text, w - MARGIN, TOP_LINE, mix_colour(0x0c0d10, 0xffffff, mix));
        if title_alpha < 0.02 {
            return;
        }
        let ink = |full: u32| mix_colour(0x14161a, full, (title_alpha * 255.0) as u8);
        let room = w - 2 * MARGIN;
        let title = self.text.fit(Face::Title, TITLE_PX, &slide.title, room);
        let tw = self.text.width(Face::Title, TITLE_PX, &title);
        let baseline = h - h / 14 - 34;
        // A shadow too: a bright box art or a white title screen is behind it now and then.
        if title_alpha > 0.35 {
            self.text.draw(c, Face::Title, TITLE_PX, (w - tw) / 2 + 2, baseline + 2, 0, &title);
        }
        self.text.draw(c, Face::Title, TITLE_PX, (w - tw) / 2, baseline, ink(0xffffff), &title);
        // System, then for the player's own pictures what they are.
        let caption = slide.source.caption();
        let sys_w = self.text.width(Face::Body, SYSTEM_PX, &slide.system);
        let sep = "   ·   ";
        let sep_w = self.text.width(Face::Body, SYSTEM_PX, sep);
        let cap_w = caption.map_or(0, |t| self.text.width(Face::Body, SYSTEM_PX, t));
        let total = sys_w + caption.map_or(0, |_| sep_w + cap_w);
        let mut x = (w - total) / 2;
        let line = baseline + 12 + pocket_ui::type_scale::px(SYSTEM_PX) as i32;
        x += self.text.draw(c, Face::Body, SYSTEM_PX, x, line, ink(0xc9ced6), &slide.system);
        if let Some(t) = caption {
            x += self.text.draw(c, Face::Body, SYSTEM_PX, x, line, ink(0x8d95a1), sep);
            self.text.draw(c, Face::Body, SYSTEM_PX, x, line, ink(0xe8c36a), t);
        }
    }
}

/// `a` towards `b` by `t` of 255.
fn mix_colour(a: u32, b: u32, t: u8) -> u32 {
    let t = t as u32 + (t as u32 >> 7);
    let ch = |s: u32| (((a >> s & 0xff) * (256 - t) + (b >> s & 0xff) * t) >> 8) << s;
    ch(16) | ch(8) | ch(0)
}
