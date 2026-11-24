//! Saves and Pocket Sync in the browser (`saves.rs`, `pockets.rs`, `zip.rs`, the streamed body), end to end over a real
//! socket with the rig of `tests.rs`: one GBA game, "Advance Wars (USA).gba".

use std::path::Path;

use super::saves::{self, Kind};
use super::tests::{rig, Answer, Rig};
use super::zip;

const GAME: &str = "Advance Wars (USA)";

/// The body of a chunked answer, put together; None when it is not whole (no last chunk).
fn unchunk(body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    loop {
        let line_end = at + body[at..].windows(2).position(|w| w == b"\r\n")?;
        let n = usize::from_str_radix(std::str::from_utf8(&body[at..line_end]).ok()?.trim(), 16).ok()?;
        at = line_end + 2;
        if n == 0 {
            return Some(out);
        }
        out.extend_from_slice(body.get(at..at + n)?);
        at += n + 2;
    }
}

fn whole(a: &Answer) -> Vec<u8> {
    assert_eq!(a.header("transfer-encoding").as_deref(), Some("chunked"), "{}", a.head);
    unchunk(&a.body).expect("a whole chunked body")
}

/// The rig's game with two saves, three states and a state's picture, and a save of another game beside it.
fn with_saves(r: &Rig) {
    let roms = r.root.join("roms");
    std::fs::write(roms.join(format!("gba/{GAME}.srm")), b"battery").unwrap();
    std::fs::write(roms.join(format!("gba/{GAME}.rtc")), b"clock").unwrap();
    std::fs::write(roms.join("gba/Other.srm"), b"not ours").unwrap();
    let states = roms.join("savestates/gba");
    std::fs::create_dir_all(&states).unwrap();
    for (name, bytes) in [(".pstate", &b"slot1"[..]), (".pstate2", b"slot3"), (".pstate.auto", b"continue"), (".pstate.png", b"picture")] {
        std::fs::write(states.join(format!("{GAME}{name}")), bytes).unwrap();
    }
}

fn game_id(r: &Rig) -> String {
    super::api::game_id(&r.root.join(format!("roms/gba/{GAME}.gba")))
}

#[test]
fn a_games_saves_and_states_are_listed_by_name_time_and_size_and_never_by_path() {
    let r = rig("saves-list", false);
    with_saves(&r);
    let (cookie, _) = r.pair();
    let id = game_id(&r);
    let a = r.req("GET", &format!("/api/saves?id={id}"), &[("Cookie", &cookie)], b"");
    assert_eq!(a.status, 200, "{}", String::from_utf8_lossy(&a.body));
    let v = a.json();
    let names = |k: &str| v[k].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(names("saves"), [format!("{GAME}.rtc"), format!("{GAME}.srm")], "another game's save is not this one's");
    assert_eq!(v["saves"][0]["kind"], "clock");
    assert_eq!(v["saves"][1]["kind"], "save");
    assert_eq!(v["saves"][1]["bytes"], 7);
    assert!(v["saves"][1]["modified"].as_u64().unwrap() > 0);
    assert_eq!(names("states"), [format!("{GAME}.pstate.auto"), format!("{GAME}.pstate"), format!("{GAME}.pstate2")], "Continue first, then the slots; the picture is no state");
    assert_eq!((v["states"][0]["slot"].as_str(), v["states"][1]["n"].as_u64(), v["states"][2]["n"].as_u64()), (Some("continue"), Some(1), Some(3)));
    assert_eq!(v["states"][1]["picture"], true);
    let text = String::from_utf8_lossy(&a.body);
    assert!(!text.contains("/roms") && !text.contains(&r.root.to_string_lossy().into_owned()), "no path: {text}");
    assert_eq!(r.req("GET", "/api/saves?id=../../etc", &[("Cookie", &cookie)], b"").status, 404);
    assert_eq!(r.req("GET", &format!("/api/saves?id={id}"), &[], b"").status, 401, "paired browsers only");
}

#[test]
fn a_game_without_saves_says_so_and_the_card_counts_none() {
    let r = rig("saves-none", false);
    let (cookie, _) = r.pair();
    let v = r.req("GET", &format!("/api/saves?id={}", game_id(&r)), &[("Cookie", &cookie)], b"").json();
    assert_eq!((v["saves"].as_array().unwrap().len(), v["states"].as_array().unwrap().len(), &v["folder"]), (0, 0, &serde_json::json!(false)));
    let all = r.req("GET", "/api/saves/all", &[("Cookie", &cookie)], b"").json();
    assert_eq!((all["saves"]["count"].as_u64(), all["states"]["count"].as_u64(), all["backups"].as_u64()), (Some(0), Some(0), Some(0)));
    assert_eq!(r.req("GET", &format!("/api/saves/zip?id={}", game_id(&r)), &[("Cookie", &cookie)], b"").status, 404, "no empty ZIP for a game");
    let empty = r.req("GET", "/api/saves/zip", &[("Cookie", &cookie)], b"");
    assert_eq!(empty.status, 200);
    assert!(zip::tests::read_back(&whole(&empty)).is_empty(), "the card's ZIP with nothing in it is still a ZIP");
}

