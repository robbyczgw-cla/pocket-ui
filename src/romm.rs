//! RomM as a source of games (Settings → Games → RomM, docs/user/romm.md, docs/ROMM.md): the games on the player's own RomM
//! server stand on the shelf beside the ones on the card. The first start of one downloads it, with its progress on screen;
//! from then on it is a file on the card like every other game.
//!
//! * **Pairing.** The player types the server's address once. Then RomM's own device flow (RomM 5.0 and newer): the Pocket
//!   asks for a code ([`device_init`]), shows it with a QR code of RomM's approval page, and asks again every few seconds
//!   ([`device_poll`]) until the player approved it on their phone. Older servers (4.8 and newer) pair with the 8-digit code
//!   of a Client API Token instead ([`exchange_code`]). Either way the Pocket keeps a token, never a password: `token` in
//!   `<data>/romm/`, readable by its owner only ([`crate::secret::write_private`]), never logged, never on a command line.
//! * **The list.** [`fetch_catalog`] reads the server's platforms and their games and keeps what this Pocket can play in
//!   `<data>/romm/catalog.json` ([`Catalog`]): RomM's id, our system, the file (or the files of a game in a folder), its
//!   name and size, the cover's address. Platforms we have no system for are counted, not listed.
//! * **The shelf.** [`shelf_games`] gives `library::load_with` the server's games of a system that are not on the card,
//!   each at the path its file will have once it is downloaded (`<system folder>/<file>`). So favourites, play history,
//!   hidden games and covers key it the same before and after the download, and nothing on the shelf moves when it lands.
//! * **The download.** [`download`]: a check of the free space first, then the file into a hidden folder beside where it
//!   goes (`<system folder>/.pocket-ui-romm/`), resumed where it stopped when it broke off, its size checked, and one
//!   rename into place that never replaces a file. A game in a folder (a disc's `.cue` and its tracks) downloads file by
//!   file into its folder, which is renamed into place when all are there.
//!
//! Every function here that talks to the server or touches a file blocks: a worker's job, never the UI thread. The HTTP is
//! `romm_net`'s: the system's curl on Linux, Android's own HTTPS and a small plain-HTTP client on Android.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

pub use crate::romm_net::{Answer, Net};

/// The folder under the data folder with the account, the token and the list.
pub const DIR: &str = "romm";
const ACCOUNT_FILE: &str = "account.json";
const TOKEN_FILE: &str = "token";
const CATALOG_FILE: &str = "catalog.json";
const DEVICE_ID_FILE: &str = "device-id";
/// Hidden folder in a system's folder where downloads grow before they are renamed into place.
pub const PARTS_DIR: &str = ".pocket-ui-romm";
/// What the Pocket asks RomM to allow: read the games, their platforms and files, and who it is signed in as.
pub const SCOPES: &[&str] = &["me.read", "roms.read", "platforms.read", "assets.read"];
/// Space left free on the card after a download: saves, states and screenshots still need room.
pub const RESERVE: u64 = 64 << 20;
/// Games asked for per request while the list is read.
const PAGE: usize = 500;
/// The most games the list keeps (a server with more shows the first ones by name and says so).
pub const MAX_GAMES: usize = 50_000;
/// Systems RomM names in its own way that `layouts::system_ids` doesn't know by those names.
const EXTRA_SLUGS: &[(&str, &str)] = &[
    ("neogeoaes", "neogeo"),
    ("neogeomvs", "neogeo"),
    ("neo-geo-pocket", "ngp"),
    ("neo-geo-pocket-color", "ngpc"),
    ("wonderswan-color", "wonderswancolor"),
    ("virtualboy", "virtualboy"),
    ("pokemon-mini", "pokemini"),
    ("turbografx-cd", "pcenginecd"),
    ("sfam", "sfc"),
    ("new-nintendo-3ds", "3ds"),
];
/// RomM platforms that never go on the shelf, whatever folder matches them.
const NEVER: &[&str] = &["switch", "switch-2", "win", "win3x", "win9x", "windows-apps", "android", "ios"];

// ---- words --------------------------------------------------------------------------------------------------------

/// Why something didn't work, in words for the player (the English line is looked up in the language tables).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// No answer: no network, the server is off, a wrong address.
    Unreachable,
    /// The address answered, but not as RomM does.
    NotRomm,
    /// 401: the token was revoked or has run out; pairing again helps.
    SignedOut,
    /// 403: the account may not do this.
    NoAccess,
    /// 404 for a game: RomM doesn't have it any more.
    Gone,
    /// The server has no device flow (older than 5.0): pair with a code.
    OldServer,
    /// The pairing code is wrong or ran out.
    BadCode,
    /// The approval was declined on the phone.
    Denied,
    /// The QR code's time ran out before it was approved.
    Expired,
    /// Not enough space: the bytes still needed.
    NoRoom(u64),
    /// The download stopped part way; the part stays and the next start goes on from there.
    Broke,
    /// B while it downloaded.
    Stopped,
    /// The file came out another size than RomM said.
    WrongSize,
    /// The card refused a write (read-only, a folder in the way).
    CantWrite,
    /// Any other error status from the server.
    Server(u16),
}

