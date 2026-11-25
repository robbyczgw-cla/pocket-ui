//! Android's handheld view (prototype, android/handheld/Cargo.toml): the App's side on Android, compiled only with the
//! `handheld` feature for Android. `android_main` sets the App's folders and runs the same `main` as on Linux. The
//! library is the Linux one, read from real folders (an es_systems.cfg made here, pointing at /storage/emulated/0/ROMs;
//! the view needs "All files access"). A game starts in the APK's own player (PlayerActivity, `launchPath`), and its end
//! is the Activity coming back: then `GameExited`, and the way back with the last picture the player left.
use super::{App, Run, UserEvent, CANCEL};
use pocket_ui::pad::{Button, PadEvent};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopBuilder, EventLoopProxy};
use winit::keyboard::{KeyCode, NativeKeyCode, PhysicalKey};
use winit::platform::android::activity::AndroidApp;
use winit::platform::android::EventLoopBuilderExtAndroid;

/// QA only: the RetroAchievements server answered from the device (the player's own, the sign-in page here asks it too).
#[cfg(feature = "ra-replay")]
#[path = "../src/ra_replay.rs"]
mod ra_replay;

static ANDROID: Mutex<Option<AndroidApp>> = Mutex::new(None);
static PROXY: Mutex<Option<EventLoopProxy<UserEvent>>> = Mutex::new(None);
/// The game the player has: when it started, its file and system. Taken when the Activity comes back.
static PLAYING: Mutex<Option<(Instant, PathBuf, String)>> = Mutex::new(None);
/// The player was let go (`releaseLaunch`): the start's scene is over. Before that, leaving the view cancels the start.
static RELEASED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// Which start the scene's thread lets go: a cancelled one's thread lets nothing go.
static LAUNCH_TURN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn bridge(name: &str, args: &[&str]) -> String {
    pocket_ui::platform::android::strings(name, args).unwrap_or_default()
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_tag("PocketHandheld").with_max_level(log::LevelFilter::Info));
    stdio_to_log();
    // SAFETY: both pointers come from the live NativeActivity that is calling us.
    if let Err(e) = unsafe { pocket_ui::platform::android::initialize(app.vm_as_ptr().cast(), app.activity_as_ptr().cast()) } {
        log::error!("activity bridge: {e}");
    }
    // The first start speaks the device's language, as the Mobile view and the permission dialog do: `i18n::system_default`
    // reads `LANG` while settings.json has no language yet (a German Odin opened on "Welcome to your Pocket" in English, 13.11.).
    // A choice made in Settings is in settings.json and wins.
    if std::env::var_os("LANG").is_none_or(|l| l.is_empty()) {
        let lang = bridge("language", &[]);
        if !lang.is_empty() {
            std::env::set_var("LANG", lang);
        }
    }
    let files = PathBuf::from(bridge("dataPath", &[]));
    let data = files.join("handheld");
    let _ = std::fs::create_dir_all(&data);
    let cache = app.internal_data_path().unwrap_or_else(|| files.clone()).join("handheld-tmp");
    let _ = std::fs::create_dir_all(&cache);
    let es = data.join("es_systems.cfg");
    // A core downloaded before it came into the APK: the APK's copy is used, the download goes (as in the Mobile view).
    let retired = pocket_ui::android_cores::retire_bundled(&files, |core| bridge("coreBundled", &[core]) == "true", !offers().is_empty());
    if !retired.is_empty() {
        eprintln!("downloaded cores now in the APK, download removed: {retired:?}");
    }
    // The phone's apps and cores and the choices in launcher.json, before the library: which systems are listed depends on them.
    refresh_phone();
    let (roots, systems) = system_list(&data);
    // Written only when it changes: the library's cache is kept while es_systems.cfg is the same (`library_cache`).
    if let Err(e) = pocket_ui::android_handheld::files::keep_system_list(&es, &systems) {
        log::error!("es_systems.cfg: {e}");
    }
    // The first start's "Your games": the games folders of other apps, the BIOS there (`android_first_start`), on a thread.
    look_for_games();
    // What was played in the Mobile view: its last start and play time on this view's shelf (Continue, the recent games,
    // "Last played …" instead of "Not played yet").
    let mut recents = pocket_ui::recents::Recents::load(&data);
    let taken = pocket_ui::android_handheld::take_mobile_history(&mut recents, &data, &launcher_json(), &systems);
    if taken > 0 {
        recents.save();
        log::info!("handheld: play history of {taken} games taken from the Mobile view");
    }
    log::info!("handheld: data {} games in {roots:?}", data.display());
    // The tallies and the sign-in of RetroAchievements are the app's (the player and the Mobile view keep them there).
    if let Err(e) = pocket_ui::android_handheld::share_account(&data, &files) {
        log::error!("achievements folder: {e}");
    }
    // The sign-in is one file in the app's folder too (the player reads it there).
    pocket_ui::achievements::account::share_in(&files);
    // The shelf's moments are the player's: `states/<key>/<stem>` (the open case, Continue, the card's slots).
    pocket_ui::slots::set_resolver(state_base);
    #[cfg(feature = "ra-replay")]
    pocket_ui::achievements::http::set_qa_transport(|_| Box::new(ra_replay::Replay::new(0)));
    // "The host is there": every system the player plays starts as pocket-core's do on Linux (the start's scene waits for
    // the game). The player is the APK's; this file only says it is there.
    let host = data.join("pocket-core-is-the-apk-player");
    if !host.exists() {
        let _ = std::fs::write(&host, b"");
    }
    std::env::set_var("POCKETUI_HOST_BIN", &host);
    // Before any thread starts: the App reads these once (`main`).
    std::env::set_var("POCKETUI_DATA", &data);
    std::env::set_var("POCKETUI_ES_SYSTEMS", &es);
    std::env::set_var("TMPDIR", &cache);
    std::env::set_var("HOME", &data);
    *ANDROID.lock().unwrap_or_else(|p| p.into_inner()) = Some(app);
    super::main();
}

