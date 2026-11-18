// Android library composition: shared UI/data modules and native Android adapters.
pub mod platform;
pub mod gfx;
pub mod i18n;
pub mod image;
pub mod pad;
pub mod players;
pub mod android_pads;
pub mod palette;
pub mod proc;
pub mod shelf;
pub mod system_art;
pub mod text;
pub mod text_fallback;
pub mod latinise;
pub mod type_scale;
pub mod text_size;
pub mod covers;
pub mod game_pictures;
pub mod rom_id;
/// "The case knows where you are" (`case_knows`, `case_knows_read`): the box's back paints the line here too.
pub mod case_knows;
pub mod case_knows_read;
pub mod case_knows_art;
pub mod disc_serial;
/// A game of several discs: the pause menu's Disc row (`disc`), Linux's.
pub mod disc;
pub mod archive;
pub mod fsutil;

pub mod gamedb;
pub mod curatordb;
pub mod romfacts;
pub mod library_meta;
pub mod curator;
pub mod civil_date;
pub mod connections;
pub mod curator_page;
pub mod cabinet;
pub mod cabinet_art;
pub mod share_shelf;
pub mod phone_shelves;
pub mod media;
pub mod tape_label;
pub mod title_block;
pub mod pause_head;
/// The pause menu's pages as painted on Linux and in Android's player: one painter for both (`pause_page`).
pub mod pause_page;
/// The Picture page's live preview (`preview`): its layout and painting, as Linux's.
pub mod preview;
/// The handhelds' pause menu for Android's player when the game came from the Handheld view (`pause_list`).
pub mod pause_list;
/// The Options page of that menu, laid out as Linux's (`pause_options`).
pub mod pause_options;
/// Fast-forward and rewind as pocket-core runs them (`play_speed`), its history (`rewind`) and HOME + R2 / L2 (`hotkeys`).
pub mod play_speed;
pub mod rewind;
pub mod hotkeys;
pub mod motion;
pub mod keyboard;
mod library_names;
pub mod library { pub use crate::library_names::*; }
pub mod settings {
    pub use crate::look_settings::*;
    pub use crate::text_size::TextSize;
}
mod look_settings;
pub mod audio;
pub mod ui_sound;

pub mod game_card_model;
pub mod rating;
pub mod play_time;
pub mod game_box;

pub mod kit;
/// What a frame offers the D-pad, for tests/nav_explorer.rs (off unless POCKETUI_NAV_PROBE is set).
pub mod nav_probe;
pub mod ui_status;
pub mod ui_art;
pub mod scaled_layout;
#[path = "touch_ui.rs"]
pub mod touch;
#[path = "platform/android_route.rs"]
pub mod audio_route;
pub mod resample;
pub mod search;

pub mod document_library;
pub mod document_check;
pub mod system_names;
pub mod game_check;
pub mod game_check_page;
pub mod game_files;

#[path = "device_model.rs"]
pub mod device;

pub mod wiki_text;

pub mod libretro;
pub mod controls;
pub mod button_layout_model;
pub mod controls_card;
pub mod core_options;
pub mod vfs;
pub mod slots;
pub mod state_file;
pub mod dsv;
pub mod softpatch;
pub mod save_trouble;
pub mod save_writer;
#[path="platform/android_hw_render.rs"]
pub mod hw_render;
/// Game rumble as pocket-core runs it (`rumble`: Options › Speed, rumble, fan › Rumble, the lease, the players' targets),
/// through the Activity's vibrators.
pub mod rumble;

pub mod frozen;
pub mod frozen_phone;
pub mod save_transfer;
pub mod save_copy;
pub mod save_media;
pub mod case_stickers;
pub mod shrink_wrap;
pub mod crt_on;
pub mod eject;
pub mod game_info;
pub mod game_card { pub use crate::game_card_model::*; }
pub mod recents { pub use crate::play_time::*; }
#[path = "cabinet_tv_art.rs"]
pub mod cabinet_tv;
pub mod font_style;
pub mod retro;
pub mod room_light;
pub mod room_deco;
pub mod room_props;
pub mod button_prompts;
pub mod cozy_extras;
pub mod insert_rig;
pub mod insert_scene;
/// Save's picture flying into its slot (`save_fill`), as Linux's pause menu shows it.
pub mod save_fill;
pub mod playful;
pub mod lived_in;
pub mod scene_check;
pub mod sticker_press;

pub mod where_was_i;
pub mod zoom;

pub mod layouts;
pub mod gamelist;
pub mod cheat_pack;
pub mod cheats;

// RetroAchievements in the APK's own player (docs/user/android-mvp.md): the same session, offline copy and pages as Linux.
pub mod achievements;
pub mod achievements_page;
/// The achievement unlock card, painted alike by pocket-core's HUD and the Android player.
pub mod unlock_card;

pub mod document_art;

// The same signed packages and safe installer as Linux; transport belongs to the Activity.
pub mod store;
pub mod untar;
pub mod android_cores;
/// Which way a game starts on Android (our player, a core to download, another app), for the Mobile and the handheld view.
pub mod play_route;
/// The Android games: apps the player calls games, one list for both views (`launcher.json` `android_games`).
pub mod android_games;
#[cfg(target_os = "android")]
pub mod android_core_run;
pub mod status_corner;
pub mod phone_corner;

pub mod update_hint;
pub mod report_redact;
pub mod report_phone;

// Linux's screen frames around the picture in the player (docs/user/android-mvp.md, Screen frame): the same drawing code and GL
// layer. The picture renderer's two small parts they use, and the battery's folder for the power light (read where the phone
// lets the app; steady green where not).
pub mod bezel;
pub mod frames;
pub mod frames_more;
pub mod frames_tv;
pub mod frame_cache;
pub mod frame_led;
pub mod video { pub use crate::video_basics::*; pub use crate::video_program::{Effect, Sharpness}; pub use crate::video_settings::{FrameMode, Orientation, VideoSettings}; }
pub mod video_settings;
/// The games' settings files as Linux keeps them (`game_settings`): the player reads and writes them.
pub mod game_settings;
/// Linux's game settings' table and Looks (`option_table`, `look`): the player's Picture page has Linux's rows.
pub mod option_table;
pub mod look;
/// Linux's picture shader (`shaders/video.frag`) and its effects' constants: the player draws every Look as pocket-core does.
pub mod video_program;
pub mod picture_fx;
pub mod battery {
    /// As Linux's `battery::dir`.
    pub fn dir() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var("POCKETUI_BATTERY").unwrap_or_else(|_| "/sys/class/power_supply/battery".into()))
    }
}

pub mod timeline;
pub mod timezone;
pub mod run_ahead;
pub mod storage { pub use crate::storage_space::*; }
