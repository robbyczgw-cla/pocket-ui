//! Saved controls of a standalone emulator that name another controller than the one the game gets: before such a
//! start pocket-ui sets the file aside (kept, never deleted), ROCKNIX's start script writes a fresh one for this
//! controller, and the shelf says so. Alex's Mini V2, 25.11.2026: YabaSanshiro had no Start, no buttons at all.
//!
//! Why it happens: ROCKNIX writes these files only when they are missing, and names one controller in them. On the
//! InputPlumber platforms (SM8250, SM8550, SM4450, SM6115, RK3576 and newer) the game sees InputPlumber's virtual
//! "Sony Interactive Entertainment DualSense Wireless Controller"; the real pad ("Retroid Pocket Gamepad") is held by
//! InputPlumber, its node moved to `/dev/inputplumber/sources`. A file made for the real pad (or on another device,
//! or with a Bluetooth pad as the first one) stays dead for good.
//!
//! What counts as stale (the player's own changes for this controller stay):
//! - **YabaSanshiro** (`/storage/.config/yabasanshiro`): `input.cfg` (ROCKNIX's copy of EmulationStation's entry,
//!   rewritten with `keymapv2.json` only when `input.cfg` is missing) has no entry for the controller ROCKNIX picks,
//!   or `keymapv2.json`'s player 1 is another controller, or its player 1 has nothing bound (every id -1: what
//!   YabaSanshiro writes for a controller `input.cfg` doesn't know). YabaSanshiro finds player 1 by
//!   "<id>_<name>_<GUID>" only; with no match it has no pad at all.
//! - **Azahar** (`/storage/.config/azahar/qt-config.ini`, copied from ROCKNIX's only when missing): the active
//!   profile binds its buttons to SDL GUIDs, and none of them is a controller the game can see. Keyboard-only
//!   profiles stay.
//!
//! Only when a fresh file would work: ROCKNIX's choice is a controller the game can see (its event node is in
//! `/dev/input`), and for YabaSanshiro EmulationStation has controls for it; for Azahar ROCKNIX's own copy binds a
//! controller that is here. Should ROCKNIX write the same stale file again, the second time it stays (the newest
//! set-aside copy is the same), so the shelf never says it at every start.
//!
//! The others need nothing (ROCKNIX scripts of the 20261001 and 20261101 pins, docs/STANDALONE-CONTROLS.md):
//! Dolphin's `Device =` lines and Mupen64Plus' `InputAutoCfg.ini` are written at every start for the first SDL
//! controller; ARMSX2, DuckStation, melonDS, PPSSPP, DraStic and Flycast bind by SDL index, not by name or GUID;
//! Flycast keeps one mapping file per controller name. Mednafen's `mednafen.cfg` is the same kind of file (written
//! once, bound to a hash of the first controller's name) and is not handled yet.

use std::path::{Path, PathBuf};

/// InputPlumber's virtual pad, which ROCKNIX's start_yabasanshiro.sh names on the InputPlumber platforms.
pub const DUALSENSE: &str = "Sony Interactive Entertainment DualSense Wireless Controller";
/// The platforms start_yabasanshiro.sh of the 20261001 pin treats as InputPlumber ones (20261101 asks whether
/// `/usr/share/inputplumber` is there).
const INPUTPLUMBER_HW: &[&str] = &["RK3576", "SM6115", "SM4450", "SM8550", "SM8650", "SM8750", "SM8250"];
/// The folder in the emulator's config folder that keeps the set-aside files: `pocket-ui-controls-<UTC time>`.
pub const BACKUP_PREFIX: &str = "pocket-ui-controls-";

const YABA_DIR: &str = "storage/.config/yabasanshiro";
const YABA_FILES: &[&str] = &["input.cfg", "keymapv2.json"];
const ES_INPUT: &str = "storage/.emulationstation/es_input.cfg";
const AZAHAR_DIR: &str = "storage/.config/azahar";
const AZAHAR_FILES: &[&str] = &["qt-config.ini"];
const AZAHAR_STOCK: &str = "usr/config/azahar/qt-config.ini";

/// An input device from `/proc/bus/input/devices`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pad {
    pub name: String,
    pub bus: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
    /// `jsN`: the kernel took it for a joystick.
    pub js: Option<u32>,
    /// `eventN`.
    pub event: Option<u32>,
}

impl Pad {
    /// SDL 2's GUID string for this device on Linux: bus, CRC-16 of the name, vendor, product, version, each
    /// little-endian and followed by two zero bytes (but the CRC). ROCKNIX's es_input.cfg and Azahar's
    /// qt-config.ini carry these ("0300fd574c050000e60c000011810000" for InputPlumber's DualSense).
    pub fn guid(&self) -> String {
        let le = |v: u16| format!("{:02x}{:02x}", v & 0xff, v >> 8);
        format!("{}{}{}0000{}0000{}0000", le(self.bus), le(crc16(self.name.as_bytes())), le(self.vendor), le(self.product), le(self.version))
    }

