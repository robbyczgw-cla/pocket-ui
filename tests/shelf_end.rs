//! The Shelf look at the ends of its shelf (`app_cabinet.rs` "the Shelf look", docs/DESIGN.md "Layout"): with the first
//! entry selected the row starts at the shelf's left end, with the last one selected it reaches the right end (or
//! stands whole from the left, where it fits), so the end of the row never leaves the shelf half empty; a short list
//! shows whole; nothing is painted past the shelf's ends. Rendered at the Mini V2's 1240x1080, the Pocket 5's
//! 1920x1080 (in its column) and the Mini V1's 1280x960, dark and light, with scripts/curator-fake-library.py's
//! library, and measured in the pictures: the spines along a line just above the board. Android's handheld view (a
//! made-up phone, `POCKETUI_PHONE`) gives the shelf the whole width of a 16:9 or 20:9 screen (`app_column::shelf_fills_width`)
//! and paints the 4:3 and square screens as Linux does.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/text_size.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");
/// The page's side margin (`palette::MARGIN`).
const MARGIN: i32 = 72;

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
    test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// The picture: width, height, RGB.
/// In Android's handheld view (`android`: the host read a phone, here a made-up one without apps or cores) or on Linux.
fn shot_on(tmp: &Path, stub: &Option<Stub>, look: &str, size: &str, state: &str, android: bool) -> (i32, i32, Vec<u8>) {
    let lib = tmp.join(format!("lib-{look}"));
    if !lib.exists() {
        let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), look])
            .stdout(std::process::Stdio::null())
            .status()
            .expect("python3 for scripts/curator-fake-library.py");
        assert!(ok.success());
    }
    let name = format!("{}{look}-{size}-{}", if android { "android-" } else { "" }, state.replace(':', "_"));
    let data = tmp.join(format!("data-{name}"));
    copy_dir(&lib.join("data"), &data);
    // Settings → Look → Look: Shelf, whatever the made-up library chose.
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!("shelf");
    json["onboarded"] = serde_json::json!(true);
    std::fs::write(&settings, json.to_string()).unwrap();
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    if android {
        let phone = tmp.join("phone.json");
        std::fs::write(&phone, "{}").unwrap();
        cmd.env("POCKETUI_PHONE", &phone);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", "normal")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    if let Some(dir) = std::env::var_os("SHELF_SHOTS").filter(|_| android) {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::copy(&png, Path::new(&dir).join(format!("{name}.png")));
    }
    let decoder = png::Decoder::new(std::fs::File::open(&png).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    (info.width as i32, info.height as i32, buf[..info.buffer_size()].to_vec())
}

/// The shelf on a screen `w` x `h` as `app_cabinet::shelf_layout` lays it out: its left and right end (in the column
/// of a wide screen, `app_column::column_width`) and the line to look along, above the lifted spine's foot.
/// In Android's handheld view (`android`) the shelf has the whole width at every size.
fn shelf_line_on(w: i32, h: i32, android: bool) -> (i32, i32, i32) {
    let top = if h < 600 { 80 } else { 104 };
    // The board stands over the hint bar (`kit::content_bottom`); the spines' size is the one 112 px of room gave.
    let bottom = h - 128;
    let f = (((h - 112 - top).max(200) as f32 * 0.42 / 120.0 * 2.0).floor() / 2.0).clamp(1.0, 4.0);
    let floor = bottom - (8.0 * f).round() as i32;
    let lift = (16.0 * f).round() as i32;
    let col = if w >= 1600 && !android { (w * 3 / 4).max(1440).min(w) } else { w };
    let x0 = (w - col) / 2;
    (x0 + MARGIN, x0 + col - MARGIN, floor - lift - 12)
}

/// What stands on the shelf along that line: (left, right) of each run of pixels that differ from the page.
fn runs((w, _, px): &(i32, i32, Vec<u8>), y: i32) -> Vec<(i32, i32)> {
    let at = |x: i32| {
        let i = ((y * w + x) * 3) as usize;
        (px[i], px[i + 1], px[i + 2])
    };
    let bg = at(2);
    let mut out: Vec<(i32, i32)> = Vec::new();
    for x in 0..*w {
        if at(x) == bg {
            continue;
        }
        match out.last_mut() {
            Some(r) if r.1 == x - 1 => r.1 = x,
            _ => out.push((x, x)),
        }
    }
    out
}

