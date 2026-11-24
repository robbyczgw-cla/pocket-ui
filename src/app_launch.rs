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
    pub(crate) fn queue_launch(&mut self, system: String, name: String, path: PathBuf) {
        if self.pending_launch.is_some() { return; }
        let asked = pocket_ui::startup::clock_ms();
        if pocket_ui::startup::is_r36s_n64(&system) {
            println!("n64 launch: requested at uptime_ms={asked:.0}");
        }
        // PortMaster's missing-files page uses its own cached installation state and no insertion scene.
        if matches!(system.as_str(), "ports") && !pocket_ui::device::darkos_port() && self.port_needs_files(&path) { return; }
        let Some(sys) = self.systems.iter().find(|s| s.name == system) else { return };
        // Android (a phone, `play_route`): our player is always there, it is the APK's.
        let host = std::path::Path::new(super::host_bin()).exists() || pocket_ui::play_route::phone().is_some();
        let (emulator, core) = self.route(sys, Some(&path), host);
        // Android's handheld view: a core to download first, or no app for the system, is told here instead of started.
        if self.phone_prelaunch(&system, &name, &path) { return; }
        // Our own player on Android stays pocket-core's route (hosted), as on Linux (Lead, 13.11.2026: the ritual first): the
        // start's scene runs in full and holds the game, and the moments and open cases are pocket-core's (`frozen`). Another app's game is that app's (`play_route::Way::App`) and starts at once.
        if self.kids_route_closed(&system, &emulator, &core, &path) {
            println!("kids mode: stopped, {} would start on {emulator}", path.display());
            return self.kids_stuck();
        }
        pocket_ui::covers::set_game_on(true);
        self.cover_prefetch_stop();
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
        // A phone game's first start on ROCKNIX: Java is loaded first, its progress over the start (`j2me_java`).
        let java = (!hosted && pocket_ui::j2me_java::needed(&pocket_ui::j2me_java::Places::here(), &core))
            .then(|| std::sync::Arc::new(pocket_ui::j2me_java::Fetch::default()));
        self.java_fetch = java.clone();
        // A RomM game not on the card yet: downloaded first, its bar over the start (`app_romm`).
        let romm = self.romm_fetch_for(&system, &path);
        self.romm.get_mut().fetch = romm.clone();
        let run = Run { name: name.clone(), player, system: system.clone(), core, path: path.clone() };
        pocket_ui::motion::set_cozy(self.cozy());
        pocket_ui::ui_sound::stop_for(!hosted);
        // A on the Cabinet's playing TV: the game goes on from there, its picture growing out of the TV, no insertion
        // (`App::showcase_hand_over`); pocket-core holds it still until the picture has grown.
        let grow = hosted && !port && self.showcase_hand_over(&path, &run.core);
        // An Android game is an app Android starts: no case goes into a console (as in the Mobile view).
        let app_game = system == pocket_ui::android_games::SYSTEM_ID && pocket_ui::device::android_profile();
        let animation = if port || app_game { None } else if grow { Some(super::app_cabinet_tv::HAND_HOLD) } else { self.launch_animation(&path, hosted) };
        // A sealed game is unwrapped for good now, in every look (the Cozy launch tore its film off already).
        self.unseal(&path);
        self.launch_at = Some(Instant::now());
        self.warm_launch_cover(&system, &path);
        self.launch_picture = None;
        self.loading_start();
        self.held = None;
        self.start_tap.disarm();
        // Prepare the skip file before accepting another button; a worker must never erase an early skip.
        pocket_ui::startup::prepare_skip();
        let (tx, rx) = mpsc::channel();
        // The Cabinet's TV stops playing before the game starts (prototype, `pocket_ui::showcase`), unless A just
        // took its game over (`grow`): then the run is ending by itself.
        self.cabinet.tv.showcase_stop();
        if !grow {
            self.cabinet.tv.grow = None;
        }
        self.pending_launch = Some(Pending { run, name: name.clone(), asked, animation, painted: false, skipped: false, rx });
        pocket_ui::trace::step("launch preparation queued");
        let fan = self.perf_fan_prepare_launch();
        let discard = self.large_game_library_on_rk3326().then(|| {
            self.infos_warm = (0, 0);
            self.gamedb.clear_loaded();
            (std::mem::take(&mut self.game_infos), self.curator.release_for_game(self.screen, &self.items))
        });
        std::thread::spawn(move || {
            if let Some(discard) = discard {
                drop(discard);
                pocket_ui::heap_trim::trim_now();
            }
            let prepared = prepare(&name, &game, &data, kids, pixel, shared(), shown, drains, java.as_deref(), romm.as_deref(), fan);
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
        // Android's handheld view: a core download's progress (`app_runs_on_android`).
        self.phone_poll();
        let Some(p) = self.pending_launch.as_mut().filter(|p| p.painted) else { return };
        let (outcome, fan) = match p.rx.try_recv() {
            Ok((o, fan)) => (o, Some(fan)),
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => (Outcome::Empty(pocket_ui::fallback::not_started_notice_on(&p.run.system, &p.name, None)), None),
        };
        self.perf_fan_launch_ready(fan);
        self.java_fetch = None;
        self.romm.get_mut().fetch = None;
        let Some(p) = self.pending_launch.take() else { return };
        match outcome {
            Outcome::Ready { at, wait, resumable, preview, env } => self.launch_prepared(p, at, wait, resumable, preview, env),
            outcome => {
                pocket_ui::covers::set_game_on(self.running.is_some());
                // A start that never began: how far its animation had got, for the way back out (`begin_failed_start`).
                let ran = self.launch_at.map_or(Duration::ZERO, |t| t.elapsed());
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
                        self.begin_failed_start(&path, &system, &p.name, ran);
                    }
                    Outcome::Empty(msg) => {
                        self.notice = Some(Notice::new(msg));
                        self.big_note_failure(&path, &system, &p.name);
                        self.begin_failed_start(&path, &system, &p.name, ran);
                    }
                    Outcome::Ready { .. } => unreachable!(),
                }
                if self.identify.due() || self.cover_run.resume_due() {
                    self.auto_covers("after a cancelled start");
                }
                if self.text_run.resume_due() {
                    self.auto_texts("after a cancelled start");
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

fn prepare(name: &str, game: &frozen::Game, data: &std::path::Path, kids: bool, pixel: bool,
    shared: bool, shown: bool, drains: Vec<i32>, java: Option<&pocket_ui::j2me_java::Fetch>, romm: Option<&super::app_romm::RommFetch>, fan: impl FnOnce(bool) -> pocket_ui::perf_fan::FanKeeper) -> (Outcome, pocket_ui::perf_fan::FanKeeper) {
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
    // A RomM game's first start: its download, unless it is on the card (`app_romm`). What went wrong is the start's note.
    if let Some(r) = romm {
        if let Err(f) = r.run(path) { return (Outcome::Empty(f.words()), fan(false)); }
    }
    if let Some(msg) = pocket_ui::library_cache::missing_game(name, path) { return (Outcome::Missing(msg), fan(false)); }
    if let Some(msg) = pocket_ui::library_cache::empty_game(name, path) { return (Outcome::Empty(msg), fan(false)); }
    pocket_ui::trace::step("launch files checked");
    if let Some(fetch) = java {
        println!("j2me: loading Java before {}", path.display());
        if let Err(e) = pocket_ui::j2me_java::load(&pocket_ui::j2me_java::Places::here(), fetch) {
            println!("j2me: Java not loaded: {e:?}");
            return (Outcome::Empty(e.message()), fan(false));
        }
    }
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
    // Phone games: Runs on › D-pad as RetroArch's remap for this game (`j2me_keys`).
    pocket_ui::j2me_keys::before_start(system, core, path);
    // N64, PSP, Dreamcast on ROCKNIX's emulators (H700): Runs on › D-pad, the left stick from the D-pad (`dpad_stick_rocknix`).
    pocket_ui::dpad_stick_rocknix::before_start(system, core, path);
    // Phone games at their own screen size, filling the screen (`j2me_size`).
    pocket_ui::j2me_size::before_start(system, core, path);
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