impl Failure {
    /// The line the player reads.
    pub fn words(&self) -> String {
        match self {
            // A handheld without Wi-Fi is never told to check it (tests/no_wifi_walk.rs): the line every network page says there.
            Failure::Unreachable => crate::device::wifi_or("Can't reach your RomM server. Check Wi-Fi and that the server is on.", crate::device::NEEDS_NETWORK).into(),
            Failure::NotRomm => "That address answers, but not as a RomM server. Check the address.".into(),
            Failure::SignedOut => "RomM doesn't accept this Pocket any more. Pair it again in Settings › Games › RomM.".into(),
            Failure::NoAccess => "Your RomM account may not do this. Ask the server's owner for access.".into(),
            Failure::Gone => "RomM doesn't have this game any more. Refresh the list in Settings › Games › RomM.".into(),
            Failure::OldServer => "This RomM is older than 5.0 and can't pair by QR code. Choose Pair with a code.".into(),
            Failure::BadCode => "RomM didn't accept that code. Codes work once and for 5 minutes: make a new one.".into(),
            Failure::Denied => "The pairing was declined in RomM.".into(),
            Failure::Expired => "The code ran out before it was approved. Try again.".into(),
            Failure::NoRoom(need) => format!("Not enough free space: this game needs {} more.", crate::storage_space::size(*need)),
            Failure::Broke => "The download stopped. Start the game again to go on from where it stopped.".into(),
            Failure::Stopped => "Download stopped. Start the game again to go on from where it stopped.".into(),
            Failure::WrongSize => "The download came out the wrong size. Start the game again to fetch it anew.".into(),
            Failure::CantWrite => "The game can't be written to the card. Check that the card isn't full or locked.".into(),
            Failure::Server(code) => format!("RomM answered with an error ({code}). Try again later."),
        }
    }

    /// What an HTTP status means here (`None`: it worked).
    pub fn of_status(status: u16) -> Option<Failure> {
        match status {
            200..=299 => None,
            0 => Some(Failure::Unreachable),
            401 => Some(Failure::SignedOut),
            403 => Some(Failure::NoAccess),
            404 => Some(Failure::Gone),
            code => Some(Failure::Server(code)),
        }
    }
}

// ---- the server's address -----------------------------------------------------------------------------------------

/// The address as typed ("192.0.2.10:8080", "romm.example.com", "https://games.home/romm/") as the origin the API is
/// under: a scheme (http when none was typed), no slash at the end, no `/api` or RomM page path. Err: the words why not.
pub fn normalize_server(typed: &str) -> Result<String, &'static str> {
    let t = typed.trim();
    if t.is_empty() {
        return Err("Type your RomM server's address.");
    }
    if t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("An address has no spaces.");
    }
    let (scheme, rest) = match t.split_once("://") {
        Some((s, r)) if s.eq_ignore_ascii_case("http") || s.eq_ignore_ascii_case("https") => (s.to_ascii_lowercase(), r),
        Some(_) => return Err("The address starts with http:// or https://."),
        None => ("http".to_string(), t),
    };
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let (host, path) = rest.split_once('/').map_or((rest, ""), |(h, p)| (h, p));
    if host.is_empty() || host.contains('@') {
        return Err("Type the server's name or address, e.g. 192.0.2.10:8080.");
    }
    let name = host.rsplit_once(':').map_or(host, |(h, port)| if port.chars().all(|c| c.is_ascii_digit()) && !h.ends_with(':') && !h.is_empty() { h } else { host });
    let name_ok = (name.starts_with('[') && name.ends_with(']')) || name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !name_ok {
        return Err("Type the server's name or address, e.g. 192.0.2.10:8080.");
    }
    // A path RomM's own pages or the API live under is not part of the origin.
    let mut path = path.trim_end_matches('/').to_string();
    for tail in ["api", "pair/device", "login", "setup"] {
        if path == tail {
            path.clear();
        } else if let Some(p) = path.strip_suffix(&format!("/{tail}")) {
            path = p.to_string();
        }
    }
    Ok(if path.is_empty() { format!("{scheme}://{}", host.to_ascii_lowercase()) } else { format!("{scheme}://{}/{path}", host.to_ascii_lowercase()) })
}

/// The address without its scheme, for the screen ("192.0.2.10:8080").
pub fn shown_server(server: &str) -> &str {
    server.trim_start_matches("https://").trim_start_matches("http://")
}

/// One path segment of a URL: letters, digits and `-._~` as they are, everything else as `%XX`.
pub fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// ---- the account --------------------------------------------------------------------------------------------------

