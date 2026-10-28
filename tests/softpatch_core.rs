//! Soft-patching through `libretro::Core::load` (`softpatch`), against a stub core built from C here that keeps what it
//! was handed: the patched bytes for a core that takes data, a patched copy in /tmp for one that opens the file itself,
//! the save under the ROM's own name, and no start at all when the patch doesn't fit.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pocket_ui::libretro::Core;
use pocket_ui::softpatch;

const STUB: &str = r#"
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
struct info { const char *name, *version, *exts; bool need_fullpath, block_extract; };
struct av { unsigned bw, bh, mw, mh; float aspect; double fps, rate; };
struct game { const char *path; const void *data; size_t size; const char *meta; };
static unsigned char sram[8];
static char got_path[4096];
static unsigned char got_data[64], file_bytes[64];
static size_t got_size;
static long file_len = -1;
static int loads;
void retro_set_environment(void *f) {}
void retro_set_video_refresh(void *f) {}
void retro_set_audio_sample(void *f) {}
void retro_set_audio_sample_batch(void *f) {}
void retro_set_input_poll(void *f) {}
void retro_set_input_state(void *f) {}
void retro_set_controller_port_device(unsigned port, unsigned device) {}
unsigned retro_api_version(void) { return 1; }
void retro_init(void) {}
void retro_deinit(void) {}
void retro_get_system_info(struct info *i) { memset(i, 0, sizeof *i); i->name = "stub"; i->exts = "gb"; i->need_fullpath = FULLPATH; }
void retro_get_system_av_info(struct av *a) { memset(a, 0, sizeof *a); a->bw = a->mw = 160; a->bh = a->mh = 144; a->fps = 60; a->rate = 48000; }
bool retro_load_game(const struct game *g) {
    loads++;
    strncpy(got_path, g->path, sizeof got_path - 1);
    got_size = g->size;
    if (g->data && g->size <= sizeof got_data) memcpy(got_data, g->data, g->size);
    /* What a core that opens the file itself would read. */
    FILE *f = fopen(g->path, "rb");
    file_len = -1;
    if (f) { file_len = (long)fread(file_bytes, 1, sizeof file_bytes, f); fclose(f); }
    memcpy(sram, "SAVED!!!", 8);
    return true;
}
void retro_unload_game(void) {}
void retro_reset(void) {}
void retro_run(void) {}
void *retro_get_memory_data(unsigned id) { return id == 0 ? sram : NULL; }
size_t retro_get_memory_size(unsigned id) { return id == 0 ? sizeof sram : 0; }
size_t retro_serialize_size(void) { return 0; }
bool retro_serialize(void *d, size_t s) { return false; }
bool retro_unserialize(const void *d, size_t s) { return false; }
int stub_loads(void) { return loads; }
const char *stub_path(void) { return got_path; }
size_t stub_size(void) { return got_size; }
const unsigned char *stub_data(void) { return got_data; }
long stub_file_len(void) { return file_len; }
const unsigned char *stub_file(void) { return file_bytes; }
"#;

/// One test at a time: the stub's globals and pocket-core's per-thread host state.
static ONE: Mutex<()> = Mutex::new(());

/// A folder with the stub core (taking data, or `fullpath`) and room for a game; removed when the `Setup` is dropped.
struct Setup {
    _guard: tempfile::TempDir,
    dir: PathBuf,
    core: PathBuf,
}

impl Setup {
    fn new(name: &str, fullpath: bool) -> Setup {
        let guard = tempfile::Builder::new().prefix(&format!("pocketui-softpatch-core-{name}-")).tempdir().unwrap();
        let dir = guard.path().to_path_buf();
        std::fs::create_dir_all(dir.join("roms")).unwrap();
        std::fs::write(dir.join("stub.c"), STUB).unwrap();
        let core = dir.join("stub_libretro.so");
        let out = std::process::Command::new("cc")
            .args(["-shared", "-fPIC", &format!("-DFULLPATH={}", fullpath as u8), "-o"])
            .arg(&core)
            .arg(dir.join("stub.c"))
            .output()
            .expect("the soft-patching tests build a stub core and need a C compiler (cc)");
        assert!(out.status.success(), "cc: {}", String::from_utf8_lossy(&out.stderr));
        Setup { _guard: guard, dir, core }
    }

    fn roms(&self) -> PathBuf {
        self.dir.join("roms")
    }

    fn load(&self, rom: &Path) -> Result<Core, String> {
        Core::load(&self.core, rom, &self.dir, &self.roms(), &[])
    }

