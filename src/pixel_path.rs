//! Pixel Path (docs/CURATOR.md): the chain a game's frame and its sound take from the emulator to the panel and the
//! speaker, stage by stage, each one graded. Roon's Signal Path, for games.
//!
//! - pocket-core measures ([`Report`]) and writes it to [`FILE`] when pocket-ui asks (`pixelpath <seq>` on the command
//!   FIFO), at most a few times a second and only while the pause menu's Pixel Path page is open. What it reads then
//!   already exists: the frame times and core times it keeps for its exit report, and the half-second CPU and frame
//!   counts of its FPS counter ([`Window`], one sample every 0.5 s). Nothing new runs per frame.
//! - pocket-ui turns a report into the stages ([`path`]) and paints them (`app_pixel_path.rs`).
//!
//! Grades, from best to worst (the overall grade is the worst stage):
//! - **Pristine**: pixel-perfect and native.
//! - **Faithful**: close to the original; something small differs (a 0.45 % speed-up, resampled sound).
//! - **Enhanced**: deliberately more than the original (a higher internal resolution, run-ahead, a shader that adds).
//! - **Compromised**: something is lost (late frames, sound gaps, a stretched or blurred picture where whole pixels fit).
//!
//! Everything measured is shown as measured; the latency is an estimate from the pipeline and says so (≈).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::video::{dest_rect, Scaling, Sharpness};

/// Where pocket-core leaves the report for the pause menu.
pub const FILE: &str = "/tmp/pocket-core.pixelpath.json";
/// The command pocket-ui sends, followed by a sequence number the report carries back.
pub const COMMAND: &str = "pixelpath";
/// How often the open page asks for a new report.
pub const REFRESH_MS: u64 = 250;
/// The stretch of play the live numbers cover.
pub const WINDOW_SECS: f32 = 5.0;

/// The picture: what the core made and how it is drawn.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Source {
    /// The core's base size (`retro_game_geometry`).
    pub base_w: u32,
    pub base_h: u32,
    /// The size of the last frame, as uploaded (a 3D core's internal resolution).
    pub frame_w: u32,
    pub frame_h: u32,
    pub fps: f64,
    /// The aspect ratio the picture is drawn at (0: the frame's own shape).
    pub aspect: f32,
    /// A 3D core rendering with its own GL context.
    pub hw: bool,
    /// Quarter turns the core asked for (vertical arcade games).
    pub rotation: u32,
    /// Quarter turns of Options → Picture → Screen orientation, on top (`video::Orientation::turns`).
    pub turn: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Picture {
    pub view_w: u32,
    pub view_h: u32,
    /// Where the picture is on the panel.
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// The Picture settings' values (`options.rs`): "native", "aspect", …; "sharp", "crisp", "soft".
    pub scaling: String,
    pub sharpness: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Look {
    /// "none", "grid", "scanlines", "crt".
    pub effect: String,
    /// "none", "auto", "ambient", "shell", "crt", "arcade", "glow", or one of the handheld/TV frames of
    /// `frames_more.rs`/`frames_tv.rs` ("brick", "pocket", …, "woodgrain"; the values of the Frame option).
    pub frame: String,
    /// Colour temperature and contrast (`display::Look`).
    pub warmth: i8,
    pub contrast: i8,
    /// The core's own colour options in effect, as "label: value" (colour correction, LCD blending).
    pub colour: Vec<String>,
    /// The LCD grid draws the Game Boy Advance screen's colours (`VideoSettings::lcd_colours`).
    pub lcd_colour: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Display {
    pub refresh_hz: f64,
    /// The core's rate is far from the panel's: frames are scheduled by time (50 Hz games), not one per refresh.
    pub time_driven: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RunAhead {
    /// The system is one run-ahead is offered for.
    pub offered: bool,
    /// Frames the setting asks for.
    pub wanted: u32,
    /// Frames in use now.
    pub frames: u32,
    /// Switched on, but not looked at yet (it starts with the next refresh of play).
    pub pending: bool,
    /// Why it switched itself off (the notice), empty when it didn't.
    pub off: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Audio {
    pub backend: String,
    /// The core's own sample rate.
    pub core_rate: f64,
    /// The rate the samples are fed at (the core's, nudged to the panel's refresh).
    pub in_rate: f64,
    pub out_rate: u32,
    /// The queue the rate control steers towards, and what is in it and in the output now.
    pub target_ms: f32,
    pub queue_ms: f32,
    pub server_ms: f32,
    /// Underruns in the whole session.
    pub underruns: u64,
    /// The sound goes to a Bluetooth sink (`audio_route`): the codec and the radio add a delay nobody here measures.
    pub bluetooth: bool,
}

/// The last stretch of play: up to `WINDOW_SECS`, the game's own time only (pauses don't count).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Live {
    pub secs: f32,
    /// Pictures put on the screen, and how many per second.
    pub frames: u32,
    pub fps: f32,
    /// Refreshes a picture came too late for (it stayed one more refresh).
    pub late: u32,
    /// Emulated frames per second (differs from `fps` for 50 Hz games).
    pub emulated_fps: f32,
    /// Time the core takes for a refresh's frames, mean and worst.
    pub core_ms: f32,
    pub core_ms_max: f32,
    /// pocket-core's CPU, all its threads, in percent of one core.
    pub cpu: f32,
    pub underruns: u32,
}

/// What pocket-core knows about the running game's path, written on request.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Report {
    /// The request this answers.
    pub seq: u32,
    /// The ROM as pocket-ui passed it, so the menu reads the report of its own game only.
    pub rom: String,
    pub system: String,
    pub core: String,
    pub core_version: String,
    pub source: Source,
    pub picture: Picture,
    pub look: Look,
    pub display: Display,
    pub run_ahead: RunAhead,
    /// None: no sound output could be opened.
    pub audio: Option<Audio>,
    pub live: Live,
    /// The game is paused (the pause menu): the live numbers are the last seconds before.
    pub paused: bool,
    /// The patches pocket-core applied to the game (`softpatch`), by file name; empty for the game as it is.
    pub patches: Vec<String>,
}

impl Report {
    /// The report for `rom` with sequence number `seq`, if pocket-core wrote it.
    pub fn read(path: &std::path::Path, rom: &std::path::Path, seq: u32) -> Option<Report> {
        let r: Report = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        (r.seq == seq && std::path::Path::new(&r.rom) == rom).then_some(r)
    }

    /// Written next to the file and renamed over it, so a reader never sees half a report. No fsync: it lives in
    /// /tmp (RAM) and only matters while the page is open.
    pub fn write(&self, path: &std::path::Path) {
        let tmp = path.with_extension("json.tmp");
        let written = serde_json::to_vec(self)
            .map_err(std::io::Error::other)
            .and_then(|bytes| std::fs::write(&tmp, bytes))
            .and_then(|()| std::fs::rename(&tmp, path));
        if let Err(e) = written {
            eprintln!("pixel path: can't write {}: {e}", path.display());
        }
    }

    /// The same report apart from its sequence number (the page repaints only when something changed).
    pub fn same_as(&self, other: &Report) -> bool {
        Report { seq: 0, ..self.clone() } == Report { seq: 0, ..other.clone() }
    }
}

// ---- the live window (pocket-core) ----------------------------------------------------------------------

/// One half-second of play, as the FPS counter sees it (`overlay::Hud::frame`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tick {
    pub secs: f64,
    pub frames: u32,
    pub emulated: u32,
    /// CPU time of the whole process in this half-second.
    pub cpu_secs: f64,
}

/// The last few seconds of the FPS counter's half-second samples, with the underruns of each. Ten entries; a push
/// every 0.5 s of play.
#[derive(Default)]
pub struct Window {
    ticks: VecDeque<(Tick, u32)>,
    underruns_seen: u64,
}

impl Window {
    /// Adds a half-second; `underruns` is the session's total so far.
    pub fn push(&mut self, t: Tick, underruns: u64) {
        let new = underruns.saturating_sub(self.underruns_seen) as u32;
        self.underruns_seen = underruns;
        self.ticks.push_back((t, new));
        let mut secs: f64 = self.ticks.iter().map(|(t, _)| t.secs).sum();
        while self.ticks.len() > 1 && secs - self.ticks[0].0.secs >= WINDOW_SECS as f64 {
            secs -= self.ticks[0].0.secs;
            self.ticks.pop_front();
        }
    }

    /// Emulated frames per second, CPU in percent of one core, underruns: over the window.
    pub fn summary(&self) -> (f32, f32, u32) {
        let secs: f64 = self.ticks.iter().map(|(t, _)| t.secs).sum();
        if secs <= 0.0 {
            return (0.0, 0.0, 0);
        }
        let emulated: u32 = self.ticks.iter().map(|(t, _)| t.emulated).sum();
        let cpu: f64 = self.ticks.iter().map(|(t, _)| t.cpu_secs).sum();
        let under = self.ticks.iter().map(|(_, u)| u).sum();
        ((emulated as f64 / secs) as f32, (cpu / secs * 100.0) as f32, under)
    }
}

/// The frame numbers of the last `secs` of play from pocket-core's frame-time lists: `intervals` between swaps
/// and `run_ms` per refresh (both in ms, oldest first). Returns (pictures, fps, late refreshes, core ms mean, max,
/// seconds covered).
pub fn frame_window(intervals: &[f32], run_ms: &[f32], period_ms: f32, secs: f32) -> (u32, f32, u32, f32, f32, f32) {
    let mut sum = 0.0f32;
    let mut n = 0usize;
    for iv in intervals.iter().rev() {
        if sum + iv > secs * 1000.0 && n > 0 {
            break;
        }
        sum += iv;
        n += 1;
    }
    // Refreshes in the window that brought no new picture: the one before stayed on screen (a stutter). Counted from
    // the time covered rather than per interval, so a late frame followed by a quick one (the limiter catching up)
    // and a virtual device without vsync both come out right.
    let period = period_ms.max(1.0);
    let late = ((sum / period).round() as u32).saturating_sub(n as u32);
    let runs = &run_ms[run_ms.len().saturating_sub(n.max(1))..];
    let core = if runs.is_empty() { 0.0 } else { runs.iter().sum::<f32>() / runs.len() as f32 };
    let core_max = runs.iter().copied().fold(0.0, f32::max);
    let fps = if sum > 0.0 { n as f32 / (sum / 1000.0) } else { 0.0 };
    (n as u32, fps, late, core, core_max, sum / 1000.0)
}

/// The core options that change a picture's colours or blend frames, as "label: value" when they are on (the Look
/// stage). Colour correction and LCD ghosting imitate a handheld's screen; they are the core's, not ours.
pub fn colour_options(options: &[crate::core_options::FileOption]) -> Vec<String> {
    const KEYS: [&str; 5] = ["color_correction", "colour_correction", "interframe_blending", "frame_blending", "lcd_ghosting"];
    const OFF: [&str; 7] = ["disabled", "off", "none", "false", "0", "no", ""];
    options
        .iter()
        .filter(|o| KEYS.iter().any(|k| o.key.to_ascii_lowercase().contains(k)))
        .filter(|o| !OFF.contains(&o.current.to_ascii_lowercase().as_str()))
        .map(|o| format!("{}: {}", o.label, o.label_of(&o.current)))
        .collect()
}

/// Whether the core's own colour correction is on (mGBA's or gambatte's "Color Correction"): the LCD grid's GBA colours
/// would do it a second time, so they step back (`VideoSettings::core_colour`).
pub fn corrects_colour(options: &[crate::core_options::FileOption]) -> bool {
    const OFF: [&str; 6] = ["disabled", "off", "none", "false", "0", ""];
    options.iter().any(|o| {
        let key = o.key.to_ascii_lowercase();
        (key.ends_with("color_correction") || key.ends_with("colour_correction")) && !OFF.contains(&o.current.to_ascii_lowercase().as_str())
    })
}

/// The Picture settings' values as `options.rs` writes them.
pub fn scaling_value(s: Scaling) -> &'static str {
    match s {
        Scaling::Native => "native",
        Scaling::IntegerSmaller => "integer-smaller",
        Scaling::Aspect => "aspect",
        Scaling::Fullscreen => "fullscreen",
    }
}

