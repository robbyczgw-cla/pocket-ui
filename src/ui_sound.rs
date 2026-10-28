//! Subtle UI sounds of pocket-ui: a tick as the selection moves, confirm, back, a menu opening, the game
//! switcher, a refusal, and a tick on Vol+/Vol-.
//!
//! - Which sound: `main.rs` takes a [`View`] of the screen before and after each input and asks
//!   [`sound_for`]; code that refuses something (Load on an empty slot) says so with [`refuse`].
//! - Whether at all: [`allowed`]. Never while a game is being played (in a game only the pause menu, and
//!   the switcher over it, make sounds), not asleep, not when switched off. Not muted either ([`audible`]),
//!   which the mixer thread asks the system itself, right before it opens the output.
//! - The sounds are sample files (`assets/ui-sounds/`, [`synth`] decodes them): warm, tactile hardware sounds
//!   made with ElevenLabs sound effects. The mixer thread decodes all of them once when it starts.
//! - Playing never blocks the UI thread: [`play`] only posts to the mixer thread, which owns the output.
//!   That is a native PipeWire stream through ALSA's `pipewire` device (`audio::Sink`, the same path as
//!   pocket-core), opened for the first sound and closed again after `KEEP_OPEN` of quiet, so an idle
//!   shelf keeps no stream running.
//! - Latency (measured in the virtual device, docs/HOST.md "UI sounds"): ROCKNIX runs PipeWire at a quantum of 1024
//!   frames (21 ms), and the output buffer then holds about one quantum of silence ahead of a new sound, which
//!   is picked up only when a blocked write returns. So on the shelf the output is opened at a quantum of
//!   `SHELF_QUANTUM` while sounds play (as pocket-core does for a game; not on Bluetooth), the mute check runs beside the opening
//!   instead of before it, and chunks are 128 frames. In a game the quantum is pocket-core's already.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::pad::Button;

/// A and B as the Retroid prints them (see main.rs).
const CONFIRM: Button = Button::East;
const CANCEL: Button = Button::South;

/// The sounds, one per kind of feedback.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sound {
    /// The selection moved.
    Nav,
    /// Something was chosen or switched.
    Confirm,
    /// One level up, or a menu closed.
    Back,
    /// The pause menu or the quick settings opened.
    Menu,
    /// The game switcher opened.
    Switcher,
    /// What was asked can't be done (an empty save slot, settings that couldn't be saved).
    Error,
    /// Vol+ or Vol- changed the volume.
    Volume,
    /// A screenshot was saved (HOME + A in a game, `screenshot`): two quick, quiet clicks.
    Shutter,
}

/// What covers the shelf or the game, as far as the sounds care.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Overlay {
    #[default]
    None,
    PauseMenu,
    Switcher,
    QuickSettings,
    /// The power menu (long press of the power button), over everything else.
    PowerMenu,
}

/// The screen before and after an input: what covers it, and a fingerprint of everything that can move
/// (selection, page, values).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct View {
    pub overlay: Overlay,
    pub at: u64,
}

/// What happened. Taps and swipes on the touch screen arrive as the buttons they stand for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Input {
    Press(Button),
    Release(Button),
    /// A held direction stepped again.
    Repeat(Button),
    /// Vol+ or Vol- changed the volume (not brightness or colour with START or SELECT held).
    VolumeStep,
}

/// The sound for `input`, which turned `before` into `after`; `refused`: the input asked for something
/// that can't be done. None when nothing happened.
pub fn sound_for(input: Input, before: View, after: View, refused: bool) -> Option<Sound> {
    if refused {
        return Some(Sound::Error);
    }
    if input == Input::VolumeStep {
        return Some(Sound::Volume);
    }
    let chose = input == Input::Press(CONFIRM);
    if after.overlay != before.overlay {
        return Some(match (before.overlay, after.overlay) {
            (_, Overlay::Switcher) => Sound::Switcher,
            (_, Overlay::PowerMenu) => Sound::Menu,
            (Overlay::None, _) => Sound::Menu,
            _ if chose => Sound::Confirm,
            _ => Sound::Back,
        });
    }
    if after.at == before.at {
        return None;
    }
    match input {
        Input::Press(b) | Input::Repeat(b) if b == CONFIRM => Some(Sound::Confirm),
        Input::Press(b) if b == CANCEL => Some(Sound::Back),
        Input::Press(Button::Up | Button::Down | Button::Left | Button::Right | Button::L1 | Button::R1 | Button::L2 | Button::R2) => Some(Sound::Nav),
        Input::Repeat(_) => Some(Sound::Nav),
        Input::Press(_) => Some(Sound::Confirm),
        Input::Release(_) | Input::VolumeStep => None,
    }
}

static REFUSED: AtomicBool = AtomicBool::new(false);

