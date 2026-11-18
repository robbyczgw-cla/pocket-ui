//! pocket-core inside the APK: shared libretro host, save semantics and audio engine on the EGL thread.
use jni::{
    objects::{JObject, JString},
    sys::{jboolean, jfloat, jint, jstring},
    JNIEnv,
};
use pocket_ui::{
    core_options::FileOption,
    gfx::Canvas,
    image::Image,
    libretro::Core,
    palette::{self, pal},
    pause_head::{self, EDGE, GAP, TARGET_MIN},
    pause_list,
    pause_options,
    text::{Face, Text},
    type_scale,
    touch::Area,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[path = "picture.rs"]
mod picture;
mod player_speed;
#[path = "play_cards.rs"]
mod play_cards;
thread_local! {static PLAYER:RefCell<Option<Player>>=const{RefCell::new(None)};}
#[path = "player_graphics.rs"]
mod player_graphics;
use player_graphics::Graphics;
#[path = "player_video.rs"]
mod player_video;
use player_video::png;
#[path = "player_touch.rs"]
mod player_touch;
use player_touch::touch_controls;
#[derive(Clone, Copy, PartialEq)]
enum Menu {
    Pause,
    Options,
    Controls,
    Recovery,
    Zoom,
    /// The game's RetroAchievements, as a list to read (a row of the pause menu while signed in).
    Achievements,
}
#[path = "moments.rs"]
mod moments;
#[path = "player_achievements.rs"]
mod player_achievements;
#[cfg(feature = "ra-replay")]
#[path = "ra_replay.rs"]
mod ra_replay;
// Dereferencing the core makes its isolated context current for every entry point,
// including save/load, options and destruction; composition explicitly restores the Activity.
fn core_hw_frame(core: &CoreSession) -> Option<pocket_ui::libretro::Frame> {
    core.hw.as_ref()?.capture()
}
struct CoreSession {
    core: std::mem::ManuallyDrop<Core>,
    hw: Option<pocket_ui::hw_render::HwContext>,
}
impl CoreSession {
    fn new(core: Core) -> Result<Self, String> {
        let g = core.av.geometry;
        let hw = pocket_ui::hw_render::HwContext::create(g.max_width.max(g.base_width), g.max_height.max(g.base_height))?;
        log::info!(
            "Android core renderer: {}",
            if hw.is_some() { "GLES 3 (isolated EGL context)" } else { "software" }
        );
        Ok(Self {
            core: std::mem::ManuallyDrop::new(core),
            hw,
        })
    }
    fn frontend_current(&self) {
        if let Some(hw) = &self.hw {
            hw.frontend_current();
        }
    }
}
impl std::ops::Deref for CoreSession {
    type Target = Core;
    fn deref(&self) -> &Core {
        if let Some(hw) = &self.hw {
            hw.core_current();
        }
        &self.core
    }
}
impl std::ops::DerefMut for CoreSession {
    fn deref_mut(&mut self) -> &mut Core {
        if let Some(hw) = &self.hw {
            hw.core_current();
        }
        &mut self.core
    }
}
impl moments::States for CoreSession {
    fn snapshot(&mut self) -> Option<Vec<u8>> { self.save_state() }
    fn restore(&mut self, bytes: &[u8]) -> Result<(), (bool, String)> { self.load_state_reached(bytes) }
    fn block_saves(&mut self, blocked: bool) { std::ops::DerefMut::deref_mut(self).block_saves(blocked); }
}
impl Drop for CoreSession {
    fn drop(&mut self) {
        if let Some(hw) = &mut self.hw {
            hw.destroy_callback();
            hw.core_current();
        }
        unsafe {
            std::mem::ManuallyDrop::drop(&mut self.core);
        }
    }
}
struct Player {
    // First, so it is dropped (its session closed) before the core, whose memory it reads, goes.
    ra: Option<player_achievements::Achievements>,
    /// The Achievements page's lines, read from the session's list at most once a second.
    ra_lines: Vec<String>,
    ra_read: Option<Instant>,
    core: CoreSession,
    audio: Option<pocket_ui::audio::Audio>,
    gfx: Graphics,
    text: Text,
    pixels: Vec<u32>,
    overlay_size: Option<(u32, u32)>,
    touch_visible: bool,
    frame: Option<std::sync::Arc<Image>>,
    game_area: Option<Area>,
    /// Where the touch pad reaches (a frame stays clear of it): for the layout it was measured on (size, picture, system,
    /// density), computed once per layout.
    pad_extent: Option<(((u32, u32), (i32, i32, i32, i32)), String, u32, (i32, i32))>,
    /// The window's size at the last frame, in pixels (the launcher's way back places the last picture by it).
    window: (u32, u32),
    stylus_finger: Option<i32>,
    stylus: pocket_ui::platform::android_pointer::Pointer,
    title: String,
    system: String,
    base: PathBuf,
    data: PathBuf,
    key: String,
    /// What this session's states are made with (`frozen_phone`), written beside the auto state.
    mark: Option<pocket_ui::frozen::Mark>,
    // Declared after core: drop the core before closing its source and removing its path.
    _document: Option<pocket_ui::platform::android_content::DirectGame>,
    /// Lines over the game's first moments about its start (`frozen_phone`), where they are and until when.
    note: Vec<String>,
    note_until: Option<Instant>,
    note_area: Area,
    /// Where the achievement card was painted last (the accessibility tree names it).
    card_area: Area,
    card_label: String,
    first_frame: bool,
    paused: bool,
    resumed: bool,
    suspended: bool,
    control_names: Vec<pocket_ui::controls::Target>,
    hold_button: Option<u8>,
    held: pocket_ui::controls::HoldToggle,
    ds_pad: pocket_ui::ds_pad::Pad,
    haptics: bool,
    text_size: pocket_ui::text_size::TextSize,
    last_font_scale: f32,
    reminder: Option<pocket_ui::where_was_i::Card>,
    zoom: play_cards::Zoom,
    zoom_fingers: HashMap<i32, (f32, f32)>,
    zoom_distance: Option<f32>,
    crt_started: Option<Instant>,
    crt_done: bool,
    light: bool,
    menu: Menu,
    row: usize,
    slot: usize,
    filled_slots: [bool; 8],
    /// Moments set aside with every slot taken: the picker's places after the slots (`frozen_phone::kept`).
    kept: Vec<PathBuf>,
    slot_icon: Option<pocket_ui::system_art::Sprite>,
    sounds: pocket_ui::ui_sound::Config,
    sounds_muted: bool,
    message: String,
    quit: bool,
    picture: picture::Settings,
    options: Vec<FileOption>,
    saved_options: std::collections::BTreeMap<String, String>,
    options_path: PathBuf,
    buttons: u16,
    pulses: u16,
    stick_buttons: u16,
    /// L2/R2 when the pad reports them as axes (`pad::android::Trigger`); their bits join `stick_buttons`.
    triggers: [pocket_ui::pad::android::Trigger; 2],
    /// The gamepads, who plays which player, and the buttons of players 2 to 4 (`android_pads`).
    pads: pocket_ui::android_pads::Pads,
    /// Where the pad behind the event being handled has its A/B/X/Y (`Pads::arrangement_of`).
    arrangement: pocket_ui::pad::android::Arrangement,
    /// The buttons the core was given last (a change is logged: what the pads did, as the game saw it).
    last_given: [u16; 4],
    stick_direction: i32,
    /// A direction held on a hat or stick in the pause menu repeats like a held key (`pad::android::DirectionRepeat`);
    /// `clock` is its time.
    hat_repeat: pocket_ui::pad::android::DirectionRepeat,
    clock: Instant,
    menu_drag: Option<(i32, f32, f32)>,
    // Touch pause menu: the finger whose tap only revealed it, the Options page's pixel scroll and its drag, and the
    // targets of the last paint (slot arrows, option arrows `(area, row, step, label)`).
    swallow: Option<i32>,
    scroll: i32,
    scroll_max: i32,
    /// The options page's "Advanced (this emulator)" group is open.
    advanced_open: bool,
    /// Overlay pixels per dp (from the last frame's density).
    dp: f32,
    /// The touch D-pad: its centre and radius in overlay pixels (8-way from the finger's angle).
    dpad: Option<(f32, f32, f32)>,
    scroll_base: i32,
    dragging: bool,
    slot_arrows: [Area; 2],
    /// The pause sheet's buttons and slot as last painted (`pause_head::places`): the D-pad walks them as they look.
    pad_places: Vec<(usize, Area)>,
    /// The column a run of Up/Down keeps on the pause sheet, with the row it belongs to (`pause_head::grid_step`).
    pad_anchor: Option<(usize, i32)>,
    option_arrows: Vec<(Area, usize, i32, String)>,
    fingers: HashMap<i32, u16>,
    controls: Vec<(Area, u16)>,
    menu_button: Area,
    hits: Vec<(Area, usize, String)>,
    semantics: String,
    top: i32,
    view: (i32, i32, i32, i32),
    logical: (u32, u32),
    /// Core frames per display refresh, Linux's rule (`core_pacing`): one per refresh near the display's rate.
    pacer: pocket_ui::core_pacing::Pacer,
    /// The `player pacing:` line's numbers (every 10 s with the audio line).
    pacing: pocket_ui::core_pacing::Stats,
    /// The core's thread off the little cores while a game plays (`core_pacing::Placement`): a light SNES frame placed on
    /// a little core missed the refresh (Original Mode 7, Mini V2, 17.11.2026).
    placement: pocket_ui::core_pacing::Placement,
    display_hz: f64,
    frames: u64,
    /// Fresh pictures delivered by the core, separate from emulation ticks and display presentations.
    video_frames: u64,
    diagnostic_buttons_seen: u16,
    last_flush: Instant,
    save_trouble: Option<pocket_ui::save_trouble::Trouble>,
    moment_failed: bool,
    timeline: pocket_ui::timeline::Timeline,
    timeline_paused: Option<Instant>,
    cheats: Option<pocket_ui::cheats::CheatMenu>,
    recovery_required: bool,
    /// A game from the Handheld view has the handhelds' pause menu (`pause_list`): the rows under each other, the
    /// save's picture beside them, Zoom and Screenshot under Options. The Mobile view keeps its touch sheet.
    list_menu: bool,
    /// The row of `list_rows` the list menu is on.
    list_sel: usize,
    /// Earlier's moments of this game, newest first (`refresh_earlier`), and the one Earlier is on.
    earlier_row: pocket_ui::timeline::Row,
    /// The slots' and Earlier's pictures, read once while the menu is open.
    previews: HashMap<PathBuf, Option<std::sync::Arc<Image>>>,
    /// Where the list menu's rows were painted, for a finger (`pause_list::Out`).
    list_hits: Vec<(Area, usize)>,
    /// The list menu's hint keys as painted: a finger on "B Back" goes back, on "A OK" does the row.
    list_hint_hits: Vec<(Area, pocket_ui::pad::Button)>,
    /// Zoom was opened from Options (the list menu): B goes back there.
    from_options: bool,
    /// The list menu's Options pages (`pause_options::OptNav`, Linux's): the page, the selected row (an index of
    /// `list_option_rows`), the Emulator page's category, and where B goes back to.
    nav: pause_options::OptNav,
    /// Restore defaults was pressed once: the next A on it does it, any other key takes it back (Linux's).
    restore_armed: bool,
    /// The core's categories of options (`Core::options_snapshot`), for the Emulator page.
    option_categories: Vec<pocket_ui::core_options::FileCategory>,
    /// A game of several discs: the Disc row of the main page, on the disc it shows (`pocket_ui::disc`).
    disc_row: Option<pocket_ui::disc::DiscRow>,
    /// The last action of the sheet (`activate`) worked: Save and Load go back to the game, as Linux's.
    last_ok: bool,
    /// The Picture page's live preview (`preview::Live`, Linux's): the game drawn with the Picture choices as they are now
    /// (`Graphics::render_preview`), for this key of choices and layout; pending while a frame is still being made.
    live: pocket_ui::preview::Live,
    /// The Picture values (Linux's, `VideoSettings`: Look, frame and its finish, scaling) and the layout the preview was made for.
    live_key: Option<(usize, pocket_ui::video::VideoSettings, (i32, i32, i32, i32))>,
    live_pending: bool,
    live_tries: u32,
    /// The screen's density as the last frame reported it.
    density: f32,
    /// The window less the safe insets (x, y from the bottom, w, h): where the game's overlay goes, also while the
    /// handhelds' menu fills the whole window (`view`).
    game_view: (i32, i32, i32, i32),
    /// Options → Zoom in the handhelds' menu (`open_list_zoom`): the part in view, and the screen it is a part of with the
    /// game's rectangle in it.
    list_zoom: Option<pocket_ui::zoom::Zoom>,
    zoom_shot: Option<(Image, pocket_ui::preview::GameRect)>,
    /// Save with Animations on Cozy and Motion on Full (`save_fill`, Linux's): the saved picture flies into its slot, the
    /// game stays paused meanwhile and goes on after `save_fill::LEN` or at any button; the line it says then, and
    /// whether the click as it lands has played.
    saving: Option<pocket_ui::save_fill::Saving>,
    saving_said: String,
    saving_click: bool,
    /// The selected achievement on Linux's Achievements page (the handhelds' menu).
    ach_sel: usize,
    /// Options › Controls, Linux's page (`controls::ControlsPage`): what each of the handheld's buttons presses, the
    /// sticks' rows, Hold to toggle; the choices are `ctl.*` in the game's settings (`picture`, Linux's files and Keep for).
    pad_page: pocket_ui::controls::ControlsPage,
    /// The left stick as the last motion left it, for D-pad as left stick (`controls::dpad_to_stick`).
    axes: [i16; 4],
    /// The touch pad's presses too short for a frame: the console's own buttons, not translated (`pulses` are the pad's).
    touch_pulses: u16,
    /// Over the game → FPS (`fps_line`), counted while the game runs.
    fps: pocket_ui::fps_line::Counter,
    /// Fast-forward and rewind, Linux's (`player_speed`, `play_speed`): HOME + R2, HOME + L2 held, HOME's tap.
    speed: player_speed::Speed,
}
/// What a row of the list menu's Options page does (`Player::list_option_rows`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum OptTarget {
    /// A row of the top page.
    Top(pause_options::Entry),
    /// One of the Picture page's rows (the player's picture lists in the Handheld view's words).
    /// A row of the Picture page: Linux's setting (`option_table`), by its key (`pause_options::picture_settings`).
    Picture(&'static str),
    /// Row `n` of `display_options` (an emulator option, Hold to toggle).
    Option(usize),
    /// One of the core's categories on the Emulator page: opens its page (Linux's `emu_menu::Entry::Category`).
    Category(usize),
    /// Row `n` of the cheats (`CheatMenu::rows`).
    Cheat(usize),
    /// Row `n` of the Controls page (`controls::ControlsPage::rows`).
    Control(usize),
}
impl Player {
    fn new(core_path: &str, rom: &str, system: String, key: String, title: String, light: bool, frozen: bool) -> Result<Self, String> {
        if key.len() != 16 || !key.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid game identity".into());
        }
        let data = pocket_ui::platform::data_dir();
        // Decode shared effects beside core loading, before the first visible-frame CRT hook.
        pocket_ui::ui_sound::start();
        let saves = data.join("saves").join(&key);
        let states = data.join("states").join(&key);
        let downloaded_system = Path::new(core_path).parent().unwrap_or(Path::new("")).join("system");
        let bios = if system == "psp" && downloaded_system.join("PPSSPP").is_dir() { downloaded_system } else { data.join("bios") };
        for path in [&saves, &states, &bios] {
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        }
        if system == "psp" && !bios.join("PPSSPP").is_dir() {
            match pocket_ui::platform::android::strings("preparePspAssets", &[]) {
                Ok(error) if error.is_empty() => {}
                error => {
                    log::warn!("PPSSPP system assets unavailable: {error:?}");
                    return Err("The emulator's files could not be prepared.".into());
                }
            }
        }
        let descriptor = pocket_ui::platform::android::string("gameDocumentFd");
        let document = if descriptor.is_empty() { None } else {
            use std::os::fd::FromRawFd;
            let fd = descriptor.parse::<i32>().ok().filter(|fd| *fd >= 0).ok_or_else(|| "The game's document could not be opened".to_string())?;
            let source = unsafe { std::fs::File::from_raw_fd(fd) };
            Some(pocket_ui::platform::android_content::DirectGame::new(source, &data.join("direct-content").join(&key), Path::new(rom).file_name().and_then(|n| n.to_str()).unwrap_or_default()).map_err(|e| e.to_string())?)
        };
        let rom = document.as_ref().map_or(Path::new(rom), |d| d.path());
        let rom = rom.to_str().ok_or("Invalid ROM path")?;
        let base = states.join(Path::new(rom).file_stem().unwrap_or_default());
        // A crashed session's Before load is gone at the next start, as in Linux's resume_moment.
        pocket_ui::slots::clear_undo(&base);
        let alternative = Path::new(core_path).file_name().and_then(|n| n.to_str()) == Some("libgambatte_libretro.so");
        let options_name = if alternative {
            format!("{system}-gambatte.json")
        } else {
            format!("{system}.json")
        };
        let options_path = data.join("core-options").join(options_name);
        let saved_options: std::collections::BTreeMap<String, String> = pocket_ui::fsutil::read_json_or_backup(&options_path).unwrap_or_default();
        let mut selected: Vec<_> = saved_options.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        // Options → Picture → Resolution (N64, PSP, Dreamcast, PlayStation on a handheld measured for it): the row's choice
        // over a saved Emulator value, this handheld's default where nothing chose one; the very rule pocket-core follows on
        // Linux (`render_scale::for_start`, `StartOption::place`).
        // The Picture rows and the other per-game rows as Linux keeps them (`game_settings`): a game from the Handheld view
        // reads and writes the view's files (its This game page shows the same), handed its effective values; a Mobile game the
        // player's own, keyed by the game's stable key, its old `picture/*.json` taken over once.
        let launch: serde_json::Value = serde_json::from_str(&pocket_ui::platform::android::string("playerSettings")).unwrap_or_default();
        let handed: std::collections::BTreeMap<String, String> = launch["picture"].as_object().map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()).unwrap_or_default();
        let picture = match (launch["settings_dir"].as_str(), launch["settings_rom"].as_str()) {
            (Some(dir), Some(game)) => picture::Settings::load(Path::new(dir), &system, Path::new(game), handed),
            _ => {
                picture::migrate_old(&data, &system, &key);
                picture::Settings::load(&data, &system, Path::new(&key), picture::mobile_start())
            }
        };
        let scale = picture.start_resolution(pocket_ui::render_scale::here(&system), Path::new(core_path));
        log::info!("resolution: class={:?} system={system} {}", pocket_ui::render_scale::here_class(),
            scale.as_ref().map_or("untouched".to_string(), |s| format!("{} = {}{}", s.key, s.value, if s.first { " (chosen)" } else { " (this handheld's default)" })));
        if let Some(s) = &scale {
            s.place(&mut selected);
        }
        // Same missing-key defaults as Linux, below the player's saved options and resolution choice.
        pocket_ui::core_options::add_game_defaults(&mut selected, &pocket_ui::core_options::core_id(Path::new(core_path)), Path::new(rom));
        let mut core = CoreSession::new(Core::load(Path::new(core_path), Path::new(rom), &bios, &saves, &selected)?)?;
        core.input_probe(pocket_ui::platform::android::string("diagnosticAudio") == "true");
        let (initial_frame, _, av, geometry) = core.run(0);
        if let Some(av) = av {
            core.av = av;
        }
        if let Some(geometry) = geometry {
            core.av.geometry = geometry;
        }
        let initial_frame = core_hw_frame(&core).or(initial_frame);
        // Frozen games: the file just copied decides, before the core sees a state. A moment for another file or core
        // isn't loaded and goes where it can't be lost; the start says so.
        let core_id = pocket_ui::frozen_phone::core_of(Path::new(core_path));
        let start = pocket_ui::frozen_phone::at_start(&base, Path::new(rom), &core_id, frozen);
        // Only length-bounded parsers may receive shorter raw states as they are. The pinned PPSSPP adapter ignores
        // the supplied length and reports a 128 MiB placeholder until its renderer is up: its states are padded to the
        // size asked now instead (`state_policy`).
        core.apply_state_policy(&core_id);
        // Players 2 to 4 get a pad in the ports the core lists (couch co-op); a core with one port hears every pad as player 1.
        let mut pads = pocket_ui::android_pads::Pads::default();
        let plugged = core.plug_other_players(false); // plain pads, as player 1 has them here (no DualShock plugged on Android)
        pads.set_multiplayer(!plugged.is_empty());
        for line in &plugged { log::info!("player {line}"); }
        // RetroAchievements' Hardcore: the game starts from its in-game save, never from a state (`achievements::hardcore`).
        let hardcore = pocket_ui::achievements::hardcore::at_start(&data, &system, Path::new(rom));
        let resume = if start.resume && !hardcore { core.resume(&pocket_ui::slots::auto_path(&base)) } else { pocket_ui::libretro::Resume::NoState };
        let mut note = start.note;
        if hardcore && start.resume {
            note.push(pocket_ui::achievements::hardcore::STARTED_FRESH.into());
        }
        note.extend(pocket_ui::frozen_phone::after_resume(&base, resume, core.resumed_from()));
        if let Some(message) = play_cards::recovery_note(&core_id, resume) {note.push(message.into());}
        let mark = pocket_ui::frozen::Mark::new(pocket_ui::frozen_phone::EMULATOR, &core_id, Path::new(rom), None);
        if document.is_some() {
            // The existing CRC identity has been checked and the core opened the original document.
            // The previous content directory contains only disposable GameCopy payload, never saves.
            let copy = data.join("content").join(&key);
            if copy.is_dir() {
                if let Err(error) = std::fs::remove_dir_all(&copy) { log::warn!("old content copy retained: {error}"); }
            }
            log::info!("content: direct core={} path={} copied_bytes=0", core_id, rom);
        }
        let display_hz = display_refresh_hz();
        let pacer = pocket_ui::core_pacing::Pacer::new(core.av.timing.fps, display_hz);
        let placement = pocket_ui::core_pacing::Placement::of_this_device();
        log::info!("player placement: fast_cpus={}", placement.label());
        let mut audio = pocket_ui::audio::Audio::open(core.av.timing.sample_rate, pocket_ui::audio::needs_jitter_reserve(&system));
        if let Some(audio) = &mut audio {
            audio.set_paced_timing(core.av.timing.sample_rate, core.av.timing.fps, display_hz);
            if matches!(resume, pocket_ui::libretro::Resume::NoState) { audio.reserve_startup(&system); }
        }
        if audio.is_none() {
            return Err("AAudio output unavailable".into());
        }
        let options = core.options_snapshot().1;
        log::info!(
            "pocket-core in-app: {} {} fps={} sample_rate={} resume={resume:?}",
            core.name,
            core.version,
            core.av.timing.fps,
            core.av.timing.sample_rate
        );
        core.frontend_current();
        let mut gfx = Graphics::new()?;
        // Linux's picture pass needs the console's native lines and whether a 3D core renders above them (scanlines).
        gfx.system = system.clone();
        gfx.upscaled_3d = core.hw.is_some();
        let now = Instant::now();
        let resumed_frame = if matches!(resume, pocket_ui::libretro::Resume::Resumed | pocket_ui::libretro::Resume::ResumedAgain) {
            let (frame,_,_,_) = core.run(0);
            core_hw_frame(&core).or(frame)
        } else {None};
        let frame = player_video::startup(initial_frame, resumed_frame);
        if let Err(error) = pocket_ui::timeline::migrate_android(&base) { log::warn!("Earlier migration deferred: {error}"); }
        let mut timeline = pocket_ui::timeline::Timeline::default();
        if picture.earlier_on(&system) {
            if let Some(started) = core.resumed_from() {
                timeline.submit_copy(&base, pocket_ui::timeline::Kind::Started, started,
                    &pocket_ui::slots::preview_path(&pocket_ui::slots::auto_path(&base)));
                timeline.wait();
            }
        }
        let filled_slots = std::array::from_fn(|i| pocket_ui::slots::state_path(&base, i).is_file());
        let mut kept = pocket_ui::frozen_phone::kept(&base);
        let undo = pocket_ui::slots::undo_path(&base);
        if undo.is_file() { kept.push(undo); }
        let slot_icon = pocket_ui::system_art::get(&system).map(|sprite| sprite.fitted(26, 26));
        let sounds = pocket_ui::ui_sound::Config::load(&data);
        let control_names: Vec<_> = core.described_inputs().into_iter().map(|t| pocket_ui::controls::Target{id:t.id,label:t.label}).collect();
        if let Err(e)=play_cards::remember_names(&data,&key,&control_names) {log::warn!("Could not remember core button names: {e}");}
        let zoom = play_cards::Zoom::new(&system);
        // RetroAchievements, when an account is signed in: everything slow runs on the session's own threads.
        let ra = player_achievements::Achievements::start(&data, &system, Path::new(rom), &key, &core, hardcore);
        Ok(Self {
            ra,
            ra_lines: Vec::new(),
            ra_read: None,
            core,
            audio,
            gfx,
            text: Text::builtin().following_font(),
            pixels: vec![],
            overlay_size: None,
            touch_visible: pocket_ui::platform::android::string("hasGamepad") != "true",
            frame,
            game_area: None,
            pad_extent: None,
            window: (0, 0),
            stylus_finger: None,
            stylus: Default::default(),
            title,
            system,
            base,
            data,
            key,
            mark,
            _document: document,
            note,
            note_until: None,
            note_area: Area::new(0, 0, 0, 0),
            card_area: Area::new(0, 0, 0, 0),
            card_label: String::new(),
            first_frame: false,
            paused: false,
            resumed: matches!(resume, pocket_ui::libretro::Resume::Resumed | pocket_ui::libretro::Resume::ResumedAgain),
            suspended: false,
            control_names,
            hold_button: None,
            held: Default::default(),
            ds_pad: Default::default(),
            haptics: false,
            text_size: Default::default(),
            last_font_scale: 0.0,
            reminder: None,
            zoom,
            zoom_fingers: HashMap::new(),
            zoom_distance: None,
            crt_started: None,
            crt_done: false,
            light,
            menu: Menu::Pause,
            row: 0,
            slot: 0,
            filled_slots,
            kept,
            slot_icon,
            sounds,
            sounds_muted: false,
            message: String::new(),
            quit: false,
            picture,
            options,
            saved_options,
            options_path,
            buttons: 0,
            pulses: 0,
            stick_buttons: 0,
            triggers: Default::default(),
            pads,
            arrangement: Default::default(),
            last_given: [0; 4],
            stick_direction: 0,
            hat_repeat: Default::default(),
            clock: Instant::now(),
            menu_drag: None,
            swallow: None,
            scroll: 0,
            scroll_max: 0,
            advanced_open: false,
            dp: 2.2,
            dpad: None,
            scroll_base: 0,
            dragging: false,
            slot_arrows: [Area::new(0, 0, 0, 0); 2],
            pad_places: vec![],
            pad_anchor: None,
            option_arrows: vec![],
            fingers: HashMap::new(),
            controls: vec![],
            menu_button: Area::new(0, 0, 1, 1),
            hits: vec![],
            semantics: String::new(),
            top: 0,
            view: (0, 0, 1, 1),
            logical: (1, 1),
            pacer,
            pacing: Default::default(),
            placement,
            display_hz,
            frames: 0,
            video_frames: 0,
            diagnostic_buttons_seen: 0,
            last_flush: now,
            save_trouble: None,
            moment_failed: false,
            timeline,
            timeline_paused: None,
            cheats: None,
            list_menu: false,
            list_sel: 0,
            earlier_row: Default::default(),
            previews: HashMap::new(),
            list_hits: Vec::new(),
            list_hint_hits: Vec::new(),
            from_options: false,
            nav: Default::default(),
            option_categories: Vec::new(),
            disc_row: None,
            last_ok: false,
            live: Default::default(),
            live_key: None,
            live_pending: false,
            live_tries: 0,
            density: 1.0,
            game_view: (0, 0, 1, 1),
            list_zoom: None,
            zoom_shot: None,
            saving: None,
            saving_said: String::new(),
            saving_click: false,
            ach_sel: 0,
            // Filled from the game's own file at the start (`nativeCreate`).
            pad_page: pocket_ui::controls::ControlsPage { mapping: pocket_ui::controls::Mapping::defaults(""), targets: Vec::new(), system: String::new() },
            axes: [0; 4],
            touch_pulses: 0,
            restore_armed: false,
            fps: Default::default(),
            speed: Default::default(),
            recovery_required: false,
        })
    }
    fn state(&mut self, path: &Path) -> Result<(), String> {
        if self.recovery_required { return Err(moments::RECOVERY_MESSAGE.into()); }
        let bytes = self.core.save_state().ok_or("This core could not save its state")?;
        if let Err(error) = pocket_ui::state_file::write(path, &bytes) {
            self.note_save_trouble(Some(pocket_ui::save_trouble::Trouble::of(&error)));
            return Err(error.to_string());
        }
        if let Some(image) = &self.frame {
            if let Err(error) = png(&pocket_ui::slots::preview_path(path), image) { log::warn!("Saved moment without preview: {error}"); }
        }
        self.refresh_slots();
        log::info!("player state saved: {} bytes={}", path.display(), bytes.len());
        // The frame's power light flashes once for a save, as on Linux (frames without a light ignore it).
        self.gfx.flash_frame_light();
        Ok(())
    }
    fn automatic(&mut self) -> Result<(), String> {
        if self.recovery_required { return Err(moments::RECOVERY_MESSAGE.into()); }
        let path = pocket_ui::slots::auto_path(&self.base);
        // The battery save first, so the state is never older than it (`Core::resume`), as pocket-core's exit does.
        let battery = self.core.write_sram();
        if let Some(mark) = &self.mark {
            pocket_ui::frozen_phone::write_mark(&self.base, mark);
        }
        self.note_save_trouble(battery.err().or(self.core.save_trouble()));
        let moment = self.state(&path);
        if moment.is_ok() {
            pocket_ui::slots::auto_written(&path);
            if self.picture.earlier_on(&self.system) && self.timeline.due() {
                self.timeline.submit_copy(&self.base, pocket_ui::timeline::Kind::Every, &path, &pocket_ui::slots::preview_path(&path));
            }
            pocket_ui::slots::battery_unsaved(&path, battery.is_err());
        }
        let result = moment.and_then(|()| battery.map_err(|e| e.line().to_string()));
        self.moment_failed = result.is_err();
        self.overlay_size = None;
        pocket_ui::save_trouble::report_moment(&pocket_ui::platform::data_dir().join("player-save-error"), result.is_ok(), self.save_trouble);
        result
    }
    fn note_save_trouble(&mut self, now: Option<pocket_ui::save_trouble::Trouble>) {
        if self.save_trouble != now {
            self.save_trouble = now;
            self.overlay_size = None;
            pocket_ui::save_trouble::publish_to(&pocket_ui::platform::data_dir().join("player-save-trouble"), now);
        }
    }
    /// Hardcore's rules hold for this game now (`player_achievements::Achievements::hardcore`).
    fn hardcore(&self) -> bool {
        self.ra.as_ref().is_some_and(|r| r.hardcore())
    }
    fn load(&mut self, path: &Path) -> Result<(), String> {
        // Every load comes here (Load, Earlier): none in Hardcore.
        if self.hardcore() {
            return Err(pocket_ui::achievements::hardcore::NO_LOAD.into());
        }
        let bytes = pocket_ui::state_file::read(path).map_err(|e| e.to_string())?;
        let loaded_picture = pocket_ui::image::load_png(&pocket_ui::slots::preview_path(path));
        let undo_path = pocket_ui::slots::undo_path(&self.base);
        let result = if self.recovery_required { moments::recover(&mut self.core, &bytes) }
        else { moments::load(&mut self.core, &bytes, |before| {
            pocket_ui::state_file::write(&undo_path, before).map_err(|_| pocket_ui::save_trouble::UNDO_FAILED.to_string())?;
            if let Some(image) = &self.frame {
                let _ = png(&pocket_ui::slots::preview_path(&undo_path), image);
            }
            Ok(())
        }) };
        if let Err(error) = result {
            self.recovery_required |= error.recovery;
            if error.recovery { self.menu = Menu::Recovery; self.note = vec![error.message.clone()]; self.scroll = 0; }
            return Err(error.message);
        }
        self.refresh_slots();
        self.recovery_required = false;
        // The loaded moment can't unlock anything by itself: achievements wait for the game to move on.
        if let Some(ra) = &mut self.ra { ra.reset(); }
        // Rewind's history is the past of the game left, not of the moment loaded (pocket-core clears it too).
        self.speed.forget();
        self.core.set_analog([0; 4]);
        self.stylus.clear();
        self.stylus_finger = None;
        self.core.set_pointer(None);
        if let Some(image) = loaded_picture { self.frame = Some(std::sync::Arc::new(image)); }
        self.held = Default::default();
        self.buttons = 0;
        self.fingers.clear();
        self.pulses = 0;
        self.touch_pulses = 0;
        log::info!("player state loaded: {}", path.display());
        Ok(())
    }
    /// Save & quit: the launcher's way back gets the game's last picture (the auto state's preview, just written) and
    /// where it stood on the screen (`eject::LastPicture`). The time is the quit's, for its measurement in the log.
    fn leave_last_picture(&self, saved: bool) {
        pocket_ui::slots::clear_undo(&self.base);
        log::info!("quit: saved={saved} at {:.1}", pocket_ui::platform::android::monotonic_ms());
        let preview = pocket_ui::slots::preview_path(&pocket_ui::slots::auto_path(&self.base));
        let (left, _, vw, vh) = self.view;
        let (lw, lh) = (self.logical.0.max(1) as f32, self.logical.1.max(1) as f32);
        // The game's rectangle is in the overlay's logical pixels; the view starts `top` down from the window's top.
        let rect = self.game_area.map(|a| {
            let (kx, ky) = (vw as f32 / lw, vh as f32 / lh);
            (left + (a.x as f32 * kx).round() as i32, self.top + (a.y as f32 * ky).round() as i32, (a.w as f32 * kx).round() as i32, (a.h as f32 * ky).round() as i32)
        });
        let note = pocket_ui::eject::LastPicture {
            key: self.key.clone(),
            picture: (saved && self.frame.is_some() && preview.is_file()).then_some(preview),
            rect,
            window: self.window,
            saved,
            at_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64),
        };
        if let Err(error) = note.write(&self.data) {
            log::warn!("quit: no last picture for the way back: {error}");
        }
    }
    fn refresh_slots(&mut self) {
        self.filled_slots = std::array::from_fn(|i| pocket_ui::slots::state_path(&self.base, i).is_file());
        self.kept = pocket_ui::frozen_phone::kept(&self.base);
        let undo = pocket_ui::slots::undo_path(&self.base);
        if undo.is_file() { self.kept.push(undo); }
    }
    /// The Achievements page's lines from the session's list (the file it wrote when the menu opened), at most once a
    /// second: the list is still loading when the menu opens in a game's first seconds.
    fn refresh_achievements(&mut self, now: bool) {
        let Some(ra) = &self.ra else { return };
        if !now && self.ra_read.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) { return; }
        self.ra_read = Some(Instant::now());
        ra.menu_opened();
        self.ra_lines = pocket_ui::achievements_page::list_lines(&ra.list());
    }
    fn sound(&self, sound: pocket_ui::ui_sound::Sound) {
        // The shared mixer uses Android AAudio and the app sound preference; Android owns master volume.
        let _ = pocket_ui::ui_sound::play(
            sound,
            &pocket_ui::ui_sound::Ctx {
                config: pocket_ui::ui_sound::Config {
                    on: self.sounds.on && !self.sounds_muted,
                    ..self.sounds
                },
                in_game: true,
                pause_menu: true,
                asleep: false,
            },
        );
    }
    fn pause(&mut self, on: bool) {
        if !on && self.recovery_required { return; }
        // The menu stops every motor, as pocket-core's (a rumble left on is a lease that runs out anyway).
        if on { pocket_ui::rumble::stop(); }
        if !on {self.suspended=false;self.fps.restart();}
        if !on && self.audio.is_none() {
            self.audio = pocket_ui::audio::Audio::open(self.core.av.timing.sample_rate, pocket_ui::audio::needs_jitter_reserve(&self.system));
        }
        if !on {
            if let Some(audio) = &mut self.audio {
                audio.set_paced_timing(self.core.av.timing.sample_rate, self.core.av.timing.fps, self.display_hz);
            }
        }
        if on {
            self.crt_done = true;
            self.refresh_slots();
            self.sounds = pocket_ui::ui_sound::Config::load(&self.data);
        }
        self.sound(if on {
            pocket_ui::ui_sound::Sound::Menu
        } else {
            pocket_ui::ui_sound::Sound::Back
        });
        self.timeline.stop();
        self.timeline_paused = on.then(Instant::now);
        self.paused = on;
        self.reminder = None;
        self.held = Default::default();
        self.zoom_fingers.clear();
        self.zoom_distance = None;
        self.overlay_size = None;
        self.buttons = 0;
        self.pulses = 0;
        self.touch_pulses = 0;
        self.stick_buttons = 0;
        self.pads.release_all();
        self.speed.release_all();
        self.stick_direction = 0;
        self.hat_repeat = Default::default();
        self.core.set_analog([0; 4]);
        self.stylus.clear();
        self.core.set_pointer(None);
        self.stylus_finger = None;
        self.menu_drag = None;
        self.fingers.clear();
        self.pacer.restart();
        self.pacing.pause();
        self.placement.again();
        if let Some(audio) = &self.audio {
            audio.pause(on);
        }
        self.menu = Menu::Pause;
        self.row = 0;
        self.scroll = 0;
        self.swallow = None;
        self.list_sel = 0;
        self.from_options = false;
        self.previews.clear();
        self.nav.reset();
        self.live_key = None;
        self.live_pending = false;
        if on {
            // A game of several discs: the Disc row, on the disc in the drive (Linux's `app_disc`).
            self.disc_row = self.core.disc().and_then(|(at, count)| pocket_ui::disc::DiscRow::new(pocket_ui::disc::Discs::from_core(at, count)));
            self.refresh_earlier();
            // The list menu starts without the last pause's line ("Saved to slot 1"), as Linux's.
            if self.list_menu {
                self.message.clear();
            }
        }
        log::info!("player paused={on}");
    }
    /// A key of one device. In the game a pad that plays player 2 to 4 only holds its own buttons; the keys that
    /// open the pause menu, and every key while a menu is open, are anybody's.
    fn key_of(&mut self, device: i32, key: i32, down: bool) -> bool {
        if !self.paused && !matches!(key, 4 | 82 | 110) {
            match self.pads.player_of(device) {
                Some(0) => (),
                Some(_) => {
                    self.pads.key(device, key, down);
                    return true;
                }
                None => return true,
            }
        }
        self.arrangement = self.pads.arrangement_of(device);
        self.key(key, down)
    }
    /// The sticks, hat and triggers of one device, as `key_of`.
    fn analog_of(&mut self, device: i32, axes: [f32; 4], hat: [f32; 2], triggers: [f32; 2]) -> bool {
        if !self.paused {
            match self.pads.player_of(device) {
                Some(0) => (),
                Some(_) => {
                    self.pads.motion(device, axes, hat, triggers, pocket_ui::controls::stick_dpad_default(&self.system));
                    return true;
                }
                None => return true,
            }
        }
        self.analog(axes, hat, triggers)
    }
    fn analog(&mut self, axes: [f32; 4], hat: [f32; 2], triggers: [f32; 2]) -> bool {
        for (i, (latch, value)) in self.triggers.iter_mut().zip(triggers).enumerate() {
            if let Some(down) = latch.update(value) {
                if !self.paused {
                    self.speed.trigger(i == 1, down);
                }
            }
        }
        if axes.iter().chain(hat.iter()).any(|v| v.abs() > 0.55) {
            self.crt_done = true;
        }
        self.axes = axes.map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16);
        self.core.set_analog(self.axes);
        let x = if hat[0].abs() > 0.5 { hat[0] } else { axes[0] };
        let y = if hat[1].abs() > 0.5 { hat[1] } else { axes[1] };
        // On an analog console the stick stays analog; only its actual hat presses the D-pad.
        // Options › Controls › Left stick as D-pad (Linux's row; the console's default until the player changes it).
        let (game_x,game_y) = if self.pad_page.mapping.stick_dpad {(x,y)} else {(hat[0],hat[1])};
        self.stick_buttons = (if game_x < -0.55 {
            1 << 6
        } else if game_x > 0.55 {
            1 << 7
        } else {
            0
        }) | (if game_y < -0.55 {
            1 << 4
        } else if game_y > 0.55 {
            1 << 5
        } else {
            0
        }) | (if self.triggers[0].down() { 1 << 12 } else { 0 }) | (if self.triggers[1].down() { 1 << 13 } else { 0 });
        let direction = pocket_ui::pad::android::direction_of(x, y);
        let now = self.clock.elapsed().as_millis() as u64;
        if direction != self.stick_direction {
            self.stick_direction = direction;
            if self.paused {
                if let Some(step) = self.hat_repeat.set(direction, now) {
                    self.key(step, true);
                }
            }
        } else if self.paused {
            if let Some(step) = self.hat_repeat.due(now) {
                self.key(step, true);
            }
        }
        // A held direction in the menu keeps the frames coming (`frame`), which is what repeats it.
        !self.paused || self.hat_repeat.held()
    }
    fn key(&mut self, key: i32, down: bool) -> bool {
        if down {
            self.crt_done = true;
        }
        // Any button while Save's picture flies: it ends and the game comes back (Linux's `skip_save_fill`).
        if self.saving.is_some() {
            if down {
                self.end_save_fill();
            }
            return !self.paused;
        }
        if down && self.reminder.is_some() {
            self.reminder = None;
            self.pacer.restart();
            self.pacing.pause();
            self.overlay_size = None;
            return true;
        }
        // In the game the pad's HOME is Linux's (`player_speed`): a tap opens the menu when it is let go, HOME + R2 and
        // HOME + L2 are fast-forward and rewind.
        if !self.paused {
            let open = self.speed.key(key, down);
            if key == player_speed::HOME_KEY {
                if open {
                    self.pause(true);
                }
                return !self.paused;
            }
        }
        if matches!(key, 4 | 82 | 110) || (self.paused && matches!(key, 97 | 111)) {
            if down {
                if self.paused && self.from_options && self.menu == Menu::Zoom {
                    // Zoom opened from Options (the list menu) goes back there, onto its row.
                    self.menu = Menu::Options;
                    self.nav.reset();
                    self.nav.sel = self.list_option_rows().1.iter().position(|t| *t == OptTarget::Top(pause_options::Entry::Zoom)).unwrap_or(0);
                    self.from_options = false;
                    self.zoom_fingers.clear();
                } else if self.paused && self.list_menu && self.menu == Menu::Options && self.opt_back() {
                    // A group of Options (the list menu) goes back to the top page, onto its row.
                } else if self.paused && self.menu != Menu::Pause {
                    // Back from the achievements list lands on its own row, so the next press doesn't start at the top.
                    self.row = if self.menu == Menu::Achievements { self.rows().len().saturating_sub(1) } else { 0 };
                    // The list menu: back onto the row the page came from (Options, Achievements).
                    let from = if self.menu == Menu::Achievements { pause_list::Item::Achievements } else { pause_list::Item::Options };
                    self.list_sel = self.list_rows().iter().position(|r| r.item == from).unwrap_or(0);
                    self.menu = Menu::Pause;
                    self.scroll = 0;
                    self.zoom_fingers.clear();
                } else { self.pause(!self.paused); }
            }
            return !self.paused;
        }
        if self.paused && self.list_menu && self.menu == Menu::Achievements {
            // Linux's Achievements page: Up/Down go through the achievements (round), each with its line; B is above.
            if down {
                let n = self.ra.as_ref().map_or(0, |ra| pocket_ui::achievements_page::menu_rows(&ra.list()).len()).max(1);
                match key {
                    19 => self.ach_sel = (self.ach_sel + n - 1) % n,
                    20 => self.ach_sel = (self.ach_sel + 1) % n,
                    _ => (),
                }
            }
            return !self.paused;
        }
        if self.paused && matches!(self.menu, Menu::Controls | Menu::Recovery | Menu::Achievements) {
            if down {
                // A long list (a hundred achievements) moves by five lines.
                let step = if self.menu == Menu::Achievements { 5 } else { 1 };
                match key {
                    19 => self.scroll = (self.scroll - step).max(0),
                    20 => self.scroll = (self.scroll + step).min(self.scroll_max),
                    23 | 66 | 96 => { self.key(4, true); },
                    _ => (),
                }
            }
            return !self.paused;
        }
        if self.paused && self.menu == Menu::Zoom && self.list_menu && self.from_options {
            // Linux's Zoom page (`zoom::Zoom::press`): the D-pad moves the part, A, L1 and R1 switch 2x and 3x.
            if down {
                use pocket_ui::pad::Button;
                let b = match key {
                    19 => Button::Up,
                    20 => Button::Down,
                    21 => Button::Left,
                    22 => Button::Right,
                    23 | 66 | 96 | 102 | 103 => Button::L1,
                    _ => return false,
                };
                if let Some(zoom) = self.list_zoom.as_mut() {
                    if zoom.press(b, Button::L1, Button::Select) == pocket_ui::zoom::Action::Moved {
                        self.sound(pocket_ui::ui_sound::Sound::Nav);
                    }
                }
            }
            return false;
        }
        if self.paused && self.menu == Menu::Zoom {
            if down {
                match key {
                    19 => self.zoom.pan(0.0,-0.2), 20 => self.zoom.pan(0.0,0.2),
                    21 => self.zoom.pan(-0.2,0.0), 22 => self.zoom.pan(0.2,0.0),
                    23 | 66 | 96 | 102 | 103 => self.zoom.scale = if self.zoom.scale < 2.5 {3.0} else {2.0},
                    _ => (),
                }
            }
            return false;
        }
        if self.paused {
            if !down {
                return false;
            }
            if matches!(key, 19..=22 | 102 | 103) {
                self.sound(pocket_ui::ui_sound::Sound::Nav);
            }
            if self.list_menu && self.menu == Menu::Pause && !matches!(key, 97 | 111) {
                self.key_list(key);
                return false;
            }
            if self.list_menu && self.menu == Menu::Options && !matches!(key, 97 | 111) {
                self.key_list_options(key);
                return false;
            }
            match key {
                19 | 20 if self.menu == Menu::Options => {
                    // Up/Down follow the page's order (by section), then Back.
                    let options = self.display_options();
                    let mut order = option_page(&options, self.advanced_open);
                    order.push(options.len());
                    let at = order.iter().position(|r| *r == self.row).unwrap_or(0);
                    let next = if key == 19 { at.saturating_sub(1) } else { (at + 1).min(order.len() - 1) };
                    self.row = order[next];
                }
                // The pause sheet: the D-pad moves as the sheet looks, Left/Right on the slot change it.
                21 | 22 if self.menu == Menu::Pause && self.row == pause_head::PICKER => {
                    let places = self.places();
                    self.slot = (self.slot + if key == 21 { places - 1 } else { 1 }) % places;
                }
                19..=22 if self.menu == Menu::Pause && !self.pad_places.is_empty() => {
                    let dir = match key { 19 => pause_head::Dir::Up, 20 => pause_head::Dir::Down, 21 => pause_head::Dir::Left, _ => pause_head::Dir::Right };
                    let anchor = self.pad_anchor.filter(|(row, _)| *row == self.row).map(|(_, x)| x);
                    let (row, x) = pause_head::grid_step(&self.pad_places, self.row, dir, anchor);
                    (self.row, self.pad_anchor) = (row, Some((row, x)));
                }
                // Before the sheet's first picture: reading order.
                19 | 20 if self.menu == Menu::Pause => self.row = pause_head::pad_step(self.row, self.rows().len(), key == 20),
                19 => self.row = self.row.saturating_sub(1),
                20 => self.row = (self.row + 1).min(self.rows().len().saturating_sub(1)),
                21 | 102 => {
                    if self.menu == Menu::Pause {
                        self.slot = (self.slot + self.places() - 1) % self.places()
                    } else {
                        self.change_option(-1)
                    }
                }
                22 | 103 => {
                    if self.menu == Menu::Pause {
                        self.slot = (self.slot + 1) % self.places()
                    } else {
                        self.change_option(1)
                    }
                }
                97 | 111 => {
                    if self.menu == Menu::Options {
                        self.menu = Menu::Pause;
                        self.row = 0;
                    } else {
                        self.pause(false)
                    }
                }
                23 | 66 | 96 => self.activate(),
                _ => (),
            }
        } else if let Some(bit) = pocket_ui::pad::android::retro_bit_on(key, self.arrangement) {
            if down {
                self.buttons |= 1 << bit;
                self.pulses |= 1 << bit;
            } else {
                self.buttons &= !(1 << bit);
            }
        }
        !self.paused
    }
    /// What the list menu paints this frame, gathered before the canvas holds the pixels: the rows of Linux's pages
    /// (`pause_page`), from the player's own state.
    fn list_paint(&mut self) -> ListPaint {
        if self.menu == Menu::Zoom {
            return ListPaint::Zoom;
        }
        if self.menu == Menu::Achievements {
            // Linux's Achievements page (`achievements_page::menu_rows`, `app_achievements`): the counts under the heading,
            // one row per achievement, its line at the bottom.
            let snap = self.ra.as_ref().map(|ra| ra.list()).unwrap_or_default();
            let rows: Vec<pocket_ui::pause_page::OptRow> = pocket_ui::achievements_page::menu_rows(&snap).into_iter().map(|(n, v, d)| pocket_ui::pause_page::OptRow { fixed: true, ..pocket_ui::pause_page::OptRow::new(n, v, d) }).collect();
            self.ach_sel = self.ach_sel.min(rows.len().saturating_sub(1));
            let counts = (snap.state == pocket_ui::achievements::snapshot::State::Ready && snap.total > 0).then(|| snap.headline());
            return ListPaint::Options { rows, sel: self.ach_sel, footer: None, heading: "Achievements".into(), explanation: false, picture: false, counts };
        }
        if self.menu == Menu::Options {
            let (rows, _) = self.list_option_rows();
            let sel = self.nav.sel.min(rows.len().saturating_sub(1));
            // What the last change said (Restore defaults' question and answer, a screenshot, a setting that couldn't be
            // kept) stands in place of the row's line, as Linux's note on its Options page.
            let footer = (!self.message.is_empty()).then(|| self.message.clone());
            let explanation = self.nav.page == Some(pause_options::Entry::Cheats) && self.cheats.as_ref().is_some_and(|c| !c.actionable(sel));
            let heading = match (self.nav.page, self.nav.cat) {
                (None, _) => "Options".to_string(),
                // Linux's category heading: "Emulator   Sound" (`paint_options`).
                (Some(pause_options::Entry::Emulator), Some(cat)) => format!("Emulator   {}", self.option_categories.get(cat).map_or("", |c| c.label.as_str())),
                (Some(page), _) => page.heading(),
            };
            let picture = self.nav.page == Some(pause_options::Entry::Picture);
            return ListPaint::Options { rows, sel, footer, heading, explanation, picture, counts: None };
        }
        let rows = self.list_rows();
        let sel = self.list_sel.min(rows.len().saturating_sub(1));
        let item = rows.get(sel).map(|r| r.item);
        let steps = rows.get(sel).is_some_and(|r| r.steps);
        let now = pocket_ui::play_time::now_secs();
        let age = |p: &Path| {
            let secs = std::fs::metadata(p).and_then(|m| m.modified()).ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
            Some(pocket_ui::play_time::ago_from(secs, now))
        };
        let mut step: Option<&'static str> = None;
        let window = match item {
            Some(item @ (pause_list::Item::Save | pause_list::Item::Load)) => {
                let path = self.place_path();
                let saved = if path.is_file() { age(&path) } else { None };
                let picture = if saved.is_some() { self.preview(&path) } else { None };
                let n = self.slot + 1;
                let kept = self.slot >= self.filled_slots.len();
                let save = item == pause_list::Item::Save;
                let note = if !self.message.is_empty() {
                    self.message.clone()
                } else if self.hardcore() {
                    (if save { pocket_ui::achievements::hardcore::NO_SAVE } else { pocket_ui::achievements::hardcore::NO_LOAD }).to_string()
                } else if kept && save {
                    KEPT_ONLY_LOADS.to_string()
                } else if kept {
                    self.place_name()
                } else {
                    match (save, &saved) {
                        (true, Some(t)) => format!("Slot {n} · replaces the save from {t}"),
                        (true, None) => format!("Slot {n} · empty"),
                        (false, Some(t)) => format!("Slot {n} · saved {t}"),
                        (false, None) => format!("No save in slot {n} yet"),
                    }
                };
                if steps {
                    step = Some("Slot");
                }
                let empty = if saved.is_some() { "No picture" } else { "Empty" };
                Some(ListWindow { picture, empty, note, slots: true })
            }
            Some(pause_list::Item::Earlier) => {
                let at = self.earlier_row.selected().cloned();
                let picture = at.as_ref().and_then(|e| self.preview(&e.path));
                let empty = if at.is_some() { "No picture" } else { "None yet" };
                let note = if !self.message.is_empty() {
                    self.message.clone()
                } else if self.hardcore() {
                    pocket_ui::achievements::hardcore::NO_LOAD.to_string()
                } else {
                    pocket_ui::timeline::note(at.as_ref(), now, self.earlier_row.tz)
                };
                if steps {
                    step = Some("State");
                }
                Some(ListWindow { picture, empty, note, slots: false })
            }
            _ => None,
        };
        // The line beside the rows (Linux's `menu_note`): what the last action said, Disc's, Save & quit's.
        let note = if window.is_some() {
            None
        } else if !self.message.is_empty() {
            Some(self.message.clone())
        } else {
            match item {
                Some(pause_list::Item::Disc) => self.disc_row.map(|d| d.note()),
                Some(pause_list::Item::Quit) if !self.recovery_required => Some("Next time, A continues from this moment.".to_string()),
                _ => None,
            }
        };
        if item == Some(pause_list::Item::Disc) {
            step = Some("Disc");
        }
        // Linux's hints (`pause_page::main_hints`) without START's quick settings and SELECT's recent games: the player
        // has neither (Android's own Home and recent apps are those).
        let hints = pocket_ui::pause_page::main_hints(step, item.is_some_and(pause_list::Item::opens_page), false, false);
        // Retro's rows' window is as wide as the widest value a row can show: Save's and Load's longest place.
        let widest_place = (0..self.places()).map(|p| place_name(p, self.filled_slots.len())).max_by_key(|t| t.chars().count());
        let rows = rows
            .into_iter()
            .map(|r| {
                let widest = match r.item {
                    pause_list::Item::Save | pause_list::Item::Load if r.steps => widest_place.clone().or_else(|| r.chip.clone()),
                    _ => r.chip.clone(),
                };
                pocket_ui::pause_page::MainRow { label: r.label, icon: r.item.icon(), enabled: r.enabled, opens: r.item.opens_page(), chip: r.chip, widest, arrows: r.steps }
            })
            .collect();
        ListPaint::Main { rows, sel, window, note, hints }
    }
    /// The list menu's rows (`pause_list`), in Linux's order (`main_items`, `QuickMenu::items`): Continue, Save, Load,
    /// Earlier while it has a row, Disc for a game of several, Options, Achievements while signed in, Save & quit. Save
    /// and Load show the slot, Earlier how long ago its moment is, Disc which disc.
    fn list_rows(&self) -> Vec<pause_list::Row> {
        use pause_list::{Item, Row};
        let hardcore = self.hardcore();
        let row = |item, label: &str, chip: Option<String>, steps: bool, enabled: bool| Row { item, label: label.into(), chip, steps, enabled };
        let place = match self.slot.checked_sub(self.filled_slots.len()) {
            Some(_) => self.place_name(),
            None => (self.slot + 1).to_string(),
        };
        let chip = Some(if hardcore { "Off in Hardcore".to_string() } else { place });
        let steps = !hardcore && self.places() > 1;
        let earlier = self.earlier_row.selected();
        let now = pocket_ui::play_time::now_secs();
        let earlier_chip = if hardcore { Some("Off in Hardcore".to_string()) } else { earlier.map(|e| pocket_ui::timeline::chip(e, now, self.earlier_row.tz)) };
        let earlier_row = self.picture.earlier_on(&self.system) || !self.earlier_row.entries.is_empty();
        pause_list::order(earlier_row, self.disc_row.is_some(), self.ra.is_some())
            .into_iter()
            .map(|item| match item {
                Item::Continue => row(item, item.label(), None, false, true),
                Item::Save => row(item, item.label(), chip.clone(), steps, !hardcore && self.slot < self.filled_slots.len()),
                Item::Load => row(item, item.label(), chip.clone(), steps, !hardcore && pocket_ui::slots::load_refusal(&self.filled_slots, self.slot).is_none()),
                // Earlier only while it has a row, as Linux's (`app_earlier`): switched on, or moments of it kept.
                Item::Earlier => row(item, item.label(), earlier_chip.clone(), !hardcore && self.earlier_row.entries.len() > 1, !hardcore && self.earlier_row.loadable()),
                Item::Disc => row(item, item.label(), self.disc_row.map(|d| d.chip()), true, true),
                Item::Options => row(item, item.label(), None, false, true),
                Item::Achievements => row(item, item.label(), None, false, true),
                Item::Quit => row(item, if self.recovery_required { "Return to library" } else { item.shown(true) }, None, false, true),
            })
            .collect()
    }
    /// The Picture page on a `w`×`h` overlay: how many rows its list shows and where the preview goes, as Linux's
    /// (`pause_page::picture_page`, the longest line of its rows' values setting the room).
    fn picture_layout(&mut self, w: i32, h: i32) -> (usize, pocket_ui::preview::Layout) {
        let range = pocket_ui::render_scale::here(&self.system);
        // Laid out for the most rows the page can have (Linux's `max_rows`): the preview stays put while a row comes and goes.
        let rows = pause_options::picture_max_rows(&self.system, range);
        let descs = pause_options::picture_descs(&self.system, range);
        pocket_ui::pause_page::picture_page(&mut self.text, w, h, rows, &descs)
    }
    /// The Picture page's preview for the choices as they are now (Linux: pocket-core's `preview`): the paused game drawn
    /// offscreen as it is played, with the look, the frame and the scaling chosen (`Graphics::render_preview`), then cut
    /// into the whole screen and the 1:1 detail (`preview::from_screen`). Only when a choice or the layout changed; a frame
    /// still being made on its thread asks for the next frame (`live_pending`).
    fn refresh_live(&mut self, layout: pocket_ui::preview::Layout) {
        let key = (self.picture.scaling, self.picture.video, layout.detail);
        if self.live_key == Some(key) && !self.live_pending {
            return;
        }
        // A new choice starts counting again; a frame that never comes stops asking after two seconds of frames.
        self.live_tries = if self.live_key == Some(key) { self.live_tries + 1 } else { 0 };
        self.live_key = Some(key);
        match self.screen_picture() {
            Some((rgba, drawn, _)) => {
                let (sw, sh) = (self.window.0.max(1), self.window.1.max(1));
                let (detail, whole) = pocket_ui::preview::from_screen(&rgba, sw, sh, layout.detail.2.max(1) as u32, layout.detail.3.max(1) as u32);
                let img = |p: pocket_ui::preview::Pixels| Image { w: p.w, h: p.h, px: p.px };
                self.live.set(img(whole), img(detail));
                self.live_pending = !drawn && self.live_tries < 120;
            }
            None => self.live_pending = false,
        }
    }
    /// The paused game as the screen shows it in play, with the Picture choices as they are now: drawn offscreen at the
    /// window's size (`Graphics::render_preview`), as RGBA rows of the view (the window less the system bars), whether a
    /// frame asked for was drawn yet, and where the game itself is in it (x, y, w, h), without the black borders or the
    /// frame. None before the first picture or when GLES can't draw offscreen.
    fn screen_picture(&mut self) -> Option<(Vec<u8>, bool, pocket_ui::preview::GameRect)> {
        let image = self.frame.clone()?;
        // The game's place and frame as the game shows them with a controller (no touch pad on screen).
        let game_view = self.game_view;
        let logical = pocket_ui::scaled_layout::mobile_size((game_view.2 as u32, game_view.3 as u32), self.density);
        let (lw, lh) = (logical.0 as i32, logical.1 as i32);
        let window = (self.window.0 as i32, self.window.1 as i32);
        let space = self.picture.place(window, game_view, (lw, lh), (image.w, image.h), self.core.av.geometry.aspect_ratio as f64, false);
        let frame = if self.picture.frame != 0 {
            let room = picture::Settings::frame_room(space.size, false, space.game, (0, space.size.1));
            self.picture.frame_plan(&self.system, space.size, space.view, room, space.game, image.w)
        } else {
            None
        };
        self.gfx.variant = self.picture.variant(&self.system);
        self.core.frontend_current();
        // The whole screen, as Linux's preview reads the whole panel.
        let screen = (0, 0, window.0.max(1), window.1.max(1));
        let Some((rgba, drawn)) = self.gfx.render_preview(window, screen, (&image, space.game), frame.as_ref(), (space.size, space.view)) else {
            log::warn!("player screen picture: no offscreen picture");
            return None;
        };
        let (x, y, w, h) = space.on_window(window.1);
        let (vw, vh) = (screen.2, screen.3);
        let (x0, y0) = (x.clamp(0, vw - 1), y.clamp(0, vh - 1));
        let rect = (x0 as u32, y0 as u32, w.clamp(1, vw - x0) as u32, h.clamp(1, vh - y0) as u32);
        Some((rgba, drawn, rect))
    }
    /// Options → Zoom in the handhelds' menu: Linux's page (`pause_page::paint_zoom`) over the screen as the game shows
    /// it, starting where Linux's starts (a handheld's at the bottom edge, where its dialogue boxes are).
    fn open_list_zoom(&mut self) {
        let Some((rgba, _, rect)) = self.screen_picture() else {
            self.message = pocket_ui::i18n::tr("No picture");
            return;
        };
        let shot = pocket_ui::preview::shot_from_screen(&rgba, self.window.0.max(1), self.window.1.max(1));
        self.zoom_shot = Some((Image { w: shot.w, h: shot.h, px: shot.px }, rect));
        let level = self.list_zoom.map_or(pocket_ui::zoom::LEVELS[0], |z| z.level());
        self.list_zoom = Some(if pocket_ui::zoom::starts_low(&self.system) { pocket_ui::zoom::Zoom::low(level) } else { pocket_ui::zoom::Zoom::new(level) });
        self.menu = Menu::Zoom;
        self.from_options = true;
        self.sound(pocket_ui::ui_sound::Sound::Confirm);
    }
    /// Both menus use Linux's timeline row: the ring and session starts, with their names and times intact.
    fn refresh_earlier(&mut self) {
        self.earlier_row = pocket_ui::timeline::Row::new(pocket_ui::timeline::entries(&self.base),
            pocket_ui::timezone::utc_offset_now(pocket_ui::play_time::now_secs() as i64));
    }
    fn load_earlier(&mut self) -> Result<(), String> {
        let listed = self.earlier_row.selected().ok_or(pocket_ui::save_trouble::NOTHING_EARLIER)?;
        let entry = pocket_ui::timeline::to_load(&self.base, &listed.name.wire(), Some(listed.secs())).map_err(str::to_string)?;
        self.load(&entry.path)
    }
    /// A state's picture, read once while the menu is open. Only a picture at least as new as its state: the moment at
    /// a game's start has none, and an older one beside it would show the wrong moment.
    fn preview(&mut self, state: &Path) -> Option<std::sync::Arc<Image>> {
        self.previews
            .entry(state.to_path_buf())
            .or_insert_with(|| {
                let png = pocket_ui::slots::preview_path(state);
                let time = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
                match (time(state), time(&png)) {
                    (Some(s), Some(p)) if p + Duration::from_secs(2) >= s => pocket_ui::image::load_png(&png).map(std::sync::Arc::new),
                    _ => None,
                }
            })
            .clone()
    }
    /// A on a row of the list menu. Save, Load, Options, Achievements and Quit are the sheet's own actions (`activate`
    /// on their numbers); Earlier loads the moment it is on; Disc puts in the disc it shows. As on Linux, a save or a
    /// load that worked goes back to the game (`resume_game`), and what it did shows over the game.
    fn activate_list(&mut self) {
        let rows = self.list_rows();
        let Some(item) = rows.get(self.list_sel).map(|r| r.item) else { return };
        let legacy = match item {
            pause_list::Item::Continue => 0,
            pause_list::Item::Save => 1,
            pause_list::Item::Load => 2,
            pause_list::Item::Options => 4,
            pause_list::Item::Achievements => 9,
            pause_list::Item::Quit => 8,
            pause_list::Item::Disc => {
                let Some(disc) = self.disc_row else { return };
                let target = disc.target();
                if self.core.set_disc(target) {
                    log::info!("player disc {} of {}", target + 1, disc.discs.count);
                    self.sound(pocket_ui::ui_sound::Sound::Confirm);
                    self.pause(false);
                } else {
                    self.sound(pocket_ui::ui_sound::Sound::Error);
                    self.message = pocket_ui::i18n::tr("Couldn't change the disc");
                }
                return;
            }
            pause_list::Item::Earlier => {
                let result = if self.hardcore() {
                    Err(pocket_ui::achievements::hardcore::NO_LOAD.to_string())
                } else {
                    self.load_earlier()
                };
                self.sound(if result.is_err() { pocket_ui::ui_sound::Sound::Error } else { pocket_ui::ui_sound::Sound::Confirm });
                // Every error the screen shows goes through `said` (`save_trouble::tests`).
                match result {
                    Ok(()) => self.back_to_game_saying("Loaded an earlier moment".into()),
                    Err(e) => self.message = said(e, pocket_ui::save_trouble::LOAD_FAILED),
                }
                return;
            }
        };
        self.row = legacy;
        self.message.clear();
        self.activate();
        // Options opens on its top page's first row (Picture), as Linux's.
        if self.menu == Menu::Options {
            self.nav.reset();
            self.option_categories = self.core.options_snapshot().0;
        }
        // A save made a new picture: read again.
        self.previews.clear();
        // Save and Load that worked: back to the game, as Linux's (an error stays in the menu, said under the picture).
        if matches!(item, pause_list::Item::Save | pause_list::Item::Load) && self.paused && !self.message.is_empty() && self.last_ok {
            let said = std::mem::take(&mut self.message);
            // Save with Animations on Cozy and Motion on Full: its picture flies into the slot first (`save_fill`).
            if item == pause_list::Item::Save && pocket_ui::motion::cozy() && pocket_ui::motion::level() == pocket_ui::motion::Motion::Full && self.filled_slots.len() > 1 && self.slot < self.filled_slots.len() {
                self.saving = Some(pocket_ui::save_fill::Saving::new(self.slot, Instant::now()));
                log::info!("player save fill: slot {}", self.slot + 1);
                self.saving_said = said;
                self.saving_click = false;
            } else {
                self.back_to_game_saying(said);
            }
        }
    }
    /// While Save's picture flies (`saving`): the click as it lands, and the game back when it is over. True while it
    /// plays (the frames keep coming).
    fn save_fill_tick(&mut self) -> bool {
        let Some(saving) = self.saving else { return false };
        let since = Instant::now().saturating_duration_since(saving.at);
        if !self.saving_click && since >= pocket_ui::save_fill::FLY {
            self.saving_click = true;
            self.sound(pocket_ui::ui_sound::Sound::CardIn);
        }
        if saving.moment(Instant::now()).is_none() {
            self.end_save_fill();
            return false;
        }
        true
    }
    /// Save's picture is over (or a button ended it): the game goes on, saying what the save did.
    fn end_save_fill(&mut self) {
        if self.saving.take().is_some() {
            let said = std::mem::take(&mut self.saving_said);
            self.back_to_game_saying(said);
        }
    }
    /// Back to the game with `line` over it for a moment (the player's note at the top, as a start's).
    fn back_to_game_saying(&mut self, line: String) {
        self.pause(false);
        self.note = vec![line];
        self.note_until = None;
        self.overlay_size = None;
    }
    /// The list menu's keys on its main page: Up/Down go through the rows (round), Left/Right step the slot on Save and
    /// Load, the moment on Earlier and the disc on Disc, L/R the slot from any row, A does the row. B and Back are the
    /// sheet's (Continue).
    fn key_list(&mut self, key: i32) {
        let rows = self.list_rows();
        let n = rows.len().max(1);
        self.list_sel = self.list_sel.min(n - 1);
        let item = rows.get(self.list_sel).map(|r| r.item);
        if matches!(key, 19 | 20 | 21 | 22) {
            self.message.clear();
        }
        match key {
            19 => self.list_sel = (self.list_sel + n - 1) % n,
            20 => self.list_sel = (self.list_sel + 1) % n,
            21 | 22 if item == Some(pause_list::Item::Disc) => {
                if let Some(disc) = self.disc_row.as_mut() {
                    disc.step(if key == 21 { -1 } else { 1 });
                }
            }
            21 | 22 | 102 | 103 => {
                let back = matches!(key, 21 | 102);
                let on_slots = matches!(item, Some(pause_list::Item::Save | pause_list::Item::Load));
                if matches!(key, 21 | 22) && item == Some(pause_list::Item::Earlier) && !self.earlier_row.entries.is_empty() {
                    self.earlier_row.step(if back { -1 } else { 1 });
                } else if (on_slots || matches!(key, 102 | 103)) && self.places() > 0 {
                    let places = self.places();
                    self.slot = (self.slot + if back { places - 1 } else { 1 }) % places;
                }
            }
            23 | 66 | 96 => self.activate_list(),
            _ => (),
        }
    }
    /// What the game has for the Options page's rows, as Linux decides it (`opt_entries`): Emulator when the core has
    /// options, Controls always (Hold to toggle), Cheats when the game has any.
    fn opt_has(&self) -> pause_options::Has {
        pause_options::Has { emulator: !self.options.is_empty(), controls: true, cheats: self.cheats.is_some() }
    }
    /// The page the list menu shows under Options, Linux's (`pause_options`, `main.rs` `paint_options`): the top page
    /// (Picture ›, Emulator ›, Controls ›, Cheats ›, Speed, rumble, fan ›, Zoom ›, Keep for, Restore defaults, Screenshot)
    /// or the page it opened, each row with its value and line, and what it does.
    fn list_option_rows(&self) -> (Vec<pocket_ui::pause_page::OptRow>, Vec<OptTarget>) {
        use pocket_ui::pause_page::OptRow;
        let mut rows = Vec::new();
        let mut targets = Vec::new();
        let keep_game = self.picture.for_game();
        let Some(page) = self.nav.page else {
            for entry in pause_options::top(self.opt_has()) {
                let value = match entry {
                    pause_options::Entry::Keep => pause_options::keep_label(keep_game).to_string(),
                    // A page's row ends in its angle with a value too ("2 on ›"), as Linux's Cheats row.
                    pause_options::Entry::Cheats => match self.cheats.as_ref().map_or(0, |c| c.enabled()) {
                        0 => ">".to_string(),
                        on => format!("{on} on >"),
                    },
                    e if e.opens() => ">".to_string(),
                    _ => String::new(),
                };
                rows.push(OptRow::new(entry.label(), value, entry.desc(keep_game)));
                targets.push(OptTarget::Top(entry));
            }
            return (rows, targets);
        };
        match page {
            pause_options::Entry::Picture => {
                // Linux's Picture rows for this console and these values (sub-rows such as the frame's finish only under their row's value).
                let values = self.picture.values();
                let range = pocket_ui::render_scale::here(&self.system);
                for setting in pause_options::picture_settings(&self.system, range, &values) {
                    let value = pause_options::setting_value_of(setting.key, &self.system, range, &values);
                    let label = pocket_ui::option_table::label_in(setting, value, true);
                    rows.push(OptRow::new(setting.name, label, pause_options::setting_desc(setting.key, &self.system, range, &values)));
                    targets.push(OptTarget::Picture(setting.key));
                }
            }
            pause_options::Entry::Extras => {
                for entry in pause_options::EXTRAS_ROWS {
                    let value = match entry {
                        pause_options::Entry::FastForward => pause_options::ff_label(self.picture.ff_speed),
                        pause_options::Entry::Rewind => pause_options::earlier_label(self.picture.rewind),
                        pause_options::Entry::Earlier => pause_options::earlier_label(self.picture.earlier_on(&self.system)),
                        pause_options::Entry::Rumble => pause_options::rumble_label(self.picture.value(pause_options::RUMBLE_KEY)),
                        _ => pause_options::over_game_label(self.picture.over_game),
                    };
                    rows.push(OptRow::new(entry.label(), value, entry.desc(keep_game)));
                    targets.push(OptTarget::Top(entry));
                }
            }
            pause_options::Entry::Emulator => {
                // Linux's Emulator page (`core_options::page_rows`): the core's categories as pages, then its options.
                let shown = pocket_ui::core_options::page_rows(&self.option_categories, &self.options, self.nav.cat, |o| o.current.clone());
                let entries = pocket_ui::core_options::page_entries(&self.option_categories, &self.options, self.nav.cat);
                for ((name, value, desc), entry) in shown.into_iter().zip(entries) {
                    rows.push(OptRow::new(name, value, desc));
                    targets.push(match entry {
                        pocket_ui::core_options::PageEntry::Category(c) => OptTarget::Category(c),
                        // The core's option `k` is row `ROWS + 1 + k` of `display_options` (after the picture's and Hold to toggle).
                        pocket_ui::core_options::PageEntry::Option(k) => OptTarget::Option(picture::ROWS + 1 + k),
                    });
                }
            }
            pause_options::Entry::Controls => {
                // Linux's Controls page (`controls::ControlsPage`, the rows `emu_menu` shows): the sticks, each button,
                // Hold to toggle, Reset controls.
                for (i, (name, value, desc)) in self.pad_page.rows().into_iter().enumerate() {
                    rows.push(OptRow::new(name, value, desc));
                    targets.push(OptTarget::Control(i));
                }
            }
            pause_options::Entry::Cheats => {
                if let Some(cheats) = &self.cheats {
                    for (i, row) in cheats.rows().into_iter().enumerate() {
                        rows.push(OptRow { fixed: !cheats.actionable(i), ..OptRow::new(row.name, row.value, row.desc) });
                        targets.push(OptTarget::Cheat(i));
                    }
                }
            }
            _ => (),
        }
        (rows, targets)
    }
    /// The selected row's target, the selection kept on the page (never past the end).
    fn opt_target(&mut self) -> Option<OptTarget> {
        let (_, targets) = self.list_option_rows();
        if self.nav.sel >= targets.len() {
            self.nav.sel = 0;
        }
        targets.get(self.nav.sel).copied()
    }
    /// The Options page's keys: Up/Down go through the rows (round), Left/Right change a value (L1/R1 five values at a
    /// time on a long list, as Linux's), A opens a page or does the row, B goes back (`opt_back`).
    fn key_list_options(&mut self, key: i32) {
        let (_, targets) = self.list_option_rows();
        let n = targets.len();
        if n == 0 {
            return;
        }
        let at = self.nav.sel.min(n - 1);
        // Restore defaults asks twice: only an A on it right after the first one does it.
        let armed = std::mem::take(&mut self.restore_armed);
        if !matches!(key, 23 | 66 | 96) {
            self.message.clear();
        }
        match key {
            19 => self.nav.step(-1, n),
            20 => self.nav.step(1, n),
            21 | 22 | 102 | 103 => {
                self.nav.sel = at;
                let delta = if matches!(key, 21 | 102) { -1 } else { 1 };
                // L1 and R1 skip five values of a long list (`core_options::LONG_LIST`), as the row's line says.
                let long = matches!(targets[at], OptTarget::Option(i) if self.display_options().get(i).is_some_and(|o| o.values.len() > pocket_ui::core_options::LONG_LIST));
                self.opt_change(if matches!(key, 102 | 103) && long { delta * pocket_ui::core_options::LONG_STEP } else { delta });
            }
            23 | 66 | 96 => {
                self.nav.sel = at;
                self.restore_armed = armed;
                if !armed {
                    self.message.clear();
                }
                self.opt_activate();
            }
            _ => (),
        }
    }
    /// Left/Right on the selected row: a value steps; a page, Zoom and Screenshot have none.
    fn opt_change(&mut self, delta: i32) {
        match self.opt_target() {
            Some(OptTarget::Picture(key)) => {
                // Linux's next value of the row (a Look its sharpness and effect), kept as Keep for says.
                let pairs = pause_options::setting_step(key, &self.system, pocket_ui::render_scale::here(&self.system), &self.picture.values(), delta);
                if let Err(error) = self.picture.set_values(&pairs) { self.message = said(error, pocket_ui::save_trouble::SETTING_FAILED); }
                self.overlay_size = None;
            }
            Some(OptTarget::Top(pause_options::Entry::Earlier)) => {
                let on = !self.picture.earlier_on(&self.system);
                if let Err(error) = self.picture.set_earlier(on) { self.message = said(error, pocket_ui::save_trouble::SETTING_FAILED); }
                // On again: the first moment comes 5 minutes of play from now.
                self.timeline.stop();
            }
            Some(OptTarget::Top(pause_options::Entry::FastForward)) => {
                let speed = pause_options::ff_step(self.picture.ff_speed, delta);
                if let Err(error) = self.picture.set_ff_speed(speed) { self.message = said(error, pocket_ui::save_trouble::SETTING_FAILED); }
            }
            Some(OptTarget::Top(pause_options::Entry::Rewind)) => {
                let on = !self.picture.rewind;
                if let Err(error) = self.picture.set_rewind(on) { self.message = said(error, pocket_ui::save_trouble::SETTING_FAILED); }
                // Off: the history goes (as pocket-core's `apply_settings`); on: it starts from this moment.
                self.speed.forget();
            }
            Some(OptTarget::Top(pause_options::Entry::Rumble)) => {
                let next = pause_options::rumble_step(self.picture.value(pause_options::RUMBLE_KEY), delta);
                if let Err(error) = self.picture.set_values(&[(pause_options::RUMBLE_KEY, next.to_string())]) { self.message = said(error, pocket_ui::save_trouble::SETTING_FAILED); }
                pocket_ui::rumble::set_strength(pocket_ui::rumble::Strength::parse(next));
            }
            Some(OptTarget::Top(pause_options::Entry::OverGame)) => {
                let on = !self.picture.over_game;
                if let Err(error) = self.picture.set_over_game(on) { self.message = said(error, pocket_ui::save_trouble::SETTING_FAILED); }
                self.fps.restart();
            }
            // Keep for: the picture's "Save settings for" (this console, this game).
            Some(OptTarget::Top(pause_options::Entry::Keep)) => {
                if let Err(error) = self.picture.change(3, delta) { self.message = said(error, pocket_ui::save_trouble::SETTING_FAILED); }
            }
            Some(OptTarget::Option(i)) => {
                self.row = i;
                self.change_option(delta);
            }
            Some(OptTarget::Cheat(i)) => self.press_cheat(i),
            Some(OptTarget::Control(i)) => self.step_control(i, delta),
            _ => return,
        }
        self.sound(pocket_ui::ui_sound::Sound::Nav);
    }
    /// The Controls choices as kept from 17.11. until they moved into the game's settings (`ctl.*`, 18.11.): read once.
    fn legacy_control_path(&self) -> PathBuf {
        self.data.join("android-controls").join(format!("{}.ctl.json", self.key))
    }
    /// A step on the Controls page (`ControlsPage::step`, Linux's): applied at once, kept as `ctl.*` in the game's settings
    /// where it differs from the default and where Keep for says (this game or the console), as Linux keeps it.
    fn step_control(&mut self, row: usize, delta: i32) {
        let before = (self.pad_page.mapping, self.hold_button);
        let prefix = pocket_ui::controls::KEY_PREFIX;
        let kept = match self.pad_page.step(row, delta) {
            pocket_ui::controls::ControlsChange::Set(entries) => {
                let (mut set, mut remove) = (Vec::new(), Vec::new());
                for (key, value, default) in entries {
                    if default { remove.push(format!("{prefix}{key}")) } else { set.push((format!("{prefix}{key}"), value)) }
                }
                self.picture.set_controls(&set, &remove, false)
            }
            pocket_ui::controls::ControlsChange::Reset => self.picture.set_controls(&[], &[], true),
        };
        self.hold_button = self.pad_page.mapping.hold_toggle;
        self.held = Default::default();
        self.buttons = 0;
        if let Err(e) = kept {
            (self.pad_page.mapping, self.hold_button) = before;
            self.message = said(e, pocket_ui::save_trouble::SETTING_FAILED);
        }
    }
    /// The Controls choices from the game's settings (`ctl.*`), the older files of 17.11. taken over once.
    fn load_controls(&mut self) {
        let prefix = pocket_ui::controls::KEY_PREFIX;
        let has_ctl = |v: &std::collections::BTreeMap<String, String>| v.keys().any(|k| k.starts_with(prefix));
        let legacy = self.legacy_control_path();
        let hold_path = self.data.join("android-controls").join(format!("{}.json", self.key));
        if !has_ctl(&self.picture.values()) {
            let mut set: Vec<(String, String)> = pocket_ui::fsutil::read_json_or_backup::<std::collections::BTreeMap<String, String>>(&legacy).unwrap_or_default().into_iter().filter(|(k, _)| k.starts_with(prefix)).collect();
            // Hold to toggle from its own older file, Linux's way of writing it (`Mapping::hold_value`).
            if let Some(b) = pocket_ui::fsutil::read_json_or_backup::<Option<u8>>(&hold_path).flatten().filter(|b| *b < 16 && !(2..8).contains(b)) {
                let mut m = pocket_ui::controls::Mapping::defaults(&self.system);
                m.hold_toggle = Some(b);
                set.push((format!("{prefix}{}", pocket_ui::controls::HOLD_KEY), m.hold_value().to_string()));
            }
            if !set.is_empty() {
                match self.picture.set_controls(&set, &[], false) {
                    Ok(()) => log::info!("player controls: {} older choices moved into the game's settings", set.len()),
                    // Kept where they are: tried again at the next start.
                    Err(e) => {
                        log::warn!("player controls: older choices not moved: {e}");
                        return self.controls_from_settings();
                    }
                }
            }
        }
        let _ = std::fs::remove_file(&legacy);
        let _ = std::fs::remove_file(&hold_path);
        self.controls_from_settings();
    }
    /// The Controls page and the pad's input from the game's settings as they are now.
    fn controls_from_settings(&mut self) {
        self.pad_page.system = self.system.clone();
        self.pad_page.mapping = pocket_ui::controls::Mapping::from_values(&self.system, &self.picture.values());
        self.pad_page.targets = play_cards::targets(&self.system, &self.control_names);
        self.hold_button = self.pad_page.mapping.hold_toggle;
    }
    /// A cheat's row: on or off (a line that only explains does nothing), the codes to the core at once.
    fn press_cheat(&mut self, i: usize) {
        if let Some(cheats) = self.cheats.as_mut() {
            match cheats.try_press(i) {
                Ok(true) => {
                    let codes = cheats.codes();
                    self.core.set_cheats(&codes);
                }
                Err(e) => self.message = said(e.into(), pocket_ui::save_trouble::SETTING_FAILED),
                _ => (),
            }
        }
    }
    /// A on the selected row.
    fn opt_activate(&mut self) {
        match self.opt_target() {
            Some(OptTarget::Top(entry @ (pause_options::Entry::Picture | pause_options::Entry::Emulator | pause_options::Entry::Controls | pause_options::Entry::Cheats | pause_options::Entry::Extras))) => {
                self.nav.open(entry);
                self.live_key = None;
                self.sound(pocket_ui::ui_sound::Sound::Confirm);
            }
            Some(OptTarget::Category(cat)) => {
                self.nav.open_category(cat);
                self.sound(pocket_ui::ui_sound::Sound::Confirm);
            }
            Some(OptTarget::Top(pause_options::Entry::Zoom)) => self.open_list_zoom(),
            Some(OptTarget::Top(pause_options::Entry::Restore)) => {
                if !self.restore_armed {
                    self.restore_armed = true;
                    self.message = pause_options::RESTORE_ASK.into();
                } else {
                    self.restore_armed = false;
                    self.restore_defaults();
                }
                self.sound(pocket_ui::ui_sound::Sound::Confirm);
            }
            Some(OptTarget::Top(pause_options::Entry::Screenshot)) => {
                // The sheet's Screenshot (its sound and line), then back on this row.
                self.menu = Menu::Pause;
                self.row = 5;
                self.activate();
                self.menu = Menu::Options;
            }
            Some(OptTarget::Cheat(i)) => self.press_cheat(i),
            Some(OptTarget::Top(_) | OptTarget::Picture(_) | OptTarget::Option(_) | OptTarget::Control(_)) => {
                self.opt_change(1);
            }
            None => (),
        }
    }
    /// Restore defaults (Linux's): the picture's, Earlier's and Over the game's choices for the console and the game go,
    /// every emulator option is back on its default (at once, as a change on the Emulator page is) and its saved file is
    /// emptied, Hold to toggle is off. Resolution's default comes with the next start.
    fn restore_defaults(&mut self) {
        let mut failed = self.picture.restore_defaults().err();
        pocket_ui::rumble::set_strength(pocket_ui::rumble::Strength::Medium);
        for option in &mut self.options {
            if option.current != option.default && self.core.set_option(&option.key, &option.default) {
                option.current = option.default.clone();
            }
        }
        self.saved_options.clear();
        pocket_ui::fsutil::write_json_soon(&self.options_path, &self.saved_options);
        self.hold_button = None;
        self.held = Default::default();
        // Controls back to the console's own (their `ctl.*` went with the game's settings), as Linux's
        // `emu_menu::restored`.
        self.pad_page.mapping = pocket_ui::controls::Mapping::defaults(&self.system);
        self.overlay_size = None;
        self.fps.restart();
        self.message = failed.map_or_else(|| pause_options::RESTORE_DONE.into(), |error| said(error, pocket_ui::save_trouble::SETTING_FAILED));
        log::info!("player options: restored defaults");
    }
    /// B on the Options page: a category goes back to the Emulator page onto its row, a page to the top page onto its
    /// row; the top page is the caller's (the pause menu).
    fn opt_back(&mut self) -> bool {
        self.message.clear();
        self.nav.back()
    }
    fn display_options(&self) -> Vec<FileOption> {
        let mut options = self.picture.options();
        let choices = play_cards::targets(&self.system, &self.control_names);
        let mut values = vec![pocket_ui::core_options::FileValue {value:"none".into(), label:"Off".into()}];
        values.extend(choices.iter().filter(|t| pocket_ui::controls::can_hold(t.id)).map(|t| pocket_ui::core_options::FileValue {value:t.id.to_string(),label:t.label.clone()}));
        options.push(FileOption {key:"hold_toggle".into(), label:"Hold to toggle".into(), current:self.hold_button.map_or("none".into(),|b|b.to_string()), values, category:Some("Controls".into()), ..Default::default()});
        // The emulator's options in a player's words where we have them ("Less sprite flicker", not "PPU No sprite limit").
        options.extend(self.options.iter().cloned().map(|mut o| { pocket_ui::core_options::make_plain(&mut o); o }));
        for (row, option) in options.iter_mut().enumerate() { option.category = Some(Self::section(row).into()); }
        if let Some(cheats) = &self.cheats {
            for (i, row) in cheats.rows().into_iter().enumerate() {
                let actionable = cheats.actionable(i);
                options.push(FileOption { key:format!("cheat.{i}"), label:row.name, info:row.desc, current:row.value,
                    values: if actionable { ["Off", "On"].into_iter().map(|s| pocket_ui::core_options::FileValue {value:s.into(),label:s.into()}).collect() } else {vec![]},
                    category:Some("Cheats".into()), ..Default::default() });
            }
        }
        // A game of several discs (an `.m3u`, a multi-disc `.pbp`): which one is in, as the handhelds' Disc row
        // (`pocket_ui::disc`). Last, so the rows above keep their places.
        if let Some((at, count)) = self.core.disc().filter(|(_, count)| *count > 1) {
            options.push(FileOption { key: DISC_ROW.into(), label: "Disc".into(), current: at.to_string(),
                values: (0..count).map(|i| pocket_ui::core_options::FileValue { value: i.to_string(), label: format!("Disc {} of {}", i + 1, count) }).collect(),
                category: Some("Controls".into()), ..Default::default() });
        }
        options
    }
    fn section(row: usize) -> &'static str {
        if row < picture::ROWS { "Picture" } else if row == picture::ROWS { "Controls" } else { "Emulator" }
    }
    fn rows(&self) -> Vec<String> {
        if self.menu == Menu::Options {
            let mut rows: Vec<_> = self.display_options().iter().map(|o| format!("{} · {}", o.label, o.label_of(&o.current))).collect();
            rows.push("Back".into());
            return rows;
        }
        // Hardcore: Save, Load and Earlier stay in their places (the rows are matched by number), grey, and say why.
        let place = if self.hardcore() { "Off in Hardcore".to_string() } else { self.place_name() };
        let mut rows: Vec<String> = vec![
            "Continue".into(),
            format!("Save · {place}"),
            format!("Load · {place}"),
            "Earlier".into(),
            "Options".into(),
            "Screenshot".into(),
            "Controls".into(),
            "Zoom".into(),
            if self.recovery_required { "Return to library" } else { "Save & quit" }.into(),
        ];
        // Only in a game with an account signed in (RetroAchievements); the row comes last so the others keep their numbers.
        if self.ra.is_some() { rows.push("Achievements".into()); }
        rows
    }
    fn place_name(&self) -> String {
        if self.place_path() == pocket_ui::slots::undo_path(&self.base) { "Before load".into() }
        else { place_name(self.slot, self.filled_slots.len()) }
    }
    /// The slot picker's places: the slots, then the kept moments.
    fn places(&self) -> usize {
        self.filled_slots.len() + self.kept.len()
    }
    /// The file at the picker's place.
    fn place_path(&self) -> PathBuf {
        match self.slot.checked_sub(self.filled_slots.len()) {
            Some(kept) => self.kept[kept].clone(),
            None => pocket_ui::slots::state_path(&self.base, self.slot),
        }
    }
    fn change_option(&mut self, delta: i32) {
        // Back, Advanced and the list menu's Zoom and Screenshot have no value to change.
        if self.row >= self.display_options().len() {
            return;
        }
        let cheats = self.cheats.as_ref().map_or(0, |c| c.rows().len());
        if self.row == self.options.len() + picture::ROWS + 1 + cheats {
            // The Disc row: the tray opens, the other disc goes in, it closes (`Core::set_disc`).
            if let Some((at, count)) = self.core.disc().filter(|(_, count)| *count > 1) {
                let next = (at as i32 + delta).rem_euclid(count as i32) as u32;
                if self.core.set_disc(next) {
                    log::info!("player disc {} of {count}", next + 1);
                } else {
                    self.message = pocket_ui::i18n::tr("Couldn't change the disc");
                }
            }
            return;
        }
        if self.row < picture::ROWS {
            if let Err(error) = self.picture.change(self.row, delta) { self.message = said(error, pocket_ui::save_trouble::SETTING_FAILED); }
            self.overlay_size = None;
            return;
        }
        if self.row == picture::ROWS {
            let choices = play_cards::targets(&self.system, &self.control_names);
            let ids: Vec<_> = std::iter::once(None).chain(choices.iter().filter(|t| pocket_ui::controls::can_hold(t.id)).map(|t| Some(t.id))).collect();
            let at = ids.iter().position(|id| *id == self.hold_button).unwrap_or(0);
            let before=self.hold_button;
            self.hold_button = ids[(at as i32 + delta).rem_euclid(ids.len() as i32) as usize];
            self.held = Default::default();
            // Linux's ctl.hold_toggle in the game's settings, as the handhelds' Controls page keeps it (`ControlsPage`).
            self.pad_page.mapping.hold_toggle = self.hold_button;
            let key = format!("{}{}", pocket_ui::controls::KEY_PREFIX, pocket_ui::controls::HOLD_KEY);
            let kept = if self.hold_button == pocket_ui::controls::Mapping::defaults(&self.system).hold_toggle {
                self.picture.set_controls(&[], &[key], false)
            } else {
                self.picture.set_controls(&[(key, self.pad_page.mapping.hold_value().to_string())], &[], false)
            };
            if let Err(e) = kept {self.hold_button=before;self.pad_page.mapping.hold_toggle=before;self.message = said(e, pocket_ui::save_trouble::SETTING_FAILED);}
            return;
        }
        let cheat_start = self.options.len() + picture::ROWS + 1;
        if self.row >= cheat_start {
            if let Some(cheats) = self.cheats.as_mut() {
                match cheats.try_press(self.row - cheat_start) {
                    Ok(true) => { let codes = cheats.codes(); self.core.set_cheats(&codes); },
                    Err(e) => self.message = said(e.into(), pocket_ui::save_trouble::SETTING_FAILED),
                    _ => (),
                }
            }
            return;
        }
        let Some(option) = self.options.get_mut(self.row - picture::ROWS - 1) else { return };
        if option.values.is_empty() {
            return;
        }
        let at = option.values.iter().position(|v| v.value == option.current).unwrap_or(0);
        let i = (at as i32 + delta).rem_euclid(option.values.len() as i32) as usize;
        let value = option.values[i].value.clone();
        if self.core.set_option(&option.key, &value) {
            option.current = value.clone();
            self.saved_options.insert(option.key.clone(), value);
            pocket_ui::fsutil::write_json_soon(&self.options_path, &self.saved_options);
        }
    }
    fn activate(&mut self) {
        if self.menu == Menu::Options {
            if self.row == self.display_options().len() + 1 {
                // "Advanced (this emulator)": opens or closes its group.
                self.advanced_open = !self.advanced_open;
                self.sound(pocket_ui::ui_sound::Sound::Confirm);
            } else if self.row == self.display_options().len() {
                self.menu = Menu::Pause;
                self.row = 0;
            } else {
                self.change_option(1);
                self.sound(pocket_ui::ui_sound::Sound::Confirm);
            }
            return;
        }
        let result = match self.row {
            0 => {
                self.pause(false);
                return;
            }
            // The slot picker: A there chooses nothing (Left/Right change the slot), and never falls to Save & quit below.
            pause_head::PICKER => return,
            1 | 2 | 3 if self.hardcore() => Err(if self.row == 1 { pocket_ui::achievements::hardcore::NO_SAVE } else { pocket_ui::achievements::hardcore::NO_LOAD }.into()),
            // A kept moment is the only copy of it: never written over.
            1 if self.slot >= self.filled_slots.len() => Err(KEPT_ONLY_LOADS.into()),
            1 => {
                let path = self.place_path();
                self.state(&path)
            }
            // An empty slot has nothing to load: its button is grey and says why, as Linux's Load row does.
            2 => match pocket_ui::slots::load_refusal(&self.filled_slots, self.slot) {
                Some(why) => Err(why),
                None => {
                    let path = self.place_path();
                    self.load(&path)
                }
            },
            3 => {
                if !self.earlier_row.entries.iter().any(|e| !e.damaged()) {
                    Err(pocket_ui::save_trouble::NOTHING_EARLIER.into())
                } else {
                    // Touch has one Earlier button: successive presses move back through the same row as the pad.
                    while !self.earlier_row.loadable() { self.earlier_row.step(1); }
                    let loaded = self.load_earlier();
                    if loaded.is_ok() { self.earlier_row.step(1); }
                    loaded
                }
            }
            4 => {
                (self.option_categories, self.options) = self.core.options_snapshot();
                self.menu = Menu::Options;
                self.row = 0;
                self.scroll = 0;
                return;
            }
            5 => {
                let path = self.data.join("screenshots").join(format!(
                    "{}-{}.png",
                    self.key,
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
                ));
                self.frame.as_deref().ok_or_else(|| "No frame yet".into()).and_then(|image| png(&path, image))
            }
            6 => {
                self.menu = Menu::Controls;
                self.scroll = 0;
                return;
            }
            7 => {
                self.menu = Menu::Zoom;
                self.zoom = play_cards::Zoom::new(&self.system);
                return;
            }
            9 if self.ra.is_some() => {
                self.menu = Menu::Achievements;
                self.scroll = 0;
                self.refresh_achievements(true);
                return;
            }
            _ => {
                if self.recovery_required { self.quit = true; self.audio.take(); self.leave_last_picture(false); return; }
                let saved = self.automatic();
                if saved.is_ok() {
                    self.quit = true;
                    self.audio.take();
                    self.leave_last_picture(true);
                }
                saved
            }
        };
        self.sound(if result.is_err() {
            pocket_ui::ui_sound::Sound::Error
        } else if self.row == 5 {
            pocket_ui::ui_sound::Sound::Shutter
        } else {
            pocket_ui::ui_sound::Sound::Confirm
        });
        let slot = self.slot + 1;
        self.last_ok = result.is_ok();
        self.message = match (result, self.row) {
            (Ok(()), 1) => format!("Saved to slot {slot}"),
            (Ok(()), 2) if self.slot >= self.filled_slots.len() => "Loaded the kept moment".into(),
            (Ok(()), 2) => format!("Loaded slot {slot}"),
            (Ok(()), 3) => "Loaded an earlier moment".into(),
            (Ok(()), 5) => "Screenshot saved".into(),
            (Ok(()), _) => "Saved".into(),
            (Err(e), row) => said(e, match row {
                2 | 3 => pocket_ui::save_trouble::LOAD_FAILED,
                5 => pocket_ui::save_trouble::SCREENSHOT_FAILED,
                _ => pocket_ui::save_trouble::MOMENT_FAILED,
            }),
        };
    }
    fn input_mode(&mut self, touch: bool) {
        if self.touch_visible != touch {
            self.touch_visible = touch;
            self.overlay_size = None;
            self.fingers.clear();
            self.buttons = 0;
            self.stick_buttons = 0;
            self.held = Default::default();
            self.core.set_analog([0;4]);
            self.stylus.clear();
            self.core.set_pointer(None);
            self.stylus_finger = None;
            self.pulses = 0;
            self.touch_pulses = 0;
            self.menu_drag = None;
        }
    }
    fn slot_controls(&self) -> Vec<(Area, i32, &'static str)> {
        if !self.paused || self.menu != Menu::Pause {
            return Vec::new();
        }
        // The sheet's slot picker arrows, for touch and controller alike.
        vec![
            (self.slot_arrows[0], 102, "Previous slot"),
            (self.slot_arrows[1], 103, "Next slot"),
        ]
    }
    fn touch(&mut self, id: i32, x: f32, y: f32, phase: i32) -> bool {
        if phase == 0 && self.reminder.take().is_some() {
            self.pacer.restart();
            self.pacing.pause();
            self.swallow = Some(id);
            self.overlay_size = None;
            return true;
        }
        if phase == 0 {
            self.crt_done = true;
        }
        if phase == 3 {
            self.fingers.clear();
            self.buttons = 0;
            self.stick_buttons = 0;
            self.held = Default::default();
            self.core.set_analog([0;4]);
            self.stylus.clear();
            self.core.set_pointer(None);
            self.stylus_finger = None;
            self.pulses = 0;
            self.touch_pulses = 0;
            self.menu_drag = None;
            self.swallow = None;
            self.dragging = false;
            self.zoom_fingers.clear();
            self.zoom_distance = None;
            self.overlay_size = None;
            return !self.paused;
        }
        if phase == 0 && !self.touch_visible {
            self.input_mode(true);
            // A first touch reveals the controls (or the touch menu) without firing an invisible button.
            if !self.paused {
                return true;
            }
            self.swallow = Some(id);
            return false;
        }
        if self.swallow == Some(id) {
            if phase == 2 {
                self.swallow = None;
            }
            return false;
        }
        let x = (x - self.view.0 as f32) * self.logical.0 as f32 / self.view.2.max(1) as f32;
        // View stores the bottom inset for GL; input's top inset is kept in menu_button's map by frame().
        let top = self.input_top();
        let y = (y - top) * self.logical.1 as f32 / self.view.3.max(1) as f32;
        // The list menu (a game from the Handheld view): a tap on a row does it, on a hint key does that key.
        if self.paused && self.list_menu && (matches!(self.menu, Menu::Pause | Menu::Options | Menu::Achievements) || (self.menu == Menu::Zoom && self.from_options)) {
            if phase == 2 {
                if let Some(&(_, b)) = self.list_hint_hits.iter().find(|(a, _)| a.contains(x, y)) {
                    match b {
                        pocket_ui::pad::Button::South => { self.key(4, true); }
                        pocket_ui::pad::Button::East => { self.key(96, true); }
                        _ => (),
                    }
                } else if let Some(&(_, i)) = self.list_hits.iter().find(|(a, _)| a.contains(x, y)) {
                    if self.menu == Menu::Achievements {
                        self.ach_sel = i;
                    } else if self.menu == Menu::Pause {
                        self.list_sel = i;
                        self.activate_list();
                    } else if self.list_option_rows().1.get(i).is_some() {
                        self.nav.sel = i;
                        self.opt_activate();
                    }
                }
            }
            return false;
        }
        if self.paused && self.menu == Menu::Zoom {
            if phase == 2 && self.hits.iter().any(|(a,_,_)| a.contains(x,y)) {
                self.key(4,true);
                return false;
            }
            let previous = self.zoom_fingers.insert(id,(x,y));
            if phase == 1 {
                let points:Vec<_>=self.zoom_fingers.values().copied().collect();
                if points.len() == 2 {
                    let distance=((points[0].0-points[1].0).powi(2)+(points[0].1-points[1].1).powi(2)).sqrt();
                    if let Some(before)=self.zoom_distance {if before > 1.0 {self.zoom.magnify(distance/before);}}
                    self.zoom_distance=Some(distance);
                } else if let Some((px,py))=previous {
                    let travel=(self.zoom.scale-1.0).max(0.1);
                    self.zoom.pan((px-x)/self.logical.0 as f32/travel,(py-y)/self.logical.1 as f32/travel);
                }
            }
            if phase == 2 {self.zoom_fingers.remove(&id);self.zoom_distance=None;}
            return false;
        }
        if self.paused && matches!(self.menu, Menu::Controls | Menu::Recovery | Menu::Achievements) {
            if phase == 0 {self.menu_drag=Some((id,x,y));}
            if phase == 2 {
                if let Some((_,_,sy))=self.menu_drag.take() {
                    if (y-sy).abs()>30.0 {self.scroll=(self.scroll + if y<sy {3} else {-3}).clamp(0,self.scroll_max);}
                    else if self.hits.iter().any(|(a,_,_)|a.contains(x,y)) {self.key(4,true);}
                }
            }
            return !self.paused;
        }
        if self.paused {
            // A tap activates directly. A finger that travelled is a drag: it scrolls the Options page, else it is nothing.
            if phase == 0 {
                self.menu_drag = Some((id, x, y));
                self.scroll_base = self.scroll;
                self.dragging = false;
            }
            if phase == 1 {
                if let Some((_, _, sy)) = self.menu_drag.filter(|(finger, _, _)| *finger == id) {
                    if self.dragging || (y - sy).abs() > 24.0 {
                        self.dragging = true;
                        if self.menu == Menu::Options {
                            let scroll = (self.scroll_base as f32 - (y - sy)) as i32;
                            self.scroll = scroll.clamp(0, self.scroll_max);
                        }
                    }
                }
            }
            if phase == 2 {
                let drag = self.menu_drag.take().filter(|(finger, _, _)| *finger == id);
                let moved = drag.is_some_and(|(_, sx, sy)| self.dragging || (x - sx).abs().max((y - sy).abs()) > 35.0);
                self.dragging = false;
                if drag.is_none() || moved {
                    return false;
                }
                if let Some(key) = self.slot_controls().iter().find(|(a, _, _)| a.contains(x, y)).map(|(_, key, _)| *key) {
                    self.key(key, true);
                } else if let Some((row, step)) = self.option_arrows.iter().find(|(a, _, _, _)| a.contains(x, y)).map(|(_, row, step, _)| (*row, *step)) {
                    if self.menu == Menu::Options {
                        self.row = row;
                        self.change_option(step);
                        self.sound(pocket_ui::ui_sound::Sound::Nav);
                    }
                } else if let Some(row) = self.hits.iter().find(|(a, _, _)| a.contains(x, y)).map(|(_, row, _)| *row) {
                    self.row = row;
                    self.activate();
                }
            }
            return !self.paused;
        }
        if phase == 0 && self.menu_button.contains(x, y) {
            self.pause(true);
            return false;
        }
        let control_bits = self.bits_at(x, y);
        if self.system == "nds" {
            if phase == 0 && control_bits == 0 && self.stylus_finger.is_none() && self.game_area.is_some_and(|a| a.contains(x, y)) {
                self.stylus_finger = Some(id);
            }
            if self.stylus_finger == Some(id) {
                let point = if phase == 2 {
                    None
                } else {
                    self.game_area.filter(|a| a.contains(x, y)).map(|a| {
                        (
                            ((x - a.x as f32) / a.w.max(1) as f32 * 65534.0 - 32767.0) as i16,
                            ((y - a.y as f32) / a.h.max(1) as f32 * 65534.0 - 32767.0) as i16,
                        )
                    })
                };
                self.stylus.event(point);
                if phase == 2 {
                    self.stylus_finger = None;
                }
                return true;
            }
        }
        if phase == 2 {
            if let Some(bits) = self.fingers.remove(&id) {
                if bits != 0 {
                    self.overlay_size = None;
                }
            }
        } else {
            let bits = control_bits;
            let before = self.fingers.insert(id, bits).unwrap_or(0);
            self.touch_pulses |= bits & !before;
            if self.haptics && bits & !before != 0 {
                // A tick for each new press (and for sliding onto another direction or button).
                let _ = pocket_ui::platform::android::strings("haptic", &[]);
            }
            if bits != before {
                // The pressed control lights up: the cached overlay is painted again.
                self.overlay_size = None;
            }
        }
        !self.paused
    }
    /// The pad bits under (x, y): the buttons' areas, and the D-pad by the finger's angle from its centre (8 ways;
    /// a small dead middle; generous edges so a thumb can slide round it).
    fn bits_at(&self, x: f32, y: f32) -> u16 {
        let mut bits = self.controls.iter().filter(|(a, _)| a.contains(x, y)).fold(0u16, |v, (_, bit)| v | 1 << bit);
        if let Some((cx, cy, r)) = self.dpad {
            let (dx, dy) = (x - cx, y - cy);
            let d = (dx * dx + dy * dy).sqrt();
            if d > r * 0.18 && d < r * 1.35 {
                let sector = ((dy.atan2(dx).to_degrees() + 360.0 + 22.5) % 360.0 / 45.0) as u8;
                // 0 right, 1 down-right, 2 down, 3 down-left, 4 left, 5 up-left, 6 up, 7 up-right.
                let (right, down, left, up) = (1 << 7, 1 << 5, 1 << 6, 1 << 4);
                bits |= [right, right | down, down, down | left, left, left | up, up, up | right][sector as usize % 8];
            }
        }
        bits
    }
    fn input_top(&self) -> f32 {
        self.top as f32
    }
    fn timeline_play(&mut self) {
        use pocket_ui::timeline::Moment;
        if !self.picture.earlier_on(&self.system) || self.hardcore() { self.timeline.stop(); return; }
        self.timeline.play(Instant::now());
        if !self.timeline.due() { return; }
        let core = &self.core;
        if self.timeline.moment(self.core.hw.is_some(), None, || core.state_size(), 1000.0 / self.core.av.timing.fps as f32) == Moment::InPlay {
            self.timeline_capture(Moment::InPlay);
        }
    }
    fn timeline_capture(&mut self, moment: pocket_ui::timeline::Moment) {
        if !self.core.has_states() { self.timeline.skip(); return; }
        let started = Instant::now();
        let Some(bytes) = self.core.save_state() else { self.timeline.failed(); return; };
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        let frame = self.frame.as_deref().map(|image| pocket_ui::libretro::Frame {
            width: image.w, height: image.h, pitch: image.w as usize * 4,
            format: pocket_ui::libretro::PIXEL_XRGB8888,
            data: image.px.iter().flat_map(|p| p.to_ne_bytes()).collect(),
        });
        self.timeline.submit_state(&self.base, bytes, frame, ms, moment);
    }
    fn timeline_quiet(&mut self) {
        if self.paused && self.timeline_paused.is_some_and(|at| at.elapsed() >= pocket_ui::timeline::QUIET_AFTER)
            && self.picture.earlier_on(&self.system) && !self.hardcore() && self.timeline.due() && !self.recovery_required {
            self.timeline_capture(pocket_ui::timeline::Moment::Quiet);
        }
    }
    fn frame(&mut self, w: i32, h: i32, density: f32, insets: [i32; 4], font_scale: f32) -> i32 {
        if self.quit {
            self.timeline.wait();
            return 2;
        }
        self.timeline_quiet();
        if self.last_font_scale != font_scale {self.overlay_size=None;self.last_font_scale=font_scale;}

        pocket_ui::type_scale::set_system_scale(font_scale);
        let size = play_cards::text_size(self.text_size, font_scale);
        pocket_ui::type_scale::set(size);
        let [left, top, right, bottom] = insets;
        self.density = density;
        let inset = (left, bottom, (w - left - right).max(1), (h - top - bottom).max(1));
        self.game_view = inset;
        // The handhelds' pause menu fills the panel as Linux's does (its own 30 px margins), not the safe insets Android
        // reports round the Mini V2's corners (60/48/60/64 px: the menu stood shrunk in the middle, 17.11.); the game and
        // the touch controls keep them.
        let menu = self.paused && self.list_menu;
        let view = if menu { (0, 0, w.max(1), h.max(1)) } else { inset };
        self.top = if menu { 0 } else { top };
        if self.view != view {
            log::info!("player viewport: physical={w}x{h} insets={insets:?} density={density} menu={menu}");
        }
        self.view = view;
        self.window = (w.max(0) as u32, h.max(0) as u32);
        // The handhelds' pause menu is laid out as Linux lays it out on this screen (`scaled_layout::logical_size`: the Mini
        // V2's 1240x1080 at 1:1), so its pages are Linux's in every size; the game and the touch controls keep the
        // density's layout.
        self.logical = if menu {
            pocket_ui::scaled_layout::logical_size((self.view.2 as u32, self.view.3 as u32))
        } else {
            pocket_ui::scaled_layout::mobile_size((self.view.2 as u32, self.view.3 as u32), density)
        };
        // Overlay pixels per dp: the touch controls are sized in dp (at least 56 dp a button).
        self.dp = self.logical.0 as f32 / (self.view.2.max(1) as f32 / density.max(0.5));
        // The compositor can run below the core rate (notably SwiftShader on high-DPI phones).
        // Catch up bounded emulation ticks before painting once; audio must not lose samples when
        // a presentation misses vsync. Core calls remain on the EGL thread for hardware cores.
        // One core frame per display refresh near the display's rate, as pocket-core on Linux (`core_pacing`): the
        // wall clock against the core's own rate repeated and doubled pictures for seconds every ten (SNES, 17.11.).
        let call = Instant::now();
        let mut readback_time = Duration::ZERO;
        let due = if !self.paused && self.reminder.is_none() { self.pacer.due(call) } else { 0 };
        if due > 0 {
            self.placement.keep(call);
        }
        let mut ticks = 0;
        let mut core_time = Duration::ZERO;
        // Fast-forward and rewind, as pocket-core runs them (`play_speed`): HOME + L2 held steps one state back per refresh,
        // HOME + R2 runs several frames in one, the sound muted meanwhile. The pacer is untouched: it still says when a
        // refresh is due; only what runs in it changes.
        let rewind_held = due > 0 && self.speed.rewind_held();
        let turn = if due > 0 { self.speed.hold.turn(rewind_held, self.picture.rewind, rewind_held && self.hardcore()) } else { Default::default() };
        if let Some(line) = turn.notice {
            self.note = vec![line.to_string()];
            self.note_until = None;
            self.overlay_size = None;
        }
        if let Some(rewound) = turn.ended {
            log::info!("player rewound {rewound} frames, {} left in the history", self.speed.history.len());
            // The moment rewound to can't unlock anything by itself.
            if let Some(ra) = &mut self.ra { ra.reset(); }
        }
        let back = turn.back;
        let ff = if back || due == 0 { None } else { self.speed.fast_forward(self.picture.ff_speed) };
        let runs = if back { due.min(1) } else { pocket_ui::play_speed::runs(ff, due) };
        if back {
            // One state back per refresh; at the oldest the picture holds.
            if let Some(state) = self.speed.history.step_back() {
                let _ = self.core.load_state(state);
                let (frame, _, _, _) = self.core.run(0);
                if let Some(frame) = core_hw_frame(&self.core).or(frame) {
                    self.video_frames += 1;
                    self.frame = Some(player_video::image(frame));
                }
                if let Some(ra) = &mut self.ra { ra.idle(); }
                self.speed.hold.rewound += 1;
            }
            if let Some(audio) = &mut self.audio {
                audio.push(&vec![0; self.speed.nominal_samples]);
            }
        }
        // Fast-forward shows the refresh's last frame: the others run without picture or sound where the core can skip
        // them (GET_AUDIO_VIDEO_ENABLE), 3D cores make every frame but only the last is read back.
        let skip = ff.is_some() && self.core.hw.is_none();
        let mut ff_samples = 0;
        while ticks < runs && !back {
            ticks += 1;
            let touch = self.fingers.values().fold(self.touch_pulses, |v, b| v | b);
            // The handheld's buttons as Options › Controls sets them (Linux's `Mapping::translate`), a press too short for
            // a frame (`pulses`) as well; the touch pad's are the console's own.
            // D-pad as left stick: the D-pad's presses move the stick instead (`controls::pad_input`). While HOME is down the
            // pad's buttons belong to the shortcuts, not the game (pocket-core's rule).
            let (held, pulses, stick) = if self.speed.home_held() { (0, 0, 0) } else { (self.buttons, self.pulses, self.stick_buttons) };
            let ds = if self.speed.home_held() {
                self.ds_pad.suppress(self.buttons | self.pulses | self.stick_buttons);
                pocket_ui::ds_pad::Frame { raw: 0, buttons: 0, axes: self.axes, changed: None }
            } else { self.ds_pad.apply(&self.system, &self.pad_page.mapping, held | pulses | stick, self.axes) };
            let (mut buttons, sticks) = pocket_ui::controls::pad_input(&self.pad_page.mapping, ds.raw, 0, 0, touch, ds.axes);
            self.core.set_analog(sticks.unwrap_or(ds.axes));
            if let Some(stylus) = ds.changed {
                self.held.clear(); self.overlay_size = None;
                self.note = vec![pocket_ui::i18n::tr(if stylus { "Stylus mode" } else { "D-pad mode" })];
                self.note_until = None;
            }
            self.pulses = 0;
            self.touch_pulses = 0;
            if self.touch_visible {
                if let Some((axes, consumed)) = play_cards::touch_analog(&self.system, touch) {
                    self.core.set_analog(axes);
                    buttons &= !consumed;
                }
            }
            let was_held = self.held.held();
            buttons = self.held.apply(buttons,self.hold_button) | ds.buttons;
            self.diagnostic_buttons_seen |= buttons;
            if was_held != self.held.held() {self.overlay_size = None;}
            self.core.set_pointer(self.stylus.sample());
            pocket_ui::mark!("android-core-run", "phase=begin frame={}", self.frames);
            for player in 1..4 {
                let (bits, sticks) = self.pads.player(player);
                self.core.set_player(player, bits, sticks);
            }
            let given = [buttons, self.pads.player(1).0, self.pads.player(2).0, self.pads.player(3).0];
            if given != self.last_given {
                log::info!("player input: p1={:04x} p2={:04x} p3={:04x} p4={:04x}", given[0], given[1], given[2], given[3]);
                self.last_given = given;
            }
            let run_started = Instant::now();
            let last = ff.is_none() || pocket_ui::play_speed::last_run(ff, ticks - 1, runs, call.elapsed().as_secs_f32() * 1000.0);
            let enable = match (skip, last) {
                (false, _) => pocket_ui::libretro::AV_ALL,
                (true, true) => pocket_ui::libretro::AV_VIDEO,
                (true, false) => 0,
            };
            let (frame, samples, av, geometry) = self.core.run_with(buttons, enable);
            core_time += run_started.elapsed();
            pocket_ui::mark!("android-core-run", "phase=end frame={} samples={}", self.frames, samples.len() / 2);
            if let Some(av) = av {
                self.core.av = av;
                self.pacer.set_rates(av.timing.fps, self.display_hz);
                if let Some(audio) = &mut self.audio {
                    audio.set_paced_timing(av.timing.sample_rate, av.timing.fps, self.display_hz);
                }
            }
            if let Some(geometry) = geometry {
                self.core.av.geometry = geometry;
            }
            pocket_ui::mark!("android-readback", "phase=begin frame={}", self.frames);
            let readback_started = Instant::now();
            let frame = if last { core_hw_frame(&self.core).or(frame) } else { None };
            readback_time += readback_started.elapsed();
            pocket_ui::mark!("android-readback", "phase=end frame={}", self.frames);
            if let Some(frame) = frame {
                self.video_frames += 1;
                self.frame = Some(player_video::image(frame));
            }
            if ff.is_some() {
                // Muted, but the queue gets one frame's silence per refresh so playback goes on smoothly after.
                ff_samples = samples.len();
            } else {
                if !samples.is_empty() {
                    self.speed.nominal_samples = samples.len();
                }
                if let Some(audio) = &mut self.audio {
                    audio.push(&samples);
                }
            }
            self.frames += 1;
            // RetroAchievements reads the game's memory after each emulated frame (well under a millisecond).
            if let Some(ra) = &mut self.ra { ra.frame(); }
            if self.last_flush.elapsed() > Duration::from_secs(10) {
                self.core.write_sram_in_background();
                self.note_save_trouble(self.core.save_trouble());
                self.last_flush = Instant::now();
                // The pacing of the last ten seconds, and the display's rate again (a phone may switch its mode).
                let cpu = pocket_ui::core_pacing::current_cpu();
                let mhz = pocket_ui::core_pacing::cpu_mhz(std::path::Path::new("/"), cpu);
                let fast = self.placement.label();
                log::info!("player pacing: {}", self.pacing.line(self.pacer.locked(), self.core.av.timing.fps, self.display_hz, cpu, mhz, &fast));
                let hz = display_refresh_hz();
                if (hz - self.display_hz).abs() > 0.01 {
                    self.display_hz = hz;
                    self.pacer.set_rates(self.core.av.timing.fps, hz);
                    if let Some(audio) = &mut self.audio {
                        audio.set_paced_timing(self.core.av.timing.sample_rate, self.core.av.timing.fps, hz);
                    }
                }
                if let Some(audio) = &self.audio {
                    let underruns = *audio.underruns.lock().unwrap_or_else(|e| e.into_inner());
                    let queue_ms = audio.buffered_ms();
                    let diagnostics = audio.diagnostics();
                    let dropped = diagnostics["queue"]["active_dropped_frames"].as_u64().unwrap_or(0);
                    if pocket_ui::platform::android::string("diagnosticAudio") == "true" {
                        log::info!("player supply diagnostic: missing_frames={} startup_gap_frames={} startup_last_missing_frame={} steady_gap_frames={}",diagnostics["queue"]["playing_missing_frames"],diagnostics["queue"]["startup_missing_burst_max_frames"],diagnostics["queue"]["startup_last_missing_frame"],diagnostics["queue"]["steady_missing_burst_max_frames"]);
                        log::info!("player startup diagnostic: reserve_frames={} trimmed_frames={} trim_peak={} splice_step={} return_frame={}",diagnostics["startup_reserve_frames"],diagnostics["startup_silent_trimmed_frames"],diagnostics["startup_trim_peak"],diagnostics["startup_splice_step"],diagnostics["startup_return_frame"]);
                    }
                    let input_frames = diagnostics["queue"]["input_frames"].as_u64().unwrap_or(0);
                    let resampled_frames = diagnostics["queue"]["resampled_frames"].as_u64().unwrap_or(0);
                    log::info!("player audio: frames={} underruns={} queue_ms={queue_ms:.2} dropped_frames={dropped} input_rate={:.1} target_ms={:.1} video_frames={} input_frames={input_frames} resampled_frames={resampled_frames}{} read_buffer_overrides={}", self.frames, underruns, audio.input_rate(), audio.target_ms(), self.video_frames, pocket_ui::platform::android::audio_line(), pocket_ui::hw_render::read_buffer_overrides());
                }
                if pocket_ui::platform::android::string("diagnosticAudio") == "true" {
                    let (reads,bits)=self.core.take_input_probe();
                    log::info!("player input diagnostic: buttons_seen={} core_reads={reads} core_bits={bits}",std::mem::take(&mut self.diagnostic_buttons_seen));
                    if let Some(image) = &self.frame {
                        let nonzero = image.px.iter().filter(|&&p| p & 0xff_ffff != 0).count();
                        log::info!("player picture diagnostic: width={} height={} nonzero_rgb_pixels={nonzero}",image.w,image.h);
                    }
                }
            }
            self.timeline_play();
            if ff.is_some() && last {
                break;
            }
        }
        if ff.is_some() && ticks > 0 {
            if let Some(audio) = &mut self.audio {
                audio.push(&vec![0; if ff_samples == 0 { self.speed.nominal_samples } else { ff_samples }]);
            }
        }
        // Rewind on: the refresh's state goes into the history (one per refresh, as pocket-core keeps it).
        if self.picture.rewind && !back && ticks > 0 && self.core.save_state_into(&mut self.speed.scratch) {
            self.speed.history.push(&self.speed.scratch);
        }
        if due > 0 {
            let badge = pocket_ui::play_speed::badge(back, ff);
            if badge != self.speed.badge {
                self.speed.badge = badge;
                self.overlay_size = None;
            }
        }
        // Emulated frames this refresh: a step back is one, as pocket-core counts it.
        let emulated = if back { 1 } else { ticks };
        if !self.paused && self.reminder.is_none() {
            // The pacing line judges the pacer (repeats, doubles): while fast-forward or rewind runs, what it asked for.
            self.pacing.call(call, if back || ff.is_some() { due } else { ticks }, core_time);
            self.pacing.missed(self.pacer.missed());
            if ticks > 0 && self.placement.is_slow(pocket_ui::core_pacing::current_cpu()) {
                self.pacing.on_slow_cpu();
            }
        }
        // Over the game → FPS: the line changes twice a second, and then the overlay is painted again.
        if !self.paused && self.reminder.is_none() && self.picture.over_game && self.fps.frame(Instant::now(), emulated) {
            self.overlay_size = None;
        }
        // Achievements: while paused rc_client's timers go on and the Achievements page keeps its list current; in the
        // game the unlock cards and notes come up (a card is only taken from the line while it can be seen).
        if let Some(ra) = &mut self.ra {
            if self.paused {
                ra.idle();
            } else {
                let (notes, changed) = ra.poll(Instant::now());
                if !notes.is_empty() {
                    self.note.extend(notes);
                    self.note_until = None;
                    self.overlay_size = None;
                }
                if changed { self.overlay_size = None; }
            }
        }
        if self.paused && self.menu == Menu::Achievements { self.refresh_achievements(false); }
        if self.frames > 0 && !self.note.is_empty() && !self.paused {
            let until = *self.note_until.get_or_insert_with(|| Instant::now() + if self.list_menu { LINUX_NOTE_FOR } else { NOTE_FOR });
            if Instant::now() >= until {
                self.note.clear();
                self.overlay_size = None;
            }
        }
        if self.paused {
            let now = self.clock.elapsed().as_millis() as u64;
            if let Some(step) = self.hat_repeat.due(now) {
                self.key(step, true);
            }
        }
        pocket_ui::mark!("android-present", "phase=begin frame={}", self.frames);
        let paint_started = Instant::now();
        self.paint();
        pocket_ui::mark!("android-present", "phase=end frame={}", self.frames);
        if !self.paused && self.reminder.is_none() {
            self.pacing.phases(readback_time, paint_started.elapsed());
        }
        if self.frames > 0 && !self.first_frame {
            // The tests' end of "A or tap to the game's first frame" (the launcher logs the start).
            self.first_frame = true;
            log::info!("player first game frame");
        }
        // 1: the menu is up and nothing moves (a frame only when something happens); 3: a direction is held in it, or the
        // Picture page's preview waits for its frame to be drawn.
        let filling = self.paused && self.save_fill_tick();
        if self.paused && (self.hat_repeat.held() || self.live_pending || filling) {
            3
        } else if self.paused {
            1
        } else {
            0
        }
    }
    /// Where the pad reaches on this screen (`player_touch::pad_extent`): measured by laying the pad out once for each layout.
    fn pad_extent(&mut self, size: (i32, i32), game: Area) -> (i32, i32) {
        let key = ((size.0 as u32, size.1 as u32), (game.x, game.y, game.w, game.h));
        let dp = (self.dp * 100.0) as u32;
        match &self.pad_extent {
            Some((k, system, d, extent)) if *k == key && *d == dp && *system == self.system => *extent,
            _ => {
                let extent = player_touch::pad_extent(&mut self.text, &self.system, size, self.dp, Some(game));
                self.pad_extent = Some((key, self.system.clone(), dp, extent));
                extent
            }
        }
    }
    fn control_label(&self, bit: u16) -> &'static str {
        play_cards::touch_label(&self.system, bit as usize)
    }
    fn semantic_nodes(&self) -> String {
        let areas: Vec<_> = if self.paused {
            let mut nodes: Vec<_> = self.hits.iter().map(|(a, i, label)| (*a, label.clone(), !self.touch_visible && *i == self.row)).collect();
            nodes.extend(self.slot_controls().into_iter().map(|(a, _, label)| (a, pocket_ui::i18n::tr(label), false)));
            if self.menu == Menu::Zoom {nodes.push((Area::new(0,0,self.logical.0 as i32,80),format!("{} · {:.1}x",pocket_ui::i18n::tr("Zoom"),self.zoom.scale),false));}
            if self.menu == Menu::Options {
                nodes.extend(self.option_arrows.iter().map(|(a, _, _, label)| (*a, pocket_ui::i18n::tr(label), false)));
            }
            if self.menu == Menu::Achievements {
                nodes.push((Area::new(36, 150, self.logical.0 as i32 - 72, self.logical.1 as i32 - 300), self.ra_lines.iter().filter(|l| !l.is_empty()).cloned().collect::<Vec<_>>().join(". "), false));
            }
            if matches!(self.menu, Menu::Controls | Menu::Recovery) {
                nodes.push((Area::new(36,150,self.logical.0 as i32-72,self.logical.1 as i32-300),if self.menu == Menu::Recovery {self.note.iter().map(|l|pocket_ui::i18n::tr(l)).collect::<Vec<_>>().join(". ")} else {play_cards::control_lines(&self.system,&self.title,&self.control_names,self.touch_visible).join(". ")},false));
            }
            if !self.message.is_empty() {nodes.push((Area::new(40,100,self.logical.0 as i32-80,60),self.message.clone(),false));}
            nodes
        } else {
            let mut nodes: Vec<_> = self.controls.iter().map(|(a, bit)| (*a, self.control_label(*bit).to_string(), false)).collect();
            if self.touch_visible && self.reminder.is_none() {
                nodes.push((self.menu_button, "Pause".into(), false));
            }
            if !self.note.is_empty() {
                nodes.push((self.note_area, self.note.iter().map(|l| note_text(l)).collect::<Vec<_>>().join(". "), false));
            }
            if !self.card_label.is_empty() {
                nodes.push((self.card_area, self.card_label.clone(), false));
            }
            if let Some(bit)=self.held.held() {
                let label=pocket_ui::i18n::tr("{s} held").replace("{s}",self.control_label(bit as u16));
                nodes.push((Area::new(28,20,self.logical.0 as i32-56,type_scale::row(64,28)),label,false));
            }
            if let Some(card)=&self.reminder {
                nodes.push((Area::new(40,self.logical.1 as i32-360,self.logical.0 as i32-80,320),std::iter::once(pocket_ui::i18n::tr(&card.ago)).chain(card.played.as_deref().map(pocket_ui::i18n::tr)).chain(card.note.clone()).collect::<Vec<_>>().join(". "),false));
            }
            nodes
        };
        let nodes:Vec<_>=areas.into_iter().enumerate().map(|(id,(a,label,selected))|serde_json::json!({"id":id,"label":label,"selected":selected,"x":self.view.0+a.x*self.view.2/self.logical.0 as i32,"y":self.top+a.y*self.view.3/self.logical.1 as i32,"w":a.w*self.view.2/self.logical.0 as i32,"h":a.h*self.view.3/self.logical.1 as i32})).collect();
        serde_json::to_string(&nodes).unwrap_or_default()
    }
    fn paint(&mut self) {
        palette::set_blend(if self.light { 255 } else { 0 });
        // The list menu's page, gathered before anything borrows the picture (`list_paint` reads the slots' pictures).
        let list = if self.paused && self.list_menu && (matches!(self.menu, Menu::Pause | Menu::Options | Menu::Achievements) || (self.menu == Menu::Zoom && self.from_options)) { Some(self.list_paint()) } else { None };
        let (w, h) = (self.logical.0 as i32, self.logical.1 as i32);
        // A wide screen's pages of rows are larger (`type_scale::wide_page`), as Linux's pause menu's.
        let _wide = list.as_ref().map(|_| pocket_ui::type_scale::panel(pocket_ui::type_scale::wide_page(w, h)));
        // The Picture page: where its preview goes, and the preview itself for the choices as they are now.
        let picture_page = match &list {
            Some(ListPaint::Options { picture: true, .. }) => {
                let page = self.picture_layout(w, h);
                self.refresh_live(page.1);
                Some(page)
            }
            _ => None,
        };
        // Where the picture goes, and in which pixels (`picture::Settings::place`).
        let space = self.frame.as_deref().map(|image| self.picture.place(
            (self.window.0 as i32, self.window.1 as i32), self.view, (w,h), (image.w,image.h),
            self.core.av.geometry.aspect_ratio as f64, self.touch_visible,
        ));
        let game_rect = space.map(|s| s.game);
        let (pic_size, pic_view) = space.map_or(((w, h), self.view), |s| (s.size, s.view));
        self.gfx.variant = self.picture.variant(&self.system);
        // Linux's frame around the picture, in the room the touch pad leaves (`picture::Settings::frame_plan`).
        let frame_plan = match game_rect {
            Some(rect) if self.picture.frame != 0 && !self.paused => {
                let pad = if self.touch_visible { self.pad_extent((w, h), rect) } else { (0, pic_size.1) };
                let room = picture::Settings::frame_room(pic_size, self.touch_visible, rect, pad);
                self.frame.as_deref().and_then(|image| self.picture.frame_plan(&self.system, pic_size, pic_view, room, rect, image.w))
            }
            _ => None,
        };
        // A game that had no TV switches its display on instead of the tube (`crt_on::Screen`, as on Linux).
        let screen = match pocket_ui::eject::Screen::of(pocket_ui::cabinet::FormFactor::of(&self.system)) {
            pocket_ui::eject::Screen::Lcd => pocket_ui::crt_on::Screen::Lcd,
            pocket_ui::eject::Screen::Tv => pocket_ui::crt_on::Screen::Tv,
        };
        let mut squeezed = None;
        let crt = if let Some(rect) = game_rect.filter(|_| !self.paused && !self.crt_done) {
            let started = if let Some(started) = self.crt_started {
                started
            } else {
                let now = Instant::now();
                self.crt_started = Some(now);
                pocket_ui::crt_on::start();
                if screen == pocket_ui::crt_on::Screen::Tv {
                    self.sound(pocket_ui::ui_sound::Sound::CrtOn);
                }
                now
            };
            let area = (rect.x as f32, rect.y as f32, rect.w as f32, rect.h as f32);
            let quads = pocket_ui::crt_on::plan_for(screen, started.elapsed(), area);
            if quads.is_empty() {
                self.crt_done = true;
            }
            // The tube's picture opens out of its line, squeezed into what is lit.
            let (_, y, _, h) = pocket_ui::crt_on::picture(screen, started.elapsed(), area);
            squeezed = Some((y.round() as i32, (h.round() as i32).max(1)));
            quads
        } else {
            Vec::new()
        };
        // Touches on the picture and the way back's picture read it in the overlay's pixels.
        let game_area = space.map(|s| s.in_overlay(self.window.1 as i32, self.view, (w, h)));
        if let Some(rect) = game_rect {
            let changed = self.game_area != game_area;
            if changed { log::info!("player picture: system={} logical={}x{} rect={},{},{}x{} physical={}x{} on_window={:?} scaling={} frame={} video={:?} touch={} plan={}", self.system,w,h,rect.x,rect.y,rect.w,rect.h,self.view.2,self.view.3,space.map(|s| s.on_window(self.window.1 as i32)),self.picture.scaling,self.picture.frame,self.picture.video,self.touch_visible,frame_plan.is_some()); }
        }
        self.game_area = game_area;
        let game = if self.paused { None } else { self.frame.as_deref().zip(game_rect.map(|r| match squeezed { Some((y, h)) => Area { y, h, ..r }, None => r })) };
        if !self.paused && self.overlay_size == Some(self.logical) {
            self.core.frontend_current();
            self.gfx.present(&self.pixels, (w, h), self.view, game, false, &crt, frame_plan.as_ref(), (pic_size, pic_view));
            return;
        }
        let held_label = self.held.held().map(|b| self.control_label(b as u16).to_string());
        self.pixels.resize((w * h) as usize, 0);
        let rows = self.rows();
        let display_options = self.display_options();
        // Hardcore greys Load and Earlier in the sheet (read before the canvas holds the pixels).
        let hardcore = self.hardcore();
        let selected_place_name = self.place_name();
        let mut c = Canvas::new(&mut self.pixels, w as u32, h as u32);
        c.clear(0);
        // Paused backgrounds are painted only when the menu changes; active frames are scaled by GLES.
        if self.paused {
            // In the overlay's pixels (`game_area`), where the picture stands behind the sheet.
            if let Some((image, rect)) = self.frame.as_deref().zip(self.game_area) {
                c.blit_scaled(image, rect.x, rect.y, rect.w, rect.h);
            }
        }
        self.controls.clear();
        self.menu_button = if self.touch_visible {
            Area::new(w / 2 - 70, h - 78, 140, 64)
        } else {
            Area::new(0, 0, 0, 0)
        };
        if !self.paused && self.touch_visible && self.reminder.is_none() {
            let pressed = self.fingers.values().fold(0u16, |v, b| v | b);
            let out = touch_controls(&mut c, &mut self.text, &self.system, (w, h), self.dp, game_rect, pressed);
            self.controls = out.0;
            self.dpad = out.1;
            self.menu_button = out.2;
        }
        if !self.paused && !self.note.is_empty() {
            // A game from the Handheld view says it as Linux does (pocket-core's pill at the bottom centre); the Mobile
            // view keeps its band at the top, clear of the touch pad.
            self.note_area = if self.list_menu { paint_note_pill(&mut c, &mut self.text, &self.note) } else { paint_note(&mut c, &mut self.text, w, &self.note) };
        }
        if !self.paused {
            let warning = self.save_trouble.map(|trouble| trouble.line())
                .or(self.moment_failed.then_some(pocket_ui::save_trouble::MOMENT_FAILED));
            if let Some(line) = warning {
                // A game from the Handheld view: pocket-core's place for it, the pill at the bottom, while no note has its
                // time there (Linux's `overlay`: the toast first, else the save warning).
                if !self.list_menu {
                    paint_note(&mut c, &mut self.text, w, &[line.to_string()]);
                } else if self.note.is_empty() {
                    paint_note_pill(&mut c, &mut self.text, &[line.to_string()]);
                }
            }
        }
        // An achievement unlock: its card at the top, under the note when there is one.
        self.card_label.clear();
        if !self.paused {
            if let Some(ra) = &mut self.ra {
                if let Some(card) = ra.card(Instant::now()) {
                    let top = if self.note.is_empty() || self.list_menu { 14 } else { self.note_area.h + 12 };
                    self.card_area = player_achievements::Achievements::paint_card(&mut c, &mut self.text, card, w, top);
                    self.card_label = format!("{}. {}", pocket_ui::i18n::tr(&pocket_ui::unlock_card::head(card.points)), card.title);
                }
            }
        }
        if let Some(list) = list {
            // The handhelds' menu over the paused game, darkened as Linux's (`paint_game_behind`, 225).
            c.clear(pal().void);
            if let Some((image, rect)) = self.frame.as_deref().zip(game_rect) {
                c.blit_scaled(image, rect.x, rect.y, rect.w, rect.h);
                c.shade_rect(0, 0, w, h, pal().void, 225);
            }
            self.hits.clear();
            self.pad_places.clear();
            self.option_arrows.clear();
            let mut hits = pocket_ui::touch::HitMap::default();
            match list {
                ListPaint::Main { rows, sel, window, note, hints } => {
                    let filled: Vec<bool> = self.filled_slots.to_vec();
                    let ring = (self.slot < filled.len()).then_some(self.slot);
                    let strip_h = |_: i32| 28;
                    let page = pocket_ui::pause_page::MainPage {
                        title: &self.title,
                        battery: None,
                        // Saves that don't reach the card: Linux's line under the title (`save_trouble`).
                        trouble: self.save_trouble.map(|t| t.line()),
                        rows: &rows,
                        sel,
                        window: window.as_ref().map(|w| pocket_ui::pause_page::Window { picture: w.picture.as_deref(), empty: w.empty, note: w.note.clone(), slots: w.slots && filled.len() > 1, strip_h: &strip_h }),
                        quitting: None,
                        note,
                        hints: &hints,
                    };
                    let out = pocket_ui::pause_page::paint_main(&mut c, &mut self.text, &page);
                    // The slim row of slots under the picture (`save_media::paint_row`), the console's picture in each.
                    if let Some(at) = out.slots {
                        let icon = self.slot_icon.as_ref().map_or(pocket_ui::save_media::Icon::None, pocket_ui::save_media::Icon::Console);
                        pocket_ui::save_media::paint_row(&mut c, pocket_ui::save_media::Kind::of(&self.system), at.row.0, at.row.1, pause_list::ROW_BLOCK, pause_list::ROW_GAP, &filled, ring, icon);
                        // Save's picture on its way into the slot (`save_fill::paint`, Linux's `paint_save_fill_over`).
                        if let Some(m) = self.saving.and_then(|s| s.moment(Instant::now()).map(|m| (s.slot, m))) {
                            let (slot, m) = m;
                            let block = pocket_ui::save_fill::row_block(at.row.0, at.row.1, pause_list::ROW_BLOCK, pause_list::ROW_GAP, filled.len(), slot);
                            let fallback = Image { w: 2, h: 2, px: vec![pocket_ui::bezel::system_colour(&self.system); 4] };
                            let picture = window.as_ref().and_then(|w| w.picture.as_deref());
                            let (picture, from) = match (picture, out.picture) {
                                (Some(p), Some(from)) => (p, from),
                                (p, _) => (p.unwrap_or(&fallback), (block.0, block.1, block.2, block.2)),
                            };
                            pocket_ui::save_fill::paint(&mut c, picture, from, block, m);
                        }
                    }
                    pocket_ui::pause_page::finish_main(&mut c, &mut self.text, &mut hits, &page, &out);
                    self.list_hits = out.rows.into_iter().enumerate().map(|(i, a)| (a, i)).collect();
                }
                ListPaint::Options { rows, sel, footer, heading, explanation, picture, counts } => {
                    let corner = player_corner();
                    let preview = picture_page.filter(|_| picture).map(|(rows, layout)| pocket_ui::pause_page::PicturePreview { rows, layout, live: &self.live });
                    let page = pocket_ui::pause_page::OptionsPage {
                        heading: Some(&heading),
                        counts: counts.as_deref(),
                        rows: &rows,
                        sel,
                        footer: footer.as_deref(),
                        explanation,
                        accent: pocket_ui::bezel::system_colour(&self.system),
                        corner: &corner,
                        picture: preview,
                    };
                    let out = pocket_ui::pause_page::paint_options(&mut c, &mut self.text, &mut hits, &page);
                    self.list_hits = out.rows.into_iter().enumerate().filter_map(|(i, a)| a.map(|a| (a, i))).collect();
                }
                ListPaint::Zoom => {
                    self.list_hits.clear();
                    match (&self.list_zoom, &self.zoom_shot) {
                        (Some(zoom), Some((shot, rect))) => pocket_ui::pause_page::paint_zoom(&mut c, &mut self.text, &mut hits, shot, zoom, Some(*rect)),
                        _ => c.clear(0),
                    }
                }
            }
            // The hint keys as painted: a finger on "B Back" goes back, on "A OK" does the row.
            self.list_hint_hits = hits.targets().iter().filter_map(|(a, t)| match t { pocket_ui::touch::Target::Button(b) => Some((*a, *b)), _ => None }).collect();
        } else if self.paused {
            // A flat sheet for touch and controller alike (a controller gets a focus ring and a hint line): the
            // paused picture is only a thumbnail (portrait), never behind the buttons.
            c.clear(pal().void);
            let icon = self
                .slot_icon
                .as_ref()
                .map_or(pocket_ui::save_media::Icon::None, pocket_ui::save_media::Icon::Console);
            let view = TouchView {
                size: (w, h),
                title: &self.title,
                message: &self.message,
                rows: &rows,
                menu: self.menu,
                slot: self.slot,
                place_name: &selected_place_name,
                filled: &self.filled_slots,
                kind: pocket_ui::save_media::Kind::of(&self.system),
                system: &self.system,
                icon,
                options: &display_options,
                scroll: self.scroll,
                frame: self.frame.as_deref().zip(game_rect).map(|(image, rect)| (image, rect.w as f64 / rect.h.max(1) as f64)),
                focus: (!self.touch_visible).then_some(self.row),
                grey: [pocket_ui::slots::load_refusal(&self.filled_slots, self.slot).is_some() || hardcore, !self.earlier_row.entries.iter().any(|e| !e.damaged()) || hardcore],
                advanced_open: self.advanced_open,
                card: false,
            };
            let out = if matches!(self.menu, Menu::Controls | Menu::Recovery | Menu::Achievements) {
                let (back,max) = if self.menu == Menu::Achievements {
                    play_cards::paint_lines(&mut c, &mut self.text, &pocket_ui::i18n::tr("Achievements"), &self.ra_lines, self.scroll.max(0) as usize)
                } else if self.menu == Menu::Recovery {
                    let lines:Vec<_> = self.note.iter().map(|l|pocket_ui::i18n::tr(l)).collect();
                    play_cards::paint_lines(&mut c,&mut self.text,"Recovery",&lines,self.scroll.max(0) as usize)
                } else {play_cards::paint_controls(&mut c,&mut self.text,&self.system,&self.title,&self.control_names,self.touch_visible,self.scroll.max(0) as usize)};
                TouchOut {hits:vec![(back,0,pocket_ui::i18n::tr("Back"))],scroll:self.scroll.min(max as i32),scroll_max:max as i32,..Default::default()}
            } else if self.menu == Menu::Zoom {
                let back=Area::new(40,h-120,w-80,TARGET_MIN);
                if let Some(image)=&self.frame {self.zoom.paint(&mut c,image,Area::new(0,0,w,h-150));}
                pill(&mut c,&mut self.text,back,"Back",false);
                let label=format!("{} · {:.1}x",pocket_ui::i18n::tr("Zoom"),self.zoom.scale);
                play_cards::paint_badge(&mut c,&mut self.text,&label);
                TouchOut {hits:vec![(back,0,pocket_ui::i18n::tr("Back"))],..Default::default()}
            } else {touch_pause(&mut c, &mut self.text, &view)};
            self.hits = out.hits;
            self.pad_places = out.places;
            self.option_arrows = out.option_arrows;
            self.scroll = out.scroll;
            self.scroll_max = out.scroll_max;
            if let [Some(a), Some(b)] = out.slot_arrows {
                self.slot_arrows = [a, b];
            }
        }
        if !self.paused && self.reminder.is_none() && self.picture.over_game && !self.fps.line().is_empty() {
            // Over the game → FPS, in the top left corner as on Linux.
            let line = self.fps.line().to_string();
            play_cards::paint_badge(&mut c, &mut self.text, &line);
        }
        if !self.paused && self.reminder.is_none() && !self.speed.badge.is_empty() {
            // Fast-forward or rewind: pocket-core's badge, in its pill, in the top right corner.
            paint_pill_at(&mut c, &mut self.text, &self.speed.badge, |w, _, pw, _| (w - pocket_ui::play_speed::BADGE_MARGIN - pw, pocket_ui::play_speed::BADGE_MARGIN));
        }
        if let Some(card)=&self.reminder {pocket_ui::where_was_i::paint(&mut c,&mut self.text,card,h-40,255);}
        if let Some(held)=held_label {
            let label = self.text.translated("{s} held").replace("{s}", &held);
            play_cards::paint_badge(&mut c,&mut self.text,&label);
        }
        self.core.frontend_current();
        self.gfx.backdrop = if self.paused { pal().void } else { 0 };
        self.gfx.present(&self.pixels, (w, h), self.view, game, true, &crt, frame_plan.as_ref(), (pic_size, pic_view));
        self.overlay_size = if self.paused { None } else { Some(self.logical) };
    }
}
/// The list menu's page as gathered for one paint (`Player::list_paint`), painted by Linux's painter (`pause_page`).
enum ListPaint {
    Main { rows: Vec<pocket_ui::pause_page::MainRow>, sel: usize, window: Option<ListWindow>, note: Option<String>, hints: Vec<(&'static str, &'static str)> },
    Options { rows: Vec<pocket_ui::pause_page::OptRow>, sel: usize, footer: Option<String>, heading: String, explanation: bool, picture: bool, counts: Option<String> },
    /// Options → Zoom (`open_list_zoom`).
    Zoom,
}
/// The window at the right of the main page while Save, Load or Earlier is selected.
struct ListWindow {
    picture: Option<std::sync::Arc<Image>>,
    empty: &'static str,
    note: String,
    /// The slots stand under the picture (Save and Load).
    slots: bool,
}
/// How long the lines about a start stay over the game.
const NOTE_FOR: Duration = Duration::from_secs(8);
/// How long Linux's messages over the game stay (pocket-core's `overlay` notes): a game from the Handheld view's.
const LINUX_NOTE_FOR: Duration = Duration::from_secs(6);
const KEPT_ONLY_LOADS: &str = "A kept moment can only be loaded: pick a slot to save";
/// What the slot picker calls a place: a slot, or one of the kept moments after them.
/// The status corner of the pause menu's pages (Linux's pill at the top right): the time as the system shows it, and the
/// battery where the app may read it.
fn player_corner() -> pocket_ui::status_corner::Corner {
    let clock = pocket_ui::status_corner::clock_from_libc();
    let file = |name: &str| std::fs::read_to_string(format!("/sys/class/power_supply/battery/{name}")).ok();
    let battery = pocket_ui::status_corner::parse_battery(file("capacity").as_deref(), file("status").as_deref()).map(|b| b.label());
    pocket_ui::status_corner::Corner { clock, wifi: false, battery }
}
fn place_name(place: usize, slots: usize) -> String {
    match place.checked_sub(slots) {
        Some(kept) => format!("Kept moment {}", kept + 1),
        None => format!("Slot {}", place + 1),
    }
}
fn note_text(line: &str) -> String {
    pocket_ui::i18n::tr(line)
}
/// The lines about the game's start (and what a save or a load did) in pocket-core's dark pill at the bottom centre of
/// the screen, as Linux shows its messages over the game (`pause_page::note_pill`, `NOTE_ALPHA` see-through through the
/// overlay's alpha). Returns what it covers.
fn paint_note_pill(c: &mut Canvas, text: &mut Text, lines: &[String]) -> Area {
    let msg = lines.iter().map(|l| note_text(l)).collect::<Vec<_>>().join(" ");
    paint_pill_at(c, text, &msg, |w, h, pw, ph| ((w - pw) / 2, h - pocket_ui::pause_page::NOTE_MARGIN - ph))
}
/// `msg` in pocket-core's pill (`pause_page::note_pill`), placed by `at` (canvas width and height, pill width and height →
/// its top left). Returns what it covers.
fn paint_pill_at(c: &mut Canvas, text: &mut Text, msg: &str, at: impl Fn(i32, i32, i32, i32) -> (i32, i32)) -> Area {
    use pocket_ui::pause_page::{note_pill, NOTE_ALPHA, NOTE_KEY};
    let (w, h) = (c.w, c.h);
    let (pw, ph, px) = note_pill(text, msg, w as u32);
    let (x, y) = at(w, h, pw, ph);
    let alpha = ((NOTE_ALPHA * 255.0) as u32) << 24;
    let pixels = c.pixels_mut();
    for row in 0..ph {
        for col in 0..pw {
            let p = px[(row * pw + col) as usize];
            let (sx, sy) = (x + col, y + row);
            if p != NOTE_KEY && (0..w).contains(&sx) && (0..h).contains(&sy) {
                pixels[(sy * w + sx) as usize] = alpha | (p & 0xff_ffff);
            }
        }
    }
    Area::new(x, y, pw, ph)
}
/// A band at the top of the game with the lines about its start; the band is what it covers. Colours are the band's own
/// over any game: the overlay's black is see-through.
fn paint_note(c: &mut Canvas, text: &mut Text, w: i32, lines: &[String]) -> Area {
    let wrapped: Vec<String> = lines.iter().flat_map(|l| text.wrap_at_most(Face::Body, 34, &note_text(l), w - 2 * EDGE, 3)).collect();
    let step = type_scale::row(44, 34);
    let area = Area::new(0, 0, w, wrapped.len() as i32 * step + 2 * GAP);
    c.rect(area.x, area.y, area.w, area.h, 0x241f1a);
    for (i, line) in wrapped.iter().enumerate() {
        text.draw(c, Face::Body, 34, EDGE, GAP + (i as i32 + 1) * step - 12, 0xf4efe6, line);
    }
    area
}
// Touch pause menu. Sizes are logical pixels (about 2.2 per dp): every target is at least 106 (48 dp) high with
// gaps of 20 (9 dp); the primary button is 128 (58 dp). The sizes and the head are in `pause_head`.
struct TouchView<'a> {
    size: (i32, i32),
    title: &'a str,
    message: &'a str,
    rows: &'a [String],
    menu: Menu,
    slot: usize,
    place_name: &'a str,
    filled: &'a [bool; 8],
    kind: pocket_ui::save_media::Kind,
    system: &'a str,
    icon: pocket_ui::save_media::Icon<'a>,
    options: &'a [FileOption],
    scroll: i32,
    // The paused picture and its display aspect: a thumbnail in the room a portrait phone leaves above the buttons.
    frame: Option<(&'a Image, f64)>,
    // The controller's row (a ring round it and a hint line at the bottom); None under a finger.
    focus: Option<usize>,
    // The rows that can't be chosen now (Load on an empty slot, Earlier before the first): grey, as on Linux.
    grey: [bool; 2],
    advanced_open: bool,
    // Cozy extra "Memory card in the pause menu": the whole card or cartridge under the slot name.
    card: bool,
}
#[derive(Default)]
struct TouchOut {
    hits: Vec<(Area, usize, String)>,
    slot_arrows: [Option<Area>; 2],
    /// The pause sheet's buttons and slot (`pause_head::places`); empty on every other page.
    places: Vec<(usize, Area)>,
    option_arrows: Vec<(Area, usize, i32, String)>,
    scroll: i32,
    scroll_max: i32,
}
fn centered(c: &mut Canvas, text: &mut Text, face: Face, px: u32, area: Area, color: u32, label: &str) {
    let (px, label) = pause_head::button_words(text, face, px, label, area.w - 24);
    let width = text.width(face, px, &label);
    text.draw(c, face, px, area.x + (area.w - width) / 2, area.y + area.h / 2 + type_scale::mid(px), color, &label);
}
fn chevron(c: &mut Canvas, cx: i32, cy: i32, right: bool, color: u32) {
    let (reach, thick) = (18, 7);
    for i in 0..=reach {
        let x = if right { cx - reach / 2 + i } else { cx + reach / 2 - i - thick + 1 };
        c.rect(x, cy - reach + i, thick, 1, color);
        c.rect(x, cy + reach - i, thick, 1, color);
    }
}
fn pill(c: &mut Canvas, text: &mut Text, area: Area, label: &str, primary: bool) {
    pill_in(c, text, area, label, primary, false)
}
/// A pill whose `grey` label says it can't be chosen now.
fn pill_in(c: &mut Canvas, text: &mut Text, area: Area, label: &str, primary: bool, grey: bool) {
    // The primary button is the paper selection of the design language; on a light page paper would vanish, so ink.
    let (fill, on_fill) = if pal().is_light() { (pal().ink, pal().select) } else { (pal().select, pal().ink) };
    let bg = if primary { fill } else { pal().pill };
    if pocket_ui::retro::on() { pocket_ui::retro::frame(c, area.x, area.y, area.w, area.h, bg); }
    else { c.round_rect(area.x, area.y, area.w, area.h, area.h / 2, bg); }
    if primary {
        centered(c, text, Face::Title, 52, area, on_fill, label);
    } else {
        centered(c, text, Face::Body, 38, area, if grey { pal().dust } else { pal().text }, label);
    }
}
/// The key of the Options page's Disc row (`Player::display_options`).
const DISC_ROW: &str = "disc";
/// The options page's sections, in page order: the player's own picture settings first, then the core's.
const SECTIONS: [&str; 6] = ["Picture", "Sound", "Controls", "Speed", "Cheats", "Advanced (this emulator)"];
/// The section that starts closed: what is particular to one emulator (its model, BIOS, presets, restarts).
const ADVANCED: usize = 5;
/// The Controls and Cheats sections: the list menu's Options has a page for each (`pause_options`).
const CONTROLS: usize = 2;
const CHEATS: usize = 4;
/// Which section of `SECTIONS` option `row` of `display_options` goes in, by its key and name.
fn option_section(option: &FileOption, row: usize) -> usize {
    if row < picture::ROWS {
        return 0;
    }
    if option.key == "hold_toggle" || option.key == DISC_ROW {return 2;}
    if option.key.starts_with("cheat.") {return 4;}
    pocket_ui::core_options::player_section(&option.key, &option.label, option_name(&option.label).1)
}
/// The options' rows in page order: by section, each section in the core's own order.
fn option_order(options: &[FileOption]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..options.len()).collect();
    order.sort_by_key(|&i| option_section(&options[i], i));
    order
}
/// What a controller steps through on the options page: the clear sections, then the "Advanced (this emulator)"
/// row (`options.len() + 1`), then its options when it is open. Back (`options.len()`) comes after.
fn option_page(options: &[FileOption], open: bool) -> Vec<usize> {
    let order = option_order(options);
    let (clear, advanced): (Vec<usize>, Vec<usize>) = order.into_iter().partition(|&i| option_section(&options[i], i) != ADVANCED);
    let mut page = clear;
    if !advanced.is_empty() {
        page.push(options.len() + 1);
        if open {
            page.extend(advanced);
        }
    }
    page
}
/// A core option's name without the handheld's "(Restart)" note, and whether it only works after a restart.
fn option_name(label: &str) -> (String, bool) {
    for mark in [" (Restart)", "(Restart)", " (restart)", " (Requires Restart)", " (requires restart)"] {
        if let Some(name) = label.strip_suffix(mark).or_else(|| label.find(mark).map(|at| &label[..at])) {
            return (name.trim().to_string(), true);
        }
    }
    (label.to_string(), false)
}
/// A value as a player reads it: On/Off instead of enabled/disabled, a capital first letter.
fn option_value(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "enabled" | "on" | "true" => "On".into(),
        "disabled" | "off" | "false" => "Off".into(),
        _ => {
            let mut chars = value.chars();
            chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
        }
    }
}
/// The controller's focus: a ring round `area` in the text colour.
fn ring(c: &mut Canvas, area: Area) {
    let grow = 7;
    c.round_rect(area.x - grow, area.y - grow, area.w + 2 * grow, area.h + 2 * grow, (area.h + 2 * grow) / 2, pal().text);
    c.round_rect(area.x - 2, area.y - 2, area.w + 4, area.h + 4, (area.h + 4) / 2, pal().void);
}
fn touch_pause(c: &mut Canvas, text: &mut Text, v: &TouchView) -> TouchOut {
    let (w, h) = v.size;
    let portrait = pause_head::portrait(w, h, v.card);
    let mut out = TouchOut::default();
    let inner = w - 2 * EDGE;
    // A controller's keys go in a line at the bottom; a finger needs none.
    let hint_h = if v.focus.is_some() { 56 } else { 0 };
    let bottom = h - 36 - hint_h;
    // Options repaints this same heading after clipping its scrolling rows below.
    let title = if v.menu == Menu::Options { text.fit(Face::Title, 46, "Options", if portrait { inner } else { inner - 420 }) } else { String::new() };
    let mut top = 170;
    if v.menu == Menu::Options {
        text.draw(c, Face::Title, 46, EDGE, 92, pal().chalk, &title);
    } else {
        // The game's whole name (two lines when it needs them, never translated) and the status line under it; the
        // rows start under both and keep their height.
        let head = pause_head::layout(text, v.title, v.message, inner, bottom, pause_head::rows_need(portrait, v.card));
        pause_head::paint(c, text, &head, EDGE, pal().chalk, pal().dust);
        top = head.bottom;
    }
    if v.focus.is_some() {
        let hint = if v.menu == Menu::Options { "A Choose · ‹ › Change · B Back" } else { "A Choose · B Continue · L / R Slot" };
        let hint = text.fit(Face::Body, 28, hint, inner);
        text.draw(c, Face::Body, 28, EDGE, h - 40, pal().dust, &hint);
    }
    if v.menu == Menu::Options {
        let tall = ((bottom - top - GAP - 100) / 6).clamp(TARGET_MIN, 124);
        // Back: the bottom of a portrait page, the top right of a landscape one (rows keep the full height).
        let back = if portrait {
            Area::new(EDGE, bottom - tall, inner, tall)
        } else {
            Area::new(w - EDGE - 400, 28, 400, tall.min(112))
        };
        let cols = if portrait { 1 } else { 2 };
        let (win_top, win_bottom) = (if portrait { top } else { back.y + back.h + GAP }, if portrait { back.y - GAP } else { bottom });
        let col_w = (inner - (cols - 1) * GAP) / cols;
        let (zone, row_h, head_h) = (116, 128.max(type_scale::px(26) as i32+type_scale::px(36) as i32+44), type_scale::row(76,40));
        // The page: a heading per section ("Picture", "Sound", "Controls", "Speed", "Game"), its rows under it.
        let order = option_order(v.options);
        let mut places: Vec<(usize, i32, i32)> = Vec::new();
        let mut heads: Vec<(&str, i32)> = Vec::new();
        let mut toggle: Option<(i32, usize)> = None;
        let mut y = 0;
        for (s, name) in SECTIONS.iter().enumerate() {
            let rows: Vec<usize> = order.iter().copied().filter(|&i| option_section(&v.options[i], i) == s).collect();
            if rows.is_empty() {
                continue;
            }
            if s == ADVANCED {
                // A row of its own that opens the group: "Advanced (this emulator) · 6".
                toggle = Some((y + 8, rows.len()));
                places.push((v.options.len() + 1, 0, y + 8));
                y += 8 + 104 + GAP;
                if !v.advanced_open {
                    continue;
                }
            } else {
                heads.push((name, y));
                y += head_h;
            }
            for (n, i) in rows.iter().enumerate() {
                places.push((*i, n as i32 % cols, y + n as i32 / cols * (row_h + GAP)));
            }
            y += (rows.len() as i32 + cols - 1) / cols * (row_h + GAP) + 8;
        }
        let content = y;
        out.scroll_max = (content - (win_bottom - win_top)).max(0);
        let mut scroll = v.scroll;
        if let Some((_, _, fy)) = v.focus.and_then(|f| places.iter().find(|(i, _, _)| *i == f)) {
            // The controller's row stays in view (its heading too when it is the first of a section).
            if fy - head_h < scroll {
                scroll = fy - head_h;
            } else if fy + row_h > scroll + (win_bottom - win_top) {
                scroll = fy + row_h - (win_bottom - win_top);
            }
        }
        out.scroll = scroll.clamp(0, out.scroll_max);
        let visible = |a: Area| {
            let (y0, y1) = (a.y.max(win_top), (a.y + a.h).min(win_bottom));
            (y1 > y0).then(|| Area::new(a.x, y0, a.w, y1 - y0))
        };
        for (name, hy) in &heads {
            let base = win_top + hy - out.scroll + 52;
            if base > win_top + 30 && base < win_bottom {
                text.draw(c, Face::Title, 40, EDGE + 8, base, pal().chalk, name);
            }
        }
        if let Some((ty, count)) = toggle {
            let area = Area::new(EDGE, win_top + ty - out.scroll, inner, 104);
            if area.y + area.h >= win_top && area.y <= win_bottom {
                if v.focus == Some(v.options.len() + 1) {
                    ring(c, area);
                }
                if pocket_ui::retro::on() { pocket_ui::retro::frame(c, area.x, area.y, area.w, area.h, pal().spine); }
                else { c.round_rect(area.x, area.y, area.w, area.h, area.h / 2, pal().spine); }
                let name = text.translated("Advanced (this emulator)").to_string();
                let label = format!("{name} · {count}");
                text.draw(c, Face::Title, 38, area.x + 48, area.y + area.h / 2 + type_scale::mid(38), pal().chalk, &label);
                // A drawn chevron: down while closed, up while open.
                let (cx, cy) = (area.x + area.w - 70, area.y + area.h / 2);
                for i in 0..=16 {
                    let dy = if v.advanced_open { 8 - i / 2 } else { -8 + i / 2 };
                    c.rect(cx - 16 + i, cy + dy - 3, 3, 6, pal().text);
                    c.rect(cx + 16 - i - 2, cy + dy - 3, 3, 6, pal().text);
                }
                if let Some(a) = visible(area) {
                    out.hits.push((a, v.options.len() + 1, text.translated("Advanced (this emulator)").to_string()));
                }
            }
        }
        for &(i, col, py) in &places {
            if i >= v.options.len() {
                continue;
            }
            let option = &v.options[i];
            let area = Area::new(EDGE + col * (col_w + GAP), win_top + py - out.scroll, col_w, row_h);
            if area.y + area.h < win_top || area.y > win_bottom {
                continue;
            }
            if v.focus == Some(i) {
                ring(c, area);
            }
            if pocket_ui::retro::on() { pocket_ui::retro::frame(c, area.x, area.y, area.w, area.h, pal().pill); }
            else { c.round_rect(area.x, area.y, area.w, area.h, row_h / 2, pal().pill); }
            let many = option.values.len() > 1;
            let arrow = if many { pal().text } else { pal().off };
            chevron(c, area.x + zone / 2 + 6, area.y + row_h / 2, false, arrow);
            chevron(c, area.x + area.w - zone / 2 - 6, area.y + row_h / 2, true, arrow);
            let middle = Area::new(area.x + zone, area.y, area.w - 2 * zone, row_h);
            let (name, restart) = option_name(&option.label);
            let name = if restart {
                let after = text.translated("after a restart").to_string();
                format!("{name} · {after}")
            } else {
                name
            };
            let label = text.fit(Face::Body, 26, &name, middle.w);
            let width = text.width(Face::Body, 26, &label);
            text.draw(c, Face::Body, 26, middle.x + (middle.w - width) / 2, area.y + type_scale::px(26) as i32 + 16, pal().soft, &label);
            let value = text.fit(Face::Body, 36, &option_value(option.label_of(&option.current)), middle.w);
            let width = text.width(Face::Body, 36, &value);
            text.draw(c, Face::Body, 36, middle.x + (middle.w - width) / 2, area.y + type_scale::px(26) as i32 + type_scale::px(36) as i32 + 28, pal().text, &value);
            // Taps only count inside the visible window, not under the header or the Back button.
            if many {
                if let Some(a) = visible(Area::new(area.x, area.y, zone, row_h)) {
                    out.option_arrows.push((a, i, -1, format!("{} · previous value", option.label)));
                }
                if let Some(a) = visible(Area::new(area.x + area.w - zone, area.y, zone, row_h)) {
                    out.option_arrows.push((a, i, 1, format!("{} · next value", option.label)));
                }
            }
            if let Some(a) = visible(middle) {
                out.hits.push((a, i, v.rows[i].clone()));
            }
        }
        // Rows slide under the header and the Back button: cover them with the page.
        c.rect(0, 0, w, win_top, pal().void);
        c.rect(0, win_bottom, w, (if portrait { back.y } else { bottom }) - win_bottom, pal().void);
        if portrait {
            c.rect(0, back.y - GAP, w, back.h + GAP, pal().void);
        }
        text.draw(c, Face::Title, 46, EDGE, 92, pal().chalk, &title);
        if v.focus == Some(v.options.len()) {
            ring(c, back);
        }
        pill(c, text, back, "Back", false);
        out.hits.push((back, v.options.len(), text.translated("Back").to_string()));
        let window = win_bottom - win_top;
        if content > window {
            let bar = (window * window / content).max(40);
            let at = win_top + (window - bar) * out.scroll / out.scroll_max.max(1);
            c.round_rect(w - 14, at, 6, bar, 3, pal().chip);
        }
        return out;
    }
    // With the whole memory card the slot picker is taller, to show the medium.
    let labels = ["Continue", "Save", "Load", "Earlier", "Options", "Screenshot", "Controls", "Zoom", "Save & quit", "Achievements"];
    // A game with an account signed in has one more row (Achievements, last); everyone else sees the layout as before.
    let picker = |c: &mut Canvas, text: &mut Text, out: &mut TouchOut, area: Area| {
        if v.focus == Some(pause_head::PICKER) {
            ring(c, area);
        }
        if pocket_ui::retro::on() { pocket_ui::retro::frame(c, area.x, area.y, area.w, area.h, pal().pill); }
        else { c.round_rect(area.x, area.y, area.w, area.h, 28, pal().pill); }
        let zone = 120.min(area.w / 4);
        let (left, right) = (Area::new(area.x, area.y, zone, area.h), Area::new(area.x + area.w - zone, area.y, zone, area.h));
        chevron(c, left.x + zone / 2 + 6, area.y + area.h / 2, false, pal().text);
        chevron(c, right.x + zone / 2 - 6, area.y + area.h / 2, true, pal().text);
        out.slot_arrows = [Some(left), Some(right)];
        let middle = Area::new(area.x + zone, area.y, area.w - 2 * zone, area.h);
        let name = v.place_name;
        centered(c, text, Face::Body, 36, Area::new(middle.x, area.y + 6, middle.w, 62), pal().text, &name);
        if v.card {
            // The medium with its eight slots (as the info page shows it), in the room under the name.
            let room = Area::new(middle.x, area.y + 70, middle.w, area.h - 70 - 14);
            let l = pocket_ui::save_media::Layout::fitted(v.kind, v.filled.len(), room.w, room.h);
            let slots = pocket_ui::save_media::Slots {
                filled: v.filled,
                ring: Some(v.slot),
                icon: v.icon,
                label: None,
                stripe: pocket_ui::document_library::system(v.system).map_or(pal().chip, |s| s.3),
                numbers: true,
            };
            pocket_ui::save_media::paint(c, text, v.kind, room.x + (room.w - l.w) / 2, room.y + (room.h - l.h) / 2, &l, &slots);
        } else {
            let b = ((middle.w - 7 * 14) / 8).clamp(20, 44);
            pocket_ui::save_media::paint_row(c, v.kind, middle.x + middle.w / 2, area.y + area.h - 46, b, 14, v.filled, Some(v.slot), v.icon);
        }
    };
    let button = |c: &mut Canvas, text: &mut Text, out: &mut TouchOut, area: Area, row: usize| {
        if v.focus == Some(row) {
            ring(c, area);
        }
        let grey = matches!(row, 2 | 3) && v.grey[row - 2];
        pill_in(c, text, area, if row == 8 { &v.rows[row] } else { labels[row] }, row == 0, grey);
        // The accessibility label follows the interface language, like the painted one.
        out.hits.push((area, row, text.translated(&v.rows[row]).to_string()));
    };
    // The places come from `pause_head::places`, which the D-pad walks too (its tests check every Android screen).
    let places = pause_head::places(w, h, top, bottom, v.card, v.rows.len() > 9);
    let at = |row: usize| places.iter().find(|(r, _)| *r == row).map(|(_, a)| *a);
    if let (true, Some((image, aspect)), Some(first)) = (portrait, v.frame, at(0)) {
        // The paused picture in the room a portrait phone leaves above the buttons.
        let (room_top, room_bottom) = (top + 20, first.y - 2 * GAP);
        let ih = ((room_bottom - room_top) as f64).min(inner as f64 / aspect);
        if ih >= 200.0 {
            let iw = (ih * aspect) as i32;
            let ih = ih as i32;
            c.blit_scaled(image, (w - iw) / 2, room_top + (room_bottom - room_top - ih) / 2, iw, ih);
        }
    }
    for &(row, area) in &places {
        if row == pause_head::PICKER {
            picker(c, text, &mut out, area);
        } else {
            button(c, text, &mut out, area, row);
        }
    }
    out.places = places;
    out
}
fn display_refresh_hz() -> f64 {
    pocket_ui::platform::android::string("displayRefreshRate").parse::<f64>().ok()
        .filter(|hz| hz.is_finite() && (10.0..=480.0).contains(hz)).unwrap_or(60.0)
}
/// An error as the player's screen shows it: our sentence as it is, anything else (an OS error, a core's or a library's
/// words) as `instead`, the raw text in the log (`save_trouble::shown`). Every error that reaches `message` or the
/// start dialog goes through here (`save_trouble::tests`).
fn said(error: String, instead: &'static str) -> String {
    let shown = pocket_ui::save_trouble::shown(&error, instead);
    if shown != error { log::warn!("player: {instead} ({error})"); }
    shown
}

