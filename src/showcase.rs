//! Showcase: the Cabinet's TV plays the real game. Resting about two
//! seconds on a game that was never played, the TV on the top board turns on and shows the game running: its title
//! screen, then the game's own demo, without sound. After [`LIVE`] it goes back to its still picture. Games already
//! played keep showing the player's last moment (`cabinet_tv::source`), as before.
//!
//! The game runs in a second pocket-core started with `--showcase` ([`headless`]): no window, no sound, no input, no
//! CPU or fan profile, an empty save folder of its own in /tmp (the player's saves are never read or written), and
//! `nice` 10, so the shelf always wins the CPU. It draws every second frame of a 60 Hz game (30 a second) through the
//! TV's CRT look at the glass's size and writes it to a frame file in /tmp ([`Writer`], [`Reader`]); the UI only
//! copies a finished screen into the TV.
//!
//! Only the small systems for now ([`SYSTEMS`]): the Game Boys, NES, SNES, Mega Drive.
//!
//! A on the playing TV goes on with that very game (step 2): pocket-ui asks the run to hand over (SIGUSR1), it writes
//! its state ([`hand_state`]) and the picture it stopped at ([`hand_frame`]) and ends; the picture grows out of the TV
//! to where the game will draw it, and pocket-core, held still until then (`startup::HOLD_PAUSE`), continues from the
//! state ([`STATE_ENV`], [`continue_from`]) with its sound fading in ([`FadeIn`]). Only for a game with nothing of its
//! own yet ([`may_continue`]: no battery save, no moment) on the same core and not sealed in its film ([`usual_way`]); any other
//! starts the usual way.
//!
//! The run is not a `pocket-core` to the system ([`PROCESS`], [`command`]): `pidof pocket-core` never sees it.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::cabinet_art::Pic;

/// How long the player rests on a game before the TV turns on.
pub const DWELL: Duration = Duration::from_millis(2000);
/// How long the TV plays before it goes back to the still picture.
pub const LIVE: Duration = Duration::from_secs(25);
/// `LIVE`, or `POCKETUI_SHOWCASE_SECONDS` (measurements).
pub fn live() -> Duration {
    std::env::var("POCKETUI_SHOWCASE_SECONDS").ok().and_then(|s| s.parse().ok()).map_or(LIVE, Duration::from_secs)
}
/// Pictures a second on the TV.
pub const FPS: f64 = 30.0;
/// The systems whose games play on the TV: light cores only.
pub const SYSTEMS: &[&str] = &["gb", "gbc", "gba", "nes", "famicom", "snes", "sfc", "megadrive", "genesis", "mastersystem", "gamegear"];

const MAGIC: &[u8; 4] = b"PSHW";
const HEADER: u64 = 16;

/// Whether a system's games may play on the TV.
pub fn system_plays(system: &str) -> bool {
    SYSTEMS.contains(&system)
}

/// Whether the TV may play the game: Settings → Look → Cozy extras → "TV plays the game" (`Extra::TvPlaysGame`), **off
/// at the start**. Off, no player is ever started. `POCKETUI_SHOWCASE=1` / `=0` forces it for measurements and tools.
pub fn enabled() -> bool {
    match std::env::var("POCKETUI_SHOWCASE").as_deref() {
        Ok("0") => false,
        Ok("1") => true,
        _ => crate::cozy_extras::on(crate::cozy_extras::Extra::TvPlaysGame),
    }
}

/// The frame file of the UI with this process id: in /tmp (RAM on the handhelds).
pub fn frame_file(ui_pid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("pocket-showcase-{ui_pid}.frame"))
}

/// The name the TV's player runs under: not `pocket-core`, so `pidof pocket-core` (BusyBox and procps match the name,
/// the first argument and the program file) and pocket-ui's own look at running games never take it for a game.
pub const PROCESS: &str = "pocket-showcase";

/// A command that runs `bin` (pocket-core) as [`PROCESS`], with the copy of the program it runs: kept until spawned.
pub struct Spawn {
    pub cmd: std::process::Command,
    _image: Option<std::os::fd::OwnedFd>,
}

/// The command for the TV's player: `bin` run from a copy in memory named [`PROCESS`], as `PROCESS` (BusyBox's and
/// procps' pidof look at the program file too, so a link or a new first argument alone wouldn't do). The copy lives as
/// long as the run (a few MB); where memory files are missing it runs `bin` itself.
pub fn command(bin: &Path) -> Spawn {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;
    let image = program_image(bin);
    let mut cmd = match &image {
        Some(fd) => std::process::Command::new(format!("/proc/self/fd/{}", fd.as_raw_fd())),
        None => std::process::Command::new(bin),
    };
    cmd.arg0(PROCESS);
    Spawn { cmd, _image: image }
}

