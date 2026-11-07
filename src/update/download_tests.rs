//! Real local servers reproduce failures before any install can begin.
use super::*;
use crate::testutil::TestDir;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

// One request, with a bounded lifetime even when a test fails. The returned guard collects the server.
struct Server(String, Option<std::thread::JoinHandle<()>>);
impl Server {
    fn new(reply: Vec<u8>, stall: bool) -> Self {
        Self::paced(reply, stall, None)
    }
    /// `pace`: the reply in pieces of this many bytes, one after another with the pause between (a slow Wi-Fi).
    fn paced(reply: Vec<u8>, stall: bool, pace: Option<(usize, Duration)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let join = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                        let mut reader = BufReader::new(stream.try_clone().unwrap());
                        let mut line = String::new();
                        while reader.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                            line.clear();
                        }
                        match pace {
                            Some((piece, pause)) => {
                                for part in reply.chunks(piece) {
                                    if stream.write_all(part).and_then(|()| stream.flush()).is_err() {
                                        break;
                                    }
                                    std::thread::sleep(pause);
                                }
                            }
                            None => {
                                let _ = stream.write_all(&reply);
                            }
                        }
                        if stall {
                            // Wait for curl to close after the watchdog, rather than leaving a sleeping server.
                            let _ = stream.read(&mut [0]);
                        }
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
                    Err(e) => panic!("server: {e}"),
                }
            }
        });
        Self(format!("http://{address}/asset"), Some(join))
    }
    fn bytes(data: &[u8]) -> Self {
        Self::new(
            [
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", data.len()).as_bytes(),
                data,
            ]
            .concat(),
            false,
        )
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        // Wake an unused listener so a cancellation assertion doesn't wait for its deadline.
        if let Some(address) = self.0.strip_prefix("http://").and_then(|s| s.split('/').next()) {
            let _ = TcpStream::connect(address);
        }
        self.1.take().unwrap().join().unwrap();
    }
}
fn tools() -> Tools<'static> {
    Tools {
        protocols: "=http",
        redirects: "=http",
        no_progress: Duration::from_millis(200),
        ..TOOLS
    }
}
fn release(url: String, mirrors: Vec<String>, data: &[u8]) -> Release {
    Release {
        version: "9".into(),
        url,
        mirrors,
        sha256: store::sha256_hex(data),
        size: data.len() as u64,
        notes: String::new(),
    }
}

