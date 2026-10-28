//! Display out: playing on a TV or monitor through the Mini V2's USB-C (DisplayPort alt mode), docs/DISPLAY-OUT.md.
//!
//! The Mini V2 has no HDMI port. Its USB-C carries DisplayPort over two lanes (`data-lanes = <0 1>` in ROCKNIX's
//! device tree), which leaves the other two for USB 3 through a dock, and caps the link at 4K 30 Hz or 1080p up to
//! ~120 Hz. A USB-C to HDMI adapter shows up as the connector `DP-1` next to the panel `DSI-1`.
//!
//! ROCKNIX 20261001 already reacts to it, with or without pocket-ui: its sway config moves workspace 1 to `DP-1`
//! when that appears (`workspace 1 output DP-1 DSI-1`), and its `output_monitor` (started by sway) disables the panel,
//! asks for 1920x1080 at 60 Hz and switches the sound card to its "HDMI" profile; on unplug it turns the panel back
//! on and the profile back to "HiFi". pocket-ui does not fight that. What this module adds:
//!
//! - `watch`: a thread that listens to sway's output events (its IPC socket, no polling, nothing at all while
//!   nothing is plugged in) and tells the UI whether it is on a TV now (`Tv`), so touches on the dark panel are
//!   ignored and the shelf can say where the picture went.
//! - `plan`: the repairs pocket-ui makes when, a moment after the TV came, ROCKNIX has not (a ROCKNIX without
//!   `output_monitor`, or one that did only half): the TV's mode (`choose_mode`), pocket-ui's workspace on the TV,
//!   the panel dark; and the panel back on when the TV goes. All of it is idempotent sway commands, and nothing
//!   happens on a panel-only device: `plan` is empty unless a TV is or was there (tested below).
//! - pure helpers for what is not wired yet (docs/DISPLAY-OUT.md, phase 2): `ui_fit` (how pocket-ui's 1080-line design
//!   fits a 720p or 4K output) and `sound_route` (where the sound should go, headphones and Bluetooth first).
//!
//! Everything here runs off the UI thread; the UI thread only stores the `Tv` it is sent.

use serde_json::Value;

/// One display mode as sway reports it (`refresh` in mHz: 60000 is 60 Hz).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub w: u32,
    pub h: u32,
    pub mhz: u32,
}

impl Mode {
    /// The mode as a sway `output <name> mode` argument: `1920x1080@60.000Hz`.
    pub fn sway_arg(&self) -> String {
        format!("{}x{}@{}.{:03}Hz", self.w, self.h, self.mhz / 1000, self.mhz % 1000)
    }

    fn hz(&self) -> f32 {
        self.mhz as f32 / 1000.0
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{} {:.2} Hz", self.w, self.h, self.hz())
    }
}

/// One output of `GET_OUTPUTS` (sway lists connected outputs, enabled or not).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub name: String,
    pub make: String,
    pub model: String,
    /// Enabled (part of the layout). ROCKNIX's `output_monitor` disables the panel while a TV is on.
    pub active: bool,
    /// Powered (DPMS on). An enabled output can be powered off (`output X power off`).
    pub power: bool,
    pub current: Option<Mode>,
    pub modes: Vec<Mode>,
}

impl Output {
    fn from_json(v: &Value) -> Option<Output> {
        let name = v["name"].as_str()?.to_string();
        let mode = |m: &Value| -> Option<Mode> {
            let (w, h) = (m["width"].as_u64()? as u32, m["height"].as_u64()? as u32);
            (w > 0 && h > 0).then(|| Mode { w, h, mhz: m["refresh"].as_u64().unwrap_or(0) as u32 })
        };
        let active = v["active"].as_bool().unwrap_or(false);
        Some(Output {
            name,
            make: v["make"].as_str().unwrap_or("").to_string(),
            model: v["model"].as_str().unwrap_or("").to_string(),
            active,
            // `power` since sway 1.8, `dpms` before.
            power: v["power"].as_bool().or_else(|| v["dpms"].as_bool()).unwrap_or(active),
            current: if active { mode(&v["current_mode"]) } else { None },
            modes: v["modes"].as_array().map(|a| a.iter().filter_map(mode).collect()).unwrap_or_default(),
        })
    }

    /// Showing a picture: enabled and powered.
    pub fn lit(&self) -> bool {
        self.active && self.power
    }
}

/// `GET_OUTPUTS`' answer. Anything unreadable is no outputs (and so no TV).
pub fn parse_outputs(json: &[u8]) -> Vec<Output> {
    serde_json::from_slice::<Vec<Value>>(json).map(|v| v.iter().filter_map(Output::from_json).collect()).unwrap_or_default()
}

/// The name of the output that has the focused workspace (`GET_WORKSPACES`): where pocket-ui and the game are.
pub fn focused_output(json: &[u8]) -> Option<String> {
    let v = serde_json::from_slice::<Vec<Value>>(json).ok()?;
    v.iter().find(|w| w["focused"].as_bool() == Some(true)).and_then(|w| w["output"].as_str()).map(str::to_string)
}

/// A connector that leads out of the device: USB-C DisplayPort (`DP-1`), or HDMI on other handhelds.
/// `POCKETUI_TV_OUTPUT` names one more (the virtual device's second headless output, docs/DISPLAY-OUT.md).
pub fn is_external(name: &str) -> bool {
    name.starts_with("DP-") || name.starts_with("HDMI-") || name.starts_with("DisplayPort-") || std::env::var("POCKETUI_TV_OUTPUT").is_ok_and(|n| !n.is_empty() && n == name)
}

/// The built-in panel's connector.
pub fn is_panel(name: &str) -> bool {
    name.starts_with("DSI-") || name.starts_with("eDP-") || name.starts_with("LVDS-")
}