/// `bin` copied into a memory file named [`PROCESS`], read-only: exec refuses a file still open for writing (ETXTBSY).
fn program_image(bin: &Path) -> Option<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    // SAFETY: memfd_create returns a new descriptor or -1; the File takes ownership of it.
    let fd = unsafe { libc::memfd_create(c"pocket-showcase".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    std::io::copy(&mut File::open(bin).ok()?, &mut file).ok()?;
    let ro = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_CLOEXEC).open(format!("/proc/self/fd/{fd}")).ok()?;
    drop(file);
    Some(ro.into())
}

/// Why A on the playing TV starts the game the usual way (insertion scene, the film's tear) instead of going on with
/// the TV's run: None when it may go on. `may`: the game has nothing of its own yet ([`may_continue`]); `same_core`:
/// it starts on the core the run played; `sealed`: it is still in the shop's film (Sealed new games), which tears off
/// at the first start and must never be skipped (Brick, 14.11.2026: Golden Sun went straight into its name entry).
pub fn usual_way(may: bool, same_core: bool, sealed: bool) -> Option<&'static str> {
    if sealed {
        Some("its film tears off first")
    } else if !may {
        Some("it has a save of its own")
    } else if !same_core {
        Some("another core")
    } else {
        None
    }
}

/// Where the run stopped for A: its state (the game's memory), next to its frame file `out`.
pub fn hand_state(out: &Path) -> PathBuf {
    out.with_extension("state")
}

/// Where the run stopped for A: the game's own picture (not the TV's), next to its frame file `out`.
pub fn hand_frame(out: &Path) -> PathBuf {
    out.with_extension("last")
}

/// pocket-core: continue from this state file (the TV's run handed over), not from the game's own start.
pub const STATE_ENV: &str = "POCKET_CORE_SHOWCASE_STATE";
/// pocket-core: let the game's sound come up over this many ms instead of at once.
pub const FADE_IN_ENV: &str = "POCKET_CORE_FADE_IN_MS";
/// How long the sound takes to come up after the hand-over.
pub const FADE_IN: Duration = Duration::from_millis(500);
/// How long the picture takes to grow out of the TV.
pub const GROW: Duration = Duration::from_millis(360);

/// The folder pocket-core keeps a game's battery save in (as its `main`): next to the game, the Brick's own folder.
pub fn save_dir(system: &str, rom: &Path) -> PathBuf {
    #[cfg(feature = "brick")]
    if let Some(d) = std::env::var_os("POCKET_CORE_SAVE_DIR") {
        return PathBuf::from(d).join(system);
    }
    let _ = system;
    rom.parent().map_or_else(|| PathBuf::from("/storage/roms/misc"), Path::to_path_buf)
}

/// Whether A may go on with the TV's run: the game has nothing of its own yet, no battery save and no moment. The
/// run's state carries the game's memory, a save among it, so it must never stand in for one the player has.
pub fn may_continue(system: &str, rom: &Path) -> bool {
    let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else { return false };
    let dir = save_dir(system, rom);
    if crate::game_files::SAVE_EXTS.iter().any(|e| dir.join(format!("{stem}.{e}")).exists()) {
        return false;
    }
    let auto = crate::slots::auto_path(&crate::slots::base(system, stem));
    !auto.exists() && crate::state_file::legacy_path(&auto).is_none_or(|p| !p.exists())
}

/// Removes the files and folders of a run (its frame file, its hand-over, its save folder) that may or may not be
/// there: gone already is fine, anything else is said on stderr (a /tmp that can't be cleaned would fill the RAM).
pub fn clear(paths: &[&Path]) {
    for p in paths {
        let r = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
        match r {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => eprintln!("showcase: {}: {e}", p.display()),
            _ => {}
        }
    }
}

