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
    display_hz: f64,
    frames: u64,
    /// Fresh pictures delivered by the core, separate from emulation ticks and display presentations.
    video_frames: u64,
    diagnostic_buttons_seen: u16,
    last_flush: Instant,
    last_earlier: Instant,
    earlier: usize,
    earlier_writer: Option<pocket_ui::platform::android_earlier::Writer>,
    cheats: Option<pocket_ui::cheats::CheatMenu>,
    recovery_required: bool,
    /// A game from the Handheld view has the handhelds' pause menu (`pause_list`): the rows under each other, the
    /// save's picture beside them, Zoom and Screenshot under Options. The Mobile view keeps its touch sheet.
    list_menu: bool,
    /// The row of `list_rows` the list menu is on.
    list_sel: usize,
    /// Earlier's moments of this game, newest first (`refresh_earlier`), and the one Earlier is on.
    earlier_files: Vec<(PathBuf, u64)>,
    earlier_at: usize,
    /// The slots' and Earlier's pictures, read once while the menu is open.
    previews: HashMap<PathBuf, Option<std::sync::Arc<Image>>>,
    /// Where the list menu's rows were painted, for a finger (`pause_list::Out`).
    list_hits: Vec<(Area, usize)>,
    /// The list menu's hint keys as painted: a finger on "B Back" goes back, on "A OK" does the row.
    list_hint_hits: Vec<(Area, pocket_ui::pad::Button)>,
    /// Zoom was opened from Options (the list menu): B goes back there.
    from_options: bool,
    /// The list menu's Options page (`pause_options`, Linux's layout): None the top page, else the group it opened.
    opt_page: Option<pause_options::Entry>,
    /// The selected row of the page, an index of `list_option_rows` (never a heading), and the top page's row a group
    /// came from (B goes back onto it).
    opt_sel: usize,
    opt_top_sel: usize,
    /// Over the game → FPS (`fps_line`), counted while the game runs.
    fps: pocket_ui::fps_line::Counter,
}
/// What a row of the list menu's Options page does (`Player::list_option_rows`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum OptTarget {
    /// A row of the top page.
    Top(pause_options::Entry),
    /// One of the Picture page's rows (the player's picture lists in the Handheld view's words).
    Picture(pause_options::PictureRow),
    /// Row `n` of `display_options` (an emulator option, a control row, a cheat).
    Option(usize),
    /// "Advanced (this emulator)": opens or closes its group on the Emulator page.
    Advanced,
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
        let alternative = Path::new(core_path).file_name().and_then(|n| n.to_str()) == Some("libgambatte_libretro.so");
        let options_name = if alternative {
            format!("{system}-gambatte.json")
        } else {
            format!("{system}.json")
        };
        let options_path = data.join("core-options").join(options_name);
        let saved_options: std::collections::BTreeMap<String, String> = pocket_ui::fsutil::read_json_or_backup(&options_path).unwrap_or_default();
        let selected: Vec<_> = saved_options.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
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
        let gfx = Graphics::new()?;
        let now = Instant::now();
        let resumed_frame = if matches!(resume, pocket_ui::libretro::Resume::Resumed | pocket_ui::libretro::Resume::ResumedAgain) {
            let (frame,_,_,_) = core.run(0);
            core_hw_frame(&core).or(frame)
        } else {None};
        let frame = player_video::startup(initial_frame, resumed_frame);
        // Capture after the post-resume frame, so the held state and its preview name the same moment.
        let initial = core.save_state().filter(|b| !b.is_empty() && b.len() <= pocket_ui::state_file::MAX_STATE)
            .and_then(|bytes| match player_video::moment(&base.with_extension("earlier0"), &bytes, frame.as_deref()) {
                Ok(()) => Some(1),
                Err(error) => { log::warn!("Initial Earlier moment could not be kept: {error}"); None }
            }).unwrap_or(0);
        let filled_slots = std::array::from_fn(|i| pocket_ui::slots::state_path(&base, i).is_file());
        let kept = pocket_ui::frozen_phone::kept(&base);
        let slot_icon = pocket_ui::system_art::get(&system).map(|sprite| sprite.fitted(26, 26));
        let sounds = pocket_ui::ui_sound::Config::load(&data);
        let picture = picture::Settings::load(&data, &system, &key);
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
            display_hz,
            frames: 0,
            video_frames: 0,
            diagnostic_buttons_seen: 0,
            last_flush: now,
            last_earlier: now,
            earlier: initial,
            earlier_writer: match pocket_ui::platform::android_earlier::Writer::start() {
                Ok(writer) => Some(writer),
                Err(error) => { log::warn!("Earlier worker unavailable: {error}"); None }
            },
            cheats: None,
            list_menu: false,
            list_sel: 0,
            earlier_files: Vec::new(),
            earlier_at: 0,
            previews: HashMap::new(),
            list_hits: Vec::new(),
            list_hint_hits: Vec::new(),
            from_options: false,
            opt_page: None,
            opt_sel: 0,
            opt_top_sel: 0,
            fps: Default::default(),
            recovery_required: false,
        })
    }
    fn state(&mut self, path: &Path) -> Result<(), String> {
        if self.recovery_required { return Err(moments::RECOVERY_MESSAGE.into()); }
        let bytes = self.core.save_state().ok_or("This core could not save its state")?;
        pocket_ui::state_file::write(path, &bytes).map_err(|e| e.to_string())?;
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
        self.state(&path)?;
        pocket_ui::slots::auto_written(&path);
        pocket_ui::slots::battery_unsaved(&path, battery.is_err());
        battery.map_err(|e| e.line().to_string())
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
        let undo_path = pocket_ui::slots::undo_path(&self.base);
        let result = if self.recovery_required { moments::recover(&mut self.core, &bytes) }
        else { moments::load(&mut self.core, &bytes, |before| pocket_ui::fsutil::write_atomic(&undo_path, before).map_err(|_| pocket_ui::save_trouble::UNDO_FAILED.to_string())) };
        if let Err(error) = result {
            self.recovery_required |= error.recovery;
            if error.recovery { self.menu = Menu::Recovery; self.note = vec![error.message.clone()]; self.scroll = 0; }
            return Err(error.message);
        }
        self.recovery_required = false;
        // The loaded moment can't unlock anything by itself: achievements wait for the game to move on.
        if let Some(ra) = &mut self.ra { ra.reset(); }
        self.core.set_analog([0; 4]);
        self.stylus.clear();
        self.stylus_finger = None;
        self.core.set_pointer(None);
        if let Some(image) = pocket_ui::image::load_png(&pocket_ui::slots::preview_path(path)) { self.frame = Some(std::sync::Arc::new(image)); }
        self.held = Default::default();
        self.buttons = 0;
        self.fingers.clear();
        self.pulses = 0;
        log::info!("player state loaded: {}", path.display());
        Ok(())
    }
    /// Save & quit: the launcher's way back gets the game's last picture (the auto state's preview, just written) and
    /// where it stood on the screen (`eject::LastPicture`). The time is the quit's, for its measurement in the log.
    fn leave_last_picture(&self, saved: bool) {
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
        self.paused = on;
        self.reminder = None;
        self.held = Default::default();
        self.zoom_fingers.clear();
        self.zoom_distance = None;
        self.overlay_size = None;
        self.buttons = 0;
        self.pulses = 0;
        self.stick_buttons = 0;
        self.pads.release_all();
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
        if on {
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
        for (latch, value) in self.triggers.iter_mut().zip(triggers) {
            latch.update(value);
        }
        if axes.iter().chain(hat.iter()).any(|v| v.abs() > 0.55) {
            self.crt_done = true;
        }
        self.core.set_analog(axes.map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16));
        let x = if hat[0].abs() > 0.5 { hat[0] } else { axes[0] };
        let y = if hat[1].abs() > 0.5 { hat[1] } else { axes[1] };
        // On an analog console the stick stays analog; only its actual hat presses the D-pad.
        let (game_x,game_y) = if pocket_ui::controls::stick_dpad_default(&self.system) {(x,y)} else {(hat[0],hat[1])};
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
        if down && self.reminder.is_some() {
            self.reminder = None;
            self.pacer.restart();
            self.pacing.pause();
            self.overlay_size = None;
            return true;
        }
        if matches!(key, 4 | 82 | 110) || (self.paused && matches!(key, 97 | 111)) {
            if down {
                if self.paused && self.from_options && self.menu == Menu::Zoom {
                    // Zoom opened from Options (the list menu) goes back there, onto its row.
                    self.menu = Menu::Options;
                    self.opt_page = None;
                    self.opt_sel = self.list_option_rows().1.iter().position(|t| *t == Some(OptTarget::Top(pause_options::Entry::Zoom))).unwrap_or(0);
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
    /// What the list menu paints this frame, gathered before the canvas holds the pixels.
    fn list_paint(&mut self) -> ListPaint {
        if self.menu == Menu::Options {
            let desc = self.opt_desc();
            let (rows, _) = self.list_option_rows();
            let title = self.opt_page.map_or_else(|| "Options".to_string(), |e| e.heading());
            return ListPaint::Options { rows, sel: self.opt_sel, desc, title };
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
        let mut hints: Vec<(&'static str, &'static str)> = Vec::new();
        let (preview, note) = match item {
            Some(item @ (pause_list::Item::Save | pause_list::Item::Load)) => {
                let path = self.place_path();
                let saved = if path.is_file() { age(&path) } else { None };
                let picture = if saved.is_some() { self.preview(&path) } else { None };
                let n = self.slot + 1;
                let kept = self.slot >= self.filled_slots.len();
                let save = item == pause_list::Item::Save;
                let note = if self.hardcore() {
                    (if save { pocket_ui::achievements::hardcore::NO_SAVE } else { pocket_ui::achievements::hardcore::NO_LOAD }).to_string()
                } else if kept && save {
                    KEPT_ONLY_LOADS.to_string()
                } else if kept {
                    place_name(self.slot, self.filled_slots.len())
                } else {
                    match (save, &saved) {
                        (true, Some(t)) => format!("Slot {n} · replaces the save from {t}"),
                        (true, None) => format!("Slot {n} · empty"),
                        (false, Some(t)) => format!("Slot {n} · saved {t}"),
                        (false, None) => format!("No save in slot {n} yet"),
                    }
                };
                if steps {
                    hints.push(("< >", "Slot"));
                }
                let empty = if saved.is_some() { "No picture" } else { "Empty" };
                (Some(ListPreview { picture, empty, note, slots: true }), None)
            }
            Some(pause_list::Item::Earlier) => {
                let at = self.earlier_files.get(self.earlier_at).cloned();
                let picture = at.as_ref().and_then(|(p, _)| self.preview(p));
                let empty = if at.is_some() { "No picture" } else { "None yet" };
                let note = if self.hardcore() { pocket_ui::achievements::hardcore::NO_LOAD.to_string() } else { pause_list::earlier_note(at.map(|(_, t)| t), now) };
                if steps {
                    hints.push(("< >", "State"));
                }
                (Some(ListPreview { picture, empty, note, slots: false }), None)
            }
            Some(pause_list::Item::Quit) if !self.recovery_required => (None, Some("Next time, A continues from this moment.".to_string())),
            _ => (None, None),
        };
        hints.extend([("B", "Back"), ("A", "OK")]);
        ListPaint::Main { rows, sel, preview, note, hints }
    }
    /// The list menu's rows (`pause_list`), in Linux's order: Continue, Save, Load, Earlier, Options, Achievements while
    /// signed in, Save & quit. Save and Load show the slot, Earlier how long ago its moment is.
    fn list_rows(&self) -> Vec<pause_list::Row> {
        use pause_list::{Item, Row};
        let hardcore = self.hardcore();
        let row = |item, label: &str, chip: Option<String>, steps: bool, enabled: bool| Row { item, label: label.into(), chip, steps, enabled };
        let place = match self.slot.checked_sub(self.filled_slots.len()) {
            Some(_) => place_name(self.slot, self.filled_slots.len()),
            None => (self.slot + 1).to_string(),
        };
        let chip = Some(if hardcore { "Off in Hardcore".to_string() } else { place });
        let steps = !hardcore && self.places() > 1;
        let earlier = self.earlier_files.get(self.earlier_at);
        let now = pocket_ui::play_time::now_secs();
        let earlier_chip = if hardcore { Some("Off in Hardcore".to_string()) } else { earlier.map(|(_, t)| pause_list::earlier_chip(*t, now)) };
        let mut rows = vec![
            row(Item::Continue, "Continue", None, false, true),
            row(Item::Save, "Save", chip.clone(), steps, !hardcore && self.slot < self.filled_slots.len()),
            row(Item::Load, "Load", chip, steps, !hardcore && pocket_ui::slots::load_refusal(&self.filled_slots, self.slot).is_none()),
            row(Item::Earlier, "Earlier", earlier_chip, !hardcore && self.earlier_files.len() > 1, !hardcore && earlier.is_some()),
            row(Item::Options, "Options", None, false, true),
        ];
        if self.ra.is_some() {
            rows.push(row(Item::Achievements, "Achievements", None, false, true));
        }
        rows.push(row(Item::Quit, if self.recovery_required { "Return to library" } else { "Save & quit" }, None, false, true));
        rows
    }
    /// Earlier's moments on the card, newest first: the one written at the start and one every 5 minutes of play
    /// (`earlier0` … `earlier5`), an empty file left out.
    fn refresh_earlier(&mut self) {
        let mut files: Vec<(PathBuf, u64)> = (0..6)
            .filter_map(|n| {
                let path = self.base.with_extension(format!("earlier{n}"));
                let m = std::fs::metadata(&path).ok().filter(|m| m.is_file() && m.len() > 0)?;
                let at = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
                Some((path, at))
            })
            .collect();
        files.sort_by(|a, b| b.1.cmp(&a.1));
        self.earlier_files = files;
        self.earlier_at = 0;
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
    /// on their numbers); Earlier loads the moment it is on.
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
            pause_list::Item::Earlier => {
                let result = if self.hardcore() {
                    Err(pocket_ui::achievements::hardcore::NO_LOAD.to_string())
                } else {
                    match self.earlier_files.get(self.earlier_at).map(|(p, _)| p.clone()) {
                        None => Err(pocket_ui::save_trouble::NOTHING_EARLIER.to_string()),
                        Some(path) => self.load(&path),
                    }
                };
                self.sound(if result.is_err() { pocket_ui::ui_sound::Sound::Error } else { pocket_ui::ui_sound::Sound::Confirm });
                // Every error the screen shows goes through `said` (`save_trouble::tests`).
                self.message = result.map_or_else(|e| said(e, pocket_ui::save_trouble::LOAD_FAILED), |()| "Loaded an earlier moment".into());
                return;
            }
        };
        self.row = legacy;
        self.activate();
        // Options opens on its top page's first row (Picture), as Linux's.
        if self.menu == Menu::Options {
            self.opt_page = None;
            self.opt_sel = 0;
        }
        // A save made a new picture: read again.
        self.previews.clear();
    }
    /// The list menu's keys on its main page: Up/Down go through the rows (round), Left/Right step the slot on Save and
    /// Load and the moment on Earlier, L/R the slot from any row, A does the row. B and Back are the sheet's (Continue).
    fn key_list(&mut self, key: i32) {
        let rows = self.list_rows();
        let n = rows.len().max(1);
        self.list_sel = self.list_sel.min(n - 1);
        let item = rows.get(self.list_sel).map(|r| r.item);
        match key {
            19 => self.list_sel = (self.list_sel + n - 1) % n,
            20 => self.list_sel = (self.list_sel + 1) % n,
            21 | 22 | 102 | 103 => {
                let back = matches!(key, 21 | 102);
                let on_slots = matches!(item, Some(pause_list::Item::Save | pause_list::Item::Load));
                if matches!(key, 21 | 22) && item == Some(pause_list::Item::Earlier) && !self.earlier_files.is_empty() {
                    let len = self.earlier_files.len();
                    self.earlier_at = (self.earlier_at + if back { len - 1 } else { 1 }) % len;
                } else if (on_slots || matches!(key, 102 | 103)) && self.places() > 0 {
                    let places = self.places();
                    self.slot = (self.slot + if back { places - 1 } else { 1 }) % places;
                }
            }
            23 | 66 | 96 => self.activate_list(),
            _ => (),
        }
    }
    /// What the game has for the Options page's groups: emulator options (the core's, outside Controls and Cheats),
    /// control rows, cheats.
    fn opt_has(&self) -> pause_options::Has {
        let options = self.display_options();
        let sections: Vec<usize> = options.iter().enumerate().filter(|(i, _)| *i >= picture::ROWS).map(|(i, o)| option_section(o, i)).collect();
        pause_options::Has {
            emulator: sections.iter().any(|s| !matches!(*s, CONTROLS | CHEATS)),
            controls: sections.contains(&CONTROLS),
            cheats: sections.contains(&CHEATS),
        }
    }
    /// The page the list menu shows under Options, Linux's layout (`pause_options`): the top page (Picture ›, Emulator ›,
    /// Controls ›, Zoom ›, Cheats ›, Over the game, Keep for, Screenshot) or the group it opened. Each row with what it
    /// does; a heading has None.
    fn list_option_rows(&self) -> (Vec<pause_list::ListRow>, Vec<Option<OptTarget>>) {
        use pause_list::ListRow;
        let mut rows = Vec::new();
        let mut targets = Vec::new();
        let Some(page) = self.opt_page else {
            for entry in pause_options::top(self.opt_has()) {
                let value = match entry {
                    pause_options::Entry::OverGame => pause_options::over_game_label(self.picture.over_game).to_string(),
                    pause_options::Entry::Keep => pause_options::keep_label(self.picture.for_game()).to_string(),
                    _ => String::new(),
                };
                rows.push(ListRow::Row { label: entry.label().into(), value, opens: entry.opens(), enabled: true });
                targets.push(Some(OptTarget::Top(entry)));
            }
            return (rows, targets);
        };
        if page == pause_options::Entry::Picture {
            for row in pause_options::PICTURE_ROWS {
                rows.push(ListRow::Row { label: row.name().into(), value: row.label(self.picture_index(row)).into(), opens: false, enabled: true });
                targets.push(Some(OptTarget::Picture(row)));
            }
            return (rows, targets);
        }
        let options = self.display_options();
        let in_page = |i: usize| -> bool {
            if i < picture::ROWS {
                return false;
            }
            let section = option_section(&options[i], i);
            match page {
                pause_options::Entry::Controls => section == CONTROLS,
                pause_options::Entry::Cheats => section == CHEATS,
                _ => !matches!(section, CONTROLS | CHEATS),
            }
        };
        let mut section: Option<usize> = None;
        for i in option_page(&options, self.advanced_open) {
            let (head, row) = if i == options.len() + 1 {
                if page != pause_options::Entry::Emulator {
                    continue;
                }
                let count = options.iter().enumerate().filter(|(r, o)| option_section(o, *r) == ADVANCED).count();
                (ADVANCED, ListRow::Row { label: "Advanced (this emulator)".into(), value: count.to_string(), opens: !self.advanced_open, enabled: true })
            } else {
                if i >= options.len() || !in_page(i) {
                    continue;
                }
                let option = &options[i];
                let (name, restart) = option_name(&option.label);
                let label = if restart { format!("{name} · {}", pocket_ui::i18n::tr("after a restart")) } else { name };
                let value = option_value(option.label_of(&option.current));
                (option_section(option, i), ListRow::Row { label, value, opens: false, enabled: option.values.len() > 1 })
            };
            // The Emulator page keeps its sections as quiet headings (the core's Picture, Sound, Speed); the Advanced group
            // has its own row instead of a heading; the Controls and Cheats pages are one section each.
            if section != Some(head) {
                if page == pause_options::Entry::Emulator && head != ADVANCED {
                    rows.push(ListRow::Heading(SECTIONS[head].into()));
                    targets.push(None);
                }
                section = Some(head);
            }
            rows.push(row);
            targets.push(Some(if i == options.len() + 1 { OptTarget::Advanced } else { OptTarget::Option(i) }));
        }
        (rows, targets)
    }
    /// The player's index of a Picture row (`picture::Settings`).
    fn picture_index(&self, row: pause_options::PictureRow) -> usize {
        match row {
            pause_options::PictureRow::Scaling => self.picture.scaling,
            pause_options::PictureRow::Look => self.picture.effect,
            pause_options::PictureRow::Frame => self.picture.frame,
        }
    }
    /// The selected row's target, the selection kept on a row (never on a heading, never past the end).
    fn opt_target(&mut self) -> Option<OptTarget> {
        let (_, targets) = self.list_option_rows();
        if targets.get(self.opt_sel).copied().flatten().is_none() {
            self.opt_sel = targets.iter().position(Option::is_some).unwrap_or(0);
        }
        targets.get(self.opt_sel).copied().flatten()
    }
    /// The Options page's keys: Up/Down go through the rows (round, past the headings), Left/Right change a value, A opens
    /// a group or does the row, B goes back (a group to the top page onto its row, the top page to the pause menu).
    fn key_list_options(&mut self, key: i32) {
        let (_, targets) = self.list_option_rows();
        let rows: Vec<usize> = targets.iter().enumerate().filter(|(_, t)| t.is_some()).map(|(i, _)| i).collect();
        if rows.is_empty() {
            return;
        }
        let at = rows.iter().position(|r| *r == self.opt_sel).unwrap_or(0);
        match key {
            19 => self.opt_sel = rows[(at + rows.len() - 1) % rows.len()],
            20 => self.opt_sel = rows[(at + 1) % rows.len()],
            21 | 22 | 102 | 103 => {
                self.opt_sel = rows[at];
                self.opt_change(if matches!(key, 21 | 102) { -1 } else { 1 });
            }
            23 | 66 | 96 => {
                self.opt_sel = rows[at];
                self.opt_activate();
            }
            _ => (),
        }
    }
    /// Left/Right on the selected row: a value steps; a group, Zoom and Screenshot have none.
    fn opt_change(&mut self, delta: i32) {
        match self.opt_target() {
            Some(OptTarget::Picture(row)) => {
                let (index, player_row) = match row {
                    pause_options::PictureRow::Scaling => (self.picture.scaling, 0),
                    pause_options::PictureRow::Frame => (self.picture.frame, 1),
                    pause_options::PictureRow::Look => (self.picture.effect, 2),
                };
                let next = row.step(index, delta);
                if let Err(error) = self.picture.change(player_row, next as i32 - index as i32) { self.message = said(error, pocket_ui::save_trouble::SETTING_FAILED); }
                self.overlay_size = None;
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
            _ => return,
        }
        self.sound(pocket_ui::ui_sound::Sound::Nav);
    }
    /// A on the selected row.
    fn opt_activate(&mut self) {
        match self.opt_target() {
            Some(OptTarget::Top(entry @ (pause_options::Entry::Picture | pause_options::Entry::Emulator | pause_options::Entry::Controls | pause_options::Entry::Cheats))) => {
                self.opt_top_sel = self.opt_sel;
                self.opt_page = Some(entry);
                self.opt_sel = 0;
                self.opt_target();
                self.sound(pocket_ui::ui_sound::Sound::Confirm);
            }
            Some(OptTarget::Top(pause_options::Entry::Zoom)) => {
                self.menu = Menu::Zoom;
                self.zoom = play_cards::Zoom::new(&self.system);
                self.from_options = true;
            }
            Some(OptTarget::Top(pause_options::Entry::Screenshot)) => {
                // The sheet's Screenshot (its sound and line), then back on this row.
                self.menu = Menu::Pause;
                self.row = 5;
                self.activate();
                self.menu = Menu::Options;
            }
            Some(OptTarget::Top(_) | OptTarget::Picture(_) | OptTarget::Option(_)) => {
                self.opt_change(1);
            }
            Some(OptTarget::Advanced) => {
                self.advanced_open = !self.advanced_open;
                self.sound(pocket_ui::ui_sound::Sound::Confirm);
            }
            None => (),
        }
    }
    /// B on the Options page: a group goes back to the top page onto its row; the top page is the caller's (the pause menu).
    fn opt_back(&mut self) -> bool {
        if self.opt_page.take().is_some() {
            self.opt_sel = self.opt_top_sel;
            return true;
        }
        false
    }
    /// The line at the bottom left for the selected row.
    fn opt_desc(&mut self) -> String {
        match self.opt_target() {
            Some(OptTarget::Top(pause_options::Entry::Screenshot)) if !self.message.is_empty() => self.message.clone(),
            Some(OptTarget::Top(entry)) => entry.desc(self.picture.for_game()).to_string(),
            Some(OptTarget::Picture(row)) => row.desc(self.picture_index(row)).to_string(),
            Some(OptTarget::Option(i)) => self.display_options().get(i).map_or(String::new(), |o| o.info.clone()),
            _ => String::new(),
        }
    }
    fn display_options(&self) -> Vec<FileOption> {
        let mut options = self.picture.options();
        let choices = play_cards::targets(&self.system, &self.control_names);
        let mut values = vec![pocket_ui::core_options::FileValue {value:"none".into(), label:"Off".into()}];
        values.extend(choices.iter().filter(|t| !(2..8).contains(&t.id)).map(|t| pocket_ui::core_options::FileValue {value:t.id.to_string(),label:t.label.clone()}));
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
        let place = if self.hardcore() { "Off in Hardcore".to_string() } else { place_name(self.slot, self.filled_slots.len()) };
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
            let ids: Vec<_> = std::iter::once(None).chain(choices.iter().filter(|t| !(2..8).contains(&t.id)).map(|t| Some(t.id))).collect();
            let at = ids.iter().position(|id| *id == self.hold_button).unwrap_or(0);
            let before=self.hold_button;
            self.hold_button = ids[(at as i32 + delta).rem_euclid(ids.len() as i32) as usize];
            self.held = Default::default();
            let path = self.data.join("android-controls").join(format!("{}.json",self.key));
            if let Err(e) = pocket_ui::fsutil::write_json(&path, &self.hold_button) {self.hold_button=before;self.message = said(e.to_string(), pocket_ui::save_trouble::SETTING_FAILED);}
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
            3 if self.earlier == 0 => Err(pocket_ui::save_trouble::NOTHING_EARLIER.into()),
            3 => {
                let path = self.base.with_extension(format!("earlier{}", self.earlier.saturating_sub(1) % 6));
                let result = self.load(&path);
                if result.is_ok() {
                    self.earlier = self.earlier.saturating_sub(1);
                }
                result
            }
            4 => {
                self.options = self.core.options_snapshot().1;
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
        if self.paused && self.list_menu && matches!(self.menu, Menu::Pause | Menu::Options) {
            if phase == 2 {
                if let Some(&(_, b)) = self.list_hint_hits.iter().find(|(a, _)| a.contains(x, y)) {
                    match b {
                        pocket_ui::pad::Button::South => { self.key(4, true); }
                        pocket_ui::pad::Button::East => { self.key(96, true); }
                        _ => (),
                    }
                } else if let Some(&(_, i)) = self.list_hits.iter().find(|(a, _)| a.contains(x, y)) {
                    if self.menu == Menu::Pause {
                        self.list_sel = i;
                        self.activate_list();
                    } else if self.list_option_rows().1.get(i).copied().flatten().is_some() {
                        self.opt_sel = i;
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
            self.pulses |= bits & !before;
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
    fn frame(&mut self, w: i32, h: i32, density: f32, insets: [i32; 4], font_scale: f32) -> i32 {
        if self.quit {
            return 2;
        }
        if self.last_font_scale != font_scale {self.overlay_size=None;self.last_font_scale=font_scale;}

        pocket_ui::type_scale::set_system_scale(font_scale);
        let size = play_cards::text_size(self.text_size, font_scale);
        pocket_ui::type_scale::set(size);
        let [left, top, right, bottom] = insets;
        self.top = top;
        let view = (left, bottom, (w - left - right).max(1), (h - top - bottom).max(1));
        if self.view != view {
            log::info!("player viewport: physical={w}x{h} insets={insets:?} density={density}");
        }
        self.view = view;
        self.window = (w.max(0) as u32, h.max(0) as u32);
        self.logical = pocket_ui::scaled_layout::mobile_size((self.view.2 as u32, self.view.3 as u32), density);
        // Overlay pixels per dp: the touch controls are sized in dp (at least 56 dp a button).
        self.dp = self.logical.0 as f32 / (self.view.2.max(1) as f32 / density.max(0.5));
        // The compositor can run below the core rate (notably SwiftShader on high-DPI phones).
        // Catch up bounded emulation ticks before painting once; audio must not lose samples when
        // a presentation misses vsync. Core calls remain on the EGL thread for hardware cores.
        // One core frame per display refresh near the display's rate, as pocket-core on Linux (`core_pacing`): the
        // wall clock against the core's own rate repeated and doubled pictures for seconds every ten (SNES, 17.11.).
        let call = Instant::now();
        let due = if !self.paused && self.reminder.is_none() { self.pacer.due(call) } else { 0 };
        let mut ticks = 0;
        let mut core_time = Duration::ZERO;
        while ticks < due {
            ticks += 1;
            let touch = self.fingers.values().fold(self.pulses, |v, b| v | b);
            let mut buttons = self.buttons | touch | self.stick_buttons;
            self.pulses = 0;
            if self.touch_visible {
                if let Some((axes, consumed)) = play_cards::touch_analog(&self.system, touch) {
                    self.core.set_analog(axes);
                    buttons &= !consumed;
                }
            }
            let was_held = self.held.held();
            buttons = self.held.apply(buttons,self.hold_button);
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
            let (frame, samples, av, geometry) = self.core.run(buttons);
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
            let frame = core_hw_frame(&self.core).or(frame);
            pocket_ui::mark!("android-readback", "phase=end frame={}", self.frames);
            if let Some(frame) = frame {
                self.video_frames += 1;
                self.frame = Some(player_video::image(frame));
            }
            if let Some(audio) = &mut self.audio {
                audio.push(&samples);
            }
            self.frames += 1;
            // RetroAchievements reads the game's memory after each emulated frame (well under a millisecond).
            if let Some(ra) = &mut self.ra { ra.frame(); }
            if self.last_flush.elapsed() > Duration::from_secs(10) {
                self.core.write_sram_in_background();
                self.last_flush = Instant::now();
                // The pacing of the last ten seconds, and the display's rate again (a phone may switch its mode).
                let cpu = pocket_ui::core_pacing::current_cpu();
                log::info!("player pacing: {}", self.pacing.line(self.pacer.locked(), self.core.av.timing.fps, self.display_hz, cpu));
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
            if self.last_earlier.elapsed() > Duration::from_secs(300) {
                if let Some(writer) = &self.earlier_writer {
                    let started = Instant::now();
                    if let Some((bytes, serialize_time)) = self.core.save_state_with_serialize_time().filter(|(b, _)| !b.is_empty() && b.len() <= pocket_ui::state_file::MAX_STATE) {
                        let capture_ms = started.elapsed().as_secs_f64() * 1000.0;
                        let serialize_ms = serialize_time.as_secs_f64() * 1000.0;
                        let size = bytes.len();
                        // Move the owned state and share the immutable frame. RGB copying, PNG
                        // encoding, compression and durable writes all happen on the worker.
                        let image = self.frame.clone();
                        let path = self.base.with_extension(format!("earlier{}", self.earlier % 6));
                        log::info!("player Earlier captured: bytes={size} serialize_ms={serialize_ms:.3} capture_ms={capture_ms:.3}");
                        writer.submit(move || {
                            let write_started = Instant::now();
                            if let Err(error) = player_video::moment(&path, &bytes, image.as_deref()) {
                                log::warn!("Earlier moment could not be kept: {error}");
                                return;
                            }
                            log::info!("player Earlier written: bytes={size} serialize_ms={serialize_ms:.3} worker_ms={:.3}", write_started.elapsed().as_secs_f64() * 1000.0);
                        });
                        self.earlier += 1;
                    } else { log::warn!("Earlier capture failed or exceeded the state size limit"); }
                }
                self.last_earlier = Instant::now();
            }
        }
        if !self.paused && self.reminder.is_none() {
            self.pacing.call(call, ticks, core_time);
        }
        // Over the game → FPS: the line changes twice a second, and then the overlay is painted again.
        if !self.paused && self.reminder.is_none() && self.picture.over_game && self.fps.frame(Instant::now(), ticks) {
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
            let until = *self.note_until.get_or_insert_with(|| Instant::now() + NOTE_FOR);
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
        self.paint();
        pocket_ui::mark!("android-present", "phase=end frame={}", self.frames);
        if self.frames > 0 && !self.first_frame {
            // The tests' end of "A or tap to the game's first frame" (the launcher logs the start).
            self.first_frame = true;
            log::info!("player first game frame");
        }
        // 1: the menu is up and nothing moves (a frame only when something happens); 3: a direction is held in it.
        if self.paused && self.hat_repeat.held() {
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
        let list = if self.paused && self.list_menu && matches!(self.menu, Menu::Pause | Menu::Options) { Some(self.list_paint()) } else { None };
        let (w, h) = (self.logical.0 as i32, self.logical.1 as i32);
        // Where the picture goes, and in which pixels (`picture::Settings::place`).
        let space = self.frame.as_deref().map(|image| self.picture.place(
            (self.window.0 as i32, self.window.1 as i32), self.view, (w,h), (image.w,image.h),
            self.core.av.geometry.aspect_ratio as f64, self.touch_visible,
        ));
        let game_rect = space.map(|s| s.game);
        let (pic_size, pic_view) = space.map_or(((w, h), self.view), |s| (s.size, s.view));
        self.gfx.effect = self.picture.effect;
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
            if changed { log::info!("player picture: system={} logical={}x{} rect={},{},{}x{} physical={}x{} on_window={:?} scaling={} frame={} effect={} touch={} plan={}", self.system,w,h,rect.x,rect.y,rect.w,rect.h,self.view.2,self.view.3,space.map(|s| s.on_window(self.window.1 as i32)),self.picture.scaling,self.picture.frame,self.picture.effect,self.touch_visible,frame_plan.is_some()); }
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
            self.note_area = paint_note(&mut c, &mut self.text, w, &self.note);
        }
        // An achievement unlock: its card at the top, under the note when there is one.
        self.card_label.clear();
        if !self.paused {
            if let Some(ra) = &mut self.ra {
                if let Some(card) = ra.card(Instant::now()) {
                    let top = if self.note.is_empty() { 14 } else { self.note_area.h + 12 };
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
            match list {
                ListPaint::Main { rows, sel, preview, note, hints } => {
                    let icon = self.slot_icon.as_ref().map_or(pocket_ui::save_media::Icon::None, pocket_ui::save_media::Icon::Console);
                    let filled: Vec<bool> = self.filled_slots.to_vec();
                    let ring = (self.slot < filled.len()).then_some(self.slot);
                    let preview = preview.as_ref().map(|p| pause_list::Preview {
                        picture: p.picture.as_deref(),
                        empty: p.empty,
                        note: p.note.clone(),
                        slots: p.slots.then(|| pause_list::Slots { kind: pocket_ui::save_media::Kind::of(&self.system), filled: &filled, ring, icon }),
                    });
                    let out = pause_list::paint_main(&mut c, &mut self.text, &pause_list::Main { title: &self.title, status: &self.message, rows: &rows, sel, preview, note, hints: &hints });
                    self.list_hits = out.rows.into_iter().enumerate().map(|(i, a)| (a, i)).collect();
                    self.list_hint_hits = out.hints;
                }
                ListPaint::Options { rows, sel, desc, title } => {
                    let hints: &[(&str, &str)] = match rows.get(sel) {
                        Some(pause_list::ListRow::Row { opens: true, .. }) => &[("B", "Back"), ("A", "Open")],
                        Some(pause_list::ListRow::Row { value, .. }) if !value.is_empty() => &[("B", "Back"), ("< >", "Change")],
                        _ => &[("B", "Back"), ("A", "OK")],
                    };
                    let out = pause_list::paint_list(&mut c, &mut self.text, &pause_list::List { title: &title, rows: &rows, sel, desc: &desc, hints });
                    self.list_hits = out.rows.into_iter().enumerate().filter_map(|(i, a)| a.map(|a| (a, i))).collect();
                    self.list_hint_hits = out.hints;
                }
            }
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
                filled: &self.filled_slots,
                kind: pocket_ui::save_media::Kind::of(&self.system),
                system: &self.system,
                icon,
                options: &display_options,
                scroll: self.scroll,
                frame: self.frame.as_deref().zip(game_rect).map(|(image, rect)| (image, rect.w as f64 / rect.h.max(1) as f64)),
                focus: (!self.touch_visible).then_some(self.row),
                grey: [pocket_ui::slots::load_refusal(&self.filled_slots, self.slot).is_some() || hardcore, self.earlier == 0 || hardcore],
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
/// The list menu's page as gathered for one paint (`Player::list_paint`).
enum ListPaint {
    Main { rows: Vec<pause_list::Row>, sel: usize, preview: Option<ListPreview>, note: Option<String>, hints: Vec<(&'static str, &'static str)> },
    Options { rows: Vec<pause_list::ListRow>, sel: usize, desc: String, title: String },
}
struct ListPreview {
    picture: Option<std::sync::Arc<Image>>,
    empty: &'static str,
    note: String,
    slots: bool,
}
/// How long the lines about a start stay over the game.
const NOTE_FOR: Duration = Duration::from_secs(8);
const KEPT_ONLY_LOADS: &str = "A kept moment can only be loaded: pick a slot to save";
/// What the slot picker calls a place: a slot, or one of the kept moments after them.
fn place_name(place: usize, slots: usize) -> String {
    match place.checked_sub(slots) {
        Some(kept) => format!("Kept moment {}", kept + 1),
        None => format!("Slot {}", place + 1),
    }
}
fn note_text(line: &str) -> String {
    pocket_ui::i18n::tr(line)
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
        let name = place_name(v.slot, v.filled.len());
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
                // The handheld view's picture rows for this game win over the files the player keeps (`picture::follow_view`).
                if prefs["picture"].is_object() { player.picture.follow_view(&prefs["picture"]); }
                // A game from the Handheld view: its pause menu is the handhelds' (`pause_list`).
                player.list_menu = prefs["pause_menu"].as_str() == Some("handheld");
                player.sounds_muted = prefs["muted"].as_bool().unwrap_or(false);
                player.haptics = prefs["haptics"].as_bool().unwrap_or(false);
                player.text_size = serde_json::from_value(prefs["text_size"].clone()).unwrap_or_default();
                let hold_path = player.data.join("android-controls").join(format!("{}.json",player.key));
                player.hold_button = pocket_ui::fsutil::read_json_or_backup::<Option<u8>>(&hold_path).flatten().filter(|b| *b<16 && !(2..8).contains(b));
                if player.resumed && player.frame.is_some() {
                    let r=&prefs["reminder"];
                    if let Some(ago)=r["ago"].as_str() {
                        log::info!("Android reminder: {ago}");
                        player.reminder=Some(pocket_ui::where_was_i::Card{ago:ago.into(),played:r["played"].as_str().map(str::to_string),progress:None,art:None,note:r["note"].as_str().map(str::to_string)});
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
        if p.paused && p.frame.is_some() && matches!(p.menu,Menu::Pause|Menu::Zoom) {
            p.menu=Menu::Zoom;p.zoom.magnify(factor);p.zoom_fingers.clear();p.zoom_distance=None;
        }
    }});
}

#[no_mangle]
pub extern "system" fn Java_xyz_pocketui_app_PlayerActivity_nativeForeground(_: JNIEnv,_:JObject) {
    PLAYER.with(|slot|{if let Some(p)=slot.borrow_mut().as_mut(){p.suspended=false;}});
}