/// The first start's look at the device (`android_first_start::look`), on a thread: at start, and again when the view comes
/// back with the right to read the files it hadn't before (All files access given in Android's settings meanwhile).
fn look_for_games() {
    let can_read = bridge("filesAccess", &[]) == "true";
    if pocket_ui::android_first_start::facts().is_some_and(|f| f.can_read || !can_read) {
        return;
    }
    let _ = std::thread::Builder::new().name("first-start-look".into()).spawn(move || {
        let facts = pocket_ui::android_first_start::look(Path::new("/storage"), can_read);
        log::info!("first start: {} games folders found (files readable: {can_read})", facts.folders.map_or(0, |f| f.len()));
    });
}

/// The folders this view reads and its system list: the ROMs folders of internal storage and of every card, with their real
/// names (`android_handheld`), then the games folders taken on the first start (`android_first_start::kept_folders`).
fn system_list(data: &Path) -> (Vec<PathBuf>, String) {
    let mut roots = pocket_ui::android_handheld::roms_roots(Path::new("/storage"));
    // No ROMs folder yet: the systems are listed under internal storage's, read again when the view comes back.
    if roots.is_empty() {
        roots.push(PathBuf::from("/storage/emulated/0/ROMs"));
    }
    for kept in pocket_ui::android_first_start::kept_folders(data) {
        if !roots.contains(&kept) {
            roots.push(kept);
        }
    }
    // The Android games the player marked are a system of their own, in this view's folder (`app_android_games`).
    let systems = pocket_ui::android_games::with_es_entry(&pocket_ui::play_route_cfg::es_systems(&roots), &data.join(pocket_ui::android_games::FOLDER));
    (roots, systems)
}

/// A games folder taken on the first start: kept, and the system list written again with it (the App reads it again).
pub(crate) fn add_games_folder(folder: &Path) {
    let Some(data) = std::env::var_os("POCKETUI_DATA").map(PathBuf::from) else { return };
    pocket_ui::android_first_start::keep_folder(&data, folder);
    let (roots, systems) = system_list(&data);
    let es = std::env::var_os("POCKETUI_ES_SYSTEMS").map(PathBuf::from).unwrap_or_else(|| data.join("es_systems.cfg"));
    match pocket_ui::android_handheld::files::keep_system_list(&es, &systems) {
        Ok(_) => log::info!("first start: games folder {} added, reading {roots:?}", folder.display()),
        Err(e) => log::error!("es_systems.cfg: {e}"),
    }
}

/// The App's event loop on this Activity (`main` builds it).
pub(crate) fn attach(builder: &mut EventLoopBuilder<UserEvent>) {
    if let Some(app) = ANDROID.lock().unwrap_or_else(|p| p.into_inner()).take() {
        builder.with_android_app(app);
    }
}

/// The App's println and eprintln in logcat, line by line.
fn stdio_to_log() {
    let mut fds = [0; 2];
    // SAFETY: plain pipe and dup2 on descriptors this process owns.
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return;
        }
        libc::dup2(fds[1], 1);
        libc::dup2(fds[1], 2);
    }
    let read = fds[0];
    std::thread::spawn(move || {
        use std::io::BufRead;
        // SAFETY: the read end is ours alone from here on.
        let file = unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(read) };
        for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
            log::info!("{line}");
        }
    });
}

/// The APK's own core for a system (android/cores.lock.json, as the Mobile view's `bundled_core`).
fn bundled_core(system: &str) -> Option<&'static str> {
    pocket_ui::play_route::bundled_core(system)
}

// ---- what the phone has (`play_route`) ---------------------------------------------------------------------------------

/// The core packages the download channel offers, from the APK's asset (`coreOffers`): what the Download row fetches.
fn offers() -> &'static [pocket_ui::android_cores::CorePackage] {
    static OFFERS: std::sync::OnceLock<Vec<pocket_ui::android_cores::CorePackage>> = std::sync::OnceLock::new();
    OFFERS.get_or_init(|| {
        let mut offers = pocket_ui::android_cores::packages(bridge("coreOffers", &[]).as_bytes()).unwrap_or_default();
        // A core the APK carries is never a download.
        offers.retain(|p| bridge("coreBundled", &[&p.core]) != "true");
        offers
    })
}

/// launcher.json, the Mobile view's file: its choices, the Game Boy core and the channel's address come from it.
fn launcher_json() -> String {
    pocket_ui::android_handheld::files::launcher_json(&pocket_ui::platform::android::data_dir())
}

/// The download channel: the address the APK was built with, else the one saved in launcher.json (empty: not open yet).
fn channel_url(json: &str) -> String {
    let built_in = bridge("coreCatalogUrl", &[]);
    pocket_ui::android_handheld::files::channel_url(json, &built_in)
}