/// The paired server, without the token (that is a file of its own).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// The origin, as [`normalize_server`] gives it.
    pub server: String,
    /// Who RomM says this Pocket is signed in as.
    #[serde(default)]
    pub user: String,
    /// RomM's id of this Pocket (the device flow registers one).
    #[serde(default)]
    pub device_id: Option<String>,
}

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DIR)
}

/// The paired account, if there is one with a token.
pub fn account(data_dir: &Path) -> Option<Account> {
    let text = std::fs::read_to_string(dir(data_dir).join(ACCOUNT_FILE)).ok()?;
    let a: Account = serde_json::from_str(&text).ok()?;
    (!a.server.is_empty() && token(data_dir).is_some()).then_some(a)
}

/// The token. Never printed, logged or put on a command line.
pub fn token(data_dir: &Path) -> Option<String> {
    crate::secret::load(&dir(data_dir).join(TOKEN_FILE)).filter(|t| t.chars().all(|c| c.is_ascii_graphic()))
}

/// Keeps a fresh pairing: the token readable by its owner only, then the account.
pub fn save_account(data_dir: &Path, account: &Account, token: &str) -> std::io::Result<()> {
    let d = dir(data_dir);
    std::fs::create_dir_all(&d)?;
    crate::secret::write_private(&d.join(TOKEN_FILE), token.as_bytes())?;
    let json = serde_json::to_vec_pretty(account).map_err(std::io::Error::other)?;
    crate::fsutil::write_atomic(&d.join(ACCOUNT_FILE), &json)
}

/// Unpairs: the token, the account and the list go; games already downloaded stay on the card, and so do half-finished
/// downloads' parts (removed with the rest of a system's hidden folder by [`forget_parts`]).
pub fn forget(data_dir: &Path) {
    let d = dir(data_dir);
    for f in [TOKEN_FILE, ACCOUNT_FILE, CATALOG_FILE] {
        let _ = std::fs::remove_file(d.join(f));
    }
    clear_cache();
}

/// Removes the hidden download folders in `system_dirs` (what an unpaired server's broken-off downloads left).
pub fn forget_parts(system_dirs: &[PathBuf]) {
    for d in system_dirs {
        let parts = d.join(PARTS_DIR);
        if parts.is_dir() {
            let _ = std::fs::remove_dir_all(parts);
        }
    }
}

/// This Pocket's own id for RomM's device flow: random, made once, kept with the account.
pub fn device_identifier(data_dir: &Path) -> String {
    let path = dir(data_dir).join(DEVICE_ID_FILE);
    if let Some(id) = std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string()).filter(|s| s.len() >= 16 && s.chars().all(|c| c.is_ascii_hexdigit())) {
        return id;
    }
    let mut bytes = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut bytes);
    }
    if bytes == [0u8; 16] {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        bytes = (t ^ (std::process::id() as u128) << 64).to_le_bytes();
    }
    let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let _ = std::fs::create_dir_all(dir(data_dir));
    let _ = crate::fsutil::write_atomic(&path, id.as_bytes());
    id
}

// ---- pairing ------------------------------------------------------------------------------------------------------

/// What the device flow's start gives: the code to show and the page to approve it at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pairing {
    pub device_code: String,
    /// "M9JXERPY": what the player sees and RomM's page asks for.
    pub user_code: String,
    /// The approval page with the code in it, for the QR code.
    pub url: String,
    /// Seconds between two questions.
    pub interval: u64,
    /// Seconds until the code runs out.
    pub expires_in: u64,
}

#[derive(Deserialize)]
struct InitReply {
    device_code: String,
    user_code: String,
    #[serde(default)]
    verification_path_complete: String,
    #[serde(default)]
    verification_path: String,
    #[serde(default)]
    interval: u64,
    #[serde(default)]
    expires_in: u64,
}

fn api(server: &str, path: &str) -> String {
    format!("{server}{path}")
}

/// The JSON body of an answer, or the failure its status means.
fn json_of<T: serde::de::DeserializeOwned>(a: &Answer) -> Result<T, Failure> {
    if let Some(f) = Failure::of_status(a.status) {
        return Err(f);
    }
    serde_json::from_slice(&a.body).map_err(|_| Failure::NotRomm)
}

/// Starts RomM's device flow for this Pocket (`POST /api/auth/device/init`, open, no account needed).
pub fn device_init(net: &dyn Net, server: &str, identifier: &str, name: &str) -> Result<Pairing, Failure> {
    let body = serde_json::json!({
        "client_device_identifier": identifier,
        "name": name,
        "client": "pocket-ui",
        "platform": crate::romm_net::PLATFORM,
        "client_version": crate::romm_net::version(),
        "requested_scopes": SCOPES,
    })
    .to_string();
    let a = net.call("POST", &api(server, "/api/auth/device/init"), None, Some(&body));
    // No such route (RomM before 5.0), or a page that isn't the API (a web server's own 404 or 405).
    if matches!(a.status, 404 | 405) {
        return Err(if heartbeat(net, server).is_ok() { Failure::OldServer } else { Failure::NotRomm });
    }
    let r: InitReply = json_of(&a)?;
    let path = if r.verification_path_complete.is_empty() {
        format!("{}?user_code={}", if r.verification_path.is_empty() { "/pair/device" } else { &r.verification_path }, r.user_code)
    } else {
        r.verification_path_complete
    };
    if r.device_code.is_empty() || r.user_code.is_empty() || !path.starts_with('/') {
        return Err(Failure::NotRomm);
    }
    Ok(Pairing { device_code: r.device_code, user_code: r.user_code, url: format!("{server}{path}"), interval: r.interval.clamp(2, 30), expires_in: r.expires_in.clamp(30, 3600) })
}

