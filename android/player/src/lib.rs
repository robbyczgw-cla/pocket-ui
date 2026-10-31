//! pocket-core's private Android process. No winit/softbuffer/NativeActivity dependency is linked here.
#![cfg(target_os = "android")]
#[path = "../../src/player.rs"]
mod player;
