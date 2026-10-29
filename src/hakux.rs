//! Xbox games: hakuX (rfandango's Xbox emulator, libretro/Linux port by portare-ch) played by ROCKNIX's RetroArch as a
//! standalone guest of the pause menu (`standalone::Emu::Hakux`). docs/XBOX.md has the whole story.
//!
//! - The core ships in pocket-ui's own folder (`DIR`), never in ROCKNIX's libretro folder; RetroArch is started with
//!   `-L` on it, ROCKNIX's global `retroarch.cfg` and an appendconfig of ours (`appendconfig`) that wins over it.
//! - The appendconfig makes the game fullscreen on Vulkan, takes the RetroArch hotkeys that clash with pocket-ui's
//!   HOME away, keeps every file of the game (HDD image, caches, saves, screenshots) under pocket-ui's data folder
//!   (`<data>/hakux`, never in `roms/xbox`) and opens RetroArch's network commands on a port picked for this session.
//! - Quit is RetroArch's own `QUIT` command; `standalone` falls back to signals and the process group.
//! - The core's `.info` says whether it has save states (`savestate = "true"`); today it says false (the core
//!   reports a serialize size of 0). Save, Load and the auto state of Quit appear by themselves once it says true
//!   (`states_supported`, `standalone::Session::states`).

use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// Where the core and its `.info` live (the install package puts them there; the release folders don't hold them,
/// so an update of pocket-ui leaves them as they are).
pub const DIR: &str = "/storage/pocket-ui/hakux";
pub const CORE_FILE: &str = "hakux_libretro.so";
pub const INFO_FILE: &str = "hakux_libretro.info";
/// ROCKNIX's RetroArch and its global configuration, which our appendconfig is laid over.
pub const RETROARCH: &str = "/usr/bin/retroarch";
pub const GLOBAL_CFG: &str = "/storage/.config/retroarch/retroarch.cfg";
/// RetroArch's own default is 55355 (`control::retroarch`, ROCKNIX's RetroArch games); a game of ours never uses it, so a
/// command meant for one can't reach the other.
const FALLBACK_PORT: u16 = 55359;

/// What the pause menu says in place of Save and Load while the core has no save states.
pub const NO_STATES: &str = "No save states for Xbox games yet: save in the game before you quit.";

pub fn core_path() -> PathBuf {
    Path::new(DIR).join(CORE_FILE)
}

/// The core and RetroArch are there: Xbox games can start from the shelf.
pub fn installed() -> bool {
    installed_in(Path::new(DIR), Path::new(RETROARCH))
}

fn installed_in(dir: &Path, retroarch: &Path) -> bool {
    dir.join(CORE_FILE).is_file() && dir.join(INFO_FILE).is_file() && retroarch.exists()
}

/// Whether the core's `.info` says it has save states (RetroArch's own source for it: its menu and run-ahead read the same key).
pub fn states_supported(info: &str) -> bool {
    info.lines().any(|l| {
        let l = l.trim();
        l.strip_prefix("savestate").and_then(|r| r.trim_start().strip_prefix('=')).is_some_and(|v| v.trim().trim_matches('"') == "true")
    })
}

/// The shipped core says it has save states.
pub fn core_has_states() -> bool {
    std::fs::read_to_string(Path::new(DIR).join(INFO_FILE)).is_ok_and(|i| states_supported(&i))
}

/// system.cfg keys of the Options page (`emu_settings::XBOX`): per game (`xbox["<file>"].<key>`) or for all Xbox games (`xbox.<key>`).
pub const RESOLUTION_KEY: &str = "pocket_resolution";
pub const SCALING_KEY: &str = "pocket_scaling";

/// The picture size, as the core's render scale: 1 is the Xbox's own 640x480, 2 is twice that.
pub const DEFAULT_SCALE: &str = "1";

/// The render scale of a game nobody has set: 2x on the SM8550 handhelds (Odin 2 Mini measured: 2x held the same 58 guest fps
/// and 48 new images per second as 1x, GPU 27 % against 10 %, no hotter), the Xbox's own size elsewhere.
pub fn default_scale() -> &'static str {
    if crate::device::current().is_sm8550() { "2" } else { DEFAULT_SCALE }
}

