//! Save sync with Syncthing, which ROCKNIX ships (docs/user/sync.md, docs/SECURITY-HARDENING.md).
//!
//! * `Config` reads Syncthing's `config.xml`: the API key, the web interface address, the device ID and the
//!   folders.
//! * `Prefs` is what Settings > Sync keeps in `<data>/sync.json`.
//! * `decide` / `after_game` ask Syncthing to rescan the save folders when a game ends. That runs on a thread
//!   of its own with short timeouts and never holds up the shelf.
//! * `service` switches the ROCKNIX service on and off. ROCKNIX's own start script opens the web interface to
//!   the whole network with root's password, so a drop-in swaps it for ours (`start_script`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const CONFIG_XML: &str = "/storage/.config/syncthing/config.xml";
pub const CERT_PEM: &str = "/storage/.config/syncthing/cert.pem";
pub const ROMS_DIR: &str = "/storage/roms";
pub const GUI_PORT: u16 = 8384;
/// The web interface login; ROCKNIX's own script uses `root`.
pub const GUI_USER: &str = "pocket";
/// How long after a rescan we wait for the other devices to have what we have.
pub const SETTLE: Duration = Duration::from_secs(6);
/// Our files: the web interface password and the script the service runs.
pub const STATE_DIR: &str = "/storage/.config/pocket-ui/sync";
pub const PASSWORD_PATH: &str = "/storage/.config/pocket-ui/sync/password";
pub const SCRIPT_PATH: &str = "/storage/.config/pocket-ui/sync/start.sh";
/// systemd reads units from /storage/.config/system.d before the image's (docs/SECURITY-HARDENING.md).
pub const DROPIN_PATH: &str = "/storage/.config/system.d/syncthing.service.d/10-pocket-ui.conf";
/// Which folders we paused while their emulator runs (`apply_holds`), so a crash in between is undone later.
pub const HELD_PATH: &str = "/storage/.config/pocket-ui/sync/held";

/// What can be shared: a row of Settings > Sync, one or two Syncthing folders with ids of ours.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Share {
    /// Battery saves (`.srm` and friends) next to the games.
    Saves,
    /// Save states of every system.
    States,
    /// ARMSX2's memory cards.
    Ps2Cards,
    /// Dolphin: GameCube memory cards and Wii save data.
    Dolphin,
    /// Azahar's 3DS save data.
    Azahar,
    /// The player's curation (favourites, status, notes, stickers, smart shelves, play history), merged on every
    /// device (`library_sync`, docs/SYNC-CURATION.md); off unless asked for.
    Library,
    /// BIOS files; off unless asked for.
    Bios,
}

/// One Syncthing folder.
#[derive(Debug, PartialEq)]
pub struct Dir {
    pub id: &'static str,
    /// The name it has in Syncthing's own web interface.
    pub label: &'static str,
    pub path: &'static str,
    /// Its `.stignore`, a line each (empty: everything in it is shared). Syncthing takes the first line that
    /// matches; `!` shares, the rest keeps out.
    pub ignore: &'static [&'static str],
    /// The emulator makes this folder on its first start, and ROCKNIX's start script copies its own files
    /// into what is around it only when it is missing: so it is added once it exists, never made by us.
    pub wait_for_root: bool,
}

const SAVES: Dir = Dir {
    id: "pocket-saves",
    label: "Pocket saves",
    path: ROMS_DIR,
    // Azahar's save data lies below /storage/roms too, and has its own folder: never shared twice. The versions of
    // a save the player chose not to keep (`sync_conflicts::keep`) stay on this device.
    ignore: &[
        "/3ds/azahar",
        "/sync-backups",
        "!**/*.srm",
        // The cartridge clock beside the battery save (pocket-core and RetroArch: Pokémon Gold, Silver, Crystal).
        "!**/*.rtc",
        "!**/*.sav",
        "!**/*.eep",
        "!**/*.fla",
        "!**/*.sra",
        "!**/*.mcr",
        "!**/*.mcd",
        "!**/*.dsv",
        "*",
    ],
    wait_for_root: false,
};

const STATES: Dir = Dir {
    id: "pocket-states",
    label: "Pocket save states",
    path: "/storage/roms/savestates",
    // Earlier's states stay on this device (`timeline`): up to 1 GB, a new one every 5 minutes of play, and only worth
    // anything on the device that played them. A `.stignore` of the player's own decides instead (`write_ignore_at`).
    ignore: &["*.pstate.earlier*", "*.pstate.started*"],
    wait_for_root: false,
};

/// ARMSX2's memory cards: ROCKNIX's PCSX2.ini keeps them beside the games (`MemoryCards = /storage/roms/ps2`,
/// `Mcd001.ps2`, `Mcd002.ps2`, the multitap ones `Mcd-Multitap…ps2`).
const PS2_CARDS: Dir = Dir {
    id: "pocket-ps2-cards",
    label: "Pocket PS2 memory cards",
    path: "/storage/roms/ps2",
    ignore: &["!/Mcd*.ps2", "*"],
    wait_for_root: true,
};

/// Dolphin's GameCube memory cards: start_dolphin_gc.sh and start_dolphin_wii.sh keep the `.raw` cards and the
/// GCI folders (`GCIFolderAPath`, one per region) in /storage/roms/bios/GC, next to the IPL and DSP files.
const GC_CARDS: Dir = Dir {
    id: "pocket-gc-cards",
    label: "Pocket GameCube memory cards",
    path: "/storage/roms/bios/GC",
    ignore: &["!/MemoryCard*.raw", "!/*/*.gci", "*"],
    wait_for_root: true,
};

/// Wii save data in Dolphin's NAND (`NANDRootPath` empty: the user folder's `Wii`): `title/<type>/<id>/data`
/// of disc games (00010000, 00010004) and channels (00010001), not their content or the system's titles.
const WII_SAVES: Dir = Dir {
    id: "pocket-wii-saves",
    label: "Pocket Wii saves",
    path: "/storage/.config/dolphin-emu/Wii/title",
    ignore: &["!/00010000/*/data", "!/00010001/*/data", "!/00010004/*/data", "*"],
    wait_for_root: true,
};

/// Azahar's save data: start_azahar.sh links its sdmc to /storage/roms/3ds/azahar/sdmc. Each game's save is
/// `title/00040000/<id>/data`, extra data `extdata/00000000/<id>`; installed games, updates and DLC are the
/// `content` folders next to them and stay out.
const AZAHAR_SAVES: Dir = Dir {
    id: "pocket-3ds-saves",
    label: "Pocket 3DS saves",
    path: "/storage/roms/3ds/azahar/sdmc/Nintendo 3DS",
    ignore: &["!/*/*/title/00040000/*/data", "!/*/*/extdata/00000000", "*"],
    wait_for_root: true,
};

/// The data folder of pocket-ui on the device (`POCKETUI_DATA` changes it only for development runs).
pub const DATA_DIR: &str = "/storage/.config/pocket-ui";

/// Library data: a folder per device, each written only by its device and read by all (`library_sync`). Our own
/// temporary files and the last good copies (`fsutil::write_json`) stay out.
const LIBRARY: Dir = Dir {
    id: "pocket-library",
    label: "Pocket library data",
    path: "/storage/.config/pocket-ui/library-sync",
    ignore: &["*.tmp", "*.bak"],
    wait_for_root: false,
};

const BIOS: Dir = Dir {
    id: "pocket-bios",
    label: "Pocket BIOS",
    path: "/storage/roms/bios",
    // The GameCube memory cards beside Dolphin's BIOS files belong to their own row.
    ignore: &["/GC/MemoryCard*.raw", "/GC/*/*.gci"],
    wait_for_root: false,
};

impl Share {
    pub const ALL: [Share; 7] = [Share::Saves, Share::States, Share::Ps2Cards, Share::Dolphin, Share::Azahar, Share::Library, Share::Bios];

    pub fn dirs(self) -> &'static [Dir] {
        match self {
            Share::Saves => &[SAVES],
            Share::States => &[STATES],
            Share::Ps2Cards => &[PS2_CARDS],
            Share::Dolphin => &[GC_CARDS, WII_SAVES],
            Share::Azahar => &[AZAHAR_SAVES],
            Share::Library => &[LIBRARY],
            Share::Bios => &[BIOS],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Share::Saves => "Saves",
            Share::States => "Save states",
            Share::Ps2Cards => "PS2 memory cards",
            Share::Dolphin => "GameCube & Wii saves",
            Share::Azahar => "3DS saves",
            Share::Library => "Library data",
            Share::Bios => "BIOS files",
        }
    }

    /// One line under the choice in Settings.
    pub fn blurb(self) -> &'static str {
        match self {
            Share::Saves => "The .srm files your games write. Recommended.",
            Share::States => "Where you stopped in games. Recommended.",
            Share::Ps2Cards => "Both PS2 memory cards. Paused while playing.",
            Share::Dolphin => "GameCube cards, Wii saves. Paused while playing.",
            Share::Azahar => "3DS save data. Paused while playing.",
            Share::Library => "Favourites, status, notes, stickers, play history.",
            Share::Bios => "Copyrighted. Only between your own devices.",
        }
    }

    pub fn recommended(self) -> bool {
        !matches!(self, Share::Bios | Share::Library)
    }

    /// The share holds saves (all but Library data and BIOS files): what the search for two versions looks through.
    pub fn saves(self) -> bool {
        !matches!(self, Share::Bios | Share::Library)
    }
}

/// The share holding the saves of a system's standalone emulator.
pub fn standalone_share(system: &str) -> Option<Share> {
    match system {
        "ps2" => Some(Share::Ps2Cards),
        "gamecube" | "wii" | "wiiware" => Some(Share::Dolphin),
        "3ds" => Some(Share::Azahar),
        _ => None,
    }
}

// --- config.xml -------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Folder {
    pub id: String,
    pub label: String,
    pub path: String,
    pub paused: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub api_key: String,
    /// `host:port` the web interface listens on, as written in the file.
    pub gui_address: String,
    pub gui_tls: bool,
    pub gui_user: String,
    pub gui_password_set: bool,
    /// This device's ID; empty while it is not known (see `with_cert`).
    pub device_id: String,
    /// Every device in the file, this one included, sorted by ID like Syncthing keeps them.
    pub devices: Vec<String>,
    pub folders: Vec<Folder>,
}

/// Who can reach the web interface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Exposure {
    Localhost,
    Network { tls: bool },
}

/// What is wrong with how the web interface is set up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Problem {
    /// Open to the network without a password.
    NoPassword,
    /// ROCKNIX's login: user root, root's password (`rocknix` unless it was changed).
    RocknixLogin,
    /// Open to the network over plain http.
    ClearText,
}

fn child<'a, 'i>(node: roxmltree::Node<'a, 'i>, tag: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children().find(|c| c.has_tag_name(tag))
}

fn text<'a>(node: roxmltree::Node<'a, '_>, tag: &str) -> &'a str {
    child(node, tag).and_then(|c| c.text()).map(str::trim).unwrap_or("")
}