/// Retroid's dual-screen add-on is a DisplayPort screen too (a Lontium bridge); ROCKNIX sets it up as a second
/// screen, and it is not a TV.
fn is_add_on_screen(o: &Output) -> bool {
    let l = |s: &str| s.to_ascii_lowercase().contains("lontium");
    l(&o.make) || l(&o.model)
}

/// The panel and the TV, as sway has them now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Screens {
    pub panel: Option<Output>,
    pub tv: Option<Output>,
}

impl Screens {
    pub fn from_outputs(outputs: Vec<Output>) -> Screens {
        let mut s = Screens::default();
        for o in outputs {
            if is_panel(&o.name) && s.panel.is_none() {
                s.panel = Some(o);
            } else if is_external(&o.name) && !is_add_on_screen(&o) && s.tv.is_none() {
                s.tv = Some(o);
            }
        }
        s
    }

    /// What the UI is told: the TV while it shows a picture.
    pub fn shown_tv(&self) -> Option<Tv> {
        let tv = self.tv.as_ref().filter(|t| t.active)?;
        let m = tv.current?;
        Some(Tv { name: tv.name.clone(), w: m.w, h: m.h, mhz: m.mhz })
    }
}

/// pocket-ui is on a TV: its output and mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tv {
    pub name: String,
    pub w: u32,
    pub h: u32,
    pub mhz: u32,
}

/// The mode TV mode asks for. 1920x1080 at 60 Hz wherever the screen has it: pocket-ui's design is 1080 lines
/// high, so the shelf and the menus are drawn 1:1, a 4K TV doubles it exactly, games scale by whole numbers to
/// 1080 lines (`video::dest_rect`), and 60 Hz matches the consoles. 4K is no choice anyway: two DisplayPort lanes
/// carry it at 30 Hz at most, and pocket-ui draws in software (four times the pixels). Without 1080p: the largest
/// landscape mode at most 1080 lines high at about 60 Hz, then any refresh. `None` when sway lists no modes.
pub fn choose_mode(modes: &[Mode]) -> Option<Mode> {
    let sixty = |m: &&Mode| (59_000..=61_000).contains(&m.mhz);
    // 1920x1080 first, then the tallest, the widest, and the closest to 60.000 Hz (60 Hz over 59.94 Hz).
    let best = |c: Vec<&Mode>| -> Option<Mode> {
        c.into_iter()
            .max_by_key(|m| {
                let full_hd = m.w == 1920 && m.h == 1080;
                (full_hd, m.h, m.w, std::cmp::Reverse((m.mhz as i64 - 60_000).abs()))
            })
            .copied()
    };
    let fits = |m: &&Mode| m.h <= 1080 && m.w >= m.h;
    best(modes.iter().filter(fits).filter(sixty).collect())
        .or_else(|| best(modes.iter().filter(fits).filter(|m| m.mhz >= 49_000).collect()))
        .or_else(|| best(modes.iter().filter(fits).collect()))
        .or_else(|| modes.iter().min_by_key(|m| m.w as u64 * m.h as u64).copied())
}

/// pocket-ui's design size: the Mini V2's panel turned to landscape.
pub const DESIGN: (u32, u32) = (1240, 1080);

/// How pocket-ui's pages fit an output of another size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiFit {
    /// The size the pages lay out at.
    pub canvas: (u32, u32),
    /// Output pixels per canvas pixel: 1 (drawn as it is), a whole number (each pixel doubled or more), or below
    /// 1 (an output with fewer than 1080 lines: the 1080-line design scaled down to it).
    pub scale: f32,
}

impl UiFit {
    /// The pages can be drawn straight into the output's buffer.
    pub fn one_to_one(&self) -> bool {
        self.scale == 1.0
    }
}

/// The canvas for an output of `w` x `h`. The design is 1080 lines high (text, rows and the hint bar are sized for
/// it) and 1240 wide at the least; a wider output gives the pages more room, never larger type. 1080 lines and more:
/// the largest whole-number scale that keeps both, so 3840x2160 is 1920x1080 doubled; fewer lines (a 720p
/// screen): the design scaled down to fit, which only `ui_fit`'s caller can do (docs/DISPLAY-OUT.md, phase 2).
pub fn ui_fit(w: u32, h: u32) -> UiFit {
    let (dw, dh) = DESIGN;
    let (w, h) = (w.max(1), h.max(1));
    let k = (h / dh).min(w / dw);
    if k >= 1 {
        return UiFit { canvas: (w / k, h / k), scale: k as f32 };
    }
    let s = (w as f32 / dw as f32).min(h as f32 / dh as f32);
    UiFit { canvas: ((w as f32 / s).round() as u32, (h as f32 / s).round() as u32), scale: s }
}

/// `1920x1080` as a size, for `POCKETUI_SNAPSHOT_SIZE`. Sizes below 320x240 or above 7680x4320 are none.
pub fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once(['x', 'X'])?;
    let (w, h) = (w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?);
    ((320..=7680).contains(&w) && (240..=4320).contains(&h)).then_some((w, h))
}

/// Where the sound should play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// The Pocket: its speakers, the headphone jack, or Bluetooth headphones (whichever PipeWire has as default).
    Device,
    /// The TV's speakers through DisplayPort (the sound card's "HDMI" profile).
    Tv,
}

/// What `sound_route` decides from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoundInputs {
    /// A TV shows pocket-ui's picture.
    pub tv: bool,
    /// The TV said it has speakers (its ELD has audio). `None`: not known, take it that it has.
    pub tv_speakers: Option<bool>,
    /// Headphones are in the Pocket's jack.
    pub headphones: bool,
    /// Bluetooth headphones or a speaker are connected and are PipeWire's default.
    pub bluetooth: bool,
}

