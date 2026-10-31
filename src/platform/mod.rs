//! OS adapters. Keep target selection here instead of in shared UI and matching code.
#[cfg(target_os = "linux")]
#[path = "linux_http.rs"]
pub(crate) mod http;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "android")]
pub use android::{data_dir, is_speaker, sink_volume, wiki_json, console_buttons, screenshot_hint};
#[cfg(target_os = "android")]
#[path = "android_http.rs"]
pub(crate) mod http;

#[cfg(target_os="linux")]
#[path="linux_audio.rs"]
pub(crate) mod audio_backend;
#[cfg(target_os="android")]
#[path="android_audio.rs"]
pub(crate) mod audio_backend;