/// The input being handled asked for something that can't be done: it gets the error sound.
pub fn refuse() {
    REFUSED.store(true, Ordering::Relaxed);
}

/// Whether [`refuse`] was called since the last time; clears it.
pub fn take_refused() -> bool {
    REFUSED.swap(false, Ordering::Relaxed)
}

/// What decides whether a sound may play at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ctx {
    pub config: Config,
    /// A game is running (pocket-core, RetroArch or a standalone emulator).
    pub in_game: bool,
    /// The pause menu is up (the game switcher in a game opens over it).
    pub pause_menu: bool,
    /// Light or deep sleep, or powering off.
    pub asleep: bool,
}

/// Whether a UI sound may play now: switched on, awake, and not over a game being played.
pub fn allowed(ctx: &Ctx) -> bool {
    ctx.config.on && ctx.config.volume > 0 && !ctx.asleep && (!ctx.in_game || ctx.pause_menu)
}

/// Whether the speaker plays anything, from the sink's volume in percent and mute (`sleep::sink_volume`):
/// not when muted or at 0. Unknown (no pactl) counts as audible.
pub fn audible(sink: Option<(u32, bool)>) -> bool {
    !matches!(sink, Some((volume, muted)) if muted || volume == 0)
}

/// Settings > Sounds: on or off, and how loud, separate from the system volume that games use.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Config {
    #[serde(default = "default_on")]
    pub on: bool,
    /// 1 to `MAX_VOLUME`.
    #[serde(default = "default_volume")]
    pub volume: u8,
}

pub const MAX_VOLUME: u8 = 10;

fn default_on() -> bool {
    true
}

fn default_volume() -> u8 {
    7
}

impl Default for Config {
    fn default() -> Self {
        Config { on: default_on(), volume: default_volume() }
    }
}

const CONFIG_FILE: &str = "sounds.json";

impl Config {
    pub fn load(data_dir: &Path) -> Self {
        crate::fsutil::read_json_or_backup(&data_dir.join(CONFIG_FILE)).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) {
        if let Err(e) = crate::fsutil::write_json(&data_dir.join(CONFIG_FILE), self) {
            eprintln!("ui-sound: can't save {CONFIG_FILE}: {e}");
        }
    }

    /// One step quieter (`delta` < 0) or louder, 1 to `MAX_VOLUME`.
    pub fn step(&mut self, delta: i32) {
        self.volume = (self.volume as i32 + delta.signum()).clamp(1, MAX_VOLUME as i32) as u8;
    }

    /// "Off", or the volume as "30 %".
    pub fn label(&self) -> String {
        if self.on {
            format!("{}%", self.volume as u32 * 100 / MAX_VOLUME as u32)
        } else {
            "Off".into()
        }
    }

    /// Factor on the samples: 2.5 dB a step below the top, so the bottom step (-22.5 dB) is still
    /// audible and the default (7, -7.5 dB) puts the -1.5 dBFS samples at about -9 dBFS, near a game's level.
    pub fn gain(&self) -> f32 {
        if self.volume == 0 {
            return 0.0;
        }
        10f32.powf(-((MAX_VOLUME - self.volume.min(MAX_VOLUME)) as f32) * 2.5 / 20.0)
    }
}

pub const RATE: u32 = crate::audio::OUT_RATE;

/// The sample files, mono 16-bit 48 kHz WAV (`assets/ui-sounds/`, made with ElevenLabs sound effects, docs/LICENCES.md).
/// Each is trimmed, faded in and out, and normalised to a peak of -1.5 dBFS (the nav tick -3 dBFS), so the
/// loudness of the whole set is decided by [`Config::gain`] alone.
fn wav(sound: Sound) -> &'static [u8] {
    match sound {
        Sound::Nav => include_bytes!("../assets/ui-sounds/nav.wav"),
        Sound::Confirm => include_bytes!("../assets/ui-sounds/confirm.wav"),
        Sound::Back => include_bytes!("../assets/ui-sounds/back.wav"),
        Sound::Menu => include_bytes!("../assets/ui-sounds/menu.wav"),
        Sound::Switcher => include_bytes!("../assets/ui-sounds/switcher.wav"),
        Sound::Error => include_bytes!("../assets/ui-sounds/error.wav"),
        Sound::Volume => include_bytes!("../assets/ui-sounds/volume.wav"),
        Sound::Shutter => include_bytes!("../assets/ui-sounds/shutter.wav"),
    }
}

/// The samples of `sound` (mono, `RATE`), at full UI volume.
pub fn synth(sound: Sound) -> Vec<i16> {
    synth_at(sound, RATE)
}

