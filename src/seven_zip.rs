//! Reads one ROM out of a .7z, as `archive` does out of a .zip, for the cores that take ROM data but don't open archives
//! (the R36S's card comes with every GBA game as a 7z). The game is picked among the files the same way
//! (`archive::pick_name`: the one file of the system's extensions, readmes and pictures not counted), within the same
//! limits, and a 7z that is not to be played is refused with the same sentences (`archive::message`, `is_refusal`).
//!
//! LZMA, LZMA2 and PPMd with 7-Zip's filters, through sevenz-rust2 (pure Rust). A password, or another method (BZip2,
//! Deflate, Zstandard), is refused: "packed in a way pocket-core can't open". The header (at the end of the file, often
//! packed itself) names the files with their sizes and CRC-32s: `rom_crc` and `refusal_in` read only that. In a solid 7z
//! the files share one stream, so the bytes in front of the game are unpacked on the way to it (and not kept).

use std::io::{Read, Seek};
use std::path::Path;

use sevenz_rust2::{Archive, BlockDecoder, EncoderMethod, Error, Password};

use crate::archive::{message, MAX_ROM, MAX_ZIP};

/// The largest dictionary or model a 7z may ask for, in KiB: refused before it is allocated ("packed in a way pocket-core
/// can't open"). 7-Zip makes the dictionary no bigger than the files when they are smaller, so a game's is at most about
/// the game ([`MAX_ROM`]); 7-Zip's strongest setting asks for 64 MB.
const MEMORY_KB: usize = 256 << 10;
/// The same on a Pocket with about 1 GB and no swap (the H700s, the RK3326s, the Brick): 7-Zip's strongest setting and
/// a little over, so every 7z of one game still opens. 256 MiB there, beside the game's 64 MiB, the core and pocket-ui,
/// could end pocket-core for want of memory (review 21.11.2026).
const SMALL_MEMORY_KB: usize = 80 << 10;
/// Below this much memory (`MemTotal`, KiB) a Pocket gets [`SMALL_MEMORY_KB`]: a 1 GB board shows about 0.9 GB.
const SMALL_DEVICE_KB: u64 = 1536 << 10;

/// The limit for a Pocket with `mem_total_kb` of memory; unknown: the larger one.
fn memory_limit_kb(mem_total_kb: Option<u64>) -> usize {
    match mem_total_kb {
        Some(total) if total < SMALL_DEVICE_KB => SMALL_MEMORY_KB,
        _ => MEMORY_KB,
    }
}

/// [`memory_limit_kb`] of this Pocket, from sysinfo(2) (what `/proc/meminfo` calls `MemTotal`), asked once.
fn memory_limit() -> usize {
    static LIMIT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *LIMIT.get_or_init(|| {
        // SAFETY: a plain C struct of numbers, all zero is a valid value.
        let mut info: libc::sysinfo = unsafe { std::mem::zeroed() };
        // SAFETY: sysinfo(2) fills the struct it is given and touches nothing else.
        let total = (unsafe { libc::sysinfo(&mut info) } == 0).then(|| info.totalram as u64 * info.mem_unit.max(1) as u64 / 1024);
        memory_limit_kb(total.filter(|kb| *kb > 0))
    })
}

/// The methods this reader unpacks (sevenz-rust2 without its default features, with PPMd).
const METHODS: [&[u8]; 13] = [
    EncoderMethod::ID_COPY,
    EncoderMethod::ID_LZMA,
    EncoderMethod::ID_LZMA2,
    EncoderMethod::ID_PPMD,
    EncoderMethod::ID_BCJ_X86,
    EncoderMethod::ID_BCJ_ARM,
    EncoderMethod::ID_BCJ_ARM64,
    EncoderMethod::ID_BCJ_ARM_THUMB,
    EncoderMethod::ID_BCJ_PPC,
    EncoderMethod::ID_BCJ_IA64,
    EncoderMethod::ID_BCJ_SPARC,
    EncoderMethod::ID_BCJ_RISCV,
    EncoderMethod::ID_DELTA,
];

