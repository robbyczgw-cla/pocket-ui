//! Subtle UI sounds of pocket-ui: a tick as the selection moves, confirm, back, a menu opening, the game
//! switcher, a refusal, and a tick on Vol+/Vol-.
//!
//! - Which sound: `main.rs` takes a [`View`] of the screen before and after each input and asks
//!   [`sound_for`]; code that refuses something (Load on an empty slot) says so with [`refuse`].
//! - Whether at all: [`allowed`]. Never while a game is being played (in a game only the pause menu, and
//!   the switcher over it, make sounds), not asleep, not when switched off. Not muted either ([`audible`]),
//!   which the mixer thread asks the system itself, right before it opens the output.
//! - The sounds are sample files (`assets/ui-sounds/`, [`synth`] decodes them): warm, tactile hardware sounds
//!   made with ElevenLabs sound effects; the sounds of the cases and what goes into the console ([`Sound::CaseOpen`]
//!   on) are ElevenLabs sound effects too, cut and levelled by `scripts/ui-sounds/prep_media_sounds.py`
//!   (`media-sounds.tsv`: prompt and cut of each). The mixer thread decodes all of them
//!   once when it starts.
//! - The cases ([`opening`], [`going_in`]): X opens a game's case with its own sound, and the Cozy launch plays the
//!   case opening and the game going into the console as it goes in ([`hit_at`]), and pocket-core a CRT switching on
//!   with the picture's ([`Sound::CrtOn`]); Quick only a quiet click. They are queued at once with how long to wait ([`play_after`]), so
//!   nothing waits on the UI thread.
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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
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
    /// The Quick launch (`app_cabinet`): the cartridge or disc seated, a short click, played quietly ([`QUICK_DB`]).
    Insert,
    /// X on a game in a plastic case (keep case, jewel case, clamshell): the latch snaps, the lid knocks.
    CaseOpen,
    /// X on a game in a cardboard box (the cartridge systems): the tuck flap pulled open.
    BoxOpen,
    /// The Cozy launch: a cartridge slides into the slot and seats with a solid click.
    CartIn,
    /// The Cozy launch: a disc (or UMD) drops onto the spindle and the motor spins it up.
    DiscIn,
    /// The Cozy launch: a game card or memory card clicks into its port.
    CardIn,
    /// The Cozy launch's end: a CRT switched on, a thunk and a faint whine. pocket-core plays it into the game's sound
    /// as the picture's old-TV power-on starts (`crt_on`, its hook `crt-on`).
    CrtOn,
}

/// The sounds of the cases and what goes into the console, which Settings → Sound & feel → Case sounds
/// switches apart from the menu sounds (`cozy_extras`).
pub fn is_case_sound(sound: Sound) -> bool {
    matches!(sound, Sound::Insert | Sound::CaseOpen | Sound::BoxOpen | Sound::CartIn | Sound::DiscIn | Sound::CardIn | Sound::CrtOn)
}

/// How much quieter than the other UI sounds the Quick launch's click plays, in dB.
pub const QUICK_DB: f32 = -6.0;

/// The sound of opening the case `m` stands in: cardboard for a cartridge's box, plastic for every other case.
pub fn opening(m: crate::media::Medium) -> Sound {
    match crate::game_box::Hull::of(m) {
        crate::game_box::Hull::CartBox => Sound::BoxOpen,
        _ => Sound::CaseOpen,
    }
}

/// The sound of the game inside the case `m` (`Medium::game`) going into its console. Every kind named, so a new one
/// gets a choice.
pub fn going_in(m: crate::media::Medium) -> Sound {
    use crate::media::Game as G;
    match m.game() {
        G::Snes | G::SnesUs | G::N64 | G::Nes | G::Gb | G::Gbc | G::Gba | G::Cart | G::MegaDrive | G::MasterSystem | G::GameGear | G::HuCard
        | G::NeoGeoPocket | G::WonderSwan | G::Lynx => Sound::CartIn,
        G::Disc | G::MiniDisc | G::Umd => Sound::DiscIn,
        G::Card | G::Card3ds | G::Memory => Sound::CardIn,
    }
}

/// How far into `sound` what it stands for happens: the cartridge seats, the disc drops, the card or the Quick click
/// clicks (the file's loudest moment). The launch queues it that much before the animation's moment.
pub fn hit_at(sound: Sound) -> Duration {
    Duration::from_millis(match sound {
        Sound::CartIn => 604,
        Sound::DiscIn => 34,
        Sound::CardIn => 33,
        Sound::Insert => 30,
        _ => 0,
    })
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
/// The menu ones are trimmed, faded in and out, and normalised to a peak of -1.5 dBFS (the nav tick -3 dBFS); the media
/// sounds are levelled to the same loudness (`prep_media_sounds.py`: -24.5 to -29 LUFS at their loudest, true peak at
/// most -1.5 dBTP). So the
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
        Sound::Insert => include_bytes!("../assets/ui-sounds/seat.wav"),
        Sound::CaseOpen => include_bytes!("../assets/ui-sounds/case-open.wav"),
        Sound::BoxOpen => include_bytes!("../assets/ui-sounds/box-open.wav"),
        Sound::CartIn => include_bytes!("../assets/ui-sounds/cart-in.wav"),
        Sound::DiscIn => include_bytes!("../assets/ui-sounds/disc-in.wav"),
        Sound::CardIn => include_bytes!("../assets/ui-sounds/card-in.wav"),
        Sound::CrtOn => include_bytes!("../assets/ui-sounds/crt-on.wav"),
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

const ALL: [Sound; 15] = [
    Sound::Nav,
    Sound::Confirm,
    Sound::Back,
    Sound::Menu,
    Sound::Switcher,
    Sound::Error,
    Sound::Volume,
    Sound::Shutter,
    Sound::Insert,
    Sound::CaseOpen,
    Sound::BoxOpen,
    Sound::CartIn,
    Sound::DiscIn,
    Sound::CardIn,
    Sound::CrtOn,
];
/// The longest a sound may be asked to wait ([`play_after`]): a launch's whole sequence fits.
const MAX_AFTER: Duration = Duration::from_secs(3);
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
/// A closed warm stream polls a pending mute answer without waiting for another UI event.
const MUTE_PENDING: Duration = Duration::from_millis(100);

enum Msg {
    /// Which sound, how loud, whether a game runs, when the UI thread asked (for the numbers `POCKETUI_SOUND_DEBUG`
    /// prints), and how long after that it starts.
    Play(Sound, f32, bool, Instant, Duration),
    /// Cut, close, and leave the shelf: true gives ROCKNIX's quantum back (`stop_for`).
    Stop(bool),
    /// A game is starting: what plays ends, then the output closes; true to give PipeWire's quantum back then.
    CloseWhenDone(bool),
    /// A game ended and its quantum goes back (`game_ended`): through the quantum thread, in order with the shelf's.
    GameEnded,
    /// pocket-ui is in front with the screen on and sounds on (`keep_warm`): the output opens now and stays open.
    Warm(bool),
}

/// The quantum to lower PipeWire to while the output is open, if at all: on the shelf only (in a game
/// pocket-core already holds its own), and not when `configured` is 0.
fn shelf_quantum(in_game: bool, configured: Option<u32>) -> Option<u32> {
    let q = configured.unwrap_or(SHELF_QUANTUM);
    (!in_game && q > 0).then_some(q)
}

/// What the quantum thread is asked to do, in order.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Q {
    /// On the shelf (a sound outside a game): the graph runs at this quantum from now on (on the speaker), also while
    /// no sound plays, so it never changes under a sound.
    Shelf(u32),
    /// A game or an app starts: the shelf's quantum is off until it is back; true gives ROCKNIX's back (announced),
    /// false leaves the quantum to the game (a pocket-core game sets the same 256; pocket-ui gives it back at its end,
    /// `GameEnded`).
    Leave(bool),
    /// A game ended (pocket-core may have died before putting ROCKNIX's back): ROCKNIX's quantum back, then the shelf's
    /// at once if given, before the first sound on the shelf.
    GameEnded(Option<u32>),
    /// `watch_quantum` saw the graph forced to this: someone else changed it (ROCKNIX's wake script, a late restore).
    Seen(u32),
}