/// [`synth`] at another sample rate (linear interpolation; the sounds hold little above 8 kHz): pocket-core mixes the
/// shutter into the game's own sound at the core's rate.
pub fn synth_at(sound: Sound, rate: u32) -> Vec<i16> {
    let bytes = wav(sound);
    // The `data` chunk of a plain PCM file: after the 12-byte RIFF header, chunks of id + length.
    let mut at = 12;
    while at + 8 <= bytes.len() && &bytes[at..at + 4] != b"data" {
        at += 8 + u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
    }
    let data = bytes.get(at + 8..).unwrap_or(&[]);
    let pcm: Vec<i16> = data.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    if rate == RATE || pcm.is_empty() {
        return pcm;
    }
    let n = (pcm.len() as u64 * rate as u64 / RATE as u64) as usize;
    (0..n)
        .map(|i| {
            let pos = i as f64 * RATE as f64 / rate as f64;
            let (k, frac) = (pos as usize, pos.fract());
            let (a, b) = (pcm[k.min(pcm.len() - 1)] as f64, pcm[(k + 1).min(pcm.len() - 1)] as f64);
            (a + (b - a) * frac).round() as i16
        })
        .collect()
}

// ---- playing ----------------------------------------------------------------------------------------------

const ALL: [Sound; 8] = [Sound::Nav, Sound::Confirm, Sound::Back, Sound::Menu, Sound::Switcher, Sound::Error, Sound::Volume, Sound::Shutter];
/// How long the output stays open after the last sound, writing silence: moving through a list then
/// costs no reopening, and an idle shelf keeps no stream (and no PipeWire graph) running.
const KEEP_OPEN: Duration = Duration::from_secs(2);
/// Frames per write: 2.7 ms at 48 kHz, how soon a new sound can start while the output is open.
const CHUNK_FRAMES: usize = 128;
/// PipeWire's quantum while the output is open on the shelf (`POCKETUI_SOUND_QUANTUM`, 0 keeps ROCKNIX's 1024).
/// 256 is what pocket-core uses for games on this device without underruns (`audio::GAME_QUANTUM`).
const SHELF_QUANTUM: u32 = 256;
/// More at once is noise; the oldest one makes room.
const MAX_VOICES: usize = 4;
/// Ticks while a direction is held (about 14 a second) keep at least this far apart.
const TICK_GAP_MS: u64 = 40;
/// The mute state read before opening the output is trusted this long.
const MUTE_FRESH: Duration = Duration::from_millis(1500);
/// After the output couldn't be opened, the next try.
const RETRY_OPEN: Duration = Duration::from_secs(5);

enum Msg {
    /// Which sound, how loud, whether a game runs, and when the UI thread asked (for the numbers
    /// `POCKETUI_SOUND_DEBUG` prints).
    Play(Sound, f32, bool, Instant),
    Stop,
}

/// The quantum to lower PipeWire to while the output is open, if at all: on the shelf only (in a game
/// pocket-core already holds its own), and not when `configured` is 0.
fn shelf_quantum(in_game: bool, configured: Option<u32>) -> Option<u32> {
    let q = configured.unwrap_or(SHELF_QUANTUM);
    (!in_game && q > 0).then_some(q)
}

/// Raises and restores PipeWire's quantum in the order asked, on a thread of its own: `pw-metadata` and
/// ROCKNIX's `quantum` script take tens of milliseconds, which must not stand between two sounds. On a Bluetooth
/// sink ROCKNIX's quantum stays (`audio_route`: the small one only crackles there and shortens nothing), and nothing
/// is put back after it. Whether it is Bluetooth is asked here, beside the sound that already plays, and PipeWire only
/// when a Bluetooth link exists.
fn quantum_worker() -> SyncSender<Option<u32>> {
    let (tx, rx) = sync_channel::<Option<u32>>(8);
    let _ = std::thread::Builder::new().name("ui-sound-quantum".into()).spawn(move || {
        let mut lowered = false;
        // Once at the start, so the first sound already gets the right buffer (`ui_ring`).
        let speaker = || {
            let speaker = crate::audio_route::current(&crate::audio_route::Real) == crate::audio_route::Output::Speaker;
            ON_BLUETOOTH.store(!speaker, Ordering::Relaxed);
            speaker
        };
        speaker();
        for cmd in rx {
            match cmd {
                Some(frames) => {
                    if speaker() {
                        crate::audio::force_quantum(frames);
                        lowered = true;
                    }
                }
                None if std::mem::take(&mut lowered) => crate::audio::restore_quantum(),
                None => {}
            }
        }
    });
    tx
}

/// The mute check on a thread of its own: its answer comes back on the returned channel.
fn check_audible() -> Receiver<bool> {
    let (tx, rx) = sync_channel(1);
    let _ = std::thread::Builder::new().name("ui-sound-mute".into()).spawn(move || {
        let _ = tx.send(audible(crate::sleep::sink_volume()));
    });
    rx
}