/// Screen scaling, the names of pocket-core's Options → Picture → Screen scaling (`options.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scaling {
    /// As large as fits in the core's shape (4:3, or 16:9 with the widescreen screen format), black bars.
    Aspect,
    /// The whole 1240x1080 panel; the picture stretches.
    Fullscreen,
    /// Whole-number multiples of the picture's height.
    Integer,
}

/// What the player chose for a game (`emu_settings::XBOX`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    /// The core's `hakux_render_scale`: "1" to "4" (we offer 1 and 2).
    pub render_scale: String,
    pub scaling: Scaling,
}

impl Default for Picture {
    fn default() -> Picture {
        Picture { render_scale: DEFAULT_SCALE.into(), scaling: Scaling::Aspect }
    }
}

/// The game's picture settings from system.cfg's text: its own key, then the Xbox's, then the defaults.
pub fn picture_of(cfg: &str, file: &str) -> Picture {
    picture_with_default(cfg, file, default_scale())
}

fn picture_with_default(cfg: &str, file: &str, default_scale: &str) -> Picture {
    let get = |key: &str| crate::emu_settings::es_get(cfg, "xbox", file, key);
    Picture {
        render_scale: get(RESOLUTION_KEY).filter(|v| matches!(v.as_str(), "1" | "2" | "3" | "4")).unwrap_or_else(|| default_scale.into()),
        scaling: match get(SCALING_KEY).as_deref() {
            Some("fullscreen") => Scaling::Fullscreen,
            Some("integer") => Scaling::Integer,
            _ => Scaling::Aspect,
        },
    }
}

/// Everything under pocket-ui's data folder that the game writes.
pub fn data_in(data_dir: &Path) -> PathBuf {
    data_dir.join("hakux")
}

/// The core's options at every start: hakuX's own defaults and the game's resolution (the core reads it once, before the
/// console boots). RetroArch's menu is off, so nobody changes the others meanwhile.
pub fn core_options(p: &Picture) -> String {
    format!(
        "hakux_render_scale = \"{}\"\nhakux_crop_overscan = \"enabled\"\nhakux_screen_format = \"normal\"\nhakux_language = \"english\"\nhakux_video_region = \"auto\"\n",
        p.render_scale
    )
}

/// RetroArch keys that open its menu, close the game or act on states, taken away so only pocket-ui's HOME (pause menu)
/// reaches the player: RetroArch's menu over pocket-ui's would pause the game twice, a hotkey exit would skip the Quit.
const NO_HOTKEYS: &[&str] = &[
    "input_menu_toggle",
    "input_menu_toggle_btn",
    "input_exit_emulator",
    "input_exit_emulator_btn",
    "input_close_content",
    "input_close_content_btn",
    "input_enable_hotkey_btn",
    "input_save_state",
    "input_save_state_btn",
    "input_load_state",
    "input_load_state_btn",
    "input_state_slot_increase",
    "input_state_slot_increase_btn",
    "input_state_slot_decrease",
    "input_state_slot_decrease_btn",
    "input_pause_toggle",
    "input_pause_toggle_btn",
    "input_reset",
    "input_reset_btn",
    "input_game_focus_toggle",
    "input_game_focus_toggle_btn",
    "input_fps_toggle_btn",
    "input_toggle_fast_forward_btn",
    "input_hold_fast_forward_btn",
    "input_rewind_btn",
    "input_screenshot_btn",
    "input_shader_toggle_btn",
    "input_toggle_fullscreen_btn",
    "input_ai_service_btn",
    "input_recording_toggle_btn",
    "input_streaming_toggle_btn",
    "input_overlay_next_btn",
    "input_disk_eject_toggle_btn",
    "input_disk_next_btn",
    "input_disk_prev_btn",
    "input_cheat_index_plus_btn",
    "input_cheat_index_minus_btn",
    "input_cheat_toggle_btn",
    "input_audio_mute_btn",
    "input_volume_up_btn",
    "input_volume_down_btn",
    "input_netplay_game_watch_btn",
    "input_slowmotion_btn",
    "input_frame_advance_btn",
    "input_grab_mouse_toggle_btn",
    "input_osk_toggle_btn",
];

