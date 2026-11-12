//! Talking to retroachievements.org: one request, one answer, through the system's curl (as the cover download does).
//! Every call has a connect timeout and an overall one, and none runs on a UI or game thread.
//!
//! Etiquette (RetroAchievements' rules for API clients): a User-Agent that names the client and its version, the
//! core and rcheevos ("pocket-ui/0.4.1 (Linux) mGBA/0.10.3 rcheevos/12.5.0"); requests only as rc_client needs them
//! (login, the game's data, its session, unlocks and the rich-presence ping every two minutes), one at a time;
//! retries back off (rc_client doubles the wait up to two minutes); nothing is fetched in bulk.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::ffi;

/// One request as rcheevos built it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    /// Sent as a POST when there is some, else a GET.
    pub post_data: Option<String>,
    pub content_type: Option<String>,
}

/// The answer: the HTTP status, or `ffi::RETRYABLE_CLIENT_ERROR` when nothing came back (no network, timeout).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: i32,
    pub body: Vec<u8>,
}

impl Response {
    pub fn unreachable() -> Response {
        Response {
            status: ffi::RETRYABLE_CLIENT_ERROR,
            body: Vec::new(),
        }
    }

    pub fn ok(body: impl Into<Vec<u8>>) -> Response {
        Response {
            status: 200,
            body: body.into(),
        }
    }

    /// The request never got an answer from the server (or only a gateway's).
    pub fn failed(&self) -> bool {
        self.status <= 0 || (self.status >= 500 && self.body.is_empty()) || matches!(self.status, 502..=504 | 521..=525)
    }
}

/// Sends requests. `Curl` on the device; tests answer with canned responses.
pub trait Transport: Send + Sync {
    fn call(&self, req: &Request) -> Response;

    /// Downloads a file (an achievement's badge) to `dest`; true when it is there.
    fn download(&self, _url: &str, _dest: &std::path::Path) -> bool {
        false
    }
}

#[cfg(not(target_os = "android"))]
const PLATFORM: &str = "Linux";
#[cfg(target_os = "android")]
const PLATFORM: &str = "Android";

#[cfg(not(target_os = "android"))]
fn app_version() -> &'static str {
    crate::update::app_version()
}
#[cfg(target_os = "android")]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The User-Agent: client and version, the platform, the core and rcheevos.
pub fn user_agent(core: Option<(&str, &str)>) -> String {
    let core = match core {
        Some((name, version)) if !name.is_empty() => {
            let name: String = name.chars().filter(|c| !c.is_whitespace()).collect();
            let version: String = version.chars().filter(|c| !c.is_whitespace()).collect();
            if version.is_empty() {
                format!(" {name}")
            } else {
                format!(" {name}/{version}")
            }
        }
        _ => String::new(),
    };
    format!("pocket-ui/{} ({PLATFORM}){core} rcheevos/{}", app_version(), ffi::version())
}

/// The system's curl. The POST body goes in through stdin, so a password or token never shows in the process list.
pub struct Curl {
    pub user_agent: String,
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

impl Curl {
    pub fn new(user_agent: String) -> Curl {
        Curl {
            user_agent,
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(8),
        }
    }

    /// Downloads `url` (a badge) to `dest` through a temporary file; true when it is there.
    fn fetch(&self, url: &str, dest: &std::path::Path) -> bool {
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let tmp = dest.with_extension("part");
        let ok = Command::new("curl")
            .args(["-fsSL", "--max-time", "15", "--connect-timeout"])
            .arg(self.connect_timeout.as_secs().to_string())
            .arg("-A")
            .arg(&self.user_agent)
            .arg("-o")
            .arg(&tmp)
            .arg(url)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
            && std::fs::rename(&tmp, dest).is_ok();
        std::fs::remove_file(&tmp).ok();
        ok
    }
}

/// Splits curl's output into the body and the status that `-w "\n%{http_code}"` put on the last line.
fn split_status(out: &[u8]) -> (Vec<u8>, i32) {
    let cut = out.iter().rposition(|b| *b == b'\n').unwrap_or(0);
    let code = std::str::from_utf8(&out[(cut + 1).min(out.len())..])
        .ok()
        .and_then(|s| s.trim().parse::<i32>().ok())
        .unwrap_or(0);
    let body = if cut == 0 && out.first() != Some(&b'\n') {
        Vec::new()
    } else {
        out[..cut].to_vec()
    };
    (body, code)
}

impl Transport for Curl {
    fn call(&self, req: &Request) -> Response {
        let mut cmd = Command::new("curl");
        cmd.args(["-sS", "--max-time"])
            .arg(self.timeout.as_secs().to_string())
            .arg("--connect-timeout")
            .arg(self.connect_timeout.as_secs().to_string())
            .arg("-A")
            .arg(&self.user_agent)
            .args(["-w", "\n%{http_code}"]);
        if req.post_data.is_some() {
            let ct = req.content_type.as_deref().unwrap_or("application/x-www-form-urlencoded");
            cmd.arg("-H").arg(format!("Content-Type: {ct}")).args(["--data-binary", "@-"]);
        }
        cmd.arg(&req.url).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let Ok(mut child) = cmd.spawn() else { return Response::unreachable() };
        if let (Some(mut stdin), Some(post)) = (child.stdin.take(), req.post_data.as_ref()) {
            let _ = stdin.write_all(post.as_bytes());
        }
        let Ok(out) = child.wait_with_output() else { return Response::unreachable() };
        let (body, status) = split_status(&out.stdout);
        if !out.status.success() || status == 0 {
            return Response::unreachable();
        }
        Response { status, body }
    }

