//! Sealed new games (Settings → Look → Cozy extras → Sealed new games, `src/shrink_wrap.rs`) as the player meets them,
//! on the made-up library (scripts/curator-fake-library.py, where Castlevania was never played and Super Metroid was):
//! a never-played game stands in its film in every look and on its box's front and back, a played one never does, the
//! extra off takes the film away, and the first start tears it off for good (every 60 Hz frame of the tear, then the
//! start as without it). The pictures run with `POCKETUI_SEALED=1`: snapshots leave the made-up libraries unsealed
//! otherwise. `SEALED_SHOTS=<folder>` keeps the pictures. Needs python3 (and, on Linux without a compositor,
//! scripts/wayland-stub.py), as tests/cozy_extras.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

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
    let path = tmp.join("wl");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let stub = Stub(child, path);
    test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib");
    let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    // Box art for the two games looked at (the made-up library has none): the film lies over a cover in Split and on the
    // card. A dark night gradient with a pale moon, as a box's art would be.
    for (system, stem) in [("psx", "Castlevania - Symphony of the Night (USA)"), ("snes", "Super Metroid (Japan, USA) (En,Ja)")] {
        let path = lib.join("data/covers").join(system).join(format!("{stem}.png"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let (w, h) = (300u32, 300u32);
        let mut e = png::Encoder::new(std::fs::File::create(&path).unwrap(), w, h);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = (0..w * h).flat_map(|i| {
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            let moon = (x - 200).pow(2) + (y - 90).pow(2) < 45 * 45;
            if moon { [225, 220, 200] } else { [(30 + y / 6) as u8, (20 + y / 10) as u8, (60 + x / 8) as u8] }
        }).collect();
        e.write_header().unwrap().write_image_data(&data).unwrap();
    }
    lib
}

struct Pic {
    w: i32,
    h: i32,
    px: Vec<u8>,
}

impl Pic {
    fn read(png: &Path) -> Pic {
        let mut reader = png::Decoder::new(std::fs::File::open(png).unwrap()).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgb);
        Pic { w: info.width as i32, h: info.height as i32, px: buf }
    }

    /// Pixels that differ from `o` by more than a rounding (below the status corner's live clock).
    fn changed(&self, o: &Pic) -> usize {
        assert_eq!((self.w, self.h), (o.w, o.h));
        let from = (self.w * 110 * 3) as usize;
        self.px[from..].chunks(3).zip(o.px[from..].chunks(3)).filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 6)).count()
    }

    /// The box around the pixels that differ (left, top, right, bottom), below the top 110 rows as [`Pic::changed`].
    fn changed_box(&self, o: &Pic) -> (i32, i32, i32, i32) {
        let (mut l, mut t, mut r, mut b) = (i32::MAX, i32::MAX, -1, -1);
        for y in 110..self.h {
            for x in 0..self.w {
                let i = ((y * self.w + x) * 3) as usize;
                if self.px[i..i + 3].iter().zip(&o.px[i..i + 3]).any(|(p, q)| p.abs_diff(*q) > 6) {
                    (l, t, r, b) = (l.min(x), t.min(y), r.max(x), b.max(y));
                }
            }
        }
        (l, t, r, b)
    }

    /// How many columns of the middle row show something other than the page at its left edge.
    fn wide(&self) -> i32 {
        let y = self.h / 2;
        let at = |x: i32| &self.px[((y * self.w + x) * 3) as usize..][..3];
        let page = at(2).to_vec();
        (0..self.w).filter(|x| at(*x).iter().zip(&page).any(|(a, b)| a.abs_diff(*b) > 12)).count() as i32
    }

    /// Which pixels differ from `o` (below the status corner).
    fn mask(&self, o: &Pic) -> Vec<bool> {
        let from = (self.w * 110 * 3) as usize;
        self.px[from..].chunks(3).zip(o.px[from..].chunks(3)).map(|(a, b)| a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > 6)).collect()
    }

    /// The mean brightness (0-255) of the pixels that differ from `o`, here and there: the film lightens what it covers.
    fn lighter_where_changed(&self, o: &Pic) -> (f64, f64) {
        let (mut a, mut b, mut n) = (0f64, 0f64, 0f64);
        let from = (self.w * 110 * 3) as usize;
        for (p, q) in self.px[from..].chunks(3).zip(o.px[from..].chunks(3)) {
            if p.iter().zip(q).any(|(x, y)| x.abs_diff(*y) > 6) {
                a += p.iter().map(|v| *v as f64).sum::<f64>() / 3.0;
                b += q.iter().map(|v| *v as f64).sum::<f64>() / 3.0;
                n += 1.0;
            }
        }
        (a / n.max(1.0), b / n.max(1.0))
    }
}

