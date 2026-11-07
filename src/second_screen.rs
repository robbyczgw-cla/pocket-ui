//! The RG DS Plus's lower panel (docs/DUAL-SCREEN.md). Other devices require an explicit testing override.
//!
//! One idea, nothing more: pocket-ui as everywhere on the upper screen, and on the lower one the game the shelf rests on
//! as its box (what X shows: the case with its disc or cartridge sliding out, after a moment the info page with the
//! facts, your play time and the save slots). In a game the lower screen is switched off (battery), except for the
//! systems whose emulator draws on both screens itself (DS, 3DS): those are left to the emulator.
//!
//! **RG DS Plus.** Two 1024x768 panels, both on from boot (ROCKNIX 20261001, `111-sway-init`), touch on the lower one
//! only. Which sway output is which panel, their backlights and the touch screen are best guesses from ROCKNIX's sources
//! until the device confirms them: all of it is `Panels` (one table, one log line at start, and `POCKETUI_DUAL_PANELS`
//! to correct it on the device without a new build; RG-DS-PLUS-FIRST-BOOT.md). Touch on the lower panel is dropped
//! while the second screen is on (`app_touch`): it can never tap the shelf above.
//!
//! The imported Retroid add-on prototype is opt-in only (`POCKETUI_SECOND_OUTPUT`), so existing devices keep their
//! single-window behavior when this branch is merged.
//!
//! Elsewhere nothing here runs: `find` is None, no second window is made, a single screen is as before.
//! This file has the decisions (which output, lit or not, the sway commands); `app_second_screen` the window.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::display_out::Output;

pub use crate::control::SECOND_APP_ID as APP_ID;

/// Names the second screen's output where there is no add-on or RG DS Plus to find: the virtual device's second
/// headless output (`POCKETUI_SECOND_OUTPUT=HEADLESS-2`, docs/DUAL-SCREEN.md). Wins over `Panels` too.
pub const ENV: &str = "POCKETUI_SECOND_OUTPUT";

/// Corrects `Panels` on the device without a new build, e.g. `top=DSI-1 bottom=DSI-2 top_light=backlight0` (any of
/// the keys of `Panels::set`, separated by spaces or commas).
pub const PANELS_ENV: &str = "POCKETUI_DUAL_PANELS";

/// The window's title. Not one of the words ROCKNIX's sway rules send to the lower panel (`111-sway-init`:
/// `.*(Secondary|\[w2\]|Sub|Bottom|Screen 2|GamePad).*`, for the emulators' second windows): our rules place it.
pub const TITLE: &str = "pocket-ui game info";

/// How long the selection rests before the screen below follows it. Scrolling along the shelf paints nothing there.
pub const FOLLOW_MS: u64 = 180;
/// How long the box shows before the info page (the facts, your play, the save slots) takes its place.
pub const TURN_MS: u64 = 4000;

/// The two panels of a handheld with two screens: what the device must confirm, in one place. On the RG DS Plus the
/// best guesses are `Panels::rg_ds_plus`; `POCKETUI_DUAL_PANELS` changes any of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panels {
    /// sway's output of the upper panel: the shelf, menus and games.
    pub top: String,
    /// sway's output of the lower panel: the game info, off in games.
    pub bottom: String,
    /// The upper panel's backlight (`/sys/class/backlight/<top_light>`): the brightness row and the screen-off moves it.
    pub top_light: String,
    /// The lower panel's: off with its output, and following the upper one's brightness.
    pub bottom_light: String,
    /// The touch screen's name (evdev). It lies on the lower panel; pocket-ui drops its touches there.
    pub touch: String,
    /// How the touch screen is turned against the lower panel, in degrees (only logged: no touch is used yet).
    pub touch_turn: u16,
}