/// Headphones win over the TV: someone who plugs them in (or connects Bluetooth ones) wants the sound there, also
/// while the picture is on the TV. A monitor without speakers keeps the sound on the Pocket.
pub fn sound_route(i: SoundInputs) -> Sound {
    if !i.tv || i.headphones || i.bluetooth || i.tv_speakers == Some(false) {
        Sound::Device
    } else {
        Sound::Tv
    }
}

/// The sound card profile for a route (ROCKNIX's UCM for the Mini V2: verbs "HiFi" and "HDMI"), and whether the
/// card has to change from `current` to get there.
pub fn profile_for(route: Sound) -> &'static str {
    match route {
        Sound::Device => "HiFi",
        Sound::Tv => "HDMI",
    }
}

pub fn profile_change(current: Option<&str>, route: Sound) -> Option<&'static str> {
    let want = profile_for(route);
    (current != Some(want)).then_some(want)
}

/// One repair, as a sway command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// The TV to this mode.
    SetMode { output: String, mode: Mode },
    /// The focused workspace (pocket-ui, the game) to the TV.
    WorkspaceTo(String),
    /// The panel dark while the TV shows the picture (powered off, still enabled: when the TV goes, the workspace has
    /// somewhere to go back to at once).
    PanelOff(String),
    /// The panel on again: enabled and powered, turned as ROCKNIX turns it (`panel_transform`) when that is known.
    /// A Pocket started with the TV plugged in never gave sway the panel's transform, so without it the panel would
    /// come back sideways (ROCKNIX PR #3080 saw that on the Odin 2).
    PanelOn { panel: String, transform: Option<u32> },
}

impl Action {
    pub fn sway_command(&self) -> String {
        match self {
            Action::SetMode { output, mode } => format!("output {output} mode {}", mode.sway_arg()),
            Action::WorkspaceTo(output) => format!("move workspace to output {output}"),
            Action::PanelOff(panel) => format!("output {panel} power off"),
            Action::PanelOn { panel, transform: Some(t) } => format!("output {panel} enable power on transform {t}"),
            Action::PanelOn { panel, transform: None } => format!("output {panel} enable power on"),
        }
    }
}

/// The panel's sway transform as ROCKNIX's `111-sway-init` works it out from `/sys/class/graphics/fbcon/rotate`
/// (1 is 90, 2 is 180, 3 is 270, anything else 0).
pub fn panel_transform(fbcon_rotate: &str) -> u32 {
    match fbcon_rotate.trim() {
        "1" => 90,
        "2" => 180,
        "3" => 270,
        _ => 0,
    }
}

/// The repairs for `now`, after `before`. `focused`: the output with the focused workspace; `transform`: the panel's
/// (`panel_transform`). Empty while no TV is or was there, so a panel-only Pocket never gets a command from here (not
/// even a panel someone switched off on purpose, by sleep or by hand). With a TV: its mode when another one fits
/// better, the workspace onto it, the panel dark. The TV gone (it was there `before`): the panel back on if it is not.
pub fn plan(before: &Screens, now: &Screens, focused: Option<&str>, transform: Option<u32>) -> Vec<Action> {
    let mut out = Vec::new();
    match now.tv.as_ref() {
        Some(tv) if tv.active => {
            if let (Some(want), Some(cur)) = (choose_mode(&tv.modes), tv.current) {
                // The same size within a hertz (59.94 for 60) is no reason for another mode change: each one blanks
                // the TV for a second or two while it syncs again.
                if (cur.w, cur.h) != (want.w, want.h) || cur.mhz.abs_diff(want.mhz) > 1000 {
                    out.push(Action::SetMode { output: tv.name.clone(), mode: want });
                }
            }
            if focused.is_some_and(|f| f != tv.name) {
                out.push(Action::WorkspaceTo(tv.name.clone()));
            }
            if let Some(p) = now.panel.as_ref().filter(|p| p.lit()) {
                out.push(Action::PanelOff(p.name.clone()));
            }
        }
        _ => {
            let tv_was_on = before.tv.as_ref().is_some_and(|t| t.active);
            if tv_was_on {
                if let Some(p) = now.panel.as_ref().filter(|p| !p.lit()) {
                    out.push(Action::PanelOn { panel: p.name.clone(), transform });
                }
            }
        }
    }
    out
}

/// What the screen-change messages on the shelf say (`App::display_changed`).
pub fn change_message(before: Option<&Tv>, now: Option<&Tv>) -> Option<&'static str> {
    match (before, now) {
        (None, Some(_)) => Some("Showing on the TV"),
        (Some(_), None) => Some("Back on the built-in screen"),
        _ => None,
    }
}

/// How long after the TV appeared pocket-ui looks whether ROCKNIX has set it up (its `output_monitor` waits 0.3 s
/// for sway, then runs `wlr-randr` twice), before it repairs what is missing.
pub const SETTLE: std::time::Duration = std::time::Duration::from_millis(2500);
/// Events closer together than this are one change (a plug-in is several: the output, its mode, the panel).
pub const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

#[cfg(target_os = "linux")]
pub use watch::watch;