struct Run<'a> {
    tmp: &'a Path,
    lib: &'a Path,
    data: &'a Path,
    stub: &'a Option<Stub>,
}

impl Run<'_> {
    /// `state` in look `theme` at `size`, sealing on or off for this run; extra environment in `env`.
    fn shot(&self, state: &str, theme: &str, size: &str, sealed: bool, env: &[(&str, &str)]) -> (PathBuf, String) {
        let settings = self.data.join("settings.json");
        let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
        json["theme"] = serde_json::json!(theme);
        json["onboarded"] = serde_json::json!(true);
        json["motion"] = serde_json::json!("full");
        json["animations"] = serde_json::json!("cozy");
        std::fs::write(&settings, json.to_string()).unwrap();
        let name = format!("{theme}-{size}-{}-{}.png", state.replace([':', ' ', ','], "_"), if sealed { "sealed" } else { "plain" });
        let png = self.tmp.join(&name);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_ROOM_TIME", "12:00")
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_ES_SYSTEMS", self.lib.join("es_systems.cfg"))
            .env("POCKETUI_DATA", self.data)
            .env("POCKETUI_SEALED", if sealed { "1" } else { "0" })
            .envs(env.iter().copied())
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        if let Some(dir) = std::env::var_os("SEALED_SHOTS") {
            let dir = PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            if png.is_file() {
                let _ = std::fs::copy(&png, dir.join(&name));
            }
        }
        (png, String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn pic(&self, state: &str, theme: &str, size: &str, sealed: bool) -> Pic {
        Pic::read(&self.shot(state, theme, size, sealed, &[]).0)
    }
}

const CASTLEVANIA: &str = "Castlevania - Symphony of the Night (USA).chd";

#[test]
fn a_never_played_game_stands_in_its_film_everywhere_and_a_played_one_never_does() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let run = Run { tmp: tmp.path(), lib: &lib, data: &data, stub: &stub };
    // Castlevania (never played) in every look, its box's front and back, and its card in Split; on the H700's screen,
    // the Brick's and a TV.
    let big = format!("big-shelf:{CASTLEVANIA}");
    let views: [(&str, &str); 7] = [
        ("shelf:psx:shelf:games:0", "shelf"),
        ("shelf:psx:cabinet:games:0", "cabinet"),
        (&big, "big"),
        ("shelf:psx:split:games:0", "split"),
        ("box:psx:castlevania", "shelf"),
        ("box:psx:castlevania:r", "shelf"),
        ("curator:card:psx:castlevania", "split"),
    ];
    for size in ["640x480", "1024x768", "1920x1080"] {
        for (state, theme) in views {
            let sealed = run.pic(state, theme, size, true);
            let plain = run.pic(state, theme, size, false);
            let changed = sealed.changed(&plain);
            let area = (sealed.w * sealed.h) as usize;
            assert!(changed * 1000 > area, "{size} {theme} {state}: the never-played game's film shows ({changed} pixels of {area})");
            if state.starts_with("box:") {
                // The sealed box stays shut: its disc is not out beside it, so the box (front and back) is narrower.
                assert!(sealed.wide() + 20 < plain.wide(), "{size} {state}: a sealed case keeps its game inside ({} against {} columns)", sealed.wide(), plain.wide());
                continue;
            }
            let (with, without) = sealed.lighter_where_changed(&plain);
            assert!(with > without + 4.0, "{size} {theme} {state}: the film is a light sheen over the case ({with:.0} against {without:.0})");
        }
    }
    // Super Metroid was played (8 hours, Completed): its box and card are exactly as they always were.
    for (state, theme) in [("box:snes:metroid", "shelf"), ("box:snes:metroid:r", "shelf"), ("curator:card:snes:metroid", "split")] {
        let sealed = run.pic(state, theme, "1024x768", true);
        let plain = run.pic(state, theme, "1024x768", false);
        assert_eq!(sealed.changed(&plain), 0, "{state}: a played game is never sealed");
    }
    // Cozy extras → Sealed new games off: no film anywhere.
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    json["extras"] = serde_json::json!({ "sealed_new_games": false });
    std::fs::write(&settings, json.to_string()).unwrap();
    for (state, theme) in views {
        let sealed = run.pic(state, theme, "1024x768", true);
        let plain = run.pic(state, theme, "1024x768", false);
        assert_eq!(sealed.changed(&plain), 0, "{theme} {state}: the extra off takes the film away");
    }
}

