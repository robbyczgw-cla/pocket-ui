// Linux library composition, retaining the existing ROCKNIX module API.
/// OS adapters used at I/O boundaries; UI and matching logic stay shared.
pub mod platform;
pub mod control;
pub mod proc;
pub mod frame;
/// The rows a frame changed, for the Brick's partial upload (`pocket-fb`).
pub mod damage;
/// The canvas as the bytes of an Android window (R, G, B, X), one vector pass.
pub mod rgbx;
/// The handheld view's window on Android: a canvas kept between frames, painted before the buffer is locked (`rgbx`).
#[cfg(target_os = "android")]
pub mod native_surface;
pub mod scaled_layout;
/// Save slots drawn on the save medium of their era: memory card, cartridge, a plain row (`save_media`).
pub mod save_media;
/// Save made visible in the pause menu: the saved picture flies from the preview into its slot, Cozy and Motion Full (`save_fill`).
pub mod save_fill;
pub mod crash;
pub mod logs;
pub mod net_log;
pub mod shutdown_log;
pub mod idle;
pub mod gfx;
pub mod image;
pub mod gamelist;
pub mod layouts;
pub mod gamelist_page;
pub mod gallery;
pub mod library;
pub mod library_cache;
pub mod pad;
/// Couch co-op: which controller is player 1 to 4 in pocket-core games, and each player's input.
pub mod players;
pub mod android_pads;
pub mod players_page;
pub mod zoom;
pub mod touch;
pub mod frozen;
pub mod kids;
pub mod big_shelf;
// Android's files for Frozen games. On Linux pocket-core uses its set-aside (a moment that isn't loaded goes into a free
// slot, never under a name the next quit removes), and the gate tests its rules.
pub mod frozen_phone;
pub mod save_transfer;
pub mod save_copy;
// A phone's Games page: on Linux only so the gate tests its rules.
#[cfg(test)]
pub(crate) mod phone_shelves;
pub mod recents;
pub mod text;
pub mod this_game_page;
/// The boot screen pocket-splash draws until sway takes over (docs/BOOT-SPLASH.md).
pub mod splash;
/// The first frame after the Pocket starts fades in from black (docs/BOOT-SPLASH.md).
pub mod boot_fade;
/// Settings → Playing → Console boot screen: a fresh start runs the console's own boot screen (the player's BIOS file).
pub mod boot_screen;
pub mod crt_on;
pub mod lcd_on;
pub mod eject;
/// A start that failed: the medium comes back out, the case goes back on the shelf, one friendly sentence (`start_failed`).
pub mod start_failed;
pub mod cozy_extras;
pub mod insert_rig;
pub mod insert_scene;
pub mod playful;
pub mod lived_in;
pub mod shelf_space;
pub mod crate_scene;
pub mod scene_check;
pub mod sticker_press;
pub mod text_fallback;
pub mod latinise;
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
pub mod report_redact;
pub mod report_phone;
pub mod report;
pub mod palette;
pub mod kit;
/// Where a page's main pictures stand, for tests/brick_proportions.rs (off unless POCKETUI_SHARES is set).
pub mod shares;
/// What a frame offers the D-pad, for tests/nav_explorer.rs (off unless POCKETUI_NAV_PROBE is set).
pub mod nav_probe;
/// Where the UI is, one line in /tmp/pocket-ui.status for agents and test helpers (docs/DEVICE-TESTS.md).
pub mod ui_status;
pub mod appearance;
pub mod setup;
// The feature registry, its cards and their previews (first start, Settings → Tour, "New in this version").
pub mod features;
pub mod feature_previews;
pub mod covers;
pub mod dir_index;
/// RomM as a source of games: pairing, the server's list on the shelf, a game's download at its first start (docs/ROMM.md).
pub mod romm;
pub mod romm_net;
pub mod romm_page;
pub mod auto_covers;
pub mod game_pictures;
pub mod picture_cycle;
pub mod picture_loader;
pub mod ports;
pub mod ports_card;
pub mod port_files;
pub mod port_perf;
pub mod port_perf_page;
pub mod audio;
pub mod audio_route;
pub mod h700_volume;
pub mod resample;
pub mod clock_check;
pub mod manual_time;
pub mod libretro;
#[cfg_attr(any(target_os = "emscripten", target_os = "android"), path = "platform/android_hw_render.rs")] pub mod hw_render;
pub mod options;
pub mod look;
pub mod j2me_java;
pub mod j2me_keys;
pub mod dpad_stick_rocknix;
pub mod j2me_size;
pub mod starter;
pub mod store;
pub mod update;
/// The quiet "a newer pocket-ui is ready" notice, once for each version (`app_update_hint.rs`).
pub mod update_hint;
/// Whether a new ROCKNIX system still has what pocket-ui needs (OS updates, docs/UPDATES.md).
pub mod os_compat;
pub mod untar;
pub mod video;
/// The picture's shader, shared with Android's player (`video_program`).
pub mod video_program;
/// Picture settings as values, shared with Android's player (`video_settings`).
pub mod video_settings;
/// A game's settings files, shared with Android's player (`game_settings`).
pub mod game_settings;
/// The game settings' table, shared with Android's player (`option_table`).
pub mod option_table;
pub mod picture_fx;
#[cfg(target_os = "linux")]
pub mod headless_gl;
pub mod hakux;
pub mod heap_trim;
pub mod hostsys;
pub mod notice;
pub mod archive;
/// The game inside a .7z (`archive` hands it over). The browser demo has no 7z games and no 7z reader.
#[cfg_attr(target_os = "emscripten", path = "platform/seven_zip_none.rs")] pub mod seven_zip;
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
pub mod button_layout_model;
pub mod controls_card;
pub mod emu_menu;
pub mod slots;
pub mod state_file;
// The pause menu: the Disc row of multi-disc games, and Save and Load only where they do something.
pub mod disc;
pub mod pause_saves;
pub mod states;
pub mod save_trouble;
pub mod save_writer;
pub mod rewind;
pub mod play_speed;
pub mod hotkeys;
/// RetroArch's HOME shortcuts on ROCKNIX: pocket-core's keys and switches, written before each RetroArch start.
pub mod ra_hotkeys;
pub mod ra_state;
/// The pause menu's list of a RetroArch game's HOME shortcuts.
pub mod shortcut_list;
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
/// The same over-the-game pieces put into the picture by the CPU, where no GPU draws them (dArkOS, a Brick without GL).
pub mod soft_hud;
/// The achievement unlock card, painted alike by pocket-core's HUD and the Android player.
pub mod unlock_card;
pub mod standalone;
pub mod emu_settings;
pub mod fallback;
pub mod first_game;
pub mod h700_gba;
pub mod core_prewarm;
pub mod emu_choice;
pub mod emu_choice_page;
pub mod cheats;
pub mod cheat_pack;
pub mod cheat_download;
pub mod cheat_data;
pub mod extras_page;
pub mod optional_data;
pub mod rumble;
pub mod ui_sound;
pub mod favorites;
pub mod marquee;
pub mod motion;
pub mod button_layout;
/// Settings → Controls → Test buttons: what each connected controller does, with the remaps shown (`app_test_buttons.rs` paints it).
pub mod button_test;
pub mod button_prompts;
pub mod font_style;
pub mod retro;
pub mod switcher;
/// The status corner at the top right: the time, a Wi-Fi symbol while connected, the battery.
pub mod status_corner;
pub mod battery;
pub mod battery_curve;
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
/// The heat guard (the TrimUI Brick's own card): over the CPU zone's limit the game is saved and the device switched off.
#[cfg(feature = "brick")]
pub mod heat;
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
pub mod room_light;
pub mod tv_glow;
pub mod room_deco;
pub mod room_props;
/// The window on the room's wall: the sky now and the season on its sill (Cozy extras → Window).
pub mod room_window;
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
pub mod share_shelf;
// Stickers on a game's case (Completed, 100 %, Favourite, Playing now, Game of the year): kept, painted on fronts and spines.
pub mod case_stickers;
pub mod shrink_wrap;
// The CRT TV on the Cabinet showing the selected game's screenshot (its own thread, cached, crossfaded).
pub mod cabinet_tv;
// The Cabinet's TV plays the real game, and A goes on with it (`showcase`).
pub mod showcase;
pub mod media;
pub mod tape_label;
pub mod title_block;
pub mod pause_head;
/// The pause menu's pages as painted on Linux and in Android's player: one painter for both (`pause_page`).
pub mod pause_page;
/// The handhelds' pause menu for Android's player when the game came from the Handheld view (`pause_list`).
pub mod pause_list;
/// The Options page of that menu, laid out as Linux's (`pause_options`).
pub mod pause_options;
pub mod quit;
pub mod exit_guard;
pub mod fast_switch;
pub mod system_art;
pub mod ui_art;
pub mod systems_grid;
pub mod system_notes;
pub mod look_sample;
pub mod lastframe;
pub mod screenshot;
pub mod gamedb;
// Curator data (docs/CURATOR.md, layers B and C): feel tags and ROM file-name facts.
pub mod curatordb;
pub mod romfacts;
/// Games recognised by their checksum: the No-Intro name for covers, pictures and the game facts (`rom_id`).
pub mod rom_id;
/// "The case knows where you are": one line read from a game's own battery save, checked by the game's checksum
/// (`case_knows`), and the files, cache and thread behind it (`case_knows_read`).
pub mod case_knows;
pub mod case_knows_read;
/// Photos of big moments in the game's case, taken when its save shows a step (`album`).
pub mod album;
/// Painting it in the open case (`album_paint`).
pub mod album_paint;
/// The moment a photo is taken, over the running game: flash, the photo out of the screen's edge (`album_eject`).
pub mod album_eject;
/// Pictures for it from the player's own ROM (`case_knows_art`): the team's icons and the badges.
pub mod case_knows_art;
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
/// Connections: a game's developer, publisher, series and genre lead to the player's other games (docs/CURATOR.md).
pub mod civil_date;
pub mod connections;
pub mod credits_page;
pub mod wiki_text;
pub mod sync;
pub mod moonlight;
pub mod sync_page;
/// Settings > Connections > Pockets: the page of Pocket-to-Pocket sync (docs/SYNC-OWN.md).
pub mod pocket_sync_page;
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
/// "Send from your phone": a small upload server that exists only while its page is open (Settings → Games → Add games).
pub mod phone_upload;
pub mod phone_upload_page;
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
/// Android's handheld view: where the games are and the Mobile view's key of each (android/handheld/app_android.rs).
pub mod android_handheld;
/// The Android games: apps the player calls games, one list for both views (`launcher.json` `android_games`).
pub mod android_games;
// "Where was I?": the card over a moment continued after a long break.
pub mod where_was_i;
/// RetroAchievements: rcheevos (vendor/rcheevos) for pocket-core games, the account, the files pocket-ui reads.
pub mod achievements;
pub mod achievements_page;
/// Find games fast: the search over the whole library and the letter jump of the lists.
pub mod search;
pub mod search_page;
pub mod letters;
// Latency trace for scripts/latency.sh (POCKETUI_TRACE, off by default).
// Settings → Tools (ROCKNIX's tools and the Store's, without EmulationStation) and its page state.
pub mod timeline;
pub mod tools;
pub mod game_check;
pub mod game_check_page;
pub mod tools_page;
// Settings → PC streaming: pairing Moonlight on the device, the paired PCs, their games.
pub mod moonlight_page;
// Settings → Backup & reset: pocket-ui's settings and library notes, and the saves and states, to a dated folder and back; reset.
pub mod backup;
pub mod backup_page;
pub mod backup_saves;
// Settings → System → Storage: what takes the room on the card, the clean-ups, the low-space warning.
pub mod storage;
// Storage's Card check: read-only mount, the file system's error count and the kernel's log say something is wrong.
pub mod card_health;
// A games card in the second slot: it came or went (the shelf reads again, or says so). Sleeps in poll, no wake-ups.
pub mod games_card;
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


