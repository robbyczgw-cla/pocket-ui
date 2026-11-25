//! Persistent-input fault rule. Each format starts with a valid seed and runs
//! in a bounded child, so both a panic/abort and an infinite parser loop fail.
//! The view checks below read words from the actual painter, not page titles.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use pocket_ui::{archive, cheats, covers, disc, favorites, frame_cache, gamelist, image, library, library_cache, recents, settings, slots, softpatch, store, update};
include!("common/root.rs");
#[path = "common/data_local.rs"]
mod local;
#[path = "common/data_extra.rs"]
mod extra;

const SOURCES: &[&str] = &["settings", "favorites", "recents", "systems", "gamelist", "library-cache", "installed", "cheats", "disc", "ips", "bps", "ups", "archive", "7z", "cover", "frame-cache", "update", "case-knows", "moments", "game-settings", "console-settings", "curator", "update-hint", "checksums", "case-cache", "art-cache", "disc-cache", "playlist", "own-cheats", "play-log", "cover-missing"];

#[path = "common/data_faults.rs"]
mod data_faults;
use data_faults::faults;

fn footer(mut b: Vec<u8>, source: &[u8], target: &[u8]) -> Vec<u8> {
    b.extend(crc32fast::hash(source).to_le_bytes()); b.extend(crc32fast::hash(target).to_le_bytes());
    b.extend(crc32fast::hash(&b).to_le_bytes()); b
}
fn zip() -> Vec<u8> {
    let name = b"game.gb"; let data = b"reader rule game"; let crc = crc32fast::hash(data);
    let mut b = Vec::new();
    b.extend(0x04034b50u32.to_le_bytes()); b.extend(20u16.to_le_bytes()); b.extend([0; 8]);
    b.extend(crc.to_le_bytes()); b.extend((data.len() as u32).to_le_bytes()); b.extend((data.len() as u32).to_le_bytes());
    b.extend((name.len() as u16).to_le_bytes()); b.extend(0u16.to_le_bytes()); b.extend(name); b.extend(data);
    let off = b.len(); b.extend(0x02014b50u32.to_le_bytes()); b.extend(20u16.to_le_bytes()); b.extend(20u16.to_le_bytes()); b.extend([0; 8]);
    b.extend(crc.to_le_bytes()); b.extend((data.len() as u32).to_le_bytes()); b.extend((data.len() as u32).to_le_bytes());
    b.extend((name.len() as u16).to_le_bytes()); b.extend([0; 16]); b.extend(name);
    let len = b.len()-off; b.extend(0x06054b50u32.to_le_bytes()); b.extend([0; 4]); b.extend(1u16.to_le_bytes()); b.extend(1u16.to_le_bytes());
    b.extend((len as u32).to_le_bytes()); b.extend((off as u32).to_le_bytes()); b.extend([0; 2]); b
}
fn seed(source: &str, d: &Path) -> Vec<u8> {
    if local::SOURCES.contains(&source) { return local::seed(source, d); }
    if extra::SOURCES.contains(&source) { return extra::seed(source, d); }
    match source {
        "settings" => br#"{"onboarded":true,"seen_version":"9.9.9","theme":"split","motion":"off","art":"off"}"#.to_vec(),
        "favorites" => format!(r#"[{{"system":"gb","name":"Reader Rule Game","path":"{}"}}]"#, d.join("gb/Reader Rule Game.gb").display()).into_bytes(),
        "recents" => format!(r#"[{{"system":"gb","name":"Reader Rule Game","path":"{}","last_played":1,"play_secs":2}}]"#, d.join("gb/Reader Rule Game.gb").display()).into_bytes(),
        "systems" => format!("<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension><command>true</command></system></systemList>", d.join("gb").display()).into_bytes(),
        "gamelist" => b"<gameList><game><path>./Reader Rule Game.gb</path><name>Named Reader Game</name><favorite>true</favorite></game></gameList>".to_vec(),
        "library-cache" => {
            let cfg = d.join("es_systems.cfg"); let key = library_cache::Key::now(&cfg).unwrap();
            library_cache::write(&d.join("data"), &key, &library::load(&cfg).unwrap()).unwrap();
            std::fs::read(d.join("data/library-cache.json")).unwrap()
        }
        "installed" => br#"{"reader":{"version":"9.0","files":[],"name":"Reader tool"}}"#.to_vec(),
        "game-settings" | "console-settings" => br#"{"scaling":"native"}"#.to_vec(),
        "curator" => br#"{"lens":true,"just_play":true,"history":true,"pixel_path":true}"#.to_vec(),
        "update-hint" => br#"{"said":"0.5.1","off":true}"#.to_vec(),
        "checksums" => format!("gb\t11\t0\t12345678\t-\t{}\n", d.join("gb/Reader Rule Game.gb").display()).into_bytes(),
        "case-cache" => {
            let rom=d.join("gb/Reader Rule Game.gb"); let m=std::fs::metadata(&rom).unwrap(); let stamp=m.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let r=pocket_ui::case_knows::RELEASES.iter().find(|r|r.kind==pocket_ui::case_knows::Kind::MarioWorld).unwrap();
            format!("{}\t{stamp}\t{:08X}\t-\t\t{}\n",m.len(),r.crc,rom.display()).into_bytes()
        },
        "art-cache" => {
            let rom=d.join("gb/Reader Rule Game.gb"); let m=std::fs::metadata(&rom).unwrap(); let stamp=m.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let r=pocket_ui::case_knows::RELEASES.iter().find(|r|r.kind==pocket_ui::case_knows::Kind::Gen1).unwrap();
            std::fs::write(d.join("data/identify/case-knows.tsv"),format!("{}\t{stamp}\t{:08X}\t-\t\t{}\n",m.len(),r.crc,rom.display())).unwrap();
            format!("{}\t{stamp}\t-\t{}\n",m.len(),rom.display()).into_bytes()
        },
        "disc-cache" => format!("psx\t11\t0\tSLUS00001\t{}\n", d.join("gb/Reader Rule Game.gb").display()).into_bytes(),
        "playlist" => { std::fs::write(d.join("gb/Reader.iso"),include_bytes!("fixtures/reader-disc.iso")).unwrap(); b"Reader.iso\n".to_vec() },
        "own-cheats" => seed("cheats", d),
        "play-log" => {
            use pocket_ui::play_log::{Game, Session};
            let game=Game{path:"reader://game".into(),name:"Reader Rule Game".into(),system:"gb".into()};
            let session=Session{game:0,start:FIXTURE_NOW.parse::<u64>().unwrap()-600,secs:120,ended:true,..Default::default()};
            pocket_ui::play_log::encode(&pocket_ui::play_log::build(vec![game],vec![session],vec![],vec![]))
        },
        "cover-missing" => b"Reader Rule Game\n".to_vec(),
        "cheats" => b"cheats = 1\ncheat0_desc = \"Infinite lives\"\ncheat0_code = \"010138CD\"\ncheat0_enable = false\n".to_vec(),
        "disc" => format!("1 2\n{}\n", d.join("gb/Reader Rule Game.gb").display()).into_bytes(),
        "ips" => b"PATCH\0\0\0\0\x01XEOF".to_vec(),
        "bps" => footer(b"BPS1\x83\x83\x80\x89xyz".to_vec(), b"abc", b"xyz"),
        "ups" => footer(b"UPS1\x83\x83\x80\x19\x1b\x19\0".to_vec(), b"abc", b"xyz"),
        "archive" => zip(),
        // A real 7-Zip file (testdata/7z/make.sh): Demo.gba, LZMA.
        "7z" => include_bytes!("../testdata/7z/lzma.7z").to_vec(),
        "cover" => {
            let mut b = Vec::new(); { let mut e = png::Encoder::new(&mut b, 1, 1); e.set_color(png::ColorType::Rgb); let mut w = e.write_header().unwrap(); w.write_image_data(&[60, 100, 40]).unwrap(); } b
        }
        "frame-cache" => {
            frame_cache::store(&d.join("frames"), "rule", &pocket_ui::bezel::Bezel::new(2, 2, vec![255; 16], pocket_ui::video::Rect { x:0, y:0, w:1, h:1 }));
            std::fs::read(std::fs::read_dir(d.join("frames")).unwrap().next().unwrap().unwrap().path()).unwrap()
        }
        "update" => br#"{"format":1,"app":null,"os":null}"#.to_vec(),
        // Garbage probe only. Valid saves for every layout are exercised by
        // case_knows_fuzz_tests::file_source_rule_starts_with_a_recognised_save_of_every_layout.
        "case-knows" => vec![0; 32768],
        "moments" => { let p = slots::auto_path(&d.join("states/gb/Reader Rule Game")); pocket_ui::state_file::write(&p, b"reader rule state").unwrap(); std::fs::read(p).unwrap() },
        _ => unreachable!(),
    }
}
fn location(source: &str, d: &Path) -> PathBuf {
    if local::SOURCES.contains(&source) { return local::location(source, d); }
    if extra::SOURCES.contains(&source) { return extra::location(source, d); }
    match source {
        "settings" => d.join("data/settings.json"), "favorites" => d.join("data/favorites.json"), "recents" => d.join("data/recents.json"),
        "systems" => d.join("es_systems.cfg"), "gamelist" => d.join("gb/gamelist.xml"), "library-cache" => d.join("data/library-cache.json"),
        "installed" => d.join("data/store/installed.json"),
        "game-settings" => d.join("data/game-settings/gb/Reader Rule Game.json"),
        "console-settings" => d.join("data/game-settings/gb.json"),
        "curator" => d.join("data/curator.json"),
        "update-hint" => d.join("data/update-hint.json"),
        "checksums" => d.join("data/identify/checksums.tsv"),
        "case-cache" => d.join("data/identify/case-knows.tsv"),
        "art-cache" => d.join("data/identify/case-knows-art.tsv"),
        "disc-cache" => d.join("data/identify/discs.tsv"),
        "playlist" => d.join("gb/Reader Rule Game.m3u"),
        "own-cheats" => d.join("gb/Reader Rule Game.cht"),
        "play-log" => d.join("data/play-log.bin"),
        "cover-missing" => d.join("data/covers/gb/.missing"), "cheats" => d.join("cht/Nintendo - Game Boy/Reader Rule Game.cht"),
        "cover" => d.join("data/covers/gb/Reader Rule Game.png"), "moments" => slots::auto_path(&d.join("states/gb/Reader Rule Game")),
        "frame-cache" => std::fs::read_dir(d.join("frames")).unwrap().next().unwrap().unwrap().path(),
        _ => d.join(format!("input.{source}")),
    }
}
fn prepare(d: &Path) {
    for p in ["data/store", "data/identify", "data/game-settings/gb", "data/core-options", "data/covers/gb", "gb", "cht/Nintendo - Game Boy", "states/gb"] { std::fs::create_dir_all(d.join(p)).unwrap(); }
    std::fs::write(d.join("gb/Reader Rule Game.gb"), b"reader game").unwrap();
    std::fs::write(d.join("es_systems.cfg"), seed("systems", d)).unwrap();
    std::fs::write(d.join("data/settings.json"), seed("settings", d)).unwrap();
}

#[test]
fn all_read_sites_remain_in_the_reviewed_inventory() {
    let output = Command::new("python3").arg("-I").arg(format!("{ROOT}/scripts/reader-inventory.py")).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
#[test]
fn data_source_rule_faults_terminate() {
    let start = Instant::now(); let dir = tempfile::tempdir().unwrap(); let d = dir.path(); prepare(d);
    let only=std::env::var("POCKET_READER_ONLY").ok();
    let mut jobs: Vec<(&'static str, String, Option<Vec<u8>>)> = Vec::new();
    for source in SOURCES.iter().chain(local::SOURCES).chain(extra::SOURCES) {
        if only.as_deref().is_some_and(|only|*source!=only) { continue; }
        for (label, bytes) in faults(&seed(source, d)) {
            jobs.push((*source, label, bytes));
        }
    }
    let count = jobs.len();
    assert!(count>0,"no registered source matched POCKET_READER_ONLY={only:?}");
    let exe = std::env::current_exe().unwrap();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failed_count = std::sync::atomic::AtomicUsize::new(0);
    let failures = std::sync::Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 4).min(count);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let (jobs, next, failed_count, failures, exe) = (&jobs, &next, &failed_count, &failures, &exe);
            scope.spawn(move || {
                loop {
                    if failed_count.load(std::sync::atomic::Ordering::Relaxed) >= 5 { break; }
                    let idx = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((source, label, bytes)) = jobs.get(idx) else { break };
                    let fault = d.join(format!("fault-{idx}"));
                    let log_path = d.join(format!("probe-{idx}.log"));
                    let _ = std::fs::remove_file(&fault);
                    if let Some(bytes) = bytes { std::fs::write(&fault, bytes).unwrap(); }
                    let log = std::fs::File::create(&log_path).unwrap();
                    let mut cmd = Command::new(exe);
                    cmd.args(["--exact", "data_source_probe", "--nocapture"])
                        .env("POCKET_READER_FAULT", &fault).env("POCKET_READER_SOURCE", source).env("POCKET_READER_LABEL", label).env("POCKET_READER_VALID", if label == "valid" {"1"} else {"0"})
                        .stdout(log.try_clone().unwrap()).stderr(log);
                    parent_lifetime(&mut cmd);
                    let mut child = cmd.spawn().unwrap();
                    let deadline = Instant::now() + Duration::from_secs(15);
                    loop {
                        if let Some(status) = child.try_wait().unwrap() {
                            if !status.success() {
                                let out = std::fs::read_to_string(&log_path).unwrap_or_default();
                                failures.lock().unwrap().push(format!("{source}/{label}: {out}"));
                                failed_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                            break;
                        }
                        if Instant::now() > deadline {
                            let _ = child.kill(); let _ = child.wait();
                            failures.lock().unwrap().push(format!("{source}/{label}: reader hung (15s deadline)"));
                            failed_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    let _ = std::fs::remove_file(&fault);
                    let _ = std::fs::remove_file(&log_path);
                }
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    eprintln!("data source faults: {count} probes in {:?}", start.elapsed());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn data_source_probe() {
    let Some(fault) = std::env::var_os("POCKET_READER_FAULT") else { return };
    let source = std::env::var("POCKET_READER_SOURCE").unwrap(); let valid = std::env::var("POCKET_READER_VALID").unwrap() == "1";
    let dir = tempfile::tempdir().unwrap(); let d = dir.path(); prepare(d);
    // Seeds containing paths were made for the parent's card. Relocate those paths to this card.
    let _ = seed(&source, d); let file = location(&source, d);
    let bytes = if valid { Some(seed(&source, d)) } else {
        let parent=Path::new(&fault).parent().unwrap();
        std::fs::read(&fault).ok().map(|b|relocate_source(&source,b,parent,d))
    };
    let _ = std::fs::remove_file(&file); if let Some(b) = &bytes { std::fs::write(&file, b).unwrap(); }
    let text = bytes.as_deref().map(gamelist::decode).unwrap_or_default();
    if local::SOURCES.contains(&source.as_str()) { local::probe(&source, d, &file, bytes.as_deref(), valid);
        if matches!(source.as_str(), "kids-config" | "kids-ledger" | "library-meta") && paint_variant() { paint_source(&source,d,bytes.as_deref(),valid); }
        if paint_variant() { paint_live_source(&source,d,valid); }
        pocket_ui::fsutil::settle(); return; }
    if extra::SOURCES.contains(&source.as_str()) { extra::probe(&source, d, &file, bytes.as_deref(), valid); if paint_variant() { paint_live_source(&source,d,valid); }
        pocket_ui::fsutil::settle(); return; }
    match source.as_str() {
        "game-settings" | "console-settings" => { let f = pocket_ui::game_settings::Files::new(&d.join("data"), "gb", &d.join("gb/Reader Rule Game.gb")); let (map, _) = f.read(); if valid { assert_eq!(map.get("scaling").map(String::as_str), Some("native")); } }
        "curator" => { let got = pocket_ui::curator::prefs::Prefs::load(&d.join("data")); if valid { assert!(got.lens && got.pixel_path); } }
        "update-hint" => { let got = pocket_ui::update_hint::State::load(&d.join("data")); if valid { assert!(got.off); } }
        "checksums" => { let got = pocket_ui::rom_id::Cache::load(&d.join("data")); if valid { assert_eq!(got.sums_of(&d.join("gb/Reader Rule Game.gb"),(11,0)).unwrap().unwrap().crc,0x12345678); } }
        "case-cache" | "art-cache" => { let mut got = pocket_ui::case_knows_read::Reading::new(&d.join("data")); if valid { assert!(got.cheap(&d.join("gb/Reader Rule Game.gb")),"{source}: cache must supply the recognised ROM/known art answer"); } let _ = got.progress_from_save("gb", &d.join("gb/Reader Rule Game.gb"), &file); }
        "disc-cache" => { let got = pocket_ui::disc_serial::Cache::load(&d.join("data")); if valid { assert_eq!(got.len(), 1); } }
        "playlist" => { let got = pocket_ui::disc_serial::serial_of(&file); if valid { assert_eq!(got.unwrap().as_deref(),Some("SLES55673")); } }
        "own-cheats" => { let got = cheats::read_own("gb", &d.join("gb/Reader Rule Game.gb")); if valid { assert!(matches!(got, Some(cheats::Own::Parsed(_)))); } }
        "play-log" => { let got = pocket_ui::play_log::read(&d.join("data")); if valid { assert_eq!(got.damaged, 0); assert_eq!(got.games[0].name,"Reader Rule Game"); assert_eq!(got.sessions[0].secs,120); } }
        "cover-missing" => { let got = covers::listed_missing(&d.join("data"), "gb"); if valid { assert!(got.contains("Reader Rule Game")); } }
        "settings" => { let got = settings::Settings::load(&d.join("data")); if valid { assert!(got.onboarded); } }
        "favorites" => { let got = favorites::Favorites::load(&d.join("data")); if valid { assert!(got.contains(&d.join("gb/Reader Rule Game.gb"))); } }
        "recents" => { let got = recents::Recents::load(&d.join("data")); if valid { assert_eq!(got.list.len(), 1); } }
        "systems" => { let _ = pocket_ui::library::system_list_issues(&file); let got = library::load(&file); if valid { assert_eq!(got.unwrap().len(), 1); } }
        "gamelist" => { let _ = pocket_ui::tools::parse_gamelist(&text); let got = gamelist::parse(&text, &d.join("gb")); if valid { assert_eq!(got.len(), 1); } let _ = gamelist::system(&d.join("data"), &gamelist::Locations::none(), "gb", &d.join("gb")); }
        "library-cache" => { let key = library_cache::Key::now(&d.join("es_systems.cfg")).unwrap(); let got = library_cache::read(&d.join("data"), &key); if valid { assert_eq!(got.unwrap().len(), 1); } }
        "installed" => { let got = store::Store::open(&d.join("data")); if valid { assert!(got.damaged().is_none()); assert_eq!(got.installed["reader"].version,"9.0"); } else if bytes.is_some() && got.damaged().is_some() { assert!(got.damaged().unwrap().contains("damaged")); } }
        "cheats" => { let got = cheats::collect("gb", &d.join("gb/Reader Rule Game.gb"), &[d.join("cht")]); if valid { assert_eq!(got.cheats.len(), 1); } }
        "disc" => { let got = disc::Discs::parse(&text, &d.join("gb/Reader Rule Game.gb")); if valid { assert_eq!(got.unwrap().count, 2); } }
        "ips" | "bps" | "ups" => { let format = match source.as_str() { "ips" => softpatch::Format::Ips, "bps" => softpatch::Format::Bps, _ => softpatch::Format::Ups }; let got = softpatch::apply(format, bytes.as_deref().unwrap_or_default(), b"abc"); if valid { assert!(got.is_ok(), "{got:?}"); } let _ = softpatch::apply_files(&[file], b"abc".to_vec()); }
        "7z" => { let got = archive::extract_rom(&file, &["gba"]); if valid { assert_eq!(got.unwrap().1.len(), 4096); } else if let Err(e) = &got { assert!(!e.is_empty()); } let _ = archive::rom_crc(&file, &["gba"]); let _ = archive::refusal_of(&file, &["gba"], false); }
        "archive" => { let got = archive::extract_rom(&file, &["gb"]); if valid { assert_eq!(got.unwrap().1, b"reader rule game"); } let _ = archive::stored_entries(&file); let _ = archive::rom_crc(&file, &["gb"]); }
        "cover" => { let got = image::load_image(&file); if valid { assert!(got.is_some()); } let _ = covers::parse_index(&text); }
        "frame-cache" => { let got = frame_cache::load(&d.join("frames"), "rule"); if valid { assert!(got.is_some()); } }
        "update" => { let _ = update::read_small(&file, 1048576); use ed25519_dalek::Signer; use base64::Engine;
            let key = ed25519_dalek::SigningKey::from_bytes(&[7; 32]); // public test fixture, never a release key
            let sig = base64::engine::general_purpose::STANDARD.encode(key.sign(&seed("update", d)).to_bytes());
            let got = update::parse_manifest(bytes.as_deref().unwrap_or_default(), &sig, &key.verifying_key().to_bytes());
            if valid { assert_eq!(got.unwrap().format, 1); } else if let Err(reason) = got { assert!(reason.contains("update list")); } }
        "case-knows" => { for release in pocket_ui::case_knows::RELEASES { let _ = pocket_ui::case_knows::read(release, bytes.as_deref().unwrap_or_default(), None); } }
        "moments" => { let raw = pocket_ui::state_file::read(&file); if valid { assert_eq!(raw.unwrap(), b"reader rule state"); } let base = d.join("states/gb/Reader Rule Game"); let _ = slots::filled(&base); let _ = slots::load_preview(&base, 0); let _ = slots::newest_written(&base); if bytes.is_none() { assert!(slots::load_refusal(&slots::filled(&base), 0).unwrap().contains("No save in slot 1 yet")); } }
        _ => unreachable!(),
    }
    pocket_ui::fsutil::settle();
    if paint_variant() { paint_live_source(&source,d,valid); }
    if paint_variant() && matches!(source.as_str(), "settings" | "favorites" | "recents" | "systems" | "gamelist" | "library-cache" | "cover") {
        paint_source(&source, d, bytes.as_deref(), valid);
    }

}

struct Reap(std::process::Child);
impl Drop for Reap { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
fn paint_source(source: &str, d: &Path, bytes: Option<&[u8]>, valid: bool) {
    // Each probe owns its compositor socket; the view reads the very card just mutated.
    let socket = d.join("wl");
    let _wayland = if cfg!(target_os = "linux") {
        let mut cmd = Command::new("python3");
        cmd.arg("-I").arg(format!("{ROOT}/scripts/wayland-stub.py")).arg(&socket);
        parent_lifetime(&mut cmd);
        let child = cmd.spawn().unwrap();
        let child = Reap(child);
        test_clock::wait_for(|| socket.exists(), Duration::from_secs(3), "fault-view compositor");
        Some(child)
    } else { None };
    let (state, expected) = match source {
        "kids-config" | "kids-ledger" => ("kids-shelf", "No games here yet"),
        "favorites" => ("shelf:favorites:split:games", if valid { "Reader Rule Game" } else { "No games yet" }),
        "recents" => ("switcher", if valid { "Reader Rule Game" } else { "Nothing played yet" }),
        "systems" => ("home", if valid { "Game Boy" } else { "No games yet" }),
        "settings" => ("settings:1", "Appearance"),
        "gamelist" => ("shelf:gb:split:games", "Reader Rule Game"),
        "cover" => ("info:split:gb:reader rule game", "Reader Rule Game"),
        _ => ("shelf:gb:split:games", "Reader Rule Game"),
    };
    let out = d.join("words.txt");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state]).arg(d.join("view.png"))
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_DATA", d.join("data")).env("POCKETUI_ES_SYSTEMS", d.join("es_systems.cfg"))
        .env("POCKETUI_BUILT_IN_ROMS", d.join("empty-roms")).env("POCKETUI_VISIBLE_TEXT", &out)
        .env("POCKETUI_LANG", "en").env("POCKETUI_NAV_PROBE", d.join("snapshot-positions.tsv"))
        .env("POCKETUI_SNAPSHOT_SIZE", if source == "cover" {"1240x1080"} else {"640x480"});
    std::fs::create_dir_all(d.join("empty-roms")).unwrap();
    if _wayland.is_some() { cmd.env("WAYLAND_DISPLAY", socket); }
    parent_lifetime(&mut cmd);
    let result = cmd.output().unwrap();
    assert!(result.status.success(), "{source}: {}", String::from_utf8_lossy(&result.stderr));
    let words = std::fs::read_to_string(out).unwrap_or_default();
    if source == "cover" && image::load_image(&d.join("data/covers/gb/Reader Rule Game.png")).is_none() {
        // Split's game-info page calls game_info::paint_title_card in the
        // cover's own left column. Its header is above y=104 and cannot satisfy
        // this assertion: a global game/list title would hide an empty cover.
        let positions=std::fs::read_to_string(d.join("snapshot-positions.tsv")).unwrap();
        let cover_words:Vec<&str>=positions.lines().filter_map(|line| {
            let fields:Vec<&str>=line.splitn(5,'\t').collect();
            let (x,y)=(fields.first()?.parse::<i32>().ok()?,fields.get(1)?.parse::<i32>().ok()?);
            (x>=48 && x<480 && y>104 && y<700).then(||fields.get(4).copied()).flatten()
        }).collect();
        assert!(cover_words.iter().any(|s|s.contains("Reader")), "{source}: no title inside missing-cover region; region={cover_words:?}\n{positions}");
    }

    // A byte flip or a late cut may leave a lenient format valid. It must show
    // either the entry that survived or a concrete explanation of its absence.
    let survived = match source {
        "favorites" => words.contains("Reader Rule Game"),
        "recents" => words.contains("Reader Rule Game") || words.contains("No recent games on the shelf"),
        "systems" => words.contains("Game Boy"),
        "gamelist" => words.contains("Named Reader Game"),
        _ => false,
    };
    assert!(words.contains(expected) || (!valid && survived), "{source} ({} bytes): expected data or fallback {expected:?}, painted:\n{words}", bytes.map_or(0, <[u8]>::len));
}

fn parent_lifetime(cmd: &mut Command) {
    #[cfg(target_os = "linux")]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) });
    }
}

fn paint_variant() -> bool {
    std::env::var_os("POCKETUI_TEST_QUICK").is_none() || matches!(std::env::var("POCKET_READER_LABEL").as_deref(), Ok("valid" | "missing" | "encoding"))
}

/// These pages open their normal file readers; snapshot fixtures do not supply
/// models for them. A missing optional field must leave a usable explanation.
fn paint_live_source(source: &str, d: &Path, valid: bool) {
    let is_cheat = matches!(source, "cheats" | "own-cheats");
    let menu = is_cheat.then(|| cheats::CheatMenu::load_in(&d.join("data"), "gb", &d.join("gb/Reader Rule Game.gb"), &[d.join("cht")], false)).flatten();
    if is_cheat && valid { assert!(menu.is_some(), "valid cheat seed must expose its real page"); }
    let names: Vec<String> = menu.as_ref().map(|m| m.rows().into_iter().map(|r| { assert!(!r.name.trim().is_empty()); r.name }).collect()).unwrap_or_default();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let (page, words): (&str, &[&str]) = match source {
        "moments" => ("game-load", &["Where I quit", "No picture", "From the start"]),
        "state-preview" => if image::load_png(&local::location(source,d)).is_some() { ("game-load", &["Slot 1 · saved"]) } else { ("game-load", &["No picture"]) },
        "timeline" => ("game-earlier", &["No picture", "keeps a moment every 5 min of play", "damaged, can't be loaded"]),
        "game-settings" | "console-settings" | "core-options" => ("game-options", &["Picture", "Controls"]),
        "cheats" | "own-cheats" => if menu.is_some() { ("game-cheats", &names) } else { ("game-options", &["Picture", "Shortcuts"]) },
        "ra-account" => ("achievements", &["Sign in", "Sign out", "Log out"]),
        "battery-history" | "battery-games" | "battery-curve" => ("battery", &["Not enough data yet", "No battery found"]),
        "backup" | "backup-saves" => ("backup", &["A counts them first"]),
        "play-log" => if pocket_ui::play_log::read(&d.join("data")).sessions.is_empty() { ("history", &["Nothing played", "Games you play show up here"]) } else { ("history", &["Reader Rule Game", "2 min"]) },
        "sync-prefs" => ("sync", &["Nothing waiting", "After playing"]),
        _ => return,
    };
    let mut cmd = Command::new("python3");
    cmd.arg("-I").arg(format!("{ROOT}/scripts/data-source-view.py"))
        .arg("--ui").arg(env!("CARGO_BIN_EXE_pocket-ui"))
        .arg("--card").arg(d).arg("--page").arg(page)
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW);
    if source=="moments" { cmd.arg("--load-target").arg("auto"); }
    for word in words { cmd.arg("--contains").arg(word); }
    parent_lifetime(&mut cmd);
    let result = cmd.output().unwrap();
    assert!(result.status.success(), "{source}: file-to-view rule: {}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
}

/// Rewrite only intact card prefixes. This deliberately works on bytes: a bad
/// encoding or a flipped byte inside the path must remain damaged.
fn relocate_prefix(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    if from.is_empty() { return bytes.to_vec(); }
    let mut out=Vec::with_capacity(bytes.len());
    let mut at=0;
    while at<bytes.len() {
        if bytes[at..].starts_with(from) { out.extend_from_slice(to); at+=from.len(); }
        else { out.push(bytes[at]); at+=1; }
    }
    out
}

#[test]
fn source_relocation_keeps_encoding_faults_and_broken_paths() {
    let b=b"\xff/tmp/parent/game\x80:/tmp/parXnt/game:/tmp/parent/last";
    assert_eq!(relocate_prefix(b,b"/tmp/parent",b"/tmp/child"),b"\xff/tmp/child/game\x80:/tmp/parXnt/game:/tmp/child/last");
}

fn relocate_source(source: &str, mut bytes: Vec<u8>, parent: &Path, child: &Path) -> Vec<u8> {
    // A surviving cache header describes its new card; corruption in that header
    // is retained. Otherwise every body mutation would stop at "stale cache".
    if source=="library-cache" {
        if let (Some(old),Some(new))=(library_cache::Key::now(&parent.join("es_systems.cfg")),library_cache::Key::now(&child.join("es_systems.cfg"))) {
            let old=serde_json::to_vec(&old).unwrap(); let new=serde_json::to_vec(&new).unwrap();
            if bytes.starts_with(&old) { bytes.splice(..old.len(),new); }
        }
    }
    if matches!(source,"case-cache"|"art-cache") {
        let stamp=|root:&Path|->Option<Vec<u8>> {
            let m=std::fs::metadata(root.join("gb/Reader Rule Game.gb")).ok()?;
            let time=m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
            Some(format!("{}\t{time}\t",m.len()).into_bytes())
        };
        if let (Some(old),Some(new))=(stamp(parent),stamp(child)) {
            if bytes.starts_with(&old) { bytes.splice(..old.len(),new); }
        }
    }
    relocate_prefix(&bytes,parent.as_os_str().as_encoded_bytes(),child.as_os_str().as_encoded_bytes())
}