/// What a question during the device flow found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Poll {
    /// Not approved yet: ask again after the interval.
    Waiting,
    /// Asked too often: wait longer.
    SlowDown,
    Approved { token: String, device_id: Option<String> },
    Failed(Failure),
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    #[serde(default)]
    device_id: Option<String>,
}

#[derive(Deserialize)]
struct Detail {
    #[serde(default)]
    detail: serde_json::Value,
}

/// Asks whether the player approved the code yet (`POST /api/auth/device/token`).
pub fn device_poll(net: &dyn Net, server: &str, device_code: &str) -> Poll {
    let body = serde_json::json!({ "device_code": device_code }).to_string();
    let a = net.call("POST", &api(server, "/api/auth/device/token"), None, Some(&body));
    if a.status == 200 {
        return match serde_json::from_slice::<TokenReply>(&a.body) {
            Ok(r) if valid_token(&r.access_token) => Poll::Approved { token: r.access_token, device_id: r.device_id },
            _ => Poll::Failed(Failure::NotRomm),
        };
    }
    if a.status == 0 {
        // The network went for a moment: the next question may get through.
        return Poll::Waiting;
    }
    let detail = serde_json::from_slice::<Detail>(&a.body).ok().and_then(|d| d.detail.as_str().map(str::to_string)).unwrap_or_default();
    match detail.as_str() {
        "authorization_pending" => Poll::Waiting,
        "slow_down" => Poll::SlowDown,
        "access_denied" => Poll::Failed(Failure::Denied),
        "expired_token" => Poll::Failed(Failure::Expired),
        _ if a.status == 429 => Poll::SlowDown,
        _ => Poll::Failed(Failure::of_status(a.status).unwrap_or(Failure::NotRomm)),
    }
}

#[derive(Deserialize)]
struct ExchangeReply {
    raw_token: String,
    #[serde(default)]
    device_id: Option<String>,
}

/// The 8 digits of a Client API Token's pairing (RomM 4.8 and newer: Client API Tokens → Pair) for its token.
pub fn exchange_code(net: &dyn Net, server: &str, code: &str) -> Result<(String, Option<String>), Failure> {
    let code: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if code.len() < 6 {
        return Err(Failure::BadCode);
    }
    let body = serde_json::json!({ "code": code }).to_string();
    let a = net.call("POST", &api(server, "/api/client-tokens/exchange"), None, Some(&body));
    match a.status {
        200 => match serde_json::from_slice::<ExchangeReply>(&a.body) {
            Ok(r) if valid_token(&r.raw_token) => Ok((r.raw_token, r.device_id)),
            _ => Err(Failure::NotRomm),
        },
        400 | 401 | 404 | 410 | 422 => Err(if heartbeat(net, server).is_ok() { Failure::BadCode } else { Failure::NotRomm }),
        s => Err(Failure::of_status(s).unwrap_or(Failure::NotRomm)),
    }
}

/// A token as RomM makes them: printable, no spaces, not absurdly long.
fn valid_token(t: &str) -> bool {
    (16..=4096).contains(&t.len()) && t.chars().all(|c| c.is_ascii_graphic())
}

#[derive(Deserialize)]
struct Heartbeat {
    #[serde(rename = "SYSTEM")]
    system: HeartbeatSystem,
}

#[derive(Deserialize)]
struct HeartbeatSystem {
    #[serde(rename = "VERSION", default)]
    version: String,
}

/// RomM's version, which also proves the address is a RomM server (`GET /api/heartbeat`, open).
pub fn heartbeat(net: &dyn Net, server: &str) -> Result<String, Failure> {
    let a = net.call("GET", &api(server, "/api/heartbeat"), None, None);
    if a.status == 0 {
        return Err(Failure::Unreachable);
    }
    if a.status != 200 {
        return Err(Failure::NotRomm);
    }
    serde_json::from_slice::<Heartbeat>(&a.body).map(|h| h.system.version).map_err(|_| Failure::NotRomm)
}

#[derive(Deserialize)]
struct Me {
    #[serde(default)]
    username: String,
}

/// Who the token belongs to (`GET /api/users/me`); a token that no longer works says so here.
pub fn whoami(net: &dyn Net, server: &str, token: &str) -> Result<String, Failure> {
    let a = net.call("GET", &api(server, "/api/users/me"), Some(token), None);
    json_of::<Me>(&a).map(|m| m.username)
}

