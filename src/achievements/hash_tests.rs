//! How a game is recognised on Android: the same hash as for the file itself when the game was opened without a copy
//! (a document's descriptor, reached by the alias `/proc/self/fd/N`: `platform::android_content::DirectGame`), inside a
//! zip, and as a disc (an ISO, a CUE sheet with its track file, an M3U playlist). A CHD can't be read: its test says so.

use super::hash_rom;
use crate::platform::android_content::DirectGame;
use crate::testutil::TestDir;
use std::fs::File;
use std::path::Path;

const SECTOR: usize = 2048;

/// One ISO 9660 directory record.
fn record(name: &[u8], extent: u32, size: u32, directory: bool) -> Vec<u8> {
    let mut r = vec![0u8; 33 + name.len() + usize::from(name.len() % 2 == 0)];
    r[0] = r.len() as u8;
    r[2..6].copy_from_slice(&extent.to_le_bytes());
    r[6..10].copy_from_slice(&extent.to_be_bytes());
    r[10..14].copy_from_slice(&size.to_le_bytes());
    r[14..18].copy_from_slice(&size.to_be_bytes());
    r[25] = if directory { 2 } else { 0 };
    r[28..30].copy_from_slice(&1u16.to_le_bytes());
    r[30..32].copy_from_slice(&1u16.to_be_bytes());
    r[32] = name.len() as u8;
    r[33..33 + name.len()].copy_from_slice(name);
    r
}

/// A small PlayStation disc as an ISO: SYSTEM.CNF names the boot file, which has a PS-X EXE header of two sectors.
fn psx_disc(serial: &str) -> Vec<u8> {
    let mut iso = vec![0u8; 24 * SECTOR];
    let (cnf_at, exe_at) = (21usize, 22usize);
    let cnf = format!("BOOT = cdrom:\\{serial};1\r\nTCB = 4\r\n");
    let pvd = &mut iso[16 * SECTOR..17 * SECTOR];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    let root = record(&[0], 20, SECTOR as u32, true);
    pvd[156..156 + root.len()].copy_from_slice(&root);
    let mut dir = Vec::new();
    dir.extend(record(&[0], 20, SECTOR as u32, true));
    dir.extend(record(&[1], 20, SECTOR as u32, true));
    dir.extend(record(b"SYSTEM.CNF;1", cnf_at as u32, cnf.len() as u32, false));
    dir.extend(record(format!("{serial};1").as_bytes(), exe_at as u32, 2 * SECTOR as u32, false));
    iso[20 * SECTOR..20 * SECTOR + dir.len()].copy_from_slice(&dir);
    iso[cnf_at * SECTOR..cnf_at * SECTOR + cnf.len()].copy_from_slice(cnf.as_bytes());
    let exe = &mut iso[exe_at * SECTOR..];
    exe[..8].copy_from_slice(b"PS-X EXE");
    // The size after the 2048-byte header, then the program's own bytes.
    exe[28..32].copy_from_slice(&(SECTOR as u32).to_le_bytes());
    for (i, b) in exe[SECTOR..2 * SECTOR].iter_mut().enumerate() {
        *b = (i * 13 + 5) as u8;
    }
    iso
}

/// The game the way Android opens it without a copy: the document's descriptor under its own name.
fn as_document(real: &Path, root: &Path, name: &str) -> DirectGame {
    DirectGame::new(File::open(real).unwrap(), root, name).unwrap()
}

#[test]
fn a_game_opened_without_a_copy_hashes_like_the_file_itself() {
    let d = TestDir::new("ra-hash-document");
    let rom: Vec<u8> = (0..65536u32).map(|i| (i * 7 + 3) as u8).collect();
    std::fs::write(d.join("Original.gba"), &rom).unwrap();
    let alias = as_document(&d.join("Original.gba"), &d.join("direct"), "Original.gba");
    assert!(std::fs::symlink_metadata(alias.path()).unwrap().file_type().is_symlink(), "an alias of the descriptor, no copy");
    let real = hash_rom(5, &d.join("Original.gba")).expect("the file");
    assert_eq!(hash_rom(5, alias.path()).as_deref(), Some(real.as_str()), "the document's alias");
    // An NES document: the 16-byte header stays out of the hash through the alias too.
    let mut nes = b"NES\x1a\x02\x01\0\0\0\0\0\0\0\0\0\0".to_vec();
    nes.extend_from_slice(&rom);
    std::fs::write(d.join("Game.nes"), &nes).unwrap();
    let alias = as_document(&d.join("Game.nes"), &d.join("direct"), "Game.nes");
    assert_eq!(hash_rom(7, alias.path()), hash_rom(7, &d.join("Game.nes")));
    assert!(hash_rom(7, alias.path()).is_some());
    // The same document under a name with spaces and brackets (a real file name on a card).
    let alias = as_document(&d.join("Game.nes"), &d.join("direct"), "Super Game (USA) [!].nes");
    assert_eq!(hash_rom(7, alias.path()), hash_rom(7, &d.join("Game.nes")));
}

