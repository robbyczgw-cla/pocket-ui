//! Is this Wi-Fi a hotel, train or café network that wants a sign-in in a browser? pocket-ui has no browser, and
//! until the sign-in is done everything online fails in ways that look like "no internet". NetworkManager's own
//! community address answers a plain HTTP request with `200` and the text "NetworkManager is online" (what
//! NetworkManager's connectivity check expects); a network with a sign-in page answers with a redirect or a page of
//! its own. No Google address is asked, and nothing but this one request is sent (no identifier, no data of the Pocket).
//!
//! Nothing is sent until something online has failed or the Wi-Fi page shows a connected network, and never from the
//! UI thread: `probe` runs `curl` for up to [`MAX_TIME`] seconds. An answer is kept for [`KEEP`] so a page that fails
//! three times in a row asks once. While the Wi-Fi page shows a sign-in, it asks again every 15 s ([`recheck`],
//! `setup`), so the page turns to online by itself once the network lets the Pocket in; that stops with the answer
//! online or when the page closes. The sign-in itself is `portal` and `portal_page`.

use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// NetworkManager's connectivity check page (`[connectivity] uri` of its community default).
pub const PROBE_URL: &str = "http://nmcheck.gnome.org/check_network_status.txt";
/// What it says (the file has no newline today; one at the end is ignored).
pub const ONLINE_BODY: &str = "NetworkManager is online";
const HOST: &str = "nmcheck.gnome.org";
const MAX_TIME: &str = "4";
const KEEP: Duration = Duration::from_secs(20);

/// What the network does with a plain web request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Net {
    /// The page came back as it should: the internet is there.
    Online,
    /// A redirect or a page of its own: the network wants a sign-in.
    Portal,
    /// No answer at all (no route, no DNS, a timeout): offline, or the page is blocked.
    Unreachable,
    /// `curl` is missing or said something else.
    Unknown,
}

/// What the errors (updates, store, PortMaster) say about a network with a sign-in: where it can be done.
pub const PORTAL_MESSAGE: &str = "Connected, but this network wants a sign-in first. Open Settings › Connections › Wi-Fi.";

/// What `probe_with` makes of the answer: the status, the body and where a redirect leads ("" for none).
/// - 200 and the expected text: online.
/// - A redirect: a sign-in page, unless it only leads on within the check's own host (http to https there: not what
///   NetworkManager's check follows, and not a sign-in either).
/// - Any other body or status: a sign-in page too (a network that answers every address with its own page), except an
///   error of the page's own server (404, 5xx), which says nothing about this network.
pub fn classify(code: u32, body: &str, redirect: &str) -> Net {
    match code {
        200 if body.trim() == ONLINE_BODY => Net::Online,
        200 => Net::Portal,
        301 | 302 | 303 | 307 | 308 => {
            let host = redirect.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
            if host.eq_ignore_ascii_case(HOST) {
                Net::Unknown
            } else {
                Net::Portal
            }
        }
        0 => Net::Unknown,
        404 | 500..=599 => Net::Unknown,
        _ => Net::Portal,
    }
}

/// What one request found: the answer, and for a sign-in that redirects, where to (the sign-in page's address).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub net: Net,
    /// The redirect's target, kept only for a sign-in, only when it is an http or https address and not longer
    /// than [`MAX_URL`]. None for a network that answers with a page of its own: the check's own address is that page.
    pub portal: Option<String>,
}

/// The longest sign-in address kept; a longer one is no address a person needs to see.
pub const MAX_URL: usize = 2048;

/// The redirect of a sign-in answer, if it is worth keeping (`Check::portal`).
fn portal_address(net: Net, redirect: &str) -> Option<String> {
    let r = redirect.trim();
    let web = r.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("http://")) || r.get(..8).is_some_and(|p| p.eq_ignore_ascii_case("https://"));
    (net == Net::Portal && web && r.len() <= MAX_URL && !r.chars().any(|c| c.is_control() || c == ' ')).then(|| r.to_string())
}