#[test]
fn the_shelf_stops_at_the_ends_of_its_list_at_every_size() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    // Linux at three sizes; Android's handheld view on the wide screens, where its shelf has the whole width.
    let cases = [("1240x1080", false), ("1920x1080", false), ("1280x960", false), ("1920x1080", true), ("2400x1080", true)];
    for look in ["dark", "light"] {
        for (size, android) in cases {
            for state in ["home", "home:-2", "home:0", "curator:hub:shelf:6"] {
                let pic = shot_on(&tmp, &stub, look, size, state, android);
                let (left, right, y) = shelf_line_on(pic.0, pic.1, android);
                let r = runs(&pic, y);
                let what = format!("{}{state} at {size}, {look}: along y {y} from {left} to {right}: {r:?}", if android { "Android " } else { "" });
                let (first, last) = (r.first().map_or(i32::MAX, |r| r.0), r.last().map_or(0, |r| r.1));
                if first < left || last > right {
                    wrong.push(format!("{what}: painted past the shelf's ends"));
                }
                let from_left = first <= left + 4;
                let to_right = last >= right - 4;
                match state {
                    // The first entry selected: the row starts at the shelf's left end.
                    "home:0" if !from_left => wrong.push(format!("{what}: doesn't start at the left end")),
                    // The last ones: the row reaches the right end, or stands whole from the left.
                    "home" | "home:-2" if !(to_right || from_left) => wrong.push(format!("{what}: the shelf's end left empty")),
                    // The short list (Curator's seven entries, the last selected): all of them, from the left (counted on
                    // the dark page; on the light one the cases' shadows fill the gaps between them).
                    "curator:hub:shelf:6" if !from_left || (look == "dark" && r.len() < 7) => {
                        wrong.push(format!("{what}: not the seven entries from the left"))
                    }
                    _ => {}
                }
            }
        }
    }
    drop(stub);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Android's handheld view on the Odin 2 Mini's 1920x1080 and a 20:9 phone turned (2400x1080): the shelf reaches from
/// the left margin to the right one, so it shows as many spines as the screen holds. In the column (Linux's wide-screen
/// rule) it stopped at three quarters of the width with black bands both sides (Odin, ANDROID-READINESS 13.11.). The
/// 4:3 and square screens (the Mini V1's 1280x960, the CubeXX's 720x720) and the Mini V2's 1240x1080 are painted pixel
/// for pixel as on Linux. `SHELF_SHOTS=<dir>` keeps the Android pictures.
#[test]
fn android_shelf_uses_the_whole_width_of_a_wide_screen_and_keeps_4_3_and_square() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    for size in ["1920x1080", "2400x1080"] {
        for state in ["home:0", "home"] {
            let pic = shot_on(&tmp, &stub, "dark", size, state, true);
            let (left, right, y) = shelf_line_on(pic.0, pic.1, true);
            let r = runs(&pic, y);
            let (first, last) = (r.first().map_or(i32::MAX, |r| r.0), r.last().map_or(0, |r| r.1));
            let what = format!("Android {state} at {size}: along y {y} from {left} to {right}: spines from {first} to {last}");
            if first < left || last > right {
                wrong.push(format!("{what}: painted past the shelf's ends"));
            }
            // What the player sees: spines from near the left edge to near the right one, at least 90 % of the width.
            if (last - first) * 10 < pic.0 * 9 {
                wrong.push(format!("{what}: the spines cover {} % of the width", (last - first) * 100 / pic.0.max(1)));
            }
        }
    }
    for size in ["1280x960", "1240x1080", "720x720"] {
        for state in ["home:0", "home"] {
            let (linux, android) = (shot_on(&tmp, &stub, "dark", size, state, false), shot_on(&tmp, &stub, "dark", size, state, true));
            if linux != android {
                wrong.push(format!("{state} at {size}: Android's picture is not Linux's"));
            }
        }
    }
    drop(stub);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The selection band of a page of rows (`kit::selection`, light on the dark page): its height (the rows of the picture
/// that are mostly band) and its width along its middle.
fn band((w, h, px): &(i32, i32, Vec<u8>)) -> (i32, i32) {
    let light = |x: i32, y: i32| {
        let i = ((y * w + x) * 3) as usize;
        px[i] > 200 && px[i + 1] > 200 && px[i + 2] > 200
    };
    let rows: Vec<i32> = (0..*h).filter(|&y| (0..*w).filter(|&x| light(x, y)).count() as i32 > w / 2).collect();
    let Some(&mid) = rows.get(rows.len() / 2) else { return (0, 0) };
    let (l, r) = ((0..*w).find(|&x| light(x, mid)).unwrap_or(0), (0..*w).rev().find(|&x| light(x, mid)).unwrap_or(0));
    (rows.len() as i32, r - l + 1)
}

/// A hand test on the Odin at 1920x1080 (15.11.2026): "Settings don't use the whole screen" (the rows in the ¾ column, in
/// the Mini V2's sizes, a third of the height). A category's rows now take nearly the whole width of a wide screen and
/// are taller with the type (`app_column::page_column_width`, `type_scale::wide_page`); the Mini V2's page is as it was.
#[test]
fn settings_rows_use_a_wide_screen_and_the_mini_v2_keeps_its_page() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path();
    let stub = wayland(tmp);
    let state = "settings:3:0";
    let (mini_h, mini_w) = band(&shot_on(tmp, &stub, "dark", "1240x1080", state, false));
    // The 66 px row's band as measured on main before the change (its round ends are not counted whole).
    assert_eq!((mini_h, mini_w), (58, 1178), "the Mini V2's selected row as before");
    let mut wrong = Vec::new();
    for (size, android) in [("1920x1080", false), ("1920x1080", true), ("2400x1080", true)] {
        let shot = shot_on(tmp, &stub, "dark", size, state, android);
        let (h, w) = band(&shot);
        if w * 100 < shot.0 * 85 || h * 4 < mini_h * 5 {
            wrong.push(format!("{size} (Android {android}): the selected row is {w} x {h} px, wanted at least 85 % of {} wide and {} high", shot.0, mini_h * 5 / 4));
        }
    }
    drop(stub);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
