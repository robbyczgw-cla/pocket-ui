//! Additional persistent file families for the shared mutation/deadline runner.
use std::path::{Path, PathBuf};
use pocket_ui::{battery_curve, battery_games, battery_history, core_options, emu_choice, kids, library_meta, system_cfg};
pub const SOURCES: &[&str] = &["android-app-icon", "setup-resume", "check-zip", "check-rom", "document-zip", "kids-config", "kids-ledger", "library-meta", "controllers", "emu-choice", "system-cfg", "core-options", "battery-history", "battery-games", "battery-curve", "secret", "gallery", "preview", "dsv", "backup", "backup-saves", "save-transfer", "sync-prefs", "sync-config", "sync-conflict", "frozen-mark", "timeline", "boot-state", "kernel-log", "sensors-log", "web-prefs", "tailscale-prefs", "performance-prefs", "moonlight-pcs", "last-game", "started-boot", "auto-off", "fallback", "json-backup", "jpeg", "ppm", "cue", "gdi", "kept-frame", "state-preview", "players-order", "players-seats", "crash-marker", "crash-log", "launcher-log", "save-trouble", "scummvm-ini", "sounds-config", "night-light", "frame-pack", "rig-json", "rig-base", "sleep-config", "haptics", "leds-config", "display-config", "storage-warning", "album-index", "album-seen", "album-settings"];
pub fn seed(source: &str, d: &Path) -> Vec<u8> {
    for p in ["check/gb", "check/bios", "provider", "data", "data/core-options", "backups/one", "saves-backups/one", "states/gb", "data/logs", "roms/gb"] { std::fs::create_dir_all(d.join(p)).unwrap(); }
    match source {
        "android-app-icon" => [1u32.to_be_bytes(),1u32.to_be_bytes(),0xff285078u32.to_be_bytes()].concat(),
        "setup-resume" => b"shelf-look\n".to_vec(),
        "check-zip" | "document-zip" => super::zip(),
        "check-rom" => b"reader cartridge bytes".to_vec(),
        "kids-config" => br#"{"on":false,"pin":"1234","games":[],"minutes":30}"#.to_vec(),
        "kids-ledger" => br#"{"day":100,"seconds":300,"extra_minutes":10,"warned_for":5}"#.to_vec(),
        "library-meta" => br#"{"version":2,"games":{},"stickers":[{"id":1,"name":"Reader sticker","pinned":true}],"preferred":{}}"#.to_vec(),
        "controllers" => {let mut profiles=pocket_ui::pad::bindings::Profiles::default();let mut layout=pocket_ui::pad::bindings::Layout::default();layout.assign(pocket_ui::pad::Button::South,pocket_ui::pad::Button::East);profiles.devices.insert("reader-pad".into(),layout);serde_json::to_vec(&profiles).unwrap()},
        "emu-choice" | "system-cfg" => b"gb.emulator=libretro\ngb.core=gambatte\n".to_vec(),
        "core-options" => format!(r#"{{"core":"gambatte","rom":"{}","categories":[],"options":[]}}"#, d.join("gb/Reader Rule Game.gb").display()).into_bytes(),
        "battery-history" => {let mut history=battery_history::History::new();assert!(history.push(pocket_ui::battery::Sample {t:1794139200,capacity:73,charging:false,full:false,mv:3850,ma:-400,temp:Some(290),tag:0},Some("gb")));history.encode()},
        "battery-curve" => serde_json::json!({"charge": (0..=100).map(|p| if p == 0 { 0.0 } else { 1.0 }).collect::<Vec<f64>>(), "seen": vec![1u8; 101], "amps": false, "upto": 1794139200u64}).to_string().into_bytes(),
        "battery-games" => serde_json::to_vec(&battery_games::Ledger {games:vec![battery_games::Entry {system:"gb".into(),game:"Reader Rule Game.gb".into(),emulator:"pocket-core".into(),secs:3600.0,drop:10.0,used:None,last:1794139200}]}).unwrap(),
        "secret" => b"reader-rule-test-password".to_vec(),
        "gallery" => { std::fs::create_dir_all(d.join("shots")).unwrap(); super::seed("cover", d) }
        "preview" => pocket_ui::preview::encode(1, &[&pocket_ui::preview::Pixels { w:1, h:1, px:vec![0x285078] }]),
        "web-prefs" => br#"{"enabled":true,"trust_tailnet":false}"#.to_vec(),
        "tailscale-prefs" => br#"{"hostname":"reader-rule"}"#.to_vec(),
        "performance-prefs" => br#"{"games_performance":"battery","games_fan":"cool","smooth_fan":false}"#.to_vec(),
        "moonlight-pcs" => br#"["192.0.2.1"]"#.to_vec(),
        "last-game" => { pocket_ui::on_start::note_started(&d.join("data"),&d.join("gb/Reader Rule Game.gb")); std::fs::read(d.join("data/last-game.json")).unwrap() },
        "started-boot" => b"reader-boot".to_vec(),
        "auto-off" => br#"{"hours":3,"brightness":90}"#.to_vec(),
        "fallback" => serde_json::to_vec(&vec![d.join("gb/Reader Rule Game.gb").display().to_string()]).unwrap(),
        "json-backup" => br#"{"reader":"backup"}"#.to_vec(),
        "jpeg" => std::fs::read(PathBuf::from(super::Root::dir()).join("tests/fixtures/reader.jpg")).unwrap(),
        "ppm" => b"P6\n1 1\n255\n\x28\x50\x78".to_vec(),
        "cue" => { std::fs::write(d.join("gb/track.bin"),b"track").unwrap(); b"FILE \"track.bin\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n".to_vec() },
        "gdi" => { std::fs::write(d.join("gb/track.bin"),b"track").unwrap(); b"1\n1 0 4 2352 track.bin 0\n".to_vec() },
        "kept-frame" | "state-preview" => { for p in ["data/frames/gb","states/gb"] {std::fs::create_dir_all(d.join(p)).unwrap();} if source=="state-preview" { std::fs::write(pocket_ui::slots::state_path(&d.join("states/gb/Reader Rule Game"),0),b"reader state").unwrap(); } super::seed("cover",d) },
        "players-order" => br#"{"order":["reader-pad"]}"#.to_vec(),
        "players-seats" => br#"{"seats":[{"key":"reader-pad","name":"Reader pad"}],"spare":[]}"#.to_vec(),
        "crash-marker" => b"Reader crash marker".to_vec(),
        "crash-log" => b"Reader crash record\n".to_vec(),
        "launcher-log" => b"launcher: Reader launcher record\n".to_vec(),
        "save-trouble" => b"full".to_vec(),
        "scummvm-ini" => b"[scummvm]\nextrapath=/missing-reader-data\n".to_vec(),
        "sounds-config" => br#"{"on":false,"volume":5}"#.to_vec(),
        "night-light" => br#"{"strength":4}"#.to_vec(),
        "frame-pack" => { let p=d.join("framepacks/rule"); pocket_ui::bezel::write_pack(&p,"Reader frame","gb",&pocket_ui::bezel::Bezel::new(2,2,vec![255;16],pocket_ui::video::Rect{x:0,y:0,w:1,h:1})).unwrap(); std::fs::read(p.join("pack.json")).unwrap() },
        "rig-json" | "rig-base" => { let p=d.join("rigs/gb"); std::fs::create_dir_all(&p).unwrap(); let root=PathBuf::from(super::Root::dir()); std::fs::copy(root.join("assets/media/rig/gb/rig.json"),p.join("rig.json")).unwrap(); std::fs::write(p.join("base.png"),super::seed("cover",d)).unwrap(); std::fs::read(p.join(if source=="rig-json" {"rig.json"} else {"base.png"})).unwrap() },
        "sleep-config" => { let mut cfg=pocket_ui::sleep::Config::default(); cfg.screen_off=None; serde_json::to_vec(&cfg).unwrap() },
        "haptics" => br#"{"menu":true,"step":2}"#.to_vec(),
        "leds-config" => serde_json::to_vec(&pocket_ui::leds::Config::default()).unwrap(),
        "display-config" => { let mut cfg=pocket_ui::display::Look::default(); cfg.dim=2; serde_json::to_vec(&cfg).unwrap() },
        "storage-warning" => br#"{"low_space":true}"#.to_vec(),
        "dsv" => vec![0;512],
        "backup" => br#"{"version":1,"created":1,"reason":"manual","app":"reader-test","files":[]}"#.to_vec(),
        "backup-saves" => br#"{"version":1,"created":1,"reason":"manual","app":"reader-test","items":[],"unreadable":[]}"#.to_vec(),
        "save-transfer" => br#"{"format":1,"app":"pocket-ui","made":1,"games":[],"files":{}}"#.to_vec(),
        "sync-prefs" => br#"{"enabled":true,"after_play":false}"#.to_vec(),
        "sync-config" => b"<configuration><gui enabled=\"true\"><address>127.0.0.1:8384</address><apikey>reader-test</apikey></gui><device id=\"READER\" name=\"Reader\"/></configuration>".to_vec(),
        "sync-conflict" => b"reader conflict save".to_vec(),
        "frozen-mark" => br#"{"emulator":"pocket-core","core":"gambatte","rom_bytes":11,"rom_mtime":0,"rom_hash":0}"#.to_vec(),
        "timeline" => b"reader earlier state".to_vec(),
        "boot-state" => b"id=reader-previous\nseq=1\nstarted=1\n".to_vec(),
        "kernel-log" => b"reader kernel log\n".to_vec(),
        "sensors-log" => b"reader sensors log\n".to_vec(),
        // The photo album (`album`): its index (its photo beside it), the case's last opening, the switches.
        "album-index" => {
            let dir = d.join("data/album/gb/Reader Rule Game");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("1795000000.jpg"), super::seed("jpeg", d)).unwrap();
            b"1795000000.jpg\t1795000000\t160x144\tBadge 3\t4 h 12 min\n".to_vec()
        }
        "album-seen" => { std::fs::create_dir_all(d.join("data/album/gb/Reader Rule Game")).unwrap(); b"1795000000".to_vec() }
        "album-settings" => br#"{"moment_photos":true,"moment_in_play":true}"#.to_vec(),
        _ => unreachable!("local seed {source}"),
    }
}
pub fn location(source: &str, d: &Path) -> PathBuf {
    d.join(match source {
        "android-app-icon" => "data/app-icon",
        "album-index" => "data/album/gb/Reader Rule Game/album.tsv", "album-seen" => "data/album/gb/Reader Rule Game/seen", "album-settings" => "data/settings.json",
        "setup-resume" => "data/setup-step",
        "check-zip" => "check/gb/Reader.zip",
        "check-rom" => "check/gb/Reader.gb",
        "document-zip" => "provider/Reader.zip",
        "kids-config" => "data/kids.json", "kids-ledger" => "data/kids-time.json", "library-meta" => "data/library.json", "controllers" => "data/controllers.json",
        "emu-choice" | "system-cfg" => "system.cfg", "core-options" => "data/core-options/gb.json", "battery-history" => "data/battery-history.bin", "battery-games" => "data/battery-games.json", "battery-curve" => "data/battery-curve.json",
        "backup" => "backups/one/backup.json", "backup-saves" => "saves-backups/one/saves.json", "save-transfer" => "manifest.json",
        "sync-prefs" => "data/sync.json", "sync-config" => "syncthing.xml", "sync-conflict" => "roms/gb/Game.sync-conflict-20261118-100000-ABCD.sav",
        "frozen-mark" => "states/gb/Reader Rule Game.pstate.auto.z.mark", "timeline" => "states/gb/Reader Rule Game.pstate.earlier1",
        "boot-state" => "data/logs/boot", "kernel-log" => "data/logs/kernel.log", "sensors-log" => "data/logs/sensors.log",
        "web-prefs" => "data/web.json", "tailscale-prefs" => "data/tailscale.json", "performance-prefs" => "data/performance.json", "moonlight-pcs" => "data/moonlight-pcs.json",
        "last-game" => "data/last-game.json", "started-boot" => "data/started-boot", "auto-off" => "data/switched-off.json", "fallback" => "data/fallback.json",
        "json-backup" => "recovery.json.bak", "jpeg" => "input.jpg", "ppm" => "input.ppm", "cue" => "gb/game.cue", "gdi" => "gb/game.gdi",
        "kept-frame" => "data/frames/gb/Reader Rule Game.png", "state-preview" => "states/gb/Reader Rule Game.pstate.png",
        "players-order" => "data/players.json", "players-seats" => "seats.json", "crash-marker" => "data/crash.marker", "crash-log" => "data/crash.log", "launcher-log" => "launcher.log", "save-trouble" => "save-trouble.txt", "scummvm-ini" => "scummvm.ini",
        "sounds-config" => "data/sounds.json", "night-light" => "data/night-light.json", "frame-pack" => "framepacks/rule/pack.json", "rig-json" => "rigs/gb/rig.json", "rig-base" => "rigs/gb/base.png",
        "sleep-config" => "data/sleep.json", "haptics" => "data/haptics.json", "leds-config" => "data/leds.json", "display-config" => "data/display.json", "storage-warning" => "data/storage.json",
        "secret" => "test-password", "gallery" => "shots/Reader Rule Game_2026-11-08_12-00-00.png", "preview" => "preview.bin", "dsv" => "game.dsv",
        _ => unreachable!("local path {source}"),
    })
}
pub fn probe(source: &str, d: &Path, file: &Path, bytes: Option<&[u8]>, valid: bool) {
    let data = d.join("data");
    match source {
        "android-app-icon" => {
            let icon=pocket_ui::android_games::read_icon(file.to_str().unwrap());
            if valid { assert!(icon.is_some()); }
            if super::paint_variant() { paint_app_icon_variants(d,icon.as_ref()); }
        }
        "setup-resume" => {
            let hw=std::sync::Arc::new(pocket_ui::system_ctl::MockHw::default());
            let mut got=pocket_ui::setup::Setup::new(std::sync::Arc::new(|_|{}),vec![("split","Shelf")],0,data.clone(),vec![],hw).resume();
            if super::paint_variant() {
                let words=d.join("setup-words.txt"); std::env::set_var("POCKETUI_VISIBLE_TEXT",&words);
                let mut pixels=vec![0;1240*1080]; let mut canvas=pocket_ui::gfx::Canvas::new(&mut pixels,1240,1080);
                let mut text=pocket_ui::text::Text::new(); pocket_ui::text::boxes::reset(); got.paint(&mut canvas,&mut text); pocket_ui::text::boxes::finish_canvas(&canvas);
                let words=std::fs::read_to_string(words).unwrap();
                if valid { assert!(words.contains("Pick how your shelf looks"),"{words}"); }
                else { assert!(words.contains("Get started")||words.contains("Pick how your shelf looks"),"{words}"); }
            }
        }
        "check-zip" | "check-rom" => {
            let info=pocket_ui::library::SysInfo{id:"gb".into(),name:"Game Boy".into(),folder:"gb".into(),path:d.join("check/gb"),extensions:vec!["gb".into(),"zip".into()]};
            let input=pocket_ui::game_check::Input{catalog:vec![info],shelf:vec![("gb".into(),vec![file.to_path_buf()])],bios_dir:d.join("check/bios"),roms:d.join("check"),h700:false};
            let report=pocket_ui::game_check::check(&input,&std::sync::atomic::AtomicBool::new(false),&mut |_,_,_|{}).unwrap();
            if valid { assert_eq!(report.ready,1,"{report:?}"); }
            readable_report(&report);
        }
        "document-zip" => {
            use pocket_ui::document_check::{Listing,Root,RootState,Listed};
            let uri=file.to_string_lossy().into_owned();
            let listing=Listing{roots:vec![Root{uri:"test://tree".into(),name:"Reader games".into(),state:RootState::Ok,prefix:String::new(),files:vec![Listed{uri,id:"rule".into(),authority:"test".into(),relative:"Game Boy/Reader.zip".into(),sheet:String::new(),size:std::fs::metadata(file).map(|m|m.len()as i64).unwrap_or(-1)}]}]};
            let got=pocket_ui::document_check::check(&listing,&FileProvider,&std::sync::atomic::AtomicBool::new(false),&mut|_,_,_|{}).unwrap();
            if valid { assert_eq!(got.report.ready,1,"{:?}",got.report); }
            readable_report(&got.report);
        }
        "kids-config" => { let got = kids::Config::load(&data); if valid { assert_eq!(got.pin,"1234"); } }
        "album-index" => { let got = pocket_ui::album::list(file.parent().unwrap()); if valid { assert_eq!(got.len(), 1); assert_eq!(got[0].note[0], "Badge 3"); assert_eq!(got[0].size, (160, 144)); } }
        "album-seen" => { let got = pocket_ui::album::seen(file.parent().unwrap()); if valid { assert_eq!(got, 1795000000); } }
        "album-settings" => { let (on, play) = (pocket_ui::album::on(&data), pocket_ui::album::moment_in_play(&data)); if valid { assert!(on && play); } else { let _ = (on, play); } }
        "kids-ledger" => { let got = kids::Ledger::load(&data); if valid { assert_eq!(got.seconds,300); } }
        "library-meta" => { let got = library_meta::LibraryMeta::load(&data); if valid { assert_eq!(got.stickers()[0].name,"Reader sticker"); } }
        "controllers" => { let got = pocket_ui::pad::bindings::Profiles::load(&data); if valid {assert!(got.devices.contains_key("reader-pad"));assert_eq!(got.layout("reader-pad").role(pocket_ui::pad::Button::South),pocket_ui::pad::Button::East);} }
        "emu-choice" => { let got = emu_choice::Cfg::read(file); if valid { assert_eq!(got.get("gb.core"),Some("gambatte")); } }
        "system-cfg" => { let got = system_cfg::SystemCfg::at(file); if valid { assert_eq!(got.get("gb.core").as_deref(),Some("gambatte")); } else { let _ = got.read(); } }
        "core-options" => { let got = core_options::CoreOptionsFile::read(file,&d.join("gb/Reader Rule Game.gb")); if valid { assert_eq!(got.unwrap().core,"gambatte"); } }
        "battery-history" => { let got = battery_history::History::load(file); if valid {assert_eq!(got.samples().len(),1);assert_eq!(got.samples()[0].capacity,73);assert_eq!(got.tag(&got.samples()[0]),Some("gb"));} }
        "battery-curve" => { let got = battery_curve::Curve::load(&data); if valid { assert_eq!(got.upto,1794139200); assert!(got.usable(50).is_some()); } else { assert_eq!(got.usable(50),None); } }
        "battery-games" => { let got = battery_games::Ledger::load(&data);if valid {assert_eq!(got.games.len(),1);let key=battery_games::Key::new("gb",&d.join("gb/Reader Rule Game.gb"),"pocket-core");let rate=got.rate(&key).unwrap();assert_eq!(rate.per_hour,10.0);assert_eq!(rate.basis,battery_games::Basis::Game);} }
        "secret" => { let got = pocket_ui::secret::load(file); if valid { assert_eq!(got.as_deref(),Some("reader-rule-test-password")); } }
        "gallery" => { let _ = pocket_ui::gallery::scan(&d.join("shots")); let got = pocket_ui::image::load_image(file); if valid { assert!(got.is_some()); } }
        "preview" => { let got = pocket_ui::preview::decode(bytes.unwrap_or_default()); if valid { assert_eq!(got.unwrap().0,1); } }
        "backup" => { let got = pocket_ui::backup::list(&d.join("backups")); if valid { assert_eq!(got.len(),1); } }
        "backup-saves" => { let got = pocket_ui::backup_saves::list(&d.join("saves-backups")); if valid { assert_eq!(got.len(),1); } let _ = pocket_ui::backup_saves::compare(&d.join("saves-backups/one"),&d.join("storage"),&d.join("restore-backups")); }
        "save-transfer" => { let text = bytes.and_then(|b|std::str::from_utf8(b).ok()); let got = pocket_ui::save_transfer::read_backup(&data,text,&["saves/0123456789abcdef/game.sav".into()],&[]); if valid { assert_eq!(got.unwrap().games(),1); } }
        "sync-prefs" => { let got = pocket_ui::sync::Prefs::load(&data); if valid { assert!(got.enabled); } }
        "sync-config" => { let got = pocket_ui::sync::Config::parse(std::str::from_utf8(bytes.unwrap_or_default()).unwrap_or("")); if valid { assert!(got.is_ok(),"{got:?}"); } }
        "sync-conflict" => { let got = pocket_ui::sync_conflicts::gather(&[file.to_path_buf()],&pocket_ui::sync_conflicts::Layout::under(d)); if valid { assert_eq!(got.len(),1); } }
        "frozen-mark" => { let got = pocket_ui::frozen_phone::read_mark(&d.join("states/gb/Reader Rule Game")); if valid { assert_eq!(got.unwrap().core,"gambatte"); } }
        "timeline" => { let base=d.join("states/gb/Reader Rule Game"); let got = pocket_ui::timeline::to_load(&base,"earlier1",None); if valid { assert_eq!(got.unwrap().bytes,20); } else if let Err(reason)=got { assert!(reason.contains("earlier state")); } let _ = pocket_ui::timeline::entries(&base); }
        "boot-state" | "kernel-log" | "sensors-log" => { if source != "boot-state" { std::fs::write(data.join("logs/boot"),seed("boot-state",d)).unwrap(); } let got = pocket_ui::logs::begin_boot(&data,"reader-current",2); if valid { assert!(got.is_some()); } }
        "web-prefs" => { let got = pocket_ui::web::Prefs::load(&data); if valid {assert!(got.enabled);assert!(!got.trust_tailnet);} }
        "tailscale-prefs" => { let got = pocket_ui::tailscale::Prefs::load(&data); if valid { assert_eq!(got.hostname.as_deref(),Some("reader-rule")); } }
        "performance-prefs" => { let got = pocket_ui::perf_fan::Prefs::load(&data); if valid {assert!(!got.smooth_fan);assert_eq!(got.games_performance,pocket_ui::power::Performance::Battery);assert_eq!(got.games_fan,pocket_ui::power::Fan::Cool);} }
        "moonlight-pcs" => { let got = pocket_ui::moonlight::load_pcs(&data); if valid { assert_eq!(got,vec!["192.0.2.1"]); } }
        "last-game" => { let got = pocket_ui::on_start::last_game(&data); if valid { assert_eq!(got.unwrap().path,d.join("gb/Reader Rule Game.gb")); } }
        "started-boot" => { let got = pocket_ui::on_start::first_start_this_boot(&data,Some("reader-boot")); if valid { assert!(!got); } }
        "auto-off" => { let got = pocket_ui::auto_off::take_note(&data); if valid { assert_eq!(got.unwrap().brightness,Some(90)); } }
        "fallback" => { let got = pocket_ui::fallback::Fallback::load(&data); if valid {assert!(!got.contains(&d.join("gb/Reader Rule Game.gb")));pocket_ui::fsutil::settle();assert_eq!(std::fs::read_to_string(file).unwrap().trim(),"[]","recognised legacy entries must actually be retired");} }
        "json-backup" => { let original=d.join("recovery.json"); std::fs::write(&original,b"{cut").unwrap(); let got = pocket_ui::fsutil::read_json_or_backup::<serde_json::Value>(&original); if valid { assert_eq!(got.unwrap()["reader"],"backup"); } }
        "jpeg" => { let got=pocket_ui::image::load_image(file); if valid { assert!(got.is_some()); } }
        "ppm" => { let got=pocket_ui::image::parse_ppm(bytes.unwrap_or_default()); if valid { assert!(got.is_some()); } }
        "cue" | "gdi" => { let got=pocket_ui::game_files::files_of(file); if valid { assert_eq!(got.len(),2); } let _ = pocket_ui::frozen::fingerprint(file); }
        "kept-frame" | "state-preview" => { let got=pocket_ui::image::load_image(file); if valid { assert!(got.is_some()); } let _ = pocket_ui::launch_picture::source_at(&d.join("states/gb/Reader Rule Game"),file); let _ = pocket_ui::lastframe::FileKey::of(file);
            if source=="state-preview" { let got=pocket_ui::slots::load_preview(&d.join("states/gb/Reader Rule Game"),0); if valid { assert!(got.unwrap().0.is_some()); } }
        }
        "players-order" => { let got=pocket_ui::players::Order::load(&data); if valid { assert_eq!(got.order,vec!["reader-pad"]); } }
        "players-seats" => { let got=pocket_ui::players::SeatsView::read(file); if valid { assert_eq!(got.unwrap().seats[0].as_ref().unwrap().name,"Reader pad"); } }
        "crash-marker" => { let got=pocket_ui::crash::take_marker(&data); if valid { assert_eq!(got.as_deref(),Some("Reader crash marker")); } }
        "crash-log" | "launcher-log" => { let got=pocket_ui::rescue::WhatHappened::read(None,&data,&d.join("launcher.log")); if valid && source=="crash-log" { assert_eq!(got.last_crash.as_deref(),Some("Reader crash record")); } if valid && source=="launcher-log" { assert_eq!(got.launcher,vec!["Reader launcher record"]); } assert!(!got.headline().is_empty()); }
        "save-trouble" => { let got=pocket_ui::save_trouble::read_from(file); if valid { assert!(got.unwrap().line().contains("SD card is full")); } }
        "scummvm-ini" => { let got=pocket_ui::hostsys::scummvm_ini(std::str::from_utf8(bytes.unwrap_or_default()).unwrap_or(""), |_|false); if valid { assert!(got.unwrap().contains("extrapath=")); } }
        "sounds-config" => { let got=pocket_ui::ui_sound::Config::load(&data); if valid { assert!(!got.on); assert_eq!(got.volume,5); } }
        "night-light" => { let got=pocket_ui::night_light::Prefs::load(&data); if valid { assert_eq!(got.strength,4); } assert!(!got.summary().is_empty()); }
        "frame-pack" => { let got=pocket_ui::bezel::find_pack_frame(&d.join("framepacks"),"gb"); if valid { let (path,hole)=got.unwrap(); assert_eq!(hole.w,1); assert!(pocket_ui::bezel::load_png_rgba(&path).is_some()); } }
        "rig-json" | "rig-base" => { std::env::set_var("POCKETUI_RIG_ASSETS",d.join("rigs")); let got=pocket_ui::insert_rig::files("gb",pocket_ui::media::Game::of("gb","Reader Rule Game.gb")); if valid { assert_eq!(got.unwrap().base.len(),super::seed("cover",d).len()); } let _=pocket_ui::bezel::load_png_rgba(&d.join("rigs/gb/base.png")); }
        "sleep-config" => { let got=pocket_ui::sleep::Config::load(&data); if valid { assert_eq!(got.screen_off,None); } }
        "haptics" => { let got=pocket_ui::rumble::load_menu_haptics(&data); if valid { assert!(got); } }
        "leds-config" => { let got=pocket_ui::leds::Config::load(&data); if valid { assert!(got.is_some()); } }
        "display-config" => { let got=pocket_ui::display::Look::load(&data); if valid { assert_eq!(got.dim,2); } }
        "storage-warning" => { let got=pocket_ui::storage::Warned::load(&data); if valid { assert!(got.low_space); } }
        "dsv" => { let got = pocket_ui::dsv::raw_save(bytes.unwrap_or_default()); if valid { assert_eq!(got.unwrap().0.len(),512); } let _ = pocket_ui::dsv::take_over(file,&d.join("game.sav"),&d.join("game.srm")); }
        _ => unreachable!("local probe {source}"),
    }
}

