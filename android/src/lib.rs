//! Native Android host for pocket-ui's shared drawing, media, discovery and platform adapters.
#![cfg(target_os = "android")]
mod eject;
mod insert;
mod copy_run;
mod core_downloads;
mod ui;
mod updates;
#[cfg(feature = "ra-replay")]
mod ra_replay;
use android_activity::AndroidApp;
use pocket_ui::{
    document_library::{self, Game},
    gfx::Canvas,
    i18n,
    pad::Button,
    text::Text,
};
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use ui::{Command, Preferences, Ui};
use pocket_ui::platform::data_dir;
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
enum BridgeEvent { Back, Configuration }
static BACK_PROXY: std::sync::OnceLock<std::sync::RwLock<Option<winit::event_loop::EventLoopProxy<BridgeEvent>>>> = std::sync::OnceLock::new();
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PocketActivity_nativeBack(_: *mut jni::sys::JNIEnv, _: jni::sys::jobject) -> jni::sys::jboolean {
    BACK_PROXY
        .get()
        .and_then(|p| p.read().ok())
        .and_then(|p| p.as_ref().map(|p| p.send_event(BridgeEvent::Back).is_ok()))
        .unwrap_or(false) as jni::sys::jboolean
}
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PocketActivity_nativeConfigurationChanged(_: *mut jni::sys::JNIEnv, _: jni::sys::jobject) {
    if let Some(proxy)=BACK_PROXY.get().and_then(|p|p.read().ok()) {
        if let Some(proxy)=proxy.as_ref() {let _=proxy.send_event(BridgeEvent::Configuration);}
    }
}
fn bridge(name: &str, args: &[&str]) -> String {
    pocket_ui::platform::android::strings(name, args).unwrap_or_default()
}
/// Own the worker's duplicate, prime only checksum metadata and close the temporary launcher alias.
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PocketActivity_nativePrepareDocument(
    mut env: jni::JNIEnv, _: jni::objects::JObject, fd: jni::sys::jint,
    key: jni::objects::JString, name: jni::objects::JString,
) -> jni::sys::jstring {
    use std::os::fd::FromRawFd;
    let result = (|| -> Result<(), String> {
        if fd < 0 { return Err("No document descriptor".into()); }
        let source = unsafe { std::fs::File::from_raw_fd(fd) };
        let key: String = env.get_string(&key).map_err(|e| e.to_string())?.into();
        let name: String = env.get_string(&name).map_err(|e| e.to_string())?.into();
        if key.len() != 16 || !key.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("Invalid game identity".into()); }
        let _document = pocket_ui::platform::android_content::DirectGame::new(source, &data_dir().join("direct-content").join(key), &name).map_err(|e| e.to_string())?;
        Ok(())
    })();
    env.new_string(result.err().unwrap_or_default()).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
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
/// The core of our own player that would start this game; None when another app would (no moment of ours: never frozen).
/// Another app starts it when Settings → Plays with says so for its system (and the app is on the phone), and for the
/// test's `externalPlayer` launch extra.
fn player_core(game: &Game, ui: &ui::Ui) -> Option<&'static str> {
    if ui.plays_in_app(game).is_some() {
        None
    } else {
        own_core(game, ui.prefs.gameboy_gambatte)
    }
}
/// `player_core` without Settings → Plays with (the Game check asks it for every game, whatever app would start it).
fn own_core(game: &Game, gambatte: bool) -> Option<&'static str> {
    if pocket_ui::platform::android::string("externalPlayer") == "true" {
        None
    } else if gambatte && matches!(game.system_id, "gb" | "gbc") {
        Some("gambatte")
    } else {
        bundled_core(game.system_id)
    }
}
fn scan(tree: &str) -> Result<ui::Library, String> {
    let json = pocket_ui::platform::android::strings("scanLibraries", &[tree])?;
    let docs: Vec<document_library::Document> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let bios = document_library::player_bios(&docs);
    let mut library = ui::library(document_library::scan(docs));
    library.bios = bios;
    Ok(library)
}
fn picture(game: &Game, snaps: bool, download: bool) -> Option<pocket_ui::image::Image> {
    for uri in if snaps { &game.pictures } else { &game.covers } {
        if let Ok(path) = pocket_ui::platform::android::strings("readPicture", &[uri]) {
            if let Some(image) = pocket_ui::image::load_png(std::path::Path::new(&path)) { return Some(image); }
        }
    }
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
    /// The read of which games stand open (`frozen_refresh`); a newer one replaces it, so a late answer is dropped.
    frozen: Option<mpsc::Receiver<Vec<String>>>,
    check_at: Instant,
    /// When the status corner's facts are asked for again (`phone_corner::Reading::next_look`).
    corner_at: Instant,
    media: mpsc::Sender<(Game, bool)>,
    media_policy: bool,
    battery_saver: bool,
    media_results: mpsc::Receiver<Media>,
    cover_key: String,
    cover: Option<pocket_ui::image::Image>,
    last_launch: Option<Instant>,
    /// The missing BIOS was explained for each system in this run of the app (`document_library::bios_hint`).
    bios_hint_said: std::collections::HashSet<String>,
    cursor: (f64, f64),
    finger: Option<Finger>,
    last_tick: Instant,
    semantics: String,
    last_persist: String,
    /// A touch that ended the insert animation: the rest of that touch does nothing.
    swallow_touch: bool,
    /// A game's start on its way (`insert`): the case going into its console, until the player is up.
    insert: Option<insert::Launch>,
    /// The way back from a game after Save & quit (`eject`): the case coming out of its console and home to the shelf.
    eject: Option<eject::Eject>,
    /// Back from a game: the first frame says when it was on screen (the quit's time, against the player's `quit:`).
    back_at: Option<Instant>,
    /// The scan running was asked for by choosing a folder (not the look at the start).
    scan_picked: bool,
    check: Option<CheckRun>,
    /// The first start was over at the last look (its end says where the Game check is).
    setup_over: bool,
    saves: SavesRun,
    /// Settings → Report a problem: the worker that writes the report and hands it to Android says how it went.
    report: Option<mpsc::Receiver<&'static str>>,
    /// A RetroAchievements sign-in on its way (its worker's answer), and when the launcher last looked for games the
    /// player mastered (`mastery_tick`).
    ra_login: Option<mpsc::Receiver<Result<pocket_ui::achievements::account::Account, String>>>,
    mastery_look: Instant,
    /// The saves' copies (`copy_run`): the job on its way, when the next look is due, and the notes it could not clear.
    copy: Option<copy_run::Run>,
    copy_look: Instant,
    copy_started: bool,
    copy_restore_due: bool,
    /// The notes' newest time when the folder last failed: nothing is tried again until a newer game note or a new choice.
    copy_blocked: Option<std::time::SystemTime>,
    copy_manual: bool,
    core_run: Option<core_downloads::Run>,

    update_receiver: Option<mpsc::Receiver<Result<pocket_ui::update_channel::Manifest, String>>>,
    update_url: String,
    update_after: Instant,
    update_idle: Instant,
    update_failures: u32,
    update_automatic: bool,
    /// Android's list of apps for the drawer, read on a thread at each start and return (apps come and go meanwhile).
    apps_read: Option<mpsc::Receiver<Vec<ui::apps_page::AndroidApp>>>,
}
/// Settings → Saves while Android is asked for a file or copies (`PocketActivity.exportSaves`, `importSaves`).
#[derive(Default)]
struct SavesRun {
    /// Android's side has something on its way: its status is read at each look.
    waiting: bool,
    /// Games the export on its way holds.
    exporting: usize,
    /// The backup that was read, while the page asks which saves count.
    backup: Option<pocket_ui::save_transfer::Backup>,
    /// What the import on its way does: its end says it.
    plan: Option<pocket_ui::save_transfer::ImportPlan>,
}
/// The Game check on its worker.
struct CheckRun {
    news: mpsc::Receiver<CheckNews>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
enum CheckNews {
    Progress(usize, usize, String),
    /// The check's outcome, the shelf when it changed, and the supported BIOS files found (None: a folder couldn't be read).
    Done(Box<(pocket_ui::document_check::Outcome, Option<ui::Library>, Option<Vec<document_library::Part>>)>),
    Failed,
}
/// What the Game check asks the phone (`document_check::Host`).
struct Phone;
impl pocket_ui::document_check::Host for Phone {
    fn bundled(&self, system: &str) -> bool {
        bundled_core(system).is_some()
    }
    fn installed(&self, profile: &str) -> bool {
        bridge("emulatorInstalled", &[profile]) == "true"
    }
    fn open(&self, uri: &str) -> Option<Box<dyn pocket_ui::document_check::ReadSeek>> {
        Some(Box::new(open_document(uri)?))
    }
    fn room_for_copy(&self, game: &pocket_ui::document_library::Game) -> Option<u64> {
        let key = ui::game_key_of(&game.key);
        let name = game.relative.rsplit('/').next().unwrap_or_default();
        let core = own_core(game, false)?;
        if bridge("directGameReason", &[&game.uri, core, name, if game.parts.is_empty() { "false" } else { "true" }]).is_empty() { return None; }
        u64::try_from(bridge("roomForCopy", &[&key]).parse::<i64>().ok()?).ok()
    }
}
/// SAF descriptor owned by this process, used only on document workers.
fn open_document(uri: &str) -> Option<std::fs::File> {
    use std::os::fd::FromRawFd;
    let fd: i32 = bridge("openDocument", &[uri]).parse().ok()?;
    if fd < 0 { return None; }
    Some(unsafe { std::fs::File::from_raw_fd(fd) })
}
/// One run of the check, on its worker. `known`: the shelf (`shelf_state`), to bring it the new list when the folders changed.
fn run_check(news: &mpsc::Sender<CheckNews>, known: HashMap<String, Vec<String>>, stop: &std::sync::atomic::AtomicBool) {
    let started = Instant::now();
    let listing = pocket_ui::platform::android::strings("checkListing", &[]).ok().and_then(|json| serde_json::from_str::<pocket_ui::document_check::Listing>(&json).ok());
    let Some(listing) = listing else {
        let _ = news.send(CheckNews::Failed);
        return;
    };
    let listed = started.elapsed();
    let mut last = Instant::now();
    let outcome = pocket_ui::document_check::check(&listing, &Phone, stop, &mut |done, total, system| {
        if last.elapsed() >= Duration::from_millis(150) {
            last = Instant::now();
            let _ = news.send(CheckNews::Progress(done, total, system.to_string()));
        }
    });
    let Some(mut outcome) = outcome else { return };
    log::info!("game check: {} of {} ready, {} findings, listing {listed:?}, all {:?}", outcome.report.ready, outcome.report.total, outcome.report.findings.len(), started.elapsed());
    // The shelf reads the same folders, so the two say the same afterwards; a folder that can't be read keeps its games there.
    let (library, bios) = if outcome.unreadable.is_empty() {
        let bios = document_library::player_bios(&outcome.documents);
        let games = document_library::scan(std::mem::take(&mut outcome.documents));
        ((!document_library::same_shelf(&known, &games)).then(|| ui::library(games)), Some(bios))
    } else {
        (None, None)
    };
    let _ = news.send(CheckNews::Done(Box::new((outcome, library, bios))));
}
impl App {
    fn check_start(&mut self) {
        self.check_stop();
        let (news, receiver) = mpsc::channel();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let known = document_library::shelf_state(self.ui.library.iter().flat_map(|l| l.games.iter()));
        let worker = stop.clone();
        let spawned = std::thread::Builder::new().name("game-check".into()).spawn(move || run_check(&news, known, &worker));
        if spawned.is_err() {
            self.ui.check_failed();
            return;
        }
        self.check = Some(CheckRun { news: receiver, stop });
    }
    fn check_stop(&mut self) {
        if let Some(run) = self.check.take() {
            run.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    /// The worker's news, and its end when the page was left.
    fn check_look(&mut self) {
        if !self.ui.check_running() {
            self.check_stop();
            return;
        }
        while let Some(news) = self.check.as_ref().and_then(|run| run.news.try_recv().ok()) {
            match news {
                CheckNews::Progress(done, total, system) => self.ui.check_progress(done, total, system),
                CheckNews::Done(done) => {
                    let (outcome, library, bios) = *done;
                    if let Some(library) = library {
                        i18n::learn_names(library.games.iter().map(|g| g.name.as_str()));
                        self.ui.install(library);
                        self.read_apps();
                    }
                    // A BIOS put in since the last look counts at the next game start, though the shelf is the same.
                    if let (Some(bios), Some(library)) = (bios, self.ui.library.as_mut()) {
                        library.bios = bios;
                    }
                    self.ui.check_done(outcome);
                    self.check = None;
                }
                CheckNews::Failed => {
                    self.ui.check_failed();
                    self.check = None;
                }
            }
        }
        // The dots move while it looks.
        self.redraw();
    }
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
    /// Frozen games: which started games stand open, read on a thread (it looks at their files), newest start first.
    fn frozen_refresh(&mut self) {
        let Some(lib) = &self.ui.library else { return };
        self.ui.frozen_stale = false;
        let ui = &self.ui;
        let prefs = &ui.prefs;
        let mut played: Vec<(u64, &Game)> = lib.games.iter().filter(|g| g.missing.is_empty()).filter_map(|g| Some((*prefs.played.get(&g.key)?, g))).collect();
        played.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        let data = pocket_ui::platform::data_dir();
        let candidates: Vec<_> = played
            .into_iter()
            .filter_map(|(_, g)| {
                let core = player_core(g, ui)?;
                let key = ui::game_key(g, prefs.gameboy_gambatte);
                let name = g.relative.rsplit('/').next().unwrap_or_default();
                let base = pocket_ui::frozen_phone::base(&data, &key, name);
                let mark = pocket_ui::frozen_phone::read_mark(&base);
                let path = pocket_ui::frozen_phone::staged_rom(&data, &key, name);
                let game = pocket_ui::frozen::Game { system: g.system_id.into(), path, emulator: pocket_ui::frozen_phone::EMULATOR.into(), core: core.into(), mark };
                Some((pocket_ui::frozen_phone::Candidate::from_game(g, game, base), g.parts.is_empty().then(|| (g.uri.clone(), name.to_string()))))
            })
            .collect();
        let (tx, rx) = mpsc::channel();
        self.frozen = Some(rx);
        std::thread::spawn(move || {
            // A direct document has no persistent private payload. Read it in this process with a fresh fd;
            // a dangling /proc/self alias from :player would point at an unrelated launcher descriptor.
            let mut documents = Vec::new();
            let candidates: Vec<_> = candidates.into_iter().map(|(mut candidate, direct)| {
                if !candidate.game.path.is_file() {
                    let direct = direct.filter(|(uri, name)| bridge("directGameReason", &[uri, &candidate.game.core, name, "false"]).is_empty());
                    if let Some((uri, name)) = direct {
                        if let Some(document) = open_document(&uri).and_then(|file| pocket_ui::platform::android_content::DirectGame::new(file, &data.join("direct-content").join(candidate.base.parent().and_then(|p| p.file_name()).unwrap_or_default()), &name).ok()) {
                            candidate.game.path = document.path().into();
                            documents.push(document);
                        }
                    }
                }
                candidate
            }).collect();
            let _ = tx.send(pocket_ui::frozen_phone::stand_open(&candidates));
        });
    }


    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
    fn command(&mut self, command: Command) {
        self.update_idle = Instant::now();
        match command {
            Command::CoreDownload(core) => {
                self.ui.prefs.selected_cores.insert(core);
                self.ui.prefs.core_download_consent = true;
                self.persist();
                self.cores_poll();
            }
            Command::CoreCancel => {
                bridge("coreCancel", &[]);
                self.ui.prefs.selected_cores.clear();
                self.persist();
            }
            Command::Notice(message) => { bridge("toast", &[&ui::tr(message)]); }
            Command::OpenApp(package) => {
                self.open_app(&package);
            }
            Command::Launch => {
                if self.last_launch.is_some_and(|at| at.elapsed() < Duration::from_millis(900)) {
                    return;
                }
                if let Some(g) = self.ui.game() {
                    // A game of the Android shelf is its app: started as the drawer starts it, its time counted on return.
                    if let Some(package) = ui::apps_page::package_of(g).map(str::to_string) {
                        let key = g.key.clone();
                        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                        self.last_launch = Some(Instant::now());
                        self.ui.prefs.played.insert(key.clone(), now);
                        self.ui.prefs.playing = Some((key, now));
                        self.persist();
                        if !self.open_app(&package) {
                            self.ui.prefs.playing = None;
                        }
                        return;
                    }
                    if !g.missing.is_empty() {
                        self.ui.message = format!("Missing disc files: {}", g.missing.join(", "));
                        // The shelf has no line for a message, and a Play that does nothing says nothing.
                        bridge("toast", &[&ui::tr(&self.ui.message)]);
                        self.play(pocket_ui::ui_sound::Sound::Confirm);
                        return;
                    }
                    if let Some(core) = player_core(g, &self.ui) {
                        if bridge("corePath", &[core]).is_empty() && bridge("insertRehearsal", &[]) != "true" {
                            if let Some(p) = self.ui.core_offers.iter().find(|p| p.core == core).filter(|_| self.ui.core_channel_open) {
                                bridge("offerCoreDownload", &[core, &p.package.name, &ui::core_size(p.package.size), &serde_json::to_string(&self.ui.prefs).unwrap_or_default()]);
                            }
                            self.ui.missing_core(core);
                            self.redraw();
                            return;
                        }
                    }
                    // Another app's game without that app: say which and where it comes from, and change nothing
                    // (no Continue row, no time played, no start picture).
                    if player_core(g, &self.ui).is_none() && bridge("insertRehearsal", &[]) != "true" && !pocket_ui::document_check::Host::installed(&Phone, g.profile) {
                        if let Some(said) = pocket_ui::document_check::app_missing(g.profile) {
                            bridge("notice", &[&ui::tr("App missing"), &ui::tr(said)]);
                            self.play(pocket_ui::ui_sound::Sound::Confirm);
                            return;
                        }
                    }
                    self.last_launch = Some(Instant::now());
                    let game = g.clone();
                    // Rediscover's input: when this game was last started.
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                    let reminder = if self.ui.is_frozen(&game.key) {
                        pocket_ui::where_was_i::card(&pocket_ui::where_was_i::Facts {
                            on: self.ui.prefs.remind_where.unwrap_or(true), kids: false,
                            easy: self.ui.prefs.theme == pocket_ui::settings::Theme::Big,
                            last_played: self.ui.prefs.played.get(&game.key).copied(),
                            play_secs: self.ui.prefs.play_secs.get(&game.key).copied().unwrap_or(0),
                            note: "",
                        }, now)
                    } else { None };
                    self.ui.prefs.played.insert(game.key.clone(), now);
                    // Decorations' input: the time away counts as played when the launcher is back (`game_returned`).
                    self.ui.prefs.playing = Some((game.key.clone(), now));
                    self.persist();
                    // Cozy's start, or the quick way: the same rules as the handhelds (`cabinet::starts_cozy`).
                    let frozen = self.ui.is_frozen(&game.key);
                    let cozy = pocket_ui::cabinet::starts_cozy(self.ui.prefs.cozy.unwrap_or(true), pocket_ui::motion::level(), frozen);
                    self.begin_insert(&game, cozy);
                    // Test only (`insertRehearsal` launch extra, as `externalPlayer`): the start's picture and sounds of
                    // any system, without a core or an emulator to start.
                    if bridge("insertRehearsal", &[]) == "true" {
                        if let Some(launch) = self.insert.as_mut() {
                            launch.rehearsal = true;
                        }
                        self.redraw();
                        return;
                    }
                    let core = player_core(&game, &self.ui);
                    if let Some(core) = core {
                        let key = ui::game_key(&game, self.ui.prefs.gameboy_gambatte);
                        // The shelf showed it open: "gone" if the moment went since, and no old-TV switch-on in the player.
                        log::info!("launch: {} frozen={frozen}", game.name);
                        let mut settings = serde_json::to_value(&self.ui.prefs).unwrap_or_default();
                        settings["frozen_shown"] = frozen.into();
                        settings["cheat_text"] = game.cheats.clone().into();
                        if let Some(card) = reminder {
                            settings["reminder"] = serde_json::json!({"ago":card.ago,"played":card.played,"note":card.note});
                        }
                        let settings = settings.to_string();
                        let listed = |parts: &[document_library::Part]| serde_json::Value::from(parts.iter().map(|p| serde_json::json!({"uri": p.uri, "name": p.name})).collect::<Vec<_>>()).to_string();
                        let bios: Vec<_> = self.ui.library.as_ref().map(|l| l.bios.iter().filter(|p| document_library::bios_file(&p.name).is_some_and(|b| b.0 == game.system_id)).cloned().collect()).unwrap_or_default();
                        // Explain missing firmware once per system in this launcher run; the copy worker enforces required BIOS.
                        if !self.bios_hint_said.contains(game.system_id) {
                            if let Some(hint) = document_library::bios_hint(game.system_id, &bios, &pocket_ui::platform::data_dir().join("bios")) {
                                self.bios_hint_said.insert(game.system_id.to_string());
                                bridge("toast", &[&ui::tr(hint)]);
                            }
                        }
                        let _ = pocket_ui::platform::android::strings(
                            "launchInternal",
                            &[&game.uri, &game.relative, core, &key, &game.name, game.system_id, &settings, &listed(&game.parts), &listed(&bios)],
                        );
                    } else {
                        // The app Settings → Plays with chose for this system; "": the first one on the phone.
                        let app = self.ui.plays_in_app(&game).map_or("", |a| a.package.as_str());
                        let _ = pocket_ui::platform::android::strings("launchGame", &[&game.uri, &game.relative, game.profile, app]);
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
            Command::Check => self.check_start(),
            Command::Forget(uri) => {
                bridge("forgetFolder", &[&uri]);
                self.check_start();
                self.ui.begin_check();
            }
            Command::SavesExport => {
                use pocket_ui::save_transfer;
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let plan = save_transfer::export_plan(&pocket_ui::platform::data_dir(), &self.known_games(), now);
                if plan.games == 0 {
                    self.ui.saves_said(save_transfer::NOTHING_YET);
                } else {
                    let day = bridge("localDay", &[]).parse().unwrap_or((now / 86_400) as i64);
                    self.saves = SavesRun { waiting: true, exporting: plan.games, ..Default::default() };
                    bridge("exportSaves", &[&plan.json(), &save_transfer::file_name(day)]);
                }
            }
            Command::SavesImport => {
                self.saves = SavesRun { waiting: true, ..Default::default() };
                bridge("importSaves", &[]);
            }
            Command::SavesAnswer(replace) => match self.saves.backup.take() {
                Some(backup) => self.saves_apply(backup.plan(replace)),
                None => self.ui.saves_said(ui::saves_page::STOPPED),
            },
            Command::SavesCopyNow => {
                self.copy_blocked = None;
                self.copy_manual = true;
                self.copy_start(copy_run::Job { keys: None, copy: true, restore: true });
            }
            Command::SavesCancel => {
                self.saves = SavesRun::default();
                bridge("savesCancel", &[]);
            }
            Command::RaLogin(username, password) => self.ra_login(username, password),
            Command::RaLogout => {
                let gone = pocket_ui::achievements::account::Account::remove(&data_dir());
                log::info!("retroachievements: signed out ({})", if gone { "key deleted" } else { "key still there" });
                self.ui.achievements_logged_out();
            }
            Command::ReportShare => self.report_start(true),
            Command::ReportCopy => self.report_start(false),
            Command::UpdateCheck => self.update_check(false),
            Command::UpdateInstall => { self.persist(); pocket_ui::fsutil::settle(); self.update_install(); }
            Command::UpdateCancel => { bridge("updateCancel", &[]); }
            Command::UpdateAuto => {
                self.ui.updates.hint.save(&data_dir());
                self.update_after = Instant::now() + pocket_ui::update_hint::AGAIN;
                self.update_failures = 0;
            }
            Command::None => (),
        }
        self.redraw();
    }
    /// Settings → RetroAchievements → Sign in: on a worker (the network never blocks the page). The password is used for
    /// this one request and wiped; only the token the site answers with is saved (`achievements::account`, 0600).
    fn ra_login(&mut self, username: String, mut password: String) {
        use pocket_ui::achievements::{api, http};
        let (tx, rx) = mpsc::channel();
        let agent = http::user_agent(None);
        let spawned = std::thread::Builder::new().name("ra-login".into()).spawn(move || {
            #[cfg(not(feature = "ra-replay"))]
            let transport = http::AndroidHttp { user_agent: agent };
            // QA build only: the account page signs in against the replay server (`ra_replay`).
            #[cfg(feature = "ra-replay")]
            let transport = { let _ = agent; ra_replay::Replay::new(0) };
            let result = api::login(&transport, &username, &password);
            pocket_ui::achievements_page::wipe(&mut password);
            let _ = tx.send(match result {
                Ok(account) => account.save(&data_dir()).map(|_| account).map_err(|e| format!("Couldn't keep the sign-in key: {e}")),
                Err(api::LoginError::Unreachable) => Err("Couldn't reach RetroAchievements. Is this device online?".to_string()),
                Err(why) => Err(why.to_string()),
            });
        });
        if spawned.is_err() {
            self.ui.achievements_login_done(Err("Couldn't start the sign-in.".into()));
            return;
        }
        self.ra_login = Some(rx);
    }
    /// The sign-in worker's end: the page says whether it worked (a failed one keeps the user name for another go).
    fn ra_login_look(&mut self) {
        let Some(answer) = self.ra_login.as_ref().map(|rx| rx.try_recv()) else { return };
        let answer = match answer {
            Ok(answer) => answer,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("Couldn't sign in.".into()),
        };
        self.ra_login = None;
        log::info!("retroachievements: sign-in {}", if answer.is_ok() { "worked" } else { "failed" });
        self.ui.achievements_login_done(answer);
        self.redraw();
    }
    /// A game whose every achievement the player unlocked got a mark (`achievements::mastery`): its case gets the golden
    /// "100 %" sticker, once, which counts for the trophy on the shelf. Looks every few seconds, only once the shelf is
    /// there; a game that isn't on it (left the folders) keeps its mark until it is.
    fn mastery_tick(&mut self) {
        if Instant::now() < self.mastery_look || self.ui.library.is_none() {
            return;
        }
        self.mastery_look = Instant::now() + Duration::from_secs(3);
        let data = data_dir();
        let marks = pocket_ui::achievements::mastery::pending(&data);
        if marks.is_empty() {
            return;
        }
        let mut stuck = false;
        for (key, title) in marks {
            if self.ui.master_game(&key) {
                pocket_ui::achievements::mastery::done(&data, &key);
                log::info!("retroachievements: {title:?} mastered, the 100 % sticker is on its case");
                stuck = true;
            }
        }
        if stuck {
            self.persist();
            self.redraw();
        }
    }
    /// Settings → Report a problem: what only the app knows is gathered here, then a worker asks Android for the device
    /// and the logs, writes the text (`pocket_ui::report_phone`) and hands it to the share sheet or the clipboard.
    fn report_start(&mut self, share: bool) {
        use pocket_ui::report_phone as report;
        let games: Vec<pocket_ui::report_redact::GameName> = self.ui.library.iter().flat_map(|l| l.games.iter())
            .map(|g| pocket_ui::report_redact::GameName { system: g.system_id.to_string(), name: g.name.clone(), path: std::path::PathBuf::from(&g.relative) }).collect();
        let mut systems: Vec<report::SystemLine> = Vec::new();
        let mut ids: Vec<&str> = Vec::new();
        for g in self.ui.library.iter().flat_map(|l| l.games.iter()) {
            let at = match ids.iter().position(|id| *id == g.system_id) {
                Some(at) => at,
                None => {
                    let core = match player_core(g, &self.ui) {
                        None => "played by another app".to_string(),
                        Some(core) if bridge("corePath", &[core]).is_empty() => format!("{core} (optional download, not installed)"),
                        Some(core) => core.to_string(),
                    };
                    systems.push(report::SystemLine { system: format!("{} ({})", g.system_id, g.system), games: 0, core });
                    ids.push(g.system_id);
                    systems.len() - 1
                }
            };
            systems[at].games += 1;
        }
        let downloads: Vec<(String, bool)> = self.ui.core_offers.iter().filter(|p| self.ui.prefs.selected_cores.contains(&p.core)).map(|p| (p.core.clone(), self.ui.installed_cores.contains(&p.core))).collect();
        let p = &self.ui.prefs;
        let settings = vec![
            ("Look".to_string(), p.theme.name().to_string()),
            ("Appearance".to_string(), if p.light { "light" } else { "dark" }.to_string()),
            ("Language".to_string(), i18n::current().name().to_string()),
            ("Input".to_string(), if self.ui.controller_connected { "controller" } else { "touch" }.to_string()),
        ];
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("report".into()).spawn(move || {
            let facts = report::Facts { device: bridge("deviceReport", &[]), settings, systems, downloads, log: bridge("recentLog", &[]) };
            let text = report::build(&facts, &games);
            let title = ui::tr("Share the report");
            let outcome = bridge(if share { "shareReport" } else { "copyReport" }, &[&text, &title]);
            log::info!("report: {} bytes, {}: {outcome}", text.len(), if share { "share" } else { "copy" });
            let _ = tx.send(match (outcome.as_str(), share) {
                ("ok", true) => report::SHARED,
                ("ok", false) => report::COPIED,
                (_, true) => report::NO_SHARE,
                (_, false) => report::NO_COPY,
            });
        });
        if spawned.is_err() {
            self.ui.report_said(report::FAILED);
            return;
        }
        self.report = Some(rx);
    }
    /// The report worker's end: the page says what happened (also when the page was left meanwhile, by a toast).
    fn report_look(&mut self) {
        let Some(said) = self.report.as_ref().map(|rx| rx.try_recv()) else { return };
        let said = match said {
            Ok(said) => said,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => pocket_ui::report_phone::FAILED,
        };
        self.report = None;
        self.ui.report_said(said);
        if self.ui.screen != ui::Screen::Report {
            bridge("toast", &[&ui::tr(said)]);
        }
        self.redraw();
    }
    /// The saves' copies: what a game that ended left (`save_copy::mark_pending`), what the start finds not copied, and a
    /// look at the folders' state while the page is open. Nothing here waits for Android: the work is on its worker.
    fn copy_tick(&mut self) {
        if let Some(done) = self.copy.as_ref().and_then(|run| run.poll()) {
            self.copy = None;
            self.copy_done(done);
        }
        if self.copy.is_some() || Instant::now() < self.copy_look {
            return;
        }
        self.copy_look = Instant::now() + Duration::from_secs(3);
        let data = data_dir();
        // Not before the shelf is there: the games' names and keys come with it.
        if self.ui.library.is_none() {
            return;
        }
        let noted = pocket_ui::save_copy::pending(&data);
        let stamp = noted.iter().filter_map(|k| std::fs::metadata(data.join(pocket_ui::save_copy::PENDING).join(k)).and_then(|m| m.modified()).ok()).max();
        let blocked = self.copy_blocked.is_some() && self.copy_blocked >= stamp;
        if !self.copy_started {
            // The start: copies to bring back, and games whose files were never copied (a game killed with the app).
            self.copy_started = true;
            self.copy_restore_due = false;
            let mut keys = pocket_ui::save_copy::stale(&data);
            keys.extend(noted.iter().cloned());
            keys.sort();
            keys.dedup();
            self.copy_start(copy_run::Job { keys: Some(keys), copy: true, restore: true });
        } else if self.copy_restore_due && !blocked {
            self.copy_restore_due = false;
            let mut keys = pocket_ui::save_copy::stale(&data);
            keys.extend(noted.iter().cloned());
            keys.sort();
            keys.dedup();
            self.copy_start(copy_run::Job { keys: Some(keys), copy: true, restore: true });
        } else if !noted.is_empty() && !blocked {
            self.copy_start(copy_run::Job { keys: Some(noted), copy: true, restore: false });
        } else if self.ui.screen == ui::Screen::Saves && self.ui.saves.copies == ui::saves_page::Copies::Unknown {
            let copies = copy_run::copies_of(&copy_run::targets());
            self.ui.saves_copies(copies, "");
            self.redraw();
        }
    }
    fn copy_start(&mut self, job: copy_run::Job) {
        if self.copy.is_some() {
            return;
        }
        if job.keys.as_ref().is_some_and(|k| k.is_empty()) && !job.restore {
            return;
        }
        let data = data_dir();
        let noted = pocket_ui::save_copy::pending(&data);
        self.copy = Some(copy_run::Run::start(self.known_games(), job, noted));
        if self.copy_manual {
            self.ui.saves.copying = true;
            self.redraw();
        }
    }
    fn copy_done(&mut self, done: copy_run::Done) {
        let data = data_dir();
        for line in &done.report.log {
            log::info!("{line}");
        }
        let report = &done.report;
        log::info!(
            "save copy: {} games copied ({} files, {} bytes), {} restored, {} conflicts, {} failed, {:?} in {:.1} s",
            report.copied_games, report.copied_files, report.bytes, report.restored_games, report.conflicts.len(), report.failed, report.trouble, done.seconds
        );
        let manual = std::mem::take(&mut self.copy_manual);
        // A folder that fails, or a write that fails, is said (a toast now, the page's words later) and not tried again
        // until a game ends again or the folder is chosen again: no loop, no silence.
        let problem = match &report.trouble {
            Some(trouble) => Some(trouble.sentence()),
            None if report.failed > 0 => Some(pocket_ui::save_copy::FAILED),
            None => None,
        };
        match problem {
            None => {
                self.copy_blocked = None;
                if let Err(e) = pocket_ui::save_copy::clear_pending(&data, &done.noted) {
                    log::warn!("save copy: notes not removed: {e}");
                }
            }
            Some(sentence) => {
                let stamp = pocket_ui::save_copy::pending(&data).iter().filter_map(|k| std::fs::metadata(data.join(pocket_ui::save_copy::PENDING).join(k)).and_then(|m| m.modified()).ok()).max();
                let first = self.copy_blocked.is_none() || self.copy_blocked < stamp;
                self.copy_blocked = Some(stamp.unwrap_or_else(std::time::SystemTime::now));
                if first && (!done.noted.is_empty() || manual) {
                    bridge("toast", &[&ui::tr(sentence)]);
                }
            }
        }
        let words = report.sentence().or_else(|| pocket_ui::save_copy::state_words(&data)).unwrap_or_default();
        if manual && self.ui.screen == ui::Screen::Saves {
            if let ui::saves_page::Step::Said(_) = self.ui.saves.step {
                self.ui.saves.step = ui::saves_page::Step::Rest;
            }
        }
        self.ui.saves_copies(done.copies, &words);
        // What came back shows on the Continue shelf and in the cases.
        if report.restored_games > 0 {
            self.ui.frozen_stale = true;
            if self.ui.screen != ui::Screen::Saves {
                bridge("toast", &[&report.sentence().unwrap_or_default().lines().map(ui::tr).collect::<Vec<_>>().join(" ")]);
            }
        }
        self.redraw();
    }
    /// Asks Android for its apps with a start icon on a thread (icons are files; the first read draws them).
    fn read_launcher_apps(&mut self) {
        if self.apps_read.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.apps_read = Some(rx);
        std::thread::spawn(move || {
            let json = bridge("launcherApps", &[]);
            let _ = tx.send(ui::apps_page::parse(&json, ui::apps_page::read_icon));
        });
    }
    /// Starts an app the way a home screen does; when Android can't, says so and reads the apps again (it went).
    fn open_app(&mut self, package: &str) -> bool {
        self.persist();
        let refused = bridge("launchApp", &[package]);
        if refused.is_empty() {
            log::info!("app drawer: started {package}");
            return true;
        }
        log::info!("app drawer: {package} {refused}");
        bridge("toast", &[&ui::tr("This app can't be started. It may have been removed.")]);
        self.play(pocket_ui::ui_sound::Sound::Confirm);
        self.read_launcher_apps();
        false
    }
    /// The library's games under each of their keys, for a backup's list and for finding its games here.
    fn known_games(&self) -> Vec<pocket_ui::save_transfer::Known> {
        let mut known = Vec::new();
        for g in self.ui.library.iter().flat_map(|l| l.games.iter()) {
            let file = g.relative.rsplit('/').next().unwrap_or(&g.relative).to_string();
            let entry = |key: String, core: &str| pocket_ui::save_transfer::Known { key, system: g.system_id.to_string(), file: file.clone(), title: g.name.clone(), core: core.to_string() };
            known.push(entry(ui::game_key(g, false), ""));
            if matches!(g.system_id, "gb" | "gbc") {
                known.push(entry(ui::game_key(g, true), "gambatte"));
            }
        }
        known
    }
    fn saves_apply(&mut self, plan: pocket_ui::save_transfer::ImportPlan) {
        if plan.take.is_empty() {
            // Every game of the backup keeps the phone's saves: nothing to copy.
            bridge("savesCancel", &[]);
            self.saves = SavesRun::default();
            self.ui.saves_said(&plan.sentence());
            return;
        }
        self.ui.saves.step = ui::saves_page::Step::Busy(false);
        bridge("importApply", &[&plan.json()]);
        self.saves = SavesRun { waiting: true, plan: Some(plan), ..Default::default() };
    }
    /// What Android's side says about the export or import on its way (read while one is).
    fn saves_look(&mut self) {
        use pocket_ui::save_transfer;
        use ui::saves_page;
        let status = bridge("savesStatus", &[]);
        let (state, rest) = status.split_once('\n').unwrap_or((status.as_str(), ""));
        let said = match state {
            "exported" => Some(save_transfer::exported_sentence(self.saves.exporting)),
            "export-failed" => Some(saves_page::FAILED_WRITE.to_string()),
            "import-failed" => Some(saves_page::FAILED_READ.to_string()),
            "no-room" => Some(saves_page::NO_ROOM.to_string()),
            "cancelled" => Some(saves_page::STOPPED.to_string()),
            "imported" => Some(self.saves.plan.take().map_or_else(String::new, |p| p.sentence())),
            "listed" => {
                let listing: serde_json::Value = serde_json::from_str(rest).unwrap_or_default();
                let names: Vec<String> = listing["names"].as_array().map(|a| a.iter().filter_map(|n| n.as_str().map(str::to_string)).collect()).unwrap_or_default();
                match save_transfer::read_backup(&pocket_ui::platform::data_dir(), listing["manifest"].as_str(), &names, &self.known_games()) {
                    Err(sentence) => {
                        bridge("savesCancel", &[]);
                        Some(sentence.to_string())
                    }
                    Ok(backup) if backup.here() > 0 => {
                        self.ui.saves_ask(backup.games(), backup.here());
                        self.saves = SavesRun { backup: Some(backup), ..Default::default() };
                        self.redraw();
                        None
                    }
                    Ok(backup) => {
                        self.saves_apply(backup.plan(false));
                        self.redraw();
                        None
                    }
                }
            }
            _ => None,
        };
        if let Some(sentence) = said {
            log::info!("saves: {state}: {}", sentence.replace('\n', " "));
            self.saves = SavesRun::default();
            self.ui.saves_said(&sentence);
            // Also when the page was left meanwhile: what happened is said where the player is.
            if self.ui.screen != ui::Screen::Saves {
                bridge("toast", &[&sentence.lines().map(ui::tr).collect::<Vec<_>>().join(" ")]);
            }
            self.redraw();
        }
    }
    /// The case goes from the page into its console, or the page stays when it shows no case (the list looks, the search,
    /// a card turned over): as on the handhelds, no animation without a case to start from.
    fn begin_insert(&mut self, game: &Game, cozy: bool) {
        self.insert = None;
        self.ui.launching = true;
        self.ui.follow_selection();
        let painted = self.page_frame();
        self.ui.launching = false;
        let found = self.ui.front_at.take().zip(painted);
        let Some((((x, y, fw, fh), mut front), painted_size)) = found else {
            self.play(pocket_ui::ui_sound::Sound::Confirm);
            self.redraw();
            return;
        };
        let v = self.viewport;
        // The card's canvas is smaller than the screen (`Ui::zoom`): the case at the screen's pixels.
        let (sx, sy) = (v.w as f32 / v.lw.max(1) as f32, v.h as f32 / v.lh.max(1) as f32);
        let from = ((x as f32 * sx) as i32, (y as f32 * sy) as i32, (fw as f32 * sx) as i32, (fh as f32 * sy) as i32);
        if (from.2, from.3) != (front.w, front.h) {
            front = front.scaled_smooth(from.2, from.3);
        }
        let cover = self.cover.as_ref().filter(|_| self.cover_key == game.key);
        let backdrop = std::mem::take(&mut self.scaled);
        let mut launch = insert::Launch::new(game, front, from, backdrop, (v.w, v.h), (painted_size.width, painted_size.height), cover, self.ui.prefs.extras.printed_discs, cozy, &mut self.text);
        launch.trace = bridge("diagnosticAudio", &[]) == "true";
        if cozy {
            // The player comes when the fade has run; `launchInternal` only gets it ready.
            bridge("holdLaunch", &[]);
        }
        if !self.ui.prefs.muted {
            insert::play_cues(cozy, game);
        }
        log::info!("insert: {} start cozy={cozy} at {:.1}", game.system_id, pocket_ui::platform::android::monotonic_ms());
        self.insert = Some(launch);
        // Nothing of the page behind it can be tapped or read out.
        self.ui.regions.clear();
        self.semantics = "[]".into();
        bridge("publishSemantics", &[&self.semantics]);
        self.redraw();
    }
    /// The start's picture ends now; the game comes as soon as it is ready. False when no start is on its way.
    fn skip_insert(&mut self) -> bool {
        let Some(launch) = self.insert.as_mut() else { return false };
        if launch.skip() {
            log::info!("insert: skipped at {:.1}", pocket_ui::platform::android::monotonic_ms());
            self.release_player();
            self.redraw();
        }
        true
    }
    /// Back from a game (`resumed`): its way back when the player left its last picture for it (Save & quit) and the page
    /// shows the game's case, as the start; else the page as it is. `played`: the game started last (`Preferences::playing`).
    fn begin_eject(&mut self, played: Option<String>) {
        self.eject = None;
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let Some(note) = pocket_ui::eject::LastPicture::take(&pocket_ui::platform::data_dir(), now_ms) else { return };
        let game = self.ui.game().cloned().filter(|g| played.as_deref() == Some(g.key.as_str()) && ui::game_key(g, self.ui.prefs.gameboy_gambatte) == note.key);
        let on_shelf = self.ui.shows_case();
        let cozy = self.ui.prefs.cozy.unwrap_or(true);
        if let Some(why) = eject::why_not(cozy, pocket_ui::motion::level(), on_shelf, game.is_some()) {
            log::info!("eject: not shown ({why})");
            return;
        }
        let game = game.unwrap();
        // Its first frames are the last picture switching off: nothing of the page yet, so they come as fast as the
        // page would (the page and the scene are made after the first frame, `eject_scene`).
        let Some(size) = self.window.as_ref().map(|w| w.inner_size()) else { return };
        let v = self.viewport;
        let window = (size.width, size.height);
        let frame = note.picture.as_deref().and_then(pocket_ui::image::load_png);
        let area = note.area_in(window, (v.left as i32, v.top as i32));
        let ej = eject::Eject::new(&game, frame, area, (v.w, v.h), window, Instant::now());
        if !self.ui.prefs.muted {
            ej.play_cues();
        }
        log::info!("eject: {} {:?} begin at {:.1}", game.system_id, ej.way, pocket_ui::platform::android::monotonic_ms());
        self.ui.back_from(&game.key, note.saved);
        self.eject = Some(ej);
        self.redraw();
    }
    /// The way back's scene, once its first frame is on screen: the page under it with the case's place empty (its
    /// shadow and the selection's frame kept, what is inside held in), and the start's scene from there. No case on the
    /// page after all: the shelf at once.
    fn eject_scene(&mut self) {
        let Some(key) = self.eject.as_ref().filter(|e| e.needs_scene()).map(|e| e.key.clone()) else { return };
        let game = self.ui.game().cloned().filter(|g| g.key == key);
        self.ui.launching = true;
        self.ui.returning = true;
        self.ui.follow_selection();
        let painted = self.page_frame();
        self.ui.launching = false;
        self.ui.returning = false;
        let v = self.viewport;
        let found = game.zip(self.ui.front_at.take()).zip(painted).filter(|(_, p)| self.eject.as_ref().is_some_and(|e| e.window() == (p.width, p.height) && e.size() == (v.w, v.h)));
        let Some(((game, case), painted_size)) = found else {
            log::info!("eject: ended (no case on the page)");
            self.end_eject(Instant::now());
            return;
        };
        let page = eject::Page {
            pixels: self.scaled.clone(),
            size: (v.w, v.h),
            window: (painted_size.width, painted_size.height),
            offset: (v.left as i32, v.top as i32),
            scale: (v.w as f32 / v.lw.max(1) as f32, v.h as f32 / v.lh.max(1) as f32),
        };
        let cover = self.cover.as_ref().filter(|_| self.cover_key == game.key);
        let scene = eject::scene(&game, case, page, cover, self.ui.prefs.extras.printed_discs, &mut self.text);
        if let Some(ej) = self.eject.as_mut() {
            ej.attach(scene);
        }
        log::info!("eject: scene at {:.1}", pocket_ui::platform::android::monotonic_ms());
    }
    /// A button or a touch during the way back: it ends at once, its sound waiting with it, and the input goes on to the
    /// page as always. False when none was playing.
    fn skip_eject(&mut self) -> bool {
        if self.eject.is_none() {
            return false;
        }
        log::info!("eject: skipped at {:.1}", pocket_ui::platform::android::monotonic_ms());
        pocket_ui::ui_sound::forget_waiting();
        self.end_eject(Instant::now());
        true
    }
    /// The case is home: what is inside comes a little out of it from now on (`Ui::ajar`).
    fn end_eject(&mut self, now: Instant) {
        if let Some(ej) = self.eject.take() {
            self.ui.case_home(&ej.key, now);
        }
        self.redraw();
    }
    /// A frame of the way back, as the start's (`paint_insert`).
    fn paint_eject(&mut self) -> Result<(), String> {
        let Some(window) = &self.window else { return Ok(()) };
        let size = window.inner_size();
        let v = self.viewport;
        let now = Instant::now();
        let turned = self.eject.as_ref().is_some_and(|e| e.window() != (size.width, size.height) || e.size() != (v.w, v.h));
        if turned || self.eject.as_ref().is_some_and(|e| e.done(now)) {
            self.end_eject(now);
            return self.paint();
        }
        let Some(mut ej) = self.eject.take() else { return Ok(()) };
        self.pixels.resize((v.w * v.h) as usize, 0);
        ej.paint(&mut self.pixels, now);
        let margin = ej.margin(now);
        self.eject = Some(ej);
        let Some(surface) = &mut self.surface else { return Ok(()) };
        surface.resize(NonZeroU32::new(size.width).unwrap(), NonZeroU32::new(size.height).unwrap()).map_err(|e| e.to_string())?;
        let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
        buffer.fill(margin);
        for (y, row) in self.pixels.chunks_exact(v.w as usize).enumerate() {
            let at = (y + v.top as usize) * size.width as usize + v.left as usize;
            buffer[at..at + v.w as usize].copy_from_slice(row);
        }
        buffer.present().map_err(|e| e.to_string())?;
        self.log_back(true);
        // The first frame is on screen: the page and the scene for the frames after it.
        self.eject_scene();
        Ok(())
    }
    /// The first frame after a game: when it was on screen and whether it was the way back's (the quit's time).
    fn log_back(&mut self, eject: bool) {
        if self.back_at.take().is_some() {
            log::info!("back: first frame at {:.1} eject={eject}", pocket_ui::platform::android::monotonic_ms());
        }
    }
    /// The player's turn, once.
    fn release_player(&mut self) {
        let Some(launch) = self.insert.as_mut() else { return };
        if launch.take_release(Instant::now()) {
            bridge("releaseLaunch", &[]);
        }
    }
    fn sound(&self, b: Button) {
        let sound = match b {
            Button::South => pocket_ui::ui_sound::Sound::Back,
            Button::North if self.ui.screen != ui::Screen::Box && self.ui.game().is_some() => {
                pocket_ui::ui_sound::opening(pocket_ui::media::Medium::of(self.ui.game().unwrap().system_id))
            }
            Button::East => pocket_ui::ui_sound::Sound::Confirm,
            _ => pocket_ui::ui_sound::Sound::Nav,
        };
        self.play(sound);
    }
    fn play(&self, sound: pocket_ui::ui_sound::Sound) {
        if self.ui.prefs.muted {
            return;
        }
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
        if self.skip_insert() {
            return;
        }
        self.skip_eject();
        self.sound(Button::South);
        let command = self.ui.back();
        self.command(command);
    }
    fn press(&mut self, b: Button) {
        self.skip_eject();
        // A start plays its own sounds (`begin_insert`): the button's own only when it doesn't start a game.
        let command = self.ui.press(b);
        if !matches!(command, Command::Launch) {
            self.sound(b);
        }
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
        self.skip_eject();
        // The sound of what the tap does: moving the selection or a tab is Nav, Back is Back, the rest Confirm.
        let action = self.ui.regions.iter().rev().find(|r| r.area.contains(x, y)).map(|r| r.action.clone());
        let button = match action {
            Some(ui::Action::Press(b)) => Some(b),
            Some(ui::Action::Tab(_) | ui::Action::Shelf(_) | ui::Action::Year(_)) => Some(Button::Right),
            Some(ui::Action::Row(i)) if ui::Ui::list_screen(self.ui.screen) && i != self.ui.selected => Some(Button::Right),
            Some(ui::Action::Label) | None => None,
            _ => Some(Button::East),
        };
        let command = self.ui.tap(x, y);
        if let (Some(button), false) = (button, matches!(command, Command::Launch)) {
            self.sound(button);
        }
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
        self.scan_picked = !self.selection.is_empty();
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
        if self.insert.is_some() {
            return self.paint_insert();
        }
        if self.eject.is_some() {
            return self.paint_eject();
        }
        let Some(size) = self.page_frame() else { return Ok(()) };
        let Viewport { left, top, w, h, lw, .. } = self.viewport;
        let Some(surface) = &mut self.surface else { return Ok(()) };
        surface
            .resize(NonZeroU32::new(size.width).unwrap(), NonZeroU32::new(size.height).unwrap())
            .map_err(|e| e.to_string())?;
        let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
        buffer.fill(pocket_ui::palette::pal().void);
        for (y, row) in self.scaled.chunks_exact(w as usize).enumerate() {
            let at = (y + top as usize) * size.width as usize + left as usize;
            buffer[at..at + w as usize].copy_from_slice(row);
        }
        buffer.present().map_err(|e| e.to_string())?;
        self.log_back(false);
        let lh = self.viewport.lh;
        let regions:Vec<_>=self.ui.regions.iter().enumerate().map(|(id,r)|serde_json::json!({"id":id,"label":r.label,"selected":self.ui.selected_region(r),"x":left+r.area.x.max(0) as u32*w/lw,"y":top+r.area.y.max(0) as u32*h/lh,"w":r.area.w.max(0) as u32*w/lw,"h":r.area.h.max(0) as u32*h/lh})).collect();
        let semantics = serde_json::to_string(&regions).unwrap_or_default();
        if semantics != self.semantics {
            self.semantics = semantics;
            bridge("publishSemantics", &[&self.semantics]);
        }
        Ok(())
    }
    /// A frame of the start: the scene at the safe area's pixels, the rest of the screen going dark with it.
    fn paint_insert(&mut self) -> Result<(), String> {
        let Some(window) = &self.window else { return Ok(()) };
        let size = window.inner_size();
        let v = self.viewport;
        let Some(mut launch) = self.insert.take() else { return Ok(()) };
        // Turned while it plays: the case's place on the old page means nothing any more, the game comes now.
        if (size.width, size.height) != launch.window || launch.size != (v.w, v.h) {
            self.insert = Some(launch);
            self.skip_insert();
            if let Some(launch) = self.insert.as_mut() {
                launch.window = (size.width, size.height);
                launch.size = (v.w, v.h);
                launch.drop_backdrop();
            }
            return Ok(());
        }
        let now = Instant::now();
        self.pixels.resize((v.w * v.h) as usize, 0);
        launch.paint(&mut self.pixels, &mut self.text, now);
        let (margin, trace, elapsed) = (launch.margin(now), launch.trace, launch.elapsed(now));
        self.insert = Some(launch);
        let Some(surface) = &mut self.surface else { return Ok(()) };
        surface.resize(NonZeroU32::new(size.width).unwrap(), NonZeroU32::new(size.height).unwrap()).map_err(|e| e.to_string())?;
        let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
        buffer.fill(margin);
        for (y, row) in self.pixels.chunks_exact(v.w as usize).enumerate() {
            let at = (y + v.top as usize) * size.width as usize + v.left as usize;
            buffer[at..at + v.w as usize].copy_from_slice(row);
        }
        buffer.present().map_err(|e| e.to_string())?;
        if trace {
            log::info!("insert-frame {:.1} {:.1}", pocket_ui::platform::android::monotonic_ms(), elapsed.as_secs_f64() * 1000.0);
        }
        Ok(())
    }
    /// The page in `self.scaled` at the screen's pixels, not yet on screen. None without a window.
    fn page_frame(&mut self) -> Option<winit::dpi::PhysicalSize<u32>> {
        let window = self.window.as_ref()?;
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return None;
        }
        let values: Vec<f32> = bridge("windowMetrics", &[]).split(',').filter_map(|n| n.parse().ok()).collect();
        let density = values.first().copied().unwrap_or(1.0);
        self.ui.system_font_scale = values.get(1).copied().unwrap_or(1.0);
        self.ui.system_bar = values.get(6).copied() == Some(1.0);
        let inset = |i: usize, max: u32| values.get(i).copied().unwrap_or(0.0).max(0.0).min(max as f32 / 4.0) as u32;
        let left = inset(2, size.width);
        let top = inset(3, size.height);
        let w = size.width - left - inset(4, size.width);
        let h = size.height - top - inset(5, size.height);
        // The phone layout is sized in dp itself, so the canvas is the screen's own pixels (sharp type, no
        // resampling); the shared game card gets a smaller canvas that is enlarged (`Ui::zoom`).
        let (lw, lh) = self.ui.canvas_for((w, h), density);
        let viewport = Viewport { left, top, w, h, lw, lh };
        if self.viewport != viewport {
            log::info!("launcher viewport: {viewport:?} density={density}");
        }
        self.viewport = viewport;
        self.pixels.resize((lw * lh) as usize, 0);
        let mut c = Canvas::new(&mut self.pixels, lw, lh);
        self.ui.paint(&mut c, &mut self.text, self.cover.as_ref());
        self.scaled.resize((w * h) as usize, 0);
        self.scaler.downscale(&self.pixels, (lw, lh), &mut self.scaled, (w, h));
        Some(size)
    }
}
impl ApplicationHandler<BridgeEvent> for App {
    fn user_event(&mut self, _: &ActiveEventLoop, event: BridgeEvent) {
        if matches!(event,BridgeEvent::Configuration) {
            self.ui.cover_changed();
            self.redraw();
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
        let played = self.ui.prefs.playing.as_ref().map(|(key, _)| key.clone());
        if played.is_some() {
            self.back_at = Some(Instant::now());
            log::info!("back: resumed at {:.1}", pocket_ui::platform::android::monotonic_ms());
        }
        self.ui.game_returned();
        // An emulator app may have come or gone while we were away (Settings → Plays with offers what is on the phone).
        self.read_apps();
        self.persist();
        self.refresh();
        self.read_launcher_apps();
        self.begin_eject(played);
        self.redraw();
        let installed = bridge("updateNotice", &[]);
        if !installed.is_empty() {
            let notice = ui::tr(&format!("Updated to {installed}"));
            bridge("toast", &[&notice]);
            log::info!("update notice: {notice}");
        }
    }
    fn suspended(&mut self, _: &ActiveEventLoop) {
        match self.insert.take() {
            Some(launch) => {
                // Home while the game was on its way, still being copied too: nothing comes up behind the player's back.
                let stopped = bridge("cancelLaunch", &[]) == "true";
                // The player took over: the start's last sounds play out under it, as on the handhelds.
                if launch.released() && !stopped {
                    pocket_ui::ui_sound::close_when_done(false);
                } else {
                    pocket_ui::ui_sound::stop();
                }
            }
            None => pocket_ui::ui_sound::stop(),
        }
        if self.eject.take().is_some() {
            self.ui.ajar = None;
        }
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
                // A start on its way: a touch skips it and does nothing else.
                if self.insert.is_some() {
                    if t.phase == TouchPhase::Started {
                        self.skip_insert();
                        self.finger = None;
                    }
                    self.swallow_touch = true;
                    return;
                }
                if self.swallow_touch {
                    self.swallow_touch = !matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled);
                    return;
                }
                // The way back ends at the first touch, which goes on to the page.
                if t.phase == TouchPhase::Started {
                    self.skip_eject();
                }
                let (x, y) = (t.location.x, t.location.y);
                let now = Instant::now();
                match t.phase {
                    TouchPhase::Started => {
                        if self.finger.is_none() {
                            let at = self.point(x, y);
                            self.ui.hold_at(at.1);
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
            } => {
                if !self.skip_insert() {
                    self.tap(self.cursor.0, self.cursor.1);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                // Any button skips a start on its way; the finger still on the A that started it doesn't count.
                if self.insert.is_some() {
                    if !event.repeat {
                        self.skip_insert();
                    }
                    return;
                }
                // Any button ends the way back and does what it does on the page; a button still held from the
                // player's Save & quit (its repeats) does neither.
                if self.eject.is_some() {
                    if event.repeat {
                        return;
                    }
                    self.skip_eject();
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
                    PhysicalKey::Code(KeyCode::Enter | KeyCode::Space) => Some(Button::East),
                    PhysicalKey::Code(KeyCode::Escape) => Some(Button::South),
                    PhysicalKey::Code(KeyCode::KeyX) => Some(Button::North),
                    PhysicalKey::Code(KeyCode::KeyY) => Some(Button::West),
                    PhysicalKey::Code(KeyCode::Tab) => Some(Button::Select),
                    PhysicalKey::Code(KeyCode::KeyS) => Some(Button::Start),
                    // The pad's own keys, L2/R2/L3/R3 included (`pad::android`, shared with the player).
                    PhysicalKey::Unidentified(NativeKeyCode::Android(key)) => pocket_ui::pad::android::button(key as i32),
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
                    // Games came by choosing a folder: the Game check offers to look at them, as after Add games on a handheld.
                    let known: HashSet<&str> = self.ui.library.iter().flat_map(|l| l.games.iter().map(|g| g.key.as_str())).collect();
                    let added = lib.games.iter().filter(|g| !known.contains(g.key.as_str())).count();
                    self.ui.install(lib);
                    // A chosen folder may hold copies of saves for games that have none here (after installing again).
                    self.copy_restore_due = true;
                    self.copy_blocked = None;
                    self.read_apps();
                    if self.scan_picked && self.ui.prefs.onboarding_done {
                        if self.ui.check_open() {
                            // Chosen from the page itself: it looks again at once.
                            self.ui.begin_check();
                            self.check_start();
                        } else if added > 0 {
                            self.ui.offer_check(added);
                        } else if let Some(note) = ui::Ui::folder_without_new_games(added) {
                            bridge("toast", &[&note]);
                        }
                    }
                }
                Err(e) => {
                    log::error!("library scan: {e}");
                    self.ui.message = "Could not read the game folders. Choose your folder again.".into();
                    if self.scan_picked && self.ui.check_open() {
                        self.ui.begin_check();
                        self.check_start();
                    }
                }
            }
            self.scan = None;
            self.ui.scan_running = false;
            self.redraw();
        }
        if let Some(list) = self.apps_read.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.apps_read = None;
            log::info!("app drawer: {} apps", list.len());
            self.ui.apps_got(list);
            self.persist();
            self.redraw();
        }
        if self.ui.frozen_stale {
            self.frozen_refresh();
        }
        if let Some(open) = self.frozen.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.frozen = None;
            log::info!("frozen games standing open: {}", open.len());
            // Changed under the way back (the shelf made again): its page is gone, the case is home at once.
            if self.ui.frozen_got(open) && self.eject.is_some() {
                log::info!("eject: ended, the shelf changed");
                self.end_eject(Instant::now());
            }
            self.redraw();
        }
        if self.check.is_some() {
            self.check_look();
        }
        if self.report.is_some() {
            self.report_look();
        }
        if self.ra_login.is_some() {
            self.ra_login_look();
        }
        self.mastery_tick();
        // The first start is over: a short note says where the Game check is, when it brought games.
        if self.ui.prefs.onboarding_done != self.setup_over {
            self.setup_over = self.ui.prefs.onboarding_done;
            let games = self.ui.library.as_ref().map_or(0, |l| l.games.len());
            if self.setup_over && games > 0 {
                let line = if games == 1 { "1 new game. Game check in Settings looks it over.".to_string() } else { format!("{games} new games. Game check in Settings looks them over.") };
                bridge("toast", &[&ui::tr(&line)]);
            }
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
            if connected != self.ui.controller_connected {
                // A controller was just connected: show its hints until the screen is touched again.
                self.ui.touch_mode = !connected;
                self.redraw();
            }
            self.ui.controller_connected = connected;
            let saver = bridge("batterySaver", &[]) == "true";
            if saver != self.battery_saver {log::info!("Android battery saver={saver}");}
            self.battery_saver = saver;
            self.ui.wifi_ready = bridge("unmeteredWifi", &[]) == "true";
            self.cores_poll();
            self.corner_poll();

            self.updates_tick();
            let policy = self.ui.prefs.download_media == Some(true) && self.ui.wifi_ready && !self.battery_saver;
            if policy != self.media_policy {
                self.media_policy = policy;
                self.cover_key.clear();
            }
            if !self.ui.prefs.onboarding_done {
                self.redraw();
            }
            if self.saves.waiting {
                self.saves_look();
            }
            self.copy_tick();
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
        if self.insert.is_some() {
            return self.insert_tick(event_loop, now);
        }
        if self.eject.is_some() {
            self.redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(16)));
            return;
        }
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
            // A held app in the drawer goes onto the Android shelf or off it: the tick's sound.
            if self.ui.screen == ui::Screen::Apps && self.ui.regions.iter().rev().find(|r| r.area.contains(x, y)).is_some_and(|r| matches!(r.action, ui::Action::App(_))) {
                self.sound(Button::East);
            }
            let command = self.ui.long_press(x, y);
            self.command(command);
        }
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
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(16)));
        } else if let Some(at) = long_at {
            event_loop.set_control_flow(ControlFlow::WaitUntil(at.min(self.check_at)));
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.check_at));
        }
    }
}
impl App {
    /// Which emulator apps the phone has for the systems of our own player (Settings → Plays with).
    fn read_apps(&mut self) {
        let profiles = self.ui.plays_profiles();
        if profiles.is_empty() {
            return;
        }
        let json = bridge("installedApps", &[&profiles]);
        // Nothing came back (the bridge failed): keep what was known rather than offer no app at all.
        if !json.is_empty() {
            self.ui.set_installed_apps(&json);
        }
    }
    /// While a start is on its way: frames, the player's turn, and back to the page when the game couldn't start.
    fn insert_tick(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let failed = bridge("launchFailed", &[]) == "true";
        // A slow copy isn't a player that never came: its words stay up and Home leaves it (`suspended`).
        let gave_up = self.insert.as_ref().is_some_and(|l| l.gave_up(now)) && bridge("launchOnItsWay", &[]) != "true";
        if failed || gave_up {
            // Nothing comes up behind the shelf later. The dialog says why (`PocketActivity.error`).
            bridge("cancelLaunch", &[]);
            self.insert = None;
            pocket_ui::ui_sound::forget_waiting();
            self.redraw();
            return;
        }
        self.release_player();
        if self.insert.as_ref().is_some_and(|l| l.rehearsal && l.released() && l.at_end(now)) {
            self.insert = None;
            self.redraw();
            return;
        }
        let settled = !self.insert.as_ref().is_some_and(|l| l.needs_frame(now));
        if !settled {
            // From the player's turn on Android pauses the launcher, and winit asks for no frame any more
            // (`RedrawRequested`), while its window shows until the player's covers it: the fade is painted here to its
            // end. 12.11. the start stood still, half dark, for the player's last 250 ms (`insert::PLAYER_START`).
            if self.insert.as_ref().is_some_and(|l| l.released()) {
                if let Err(e) = self.paint_insert() {
                    log::error!("paint: {e}");
                }
            } else {
                self.redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(if settled { 100 } else { 16 })));
    }
}