pub fn sharpness_value(s: Sharpness) -> &'static str {
    match s {
        Sharpness::Sharp => "sharp",
        Sharpness::Crisp => "crisp",
        Sharpness::Soft => "soft",
    }
}

pub fn effect_value(e: crate::video::Effect) -> &'static str {
    use crate::video::Effect;
    match e {
        Effect::None => "none",
        Effect::Grid => "grid",
        Effect::Scanlines => "scanlines",
        Effect::Crt => "crt",
    }
}

/// The frame only counts where it shows (`VideoSettings::shows_frame`).
pub fn frame_value(v: &crate::video::VideoSettings) -> &'static str {
    use crate::video::FrameMode;
    if !v.shows_frame() {
        return "none";
    }
    match v.frame {
        FrameMode::None => "none",
        FrameMode::Auto => "auto",
        FrameMode::Ambient => "ambient",
        FrameMode::Shell => "shell",
        FrameMode::Crt => "crt",
        FrameMode::Arcade => "arcade",
        FrameMode::Glow => "glow",
        FrameMode::Brick => "brick",
        FrameMode::PocketSlim => "pocket",
        FrameMode::Clamshell => "clamshell",
        FrameMode::Wide => "wide",
        FrameMode::Twin => "twin",
        FrameMode::Gloss => "gloss",
        FrameMode::Swan => "swan",
        FrameMode::Watch => "watch",
        FrameMode::Goggles => "goggles",
        FrameMode::Studio => "studio",
        FrameMode::PortableTv => "portable-tv",
        FrameMode::Woodgrain => "woodgrain",
    }
}

// ---- grading (pocket-ui) --------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Grade {
    Pristine,
    Faithful,
    Enhanced,
    Compromised,
    /// Not graded: a standalone emulator's stage its settings alone don't decide (`pixel_standalone`).
    Unknown,
}

impl Grade {
    pub const SCALE: [Grade; 4] = [Grade::Pristine, Grade::Faithful, Grade::Enhanced, Grade::Compromised];

    pub fn name(self) -> &'static str {
        match self {
            Grade::Pristine => "Pristine",
            Grade::Faithful => "Faithful",
            Grade::Enhanced => "Enhanced",
            Grade::Compromised => "Compromised",
            Grade::Unknown => "Not known",
        }
    }

    /// The colour on the pause menu's dark page: muted, readable on black, never alarming.
    pub fn colour(self) -> u32 {
        match self {
            Grade::Pristine => 0xA88CEB,
            Grade::Faithful => 0x6FC28F,
            Grade::Enhanced => 0x62A6E6,
            Grade::Compromised => 0xE59E5A,
            Grade::Unknown => 0x5A5852,
        }
    }

    /// The legend's words.
    pub fn meaning(self) -> &'static str {
        match self {
            Grade::Pristine => "pixel-perfect, native",
            Grade::Faithful => "close to the original",
            Grade::Enhanced => "more than the original",
            Grade::Compromised => "something is lost",
            Grade::Unknown => "not known here",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Core,
    Source,
    Scale,
    Look,
    Display,
    Latency,
    Audio,
}

impl Kind {
    pub const ALL: [Kind; 7] = [Kind::Core, Kind::Source, Kind::Scale, Kind::Look, Kind::Display, Kind::Latency, Kind::Audio];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Core => "Core",
            Kind::Source => "Source",
            Kind::Scale => "Scale",
            Kind::Look => "Look",
            Kind::Display => "Display",
            Kind::Latency => "Latency",
            Kind::Audio => "Audio",
        }
    }
}

/// One stage of the chain as the page shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Stage {
    pub kind: Kind,
    pub grade: Grade,
    /// The facts, short: "240×160 · 59.73 Hz · square pixels".
    pub value: String,
    /// One line: what it means.
    pub line: String,
    /// A paragraph for A.
    pub about: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub stages: Vec<Stage>,
    pub overall: Grade,
    /// "Last 5 s of play · 59.9 fps · …", None when nothing was measured.
    pub live: Option<String>,
    /// Some stages have no grade (a standalone emulator's, read from its settings: `pixel_standalone`): the
    /// heading says "Partly graded".
    pub partial: bool,
}

/// The worst grade of the known ones (Unknown when none is known).
pub fn overall(grades: impl IntoIterator<Item = Grade>) -> Grade {
    grades.into_iter().filter(|g| *g != Grade::Unknown).max().unwrap_or(Grade::Unknown)
}

fn stage(kind: Kind, grade: Grade, value: impl Into<String>, line: impl Into<String>, about: impl Into<String>) -> Stage {
    Stage {
        kind,
        grade,
        value: value.into(),
        line: line.into(),
        about: about.into(),
    }
}

/// What pocket-ui adds that pocket-core doesn't know: the system's name, the cheats switched on in the menu.
#[derive(Clone, Debug, Default)]
pub struct Known {
    pub system_name: String,
    pub cheats: usize,
}

/// The chain of a pocket-core game.
pub fn path(r: &Report, known: &Known) -> Path {
    let stages = vec![
        core_stage(r, known),
        source_stage(r, known),
        scale_stage(r),
        look_stage(r),
        display_stage(r),
        latency_stage(r),
        audio_stage(r),
    ];
    let overall = overall(stages.iter().map(|s| s.grade));
    Path {
        stages,
        overall,
        live: live_line(r),
        partial: false,
    }
}