#[test]
fn one_save_downloads_as_it_is_and_only_one_of_the_games() {
    let r = rig("saves-file", false);
    with_saves(&r);
    let (cookie, _) = r.pair();
    let id = game_id(&r);
    let get = |name: &str| r.req("GET", &format!("/api/saves/file?id={id}&name={}", name.replace(' ', "%20").replace('(', "%28").replace(')', "%29")), &[("Cookie", &cookie)], b"");
    let a = get(&format!("{GAME}.srm"));
    assert_eq!(a.status, 200);
    assert_eq!(whole(&a), b"battery");
    let disposition = a.header("content-disposition").unwrap();
    assert!(disposition.starts_with("attachment; filename=\"Advance Wars (USA).srm\""), "{disposition}");
    assert_eq!(get(&format!("{GAME}.pstate.png")).status, 200, "a state's picture comes with it");
    for other in ["Other.srm", "../gba/Other.srm", "..%2F..%2Fdata%2Fweb.json", "Advance Wars (USA).gba"] {
        assert_eq!(get(other).status, 404, "{other}");
    }
}

#[test]
fn all_saves_as_a_zip_with_the_cards_paths_and_states_only_when_asked() {
    let r = rig("saves-zip", false);
    with_saves(&r);
    let (cookie, _) = r.pair();
    let names = |q: &str| {
        let a = r.req("GET", &format!("/api/saves/zip{q}"), &[("Cookie", &cookie)], b"");
        assert_eq!(a.status, 200, "{q}");
        assert!(a.header("content-disposition").unwrap().contains(".zip"));
        let mut n: Vec<(String, Vec<u8>)> = zip::tests::read_back(&whole(&a));
        n.sort();
        n
    };
    let saves = names("");
    let only: Vec<&str> = saves.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(only, ["roms/gba/Advance Wars (USA).rtc", "roms/gba/Advance Wars (USA).srm", "roms/gba/Other.srm"]);
    assert_eq!(saves[1].1, b"battery");
    let with_states = names("?states=1");
    assert!(with_states.iter().any(|(n, b)| n == "roms/savestates/gba/Advance Wars (USA).pstate.auto" && b == b"continue"));
    assert_eq!(with_states.len(), 7);
    let game = names(&format!("?id={}", game_id(&r)));
    assert_eq!(game.len(), 6, "the game's saves, states and pictures, not the other game's: {:?}", game.iter().map(|g| &g.0).collect::<Vec<_>>());
    let all = r.req("GET", "/api/saves/all", &[("Cookie", &cookie)], b"").json();
    assert_eq!((all["saves"]["count"].as_u64(), all["states"]["count"].as_u64(), all["saves"]["fits"].as_bool()), (Some(3), Some(3), Some(true)));
}

#[test]
fn a_download_stops_when_a_game_starts() {
    let dir = crate::testutil::TestDir::new("web-zip-game-starts");
    std::fs::write(dir.join("a.srm"), vec![1u8; 300_000]).unwrap();
    let files = vec![zip::Entry { name: "a.srm".into(), path: dir.join("a.srm"), modified: 0 }];
    let calls = std::sync::atomic::AtomicUsize::new(0);
    // The game starts after the first block.
    let stop = || calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 2;
    let mut out = Vec::new();
    assert!(zip::write(&mut out, &files, &stop).is_err());
    // Over HTTP: no last chunk, so the browser keeps nothing.
    let resp = super::http::Response::stream(200, "application/zip", |w| {
        w.write_all(b"part")?;
        Err(std::io::Error::other("a game started"))
    });
    let mut wire = Vec::new();
    assert!(super::http::write_response(&mut wire, &resp, false).is_err());
    let at = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    assert!(unchunk(&wire[at..]).is_none(), "cut off, not ended");
}

