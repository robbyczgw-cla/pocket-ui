//! Light by time of day and decorations that grow with play (docs/DESIGN.md "The room: light and decorations"), as
//! the player meets them on the made-up library (scripts/curator-fake-library.py), in the Shelf look and the Cabinet,
//! dark and light, at 640x480 and 1280x720:
//!
//! - the clock (`POCKETUI_ROOM_TIME`) changes the wood: night darker than afternoon, evening warmer, the lamp's glow;
//!   the showcase's cover and the title stay exactly as they are; with Settings → Look → Cozy extras → Light
//!   by time of day off the night looks like the afternoon;
//! - a new player (twenty minutes played) has a sprout and nothing else; a long history (66 hours, a completed game,
//!   a "100 %" sticker, 12 hours on the SNES) a big plant, the trophy, the framed "100 %" and the SNES's figurine;
//!   with Decorations off the shelf is as it always was. Nothing earned stands over a case.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/stickers_tv.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const METROID: &str = "Super Metroid (Japan, USA) (En,Ja).sfc";
/// A PS2 game: its "100 %" sticker is not on the SNES row the snapshots show.
const BURNOUT: &str = "Burnout 3 - Takedown (USA).iso";

struct Stub(std::process::Child, PathBuf);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let stub = Stub(child, path);
    for _ in 0..200 {
        if stub.1.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib");
    let ok = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    lib
}

/// A copy of the made-up library's data with a history: "new" (twenty minutes on one game, nothing completed) or
/// "long" (its own 66 hours with a completed game, 12 hours on Super Metroid, a "100 %" sticker on Burnout 3).
fn history(lib: &Path, tmp: &Path, which: &str) -> PathBuf {
    let data = tmp.join(format!("data-{which}"));
    let _ = std::fs::remove_dir_all(&data);
    assert!(Command::new("cp").args(["-r", lib.join("data").to_str().unwrap(), data.to_str().unwrap()]).status().unwrap().success());
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(data.join("library.json")).unwrap()).unwrap();
    let games = json["games"].as_object_mut().unwrap();
    if which == "new" {
        for g in games.values_mut() {
            g["secs"] = serde_json::json!(0);
            if g["status"] == serde_json::json!("completed") {
                g["status"] = serde_json::json!("playing");
            }
        }
        games.values_mut().next().unwrap()["secs"] = serde_json::json!(1200);
    } else {
        let key = games.keys().find(|k| k.ends_with(METROID)).expect("Super Metroid in the made-up library").clone();
        games[&key]["secs"] = serde_json::json!(12 * 3600);
        let key = games.keys().find(|k| k.ends_with(BURNOUT)).expect("Burnout 3 in the made-up library").clone();
        games[&key]["case"] = serde_json::json!({ "100": true });
    }
    std::fs::write(data.join("library.json"), json.to_string()).unwrap();
    data
}

struct Shot {
    w: i32,
    h: i32,
    px: Vec<u8>,
}

impl Shot {
    fn at(&self, x: i32, y: i32) -> (u8, u8, u8) {
        let i = ((y * self.w + x) * 3) as usize;
        (self.px[i], self.px[i + 1], self.px[i + 2])
    }

    /// Mean (r, g, b) over the rectangle.
    fn mean(&self, (x0, y0, x1, y1): (i32, i32, i32, i32)) -> (f32, f32, f32) {
        let (mut r, mut g, mut b, mut n) = (0f32, 0f32, 0f32, 0f32);
        for y in y0.max(0)..y1.min(self.h) {
            for x in x0.max(0)..x1.min(self.w) {
                let p = self.at(x, y);
                (r, g, b, n) = (r + p.0 as f32, g + p.1 as f32, b + p.2 as f32, n + 1.0);
            }
        }
        (r / n, g / n, b / n)
    }

    fn count(&self, (x0, y0, x1, y1): (i32, i32, i32, i32), pred: impl Fn((u8, u8, u8)) -> bool) -> usize {
        let mut n = 0;
        for y in y0.max(0)..y1.min(self.h) {
            for x in x0.max(0)..x1.min(self.w) {
                n += usize::from(pred(self.at(x, y)));
            }
        }
        n
    }

    fn same(&self, other: &Shot, (x0, y0, x1, y1): (i32, i32, i32, i32)) -> bool {
        (y0..y1).all(|y| (x0..x1).all(|x| self.at(x, y) == other.at(x, y)))
    }

    /// The same picture under the status corner's clock (the minute may turn between two snapshots).
    fn same_below_clock(&self, other: &Shot) -> bool {
        self.same(other, (0, self.h / 8, self.w, self.h))
    }

    /// The cabinet's top board under the showcase: rows a little under its top edge, between the case shown and the TV.
    fn board(&self) -> (i32, i32, i32, i32) {
        let (x, page) = (self.w / 2, self.at(self.w / 2, self.h / 4));
        let top = (self.h / 4..self.h).find(|&y| self.at(x, y) != page).expect("the cabinet's top board");
        (self.w * 35 / 100, top + 4, self.w * 65 / 100, top + 12)
    }
}

