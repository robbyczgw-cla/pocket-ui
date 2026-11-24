//! RetroArch's HOME shortcuts on ROCKNIX: the keys of pocket-core's own host (Options → Shortcuts, `hotkeys`) for a
//! game that plays in RetroArch, and the same switches.
//!
//! - **Where they come from.** runemu.sh starts RetroArch with the player's `retroarch.cfg` and an appendconfig that
//!   setsettings.sh writes anew at every start (`/tmp/.retroarch.cfg`), so there is no file of ours to give it. With
//!   `system.autohotkeys=1` (ROCKNIX's start value) setsettings.sh's `configure_hotkeys` also rewrites the hotkey lines in
//!   `retroarch.cfg` at every start, from the built-in pad's autoconfig in `/tmp/joypads`: the hotkey button, HOME + R1
//!   save, L1 load, R2 fast-forward, L2 rewind, START quit, Y the FPS, X RetroArch's menu. pocket-ui does that step
//!   itself before each RetroArch start instead (`prepare`): the same lines from the same autoconfig, with the
//!   switches of Options → Shortcuts applied, and `system.autohotkeys=0` so that setsettings.sh leaves them alone.
//!   `prepare` also enables the pause menu's network commands (`control::retroarch`) at the port pocket-ui sends to.
//! - **The hotkey button** is the autoconfig's `input_enable_hotkey_btn` (button 10, BTN_MODE: HOME on the H700 pad
//!   and the InputPlumber DualSense of the Retroid and AYN handhelds), Select where the autoconfig names none, as in
//!   setsettings.sh.
//! - **A shortcut that is off** gets a button and an axis no pad has (`NEVER_BTN`, `NEVER_AXIS`): RetroArch's "nul"
//!   would mean "unset" and fall back to the autoconfig's own binding (input_driver.c: `bind_joykey != NO_BTN ?
//!   bind_joykey : autobind_joykey`), which has save, load and the triggers on it. udev pads have 64 buttons and 32
//!   axes at most (udev_joypad.c), so these never fire.
//! - **HOME + A** is pocket-ui's screenshot there (`app_screenshot`: it lands in the gallery's folder under its usual
//!   name), so RetroArch's screenshot key and its pause key, which the H700 and DualSense autoconfigs put on A, are
//!   off. **HOME + SELECT** opens Recent games (pocket-ui), as in every game.
//! - **Not in RetroArch games:** HOME + Left / Right (pocket-ui gives RetroArch one save: the pause menu's Save and Load
//!   are its slot 0, `quit::Saver::RetroArch`, and a slot changed behind it would make them miss), Zoom (pocket-core's
//!   picture). HOME + X stays RetroArch's menu and HOME + START its quit, as ROCKNIX set them.

use std::collections::BTreeMap;
use std::path::Path;

use crate::hotkeys::Enabled;

/// The player's RetroArch configuration, which runemu.sh starts RetroArch with.
pub const RETROARCH_CFG: &str = "/storage/.config/retroarch/retroarch.cfg";
/// RetroArch's autoconfig folder on ROCKNIX (`joypad_autoconfig_dir`, the tmp-joypads mount).
pub const JOYPADS: &str = "/tmp/joypads";
/// The kernel's list of input devices, which setsettings.sh picks the built-in pad from.
pub const INPUT_DEVICES: &str = "/proc/bus/input/devices";
/// setsettings.sh's switch: "1" makes it write the hotkey lines at every start (`configure_hotkeys`).
pub const AUTOHOTKEYS: &str = "system.autohotkeys";

/// A button no udev pad has (it has 64 at most).
pub const NEVER_BTN: &str = "100";
/// An axis no udev pad has (32 at most).
pub const NEVER_AXIS: &str = "+100";

/// The InputPlumber pad setsettings.sh takes first when it is there.
const DUALSENSE: &str = "Sony Interactive Entertainment DualSense Wireless Controller";

/// One binding of the pad: a button (or a hat direction, "h0left") or an axis ("+5").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bind {
    Btn(String),
    Axis(String),
}

/// The buttons of the built-in pad the shortcuts use, from its RetroArch autoconfig.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pad {
    pub hotkey: String,
    pub a: Option<String>,
    pub x: Option<String>,
    pub y: Option<String>,
    pub l: Option<String>,
    pub r: Option<String>,
    pub start: Option<String>,
    pub l2: Option<Bind>,
    pub r2: Option<Bind>,
}

