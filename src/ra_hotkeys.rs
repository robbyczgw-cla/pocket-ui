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
use std::path::{Path, PathBuf};

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

// ---- The dArkOS port ------------------------------------------------------------------------------------------------
//
// The port starts dArkOS's own RetroArch through dArkOS's command (`deploy/darkos/host-launch.py`), with dArkOS's
// `~ark/.config/retroarch/retroarch.cfg`, which stays the player's: pocket-ui never writes it. Its shortcuts go into a
// file of the session instead (`DARKOS_SESSION` in the port's run folder, a tmpfs), which host-launch.py hands to
// RetroArch as `--appendconfig` when dArkOS's command starts `/usr/local/bin/retroarch` (or `retroarch32`) itself.
// - **The hotkey button** is pocket-ui's HOME on that pad, as RetroArch numbers it: the F key (BTN_MODE) of the RK3566
//   `retrogame_joypad`; on the RK3326 `GO-Super Gamepad` the R36S family's FN (708) where the pad has it, else R3 (707),
//   both HOME in pocket-ui (`darkos::pad_button`). dArkOS's own hotkey (Select on the RG353M) is not one then.
// - **HOME + A** is RetroArch's screenshot, into pocket-ui's Screenshots folder: pocket-ui's own (`app_screenshot`)
//   cannot read the screen while dArkOS's RetroArch has it. Rewind is enabled while HOME + L2 is on (dArkOS ships it off).
// - Without the pad's autoconfig or a HOME key there is no session file: dArkOS's own hotkeys stay.

/// The session file's name in the port's run folder (`DARKOS_STATE`, /tmp/pocket-ui-darkos, the same path inside and
/// outside pocket-ui's view). host-launch.py looks for it under this name.
pub const DARKOS_SESSION: &str = "retroarch-shortcuts.cfg";
/// dArkOS's RetroArch autoconfigs (in ark's home, `joypad_autoconfig_dir`).
pub const DARKOS_AUTOCONFIG: &str = ".config/retroarch/autoconfig/udev";

/// The evdev codes of pocket-ui's HOME on a dArkOS pad, the preferred one first (`darkos::pad_button`, `pad::button`).
pub fn darkos_home_codes(pad_name: &str) -> &'static [u16] {
    if pad_name.trim() == "GO-Super Gamepad" { &[708, 707] } else { &[0x13c] }
}

/// The `/proc/bus/input/devices` block of the pad called `name`.
fn device_block<'a>(devices: &'a str, name: &str) -> Option<&'a str> {
    devices.split("\n\n").find(|b| b.lines().any(|l| l.strip_prefix("N: Name=").is_some_and(|n| n.trim().trim_matches('"') == name)))
}

/// RetroArch's button number for key `code` on the pad of this `/proc/bus/input/devices` block: udev_joypad.c numbers the
/// pad's keys from BTN_JOYSTICK (0x120) upwards in code order. None when the pad has no such key. The words of the
/// `B: KEY=` bitmap are 64 bits (dArkOS's kernels are aarch64), the last word holding codes 0 to 63.
pub fn udev_button(block: &str, code: u16) -> Option<String> {
    const BTN_JOYSTICK: usize = 0x120;
    let words: Vec<u64> = block.lines().find_map(|l| l.strip_prefix("B: KEY="))?.split_whitespace().map(|w| u64::from_str_radix(w, 16).ok()).collect::<Option<_>>()?;
    let has = |c: usize| words.len() > c / 64 && words[words.len() - 1 - c / 64] >> (c % 64) & 1 == 1;
    let code = code as usize;
    (code >= BTN_JOYSTICK && has(code)).then(|| (BTN_JOYSTICK..code).filter(|&c| has(c)).count().to_string())
}

/// The session's lines for dArkOS's RetroArch: the ROCKNIX set (`lines`) on pocket-ui's HOME, HOME + A RetroArch's
/// screenshot into `screenshots`, rewind enabled while its shortcut is on.
pub fn darkos_lines(pad: &Pad, on: Enabled, screenshots: &Path) -> Vec<(String, String)> {
    let mut out = lines(pad, on);
    out.retain(|(k, _)| k != "input_screenshot_btn");
    out.push(("input_screenshot_btn".into(), btn(on.screenshot, &pad.a)));
    for (k, v) in [
        ("rewind_enable", if on.rewind { "true" } else { "false" }.to_string()),
        ("screenshot_directory", screenshots.display().to_string()),
        // dArkOS puts them beside the game, which pocket-ui shows read only and whose folder is not the gallery's.
        ("screenshots_in_content_dir", "false".into()),
        ("sort_screenshots_by_content_enable", "false".into()),
    ] {
        out.push((k.into(), v));
    }
    out
}