/// `("127.0.0.1", "8384")` of `"127.0.0.1:8384"`; a listen-everywhere host becomes loopback, because that is
/// where we reach it from.
fn split_address(address: &str) -> (&str, &str) {
    let (host, port) = address.rsplit_once(':').unwrap_or((address, ""));
    (host, port)
}

impl Config {
    pub fn parse(xml: &str) -> Result<Config, String> {
        let doc = roxmltree::Document::parse(xml).map_err(|e| format!("config.xml: {e}"))?;
        let root = doc.root_element();
        if !root.has_tag_name("configuration") {
            return Err("config.xml: not a Syncthing configuration".into());
        }
        let gui = child(root, "gui").ok_or("config.xml: no <gui> section")?;
        let api_key = text(gui, "apikey");
        if api_key.is_empty() {
            return Err("config.xml: no API key".into());
        }
        // The <device> elements directly below <configuration>; the ones inside <folder> and <defaults> only
        // say who a folder is shared with.
        let devices: Vec<String> = root
            .children()
            .filter(|c| c.has_tag_name("device"))
            .filter_map(|d| d.attribute("id"))
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect();
        if devices.is_empty() {
            return Err("config.xml: no device".into());
        }
        // Syncthing keeps the devices sorted by ID, not itself first: with one device it is this one, with more
        // the certificate has to say (`with_cert`).
        let device_id = if devices.len() == 1 { devices[0].clone() } else { String::new() };
        let folders = root
            .children()
            .filter(|c| c.has_tag_name("folder"))
            .map(|f| Folder {
                id: f.attribute("id").unwrap_or_default().to_string(),
                label: f.attribute("label").unwrap_or_default().to_string(),
                path: f.attribute("path").unwrap_or_default().to_string(),
                paused: text(f, "paused") == "true",
            })
            .collect();
        Ok(Config {
            api_key: api_key.to_string(),
            gui_address: text(gui, "address").to_string(),
            gui_tls: gui.attribute("tls") == Some("true"),
            gui_user: text(gui, "user").to_string(),
            gui_password_set: !text(gui, "password").is_empty(),
            device_id,
            devices,
            folders,
        })
    }

    /// Takes this device's ID from its certificate (`cert.pem`), when that is one of the devices in the file.
    pub fn with_cert(mut self, cert_pem: &str) -> Config {
        if let Some(id) = device_id_of_cert(cert_pem).filter(|id| self.devices.contains(id)) {
            self.device_id = id;
        }
        self
    }

    /// The configuration Syncthing keeps on this device.
    pub fn read() -> Result<Config, String> {
        let xml = std::fs::read_to_string(CONFIG_XML).map_err(|e| format!("{CONFIG_XML}: {e}"))?;
        let cert = std::fs::read_to_string(CERT_PEM).unwrap_or_default();
        Ok(Config::parse(&xml)?.with_cert(&cert))
    }

    fn scheme(&self) -> &'static str {
        if self.gui_tls {
            "https"
        } else {
            "http"
        }
    }

    fn port(&self) -> String {
        let (_, port) = split_address(&self.gui_address);
        if port.is_empty() {
            GUI_PORT.to_string()
        } else {
            port.to_string()
        }
    }

    /// Where pocket-ui itself reaches the REST API: the loopback address, with the scheme the interface uses.
    pub fn api_base(&self) -> String {
        let (host, _) = split_address(&self.gui_address);
        let host = match host {
            "" | "0.0.0.0" | "[::]" | "::" | "*" => "127.0.0.1",
            h => h,
        };
        format!("{}://{host}:{}", self.scheme(), self.port())
    }

    /// What to type into a browser on another computer; `ip` is this device's address when known.
    pub fn gui_url(&self, ip: Option<&str>) -> String {
        format!("{}://{}:{}", self.scheme(), ip.unwrap_or("<device address>"), self.port())
    }

    pub fn exposure(&self) -> Exposure {
        let (host, _) = split_address(&self.gui_address);
        match host {
            "127.0.0.1" | "localhost" | "[::1]" => Exposure::Localhost,
            _ => Exposure::Network { tls: self.gui_tls },
        }
    }

    pub fn problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        if let Exposure::Network { tls } = self.exposure() {
            if !self.gui_password_set {
                problems.push(Problem::NoPassword);
            }
            if self.gui_user == "root" {
                problems.push(Problem::RocknixLogin);
            }
            if !tls {
                problems.push(Problem::ClearText);
            }
        }
        problems
    }

    fn folder(&self, id: &str) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    fn set_paused(&mut self, id: &str, paused: bool) {
        if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
            f.paused = paused;
        }
    }
}

const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Syncthing's check character for 13 characters of the ID (a Luhn mod 32).
fn luhn32(group: &[u8]) -> u8 {
    let (mut factor, mut sum) = (1usize, 0usize);
    for c in group {
        let code = BASE32.iter().position(|b| b == c).unwrap_or(0);
        let addend = factor * code;
        factor = if factor == 2 { 1 } else { 2 };
        sum += addend / 32 + addend % 32;
    }
    BASE32[(32 - sum % 32) % 32]
}

/// The device ID of the certificate in `pem`, the way Syncthing makes it: SHA-256 of the certificate, in
/// base32, a check character after every 13, written in groups of 7 (`P7T4NX2-24CSKCT-...`).
pub fn device_id_of_cert(pem: &str) -> Option<String> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let body: String = pem.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("-----")).collect();
    let der = base64::engine::general_purpose::STANDARD.decode(body).ok().filter(|d| !d.is_empty())?;
    let hash = Sha256::digest(&der);
    // 32 bytes are 52 base32 characters.
    let (mut bits, mut have, mut plain) = (0u32, 0u32, Vec::with_capacity(52));
    for byte in hash.iter() {
        bits = (bits << 8) | *byte as u32;
        have += 8;
        while have >= 5 {
            plain.push(BASE32[((bits >> (have - 5)) & 31) as usize]);
            have -= 5;
        }
    }
    if have > 0 {
        plain.push(BASE32[((bits << (5 - have)) & 31) as usize]);
    }
    let mut with_checks = Vec::with_capacity(56);
    for group in plain.chunks(13) {
        with_checks.extend_from_slice(group);
        with_checks.push(luhn32(group));
    }
    let groups: Vec<&str> = with_checks.chunks(7).map(|g| std::str::from_utf8(g).unwrap_or("")).collect();
    Some(groups.join("-"))
}

// --- what Settings keeps ----------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Syncthing is switched on (the ROCKNIX setting `syncthing.enabled` is the truth at boot; this is what we set).
    pub enabled: bool,
    /// Rescan the save folders when a game ends.
    pub after_play: bool,
    pub saves: bool,
    pub states: bool,
    pub ps2: bool,
    pub dolphin: bool,
    pub azahar: bool,
    /// Library data (`library_sync`): off at the start.
    pub library: bool,
    pub bios: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            enabled: false,
            after_play: true,
            saves: true,
            states: true,
            ps2: true,
            dolphin: true,
            azahar: true,
            library: false,
            bios: false,
        }
    }
}

const PREFS_FILE: &str = "sync.json";

impl Prefs {
    pub fn load(data_dir: &Path) -> Prefs {
        crate::fsutil::read_json_or_backup(&data_dir.join(PREFS_FILE)).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) {
        if let Err(e) = crate::fsutil::write_json(&data_dir.join(PREFS_FILE), self) {
            eprintln!("sync: can't save {PREFS_FILE}: {e}");
        }
    }

    pub fn shared(&self, share: Share) -> bool {
        match share {
            Share::Saves => self.saves,
            Share::States => self.states,
            Share::Ps2Cards => self.ps2,
            Share::Dolphin => self.dolphin,
            Share::Azahar => self.azahar,
            Share::Library => self.library,
            Share::Bios => self.bios,
        }
    }

    pub fn set_shared(&mut self, share: Share, on: bool) {
        let flag = match share {
            Share::Saves => &mut self.saves,
            Share::States => &mut self.states,
            Share::Ps2Cards => &mut self.ps2,
            Share::Dolphin => &mut self.dolphin,
            Share::Azahar => &mut self.azahar,
            Share::Library => &mut self.library,
            Share::Bios => &mut self.bios,
        };
        *flag = on;
    }

    /// Whether a game exit should even look at Syncthing.
    pub fn wants_scan(&self) -> bool {
        self.enabled && self.after_play
    }
}

// --- the scan after a game ----------------------------------------------------------------------------------

/// One `POST /rest/db/scan`.
#[derive(Clone, Debug, PartialEq)]
pub struct ScanReq {
    pub folder: String,
    /// Only this folder below the share's root (the system the game belongs to).
    pub sub: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Skip {
    SyncOff,
    AfterPlayOff,
    NotRunning,
    NothingShared,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    Skip(Skip),
    Scan(Vec<ScanReq>),
}

/// The folder of the game's system below `/storage/roms` (`gba` for `/storage/roms/gba/Game.gba`).
pub fn scan_sub(game: &Path) -> Option<String> {
    let rel = game.strip_prefix(ROMS_DIR).ok()?;
    let mut parts = rel.components();
    let system = parts.next()?.as_os_str().to_str()?;
    // A file directly in /storage/roms has no system folder.
    parts.next()?;
    Some(system.to_string())
}

/// Whether a game that just ended should be followed by a rescan, and of what. `running` is asked only when
/// everything else says yes (it may look at the process list).
pub fn decide(prefs: &Prefs, cfg: Option<&Config>, game: &Path, running: impl FnOnce() -> bool) -> Decision {
    if !prefs.enabled {
        return Decision::Skip(Skip::SyncOff);
    }
    if !prefs.after_play {
        return Decision::Skip(Skip::AfterPlayOff);
    }
    let Some(cfg) = cfg.filter(|_| running()) else {
        return Decision::Skip(Skip::NotRunning);
    };
    let sub = scan_sub(game);
    let present = |d: &&Dir| cfg.folder(d.id).is_some_and(|f| !f.paused);
    // Playing changes saves and states; BIOS files are never rescanned for it.
    let mut reqs: Vec<ScanReq> = [Share::Saves, Share::States]
        .into_iter()
        .filter(|s| prefs.shared(*s))
        .flat_map(Share::dirs)
        .filter(present)
        .map(|d| ScanReq {
            folder: d.id.to_string(),
            sub: sub.clone(),
        })
        .collect();
    // A standalone emulator's own saves (memory cards, NAND, sdmc): the folders of the game's system, whole.
    if let Some(share) = sub.as_deref().and_then(standalone_share).filter(|s| prefs.shared(*s)) {
        reqs.extend(share.dirs().iter().filter(present).map(|d| ScanReq {
            folder: d.id.to_string(),
            sub: None,
        }));
    }
    if reqs.is_empty() {
        Decision::Skip(Skip::NothingShared)
    } else {
        Decision::Scan(reqs)
    }
}

// --- the standalone emulators' folders while they run --------------------------------------------------------

/// The folders to keep paused while a game of `system` runs: its emulator writes them (memory cards are whole
/// files), and Syncthing must neither send a half-written one nor replace one the emulator holds open.
///
/// That is every game's battery saves and states too (review N1): pocket-core and RetroArch keep the battery
/// save in memory and write it every few seconds and at the end, so a newer `.srm` Syncthing brought in while
/// the game ran was overwritten by the next write, which Syncthing then took for an ordinary later change of
/// ours and sent on: the other device's save was gone, without a conflict. Paused, the incoming change waits;
/// resumed after the game (`run_after_game`), both sides have changed the file and Syncthing keeps the older
/// one as a `.sync-conflict-…` copy instead of losing it.
pub fn held_for(prefs: &Prefs, system: &str) -> Vec<&'static str> {
    if !prefs.enabled {
        return Vec::new();
    }
    [Share::Saves, Share::States]
        .into_iter()
        .chain(standalone_share(system))
        .filter(|s| prefs.shared(*s))
        .flat_map(Share::dirs)
        .map(|d| d.id)
        .collect()
}

