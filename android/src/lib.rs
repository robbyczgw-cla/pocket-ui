//! Native Android host for pocket-ui's shared drawing, media, discovery and platform adapters.
#![cfg(target_os = "android")]
mod show;
mod ui;
use android_activity::AndroidApp;
use pocket_ui::{
    document_library::{self, Game},
    gfx::Canvas,
    i18n,
    pad::Button,
    text::Text,
};
use std::{
    num::NonZeroU32,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use ui::{Command, Preferences, Ui};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, NativeKeyCode, PhysicalKey},
    platform::android::EventLoopBuilderExtAndroid,
    window::{Window, WindowId},
};
/// How long a finger rests on a game before its card opens.
const LONG_PRESS: Duration = Duration::from_millis(450);
static BACK_PROXY: std::sync::OnceLock<std::sync::RwLock<Option<winit::event_loop::EventLoopProxy<()>>>> = std::sync::OnceLock::new();
#[no_mangle]
pub extern "system" fn Java_dev_pocket_spike_PocketActivity_nativeBack(_: *mut jni::sys::JNIEnv, _: jni::sys::jobject) -> jni::sys::jboolean {
    BACK_PROXY
        .get()
        .and_then(|p| p.read().ok())
        .and_then(|p| p.as_ref().map(|p| p.send_event(()).is_ok()))
        .unwrap_or(false) as jni::sys::jboolean
}
fn bridge(name: &str, args: &[&str]) -> String {
    pocket_ui::platform::android::strings(name, args).unwrap_or_default()
}
fn bundled_core(system: &str) -> Option<&'static str> {
    static CORES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    CORES
        .get_or_init(|| serde_json::from_str(include_str!("../cores.lock.json")).expect("bundled core lock"))
        .get("cores")?
        .as_array()?
        .iter()
        .find(|c| {
            c["systems"]
                .as_array()
                .is_some_and(|systems| systems.iter().any(|s| s.as_str() == Some(system)))
        })?
        .get("id")?
        .as_str()
}
fn scan(tree: &str) -> Result<ui::Library, String> {
    let json = pocket_ui::platform::android::strings("scanLibraries", &[tree])?;
    let docs = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(ui::library(document_library::scan(docs)))
}
fn picture(game: &Game, snaps: bool, download: bool) -> Option<pocket_ui::image::Image> {
    // Deterministic FNV key; provider identity remains stable when the display name changes.
    let hash = game
        .key
        .bytes()
        .fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
    let dest = pocket_ui::platform::data_dir()
        .join(if snaps { "pictures" } else { "covers" })
        .join(format!("{hash:016x}.png"));
    if !dest.exists() {
        if !download {
            return None;
        }
        let playlist = pocket_ui::covers::playlist(game.system_id)?;
        let stem = std::path::Path::new(&game.relative).file_stem()?.to_str()?;
        let index = pocket_ui::covers::fetch_index(playlist);
        let name = pocket_ui::covers::best_match(stem, &index)?;
        if !pocket_ui::covers::download_from(
            playlist,
            if snaps { "Named_Snaps" } else { "Named_Boxarts" },
            name,
            &dest,
            &["--max-filesize", "8388608"],
            &|p| pocket_ui::image::png_size(p).is_some(),
        ) {
            return None;
        }
        log::info!("cover downloaded: {}", game.name);
    }
    pocket_ui::image::load_png(&dest)
}
enum Media {
    Picture(String, Option<pocket_ui::image::Image>),
    Cover(String, Option<pocket_ui::image::Image>),
    Summary(String, pocket_ui::game_card_model::Summary),
}
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
struct Viewport {
    left: u32,
    top: u32,
    w: u32,
    h: u32,
    lw: u32,
    lh: u32,
}
/// One finger on the screen: where it came down, where it is, whether it became a drag or a long press, and its
/// last positions for the fling's speed.
struct Finger {
    id: u64,
    start: (f64, f64),
    at: (f64, f64),
    down: Instant,
    dragging: bool,
    long: bool,
    samples: std::collections::VecDeque<(Instant, f64, f64)>,
}
struct App {
    ui: Ui,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    text: Text,
    pixels: Vec<u32>,
    scaled: Vec<u32>,
    scaler: pocket_ui::scaled_layout::ScaledFrame,
    viewport: Viewport,
    selection: String,
    scan: Option<mpsc::Receiver<Result<ui::Library, String>>>,
    check_at: Instant,
    media: mpsc::Sender<(Game, bool)>,
    media_policy: bool,
    media_results: mpsc::Receiver<Media>,
    cover_key: String,
    cover: Option<pocket_ui::image::Image>,
    last_launch: Option<Instant>,
    cursor: (f64, f64),
    finger: Option<Finger>,
    last_tick: Instant,
    semantics: String,
    last_persist: String,
    show: show::Show,
    keeping_screen_on: bool,
    /// A touch that only ended the slideshow: the rest of that touch does nothing.
    swallow_touch: bool,
}
impl App {
    fn persist(&mut self) {
        if let Some((key, uri)) = self.ui.game().map(|g| (g.key.clone(), g.uri.clone())) {
            self.ui.prefs.selected_key = key;
            self.ui.prefs.selected_uri = uri;
        }
        let Ok(json) = serde_json::to_string(&self.ui.prefs) else { return };
        if json != self.last_persist {
            self.last_persist = json;
            let path = pocket_ui::platform::data_dir().join("launcher.json");
            if self.ui.prefs.onboarding_done {
                pocket_ui::fsutil::write_json_soon(&path, &self.ui.prefs);
            } else {
                let _ = pocket_ui::fsutil::write_json(&path, &self.ui.prefs);
            }
        }
    }
    /// Any input for the idle slideshow: true when it was on screen, so that input only ended it.
    fn show_input(&mut self) -> bool {
        let was = self.show.input(Instant::now());
        if was {
            self.redraw();
        }
        was
    }
    /// One look at the slideshow: the screen stays on while it can run, and a frame is due while it moves.
    fn show_tick(&mut self, now: Instant) -> Option<Instant> {
        let eligible = self.window.is_some()
            && self.ui.prefs.onboarding_done
            && self.ui.prefs.extras.idle_slideshow
            && self.ui.keyboard.is_none()
            && self.scan.is_none()
            && matches!(self.ui.screen, ui::Screen::Library | ui::Screen::Favorites)
            && self.ui.library.as_ref().is_some_and(|l| !l.games.is_empty());
        let size = ((self.viewport.w / 2).max(1) as i32, (self.viewport.h / 2).max(1) as i32);
        let gambatte = self.ui.prefs.gameboy_gambatte;
        let games = || {
            self.ui.library.iter().flat_map(|l| l.games.iter()).map(|g| {
                let art = g.key.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
                show::Entry {
                    name: g.name.clone(),
                    system: g.system.to_string(),
                    art: format!("{art:016x}"),
                    key: ui::game_key(g, gambatte),
                    stem: std::path::Path::new(&g.relative).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                }
            }).collect()
        };
        let wake = self.show.tick(eligible, size, games, &pocket_ui::platform::data_dir(), now);
        if wake.keep_screen_on != self.keeping_screen_on {
            self.keeping_screen_on = wake.keep_screen_on;
            bridge("keepScreenOn", &[if wake.keep_screen_on { "true" } else { "false" }]);
        }
        if self.show.visible() {
            self.redraw();
        }
        wake.at
    }
    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
    fn command(&mut self, command: Command) {
        match command {
            Command::Launch => {
                if self.last_launch.is_some_and(|at| at.elapsed() < Duration::from_millis(900)) {
                    return;
                }
                if let Some(g) = self.ui.game() {
                    if !g.missing.is_empty() {
                        self.ui.message = format!("Missing disc files: {}", g.missing.join(", "));
                        return;
                    }
                    self.last_launch = Some(Instant::now());
                    let game = g.clone();
                    // Rediscover's input: when this game was last started.
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                    self.ui.prefs.played.insert(game.key.clone(), now);
                    // Decorations' input: the time away counts as played when the launcher is back (`game_returned`).
                    self.ui.prefs.playing = Some((game.key.clone(), now));
                    self.persist();
                    let core = if pocket_ui::platform::android::string("externalPlayer") == "true" {
                        None
                    } else {
                        if self.ui.prefs.gameboy_gambatte && matches!(game.system_id, "gb" | "gbc") {
                            Some("gambatte")
                        } else {
                            bundled_core(game.system_id)
                        }
                    };
                    if let Some(core) = core {
                        let key = ui::game_key(&game, self.ui.prefs.gameboy_gambatte);
                        let settings = serde_json::to_string(&self.ui.prefs).unwrap_or_default();
                        let _ = pocket_ui::platform::android::strings(
                            "launchInternal",
                            &[&game.uri, &game.relative, core, &key, &game.name, game.system_id, &settings],
                        );
                    } else {
                        let _ = pocket_ui::platform::android::strings("launchGame", &[&game.uri, &game.relative, game.profile]);
                    }
                }
            }
            Command::Folder => {
                if !self.ui.prefs.onboarding_done {
                    let _ = pocket_ui::fsutil::write_json(&pocket_ui::platform::data_dir().join("launcher.json"), &self.ui.prefs);
                }
                bridge("chooseFolder", &[]);
            }
            Command::PersistSetup => {
                // A confirmed step survives an immediate force-stop, including a system picker handoff.
                let _ = pocket_ui::fsutil::write_json(&pocket_ui::platform::data_dir().join("launcher.json"), &self.ui.prefs);
                self.last_persist.clear();
                self.cover_key.clear();
            }
            Command::Persist => self.persist(),
            Command::None => (),
        }
        self.redraw();
    }
    fn sound(&self, b: Button) {
        if self.ui.prefs.muted {
            return;
        }
        let sound = match b {
            Button::South => pocket_ui::ui_sound::Sound::Back,
            Button::North if self.ui.screen != ui::Screen::Box && self.ui.game().is_some() => {
                pocket_ui::ui_sound::opening(pocket_ui::media::Medium::of(self.ui.game().unwrap().system_id))
            }
            Button::East => pocket_ui::ui_sound::Sound::Confirm,
            _ => pocket_ui::ui_sound::Sound::Nav,
        };
        let _ = pocket_ui::ui_sound::play(
            sound,
            &pocket_ui::ui_sound::Ctx {
                config: Default::default(),
                in_game: false,
                pause_menu: false,
                asleep: self.window.is_none(),
            },
        );
    }
    fn back(&mut self) {
        self.sound(Button::South);
        let command = self.ui.back();
        self.command(command);
    }
    fn press(&mut self, b: Button) {
        self.sound(b);
        let command = self.ui.press(b);
        self.command(command);
    }
    fn point(&self, x: f64, y: f64) -> (f32, f32) {
        let v = self.viewport;
        (
            ((x - v.left as f64) * v.lw as f64 / v.w.max(1) as f64) as f32,
            ((y - v.top as f64) * v.lh as f64 / v.h.max(1) as f64) as f32,
        )
    }
    fn logical_tap(&mut self, x: f32, y: f32) {
        // The sound of what the tap does: moving the selection or a tab is Nav, Back is Back, the rest Confirm.
        let action = self.ui.regions.iter().rev().find(|r| r.area.contains(x, y)).map(|r| r.action.clone());
        let button = match action {
            Some(ui::Action::Press(b)) => Some(b),
            Some(ui::Action::Tab(_)) => Some(Button::Right),
            Some(ui::Action::Row(i)) if ui::Ui::list_screen(self.ui.screen) && i != self.ui.selected => Some(Button::Right),
            Some(ui::Action::Label) | None => None,
            _ => Some(Button::East),
        };
        if let Some(button) = button {
            self.sound(button);
        }
        let command = self.ui.tap(x, y);
        self.command(command);
    }
    fn tap(&mut self, x: f64, y: f64) {
        let (x, y) = self.point(x, y);
        self.logical_tap(x, y);
    }
    fn refresh(&mut self) {
        if self.scan.is_some() {
            return;
        }
        let selected = bridge("librarySelection", &[]);
        if selected == self.selection {
            return;
        }
        let tree = selected.split('\n').next().unwrap_or("").to_string();
        self.selection = selected;
        if tree.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        self.ui.scan_running = true;
        self.ui.message = "Reading library...".into();
        std::thread::spawn(move || {
            let _ = tx.send(scan(&tree));
        });
    }
    fn paint(&mut self) -> Result<(), String> {
        let Some(window) = &self.window else { return Ok(()) };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let values: Vec<f32> = bridge("windowMetrics", &[]).split(',').filter_map(|n| n.parse().ok()).collect();
        let density = values.first().copied().unwrap_or(1.0);
        self.ui.system_font_scale = values.get(1).copied().unwrap_or(1.0);
        let inset = |i: usize, max: u32| values.get(i).copied().unwrap_or(0.0).max(0.0).min(max as f32 / 4.0) as u32;
        let left = inset(2, size.width);
        let top = inset(3, size.height);
        let w = size.width - left - inset(4, size.width);
        let h = size.height - top - inset(5, size.height);
        // The phone layout is sized in dp itself, so the canvas is the screen's own pixels (sharp type, no
        // resampling); the shared game card gets a smaller canvas that is enlarged (`Ui::zoom`).
        let zoom = self.ui.zoom(w > h);
        let showing = self.show.visible();
        let (lw, lh) = if showing {
            // The slideshow paints at half the screen's size: smooth enough for slow movement, a quarter of the work.
            (((w / 2).max(1)), ((h / 2).max(1)))
        } else if zoom < 1.0 {
            let (mw, mh) = pocket_ui::scaled_layout::mobile_size((w, h), density);
            let k = (w.min(h) as f32 * zoom / mw.min(mh) as f32).clamp(0.5, 1.0);
            (((mw as f32 * k) as u32).max(1), ((mh as f32 * k) as u32).max(1))
        } else {
            (w, h)
        };
        if !showing {
            self.ui.dp = lw as f32 / (w as f32 / density.max(0.5));
        }
        let viewport = Viewport { left, top, w, h, lw, lh };
        if self.viewport != viewport {
            log::info!("launcher viewport: {viewport:?} density={density}");
        }
        self.viewport = viewport;
        self.pixels.resize((lw * lh) as usize, 0);
        let mut c = Canvas::new(&mut self.pixels, lw, lh);
        if showing {
            let still = pocket_ui::motion::level() == pocket_ui::motion::Motion::Off;
            self.show.paint(&mut c, &mut self.text, Instant::now(), still);
            // Nothing of the page behind it can be tapped or read out.
            self.ui.regions.clear();
        } else {
            self.ui.paint(&mut c, &mut self.text, self.cover.as_ref());
        }
        self.scaled.resize((w * h) as usize, 0);
        self.scaler.downscale(&self.pixels, (lw, lh), &mut self.scaled, (w, h));
        let Some(surface) = &mut self.surface else { return Ok(()) };
        surface
            .resize(NonZeroU32::new(size.width).unwrap(), NonZeroU32::new(size.height).unwrap())
            .map_err(|e| e.to_string())?;
        let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
        buffer.fill(if showing { 0 } else { pocket_ui::palette::pal().void });
        for (y, row) in self.scaled.chunks_exact(w as usize).enumerate() {
            let at = (y + top as usize) * size.width as usize + left as usize;
            buffer[at..at + w as usize].copy_from_slice(row);
        }
        buffer.present().map_err(|e| e.to_string())?;
        let regions:Vec<_>=self.ui.regions.iter().enumerate().map(|(id,r)|serde_json::json!({"id":id,"label":r.label,"selected":self.ui.selected_region(r),"x":left+r.area.x.max(0) as u32*w/lw,"y":top+r.area.y.max(0) as u32*h/lh,"w":r.area.w.max(0) as u32*w/lw,"h":r.area.h.max(0) as u32*h/lh})).collect();
        let semantics = serde_json::to_string(&regions).unwrap_or_default();
        if semantics != self.semantics {
            self.semantics = semantics;
            bridge("publishSemantics", &[&self.semantics]);
        }
        Ok(())
    }
}
impl ApplicationHandler for App {
    fn user_event(&mut self, _: &ActiveEventLoop, _: ()) {
        if self.show_input() {
            return;
        }
        self.back();
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = match event_loop.create_window(Window::default_attributes().with_title("pocket-ui")) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                log::error!("window: {e}");
                return;
            }
        };
        match softbuffer::Context::new(window.clone()).and_then(|c| softbuffer::Surface::new(&c, window.clone())) {
            Ok(s) => self.surface = Some(s),
            Err(e) => log::error!("surface: {e}"),
        };
        self.window = Some(window);
        self.ui.message.clear();
        self.ui.game_returned();
        self.persist();
        self.refresh();
        self.redraw();
    }
    fn suspended(&mut self, _: &ActiveEventLoop) {
        pocket_ui::ui_sound::stop();
        self.persist();
        self.surface = None;
        self.window = None;
        self.finger = None;
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.persist();
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
                self.ui.cover_changed();
                self.redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.paint() {
                    log::error!("paint: {e}");
                }
            }
            WindowEvent::Touch(t) => {
                // The slideshow: the touch that ends it does nothing else, nor does the rest of that touch.
                if t.phase == TouchPhase::Started && self.show_input() {
                    self.swallow_touch = true;
                    return;
                }
                if self.swallow_touch {
                    self.swallow_touch = !matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled);
                    return;
                }
                self.show.input(Instant::now());
                let (x, y) = (t.location.x, t.location.y);
                let now = Instant::now();
                match t.phase {
                    TouchPhase::Started => {
                        if self.finger.is_none() {
                            self.ui.hold();
                            self.finger = Some(Finger {
                                id: t.id,
                                start: (x, y),
                                at: (x, y),
                                down: now,
                                dragging: false,
                                long: false,
                                samples: [(now, x, y)].into(),
                            });
                        }
                    }
                    TouchPhase::Moved => {
                        let density = self.window.as_ref().map_or(1.0, |w| w.scale_factor());
                        let scale = self.viewport.lw as f64 / self.viewport.w.max(1) as f64;
                        let mut moved = None;
                        if let Some(f) = self.finger.as_mut().filter(|f| f.id == t.id && !f.long) {
                            if !f.dragging && (x - f.start.0).abs().max((y - f.start.1).abs()) > 8.0 * density {
                                f.dragging = true;
                            }
                            if f.dragging {
                                moved = Some(((x - f.at.0) * scale, (y - f.at.1) * scale));
                                f.at = (x, y);
                            }
                            f.samples.push_back((now, x, y));
                            while f.samples.len() > 2 && now.duration_since(f.samples[0].0) > Duration::from_millis(100) {
                                f.samples.pop_front();
                            }
                        }
                        if let Some((dx, dy)) = moved {
                            let before = self.ui.selected;
                            if self.ui.drag(dx as f32, dy as f32) || before != self.ui.selected {
                                self.redraw();
                            }
                        }
                    }
                    TouchPhase::Ended => {
                        if let Some(f) = self.finger.take() {
                            if f.id == t.id && !f.long {
                                if f.dragging {
                                    let scale = self.viewport.lw as f64 / self.viewport.w.max(1) as f64;
                                    let (t0, x0, y0) = f.samples[0];
                                    let dt = now.duration_since(t0).as_secs_f64().max(0.016);
                                    // A finger that stopped before lifting does not throw the page.
                                    let still = f.samples.back().is_some_and(|(at, _, _)| now.duration_since(*at) > Duration::from_millis(80));
                                    if !still {
                                        self.ui.fling(((x - x0) / dt * scale) as f32, ((y - y0) / dt * scale) as f32);
                                        self.last_tick = now;
                                    }
                                    self.redraw();
                                } else {
                                    self.tap(x, y);
                                }
                            } else if f.id != t.id {
                                self.finger = Some(f);
                            }
                        }
                    }
                    TouchPhase::Cancelled => self.finger = None,
                }
            }
            WindowEvent::CursorMoved { position, .. } => self.cursor = (position.x, position.y),
            WindowEvent::MouseInput {
                state: ElementState::Released, ..
            } => self.tap(self.cursor.0, self.cursor.1),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if self.show_input() {
                    return;
                }
                // A key from a controller or keyboard: its hints and selection come back (a touch hides them).
                if self.ui.touch_mode {
                    self.ui.touch_mode = false;
                    self.redraw();
                }
                let b = match event.physical_key {
                    PhysicalKey::Code(KeyCode::ArrowUp) => Some(Button::Up),
                    PhysicalKey::Code(KeyCode::ArrowDown) => Some(Button::Down),
                    PhysicalKey::Code(KeyCode::ArrowLeft) => Some(Button::Left),
                    PhysicalKey::Code(KeyCode::ArrowRight) => Some(Button::Right),
                    PhysicalKey::Code(KeyCode::Enter | KeyCode::Space) | PhysicalKey::Unidentified(NativeKeyCode::Android(23 | 96)) => Some(Button::East),
                    PhysicalKey::Code(KeyCode::Escape) | PhysicalKey::Unidentified(NativeKeyCode::Android(4 | 97)) => Some(Button::South),
                    PhysicalKey::Code(KeyCode::KeyX) | PhysicalKey::Unidentified(NativeKeyCode::Android(99)) => Some(Button::North),
                    PhysicalKey::Code(KeyCode::KeyY) | PhysicalKey::Unidentified(NativeKeyCode::Android(100)) => Some(Button::West),
                    PhysicalKey::Code(KeyCode::Tab) | PhysicalKey::Unidentified(NativeKeyCode::Android(109)) => Some(Button::Select),
                    PhysicalKey::Code(KeyCode::KeyS) | PhysicalKey::Unidentified(NativeKeyCode::Android(108)) => Some(Button::Start),
                    PhysicalKey::Unidentified(NativeKeyCode::Android(102)) => Some(Button::L1),
                    PhysicalKey::Unidentified(NativeKeyCode::Android(103)) => Some(Button::R1),
                    PhysicalKey::Code(KeyCode::KeyF) => {
                        bridge("chooseFolder", &[]);
                        None
                    }
                    PhysicalKey::Code(KeyCode::KeyL) => {
                        self.ui.prefs.light = !self.ui.prefs.light;
                        self.ui.cover_changed();
                        self.persist();
                        self.redraw();
                        None
                    }
                    _ => None,
                };
                if let Some(b) = b {
                    self.press(b);
                }
            }
            _ => (),
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        if let Some(result) = self.scan.as_ref().and_then(|rx| rx.try_recv().ok()) {
            match result {
                Ok(lib) => {
                    log::info!("library: {} games from SAF", lib.games.len());
                    i18n::learn_names(lib.games.iter().map(|g| g.name.as_str()));
                    self.ui.install(lib);
                }
                Err(e) => {
                    log::error!("library scan: {e}");
                    self.ui.message = "Could not read the game folders. Choose your folder again.".into();
                }
            }
            self.scan = None;
            self.ui.scan_running = false;
            self.redraw();
        }
        if let Some(game) = self.ui.game() {
            if game.key != self.cover_key {
                self.cover_key = game.key.clone();
                self.cover = None;
                self.ui.snapshot = None;
                self.ui.cover_changed();
                let game = self.ui.game().unwrap().clone();
                let _ = self.media.send((game, self.media_policy));
                self.persist();
            }
        }
        while let Ok(event) = self.media_results.try_recv() {
            match event {
                Media::Picture(key, image) if key == self.cover_key => {
                    self.ui.snapshot = image;
                    self.ui.cover_changed();
                    self.redraw();
                }
                Media::Cover(key, image) if key == self.cover_key => {
                    self.cover = image;
                    self.ui.cover_changed();
                    self.redraw();
                }
                Media::Summary(key, summary) => {
                    if let Some(lib) = &mut self.ui.library {
                        if let Some(i) = lib.games.iter().position(|g| g.key == key) {
                            lib.cards[i].summary = summary;
                        }
                    }
                    self.redraw();
                }
                _ => (),
            }
        }
        if Instant::now() >= self.check_at {
            self.refresh();
            let connected = bridge("deviceControls", &[]) == "controller";
            if connected && !self.ui.controller_connected {
                // A controller was just connected: show its hints until the screen is touched again.
                self.ui.touch_mode = false;
                self.redraw();
            }
            self.ui.controller_connected = connected;
            self.ui.wifi_ready = bridge("unmeteredWifi", &[]) == "true";
            let policy = self.ui.prefs.download_media == Some(true) && self.ui.wifi_ready;
            if policy != self.media_policy {
                self.media_policy = policy;
                self.cover_key.clear();
            }
            if !self.ui.prefs.onboarding_done {
                self.redraw();
            }
            if bridge("navigationEvents", &[]) == "back" {
                self.back();
            }
            let actions = bridge("semanticActions", &[]);
            for id in actions.split(',').filter_map(|n| n.parse::<usize>().ok()) {
                if let Some(r) = self.ui.regions.get(id) {
                    let a = r.area;
                    self.logical_tap((a.x + a.w / 2) as f32, (a.y + a.h / 2) as f32);
                }
            }
            self.check_at = Instant::now() + Duration::from_millis(150);
        }
        let now = Instant::now();
        let mut long_at = None;
        let mut held = None;
        if let Some(f) = self.finger.as_mut().filter(|f| !f.dragging && !f.long) {
            if now.duration_since(f.down) >= LONG_PRESS {
                f.long = true;
                held = Some(f.start);
            } else {
                long_at = Some(f.down + LONG_PRESS);
            }
        }
        if let Some((x, y)) = held {
            let (x, y) = self.point(x, y);
            let on_game = ui::Ui::list_screen(self.ui.screen)
                && self.ui.regions.iter().rev().find(|r| r.area.contains(x, y)).is_some_and(|r| matches!(r.action, ui::Action::Row(_)));
            if on_game {
                // The case's own opening sound, as X on the shelf.
                if let Some(i) = self.ui.regions.iter().rev().find(|r| r.area.contains(x, y)).and_then(|r| match r.action {
                    ui::Action::Row(i) => Some(i),
                    _ => None,
                }) {
                    self.ui.selected = i;
                }
                self.sound(Button::North);
            }
            let command = self.ui.long_press(x, y);
            self.command(command);
        }
        // The idle slideshow: starts after a quiet minute on the shelf, keeps the screen on while it can run.
        let wake = self.show_tick(now);
        let flinging = self.finger.is_none() && {
            let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.05);
            self.last_tick = now;
            self.ui.tick(dt)
        };
        if flinging {
            self.redraw();
        }
        if self.ui.animating() || flinging {
            self.redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(sooner(now + Duration::from_millis(16), wake)));
        } else if let Some(at) = long_at {
            event_loop.set_control_flow(ControlFlow::WaitUntil(sooner(at.min(self.check_at), wake)));
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(sooner(self.check_at, wake)));
        }
    }
}
/// The earlier of a wake-up and the slideshow's.
fn sooner(at: Instant, wake: Option<Instant>) -> Instant {
    wake.map_or(at, |w| w.min(at))
}
#[no_mangle]
fn android_main(android: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_tag("PocketMvp").with_max_level(log::LevelFilter::Info));
    unsafe {
        pocket_ui::platform::android::initialize(android.vm_as_ptr().cast(), android.activity_as_ptr().cast()).expect("Activity bridge");
    }
    let prefs: Preferences = pocket_ui::fsutil::read_json_or_backup(&pocket_ui::platform::data_dir().join("launcher.json")).unwrap_or_default();
    i18n::set(prefs.german.map_or_else(
        || i18n::Lang::from_locale(&bridge("language", &[])),
        |g| if g { i18n::Lang::De } else { i18n::Lang::En },
    ));
    prefs.apply_look();
    pocket_ui::ui_sound::start();
    let (media, requests) = mpsc::channel::<(Game, bool)>();
    let (results, media_results) = mpsc::channel();
    std::thread::spawn(move || {
        let mut db = pocket_ui::gamedb::GameDb::embedded();
        while let Ok(mut request) = requests.recv() {
            while let Ok(next) = requests.try_recv() {
                request = next;
            }
            let (game, wanted) = request;
            let download = wanted && bridge("unmeteredWifi", &[]) == "true";
            let image = picture(&game, false, download);
            if results.send(Media::Cover(game.key.clone(), image)).is_err() {
                break;
            }
            if results.send(Media::Picture(game.key.clone(), picture(&game, true, download))).is_err() {
                break;
            }
            let stem = std::path::Path::new(&game.relative).file_stem().unwrap_or_default().to_string_lossy();
            if let Some(title) = db.lookup(game.system_id, &stem).and_then(|g| g.enwiki) {
                let data = pocket_ui::platform::data_dir();
                let summary = match pocket_ui::wiki_text::cached(&data, &title).map(Ok).unwrap_or_else(|| {
                    if download {
                        pocket_ui::wiki_text::fetch(&data, &title)
                    } else {
                        Err("Downloads are off or waiting for Wi-Fi".into())
                    }
                }) {
                    Ok(text) => pocket_ui::game_card_model::Summary::Wikipedia { text, title },
                    Err(reason) => pocket_ui::game_card_model::Summary::Unavailable(reason),
                };
                if results.send(Media::Summary(game.key, summary)).is_err() {
                    break;
                }
            }
        }
    });
    let mut builder = EventLoop::builder();
    builder.with_android_app(android);
    let event_loop = builder.build().expect("Android event loop");
    *BACK_PROXY
        .get_or_init(|| std::sync::RwLock::new(None))
        .write()
        .unwrap_or_else(|e| e.into_inner()) = Some(event_loop.create_proxy());
    let mut app = App {
        ui: Ui::new(prefs),
        window: None,
        surface: None,
        text: Text::builtin().following_font(),
        pixels: vec![],
        scaled: vec![],
        scaler: Default::default(),
        viewport: Default::default(),
        selection: String::new(),
        scan: None,
        check_at: Instant::now(),
        media,
        media_policy: false,
        media_results,
        cover_key: String::new(),
        cover: None,
        last_launch: None,
        cursor: (0.0, 0.0),
        finger: None,
        last_tick: Instant::now(),
        semantics: String::new(),
        last_persist: String::new(),
        show: show::Show::new(),
        keeping_screen_on: false,
        swallow_touch: false,
    };
    log::info!("android_main: shared launcher starting");
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("event loop: {e}");
    }
}
