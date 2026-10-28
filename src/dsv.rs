//! A DraStic save taken over (docs/user/coming-from.md "DraStic", docs/HOST.md "NDS saves"): DraStic keeps a game's
//! save as `<game>.dsv`, pocket-core's melonDS core as the raw `<game>.sav`. When a game starts with a `.dsv` next to
//! it and no `.sav` (and no `.srm` either), the raw save is written to `<game>.sav` once. The `.dsv` stays as it is.
//!
//! The format is DeSmuME's (`BackupDevice::ensure` / `readFooter` in desmume/src/mc.cpp, which DraStic's saves share):
//! the raw save, then a 122-byte footer. The raw save is the file without its footer.
//!
//! ```text
//! 82 bytes  "|<--Snip above here to create a raw sav by excluding this DeSmuME savedata footer:"
//!  4 bytes  size of the data actually written          (u32, little endian)
//!  4 bytes  padSize: the size the save was padded to   (the raw save's length)
//!  4 bytes  save memory type
//!  4 bytes  address size
//!  4 bytes  save memory size
//!  4 bytes  version, 0
//! 16 bytes  "|-DESMUME SAVE-|"
//! ```
//!
//! - A file with that ending whose `padSize` is the file's length minus the footer is cut at `padSize`. DeSmuME
//!   itself checks the same two things (the cookie, `padSize` against the file's length).
//! - A file without the cookie is taken as the raw save, if its length is a size a DS game's save has.
//! - Sizes: 512 bytes up to 8 MiB, powers of two (EEPROM 512 B to 64 KiB, flash 128 KiB to 1 MiB, and the 2 to 8 MiB
//!   of the biggest cards). Anything else is refused with a line in the log and left alone: a wrong file is never
//!   written as the player's save.
//! - An existing `.sav` is never touched, nor is the `.dsv` ever changed. A `.srm` (RetroArch's name, which the host
//!   carries over to `.sav`, `libretro::Core::use_save_extension`) counts as a save already there.

use std::path::Path;

const FOOTER_TEXT: &[u8] = b"|<--Snip above here to create a raw sav by excluding this DeSmuME savedata footer:";
const COOKIE: &[u8] = b"|-DESMUME SAVE-|";
/// The footer: its text, six `u32` fields and the cookie.
pub const FOOTER_LEN: usize = FOOTER_TEXT.len() + 6 * 4 + COOKIE.len();
/// A DS game's save: from 512 bytes to 8 MiB.
const MIN_SAVE: usize = 512;
const MAX_SAVE: usize = 8 << 20;

/// What `take_over` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing to do: no `.dsv`, or a `.sav` (or `.srm`) is there already.
    Nothing,
    /// `<game>.sav` was written from the `.dsv`: its length in bytes and whether the `.dsv` had DeSmuME's footer.
    Took { bytes: usize, footer: bool },
    /// The `.dsv` is not a DS save we can use; nothing was written. The reason is for the log.
    Refused(String),
}

/// The raw save inside a `.dsv`'s bytes, or why there is none.
pub fn raw_save(dsv: &[u8]) -> Result<(&[u8], bool), String> {
    let sized = |raw: &[u8]| {
        if (MIN_SAVE..=MAX_SAVE).contains(&raw.len()) && raw.len().is_power_of_two() {
            Ok(())
        } else {
            Err(format!("{} bytes is not the size of a DS save (512 bytes to 8 MiB, a power of two)", raw.len()))
        }
    };
    if dsv.len() >= FOOTER_LEN && dsv.ends_with(COOKIE) {
        let foot = &dsv[dsv.len() - FOOTER_LEN..];
        let raw = &dsv[..dsv.len() - FOOTER_LEN];
        if !foot.starts_with(FOOTER_TEXT) {
            return Err("it ends like a DeSmuME save but its footer is damaged".into());
        }
        let field = |i: usize| {
            let at = FOOTER_TEXT.len() + 4 * i;
            u32::from_le_bytes([foot[at], foot[at + 1], foot[at + 2], foot[at + 3]])
        };
        if field(5) != 0 {
            return Err(format!("the footer is version {}, only version 0 is known", field(5)));
        }
        if field(1) as usize != raw.len() {
            return Err(format!("the footer says {} bytes of save, the file holds {}", field(1), raw.len()));
        }
        sized(raw)?;
        return Ok((raw, true));
    }
    sized(dsv)?;
    Ok((dsv, false))
}