/// What `apply_holds` changes: folders to pause, folders to resume, and the ones paused by us afterwards.
#[derive(Debug, Default, PartialEq)]
pub struct HoldPlan {
    pub pause: Vec<String>,
    pub resume: Vec<String>,
    pub held: Vec<String>,
}

/// From the folders wanted paused now, the ones we paused before (`held`) and Syncthing's config. A folder
/// paused by someone else (by hand, in the web interface) is never resumed by us.
pub fn hold_plan(want: &[&str], held: &[String], cfg: &Config) -> HoldPlan {
    let mut plan = HoldPlan::default();
    for id in want {
        match cfg.folder(id) {
            Some(f) if !f.paused => {
                plan.pause.push(id.to_string());
                plan.held.push(id.to_string());
            }
            Some(_) if held.iter().any(|h| h == id) => plan.held.push(id.to_string()),
            _ => {}
        }
    }
    for id in held.iter().filter(|h| !want.contains(&h.as_str())) {
        if cfg.folder(id).is_some_and(|f| f.paused) {
            plan.resume.push(id.clone());
        }
    }
    plan
}

/// The folders wanted paused now (set when a game starts and ends, on the UI thread).
static WANT_HELD: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
/// One `apply_holds` at a time: each takes the latest wish, so a quick end and start come out right.
static APPLYING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A game of `system` starts (or, with None, the game ended): which folders are to be paused from now on.
/// Returns them; nothing is sent to Syncthing here (`apply_holds` does that, on a thread).
pub fn want_held(prefs: &Prefs, system: Option<&str>) -> Vec<&'static str> {
    let ids = system.map(|s| held_for(prefs, s)).unwrap_or_default();
    if let Ok(mut w) = WANT_HELD.lock() {
        *w = ids.clone();
    }
    ids
}

fn read_held(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether we left folders paused (pocket-ui stopped while a big game ran).
pub fn holds_left(path: &Path) -> bool {
    !read_held(path).is_empty()
}

/// Pauses and resumes our folders as `want_held` asked, keeping `cfg` in step and `held_path` up to date.
pub fn apply_holds(api: &mut dyn Api, cfg: &mut Config, held_path: &Path) -> Result<(), String> {
    let _one = APPLYING.lock();
    let want = WANT_HELD.lock().map(|w| w.clone()).unwrap_or_default();
    let before = read_held(held_path);
    let plan = hold_plan(&want, &before, cfg);
    let mut held: Vec<String> = plan.held.iter().filter(|id| !plan.pause.contains(id)).cloned().collect();
    let mut error = None;
    for id in &plan.pause {
        match api.set_paused(id, true) {
            Ok(()) => {
                cfg.set_paused(id, true);
                held.push(id.clone());
            }
            Err(e) => error = Some(e),
        }
    }
    for id in &plan.resume {
        match api.set_paused(id, false) {
            Ok(()) => cfg.set_paused(id, false),
            Err(e) => {
                held.push(id.clone());
                error = Some(e);
            }
        }
    }
    if held != before {
        let text: String = held.iter().map(|id| format!("{id}\n")).collect();
        let written = if held.is_empty() {
            std::fs::remove_file(held_path).or(Ok(()))
        } else {
            crate::fsutil::write_atomic(held_path, text.as_bytes())
        };
        if let Err(e) = written {
            error.get_or_insert(format!("{}: {e}", held_path.display()));
        }
    }
    if !plan.pause.is_empty() || !plan.resume.is_empty() {
        println!("sync: paused {:?}, resumed {:?}", plan.pause, plan.resume);
    }
    error.map_or(Ok(()), Err)
}

/// Folders of shares that are on but not in Syncthing yet, because their emulator had not made them when sync
/// was switched on, and that exist now (`exists` asks).
pub fn missing_dirs(prefs: &Prefs, cfg: &Config, exists: impl Fn(&str) -> bool) -> Vec<&'static Dir> {
    Share::ALL
        .into_iter()
        .filter(|s| prefs.shared(*s))
        .flat_map(Share::dirs)
        .filter(|d| d.wait_for_root && cfg.folder(d.id).is_none() && exists(d.path))
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Peers {
    /// Other devices Syncthing knows.
    pub configured: usize,
    /// Of those, the ones connected now.
    pub connected: usize,
}

/// Syncthing, as far as the scan after a game needs it. The real one talks to the REST API with curl.
pub trait Api: Send {
    fn running(&mut self) -> bool;
    fn scan(&mut self, req: &ScanReq) -> Result<(), String>;
    fn peers(&mut self) -> Result<Peers, String>;
    /// Items the other devices still lack of `folders`, or this device lacks of theirs.
    fn outstanding(&mut self, folders: &[String]) -> Result<u64, String>;
    /// Pauses or resumes one folder.
    fn set_paused(&mut self, folder: &str, paused: bool) -> Result<(), String>;
    /// Adds one of our folders, offered to the devices the others are offered to.
    fn add_folder(&mut self, dir: &'static Dir) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Skipped(Skip),
    /// The scan ran and the connected devices have everything.
    Synced,
    /// The scan ran and devices are connected, but still pulling.
    Syncing,
    /// The scan ran, no other device is connected: the saves wait here.
    Waiting,
    Failed(String),
}

impl Outcome {
    /// The small notice on the shelf, if there is one.
    pub fn notice(&self) -> Option<String> {
        match self {
            Outcome::Synced => Some("Saves synced".into()),
            Outcome::Syncing => Some("Syncing saves".into()),
            Outcome::Waiting => Some("Sync waiting for a device".into()),
            Outcome::Failed(_) => Some("Sync failed".into()),
            Outcome::Skipped(Skip::NotRunning) => Some("Syncthing is not running".into()),
            Outcome::Skipped(_) => None,
        }
    }
}

/// Runs the decision and, if it says so, the scans. Blocks for as long as Syncthing takes: call it on a thread.
/// First the folders held while the game ran are resumed (`held_path`, see `apply_holds`), and folders whose
/// emulator has made them by now are added.
pub fn run_after_game(prefs: &Prefs, cfg: Option<&Config>, game: &Path, api: &mut dyn Api, settle: Duration, held_path: &Path) -> Outcome {
    let mut cfg = cfg.cloned();
    if let Some(c) = cfg.as_mut().filter(|_| prefs.enabled && api.running()) {
        if let Err(e) = apply_holds(api, c, held_path) {
            println!("sync: resuming the folders held while playing: {e}");
        }
        // Folders added before a line was added to their `.stignore` (Earlier's states, 2026-10-25) get it now; a file of
        // the player's own is left alone.
        for d in Share::ALL.iter().filter(|s| prefs.shared(**s)).flat_map(|s| s.dirs()).filter(|d| c.folder(d.id).is_some() && Path::new(d.path).is_dir()) {
            if let Err(e) = write_ignore(d) {
                println!("sync: {}/.stignore: {e}", d.path);
            }
        }
        for d in missing_dirs(prefs, c, |p| Path::new(p).is_dir()) {
            match write_ignore(d).map_err(|e| e.to_string()).and_then(|()| api.add_folder(d)) {
                Ok(()) => c.folders.push(Folder {
                    id: d.id.into(),
                    label: d.label.into(),
                    path: d.path.into(),
                    paused: false,
                }),
                Err(e) => println!("sync: adding {}: {e}", d.id),
            }
        }
    }
    let reqs = match decide(prefs, cfg.as_ref(), game, || api.running()) {
        Decision::Skip(why) => return Outcome::Skipped(why),
        Decision::Scan(reqs) => reqs,
    };
    for req in &reqs {
        if let Err(e) = api.scan(req) {
            return Outcome::Failed(e);
        }
    }
    match api.peers() {
        Err(e) => return Outcome::Failed(e),
        Ok(p) if p.connected == 0 => return Outcome::Waiting,
        Ok(_) => {}
    }
    // The other device pulls what the scan found; give it a moment before saying "synced".
    let folders: Vec<String> = reqs.iter().map(|r| r.folder.clone()).collect();
    let deadline = Instant::now() + settle;
    loop {
        match api.outstanding(&folders) {
            Ok(0) => return Outcome::Synced,
            Ok(_) if Instant::now() >= deadline => return Outcome::Syncing,
            Ok(_) => std::thread::sleep(Duration::from_millis(500)),
            Err(e) => return Outcome::Failed(e),
        }
    }
}

/// Whether a game's end has anything for Syncthing to do: the scan, or (with sync after playing off) the
/// folders held while the game ran, or a standalone emulator's folder to add now that it exists.
/// Folders wanted paused for the game count even before their pause is written down: every game holds the saves
/// now (`held_for`), and a short game may end before its pause is through.
pub fn after_game_due(prefs: &Prefs, game: &Path, held_path: &Path) -> bool {
    let wanted = WANT_HELD.lock().is_ok_and(|w| !w.is_empty());
    prefs.wants_scan()
        || (prefs.enabled && (wanted || holds_left(held_path) || scan_sub(game).as_deref().and_then(standalone_share).is_some_and(|s| prefs.shared(s))))
}

/// Called when a game has ended. Starts the scan on a thread of its own and returns at once; `done` is
/// called from that thread with the result. False (and nothing started) when sync is off.
pub fn after_game(prefs: Prefs, cfg: Option<Config>, game: PathBuf, api: impl Api + 'static, done: impl FnOnce(Outcome) + Send + 'static) -> bool {
    let held = PathBuf::from(HELD_PATH);
    want_held(&prefs, None);
    if !after_game_due(&prefs, &game, &held) {
        return false;
    }
    std::thread::Builder::new()
        .name("sync-after-game".into())
        .spawn(move || {
            let mut api = api;
            done(run_after_game(&prefs, cfg.as_ref(), &game, &mut api, SETTLE, &held));
        })
        .is_ok()
}

// --- talking to Syncthing --------------------------------------------------------------------------------------

/// The REST API through the system's curl, as the rest of pocket-ui talks to the network. The certificate is
/// Syncthing's own self-signed one and the address is loopback, so it isn't checked.
pub struct CurlApi {
    base: String,
    key: String,
    own: String,
}

impl CurlApi {
    pub fn new(cfg: &Config) -> CurlApi {
        CurlApi {
            base: cfg.api_base(),
            key: cfg.api_key.clone(),
            own: cfg.device_id.clone(),
        }
    }

    /// (HTTP status, body). `Err` when curl couldn't connect or timed out.
    pub fn call(&self, method: &str, path: &str, body: Option<&str>, timeout: Duration) -> Result<(u16, String), String> {
        let mut cmd = std::process::Command::new("curl");
        cmd.args([
            "-sk",
            "-m",
            &timeout.as_secs().max(1).to_string(),
            "-X",
            method,
            "-H",
            &format!("X-API-Key: {}", self.key),
        ]);
        if let Some(body) = body {
            cmd.args(["-H", "Content-Type: application/json", "-d", body]);
        }
        cmd.args(["-w", "\n%{http_code}", &format!("{}{path}", self.base)]);
        let out = crate::proc::run_with_timeout(&mut cmd, timeout + Duration::from_secs(2)).map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "curl exited with {}",
                out.status.code().map_or("a signal".to_string(), |c| c.to_string())
            ));
        }
        let all = String::from_utf8_lossy(&out.stdout).into_owned();
        let (text, code) = all.rsplit_once('\n').unwrap_or(("", &all));
        let code: u16 = code.trim().parse().map_err(|_| "no status from Syncthing".to_string())?;
        Ok((code, text.to_string()))
    }

    fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
        match self.call("GET", path, None, Duration::from_secs(4))? {
            (200, body) => serde_json::from_str(&body).map_err(|e| format!("{path}: {e}")),
            (code, _) => Err(format!("{path}: HTTP {code}")),
        }
    }

    pub fn health(&self) -> bool {
        matches!(self.call("GET", "/rest/noauth/health", None, Duration::from_secs(2)), Ok((200, _)))
    }

    pub fn snapshot(&mut self, folders: &[String]) -> Snapshot {
        if !self.health() {
            return Snapshot::default();
        }
        let peers = self.peers().unwrap_or_default();
        let outstanding = if peers.connected > 0 { self.outstanding(folders).unwrap_or(0) } else { 0 };
        Snapshot {
            running: true,
            peers,
            outstanding,
        }
    }

    /// This device's ID: from the certificate when the config could say, else Syncthing's own answer.
    pub fn own(&mut self) -> Result<String, String> {
        if self.own.is_empty() {
            self.own = self.get_json("/rest/system/status")?["myID"]
                .as_str()
                .ok_or("Syncthing didn't say its device ID")?
                .to_string();
        }
        Ok(self.own.clone())
    }

    /// Creates or replaces one of our folders; `others` are the devices it is offered to besides this one.
    pub fn put_folder(&mut self, dir: &Dir, others: &[String]) -> Result<(), String> {
        let json = folder_json(dir, &self.own()?, others);
        match self.call("PUT", &format!("/rest/config/folders/{}", dir.id), Some(&json), Duration::from_secs(8))? {
            (200, _) => Ok(()),
            (code, body) => Err(format!("folder {}: HTTP {code} {}", dir.id, body.trim())),
        }
    }

    pub fn delete_folder(&self, id: &str) -> Result<(), String> {
        match self.call("DELETE", &format!("/rest/config/folders/{id}"), None, Duration::from_secs(8))? {
            (200 | 404, _) => Ok(()),
            (code, _) => Err(format!("folder {id}: HTTP {code}")),
        }
    }

    /// The IDs of the devices besides this one.
    pub fn other_devices(&mut self) -> Result<Vec<String>, String> {
        let own = self.own()?;
        let devices = self.get_json("/rest/config/devices")?;
        Ok(devices
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|d| d["deviceID"].as_str())
                    .filter(|id| *id != own)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default())
    }

    /// The other devices that are connected now.
    fn connected(&mut self) -> Result<Vec<String>, String> {
        let own = self.own()?;
        let conns = self.get_json("/rest/system/connections")?;
        Ok(conns["connections"]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter(|(id, c)| **id != own && c["connected"].as_bool() == Some(true))
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default())
    }
}