/// The appendconfig: what a hakuX game needs on top of ROCKNIX's `retroarch.cfg`. `data` is `data_in(..)`; `states_dir`
/// is where RetroArch keeps save states, which only count once the core has them.
pub fn appendconfig(data: &Path, states_dir: &Path, port: u16, picture: &Picture) -> String {
    let d = |sub: &str| data.join(sub).display().to_string();
    let mut c = String::from("# Written by pocket-ui at every Xbox start (src/hakux.rs); changes here don't last.\n");
    let mut set = |k: &str, v: &str| c.push_str(&format!("{k} = \"{v}\"\n"));
    // Picture: fullscreen on the Vulkan device the core negotiates, nothing between the core and the screen.
    set("video_driver", "vulkan");
    set("video_context_driver", "wayland");
    set("video_fullscreen", "true");
    set("video_windowed_fullscreen", "true");
    set("video_threaded", "false");
    set("video_vsync", "true");
    // Pacing: RetroArch's own clock at the core's 59.94 Hz, not the audio's. With audio_sync on, RetroArch blocks in each
    // audio write until PipeWire takes the next quantum (ROCKNIX forces 1024 frames: 21.3 ms), so retro_run comes in
    // bursts 46.9 times a second and RetroArch took only ~47.5 new pictures a second from the core's 59.9 (Mini V2 and
    // Odin 2 Mini alike). audio_sync off alone lets Mesa's Wayland swapchain run the loop at ~62.5 Hz, and the core's
    // sound, made as fast as retro_run takes it, then runs 4 % ahead. With RetroArch's own limiter (vrr_runloop_enable)
    // on the Mini V2: 59.94 retro_run/s, 59.9 new pictures a second at 59.9 guest fps, the core's audio at 1500 frames/s
    // with no short period, no xrun (docs/XBOX.md "Frame pacing").
    set("audio_sync", "false");
    set("vrr_runloop_enable", "true");
    set("video_shader_enable", "false");
    // RetroArch's aspect_ratio_index (setsettings.sh's CORE_RATIOS): 22 core provided, 24 full; integer scaling is its own switch.
    set("aspect_ratio_index", if picture.scaling == Scaling::Fullscreen { "24" } else { "22" });
    set("video_scale_integer", if picture.scaling == Scaling::Integer { "true" } else { "false" });
    set("video_fps_show", "false");
    set("run_ahead_enabled", "false");
    set("preemptive_frames_enable", "false");
    set("video_gpu_screenshot", "true");
    // Where everything goes: the HDD image, caches and saves of the console, never next to the ISO.
    set("savefile_directory", &d("save"));
    set("system_directory", &d("system"));
    set("cache_directory", &d("cache"));
    set("screenshot_directory", &d("screenshots"));
    // core_options_path is the file of the *global* options: RetroArch reads it only with this on (else <config>/hakuX/hakuX.opt).
    set("global_core_options", "true");
    set("core_options_path", &d("core-options.cfg"));
    set("content_history_path", &d("history.lpl"));
    set("content_favorites_path", &d("favorites.lpl"));
    set("savestate_directory", &states_dir.display().to_string());
    set("libretro_directory", DIR);
    set("libretro_info_path", DIR);
    set("savefiles_in_content_dir", "false");
    set("savestates_in_content_dir", "false");
    set("sort_savefiles_enable", "false");
    set("sort_savefiles_by_content_enable", "false");
    set("sort_savestates_enable", "false");
    set("sort_savestates_by_content_enable", "false");
    set("screenshots_in_content_dir", "false");
    set("content_history_write", "false");
    set("history_list_enable", "false");
    set("config_save_on_exit", "false");
    // ROCKNIX's retroarch.cfg has quit_press_twice on, which makes RetroArch ignore a single QUIT command (measured on the
    // Mini V2: a QUIT sent to the running game did nothing, SIGTERM only worked after a 10 s wait).
    set("quit_press_twice", "false");
    set("auto_overrides_enable", "false");
    set("auto_remaps_enable", "false");
    set("auto_shaders_enable", "false");
    set("savestate_auto_load", "false");
    set("savestate_auto_save", "false");
    set("cheevos_enable", "false");
    // Our pause menu does the pausing; RetroArch's own menu and hotkeys stay out of the way.
    set("menu_pause_libretro", "true");
    set("pause_nonactive", "false");
    set("menu_swap_ok_cancel_buttons", "false");
    set("menu_toggle_gamepad_combo", "0");
    set("menu_show_start_screen", "false");
    set("notification_show_when_menu_is_alive", "false");
    for k in NO_HOTKEYS {
        set(k, "nul");
    }
    // The commands pocket-ui sends (`send`), on a port of this session.
    set("network_cmd_enable", "true");
    set("network_cmd_port", &port.to_string());
    set("stdin_cmd_enable", "false");
    c
}

