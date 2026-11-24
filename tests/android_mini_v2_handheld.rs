//! The handheld view on the Retroid Pocket Mini V2's Android (1240x1080, built-in controller): the Linux pages in Android's
//! profile (`POCKETUI_ANDROID_PROFILE=1`, the rows Android answers for left out), painted at the Mini V2's own panel. Every
//! page of `tests/layout-risk-states.txt` (narrow columns, long metadata, keyboards, bottom rows) plus the settings pages and
//! the game card, in English, German and Spanish, in the plain and the Retro style, at Normal and Large text: no word cut short,
//! no text off its room or into other text, and every page says something (never a blank screen).
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, for scripts/wayland-stub.py.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

include!("common/root.rs");

struct Stub(std::process::Child);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
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

/// The pages: the layout matrix's, and the ones a Mini V2 player opens first (the game card, Settings, Quick settings, About).
fn states() -> Vec<String> {
    let mut states: Vec<String> = std::fs::read_to_string(Path::new(&ROOT).join("tests/layout-risk-states.txt")).unwrap()
        .lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string)
        // Sync is one of the services the handheld view leaves out (`device::ANDROID_HIDES`): there is no such page to fit.
        .filter(|l| !l.starts_with("sync:"))
        .chain(["about", "extras", "save", "slot-save:gba", "controls", "shortcuts", "options", "picture", "info:shelf:gba:golden"].map(str::to_string)).collect();
    states.dedup();
    states
}

struct Job {
    state: String,
    large: bool,
    retro: bool,
    lang: &'static str,
}