impl Panels {
    /// ROCKNIX 20261001's RG DS Plus, read from its sources, not from a device yet:
    /// - outputs: `111-sway-init` takes the last connected connector as the main output (`con`, so DSI-2 of DSI-1 and
    ///   DSI-2) and the other as `second_con` (DSI-1), which it powers off for EmulationStation and DraStic powers on.
    ///   So DSI-2 above, DSI-1 below.
    /// - backlights: `rk3568-anbernic-rg-ds-plus.dts` gives dsi0 (DSI-1) `backlight0` and dsi1 (DSI-2) `backlight1`.
    /// - touch: one Goodix GT911 (`touchscreen-size` 1024x768, landscape like the panel), which the RG DS block of
    ///   `111-sway-init` names `1046:911:Goodix_Capacitive_TouchScreen`.
    pub fn rg_ds_plus() -> Panels {
        Panels {
            top: "DSI-2".into(),
            bottom: "DSI-1".into(),
            top_light: "backlight1".into(),
            bottom_light: "backlight0".into(),
            touch: "Goodix Capacitive TouchScreen".into(),
            touch_turn: 0,
        }
    }

    /// One `key=value`. False for an unknown key or a bad value.
    fn set(&mut self, key: &str, value: &str) -> bool {
        let value = value.trim().to_string();
        if key.trim() != "touch" && !value.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) {
            return false;
        }
        match key.trim() {
            "top" => self.top = value,
            "bottom" => self.bottom = value,
            "top_light" => self.top_light = value,
            "bottom_light" => self.bottom_light = value,
            "touch" => self.touch = value.replace('_', " "),
            "touch_turn" => match value.parse() {
                Ok(d @ (0 | 90 | 180 | 270)) => self.touch_turn = d,
                _ => return false,
            },
            _ => return false,
        }
        true
    }

    /// `POCKETUI_DUAL_PANELS`'s changes; the pairs it could not use, for the log.
    pub fn with(mut self, spec: &str) -> (Panels, Vec<String>) {
        let mut bad = Vec::new();
        for pair in spec.split([' ', ',']).filter(|p| !p.is_empty()) {
            if !pair.split_once('=').is_some_and(|(k, v)| !v.trim().is_empty() && self.set(k, v)) {
                bad.push(pair.to_string());
            }
        }
        if self.top == self.bottom || self.top_light == self.bottom_light {
            bad.push("top and bottom must be different outputs and backlights".into());
            return (Panels::rg_ds_plus(), bad);
        }
        (self, bad)
    }

    /// The device's two panels (None: one screen), with `POCKETUI_DUAL_PANELS` applied.
    pub fn of(model: Option<crate::device::Model>, spec: Option<&str>) -> Option<(Panels, Vec<String>)> {
        let base = match model? {
            crate::device::Model::RgDsPlus => Panels::rg_ds_plus(),
            _ => return None,
        };
        Some(base.with(spec.unwrap_or("")))
    }

    /// This device's, read once.
    pub fn current() -> Option<&'static Panels> {
        static PANELS: OnceLock<Option<Panels>> = OnceLock::new();
        PANELS
            .get_or_init(|| {
                let spec = std::env::var(PANELS_ENV).ok();
                let (p, bad) = Panels::of(crate::device::current().model, spec.as_deref())?;
                if !bad.is_empty() {
                    eprintln!("second screen: {PANELS_ENV}: can't use {}", bad.join(" "));
                }
                Some(p)
            })
            .as_ref()
    }

    /// The table and what the device has, in one line (at start, `app_second_screen`): what to correct is readable
    /// from it. `outputs` are sway's, `lights` the names in /sys/class/backlight. The touch screen that was opened has
    /// its own line (`touch: <name>`).
    pub fn log_line(&self, outputs: &[Output], lights: &[String]) -> String {
        let has_out = |n: &str| outputs.iter().any(|o| o.name == n);
        let has_light = |n: &str| lights.iter().any(|l| l == n);
        let mut wrong = Vec::new();
        for (what, name, there) in [
            ("top", &self.top, has_out(&self.top)),
            ("bottom", &self.bottom, has_out(&self.bottom)),
            ("top_light", &self.top_light, has_light(&self.top_light)),
            ("bottom_light", &self.bottom_light, has_light(&self.bottom_light)),
        ] {
            if !there {
                wrong.push(format!("{what} {name} not found"));
            }
        }
        let outs: Vec<String> = outputs
            .iter()
            .map(|o| {
                let mode = o.current.as_ref().map_or("no mode".to_string(), |m| format!("{}x{}", m.w, m.h));
                format!("{} {mode}{}", o.name, if o.power { "" } else { " off" })
            })
            .collect();
        format!(
            "second screen: panels top {} (light {}), bottom {} (light {}), touch {:?} turned {}; sway has [{}], backlights [{}]; {}",
            self.top,
            self.top_light,
            self.bottom,
            self.bottom_light,
            self.touch,
            self.touch_turn,
            outs.join(", "),
            lights.join(", "),
            if wrong.is_empty() { "all found".to_string() } else { format!("CHECK: {} (set {PANELS_ENV})", wrong.join("; ")) }
        )
    }
}