    /// Whether a saved name (and GUID, when it is one SDL makes from the device's ids) is this device. The version
    /// and the CRC are not compared: a name or GUID that differs only there names the same controller.
    fn is(&self, name: &str, guid: Option<&str>) -> bool {
        name.trim() == self.name && guid.and_then(Ids::parse).is_none_or(|g| g.vendor == self.vendor && g.product == self.product)
    }
}

/// SDL's `SDL_crc16` (CRC-16/ARC), which SDL 2.26 and later put into the GUID.
fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |mut crc, &b| {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
        crc
    })
}

/// The ids in an SDL GUID made from a device's ids. Others (from a name only, the keyboard's "-1") are None.
#[derive(Debug, PartialEq)]
struct Ids {
    vendor: u16,
    product: u16,
}

impl Ids {
    fn parse(guid: &str) -> Option<Ids> {
        let g = guid.trim();
        if g.len() != 32 || !g.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&g[2 * i..2 * i + 2], 16).ok();
        let word = |i: usize| Some(u16::from(byte(i)?) | u16::from(byte(i + 1)?) << 8);
        // Bytes 6-7 and 10-11 are zero in GUIDs made from ids; the vendor is never 0 there.
        if word(6)? != 0 || word(10)? != 0 || word(4)? == 0 {
            return None;
        }
        Some(Ids { vendor: word(4)?, product: word(8)? })
    }
}

/// The devices in `/proc/bus/input/devices`, in the kernel's order.
pub fn parse_devices(text: &str) -> Vec<Pad> {
    text.split("\n\n").filter_map(|block| {
        let mut pad = Pad { name: String::new(), bus: 0, vendor: 0, product: 0, version: 0, js: None, event: None };
        let mut named = false;
        for line in block.lines() {
            if let Some(ids) = line.strip_prefix("I: ") {
                for field in ids.split_whitespace() {
                    let Some((k, v)) = field.split_once('=') else { continue };
                    let v = u16::from_str_radix(v, 16).unwrap_or(0);
                    match k {
                        "Bus" => pad.bus = v,
                        "Vendor" => pad.vendor = v,
                        "Product" => pad.product = v,
                        "Version" => pad.version = v,
                        _ => {}
                    }
                }
            } else if let Some(name) = line.strip_prefix("N: Name=") {
                pad.name = name.trim().trim_matches('"').to_string();
                named = true;
            } else if let Some(handlers) = line.strip_prefix("H: Handlers=") {
                for h in handlers.split_whitespace() {
                    if let Some(n) = h.strip_prefix("js").and_then(|n| n.parse().ok()) {
                        pad.js = Some(n);
                    } else if let Some(n) = h.strip_prefix("event").and_then(|n| n.parse().ok()) {
                        pad.event = Some(n);
                    }
                }
            }
        }
        named.then_some(pad)
    }).collect()
}

/// Where the files are: `/` on the device, a test folder in tests.
pub struct Here {
    root: PathBuf,
}

impl Here {
    pub fn device() -> Here {
        Here { root: PathBuf::from("/") }
    }

    pub fn at(root: &Path) -> Here {
        Here { root: root.to_path_buf() }
    }

    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p.trim_start_matches('/'))
    }

    fn devices(&self) -> Vec<Pad> {
        parse_devices(&std::fs::read_to_string(self.path("proc/bus/input/devices")).unwrap_or_default())
    }

    /// A device a game's SDL can open: its node is in `/dev/input` (InputPlumber moves the ones it holds away).
    fn visible(&self, p: &Pad) -> bool {
        p.event.is_some_and(|e| self.path(&format!("dev/input/event{e}")).exists())
    }

    /// The joysticks a game can see, in the kernel's order.
    fn pads(&self) -> Vec<Pad> {
        self.devices().into_iter().filter(|p| p.js.is_some() && self.visible(p)).collect()
    }

    fn inputplumber(&self) -> bool {
        self.path("usr/share/inputplumber").is_dir()
            || std::fs::read_to_string(self.path("etc/os-release")).is_ok_and(|s| {
                s.lines().filter_map(|l| l.strip_prefix("HW_DEVICE=")).any(|v| INPUTPLUMBER_HW.contains(&v.trim().trim_matches('"')))
            })
    }

    /// The controller start_yabasanshiro.sh writes its files for: InputPlumber's DualSense on those platforms, else
    /// js0 (else a "joypad"); None when the game couldn't see it (a fresh file wouldn't help).
    fn rocknix_pad(&self) -> Option<Pad> {
        let devices = self.devices();
        let name = if self.inputplumber() {
            DUALSENSE.to_string()
        } else {
            devices.iter().find(|p| p.js == Some(0)).or_else(|| devices.iter().find(|p| p.name.contains("joypad")))?.name.clone()
        };
        devices.into_iter().find(|p| p.name == name && self.visible(p))
    }
}