/// pocket-core: loads the state the TV's run handed over (`path`, waited for up to `wait`; it is gone afterwards).
pub fn continue_from(core: &crate::libretro::Core, path: &Path, wait: Duration) -> Result<(), String> {
    let t = Instant::now();
    while !path.exists() && t.elapsed() < wait {
        std::thread::sleep(Duration::from_millis(5));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    clear(&[path]);
    core.load_state(&bytes)
}

/// Writes the hand-over: the state, then the picture (its file there means both are).
fn write_hand(out: &Path, state: &[u8], img: &crate::image::Image) -> std::io::Result<()> {
    let put = |to: PathBuf, bytes: &[u8]| -> std::io::Result<()> {
        let part = to.with_extension("part");
        std::fs::write(&part, bytes)?;
        std::fs::rename(&part, &to)
    };
    put(hand_state(out), state)?;
    let mut b = Vec::with_capacity(12 + img.px.len() * 4);
    b.extend_from_slice(b"PSHL");
    b.extend_from_slice(&img.w.to_le_bytes());
    b.extend_from_slice(&img.h.to_le_bytes());
    for p in &img.px {
        b.extend_from_slice(&p.to_le_bytes());
    }
    put(hand_frame(out), &b)
}

/// The picture a run stopped at (`hand_frame`), 0RGB.
pub fn read_hand_frame(path: &Path) -> Option<crate::image::Image> {
    let b = std::fs::read(path).ok()?;
    if b.len() < 12 || &b[..4] != b"PSHL" {
        return None;
    }
    let w = u32::from_le_bytes(b[4..8].try_into().ok()?);
    let h = u32::from_le_bytes(b[8..12].try_into().ok()?);
    if w == 0 || h == 0 || w > 4096 || h > 4096 || b.len() != 12 + (w * h * 4) as usize {
        return None;
    }
    let px = b[12..].chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    Some(crate::image::Image { w, h, px })
}

/// The sound coming up after a hand-over: silent at first, full after its length, no step on the way.
pub struct FadeIn {
    len: u32,
    done: u32,
    /// The level of what went out, per 50 ms block, from the first sample to 0.3 s past full (`report`).
    blocks: Vec<(f64, u32)>,
    block: u32,
    reported: bool,
}

/// No fade, nothing to report (a start that did not go on from the TV: pocket-core resets to this).
impl Default for FadeIn {
    fn default() -> FadeIn {
        FadeIn::new(0, 48000.0)
    }
}

impl FadeIn {
    /// Over `ms` at `rate` (frames a second); 0: no fade.
    pub fn new(ms: u32, rate: f64) -> FadeIn {
        let len = (ms as f64 * rate / 1000.0) as u32;
        FadeIn { len, done: 0, blocks: Vec::new(), block: (rate / 20.0).max(1.0) as u32, reported: len == 0 }
    }

    /// From pocket-core's `FADE_IN_ENV`.
    pub fn from_env(rate: f64) -> FadeIn {
        FadeIn::new(std::env::var(FADE_IN_ENV).ok().and_then(|v| v.parse().ok()).unwrap_or(0), rate)
    }

    /// Scales interleaved stereo `samples` on the way up.
    pub fn apply(&mut self, samples: &mut [i16]) {
        let watch = self.len + self.block * 6;
        if self.done >= watch {
            return;
        }
        for pair in samples.chunks_mut(2) {
            if self.done < self.len {
                let g = self.done as f32 / self.len as f32;
                // Smooth at both ends: no click where it starts, no corner where it is full.
                let g = g * g * (3.0 - 2.0 * g);
                for s in pair.iter_mut() {
                    *s = (*s as f32 * g) as i16;
                }
            }
            if self.done < watch {
                let i = (self.done / self.block) as usize;
                if self.blocks.len() <= i {
                    self.blocks.resize(i + 1, (0.0, 0));
                }
                let b = &mut self.blocks[i];
                b.0 += pair.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>();
                b.1 += pair.len() as u32;
            }
            self.done = self.done.saturating_add(1);
        }
    }

    /// Once the sound is up and 0.3 s past: its level per 50 ms (dBFS, RMS) from the first sample, as measured in what
    /// went out (pocket-core prints it: the proof the sound came up softly and then stayed).
    pub fn report(&mut self) -> Option<String> {
        if self.reported || self.done < self.len + self.block * 6 {
            return None;
        }
        self.reported = true;
        let db: Vec<String> = self
            .blocks
            .iter()
            .map(|(sum, n)| {
                let rms = (sum / (*n).max(1) as f64).sqrt() / 32768.0;
                if rms > 0.0 { format!("{:.0}", 20.0 * rms.log10()) } else { "-inf".into() }
            })
            .collect();
        Some(format!("sound came up over {} ms, dBFS per 50 ms: {}", self.len as u64 * 1000 / (self.block as u64 * 20), db.join(" ")))
    }
}

/// The core file for a system's games, as pocket-ui would start them with pocket-core: the handheld cores, the routed
/// ones, the Brick's and the H700's own choices. None: no such core on this device.
pub fn core_for(system: &str, rom: &Path) -> Option<PathBuf> {
    let core = crate::hostsys::HANDHELD_CORES
        .iter()
        .find(|(s, _)| *s == system)
        .map(|(_, c)| *c)
        .or_else(|| crate::hostsys::find(system).map(|s| s.core))?;
    #[cfg(feature = "brick")]
    let core = crate::brick_profile::additional_core(system).unwrap_or_else(|| crate::brick_profile::core_for(core));
    #[cfg(not(feature = "brick"))]
    let core = if crate::h700_gba::is_gba(system) && crate::h700_gba::enabled() { crate::h700_gba::default_core(system, Some(rom)) } else { core };
    #[cfg(feature = "brick")]
    let _ = rom;
    let path = crate::store::core_path(core);
    path.is_file().then_some(path)
}

/// Writes the screens of a showcase run: header (magic, sequence, width, height), then the pixels. The sequence is
/// written last, so a reader that sees it change after reading the pixels knows they were torn and waits a tick.
pub struct Writer {
    file: File,
    seq: u32,
    bytes: Vec<u8>,
}

impl Writer {
    pub fn create(path: &Path) -> std::io::Result<Writer> {
        let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path)?;
        file.write_all_at(MAGIC, 0)?;
        file.write_all_at(&0u32.to_le_bytes(), 4)?;
        Ok(Writer { file, seq: 0, bytes: Vec::new() })
    }

    pub fn write(&mut self, pic: &Pic) -> std::io::Result<()> {
        self.bytes.clear();
        self.bytes.reserve(pic.px.len() * 4);
        for p in &pic.px {
            self.bytes.extend_from_slice(&p.to_le_bytes());
        }
        // Odd while writing: a reader skips it.
        self.seq = self.seq.wrapping_add(1) | 1;
        self.file.write_all_at(&self.seq.to_le_bytes(), 4)?;
        let mut head = [0u8; 8];
        head[..4].copy_from_slice(&(pic.w as u32).to_le_bytes());
        head[4..].copy_from_slice(&(pic.h as u32).to_le_bytes());
        self.file.write_all_at(&head, 8)?;
        self.file.write_all_at(&self.bytes, HEADER)?;
        self.seq = self.seq.wrapping_add(1);
        self.file.write_all_at(&self.seq.to_le_bytes(), 4)
    }
}