fn core_stage(r: &Report, known: &Known) -> Stage {
    let name = if r.core_version.is_empty() {
        r.core.clone()
    } else {
        format!("{} {}", r.core, r.core_version)
    };
    let about_core = format!(
        "{name} emulates the {} and hands pocket-core each frame and its sound, in the console's own resolution and timing. Pixel Path \
         can't measure how exact an emulator is; it follows what happens to the frame and the sound after the emulator has made them.",
        if known.system_name.is_empty() {
            "console"
        } else {
            known.system_name.as_str()
        }
    );
    if !r.patches.is_empty() {
        // A translation or hack applied at the start: the game is changed on purpose, as with cheats, which follow.
        let list = r.patches.join(", ");
        let cheats = match known.cheats {
            0 => String::new(),
            1 => " 1 cheat is on (Options › Cheats).".into(),
            n => format!(" {n} cheats are on (Options › Cheats)."),
        };
        return stage(
            Kind::Core,
            Grade::Enhanced,
            name,
            format!("Patched: {list}"),
            format!("{about_core} pocket-core applied the patch {list} from the game's folder when the game started, so this is the changed game.{cheats}"),
        );
    }
    if known.cheats > 0 {
        let n = if known.cheats == 1 {
            "1 cheat is on".to_string()
        } else {
            format!("{} cheats are on", known.cheats)
        };
        return stage(
            Kind::Core,
            Grade::Enhanced,
            name,
            format!("{}: the game is changed on purpose.", capital(&n)),
            format!("{about_core} {} (Options › Cheats).", capital(&n)),
        );
    }
    let line = if known.system_name.is_empty() {
        "The console, frame by frame.".to_string()
    } else {
        format!("{}, frame by frame.", known.system_name)
    };
    stage(Kind::Core, Grade::Pristine, name, line, about_core)
}

/// Lines a console draws natively, for 3D cores whose internal resolution can be raised.
pub fn native_lines(system: &str) -> Option<u32> {
    Some(match system {
        "n64" => 240,
        "psp" => 272,
        "dreamcast" | "naomi" | "atomiswave" => 480,
        "psx" => 240,
        "saturn" => 224,
        "nds" => 192,
        _ => return None,
    })
}

/// "8:7" for a pixel 8/7 as wide as tall, from small whole numbers where one is close enough.
fn pixel_ratio(par: f32) -> String {
    if (par - 1.0).abs() < 0.01 {
        return "square pixels".into();
    }
    for d in 1..=8u32 {
        let n = (par * d as f32).round() as u32;
        if n > 0 && ((n as f32 / d as f32) / par - 1.0).abs() < 0.003 {
            return format!("{n}:{d} pixels");
        }
    }
    format!("{par:.2}:1 pixels")
}

fn hz(f: f64) -> String {
    format!("{f:.2} Hz")
}

fn thousands(n: f64) -> String {
    let s = format!("{:.0}", n.max(0.0));
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

/// The frame as it is turned on the screen: (width, height, aspect).
fn turned(r: &Report) -> (u32, u32, f32) {
    let s = &r.source;
    let (w, h) = if s.frame_w > 0 { (s.frame_w, s.frame_h) } else { (s.base_w, s.base_h) };
    let (dw, dh, aspect) = crate::video::turned_shape(w, h, s.aspect, s.rotation, s.turn);
    (dw, dh, if aspect > 0.0 { aspect } else { dw.max(1) as f32 / dh.max(1) as f32 })
}

fn source_stage(r: &Report, known: &Known) -> Stage {
    let s = &r.source;
    let (fw, fh) = if s.frame_w > 0 { (s.frame_w, s.frame_h) } else { (s.base_w, s.base_h) };
    let console = if known.system_name.is_empty() {
        "console".to_string()
    } else {
        known.system_name.clone()
    };
    if s.hw {
        if let Some(native) = native_lines(&r.system) {
            let factor = fh as f32 / native as f32;
            let value = format!("{fw}×{fh} · {}", hz(s.fps));
            if factor >= 1.5 {
                let x = if (factor - factor.round()).abs() < 0.05 {
                    format!("{:.0}×", factor.round())
                } else {
                    format!("{factor:.1}×")
                };
                return stage(
                    Kind::Source,
                    Grade::Enhanced,
                    value,
                    format!("Drawn at {x} the {console}'s {native} lines."),
                    format!(
                        "The {console} drew {native} lines. This emulator renders the 3D scenes at {fw}×{fh} instead, {x} as many lines, \
                         so edges and textures are finer than they ever were on the console. Menus and 2D parts may stay at their own size. \
                         The resolution is an emulator option (Options › Emulator)."
                    ),
                );
            }
            if factor < 0.9 {
                return stage(
                    Kind::Source,
                    Grade::Compromised,
                    value,
                    format!("Below the {console}'s {native} lines."),
                    format!("The {console} drew {native} lines; this emulator renders only {fh}. Raise the resolution under Options › Emulator if the game runs fast enough."),
                );
            }
            return stage(
                Kind::Source,
                Grade::Pristine,
                value,
                format!("The {console}'s own resolution."),
                format!(
                    "The emulator renders at {fw}×{fh}, the resolution the {console} drew in, {} times a second.",
                    hz(s.fps).trim_end_matches(" Hz")
                ),
            );
        }
    }
    let par = if s.aspect > 0.0 {
        s.aspect / (fw.max(1) as f32 / fh.max(1) as f32)
    } else {
        1.0
    };
    let pixels = pixel_ratio(par);
    let value = format!("{fw}×{fh} · {} · {pixels}", hz(s.fps));
    let shape = if pixels == "square pixels" {
        "Every pixel is square, as on the console's screen.".to_string()
    } else {
        format!(
            "The console's pixels are not square ({pixels}): the picture keeps the shape it had on a TV, {:.2}:1.",
            s.aspect
        )
    };
    stage(
        Kind::Source,
        Grade::Pristine,
        value,
        format!("The {console}'s own picture."),
        format!(
            "The emulator makes a {fw}×{fh} picture {} times a second, as the {console} did. {shape}",
            hz(s.fps).trim_end_matches(" Hz")
        ),
    )
}

/// The facts of how the frame is laid on the panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub sx: f32,
    pub sy: f32,
    pub whole_x: bool,
    pub whole_y: bool,
    /// How much wider (or narrower, below 0) than the game's shape the picture is drawn, 0.12 = 12 %.
    pub stretch: f32,
    /// The whole-number scale Native would use, when one fits.
    pub whole_fits: Option<u32>,
}

fn is_whole(s: f32) -> bool {
    s >= 0.99 && (s - s.round()).abs() < 0.01
}

pub fn fit(r: &Report) -> Fit {
    let p = &r.picture;
    let (dw, dh, aspect) = turned(r);
    let (dw, dh) = (dw.max(1), dh.max(1));
    let (sx, sy) = (p.w as f32 / dw as f32, p.h as f32 / dh as f32);
    let shown = p.w.max(1) as f32 / p.h.max(1) as f32;
    let native = dest_rect(Scaling::Native, p.view_w.max(1), p.view_h.max(1), dw, dh, aspect);
    let n = native.h as f32 / dh as f32;
    Fit {
        sx,
        sy,
        whole_x: is_whole(sx),
        whole_y: is_whole(sy),
        stretch: shown / aspect - 1.0,
        whole_fits: is_whole(n).then_some(n.round() as u32),
    }
}

pub fn scale_text(s: f32) -> String {
    if is_whole(s) {
        format!("{:.0}×", s.round())
    } else {
        format!("{s:.2}×").replace(".00×", "×")
    }
}

