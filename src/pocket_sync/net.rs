//! Pocket-to-Pocket sync, the connection (docs/SYNC-OWN.md, merge 2): pairing with a six-digit code, the service that
//! answers another Pocket, and one sync with a paired Pocket, applying the core's plan (`pocket_sync::plan`) with the
//! core's record functions. Plain std networking (no TLS, no extra crates), so the same code runs on ROCKNIX, the
//! Brick, dArkOS and Android. Nothing in the App calls it yet (the Sync page and the triggers come next).
//!
//! - **Pairing** ([`Pairing`]): the Pocket that shows the code accepts one Pocket per code, five tries, five minutes,
//!   a second's wait after a wrong try (the web companion's rules). Both keep a random 32-byte key ([`Peers`],
//!   `<data>/pocket-sync/peers.json`, readable by root only).
//! - **Every request and every answer is signed** (HMAC-SHA256 over method, path, body hash; the answer over the
//!   request's signature, its status and its body hash), so nobody without the key reads or writes a save. Not
//!   encrypted: saves are no secret, and the home Wi-Fi or Tailscale carries them. No clock is checked (the RG35XX Plus
//!   has none that survives a restart); a replayed request can only write over the exact version it was made for.
//! - **Only from home networks**: connections from other than private, link-local, loopback and Tailscale addresses
//!   are closed at once ([`peer_allowed`]).
//! - **Never while a game runs**: the service answers 423 to every write and reports `busy` in the index; a sync does
//!   not start (the caller's `busy`, pocket-ui's game state).
//! - **Nothing lost**: a save that is replaced goes to `<roms>/sync-backups/<date>/` first (the folder the "Two
//!   versions" page uses); writes go through a temporary file (`fsutil::write_atomic`); a write is only taken over the
//!   exact version the sender saw (409 otherwise); saves are never deleted by sync.
//! - **Finding Pockets** ([`discover`], [`answer_discovery`]): one UDP broadcast on the Wi-Fi asks who is there; paired
//!   Pockets are also tried at the address they had last time.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::log::Log;
use super::{
    answered, conflict_name, conflict_found, index, mark, offered, plan, received, safe_rel, same, sha256_hex, vv_parse,
    vv_str, Action, CrcCache, Entry, Layout, Versions,
};

/// The service's TCP port.
pub const PORT: u16 = 47811;
/// The UDP port of "who is there?" on the Wi-Fi.
pub const DISCOVERY_PORT: u16 = 47812;
/// How long a pairing code is valid.
pub const PAIRING_VALID: Duration = Duration::from_secs(5 * 60);
/// Wrong codes before the code is thrown away.
pub const PAIRING_TRIES: u8 = 5;
/// Paired Pockets kept at most.
pub const MAX_PEERS: usize = 16;
const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: u64 = 32 << 20;
const IO_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const DISCOVERY_ASK: &[u8] = b"pocket-sync 1 who";
/// At most this many saves per bundle (`/put-many`, `/get-many`), and about this many bytes.
const MAX_BATCH: usize = 64;
const BATCH_BYTES: usize = 8 << 20;

// ---------------------------------------------------------------------------------------------------------------
// This Pocket, the paired ones, pairing.

/// This Pocket: a random id (made once, `<data>/pocket-sync/device.json`) and the name other Pockets show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Me {
    pub id: String,
    pub name: String,
}

impl Me {
    /// The id made on the first call; `name` replaces the stored one when given.
    pub fn load_or_new(dir: &Path, name: Option<&str>) -> std::io::Result<Me> {
        let path = dir.join("device.json");
        let old: Option<Me> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| {
            let id = v["id"].as_str()?.to_string();
            super::valid_id(&id).then(|| Me { id, name: v["name"].as_str().unwrap_or("Pocket").to_string() })
        });
        let me = match (old, name) {
            (Some(m), None) => return Ok(m),
            (Some(m), Some(n)) if m.name == n => return Ok(m),
            (Some(m), Some(n)) => Me { id: m.id, name: clean_name(n) },
            (None, n) => Me { id: hex(&random::<8>()?).to_uppercase(), name: clean_name(n.unwrap_or("Pocket")) },
        };
        crate::fsutil::write_atomic(&path, json!({"id": me.id, "name": me.name}).to_string().as_bytes())?;
        Ok(me)
    }
}

/// A name from another Pocket is data: printable, at most 40 characters.
fn clean_name(n: &str) -> String {
    let n: String = n.chars().filter(|c| !c.is_control()).take(40).collect();
    if n.trim().is_empty() { "Pocket".to_string() } else { n.trim().to_string() }
}

/// A paired Pocket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub id: String,
    pub name: String,
    /// Where it answered last (`ip:port`).
    pub addr: String,
    pub key: [u8; 32],
}

/// The paired Pockets (`<data>/pocket-sync/peers.json`, root only: it holds the keys).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Peers(pub Vec<Peer>);