#[test]
fn blocked_redirect_stall_truncated_and_bad_sha_try_a_clean_fallback() {
    let data = b"the signed update";
    for failure in ["blocked", "redirect", "stall", "truncated", "bad-sha"] {
        let good = Server::bytes(data);
        let bad = match failure {
            "blocked" => Server::new(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\n\r\n".to_vec(), false),
            "redirect" => Server::new(
                format!("HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\n\r\n", good.0).into_bytes(),
                false,
            ),
            "stall" => Server::new(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\nx", data.len()).into_bytes(), true),
            "truncated" => Server::new(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\nwrong", data.len()).into_bytes(), false),
            _ => Server::bytes(&vec![b'x'; data.len()]),
        };
        let dir = TestDir::new("fallback");
        let dest = dir.join("update");
        let rel = release(bad.0.clone(), if failure == "redirect" { vec![] } else { vec![good.0.clone()] }, data);
        download_checked(&tools(), &rel, "http://unused/update.json", &dest, &Cancel::new(), &|_| {}).unwrap_or_else(|e| panic!("{failure}: {e}"));
        assert_eq!(std::fs::read(dest).unwrap(), data, "{failure}: only verified bytes reach the installer");
    }
}

#[test]
fn all_sources_stall_returns_a_clear_error_and_cancellation_never_falls_back() {
    let dir = TestDir::new("stalled-update");
    let stalled = Server::new(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n".to_vec(), true);
    let rel = release(stalled.0.clone(), vec![], b"0123456789");
    let err = download_checked(&tools(), &rel, "http://unused/u.json", &dir.join("f"), &Cancel::new(), &|_| {}).unwrap_err();
    assert!(err.contains("stopped responding") && err.contains("Nothing was installed"), "{err}");
    let cancelled = Cancel::new();
    cancelled.cancel();
    let rel = release(stalled.0.clone(), vec!["http://unused/never".into()], b"0123456789");
    assert_eq!(
        download_checked(&tools(), &rel, "http://unused/u.json", &dir.join("f"), &cancelled, &|_| {}).unwrap_err(),
        "Download cancelled."
    );
}

#[test]
fn changed_mirror_list_requires_a_new_signature_and_plain_http_is_refused() {
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    let key = SigningKey::from_bytes(&[7; 32]);
    let json = br#"{"format":1,"app":{"version":"9","url":"https://primary/u","mirrors":["https://backup/u"],"sha256":"aa","size":10}}"#;
    let sig = base64::engine::general_purpose::STANDARD.encode(key.sign(json).to_bytes());
    let manifest = parse_manifest(json, &sig, &key.verifying_key().to_bytes()).unwrap();
    assert_eq!(manifest.app.unwrap().mirrors, ["https://backup/u"]);
    let changed = String::from_utf8(json.to_vec()).unwrap().replace("backup", "attacker");
    assert!(parse_manifest(changed.as_bytes(), &sig, &key.verifying_key().to_bytes()).is_err());
    let rel = release("https://primary/u".into(), vec!["http://backup/u".into()], b"x");
    let dir = TestDir::new("unsafe-mirror");
    assert!(
        download_checked(&TOOLS, &rel, "https://primary/u.json", &dir.join("f"), &Cancel::new(), &|_| {})
            .unwrap_err()
            .contains("isn't on https")
    );
}

#[test]
fn every_source_is_corrupt_so_nothing_reaches_the_installer() {
    let a = Server::bytes(b"bad");
    let b = Server::bytes(b"bad");
    let rel = release(a.0.clone(), vec![b.0.clone()], b"yes");
    let dir = TestDir::new("all-corrupt");
    let dest = dir.join("app");
    assert!(download_checked(&tools(), &rel, "http://unused/u.json", &dest, &Cancel::new(), &|_| {})
        .unwrap_err()
        .contains("damaged"));
    assert!(!dest.exists(), "no corrupt file can be installed or resumed");
}

#[test]
fn unreachable_host_falls_back_without_an_install_attempt() {
    let unavailable = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/blocked", unavailable.local_addr().unwrap());
    drop(unavailable);
    let good = Server::bytes(b"yes");
    let rel = release(url, vec![good.0.clone()], b"yes");
    let dir = TestDir::new("unreachable-host");
    let dest = dir.join("app");
    download_checked(&tools(), &rel, "http://unused/u.json", &dest, &Cancel::new(), &|_| {}).unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), b"yes");
}

/// Review 0.3: a flat five minutes per source cut every download that needs longer, even one that never stalls (a
/// 1.5 GB system update, or the app on weak Wi-Fi), and the next source began from nothing. A source that keeps
/// sending at a steady pace gets the time its file needs.
#[test]
fn a_slow_steady_download_is_not_cut_off_by_the_source_time_limit() {
    let data: Vec<u8> = (0..30 * 1024).map(|i| (i % 251) as u8).collect();
    let reply = [format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", data.len()).as_bytes(), &data].concat();
    // 30 KB at 2 KB every 100 ms: about 1.5 s, never 200 ms without new bytes, 20 KB/s on average.
    let slow = Server::paced(reply, false, Some((2048, Duration::from_millis(100))));
    let tools = Tools { no_progress: Duration::from_secs(1), source_timeout: Duration::from_millis(500), ..tools() };
    let dir = TestDir::new("slow-steady");
    let dest = dir.join("update");
    let rel = release(slow.0.clone(), vec![], &data);
    download_checked(&tools, &rel, "http://unused/u.json", &dest, &Cancel::new(), &|_| {}).unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), data);
}

/// Still coming, but slower than the file's time allows: given up with that said, not as a server that stopped answering.
#[test]
fn a_download_too_slow_for_its_time_says_so() {
    let data = vec![7u8; 2000];
    let reply = [format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", data.len()).as_bytes(), &data].concat();
    // 200 bytes every 150 ms: never long without new bytes, but 1.5 s for a file whose time here is 300 ms.
    let slow = Server::paced(reply, false, Some((200, Duration::from_millis(150))));
    let tools = Tools { no_progress: Duration::from_secs(30), source_timeout: Duration::from_millis(300), ..tools() };
    let dir = TestDir::new("too-slow");
    let err = download_checked(&tools, &release(slow.0.clone(), vec![], &data), "http://unused/u.json", &dir.join("f"), &Cancel::new(), &|_| {}).unwrap_err();
    assert_eq!(err, TOO_SLOW);
}