fn scale_stage(r: &Report) -> Stage {
    let p = &r.picture;
    let f = fit(r);
    let sharp = Sharpness::parse(&p.sharpness);
    let filter = match sharp {
        Sharpness::Sharp => "sharp",
        Sharpness::Crisp => "crisp",
        Sharpness::Soft => "soft",
    };
    let place = if p.w >= p.view_w && p.h >= p.view_h {
        "fills the screen".to_string()
    } else if p.x == (p.view_w.saturating_sub(p.w)) / 2 && p.y == (p.view_h.saturating_sub(p.h)) / 2 {
        "centred".to_string()
    } else {
        format!("at {},{}", p.x, p.y)
    };
    let size = if f.whole_x && f.whole_y {
        scale_text(f.sx)
    } else if f.whole_y {
        format!("{} tall, {} wide", scale_text(f.sy), scale_text(f.sx))
    } else if (f.sx - f.sy).abs() < 0.01 {
        scale_text(f.sy)
    } else {
        format!("{:.2} × {}", f.sx, scale_text(f.sy))
    };
    let value = format!("{size} · {}×{} {place} · {filter}", p.w, p.h);
    let (dw, dh, _) = turned(r);
    let filter_words = match sharp {
        Sharpness::Sharp => "Sharp draws every pixel with hard edges.",
        Sharpness::Crisp => "Crisp keeps pixels flat and blends only a one-pixel band where they meet, so uneven sizes don't show.",
        Sharpness::Soft => "Soft blends neighbouring pixels everywhere, like an old screen's blur.",
    };
    let panel = format!("{}×{}", p.view_w, p.view_h);
    let whole_note = f
        .whole_fits
        .map(|n| format!(" At {n}× whole the picture would be sharp and even."))
        .unwrap_or_default();
    // Stretched out of the game's shape.
    if f.stretch.abs() > 0.01 {
        let pct = (f.stretch.abs() * 100.0).round();
        let way = if f.stretch > 0.0 { "wider" } else { "narrower" };
        return stage(
            Kind::Scale,
            Grade::Compromised,
            value,
            format!("Stretched: {pct:.0}% {way} than the game."),
            format!(
                "The picture is stretched to fill the {panel} panel, so it is drawn {pct:.0}% {way} than the game was made: circles become ovals. \
                 {filter_words} Screen scaling under Options › Picture keeps the shape with Integer or Aspect.{whole_note}"
            ),
        );
    }
    if f.whole_x && f.whole_y {
        let n = f.sx.round() as u32;
        return match sharp {
            Sharpness::Soft => stage(
                Kind::Scale,
                Grade::Faithful,
                value,
                format!("Whole {n}×, softened on purpose."),
                format!("Every pixel of the {dw}×{dh} picture covers {n}×{n} panel pixels, the sharpest the panel can show it. {filter_words} Sharp or Crisp under Options › Picture shows the pixels as they are."),
            ),
            _ => stage(
                Kind::Scale,
                Grade::Pristine,
                value,
                format!("Every pixel is a {n}×{n} block."),
                format!(
                    "Every pixel of the {dw}×{dh} picture covers exactly {n}×{n} pixels of the {panel} panel: nothing is blended, \
                     nothing is left out, every pixel the same size. {}",
                    match f.whole_fits {
                        Some(m) if m == n => format!("{}× would not fit on the panel.", n + 1),
                        Some(m) if m > n => format!("{m}× would fit; this size leaves room around the picture."),
                        _ => String::new(),
                    }
                )
                .trim()
                .to_string(),
            ),
        };
    }
    if f.whole_y {
        let line = match sharp {
            Sharpness::Sharp => "Whole lines; columns vary by a pixel for the shape.",
            Sharpness::Crisp => "Whole lines; column edges blended by one pixel.",
            Sharpness::Soft => "Whole lines, softened.",
        };
        return stage(
            Kind::Scale,
            Grade::Faithful,
            value,
            line,
            format!(
                "Every line of the picture is exactly {} panel lines tall. The console's pixels are not square, so to keep the picture's shape \
                 each one is {:.2} panel pixels wide, which no whole number can be. {filter_words}",
                f.sy.round(),
                f.sx
            ),
        );
    }
    // Fractional both ways.
    let small_note = if f.sx < 1.0 {
        " It is larger than the panel, so it is drawn smaller."
    } else {
        ""
    };
    let free = r.source.hw || f.whole_fits.is_none();
    match (sharp, free) {
        (_, true) | (Sharpness::Crisp, false) => stage(
            Kind::Scale,
            Grade::Faithful,
            value,
            if sharp == Sharpness::Crisp && !free { "Not whole; edges blended by one pixel." } else { "Scaled to fit; no whole size would." },
            format!("The {dw}×{dh} picture is drawn {:.2} times as large.{small_note} {filter_words}{whole_note}", f.sy),
        ),
        (Sharpness::Sharp, false) => stage(
            Kind::Scale,
            Grade::Compromised,
            value,
            "Not whole: some pixels are larger than others.",
            format!(
                "The {dw}×{dh} picture is drawn {:.2} times as large, which no pixel can be: some become {} panel pixels wide, others {}, \
                 and lines shimmer as things move. {filter_words}{whole_note} Integer under Options › Picture uses it.",
                f.sy,
                f.sy.floor(),
                f.sy.ceil()
            ),
        ),
        (Sharpness::Soft, false) => stage(
            Kind::Scale,
            Grade::Compromised,
            value,
            "Blurred, where whole pixels would fit.",
            format!("The {dw}×{dh} picture is drawn {:.2} times as large and blended to hide the uneven sizes. {filter_words}{whole_note} Integer under Options › Picture uses it.", f.sy),
        ),
    }
}

fn effect_name(effect: &str) -> Option<&'static str> {
    match effect {
        "grid" => Some("LCD grid"),
        "scanlines" => Some("Scanlines"),
        "crt" => Some("CRT"),
        _ => None,
    }
}

/// What an effect does to the pixels, for the Look stage: honest about what is added (all of them are Enhanced).
fn effect_about(effect: &str) -> &'static str {
    match effect {
        "grid" => "A thin dark gap between the game's pixels, a fixed share of each pixel, so it grows with the scale: it looks like a handheld's screen. It adds to the picture rather than taking away.",
        "scanlines" => "Each line of the picture is drawn as a TV's beam: thin with dark gaps in dark parts, closed in bright ones, and every line keeps its light. It looks like a TV, not like the console's own pixels.",
        _ => "Scanlines like a TV's beam, a fine colour mask too small to see at arm's length and a soft glow around bright parts, mixed in linear light; no curved screen. It looks like a TV, not like the console's own pixels.",
    }
}

fn frame_name(frame: &str) -> Option<&'static str> {
    match frame {
        "auto" => Some("Console frame"),
        "ambient" => Some("Ambient light"),
        "shell" => Some("Handheld shell"),
        "crt" => Some("TV frame"),
        "arcade" => Some("Arcade bezel"),
        "glow" => Some("Glow frame"),
        "brick" | "pocket" | "clamshell" | "wide" | "twin" | "gloss" | "swan" | "watch" | "goggles" => Some("Handheld frame"),
        "studio" | "portable-tv" | "woodgrain" => Some("TV frame"),
        _ => None,
    }
}

fn look_stage(r: &Report) -> Stage {
    let l = &r.look;
    let mut parts: Vec<String> = Vec::new();
    let mut about: Vec<String> = Vec::new();
    let mut grade = Grade::Pristine;
    let mut line = "Nothing over the picture.".to_string();
    if let Some(e) = effect_name(&l.effect) {
        grade = Grade::Enhanced;
        let lcd_colour = l.effect == "grid" && l.lcd_colour;
        parts.push(if lcd_colour { "LCD grid, GBA colours".into() } else { e.into() });
        line = format!("{e} drawn over the picture.");
        about.push(effect_about(&l.effect).into());
        if lcd_colour {
            about.push("The colours are the Game Boy Advance screen's: darker and softer, as the games were made for.".into());
        }
    }
    if l.warmth != 0 || l.contrast != 0 {
        grade = Grade::Enhanced;
        let mut c = Vec::new();
        if l.warmth != 0 {
            let k = crate::display::NEUTRAL_KELVIN - l.warmth as i32 * crate::display::KELVIN_PER_STEP;
            c.push(format!("{} {k} K", if l.warmth > 0 { "warmer," } else { "cooler," }));
        }
        if l.contrast != 0 {
            c.push(format!("contrast {:+}", l.contrast));
        }
        parts.push(c.join(", "));
        if effect_name(&l.effect).is_none() {
            line = "Colours tinted on purpose.".into();
        }
        about.push(format!(
            "The screen's colour ({}) is changed for everything on it, games included (SELECT + Vol+ or Vol-, or quick settings).",
            c.join(", ")
        ));
    }
    if !l.colour.is_empty() {
        parts.push(l.colour.join(", "));
        if grade == Grade::Pristine {
            grade = Grade::Faithful;
            line = "The emulator imitates the console's screen.".into();
        }
        about.push(format!(
            "The emulator changes the colours itself ({}): it imitates how the console's own screen showed them, which is closer to the original, but not the raw pixels.",
            l.colour.join(", ")
        ));
    }
    if let Some(f) = frame_name(&l.frame) {
        parts.push(format!("{f} around"));
        about.push(format!("{f} is drawn around the picture, not over it: the game's pixels are untouched."));
    }
    let value = if parts.is_empty() { "Nothing added".to_string() } else { parts.join(" · ") };
    let about = if about.is_empty() {
        "No effect, filter, frame or colour change: the pixels reach the panel as the scale stage left them.".to_string()
    } else {
        about.join(" ")
    };
    stage(Kind::Look, grade, value, line, about)
}