mod library_names;
mod look_settings;
mod text_size;


pub mod game_card_model;
pub mod rating;
pub mod play_time;

pub mod touch_ui;

pub mod document_library;
pub mod document_check;
pub mod system_names;

pub mod device_model;
#[cfg(feature = "brick")]
pub mod soft_screen;
#[cfg(feature = "brick")]
pub mod brick_profile;
#[cfg(feature = "brick")]
pub mod darkos;
#[cfg(feature = "brick")]
pub mod brick_gl;
#[cfg(feature = "brick")]
pub mod brick_gpu;
#[cfg(feature = "brick")]
pub mod brick_gles;
#[cfg(feature = "brick")]
pub mod brick_picture;
#[cfg(feature = "brick")]
pub mod brick_hw;

/// Backward-compatible pocket-core FIFO decoding.
pub mod core_command;

/// Read-only pocket-core play state, using the shared v1 status writer.
pub mod core_status;
pub mod document_art;

pub mod android_cores;
/// Which way a game starts on Android (our player, a core to download, another app), for the Mobile and the handheld view.
pub mod play_route;
/// Android's first start, shared by both views: games folders of other apps, emulator apps, missing BIOS.
pub mod android_first_start;
/// "How cozy should it be?": the first start's cozy levels (Plain, Cozy, Everything) and their page.
pub mod cozy_level;
#[cfg(target_os = "android")]
pub mod android_core_run;
pub mod play_route_cfg;
pub mod phone_corner;
/// Android's handheld view: the volume and brightness rows act on Android's media volume and this app's window.
pub mod android_controls;
