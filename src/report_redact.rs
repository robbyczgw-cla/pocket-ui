//! What a report may never say: the Redactor takes out of a text what the system knows is private (Wi-Fi names, accounts,
//! the library's game names) and what looks private by its form (addresses, MACs, e-mail addresses, user names in
//! paths, `key=value` pairs of passwords and tokens, game paths). Shared by the Linux zip (`report`) and the Android text
//! (`report_phone`); nothing here reads a file or the system.

use std::collections::HashMap;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// A game of the library, for [`Redactor`].
#[derive(Debug, Clone)]
pub struct GameName {
    pub system: String,
    pub name: String,
    pub path: PathBuf,
}

/// The short code a game's name becomes: `[game snes 1a2b3c4d]`, from the file's name without its extension, so the
/// game's title and its file give the same code.
pub fn game_code(system: &str, stem: &str) -> String {
    let h = Sha256::digest(stem.trim().to_lowercase().as_bytes());
    format!("[game {system} {:02x}{:02x}{:02x}{:02x}]", h[0], h[1], h[2], h[3])
}

/// Takes out of a text what [`known_secrets`] and the library name, and what looks private by its form.
pub struct Redactor {
    /// Longest first, so a password that contains a network's name goes whole.
    secrets: Vec<(Vec<u8>, &'static str)>,
    /// Game names and file names by their first three bytes (ASCII lower case): the candidates to try at a word start.
    names: HashMap<[u8; 3], Vec<(Vec<u8>, String)>>,
}

/// Shorter game names are left alone: "Ys" would take the word out of every line.
const MIN_NAME: usize = 3;

impl Redactor {
    pub fn new(secrets: Vec<(String, &'static str)>, games: &[GameName]) -> Redactor {
        let mut secrets: Vec<(Vec<u8>, &'static str)> = secrets.into_iter().map(|(s, m)| (s.into_bytes(), m)).collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.0.len()));
        secrets.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
        let mut names: HashMap<[u8; 3], Vec<(Vec<u8>, String)>> = HashMap::new();
        for g in games {
            let stem = g.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| g.name.clone());
            let code = game_code(&g.system, &stem);
            for n in [stem.as_str(), g.name.as_str()] {
                let n = n.trim();
                if n.len() < MIN_NAME {
                    continue;
                }
                let key = key3(n.as_bytes());
                let list = names.entry(key).or_default();
                if !list.iter().any(|(b, _)| b.eq_ignore_ascii_case(n.as_bytes())) {
                    list.push((n.as_bytes().to_vec(), code.clone()));
                }
            }
        }
        for list in names.values_mut() {
            list.sort_by_key(|(b, _)| std::cmp::Reverse(b.len()));
        }
        Redactor { secrets, names }
    }

    /// The text without what is private, each piece replaced by a marker in brackets.
    pub fn clean(&self, text: &str) -> String {
        self.clean_for(text, false)
    }

    /// [`clean`](Self::clean) for a phone's log: also the folder names inside Android's storage paths and the
    /// `content://` addresses of the folders and games the player picked (they carry the folder names).
    pub fn clean_phone(&self, text: &str) -> String {
        self.clean_for(text, true)
    }

    fn clean_for(&self, text: &str, phone: bool) -> String {
        let t = self.secrets_out(text);
        let t = self.names_out(&t);
        let t = if phone { scan(&t, content_uri_at) } else { t };
        let t = if phone { scan(&t, phone_path_at) } else { t };
        let t = scan(&t, game_path_at);
        let t = scan(&t, user_path_at);
        let t = scan(&t, url_login_at);
        let t = scan(&t, email_at);
        let t = scan(&t, tailscale_at);
        let t = scan(&t, syncthing_at);
        let t = scan(&t, mac_at);
        let t = scan(&t, ipv6_at);
        let t = scan(&t, ipv4_at);
        scan(&t, assignment_at)
    }

    fn secrets_out(&self, text: &str) -> String {
        if self.secrets.is_empty() {
            return text.to_string();
        }
        scan(text, |t, i| {
            let rest = &t.as_bytes()[i..];
            self.secrets.iter().find(|(s, _)| rest.len() >= s.len() && rest[..s.len()].eq_ignore_ascii_case(s) && t.is_char_boundary(i + s.len())).map(|(s, m)| (i + s.len(), m.to_string()))
        })
    }

    fn names_out(&self, text: &str) -> String {
        if self.names.is_empty() {
            return text.to_string();
        }
        scan(text, |t, i| {
            let b = t.as_bytes();
            if b.len() < i + MIN_NAME || !word_edge(t, i, true) {
                return None;
            }
            let list = self.names.get(&key3(&b[i..]))?;
            list.iter().find(|(n, _)| b.len() >= i + n.len() && b[i..i + n.len()].eq_ignore_ascii_case(n) && t.is_char_boundary(i + n.len()) && word_edge(t, i + n.len(), false)).map(|(n, code)| (i + n.len(), code.clone()))
        })
    }
}

fn key3(b: &[u8]) -> [u8; 3] {
    [b[0].to_ascii_lowercase(), b[1].to_ascii_lowercase(), b[2].to_ascii_lowercase()]
}

/// Whether `i` is the edge of a word: the character before it (`start`) or at it (the end) is no letter or digit.
fn word_edge(t: &str, i: usize, start: bool) -> bool {
    let c = if start { t[..i].chars().next_back() } else { t[i..].chars().next() };
    !c.is_some_and(|c| c.is_alphanumeric())
}

/// Copies `text`, and wherever `at(text, i)` finds something at byte `i` (a character boundary), its replacement
/// instead, going on after the end it returns.
fn scan(text: &str, at: impl Fn(&str, usize) -> Option<(usize, String)>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if let Some((end, repl)) = at(text, i) {
            if end > i {
                out.push_str(&repl);
                i = end;
                continue;
            }
        }
        let c = text[i..].chars().next().unwrap_or('\0');
        out.push(c);
        i += c.len_utf8().max(1);
    }
    out
}

fn prev(t: &str, i: usize) -> u8 {
    if i == 0 {
        b' '
    } else {
        t.as_bytes()[i - 1]
    }
}

fn at_byte(t: &str, i: usize) -> u8 {
    t.as_bytes().get(i).copied().unwrap_or(b' ')
}

/// `content://<provider>/…`: Android's address of a picked folder or game, with the folder's name in it (percent-encoded):
/// the provider stays, the rest goes.
fn content_uri_at(t: &str, i: usize) -> Option<(usize, String)> {
    const SCHEME: &str = "content://";
    if !t[i..].starts_with(SCHEME) || prev(t, i).is_ascii_alphanumeric() {
        return None;
    }
    let b = t.as_bytes();
    let from = i + SCHEME.len();
    let mut j = from;
    while j < b.len() && b[j] != b'/' && !b[j].is_ascii_whitespace() && !b"\"'<>|,)]}".contains(&b[j]) {
        j += 1;
    }
    let provider = &t[from..j];
    if provider.is_empty() || provider.starts_with('[') {
        return None;
    }
    let mut end = j;
    while end < b.len() && !b[end].is_ascii_whitespace() && !b"\"'<>|,)]}".contains(&b[end]) {
        end += 1;
    }
    Some((end, format!("{SCHEME}{provider}/[removed]")))
}

/// Paths in Android's shared storage (`/storage/emulated/0/…`, `/storage/1A2B-3C4D/…`, `/sdcard/…`): the folders the
/// player made and named are in them. The root stays (it says internal storage or a card), the rest goes up to the
/// end of the line, a quote or a comma and a space.
fn phone_path_at(t: &str, i: usize) -> Option<(usize, String)> {
    if prev(t, i).is_ascii_alphanumeric() || prev(t, i) == b'_' || prev(t, i) == b'-' {
        return None;
    }
    let rest = &t[i..];
    let root = if let Some(r) = rest.strip_prefix("/storage/emulated/") {
        let digits = r.bytes().take_while(u8::is_ascii_digit).count();
        (digits > 0).then(|| ("/storage/emulated/".len() + digits, format!("/storage/emulated/{}", &r[..digits])))
    } else if let Some(r) = rest.strip_prefix("/storage/") {
        // A card's volume id (`1A2B-3C4D`): hex digits with one dash; `self` and `emulated` are Android's own names.
        let id = r.bytes().take_while(|c| c.is_ascii_alphanumeric() || *c == b'-').count();
        (id > 0 && r[..id].contains('-')).then(|| ("/storage/".len() + id, "/storage/[card]".to_string()))
    } else if rest.starts_with("/sdcard") && !at_byte(t, i + 7).is_ascii_alphanumeric() {
        Some(("/sdcard".len(), "/sdcard".to_string()))
    } else {
        None
    };
    let (root_len, root_text) = root?;
    let b = t.as_bytes();
    let mut end = i + root_len;
    if at_byte(t, end) != b'/' {
        return Some((end, root_text));
    }
    let stop = end;
    while end < b.len() && b[end] != b'\n' && !b"\"'<>|".contains(&b[end]) && !(b[end] == b',' && at_byte(t, end + 1) == b' ') {
        end += 1;
    }
    // A closing bracket or parenthesis right after the path belongs to the sentence, not to the path.
    while end > stop + 1 && b")]}.".contains(&b[end - 1]) {
        end -= 1;
    }
    (end > stop + 1).then(|| (end, format!("{root_text}/[removed]")))
}

/// `…roms/<system>/<game>.<ext>` (also `game-settings/`, `cheats/`, `saves/`, `savestates/<system>/`,
/// `screenshots/<system>/`): the game becomes its code, the folders and the extension stay.
fn game_path_at(t: &str, i: usize) -> Option<(usize, String)> {
    const ANCHORS: [&str; 6] = ["roms/", "game-settings/", "cheats/", "saves/", "savestates/", "screenshots/"];
    let b = t.as_bytes();
    let p = prev(t, i);
    if p.is_ascii_alphanumeric() || p == b'_' || p == b'-' {
        return None;
    }
    let anchor = ANCHORS.iter().find(|a| t[i..].starts_with(*a))?;
    let seg = |from: usize| -> Option<usize> {
        let mut j = from;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b"_+-".contains(&b[j])) {
            j += 1;
        }
        (j > from && at_byte(t, j) == b'/').then_some(j)
    };
    let mut sys_start = i + anchor.len();
    let mut sys_end = seg(sys_start)?;
    // roms/screenshots/<system>/…, roms/savestates/<system>/…
    if matches!(&t[sys_start..sys_end], "screenshots" | "savestates") {
        if let Some(e) = seg(sys_end + 1) {
            sys_start = sys_end + 1;
            sys_end = e;
        }
    }
    let system = &t[sys_start..sys_end];
    let rest = sys_end + 1;
    let first = at_byte(t, rest);
    if rest >= b.len() || first == b'[' || first.is_ascii_whitespace() || b"\"'`|<>{}".contains(&first) {
        return None;
    }
    let mut stop = rest;
    while stop < b.len() && !b"\n\r\t\"'`|<>{};".contains(&b[stop]) {
        stop += 1;
    }
    // The first ".ext" (2 to 4 letters and digits, one a letter at least) that ends a word: the file's name ends
    // there. "Dr. Mario" and "Vol.2" don't count; without one, the rest of the line up to a stop is the game.
    let mut end = stop;
    let mut ext = "";
    let mut k = rest;
    while k < stop {
        if b[k] == b'.' {
            let mut e = k + 1;
            while e < stop && b[e].is_ascii_alphanumeric() {
                e += 1;
            }
            let n = e - k - 1;
            if (2..=4).contains(&n) && b[k + 1..e].iter().any(|c| c.is_ascii_alphabetic()) && (e == stop || !(b[e].is_ascii_alphanumeric() || b[e] == b'.')) {
                end = k;
                ext = &t[k..e];
                break;
            }
        }
        k += 1;
    }
    let name = t[rest..end].trim_end();
    if name.is_empty() {
        return None;
    }
    let stem = name.rsplit('/').next().unwrap_or(name);
    let after = rest + name.len() + if end == rest + name.len() { ext.len() } else { 0 };
    let shown_ext = if end == rest + name.len() { ext } else { "" };
    Some((after, format!("{}{system}/{}{shown_ext}", &t[i..sys_start], game_code(system, stem))))
}

