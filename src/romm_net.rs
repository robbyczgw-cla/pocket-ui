//! The requests of `romm`: one question, one answer ([`Net::call`]), and one file appended to its part ([`Net::fetch`]).
//!
//! * **Linux:** the system's curl, as the cover and update downloads. Its whole request (address, method, the token's
//!   header, the body) goes in through stdin as a curl config (`-K -`), so the token never shows in the process list.
//! * **Android:** Android's HTTPS for `https://` servers (`RommHttp.java`, the system's certificates), and a small HTTP/1.1
//!   client of our own ([`Plain`]) for a server at home under plain `http://`: the APK allows no clear-text traffic to
//!   Java's HTTP classes (`usesCleartextTraffic="false"`), and a RomM at home is usually `http://192.168.x.y:8080`.
//!
//! Every call has a time limit for connecting and one for a stalled transfer, and none runs on a UI or game thread.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::romm::Fetched;

/// An answer: the HTTP status (0: none came) and the body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Answer {
    pub fn none() -> Answer {
        Answer::default()
    }
}

/// The way to the server. `system` gives the platform's; tests answer from a list or a local server.
pub trait Net: Send + Sync {
    /// One request: `json` is sent as the body (a POST), `token` as `Authorization: Bearer`.
    fn call(&self, method: &str, url: &str, token: Option<&str>, json: Option<&str>) -> Answer;
    /// Appends `url`'s bytes to `part` from its current length (a range request), calling `progress` with the part's
    /// length as it grows. `size`: what the whole file should be (0: unknown). Ends early when `stop` is set.
    fn fetch(&self, url: &str, token: &str, part: &Path, size: u64, stop: &AtomicBool, progress: &(dyn Fn(u64) + Sync)) -> Fetched;
}

#[cfg(not(target_os = "android"))]
pub const PLATFORM: &str = "linux";
#[cfg(target_os = "android")]
pub const PLATFORM: &str = "android";

pub fn version() -> &'static str {
    crate::update::shown_version()
}

pub fn user_agent() -> String {
    format!("pocket-ui/{} ({PLATFORM})", version())
}

/// The most a list answer may be (a server of 50 000 games with their descriptions stays well under it).
const MAX_ANSWER: u64 = 96 << 20;
const CONNECT: Duration = Duration::from_secs(8);
/// A call that takes longer is given up.
const CALL_TIME: Duration = Duration::from_secs(60);
/// A transfer that moves fewer than [`STALL_BYTES`] in this long is given up (and resumed on the next start).
const STALL: Duration = Duration::from_secs(45);
const STALL_BYTES: u64 = 1024;

/// The platform's way to the server.
pub fn system() -> Box<dyn Net> {
    #[cfg(target_os = "android")]
    return Box::new(AndroidNet { plain: Plain::new() });
    #[cfg(not(target_os = "android"))]
    Box::new(Curl::new())
}

// ---- curl (Linux) -------------------------------------------------------------------------------------------------

/// A value in a curl config file: in quotes, with `\` and `"` escaped and no line breaks.
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
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The config curl reads from stdin: where, how, and the token's header.
fn curl_config(method: &str, url: &str, token: Option<&str>, json: Option<&str>) -> String {
    let mut cfg = format!("url = {}\n", quoted(url));
    if method != "GET" {
        cfg.push_str(&format!("request = {}\n", quoted(method)));
    }
    if let Some(t) = token {
        cfg.push_str(&format!("header = {}\n", quoted(&format!("Authorization: Bearer {t}"))));
    }
    cfg.push_str("header = \"Accept: application/json\"\n");
    if let Some(body) = json {
        cfg.push_str("header = \"Content-Type: application/json\"\n");
        cfg.push_str(&format!("data-binary = {}\n", quoted(body)));
    }
    cfg
}