    /// The stub's record of what it was handed; the same library, opened again (dlopen shares it).
    fn seen(&self) -> Seen {
        unsafe {
            let lib = libloading::Library::new(&self.core).unwrap();
            let loads: libloading::Symbol<unsafe extern "C" fn() -> i32> = lib.get(b"stub_loads").unwrap();
            let path: libloading::Symbol<unsafe extern "C" fn() -> *const std::ffi::c_char> = lib.get(b"stub_path").unwrap();
            let size: libloading::Symbol<unsafe extern "C" fn() -> usize> = lib.get(b"stub_size").unwrap();
            let data: libloading::Symbol<unsafe extern "C" fn() -> *const u8> = lib.get(b"stub_data").unwrap();
            let file_len: libloading::Symbol<unsafe extern "C" fn() -> i64> = lib.get(b"stub_file_len").unwrap();
            let file: libloading::Symbol<unsafe extern "C" fn() -> *const u8> = lib.get(b"stub_file").unwrap();
            let n = size().min(64);
            let f = file_len();
            Seen {
                loads: loads(),
                path: PathBuf::from(std::ffi::CStr::from_ptr(path()).to_string_lossy().into_owned()),
                data: std::slice::from_raw_parts(data(), n).to_vec(),
                file: (f >= 0).then(|| std::slice::from_raw_parts(file(), f as usize).to_vec()),
            }
        }
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[derive(Debug)]
struct Seen {
    loads: i32,
    path: PathBuf,
    data: Vec<u8>,
    file: Option<Vec<u8>>,
}

fn ips(offset: usize, bytes: &[u8]) -> Vec<u8> {
    let mut p = b"PATCH".to_vec();
    p.extend_from_slice(&[(offset >> 16) as u8, (offset >> 8) as u8, offset as u8, 0, bytes.len() as u8]);
    p.extend_from_slice(bytes);
    p.extend_from_slice(b"EOF");
    p
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ONE.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn a_core_that_takes_data_gets_the_patched_bytes_and_the_save_keeps_the_roms_name() {
    let _l = lock();
    let s = Setup::new("data", false);
    let rom = s.roms().join("Game (USA).gb");
    std::fs::write(&rom, b"ORIGINAL").unwrap();
    std::fs::write(s.roms().join("Game (USA).ips"), ips(0, b"PATCHED")).unwrap();
    let core = s.load(&rom).unwrap();
    let seen = s.seen();
    assert_eq!(seen.data, b"PATCHEDL", "the core sees the patched game");
    assert_eq!(seen.path, rom, "and the game's own path");
    assert_eq!(core.patches, ["Game (USA).ips"], "for Pixel Path");
    drop(core);
    assert_eq!(std::fs::read(&rom).unwrap(), b"ORIGINAL", "the file on the SD card is never changed");
    // As RetroArch: the save has the ROM file's name, patched or not.
    assert_eq!(std::fs::read(s.roms().join("Game (USA).srm")).unwrap(), b"SAVED!!!");
    let saves: Vec<_> = std::fs::read_dir(s.roms()).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "srm")).collect();
    assert_eq!(saves.len(), 1, "one save, no second name for the patched game");
}

#[test]
fn a_game_without_a_patch_is_handed_over_as_it_is() {
    let _l = lock();
    let s = Setup::new("plain", false);
    let rom = s.roms().join("Game.gb");
    std::fs::write(&rom, b"ORIGINAL").unwrap();
    std::fs::write(s.roms().join("Other Game.ips"), ips(0, b"X")).unwrap();
    let core = s.load(&rom).unwrap();
    assert_eq!(s.seen().data, b"ORIGINAL");
    assert!(core.patches.is_empty());
}

#[test]
fn a_core_that_opens_the_file_gets_a_patched_copy_that_goes_with_the_game() {
    let _l = lock();
    let s = Setup::new("fullpath", true);
    let rom = s.roms().join("Game.gb");
    std::fs::write(&rom, b"ORIGINAL").unwrap();
    std::fs::write(s.roms().join("Game.ips"), ips(4, b"FIX!")).unwrap();
    let core = s.load(&rom).unwrap();
    let seen = s.seen();
    assert!(seen.data.is_empty(), "no bytes: the core reads the file");
    assert_eq!(seen.path.file_name().unwrap(), "Game.gb", "the copy keeps the game's file name and ending");
    assert!(seen.path.starts_with(std::env::temp_dir()), "{}", seen.path.display());
    assert_eq!(seen.file.as_deref(), Some(&b"ORIGFIX!"[..]), "what the core read is patched");
    assert!(seen.path.exists(), "kept while the game runs");
    drop(core);
    assert!(!seen.path.exists(), "removed after the game");
    assert!(!seen.path.parent().unwrap().exists(), "with its folder");
    assert!(s.roms().join("Game.srm").exists(), "the save is next to the ROM, under its name, not in /tmp");
}

#[test]
fn a_patch_for_another_version_stops_the_start_and_says_why() {
    let _l = lock();
    let s = Setup::new("refused", false);
    let rom = s.roms().join("Game.gb");
    std::fs::write(&rom, b"EUROPE").unwrap();
    // A BPS made for "USA!!!": its source checksum doesn't match this file.
    let mut p = b"BPS1".to_vec();
    p.extend_from_slice(&[0x80 | 6, 0x80 | 6, 0x80]);
    p.extend_from_slice(&crc32fast::hash(b"USA!!!").to_le_bytes());
    p.extend_from_slice(&crc32fast::hash(b"USA!!?").to_le_bytes());
    let own = crc32fast::hash(&p);
    p.extend_from_slice(&own.to_le_bytes());
    std::fs::write(s.roms().join("Game.bps"), p).unwrap();
    let err = s.load(&rom).err().expect("the game must not start unpatched");
    assert_eq!(err, softpatch::message(softpatch::Refusal::OtherVersion, "Game.bps"));
    assert!(err.starts_with("The patch Game.bps is for another version of the game."), "{err}");
    assert_eq!(s.seen().loads, 0, "the core never got the game");
    assert!(!pocket_ui::fallback::should_retry(false, 1, false, Some(&err)), "nor RetroArch, which would start the original");
}
