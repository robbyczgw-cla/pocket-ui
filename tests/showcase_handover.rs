//! A on the Cabinet's playing TV goes on with the very game the TV plays (`pocket_ui::showcase`), against a stub core
//! built from C here that counts its frames and shows the count in its picture: the run handed over by `pocket-core
//! --showcase` stops at a picture, its state is that picture's moment, and a game loaded from the state draws the next
//! frame, not the first one. A run that is stopped (another game chosen) leaves nothing behind.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pocket_ui::libretro::Core;
use pocket_ui::showcase;

const STUB: &str = r#"
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
struct info { const char *name, *version, *exts; bool need_fullpath, block_extract; };
struct av { unsigned bw, bh, mw, mh; float aspect; double fps, rate; };
struct game { const char *path; const void *data; size_t size; const char *meta; };
typedef void (*video_t)(const void *, unsigned, unsigned, size_t);
static video_t video;
static uint32_t n;
static uint16_t fb[160 * 144];
void retro_set_environment(void *f) {}
void retro_set_video_refresh(video_t f) { video = f; }
void retro_set_audio_sample(void *f) {}
void retro_set_audio_sample_batch(void *f) {}
void retro_set_input_poll(void *f) {}
void retro_set_input_state(void *f) {}
void retro_set_controller_port_device(unsigned port, unsigned device) {}
unsigned retro_api_version(void) { return 1; }
void retro_init(void) {}
void retro_deinit(void) {}
void retro_get_system_info(struct info *i) { memset(i, 0, sizeof *i); i->name = "stub"; i->exts = "gb"; }
void retro_get_system_av_info(struct av *a) { memset(a, 0, sizeof *a); a->bw = a->mw = 160; a->bh = a->mh = 144; a->fps = 60; a->rate = 48000; }
bool retro_load_game(const struct game *g) { n = 0; return true; }
void retro_unload_game(void) {}
void retro_reset(void) { n = 0; }
/* Frame n shows n (0RGB1555, every pixel): bright enough from the start not to count as a black boot. */
void retro_run(void) {
    n++;
    for (int i = 0; i < 160 * 144; i++) fb[i] = (uint16_t)((n & 0x7fff) | 0x4210);
    if (video) video(fb, 160, 144, 320);
}
void *retro_get_memory_data(unsigned id) { return NULL; }
size_t retro_get_memory_size(unsigned id) { return 0; }
size_t retro_serialize_size(void) { return 4; }
bool retro_serialize(void *d, size_t s) { if (s < 4) return false; memcpy(d, &n, 4); return true; }
bool retro_unserialize(const void *d, size_t s) { if (s < 4) return false; memcpy(&n, d, 4); return true; }
"#;

/// The count a stub frame shows, from a 0RGB pixel of it: the bits the stub keeps lit left out (two frames in a row
/// still differ, in the lowest bit).
fn count_of(px: u32) -> u32 {
    let (r, g, b) = ((px >> 16 & 0xff) >> 3, (px >> 8 & 0xff) >> 3, (px & 0xff) >> 3);
    (r << 10 | g << 5 | b) & 0x7fff & !0x4210
}

fn shown(n: u32) -> u32 {
    n & 0x7fff & !0x4210
}

struct Setup {
    _guard: tempfile::TempDir,
    dir: PathBuf,
    core: PathBuf,
    rom: PathBuf,
}