/// Whether `path` is a 7z by its name: what `archive` hands over here.
pub fn is_7z(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("7z"))
}

/// What the player is told when the 7z could not be read: a refusal (`archive::is_refusal`) for a fault of the file, a
/// plain error for one of the card (it could not be read at all).
fn why(e: Error, name: &str) -> String {
    match e {
        Error::PasswordRequired | Error::MaybeBadPassword(_) => message("password", name),
        // Without sevenz-rust2's AES feature an encrypted 7z (also one with encrypted names) is an unknown method.
        Error::UnsupportedCompressionMethod(m) if m.contains("AES") => message("password", name),
        Error::UnsupportedCompressionMethod(_) | Error::Unsupported(_) | Error::ExternalUnsupported | Error::MaxMemLimited { .. } | Error::UnsupportedVersion { .. } => {
            message("packing", name)
        }
        Error::ChecksumVerificationFailed => message("checksum", name),
        Error::Io(e, _) | Error::FileOpen(e, _) => match e.get_ref().and_then(|inner| inner.downcast_ref::<Error>()) {
            Some(Error::ChecksumVerificationFailed) => message("checksum", name),
            _ if matches!(e.kind(), std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::InvalidData | std::io::ErrorKind::Other) => message("incomplete", name),
            _ => format!("{name}: {e}"),
        },
        // A bad signature (no 7z at all), a header that doesn't read as one, sizes that don't add up.
        _ => message("incomplete", name),
    }
}

/// The 7z's header: its files with their names, sizes and CRC-32s. Nothing of the files is unpacked.
fn header(f: &mut (impl Read + Seek)) -> Result<Archive, Error> {
    Archive::read_with_memory_limit(f, &Password::empty(), memory_limit())
}

/// The game among the 7z's files, by its place in `a.files` (`archive::pick_name`, as for a zip).
fn choose(a: &Archive, wanted: &[&str], name: &str) -> Result<usize, String> {
    // Anti-items (marks of deleted files in an update archive) are no files.
    let (at, names): (Vec<usize>, Vec<String>) = a
        .files
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_anti_item)
        .map(|(i, e)| {
            let n = e.name.replace('\\', "/");
            (i, if e.is_directory { format!("{n}/") } else { n })
        })
        .unzip();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    crate::archive::pick_name(&names, wanted, name).map(|i| at[i])
}

/// Whether this reader can unpack the file at `at`: every method of its stream is one of [`METHODS`]. True for a file
/// without a stream (an empty one).
fn unpacks(a: &Archive, at: usize) -> bool {
    let Some(block) = a.stream_map.file_block_index.get(at).copied().flatten() else { return true };
    a.blocks.get(block).is_some_and(|b| b.coders.iter().all(|c| METHODS.contains(&c.encoder_method_id())))
}

fn shown_name(path: &Path) -> &str {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("The 7z file")
}

/// The ROM inside the 7z at `path` (`archive::extract_rom` for a 7z): its name inside the archive (without folders) and
/// its bytes, checked against the CRC-32 the 7z states. Refused with a sentence for the player (`archive::is_refusal`):
/// several games or none, a password, a method this reader lacks, a damaged or cut-off file.
pub fn extract_rom(path: &Path, wanted: &[&str]) -> Result<(String, Vec<u8>), String> {
    extract_rom_within(path, wanted, memory_limit())
}