/// Reads the phone again: its emulator apps, the cores on it and the choices in launcher.json. At start, and whenever the
/// view is back in front (an app may have been installed, a core downloaded).
pub(crate) fn refresh_phone() {
    use pocket_ui::play_route;
    let json = launcher_json();
    let mut phone = play_route::Phone { channel_open: !channel_url(&json).is_empty(), gambatte: play_route::gambatte_from_json(&json), ..Default::default() };
    phone.set_installed_apps(&bridge("installedApps", &[&play_route::all_profiles()]));
    let lock: serde_json::Value = serde_json::from_str(include_str!("../cores.lock.json")).unwrap_or_default();
    for core in lock["cores"].as_array().into_iter().flatten().filter_map(|c| c["id"].as_str()) {
        if !bridge("corePath", &[core]).is_empty() {
            phone.cores.insert(core.to_string());
        }
    }
    phone.offers = offers().iter().map(|p| play_route::CoreOffer { core: p.core.clone(), name: p.package.name.clone(), size: p.package.size }).collect();
    play_route::set_phone(phone);
    play_route::set_prefs(play_route::prefs_from_json(&json));
}

/// How a core download stands (`core_download_poll`).
pub(crate) enum CoreDownload {
    Running(String),
    Done,
    Failed(String),
}

static CORE_RUN: Mutex<Option<pocket_ui::android_core_run::Run>> = Mutex::new(None);

/// The Download row's A: fetch this core on unmetered Wi-Fi. Err: why not, in a sentence. A thread wakes the App every
/// 400 ms to read the progress (`UserEvent::LaunchPrepared` is its poll event).
pub(crate) fn start_core_download(core: &str, proxy: EventLoopProxy<UserEvent>) -> Result<(), String> {
    let mut slot = CORE_RUN.lock().unwrap_or_else(|p| p.into_inner());
    if slot.is_some() {
        return Err("Downloading".into());
    }
    let Some(offer) = offers().iter().find(|p| p.core == core).cloned() else { return Err("This system is not in the download channel.".into()) };
    let url = channel_url(&launcher_json());
    if url.is_empty() {
        return Err("This version of pocket-ui can't download this system.".into());
    }
    if bridge("unmeteredWifi", &[]) != "true" {
        return Err("Downloads wait for unmetered Wi-Fi.".into());
    }
    bridge("coreBegin", &[]);
    *slot = Some(pocket_ui::android_core_run::Run::start(offer, url));
    std::thread::spawn(move || {
        while CORE_RUN.lock().unwrap_or_else(|p| p.into_inner()).is_some() {
            std::thread::sleep(Duration::from_millis(400));
            if proxy.send_event(UserEvent::LaunchPrepared).is_err() {
                break;
            }
        }
    });
    Ok(())
}

/// The running download's state, None when there is none. A finished one is taken (and the phone read again, so the core
/// is there for the next launch).
pub(crate) fn core_download_poll() -> Option<CoreDownload> {
    let mut slot = CORE_RUN.lock().unwrap_or_else(|p| p.into_inner());
    let run = slot.as_ref()?;
    match run.poll() {
        None => {
            let bytes: u64 = bridge("coreProgress", &[]).parse().unwrap_or(0);
            let total = offers().iter().find(|p| p.core == run.core).map_or(0, |p| p.package.size);
            let size = |b: u64| format!("{:.1} MB", b as f64 / 1_000_000.0);
            Some(CoreDownload::Running(format!("{} · {} / {}", pocket_ui::i18n::tr("Downloading"), size(bytes), size(total))))
        }
        Some(result) => {
            *slot = None;
            drop(slot);
            refresh_phone();
            Some(match result {
                Ok(()) => CoreDownload::Done,
                Err(why) => {
                    log::warn!("core download: {why}");
                    CoreDownload::Failed(why.0)
                }
            })
        }
    }
}

/// The game's key in the player: the Mobile view's for the same file (`android_handheld::game_key`: the storage provider's
/// document, Gambatte folded in when it plays a Game Boy game), so both views share its saves and moments.
fn game_key(path: &Path, system: &str, core: &str) -> Option<String> {
    pocket_ui::android_handheld::game_key(path, system, core == "gambatte")
}

/// The BIOS files of `system` in the ROMs folders' `bios` (any case), as the Mobile view hands them over (`bios_file`):
/// `[{"uri", "name"}]` for the player's own BIOS folder.
fn bios_for(system: &str) -> String {
    let mut list = Vec::new();
    let data = std::env::var_os("POCKETUI_DATA").map(PathBuf::from).unwrap_or_default();
    for root in pocket_ui::android_handheld::roms_roots(Path::new("/storage")).into_iter().chain(pocket_ui::android_first_start::kept_folders(&data)) {
        let Some(dir) = pocket_ui::android_handheld::child_ci(&root, "bios") else { continue };
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            if pocket_ui::document_library::bios_file(&name).is_some_and(|b| b.0 == system) {
                list.push(serde_json::json!({"uri": format!("file://{}", e.path().display()), "name": name}));
            }
        }
    }
    serde_json::Value::from(list).to_string()
}

