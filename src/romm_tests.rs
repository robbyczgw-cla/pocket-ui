//! `romm`'s tests: a made-up server ([`Fake`]) answers from a table, so every path (pairing, the list, the shelf, the
//! download and each way it can fail) is a plain test. The same flows against a real RomM in Docker: tests/romm_server.rs.

use super::*;
use crate::library::{Game, SysInfo};
use crate::testutil::TestDir;
use std::sync::Mutex as StdMutex;

/// A server from a table: (method, address start) → answer, files by address for `fetch`, and what was asked.
#[derive(Default)]
struct Fake {
    answers: Vec<(&'static str, String, Answer)>,
    files: HashMap<String, Vec<u8>>,
    /// `fetch` stops after this many bytes (a connection that breaks).
    break_after: Option<u64>,
    /// Every address `fetch` was asked for, with the length the part had.
    fetched: StdMutex<Vec<(String, u64)>>,
    calls: StdMutex<Vec<(String, String, Option<String>, Option<String>)>>,
    offline: bool,
}

impl Fake {
    fn answer(mut self, method: &'static str, url: &str, status: u16, body: &str) -> Fake {
        self.answers.push((method, url.to_string(), Answer { status, body: body.as_bytes().to_vec() }));
        self
    }
    fn file(mut self, url: &str, bytes: Vec<u8>) -> Fake {
        self.files.insert(url.to_string(), bytes);
        self
    }
}

impl Net for Fake {
    fn call(&self, method: &str, url: &str, token: Option<&str>, json: Option<&str>) -> Answer {
        self.calls.lock().unwrap().push((method.into(), url.into(), token.map(str::to_string), json.map(str::to_string)));
        if self.offline {
            return Answer::none();
        }
        self.answers.iter().find(|(m, u, _)| *m == method && url.starts_with(u.as_str())).map(|(_, _, a)| a.clone()).unwrap_or(Answer { status: 404, body: b"{\"detail\":\"Not Found\"}".to_vec() })
    }

    fn fetch(&self, url: &str, _token: &str, part: &Path, _size: u64, stop: &AtomicBool, progress: &(dyn Fn(u64) + Sync)) -> Fetched {
        let have = std::fs::metadata(part).map_or(0, |m| m.len());
        self.fetched.lock().unwrap().push((url.to_string(), have));
        if self.offline {
            return Fetched::Unreachable;
        }
        let Some(bytes) = self.files.get(url) else { return Fetched::Status(404) };
        if stop.load(Ordering::Relaxed) {
            return Fetched::Stopped;
        }
        let end = self.break_after.map_or(bytes.len() as u64, |b| (have + b).min(bytes.len() as u64));
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(part).unwrap();
        std::io::Write::write_all(&mut f, &bytes[have as usize..end as usize]).unwrap();
        progress(end);
        if end < bytes.len() as u64 { Fetched::Broke } else { Fetched::Done }
    }
}

const S: &str = "http://romm.test:8080";

fn systems(root: &Path) -> Vec<SysInfo> {
    let sys = |id: &str, name: &str, exts: &[&str]| SysInfo {
        id: id.into(),
        name: name.into(),
        folder: format!("roms/{id}"),
        path: root.join(id),
        extensions: exts.iter().map(|e| e.to_string()).collect(),
    };
    vec![
        sys("gb", "Game Boy", &["gb", "zip"]),
        sys("gba", "Game Boy Advance", &["gba", "zip"]),
        sys("megadrive", "Mega Drive", &["md", "bin", "gen", "zip"]),
        sys("psx", "PlayStation", &["cue", "bin", "chd", "m3u", "pbp"]),
        sys("nes", "Nintendo Entertainment System", &["nes", "zip"]),
    ]
}

fn platforms() -> &'static str {
    r#"[{"id":1,"slug":"gb","fs_slug":"gb","name":"Game Boy","rom_count":1},
        {"id":2,"slug":"gba","fs_slug":"gba","name":"Game Boy Advance","rom_count":2},
        {"id":3,"slug":"genesis","fs_slug":"genesis","name":"Sega Mega Drive/Genesis","rom_count":1},
        {"id":4,"slug":"psx","fs_slug":"psx","name":"PlayStation","rom_count":1},
        {"id":5,"slug":"ps5","fs_slug":"ps5","name":"PlayStation 5","rom_count":7},
        {"id":6,"slug":"switch","fs_slug":"gba","name":"Nintendo Switch","rom_count":3},
        {"id":7,"slug":"nes","fs_slug":"nes","name":"NES","rom_count":0}]"#
}

