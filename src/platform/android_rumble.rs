//! Rumble stays unavailable until Android's per-controller vibrator path is connected.
use std::ffi::{c_uint,c_void};
pub unsafe fn environment(cmd:c_uint,_:*mut c_void)->Option<bool> {(cmd==23).then_some(false)}