/// Pocket-to-Pocket sync's view of this device (`pocket_sync::Layout::android`): the games in the ROMs folders this view reads,
/// their saves where the player keeps them (`saves/<key>/<stem>.<ext>`, the key as [`game_key`] makes it), replaced saves in
/// the app's own `sync-backups`. Settings → Game Boy core is read again at most every 5 s (it folds into Game Boy keys).
pub(crate) fn pocket_sync_layout() -> pocket_ui::pocket_sync::Layout {
    let files = pocket_ui::platform::android::data_dir();
    let data = std::env::var_os("POCKETUI_DATA").map(PathBuf::from).unwrap_or_default();
    let mut roots = pocket_ui::android_handheld::roms_roots(Path::new("/storage"));
    for kept in pocket_ui::android_first_start::kept_folders(&data) {
        if !roots.contains(&kept) {
            roots.push(kept);
        }
    }
    static GAMBATTE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let key = std::sync::Arc::new(|path: &Path, system: &str| {
        let gambatte = {
            let mut g = GAMBATTE.lock().unwrap_or_else(|p| p.into_inner());
            match *g {
                Some((at, v)) if at.elapsed() < Duration::from_secs(5) => v,
                _ => {
                    let v = pocket_ui::play_route::gambatte_from_json(&launcher_json());
                    *g = Some((Instant::now(), v));
                    v
                }
            }
        };
        pocket_ui::android_handheld::game_key(path, system, gambatte)
    });
    pocket_ui::pocket_sync::Layout::android(roots, files.join("saves"), files.join("sync-backups"), key)
}

/// The player's folder of each game of the shelf, found again at most every 5 s (a game copied meanwhile, Settings → Game
/// Boy core) and when a game starts or comes back.
static KEYS: Mutex<Option<(Instant, pocket_ui::android_handheld::StateKeys)>> = Mutex::new(None);

fn refresh_keys() -> pocket_ui::android_handheld::StateKeys {
    let es = std::env::var_os("POCKETUI_ES_SYSTEMS").map(PathBuf::from).unwrap_or_default();
    let games: Vec<(String, PathBuf)> = pocket_ui::library::load(&es)
        .unwrap_or_default()
        .into_iter()
        .flat_map(|s| s.games.into_iter().map(move |g| (s.name.clone(), g.path)))
        .collect();
    pocket_ui::android_handheld::StateKeys::of(&games, pocket_ui::play_route::gambatte_from_json(&launcher_json()))
}

/// `slots::base` for the handheld view: where the player keeps this game's states.
fn state_base(system: &str, stem: &str) -> Option<PathBuf> {
    let files = pocket_ui::platform::android::data_dir();
    let mut keys = KEYS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(base) = keys.as_ref().and_then(|(_, k)| k.base(&files, system, stem)) {
        return Some(base);
    }
    if keys.as_ref().is_some_and(|(at, _)| at.elapsed() < Duration::from_secs(5)) {
        return None;
    }
    let fresh = refresh_keys();
    let base = fresh.base(&files, system, stem);
    *keys = Some((Instant::now(), fresh));
    base
}

/// The keys are made again now: a game starts or is back, and what it wrote is read next.
fn forget_keys() {
    *KEYS.lock().unwrap_or_else(|p| p.into_inner()) = None;
}

/// The game the player has, on the card (`PLAYING` is gone with this process).
fn playing_note() -> PathBuf {
    PathBuf::from(std::env::var_os("POCKETUI_DATA").unwrap_or_default()).join("android-playing.json")
}

/// A game in the APK's player. The start's scene runs first (`HOLD_UNTIL` in `env`, the same hold pocket-core gets):
/// the player is made ready at once and comes when the scene is over or skipped.
pub(crate) fn spawn_game(proxy: EventLoopProxy<UserEvent>, run: &Run, env: &[(&str, String)]) -> Option<i32> {
    *PROXY.lock().unwrap_or_else(|p| p.into_inner()) = Some(proxy.clone());
    let failed = |why: &str| {
        log::warn!("launch: {why}");
        let _ = proxy.send_event(UserEvent::GameExited { secs: 0, ok: false, pgid: None, start: Default::default(), moments: false });
        None
    };
    // An Android game: its app, started as a home screen starts it (`app_android_games`).
    if run.system == pocket_ui::android_games::SYSTEM_ID {
        return spawn_android_game(proxy, run);
    }
    // Another app (`play_route::Way::App`): the route's emulator is its package. Our own player is pocket-core's route (hosted).
    if let super::Player::Other(package) = &run.player {
        return spawn_app_game(proxy, run, package, env);
    }
    let core = if run.core.is_empty() { bundled_core(&run.system).unwrap_or_default() } else { run.core.as_str() };
    let (true, Some(key)) = (!core.is_empty(), game_key(&run.path, &run.system, core)) else {
        return failed(&format!("no core of ours for {} ({})", run.system, run.path.display()));
    };
    let stem = run.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let title = pocket_ui::android_handheld::player_title(&run.name, &run.path);
    // The player's settings: the Mobile view's file (Gambatte, sounds ...) with this view's own choices over it: its language
    // and look, the game's picture rows, the cheats beside the game, whether the shelf showed its case open.
    forget_keys();
    let files = pocket_ui::platform::android::data_dir();
    let data = PathBuf::from(std::env::var_os("POCKETUI_DATA").unwrap_or_default());
    let launcher = pocket_ui::android_handheld::files::launcher_json(&files);
    let mut view = serde_json::to_value(pocket_ui::settings::Settings::load(&data)).unwrap_or_default();
    view["animations"] = pocket_ui::settings::Settings::load(&data).animations_or_default().name().into();
    // The game's settings: the player's pause menu reads and writes this view's files (`game_settings`), as pocket-core's does.
    let picture = pocket_ui::android_handheld::GamePicture::load(&data, &run.system, &run.path);
    let fresh = env.iter().any(|(k, _)| *k == pocket_ui::frozen::SKIP_RESUME_ENV);
    let moment = !fresh && pocket_ui::slots::auto_candidates(&pocket_ui::slots::auto_path(&files.join("states").join(&key).join(stem)), false).iter().any(|f| f.exists());
    let settings = pocket_ui::android_handheld::launch_settings(&launcher, &view, Some(picture), &pocket_ui::android_handheld::sidecar_cheats(&run.path), moment);
    let hold = env.iter().find(|(k, _)| *k == pocket_ui::startup::HOLD_UNTIL).and_then(|(_, v)| v.parse::<f64>().ok());
    if hold.is_some() {
        bridge("holdLaunch", &[]);
    }
    let path = run.path.to_string_lossy().into_owned();
    let said = bridge("launchPath", &[&path, &core, &key, &title, &run.system, &settings, &bios_for(&run.system)]);
    if !said.is_empty() {
        bridge("releaseLaunch", &[]);
        return failed(&said);
    }
    log::info!("launch: {} {path} core {core} key {key} hold {hold:?}", run.system);
    *PLAYING.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), run.path.clone(), run.system.clone()));
    // Also on the card: Android may end this process while the player has the screen; the next start ends the session.
    let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let _ = pocket_ui::fsutil::write_atomic(&playing_note(), serde_json::json!({"path": run.path, "system": run.system, "at": unix}).to_string().as_bytes());
    use std::sync::atomic::Ordering;
    RELEASED.store(hold.is_none(), Ordering::SeqCst);
    let turn = LAUNCH_TURN.fetch_add(1, Ordering::SeqCst) + 1;
    if let Some(until) = hold {
        let skip = pocket_ui::startup::skip_path();
        std::thread::spawn(move || {
            while pocket_ui::startup::clock_ms() < until && !skip.exists() && LAUNCH_TURN.load(Ordering::SeqCst) == turn {
                std::thread::sleep(Duration::from_millis(5));
            }
            if LAUNCH_TURN.load(Ordering::SeqCst) == turn {
                RELEASED.store(true, Ordering::SeqCst);
                bridge("releaseLaunch", &[]);
            }
        });
    }
    None
}

