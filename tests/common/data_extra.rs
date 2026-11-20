//! Additional persistent readers for data_sources' bounded, per-variant subprocesses.
//! All credentials and signing material below are synthetic test values.
use std::path::{Path, PathBuf};
use pocket_ui::{achievements::{account::Account, http::{Request, Response, Transport}, mastery, offline}, android_games, android_handheld, archive, cheat_download, cheat_pack, j2me_java, j2me_keys, j2me_size, ports, recents, store, untar, update};

pub const SOURCES: &[&str] = &[
    #[cfg(feature="brick")] "brick-fkeys",
    #[cfg(feature="brick")] "brick-lights",
    #[cfg(feature="brick")] "brick-slider",
    "fan-recovery-note", "fan-beat", "dpad-remap", "starter-zip", "kids-normal-volume", "case-art-gba-rom",
    #[cfg(feature="brick")] "brick-assets-manifest",
    #[cfg(feature="brick")] "brick-asset-archive",
    #[cfg(feature="brick")] "brick-delta-file",
    #[cfg(feature="brick")] "brick-app-archive",
    #[cfg(feature="brick")] "brick-seed-source",
    #[cfg(feature="brick")] "brick-seed-target",
    #[cfg(feature="brick")] "brick-seed-marker",
    #[cfg(feature="brick")] "brick-seed-archive",
    "boot-rom-header", "boot-gbc-bios", "run-ahead-info", "core-resume-raw", "core-resume-compressed",
    "report-settings", "report-log", "report-network-profile", "report-core-info",
    "report-bluetooth-profile", "report-portal", "report-previous-portal", "report-store", "report-share-password", "report-remote-password", "report-hostname", "report-system-cfg", "report-core-error", "report-live-log", "report-previous-log",
    "pixel-disc-plain", "pixel-disc-rvz", "pixel-disc-wia", "pixel-disc-ciso", "pixel-disc-wbfs", "pixel-disc-gcz-stored", "pixel-disc-gcz-deflate",
    "installed-title-font", "installed-body-font", "elf-executable", "web-sessions", "timezone-file", "timezone-list",
    "sleep-resume", "power-restore", "rescue-es-note", "pixel-report", "pixel-standalone", "recent-searches", "wiki-cache", "eject-note", "sram-file", "sram-oversize", "rtc-file",
    "ra-account", "ra-mastery", "ra-pending", "ra-offline", "ra-summary", "ra-snapshot", "ra-console-progress", "port-catalog", "port-installed", "cheat-pack", "cheat-download",
    "android-core-manifest", "android-core-archive", "j2me-jar", "j2me-jad", "j2me-options", "j2me-remap", "j2me-java", "android-apps", "android-mobile-history", "android-sidecar",
    "emu-dolphin-gfx", "emu-dolphin-main", "emu-dolphin-game", "emu-gc-custom", "emu-gc-stock", "emu-wii-custom", "emu-wii-stock", "emu-azahar", "emu-pcsx2", "emu-pcsx2-game", "emu-system",
    "library-sync-own", "library-sync-foreign", "library-history-own", "library-history-foreign", "save-copy-index", "port-perf-game", "port-perf-console", "port-perf-notes", "snap-missing", "title-missing", "fallback-font",
    "port-game-files", "bios-psx", "bios-saturn", "bios-dreamcast", "update-pending", "update-known-good",
    "update-card", "update-payload", "seed-archive", "runtime-archive",
];
const GAME: &str = "Reader Rule Game";
const HASH: &str = "0123456789abcdef0123456789abcdef";
const KEY: &str = "0123456789abcdef";
const CHEAT: &[u8] = b"cheats = 1\ncheat0_desc = \"Infinite lives\"\ncheat0_code = \"010138CD\"\ncheat0_enable = false\n";
const CATALOG: &[u8] = br#"{"ports":{"rule.zip":{"name":"rule.zip","items":["Rule.sh","rule"],"attr":{"title":"Reader Rule Port","arch":["aarch64"],"rtr":true},"source":{"size":123,"md5":"0123456789abcdef0123456789abcdef"}}}}"#;
const PAYLOAD: &[u8] = b"reader rule optional payload";

fn dolphin_disc_seed(source: &str) -> Vec<u8> {
    let mut plain=vec![0;0x60];plain[..6].copy_from_slice(b"RULE01");plain[0x1c..0x20].copy_from_slice(&[0xc2,0x33,0x9f,0x3d]);
    match source {
        "pixel-disc-plain" => plain,
        "pixel-disc-rvz" | "pixel-disc-wia" => {let mut b=vec![0;0x60];b[..4].copy_from_slice(if source.ends_with("rvz") {b"RVZ\x01"}else{b"WIA\x01"});b[0x58..0x5e].copy_from_slice(b"RULE01");b},
        "pixel-disc-ciso" | "pixel-disc-wbfs" => {let at=if source.ends_with("ciso") {0x8000}else{0x200};let mut b=vec![0;at+6];b[..4].copy_from_slice(if at==0x8000 {b"CISO"}else{b"WBFS"});b[at..].copy_from_slice(b"RULE01");b},
        "pixel-disc-gcz-stored" | "pixel-disc-gcz-deflate" => {
            let stored=source.ends_with("stored");let block=if stored {plain}else{miniz_oxide::deflate::compress_to_vec_zlib(&plain,6)};
            let mut b=vec![0;0x2c];b[..4].copy_from_slice(&0xB10BC001u32.to_le_bytes());b[8..16].copy_from_slice(&(block.len() as u64).to_le_bytes());b[0x1c..0x20].copy_from_slice(&1u32.to_le_bytes());b[0x20..0x28].copy_from_slice(&(if stored {1u64<<63}else{0}).to_le_bytes());b.extend(block);b
        }
        _=>unreachable!(),
    }
}

fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new(); let mut central = Vec::new();
    for (name, data) in files {
        let crc = crc32fast::hash(data); let at = out.len() as u32;
        out.extend(0x04034b50u32.to_le_bytes()); out.extend(20u16.to_le_bytes()); out.extend([0; 8]);
        out.extend(crc.to_le_bytes()); out.extend((data.len() as u32).to_le_bytes()); out.extend((data.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes()); out.extend(0u16.to_le_bytes()); out.extend(name.as_bytes()); out.extend(*data);
        central.extend(0x02014b50u32.to_le_bytes()); central.extend(20u16.to_le_bytes()); central.extend(20u16.to_le_bytes()); central.extend([0; 8]);
        central.extend(crc.to_le_bytes()); central.extend((data.len() as u32).to_le_bytes()); central.extend((data.len() as u32).to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes()); central.extend([0; 12]); central.extend(at.to_le_bytes()); central.extend(name.as_bytes());
    }
    let at = out.len(); out.extend(&central); out.extend(0x06054b50u32.to_le_bytes()); out.extend([0; 4]);
    out.extend((files.len() as u16).to_le_bytes()); out.extend((files.len() as u16).to_le_bytes()); out.extend((central.len() as u32).to_le_bytes()); out.extend((at as u32).to_le_bytes()); out.extend([0; 2]); out
}
fn tar(files: &[(&str, &[u8])]) -> Vec<u8> {
    fn octal(field: &mut [u8], value: usize) { let s = format!("{:0width$o}", value, width=field.len()-1); field[..s.len()].copy_from_slice(s.as_bytes()); }
    let mut out = Vec::new();
    for (name, data) in files {
        let mut h = [0u8; 512]; h[..name.len()].copy_from_slice(name.as_bytes()); octal(&mut h[100..108], 0o755); octal(&mut h[108..116], 0); octal(&mut h[116..124], 0);
        octal(&mut h[124..136], data.len()); octal(&mut h[136..148], 1); h[148..156].fill(b' '); h[156] = b'0'; h[257..263].copy_from_slice(b"ustar\0"); h[263..265].copy_from_slice(b"00");
        let sum = h.iter().map(|&b| b as usize).sum(); octal(&mut h[148..155], sum); h[155] = b' '; out.extend(h); out.extend(*data); out.resize(out.len().div_ceil(512)*512, 0);
    }
    out.extend([0; 1024]); out
}
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f,0x8b,8,0,0,0,0,0,0,3]; out.extend(miniz_oxide::deflate::compress_to_vec(bytes, 6)); out.extend(crc32fast::hash(bytes).to_le_bytes()); out.extend((bytes.len() as u32).to_le_bytes()); out
}
fn java_archive() -> Vec<u8> { gzip(&tar(&[("rule-jdk/bin/java", b"#!/bin/sh\nexit 0\n"), ("rule-jdk/lib/modules", b"reader rule modules"), ("rule-jdk/conf/security", b"reader rule config")])) }
fn cheat_seed() -> Vec<u8> {
    let folder = b"Nintendo - Game Boy"; let name = b"Reader Rule Game.cht";
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(CHEAT,6);
    let index_at = 12 + 2 + folder.len() + 16;
    let index_len = 2 + name.len() + 20;
    let data_at = index_at + index_len;
    let mut out = b"PUICHT01".to_vec(); out.extend(1u32.to_le_bytes());
    out.extend((folder.len() as u16).to_le_bytes()); out.extend(folder); out.extend(1u32.to_le_bytes());
    out.extend((index_len as u32).to_le_bytes()); out.extend((index_at as u64).to_le_bytes());
    out.extend((name.len() as u16).to_le_bytes()); out.extend(name); out.extend((data_at as u64).to_le_bytes());
    out.extend((packed.len() as u32).to_le_bytes()); out.extend((CHEAT.len() as u32).to_le_bytes()); out.extend(crc32fast::hash(CHEAT).to_le_bytes()); out.extend(packed); out
}
fn signing_key() -> ed25519_dalek::SigningKey { ed25519_dalek::SigningKey::from_bytes(&[3; 32]) }
fn update_bundle() -> Vec<u8> {
    use base64::Engine; use ed25519_dalek::Signer;
    let app = gzip(&tar(&[("pocket-ui", b"reader rule ui"), ("pocket-core", b"reader rule core")]));
    let manifest = format!(r#"{{"format":1,"app":{{"version":"9.9.9","url":"https://example.invalid/pocket-ui-9.9.9-aarch64.tar.gz","sha256":"{}","size":{},"notes":"Reader rule update"}},"os":null}}"#, store::sha256_hex(&app), app.len());
    let signature = base64::engine::general_purpose::STANDARD.encode(signing_key().sign(manifest.as_bytes()).to_bytes());
    zip(&[("update.json", manifest.as_bytes()), ("update.json.sig", signature.as_bytes()), ("pocket-ui-9.9.9-aarch64.tar.gz", &app)])
}

pub fn location(source: &str, d: &Path) -> PathBuf {
    match source {
        source if source.starts_with("pixel-disc-") => d.join("discs/Rule.iso"),
        "report-bluetooth-profile" => d.join("report-storage/.cache/bluetooth/adapter/device/info"),
        "report-portal" => d.join("data/logs").join(pocket_ui::logs::PORTAL_LAST), "report-previous-portal" => d.join("data/logs/boot-1-reader").join(pocket_ui::logs::PORTAL_LAST),
        "report-store" => d.join("data/store/installed.json"), "report-share-password" => d.join("data/share/password"), "report-remote-password" => d.join("data/remote/password"),
        "report-hostname" => d.join("report-root/etc/hostname"), "report-system-cfg" => d.join("report-storage/.config/system/configs/system.cfg"), "report-core-error" => d.join("report-root/tmp/pocket-core.error"),
        "report-live-log" => d.join("absent-live-log"), "report-previous-log" => d.join("data/logs/boot-1-reader/pocket-ui.log"),
        "brick-fkeys" => d.join("data/brick-keys.json"), "brick-lights" => d.join("data/brick-lights.json"), "brick-slider" => d.join("data/brick-slider"),
        "fan-recovery-note" => d.join("fan.restore"), "fan-beat" => d.join("fan.beat"), "dpad-remap" => d.join("remaps/Reader Rule Game.rmp"),
        "starter-zip" => d.join("starter.zip"), "kids-normal-volume" => d.join("data/kids-normal-volume"),
        "case-art-gba-rom" => d.join("art/Rule.gba"),
        "boot-rom-header" => d.join("boot/Rule.gb"), "boot-gbc-bios" => d.join("boot/bios/gbc_bios.bin"),
        "run-ahead-info" => d.join("cores/reader_libretro.info"),
        "core-resume-raw" => d.join("saves/Rule.pstate.auto"), "core-resume-compressed" => d.join("saves/Rule.pstate.auto.z"),
        "report-settings" => d.join("data/settings.json"), "report-log" => d.join("data/logs/pocket-ui.log"),
        "report-network-profile" => d.join("report-storage/.config/NetworkManager/system-connections/reader.nmconnection"),
        "report-core-info" => d.join("report-storage/cores/reader_libretro.info"),
        "installed-title-font" => d.join("data/fonts/title.ttf"), "installed-body-font" => d.join("data/fonts/body.ttf"), "elf-executable" => d.join("elf/pocket-core"),
        "web-sessions" => d.join("data/web/browsers.json"), "timezone-file" => d.join("zoneinfo/UTC"), "timezone-list" => d.join("zones.txt"),
        "sleep-resume" => d.join("data/resume.json"), "power-restore" => d.join("power.restore"), "rescue-es-note" => d.join("rescue/es-fallback"),
        "pixel-report" => d.join("pixel-report.json"), "pixel-standalone" => d.join("storage/.config/dolphin-emu/GFX.ini"),
        "recent-searches" => d.join("data/search.json"), "wiki-cache" => pocket_ui::wiki_text::cache_path(&d.join("data"),GAME),
        "eject-note" => pocket_ui::eject::LastPicture::path(&d.join("data")),
        "sram-file" | "sram-oversize" => d.join("saves/Reader Rule Game.srm"), "rtc-file" => d.join("saves/Reader Rule Game.rtc"),
        "brick-assets-manifest" => d.join("asset-packages.json"), "brick-asset-archive" => d.join("asset-part.tar.gz"), "brick-delta-file" => d.join("brick-card/app/assets/rule.dat"), "brick-app-archive" => d.join("brick-app.tar.gz"),
        "brick-seed-source" => d.join("brick-seed/source/rule.bin"), "brick-seed-target" => d.join("brick-seed/target/rule.bin"), "brick-seed-marker" => d.join("brick-seed/target/.pocket-ui-data"), "brick-seed-archive" => d.join("brick-seed.tar.gz"),
        "emu-dolphin-gfx" => d.join("storage/.config/dolphin-emu/GFX.ini"),
        "emu-dolphin-main" => d.join("storage/.config/dolphin-emu/Dolphin.ini"),
        "emu-dolphin-game" => d.join("storage/.config/dolphin-emu/GameSettings/ID-Rule.ini"),
        "emu-gc-custom" => d.join("storage/.config/dolphin-emu/GamecubeControllerProfiles/GCPadNew.ini.custom"),
        "emu-gc-stock" => d.join("storage/.config/dolphin-emu/GamecubeControllerProfiles/GCPadNew.ini.south.stacked"),
        "emu-wii-custom" => d.join("storage/.config/dolphin-emu/WiiControllerProfiles/custom.ini"),
        "emu-wii-stock" => d.join("storage/.config/dolphin-emu/WiiControllerProfiles/vremote.ini"),
        "emu-azahar" => d.join("storage/.config/azahar/qt-config.ini"),
        "emu-pcsx2" => d.join("storage/.config/ARMSX2/inis/PCSX2.ini"),
        "emu-pcsx2-game" => d.join("storage/.config/ARMSX2/gamesettings/SLUS-21050_A1B2C3D4.ini"),
        "emu-system" => d.join("system.cfg"),
        "library-sync-own" => d.join("data/library-sync/READER1-READER2/library.json"), "library-sync-foreign" => d.join("data/library-sync/OTHER01-OTHER02/library.json"),
        "library-history-own" => d.join("data/library-sync/READER1-READER2/history.json"), "library-history-foreign" => d.join("data/library-sync/OTHER01-OTHER02/history.json"),
        "save-copy-index" => d.join("data/copy-state.json"),
        "port-perf-game" => d.join("data/game-settings/ports/Rule.json"), "port-perf-console" => d.join("data/game-settings/ports.json"), "port-perf-notes" => d.join("data/port-cpu.json"),
        "snap-missing" => d.join("data/snaps/gb/.missing"), "title-missing" => d.join("data/titles/gb/.missing"),
        "fallback-font" => d.join("fallback.ttf"), "port-game-files" => d.join("ports/rule/assets/Rule.dat"),
        "bios-psx" => d.join("bios/scph5501.bin"), "bios-saturn" => d.join("bios/saturn_bios.bin"), "bios-dreamcast" => d.join("bios/dc/dc_boot.bin"),
        "update-pending" => d.join("status/storage/pocket-ui/update-pending"), "update-known-good" => d.join("status/storage/pocket-ui/known-good"),
        "ra-account" => Account::path(&d.join("data")), "ra-mastery" => d.join(format!("data/achievements/mastered/{KEY}")),
        "ra-summary" => pocket_ui::achievements::snapshot::Summary::path(&d.join("data"), "gb", &d.join("gb/Reader Rule Game.gb")),
        "ra-snapshot" => d.join("pocket-core.achievements.json"), "ra-console-progress" => d.join("data/achievements/progress/4.json"),
        "ra-pending" => offline::Pending::path(&d.join("data")), "ra-offline" => d.join(format!("data/achievements/offline/{HASH}.json")),
        "port-catalog" => d.join("data/ports.json"), "port-installed" => d.join("ports/rule/port.json"),
        "cheat-pack" => d.join("data/cheats/cht.pack"), "cheat-download" => d.join("app/database/cheats-download.json"),
        "android-core-manifest" => d.join("core.json"), "android-core-archive" => d.join("core.tar.gz"),
        "j2me-jar" => d.join("j2me/Rule.jar"), "j2me-jad" => d.join("j2me/Rule.jad"), "j2me-options" => d.join("j2me/Rule.opt"), "j2me-remap" => d.join("j2me/Rule.rmp"),
        "j2me-java" => d.join("jdk.tar.gz"), "android-apps" => d.join("data/launcher.json"), "android-mobile-history" => d.join("data/mobile-history.json"), "android-sidecar" => d.join("gb/Reader Rule Game.cht"),
        "update-card" => d.join("card/pocket-ui-update-rule.zip"), "update-payload" => d.join("optional-payload"), "seed-archive" => d.join("bios-data.tar.gz"), "runtime-archive" => d.join("runtime.zip"),
        _ => panic!("unknown extra source {source}"),
    }
}
pub fn seed(source: &str, d: &Path) -> Vec<u8> {
    let path = location(source, d); std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    match source {
        source if source.starts_with("pixel-disc-") => {
            let gfx=location("emu-dolphin-gfx",d);std::fs::create_dir_all(gfx.parent().unwrap()).unwrap();std::fs::write(gfx,emu_seed("emu-dolphin-gfx")).unwrap();
            let game=d.join("storage/.config/dolphin-emu/GameSettings/RULE01.ini");std::fs::create_dir_all(game.parent().unwrap()).unwrap();std::fs::write(game,b"[Video_Settings]\nInternalResolution = 4\n").unwrap();
            dolphin_disc_seed(source)
        }
        "brick-fkeys" => br#"{"f1":"recent","f2":"screenshot"}"#.to_vec(),
        "brick-lights" => br#"{"zones":[{"use_own":true},{},{},{},{}]}"#.to_vec(), "brick-slider" => b"nothing\n".to_vec(),
        "fan-recovery-note" => {
            let sys=d.join("fake-fan-root/sys");std::fs::create_dir_all(&sys).unwrap();
            std::fs::write(sys.join("pwm"),b"200").unwrap();std::fs::write(sys.join("trip"),b"99000").unwrap();std::fs::write(sys.join("temp"),b"60000").unwrap();
            pocket_ui::fan_curve::Note {pwm:sys.join("pwm"),trips:vec![(sys.join("trip"),52000)],zone_temp:sys.join("temp")}.text().into_bytes()
        }
        "fan-beat" => b"1 1795003200 curve quiet 500 0\n".to_vec(),
        "dpad-remap" => pocket_ui::dpad_stick_rocknix::REMAP.as_bytes().to_vec(),
        "starter-zip" => zip(&[("Rule.gb",b"reader rule starter game")]), "kids-normal-volume" => b"37\n".to_vec(),
        "case-art-gba-rom" => {let mut rom=vec![0;1024];rom[4..8].copy_from_slice(&0x08000200u32.to_le_bytes());rom[0x62..0x64].copy_from_slice(&31u16.to_le_bytes());rom[0x200..0x400].fill(0x11);rom},
        "boot-rom-header" | "boot-gbc-bios" => {
            let mut rom=vec![0;0x200];rom[0x143]=0x80;
            std::fs::create_dir_all(d.join("boot/bios")).unwrap();
            if source=="boot-rom-header" {std::fs::write(d.join("boot/bios/gbc_bios.bin"),vec![0;2304]).unwrap();rom}
            else {std::fs::write(d.join("boot/Rule.gb"),rom).unwrap();vec![0;2304]}
        }
        "run-ahead-info" => b"display_name = \"Reader Rule Core\"\nsavestate_features = \"deterministic\"\n".to_vec(),
        "core-resume-raw" | "core-resume-compressed" => {
            let _=memory_core(d);let mut raw=vec![0;8256];raw[..9].copy_from_slice(b"STUBSTATE");raw[16..20].copy_from_slice(&37u32.to_ne_bytes());
            if source=="core-resume-compressed" {pocket_ui::state_file::write(&path,&raw).unwrap();std::fs::read(path).unwrap()} else {raw}
        }
        source if source.starts_with("report-") => {
            let core=d.join("report-storage/cores/reader_libretro.so");std::fs::create_dir_all(core.parent().unwrap()).unwrap();std::fs::write(core,b"reader rule core").unwrap();
            match source {
                "report-settings" => br#"{"reader_rule":"present"}"#.to_vec(),
                "report-log" => b"reader rule diagnostic line\n".to_vec(),
                "report-network-profile" => b"[wifi]\nssid=RulePrivateNetwork\n[wifi-security]\npsk=RuleSyntheticPassword\n".to_vec(),
                "report-core-info" => b"display_version = \"rule-version-7\"\n".to_vec(),
                "report-bluetooth-profile" => b"[General]\nName=Reader's Headphones\nAlias=Reader's Headphones\n".to_vec(),
                "report-portal" | "report-previous-portal" => b"<html>Reader rule portal page</html>".to_vec(),
                "report-store" => br#"{"reader_rule":"installed"}"#.to_vec(),
                "report-share-password" | "report-remote-password" => b"reader-synthetic-report-password\n".to_vec(),
                "report-hostname" => b"reader-private-host\n".to_vec(),
                "report-system-cfg" => b"wifi.ssid=RulePrivateNetwork\nwifi.key=RuleSyntheticPassword\n".to_vec(),
                "report-core-error" | "report-live-log" | "report-previous-log" => b"Reader rule diagnostic line\n".to_vec(),
                _=>unreachable!(),
            }
        }
        "installed-title-font" | "installed-body-font" => std::fs::read(PathBuf::from(super::Root::dir()).join("assets/fonts/CaveatBrush-Regular.ttf")).unwrap(),
        "elf-executable" => std::fs::read(memory_core(d)).unwrap(),
        "web-sessions" => {let now=pocket_ui::recents::now_secs();serde_json::to_vec(&serde_json::json!({"key":"a".repeat(64),"browsers":[{"id":"ReaderRule","hash":"b".repeat(64),"name":"Reader Rule Browser","paired":now,"last_seen":now}]})).unwrap()},
        "timezone-file" => {let mut b=vec![0;44];b[..4].copy_from_slice(b"TZif");b[36..40].copy_from_slice(&1u32.to_be_bytes());b[40..44].copy_from_slice(&4u32.to_be_bytes());b.extend([0;6]);b.extend(b"UTC\0");b},
        "timezone-list" => b"UTC\nEurope/Vienna\n".to_vec(),
        "sleep-resume" => serde_json::to_vec(&pocket_ui::sleep::Resume {system:"gb".into(),name:GAME.into(),path:d.join("gb/Reader Rule Game.gb")}).unwrap(),
        "power-restore" => br#"{"cpu":{},"fan_zone":"","fan":{},"gpu":{},"ours":{}}"#.to_vec(),
        "rescue-es-note" => b"Reader rule startup failed\n".to_vec(),
        "pixel-report" => serde_json::to_vec(&pocket_ui::pixel_path::Report {seq:7,rom:d.join("gb/Reader Rule Game.gb").display().to_string(),..Default::default()}).unwrap(),
        "pixel-standalone" => { std::fs::write(d.join("Rule.dol"),b"reader rule executable").unwrap(); emu_seed("emu-dolphin-gfx").into_bytes() },
        "recent-searches" => br#"{"list":["Reader Rule Game"]}"#.to_vec(), "wiki-cache" => b"Reader Rule Game is a made-up game for the file-source rule.".to_vec(),
        "eject-note" => serde_json::to_vec(&pocket_ui::eject::LastPicture {key:KEY.into(),picture:None,rect:None,window:(640,480),saved:true,at_ms:1794139200000}).unwrap(),
        "sram-file" | "sram-oversize" | "rtc-file" => { let _=memory_core(d); vec![23;match source {"sram-file"=>8192,"sram-oversize"=>65536,_=>16}] },
        "brick-assets-manifest" => brick_asset_manifest(&brick_asset_archive()),
        "brick-asset-archive" => brick_asset_archive(),
        "brick-delta-file" => { std::fs::write(d.join("brick-card/app/start.sh"),b"#!/bin/sh\n").unwrap(); vec![7;65536] },
        "brick-app-archive" => brick_app(None),
        source if source.starts_with("brick-seed-") => {
            std::fs::create_dir_all(d.join("brick-seed/source")).unwrap(); std::fs::create_dir_all(d.join("brick-seed/target")).unwrap();
            if source!="brick-seed-source" { std::fs::write(d.join("brick-seed/source/rule.bin"),b"reader rule seed").unwrap(); }
            if source=="brick-seed-marker" {b"9.9.9\n".to_vec()} else if source=="brick-seed-archive" {gzip(&tar(&[("bios-data/rule.bin",b"reader rule seed")]))} else {b"reader rule seed".to_vec()}
        }
        source if source.starts_with("emu-") => {
            std::fs::write(d.join("Rule.dol"), b"reader rule game").unwrap();
            // Supply valid surrounding INIs, so the independent Pixel Path reader reaches the selected file too.
            for other in ["emu-dolphin-gfx","emu-dolphin-main","emu-azahar","emu-pcsx2"] {
                let other_path=location(other,d);
                if other_path!=path && !other_path.exists() {std::fs::create_dir_all(other_path.parent().unwrap()).unwrap();std::fs::write(other_path,emu_seed(other)).unwrap();}
            }
            if source.contains("-custom") || source.contains("-stock") { b"[GCPad1]\nButtons/A = `Button S`\n[Wiimote1]\nButtons/A = `Button E`\n".to_vec() }
            else { emu_seed(source).into_bytes() }
        }
        "library-sync-own" | "library-sync-foreign" => br#"{"version":1,"device":"OTHER01-OTHER02","games":{"gb/Reader Rule Game.gb":{"f":{"reader-sentinel":{"v":"present","t":1,"d":"OTHER01-OTHER02"}}}}}"#.to_vec(),
        "library-history-own" | "library-history-foreign" => br#"{"version":1,"device":"OTHER01-OTHER02","games":{},"sessions":[{"g":"gb/Reader Rule Game.gb","s":1794139200,"secs":60,"e":true,"em":"pocket-core"}],"statuses":[]}"#.to_vec(),
        "save-copy-index" => br#"{"games":{"0123456789abcdef":{"dir":"Reader Rule","files":{}}},"conflicts":{},"last":1794139200}"#.to_vec(),
        "port-perf-game" | "port-perf-console" => br#"{"performance":"battery","fan":"quiet"}"#.to_vec(),
        "port-perf-notes" => br#"{"Rule.sh":{"busy":true,"top":1.0,"rest":0.1,"at":1794139200,"seen":false}}"#.to_vec(),
        "snap-missing" | "title-missing" => b"Reader Rule Game\n".to_vec(),
        "fallback-font" => std::fs::read(PathBuf::from(super::Root::dir()).join("assets/fonts/Barlow-Medium.ttf")).unwrap(),
        "port-game-files" => b"reader rule game data".to_vec(),
        "bios-psx" | "bios-saturn" => vec![0; 512 << 10], "bios-dreamcast" => vec![0; 2 << 20],
        "update-pending" | "update-known-good" => { std::fs::create_dir_all(d.join("status/app")).unwrap(); std::fs::write(d.join("status/app/start.sh"), b"reader rule").unwrap(); b"9.9.9\n".to_vec() },
        "ra-summary" => serde_json::to_vec(&pocket_ui::achievements::snapshot::Summary { game_id:7, hash:HASH.into(), total:4, unlocked:2, updated:1, ..Default::default() }).unwrap(),
        "ra-snapshot" => serde_json::to_vec(&pocket_ui::achievements::snapshot::Snapshot { rom:d.join("gb/Reader Rule Game.gb").display().to_string(), state:pocket_ui::achievements::snapshot::State::Ready, ..Default::default() }).unwrap(),
        "ra-console-progress" => {
            let summary=location("ra-summary",d); std::fs::create_dir_all(summary.parent().unwrap()).unwrap(); std::fs::write(summary,seed("ra-summary",d)).unwrap();
            br#"{"updated":1794139200,"games":[[7,4,2]]}"#.to_vec()
        }
        "ra-account" => br#"{"username":"ReaderRule","token":"synthetic-test-token","display_name":"Reader Rule","hardcore":false}"#.to_vec(),
        "ra-mastery" => b"Reader Rule Game".to_vec(),
        "ra-pending" => format!(r#"{{"username":"ReaderRule","unlocks":[{{"id":7,"hash":"{HASH}","when":1794139200,"hardcore":false}}]}}"#).into_bytes(),
        "ra-offline" => br#"{"sets":{"Success":true,"GameID":7,"Sets":[]},"session":{"Success":true,"Unlocks":[{"ID":7,"When":1794139200}]}}"#.to_vec(),
        "port-catalog" => CATALOG.to_vec(),
        "port-installed" => br#"{"name":"rule.zip","items":["Rule.sh","rule"],"attr":{"title":"Reader Rule Port","rtr":true},"status":{"md5":"0123456789abcdef0123456789abcdef"}}"#.to_vec(),
        "cheat-pack" => cheat_seed(),
        "cheat-download" => format!(r#"{{"version":"1","url":"https://example.invalid/cht.pack","sha256":"{}","size":123,"notes":"Reader rule pack"}}"#, "a".repeat(64)).into_bytes(),
        "android-core-manifest" => br#"{"id":"reader","corresponding_source":"reader-source.tar.gz","source_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#.to_vec(),
        "android-core-archive" => core_archive(Some(&seed("android-core-manifest",d))),
        "j2me-jar" => zip(&[("META-INF/MANIFEST.MF", b"MIDlet-Name: Rule\nMIDlet-Screen-Size: 240x320\n")]),
        "j2me-jad" => { std::fs::write(d.join("j2me/Rule.jar"), b"no manifest").unwrap(); b"MIDlet-Name: Rule\nMIDlet-Screen-Size: 240x320\n".to_vec() },
        "j2me-options" => b"freej2me_resolution = \"240x320\"\nreader_preserved = \"yes\"\n".to_vec(),
        "j2me-remap" => b"input_player1_btn_a = \"8\"\nreader_preserved = \"yes\"\n".to_vec(),
        "j2me-java" => { std::fs::create_dir_all(d.join("java/storage")).unwrap(); java_archive() },
        "android-apps" => br#"{"android_games":{"xyz.reader.rule":"Reader Rule Game"},"plays_with":{"gb":"xyz.reader.rule"},"plays_with_game":{"gb/Reader Rule Game.gb":"xyz.reader.rule"},"gameboy_gambatte":true}"#.to_vec(),
        "android-mobile-history" => br#"{"com.android.externalstorage.documents:primary:ROMs/gb/Rule.gb":[1794139200,7]}"#.to_vec(),
        "android-sidecar" => CHEAT.to_vec(), "update-card" => update_bundle(), "update-payload" => PAYLOAD.to_vec(),
        "seed-archive" => gzip(&tar(&[("bios-data/rule.bin", b"reader rule bios data")])),
        "runtime-archive" => zip(&[("rule/runtime", b"reader rule runtime")]), _ => unreachable!(),
    }
}
struct Offline;
impl Transport for Offline { fn call(&self, _: &Request) -> Response { Response::unreachable() } }
fn decoded(bytes: Option<&[u8]>) -> String { bytes.and_then(|b| std::str::from_utf8(b).ok()).unwrap_or_default().to_string() }

pub fn probe(source: &str, d: &Path, file: &Path, bytes: Option<&[u8]>, valid: bool) {
    let data = d.join("data");
    match source {
        source if source.starts_with("pixel-disc-") => {
            let files=pocket_ui::emu_settings::Files {root:d.to_path_buf(),system_cfg:d.join("system.cfg")};
            let got=pocket_ui::pixel_standalone::read(pocket_ui::standalone::Emu::Dolphin,"gamecube",file,None,&files);
            if valid {let got=got.unwrap();assert!(got.sources.iter().any(|s|s=="GameSettings/RULE01.ini"));assert_eq!(got.resolution,Some(pocket_ui::pixel_standalone::Resolution::Times(2.0)));}else if let Err(why)=got {assert!(!why.trim().is_empty());}
        }
        #[cfg(feature="brick")]
        "brick-fkeys" => {let got=pocket_ui::brick_hw::FKeys::load(&data);assert!(!got.f1.label().is_empty() && !got.f2.label().is_empty());if valid {assert_eq!(got.f1,pocket_ui::brick_hw::FKey::Recent);assert_eq!(got.f2,pocket_ui::brick_hw::FKey::Screenshot);}}
        #[cfg(feature="brick")]
        "brick-lights" => {let got=pocket_ui::brick_hw::Lights::load(&data);if valid {assert!(got.zones[0].use_own);}let _=got.zones[0].colour((100,120,140));}
        #[cfg(feature="brick")]
        "brick-slider" => {let got=pocket_ui::brick_hw::Slider::load(&data);assert!(!got.label().is_empty());if valid {assert_eq!(got,pocket_ui::brick_hw::Slider::Nothing);}}
        "fan-recovery-note" => {let _=pocket_ui::fan_curve::hand_back(file,&d.join("fake-fan-root"));if valid {assert_eq!(std::fs::read_to_string(d.join("fake-fan-root/sys/trip")).unwrap(),"52000");assert!(!file.exists());}}
        "fan-beat" => {let got=pocket_ui::fan_curve::Beat::read(file,1795003201);if valid {let got=got.unwrap();assert!(got.driving);assert_eq!(got.ema,Some(50000));}}
        "dpad-remap" => {pocket_ui::dpad_stick_rocknix::apply_remap(file.parent().unwrap(),&d.join("Reader Rule Game.gb"),false).unwrap();if valid {assert!(!file.exists());}}
        "starter-zip" => {
            let original=zip(&[("Rule.gb",b"reader rule starter game")]);let mut game=pocket_ui::starter::games()[0].clone();
            game.id="reader-rule".into();game.file=pocket_ui::starter::File {url:"https://example.invalid/rule.zip".into(),sha256:store::sha256_hex(&original),size:original.len() as u64,name_on_disk:"Rule.gb".into(),archive:Some("zip".into()),rom_sha256:Some(store::sha256_hex(b"reader rule starter game")),rom_size:Some(24)};
            let dest=d.join("starter-games");let work=d.join("starter-work");let env=pocket_ui::starter::Env {dest_dir:&dest,work_dir:&work,free_dest:None,free_work:None,online:true};
            let fetch=|_:&str,path:&Path,_:Option<u64>,_:&dyn Fn(u64)|std::fs::copy(file,path).map(|_|()).map_err(|e|store::DownloadFailure::Broke(e.to_string()));
            let got=pocket_ui::starter::install(&game,&env,&fetch,&|_|{});
            if valid {assert!(matches!(got,Ok(pocket_ui::starter::Outcome::Added(_))));assert_eq!(std::fs::read(dest.join("Rule.gb")).unwrap(),b"reader rule starter game");}else if let Err(why)=got {assert!(!why.message().trim().is_empty());}
        }
        "kids-normal-volume" => {
            std::env::set_var("POCKETUI_MOCK","1");let hw=std::sync::Arc::new(pocket_ui::system_ctl::MockHw::default());let mut sys=pocket_ui::quick::Sys::for_test(hw.clone(),data.clone());
            sys.kids_volume_leave(&data);if valid {assert_eq!(hw.state().volume,37);}assert!(!file.exists());
        }
        "case-art-gba-rom" => {
            let places=pocket_ui::case_knows_art::Places::parse("0 40 60 80 a0 - - -").unwrap();let team=pocket_ui::case_knows::TeamArt {team:vec![1],badges:0};
            let got=pocket_ui::case_knows_art::art(file,&places,&team);if valid {assert_eq!(got.team.len(),1);assert_eq!((got.team[0].w,got.team[0].h),(32,32));assert!(got.team[0].px.iter().any(|p|p & 0xff000000!=0));}
        }
        "boot-rom-header" | "boot-gbc-bios" => {
            let got=pocket_ui::boot_screen::plan("gambatte","gb",&d.join("boot/Rule.gb"),&d.join("boot/bios"));
            if valid {assert!(matches!(got,pocket_ui::boot_screen::Boot::Screen(_)));}
            if let pocket_ui::boot_screen::Boot::Missing(name)=got {assert!(!pocket_ui::boot_screen::missing_note(name).trim().is_empty());}
        }
        "run-ahead-info" => {let got=pocket_ui::run_ahead::core_info_deterministic(&d.join("cores/reader_libretro.so"));if valid {assert_eq!(got,Some(true));}}
        "core-resume-raw" | "core-resume-compressed" => {
            let shared=memory_core(d);let mut core=pocket_ui::libretro::Core::load(&shared,&d.join("gb/Reader Rule Game.gb"),&data,&d.join("saves"),&[]).unwrap();
            let got=core.resume(file);if valid {assert_eq!(got,pocket_ui::libretro::Resume::Resumed);let raw=core.save_state().unwrap();assert_eq!(&raw[16..20],&37u32.to_ne_bytes());}
            let (picture,sound,..)=core.run(0);assert!(picture.is_some() && sound.iter().any(|&s|s!=0));
        }
        source if source.starts_with("report-") => {
            let src=pocket_ui::report::Sources {data_dir:data.clone(),live_log:d.join("absent-live-log"),root:d.join("report-root"),storage:d.join("report-storage"),battery:d.join("report-battery"),pocket_core:d.join("absent-pocket-core"),out_dir:d.join("reports")};
            let local=pocket_ui::report::Stamp {year:2026,month:11,day:18,hour:12,min:0,sec:0};
            let got=pocket_ui::report::entries(&src,&[],1795003200,&local);
            let readme=got.iter().find(|(n,_)|n=="README.txt").unwrap();assert!(!String::from_utf8_lossy(&readme.1).trim().is_empty());
            if valid {match source {
                "report-settings" => assert!(got.iter().any(|(n,b)|n=="settings/settings.json" && String::from_utf8_lossy(b).contains("reader_rule"))),
                "report-log" => assert!(got.iter().any(|(n,b)|n=="logs/pocket-ui.log" && String::from_utf8_lossy(b).contains("reader rule diagnostic"))),
                "report-network-profile" => {let secrets=pocket_ui::report::known_secrets(&src);assert!(secrets.iter().any(|(s,_)|s=="RulePrivateNetwork"));assert!(secrets.iter().any(|(s,_)|s=="RuleSyntheticPassword"));},
                "report-core-info" => assert!(got.iter().any(|(n,b)|n=="emulators.txt" && String::from_utf8_lossy(b).contains("rule-version-7"))),
                "report-bluetooth-profile" => assert!(pocket_ui::report::known_secrets(&src).iter().any(|(s,_)|s=="Reader's Headphones")),
                "report-portal" | "report-previous-portal" => assert!(got.iter().any(|(n,b)|n.ends_with(pocket_ui::logs::PORTAL_LAST) && String::from_utf8_lossy(b).contains("Reader rule portal page"))),
                "report-store" => assert!(got.iter().any(|(n,b)|n=="settings/store-installed.json" && String::from_utf8_lossy(b).contains("reader_rule"))),
                "report-share-password" | "report-remote-password" => assert!(pocket_ui::report::known_secrets(&src).iter().any(|(s,_)|s=="reader-synthetic-report-password")),
                "report-hostname" => assert!(pocket_ui::report::known_secrets(&src).iter().any(|(s,_)|s=="reader-private-host")),
                "report-system-cfg" => assert!(pocket_ui::report::known_secrets(&src).iter().any(|(s,_)|s=="RulePrivateNetwork")),
                "report-core-error" | "report-live-log" | "report-previous-log" => assert!(got.iter().any(|(n,b)|n.starts_with("logs/") && String::from_utf8_lossy(b).contains("Reader rule diagnostic line"))),
                _=>unreachable!(),
            }}
        }
        "installed-title-font" | "installed-body-font" => {
            std::env::set_var("POCKETUI_DATA",&data);let mut text=pocket_ui::text::Text::new().pinned_to(pocket_ui::font_style::FontStyle::Barlow);
            assert!(text.width(pocket_ui::text::Face::Title,24,"Reader Rule")>0);assert!(text.width(pocket_ui::text::Face::Body,24,"Reader Rule")>0);
            if valid {let face=if source=="installed-title-font" {pocket_ui::text::Face::Title}else{pocket_ui::text::Face::Body};let mut builtin=pocket_ui::text::Text::builtin();assert!(["Reader Rule","iiiiMMMM","WWWW0000"].iter().any(|word|text.width(face,32,word)!=builtin.width(face,32,word)),"the installed Caveat Brush face must replace the built-in Barlow metrics");}
        }
        "elf-executable" => {let got=pocket_ui::os_compat::Needs::installed(&d.join("elf")); if valid {assert!(!got.linked.is_empty());assert!(!got.notes.iter().any(|n|n.starts_with("pocket-core")));} }
        "web-sessions" => {let got=pocket_ui::web::auth::Sessions::load(&data);if valid {assert_eq!(got.browsers[0].name,"Reader Rule Browser");}}
        "timezone-file" => {let got=pocket_ui::timezone::Tz::load(&d.join("zoneinfo"),"UTC");if valid {assert_eq!(got.unwrap().offset_at(1794139200),0);}}
        "timezone-list" => {let got=pocket_ui::timezone::list(&d.join("zoneinfo"),file);if valid {assert!(got.contains(&"UTC".to_string()));}}
        "sleep-resume" => {let got=pocket_ui::sleep::take_resume(&data);if valid {assert_eq!(got.unwrap().name,GAME);}}
        "power-restore" => { let got=pocket_ui::power::Power::with_root(d.join("fake-power-root"),file).recover(); if valid {assert!(got);assert!(!PathBuf::from(format!("{}.bad",file.display())).exists());} }
        "rescue-es-note" => {let got=pocket_ui::rescue::take_es_note(&d.join("rescue"));if valid {assert!(got.unwrap().contains("Reader rule startup failed"));}else if let Some(words)=got {assert!(!words.trim().is_empty());}}
        "pixel-report" => {let got=pocket_ui::pixel_path::Report::read(file,&d.join("gb/Reader Rule Game.gb"),7);if valid {assert_eq!(got.unwrap().seq,7);}}
        "pixel-standalone" => {let got=pocket_ui::pixel_standalone::read(pocket_ui::standalone::Emu::Dolphin,"gamecube",&d.join("Rule.dol"),None,&pocket_ui::emu_settings::Files {root:d.to_path_buf(),system_cfg:d.join("system.cfg")});if valid {assert!(!got.unwrap().sources.is_empty());}else if let Err(why)=got {assert!(!why.trim().is_empty());}}
        "recent-searches" => {let got=pocket_ui::search::Recent::load(&data);if valid {assert_eq!(got.list,vec![GAME]);}}
        "wiki-cache" => {let got=pocket_ui::wiki_text::cached(&data,GAME);if valid {assert!(got.unwrap().contains("made-up game"));}}
        "eject-note" => {let got=pocket_ui::eject::LastPicture::take(&data,1794139200001);if valid {assert!(got.unwrap().saved);}}
        "sram-file" | "sram-oversize" | "rtc-file" => {
            let shared=memory_core(d);let core=pocket_ui::libretro::Core::load(&shared,&d.join("gb/Reader Rule Game.gb"),&data,&d.join("saves"),&[]).unwrap();
            if valid {unsafe {let library=libloading::Library::new(&shared).unwrap();let memory:libloading::Symbol<unsafe extern "C" fn(u32)->*const u8>=library.get(b"retro_get_memory_data").unwrap();let id=if source=="rtc-file" {1}else{0};let n=if id==0 {8192}else{16};assert_eq!(std::slice::from_raw_parts(memory(id),n),vec![23;n]);}}
            let (picture,sound,..)=core.run(0);assert!(picture.is_some() && sound.iter().any(|&s|s!=0));
        }
        #[cfg(feature="brick")]
        "brick-assets-manifest" | "brick-asset-archive" | "brick-delta-file" | "brick-app-archive" => brick_install_probe(source,d,bytes,valid),
        #[cfg(feature="brick")]
        "brick-seed-source" | "brick-seed-target" | "brick-seed-marker" => {
            let source_dir=d.join("brick-seed/source"); let dest=d.join("brick-seed/target");
            if source=="brick-seed-marker" {
                update::seed_brick_data_for_release(&source_dir,&dest,"9.9.9").unwrap();
                let marker=std::fs::read(dest.join(".pocket-ui-data")).unwrap(); assert_eq!(marker.strip_suffix(b"\n").unwrap_or(&marker),b"9.9.9");
            } else {
                update::seed_brick_data(&source_dir,&dest).unwrap();
                if source=="brick-seed-target" || valid {assert_eq!(std::fs::read(dest.join("rule.bin")).unwrap(),b"reader rule seed");}
            }
        }
        #[cfg(feature="brick")]
        "brick-seed-archive" => {
            let got=untar::unpack_file_throttled(file,&d.join("brick-unpack"),1<<20);
            if valid {got.unwrap();assert_eq!(std::fs::read(d.join("brick-unpack/bios-data/rule.bin")).unwrap(),b"reader rule seed");}
        }
        source if source.starts_with("emu-") => emu_probe(source,d,bytes,valid),
        "library-sync-own" | "library-sync-foreign" | "library-history-own" | "library-history-foreign" => {
            let root=data.join("library-sync"); let find=|_: &str| Some(d.join("gb/Reader Rule Game.gb"));
            let got=pocket_ui::library_sync::pass(&pocket_ui::library_sync::Setup { data_dir:&data,root:&root,me:"READER1-READER2",now:1794139200000,log:Some(std::sync::Arc::new(Default::default())),find:&find });
            if valid {
                if source.contains("history") { assert!(got.history.unwrap().sessions.iter().any(|s|s.s==1794139200 && s.secs==60)); }
                else { assert!(got.state.games["gb/Reader Rule Game.gb"].f.contains_key("reader-sentinel")); }
                if source.ends_with("foreign") { assert!(got.devices.contains(&"OTHER01-OTHER02".to_string())); }
            }
        }
        "save-copy-index" => { let got=pocket_ui::save_copy::Index::load(&data); if valid { assert_eq!(got.last,1794139200); assert!(got.games.contains_key(KEY)); } }
        "port-perf-game" | "port-perf-console" | "port-perf-notes" => {
            let got=pocket_ui::port_perf::Store::load(&data); let rom=d.join("Rule.sh");
            if valid {
                if source.ends_with("notes") { assert!(got.note(&rom).unwrap().busy); }
                else { let chosen=got.choice(&rom,Default::default()); assert_eq!(chosen.performance,pocket_ui::power::Performance::Battery); assert_eq!(chosen.fan,pocket_ui::power::Fan::Quiet); }
            }
        }
        "snap-missing" | "title-missing" => { let kind=if source=="snap-missing" {pocket_ui::game_pictures::Kind::Snap} else {pocket_ui::game_pictures::Kind::Title}; let got=pocket_ui::game_pictures::listed_missing(&data,kind,"gb"); if valid { assert!(got.contains(GAME)); } }
        "fallback-font" => { let got=pocket_ui::text_fallback::open(file); if valid { assert!(got.unwrap().has('A')); } }
        "port-game-files" => {
            let expected=pocket_ui::port_files::Expect { folders:vec!["assets".into()],sure:true,files:vec!["Rule.dat".into()],..Default::default() };
            let got=pocket_ui::port_files::check(&d.join("ports/rule"),None,&expected,false); if valid { assert_eq!(got,pocket_ui::port_files::Verdict::Launch); }
        }
        "bios-psx" | "bios-saturn" | "bios-dreamcast" => { let system=source.strip_prefix("bios-").unwrap(); let got=pocket_ui::document_library::bios_hint(system,&[],&d.join("bios")); if valid { assert!(got.is_none()); } else if let Some(why)=got { assert!(!why.trim().is_empty()); } }
        "update-pending" | "update-known-good" => {
            use update::observation::{Inspection,Layout,HealthObservation,Pending};
            let got=Inspection::read(Layout::Brick,&d.join("status"),"9.9.9",&d.join("status/app"),HealthObservation::Unobserved);
            if valid { if source=="update-pending" { assert_eq!(got.pending,Pending::Ours("9.9.9".into())); } else { assert!(got.recorded_stable); } }
        }
        "ra-summary" => {
            let rom=d.join("gb/Reader Rule Game.gb"); let got=pocket_ui::achievements::snapshot::Summary::load(&data,"gb",&rom);
            let tally=pocket_ui::achievements::snapshot::console_tally(&data,"gb",std::iter::once(rom.as_path()));
            if valid { assert_eq!(got.unwrap().text().as_deref(),Some("2 of 4")); assert_eq!(tally,Some((2,4,1))); }
        }
        "ra-snapshot" => { let got=pocket_ui::achievements::snapshot::Snapshot::read(file,&d.join("gb/Reader Rule Game.gb")); if valid { assert_eq!(got.unwrap().state,pocket_ui::achievements::snapshot::State::Ready); } }
        "ra-console-progress" => {
            let account=Account { username:"ReaderRule".into(),token:"synthetic-test-token".into(),..Default::default() };
            let got=pocket_ui::achievements::lookup::lookup(&Offline,&data,&account,"gb",&d.join("gb/Reader Rule Game.gb"),1794139200,false);
            if valid { let got=got.unwrap(); assert_eq!((got.game_id,got.total,got.unlocked),(7,4,2)); }
        }
        "ra-account" => { let got = Account::load(&data); if valid { assert_eq!(got.unwrap().shown_name(), "Reader Rule"); } }
        "ra-mastery" => { let got = mastery::pending(&data); if valid { assert_eq!(got, vec![(KEY.into(), GAME.into())]); } }
        "ra-pending" => { let got = offline::Pending::load(&data, "ReaderRule"); if valid { assert_eq!(got.ids_for(HASH), vec![7]); } }
        "ra-offline" => {
            let account = Account { username: "ReaderRule".into(), token: "synthetic-test-token".into(), ..Default::default() };
            for command in ["achievementsets", "startsession"] {
                let got = offline::exchange(&Offline, &data, &account, &Default::default(), &Request { url:"https://example.invalid".into(), post_data:Some(format!("r={command}&m={HASH}")), content_type:None });
                if valid { assert_eq!(got.status, 200); assert_eq!(serde_json::from_slice::<serde_json::Value>(&got.body).unwrap()["Success"], true); }
            }
        }
        "port-catalog" => {
            if !file.exists() {
                // Exercise the actual missing-cache read without reaching PortMaster or a captive-portal probe.
                let bin=d.join("offline-bin"); std::fs::create_dir_all(&bin).unwrap(); let curl=bin.join("curl");
                std::fs::write(&curl,b"#!/bin/sh\nexit 7\n").unwrap(); std::fs::set_permissions(&curl,std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
                let mut paths=vec![bin]; paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
                std::env::set_var("PATH",std::env::join_paths(paths).unwrap());
            }
            let got=ports::catalog(&data);
            if valid { assert_eq!(got.unwrap()[0].title,"Reader Rule Port"); }
            else if let Err(why)=got { assert!(!why.trim().is_empty()); }
            ports::load_versions(&data);
        }
        "port-installed" => {
            let got = ports::installed_port(&d.join("ports"), &d.join("ports/Rule.sh")); let index = ports::scan_installed(&d.join("ports")); let _ = ports::installed_needing_files(&d.join("ports"));
            if valid { assert_eq!(got.unwrap().title, "Reader Rule Port"); assert_eq!(index["Rule.sh"].md5.as_deref(), Some(HASH)); }
        }
        "cheat-pack" => {
            let got = cheat_pack::list(file, "Nintendo - Game Boy"); let installed = cheat_download::installed(&[file.to_path_buf()]);
            if let Some(entries) = got { for entry in &entries { let content = cheat_pack::read(entry); if valid { assert_eq!(content.as_deref(), Some(CHEAT)); } } if valid { assert_eq!(entries.len(),1); } }
            else { assert!(!valid); }
            if valid { assert!(installed); }
        }
        "cheat-download" => { let got = update::cheats::description(file); if valid { assert_eq!(got.unwrap().size,123); } }
        "android-core-manifest" | "android-core-archive" => {
            let archive=if source=="android-core-manifest" {core_archive(bytes)} else {bytes.unwrap_or_default().to_vec()};
            let correct_archive=core_archive(Some(&seed("android-core-manifest",d)));
            let expected=if source=="android-core-manifest" {&archive} else {&correct_archive};
            let json=serde_json::json!({"packages":[{"id":"android-core-reader","name":"Reader Rule Core","category":"android-cores","version":"1","description":"Reader rule","size":expected.len(),"url":"core.tar.gz","sha256":store::sha256_hex(expected),"kind":"tar","dest":"/never-write-here","core":"reader","systems":["gb"],"library_sha256":store::sha256_hex(b"reader rule library"),"corresponding_source":"reader-source.tar.gz","source_sha256":"a".repeat(64)}]});
            let package=pocket_ui::android_cores::packages(json.to_string().as_bytes()).unwrap().remove(0);
            let got=pocket_ui::android_cores::install(&data,&package,&archive);
            let _=pocket_ui::android_cores::retire_bundled(&data, |_|false,false);
            if valid { got.unwrap(); assert_eq!(std::fs::read(data.join("android-cores/reader/libreader_libretro.so")).unwrap(),b"reader rule library"); }
        }
        "j2me-jar" | "j2me-jad" => { let got = j2me_size::detect(&d.join("j2me/Rule.jar")); if valid { assert_eq!(got.0,(240,320)); } }
        "j2me-options" => { let got = j2me_size::set_resolution(&d.join("j2me"), &d.join("j2me/Rule.jar"), (320,240)); assert!(got.is_ok()); if valid { assert!(std::fs::read_to_string(file).unwrap().contains("reader_preserved")); } }
        "j2me-remap" => { let got = j2me_keys::apply(&d.join("j2me"), &d.join("j2me/Rule.jar"), true); assert!(got.is_ok()); if valid { assert!(std::fs::read_to_string(file).unwrap().contains("reader_preserved")); } }
        "j2me-java" => {
            std::env::set_var("POCKETUI_MOCK","1"); std::env::set_var("POCKETUI_J2ME_ARCHIVE",file); std::env::set_var("POCKETUI_J2ME_TOP","rule-jdk");
            let places = j2me_java::Places { storage:d.join("java/storage"), script:d.join("java/freej2me.sh"), card:vec![] };
            let got = j2me_java::load(&places, &Default::default()); if valid { got.unwrap(); assert!(places.ready()); }
        }
        "android-apps" => {
            let text=decoded(bytes); let got=android_games::marked(&text); let prefs=pocket_ui::play_route::prefs_from_json(&text);
            if valid { assert_eq!(got.get("xyz.reader.rule").map(String::as_str),Some(GAME)); assert_eq!(prefs.plays_with.get("gb").map(String::as_str),Some("xyz.reader.rule")); assert!(pocket_ui::play_route::gambatte_from_json(&text)); }
            let _=android_games::shelf(&got,None);
        }
        "android-mobile-history" => { let mut recent = recents::Recents::load(&data); let got = android_handheld::take_mobile_history(&mut recent, &data, r#"{"played":{}}"#, "<systemList/>"); assert_eq!(got,0); if valid { assert!(pocket_ui::fsutil::read_json_or_backup::<std::collections::BTreeMap<String,(u64,u64)>>(file).unwrap().len()>0); } }
        "android-sidecar" => { let got = android_handheld::sidecar_cheats(&d.join("gb/Reader Rule Game.gb")); if valid { assert_eq!(got.as_bytes(),CHEAT); } }
        "update-card" => { let got = update::card::look_in(&[d.join("card")], "0.0.1", true, &signing_key().verifying_key().to_bytes(), update::card::Target::Rocknix); assert!(!got.message().trim().is_empty()); if valid { assert!(matches!(got,update::card::Look::Offer(_))); } }
        "update-payload" => {
            let rel: update::Release = serde_json::from_value(serde_json::json!({"version":"1","url":"https://example.invalid/payload","sha256":store::sha256_hex(PAYLOAD),"size":PAYLOAD.len(),"notes":"Reader rule"})).unwrap();
            let cancel = update::Cancel::default(); cancel.cancel(); let got = update::payload::download(&rel,file,&cancel,&|_|{}); if valid { assert!(got.is_ok()); }
        }
        "seed-archive" => { let got = std::fs::read(file).map_err(|e|e.to_string()).and_then(|b| untar::unpack(&b,&d.join("bios-unpack"))); if valid { got.unwrap(); assert_eq!(std::fs::read(d.join("bios-unpack/bios-data/rule.bin")).unwrap(),b"reader rule bios data"); } }
        "runtime-archive" => { let got = archive::unpack_runtime(file,&d.join("runtime-unpack")); if valid { got.unwrap(); assert_eq!(std::fs::read(d.join("runtime-unpack/rule/runtime")).unwrap(),b"reader rule runtime"); } }
        _ => panic!("unknown extra source {source}"),
    }
}

fn emu_kind(source:&str)->(pocket_ui::standalone::Emu,&'static str) {
    use pocket_ui::standalone::Emu;
    if source.contains("azahar") {(Emu::Azahar,"3ds")} else if source.contains("pcsx2") {(Emu::Armsx2,"ps2")} else if source.contains("wii") {(Emu::Dolphin,"wii")} else {(Emu::Dolphin,"gamecube")}
}
fn emu_seed(source:&str)->String {
    use pocket_ui::emu_settings::{self as es,BaseFile,Store};
    let (emu,system)=emu_kind(source); let t=es::table(emu,system).unwrap();
    let base=match source {"emu-dolphin-main"=>BaseFile::DolphinMain,"emu-azahar"=>BaseFile::AzaharQt,"emu-pcsx2"|"emu-pcsx2-game"=>BaseFile::Pcsx2,"emu-system"=>BaseFile::SystemCfg,_=>BaseFile::DolphinGfx};
    let mut out=String::new();
    for table in std::iter::once(t).chain(t.more) {
        for row in table.settings {
            let choice=&row.choices[1.min(row.choices.len()-1)];
            if source=="emu-system" { if let Store::Es(key)=row.store { out+=&format!("{}.{key}={}\n",table.platform,choice.stored); } }
            else if source=="emu-dolphin-game" { if let Store::GameIni(section,key)=row.store { out+=&format!("[{section}]\n{key} = {}\n",choice.emu); } }
            else if row.ini.0==base { out+=&format!("[{}]\n{} = {}\n",row.ini.1,row.ini.2,choice.emu); }
        }
    }
    if out.is_empty() { out="[ReaderRule]\nPresent = yes\n".into(); }
    out
}
fn emu_probe(source:&str,d:&Path,bytes:Option<&[u8]>,valid:bool) {
    use pocket_ui::emu_settings::{self as es,Files};
    let (emu,system)=emu_kind(source); let rom=d.join("Rule.dol");
    let mut all_rows=Vec::new();
    let mut menu=es::Menu::for_game(emu,system,&rom,Some("SLUS-21050 (A1B2C3D4)"),&Files {root:d.to_path_buf(),system_cfg:d.join("system.cfg")}).expect("emulator menu");
    for page in 0..2 {
        let rows=menu.rows(); assert!(!rows.is_empty(),"{source}: no words on menu page{page}");
        all_rows.extend(rows.clone());
        for (name,value,_) in rows { assert!(!name.trim().is_empty(),"{source}: unnamed row"); assert!(!value.trim().is_empty(),"{source}: {name} leaves an empty value"); }
        menu.open_more();
    }
    let pixel=pocket_ui::pixel_standalone::read(emu,system,&rom,Some("SLUS-21050 (A1B2C3D4)"),&Files {root:d.to_path_buf(),system_cfg:d.join("system.cfg")});
    if valid {assert!(!pixel.unwrap().sources.is_empty(),"{source}: valid input reaches Pixel Path");}
    else if let Err(why)=pixel {assert!(!why.trim().is_empty(),"{source}: Pixel Path refuses with visible words");}
    if valid {
        let text=std::str::from_utf8(bytes.unwrap()).unwrap(); assert!(!text.is_empty());
        if source.contains("custom") || source.contains("stock") {
            assert_eq!(es::ini_get(text,"GCPad1","Buttons/A").as_deref(),Some("`Button S`"));
        } else {
            use es::{BaseFile,Store};
            let base=match source {"emu-dolphin-main"=>BaseFile::DolphinMain,"emu-azahar"=>BaseFile::AzaharQt,"emu-pcsx2"|"emu-pcsx2-game"=>BaseFile::Pcsx2,"emu-system"=>BaseFile::SystemCfg,_=>BaseFile::DolphinGfx};
            let table=es::table(emu,system).unwrap();
            let witness=std::iter::once(table).chain(table.more).flat_map(|t|t.settings).find(|row| {
                if source=="emu-system" {matches!(row.store,Store::Es(_))}
                else if source=="emu-dolphin-game" {matches!(row.store,Store::GameIni(_,_))}
                else {row.ini.0==base}
            });
            if let Some(row)=witness {
                let expected=row.choices[1.min(row.choices.len()-1)].label;
                assert!(all_rows.iter().any(|(n,v,_)|n==row.name && v==expected),"{source}: valid seed did not reach {}={expected}: {all_rows:?}",row.name);
            } else { assert!(text.contains("Present = yes"),"{source}: missing seed witness"); }
        }
    }
}

fn core_archive(manifest:Option<&[u8]>)->Vec<u8> {
    let mut files=vec![("libreader_libretro.so",b"reader rule library".as_slice())];
    if let Some(manifest)=manifest {files.push(("core.json",manifest));}
    gzip(&tar(&files))
}

fn brick_asset_archive()->Vec<u8> { gzip(&tar(&[("bios-data/rule.bin",b"reader rule component")])) }
fn brick_asset_manifest(archive:&[u8])->Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"format":1,"assets":[{"path":"bios-data/rule.bin","size":b"reader rule component".len(),"sha256":store::sha256_hex(b"reader rule component"),"package":{"version":"9.9.9","url":"https://example.invalid/asset.tar.gz","sha256":store::sha256_hex(archive),"size":archive.len(),"notes":"Reader rule component"}}]})).unwrap()
}
fn brick_app(assets:Option<&[u8]>)->Vec<u8> {
    let delta=vec![7;65536];
    let mut files=vec![("start.sh",b"#!/bin/sh\nexit 0\n".as_slice()),("RELEASE",b"9.9.9\n".as_slice()),("bin/pocket-ui",b"#!/bin/sh\necho pocket-ui 9.9.9\n".as_slice()),("bin/pocket-core",b"#!/bin/sh\necho pocket-core 9.9.9\n".as_slice()),("bin/pocket-fb",b"reader rule bridge".as_slice()),("bin/brick-hw",b"reader rule controls".as_slice()),("msg/starting.png",b"reader rule boot image".as_slice()),("assets/rule.dat",delta.as_slice())];
    if let Some(assets)=assets { files.push(("asset-packages.json",assets)); }
    gzip(&tar(&files))
}
#[cfg(feature="brick")]
fn brick_install_probe(source:&str,d:&Path,bytes:Option<&[u8]>,valid:bool) {
    use base64::Engine; use ed25519_dalek::Signer;
    let card=d.join("brick-card"); std::fs::create_dir_all(&card).unwrap();
    let asset=if source=="brick-asset-archive" {bytes.unwrap_or_default().to_vec()} else {brick_asset_archive()};
    let asset_manifest=if source=="brick-assets-manifest" {bytes.map(|b|b.to_vec())} else if source=="brick-asset-archive" {Some(brick_asset_manifest(&asset))} else {None};
    let app=if source=="brick-app-archive" {bytes.unwrap_or_default().to_vec()} else {brick_app(asset_manifest.as_deref())};
    let manifest=format!(r#"{{"format":1,"app":{{"version":"9.9.9","url":"https://example.invalid/pocket-ui-brick-9.9.9.tar.gz","sha256":"{}","size":{},"notes":"Reader rule Brick update"}},"os":null}}"#,store::sha256_hex(&app),app.len());
    let signature=base64::engine::general_purpose::STANDARD.encode(signing_key().sign(manifest.as_bytes()).to_bytes());
    let bundle=d.join("card/pocket-ui-update-brick-rule.zip"); std::fs::create_dir_all(bundle.parent().unwrap()).unwrap();
    std::fs::write(&bundle,zip(&[("update.json",manifest.as_bytes()),("update.json.sig",signature.as_bytes()),("app.tar.gz",&app),("asset.tar.gz",&asset)])).unwrap();
    if source=="brick-delta-file" {
        let file=card.join("app/assets/rule.dat"); if file.exists() {std::fs::set_permissions(file,std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();}
    }
    std::env::set_var("POCKETUI_BRICK_CARD",&card);
    let look=update::card::look_in(&[d.join("card")],"0.0.1",true,&signing_key().verifying_key().to_bytes(),update::card::Target::Brick);
    let result=match look {update::card::Look::Offer(found)=>update::card::install(&found,&Default::default(),&|_|{}),other=>Err(other.message())};
    // These installers intentionally keep failed downloads for a device retry. This synthetic card has no next boot.
    let key=store::sha256_hex(card.as_os_str().as_encoded_bytes()); let ram=Path::new("/tmp").join(format!("pocket-ui-update-{}",&key[..16]));
    if ram.exists() { let _entries=std::fs::read_dir(&ram).unwrap().collect::<Result<Vec<_>,_>>().unwrap(); std::fs::remove_dir_all(&ram).unwrap(); }
    if valid {
        result.unwrap(); assert!(card.join("app/start.sh").is_file());
        if source.starts_with("brick-asset") {assert_eq!(std::fs::read(card.join("app/bios-data/rule.bin")).unwrap(),b"reader rule component");}
        if source=="brick-delta-file" {assert_eq!(std::fs::read(card.join("app/assets/rule.dat")).unwrap(),vec![7;65536]); assert!(!card.join("app.previous/assets/rule.dat").exists(),"unchanged file was moved, not unpacked twice");}
    } else if let Err(why)=result {assert!(!why.trim().is_empty());}
}

fn memory_core(d:&Path)->PathBuf {
    if let Some(path)=std::env::var_os("POCKET_READER_MEMORY_CORE").map(PathBuf::from).filter(|p|p.is_file()) {return path;}
    let source=d.join("memory-core.c");let library=d.join("memory_core_libretro.so");
    let mut code=String::from("#define FPS 60.0\n#define RATE 48000.0\n#define retro_get_memory_data common_memory_data\n#define retro_get_memory_size common_memory_size\n");
    code+=include_str!("../../scripts/crash-sweep/stub-core.c");
    code+="\n#undef retro_get_memory_data\n#undef retro_get_memory_size\nstatic uint8_t clock_memory[16];\nvoid *retro_get_memory_data(unsigned id) {return id==1?clock_memory:common_memory_data(id); }\nsize_t retro_get_memory_size(unsigned id) {return id==1?sizeof clock_memory:common_memory_size(id); }\n";
    std::fs::write(&source,code).unwrap();let out=std::process::Command::new("cc").args(["-shared","-fPIC","-O1","-o"]).arg(&library).arg(&source).arg("-lm").output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    std::env::set_var("POCKET_READER_MEMORY_CORE",&library);library
}
