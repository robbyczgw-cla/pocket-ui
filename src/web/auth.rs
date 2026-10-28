//! Who may use the companion: paired browsers (a six-digit code shown on the Pocket, then a session cookie), the
//! owner's tailnet devices (`net::owner_allows`), and the CSRF token every write carries.
//!
//! - **Pairing** ([`Pairing`]): Settings → Connections → Web companion → "Pair a browser" makes a code from the kernel's random
//!   source, valid for five minutes and one browser. Five wrong tries end it; the server also waits a second after
//!   every wrong try, so the million codes can't be walked through.
//! - **Sessions** ([`Sessions`]): the cookie holds 32 random bytes; the Pocket keeps only their SHA-256 (in
//!   `<data>/web/browsers.json`, in a folder only root can read), with the browser's name and when it was paired and
//!   last seen. Revoking one in Settings deletes its entry: its cookie is worth nothing from then on.
//! - **CSRF** ([`Sessions::csrf`]): HMAC-SHA256 of the principal (a paired browser's id, or the tailnet node) under a
//!   key that never leaves the Pocket, sent by the page in `X-CSRF-Token`. A page of another site can't read it, and
//!   can't send the header without a CORS preflight the companion never allows.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The cookie's name.
pub const COOKIE: &str = "pocket_session";
/// How long a pairing code is valid.
pub const PAIRING_VALID: Duration = Duration::from_secs(5 * 60);
/// Wrong codes before the code is thrown away.
pub const PAIRING_TRIES: u8 = 5;
/// Paired browsers kept at most; pairing one more forgets the one unused longest.
pub const MAX_BROWSERS: usize = 16;
/// A paired browser is valid this long after its pairing, the same as its cookie (review F6); then it needs a new
/// code.
pub const SESSION_LIFETIME: Duration = Duration::from_secs(365 * 24 * 3600);
/// `last_seen` is written to the card at most this often.
const SEEN_EVERY: u64 = 3600;

/// The code on the Pocket's screen.
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
            let b = super::random_bytes::<4>()?;
            let v = u32::from_le_bytes(b);
            // 4 294 000 000 is a multiple of 1 000 000: no digit more likely than another.
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

/// What a try with a code gives.
#[derive(Debug, PartialEq)]
pub enum Tried {
    Paired,
    Wrong,
    /// No code is shown, or it ran out.
    NoCode,
}

/// Checks `typed` against the code on screen. A right code is used up; a wrong one costs a try.
pub fn try_code(pairing: &mut Option<Pairing>, typed: &str, now: Instant) -> Tried {
    let Some(p) = pairing.as_mut().filter(|p| !p.expired(now)) else {
        *pairing = None;
        return Tried::NoCode;
    };
    let typed: String = typed.chars().filter(|c| !c.is_whitespace()).collect();
    if ct_eq(typed.as_bytes(), p.code.as_bytes()) {
        *pairing = None;
        return Tried::Paired;
    }
    p.tries_left = p.tries_left.saturating_sub(1);
    if p.tries_left == 0 {
        *pairing = None;
    }
    Tried::Wrong
}

/// One paired browser.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Browser {
    /// A short public id, for the list on the Pocket and for revoking (not the secret).
    pub id: String,
    /// SHA-256 of the cookie's token, hex.
    pub hash: String,
    /// "Safari on iPhone".
    pub name: String,
    /// Unix seconds.
    pub paired: u64,
    pub last_seen: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct File {
    /// The CSRF key, hex.
    key: String,
    browsers: Vec<Browser>,
}

/// The paired browsers and the CSRF key.
#[derive(Debug)]
pub struct Sessions {
    file: PathBuf,
    /// None only when the kernel gave no random bytes: then no write is accepted.
    key: Option<[u8; 32]>,
    pub browsers: Vec<Browser>,
    /// When `last_seen` was last written.
    seen_written: u64,
}

fn now_secs() -> u64 {
    crate::recents::now_secs()
}

impl Sessions {
    /// The folder of the companion's secrets, readable by root only.
    pub fn dir(data_dir: &Path) -> PathBuf {
        data_dir.join("web")
    }

    pub fn load(data_dir: &Path) -> Sessions {
        let file = Self::dir(data_dir).join("browsers.json");
        let f: File = crate::fsutil::read_json_or_backup(&file).unwrap_or_default();
        let key = parse_key(&f.key).or_else(|| super::random_bytes::<32>().ok());
        let mut s = Sessions { file, key, browsers: f.browsers, seen_written: now_secs() };
        let before = s.browsers.len();
        s.browsers.retain(|b| !expired(b, now_secs()));
        if (f.key.is_empty() && s.key.is_some()) || s.browsers.len() != before {
            s.save();
        }
        s
    }