/// A game in another emulator app (PPSSPP, Dolphin, Azahar, ARMSX2, RetroArch ...): the start's scene runs first as for our own
/// player, then the Activity starts the app on the game's file; the way back is this Activity coming to the front again
/// (`android_resumed`), with the play time counted from here.
fn spawn_app_game(proxy: EventLoopProxy<UserEvent>, run: &Run, package: &str, env: &[(&str, String)]) -> Option<i32> {
    *PROXY.lock().unwrap_or_else(|p| p.into_inner()) = Some(proxy.clone());
    let profile = pocket_ui::play_route::profile_of(&run.system).unwrap_or_default();
    let hold = env.iter().find(|(k, _)| *k == pocket_ui::startup::HOLD_UNTIL).and_then(|(_, v)| v.parse::<f64>().ok());
    if hold.is_some() {
        bridge("holdLaunch", &[]);
    }
    let path = run.path.to_string_lossy().into_owned();
    let said = bridge("launchApp", &[&path, package, profile]);
    if !said.is_empty() {
        log::warn!("launch: {said}");
        bridge("releaseLaunch", &[]);
        let _ = proxy.send_event(UserEvent::GameExited { secs: 0, ok: false, pgid: None, start: Default::default(), moments: false });
        return None;
    }
    log::info!("launch: {} {path} in {package} hold {hold:?}", run.system);
    *PLAYING.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), run.path.clone(), run.system.clone()));
    let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let _ = pocket_ui::fsutil::write_atomic(&playing_note(), serde_json::json!({"path": run.path, "system": run.system, "at": unix}).to_string().as_bytes());
    if let Some(until) = hold {
        let skip = pocket_ui::startup::skip_path();
        std::thread::spawn(move || {
            while pocket_ui::startup::clock_ms() < until && !skip.exists() {
                std::thread::sleep(Duration::from_millis(5));
            }
            bridge("releaseLaunch", &[]);
        });
    }
    None
}

/// An Android game of the shelf (`android_games`): Android starts its app at once (no scene, as in the Mobile view); the way
/// back is this Activity coming to the front again (`android_resumed`), the play time counted from here. An app that went
/// ends the start at once (the shelf says so and loses it).
fn spawn_android_game(proxy: EventLoopProxy<UserEvent>, run: &Run) -> Option<i32> {
    *PROXY.lock().unwrap_or_else(|p| p.into_inner()) = Some(proxy.clone());
    let package = pocket_ui::android_games::package_of(&run.path).unwrap_or_default();
    let refused = if package.is_empty() { "no package".to_string() } else { bridge("launchApp", &[&package]) };
    if !refused.is_empty() {
        log::warn!("launch: android app {package}: {refused}");
        let _ = proxy.send_event(UserEvent::GameExited { secs: 0, ok: false, pgid: None, start: Default::default(), moments: false });
        return None;
    }
    log::info!("launch: android app {package}");
    *PLAYING.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), run.path.clone(), run.system.clone()));
    let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let _ = pocket_ui::fsutil::write_atomic(&playing_note(), serde_json::json!({"path": run.path, "system": run.system, "name": run.name, "at": unix}).to_string().as_bytes());
    None
}

