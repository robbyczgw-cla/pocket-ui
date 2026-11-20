//! A on the Cabinet's playing TV goes on with the very game the TV plays (`pocket_ui::showcase`), against a stub core
//! built from C here that counts its frames and shows the count in its picture: the run handed over by `pocket-core
//! --showcase` stops at a picture, its state is that picture's moment, and a game loaded from the state draws the next
//! frame, not the first one. A run that is stopped (another game chosen) leaves nothing behind.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pocket_ui::libretro::Core;
use pocket_ui::showcase;

const STUB: &str = concat!(r#"#define WIDTH 160
#define HEIGHT 144
#define FPS 60.0
#define RATE 48000.0
#define NO_SRAM
#define retro_run stub_default_run
#define retro_serialize_size stub_default_size
#define retro_serialize stub_default_serialize
#define retro_unserialize stub_default_unserialize

"#, include_str!("../scripts/crash-sweep/stub-core.c"), r#"
#undef retro_run
#undef retro_serialize_size
#undef retro_serialize
#undef retro_unserialize
/* Frame n shows n in the original 0RGB1555 contract, expanded to the common XRGB8888 buffer. */
void retro_run(void) {
    frame++;
    unsigned p = (frame & 0x7fff) | 0x4210;
    unsigned rgb = ((p >> 10 & 31) * 255 / 31) << 16 | ((p >> 5 & 31) * 255 / 31) << 8 | (p & 31) * 255 / 31;
    for (unsigned i = 0; i < WIDTH * HEIGHT; i++) fb[i] = rgb;
    if (video) video(fb, WIDTH, HEIGHT, WIDTH * 4);
    stub_sound();
}
size_t retro_serialize_size(void) { return 4; }
bool retro_serialize(void *d, size_t s) { if (s < 4) return false; memcpy(d, &frame, 4); return true; }
bool retro_unserialize(const void *d, size_t s) { if (s < 4) return false; memcpy(&frame, d, 4); return true; }
"#);
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
            .arg(dir.join("stub.c")).arg("-lm")
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

/// The shared fixture advertises this rate; its batches must carry exactly that much time.
#[test]
fn the_shared_stub_delivers_its_advertised_sound_rate() {
    let setup = Setup::new("audio-rate");
    let core = Core::load(&setup.core, &setup.rom, &setup.dir, &setup.dir.join("roms/gb"), &[]).unwrap();
    let mut delivered = 0;
    for _ in 0..60 {
        let (picture, sound, ..) = core.run(0);
        assert!(picture.is_some(), "each run supplies its picture");
        assert!(sound.iter().any(|&s| s != 0), "each run supplies sound");
        assert_eq!(sound.len() % 2, 0, "stereo pairs");
        delivered += sound.len() / 2;
    }
    assert_eq!(delivered, 48000, "one second at the advertised 60 Hz / 48 kHz");
}
