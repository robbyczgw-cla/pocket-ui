//! pocket-core: a small libretro host for software-rendered systems (Game Boy first) and, through
//! OpenGL, hardware-rendered 3D cores (N64, PSP, Dreamcast; see `hw_render`). An empty <rom> ("")
//! starts a core without a game (a console's BIOS menu).
//! Draws the core's frames with OpenGL ES at the largest whole-number scale, syncs to vsync, plays
//! audio with dynamic rate control, and reports frame-time statistics on exit.
//!
//! Usage: pocket-core <core.so> <rom> [--system NAME] [--bench SECONDS] [--rotate N] [--run-ahead N]
//! Quit: hold SELECT + START for one second, close the window, or SIGTERM.
//!
//! Saves, next to RetroArch's in /storage/roms/savestates/<system>/ (see `slots`):
//!   <game>.pstate(.png), .pstate1 … .pstate7   eight slots, written by the menu and HOME + R1
//!   <game>.pstate.auto(.png)                   written on every exit and loaded on start: instant resume
//!                                              (unless the battery save is newer, see `Core::resume`)
//!   <game>.pstate.earlier1 … 6, .started1 … 3  Earlier: one every 5 min of play, and where sessions began
//!                                              (`pocket_ui::timeline`; written on its own thread)
//! Commands, one per line, on /tmp/pocket-core.cmd: save [slot], load [slot], slot <n> (slots count from 1),
//! load earlier<n>|started<n> <time> (Earlier, see `load_earlier`),
//! reload (settings), set <key> <value>, disc next | disc <n> (multi-disc games: the pause menu's Disc row reads what
//! `pocket_ui::disc::FILE` says, see `publish_discs`), cheats (read the game's switched-on
//! cheats again, see `cheats`), quit, quit nosave (the pause menu's "Quit without saving": the battery save but no
//! new auto-resume state, see `App::exit_without_saving`), quit handoff (the game switcher: `quit`, and a line on
//! pocket-ui's pipe once the save is done, see `fast_switch`). `pocket-core --rumble-test` shakes the motor through the path a core's
//! rumble takes (see `rumble`).
//! When a game cannot start, the reason goes to /tmp/pocket-core.error for pocket-ui to show (`notice`).
//! While the game's saves (`.srm`, and `.rtc` for a cartridge clock) don't reach the card, a line shows over the game and
//! /tmp/pocket-core.save-trouble says why, for pocket-ui's pause menu (`pocket_ui::save_trouble`).
//! Hotkeys, fast-forward, rewind, Over the game (the FPS line, sensors, Pixel Path's line; `over_game`), CPU and fan
//! profiles: see `hotkeys`, `extras`, `power`.
//! Run-ahead (the game answers the buttons 1-2 frames sooner): `run_ahead`; `--run-ahead N` overrides the setting.
//! `note <sentence>` shows a line of pocket-ui's over the game for a few seconds (`overlay::Hud::note`).
//! `shot <seq>` writes the last frame, whole, for the pause menu's background (`pocket_ui::preview::SHOT_FILE`).
//! `pixelpath <seq>` writes the pause menu's Pixel Path report (`pocket_ui::pixel_path`, `App::write_pixel_path`); the
//! same report, built twice a second while Over the game is on Pixel Path, is the strip beside the picture
//! (`pocket_ui::pixel_live`).
//! `pocket-core --restore-system` undoes CPU/fan changes a crashed run left behind (and pocket-ui's fan outside games, and
//! a PortMaster port's performance, `port_perf`).
//! pause / resume (pause menu: the core stops, sound goes quiet, the last frame stays);
//! sleep / wake (pocket-ui's sleep, `src/sleep.rs`: like pause, but kept apart from it, so waking up
//! behind an open pause menu leaves the game paused; `wake` also lowers PipeWire's quantum again,
//! which ROCKNIX's wake hook resets); `autosave` (writes the auto-resume state and the battery save
//! now, before the system suspends or powers off); and
//! `preview <w> <h> [<seq>]` (renders the last frame with the current settings into
//! `pocket_ui::preview::FILE`, see `src/preview.rs`). `opt <core option key> <value>` and
//! `ctl <button|stick_dpad> <value>` come from the Emulator and Controls pages, which read
//! /storage/.config/pocket-ui/core-options/<system>.json; this process rewrites it whenever the
//! core's options, their visibility or the button layout change.
//!
//! Frame art on the desktop: `pocket-core --frame-demo <system> <game.png> <out.png> [scaling] [frame]
//! [colour|cabinet]` draws a built-in frame around a game picture (no effects; Ambient is GPU only), `--export-frame <system> <w> <h> <dir>`
//! writes it as a frame pack (a template for frame designers, see docs/FRAMES.md).

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{NotCurrentGlContext, PossiblyCurrentContext, PossiblyCurrentGlContext};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use pocket_ui::audio::Audio;
use pocket_ui::controls::{self, Mapping, StickPad};
use pocket_ui::core_options::{self, CoreOptionsFile};
use pocket_ui::extras::{Extras, FastForward, MAX_FF_BUDGET_MS, MAX_FF_RUNS};
use pocket_ui::hostsys;
use pocket_ui::hotkeys::Action;
use pocket_ui::hw_render::{self, HwContext};
use pocket_ui::libretro::{joypad, Core, Frame, Resume, AV_ALL, AV_VIDEO};
use pocket_ui::options::GameSettings;
use pocket_ui::overlay::{Hud, Overlay};
use pocket_ui::players::{self, Inputs};
use pocket_ui::power::Power;
use pocket_ui::slots;
use pocket_ui::video::{Renderer, VideoSettings};
use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowId};

/// pocket-ui's data directory: the pause menu writes the per-console/per-game settings here.
const DATA_DIR: &str = "/storage/.config/pocket-ui";

/// `POCKETUI_DATA` moves it (tests on the device use a scratch directory).
fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("POCKETUI_DATA").unwrap_or_else(|_| DATA_DIR.into()))
}

static TERMINATE: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigterm(_: libc::c_int) {
    TERMINATE.store(true, Ordering::SeqCst);
}

/// The screenshot thread and its answers (`App::screenshot`).
struct Shots {
    saver: pocket_ui::screenshot::Saver,
    done: std::sync::mpsc::Receiver<pocket_ui::screenshot::Outcome>,
}

struct Gfx {
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    video: Renderer,
    /// None when its shaders don't compile: the game still runs, without messages.
    overlay: Option<Overlay>,
    /// The core's own GL context and framebuffer, for hardware-rendered cores (`hw_render.rs`).
    hw: Option<HwContext>,
}

struct Stats {
    /// Emulated frames, which differ from the screen's while fast-forwarding or rewinding.
    emulated: u64,
    started: Instant,
    last_swap: Option<Instant>,
    intervals: Vec<f32>,
    run_ms: Vec<f32>,
    /// Bench runs only: time to draw a frame (picture, screen frame, HUD) until the GPU is done.
    draw_ms: Vec<f32>,
    /// Bench runs only: Pixel Path's strip, building its report and grading it (twice a second while it shows).
    strip_us: Vec<f32>,
}

impl Stats {
    fn report(&self, audio: Option<&Audio>) {
        let mut v = self.intervals.clone();
        if v.is_empty() {
            return;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt();
        let p99 = v[(v.len() * 99 / 100).min(v.len() - 1)];
        let late = v.iter().filter(|x| **x > 25.0).count();
        let run = self.run_ms.iter().sum::<f32>() / self.run_ms.len().max(1) as f32;
        let run_max = self.run_ms.iter().cloned().fold(0.0, f32::max);
        let underruns = audio.map_or(0, |a| *a.underruns.lock().unwrap());
        if let Some(a) = audio {
            println!("audio latency ({}): server {:.0} ms + queue {:.0} ms", a.backend, a.server_ms(), a.buffered_ms());
        }
        println!("emulated {} frames = {:.1} per second of play", self.emulated, self.emulated as f32 / self.started.elapsed().as_secs_f32());
        println!(
            "frames {}  fps {:.2}  frame {:.2} ms ± {:.2}  p99 {:.2} ms  max {:.2} ms  late(>25ms) {}  core {:.2} ms/frame (max {:.1})  audio underruns {}  over {:.0} s",
            v.len(),
            1000.0 / mean,
            mean,
            sd,
            p99,
            v[v.len() - 1],
            late,
            run,
            run_max,
            underruns,
            self.started.elapsed().as_secs_f32()
        );
        if !self.draw_ms.is_empty() {
            let mut d = self.draw_ms.clone();
            d.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mean = d.iter().sum::<f32>() / d.len() as f32;
            println!("draw {:.2} ms/frame  median {:.2}  p99 {:.2} ms (until the GPU finished)", mean, d[d.len() / 2], d[(d.len() * 99 / 100).min(d.len() - 1)]);
        }
        if !self.strip_us.is_empty() {
            let s = &self.strip_us;
            println!("pixel path strip: {} updates, {:.0} µs each (max {:.0} µs)", s.len(), s.iter().sum::<f32>() / s.len() as f32, s.iter().cloned().fold(0.0, f32::max));
        }
    }
}

struct App {
    core: Core,
    window: Option<Window>,
    gfx: Option<Gfx>,
    audio: Option<Audio>,
    /// `Audio::open` running on its thread until `resumed` takes the result (instant start).
    audio_opening: Option<std::thread::JoinHandle<Option<Audio>>>,
    /// When the frame around the picture was first on screen with the game (at the first frame, or a frame or two
    /// later when it was still being prepared, `Renderer::set_frame_async`); it fades in from then (`Overlay::draw_veil`).
    shown_at: Option<Instant>,
    /// Every player's buttons and sticks, numbered like the RetroPad (couch co-op: `players::Inputs`; player 1 is
    /// the Pocket unless the pause menu's Players page chose otherwise).
    input: Arc<Inputs>,
    /// The finger on the touch screen, read by its own thread (`touch`); the core gets it as a pointer.
    touch: Arc<pocket_ui::touch::Shared>,
    /// Which console button each Mini V2 button presses, and the left stick as D-pad (pause menu: Controls).
    mapping: Mapping,
    /// Per player.
    stick_pad: [StickPad; players::MAX],
    exit_since: Option<Instant>,
    stats: Stats,
    frames: u64,
    bench: Option<Duration>,
    /// Display refresh period; vsync alone lets some frames through early on this sway.
    period: Duration,
    next_frame: Option<Instant>,
    /// Last frame the core produced, for save previews.
    last_frame: Option<Frame>,
    /// `/storage/roms/savestates/<system>/<game>`, the start of every state file (see `slots`).
    states: PathBuf,
    commands: std::sync::mpsc::Receiver<String>,
    /// Picture settings (pause menu), and the console/ROM they are read for.
    video: VideoSettings,
    video_src: (String, PathBuf),
    /// The pause menu is open: the core doesn't run and nothing is drawn (pocket-ui covers us).
    paused: bool,
    /// pocket-ui's sleep (screen off, or the system suspended): the core doesn't run either.
    sleeping: bool,
    /// pocket-ui's "screen off" (the power button set to Screen off): the game and its sound go on, nothing is drawn
    /// (the time per frame is `limit`'s), and no button reaches the game until "wake" (then the ones still held only
    /// once they were let go).
    dark: bool,
    /// Cozy's launch animation still plays over pocket-ui's shelf: nothing is drawn, so the window stays unmapped
    /// until it is over (`startup::held`), a button cuts it short. The game itself runs, silently.
    hold: bool,
    /// The old-TV power-on over the game's first 300 ms (`crt_on`): wanted by pocket-ui (Cozy, Motion Full) and not yet
    /// cut short by a button; `crt_started` once its hook was written.
    crt_on: bool,
    crt_started: bool,
    /// Woken from sleep and running: the next frame swapped is the first after the wake (`core-frame wake` in the
    /// trace, for `latency.py measure --wake`).
    woke_frame: bool,
    /// Buttons held when the game resumed (the one that closed the menu); hidden from the core until
    /// released. Per player.
    held_mask: [u16; players::MAX],
    /// Size of the last frame drawn in play, the size previews are rendered at.
    view: (u32, u32),
    /// A command that arrived while waiting during the pause.
    early: Option<String>,
    /// Fast-forward, rewind, hotkeys, CPU and fan profile.
    ex: Extras,
    hud: Hud,
    /// The save slot (0-based) for the menu's Save/Load and the hotkeys.
    slot: usize,
    /// Samples of one frame, to keep the audio queue fed while sound is muted.
    nominal_samples: usize,
    /// The core's rate is not close to the display's (PAL 50 Hz, WonderSwan 75 Hz): core frames are
    /// scheduled by time, not one per refresh. `owed` is the time not yet spent on core frames.
    time_driven: bool,
    owed: f64,
    /// Loudest sample the core produced, to tell silence from sound in test runs.
    audio_peak: u16,
    /// `--rotate N`: turn the picture N quarter turns as if the core had asked for them (SET_ROTATION), whatever it
    /// says. The core's aspect is then taken as the turned picture's; Options → Picture → Screen orientation turns any
    /// game in its own shape.
    rotate_override: Option<u32>,
    /// Test input (POCKET_CORE_TEST=1): buttons pressed by "press" commands and when they are let go.
    inject_release: Vec<(u16, Instant)>,
    /// Frames to run before loading the auto-resume state (3D cores need their GL context first).
    resume_in: Option<u32>,
    /// RetroArch's options for a 3D core, beneath ours (see `retroarch_beneath`); empty otherwise.
    ra_options: Vec<(String, String)>,
    /// RetroAchievements for this game, when an account is signed in (`pocket_ui::achievements`).
    cheevos: Option<pocket_ui::achievements::session::Session>,
    /// The unlock sound over the game's sound, and how loud (0: UI sounds are off).
    chime: pocket_ui::achievements::chime::Player,
    chime_gain: f32,
    /// Screenshots (HOME + A, `pocket_ui::screenshot`): the thread that encodes and writes them, started at the first
    /// one, and what it reported back. The emulation thread only copies the frame and hands it over.
    shots: Option<Shots>,
    /// Pixel Path: the FPS counter's last half-seconds of play (one push every 0.5 s, `pixel_path::Window`).
    pixel_window: pocket_ui::pixel_path::Window,
    /// Over the game on Pixel Path (`pixel_live`): what the strip remembers between updates (its alerts).
    pixel_live: pocket_ui::pixel_live::Live,
    /// The discs last written for the pause menu's Disc row (`publish_discs`).
    discs: Option<pocket_ui::disc::Discs>,
    /// Earlier (`pocket_ui::timeline`): play time, the cost guard, and the thread that writes its states.
    timeline: pocket_ui::timeline::Timeline,
    /// When the pause menu opened: a state Earlier left for a pause is taken a second into it (`timeline_quiet`).
    paused_at: Option<Instant>,
}

impl App {
    /// Sets up the window and the GL context. When that fails the reason goes to ERROR_FILE for pocket-ui
    /// to show (`fail`), instead of an abort that leaves nothing behind (K5).
    fn create_gfx(&mut self, el: &ActiveEventLoop) {
        if let Err(e) = self.try_create_gfx(el) {
            fail(&format!("The graphics couldn't start: {e}"));
        }
    }