impl Api for CurlApi {
    fn running(&mut self) -> bool {
        self.health()
    }

    fn scan(&mut self, req: &ScanReq) -> Result<(), String> {
        let mut path = format!("/rest/db/scan?folder={}", req.folder);
        if let Some(sub) = &req.sub {
            path.push_str("&sub=");
            path.push_str(&url_escape(sub));
        }
        // The call returns when the scan is done.
        match self.call("POST", &path, None, Duration::from_secs(15))? {
            (200, _) => Ok(()),
            (code, _) => Err(format!("scan {}: HTTP {code}", req.folder)),
        }
    }

    fn peers(&mut self) -> Result<Peers, String> {
        let configured = self.other_devices()?.len();
        let connected = self.connected()?.len();
        Ok(Peers { configured, connected })
    }

    fn outstanding(&mut self, folders: &[String]) -> Result<u64, String> {
        let connected = self.connected()?;
        let mut total = 0;
        for folder in folders {
            total += self.get_json(&format!("/rest/db/status?folder={folder}"))?["needTotalItems"]
                .as_u64()
                .unwrap_or(0);
            for device in &connected {
                total += self.get_json(&format!("/rest/db/completion?folder={folder}&device={device}"))?["needItems"]
                    .as_u64()
                    .unwrap_or(0);
            }
        }
        Ok(total)
    }

    fn set_paused(&mut self, folder: &str, paused: bool) -> Result<(), String> {
        let body = serde_json::json!({ "paused": paused }).to_string();
        match self.call("PATCH", &format!("/rest/config/folders/{folder}"), Some(&body), Duration::from_secs(8))? {
            (200, _) => Ok(()),
            (code, body) => Err(format!("pause {folder}: HTTP {code} {}", body.trim())),
        }
    }

    fn add_folder(&mut self, dir: &'static Dir) -> Result<(), String> {
        let others = self.other_devices()?;
        self.put_folder(dir, &others)
    }
}

/// Enough of percent-encoding for a folder name in a query string.
fn url_escape(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// The folder as Syncthing's REST API takes it.
pub fn folder_json(dir: &Dir, own: &str, others: &[String]) -> String {
    let devices: Vec<serde_json::Value> = std::iter::once(own)
        .chain(others.iter().map(String::as_str))
        .map(|id| serde_json::json!({ "deviceID": id }))
        .collect();
    serde_json::json!({
        "id": dir.id,
        "label": dir.label,
        "path": dir.path,
        "type": "sendreceive",
        "devices": devices,
        "rescanIntervalS": 3600,
        "fsWatcherEnabled": true,
        "fsWatcherDelayS": 10,
        // The card's file system has no permissions worth syncing.
        "ignorePerms": true,
    })
    .to_string()
}

/// A folder's `.stignore`, None when everything in it is shared.
pub fn ignore_file(dir: &Dir) -> Option<String> {
    if dir.ignore.is_empty() {
        return None;
    }
    let mut s = String::from("// Written by pocket-ui (Settings › Connections › Sync). The first line that matches counts: ! shares, the rest keeps out.\n");
    for line in dir.ignore {
        s.push_str(line);
        s.push('\n');
    }
    Some(s)
}

/// `/storage/roms/.stignore`: of the game folders only the save files are shared (the save states and BIOS have
/// folders of their own, and are left out of this one).
pub fn saves_ignore() -> String {
    ignore_file(&SAVES).unwrap_or_default()
}

/// Writes `.stignore` of the saves folder before the folder exists, so that its first scan doesn't hash the
/// games. Leaves a file the user wrote alone.
pub fn write_saves_ignore(dir: &Path) -> std::io::Result<()> {
    write_ignore_at(dir, &saves_ignore())
}

fn write_ignore_at(dir: &Path, text: &str) -> std::io::Result<()> {
    let path = dir.join(".stignore");
    match std::fs::read_to_string(&path) {
        Ok(old) if !old.starts_with("// Written by pocket-ui") || old == text => Ok(()),
        _ => crate::fsutil::write_atomic(&path, text.as_bytes()),
    }
}

/// Brings `/storage/roms/.stignore` up to date when pocket-ui wrote it (a line added since, such as the backup
/// folder of `sync_conflicts`); a missing one or the user's own is left alone.
pub fn refresh_saves_ignore() {
    let dir = Path::new(ROMS_DIR);
    if dir.join(".stignore").is_file() {
        if let Err(e) = write_saves_ignore(dir) {
            println!("sync: {}/.stignore: {e}", dir.display());
        }
    }
}

/// Writes a folder's `.stignore` before Syncthing first scans it, so no game file is ever hashed or offered.
pub fn write_ignore(dir: &Dir) -> std::io::Result<()> {
    match ignore_file(dir) {
        Some(text) => write_ignore_at(Path::new(dir.path), &text),
        None => Ok(()),
    }
}

// --- status line ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub running: bool,
    pub peers: Peers,
    pub outstanding: u64,
}

/// "Up to date", "Syncing 3 files", "Not connected" ...
pub fn status_line(enabled: bool, snap: Option<&Snapshot>) -> String {
    if !enabled {
        return "Off".into();
    }
    let Some(s) = snap else { return "Starting".into() };
    if !s.running {
        "Not running".into()
    } else if s.peers.connected == 0 {
        "Not connected".into()
    } else if s.outstanding > 0 {
        format!("Syncing {} file{}", s.outstanding, if s.outstanding == 1 { "" } else { "s" })
    } else {
        "Up to date".into()
    }
}

// --- the service ---------------------------------------------------------------------------------------------

/// The script the Syncthing service runs instead of ROCKNIX's `start_syncthing.sh`. That one resets the web
/// interface to every address, with root's password (`rocknix` unless it was changed), at each start.
pub fn start_script() -> String {
    format!(
        r#"#!/bin/sh
# Written by pocket-ui (src/sync.rs). Replaces ROCKNIX's start_syncthing.sh, which opens Syncthing's web
# interface to the whole network with root's password at every start.
export HOME=/storage
DIR={STATE_DIR}
CONF=/storage/.config/syncthing
PASS=$(cat "$DIR/password" 2>/dev/null)
[ -n "$PASS" ] || {{ echo "pocket-ui sync: no password file, not starting" >&2; exit 1; }}
mkdir -p "$CONF"
# Keys on the first start; afterwards only the login is set again. The password comes in on stdin.
printf '%s' "$PASS" | syncthing generate --home "$CONF" --gui-user {GUI_USER} --gui-password -
xmlstarlet ed --inplace \
  -u "//configuration/gui/address" -v "0.0.0.0:{GUI_PORT}" \
  -u "//configuration/gui/@tls" -v true \
  -u "//configuration/options/urAccepted" -v -1 \
  -u "//configuration/options/crashReportingEnabled" -v false \
  -u "//configuration/options/startBrowser" -v false \
  -u "//configuration/options/autoUpgradeIntervalH" -v 0 \
  "$CONF/config.xml" || exit 1
exec syncthing serve --home "$CONF" --no-browser --no-restart
"#
    )
}