/// Android's apps with a start icon (`AppDrawer.list`, a JSON list), for the Android games (`app_android_games`).
pub(crate) fn launcher_apps() -> String {
    bridge("launcherApps", &[])
}

/// An Android game's cover at `path` (`AppDrawer.cover`: its banner or icon), drawn again only after the app was updated.
pub(crate) fn app_cover(package: &str, path: &Path) {
    let said = bridge("appCover", &[package, &path.to_string_lossy()]);
    if !said.is_empty() {
        log::info!("android games: no cover for {package}: {said}");
    }
}

/// Apps (PC streaming, tools) are Linux's; here they end at once.
pub(crate) fn spawn_app(proxy: EventLoopProxy<UserEvent>, _cmd: std::process::Command) -> Option<i32> {
    let _ = proxy.send_event(UserEvent::GameExited { secs: 0, ok: false, pgid: None, start: Default::default(), moments: false });
    None
}

/// Settings → View → Mobile: the Mobile view opens and this one ends (`PocketBase.setView`).
/// Android's signed update list (`UpdateInstaller.channel`: pocketui.xyz's `updates/android/update.json`; a QA package's
/// own address), the same the Mobile view's Updates page reads (`app_update_hint`).
pub(crate) fn update_channel() -> String {
    bridge("updateChannel", &[])
}

/// The offered APK to Android's installer (`UpdateInstaller.start`, a JSON request of urls, version, sha256, size).
pub(crate) fn update_start(request: &str) {
    let _ = bridge("updateStart", &[request]);
}

/// The installer's state: phase, bytes, total, message, whether it can still be cancelled.
pub(crate) fn update_status() -> (String, u64, u64, String, bool) {
    let v: serde_json::Value = serde_json::from_str(&bridge("updateStatus", &[])).unwrap_or_default();
    let text = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    (text("phase"), v["bytes"].as_u64().unwrap_or(0), v["total"].as_u64().unwrap_or(0), text("message"), v["cancellable"].as_bool().unwrap_or(false))
}

pub(crate) fn update_cancel() {
    let _ = bridge("updateCancel", &[]);
}

/// Unmetered Wi-Fi is up: the only network the update reader uses.
pub(crate) fn unmetered_wifi() -> bool {
    bridge("unmeteredWifi", &[]) == "true"
}

pub(crate) fn open_mobile() {
    bridge("setView", &["mobile"]);
}

/// Painting time per frame, in logcat every 5 s (`paint:`): frames, the frame as the wall clock saw it (from the start of
/// the draw to the picture handed over), what the surface call took of it and how much of that was waiting for the window's
/// buffer, what was painted, and the thread's own CPU time over the surface call. Waiting is not work: the CPU column is.
pub(crate) fn paint_took(took: Duration, st: pocket_ui::frame::Stages, painter: &str) {
    #[derive(Default, Clone, Copy)]
    struct Col {
        sum: Duration,
        max: Duration,
        over: u32,
    }
    impl Col {
        fn add(&mut self, d: Duration) {
            self.sum += d;
            self.max = self.max.max(d);
            self.over += (d > Duration::from_micros(16_667)) as u32;
        }
        fn show(&self, n: u32) -> String {
            format!("{:.1}/{:.1}", self.sum.as_secs_f64() * 1000.0 / f64::from(n), self.max.as_secs_f64() * 1000.0)
        }
    }
    #[derive(Default)]
    struct Stats {
        since: Option<Instant>,
        n: u32,
        frame: Col,
        call: Col,
        waited: Col,
        painted: Col,
        cpu: Col,
    }
    static STATS: Mutex<Option<Stats>> = Mutex::new(None);
    // The parts of the frame (`frame::timed`), and a frame that cost more than a refresh in CPU time says which page it was.
    let parts = pocket_ui::frame::take_sections();
    // Slow in CPU time (work) or in the wall clock without waiting for the buffer (the thread was held up: counters say how).
    let held_up = st.total.saturating_sub(st.waited) > Duration::from_millis(16);
    if st.cpu > Duration::from_millis(12) || held_up {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        log::info!(
            "slow frame: {} cpu {:.1} (wall {:.1}, before the surface call {:.1}; waited {} preempted {} faults {}/{}); {}",
            painter,
            ms(st.cpu),
            ms(took),
            ms(took.saturating_sub(st.total)),
            st.waits,
            st.preempted,
            st.faults,
            st.disk_faults,
            pocket_ui::frame::SECTIONS.iter().zip(parts).map(|(n, d)| format!("{n} {:.1}", ms(d))).collect::<Vec<_>>().join(", ")
        );
    }
    let mut stats = STATS.lock().unwrap_or_else(|p| p.into_inner());
    let s = stats.get_or_insert_with(|| Stats { since: Some(Instant::now()), ..Default::default() });
    s.n += 1;
    s.frame.add(took);
    s.call.add(st.total);
    s.waited.add(st.waited);
    s.painted.add(st.paint);
    s.cpu.add(st.cpu);
    if s.since.is_some_and(|t| t.elapsed() >= Duration::from_secs(5)) {
        log::info!(
            "paint: {} frames, ms mean/max: frame {} (over 16.7: {}), surface call {} of which waiting {}, painted {}, cpu {} (over 16.7: {})",
            s.n,
            s.frame.show(s.n),
            s.frame.over,
            s.call.show(s.n),
            s.waited.show(s.n),
            s.painted.show(s.n),
            s.cpu.show(s.n),
            s.cpu.over
        );
        *stats = None;
    }
}