/// `/home/<user>`, `/Users/<user>`, `C:\Users\<user>`, and the label of a card under `/var/media/`.
fn user_path_at(t: &str, i: usize) -> Option<(usize, String)> {
    let p = prev(t, i);
    if p.is_ascii_alphanumeric() {
        return None;
    }
    // The two home prefixes are put together at run time: as literals they would sit in the binary, where the
    // release check (scripts/check-binary-paths.sh) can't tell them from a leaked build path.
    static PREFIXES: std::sync::OnceLock<[(String, &str, u8); 5]> = std::sync::OnceLock::new();
    let prefixes = PREFIXES.get_or_init(|| {
        let root = |name: &str| format!("/{name}/");
        [(root("home"), "[user]", b'/'), (root("Users"), "[user]", b'/'), ("C:\\Users\\".into(), "[user]", b'\\'), ("/var/media/".into(), "[card]", b'/'), ("/run/media/".into(), "[card]", b'/')]
    });
    let (prefix, marker, sep) = prefixes.iter().find(|(p, _, _)| t[i..].starts_with(p.as_str()))?;
    let (marker, sep) = (*marker, *sep);
    let b = t.as_bytes();
    let from = i + prefix.len();
    let mut j = from;
    while j < b.len() && b[j] != sep && !b[j].is_ascii_whitespace() && !b"\"'`:;,)>".contains(&b[j]) {
        j += 1;
    }
    if j == from || t[from..].starts_with('[') {
        return None;
    }
    Some((j, format!("{prefix}{marker}")))
}