/// The systemd drop-in that makes the service run `start_script`.
pub fn dropin(script: &Path) -> String {
    format!("[Service]\nExecStart=\nExecStart={}\n", script.display())
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::process::Command;

    fn sh(script: &str, timeout: Duration) -> Result<(), String> {
        let out = crate::proc::run_with_timeout(Command::new("sh").arg("-c").arg(script), timeout).map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("failed").trim().to_string())
        }
    }

    /// The service is up (a process alive is enough; the REST API says more).
    pub fn running() -> bool {
        crate::proc::status_with_timeout(Command::new("systemctl").args(["is-active", "--quiet", "syncthing"]), crate::proc::HELPER).is_ok_and(|s| s.success())
    }

    pub fn password() -> Option<String> {
        crate::secret::load(Path::new(PASSWORD_PATH))
    }

    /// Writes our script and drop-in, sets ROCKNIX's `syncthing.enabled` (so that it starts at boot too) and
    /// (re)starts the service. Returns the web interface password.
    pub fn enable() -> Result<String, String> {
        let pw = crate::secret::load_or_create(Path::new(PASSWORD_PATH)).map_err(|e| e.to_string())?;
        crate::fsutil::write_atomic(Path::new(SCRIPT_PATH), start_script().as_bytes()).map_err(|e| e.to_string())?;
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(SCRIPT_PATH, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        }
        crate::fsutil::write_atomic(Path::new(DROPIN_PATH), dropin(Path::new(SCRIPT_PATH)).as_bytes()).map_err(|e| e.to_string())?;
        // system.cfg under ROCKNIX's lock (`system_cfg`), then the service.
        crate::system_cfg::set("syncthing.enabled", "1");
        sh("systemctl daemon-reload; systemctl restart syncthing", Duration::from_secs(30))?;
        Ok(pw)
    }

    pub fn disable() -> Result<(), String> {
        crate::system_cfg::set("syncthing.enabled", "0");
        sh("systemctl stop syncthing", Duration::from_secs(30))
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    //! Desktop preview: a switch that remembers its state.
    use std::sync::atomic::{AtomicBool, Ordering};

    static ON: AtomicBool = AtomicBool::new(false);

    pub fn running() -> bool {
        ON.load(Ordering::Relaxed)
    }

    pub fn password() -> Option<String> {
        ON.load(Ordering::Relaxed).then(|| "otter-cedar-lamp-315".into())
    }

    pub fn enable() -> Result<String, String> {
        ON.store(true, Ordering::Relaxed);
        Ok("otter-cedar-lamp-315".into())
    }

    pub fn disable() -> Result<(), String> {
        ON.store(false, Ordering::Relaxed);
        Ok(())
    }
}

pub use imp::{disable, enable, password, running};

