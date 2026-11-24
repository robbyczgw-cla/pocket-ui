//! Brick audit 2026-11-20 (#9): the first start's look step ("How your games stand") on the Brick's 1024x768. A note too
//! long for one line beside its pills goes on two lines; at Large text they hung down onto the next row, and in German
//! from row to row down into the line under the rows. Painted by the real program in English and German, Normal and
//! Large text: from the look's name to the hint bar every band of ink (a row, a line) stands at least 6 px (half the
//! rows' gap) from the next, and none is as tall as two rows run together. `FIRST_START_SHOTS=<dir>` keeps the pictures.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, for scripts/wayland-stub.py.
use std::path::PathBuf;
use std::process::Command;
include!("common/root.rs");
struct Stub(std::process::Child);
impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The picture's pixels as 0xRRGGBB, and its width and height.
fn pixels(path: &std::path::Path) -> (Vec<u32>, usize, usize) {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).expect("snapshot written"))).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let ch = info.color_type.samples();
    let px = buf[..info.buffer_size()].chunks_exact(ch).map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect();
    (px, info.width as usize, info.height as usize)
}

#[test]
fn the_look_rows_stand_apart_on_the_brick_in_every_text_size() {
    let tmp_guard = tempfile::Builder::new().prefix("pui-look-rows-").tempdir().unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null()).status().expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    let socket = tmp.join("wl");
    let _stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
        let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), socket.to_str().unwrap()]).spawn().expect("python3");
        test_clock::wait_for(|| socket.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
        Stub(child)
    });
    let bin: PathBuf = std::env::var_os("POCKETUI_NAV_BINARY").map(PathBuf::from).unwrap_or_else(|| env!("CARGO_BIN_EXE_pocket-ui").into());
    let shots = std::env::var_os("FIRST_START_SHOTS").map(PathBuf::from);
    let mut failed = Vec::new();
    let mut i = 0;
    for lang in ["en", "de"] {
        for large in [false, true] {
            i += 1;
            let data = tmp.join(format!("data-{i}"));
            std::fs::create_dir_all(&data).unwrap();
            for e in std::fs::read_dir(tmp.join("data")).unwrap().flatten() {
                if e.path().is_file() {
                    std::fs::copy(e.path(), data.join(e.file_name())).unwrap();
                }
            }
            std::fs::write(data.join("settings.json"), r#"{"theme": "shelf", "onboarded": false, "motion": "off"}"#).unwrap();
            let (png, boxes) = (tmp.join(format!("{i}.png")), tmp.join(format!("{i}.boxes")));
            let mut cmd = Command::new(&bin);
            if socket.exists() {
                cmd.env("WAYLAND_DISPLAY", &socket).env_remove("SWAYSOCK");
            }
            let out = cmd.args(["--snapshot", "setup:first-look", png.to_str().unwrap()])
                .env("POCKETUI_SNAPSHOT_SIZE", "1024x768").env("POCKETUI_SNAPSHOT_FROZEN", "1")
                .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_LANG", lang)
                .env("POCKETUI_TEXT_SIZE", if large { "large" } else { "normal" })
                .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg")).env("POCKETUI_DATA", &data)
                .env("POCKETUI_TEXT_BOXES", &boxes)
                .output().expect("run pocket-ui");
            let name = format!("1024x768 {lang} {}", if large { "large" } else { "normal" });
            if !out.status.success() {
                failed.push(format!("{name}: did not render: {}", String::from_utf8_lossy(&out.stderr)));
                continue;
            }
            if let Some(dir) = &shots {
                std::fs::create_dir_all(dir).unwrap();
                std::fs::copy(&png, dir.join(format!("look-rows-{}.png", name.replace(' ', "-")))).unwrap();
            }
            let bad: Vec<String> = std::fs::read_to_string(&boxes).unwrap_or_default().lines()
                .filter(|l| l.starts_with(['E', 'O', 'V', 'W', 'T'])).map(str::to_string).collect();
            if !bad.is_empty() {
                failed.push(format!("{name}: {bad:?}"));
            }
            // Bands of scanlines with ink between the page's margins, from the middle of the screen (the look's name)
            // down; the last one is the hint bar.
            let (px, w, h) = pixels(&png);
            let margin = 72;
            let bg = px[(h / 2) * w + 2];
            let mut bands: Vec<(usize, usize)> = Vec::new();
            for y in h / 2..h {
                if !(margin..w - margin).any(|x| px[y * w + x] != bg) {
                    continue;
                }
                match bands.last_mut() {
                    Some(b) if b.1 == y => b.1 = y + 1,
                    _ => bands.push((y, y + 1)),
                }
            }
            bands.pop();
            for pair in bands.windows(2) {
                if pair[1].0 - pair[0].1 < 6 {
                    failed.push(format!("{name}: {} px between {:?} and {:?}", pair[1].0 - pair[0].1, pair[0], pair[1]));
                }
            }
            if let Some(b) = bands.iter().find(|b| b.1 - b.0 > 80) {
                failed.push(format!("{name}: a band {b:?} as tall as two rows run together: {bands:?}"));
            }
            if bands.len() < 5 {
                failed.push(format!("{name}: {} bands, want the name, its line and three rows: {bands:?}", bands.len()));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