/// The value of `key` in a RetroArch config text (`key = "value"`, the last line wins as in RetroArch).
pub fn value<'a>(cfg: &'a str, key: &str) -> Option<&'a str> {
    cfg.lines().filter_map(|l| parse_line(l)).filter(|(k, _)| *k == key).map(|(_, v)| v).last()
}

fn parse_line(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.split_once('=')?;
    let k = k.trim();
    let v = v.trim();
    let v = v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).unwrap_or(v);
    (!k.is_empty() && !k.starts_with('#')).then_some((k, v))
}

impl Pad {
    /// The pad from its autoconfig text. L2 and R2 are buttons when the pad has both as buttons, else axes when it has
    /// both as axes (setsettings.sh's order); None when it has neither.
    pub fn from_autoconfig(cfg: &str) -> Pad {
        let get = |k: &str| value(cfg, k).filter(|v| !v.is_empty() && *v != "nul").map(str::to_string);
        let select = get("input_select_btn");
        let hotkey = get("input_enable_hotkey_btn").or(select).unwrap_or_else(|| NEVER_BTN.to_string());
        let (l2, r2) = match (get("input_l2_btn"), get("input_r2_btn"), get("input_l2_axis"), get("input_r2_axis")) {
            (Some(l), Some(r), _, _) => (Some(Bind::Btn(l)), Some(Bind::Btn(r))),
            (_, _, Some(l), Some(r)) => (Some(Bind::Axis(l)), Some(Bind::Axis(r))),
            _ => (None, None),
        };
        Pad { hotkey, a: get("input_a_btn"), x: get("input_x_btn"), y: get("input_y_btn"), l: get("input_l_btn"), r: get("input_r_btn"), start: get("input_start_btn"), l2, r2 }
    }
}

/// The name setsettings.sh looks the built-in pad's autoconfig up by: the InputPlumber DualSense when it is there, else
/// the device behind `js0`, else the first "joypad".
pub fn controller_name(devices: &str) -> Option<String> {
    if devices.contains(DUALSENSE) {
        return Some(DUALSENSE.to_string());
    }
    let blocks: Vec<&str> = devices.split("\n\n").collect();
    let name = |b: &str| b.lines().find_map(|l| l.strip_prefix("N: Name=")).map(|n| n.trim().trim_matches('"').to_string());
    let handlers = |b: &str| b.lines().find_map(|l| l.strip_prefix("H: Handlers=")).unwrap_or("").split_whitespace().any(|h| h == "js0");
    blocks
        .iter()
        .find(|b| handlers(b))
        .and_then(|b| name(b))
        .or_else(|| blocks.iter().find(|b| b.contains("joypad")).and_then(|b| name(b)))
        .filter(|n| !n.is_empty())
}

/// A button binding, or off.
fn btn(on: bool, b: &Option<String>) -> String {
    b.as_ref().filter(|_| on).cloned().unwrap_or_else(|| NEVER_BTN.to_string())
}

/// `<action>_btn` and `<action>_axis` for a trigger: its button or its axis, the other one unset; off, both never.
fn trigger(out: &mut Vec<(String, String)>, action: &str, on: bool, b: &Option<Bind>) {
    let (button, axis) = match b.as_ref().filter(|_| on) {
        Some(Bind::Btn(v)) => (v.clone(), "nul".to_string()),
        Some(Bind::Axis(v)) => ("nul".to_string(), v.clone()),
        None => (NEVER_BTN.to_string(), NEVER_AXIS.to_string()),
    };
    out.push((format!("{action}_btn"), button));
    out.push((format!("{action}_axis"), axis));
}