/// [`extract_rom`] with at most `memory_kb` for the dictionary or model ([`memory_limit_kb`]).
fn extract_rom_within(path: &Path, wanted: &[&str], memory_kb: usize) -> Result<(String, Vec<u8>), String> {
    let name = shown_name(path);
    // The same bound as a zip's, checked before a byte is read; the file itself is read as it is unpacked.
    crate::archive::check_size(path, MAX_ZIP)?;
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let a = header(&mut f).map_err(|e| why(e, name))?;
    let at = choose(&a, wanted, name)?;
    let entry = &a.files[at];
    if a.is_file_encrypted(at) {
        return Err(message("password", name));
    }
    if !unpacks(&a, at) {
        return Err(message("packing", name));
    }
    if entry.size > MAX_ROM as u64 {
        return Err(format!("{} is too big to unpack", entry.name));
    }
    let file_name = entry.name.rsplit(['/', '\\']).next().unwrap_or(&entry.name).to_string();
    let Some(block) = a.stream_map.file_block_index[at] else {
        // An empty file: no stream to unpack. The core says what it makes of it, as for an empty file in a zip.
        return Ok((file_name, Vec::new()));
    };
    // Only the game's block is unpacked, up to the game; the files before it in a solid block are read and dropped.
    let mut got: Option<Result<Vec<u8>, String>> = None;
    let no_password = Password::empty();
    let decoder = BlockDecoder::new_with_memory_limit(1, memory_kb, block, &a, &no_password, &mut f);
    let walked = decoder.for_each_entries(&mut |e, r| {
        if !std::ptr::eq(e, entry) {
            return Ok(true);
        }
        let mut data = Vec::new();
        got = Some(match data.try_reserve_exact(e.size as usize) {
            Err(_) => Err(format!("{} is too big to unpack", e.name)),
            // Reading to its end makes the reader check the CRC-32 (an error if it doesn't match).
            Ok(()) => (&mut *r).take(MAX_ROM as u64 + 1).read_to_end(&mut data).map(|_| data).map_err(|err| why(Error::from(err), name)),
        });
        Ok(false)
    });
    let data = match (got, walked) {
        (Some(result), _) => result?,
        (None, Err(e)) => return Err(why(e, name)),
        (None, Ok(_)) => return Err(message("incomplete", name)),
    };
    if data.len() as u64 != entry.size || (entry.has_crc && crc32fast::hash(&data) != entry.crc as u32) {
        return Err(message("checksum", name));
    }
    Ok((file_name, data))
}

/// The CRC-32 and unpacked size of the game in the 7z at `path` as its header states them (`archive::rom_crc` for a 7z):
/// nothing is unpacked. None when `extract_rom` would refuse it for its files (several games, none) or the header doesn't
/// read, or states no CRC.
pub fn rom_crc(path: &Path, wanted: &[&str]) -> Option<(u32, u64)> {
    rom_crc_in(&mut crate::background_io::open(path).ok()?, wanted)
}

pub(crate) fn rom_crc_in(file: &mut (impl Read + Seek), wanted: &[&str]) -> Option<(u32, u64)> {
    let a = header(file).ok()?;
    let e = &a.files[choose(&a, wanted, "").ok()?];
    e.has_crc.then_some((e.crc as u32, e.size))
}

