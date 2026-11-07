//! Shared pocket-ui library with target-owned composition.
pub mod trace;
pub mod state_policy;
pub mod audio_diagnostics;
pub mod audio_pw_stats;
#[cfg(target_os = "linux")]
include!("linux_modules.rs");
#[cfg(target_os = "android")]
include!("android_modules.rs");

#[cfg(all(test, target_os = "linux"))]
extern crate self as pocket_ui;
#[cfg(all(test, target_os = "linux"))]
mod android_02_tests;

#[cfg(all(test, target_os = "linux"))]
mod input_corpus_tests;
