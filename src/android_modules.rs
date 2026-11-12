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
pub mod type_scale;
pub mod text_size;
pub mod covers;
pub mod game_pictures;
pub mod rom_id;
pub mod disc_serial;
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
pub mod phone_shelves;
pub mod media;
pub mod title_block;
pub mod pause_head;
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
#[path="platform/android_rumble.rs"]
pub mod rumble;

pub mod frozen;
pub mod frozen_phone;
pub mod save_transfer;
pub mod save_copy;
pub mod save_media;
pub mod case_stickers;
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
pub mod playful;

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
pub mod status_corner;
pub mod phone_corner;

pub mod update_hint;
pub mod report_redact;
pub mod report_phone;
