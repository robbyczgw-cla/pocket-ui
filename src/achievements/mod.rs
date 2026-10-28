//! RetroAchievements for games on pocket-core, through the official rcheevos library (vendor/rcheevos, MIT; built by
//! build.rs, glued in src/rc_shim.c).
//!
//! - [`account`]: the user name and token in `<data>/retroachievements.json` (0600). Settings > RetroAchievements signs
//!   in once with the password ([`api::login`]); only the token is kept.
//! - [`session`]: pocket-core's rc_client for the running game. The game starts at once; signing in, hashing the ROM and
//!   loading the game's achievements happen on threads of their own, as do all network calls (curl, [`http`]). The game
//!   thread only calls `rc_client_do_frame` after each emulated frame, which reads the game's memory through the core's
//!   memory map (`libretro::Core::achievement_memory`).
//! - [`offline`]: what the last online session learnt (the game's achievements, the unlocks) is kept, so a game started
//!   without network still counts; unlocks the server hasn't confirmed are kept in `pending.json` and sent later.
//! - [`snapshot`]: the files pocket-ui reads: the running game's list for the pause menu, and a tally per game for the
//!   game card and the console page ([`lookup`] fills it in for a game not played yet).
//! - [`chime`]: the unlock sound, mixed into the game's own sound when UI sounds are on.
//!
//! Softcore only: hardcore would forbid loading states, and instant resume is a state load (docs/user/achievements.md).

pub mod account;
pub mod api;
pub mod chime;
pub mod consoles;
pub mod ffi;
pub mod game;
pub mod http;
pub mod lookup;
pub mod offline;
pub mod session;
pub mod snapshot;

use std::ffi::{c_char, CString};
use std::path::Path;

/// The directory under the data directory with everything achievements keep (tallies, the offline copy, badges).
pub fn dir(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("achievements")
}

