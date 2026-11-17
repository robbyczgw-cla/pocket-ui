//! The Cabinet's case out of its shelf never stands over the TV (or the handheld in the TV's place), wherever its spine
//! stands on the shelf: a hand test on the Brick (17.11.2026), Donkey Kong Country 2, the last game on its shelf's right end,
//! stood open after "Save & quit", and the cartridge out of its case reached over the TV's console.
//!
//! Every spine of every shelf is walked (a snake: right along one shelf, down, left along the next), with every case
//! open (`POCKETUI_SNAPSHOT_FROZEN=all`: the game stands out of its right side) and every case shut, at the looks' sizes,
//! dark and light. The case's rectangle and its game's must not meet the TV's place, nor the place of the console beside
//! it (17.11.2026), which stays the same on every shelf and never stands on the TV.
use std::{fs, path::Path, process::{Child, Command}, time::Duration};

struct Compositor(Child);
impl Drop for Compositor {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

fn compositor(root: &Path) -> Compositor {
    let socket = root.join("wayland");
    let child = Command::new("python3").args(["-I", concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/wayland-stub.py")])
        .arg(&socket).spawn().unwrap();
    let stub = Compositor(child);
    test_clock::wait_for(|| socket.exists(), Duration::from_secs(2), "Wayland stub listening");
    stub
}

type Rect = (i32, i32, i32, i32);

fn rect(v: &str) -> Option<Rect> {
    let n: Vec<i32> = v.split(',').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    (n.len() == 4).then(|| (n[0], n[1], n[2], n[3]))
}

fn meet(a: Rect, b: Rect) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// (system, name, extension): carts landscape and upright, a jewel case, a DVD case, the handhelds (their handheld in the
/// TV's place), the PSP's UMD and the DS's card box.
const SYSTEMS: [(&str, &str, &str); 7] = [
    ("gba", "Game Boy Advance", "gba"), ("n64", "Nintendo 64", "z64"), ("nds", "Nintendo DS", "nds"),
    ("nes", "Nintendo Entertainment System", "nes"), ("ps2", "PlayStation 2", "iso"), ("psx", "PlayStation", "bin"),
    ("snes", "Super Nintendo", "sfc"),
];
/// Games a shelf: more than a row of the widest screen shows, so the shelf scrolls and its end stands at the TV.
const GAMES: usize = 26;

fn library(root: &Path) {
    fs::create_dir_all(root.join("data")).unwrap();
    fs::write(root.join("data/curator.json"), r#"{"lens":false,"stickers":false,"history":false}"#).unwrap();
    let mut xml = String::from("<systemList>");
    let mut recents = Vec::new();
    for (id, full, ext) in SYSTEMS {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        for g in 0..GAMES {
            let path = dir.join(format!("Fixture {id} {g:02}.{ext}"));
            fs::write(&path, "x").unwrap();
            recents.push(format!(
                r#"{{"system":"{id}","name":"Fixture {id} {g:02}","path":"{}","last_played":{},"play_secs":60}}"#,
                path.display(),
                1_700_000_000 - recents.len() as u64
            ));
        }
        xml += &format!("<system><name>{id}</name><fullname>{full}</fullname><path>{}</path><extension>.{ext}</extension></system>", dir.display());
    }
    fs::write(root.join("es_systems.cfg"), xml + "</systemList>").unwrap();
    fs::write(root.join("data/recents.json"), format!("[{}]", recents.join(","))).unwrap();
}

/// The outcome line after every key of the walk (`POCKETUI_SNAPSHOT_OUTCOME_EACH`): each line's fields by name.
fn walk(root: &Path, keys: &str, size: &str, look: &str, open: bool) -> Vec<std::collections::HashMap<String, String>> {
    fs::write(root.join("data/settings.json"), format!(r#"{{"appearance":"{look}","onboarded":true}}"#)).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", &format!("cabinet:{keys}")]).arg(root.join("shot.png"))
        .env("WAYLAND_DISPLAY", root.join("wayland"))
        .env("POCKETUI_MOCK", "1").env("POCKETUI_LANG", "en")
        .env("POCKETUI_DEVICE", "miniv2").env("POCKETUI_TEXT_SIZE", "normal")
        .env("POCKETUI_SNAPSHOT_SIZE", size).env("POCKETUI_DATA", root.join("data"))
        .env("POCKETUI_ES_SYSTEMS", root.join("es_systems.cfg"))
        .env("POCKETUI_SNAPSHOT_OUTCOME_EACH", "1");
    if open {
        cmd.env("POCKETUI_SNAPSHOT_FROZEN", "all");
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{size} {look}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().lines().filter(|l| l.starts_with("cabinet-outcome "))
        .map(|l| l.split_whitespace().filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect())
        .collect()
}

#[test]
fn the_case_out_of_its_shelf_and_its_open_game_never_stand_over_the_tv() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_owned();
    library(&root);
    let _compositor = compositor(&root);
    // Right along a shelf, down, left along the next: every spine of every shelf (the Continue shelf first).
    let mut keys = String::new();
    for s in 0..=SYSTEMS.len() {
        keys += &(if s % 2 == 0 { "r" } else { "l" }).repeat(GAMES);
        keys.push('d');
    }
    // The looks' sizes: the H700's 640 x 480 (laid out at twice its size), the Brick, the Mini V2, the Smart Pro, a TV.
    // The light look on the Brick, dark everywhere; the colours do not move anything.
    let runs = ["640x480", "1024x768", "1240x1080", "1280x720", "1920x1080"].map(|s| (s, "dark")).into_iter().chain([("1024x768", "light")]);
    let mut failures = Vec::new();
    for (size, look) in runs {
        for open in [true, false] {
            let lines = walk(&root, &keys, size, look, open);
            assert!(lines.len() > GAMES * SYSTEMS.len(), "{size} {look}: one outcome a key, got {}", lines.len());
            let mut ajar = 0;
            let mut consoles = 0;
            let first_place = lines.iter().find_map(|f| rect(&f["console_place"]));
            for (step, f) in lines.iter().enumerate() {
                let Some(tv) = rect(&f["place"]) else { continue };
                let case = rect(&f["case"]).unwrap();
                let game = rect(&f["ajar"]);
                ajar += game.is_some() as usize;
                // The console beside the TV (17.11.2026): its place is the same on every shelf, it stands at that place's
                // right end on the board, never on the TV, and the case and its game keep clear of the whole place.
                let place = rect(&f["console_place"]);
                if place != first_place {
                    failures.push(format!("{size} {look} open={open} key {step}: the console's place {place:?} moved from {first_place:?}"));
                }
                let console = rect(&f["console"]);
                if let Some(cn) = console {
                    consoles += 1;
                    let p = place.expect("a console stands only in its place");
                    if cn.0 < p.0 || cn.0 + cn.2 != p.0 + p.2 || cn.1 + cn.3 != p.1 + p.3 || cn.3 > p.3 {
                        failures.push(format!("{size} {look} open={open} key {step}: the console {cn:?} is not at the right end of its place {p:?} on the board"));
                    }
                    if f["handheld"] == "true" {
                        failures.push(format!("{size} {look} open={open} key {step}: a console {cn:?} beside a handheld"));
                    }
                }
                for (what, r) in [("console's place", place), ("console", console)] {
                    if let Some(r) = r.filter(|r| meet(*r, tv)) {
                        failures.push(format!("{size} {look} open={open} key {step}: the {what} {r:?} stands on the TV {tv:?}"));
                    }
                }
                for (what, r) in [("case", Some(case)), ("open game", game)] {
                    if let Some(r) = r.filter(|r| meet(*r, tv)) {
                        failures.push(format!("{size} {look} open={open} key {step}: the {what} {r:?} stands over the TV {tv:?}"));
                    }
                    if let (Some(r), Some(p)) = (r, place.filter(|p| r.is_some_and(|r| meet(r, *p)))) {
                        failures.push(format!("{size} {look} open={open} key {step}: the {what} {r:?} stands over the console's place {p:?}"));
                    }
                }
            }
            // Five of the seven systems are home consoles: where the console has its place, it stands on their shelves.
            if first_place.is_some() {
                assert!(consoles > GAMES * 4, "{size} {look}: the console stood beside the TV in {consoles} of {} keys", lines.len());
            }
            if ["640x480", "1024x768", "1280x720"].contains(&size) {
                assert!(first_place.is_some(), "{size} {look}: the console has its place beside the TV");
            }
            if open {
                assert!(ajar > GAMES * SYSTEMS.len() / 2, "{size} {look}: the cases stood open ({ajar} of {} keys)", lines.len());
            } else {
                assert_eq!(ajar, 0, "{size} {look}: no case stands open without a moment");
            }
        }
    }
    // How many a size and look, then the first few of each.
    let mut by_run: std::collections::BTreeMap<String, Vec<&String>> = Default::default();
    for f in &failures {
        by_run.entry(f.split(" key ").next().unwrap_or_default().to_string()).or_default().push(f);
    }
    let report: Vec<String> = by_run.iter().map(|(run, fs)| format!("{run}: {} frames, e.g.\n  {}", fs.len(), fs.iter().take(3).map(|f| f.as_str()).collect::<Vec<_>>().join("\n  "))).collect();
    assert!(failures.is_empty(), "{} of the walk's frames:\n{}", failures.len(), report.join("\n"));
}

#[path = "../src/test_clock.rs"]
mod test_clock;