#[cfg(target_os = "linux")]
mod watch {
    use std::io::{self, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::control::{self, ipc};

    /// Listens to sway's output events on a thread of its own, for as long as pocket-ui runs; `send` gets the TV
    /// (or `None`) each time that changes, and once at the start only if a TV is there already. Does nothing where
    /// there is no sway socket (the desktop). Costs one `GET_OUTPUTS` at the start and then a thread asleep on a
    /// socket until an output changes.
    pub fn watch(send: impl Fn(Option<Tv>) -> bool + Send + 'static) {
        let Some(path) = ipc::socket() else { return };
        let _ = std::thread::Builder::new().name("display-out".into()).spawn(move || {
            let mut state = State::new(SETTLE);
            let mut wait = Duration::from_secs(1);
            loop {
                match listen(&path, &mut state, &send) {
                    // pocket-ui is going away (the UI no longer takes messages).
                    Ok(()) => return,
                    Err(e) => {
                        eprintln!("display: sway events lost ({e}), listening again in {wait:?}");
                        std::thread::sleep(wait);
                        wait = (wait * 2).min(Duration::from_secs(30));
                    }
                }
            }
        });
    }

    pub(super) struct State {
        screens: Screens,
        told: Option<Tv>,
        started: bool,
        /// How long a TV that came is left to ROCKNIX (`SETTLE`).
        settle: Duration,
        /// A TV came: no repairs before this.
        settle_until: Option<Instant>,
    }

    impl State {
        pub(super) fn new(settle: Duration) -> State {
            State { screens: Screens::default(), told: None, started: false, settle, settle_until: None }
        }
    }

    enum Next {
        /// Asleep until the next event, or until then at the latest.
        Wait(Option<Instant>),
        /// The UI takes no more messages: pocket-ui is ending.
        UiGone,
    }

    /// One subscription to the sway at `path`: the outputs read once, then again a moment after every burst of
    /// events. `Ok` only when the UI is gone.
    pub(super) fn listen(path: &Path, state: &mut State, send: &impl Fn(Option<Tv>) -> bool) -> io::Result<()> {
        let mut s = UnixStream::connect(path)?;
        write_msg(&mut s, ipc::SUBSCRIBE, r#"["output"]"#)?;
        let (kind, body) = read_msg(&mut s, Some(control::SWAY_TIMEOUT))?;
        if kind != ipc::SUBSCRIBE || !String::from_utf8_lossy(&body).contains("true") {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "sway refused the subscription"));
        }
        // What is there now (a TV plugged in before pocket-ui started included).
        let mut until = match look(path, state, send) {
            Next::Wait(t) => t,
            Next::UiGone => return Ok(()),
        };
        loop {
            // Asleep until an output changes, or until a TV that just came should have been set up by ROCKNIX.
            let event = match read_msg(&mut s, until.map(|at| at.saturating_duration_since(Instant::now()))) {
                Ok((kind, _)) => kind == ipc::EVENT_OUTPUT,
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => false,
                Err(e) => return Err(e),
            };
            if event {
                // The rest of the burst.
                while read_msg(&mut s, Some(DEBOUNCE)).is_ok() {}
            }
            until = match look(path, state, send) {
                Next::Wait(t) => t,
                Next::UiGone => return Ok(()),
            };
        }
    }

    /// Reads the outputs, makes the repairs that are due and tells the UI what changed.
    fn look(path: &Path, state: &mut State, send: &impl Fn(Option<Tv>) -> bool) -> Next {
        let Ok(json) = ipc::request_at(path, ipc::GET_OUTPUTS, "", control::SWAY_TIMEOUT) else { return Next::Wait(None) };
        let now = Screens::from_outputs(parse_outputs(&json));
        let before = std::mem::replace(&mut state.screens, now.clone());
        let on = |s: &Screens| s.tv.as_ref().is_some_and(|t| t.active);
        if on(&now) && !on(&before) {
            state.settle_until = Some(Instant::now() + state.settle);
        }
        if !on(&now) {
            state.settle_until = None;
        }
        let settling = state.settle_until.filter(|at| *at > Instant::now());
        if settling.is_none() {
            repair(path, &before, &now);
        }
        let tv = now.shown_tv();
        // The first look tells the UI only about a TV: a panel-only start sends nothing.
        if tv != state.told || (!state.started && tv.is_some()) {
            match (&tv, now.tv.as_ref()) {
                (Some(t), Some(o)) => println!("display: on {} {}x{} {:.2} Hz ({} {})", t.name, t.w, t.h, t.mhz as f32 / 1000.0, o.make, o.model),
                _ => println!("display: back on the panel"),
            }
            if !send(tv.clone()) {
                return Next::UiGone;
            }
            state.told = tv;
        }
        state.started = true;
        Next::Wait(settling)
    }

    /// `plan`'s commands, sent to sway. A panel-only Pocket is not even asked where its workspace is.
    fn repair(path: &Path, before: &Screens, now: &Screens) {
        if now.tv.is_none() && before.tv.is_none() {
            return;
        }
        let focused = ipc::request_at(path, ipc::GET_WORKSPACES, "", control::SWAY_TIMEOUT).ok().and_then(|j| focused_output(&j));
        let transform = std::fs::read_to_string("/sys/class/graphics/fbcon/rotate").ok().map(|r| panel_transform(&r));
        for a in plan(before, now, focused.as_deref(), transform) {
            let cmd = a.sway_command();
            let ok = ipc::request_at(path, ipc::RUN_COMMAND, &cmd, control::SWAY_TIMEOUT).is_ok_and(|r| ipc::all_succeeded(&r));
            println!("display: {cmd} ({})", if ok { "done" } else { "sway said no" });
        }
    }

    fn write_msg(s: &mut UnixStream, kind: u32, payload: &str) -> io::Result<()> {
        let mut msg = Vec::with_capacity(14 + payload.len());
        msg.extend_from_slice(ipc::MAGIC);
        msg.extend_from_slice(&(payload.len() as u32).to_ne_bytes());
        msg.extend_from_slice(&kind.to_ne_bytes());
        msg.extend_from_slice(payload.as_bytes());
        s.write_all(&msg)
    }

