//! RomM against a real server (docs/ROMM.md "Tests"): pairing by RomM's device flow, the list, downloads through both ways
//! the Pocket talks (curl, and the plain-HTTP client Android uses at home), a resumed download, a game in a folder, the
//! cover, and the failures a player meets: a token RomM no longer accepts, a wrong pairing code, a server that is gone.
//!
//! Needs a RomM with only homebrew and dummy files (never real games), so it is `#[ignore]` and runs by hand:
//! `POCKETUI_ROMM_URL=http://<host>:18080 POCKETUI_ROMM_LOGIN=<user>:<password> cargo test --test romm_server -- --ignored`.
//! The login approves the pairing, as the player does on the phone; it is read from the environment and never printed.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use pocket_ui::romm::{self, Failure, Net, Poll, Progress};
use pocket_ui::romm_net::{Curl, Plain};

fn server() -> Option<(String, String)> {
    Some((std::env::var("POCKETUI_ROMM_URL").ok()?, std::env::var("POCKETUI_ROMM_LOGIN").ok()?))
}

/// What the player does on the phone: RomM's approval, with their login (through curl's stdin, never its arguments).
fn as_player(server: &str, login: &str, method: &str, path: &str, json: &str) -> (u16, String) {
    let mut child = Command::new("curl")
        .args(["-sS", "-w", "\n%{http_code}", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let cfg = format!(
        "url = \"{server}{path}\"\nrequest = \"{method}\"\nuser = \"{}\"\nheader = \"Content-Type: application/json\"\n{}",
        login.replace('\\', "\\\\").replace('"', "\\\""),
        if json.is_empty() { String::new() } else { format!("data-binary = \"{}\"\n", json.replace('\\', "\\\\").replace('"', "\\\"")) }
    );
    child.stdin.take().unwrap().write_all(cfg.as_bytes()).unwrap();
    let out = String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned();
    let (body, code) = out.rsplit_once('\n').unwrap_or(("", &out));
    (code.trim().parse().unwrap_or(0), body.to_string())
}

fn es_systems(dir: &Path) -> std::path::PathBuf {
    let cfg = dir.join("es_systems.cfg");
    let entry = |id: &str, ext: &str| format!("<system><name>{id}</name><fullname>{id}</fullname><path>{}</path><extension>{ext}</extension></system>", dir.join("roms").join(id).display());
    std::fs::write(&cfg, format!("<systemList>{}{}{}{}</systemList>", entry("gb", ".gb .zip"), entry("gba", ".gba .zip"), entry("nes", ".nes .zip"), entry("psx", ".cue .bin .chd .m3u"))).unwrap();
    cfg
}

/// Pairs a fresh identity by the device flow and approves it as the player; the token and RomM's device id.
fn pair(net: &dyn Net, server: &str, login: &str, ident: &str) -> (String, Option<String>) {
    let p = romm::device_init(net, server, ident, "pocket-ui test").expect("device flow starts");
    assert_eq!(p.user_code.len(), 8, "{}", p.user_code);
    assert!(p.url.starts_with(server) && p.url.contains(&p.user_code), "the QR code opens RomM's approval page with the code in it");
    assert_eq!(romm::device_poll(net, server, &p.device_code), Poll::Waiting, "not approved yet");
    let (status, _) = as_player(server, login, "POST", "/api/auth/device/approve", &format!(r#"{{"user_code":"{}","approved_scopes":["me.read","roms.read","platforms.read","assets.read"]}}"#, p.user_code));
    assert_eq!(status, 200, "approved on the phone");
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        match romm::device_poll(net, server, &p.device_code) {
            Poll::Approved { token, device_id } => return (token, device_id),
            Poll::Waiting | Poll::SlowDown if Instant::now() < until => std::thread::sleep(Duration::from_secs(1)),
            other => panic!("no token: {other:?}"),
        }
    }
}

#[test]
#[ignore = "needs a RomM test server (POCKETUI_ROMM_URL, POCKETUI_ROMM_LOGIN)"]
fn a_real_romm_pairs_lists_and_downloads() {
    let Some((server, login)) = server() else {
        eprintln!("no POCKETUI_ROMM_URL: skipped");
        return;
    };
    let server = romm::normalize_server(&server).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let curl = Curl::new();
    let plain = Plain::new();
    assert!(romm::heartbeat(&curl, &server).unwrap().starts_with(|c: char| c.is_ascii_digit()));
    assert_eq!(romm::heartbeat(&plain, &server), romm::heartbeat(&curl, &server), "both ways see the same server");

    let ident = format!("test-{}", std::process::id());
    let (token, device_id) = pair(&curl, &server, &login, &ident);
    assert!(device_id.is_some(), "RomM registered this Pocket");
    assert!(!romm::whoami(&plain, &server, &token).unwrap().is_empty());

    let cfg = es_systems(dir.path());
    let systems = pocket_ui::library::all_systems(&cfg).unwrap();
    let cat = romm::fetch_catalog(&plain, &server, &token, &systems, &AtomicBool::new(false)).unwrap();
    assert_eq!(romm::fetch_catalog(&curl, &server, &token, &systems, &AtomicBool::new(false)).unwrap().games, cat.games, "the same list both ways");
    assert!(cat.games.len() >= 3, "{:?}", cat.games);
    let gba = cat.games.iter().find(|e| e.system == "gba").expect("a GBA game");

    // Each game, by curl into one card and by the plain client into another: the same bytes, the size RomM said.
    for (net, card) in [(&curl as &dyn Net, "card-a"), (&plain as &dyn Net, "card-b")] {
        for e in &cat.games {
            let dest = e.path_in(&dir.path().join(card).join(&e.system));
            let p = Progress::default();
            romm::download(net, &server, &token, e, &dest, &p).unwrap_or_else(|f| panic!("{}: {f:?}", e.file));
            let size: u64 = if e.parts.is_empty() { std::fs::metadata(&dest).unwrap().len() } else { e.parts.iter().map(|p| std::fs::metadata(dest.with_file_name(&p.name)).unwrap().len()).sum() };
            assert_eq!(size, e.size, "{}", e.file);
            assert_eq!(p.done.load(std::sync::atomic::Ordering::Relaxed), e.size);
        }
    }
    for e in &cat.games {
        let a = std::fs::read(e.path_in(&dir.path().join("card-a").join(&e.system))).unwrap();
        let b = std::fs::read(e.path_in(&dir.path().join("card-b").join(&e.system))).unwrap();
        assert!(a == b, "{}: curl and the plain client differ", e.file);
    }

    // A download that broke off half way goes on from there, with both.
    let whole = std::fs::read(gba.path_in(&dir.path().join("card-a/gba"))).unwrap();
    for (net, card) in [(&curl as &dyn Net, "card-c"), (&plain as &dyn Net, "card-d")] {
        let sys_dir = dir.path().join(card).join("gba");
        let part = sys_dir.join(romm::PARTS_DIR).join(gba.id.to_string()).join(format!("{}.part", gba.file));
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        std::fs::write(&part, &whole[..whole.len() / 2]).unwrap();
        let dest = gba.path_in(&sys_dir);
        romm::download(net, &server, &token, gba, &dest, &Progress::default()).unwrap();
        assert!(std::fs::read(&dest).unwrap() == whole, "resumed into the same bytes ({card})");
    }

    // The cover, when RomM has one: into the cover cache as a PNG.
    let data = dir.path().join("data");
    for e in cat.games.iter().filter(|e| e.cover.is_some()) {
        let shelf = e.path_in(&dir.path().join("card-a").join(&e.system));
        if romm::fetch_cover(&plain, &server, &token, &data, e, &shelf) {
            let png = std::fs::read(pocket_ui::covers::cover_path(&data, &e.system, &shelf)).unwrap();
            assert!(png.starts_with(b"\x89PNG"));
        }
    }

    // The failures, as the player meets them.
    let gone = "http://127.0.0.1:9";
    assert_eq!(romm::heartbeat(&curl, gone), Err(Failure::Unreachable));
    assert_eq!(romm::heartbeat(&plain, gone), Err(Failure::Unreachable));
    assert_eq!(romm::device_init(&plain, gone, &ident, "x"), Err(Failure::Unreachable));
    assert_eq!(romm::download(&curl, gone, &token, gba, &dir.path().join("card-e/gba").join(&gba.file), &Progress::default()), Err(Failure::Unreachable));
    assert_eq!(romm::download(&plain, gone, &token, gba, &dir.path().join("card-f/gba").join(&gba.file), &Progress::default()), Err(Failure::Unreachable));
    assert_eq!(romm::whoami(&curl, &server, "rmm_0000000000000000000000000000000000000000000000000000000000000000"), Err(Failure::SignedOut), "a wrong token");
    assert_eq!(romm::exchange_code(&plain, &server, "00000000"), Err(Failure::BadCode), "a wrong pairing code");
    assert!(matches!(romm::heartbeat(&plain, &format!("{server}/nothing-here")), Err(Failure::NotRomm)), "an address that isn't RomM's API");

    // Unpaired in RomM (the token revoked in its Client API Tokens page): the Pocket is told to pair again. Removing the
    // device alone keeps its token working (RomM 5.4).
    let (status, list) = as_player(&server, &login, "GET", "/api/client-tokens", "");
    assert_eq!(status, 200);
    let tokens: serde_json::Value = serde_json::from_str(&list).unwrap();
    let id = tokens.as_array().unwrap().iter().find(|t| t["device_id"].as_str() == device_id.as_deref()).and_then(|t| t["id"].as_u64()).expect("the pairing's token");
    let (status, _) = as_player(&server, &login, "DELETE", &format!("/api/client-tokens/{id}"), "");
    assert!(matches!(status, 200 | 204), "token revoked: {status}");
    assert_eq!(romm::fetch_catalog(&curl, &server, &token, &systems, &AtomicBool::new(false)), Err(Failure::SignedOut));
    assert_eq!(romm::download(&plain, &server, &token, gba, &dir.path().join("card-g/gba").join(&gba.file), &Progress::default()), Err(Failure::SignedOut));
    let (status, _) = as_player(&server, &login, "DELETE", &format!("/api/devices/{}", device_id.unwrap()), "");
    assert!(matches!(status, 200 | 204), "device removed: {status}");
}