/// Before the quantum goes up the shelf's stream takes the longer buffer (`ui_ring`): `RAISING` is set, the mixer
/// reopens within a chunk, and only then does the quantum change. A short buffer under a 1024 cycle for even one
/// cycle is a gap.
const RAISE_LEAD: Duration = Duration::from_millis(25);
/// A quantum seen other than the shelf's is set again at most this often (ROCKNIX's script writes 1023, then 1024).
const REASSERT_EVERY: Duration = Duration::from_millis(300);

/// The only place pocket-ui changes PipeWire's quantum (pocket-core's `audio_route` sets it for its own game), on a
/// thread of its own: `pw-metadata` and ROCKNIX's `quantum` script take tens of milliseconds, which must not stand
/// between two sounds, and one thread keeps the order (2026-10-28: the restore after a game ran on a thread of its own
/// and could land after the shelf's 256). The shelf's quantum holds while pocket-ui is on the shelf, not only while a
/// sound plays: a quantum that changes under a sound leaves a part of a cycle out (the first click after quiet, measured
/// in scripts/audio-bench `ui-guest`, whatever the buffer). What `watch_quantum` sees is the truth: a change by someone
/// else is set right again. On a Bluetooth sink ROCKNIX's quantum stays (`audio_route`: the small one only crackles
/// there and shortens nothing); whether it is Bluetooth is asked here, and PipeWire only when a Bluetooth link exists.
fn quantum_worker() -> SyncSender<Q> {
    quantum_worker_with(crate::audio_route::Real, &RAISING)
}

fn quantum_worker_with<S: crate::audio_route::System>(sys: S, raising: &'static AtomicBool) -> SyncSender<Q> {
    quantum_worker_flags(
        sys,
        raising,
        &CHANGING,
        &FORCED_QUANTUM,
        &VOICES,
        RouteWatch { bluetooth: &ON_BLUETOOTH, shelf_requested: &SHELF_REQUESTED, every: crate::audio_route::CHECK_EVERY },
    )
}

struct RouteWatch {
    bluetooth: &'static AtomicBool,
    shelf_requested: &'static AtomicBool,
    every: Duration,
}