// ---- the list -----------------------------------------------------------------------------------------------------

/// One file of a game that is a folder on the server (a disc's sheet and tracks).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    /// RomM's id of the file (`file_ids` of the download).
    #[serde(default)]
    pub id: u64,
    pub name: String,
    pub size: u64,
}

/// A game of the server this Pocket can play.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// RomM's id of the game.
    pub id: u64,
    /// Our system's id ("gba").
    pub system: String,
    /// Its file's name on the server, or its folder's for a game of several files.
    pub file: String,
    /// The name RomM shows.
    pub name: String,
    /// Bytes in all.
    pub size: u64,
    /// A game of several files: those files (all in its folder), and the one that starts it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<String>,
    /// The cover's path on the server (`/assets/romm/resources/…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// RomM's description of the game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl Entry {
    /// Where the game is once it is downloaded, in its system's folder `dir`: the file, or the start file in its folder.
    pub fn path_in(&self, dir: &Path) -> PathBuf {
        match &self.launch {
            Some(start) => dir.join(&self.file).join(start),
            None => dir.join(&self.file),
        }
    }

    /// The game `path` is, by its names (system folder or not): `<file>`, or `<file>/<launch>`.
    fn is_at(&self, path: &Path) -> bool {
        let name = path.file_name().and_then(|n| n.to_str());
        match &self.launch {
            None => name == Some(self.file.as_str()),
            Some(start) => name == Some(start.as_str()) && path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) == Some(self.file.as_str()),
        }
    }
}

/// The server's games this Pocket can play, as the last refresh found them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub server: String,
    /// When it was read (seconds since 1970).
    pub fetched: u64,
    pub games: Vec<Entry>,
    /// Games of platforms this Pocket has no system for, or files it can't start: counted, not listed.
    #[serde(default)]
    pub skipped: usize,
    /// The server had more than [`MAX_GAMES`].
    #[serde(default)]
    pub cut: bool,
}

impl Catalog {
    /// The game of `system` at `path` (the shelf's path of a RomM game, before or after its download).
    pub fn find(&self, system: &str, path: &Path) -> Option<&Entry> {
        self.games.iter().find(|e| e.system == system && e.is_at(path))
    }

    /// Games per system, for the page ("12 games in 3 systems").
    pub fn systems(&self) -> usize {
        let mut s: Vec<&str> = self.games.iter().map(|e| e.system.as_str()).collect();
        s.sort_unstable();
        s.dedup();
        s.len()
    }
}

#[derive(Deserialize)]
struct PlatformJson {
    id: u64,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    fs_slug: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    rom_count: u64,
}

#[derive(Deserialize)]
struct RomPage {
    #[serde(default)]
    items: Vec<RomJson>,
    #[serde(default)]
    total: Option<u64>,
}

#[derive(Deserialize)]
struct RomJson {
    id: u64,
    #[serde(default)]
    fs_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    fs_size_bytes: u64,
    #[serde(default)]
    has_multiple_files: bool,
    #[serde(default)]
    missing_from_fs: bool,
    #[serde(default)]
    path_cover_large: Option<String>,
    #[serde(default)]
    path_cover_small: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    files: Vec<FileJson>,
}

#[derive(Deserialize)]
struct FileJson {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    file_path: String,
    #[serde(default)]
    file_size_bytes: u64,
}

/// Our system for a RomM platform, among the systems of this Pocket (`library::all_systems`).
pub fn system_for(slug: &str, fs_slug: &str, name: &str, systems: &[crate::library::SysInfo]) -> Option<String> {
    if NEVER.contains(&slug) || NEVER.contains(&fs_slug) {
        return None;
    }
    if let Some((_, id)) = EXTRA_SLUGS.iter().find(|(s, _)| *s == slug || *s == fs_slug) {
        if let Some(s) = systems.iter().find(|s| s.id == *id) {
            return Some(s.id.clone());
        }
    }
    [fs_slug, slug, name].iter().filter(|n| !n.is_empty()).find_map(|n| crate::library::system_for_folder(n, systems)).map(|s| s.id.clone())
}

/// The file of a game of several files that starts it: a playlist, then a disc sheet, then a disc image, then the first
/// file the system takes.
fn start_file(parts: &[Part], exts: &[String]) -> Option<String> {
    let ext = |p: &Part| p.name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    for want in ["m3u", "cue", "gdi", "ccd", "chd", "iso", "cdi", "pbp"] {
        if let Some(p) = parts.iter().find(|p| ext(p) == want && exts.iter().any(|e| e == want)) {
            return Some(p.name.clone());
        }
    }
    parts.iter().find(|p| exts.contains(&ext(p))).map(|p| p.name.clone())
}

/// A file name that is safe to make on the card: no folders, nothing hidden, nothing a FAT card refuses.
fn plain_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 255 && !n.starts_with('.') && !n.contains(['/', '\\', '\0', ':', '*', '?', '"', '<', '>', '|']) && n.trim() == n
}