/// Air emulator, Android handheld view, 14.11.: the Curator and Just Play boxes on the home shelf stood in film. Their made-up
/// "ROM" (it only tells their pictures apart) has no play history, so it counted as a game never played. Only games are sealed.
#[test]
fn curator_s_and_just_play_s_boxes_are_never_sealed() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let run = Run { tmp: tmp.path(), lib: &lib, data: &data, stub: &stub };
    // The Shelf look's home: Continue (played games), Just Play, Search, Curator and the systems. (The Cabinet's "home" is a
    // system's shelf with never-played games, rightly in film.)
    for (theme, size) in [("shelf", "640x480"), ("shelf", "1024x768"), ("shelf", "1920x1080")] {
        let sealed = run.pic("home", theme, size, true);
        let plain = run.pic("home", theme, size, false);
        assert_eq!(sealed.changed(&plain), 0, "{theme} {size}: the home shelf's boxes are no games, nothing on it is in film (changed in {:?})", sealed.changed_box(&plain));
    }
}

#[test]
fn the_first_start_tears_the_film_off_frame_by_frame_and_it_stays_off() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let run = Run { tmp: tmp.path(), lib: &lib, data: &data, stub: &stub };
    let state = "shelf:psx:shelf:games:0";
    let series = |name: &str, sealed: bool| -> Vec<Pic> {
        let dir = tmp.path().join(name);
        let (_, out) = run.shot(state, "shelf", "640x480", sealed, &[("POCKETUI_SNAPSHOT_LAUNCH_SERIES", dir.to_str().unwrap())]);
        assert!(out.contains("launch-series paint frames="), "{out}");
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
        files.sort();
        if let Some(keep) = std::env::var_os("SEALED_SHOTS") {
            let keep = PathBuf::from(keep).join(name);
            let _ = std::fs::create_dir_all(&keep);
            for f in &files {
                let _ = std::fs::copy(f, keep.join(f.file_name().unwrap()));
            }
        }
        files.iter().map(|f| Pic::read(f)).collect()
    };
    // The sealed shelf; the first start, frame by frame (it unwraps Castlevania for good); then the same start again on the
    // same library, now unwrapped while every other new game is still sealed: what a start without a film looks like.
    let shelf = run.pic(state, "shelf", "640x480", true);
    // Without film, capture the semantic change at first start separately ("Not played yet" becomes "Not here yet").
    let bare_before = run.pic(state, "shelf", "640x480", false);
    let torn = series("torn", true);
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(data.join("library.json")).unwrap()).unwrap();
    let key = lib.join("roms/psx").join(CASTLEVANIA);
    assert_eq!(saved["games"][key.to_str().unwrap()]["unwrapped"], serde_json::json!(true), "unwrapped is kept in library.json");
    let shelf_after = run.pic(state, "shelf", "640x480", true);
    let bare_after = run.pic(state, "shelf", "640x480", false);
    let plain = series("again", true);
    // Twenty frames longer: the tear, then the same start. (The series rounds its length up to whole frames: 20 or 21.)
    assert!((plain.len() + 20..=plain.len() + 21).contains(&torn.len()), "the scene waits 20 frames for the tear: {} against {}", torn.len(), plain.len());
    // A: the film is still on the case, exactly as the shelf showed it (nothing jumps): where the first frame with film
    // differs from the first frame of a start without is where the sealed shelf differs from the unwrapped one.
    assert!(shelf.changed(&shelf_after) > 300, "the shelf shows the case unwrapped after its first start");
    let film_at_a = torn[0].mask(&plain[0]);
    let label_changed = bare_before.mask(&bare_after);
    let film_on_shelf: Vec<_> = shelf.mask(&shelf_after).into_iter().zip(label_changed).map(|(film, label)| film && !label).collect();
    let moved = film_at_a.iter().zip(&film_on_shelf).filter(|(a, b)| a != b).count();
    assert!(moved < 150, "the first frame shows the shelf's film where it was ({moved} pixels elsewhere)");
    // Every frame of the tear moves on from the one before by a part of the picture, never all at once.
    let area = (torn[0].w * torn[0].h) as usize;
    for k in 1..21 {
        let step = torn[k].changed(&torn[k - 1]);
        assert!(step < area / 6, "frame {k}: the tear moves by pieces ({step} pixels)");
    }
    assert!(torn[16].changed(&plain[0]) > torn[0].changed(&plain[0]) / 4, "the pieces are on their way, not gone at once");
    // Once the pieces are gone (560 ms, frame 34) each frame is the start without film.
    for k in 34..plain.len() {
        let off = torn[k + 20].changed(&plain[k]);
        assert!(off < 50, "frame {k}: after the tear the start is as for any game ({off} pixels differ)");
    }
    // For good: its box is the box of a game without film, sealing on or not.
    let boxed = run.pic("box:psx:castlevania", "shelf", "640x480", true);
    let never = run.pic("box:psx:castlevania", "shelf", "640x480", false);
    assert_eq!(boxed.changed(&never), 0, "after its first start the case stays unwrapped");
}