static TX: OnceLock<SyncSender<Msg>> = OnceLock::new();
/// The sound went to Bluetooth when the quantum thread last asked (at the start and with every opening outside a game).
static ON_BLUETOOTH: AtomicBool = AtomicBool::new(false);

/// The ALSA buffer for the shelf's stream (`audio::Ring`): short only where the graph runs at a quantum of 256, the
/// shelf's own (`lowers`) or a game's on the speaker. Elsewhere (Bluetooth) a short one left part of every 1024-frame
/// cycle empty: the menu sounds came out "a bit distorted and quiet" on headphones (2026-10-27).
fn ui_ring(bluetooth: bool, lowers: bool, in_game: bool) -> crate::audio::Ring {
    if !bluetooth && (lowers || in_game) {
        crate::audio::Ring::Short
    } else {
        crate::audio::Ring::Cycle
    }
}
static LAST_TICK_MS: AtomicU64 = AtomicU64::new(0);

/// Starts the mixer thread, which renders all sounds right away. `play` starts it too; calling this at
/// start-up keeps the rendering off the first sound.
pub fn start() {
    TX.get_or_init(|| {
        let (tx, rx) = sync_channel(16);
        if let Err(e) = std::thread::Builder::new().name("ui-sound".into()).spawn(move || mixer(rx)) {
            eprintln!("ui-sound: no mixer thread: {e}");
        }
        tx
    });
}

/// Plays `sound` if [`allowed`] in `ctx`, at its UI volume. Returns at once.
pub fn play(sound: Sound, ctx: &Ctx) -> bool {
    if !allowed(ctx) {
        return false;
    }
    if matches!(sound, Sound::Nav | Sound::Volume) {
        static START: OnceLock<Instant> = OnceLock::new();
        // Offset by a second so that 0 means "no tick yet".
        let now = START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1000;
        if now < LAST_TICK_MS.load(Ordering::Relaxed) + TICK_GAP_MS {
            return false;
        }
        LAST_TICK_MS.store(now, Ordering::Relaxed);
    }
    start();
    TX.get().is_some_and(|tx| tx.try_send(Msg::Play(sound, ctx.config.gain(), ctx.in_game, Instant::now())).is_ok())
}

/// Plays `sound` over a game that runs: [`play`] refuses sounds there (in a game only the pause menu makes them), but
/// the shutter answers the player's own key press (`screenshot`). Same switch and volume (`config`) as every UI sound.
pub fn play_over_game(sound: Sound, config: Config) -> bool {
    play(sound, &Ctx { config, in_game: true, pause_menu: true, asleep: false })
}

/// Cuts what is playing and closes the output now (a game is starting and wants the device).
pub fn stop() {
    if let Some(tx) = TX.get() {
        let _ = tx.try_send(Msg::Stop);
    }
}

/// A sound being played: which one, how far, how loud.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Voice {
    sound: usize,
    pos: usize,
    gain: f32,
}

/// Fills `chunk` (interleaved stereo) with the voices and moves them on; finished ones are dropped.
fn mix(chunk: &mut [i16], voices: &mut Vec<Voice>, bank: &[Vec<i16>]) {
    for (i, frame) in chunk.chunks_exact_mut(2).enumerate() {
        let sum: f32 = voices.iter().filter_map(|v| bank[v.sound].get(v.pos + i).map(|s| *s as f32 * v.gain)).sum();
        let s = sum.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        frame[0] = s;
        frame[1] = s;
    }
    let frames = chunk.len() / 2;
    for v in voices.iter_mut() {
        v.pos += frames;
    }
    voices.retain(|v| v.pos < bank[v.sound].len());
}

/// Gives the calling thread a better share of the CPU than the UI thread that draws the shelf: at a quantum of
/// 256 frames the output wants its next chunk within 5 ms, and a frame of the pull-out animation can take longer
/// than that. Nice -10, not a real-time class: a bug here must not be able to starve the system.
/// Best effort (it needs root, as on the device); true when it worked.
#[cfg(target_os = "linux")]
fn favour_this_thread() -> bool {
    // SAFETY: gettid and setpriority take and return plain integers.
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, tid, MIXER_NICE) == 0
    }
}

#[cfg(not(target_os = "linux"))]
fn favour_this_thread() -> bool {
    false
}

#[cfg(target_os = "linux")]
const MIXER_NICE: libc::c_int = -10;