/// The files `prepare_darkos` reads and writes, so that tests can point it elsewhere.
pub struct DarkosPaths {
    pub input_devices: PathBuf,
    /// dArkOS's RetroArch autoconfigs.
    pub autoconfig: PathBuf,
    /// The session file (in the port's run folder).
    pub session: PathBuf,
    /// pocket-ui's Screenshots folder as dArkOS's RetroArch sees it (outside pocket-ui's view).
    pub screenshots: PathBuf,
}

impl DarkosPaths {
    /// From start.sh's environment: `DARKOS_STATE`, `DARKOS_ARK_HOME` and `POCKETUI_BRICK_CARD` (the port's folder, whose
    /// `storage` is pocket-ui's /storage). None outside the port.
    pub fn from_env() -> Option<DarkosPaths> {
        let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let card = var("POCKETUI_BRICK_CARD")?;
        let in_view = crate::screenshot::dir();
        let screenshots = in_view.strip_prefix("/storage").map_or(in_view.clone(), |rest| card.join("storage").join(rest));
        Some(DarkosPaths {
            input_devices: PathBuf::from(INPUT_DEVICES),
            autoconfig: var("DARKOS_ARK_HOME")?.join(DARKOS_AUTOCONFIG),
            session: var("DARKOS_STATE")?.join(DARKOS_SESSION),
            screenshots,
        })
    }
}