/// From the Activity's Back (a gesture or a Back key): B, as on the pad.
fn back() -> bool {
    let Some(proxy) = PROXY.lock().unwrap_or_else(|p| p.into_inner()).clone() else { return false };
    let ok = proxy.send_event(UserEvent::Pad(PadEvent::Button { source: 0, button: CANCEL, pressed: true })).is_ok();
    ok && proxy.send_event(UserEvent::Pad(PadEvent::Button { source: 0, button: CANCEL, pressed: false })).is_ok()
}

#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_HandheldActivity_nativeBack(_: *mut jni::sys::JNIEnv, _: jni::sys::jobject) -> jni::sys::jboolean {
    back() as jni::sys::jboolean
}

/// The view ends and its process with it (`HandheldActivity.onDestroy`): the lists handed to the card writer and the play
/// log's last lines are written first, as at the end of `main`.
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_HandheldActivity_nativeEnding(_: *mut jni::sys::JNIEnv, _: jni::sys::jobject) {
    log::info!("ending: the view closes, its process ends");
    pocket_ui::fsutil::settle();
    pocket_ui::play_log::settle();
}

#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_HandheldActivity_nativeConfigurationChanged(_: *mut jni::sys::JNIEnv, _: jni::sys::jobject) {}

/// Android's photo picker answered "Choose cover…" (`app_cover_pick`), from the UI thread or the copy's thread.
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_HandheldActivity_nativeCoverPicked(mut env: jni::JNIEnv, _: jni::objects::JObject, outcome: jni::objects::JString) {
    let outcome: String = env.get_string(&outcome).map(Into::into).unwrap_or_else(|_| "failed".into());
    if let Some(proxy) = PROXY.lock().unwrap_or_else(|p| p.into_inner()).clone() {
        let _ = proxy.send_event(UserEvent::CoverPicked(outcome));
    } else {
        log::warn!("cover: the picker answered {outcome}, but the app isn't listening");
    }
}