fn display_stage(r: &Report) -> Stage {
    let d = &r.display;
    let fps = r.source.fps.max(1.0);
    let refresh = if d.refresh_hz > 0.0 { d.refresh_hz } else { 60.0 };
    let panel = if r.picture.view_w > 0 {
        format!("{}×{}", r.picture.view_w, r.picture.view_h)
    } else {
        // Not measured: the device's screen (the Mini V2's 1240×1080 when unknown, `device`).
        let (w, h) = crate::device::current().screen();
        format!("{w}×{h}")
    };
    let value_head = format!("{panel} @ {}", hz(refresh));
    let late = r.live.late;
    let late_note = (late > 0).then(|| {
        format!(
            "{late} {} came too late in the last {:.0} s of play: the previous picture stayed on screen, which shows as a stutter.",
            if late == 1 { "picture" } else { "pictures" },
            r.live.secs.max(1.0)
        )
    });
    if d.time_driven {
        let (value, line, about) = if refresh > fps {
            let n = (1.0 / (1.0 - fps / refresh)).round().max(2.0) as u32;
            (
                format!("{value_head} · {:.0} Hz game", fps),
                format!("Every {} refresh repeats a picture.", ordinal(n)),
                format!(
                    "The game runs at {} and the panel at {}. Each game frame is shown once and every {} refresh shows one twice, so movement \
                     is slightly uneven, and the game keeps its own speed and pitch.",
                    hz(fps),
                    hz(refresh),
                    ordinal(n)
                ),
            )
        } else {
            let n = (1.0 / (fps / refresh - 1.0)).round().max(2.0) as u32;
            (
                format!("{value_head} · {:.0} Hz game", fps),
                format!("Every {} game frame isn't shown.", ordinal(n)),
                format!(
                    "The game runs at {} and the panel at {}: two game frames go into one refresh now and then, and every {} frame is never seen.",
                    hz(fps),
                    hz(refresh),
                    ordinal(n)
                ),
            )
        };
        let about = late_note.map_or(about.clone(), |l| format!("{about} {l}"));
        return stage(Kind::Display, Grade::Compromised, value, line, about);
    }
    let speed = (refresh / fps - 1.0) * 100.0;
    let value = if speed.abs() < 0.05 {
        format!("{value_head} · in step")
    } else {
        format!("{value_head} · {speed:+.2}%")
    };
    let pace = format!(
        "pocket-core shows one game frame per refresh, in step with the panel's vsync, and a frame limiter keeps the pace even. The game ({}) runs {}",
        hz(fps),
        if speed.abs() < 0.05 {
            "at the panel's rate, so nothing is changed.".to_string()
        } else {
            format!(
                "{:.2}% {} to match the panel's {}; the sound follows, so its pitch stays right.",
                speed.abs(),
                if speed > 0.0 { "faster" } else { "slower" },
                hz(refresh)
            )
        }
    );
    if let Some(l) = late_note {
        let what = if late == 1 { "picture" } else { "pictures" };
        return stage(
            Kind::Display,
            Grade::Compromised,
            value,
            format!("{late} {what} late in the last {:.0} s of play.", r.live.secs.max(1.0)),
            format!("{pace} {l}"),
        );
    }
    if speed.abs() < 0.05 {
        stage(Kind::Display, Grade::Pristine, value, "Every frame shown once, in step.", pace)
    } else {
        stage(
            Kind::Display,
            Grade::Faithful,
            value,
            format!(
                "Every frame shown once, {:.2}% {}.",
                speed.abs(),
                if speed > 0.0 { "faster" } else { "slower" }
            ),
            pace,
        )
    }
}

fn ordinal(n: u32) -> String {
    match n {
        2 => "second".into(),
        3 => "third".into(),
        4 => "fourth".into(),
        5 => "fifth".into(),
        6 => "sixth".into(),
        7 => "seventh".into(),
        8 => "eighth".into(),
        9 => "ninth".into(),
        10 => "tenth".into(),
        n => format!("{n}th"),
    }
}

/// The estimate of the time from a button press to the light of the picture that answers it, in refreshes, as the
/// pipeline is built (`src/bin/pocket_core.rs`): not measured with a camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Latency {
    /// The buttons are read once per refresh, at its start: half a refresh on average.
    pub poll: f32,
    /// The core's frames for the refresh (measured).
    pub core: f32,
    /// From the swap to the panel: the compositor takes the frame at the next vsync.
    pub screen: f32,
    /// The panel draws top to bottom: half a refresh to the middle of the screen.
    pub scanout: f32,
    /// Frames run-ahead takes off the game's own reaction.
    pub ahead: u32,
    pub period_ms: f32,
}

impl Latency {
    pub fn estimate(period_ms: f32, core_ms: f32, ahead: u32) -> Latency {
        let period = period_ms.max(1.0);
        Latency {
            poll: 0.5,
            core: (core_ms / period).min(1.0),
            screen: 1.0,
            scanout: 0.5,
            ahead,
            period_ms: period,
        }
    }

    /// Refreshes from the press to the light, before run-ahead.
    pub fn pipeline(&self) -> f32 {
        self.poll + self.core + self.screen + self.scanout
    }

    /// With run-ahead's frames taken off (it can't take off more than the game's own reaction, which is on top of
    /// this, so the result can be small).
    pub fn frames(&self) -> f32 {
        self.pipeline() - self.ahead as f32
    }

    pub fn ms(&self) -> f32 {
        self.frames() * self.period_ms
    }
}

fn latency_stage(r: &Report) -> Stage {
    let period_ms = 1000.0 / if r.display.refresh_hz > 0.0 { r.display.refresh_hz } else { 60.0 } as f32;
    let ra = &r.run_ahead;
    let l = Latency::estimate(period_ms, r.live.core_ms, ra.frames);
    let parts = format!(
        "reading the buttons {:.1}, the emulator {:.1}, waiting for the screen {:.1} and the panel drawing down to its middle {:.1} refreshes",
        l.poll, l.core, l.screen, l.scanout
    );
    let base = format!(
        "An estimate from how pocket-core is built, not a camera measurement: {parts}, ≈ {:.1} refreshes or {:.0} ms. The game's own reaction \
         comes on top of that: most games respond to a button one to three frames later, as they did on the original console.",
        l.pipeline(),
        l.pipeline() * period_ms
    );
    let value = if ra.frames > 0 {
        format!("≈ {:.0} ms · {:.1} frames, {} ahead", l.ms().max(0.0), l.frames(), ra.frames)
    } else {
        format!("≈ {:.0} ms · {:.1} frames", l.ms(), l.frames())
    };
    // The emulator takes most of a refresh: the answer is late, and frames soon will be too.
    let slow = r.live.core_ms >= period_ms * 0.75;
    if ra.frames > 0 {
        let n = if ra.frames == 1 { "1 frame" } else { "2 frames" };
        return stage(
            Kind::Latency,
            if slow { Grade::Compromised } else { Grade::Enhanced },
            value,
            format!("Run-ahead shows the game {n} ahead. An estimate."),
            format!(
                "{base} Run-ahead is on: every refresh the emulator runs {n} further with the buttons as they are and shows that, then goes back, \
                 so a press appears {n} sooner than on the console ({:.0} ms less).",
                ra.frames as f32 * period_ms
            ),
        );
    }
    let line = if ra.pending {
        "Run-ahead starts when the game goes on.".to_string()
    } else if !ra.off.is_empty() {
        format!("{}.", ra.off.trim_end_matches('.'))
    } else {
        "Button to light, plus the game's own reaction.".to_string()
    };
    let more = if ra.pending {
        " Run-ahead is switched on and starts with the game.".to_string()
    } else if !ra.off.is_empty() {
        format!(
            " {}; it checks itself and stays off for this session (Options › Extras › Run-ahead tries again).",
            ra.off.trim_end_matches('.')
        )
    } else if ra.offered {
        " Run-ahead (Options › Extras) can take one or two frames off.".to_string()
    } else {
        String::new()
    };
    stage(
        Kind::Latency,
        if slow { Grade::Compromised } else { Grade::Faithful },
        value,
        if slow { "Slow: the emulator takes most of a refresh.".into() } else { line },
        format!("{base}{more}"),
    )
}

fn audio_stage(r: &Report) -> Stage {
    let Some(a) = r.audio.as_ref() else {
        return stage(
            Kind::Audio,
            Grade::Compromised,
            "No sound",
            "No sound output could be opened.",
            "pocket-core found no sound server (PipeWire) when the game started, so the game plays without sound.",
        );
    };
    // Bluetooth: what the stream reports as "out" ends where the codec and the radio begin, so it is no total.
    let value = if a.bluetooth {
        format!("{} to {} Hz · {:.0} ms queue · Bluetooth", thousands(a.core_rate), thousands(a.out_rate as f64), a.target_ms)
    } else {
        format!(
            "{} to {} Hz · {:.0} ms queue + {:.0} ms out",
            thousands(a.core_rate),
            thousands(a.out_rate as f64),
            a.target_ms,
            a.server_ms
        )
    };
    let bluetooth = if a.bluetooth {
        " The sound goes to Bluetooth headphones or a speaker. Their codec and the radio add a delay of their own that pocket-core can't \
         measure, often 100 to 250 ms, so the sound comes later than on the Pocket's speaker and the numbers here don't include it."
    } else {
        ""
    };
    let skew = (a.in_rate / a.core_rate.max(1.0) - 1.0) * 100.0;
    let base = format!(
        "The emulator makes sound at {} Hz; pocket-core resamples it to the {} Hz of the output with linear interpolation{}, and steers a \
         {:.0} ms queue ({:.0} ms in it now) by nudging the rate by up to ±0.5%, so the sound neither runs dry nor drifts behind the \
         picture. After the queue, the output ({}) holds about {:.0} ms more. {} in this session.{}",
        thousands(a.core_rate),
        thousands(a.out_rate as f64),
        if skew.abs() >= 0.01 {
            format!(
                ", fed {:.2}% {} to follow the picture's pace",
                skew.abs(),
                if skew > 0.0 { "faster" } else { "slower" }
            )
        } else {
            String::new()
        },
        a.target_ms,
        a.queue_ms,
        match a.backend.as_str() {
            "pipewire (alsa)" => "PipeWire",
            "pulse" => "PulseAudio",
            other => other,
        },
        a.server_ms,
        match a.underruns {
            0 => "No underruns".to_string(),
            1 => "1 underrun".to_string(),
            n => format!("{n} underruns"),
        },
        bluetooth
    );
    let under = r.live.underruns;
    if under > 0 {
        return stage(
            Kind::Audio,
            Grade::Compromised,
            value,
            format!(
                "{under} {} in the last {:.0} s: short gaps.",
                if under == 1 { "underrun" } else { "underruns" },
                r.live.secs.max(1.0)
            ),
            format!(
                "{base} In the last {:.0} s of play the queue ran empty {under} times, each a short gap in the sound.",
                r.live.secs.max(1.0)
            ),
        );
    }
    let line = if a.bluetooth { "No gaps. Bluetooth adds its own delay." } else { "Resampled, rate-controlled, no gaps." };
    stage(Kind::Audio, Grade::Faithful, value, line, base)
}