/// Splits curl's output into the body and the status that `-w "\n%{http_code}"` put on the last line.
fn split_status(out: &[u8]) -> (Vec<u8>, u16) {
    let cut = out.iter().rposition(|b| *b == b'\n');
    let (body, code) = match cut {
        Some(i) => (&out[..i], &out[i + 1..]),
        None => (&out[..0], out),
    };
    let code = std::str::from_utf8(code).ok().and_then(|s| s.trim().parse::<u16>().ok()).unwrap_or(0);
    (body.to_vec(), code)
}

pub struct Curl {
    pub user_agent: String,
}

impl Curl {
    pub fn new() -> Curl {
        Curl { user_agent: user_agent() }
    }
}

impl Default for Curl {
    fn default() -> Self {
        Curl::new()
    }
}

impl Net for Curl {
    fn call(&self, method: &str, url: &str, token: Option<&str>, json: Option<&str>) -> Answer {
        use std::process::{Command, Stdio};
        let mut cmd = Command::new("curl");
        cmd.args(["-sS", "-L", "--max-redirs", "4", "--max-time"])
            .arg(CALL_TIME.as_secs().to_string())
            .arg("--connect-timeout")
            .arg(CONNECT.as_secs().to_string())
            .arg("--max-filesize")
            .arg(MAX_ANSWER.to_string())
            .arg("-A")
            .arg(&self.user_agent)
            .args(["-w", "\n%{http_code}", "-K", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let Ok(mut child) = cmd.spawn() else { return Answer::none() };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(curl_config(method, url, token, json).as_bytes());
        }
        let Ok(out) = child.wait_with_output() else { return Answer::none() };
        let (body, status) = split_status(&out.stdout);
        if !out.status.success() || status == 0 {
            return Answer::none();
        }
        Answer { status, body }
    }

    fn fetch(&self, url: &str, token: &str, part: &Path, size: u64, stop: &AtomicBool, progress: &(dyn Fn(u64) + Sync)) -> Fetched {
        use std::process::{Command, Stdio};
        let len = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
        let mut cmd = Command::new("curl");
        cmd.args(["-sS", "-f", "-L", "--max-redirs", "4", "--connect-timeout"])
            .arg(CONNECT.as_secs().to_string())
            .args(["--speed-limit", &STALL_BYTES.to_string(), "--speed-time", &STALL.as_secs().to_string()])
            .arg("-A")
            .arg(&self.user_agent)
            .args(["-w", "%{http_code}"]);
        if size > 0 {
            cmd.arg("--max-filesize").arg(size.to_string());
        }
        if len(part) > 0 {
            cmd.args(["-C", "-"]);
        }
        cmd.arg("-o").arg(part).args(["-K", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let Ok(mut child) = cmd.spawn() else { return Fetched::Unreachable };
        if let Some(mut stdin) = child.stdin.take() {
            // No token (ScreenScraper's pictures, `screenscraper`): no Authorization header at all.
            let _ = stdin.write_all(curl_config("GET", url, Some(token).filter(|t| !t.is_empty()), None).as_bytes());
        }
        let mut stdout = child.stdout.take();
        loop {
            progress(len(part));
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut out = Vec::new();
                    if let Some(s) = stdout.as_mut() {
                        let _ = s.read_to_end(&mut out);
                    }
                    let code = std::str::from_utf8(&out).ok().and_then(|s| s.trim().parse::<u16>().ok()).unwrap_or(0);
                    progress(len(part));
                    return match status.code() {
                        Some(0) if matches!(code, 200 | 206) => Fetched::Done,
                        Some(0) => Fetched::Status(code),
                        Some(22) => Fetched::Status(code),
                        Some(6 | 7) => Fetched::Unreachable,
                        Some(28) if len(part) == 0 && code == 0 => Fetched::Unreachable,
                        Some(23) => write_failure(part),
                        // The server can't go on from the middle (no ranges): the next try starts the file anew.
                        Some(33) => {
                            let _ = std::fs::remove_file(part);
                            Fetched::Broke
                        }
                        // 63: bigger than RomM said; the size check after says so.
                        Some(63) => Fetched::Done,
                        _ => Fetched::Broke,
                    };
                }
                Ok(None) if stop.load(Ordering::Relaxed) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Fetched::Stopped;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                Err(_) => return Fetched::Broke,
            }
        }
    }
}

/// A write that failed: full (or as good as), or refused.
fn write_failure(part: &Path) -> Fetched {
    let dir = part.parent().unwrap_or(part);
    match crate::storage_space::free_space(dir) {
        Some((free, _)) if free < 4 << 20 => Fetched::NoSpace,
        _ => Fetched::CantWrite,
    }
}

// ---- plain HTTP (Android at home, and the tests) ------------------------------------------------------------------

/// `http://host:port/path?query` taken apart; None for anything else.
fn split_http(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (hostport, path) = match rest.find(['/', '?']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let path = if path.starts_with('?') { format!("/{path}") } else { path.to_string() };
    let (host, port) = if let Some(h) = hostport.strip_prefix('[') {
        let (h, after) = h.split_once(']')?;
        (h.to_string(), after.strip_prefix(':').map_or(Some(80), |p| p.parse().ok())?)
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().ok()?),
            None => (hostport.to_string(), 80),
        }
    };
    (!host.is_empty() && !host.contains('@')).then_some((host, port, path))
}