impl Setup {
    fn new(name: &str) -> Setup {
        let guard = tempfile::Builder::new().prefix(&format!("pocketui-showcase-{name}-")).tempdir().unwrap();
        let dir = guard.path().to_path_buf();
        std::fs::create_dir_all(dir.join("roms/gb")).unwrap();
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join("stub.c"), STUB).unwrap();
        let core = dir.join("stub_libretro.so");
        let out = std::process::Command::new("cc")
            .args(["-shared", "-fPIC", "-o"])
            .arg(&core)
            .arg(dir.join("stub.c"))
            .output()
            .expect("the showcase hand-over tests build a stub core and need a C compiler (cc)");
        assert!(out.status.success(), "cc: {}", String::from_utf8_lossy(&out.stderr));
        let rom = dir.join("roms/gb/Showcase Stub.gb");
        std::fs::write(&rom, [0u8; 64]).unwrap();
        Setup { _guard: guard, dir, core, rom }
    }

    /// `pocket-core --showcase` on the stub, writing to `out`, its /tmp in the test folder.
    fn run(&self, out: &Path) -> std::process::Child {
        std::process::Command::new(env!("CARGO_BIN_EXE_pocket-core"))
            .arg("--showcase")
            .arg(&self.core)
            .arg(&self.rom)
            .args(["--system", "gb", "--out"])
            .arg(out)
            .args(["--size", "90x70", "--seconds", "20"])
            .env("TMPDIR", &self.dir)
            .env("POCKETUI_DATA", self.dir.join("data"))
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap()
    }

    /// Waits until the run has shown a few screens on the TV.
    fn playing(&self, out: &Path) {
        let mut reader = showcase::Reader::new(out);
        let t = Instant::now();
        let mut screens = 0;
        while screens < 5 && t.elapsed() < Duration::from_secs(10) {
            if reader.take().is_some() {
                screens += 1;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(screens, 5, "the TV plays");
    }
}

fn ends(child: &mut std::process::Child, within: Duration) -> Option<std::process::ExitStatus> {
    let t = Instant::now();
    while t.elapsed() < within {
        if let Some(s) = child.try_wait().unwrap() {
            return Some(s);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

#[test]
fn a_on_the_tv_goes_on_from_the_frame_it_shows() {
    let s = Setup::new("hand");
    let out = s.dir.join("pocket-showcase-1.frame");
    let mut child = s.run(&out);
    s.playing(&out);
    let asked = Instant::now();
    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGUSR1) };
    let status = ends(&mut child, Duration::from_secs(2)).expect("the run ends after handing over");
    assert!(status.success());
    let state = showcase::hand_state(&out);
    let frame = showcase::read_hand_frame(&showcase::hand_frame(&out)).expect("the picture it stopped at");
    assert!(asked.elapsed() < Duration::from_millis(500), "handed over at once, not at the next screen: {:?}", asked.elapsed());
    let at = u32::from_le_bytes(std::fs::read(&state).unwrap()[..4].try_into().unwrap());
    assert!(at > 5, "the game ran a while on the TV: frame {at}");
    assert_eq!((frame.w, frame.h), (160, 144), "the game's own picture, not the TV's");
    assert_eq!(count_of(frame.px[0]), shown(at), "the picture is the state's own frame");
    // pocket-core with the state: the next frame, not the game's first.
    let core = Core::load(&s.core, &s.rom, &s.dir, &s.dir.join("roms/gb"), &[]).unwrap();
    showcase::continue_from(&core, &state, Duration::ZERO).unwrap();
    assert!(!state.exists(), "the state is used once");
    let (f, ..) = core.run(0);
    let rgb = f.expect("a frame").to_rgb();
    let px = (rgb[0] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[2] as u32;
    assert_eq!(count_of(px), shown(at + 1), "the game goes on with the frame after the TV's");
    assert!(!out.exists(), "the TV's frame file is gone");
}

#[test]
fn a_run_stopped_hands_nothing_over() {
    // Another game chosen, a page opened: TERM. Nothing for a pocket-core to go on from, nothing left in /tmp.
    let s = Setup::new("term");
    let out = s.dir.join("pocket-showcase-2.frame");
    let mut child = s.run(&out);
    s.playing(&out);
    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
    assert!(ends(&mut child, Duration::from_secs(2)).is_some());
    for f in [out.clone(), showcase::hand_state(&out), showcase::hand_frame(&out)] {
        assert!(!f.exists(), "{} is left", f.display());
    }
    let left: Vec<_> = std::fs::read_dir(&s.dir).unwrap().flatten().map(|e| e.file_name()).filter(|n| n.to_string_lossy().starts_with("pocket-showcase")).collect();
    assert!(left.is_empty(), "its save folder is gone too: {left:?}");
}