/// Reads the newest screen of a showcase run (`Writer`).
pub struct Reader {
    path: PathBuf,
    file: Option<File>,
    seq: u32,
    bytes: Vec<u8>,
}

impl Reader {
    pub fn new(path: &Path) -> Reader {
        Reader { path: path.to_path_buf(), file: None, seq: 0, bytes: Vec::new() }
    }

    fn seq_now(&mut self) -> Option<u32> {
        if self.file.is_none() {
            self.file = File::open(&self.path).ok();
        }
        let f = self.file.as_ref()?;
        let mut head = [0u8; 8];
        f.read_exact_at(&mut head, 0).ok()?;
        (&head[..4] == MAGIC).then(|| u32::from_le_bytes([head[4], head[5], head[6], head[7]]))
    }

    /// A newer whole screen than the last one taken is there (cheap: 8 bytes).
    pub fn fresh(&mut self) -> bool {
        self.seq_now().is_some_and(|s| s != self.seq && s % 2 == 0 && s != 0)
    }

    /// The newest whole screen, if it is newer than the last one taken.
    pub fn take(&mut self) -> Option<Pic> {
        let seq = self.seq_now()?;
        if seq == self.seq || seq % 2 == 1 || seq == 0 {
            return None;
        }
        let f = self.file.as_ref()?;
        let mut head = [0u8; 8];
        f.read_exact_at(&mut head, 8).ok()?;
        let (w, h) = (u32::from_le_bytes([head[0], head[1], head[2], head[3]]), u32::from_le_bytes([head[4], head[5], head[6], head[7]]));
        if w == 0 || h == 0 || w > 2048 || h > 2048 {
            return None;
        }
        self.bytes.resize((w * h * 4) as usize, 0);
        f.read_exact_at(&mut self.bytes, HEADER).ok()?;
        // Torn: the writer moved on while we read. The next tick takes the next one.
        if self.seq_now()? != seq {
            return None;
        }
        self.seq = seq;
        let mut pic = Pic::new(w as i32, h as i32, 0);
        for (p, b) in pic.px.iter_mut().zip(self.bytes.chunks_exact(4)) {
            *p = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        }
        Some(pic)
    }
}

