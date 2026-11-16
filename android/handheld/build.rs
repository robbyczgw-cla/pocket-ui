//! The handheld view's build script: the root build.rs (clock stamp, libretro log shim, rcheevos, each fresh by content)
//! with the crate root's paths (`../../src`, `../../vendor`). cc uses the NDK's compiler (CC_aarch64_linux_android).
#[path = "../../build.rs"]
mod root;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    root::build(std::path::Path::new("../.."));
}
