//! The sign-in helper's requests (`portal_page` shows what they bring): loads a Wi-Fi network's sign-in page with
//! `curl`, follows its redirects one by one, reads it (`html_form`), and sends the form the person filled in, only when
//! they press its button. Everything here blocks: a worker's job, never the UI thread.
//!
//! Rules (docs/PORTAL-SIGNIN.md, "What it never does"):
//! - Only `http` and `https`, no user name in an address, no other scheme (`file:`, `javascript:`, `data:`).
//! - Every host is looked up here and handed to curl pinned (`--resolve`), and no request goes to the Pocket itself
//!   (loopback, 0.0.0.0) or to a multicast or broadcast address.
//! - A form is sent only to its own `action`, and only when that host is one this session loaded a page from (the sign-in
//!   page's site) or an address of a private network (the network's own gateway); never anywhere else.
//! - Cookies live in a file in a private folder of this session only (0700, in RAM: `XDG_RUNTIME_DIR` or `/tmp`) and go
//!   with the session. What a person typed is never written to a file or a command line: curl reads its whole
//!   request from a pipe (`-K -`), and the copies are overwritten once sent.
//! - Limits on everything: 8 redirects, 1 MiB a page, 10 s a request (`MAX_TIME`), 5 s to connect, 12 s to look up (`LOOKUP_TIME`).
//!
//! A "please wait" interstitial (a page that forwards the browser to the real sign-in form) is followed without
//! running its scripts, as pure navigation (`follow`): a link in `<noscript>`, a form the page submits on load whose
//! fields are all hidden (sent exactly as a browser would, method, action and hidden values), a script or `onload`
//! that goes to one literal address, and a page that is a single `<iframe>`. Each hop costs one of [`MAX_HOPS`] and
//! the same address rules as everything else; nothing is typed, ticked or accepted on the way. When the end is
//! "needs a full browser", the last page's HTML is kept for the report (`last_page`, `logs::save_portal_page`).

use std::io::{Read, Write};
use std::net::{IpAddr, ToSocketAddrs};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::html_form::{self, Form, Method, NoForm, Page};

/// Redirects followed for one page (HTTP, `<meta refresh>` and the trivial script ones together).
pub const MAX_HOPS: usize = 8;
/// The most a page may be.
pub const MAX_BODY: usize = 1 << 20;
const MAX_HEADERS: usize = 64 * 1024;
/// curl's own time limit for one request, and how long we wait for curl at most.
const MAX_TIME: u64 = 10;
const CONNECT_TIME: u64 = 5;
/// More than glibc's own wait before it asks the DNS server again (`timeout:5`): with 5 s, one answer lost or held up
/// on the way was "No answer" at the first Sign in… every time (Wi-Fi bench, café network, 2026-10-27/28: the A
/// question answered at once by the network's DNS, glibc asking again 5 s later, the helper gone at 5 s).
const LOOKUP_TIME: Duration = Duration::from_secs(12);

/// What this session may reach.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    /// The Pocket's own addresses (127.0.0.1, ::1). Never on the device; the end-to-end test's stand-in portal runs there.
    pub allow_loopback: bool,
    /// How long one curl may run before it is ended.
    pub deadline: Duration,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { allow_loopback: false, deadline: Duration::from_secs(MAX_TIME + 3) }
    }
}

/// A web address as the helper uses it: scheme, host (lower case; an IPv6 address without brackets), port, and the
/// path with its query (no fragment).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    pub https: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl Url {
    pub fn parse(s: &str) -> Option<Url> {
        let s = s.trim();
        if s.len() > 4096 || s.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
            return None;
        }
        let (scheme, rest) = s.split_once("://")?;
        let https = match scheme.to_ascii_lowercase().as_str() {
            "http" => false,
            "https" => true,
            _ => return None,
        };
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        // No "user:password@": a sign-in page has no reason to, and it hides the real host.
        if authority.contains('@') || authority.is_empty() {
            return None;
        }
        let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
            let (h, after) = v6.split_once(']')?;
            h.parse::<std::net::Ipv6Addr>().ok()?;
            (h.to_ascii_lowercase(), after.strip_prefix(':'))
        } else {
            match authority.split_once(':') {
                Some((h, p)) => (h.to_ascii_lowercase(), Some(p)),
                None => (authority.to_ascii_lowercase(), None),
            }
        };
        let port = match port {
            Some(p) => p.parse::<u16>().ok().filter(|p| *p > 0)?,
            None if https => 443,
            None => 80,
        };
        let host_ok = !host.is_empty() && host.len() <= 253 && (host.contains(':') || host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'));
        if !host_ok || host.starts_with('.') || host.starts_with('-') {
            return None;
        }
        let tail = tail.split('#').next().unwrap_or("");
        let path = if tail.starts_with('/') { tail.to_string() } else { format!("/{tail}") };
        Some(Url { https, host, port, path: encode_path(&path) })
    }

    /// `href` as a browser on this page would follow it: absolute, scheme-relative, from the root, a query, or
    /// relative to the page's folder. None for anything that isn't http or https.
    pub fn join(&self, href: &str) -> Option<Url> {
        let href = href.trim();
        let lower = href.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            return Url::parse(href);
        }
        if href.contains("://") || lower.split(['/', '?', '#']).next().is_some_and(|first| first.contains(':')) {
            // Another scheme: javascript:, file:, data:, mailto:.
            return None;
        }
        let scheme = if self.https { "https" } else { "http" };
        if let Some(rest) = href.strip_prefix("//") {
            return Url::parse(&format!("{scheme}://{rest}"));
        }
        let origin = format!("{scheme}://{}", self.authority());
        let path = if href.is_empty() || href.starts_with('#') {
            self.path.clone()
        } else if href.starts_with('/') {
            href.to_string()
        } else if href.starts_with('?') {
            format!("{}{href}", self.path.split('?').next().unwrap_or("/"))
        } else {
            let dir = self.path.split('?').next().unwrap_or("/");
            let dir = &dir[..dir.rfind('/').map_or(0, |i| i + 1)];
            format!("{dir}{href}")
        };
        Url::parse(&format!("{origin}{}", normalize(&path)))
    }

    fn authority(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        let default = if self.https { 443 } else { 80 };
        if self.port == default {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    /// The same address with its query replaced (a GET form's data).
    pub fn with_query(&self, query: &str) -> Url {
        let path = self.path.split('?').next().unwrap_or("/");
        Url { path: if query.is_empty() { path.to_string() } else { format!("{path}?{query}") }, ..self.clone() }
    }

    pub fn same_origin(&self, other: &Url) -> bool {
        self.https == other.https && self.host == other.host && self.port == other.port
    }
}

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}://{}{}", if self.https { "https" } else { "http" }, self.authority(), self.path)
    }
}

/// "." and ".." of a path taken away, as browsers do (never above the root).
fn normalize(path: &str) -> String {
    let (p, query) = match path.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (path, None),
    };
    let mut parts: Vec<&str> = Vec::new();
    let segments: Vec<&str> = p.split('/').collect();
    for (i, seg) in segments.iter().enumerate() {
        let last = i == segments.len() - 1;
        match *seg {
            "." => {
                if last {
                    parts.push("");
                }
            }
            ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
                if last {
                    parts.push("");
                }
            }
            s => parts.push(s),
        }
    }
    let mut out = parts.join("/");
    if !out.starts_with('/') {
        out.insert(0, '/');
    }
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    out
}