fn mixer(rx: Receiver<Msg>) {
    let bank: Vec<Vec<i16>> = ALL.iter().map(|s| synth(*s)).collect();
    if !favour_this_thread() {
        eprintln!("ui-sound: couldn't raise the mixer thread's priority");
    }
    let debug = std::env::var_os("POCKETUI_SOUND_DEBUG").is_some();
    let configured_quantum = std::env::var("POCKETUI_SOUND_QUANTUM").ok().and_then(|v| v.parse().ok());
    let quantum = quantum_worker();
    let mut out: Option<crate::audio::Sink> = None;
    // What the open stream was opened for (`ui_ring`): lowers the quantum, in a game.
    let mut opened_for = (false, false);
    // PipeWire's quantum is lowered for this output (and has to be restored when it closes).
    let mut lowered = false;
    let mut voices: Vec<Voice> = Vec::new();
    let mut chunk = vec![0i16; CHUNK_FRAMES * 2];
    let mut last_sound = Instant::now();
    // The last answer of the mute check, and one still on its way.
    let mut mute: Option<(Instant, bool)> = None;
    let mut checking: Option<Receiver<bool>> = None;
    let mut open_failed: Option<Instant> = None;
    // Closes the output and gives the quantum back.
    let close = |out: &mut Option<crate::audio::Sink>, lowered: &mut bool| {
        *out = None;
        if std::mem::take(lowered) {
            let _ = quantum.try_send(None);
        }
    };
    loop {
        // Closed: sleep until there is something to play. Open: take what came in since the last chunk.
        let first = if out.is_none() {
            match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => return,
            }
        } else {
            None
        };
        for msg in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
            let (sound, gain, in_game, asked) = match msg {
                Msg::Stop => {
                    voices.clear();
                    close(&mut out, &mut lowered);
                    continue;
                }
                Msg::Play(sound, gain, in_game, asked) => (sound, gain, in_game, asked),
            };
            let picked = asked.elapsed();
            let was_open = out.is_some();
            let mut open_ms = 0;
            take_answer(&mut checking, &mut mute);
            if out.is_none() {
                // Muted or at 0? Asked fresh, but beside the opening: a stale value can be wrong both ways (sounds
                // after a mute, or none for a while after Vol+ brought the volume back), and waiting for pactl
                // (two processes) before the first sound cost more than the stream does. A sound that starts and
                // finds the sink muted plays into a muted sink: silence.
                let muted_before = matches!(mute, Some((at, true)) if at.elapsed() < MUTE_FRESH);
                if muted_before || open_failed.is_some_and(|t| t.elapsed() < RETRY_OPEN) {
                    continue;
                }
                if !matches!(mute, Some((at, _)) if at.elapsed() < MUTE_FRESH) && checking.is_none() {
                    checking = Some(check_audible());
                }
                let t = Instant::now();
                opened_for = (shelf_quantum(in_game, configured_quantum).is_some(), in_game);
                out = crate::audio::Sink::open(ui_ring(ON_BLUETOOTH.load(Ordering::Relaxed), opened_for.0, opened_for.1));
                open_ms = t.elapsed().as_millis();
                match &out {
                    Some(s) if debug => println!("ui-sound: opened {} in {} ms", s.name(), open_ms),
                    Some(_) => {}
                    None => {
                        eprintln!("ui-sound: no audio output");
                        open_failed = Some(Instant::now());
                        continue;
                    }
                }
                if let Some(frames) = shelf_quantum(in_game, configured_quantum) {
                    lowered = quantum.try_send(Some(frames)).is_ok();
                }
            }
            if voices.len() >= MAX_VOICES {
                voices.remove(0);
            }
            voices.push(Voice { sound: ALL.iter().position(|s| *s == sound).unwrap_or(0), pos: 0, gain });
            last_sound = Instant::now();
            if debug {
                if let Some(o) = out.as_mut() {
                    println!(
                        "ui-sound: {sound:?} at gain {gain:.3}: thread took it after {} us ({}), open {open_ms} ms, {} us already queued",
                        picked.as_micros(),
                        if was_open { "output open" } else { "output closed" },
                        o.latency_us()
                    );
                }
            }
        }
        // The mute check came back: muted, so the output closes again.
        if take_answer(&mut checking, &mut mute) && mute.is_some_and(|(_, m)| m) && out.is_some() {
            voices.clear();
            close(&mut out, &mut lowered);
        }
        // The output moved to or from Bluetooth while open: a stream with the buffer for it. To the longer one at once
        // (the short one plays broken there), back to the short one between sounds.
        let want = ui_ring(ON_BLUETOOTH.load(Ordering::Relaxed), opened_for.0, opened_for.1);
        if out
            .as_ref()
            .and_then(|o| o.ring())
            .is_some_and(|r| r != want && (want == crate::audio::Ring::Cycle || voices.is_empty()))
        {
            if let Some(new) = crate::audio::Sink::open(want) {
                out = Some(new);
            }
        }
        let Some(o) = out.as_mut() else { continue };
        mix(&mut chunk, &mut voices, &bank);
        // Blocks until the output has room: this thread runs at the sound card's pace while open.
        if !o.write(&chunk) {
            voices.clear();
            close(&mut out, &mut lowered);
            continue;
        }
        if voices.is_empty() && last_sound.elapsed() > KEEP_OPEN {
            close(&mut out, &mut lowered);
            mute = None;
        }
    }
}