/// The hotkey lines for `retroarch.cfg`: HOME as the hotkey button and every shortcut that `on` has switched on.
pub fn lines(pad: &Pad, on: Enabled) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |k: &str, v: String| out.push((k.to_string(), v));
    push("input_enable_hotkey_btn", pad.hotkey.clone());
    // ROCKNIX's two that pocket-core has no switch for: HOME + START quits (RetroArch writes its auto state first),
    // HOME + X is RetroArch's menu.
    push("input_exit_emulator_btn", btn(true, &pad.start));
    push("input_menu_toggle_btn", btn(true, &pad.x));
    push("input_save_state_btn", btn(on.save, &pad.r));
    push("input_save_state_axis", NEVER_AXIS.into());
    push("input_load_state_btn", btn(on.load, &pad.l));
    push("input_load_state_axis", NEVER_AXIS.into());
    // HOME + Y: Over the game's FPS there, RetroArch's own FPS line here.
    push("input_fps_toggle_btn", btn(on.over_game, &pad.y));
    push("input_fps_toggle_axis", NEVER_AXIS.into());
    // HOME + A is pocket-ui's screenshot: RetroArch's screenshot and pause keys (the autoconfigs' A and B) are off.
    push("input_screenshot_btn", NEVER_BTN.into());
    push("input_screenshot_axis", NEVER_AXIS.into());
    push("input_pause_toggle_btn", NEVER_BTN.into());
    push("input_pause_toggle_axis", NEVER_AXIS.into());
    // Fast-forward turns on and off with HOME + R2, as in pocket-core: the DualSense autoconfig's hold on the same
    // trigger is off.
    trigger(&mut out, "input_toggle_fast_forward", on.fast_forward, &pad.r2);
    out.push(("input_hold_fast_forward_btn".into(), NEVER_BTN.into()));
    out.push(("input_hold_fast_forward_axis".into(), NEVER_AXIS.into()));
    trigger(&mut out, "input_rewind", on.rewind, &pad.l2);
    out
}

/// `cfg` with `lines` in it: each key's lines replaced by one `key = "value"` where the first of them stood, the
/// missing keys added at the end. Every other line stays as it is.
pub fn with_lines(cfg: &str, lines: &[(String, String)]) -> String {
    let wanted: BTreeMap<&str, &str> = lines.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut written = std::collections::BTreeSet::new();
    let mut out = String::with_capacity(cfg.len() + 1024);
    for line in cfg.lines() {
        match parse_line(line).and_then(|(k, _)| wanted.get_key_value(k)) {
            Some((k, v)) => {
                if written.insert(*k) {
                    out.push_str(&format!("{k} = \"{v}\"\n"));
                }
            }
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    for (k, v) in lines {
        if written.insert(k.as_str()) {
            out.push_str(&format!("{k} = \"{v}\"\n"));
        }
    }
    out
}

/// The files `prepare` reads and writes, so that tests can point it elsewhere.
pub struct Paths<'a> {
    pub retroarch_cfg: &'a Path,
    pub joypads: &'a Path,
    pub input_devices: &'a Path,
    pub system_cfg: &'a Path,
}

impl Paths<'static> {
    pub fn device() -> Paths<'static> {
        Paths {
            retroarch_cfg: Path::new(RETROARCH_CFG),
            joypads: Path::new(JOYPADS),
            input_devices: Path::new(INPUT_DEVICES),
            system_cfg: Path::new(crate::system_cfg::PATH),
        }
    }
}

/// Before a RetroArch start, enable the pause menu commands in the player's config and apply the game's hotkeys.
/// Returns whether pocket-ui prepared the hotkeys. Without a pad autoconfig, ROCKNIX still configures them.
pub fn prepare(paths: &Paths, on: Enabled) -> std::io::Result<bool> {
    let Ok(cfg) = std::fs::read_to_string(paths.retroarch_cfg) else { return Ok(false) };
    let devices = std::fs::read_to_string(paths.input_devices).unwrap_or_default();
    let autoconfig = controller_name(&devices)
        .and_then(|name| std::fs::read_to_string(paths.joypads.join(format!("{name}.cfg"))).ok());
    let mut settings = vec![
        ("network_cmd_enable".into(), "true".into()),
        ("network_cmd_port".into(), crate::control::RETROARCH_CMD_PORT.to_string()),
    ];
    if let Some(autoconfig) = &autoconfig {
        settings.extend(lines(&Pad::from_autoconfig(autoconfig), on));
    }
    let new = with_lines(&cfg, &settings);
    if new != cfg {
        crate::fsutil::write_atomic(paths.retroarch_cfg, new.as_bytes())?;
    }
    if autoconfig.is_none() {
        return Ok(false);
    }
    let sys = crate::system_cfg::SystemCfg::at(paths.system_cfg);
    if sys.get(AUTOHOTKEYS).as_deref() != Some("0") {
        sys.set(AUTOHOTKEYS, "0")?;
    }
    Ok(true)
}