/// The head of an answer: status and the headers we look at.
struct Head {
    status: u16,
    length: Option<u64>,
    chunked: bool,
    location: Option<String>,
    range_start: Option<u64>,
}

/// A small HTTP/1.1 client: one connection per request (`Connection: close`), lengths and chunked bodies, redirects
/// within plain HTTP. Only `http://`; nothing else is ever sent by it.
pub struct Plain {
    pub user_agent: String,
}

impl Plain {
    pub fn new() -> Plain {
        Plain { user_agent: user_agent() }
    }

    fn connect(host: &str, port: u16) -> std::io::Result<std::net::TcpStream> {
        use std::net::ToSocketAddrs;
        let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, "no address");
        for addr in (host, port).to_socket_addrs()? {
            match std::net::TcpStream::connect_timeout(&addr, CONNECT) {
                Ok(s) => {
                    s.set_read_timeout(Some(STALL))?;
                    s.set_write_timeout(Some(STALL))?;
                    return Ok(s);
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Sends one request and reads the head; the body is left in the reader.
    fn send(&self, method: &str, url: &str, token: Option<&str>, json: Option<&str>, from: u64) -> std::io::Result<(Head, BufReader<std::net::TcpStream>)> {
        let bad = || std::io::Error::new(std::io::ErrorKind::InvalidInput, "not an http address");
        let (host, port, path) = split_http(url).ok_or_else(bad)?;
        if path.contains(['\r', '\n', ' ']) || method.contains(['\r', '\n', ' ']) {
            return Err(bad());
        }
        let mut stream = Self::connect(&host, port)?;
        let host_header = if host.contains(':') { format!("[{host}]") } else { host.clone() };
        let host_header = if port == 80 { host_header } else { format!("{host_header}:{port}") };
        let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: {}\r\nAccept: application/json\r\nAccept-Encoding: identity\r\nConnection: close\r\n", self.user_agent);
        if let Some(t) = token.filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_graphic())) {
            req.push_str(&format!("Authorization: Bearer {t}\r\n"));
        }
        if from > 0 {
            req.push_str(&format!("Range: bytes={from}-\r\n"));
        }
        match json {
            Some(body) => req.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len())),
            None if method != "GET" => req.push_str("Content-Length: 0\r\n\r\n"),
            None => req.push_str("\r\n"),
        }
        stream.write_all(req.as_bytes())?;
        let mut reader = BufReader::new(stream);
        let head = read_head(&mut reader)?;
        Ok((head, reader))
    }

    /// [`send`] with up to 4 redirects inside plain HTTP (a redirect elsewhere is the answer).
    fn send_following(&self, method: &str, url: &str, token: Option<&str>, json: Option<&str>, from: u64) -> std::io::Result<(Head, BufReader<std::net::TcpStream>)> {
        let mut url = url.to_string();
        let mut method = method.to_string();
        let mut json = json.map(str::to_string);
        for _ in 0..4 {
            let (head, reader) = self.send(&method, &url, token, json.as_deref(), from)?;
            let next = head.location.as_deref().filter(|_| matches!(head.status, 301 | 302 | 303 | 307 | 308)).and_then(|l| join_location(&url, l));
            match next {
                Some(n) if n.starts_with("http://") && same_host(&url, &n) => {
                    if matches!(head.status, 301..=303) {
                        method = "GET".into();
                        json = None;
                    }
                    url = n;
                }
                _ => return Ok((head, reader)),
            }
        }
        self.send(&method, &url, token, json.as_deref(), from)
    }
}

