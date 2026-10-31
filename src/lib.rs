//! Shared pocket-ui library with target-owned composition.
pub mod trace;
#[cfg(target_os = "linux")]
include!("linux_modules.rs");
#[cfg(target_os = "android")]
include!("android_modules.rs");