fn readable_report(report: &pocket_ui::game_check::Report) {
    assert!(!report.summary().trim().is_empty());
    for finding in &report.findings { assert!(!finding.name.trim().is_empty()); assert!(!finding.what.trim().is_empty(),"{finding:?}"); }
}
struct FileProvider;
impl pocket_ui::document_check::Host for FileProvider {
    fn bundled(&self,_:&str)->bool {true}
    fn installed(&self,_:&str)->bool {false}
    fn open(&self,uri:&str)->Option<Box<dyn pocket_ui::document_check::ReadSeek>> {Some(Box::new(std::fs::File::open(uri).ok()?))}
    fn room_for_copy(&self,_:&pocket_ui::document_library::Game)->Option<u64> {Some(1<<30)}
}

fn paint_app_icon_variants(d: &Path, icon: Option<&pocket_ui::android_games::AppIcon>) {
    use pocket_ui::{font_style::{FontStyle,RetroLetters},i18n::Lang,settings::TextSize};
    let record=d.join("icon-words.txt"); std::env::set_var("POCKETUI_VISIBLE_TEXT",&record);
    for (style,letters) in [(FontStyle::Barlow,RetroLetters::Departure),(FontStyle::Retro,RetroLetters::Departure),(FontStyle::Retro,RetroLetters::Pixelify)] {
        let _style=pocket_ui::font_style::for_this_thread(style,letters);
        for lang in [Lang::En,Lang::De] {
            let _language=pocket_ui::i18n::for_this_thread(lang);
            let mut text=pocket_ui::text::Text::new();
            for size in TextSize::ALL {
                pocket_ui::type_scale::set(size);
                for font_scale in [0.85,1.0,2.0] {
                pocket_ui::type_scale::set_system_scale(font_scale);
                for side in [18,24,30,45,60] {
                    for (bg,ink) in [(0x101010,0xffffff),(0xffffff,0x202020)] {
                        let width=side+16; let mut pixels=vec![bg;(width*width)as usize];
                        let mut canvas=pocket_ui::gfx::Canvas::new(&mut pixels,width as u32,width as u32);
                        pocket_ui::text::boxes::reset();
                        pocket_ui::android_games::paint_app_icon(&mut canvas,&mut text,icon,8,8,side,ink);
                        pocket_ui::text::boxes::finish_canvas(&canvas);
                        if icon.is_none() {
                            let words=std::fs::read_to_string(&record).unwrap();
                            assert!(words.lines().any(|line|line=="App"),"missing icon must say App inside its own region: {style:?}/{letters:?}/{lang:?}/{size:?}/{font_scale}/{side}, words={words:?}");
                        }
                        assert!(pixels.iter().any(|p|*p!=bg),"icon cell must contain visible image or words");
                        if style==FontStyle::Barlow && lang==Lang::En && size==TextSize::Normal && font_scale==1.0 && side==30 && bg==0x101010 {
                            if let Some(dir)=std::env::var_os("POCKETUI_TEST_EVIDENCE") {
                                let dir=PathBuf::from(dir); std::fs::create_dir_all(&dir).unwrap();
                                let file=std::fs::File::create(dir.join(if icon.is_some(){"valid.png"}else{"missing.png"})).unwrap();
                                let mut encoder=png::Encoder::new(file,width as u32,width as u32); encoder.set_color(png::ColorType::Rgb);
                                let mut writer=encoder.write_header().unwrap();
                                let rgb:Vec<u8>=pixels.iter().flat_map(|p|[(*p>>16)as u8,(*p>>8)as u8,*p as u8]).collect();
                                writer.write_image_data(&rgb).unwrap();
                            }
                        }
                        for (at,p) in pixels.iter().enumerate().filter(|(_,p)|**p!=bg) {
                            let (x,y)=(at as i32%width,at as i32/width);
                            assert!((8..8+side).contains(&x)&&(8..8+side).contains(&y),"icon cell ink escaped its region at {x}/{y}: {p:x}; {style:?}/{letters:?}/{lang:?}/{size:?}/{font_scale}/{side}");
                        }
                    }
                }
                }
            }
        }
    }
}