impl Default for Plain {
    fn default() -> Self {
        Plain::new()
    }
}

/// The token only ever goes back to the host it was given for.
fn same_host(a: &str, b: &str) -> bool {
    match (split_http(a), split_http(b)) {
        (Some((h1, p1, _)), Some((h2, p2, _))) => h1.eq_ignore_ascii_case(&h2) && p1 == p2,
        _ => false,
    }
}

/// A `Location` against the address it came from.
fn join_location(base: &str, location: &str) -> Option<String> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Some(location.to_string());
    }
    let (host, port, _) = split_http(base)?;
    let host = if host.contains(':') { format!("[{host}]") } else { host };
    location.starts_with('/').then(|| format!("http://{host}:{port}{location}"))
}

fn read_head(r: &mut impl BufRead) -> std::io::Result<Head> {
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());
    let mut line = String::new();
    let mut total = 0usize;
    let mut next = |r: &mut dyn BufRead, line: &mut String| -> std::io::Result<()> {
        line.clear();
        let n = r.take(8192).read_line(line)?;
        total += n;
        if n == 0 || total > 64 * 1024 {
            return Err(bad("head"));
        }
        Ok(())
    };
    next(r, &mut line)?;
    let status = line.split_whitespace().nth(1).and_then(|s| s.parse::<u16>().ok()).filter(|_| line.starts_with("HTTP/1.")).ok_or_else(|| bad("status line"))?;
    let mut head = Head { status, length: None, chunked: false, location: None, range_start: None };
    loop {
        next(r, &mut line)?;
        let l = line.trim_end_matches(['\r', '\n']);
        if l.is_empty() {
            break;
        }
        let Some((k, v)) = l.split_once(':') else { continue };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        match k.as_str() {
            "content-length" => head.length = v.parse().ok(),
            "transfer-encoding" => head.chunked = v.to_ascii_lowercase().contains("chunked"),
            "location" => head.location = Some(v.to_string()),
            "content-range" => head.range_start = v.strip_prefix("bytes ").and_then(|r| r.split('-').next()).and_then(|s| s.trim().parse().ok()),
            _ => {}
        }
    }
    Ok(head)
}

/// Copies the body of `head` from `r` into `out`, at most `max` bytes; calls `each` after every piece.
fn copy_body(head: &Head, r: &mut impl BufRead, out: &mut dyn Write, max: u64, each: &mut dyn FnMut(u64) -> bool) -> std::io::Result<u64> {
    let too_big = || std::io::Error::new(std::io::ErrorKind::InvalidData, "too big");
    let stopped = || std::io::Error::new(std::io::ErrorKind::Interrupted, "stopped");
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    let mut copy = |r: &mut dyn Read, mut left: Option<u64>, done: &mut u64| -> std::io::Result<()> {
        loop {
            let want = left.map_or(buf.len(), |l| (l as usize).min(buf.len()));
            if want == 0 {
                return Ok(());
            }
            let n = r.read(&mut buf[..want])?;
            if n == 0 {
                return if left.is_some() { Err(std::io::ErrorKind::UnexpectedEof.into()) } else { Ok(()) };
            }
            *done += n as u64;
            if *done > max {
                return Err(too_big());
            }
            out.write_all(&buf[..n])?;
            if let Some(l) = left.as_mut() {
                *l -= n as u64;
            }
            if !each(*done) {
                return Err(stopped());
            }
        }
    };
    if head.chunked {
        loop {
            let mut size_line = String::new();
            r.take(1024).read_line(&mut size_line)?;
            let size = u64::from_str_radix(size_line.trim().split(';').next().unwrap_or("").trim(), 16).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "chunk"))?;
            if size == 0 {
                let mut rest = String::new();
                let _ = r.take(1024).read_line(&mut rest);
                return Ok(done);
            }
            copy(r, Some(size), &mut done)?;
            let mut crlf = [0u8; 2];
            r.read_exact(&mut crlf)?;
        }
    }
    copy(r, head.length, &mut done)?;
    Ok(done)
}

