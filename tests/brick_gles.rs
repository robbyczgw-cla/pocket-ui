//! A 3D core on the Brick's way, on the host (docs/TRIMUI-BRICK.md, "3D cores"): a stub core built from C here asks for GLES 3, draws
//! the top half red and the bottom half blue into the framebuffer it is given, and presents it as a hardware frame. pocket-core runs it
//! with the Brick's CPU picture (`POCKET_CORE_SOFTWARE`) against `pocket-fb` with a file as the screen, Mesa's software GPU standing in
//! for the vendor's: the picture must arrive upright, through the context, the framebuffer and the read-back. Skips with a line when the
//! host has no GLES. `cargo test --features brick --test brick_gles`.
#![cfg(all(feature = "brick", target_os = "linux"))]


use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CORE: &str = concat!(r#"#define WIDTH 64
#define HEIGHT 48
#define FPS 60.0
#define RATE 48000.0
#define NO_SRAM
#define retro_load_game stub_default_load
#define retro_run stub_default_run
#define retro_serialize_size stub_default_size
#define retro_serialize stub_default_serialize
#define retro_unserialize stub_default_unserialize

"#, include_str!("../scripts/crash-sweep/stub-core.c"), r#"
#undef retro_load_game
#undef retro_run
#undef retro_serialize_size
#undef retro_serialize
#undef retro_unserialize
typedef void (*reset_fn)(void);
typedef uintptr_t (*fb_fn)(void);
typedef void *(*proc_fn)(const char *);
struct hw { unsigned type; reset_fn reset; fb_fn fb; proc_fn proc; bool depth, stencil, bottom_left; unsigned major, minor; bool cache; reset_fn destroy; bool debug; };
static struct hw hw; static int resets, frames;
static void (*BindFramebuffer)(unsigned, unsigned); static void (*Viewport)(int, int, int, int); static void (*ClearColor)(float, float, float, float);
static void (*Clear)(unsigned); static void (*Enable)(unsigned); static void (*Disable)(unsigned); static void (*Scissor)(int, int, int, int);
static void context_reset(void) {
    resets++;
    BindFramebuffer = hw.proc("glBindFramebuffer"); Viewport = hw.proc("glViewport"); ClearColor = hw.proc("glClearColor");
    Clear = hw.proc("glClear"); Enable = hw.proc("glEnable"); Disable = hw.proc("glDisable"); Scissor = hw.proc("glScissor");
}
static void context_destroy(void) {}
bool retro_load_game(const struct game *g) {
    unsigned xrgb = 1; env(10, &xrgb);
    memset(&hw, 0, sizeof hw); hw.type = 4; hw.reset = context_reset; hw.destroy = context_destroy; hw.bottom_left = true; hw.major = 3;
    return env(14, &hw);
}
void retro_run(void) {
    stub_sound();
    if (!BindFramebuffer) { video(NULL, 64, 48, 0); return; }
    BindFramebuffer(0x8D40, (unsigned)hw.fb());
    Viewport(0, 0, 64, 48);
    Enable(0x0C11);
    /* GL's origin is the bottom left: rows 24..47 are the top half. */
    Scissor(0, 24, 64, 24); ClearColor(1, 0, 0, 1); Clear(0x4000);
    Scissor(0, 0, 64, 24); ClearColor(0, 0, 1, 1); Clear(0x4000);
    Disable(0x0C11);
    frames++;
    video((const void *)(uintptr_t)-1, 64, 48, 0);
}

size_t retro_serialize_size(void) { return 0; }
bool retro_serialize(void *d, size_t s) { (void)d; (void)s; return false; }
bool retro_unserialize(const void *d, size_t s) { (void)d; (void)s; return false; }
"#);
fn wait_for(what: &str, mut ok: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while !ok() {
        if start.elapsed() > Duration::from_secs(30) {
            eprintln!("no {what} after 30 s");
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

#[test]
fn a_gles_core_reaches_the_screen_upright_through_the_read_back() {
    if let Some(err) = run() {
        assert!(err.contains("read back (rgba, ") || err.contains("read back (bgra, "), "the order read, in the log: {err}");
    }
}

/// The direct way's prototype asked for where there is no screen surface (a computer): it says so and reads back as always.
#[test]
fn the_direct_way_without_a_screen_falls_back_to_the_read_back() {
    if let Some(err) = run_with(true) {
        assert!(err.contains("direct: no screen surface"), "{err}");
    }
}

/// Runs the stub core; the core's log, None when the host has no GLES. The log has the order read and the read-back's time split into
/// the read and the turn into the picture.
fn run() -> Option<String> {
    run_with(false)
}

fn run_with(direct: bool) -> Option<String> {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("core.c"), CORE).unwrap();
    let built = Command::new("cc").args(["-shared", "-fPIC", "-o"]).arg(d.join("glstub_libretro.so")).arg(d.join("core.c")).arg("-lm").status().unwrap();
    assert!(built.success(), "the stub core doesn't build");
    std::fs::write(d.join("game.n64"), b"stub").unwrap();
    let (w, h) = (64u32, 48u32);
    std::fs::write(d.join("fb"), vec![0u8; (w * h * 4) as usize]).unwrap();
    let mut bridge = Command::new(env!("CARGO_BIN_EXE_pocket-fb"))
        .args(["--wayland", d.join("w").to_str().unwrap(), "--ipc", d.join("i").to_str().unwrap()])
        .args(["--fb", d.join("fb").to_str().unwrap(), "--mode", &format!("{w}x{h}x32")])
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(wait_for("bridge sockets", || d.join("w").exists() && d.join("i").exists()));
    let log = d.join("core.log");
    std::fs::write(d.join("asound.conf"), "pcm.!default { type null }\n").unwrap();
    let mut core = Command::new(env!("CARGO_BIN_EXE_pocket-core"))
        .arg(d.join("glstub_libretro.so"))
        .arg(d.join("game.n64"))
        .args(["--system", "n64", "--bench", "3"])
        .env("POCKET_CORE_SOFTWARE", "1")
        .env("POCKETUI_DEVICE", "brick")
        .env("EGL_PLATFORM", "surfaceless")
        .env("WAYLAND_DISPLAY", d.join("w"))
        .env("SWAYSOCK", d.join("i"))
        .env("XDG_RUNTIME_DIR", d)
        .env("POCKETUI_DATA", d.join("data"))
        .env("ALSA_CONFIG_PATH", d.join("asound.conf"))
        .env(if direct { "POCKET_CORE_BRICK_DIRECT" } else { "POCKET_CORE_NOT_DIRECT" }, "1")
        .stdout(std::fs::File::create(&log).unwrap())
        .stderr(std::fs::File::create(d.join("core.err")).unwrap())
        .spawn()
        .unwrap();
    let px = |fb: &[u8], x: u32, y: u32| -> [u8; 3] {
        let i = ((y * w + x) * 4) as usize;
        [fb[i + 2], fb[i + 1], fb[i]]
    };
    // The top half mostly red over blue, the bottom half mostly blue over red. Not exact colours: on this 64x48 screen the software
    // HUD's note (a dark pill over most of the top half) dims the red under it; an upside-down or red-blue swapped picture still fails.
    let upright = |fb: &[u8]| {
        let share = |rows: std::ops::Range<u32>, red: bool| {
            let n = rows.clone().flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| {
                let [r, _, b] = px(fb, x, y);
                if red { r > b.saturating_add(30) } else { b > r.saturating_add(30) }
            });
            n.count() * 2 > rows.len() * w as usize
        };
        fb.len() >= (w * h * 4) as usize && share(0..h / 2, true) && share(h / 2..h, false)
    };
    let mut seen = false;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        let fb = std::fs::read(d.join("fb")).unwrap();
        if upright(&fb) {
            seen = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = core.wait();
    let _ = bridge.kill();
    let _ = bridge.wait();
    let err = std::fs::read_to_string(d.join("core.err")).unwrap_or_default();
    if err.contains("GL context") && !seen {
        eprintln!("no GLES on this host, nothing to check: {err}");
        return None;
    }
    assert!(seen, "the core's picture never reached the screen upright\n{err}");
    assert!(err.contains("read back to the CPU") && err.contains("frames read back"), "{err}");
    assert!(["read ", "convert "].iter().all(|k| err.contains(k)), "the read-back's time split: {err}");
    Some(err)
}