#[no_mangle]
fn android_main(android: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_tag("PocketMvp").with_max_level(log::LevelFilter::Info));
    unsafe {
        pocket_ui::platform::android::initialize(android.vm_as_ptr().cast(), android.activity_as_ptr().cast()).expect("Activity bridge");
    }
    let prefs: Preferences = pocket_ui::fsutil::read_json_or_backup(&pocket_ui::platform::data_dir().join("launcher.json")).unwrap_or_default();
    i18n::set(prefs.chosen_language().unwrap_or_else(|| i18n::Lang::from_locale(&bridge("language", &[]))));
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
            let download = wanted && bridge("unmeteredWifi", &[]) == "true" && bridge("batterySaver", &[]) != "true";
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
    let mut builder = EventLoop::<BridgeEvent>::with_user_event();
    builder.with_android_app(android);
    let event_loop = builder.build().expect("Android event loop");
    *BACK_PROXY
        .get_or_init(|| std::sync::RwLock::new(None))
        .write()
        .unwrap_or_else(|e| e.into_inner()) = Some(event_loop.create_proxy());
    let setup_over = prefs.onboarding_done;
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
        frozen: None,
        check_at: Instant::now(),
        corner_at: Instant::now(),
        media,
        media_policy: false,
        battery_saver: false,
        media_results,
        cover_key: String::new(),
        cover: None,
        last_launch: None,
        eject: None,
        back_at: None,
        bios_hint_said: Default::default(),
        cursor: (0.0, 0.0),
        finger: None,
        last_tick: Instant::now(),
        semantics: String::new(),
        last_persist: String::new(),
        swallow_touch: false,
        insert: None,
        scan_picked: false,
        check: None,
        saves: SavesRun::default(),
        report: None,
        ra_login: None,
        mastery_look: Instant::now(),
        copy: None,
        copy_look: Instant::now(),
        copy_started: false,
        copy_restore_due: true,
        copy_blocked: None,
        copy_manual: false,
        core_run: None,
        update_receiver: None,
        update_url: bridge("updateChannel", &[]),
        update_after: Instant::now() + pocket_ui::update_hint::FIRST_LOOK,
        update_idle: Instant::now(),
        update_failures: 0,
        update_automatic: false,
        apps_read: None,
        setup_over,
    };
    app.ui.achievements_remember(pocket_ui::achievements::account::Account::load(&pocket_ui::platform::data_dir()).as_ref());
    app.ui.core_offers = pocket_ui::android_cores::packages(bridge("coreOffers", &[]).as_bytes()).unwrap_or_default();
    // The licences of every core, the optional downloads too (a list the build wrote from the same manifest as the source archives).
    app.ui.licences.load(&bridge("licenceList", &[]));
    app.ui.core_channel_open = !core_channel_url(&app.ui.prefs).is_empty();
    for p in &app.ui.core_offers {
        if !bridge("corePath", &[&p.core]).is_empty() { app.ui.installed_cores.insert(p.core.clone()); }
    }
    app.ui.updates.version = bridge("updateVersion", &[]);
    app.ui.updates.hint = pocket_ui::update_hint::State::load(&data_dir());
    log::info!("android_main: shared launcher starting");
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("event loop: {e}");
    }
}