    fn try_create_gfx(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        let attrs = Window::default_attributes().with_title("pocket-core").with_fullscreen(Some(Fullscreen::Borderless(None)));
        let template = ConfigTemplateBuilder::new().with_alpha_size(0);
        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attrs))
            .build(el, template, |mut configs| configs.next().unwrap_or_else(|| fail("The graphics couldn't start: no EGL configuration.")))
            .map_err(|e| format!("display: {e}"))?;
        let window = window.ok_or("no window")?;
        let handle = window.window_handle().ok().map(|h| h.as_raw());
        let display = config.display();
        let ctx_attrs = hw_render::main_context_attributes(handle);
        let not_current = unsafe { display.create_context(&config, &ctx_attrs).map_err(|e| format!("GLES 3 context: {e}"))? };
        let surface_attrs = window.build_surface_attributes(Default::default()).map_err(|e| format!("surface attributes: {e}"))?;
        let surface = unsafe { display.create_window_surface(&config, &surface_attrs).map_err(|e| format!("window surface: {e}"))? };
        let context = not_current.make_current(&surface).map_err(|e| format!("make current: {e}"))?;
        surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap())).ok();
        let gl = unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) };

        let mut video = Renderer::new(gl).map_err(|e| format!("video shaders: {e}"))?;
        video.set_frame_source(&self.video_src.0, &data_dir().join("frames"));
        // The screen frame doesn't hold up the first frame: read from the cache or drawn on a thread, it fades in when
        // it is there (perf/bezel-cache, docs/FRAMES.md "Cache"). Off again once it has faded in (`draw`).
        video.set_frame_async(true);
        let overlay = Overlay::new(video.gl()).map_err(|e| eprintln!("overlay shaders: {e}")).ok();
        // A 3D core gets its context now that ours exists; from here on the core's is current
        // except while we draw (and render previews).
        let hw = match hw_render::requested() {
            Some(_) => {
                let g = self.core.av.geometry;
                Some(HwContext::new(&display, &config, &context, &surface, handle, (g.max_width, g.max_height))?)
            }
            None => None,
        };
        self.gfx = Some(Gfx { surface, context, video, overlay, hw });
        self.window = Some(window);
        Ok(())
    }

    /// The aspect ratio the picture is drawn at: the core's, or the system's own (`hostsys::display_aspect`).
    fn display_aspect(core: &Core, system: &str) -> f32 {
        let g = core.av.geometry;
        hostsys::display_aspect(system, g.base_width, g.base_height, g.aspect_ratio)
    }

    fn draw(&mut self) {
        let (Some(window), Some(gfx)) = (&self.window, &mut self.gfx) else { return };
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        self.view = (w, h);
        gfx.surface.resize(&gfx.context, NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap());
        let aspect = Self::display_aspect(&self.core, &self.video_src.0);
        gfx.video.set_rotation(self.rotate_override.unwrap_or_else(|| self.core.rotation()));
        let t0 = Instant::now();
        gfx.video.draw(w, h, aspect, &self.video);
        if let Some(overlay) = gfx.overlay.as_mut() {
            // The first quarter second: the frame around the picture fades in from the black pocket-ui showed around
            // the game's last picture while it started (`launch_picture`). Not in bench runs.
            let since = self.shown_at.map_or(Duration::ZERO, |t| t.elapsed());
            if self.bench.is_none() && since < pocket_ui::overlay::VEIL {
                let hole = gfx.video.placement(w, h, aspect, &self.video).map(|(r, _)| (r.x as f32, r.y as f32, r.w as f32, r.h as f32));
                overlay.draw_veil(gfx.video.gl(), w, h, hole, pocket_ui::overlay::veil_alpha(since));
            }
            // Cozy's power-on: dark, a line, the picture opening out of it (`crt_on`), over the game's first 300 ms.
            if self.crt_on && self.bench.is_none() {
                if let Some(t) = self.shown_at {
                    if !self.crt_started {
                        self.crt_started = true;
                        pocket_ui::crt_on::start();
                        // Its sound, a thunk and a faint whine, into the game's own: the chime, at the UI sounds' volume.
                        let rate = self.core.av.timing.sample_rate;
                        if self.chime_gain > 0.0 && rate > 0.0 && !pocket_ui::crt_on::silent() {
                            self.chime.start_samples(pocket_ui::ui_sound::synth_at(pocket_ui::ui_sound::Sound::CrtOn, rate as u32), self.chime_gain);
                        }
                    }
                    // Over the game's picture only, the frame around it keeps its own fade-in (the veil).
                    let area = gfx.video.placement(w, h, aspect, &self.video).map_or((0.0, 0.0, w as f32, h as f32), |(r, _)| (r.x as f32, r.y as f32, r.w as f32, r.h as f32));
                    let quads = pocket_ui::crt_on::plan(t.elapsed(), area);
                    if quads.is_empty() {
                        self.crt_on = false;
                    } else {
                        overlay.draw_solid(gfx.video.gl(), w, h, &quads);
                    }
                } else {
                    // The game's first picture isn't there yet: it stays dark.
                    let area = gfx.video.placement(w, h, aspect, &self.video).map_or((0.0, 0.0, w as f32, h as f32), |(r, _)| (r.x as f32, r.y as f32, r.w as f32, r.h as f32));
                    overlay.draw_solid(gfx.video.gl(), w, h, &[pocket_ui::crt_on::Quad { rect: area, white: false, alpha: 1.0 }]);
                }
            }
            overlay.draw(gfx.video.gl(), &mut self.hud, w, h);
        }
        // Bench runs measure what drawing costs (frame styles, effects); waiting for the GPU here
        // would only add latency in play.
        if self.bench.is_some() {
            unsafe { glow::HasContext::finish(gfx.video.gl()) };
            self.stats.draw_ms.push(t0.elapsed().as_secs_f32() * 1000.0);
        }
        // Blocks until the next vsync: this paces the whole loop.
        gfx.surface.swap_buffers(&gfx.context).ok();
        // Fast switching: when the game became visible (`fast_switch`, once per process).
        pocket_ui::fast_switch::first_frame();
        // The veil starts with the frame: until it is there the bands are black anyway, so it never pops in.
        if self.shown_at.is_none() && gfx.video.has_frame() && !gfx.video.frame_pending() {
            self.shown_at = Some(Instant::now());
        }
        if self.shown_at.is_some_and(|t| t.elapsed() >= pocket_ui::overlay::VEIL) {
            gfx.video.set_frame_async(false);
        }
        if let Some(started) = STARTED.get().filter(|_| !FIRST_SWAP.swap(true, Ordering::Relaxed)) {
            pocket_ui::startup::mark("first frame");
            let launch = pocket_ui::startup::since_launch_ms().map_or(String::new(), |ms| format!(", {ms:.0} ms after the game was started in pocket-ui"));
            println!("first frame after {:?}{launch} ({} ms)", started.elapsed(), pocket_ui::startup::report(*started));
        }
        // Instant on: the first frame after a wake is out (`latency.py measure --wake`).
        if std::mem::take(&mut self.woke_frame) {
            pocket_ui::mark!("core-frame", "wake");
        }
        // Speed guard (scripts/latency.sh): the game's first frame is out. Off unless POCKETUI_TRACE is set.
        if pocket_ui::trace::on() {
            static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
            if FIRST.swap(false, std::sync::atomic::Ordering::Relaxed) {
                pocket_ui::mark!("core-frame", "first");
            }
        }
    }

    /// The finger as the core's pointer: on the picture as drawn (Nintendo DS: on its bottom screen).
    fn touch_pointer(&self) -> Option<(i16, i16)> {
        let at = self.touch.frame()?;
        let gfx = self.gfx.as_ref()?;
        let (w, h) = self.view;
        let picture = gfx.video.picture(w, h, Self::display_aspect(&self.core, &self.video_src.0), &self.video);
        // melonDS DS's layouts (`melonds_screen_layout1..N`) tell where its bottom screen is in the frame.
        let count = self.core.option_value("melonds_number_of_screen_layouts").and_then(|n| n.parse::<usize>().ok()).unwrap_or(1).clamp(1, 8);
        let layouts: Vec<String> = (1..=count).filter_map(|i| self.core.option_value(&format!("melonds_screen_layout{i}"))).collect();
        let layouts: Vec<&str> = layouts.iter().map(String::as_str).collect();
        pocket_ui::touch::pointer_for(&self.video_src.0, at, (w, h), picture, &layouts)
    }

    /// How many core frames are due on this display refresh: one, or by time for cores whose rate
    /// differs from the display's (50 Hz games get a repeated picture every sixth refresh, 75 Hz
    /// ones two frames every fourth).
    fn frames_due(&mut self) -> u32 {
        if !self.time_driven {
            return 1;
        }
        let period = 1.0 / self.core.av.timing.fps.max(1.0);
        self.owed += self.period.as_secs_f64();
        let n = (self.owed / period) as u32;
        self.owed -= n as f64 * period;
        n.min(4)
    }

    /// Each player's buttons as the console's (the Controls mapping, the left stick as D-pad, buttons still held from
    /// the pause menu hidden until let go) and their sticks, player 1 first (`players::Inputs`). The mapping is the
    /// same for every player.
    fn read_players(&mut self) -> ([u16; players::MAX], [[i16; 4]; players::MAX]) {
        let mut buttons = [0; players::MAX];
        let mut sticks = [[0; 4]; players::MAX];
        // Options → Picture → Screen orientation: the Pocket held turned turns its own D-pad and sticks with the
        // picture (a Bluetooth controller isn't turned with it).
        let turn = self.video.orientation.turns();
        for p in 0..players::MAX {
            let s = self.input.sticks(p);
            let turn = if self.input.is_pocket(p) { turn } else { 0 };
            let (lx, ly) = if self.mapping.stick_dpad { s.left() } else { (0.0, 0.0) };
            let raw = controls::turn_buttons(self.input.buttons(p) | self.stick_pad[p].update(lx, ly), turn);
            // Buttons still held from the pause menu reach the game only after they were let go. With the screen off
            // nothing reaches it: what is held then stays held back until it is let go.
            self.held_mask[p] = if self.dark { raw } else { self.held_mask[p] & raw };
            buttons[p] = self.mapping.translate(raw & !self.held_mask[p]);
            sticks[p] = if self.dark { [0; 4] } else { controls::turn_sticks(s.analog(!self.mapping.stick_dpad), turn) };
        }
        (buttons, sticks)
    }

    fn step(&mut self) {
        let now = Instant::now();
        if !self.inject_release.is_empty() {
            for (bit, _) in self.inject_release.iter().filter(|(_, t)| *t <= now) {
                self.input.inject(*bit, false);
            }
            self.inject_release.retain(|(_, t)| *t > now);
        }
        // Couch co-op: every player's buttons and sticks; player 1's go with `run`, the others' to their ports below.
        let (players, sticks) = self.read_players();
        let (mapped, analog) = (players[0], sticks[0]);
        // Over when the deadline passed or any button is pressed (pocket-ui skips its animation on the same press).
        if self.hold && (!pocket_ui::startup::held() || players.iter().any(|p| *p != 0)) {
            self.hold = false;
            // A press that ends the hold is someone who wants the game now: no effect.
            if players.iter().any(|p| *p != 0) {
                self.crt_on = false;
            }
        }
        if self.crt_on && players.iter().any(|p| *p != 0) {
            self.crt_on = false;
        }
        for note in self.input.take_notes() {
            self.hud.toast(note);
        }
        let pointer = self.touch_pointer();
        self.core.set_analog(analog);
        self.core.set_pointer(pointer);
        let t0 = Instant::now();
        let (home, rewind_held, actions) = {
            let mut hk = self.ex.hotkeys.lock().unwrap();
            (hk.home_held(), hk.rewind_held(), hk.take())
        };
        // With the screen off no shortcut acts (a press in a pocket); the game just goes on.
        let (rewind_held, actions) = if self.dark { (false, Vec::new()) } else { (rewind_held, actions) };
        // A screenshot only reads the last frame: run-ahead keeps its state.
        if actions.iter().any(|a| *a != Action::Screenshot) {
            self.ex.ahead.forget();
        }
        for a in actions {
            self.hotkey(a);
        }
        self.screenshot_results();
        // While HOME is down the buttons belong to the shortcuts, not the game.
        let buttons = if home { 0 } else { mapped };
        for p in 1..players::MAX {
            self.core.set_player(p, if home { 0 } else { players[p] }, sticks[p]);
        }
        if rewind_held && !self.ex.rewind_was_held && !self.ex.cfg.rewind {
            self.hud.toast("Rewind is off. Switch it on under Extras.");
        }
        if self.ex.rewind_was_held && !rewind_held && self.ex.rewound > 0 {
            println!("rewound {} frames, {} left in the history", self.ex.rewound, self.ex.rewind.len());
            self.ex.rewound = 0;
            // Achievements: the moment rewound to can't unlock anything by itself.
            if let Some(a) = self.cheevos.as_mut() {
                a.reset();
            }
        }
        self.ex.rewind_was_held = rewind_held;
        let back = rewind_held && self.ex.cfg.rewind;
        let ff = if back || self.dark { None } else { self.ex.fast_forward() };

        let (mut frame, mut samples, mut av, mut geometry) = (None, Vec::new(), None, None);
        // Set when run-ahead ran this refresh (`run_ahead`): its state of the real game is what rewind keeps.
        let mut ran_ahead = false;
        if back {
            self.ex.ahead.forget();
            // One state back per screen refresh: real-time reverse. At the oldest state the picture holds.
            if let Some(state) = self.ex.rewind.step_back() {
                // Taken from this core in this session: the right size, and the core's own.
                let _ = self.core.load_state(state);
                (frame, samples, av, geometry) = self.core.run(0);
                if let Some(a) = self.cheevos.as_mut() {
                    a.idle();
                }
                self.stats.emulated += 1;
                self.hud.emulated += 1;
                self.ex.rewound += 1;
                if let (Some(v), Some(f)) = (self.ex.verify.as_mut(), frame.as_ref()) {
                    match v.hashes.pop_back() {
                        Some(h) if h == pocket_ui::extras::frame_hash(&f.data) => v.same += 1,
                        Some(_) => v.different += 1,
                        None => {}
                    }
                }
            }
        } else {
            let runs = match ff {
                None => self.frames_due(),
                Some(FastForward::Times(n)) => n,
                Some(FastForward::Max) => MAX_FF_RUNS,
            };
            // Run-ahead (src/run_ahead.rs): the due frames, then 1-2 frames ahead shown, then back. Not while
            // fast-forwarding. When it runs nothing (off, switched itself off), the loop below runs as always.
            if ff.is_none() && self.run_ahead_ready() {
                // The other players' input counts too: a change of theirs is no mismatch of the frames ahead.
                let input = (1..players::MAX).fold(pocket_ui::run_ahead::input_key(buttons, analog, pointer), |k, p| {
                    k.rotate_left(17) ^ pocket_ui::run_ahead::input_key(players[p], sticks[p], None)
                });
                let period_ms = self.period.as_secs_f32() * 1000.0;
                let (stats, hud, cheevos) = (&mut self.stats, &mut self.hud, &mut self.cheevos);
                ran_ahead = self.ex.ahead.step(&mut self.core, runs, buttons, input, period_ms, |(f, s, a, g), keep| {
                    if keep.video {
                        frame = f.or(frame.take());
                    }
                    if keep.audio {
                        samples.extend_from_slice(&s);
                    }
                    // Size or rate changes of frames ahead too: the picture shown is one of them.
                    (av, geometry) = (a.or(av), g.or(geometry));
                    if keep.real {
                        stats.emulated += 1;
                        hud.emulated += 1;
                        // Achievements look at the game's memory after every real frame, never after one run ahead
                        // (that one is undone by the state load at the end of the refresh).
                        if let Some(cheevos) = cheevos.as_mut() {
                            cheevos.frame();
                        }
                    }
                });
                if let Some(n) = self.ex.ahead.take_notice() {
                    self.hud.toast(n);
                }
            }
            // Fast-forward shows the last frame of a refresh and mutes the sound: the others are run without
            // picture or sound for cores that can skip them (GET_AUDIO_VIDEO_ENABLE). 3D cores make every frame.
            let skip = ff.is_some() && self.gfx.as_ref().is_some_and(|g| g.hw.is_none());
            // The CPU time of an ordinary refresh, for the exit report's baseline (`RunAhead::note_plain`).
            let plain_c0 = (ff.is_none() && !ran_ahead && runs > 0).then(pocket_ui::run_ahead::thread_cpu_ms);
            for i in (0..runs).take_while(|_| !ran_ahead) {
                // "As fast as possible" stops at its time budget: the frame expected to be the last one is shown.
                let spent = t0.elapsed().as_secs_f32() * 1000.0;
                let last = i + 1 == runs || (ff == Some(FastForward::Max) && i >= 1 && spent + spent / i as f32 > MAX_FF_BUDGET_MS);
                let enable = match (skip, last) {
                    (false, _) => AV_ALL,
                    (true, true) => AV_VIDEO,
                    (true, false) => 0,
                };
                let (f, s, a, g) = self.core.run_with(buttons, enable);
                // Achievements look at the game's memory after every emulated frame.
                if let Some(cheevos) = self.cheevos.as_mut() {
                    cheevos.frame();
                }
                // Several frames in one refresh (75 Hz cores): all their sound is kept. Fast-forward drops it anyway.
                if ff.is_none() {
                    samples.extend_from_slice(&s);
                } else {
                    samples = s;
                }
                (frame, av, geometry) = (f.or(frame), a.or(av), g.or(geometry));
                self.stats.emulated += 1;
                self.hud.emulated += 1;
                if last {
                    break;
                }
            }
            if let Some(c0) = plain_c0 {
                self.ex.ahead.note_plain((pocket_ui::run_ahead::thread_cpu_ms() - c0) as f32);
            }
            if self.ex.cfg.rewind {
                let c0 = Instant::now();
                if ran_ahead {
                    // Run-ahead saved the real game already; the picture shown is ahead of it, so no rewind check.
                    self.ex.rewind.push(self.ex.ahead.state());
                } else if self.core.save_state_into(&mut self.ex.scratch) {
                    self.ex.rewind.push(&self.ex.scratch);
                    if let (Some(v), Some(f)) = (self.ex.verify.as_mut(), frame.as_ref().or(self.last_frame.as_ref())) {
                        v.hashes.push_back(pocket_ui::extras::frame_hash(&f.data));
                        while v.hashes.len() > self.ex.rewind.len() + 1 {
                            v.hashes.pop_front();
                        }
                    }
                }
                self.ex.capture.add(c0.elapsed().as_secs_f32() * 1000.0);
            }
        }
        self.stats.run_ms.push(t0.elapsed().as_secs_f32() * 1000.0);
        if self.core.take_snapshot_dirty() {
            self.write_snapshot();
        }
        if let Some(av) = av {
            self.core.av = av;
            self.sync_audio_rate();
        }
        if let Some(g) = geometry {
            self.core.av.geometry = g;
        }
        // With the screen off nothing is uploaded or drawn: `limit` keeps the time per frame, the sound goes on.
        if let (Some(frame), Some(gfx), false) = (frame.as_ref(), self.gfx.as_mut(), self.dark) {
            gfx.video.upload(frame);
        }
        self.hw_frame(av.is_some());
        if frame.is_some() {
            self.last_frame = frame;
        }
        self.audio_peak = samples.iter().map(|s| s.unsigned_abs()).fold(self.audio_peak, u16::max);
        // Achievements: unlocks to the HUD; the unlock sound over the game's own while its card comes up.
        if let Some(a) = self.cheevos.as_ref() {
            pocket_ui::achievements::game::show(a.take_notices(), &mut self.hud);
        }
        if self.hud.take_chime() {
            self.chime.start(self.core.av.timing.sample_rate, self.chime_gain);
        }
        if self.chime.playing() && !back && ff.is_none() {
            self.chime.mix(&mut samples);
        }
        // Sound is muted while fast-forwarding or rewinding, but the queue still gets its silence
        // so playback continues smoothly when the shortcut is released.
        if let Some(audio) = self.audio.as_mut() {
            if back || ff.is_some() || self.hold {
                audio.push(&vec![0; if samples.is_empty() { self.nominal_samples } else { samples.len() }]);
            } else {
                if !samples.is_empty() {
                    self.nominal_samples = samples.len();
                }
                audio.push(&samples);
            }
        }
        // Over the game (`over_game`): the FPS line, with the sensors, or Pixel Path's line (`update_strip`), one at a
        // time. FPS + sensors starts or ends the reader thread when the level changes; nothing else per frame.
        self.hud.show_fps = self.ex.over_game.counter();
        self.hud.set_sensors(self.ex.over_game.sensors());
        self.hud.badge = match (back, ff) {
            (true, _) => "REWIND".into(),
            (_, Some(speed)) => format!("FF {}", speed.label()),
            _ => String::new(),
        };
        if !self.dark && !self.hold {
            self.draw();
        }
        // Twice a second the FPS counter's half-second goes into Pixel Path's window, with the underruns so far.
        if let Some(tick) = self.hud.frame() {
            let underruns = self.audio.as_ref().map_or(0, |a| *a.underruns.lock().unwrap());
            self.pixel_window.push(tick, underruns);
            self.update_strip();
        }
        self.hw_resume();
        // Earlier: in what is left of this refresh, before the wait for the next one.
        self.timeline_play();
        self.limit();
        // Battery saves reach the disk every 10 s (RetroArch's autosave interval), not only on exit,
        // so a crash or power loss costs seconds, not the session.
        self.frames += 1;
        if self.frames % 600 == 0 {
            // What became of the last flushes (no waiting): a line over the game while they fail (`save_trouble`).
            self.note_save_trouble(self.core.save_trouble());
            self.core.write_sram_in_background();
        }
        let now = Instant::now();
        if let Some(last) = self.stats.last_swap {
            self.stats.intervals.push((now - last).as_secs_f32() * 1000.0);
        }
        self.stats.last_swap = Some(now);
    }

    /// Whether run-ahead runs this refresh. Switched on, the core is looked at once first (`RunAhead::admit`);
    /// a refusal shows as a notice.
    fn run_ahead_ready(&mut self) -> bool {
        if self.ex.ahead.needs_admit() {
            let hw = self.gfx.as_ref().is_some_and(|g| g.hw.is_some());
            self.ex.ahead.admit(self.core.has_states(), hw, self.core.serialization_quirks());
            if let Some(n) = self.ex.ahead.take_notice() {
                self.hud.toast(n);
            }
        }
        self.ex.ahead.frames() > 0
    }

    /// Hardware-rendered core: switches from its context to ours and copies a new frame into the
    /// renderer. `av_changed`: the core changed its maximum size, so its framebuffer follows.
    fn hw_frame(&mut self, av_changed: bool) {
        let Some(Gfx { surface, context, video, hw: Some(hw), .. }) = self.gfx.as_mut() else { return };
        if av_changed {
            let g = self.core.av.geometry;
            if let Err(e) = hw.resize(g.max_width, g.max_height) {
                eprintln!("[hw] {e}");
            }
        }
        context.make_current(surface).ok();
        if let Some((w, h)) = hw_render::take_frame() {
            let (fw, fh) = hw.size();
            video.copy_texture(hw.texture(), w.min(fw), h.min(fh), hw.request.bottom_left_origin);
            // Instant resume for 3D cores waits until they present frames with their GL context:
            // PPSSPP boots on a thread and hangs on a state loaded before that.
            if let Some(n) = self.resume_in.as_mut() {
                *n = n.saturating_sub(1);
            }
        }
    }

    /// Hands the GL thread back to the core after we drew (hardware-rendered cores only).
    fn hw_resume(&mut self) {
        if let Some(Gfx { surface, hw: Some(hw), .. }) = self.gfx.as_ref() {
            hw.make_current(surface);
        }
        if self.resume_in == Some(0) {
            self.resume_in = None;
            self.resume();
        }
    }

    /// Save-state preview of a hardware-rendered core, read back from the renderer's texture.
    fn hw_snapshot(&self) -> Option<Frame> {
        let Some(Gfx { surface, context, video, hw: Some(hw), .. }) = self.gfx.as_ref() else { return None };
        context.make_current(surface).ok()?;
        let frame = video.read_frame();
        hw.make_current(surface);
        frame
    }

    /// Waits until one display period has passed since the previous frame. The deadline advances by
    /// whole periods, so short hiccups are absorbed; after a long stall it resyncs.
    fn limit(&mut self) {
        let now = Instant::now();
        let next = self.next_frame.map_or(now, |n| n + self.period);
        if next > now {
            // Sleep most of the way, spin the last stretch for accuracy.
            let until = next - Duration::from_micros(150);
            if until > now {
                std::thread::sleep(until - now);
            }
            while Instant::now() < next {
                std::hint::spin_loop();
            }
            self.next_frame = Some(next);
        } else {
            self.next_frame = Some(if now - next > self.period * 2 { now } else { next });
        }
    }

    /// The core runs once per display refresh, slightly off its own rate (59.73 Hz for a Game Boy).
    /// Feeding its samples at the display rate keeps pitch right and audio in step.
    fn sync_audio_rate(&mut self) {
        let skew = 1.0 / (self.period.as_secs_f64() * self.core.av.timing.fps);
        // Within 5 % the core simply runs once per refresh; further off, frames are scheduled by time
        // and the audio keeps its own rate.
        self.time_driven = !(0.95..1.05).contains(&skew);
        let skew = if self.time_driven { 1.0 } else { skew };
        if let Some(audio) = self.audio.as_mut() {
            audio.set_input_rate(self.core.av.timing.sample_rate * skew);
        }
    }

    /// Writes a state and its preview image; the preview first so a listed state always has one.
    fn save_to(&self, path: &Path) -> bool {
        let t0 = Instant::now();
        let Some(data) = self.core.save_state() else { return false };
        // Latency trace only (`trace::step`): where a slow exit save spends its time.
        pocket_ui::trace::step("save: state serialized");
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Hardware-rendered cores leave no frame in memory: their preview is read back from the GPU.
        let hw_frame = if self.last_frame.is_none() { self.hw_snapshot() } else { None };
        if let Some(frame) = self.last_frame.as_ref().or(hw_frame.as_ref()) {
            // Encoded in memory, then put in place through a new temporary file: never written into a link someone
            // left in the (synced) states folder, never half a picture (review N3). No fsync: a lost preview is harmless.
            let mut png_bytes = Vec::new();
            let mut enc = png::Encoder::new(&mut png_bytes, frame.width, frame.height);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let encoded = enc.write_header().and_then(|mut w| w.write_image_data(&frame.to_rgb())).is_ok();
            if encoded {
                let _ = pocket_ui::fsutil::replace_file(&slots::preview_path(path), &png_bytes);
            }
            pocket_ui::trace::step("save: preview written");
        }
        let ok = pocket_ui::fsutil::write_atomic(path, &data).is_ok();
        pocket_ui::trace::step("save: state synced to the SD card");
        if ok {
            println!("saved {} ({} KB, {:.0} ms)", path.display(), data.len() / 1024, t0.elapsed().as_secs_f32() * 1000.0);
        }
        ok
    }

    /// Tells the pause menu what the core offers: its options with their values, its button names, the layout in use.
    /// Also notes whether the core corrects colours itself (the LCD grid's GBA colours then step back).
    fn write_snapshot(&mut self) {
        let (categories, options) = self.core.options_snapshot();
        self.video.core_colour = pocket_ui::pixel_path::corrects_colour(&options);
        let targets = controls::targets_for(&self.video_src.0, &self.core.described_inputs());
        let file = CoreOptionsFile {
            core: self.core.name.clone(),
            rom: self.video_src.1.to_string_lossy().into_owned(),
            categories,
            options,
            controls: self.mapping.to_file(targets),
        };
        file.write(&core_options::file_path(&data_dir(), &self.video_src.0));
    }

    /// Pixel Path (pause menu): what this session's picture and sound go through, measured where it can be, written to
    /// `pixel_path::FILE` for request `seq`. Runs only when asked; it reads what is kept anyway (frame times, the FPS
    /// counter's half-seconds, the audio counters).
    fn write_pixel_path(&self, seq: u32) {
        self.pixel_report(seq, true).write(Path::new(pocket_ui::pixel_path::FILE));
    }

    /// Over the game on Pixel Path (`pixel_live`), twice a second with the FPS counter's half-second: the strip from a
    /// fresh report while the level is Pixel Path, else none (no report is built). Also right after HOME + Y.
    fn update_strip(&mut self) {
        let strip = if self.ex.over_game.strip() {
            let t0 = Instant::now();
            // The core's colour options can't change the overall grade (the sound caps it at Faithful): not read here.
            let r = self.pixel_report(0, false);
            let strip = self.pixel_live.update(&r);
            if self.bench.is_some() {
                self.stats.strip_us.push(t0.elapsed().as_secs_f32() * 1e6);
            }
            Some(strip)
        } else {
            self.pixel_live.hidden(self.ex.ahead.off().is_some());
            None
        };
        self.hud.set_strip(strip);
    }

    /// What `write_pixel_path` writes; `colour`: with the core's colour options (the page's Look stage).
    fn pixel_report(&self, seq: u32, colour: bool) -> pocket_ui::pixel_path::Report {
        use pocket_ui::pixel_path as pp;
        let g = self.core.av.geometry;
        let (view_w, view_h) = if self.view.0 > 0 { self.view } else { pocket_ui::device::current().screen() };
        let aspect = Self::display_aspect(&self.core, &self.video_src.0);
        let gfx = self.gfx.as_ref();
        let (frame_w, frame_h) = gfx.map_or((0, 0), |g| g.video.frame_size());
        let rect = gfx.and_then(|g| g.video.placement(view_w, view_h, aspect, &self.video)).map(|(r, _)| r);
        let period_ms = self.period.as_secs_f32() * 1000.0;
        let (frames, fps, late, core_ms, core_ms_max, secs) = pp::frame_window(&self.stats.intervals, &self.stats.run_ms, period_ms, pp::WINDOW_SECS);
        let (emulated_fps, cpu, underruns) = self.pixel_window.summary();
        let ahead = &self.ex.ahead;
        pp::Report {
            seq,
            rom: self.video_src.1.to_string_lossy().into_owned(),
            system: self.video_src.0.clone(),
            core: self.core.name.clone(),
            core_version: self.core.version.clone(),
            source: pp::Source {
                base_w: g.base_width,
                base_h: g.base_height,
                frame_w,
                frame_h,
                fps: self.core.av.timing.fps,
                aspect,
                hw: gfx.is_some_and(|g| g.hw.is_some()),
                rotation: self.rotate_override.unwrap_or_else(|| self.core.rotation()),
                turn: self.video.orientation.turns(),
            },
            picture: pp::Picture {
                view_w,
                view_h,
                x: rect.map_or(0, |r| r.x),
                y: rect.map_or(0, |r| r.y),
                w: rect.map_or(0, |r| r.w),
                h: rect.map_or(0, |r| r.h),
                scaling: pp::scaling_value(self.video.scaling).into(),
                sharpness: pp::sharpness_value(self.video.sharpness).into(),
            },
            look: pp::Look {
                effect: pp::effect_value(self.video.effect).into(),
                frame: pp::frame_value(&self.video).into(),
                // Either backend changes the game's colours: the multiply layer here (Soft) or the compositor's gamma
                // ramps, which also take the contrast (`display.rs`).
                warmth: self.hud.look.warmth,
                contrast: if self.hud.look.soft { 0 } else { self.hud.look.contrast },
                colour: if colour { pp::colour_options(&self.core.options_snapshot().1) } else { Vec::new() },
                lcd_colour: self.video.lcd_colours(&self.video_src.0),
            },
            display: pp::Display { refresh_hz: 1.0 / self.period.as_secs_f64(), time_driven: self.time_driven },
            run_ahead: pp::RunAhead {
                offered: pocket_ui::run_ahead::available_for(&self.video_src.0),
                wanted: ahead.wanted(),
                frames: ahead.frames(),
                pending: ahead.needs_admit(),
                off: ahead.off().map_or(String::new(), |o| o.notice().to_string()),
            },
            audio: self.audio.as_ref().map(|a| pp::Audio {
                backend: a.backend.to_string(),
                core_rate: self.core.av.timing.sample_rate,
                in_rate: a.input_rate(),
                out_rate: pocket_ui::audio::OUT_RATE,
                target_ms: a.target_ms() as f32,
                queue_ms: a.buffered_ms() as f32,
                server_ms: a.server_ms() as f32,
                underruns: *a.underruns.lock().unwrap(),
                bluetooth: a.bluetooth(),
            }),
            live: pp::Live { secs, frames, fps, late, emulated_fps, core_ms, core_ms_max, cpu, underruns },
            paused: self.paused || self.sleeping,
            patches: self.core.patches.clone(),
        }
    }

    /// Instant resume: continues where the last session ended.
    fn resume(&mut self) {
        // "Before load" of a session that didn't end with a Quit (a crash) is gone with it.
        slots::clear_undo(&self.states);
        let path = slots::auto_path(&self.states);
        let resumed = self.core.resume(&path);
        if matches!(resumed, Resume::Resumed | Resume::ResumedAgain) {
            if let Some(a) = self.cheevos.as_mut() {
                a.reset();
            }
        }
        // A hardware core resumes some 30 frames in (`resume_in`), by when rewind has recorded the boot: that history
        // belongs to another timeline, as after a manual load (SOL-11). Also after a state that was undone again.
        if resumed != Resume::NoState {
            self.ex.rewind.clear();
            self.ex.verify.iter_mut().for_each(|v| v.hashes.clear());
        }
        // Earlier: where this session began is the state it continued from, copied on the writer's thread.
        if matches!(resumed, Resume::Resumed | Resume::ResumedAgain) && self.ex.cfg.timeline {
            let from = if resumed == Resume::Resumed { slots::taken_auto_path(&path) } else { slots::retry_auto_path(&path) };
            self.timeline.submit_copy(&self.states, pocket_ui::timeline::Kind::Started, &from, &slots::preview_path(&path));
        }
        match resumed {
            Resume::Resumed => println!("loaded {}\nresumed", path.display()),
            Resume::ResumedAgain => println!("loaded {}\nresumed (once more: the last session ended without an exit save)", path.display()),
            Resume::Failed => println!("couldn't resume from {}", path.display()),
            Resume::SaveIsNewer => {
                println!("not resumed: the battery save is newer than the resume state of {}", path.display());
                self.hud.toast("In-game save is newer: not resumed");
            }
            Resume::NoState => {}
        }
    }

    /// The state written on every exit and before a sleep, for instant resume. The battery save goes
    /// first, so the state is never older than it (see `Core::resume`).
    /// A battery save that fails (a full or failing card) doesn't hold the state back: it is tried once, and the
    /// state, if it gets written, then holds the newer save (`slots::battery_unsaved`, `Core::resume`).
    fn save_auto(&mut self) {
        let battery = self.core.write_sram();
        self.note_save_trouble(battery.err());
        pocket_ui::trace::step("save: battery save checked");
        let path = slots::auto_path(&self.states);
        if self.save_to(&path) {
            slots::auto_written(&path);
            slots::battery_unsaved(&path, battery.is_err());
            self.flash_frame_light();
        }
    }

    /// Whether the game's saves reach the card (`pocket_ui::save_trouble`): the line over the game and pocket-ui's
    /// note (the pause menu's line and its question before Quit, the shelf's notice after the game) follow, written
    /// only when that changes.
    fn note_save_trouble(&mut self, now: Option<pocket_ui::save_trouble::Trouble>) {
        if self.hud.save_trouble != now {
            self.hud.save_trouble = now;
            pocket_ui::save_trouble::publish_to(Path::new(pocket_ui::save_trouble::FILE), now);
        }
    }

    /// A state was saved: the screen frame's power light flashes (frames that have one).
    fn flash_frame_light(&mut self) {
        if let Some(gfx) = self.gfx.as_mut() {
            gfx.video.flash_frame_light();
        }
    }

    /// The exit save: the auto state, and "Before load" ends with the session.
    fn save_exit(&mut self) {
        self.save_auto();
        slots::clear_undo(&self.states);
    }

    /// The pause menu's "Quit without saving": the battery save as on every exit, but no new auto state. The one
    /// this session resumed from goes back in place (`Core::keep_resume_point`), so the next start continues from
    /// the last saving quit; one older than the battery save written now doesn't roll it back (`Core::resume`).
    fn exit_without_saving(&mut self) {
        let battery = self.core.write_sram();
        self.note_save_trouble(battery.err());
        self.core.keep_resume_point(&slots::auto_path(&self.states));
        slots::clear_undo(&self.states);
    }

    fn save_slot(&mut self, slot: usize) {
        let ok = self.save_to(&slots::state_path(&self.states, slot));
        if ok {
            self.flash_frame_light();
        }
        self.hud.toast(if ok {
            format!("Saved to slot {}", slot + 1)
        } else if !self.core.has_states() {
            // ScummVM: the game's own saves (`hostsys::HostSystem::no_states`).
            hostsys::find(&self.video_src.0).and_then(|h| h.no_states).unwrap_or("This emulator has no save states").to_string()
        } else {
            format!("Couldn't save slot {}", slot + 1)
        });
    }

    fn load_slot(&mut self, slot: usize) {
        self.load_state_file(&slots::state_path(&self.states, slot), &format!("slot {}", slot + 1));
    }

    /// The auto state (or the one this session was resumed from) or "Before load": `load auto`, `load undo`.
    fn load_named(&mut self, name: &str) {
        let set = pocket_ui::states::StateSet::pocket_core(&self.states);
        let (target, label) = match name {
            "auto" => (pocket_ui::states::Target::Auto, "the auto state"),
            _ => (pocket_ui::states::Target::Undo, "the state before the load"),
        };
        match set.path(target).map(Path::to_path_buf) {
            Some(path) => self.load_state_file(&path, label),
            // "There is no auto state": the label without its article.
            None => self.hud.toast(format!("There is no {}", label.trim_start_matches("the "))),
        }
    }

    /// Loads a state file, after keeping what is running now as "Before load" (a Load can be undone, whatever it
    /// brings back; loading "Before load" itself swaps the two, the file being read first).
    fn load_state_file(&mut self, path: &Path, label: &str) {
        // The order of it is `slots::load_keeping_undo`'s (tested there).
        let loaded = slots::load_keeping_undo(
            &self.states,
            path,
            |undo| self.core.has_states() && self.save_to(undo),
            |data| self.core.load_state_reached(data),
            |before| {
                let _ = self.core.load_state(before);
            },
        );
        let msg = match loaded {
            slots::Loaded::Unreadable => {
                // "Slot 3 is empty", "The auto state is empty": the label starts the sentence.
                let mut what = label.to_string();
                if let Some(first) = what.get_mut(..1) {
                    first.make_ascii_uppercase();
                }
                self.hud.toast(format!("{what} is empty"));
                return;
            }
            slots::Loaded::Loaded => {
                println!("loaded {}", path.display());
                // Achievements start again from waiting: the loaded moment itself can't unlock anything.
                if let Some(a) = self.cheevos.as_mut() {
                    a.reset();
                }
                // The history before the load belongs to another timeline.
                self.ex.rewind.clear();
                self.ex.verify.iter_mut().for_each(|v| v.hashes.clear());
                format!("Loaded {label}")
            }
            // The core may have put part of the state in place (its battery save among it) before it gave up
            // (SOL-02): `load_keeping_undo` went back to the moment just written as "Before load".
            slots::Loaded::Refused(why) => {
                println!("couldn't load {}: {why}", path.display());
                format!("Couldn't load {label}: {why}")
            }
        };
        self.hud.toast(msg);
    }

    /// A state of Earlier (`load earlier3 <time>`). `listed` is the file's time as the pause menu listed it: a state
    /// replaced since (by a pause's own capture, `timeline_quiet`) is not loaded in its place. What runs now becomes
    /// Before load, as for every Load (`load_state_file`).
    fn load_earlier(&mut self, name: &str, listed: Option<u64>) {
        use pocket_ui::timeline;
        match timeline::to_load(&self.states, name, listed) {
            Ok(e) => {
                let label = timeline::load_label(&e, pocket_ui::recents::now_secs());
                self.load_state_file(&e.path, &label);
            }
            Err(why) => self.hud.toast(why),
        }
    }

    /// Earlier (`pocket_ui::timeline`), once a refresh of play: counts the play, and every 5 minutes of it takes a
    /// state here when the serialize fits into the refresh (`Timeline::moment`, the cost guard); otherwise the next
    /// pause takes it (`timeline_quiet`), or a sleep's resume state is copied (`timeline_after_autosave`).
    fn timeline_play(&mut self) {
        if !self.ex.cfg.timeline || self.bench.is_some() {
            self.timeline.stop();
            return;
        }
        self.timeline.play(Instant::now());
        if !self.timeline.due() {
            return;
        }
        // ScummVM and the like: nothing to keep (asked only every 5 minutes).
        if !self.core.has_states() {
            self.timeline.skip();
            return;
        }
        let hw = self.gfx.as_ref().is_some_and(|g| g.hw.is_some());
        // What a serialize of this core costs, if run-ahead or rewind measured it already.
        let c = &self.ex.capture;
        let measured = self.ex.ahead.serialize_ms().or_else(|| (c.count > 0).then(|| (c.total_ms / c.count as f64) as f32));
        let period_ms = self.period.as_secs_f32() * 1000.0;
        let core = &self.core;
        if self.timeline.moment(hw, measured, || core.state_size(), period_ms) == pocket_ui::timeline::Moment::InPlay {
            self.timeline_capture(pocket_ui::timeline::Moment::InPlay);
        }
    }

    /// A state Earlier left for a pause is taken a second into the pause menu, once the menu has listed the states;
    /// not while a command waits (a Quit or a Load goes first).
    fn timeline_quiet(&mut self) {
        let menu_up = self.paused_at.is_some_and(|t| t.elapsed() >= pocket_ui::timeline::QUIET_AFTER);
        if menu_up && self.early.is_none() && !self.sleeping && self.ex.cfg.timeline && self.bench.is_none() && self.timeline.due() {
            self.timeline_capture(pocket_ui::timeline::Moment::Quiet);
        }
    }

    /// A sleep wrote the resume state: when Earlier waits for a state, that one is copied (no second serialize).
    fn timeline_after_autosave(&mut self) {
        let auto = slots::auto_path(&self.states);
        if self.ex.cfg.timeline && self.bench.is_none() && self.timeline.due() && auto.exists() {
            self.timeline.submit_copy(&self.states, pocket_ui::timeline::Kind::Every, &auto, &slots::preview_path(&auto));
        }
    }

    /// Takes a state for Earlier: the serialize here, the rest on the writer's thread.
    fn timeline_capture(&mut self, at: pocket_ui::timeline::Moment) {
        let t0 = Instant::now();
        let Some(state) = self.core.save_state() else {
            self.timeline.failed();
            return;
        };
        let ms = t0.elapsed().as_secs_f32() * 1000.0;
        // A 3D core leaves no frame in memory: read back from the GPU, in a pause only (never in play, see `moment`).
        let picture = match self.last_frame.clone() {
            Some(f) => Some(f),
            None if at == pocket_ui::timeline::Moment::Quiet => self.hw_snapshot(),
            None => None,
        };
        println!("earlier: state taken {} ({} KB, serialize {ms:.2} ms)", if at == pocket_ui::timeline::Moment::InPlay { "in play" } else { "in a pause" }, state.len() / 1024);
        self.timeline.submit_state(&self.states, state, picture, ms, at);
    }

    /// Makes `slot` the one for Save, Load and the hotkeys; remembered for the next session.
    fn set_slot(&mut self, slot: usize) {
        self.slot = slot;
        slots::set_current(&data_dir(), &self.video_src.0, &self.stem(), slot);
    }

    fn stem(&self) -> String {
        self.video_src.1.file_stem().and_then(|s| s.to_str()).unwrap_or("game").to_string()
    }

    fn hotkey(&mut self, action: Action) {
        match action {
            Action::Save => self.save_slot(self.slot),
            Action::Load => self.load_slot(self.slot),
            Action::SlotPrev | Action::SlotNext => {
                self.set_slot(slots::step(self.slot, if action == Action::SlotNext { 1 } else { -1 }));
                let state = if slots::state_path(&self.states, self.slot).exists() { "" } else { " (empty)" };
                self.hud.toast(format!("Slot {}{state}", self.slot + 1));
            }
            Action::ToggleFastForward => {
                self.ex.ff_on = !self.ex.ff_on;
                let speed = self.ex.cfg.fast_forward.label();
                println!("fast-forward {} ({speed})", if self.ex.ff_on { "on" } else { "off" });
            }
            Action::Screenshot => self.screenshot(),
            // Off → FPS → FPS + sensors → Pixel Path → Off, for this game until it quits or the option changes.
            Action::NextOverGame => {
                self.ex.over_game = self.ex.over_game.step(1);
                self.update_strip();
                self.hud.toast(format!("Over the game: {}", self.ex.over_game.label()));
            }
        }
    }

    /// HOME + A: the core's last frame, at its own size, to `pocket_ui::screenshot`'s folder. This thread copies the
    /// frame (a 3D core's is read back from the GPU first, as for a state's preview) and goes on; a thread of its own
    /// converts, encodes and writes it. The toast comes when the file is in place (`screenshot_results`).
    fn screenshot(&mut self) {
        use pocket_ui::screenshot::{self as shot, Failure, Source};
        let Some(frame) = self.last_frame.clone().or_else(|| self.hw_snapshot()) else {
            self.hud.toast(Failure::NoPicture.toast());
            return;
        };
        if self.shots.is_none() {
            let (tx, done) = std::sync::mpsc::channel();
            match shot::Saver::start(shot::dir(), move |outcome| {
                let _ = tx.send(outcome);
            }) {
                Ok(saver) => self.shots = Some(Shots { saver, done }),
                Err(e) => {
                    eprintln!("screenshot: no thread: {e}");
                    self.hud.toast(Failure::Other.toast());
                    return;
                }
            }
        }
        let req = shot::Request { system: self.video_src.0.clone(), game: self.stem(), stamp: shot::Stamp::now(), source: Source::Frame(frame) };
        if self.shots.as_ref().is_some_and(|s| !s.saver.submit(req)) {
            // The last one is still being written: this press is dropped, quietly.
            println!("screenshot: busy, press ignored");
        }
    }

    /// What the screenshot thread finished since the last frame: "Screenshot saved" with a quiet shutter (when
    /// Settings > Sounds is on), or why not.
    fn screenshot_results(&mut self) {
        use pocket_ui::screenshot::Outcome;
        let Some(shots) = self.shots.as_ref() else { return };
        for outcome in shots.done.try_iter().collect::<Vec<_>>() {
            match outcome {
                Outcome::Saved(_) => {
                    self.hud.toast("Screenshot saved");
                    let rate = self.core.av.timing.sample_rate;
                    if self.chime_gain > 0.0 && rate > 0.0 {
                        self.chime.start_samples(pocket_ui::ui_sound::synth_at(pocket_ui::ui_sound::Sound::Shutter, rate as u32), self.chime_gain);
                    }
                }
                Outcome::Failed(why) => self.hud.toast(why.toast()),
            }
        }
    }

    /// The settings of this console/game, read again after the menu saved or restored them: picture,
    /// extras, core options and controls.
    fn reload_settings(&mut self) {
        self.video = VideoSettings::load(&data_dir(), &self.video_src.0, &self.video_src.1);
        let settings = GameSettings::load(&data_dir(), &self.video_src.0, &self.video_src.1);
        self.mapping = Mapping::from_values(&self.video_src.0, &settings.values);
        let mut saved = core_options::saved_from(&settings.values);
        let above: Vec<(&str, &str)> = saved.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let ra = retroarch_beneath(&self.ra_options, &above);
        saved.extend(ra);
        self.core.reload_options(&saved);
        self.ex.apply_settings(pocket_ui::extras::Settings::from_values(&settings.effective()));
        self.write_snapshot();
    }

    /// Runs commands from pocket-ui between frames; returns how to quit when one says so.
    fn handle_commands(&mut self) -> Option<Exit> {
        let cmds: Vec<String> = self.early.take().into_iter().chain(self.commands.try_iter()).collect();
        // Loads, options, cheats, the pause: the game may have changed from outside (`RunAhead::forget`). Pixel Path's
        // requests only read.
        if cmds.iter().any(|c| !c.starts_with(pocket_ui::pixel_path::COMMAND)) {
            self.ex.ahead.forget();
        }
        // Only the newest preview request is worth drawing; it runs after the settings before it.
        let mut preview = None;
        for cmd in cmds {
            let cmd = cmd.trim();
            let words: Vec<&str> = cmd.split_whitespace().collect();
            // Slots count from 1 on the wire, as on screen.
            let slot_arg = |i: usize| words.get(i).and_then(|n| n.parse::<usize>().ok()).and_then(|n| n.checked_sub(1)).filter(|n| *n < slots::SLOTS);
            // Everything after the key, spaces included (core option values can have them).
            let rest = |prefix: &str| cmd.strip_prefix(prefix).and_then(|r| r.split_once(' ')).map(|(k, v)| (k, v.trim()));
            match words.as_slice() {
                ["pause"] if !self.paused => {
                    self.paused = true;
                    self.paused_at = Some(Instant::now());
                    self.timeline.stop();
                    // No more frames from here: pocket-ui may cover us now (`fast_switch::PAUSED`).
                    pocket_ui::fast_switch::announce_paused();
                    // The pause menu lists the achievements: their file is written now.
                    if let Some(a) = self.cheevos.as_ref() {
                        a.menu_opened();
                    }
                    // The Disc row: only when the core's answer changed (after the menu is free to show).
                    self.publish_discs();
                    pocket_ui::rumble::stop();
                    if let Some(a) = &self.audio {
                        a.pause(true);
                    }
                    println!("play paused");
                }
                // Paused already (or asleep): it draws nothing, so it says so again.
                ["pause"] => pocket_ui::fast_switch::announce_paused(),
                ["resume"] if self.paused => {
                    self.paused = false;
                    self.paused_at = None;
                    self.after_pause();
                    println!("play resumed");
                }
                ["sleep"] if !self.sleeping => {
                    self.sleeping = true;
                    self.timeline.stop();
                    if let Some(a) = &self.audio {
                        a.pause(true);
                    }
                    println!("asleep");
                }
                ["screen", "off"] if !self.dark => {
                    self.dark = true;
                    println!("screen off, playing on");
                }
                // Also sent after a suspend pocket-ui didn't start itself: the quantum is reset either way.
                ["wake"] => {
                    if let Some(a) = &self.audio {
                        a.reassert_quantum();
                    }
                    // The screen is back: the button that woke it reaches the game only once it was let go.
                    if std::mem::take(&mut self.dark) {
                        self.held_mask = std::array::from_fn(|p| self.input.buttons(p));
                        println!("screen on");
                    }
                    if std::mem::take(&mut self.sleeping) {
                        self.after_pause();
                        self.woke_frame = !self.paused;
                        pocket_ui::trace::step("core: awake");
                        println!("awake{}", if self.paused { " (still paused)" } else { "" });
                    }
                }
                ["autosave"] => {
                    self.save_auto();
                    self.timeline_after_autosave();
                }
                // The pause menu's Pixel Path page asks a few times a second while it is open.
                ["pixelpath", seq] => self.write_pixel_path(seq.parse().unwrap_or(0)),
                // The pause menu's background, right after "pause" (`pocket_ui::preview::SHOT_FILE`).
                ["shot", seq] => self.write_shot(seq.parse().unwrap_or(0)),
                ["preview", ..] => {
                    let n: Vec<u32> = words[1..].iter().filter_map(|v| v.parse().ok()).collect();
                    if let [w, h, more @ ..] = &n[..] {
                        preview = Some((*w, *h, more.first().copied().unwrap_or(0)));
                    }
                }
                ["save", ..] => self.save_slot(slot_arg(1).unwrap_or(self.slot)),
                ["load", name @ ("auto" | "undo")] => self.load_named(name),
                // "load earlier3 <time>": a state of Earlier as the menu listed it (`pocket_ui::timeline`).
                ["load", name, listed @ ..] if pocket_ui::timeline::Name::parse(name).is_some() => {
                    self.load_earlier(name, listed.first().and_then(|t| t.parse().ok()));
                }
                ["load", ..] => self.load_slot(slot_arg(1).unwrap_or(self.slot)),
                ["slot", ..] => {
                    if let Some(n) = slot_arg(1) {
                        self.set_slot(n);
                    }
                }
                ["reload"] => self.reload_settings(),
                // "players": the pause menu's Players page wrote a new order (`players::COMMAND`).
                [players::COMMAND] => self.input.reorder(players::Order::load(&data_dir()).order),
                ["cheats"] => {
                    let n = pocket_ui::cheats::apply(&self.core, &data_dir(), &self.video_src.0, &self.video_src.1);
                    println!("cheats: {n} on");
                    self.pixel_live.cheats = n;
                    self.hud.toast(match n {
                        0 => "Cheats off".to_string(),
                        1 => "1 cheat on".to_string(),
                        n => format!("{n} cheats on"),
                    });
                }
                // "look <warmth> <contrast> [<night light kelvin>]": the colour temperature changed (Select + Vol), or a
                // step of the night light's fade (0 or no number: it is off).
                ["look", w, c, rest @ ..] => {
                    let night = rest.first().and_then(|n| n.parse().ok()).unwrap_or(0);
                    let look = pocket_ui::display::Look { warmth: w.parse().unwrap_or(0), contrast: c.parse().unwrap_or(0), soft: true, night }.clamped();
                    self.hud.set_look(&look);
                }
                // "note <sentence>": a line of pocket-ui's at the bottom for a few seconds (after a switch-off after
                // sleeping, `pocket_ui::auto_off`).
                ["note", ..] => {
                    if let Some(text) = cmd.strip_prefix("note ") {
                        self.hud.note(text.trim());
                    }
                }
                // "osd Brightness|60 %|0.600|0": a level bar over the picture.
                ["osd", ..] => {
                    if let Some(wire) = cmd.strip_prefix("osd ") {
                        self.hud.osd(wire.trim());
                    }
                }
                // "disc next", or "disc <n>" (1-based) from the pause menu's Disc row (`pocket_ui::disc`).
                ["disc", which] => {
                    let to = match *which {
                        "next" => self.core.next_disc(),
                        n => n.parse::<u32>().ok().filter(|n| *n >= 1 && self.core.set_disc(n - 1)),
                    };
                    match (to, self.core.disc()) {
                        (Some(n), Some((_, count))) => {
                            println!("disc {n}");
                            self.hud.toast(format!("Disc {n} of {count}"));
                        }
                        _ => {
                            eprintln!("disc {which}: refused");
                            self.hud.toast("Couldn't change the disc".to_string());
                        }
                    }
                    self.publish_discs();
                }
                // Test input, only with POCKET_CORE_TEST=1: "press A 150" holds a button for 150 ms,
                // "axis lx 0.8" holds a stick position (lx ly rx ry, -1..1; 0 lets go).
                ["press", name, ..] if std::env::var_os("POCKET_CORE_TEST").is_some() => {
                    let ms: u64 = words.get(2).and_then(|m| m.parse().ok()).unwrap_or(100);
                    let names = ["B", "Y", "SELECT", "START", "UP", "DOWN", "LEFT", "RIGHT", "A", "X", "L", "R", "L2", "R2", "L3", "R3"];
                    if let Some(bit) = names.iter().position(|n| n.eq_ignore_ascii_case(name)) {
                        self.input.inject(1 << bit, true);
                        self.inject_release.push((1 << bit, Instant::now() + Duration::from_millis(ms)));
                    }
                }
                // "achievement-demo": an unlock card and its sound as a real unlock shows them (screenshots, sound checks).
                ["achievement-demo"] if std::env::var_os("POCKET_CORE_TEST").is_some() => {
                    self.hud.achievement(1, "Demo: First steps into the dungeon", 10, "");
                }
                // "peek <hex offset>": one byte of the core's work RAM, printed.
                ["peek", at] if std::env::var_os("POCKET_CORE_TEST").is_some() => {
                    let at = usize::from_str_radix(at.trim_start_matches("0x"), 16).unwrap_or(0);
                    println!("peek {at:#x} = {:?}", self.core.peek_ram(at));
                }
                ["axis", name, value] if std::env::var_os("POCKET_CORE_TEST").is_some() => {
                    use pocket_ui::pad::Axis;
                    let axis = match *name {
                        "lx" => Some(Axis::LeftX),
                        "ly" => Some(Axis::LeftY),
                        "rx" => Some(Axis::RightX),
                        "ry" => Some(Axis::RightY),
                        _ => None,
                    };
                    if let (Some(axis), Ok(v)) = (axis, value.parse::<f32>()) {
                        self.input.sticks(0).set(axis, v);
                    }
                }
                // "opt <key> <value>": a core option changed in the pause menu's Emulator page.
                ["opt", ..] => {
                    if let Some((key, value)) = rest("opt ") {
                        self.core.set_option(key, value);
                    }
                }
                // "ctl <button> <console button|none>" or "ctl stick_dpad on|off": the Controls page.
                ["ctl", ..] => {
                    if let Some((key, value)) = rest("ctl ") {
                        if self.mapping.set(key, value) {
                            self.write_snapshot();
                        }
                    }
                }
                // "set <key> <value>": a change in the pause menu, applied before it is saved.
                ["set", key, value] => {
                    if !self.video.set(key, value) {
                        self.ex.set(key, value);
                    }
                }
                ["quit", ..] => {
                    if let Some(exit) = Exit::from_command(&words) {
                        return Some(exit);
                    }
                }
                _ => {}
            }
        }
        if let Some((w, h, seq)) = preview {
            self.preview(w, h, seq);
        }
        None
    }

    /// Renders the last frame as the screen would show it with the current settings, and writes the
    /// centre at 1:1 plus a whole-screen overview for pocket-ui's pause menu.
    fn preview(&mut self, w: u32, h: u32, seq: u32) {
        let t0 = Instant::now();
        let (sw, sh) = if self.view.0 > 0 { self.view } else { pocket_ui::device::current().screen() };
        let aspect = Self::display_aspect(&self.core, &self.video_src.0);
        let Some(gfx) = self.gfx.as_mut() else { return };
        // A 3D core's context is current between frames; the preview is drawn with ours, from the
        // frame its last picture was copied into.
        if gfx.hw.is_some() {
            gfx.context.make_current(&gfx.surface).ok();
        }
        let rgba = gfx.video.render_offscreen(sw, sh, aspect, &self.video);
        if let Some(hw) = gfx.hw.as_ref() {
            hw.make_current(&gfx.surface);
        }
        let rendered = t0.elapsed();
        let (crop, overview) = pocket_ui::preview::from_screen(&rgba, sw, sh, w, h);
        if let Err(e) = pocket_ui::preview::write(Path::new(pocket_ui::preview::FILE), seq, &[&crop, &overview]) {
            eprintln!("preview: {e}");
        }
        println!(
            "preview {seq}: {}x{} + {}x{} in {:.1} ms (render and read back {:.1} ms)",
            crop.w,
            crop.h,
            overview.w,
            overview.h,
            t0.elapsed().as_secs_f32() * 1000.0,
            rendered.as_secs_f32() * 1000.0
        );
    }

    /// The last frame as the screen shows it, whole and full size, for the pause menu's background
    /// (`pocket_ui::preview::SHOT_FILE`): pocket-ui doesn't wait for a screenshot before it shows the menu.
    fn write_shot(&mut self, seq: u32) {
        let t0 = Instant::now();
        let (sw, sh) = if self.view.0 > 0 { self.view } else { pocket_ui::device::current().screen() };
        let aspect = Self::display_aspect(&self.core, &self.video_src.0);
        let Some(gfx) = self.gfx.as_mut() else { return };
        if gfx.hw.is_some() {
            gfx.context.make_current(&gfx.surface).ok();
        }
        let rgba = gfx.video.render_offscreen(sw, sh, aspect, &self.video);
        if let Some(hw) = gfx.hw.as_ref() {
            hw.make_current(&gfx.surface);
        }
        let shot = pocket_ui::preview::shot_from_screen(&rgba, sw, sh);
        if let Err(e) = pocket_ui::preview::write(Path::new(pocket_ui::preview::SHOT_FILE), seq, &[&shot]) {
            eprintln!("shot: {e}");
        }
        pocket_ui::trace::step("menu shot written");
        println!("menu shot {seq}: {sw}x{sh} in {:.1} ms", t0.elapsed().as_secs_f32() * 1000.0);
    }

    /// The pause menu's Disc row (`pocket_ui::disc`): what the core's disk control says (the disc in the drive, how many),
    /// written to `disc::FILE` when it differs from the last time. A core without disc control writes nothing.
    fn publish_discs(&mut self) {
        let now = self.core.disc().map(|(at, count)| pocket_ui::disc::Discs::from_core(at, count));
        if now != self.discs {
            match now {
                Some(d) => d.write(&self.video_src.1),
                None => pocket_ui::disc::forget(&self.video_src.1),
            }
            self.discs = now;
        }
    }

    /// Back from the pause menu or from sleep: sound and frames go on unless the other still holds them.
    fn after_pause(&mut self) {
        if let Some(a) = &self.audio {
            a.pause(self.paused || self.sleeping);
        }
        // The button that ended the pause (or woke the device) reaches the game only after it was let go.
        self.held_mask = std::array::from_fn(|p| self.input.buttons(p));
        self.touch.after_pause();
        // The pause is no late frame: restart pacing and the frame-time statistics. Nor is it play (Earlier).
        self.timeline.stop();
        self.next_frame = None;
        self.stats.last_swap = None;
        self.hud.restart();
    }

    fn should_exit(&mut self) -> bool {
        // The pad still reaches us behind the pause menu; the quit combo is for play only, on one controller.
        let combo = !self.paused && !self.sleeping && !self.dark && self.input.one_player_holds((1 << joypad::SELECT) | (1 << joypad::START));
        self.exit_since = if combo { self.exit_since.or(Some(Instant::now())) } else { None };
        TERMINATE.load(Ordering::SeqCst)
            || self.exit_since.is_some_and(|t| t.elapsed() >= Duration::from_secs(1))
            || self.bench.is_some_and(|d| self.stats.started.elapsed() >= d)
    }
}