/// What the check found.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Keep,
    /// Set the files aside; why, for the log.
    Reset(String),
}

/// The `inputConfig` entries of an EmulationStation input file: (deviceName, deviceGUID). None when it doesn't parse.
fn input_configs(xml: &str) -> Option<Vec<(String, String)>> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    Some(doc.descendants().filter(|n| n.has_tag_name("inputConfig"))
        .map(|n| (n.attribute("deviceName").unwrap_or_default().to_string(), n.attribute("deviceGUID").unwrap_or_default().to_string()))
        .collect())
}

/// YabaSanshiro's files (`input.cfg`, and `keymapv2.json` when there) for `pad`, ROCKNIX's choice. `es_input` is
/// EmulationStation's `es_input.cfg`, which ROCKNIX copies the controller's entry from.
pub fn yabasanshiro(input_cfg: Option<&str>, keymap: Option<&str>, es_input: &str, pad: &Pad) -> Verdict {
    // Without input.cfg ROCKNIX writes both itself.
    let Some(input_cfg) = input_cfg else { return Verdict::Keep };
    let es_knows = input_configs(es_input).is_some_and(|c| c.iter().any(|(name, guid)| pad.is(name, Some(guid))));
    if !es_knows {
        println!("controls: EmulationStation has no controls for {}, YabaSanshiro's stay", pad.name);
        return Verdict::Keep;
    }
    match input_configs(input_cfg) {
        None => return Verdict::Reset("input.cfg doesn't read".into()),
        Some(c) if !c.iter().any(|(name, guid)| pad.is(name, Some(guid)) || guid == &pad.guid()) => {
            let names: Vec<&str> = c.iter().map(|(n, _)| n.as_str()).collect();
            return Verdict::Reset(format!("input.cfg is for {names:?}, the game gets {}", pad.name));
        }
        Some(_) => {}
    }
    let Some(keymap) = keymap else { return Verdict::Keep };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(keymap) else { return Verdict::Reset("keymapv2.json doesn't read".into()) };
    let p1 = &json["player1"];
    let (name, guid) = (p1["deviceName"].as_str().unwrap_or_default(), p1["deviceGUID"].as_str().unwrap_or_default());
    if !pad.is(name, Some(guid)) {
        return Verdict::Reset(format!("keymapv2.json's player 1 is {name:?}, the game gets {}", pad.name));
    }
    // YabaSanshiro's key for the player's bindings: "<DeviceID>_<deviceName>_<deviceGUID>" (`genJoyString`).
    let key = format!("{}_{name}_{guid}", p1["DeviceID"].as_i64().unwrap_or(0));
    let bound = json[key.as_str()].as_object().is_some_and(|b| {
        // The four analog entries are always written with fixed axes; the buttons tell.
        b.iter().any(|(k, v)| !k.starts_with("analog") && v["id"].as_i64().is_some_and(|id| id >= 0))
    });
    if !bound {
        return Verdict::Reset(format!("keymapv2.json has nothing bound for {name}"));
    }
    Verdict::Keep
}

/// The SDL GUIDs the active controls profile of an Azahar qt-config.ini binds buttons and sticks to.
fn azahar_guids(ini: &str) -> Vec<String> {
    let mut section = "";
    let mut profile = 0usize;
    let mut lines = Vec::new();
    for line in ini.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line;
        } else if section == "[Controls]" {
            if let Some(v) = line.strip_prefix("profile=") {
                profile = v.trim().parse().unwrap_or(0);
            }
            lines.push(line);
        }
    }
    // Qt arrays count from 1: profile 0 is `profiles\1\…`.
    let prefix = format!("profiles\\{}\\", profile + 1);
    let mut guids: Vec<String> = lines.iter()
        .filter_map(|l| l.strip_prefix(prefix.as_str()))
        .filter(|l| l.contains("engine:sdl"))
        .filter_map(|l| l.split("guid:").nth(1))
        .map(|g| g.split([',', '"']).next().unwrap_or_default().to_string())
        .collect();
    guids.sort();
    guids.dedup();
    guids
}