/// Where the system download channel is: an intent's address (tests), else the one the APK was built with
/// (`android/core-channel.url`, empty until the channel is public), else a saved setting. Empty: not public.
fn core_channel_url(prefs: &ui::Preferences) -> String {
    let built_in = bridge("coreCatalogUrl", &[]);
    if built_in.is_empty() { prefs.core_catalog_url.clone() } else { built_in }
}

impl App {
    /// The clock and the battery for the header's status corner: asked of the Activity every few seconds and as the
    /// minute turns, with one redraw only when something changed (the page is not repainted otherwise).
    fn corner_poll(&mut self) {
        if Instant::now() < self.corner_at {
            return;
        }
        let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        self.corner_at = Instant::now() + pocket_ui::phone_corner::Reading::next_look(since);
        let reading = pocket_ui::phone_corner::Reading::parse(&bridge("statusCorner", &[]));
        if reading != self.ui.corner {
            self.ui.corner = reading;
            self.redraw();
        }
    }
    fn cores_poll(&mut self) {
        // Nothing is requested, started or retried while there is no public channel: the systems read "Coming soon".
        if !self.ui.core_channel_open { return; }
        let requested = bridge("coreRequested", &[]);
        if self.ui.core_offers.iter().any(|p| p.core == requested) {
            self.ui.prefs.selected_cores.insert(requested);
            self.ui.prefs.core_download_consent = true;
            self.persist();
        }
        if let Some(run) = &self.core_run {
            if let Some(result) = run.poll() {
                let core = run.core.clone();
                self.ui.prefs.selected_cores.remove(&core);
                self.ui.core_busy = None;
                self.ui.core_note = match result {
                    Ok(()) => { self.ui.installed_cores.insert(core); ui::tr("System installed. You can start your game now.") }
                    Err(e) => { log::warn!("core download: {e}"); ui::tr(&e) }
                };
                self.core_run = None;
                self.persist();
                self.redraw();
            } else {
                let bytes: u64 = bridge("coreProgress", &[]).parse().unwrap_or(0);
                let total = self.ui.core_offers.iter().find(|p| p.core == run.core).map_or(0, |p| p.package.size);
                self.ui.core_note = format!("{} · {} / {}", ui::tr("Downloading"), ui::core_size(bytes), ui::core_size(total));
                self.redraw();
            }
            return;
        }
        if !self.ui.prefs.core_download_consent || (!self.ui.prefs.onboarding_done && self.ui.prefs.onboarding_step <= 5) { return; }
        let Some(p) = self.ui.core_offers.iter().find(|p| self.ui.prefs.selected_cores.contains(&p.core) && !self.ui.installed_cores.contains(&p.core)).cloned() else { return };
        if !self.ui.wifi_ready {
            self.ui.core_note = ui::tr("Downloads wait for unmetered Wi-Fi.");
            return;
        }
        let url = core_channel_url(&self.ui.prefs);
        bridge("coreBegin", &[]);
        self.ui.core_busy = Some(p.core.clone());
        self.core_run = Some(core_downloads::Run::start(p, url));
    }
}