/// One RomM game as a list entry, if this Pocket can start it.
fn entry_of(r: RomJson, system: &crate::library::SysInfo) -> Option<Entry> {
    if r.missing_from_fs || !plain_name(&r.fs_name) {
        return None;
    }
    let mut exts = system.extensions.clone();
    exts.extend(crate::hostsys::shelf_extensions(&system.id).iter().map(|e| e.to_string()));
    let name = r.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| crate::library::display_name(r.fs_name.rsplit_once('.').map_or(r.fs_name.as_str(), |(s, _)| s)));
    let cover = r.path_cover_large.or(r.path_cover_small).map(|c| c.split('?').next().unwrap_or_default().to_string()).filter(|c| c.starts_with('/'));
    let summary = r.summary.filter(|s| !s.trim().is_empty());
    if !r.has_multiple_files {
        let ext = r.fs_name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
        if !exts.contains(&ext) {
            return None;
        }
        return Some(Entry { id: r.id, system: system.id.clone(), file: r.fs_name, name, size: r.fs_size_bytes, parts: Vec::new(), launch: None, cover, summary });
    }
    // A folder: every file directly in it (a game with folders inside stays on the server for now).
    let folder = r.fs_name.clone();
    let mut parts = Vec::new();
    for f in r.files {
        let inside = f.file_path.trim_end_matches('/').rsplit('/').next() == Some(folder.as_str());
        if !inside || !plain_name(&f.file_name) {
            return None;
        }
        parts.push(Part { id: f.id, name: f.file_name, size: f.file_size_bytes });
    }
    let launch = start_file(&parts, &exts)?;
    let size = parts.iter().map(|p| p.size).sum();
    Some(Entry { id: r.id, system: system.id.clone(), file: folder, name, size, parts, launch: Some(launch), cover, summary })
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Reads the server's list (`GET /api/platforms`, then `GET /api/roms` page by page for each platform we have a system
/// for). `stop`: give up between two pages.
pub fn fetch_catalog(net: &dyn Net, server: &str, token: &str, systems: &[crate::library::SysInfo], stop: &AtomicBool) -> Result<Catalog, Failure> {
    let a = net.call("GET", &api(server, "/api/platforms"), Some(token), None);
    let platforms: Vec<PlatformJson> = json_of(&a)?;
    let mut cat = Catalog { server: server.to_string(), fetched: now_secs(), ..Catalog::default() };
    for p in platforms.iter().filter(|p| p.rom_count > 0) {
        let Some(id) = system_for(&p.slug, &p.fs_slug, &p.name, systems) else {
            cat.skipped += p.rom_count as usize;
            continue;
        };
        let Some(sys) = systems.iter().find(|s| s.id == id) else { continue };
        let mut offset = 0usize;
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err(Failure::Stopped);
            }
            let url = api(server, &format!("/api/roms?platform_ids={}&limit={PAGE}&offset={offset}&order_by=name&order_dir=asc&with_files=true&with_char_index=false&with_filter_values=false", p.id));
            let page: RomPage = json_of(&net.call("GET", &url, Some(token), None))?;
            let n = page.items.len();
            for r in page.items {
                match entry_of(r, sys) {
                    Some(e) if cat.games.len() < MAX_GAMES => cat.games.push(e),
                    Some(_) => cat.cut = true,
                    None => cat.skipped += 1,
                }
            }
            offset += n;
            if n == 0 || n < PAGE || page.total.is_some_and(|t| offset as u64 >= t) || cat.cut {
                break;
            }
        }
    }
    // Two RomM games with the same file in one system (two platforms that are one system here): the first is kept.
    let mut seen = std::collections::HashSet::new();
    let before = cat.games.len();
    cat.games.retain(|e| seen.insert((e.system.clone(), e.file.to_lowercase())));
    cat.skipped += before - cat.games.len();
    Ok(cat)
}

pub fn save_catalog(data_dir: &Path, cat: &Catalog) -> std::io::Result<()> {
    let json = serde_json::to_vec(cat).map_err(std::io::Error::other)?;
    crate::fsutil::write_atomic(&dir(data_dir).join(CATALOG_FILE), &json)?;
    clear_cache();
    Ok(())
}

/// The list as the last refresh kept it, for the account that is paired now (a list of another server is no list).
pub fn load_catalog(data_dir: &Path) -> Option<Catalog> {
    let account = account(data_dir)?;
    let text = std::fs::read(dir(data_dir).join(CATALOG_FILE)).ok()?;
    let cat: Catalog = serde_json::from_slice(&text).ok()?;
    (cat.server == account.server).then_some(cat)
}

// ---- the shelf ----------------------------------------------------------------------------------------------------

type Cached = Option<(PathBuf, Option<std::time::SystemTime>, Arc<Catalog>)>;
static CACHE: Mutex<Cached> = Mutex::new(None);

