//! OS adapters. Keep target selection here instead of in shared UI and matching code.
#[cfg(any(target_os = "linux", target_os = "emscripten"))]
#[path = "linux_http.rs"]
pub(crate) mod http;

#[cfg(any(target_os = "linux", target_os = "emscripten"))]
mod linux;
#[cfg(any(target_os = "linux", target_os = "emscripten"))]
pub use linux::*;
#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "android")]
pub use android::{data_dir, is_speaker, sink_volume, wiki_json, console_buttons, screenshot_hint};
#[cfg(target_os = "android")]
#[path = "android_http.rs"]
pub(crate) mod http;

#[cfg(any(target_os = "linux", target_os = "emscripten"))]
#[cfg_attr(not(target_os = "emscripten"), path = "linux_audio.rs")] #[cfg_attr(target_os = "emscripten", path = "../../web/web_audio.rs")]
pub(crate) mod audio_backend;
#[cfg(target_os="android")]
#[path="android_audio.rs"]
pub(crate) mod audio_backend;