#[test]
fn the_target_comes_from_the_game_never_from_the_name_sent() {
    let dir = crate::testutil::TestDir::new("web-saves-target");
    let roms = dir.join("roms");
    std::fs::create_dir_all(roms.join("gba")).unwrap();
    std::fs::create_dir_all(roms.join("savestates")).unwrap();
    let game = roms.join("gba/Pokemon - Emerald Version (USA).gba");
    std::fs::write(&game, b"rom").unwrap();
    let roots = super::files::Roots::under(&roms);
    let real = |p: &str| roms.canonicalize().unwrap().join(p);
    let t = |name: &str| saves::target(&roots, "gba", &game, name);
    // A save from another device, under another name, lands as this game's.
    assert_eq!(t("Emerald.srm").unwrap(), (Kind::Save, real("gba/Pokemon - Emerald Version (USA).srm")));
    assert_eq!(t("../../../etc/Emerald.SAV").unwrap(), (Kind::Save, real("gba/Pokemon - Emerald Version (USA).sav")));
    // The save that is there keeps its spelling.
    std::fs::write(roms.join("gba/Pokemon - Emerald Version (USA).SRM"), b"old").unwrap();
    assert_eq!(t("x.srm").unwrap().1, real("gba/Pokemon - Emerald Version (USA).SRM"));
    // States: their ending is kept, the folder is made.
    assert_eq!(t("Emerald.pstate3").unwrap(), (Kind::State, real("savestates/gba/Pokemon - Emerald Version (USA).pstate3")));
    assert_eq!(t("Super Mario Bros. 3.state1").unwrap().1, real("savestates/gba/Pokemon - Emerald Version (USA).state1"));
    assert_eq!(t("Emerald.pstate.auto").unwrap().1, real("savestates/gba/Pokemon - Emerald Version (USA).pstate.auto"));
    assert_eq!(t("Emerald.gba.s01").unwrap().1, real("savestates/gba/Pokemon - Emerald Version (USA).gba.s01"), "Dolphin's form keeps the type");
    for bad in ["Emerald.gba", "evil.sh", "Emerald.pstate.png", "", "..", ".srm2"] {
        assert!(t(bad).is_err(), "{bad:?}");
    }
    // A states folder that is a link out of the card is refused.
    #[cfg(unix)]
    {
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, roms.join("savestates/nes")).unwrap();
        assert!(saves::target(&roots, "nes", &game, "x.pstate").is_err());
    }
}

#[test]
fn slots_are_named_as_the_pause_menu_names_them() {
    for (ending, want) in [(".pstate", ("slot", 1)), (".pstate7", ("slot", 8)), (".pstate.auto.z", ("continue", 0)), (".pstate.undo", ("undo", 0)), (".state", ("slot", 1)), (".state2", ("slot", 3)), (".state.auto", ("continue", 0)), (".s01", ("slot", 1)), (".auto.cst", ("continue", 0)), (".pocket-undo", ("undo", 0))] {
        assert_eq!(saves::slot(ending), want, "{ending}");
    }
}

fn backups(r: &Rig) -> Vec<crate::backup_saves::SavesBackup> {
    crate::backup_saves::list(&r.root.join("roms/backup/pocket-ui-saves"))
}

#[test]
fn putting_a_save_back_backs_up_the_one_it_replaces_first() {
    let r = rig("saves-upload", false);
    with_saves(&r);
    let (cookie, csrf) = r.pair();
    let auth = [("Cookie", cookie.as_str()), ("X-CSRF-Token", csrf.as_str())];
    let id = game_id(&r);
    let srm = r.root.join(format!("roms/gba/{GAME}.srm"));

    let check = r.req("POST", &format!("/api/saves/check?id={id}&name=Advance%20Wars%202.srm&size=5"), &auth, b"").json();
    assert_eq!((check["ok"].as_bool(), check["kind"].as_str(), check["name"].as_str()), (Some(true), Some("save"), Some("Advance Wars (USA).srm")));
    assert_eq!(check["replaces"]["bytes"], 7, "the page asks before replacing: {check}");
    assert_eq!(r.req("POST", &format!("/api/saves/check?id={id}&name=x.srm&size=5"), &[("Cookie", cookie.as_str())], b"").status, 403, "a check needs the token");

    let up = |q: &str, body: &[u8]| r.req("POST", &format!("/api/saves/upload?id={id}&{q}"), &auth, body);
    assert_eq!(up("name=x.srm", b"newer").status, 409, "never over a save without asking");
    assert_eq!(std::fs::read(&srm).unwrap(), b"battery");
    assert!(backups(&r).is_empty());
    let a = up("name=x.srm&replace=1", b"newer");
    assert_eq!(a.status, 200, "{}", String::from_utf8_lossy(&a.body));
    assert_eq!(a.json()["replaced"], true);
    assert_eq!(std::fs::read(&srm).unwrap(), b"newer");
    let b = backups(&r);
    assert_eq!(b.len(), 1, "the one replaced went into a saves backup");
    assert_eq!(b[0].manifest.reason, crate::backup::Reason::BeforeRestore);
    assert_eq!(b[0].manifest.items[0].path, format!("roms/gba/{GAME}.srm"));
    assert_eq!(std::fs::read(b[0].dir.join(&b[0].manifest.items[0].stored)).unwrap(), b"battery");

    // A new state: no backup needed.
    let s = up("name=Whatever.pstate5", b"slot6");
    assert_eq!(s.status, 200, "{}", String::from_utf8_lossy(&s.body));
    assert_eq!(std::fs::read(r.root.join(format!("roms/savestates/gba/{GAME}.pstate5"))).unwrap(), b"slot6");
    assert_eq!(backups(&r).len(), 1);

    assert_eq!(up("name=evil.sh", b"#!/bin/sh").status, 403);
    assert_eq!(r.req("POST", "/api/saves/upload?id=nope&name=x.srm", &auth, b"x").status, 404);
    let big = r.raw(format!("POST /api/saves/upload?id={id}&name=x.srm&replace=1 HTTP/1.1\r\nHost: {}\r\nCookie: {cookie}\r\nX-CSRF-Token: {csrf}\r\nContent-Length: {}\r\n\r\n", r.addr, saves::SAVE_MAX + 1).as_bytes());
    assert_eq!(big.status, 413, "refused before a byte is read");
    let leftovers: Vec<_> = walk(&r.root.join("roms")).into_iter().filter(|p| p.to_string_lossy().ends_with(".upload")).collect();
    assert!(leftovers.is_empty(), "nothing left behind: {leftovers:?}");
}