fn rom(id: u64, fs_name: &str, name: &str, size: u64) -> String {
    format!(r#"{{"id":{id},"fs_name":"{fs_name}","name":"{name}","fs_size_bytes":{size},"has_multiple_files":false,"missing_from_fs":false,"path_cover_large":"/assets/romm/resources/roms/{id}/cover/big.png?ts=2026-11-18 20:27:11","summary":"A game.","files":[]}}"#)
}

fn server() -> Fake {
    let disc = r#"{"id":40,"fs_name":"Dummy Disc (World)","name":"Dummy Disc","fs_size_bytes":30081,"has_multiple_files":true,"files":[
        {"id":5,"file_name":"Dummy Disc (World).bin","file_path":"roms/psx/Dummy Disc (World)","file_size_bytes":30000},
        {"id":4,"file_name":"Dummy Disc (World).cue","file_path":"roms/psx/Dummy Disc (World)","file_size_bytes":81}]}"#;
    let page = |items: &[String]| format!(r#"{{"items":[{}],"total":{}}}"#, items.join(","), items.len());
    Fake::default()
        .answer("GET", &format!("{S}/api/heartbeat"), 200, r#"{"SYSTEM":{"VERSION":"5.4.1"}}"#)
        .answer("GET", &format!("{S}/api/platforms"), 200, platforms())
        .answer("GET", &format!("{S}/api/roms?platform_ids=1&"), 200, &page(&[rom(10, "Tobu Tobu Girl.gb", "Tobu Tobu Girl", 4)]))
        .answer("GET", &format!("{S}/api/roms?platform_ids=2&"), 200, &page(&[rom(20, "Attack on Voxelburg.gba", "Attack on Voxelburg", 6), rom(21, "Readme.txt", "Readme", 3)]))
        .answer("GET", &format!("{S}/api/roms?platform_ids=3&"), 200, &page(&[rom(30, "Homebrew Racer (World).md", "Homebrew Racer", 5)]))
        .answer("GET", &format!("{S}/api/roms?platform_ids=4&"), 200, &format!(r#"{{"items":[{disc}],"total":1}}"#))
        .file(&format!("{S}/api/roms/10/content/Tobu%20Tobu%20Girl.gb"), b"TOBU".to_vec())
        .file(&format!("{S}/api/roms/20/content/Attack%20on%20Voxelburg.gba"), b"VOXELS".to_vec())
        .file(&format!("{S}/api/roms/40/content/Dummy%20Disc%20%28World%29.bin?file_ids=5"), vec![7u8; 30000])
        .file(&format!("{S}/api/roms/40/content/Dummy%20Disc%20%28World%29.cue?file_ids=4"), vec![b'c'; 81])
}

fn paired(dir: &TestDir) -> PathBuf {
    let data = dir.join("data");
    save_account(&data, &Account { server: S.into(), user: "sam".into(), device_id: Some("d-1".into()) }, "example_token_0123456789abcdef").unwrap();
    data
}

#[test]
fn typed_addresses_become_the_servers_origin() {
    for (typed, want) in [
        ("192.0.2.10:8080", "http://192.0.2.10:8080"),
        (" http://192.0.2.10:8080/ ", "http://192.0.2.10:8080"),
        ("HTTPS://Romm.Example.com", "https://romm.example.com"),
        ("https://games.home/romm/", "https://games.home/romm"),
        ("http://nas:8080/api", "http://nas:8080"),
        ("http://nas:8080/api/", "http://nas:8080"),
        ("nas.local:8080/pair/device?user_code=X", "http://nas.local:8080"),
        ("[2001:db8::5]:8080", "http://[2001:db8::5]:8080"),
    ] {
        assert_eq!(normalize_server(typed).as_deref(), Ok(want), "{typed:?}");
    }
    for bad in ["", "   ", "ftp://nas", "http://", "user@nas:8080", "nas 8080", "http://n<a>s/"] {
        assert!(normalize_server(bad).is_err(), "{bad:?} is no address");
    }
    assert_eq!(shown_server("https://romm.example.com"), "romm.example.com");
    assert_eq!(encode_segment("Dummy Disc (World).cue"), "Dummy%20Disc%20%28World%29.cue");
    assert_eq!(encode_segment("Pokémon/1"), "Pok%C3%A9mon%2F1");
}

#[test]
fn the_token_is_private_and_an_account_needs_one() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TestDir::new("romm-account");
    let data = dir.join("data");
    assert!(account(&data).is_none());
    std::fs::create_dir_all(super::dir(&data)).unwrap();
    std::fs::write(super::dir(&data).join(ACCOUNT_FILE), r#"{"server":"http://x"}"#).unwrap();
    assert!(account(&data).is_none(), "an account without its token is no pairing");
    let data = paired(&dir);
    let a = account(&data).unwrap();
    assert_eq!((a.server.as_str(), a.user.as_str()), (S, "sam"));
    assert_eq!(token(&data).as_deref(), Some("example_token_0123456789abcdef"));
    let mode = std::fs::metadata(super::dir(&data).join(TOKEN_FILE)).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "only its owner reads the token");
    let account_text = std::fs::read_to_string(super::dir(&data).join(ACCOUNT_FILE)).unwrap();
    assert!(!account_text.contains("rmm_"), "the token is in no other file");
    forget(&data);
    assert!(account(&data).is_none() && token(&data).is_none());
    let id = device_identifier(&data);
    assert_eq!(id.len(), 32);
    assert_eq!(device_identifier(&data), id, "made once, then kept");
}

#[test]
fn pairing_by_qr_code_follows_romms_device_flow() {
    let fake = Fake::default().answer(
        "POST",
        &format!("{S}/api/auth/device/init"),
        201,
        r#"{"device_code":"dc","user_code":"M9JXERPY","verification_path":"/pair/device","verification_path_complete":"/pair/device?user_code=M9JXERPY","expires_in":600,"interval":5}"#,
    );
    let p = device_init(&fake, S, "abc", "Pocket").unwrap();
    assert_eq!(p.user_code, "M9JXERPY");
    assert_eq!(p.url, format!("{S}/pair/device?user_code=M9JXERPY"));
    assert_eq!((p.interval, p.expires_in), (5, 600));
    let (_, _, _, body) = fake.calls.lock().unwrap()[0].clone();
    let body: serde_json::Value = serde_json::from_str(&body.unwrap()).unwrap();
    assert_eq!(body["client"], "pocket-ui");
    assert_eq!(body["requested_scopes"].as_array().unwrap().len(), SCOPES.len());
    assert!(!SCOPES.iter().any(|s| s.ends_with(".write")), "the Pocket asks to read only");

    let poll = |status: u16, body: &str| device_poll(&Fake::default().answer("POST", &format!("{S}/api/auth/device/token"), status, body), S, "dc");
    assert_eq!(poll(400, r#"{"detail":"authorization_pending"}"#), Poll::Waiting);
    assert_eq!(poll(400, r#"{"detail":"slow_down"}"#), Poll::SlowDown);
    assert_eq!(poll(429, r#"{"detail":"Too many"}"#), Poll::SlowDown);
    assert_eq!(poll(400, r#"{"detail":"access_denied"}"#), Poll::Failed(Failure::Denied));
    assert_eq!(poll(400, r#"{"detail":"expired_token"}"#), Poll::Failed(Failure::Expired));
    assert_eq!(
        poll(200, r#"{"access_token":"example_token_0123456789abcdef","device_id":"d-1","scopes":["roms.read"],"expires_at":null}"#),
        Poll::Approved { token: "example_token_0123456789abcdef".into(), device_id: Some("d-1".into()) }
    );
    assert_eq!(poll(200, r#"{"access_token":"x y","device_id":null}"#), Poll::Failed(Failure::NotRomm), "a token with a space is none");
    assert_eq!(device_poll(&Fake { offline: true, ..Fake::default() }, S, "dc"), Poll::Waiting, "a dropped question is asked again");
}

#[test]
fn an_older_romm_pairs_with_a_code_and_other_servers_say_so() {
    // RomM 4.x: no device flow, but the heartbeat answers.
    let old = Fake::default().answer("GET", &format!("{S}/api/heartbeat"), 200, r#"{"SYSTEM":{"VERSION":"4.8.1"}}"#);
    assert_eq!(device_init(&old, S, "abc", "Pocket"), Err(Failure::OldServer));
    // Some other web server at that address.
    assert_eq!(device_init(&Fake::default(), S, "abc", "Pocket"), Err(Failure::NotRomm));
    assert_eq!(device_init(&Fake { offline: true, ..Fake::default() }, S, "abc", "Pocket"), Err(Failure::Unreachable));
    assert_eq!(heartbeat(&old, S).as_deref(), Ok("4.8.1"));

    let ex = |status: u16, body: &str| exchange_code(&old_with(status, body), S, "1234 5678");
    fn old_with(status: u16, body: &str) -> Fake {
        Fake::default()
            .answer("GET", &format!("{S}/api/heartbeat"), 200, r#"{"SYSTEM":{"VERSION":"4.8.1"}}"#)
            .answer("POST", &format!("{S}/api/client-tokens/exchange"), status, body)
    }
    assert_eq!(ex(200, r#"{"raw_token":"example_token_0123456789abcdef","device_id":null}"#), Ok(("example_token_0123456789abcdef".into(), None)));
    assert_eq!(ex(400, r#"{"detail":"Invalid or expired code"}"#), Err(Failure::BadCode));
    assert_eq!(ex(404, r#"{"detail":"Not Found"}"#), Err(Failure::BadCode));
    assert_eq!(exchange_code(&old, S, "12"), Err(Failure::BadCode), "too short to ask");
    let sent = old_with(200, "{}");
    let _ = exchange_code(&sent, S, "1234 5678");
    let body = sent.calls.lock().unwrap().iter().find(|c| c.1.ends_with("/exchange")).and_then(|c| c.3.clone()).unwrap();
    assert_eq!(body, r#"{"code":"12345678"}"#, "the code without its space");
}

#[test]
fn the_list_holds_what_this_pocket_can_play() {
    let dir = TestDir::new("romm-list");
    let sys = systems(&dir.join("roms"));
    let cat = fetch_catalog(&server(), S, "rmm_t", &sys, &AtomicBool::new(false)).unwrap();
    let files: Vec<(&str, &str)> = cat.games.iter().map(|e| (e.system.as_str(), e.file.as_str())).collect();
    assert_eq!(
        files,
        vec![("gb", "Tobu Tobu Girl.gb"), ("gba", "Attack on Voxelburg.gba"), ("megadrive", "Homebrew Racer (World).md"), ("psx", "Dummy Disc (World)")],
        "Genesis is Mega Drive here; the PS5's games and a text file are not on the shelf"
    );
    assert_eq!(cat.skipped, 7 + 3 + 1, "PS5, Switch (never, whatever its folder) and the text file are counted");
    let disc = cat.games.iter().find(|e| e.system == "psx").unwrap();
    assert_eq!(disc.launch.as_deref(), Some("Dummy Disc (World).cue"), "a disc in a folder starts from its sheet");
    assert_eq!((disc.size, disc.parts.len()), (30081, 2));
    let gb = &cat.games[0];
    assert_eq!(gb.cover.as_deref(), Some("/assets/romm/resources/roms/10/cover/big.png"), "the cover's address without its stamp");
    assert_eq!(gb.summary.as_deref(), Some("A game."));
    assert_eq!(cat.systems(), 4);
    assert!(fetch_catalog(&Fake { offline: true, ..Fake::default() }, S, "t", &sys, &AtomicBool::new(false)) == Err(Failure::Unreachable));
    let revoked = Fake::default().answer("GET", &format!("{S}/api/platforms"), 401, r#"{"detail":"Invalid token"}"#);
    assert_eq!(fetch_catalog(&revoked, S, "t", &sys, &AtomicBool::new(false)), Err(Failure::SignedOut));
    let not_romm = Fake::default().answer("GET", &format!("{S}/api/platforms"), 200, "<html>router</html>");
    assert_eq!(fetch_catalog(&not_romm, S, "t", &sys, &AtomicBool::new(false)), Err(Failure::NotRomm));
}

#[test]
fn long_lists_are_read_page_by_page() {
    let dir = TestDir::new("romm-pages");
    let sys = systems(&dir.join("roms"));
    let first: Vec<String> = (0..PAGE as u64).map(|i| rom(1000 + i, &format!("Game {i:04}.gb"), &format!("Game {i}"), 1)).collect();
    let fake = Fake::default()
        .answer("GET", &format!("{S}/api/platforms"), 200, r#"[{"id":1,"slug":"gb","fs_slug":"gb","name":"Game Boy","rom_count":501}]"#)
        .answer("GET", &format!("{S}/api/roms?platform_ids=1&limit={PAGE}&offset=0&"), 200, &format!(r#"{{"items":[{}],"total":501}}"#, first.join(",")))
        .answer("GET", &format!("{S}/api/roms?platform_ids=1&limit={PAGE}&offset={PAGE}&"), 200, &format!(r#"{{"items":[{}],"total":501}}"#, rom(9, "Last.gb", "Last", 1)));
    let cat = fetch_catalog(&fake, S, "t", &sys, &AtomicBool::new(false)).unwrap();
    assert_eq!(cat.games.len(), 501);
    assert_eq!(cat.games.last().unwrap().name, "Last");
}

#[test]
fn the_shelf_shows_the_servers_games_at_their_future_paths() {
    let dir = TestDir::new("romm-shelf");
    let data = paired(&dir);
    let roms = dir.join("roms");
    let sys = systems(&roms);
    let cat = fetch_catalog(&server(), S, "t", &sys, &AtomicBool::new(false)).unwrap();
    save_catalog(&data, &cat).unwrap();
    let local = vec![Game { name: "Attack On Voxelburg".into(), path: roms.join("gba/attack on voxelburg.GBA") }];
    let gba = shelf_games(&data, "gba", &roms.join("gba"), &local);
    assert!(gba.is_empty(), "a game on the card already (any case) is not shown twice");
    let gb = shelf_games(&data, "gb", &roms.join("gb"), &[]);
    assert_eq!(gb, vec![Game { name: "Tobu Tobu Girl".into(), path: roms.join("gb/Tobu Tobu Girl.gb") }]);
    let psx = shelf_games(&data, "psx", &roms.join("psx"), &[]);
    assert_eq!(psx[0].path, roms.join("psx/Dummy Disc (World)/Dummy Disc (World).cue"));
    assert_eq!(names_in(&data, "gb").get("tobu tobu girl.gb").map(String::as_str), Some("Tobu Tobu Girl"));
    assert!(cat.find("psx", &psx[0].path).is_some_and(|e| e.id == 40));
    assert!(cat.find("gb", &roms.join("gba/Tobu Tobu Girl.gb")).is_some(), "found by its names, whichever folder");
    assert!(cat.find("gba", &roms.join("gb/Tobu Tobu Girl.gb")).is_none(), "but only in its own system");
    // Another server paired since: its list is not this one's.
    save_account(&data, &Account { server: "http://other:8080".into(), ..Account::default() }, "example_token_0123456789abcdef").unwrap();
    clear_cache();
    assert!(shelf_games(&data, "gb", &roms.join("gb"), &[]).is_empty());
    forget(&data);
    assert!(shelf_games(&data, "gb", &roms.join("gb"), &[]).is_empty(), "unpaired: no games of the server");
}

/// The library's scan puts them on the shelf with the card's own games, and a downloaded one keeps RomM's name.
#[test]
fn the_library_scan_lists_romm_games_beside_the_cards() {
    let dir = TestDir::new("romm-scan");
    let data = paired(&dir);
    let roms = dir.join("roms");
    std::fs::create_dir_all(roms.join("gba")).unwrap();
    std::fs::write(roms.join("gba/My Own Game.gba"), b"x").unwrap();
    let cfg = dir.join("es_systems.cfg");
    let entry = |id: &str, ext: &str| format!("<system><name>{id}</name><fullname>{id}</fullname><path>{}</path><extension>{ext}</extension></system>", roms.join(id).display());
    std::fs::write(&cfg, format!("<systemList>{}{}</systemList>", entry("gba", ".gba .zip"), entry("gb", ".gb"))).unwrap();
    let cat = fetch_catalog(&server(), S, "t", &crate::library::all_systems(&cfg).unwrap(), &AtomicBool::new(false)).unwrap();
    save_catalog(&data, &cat).unwrap();
    let shown = crate::library::load_shown(&cfg, &data).unwrap();
    let games: Vec<(String, String)> = shown.iter().flat_map(|s| s.games.iter().map(|g| (s.name.clone(), g.name.clone()))).collect();
    assert!(games.contains(&("gba".into(), "My Own Game".into())), "{games:?}");
    assert!(games.contains(&("gba".into(), "Attack on Voxelburg".into())), "{games:?}");
    assert!(games.contains(&("gb".into(), "Tobu Tobu Girl".into())), "a system with no game on the card yet is on the shelf too: {games:?}");
    std::fs::create_dir_all(roms.join("gb")).unwrap();
    std::fs::write(roms.join("gb/Tobu Tobu Girl.gb"), b"TOBU").unwrap();
    let shown = crate::library::load_shown(&cfg, &data).unwrap();
    let gb: Vec<&Game> = shown.iter().filter(|s| s.name == "gb").flat_map(|s| &s.games).collect();
    assert_eq!(gb.len(), 1, "downloaded: one game, not two");
    assert_eq!(gb[0].name, "Tobu Tobu Girl");
}

#[test]
fn a_download_lands_whole_in_one_step() {
    let dir = TestDir::new("romm-dl");
    let roms = dir.join("roms");
    let cat = fetch_catalog(&server(), S, "t", &systems(&roms), &AtomicBool::new(false)).unwrap();
    let e = cat.games.iter().find(|e| e.id == 20).unwrap();
    let dest = e.path_in(&roms.join("gba"));
    let p = Progress::default();
    download(&server(), S, "t", e, &dest, &p).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"VOXELS");
    assert_eq!((p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed)), (6, 6));
    assert!(!roms.join("gba").join(PARTS_DIR).exists(), "nothing left behind");
    // There already: nothing is fetched, nothing replaced.
    std::fs::write(&dest, b"MINE").unwrap();
    let fake = server();
    download(&fake, S, "t", e, &dest, &Progress::default()).unwrap();
    assert!(fake.fetched.lock().unwrap().is_empty());
    assert_eq!(std::fs::read(&dest).unwrap(), b"MINE");
}

#[test]
fn a_broken_off_download_goes_on_from_where_it_stopped() {
    let dir = TestDir::new("romm-resume");
    let roms = dir.join("roms");
    let cat = fetch_catalog(&server(), S, "t", &systems(&roms), &AtomicBool::new(false)).unwrap();
    let e = cat.games.iter().find(|e| e.id == 20).unwrap();
    let dest = e.path_in(&roms.join("gba"));
    let flaky = Fake { break_after: Some(4), ..server() };
    assert_eq!(download(&flaky, S, "t", e, &dest, &Progress::default()), Err(Failure::Broke));
    assert!(!dest.exists(), "half a game is never on the shelf");
    let fake = server();
    download(&fake, S, "t", e, &dest, &Progress::default()).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"VOXELS");
    assert_eq!(fake.fetched.lock().unwrap()[0].1, 4, "asked from byte 4 on");
}

#[test]
fn a_game_of_several_files_lands_as_its_folder() {
    let dir = TestDir::new("romm-disc");
    let roms = dir.join("roms");
    let cat = fetch_catalog(&server(), S, "t", &systems(&roms), &AtomicBool::new(false)).unwrap();
    let e = cat.games.iter().find(|e| e.id == 40).unwrap();
    let dest = e.path_in(&roms.join("psx"));
    let p = Progress::default();
    download(&server(), S, "t", e, &dest, &p).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap().len(), 81);
    assert_eq!(std::fs::read(dest.with_file_name("Dummy Disc (World).bin")).unwrap().len(), 30000);
    assert_eq!(p.done.load(Ordering::Relaxed), 30081);
    assert!(!roms.join("psx").join(PARTS_DIR).exists());
}

#[test]
fn every_failure_is_said_in_plain_words() {
    let dir = TestDir::new("romm-fail");
    let roms = dir.join("roms");
    let cat = fetch_catalog(&server(), S, "t", &systems(&roms), &AtomicBool::new(false)).unwrap();
    let e = cat.games.iter().find(|e| e.id == 20).unwrap();
    let dest = e.path_in(&roms.join("gba"));
    // Server gone.
    assert_eq!(download(&Fake { offline: true, ..server() }, S, "t", e, &dest, &Progress::default()), Err(Failure::Unreachable));
    // Not on the server any more.
    assert_eq!(download(&Fake::default(), S, "t", e, &dest, &Progress::default()), Err(Failure::Gone));
    // No room: a game bigger than any card.
    let huge = Entry { size: u64::MAX / 4, ..e.clone() };
    assert!(matches!(download(&server(), S, "t", &huge, &dest, &Progress::default()), Err(Failure::NoRoom(n)) if n > 1 << 40));
    // The wrong size: the part goes, the next start fetches it anew.
    let wrong = Entry { size: 5, ..e.clone() };
    assert_eq!(download(&server(), S, "t", &wrong, &dest, &Progress::default()), Err(Failure::WrongSize));
    assert!(!dest.exists());
    // B while it downloads.
    let p = Progress::default();
    p.stop.store(true, Ordering::Relaxed);
    assert_eq!(download(&server(), S, "t", e, &dest, &p), Err(Failure::Stopped));
    for f in [
        Failure::Unreachable,
        Failure::NotRomm,
        Failure::SignedOut,
        Failure::NoAccess,
        Failure::Gone,
        Failure::OldServer,
        Failure::BadCode,
        Failure::Denied,
        Failure::Expired,
        Failure::NoRoom(3_200_000),
        Failure::Broke,
        Failure::Stopped,
        Failure::WrongSize,
        Failure::CantWrite,
        Failure::Server(502),
    ] {
        let w = f.words();
        assert!(w.len() > 20 && w.ends_with('.'), "{f:?}: {w}");
        assert!(!w.to_lowercase().contains("home screen") && !w.contains("HTTP") && !w.contains("token"), "{f:?}: {w}");
    }
    assert_eq!(Failure::NoRoom(3_200_000).words(), "Not enough free space: this game needs 3 MB more.");
    assert_eq!(Failure::of_status(401), Some(Failure::SignedOut));
    assert_eq!(Failure::of_status(206), None);
}

#[test]
fn romms_own_platform_names_find_our_systems() {
    let dir = TestDir::new("romm-slugs");
    let mut sys = systems(&dir.join("roms"));
    sys.push(SysInfo { id: "ngpc".into(), name: "Neo Geo Pocket Color".into(), folder: "roms/ngpc".into(), path: dir.join("roms/ngpc"), extensions: vec!["ngc".into()] });
    sys.push(SysInfo { id: "pcengine".into(), name: "PC Engine".into(), folder: "roms/pcengine".into(), path: dir.join("roms/pcengine"), extensions: vec!["pce".into()] });
    assert_eq!(system_for("genesis", "genesis", "Sega Mega Drive/Genesis", &sys).as_deref(), Some("megadrive"));
    assert_eq!(system_for("neo-geo-pocket-color", "neo-geo-pocket-color", "Neo Geo Pocket Color", &sys).as_deref(), Some("ngpc"));
    assert_eq!(system_for("tg16", "tg16", "TurboGrafx-16/PC Engine", &sys).as_deref(), Some("pcengine"));
    assert_eq!(system_for("ps", "psx", "PlayStation", &sys).as_deref(), Some("psx"));
    assert_eq!(system_for("ps5", "ps5", "PlayStation 5", &sys), None);
    assert_eq!(system_for("switch", "switch", "Nintendo Switch", &sys), None);
}

#[test]
fn covers_come_from_romm_into_the_cover_cache() {
    let dir = TestDir::new("romm-cover");
    let data = dir.join("data");
    let roms = dir.join("roms");
    let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
    let fake = server().file(&format!("{S}/assets/romm/resources/roms/10/cover/big.png"), png.clone()).file(&format!("{S}/assets/romm/resources/roms/20/cover/big.png"), b"RIFF0000WEBP".to_vec());
    let cat = fetch_catalog(&fake, S, "t", &systems(&roms), &AtomicBool::new(false)).unwrap();
    let gb = &cat.games[0];
    let path = gb.path_in(&roms.join("gb"));
    assert!(fetch_cover(&fake, S, "t", &data, gb, &path));
    assert_eq!(std::fs::read(crate::covers::cover_path(&data, "gb", &path)).unwrap(), png);
    assert!(!fetch_cover(&fake, S, "t", &data, gb, &path), "one there already is kept");
    let gba = &cat.games[1];
    assert!(!fetch_cover(&fake, S, "t", &data, gba, &gba.path_in(&roms.join("gba"))), "a picture we can't show is left to the box art download");
    assert!(std::fs::read_dir(data.join("covers/gb")).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains(".part")));
}

/// RomM's description stands on the game card's back and info page, with its source named, as a gamelist's does.
#[test]
fn romms_description_is_on_the_card_with_its_source() {
    use crate::game_card_model::{CardData, Summary};
    let d = CardData { summary: Summary::Romm("A homebrew racer. Fast and short.".into()), ..Default::default() };
    assert_eq!(d.summary.note().as_deref(), Some("From your RomM server"));
    assert_eq!(crate::game_box::about(&d).0.as_deref(), Some("A homebrew racer. Fast and short."));
    let plain = CardData { plain: true, ..d };
    assert_eq!(crate::game_box::about(&plain).0.as_deref(), Some("A homebrew racer."), "the short card: its first sentence");
}

/// Every file `romm` reads back (the account, the token, the list, this Pocket's id, a download's part) through the shared
/// faults (missing, empty, cut, flipped, another encoding): never a panic, a good file is read, a bad one is no pairing,
/// no list or a fresh id, and a part that is too long or cut short still ends as a whole file of the right size.
#[test]
fn file_source_rule_romm_account_token_catalog_device_id_and_parts() {
    let dir = TestDir::new("romm-faults");
    let data = paired(&dir);
    let d = super::dir(&data);
    let roms = dir.join("roms");
    let cat = fetch_catalog(&server(), S, "t", &systems(&roms), &AtomicBool::new(false)).unwrap();
    save_catalog(&data, &cat).unwrap();
    let account_seed = std::fs::read(d.join(ACCOUNT_FILE)).unwrap();
    let token_seed = std::fs::read(d.join(TOKEN_FILE)).unwrap();
    let catalog_seed = std::fs::read(d.join(CATALOG_FILE)).unwrap();
    let put = |name: &str, bytes: &Option<Vec<u8>>| {
        let _ = std::fs::remove_file(d.join(name));
        if let Some(b) = bytes {
            std::fs::write(d.join(name), b).unwrap();
        }
        clear_cache();
    };
    for (label, bytes) in crate::file_faults::faults(&account_seed) {
        put(ACCOUNT_FILE, &bytes);
        let a = account(&data);
        if label == "valid" {
            assert_eq!(a.map(|a| a.server).as_deref(), Some(S), "{label}");
        }
        let _ = shelf_games(&data, "gb", &roms.join("gb"), &[]);
    }
    put(ACCOUNT_FILE, &Some(account_seed.clone()));
    for (label, bytes) in crate::file_faults::faults(&token_seed) {
        put(TOKEN_FILE, &bytes);
        let t = token(&data);
        assert!(t.as_deref().is_none_or(|t| t.chars().all(|c| c.is_ascii_graphic())), "{label}: a token is printable or none");
        if label == "valid" {
            assert!(t.is_some());
        }
        if matches!(label.as_str(), "missing" | "empty" | "encoding") {
            assert!(account(&data).is_none(), "{label}: no token, no pairing");
        }
    }
    put(TOKEN_FILE, &Some(token_seed.clone()));
    for (label, bytes) in crate::file_faults::faults(&catalog_seed) {
        put(CATALOG_FILE, &bytes);
        let c = load_catalog(&data);
        if label == "valid" {
            assert_eq!(c.as_ref().map(|c| c.games.len()), Some(cat.games.len()));
        }
        let shown = shelf_games(&data, "gb", &roms.join("gb"), &[]);
        assert!(shown.len() <= 1, "{label}");
        let _ = names_in(&data, "psx");
    }
    for (label, bytes) in crate::file_faults::faults(b"00112233445566778899aabbccddeeff") {
        put(DEVICE_ID_FILE, &bytes);
        let id = device_identifier(&data);
        assert!(id.len() >= 16 && id.chars().all(|c| c.is_ascii_hexdigit()), "{label}: {id}");
        if label == "valid" {
            assert_eq!(id, "00112233445566778899aabbccddeeff");
        }
    }
    // A broken-off download's part, whatever is left of it: the game ends whole and the right size, or says why not.
    let e = cat.games.iter().find(|e| e.id == 20).unwrap();
    for (label, bytes) in crate::file_faults::faults(b"VOXELS") {
        let sys = dir.join(format!("part-{label}")).join("gba");
        let part = sys.join(PARTS_DIR).join(e.id.to_string()).join(format!("{}.part", e.file));
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        if let Some(b) = &bytes {
            std::fs::write(&part, b).unwrap();
        }
        let dest = e.path_in(&sys);
        match download(&server(), S, "t", e, &dest, &Progress::default()) {
            Ok(()) => assert_eq!(std::fs::metadata(&dest).unwrap().len(), e.size, "{label}"),
            Err(f) => assert!(!dest.exists() && !f.words().is_empty(), "{label}: {f:?}"),
        }
    }
}