/// Reading the back of a new game's box must not spend a frame compositing the same full-size translucent film.
#[test]
fn reading_a_sealed_box_back_is_as_quick_as_an_opened_one() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let run = Run { tmp: tmp.path(), lib: &lib, data: &data, stub: &stub };
    for size in ["640x480", "720x720"] {
        let field = |out: &str, name: &str| {
            let line = out.lines().find(|l| l.starts_with("bench-box ")).unwrap();
            line.split_whitespace().find_map(|word| word.strip_prefix(&format!("{name}="))).unwrap().parse::<f64>().unwrap()
        };
        let mut failures = Vec::new();
        for _ in 0..3 {
            let (_, plain) = run.shot("box:psx:castlevania:r", "shelf", size, false, &[("POCKETUI_BENCH_FRAMES", "60")]);
            let (_, film) = run.shot("box:psx:castlevania:r", "shelf", size, true, &[("POCKETUI_BENCH_FRAMES", "60")]);
            assert!(plain.contains("boxed=true sealed=false"), "{plain}");
            assert!(film.contains("boxed=true sealed=true"), "{film}");
            let (plain, film) = (field(&plain, "p50_us"), field(&film, "p50_us"));
            println!("box back {size}: opened {plain}, film {film} us");
            if film <= plain * 1.25 + 100.0 { failures.clear(); break; }
            failures.push(format!("{size}: opened {plain}, film {film} us"));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

#[test]
fn a_box_back_keeps_its_visible_words_in_layout_and_navigation_checks() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let run = Run { tmp: tmp.path(), lib: &lib, data: &data, stub: &stub };
    let visible = tmp.path().join("visible.txt");
    for size in ["1024x768", "720x720"] {
        run.shot("box:psx:castlevania:r", "shelf", size, true, &[("POCKETUI_VISIBLE_TEXT", visible.to_str().unwrap())]);
        let words = std::fs::read_to_string(&visible).unwrap();
        assert!(words.lines().filter(|line| line.contains("Castlevania")).count() >= 2,
            "the page header and the back's title are both visible at {size}: {words}");
    }
}