/// A frame as 0RGB pixels, for the CRT look.
fn image_of(f: &crate::libretro::Frame) -> crate::image::Image {
    let rgb = f.to_rgb();
    crate::image::Image { w: f.width, h: f.height, px: rgb.chunks_exact(3).map(|c| (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect() }
}

/// Mostly black: the boot of a game before its first picture (a few lit pixels of a logo count as black).
fn dark(img: &crate::image::Image) -> bool {
    let lit = img.px.iter().step_by(7).filter(|p| (**p >> 16 & 0xff) + (**p >> 8 & 0xff) + (**p & 0xff) > 60).count();
    lit * 7 < img.px.len() / 50
}

static TERM: AtomicBool = AtomicBool::new(false);
/// pocket-ui asked for the hand-over (SIGUSR1): A was pressed on the TV.
static HAND: AtomicBool = AtomicBool::new(false);

extern "C" fn on_term(_: libc::c_int) {
    TERM.store(true, Ordering::Relaxed);
}

extern "C" fn on_hand(_: libc::c_int) {
    HAND.store(true, Ordering::Relaxed);
}

/// The core options pocket-core starts this game with (its `main`): the player's saved ones first, then the system's,
/// then the game's defaults. The same options, so the state the run hands over fits the game that goes on with it.
fn options_for(core_so: &Path, system: &str, rom: &Path) -> Vec<(String, String)> {
    let data = PathBuf::from(std::env::var("POCKETUI_DATA").unwrap_or_else(|_| "/storage/.config/pocket-ui".into()));
    let settings = crate::options::GameSettings::load(&data, system, rom);
    let saved = crate::core_options::saved_from(&settings.values);
    let mut options: Vec<(&str, &str)> = saved.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    options.extend_from_slice(crate::hostsys::find(system).map_or(&[][..], |h| h.options));
    crate::core_options::add_game_defaults(&mut options, &crate::core_options::core_id(core_so), rom);
    options.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// `pocket-core --showcase <core.so> <rom> --system S --out FILE --size WxH [--seconds N]`: plays the game without a
/// window or sound and writes its screens, through the TV's CRT look at WxH, to FILE ([`Writer`]). Prints what it cost
/// at the end (one line, `showcase-core …`). The exit code: 0 done, 1 the game didn't start, 2 wrong arguments.
pub fn headless(args: &[String]) -> i32 {
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let (Some(core_so), Some(rom), Some(out), Some(size)) = (args.get(2).cloned(), args.get(3).cloned(), arg("--out"), arg("--size")) else {
        eprintln!("usage: pocket-core --showcase <core.so> <rom> --system S --out FILE --size WxH [--seconds N]");
        return 2;
    };
    let Some((w, h)) = size.split_once('x').and_then(|(w, h)| Some((w.parse::<i32>().ok()?, h.parse::<i32>().ok()?))) else { return 2 };
    let seconds = arg("--seconds").and_then(|s| s.parse::<f64>().ok()).unwrap_or(live().as_secs_f64() + 2.0);
    let rom = PathBuf::from(rom);
    let system = arg("--system").unwrap_or_else(|| rom.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("misc").to_string());
    unsafe {
        // Behind the shelf, always; gone with pocket-ui; quiet on TERM.
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
        libc::prctl(libc::PR_SET_NAME, c"pocket-showcase".as_ptr());
        libc::signal(libc::SIGTERM, on_term as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_term as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGUSR1, on_hand as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }
    // An empty save folder of its own: the game starts fresh, and nothing of the player's is read or written.
    let saves = std::env::temp_dir().join(format!("pocket-showcase-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&saves) {
        eprintln!("showcase: {}: {e}", saves.display());
        return 1;
    }
    let bios = crate::hostsys::bios_dir();
    let owned = options_for(Path::new(&core_so), &system, &rom);
    let options: Vec<(&str, &str)> = owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let content = crate::hostsys::content_path(&system, &rom);
    let t0 = Instant::now();
    let core = match crate::libretro::Core::load(Path::new(&core_so), &content, &bios, &saves, &options) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("showcase: {e}");
            clear(&[&saves]);
            return 1;
        }
    };
    let load_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let mut writer = match Writer::create(Path::new(&out)) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("showcase: {out}: {e}");
            drop(core);
            clear(&[&saves]);
            return 1;
        }
    };
    let fps = if core.av.timing.fps > 1.0 { core.av.timing.fps } else { 60.0 };
    // Frames the core runs for each one shown: 2 at 60 Hz.
    let step = ((fps / FPS).round() as u32).max(1);
    let cpu = || crate::run_ahead::thread_cpu_ms();
    // The black boot, fast: at most four seconds of the game, as quick as the core goes.
    let skip_t = Instant::now();
    let mut skipped = 0u32;
    while skipped < (fps * 4.0) as u32 && !TERM.load(Ordering::Relaxed) {
        let (frame, ..) = core.run_with(0, if skipped % 4 == 3 { crate::libretro::AV_VIDEO } else { 0 });
        skipped += 1;
        if let Some(f) = frame {
            if !dark(&image_of(&f)) {
                break;
            }
        }
    }
    let skip_ms = skip_t.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let period = Duration::from_secs_f64(step as f64 / fps);
    let mut next = Instant::now();
    let (mut shown, mut late) = (0u32, 0u32);
    let (mut run_ms, mut look_ms) = (0.0f64, 0.0f64);
    let mut last: Option<crate::libretro::Frame> = None;
    let mut handed = false;
    while started.elapsed().as_secs_f64() < seconds && !TERM.load(Ordering::Relaxed) {
        // A on the TV: the game's memory and the picture it is at, for pocket-core to go on from (`continue_from`).
        // The picture is the one the TV shows: the state is taken after the frame it came from.
        if HAND.load(Ordering::Relaxed) {
            if let (Some(f), Some(state)) = (&last, core.save_state()) {
                handed = write_hand(Path::new(&out), &state, &image_of(f)).is_ok();
            }
            break;
        }
        let c0 = cpu();
        for i in 0..step {
            let (frame, ..) = core.run_with(0, if i + 1 == step { crate::libretro::AV_VIDEO } else { 0 });
            if let Some(f) = frame {
                last = Some(f);
            }
        }
        let c1 = cpu();
        run_ms += c1 - c0;
        if let Some(f) = &last {
            let pic = crate::cabinet_tv::crt(&image_of(f), w, h);
            if writer.write(&pic).is_err() {
                break;
            }
            shown += 1;
        }
        look_ms += cpu() - c1;
        next += period;
        let now = Instant::now();
        if next > now {
            // One nanosleep, which a signal ends early (std's sleep would go on): a hand-over doesn't wait for it.
            let d = next - now;
            let ts = libc::timespec { tv_sec: d.as_secs() as libc::time_t, tv_nsec: d.subsec_nanos() as libc::c_long };
            unsafe { libc::nanosleep(&ts, std::ptr::null_mut()) };
        } else if now - next > period * 3 {
            // Far behind (the shelf took the CPU): go on from now rather than racing to catch up.
            late += 1;
            next = now;
        }
    }
    let secs = started.elapsed().as_secs_f64().max(0.001);
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    let total_cpu_ms = (ru.ru_utime.tv_sec as f64 + ru.ru_stime.tv_sec as f64) * 1000.0 + (ru.ru_utime.tv_usec as f64 + ru.ru_stime.tv_usec as f64) / 1000.0;
    println!(
        "showcase-core system={system} handed={handed} fps={fps:.2} step={step} load_ms={load_ms:.0} skip_frames={skipped} skip_ms={skip_ms:.0} secs={secs:.1} shown={shown} shown_per_s={:.1} late={late} run_ms_per_shown={:.2} look_ms_per_shown={:.2} cpu_ms_total={total_cpu_ms:.0} max_rss_kb={}",
        shown as f64 / secs,
        run_ms / shown.max(1) as f64,
        look_ms / shown.max(1) as f64,
        ru.ru_maxrss
    );
    drop(core);
    clear(&[&saves, Path::new(&out)]);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_source_rule_showcase_frames() {
        let d = crate::testutil::TestDir::new("showcase-file-rule");
        let path = d.join("frame");
        let mut writer = Writer::create(&path).unwrap();
        writer.write(&Pic::new(3, 2, 0xff123456)).unwrap();
        let live = std::fs::read(&path).unwrap();
        drop(writer);
        let out = d.join("handover");
        write_hand(&out, b"state", &crate::image::Image { w:3, h:2, px:vec![0x123456;6] }).unwrap();
        let hand = std::fs::read(hand_frame(&out)).unwrap();
        for (is_hand, seed) in [(false, live), (true, hand)] {
            for (label, bytes) in crate::file_faults::faults(&seed) {
                let _ = std::fs::remove_file(&path);
                if let Some(bytes) = bytes { std::fs::write(&path, bytes).unwrap(); }
                if is_hand {
                    let got = read_hand_frame(&path);
                    if label == "valid" { let pic=got.unwrap(); assert_eq!((pic.w,pic.h,pic.px.len()),(3,2,6)); }
                } else {
                    let mut reader=Reader::new(&path); let got=reader.take();
                    if label == "valid" { let pic=got.unwrap();assert_eq!((pic.w,pic.h,pic.px.len()),(3,2,6)); }
                    assert!(reader.take().is_none(), "a live frame is not repeated");
                }
            }
        }
    }

    /// The TV plays the game only when the player switched it on (Settings → Look → Cozy extras): at the start no
    /// player is ever started (red before: `enabled()` was true unless `POCKETUI_SHOWCASE=0`).
    #[test]
    fn the_tv_does_not_play_the_game_at_the_start() {
        if std::env::var_os("POCKETUI_SHOWCASE").is_none() {
            assert!(!enabled());
        }
        assert!(!crate::cozy_extras::Extras::default().get(crate::cozy_extras::Extra::TvPlaysGame));
    }

    #[test]
    fn a_screen_written_is_read_whole_once() {
        let d = crate::testutil::TestDir::new("showcase-frames");
        let path = d.join("f");
        let mut r = Reader::new(&path);
        assert!(r.take().is_none(), "no file yet");
        let mut w = Writer::create(&path).unwrap();
        assert!(!r.fresh() && r.take().is_none(), "nothing written yet");
        let mut pic = Pic::new(3, 2, 0xff112233);
        pic.px[5] = 0xffabcdef;
        w.write(&pic).unwrap();
        assert!(r.fresh());
        let got = r.take().unwrap();
        assert_eq!((got.w, got.h), (3, 2));
        assert_eq!(got.px, pic.px);
        assert!(!r.fresh() && r.take().is_none(), "the same screen is not taken twice");
        w.write(&Pic::new(4, 4, 0xff000000)).unwrap();
        assert_eq!(r.take().unwrap().w, 4);
    }

    /// What `pidof NAME` matches (BusyBox's `find_pid_by_name` and procps' pidof alike): the process name, the first
    /// argument's file name, or the program file's name.
    fn pidof_sees(pid: u32, name: &str) -> bool {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        let argv0 = String::from_utf8_lossy(cmdline.split(|b| *b == 0).next().unwrap_or_default()).to_string();
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let base = |s: &str| s.rsplit('/').next().unwrap_or_default().to_string();
        comm.trim() == name || base(&argv0) == name || base(&exe) == name
    }

    #[test]
    fn the_tv_player_is_never_taken_for_a_running_game() {
        // Step 1 (13.11.2026): the TV's player was a plain pocket-core, so `pidof pocket-core` (device-check, the
        // latency tools, Alex's own scripts) said "a game runs" for the 25 s the TV played.
        let d = crate::testutil::TestDir::new("showcase-name");
        let bin = d.join("pocket-core");
        std::fs::copy("/bin/sleep", &bin).unwrap();
        let mut spawn = command(&bin);
        spawn.cmd.arg("5");
        let mut child = spawn.cmd.spawn().unwrap();
        drop(spawn);
        let pid = child.id();
        let seen = pidof_sees(pid, "pocket-core");
        let real = std::process::Command::new("pidof").arg("pocket-core").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).to_string());
        let _ = child.kill();
        let _ = child.wait();
        assert!(!seen, "pidof pocket-core finds the TV's player");
        if let Some(out) = real {
            assert!(!out.split_whitespace().any(|p| p == pid.to_string()), "the real pidof finds it: {out}");
        }
    }

    #[test]
    fn a_game_with_a_save_of_its_own_never_goes_on_from_the_tv() {
        // The run's state carries the game's memory: going on from it, the next save would write the TV's fresh game
        // over the player's (a .srm brought from RetroArch, a .sav from another device).
        let d = crate::testutil::TestDir::new("showcase-saves");
        let rom = d.join("Showcase Saves Game.gba");
        std::fs::write(&rom, b"rom").unwrap();
        assert!(may_continue("gba", &rom), "nothing of its own: the TV's game goes on");
        for ext in ["srm", "sav", "rtc"] {
            let save = d.join(format!("Showcase Saves Game.{ext}"));
            std::fs::write(&save, b"save").unwrap();
            assert!(!may_continue("gba", &rom), "{ext}: it starts the usual way, with its save");
            std::fs::remove_file(&save).unwrap();
        }
    }

    #[test]
    fn the_picture_a_run_stopped_at_is_read_back_whole() {
        let d = crate::testutil::TestDir::new("showcase-hand");
        let out = d.join("pocket-showcase-1.frame");
        let img = crate::image::Image { w: 3, h: 2, px: vec![1, 2, 3, 4, 5, 0xabcdef] };
        write_hand(&out, b"state", &img).unwrap();
        assert_eq!(std::fs::read(hand_state(&out)).unwrap(), b"state");
        let back = read_hand_frame(&hand_frame(&out)).unwrap();
        assert_eq!((back.w, back.h, back.px), (3, 2, img.px));
        std::fs::write(hand_frame(&out), b"PSHL\x03\0\0\0").unwrap();
        assert!(read_hand_frame(&hand_frame(&out)).is_none(), "a cut file is no picture");
    }

    #[test]
    fn the_sound_comes_up_softly_after_the_hand_over() {
        let mut fade = FadeIn::new(500, 48000.0);
        let mut first = vec![10000i16; 2 * 800];
        fade.apply(&mut first);
        assert!(first[0].abs() < 50, "no click where it starts: {}", first[0]);
        let steps: Vec<i32> = first.chunks(2).map(|p| p[0] as i32).collect();
        assert!(steps.windows(2).all(|w| w[1] >= w[0] && w[1] - w[0] < 100), "it rises without a step");
        let mut rest = vec![10000i16; 2 * 48000];
        fade.apply(&mut rest);
        // Frame d of the fade is rest[2 * (d - 800)]: still rising at 0.45 s (d 21600), full at 0.5 s (d 24000).
        assert!(rest[2 * 20800] < 9990 && rest[2 * 23200] == 10000, "full after 0.5 s, not before");
        let line = fade.report().expect("measured once it is up");
        let levels: Vec<f64> = line.rsplit(": ").next().unwrap().split(' ').map(|v| v.parse().unwrap_or(-200.0)).collect();
        assert!(levels[0] < levels[4] && levels[4] < levels[9] && levels[10] > -11.0, "rising, then full: {line}");
        assert!(fade.report().is_none(), "said once");
        // pocket-core resets to the default when a start does not go on from the TV (a game with a moment of its own):
        // the default has nothing to report, not a division by its zero block (Retroid pocket, 14.11.2026: "attempt to
        // divide by zero", Donkey Kong Country with a moment, the start failed).
        let mut reset = FadeIn::default();
        reset.apply(&mut [1000i16; 4800]);
        assert!(reset.report().is_none() && reset.report().is_none());
        let mut none = FadeIn::new(0, 48000.0);
        let mut s = vec![1234i16; 4];
        none.apply(&mut s);
        assert_eq!(s, vec![1234; 4], "no fade asked for: the sound as it is");
    }

    /// A game still sealed in its film never goes on from the TV: the first start is the tear scene (red before: the
    /// hand-over had no such condition and a never-played game, the only kind the TV plays, always went straight on).
    #[test]
    fn a_sealed_game_never_goes_on_from_the_tv() {
        assert_eq!(usual_way(true, true, false), None, "an unsealed game with nothing of its own goes on");
        assert!(usual_way(true, true, true).is_some(), "sealed: the usual start, with the tear");
        assert!(usual_way(false, true, false).is_some(), "a save of its own");
        assert!(usual_way(true, false, false).is_some(), "another core");
    }

    #[test]
    fn only_the_small_systems_play() {
        for s in ["gb", "gbc", "gba", "nes", "snes", "megadrive"] {
            assert!(system_plays(s), "{s}");
        }
        for s in ["psx", "n64", "psp", "dreamcast", "nds", "ports"] {
            assert!(!system_plays(s), "{s}");
        }
    }

    #[test]
    fn a_black_boot_is_dark_and_a_title_is_not() {
        let black = crate::image::Image { w: 160, h: 144, px: vec![0; 160 * 144] };
        assert!(dark(&black));
        let mut logo = black.clone();
        for p in logo.px.iter_mut().take(200) {
            *p = 0xffffff;
        }
        assert!(dark(&logo), "a small logo is still the boot");
        let title = crate::image::Image { w: 160, h: 144, px: (0..160 * 144).map(|i| if i % 3 == 0 { 0x8090a0 } else { 0x101010 }).collect() };
        assert!(!dark(&title));
    }
}