#[test]
fn nothing_is_put_back_while_a_game_runs() {
    let r = rig("saves-playing", false);
    with_saves(&r);
    let (cookie, csrf) = r.pair();
    r.shared.game_running.store(true, std::sync::atomic::Ordering::Relaxed);
    let a = r.req("POST", &format!("/api/saves/upload?id={}&name=x.srm&replace=1", game_id(&r)), &[("Cookie", cookie.as_str()), ("X-CSRF-Token", csrf.as_str())], b"newer");
    assert_eq!(a.status, 503);
    assert_eq!(a.json()["playing"], true);
    assert_eq!(std::fs::read(r.root.join(format!("roms/gba/{GAME}.srm"))).unwrap(), b"battery");
    for p in ["/api/saves/zip", "/api/saves/all", "/api/pockets"] {
        assert_eq!(r.req("GET", p, &[("Cookie", &cookie)], b"").status, 503, "{p}");
    }
}

#[test]
fn a_game_that_starts_during_the_upload_ends_it_with_nothing_changed() {
    let r = rig("saves-starts", false);
    with_saves(&r);
    let (cookie, csrf) = r.pair();
    let id = game_id(&r);
    // The head and the first bytes; then the game starts; then the rest.
    let mut s = std::net::TcpStream::connect(r.addr).unwrap();
    use std::io::{Read, Write};
    write!(s, "POST /api/saves/upload?id={id}&name=x.srm&replace=1 HTTP/1.1\r\nHost: {}\r\nCookie: {cookie}\r\nX-CSRF-Token: {csrf}\r\nContent-Length: 10\r\n\r\nnew", r.addr).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    r.shared.game_running.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = s.write_all(b"er-save");
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    assert!(!out.starts_with("HTTP/1.1 200"), "{out}");
    assert_eq!(std::fs::read(r.root.join(format!("roms/gba/{GAME}.srm"))).unwrap(), b"battery");
    assert!(backups(&r).is_empty());
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

// --- Pocket Sync ---------------------------------------------------------------------------------------------------

#[test]
fn pocket_sync_off_and_never_paired_is_an_empty_overview_not_an_error() {
    let r = rig("pockets-none", false);
    let (cookie, _) = r.pair();
    let v = r.req("GET", "/api/pockets", &[("Cookie", &cookie)], b"").json();
    assert_eq!(v["enabled"], false);
    for k in ["pockets", "recent", "questions"] {
        assert_eq!(v[k].as_array().map(Vec::len), Some(0), "{k}");
    }
}

#[test]
fn pocket_sync_shows_each_pocket_the_last_saves_and_open_questions_without_keys_or_addresses() {
    use crate::pocket_sync::log::{Dir, Event, Last, Log, State};
    let r = rig("pockets", false);
    let dir = r.shared.data_dir.join("pocket-sync");
    std::fs::create_dir_all(&dir).unwrap();
    crate::pocket_sync_page::Prefs { enabled: true }.save(&dir).unwrap();
    let key = [0x5au8; 32];
    let peers = crate::pocket_sync::net::Peers(vec![
        crate::pocket_sync::net::Peer { id: "BRICK1".into(), name: "TrimUI Brick".into(), addr: "192.0.2.77:4711".into(), key },
        crate::pocket_sync::net::Peer { id: "PLUS1".into(), name: "RG35XX Plus".into(), addr: "192.0.2.78:4711".into(), key },
    ]);
    peers.save(&dir).unwrap();
    let mut log = Log::default();
    log.peers.insert("BRICK1".into(), State { last: Last::Synced, ok_at: Some(1_000), down_since: None });
    log.peers.insert("PLUS1".into(), State { last: Last::Unreachable, ok_at: Some(500), down_since: Some(900) });
    log.events.push(Event { at: 1_000, peer: "BRICK1".into(), dir: Dir::From, rel: format!("gba/{GAME}.srm") });
    log.events.push(Event { at: 800, peer: "GONE".into(), dir: Dir::To, rel: "gba/Unknown.srm".into() });
    log.save(&dir).unwrap();
    let mut versions = crate::pocket_sync::Versions::default();
    let vv: crate::pocket_sync::Vv = [("BRICK1".to_string(), 2u64)].into_iter().collect();
    versions.0.insert(format!("gba/{GAME}.srm"), crate::pocket_sync::Rec { sha: "a".repeat(64), vv: vv.clone(), asking: Some(vv), ..Default::default() });
    versions.0.insert("gba/Quiet.srm".into(), crate::pocket_sync::Rec { sha: "b".repeat(64), ..Default::default() });
    versions.save(&dir.join("versions.json")).unwrap();

    let (cookie, _) = r.pair();
    let a = r.req("GET", "/api/pockets", &[("Cookie", &cookie)], b"");
    let v = a.json();
    assert_eq!(v["enabled"], true);
    assert_eq!(v["pockets"][0]["name"], "TrimUI Brick");
    assert_eq!((v["pockets"][0]["last"].as_str(), v["pockets"][0]["okAt"].as_u64()), (Some("synced"), Some(1_000)));
    assert_eq!((v["pockets"][1]["last"].as_str(), v["pockets"][1]["downSince"].as_u64()), (Some("unreachable"), Some(900)));
    assert!(v["recent"][0]["game"].as_str().is_some_and(|g| g.starts_with("Advance Wars")), "{v}");
    assert_eq!(v["recent"][0]["id"], game_id(&r));
    assert_eq!((v["recent"][0]["pocket"].as_str(), v["recent"][0]["dir"].as_str()), (Some("TrimUI Brick"), Some("from")));
    assert!(v["recent"][1]["pocket"].is_null() && v["recent"][1]["game"].is_null(), "a forgotten Pocket, a game not here");
    assert_eq!(v["recent"][1]["file"], "Unknown.srm");
    assert_eq!(v["questions"].as_array().unwrap().len(), 1, "only the save with an open question");
    assert!(v["questions"][0]["game"].as_str().is_some_and(|g| g.starts_with("Advance Wars")), "{v}");
    let text = String::from_utf8_lossy(&a.body);
    for secret in ["5a5a5a5a", "192.0.2.77", "BRICK1", "4711"] {
        assert!(!text.contains(secret), "{secret} left the Pocket: {text}");
    }
    // Nothing here pairs: no route takes a code or a Pocket.
    assert_eq!(r.req("POST", "/api/pockets", &[("Cookie", &cookie)], b"{}").status, 403);
}

// --- the page's texts ------------------------------------------------------------------------------------------------

#[test]
fn the_saves_page_texts_are_translated_from_the_pockets_tables() {
    let keys = super::texts::keys(super::assets::SAVES_JS);
    assert!(keys.iter().any(|k| k == "No saves of this game on the Pocket yet."), "the empty game says so");
    assert!(keys.iter().any(|k| k == "No saves on the Pocket yet. They show up here once you have played."));
    let de = super::texts::translations(crate::i18n::Lang::De, &keys);
    assert_eq!(de.get("Saves and states").map(String::as_str), Some("Spielspeicher und Spielstände"));
    assert_eq!(de.get("{n} saves · {s}").map(String::as_str), Some("{n} Spielspeicher · {s}"));
    let r = rig("texts", false);
    let a = r.req("GET", "/api/texts", &[], b"");
    assert_eq!(a.status, 200, "before pairing too");
    assert!(a.json()["texts"].is_object());
    assert_eq!(r.req("GET", "/saves.js", &[], b"").status, 200);
    assert_eq!(r.req("GET", "/saves.css", &[], b"").status, 200);
}