/// A path as curl can send it: bytes outside printable ASCII (and the space) as `%XX`.
fn encode_path(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    for &b in p.as_bytes() {
        if b > 0x20 && b < 0x7f && b != b'"' && b != b'\\' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// An address on a network of its own: 10/8, 172.16/12, 192.168/16, 100.64/10 (carrier and hotel networks), 169.254/16,
/// IPv6 unique-local and link-local. A network's gateway lives at one of these.
pub fn private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_private() || v4.is_link_local() || (o[0] == 100 && (o[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => {
            let s = v6.segments()[0];
            (s & 0xfe00) == 0xfc00 || (s & 0xffc0) == 0xfe80
        }
    }
}

/// An address no request of the helper may go to: the Pocket itself (unless the rules allow it), nowhere, multicast,
/// broadcast.
fn forbidden(ip: IpAddr, rules: &Rules) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    };
    let loopback = ip.is_loopback();
    (loopback && !rules.allow_loopback)
        || ip.is_unspecified()
        || ip.is_multicast()
        || matches!(ip, IpAddr::V4(v4) if v4.is_broadcast() || v4.octets()[0] == 0)
}

/// The address `host` is reached at: itself when it is one, else the system's resolver, on a thread so a resolver that
/// hangs costs at most `LOOKUP_TIME`.
fn lookup(host: &str, port: u16) -> Result<IpAddr, Failure> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(ip);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let name = host.to_string();
    std::thread::spawn(move || {
        let found = (name.as_str(), port).to_socket_addrs().map(|a| a.map(|s| s.ip()).collect::<Vec<_>>());
        let _ = tx.send(found);
    });
    match rx.recv_timeout(LOOKUP_TIME) {
        Ok(Ok(ips)) => ips.iter().find(|ip| ip.is_ipv4()).or(ips.first()).copied().ok_or(Failure::Unreachable),
        _ => Err(Failure::Unreachable),
    }
}

/// What went wrong with one request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// No answer: the name isn't known, nothing at the address, a time-out.
    Unreachable,
    /// The page is larger than `MAX_BODY`.
    TooBig,
    /// An address the helper never sends to (scheme, the Pocket itself, a user name in it).
    Refused(String),
    /// More than `MAX_HOPS` redirects.
    Loop,
    /// curl is missing or said something else.
    Other(String),
}

/// One answer.
#[derive(Clone, Debug, Default)]
pub struct Response {
    pub status: u32,
    pub location: Option<String>,
    pub content_type: String,
    pub body: Vec<u8>,
}

/// Reads curl's `--include` output: header blocks (the 1xx ones skipped), then the body.
pub fn split_response(out: &[u8]) -> Option<Response> {
    let mut rest = out;
    loop {
        let end = rest.windows(4).take(MAX_HEADERS).position(|w| w == b"\r\n\r\n").map(|p| (p, 4)).or_else(|| rest.windows(2).take(MAX_HEADERS).position(|w| w == b"\n\n").map(|p| (p, 2)))?;
        let head = String::from_utf8_lossy(&rest[..end.0]);
        let body = &rest[end.0 + end.1..];
        let mut lines = head.lines();
        let status_line = lines.next()?;
        if !status_line.starts_with("HTTP/") {
            return None;
        }
        let status: u32 = status_line.split_whitespace().nth(1)?.parse().ok()?;
        if (100..200).contains(&status) && body.starts_with(b"HTTP/") {
            rest = body;
            continue;
        }
        let mut r = Response { status, ..Response::default() };
        for line in lines {
            let Some((k, v)) = line.split_once(':') else { continue };
            let v = v.trim();
            match k.trim().to_ascii_lowercase().as_str() {
                "location" if r.location.is_none() => r.location = Some(v.to_string()),
                "content-type" if r.content_type.is_empty() => r.content_type = v.chars().take(200).collect(),
                _ => {}
            }
        }
        r.body = body[..body.len().min(MAX_BODY)].to_vec();
        return Some(r);
    }
}

