//! Shared pocket-ui library with target-owned composition.
pub mod trace;

/// The public contact address, the only place in the code that spells it (report.rs, report_phone.rs and the Android help use it). A macro, so `concat!` can put it into the
/// constants below and into `android`'s help text. Changing it also means the translated lines in `assets/i18n/*.tsv`
/// and the docs/site pages that name it (`contact_address_is_the_same_everywhere` lists what is left).
#[macro_export]
macro_rules! contact_address { () => { "robby@robbyczesany.com" }; }
pub const CONTACT: &str = contact_address!();

pub mod core_pacing;
pub mod fps_line;
/// Options → Picture → Resolution, the same on Linux and in Android's player (`render_scale`).
pub mod render_scale;
/// Earlier's on/off and its starting value per console, the same on Linux and in Android's player (`timeline` uses it).
pub mod timeline_policy;
pub mod frame_clock;
pub mod frame_dump;
pub mod frame_pace;
pub mod video_basics;
pub mod state_policy;
pub mod signed_data;
#[path = "update/channel.rs"]
pub mod update_channel;

pub mod update_page;
#[cfg(any(target_os = "android", test))]
#[path = "platform/android_pcm.rs"]
mod android_pcm;
#[cfg(any(target_os = "android", test))]
#[path = "platform/android_audio_timing.rs"]
mod android_audio_timing;
pub mod audio_diagnostics;
pub mod audio_pw_stats;
// and the browser demo over its stand-ins (docs/WEB-DEMO.md), and Android's handheld view (android/handheld/Cargo.toml)
#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
include!("linux_modules.rs");
#[cfg(all(target_os = "android", not(feature = "handheld")))]
include!("android_modules.rs");

#[cfg(all(test, target_os = "linux"))]
extern crate self as pocket_ui;
#[cfg(all(test, target_os = "linux"))]
mod android_02_tests;
#[cfg(all(test, target_os = "linux"))]
mod android_network_rule_tests;
#[cfg(all(test, target_os = "linux"))]
mod android_resolution_tests;

#[cfg(all(test, target_os = "linux"))]
mod case_knows_fuzz_tests;

#[cfg(test)]
mod case_knows_rpg_tests;

#[cfg(test)]
mod case_knows_more_tests;

#[cfg(all(test, target_os = "linux"))]
mod android_eject_tests;

#[cfg(all(test, target_os = "linux"))]
mod pad_axes_tests;

#[cfg(all(test, target_os = "linux"))]
mod input_corpus_tests;

#[cfg(all(test, target_os = "linux"))]
mod android_gles_tests;

#[cfg(all(test, target_os = "linux"))]
mod android_video_tests;

/// Shared play evidence for film, labels and discovery.
pub mod play_evidence;