/// The shortcuts a RetroArch game gets: the game's (or its console's) Options → Shortcuts, none in Kids Mode, no HOME +
/// Y in Easy (`extras::allowed_for`).
pub fn enabled(values: &BTreeMap<String, String>, kids: bool, easy: bool) -> Enabled {
    crate::extras::allowed_for(crate::extras::Settings::from_values(values).shortcuts, kids, easy)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ROCKNIX's `H700 Gamepad.cfg` (retroarch-joypads, 2026-10-01), the lines that matter here.
    const H700: &str = r#"input_driver = "udev"
input_device = "H700 Gamepad"
input_b_btn = "0"
input_a_btn = "1"
input_y_btn = "2"
input_x_btn = "3"
input_l_btn = "4"
input_r_btn = "5"
input_l2_btn = "6"
input_r2_btn = "7"
input_select_btn = "8"
input_start_btn = "9"
input_left_btn = "15"
input_right_btn = "16"
input_enable_hotkey_btn = "10"
input_exit_emulator_btn = "9"
input_pause_toggle_btn = "1"
input_menu_toggle_btn = "3"
input_fps_toggle_btn = "2"
input_screenshot_btn = "nul"
input_load_state_btn = "4"
input_save_state_btn = "5"
input_toggle_fast_forward_btn = "7"
input_rewind_btn = "6"
"#;

    /// ROCKNIX's InputPlumber DualSense autoconfig (the Retroid Pocket Mini V2, the AYN Odin 2): triggers as axes.
    const DS5: &str = r#"input_device = "Sony Interactive Entertainment DualSense Wireless Controller"
input_a_btn = "1"
input_b_btn = "0"
input_x_btn = "2"
input_y_btn = "3"
input_l_btn = "4"
input_r_btn = "5"
input_select_btn = "8"
input_start_btn = "9"
input_up_btn = "h0up"
input_left_btn = "h0left"
input_right_btn = "h0right"
input_l2_axis = "+2"
input_r2_axis = "+5"
input_enable_hotkey_btn = "10"
input_exit_emulator_btn = "9"
input_menu_toggle_btn = "2"
input_pause_toggle_btn = "1"
input_fps_toggle_btn = "3"
input_screenshot_btn = "0"
input_load_state_btn = "4"
input_save_state_btn = "5"
input_rewind_axis = "+2"
input_hold_fast_forward_axis = "+5"
"#;

    /// ROCKNIX's `retrogame_joypad.cfg` (RK3566): no HOME button of its own, Select is the hotkey button.
    const RK3566: &str = r#"input_select_btn = "8"
input_start_btn = "9"
input_a_btn = "1"
input_l_btn = "4"
input_r_btn = "5"
input_l2_btn = "6"
input_r2_btn = "7"
input_enable_hotkey_btn = "8"
"#;

    fn get(lines: &[(String, String)], key: &str) -> String {
        lines.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_else(|| panic!("no {key}"))
    }

    fn on() -> Enabled {
        Enabled::default()
    }

    /// The pocket-core keys in RetroArch: HOME is the hotkey button, R1 saves, L1 loads, R2 fast-forward, L2 rewind,
    /// Y the FPS; START quits and X opens RetroArch's menu as ROCKNIX set them.
    #[test]
    fn retroarch_gets_pocket_cores_keys_on_the_h700_pad() {
        let l = lines(&Pad::from_autoconfig(H700), on());
        assert_eq!(get(&l, "input_enable_hotkey_btn"), "10", "HOME (BTN_MODE) is the hotkey button");
        assert_eq!(get(&l, "input_save_state_btn"), "5", "HOME + R1 saves");
        assert_eq!(get(&l, "input_load_state_btn"), "4", "HOME + L1 loads");
        assert_eq!(get(&l, "input_toggle_fast_forward_btn"), "7", "HOME + R2 fast-forward");
        assert_eq!(get(&l, "input_rewind_btn"), "6", "hold HOME + L2 rewinds");
        assert_eq!(get(&l, "input_fps_toggle_btn"), "2", "HOME + Y");
        assert_eq!(get(&l, "input_exit_emulator_btn"), "9");
        assert_eq!(get(&l, "input_menu_toggle_btn"), "3");
    }

    #[test]
    fn the_dualsense_triggers_are_axes_and_fast_forward_only_toggles() {
        let l = lines(&Pad::from_autoconfig(DS5), on());
        assert_eq!(get(&l, "input_enable_hotkey_btn"), "10");
        assert_eq!((get(&l, "input_toggle_fast_forward_btn"), get(&l, "input_toggle_fast_forward_axis")), ("nul".into(), "+5".into()));
        assert_eq!((get(&l, "input_rewind_btn"), get(&l, "input_rewind_axis")), ("nul".into(), "+2".into()));
        // The autoconfig's hold on R2 would end a toggled fast-forward when the trigger is let go.
        assert_eq!((get(&l, "input_hold_fast_forward_btn"), get(&l, "input_hold_fast_forward_axis")), (NEVER_BTN.into(), NEVER_AXIS.into()));
    }

    /// HOME + A is pocket-ui's screenshot in every game: RetroArch's pause (A on both pads) and screenshot (B on the
    /// DualSense) must not fire beside it.
    #[test]
    fn home_a_is_left_to_pocket_ui() {
        for pad in [H700, DS5] {
            let l = lines(&Pad::from_autoconfig(pad), on());
            for key in ["input_pause_toggle_btn", "input_screenshot_btn"] {
                assert_eq!(get(&l, key), NEVER_BTN, "{key}");
            }
        }
    }

    /// Off in Options → Shortcuts is off in RetroArch too: never a button or axis, not "nul", which RetroArch fills from
    /// the autoconfig (whose save, load, rewind and fast-forward are on these very buttons).
    #[test]
    fn switched_off_shortcuts_are_off_in_retroarch_and_not_nul() {
        for pad in [H700, DS5] {
            let off = Enabled { save: false, load: false, fast_forward: false, rewind: false, over_game: false, ..on() };
            let l = lines(&Pad::from_autoconfig(pad), off);
            for key in ["input_save_state", "input_load_state", "input_toggle_fast_forward", "input_rewind", "input_fps_toggle"] {
                assert_eq!(get(&l, &format!("{key}_btn")), NEVER_BTN, "{key}");
                assert_eq!(get(&l, &format!("{key}_axis")), NEVER_AXIS, "{key}");
            }
            // The hotkey button, quit and RetroArch's menu stay.
            assert_eq!(get(&l, "input_enable_hotkey_btn"), "10");
            assert_eq!(get(&l, "input_exit_emulator_btn"), "9");
        }
        // One switch off leaves the others on.
        let l = lines(&Pad::from_autoconfig(H700), Enabled { save: false, ..on() });
        assert_eq!(get(&l, "input_save_state_btn"), NEVER_BTN);
        assert_eq!(get(&l, "input_load_state_btn"), "4");
    }

    #[test]
    fn a_pad_without_a_home_button_keeps_rocknix_hotkey_button() {
        assert_eq!(Pad::from_autoconfig(RK3566).hotkey, "8");
        assert_eq!(Pad::from_autoconfig("input_select_btn = \"7\"\n").hotkey, "7", "Select, as setsettings.sh");
        // No trigger at all: fast-forward and rewind are off rather than on some other button.
        let l = lines(&Pad::from_autoconfig("input_select_btn = \"7\"\n"), on());
        assert_eq!(get(&l, "input_rewind_btn"), NEVER_BTN);
    }

    #[test]
    fn kids_mode_and_easy() {
        let values = BTreeMap::new();
        assert_eq!(enabled(&values, true, false), Enabled::none());
        let easy = enabled(&values, false, true);
        assert!(!easy.over_game && easy.save && easy.rewind, "Easy: no HOME + Y, the rest as set");
        let mut off = BTreeMap::new();
        off.insert("hk_ff".to_string(), "off".to_string());
        off.insert("hk_save".to_string(), "on".to_string());
        let e = enabled(&off, false, false);
        assert!(!e.fast_forward && e.save);
    }

    /// Only the hotkey keys change; the player's other lines stay, a key that stood twice is there once.
    #[test]
    fn only_the_hotkey_lines_of_the_players_config_change() {
        let cfg = "video_driver = \"gl\"\n# a comment\ninput_save_state_btn = \"5\"\nsavestate_auto_save = \"true\"\ninput_save_state_btn = \"5\"\n";
        let out = with_lines(cfg, &[("input_save_state_btn".into(), NEVER_BTN.into()), ("input_screenshot_btn".into(), NEVER_BTN.into())]);
        assert_eq!(out, "video_driver = \"gl\"\n# a comment\ninput_save_state_btn = \"100\"\nsavestate_auto_save = \"true\"\ninput_screenshot_btn = \"100\"\n");
        assert_eq!(with_lines(&out, &[("input_save_state_btn".into(), NEVER_BTN.into()), ("input_screenshot_btn".into(), NEVER_BTN.into())]), out, "a second start changes nothing");
    }

    #[test]
    fn the_built_in_pad_is_found_as_setsettings_finds_it() {
        let h700 = "I: Bus=0019 Vendor=484b Product=1121 Version=0100\nN: Name=\"H700 Gamepad\"\nP: Phys=\nH: Handlers=event1 js0\nB: KEY=1\n\nI: Bus=0019\nN: Name=\"axp20x-pek\"\nH: Handlers=kbd event0\n";
        assert_eq!(controller_name(h700).as_deref(), Some("H700 Gamepad"));
        let mini = format!("N: Name=\"Retroid Pocket Controller\"\nH: Handlers=event2 js1\n\nN: Name=\"{DUALSENSE}\"\nH: Handlers=event5 js0\n");
        assert_eq!(controller_name(&mini).as_deref(), Some(DUALSENSE), "InputPlumber's pad first");
        assert_eq!(controller_name("N: Name=\"gpio-keys\"\nH: Handlers=kbd event0\n"), None);
    }

    /// Every input `prepare` reads, missing, empty, cut, flipped or not text (`file_faults`): never a panic, never a
    /// hotkey line from garbage, and the player's other lines kept. A cut autoconfig without the hotkey line falls back to
    /// Select as setsettings.sh does. Without a pad name ROCKNIX keeps managing the hotkeys.
    #[test]
    fn broken_inputs_never_break_the_players_config() {
        let base = crate::testutil::TestDir::new("ra-hotkeys-faults");
        let devices_seed = b"N: Name=\"H700 Gamepad\"\nH: Handlers=event1 js0\n".to_vec();
        let cfg_seed = b"video_driver = \"gl\"\ninput_save_state_btn = \"5\"\nnetwork_cmd_enable = \"true\"\nnetwork_cmd_port = \"55355\"\n".to_vec();
        for (which, seed) in [("devices", devices_seed.clone()), ("autoconfig", H700.as_bytes().to_vec()), ("cfg", cfg_seed.clone())] {
            for (label, bytes) in crate::file_faults::faults(&seed) {
                let dir = base.join(format!("{which}-{label}"));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(dir.join("joypads")).unwrap();
                let (ra, sys, devices, pad) = (dir.join("retroarch.cfg"), dir.join("system.cfg"), dir.join("devices"), dir.join("joypads/H700 Gamepad.cfg"));
                std::fs::write(&sys, "system.autohotkeys=1\n").unwrap();
                let file = match which { "devices" => &devices, "autoconfig" => &pad, _ => &ra };
                for (f, good) in [(&devices, &devices_seed), (&pad, &H700.as_bytes().to_vec()), (&ra, &cfg_seed)] {
                    if f != file {
                        std::fs::write(f, good).unwrap();
                    }
                }
                if let Some(b) = &bytes {
                    std::fs::write(file, b).unwrap();
                }
                let paths = Paths { retroarch_cfg: &ra, joypads: &dir.join("joypads"), input_devices: &devices, system_cfg: &sys };
                let wrote = prepare(&paths, Enabled::default()).unwrap_or(false);
                let after = std::fs::read(&ra).unwrap_or_default();
                if wrote {
                    let text = String::from_utf8_lossy(&after);
                    // Every hotkey value is a button number, an axis or nul: nothing else came through.
                    for key in ["input_enable_hotkey_btn", "input_save_state_btn", "input_rewind_btn"] {
                        let v = value(&text, key).unwrap_or_else(|| panic!("{which} {label}: no {key}"));
                        assert!(v == "nul" || v.trim_start_matches(['+', '-']).chars().all(|c| c.is_ascii_digit()) || v.starts_with('h'), "{which} {label}: {key} = {v:?}");
                    }
                    if which != "cfg" {
                        assert!(text.starts_with("video_driver = \"gl\"\n"), "{which} {label}: the player's lines stay: {text}");
                    }
                } else {
                    assert_eq!(std::fs::read_to_string(&sys).unwrap(), "system.autohotkeys=1\n", "{which} {label}: nothing written, ROCKNIX keeps its own");
                    if which != "cfg" {
                        assert_eq!(after, cfg_seed, "{which} {label}");
                    }
                }
            }
        }
    }

    #[test]
    fn pause_menu_commands_are_enabled_even_without_a_pad() {
        let dir = crate::testutil::TestDir::new("ra-pause-commands");
        let (ra, sys, devices, joypads) = (dir.join("retroarch.cfg"), dir.join("system.cfg"), dir.join("devices"), dir.join("joypads"));
        let paths = Paths { retroarch_cfg: &ra, joypads: &joypads, input_devices: &devices, system_cfg: &sys };
        let other = "# Player settings\nvideo_driver = \"gl\"\ninput_save_state_btn = \"5\"\n";
        for network in ["network_cmd_enable = \"false\"\nnetwork_cmd_port = \"12345\"\n", ""] {
            std::fs::write(&ra, format!("{other}{network}")).unwrap();
            let expected = format!("{other}network_cmd_enable = \"true\"\nnetwork_cmd_port = \"55355\"\n");
            for _ in 0..2 {
                assert!(!prepare(&paths, on()).unwrap(), "no pad hotkeys to configure");
                assert_eq!(std::fs::read_to_string(&ra).unwrap(), expected);
                assert!(!sys.exists(), "ROCKNIX still manages the hotkeys");
            }
        }
    }

    /// With a pad autoconfig, pocket-ui writes the hotkeys and switches off ROCKNIX's hotkey generation.
    #[test]
    fn prepare_writes_the_lines_and_hands_autohotkeys_over() {
        let dir = crate::testutil::TestDir::new("ra-hotkeys");
        std::fs::create_dir_all(dir.join("joypads")).unwrap();
        let (ra, sys, devices) = (dir.join("retroarch.cfg"), dir.join("system.cfg"), dir.join("devices"));
        std::fs::write(&ra, "video_driver = \"gl\"\ninput_save_state_btn = \"5\"\n").unwrap();
        std::fs::write(&sys, "system.autohotkeys=1\nglobal.autosave=1\n").unwrap();
        std::fs::write(&devices, "N: Name=\"H700 Gamepad\"\nH: Handlers=event1 js0\n").unwrap();
        let paths = Paths { retroarch_cfg: &ra, joypads: &dir.join("joypads"), input_devices: &devices, system_cfg: &sys };
        assert!(!prepare(&paths, on()).unwrap(), "no autoconfig: ROCKNIX keeps its own");
        assert_eq!(std::fs::read_to_string(&sys).unwrap(), "system.autohotkeys=1\nglobal.autosave=1\n");
        std::fs::write(dir.join("joypads/H700 Gamepad.cfg"), H700).unwrap();
        assert!(prepare(&paths, Enabled { save: false, ..on() }).unwrap());
        let cfg = std::fs::read_to_string(&ra).unwrap();
        assert!(cfg.starts_with("video_driver = \"gl\"\ninput_save_state_btn = \"100\"\n"), "{cfg}");
        assert_eq!(value(&cfg, "input_enable_hotkey_btn"), Some("10"));
        assert_eq!(value(&cfg, "input_load_state_btn"), Some("4"));
        assert_eq!(value(&cfg, "network_cmd_enable"), Some("true"));
        assert_eq!(value(&cfg, "network_cmd_port"), Some("55355"));
        let sys_now = std::fs::read_to_string(&sys).unwrap();
        assert!(sys_now.contains("system.autohotkeys=0") && sys_now.contains("global.autosave=1"), "{sys_now}");
    }
}