/// Writes `sav` from `dsv` when `dsv` exists and neither `sav` nor `srm` does.
pub fn take_over(dsv: &Path, sav: &Path, srm: &Path) -> Outcome {
    // Case-insensitive on the card's side too: a copy from Android may be `Game.DSV`.
    let dsv = if dsv.is_file() { dsv.to_path_buf() } else { dsv.with_extension("DSV") };
    if !dsv.is_file() || sav.exists() || srm.exists() {
        return Outcome::Nothing;
    }
    let name = dsv.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = match std::fs::read(&dsv) {
        Ok(b) => b,
        Err(e) => return Outcome::Refused(format!("{name}: {e}")),
    };
    let (raw, footer) = match raw_save(&bytes) {
        Ok(r) => r,
        Err(why) => return Outcome::Refused(format!("{name}: {why}")),
    };
    match crate::fsutil::write_atomic(sav, raw) {
        Ok(()) => Outcome::Took { bytes: raw.len(), footer },
        Err(e) => Outcome::Refused(format!("{name}: could not write {}: {e}", sav.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// A save of `size` bytes with a pattern a mix-up would show.
    fn raw(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i * 7 + i / 251) as u8).collect()
    }

    /// DeSmuME's footer after `raw`, laid out as its `BackupDevice::ensure` writes it.
    fn with_footer(raw: &[u8], version: u32) -> Vec<u8> {
        let mut v = raw.to_vec();
        v.extend_from_slice(FOOTER_TEXT);
        // written, padded, type (EEPROM 2), address size, memory size, version
        for f in [raw.len() as u32, raw.len() as u32, 2, 2, raw.len() as u32, version] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        v.extend_from_slice(COOKIE);
        v
    }

    fn game(dir: &TestDir, dsv: Option<&[u8]>) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let (d, s, r) = (dir.join("Game.dsv"), dir.join("Game.sav"), dir.join("Game.srm"));
        if let Some(b) = dsv {
            std::fs::write(&d, b).unwrap();
        }
        (d, s, r)
    }

    #[test]
    fn the_footer_is_122_bytes_as_desmume_writes_it() {
        assert_eq!(FOOTER_LEN, 122);
        assert_eq!(FOOTER_TEXT.len(), 82);
        assert_eq!(COOKIE.len(), 16);
    }

    #[test]
    fn a_dsv_with_the_footer_becomes_the_raw_sav_and_the_dsv_stays() {
        let dir = TestDir::new("dsv-footer");
        let save = raw(8192);
        let file = with_footer(&save, 0);
        let (d, s, r) = game(&dir, Some(&file));
        assert_eq!(take_over(&d, &s, &r), Outcome::Took { bytes: 8192, footer: true });
        assert_eq!(std::fs::read(&s).unwrap(), save, "the footer is cut off");
        assert_eq!(std::fs::read(&d).unwrap(), file, "the .dsv is untouched");
    }

    #[test]
    fn a_dsv_without_the_footer_is_taken_as_it_is() {
        let dir = TestDir::new("dsv-plain");
        let save = raw(512 * 1024);
        let (d, s, r) = game(&dir, Some(&save));
        assert_eq!(take_over(&d, &s, &r), Outcome::Took { bytes: 512 * 1024, footer: false });
        assert_eq!(std::fs::read(&s).unwrap(), save);
        assert_eq!(std::fs::read(&d).unwrap(), save);
    }

    #[test]
    fn every_ds_save_size_is_taken_with_and_without_footer() {
        let mut size = MIN_SAVE;
        while size <= MAX_SAVE {
            let save = raw(size);
            assert_eq!(raw_save(&save).unwrap(), (save.as_slice(), false), "{size}");
            let with = with_footer(&save, 0);
            assert_eq!(raw_save(&with).unwrap(), (save.as_slice(), true), "{size}");
            size *= 2;
        }
    }

    #[test]
    fn a_wrong_size_is_refused_and_nothing_is_written() {
        for size in [0, 1, 100, 511, 1000, 8193, 3 << 20, 16 << 20] {
            let dir = TestDir::new("dsv-size");
            let (d, s, r) = game(&dir, Some(&raw(size)));
            assert!(matches!(take_over(&d, &s, &r), Outcome::Refused(_)), "{size} bytes plain");
            assert!(!s.exists(), "{size}");
            if size > 0 && size < 100_000 {
                let (d, s, r) = game(&dir, Some(&with_footer(&raw(size), 0)));
                assert!(matches!(take_over(&d, &s, &r), Outcome::Refused(_)), "{size} bytes with footer");
                assert!(!s.exists());
            }
        }
    }

    #[test]
    fn a_damaged_footer_is_refused() {
        let save = raw(8192);
        // padSize that is not the file's length, a version we don't know, a cookie without the text before it
        let mut wrong_pad = with_footer(&save, 0);
        let at = save.len() + FOOTER_TEXT.len() + 4;
        wrong_pad[at..at + 4].copy_from_slice(&4096u32.to_le_bytes());
        let mut text = with_footer(&save, 0);
        text[save.len() + 3] = b'X';
        for (what, file) in [("padSize", wrong_pad), ("version", with_footer(&save, 1)), ("text", text)] {
            let dir = TestDir::new("dsv-damaged");
            let (d, s, r) = game(&dir, Some(&file));
            assert!(matches!(take_over(&d, &s, &r), Outcome::Refused(_)), "{what}");
            assert!(!s.exists(), "{what}");
        }
    }

    #[test]
    fn an_existing_sav_or_srm_is_never_overwritten() {
        for existing in ["sav", "srm"] {
            let dir = TestDir::new("dsv-existing");
            let (d, s, r) = game(&dir, Some(&with_footer(&raw(8192), 0)));
            let mine = dir.join(format!("Game.{existing}"));
            std::fs::write(&mine, b"the player's own save").unwrap();
            assert_eq!(take_over(&d, &s, &r), Outcome::Nothing, "{existing}");
            assert_eq!(std::fs::read(&mine).unwrap(), b"the player's own save");
            assert_eq!(s.exists(), existing == "sav");
        }
    }

    #[test]
    fn no_dsv_is_nothing_and_the_second_start_does_not_do_it_again() {
        let dir = TestDir::new("dsv-none");
        let (d, s, r) = game(&dir, None);
        assert_eq!(take_over(&d, &s, &r), Outcome::Nothing);
        assert!(!s.exists());
        std::fs::write(&d, raw(8192)).unwrap();
        assert!(matches!(take_over(&d, &s, &r), Outcome::Took { .. }));
        assert_eq!(take_over(&d, &s, &r), Outcome::Nothing, "the .sav is there now");
    }

    #[test]
    fn an_uppercase_ending_from_android_is_found() {
        let dir = TestDir::new("dsv-upper");
        let save = raw(8192);
        std::fs::write(dir.join("Game.DSV"), &save).unwrap();
        let (d, s, r) = (dir.join("Game.dsv"), dir.join("Game.sav"), dir.join("Game.srm"));
        assert!(matches!(take_over(&d, &s, &r), Outcome::Took { .. }));
        assert_eq!(std::fs::read(&s).unwrap(), save);
    }
}
