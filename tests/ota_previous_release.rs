//! Upgrade the previous public layout, then start the selected installed program twice.
//! Real installer/download/hash/unpack/link path; version-reporting shims delegate painting to
//! this tree's pocket-ui. Layouts come from checksummed public 0.2.5/0.3.0 card artifacts.
//! ARM payloads are represented by marker files; this does not execute the archived ARM code.
#![cfg(target_os = "linux")]
#[path = "common/executable.rs"]
mod fixture_executable;

use std::{fs, os::unix::fs::{symlink, PermissionsExt}, path::Path, process::{Child, Command}, time::Duration};
use pocket_ui::update::{self, Cancel, Release};
include!("common/root.rs");

struct Stub(Child);
impl Drop for Stub {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}
fn program(path: &Path, version: &str, ui: bool) {
    let real = env!("CARGO_BIN_EXE_pocket-ui").replace('\'', "'\\''");
    let name = if ui { "pocket-ui" } else { "pocket-core" };
    let rest = if ui { format!("exec '{real}' \"$@\"\n") } else { "exit 0\n".into() };
    fixture_executable::publish(path, format!("#!/bin/sh\n[ \"$1\" = --version ] && echo '{name} {version}' && exit 0\n{rest}")).unwrap();
}

#[test]
fn ota_from_public_layouts_keeps_settings_progress_and_games_across_two_starts() {
    let layouts: serde_json::Value = serde_json::from_str(include_str!("fixtures/updates/public-layouts.json")).unwrap();
    for layout in layouts.as_array().unwrap() {
        let previous = layout["version"].as_str().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("pocket-ui");
        let old = root.join("releases").join(previous);
        let data = root.join("data");
        let games = tmp.path().join("roms/gb");
        for dir in [&old, &data, &games, &root.join("states/gb"), &tmp.path().join("saves/gb")] {
            fs::create_dir_all(dir).unwrap();
        }
        // Replay the actual paths, modes and links from the published seed, retaining no large binaries.
        for entry in layout["entries"].as_array().unwrap() {
            let path = root.join(entry["path"].as_str().unwrap());
            match entry["kind"].as_str().unwrap() {
                "directory" => fs::create_dir_all(path).unwrap(),
                "link" => symlink(entry["target"].as_str().unwrap(), path).unwrap(),
                "file" => {
                    fs::write(&path, b"previous release asset").unwrap();
                    fs::set_permissions(path, fs::Permissions::from_mode(entry["mode"].as_u64().unwrap() as u32)).unwrap();
                }
                _ => panic!("unknown public artifact entry"),
            }
        }
        // A card whose old release has already shown a frame has this additional health link.
        symlink(format!("releases/{previous}"), root.join("known-good")).unwrap();
        for (name, ui) in [("pocket-ui", true), ("pocket-core", false)] { program(&old.join(name), previous, ui); }
        let protected = [
            (old.join("cores/gpsp_libretro.so"), b"previous release asset".to_vec()),
            (data.join("settings.json"), format!(r#"{{"onboarded":true,"seen_version":"{previous}","language":"de","theme":"minimal","appearance":"light","text_size":"large"}}"#).into_bytes()),
            (root.join("states/gb/Clock Demo.pstate.auto"), b"continue from last night".to_vec()),
            (root.join("states/gb/Clock Demo.pstate3"), b"slot four".to_vec()),
            (tmp.path().join("saves/gb/Clock Demo.srm"), b"battery progress".to_vec()),
            (tmp.path().join("saves/gb/Clock Demo.rtc"), b"cartridge clock".to_vec()),
            (games.join("Clock Demo.gb"), vec![0; 32768]),
        ];
        for (path, bytes) in &protected { fs::write(path, bytes).unwrap(); }
        let cfg = tmp.path().join("es_systems.cfg");
        fs::write(&cfg, format!("<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension><command>true</command></system></systemList>", games.display())).unwrap();
        let src = tmp.path().join("offered"); fs::create_dir(&src).unwrap();
        for (name, ui) in [("pocket-ui", true), ("pocket-core", false)] { program(&src.join(name), "0.3.1", ui); }
        let tar = tmp.path().join("app.tar.gz");
        assert!(Command::new("tar").arg("-czf").arg(&tar).arg("-C").arg(&src).args(["pocket-ui", "pocket-core"]).status().unwrap().success());
        let bytes = fs::read(&tar).unwrap();
        let rel = Release { version: "0.3.1".into(), url: "app.tar.gz".into(), preferred_sources: Vec::new(), mirrors: vec![],
            sha256: pocket_ui::store::sha256_hex(&bytes), size: bytes.len() as u64, notes: String::new() };
        update::install_app(&rel, &format!("file://{}/update.json", tmp.path().display()), &root, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(fs::read_link(root.join("previous")).unwrap(), Path::new(&format!("releases/{previous}")));
        assert_eq!(fs::read_link(root.join("current")).unwrap(), Path::new("releases/0.3.1"));
        let socket = tmp.path().join("wl");
        let _stub = Stub(Command::new("python3").arg("-I").arg(format!("{ROOT}/scripts/wayland-stub.py")).arg(&socket).spawn().unwrap());
        test_clock::wait_for(|| socket.exists(), Duration::from_secs(2), "Wayland ready");
        for n in 0..2 {
            let record = tmp.path().join("painted.tsv");
            let out = Command::new(root.join("pocket-ui"))
                .args(["--snapshot", "shelf:gb:minimal"]).arg(tmp.path().join(format!("restart-{n}.png")))
                .env("WAYLAND_DISPLAY", &socket).env("POCKETUI_MOCK", "1").env("POCKETUI_LANG", "de")
                .env("POCKETUI_ES_SYSTEMS", &cfg).env("POCKETUI_DATA", &data).env("POCKETUI_STATES", root.join("states"))
                .env("POCKETUI_I18N_MISSES", &record).output().unwrap();
            assert!(out.status.success(), "installed app restart {n}: {}", String::from_utf8_lossy(&out.stderr));
            assert!(fs::read_to_string(&record).unwrap().contains("Game Boy"), "the game library is still visible");
            for (path, bytes) in &protected { assert_eq!(&fs::read(path).unwrap(), bytes, "restart {n}: {} changed", path.display()); }
            assert_eq!(fs::read_link(root.join("current")).unwrap(), Path::new("releases/0.3.1"));
        }
    }
}