/// Settings → System → Second screen is there: a handheld with two panels (`Panels`), or a second output named for
/// testing (`ENV`).
pub fn offered() -> bool {
    Panels::current().is_some() || std::env::var_os(ENV).is_some_and(|v| !v.is_empty())
}

/// The two outputs, from sway's: (the shelf's, the second screen's). `named` (from `ENV`) names the second screen and
/// the shelf goes on any other; else the panels of a device with two (`Panels`, both outputs there); else the
/// no automatic second screen on other profiles.
pub fn find(outputs: &[Output], named: Option<&str>, panels: Option<&Panels>) -> Option<(String, String)> {
    if named.is_some_and(|n| !n.is_empty() && !n.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))) { return None; }
    let (main, second) = match (named.filter(|n| !n.is_empty()), panels) {
        (Some(n), _) => (outputs.iter().find(|o| o.active && o.name != n)?, outputs.iter().find(|o| o.active && o.name == n)?),
        (None, Some(p)) if p.top != p.bottom => (outputs.iter().find(|o| o.active && o.name == p.top)?, outputs.iter().find(|o| o.active && o.name == p.bottom)?),
        _ => return None,
    };
    [main, second].iter().all(|o| o.name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)))
        .then(|| (main.name.clone(), second.name.clone()))
}

/// The systems whose emulator puts its second picture on the second screen itself (DraStic and melonDS for DS, Azahar
/// for 3DS). pocket-ui leaves the screen lit for those and takes its window off it.
pub fn emulator_owns_both(system: &str) -> bool {
    matches!(system, "nds" | "3ds")
}

/// What the second screen does now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    /// pocket-ui's window shows the selected game.
    Lit,
    /// Switched off: a game is running that has no use for it, or Settings says Off.
    Off,
    /// Left to the emulator (DS, 3DS): lit, pocket-ui's window off it.
    Emulator,
}

/// `running`: the system of the game that runs now, if any. `on`: Settings → Look → Second screen is Game info.
pub fn power(running: Option<&str>, on: bool) -> Power {
    match running {
        Some(s) if emulator_owns_both(s) => Power::Emulator,
        Some(_) => Power::Off,
        None if on => Power::Lit,
        None => Power::Off,
    }
}

/// Sleep always wins over menu/Game info and the emulator's two-panel request.
pub fn power_awake(running: Option<&str>, on: bool, awake: bool) -> Power {
    if awake { power(running, on) } else { Power::Off }
}

