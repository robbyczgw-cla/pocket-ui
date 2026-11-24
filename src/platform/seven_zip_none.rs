//! The browser demo's `seven_zip` (docs/WEB-DEMO.md): its library has no 7z games, and web/Cargo.toml no 7z reader. A
//! 7z is refused as one packed in a way pocket-core can't open.

use std::io::{Read, Seek};
use std::path::Path;

pub fn is_7z(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("7z"))
}

pub fn extract_rom(path: &Path, _wanted: &[&str]) -> Result<(String, Vec<u8>), String> {
    Err(crate::archive::message("packing", path.file_name().and_then(|n| n.to_str()).unwrap_or("The 7z file")))
}

pub fn rom_crc(_path: &Path, _wanted: &[&str]) -> Option<(u32, u64)> {
    None
}

pub fn rom_crc_in(_f: &mut (impl Read + Seek), _wanted: &[&str]) -> Option<(u32, u64)> {
    None
}

pub fn refusal_in(_f: &mut (impl Read + Seek), _name: &str, _wanted: &[&str], _only_cut_off: bool) -> Option<String> {
    None
}