impl Peers {
    pub fn load(dir: &Path) -> Peers {
        let v: Value = std::fs::read(dir.join("peers.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let list = v["peers"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let id = p["id"].as_str()?.to_string();
                        let key = unhex(p["key"].as_str()?).and_then(|k| <[u8; 32]>::try_from(k).ok())?;
                        super::valid_id(&id).then(|| Peer {
                            id,
                            name: clean_name(p["name"].as_str().unwrap_or("")),
                            addr: p["addr"].as_str().unwrap_or("").to_string(),
                            key,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Peers(list)
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let list: Vec<Value> =
            self.0.iter().map(|p| json!({"id": p.id, "name": p.name, "addr": p.addr, "key": hex(&p.key)})).collect();
        let path = dir.join("peers.json");
        crate::fsutil::write_atomic(&path, json!({"peers": list}).to_string().as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Peer> {
        self.0.iter().find(|p| p.id == id)
    }

    /// Adds or replaces a Pocket (paired again: a new key). Over [`MAX_PEERS`], the oldest goes.
    pub fn put(&mut self, p: Peer) {
        self.0.retain(|q| q.id != p.id);
        self.0.push(p);
        if self.0.len() > MAX_PEERS {
            self.0.remove(0);
        }
    }

    /// "Forget this Pocket": its key is gone, its requests are refused from now on.
    pub fn forget(&mut self, id: &str) {
        self.0.retain(|p| p.id != id);
    }
}

/// The code on this Pocket's screen.
#[derive(Clone, Debug)]
pub struct Pairing {
    pub code: String,
    pub until: Instant,
    pub tries_left: u8,
}

impl Pairing {
    /// A new code: six digits, uniform (rejection sampling over the kernel's random bytes).
    pub fn new() -> std::io::Result<Pairing> {
        let code = loop {
            let v = u32::from_le_bytes(random::<4>()?);
            if v < 4_294_000_000 {
                break format!("{:06}", v % 1_000_000);
            }
        };
        Ok(Pairing { code, until: Instant::now() + PAIRING_VALID, tries_left: PAIRING_TRIES })
    }

    pub fn expired(&self, now: Instant) -> bool {
        now >= self.until || self.tries_left == 0
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The saves of this Pocket, guarded: the service and a sync started here share one record.

/// This Pocket's side of sync: where the saves are, its record, its CRC cache (`<data>/pocket-sync/`).
pub struct Store {
    pub dir: PathBuf,
    pub layout: Layout,
    pub me: Me,
    lock: Mutex<()>,
    /// The log of what came and went (`log.rs`), one writer at a time.
    log_lock: Mutex<()>,
    /// Saves other Pockets sent here since the App last asked (by Pocket id): the shelf line for them.
    received: Mutex<Vec<(String, Vec<String>)>>,
    /// Called when saves came that way (the App wakes up for its shelf line).
    on_received: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl Store {
    pub fn new(dir: PathBuf, layout: Layout, me: Me) -> Store {
        Store { dir, layout, me, lock: Mutex::new(()), log_lock: Mutex::new(()), received: Mutex::new(Vec::new()), on_received: Mutex::new(None) }
    }

    /// The log of what came and went (`log.rs`), as it is now.
    pub fn log(&self) -> Log {
        let _g = self.log_lock.lock().unwrap_or_else(|e| e.into_inner());
        Log::load(&self.dir)
    }

    /// A change to the log, read and written under its lock. A log that cannot be written only loses its news.
    pub fn note(&self, f: impl FnOnce(&mut Log)) {
        let _g = self.log_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut log = Log::load(&self.dir);
        f(&mut log);
        let _ = log.save(&self.dir);
    }

    /// Saves `peer` sent here: into the log, and waiting for the App's shelf line.
    fn note_received(&self, peer: &str, rels: Vec<String>) {
        if rels.is_empty() {
            return;
        }
        self.note(|l| l.received(peer, now_secs(), &rels));
        self.received.lock().unwrap_or_else(|e| e.into_inner()).push((peer.to_string(), rels));
        if let Some(f) = self.on_received.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            f();
        }
    }

    /// What to call when saves came from another Pocket's own sync ([`Store::take_received`] has them).
    pub fn on_received(&self, f: Box<dyn Fn() + Send>) {
        *self.on_received.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
    }

    /// The saves other Pockets sent here since the last call, by Pocket id.
    pub fn take_received(&self) -> Vec<(String, Vec<String>)> {
        std::mem::take(&mut *self.received.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Read-modify-write of the record, one at a time. A newer record than this pocket-ui knows is left alone: `None`.
    fn versions<T>(&self, f: impl FnOnce(&mut Versions) -> T) -> Option<T> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.dir.join("versions.json");
        // A lost history starts a new one under a new id (`Versions::open`); a card that does not give the record, or a
        // newer record, stops sync this time.
        let salt = random::<2>().map(|b| hex(&b).to_uppercase()).unwrap_or_default();
        let mut v = Versions::open(&path, &self.me.id, &salt).ok()?;
        let before = v.clone();
        let out = f(&mut v);
        if v != before {
            if let Err(e) = v.save(&path) {
                eprintln!("pocket-sync: record not written: {e}");
            }
        }
        Some(out)
    }

    /// This Pocket's index, its record brought up to date (`pocket_sync::index`). Empty when the record is newer.
    pub fn index(&self) -> Vec<Entry> {
        let crc_path = self.dir.join("crc-cache.json");
        let mut crcs = CrcCache::load(&crc_path);
        let before = crcs.clone();
        let out = self.versions(|v| index(&self.me.id, &self.layout, v, &mut crcs)).unwrap_or_default();
        if crcs != before {
            let _ = crcs.save(&crc_path);
        }
        out
    }

    /// The file of a save here (`Layout::file`: beside the game, on Android in the player's folder by the game's key).
    pub fn path(&self, rel: &str) -> Option<PathBuf> {
        safe_rel(rel)?;
        self.layout.file(rel)
    }

    fn sha_now(&self, rel: &str) -> Option<String> {
        std::fs::read(self.path(rel)?).ok().map(|b| sha256_hex(&b))
    }

    /// Writes a save, the old one to the backup folder first. The file time is the version's (for showing).
    fn replace(&self, rel: &str, bytes: &[u8], mtime: u64) -> std::io::Result<()> {
        let dest = self.path(rel).ok_or_else(|| std::io::Error::other("not a save path"))?;
        if let Ok(old) = std::fs::read(&dest) {
            let b = unique(self.backup_dir().join(rel));
            crate::fsutil::write_atomic(&b, &old)?;
        }
        if let Some(d) = dest.parent() {
            // Android: a game's first save makes its folder in the player's saves.
            std::fs::create_dir_all(d)?;
        }
        crate::fsutil::write_atomic(&dest, bytes)?;
        set_mtime(&dest, mtime);
        Ok(())
    }

    /// `<roms>/sync-backups/<YYYY-MM-DD>` (UTC; Android: the app's own folder): where replaced saves and answered copies go.
    pub fn backup_dir(&self) -> PathBuf {
        let (y, m, d) = crate::civil_date::civil((now() / 86_400) as i64);
        self.layout.backups().join(format!("{y:04}-{m:02}-{d:02}"))
    }

    /// The `.sync-conflict-` copies beside a save, with their hashes.
    fn copies(&self, rel: &str) -> Vec<(PathBuf, String)> {
        let Some(p) = self.path(rel) else { return Vec::new() };
        let (Some(dir), Some(name)) = (p.parent(), p.file_name().and_then(|n| n.to_str())) else { return Vec::new() };
        let (stem, ext) = name.rsplit_once('.').map_or((name, String::new()), |(s, e)| (s, format!(".{e}")));
        let start = format!("{stem}.sync-conflict-");
        let mut out: Vec<(PathBuf, String)> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_str()?.to_string();
                (n.starts_with(&start) && n.ends_with(&ext)).then(|| e.path())
            })
            .filter_map(|p| std::fs::read(&p).ok().map(|b| (p, sha256_hex(&b))))
            .collect();
        out.sort();
        out
    }

    /// Puts a version beside a save as a copy (the question here shows it); not twice with the same bytes.
    fn add_copy(&self, rel: &str, bytes: &[u8], mtime: u64, from: &str) -> std::io::Result<()> {
        let sha = sha256_hex(bytes);
        if self.copies(rel).iter().any(|(_, s)| *s == sha) {
            return Ok(());
        }
        let name = conflict_name(rel, mtime, from);
        let dir = self.layout.dir_of(rel).ok_or_else(|| std::io::Error::other("not a save path"))?;
        let dest = dir.join(Path::new(&name).file_name().ok_or_else(|| std::io::Error::other("not a save path"))?);
        crate::fsutil::write_atomic(&unique(dest.clone()), bytes)?;
        Ok(())
    }

    /// A question here was answered on another Pocket: its copies go to the backup folder, as after a choice.
    fn clear_copies(&self, rel: &str) {
        for (p, _) in self.copies(rel) {
            if let Some(name) = p.file_name() {
                let sub = Path::new(rel).parent().unwrap_or(Path::new(""));
                let b = unique(self.backup_dir().join(sub).join(name));
                if let Some(d) = b.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                if std::fs::rename(&p, &b).is_err() && std::fs::copy(&p, &b).is_ok() {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
    }

    /// The player chose in the question asked here and the kept save is in place (the page moved the others away):
    /// record it (`pocket_sync::chose`). Call [`Store::index`] before the page changes anything, so play before the
    /// choice is counted as play and the choice itself is not.
    pub fn chose(&self, rel: &str) {
        if let Some(sha) = self.sha_now(rel) {
            self.versions(|v| {
                let writer = v.writer(&self.me.id).to_string();
                if let Some(r) = v.0.get_mut(rel) {
                    super::chose(r, &sha, &writer);
                }
            });
        }
    }
}

fn unique(p: PathBuf) -> PathBuf {
    if std::fs::symlink_metadata(&p).is_err() {
        return p;
    }
    let stem = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..).map(|n| p.with_file_name(format!("{stem} ({n}){ext}"))).find(|q| std::fs::symlink_metadata(q).is_err()).unwrap_or(p)
}

fn set_mtime(p: &Path, unix: u64) {
    if unix == 0 {
        return;
    }
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(p) {
        let t = UNIX_EPOCH + Duration::from_secs(unix);
        let _ = f.set_times(std::fs::FileTimes::new().set_modified(t).set_accessed(t));
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

// ---------------------------------------------------------------------------------------------------------------
// Keys, signatures, hex.

fn random<const N: usize>() -> std::io::Result<[u8; N]> {
    let mut b = [0u8; N];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(b)
}

fn hmac(key: &[u8; 32], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    k[..32].copy_from_slice(key);
    let mut inner = Sha256::new();
    inner.update(k.map(|b| b ^ 0x36));
    inner.update(msg);
    let mut outer = Sha256::new();
    outer.update(k.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

fn eq_ct(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn sign_req(key: &[u8; 32], method: &str, target: &str, body: &[u8]) -> String {
    hex(&hmac(key, format!("{method}\n{target}\n{}", sha256_hex(body)).as_bytes()))
}

fn sign_resp(key: &[u8; 32], status: u16, req_sig: &str, body: &[u8]) -> String {
    hex(&hmac(key, format!("ANSWER\n{status}\n{req_sig}\n{}", sha256_hex(body)).as_bytes()))
}

/// Connections only from home networks: private, link-local, loopback, and Tailscale's shared address range (IPv4
/// and IPv6).
pub fn peer_allowed(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            let o = a.octets();
            a.is_private() || a.is_loopback() || a.is_link_local() || (o[0] == 100 && (o[1] & 0xc0) == 64)
        }
        IpAddr::V6(a) => {
            let s = a.segments();
            a.is_loopback() || (s[0] & 0xffc0) == 0xfe80 || (s[0] & 0xfe00) == 0xfc00
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// HTTP, the smallest that works: one request per connection.

struct Msg {
    first: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn read_msg(s: &TcpStream) -> std::io::Result<Msg> {
    let mut r = BufReader::new(s);
    let mut first = String::new();
    r.read_line(&mut first)?;
    let mut headers = BTreeMap::new();
    let mut head = first.len();
    loop {
        let mut l = String::new();
        let n = r.read_line(&mut l)?;
        head += n;
        if n == 0 || l == "\r\n" || l == "\n" {
            break;
        }
        if head > MAX_HEAD {
            return Err(std::io::Error::other("head too large"));
        }
        if let Some((k, v)) = l.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let len: u64 = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    if len > MAX_BODY {
        return Err(std::io::Error::other("body too large"));
    }
    let mut body = vec![0; len as usize];
    r.read_exact(&mut body)?;
    Ok(Msg { first: first.trim_end().to_string(), headers, body })
}

fn write_msg(s: &mut TcpStream, first: &str, headers: &[(&str, String)], body: &[u8]) -> std::io::Result<()> {
    let mut h = format!("{first}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (k, v) in headers {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("\r\n");
    s.write_all(h.as_bytes())?;
    s.write_all(body)
}

fn pct_encode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

fn pct_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn query(target: &str, name: &str) -> Option<String> {
    let q = target.split_once('?')?.1;
    q.split('&').find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == name).and_then(|(_, v)| pct_decode(v)))
}

// ---------------------------------------------------------------------------------------------------------------
// The service: another Pocket asks, this one answers.

/// What the service needs: the saves, the paired Pockets, the code on screen, whether a game runs.
pub struct Service {
    pub store: Arc<Store>,
    pub peers: Mutex<Peers>,
    /// The code shown on this Pocket (Settings → Sync → Pair a Pocket), if any.
    pub pairing: Mutex<Option<Pairing>>,
    /// A game runs here: nothing is written, the index says so.
    pub busy: Box<dyn Fn() -> bool + Send + Sync>,
    /// The port this Pocket's service listens on (told to a Pocket that pairs, so it can start a sync too).
    pub port: u16,
    /// Set by a pairing that just succeeded (the page shows "Paired with …").
    pub paired: Mutex<Option<String>>,
}

impl Service {
    pub fn new(store: Arc<Store>, busy: Box<dyn Fn() -> bool + Send + Sync>, port: u16) -> Service {
        let peers = Peers::load(&store.dir);
        Service { store, peers: Mutex::new(peers), pairing: Mutex::new(None), busy, port, paired: Mutex::new(None) }
    }

    fn peers(&self) -> std::sync::MutexGuard<'_, Peers> {
        self.peers.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// "Forget this Pocket": its key is gone from memory and from the card, so its requests are refused from now on
    /// (the service looks at the card again for a Pocket it doesn't know, so forgetting only in memory would not last).
    pub fn forget(&self, id: &str) -> std::io::Result<()> {
        let mut peers = self.peers();
        peers.forget(id);
        self.store.note(|l| l.forget(id));
        peers.save(&self.store.dir)
    }
}

/// Accepts connections until `stop` is set (checked at least every 200 ms). One connection at a time: a save sync is
/// a handful of small requests, and one at a time keeps the record simple.
pub fn serve(listener: TcpListener, svc: Arc<Service>, stop: Arc<AtomicBool>) {
    let _ = listener.set_nonblocking(true);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((s, from)) => {
                if !peer_allowed(from.ip()) {
                    continue;
                }
                let _ = s.set_nonblocking(false);
                let _ = s.set_read_timeout(Some(IO_TIMEOUT));
                let _ = s.set_write_timeout(Some(IO_TIMEOUT));
                handle_conn(s, from, &svc);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(200)),
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

fn handle_conn(mut s: TcpStream, from: SocketAddr, svc: &Service) {
    let Ok(m) = read_msg(&s) else {
        let _ = write_msg(&mut s, "HTTP/1.1 400 Bad", &[], b"");
        return;
    };
    let mut parts = m.first.split(' ');
    let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
    let path = target.split('?').next().unwrap_or("").to_string();
    if method == "POST" && path == "/pair" {
        let (status, body) = handle_pair(svc, &m.body, from);
        let _ = write_msg(&mut s, &format!("HTTP/1.1 {status} X"), &[], body.to_string().as_bytes());
        return;
    }
    // A Pocket this service doesn't know may have been paired since it started, by this Pocket asking another one
    // (`pair`, which stores the key on the card): look at the card once more before refusing.
    let peer = m.headers.get("x-pocket-id").and_then(|id| {
        let mut peers = svc.peers();
        if peers.get(id).is_none() {
            *peers = Peers::load(&svc.store.dir);
        }
        peers.get(id).cloned()
    });
    let sig = m.headers.get("x-pocket-sig").cloned().unwrap_or_default();
    let Some(peer) = peer.filter(|p| eq_ct(sig.as_bytes(), sign_req(&p.key, &method, &target, &m.body).as_bytes())) else {
        let _ = write_msg(&mut s, "HTTP/1.1 401 Unpaired", &[], b"");
        return;
    };
    // The address it reached us from is where it is now (its service port is the one it gave when pairing).
    let (status, extra, body) = handle(svc, &peer, &method, &path, &target, &m.body);
    let answer = sign_resp(&peer.key, status, &sig, &body);
    let mut headers = vec![("X-Pocket-Sig", answer)];
    headers.extend(extra);
    let _ = write_msg(&mut s, &format!("HTTP/1.1 {status} X"), &headers, &body);
}

fn handle_pair(svc: &Service, body: &[u8], from: SocketAddr) -> (u16, Value) {
    let mut pairing = svc.pairing.lock().unwrap_or_else(|e| e.into_inner());
    let Some(p) = pairing.as_mut().filter(|p| !p.expired(Instant::now())) else {
        return (403, json!({"error": "no code"}));
    };
    let v: Value = serde_json::from_slice(body).unwrap_or_default();
    if !eq_ct(v["code"].as_str().unwrap_or("").as_bytes(), p.code.as_bytes()) {
        p.tries_left = p.tries_left.saturating_sub(1);
        drop(pairing);
        std::thread::sleep(Duration::from_secs(1));
        return (403, json!({"error": "wrong code"}));
    }
    let id = v["id"].as_str().unwrap_or("").to_string();
    if !super::valid_id(&id) || id == svc.store.me.id {
        return (400, json!({"error": "id"}));
    }
    let Ok(key) = random::<32>() else { return (500, json!({"error": "random"})) };
    let port = v["port"].as_u64().and_then(|p| u16::try_from(p).ok()).unwrap_or(PORT);
    let name = clean_name(v["name"].as_str().unwrap_or(""));
    let peer = Peer { id, name: name.clone(), addr: SocketAddr::new(from.ip(), port).to_string(), key };
    let mut peers = svc.peers();
    peers.put(peer);
    if peers.save(&svc.store.dir).is_err() {
        return (500, json!({"error": "save"}));
    }
    // One code, one Pocket.
    *pairing = None;
    *svc.paired.lock().unwrap_or_else(|e| e.into_inner()) = Some(name);
    (200, json!({"id": svc.store.me.id, "name": svc.store.me.name, "key": hex(&key)}))
}

type Answer = (u16, Vec<(&'static str, String)>, Vec<u8>);

fn handle(svc: &Service, peer: &Peer, method: &str, path: &str, target: &str, body: &[u8]) -> Answer {
    let store = &svc.store;
    let busy = (svc.busy)();
    let write = method != "GET";
    if write && busy {
        return (423, vec![], b"a game is running".to_vec());
    }
    match (method, path) {
        ("GET", "/index") => {
            let idx = if busy { Vec::new() } else { store.index() };
            let v = json!({"id": store.me.id, "name": store.me.name, "busy": busy,
                           "saves": idx.iter().map(Entry::to_json).collect::<Vec<_>>()});
            (200, vec![], v.to_string().into_bytes())
        }
        ("GET", "/file") => {
            let Some(p) = query(target, "rel").and_then(|r| store.path(&r)) else { return (400, vec![], vec![]) };
            match std::fs::read(&p) {
                Ok(b) if b.len() as u64 <= super::MAX_SAVE => {
                    let mt = std::fs::metadata(&p)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_secs());
                    (200, vec![("X-Pocket-Mtime", mt.to_string())], b)
                }
                _ => (404, vec![], vec![]),
            }
        }
        // A save that contains ours (or one we have none of): it replaces ours, only over the version the sender saw.
        ("PUT", "/file") => {
            let (Some(rel), Some(e)) = (query(target, "rel"), entry_of(target, body)) else { return (400, vec![], vec![]) };
            let expect = query(target, "expect").unwrap_or_default();
            match store.versions(|v| put_one(store, v, &e, &rel, &expect, body)) {
                Some(Ok((rel, closed))) => {
                    if closed {
                        store.clear_copies(&rel);
                    }
                    store.note_received(&peer.id, vec![rel.clone()]);
                    (200, vec![], rel.into_bytes())
                }
                Some(Err(code)) => (code, vec![], vec![]),
                None => (409, vec![], b"newer record".to_vec()),
            }
        }
        // Many saves in one request (the first sync of a big library): each as a PUT, one record write for all.
        ("POST", "/put-many") => {
            let Some((head, parts)) = unbundle(body) else { return (400, vec![], vec![]) };
            let files = head["files"].as_array().cloned().unwrap_or_default();
            let res = store.versions(|v| {
                files
                    .iter()
                    .zip(&parts)
                    .map(|(f, bytes)| {
                        let (Some(rel), Some(t)) = (f["rel"].as_str(), f["target"].as_str()) else { return Err(400) };
                        let Some(e) = entry_of(t, bytes) else { return Err(400) };
                        put_one(store, v, &e, rel, f["expect"].as_str().unwrap_or("none"), bytes)
                    })
                    .collect::<Vec<_>>()
            });
            let Some(results) = res else { return (409, vec![], b"newer record".to_vec()) };
            let out: Vec<Value> = results
                .iter()
                .map(|r| match r {
                    Ok((rel, closed)) => {
                        if *closed {
                            store.clear_copies(rel);
                        }
                        json!({"status": 200, "rel": rel})
                    }
                    Err(code) => json!({"status": code}),
                })
                .collect();
            store.note_received(&peer.id, results.iter().filter_map(|r| r.as_ref().ok().map(|(rel, _)| rel.clone())).collect());
            (200, vec![], json!({"results": out}).to_string().into_bytes())
        }
        ("POST", "/get-many") => {
            let v: Value = serde_json::from_slice(body).unwrap_or_default();
            let rels: Vec<String> = v["rels"].as_array().map(|a| a.iter().filter_map(|r| r.as_str().map(str::to_string)).collect()).unwrap_or_default();
            let mut head = Vec::new();
            let mut parts: Vec<Vec<u8>> = Vec::new();
            for rel in rels.iter().take(MAX_BATCH) {
                let p = store.path(rel);
                let bytes = p.as_ref().and_then(|p| std::fs::read(p).ok()).filter(|b| b.len() as u64 <= super::MAX_SAVE);
                let mtime = p.as_ref().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
                head.push(json!({"rel": rel, "size": bytes.as_ref().map_or(0, Vec::len), "mtime": mtime, "found": bytes.is_some()}));
                parts.push(bytes.unwrap_or_default());
            }
            let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
            (200, vec![], bundle(&json!({"files": head}), &refs))
        }
        // The peer's version joins the question open here (Action::Offer); the answer is the mark for it.
        ("POST", "/offer") => {
            let (Some(rel), Some(e)) = (query(target, "rel"), entry_of(target, body)) else { return (400, vec![], vec![]) };
            let res = store.versions(|v| {
                let r = v.0.get_mut(&rel).filter(|r| r.asking.is_some())?;
                store.add_copy(&rel, body, e.mtime, &peer.id).ok()?;
                Some(offered(r, &e))
            });
            match res.flatten() {
                Some(whole) => (200, vec![], vv_str(&whole).into_bytes()),
                None => (409, vec![], vec![]),
            }
        }
        // A question found there shows our version: mark it.
        ("POST", "/mark") => {
            let v: Value = serde_json::from_slice(body).unwrap_or_default();
            let (Some(rel), Some(sha), Some(q)) = (v["rel"].as_str(), v["sha"].as_str(), v["q"].as_str().and_then(vv_parse)) else {
                return (400, vec![], vec![]);
            };
            let ok = store.versions(|vs| vs.0.get_mut(rel).is_some_and(|r| mark(r, sha, &q)));
            (if ok == Some(true) { 200 } else { 409 }, vec![], vec![])
        }
        // Same bytes there: take the union and the questions about these bytes.
        ("POST", "/same") => {
            let v: Value = serde_json::from_slice(body).unwrap_or_default();
            let (Some(rel), Some(e)) = (v["rel"].as_str().map(str::to_string), Entry::from_json(&v["entry"])) else {
                return (400, vec![], vec![]);
            };
            let res = store.versions(|vs| {
                let r = vs.0.get_mut(&rel).filter(|r| r.sha == e.sha)?;
                let before = r.clone();
                same(r, &e);
                Some(answered(Some(&before), r))
            });
            match res.flatten() {
                Some(closed) => {
                    if closed {
                        store.clear_copies(&rel);
                    }
                    (200, vec![], vec![])
                }
                None => (409, vec![], vec![]),
            }
        }
        _ => (404, vec![], vec![]),
    }
}

/// One save that contains ours (or one we have none of), under the record's lock: it replaces ours only over the
/// version the sender saw (`expect`, a hash or "none"). A save new here for a game we have under another name goes
/// beside our game file. Returns where it went and whether a question here closed by it.
fn put_one(store: &Store, v: &mut Versions, e: &Entry, rel: &str, expect: &str, bytes: &[u8]) -> Result<(String, bool), u16> {
    let mut rel = rel.to_string();
    if expect == "none" {
        let t = e.crc.and_then(|c| super::target(&store.layout, &mut CrcCache::load(&store.dir.join("crc-cache.json")), &e.system, &e.ext, c));
        if let Some(t) = t.filter(|t| store.path(t).is_some_and(|p| !p.exists())) {
            rel = t;
        }
    }
    // A save path, but no game for it here (Android): 404, the sender counts it as nowhere to go, not as failed.
    store.path(&rel).ok_or(if safe_rel(&rel).is_some() { 404u16 } else { 400 })?;
    if store.sha_now(&rel).unwrap_or_else(|| "none".into()) != expect {
        return Err(409);
    }
    // New here, but the card has it in other letters (one file on FAT): not over that one.
    if expect == "none" && super::collides(&store.layout, &rel) {
        return Err(409);
    }
    let before = v.0.get(&rel).cloned();
    store.replace(&rel, bytes, e.mtime).map_err(|_| 500u16)?;
    let r = received(before.as_ref(), e);
    let closed = answered(before.as_ref(), &r);
    v.0.insert(rel.clone(), r);
    Ok((rel, closed))
}

/// Several files in one body: the header's length, a newline, the JSON header (`files`, each with its `size`), then the
/// files' bytes one after the other.
fn bundle(head: &Value, parts: &[&[u8]]) -> Vec<u8> {
    let h = head.to_string();
    let mut out = format!("{}\n{h}", h.len()).into_bytes();
    for p in parts {
        out.extend_from_slice(p);
    }
    out
}

fn unbundle(body: &[u8]) -> Option<(Value, Vec<&[u8]>)> {
    let nl = body.iter().position(|&b| b == b'\n').filter(|&i| i <= 10)?;
    let len: usize = std::str::from_utf8(&body[..nl]).ok()?.parse().ok()?;
    let head: Value = serde_json::from_slice(body.get(nl + 1..nl + 1 + len)?).ok()?;
    let mut at = nl + 1 + len;
    let mut parts = Vec::new();
    for f in head["files"].as_array()?.iter().take(MAX_BATCH) {
        let n = usize::try_from(f["size"].as_u64()?).ok()?;
        parts.push(body.get(at..at.checked_add(n)?)?);
        at += n;
    }
    (at == body.len()).then_some((head, parts))
}

/// The entry a PUT or an offer describes: its vector, marks and time in the query, its bytes in the body.
fn entry_of(target: &str, body: &[u8]) -> Option<Entry> {
    let rel = query(target, "rel")?;
    let mut v = json!({"rel": rel, "sha": sha256_hex(body), "size": body.len(), "vv": query(target, "vv")?,
                       "mtime": query(target, "mtime").and_then(|m| m.parse::<u64>().ok()).unwrap_or(0)});
    if let Some(c) = query(target, "crc").and_then(|c| u32::from_str_radix(&c, 16).ok()) {
        v["crc"] = c.into();
    }
    if let Some(m) = query(target, "marks").filter(|m| !m.is_empty()) {
        v["marks"] = m.split(';').map(Value::from).collect();
    }
    Entry::from_json(&v)
}

fn entry_query(e: &Entry) -> String {
    let marks: Vec<String> = e.marks.iter().map(vv_str).collect();
    format!(
        "rel={}&vv={}&mtime={}&crc={}&marks={}",
        pct_encode(&e.rel),
        pct_encode(&vv_str(&e.vv)),
        e.mtime,
        e.crc.map_or(String::new(), |c| format!("{c:08x}")),
        pct_encode(&marks.join(";"))
    )
}

// ---------------------------------------------------------------------------------------------------------------
// The other side: this Pocket asks.

/// Why a sync with a Pocket did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncError {
    /// Not reachable (asleep, off, another network).
    Unreachable,
    /// A game runs here or there.
    Busy,
    /// It does not know this Pocket any more (forgotten there, or paired again elsewhere).
    Unpaired,
    /// An answer that is not signed with the key, or not understood.
    Bad,
}

/// What one sync did, for the line on the shelf.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub sent: Vec<String>,
    pub fetched: Vec<String>,
    /// Questions put here (a copy beside the save).
    pub asked: Vec<String>,
    /// Ours joined a question open there.
    pub offered: Vec<String>,
    pub waiting: usize,
    pub same: usize,
    pub failed: Vec<String>,
    /// Saves of games the other side doesn't have (Android keeps a save only by its game): nothing failed, nothing to do.
    pub no_place: usize,
}

struct Conn<'a> {
    peer: &'a Peer,
    me: &'a str,
}

impl Conn<'_> {
    fn call(&self, method: &str, target: &str, body: &[u8]) -> Result<(u16, BTreeMap<String, String>, Vec<u8>), SyncError> {
        let addr: SocketAddr = self.peer.addr.parse().map_err(|_| SyncError::Unreachable)?;
        let mut s = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|_| SyncError::Unreachable)?;
        let _ = s.set_read_timeout(Some(IO_TIMEOUT));
        let _ = s.set_write_timeout(Some(IO_TIMEOUT));
        let sig = sign_req(&self.peer.key, method, target, body);
        write_msg(&mut s, &format!("{method} {target} HTTP/1.1"), &[("X-Pocket-Id", self.me.to_string()), ("X-Pocket-Sig", sig.clone())], body)
            .map_err(|_| SyncError::Unreachable)?;
        let m = read_msg(&s).map_err(|_| SyncError::Unreachable)?;
        let status: u16 = m.first.split(' ').nth(1).and_then(|v| v.parse().ok()).ok_or(SyncError::Bad)?;
        if status == 401 {
            return Err(SyncError::Unpaired);
        }
        let answer = m.headers.get("x-pocket-sig").cloned().unwrap_or_default();
        if !eq_ct(answer.as_bytes(), sign_resp(&self.peer.key, status, &sig, &m.body).as_bytes()) {
            return Err(SyncError::Bad);
        }
        if status == 423 {
            return Err(SyncError::Busy);
        }
        Ok((status, m.headers, m.body))
    }
}

/// Pairs with the Pocket at `addr` that shows `code`; returns it (also stored in `peers`).
pub fn pair(store: &Store, peers: &mut Peers, addr: SocketAddr, code: &str, my_port: u16) -> Result<Peer, SyncError> {
    let body = json!({"code": code, "id": store.me.id, "name": store.me.name, "port": my_port}).to_string();
    let mut s = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).map_err(|_| SyncError::Unreachable)?;
    let _ = s.set_read_timeout(Some(IO_TIMEOUT));
    write_msg(&mut s, "POST /pair HTTP/1.1", &[], body.as_bytes()).map_err(|_| SyncError::Unreachable)?;
    let m = read_msg(&s).map_err(|_| SyncError::Unreachable)?;
    if !m.first.contains(" 200 ") {
        return Err(SyncError::Unpaired);
    }
    let v: Value = serde_json::from_slice(&m.body).map_err(|_| SyncError::Bad)?;
    let id = v["id"].as_str().filter(|i| super::valid_id(i)).ok_or(SyncError::Bad)?.to_string();
    let key = v["key"].as_str().and_then(unhex).and_then(|k| <[u8; 32]>::try_from(k).ok()).ok_or(SyncError::Bad)?;
    let peer = Peer { id, name: clean_name(v["name"].as_str().unwrap_or("")), addr: addr.to_string(), key };
    peers.put(peer.clone());
    peers.save(&store.dir).map_err(|_| SyncError::Bad)?;
    Ok(peer)
}

/// One sync with a paired Pocket: its index, the plan, then each step with the core's record functions. A step that
/// fails (the save changed meanwhile, the connection dropped) is left for the next sync; nothing half done stays.
/// How it ended, and each save that came or went, goes into the log (`log.rs`); not when a game runs here.
pub fn sync_with(store: &Store, peer: &Peer, busy: &dyn Fn() -> bool) -> Result<Report, SyncError> {
    if busy() {
        return Err(SyncError::Busy);
    }
    let r = sync_once(store, peer, busy);
    store.note(|l| l.synced(&peer.id, now_secs(), &r));
    r
}

fn sync_once(store: &Store, peer: &Peer, busy: &dyn Fn() -> bool) -> Result<Report, SyncError> {
    let c = Conn { peer, me: &store.me.id };
    let (st, _, body) = c.call("GET", "/index", b"")?;
    if st != 200 {
        return Err(SyncError::Bad);
    }
    let v: Value = serde_json::from_slice(&body).map_err(|_| SyncError::Bad)?;
    if v["busy"].as_bool() == Some(true) {
        return Err(SyncError::Busy);
    }
    let remote: Vec<Entry> = v["saves"].as_array().map(|a| a.iter().filter_map(Entry::from_json).collect()).unwrap_or_default();
    let local = store.index();
    let mut rep = Report::default();
    let steps = plan(&local, &remote);
    // Pushes and pulls go in bundles (a first sync can be hundreds of saves); the rest one by one.
    let (bulk, single): (Vec<_>, Vec<_>) =
        steps.into_iter().partition(|s| matches!(s.action, Action::Push { .. } | Action::Pull));
    let (pushes, pulls): (Vec<_>, Vec<_>) = bulk.into_iter().partition(|s| matches!(s.action, Action::Push { .. }));
    push_many(store, &c, &pushes, &mut rep, busy)?;
    pull_many(store, &c, &pulls, &mut rep, busy)?;
    for step in single {
        if busy() {
            break;
        }
        let name = step.local.as_ref().or(step.remote.as_ref()).map(|e| e.rel.clone()).unwrap_or_default();
        let res = apply(store, &c, &step);
        match res {
            Ok(()) => match step.action {
                Action::Same { .. } => rep.same += 1,
                Action::Push { .. } => rep.sent.push(name),
                Action::Pull => rep.fetched.push(name),
                Action::Conflict => rep.asked.push(name),
                Action::Offer => rep.offered.push(name),
                Action::Waiting { .. } => rep.waiting += 1,
            },
            Err(SyncError::Busy) => return Err(SyncError::Busy),
            Err(SyncError::Unreachable) => return Err(SyncError::Unreachable),
            Err(_) => rep.failed.push(name),
        }
    }
    Ok(rep)
}

/// Sends the saves that contain the peer's (or that it lacks), in bundles; the peer takes each only over the version
/// planned with.
fn push_many(store: &Store, c: &Conn, steps: &[super::Step], rep: &mut Report, busy: &dyn Fn() -> bool) -> Result<(), SyncError> {
    let mut queue: Vec<(Value, Vec<u8>, String)> = Vec::new();
    for s in steps {
        let (Some(l), Action::Push { expect }) = (s.local.as_ref(), &s.action) else { continue };
        let Some(bytes) = store.path(&l.rel).and_then(|p| std::fs::read(p).ok()).filter(|b| sha256_hex(b) == l.sha) else {
            rep.failed.push(l.rel.clone());
            continue;
        };
        let mut e = l.clone();
        if let Some(r) = s.remote.as_ref() {
            e.rel = r.rel.clone();
        }
        let head = json!({"rel": e.rel, "target": format!("/file?{}", entry_query(&e)), "expect": expect.as_deref().unwrap_or("none"),
                          "size": bytes.len()});
        queue.push((head, bytes, l.rel.clone()));
    }
    let mut start = 0;
    while start < queue.len() {
        if busy() {
            return Err(SyncError::Busy);
        }
        let mut end = start;
        let mut size = 0;
        while end < queue.len() && end - start < MAX_BATCH && (end == start || size + queue[end].1.len() <= BATCH_BYTES) {
            size += queue[end].1.len();
            end += 1;
        }
        let chunk = &queue[start..end];
        let head = json!({"files": chunk.iter().map(|q| q.0.clone()).collect::<Vec<_>>()});
        let parts: Vec<&[u8]> = chunk.iter().map(|q| q.1.as_slice()).collect();
        let (st, _, body) = c.call("POST", "/put-many", &bundle(&head, &parts))?;
        let v: Value = serde_json::from_slice(&body).map_err(|_| SyncError::Bad)?;
        let results = v["results"].as_array().filter(|r| st == 200 && r.len() == chunk.len()).ok_or(SyncError::Bad)?;
        for (q, r) in chunk.iter().zip(results) {
            if r["status"].as_u64() == Some(200) {
                rep.sent.push(q.2.clone());
            } else if r["status"].as_u64() == Some(404) {
                rep.no_place += 1;
            } else {
                rep.failed.push(q.2.clone());
            }
        }
        start = end;
    }
    Ok(())
}

/// Fetches the saves that contain ours (or that we lack), in bundles, and puts each in place under the record's lock,
/// only over the version planned with.
fn pull_many(store: &Store, c: &Conn, steps: &[super::Step], rep: &mut Report, busy: &dyn Fn() -> bool) -> Result<(), SyncError> {
    let mut crcs = CrcCache::load(&store.dir.join("crc-cache.json"));
    let wanted: Vec<(&Entry, Option<&Entry>, String)> = steps
        .iter()
        .filter_map(|s| {
            let r = s.remote.as_ref()?;
            let to = match &s.local {
                Some(l) => l.rel.clone(),
                None => r.crc.and_then(|crc| super::target(&store.layout, &mut crcs, &r.system, &r.ext, crc)).unwrap_or_else(|| r.rel.clone()),
            };
            // No game for it here (Android keeps saves by their game): it comes when the game does.
            if store.path(&to).is_none() {
                rep.no_place += 1;
                return None;
            }
            Some((r, s.local.as_ref(), to))
        })
        .collect();
    let mut start = 0;
    while start < wanted.len() {
        if busy() {
            return Err(SyncError::Busy);
        }
        let mut end = start;
        let mut size = 0u64;
        while end < wanted.len() && end - start < MAX_BATCH && (end == start || size + wanted[end].0.size <= BATCH_BYTES as u64) {
            size += wanted[end].0.size;
            end += 1;
        }
        let chunk = &wanted[start..end];
        let body = json!({"rels": chunk.iter().map(|w| w.0.rel.clone()).collect::<Vec<_>>()}).to_string();
        let (st, _, answer) = c.call("POST", "/get-many", body.as_bytes())?;
        let (head, parts) = unbundle(&answer).filter(|_| st == 200).ok_or(SyncError::Bad)?;
        let files = head["files"].as_array().filter(|f| f.len() == chunk.len()).ok_or(SyncError::Bad)?.clone();
        let mut closed = Vec::new();
        store.versions(|vs| {
            for ((r, l, to), (f, bytes)) in chunk.iter().zip(files.iter().zip(&parts)) {
                let fits = f["found"].as_bool() == Some(true) && sha256_hex(bytes) == r.sha;
                // Ours must still be the version planned with (a game could have just written it), and a save new
                // here never goes over a file the card has in other letters.
                let ours = store.sha_now(to) == l.map(|l| l.sha.clone()) && (l.is_some() || !super::collides(&store.layout, to));
                if !fits || !ours || store.replace(to, bytes, f["mtime"].as_u64().unwrap_or(r.mtime)).is_err() {
                    rep.failed.push(to.clone());
                    continue;
                }
                let before = vs.0.get(to).cloned();
                let rec = received(before.as_ref(), r);
                if answered(before.as_ref(), &rec) {
                    closed.push(to.clone());
                }
                vs.0.insert(to.clone(), rec);
                rep.fetched.push(to.clone());
            }
        });
        for rel in closed {
            store.clear_copies(&rel);
        }
        start = end;
    }
    let _ = crcs.save(&store.dir.join("crc-cache.json"));
    Ok(())
}

fn apply(store: &Store, c: &Conn, step: &super::Step) -> Result<(), SyncError> {
    let (l, r) = (step.local.as_ref(), step.remote.as_ref());
    match &step.action {
        Action::Waiting { .. } => Ok(()),
        Action::Same { .. } => {
            let (l, r) = (l.ok_or(SyncError::Bad)?, r.ok_or(SyncError::Bad)?);
            let differs = l.vv != r.vv || l.marks != r.marks || l.asking.is_some() || r.asking.is_some();
            if !differs {
                return Ok(());
            }
            let closed = store
                .versions(|vs| {
                    let rec = vs.0.get_mut(&l.rel).filter(|x| x.sha == l.sha)?;
                    let before = rec.clone();
                    same(rec, r);
                    Some(answered(Some(&before), rec))
                })
                .flatten();
            if closed == Some(true) {
                store.clear_copies(&l.rel);
            }
            let body = json!({"rel": r.rel, "entry": l.to_json()}).to_string();
            c.call("POST", "/same", body.as_bytes()).map(|_| ())
        }
        Action::Push { expect } => {
            let l = l.ok_or(SyncError::Bad)?;
            let bytes = std::fs::read(store.path(&l.rel).ok_or(SyncError::Bad)?).map_err(|_| SyncError::Bad)?;
            if sha256_hex(&bytes) != l.sha {
                return Err(SyncError::Bad);
            }
            let mut e = l.clone();
            // There: their file for this game, else ours by name (they place it beside their game file by CRC).
            if let Some(r) = r {
                e.rel = r.rel.clone();
            }
            let t = format!("/file?{}&expect={}", entry_query(&e), expect.as_deref().unwrap_or("none"));
            match c.call("PUT", &t, &bytes)?.0 {
                200 => Ok(()),
                _ => Err(SyncError::Bad),
            }
        }
        Action::Pull => {
            let r = r.ok_or(SyncError::Bad)?;
            let to = match l {
                Some(l) => l.rel.clone(),
                None => r
                    .crc
                    .and_then(|crc| super::target(&store.layout, &mut CrcCache::load(&store.dir.join("crc-cache.json")), &r.system, &r.ext, crc))
                    .unwrap_or_else(|| r.rel.clone()),
            };
            let (st, h, bytes) = c.call("GET", &format!("/file?rel={}", pct_encode(&r.rel)), b"")?;
            if st != 200 || sha256_hex(&bytes) != r.sha {
                return Err(SyncError::Bad);
            }
            let mtime = h.get("x-pocket-mtime").and_then(|m| m.parse().ok()).unwrap_or(r.mtime);
            let closed = store
                .versions(|vs| {
                    // Ours must still be the version planned with (a game could have just written it), and a save new
                    // here never goes over a file the card has in other letters.
                    if store.sha_now(&to) != l.map(|l| l.sha.clone()) || (l.is_none() && super::collides(&store.layout, &to)) {
                        return None;
                    }
                    let before = vs.0.get(&to).cloned();
                    store.replace(&to, &bytes, mtime).ok()?;
                    let rec = received(before.as_ref(), r);
                    let closed = answered(before.as_ref(), &rec);
                    vs.0.insert(to.clone(), rec);
                    Some(closed)
                })
                .flatten()
                .ok_or(SyncError::Bad)?;
            if closed {
                store.clear_copies(&to);
            }
            Ok(())
        }
        Action::Conflict => {
            let (l, r) = (l.ok_or(SyncError::Bad)?, r.ok_or(SyncError::Bad)?);
            let (st, h, bytes) = c.call("GET", &format!("/file?rel={}", pct_encode(&r.rel)), b"")?;
            if st != 200 || sha256_hex(&bytes) != r.sha {
                return Err(SyncError::Bad);
            }
            let mtime = h.get("x-pocket-mtime").and_then(|m| m.parse().ok()).unwrap_or(r.mtime);
            let whole = store
                .versions(|vs| {
                    if store.sha_now(&l.rel).as_deref() != Some(l.sha.as_str()) {
                        return None;
                    }
                    store.add_copy(&l.rel, &bytes, mtime, &c.peer.id).ok()?;
                    let (rec, whole) = conflict_found(vs.0.get(&l.rel), l, r);
                    vs.0.insert(l.rel.clone(), rec);
                    Some(whole)
                })
                .flatten()
                .ok_or(SyncError::Bad)?;
            let body = json!({"rel": r.rel, "sha": r.sha, "q": vv_str(&whole)}).to_string();
            c.call("POST", "/mark", body.as_bytes()).map(|_| ())
        }
        Action::Offer => {
            let (l, r) = (l.ok_or(SyncError::Bad)?, r.ok_or(SyncError::Bad)?);
            let bytes = std::fs::read(store.path(&l.rel).ok_or(SyncError::Bad)?).map_err(|_| SyncError::Bad)?;
            if sha256_hex(&bytes) != l.sha {
                return Err(SyncError::Bad);
            }
            let mut e = l.clone();
            e.rel = r.rel.clone();
            let (st, _, body) = c.call("POST", &format!("/offer?{}", entry_query(&e)), &bytes)?;
            let whole = (st == 200).then(|| std::str::from_utf8(&body).ok().and_then(vv_parse)).flatten().ok_or(SyncError::Bad)?;
            store.versions(|vs| {
                if let Some(rec) = vs.0.get_mut(&l.rel) {
                    mark(rec, &l.sha, &whole);
                }
            });
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Finding Pockets on the Wi-Fi.

/// A Pocket that answered "who is there?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub id: String,
    pub name: String,
    pub addr: SocketAddr,
    /// It shows a pairing code right now.
    pub pairing: bool,
}

/// Answers "who is there?" until `stop` (a thread of the service). `pairing`: whether a code is shown now.
pub fn answer_discovery(sock: UdpSocket, svc: Arc<Service>, stop: Arc<AtomicBool>) {
    let _ = sock.set_read_timeout(Some(Duration::from_millis(300)));
    let mut buf = [0u8; 64];
    while !stop.load(Ordering::Relaxed) {
        let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
        if &buf[..n] != DISCOVERY_ASK || !peer_allowed(from.ip()) {
            continue;
        }
        let pairing = svc.pairing.lock().map(|p| p.as_ref().is_some_and(|p| !p.expired(Instant::now()))).unwrap_or(false);
        let me = &svc.store.me;
        let answer = json!({"pocket-sync": 1, "id": me.id, "name": me.name, "port": svc.port, "pairing": pairing}).to_string();
        let _ = sock.send_to(answer.as_bytes(), from);
    }
}

/// Asks the Wi-Fi (broadcast to `to`, normally `255.255.255.255:47812`) who is there; collects answers for `wait`.
pub fn discover(to: SocketAddr, wait: Duration) -> Vec<Found> {
    discover_at(&[to], wait)
}

/// [`discover`] asking several addresses at once: the Wi-Fi's broadcast and each device of the tailnet (no broadcast
/// crosses Tailscale, so a Pocket at work and one at home find each other only so).
pub fn discover_at(to: &[SocketAddr], wait: Duration) -> Vec<Found> {
    let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)) else { return Vec::new() };
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(100)));
    let sent = to.iter().filter(|a| sock.send_to(DISCOVERY_ASK, a).is_ok()).count();
    if sent == 0 {
        return Vec::new();
    }
    let until = Instant::now() + wait;
    let mut out: Vec<Found> = Vec::new();
    let mut buf = [0u8; 512];
    while Instant::now() < until {
        let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
        let Ok(v) = serde_json::from_slice::<Value>(&buf[..n]) else { continue };
        let (Some(id), Some(port)) = (v["id"].as_str(), v["port"].as_u64().and_then(|p| u16::try_from(p).ok())) else { continue };
        if v["pocket-sync"].as_u64() != Some(1) || !super::valid_id(id) || out.iter().any(|f| f.id == id) {
            continue;
        }
        out.push(Found {
            id: id.to_string(),
            name: clean_name(v["name"].as_str().unwrap_or("")),
            addr: SocketAddr::new(from.ip(), port),
            pairing: v["pairing"].as_bool().unwrap_or(false),
        });
    }
    out
}

#[cfg(test)]
#[path = "net_tests.rs"]
mod tests;

/// Unix seconds now.
fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