/// Azahar's qt-config.ini against the controllers the game can see; `stock` is ROCKNIX's copy (what a fresh one is).
pub fn azahar(ini: Option<&str>, stock: &str, pads: &[Pad]) -> Verdict {
    let Some(ini) = ini else { return Verdict::Keep };
    let here = |g: &String| Ids::parse(g).is_some_and(|ids| pads.iter().any(|p| p.vendor == ids.vendor && p.product == ids.product));
    let saved = azahar_guids(ini);
    if saved.is_empty() || saved.iter().any(here) {
        return Verdict::Keep;
    }
    if !azahar_guids(stock).iter().any(here) {
        println!("controls: ROCKNIX's Azahar controls are for none of the controllers here either, Azahar's stay");
        return Verdict::Keep;
    }
    Verdict::Reset(format!("qt-config.ini binds {saved:?}, none of them is here"))
}

/// Moves `files` (those that are there) of `dir` into a new `pocket-ui-controls-<time>` folder beside them. Not when
/// the newest such folder holds the same files: ROCKNIX wrote the same again, and another round wouldn't help.
fn set_aside(dir: &Path, files: &[&str], unix_secs: u64) -> std::io::Result<Option<PathBuf>> {
    let present: Vec<&str> = files.iter().copied().filter(|f| dir.join(f).is_file()).collect();
    let newest = std::fs::read_dir(dir)?.flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(BACKUP_PREFIX))
        .map(|e| e.path())
        .max();
    if let Some(newest) = newest {
        if present.iter().all(|f| std::fs::read(dir.join(f)).ok() == std::fs::read(newest.join(f)).ok()) {
            println!("controls: {} holds these same files already, they stay", newest.display());
            return Ok(None);
        }
    }
    let stamp: String = crate::crash::timestamp(unix_secs).chars().filter(char::is_ascii_digit).collect();
    let mut to = dir.join(format!("{BACKUP_PREFIX}{}-{}", &stamp[..8], &stamp[8..]));
    let mut n = 1;
    while to.exists() {
        n += 1;
        to = dir.join(format!("{BACKUP_PREFIX}{}-{}-{n}", &stamp[..8], &stamp[8..]));
    }
    std::fs::create_dir(&to)?;
    for f in present {
        std::fs::rename(dir.join(f), to.join(f))?;
    }
    Ok(Some(to))
}

/// The shelf's line after a reset.
pub fn reset_line(system: &str) -> String {
    format!("Controls for {} were reset to match this controller.", crate::system_names::short(system).unwrap_or(system))
}

/// Before a standalone start through ROCKNIX's runemu.sh: stale saved controls of YabaSanshiro or Azahar are set
/// aside so the start script writes fresh ones. Returns the shelf's line when it did. Reads a few small files; on the
/// launch's preparation thread, never the UI thread.
pub fn before_start(system: &str, emulator: &str) -> Option<String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    before_start_in(&Here::device(), system, emulator, now)
}