/// "login.hotel.example" for "http://login.hotel.example:8080/start?x=1": the host of an address, for the Wi-Fi page.
pub fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host.starts_with('[') { host.split_once(']').map_or(host, |(h, _)| &h[1..]) } else { host.split(':').next().unwrap_or(host) };
    (!host.is_empty()).then_some(host)
}

/// Asks `curl` once. Blocks: a worker's job.
pub fn probe_with(curl: &str) -> Net {
    check_with(curl, PROBE_URL).net
}

/// `probe_with` for any check address (the end-to-end test's stand-in portal), with the sign-in page's address.
pub fn check_with(curl: &str, url: &str) -> Check {
    let out = crate::proc::run_with_timeout(
        Command::new(curl).args([
            "-s", "--noproxy", "*", "--max-time", MAX_TIME, "--max-filesize", "4096", "-w", "\n%{http_code} %{redirect_url}", url,
        ]),
        Duration::from_secs(6),
    );
    let only = |net| Check { net, portal: None };
    let Ok(out) = out else { return only(Net::Unknown) };
    if !out.status.success() {
        return only(match out.status.code() {
            // 63: the page is bigger than anything this one is: a sign-in page.
            Some(63) => Net::Portal,
            // curl ran and the request failed: resolve, connect, timeout.
            Some(_) => Net::Unreachable,
            None => Net::Unknown,
        });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let Some((body, tail)) = text.rsplit_once('\n') else { return only(Net::Unknown) };
    let (code, redirect) = tail.split_once(' ').unwrap_or((tail, ""));
    match code.parse() {
        Ok(code) => {
            let net = classify(code, body, redirect.trim());
            Check { net, portal: portal_address(net, redirect) }
        }
        Err(_) => only(Net::Unknown),
    }
}

static LAST: Mutex<Option<(Instant, Check)>> = Mutex::new(None);

/// `probe_with("curl")`, remembered for [`KEEP`].
pub fn probe() -> Net {
    probe_check().net
}

/// `probe`, with the sign-in page's address.
pub fn probe_check() -> Check {
    if let Some((at, check)) = LAST.lock().unwrap().as_ref() {
        if at.elapsed() < KEEP {
            return check.clone();
        }
    }
    recheck()
}

/// Asks again now, whatever is remembered, and remembers the answer: the Wi-Fi page's re-check while it shows a
/// sign-in, and the check after the sign-in helper sent a form.
pub fn recheck() -> Check {
    let check = check_with("curl", PROBE_URL);
    *LAST.lock().unwrap() = Some((Instant::now(), check.clone()));
    check
}

/// Forgets the answer: a different network was joined.
pub fn forget() {
    *LAST.lock().unwrap() = None;
}

/// For a failure that says "Check Wi-Fi": the sign-in message when the network wants one, else the message as it is.
/// Call it where the failure is already known, on the thread that found it.
pub fn refine(generic: &str) -> String {
    refine_with(generic, probe())
}

fn refine_with(generic: &str, net: Net) -> String {
    if net == Net::Portal {
        PORTAL_MESSAGE.to_string()
    } else {
        generic.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::write_executable;
    use crate::testutil::TestDir;

    #[test]
    fn the_expected_text_is_online_a_redirect_or_another_page_is_a_sign_in() {
        assert_eq!(classify(200, "NetworkManager is online", ""), Net::Online);
        assert_eq!(classify(200, "NetworkManager is online\n", ""), Net::Online, "a newline at the end");
        assert_eq!(classify(200, "<html>Welcome to the hotel</html>", ""), Net::Portal, "a page of the network's own");
        assert_eq!(classify(200, "", ""), Net::Portal, "an empty 200 is not the text");
        assert_eq!(classify(302, "", "http://login.hotel.example/start"), Net::Portal);
        assert_eq!(classify(307, "", "https://captive.train.example/"), Net::Portal);
        assert_eq!(classify(301, "", "https://nmcheck.gnome.org/check_network_status.txt"), Net::Unknown, "the check's own http to https");
        assert_eq!(classify(204, "", ""), Net::Portal, "not what this page says");
        assert_eq!(classify(403, "denied", ""), Net::Portal);
        assert_eq!(classify(404, "", ""), Net::Unknown, "the page's server, not the network");
        assert_eq!(classify(503, "", ""), Net::Unknown);
        assert_eq!(classify(0, "", ""), Net::Unknown);
    }

    #[test]
    fn the_probe_reads_what_curl_says() {
        let dir = TestDir::new("netcheck");
        let fake = |name: &str, body: &str| {
            let p = dir.join(name);
            write_executable(&p, format!("#!/bin/sh\n{body}\n"));
            p.to_string_lossy().into_owned()
        };
        // curl prints the body, then (-w) a newline, the status and the redirect.
        assert_eq!(probe_with(&fake("online", "printf 'NetworkManager is online\\n200 '")), Net::Online);
        assert_eq!(probe_with(&fake("online-nl", "printf 'NetworkManager is online\\n\\n200 '")), Net::Online);
        assert_eq!(probe_with(&fake("portal", "printf '\\n302 http://login.hotel.example/'")), Net::Portal);
        assert_eq!(probe_with(&fake("page", "printf '<html>hi</html>\\n200 '")), Net::Portal);
        assert_eq!(probe_with(&fake("big", "exit 63")), Net::Portal, "curl 63: a page bigger than the check's text");
        assert_eq!(probe_with(&fake("down", "exit 6")), Net::Unreachable, "curl 6: couldn't resolve");
        assert_eq!(probe_with(&fake("odd", "printf 'hello'")), Net::Unknown);
        assert_eq!(probe_with(&dir.join("missing").to_string_lossy()), Net::Unknown, "no curl");
    }

    #[test]
    fn a_sign_in_keeps_where_it_leads_and_only_a_web_address() {
        assert_eq!(portal_address(Net::Portal, "http://login.hotel.example/start?x=1 "), Some("http://login.hotel.example/start?x=1".into()));
        assert_eq!(portal_address(Net::Portal, "HTTPS://captive.train.example/"), Some("HTTPS://captive.train.example/".into()));
        assert_eq!(portal_address(Net::Portal, ""), None, "a page of its own");
        assert_eq!(portal_address(Net::Portal, "file:///etc/passwd"), None);
        assert_eq!(portal_address(Net::Portal, "javascript:alert(1)"), None);
        assert_eq!(portal_address(Net::Portal, "http://a.example/\u{1b}[2J"), None, "no control characters");
        assert_eq!(portal_address(Net::Portal, &format!("http://a.example/{}", "x".repeat(MAX_URL))), None, "too long");
        assert_eq!(portal_address(Net::Online, "http://login.hotel.example/"), None, "only for a sign-in");
        assert_eq!(host_of("http://login.hotel.example:8080/start?x=1"), Some("login.hotel.example"));
        let with_login = format!("https://user{}pw@192.0.2.1/", ':'); // built here: no login in a URL in the source
        assert_eq!(host_of(&with_login), Some("192.0.2.1"));
        assert_eq!(host_of("http://[fe80::1]:2050/"), Some("fe80::1"));
        assert_eq!(host_of("http://198.51.100.1?a"), Some("198.51.100.1"));
        assert_eq!(host_of("nothing"), None);
        assert_eq!(host_of("http:///path"), None);
    }

    #[test]
    fn the_check_reads_the_sign_in_address() {
        let dir = TestDir::new("netcheck-at");
        let p = dir.join("portal");
        write_executable(&p, "#!/bin/sh\nprintf '\\n302 http://login.hotel.example/start'\n");
        let c = check_with(&p.to_string_lossy(), PROBE_URL);
        assert_eq!(c, Check { net: Net::Portal, portal: Some("http://login.hotel.example/start".into()) });
    }

    #[test]
    fn only_a_sign_in_changes_the_message() {
        let generic = "Couldn't reach the store. Check Wi-Fi.";
        assert_eq!(refine_with(generic, Net::Portal), PORTAL_MESSAGE);
        for net in [Net::Online, Net::Unreachable, Net::Unknown] {
            assert_eq!(refine_with(generic, net), generic, "{net:?}");
        }
    }
}