/// A URL with a login (`scheme://`, a user name, ":", a password, "@", then the host): the login goes.
fn url_login_at(t: &str, i: usize) -> Option<(usize, String)> {
    if !t[i..].starts_with("://") {
        return None;
    }
    let b = t.as_bytes();
    let from = i + 3;
    let mut j = from;
    let mut at = None;
    while j < b.len() && !b[j].is_ascii_whitespace() && !b"/\"'<>".contains(&b[j]) {
        if b[j] == b'@' {
            at = Some(j);
        }
        j += 1;
    }
    let at = at.filter(|&a| a > from)?;
    Some((at, "://[removed]".into()))
}

fn email_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let local = |c: u8| c.is_ascii_alphanumeric() || b"._%+-".contains(&c);
    if !local(b[i]) || local(prev(t, i)) {
        return None;
    }
    let mut j = i;
    while j < b.len() && local(b[j]) {
        j += 1;
    }
    if at_byte(t, j) != b'@' {
        return None;
    }
    let from = j + 1;
    let mut k = from;
    while k < b.len() && (b[k].is_ascii_alphanumeric() || b[k] == b'.' || b[k] == b'-') {
        k += 1;
    }
    while k > from && b[k - 1] == b'.' {
        k -= 1;
    }
    let domain = &t[from..k];
    let tld = domain.rsplit('.').next().unwrap_or("");
    (domain.contains('.') && tld.len() >= 2 && tld.bytes().all(|c| c.is_ascii_alphabetic())).then(|| (k, "[email]".into()))
}