    /// One message (an answer or an event) and its type; `timeout` None waits for as long as it takes.
    fn read_msg(s: &mut UnixStream, timeout: Option<Duration>) -> io::Result<(u32, Vec<u8>)> {
        if timeout.is_some_and(|t| t.is_zero()) {
            return Err(io::ErrorKind::TimedOut.into());
        }
        s.set_read_timeout(timeout)?;
        let mut head = [0u8; 14];
        s.read_exact(&mut head)?;
        let len = u32::from_ne_bytes(head[6..10].try_into().unwrap()) as usize;
        if &head[..6] != ipc::MAGIC || len > 16 << 20 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not sway's IPC"));
        }
        let kind = u32::from_ne_bytes(head[10..14].try_into().unwrap());
        // The rest of a message that has started comes without a time limit of its own.
        s.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut body = vec![0u8; len];
        s.read_exact(&mut body)?;
        Ok((kind, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(w: u32, h: u32, hz: f32) -> Mode {
        Mode { w, h, mhz: (hz * 1000.0).round() as u32 }
    }

    /// The Mini V2 today (2026-10-25): the panel alone, 1080x1240 turned to landscape.
    const PANEL_ONLY: &str = r#"[{"id":3,"type":"output","name":"DSI-1","make":"Unknown","model":"Unknown","serial":"Unknown",
        "active":true,"dpms":true,"power":true,"primary":false,"scale":1.0,"transform":"90","current_workspace":"1",
        "modes":[{"width":1080,"height":1240,"refresh":60000}],"current_mode":{"width":1080,"height":1240,"refresh":60000},
        "rect":{"x":0,"y":0,"width":1240,"height":1080},"focused":true}]"#;