/// The sentence the 7z would be refused with, from its header alone (`archive::refusal_in` for a 7z; the Game check):
/// the game inside is not unpacked. With `only_cut_off` just a damaged or cut-off file counts. None: nothing against it,
/// or the file could not be read (the card, not the 7z).
pub fn refusal_in(f: &mut (impl Read + Seek), name: &str, wanted: &[&str], only_cut_off: bool) -> Option<String> {
    let a = match header(f) {
        Ok(a) => a,
        Err(e) => {
            let said = why(e, name);
            let damaged = said == message("incomplete", name);
            return (crate::archive::is_refusal(&said) && (damaged || !only_cut_off)).then_some(said);
        }
    };
    if only_cut_off {
        return None;
    }
    let at = match choose(&a, wanted, name) {
        Ok(at) => at,
        Err(why) => return Some(why),
    };
    if a.is_file_encrypted(at) {
        return Some(message("password", name));
    }
    (!unpacks(&a, at)).then(|| message("packing", name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::is_refusal;
    use crate::testutil::TestDir;

    /// The "game" of testdata/7z (make.sh): a pattern, no real ROM.
    fn fake_rom() -> Vec<u8> {
        (0..4096u32).map(|i| ((i * 31 + i / 7) & 255) as u8).collect()
    }

    /// The test 7z `file` of testdata/7z, written into `d` under `as_name`.
    fn put(d: &TestDir, file: &str, as_name: &str) -> std::path::PathBuf {
        let bytes = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/7z").join(file)).unwrap();
        let p = d.join(as_name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn the_game_comes_out_of_a_solid_7z_past_the_readme_and_of_one_stream_per_file() {
        let d = TestDir::new("7z-game");
        for file in ["game.7z", "separate.7z"] {
            let p = put(&d, file, file);
            let (name, data) = extract_rom(&p, &["gba"]).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(name, "Pocket Demo (World).gba", "{file}");
            assert!(data == fake_rom(), "{file}: the game's bytes");
            // No list of extensions (achievements): the one file that is no readme.
            assert_eq!(extract_rom(&p, &[]).unwrap().0, "Pocket Demo (World).gba", "{file}");
        }
    }

    #[test]
    fn lzma_and_ppmd_open_too() {
        let d = TestDir::new("7z-methods");
        for file in ["lzma.7z", "ppmd.7z"] {
            let (name, data) = extract_rom(&put(&d, file, file), &["gba"]).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!((name.as_str(), data == fake_rom()), ("Demo.gba", true), "{file}");
        }
    }

    #[test]
    fn several_games_no_game_a_password_and_other_packing_are_refused_as_a_zips() {
        let d = TestDir::new("7z-refusals");
        let said = |file: &str| extract_rom(&put(&d, file, file), &["gba"]).unwrap_err();
        for (file, starts) in [
            ("two.7z", "two.7z holds 2 game files."),
            ("text.7z", "There is no game file in text.7z."),
            ("locked.7z", "locked.7z is password-protected"),
            ("locked-names.7z", "locked-names.7z is password-protected"),
            ("bzip2.7z", "bzip2.7z is packed in a way pocket-core can't open."),
        ] {
            let m = said(file);
            assert!(m.starts_with(starts) && is_refusal(&m), "{file}: {m}");
        }
    }

    #[test]
    fn a_damaged_or_cut_off_7z_is_refused_and_a_big_game_is_not_unpacked() {
        let d = TestDir::new("7z-damaged");
        let good = std::fs::read(put(&d, "game.7z", "game.7z")).unwrap();
        // A byte of the packed stream (after the 32-byte signature header) changed.
        let mut bad = good.clone();
        bad[40] ^= 0x55;
        std::fs::write(d.join("Bad.7z"), &bad).unwrap();
        let m = extract_rom(&d.join("Bad.7z"), &["gba"]).unwrap_err();
        assert!(m.starts_with("Bad.7z is damaged") && is_refusal(&m), "{m}");
        // A copy that stopped on the way: the header at the end is missing.
        std::fs::write(d.join("Cut.7z"), &good[..good.len() / 2]).unwrap();
        let m = extract_rom(&d.join("Cut.7z"), &["gba"]).unwrap_err();
        assert_eq!(m, message("incomplete", "Cut.7z"));
        // 65 MB unpacked: over MAX_ROM, said before anything is unpacked; not a refusal (RetroArch may still take it).
        let m = extract_rom(&put(&d, "big.7z", "big.7z"), &["gba"]).unwrap_err();
        assert_eq!(m, "Big.gba is too big to unpack");
        assert!(!is_refusal(&m));
        // A file over MAX_ZIP is not opened at all.
        let huge = d.join("Huge.7z");
        std::fs::File::create(&huge).unwrap().set_len(MAX_ZIP + 1).unwrap();
        assert!(extract_rom(&huge, &["gba"]).unwrap_err().starts_with("Huge.7z is too big to load"));
    }

    /// 21.11.2026 (review): a 7z could ask for 256 MiB of dictionary on every Pocket, also on the 1 GB ones without swap,
    /// beside the game's 64 MiB, the core and pocket-ui. There it is 80 MiB now (7-Zip's strongest setting and a little
    /// over); a Pocket with more memory keeps 256 MiB, and every 7z of the test set opens on both.
    #[test]
    fn a_pocket_with_1_gb_refuses_a_7z_that_asks_for_too_much_memory_and_opens_the_rest() {
        let gb = |n: f64| Some((n * (1 << 20) as f64) as u64);
        // MemTotal of a 1 GB board (H700, RK3326, Brick), of the Mini V2 and of a desktop; and none (unknown).
        let small = memory_limit_kb(gb(0.95));
        assert_eq!(small, SMALL_MEMORY_KB);
        assert!(small >= (64 << 10) + 1024, "7-Zip's strongest setting (a 64 MB dictionary) must still open");
        for big in [gb(3.6), gb(15.5), None] {
            assert_eq!(memory_limit_kb(big), MEMORY_KB);
        }
        let d = TestDir::new("7z-memory");
        let p = put(&d, "big-dictionary.7z", "big-dictionary.7z");
        let m = extract_rom_within(&p, &["gba"], small).unwrap_err();
        assert_eq!(m, message("packing", "big-dictionary.7z"), "a 128 MiB dictionary on a 1 GB Pocket");
        let (name, data) = extract_rom_within(&p, &["gba"], MEMORY_KB).unwrap();
        assert_eq!((name.as_str(), data == fake_rom()), ("Demo.gba", true), "the same file where there is memory for it");
        for file in ["game.7z", "separate.7z", "lzma.7z", "ppmd.7z"] {
            assert!(extract_rom_within(&put(&d, file, file), &["gba"], small).is_ok(), "{file} on a 1 GB Pocket");
        }
    }

    #[test]
    fn the_crc_comes_from_the_header_and_only_for_one_game() {
        let d = TestDir::new("7z-crc");
        let rom = fake_rom();
        assert_eq!(rom_crc(&put(&d, "game.7z", "game.7z"), &["gba"]), Some((crc32fast::hash(&rom), rom.len() as u64)));
        assert_eq!(rom_crc(&put(&d, "separate.7z", "separate.7z"), &[]), Some((crc32fast::hash(&rom), rom.len() as u64)));
        assert_eq!(rom_crc(&put(&d, "two.7z", "two.7z"), &["gba"]), None);
        assert_eq!(rom_crc(&d.join("missing.7z"), &["gba"]), None);
    }

    #[test]
    fn the_game_check_sees_the_refusals_from_the_header() {
        let d = TestDir::new("7z-check");
        let check = |file: &str, only_cut_off: bool| {
            let p = put(&d, file, file);
            refusal_in(&mut std::fs::File::open(&p).unwrap(), file, &["gba"], only_cut_off)
        };
        assert_eq!(check("game.7z", false), None);
        assert_eq!(check("ppmd.7z", false), None);
        assert_eq!(check("two.7z", false).as_deref(), Some(message("several", "two.7z").as_str()));
        assert_eq!(check("locked.7z", false).as_deref(), Some(message("password", "locked.7z").as_str()));
        assert_eq!(check("locked-names.7z", false).as_deref(), Some(message("password", "locked-names.7z").as_str()));
        assert_eq!(check("bzip2.7z", false).as_deref(), Some(message("packing", "bzip2.7z").as_str()));
        // An arcade set is many files by design: only a cut-off file counts there.
        assert_eq!(check("two.7z", true), None);
        let good = std::fs::read(d.join("game.7z")).unwrap();
        let mut cut = std::io::Cursor::new(good[..good.len() / 2].to_vec());
        assert_eq!(refusal_in(&mut cut, "Cut.7z", &["gba"], true), Some(message("incomplete", "Cut.7z")));
    }
}