/// A value for curl's config file, in double quotes with its escapes.
fn quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// What a form sends, as the request's body: `multipart/form-data` when the form says so, else
/// `application/x-www-form-urlencoded`, in UTF-8. The caller overwrites the body once the request is made.
fn form_body(form: &Form, pairs: &[(String, String)]) -> (String, String) {
    if form.multipart {
        let boundary = format!("----pocketui{:016x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64));
        (format!("multipart/form-data; boundary={boundary}"), html_form::multipart(pairs, &boundary))
    } else {
        ("application/x-www-form-urlencoded".to_string(), html_form::urlencode(pairs))
    }
}

/// Overwrites a string that held what a person typed, then empties it (best effort: earlier copies the allocator moved
/// are not reached).
pub fn wipe(s: &mut String) {
    let mut bytes = std::mem::take(s).into_bytes();
    for b in bytes.iter_mut() {
        // SAFETY: a plain write to a byte of our own buffer; volatile, so it isn't left out as a dead store.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    drop(bytes);
}

/// The page's HTML as it may go into the logs (`logs::save_portal_page`): query strings with values that look like a
/// MAC or IP address (raw or percent-encoded) are redacted (`?mac=00:00:5E:…` → `?mac=[mac]`), and so is every value of a
/// key that names something private (`token`, `sessionid`, `room`, …). Everything else stays as the page served it.
/// (`report::Redactor` cleans what the report takes along, this keeps the log file itself free of them.)
pub fn redact_query(html: &str) -> String {
    fn mac(v: &str) -> bool {
        let parts: Vec<&str> = v.split([':', '-']).collect();
        (parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit())))
            || (v.len() == 12 && v.bytes().all(|b| b.is_ascii_hexdigit()))
    }
    fn ip(v: &str) -> bool {
        let v = v.trim_matches(['[', ']']);
        if v.parse::<std::net::Ipv4Addr>().is_ok() || v.parse::<std::net::Ipv6Addr>().is_ok() {
            return true;
        }
        // An address a name's zone or an odd page makes longer than the parser takes.
        v.matches(':').count() >= 2 && v.bytes().all(|b| b.is_ascii_hexdigit() || matches!(b, b':' | b'.' | b'%' | b'[' | b']'))
    }
    /// A key that names something private (`mac`, `?client_token=`, `room`, …).
    fn keyed(k: &str) -> bool {
        const KEYS: [&str; 14] = ["mac", "ip", "token", "session", "sessionid", "sid", "auth", "key", "user", "name", "room", "email", "code", "password"];
        let k = k.to_ascii_lowercase();
        KEYS.iter().any(|w| k == *w || (k.ends_with(w) && b"_.-".contains(&k.as_bytes()[k.len() - w.len() - 1])))
    }
    /// `00%3A00%3A…` as the address bar carries it: the bytes behind the `%XX`.
    fn pct(v: &str) -> String {
        let b = v.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
                out.push(u8::from_str_radix(&v[i + 1..i + 3], 16).unwrap_or(b'?'));
                i += 3;
            } else {
                out.push(b[i]);
                i += 1;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
    fn pairs(run: &str) -> String {
        run.split('&')
            .map(|piece| match piece.split_once('=') {
                Some((k, _)) if keyed(k) => format!("{k}=[removed]"),
                Some((k, v)) if mac(&pct(v)) => format!("{k}=[mac]"),
                Some((k, v)) if ip(&pct(v)) => format!("{k}=[ip]"),
                _ => piece.to_string(),
            })
            .collect::<Vec<_>>()
            .join("&")
    }
    let mut out = String::with_capacity(html.len());
    let b = html.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'?' {
            let ch = html[i..].chars().next().unwrap_or('\u{fffd}');
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        out.push('?');
        i += 1;
        let start = i;
        while i < b.len() && !matches!(b[i], b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\'' | b'<' | b'>' | b')') {
            i += 1;
        }
        out.push_str(&pairs(&html[start..i]));
    }
    out
}

/// The value of the attribute `name` of `tag`, as written (quotes off): its byte span (start, end) in `tag`.
fn attr_span(tag: &str, name: &str) -> Option<(usize, usize)> {
    let low = tag.to_ascii_lowercase();
    let b = tag.as_bytes();
    let mut i = 0;
    while let Some(p) = low[i..].find(name) {
        let at = i + p;
        i = at + name.len();
        let before = if at == 0 { b' ' } else { low.as_bytes()[at - 1] };
        if before.is_ascii_alphanumeric() || before == b'-' || before == b'_' {
            continue;
        }
        let mut k = at + name.len();
        while k < b.len() && b[k].is_ascii_whitespace() {
            k += 1;
        }
        if b.get(k) != Some(&b'=') {
            continue;
        }
        k += 1;
        while k < b.len() && b[k].is_ascii_whitespace() {
            k += 1;
        }
        return match b.get(k) {
            Some(&q @ (b'"' | b'\'')) => {
                let end = low[k + 1..].find(q as char).map_or(b.len(), |e| k + 1 + e);
                Some((k + 1, end))
            }
            _ => {
                let end = k + low[k..].find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(b.len() - k);
                Some((k, end))
            }
        };
    }
    None
}

/// Every `<input type=hidden …>` of the page with its `value` blanked (`value="[hidden]"`): what the page carries
/// (tokens, addresses) is not for the logs. The field's `name` stays — the names are what helps find a problem.
pub fn blank_hidden(html: &str) -> String {
    let low = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut rest = 0;
    let mut from = 0;
    while let Some(p) = low[from..].find("<input") {
        let start = from + p;
        let end = low[start..].find('>').map_or(html.len(), |e| start + e + 1);
        let tag = &html[start..end];
        out.push_str(&html[rest..start]);
        let hidden = attr_span(&low[start..end], "type").is_some_and(|(a, e)| low[start + a..start + e].trim() == "hidden");
        if hidden {
            if let Some((a, e)) = attr_span(tag, "value") {
                out.push_str(&tag[..a]);
                out.push_str("[hidden]");
                out.push_str(&tag[e..]);
            } else {
                out.push_str(tag);
            }
        } else {
            out.push_str(tag);
        }
        rest = end;
        from = end;
    }
    out.push_str(&html[rest..]);
    out
}

/// What came of loading a page or sending a form.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// A page with a form to fill in: the page, where it was loaded from, and its usable forms.
    Form { page: Page, url: String, forms: Vec<usize> },
    /// The internet is there (the check says so).
    Online,
    /// The page needs what the helper doesn't do: scripts, or no form at all; with the page (for its text) when one came.
    NeedsBrowser { why: NoForm, page: Option<Page> },
    /// The form sends to a host the helper doesn't send to (`Session::allowed`).
    ElseWhere(String),
    /// The request failed.
    Failed(Failure),
}

impl Outcome {
    /// The end the UI paints as "This page needs a full browser" (`portal_form`'s `paint_end`): the page needs scripts
    /// or has nothing to fill in, or it is too big, loops, or leads to an address the Pocket doesn't open. The last
    /// page fetched is saved for the report when this is the end (`logs::save_portal_page`).
    pub fn shows_full_browser(&self) -> bool {
        matches!(self, Outcome::NeedsBrowser { .. } | Outcome::Failed(Failure::TooBig | Failure::Loop | Failure::Refused(_)))
    }
}

/// One sign-in: a private folder with the cookie jar, the hosts whose pages were loaded, the rules.
pub struct Session {
    dir: PathBuf,
    curl: String,
    rules: Rules,
    hosts: Mutex<Vec<String>>,
    /// The address the connectivity check asks (`net_check::PROBE_URL`; the stand-in portal's in the end-to-end test).
    check_url: String,
    /// An https page whose certificate couldn't be checked was loaded anyway (the page says so).
    pub insecure: AtomicBool,
    /// The HTML of the last page fetched, for the report when the helper ends with "needs a full browser". Never
    /// from a chain that carries a person's form (`submit`): an answer that echoes what was typed is not kept.
    last_page: Mutex<Option<String>>,
    /// A form of a person went out (`submit`): no page is kept from then on.
    sent: AtomicBool,
}