/// [`quantum_worker_with`] with the flags it sets and reads given (the tests' own).
fn quantum_worker_flags<S: crate::audio_route::System>(
    sys: S,
    raising: &'static AtomicBool,
    changing: &'static AtomicBool,
    seen: &'static AtomicU32,
    voices: &'static AtomicBool,
    route: RouteWatch,
) -> SyncSender<Q> {
    let (tx, rx) = sync_channel::<Q>(32);
    let _ = std::thread::Builder::new().name("ui-sound-quantum".into()).spawn(move || {
        // The shelf's quantum while on the shelf.
        let mut shelf: Option<u32> = None;
        // This thread forced the quantum and has to put ROCKNIX's back when leaving the shelf.
        let mut forced = false;
        let mut last_reassert: Option<Instant> = None;
        // Once at the start, so the first sound already gets the right buffer (`ui_ring`).
        let speaker = |sys: &S| {
            let speaker = crate::audio_route::current(sys) == crate::audio_route::Output::Speaker;
            route.bluetooth.store(!speaker, Ordering::Relaxed);
            speaker
        };
        speaker(&sys);
        let raise = |sys: &S, shelf_only: bool| {
            raising.store(true, Ordering::Relaxed);
            std::thread::sleep(RAISE_LEAD);
            // stop_for revokes periodic work before the launch prepares or spawns the game's own keeper. Check
            // again after the lead/wait: a route query can have begun while the UI was still on the shelf.
            let apply = !shelf_only || route.shelf_requested.load(Ordering::SeqCst);
            if apply { sys.restore(); }
            raising.store(false, Ordering::Relaxed);
            apply
        };
        let lower = |sys: &S, frames: u32, forced: &mut bool, shelf_only: bool| {
            if speaker(sys) && (!shelf_only || route.shelf_requested.load(Ordering::SeqCst)) {
                *forced |= sys.force(frames);
            }
        };
        // A change: new sounds wait from now (`CHANGING`), the ones playing are let end (`QUIET_WAIT`), then the change,
        // until `watch_quantum` has seen `to` (or `SEEN_WAIT` has passed: no pw-metadata).
        let change = |to: Option<u32>, work: &mut dyn FnMut()| {
            changing.store(true, Ordering::Relaxed);
            let t0 = Instant::now();
            while voices.load(Ordering::Relaxed) && t0.elapsed() < QUIET_WAIT {
                std::thread::sleep(Duration::from_millis(2));
            }
            work();
            let t0 = Instant::now();
            while to.is_some_and(|q| seen.load(Ordering::Relaxed) != q) && t0.elapsed() < SEEN_WAIT {
                std::thread::sleep(Duration::from_millis(2));
            }
            changing.store(false, Ordering::Relaxed);
        };
        let mut route_due = Instant::now() + route.every;
        loop {
            // A warm stream may stay open for hours: a new default sink need not change the graph's quantum or
            // send a mixer command. Follow the route here, on the worker, even when nobody touches the shelf.
            let cmd = match rx.recv_timeout(route_due.saturating_duration_since(Instant::now())) {
                Ok(cmd) => Some(cmd),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            let asked = cmd.is_some_and(|q| !matches!(q, Q::Seen(_)));
            if let Some(cmd) = cmd {
                match cmd {
                    Q::Shelf(frames) => {
                        if shelf != Some(frames) {
                            shelf = Some(frames);
                            change(Some(frames), &mut || lower(&sys, frames, &mut forced, false));
                        }
                    }
                    Q::Leave(restore) => {
                        shelf = None;
                        if std::mem::take(&mut forced) && restore {
                            raise(&sys, false);
                        }
                    }
                    // The shelf's quantum already runs (a pocket-core game's 256, which pocket-ui puts back, not
                    // pocket-core: `POCKET_CORE_QUANTUM_OWNER`): nothing to change, so nothing changes under a sound.
                    Q::GameEnded(Some(f)) if seen.load(Ordering::Relaxed) == f && speaker(&sys) => {
                        shelf = Some(f);
                        forced = true;
                    }
                    Q::GameEnded(frames) => {
                        shelf = frames;
                        change(frames, &mut || {
                            raise(&sys, false);
                            forced = false;
                            if let Some(f) = frames {
                                lower(&sys, f, &mut forced, false);
                            }
                        });
                    }
                    Q::Seen(q) => {
                        let off = shelf.is_some_and(|f| f != q) && !raising.load(Ordering::Relaxed);
                        if off && last_reassert.is_none_or(|t| t.elapsed() > REASSERT_EVERY) {
                            last_reassert = Some(Instant::now());
                            if let Some(f) = shelf {
                                change(Some(f), &mut || lower(&sys, f, &mut forced, false));
                            }
                        }
                    }
                }
            }
            // The mixer marked the request as a change when it sent it (its `ask`): done now, whatever was done.
            if asked {
                changing.store(false, Ordering::Relaxed);
            }
            if Instant::now() >= route_due {
                route_due = Instant::now() + route.every;
                if let Some(frames) = shelf.filter(|_| route.shelf_requested.load(Ordering::SeqCst)) {
                    let on_speaker = speaker(&sys);
                    if on_speaker && !forced {
                        change(Some(frames), &mut || lower(&sys, frames, &mut forced, true));
                    } else if !on_speaker && forced {
                        change(None, &mut || {
                            if raise(&sys, true) { forced = false; }
                        });
                    }
                }
            }
        }
    });
    tx
}

/// The quantum PipeWire is forced to (`clock.force-quantum`), as last seen: `u32::MAX` while unknown (no
/// `pw-metadata`, not read yet), 0 when nothing forces one.
static FORCED_QUANTUM: AtomicU32 = AtomicU32::new(u32::MAX);
/// The quantum thread is about to raise the quantum (`RAISE_LEAD`).
static RAISING: AtomicBool = AtomicBool::new(false);
/// The quantum thread is changing the quantum and its change hasn't been seen yet: a new sound waits (`CHANGE_WAIT`)
/// instead of playing through it. PipeWire leaves a part of a cycle out of whatever plays while the quantum changes,
/// whatever the buffer (scripts/audio-bench `ui-guest`: the click at a game's end).
static CHANGING: AtomicBool = AtomicBool::new(false);
/// How long a change waits for the sounds that play to end (`VOICES`), and how long it waits to be seen.
const QUIET_WAIT: Duration = Duration::from_millis(500);
const SEEN_WAIT: Duration = Duration::from_millis(250);
/// How long a sound waits for a quantum change at most: the sounds before it, then the change.
const CHANGE_WAIT: Duration = Duration::from_millis(900);
/// The mixer has sounds playing: a quantum change waits for them (`QUIET_WAIT`), so it never lands under one.
static VOICES: AtomicBool = AtomicBool::new(false);

/// `clock.force-quantum`'s value in a line of `pw-metadata [-m] -n settings 0 clock.force-quantum`:
/// `update: id:0 key:'clock.force-quantum' value:'256' type:''`.
fn forced_quantum_of(line: &str) -> Option<u32> {
    if !line.contains("key:'clock.force-quantum'") {
        return None;
    }
    line.split("value:'").nth(1)?.split('\'').next()?.parse().ok()
}

/// Follows `clock.force-quantum` into `FORCED_QUANTUM`, whoever changes it: pocket-core for its game, the shelf,
/// ROCKNIX's wake script, a standalone emulator. `pw-metadata -m` prints the value at once and every change after,
/// blocking in between (no polling). When it ends (PipeWire restarted) the value is unknown until it runs again.
fn watch_quantum(tell: SyncSender<Q>) {
    let _ = std::thread::Builder::new().name("ui-sound-graph".into()).spawn(move || loop {
        let child = std::process::Command::new("pw-metadata")
            .args(["-m", "-n", "settings", "0", "clock.force-quantum"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            // No PipeWire tools (a desktop without them): unknown, so the longer buffer, which is right everywhere.
            return;
        };
        if let Some(out) = child.stdout.take() {
            use std::io::BufRead;
            for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                if let Some(q) = forced_quantum_of(&line) {
                    FORCED_QUANTUM.store(q, Ordering::Relaxed);
                    let _ = tell.try_send(Q::Seen(q));
                }
            }
        }
        let _ = child.wait();
        FORCED_QUANTUM.store(u32::MAX, Ordering::Relaxed);
        std::thread::sleep(Duration::from_secs(2));
    });
}

/// The mute check on a thread of its own: its answer comes back on the returned channel.
fn check_audible() -> Receiver<bool> {
    let (tx, rx) = sync_channel(1);
    let _ = std::thread::Builder::new().name("ui-sound-mute".into()).spawn(move || {
        let _ = tx.send(audible(crate::platform::sink_volume()));
    });
    rx
}

static TX: OnceLock<SyncSender<Msg>> = OnceLock::new();
/// The sound went to Bluetooth when the quantum thread last asked (at the start and with every opening outside a game).
static ON_BLUETOOTH: AtomicBool = AtomicBool::new(false);
/// Revoked synchronously before a game starts; delayed route polls must not restore over its audio keeper.
static SHELF_REQUESTED: AtomicBool = AtomicBool::new(true);

/// The ALSA buffer for the shelf's stream (`audio::Ring`): short only while the graph is seen forced to a quantum of
/// 256 or less (`FORCED_QUANTUM`) and isn't about to go up (`raising`). A short buffer under a 1024-frame cycle leaves
/// part of every cycle empty: menu sounds "a bit distorted and quiet" on headphones (2026-10-27), and "distorted for a
/// while" after a standalone game (2026-10-28), when the stream had been opened in its pause menu on the belief that
/// any game holds 256 and stayed open on the shelf. What is seen decides, not what anyone believes.
fn ui_ring(bluetooth: bool, forced: u32, raising: bool) -> crate::audio::Ring {
    if !bluetooth && !raising && (1..=256).contains(&forced) {
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
    play_after(sound, ctx, Duration::ZERO, 0.0)
}

/// [`play`], `after` from now (at most `MAX_AFTER`) and `db` louder (below 0: quieter) than the UI volume: queued at
/// once, the mixer thread starts it on time (to a chunk, 2.7 ms) and keeps the output open for it.
pub fn play_after(sound: Sound, ctx: &Ctx, after: Duration, db: f32) -> bool {
    if !allowed(ctx) || (is_case_sound(sound) && !crate::cozy_extras::on(crate::cozy_extras::Extra::CaseSounds)) {
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
    let gain = ctx.config.gain() * 10f32.powf(db.min(0.0) / 20.0);
    TX.get().is_some_and(|tx| tx.try_send(Msg::Play(sound, gain, ctx.in_game, Instant::now(), after.min(MAX_AFTER))).is_ok())
}

static PLAYED: AtomicBool = AtomicBool::new(false);

/// The input being handled played its own sound already (X opening a case, A starting the launch's sounds): the
/// generic one for it ([`sound_for`]) stays out.
pub fn played() {
    PLAYED.store(true, Ordering::Relaxed);
}

/// Whether [`played`] was called since the last time; clears it.
pub fn take_played() -> bool {
    PLAYED.swap(false, Ordering::Relaxed)
}

/// Plays `sound` over a game that runs: [`play`] refuses sounds there (in a game only the pause menu makes them), but
/// the shutter answers the player's own key press (`screenshot`). Same switch and volume (`config`) as every UI sound.
pub fn play_over_game(sound: Sound, config: Config) -> bool {
    play(sound, &Ctx { config, in_game: true, pause_menu: true, asleep: false })
}

/// A game is starting while a sound of its start plays (the insert click): the output closes once it has ended. A game
/// of pocket-core's keeps the quantum the output lowered (`restore_quantum` false): it sets the same one itself, and
/// giving it back now could undo pocket-core's own after it was set. pocket-ui gives it back when the game ends.
pub fn close_when_done(restore_quantum: bool) {
    if let Some(tx) = TX.get() {
        let _ = tx.try_send(Msg::CloseWhenDone(restore_quantum));
    }
}

/// Whether pocket-ui is in front (no game or app running), awake with the screen on, and its sounds on: then the output
/// stays open, writing silence, so the first sound after a quiet spell starts at once. Opening it costs 20 to 60 ms on a
/// Mini V2 and 100 to 220 ms on an H700 (PipeWire's stream setup), and a speaker sink suspended after 5 s of quiet takes
/// up to 0.8 s more to wake there (scripts/ui-sounds `ui_open_bench`). Called every turn of the event loop; only a
/// change is sent.
pub fn keep_warm(warm: bool) {
    // Reconcile the route fence every turn, even without a Warm transition: a next game's missing ROM may have
    // returned to the shelf without game_ended, while sleep or Sounds Off must revoke periodic route work.
    SHELF_REQUESTED.store(warm, Ordering::SeqCst);
    static WARM: AtomicBool = AtomicBool::new(false);
    if WARM.swap(warm, Ordering::Relaxed) != warm {
        start();
        if let Some(tx) = TX.get() {
            if tx.try_send(Msg::Warm(warm)).is_err() {
                // Full queue: asked again next turn.
                WARM.store(!warm, Ordering::Relaxed);
            }
        }
    }
}

/// Cuts what is playing and closes the output now (a game or an app is starting and wants the device), and gives
/// ROCKNIX's quantum back.
pub fn stop() {
    stop_for(true);
}

/// [`stop`] for a game: `restore` false for pocket-core, which sets the shelf's 256 itself, so nothing changes under
/// the launch animation's click (pocket-ui gives it back when the game ends, `game_ended`).
pub fn stop_for(restore: bool) {
    SHELF_REQUESTED.store(false, Ordering::SeqCst);
    if let Some(tx) = TX.get() {
        // Never waits on the UI thread; the queue holds 16 and the mixer empties it every chunk.
        let _ = tx.try_send(Msg::Stop(restore));
    }
}

/// A game ended: ROCKNIX's quantum back (pocket-core sets it back itself, but may have died first). Through the
/// quantum thread, so the shelf's own quantum, asked for by the next sound, comes after it and not before.
pub fn game_ended() {
    SHELF_REQUESTED.store(true, Ordering::SeqCst);
    start();
    match TX.get() {
        // Never waits on the UI thread: a full queue (a stuck mixer) falls back to the restore alone.
        Some(tx) if tx.try_send(Msg::GameEnded).is_ok() => {}
        _ => {
            std::thread::spawn(crate::audio::restore_quantum);
        }
    }
}

/// A sound being played: which one, how far, how loud, and how many frames of silence before it starts.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Voice {
    sound: usize,
    pos: usize,
    gain: f32,
    wait: usize,
}

/// Fills `chunk` (interleaved stereo) with the voices and moves them on; finished ones are dropped.
fn mix(chunk: &mut [i16], voices: &mut Vec<Voice>, bank: &[Vec<i16>]) {
    for (i, frame) in chunk.chunks_exact_mut(2).enumerate() {
        let sum: f32 = voices
            .iter()
            .filter_map(|v| (v.pos + i).checked_sub(v.wait).and_then(|at| bank[v.sound].get(at)).map(|s| *s as f32 * v.gain))
            .sum();
        let s = sum.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        frame[0] = s;
        frame[1] = s;
    }
    let frames = chunk.len() / 2;
    for v in voices.iter_mut() {
        v.pos += frames;
    }
    voices.retain(|v| v.pos < v.wait + bank[v.sound].len());
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

/// The mixer's stream operations, kept small so its real lifecycle can be tested with a silent in-memory sink.
trait MixerOutput {
    fn ring(&self) -> Option<crate::audio::Ring>;
    fn write(&mut self, samples: &[i16]) -> bool;
    fn name(&self) -> &'static str;
    fn latency_us(&mut self) -> u64;
}

impl MixerOutput for crate::audio::Sink {
    fn ring(&self) -> Option<crate::audio::Ring> { self.ring() }
    fn write(&mut self, samples: &[i16]) -> bool { self.write(samples) }
    fn name(&self) -> &'static str { self.name() }
    fn latency_us(&mut self) -> u64 { self.latency_us() }
}

fn mixer(rx: Receiver<Msg>) {
    if !favour_this_thread() {
        eprintln!("ui-sound: couldn't raise the mixer thread's priority");
    }
    let configured_quantum = std::env::var("POCKETUI_SOUND_QUANTUM").ok().and_then(|v| v.parse().ok());
    let quantum = quantum_worker();
    watch_quantum(quantum.clone());
    mixer_with(rx, crate::audio::Sink::open, check_audible, quantum, configured_quantum, MUTE_FRESH);
}

fn mixer_with<O: MixerOutput>(
    rx: Receiver<Msg>,
    open: impl Fn(crate::audio::Ring) -> Option<O>,
    check_audible: impl Fn() -> Receiver<bool>,
    quantum: SyncSender<Q>,
    configured_quantum: Option<u32>,
    mute_fresh: Duration,
) {
    let bank: Vec<Vec<i16>> = ALL.iter().map(|s| synth(*s)).collect();
    let debug = std::env::var_os("POCKETUI_SOUND_DEBUG").is_some();
    // Asks the quantum thread for a change. New sounds wait from this moment (`CHANGING`), not from when the thread
    // takes it up: a sound let in meanwhile played through the change, or was lost when the output reopened for it.
    // The thread clears it when it is done, also when there was nothing to change.
    let ask = |q: Q| {
        CHANGING.store(true, Ordering::Relaxed);
        let sent = quantum.try_send(q).is_ok();
        if !sent {
            CHANGING.store(false, Ordering::Relaxed);
        }
        sent
    };
    let ring_now = || ui_ring(ON_BLUETOOTH.load(Ordering::Relaxed), FORCED_QUANTUM.load(Ordering::Relaxed), RAISING.load(Ordering::Relaxed));
    let mut out: Option<O> = None;
    // Sounds held back while the quantum changes (`CHANGING`), and since when.
    let mut waiting: Vec<Voice> = Vec::new();
    let mut waiting_since: Option<Instant> = None;
    // The quantum thread knows pocket-ui is on the shelf (`Q::Shelf`) since the last game or app. pocket-ui starts on
    // the shelf (`start` runs at launch): its quantum before the first sound, not under it. A game it continues at
    // once stops the sounds first (`stop`), which gives ROCKNIX's back.
    let mut on_shelf = shelf_quantum(false, configured_quantum).is_some_and(|f| ask(Q::Shelf(f)));
    let mut voices: Vec<Voice> = Vec::new();
    let mut chunk = vec![0i16; CHUNK_FRAMES * 2];
    let mut last_sound = Instant::now();
    // The last answer of the mute check, and one still on its way.
    let mut mute: Option<(Instant, bool)> = None;
    let mut checking: Option<Receiver<bool>> = None;
    let mut open_failed: Option<Instant> = None;
    // A game is starting: close once the voices have ended, giving the quantum back or not (`close_when_done`).
    let mut closing: Option<bool> = None;
    // Closes the output; the shelf's quantum stays (`Q::Shelf`).
    let close = |out: &mut Option<O>| *out = None;
    // The output is kept open (`keep_warm`): no closing after `KEEP_OPEN`.
    let mut warm = false;
    loop {
        // A warm stream outlives any individual sound. Recheck mute while it is open too, so muting an idle shelf
        // releases the stream and lets the sink suspend. A closed warm stream keeps checking for an unmute.
        if warm && checking.is_none() && !matches!(mute, Some((at, _)) if at.elapsed() < mute_fresh) {
            checking = Some(check_audible());
        }
        // Closed: sleep until there is something to play. Open: take what came in since the last chunk.
        let first = if out.is_none() {
            match closed_message(&rx, warm, checking.is_some(), mute_fresh) {
                Ok(m) => Some(m),
                Err(_) => return,
            }
        } else {
            None
        };
        for msg in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
            let (sound, gain, in_game, asked, after) = match msg {
                Msg::Stop(restore) => {
                    voices.clear();
                    waiting.clear();
                    // Kept warm and open: closed (and the quantum left) once the launch's own sounds, queued right
                    // after this, have played, as `close_when_done` does; closing now and opening again for them cost
                    // 100 to 300 ms on an H700 while the game starts.
                    if std::mem::take(&mut warm) && out.is_some() {
                        closing = Some(restore);
                        continue;
                    }
                    closing = None;
                    close(&mut out);
                    // A game or an app starts: ROCKNIX's quantum for it, or pocket-core's (the same as the shelf's).
                    ask(Q::Leave(restore));
                    on_shelf = false;
                    continue;
                }
                Msg::GameEnded => {
                    let frames = shelf_quantum(false, configured_quantum);
                    ask(Q::GameEnded(frames));
                    on_shelf = frames.is_some();
                    continue;
                }
                Msg::CloseWhenDone(restore) => {
                    warm = false;
                    closing = Some(restore);
                    continue;
                }
                Msg::Warm(w) => {
                    warm = w;
                    take_answer(&mut checking, &mut mute);
                    // A stale muted answer still waits for its fresh replacement before opening speculatively.
                    let muted = mute.is_some_and(|(_, muted)| muted);
                    if w && out.is_none() && closing.is_none() && !muted && !open_failed.is_some_and(|t| t.elapsed() < RETRY_OPEN) {
                        // Opened ahead of the first sound, with the mute check beside it as for a sound.
                        if checking.is_none() {
                            checking = Some(check_audible());
                        }
                        let t = Instant::now();
                        out = open(ring_now());
                        match &out {
                            Some(s) if debug => println!("ui-sound: kept warm, opened {} in {} ms", s.name(), t.elapsed().as_millis()),
                            Some(_) => {}
                            None => open_failed = Some(Instant::now()),
                        }
                        if out.is_some() && !on_shelf {
                            if let Some(frames) = shelf_quantum(false, configured_quantum) {
                                on_shelf = ask(Q::Shelf(frames));
                            }
                        }
                        last_sound = Instant::now();
                    }
                    continue;
                }
                Msg::Play(sound, gain, in_game, asked, after) => (sound, gain, in_game, asked, after),
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
                let muted_before = matches!(mute, Some((at, true)) if at.elapsed() < mute_fresh);
                if muted_before || open_failed.is_some_and(|t| t.elapsed() < RETRY_OPEN) {
                    continue;
                }
                if !matches!(mute, Some((at, _)) if at.elapsed() < mute_fresh) && checking.is_none() {
                    checking = Some(check_audible());
                }
                let t = Instant::now();
                out = open(ring_now());
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
            }
            // On the shelf (also with the output still open from a game's pause menu): its own quantum, once; the
            // quantum thread keeps it from then on.
            if !on_shelf {
                if let Some(frames) = shelf_quantum(in_game, configured_quantum) {
                    on_shelf = ask(Q::Shelf(frames));
                }
            }
            // The wait counts from when it was asked: the time it took to get here (opening the output) is part of it.
            let wait = (RATE as u128 * after.saturating_sub(asked.elapsed()).as_micros() / 1_000_000) as usize;
            let voice = Voice { sound: ALL.iter().position(|s| *s == sound).unwrap_or(0), pos: 0, gain, wait };
            // The quantum is changing (right after a game): the sound waits for it rather than play through it.
            if CHANGING.load(Ordering::Relaxed) {
                waiting.push(voice);
                waiting_since.get_or_insert_with(Instant::now);
            } else {
                if voices.len() >= MAX_VOICES {
                    voices.remove(0);
                }
                voices.push(voice);
            }
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
        take_answer(&mut checking, &mut mute);
        if mute.is_some_and(|(_, m)| m) && out.is_some() {
            voices.clear();
            waiting.clear();
            close(&mut out);
        }
        VOICES.store(!voices.is_empty(), Ordering::Relaxed);
        // Sounds that waited for a quantum change: on when it is done, or after `CHANGE_WAIT` whatever happens.
        if !waiting.is_empty() && (!CHANGING.load(Ordering::Relaxed) || waiting_since.is_some_and(|t| t.elapsed() > CHANGE_WAIT)) {
            for v in waiting.drain(..) {
                if voices.len() >= MAX_VOICES {
                    voices.remove(0);
                }
                voices.push(v);
            }
            waiting_since = None;
        }
        // The graph's quantum changed (or the output moved to or from Bluetooth) while open: a stream with the buffer
        // for it. To the longer one at once (the short one plays broken there), back to the short one between sounds.
        let want = ring_now();
        if out
            .as_ref()
            .and_then(|o| o.ring())
            .is_some_and(|r| r != want && (want == crate::audio::Ring::Cycle || voices.is_empty()))
        {
            if let Some(new) = open(want) {
                out = Some(new);
            }
        }
        let Some(o) = out.as_mut() else {
            // Nothing open (muted, no output): nothing to close later, but the game's quantum all the same.
            if let Some(restore) = closing.take() {
                ask(Q::Leave(restore));
                on_shelf = false;
            }
            continue;
        };
        mix(&mut chunk, &mut voices, &bank);
        // Blocks until the output has room: this thread runs at the sound card's pace while open.
        if !o.write(&chunk) {
            voices.clear();
            close(&mut out);
            continue;
        }
        if let Some(restore) = closing.filter(|_| voices.is_empty() && waiting.is_empty()) {
            closing = None;
            close(&mut out);
            ask(Q::Leave(restore));
            on_shelf = false;
            continue;
        }
        if voices.is_empty() && last_sound.elapsed() > KEEP_OPEN && !warm {
            close(&mut out);
            mute = None;
        }
    }
}

/// Closed outputs sleep indefinitely outside the shelf. A warm one wakes to finish a mute query, notice an unmute
/// or retry after a stream failure; `keep_warm` sends only transitions, so waiting for another UI event loses warmth.
fn closed_message(rx: &Receiver<Msg>, warm: bool, checking: bool, retry: Duration) -> Result<Msg, std::sync::mpsc::RecvError> {
    if !warm {
        return rx.recv();
    }
    match rx.recv_timeout(if checking { MUTE_PENDING.min(retry) } else { retry }) {
        Ok(msg) => Ok(msg),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(Msg::Warm(true)),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(std::sync::mpsc::RecvError),
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
    fn the_buffer_follows_the_quantum_the_graph_is_seen_at() {
        use crate::audio::Ring;
        assert_eq!(ui_ring(false, 256, false), Ring::Short, "the shelf's or a pocket-core game's 256 on the speaker");
        assert_eq!(ui_ring(false, 128, false), Ring::Short);
        assert_eq!(ui_ring(false, 1024, false), Ring::Cycle, "ROCKNIX's 1024: a standalone game, the shelf before it lowers");
        assert_eq!(ui_ring(false, 0, false), Ring::Cycle, "nothing forced: the graph picks, so no short buffer");
        assert_eq!(ui_ring(false, u32::MAX, false), Ring::Cycle, "not known (no pw-metadata): the buffer right everywhere");
        assert_eq!(ui_ring(false, 256, true), Ring::Cycle, "about to go up: the longer buffer first");
        assert_eq!(ui_ring(true, 256, false), Ring::Cycle, "headphones");
    }

    #[test]
    fn the_forced_quantum_is_read_from_pw_metadata() {
        assert_eq!(forced_quantum_of("update: id:0 key:'clock.force-quantum' value:'256' type:''"), Some(256));
        assert_eq!(forced_quantum_of("update: id:0 key:'clock.force-quantum' value:'0' type:''"), Some(0));
        assert_eq!(forced_quantum_of("update: id:0 key:'clock.quantum' value:'1024' type:''"), None, "another key");
        assert_eq!(forced_quantum_of("Found \"settings\" metadata 29"), None);
        assert_eq!(forced_quantum_of("update: id:0 key:'clock.force-quantum' value:'' type:''"), None, "removed");
    }

    /// A system for the quantum thread: no Bluetooth, and a log of what was set, with whether the buffer had been
    /// told to go long first.
    #[derive(Clone, Default)]
    struct Graph(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

    static TEST_RAISING: AtomicBool = AtomicBool::new(false);

    impl crate::audio_route::System for Graph {
        fn connections(&self) -> Option<Vec<String>> {
            Some(Vec::new())
        }
        fn default_sink(&self) -> Option<String> {
            None
        }
        fn force(&self, frames: u32) -> bool {
            self.0.lock().unwrap().push(format!("force {frames}"));
            true
        }
        fn restore(&self) {
            let long_first = TEST_RAISING.load(Ordering::Relaxed);
            self.0.lock().unwrap().push(format!("restore{}", if long_first { "" } else { " UNANNOUNCED" }));
        }
    }

    /// 2026-10-28: the restore after a game ran on a thread of its own and could land after the shelf had lowered the
    /// quantum, a raise came with no warning to the short buffer, and the shelf's quantum changed under the first sound
    /// after quiet. Now one thread does all of it in the order asked, every raise is announced, the shelf's quantum is
    /// set again right after a game (before any sound) and holds while on the shelf, and a change by someone else is
    /// set right.
    #[test]
    fn the_quantum_changes_in_order_and_every_raise_is_announced() {
        let g = Graph::default();
        static CHANGING_T: AtomicBool = AtomicBool::new(false);
        static SEEN_T: AtomicU32 = AtomicU32::new(u32::MAX);
        static VOICES_T: AtomicBool = AtomicBool::new(false);
        static BLUETOOTH_T: AtomicBool = AtomicBool::new(false);
        static SHELF_T: AtomicBool = AtomicBool::new(true);
        let tx = quantum_worker_flags(
            g.clone(),
            &TEST_RAISING,
            &CHANGING_T,
            &SEEN_T,
            &VOICES_T,
            RouteWatch { bluetooth: &BLUETOOTH_T, shelf_requested: &SHELF_T, every: crate::audio_route::CHECK_EVERY },
        );
        let steps = [
            Q::Shelf(256),
            Q::Shelf(256),
            Q::GameEnded(Some(256)),
            Q::Leave(true),
            Q::Leave(true),
            Q::GameEnded(None),
            Q::Shelf(256),
            Q::Seen(256),
            Q::Seen(1024),
        ];
        for q in steps {
            tx.send(q).unwrap();
        }
        drop(tx);
        let want = ["force 256", "restore", "force 256", "restore", "restore", "force 256", "force 256"];
        let t0 = Instant::now();
        while g.0.lock().unwrap().len() < want.len() && t0.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            *g.0.lock().unwrap(),
            want,
            "the shelf once; a game's end: restore, then the shelf's 256 at once; leaving restores once; a game ending \
             off the shelf only restores; 1024 seen on the shelf is set right, 256 seen is left"
        );
        assert!(!TEST_RAISING.load(Ordering::Relaxed), "not left raising");
    }

    #[test]
    fn the_idle_shelf_follows_bluetooth_without_a_sound_or_a_quantum_event() {
        use std::sync::{Arc, Mutex};
        static RAISING_T: AtomicBool = AtomicBool::new(false);
        static CHANGING_T: AtomicBool = AtomicBool::new(false);
        static SEEN_T: AtomicU32 = AtomicU32::new(256);
        static VOICES_T: AtomicBool = AtomicBool::new(false);
        static BLUETOOTH_T: AtomicBool = AtomicBool::new(false);
        static SHELF_T: AtomicBool = AtomicBool::new(true);
        #[derive(Clone, Default)]
        struct RouteGraph {
            connected: Arc<AtomicBool>,
            events: Arc<Mutex<Vec<&'static str>>>,
        }
        impl crate::audio_route::System for RouteGraph {
            fn connections(&self) -> Option<Vec<String>> {
                Some(if self.connected.load(Ordering::Relaxed) {
                    vec!["hci0:1".into()]
                } else {
                    Vec::new()
                })
            }
            fn default_sink(&self) -> Option<String> {
                Some("bluez_output.AA_BB.1".into())
            }
            fn force(&self, _: u32) -> bool {
                self.events.lock().unwrap().push("force");
                true
            }
            fn restore(&self) {
                self.events
                    .lock()
                    .unwrap()
                    .push(if RAISING_T.load(Ordering::Relaxed) { "restore" } else { "UNANNOUNCED" });
            }
        }
        let graph = RouteGraph::default();
        let tx = quantum_worker_flags(
            graph.clone(),
            &RAISING_T,
            &CHANGING_T,
            &SEEN_T,
            &VOICES_T,
            RouteWatch { bluetooth: &BLUETOOTH_T, shelf_requested: &SHELF_T, every: Duration::from_millis(20) },
        );
        let wait_for = |events: usize| {
            let start = Instant::now();
            while graph.events.lock().unwrap().len() < events && start.elapsed() < Duration::from_secs(2) {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(graph.events.lock().unwrap().len(), events);
        };
        tx.send(Q::Shelf(256)).unwrap();
        wait_for(1);
        graph.connected.store(true, Ordering::Relaxed);
        wait_for(2);
        assert!(BLUETOOTH_T.load(Ordering::Relaxed), "the mixer must use the long Bluetooth buffer");
        graph.connected.store(false, Ordering::Relaxed);
        wait_for(3);
        assert!(!BLUETOOTH_T.load(Ordering::Relaxed));
        assert_eq!(*graph.events.lock().unwrap(), ["force", "restore", "force"]);
        // The route changes again just as a game starts. stop_for revokes periodic work synchronously, even if
        // the quantum worker has already started its 25 ms raise lead and has not read Leave yet.
        graph.connected.store(true, Ordering::Relaxed);
        let start = Instant::now();
        while !RAISING_T.load(Ordering::Relaxed) && start.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(RAISING_T.load(Ordering::Relaxed));
        SHELF_T.store(false, Ordering::SeqCst);
        tx.send(Q::Leave(false)).unwrap();
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(graph.events.lock().unwrap().len(), 3, "a running game owns route changes now");
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
    fn a_closed_warm_stream_retries_without_another_ui_event() {
        let (tx, rx) = sync_channel(16);
        // Initial mute, a failed open or a failed write: the UI still considers the shelf warm and sends no new
        // Warm(true). The mixer must wake itself to consume the pending answer and open after an unmute/recovery.
        assert!(matches!(closed_message(&rx, true, true, MUTE_FRESH), Ok(Msg::Warm(true))));
        tx.send(Msg::Stop(false)).unwrap();
        assert!(matches!(closed_message(&rx, true, true, MUTE_FRESH), Ok(Msg::Stop(false))), "a stop wins over the retry timer");
        tx.send(Msg::Warm(false)).unwrap();
        assert!(matches!(closed_message(&rx, false, false, MUTE_FRESH), Ok(Msg::Warm(false))));
        drop(tx);
        assert!(closed_message(&rx, true, false, MUTE_FRESH).is_err(), "no retry after the sender has gone");
    }

    #[test]
    fn the_mixer_releases_muted_warm_streams_and_recovers_without_ui_input() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicUsize;
        #[derive(Default)]
        struct State {
            audible: AtomicBool,
            active: AtomicUsize,
            opens: AtomicUsize,
            fail_write: AtomicBool,
        }
        struct SilentSink(Arc<State>);
        impl MixerOutput for SilentSink {
            fn ring(&self) -> Option<crate::audio::Ring> { None }
            fn name(&self) -> &'static str { "silent test stream" }
            fn latency_us(&mut self) -> u64 { 0 }
            fn write(&mut self, _: &[i16]) -> bool {
                std::thread::sleep(Duration::from_millis(1));
                !self.0.fail_write.swap(false, Ordering::SeqCst)
            }
        }
        impl Drop for SilentSink {
            fn drop(&mut self) {
                self.0.active.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let state = Arc::new(State::default());
        let (tx, rx) = sync_channel(16);
        let (quantum, _quantum_rx) = sync_channel(32);
        let s = state.clone();
        let mixer = std::thread::spawn(move || {
            let checking = s.clone();
            mixer_with(rx, move |_| {
                s.active.fetch_add(1, Ordering::SeqCst);
                s.opens.fetch_add(1, Ordering::SeqCst);
                Some(SilentSink(s.clone()))
            }, move || {
                let (tx, rx) = sync_channel(1);
                tx.send(checking.audible.load(Ordering::SeqCst)).unwrap();
                rx
            }, quantum, Some(0), Duration::from_millis(20));
        });
        let wait_for = |predicate: &dyn Fn() -> bool| {
            let start = Instant::now();
            while !predicate() && start.elapsed() < Duration::from_secs(2) {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(predicate());
        };
        // Start already muted: the initial speculative stream closes once its mute answer arrives.
        tx.send(Msg::Warm(true)).unwrap();
        wait_for(&|| state.opens.load(Ordering::SeqCst) > 0 && state.active.load(Ordering::SeqCst) == 0);
        state.audible.store(true, Ordering::SeqCst);
        wait_for(&|| state.active.load(Ordering::SeqCst) == 1);
        // Now mute an existing warm stream, with no further Warm or Play messages.
        state.audible.store(false, Ordering::SeqCst);
        wait_for(&|| state.active.load(Ordering::SeqCst) == 0);
        state.audible.store(true, Ordering::SeqCst);
        wait_for(&|| state.active.load(Ordering::SeqCst) == 1);
        let opened = state.opens.load(Ordering::SeqCst);
        state.fail_write.store(true, Ordering::SeqCst);
        wait_for(&|| state.opens.load(Ordering::SeqCst) > opened && state.active.load(Ordering::SeqCst) == 1);
        // End while closed: disconnecting the sender must let the mixer finish, rather than retain its test stream.
        state.audible.store(false, Ordering::SeqCst);
        wait_for(&|| state.active.load(Ordering::SeqCst) == 0);
        drop(tx);
        mixer.join().unwrap();
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
            // A disc spinning up and a CRT coming on take longer than a click; still under a second.
            // The media sounds (a slide before the click, a disc spinning up) take longer than a menu tick.
            let longest = if matches!(s, Sound::CaseOpen | Sound::BoxOpen | Sound::CartIn | Sound::DiscIn | Sound::CardIn | Sound::CrtOn) { 1200 } else { 450 };
            assert!(ms > 10 && ms <= longest, "{s:?}: {ms} ms");
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
        let v = |sound| Voice { sound, pos: 0, gain: 1.0, wait: 0 };
        let mut voices = vec![v(0), v(1), v(1)];
        let mut chunk = vec![0i16; CHUNK_FRAMES * 2];
        mix(&mut chunk, &mut voices, &bank);
        assert_eq!(&chunk[..2], &[i16::MAX, i16::MAX], "41000 is clamped, both channels");
        assert_eq!(chunk[110 * 2], 1000, "the short ones have ended");
        assert_eq!(voices, vec![Voice { sound: 0, pos: CHUNK_FRAMES, gain: 1.0, wait: 0 }]);
        mix(&mut chunk, &mut voices, &bank);
        assert_eq!(chunk[43 * 2 + 1], 1000);
        assert_eq!(chunk[44 * 2], 0, "then silence");
        assert!(voices.is_empty());
    }

    #[test]
    fn a_voice_asked_to_wait_starts_that_many_frames_later_and_then_plays_whole() {
        let bank = vec![vec![500i16; 100]];
        let mut voices = vec![Voice { sound: 0, pos: 0, gain: 1.0, wait: CHUNK_FRAMES + 10 }];
        let mut chunk = vec![0i16; CHUNK_FRAMES * 2];
        mix(&mut chunk, &mut voices, &bank);
        assert!(chunk.iter().all(|s| *s == 0), "still waiting");
        assert_eq!(voices.len(), 1, "a waiting voice is kept");
        mix(&mut chunk, &mut voices, &bank);
        assert_eq!((chunk[9 * 2], chunk[10 * 2], chunk[109 * 2], chunk[110 * 2]), (0, 500, 500, 0), "frames 10 to 109");
        assert!(voices.is_empty());
    }

    #[test]
    fn every_case_has_an_opening_and_every_game_a_way_in() {
        use crate::media::Medium as M;
        assert_eq!(opening(M::Snes), Sound::BoxOpen, "the cartridge systems' cardboard box");
        assert_eq!(opening(M::Gba), Sound::BoxOpen);
        for m in [M::MegaDrive, M::Ps1, M::Ps2, M::Umd, M::Ds, M::Xbox] {
            assert_eq!(opening(m), Sound::CaseOpen, "{m:?}: plastic");
        }
        assert_eq!(going_in(M::N64), Sound::CartIn);
        assert_eq!(going_in(M::MegaDrive), Sound::CartIn, "a cartridge in a plastic case");
        assert_eq!(going_in(M::GameCube), Sound::DiscIn);
        assert_eq!(going_in(M::Umd), Sound::DiscIn);
        assert_eq!(going_in(M::Vita), Sound::CardIn);
    }

    /// The launch queues the sound `hit_at` before the moment the animation shows: the loudest moment of the file is
    /// right there (the click), not before it (the slide) or long after.
    #[test]
    fn the_hit_is_where_hit_at_says() {
        for s in [Sound::CartIn, Sound::DiscIn, Sound::CardIn, Sound::Insert] {
            let pcm = synth(s);
            let loudest = pcm.iter().enumerate().max_by_key(|(_, v)| v.unsigned_abs()).unwrap().0;
            let ms = loudest as f32 * 1000.0 / RATE as f32;
            let hit = hit_at(s).as_secs_f32() * 1000.0;
            assert!(ms >= hit && ms < hit + 12.0, "{s:?}: loudest at {ms:.1} ms, hit_at {hit} ms");
        }
    }

    #[test]
    fn the_quick_click_is_quieter_and_a_wait_is_bounded() {
        assert!(10f32.powf(QUICK_DB / 20.0) < 0.6);
        assert!(MAX_AFTER >= Duration::from_secs(2) && MAX_AFTER <= Duration::from_secs(5));
    }

    #[test]
    fn a_sound_played_by_hand_keeps_the_generic_one_out_once() {
        played();
        assert!(take_played());
        assert!(!take_played());
    }

    #[test]
    fn refusals_are_taken_once() {
        refuse();
        assert!(take_refused());
        assert!(!take_refused());
    }
}
