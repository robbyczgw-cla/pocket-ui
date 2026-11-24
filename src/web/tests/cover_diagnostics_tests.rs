use super::*;
use crate::covers::custom;

#[test]
fn cover_empty_state_and_edit_round_trip_with_backups() {
    let r = rig("cover-change", false);
    let (cookie, csrf) = r.pair();
    let get = |path: &str| r.req("GET", path, &[("Cookie", &cookie)], b"");
    let rom = r.root.join("roms/gba/Advance Wars (USA).gba");
    let id = api::game_id(&rom);
    let game = format!("/api/game?id={id}");
    let art = format!("/api/cover?id={id}");
    let change = format!("/api/cover/change?id={id}&name=../../evil.png&system=wrong");
    let reset = format!("/api/cover/reset?id={id}");
    let headers = [("Cookie", cookie.as_str()), ("X-CSRF-Token", csrf.as_str()), ("Content-Type", "image/png")];
    let post = |path: &str, png: &[u8]| r.req("POST", path, &headers, png);

    let empty = get(&game).json();
    assert_eq!(empty["cover"], false);
    assert_eq!(empty["coverCustom"], false);
    assert_eq!(get(&art).status, 404);
    assert!(super::super::assets::COVER_DIAGNOSTICS_JS.contains("No cover yet."));

    // The stand-in App applies the requested edit to the real cover store.
    let data = r.shared.data_dir.clone();
    let edits = r.edits.clone();
    *r.shared.edits.lock().unwrap() = Some(Box::new(move |edit, tx| {
        let Edit::Cover { system, rom, png } = &edit else { panic!("unexpected edit") };
        assert_eq!(system, "gba");
        let result = custom::apply(&data, system, rom, png.as_deref()).map_err(|e| e.to_string());
        edits.lock().unwrap().push(edit);
        tx.send(result).is_ok()
    }));

    let png = |rgb: u32| api::shrink_png(&crate::image::Image { w: 4, h: 4, px: vec![rgb; 16] }).unwrap();
    let auto = png(0x0000ff);
    let first = png(0xff0000);
    let second = png(0x00ff00);
    let automatic = crate::covers::cover_path(&r.shared.data_dir, "gba", &rom);
    std::fs::create_dir_all(automatic.parent().unwrap()).unwrap();
    std::fs::write(&automatic, &auto).unwrap();
    assert_eq!(get(&art).body, auto);

    assert_eq!(r.req("POST", &change, &[("Cookie", &cookie), ("Content-Type", "image/png")], &first).status, 403);
    assert_eq!(r.req("POST", &change, &[], &first).status, 401);
    assert_eq!(post("/api/cover/change?id=../../evil.png", &first).status, 404);
    assert_eq!(r.req("POST", &change, &[headers[0], headers[1], headers[2], ("Sec-Fetch-Site", "cross-site")], &first).status, 403);
    assert_eq!(post(&change, b"not a png").status, 400);
    let mut broken = first.clone();
    broken[33] ^= 0xff;
    let mut huge = first.clone();
    huge[16..20].copy_from_slice(&(custom::MAX_SIDE + 1).to_be_bytes());
    assert_eq!(post(&change, &huge).status, 400);
    assert_eq!(post(&change, &broken).status, 400);
    let oversized = format!("POST {change} HTTP/1.1\r\nHost: {}\r\nCookie: {cookie}\r\nX-CSRF-Token: {csrf}\r\nContent-Type: image/png\r\nContent-Length: {}\r\n\r\n", r.addr, custom::MAX_BYTES + 1);
    assert_eq!(r.raw(oversized.as_bytes()).status, 413);
    r.shared.game_running.store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(post(&change, &first).status, 503);
    assert_eq!(post(&reset, b"").status, 503);
    assert!(r.edits.lock().unwrap().is_empty());
    r.shared.game_running.store(false, std::sync::atomic::Ordering::Relaxed);

    let saved = post(&change, &first);
    assert_eq!(saved.status, 200);
    assert_eq!(saved.json()["coverCustom"], true);
    assert_eq!(get(&game).json()["coverCustom"], true);
    assert_eq!(get(&art).body, first);
    assert_eq!(get(&art).header("cache-control").as_deref(), Some("no-store"));
    assert_eq!(std::fs::read(&automatic).unwrap(), auto);
    assert_eq!(post(&change, &second).status, 200);
    assert_eq!(get(&art).body, second);

    let reset_answer = post(&reset, b"");
    assert_eq!(reset_answer.status, 200);
    assert_eq!(reset_answer.json()["coverCustom"], false);
    assert_eq!(get(&art).body, auto);
    let custom_path = custom::path(&r.shared.data_dir, "gba", &rom);
    assert!(!custom_path.exists());
    let backups = custom_path.parent().unwrap().join("backups");
    let mut retained: Vec<_> = std::fs::read_dir(backups).unwrap().map(|e| std::fs::read(e.unwrap().path()).unwrap()).collect();
    retained.sort();
    let mut expected = vec![first, second];
    expected.sort();
    assert_eq!(retained, expected);
}

