//! Japanese game names (`src/text_fallback.rs`): a made-up library of Super Famicom and Famicom games with Japanese
//! names, rendered with `pocket-ui --snapshot` on the shelf (a system's list in Split and in Cards), on the game card
//! and in the search, dark and light, Normal and Large text, with ROCKNIX's Noto Sans CJK (`POCKETUI_CJK_FONT`) and
//! without it. With the font the names are painted from it and run into nothing the plain names don't; without it
//! every screen still renders, as before ("?" for each letter).
//!
//! Needs the font: `POCKETUI_TEST_CJK_FONT`, else the one of an unpacked ROCKNIX image at
//! /tmp/rocknix-root-20261001; without it the test says so and passes. On Linux without a compositor it starts
//! scripts/wayland-stub.py (python3), as tests/i18n_snapshots.rs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

struct Stub {
    path: PathBuf,
    child: std::process::Child,
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    for _ in 0..200 {
        if stub.path.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

fn font() -> Option<PathBuf> {
    let path = std::env::var_os("POCKETUI_TEST_CJK_FONT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp/rocknix-root-20261001/usr/share/fonts/truetype/noto-cjk/NotoSansCJKsc-Regular.otf"));
    path.is_file().then_some(path)
}

/// The library: es_systems.cfg and empty ROM files with the names as No-Intro has them.
fn library(dir: &Path) {
    let systems = [
        (
            "snes",
            "Super Nintendo",
            "sfc",
            &["ファイナルファンタジーVI (Japan)", "スーパーマリオワールド (Japan)", "ゼルダの伝説 神々のトライフォース (Japan)", "Super Mario World (USA)", "ロックマンX (Japan)", "聖剣伝説2 (Japan)", "クロノ・トリガー (Japan)"][..],
        ),
        ("nes", "Nintendo Entertainment System", "nes", &["スーパーマリオブラザーズ3 (Japan)", "ドラゴンクエストIII そして伝説へ… (Japan)", "Tetris (USA)"][..]),
    ];
    let mut cfg = String::from("<?xml version=\"1.0\"?>\n<systemList>\n");
    for (id, full, ext, games) in systems {
        let folder = dir.join("roms").join(id);
        std::fs::create_dir_all(&folder).unwrap();
        for g in games {
            std::fs::write(folder.join(format!("{g}.{ext}")), b"").unwrap();
        }
        cfg += &format!("  <system><name>{id}</name><fullname>{full}</fullname><path>{}</path><extension>.{ext}</extension><command>true</command></system>\n", folder.display());
    }
    cfg += "</systemList>\n";
    std::fs::write(dir.join("es_systems.cfg"), cfg).unwrap();
}

/// A screen's pixels and the text painter's record of text running into text or off the edge (`POCKETUI_TEXT_BOXES`).
fn render(tmp: &Path, stub: Option<&Stub>, state: &str, light: bool, size: &str, font: Option<&Path>) -> (Vec<u8>, BTreeSet<String>) {
    let name = format!("{}-{}-{size}-{}", state.replace(':', "-"), if light { "light" } else { "dark" }, if font.is_some() { "font" } else { "none" });
    let data = tmp.join(format!("data-{name}"));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings.json"), if light { r#"{"onboarded": true, "appearance": "light"}"# } else { r#"{"onboarded": true}"# }).unwrap();
    let (png, boxes) = (tmp.join(format!("{name}.png")), tmp.join(format!("{name}.box")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", size)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_CJK_FONT", font.map_or("off".into(), |f| f.as_os_str().to_owned()))
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{name} didn't render: {}", String::from_utf8_lossy(&out.stderr));
    let record = std::fs::read_to_string(&boxes).unwrap_or_default();
    (std::fs::read(&png).unwrap(), record.lines().filter(|l| !l.is_empty()).map(str::to_string).collect())
}

#[test]
fn japanese_names_are_painted_from_the_system_font_and_run_into_nothing() {
    let Some(font) = font() else {
        eprintln!("SKIPPED: no Noto Sans CJK (set POCKETUI_TEST_CJK_FONT to ROCKNIX's NotoSansCJKsc-Regular.otf)");
        return;
    };
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    library(&tmp);
    let stub = wayland(&tmp);
    let mut problems = Vec::new();
    // Split's list, Cards with the jump's big letter, the card of a Japanese name, the search finding two.
    for state in ["letters:snes:0", "letters:snes:3:cards", "curator:card:snes:ファイナル", "search:results:マリオ"] {
        for light in [false, true] {
            for size in ["normal", "large"] {
                let (with, with_rec) = render(&tmp, stub.as_ref(), state, light, size, Some(&font));
                let (without, without_rec) = render(&tmp, stub.as_ref(), state, light, size, None);
                let what = format!("{state} ({}, {size})", if light { "light" } else { "dark" });
                if with == without {
                    problems.push(format!("{what}: the same picture with the font as without it"));
                }
                // No more text running into text, touching or cut by the edge than with the names as "?" (the strings
                // differ: a name is cut short at another letter). The jump's big letter covers the selected card's
                // name on purpose, with either.
                for kind in ["O\t", "T\t", "E\t"] {
                    let count = |rec: &BTreeSet<String>| rec.iter().filter(|l| l.starts_with(kind)).count();
                    if count(&with_rec) > count(&without_rec) {
                        problems.extend(with_rec.iter().filter(|l| l.starts_with(kind)).map(|l| format!("{what}: {}", l.replace('\t', " | "))));
                        problems.push(format!("{what}: without the font only {:?}", without_rec.iter().filter(|l| l.starts_with(kind)).collect::<Vec<_>>()));
                    }
                }
            }
        }
    }
    drop(stub);
    assert!(problems.is_empty(), "{} problems (O: runs into another string, T: touches the one beside it, E: cut by the edge):\n{}", problems.len(), problems.join("\n"));
}
