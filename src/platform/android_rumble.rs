//! Rumble stays unavailable until Android's per-controller vibrator path is connected.
use std::ffi::{c_uint,c_void};
pub unsafe fn environment(cmd:c_uint,_:*mut c_void)->Option<bool> {(cmd==23).then_some(false)}
/// `players` seats controllers for the rumble too; Android has no per-controller rumble yet, so these go nowhere.
pub const PORTS: usize = 4;
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Target {
    #[default]
    None,
    Motor,
    Node(String),
}
pub fn set_targets(_: [Target; PORTS]) {}