fn live_line(r: &Report) -> Option<String> {
    let l = &r.live;
    if l.frames == 0 {
        return None;
    }
    let when = if r.paused {
        format!("Last {:.0} s of play", l.secs.max(1.0))
    } else {
        "Now".to_string()
    };
    let mut parts = vec![
        when,
        format!("{:.1} fps", l.fps),
        format!("core {:.1} ms", l.core_ms),
        format!("CPU {:.0}%", l.cpu),
    ];
    parts.push(if l.late == 1 { "1 late".into() } else { format!("{} late", l.late) });
    parts.push(if l.underruns == 1 {
        "1 underrun".into()
    } else {
        format!("{} underruns", l.underruns)
    });
    Some(parts.join(" · "))
}

// ---- made-up reports for snapshots and tests -----------------------------------------------------------

/// Reports for `--snapshot pixel-path:<case>`: gba (whole scale), snes (non-square pixels), n64 (upscaled),
/// runahead, compromised (stretched, soft, late frames, underruns), pal (a 50 Hz game), bluetooth (the sound on
/// Bluetooth headphones).
pub fn demo(case: &str) -> Option<(Report, Known)> {
    let audio = |core_rate: f64, in_rate: f64| Audio {
        backend: "pipewire (alsa)".into(),
        core_rate,
        in_rate,
        out_rate: 48_000,
        target_ms: 20.0,
        queue_ms: 20.4,
        server_ms: 26.0,
        underruns: 0,
        bluetooth: false,
    };
    let live = Live {
        secs: 5.0,
        frames: 300,
        fps: 60.0,
        late: 0,
        emulated_fps: 59.98,
        core_ms: 1.1,
        core_ms_max: 2.3,
        cpu: 21.0,
        underruns: 0,
    };
    let picture = |x, y, w, h, scaling: &str, sharpness: &str| Picture {
        view_w: 1240,
        view_h: 1080,
        x,
        y,
        w,
        h,
        scaling: scaling.into(),
        sharpness: sharpness.into(),
    };
    let look = Look {
        effect: "none".into(),
        frame: "none".into(),
        ..Default::default()
    };
    let gba = Report {
        seq: 1,
        rom: "/storage/roms/gba/Advance Wars (USA).gba".into(),
        system: "gba".into(),
        core: "mGBA".into(),
        core_version: "0.11-dev".into(),
        source: Source {
            base_w: 240,
            base_h: 160,
            frame_w: 240,
            frame_h: 160,
            fps: 59.7275,
            aspect: 1.5,
            hw: false,
            rotation: 0,
            turn: 0,
        },
        picture: picture(20, 140, 1200, 800, "native", "sharp"),
        look: look.clone(),
        display: Display {
            refresh_hz: 60.0,
            time_driven: false,
        },
        run_ahead: RunAhead {
            offered: true,
            ..Default::default()
        },
        audio: Some(audio(32_768.0, 32_768.0 * 60.0 / 59.7275)),
        live: live.clone(),
        paused: true,
        patches: Vec::new(),
    };
    let known = |name: &str| Known {
        system_name: name.into(),
        cheats: 0,
    };
    Some(match case {
        "gba" => (gba, known("Game Boy Advance")),
        "bluetooth" => {
            let mut r = gba;
            if let Some(a) = r.audio.as_mut() {
                a.bluetooth = true;
                a.server_ms = 41.0;
            }
            (r, known("Game Boy Advance"))
        }
        "runahead" => {
            let mut r = gba;
            r.rom = "/storage/roms/gba/Mega Man Zero (USA).gba".into();
            r.run_ahead = RunAhead {
                offered: true,
                wanted: 1,
                frames: 1,
                pending: false,
                off: String::new(),
            };
            r.live.core_ms = 2.4;
            r.live.cpu = 34.0;
            (r, known("Game Boy Advance"))
        }
        "snes" => (
            Report {
                rom: "/storage/roms/snes/Super Metroid (Japan, USA).sfc".into(),
                system: "snes".into(),
                core: "Snes9x".into(),
                core_version: "1.62.3".into(),
                source: Source {
                    base_w: 256,
                    base_h: 224,
                    frame_w: 256,
                    frame_h: 224,
                    fps: 60.0988,
                    aspect: 4.0 / 3.0,
                    hw: false,
                    rotation: 0,
                    turn: 0,
                },
                picture: picture(22, 92, 1195, 896, "native", "crisp"),
                audio: Some(audio(32_040.0, 32_040.0 * 60.0 / 60.0988)),
                live: Live {
                    cpu: 18.0,
                    core_ms: 0.9,
                    ..live.clone()
                },
                ..gba_like()
            },
            known("Super Nintendo"),
        ),
        // A fan translation applied at the start (`softpatch`).
        "patched" => {
            let (mut r, k) = demo("snes")?;
            r.rom = "/storage/roms/snes/Seiken Densetsu 3 (Japan).sfc".into();
            r.patches = vec!["Seiken Densetsu 3 (Japan).ips".into()];
            (r, k)
        }
        "n64" => (
            Report {
                rom: "/storage/roms/n64/Mario Kart 64 (USA).z64".into(),
                system: "n64".into(),
                core: "Mupen64Plus-Next".into(),
                core_version: "2.6-Vulkan".into(),
                source: Source {
                    base_w: 1280,
                    base_h: 960,
                    frame_w: 1280,
                    frame_h: 960,
                    fps: 60.0,
                    aspect: 4.0 / 3.0,
                    hw: true,
                    rotation: 0,
                    turn: 0,
                },
                picture: picture(0, 75, 1240, 930, "native", "soft"),
                run_ahead: RunAhead::default(),
                audio: Some(Audio {
                    target_ms: 48.0,
                    queue_ms: 47.0,
                    ..audio(44_100.0, 44_100.0)
                }),
                live: Live {
                    core_ms: 7.8,
                    core_ms_max: 13.5,
                    cpu: 96.0,
                    emulated_fps: 60.0,
                    ..live.clone()
                },
                ..gba_like()
            },
            known("Nintendo 64"),
        ),
        "pal" => (
            Report {
                rom: "/storage/roms/psx/Crash Bandicoot 2 (Europe).chd".into(),
                system: "psx".into(),
                core: "PCSX-ReARMed".into(),
                core_version: "r24".into(),
                source: Source {
                    base_w: 320,
                    base_h: 240,
                    frame_w: 512,
                    frame_h: 240,
                    fps: 50.0,
                    aspect: 4.0 / 3.0,
                    hw: false,
                    rotation: 0,
                    turn: 0,
                },
                picture: picture(140, 180, 960, 720, "native", "crisp"),
                display: Display {
                    refresh_hz: 60.0,
                    time_driven: true,
                },
                run_ahead: RunAhead::default(),
                audio: Some(audio(44_100.0, 44_100.0)),
                live: Live {
                    emulated_fps: 50.0,
                    core_ms: 3.2,
                    cpu: 41.0,
                    ..live.clone()
                },
                ..gba_like()
            },
            known("PlayStation"),
        ),
        "compromised" => {
            let mut r = gba;
            r.rom = "/storage/roms/gba/Golden Sun (USA, Europe).gba".into();
            r.picture = picture(0, 0, 1240, 1080, "fullscreen", "soft");
            r.run_ahead = RunAhead {
                offered: true,
                wanted: 2,
                frames: 0,
                pending: false,
                off: "Run-ahead is off: too slow for this game".into(),
            };
            r.live = Live {
                secs: 5.0,
                frames: 287,
                fps: 57.4,
                late: 13,
                emulated_fps: 57.4,
                core_ms: 9.6,
                core_ms_max: 21.0,
                cpu: 88.0,
                underruns: 3,
            };
            if let Some(a) = r.audio.as_mut() {
                a.underruns = 7;
                a.queue_ms = 6.0;
            }
            (r, known("Game Boy Advance"))
        }
        _ => return None,
    })
}