/// How a quit ends the session (`App::handle_commands`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Exit {
    /// The exit save: battery save and auto-resume state (SELECT + START, a closed window, SIGTERM, `quit`).
    Save,
    /// `quit nosave`: the battery save only.
    WithoutSaving,
    /// `quit handoff` (`fast_switch::QUIT_FOR_SWITCH`): `Save`, then pocket-ui hears when it is done.
    Handoff,
}

impl Exit {
    /// `quit` and `quit nosave` (`control::QUIT_WITHOUT_SAVING`); anything else is no quit.
    fn from_command(words: &[&str]) -> Option<Exit> {
        match words {
            ["quit"] => Some(Exit::Save),
            ["quit", "nosave"] => Some(Exit::WithoutSaving),
            ["quit", "handoff"] => Some(Exit::Handoff),
            _ => None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_none() {
            pocket_ui::startup::mark("loop");
            self.create_gfx(el);
            pocket_ui::startup::mark("graphics");
            if let Some(opening) = self.audio_opening.take() {
                self.audio = opening.join().ok().flatten();
                if self.audio.is_none() {
                    eprintln!("no audio output");
                }
            }
            pocket_ui::startup::mark("audio");
            let mhz = self.window.as_ref().and_then(|w| w.current_monitor()).and_then(|m| m.refresh_rate_millihertz()).unwrap_or(60_000);
            self.period = Duration::from_secs_f64(1000.0 / mhz as f64);
            println!("display {:.3} Hz", mhz as f64 / 1000.0);
            self.sync_audio_rate();
            self.stats.started = Instant::now();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested = event {
            pocket_ui::fast_switch::mark("quit received (window closed)");
            self.save_exit();
            pocket_ui::fast_switch::mark("exit save done");
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // Quitting (the window's close request): no more waiting for commands, the loop is about to end.
        if self.gfx.is_none() || el.exiting() {
            return;
        }
        if self.paused || self.sleeping {
            // Nothing runs: wait for pocket-ui's next command. The timeout keeps window events and
            // SIGTERM flowing.
            if let Ok(cmd) = self.commands.recv_timeout(Duration::from_millis(30)) {
                self.early = Some(cmd);
            }
            // Achievements' timers (an unlock being sent again, the two-minute ping) go on in the pause.
            if let Some(a) = self.cheevos.as_mut() {
                a.idle();
            }
            self.timeline_quiet();
        }
        let exit = if self.should_exit() { Some(Exit::Save) } else { self.handle_commands() };
        if let Some(exit) = exit {
            // Fade out now: the exit save stalls for a while (16-40 MB states of 3D cores), which
            // would otherwise cut the sound off mid-sample.
            if let Some(a) = &self.audio {
                a.pause(true);
            }
            pocket_ui::fast_switch::mark("quit received");
            match exit {
                _ if self.bench.is_some() => {}
                Exit::Save => self.save_exit(),
                Exit::WithoutSaving => self.exit_without_saving(),
                // The switcher: the same exit save; the hand-over line follows once the system is back (`main`).
                Exit::Handoff => {
                    self.save_exit();
                    pocket_ui::fast_switch::request();
                }
            }
            pocket_ui::fast_switch::mark("exit save done");
            el.exit();
            return;
        }
        if !self.paused && !self.sleeping {
            self.step();
        }
    }
}

/// 3D cores run with the options RetroArch's setup gave them (resolution factor, renderer …), the
/// lowest layer: our curated defaults were already left out of `ra` (see `main`), and so are the
/// keys `above` sets (the user's saved values, the system's required options).
fn retroarch_beneath(ra: &[(String, String)], above: &[(&str, &str)]) -> Vec<(String, String)> {
    ra.iter().filter(|(k, _)| !above.iter().any(|(a, _)| a == k)).cloned().collect()
}

/// `pocket-core --dump-options <core.so>`: what a core declares, with our curated defaults marked. For curating new cores.
fn dump_options(core: &str) {
    let set = match Core::probe_options(Path::new(core)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("# {} options, {} categories", set.options.len(), set.categories.len());
    for c in &set.categories {
        println!("category {}: {} - {}", c.key, c.label, c.info.replace('\n', " "));
    }
    for o in &set.options {
        let values: Vec<String> = o.values.iter().map(|(v, l)| if v == l { v.clone() } else { format!("{v}={l}") }).collect();
        let cat = o.category.as_deref().map(|c| format!(" [{c}]")).unwrap_or_default();
        println!("{} = {}{}{}{}\n    {}  ({})\n    {}", o.key, o.default, if o.hidden { "  (hidden)" } else { "" }, cat, "", o.label, values.join(" | "), o.info.replace('\n', " "));
    }
}

/// `--frame-demo` and `--export-frame` (see the top of this file). Returns false for other arguments.
fn frame_tools(args: &[String]) -> bool {
    use pocket_ui::bezel;
    use pocket_ui::video::{dest_rect, FrameMode};
    // Display aspect of the consoles whose pixels aren't square, as their cores report it.
    let aspect = |system: &str| if matches!(system, "nes" | "snes" | "famicom" | "sfc" | "megadrive" | "genesis" | "mastersystem" | "pcengine" | "atari2600" | "atari7800") { 4.0 / 3.0 } else { 0.0 };
    let (view_w, view_h) = (1240, 1080);
    // Auto's built-in style unless another frame is asked for (Ambient is drawn live only).
    let frame = |system: &str, v: &VideoSettings, w: u32, h: u32| {
        let rect = dest_rect(v.scaling, view_w, view_h, w, h, aspect(system));
        let style = v.fixed_style(system).unwrap_or(bezel::style_for(system));
        let b = (v.shows_frame() && v.frame != FrameMode::Ambient).then(|| bezel::generate(style, view_w, view_h, rect, rect.h as f32 / h as f32)).flatten();
        (rect, b)
    };
    match args.get(1).map(String::as_str) {
        Some("--frame-demo") if args.len() >= 5 => {
            let (system, out) = (&args[2], &args[4]);
            let Some(pic) = pocket_ui::image::load_png(Path::new(&args[3])) else {
                eprintln!("can't read {}", args[3]);
                std::process::exit(1);
            };
            // [scaling] [frame] [shell colour or TV cabinet]
            let mut v = VideoSettings { frame: FrameMode::Auto, ..Default::default() };
            v.set("scaling", args.get(5).map_or("native", String::as_str));
            v.set("frame", args.get(6).map_or("auto", String::as_str));
            if let Some(sub) = args.get(7) {
                for key in ["frame_colour", "frame_tv", "frame_brick", "frame_pocket", "frame_clamshell", "frame_swan", "frame_watch"] {
                    v.set(key, sub);
                }
            }
            let t0 = Instant::now();
            let (rect, mut b) = frame(system, &v, pic.w, pic.h);
            println!("frame drawn in {:.1} ms", t0.elapsed().as_secs_f32() * 1000.0);
            // FRAME_DEMO_LIGHT=<battery %>[,flash|,off] shows the power light as it would be then.
            if let (Some(b), Ok(spec)) = (b.as_mut(), std::env::var("FRAME_DEMO_LIGHT")) {
                use pocket_ui::frame_led;
                let mut parts = spec.split(',');
                let capacity = parts.next().and_then(|c| c.parse().ok());
                let rest = parts.next().unwrap_or("");
                let mut look = frame_led::look_at(capacity, 0, (rest == "flash").then_some(0));
                look.lit &= rest != "off";
                if let Some(led) = b.led.clone() {
                    let patch = frame_led::paint(&led, look, b.hole);
                    frame_led::blit(&mut b.rgba, b.w, led.rect, &patch);
                }
            }
            let rgba = bezel::compose(view_w, view_h, rect, b.as_ref(), pic.w, pic.h, &pic.px);
            let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out).expect("create output")), view_w, view_h);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().and_then(|mut w| w.write_image_data(&rgb)).expect("write png");
            println!("{system}: picture {rect:?}, frame {}", if b.is_some() { "drawn" } else { "none (no room)" });
            true
        }
        Some("--export-frame") if args.len() >= 6 => {
            let system = &args[2];
            let (Ok(w), Ok(h)) = (args[3].parse(), args[4].parse()) else { return false };
            let (rect, b) = frame(system, &VideoSettings { frame: FrameMode::Auto, ..Default::default() }, w, h);
            let Some(b) = b else {
                eprintln!("{system}: no room for a frame at {rect:?}");
                std::process::exit(1);
            };
            bezel::write_pack(Path::new(&args[5]), &format!("pocket-ui {system}"), system, &b).expect("write pack");
            println!("{system}: pack in {}, hole {rect:?}", args[5]);
            true
        }
        _ => false,
    }
}

/// True when a pocket-core other than this process is running (looks at /proc/<pid>/comm).
fn another_pocket_core_runs() -> bool {
    let me = std::process::id().to_string();
    std::fs::read_dir("/proc").into_iter().flatten().flatten().any(|e| {
        let pid = e.file_name().to_string_lossy().into_owned();
        pid != me && pid.bytes().all(|b| b.is_ascii_digit()) && std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "pocket-core")
    })
}