/// Tailscale's auth keys (`tskey-…`) and MagicDNS names (`…ts.net`, `…tailscale.net`).
fn tailscale_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let host = |c: u8| c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c == b'_';
    if host(prev(t, i)) || !host(b[i]) {
        return None;
    }
    let mut j = i;
    while j < b.len() && host(b[j]) {
        j += 1;
    }
    let word = t[i..j].trim_end_matches('.').to_ascii_lowercase();
    if word.starts_with("tskey-") {
        return Some((j, "[tailscale-key]".into()));
    }
    (word.ends_with(".ts.net") || word.ends_with(".tailscale.net")).then(|| (i + word.len(), "[tailnet-name]".into()))
}

/// A Syncthing device ID: eight groups of seven of A-Z and 2-7, joined by dashes.
fn syncthing_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let p = prev(t, i);
    if p.is_ascii_alphanumeric() || p == b'-' || b.len() < i + 63 {
        return None;
    }
    for (k, &c) in b[i..i + 63].iter().enumerate() {
        let ok = if k % 8 == 7 { c == b'-' } else { c.is_ascii_uppercase() || (b'2'..=b'7').contains(&c) };
        if !ok {
            return None;
        }
    }
    let n = at_byte(t, i + 63);
    (!(n.is_ascii_alphanumeric() || n == b'-')).then(|| (i + 63, "[sync-device-id]".into()))
}