impl Net for Plain {
    fn call(&self, method: &str, url: &str, token: Option<&str>, json: Option<&str>) -> Answer {
        let started = Instant::now();
        let Ok((head, mut r)) = self.send_following(method, url, token, json, 0) else { return Answer::none() };
        let mut body = Vec::new();
        let ok = copy_body(&head, &mut r, &mut body, MAX_ANSWER, &mut |_| started.elapsed() < CALL_TIME).is_ok();
        if !ok && (200..300).contains(&head.status) {
            return Answer::none();
        }
        Answer { status: head.status, body }
    }

    fn fetch(&self, url: &str, token: &str, part: &Path, size: u64, stop: &AtomicBool, progress: &(dyn Fn(u64) + Sync)) -> Fetched {
        let have = std::fs::metadata(part).map_or(0, |m| m.len());
        progress(have);
        let (head, mut r) = match self.send_following("GET", url, Some(token), None, have) {
            Ok(x) => x,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::InvalidInput) => return Fetched::Unreachable,
            Err(_) if have == 0 => return Fetched::Unreachable,
            Err(_) => return Fetched::Broke,
        };
        let append = head.status == 206 && head.range_start == Some(have) && have > 0;
        if head.status != 200 && !append {
            return Fetched::Status(head.status);
        }
        // A server that sent the whole file again: the part starts over.
        let file = std::fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(part);
        let Ok(mut file) = file else { return Fetched::CantWrite };
        let base = if append { have } else { 0 };
        let max = if size > 0 { size.saturating_sub(base) } else { u64::MAX };
        let mut moved_at = (Instant::now(), 0u64);
        let result = copy_body(&head, &mut r, &mut file, max, &mut |n| {
            progress(base + n);
            if n - moved_at.1 >= STALL_BYTES {
                moved_at = (Instant::now(), n);
            }
            !stop.load(Ordering::Relaxed) && moved_at.0.elapsed() < STALL
        });
        let _ = file.sync_all();
        match result {
            Ok(_) => Fetched::Done,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted && stop.load(Ordering::Relaxed) => Fetched::Stopped,
            Err(e) if e.raw_os_error() == Some(libc::ENOSPC) => Fetched::NoSpace,
            Err(e) if e.raw_os_error().is_some_and(|c| c == libc::EROFS || c == libc::EACCES) => Fetched::CantWrite,
            Err(_) => Fetched::Broke,
        }
    }
}

// ---- Android ------------------------------------------------------------------------------------------------------

/// What the Activity answers: `OK\n<status>\n<body>`, or `ERR\n<why>` when nothing came back.
pub fn parse_bridge(reply: &str) -> Answer {
    let Some(rest) = reply.strip_prefix("OK\n") else { return Answer::none() };
    let (status, body) = rest.split_once('\n').unwrap_or((rest, ""));
    match status.trim().parse::<u16>() {
        Ok(status) if status > 0 => Answer { status, body: body.as_bytes().to_vec() },
        _ => Answer::none(),
    }
}

/// The screen kept on while a RomM game downloads (`PocketBase.awakeHold`): on when made, off when dropped.
#[cfg(target_os = "android")]
struct AwakeHold(bool);

#[cfg(target_os = "android")]
impl AwakeHold {
    fn new() -> Self {
        AwakeHold(crate::platform::android::strings("awakeHold", &["on"]).is_ok())
    }
}