    fn download(&self, url: &str, dest: &std::path::Path) -> bool {
        self.fetch(url, dest)
    }
}

/// What the Android Activity answers a request with: `OK\n<status>\n<body>`, or `ERR\n<why>` when nothing came back
/// (no network, timeout, a certificate that isn't trusted). Anything else counts as no answer.
pub fn parse_bridge_reply(reply: &str) -> Response {
    let Some(rest) = reply.strip_prefix("OK\n") else { return Response::unreachable() };
    let (status, body) = rest.split_once('\n').unwrap_or((rest, ""));
    match status.trim().parse::<i32>() {
        Ok(status) if status > 0 => Response { status, body: body.as_bytes().to_vec() },
        _ => Response::unreachable(),
    }
}

/// Android's HTTPS (the system's certificate store) through the Activity (`RaHttp.java`): the same one-request,
/// one-answer shape as curl. The POST body travels as an argument of the call, never in a file or a log line.
#[cfg(target_os = "android")]
pub struct AndroidHttp {
    pub user_agent: String,
}

#[cfg(target_os = "android")]
impl Transport for AndroidHttp {
    fn call(&self, req: &Request) -> Response {
        let content_type = req.content_type.as_deref().unwrap_or("application/x-www-form-urlencoded");
        let post = req.post_data.as_deref().unwrap_or("");
        let reply = crate::platform::android::strings(
            "raRequest",
            &[&req.url, post, if req.post_data.is_some() { content_type } else { "" }, &self.user_agent],
        );
        match reply {
            Ok(reply) => parse_bridge_reply(&reply),
            Err(_) => Response::unreachable(),
        }
    }

    fn download(&self, url: &str, dest: &std::path::Path) -> bool {
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let tmp = dest.with_extension("part");
        let Some(tmp_str) = tmp.to_str() else { return false };
        let ok = crate::platform::android::strings("raDownload", &[url, tmp_str, &self.user_agent]).is_ok_and(|r| r.starts_with("OK")) && std::fs::rename(&tmp, dest).is_ok();
        std::fs::remove_file(&tmp).ok();
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_comes_off_the_last_line() {
        assert_eq!(split_status(b"{\"Success\":true}\n200"), (b"{\"Success\":true}".to_vec(), 200));
        assert_eq!(split_status(b"line one\nline two\n503"), (b"line one\nline two".to_vec(), 503));
        assert_eq!(split_status(b"\n000"), (Vec::new(), 0));
        assert_eq!(split_status(b""), (Vec::new(), 0));
    }

    #[test]
    fn the_user_agent_names_client_core_and_rcheevos() {
        let ua = user_agent(Some(("mGBA", "0.10.3 abc")));
        assert!(ua.starts_with("pocket-ui/"), "{ua}");
        assert!(ua.contains(" (Linux) mGBA/0.10.3abc rcheevos/12."), "{ua}");
        assert!(!user_agent(None).contains("  "));
        assert!(user_agent(Some(("Snes9x", ""))).contains(" Snes9x rcheevos/"));
    }

    #[test]
    fn the_activitys_answer_becomes_a_response() {
        assert_eq!(parse_bridge_reply("OK\n200\n{\"Success\":true}"), Response::ok("{\"Success\":true}"));
        assert_eq!(parse_bridge_reply("OK\n401\n{\"Success\":false}\nsecond line").status, 401);
        assert_eq!(parse_bridge_reply("OK\n204\n").body, Vec::<u8>::new());
        for none in ["", "ERR\ntimeout", "OK\nsoon\nbody", "OK\n0\n", "garbage"] {
            assert!(parse_bridge_reply(none).failed(), "{none:?} is no answer");
        }
    }

    #[test]
    fn what_counts_as_no_answer() {
        assert!(Response::unreachable().failed());
        assert!(Response { status: 503, body: Vec::new() }.failed());
        assert!(Response {
            status: 502,
            body: b"<html>".to_vec()
        }
        .failed());
        assert!(
            !Response {
                status: 401,
                body: b"{}".to_vec()
            }
            .failed(),
            "the server answered"
        );
        assert!(!Response::ok("{}").failed());
    }
}