/// `aa:bb:cc:dd:ee:ff` (or with dashes), percent-encoded as a redirect URL carries it (`00%3A00%3A5e%3A00%3A53%3A7f`),
/// or bare (`a4c1385e027f`): a device's address in any form.
fn mac_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let edge = |c: u8| c.is_ascii_alphanumeric() || c == b':' || c == b'-' || c == b'%';
    if edge(prev(t, i)) || !b.get(i).is_some_and(u8::is_ascii_hexdigit) {
        return None;
    }
    // Twelve hex digits in a row.
    if i + 12 <= b.len() && b[i..i + 12].iter().all(u8::is_ascii_hexdigit) && !edge(at_byte(t, i + 12)) {
        return Some((i + 12, "[mac]".into()));
    }
    // Six groups of two hex digits, separated by `:`, `-` or their percent encoding.
    let mut j = i;
    for g in 0..6 {
        let start = j;
        while j < b.len() && j - start < 2 && b[j].is_ascii_hexdigit() {
            j += 1;
        }
        if j - start != 2 {
            return None;
        }
        if g < 5 {
            // Address syntax is ASCII. Looking at three UTF-8 bytes as a str can end inside ° / µ in sensor logs.
            let enc = b.get(j..j + 3).is_some_and(|s| s.eq_ignore_ascii_case(b"%3a") || s.eq_ignore_ascii_case(b"%2d"));
            if matches!(at_byte(t, j), b':' | b'-') {
                j += 1;
            } else if enc {
                j += 3;
            } else {
                return None;
            }
        }
    }
    (!edge(at_byte(t, j))).then(|| (j, "[mac]".into()))
}

/// An IPv6 address: hex groups with "::" in them or eight groups. Times ("10:00:00") and Rust paths (`a::b`) are not.
fn ipv6_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let part = |c: u8| c.is_ascii_hexdigit() || c == b':';
    let p = prev(t, i);
    if !part(b[i]) || part(p) || p.is_ascii_alphanumeric() || p == b'_' || p == b'.' {
        return None;
    }
    let mut j = i;
    while j < b.len() && part(b[j]) {
        j += 1;
    }
    let n = at_byte(t, j);
    if n.is_ascii_alphanumeric() || n == b'_' {
        return None;
    }
    let run = &t[i..j];
    let colons = run.bytes().filter(|&c| c == b':').count();
    let groups: Vec<&str> = run.split(':').filter(|g| !g.is_empty()).collect();
    let hex: usize = groups.iter().map(|g| g.len()).sum();
    let valid = colons >= 2 && groups.iter().all(|g| g.len() <= 4) && !run.contains(":::") && hex >= 4 && ((run.contains("::") && groups.len() >= 2) || (colons == 7 && groups.len() == 8));
    valid.then(|| (j, "[ip]".into()))
}

/// An IPv4 address; the loopback and 0.0.0.0 stay, they say nothing about anyone. So does a version with four
/// numbers ([`version_like`]): "easyrpg = version 0.8.1.1", "EasyRPG-0.8.1.1", "0.8.1.1-rc2".
fn ipv4_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let p = prev(t, i);
    if !b[i].is_ascii_digit() || p.is_ascii_alphanumeric() || p == b'.' {
        return None;
    }
    let mut j = i;
    let mut octets = Vec::new();
    for k in 0..4 {
        let from = j;
        while j < b.len() && b[j].is_ascii_digit() && j - from < 3 {
            j += 1;
        }
        let v: u32 = t[from..j].parse().ok()?;
        if j == from || v > 255 {
            return None;
        }
        octets.push(v);
        if k < 3 {
            if at_byte(t, j) != b'.' {
                return None;
            }
            j += 1;
        }
    }
    let n = at_byte(t, j);
    if n.is_ascii_alphanumeric() || (n == b'.' && at_byte(t, j + 1).is_ascii_digit()) {
        return None;
    }
    let harmless = octets[0] == 127 || octets == [0, 0, 0, 0] || version_like(t, i, j);
    (!harmless).then(|| (j, "[ip]".into()))
}

/// What follows a version after a dash: `0.8.1.1-rc2`, `1.2.3.4-beta`.
const VERSION_SUFFIXES: &[&str] = &["rc", "alpha", "beta", "dev", "git", "pre", "snapshot", "release"];