/// One screen's record: the text-fit faults the program wrote, what was painted, the picture (when asked for).
fn run(jobs: Vec<Job>, print_shots: bool) -> Vec<String> {
    let tmp_guard = tempfile::Builder::new().prefix("pui-miniv2-").tempdir().unwrap();
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
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map(PathBuf::from).unwrap_or_else(|| env!("CARGO_BIN_EXE_pocket-ui").into());
    let work = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let failed = Arc::new(Mutex::new(Vec::<String>::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(4)))
        .map(|_| {
            let (work, failed, tmp, bin, socket) = (work.clone(), failed.clone(), tmp.clone(), bin.clone(), socket.clone());
            std::thread::spawn(move || loop {
                let Some((i, job)) = work.lock().unwrap().pop() else { break };
                let name = format!("{} / {} / {} / {}", job.state, job.lang, if job.retro { "retro" } else { "plain" }, if job.large { "large" } else { "normal" });
                let data = tmp.join(format!("data-{i}"));
                copy_dir(&tmp.join("data"), &data);
                let mut settings = serde_json::json!({"theme": "shelf", "onboarded": true, "motion": "off"});
                if job.retro {
                    settings["font"] = "retro".into();
                    settings["retro_letters"] = "departure".into();
                }
                std::fs::write(data.join("settings.json"), settings.to_string()).unwrap();
                let (png, boxes, seen) = (tmp.join(format!("{i}.png")), tmp.join(format!("{i}.boxes")), tmp.join(format!("{i}.txt")));
                let mut cmd = Command::new(&bin);
                if socket.exists() {
                    cmd.env("WAYLAND_DISPLAY", &socket).env_remove("SWAYSOCK");
                }
                let out = cmd.args(["--snapshot", &job.state, png.to_str().unwrap()])
                    .env("POCKETUI_ANDROID_PROFILE", "1").env("POCKETUI_SNAPSHOT_SIZE", "1240x1080").env("POCKETUI_SNAPSHOT_FROZEN", "1")
                    .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_LANG", job.lang)
                    .env("POCKETUI_TEXT_SIZE", if job.large { "large" } else { "normal" })
                    .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg")).env("POCKETUI_DATA", &data)
                    .env("POCKETUI_TEXT_BOXES", &boxes).env("POCKETUI_VISIBLE_TEXT", &seen)
                    .output().expect("run pocket-ui");
                let _ = std::fs::remove_dir_all(&data);
                if !out.status.success() {
                    failed.lock().unwrap().push(format!("{name}: did not render: {}", String::from_utf8_lossy(&out.stderr)));
                    continue;
                }
                if print_shots {
                    let file = name.replace([' ', '/', ':', '='], "_").replace("__", "_");
                    let hex: String = std::fs::read(&png).unwrap().iter().map(|b| format!("{b:02x}")).collect();
                    println!("SHOT {file} {hex}");
                }
                let mut bad: Vec<String> = std::fs::read_to_string(&boxes).unwrap_or_default().lines()
                    .filter(|l| l.starts_with(['E', 'O', 'V', 'W', 'T'])).map(str::to_string).collect();
                if std::fs::read_to_string(&seen).unwrap_or_default().trim().is_empty() {
                    bad.push("EMPTY: the page paints no text".into());
                }
                if !bad.is_empty() {
                    failed.lock().unwrap().push(format!("{name}: {}", bad.join("; ")));
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let mut failed = failed.lock().unwrap().clone();
    failed.sort();
    failed
}

#[test]
fn the_handheld_view_fits_the_retroid_mini_v2_in_every_language_style_and_text_size() {
    let quick = std::env::var_os("POCKETUI_TEST_QUICK").is_some();
    let mut jobs = Vec::new();
    for state in states() {
        for lang in if quick { &["de"][..] } else { &["en", "de", "es"][..] } {
            for retro in [false, true] {
                for large in [false, true] {
                    jobs.push(Job { state: state.clone(), large, retro, lang, });
                }
            }
        }
    }
    let failed = run(jobs, false);
    assert!(failed.is_empty(), "{} page(s) that do not fit the Mini V2 in the handheld view:\n{}", failed.len(), failed.join("\n"));
}

/// Not a check: `cargo test --test android_mini_v2_handheld handheld_view_shots -- --ignored --nocapture` prints the pages' pictures
/// (English and German, Retro style, Normal text) as `SHOT <name> <hex of the PNG>` lines, for looking at.
#[test]
#[ignore]
fn handheld_view_shots() {
    let pages = ["home", "home:3", "shelf:gba:shelf", "shelf:psx:split", "cabinet:ddrra", "info:split:gba:golden", "box:gba:advance:saves:end",
        "setup:first:device=mini", "settings", "settings:1:2", "sys:quick", "menu:pocket-core", "big-pause:retroarch", "slot-save:psx", "slot-load:snes", "about"];
    let jobs = pages.iter().flat_map(|state| ["en", "de"].map(|lang| Job { state: state.to_string(), large: false, retro: true, lang })).collect();
    let failed = run(jobs, true);
    println!("{} pages with faults:\n{}", failed.len(), failed.join("\n"));
}

/// Android's first start in the handheld view (18.11.2026): "Your games" with what the device has (a games folder of
/// another app, ARMSX2 installed, GameCube games without an app, Saturn games without a BIOS), and the same page on a
/// device with nothing found and no app. Every language, Normal and Large: no word cut short, nothing off its room, and the
/// page names the folder, the apps, the link and the BIOS. `FIRST_START_SHOTS=<dir>` keeps the pictures.
#[test]
fn the_first_start_s_your_games_fits_with_what_the_device_has_in_every_language() {
    let tmp_guard = tempfile::Builder::new().prefix("pui-fs-").tempdir().unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null()).status().expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    // The device's storage: ES-DE's ROMs (read already), an Emulation/roms folder of another app, no BIOS.
    let storage = tmp.join("storage");
    for f in ["emulated/0/ROMs/saturn/Nights.cue", "emulated/0/ROMs/gamecube/Wind Waker.rvz", "emulated/0/ROMs/ps2/Okami.iso",
        "emulated/0/Emulation/roms/snes/Chrono Trigger.sfc", "emulated/0/Emulation/roms/gba/Golden Sun.gba"] {
        let p = storage.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"game").unwrap();
    }
    std::fs::create_dir_all(tmp.join("empty/emulated/0")).unwrap();
    std::fs::write(tmp.join("phone.json"), r#"{"apps": {"ps2": [{"p": "com.armsx2", "l": "ARMSX2"}], "3ds": [{"p": "org.azahar_emu.azahar", "l": "Azahar"}], "xbox": [{"p": "com.rfandango.haku_x", "l": "hakuX"}]}}"#).unwrap();
    std::fs::write(tmp.join("nophone.json"), r#"{"apps": {}}"#).unwrap();
    // A device with nothing yet: no system list with games either.
    std::fs::write(tmp.join("empty.cfg"), "<?xml version=\"1.0\"?>\n<systemList>\n</systemList>\n").unwrap();
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
    for (case, store, phone, state, es) in [("found", "storage", "phone.json", "setup:first-games", "es_systems.cfg"),
        ("found-apps", "storage", "phone.json", "setup:first-games:ddd", "es_systems.cfg"), ("nothing", "empty", "nophone.json", "setup:first-games", "empty.cfg")] {
        for lang in ["en", "de", "es", "pt", "fr", "it"] {
            for large in [false, true] {
                i += 1;
                let data = tmp.join(format!("data-{i}"));
                copy_dir(&tmp.join("data"), &data);
                std::fs::write(data.join("settings.json"), r#"{"theme": "shelf", "onboarded": false, "motion": "off"}"#).unwrap();
                let (png, boxes, seen) = (tmp.join(format!("{i}.png")), tmp.join(format!("{i}.boxes")), tmp.join(format!("{i}.txt")));
                let mut cmd = Command::new(&bin);
                if socket.exists() {
                    cmd.env("WAYLAND_DISPLAY", &socket).env_remove("SWAYSOCK");
                }
                let out = cmd.args(["--snapshot", state, png.to_str().unwrap()])
                    .env("POCKETUI_ANDROID_PROFILE", "1").env("POCKETUI_SNAPSHOT_SIZE", "1240x1080").env("POCKETUI_SNAPSHOT_FROZEN", "1")
                    .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_LANG", lang)
                    .env("POCKETUI_TEXT_SIZE", if large { "large" } else { "normal" })
                    .env("POCKETUI_ES_SYSTEMS", tmp.join(es)).env("POCKETUI_DATA", &data)
                    .env("POCKETUI_FIRST_START_STORAGE", tmp.join(store)).env("POCKETUI_PHONE", tmp.join(phone))
                    .env("POCKETUI_TEXT_BOXES", &boxes).env("POCKETUI_VISIBLE_TEXT", &seen)
                    .output().expect("run pocket-ui");
                let name = format!("{case} {lang} {}", if large { "large" } else { "normal" });
                if !out.status.success() {
                    failed.push(format!("{name}: did not render: {}", String::from_utf8_lossy(&out.stderr)));
                    continue;
                }
                if let Some(dir) = &shots {
                    std::fs::create_dir_all(dir).unwrap();
                    std::fs::copy(&png, dir.join(format!("handheld-{}.png", name.replace(' ', "-")))).unwrap();
                }
                let bad: Vec<String> = std::fs::read_to_string(&boxes).unwrap_or_default().lines()
                    .filter(|l| l.starts_with(['E', 'O', 'V', 'W', 'T'])).map(str::to_string).collect();
                let said = std::fs::read_to_string(&seen).unwrap_or_default();
                if !bad.is_empty() {
                    failed.push(format!("{name}: {bad:?}"));
                }
                if said.trim().is_empty() {
                    failed.push(format!("{name}: the page paints no text"));
                }
                // A needed BIOS is said in every language and size, whatever row has the focus (the file names are the same in all).
                if case != "nothing" && !said.contains("saturn_bios.bin") {
                    failed.push(format!("{name}: Saturn's BIOS is not said: {said}"));
                }
                if lang == "en" {
                    let want: &[&str] = match case {
                        "found" => &["2 games in Emulation/roms", "Emulator apps", "Get Dolphin (GameCube / Wii)", "Saturn needs a BIOS", "PlayStation plays without a BIOS"],
                        // The whole note, never cut short ("…") however many apps there are.
                        "found-apps" => &["Found on this device: ", "ARMSX2 (PlayStation 2)", "per system later."],
                        _ => &["Add your games"],
                    };
                    for w in want {
                        if !said.contains(w) {
                            failed.push(format!("{name}: {w:?} not on the page: {said}"));
                        }
                    }
                    if case == "nothing" && (said.contains("Emulator apps") || said.contains("Get ") || said.contains("BIOS")) {
                        failed.push(format!("{name}: offers what isn't there: {said}"));
                    }
                }
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
