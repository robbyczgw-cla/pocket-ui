//! A new player's first day on a messy card (fix/real-user-run, 2026-10-26): what a screen shows where its data is
//! missing. Rendered with `pocket-ui --snapshot` on a small library made here: a Game Boy game, a Sega CD game and a Watara Supervision
//! game; the last has neither a fact sheet (`consoles::for_system`) nor a picture (`system_art`).
//!
//! - Split with a system without a fact sheet selected (Sega CD had none then): the room under the name held nothing at all (no headline, no facts), as if
//!   the page had not finished drawing. It shows Your games now (`consoles::here_rows`).
//!
//! Needs python3 for scripts/wayland-stub.py on Linux without a compositor (as tests/card_rows.rs).

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
    let child = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()])
        .spawn()
        .expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    for _ in 0..200 {
        if stub.path.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

/// Three systems, one game each: Game Boy and Sega CD (with a fact sheet and a picture) and Watara Supervision (with neither; the SuperGrafx had neither until it got a fact sheet and a picture).
fn library(dir: &Path) {
    let mut cfg = String::from("<?xml version=\"1.0\"?>\n<systemList>\n");
    for (id, name, exts, files) in [
        ("gb", "Game Boy", ".gb .zip", &["Tobu Tobu Girl (World).gb"][..]),
        (
            "segacd",
            "Sega CD",
            ".chd .cue .iso .m3u",
            &["Pocket CD Demo (USA).cue", "Pocket CD Demo (USA).bin"][..],
        ),
        ("supervision", "Supervision", ".sv .bin .zip", &["Pocket Demo (World).sv"][..]),
    ] {
        let folder = dir.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        for f in files {
            let body = if f.ends_with(".cue") {
                "FILE \"Pocket CD Demo (USA).bin\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n"
            } else {
                ""
            };
            std::fs::write(folder.join(f), body).unwrap();
        }
        cfg += &format!(
            "  <system><name>{id}</name><fullname>{name}</fullname><path>{}</path><extension>{exts}</extension><command>true</command></system>\n",
            folder.display()
        );
    }
    cfg += "</systemList>\n";
    std::fs::write(dir.join("es_systems.cfg"), cfg).unwrap();
}

/// The picture as 0RGB pixels and its width.
fn render(tmp: &Path, stub: Option<&Stub>, state: &str, light: bool) -> (Vec<u32>, usize) {
    let name = format!("{}-{}", state.replace(':', "-"), if light { "light" } else { "dark" });
    let data = tmp.join(format!("data-{name}"));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("settings.json"),
        if light {
            r#"{"onboarded": true, "appearance": "light"}"#
        } else {
            r#"{"onboarded": true}"#
        },
    )
    .unwrap();
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", "normal")
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .env_remove("POCKETUI_SCREENSHOTS")
        .output()
        .expect("run pocket-ui");
    assert!(
        out.status.success() && png.exists(),
        "{name} didn't render: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&png).unwrap()))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let ch = info.color_type.samples();
    let px = buf[..info.buffer_size()]
        .chunks_exact(ch)
        .map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32)
        .collect();
    (px, info.width as usize)
}

/// Pixels of the rectangle that differ from its most common colour (the page behind it).
fn ink(px: &[u32], w: usize, (x0, y0, x1, y1): (usize, usize, usize, usize)) -> usize {
    let mut counts = std::collections::HashMap::new();
    for y in y0..y1 {
        for x in x0..x1 {
            *counts.entry(px[y * w + x]).or_insert(0usize) += 1;
        }
    }
    let page = counts.iter().max_by_key(|(_, n)| **n).map(|(c, _)| *c).unwrap_or(0);
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|(x, y)| px[y * w + x] != page)
        .count()
}

#[test]
fn split_shows_a_system_without_a_fact_sheet_with_its_own_rows() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    library(&tmp);
    let stub = wayland(&tmp);
    for light in [false, true] {
        // The room under the selected system's name in Split (the panel's right column, from below the name to
        // above the hint bar): the fact sheet's rows for Game Boy and Sega CD, Your games alone for the Supervision.
        let room = (600, 640, 1230, 900);
        for system in ["gb", "segacd", "supervision"] {
            let (px, w) = render(&tmp, stub.as_ref(), &format!("shelf:{system}:split"), light);
            let n = ink(&px, w, room);
            assert!(
                n > 300,
                "shelf:{system}:split ({}): the room under the name is empty ({n} pixels of text)",
                if light { "light" } else { "dark" }
            );
        }
    }
}
