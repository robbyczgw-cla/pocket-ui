//! Launch preparation runs beside the insertion animation. The selected route and save policy are captured once;
//! only the matching pending launch may start a process, after at least one response frame was painted.
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use pocket_ui::{control, emu_settings, frozen, standalone};
use super::{App, Notice, Player, Run, UserEvent};

enum Outcome {
    Missing(String),
    Empty(String),
    Controls,
    Ready { at: frozen::AtLaunch, wait: bool, resumable: bool, preview: bool, env: Vec<(&'static str, String)> },
}

pub(crate) struct Pending {
    pub run: Run,
    pub name: String,
    pub asked: f64,
    pub animation: Option<Duration>,
    pub painted: bool,
    pub skipped: bool,
    rx: Receiver<(Outcome, pocket_ui::perf_fan::FanKeeper)>,
}

impl App {
    pub(crate) fn queue_launch(&mut self, system: String, name: String, path: PathBuf, controls: bool) {
        if self.pending_launch.is_some() { return; }
        let asked = pocket_ui::startup::clock_ms();
        // PortMaster's missing-files page uses its own cached installation state and no insertion scene.
        if matches!(system.as_str(), "ports") && self.port_needs_files(&path) { return; }
        let Some(sys) = self.systems.iter().find(|s| s.name == system) else { return };
        let (emulator, core) = self.route(sys, Some(&path), std::path::Path::new(super::host_bin()).exists());
        if self.kids_route_closed(&system, &emulator, &core, &path) {
            println!("kids mode: stopped, {} would start on {emulator}", path.display());
            return self.kids_stuck();
        }
        let player = Player::of(&system, &emulator, &core);
        let hosted = player == Player::Hosted;
        let port = system == "ports";
        let game = frozen::Game { system: system.clone(), path: path.clone(), emulator: emulator.clone(), core: core.clone(),
            mark: self.recents.list.iter().find(|r| r.path == path).and_then(|r| r.frozen.clone()) };
        // Capture names in memory; hidden twins may need directory/config reads, which belong on the worker too.
        let same_stem = |p: &std::path::Path| p != path && p.file_stem() == path.file_stem();
        let shown_games: Vec<_> = self.systems.iter().filter(|s| s.name == system).flat_map(|s| &s.games)
            .filter(|g| same_stem(&g.path)).map(|g| g.path.clone()).collect();
        let hidden_games: Vec<_> = self.library.hidden_games().filter(|p| same_stem(p)).map(std::path::Path::to_path_buf).collect();
        let recent = self.recents.list.clone();
        let es_systems = self.es_systems.clone();
        let (twin_path, twin_system, twin_emulator) = (path.clone(), system.clone(), emulator.clone());
        let shared = move || {
            if !matches!(twin_emulator.as_str(), "pocket-core" | "retroarch") { return false; }
            let in_system = |p: &std::path::Path| {
                let all = pocket_ui::library::all_systems(&es_systems).unwrap_or_default();
                all.iter().find(|s| s.id == twin_system).is_none_or(|s| p.starts_with(&s.path))
            };
            let twins = frozen::twins(&twin_path, shown_games.iter().map(PathBuf::as_path), hidden_games.iter().map(PathBuf::as_path), in_system);
            if twins.is_empty() { return false; }
            let entry = |p: &std::path::Path| recent.iter().find(|r| r.path == p);
            !frozen::shared_moment_is_own(entry(&twin_path).map(|r| (r.last_played, r.frozen.is_some())), twins.iter().map(|p| entry(p).map(|r| r.last_played)))
        };
        let shown = self.is_frozen_path(&path);
        let drains = self.launch_drains(&system, &path, &emulator, &core);
        let (data, kids, pixel, proxy) = (self.data_dir.clone(), self.kids_live(), self.pixel_path, self.proxy.clone());
        let run = Run { player, system: system.clone(), core, path: path.clone() };
        pocket_ui::motion::set_cozy(self.cozy());
        pocket_ui::ui_sound::stop_for(!hosted);
        let animation = if port { None } else { self.launch_animation(&path, hosted) };
        self.launch_at = Some(Instant::now());
        self.launch_picture = None;
        self.loading_start();
        self.held = None;
        self.start_tap.disarm();
        // Prepare the skip file before accepting another button; a worker must never erase an early skip.
        pocket_ui::startup::prepare_skip();
        let (tx, rx) = mpsc::channel();
        self.pending_launch = Some(Pending { run, name: name.clone(), asked, animation, painted: false, skipped: false, rx });
        pocket_ui::trace::step("launch preparation queued");
        let fan = self.perf_fan_prepare_launch();
        std::thread::spawn(move || {
            let prepared = prepare(&name, &game, &data, kids, pixel, controls, shared(), shown, drains, fan);
            let _ = tx.send(prepared);
            let _ = proxy.send_event(UserEvent::LaunchPrepared);
        });
        self.request_redraw();
        // The headless driver has no window to deliver RedrawRequested; paint the same response through its presenter.
        #[cfg(target_os = "linux")]
        if super::test_driver_dir().is_some() { let _ = self.proxy.send_event(UserEvent::TestPresent); }
    }

    /// Called only after presentation (also by the headless driver's real paint path).
    pub(crate) fn launch_response_painted(&mut self) {
        if let Some(p) = self.pending_launch.as_mut() {
            if !p.painted { pocket_ui::trace::step("launch first response painted"); }
            p.painted = true;
        }
    }

    pub(crate) fn launch_poll(&mut self) {
        let Some(p) = self.pending_launch.as_ref().filter(|p| p.painted) else { return };
        let (outcome, fan) = match p.rx.try_recv() {
            Ok((o, fan)) => (o, Some(fan)),
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => (Outcome::Empty(pocket_ui::fallback::not_started_notice_on(&p.run.system, &p.name, None)), None),
        };
        self.perf_fan_launch_ready(fan);
        let Some(p) = self.pending_launch.take() else { return };
        match outcome {
            Outcome::Ready { at, wait, resumable, preview, env } => self.launch_prepared(p, at, wait, resumable, preview, env),
            outcome => {
                self.launch_at = None;
                self.end_insert();
                pocket_ui::ui_sound::forget_waiting();
                let Run { system, path, .. } = p.run;
                match outcome {
                    Outcome::Missing(msg) => {
                        self.notice = Some(Notice::new(msg));
                        self.library_rescan("a game's file was missing");
                        #[cfg(feature = "brick")]
                        self.rebuild();
                    }
                    Outcome::Empty(msg) => {
                        self.notice = Some(Notice::new(msg));
                        self.big_note_failure(&path, &system, &p.name);
                    }
                    Outcome::Controls => self.open_launch_controls(system, p.name, path),
                    Outcome::Ready { .. } => unreachable!(),
                }
                self.request_redraw();
            }
        }
    }
}

/// Tests (`POCKETUI_MOCK`, `POCKETUI_TEST_PREPARE_HOLD=1`): the preparation waits (at most 2 s) until the first response
/// to A and four more are painted and `TEST_PREPARE_RELEASED` lets it go, and says when it got past that (`TEST_PREPARED`). So whether A
/// paints before the preparation is done is a fact of the order, not of how fast the computer is (tests/launch_response.rs).
pub(crate) static TEST_PREPARE_RELEASED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(crate) static TEST_PREPARED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn prepare(name: &str, game: &frozen::Game, data: &std::path::Path, kids: bool, pixel: bool, controls: bool,
    shared: bool, shown: bool, drains: Vec<i32>, fan: impl FnOnce(bool) -> pocket_ui::perf_fan::FanKeeper) -> (Outcome, pocket_ui::perf_fan::FanKeeper) {
    pocket_ui::trace::step("launch preparation begin");
    if pocket_ui::proc::mock() {
        if let Some(ms) = std::env::var("POCKETUI_TEST_PREPARE_MS").ok().and_then(|s| s.parse::<u64>().ok()) {
            std::thread::sleep(Duration::from_millis(ms.min(2000)));
        }
        if std::env::var_os("POCKETUI_TEST_PREPARE_HOLD").is_some() {
            use std::sync::atomic::Ordering;
            let held = std::time::Instant::now();
            while !TEST_PREPARE_RELEASED.load(Ordering::SeqCst) && held.elapsed() < Duration::from_secs(2) {
                std::thread::sleep(Duration::from_millis(2));
            }
            TEST_PREPARED.store(true, Ordering::SeqCst);
        }
    }
    let hosted = Player::of(&game.system, &game.emulator, &game.core) == Player::Hosted;
    let (system, path, emulator, core) = (&game.system, &game.path, &game.emulator, &game.core);
    if let Some(msg) = pocket_ui::library_cache::missing_game(name, path) { return (Outcome::Missing(msg), fan(false)); }
    if let Some(msg) = pocket_ui::library_cache::empty_game(name, path) { return (Outcome::Empty(msg), fan(false)); }
    pocket_ui::trace::step("launch files checked");
    if controls && !kids && hosted {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
        let states = pocket_ui::states::StateSet::pocket_core(&pocket_ui::slots::base(system, stem));
        let resuming = states.path(pocket_ui::states::Target::Auto).is_some();
        if pocket_ui::controls_card::first_start_due(data, system, false, resuming) { return (Outcome::Controls, fan(false)); }
    }
    pocket_ui::trace::step("launch controls check ready");
    for pgid in drains {
        if !pocket_ui::fast_switch::wait_ended(pgid, pocket_ui::quit::WINDOW_GRACE) {
            control::terminate_game(pgid, true);
            pocket_ui::fast_switch::wait_ended(pgid, Duration::from_secs(1));
        }
    }
    pocket_ui::trace::step("launch drain ready");
    #[cfg(target_os = "linux")]
    control::rules_in_place();
    pocket_ui::trace::step("launch window rules ready");
    let keeper = fan(true);
    pocket_ui::trace::step("launch fan ready");
    #[cfg(target_os = "linux")]
    emu_settings::apply_start_defaults_once();
    pocket_ui::trace::step("launch defaults ready");
    let at = if !frozen::available() { frozen::AtLaunch::Normal } else if shared {
        if hosted && frozen::resume_files(game).iter().any(|f| f.exists()) {
            frozen::AtLaunch::Fresh(frozen::SHARED)
        } else { frozen::AtLaunch::Note(frozen::SHARED) }
    } else { frozen::at_launch(game, shown) };
    let wait = shown && !hosted && standalone::auto_states(system, emulator, core, path).is_some();
    let resumable = frozen::is_frozen(game, &frozen::resume_files(game));
    let preview = hosted && pocket_ui::launch_picture::source(data, system, path).is_some();
    pocket_ui::trace::step("launch frozen checked");
    if matches!(at, frozen::AtLaunch::Fresh(_)) && !hosted { standalone::skip_next_resume(); }
    standalone::set_kids(kids);
    standalone::launch(system, emulator, core, path, data);
    pocket_ui::trace::step("launch standalone prepared");
    emu_settings::launch(standalone::session().map(|s| s.emu), system, path);
    if pixel { pocket_ui::pixel_standalone::prefetch(); }
    let env = emu_settings::launch_env(system, path);
    pocket_ui::trace::step("launch environment ready");
    pocket_ui::on_start::note_started(data, path);
    (Outcome::Ready { at, wait, resumable, preview, env }, keeper)
}