/// RetroAchievements' hash of the game in `rom` for `console`: the file itself, or the ROM inside a zip (the first
/// file; arcade and DOS zips are hashed as they are), patched when a patch is next to it. None when it can't be read
/// or hashed (`.7z`, a broken zip, a patch that doesn't apply).
pub fn hash_rom(console: u32, rom: &Path) -> Option<String> {
    let ext = rom.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    if ext == "7z" {
        return None;
    }
    let by_content = !consoles::hashes_file_name(console) && !consoles::hashes_zip(console);
    let unpack = ext == "zip" && by_content;
    // A patched game (`softpatch`) is hashed as it is played, as RetroArch hashes it: RetroAchievements knows some
    // translations and hacks by that hash, and the original's hash is never sent for a patched game. A patch that
    // doesn't apply gives no hash (the game doesn't start either).
    let patched = if by_content { crate::softpatch::patched_content(rom) } else { None };
    let (name, data) = if let Some(p) = patched {
        p.ok()?
    } else if unpack {
        let (name, data) = crate::archive::extract_rom(rom, &[]).ok()?;
        (rom.parent().unwrap_or(Path::new("")).join(name), data)
    } else {
        (rom.to_path_buf(), Vec::new())
    };
    let path = CString::new(name.as_os_str().as_encoded_bytes()).ok()?;
    let mut out = [0u8; 33];
    let ok = unsafe {
        ffi::pocket_rc_hash(
            out.as_mut_ptr() as *mut c_char,
            console,
            path.as_ptr(),
            if data.is_empty() { std::ptr::null() } else { data.as_ptr() },
            data.len(),
        )
    };
    let hash = String::from_utf8_lossy(&out[..32]).into_owned();
    (ok != 0 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn scratch(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("ra-hash-{name}"))
    }

    /// A zip with one stored file, as `archive` reads them.
    fn zip_of(name: &str, data: &[u8]) -> Vec<u8> {
        let crc = {
            let mut c = 0xffff_ffffu32;
            for b in data {
                c ^= *b as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
                }
            }
            !c
        };
        let mut z = Vec::new();
        let local = |z: &mut Vec<u8>| {
            z.extend_from_slice(&[0x50, 0x4b, 3, 4, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            z.extend_from_slice(&crc.to_le_bytes());
            z.extend_from_slice(&(data.len() as u32).to_le_bytes());
            z.extend_from_slice(&(data.len() as u32).to_le_bytes());
            z.extend_from_slice(&(name.len() as u16).to_le_bytes());
            z.extend_from_slice(&[0, 0]);
            z.extend_from_slice(name.as_bytes());
            z.extend_from_slice(data);
        };
        local(&mut z);
        let cd = z.len() as u32;
        z.extend_from_slice(&[0x50, 0x4b, 1, 2, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&crc.to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&[0; 12]);
        z.extend_from_slice(&0u32.to_le_bytes());
        z.extend_from_slice(name.as_bytes());
        let cd_len = z.len() as u32 - cd;
        z.extend_from_slice(&[0x50, 0x4b, 5, 6, 0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend_from_slice(&cd_len.to_le_bytes());
        z.extend_from_slice(&cd.to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        z
    }

    #[test]
    fn roms_hash_like_retroarch_also_inside_a_zip() {
        let d = scratch("rom");
        // A Game Boy ROM is hashed whole: the MD5 of the file.
        let rom: Vec<u8> = (0..32768u32).map(|i| (i * 7 + 3) as u8).collect();
        std::fs::write(d.join("Game.gb"), &rom).unwrap();
        let gb = hash_rom(4, &d.join("Game.gb")).unwrap();
        assert_eq!(gb.len(), 32);
        std::fs::write(d.join("Game.zip"), zip_of("Game.gb", &rom)).unwrap();
        assert_eq!(hash_rom(4, &d.join("Game.zip")).as_deref(), Some(gb.as_str()), "the zip's ROM, not the zip");
        // An NES ROM's 16-byte iNES header is left out of the hash.
        let mut nes = b"NES\x1a\x02\x01\0\0\0\0\0\0\0\0\0\0".to_vec();
        nes.extend_from_slice(&rom);
        std::fs::write(d.join("Game.nes"), &nes).unwrap();
        std::fs::write(d.join("Bare.nes"), &rom).unwrap();
        assert_eq!(hash_rom(7, &d.join("Game.nes")), hash_rom(7, &d.join("Bare.nes")));
        assert_eq!(hash_rom(4, &d.join("missing.gb")), None);
        assert_eq!(hash_rom(4, &d.join("Game.7z")), None);
    }

    /// A patched game (`softpatch`) is hashed as it is played, as RetroArch hashes it; never as the original.
    #[test]
    fn a_patched_game_hashes_as_patched_and_a_refused_patch_gives_none() {
        let d = scratch("patched");
        let rom: Vec<u8> = (0..32768u32).map(|i| (i * 7 + 3) as u8).collect();
        std::fs::write(d.join("Game.gb"), &rom).unwrap();
        let original = hash_rom(4, &d.join("Game.gb")).unwrap();
        // "HACK" at 0x100 by IPS; the same bytes in a file of their own hash the same.
        std::fs::write(d.join("Game.ips"), b"PATCH\x00\x01\x00\x00\x04HACKEOF").unwrap();
        let mut patched = rom.clone();
        patched[0x100..0x104].copy_from_slice(b"HACK");
        std::fs::write(d.join("Prepatched.gb"), &patched).unwrap();
        let hash = hash_rom(4, &d.join("Game.gb")).unwrap();
        assert_ne!(hash, original);
        assert_eq!(Some(hash.clone()), hash_rom(4, &d.join("Prepatched.gb")));
        std::fs::write(d.join("Game.zip"), zip_of("Game.gb", &rom)).unwrap();
        assert_eq!(hash_rom(4, &d.join("Game.zip")), Some(hash), "a zipped game with the zip's patch");
        // A patch that doesn't apply: no hash at all (the game doesn't start either).
        std::fs::write(d.join("Game.ips"), b"PATCH\x00\x01").unwrap();
        assert_eq!(hash_rom(4, &d.join("Game.gb")), None);
    }
}