/// Takes the mute check's answer if it has come, into `mute`. True when it did.
fn take_answer(checking: &mut Option<Receiver<bool>>, mute: &mut Option<(Instant, bool)>) -> bool {
    let Some(rx) = checking.as_ref() else { return false };
    match rx.try_recv() {
        Ok(audible) => {
            *mute = Some((Instant::now(), !audible));
            *checking = None;
            true
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => false,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            *checking = None;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn at(at: u64) -> View {
        View { overlay: Overlay::None, at }
    }

    fn over(overlay: Overlay, at: u64) -> View {
        View { overlay, at }
    }

    #[test]
    fn the_shelf_lowers_pipewires_quantum_while_the_output_is_open_a_game_does_not() {
        // ROCKNIX's 1024 frames put one quantum of silence ahead of every sound; pocket-core's 256 (a game) is kept.
        assert_eq!(shelf_quantum(false, None), Some(SHELF_QUANTUM));
        assert_eq!(shelf_quantum(true, None), None, "pocket-core holds its own in a game");
        assert_eq!(shelf_quantum(false, Some(0)), None, "POCKETUI_SOUND_QUANTUM=0 keeps ROCKNIX's");
        assert_eq!(shelf_quantum(false, Some(512)), Some(512));
    }

    #[test]
    fn bluetooth_gets_the_buffer_for_a_whole_cycle() {
        use crate::audio::Ring;
        assert_eq!(ui_ring(false, true, false), Ring::Short, "the menu on the speaker: 256, as before");
        assert_eq!(ui_ring(false, false, true), Ring::Short, "in a game on the speaker: pocket-core's 256");
        assert_eq!(ui_ring(true, true, false), Ring::Cycle, "headphones: ROCKNIX's 1024 stays");
        assert_eq!(ui_ring(true, false, true), Ring::Cycle);
        assert_eq!(ui_ring(false, false, false), Ring::Cycle, "POCKETUI_SOUND_QUANTUM=0: 1024 runs");
    }

    #[test]
    fn a_write_is_short_enough_to_start_a_sound_within_three_milliseconds() {
        assert!(CHUNK_FRAMES as u32 * 1000 / RATE <= 3, "{CHUNK_FRAMES} frames");
    }

    #[test]
    fn the_mute_answer_is_taken_when_it_has_come() {
        let (tx, rx) = sync_channel(1);
        let (mut checking, mut mute) = (Some(rx), None);
        assert!(!take_answer(&mut checking, &mut mute) && checking.is_some() && mute.is_none(), "not yet");
        tx.send(false).unwrap();
        assert!(take_answer(&mut checking, &mut mute));
        assert!(checking.is_none() && mute.is_some_and(|(_, muted)| muted), "not audible = muted");
        assert!(!take_answer(&mut checking, &mut mute), "nothing pending any more");
    }

    #[test]
    fn moving_the_selection_ticks() {
        for b in [Button::Up, Button::Down, Button::Left, Button::Right, Button::L1, Button::R1] {
            assert_eq!(sound_for(Input::Press(b), at(1), at(2), false), Some(Sound::Nav), "{b:?}");
        }
        assert_eq!(sound_for(Input::Repeat(Button::Down), at(2), at(3), false), Some(Sound::Nav), "held");
    }

    #[test]
    fn nothing_moves_nothing_sounds() {
        // The end of a list, B on Home, a release.
        assert_eq!(sound_for(Input::Press(Button::Down), at(5), at(5), false), None);
        assert_eq!(sound_for(Input::Press(CANCEL), at(0), at(0), false), None);
        assert_eq!(sound_for(Input::Press(CONFIRM), at(0), at(0), false), None);
        assert_eq!(sound_for(Input::Release(Button::Down), at(5), at(5), false), None);
    }

    #[test]
    fn a_and_b_confirm_and_go_back() {
        assert_eq!(sound_for(Input::Press(CONFIRM), at(1), at(2), false), Some(Sound::Confirm));
        assert_eq!(sound_for(Input::Press(CANCEL), at(2), at(1), false), Some(Sound::Back));
        // Y stars a game.
        assert_eq!(sound_for(Input::Press(Button::West), at(1), at(2), false), Some(Sound::Confirm));
    }

    #[test]
    fn menus_and_the_switcher_have_their_own_sound() {
        // HOME tap in a game, HOME tap on the shelf, SELECT on the shelf: all on release.
        assert_eq!(sound_for(Input::Release(Button::Mode), at(1), over(Overlay::PauseMenu, 7), false), Some(Sound::Menu));
        assert_eq!(sound_for(Input::Release(Button::Mode), at(1), over(Overlay::QuickSettings, 7), false), Some(Sound::Menu));
        assert_eq!(sound_for(Input::Release(Button::Select), at(1), over(Overlay::Switcher, 7), false), Some(Sound::Switcher));
        // SELECT in the pause menu opens the switcher over it.
        assert_eq!(sound_for(Input::Press(Button::Select), over(Overlay::PauseMenu, 1), over(Overlay::Switcher, 7), false), Some(Sound::Switcher));
    }

    #[test]
    fn the_power_menu_sounds_like_a_menu_even_over_the_pause_menu() {
        assert_eq!(sound_for(Input::Press(Button::Power), over(Overlay::PauseMenu, 1), over(Overlay::PowerMenu, 7), false), Some(Sound::Menu));
        assert_eq!(sound_for(Input::Press(Button::Power), at(1), over(Overlay::PowerMenu, 7), false), Some(Sound::Menu));
        // B or the power button closes it: back. A on an entry: confirm.
        assert_eq!(sound_for(Input::Press(CANCEL), over(Overlay::PowerMenu, 3), over(Overlay::PauseMenu, 1), false), Some(Sound::Back));
        assert_eq!(sound_for(Input::Press(Button::Power), over(Overlay::PowerMenu, 3), at(1), false), Some(Sound::Back));
        assert_eq!(sound_for(Input::Press(CONFIRM), over(Overlay::PowerMenu, 3), at(1), false), Some(Sound::Confirm));
        // Moving inside it ticks.
        assert_eq!(sound_for(Input::Press(Button::Down), over(Overlay::PowerMenu, 3), over(Overlay::PowerMenu, 4), false), Some(Sound::Nav));
    }

    #[test]
    fn closing_a_menu_goes_back_unless_something_was_chosen() {
        assert_eq!(sound_for(Input::Press(CANCEL), over(Overlay::QuickSettings, 3), at(1), false), Some(Sound::Back));
        assert_eq!(sound_for(Input::Press(Button::Mode), over(Overlay::QuickSettings, 3), at(1), false), Some(Sound::Back));
        // B in the switcher opened from the pause menu: back to the menu.
        assert_eq!(sound_for(Input::Press(CANCEL), over(Overlay::Switcher, 3), over(Overlay::PauseMenu, 1), false), Some(Sound::Back));
        // A on a game in the switcher on the shelf.
        assert_eq!(sound_for(Input::Press(CONFIRM), over(Overlay::Switcher, 3), at(1), false), Some(Sound::Confirm));
    }

    #[test]
    fn the_volume_keys_tick() {
        assert_eq!(sound_for(Input::VolumeStep, at(1), at(1), false), Some(Sound::Volume));
        // Not when they changed the brightness (nothing on screen of ours moves either).
        assert_eq!(sound_for(Input::Press(Button::VolUp), at(1), at(1), false), None);
    }

    #[test]
    fn a_refusal_sounds_like_one() {
        // A on Load with an empty slot: nothing moves.
        assert_eq!(sound_for(Input::Press(CONFIRM), over(Overlay::PauseMenu, 1), over(Overlay::PauseMenu, 1), true), Some(Sound::Error));
        // Save changes failed, and the page changed back to the list anyway.
        assert_eq!(sound_for(Input::Press(CONFIRM), over(Overlay::PauseMenu, 1), over(Overlay::PauseMenu, 2), true), Some(Sound::Error));
    }

    fn shelf() -> Ctx {
        Ctx { config: Config::default(), in_game: false, pause_menu: false, asleep: false }
    }

    #[test]
    fn sounds_play_on_the_shelf_by_default() {
        assert!(allowed(&shelf()));
    }

    #[test]
    fn never_during_play_only_in_the_pause_menu() {
        assert!(!allowed(&Ctx { in_game: true, ..shelf() }), "playing");
        assert!(allowed(&Ctx { in_game: true, pause_menu: true, ..shelf() }), "paused");
    }

    #[test]
    fn not_asleep_or_switched_off() {
        assert!(!allowed(&Ctx { asleep: true, ..shelf() }));
        assert!(!allowed(&Ctx { asleep: true, in_game: true, pause_menu: true, ..shelf() }));
        assert!(!allowed(&Ctx { config: Config { on: false, ..Config::default() }, ..shelf() }));
        assert!(!allowed(&Ctx { config: Config { volume: 0, ..Config::default() }, ..shelf() }));
    }

    #[test]
    fn nothing_plays_into_a_muted_or_silent_speaker() {
        assert!(audible(Some((5, false))));
        assert!(!audible(Some((5, true))), "muted");
        assert!(!audible(Some((0, false))), "at 0");
        assert!(audible(None), "unknown: the sink itself still decides");
    }

    #[test]
    fn config_defaults_to_on_and_quiet_and_is_remembered() {
        let dir = TestDir::new("sounds");
        let c = Config::load(&dir);
        assert!(c.on, "on by default");
        assert!(c.volume >= 1 && c.volume < MAX_VOLUME, "audible but not at full UI volume by default: {}", c.volume);
        let chosen = Config { on: false, volume: 7 };
        chosen.save(&dir);
        assert_eq!(Config::load(&dir), chosen);
        std::fs::write(dir.join(CONFIG_FILE), "{\"on\": false}").unwrap();
        assert_eq!(Config::load(&dir), Config { on: false, volume: default_volume() }, "a missing field keeps its default");
    }

    #[test]
    fn volume_steps_stay_audible_and_in_range() {
        let mut c = Config { on: true, volume: 2 };
        c.step(-1);
        c.step(-1);
        assert_eq!(c.volume, 1, "the bottom step is still audible; Off is the switch");
        for _ in 0..20 {
            c.step(1);
        }
        assert_eq!(c.volume, MAX_VOLUME);
    }

    #[test]
    fn every_sound_is_short_and_starts_and_ends_silent() {
        for s in ALL {
            let pcm = synth(s);
            let ms = pcm.len() as u32 * 1000 / RATE;
            assert!(ms > 10 && ms <= 450, "{s:?}: {ms} ms");
            // No click: the files fade in over 2 ms and out over up to 30 ms, from and to exactly zero.
            let edge = RATE as usize / 1000;
            assert!(pcm[0].unsigned_abs() < 100, "{s:?} first {}", pcm[0]);
            for v in &pcm[pcm.len() - edge..] {
                assert!(v.unsigned_abs() < 2000, "{s:?} edge {v}");
            }
            let peak = pcm.iter().map(|v| v.unsigned_abs()).max().unwrap();
            assert!(peak > 16000 && peak <= 28500, "{s:?}: peak {peak} (-1 dBFS is 29204)");
        }
    }

    #[test]
    fn the_shutter_at_the_cores_rate_keeps_its_length_and_level() {
        let (a, b) = (synth(Sound::Shutter), synth_at(Sound::Shutter, 32_000));
        let ms = |n: usize, rate: u32| n as u32 * 1000 / rate;
        assert!(ms(a.len(), RATE).abs_diff(ms(b.len(), 32_000)) <= 1, "{} vs {}", a.len(), b.len());
        let peak = |v: &[i16]| v.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak(&b) as f32 > peak(&a) as f32 * 0.8, "{} vs {}", peak(&b), peak(&a));
    }

    #[test]
    fn the_sounds_differ_from_each_other() {
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(synth(*a), synth(*b), "{a:?} = {b:?}");
            }
        }
    }

    #[test]
    fn ui_volume_is_quieter_by_default_and_louder_by_step() {
        let gains: Vec<f32> = (1..=MAX_VOLUME).map(|volume| Config { on: true, volume }.gain()).collect();
        assert!(gains.windows(2).all(|w| w[0] < w[1]), "{gains:?}");
        assert_eq!(gains[MAX_VOLUME as usize - 1], 1.0);
        let default = Config::default().gain();
        assert!(default > 0.3 && default < 0.6, "{default}");
        assert_eq!(Config { on: true, volume: 0 }.gain(), 0.0);
    }

    #[test]
    fn the_mixer_adds_voices_clamps_and_drops_finished_ones() {
        let bank = vec![vec![1000i16; CHUNK_FRAMES + 44], vec![20000i16; 100]];
        let mut voices = vec![Voice { sound: 0, pos: 0, gain: 1.0 }, Voice { sound: 1, pos: 0, gain: 1.0 }, Voice { sound: 1, pos: 0, gain: 1.0 }];
        let mut chunk = vec![0i16; CHUNK_FRAMES * 2];
        mix(&mut chunk, &mut voices, &bank);
        assert_eq!(&chunk[..2], &[i16::MAX, i16::MAX], "41000 is clamped, both channels");
        assert_eq!(chunk[110 * 2], 1000, "the short ones have ended");
        assert_eq!(voices, vec![Voice { sound: 0, pos: CHUNK_FRAMES, gain: 1.0 }]);
        mix(&mut chunk, &mut voices, &bank);
        assert_eq!(chunk[43 * 2 + 1], 1000);
        assert_eq!(chunk[44 * 2], 0, "then silence");
        assert!(voices.is_empty());
    }

    #[test]
    fn refusals_are_taken_once() {
        refuse();
        assert!(take_refused());
        assert!(!take_refused());
    }
}