fn clear_cache() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The list for the shelf, read once per change of its file (the library's scan asks once per system).
pub fn shelf_catalog(data_dir: &Path) -> Option<Arc<Catalog>> {
    let file = dir(data_dir).join(CATALOG_FILE);
    let stamp = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    stamp?;
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((d, s, cat)) = cache.as_ref() {
        if d == data_dir && *s == stamp {
            return Some(cat.clone());
        }
    }
    let cat = Arc::new(load_catalog(data_dir)?);
    *cache = Some((data_dir.to_path_buf(), stamp, cat.clone()));
    Some(cat)
}

/// The server's games of `system` (whose folder is `dir`) that aren't among `local` (by file name, any case): each at the
/// path its download will have. Their names are RomM's.
pub fn shelf_games(data_dir: &Path, system: &str, dir: &Path, local: &[crate::library::Game]) -> Vec<crate::library::Game> {
    let Some(cat) = shelf_catalog(data_dir) else { return Vec::new() };
    let have: std::collections::HashSet<String> = local.iter().filter_map(|g| g.path.file_name()?.to_str().map(str::to_lowercase)).collect();
    let folders: std::collections::HashSet<String> = local.iter().filter_map(|g| g.path.parent()?.file_name()?.to_str().map(str::to_lowercase)).collect();
    cat.games
        .iter()
        .filter(|e| e.system == system)
        .filter(|e| match &e.launch {
            None => !have.contains(&e.file.to_lowercase()),
            Some(_) => !folders.contains(&e.file.to_lowercase()) && !dir.join(&e.file).exists(),
        })
        .map(|e| crate::library::Game { name: e.name.clone(), path: e.path_in(dir) })
        .collect()
}

/// RomM's names of the games of `system` by file name (lower case; a game in a folder by its start file), so a game on the
/// card that came from RomM keeps RomM's name and nothing on the shelf changes when its download lands.
pub fn names_in(data_dir: &Path, system: &str) -> HashMap<String, String> {
    let Some(cat) = shelf_catalog(data_dir) else { return HashMap::new() };
    cat.games.iter().filter(|e| e.system == system).map(|e| (e.launch.as_deref().unwrap_or(&e.file).to_lowercase(), e.name.clone())).collect()
}

// ---- downloads ----------------------------------------------------------------------------------------------------

/// A download's progress and its stop, shared between the worker and the screen.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub stop: AtomicBool,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        (self.done.load(Ordering::Relaxed) as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}

/// How a transfer of one file ended (`Net::fetch`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// The whole file is in the part.
    Done,
    /// An HTTP status other than 200/206 (no bytes were written for it).
    Status(u16),
    /// No answer at all.
    Unreachable,
    /// The connection broke part way.
    Broke,
    /// The card refused the write (full).
    NoSpace,
    /// The card refused the write (other).
    CantWrite,
    Stopped,
}