    /// A 4K TV just plugged in: sway enabled it at its preferred mode (4K 30 Hz over two lanes), the panel still lit.
    const TV_JUST_PLUGGED: &str = r#"[
        {"name":"DSI-1","make":"Unknown","model":"Unknown","active":true,"power":true,"transform":"90",
         "modes":[{"width":1080,"height":1240,"refresh":60000}],"current_mode":{"width":1080,"height":1240,"refresh":60000}},
        {"name":"DP-1","make":"Samsung Electric Company","model":"SAMSUNG","active":true,"power":true,"transform":"normal",
         "modes":[{"width":3840,"height":2160,"refresh":30000},{"width":1920,"height":1080,"refresh":60000},
                  {"width":1920,"height":1080,"refresh":59940},{"width":1280,"height":720,"refresh":60000}],
         "current_mode":{"width":3840,"height":2160,"refresh":30000}}]"#;

    /// After ROCKNIX's `output_monitor`: the panel disabled, the TV at 1080p 60 Hz.
    const TV_SET_UP: &str = r#"[
        {"name":"DSI-1","make":"Unknown","model":"Unknown","active":false,"power":false,"transform":"90",
         "modes":[{"width":1080,"height":1240,"refresh":60000}]},
        {"name":"DP-1","make":"Samsung Electric Company","model":"SAMSUNG","active":true,"power":true,"transform":"normal",
         "modes":[{"width":3840,"height":2160,"refresh":30000},{"width":1920,"height":1080,"refresh":60000}],
         "current_mode":{"width":1920,"height":1080,"refresh":60000}}]"#;

    /// Unplugged while the panel was disabled: only the dark panel is left.
    const UNPLUGGED_DARK: &str = r#"[{"name":"DSI-1","make":"Unknown","model":"Unknown","active":false,"power":false,
        "modes":[{"width":1080,"height":1240,"refresh":60000}]}]"#;

    fn screens(json: &str) -> Screens {
        Screens::from_outputs(parse_outputs(json.as_bytes()))
    }

    #[test]
    fn a_panel_only_pocket_gets_no_command_and_no_tv() {
        let s = screens(PANEL_ONLY);
        assert_eq!(s.panel.as_ref().map(|p| p.name.as_str()), Some("DSI-1"));
        assert!(s.tv.is_none());
        assert_eq!(s.shown_tv(), None);
        // Whatever came before, whatever is focused: nothing to do.
        assert!(plan(&Screens::default(), &s, Some("DSI-1"), Some(90)).is_empty());
        assert!(plan(&s, &s, Some("DSI-1"), Some(90)).is_empty());
        assert!(plan(&s, &s, None, Some(90)).is_empty());
        // The panel dark on purpose (sleep, by hand) is left alone too.
        let mut dark = s.clone();
        dark.panel.as_mut().unwrap().power = false;
        assert!(plan(&s, &dark, Some("DSI-1"), Some(90)).is_empty());
        assert!(plan(&dark, &dark, Some("DSI-1"), Some(90)).is_empty());
        assert_eq!(change_message(None, s.shown_tv().as_ref()), None);
    }

    #[test]
    fn nothing_readable_is_no_screens() {
        assert!(parse_outputs(b"").is_empty());
        assert!(parse_outputs(b"{\"error\":1}").is_empty());
        assert_eq!(Screens::from_outputs(parse_outputs(b"[{}]")), Screens::default());
        assert!(plan(&Screens::default(), &Screens::default(), None, None).is_empty());
    }

    #[test]
    fn a_tv_just_plugged_in_gets_1080p_the_workspace_and_a_dark_panel() {
        let s = screens(TV_JUST_PLUGGED);
        assert_eq!(s.shown_tv(), Some(Tv { name: "DP-1".into(), w: 3840, h: 2160, mhz: 30000 }));
        let p = plan(&screens(PANEL_ONLY), &s, Some("DSI-1"), Some(90));
        assert_eq!(
            p.iter().map(Action::sway_command).collect::<Vec<_>>(),
            ["output DP-1 mode 1920x1080@60.000Hz", "move workspace to output DP-1", "output DSI-1 power off"]
        );
    }

    #[test]
    fn what_rocknix_has_set_up_already_is_left_alone() {
        let s = screens(TV_SET_UP);
        assert_eq!(s.shown_tv().map(|t| (t.w, t.h)), Some((1920, 1080)));
        assert!(plan(&screens(TV_JUST_PLUGGED), &s, Some("DP-1"), Some(90)).is_empty());
    }

    #[test]
    fn a_tv_at_1080p_5994_is_not_switched_to_60() {
        let mut s = screens(TV_SET_UP);
        s.tv.as_mut().unwrap().modes.push(m(1920, 1080, 59.94));
        s.tv.as_mut().unwrap().current = Some(m(1920, 1080, 59.94));
        assert!(plan(&s, &s, Some("DP-1"), Some(90)).is_empty());
        // 1080p at 30 Hz is: a whole other refresh.
        s.tv.as_mut().unwrap().current = Some(m(1920, 1080, 30.0));
        assert_eq!(plan(&s, &s, Some("DP-1"), Some(90)), [Action::SetMode { output: "DP-1".into(), mode: m(1920, 1080, 60.0) }]);
    }

    #[test]
    fn unplugged_brings_the_panel_back() {
        let gone = screens(UNPLUGGED_DARK);
        assert_eq!(gone.shown_tv(), None);
        assert_eq!(plan(&screens(TV_SET_UP), &gone, None, Some(90)), [Action::PanelOn { panel: "DSI-1".into(), transform: Some(90) }]);
        assert_eq!(plan(&screens(TV_SET_UP), &gone, None, None)[0].sway_command(), "output DSI-1 enable power on");
        assert_eq!(plan(&screens(TV_SET_UP), &gone, None, Some(90))[0].sway_command(), "output DSI-1 enable power on transform 90");
        assert_eq!((panel_transform("1\n"), panel_transform("3"), panel_transform("0"), panel_transform("")), (90, 270, 0, 0));
        // Already back (ROCKNIX was first): nothing.
        assert!(plan(&screens(TV_SET_UP), &screens(PANEL_ONLY), Some("DSI-1"), Some(90)).is_empty());
        let tv = screens(TV_SET_UP).shown_tv();
        assert_eq!(change_message(tv.as_ref(), None), Some("Back on the built-in screen"));
        assert_eq!(change_message(None, tv.as_ref()), Some("Showing on the TV"));
        assert_eq!(change_message(tv.as_ref(), tv.as_ref()), None);
    }

    #[test]
    fn a_tv_that_is_switched_off_in_sway_is_not_shown_on() {
        let mut s = screens(TV_SET_UP);
        s.tv.as_mut().unwrap().active = false;
        assert_eq!(s.shown_tv(), None);
    }

    #[test]
    fn the_dual_screen_add_on_is_not_a_tv() {
        let json = r#"[{"name":"DSI-1","active":true,"power":true,"modes":[]},
            {"name":"DP-1","make":"Lontium Semiconductor","model":"LT6911","active":true,"power":true,
             "modes":[{"width":1080,"height":1920,"refresh":60000}],"current_mode":{"width":1080,"height":1920,"refresh":60000}}]"#;
        let s = screens(json);
        assert!(s.tv.is_none());
        assert!(plan(&s, &s, Some("DSI-1"), Some(90)).is_empty());
    }

    #[test]
    fn connectors() {
        assert!(is_external("DP-1") && is_external("HDMI-A-1") && is_external("DisplayPort-2"));
        assert!(!is_external("DSI-1") && !is_external("HEADLESS-1") && !is_external("eDP-1"));
        assert!(is_panel("DSI-1") && is_panel("eDP-1") && !is_panel("DP-1"));
    }

    #[test]
    fn the_focused_workspace_names_its_output() {
        let j = br#"[{"name":"1","focused":false,"output":"DSI-1"},{"name":"2","focused":true,"output":"DP-1"}]"#;
        assert_eq!(focused_output(j).as_deref(), Some("DP-1"));
        assert_eq!(focused_output(b"[]"), None);
        assert_eq!(focused_output(b"nonsense"), None);
    }

    #[test]
    fn modes_1080p_60_first() {
        // A 4K TV: 1080p at exactly 60 Hz, not 4K, not 59.94.
        let tv = [m(3840, 2160, 30.0), m(3840, 2160, 60.0), m(1920, 1080, 59.94), m(1920, 1080, 60.0), m(1920, 1080, 50.0), m(1280, 720, 60.0)];
        assert_eq!(choose_mode(&tv), Some(m(1920, 1080, 60.0)));
        assert_eq!(m(1920, 1080, 60.0).sway_arg(), "1920x1080@60.000Hz");
        assert_eq!(m(1920, 1080, 59.94).sway_arg(), "1920x1080@59.940Hz");
        // Only 59.94 there.
        assert_eq!(choose_mode(&[m(1920, 1080, 59.94), m(1920, 1080, 24.0)]), Some(m(1920, 1080, 59.94)));
        // A 144 Hz monitor: 60 Hz, not 144.
        assert_eq!(choose_mode(&[m(1920, 1080, 144.0), m(1920, 1080, 120.0), m(1920, 1080, 60.0)]), Some(m(1920, 1080, 60.0)));
        // A 1440p monitor: its 1080p mode.
        assert_eq!(choose_mode(&[m(2560, 1440, 60.0), m(1920, 1080, 60.0), m(1280, 720, 60.0)]), Some(m(1920, 1080, 60.0)));
        // A 720p screen without 1080p: its largest mode at 60 Hz.
        assert_eq!(choose_mode(&[m(1366, 768, 60.0), m(1280, 720, 60.0), m(1024, 768, 60.0)]), Some(m(1366, 768, 60.0)));
        // A PAL-only screen: 50 Hz rather than nothing.
        assert_eq!(choose_mode(&[m(1920, 1080, 50.0), m(720, 576, 50.0)]), Some(m(1920, 1080, 50.0)));
        // Only modes above 1080 lines (a 4K-only list): the smallest.
        assert_eq!(choose_mode(&[m(3840, 2160, 30.0), m(2560, 1440, 60.0)]), Some(m(2560, 1440, 60.0)));
        assert_eq!(choose_mode(&[]), None);
    }

    #[test]
    fn the_design_fits_1080p_as_it_is_4k_doubled_and_720p_scaled_down() {
        // The panel: unchanged.
        assert_eq!(ui_fit(1240, 1080), UiFit { canvas: (1240, 1080), scale: 1.0 });
        assert!(ui_fit(1240, 1080).one_to_one());
        // 1080p: the same height, more room to the sides.
        assert_eq!(ui_fit(1920, 1080), UiFit { canvas: (1920, 1080), scale: 1.0 });
        // 4K: 1080p with every pixel doubled.
        assert_eq!(ui_fit(3840, 2160), UiFit { canvas: (1920, 1080), scale: 2.0 });
        // 720p: the 1080p canvas at two thirds.
        let f = ui_fit(1280, 720);
        assert_eq!(f.canvas, (1920, 1080));
        assert!((f.scale - 2.0 / 3.0).abs() < 1e-6);
        assert!(!f.one_to_one());
        // 1440p: 1:1 with more room (no blurry 4/3 scale).
        assert_eq!(ui_fit(2560, 1440), UiFit { canvas: (2560, 1440), scale: 1.0 });
        // 4:3 and narrow screens are never narrower than the design.
        assert_eq!(ui_fit(1024, 768).canvas, (1440, 1080));
        assert!(ui_fit(1080, 1920).canvas.0 >= DESIGN.0);
        // Nothing (a size of 0 from a broken mode) is no division by zero.
        assert!(ui_fit(0, 0).canvas.0 >= 1);
    }

    #[test]
    fn snapshot_sizes() {
        assert_eq!(parse_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_size(" 3840X2160 "), Some((3840, 2160)));
        assert_eq!(parse_size("1920"), None);
        assert_eq!(parse_size("0x0"), None);
        assert_eq!(parse_size("99999x1080"), None);
        assert_eq!(parse_size("ax b"), None);
    }

    #[test]
    fn sound_goes_to_the_tv_unless_headphones_or_a_speakerless_monitor() {
        let tv = SoundInputs { tv: true, ..Default::default() };
        assert_eq!(sound_route(SoundInputs::default()), Sound::Device);
        assert_eq!(sound_route(tv), Sound::Tv);
        assert_eq!(sound_route(SoundInputs { tv_speakers: Some(true), ..tv }), Sound::Tv);
        assert_eq!(sound_route(SoundInputs { tv_speakers: Some(false), ..tv }), Sound::Device);
        assert_eq!(sound_route(SoundInputs { headphones: true, ..tv }), Sound::Device);
        assert_eq!(sound_route(SoundInputs { bluetooth: true, ..tv }), Sound::Device);
        // Unplugged: back to the Pocket, whatever the TV had.
        assert_eq!(sound_route(SoundInputs { tv_speakers: Some(true), ..Default::default() }), Sound::Device);
        assert_eq!(profile_change(Some("HiFi"), Sound::Tv), Some("HDMI"));
        assert_eq!(profile_change(Some("HDMI"), Sound::Tv), None);
        assert_eq!(profile_change(Some("HDMI"), Sound::Device), Some("HiFi"));
        assert_eq!(profile_change(Some("HiFi"), Sound::Device), None);
        assert_eq!(profile_change(None, Sound::Device), Some("HiFi"));
    }

    /// The watcher against a fake sway on a socket: the subscription, the events, the outputs it reads and the
    /// commands it sends.
    #[cfg(target_os = "linux")]
    mod watcher {
        use std::io::{Read, Write};
        use std::os::unix::net::{UnixListener, UnixStream};
        use std::sync::mpsc::{channel, Receiver};
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};

        use super::super::watch::{listen, State};
        use super::*;
        use crate::control::ipc;

        #[derive(Default)]
        struct Sway {
            outputs: String,
            workspaces: String,
            asked: Vec<u32>,
            commands: Vec<String>,
            subscriber: Option<UnixStream>,
        }

        fn msg(kind: u32, body: &str) -> Vec<u8> {
            let mut m = ipc::MAGIC.to_vec();
            m.extend_from_slice(&(body.len() as u32).to_ne_bytes());
            m.extend_from_slice(&kind.to_ne_bytes());
            m.extend_from_slice(body.as_bytes());
            m
        }

        fn fake_sway(tag: &str, outputs: &str) -> (crate::testutil::TestDir, std::path::PathBuf, Arc<Mutex<Sway>>) {
            let dir = crate::testutil::TestDir::new(&format!("display-{tag}"));
            let path = dir.join("sway.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let sway = Arc::new(Mutex::new(Sway { outputs: outputs.into(), workspaces: r#"[{"focused":true,"output":"DSI-1"}]"#.into(), ..Default::default() }));
            let shared = sway.clone();
            std::thread::spawn(move || {
                for conn in listener.incoming() {
                    let Ok(mut c) = conn else { return };
                    let sway = shared.clone();
                    std::thread::spawn(move || loop {
                        let mut head = [0u8; 14];
                        if c.read_exact(&mut head).is_err() {
                            return;
                        }
                        let len = u32::from_ne_bytes(head[6..10].try_into().unwrap()) as usize;
                        let kind = u32::from_ne_bytes(head[10..14].try_into().unwrap());
                        let mut body = vec![0u8; len];
                        c.read_exact(&mut body).unwrap();
                        let mut s = sway.lock().unwrap();
                        s.asked.push(kind);
                        let reply = match kind {
                            ipc::SUBSCRIBE => {
                                s.subscriber = Some(c.try_clone().unwrap());
                                r#"{"success":true}"#.to_string()
                            }
                            ipc::GET_OUTPUTS => s.outputs.clone(),
                            ipc::GET_WORKSPACES => s.workspaces.clone(),
                            ipc::RUN_COMMAND => {
                                let cmd = String::from_utf8(body).unwrap();
                                // The one command whose effect the watcher reads back itself.
                                if let Some(out) = cmd.strip_prefix("move workspace to output ") {
                                    s.workspaces = format!(r#"[{{"focused":true,"output":"{out}"}}]"#);
                                }
                                s.commands.push(cmd);
                                r#"[{"success":true}]"#.to_string()
                            }
                            _ => "[]".to_string(),
                        };
                        drop(s);
                        let _ = c.write_all(&msg(kind, &reply));
                    });
                }
            });
            (dir, path, sway)
        }

        /// sway changes its outputs and says so.
        fn change(sway: &Mutex<Sway>, outputs: &str) {
            let mut s = sway.lock().unwrap();
            s.outputs = outputs.into();
            let ev = msg(ipc::EVENT_OUTPUT, r#"{"change":"unspecified"}"#);
            s.subscriber.as_mut().expect("subscribed").write_all(&ev).unwrap();
        }

        fn until(what: &str, within: Duration, mut ok: impl FnMut() -> bool) {
            let end = Instant::now() + within;
            while !ok() {
                assert!(Instant::now() < end, "{what}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        /// Starts the watcher; its messages come out of the receiver. It stops at the first `None` it sends.
        fn start(path: std::path::PathBuf, settle: Duration) -> (Receiver<Option<Tv>>, std::thread::JoinHandle<std::io::Result<()>>) {
            let (tx, rx) = channel();
            let h = std::thread::spawn(move || {
                let mut state = State::new(settle);
                listen(&path, &mut state, &move |tv: Option<Tv>| {
                    let go_on = tv.is_some();
                    tx.send(tv).unwrap();
                    go_on
                })
            });
            (rx, h)
        }

        #[test]
        fn a_panel_only_pocket_is_asked_for_its_outputs_and_nothing_else() {
            let (_dir, path, sway) = fake_sway("panel", PANEL_ONLY);
            let (rx, _h) = start(path.clone(), Duration::from_millis(100));
            until("subscribed", Duration::from_secs(2), || sway.lock().unwrap().asked.contains(&ipc::GET_OUTPUTS));
            // A change that is not a TV (the panel's power, a mode): looked at, nothing done.
            let mut dark: Vec<serde_json::Value> = serde_json::from_str(PANEL_ONLY).unwrap();
            dark[0]["power"] = false.into();
            change(&sway, &serde_json::to_string(&dark).unwrap());
            until("looked again", Duration::from_secs(2), || sway.lock().unwrap().asked.iter().filter(|k| **k == ipc::GET_OUTPUTS).count() == 2);
            std::thread::sleep(Duration::from_millis(300));
            assert!(rx.try_recv().is_err(), "the UI is told nothing");
            let s = sway.lock().unwrap();
            assert!(s.commands.is_empty());
            assert!(s.asked.iter().all(|k| [ipc::SUBSCRIBE, ipc::GET_OUTPUTS].contains(k)), "{:?}", s.asked);
            let _ = std::fs::remove_file(&path);
        }

        #[test]
        fn plugging_in_waits_for_rocknix_then_repairs_and_unplugging_brings_the_panel_back() {
            let (_dir, path, sway) = fake_sway("tv", PANEL_ONLY);
            let settle = Duration::from_millis(400);
            let (rx, h) = start(path.clone(), settle);
            until("subscribed", Duration::from_secs(2), || sway.lock().unwrap().subscriber.is_some());
            let plugged = Instant::now();
            change(&sway, TV_JUST_PLUGGED);
            let tv = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(tv.map(|t| (t.name, t.w, t.h)), Some(("DP-1".to_string(), 3840, 2160)));
            // ROCKNIX's turn first.
            assert!(sway.lock().unwrap().commands.is_empty());
            until("repaired", Duration::from_secs(3), || sway.lock().unwrap().commands.len() == 3);
            assert!(plugged.elapsed() >= settle);
            assert_eq!(
                sway.lock().unwrap().commands,
                ["output DP-1 mode 1920x1080@60.000Hz", "move workspace to output DP-1", "output DSI-1 power off"]
            );
            // sway did it: the TV at 1080p. The UI hears the new size, nothing more is sent to sway.
            change(&sway, TV_SET_UP);
            let tv = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(tv.map(|t| (t.w, t.h)), Some((1920, 1080)));
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(sway.lock().unwrap().commands.len(), 3);
            // Unplugged with the panel dark: back on, and the UI told (which ends this watcher).
            change(&sway, UNPLUGGED_DARK);
            assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), None);
            assert!(h.join().unwrap().is_ok());
            // With the transform this computer's fbcon gives, if it has one.
            assert!(sway.lock().unwrap().commands.last().is_some_and(|c| c.starts_with("output DSI-1 enable power on")));
            let _ = std::fs::remove_file(&path);
        }

        #[test]
        fn a_tv_there_at_the_start_is_told_at_once() {
            let (_dir, path, _sway) = fake_sway("at-start", TV_SET_UP);
            let (rx, _h) = start(path.clone(), Duration::from_millis(50));
            assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap().map(|t| t.name), Some("DP-1".to_string()));
            let _ = std::fs::remove_file(&path);
        }
    }
}