#[test]
fn a_zipped_game_opened_without_a_copy_hashes_like_the_game_inside() {
    let d = TestDir::new("ra-hash-document-zip");
    let rom: Vec<u8> = (0..32768u32).map(|i| (i * 11 + 1) as u8).collect();
    std::fs::write(d.join("Game.gb"), &rom).unwrap();
    std::fs::write(d.join("Game.zip"), super::tests::zip_of("Game.gb", &rom)).unwrap();
    let alias = as_document(&d.join("Game.zip"), &d.join("direct"), "Game.zip");
    assert_eq!(hash_rom(4, alias.path()), hash_rom(4, &d.join("Game.gb")));
    assert!(hash_rom(4, alias.path()).is_some());
}

#[test]
fn a_disc_hashes_as_an_iso_a_cue_sheet_and_a_playlist_and_through_a_document() {
    let d = TestDir::new("ra-hash-disc");
    std::fs::write(d.join("Game.iso"), psx_disc("SLUS_000.01")).unwrap();
    let iso = hash_rom(12, &d.join("Game.iso")).expect("the disc is recognised by its boot file");
    // A document: the ISO opened without a copy.
    let alias = as_document(&d.join("Game.iso"), &d.join("direct"), "Game.iso");
    assert_eq!(hash_rom(12, alias.path()).as_deref(), Some(iso.as_str()), "the disc through its descriptor");
    // A CUE sheet with its track file next to it (the app keeps sets like this together).
    std::fs::write(d.join("Game.bin"), psx_disc("SLUS_000.01")).unwrap();
    std::fs::write(d.join("Game.cue"), "FILE \"Game.bin\" BINARY\r\n  TRACK 01 MODE1/2048\r\n    INDEX 01 00:00:00\r\n").unwrap();
    assert_eq!(hash_rom(12, &d.join("Game.cue")).as_deref(), Some(iso.as_str()), "the CUE sheet");
    // An M3U playlist of discs: the first disc stands for the game.
    std::fs::write(d.join("Game.m3u"), "Game.cue\n").unwrap();
    assert_eq!(hash_rom(12, &d.join("Game.m3u")).as_deref(), Some(iso.as_str()), "the playlist");
    // Another game's disc is another hash (the boot file's name is part of it).
    std::fs::write(d.join("Other.iso"), psx_disc("SLES_123.45")).unwrap();
    assert_ne!(hash_rom(12, &d.join("Other.iso")), Some(iso));
    // A track file that is missing gives no hash, never a hang or a wrong one.
    std::fs::write(d.join("Broken.cue"), "FILE \"Gone.bin\" BINARY\r\n  TRACK 01 MODE1/2048\r\n    INDEX 01 00:00:00\r\n").unwrap();
    assert_eq!(hash_rom(12, &d.join("Broken.cue")), None);
}

/// RetroAchievements' CHD support needs a CHD reader the host provides (libchdr in RetroArch); pocket-ui has none, on
/// Linux or Android. Such a disc is not recognised: the pause menu says so (`session`: the "Failed" message), the game
/// plays on. A CUE/BIN, ISO or M3U copy of the same disc is recognised.
#[test]
fn a_chd_disc_is_not_recognised_and_says_so_instead_of_hanging() {
    let d = TestDir::new("ra-hash-chd");
    let mut chd = b"MComprHD".to_vec();
    chd.extend_from_slice(&[0u8; 4096]);
    std::fs::write(d.join("Game.chd"), &chd).unwrap();
    let started = std::time::Instant::now();
    assert_eq!(hash_rom(12, &d.join("Game.chd")), None);
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert!(super::is_chd(&d.join("Game.chd")));
    assert!(super::is_chd(Path::new("/x/Game.CHD")) && !super::is_chd(Path::new("/x/Game.iso")));
}