fn string(env: &mut JNIEnv, value: JString) -> String {
    env.get_string(&value).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}
/// Native Android dialogs use the same tables and language as the player, initialized by nativeCreate before its
/// error callback. The Java UI thread follows the process language, as the renderer's GL thread does.
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeTranslate(mut env: JNIEnv, _: JObject, value: JString) -> jstring {
    let text = pocket_ui::i18n::tr(&string(&mut env, value));
    env.new_string(text).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeCreate(
    mut env: JNIEnv,
    activity: JObject,
    core: JString,
    rom: JString,
    system: JString,
    key: JString,
    title: JString,
) -> jstring {
    android_logger::init_once(android_logger::Config::default().with_tag("PocketMvp").with_max_level(log::LevelFilter::Info));
    let initialized = env
        .get_java_vm()
        .map_err(|e| e.to_string())
        .and_then(|vm| unsafe { pocket_ui::platform::android::initialize(vm.get_java_vm_pointer(), activity.as_raw()) });
    if let Err(error) = initialized {
        return env.new_string(error).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut());
    }
    // The launch carries the current preferences, so a fast launch cannot race the async disk writer.
    let prefs: serde_json::Value = serde_json::from_str(&pocket_ui::platform::android::string("playerSettings"))
        .unwrap_or_else(|_| pocket_ui::fsutil::read_json_or_backup(&pocket_ui::platform::data_dir().join("launcher.json")).unwrap_or_default());
    // As `Preferences::chosen_language`: the choice, or what an older version wrote, or the phone's.
    let language = prefs["language"].as_str().map(pocket_ui::i18n::Lang::from_locale).or(prefs["german"].as_bool().map(|g| if g { pocket_ui::i18n::Lang::De } else { pocket_ui::i18n::Lang::En })).unwrap_or_else(|| pocket_ui::i18n::Lang::from_locale(&pocket_ui::platform::android::string("language")));
    pocket_ui::i18n::set(language);
    let font_scale = pocket_ui::platform::android::string("fontScale").parse::<f32>().unwrap_or(1.0);
    pocket_ui::type_scale::set_system_scale(font_scale);
    let text_size = prefs["text_size"].as_str().unwrap_or("normal");
    pocket_ui::type_scale::set(if font_scale >= 1.25 || text_size == "large" {
        pocket_ui::text_size::TextSize::Large
    } else if font_scale >= 1.12 || text_size == "medium" {
        pocket_ui::text_size::TextSize::Medium
    } else {
        pocket_ui::text_size::TextSize::Normal
    });
    // The launcher's look settings (colours, font, hint symbols, motion, Cozy extras) apply here too.
    macro_rules! pref {
        ($ty:ty, $key:expr) => {
            serde_json::from_value::<$ty>(prefs[$key].clone()).unwrap_or_default()
        };
    }
    let extras = pref!(pocket_ui::cozy_extras::Extras, "extras");
    pocket_ui::palette::set_colours(pref!(pocket_ui::palette::Colours, "colours"));
    pocket_ui::font_style::set(pref!(pocket_ui::font_style::FontStyle, "font"));
    pocket_ui::retro::set_colours(pref!(pocket_ui::retro::Colours, "retro_colours"));
    pocket_ui::button_prompts::set(pref!(pocket_ui::button_prompts::PromptStyle, "prompts"));
    pocket_ui::motion::set_level(pref!(pocket_ui::motion::Motion, "motion"));
    pocket_ui::motion::set_cozy(prefs["cozy"].as_bool().unwrap_or(true));
    pocket_ui::cozy_extras::set(extras);
    let core = string(&mut env, core);
    let rom = string(&mut env, rom);
    let system = string(&mut env, system);
    let key = string(&mut env, key);
    let title = string(&mut env, title);
    let error = PLAYER.with(|slot| {
        slot.borrow_mut().take();
        let frozen = prefs["frozen_shown"].as_bool().unwrap_or(false);
        match Player::new(&core, &rom, system, key, title, prefs["light"].as_bool().unwrap_or(false), frozen) {
            Ok(mut player) => {
                // The sidecar came from this game's SAF document. No database is downloaded or shared by name.
                // Hardcore: no cheats, and no Cheats rows in Options (`achievements::hardcore`).
                if matches!(player.core.name.as_str(), "mGBA" | "Gambatte" | "Nestopia" | "bsnes") && !player.hardcore() {
                    player.cheats = pocket_ui::cheats::CheatMenu::from_text(player.data.join("android-cheats").join(format!("{}.json",player.key)), prefs["cheat_text"].as_str().unwrap_or(""));
                    if let Some(cheats) = &player.cheats { player.core.set_cheats(&cheats.codes()); }
                }
                // A game from the Handheld view: its pause menu is the handhelds' (`pause_list`).
                player.list_menu = prefs["pause_menu"].as_str() == Some("handheld");
                // Rumble as Speed, rumble, fan › Rumble says for this game (Linux's `Extras::apply_settings`).
                pocket_ui::rumble::set_strength(pocket_ui::rumble::Strength::parse(player.picture.value(pause_options::RUMBLE_KEY).unwrap_or("medium")));
                player.sounds_muted = prefs["muted"].as_bool().unwrap_or(false);
                player.haptics = prefs["haptics"].as_bool().unwrap_or(false);
                player.text_size = serde_json::from_value(prefs["text_size"].clone()).unwrap_or_default();
                // Options › Controls (Linux's page): the button choices from the game's settings (`ctl.*`).
                player.load_controls();
                if player.resumed && player.frame.is_some() {
                    let r=&prefs["reminder"];
                    if let Some(ago)=r["ago"].as_str() {
                        log::info!("Android reminder: {ago}");
                        player.reminder=Some(pocket_ui::where_was_i::Card{ago:ago.into(),played:r["played"].as_str().map(str::to_string),progress:r["progress"].as_str().map(str::to_string),art:None,note:r["note"].as_str().map(str::to_string)});
                    }
                }
                if !player.note.is_empty() && player.system == "psp" && !player.resumed {
                    player.pause(true);
                    player.menu=Menu::Recovery;
                }
                // Cozy's switch-on also over a continued moment (`cabinet::starts_cozy`).
                let cozy = pocket_ui::cabinet::starts_cozy(prefs["cozy"].as_bool().unwrap_or(true), pocket_ui::motion::Motion::Full);
                player.crt_done = player.paused || !cozy || !extras.tv_switch_on || pocket_ui::platform::android::string("batterySaver") == "true";
                player.core.frontend_current();
                *slot.borrow_mut() = Some(player);
                String::new()
            }
            // The dialog "Game couldn't start" shows our sentence, never a library's words (`said`).
            Err(e) => pocket_ui::i18n::tr(&said(e, pocket_ui::save_trouble::START_FAILED)),
        }
    });
    env.new_string(error).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeFrame(
    mut env: JNIEnv,
    activity: JObject,
    w: jint,
    h: jint,
    density: jfloat,
    font_scale: jfloat,
    left: jint,
    top: jint,
    right: jint,
    bottom: jint,
) -> jint {
    PLAYER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let result = slot.as_mut().map_or(2, |p| p.frame(w, h, density, [left, top, right, bottom], font_scale));
        if let Some(p) = slot.as_mut() {
            let nodes = p.semantic_nodes();
            if nodes != p.semantics {
                p.semantics = nodes;
                if let Ok(text) = env.new_string(&p.semantics) {
                    let object = JObject::from(text);
                    let _ = env.call_method(&activity, "publishSemantics", "(Ljava/lang/String;)V", &[jni::objects::JValue::Object(&object)]);
                }
            }
        }

        if result == 2 {
            // The game is over and its files are whole: the launcher copies them into the player's folder (never while playing).
            if let Some(player) = slot.take() {
                let key = player.key.clone();
                drop(player);
                if let Err(e) = pocket_ui::save_copy::mark_pending(&pocket_ui::platform::data_dir(), &key) {
                    log::warn!("save copy: no note for the launcher: {e}");
                }
            }
        }
        result
    })
}
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeKey(_: JNIEnv, _: JObject, key: jint, down: jboolean, controller: jboolean, device: jint) -> jboolean {
    PLAYER.with(|slot| {
        slot.borrow_mut().as_mut().is_some_and(|p| {
            if controller != 0 && down != 0 {
                p.input_mode(false);
            }
            let result = p.key_of(device, key, down != 0);
            p.core.frontend_current();
            result
        })
    }) as jboolean
}
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeAnalog(
    _: JNIEnv,
    _: JObject,
    x: jfloat,
    y: jfloat,
    rx: jfloat,
    ry: jfloat,
    hx: jfloat,
    hy: jfloat,
    lt: jfloat,
    rt: jfloat,
    device: jint,
) -> jboolean {
    PLAYER.with(|slot| {
        slot.borrow_mut().as_mut().is_some_and(|p| {
            if [x, y, rx, ry, hx, hy, lt, rt].iter().any(|v| v.abs() > 0.01) {
                p.input_mode(false);
            }
            let result = p.analog_of(device, [x, y, rx, ry], [hx, hy], [lt, rt]);
            p.core.frontend_current();
            result
        })
    }) as jboolean
}
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeTouch(_: JNIEnv, _: JObject, id: jint, x: jfloat, y: jfloat, phase: jint) -> jboolean {
    PLAYER.with(|slot| {
        slot.borrow_mut().as_mut().is_some_and(|p| {
            let result = p.touch(id, x, y, phase);
            p.core.frontend_current();
            result
        })
    }) as jboolean
}
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeSuspend(_: JNIEnv, _: JObject) {
    PLAYER.with(|slot| {
        if let Some(p) = slot.borrow_mut().as_mut() {
            if p.suspended {return;}
            p.suspended=true;
            p.pause(true);
            p.audio.take();
            if let Err(e) = p.automatic() {
                log::error!("suspend save: {e}");
                p.message = said(e, pocket_ui::save_trouble::MOMENT_FAILED);
            }
            p.core.frontend_current();
        }
    });
}