/// Before a game in dArkOS's command: the session file for this game's shortcuts, or none (removed) when the pad has no
/// autoconfig or no HOME key. dArkOS's `retroarch.cfg` is not read or written. Returns whether there is one.
pub fn prepare_darkos(paths: &DarkosPaths, on: Enabled) -> std::io::Result<bool> {
    let devices = std::fs::read_to_string(&paths.input_devices).unwrap_or_default();
    let pad = controller_name(&devices).and_then(|name| {
        let autoconfig = std::fs::read_to_string(paths.autoconfig.join(format!("{name}.cfg"))).ok()?;
        let block = device_block(&devices, &name)?;
        let hotkey = darkos_home_codes(&name).iter().find_map(|c| udev_button(block, *c))?;
        Some(Pad { hotkey, ..Pad::from_autoconfig(&autoconfig) })
    });
    let Some(pad) = pad else {
        match std::fs::remove_file(&paths.session) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => return Ok(false),
        }
    };
    let text = with_lines("# pocket-ui: this game's HOME shortcuts, for this session only (ra_hotkeys::prepare_darkos)\n", &darkos_lines(&pad, on, &paths.screenshots));
    crate::fsutil::write_atomic(&paths.session, text.as_bytes())?;
    // RetroArch does not make the folder it saves into (on the card: every user may write there).
    let _ = std::fs::create_dir_all(&paths.screenshots);
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

    /// dArkOS's `retrogame_joypad.cfg` (RG353M image v08272026), the lines that matter here: no hotkey of its own (dArkOS
    /// sets Select in its `retroarch.cfg`).
    const DARKOS_RK3566: &str = "# ArkOS\ninput_device = \"retrogame_joypad\"\ninput_up_btn = \"13\"\ninput_a_btn = \"1\"\ninput_b_btn = \"0\"\ninput_x_btn = \"2\"\ninput_y_btn = \"3\"\ninput_l_btn = \"4\"\ninput_r_btn = \"5\"\ninput_l2_btn = \"6\"\ninput_r2_btn = \"7\"\ninput_l3_btn = \"11\"\ninput_r3_btn = \"12\"\ninput_select_btn = \"8\"\ninput_start_btn = \"9\"\n";

    /// A `/proc/bus/input/devices` block for a pad with these key codes (64-bit words, as on dArkOS's aarch64 kernels).
    fn block(name: &str, handlers: &str, codes: &[u16]) -> String {
        let mut words = vec![0u64; 12];
        for c in codes {
            words[*c as usize / 64] |= 1 << (c % 64);
        }
        while words.len() > 1 && words.last() == Some(&0) {
            words.pop();
        }
        let key: Vec<String> = words.iter().rev().map(|w| format!("{w:x}")).collect();
        format!("I: Bus=0019 Vendor=484b Product=1101 Version=0100\nN: Name=\"{name}\"\nP: Phys=\nH: Handlers={handlers}\nB: EV=b\nB: KEY={}\nB: ABS=3f\n", key.join(" "))
    }

    const RK3566_KEYS: &[u16] = &[304, 305, 307, 308, 310, 311, 312, 313, 314, 315, 316, 317, 318, 544, 545, 546, 547];
    const RK3326_KEYS: &[u16] = &[114, 115, 304, 305, 307, 308, 310, 311, 312, 313, 544, 545, 546, 547, 704, 705, 706, 707];

    /// RetroArch's numbers from the pad's key bitmap agree with dArkOS's own autoconfigs: RK3566 Select 8, Start 9, R3
    /// 12, up 13, so the F key (BTN_MODE) is 10; RK3326 Select 12 and R3 15 (`es_input.cfg.rg351mp`), the R36S's FN 16
    /// (dArkOSRE-R36's `input_menu_toggle_btn`). The pad's volume keys (below BTN_JOYSTICK) do not count.
    #[test]
    fn udev_numbers_match_darkos_autoconfigs() {
        let rk3566 = block("retrogame_joypad", "event2 js0", RK3566_KEYS);
        let n = |b: &str, c| udev_button(b, c);
        assert_eq!((n(&rk3566, 314), n(&rk3566, 315), n(&rk3566, 316), n(&rk3566, 318), n(&rk3566, 544)), (Some("8".into()), Some("9".into()), Some("10".into()), Some("12".into()), Some("13".into())));
        let rk3326 = block("GO-Super Gamepad", "event3 js0", RK3326_KEYS);
        assert_eq!((n(&rk3326, 704), n(&rk3326, 707), n(&rk3326, 708)), (Some("12".into()), Some("15".into()), None));
        let r36s = block("GO-Super Gamepad", "event3 js0", &[RK3326_KEYS, &[708]].concat());
        assert_eq!(n(&r36s, 708), Some("16".into()));
        assert_eq!(n(&rk3326, 114), None, "a key below the pad buttons is no RetroArch button number of ours");
        assert_eq!(udev_button("N: Name=\"x\"\nB: KEY=zz\n", 316), None);
    }

    fn darkos_setup(dir: &crate::testutil::TestDir, devices: &str, autoconfig: Option<(&str, &str)>) -> DarkosPaths {
        let paths = DarkosPaths { input_devices: dir.join("devices"), autoconfig: dir.join("udev"), session: dir.join("run").join(DARKOS_SESSION), screenshots: dir.join("card/storage/roms/screenshots") };
        std::fs::create_dir_all(&paths.autoconfig).unwrap();
        std::fs::create_dir_all(dir.join("run")).unwrap();
        std::fs::write(&paths.input_devices, devices).unwrap();
        if let Some((name, text)) = autoconfig {
            std::fs::write(paths.autoconfig.join(format!("{name}.cfg")), text).unwrap();
        }
        paths
    }

    /// The dArkOS session file holds pocket-ui's combos on pocket-ui's HOME: HOME + R1 save, L1 load, R2 fast-forward,
    /// L2 rewind (enabled), A RetroArch's screenshot into the Screenshots folder, Y the FPS. dArkOS's own retroarch.cfg
    /// is not part of it and is never touched.
    #[test]
    fn darkos_session_has_pocket_uis_combos_on_home() {
        let dir = crate::testutil::TestDir::new("ra-hotkeys-darkos");
        let devices = format!("N: Name=\"rk805 pwrkey\"\nH: Handlers=kbd event0\nB: KEY=100000 0 0 0\n\n{}", block("retrogame_joypad", "event2 js0", RK3566_KEYS));
        let paths = darkos_setup(&dir, &devices, Some(("retrogame_joypad", DARKOS_RK3566)));
        let darkos_cfg = dir.join("ark-retroarch.cfg");
        let darkos_text = "input_enable_hotkey_btn = \"8\"\ninput_save_state_btn = \"5\"\nrewind_enable = \"false\"\nscreenshots_in_content_dir = \"true\"\n";
        std::fs::write(&darkos_cfg, darkos_text).unwrap();
        assert!(prepare_darkos(&paths, on()).unwrap());
        let cfg = std::fs::read_to_string(&paths.session).unwrap();
        let v = |k| value(&cfg, k).unwrap_or_else(|| panic!("no {k}: {cfg}")).to_string();
        assert_eq!(v("input_enable_hotkey_btn"), "10", "HOME (the F key, BTN_MODE), not dArkOS's Select");
        assert_eq!(v("input_save_state_btn"), "5", "HOME + R1 saves");
        assert_eq!(v("input_load_state_btn"), "4", "HOME + L1 loads");
        assert_eq!(v("input_toggle_fast_forward_btn"), "7", "HOME + R2 fast-forward");
        assert_eq!(v("input_rewind_btn"), "6", "hold HOME + L2 rewinds");
        assert_eq!(v("rewind_enable"), "true");
        assert_eq!(v("input_screenshot_btn"), "1", "HOME + A screenshot");
        assert_eq!(v("screenshot_directory"), dir.join("card/storage/roms/screenshots").display().to_string());
        assert_eq!(v("screenshots_in_content_dir"), "false");
        assert!(dir.join("card/storage/roms/screenshots").is_dir(), "RetroArch finds the folder");
        assert_eq!(v("input_fps_toggle_btn"), "3", "HOME + Y");
        assert_eq!(v("input_pause_toggle_btn"), NEVER_BTN, "nothing else on A");
        assert_eq!(std::fs::read_to_string(&darkos_cfg).unwrap(), darkos_text, "dArkOS's own file stays as it was");
        // The R36S: FN is HOME where the pad has it, else R3.
        for (keys, home) in [(RK3326_KEYS.to_vec(), "15"), ([RK3326_KEYS, &[708]].concat(), "16")] {
            let paths = darkos_setup(&dir, &block("GO-Super Gamepad", "event3 js0", &keys), Some(("GO-Super Gamepad", "input_r_btn = \"5\"\ninput_select_btn = \"12\"\n")));
            assert!(prepare_darkos(&paths, on()).unwrap());
            assert_eq!(value(&std::fs::read_to_string(&paths.session).unwrap(), "input_enable_hotkey_btn"), Some(home));
        }
    }

    /// Switched off in Options → Shortcuts (or Kids Mode): off in dArkOS's RetroArch too, never "nul", and no rewind.
    #[test]
    fn darkos_switched_off_combos_stay_off() {
        let dir = crate::testutil::TestDir::new("ra-hotkeys-darkos-off");
        let paths = darkos_setup(&dir, &block("retrogame_joypad", "event2 js0", RK3566_KEYS), Some(("retrogame_joypad", DARKOS_RK3566)));
        for off in [Enabled::none(), Enabled { save: false, load: false, fast_forward: false, rewind: false, over_game: false, screenshot: false, ..on() }] {
            assert!(prepare_darkos(&paths, off).unwrap());
            let cfg = std::fs::read_to_string(&paths.session).unwrap();
            for key in ["input_save_state", "input_load_state", "input_toggle_fast_forward", "input_rewind", "input_fps_toggle", "input_screenshot"] {
                assert_eq!(value(&cfg, &format!("{key}_btn")), Some(NEVER_BTN), "{key}: {cfg}");
            }
            assert_eq!(value(&cfg, "rewind_enable"), Some("false"));
            assert_eq!(value(&cfg, "input_enable_hotkey_btn"), Some("10"));
        }
        let cfg = { prepare_darkos(&paths, Enabled { load: false, ..on() }).unwrap(); std::fs::read_to_string(&paths.session).unwrap() };
        assert_eq!((value(&cfg, "input_load_state_btn"), value(&cfg, "input_save_state_btn")), (Some(NEVER_BTN), Some("5")), "one off, the rest on");
    }

    /// No autoconfig, no pad, no HOME key or garbage: no session file (an old one goes), dArkOS's hotkeys stay.
    #[test]
    fn darkos_without_a_known_pad_leaves_dark_os_hotkeys_alone() {
        let dir = crate::testutil::TestDir::new("ra-hotkeys-darkos-none");
        let no_home: Vec<u16> = RK3566_KEYS.iter().copied().filter(|c| *c != 316).collect();
        for (devices, autoconfig) in [
            (block("retrogame_joypad", "event2 js0", RK3566_KEYS), None),
            (String::new(), Some(("retrogame_joypad", DARKOS_RK3566))),
            (block("retrogame_joypad", "event2 js0", &no_home), Some(("retrogame_joypad", DARKOS_RK3566))),
            ("N: Name=\"retrogame_joypad\"\nH: Handlers=js0\nB: KEY=not hex\n".to_string(), Some(("retrogame_joypad", DARKOS_RK3566))),
        ] {
            let paths = darkos_setup(&dir, &devices, autoconfig);
            std::fs::write(&paths.session, "stale").unwrap();
            assert!(!prepare_darkos(&paths, on()).unwrap());
            assert!(!paths.session.exists());
            std::fs::remove_dir_all(&paths.autoconfig).unwrap();
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