fn gba_like() -> Report {
    Report {
        seq: 1,
        look: Look {
            effect: "none".into(),
            frame: "none".into(),
            ..Default::default()
        },
        display: Display {
            refresh_hz: 60.0,
            time_driven: false,
        },
        run_ahead: RunAhead {
            offered: true,
            ..Default::default()
        },
        paused: true,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn grades(case: &str) -> Vec<(Kind, Grade)> {
        let (r, k) = demo(case).unwrap();
        path(&r, &k).stages.iter().map(|s| (s.kind, s.grade)).collect()
    }

    fn grade_of(case: &str, kind: Kind) -> Grade {
        grades(case).into_iter().find(|(k, _)| *k == kind).unwrap().1
    }

    #[test]
    fn overall_is_the_worst_known_stage() {
        use Grade::*;
        assert_eq!(overall([Pristine, Pristine]), Pristine);
        assert_eq!(overall([Pristine, Faithful, Pristine]), Faithful);
        assert_eq!(overall([Faithful, Enhanced]), Enhanced);
        assert_eq!(overall([Enhanced, Compromised, Pristine]), Compromised);
        assert_eq!(overall([Pristine, Unknown]), Pristine, "unknown stages don't count");
        assert_eq!(overall([Unknown, Unknown]), Unknown);
        assert_eq!(overall([]), Unknown);
    }

    #[test]
    fn gba_at_five_times_is_pristine_where_the_pixels_are() {
        let (r, k) = demo("gba").unwrap();
        let p = path(&r, &k);
        let g: Vec<Grade> = p.stages.iter().map(|s| s.grade).collect();
        // Core, Source, Scale, Look: the console's pixels untouched. Display: 59.73 Hz on a 60 Hz panel. Audio: resampled.
        assert_eq!(
            g,
            vec![
                Grade::Pristine,
                Grade::Pristine,
                Grade::Pristine,
                Grade::Pristine,
                Grade::Faithful,
                Grade::Faithful,
                Grade::Faithful
            ]
        );
        assert_eq!(p.overall, Grade::Faithful);
        let scale = &p.stages[2];
        assert_eq!(scale.value, "5× · 1200×800 centred · sharp");
        assert_eq!(scale.line, "Every pixel is a 5×5 block.");
        assert!(scale.about.contains("6× would not fit"), "{}", scale.about);
        assert_eq!(p.stages[1].value, "240×160 · 59.73 Hz · square pixels");
        assert_eq!(p.stages[4].value, "1240×1080 @ 60.00 Hz · +0.46%");
        assert!(
            p.stages[6].value.starts_with("32,768 to 48,000 Hz · 20 ms queue + 26 ms out"),
            "{}",
            p.stages[6].value
        );
    }

    #[test]
    fn crisp_at_a_whole_scale_is_still_pixel_perfect_and_soft_is_not() {
        let (mut r, k) = demo("gba").unwrap();
        r.picture.sharpness = "crisp".into();
        assert_eq!(path(&r, &k).stages[2].grade, Grade::Pristine, "Crisp is nearest at whole scales (video.frag)");
        r.picture.sharpness = "soft".into();
        assert_eq!(path(&r, &k).stages[2].grade, Grade::Faithful);
    }

    #[test]
    fn snes_has_whole_lines_and_uneven_columns() {
        assert_eq!(grade_of("snes", Kind::Scale), Grade::Faithful);
        let (r, k) = demo("snes").unwrap();
        let p = path(&r, &k);
        assert_eq!(p.stages[2].value, "4× tall, 4.67× wide · 1195×896 centred · crisp");
        assert_eq!(p.stages[1].value, "256×224 · 60.10 Hz · 7:6 pixels");
        let f = fit(&r);
        assert!(f.whole_y && !f.whole_x && f.stretch.abs() < 0.01 && f.whole_fits == Some(4));
    }

    #[test]
    fn fractional_where_whole_pixels_fit_is_compromised_unless_crisp() {
        let (mut r, k) = demo("gba").unwrap();
        // Aspect: 7.75× of 240×160 would not fit; it is 1240×827, 5.17× both ways.
        r.picture = Picture {
            x: 0,
            y: 127,
            w: 1240,
            h: 827,
            scaling: "aspect".into(),
            sharpness: "sharp".into(),
            ..r.picture
        };
        let f = fit(&r);
        assert!(!f.whole_x && !f.whole_y && f.whole_fits == Some(5), "{f:?}");
        assert_eq!(path(&r, &k).stages[2].grade, Grade::Compromised);
        r.picture.sharpness = "soft".into();
        assert_eq!(path(&r, &k).stages[2].grade, Grade::Compromised, "non-integer blur where integer was possible");
        r.picture.sharpness = "crisp".into();
        assert_eq!(path(&r, &k).stages[2].grade, Grade::Faithful);
    }

    #[test]
    fn stretched_pictures_are_compromised() {
        let (r, k) = demo("compromised").unwrap();
        let p = path(&r, &k);
        let scale = &p.stages[2];
        assert_eq!(scale.grade, Grade::Compromised);
        // 1240×1080 is 1.148 against the GBA's 1.5: 23 % narrower.
        assert_eq!(scale.line, "Stretched: 23% narrower than the game.");
        assert_eq!(p.overall, Grade::Compromised);
        assert_eq!(grade_of("compromised", Kind::Display), Grade::Compromised, "13 late pictures");
        assert_eq!(grade_of("compromised", Kind::Audio), Grade::Compromised, "3 underruns in the window");
        assert!(p.stages[5].line.contains("too slow"), "run-ahead says why it is off: {}", p.stages[5].line);
    }

    #[test]
    fn n64_rendering_above_its_240_lines_is_enhanced() {
        let (r, k) = demo("n64").unwrap();
        let p = path(&r, &k);
        assert_eq!(p.stages[1].grade, Grade::Enhanced);
        assert_eq!(p.stages[1].line, "Drawn at 4× the Nintendo 64's 240 lines.");
        // 1280×960 doesn't fit the panel at 1×: scaled down to fit, soft, which is right for 3D.
        assert_eq!(p.stages[2].grade, Grade::Faithful);
        assert_eq!(p.overall, Grade::Enhanced);
        // A 3D core at its console's own lines is native, below them it loses detail.
        let mut r2 = r.clone();
        (r2.source.frame_w, r2.source.frame_h) = (320, 240);
        assert_eq!(path(&r2, &k).stages[1].grade, Grade::Pristine);
        (r2.source.frame_w, r2.source.frame_h) = (160, 120);
        assert_eq!(path(&r2, &k).stages[1].grade, Grade::Compromised);
    }

    #[test]
    fn run_ahead_is_enhanced_and_takes_its_frames_off_the_estimate() {
        let (r, k) = demo("runahead").unwrap();
        let p = path(&r, &k);
        assert_eq!(p.stages[5].grade, Grade::Enhanced);
        assert_eq!(p.overall, Grade::Enhanced);
        assert!(p.stages[5].value.ends_with("1 ahead"), "{}", p.stages[5].value);
        // Switched on in the menu but not started yet; switched itself off.
        let mut r2 = r.clone();
        r2.run_ahead = RunAhead {
            offered: true,
            wanted: 1,
            frames: 0,
            pending: true,
            off: String::new(),
        };
        assert_eq!(path(&r2, &k).stages[5].line, "Run-ahead starts when the game goes on.");
        assert_eq!(path(&r2, &k).stages[5].grade, Grade::Faithful);
    }

    #[test]
    fn latency_estimate_adds_up_the_pipeline() {
        let period = 1000.0 / 60.0;
        let l = Latency::estimate(period, 1.6667, 0);
        assert!((l.core - 0.1).abs() < 1e-3);
        assert!((l.pipeline() - 2.1).abs() < 1e-3, "0.5 poll + 0.1 core + 1 screen + 0.5 scan-out");
        assert!((l.ms() - 35.0).abs() < 0.1);
        let ra = Latency::estimate(period, 1.6667, 1);
        assert!((ra.frames() - 1.1).abs() < 1e-3);
        assert!((l.ms() - ra.ms() - period).abs() < 0.01, "one frame ahead is one refresh less");
        // A core that takes longer than a refresh counts as a whole one (the frame is late anyway: Display says so).
        assert_eq!(Latency::estimate(period, 40.0, 0).core, 1.0);
        let (r, k) = demo("gba").unwrap();
        let s = &path(&r, &k).stages[5];
        // 0.5 + 1.1 ms of 16.7 + 1 + 0.5 = 2.07 refreshes.
        assert_eq!(s.value, "≈ 34 ms · 2.1 frames");
        assert!(s.about.starts_with("An estimate"), "{}", s.about);
    }

    #[test]
    fn a_50_hz_game_on_the_60_hz_panel_repeats_every_sixth_picture() {
        let (r, k) = demo("pal").unwrap();
        let s = &path(&r, &k).stages[4];
        assert_eq!(s.grade, Grade::Compromised);
        assert_eq!(s.line, "Every sixth refresh repeats a picture.");
        let mut fast = r.clone();
        fast.source.fps = 75.0;
        assert_eq!(path(&fast, &k).stages[4].line, "Every fourth game frame isn't shown.");
        // In step: a 60.00 Hz core on the 60 Hz panel.
        let (mut r60, k60) = demo("gba").unwrap();
        r60.source.fps = 60.0;
        assert_eq!(path(&r60, &k60).stages[4].grade, Grade::Pristine);
    }

    #[test]
    fn look_grades_effects_colour_and_frames() {
        let (mut r, k) = demo("gba").unwrap();
        r.look.frame = "shell".into();
        let s = path(&r, &k).stages[3].clone();
        assert_eq!(
            (s.grade, s.value.as_str()),
            (Grade::Pristine, "Handheld shell around"),
            "a frame around doesn't touch the pixels"
        );
        r.look.effect = "scanlines".into();
        assert_eq!(path(&r, &k).stages[3].grade, Grade::Enhanced);
        r.look.effect = "crt".into();
        let s = path(&r, &k).stages[3].clone();
        assert_eq!((s.grade, s.value.as_str(), s.line.as_str()), (Grade::Enhanced, "CRT · Handheld shell around", "CRT drawn over the picture."));
        assert!(s.about.contains("colour mask") && s.about.contains("no curved screen"), "{}", s.about);
        r.look.effect = "grid".into();
        r.look.lcd_colour = true;
        let s = path(&r, &k).stages[3].clone();
        assert_eq!((s.grade, s.value.as_str()), (Grade::Enhanced, "LCD grid, GBA colours · Handheld shell around"));
        assert!(s.about.contains("Game Boy Advance screen's"), "{}", s.about);
        r.look.lcd_colour = false;
        r.look.effect = "none".into();
        r.look.colour = vec!["Color correction: Game Boy Advance".into()];
        assert_eq!(path(&r, &k).stages[3].grade, Grade::Faithful);
        r.look.warmth = 3;
        let s = path(&r, &k).stages[3].clone();
        assert_eq!(s.grade, Grade::Enhanced);
        assert!(s.value.contains("warmer, 5600 K"), "{}", s.value);
    }

    #[test]
    fn cheats_change_the_game_on_purpose() {
        let (r, mut k) = demo("gba").unwrap();
        k.cheats = 2;
        let s = &path(&r, &k).stages[0];
        assert_eq!(s.grade, Grade::Enhanced);
        assert_eq!(s.line, "2 cheats are on: the game is changed on purpose.");
    }

    #[test]
    fn bluetooth_sound_doesnt_claim_the_speakers_numbers() {
        let (r, k) = demo("bluetooth").unwrap();
        let s = &path(&r, &k).stages[6];
        assert_eq!(s.kind, Kind::Audio);
        assert_eq!((s.grade, s.value.as_str()), (Grade::Faithful, "32,768 to 48,000 Hz · 20 ms queue · Bluetooth"));
        assert_eq!(s.line, "No gaps. Bluetooth adds its own delay.");
        assert!(s.about.contains("can't measure, often 100 to 250 ms") && s.about.contains("don't include it"), "{}", s.about);
        // The speaker's stage says nothing of Bluetooth, and a report from before the field reads as the speaker.
        let (gba, _) = demo("gba").unwrap();
        let s = &path(&gba, &k).stages[6];
        assert!(s.value.ends_with("ms out") && !s.about.contains("Bluetooth"), "{} / {}", s.value, s.about);
        let mut json = serde_json::to_value(&r).unwrap();
        json["audio"].as_object_mut().unwrap().remove("bluetooth");
        let old: Report = serde_json::from_value(json).unwrap();
        assert!(!old.audio.unwrap().bluetooth);
    }

    #[test]
    fn no_sound_device_is_compromised() {
        let (mut r, k) = demo("gba").unwrap();
        r.audio = None;
        assert_eq!(path(&r, &k).stages[6].grade, Grade::Compromised);
    }

    #[test]
    fn the_frame_window_counts_the_last_seconds_of_play() {
        let period = 1000.0 / 60.0;
        // 10 s of steady frames, then one picture that stayed two extra refreshes.
        let mut iv = vec![period; 600];
        iv.push(period * 3.0);
        let run = vec![1.0; iv.len()];
        let (frames, fps, late, core, core_max, secs) = frame_window(&iv, &run, period, 5.0);
        assert_eq!(late, 2);
        assert!((secs - 5.0).abs() < 0.02, "{secs}");
        assert!((frames as i32 - 298).abs() <= 1, "{frames}");
        assert!((fps - 59.6).abs() < 0.3, "{fps}");
        assert_eq!((core, core_max), (1.0, 1.0));
        // Every picture a little late without vsync's rounding (19.5 ms instead of 16.7): 300 refreshes, 256 pictures.
        let slow = vec![19.53; 256];
        assert_eq!(frame_window(&slow, &vec![1.0; 256], period, 5.0).2, 44);
        // Before the first full window, and with nothing at all.
        assert_eq!(frame_window(&[period; 3], &[2.0; 3], period, 5.0).0, 3);
        assert_eq!(frame_window(&[], &[], period, 5.0), (0, 0.0, 0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn the_window_keeps_five_seconds_of_half_second_ticks() {
        let mut w = Window::default();
        let tick = Tick {
            secs: 0.5,
            frames: 30,
            emulated: 30,
            cpu_secs: 0.1,
        };
        for i in 0..30u64 {
            // An underrun every second, 15 in the whole session.
            w.push(tick, i / 2);
        }
        let (emulated, cpu, under) = w.summary();
        assert!((emulated - 60.0).abs() < 0.01);
        assert!((cpu - 20.0).abs() < 0.01);
        assert_eq!(under, 5, "the last 5 s only");
        assert!(w.ticks.len() <= 10);
        assert_eq!(Window::default().summary(), (0.0, 0.0, 0));
    }

    #[test]
    fn the_report_goes_through_its_file_for_its_own_game_and_request() {
        let dir = TestDir::new("pixelpath");
        let file = dir.join("pixelpath.json");
        let (mut r, _) = demo("snes").unwrap();
        r.seq = 7;
        // serde_json reads floats back to the nearest few bits only; a value that is exact in binary compares equal.
        r.audio.as_mut().unwrap().in_rate = 31_987.25;
        r.write(&file);
        let rom = std::path::Path::new(&r.rom).to_path_buf();
        assert_eq!(Report::read(&file, &rom, 7), Some(r.clone()));
        assert_eq!(Report::read(&file, &rom, 6), None, "an older answer");
        assert_eq!(Report::read(&file, std::path::Path::new("/other.sfc"), 7), None, "another game's");
        // Fields this version doesn't know are fine, missing ones default.
        std::fs::write(&file, br#"{"seq": 2, "rom": "/x.gb", "core": "Gambatte", "something_new": 1}"#).unwrap();
        let old = Report::read(&file, std::path::Path::new("/x.gb"), 2).unwrap();
        assert_eq!((old.core.as_str(), old.audio.is_none(), old.live.frames), ("Gambatte", true, 0));
        let mut other = r.clone();
        other.seq = 8;
        assert!(r.same_as(&other) && r != other);
    }

    #[test]
    fn colour_options_in_effect_are_named() {
        use crate::core_options::{FileOption, FileValue};
        let opt = |key: &str, label: &str, current: &str| FileOption {
            key: key.into(),
            label: label.into(),
            info: String::new(),
            category: None,
            values: vec![
                FileValue {
                    value: "disabled".into(),
                    label: "Off".into(),
                },
                FileValue {
                    value: "GBA".into(),
                    label: "Game Boy Advance".into(),
                },
            ],
            default: "disabled".into(),
            current: current.into(),
        };
        let opts = [
            opt("mgba_color_correction", "Color Correction", "GBA"),
            opt("mgba_interframe_blending", "Interframe Blending", "disabled"),
            opt("mgba_skip_bios", "Skip BIOS", "GBA"),
        ];
        assert_eq!(colour_options(&opts), vec!["Color Correction: Game Boy Advance".to_string()]);
        assert_eq!(scaling_value(Scaling::parse("integer-smaller")), "integer-smaller");
        let mut v = crate::video::VideoSettings {
            frame: crate::video::FrameMode::Shell,
            ..Default::default()
        };
        assert_eq!(frame_value(&v), "shell");
        v.scaling = Scaling::Fullscreen;
        assert_eq!(frame_value(&v), "none", "no room for it");
    }

    #[test]
    fn small_formats() {
        assert_eq!(thousands(32_768.0), "32,768");
        assert_eq!(thousands(48_000.0), "48,000");
        assert_eq!(thousands(800.0), "800");
        assert_eq!(pixel_ratio(1.0), "square pixels");
        assert_eq!(pixel_ratio(8.0 / 7.0), "8:7 pixels");
        assert_eq!(pixel_ratio(1.2345), "1.23:1 pixels");
        assert_eq!(scale_text(4.0), "4×");
        assert_eq!(scale_text(4.6667), "4.67×");
        let live = live_line(&demo("gba").unwrap().0).unwrap();
        assert_eq!(live, "Last 5 s of play · 60.0 fps · core 1.1 ms · CPU 21% · 0 late · 0 underruns");
    }
}