/// A UDP port on localhost that nobody holds right now.
fn free_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0").and_then(|s| s.local_addr()).map_or(FALLBACK_PORT, |a| a.port())
}

/// What `prepare` made for the game about to start.
#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    pub port: u16,
    pub config: PathBuf,
}

static PREPARED: Mutex<Option<Prepared>> = Mutex::new(None);

/// Makes the folders, the core options and the appendconfig for one start (before the process is spawned) and
/// remembers the port. None when the files can't be written.
pub fn prepare(data_dir: &Path, rom: &Path) -> Option<Prepared> {
    let data = data_in(data_dir);
    for sub in ["save", "system", "cache", "screenshots"] {
        std::fs::create_dir_all(data.join(sub)).ok()?;
    }
    let cfg = std::fs::read_to_string(crate::emu_settings::SYSTEM_CFG).unwrap_or_default();
    let picture = picture_of(&cfg, rom.file_name().and_then(|f| f.to_str()).unwrap_or_default());
    crate::fsutil::write_atomic(&data.join("core-options.cfg"), core_options(&picture).as_bytes()).ok()?;
    let states_dir = Path::new(crate::slots::STATES_DIR).join("xbox");
    let port = free_port();
    let config = data.join("pocket-ui.cfg");
    crate::fsutil::write_atomic(&config, appendconfig(&data, &states_dir, port, &picture).as_bytes()).ok()?;
    let p = Prepared { port, config };
    *PREPARED.lock().ok()? = Some(p.clone());
    Some(p)
}

/// The command that starts the game: ROCKNIX's RetroArch on our core with our appendconfig (`prepare` has run).
pub fn command(rom: &Path) -> Command {
    let prepared = PREPARED.lock().ok().and_then(|p| p.clone());
    let mut c = Command::new(RETROARCH);
    c.arg("--config").arg(GLOBAL_CFG);
    if let Some(p) = prepared {
        c.arg("--appendconfig").arg(p.config);
    }
    c.arg("-L").arg(core_path()).arg(rom);
    c
}

/// The port of the running game's RetroArch.
pub fn port() -> u16 {
    PREPARED.lock().ok().and_then(|p| p.as_ref().map(|p| p.port)).unwrap_or(FALLBACK_PORT)
}

/// Sends one RetroArch network command (`QUIT`, `SAVE_STATE_SLOT 1`, …) to this game's RetroArch.
pub fn send(cmd: &str) -> bool {
    send_to(port(), cmd)
}

fn send_to(port: u16, cmd: &str) -> bool {
    UdpSocket::bind("127.0.0.1:0").and_then(|s| s.send_to(cmd.as_bytes(), ("127.0.0.1", port))).is_ok()
}

/// RetroArch's `GET_STATUS` answer ("GET_STATUS PLAYING xbox,<content>,crc32=…"), None when it doesn't answer in time.
pub fn status() -> Option<String> {
    let s = UdpSocket::bind("127.0.0.1:0").ok()?;
    s.set_read_timeout(Some(std::time::Duration::from_millis(400))).ok()?;
    s.send_to(b"GET_STATUS", ("127.0.0.1", port())).ok()?;
    let mut buf = [0u8; 512];
    let n = s.recv(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf[..n]).trim().to_string())
}

