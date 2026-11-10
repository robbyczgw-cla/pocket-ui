//! Shared pocket-ui library with target-owned composition.
pub mod trace;
pub mod frame_pace;
pub mod state_policy;
pub mod audio_diagnostics;
pub mod audio_pw_stats;
#[cfg(any(target_os = "linux", target_os = "emscripten"))] // and the browser demo over its stand-ins (docs/WEB-DEMO.md)
include!("linux_modules.rs");
#[cfg(target_os = "android")]
include!("android_modules.rs");

#[cfg(all(test, target_os = "linux"))]
extern crate self as pocket_ui;
#[cfg(all(test, target_os = "linux"))]
mod android_02_tests;

#[cfg(all(test, target_os = "linux"))]
mod input_corpus_tests;