    fn save(&self) -> bool {
        if let Some(dir) = self.file.parent() {
            let _ = std::fs::create_dir_all(dir);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
            }
        }
        let f = File { key: self.key.map(|k| super::hex(&k)).unwrap_or_default(), browsers: self.browsers.clone() };
        match crate::fsutil::write_json(&self.file, &f) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("web: can't save the paired browsers: {e}");
                false
            }
        }
    }

    /// Pairs a browser: a new token (returned, for the cookie) whose hash is kept.
    pub fn pair(&mut self, name: &str) -> std::io::Result<String> {
        let token = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, super::random_bytes::<32>()?);
        let id = super::hex(&super::random_bytes::<4>()?);
        let now = now_secs();
        if self.browsers.len() >= MAX_BROWSERS {
            if let Some(oldest) = self.browsers.iter().enumerate().min_by_key(|(_, b)| b.last_seen).map(|(i, _)| i) {
                self.browsers.remove(oldest);
            }
        }
        self.browsers.push(Browser { id, hash: token_hash(&token), name: name.chars().take(60).collect(), paired: now, last_seen: now });
        if !self.save() {
            self.browsers.pop();
            return Err(std::io::Error::other("the SD card refused the write"));
        }
        Ok(token)
    }

    /// The browser this cookie belongs to; notes that it was seen (written now and then, not on every request).
    pub fn check(&mut self, token: &str) -> Option<Browser> {
        if token.len() < 40 || token.len() > 64 {
            return None;
        }
        let hash = token_hash(token);
        let i = self.browsers.iter().position(|b| ct_eq(b.hash.as_bytes(), hash.as_bytes()))?;
        let now = now_secs();
        if expired(&self.browsers[i], now) {
            self.browsers.remove(i);
            self.save();
            return None;
        }
        self.browsers[i].last_seen = now;
        if now.saturating_sub(self.seen_written) >= SEEN_EVERY {
            self.seen_written = now;
            self.save();
        }
        Some(self.browsers[i].clone())
    }

    /// Forgets a browser (Settings → Connections → Web companion → Paired browsers). False when there was none of that id.
    pub fn revoke(&mut self, id: &str) -> bool {
        let before = self.browsers.len();
        self.browsers.retain(|b| b.id != id);
        before != self.browsers.len() && self.save()
    }

    /// Forgets every browser.
    pub fn revoke_all(&mut self) -> bool {
        self.browsers.clear();
        self.save()
    }

    /// The CSRF token of a principal ("b:<browser id>" or "t:<tailnet node>").
    pub fn csrf(&self, principal: &str) -> Option<String> {
        self.key.map(|k| super::hex(&hmac_sha256(&k, principal.as_bytes())[..16]))
    }

    pub fn csrf_ok(&self, principal: &str, sent: Option<&str>) -> bool {
        match (sent, self.csrf(principal)) {
            (Some(s), Some(want)) => ct_eq(s.as_bytes(), want.as_bytes()),
            _ => false,
        }
    }
}

/// Paired longer ago than [`SESSION_LIFETIME`].
fn expired(b: &Browser, now: u64) -> bool {
    now.saturating_sub(b.paired) > SESSION_LIFETIME.as_secs()
}

fn parse_key(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(k)
}

pub fn token_hash(token: &str) -> String {
    super::hex(&Sha256::digest(token.as_bytes()))
}

/// HMAC-SHA256 (RFC 2104) on the sha2 crate already in the tree.
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.map(|b| b ^ 0x36));
    inner.update(msg);
    let mut outer = Sha256::new();
    outer.update(k.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// Comparison whose time does not depend on where the inputs differ.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A name for the list on the Pocket, from the User-Agent: "Safari on iPhone", "Chrome on Mac", "Firefox on Windows".
pub fn browser_name(user_agent: &str) -> String {
    let ua = user_agent;
    let browser = if ua.contains("Firefox/") || ua.contains("FxiOS/") {
        "Firefox"
    } else if ua.contains("Edg/") || ua.contains("EdgiOS/") {
        "Edge"
    } else if ua.contains("OPR/") {
        "Opera"
    } else if ua.contains("Chrome/") || ua.contains("CriOS/") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "A browser"
    };
    let system = if ua.contains("iPhone") {
        "iPhone"
    } else if ua.contains("iPad") {
        "iPad"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        "Mac"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("CrOS") {
        "Chromebook"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        return browser.to_string();
    };
    format!("{browser} on {system}")
}
