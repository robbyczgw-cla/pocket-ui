//! A phone game's (J2ME) first start on ROCKNIX needs Java, which is not on the image. Before, ROCKNIX's script loaded it
//! behind a black screen for over a minute and, offline, ended in a dialog nobody saw. Now pocket-ui loads it itself over
//! the start: the player sees "Loading Java" with its progress, B stops it, offline says so, and the game then starts.
//! Driven through the real A press and launch preparation (`--snapshot bench-launch-j2me`, `src/j2me_java.rs`).
use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

const TOP: &str = "zulu-test-linux_aarch64";

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

/// A snapshot opens no window, but winit's event loop wants a Wayland compositor to say hello first.
fn wayland_for_snapshots(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    // One socket per start: a second start in the same folder must not take the first stub's socket file for a listener.
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = tmp.join(format!("wayland-stub-{}", N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    let child = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()])
        .spawn()
        .expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

/// A library with one phone game, ROCKNIX's script as a stand-in, no Java yet, and a small archive shaped like Azul's.
fn handheld(tmp: &Path) -> PathBuf {
    let roms = tmp.join("roms/j2me");
    std::fs::create_dir_all(&roms).unwrap();
    std::fs::create_dir_all(tmp.join("data")).unwrap();
    std::fs::write(roms.join("Shooter.jar"), b"PK game").unwrap();
    std::fs::write(tmp.join("es_systems.cfg"), format!("<systemList><system><name>j2me</name><fullname>J2ME</fullname><path>{}</path><extension>.jar</extension><command>true</command><emulators><emulator name=\"retroarch\"><cores><core default=\"true\">freej2me</core></cores></emulator></emulators></system></systemList>", roms.display())).unwrap();
    let root = tmp.join("rocknix");
    std::fs::create_dir_all(root.join("storage")).unwrap();
    std::fs::write(root.join("freej2me.sh"), b"#!/bin/bash\n").unwrap();
    let src = tmp.join("jdk-src").join(TOP);
    for d in ["bin", "lib", "conf"] { std::fs::create_dir_all(src.join(d)).unwrap(); }
    std::fs::write(src.join("bin/java"), b"#!/bin/sh\n").unwrap();
    std::fs::write(src.join("lib/libjvm.so"), vec![7u8; 4096]).unwrap();
    assert!(Command::new("tar").arg("-czf").arg(tmp.join("jdk.tar.gz")).arg("-C").arg(tmp.join("jdk-src")).arg(TOP).status().unwrap().success());
    root
}

/// The start of the phone game, as a player presses A on it; `extra` env for the case. Returns (stdout line, picture).
fn start(tmp: &Path, mode: &str, extra: &[(&str, &str)]) -> (String, PathBuf) {
    let wayland = wayland_for_snapshots(tmp);
    let shot = tmp.join(format!("j2me{}.png", mode.replace(':', "-")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", &format!("bench-launch-j2me{mode}"), shot.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_STATES", tmp.join("states"))
        .env("POCKETUI_DATA", tmp.join("data"))
        .env("POCKETUI_RUNEMU", "/bin/true")
        .env("POCKETUI_J2ME_ROOT", tmp.join("rocknix"))
        .env("POCKETUI_J2ME_ARCHIVE", tmp.join("jdk.tar.gz"))
        .env("POCKETUI_J2ME_TOP", TOP)
        .env("POCKETUI_J2ME_STEP_MS", "120");
    for (k, v) in extra { cmd.env(k, v); }
    if let Some(w) = &wayland { cmd.env("WAYLAND_DISPLAY", &w.path); }
    let out = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    let line = stdout.lines().find(|l| l.starts_with("bench-j2me ")).unwrap_or_else(|| panic!("{stdout}")).to_string();
    // `J2ME_SHOTS=<folder>`: the half-way picture kept for a look (one per size and language asked for).
    if let (Some(keep), true) = (std::env::var_os("J2ME_SHOTS"), shot.is_file()) {
        let tag = extra.iter().map(|(k, v)| format!("{}{v}", k.trim_start_matches("POCKETUI_"))).collect::<Vec<_>>().join("-");
        std::fs::create_dir_all(&keep).unwrap();
        std::fs::copy(&shot, Path::new(&keep).join(format!("j2me{}-{tag}.png", mode.replace(':', "-")))).unwrap();
    }
    (line, shot)
}

fn storage(tmp: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(tmp.join("rocknix/storage")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn the_first_phone_game_shows_java_loading_then_starts_and_the_next_start_goes_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path();
    handheld(tmp);
    let (line, shot) = start(tmp, "", &[]);
    assert!(line.contains("shown=true"), "the load must be on the screen, not a black wait: {line}");
    assert!(line.contains("saved=true") && shot.is_file(), "{line}");
    assert!(line.contains("started=true") && line.ends_with("notice="), "the game starts once Java is there: {line}");
    assert!(tmp.join("rocknix/storage/jdk/bin/java").is_file(), "ROCKNIX's script finds the runtime where JAVA_HOME points");
    assert_eq!(storage(tmp), ["jdk"], "no download or unpack folder is left");
    // The second start: nothing to load, nothing said.
    let (line, _) = start(tmp, "", &[]);
    assert!(line.contains("shown=false") && line.contains("started=true"), "{line}");
}

/// The plate in every size the snapshots use and in German and Spanish (longer words), for the eye and for clipping.
#[test]
fn the_java_plate_fits_on_small_and_large_screens_in_every_language() {
    for size in ["640x480", "720x720", "1024x768", "1240x1080"] {
        for lang in ["en", "de", "es"] {
            let dir = tempfile::tempdir().unwrap();
            let tmp = dir.path();
            handheld(tmp);
            let (line, shot) = start(tmp, "", &[("POCKETUI_SNAPSHOT_SIZE", size), ("POCKETUI_LANG", lang)]);
            assert!(line.contains("shown=true") && shot.is_file(), "{size} {lang}: {line}");
        }
    }
}

#[test]
fn offline_a_phone_game_says_it_needs_wi_fi_once_instead_of_hanging() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path();
    handheld(tmp);
    let (line, _) = start(tmp, "", &[("POCKETUI_J2ME_OFFLINE", "1")]);
    assert!(line.contains("started=false"), "{line}");
    assert!(line.contains("Wi-Fi") && line.contains("200 MB"), "the notice says what to do: {line}");
    assert!(storage(tmp).is_empty());
}

#[test]
fn b_stops_the_java_load_and_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path();
    handheld(tmp);
    let (line, _) = start(tmp, ":stop", &[]);
    assert!(line.contains("started=false") && line.contains("Java wasn't loaded"), "{line}");
    assert!(storage(tmp).is_empty(), "a stopped load leaves no half runtime ROCKNIX's script would take for a whole one");
}

/// A phone game's keys are not a console's (15.11.2026, the controls card went): its card says in one line which buttons
/// are OK and the soft keys, on the Plus and a Retroid Pocket Mini, English and German, normal and large text, nothing
/// clipped.
#[test]
fn a_phone_games_card_says_which_buttons_are_its_keys() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path();
    handheld(tmp);
    let wayland = wayland_for_snapshots(tmp);
    for (device, size) in [("rg35xx-plus", "640x480"), ("miniv2", "1240x1080")] {
        for (lang, line) in [("en", "A is OK, SELECT and START are the soft keys."), ("de", "A ist OK, SELECT und START sind die Softtasten.")] {
            for text in ["normal", "large"] {
                let tag = format!("{device}-{lang}-{text}");
                let png = tmp.join(format!("card-{tag}.png"));
                let (boxes, seen) = (tmp.join(format!("boxes-{tag}")), tmp.join(format!("seen-{tag}")));
                let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                cmd.args(["--snapshot", "curator:card:j2me:shooter", png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1").env("POCKETUI_DEVICE", device)
                    .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg")).env("POCKETUI_DATA", tmp.join("data"))
                    .env("POCKETUI_SNAPSHOT_SIZE", size).env("POCKETUI_TEXT_SIZE", text).env("POCKETUI_LANG", lang)
                    .env("POCKETUI_TEXT_BOXES", &boxes).env("POCKETUI_VISIBLE_TEXT", &seen);
                if let Some(w) = &wayland { cmd.env("WAYLAND_DISPLAY", &w.path); }
                let out = cmd.output().unwrap();
                assert!(out.status.success(), "{tag}: {}", String::from_utf8_lossy(&out.stderr));
                let problems = std::fs::read_to_string(&boxes).unwrap_or_default();
                assert!(problems.trim().is_empty(), "{tag}: the card's text must be readable and separate: {problems}");
                let shown = std::fs::read_to_string(&seen).unwrap();
                let words = shown.split_whitespace().collect::<Vec<_>>().join(" ");
                assert!(words.contains(line), "{tag}: the card names the phone's keys: {shown}");
                if let Some(keep) = std::env::var_os("J2ME_SHOTS") {
                    std::fs::create_dir_all(&keep).unwrap();
                    std::fs::copy(&png, Path::new(&keep).join(format!("card-{tag}.png"))).unwrap();
                }
            }
        }
    }
}

/// A handheld without sticks: Runs on › D-pad offers the number keys for a phone game (and only for it).
#[test]
fn phone_games_can_put_the_number_keys_on_the_dpad() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path();
    handheld(tmp);
    let wayland = wayland_for_snapshots(tmp);
    let shoot = |state: &str, tag: &str| -> String {
        let png = tmp.join(format!("{tag}.png"));
        let seen = tmp.join(format!("seen-{tag}"));
        let boxes = tmp.join(format!("boxes-{tag}"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_DEVICE", "rg35xx-plus")
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg")).env("POCKETUI_DATA", tmp.join("data"))
            .env("POCKETUI_SNAPSHOT_SIZE", "640x480").env("POCKETUI_LANG", "en")
            .env("POCKETUI_TEXT_BOXES", &boxes).env("POCKETUI_VISIBLE_TEXT", &seen);
        if let Some(w) = &wayland { cmd.env("WAYLAND_DISPLAY", &w.path); }
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{tag}: {}", String::from_utf8_lossy(&out.stderr));
        if let Some(keep) = std::env::var_os("J2ME_SHOTS") {
            std::fs::create_dir_all(&keep).unwrap();
            std::fs::copy(&png, Path::new(&keep).join(format!("{tag}.png"))).unwrap();
        }
        std::fs::read_to_string(&seen).unwrap()
    };
    let page = shoot("emu:j2me:Shooter.jar:retroarch/freej2me:8", "runs-on-dpad");
    assert!(page.contains("D-pad") && page.contains("Arrow keys"), "the phone game's Runs on has the D-pad row: {page}");
    // Below it the game's screen size (`j2me_size`): Automatic unless one is picked, its explanation in full.
    let size = shoot("emu:j2me:Shooter.jar:retroarch/freej2me:9", "runs-on-size");
    assert!(size.contains("Screen size") && size.contains("Automatic"), "the phone game's Runs on has the screen size row: {size}");
    let snes = shoot("emu:j2me:Shooter.jar:retroarch/freej2me:0", "runs-on-top");
    assert!(!snes.contains("Number keys 2 4 6 8") || snes.contains("D-pad"), "{snes}");
}