/// The game runs (not just RetroArch's start-up).
pub fn playing(status: &str) -> bool {
    status.starts_with("GET_STATUS PLAYING")
}

/// The slot numbers RetroArch takes for pocket-ui's states: its manual slots, `-1` for the auto state it names
/// `<game>.state.auto`, and 98 for "Before load".
pub const AUTO_SLOT: i32 = -1;
pub const UNDO_SLOT: i32 = 98;

pub fn save_command(slot: i32) -> String {
    format!("SAVE_STATE_SLOT {slot}")
}

pub fn load_command(slot: i32) -> String {
    format!("LOAD_STATE_SLOT {slot}")
}

/// RetroArch's state file names: `<game>.state` (slot 0), `.state<N>`, `.state.auto`.
pub fn state_file(dir: &Path, base: &str, slot: i32) -> PathBuf {
    dir.join(match slot {
        -1 => format!("{base}.state.auto"),
        0 => format!("{base}.state"),
        n => format!("{base}.state{n}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO_NO: &str = "display_name = \"Microsoft - Xbox (hakuX)\"\nsavestate = \"false\"\nsavestate_features = \"null\"\n";

    #[test]
    fn states_follow_the_core_info() {
        assert!(!states_supported(INFO_NO), "the shipped info says no save states");
        assert!(states_supported(&INFO_NO.replace("\"false\"\nsavestate_f", "\"true\"\nsavestate_f")));
        assert!(states_supported("savestate=true\n"));
        assert!(!states_supported("# savestate = \"true\"\n"), "a comment says nothing");
        assert!(!states_supported("savestate_features = \"true\"\n"), "another key");
        assert!(!states_supported(""));
        // The info file that ships with the core (deploy/hakux) is the one the rule applies to.
        assert!(!states_supported(include_str!("../deploy/hakux/hakux_libretro.info")));
    }

    #[test]
    fn appendconfig_keeps_the_game_out_of_the_roms_and_off_the_ra_menu() {
        let data = Path::new("/storage/.config/pocket-ui/hakux");
        let c = appendconfig(data, Path::new("/storage/roms/savestates/xbox"), 41234, &Picture::default());
        assert!(!c.contains("/storage/roms/xbox"), "nothing is written into the ROM folder");
        for (k, v) in [
            ("video_driver", "vulkan"),
            ("video_fullscreen", "true"),
            ("video_vsync", "true"),
            ("audio_sync", "false"),
            ("vrr_runloop_enable", "true"),
            ("network_cmd_enable", "true"),
            ("network_cmd_port", "41234"),
            ("savefile_directory", "/storage/.config/pocket-ui/hakux/save"),
            ("system_directory", "/storage/.config/pocket-ui/hakux/system"),
            ("savestate_directory", "/storage/roms/savestates/xbox"),
            ("global_core_options", "true"),
            ("config_save_on_exit", "false"),
            ("quit_press_twice", "false"),
            ("input_menu_toggle_btn", "nul"),
            ("input_exit_emulator_btn", "nul"),
            ("savestate_auto_save", "false"),
        ] {
            assert!(c.lines().any(|l| l == format!("{k} = \"{v}\"")), "{k} = {v} in:\n{c}");
        }
        // Every key once: RetroArch takes the last, but a repeat means two rules fight.
        let mut keys: Vec<&str> = c.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split(" = ").next()).collect();
        let n = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), n);
    }

    #[test]
    fn the_picture_follows_the_game_then_the_xbox_then_the_defaults() {
        let file = "Jade Empire (USA).iso";
        assert_eq!(picture_with_default("", file, DEFAULT_SCALE), Picture::default());
        assert_eq!(picture_with_default("", file, "2").render_scale, "2", "the device's default when nothing is set");
        assert_eq!(picture_with_default("xbox.pocket_resolution=1\n", file, "2").render_scale, "1", "a chosen Original stays");
        assert_eq!(Picture::default().scaling, Scaling::Aspect);
        let cfg = format!("xbox.{RESOLUTION_KEY}=2\nxbox.{SCALING_KEY}=fullscreen\nxbox[\"{file}\"].{SCALING_KEY}=integer\nxbox[\"{file}\"].{RESOLUTION_KEY}=1\n");
        assert_eq!(picture_of(&cfg, file), Picture { render_scale: "1".into(), scaling: Scaling::Integer }, "the game's own keys win");
        assert_eq!(picture_of(&cfg, "Other.iso"), Picture { render_scale: "2".into(), scaling: Scaling::Fullscreen }, "the Xbox's keys for the others");
        assert_eq!(picture_of(&format!("xbox.{RESOLUTION_KEY}=9\nxbox.{SCALING_KEY}=weird\n"), file), Picture::default(), "nonsense is the default");
    }

    #[test]
    fn resolution_and_scaling_reach_the_core_and_retroarch() {
        let data = Path::new("/d");
        let states = Path::new("/s");
        let has = |c: &str, k: &str, v: &str| c.lines().any(|l| l == format!("{k} = \"{v}\""));
        let two = Picture { render_scale: "2".into(), scaling: Scaling::Fullscreen };
        assert!(core_options(&two).contains("hakux_render_scale = \"2\""));
        assert!(core_options(&Picture::default()).contains("hakux_render_scale = \"1\""));
        let full = appendconfig(data, states, 1, &two);
        assert!(has(&full, "aspect_ratio_index", "24") && has(&full, "video_scale_integer", "false"), "Fullscreen is RetroArch's Full");
        let aspect = appendconfig(data, states, 1, &Picture::default());
        assert!(has(&aspect, "aspect_ratio_index", "22") && has(&aspect, "video_scale_integer", "false"), "Aspect is what the core reports");
        let int = appendconfig(data, states, 1, &Picture { scaling: Scaling::Integer, ..Picture::default() });
        assert!(has(&int, "aspect_ratio_index", "22") && has(&int, "video_scale_integer", "true"));
    }

    #[test]
    fn the_command_loads_our_core_with_our_config() {
        let c = command(Path::new("/storage/roms/xbox/Game.iso"));
        let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().to_string()).collect();
        assert_eq!(c.get_program(), RETROARCH);
        assert!(args.windows(2).any(|w| w[0] == "-L" && w[1] == "/storage/pocket-ui/hakux/hakux_libretro.so"), "{args:?}");
        assert_eq!(args.last().map(String::as_str), Some("/storage/roms/xbox/Game.iso"));
        assert!(!args.iter().any(|a| a.contains("/usr/lib/libretro") || a.contains("/tmp/cores")), "ROCKNIX's core folders stay out of it");
    }

    #[test]
    fn installed_needs_core_info_and_retroarch() {
        let d = std::env::temp_dir().join(format!("hakux-inst-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let ra = d.join("retroarch");
        assert!(!installed_in(&d, &ra));
        std::fs::write(d.join(CORE_FILE), b"x").unwrap();
        std::fs::write(d.join(INFO_FILE), b"x").unwrap();
        assert!(!installed_in(&d, &ra), "no RetroArch");
        std::fs::write(&ra, b"x").unwrap();
        assert!(installed_in(&d, &ra));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn commands_and_state_names() {
        assert_eq!(save_command(3), "SAVE_STATE_SLOT 3");
        assert_eq!(load_command(AUTO_SLOT), "LOAD_STATE_SLOT -1");
        let dir = Path::new("/s");
        assert_eq!(state_file(dir, "Game", AUTO_SLOT), PathBuf::from("/s/Game.state.auto"));
        assert_eq!(state_file(dir, "Game", 0), PathBuf::from("/s/Game.state"));
        assert_eq!(state_file(dir, "Game", 2), PathBuf::from("/s/Game.state2"));
        assert!(playing("GET_STATUS PLAYING xbox,Game,crc32=0"));
        assert!(!playing("GET_STATUS CONTENTLESS"));
    }

    #[test]
    fn a_command_reaches_the_port_of_this_game_only() {
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        rx.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        let port = rx.local_addr().unwrap().port();
        assert!(send_to(port, "QUIT"));
        let mut buf = [0u8; 16];
        let n = rx.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"QUIT");
    }
}