/// Downloads `entry` to `dest` (its path on the shelf, [`Entry::path_in`]), unless it is there already. The parts grow in
/// `<system folder>/.pocket-ui-romm/`, and the next try goes on from the bytes a broken-off one left.
pub fn download(net: &dyn Net, server: &str, token: &str, entry: &Entry, dest: &Path, progress: &Progress) -> Result<(), Failure> {
    // `dest` is the start file; the thing renamed into place is the game's file, or its folder.
    let target = if entry.launch.is_some() { dest.parent().map(Path::to_path_buf).ok_or(Failure::CantWrite)? } else { dest.to_path_buf() };
    if dest.exists() {
        return Ok(());
    }
    let system_dir = target.parent().ok_or(Failure::CantWrite)?;
    let parts_dir = system_dir.join(PARTS_DIR).join(entry.id.to_string());
    std::fs::create_dir_all(&parts_dir).map_err(|_| Failure::CantWrite)?;
    let files: Vec<(String, u64, u64)> = if entry.parts.is_empty() { vec![(entry.file.clone(), entry.size, 0)] } else { entry.parts.iter().map(|p| (p.name.clone(), p.size, p.id)).collect() };
    let have = |name: &str| std::fs::metadata(parts_dir.join(format!("{name}.part"))).map_or(0, |m| m.len());
    let total: u64 = files.iter().map(|(_, s, _)| s).sum();
    let already: u64 = files.iter().map(|(n, s, _)| have(n).min(*s)).sum();
    progress.total.store(total, Ordering::Relaxed);
    progress.done.store(already, Ordering::Relaxed);
    let need = total.saturating_sub(already) + RESERVE;
    if let Some((free, _)) = crate::storage_space::free_space(&parts_dir) {
        if free < need {
            return Err(Failure::NoRoom(need - free));
        }
    }
    let mut before = 0u64;
    for (name, size, file_id) in &files {
        let part = parts_dir.join(format!("{name}.part"));
        if have(name) > *size {
            let _ = std::fs::remove_file(&part);
        }
        if have(name) != *size || *size == 0 {
            let url = match &entry.launch {
                None => api(server, &format!("/api/roms/{}/content/{}", entry.id, encode_segment(name))),
                // One file of a game in a folder, as it is (the whole folder would come as a zip).
                Some(_) => api(server, &format!("/api/roms/{}/content/{}?file_ids={file_id}", entry.id, encode_segment(name))),
            };
            let base = before;
            let watch = |n: u64| progress.done.store(base + n, Ordering::Relaxed);
            let result = net.fetch(&url, token, &part, *size, &progress.stop, &watch);
            match result {
                Fetched::Done => {}
                Fetched::Status(416) if have(name) == *size => {}
                Fetched::Status(404) => return Err(Failure::Gone),
                Fetched::Status(s) => {
                    // A status's body never lands in the part, but a server that ignored the range may have.
                    return Err(Failure::of_status(s).unwrap_or(Failure::Broke));
                }
                Fetched::Unreachable => return Err(Failure::Unreachable),
                Fetched::Broke => return Err(Failure::Broke),
                Fetched::Stopped => return Err(Failure::Stopped),
                Fetched::NoSpace => {
                    let _ = std::fs::remove_file(&part);
                    return Err(Failure::NoRoom(size.saturating_sub(have(name)) + RESERVE));
                }
                Fetched::CantWrite => return Err(Failure::CantWrite),
            }
            if have(name) != *size {
                let _ = std::fs::remove_file(&part);
                return Err(Failure::WrongSize);
            }
        }
        before += size;
        progress.done.store(before, Ordering::Relaxed);
    }
    // All there: into place, never over a file that is there now.
    if target.exists() {
        return Ok(());
    }
    if entry.launch.is_none() {
        let part = parts_dir.join(format!("{}.part", entry.file));
        hard_rename(&part, &target)?;
    } else {
        for (name, _, _) in &files {
            let part = parts_dir.join(format!("{name}.part"));
            let into = parts_dir.join(name);
            std::fs::rename(&part, &into).map_err(|_| Failure::CantWrite)?;
        }
        hard_rename(&parts_dir, &target)?;
    }
    let _ = std::fs::remove_dir(&parts_dir);
    let _ = std::fs::remove_dir(system_dir.join(PARTS_DIR));
    Ok(())
}

/// Renames `from` to `to` only when nothing is at `to` (a file that appeared meanwhile is never replaced).
fn hard_rename(from: &Path, to: &Path) -> Result<(), Failure> {
    if to.symlink_metadata().is_ok() {
        return Err(Failure::CantWrite);
    }
    std::fs::rename(from, to).map_err(|_| Failure::CantWrite)
}

// ---- covers -------------------------------------------------------------------------------------------------------

/// RomM's cover of `entry` into the cover cache (`covers::cover_path`, PNG) when it has one and there is none yet. A
/// JPEG is turned into a PNG; any other picture is left (the box art download finds one by name then). True when one was
/// written.
pub fn fetch_cover(net: &dyn Net, server: &str, token: &str, data_dir: &Path, entry: &Entry, shelf_path: &Path) -> bool {
    let Some(cover) = &entry.cover else { return false };
    let dest = crate::covers::cover_path(data_dir, &entry.system, shelf_path);
    if dest.exists() {
        return false;
    }
    let path: String = cover.split('/').map(|s| if s.is_empty() { String::new() } else { encode_segment(&percent_decode(s)) }).collect::<Vec<_>>().join("/");
    // Through a file, as a game is: the same on every platform, and a picture is bytes, not text.
    let Some(dir) = dest.parent() else { return false };
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let tmp = dir.join(format!(".romm-{}.part", entry.id));
    let _ = std::fs::remove_file(&tmp);
    let got = net.fetch(&api(server, &path), token, &tmp, 0, &AtomicBool::new(false), &|_| ());
    let body = std::fs::read(&tmp).unwrap_or_default();
    let _ = std::fs::remove_file(&tmp);
    if got != Fetched::Done || body.len() > 8 << 20 {
        return false;
    }
    let png = if body.starts_with(b"\x89PNG\r\n\x1a\n") {
        body
    } else if body.starts_with(&[0xff, 0xd8, 0xff]) {
        match jpeg_to_png(&body) {
            Some(p) => p,
            None => return false,
        }
    } else {
        return false;
    };
    crate::fsutil::write_atomic(&dest, &png).is_ok()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn jpeg_to_png(jpeg: &[u8]) -> Option<Vec<u8>> {
    let mut d = jpeg_decoder::Decoder::new(std::io::Cursor::new(jpeg));
    let px = d.decode().ok()?;
    let info = d.info()?;
    let (w, h) = (info.width as u32, info.height as u32);
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let rgb: Vec<u8> = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => px,
        jpeg_decoder::PixelFormat::L8 => px.iter().flat_map(|g| [*g, *g, *g]).collect(),
        _ => return None,
    };
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgb).ok()?;
    }
    Some(out)
}

#[cfg(test)]
#[path = "romm_tests.rs"]
mod tests;