/// Only the Mobile view's direct start uses it (`launchInternal`); this view starts by path.
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_HandheldActivity_nativePrepareDocument(
    env: jni::JNIEnv, _: jni::objects::JObject, fd: jni::sys::jint, _: jni::objects::JString, _: jni::objects::JString,
) -> jni::sys::jstring {
    if fd >= 0 {
        // SAFETY: Java handed this descriptor over to us.
        unsafe { libc::close(fd) };
    }
    env.new_string("Not in the handheld view").map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

impl App {
    /// A pad key, a keyboard key or a finger; true when it was ours to take (the Linux arms after it don't see it).
    pub(crate) fn android_window_event(&mut self, el: &ActiveEventLoop, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key, state, repeat, .. }, .. } => {
                // The App repeats held buttons itself (`repeat_after`).
                if *repeat {
                    return true;
                }
                let button = match physical_key {
                    PhysicalKey::Code(KeyCode::ArrowUp) => Some(Button::Up),
                    PhysicalKey::Code(KeyCode::ArrowDown) => Some(Button::Down),
                    PhysicalKey::Code(KeyCode::ArrowLeft) => Some(Button::Left),
                    PhysicalKey::Code(KeyCode::ArrowRight) => Some(Button::Right),
                    PhysicalKey::Code(KeyCode::Enter | KeyCode::Space) => Some(super::CONFIRM),
                    PhysicalKey::Code(KeyCode::Escape | KeyCode::Backspace) => Some(CANCEL),
                    // The pad's menu button (KEYCODE_BUTTON_MODE): HOME on a Linux handheld, the quick settings.
                    PhysicalKey::Unidentified(NativeKeyCode::Android(110)) => Some(Button::Mode),
                    // The pad's own keys, L2/R2/L3/R3 included (`pad::android`, the Mobile view's and the player's).
                    PhysicalKey::Unidentified(NativeKeyCode::Android(key)) => pocket_ui::pad::android::button(*key as i32),
                    _ => None,
                };
                if let Some(button) = button {
                    *PROXY.lock().unwrap_or_else(|p| p.into_inner()) = Some(self.proxy.clone());
                    // As on Linux, each button enters as the role Settings → Buttons → Button layout gives it (review #10).
                    static REMAP: Mutex<Option<pocket_ui::button_layout::Remap>> = Mutex::new(None);
                    let e = PadEvent::Button { source: 0, button, pressed: *state == ElementState::Pressed };
                    let e = REMAP.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(Default::default).event(e);
                    self.user_event(el, UserEvent::Pad(e));
                }
                true
            }
            WindowEvent::Touch(t) => {
                use pocket_ui::touch::{Phase, TouchEvent};
                let Some(size) = self.window.as_ref().map(|w| w.inner_size()) else { return true };
                let phase = match t.phase {
                    winit::event::TouchPhase::Started => Phase::Down,
                    winit::event::TouchPhase::Moved => Phase::Move,
                    _ => Phase::Up,
                };
                let (x, y) = ((t.location.x as f32 / size.width.max(1) as f32).clamp(0.0, 1.0), (t.location.y as f32 / size.height.max(1) as f32).clamp(0.0, 1.0));
                self.user_event(el, UserEvent::Touch(TouchEvent { phase, x, y }));
                true
            }
            _ => false,
        }
    }

    /// The Activity went to the background (Home, the player, the screen off): its window is gone, and with it the
    /// surface. Made anew in `resumed`.
    pub(crate) fn android_suspended(&mut self) {
        use std::sync::atomic::Ordering;
        log::info!("suspended");
        // Pocket Sync listens only while this view is in front (`app_pocket_sync`).
        self.pocket_sync_background();
        self.surface = pocket_ui::frame::Presenter::empty();
        self.window = None;
        // A button held as the view went (a start, Home): its repeat ends, the next press is a press.
        self.held = None;
        // Home or the screen off during the start's scene: no game comes up behind the player's back, and no play time
        // counts while away (review of 12.11.2026, #12). The shelf is there again on the way back.
        if !RELEASED.load(Ordering::SeqCst) && PLAYING.lock().unwrap_or_else(|p| p.into_inner()).take().is_some() {
            LAUNCH_TURN.fetch_add(1, Ordering::SeqCst);
            RELEASED.store(true, Ordering::SeqCst);
            bridge("cancelLaunch", &[]);
            let _ = std::fs::remove_file(playing_note());
            log::info!("launch: cancelled, the view went before the game came");
            self.game_exited(0, false, Default::default(), false);
        }
    }

    /// Back in front. After a game: its end, as pocket-core's on Linux, and the way back from the picture the player
    /// left (`eject::LastPicture`). Called after the window is made.
    pub(crate) fn android_resumed(&mut self) {
        let save_error = pocket_ui::save_trouble::take_moment(&pocket_ui::platform::android::data_dir().join("player-save-error"));
        if let Some(line) = &save_error {
            self.notice = Some(pocket_ui::notice::Notice::new(line.clone()));
            self.request_redraw();
        }
        *PROXY.lock().unwrap_or_else(|p| p.into_inner()) = Some(self.proxy.clone());
        // Back from Home or a dark screen: a look soon when the last is an hour old, and a version found meanwhile is said
        // now that it can be seen (`app_update_hint`).
        self.update_hint_front(std::time::Instant::now());
        // Opened again after an update from the shelf's card (or the Mobile view's page): once, which version it is now.
        let installed = pocket_ui::platform::android::strings("updateNotice", &[]).unwrap_or_else(|e| {
            log::warn!("update notice: {e}");
            String::new()
        });
        if !installed.is_empty() {
            self.toast = Some((format!("Updated to {installed}"), std::time::Instant::now()));
            log::info!("update notice: {installed}");
        }
        // An app may have been installed or a core downloaded while this view was behind another one.
        refresh_phone();
        look_for_games();
        // And an Android game installed, renamed or removed, or marked in the Mobile view (`app_android_games`).
        self.android_games_refresh("back in front");
        // The player may have written moments and slots: they are read from the player's folders next.
        forget_keys();
        let note = pocket_ui::android_handheld::files::take_playing_note(&playing_note());
        let Some((started, path, system)) = PLAYING.lock().unwrap_or_else(|p| p.into_inner()).take() else {
            // This process was ended while the game ran: the session ends here, as Quit would have ended it.
            if let Some(n) = note {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let pocket_ui::android_handheld::files::ResumedGame { path, system, name, secs } = pocket_ui::android_handheld::files::resumed_game(&n, now);
                log::info!("resumed after this process ended in a game: {} ({secs} s)", path.display());
                self.recents.note(&system, &name, &path, secs);
                pocket_ui::play_log::ended(&system, &name, &path, secs);
                self.request_redraw();
            } else {
                // The zone libc uses here (Android's own, `system_ctl::mock_owns_clock`): Two versions, Pocket Sync's times.
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
                log::info!("resumed (local time {:+} s from UTC)", pocket_ui::timezone::utc_offset_now(now));
            }
            // The Mobile view may have been in front meanwhile (this process kept): what it played comes in too.
            let es = pocket_ui::android_handheld::files::system_list(Path::new(&std::env::var_os("POCKETUI_ES_SYSTEMS").unwrap_or_default()));
            let data = PathBuf::from(std::env::var_os("POCKETUI_DATA").unwrap_or_default());
            if pocket_ui::android_handheld::take_mobile_history(&mut self.recents, &data, &launcher_json(), &es) > 0 {
                self.recents.save_soon();
                self.request_redraw();
            }
            // Games copied meanwhile, a card put in, All files access given: the folders are read again (on a thread).
            self.library_rescan("back in front");
            return;
        };
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let note = pocket_ui::eject::LastPicture::take(&pocket_ui::platform::android::data_dir(), now_ms);
        log::info!("back from {} after {:?}: last picture {:?}", path.display(), started.elapsed(), note.as_ref().map(|n| (n.saved, n.picture.is_some())));
        // The end before the way back: the shelf after Quit first (`game_exited`), then its scene (`begin_eject`).
        self.game_exited(started.elapsed().as_secs(), true, Default::default(), false);
        if let Some(line) = save_error { self.notice = Some(pocket_ui::notice::Notice::new(line)); }
        let size = self.window.as_ref().map(|w| (w.inner_size().width, w.inner_size().height));
        let frame = note.as_ref().and_then(|n| pocket_ui::android_handheld::files::return_frame(n, size)).map(std::sync::Arc::new);
        // The whole screen as the player left it is the way back's first picture (`begin_eject`, as the pause menu's grab).
        // An Android game went into no console: the shelf is there at once, on its case.
        if system != pocket_ui::android_games::SYSTEM_ID {
            self.begin_eject(&path, &system, frame);
        }
        self.request_redraw();
    }
}