pub fn before_start_in(here: &Here, system: &str, emulator: &str, unix_secs: u64) -> Option<String> {
    let read = |p: PathBuf| std::fs::read_to_string(p).ok();
    let (dir, files, verdict) = match emulator {
        "yabasanshiro" => {
            let dir = here.path(YABA_DIR);
            let input = read(dir.join("input.cfg"));
            input.as_ref()?;
            let pad = here.rocknix_pad()?;
            let es = read(here.path(ES_INPUT)).unwrap_or_default();
            let verdict = yabasanshiro(input.as_deref(), read(dir.join("keymapv2.json")).as_deref(), &es, &pad);
            (dir, YABA_FILES, verdict)
        }
        "azahar" => {
            let dir = here.path(AZAHAR_DIR);
            let ini = read(dir.join("qt-config.ini"));
            ini.as_ref()?;
            let verdict = azahar(ini.as_deref(), &read(here.path(AZAHAR_STOCK)).unwrap_or_default(), &here.pads());
            (dir, AZAHAR_FILES, verdict)
        }
        _ => return None,
    };
    let Verdict::Reset(why) = verdict else { return None };
    match set_aside(&dir, files, unix_secs) {
        Ok(Some(to)) => {
            println!("controls: {emulator}: {why}; set aside in {}", to.display());
            Some(reset_line(system))
        }
        Ok(None) => None,
        Err(e) => {
            println!("controls: {emulator}: {why}; could not set the files aside: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const DEVICES: &str = include_str!("fixtures/stale_controls/proc-bus-input-devices-retroid-mini-v2.txt");
    const ES_INPUT_CFG: &str = include_str!("fixtures/stale_controls/es_input.cfg");
    const INPUT_RETROID: &str = include_str!("fixtures/stale_controls/yabasanshiro-input-retroid.cfg");
    const INPUT_DUALSENSE: &str = include_str!("fixtures/stale_controls/yabasanshiro-input-dualsense.cfg");
    const KEYMAP_DUALSENSE: &str = include_str!("fixtures/stale_controls/yabasanshiro-keymapv2-dualsense.json");
    const KEYMAP_EMPTY: &str = include_str!("fixtures/stale_controls/yabasanshiro-keymapv2-dualsense-empty.json");
    const AZAHAR_IP: &str = include_str!("fixtures/stale_controls/azahar-qt-config-inputplumber.ini");
    const RETROID: &str = "Retroid Pocket Gamepad";
    const RETROID_GUID: &str = "0300f353202000000130000001000000";
    const DUALSENSE_GUID: &str = "0300fd574c050000e60c000011810000";

    fn pad(name: &str) -> Pad {
        parse_devices(DEVICES).into_iter().find(|p| p.name == name).unwrap()
    }

    /// A Mini V2 under ROCKNIX in a folder: InputPlumber holds the real pad (its node moved away), its DualSense is
    /// in /dev/input.
    fn pocket(d: &TestDir) -> Here {
        let w = |p: &str, s: &str| {
            let p = d.join(p);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, s).unwrap();
        };
        w("proc/bus/input/devices", DEVICES);
        for e in [0, 1, 3, 8, 9, 10] {
            w(&format!("dev/input/event{e}"), "");
        }
        w("dev/inputplumber/sources/event5", "");
        std::fs::create_dir_all(d.join("usr/share/inputplumber")).unwrap();
        w(ES_INPUT, ES_INPUT_CFG);
        w(AZAHAR_STOCK, AZAHAR_IP);
        Here::at(d)
    }

    fn put(d: &TestDir, rel: &str, s: &str) {
        let p = d.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    }

    fn backups(dir: &Path) -> Vec<PathBuf> {
        let mut b: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(BACKUP_PREFIX)).collect();
        b.sort();
        b
    }

    #[test]
    fn sdl_guids_are_made_as_rocknix_files_carry_them() {
        // es_input.cfg and Azahar's InputPlumber qt-config.ini (ROCKNIX 20261101) against the Mini V2's devices.
        assert_eq!(pad(DUALSENSE).guid(), DUALSENSE_GUID);
        assert_eq!(pad(RETROID).guid(), RETROID_GUID);
        assert!(ES_INPUT_CFG.contains(&format!("deviceName=\"{RETROID}\" deviceGUID=\"{RETROID_GUID}\"")));
        assert!(AZAHAR_IP.contains(&format!("guid:{DUALSENSE_GUID}")));
        // YabaSanshiro's own keymap of the RG ARC (ROCKNIX's devices/keymapv2_rg_arc_joypad.json).
        let arc = Pad { name: "rg_arc_joypad".into(), bus: 0x19, vendor: 1, product: 0x0a2c, version: 0x0100, js: Some(0), event: Some(2) };
        assert_eq!(arc.guid(), "1900d632010000002c0a000000010000");
        assert_eq!(Ids::parse("-1"), None, "the keyboard");
        assert_eq!(Ids::parse(DUALSENSE_GUID), Some(Ids { vendor: 0x054c, product: 0x0ce6 }));
    }

    #[test]
    fn the_device_list_keeps_names_ids_and_nodes() {
        let pads = parse_devices(DEVICES);
        let ds = pads.iter().find(|p| p.name == DUALSENSE).unwrap();
        assert_eq!((ds.bus, ds.vendor, ds.product, ds.version, ds.js, ds.event), (3, 0x054c, 0x0ce6, 0x8111, Some(1), Some(9)));
        assert!(pads.iter().any(|p| p.name == format!("{DUALSENSE} Motion Sensors")), "a name that only starts the same is another device");
        assert_eq!(pads.iter().find(|p| p.name == "spmi_haptics").unwrap().js, None);
    }

    #[test]
    fn the_game_sees_inputplumbers_pad_and_not_the_one_it_holds() {
        let d = TestDir::new("stale-controls-visible");
        let here = pocket(&d);
        let names: Vec<String> = here.pads().into_iter().map(|p| p.name).collect();
        assert_eq!(names, [DUALSENSE.to_string(), format!("{DUALSENSE} Motion Sensors")]);
        assert_eq!(here.rocknix_pad().map(|p| p.name).as_deref(), Some(DUALSENSE));
    }

    #[test]
    fn without_inputplumber_rocknix_picks_js0() {
        let d = TestDir::new("stale-controls-js0");
        let here = pocket(&d);
        std::fs::remove_dir(d.join("usr/share/inputplumber")).unwrap();
        put(&d, "dev/input/event5", "");
        assert_eq!(here.rocknix_pad().map(|p| p.name).as_deref(), Some(RETROID));
        // The 20261001 pin decides by HW_DEVICE instead of the folder.
        put(&d, "etc/os-release", "NAME=\"ROCKNIX\"\nHW_DEVICE=\"SM8250\"\n");
        assert_eq!(here.rocknix_pad().map(|p| p.name).as_deref(), Some(DUALSENSE));
    }

    /// Alex's Mini V2, 25.11.2026: input.cfg of 21.11. for the real pad, and the keymap YabaSanshiro made from it for
    /// InputPlumber's DualSense, nothing bound. The old code started the game like that: no button worked.
    #[test]
    fn alexs_saturn_controls_are_set_aside_and_rocknix_writes_new_ones() {
        let d = TestDir::new("stale-controls-alex");
        let here = pocket(&d);
        put(&d, "storage/.config/yabasanshiro/input.cfg", INPUT_RETROID);
        put(&d, "storage/.config/yabasanshiro/keymapv2.json", KEYMAP_EMPTY);
        put(&d, "storage/.config/yabasanshiro/Some Game.cue.config", "kept");
        let line = before_start_in(&here, "saturn", "yabasanshiro", 1_795_643_100);
        assert_eq!(line.as_deref(), Some("Controls for Saturn were reset to match this controller."));
        let dir = d.join(YABA_DIR);
        assert!(!dir.join("input.cfg").exists() && !dir.join("keymapv2.json").exists(), "ROCKNIX writes both again when input.cfg is missing");
        let b = backups(&dir);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].file_name().unwrap().to_string_lossy(), format!("{BACKUP_PREFIX}20261125-214500"));
        assert_eq!(std::fs::read_to_string(b[0].join("input.cfg")).unwrap(), INPUT_RETROID, "kept, not deleted");
        assert_eq!(std::fs::read_to_string(b[0].join("keymapv2.json")).unwrap(), KEYMAP_EMPTY);
        assert!(dir.join("Some Game.cue.config").exists(), "the game's own settings stay");
        // What ROCKNIX and YabaSanshiro write then is for this controller, and stays.
        put(&d, "storage/.config/yabasanshiro/input.cfg", INPUT_DUALSENSE);
        put(&d, "storage/.config/yabasanshiro/keymapv2.json", KEYMAP_DUALSENSE);
        assert_eq!(before_start_in(&here, "saturn", "yabasanshiro", 1_795_643_200), None);
        assert_eq!(backups(&dir).len(), 1);
    }

    #[test]
    fn each_stale_yabasanshiro_case_resets_and_the_players_own_stay() {
        let ds = pad(DUALSENSE);
        let es = ES_INPUT_CFG;
        let reset = |input: Option<&str>, keymap: Option<&str>| matches!(yabasanshiro(input, keymap, es, &ds), Verdict::Reset(_));
        assert!(reset(Some(INPUT_RETROID), None), "input.cfg for the real pad, before YabaSanshiro's first start");
        assert!(reset(Some(INPUT_RETROID), Some(KEYMAP_DUALSENSE)), "a fresh keymap still comes from input.cfg later");
        assert!(reset(Some(INPUT_DUALSENSE), Some(KEYMAP_EMPTY)), "nothing bound");
        let retroid_keymap = KEYMAP_DUALSENSE.replace(DUALSENSE, RETROID).replace(DUALSENSE_GUID, RETROID_GUID);
        assert!(reset(Some(INPUT_DUALSENSE), Some(&retroid_keymap)), "player 1 another controller");
        let keyboard = r#"{"player1": {"DeviceID": -1, "deviceGUID": "-1", "deviceName": "Keyboard", "padmode": 0}, "-1": {}}"#;
        assert!(reset(Some(INPUT_DUALSENSE), Some(keyboard)), "made while no controller was there");
        assert!(reset(Some("<inputList><inputConfig"), None), "input.cfg broken");
        assert!(reset(Some(INPUT_DUALSENSE), Some("{")), "keymapv2.json broken");
        // The player's own choices for this controller: Start moved to another button, Saturn's 3D pad mode.
        let own = KEYMAP_DUALSENSE.replacen("\"id\": 9,", "\"id\": 3,", 1).replace("\"padmode\": 0", "\"padmode\": 1");
        assert_ne!(own, KEYMAP_DUALSENSE);
        assert_eq!(yabasanshiro(Some(INPUT_DUALSENSE), Some(&own), es, &ds), Verdict::Keep);
        assert_eq!(yabasanshiro(Some(INPUT_DUALSENSE), None, es, &ds), Verdict::Keep, "YabaSanshiro writes its keymap from it");
        assert_eq!(yabasanshiro(None, Some(KEYMAP_EMPTY), es, &ds), Verdict::Keep, "without input.cfg ROCKNIX writes both itself");
        // Another firmware version of the same pad is the same controller.
        let older = INPUT_DUALSENSE.replace(DUALSENSE_GUID, "0300fd574c050000e60c000000810000");
        assert_eq!(yabasanshiro(Some(&older), Some(KEYMAP_DUALSENSE), es, &ds), Verdict::Keep);
    }

    #[test]
    fn nothing_is_reset_when_a_fresh_file_would_not_work_either() {
        let d = TestDir::new("stale-controls-no-help");
        let here = pocket(&d);
        put(&d, "storage/.config/yabasanshiro/input.cfg", INPUT_RETROID);
        // EmulationStation has no entry for InputPlumber's pad: ROCKNIX would write no input.cfg.
        put(&d, ES_INPUT, INPUT_RETROID);
        assert_eq!(before_start_in(&here, "saturn", "yabasanshiro", 1), None);
        put(&d, ES_INPUT, ES_INPUT_CFG);
        // The game can't see ROCKNIX's choice (InputPlumber not running).
        std::fs::remove_file(d.join("dev/input/event9")).unwrap();
        assert_eq!(before_start_in(&here, "saturn", "yabasanshiro", 1), None);
        assert!(d.join(YABA_DIR).join("input.cfg").exists());
        assert!(backups(&d.join(YABA_DIR)).is_empty());
    }

    #[test]
    fn rocknix_writing_the_same_stale_file_again_is_left_after_one_try() {
        let d = TestDir::new("stale-controls-once");
        let here = pocket(&d);
        let dir = d.join(YABA_DIR);
        put(&d, "storage/.config/yabasanshiro/input.cfg", INPUT_DUALSENSE);
        put(&d, "storage/.config/yabasanshiro/keymapv2.json", KEYMAP_EMPTY);
        assert!(before_start_in(&here, "saturn", "yabasanshiro", 100).is_some());
        // YabaSanshiro made the same empty keymap again (its own pad order): no line at every start.
        put(&d, "storage/.config/yabasanshiro/input.cfg", INPUT_DUALSENSE);
        put(&d, "storage/.config/yabasanshiro/keymapv2.json", KEYMAP_EMPTY);
        assert_eq!(before_start_in(&here, "saturn", "yabasanshiro", 200), None);
        assert_eq!(backups(&dir).len(), 1);
        // Two resets in one second keep both copies.
        put(&d, "storage/.config/yabasanshiro/input.cfg", INPUT_RETROID);
        assert!(before_start_in(&here, "saturn", "yabasanshiro", 100).is_some());
        assert_eq!(backups(&dir).len(), 2);
    }

    #[test]
    fn azahar_controls_for_a_controller_that_is_not_here_are_set_aside() {
        let d = TestDir::new("stale-controls-azahar");
        let here = pocket(&d);
        let dir = d.join(AZAHAR_DIR);
        // ROCKNIX's own copy binds InputPlumber's pad: it stays, as do the player's changes to it.
        put(&d, "storage/.config/azahar/qt-config.ini", AZAHAR_IP);
        assert_eq!(before_start_in(&here, "3ds", "azahar", 1), None);
        let own = AZAHAR_IP.replacen("button:1,engine:sdl", "button:2,engine:sdl", 1);
        put(&d, "storage/.config/azahar/qt-config.ini", &own);
        assert_eq!(before_start_in(&here, "3ds", "azahar", 1), None);
        // Made where the real pad was the game's (no InputPlumber then): every button names it.
        let real = AZAHAR_IP.replace(DUALSENSE_GUID, RETROID_GUID);
        put(&d, "storage/.config/azahar/qt-config.ini", &real);
        assert_eq!(before_start_in(&here, "3ds", "azahar", 1).as_deref(), Some("Controls for 3DS were reset to match this controller."));
        assert!(!dir.join("qt-config.ini").exists(), "pocket-ui and start_azahar.sh copy ROCKNIX's");
        assert_eq!(std::fs::read_to_string(backups(&dir)[0].join("qt-config.ini")).unwrap(), real);
    }

    #[test]
    fn azahar_reads_the_active_profile_and_leaves_keyboard_ones() {
        let ds = [pad(DUALSENSE)];
        let real = AZAHAR_IP.replace(DUALSENSE_GUID, RETROID_GUID);
        assert_eq!(azahar_guids(AZAHAR_IP), [DUALSENSE_GUID]);
        assert!(matches!(azahar(Some(&real), AZAHAR_IP, &ds), Verdict::Reset(_)));
        // The second profile is active and binds the DualSense.
        let second = format!("{}\n", real.replace("profile=0", "profile=1").replace("[Controls]",
            &format!("[Controls]\nprofiles\\2\\button_a=\"button:1,engine:sdl,guid:{DUALSENSE_GUID},port:0\"")));
        assert_eq!(azahar(Some(&second), AZAHAR_IP, &ds), Verdict::Keep);
        let keyboard = "[Controls]\nprofile=0\nprofiles\\1\\button_a=\"code:65,engine:keyboard\"\n";
        assert_eq!(azahar(Some(keyboard), AZAHAR_IP, &ds), Verdict::Keep);
        assert_eq!(azahar(Some(&real), &real, &ds), Verdict::Keep, "ROCKNIX's copy wouldn't work either");
        assert_eq!(azahar(None, AZAHAR_IP, &ds), Verdict::Keep);
    }

    #[test]
    fn other_emulators_are_not_touched() {
        let d = TestDir::new("stale-controls-others");
        let here = pocket(&d);
        put(&d, "storage/.config/yabasanshiro/input.cfg", INPUT_RETROID);
        for (system, emulator) in [("saturn", "retroarch"), ("gamecube", "dolphin"), ("ps2", "armsx2"), ("psp", "ppsspp")] {
            assert_eq!(before_start_in(&here, system, emulator, 1), None, "{emulator}");
        }
        assert!(d.join(YABA_DIR).join("input.cfg").exists());
    }

    /// Every file the check reads, missing, empty, cut, flipped or in another encoding: no panic, nothing lost (a file is
    /// in its place or, byte for byte, in the one set-aside folder), and a line on the shelf exactly when files moved.
    #[test]
    fn file_source_rule_stale_controls() {
        // The stale example of each emulator: Alex's Saturn files, a qt-config.ini made for the real pad.
        let real = AZAHAR_IP.replace(DUALSENSE_GUID, RETROID_GUID);
        let sources: &[(&str, &str, &str, &str)] = &[
            ("saturn", "yabasanshiro", "storage/.config/yabasanshiro/input.cfg", INPUT_RETROID),
            ("saturn", "yabasanshiro", "storage/.config/yabasanshiro/keymapv2.json", KEYMAP_EMPTY),
            ("saturn", "yabasanshiro", ES_INPUT, ES_INPUT_CFG),
            ("saturn", "yabasanshiro", "proc/bus/input/devices", DEVICES),
            ("3ds", "azahar", "storage/.config/azahar/qt-config.ini", &real),
            ("3ds", "azahar", AZAHAR_STOCK, AZAHAR_IP),
            ("3ds", "azahar", "proc/bus/input/devices", DEVICES),
        ];
        for &(system, emulator, rel, seed) in sources {
            for (label, bytes) in crate::file_faults::faults(seed.as_bytes()) {
                let d = TestDir::new("stale-controls-faults");
                let here = pocket(&d);
                let (dir, files) = if emulator == "azahar" { (d.join(AZAHAR_DIR), AZAHAR_FILES) } else { (d.join(YABA_DIR), YABA_FILES) };
                put(&d, "storage/.config/yabasanshiro/input.cfg", INPUT_RETROID);
                put(&d, "storage/.config/yabasanshiro/keymapv2.json", KEYMAP_EMPTY);
                put(&d, "storage/.config/azahar/qt-config.ini", &real);
                match &bytes {
                    Some(b) => { std::fs::create_dir_all(d.join(rel).parent().unwrap()).unwrap(); std::fs::write(d.join(rel), b).unwrap(); }
                    None => { let _ = std::fs::remove_file(d.join(rel)); }
                }
                let before: Vec<(&str, Option<Vec<u8>>)> = files.iter().map(|f| (*f, std::fs::read(dir.join(f)).ok())).collect();
                let line = before_start_in(&here, system, emulator, 1_795_643_100);
                let b = backups(&dir);
                assert_eq!(line.is_some(), !b.is_empty(), "{rel} {label}: a line exactly when files moved");
                for (f, was) in before {
                    let now = std::fs::read(dir.join(f)).ok().or_else(|| b.first().and_then(|b| std::fs::read(b.join(f)).ok()));
                    assert_eq!(now, was, "{rel} {label}: {f} kept");
                }
                if label == "valid" {
                    assert!(line.is_some(), "{rel}: the stale controls of the example are set aside");
                }
            }
        }
    }

    #[test]
    fn the_shelf_line_names_the_system_as_the_shelf_does() {
        assert_eq!(reset_line("saturn"), "Controls for Saturn were reset to match this controller.");
        assert_eq!(reset_line("n3ds"), "Controls for 3DS were reset to match this controller.");
    }
}