/// The rule that puts the second window on its output, fullscreen, as it opens; and nothing focuses it on its own
/// (games open on the focused output, which stays the shelf's). Set before the window is made: a command sent after it
/// can come before sway has the window and finds nothing. Two rules: `fullscreen enable` chained after the move in one
/// rule leaves the window tiled (Debian trixie's sway, headless, 2026-11-04); on a rule of its own it takes.
pub fn rule_cmd(output: &str) -> String {
    format!(r#"for_window [app_id="{APP_ID}"] move container to output {output}; for_window [app_id="{APP_ID}"] fullscreen enable; no_focus [app_id="{APP_ID}"]"#)
}

/// Puts the shelf's window on its output and the focus there, so that games open there too. ROCKNIX's rules name
/// EmulationStation's window, not pocket-ui's, and the shelf could open on the other screen.
pub fn main_cmd(output: &str) -> String {
    format!(r#"[app_id="{own}"] move container to output {output}; focus output {output}"#, own = crate::control::OWN_APP_ID)
}

/// The sway commands from `from` to `to`, each sent on its own (a command whose criteria find nothing ends its line).
/// Off powers the output off; the others on. Emulator moves our window to the scratchpad, so the emulator's second
/// window has the screen; back to Lit it comes out of there onto its output. Every change ends with the focus on the
/// shelf's output, where the game opens.
pub fn power_cmds(main: &str, output: &str, from: Power, to: Power) -> Vec<String> {
    let mut cmds = vec![format!("output {output} power {}", if to == Power::Off { "off" } else { "on" })];
    let win = format!(r#"[app_id="{APP_ID}"]"#);
    match (from, to) {
        (_, Power::Emulator) => cmds.push(format!("{win} move scratchpad")),
        (Power::Emulator, _) => {
            // Hide first: retrying a partially accepted transition must not toggle a visible window off again.
            cmds.push(format!("{win} move scratchpad"));
            cmds.push(format!("{win} scratchpad show, floating disable, move container to output {output}, fullscreen enable"));
        }
        _ => {}
    }
    cmds.push(format!("focus output {main}"));
    cmds
}

/// `bl_power` for the lower panel's backlight: 0 lit, 4 off (FB_BLANK_POWERDOWN). The panel's own driver may switch it
/// with the output already; writing it as well makes sure the light is off in a game.
pub fn bl_power(p: Power) -> &'static str {
    if p == Power::Off {
        "4"
    } else {
        "0"
    }
}

/// The level `level` of a light whose maximum is `from_max`, on one whose maximum is `to_max` (the lower panel's
/// brightness follows the upper one's).
pub fn same_level(level: u32, from_max: u32, to_max: u32) -> u32 {
    if from_max == 0 {
        return 0;
    }
    ((level as u64 * to_max as u64 + from_max as u64 / 2) / from_max as u64) as u32
}

/// The backlight directory `name` below `root`'s /sys/class/backlight, if it is there.
pub fn light_dir(root: &Path, name: &str) -> Option<PathBuf> {
    let d = root.join("sys/class/backlight").join(name);
    (!name.is_empty() && d.join("brightness").exists()).then_some(d)
}

/// Sets the lower panel's light to `level` of the upper one's maximum `top_max` (brightness row, screen on and off).
pub fn follow_light(root: &Path, level: u32, top_max: u32) {
    let Some(d) = Panels::current().and_then(|p| light_dir(root, &p.bottom_light)) else { return };
    let max = std::fs::read_to_string(d.join("max_brightness")).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(top_max);
    let _ = std::fs::write(d.join("brightness"), same_level(level, top_max, max).to_string());
}

/// Reads sway's outputs on the controller thread: (the
/// shelf's, the second screen's). None without sway or without a second screen. On a device with two panels the
/// table's line is logged once, whatever is found.
#[cfg(target_os = "linux")]
pub fn detect() -> Option<(String, String)> {
    let named = std::env::var(ENV).ok();
    let panels = Panels::current();
    if named.as_deref().is_none_or(str::is_empty) && panels.is_none() {
        return None;
    }
    let json = crate::control::ipc::request(crate::control::ipc::GET_OUTPUTS, "", crate::control::SWAY_TIMEOUT).ok()?;
    let outputs = crate::display_out::parse_outputs(&json);
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if let Some(p) = panels.filter(|_| !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed)) {
        let mut lights: Vec<String> = std::fs::read_dir("/sys/class/backlight")
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        lights.sort();
        println!("{}", p.log_line(&outputs, &lights));
    }
    find(&outputs, named.as_deref(), panels)
}

/// A single worker owns detection, window placement and power writes. No older transition can finish after a
/// newer one. A missing output releases the second window; reconnecting creates a fresh one.
pub struct Controller {
    tx: Option<std::sync::mpsc::Sender<Power>>,
    thread: Option<std::thread::JoinHandle<()>>,
    wanted: Option<Power>,
}

impl Controller {
    #[cfg(target_os = "linux")]
    pub fn start(send: impl Fn(Option<(String, String)>) -> bool + Send + 'static) -> Option<Self> {
        if !offered() { return None; }
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new().name("second-screen".into()).spawn(move || {
            let mut outputs: Option<(String, String)> = None;
            let mut wanted = Power::Off;
            let mut applied = None;
            let mut rule_output: Option<String> = None;
            loop {
                let mut found = detect();
                if found != outputs && found.as_ref().is_some_and(|(main, output)| {
                    if rule_output.as_ref() != Some(output) {
                        if !crate::control::sway_command(&rule_cmd(output)) { return true; }
                        rule_output = Some(output.clone());
                    }
                    !crate::control::sway_command(&main_cmd(main))
                }) { found = None; }
                if found != outputs {
                    // Do not power a disappearing output back on: this can be a suspend modeset. Reconnect
                    // reapplies the latest request, and the launcher handles restoration if the UI ends here.
                    outputs = found;
                    applied = None;
                    if !send(outputs.clone()) { break; }
                }
                if let Some((main, output)) = &outputs {
                    if applied != Some(wanted) {
                        // A failed transition may still have moved the window to or from the scratchpad.
                        // Its previous successful state no longer describes the panel. Treat an unknown
                        // state as hidden so returning to Game info always restores the window explicitly.
                        applied = if apply_power(main, output, applied.unwrap_or(Power::Emulator), wanted) {
                            Some(wanted)
                        } else {
                            None
                        };
                    }
                }
                match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                    Ok(p) => {
                        wanted = p;
                        while let Ok(p) = rx.try_recv() { wanted = p; }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {},
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            if let Some((main, output)) = outputs {
                apply_power(&main, &output, applied.unwrap_or(Power::Emulator), Power::Lit);
            }
        }).ok()?;
        Some(Self { tx: Some(tx), thread: Some(thread), wanted: None })
    }

    pub fn set_power(&mut self, power: Power) {
        if self.wanted != Some(power) {
            if let Some(tx) = &self.tx { let _ = tx.send(power); }
            self.wanted = Some(power);
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}

/// Recovery marker, before either brightness or output writes. Sleep registers it even before output discovery.
pub fn note_restore() -> bool {
    let note = std::env::var_os("POCKETUI_SECOND_RESTORE_FILE").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/pocket-ui-second.restore"));
    std::fs::write(note, "restore\n").is_ok()
}

fn apply_power(main: &str, output: &str, from: Power, to: Power) -> bool {
    let mut ok = true;
    // Register cleanup before the first hardware write: the launcher recovers even SIGKILL/abort.
    if Panels::current().is_some() && !note_restore() { return false; }
    for cmd in power_cmds(main, output, from, to) {
        let done = crate::control::sway_command(&cmd);
        println!("second screen: {cmd} ({})", if done { "done" } else { "sway said no" });
        ok &= done;
    }
    if let Some(d) = Panels::current().and_then(|p| light_dir(Path::new("/"), &p.bottom_light)) {
        let done = std::fs::write(d.join("bl_power"), bl_power(to)).is_ok();
        println!("second screen: {} bl_power {} ({})", d.display(), bl_power(to), if done { "done" } else { "can't write" });
        ok &= done;
    }
    ok
}

/// Recover both lights after a UI crash, including one in light sleep. Called by the launcher, after the game
/// has ended, before restarting or handing over to EmulationStation. None of this runs on other profiles.
pub fn recover_panels(root: &Path, panels: &Panels, saved: Option<u32>) {
    let top = light_dir(root, &panels.top_light);
    let bottom = light_dir(root, &panels.bottom_light);
    // One wrong/missing table entry must not strand the other panel dark after a crash.
    let read = |p: PathBuf| std::fs::read_to_string(p).ok().and_then(|v| v.trim().parse::<u32>().ok());
    let max = top.as_ref().and_then(|d| read(d.join("max_brightness"))).unwrap_or(255).max(1);
    let level = saved.filter(|v| *v > 0).or_else(|| top.as_ref().and_then(|d| read(d.join("brightness"))).filter(|v| *v > 0)).unwrap_or((max / 2).max(1)).min(max);
    if let Some(top) = top {
        let _ = std::fs::write(top.join("brightness"), level.to_string());
        let _ = std::fs::write(top.join("bl_power"), "0");
    }
    if let Some(bottom) = bottom {
        let bottom_max = read(bottom.join("max_brightness")).unwrap_or(max).max(1);
        let _ = std::fs::write(bottom.join("brightness"), same_level(level, max, bottom_max).to_string());
        let _ = std::fs::write(bottom.join("bl_power"), "0");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display_out::Mode;

    fn out(name: &str, make: &str) -> Output {
        Output { name: name.into(), make: make.into(), model: String::new(), active: true, power: true, current: None, modes: vec![] }
    }

    #[test]
    fn a_single_screen_has_no_second_screen() {
        assert_eq!(find(&[out("DSI-1", "")], None, None), None);
        // A TV on USB-C is display out (`display_out`), not a second screen.
        assert_eq!(find(&[out("DSI-1", ""), out("DP-1", "Samsung")], None, None), None);
    }

    #[test]
    fn existing_devices_do_not_auto_activate_the_prototype() {
        let found = find(&[out("DP-1", "Lontium Semiconductor"), out("DSI-1", "")], None, None);
        assert_eq!(found, None, "even a Retroid add-on needs explicit opt-in on this branch");
    }

    #[test]
    fn a_named_output_wins_only_when_it_is_there() {
        let outs = [out("HEADLESS-1", ""), out("HEADLESS-2", "")];
        assert_eq!(find(&outs, Some("HEADLESS-2"), None), Some(("HEADLESS-1".into(), "HEADLESS-2".into())));
        assert_eq!(find(&outs, Some("HEADLESS-3"), None), None);
        // Over the RG DS Plus's table too: the virtual device names its outputs itself.
        assert_eq!(find(&outs, Some("HEADLESS-2"), Some(&Panels::rg_ds_plus())), Some(("HEADLESS-1".into(), "HEADLESS-2".into())));
    }

    /// The RG DS Plus: the table's outputs, in its order whatever order sway lists them in; nothing when one is missing
    /// (the log line says which).
    #[test]
    fn the_rg_ds_plus_puts_the_shelf_on_its_upper_panel() {
        let p = Panels::rg_ds_plus();
        let both = [out("DSI-1", ""), out("DSI-2", "")];
        assert_eq!(find(&both, None, Some(&p)), Some(("DSI-2".into(), "DSI-1".into())));
        assert_eq!(find(&[out("DSI-2", "")], None, Some(&p)), None);
        let (swapped, bad) = p.clone().with("top=DSI-1 bottom=DSI-2");
        assert!(bad.is_empty());
        assert_eq!(find(&both, None, Some(&swapped)), Some(("DSI-1".into(), "DSI-2".into())));
    }

    #[test]
    fn the_table_is_the_rg_ds_plus_alone_and_the_env_corrects_it() {
        use crate::device::Model;
        for model in [Some(Model::MiniV2), Some(Model::Odin2Mini), Some(Model::SteamDeck), crate::device::from_override("rg35xx-plus").model] {
            assert_eq!(Panels::of(model, Some("top=DSI-1 bottom=DSI-2")), None,
                "an override must not activate dual-panel hardware writes on an existing profile: {model:?}");
        }
        assert_eq!(Panels::of(None, Some("top=DSI-1")), None);
        let (p, bad) = Panels::of(Some(Model::RgDsPlus), Some("top_light=backlight0,bottom_light=backlight1 touch=Goodix_X touch_turn=90 nope=1 touch_turn=45 top=")).unwrap();
        assert_eq!((p.top.as_str(), p.bottom.as_str()), ("DSI-2", "DSI-1"));
        assert_eq!((p.top_light.as_str(), p.bottom_light.as_str(), p.touch.as_str(), p.touch_turn), ("backlight0", "backlight1", "Goodix X", 90));
        assert_eq!(bad, ["nope=1", "touch_turn=45", "top="]);
    }

    #[test]
    fn the_log_line_names_what_the_device_must_correct() {
        let p = Panels::rg_ds_plus();
        let mut a = out("DSI-1", "");
        a.current = Some(Mode { w: 1024, h: 768, mhz: 60000 });
        a.power = false;
        let lights = vec!["backlight0".to_string(), "backlight1".to_string()];
        let line = p.log_line(&[a.clone(), out("DSI-2", "")], &lights);
        assert!(line.contains("top DSI-2 (light backlight1), bottom DSI-1 (light backlight0)"), "{line}");
        assert!(line.contains("sway has [DSI-1 1024x768 off, DSI-2 no mode]") && line.ends_with("all found"), "{line}");
        let line = p.log_line(&[a], &["panel".to_string()]);
        assert!(line.contains("CHECK: top DSI-2 not found; top_light backlight1 not found; bottom_light backlight0 not found (set POCKETUI_DUAL_PANELS)"), "{line}");
    }

    #[test]
    fn games_switch_it_off_but_ds_and_3ds_keep_it() {
        assert_eq!(power(None, true), Power::Lit);
        assert_eq!(power(None, false), Power::Off, "Settings: Off");
        assert_eq!(power(Some("snes"), true), Power::Off);
        assert_eq!(power(Some("nds"), true), Power::Emulator);
        assert_eq!(power(Some("nds"), false), Power::Emulator, "a DS game needs both screens whatever the setting");
        assert_eq!(power(Some("3ds"), true), Power::Emulator);
        assert_eq!((bl_power(Power::Off), bl_power(Power::Lit), bl_power(Power::Emulator)), ("4", "0", "0"));
    }

    #[test]
    fn the_power_commands_hand_the_screen_to_the_emulator_and_back() {
        let w = r#"[app_id="pocket-ui-second"]"#;
        assert_eq!(power_cmds("DSI-2", "DSI-1", Power::Lit, Power::Off), ["output DSI-1 power off", "focus output DSI-2"]);
        assert_eq!(power_cmds("DSI-2", "DSI-1", Power::Off, Power::Lit), ["output DSI-1 power on", "focus output DSI-2"]);
        assert_eq!(power_cmds("DSI-2", "DSI-1", Power::Lit, Power::Emulator), ["output DSI-1 power on".to_string(), format!("{w} move scratchpad"), "focus output DSI-2".into()]);
        assert_eq!(
            power_cmds("DSI-2", "DSI-1", Power::Emulator, Power::Lit),
            ["output DSI-1 power on".to_string(), format!("{w} move scratchpad"), format!("{w} scratchpad show, floating disable, move container to output DSI-1, fullscreen enable"), "focus output DSI-2".into()]
        );
    }

    #[test]
    fn the_second_window_goes_to_its_output_as_it_opens() {
        assert_eq!(
            rule_cmd("DSI-1"),
            r#"for_window [app_id="pocket-ui-second"] move container to output DSI-1; for_window [app_id="pocket-ui-second"] fullscreen enable; no_focus [app_id="pocket-ui-second"]"#
        );
        assert_eq!(main_cmd("DSI-2"), r#"[app_id="pocket-ui"] move container to output DSI-2; focus output DSI-2"#);
    }

    /// ROCKNIX's rule for the emulators' second windows must not catch ours (it would move and power it on its own).
    #[test]
    fn the_title_is_none_of_rocknixs_second_window_words() {
        for word in ["Secondary", "[w2]", "Sub", "Bottom", "Screen 2", "GamePad"] {
            assert!(!TITLE.contains(word), "{word}");
        }
    }

    #[test]
    fn the_lower_light_follows_the_upper_ones_level() {
        assert_eq!(same_level(128, 255, 255), 128);
        assert_eq!(same_level(255, 255, 1023), 1023);
        assert_eq!(same_level(51, 255, 100), 20);
        assert_eq!(same_level(5, 0, 100), 0);
    }
    #[test]
    fn sleep_keeps_both_menu_and_ds_requests_dark() {
        for running in [None, Some("snes"), Some("nds")] {
            for on in [false, true] { assert_eq!(power_awake(running, on, false), Power::Off); }
        }
        assert_eq!(power_awake(None, true, true), Power::Lit);
        assert_eq!(power_awake(Some("nds"), false, true), Power::Emulator);
    }

    #[test]
    fn a_disabled_or_missing_panel_is_not_a_second_screen() {
        let p = Panels::rg_ds_plus();
        let mut bottom = out(&p.bottom, "");
        bottom.active = false;
        assert_eq!(find(&[out(&p.top, ""), bottom], None, Some(&p)), None);
        let (bad, errors) = p.with("top=DSI-1 bottom=DSI-1");
        assert!(!errors.is_empty());
        assert_ne!(bad.top, bad.bottom, "a bad override must never power off the upper screen");
        let (_, errors) = bad.with("bottom_light=../../backlight1 top=DSI-2;power");
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn a_crash_in_a_game_or_sleep_restores_both_lights() {
        let root = tempfile::tempdir().unwrap();
        let panels = Panels::rg_ds_plus();
        for (light, max) in [(&panels.top_light, 255), (&panels.bottom_light, 1023)] {
            let dir = root.path().join("sys/class/backlight").join(light);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("brightness"), "0").unwrap();
            std::fs::write(dir.join("bl_power"), "4").unwrap();
            std::fs::write(dir.join("max_brightness"), max.to_string()).unwrap();
        }
        recover_panels(root.path(), &panels, Some(128));
        let top = light_dir(root.path(), &panels.top_light).unwrap();
        let bottom = light_dir(root.path(), &panels.bottom_light).unwrap();
        assert_eq!(std::fs::read_to_string(top.join("brightness")).unwrap(), "128");
        assert_eq!(std::fs::read_to_string(bottom.join("brightness")).unwrap(), "514");
        for light in [&top, &bottom] { assert_eq!(std::fs::read_to_string(light.join("bl_power")).unwrap(), "0"); }
        // No saved brightness (e.g. a crash during a game): even a driver left at zero recovers visibly.
        std::fs::write(top.join("brightness"), "0").unwrap();
        recover_panels(root.path(), &panels, None);
        assert!(std::fs::read_to_string(top.join("brightness")).unwrap().parse::<u32>().unwrap() > 0);
        assert!(std::fs::read_to_string(bottom.join("brightness")).unwrap().parse::<u32>().unwrap() > 0);
    }

    #[test]
    fn a_missing_upper_backlight_does_not_strand_the_lower_panel_dark() {
        let root = tempfile::tempdir().unwrap();
        let panels = Panels::rg_ds_plus();
        let bottom = root.path().join("sys/class/backlight").join(&panels.bottom_light);
        std::fs::create_dir_all(&bottom).unwrap();
        std::fs::write(bottom.join("brightness"), "0").unwrap();
        std::fs::write(bottom.join("max_brightness"), "255").unwrap();
        std::fs::write(bottom.join("bl_power"), "4").unwrap();
        recover_panels(root.path(), &panels, None);
        assert_eq!(std::fs::read_to_string(bottom.join("bl_power")).unwrap(), "0");
        assert!(std::fs::read_to_string(bottom.join("brightness")).unwrap().parse::<u32>().unwrap() > 0);
    }

}
