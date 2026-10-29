/// OS adapters used at I/O boundaries; UI and matching logic stay shared.
pub mod platform;
pub mod control;
pub mod proc;
pub mod frame;
pub mod scaled_layout;
pub mod crash;
pub mod logs;
pub mod net_log;
pub mod shutdown_log;
pub mod idle;
pub mod gfx;
pub mod image;
pub mod gamelist;
pub mod gamelist_page;
pub mod gallery;
pub mod library;
pub mod library_cache;
pub mod pad;
/// Couch co-op: which controller is player 1 to 4 in pocket-core games, and each player's input.
pub mod players;
pub mod players_page;
pub mod touch;
pub mod recents;
pub mod text;
pub mod this_game_page;
/// The boot screen pocket-splash draws until sway takes over (docs/BOOT-SPLASH.md).
pub mod splash;
/// The first frame after the Pocket starts fades in from black (docs/BOOT-SPLASH.md).
pub mod boot_fade;
pub mod text_fallback;
pub mod type_scale;
/// The interface in German: the table the text painter looks every string up in (Settings → Language).
pub mod i18n;
pub mod keyboard;
pub mod wifi;
mod wifi_activation;
/// One shared picture of the Wi-Fi (radio, connection, last scan) and the one worker that changes it: quick settings and
/// Settings → Wi-Fi read the same snapshot.
pub mod net_state;
pub mod secret;
pub mod sockets;
pub mod remote;
pub mod report;
pub mod palette;
pub mod kit;
pub mod appearance;
pub mod setup;
// The feature registry, its cards and their previews (first start, Settings → Tour, "New in this version").
pub mod features;
pub mod feature_previews;
pub mod covers;
pub mod auto_covers;
pub mod game_pictures;
pub mod picture_cycle;
pub mod picture_loader;
pub mod ports;
pub mod port_files;
pub mod port_perf;
pub mod port_perf_page;
pub mod audio;
pub mod audio_route;
pub mod clock_check;
pub mod manual_time;
pub mod libretro;
pub mod hw_render;
pub mod options;
pub mod starter;
pub mod store;
pub mod update;
/// Whether a new ROCKNIX system still has what pocket-ui needs (OS updates, docs/UPDATES.md).
pub mod os_compat;
pub mod untar;
pub mod video;
pub mod picture_fx;
#[cfg(target_os = "linux")]
pub mod headless_gl;
pub mod hakux;
pub mod heap_trim;
pub mod hostsys;
pub mod notice;
pub mod archive;
pub mod softpatch;
pub mod dsv;
pub mod vfs;
pub mod bezel;
pub mod ambient;
mod frames;
mod frames_more;
mod frames_tv;
pub mod frame_led;
pub mod frame_cache;
pub mod preview;
pub mod core_options;
pub mod controls;
pub mod emu_menu;
pub mod slots;
// The pause menu: the Disc row of multi-disc games, and Save and Load only where they do something.
pub mod disc;
pub mod pause_saves;
pub mod states;
pub mod save_trouble;
pub mod save_writer;
pub mod rewind;
pub mod hotkeys;
pub mod power;
pub mod extras;
pub mod run_ahead;
/// Pixel Path: the graded chain of a frame and its sound (pause menu, docs/CURATOR.md).
pub mod pixel_path;
pub mod pixel_path_page;
pub mod pixel_standalone;
/// Pixel Path over the running game: the strip pocket-core's HUD draws beside the picture.
pub mod pixel_live;
/// Over the game: Off, FPS, FPS + sensors or Pixel Path, one setting for what pocket-core draws over a game.
pub mod over_game;
pub mod overlay;
pub mod standalone;
pub mod emu_settings;
pub mod fallback;
pub mod emu_choice;
pub mod emu_choice_page;
pub mod cheats;
pub mod rumble;
pub mod ui_sound;
pub mod favorites;
pub mod marquee;
pub mod motion;
pub mod button_layout;
pub mod switcher;
/// The status corner at the top right: the time, a Wi-Fi symbol while connected, the battery.
pub mod status_corner;
pub mod battery;
pub mod battery_estimate;
pub mod battery_history;
pub mod battery_games;
pub mod manual;
pub mod sleep;
/// Settings → System → Sleep & power: the times of sleep and what the power button does.
pub mod sleep_settings;
/// Switch off after sleeping (L3 of sleep).
pub mod auto_off;
/// Low-battery warning and power-off (review B12 / SOL-05).
pub mod low_battery;
pub mod powerkey;
pub mod power_menu;
pub mod timezone;
pub mod system_ctl;
/// ROCKNIX's system.cfg: every write under its lock (`/tmp/.system.cfg.lock`), atomically.
pub mod system_cfg;
/// The way back when pocket-ui won't start: launcher exit codes, safe mode's rescue page, the dialog before EmulationStation.
pub mod rescue;
pub mod display;
pub mod display_out;
/// Which handheld this is: the Mini V2 (tested) or one of its experimental SM8250 siblings (docs/DEVICES.md).
pub mod device;
#[cfg(target_os = "linux")]
pub mod gamma;
pub mod night_light;
pub mod osd;
pub mod fn_keys;
pub mod mako_osd;
pub mod quick;
pub mod quick_paint;
pub mod quick_profiles;
pub mod fsutil;
pub mod settings;
pub mod settings_values;
pub mod shelf;
pub mod cabinet;
pub mod cabinet_art;
pub mod media;
pub mod quit;
pub mod fast_switch;
pub mod system_art;
pub mod ui_art;
pub mod systems_grid;
pub mod look_sample;
pub mod lastframe;
pub mod screenshot;
pub mod gamedb;
// Curator data (docs/CURATOR.md, layers B and C): feel tags and ROM file-name facts.
pub mod curatordb;
pub mod romfacts;
/// Games recognised by their checksum: the No-Intro name for covers, pictures and the game facts (`rom_id`).
pub mod rom_id;
/// PlayStation discs recognised by the serial on the disc: the Redump name for covers, pictures and the game facts
/// (`disc_serial`, the disc half of `rom_id`).
pub mod disc_serial;
pub mod library_meta;
pub mod library_sync;
pub mod game_box;
pub mod game_info;
pub mod game_card;
pub mod consoles;
pub mod console_page;
// Credits & links on the console pages (X there): the data and its page.
pub mod console_credits;
pub mod credits_page;
pub mod wiki_text;
pub mod sync;
pub mod moonlight;
pub mod sync_page;
/// Save conflicts from Syncthing: found, tied to their games, one version kept and the others moved aside.
pub mod sync_conflicts;
pub mod sync_conflict_page;
pub mod auto_texts;
pub mod leds;
pub mod tailscale;
pub mod tailscale_page;
/// The web companion: the library, edits and uploads from a browser on the tailnet (docs/user/web-companion.md).
pub mod web;
pub mod web_page;
pub mod bluetooth;
pub mod bluetooth_page;
pub mod perf_fan;
pub mod osd_sensors;
pub mod fan_curve;
pub mod perf_fan_page;
pub mod curator;
pub mod curator_page;
// The picture side of Home's Curator, Favorites and pinned rows (docs/DESIGN.md "Home panels").
pub mod home_panel;
// Play history (docs/HISTORY.md): the log, its read API (also for the web companion) and the History pages.
pub mod play_log;
pub mod play_history;
pub mod history_page;
pub mod startup;
pub mod on_start;
pub mod launch_picture;
/// RetroAchievements: rcheevos (vendor/rcheevos) for pocket-core games, the account, the files pocket-ui reads.
pub mod achievements;
pub mod achievements_page;
/// Find games fast: the search over the whole library and the letter jump of the lists.
pub mod search;
pub mod search_page;
pub mod letters;
// Latency trace for scripts/latency.sh (POCKETUI_TRACE, off by default).
pub mod trace;
// Settings → Tools (ROCKNIX's tools and the Store's, without EmulationStation) and its page state.
pub mod timeline;
pub mod tools;
pub mod tools_page;
// Settings → PC streaming: pairing Moonlight on the device, the paired PCs, their games.
pub mod moonlight_page;
// Settings → Backup & reset: pocket-ui's settings and library notes to a dated folder, back, and reset.
pub mod backup;
pub mod backup_page;
// Settings → System → Storage: what takes the room on the card, the clean-ups, the low-space warning.
pub mod storage;
// Storage's Card check: read-only mount, the file system's error count and the kernel's log say something is wrong.
pub mod card_health;
// A Wi-Fi network that wants a sign-in in a browser (a hotel, a train): found by one plain request, said instead of "Check Wi-Fi".
pub mod net_check;
// A network with a sign-in page: the Wi-Fi page's sheet (other ways online, the sign-in helper).
pub mod portal_page;
// The sign-in helper: the sign-in page's form in pocket-ui's style, its requests, and the small HTML reader they use.
pub mod portal_form;
pub mod portal;
pub mod html_form;
// The Storage page: its rows, lists and what a press asks for (`app_storage.rs` paints it).
pub mod storage_page;
// The files of one game, its saves and states: what "Delete from the card" on the game card takes away.
pub mod game_files;
pub mod kept;
// Helpers for the unit tests (fake programs the code under test runs).
#[cfg(test)]
pub(crate) mod testutil;

/// The row above or below `sel` in a short menu of `n` rows, wrapping at the ends (Up from the top is the last row).
pub fn wrap_step(sel: usize, n: usize, up: bool) -> usize {
    match (n, up) {
        (0, _) => 0,
        (_, true) => (sel + n - 1) % n,
        (_, false) => (sel + 1) % n,
    }
}

#[cfg(test)]
mod wrap_tests {
    use super::wrap_step;

    /// The pause menu clamped while every other menu wrapped: Quit took 8 presses from Continue, now Up once.
    #[test]
    fn short_menus_wrap_at_both_ends() {
        assert_eq!(wrap_step(0, 7, true), 6, "Up from Continue reaches Quit");
        assert_eq!(wrap_step(6, 7, false), 0);
        assert_eq!((wrap_step(3, 7, true), wrap_step(3, 7, false)), (2, 4));
        assert_eq!(wrap_step(0, 1, true), 0);
        assert_eq!(wrap_step(0, 0, false), 0, "an empty menu doesn't divide by zero");
    }
}