/// Where the reason a game could not start is left for pocket-ui to show.
const ERROR_FILE: &str = pocket_ui::notice::ERROR_FILE;
const BIOS_DIR: &str = "/storage/roms/bios";

/// Reports why the game cannot start (log and ERROR_FILE) and exits.
fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    let _ = std::fs::write(ERROR_FILE, msg);
    std::process::exit(1);
}

/// When main began, and whether the first frame has been shown: for the start time line in the log.
static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static FIRST_SWAP: AtomicBool = AtomicBool::new(false);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("pocket-core {}", pocket_ui::update::app_version());
        return;
    }
    // The launcher's logger for logs/ on the card (src/logs.rs); it runs under the name pocket-logd.
    if args.get(1).map(String::as_str) == Some("--log-daemon") {
        std::process::exit(pocket_ui::logs::run_daemon());
    }
    if args.get(1).map(String::as_str) == Some("--rumble-test") {
        std::process::exit(pocket_ui::rumble::selftest(&args[2..]));
    }
    if args.len() == 3 && args[1] == "--dump-options" {
        dump_options(&args[2]);
        return;
    }
    if frame_tools(&args) {
        return;
    }
    // pocket-ui runs this after a game ends, in case pocket-core was killed before it could put the CPU
    // and fan settings back; and at its start and the launcher after it ended, for pocket-ui's own fan profile
    // outside games (Settings > Performance & fan), which it only keeps while no game runs.
    if args.get(1).map(String::as_str) == Some("--restore-system") {
        if Power::with_root("/", pocket_ui::power::OUTSIDE_RESTORE_FILE).recover() {
            println!("restored the fan profile pocket-ui left behind outside games");
        }
        // A patched copy of a game a killed run left in /tmp, which is RAM (`softpatch`); a running game's stays.
        pocket_ui::softpatch::clean_stale(&std::env::temp_dir());
        // A PortMaster port's performance a crashed pocket-ui left behind, unless that port still runs (`port_perf`).
        if pocket_ui::port_perf::recover_now() {
            println!("restored the CPU settings a port's run left behind");
        }
        // A game that is running right now still needs its settings: it holds the CPU's lease (`power::Lease`).
        let lease = pocket_ui::power::Lease::take(std::path::Path::new(pocket_ui::power::LEASE_FILE), Duration::from_millis(200), || false);
        if lease.is_none() || another_pocket_core_runs() {
            return;
        }
        if Power::system().recover() {
            println!("restored the CPU and fan settings a previous run left behind");
        }
        return;
    }
    if args.len() < 3 {
        eprintln!("usage: pocket-core <core.so> <rom> [--system NAME] [--bench SECONDS] [--rotate N] [--run-ahead N]");
        std::process::exit(2);
    }
    // Fast switching (`fast_switch`): the start in the log, and the pipe for the hand-over; before any thread starts.
    pocket_ui::fast_switch::process_start();
    let started = Instant::now();
    // Speed guard: the game's process has started (POCKETUI_TRACE, docs/LATENCY.md).
    pocket_ui::mark!("core-start");
    let _ = std::fs::remove_file(ERROR_FILE);
    // A Disc row file this game left in a run that crashed (`publish_discs` writes it anew).
    pocket_ui::disc::forget(Path::new(&args[2]));
    // A panic aborts (Cargo.toml); the hook still leaves crash.log and the reason for pocket-ui to show.
    pocket_ui::crash::install("pocket-core", data_dir(), Some(PathBuf::from(ERROR_FILE)));
    // Settings → Language for the overlay's notes (`i18n`; one field of settings.json).
    pocket_ui::i18n::set(pocket_ui::settings::language(&data_dir()));
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));
    let bench = arg("--bench").and_then(|s| s.parse().ok()).map(Duration::from_secs);
    unsafe {
        libc::signal(libc::SIGTERM, on_sigterm as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }

    let rom = PathBuf::from(&args[2]);
    // The ROCKNIX system name (pocket-ui passes it); otherwise the ROM's folder, which is the same for most.
    let system = arg("--system").cloned().or_else(|| rom.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).map(str::to_string)).unwrap_or_else(|| "misc".into());
    let host = hostsys::find(&system);
    if let Some(msg) = host.and_then(|h| h.missing_bios(Path::new(BIOS_DIR))) {
        fail(&msg);
    }
    let save_dir = rom.parent().map_or_else(|| PathBuf::from("/storage/roms/misc"), Path::to_path_buf);
    let settings = GameSettings::load(&data_dir(), &system, &rom);
    let saved = core_options::saved_from(&settings.values);
    // The user's saved options first (the first match wins), then the ones this system needs.
    let mut options: Vec<(&str, &str)> = saved.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    options.extend_from_slice(host.map_or(&[][..], |h| h.options));
    // 3D cores: RetroArch's options beneath all of these (see `retroarch_beneath`).
    let core_so = Path::new(&args[1]);
    // Mesa's threaded context (a driver thread per GL context) costs 3D cores more than it saves here:
    // Flycast, Sonic Adventure 2 89 % → 69 % CPU, PPSSPP 20-26 % → 18 %, Mupen level, frames as
    // steady (docs/HOST.md). Set before any GL context exists; no other thread runs yet.
    if hw_render::is_3d_core(core_so) && std::env::var_os("GALLIUM_THREAD").is_none() {
        std::env::set_var("GALLIUM_THREAD", "0");
    }
    // Dreamcast: one memory card per game, the same file the standalone Flycast uses (`vfs`).
    let dc_cards = Path::new(BIOS_DIR).join("dc");
    if core_options::core_id(core_so) == "flycast" && dc_cards.is_dir() {
        pocket_ui::vfs::share_flycast_vmus(&dc_cards);
    }
    let ra_options: Vec<(String, String)> = if hw_render::is_3d_core(core_so) {
        let curated = core_options::curation(&core_options::core_id(core_so)).defaults;
        hw_render::retroarch_options(core_so).into_iter().filter(|(k, _)| !curated.iter().any(|(c, _)| c == k)).collect()
    } else {
        Vec::new()
    };
    let ra = retroarch_beneath(&ra_options, &options);
    options.extend(ra.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    // ScummVM's ROCKNIX list names the game folder rather than being a game (`hostsys::content_path`).
    let content = hostsys::content_path(&system, &rom);
    if core_options::core_id(core_so) == "scummvm" {
        hostsys::prepare_scummvm(Path::new(BIOS_DIR));
    }
    let mut core = match Core::load(core_so, &content, Path::new(BIOS_DIR), &save_dir, &options) {
        Ok(c) => c,
        Err(e) => fail(&e),
    };
    pocket_ui::startup::mark("core");
    // A note of the game before about saves that didn't reach the card is not this game's (`save_trouble`).
    pocket_ui::save_trouble::publish_to(Path::new(pocket_ui::save_trouble::FILE), None);
    // A DraStic `.dsv` next to the game became the `.sav` (`dsv`): the toast comes once the window is up.
    let mut took_dsv = false;
    if let Some(h) = host {
        took_dsv = core.use_save_extension(h.save_ext);
        // ScummVM has no states, TIC-80's don't hold the game: no slots, resume or rewind (`hostsys`).
        if h.no_states.is_some() {
            core.disable_states();
        }
        if h.states_vary {
            core.states_vary_in_size();
        }
    }
    // GBA and the other HOST_CORES systems have no `HostSystem`: the core decides (`hostsys::core_states_vary`).
    if host.is_none_or(|h| !h.states_vary) && hostsys::core_states_vary(&core_options::core_id(core_so)) {
        core.states_vary_in_size();
    }
    if host.is_some_and(|h| h.analog) {
        println!("player 1: {}", core.use_analog_pad());
    }
    // Players 2 to 4 get a pad in the ports the core lists, as RetroArch plugs them in (couch co-op, `players`).
    for line in core.plug_other_players(host.is_some_and(|h| h.analog)) {
        println!("{line}");
    }
    println!(
        "{}: {}x{} aspect {:.3} @ {:.4} fps, audio {} Hz",
        core.name, core.av.geometry.base_width, core.av.geometry.base_height, core.av.geometry.aspect_ratio, core.av.timing.fps, core.av.timing.sample_rate
    );

    // Settings of this console/game from the pause menu; a crashed run's CPU/fan changes come back first.
    let data = data_dir();
    let values = GameSettings::load(&data, &system, &rom).effective();
    // The CPU is this game's only once the game before (a port's session, a handed-over pocket-core) has put it back:
    // its caps would otherwise be noted as the system's (`power::Lease`, review SOL 2026-10-24 #1). Held until the
    // restore at the end; uncontended it is one `flock`.
    let perf_lease = pocket_ui::power::Lease::for_game("pocket-core");
    Power::system().recover();
    let mut ex = Extras::new(&values, Power::system());
    // Run-ahead looks at the core's RetroArch info file and the system when it is switched on (`run_ahead`).
    ex.ahead.set_core(core_so, &system);
    if let Some(n) = arg("--run-ahead") {
        ex.set("run_ahead", n);
    }

    // Who is which player: the order from the Players page, then everyone connected (`players`).
    let input = Arc::new(Inputs::new(players::Order::load(&data).order, Some(PathBuf::from(players::SEATS_FILE))));
    // The pads, the touch screen and the command FIFO are opened on a thread (instant start: walking the input devices
    // took 17-20 ms in the virtual device); the window and the core don't wait for them.
    #[cfg(target_os = "linux")]
    let inputs = std::thread::spawn({
        // Pads that appear later (Bluetooth after a deep sleep, InputPlumber restarted) are picked up by the supervisor.
        let input = input.clone();
        let hotkeys = ex.hotkeys.clone();
        move || {
            let feed = input.clone();
            let names = pocket_ui::pad::linux::supervise(move |e| {
                // Each player's input (`players::Inputs`); the HOME + button shortcuts listen to every controller
                // together. HOME itself and the triggers never reach the game.
                feed.feed(e, |merged| hotkeys.lock().unwrap().feed(merged));
                true
            });
            // Everyone found at the start is seated by the order; later ones are announced with a toast.
            input.arrange();
            names
        }
    });

    // The touch screen, for cores that read a pointer (the Nintendo DS's stylus).
    let touch = Arc::new(pocket_ui::touch::Shared::default());
    #[cfg(target_os = "linux")]
    {
        let touch = touch.clone();
        std::thread::spawn(move || {
            // After the pads: both walk /dev/input, one at a time is enough.
            let _ = inputs.join();
            if let Some((_, dev)) = pocket_ui::touch::linux::open() {
                pocket_ui::touch::linux::pump(dev, true, move |e| {
                    touch.feed(&e);
                    true
                })
            }
        });
    }
    pocket_ui::startup::mark("input");
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("game").to_string();
    let states = PathBuf::from(format!("/storage/roms/savestates/{system}/{stem}"));

    let (cmd_tx, commands) = std::sync::mpsc::channel();
    #[cfg(target_os = "linux")]
    {
        const FIFO: &str = "/tmp/pocket-core.cmd";
        let _ = std::fs::remove_file(FIFO);
        // mkfifo(3) rather than the mkfifo program: no process to start before the first frame.
        // SAFETY: the path is a valid C string that lives through the call.
        let made = std::ffi::CString::new(FIFO).is_ok_and(|c| unsafe { libc::mkfifo(c.as_ptr(), 0o644) } == 0);
        if made {
            // Read-write: we count as a writer ourselves, so the FIFO never hits EOF and always has a
            // reader, and pocket-ui's writes never block.
            std::thread::spawn(move || {
                use std::io::BufRead;
                let Ok(f) = std::fs::OpenOptions::new().read(true).write(true).open(FIFO) else { return };
                for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
                    if cmd_tx.send(line).is_err() {
                        return;
                    }
                }
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    drop(cmd_tx);

    pocket_ui::startup::mark("fifo");
    // The sound device is opened while the window and GL come up (`resumed` takes it): connecting to PipeWire took
    // 13-18 ms in the virtual device.
    let audio_opening = {
        let (rate, heavy) = (core.av.timing.sample_rate, hw_render::is_3d_core(core_so));
        std::thread::spawn(move || Audio::open(rate, heavy))
    };
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        core,
        window: None,
        gfx: None,
        audio: None,
        audio_opening: Some(audio_opening),
        shown_at: None,
        input,
        touch,
        mapping: Mapping::from_values(&system, &settings.values),
        stick_pad: Default::default(),
        exit_since: None,
        frames: 0,
        stats: Stats { emulated: 0, started: Instant::now(), last_swap: None, intervals: Vec::new(), run_ms: Vec::new(), draw_ms: Vec::new(), strip_us: Vec::new() },
        bench,
        period: Duration::from_micros(16_667),
        next_frame: None,
        last_frame: None,
        states,
        commands,
        video: VideoSettings::load(&data, &system, &rom),
        slot: slots::current(&data, &system, &stem),
        video_src: (system, rom),
        paused: false,
        sleeping: false,
        dark: false,
        hold: pocket_ui::startup::held(),
        crt_on: pocket_ui::startup::held() && pocket_ui::crt_on::wanted(),
        crt_started: false,
        woke_frame: false,
        held_mask: [0; players::MAX],
        view: (0, 0),
        early: None,
        ex,
        hud: Hud::default(),
        nominal_samples: 1600,
        time_driven: false,
        owed: 0.0,
        audio_peak: 0,
        rotate_override: arg("--rotate").and_then(|r| r.parse().ok()),
        inject_release: Vec::new(),
        resume_in: None,
        ra_options,
        cheevos: None,
        chime: Default::default(),
        shots: None,
        chime_gain: {
            let sounds = pocket_ui::ui_sound::Config::load(&data);
            if sounds.on { sounds.gain() } else { 0.0 }
        },
        pixel_window: Default::default(),
        pixel_live: Default::default(),
        discs: None,
        timeline: Default::default(),
        paused_at: None,
    };
    // RetroAchievements (an account signed in, a console that has them): returns at once, works on its own threads.
    if bench.is_none() {
        app.cheevos = pocket_ui::achievements::game::start(&data, &app.video_src.0, &app.video_src.1, &app.core);
    }
    // Instant resume: continue exactly where the last session ended (not in bench runs). A 3D core
    // gets its state once it has presented frames with its GL context (see `hw_frame`).
    if bench.is_none() {
        if hw_render::requested().is_some() {
            app.resume_in = Some(30);
        } else {
            app.resume();
        }
    }
    let cheats = pocket_ui::cheats::apply(&app.core, &data, &app.video_src.0, &app.video_src.1);
    if cheats > 0 {
        println!("cheats: {cheats} on");
    }
    app.pixel_live.cheats = cheats;
    // The colour temperature chosen on the shelf (display.json, written by pocket-ui when it draws the Soft look), and
    // the night light by the clock (its own file and the same function pocket-ui uses; only the Soft backend is drawn here).
    let mut look = pocket_ui::display::Look::load(&data);
    if look.soft {
        look.night = pocket_ui::night_light::level_now(&pocket_ui::night_light::Prefs::load(&data));
    }
    app.hud.set_look(&look);
    if took_dsv {
        app.hud.note("Took over the DraStic save");
    }
    app.write_snapshot();
    app.core.take_snapshot_dirty();
    // Multi-disc games: the discs for the pause menu's Disc row, known once the game is loaded (`publish_discs`).
    app.publish_discs();
    pocket_ui::startup::mark("resume");
    STARTED.set(started).ok();
    event_loop.run_app(&mut app).expect("run");
    pocket_ui::fast_switch::mark("event loop left");
    // The Disc row's file ends with the game (a game started meanwhile keeps its own).
    pocket_ui::disc::forget(&app.video_src.1);
    // RetroArch's order: context_destroy, then retro_unload_game (when `core` drops below), both
    // with the core's context current.
    if let Some(Gfx { surface, hw: Some(hw), .. }) = app.gfx.as_mut() {
        hw.destroy(surface);
    }
    // Achievements end before the core unloads (they read its memory); an unlock still on its way gets a moment.
    if let Some(a) = app.cheevos.take() {
        a.close(Duration::from_millis(300));
        pocket_ui::fast_switch::mark("achievements closed");
    }
    app.ex.power.restore();
    // The next game (a hand-over starts it right after the line below) may note the CPU now.
    drop(perf_lease);
    pocket_ui::rumble::stop();
    pocket_ui::fast_switch::mark("cpu, fan and rumble back");
    // Fast switching (`fast_switch`): the exit save is on the card and the CPU, fan and rumble are back, so pocket-ui
    // may start the next game while the rest of this process ends. PipeWire's quantum stays for that game. Only for
    // the cores whose unload writes nothing of their own; any other ends first, as before (no line).
    if pocket_ui::fast_switch::requested() && pocket_ui::fast_switch::may_hand_over(&core_options::core_id(core_so), &app.video_src.0) {
        if let Some(a) = app.audio.as_mut() {
            a.keep_quantum();
        }
        pocket_ui::fast_switch::announce_saved();
    }
    app.stats.report(app.audio.as_ref());
    println!("core rate {:.3} fps  audio peak {}  {}", app.core.av.timing.fps, app.audio_peak, if app.time_driven { "scheduled by time" } else { "one per refresh" });
    app.ex.report();
    if let Some(line) = app.timeline.report() {
        println!("{line}");
    }
    if let Some(a) = &app.audio {
        // Bursts: underruns less than 0.2 s apart count as one stall, "time s ×count".
        let mut bursts: Vec<(f32, u32, f32)> = Vec::new();
        for &t in a.underrun_at.lock().unwrap().iter() {
            match bursts.last_mut() {
                Some((_, n, last)) if t - *last < 0.2 => (*n, *last) = (*n + 1, t),
                _ => bursts.push((t, 1, t)),
            }
        }
        let list: Vec<String> = bursts.iter().map(|(t, n, _)| format!("{t:.1}s×{n}")).collect();
        println!("audio buffered at exit {:.0} ms, underruns at {}", a.buffered_ms(), list.join(" "));
    }
    pocket_ui::fast_switch::mark("system restored, reported");
    // The core drops first (and writes the battery save), with GL still there for a 3D core, then the window, GL and
    // the sound stream, as the fields' order had it; a step each, to see which part of a slow teardown it is.
    let (window, gfx, audio) = (app.window.take(), app.gfx.take(), app.audio.take());
    drop(app);
    pocket_ui::fast_switch::mark("core unloaded");
    drop(window);
    drop(gfx);
    pocket_ui::fast_switch::mark("window and GL closed");
    drop(audio);
    pocket_ui::fast_switch::mark("torn down");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_commands() {
        let words = |c: &'static str| c.split_whitespace().collect::<Vec<_>>();
        assert_eq!(Exit::from_command(&words("quit")), Some(Exit::Save));
        assert_eq!(Exit::from_command(&words(pocket_ui::control::QUIT_WITHOUT_SAVING)), Some(Exit::WithoutSaving));
        // A garbled line is no quit at all, least of all a saving one.
        assert_eq!(Exit::from_command(&words("quit later")), None);
        assert_eq!(Exit::from_command(&words("quitnosave")), None);
        assert_eq!(Exit::from_command(&words(pocket_ui::fast_switch::QUIT_FOR_SWITCH)), Some(Exit::Handoff));
    }
}