impl Session {
    /// A new session with its own folder: `XDG_RUNTIME_DIR` when there is one (RAM), else the system's temporary folder.
    pub fn new(curl: &str, check_url: &str, rules: Rules) -> std::io::Result<Session> {
        use std::os::unix::fs::DirBuilderExt;
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(std::env::temp_dir);
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
        let dir = base.join(format!("pocket-ui-signin-{}-{}-{nonce}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
        Ok(Session { dir, curl: curl.to_string(), rules, hosts: Mutex::new(Vec::new()), check_url: check_url.to_string(), insecure: AtomicBool::new(false), last_page: Mutex::new(None), sent: AtomicBool::new(false) })
    }

    /// The HTML of the last page fetched (no cookies, no headers): what the network served when the helper gave up.
    /// None once a person's form was sent: an answer that echoes what was typed must never be kept (B2 of the review).
    pub fn last_page(&self) -> Option<String> {
        self.last_page.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn jar(&self) -> PathBuf {
        self.dir.join("cookies")
    }

    /// May a form be sent to `url`: a host whose page this session loaded, or an address of a private network.
    pub fn allowed(&self, url: &Url) -> bool {
        self.hosts.lock().unwrap_or_else(|e| e.into_inner()).contains(&url.host) || url.host.parse::<IpAddr>().is_ok_and(private)
    }

    fn remember(&self, url: &Url) {
        let mut hosts = self.hosts.lock().unwrap_or_else(|e| e.into_inner());
        if !hosts.contains(&url.host) && hosts.len() < 32 {
            hosts.push(url.host.clone());
        }
    }

    /// One request: GET, or POST with `body` of `content_type`. `referer`: the page the form was on.
    fn request(&self, url: &Url, post: Option<(&str, &str)>, referer: Option<&str>) -> Result<Response, Failure> {
        let started = Instant::now();
        let result = self.request_once(url, post, referer);
        // What "No answer" was, for net.log (the host and port only: a path or form may carry a token).
        if let Err(e) = &result {
            eprintln!("portal: {}:{} after {} ms: {e:?}", url.host, url.port, started.elapsed().as_millis());
        }
        result
    }

    fn request_once(&self, url: &Url, post: Option<(&str, &str)>, referer: Option<&str>) -> Result<Response, Failure> {
        let ip = lookup(&url.host, url.port).inspect_err(|_| eprintln!("portal: {} wasn't looked up in {} s", url.host, LOOKUP_TIME.as_secs()))?;
        if forbidden(ip, &self.rules) {
            return Err(Failure::Refused(url.host.clone()));
        }
        let ip_text = if ip.is_ipv6() { format!("[{ip}]") } else { ip.to_string() };
        let mut insecure = self.insecure.load(Ordering::Relaxed);
        loop {
            let mut config = String::new();
            let mut line = |k: &str, v: Option<&str>| {
                config.push_str(k);
                if let Some(v) = v {
                    config.push_str(" = ");
                    config.push_str(&quoted(v));
                }
                config.push('\n');
            };
            line("url", Some(&url.to_string()));
            if url.host.parse::<IpAddr>().is_err() {
                line("resolve", Some(&format!("{}:{}:{ip_text}", url.host, url.port)));
            }
            line("silent", None);
            line("include", None);
            line("noproxy", Some("*"));
            line("proto", Some("=http,https"));
            line("max-time", Some(&MAX_TIME.to_string()));
            line("connect-timeout", Some(&CONNECT_TIME.to_string()));
            line("max-filesize", Some(&MAX_BODY.to_string()));
            line("cookie", Some(&self.jar().to_string_lossy()));
            line("cookie-jar", Some(&self.jar().to_string_lossy()));
            line("user-agent", Some("Mozilla/5.0 (Linux; aarch64) pocket-ui sign-in helper"));
            line("header", Some("Accept: text/html,application/xhtml+xml,*/*;q=0.8"));
            line("header", Some(if crate::i18n::current() == crate::i18n::Lang::De { "Accept-Language: de,en;q=0.8" } else { "Accept-Language: en,de;q=0.5" }));
            if let Some(r) = referer {
                line("referer", Some(r));
            }
            if insecure {
                line("insecure", None);
            }
            if let Some((ct, body)) = &post {
                line("header", Some(&format!("Content-Type: {ct}")));
                // A leading "@" would make curl read a file of that name; a form body never starts with one.
                if body.starts_with('@') {
                    return Err(Failure::Other("odd form".into()));
                }
                line("data-binary", Some(body));
            }
            let run = self.run(&mut config);
            wipe(&mut config);
            match run {
                // 60: the certificate can't be checked (a portal's own, self-signed): once more without, and said.
                Err(Failure::Other(code)) if code == "60" && url.https && !insecure => {
                    insecure = true;
                    self.insecure.store(true, Ordering::Relaxed);
                }
                other => return other,
            }
        }
    }

    /// Runs curl with `config` on its stdin; reads at most `MAX_BODY` plus the headers, ends it at the deadline.
    fn run(&self, config: &mut String) -> Result<Response, Failure> {
        let mut cmd = Command::new(&self.curl);
        cmd.args(["-q", "-K", "-"])
            .env_remove("http_proxy")
            .env_remove("https_proxy")
            .env_remove("HTTPS_PROXY")
            .env_remove("ALL_PROXY")
            .env_remove("all_proxy")
            .env("CURL_HOME", &self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let mut child = cmd.spawn().map_err(|e| Failure::Other(e.to_string()))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(config.as_bytes());
        }
        let limit = MAX_BODY + MAX_HEADERS;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut stdout = child.stdout.take();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let mut big = false;
            if let Some(pipe) = stdout.as_mut() {
                let mut chunk = [0u8; 16384];
                while let Ok(n) = pipe.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    out.extend_from_slice(&chunk[..n]);
                    if out.len() > limit {
                        big = true;
                        break;
                    }
                }
            }
            let _ = tx.send((out, big));
        });
        let deadline = Instant::now() + self.rules.deadline;
        let kill = |child: &mut std::process::Child| {
            // SAFETY: kill(2) with a negative pid signals only the group `process_group(0)` made for this child.
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            let _ = child.kill();
            let _ = child.wait();
        };
        let (out, big) = match rx.recv_timeout(self.rules.deadline) {
            Ok(got) => got,
            Err(_) => {
                kill(&mut child);
                return Err(Failure::Unreachable);
            }
        };
        if big {
            kill(&mut child);
            return Err(Failure::TooBig);
        }
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
                _ => {
                    kill(&mut child);
                    return Err(Failure::Unreachable);
                }
            }
        };
        match status.code() {
            Some(0) => split_response(&out).ok_or_else(|| Failure::Other("no answer".into())),
            Some(63) => Err(Failure::TooBig),
            Some(c @ (5 | 6 | 7 | 28 | 35 | 52 | 55 | 56)) => {
                eprintln!("portal: curl exit {c}");
                Err(Failure::Unreachable)
            }
            Some(c) => Err(Failure::Other(c.to_string())),
            None => Err(Failure::Unreachable),
        }
    }

    /// Loads the sign-in page from `start` (where the check was sent), following redirects.
    pub fn open(&self, start: &str) -> Outcome {
        match Url::parse(start) {
            Some(url) => self.follow(url, None, None),
            None => Outcome::Failed(Failure::Refused(start.chars().take(80).collect())),
        }
    }

    /// Follows redirects from `url` (the first request with `post`, whose body is overwritten before this returns),
    /// up to `MAX_HOPS`, and reads where it ends. Between the redirects it does what the page asks of a browser without
    /// scripts, and nothing else: a link in `<noscript>`, a form the page submits on load whose fields are all hidden
    /// (as a browser would send it: method, action, hidden values; same `allowed` rules as a form a person sent), a
    /// script or `onload` that goes to one literal address (`<meta http-equiv=refresh>` the same way), and a page that
    /// is a single `<iframe>`. Nothing is typed, ticked or accepted; what is built at run time is never followed.
    fn follow(&self, mut url: Url, mut post: Option<(String, String)>, mut referer: Option<String>) -> Outcome {
        // A hidden form the page's own script submits on load went out in this chain: where it led may be online.
        let mut auto = false;
        let mut hop = 0;
        let outcome = loop {
            if hop > MAX_HOPS {
                break Outcome::Failed(Failure::Loop);
            }
            hop += 1;
            let here = url.to_string();
            // (The first address is loaded whatever it is: a network that answers every address with its own page has
            // its sign-in at the check's address.)
            if hop > 1 && (here == self.check_url || here == crate::net_check::PROBE_URL) {
                // Back at the check's own address (where a sign-in sends the browser on): whether that answers online is
                // the check's to say; the helper doesn't load it as a page.
                break self.after();
            }
            let body = post.take();
            let resp = self.request(&url, body.as_ref().map(|(ct, b)| (ct.as_str(), b.as_str())), referer.as_deref());
            if let Some((_, mut b)) = body {
                wipe(&mut b);
            }
            let resp = match resp {
                Ok(r) => r,
                Err(e) => break Outcome::Failed(e),
            };
            if let Some(next) = resp.location.as_deref().filter(|_| (300..400).contains(&resp.status)) {
                match url.join(next) {
                    Some(n) => {
                        referer = None;
                        url = n;
                        continue;
                    }
                    None => break Outcome::Failed(Failure::Refused(next.chars().take(80).collect())),
                }
            }
            self.remember(&url);
            let text = html_form::decode(&resp.body, &resp.content_type);
            if text.trim() == crate::net_check::ONLINE_BODY {
                break Outcome::Online;
            }
            if !resp.body.is_empty() && !self.sent.load(Ordering::Relaxed) {
                // The page as it came, while no person's form is in the air: for the report when the end is
                // "needs a full browser" (saved by `setup`).
                *self.last_page.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.clone());
            }
            let page = html_form::parse(&text);
            let forms = page.usable();
            if !forms.is_empty() {
                break Outcome::Form { page, url: url.to_string(), forms };
            }
            // What the page asks of a browser without scripts, in the order a browser without scripts would do it:
            // the real link of its <noscript>, the form it submits on load, where it sends the browser on, its frame.
            // Only on the page's own site or the network's own gateway: a tracker's frame or an "enable JavaScript"
            // link to somewhere else is never followed (and so never joins the hosts a form may go to).
            if let Some(n) = page
                .noscript_link
                .as_deref()
                .and_then(|t| url.join(t))
                .filter(|n| *n != url && (url.same_origin(n) || n.host.parse::<IpAddr>().is_ok_and(private)))
            {
                referer = None;
                url = n;
                continue;
            }
            if let Some(fi) = page.auto_form {
                let action = url.join(page.forms[fi].action.as_deref().unwrap_or(""));
                if let Some(action) = action {
                    if !self.allowed(&action) {
                        // The page carries what it holds somewhere the helper never sends to, and no person pressed
                        // anything: said plainly as what it is, the page kept for the report. Only a form a person
                        // pressed is refused as "Not sent" (`submit`).
                        break Outcome::NeedsBrowser { why: NoForm::ElseWhere, page: Some(page) };
                    }
                    auto = true;
                    let pairs = html_form::form_data(&page.forms[fi], None);
                    match page.forms[fi].method {
                        Method::Get => {
                            let mut query = html_form::urlencode(&pairs);
                            url = action.with_query(&query);
                            wipe(&mut query);
                            post = None;
                        }
                        Method::Post => {
                            url = action;
                            post = Some(form_body(&page.forms[fi], &pairs));
                        }
                    }
                    referer = Some(here);
                    continue;
                }
                // A form to a script or a file: not followed; the page's other words may still lead on.
            }
            let nav = page
                .redirect
                .as_deref()
                .and_then(|t| url.join(t))
                .filter(|n| *n != url)
                .or_else(|| {
                    (page.iframes.len() == 1)
                        .then(|| page.iframes.first())
                        .flatten()
                        .and_then(|t| url.join(t))
                        .filter(|n| *n != url && (url.same_origin(n) || n.host.parse::<IpAddr>().is_ok_and(private)))
                });
            if let Some(n) = nav {
                referer = None;
                url = n;
                continue;
            }
            // The auto-submit went out in this chain: what came back may be online without a form to fill in.
            if auto && self.online() {
                break Outcome::Online;
            }
            let why = page.no_form().unwrap_or(NoForm::Nothing);
            break Outcome::NeedsBrowser { why, page: Some(page) };
        };
        if let Some((_, body)) = &mut post {
            wipe(body);
        }
        outcome
    }

    /// Sends `form` of the page loaded from `page_url`, as the button `submitter` would; then follows where it leads
    /// and asks the check. Only on a press of that button. From here on no page is kept for the report (`sent`): an
    /// answer that echoes what was typed must never reach a file.
    pub fn submit(&self, page_url: &str, form: &Form, submitter: Option<usize>) -> Outcome {
        self.sent.store(true, Ordering::Relaxed);
        *self.last_page.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let Some(page) = Url::parse(page_url) else { return Outcome::Failed(Failure::Refused(String::new())) };
        let Some(action) = page.join(form.action.as_deref().unwrap_or("")) else {
            return Outcome::Failed(Failure::Refused(form.action.clone().unwrap_or_default().chars().take(80).collect()));
        };
        if !self.allowed(&action) {
            return Outcome::ElseWhere(action.host.clone());
        }
        let mut pairs = html_form::form_data(form, submitter);
        let outcome = match form.method {
            Method::Get => {
                let mut query = html_form::urlencode(&pairs);
                let url = action.with_query(&query);
                wipe(&mut query);
                self.follow(url, None, Some(page_url.to_string()))
            }
            Method::Post => {
                // `follow` overwrites the body once the request is made.
                self.follow(action, Some(form_body(form, &pairs)), Some(page_url.to_string()))
            }
        };
        for (_, v) in pairs.iter_mut() {
            wipe(v);
        }
        match outcome {
            // A page with a form again: the next step, or the same page saying what was wrong. Online wins.
            Outcome::Form { .. } | Outcome::NeedsBrowser { .. } if self.online() => Outcome::Online,
            other => other,
        }
    }

    /// After a submit led back to the check's address: online, or the sign-in page again.
    fn after(&self) -> Outcome {
        if self.online() {
            Outcome::Online
        } else {
            Outcome::NeedsBrowser { why: NoForm::Nothing, page: None }
        }
    }

    /// Asks the check (twice, two seconds apart: a network may take a moment to open after a sign-in).
    pub fn online(&self) -> bool {
        for k in 0..2 {
            if k > 0 {
                std::thread::sleep(Duration::from_secs(2));
            }
            if crate::net_check::check_with(&self.curl, &self.check_url).net == crate::net_check::Net::Online {
                return true;
            }
        }
        false
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::write_executable;
    use crate::testutil::TestDir;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap_or_else(|| panic!("{s}"))
    }

    #[test]
    fn only_web_addresses_without_a_user_name_are_taken() {
        assert_eq!(u("http://Login.Hotel.example:8080/a?b=1#frag"), Url { https: false, host: "login.hotel.example".into(), port: 8080, path: "/a?b=1".into() });
        assert_eq!(u("https://192.0.2.1").path, "/");
        assert_eq!(u("http://[fe80::1]:2050/x").host, "fe80::1");
        let with_login = format!("http://user{}pw@a.example.com/", ':'); // built here: no login in a URL in the source
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,hi",
            "ftp://a.example/",
            with_login.as_str(),
            "http://a.example:0/",
            "http://a.example:99999/",
            "http://a.example/\"\nurl = http://evil/",
            "http://a b.example/",
            "http:///x",
            "http://[nothex]/",
            "http://.example/",
            "http://exa_mple.com/",
        ] {
            assert_eq!(Url::parse(bad), None, "{bad:?}");
        }
        assert_eq!(u("http://a.example/über raum").path, "/%C3%BCber%20raum", "encoded for curl");
    }

    #[test]
    fn links_resolve_as_in_a_browser_and_other_schemes_never() {
        let base = u("http://portal.example:8002/captive/index.php?zone=a");
        assert_eq!(base.join("login.php").unwrap().to_string(), "http://portal.example:8002/captive/login.php");
        assert_eq!(base.join("/x").unwrap().to_string(), "http://portal.example:8002/x");
        assert_eq!(base.join("?zone=b").unwrap().to_string(), "http://portal.example:8002/captive/index.php?zone=b");
        assert_eq!(base.join("").unwrap(), base);
        assert_eq!(base.join("../../../../etc/x").unwrap().to_string(), "http://portal.example:8002/etc/x");
        assert_eq!(base.join("//other.example/p").unwrap().to_string(), "http://other.example/p");
        assert_eq!(base.join("HTTPS://S.example/").unwrap().to_string(), "https://s.example/");
        for bad in ["javascript:submit()", "JavaScript:void(0)", "file:///etc/shadow", "data:,x", "mailto:a@b", " vbscript:x"] {
            assert_eq!(base.join(bad), None, "{bad}");
        }
        assert_eq!(base.with_query("a=1").to_string(), "http://portal.example:8002/captive/index.php?a=1", "a GET form replaces the query");
    }

    #[test]
    fn the_pocket_itself_and_nowhere_are_never_asked() {
        let rules = Rules::default();
        for ip in ["127.0.0.1", "127.8.9.1", "::1", "0.0.0.0", "::", "224.0.0.1", "255.255.255.255", "::ffff:127.0.0.1", "0.1.2.3"] {
            assert!(forbidden(ip.parse().unwrap(), &rules), "{ip}");
        }
        for ip in ["192.168.1.1", "10.8.0.1", "100.64.0.1", "8.8.8.8", "fe80::1"] {
            assert!(!forbidden(ip.parse().unwrap(), &rules), "{ip}");
        }
        assert!(!forbidden("127.0.0.1".parse().unwrap(), &Rules { allow_loopback: true, ..rules }));
        assert!(private("192.168.4.1".parse().unwrap()) && private("172.20.0.1".parse().unwrap()) && private("100.100.1.1".parse().unwrap()));
        assert!(private("fd00::1".parse().unwrap()) && private("fe80::1".parse().unwrap()));
        assert!(!private("8.8.8.8".parse().unwrap()) && !private("172.32.0.1".parse().unwrap()) && !private("100.128.0.1".parse().unwrap()));
    }

    #[test]
    fn curl_config_values_cannot_start_a_line_of_their_own() {
        assert_eq!(quoted("a\"b\\c\nurl = x\r\t"), "\"a\\\"b\\\\c\\nurl = x\\r\\t\"");
        assert_eq!(quoted("\u{1}x\u{7f}"), "\"x\"");
    }

    #[test]
    fn headers_and_body_are_told_apart() {
        let r = split_response(b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 302 Found\r\nLocation: /next\r\nContent-Type: text/html\r\n\r\n<p>moved</p>").unwrap();
        assert_eq!((r.status, r.location.as_deref(), r.content_type.as_str(), r.body.as_slice()), (302, Some("/next"), "text/html", b"<p>moved</p>".as_slice()));
        let r = split_response(b"HTTP/2 511\r\ncontent-type: text/html; charset=utf-8\r\n\r\n").unwrap();
        assert_eq!(r.status, 511);
        assert!(split_response(b"garbage").is_none());
        assert!(split_response(b"HTTP/1.1 abc\r\n\r\n").is_none());
    }

    #[test]
    fn a_typed_value_is_overwritten() {
        let mut s = String::from("room 412");
        wipe(&mut s);
        assert!(s.is_empty());
    }

    /// A stand-in curl: it saves its config (what it was asked) and answers with the file named by the url's last part.
    struct Fake {
        dir: crate::testutil::TestDir,
        curl: String,
    }

    impl Fake {
        fn new(name: &str, script: &str) -> Fake {
            let dir = TestDir::new(&format!("portal-{name}"));
            let curl = dir.join("curl");
            write_executable(&curl, format!("#!/bin/sh\nD='{}'\n{script}\n", dir.display()));
            Fake { curl: curl.to_string_lossy().into_owned(), dir }
        }

        fn session(&self) -> Session {
            Session::new(&self.curl, "http://check.example/ok", Rules { allow_loopback: false, deadline: Duration::from_secs(3) }).unwrap()
        }
    }

    impl Fake {
        /// A stand-in curl that answers each address's last path part with the file `page-<part>`, records every
        /// config it is given (`asked<N>`), and answers the connectivity check from the file `online` when there.
        fn serving(name: &str) -> Fake {
            Fake::new(
                name,
                r#"if [ "$1" = "-q" ]; then
  n=$(ls "$D" | grep -c '^asked'); cat > "$D/asked$n"
  u=$(sed -n 's/^url = "\(.*\)"/\1/p' "$D/asked$n" | head -1)
  head=1
else
  for u; do :; done
  head=0
fi
p=${u##*/}; p=${p%%\?*}
if [ "$p" = "check_network_status.txt" ]; then
  if [ -f "$D/online" ]; then
    [ "$head" = 1 ] && printf 'HTTP/1.1 200 OK\r\n\r\n'
    printf 'NetworkManager is online'
    [ "$head" = 0 ] && printf '\n200 '
  elif [ "$head" = 1 ]; then printf 'HTTP/1.1 503\r\n\r\nbusy'
  else printf 'busy\n503 '; fi
  exit 0
fi
if [ -f "$D/page-$p" ]; then
  [ "$head" = 1 ] && printf 'HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n'
  cat "$D/page-$p"
  [ "$head" = 0 ] && printf '\n200 '
  exit 0
fi
[ "$head" = 1 ] && printf 'HTTP/1.1 404 Not Found\r\n\r\nmissing'
exit 0"#,
            )
        }

        /// Answers the address's last path part with this page.
        fn page(&self, part: &str, html: &str) {
            std::fs::write(self.dir.join(format!("page-{part}")), html).unwrap();
        }

        /// How many requests it was given.
        fn asked(&self) -> usize {
            std::fs::read_dir(&self.dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("asked")).count()
        }
    }

    /// The session of a `serving` stand-in: the check is its own `check_network_status.txt` (answered from `online`).
    fn fsession(f: &Fake) -> Session {
        Session::new(&f.curl, "http://check.example/check_network_status.txt", Rules { allow_loopback: false, deadline: Duration::from_secs(3) }).unwrap()
    }

    /// The form the interstitials lead to, with the room field the test looks for.
    const REAL_FORM: &str = r#"<form method=post action=/login.html><input name=room><input type=submit value=Connect></form>"#;


    #[test]
    fn endless_redirects_end() {
        let f = Fake::new("loop", "cat > /dev/null; printf 'HTTP/1.1 302 Found\\r\\nLocation: /again\\r\\n\\r\\n'");
        let s = f.session();
        assert!(matches!(s.open("http://192.0.2.1/start"), Outcome::Failed(Failure::Loop)));
        let f = Fake::new("refresh", "cat > /dev/null; printf 'HTTP/1.1 200 OK\\r\\n\\r\\n<meta http-equiv=refresh content=\"0;url=/x\"><script>location.href=\"/y\"</script>'");
        assert!(matches!(f.session().open("http://192.0.2.1/start"), Outcome::Failed(Failure::Loop) | Outcome::NeedsBrowser { .. }));
    }

    #[test]
    fn a_network_that_answers_the_checks_address_with_its_page_is_loaded_there() {
        let f = Fake::new("own", "cat > /dev/null; printf 'HTTP/1.1 200 OK\\r\\n\\r\\n<form><input type=submit value=Accept></form>'");
        let check = "http://192.0.2.9/check_network_status.txt";
        let s = Session::new(&f.curl, check, Rules { allow_loopback: false, deadline: Duration::from_secs(3) }).unwrap();
        match s.open(check) {
            Outcome::Form { url, .. } => assert_eq!(url, check),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_huge_page_and_a_hanging_curl_cost_a_bounded_time() {
        let f = Fake::new("huge", "cat > /dev/null; printf 'HTTP/1.1 200 OK\\r\\n\\r\\n'; head -c 3000000 /dev/zero");
        let t = Instant::now();
        assert!(matches!(f.session().open("http://192.0.2.1/"), Outcome::Failed(Failure::TooBig)));
        let f = Fake::new("hang", "cat > /dev/null; printf 'HTTP/1.1 200 OK\\r\\n\\r\\n<p>'; sleep 30");
        assert!(matches!(f.session().open("http://192.0.2.1/"), Outcome::Failed(Failure::Unreachable)));
        assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
        let f = Fake::new("garbage", "cat > /dev/null; head -c 20000 /dev/urandom");
        assert!(matches!(f.session().open("http://192.0.2.1/"), Outcome::Failed(_)));
        let missing = Session::new("/nonexistent/curl", "http://check.example/ok", Rules::default()).unwrap();
        assert!(matches!(missing.open("http://192.0.2.1/"), Outcome::Failed(Failure::Other(_))));
    }

    #[test]
    fn the_pocket_itself_is_refused_before_curl_runs() {
        let f = Fake::new("self", "cat > \"$D/asked\"; printf 'HTTP/1.1 200 OK\\r\\n\\r\\nhi'");
        let s = f.session();
        assert!(matches!(s.open("http://127.0.0.1:8080/"), Outcome::Failed(Failure::Refused(_))));
        assert!(matches!(s.open("http://[::1]/"), Outcome::Failed(Failure::Refused(_))));
        assert!(matches!(s.open("file:///etc/passwd"), Outcome::Failed(Failure::Refused(_))));
        assert!(!f.dir.join("asked").exists(), "curl never ran");
        // A redirect to the Pocket itself is refused the same way.
        let f = Fake::new("to-self", "cat > /dev/null; printf 'HTTP/1.1 302 Found\\r\\nLocation: http://127.0.0.1:631/\\r\\n\\r\\n'");
        assert!(matches!(f.session().open("http://192.0.2.1/"), Outcome::Failed(Failure::Refused(_))));
        let f = Fake::new("to-file", "cat > /dev/null; printf 'HTTP/1.1 302 Found\\r\\nLocation: file:///etc/passwd\\r\\n\\r\\n'");
        assert!(matches!(f.session().open("http://192.0.2.1/"), Outcome::Failed(Failure::Refused(_))));
    }

    #[test]
    fn a_form_goes_only_to_its_own_site_and_typed_data_never_onto_a_command_line() {
        // Answers a page with a form whose action is set by the test; records every config it is given.
        let f = Fake::new(
            "submit",
            "n=$(ls \"$D\" | grep -c '^asked'); cat > \"$D/asked$n\"; printf 'HTTP/1.1 200 OK\\r\\nContent-Type: text/html\\r\\n\\r\\n'; cat \"$D/page\"",
        );
        let s = f.session();
        std::fs::write(f.dir.join("page"), "<form method=post action=/login><input name=room><input type=password name=pw><input type=submit></form>").unwrap();
        let (page, url, forms) = match s.open("http://192.0.2.5/start") {
            Outcome::Form { page, url, forms } => (page, url, forms),
            other => panic!("{other:?}"),
        };
        assert_eq!(url, "http://192.0.2.5/start");
        let mut form = page.forms[forms[0]].clone();
        form.fields[0].value = "412".into();
        form.fields[1].value = "s3cret pw".into();
        let _ = s.submit(&url, &form, Some(2));
        let asked = std::fs::read_to_string(f.dir.join("asked1")).unwrap();
        assert!(asked.contains("url = \"http://192.0.2.5/login\""), "{asked}");
        assert!(asked.contains("data-binary = \"room=412&pw=s3cret+pw\""), "the data travels on the pipe: {asked}");
        assert!(asked.contains("referer = \"http://192.0.2.5/start\""));
        assert!(asked.contains("proto = \"=http,https\"") && asked.contains("noproxy = \"*\""));

        // Elsewhere: another site, never asked.
        form.action = Some("https://collect.example/steal".into());
        let before = std::fs::read_dir(&f.dir).unwrap().count();
        assert!(matches!(s.submit(&url, &form, Some(2)), Outcome::ElseWhere(h) if h == "collect.example"));
        assert_eq!(std::fs::read_dir(&f.dir).unwrap().count(), before, "nothing was sent");
        form.action = Some("javascript:send()".into());
        assert!(matches!(s.submit(&url, &form, Some(2)), Outcome::Failed(Failure::Refused(_))));
        // The network's own gateway (a private address) is fine.
        form.action = Some("http://192.168.4.1:2050/auth".into());
        assert!(s.allowed(&u("http://192.168.4.1:2050/auth")));
        assert!(!s.allowed(&u("http://8.8.8.8/")));
    }

    #[test]
    fn the_cookie_jar_is_private_and_goes_with_the_session() {
        use std::os::unix::fs::PermissionsExt;
        let s = Session::new("curl", "http://check.example/ok", Rules::default()).unwrap();
        let dir = s.dir.clone();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        std::fs::write(s.jar(), "cookie").unwrap();
        drop(s);
        assert!(!dir.exists());
    }

    #[test]
    fn a_please_wait_page_leads_on_to_the_form_without_running_scripts() {
        // The hotel's "Internet Access - please wait." page (a timer with a literal address), then a Cisco/Aruba-style
        // form the page submits on load (only hidden fields), then the real form to fill in. No script runs; each hop
        // is one request, and the hidden form goes out exactly as a browser would send it on load.
        let f = Fake::serving("interstitial");
        f.page("start", include_str!("../testdata/portal/interstitial-js.html"));
        f.page("gateway.html", include_str!("../testdata/portal/interstitial-wait.html"));
        f.page("auth.html", REAL_FORM);
        let s = fsession(&f);
        let (url, forms) = match s.open("http://192.0.2.5/start") {
            Outcome::Form { url, forms, .. } => (url, forms),
            other => panic!("{other:?}"),
        };
        assert_eq!(forms, [0], "the one form to fill in");
        assert_eq!(url, "http://192.0.2.5/guest/auth.html");
        assert_eq!(f.asked(), 3, "start, the timer's page, the hidden form's page");
        let asked = std::fs::read_to_string(f.dir.join("asked2")).unwrap();
        assert!(asked.contains("url = \"http://192.0.2.5/guest/auth.html\""), "{asked}");
        assert!(
            asked.contains("data-binary = \"buttonClicked=4&err_flag=0&err_msg=&info_msg=&redirect_url=http%3A%2F%2Fnmcheck.gnome.org%2Fcheck_network_status.txt&mac=00%3A00%3A5E%3A00%3A53%3A7F&ip=192.0.2.40\""),
            "the hidden values as a browser sends them: {asked}"
        );
        assert!(asked.contains("referer = \"http://192.0.2.5/guest/gateway.html\""), "{asked}");
    }

    #[test]
    fn the_noscript_link_the_meta_refresh_and_a_single_frame_lead_to_the_form() {
        // Preferred: the link a browser without scripts is offered (the script's own target is not asked at all).
        let f = Fake::serving("noscript");
        f.page("start", include_str!("../testdata/portal/interstitial-noscript.html"));
        f.page("login.html", REAL_FORM);
        f.page("welcome.html", REAL_FORM);
        let s = fsession(&f);
        assert!(matches!(s.open("http://192.0.2.5/start"), Outcome::Form { .. }));
        assert_eq!(f.asked(), 2);
        assert!(!std::fs::read_to_string(f.dir.join("asked1")).unwrap().contains("welcome"), "the noscript link is preferred");
        // A meta refresh with odd spacing and quotes.
        let f = Fake::serving("meta");
        f.page("start", include_str!("../testdata/portal/interstitial-meta.html"));
        f.page("login.html", REAL_FORM);
        assert!(matches!(fsession(&f).open("http://192.0.2.5/start"), Outcome::Form { .. }));
        // A page that is a single frame.
        let f = Fake::serving("frame");
        f.page("start", include_str!("../testdata/portal/interstitial-iframe.html"));
        f.page("login.html", REAL_FORM);
        assert!(matches!(fsession(&f).open("http://192.0.2.5/start"), Outcome::Form { .. }));
    }

    #[test]
    fn hostile_interstitials_loop_or_are_refused_and_send_nothing() {
        // Two "please wait" pages that forward to each other: the redirect limit ends it.
        let f = Fake::serving("loop");
        f.page("hostile-loop-1.html", include_str!("../testdata/portal/hostile-loop-1.html"));
        f.page("hostile-loop-2.html", include_str!("../testdata/portal/hostile-loop-2.html"));
        assert!(matches!(fsession(&f).open("http://192.0.2.5/hostile-loop-1.html"), Outcome::Failed(Failure::Loop)));
        assert_eq!(f.asked(), MAX_HOPS + 1);
        // file: and javascript: are never followed and never asked for.
        let f = Fake::serving("scheme");
        f.page("start", include_str!("../testdata/portal/hostile-scheme.html"));
        let s = fsession(&f);
        assert!(matches!(s.open("http://192.0.2.5/start"), Outcome::NeedsBrowser { .. }));
        assert_eq!(f.asked(), 1, "only the page itself");
        // A form the page submits on load to another public host: nothing is sent, said plainly as what it is
        // ("needs a full browser", not "Not sent" — no person pressed anything), and the page is kept for the report.
        let f = Fake::serving("away");
        f.page("start", include_str!("../testdata/portal/hostile-autosubmit.html"));
        let s = fsession(&f);
        let outcome = s.open("http://192.0.2.5/start");
        assert!(matches!(&outcome, Outcome::NeedsBrowser { why: NoForm::ElseWhere, .. }), "{outcome:?}");
        assert!(outcome.shows_full_browser(), "the page is saved for the report");
        assert!(s.last_page().is_some(), "the page is kept");
        assert_eq!(f.asked(), 1, "nothing was sent to it");
    }

    #[test]
    fn a_tracker_frame_or_a_help_link_off_site_is_never_followed() {
        // The Google Tag Manager snippet and an "enable JavaScript" help link: the page's own redirect leads on,
        // the third parties are never asked and never join the hosts a form may go to.
        let f = Fake::serving("gtm");
        f.page("start", include_str!("../testdata/portal/hostile-gtm.html"));
        f.page("login.html", REAL_FORM);
        let s = fsession(&f);
        assert!(matches!(s.open("http://192.0.2.5/start"), Outcome::Form { .. }), "the page's own redirect leads on");
        assert_eq!(f.asked(), 2, "the page and its own login, never the tracker or the help link");
        for asked in ["asked0", "asked1"] {
            let a = std::fs::read_to_string(f.dir.join(asked)).unwrap();
            assert!(!a.contains("googletagmanager") && !a.contains("enable-javascript"), "{a}");
        }
        assert!(!s.allowed(&u("https://www.googletagmanager.com/")), "never a host a form may go to");
        assert!(!s.allowed(&u("https://www.enable-javascript.com/")));
        // Without a redirect of its own, the page ends honestly — and the tracker is still not loaded.
        let f = Fake::serving("gtm2");
        f.page(
            "start",
            r#"<html><body><div id=root></div>
               <noscript><iframe src="https://www.googletagmanager.com/ns.html?id=GTM-1" height=0 width=0></iframe></noscript>
               <noscript>Please <a href="https://www.enable-javascript.com/">enable JavaScript</a>.</noscript></body></html>"#,
        );
        assert!(matches!(fsession(&f).open("http://192.0.2.5/start"), Outcome::NeedsBrowser { .. }));
        assert_eq!(f.asked(), 1);
    }

    #[test]
    fn nothing_a_person_typed_is_kept_for_the_report() {
        // The answer to a sent form often echoes it ("Welcome Mr Müller, room 412"): no page is kept from a chain
        // that carries a person's form, so what was typed can never reach the file or the report.
        let f = Fake::serving("echo");
        f.page("start", r#"<form method=post action=/answer.html><input name=room><input type=submit></form>"#);
        f.page("answer.html", "<p>Welcome Mr M&uuml;ller, room 412</p><script>location.href='/done.html';</script>");
        f.page("done.html", "<p>Done</p>");
        let s = fsession(&f);
        let (page, url, forms) = match s.open("http://192.0.2.5/start") {
            Outcome::Form { page, url, forms } => (page, url, forms),
            other => panic!("{other:?}"),
        };
        assert!(s.last_page().is_some(), "before the send, the page is kept");
        let mut form = page.forms[forms[0]].clone();
        form.fields[0].value = "412".into();
        let _ = s.submit(&url, &form, Some(1));
        assert!(s.last_page().is_none(), "no page after a person's form is sent");
    }

    #[test]
    fn the_last_page_is_kept_for_the_report_and_query_values_are_redacted() {
        let f = Fake::serving("last");
        f.page("start", include_str!("../testdata/portal/hostile-scheme.html"));
        let s = fsession(&f);
        let _ = s.open("http://192.0.2.5/start");
        let last = s.last_page().expect("the page that gave up is kept");
        assert!(last.contains("file:///etc/passwd"), "{last}");
        // For the logs: MAC- and IP-looking values redacted wherever a query carries them (raw, percent-encoded,
        // twelve hex digits), and every value of a key that names something private.
        assert_eq!(
            redact_query("<a href=/c?client=00%3A00%3A5e%3A00%3A53%3A7f&host=2001:db8::1&ts=1>x <a href=/c?mac=001122334455&sessionid=abc123>y <a href=/c?room=412&id=17>z"),
            "<a href=/c?client=[mac]&host=[ip]&ts=1>x <a href=/c?mac=[removed]&sessionid=[removed]>y <a href=/c?room=[removed]&id=17>z"
        );
        assert_eq!(redact_query("Why? Because. "), "Why? Because. ");
        // What a hidden field carries is not for the logs either (its name stays: the names are what finds a problem).
        assert_eq!(
            blank_hidden(r#"<form><input type="hidden" name="sessionid" value="abc123"><input type=HIDDEN name='mac' value='00:00:5E:00:53:7F'><input name=room value=412><input type=hidden name=tok></form>"#),
            r#"<form><input type="hidden" name="sessionid" value="[hidden]"><input type=HIDDEN name='mac' value='[hidden]'><input name=room value=412><input type=hidden name=tok></form>"#
        );
        // Saved into the boot's log folder: hidden values blanked, queries redacted, capped at PORTAL_LAST_CAP.
        let d = TestDir::new("portal-last");
        crate::logs::save_portal_page(&d, r#"<input type=hidden name=sessionid value="abc123"><a href="/c?client=00%3A00%3A5e%3A00%3A53%3A7f">x</a>"#).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.join("logs/portal-last.html")).unwrap(),
            r#"<input type=hidden name=sessionid value="[hidden]"><a href="/c?client=[mac]">x</a>"#
        );
        let big = "x".repeat(crate::logs::PORTAL_LAST_CAP + 50_000);
        crate::logs::save_portal_page(&d, &big).unwrap();
        assert_eq!(std::fs::metadata(d.join("logs/portal-last.html")).unwrap().len() as usize, crate::logs::PORTAL_LAST_CAP);
    }
}