/// After the service is up: makes the folders the user chose exist in Syncthing and removes the ones they
/// switched off. Waits up to 30 s for the REST API.
pub fn apply_folders(prefs: &Prefs) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let cfg = loop {
        if let Ok(cfg) = Config::read() {
            let api = CurlApi::new(&cfg);
            if api.health() {
                break cfg;
            }
        }
        if Instant::now() >= deadline {
            return Err("Syncthing did not answer".into());
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    let mut api = CurlApi::new(&cfg);
    let others = api.other_devices()?;
    for share in Share::ALL {
        for dir in share.dirs() {
            if prefs.shared(share) {
                put_dir(&mut api, dir, &others)?;
            } else if cfg.folder(dir.id).is_some() {
                api.delete_folder(dir.id)?;
            }
        }
    }
    Ok(())
}

/// Adds or updates one folder with its `.stignore` first; one the emulator hasn't made yet waits for it (it is
/// added after a game, `run_after_game`).
fn put_dir(api: &mut CurlApi, dir: &Dir, others: &[String]) -> Result<(), String> {
    if dir.wait_for_root && !Path::new(dir.path).is_dir() {
        return Ok(());
    }
    // Ours to make (Library data's folder); the others are there on every Pocket.
    if !dir.wait_for_root && !Path::new(dir.path).exists() {
        std::fs::create_dir_all(dir.path).map_err(|e| format!("{}: {e}", dir.path))?;
    }
    write_ignore(dir).map_err(|e| format!("{}: {e}", dir.path))?;
    api.put_folder(dir, others)
}

/// Switches one share on or off while Syncthing runs.
pub fn apply_share(share: Share, on: bool) -> Result<(), String> {
    let cfg = Config::read()?;
    let mut api = CurlApi::new(&cfg);
    let others = if on { api.other_devices()? } else { Vec::new() };
    for dir in share.dirs() {
        if on {
            put_dir(&mut api, dir, &others)?;
        } else {
            api.delete_folder(dir.id)?;
        }
    }
    Ok(())
}

/// What Settings > Sync shows: read from Syncthing (or its config file), not from memory.
pub fn snapshot(prefs: &Prefs) -> Snapshot {
    let Ok(cfg) = Config::read() else { return Snapshot::default() };
    let folders: Vec<String> = Share::ALL
        .iter()
        .filter(|s| prefs.shared(**s))
        .flat_map(|s| s.dirs())
        .filter(|d| cfg.folder(d.id).is_some())
        .map(|d| d.id.to_string())
        .collect();
    CurlApi::new(&cfg).snapshot(&folders)
}

/// A game of `system` starts: pauses the folders it writes (its battery saves and states, and a standalone
/// emulator's own), on a thread (it returns at once).
pub fn hold_for_game(prefs: &Prefs, system: &str) {
    if want_held(prefs, Some(system)).is_empty() {
        return;
    }
    std::thread::spawn(|| {
        let Ok(mut cfg) = Config::read() else { return };
        let mut api = CurlApi::new(&cfg);
        if api.health() {
            if let Err(e) = apply_holds(&mut api, &mut cfg, Path::new(HELD_PATH)) {
                println!("sync: pausing the folders of the running game: {e}");
            }
        }
    });
}

/// At start: folders still paused from a game pocket-ui never saw end are resumed once that emulator has ended
/// (it outlives a pocket-ui that stopped) and Syncthing answers.
pub fn release_leftover_holds() {
    if !holds_left(Path::new(HELD_PATH)) {
        return;
    }
    std::thread::spawn(|| loop {
        if !crate::standalone::emulator_running() {
            if let Ok(mut cfg) = Config::read() {
                let mut api = CurlApi::new(&cfg);
                if api.health() {
                    if let Err(e) = apply_holds(&mut api, &mut cfg, Path::new(HELD_PATH)) {
                        println!("sync: resuming folders left paused: {e}");
                    }
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(5));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    const POCKET: &str = include_str!("fixtures/syncthing-pocket.xml");
    const ROCKNIX: &str = include_str!("fixtures/syncthing-rocknix.xml");
    const FAKE_ID: &str = "MFZWI3D-BONSGYC-YLTMRWG-C43ENR5-QXGZDMM-FZWI3DP-BONSGYY-LTMRWAD";

    fn cfg() -> Config {
        Config::parse(POCKET).unwrap()
    }

    fn on() -> Prefs {
        Prefs {
            enabled: true,
            ..Prefs::default()
        }
    }

    fn game() -> &'static Path {
        Path::new("/storage/roms/gba/Final Fantasy VI Advance.gba")
    }

    // --- config.xml, against files copied from the Mini V2 -------------------------------------------------

    #[test]
    fn the_api_key_the_address_and_the_device_come_out_of_the_device_config() {
        let c = cfg();
        assert_eq!(c.api_key, "TestApiKeyNotSecret0123456789abcd");
        assert_eq!(c.gui_address, "0.0.0.0:8384");
        assert!(c.gui_tls);
        assert_eq!(c.gui_user, "pocket");
        assert!(c.gui_password_set);
        assert_eq!(c.device_id, FAKE_ID, "the device, not a folder's device entry or the defaults");
    }

    /// Found in the virtual device: once a second device is added, Syncthing keeps its list sorted by ID, so
    /// "the first device" is whichever ID sorts first, not this device. The certificate says which is ours.
    const PAIRED: &str = include_str!("fixtures/syncthing-paired.xml");
    const CERT: &str = include_str!("fixtures/syncthing-cert.pem");
    const OWN_ID: &str = "Z7N2XOA-NE6BM5C-6S7R3FK-DNTXC7Y-OHAJ2LH-H2B6O7Y-RI6TV2Q-MHTW7QV";

    #[test]
    fn the_device_id_is_worked_out_from_the_certificate() {
        assert_eq!(device_id_of_cert(CERT).as_deref(), Some(OWN_ID));
        assert_eq!(device_id_of_cert("not a certificate"), None);
        assert_eq!(device_id_of_cert("-----BEGIN CERTIFICATE-----\n!!!!\n-----END CERTIFICATE-----\n"), None);
    }

    #[test]
    fn a_paired_device_that_sorts_first_does_not_become_ours() {
        let c = Config::parse(PAIRED).unwrap();
        assert_eq!(c.devices.len(), 2);
        assert_eq!(c.device_id, "", "two devices and no certificate: not guessed from the order of the file");
        let c = c.with_cert(CERT);
        assert_eq!(c.device_id, OWN_ID, "this device's ID, not the other device's (which sorts first)");
        assert_ne!(c.devices[0], OWN_ID);
        // A certificate of some other device does not make it ours.
        assert_eq!(Config::parse(PAIRED).unwrap().with_cert("junk").device_id, "");
    }

    #[test]
    fn a_file_with_one_device_needs_no_certificate() {
        assert_eq!(cfg().devices, vec![FAKE_ID.to_string()]);
        assert_eq!(cfg().device_id, FAKE_ID);
        assert_eq!(cfg().with_cert(CERT).device_id, FAKE_ID, "the certificate of another device is ignored");
    }

    #[test]
    fn the_folder_list_is_the_configured_folders_only() {
        let folders = cfg().folders;
        let ids: Vec<&str> = folders.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["pocket-saves", "pocket-states", "photos"], "the empty <defaults> folder is not one");
        assert_eq!(folders[0].path, "/storage/roms");
        assert_eq!(folders[1].label, "Pocket save states");
        assert!(!folders[0].paused && folders[2].paused);
    }

    #[test]
    fn rocknixs_own_config_parses_too() {
        let c = Config::parse(ROCKNIX).unwrap();
        assert_eq!(c.gui_user, "root");
        assert_eq!(c.gui_address, "0.0.0.0:8384");
        assert!(!c.gui_tls);
        assert!(c.folders.is_empty());
        assert_eq!(c.device_id, FAKE_ID);
    }

    #[test]
    fn a_config_without_a_key_or_a_device_is_an_error_not_a_guess() {
        assert!(Config::parse("").is_err());
        assert!(
            Config::parse("<configuration><gui><address>127.0.0.1:8384</address></gui></configuration>").is_err(),
            "no api key"
        );
        assert!(
            Config::parse("<configuration><gui><apikey>k</apikey></gui></configuration>").is_err(),
            "no device"
        );
        assert!(Config::parse("<other/>").is_err());
    }

    #[test]
    fn the_rest_api_is_reached_on_loopback_with_the_scheme_the_interface_uses() {
        assert_eq!(cfg().api_base(), "https://127.0.0.1:8384");
        assert_eq!(Config::parse(ROCKNIX).unwrap().api_base(), "http://127.0.0.1:8384");
        let mut c = cfg();
        c.gui_address = "127.0.0.1:8390".into();
        assert_eq!(c.api_base(), "https://127.0.0.1:8390");
        c.gui_address = "[::]:8384".into();
        assert_eq!(c.api_base(), "https://127.0.0.1:8384");
    }

    #[test]
    fn the_browser_address_uses_the_devices_ip() {
        assert_eq!(cfg().gui_url(Some("192.0.2.57")), "https://192.0.2.57:8384");
        assert_eq!(Config::parse(ROCKNIX).unwrap().gui_url(Some("192.0.2.57")), "http://192.0.2.57:8384");
        assert_eq!(cfg().gui_url(None), "https://<device address>:8384");
    }

    #[test]
    fn rocknixs_default_login_is_flagged_and_ours_is_not() {
        let rk = Config::parse(ROCKNIX).unwrap();
        assert_eq!(rk.exposure(), Exposure::Network { tls: false });
        assert_eq!(rk.problems(), vec![Problem::RocknixLogin, Problem::ClearText]);
        assert_eq!(cfg().exposure(), Exposure::Network { tls: true });
        assert_eq!(cfg().problems(), vec![]);
    }

    #[test]
    fn an_interface_without_a_password_on_the_network_is_a_problem_on_loopback_it_is_not() {
        let mut c = cfg();
        c.gui_password_set = false;
        assert_eq!(c.problems(), vec![Problem::NoPassword]);
        c.gui_address = "127.0.0.1:8384".into();
        assert_eq!(c.exposure(), Exposure::Localhost);
        assert_eq!(c.problems(), vec![]);
    }

    // --- the scan after a game ------------------------------------------------------------------------------

    fn req(folder: &str, sub: &str) -> ScanReq {
        ScanReq {
            folder: folder.into(),
            sub: Some(sub.into()),
        }
    }

    #[test]
    fn the_scan_looks_at_the_games_system_folder() {
        assert_eq!(scan_sub(game()).as_deref(), Some("gba"));
        assert_eq!(scan_sub(Path::new("/storage/roms/psx/Crash/Crash.cue")).as_deref(), Some("psx"));
        assert_eq!(scan_sub(Path::new("/somewhere/else/game.gba")), None, "whole folder then");
    }

    #[test]
    fn sync_off_means_no_scan_and_no_questions_asked() {
        let d = decide(&Prefs::default(), Some(&cfg()), game(), || panic!("must not look for Syncthing"));
        assert_eq!(d, Decision::Skip(Skip::SyncOff));
    }

    #[test]
    fn sync_after_playing_off_means_no_scan() {
        let prefs = Prefs { after_play: false, ..on() };
        assert_eq!(
            decide(&prefs, Some(&cfg()), game(), || panic!("must not look")),
            Decision::Skip(Skip::AfterPlayOff)
        );
        assert!(!prefs.wants_scan());
    }

    #[test]
    fn sync_on_scans_saves_and_states_of_that_system() {
        assert_eq!(
            decide(&on(), Some(&cfg()), game(), || true),
            Decision::Scan(vec![req("pocket-saves", "gba"), req("pocket-states", "gba")])
        );
        assert!(on().wants_scan(), "after playing is on by default once Syncthing is");
    }

    #[test]
    fn syncthing_not_running_means_no_scan() {
        assert_eq!(decide(&on(), Some(&cfg()), game(), || false), Decision::Skip(Skip::NotRunning));
        assert_eq!(
            decide(&on(), None, game(), || true),
            Decision::Skip(Skip::NotRunning),
            "no readable config is as good as not running"
        );
    }

    #[test]
    fn only_shared_unpaused_folders_are_scanned_and_bios_never() {
        let prefs = Prefs {
            states: false,
            bios: true,
            ..on()
        };
        assert_eq!(decide(&prefs, Some(&cfg()), game(), || true), Decision::Scan(vec![req("pocket-saves", "gba")]));
        // Switched on in Settings but not (yet) a folder of Syncthing's: nothing to scan.
        let mut c = cfg();
        c.folders.retain(|f| f.id != "pocket-saves");
        assert_eq!(
            decide(&Prefs { states: false, ..on() }, Some(&c), game(), || true),
            Decision::Skip(Skip::NothingShared)
        );
        // Paused by hand in the web interface.
        let mut c = cfg();
        c.folders[0].paused = true;
        assert_eq!(decide(&on(), Some(&c), game(), || true), Decision::Scan(vec![req("pocket-states", "gba")]));
    }

    /// Syncthing that waits for the test before it answers a scan.
    struct Gate {
        release: mpsc::Receiver<()>,
        scans: Arc<Mutex<Vec<ScanReq>>>,
        peers: Peers,
        outstanding: u64,
    }

    impl Api for Gate {
        fn running(&mut self) -> bool {
            true
        }
        fn scan(&mut self, req: &ScanReq) -> Result<(), String> {
            self.release.recv().map_err(|e| e.to_string())?;
            self.scans.lock().unwrap().push(req.clone());
            Ok(())
        }
        fn peers(&mut self) -> Result<Peers, String> {
            Ok(self.peers)
        }
        fn outstanding(&mut self, _folders: &[String]) -> Result<u64, String> {
            Ok(self.outstanding)
        }
        fn set_paused(&mut self, folder: &str, _paused: bool) -> Result<(), String> {
            Err(format!("nothing is held in these tests: {folder}"))
        }
        fn add_folder(&mut self, dir: &'static Dir) -> Result<(), String> {
            Err(format!("no folder is added in these tests: {}", dir.id))
        }
    }

    fn gate(peers: Peers) -> (mpsc::Sender<()>, Gate, Arc<Mutex<Vec<ScanReq>>>) {
        let (tx, rx) = mpsc::channel();
        let scans = Arc::new(Mutex::new(Vec::new()));
        (
            tx,
            Gate {
                release: rx,
                scans: scans.clone(),
                peers,
                outstanding: 0,
            },
            scans,
        )
    }

    #[test]
    fn returning_to_the_shelf_does_not_wait_for_syncthing() {
        let (open, api, scans) = gate(Peers { configured: 1, connected: 1 });
        let (done_tx, done_rx) = mpsc::channel();
        let started = Instant::now();
        let spawned = after_game(on(), Some(cfg()), game().to_path_buf(), api, move |o| done_tx.send(o).unwrap());
        let took = started.elapsed();
        assert!(spawned);
        assert!(
            took < Duration::from_millis(200),
            "after_game blocked for {took:?}: the scan must be spawned, not awaited"
        );
        // Syncthing has not answered, and the shelf is already back.
        assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
        assert!(scans.lock().unwrap().is_empty());
        open.send(()).unwrap();
        open.send(()).unwrap();
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(5)).unwrap(), Outcome::Synced);
        assert_eq!(scans.lock().unwrap().len(), 2);
    }

    #[test]
    fn nothing_is_spawned_when_sync_is_off() {
        let (_open, api, _scans) = gate(Peers::default());
        let spawned = after_game(Prefs::default(), Some(cfg()), game().to_path_buf(), api, |_| panic!("not called"));
        assert!(!spawned);
    }

    #[test]
    fn a_scan_with_no_device_connected_says_the_saves_wait() {
        let (open, mut api, _scans) = gate(Peers { configured: 1, connected: 0 });
        open.send(()).unwrap();
        open.send(()).unwrap();
        let out = run_after_game(&on(), Some(&cfg()), game(), &mut api, Duration::ZERO, Path::new("/nonexistent/held"));
        assert_eq!(out, Outcome::Waiting);
        assert_eq!(out.notice().as_deref(), Some("Sync waiting for a device"));
    }

    #[test]
    fn a_scan_with_the_other_device_up_to_date_says_synced_and_with_work_left_says_syncing() {
        let (open, mut api, _scans) = gate(Peers { configured: 1, connected: 1 });
        open.send(()).unwrap();
        open.send(()).unwrap();
        let out = run_after_game(&on(), Some(&cfg()), game(), &mut api, Duration::ZERO, Path::new("/nonexistent/held"));
        assert_eq!(out, Outcome::Synced);
        assert_eq!(out.notice().as_deref(), Some("Saves synced"));

        let (open, mut api, _scans) = gate(Peers { configured: 1, connected: 1 });
        api.outstanding = 3;
        open.send(()).unwrap();
        open.send(()).unwrap();
        let out = run_after_game(&on(), Some(&cfg()), game(), &mut api, Duration::ZERO, Path::new("/nonexistent/held"));
        assert_eq!(out, Outcome::Syncing);
        assert_eq!(out.notice().as_deref(), Some("Syncing saves"));
    }

    #[test]
    fn skipped_and_failed_scans_have_the_right_notices() {
        assert_eq!(Outcome::Skipped(Skip::SyncOff).notice(), None);
        assert_eq!(Outcome::Skipped(Skip::AfterPlayOff).notice(), None);
        assert_eq!(Outcome::Skipped(Skip::NotRunning).notice().as_deref(), Some("Syncthing is not running"));
        assert_eq!(Outcome::Failed("timeout".into()).notice().as_deref(), Some("Sync failed"));
    }

    // --- status line ----------------------------------------------------------------------------------------

    #[test]
    fn the_status_line_says_what_is_going_on() {
        let snap = |running, configured, connected, outstanding| Snapshot {
            running,
            peers: Peers { configured, connected },
            outstanding,
        };
        assert_eq!(status_line(false, None), "Off");
        assert_eq!(status_line(true, None), "Starting");
        assert_eq!(status_line(true, Some(&snap(false, 0, 0, 0))), "Not running");
        assert_eq!(status_line(true, Some(&snap(true, 0, 0, 0))), "Not connected");
        assert_eq!(status_line(true, Some(&snap(true, 2, 0, 0))), "Not connected");
        assert_eq!(status_line(true, Some(&snap(true, 2, 1, 0))), "Up to date");
        assert_eq!(status_line(true, Some(&snap(true, 2, 1, 1))), "Syncing 1 file");
        assert_eq!(status_line(true, Some(&snap(true, 2, 2, 3))), "Syncing 3 files");
    }

    // --- prefs and the service files ------------------------------------------------------------------------------

    #[test]
    fn prefs_default_to_saves_and_states_but_never_bios() {
        let p = Prefs::default();
        assert!(!p.enabled && p.after_play);
        assert!(p.shared(Share::Saves) && p.shared(Share::States) && !p.shared(Share::Bios));
        assert!(Share::ALL.iter().filter(|s| s.recommended()).count() == 5 && !Share::Bios.recommended());
    }

    #[test]
    fn prefs_survive_a_restart_and_an_old_or_odd_file() {
        let dir = TestDir::new("sync-prefs");
        assert_eq!(Prefs::load(&dir), Prefs::default());
        let mut p = on();
        p.after_play = false;
        p.set_shared(Share::Bios, true);
        p.save(&dir);
        assert_eq!(Prefs::load(&dir), p);
        std::fs::write(dir.join("sync.json"), br#"{"enabled": true}"#).unwrap();
        assert_eq!(Prefs::load(&dir), on(), "missing fields mean the defaults");
        std::fs::write(dir.join("sync.json"), b"not json").unwrap();
        assert_eq!(Prefs::load(&dir), p, "damaged: the last good copy");
        let _ = std::fs::remove_file(dir.join("sync.json.bak"));
        assert_eq!(Prefs::load(&dir), Prefs::default(), "damaged and no copy: the defaults");
    }

    #[test]
    fn our_start_script_never_opens_the_interface_without_our_own_password() {
        let s = start_script();
        assert!(s.contains("0.0.0.0:8384") && s.contains("@tls") && s.contains("--gui-user pocket"), "{s}");
        assert!(s.contains("--gui-password -"), "the password goes in on stdin, not on the command line");
        assert!(s.contains("exit 1"), "no password file, no start");
        assert!(
            !s.contains("root.password") && !s.contains("get_setting root"),
            "root's password has nothing to do with it"
        );
        assert!(s.contains("exec syncthing"));
    }

    #[test]
    fn the_dropin_replaces_exec_start() {
        let d = dropin(Path::new("/storage/.config/pocket-ui/sync/start.sh"));
        let lines: Vec<&str> = d.lines().collect();
        assert_eq!(lines[0], "[Service]");
        assert_eq!(lines[1], "ExecStart=", "clears ROCKNIX's start_syncthing.sh first");
        assert_eq!(lines[2], "ExecStart=/storage/.config/pocket-ui/sync/start.sh");
    }

    // --- the REST calls, against a small server in this process ----------------------------------------------

    /// Answers every request with `status` and `body` and reports the request line and headers.
    fn serve(status: u16, body: &'static str) -> (String, mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
                let reply = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        (base, rx)
    }

    fn api_at(base: &str) -> CurlApi {
        CurlApi {
            base: base.to_string(),
            key: "KEY123".into(),
            own: FAKE_ID.into(),
        }
    }

    #[test]
    fn the_scan_is_a_post_with_the_key_the_folder_and_the_systems_subfolder() {
        let (base, seen) = serve(200, "");
        api_at(&base).scan(&req("pocket-saves", "gba")).unwrap();
        let request = seen.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /rest/db/scan?folder=pocket-saves&sub=gba "), "{request}");
        assert!(request.to_ascii_lowercase().contains("x-api-key: key123"), "{request}");
    }

    #[test]
    fn a_refused_scan_and_a_dead_syncthing_are_errors() {
        let (base, _seen) = serve(403, "forbidden");
        assert!(api_at(&base).scan(&req("pocket-saves", "gba")).unwrap_err().contains("403"));
        // Nothing listens there.
        let mut api = api_at("http://127.0.0.1:9");
        assert!(!api.running());
        assert!(api.scan(&req("pocket-saves", "gba")).is_err());
    }

    #[test]
    fn the_other_devices_are_the_configured_ones_minus_this_one() {
        let (base, _seen) = serve(
            200,
            r#"[{"deviceID":"MFZWI3D-BONSGYC-YLTMRWG-C43ENR5-QXGZDMM-FZWI3DP-BONSGYY-LTMRWAD"},{"deviceID":"OTHER"},{"deviceID":"THIRD"}]"#,
        );
        let mut api = api_at(&base);
        assert_eq!(api.other_devices().unwrap(), vec!["OTHER".to_string(), "THIRD".to_string()]);
        // The same answer comes back for the connections call: no `connections` object, nobody connected.
        let peers = api.peers().unwrap();
        assert_eq!((peers.configured, peers.connected), (2, 0));
    }

    #[test]
    fn our_folders_are_sendreceive_with_the_other_devices_and_the_saves_folder_only_shares_save_files() {
        let json: serde_json::Value = serde_json::from_str(&folder_json(&Share::States.dirs()[0], FAKE_ID, &["OTHER".to_string()])).unwrap();
        assert_eq!(json["id"], "pocket-states");
        assert_eq!(json["path"], "/storage/roms/savestates");
        assert_eq!(json["type"], "sendreceive");
        assert_eq!(json["devices"].as_array().unwrap().len(), 2);
        let ignore = saves_ignore();
        assert!(ignore.starts_with("// Written by pocket-ui"));
        assert!(ignore.contains("!**/*.srm\n") && ignore.contains("!**/*.sav\n"));
        assert!(ignore.trim_end().ends_with("\n*"), "everything else is ignored, the games above all: {ignore}");
        assert!(!ignore.contains("gba") && !ignore.contains("iso"), "no game files");
    }

    // --- the standalone emulators' saves ---------------------------------------------------------------------

    /// Matches `.stignore` patterns the way Syncthing does for the ones we write: first match wins, `!` shares,
    /// a leading `/` anchors at the folder, a pattern also covers everything below it, `*` stays inside one
    /// directory and `**` crosses them.
    fn st_ignored(stignore: &str, path: &str) -> bool {
        fn glob(p: &[u8], s: &[u8]) -> bool {
            match p {
                [] => s.is_empty(),
                [b'*', b'*', rest @ ..] => (0..=s.len()).any(|i| glob(rest, &s[i..])),
                [b'*', rest @ ..] => (0..=s.len()).take_while(|&i| i == 0 || s[i - 1] != b'/').any(|i| glob(rest, &s[i..])),
                [b'?', rest @ ..] => s.first().is_some_and(|c| *c != b'/') && glob(rest, &s[1..]),
                [c, rest @ ..] => s.first() == Some(c) && glob(rest, &s[1..]),
            }
        }
        for line in stignore.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("//")) {
            let (share, pat) = line.strip_prefix('!').map_or((false, line), |p| (true, p));
            let pats = match pat.strip_prefix('/') {
                Some(rooted) => vec![rooted.to_string()],
                None => vec![pat.to_string(), format!("**/{pat}")],
            };
            if pats
                .iter()
                .any(|p| glob(p.as_bytes(), path.as_bytes()) || glob(format!("{p}/**").as_bytes(), path.as_bytes()))
            {
                return !share;
            }
        }
        false
    }

    fn dir(id: &str) -> &'static Dir {
        Share::ALL
            .iter()
            .flat_map(|s| s.dirs())
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("no folder {id}"))
    }

    /// Asserts which of `paths` the folder shares (`true`) and which it keeps out.
    fn shares(id: &str, paths: &[(&str, bool)]) {
        let d = dir(id);
        let ignore = ignore_file(d).unwrap_or_default();
        for (path, shared) in paths {
            assert_eq!(!st_ignored(&ignore, path), *shared, "{id} ({}): {path}", d.path);
        }
    }

    /// Game files of the big systems, as they lie in the folders below. None may ever be shared.
    const ROMS: &[&str] = &[
        "Ico (USA).iso",
        "Burnout 3 - Takedown (USA).chd",
        "SSX Tricky (USA).cso",
        "Game (USA).bin",
        "Game (USA).cue",
        "Game.m3u",
        "sub/Game.iso",
        "Mcd001.iso",
    ];

    #[test]
    fn ps2_memory_cards_are_shared_and_ps2_games_never() {
        assert_eq!(dir("pocket-ps2-cards").path, "/storage/roms/ps2");
        let mut paths: Vec<(&str, bool)> = vec![
            ("Mcd001.ps2", true),
            ("Mcd002.ps2", true),
            ("Mcd-Multitap1-Slot02.ps2", true),
            ("gamelist.xml", false),
        ];
        paths.extend(ROMS.iter().map(|r| (*r, false)));
        shares("pocket-ps2-cards", &paths);
    }

    #[test]
    fn gamecube_cards_and_wii_saves_are_shared_and_bios_and_games_never() {
        // ROCKNIX keeps the GameCube memory cards (and its GCI folders) beside the IPL in /storage/roms/bios/GC.
        assert_eq!(dir("pocket-gc-cards").path, "/storage/roms/bios/GC");
        let mut paths = vec![
            ("MemoryCardA.USA.raw", true),
            ("MemoryCardB.EUR.raw", true),
            ("USA/01-GM4E-MarioKart Double Dash!!.gci", true),
            ("JAP/01-GZLJ-gczelda.gci", true),
            ("USA/IPL.bin", false),
            ("dsp_rom.bin", false),
            ("dsp_coef.bin", false),
            ("font_western.bin", false),
            ("Triforce/segaboot.gcm", false),
        ];
        paths.extend(ROMS.iter().map(|r| (*r, false)));
        shares("pocket-gc-cards", &paths);
        // Wii: the save data of each title in Dolphin's NAND, nothing of the system or of installed channels.
        assert_eq!(dir("pocket-wii-saves").path, "/storage/.config/dolphin-emu/Wii/title");
        shares(
            "pocket-wii-saves",
            &[
                ("00010000/524d4745/data/banner.bin", true),
                ("00010000/524d4745/data/rksys.dat", true),
                ("00010004/524d4245/data/save.bin", true),
                ("00010000/524d4745/content/title.tmd", false),
                ("00010001/48414445/content/00000001.app", false),
                ("00000001/00000002/data/setting.txt", false),
                ("00010002/48414341/data/x", false),
            ],
        );
    }

    #[test]
    fn azahar_saves_are_shared_and_installed_games_never() {
        assert_eq!(dir("pocket-3ds-saves").path, "/storage/roms/3ds/azahar/sdmc/Nintendo 3DS");
        let id = "00000000000000000000000000000000/00000000000000000000000000000000";
        let p = |rest: &str| format!("{id}/{rest}");
        let (save, extdata) = (
            p("title/00040000/00055d00/data/00000001/main"),
            p("extdata/00000000/00000a8f/00000001/user/save.dat"),
        );
        let (app, update, dlc) = (
            p("title/00040000/00055d00/content/00000000.app"),
            p("title/0004000e/00055d00/content/00000000.app"),
            p("title/0004008c/00055d00/content/00000001.app"),
        );
        let sys = p("extdata/00048000/f000000b/x");
        shares(
            "pocket-3ds-saves",
            &[
                (&save, true),
                (&extdata, true),
                (&app, false),
                (&update, false),
                (&dlc, false),
                (&sys, false),
                ("Game.3ds", false),
                ("Game.cia", false),
            ],
        );
    }

    #[test]
    fn the_saves_folder_leaves_the_other_folders_to_themselves() {
        // Folders of ours below /storage/roms are not shared twice, and no game file is shared at all.
        shares(
            "pocket-saves",
            &[
                ("gba/Pokemon Emerald.srm", true),
                ("gbc/Pokemon Crystal.rtc", true),
                ("psx/memcards/card.mcd", true),
                // Syncthing's copy of the other version travels like the save; the versions the player chose not to
                // keep stay here (`sync_conflicts::keep`).
                ("gba/Pokemon Emerald.sync-conflict-20261024-181502-ABCDEF7.srm", true),
                ("sync-backups/2026-10-25/gba/Pokemon Emerald.srm", false),
                ("sync-backups/2026-10-25/ps2/Mcd001.ps2", false),
                ("3ds/azahar/sdmc/Nintendo 3DS/0/0/title/00040000/1/data/00000001/game.sav", false),
                ("ps2/Mcd001.ps2", false),
                ("bios/GC/MemoryCardA.USA.raw", false),
                ("gba/Pokemon Emerald.gba", false),
                ("ps2/Ico (USA).iso", false),
                ("3ds/Game.3ds", false),
                ("wii/Game.wbfs", false),
                ("gamecube/Game.rvz", false),
            ],
        );
        // The BIOS folder (off unless asked for) leaves the GameCube memory cards to their own folder.
        shares(
            "pocket-bios",
            &[
                ("scph5501.bin", true),
                ("GC/USA/IPL.bin", true),
                ("GC/MemoryCardA.USA.raw", false),
                ("GC/USA/01-GM4E-MarioKart Double Dash!!.gci", false),
            ],
        );
        // States: every state and its picture, but Earlier's (`timeline`): they stay on the device that played them.
        let states = ignore_file(dir("pocket-states")).unwrap();
        for (path, shared) in [
            ("gba/Golden Sun.pstate", true),
            ("gba/Golden Sun.pstate.png", true),
            ("gba/Golden Sun.pstate3", true),
            ("gba/Golden Sun.pstate.auto", true),
            ("gba/Golden Sun.pstate.undo", true),
            ("gba/Golden Sun.state1", true),
            ("ps2/SLES-12345 (ABCD1234).01.p2s", true),
            ("gba/Golden Sun.pstate.earlier3", false),
            ("gba/Golden Sun.pstate.earlier3.png", false),
            ("psx/Final Fantasy VII.pstate.started1", false),
        ] {
            assert_eq!(!st_ignored(&states, path), shared, "{path}");
        }
    }

    #[test]
    fn the_new_rows_are_on_by_default_and_old_settings_files_get_them() {
        let p = Prefs::default();
        for s in [Share::Ps2Cards, Share::Dolphin, Share::Azahar] {
            assert!(p.shared(s), "{}", s.label());
        }
        let dir = TestDir::new("sync-newrows");
        std::fs::write(dir.join("sync.json"), br#"{"enabled": true, "saves": true, "states": false, "bios": false}"#).unwrap();
        let p = Prefs::load(&dir);
        assert!(p.shared(Share::Ps2Cards) && p.shared(Share::Dolphin) && p.shared(Share::Azahar) && !p.shared(Share::States));
        let ids: Vec<&str> = Share::ALL.iter().flat_map(|s| s.dirs()).map(|d| d.id).collect();
        assert!(ids.iter().enumerate().all(|(i, id)| !ids[..i].contains(id)), "{ids:?}");
    }

    #[test]
    fn each_big_system_has_its_share() {
        assert_eq!(standalone_share("ps2"), Some(Share::Ps2Cards));
        assert_eq!(standalone_share("gamecube"), Some(Share::Dolphin));
        assert_eq!(standalone_share("wii"), Some(Share::Dolphin));
        assert_eq!(standalone_share("wiiware"), Some(Share::Dolphin));
        assert_eq!(standalone_share("3ds"), Some(Share::Azahar));
        assert_eq!(standalone_share("gba"), None);
    }

    fn with_folders(ids: &[(&str, bool)]) -> Config {
        let mut c = cfg();
        c.folders = ids
            .iter()
            .map(|(id, paused)| Folder {
                id: id.to_string(),
                label: String::new(),
                path: String::new(),
                paused: *paused,
            })
            .collect();
        c
    }

    #[test]
    fn the_running_emulators_folders_are_paused_and_only_ours_are_resumed() {
        let c = with_folders(&[
            ("pocket-saves", false),
            ("pocket-gc-cards", false),
            ("pocket-wii-saves", true),
            ("pocket-ps2-cards", false),
        ]);
        // A Wii game starts: both Dolphin folders are wanted paused; the Wii one was paused by hand already. (The
        // battery saves and states are held too, `every_game_holds_…`; left out here.)
        let only_dolphin = Prefs { saves: false, states: false, ..on() };
        let want = held_for(&only_dolphin, "wii");
        assert_eq!(want, ["pocket-gc-cards", "pocket-wii-saves"]);
        let p = hold_plan(&want, &[], &c);
        assert_eq!(p.pause, ["pocket-gc-cards"]);
        assert!(p.resume.is_empty());
        assert_eq!(p.held, ["pocket-gc-cards"], "the folder paused by hand is not ours to resume");
        // The game ends: ours is resumed, the one paused by hand stays paused.
        let c = with_folders(&[("pocket-gc-cards", true), ("pocket-wii-saves", true)]);
        let p = hold_plan(&[], &["pocket-gc-cards".to_string()], &c);
        assert_eq!((p.pause.len(), p.resume.as_slice(), p.held.len()), (0, &["pocket-gc-cards".to_string()][..], 0));
        // Started again before the release came through (Options applied by a restart): kept paused, still ours.
        let p = hold_plan(&want, &["pocket-gc-cards".to_string()], &c);
        assert!(p.pause.is_empty() && p.resume.is_empty());
        assert_eq!(p.held, ["pocket-gc-cards"]);
        // Nothing held for a share that is off, for sync off, or for a system without one.
        assert!(held_for(&Prefs { dolphin: false, ..only_dolphin }, "wii").is_empty());
        assert!(held_for(&Prefs::default(), "ps2").is_empty());
        assert_eq!(held_for(&Prefs { saves: false, states: false, ..on() }, "gba"), Vec::<&str>::new());
        assert_eq!(held_for(&Prefs { saves: false, states: false, ..on() }, "ps2"), ["pocket-ps2-cards"]);
    }

    #[test]
    fn every_game_holds_the_battery_saves_and_states_so_an_incoming_save_is_not_overwritten() {
        // Review N1: a newer .srm arriving while pocket-core (or RetroArch) runs was overwritten by the game's next
        // write of the one it holds in memory. Both folders stay paused while any game runs.
        assert_eq!(held_for(&on(), "gba"), ["pocket-saves", "pocket-states"]);
        assert_eq!(held_for(&on(), "snes"), ["pocket-saves", "pocket-states"]);
        assert_eq!(held_for(&on(), "ps2"), ["pocket-saves", "pocket-states", "pocket-ps2-cards"]);
        assert_eq!(held_for(&Prefs { states: false, ..on() }, "gba"), ["pocket-saves"]);
        assert!(held_for(&Prefs { enabled: false, ..on() }, "gba").is_empty());
        // Paused when the game starts, resumed (and so rescanned) when it ends; one paused by hand stays paused.
        let c = with_folders(&[("pocket-saves", false), ("pocket-states", true)]);
        let p = hold_plan(&held_for(&on(), "gba"), &[], &c);
        assert_eq!((p.pause.as_slice(), p.held.as_slice()), (&["pocket-saves".to_string()][..], &["pocket-saves".to_string()][..]));
        let c = with_folders(&[("pocket-saves", true), ("pocket-states", true)]);
        let p = hold_plan(&[], &["pocket-saves".to_string()], &c);
        assert_eq!((p.pause.len(), p.resume.as_slice()), (0, &["pocket-saves".to_string()][..]));
        // A switch to the next game before the end's release came through: kept paused, nothing sent twice.
        let p = hold_plan(&held_for(&on(), "snes"), &["pocket-saves".to_string()], &c);
        assert!(p.pause.is_empty() && p.resume.is_empty(), "{p:?}");
        // The scan after the game sees them only once they are resumed (`run_after_game` resumes first).
        assert_eq!(decide(&on(), Some(&c), game(), || true), Decision::Skip(Skip::NothingShared));
    }

    #[test]
    fn after_a_ps2_game_its_memory_cards_are_scanned_too() {
        let c = with_folders(&[("pocket-saves", false), ("pocket-states", false), ("pocket-ps2-cards", false)]);
        let ico = Path::new("/storage/roms/ps2/Ico (USA).iso");
        assert_eq!(
            decide(&on(), Some(&c), ico, || true),
            Decision::Scan(vec![
                req("pocket-saves", "ps2"),
                req("pocket-states", "ps2"),
                ScanReq {
                    folder: "pocket-ps2-cards".into(),
                    sub: None
                }
            ])
        );
        // A GBA game leaves the memory cards alone; with the row off they are not scanned either.
        assert_eq!(
            decide(&on(), Some(&c), game(), || true),
            Decision::Scan(vec![req("pocket-saves", "gba"), req("pocket-states", "gba")])
        );
        assert_eq!(
            decide(&Prefs { ps2: false, ..on() }, Some(&c), ico, || true),
            Decision::Scan(vec![req("pocket-saves", "ps2"), req("pocket-states", "ps2")])
        );
    }

    #[test]
    fn a_folder_the_emulator_has_not_made_yet_waits_for_it() {
        // ROCKNIX's start scripts copy Dolphin's files into /storage/roms/bios/GC only when that folder is missing:
        // making it first would leave Dolphin without them. So such folders are added once they exist.
        let c = with_folders(&[("pocket-saves", false), ("pocket-states", false)]);
        let missing: Vec<&str> = missing_dirs(&on(), &c, |_| false).iter().map(|d| d.id).collect();
        assert!(missing.is_empty(), "{missing:?}");
        let missing: Vec<&str> = missing_dirs(&on(), &c, |p| p == "/storage/roms/bios/GC" || p == "/storage/roms/ps2")
            .iter()
            .map(|d| d.id)
            .collect();
        assert_eq!(missing, ["pocket-ps2-cards", "pocket-gc-cards"]);
        let missing: Vec<&str> = missing_dirs(
            &Prefs {
                ps2: false,
                dolphin: false,
                ..on()
            },
            &c,
            |_| true,
        )
        .iter()
        .map(|d| d.id)
        .collect();
        assert_eq!(missing, ["pocket-3ds-saves"], "rows switched off are not added");
        assert!(Share::ALL
            .iter()
            .flat_map(|s| s.dirs())
            .filter(|d| d.wait_for_root)
            .all(|d| d.id != "pocket-saves" && d.id != "pocket-states"));
    }

    #[test]
    fn a_stignore_the_user_wrote_is_left_alone() {
        let dir = TestDir::new("sync-ignore");
        write_saves_ignore(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(".stignore")).unwrap(), saves_ignore());
        std::fs::write(dir.join(".stignore"), "mine\n").unwrap();
        write_saves_ignore(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(".stignore")).unwrap(), "mine\n");
    }
}