/// The gamepads now connected (one line each, `android_pads::Device::parse`): who plays which player.
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeRoster(mut env: JNIEnv, _: JObject, roster: JString) {
    let text = string(&mut env, roster);
    let devices: Vec<_> = text.lines().filter_map(pocket_ui::android_pads::Device::parse).collect();
    PLAYER.with(|slot| {
        if let Some(p) = slot.borrow_mut().as_mut() {
            let notes = p.pads.roster(devices);
            // Each player's rumble on their own controller, as Linux's `players` seats it.
            pocket_ui::rumble::set_targets(p.pads.rumble_targets());
            for note in &notes { log::info!("player pads: {note}"); }
            if let Some(note) = notes.last() { p.message = note.clone(); }
        }
    });
}
/// Input-device removal must release every source and latch before a replacement controller is used.
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeControllers(_: JNIEnv,_:JObject,connected:jboolean,interrupted:jboolean) {
    PLAYER.with(|slot|{if let Some(p)=slot.borrow_mut().as_mut(){
        if interrupted != 0 {
            p.buttons=0;p.pulses=0;p.stick_buttons=0;p.triggers=Default::default();p.pads.release_all();p.stick_direction=0;p.hat_repeat=Default::default();p.fingers.clear();p.held=Default::default();
            p.core.set_analog([0;4]);p.stylus.clear();p.stylus_finger=None;p.core.set_pointer(None);
            if !p.paused {p.pause(true);}
        }
        p.input_mode(connected==0);
        p.core.frontend_current();
    }});
}
#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeZoom(_: JNIEnv,_:JObject,factor:jfloat) {
    PLAYER.with(|slot|{if let Some(p)=slot.borrow_mut().as_mut(){
        // The handhelds' menu has Linux's Zoom page under Options (`open_list_zoom`); a pinch is the Mobile view's.
        if p.paused && !p.list_menu && p.frame.is_some() && matches!(p.menu,Menu::Pause|Menu::Zoom) {
            p.menu=Menu::Zoom;p.zoom.magnify(factor);p.zoom_fingers.clear();p.zoom_distance=None;
        }
    }});
}

#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeForeground(_: JNIEnv,_:JObject) {
    PLAYER.with(|slot|{if let Some(p)=slot.borrow_mut().as_mut(){p.suspended=false;}});
}