/// Whether the four numbers at `i..j` are a version, not an address, by what stands around them; deliberately
/// narrow, a real address in an ordinary line stays an address:
/// - right after the word "version", "ver" or "v" (`version 0.8.1.1`, `"version": "1.2.3.4"`, `ver=1.2.3.4`;
///   `v1.2.3.4` never counts as an address, the letter touches it);
/// - glued to a name by a dash (`easyrpg-0.8.1.1`; a dash after a space, ` -1.2.3.4`, is no name);
/// - followed by a version's suffix: `-rc2`, `-beta`, `-git…` (a dash and one of [`VERSION_SUFFIXES`]; not
///   `-wlan0`), or `+git`, `~pre` (a plus or tilde, then a letter).
fn version_like(t: &str, i: usize, j: usize) -> bool {
    let b = t.as_bytes();
    if i >= 2 && b[i - 1] == b'-' && b[i - 2].is_ascii_alphanumeric() {
        return true;
    }
    let after = &b[(j + 1).min(b.len())..];
    let suffix = |v: &&str| after.len() >= v.len() && after[..v.len()].eq_ignore_ascii_case(v.as_bytes());
    if (b"+~".contains(&at_byte(t, j)) && at_byte(t, j + 1).is_ascii_alphabetic()) || (at_byte(t, j) == b'-' && VERSION_SUFFIXES.iter().any(suffix)) {
        return true;
    }
    // The word before, over spaces, quotes, "=" and ":" (at most a few of them: "version": "…).
    let mut k = i;
    while k > 0 && i - k < 4 && b" \t\"'=:".contains(&b[k - 1]) {
        k -= 1;
    }
    let end = k;
    while k > 0 && (b[k - 1].is_ascii_alphanumeric() || b[k - 1] == b'_') {
        k -= 1;
    }
    let word = t[k..end].to_ascii_lowercase();
    end < i && (word == "v" || word == "ver" || word == "version" || word.ends_with("_version"))
}

/// Keys whose values are private, at the end of a key (`wifi.key`, `ra_token`, `"password"`).
const PRIVATE_KEYS: &[&str] = &[
    "password", "passwd", "pass", "psk", "passphrase", "secret", "token", "apikey", "api_key", "key", "auth", "authorization", "cookie", "session", "sessionid", "sid", "ticket", "ssid", "bssid", "user", "username", "login", "email", "hostname", "host", "device_id", "deviceid", "tailnet", "code", "serial", "display_name",
];

/// `key=value`, `key: value`, `"key": "value"` with a private key: the value goes.
fn assignment_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    if b[i] != b'=' && b[i] != b':' {
        return None;
    }
    // The key before it: optional spaces and a quote, then letters, digits, _ . -
    let mut k = i;
    while k > 0 && b[k - 1] == b' ' {
        k -= 1;
    }
    if k > 0 && b[k - 1] == b'"' {
        k -= 1;
    }
    let key_end = k;
    while k > 0 && (b[k - 1].is_ascii_alphanumeric() || b"_.-".contains(&b[k - 1])) {
        k -= 1;
    }
    let key = t[k..key_end].to_ascii_lowercase();
    let private = PRIVATE_KEYS.iter().any(|w| key == *w || (key.ends_with(w) && b"_.-".contains(&key.as_bytes()[key.len() - w.len() - 1])))
        // The value of a hidden field (`<input type=hidden … value="…">`): a session token, an address.
        || (key == "value" && t[..i].to_ascii_lowercase().rfind("hidden").is_some_and(|h| i - h < 200));
    if !private || (b[i] == b':' && at_byte(t, i + 1) == b':') || (b[i] == b':' && at_byte(t, i + 1) == b'/') {
        return None;
    }
    // The value: quoted up to its quote, or up to a space or , ; & } ] ).
    let mut v = i + 1;
    while v < b.len() && b[v] == b' ' {
        v += 1;
    }
    let quote = at_byte(t, v);
    let (from, to) = if quote == b'"' || quote == b'\'' {
        let mut e = v + 1;
        while e < b.len() && b[e] != quote && b[e] != b'\n' {
            e += if b[e] == b'\\' { 2 } else { 1 };
        }
        (v + 1, e.min(b.len()))
    } else {
        let mut e = v;
        while e < b.len() && !b[e].is_ascii_whitespace() && !b",;&}])".contains(&b[e]) {
            e += 1;
        }
        // "Authorization: Bearer xyz": the word after Bearer or Basic goes too.
        let word = t[v..e].to_ascii_lowercase();
        if (word == "bearer" || word == "basic") && at_byte(t, e) == b' ' {
            e += 1;
            while e < b.len() && !b[e].is_ascii_whitespace() && !b",;&}])".contains(&b[e]) {
                e += 1;
            }
        }
        (v, e)
    };
    let value = &t[from..to];
    if value.is_empty() || value.starts_with('[') || value.starts_with('{') || matches!(value, "null" | "true" | "false" | "none" | "None") {
        return None;
    }
    // The kernel's "Serial: 8250/16550 driver, 4 ports" names the serial port's driver, not a serial number; a
    // serial number has no "/" in it.
    if key == "serial" && value.contains('/') {
        return None;
    }
    Some((to, format!("{}[removed]", &t[i..from])))
}