#[test]
fn the_diagnosis_zip_only_contains_version_model_and_safe_events() {
    let r = rig("diagnosis", false);
    let (cookie, _) = r.pair();
    let get = |path: &str| r.req("GET", path, &[("Cookie", &cookie)], b"");
    assert_eq!(r.req("GET", "/api/diagnostics", &[], b"").status, 401);

    let file = r.shared.data_dir.join("logs/pocket-ui.log");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, concat!(
        "[    1.234] first frame after 900ms\n",
        "wifi: connected with password=fictional-wifi-password\n",
        "Authorization: Bearer fictional-bearer-token\n",
        "web: pairing code 246810 with fictional-paired-browser\n",
        "web: a browser was paired (fictional-paired-browser)\n",
        "library: /home", "/fictional-user/secret-folder/Mario.gba\n",
        "launch: game process started fictional-unlabeled-token\n",
        "audio: the stream failed; password=fictional-audio-secret\n",
        "thread 'fictional-user' panicked at '/Users", "/fictional-user/file.rs'\n",
        "fictional-multiline-secret\n",
    )).unwrap();

    let status = get("/api/diagnostics/status").json();
    assert_eq!(status["version"], crate::update::app_version());
    assert_eq!(status["hasLog"], true);
    let answer = get("/api/diagnostics");
    assert_eq!(answer.status, 200);
    assert_eq!(answer.header("content-disposition").as_deref(), Some("attachment; filename=\"pocket-diagnostics.zip\""));
    assert_eq!(answer.header("cache-control").as_deref(), Some("no-store"));
    let files = crate::report::unzip(&answer.body);
    assert_eq!(files.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["system.txt", "pocket-ui.log"]);
    let system = std::str::from_utf8(&files[0].1).unwrap();
    assert!(system.contains(crate::update::app_version()));
    if let Some(model) = crate::device::current().model {
        assert!(system.contains(model.name()));
    } else {
        assert!(system.contains("Not available"));
    }
    let log = std::str::from_utf8(&files[1].1).unwrap();
    assert_eq!(log, "ui.first_frame\nlibrary.read\ngame.started\naudio.stream_failed\nprocess.panic\n");
    for forbidden in ["fictional-", "246810", "/home/", "/Users/", &cookie] {
        assert!(!system.contains(forbidden) && !log.contains(forbidden));
    }
}

#[test]
fn diagnostic_empty_states_include_explanations_instead_of_blank_files() {
    let root = tmp("empty-diagnosis");
    assert!(super::super::diagnostics::log_events(&root, &root.join("not-present.log")).is_empty());
    let entries = super::super::diagnostics::entries("0.7.0", None, &[], crate::i18n::Lang::De);
    assert_eq!(entries[0].1, "App-Version: 0.7.0\nGerätemodell: Nicht verfügbar\n".as_bytes());
    assert_eq!(entries[1].1, "Noch keine Logeinträge zum Teilen.\n".as_bytes());
    assert!(super::super::assets::COVER_DIAGNOSTICS_JS.contains("No shareable log entries yet."));
}

#[test]
fn covers_are_per_game_and_refuse_links_out_of_the_cover_folder() {
    use std::os::unix::fs::symlink;
    let root = tmp("cover-links");
    let data = root.join("data");
    let outside = root.join("outside");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let a = Path::new("/roms/gba/a/Game.gba");
    let b = Path::new("/roms/gba/b/Game.gba");
    let cover = api::shrink_png(&crate::image::Image { w: 4, h: 4, px: vec![0xff00; 16] }).unwrap();
    symlink(&outside, data.join("covers")).unwrap();
    assert!(custom::apply(&data, "gba", a, Some(&cover)).is_err());
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
    std::fs::remove_file(data.join("covers")).unwrap();

    custom::apply(&data, "gba", a, Some(&cover)).unwrap();
    assert_ne!(custom::path(&data, "gba", a), custom::path(&data, "gba", b));
    assert!(custom::existing(&data, "gba", b).is_none());
    let other = custom::path(&data, "gba", b);
    symlink(&outside, &other).unwrap();
    assert!(custom::apply(&data, "gba", b, Some(&cover)).is_err());
    assert!(custom::existing(&data, "gba", b).is_none());
    let first = custom::path(&data, "gba", a);
    symlink(&outside, first.parent().unwrap().join("backups")).unwrap();
    assert!(custom::apply(&data, "gba", a, None).is_err(), "the original must stay if the backup cannot be made");
    assert_eq!(std::fs::read(first).unwrap(), cover);
    assert!(std::fs::read_dir(outside).unwrap().next().is_none());
}