#[allow(clippy::too_many_arguments)]
fn shot(tmp: &Path, lib: &Path, data: &Path, stub: &Option<Stub>, look: &str, appearance: &str, size: &str, time: &str, extras: serde_json::Value) -> Shot {
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!(look);
    json["appearance"] = serde_json::json!(appearance);
    json["onboarded"] = serde_json::json!(true);
    json["extras"] = extras;
    std::fs::write(&settings, json.to_string()).unwrap();
    let png = tmp.join(format!("{look}-{appearance}-{size}-{}.png", time.replace(':', "")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let state = format!("shelf:snes:{look}:games:0");
    let out = cmd
        .args(["--snapshot", &state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", data)
        .env("POCKETUI_ROOM_TIME", time)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    Shot { w: info.width as i32, h: info.height as i32, px: buf }
}

fn on() -> serde_json::Value {
    serde_json::json!({})
}

#[test]
fn the_clock_lights_the_wood_and_leaves_covers_and_text_alone() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let data = history(&lib, &tmp, "long");
    for (size, appearance) in [("640x480", "dark"), ("1280x720", "dark"), ("1280x720", "light")] {
        let noon = shot(&tmp, &lib, &data, &stub, "cabinet", appearance, size, "15:00", on());
        let evening = shot(&tmp, &lib, &data, &stub, "cabinet", appearance, size, "20:00", on());
        let night = shot(&tmp, &lib, &data, &stub, "cabinet", appearance, size, "02:00", on());
        let (w, h) = (noon.w, noon.h);
        // The cabinet's top board (wood only; the lamp's glow stays inside its compartment).
        let board = noon.board();
        let (n, e, d) = (noon.mean(board), evening.mean(board), night.mean(board));
        let luma = |c: (f32, f32, f32)| c.0 * 0.3 + c.1 * 0.6 + c.2 * 0.1;
        assert!(luma(d) < luma(n) - 3.0, "{size} {appearance}: the night is darker on the wood ({d:?} vs {n:?})");
        assert!(e.0 - e.2 > n.0 - n.2 + 2.0, "{size} {appearance}: the evening is warmer ({e:?} vs {n:?})");
        assert!(luma(d) > luma(n) * 0.8, "{size} {appearance}: subtle, never dark ({d:?} vs {n:?})");
        // The title and the showcase's case (top left) stay exactly as they are.
        assert!(night.same(&noon, (0, 0, w / 4, h * 4 / 10)), "{size} {appearance}: the title or the cover changed");
        // Switched off, the night is the afternoon.
        let off = serde_json::json!({ "time_of_day_light": false });
        let night_off = shot(&tmp, &lib, &data, &stub, "cabinet", appearance, size, "02:00", off.clone());
        let noon_off = shot(&tmp, &lib, &data, &stub, "cabinet", appearance, size, "15:00", off);
        assert!(night_off.same_below_clock(&noon_off), "{size} {appearance}: light by time of day off");
    }
}

#[test]
fn decorations_grow_with_play_and_never_stand_over_a_case() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let (new, long) = (history(&lib, &tmp, "new"), history(&lib, &tmp, "long"));
    let gold = |(r, g, b): (u8, u8, u8)| r > 190 && g > 140 && b < 90 && r as i32 > b as i32 + 110;
    let green = |(r, g, b): (u8, u8, u8)| g > 70 && g as i32 > r as i32 + 15 && g as i32 > b as i32 + 15;
    for (size, appearance) in [("640x480", "dark"), ("1280x720", "dark"), ("1280x720", "light")] {
        let a = shot(&tmp, &lib, &new, &stub, "shelf", appearance, size, "15:00", on());
        let b = shot(&tmp, &lib, &long, &stub, "shelf", appearance, size, "15:00", on());
        let (w, h) = (a.w, a.h);
        // The shelf's right part, past the middle of its row (the SNES cases are grey, none has a sticker here).
        let shelf = (w * 6 / 10, h / 2, w, h - h / 8);
        let (gold_a, gold_b) = (a.count(shelf, gold), b.count(shelf, gold));
        assert!(gold_a < 20 && gold_b > 60, "{size} {appearance}: the trophy only with a completed game ({gold_a} / {gold_b})");
        // A sprout for the new player, the big plant for the long history (at 640x480 the row leaves room for a
        // cactus only, as before).
        let (green_a, green_b) = (a.count(shelf, green), b.count(shelf, green));
        if w >= 1280 {
            assert!(green_a > 10, "{size} {appearance}: the sprout ({green_a})");
            assert!(green_b > green_a * 4 + 100, "{size} {appearance}: a sprout grows into a big plant ({green_a} / {green_b})");
        }
        // The cases themselves are the same in both.
        assert!(a.same(&b, (0, h / 2, w * 6 / 10, h - h / 6)), "{size} {appearance}: something earned stands over the cases");
        // Decorations off: the shelf as it always was, whatever was played.
        let off = serde_json::json!({ "decorations": false });
        let c = shot(&tmp, &lib, &new, &stub, "shelf", appearance, size, "15:00", off.clone());
        let d = shot(&tmp, &lib, &long, &stub, "shelf", appearance, size, "15:00", off);
        assert!(c.same_below_clock(&d), "{size} {appearance}: decorations off");
    }
    // The Cabinet: the SNES shelf's figurine and the plant beside it show there too.
    for size in ["640x480", "1280x720"] {
        let a = shot(&tmp, &lib, &new, &stub, "cabinet", "dark", size, "15:00", on());
        let b = shot(&tmp, &lib, &long, &stub, "cabinet", "dark", size, "15:00", on());
        let changed = a.px.chunks(3).zip(b.px.chunks(3)).filter(|(p, q)| p != q).count();
        assert!(changed > 300, "{size}: the Cabinet shows what was earned ({changed} pixels)");
    }
}