#[cfg(target_os = "android")]
impl Drop for AwakeHold {
    fn drop(&mut self) {
        if self.0 {
            let _ = crate::platform::android::strings("awakeHold", &["off"]);
        }
    }
}

/// Android: `https://` through the Activity (`RommHttp.java`), plain `http://` through [`Plain`].
#[cfg(target_os = "android")]
pub struct AndroidNet {
    pub plain: Plain,
}

#[cfg(target_os = "android")]
impl Net for AndroidNet {
    fn call(&self, method: &str, url: &str, token: Option<&str>, json: Option<&str>) -> Answer {
        if url.starts_with("http://") {
            return self.plain.call(method, url, token, json);
        }
        match crate::platform::android::strings("rommRequest", &[method, url, token.unwrap_or(""), json.unwrap_or(""), &self.plain.user_agent]) {
            Ok(reply) => parse_bridge(&reply),
            Err(_) => Answer::none(),
        }
    }

    fn fetch(&self, url: &str, token: &str, part: &Path, size: u64, stop: &AtomicBool, progress: &(dyn Fn(u64) + Sync)) -> Fetched {
        // The screen stays on for the whole download, https or a LAN server's plain http (`Awake.java`), off again however it ends.
        let _awake = AwakeHold::new();
        if url.starts_with("http://") {
            return self.plain.fetch(url, token, part, size, stop, progress);
        }
        let Some(part_str) = part.to_str() else { return Fetched::CantWrite };
        // Java looks for this file between two pieces: it is how B stops it.
        let stop_file = part.with_extension("stop");
        let _ = std::fs::remove_file(&stop_file);
        let done = AtomicBool::new(false);
        let reply = std::thread::scope(|s| {
            s.spawn(|| {
                while !done.load(Ordering::Relaxed) {
                    progress(std::fs::metadata(part).map_or(0, |m| m.len()));
                    if stop.load(Ordering::Relaxed) {
                        let _ = std::fs::write(&stop_file, b"");
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            });
            let reply = crate::platform::android::strings("rommFetch", &[url, token, part_str, stop_file.to_str().unwrap_or(""), &size.to_string(), &self.plain.user_agent]);
            done.store(true, Ordering::Relaxed);
            reply
        });
        let _ = std::fs::remove_file(&stop_file);
        progress(std::fs::metadata(part).map_or(0, |m| m.len()));
        match reply.as_deref() {
            Ok("OK") => Fetched::Done,
            Ok(r) if r.starts_with("HTTP ") => Fetched::Status(r[5..].trim().parse().unwrap_or(0)),
            Ok("ERR\nunreachable") => Fetched::Unreachable,
            Ok("ERR\nstopped") => Fetched::Stopped,
            Ok("ERR\nnospace") => Fetched::NoSpace,
            Ok("ERR\nwrite") => Fetched::CantWrite,
            _ if stop.load(Ordering::Relaxed) => Fetched::Stopped,
            _ => Fetched::Broke,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_goes_through_stdin_quoted() {
        let cfg = curl_config("POST", "http://h:1/api/x?a=1", Some("rmm_abc"), Some(r#"{"code":"1\"2"}"#));
        assert!(cfg.contains("url = \"http://h:1/api/x?a=1\"\n"));
        assert!(cfg.contains("request = \"POST\"\n"));
        assert!(cfg.contains("header = \"Authorization: Bearer rmm_abc\"\n"));
        assert!(cfg.contains(r#"data-binary = "{\"code\":\"1\\\"2\"}""#), "{cfg}");
        assert!(!curl_config("GET", "http://h/", None, None).contains("request"));
    }

    #[test]
    fn the_status_comes_off_the_last_line() {
        assert_eq!(split_status(b"{\"a\":1}\n200"), (b"{\"a\":1}".to_vec(), 200));
        assert_eq!(split_status(b"a\nb\n401"), (b"a\nb".to_vec(), 401));
        assert_eq!(split_status(b"000"), (Vec::new(), 0));
        assert_eq!(split_status(b""), (Vec::new(), 0));
    }

    #[test]
    fn http_addresses_come_apart() {
        assert_eq!(split_http("http://192.0.2.10:8080/api/x?y=1"), Some(("192.0.2.10".into(), 8080, "/api/x?y=1".into())));
        assert_eq!(split_http("http://romm.home"), Some(("romm.home".into(), 80, "/".into())));
        assert_eq!(split_http("http://[2001:db8::1]:81/a"), Some(("2001:db8::1".into(), 81, "/a".into())));
        assert_eq!(split_http("http://h?x"), Some(("h".into(), 80, "/?x".into())));
        assert_eq!(split_http("https://h/"), None);
        assert_eq!(split_http("http://user@h/"), None);
        assert_eq!(split_http("http://h:port/"), None);
    }

    #[test]
    fn redirects_keep_the_token_on_its_host() {
        assert!(same_host("http://a:8080/x", "http://A:8080/y"));
        assert!(!same_host("http://a:8080/x", "http://b:8080/x"));
        assert!(!same_host("http://a:8080/x", "http://a:80/x"));
        assert_eq!(join_location("http://a:8080/x", "/api/y").as_deref(), Some("http://a:8080/api/y"));
        assert_eq!(join_location("http://a/x", "https://a/x").as_deref(), Some("https://a/x"));
    }

    #[test]
    fn heads_and_chunked_bodies_are_read() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nX: y\r\n\r\n5\r\nhello\r\n6;ext\r\n world\r\n0\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        let head = read_head(&mut r).unwrap();
        assert_eq!((head.status, head.chunked), (200, true));
        let mut out = Vec::new();
        copy_body(&head, &mut r, &mut out, 100, &mut |_| true).unwrap();
        assert_eq!(out, b"hello world");
        let raw = b"HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nContent-Range: bytes 7-9/10\r\n\r\nabcdef";
        let mut r = BufReader::new(&raw[..]);
        let head = read_head(&mut r).unwrap();
        assert_eq!((head.status, head.length, head.range_start), (206, Some(3), Some(7)));
        let mut out = Vec::new();
        copy_body(&head, &mut r, &mut out, 100, &mut |_| true).unwrap();
        assert_eq!(out, b"abc");
        let short = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabc";
        let mut r = BufReader::new(&short[..]);
        let head = read_head(&mut r).unwrap();
        assert!(copy_body(&head, &mut r, &mut Vec::new(), 100, &mut |_| true).is_err(), "a body cut short is an error");
        assert!(read_head(&mut BufReader::new(&b"garbage\r\n\r\n"[..])).is_err());
    }

    /// What a server sends is read like a file from the card: an answer's bytes through the shared faults (missing, empty,
    /// cut, flipped, another encoding) never panic, and the good one is read whole.
    #[test]
    fn file_source_rule_http_answers() {
        let seed = b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}";
        let chunked = b"HTTP/1.1 206 Partial Content\r\nTransfer-Encoding: chunked\r\nContent-Range: bytes 4-9/10\r\n\r\n6\r\nVOXELS\r\n0\r\n\r\n";
        for seed in [&seed[..], &chunked[..]] {
            for (label, bytes) in crate::file_faults::faults(seed) {
                let bytes = bytes.unwrap_or_default();
                let mut r = BufReader::new(&bytes[..]);
                let Ok(head) = read_head(&mut r) else { continue };
                let mut out = Vec::new();
                let got = copy_body(&head, &mut r, &mut out, 1 << 20, &mut |_| true);
                if label == "valid" {
                    assert!(got.is_ok() && (out == b"{\"ok\":true}" || out == b"VOXELS"), "{label}: {out:?}");
                }
            }
        }
    }

    #[test]
    fn the_bridges_answer_becomes_an_answer() {
        assert_eq!(parse_bridge("OK\n200\n{}"), Answer { status: 200, body: b"{}".to_vec() });
        assert_eq!(parse_bridge("OK\n401\n").status, 401);
        for none in ["", "ERR\nunreachable", "OK\nx\n", "OK\n0\n"] {
            assert_eq!(parse_bridge(none).status, 0, "{none:?}");
        }
    }
}
