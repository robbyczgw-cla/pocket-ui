//! Small player-facing render journeys with a messy throwaway card, using the real application binary.
//! Assertions read the text recorded during rendering and the layout diagnostics, rather than private App fields.
//! No device, user library or external service is used.

#[path = "common/executable.rs"]
mod fixture_executable;

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

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
    test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

/// Two systems: Game Boy and Sega CD, one game each.
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
    ] {
        let folder = dir.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        for f in files {
            let body = if f.ends_with(".cue") {
                "FILE \"Pocket CD Demo (USA).bin\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n"
            } else {
                // Not empty: pocket-ui refuses to start a game file with nothing in it (`library_cache::empty_game`).
                "x"
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

/// One real application render, with a throwaway card and a transcript of painted text.
fn shot(tmp: &Path, stub: Option<&Stub>, state: &str, size: &str) -> String {
    let name = state.replace(':', "-");
    let data = tmp.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let transcript = tmp.join(format!("{name}.tsv"));
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(stub) = stub {
        cmd.env("WAYLAND_DISPLAY", &stub.path);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", size)
        .env("POCKETUI_VISIBLE_TEXT", &transcript)
        .env("POCKETUI_TEXT_BOXES", tmp.join(format!("{name}.boxes")))
        .env("POCKETUI_DATA", data)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .output()
        .unwrap();
    assert!(out.status.success() && png.exists(), "{state}: status={}, png={}\nstderr: {}\nstdout: {}", out.status, png.exists(), String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
    std::fs::read_to_string(transcript).expect("the render must record its text")
}

struct Card {
    tmp: PathBuf,
    stub: Option<Stub>,
    _dir: tempfile::TempDir,
}
impl Card {
    fn new(name: &str) -> Self {
        let dir = tempfile::Builder::new().prefix(&format!("pocketui-journey-{name}-")).tempdir().unwrap();
        let tmp = dir.path().to_path_buf();
        library(&tmp);
        // Junk beside the ROMs must never become games.
        std::fs::write(tmp.join("gb/README.txt"), "not a game").unwrap();
        std::fs::write(tmp.join("gb/._Ghost.gb"), "Apple metadata").unwrap();
        let stub = wayland(&tmp);
        Self { tmp, stub, _dir: dir }
    }
    fn shot(&self, state: &str, size: &str) -> String {
        shot(&self.tmp, self.stub.as_ref(), state, size)
    }
}
impl Drop for Card {
    fn drop(&mut self) {
        self.stub.take();
        // The TempDir field removes everything after the Wayland child is stopped, also on panic.
    }
}

/// A bench on the RG35XX Plus (14.11.2026) lit the owner's sleeping screen: every start, `--snapshot` too, turned a dark
/// backlight on and deleted its note (`Sleep::new` → `recover`). A snapshot or bench runs beside the real pocket-ui and
/// must leave the screen, its note and the device's sleep exactly as they were. Missed before because no snapshot test
/// had a backlight to touch: they all ran without one.
#[test]
fn a_snapshot_leaves_a_dark_screen_dark_and_its_note_in_place() {
    let card = Card::new("dark-screen");
    let light = card.tmp.join("backlight");
    std::fs::create_dir_all(&light).unwrap();
    std::fs::write(light.join("brightness"), "0").unwrap();
    std::fs::write(light.join("max_brightness"), "2047").unwrap();
    let note = card.tmp.join("brightness-note");
    std::fs::write(&note, "1124").unwrap();
    let png = card.tmp.join("home.png");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(stub) = card.stub.as_ref() {
        cmd.env("WAYLAND_DISPLAY", &stub.path);
    }
    let data = card.tmp.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let out = cmd
        .args(["--snapshot", "home", png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_DATA", &data)
        .env("POCKETUI_ES_SYSTEMS", card.tmp.join("es_systems.cfg"))
        .env("POCKETUI_BACKLIGHT", &light)
        .env("POCKETUI_DARK_FILE", &note)
        .output()
        .unwrap();
    assert!(out.status.success() && png.exists(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_to_string(light.join("brightness")).unwrap(), "0", "the screen stays dark: {}", String::from_utf8_lossy(&out.stdout));
    assert_eq!(std::fs::read_to_string(&note).ok().as_deref(), Some("1124"), "the dark screen's note stays");
}

/// Package 5 of native painting compared every state twice and found the Load page saying slot 1 in one run and 3 in the
/// next, the Cheats page one cheat or two: all snapshots kept their made-up states and cheat files in one shared
/// `pocketui-snapshot` folder, and four running side by side deleted each other's. Each snapshot has its own folder now,
/// gone when it is done. Missed before because the checks ran one snapshot at a time.
#[test]
fn snapshots_side_by_side_each_keep_their_own_made_up_saves() {
    let card = Card::new("snapshot-tmp");
    let tmpdir = card.tmp.join("tmpdir");
    std::fs::create_dir_all(&tmpdir).unwrap();
    let run = |i: usize, state: &str| {
        let data = card.tmp.join(format!("data-{i}"));
        std::fs::create_dir_all(&data).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(stub) = card.stub.as_ref() {
            cmd.env("WAYLAND_DISPLAY", &stub.path);
        }
        cmd.args(["--snapshot", state, card.tmp.join(format!("{i}.png")).to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", "en")
            .env("TMPDIR", &tmpdir)
            .env("POCKETUI_VISIBLE_TEXT", card.tmp.join(format!("{i}.txt")))
            .env("POCKETUI_DATA", &data)
            .env("POCKETUI_ES_SYSTEMS", card.tmp.join("es_systems.cfg"))
            .spawn()
            .unwrap()
    };
    let states = ["slot-empty", "slot-save", "cheats-more", "slot-undo"];
    let children: Vec<_> = (0..12).map(|i| run(i, states[i % states.len()])).collect();
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    for i in 0..12 {
        let text = std::fs::read_to_string(card.tmp.join(format!("{i}.txt"))).unwrap();
        let first = std::fs::read_to_string(card.tmp.join(format!("{}.txt", i % states.len()))).unwrap();
        assert_eq!(text, first, "{}: the same page every time", states[i % states.len()]);
    }
    let slot_empty = std::fs::read_to_string(card.tmp.join("0.txt")).unwrap();
    assert!(slot_empty.contains("No save in slot 4 yet"), "slot-empty is slot 4: {slot_empty}");
    let left: Vec<_> = std::fs::read_dir(&tmpdir).unwrap().flatten().map(|e| e.file_name()).filter(|n| n.to_string_lossy().starts_with("pocketui-snapshot")).collect();
    assert!(left.is_empty(), "each snapshot removes its own folder: {left:?}");
}

/// New-player walk 2026-11-02: the list of recent games had five names. The button, the screen and the pause menu's hint say
/// one: Recent games.
#[test]
fn the_list_of_recent_games_has_one_name() {
    let card = Card::new("recent-games");
    for size in ["normal", "large"] {
        let switcher = card.shot("switcher", size);
        assert!(switcher.contains("Recent games"), "{size}: {switcher}");
        assert!(!switcher.contains("Recently played") && !switcher.contains("Switch game"), "{size}: {switcher}");
        // The pause menu's SELECT hint is the first to go where the bar is full (Large text: START stays, `kit::top_hints`).
        let pause = card.shot("menu:pocket-core", size);
        assert!(size == "large" || pause.contains("Recent games"), "{size}: the pause menu's SELECT hint: {pause}");
        assert!(!pause.contains("Games\n") || pause.contains("Recent games"), "{size}: {pause}");
    }
}

/// New-player walk 2026-11-02: the pause menu had two ways to the quick settings (a row and START), and its Options pages
/// did not say where you were.
#[test]
fn the_pause_menu_has_one_way_to_the_quick_settings_and_its_options_pages_have_titles() {
    let card = Card::new("pause-options");
    for size in ["normal", "large"] {
        let main = card.shot("menu:pocket-core", size);
        assert_eq!(main.matches("Quick settings").count(), 1, "{size}: START's hint is the one way: {main}");
        for (state, title) in [
            ("options", "Options"),
            ("picture", "Options \u{203a} Picture"),
            ("extras", "Options \u{203a} Speed, rumble, fan"),
            ("shortcuts", "Options \u{203a} Shortcuts"),
        ] {
            let page = card.shot(state, size);
            assert!(page.lines().any(|l| l.trim() == title), "{size} {state}: the title {title:?}: {page}");
        }
    }
}

/// Taste pass 2026-11-04: Zoom is a niche help and sat right above Save & quit in every pocket-core game. The pause menu's main
/// page lists it only for a player who asked (Big text, or Settings → Playing → Zoom in the pause menu); everyone can
/// reach it under Options.
#[test]
fn zoom_is_under_options_and_on_the_main_page_only_for_whoever_asked() {
    let card = Card::new("zoom-place");
    let data = card.tmp.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let rows = |text: &str| text.lines().any(|l| l.trim() == "Zoom");
    // (settings.json, text size, whether the main page lists Zoom)
    for (json, size, listed) in [
        (r#"{"onboarded":true}"#, "normal", false),
        (r#"{"onboarded":true,"zoom_in_pause":true}"#, "normal", true),
        // Big text is Large text and a Motion that isn't Full: either half alone is not it.
        (r#"{"onboarded":true,"motion":"reduced"}"#, "normal", false),
        (r#"{"onboarded":true}"#, "large", false),
        (r#"{"onboarded":true,"motion":"reduced"}"#, "large", true),
    ] {
        std::fs::write(data.join("settings.json"), json).unwrap();
        let main = card.shot("menu:pocket-core", size);
        assert_eq!(rows(&main), listed, "{json} at {size}: the main page: {main}");
        let options = card.shot("options", size);
        assert!(rows(&options), "{json} at {size}: Zoom is always a row of Options: {options}");
    }
    // Its shortcut is a row of Options → Shortcuts, off until chosen.
    let shortcuts = card.shot("shortcuts", "normal");
    assert!(shortcuts.contains("Zoom") && shortcuts.contains("Off"), "{shortcuts}");
}

/// The first start's profile "Simple" was neither simple nor the same as skipping once changed later: it is The shelf, with a
/// line of what it gets, and from Help & tools it says what it sets back.
#[test]
fn the_default_profile_is_the_shelf_and_says_what_picking_it_later_does() {
    let card = Card::new("profile-shelf");
    for size in ["normal", "large"] {
        let first = card.shot("setup:play", size);
        assert!(first.contains("The shelf") && first.contains("Clean shelf, ready out of the box."), "{size}: {first}");
        assert!(!first.contains("Simple") && !first.contains("Resets every extra"), "{size}: the first start: nothing to set back yet: {first}");
        let later = card.shot("setup:play-later", size);
        assert!(later.contains("The shelf") && later.contains("Resets every extra") && later.contains("On start"), "{size}: {later}");
        assert!(!later.contains("Simple"), "{size}: {later}");
    }
}

#[test]
fn a_typo_still_finds_the_game() {
    let card = Card::new("typo");
    let text = card.shot("search:results:tobu tobu gril", "normal");
    assert!(text.contains("Tobu Tobu Girl"), "{text}");
}

/// The Remote access page's second explanation line wraps at Large text: as one line it ran off
/// the right edge (Mini V2, English; monkey run 2026-10-31 seed 102, TEXT E).
#[test]
fn remote_access_explains_itself_inside_the_screen_at_large_text() {
    let card = Card::new("remote-large");
    let text = card.shot("setup:remote", "large");
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(one_line.contains("until you turn it off or the Pocket restarts"), "{text}");
    let boxes = std::fs::read_to_string(card.tmp.join("setup-remote.boxes")).unwrap();
    assert!(boxes.is_empty(), "remote access text clipped or overlapping: {boxes}");
}

/// A notice card and the small toast at the same time: the toast goes below the card instead of
/// garbling its first line (monkey run 2026-10-30, TEXT O on nearly every long run).
#[test]
fn a_toast_under_a_notice_does_not_overlap_it() {
    let card = Card::new("notice-toast");
    let text = card.shot("notice:empty:toast", "large");
    assert!(text.contains("Added to Favourites"), "{text}");
    let boxes = std::fs::read_to_string(card.tmp.join("notice-empty-toast.boxes")).unwrap();
    assert!(boxes.is_empty(), "toast and notice overlap: {boxes}");
}

#[test]
fn missing_bios_names_the_file() {
    let card = Card::new("bios");
    let text = card.shot("notice:bios-segacd", "normal");
    assert!(text.contains("bios_CD_U.bin"), "a BIOS notice must name the missing file: {text}");
}

#[test]
fn large_text_keeps_search_readable() {
    let card = Card::new("large");
    let text = card.shot("search:results:tobu", "large");
    assert!(text.contains("Tobu Tobu Girl"), "{text}");
    let boxes = std::fs::read_to_string(card.tmp.join("search-results-tobu.boxes")).unwrap();
    assert!(boxes.is_empty(), "Large text overlaps or leaves its room: {boxes}");
}

/// Settings → Games → RomM says something plain in every state, never an empty panel: not paired, paired without a list
/// yet, a server with no game for this Pocket, each failure, and the pairing screen with its code.
#[test]
fn romm_says_what_is_going_on_in_every_state() {
    let card = Card::new("romm-states");
    for (state, wanted) in [
        ("romm", &["Not paired", "RomM is a self-hosted game server", "Not set"][..]),
        ("romm:paired", &["Paired as sam", "214 games in 9 systems", "Read 5 min ago", "12 more are for systems", "download on first launch"]),
        ("romm:nolist", &["The list of games hasn't been read yet."]),
        ("romm:empty", &["RomM has no games for the systems this Pocket plays.", "7 more are for systems"]),
        ("romm:error", &["Something went wrong", "Can't reach your RomM server. Check Wi-Fi and that the server is on."]),
        ("romm:signedout", &["RomM doesn't accept this Pocket any more"]),
        ("romm:old", &["older than 5.0", "Pair with a code"]),
        ("romm:code", &["M9JXERPY", "Waiting for approval", "Open the camera on your phone."]),
        ("romm:pairing", &["Asking RomM for a code"]),
        ("romm:unpair", &["Press A again to unpair", "Unlinks this server."]),
    ] {
        let text = card.shot(state, "normal");
        for w in wanted {
            assert!(text.contains(w), "{state}: {w:?} missing in\n{text}");
        }
        assert!(!text.contains("rmm_") && !text.contains("token:"), "{state}: no token on screen: {text}");
    }
}

#[test]
fn offline_tailscale_stays_calm_before_asking_for_credentials() {
    let card = Card::new("tailscale-offline");
    let text = card.shot("tailscale:offline-note", "normal");
    assert!(text.contains("No network yet"), "{text}");
    assert!(text.contains("Connect to Wi-Fi"), "{text}");
    assert!(!text.contains("Timed out") && !text.contains("login.tailscale.com"), "{text}");
}

#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_offline_achievements_stays_calm_before_asking_for_credentials() {
    let card = Card::new("achievements-offline");
    let text = card.shot("achievements:offline", "normal");
    assert!(text.contains("Connect to Wi-Fi first"), "{text}");
    assert!(!text.contains("Password") && !text.contains("Timed out"), "{text}");
}

#[cfg(target_os = "linux")]
mod fifo {
    use super::*;
    use std::io::Write;
    use std::os::fd::FromRawFd;
    use std::time::{Duration, Instant};

    #[test]
    fn a_bad_system_is_reported_on_home_even_with_continue_and_a_cached_library() {
        for cached in [false, true] {
            let mut ui = Driver::start(&format!("system-list-{cached}"), true, |tmp, _| {
                let cfg = tmp.join("es_systems.cfg");
                let xml = std::fs::read_to_string(&cfg).unwrap().replace("Game Boy</fullname>", "Game & Watch</fullname>")
                    .replace("</systemList>", "<system><name>broken</name><fullname>Broken</wrong></system></systemList>");
                std::fs::write(&cfg, xml).unwrap();
                std::fs::write(tmp.join("data/recents.json"), serde_json::to_vec(&serde_json::json!([
                    {"system":"gb", "name":"Tobu Tobu Girl", "path":tmp.join("gb/Tobu Tobu Girl (World).gb"), "last_played":pocket_ui::recents::now_secs(), "play_secs":50}
                ])).unwrap()).unwrap();
                let systems = pocket_ui::library::load(&cfg).unwrap();
                assert_eq!(systems.len(), 2, "both good systems stay");
                if cached {
                    pocket_ui::library_cache::write(&tmp.join("data"), &pocket_ui::library_cache::Key::now(&cfg).unwrap(), &systems).unwrap();
                }
            });
            let shown = ui.sees("Some systems couldn't be read.");
            assert!(shown.contains("Continue"), "the warning also reaches Home with Continue: {shown}");
            assert!(shown.lines().collect::<Vec<_>>().join(" ").contains("Game check says why."), "{shown}");
            ui.send("b\nstart");
            ui.wait(|d| {
                d.frame();
                std::fs::read_to_string(d.card.tmp.join("ui.status")).unwrap_or_default().contains("place=settings:")
            });
            // Both the 0.4.x categories and the newer tabbed Settings reach the same Game check.
            let groups = std::fs::read_to_string(ui.card.tmp.join("ui.status")).unwrap().contains("place=settings:Groups");
            ui.send(if groups { "down\ndown\ndown\ndown\na" } else { "r1\nr1\nr1\nr1\nr1" });
            ui.sees("Add games");
            ui.send("down\ndown\na");
            ui.sees("Folders that can't be read");
            ui.send("a");
            ui.sees("broken");
            ui.send("a");
            let detail = ui.sees("One system was left out");
            assert!(detail.contains("es_systems.cfg"), "the detail names the file to repair: {detail}");
        }
    }

    /// A card whose system list can't be read at all (never a gap on Home, never silence): games in the standard folders
    /// still show and a notice says why; with no standard folder the empty page says it, and Game check names the file.
    #[test]
    fn an_unreadable_system_list_is_said_on_home_and_the_standard_folders_still_show() {
        for (broken, standard_folders) in [("<systemList><system><name>gb", true), ("", true), ("<systemList></systemList>", false)] {
            let label = format!("{broken:?} standard={standard_folders}");
            let mut ui = Driver::start("system-list-gone", true, |tmp, cmd| {
                let cfg = tmp.join("es_systems.cfg");
                if broken.is_empty() { std::fs::remove_file(&cfg).unwrap(); } else { std::fs::write(&cfg, broken).unwrap(); }
                let roms = if standard_folders { tmp.to_path_buf() } else { tmp.join("no-standard-folders") };
                std::fs::create_dir_all(&roms).unwrap();
                cmd.env("POCKETUI_BUILT_IN_ROMS", roms);
            });
            let shown = ui.sees("can't be read");
            let flat = shown.lines().collect::<Vec<_>>().join(" ");
            assert!(flat.contains("using defaults") || flat.contains("System list can't be read"), "{label}: Home says the list is the problem: {shown}");
            if standard_folders {
                assert!(flat.contains("Game Boy") || flat.contains("Tobu Tobu Girl"), "{label}: the games in their standard folders are on the shelf: {shown}");
            } else {
                // The note is closed (the press goes on to Home's page: B is "Look around"); the empty page under it, the starter
                // pack's or the look's own, says it again in its own place.
                ui.send("b");
                let shown = ui.wait_frame(|t| !t.contains("Press A to close") && t.contains("using defaults"));
                assert!(shown.lines().collect::<Vec<_>>().join(" ").contains("Settings › Games › Game check"), "{label}: an empty Home names its cause: {shown}");
            }
            ui.send("b\nstart");
            ui.wait(|d| {
                d.frame();
                std::fs::read_to_string(d.card.tmp.join("ui.status")).unwrap_or_default().contains("place=settings:")
            });
            let groups = std::fs::read_to_string(ui.card.tmp.join("ui.status")).unwrap().contains("place=settings:Groups");
            ui.send(if groups { "down\ndown\ndown\ndown\na" } else { "r1\nr1\nr1\nr1\nr1" });
            ui.sees("Add games");
            ui.send("down\ndown\na");
            let report = ui.sees("Folders that can't be read");
            assert!(!report.contains("All good"), "{label}: Game check must not say All good: {report}");
            // The group lists the file; its detail says what happened and what to do.
            ui.send("a");
            ui.sees("es_systems.cfg");
            ui.send("a");
            let detail = ui.wait_frame(|t| t.contains("built-in list"));
            let flat = detail.lines().collect::<Vec<_>>().join(" ");
            assert!(flat.contains("fix this file"), "{label}: the detail says what to do: {detail}");
        }
    }

    /// A settings file that can't be read (no backup) is said at the start, not a silent reset.
    #[test]
    fn unreadable_settings_are_said_at_the_start() {
        let mut ui = Driver::start("settings-damaged", true, |tmp, _| std::fs::write(tmp.join("data/settings.json"), "{\"theme\": \"ca").unwrap());
        let shown = ui.sees("Couldn't read your settings file");
        assert!(shown.lines().collect::<Vec<_>>().join(" ").contains("loaded defaults"), "{shown}");
    }

    struct Driver {
        card: Card,
        child: std::process::Child,
        nav: Option<PathBuf>,
    }
    /// Writes an executable in a child, as testutil::write_executable does: another test's fork must not inherit a
    /// descriptor that can write it (ETXTBSY).
    fn executable(path: &Path, body: &str) {
        super::fixture_executable::publish(path, body).unwrap();
    }

    impl Driver {
        fn new(name: &str, onboarded: bool) -> Self {
            Self::start(name, onboarded, |_, _| {})
        }

        /// `new`, with `prepare` adding to the card and the command before the UI starts (programs on PATH).
        fn start(name: &str, onboarded: bool, prepare: impl FnOnce(&Path, &mut Command)) -> Self {
            let card = Card::new(name);
            let data = card.tmp.join("data");
            std::fs::create_dir_all(&data).unwrap();
            // An onboarded card has seen this version's "New in" cards (9.9.9: never newer than the build), so a version
            // bump does not put them over every journey.
            if onboarded {
                std::fs::write(data.join("settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off"}"#).unwrap();
            }
            executable(
                &card.tmp.join("run-game"),
                r#"#!/bin/sh
printf 'started\n' > "$0.started"
trap 'printf "quit\n" > "$0.quit"; exit 0' TERM INT
# Bounded (about 2 minutes): a stand-in can never outlive its test, whatever killed the test.
i=0
while [ "$i" -lt 2400 ]; do sleep 0.05; i=$((i+1)); done
"#,
            );
            std::fs::write(card.tmp.join("wakealarm"), "+3600").unwrap();
            let log = std::fs::File::create(card.tmp.join("ui.log")).unwrap();
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
            cmd.args(["--test-driver", card.tmp.to_str().unwrap()])
                .env("POCKETUI_MOCK", "1")
                .env("POCKETUI_LANG", "en")
                .env("POCKETUI_TEXT_SIZE", "normal")
                .env("POCKETUI_VISIBLE_TEXT", card.tmp.join("frame.txt"))
                .env("POCKETUI_DATA", data)
                .env("POCKETUI_ES_SYSTEMS", card.tmp.join("es_systems.cfg"))
                .env("POCKETUI_RTC_ALARM", card.tmp.join("wakealarm"))
                .env("POCKETUI_SUSPEND_CMD", "false")
                .env("POCKETUI_POWEROFF_CMD", format!("echo poweroff >> '{}'", card.tmp.join("power.log").display()))
                .env("POCKETUI_NO_AUTOSUSPEND", "1")
                .env("POCKETUI_NO_HISTORY", "1")
                .env_remove("SWAYSOCK")
                .stdout(log.try_clone().unwrap())
                .stderr(log);
            if let Some(stub) = &card.stub {
                cmd.env("WAYLAND_DISPLAY", &stub.path);
            }
            prepare(&card.tmp, &mut cmd);
            // A test that dies (or is killed by the gate) takes the UI with it, and the UI takes its game (`die_with_parent`).
            unsafe {
                use std::os::unix::process::CommandExt;
                cmd.pre_exec(|| if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) });
            }
            let nav = cmd.get_envs().find(|(key, _)| *key == "POCKETUI_NAV_PROBE").and_then(|(_, value)| value.map(PathBuf::from));
            let child = cmd.spawn().unwrap();
            let mut driver = Self { card, child, nav };
            driver.wait(|d| d.card.tmp.join("ui.in").exists());
            driver
        }
        fn wait(&mut self, mut condition: impl FnMut(&mut Self) -> bool) {
            // Polls every 5 ms, so a generous bound costs nothing when all is well; a gate under load needed more than 8 s.
            let until = Instant::now() + test_clock::timeout(Duration::from_secs(30));
            while Instant::now() < until {
                if condition(self) {
                    return;
                }
                if let Some(status) = self.child.try_wait().unwrap() {
                    panic!("UI exited ({status}): {}", self.log());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!(
                "journey did not finish; visible text:\n{}\nUI status:\n{}\nUI log:\n{}",
                std::fs::read_to_string(self.card.tmp.join("frame.txt")).unwrap_or_default(),
                std::fs::read_to_string(self.card.tmp.join("ui.status")).unwrap_or_else(|e| e.to_string()),
                self.log()
            );
        }
        fn log(&self) -> String {
            std::fs::read_to_string(self.card.tmp.join("ui.log")).unwrap_or_default()
        }
        fn send(&mut self, commands: &str) {
            let path = std::ffi::CString::new(self.card.tmp.join("ui.in").to_str().unwrap()).unwrap();
            let mut fd = -1;
            self.wait(|_| {
                // Nonblocking open: a failed UI must never leave the test waiting for a FIFO reader forever.
                fd = unsafe { libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
                fd >= 0
            });
            let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
            writeln!(file, "{commands}").unwrap();
        }
        fn frame(&mut self) -> String {
            let png = self.card.tmp.join("frame.png");
            let _ = std::fs::remove_file(&png);
            if let Some(nav) = &self.nav { let _ = std::fs::remove_file(nav); }
            self.send("frame");
            // The file exists while it is still being written: wait for the PNG's last chunk (IEND + its CRC).
            self.wait(|_| std::fs::read(&png).is_ok_and(|b| b.ends_with(&[b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82])));
            // nav_probe_done follows write_png: don't let that previous capture satisfy a later nav request.
            if let Some(nav) = self.nav.clone() {
                self.wait(|_| std::fs::read_to_string(&nav).is_ok_and(|s| s.ends_with("@end\n")));
            }
            std::fs::read_to_string(self.card.tmp.join("frame.txt")).unwrap()
        }
        /// The first frame whose words satisfy `ok`.
        fn wait_frame(&mut self, ok: impl Fn(&str) -> bool) -> String {
            let mut text = String::new();
            self.wait(|d| {
                text = d.frame();
                ok(&text)
            });
            text
        }
        fn sees(&mut self, expected: &str) -> String {
            let mut text = String::new();
            self.wait(|d| {
                text = d.frame();
                text.contains(expected)
            });
            text
        }
        /// The first start from its language page to "Your games" (`games`: the title it waits for): A, the Wi-Fi page,
        /// B, and B again on the time zone when it is asked. Each press waits for its page.
        /// The first start's Wi-Fi page left as a player without Wi-Fi leaves it: Down to its last row, Continue
        /// (offline), A. B goes back a stage there, as on every stage.
        fn wifi_continue(&mut self) {
            self.send("down\n".repeat(20).trim_end());
            self.wait(|d| d.frame().lines().any(|l| l == "Continue" || l == "Continue offline"));
            self.send("a");
        }
        fn first_start_to_games(&mut self, games: &str) {
            self.send("a");
            self.wait(|d| d.frame().lines().any(|l| l == "Wi-Fi"));
            self.wifi_continue();
            let mut page = String::new();
            self.wait(|d| {
                page = d.frame();
                page.contains(games) || page.contains("More in Settings › System › Time.") || page.contains("Several time zones here")
            });
            if page.contains("Several time zones here") {
                // A country's list has no "keep": back to Wi-Fi, and Continue goes on (asked once).
                self.send("b");
                self.wait(|d| d.frame().lines().any(|l| l == "Wi-Fi"));
                self.wifi_continue();
            } else if !page.contains(games) {
                // The time zone's card: its first row, Looks right or Keep UTC.
                self.send("a");
            }
            self.sees(games);
        }
        fn sticker_sheet(&mut self) {
            self.send("x");
            self.sees("Stickers");
            // With Echoes switched off and one version, the full card page's first row is Stickers.
            let mut focused = false;
            for _ in 0..30 {
                self.send("down");
                if self.frame().contains("Open") {
                    focused = true;
                    break;
                }
            }
            assert!(focused, "the card's Stickers row must be focusable");
            self.send("a");
            self.sees("Playing now");
        }
    }
    impl Drop for Driver {
        fn drop(&mut self) {
            // A mock game may have been left paused if an assertion failed. Its pid is named by the launch log.
            for line in self.log().lines() {
                if let Some(pid) = line.strip_prefix("test game pid ").and_then(|s| s.parse::<i32>().ok()) {
                    let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
                    let marker = self.card.tmp.to_str().unwrap().as_bytes();
                    if cmd.windows(marker.len()).any(|part| part == marker) {
                        unsafe {
                            libc::kill(-pid, libc::SIGKILL);
                        }
                    }
                }
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// A saved game must not appear over the shelf before the old-TV switch-on. Exercise the actual
    /// launch worker and rendered frame for TV and handheld systems, including Home's Continue entry. A handheld's game
    /// asks for its display switching on instead of the tube (`crt_on::LCD`).
    #[test]
    fn the_launch_picture_never_flashes_before_the_tv_switches_on() {
        for (system, continue_row, crt, reminder, preview) in [
            ("snes", false, true, false, true), ("gb", false, true, false, true),
            ("gba", false, true, false, true), ("psx", false, true, false, true),
            ("n64", false, true, false, true), ("snes", true, true, false, true),
            ("snes", false, false, false, true), ("snes", true, true, true, true),
            ("snes", true, true, true, false),
        ] {
            let mut ui = Driver::start("tv-launch-picture", true, |tmp, cmd| {
                let rom = tmp.join("Game.rom");
                std::fs::write(&rom, b"game").unwrap();
                std::fs::write(tmp.join("es_systems.cfg"), format!("<systemList><system><name>{system}</name><fullname>Fixture console</fullname><path>{}</path><extension>.rom</extension><command>true</command></system></systemList>", tmp.display())).unwrap();
                std::fs::write(tmp.join("host"), b"").unwrap();
                std::fs::write(tmp.join("data/settings.json"), format!(r#"{{"onboarded":true,"seen_version":"9.9.9","theme":"shelf","motion":"full","animations":"cozy","frames":"every","art":"off","where_was_i":true,"extras":{{"tv_switch_on":{crt}}}}}"#)).unwrap();
                let states = tmp.join("states").join(system);
                std::fs::create_dir_all(&states).unwrap();
                if !continue_row { std::fs::write(states.join("Game.pstate.auto"), b"moment").unwrap(); }
                // A colour absent from the shelf: a whole game picture of it is visible even during the fade.
                if preview {
                    let mut enc = png::Encoder::new(std::fs::File::create(states.join("Game.pstate.auto.png")).unwrap(), 8, 8);
                    enc.set_color(png::ColorType::Rgb);
                    enc.write_header().unwrap().write_image_data(&[0xff, 0x00, 0xff].repeat(64)).unwrap();
                }
                if continue_row {
                    let ago = if reminder { 23 * 86400 } else { 60 };
                    let recent = serde_json::json!([{"system":system,"name":"Game","path":rom,
                        "last_played":pocket_ui::recents::now_secs() - ago,"play_secs":7200}]);
                    std::fs::write(tmp.join("data/recents.json"), recent.to_string()).unwrap();
                }
                cmd.env("POCKETUI_HOST_BIN", tmp.join("host")).env("POCKETUI_STATES", tmp.join("states")).env("TMPDIR", tmp);
                executable(&tmp.join("run-game"), r#"#!/bin/sh
printf '%s' "${POCKET_LAUNCH_CRT-}" > "$0.crt"
printf 'started\n' > "$0.started"
sleep 30
"#);
            });
            if !continue_row {
                ui.sees("Fixture console");
                ui.send("a");
            }
            ui.sees("Game");
            // The selected case must finish turning before A can lift it into the console.
            std::thread::sleep(Duration::from_millis(500));
            ui.frame();
            if continue_row {
                // A moment arriving after the shelf read still takes Cozy/TV-on. A moment already known
                // to that read is a frozen game's direct start, with no TV-on to precede it.
                std::fs::write(ui.card.tmp.join("states").join(system).join("Game.pstate.auto"), b"moment").unwrap();
            }
            ui.send("a");
            ui.wait(|d| d.card.tmp.join("run-game.started").exists());
            // Let insertion and fade finish. The stand-in leaves pocket-ui visible, as a slow core would.
            std::thread::sleep(Duration::from_millis(1900));
            let text = ui.frame();
            let image = pocket_ui::image::load_png(&ui.card.tmp.join("frame.png")).unwrap();
            let magenta = image.px.iter().filter(|&&p| p & 0xffffff == 0xff00ff).count();
            let tv = std::fs::read_to_string(ui.card.tmp.join("run-game.crt")).unwrap();
            // A handheld's game switches its display on (no tube), a console's the old TV.
            let want = match (crt, matches!(system, "gb" | "gba")) { (false, _) => "", (true, true) => pocket_ui::crt_on::LCD, (true, false) => "1" };
            assert_eq!(tv, want, "{system}, Continue={continue_row}: wrong switch-on request: {tv:?}\n{}", ui.log());
            if !preview || (crt && !reminder) {
                assert_eq!(magenta, 0, "{system}, Continue={continue_row}: the game appeared on the shelf before TV-on");
            } else {
                assert!(magenta > image.px.len() / 4, "a preview still bridges starts without TV-on and backs the reminder: {magenta}");
            }
            assert_eq!(text.contains("Last played 3 weeks ago"), reminder && preview, "{text}");
            if reminder && !preview {
                ui.send("home");
                ui.frame();
                let status = std::fs::read_to_string(ui.card.tmp.join("ui.status")).unwrap();
                assert!(status.contains("game=paused"), "HOME must open the menu, not dismiss an invisible reminder: {status}");
            }
        }
    }

    /// A picture asked for through the FIFO and the status file agree at once: whoever reads the status right after the
    /// frame (a helper deciding what its next press means, `docs/DEVICE-TESTS.md`) never sees the page before the press.
    /// Was a flake of the launch-picture journey: HOME, frame, then `game=playing` for a menu already painted.
    #[test]
    fn the_status_file_already_says_what_the_frame_just_painted() {
        let mut ui = Driver::new("status-with-frame", true);
        ui.sees("Game Boy");
        ui.send("a\na");
        ui.wait(|d| d.card.tmp.join("run-game.started").exists());
        ui.wait(|d| std::fs::read_to_string(d.card.tmp.join("ui.status")).is_ok_and(|s| s.split('\t').any(|f| f == "game=playing")));
        for round in 0..40 {
            for (press, game) in [("home", "game=paused"), ("home", "game=playing")] {
                ui.send(press);
                ui.frame();
                let status = std::fs::read_to_string(ui.card.tmp.join("ui.status")).unwrap();
                assert!(status.split('\t').any(|f| f == game), "round {round}, after {press}: the frame shows it, the status says {status}");
            }
        }
    }

    /// A newer pocket-ui is said once on the free shelf (`POCKETUI_UPDATE_HINT` stands in for the signed list): A goes
    /// to Settings → System → Updates, B only closes it, and neither brings it back.
    fn update_ready(name: &str) -> Driver {
        Driver::start(name, true, |_, cmd| {
            cmd.env("POCKETUI_UPDATE_HINT", "99.0.0");
        })
    }

    #[test]
    fn the_frame_dump_holds_a_short_walk_frame_by_frame_on_a_fixed_60_fps_clock() {
        // POCKETUI_FRAME_DUMP (docs/dev/environment.md): videos of the real device, where a screen recorder keeps only
        // 13-56 of the 60 pictures a second. One picture per sixtieth on the UI's own fixed clock, however slowly this
        // machine paints, and the press written down with the frame it happened in.
        let mut d = Driver::start("frame-dump", true, |tmp, cmd| {
            // With its animations, as a player has them (the driver turns them off for still pictures).
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"full","art":"off"}"#).unwrap();
            std::fs::write(tmp.join("take.txt"), "# one step right, then the glide settles\n90 right\nend 150\n").unwrap();
            cmd.env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
                .env("POCKETUI_FRAME_DUMP", tmp.join("dump"))
                .env("POCKETUI_FRAME_DUMP_SCRIPT", tmp.join("take.txt"));
        });
        let dir = d.card.tmp.join("dump");
        let log = || std::fs::read_to_string(dir.join("events.log")).unwrap_or_default();
        d.wait(|_| log().lines().any(|l| l.starts_with("end ")));
        let events = log();
        assert!(events.lines().any(|l| l.starts_with("end 150 ")), "150 ticks: {events}");
        assert!(events.lines().any(|l| l == "90 script right") && events.lines().any(|l| l == "90 input Right down"), "the press at its frame: {events}");
        let frames = pocket_ui::frame_dump::read(&dir.join("frames.pfd")).unwrap();
        let ticks: Vec<u64> = frames.iter().map(|f| f.tick).collect();
        assert_eq!(ticks, (0..150).collect::<Vec<_>>(), "one frame per tick, none lost");
        for f in &frames {
            assert_eq!(f.t_ns, pocket_ui::frame_dump::tick_ns(f.tick), "the UI clock at frame {} is exactly that tick's", f.tick);
        }
        let (w, h) = (frames[0].w, frames[0].h);
        assert!(w > 0 && h > 0 && frames.iter().all(|f| (f.w, f.h) == (w, h) && f.pixels.len() == (w * h) as usize));
        let changed: Vec<usize> = (1..150).filter(|&k| frames[k].pixels != frames[k - 1].pixels).collect();
        eprintln!("frames that differ from the one before: {changed:?}");
        assert!(changed.iter().all(|&k| k < 60 || k >= 90), "nothing moves in the frames before the press: {changed:?}");
        // The cursor glides: several different pictures on the frames after the press, then it stands.
        let moving = changed.iter().filter(|&&k| k >= 90).count();
        assert!(moving >= 3, "the move is seen on {moving} frames: {changed:?}");
        assert!(frames[149].pixels == frames[148].pixels, "settled by the end: {changed:?}");
        assert!(frames[149].pixels != frames[89].pixels, "the selection moved");
        // The host script makes the video track of it: 150 pictures at 60 fps (raw here; ffmpeg isn't everywhere).
        let out = Command::new("python3").arg("-I").arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/frame-dump-decode.py")).arg(&dir).arg("--raw").output().unwrap();
        assert!(out.status.success(), "decode: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(out.stdout.len(), 150 * (w * h * 3) as usize, "{}", String::from_utf8_lossy(&out.stderr));
        let at = |k: usize| &out.stdout[k * (w * h * 3) as usize..(k + 1) * (w * h * 3) as usize];
        assert!(at(60) == at(89) && at(100) != at(90), "the same pictures as the dump");
        let p = frames[100].pixels[0];
        assert_eq!(&at(100)[..3], &[(p >> 16) as u8, (p >> 8) as u8, p as u8], "as rgb24");
    }

    #[test]
    fn without_the_frame_dump_no_dump_is_written() {
        let mut d = Driver::start("no-frame-dump", true, |_, _| {});
        d.send("right");
        d.frame();
        let mut found = Vec::new();
        let mut dirs = vec![d.card.tmp.clone()];
        while let Some(dir) = dirs.pop() {
            for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() { dirs.push(p) } else if p.file_name().is_some_and(|n| n == "frames.pfd" || n == "events.log") { found.push(p) }
            }
        }
        assert!(found.is_empty(), "no dump without POCKETUI_FRAME_DUMP: {found:?}");
    }

    #[test]
    fn a_newer_pocket_ui_is_said_once_and_a_goes_to_the_updates_page() {
        let mut d = update_ready("update-hint-a");
        let text = d.sees("pocket-ui 99.0.0 is ready");
        assert!(text.contains("Updates"), "the card names the way: {text}");
        d.send("a");
        let page = d.sees("Check again");
        assert!(!page.contains("is ready"), "the card is gone on the page: {page}");
        let said = d.card.tmp.join("data").join("update-hint.json");
        d.wait(|_| std::fs::read_to_string(&said).is_ok_and(|t| t.contains("99.0.0")));
        // Back to the shelf: not said again.
        for _ in 0..3 {
            d.send("b");
        }
        let shelf = d.frame();
        assert!(!shelf.contains("is ready") && !shelf.contains("Check again"), "{shelf}");
    }

    /// 17.11.2026 (Retroid Pocket Mini V2, android.6): the notice counted as said the moment it was put up, also behind
    /// Home, and Android's Recents ended the process before anyone saw it; that version was never offered again. Now a
    /// notice nobody closed comes back at the next start; once closed (B: later) it stays away, and a newer version than
    /// the one closed is a new notice.
    #[test]
    fn an_unclosed_notice_comes_back_and_a_newer_version_after_later_is_said_again() {
        let carry = |from: &Driver| std::fs::read_to_string(from.card.tmp.join("data").join("update-hint.json")).ok();
        let start = |name: &str, version: &'static str, memory: Option<String>| Driver::start(name, true, move |tmp, cmd| {
            if let Some(memory) = memory {
                std::fs::write(tmp.join("data").join("update-hint.json"), memory).unwrap();
            }
            cmd.env("POCKETUI_UPDATE_HINT", version);
        });
        // Put up, then the process ends (Recents, Android's own end) with the notice still up.
        let mut first = start("update-hint-unclosed", "99.0.0", None);
        first.sees("pocket-ui 99.0.0 is ready");
        first.frame();
        let memory = carry(&first);
        drop(first);
        assert!(!memory.as_deref().unwrap_or_default().contains("99.0.0"), "not said while nobody closed it: {memory:?}");
        // The next start says it again; B closes it, and that is remembered.
        let mut second = start("update-hint-again", "99.0.0", memory);
        second.sees("pocket-ui 99.0.0 is ready");
        second.send("b");
        let file = second.card.tmp.join("data").join("update-hint.json");
        second.wait(|_| std::fs::read_to_string(&file).is_ok_and(|t| t.contains("99.0.0")));
        let memory = carry(&second);
        drop(second);
        // A newer version than the one put off: a new notice.
        let mut third = start("update-hint-newer", "99.1.0", memory);
        let text = third.sees("pocket-ui 99.1.0 is ready");
        assert!(!text.contains("99.0.0 is ready"), "{text}");
    }

    #[test]
    fn update_status_explains_the_confirmation_and_consumed_notice() {
        let mut d = update_ready("update-status");
        d.sees("pocket-ui 99.0.0 is ready");
        let path = d.card.tmp.join("ui.status");
        d.wait(|_| std::fs::read_to_string(&path).is_ok_and(|s|
            pocket_ui::ui_status::parse(&s).iter().any(|(k, v)| k == "cover" && v.split(',').any(|c| c == "notice"))));
        let fields = || pocket_ui::ui_status::parse(&std::fs::read_to_string(&path).unwrap()).into_iter().collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(fields().get("update.notice").map(String::as_str), Some("shown"));
        assert_eq!(fields().get("update.notice.schedule").map(String::as_str), Some("waiting"));
        d.send("a");
        d.sees("Install pocket-ui 99.0.0");
        d.send("a");
        d.sees("Install now");
        d.wait(|_| fields().get("update").is_some_and(|v| v == "confirming"));
        let status = fields();
        assert_eq!(status.get("update.cancel").map(String::as_str), Some("unavailable"));
        assert_eq!(status.get("update.version").map(String::as_str), Some("99.0.0"));
        assert_eq!(status.get("update.notice.reason").map(String::as_str), Some("already-said"));
        d.send("b");
        d.sees("Check again");
        d.wait(|_| fields().get("update").is_some_and(|v| v == "idle"));
        assert!(!std::fs::read_to_string(path).unwrap().contains("mock.tar.gz"), "status contains no download address");
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_b_only_closes_the_newer_version_card() {
        let mut d = update_ready("update-hint-b");
        d.sees("pocket-ui 99.0.0 is ready");
        d.send("b");
        let shelf = d.frame();
        assert!(!shelf.contains("is ready") && !shelf.contains("Check again"), "B closes it and opens nothing: {shelf}");
        std::thread::sleep(Duration::from_millis(1500));
        assert!(!d.frame().contains("is ready"), "it does not come back");
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_a_version_that_was_said_before_is_not_said_again() {
        let mut d = Driver::start("update-hint-said", true, |tmp, cmd| {
            std::fs::write(tmp.join("data").join("update-hint.json"), r#"{"said":"99.0.0"}"#).unwrap();
            cmd.env("POCKETUI_UPDATE_HINT", "99.0.0");
        });
        std::thread::sleep(Duration::from_millis(1500));
        let shelf = d.frame();
        assert!(!shelf.contains("is ready"), "{shelf}");
    }

    /// 0.5 (the RG28XX and the R36S had no way to update without Wi-Fi): an update file on the card is said on the shelf at
    /// the start, and A goes to Updates and installs it at once (here the stand-in file is gone, so the install's own check
    /// says why, on the page); the file is remembered as said.
    #[test]
    fn an_update_file_on_the_card_is_said_once_and_a_installs_it() {
        let mut d = Driver::start("card-update-a", true, |_, cmd| {
            cmd.env("POCKETUI_CARD_UPDATE", "99.0.0");
        });
        let text = d.sees("Update to pocket-ui 99.0.0 found on the card");
        assert!(text.contains("Press A to install it"), "{text}");
        d.send("a");
        let page = d.sees("Copy it onto the card again");
        assert!(page.contains("Nothing was installed") && !page.contains("found on the card"), "the install ran and said why: {page}");
        let seen = d.card.tmp.join("data").join("card-update-seen");
        assert_eq!(std::fs::read_to_string(&seen).unwrap(), "mock|99.0.0");
        drop(d);
    }

    /// A file on the GAMES drive that isn't ours (here: signed by no one) is said on the shelf with its reason, through the
    /// real look, and stays where the player put it; once said (the same name, size and time), a start doesn't say it again.
    #[test]
    fn an_update_file_not_signed_by_us_is_explained_once_and_never_installed() {
        fn place(tmp: &Path, cmd: &mut Command, said: bool) {
            let games = tmp.join("games-external");
            let folder = games.join("pocket-ui-update-99.0.0-rocknix");
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("update.json"), br#"{"format":1,"app":{"version":"99.0.0","url":"pocket-ui-99.0.0-aarch64.tar.gz","sha256":"00","size":1}}"#).unwrap();
            std::fs::write(folder.join("update.json.sig"), "AAAA").unwrap();
            if said {
                let mtime = std::fs::metadata(&folder).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                std::fs::write(tmp.join("data/card-update-seen"), format!("{}|0|{mtime}", folder.display())).unwrap();
            }
            cmd.env("POCKETUI_CARD_PLACES", games);
        }
        let mut d = Driver::start("card-update-unsigned", true, |tmp, cmd| place(tmp, cmd, false));
        let text = d.sees("isn't signed by us");
        assert!(text.contains("Nothing was installed"), "{text}");
        assert!(d.card.tmp.join("games-external/pocket-ui-update-99.0.0-rocknix/update.json").exists(), "a refused file stays");
        drop(d);
        let mut again = Driver::start("card-update-unsigned-said", true, |tmp, cmd| place(tmp, cmd, true));
        std::thread::sleep(Duration::from_millis(2500));
        let shelf = again.frame();
        assert!(!shelf.is_empty() && !shelf.contains("isn't signed by us"), "said once: {shelf}");
    }

    #[test]
    fn r36s_fix_a_updates_open_searches_the_card_with_no_wifi_or_after_an_offline_check() {
        for (device, online_check) in [("r36s", false), ("rg35xx-plus", true)] {
            let mut d = Driver::start(device, true, |tmp, cmd| {
                let programs = tmp.join("programs");
                std::fs::create_dir_all(&programs).unwrap();
                executable(&programs.join("curl"), "#!/bin/sh\nprintf checked > \"$0.called\"\nexit 7\n");
                executable(&programs.join("ip"), "#!/bin/sh\nexit 0\n");
                cmd.env("POCKETUI_DEVICE", device)
                    .env("POCKETUI_CARD_PLACES", tmp.join("update-card"))
                    .env("PATH", format!("{}:{}", programs.display(), std::env::var("PATH").unwrap_or_default()));
            });
            d.sees("Game Boy");
            d.send("start");
            d.wait(|d| {
                d.frame();
                status_of(d).contains("place=settings:")
            });
            let groups = status_of(&d).contains("place=settings:Groups");
            d.send(if groups { "down\ndown\ndown\ndown\ndown\na" } else { "l1\nl1" });
            d.sees("Updates");
            let folder = d.card.tmp.join("update-card/pocket-ui-update-99.0.0-rocknix");
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("update.json"), br#"{"format":1}"#).unwrap();
            std::fs::write(folder.join("update.json.sig"), "AAAA").unwrap();
            d.send("a");
            let page = d.sees("isn't signed by us");
            assert!(page.contains("Look on the card"), "{device}: {page}");
            assert_eq!(d.card.tmp.join("programs/curl.called").exists(), online_check, "{device}");
            assert_eq!(page.contains("Check again"), online_check, "{device}: {page}");
            assert_eq!(page.contains("Check for updates automatically"), online_check, "{device}: {page}");
        }
    }

    #[test]
    fn opening_updates_checks_an_old_offer_even_with_background_checks_off() {
        let mut d = Driver::start("updates-old-offer", true, |tmp, cmd| {
            std::fs::write(tmp.join("data/update-hint.json"), r#"{"off":true}"#).unwrap();
            let programs = tmp.join("programs");
            std::fs::create_dir_all(&programs).unwrap();
            executable(&programs.join("curl"), "#!/bin/sh\nprintf checked > \"$0.called\"\nfor i in $(seq 1 500); do [ ! -f \"$0.release\" ] || break; sleep 0.02; done\nexit 7\n");
            executable(&programs.join("ip"), "#!/bin/sh\nexit 0\n");
            cmd.env("POCKETUI_UPDATE_HINT", "98.0.0")
                .env("POCKETUI_CARD_PLACES", tmp.join("update-card"))
                .env("PATH", format!("{}:{}", programs.display(), std::env::var("PATH").unwrap_or_default()));
        });
        d.sees("Game Boy");
        d.send("start");
        d.wait(|d| {
            d.frame();
            status_of(d).contains("place=settings:")
        });
        let groups = status_of(&d).contains("place=settings:Groups");
        d.send(if groups { "down\ndown\ndown\ndown\ndown\na" } else { "l1\nl1" });
        d.sees("Updates");
        d.send("a");
        d.sees("Checking for updates…");
        d.wait(|d| d.card.tmp.join("programs/curl.called").exists());
        std::fs::write(d.card.tmp.join("programs/curl.release"), "").unwrap();
        d.sees("No update file on the card");
    }

    /// 0.3.1: the notice's A lands on the page with the new version and its notes; "Install" asks first ("Install now" /
    /// "Later"), B and Later go back to the page, and nothing is installed by a single press.
    #[test]
    fn the_updates_page_asks_before_it_installs() {
        let mut d = update_ready("update-ask");
        d.sees("pocket-ui 99.0.0 is ready");
        d.send("a");
        let page = d.sees("Install pocket-ui 99.0.0");
        assert!(page.contains("Mock notes") && page.contains("Check for updates automatically"), "the version's notes and the switch are on the page: {page}");
        d.send("a");
        let ask = d.sees("Install now");
        assert!(ask.contains("Later") && !ask.contains("Check again"), "the question, nothing else to choose: {ask}");
        // B only says "Later": the page again, nothing started.
        d.send("b");
        let back = d.sees("Check again");
        assert!(back.contains("Install pocket-ui 99.0.0") && !back.contains("Installing"), "{back}");
        // And so does choosing Later.
        d.send("a");
        d.sees("Install now");
        d.send("down");
        d.send("a");
        let later = d.sees("Check again");
        assert!(!later.contains("Installing") && later.contains("Install pocket-ui 99.0.0"), "{later}");
    }

    /// 0.3.1: Updates has a switch, On by default, that turns the look off and is still off after a restart.
    #[test]
    fn the_automatic_check_is_a_switch_on_the_updates_page_and_stays_off() {
        let mut d = update_ready("update-switch");
        d.sees("pocket-ui 99.0.0 is ready");
        d.send("a");
        let page = d.sees("Check for updates automatically");
        assert!(page.lines().any(|l| l == "On"), "On by default: {page}");
        // Install, Check again, the switch.
        d.send("down\ndown");
        d.send("a");
        let off = d.wait_frame(|t| t.lines().any(|l| l == "Off"));
        assert!(off.contains("Check for updates automatically"), "{off}");
        let file = d.card.tmp.join("data").join("update-hint.json");
        d.wait(|_| std::fs::read_to_string(&file).is_ok_and(|t| t.split_whitespace().collect::<String>().contains("\"off\":true")));
        // Right turns it back on, the file forgets the Off.
        d.send("right");
        d.wait_frame(|t| t.lines().any(|l| l == "On"));
        d.wait(|_| std::fs::read_to_string(&file).is_ok_and(|t| !t.contains("\"off\"")));
    }

    /// 0.3.1: switched Off, a newer version is not said at all (no request, no card).
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_off_says_nothing_about_a_newer_version() {
        let mut d = Driver::start("update-hint-off", true, |tmp, cmd| {
            std::fs::write(tmp.join("data").join("update-hint.json"), r#"{"off":true}"#).unwrap();
            cmd.env("POCKETUI_UPDATE_HINT", "99.0.0");
        });
        std::thread::sleep(Duration::from_millis(1500));
        let shelf = d.frame();
        assert!(!shelf.contains("is ready"), "{shelf}");
    }

    #[test]
    fn headless_journeys_show_the_players_saved_light_appearance() {
        // A minute boundary can change the status clock while the two real UI processes start.
        let clockless = |t: String| t.lines().filter(|l| !(l.len() == 5 && l.as_bytes()[2] == b':')).collect::<Vec<_>>().join("\n");
        let brightness = |d: &mut Driver| {
            d.frame();
            let mut reader = png::Decoder::new(std::fs::File::open(d.card.tmp.join("frame.png")).unwrap()).read_info().unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut pixels).unwrap();
            pixels[..info.buffer_size()].iter().map(|p| u64::from(*p)).sum::<u64>() / info.buffer_size() as u64
        };
        let mut dark = Driver::start("appearance-dark", true, |tmp, _| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"split","motion":"off","appearance":"dark"}"#).unwrap();
        });
        let dark_words = clockless(dark.frame());
        let dark_brightness = brightness(&mut dark);
        let mut light = Driver::start("appearance-light", true, |tmp, _| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"split","motion":"off","appearance":"light"}"#).unwrap();
        });
        assert_eq!(dark_words, clockless(light.frame()), "the same game and controls remain visible in either appearance");
        assert!(brightness(&mut light) > dark_brightness + 100, "the player's Light setting must paint a light page");
        // Look, the second category (R1 from Connections, where START opens); its second row, Appearance (after Layout); B out.
        dark.send("start\nr1\ndown\na\nb");
        assert_eq!(dark_words, clockless(dark.frame()), "changing Appearance comes back to the same game");
        assert!(brightness(&mut dark) > dark_brightness + 100, "a Light choice in Settings takes effect in the next capture too");
    }

    /// 0.4 took out the Minimal and Cards looks. A player who had one starts in Split (not the Shelf, whose slow launch
    /// is the opposite of their choice), is told once, and the file says "split" from then on.
    #[test]
    fn a_retired_look_starts_in_split_and_says_so_once() {
        for old in ["minimal", "cards"] {
            let mut d = Driver::start(&format!("retired-look-{old}"), true, |tmp, _| {
                std::fs::write(tmp.join("data/settings.json"), format!(r#"{{"onboarded":true,"seen_version":"9.9.9","theme":"{old}","motion":"off","art":"off"}}"#)).unwrap();
            });
            let first = d.frame();
            assert!(first.contains("Minimal and Cards were retired"), "{old}: the start says what became of the look\n{first}\n{}", std::fs::read_to_string(d.card.tmp.join("ui.log")).unwrap_or_default());
            let saved = std::fs::read_to_string(d.card.tmp.join("data/settings.json")).unwrap();
            assert!(saved.contains(r#""theme": "split""#) && saved.contains(r#""onboarded": true"#), "{old}: saved at once, the rest kept\n{saved}");
        }
    }

    /// Settings → Playing → Console boot screen with the D-pad and A: Off for everyone who never touched it, A switches it On (saved
    /// for pocket-core, which reads settings.json itself) and the row's line says what it needs; A again is Off and nothing is saved.
    #[test]
    fn console_boot_screen_is_a_playing_row_that_is_off_and_saved_with_a_press() {
        let mut d = Driver::start("boot-screen-row", true, |_, _| {});
        // Playing is the fifth category (R1 four times from Connections), Console boot screen its third row.
        d.send("start\nr1\nr1\nr1\nr1\ndown\ndown");
        let row = d.frame();
        assert!(row.contains("Console boot screen"), "{row}");
        let saved = |d: &Driver| {
            let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.card.tmp.join("data/settings.json")).unwrap_or_else(|_| "{}".into())).unwrap();
            s["boot_screen"].as_bool().unwrap_or(false)
        };
        assert!(!saved(&d), "Off until switched on");
        d.send("a");
        let on = d.frame();
        assert!(saved(&d), "A switches it On and saves it\n{on}");
        assert!(on.contains("BIOS"), "On says it needs the player's BIOS file\n{on}");
        d.send("a");
        d.frame();
        assert!(!saved(&d), "A again: Off, nothing saved");
    }

    /// Settings → Playing → Over the game with the D-pad and A: Off, FPS, FPS + sensors and round again for everyone;
    /// Pixel Path comes after FPS + sensors only with the pause menu's Pixel Path on (the Enthusiast profile's).
    #[test]
    fn over_the_game_offers_pixel_path_only_with_the_pause_menus_pixel_path() {
        for pixel_path in [false, true] {
            let mut d = Driver::start(&format!("over-game-{pixel_path}"), true, |tmp, _| {
                if pixel_path {
                    std::fs::write(tmp.join("data/curator.json"), r#"{"pixel_path":true}"#).unwrap();
                }
            });
            // Playing is the fifth category (R1 four times from Connections), Over the game its sixth row.
            d.send("start\nr1\nr1\nr1\nr1\ndown\ndown\ndown\ndown\ndown");
            let row = d.frame();
            assert!(row.contains("Over the game"), "{row}");
            let saved = |d: &Driver| {
                let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.card.tmp.join("data/settings.json")).unwrap()).unwrap();
                s["over_game"].as_str().unwrap_or("off").to_string()
            };
            let mut seen = Vec::new();
            for _ in 0..3 {
                d.send("a");
                let words = d.frame();
                seen.push(saved(&d));
                assert!(words.contains("Pixel Path") == (seen.last().unwrap() == "pixel"), "{pixel_path}: the row says what is set\n{words}");
            }
            let want = if pixel_path { ["fps", "sensors", "pixel"] } else { ["fps", "sensors", "off"] };
            assert_eq!(seen, want, "pixel path in the pause menu: {pixel_path}");
        }
    }

    fn new_version_over_shelf(look: &str, cover: bool) -> Driver {
        Driver::start(&format!("tour-idle-{look}"), true, |tmp, _| {
            let name = "A very long game title that keeps scrolling all the way across the screen while the new version cards appear";
            let rom = tmp.join("gb").join(format!("{name}.gb"));
            std::fs::write(&rom, "game").unwrap();
            std::fs::write(tmp.join("data/settings.json"), serde_json::to_vec(&serde_json::json!({
                "onboarded":true, "seen_version":"0.1.0", "theme":look, "motion":"off",
                "art_off":!cover, "details_page":cover
            })).unwrap()).unwrap();
            if cover {
                let file = pocket_ui::covers::cover_path(&tmp.join("data"), "gb", &rom);
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                let mut enc = png::Encoder::new(std::fs::File::create(file).unwrap(), 1, 1);
                enc.set_color(png::ColorType::Rgb);
                enc.set_depth(png::BitDepth::Eight);
                enc.write_header().unwrap().write_image_data(&[235, 25, 15]).unwrap();
            }
            std::fs::write(tmp.join("data/recents.json"), serde_json::to_vec(&serde_json::json!([
                {"system":"gb", "name":name, "path":rom, "last_played":pocket_ui::recents::now_secs(), "play_secs":50}
            ])).unwrap()).unwrap();
            // Hold the new-version cards until the shelf has painted its long selected title.
            std::fs::write(tmp.join("data/crash.marker"), "Review fixture: dismiss this notice.").unwrap();
        })
    }

    fn read_new_cards_without_refreshing_the_hidden_shelf(mut ui: Driver, shelf_text: &str) {
        assert!(ui.sees("Review fixture").contains(shelf_text));
        ui.send("a");
        // Let the ordinary idle turn open the cards before requesting another capture of the shelf.
        ui.wait(|d| d.log().contains("new in "));
        ui.sees("New in pocket-ui");
        std::thread::sleep(Duration::from_millis(1600));
        let switches = || {
            std::fs::read_to_string(format!("/proc/{}/status", ui.child.id())).unwrap().lines()
                .find_map(|l| l.strip_prefix("voluntary_ctxt_switches:").and_then(|n| n.trim().parse::<u64>().ok())).unwrap()
        };
        let before = switches();
        std::thread::sleep(Duration::from_secs(1));
        let wakes = switches() - before;
        assert!(wakes < 20, "reading a still card must release hidden shelf timers; {wakes} main-thread wakeups/s");
        ui.send("b");
        assert!(ui.frame().contains(shelf_text), "closing the cards restores the shelf");
    }

    #[test]
    fn reading_new_version_cards_stops_the_hidden_games_title_timer() {
        read_new_cards_without_refreshing_the_hidden_shelf(new_version_over_shelf("split", false), "A very long game title");
    }

    #[test]
    fn reading_new_version_cards_stops_pending_shelf_art_timers() {
        for look in ["shelf", "cabinet"] {
            read_new_cards_without_refreshing_the_hidden_shelf(new_version_over_shelf(look, true), "Game Boy");
        }
    }

    /// A Pocket that had 0.2.2 and now runs 0.3.0 (`POCKETUI_NEW_IN`, mock only), with a game on the shelf.
    fn updated_to_0_3(name: &str) -> Driver {
        Driver::start(name, true, |tmp, cmd| {
            std::fs::write(tmp.join("gb/Tobu Two.gb"), "game").unwrap();
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"0.2.2","motion":"off","art":"off"}"#).unwrap();
            cmd.env("POCKETUI_NEW_IN", "0.3.0");
        })
    }

    fn settings_json(d: &Driver) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(d.card.tmp.join("data/settings.json")).unwrap()).unwrap()
    }

    #[test]
    fn after_the_update_to_0_3_one_press_tries_retro_and_b_keeps_the_style() {
        let mut d = updated_to_0_3("new-retro-a");
        let card = d.sees("Try Retro");
        assert!(card.contains("New in pocket-ui 0.3.0"), "{card}");
        d.send("a");
        d.wait(|d| settings_json(d)["font"] == "retro");
        let shelf = d.frame();
        assert!(!shelf.contains("Try Retro") && !shelf.contains("New in pocket-ui"), "one press and the card is gone: {shelf}");
        drop(d);

        let mut d = updated_to_0_3("new-retro-b");
        d.sees("Try Retro");
        d.send("b");
        d.wait(|d| !d.frame().contains("New in pocket-ui"));
        let s = settings_json(&d);
        assert_eq!(s["seen_version"], "0.3.0", "the card is said once");
        assert_ne!(s["font"], "retro", "B leaves the style as it was");
    }

    /// One rule across every look, through the real launch, output writer and exit save.
    #[test]
    fn game_start_rule_every_look_reaches_audio_pause_save_and_continue() { game_start_rule(false); }

    #[test]
    fn dsperate_rule_reaches_our_pause_menu_save_quit_and_continue() { game_start_rule(true); }

    fn game_start_rule(ds: bool) {
        let started = Instant::now();
        let (system, title, file, core) = if ds { ("nds", "Nintendo DS", "DS Demo", "dsperate") }
            else { ("gb", "Game Boy", "Tobu Tobu Girl (World)", "gambatte") };
        let game_title = if ds { "DS Demo" } else { "Tobu Tobu Girl" };
        let runtime = tempfile::Builder::new().prefix("pui-player-").tempdir().unwrap();
        let r = runtime.path();
        std::fs::set_permissions(r, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        let display_log = r.join("compositor.log");
        let log = std::fs::File::create(&display_log).unwrap();
        let mut compositor = Stub { path: r.join("wayland-test"), child: Command::new("python3")
            .args(["-I", &format!("{ROOT}/scripts/wayland-player.py")]).arg(r.join("wayland-test"))
            .stdout(log.try_clone().unwrap()).stderr(log).spawn().expect("python3 for real player journeys") };
        test_clock::wait_for(|| compositor.path.exists() || compositor.child.try_wait().unwrap().is_some(), Duration::from_secs(5), "software EGL compositor");
        assert!(compositor.path.exists() && compositor.child.try_wait().unwrap().is_none(), "{}", std::fs::read_to_string(&display_log).unwrap());
        let build = |source: &Path, output: &Path| {
            let out = Command::new("cc").args(["-shared", "-fPIC", "-O1", "-DFPS=60", "-DRATE=48000", "-o"]).arg(output).arg(source).arg("-lm").output().unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        };
        let source=if ds { "tests/fixtures/dsperate_stub.c" } else { "scripts/crash-sweep/stub-core.c" };
        build(&PathBuf::from(format!("{ROOT}/{source}")), &r.join(format!("{core}_libretro.so")));
        build(&PathBuf::from(format!("{ROOT}/tests/fixtures/audio_sink.c")), &r.join("libpulse-simple.so.0"));
        for look in pocket_ui::settings::Theme::ALL {
            if ds && !matches!(look,pocket_ui::settings::Theme::Shelf|pocket_ui::settings::Theme::Big) { continue; }
            let look = serde_json::to_value(look).unwrap().as_str().unwrap().to_string();
            let mut d = Driver::start(&format!("real-{look}"), true, |tmp, cmd| {
                std::fs::write(tmp.join("data/settings.json"), format!(r#"{{"onboarded":true,"seen_version":"9.9.9","theme":"{look}","motion":"full","art":"off"}}"#)).unwrap();
                std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false}"#).unwrap();
                if ds {
                    let folder=tmp.join("nds");std::fs::create_dir(&folder).unwrap();
                    std::fs::write(folder.join("DS Demo.nds"),b"rule-test ROM").unwrap();
                    std::fs::write(tmp.join("es_systems.cfg"),format!("<systemList><system><name>nds</name><fullname>Nintendo DS</fullname><path>{}</path><extension>.nds</extension><command>true</command></system></systemList>",folder.display())).unwrap();
                    std::fs::write(tmp.join("data/controls-card-seen"),"nds\n").unwrap();
                    cmd.env("POCKETUI_DEVICE","rg35xx-plus");
                    cmd.env("POCKET_TEST_DS_RESUMED",tmp.join("ds-resumed"));
                }
                cmd.env("POCKETUI_TEST_REAL_CORE", "1").env("POCKET_CORE_TEST", "1").env("TMPDIR", tmp).env("POCKETUI_HOST_BIN", env!("CARGO_BIN_EXE_pocket-core"))
                    .env("POCKETUI_CORES", r).env("POCKETUI_STATES", tmp.join("states"))
                    .env("POCKETUI_NAV_PROBE", tmp.join("nav.txt"))
                    .env("POCKETUI_TRACE", tmp.join("trace.txt"))
                    .env("WAYLAND_DISPLAY", &compositor.path).env("XDG_RUNTIME_DIR", r)
                    .env("LIBGL_ALWAYS_SOFTWARE", "1").env("vblank_mode", "0").env("LD_LIBRARY_PATH", r)
                    .env("POCKET_CORE_AUDIO", "pulse").env("POCKET_TEST_AUDIO_OUT", tmp.join("audio.out"));
            });
            if look != "big" { d.sees(title); d.send("a"); }
            d.sees(game_title);
            let launch = Instant::now();
            d.send("a");
            // Live-appended audio.out may briefly end in a partial line; ignore incomplete lines while polling.
            d.wait(|d| std::fs::read_to_string(d.card.tmp.join("audio.out")).unwrap_or_default().lines().any(|l| l.split_whitespace().nth(2).and_then(|v| v.parse::<u32>().ok()).is_some_and(|v| v > 0)));
            assert!(launch.elapsed() < test_clock::timeout(Duration::from_secs(5)), "{look}: sound arrived late");
            d.wait(|d| std::fs::read_to_string(d.card.tmp.join("trace.txt")).unwrap_or_default().lines().any(|l| l.contains(" core-present frame=")));
            // core_pacing's own panel rule judges after one second of startup.
            // Fix the two-second window before waiting, from both initialized outputs.
            let first_sound = std::fs::read_to_string(d.card.tmp.join("audio.out")).unwrap().lines()
                .map(|l| l.split_whitespace().map(|v| v.parse::<f64>().unwrap()).collect::<Vec<_>>()).find(|v| v[2]>0.0).unwrap()[0];
            let first_present = std::fs::read_to_string(d.card.tmp.join("trace.txt")).unwrap().lines().find(|l| l.contains(" core-present frame="))
                .unwrap().split_whitespace().next().unwrap().parse::<f64>().unwrap()/1e6;
            let measuring_from = first_sound.max(first_present)+1.0;
            let measuring_to = measuring_from+2.0;
            let remaining = measuring_to - pocket_ui::trace::now_us() as f64 / 1e6;
            if remaining>0.0 { std::thread::sleep(Duration::from_secs_f64(remaining)); }
            // The compositor receives real, changing GL-rendered shared-memory buffers.
            // A trace of attempted draws alone cannot establish a working display path.
            let pixels = std::fs::read_to_string(&display_log).unwrap();
            let pictures: std::collections::HashSet<_> = pixels.lines().filter_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.first() != Some(&"buffer") { return None; }
                let t: f64 = fields[1].parse().unwrap();
                (t >= measuring_from && t < measuring_to).then_some(fields[3])
            }).collect();
            assert!(pictures.len() >= 100, "{look}: real compositor received only {} distinct rendered pictures: {pixels}", pictures.len());
            d.send("home"); d.sees("Continue");
            d.wait(|d| d.log().contains("play paused"));
            let audio = std::fs::read_to_string(d.card.tmp.join("audio.out")).unwrap();
            let sounding: Vec<Vec<f64>> = audio.lines().map(|l| l.split_whitespace().map(|v| v.parse().unwrap()).collect()).filter(|v: &Vec<f64>| v[2] > 0.0).collect();
            assert!(sounding.len() > 20, "{look}: audio never flowed: {audio}");
            let trace = std::fs::read_to_string(d.card.tmp.join("trace.txt")).unwrap();
            let all_presented: Vec<(f64,u64)> = trace.lines().filter(|l| l.contains(" core-present frame="))
                .filter_map(|l| Some((l.split_whitespace().next()?.parse::<f64>().ok()? / 1e6, l.split("frame=").nth(1)?.trim().parse().ok()?))).filter(|&(_,n)| n > 0).collect();
            let in_play: Vec<_> = all_presented.iter().filter(|(t,_)| (measuring_from..=measuring_to).contains(t)).copied().collect();
            let presented: Vec<u64> = in_play.iter().map(|(_,n)| *n).collect();
            let expected = (measuring_to - measuring_from) * 60.0;
            assert!((presented.len() as f64 - expected).abs() <= 2.5, "{look}: steady play delivered {} frames, expected {expected:.1}", presented.len());
            let playing_sound: Vec<_> = sounding.iter().filter(|v| (measuring_from..=measuring_to).contains(&v[0])).collect();
            assert!(playing_sound.len() > 20 && playing_sound.last().unwrap()[0] > measuring_to - 0.3, "{look}: tone did not flow during steady play");
            let audio_frames: u64 = playing_sound.iter().map(|v| v[1] as u64).sum();
            let expected_audio = ((measuring_to - measuring_from) * 48_000.0).round() as u64;
            // At most two 256-frame output blocks straddle the fixed window.
            assert!(audio_frames.abs_diff(expected_audio) <= 512,
                "{look}: steady audio delivered {audio_frames} sample frames, expected {expected_audio} at 48 kHz");
            let mut gaps: Vec<f64> = in_play.windows(2).map(|w| w[1].0 - w[0].0).collect();
            gaps.sort_by(f64::total_cmp);
            let median = gaps[gaps.len() / 2];
            assert!((0.9 / 60.0..1.1 / 60.0).contains(&median), "{look}: output did not follow 60Hz: median {median:.4}s");
            assert!(presented.windows(2).all(|w| w[1] == w[0] + 1), "{look}: repeated or skipped output frames: {presented:?}");
            let offset = sounding[0][0] - first_present;
            assert!((-0.3..0.3).contains(&offset), "{look}: sound vs first picture: {offset:.3}s");
            std::thread::sleep(Duration::from_millis(200));
            let paused_at = pocket_ui::trace::now_us() as f64 / 1e6;
            std::thread::sleep(Duration::from_millis(100));
            let paused = std::fs::read_to_string(d.card.tmp.join("audio.out")).unwrap();
            assert!(paused.lines().all(|l| { let v: Vec<f64> = l.split_whitespace().map(|v| v.parse().unwrap()).collect(); v[0] < paused_at || v[2] == 0.0 }), "{look}: sound while paused");
            assert!(sounding.windows(2).all(|w| w[1][0]-w[0][0] < 0.3), "{look}: audio delivery stalled: {audio}");
            if look == "big" {
                d.sees("Save & quit");
                d.send("down");
            } else {
                let ready = |q: &Record| q.at == "pause:Main" && q.placed.iter().any(|(_, s)| s == "‹nav:focus›");
                for _ in 0..12 {
                    if record_until(&mut d, ready).unwrap().selected().iter().any(|s| s == "Save & quit") { break; }
                    d.send("down");
                }
                assert!(record_until(&mut d, ready).unwrap().selected().iter().any(|s| s == "Save & quit"), "Save & quit reachable");
            }
            d.send("a");
            d.wait(|d| d.log().contains("player pacing:"));
            let log = d.log();
            let pacing = log.lines().find(|l| l.starts_with("player pacing:")).unwrap();
            let startup: Vec<_> = all_presented.iter().filter(|(t,_)| *t < measuring_from).take(8).map(|(_,n)| *n).collect();
            eprintln!("{look}: steady {} frames, median {:.2}ms, repeats/skips 0; audio {audio_frames}/{expected_audio} samples, offset {offset:.3}s; startup {startup:?}; entire session: {pacing}",presented.len(),median*1000.0);
            let legacy = d.card.tmp.join("states").join(system).join(file);
            let base = if ds { pocket_ui::ds::state_base(&legacy,core) } else { legacy };
            let state = pocket_ui::slots::auto_path(&base);
            d.wait(|_| pocket_ui::state_file::exists(&state));
            let saved = pocket_ui::state_file::read(&state).unwrap();
            if ds { assert!(pocket_ui::ds::state_is_dsperate(&saved)); }
            let saved=if ds { &saved[8..] } else { &saved[..] };
            assert!(saved.starts_with(b"STUBSTATE"), "exit save belongs to the running game");
            assert!(u32::from_le_bytes(saved[16..20].try_into().unwrap()) >= 60, "save contains played frames");
            d.wait(|d| !status_of(d).contains("game=playing") && !status_of(d).contains("game=paused"));
            d.wait(|d| !pocket_ui::recents::Recents::load(&d.card.tmp.join("data")).list.is_empty());
            let recent = pocket_ui::recents::Recents::load(&d.card.tmp.join("data"));
            assert_eq!(recent.list[0].path, d.card.tmp.join(system).join(format!("{file}.{}",if ds {"nds"} else {"gb"})));
            let frame = d.frame();
            assert!(frame.contains(game_title), "{look}: shelf returns to the game: {frame}");
            if look == "big" {
                assert!(frame.contains("Continue"), "Easy names the saved moment: {frame}");
            } else {
                d.send("b\nup");
                let home = d.sees("Continue");
                assert!(home.contains(game_title), "{look}: Continue names the game: {home}");
            }
            if ds {
                assert!(!d.card.tmp.join("ds-resumed").exists(), "first start was fresh");
                d.send("a");
                // The stub creates ds-resumed before writing the frame count; wait for the parsed number, not just the file.
                let mut restored = 0u32;
                d.wait(|d| {
                    if let Ok(n) = std::fs::read_to_string(d.card.tmp.join("ds-resumed")).unwrap_or_default().trim().parse::<u32>() {
                        restored = n;
                        true
                    } else {
                        false
                    }
                });
                assert!(restored>=60,"Continue restores the played moment inside DSperate, frame {restored}");
            }
        }
        eprintln!("game start rule: {:?}", started.elapsed());
    }

    fn first_start_game(name: &str) -> Driver {
        Driver::start(name, true, |tmp, cmd| {
            std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false}"#).unwrap();
            let host = tmp.join("pocket-core-stand-in");
            std::fs::write(&host, "").unwrap();
            cmd.env("POCKETUI_HOST_BIN", host).env("POCKETUI_STATES", tmp.join("states"));
        })
    }

    /// The 0.5 hand test (15.11.2026): the controls card at a console's first start was bloat. The first game of a
    /// console pocket-core plays starts with the one A, as every other start: no card over it, nothing to press away, no
    /// twelve-second wait. (Before, the card came first and the game only after another press or the timeout.)
    #[test]
    fn a_consoles_first_start_goes_straight_to_the_game_without_a_card() {
        let mut d = first_start_game("first-start-no-card");
        d.sees("Game Boy");
        d.send("a");
        d.sees("Tobu Tobu Girl");
        let asked = Instant::now();
        d.send("a");
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        let log = d.log();
        assert!(!log.contains("controls card"), "no controls card before the game: {log}");
        assert!(asked.elapsed() < test_clock::timeout(Duration::from_secs(8)), "the one A starts the game, no card waits for a press or its timeout: {log}");
        assert!(!d.card.tmp.join("data/controls-card-seen").exists(), "no list of consoles that had a card is kept");
    }

    /// Just Play's one button too: the console's first game starts at once.
    #[test]
    fn just_play_starts_a_consoles_first_game_without_a_card() {
        let mut d = Driver::start("first-start-just-play", true, |tmp, cmd| {
            cmd.env("POCKETUI_TEST_PREPARE_MS", "500");
            let cfg = std::fs::read_to_string(tmp.join("es_systems.cfg")).unwrap();
            let only_gb = cfg.lines().filter(|line| !line.contains("<name>segacd</name>")).collect::<Vec<_>>().join("\n");
            std::fs::write(tmp.join("es_systems.cfg"), only_gb).unwrap();
            let host = tmp.join("pocket-core-stand-in");
            std::fs::write(&host, "").unwrap();
            cmd.env("POCKETUI_HOST_BIN", host).env("POCKETUI_STATES", tmp.join("states"));
        });
        d.sees("Game Boy");
        d.wait(|d| d.log().contains("curator: 1 games"));
        d.send("up");
        d.sees("Just Play");
        d.send("a");
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        let log = d.log();
        assert!(!log.contains("controls card"), "no controls card before the game: {log}");
    }

    /// A RomM game on the shelf (paired, its list read) whose file isn't on the card: `curl` is a stand-in that serves it, or
    /// that finds no server (`reachable`).
    fn romm_card(name: &str, reachable: bool) -> Driver {
        Driver::start(name, true, move |tmp, cmd| {
            std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false}"#).unwrap();
            let romm = tmp.join("data/romm");
            std::fs::create_dir_all(&romm).unwrap();
            std::fs::write(romm.join("account.json"), r#"{"server":"http://romm.test:8080","user":"sam"}"#).unwrap();
            std::fs::write(romm.join("token"), "example_token_0123456789abcdef").unwrap();
            std::fs::write(
                romm.join("catalog.json"),
                r#"{"server":"http://romm.test:8080","fetched":1,"games":[{"id":7,"system":"gb","file":"Homebrew Racer.gb","name":"Homebrew Racer","size":9}]}"#,
            )
            .unwrap();
            let bin = tmp.join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            // The download: the file to -o and the status curl's -w prints; the request (with the token) comes on stdin.
            let body = if reachable {
                "#!/bin/sh\nout=; prev=\nfor a in \"$@\"; do [ \"$prev\" = -o ] && out=$a; prev=$a; done\ncat > \"$0.request\"\nif [ -n \"$out\" ]; then printf 'RACER-ROM' >> \"$out\"; printf 200; exit 0; fi\nprintf '{}\\n404'\n"
            } else {
                "#!/bin/sh\ncat > /dev/null\nexit 7\n"
            };
            executable(&bin.join("curl"), body);
            let host = tmp.join("pocket-core-stand-in");
            std::fs::write(&host, "").unwrap();
            cmd.env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()))
                .env("POCKETUI_HOST_BIN", host)
                .env("POCKETUI_STATES", tmp.join("states"));
        })
    }

    /// RomM: a game of the server is on the shelf beside the card's; A downloads it once (into its system's folder, the
    /// token never on curl's command line) and starts it.
    #[test]
    fn a_romm_game_downloads_at_its_first_start_and_plays() {
        let mut d = romm_card("romm-start", true);
        d.sees("Game Boy");
        d.send("a");
        d.sees("Homebrew Racer");
        d.send("a");
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        assert_eq!(std::fs::read(d.card.tmp.join("gb/Homebrew Racer.gb")).unwrap(), b"RACER-ROM", "the game is on the card");
        assert!(!d.card.tmp.join("gb/.pocket-ui-romm").exists(), "nothing left of the download");
        let request = std::fs::read_to_string(d.card.tmp.join("bin/curl.request")).unwrap();
        assert!(request.contains("/api/roms/7/content/Homebrew%20Racer.gb") && request.contains("Authorization: Bearer example_token"), "{request}");
        let log = d.log();
        assert!(log.contains("romm: downloading Homebrew Racer.gb") && log.contains("romm: Homebrew Racer.gb downloaded"), "{log}");
        assert!(!log.contains("example_token"), "the token is never in the log");
    }

    /// RomM: the server is gone at the start. The start ends with plain words, no half game lands on the card, and the game
    /// stays on the shelf for a later try.
    #[test]
    fn a_romm_game_without_its_server_says_so_and_stays() {
        let mut d = romm_card("romm-gone", false);
        d.sees("Game Boy");
        d.send("a");
        d.sees("Homebrew Racer");
        d.send("a");
        let text = d.sees("Can't reach your RomM server");
        assert!(text.contains("Check Wi-Fi"), "{text}");
        assert!(!d.card.tmp.join("gb/Homebrew Racer.gb").exists());
        assert!(!d.card.tmp.join("run-game.started").exists(), "nothing started");
        d.send("b");
        d.sees("Homebrew Racer");
    }

    /// 15.11.2026 (0.5 candidate on the RG35XX Plus): the first SNES start showed the controls card instead of the Cozy
    /// insert scene. A never-played console's first game, not sealed: A plays the insert scene (the picture moves frame
    /// after frame), the game starts after it, and nothing called the card at any frame.
    #[test]
    fn a_consoles_first_start_plays_the_insert_scene_and_no_card() {
        let mut d = Driver::start("first-start-insert-scene", true, |tmp, cmd| {
            std::fs::write(tmp.join("Game.rom"), b"game").unwrap();
            std::fs::write(tmp.join("es_systems.cfg"), format!("<systemList><system><name>snes</name><fullname>Super Nintendo</fullname><path>{}</path><extension>.rom</extension><command>true</command></system></systemList>", tmp.display())).unwrap();
            std::fs::write(tmp.join("host"), b"").unwrap();
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"shelf","motion":"full","animations":"cozy","frames":"every","art":"off"}"#).unwrap();
            cmd.env("POCKETUI_HOST_BIN", tmp.join("host")).env("POCKETUI_STATES", tmp.join("states")).env("POCKETUI_SEALED", "0")
                .env("POCKETUI_TEST_PREPARE_MS", "50");
            std::fs::write(tmp.join("take.txt"), "# open the console, let the case turn, A on the game\n40 a\n120 a\nend 420\n").unwrap();
            cmd.env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
                .env("POCKETUI_FRAME_DUMP", tmp.join("dump"))
                .env("POCKETUI_FRAME_DUMP_SCRIPT", tmp.join("take.txt"));
            executable(&tmp.join("run-game"), "#!/bin/sh\nprintf 'started\\n' > \"$0.started\"\nsleep 30\n");
        });
        let dir = d.card.tmp.join("dump");
        let log = || std::fs::read_to_string(dir.join("events.log")).unwrap_or_default();
        d.wait(|_| log().lines().any(|l| l.starts_with("end ")));
        let events = log();
        let asked = events.lines().filter_map(|l| l.split_once(' ')).filter(|(_, l)| *l == "script a").nth(1)
            .and_then(|(t, _)| t.parse::<u64>().ok()).unwrap_or_else(|| panic!("A on the game was expected: {events}"));
        let frames = pocket_ui::frame_dump::read(&dir.join("frames.pfd")).unwrap();
        // The insert scene: the picture moves in most of the half second after A.
        let moving = (asked + 2..asked + 32).filter(|&k| frames[k as usize].pixels != frames[k as usize - 1].pixels).count();
        assert!(moving >= 20, "the insert scene plays after A: {moving} of 30 frames moved: {events}");
        assert!(!events.contains("controls-card"), "no controls card at any frame: {events}");
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        let after = d.log();
        assert!(!after.contains("controls card"), "{after}");
    }

    /// 15.11.2026 (Android night, also Linux): at the first start of a console the sealed game's tear was covered within
    /// 50 ms of A (its sound dropped with it). The tear plays in full and with its sound, once, and the game starts after it.
    #[test]
    fn a_sealed_games_first_start_tears_its_film_in_full_and_then_starts_the_game() {
        let mut d = Driver::start("sealed-tear-then-game", true, |tmp, cmd| {
            std::fs::write(tmp.join("Game.rom"), b"game").unwrap();
            std::fs::write(tmp.join("es_systems.cfg"), format!("<systemList><system><name>gb</name><fullname>Fixture console</fullname><path>{}</path><extension>.rom</extension><command>true</command></system></systemList>", tmp.display())).unwrap();
            std::fs::write(tmp.join("host"), b"").unwrap();
            // As a player has it: Cozy, Motion Full, the shelf; a library nobody played, so every game is in its film.
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"shelf","motion":"full","animations":"cozy","frames":"every","art":"off"}"#).unwrap();
            cmd.env("POCKETUI_HOST_BIN", tmp.join("host")).env("POCKETUI_STATES", tmp.join("states")).env("POCKETUI_SEALED", "1")
                // About what the real preparation takes: ready well before the tear is over.
                .env("POCKETUI_TEST_PREPARE_MS", "50");
            std::fs::write(tmp.join("take.txt"), "# open the console, let the case turn, A on the game\n40 a\n120 a\nend 420\n").unwrap();
            cmd.env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
                .env("POCKETUI_FRAME_DUMP", tmp.join("dump"))
                .env("POCKETUI_FRAME_DUMP_SCRIPT", tmp.join("take.txt"));
            executable(&tmp.join("run-game"), "#!/bin/sh\nprintf 'started\\n' > \"$0.started\"\nsleep 30\n");
        });
        let dir = d.card.tmp.join("dump");
        let log = || std::fs::read_to_string(dir.join("events.log")).unwrap_or_default();
        d.wait(|_| log().lines().any(|l| l.starts_with("end ")));
        let events = log();
        let lines: Vec<(u64, &str)> = events.lines().filter_map(|l| l.split_once(' ')).filter_map(|(t, rest)| Some((t.parse().ok()?, rest))).collect();
        let tick = |what: &str| lines.iter().find(|(_, l)| l.contains(what)).map(|(t, _)| *t);
        let asked = lines.iter().filter(|(_, l)| *l == "script a").nth(1).map(|(t, _)| *t)
            .unwrap_or_else(|| panic!("A on the game was expected: {events}\n{}", d.log()));
        let tear = tick("sound WrapTear").unwrap_or_else(|| panic!("the film's tear sound was never asked for: {events}"));
        assert!(tear <= asked + 2, "the tear's sound starts with A: A at {asked}, sound at {tear}: {events}");
        let frames = pocket_ui::frame_dump::read(&dir.join("frames.pfd")).unwrap();
        // The tear (`shrink_wrap::TEAR`, 560 ms = 34 frames) is seen in full: the picture moves in nearly every frame of it.
        let torn = pocket_ui::shrink_wrap::TEAR.as_secs_f64() * 60.0;
        let moving = (asked + 2..asked + torn as u64 - 2).filter(|&k| frames[k as usize].pixels != frames[k as usize - 1].pixels).count();
        assert!(moving >= torn as usize - 8, "the film tears in {moving} different pictures of {torn:.0}: {events}");
        // Nothing stopped the sound while it played.
        let cut = lines.iter().filter(|(t, l)| *l == "sound stop" && *t > tear && *t <= asked + torn as u64).count();
        assert_eq!(cut, 0, "the sound was stopped while the film tore: {events}");
        // The one A started the game, with no card on the way.
        let after = d.log();
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        assert!(!after.contains("controls card"), "{after}");
        assert_eq!(events.lines().filter(|l| l.contains("sound WrapTear")).count(), 1, "{events}");
    }

    /// A Kids Mode card whose one game is the Game Boy one, routed to pocket-core (a stand-in), `run-game` exiting with an error.
    fn kids_card_with_failing_game(name: &str, prepare: impl FnOnce(&Path)) -> Driver {
        Driver::start(name, true, |tmp, cmd| {
            let game = tmp.join("gb/Tobu Tobu Girl (World).gb");
            let games = serde_json::json!({"on": true, "pin": "0000", "games": [game], "theme": "blue"});
            std::fs::write(tmp.join("data/kids.json"), games.to_string()).unwrap();
            cmd.env("POCKETUI_HOST_BIN", tmp.join("pocket-core-stand-in"));
            std::fs::write(tmp.join("pocket-core-stand-in"), "").unwrap();
            prepare(tmp);
        })
    }

    /// Kids Mode: no way to delete a game (21.11. brief). X, Y, SELECT and START on the child's shelf open no card, no
    /// Delete row and no sheet; the game file stays.
    #[test]
    fn a_child_can_t_reach_delete_from_the_kids_shelf() {
        let mut d = kids_card_with_failing_game("kids-no-delete", |_| {});
        d.sees("Tobu Tobu Girl");
        let rom = d.card.tmp.join("gb/Tobu Tobu Girl (World).gb");
        for keys in ["x", "y", "select", "start", "x\nright\nselect\ndown\ndown\ndown"] {
            d.send(keys);
            std::thread::sleep(std::time::Duration::from_millis(300));
            let f = d.frame();
            assert!(!f.contains("Delete from the card") && !f.contains("Delete Tobu") && !f.contains("Hide from the shelf"), "{keys}: {f}");
        }
        assert!(rom.exists());
    }

    fn fail_at_once(tmp: &Path) {
        let _ = std::fs::remove_file(tmp.join("run-game"));
        executable(&tmp.join("run-game"), "#!/bin/sh\nprintf 'x' >> \"$0.starts\"\nexit 1\n");
    }

    /// pocket-core fails on a child's game: no RetroArch, no second start, nothing in `fallback.json`; the picture, and a
    /// press is the shelf again.
    #[test]
    fn a_childs_game_that_fails_stops_on_the_picture_and_never_falls_back() {
        let mut d = kids_card_with_failing_game("kids-fail", |_| {});
        d.sees("Tobu Tobu Girl");
        fail_at_once(&d.card.tmp.clone());
        d.send("a");
        let picture = d.sees("Please ask a grown-up");
        assert!(picture.contains("won't start right now"), "{picture}");
        assert_eq!(std::fs::read_to_string(d.card.tmp.join("run-game.starts")).unwrap(), "x", "one start, no retry");
        assert!(!d.card.tmp.join("data/fallback.json").exists(), "the normal shelf's list stays as it was");
        assert!(d.log().contains("(emulator pocket-core"), "{}", d.log());
        d.send("a");
        let shelf = d.sees("Tobu Tobu Girl");
        assert!(!shelf.contains("Please ask a grown-up"), "{shelf}");
    }

    /// A real launch handoff pipe distinguishes a failed start from a game which showed a frame
    /// and then crashed. A time-only guess used to reroute Continue to another emulator.
    #[test]
    fn a_crash_after_a_picture_keeps_continue_and_a_failed_start_retries_only_once() {
        for (evidence, saved_moment, retry) in [("starting\n", false, true), ("starting\nframe\n", false, false), ("starting\n", true, false), ("", false, false), ("starting\npresenting\n", false, false)] {
            let mut d = Driver::start("crash-route", true, |tmp, cmd| {
                let cfg = tmp.join("es_systems.cfg");
                let text = std::fs::read_to_string(&cfg).unwrap().replace("</command>",
                    "</command><emulators><emulator name=\"retroarch\"><cores><core default=\"true\">gambatte</core></cores></emulator></emulators>");
                std::fs::write(cfg, text).unwrap();
                std::fs::write(tmp.join("host"), "").unwrap();
                std::fs::write(tmp.join("data/controls-card-seen"), "gb\n").unwrap();
                let states = tmp.join("states/gb");
                std::fs::create_dir_all(&states).unwrap();
                if saved_moment {
                    std::fs::write(states.join("Tobu Tobu Girl (World).pstate.auto"), b"my progress").unwrap();
                }
                cmd.env("POCKETUI_HOST_BIN", tmp.join("host")).env("POCKETUI_STATES", tmp.join("states"))
                    .env("POCKETUI_NAV_PROBE", tmp.join("nav.tsv"));
                // Each attempt reports first-frame evidence over the same inherited descriptor as pocket-core.
                // Python's explicit os.write avoids shell-specific numeric-fd redirection limits.
                executable(&tmp.join("run-game"), &format!(r#"#!/usr/bin/env python3
import os, pathlib
p = pathlib.Path(__file__ + '.starts')
n = len(p.read_text()) if p.exists() else 0
p.write_text('x' * (n + 1))
fd = os.environ.get('POCKET_HANDOFF_FD')
if fd:
    os.write(int(fd), {evidence:?}.encode())
    os.close(int(fd))
raise SystemExit(1)
"#));
            });
            d.sees("Game Boy");
            d.send("a");
            d.sees("Tobu Tobu Girl");
            d.send("a");
            d.wait(|d| d.log().contains("game exited after"));
            if retry {
                d.wait(|d| d.log().matches("game exited after").count() == 2);
                assert!(d.log().contains(": retroarch for this launch"), "{}", d.log());
                let notice = d.sees("didn't start");
                assert!(notice.contains("Tobu Tobu Girl"), "{notice}");
                assert_eq!(launches(&d), 2, "one fallback, no loop: {}", d.log());
                assert!(d.log().contains("(emulator retroarch"), "{}", d.log());
            } else {
                // The visible shelf/notice is requested after the exit is handled, not merely reaped.
                d.frame();
                assert_eq!(launches(&d), 1, "never change the route of a played game or an existing moment: {}", d.log());
                assert!(!d.log().contains("(emulator retroarch"), "{}", d.log());
            }
            if saved_moment {
                assert_eq!(std::fs::read(d.card.tmp.join("states/gb/Tobu Tobu Girl (World).pstate.auto")).unwrap(), b"my progress");
            }
            d.send("b");
            // A played game returns to Home; a failed start can stay in its game list.
            // A title mentioned in Home's recents is not proof that the game list is open.
            let nav = d.card.tmp.join("nav.tsv");
            let _ = std::fs::remove_file(&nav);
            d.send("nav");
            d.wait(|_| std::fs::read_to_string(&nav).is_ok_and(|s| s.ends_with("@end\n")));
            let before = std::fs::read_to_string(&nav).unwrap();
            if before.contains("@where\tshelf:Home") {
                let _ = std::fs::remove_file(&nav);
                d.send("a\nnav");
                d.wait(|_| std::fs::read_to_string(&nav).is_ok_and(|s| s.ends_with("@end\n")));
            }
            let shelf = d.sees("Tobu Tobu Girl");
            assert!(!shelf.contains("didn't start"), "B gets back to the game list: {shelf}");
            // A second explicit start always tries pocket-core again, including after the one-launch fallback.
            d.send("a");
            d.wait(|d| launches(d) >= if retry { 3 } else { 2 });
            let log = d.log();
            let requested = log.lines().filter(|l| l.starts_with("launch ") && l.contains("(emulator")).nth(if retry { 2 } else { 1 }).unwrap();
            assert!(requested.contains("(emulator pocket-core"), "{requested}");
        }
    }

    /// New-player walk 2026-11-02: HOME did nothing in Settings. It now leaves for Home from the plain pages, and keeps
    /// out of a page that has something under way (the Updates page while it checks or downloads).
    #[test]
    fn home_in_settings_goes_home_unless_something_is_under_way() {
        let mut d = Driver::new("home-in-settings", true);
        d.sees("Game Boy");
        d.send("start");
        d.sees("Bluetooth");
        // Two levels down: a plain page of a category's list (Look's Cozy extras, its eighth row since 0.5.1).
        d.send("r1\ndown\ndown\ndown\ndown\ndown\ndown\ndown\na");
        d.sees("TV on the cabinet");
        d.send("home");
        let shelf = d.sees("Game Boy");
        assert!(!shelf.contains("TV on the cabinet") && !shelf.contains("Bluetooth"), "HOME leaves Settings for Home: {shelf}");
        // Quick settings are still HOME's on the shelf itself.
        d.send("home");
        d.sees("Brightness");
    }

    /// Without pocket-core installed, the system routes to RetroArch: Kids Mode still cannot start it there.
    #[test]
    fn a_childs_game_routed_to_retroarch_does_not_start() {
        let mut d = kids_card_with_failing_game("kids-routed", |tmp| {
            std::fs::remove_file(tmp.join("pocket-core-stand-in")).unwrap();
            let cfg = tmp.join("es_systems.cfg");
            let text = std::fs::read_to_string(&cfg).unwrap().replace(
                "<extension>.gb .zip</extension>",
                "<extension>.gb .zip</extension><emulators><emulator name=\"retroarch\"><cores><core default=\"true\">gambatte</core></cores></emulator></emulators>");
            std::fs::write(cfg, text).unwrap();
        });
        d.sees("Tobu Tobu Girl");
        d.send("a");
        d.sees("Please ask a grown-up");
        assert!(!d.card.tmp.join("run-game.started").exists(), "{}", d.log());
        assert!(d.log().contains("kids mode: stopped"), "{}", d.log());
    }

    /// Old automatic guesses are retired in Kids Mode too: the next start tries the safe own host again.
    #[test]
    fn an_old_early_crash_does_not_send_a_childs_game_to_retroarch() {
        let mut d = kids_card_with_failing_game("kids-old-fallback", |tmp| {
            let game = tmp.join("gb/Tobu Tobu Girl (World).gb");
            std::fs::write(tmp.join("data/fallback.json"), serde_json::json!([game]).to_string()).unwrap();
        });
        d.sees("Tobu Tobu Girl");
        d.send("a");
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        assert!(d.log().contains("(emulator pocket-core"), "{}", d.log());
        assert!(!d.log().contains("(emulator retroarch"), "{}", d.log());
        // Retirement is asynchronous; launching the safe player must not wait for a card write.
        d.wait(|d| std::fs::read_to_string(d.card.tmp.join("data/fallback.json")).is_ok_and(|s| s.trim() == "[]"));
        assert_eq!(std::fs::read_to_string(d.card.tmp.join("data/fallback.json")).unwrap().trim(), "[]");
    }

    /// The dialog with Open EmulationStation isn't there in Kids Mode, however it is asked for (a keyboard's Escape comes the
    /// same way as the command pipe's `leave`).
    #[test]
    fn a_child_cannot_open_the_leave_dialog() {
        let mut d = kids_card_with_failing_game("kids-leave", |_| {});
        d.sees("Tobu Tobu Girl");
        d.send("leave");
        let frame = d.frame();
        assert!(!frame.contains("EmulationStation"), "{frame}");
    }

    /// New-player walk 2026-11-02, the first start of 0.2: a parent with no games presses "For a child…". Kids Mode's steps
    /// need games it can start, so they don't come: the look stage says where to set the mode up later, and Finish ends on
    /// the normal shelf, never on a locked, empty one.
    #[test]
    fn a_first_start_for_a_child_with_no_games_never_locks_an_empty_shelf() {
        let mut d = Driver::start("kids-first", false, |tmp, _| {
            std::fs::remove_file(tmp.join("gb/Tobu Tobu Girl (World).gb")).unwrap();
            std::fs::remove_file(tmp.join("segacd/Pocket CD Demo (USA).cue")).unwrap();
        });
        d.sees("Welcome to your Pocket");
        d.send("y");
        d.sees("Kids Mode is set up after your games");
        // Wi-Fi, and the time zone when it is asked: their rows go on.
        d.first_start_to_games("Add your games");
        d.sees("Kids Mode needs a few games first");
        // I'll add games later, the second row.
        d.send("down\na");
        d.sees("Kids Mode needs games it can start");
        d.send("a");
        // The last page, how cozy: A finishes.
        d.sees("How cozy should it be?");
        d.send("a");
        // The normal empty Home with its ways to games, not the child's "No games here yet".
        let shelf = d.sees("No games yet");
        assert!(!shelf.contains("No games here yet"), "the normal shelf: {shelf}");
        let file = d.card.tmp.join("data/kids.json");
        assert!(std::fs::read_to_string(&file).map_or(true, |t| !t.contains("\"on\": true") && !t.contains("\"on\":true")), "Kids Mode stays off");
    }

    /// The first start for a child with games on the card: after "Your games" Kids Mode's own steps take the look's place,
    /// and the setup ends on the child's shelf. B on their first page goes on to the look instead.
    #[test]
    fn a_first_start_for_a_child_sets_up_kids_mode_after_the_games() {
        let mut d = Driver::start("kids-first-games", false, |_, _| {});
        d.sees("Welcome to your Pocket");
        d.send("y");
        d.sees("Kids Mode is set up after your games");
        d.first_start_to_games("Your games");
        d.sees("Kids Mode is next");
        d.send("a");
        // B goes on to the look, and its hint says so (it said "Back", first-start walk 2026-11-06).
        let intro = d.sees("Set it up for a child");
        assert!(intro.lines().any(|l| l == "Skip") && !intro.lines().any(|l| l == "Back"), "{intro}");
        d.send("b");
        let look = d.sees("How your games stand");
        assert!(look.contains("Settings › System › Kids mode"), "where Kids Mode is later: {look}");
        // B again, as somebody going back would: back to your games, never the shelf (first-start walk 2026-11-06).
        d.send("b");
        d.sees("Found 2 games");
        assert!(d.card.tmp.join("data/setup-step").exists(), "still in the first start");
        // On again: the look (Kids Mode was skipped), how cozy, and A there finishes.
        d.send("a");
        d.sees("How your games stand");
        d.send("a");
        d.sees("How cozy should it be?");
        d.send("a");
        d.sees("Game Boy");
        // Again, all the way: the PIN twice, a game, the colour, the time, the volume, done.
        let mut d = Driver::start("kids-first-games-2", false, |_, _| {});
        d.sees("Welcome to your Pocket");
        d.send("y");
        d.sees("Kids Mode is set up after your games");
        d.first_start_to_games("Your games");
        d.sees("Kids Mode is next");
        d.send("a");
        d.sees("Set it up for a child");
        d.send("a");
        d.sees("Pick a PIN");
        for _ in 0..4 {
            d.send("a");
        }
        d.sees("Once more");
        for _ in 0..4 {
            d.send("a");
        }
        d.sees("Tobu Tobu Girl");
        d.send("a");
        d.send("start");
        d.sees("Blue or Pink?");
        d.send("a");
        d.sees("How long per day?");
        d.send("a");
        d.sees("How loud at most?");
        d.send("a");
        d.send("a");
        let file = d.card.tmp.join("data/kids.json");
        d.wait(|_| std::fs::read_to_string(&file).is_ok_and(|t| t.contains("\"on\": true") || t.contains("\"on\":true")));
        d.sees("Tobu Tobu Girl");
        assert!(!d.card.tmp.join("data/setup-step").exists(), "the first start is over");
    }

    /// Kids Mode on an empty card, with the real buttons' way through: the child's shelf says so; a grown-up holds SELECT and
    /// START, enters the PIN, goes to Games, is sent to Add games and comes back; turning the mode off brings the normal shelf.
    #[test]
    fn kids_mode_with_no_games_goes_to_add_games_and_back_and_turns_off() {
        let mut d = Driver::start("kids", true, |tmp, _| {
            std::fs::remove_file(tmp.join("gb/Tobu Tobu Girl (World).gb")).unwrap();
            std::fs::remove_file(tmp.join("segacd/Pocket CD Demo (USA).cue")).unwrap();
            std::fs::write(tmp.join("data/kids.json"), r#"{"on":true,"pin":"0000","games":[],"theme":"pink","minutes":45}"#).unwrap();
        });
        d.sees("No games here yet");
        // Nothing of the normal shelf reaches a child.
        d.send("start");
        d.send("home");
        d.sees("No games here yet");
        d.send("holddown");
        d.sees("Enter your PIN");
        d.send("holdup");
        for _ in 0..4 {
            d.send("a");
        }
        d.sees("30 more minutes today");
        d.send("down");
        d.send("down");
        d.send("a");
        d.sees("No games to pick yet");
        d.send("a");
        let add = d.sees("Add your games");
        assert!(!add.contains("No games to pick yet"), "{add}");
        d.send("b");
        d.sees("No games to pick yet");
        d.send("start");
        d.sees("30 more minutes today");
        for _ in 0..5 {
            d.send("down");
        }
        d.send("a");
        let file = d.card.tmp.join("data/kids.json");
        d.wait(|_| std::fs::read_to_string(&file).is_ok_and(|t| t.contains("\"on\": false") || t.contains("\"on\":false")));
        let shelf = d.sees("Settings");
        assert!(!shelf.contains("No games here yet"), "{shelf}");
    }

    #[test]
    fn changing_case_stickers_refreshes_the_standing_shelf_spine_in_the_same_list() {
        let mut ui = Driver::start("live-case-stickers", true, |tmp, cmd| {
            let data = tmp.join("data");
            std::fs::write(tmp.join("gb/Zeta (World).gb"), b"").unwrap();
            std::fs::write(data.join("settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"shelf","details_page":true}"#).unwrap();
            std::fs::write(data.join("curator.json"), r#"{"lens":false,"echoes":false,"stickers":false,"history":false}"#).unwrap();
            cmd.env("POCKETUI_SNAPSHOT_SIZE", "1240x1080");
        });
        // Keep the first game standing rather than lifted: select the second game before comparing its neighbour.
        ui.sees("Game Boy");
        ui.send("a\ndown");
        ui.sees("Zeta");
        let spine_pixels = |ui: &mut Driver| {
            let mut decoded = None;
            ui.wait(|d| {
                // frame() observes creation of the output file; compression may still be writing it. Wait only
                // for a complete PNG, never for particular colours (which would hide a stale-cache failure).
                decoded = (|| {
                    let mut reader = png::Decoder::new(std::fs::File::open(d.card.tmp.join("frame.png")).ok()?).read_info().ok()?;
                    let mut pixels = vec![0; reader.output_buffer_size()];
                    let info = reader.next_frame(&mut pixels).ok()?;
                    reader.finish().ok()?;
                    Some((info, pixels))
                })();
                decoded.is_some()
            });
            let (info, pixels) = decoded.unwrap();
            assert_eq!(info.color_type, png::ColorType::Rgb);
            assert_eq!((info.width, info.height), (1240, 1080));
            let x = pocket_ui::palette::MARGIN as usize;
            // The first GB spine occupies x=72..132 at this canvas size; neither the selected second spine nor
            // the large front/title above the row enters this rectangle. Compare actual rendered row/tile pixels.
            (540..968).flat_map(|y| pixels[(y * 1240 + x) * 3..(y * 1240 + x + 60) * 3].iter().copied()).collect::<Vec<_>>()
        };
        let before = spine_pixels(&mut ui);
        for on in [true, false] {
            ui.send("up");
            ui.sticker_sheet();
            ui.send("a"); // Completed on/off; no restart and no navigation out of the system's list.
            ui.wait(|d| {
                let value: serde_json::Value = std::fs::read_to_string(d.card.tmp.join("data/library.json"))
                    .ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
                value["games"].as_object().is_some_and(|games| {
                    games.iter().find(|(path, _)| path.ends_with("Tobu Tobu Girl (World).gb"))
                        .is_some_and(|(_, g)| g["case"]["completed"].as_bool().unwrap_or(false)) == on
                })
            });
            ui.send("b\nb\ndown");
            ui.sees("Details");
            let after = spine_pixels(&mut ui);
            if on {
                assert!(after != before, "adding a sticker must change the already-cached standing spine");
            } else {
                assert!(after == before, "removing it must restore that same spine without leaving the list");
            }
        }
    }

    #[test]
    fn sticker_save_errors_reach_the_shelf_after_the_sheet_is_closed() {
        let mut ui = Driver::start("case-sticker-save-error", true, |tmp, _| {
            let data = tmp.join("data");
            std::fs::write(data.join("settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"shelf","details_page":true}"#).unwrap();
            std::fs::write(data.join("curator.json"), r#"{"lens":false,"echoes":false,"stickers":false,"history":false}"#).unwrap();
            // Atomic replacement cannot rename a file over this directory: deterministic SD-write failure.
            std::fs::create_dir(data.join("library.json")).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("a");
        ui.sees("Details");
        ui.sticker_sheet();
        ui.send("a\nb\nb");
        // The shelf is back with the error still on it: below Home a game's bar says Y Star (START Settings gives way there,
        // review 19.11. #4). The bar's hints give way while a fresh toast stands over them (tests/hint_bar.rs `:toasted`),
        // so the frame that shows both comes a moment after the error's first one.
        let mut text = String::new();
        ui.wait(|d| {
            text = d.frame();
            text.contains("Could not save: is the SD card full?") && text.contains("Star")
        });
        assert!(text.contains("Could not save"), "the error must remain visible after returning to the shelf: {text}");
    }

    /// First-start walk 2026-11-06, on a clock nobody set (UTC, no Wi-Fi region): the time zone came up every time the
    /// Wi-Fi stage was left, also after Skip (three times in one first start), and Wi-Fi opened from the phone page
    /// ("Connect to Wi-Fi first") led on to "Your games" instead of back to the phone page.
    #[test]
    fn the_time_zone_is_asked_once_and_wifi_from_the_phone_page_leads_back_to_it() {
        let mut ui = Driver::start("first-start-zone-once-phone", false, |tmp, cmd| {
            wifi_programs(tmp, cmd);
            // No country from the Wi-Fi chip either: nothing to guess from.
            executable(&tmp.join("bin/iw"), "#!/bin/sh\necho 'global'\necho 'country 00: DFS-UNSET'\n");
            cmd.env("POCKETUI_MOCK_ZONE", "UTC");
        });
        ui.sees("Welcome to your Pocket");
        ui.send("a");
        ui.sees("Neighbour");
        // Offline: the time zone is asked, and kept as it is (Keep UTC, its first row).
        ui.wifi_continue();
        ui.sees("More in Settings › System › Time.");
        ui.send("a");
        ui.sees("Your games");
        // B back to Wi-Fi and on offline again: straight to your games.
        ui.send("b");
        ui.sees("Neighbour");
        ui.wifi_continue();
        let games = ui.sees("Your games");
        assert!(!games.contains("More in Settings › System › Time"), "{games}");
        // Add more…, Y: the phone page without Wi-Fi, whose A opens the Wi-Fi page.
        ui.send("down\na");
        ui.sees("One folder per system");
        ui.send("y");
        ui.sees("Connect to Wi-Fi first");
        ui.send("a");
        ui.sees("Neighbour");
        ui.send("a");
        ui.sees("Password for Wifi");
        ui.send(&("right\n".to_string() + &"a\n".repeat(8) + "start"));
        ui.sees("Connected to Wifi");
        // On by itself, past the time zone (asked once already) and back to the phone page, now with an address.
        let phone = ui.sees("Send from your phone");
        assert!(!phone.contains("Connect to Wi-Fi first"), "{phone}");
        std::thread::sleep(Duration::from_millis(300));
        let still = ui.frame();
        assert!(still.contains("Send from your phone") && !still.contains("More in Settings › System › Time"), "{still}");
    }

    /// First-start walk 2026-11-06: Down on the welcome did nothing (and then Right changed the language), Back was in no
    /// hint, and L1 on the time zone skipped its Wi-Fi page.
    #[test]
    fn the_first_start_says_back_and_the_welcome_reaches_easy_to_read() {
        let mut ui = Driver::start("first-start-l1", false, |tmp, cmd| {
            let bin = tmp.join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            executable(&bin.join("iw"), "#!/bin/sh\necho 'global'\necho 'country 00: DFS-UNSET'\n");
            cmd.env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
            cmd.env("POCKETUI_MOCK_ZONE", "UTC");
        });
        let welcome = ui.sees("Welcome to your Pocket");
        assert!(!welcome.lines().any(|l| l == "L1"), "nothing before the welcome: {welcome}");
        ui.send("down\nright");
        let on = ui.sees("Aa  On");
        assert!(on.contains("English"), "the language stays: {on}");
        ui.send("left\nup\na");
        // One scheme (2026-11-07): Continue offline is the Wi-Fi page's last row, B goes back, and the bar says so.
        let wifi = ui.sees("Continue offline");
        assert!(wifi.lines().any(|l| l == "Back"), "{wifi}");
        ui.send("b");
        ui.sees("Welcome to your Pocket");
        ui.send("a");
        ui.wifi_continue();
        ui.sees("More in Settings › System › Time.");
        ui.send("l1");
        ui.sees("Continue offline");
    }

    /// First-start walk 2026-11-06: games sent from the phone during the first start were counted on "Your games", but
    /// the look after it said "Shown with sample games until yours are here" and showed sample games.
    #[test]
    fn games_that_arrive_during_the_first_start_stand_in_the_look() {
        let mut d = Driver::start("first-start-games-arrive", false, |tmp, _| {
            std::fs::remove_file(tmp.join("gb/Tobu Tobu Girl (World).gb")).unwrap();
            std::fs::remove_file(tmp.join("segacd/Pocket CD Demo (USA).cue")).unwrap();
        });
        d.sees("Welcome to your Pocket");
        d.first_start_to_games("Add your games");
        // A game arrives while the page is up, as the phone page puts it there; Add games… and back counts again.
        std::fs::write(d.card.tmp.join("gb/Tobu Tobu Girl (World).gb"), "x").unwrap();
        d.send("a");
        d.sees("One folder per system");
        d.send("b");
        d.sees("Found 1 game");
        d.send("a");
        let look = d.sees("How your games stand");
        assert!(look.contains("with your own games"), "{look}");
        assert!(!look.contains("sample games"), "{look}");
    }

    /// First-start walk 2026-11-06: games sent during the first start were greeted on the first shelf with "2 new
    /// games. Settings › Games › Game check looks them over.": the player had just sent them, and knows no Game check.
    #[test]
    fn games_sent_during_the_first_start_get_no_toast_on_the_first_shelf() {
        let mut ui = Driver::start("first-start-no-toast", false, |tmp, _| {
            std::fs::remove_file(tmp.join("gb/Tobu Tobu Girl (World).gb")).unwrap();
            std::fs::remove_file(tmp.join("segacd/Pocket CD Demo (USA).cue")).unwrap();
        });
        ui.sees("Welcome to your Pocket");
        ui.first_start_to_games("Add your games");
        std::fs::write(ui.card.tmp.join("gb/Tobu Tobu Girl (World).gb"), "x").unwrap();
        ui.send("a");
        ui.sees("One folder per system");
        ui.send("b");
        ui.sees("Found 1 game");
        ui.send("a");
        ui.sees("How your games stand");
        ui.send("a");
        ui.sees("How cozy should it be?");
        ui.send("a");
        ui.sees("Game Boy");
        for _ in 0..10 {
            let shelf = ui.frame();
            assert!(!shelf.contains("new game") && !shelf.contains("Game check"), "{shelf}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn fresh_card_through_setup_to_a_visible_game() {
        let mut ui = Driver::new("first-start", false);
        let first = ui.frame();
        assert!(first.contains("Welcome to your Pocket"), "first start should explain setup: {first}");
        // The language, Wi-Fi left by its Continue row (and the time zone when it is asked), Your games: Continue, the look:
        // Continue, how cozy: Finish.
        ui.first_start_to_games("Your games");
        // Each console with its name, not only a picture (first-start walk 2026-11-06: Game Gear, Master System).
        let found = ui.sees("Found 2 games");
        assert!(found.contains("Game Boy"), "{found}");
        ui.send("a");
        // The style is a row of the look, with Retro among its choices (2026-11-07: shown, not behind a key).
        let look = ui.sees("How your games stand");
        assert!(look.lines().any(|l| l == "Style") && look.lines().any(|l| l == "Retro"), "{look}");
        ui.send("a");
        // The last page names its three levels; A takes Cozy, as pocket-ui ships.
        let cozy = ui.sees("How cozy should it be?");
        assert!(["Plain", "Cozy", "Everything"].iter().all(|l| cozy.lines().any(|x| x == *l)), "{cozy}");
        ui.send("a");
        ui.sees("Game Boy");
        ui.send("a");
        let text = ui.sees("Tobu Tobu Girl");
        assert!(!text.contains("Ghost") && !text.contains("README"), "{text}");
    }

    /// H700 0.2.0 shipped slow GBA with mGBA. Fresh games must launch the lighter bundled gpSP;
    /// an existing mGBA Continue must keep its owner. Real fps/audio remain a device release check.
    #[test]
    fn h700_fresh_gba_uses_gpsp_and_an_old_continue_keeps_mgba() {
        for (suffix, expected) in [(None, "gpsp"), (Some(".pstate.auto"), "mgba")] {
            let mut ui = Driver::start("h700-default-core", true, |tmp, cmd| {
                let folder = tmp.join("gba");
                std::fs::create_dir(&folder).unwrap();
                std::fs::write(folder.join("Clock Demo.gba"), vec![0u8; 32768]).unwrap();
                std::fs::write(tmp.join("es_systems.cfg"), format!("<systemList><system><name>gba</name><fullname>Game Boy Advance</fullname><path>{}</path><extension>.gba</extension><command>true</command><emulators><emulator name=\"retroarch\"><cores><core default=\"true\">mgba</core></cores></emulator></emulators></system></systemList>", folder.display())).unwrap();
                std::fs::write(tmp.join("host"), "").unwrap();
                std::fs::write(tmp.join("gpsp_libretro.so"), "bundled core").unwrap();
                std::fs::write(tmp.join("data/controls-card-seen"), "gba\n").unwrap();
                let states = tmp.join("states/gba");
                std::fs::create_dir_all(&states).unwrap();
                if let Some(suffix) = suffix {
                    std::fs::write(states.join(format!("Clock Demo{suffix}")), b"old mGBA moment retained").unwrap();
                }
                cmd.env("POCKETUI_DEVICE", "rg35xx-plus").env("POCKETUI_HOST_BIN", tmp.join("host"))
                    .env("POCKETUI_GBA_CORE", tmp.join("gpsp_libretro.so")).env("POCKETUI_STATES", tmp.join("states"));
            });
            ui.sees("Game Boy Advance");
            ui.send("a");
            ui.sees("Clock Demo");
            ui.send("a");
            ui.wait(|d| d.card.tmp.join("run-game.started").exists());
            assert!(ui.log().contains(&format!("(emulator pocket-core, core {expected})")), "{}", ui.log());
            if let Some(suffix) = suffix {
                assert_eq!(std::fs::read(ui.card.tmp.join(format!("states/gba/Clock Demo{suffix}"))).unwrap(), b"old mGBA moment retained");
            }
        }
    }

    /// Seen on a device: Y on Easy's shelf moved the selection to the favourite's copy in front.
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_a_star_in_just_play_leaves_the_game_where_it_stands() {
        let mut ui = Driver::start("just-play-star", true, |tmp, _| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"big"}"#).unwrap();
        });
        ui.sees("Quick settings");
        ui.send("right");
        let before = ui.sees(" of ");
        let place = before.lines().find(|l| l.contains(" of ")).unwrap().to_string();
        ui.send("y");
        ui.sees("Added to Favourites");
        let after = ui.frame();
        assert!(after.contains(&place), "still {place}: {after}");
        std::thread::sleep(Duration::from_secs(2));
        let later = ui.frame();
        assert!(later.contains(&place), "still {place} when all has settled: {later}");
    }

    /// Quick settings are HOME's on Easy's shelf, as on Home in the other looks; B does nothing there and no hint names it.
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_on_the_easy_shelf_home_opens_quick_settings_and_b_does_nothing() {
        let mut ui = Driver::start("easy-home", true, |tmp, _| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"big"}"#).unwrap();
        });
        let shelf = ui.sees("Quick settings");
        assert!(shelf.contains("HOME") && !shelf.lines().any(|l| l == "B"), "{shelf}");
        ui.send("b");
        std::thread::sleep(Duration::from_millis(500));
        let after = ui.frame();
        assert!(after.contains("Quick settings") && after.contains("Settings"), "B leaves the shelf alone: {after}");
        ui.send("home");
        let quick = ui.sees("Brightness");
        assert!(!quick.contains("Settings"), "the panel has no shelf hint (\"Quick settings\" is its title): {quick}");
        ui.send("b");
        ui.sees("Settings");
    }

    /// Easy has no game switcher, on purpose: SELECT on its shelf, and HOME + SELECT or SELECT in the pause screen in
    /// a game, do nothing. The other looks keep all three (docs/BIG-SHELF.md "No game switcher").
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_just_play_has_no_game_switcher_and_the_other_looks_keep_theirs() {
        for (look, switcher) in [("big", false), ("shelf", true)] {
            let mut ui = Driver::start(&format!("switcher-{look}"), true, |tmp, _| {
                std::fs::write(tmp.join("data/settings.json"), format!(r#"{{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"{look}"}}"#)).unwrap();
            });
            ui.sees("Settings");
            // SELECT on the shelf.
            ui.send("select");
            if switcher {
                ui.sees("Nothing played yet");
                ui.send("b");
                ui.wait(|d| !d.frame().contains("Nothing played yet"));
            } else {
                std::thread::sleep(Duration::from_millis(600));
                let text = ui.frame();
                assert!(!text.contains("Nothing played yet") && !text.contains("No recent games"), "{text}");
                assert!(text.contains("Quick settings"), "still the shelf: {text}");
            }
            // A game, then the pause screen and SELECT there.
            ui.send(if switcher { "a\na" } else { "a" });
            ui.wait(|d| matches!(std::fs::read_to_string(d.card.tmp.join("run-game.started")).as_deref(), Ok("started\n")));
            ui.send("home");
            ui.sees("Continue");
            ui.send("select");
            if switcher {
                ui.sees("Now playing");
            } else {
                std::thread::sleep(Duration::from_millis(600));
                let text = ui.frame();
                assert!(!text.contains("Now playing") && text.contains("Continue"), "{text}");
            }
            // HOME held and SELECT pressed in the running game: back to the game first.
            ui.send(if switcher { "b\nb" } else { "a" });
            ui.wait(|d| !d.frame().contains("Continue"));
            ui.send("homedown\nselect");
            if switcher {
                ui.sees("Now playing");
            } else {
                std::thread::sleep(Duration::from_millis(600));
                let text = ui.frame();
                assert!(!text.contains("Now playing") && !text.contains("Continue"), "nothing opened: {text}");
            }
            ui.send("homeup");
        }
    }

    #[test]
    fn a_stale_preview_never_holds_a_fresh_start_for_a_reminder() {
        // `remind`: Settings → Playing → Remind me where I was is On; the last case is a player who never touched it
        // (Off since 0.5): a moment after a long break starts at once, with no card and no hold.
        for (emulator, state, early, remind) in [("pocket-core", false, false, true), ("pocket-core", true, false, true), ("retroarch", false, false, true), ("retroarch", true, false, true), ("pocket-core", true, true, true), ("pocket-core", true, false, false), ("retroarch", true, false, false)] {
            let mut ui = Driver::start("reminder-moment", true, |tmp, cmd| {
                let cfg = tmp.join("es_systems.cfg");
                let text = std::fs::read_to_string(&cfg).unwrap().replace("</command>", "</command><emulators><emulator name=\"retroarch\"><cores><core default=\"true\">gambatte</core></cores></emulator></emulators>");
                std::fs::write(cfg, text).unwrap();
                let game = tmp.join("gb/Tobu Tobu Girl (World).gb");
                let on = if remind { r#","where_was_i":true"# } else { "" };
                std::fs::write(tmp.join("data/settings.json"), format!(r#"{{"onboarded":true,"seen_version":"9.9.9","theme":"split","motion":"off","art":"off"{on}}}"#)).unwrap();
                let recent = serde_json::json!([{"system":"gb", "name":"Tobu Tobu Girl", "path":game,
                    "last_played": pocket_ui::recents::now_secs() - 23 * 86400, "play_secs":7200}]);
                std::fs::write(tmp.join("data/recents.json"), recent.to_string()).unwrap();
                let states = tmp.join("states/gb");
                std::fs::create_dir_all(&states).unwrap();
                let suffix = if emulator == "pocket-core" { "pstate.auto" } else { "state.auto" };
                let auto = states.join(format!("Tobu Tobu Girl (World).{suffix}"));
                if state {
                    std::fs::write(&auto, b"moment").unwrap();
                }
                let preview = PathBuf::from(format!("{}.png", auto.display()));
                let mut encoder = png::Encoder::new(std::fs::File::create(preview).unwrap(), 1, 1);
                encoder.set_color(png::ColorType::Rgb);
                encoder.write_header().unwrap().write_image_data(&[0x12, 0x34, 0x56]).unwrap();
                if early {
                    cmd.env("POCKETUI_TEST_PREPARE_MS", "300").env("POCKETUI_TRACE", tmp.join("trace"));
                }
                cmd.env("POCKETUI_STATES", tmp.join("states"));
                cmd.env("TMPDIR", tmp);
                if emulator == "pocket-core" {
                    std::fs::write(tmp.join("host"), "").unwrap();
                    cmd.env("POCKETUI_HOST_BIN", tmp.join("host"));
                } else {
                    cmd.env("POCKETUI_HOST_BIN", tmp.join("missing-host"));
                }
                // Capture what the actual launch asks the emulator to hold; the stand-in stays alive for the frame.
                executable(&tmp.join("run-game"), r#"#!/bin/sh
printf '%s\n' "${POCKET_LAUNCH_HOLD_UNTIL-}" "${POCKET_LAUNCH_HOLD_STILL-}" > "$0.hold"
printf 'started\n' > "$0.started"
sleep 30
"#);
            });
            ui.sees("Tobu Tobu Girl");
            ui.send("a");
            if early {
                ui.wait(|d| std::fs::read_to_string(d.card.tmp.join("trace")).unwrap_or_default().contains("launch preparation begin"));
                ui.send("volup");
            }
            ui.wait(|d| d.card.tmp.join("run-game.started").exists());
            let hold = std::fs::read_to_string(ui.card.tmp.join("run-game.hold")).unwrap();
            assert_eq!(!hold.trim().is_empty(), state && !early && remind && emulator == "pocket-core", "{emulator}, state {state}, remind {remind}: {hold:?}");
            if early {
                assert!(!ui.frame().contains("Last played"), "an early button must not reopen the reminder after preparation");
                assert!(ui.card.tmp.join("pocket-launch-skip").exists());
            } else if state && remind {
                ui.sees("Last played 3 weeks ago");
                ui.send("volup");
                ui.wait(|d| !d.frame().contains("Last played"));
                assert!(ui.card.tmp.join("pocket-launch-skip").exists(), "even a volume key dismisses the reminder and releases the game");
            } else {
                assert!(!ui.frame().contains("Last played"), "{emulator}: no reminder on a fresh start, or with the reminder Off");
            }
        }
    }

    /// An open case (a kept moment, `app_frozen`) started on Cozy plays Cozy's whole scene: the game's window is held for it
    /// and the moment stands still behind it (`startup::HOLD_PAUSE`), so nothing of it runs unseen. Quick keeps the
    /// instant way back. Until 14.11. an open case always started the quick way, whatever Animations said: the cartridge
    /// flashed in for 0.3 s and the game covered it, while the way back out played Cozy's (a player's report, RG34XXSP).
    #[test]
    fn an_open_case_on_cozy_plays_the_whole_insert_and_its_moment_waits_behind_it() {
        for animations in ["cozy", "quick"] {
            let mut ui = Driver::start("open-case-cozy", true, |tmp, cmd| {
                let cfg = tmp.join("es_systems.cfg");
                let text = std::fs::read_to_string(&cfg).unwrap().replace("</command>", "</command><emulators><emulator name=\"retroarch\"><cores><core default=\"true\">gambatte</core></cores></emulator></emulators>");
                std::fs::write(cfg, text).unwrap();
                let game = tmp.join("gb/Tobu Tobu Girl (World).gb");
                let settings = format!(r#"{{"onboarded":true,"seen_version":"9.9.9","theme":"shelf","motion":"full","animations":"{animations}","art":"off"}}"#);
                std::fs::write(tmp.join("data/settings.json"), settings).unwrap();
                // Played an hour ago: an open case, too soon for "Where was I?"'s reminder.
                let recent = serde_json::json!([{"system":"gb", "name":"Tobu Tobu Girl", "path":game,
                    "last_played": pocket_ui::recents::now_secs() - 3600, "play_secs":7200}]);
                std::fs::write(tmp.join("data/recents.json"), recent.to_string()).unwrap();
                let states = tmp.join("states/gb");
                std::fs::create_dir_all(&states).unwrap();
                std::fs::write(states.join("Tobu Tobu Girl (World).pstate.auto"), b"moment").unwrap();
                cmd.env("POCKETUI_STATES", tmp.join("states"));
                cmd.env("TMPDIR", tmp);
                std::fs::write(tmp.join("host"), "").unwrap();
                cmd.env("POCKETUI_HOST_BIN", tmp.join("host"));
                executable(&tmp.join("run-game"), r#"#!/bin/sh
printf '%s\n' "${POCKET_LAUNCH_HOLD_UNTIL-}" "${POCKET_LAUNCH_HOLD_PAUSE-}" > "$0.hold"
printf 'started\n' > "$0.started"
sleep 30
"#);
            });
            // The case stands open: A continues the game.
            ui.sees("Continue");
            ui.send("a");
            ui.wait(|d| d.card.tmp.join("run-game.started").exists());
            let hold = std::fs::read_to_string(ui.card.tmp.join("run-game.hold")).unwrap();
            let mut lines = hold.lines();
            let (until, pause) = (lines.next().unwrap_or_default(), lines.next().unwrap_or_default());
            if animations == "cozy" {
                let log = std::fs::read_to_string(ui.card.tmp.join("ui.log")).unwrap_or_default();
                let log: Vec<&str> = log.lines().filter(|l| l.contains("launch") || l.contains("frozen") || l.contains("settings")).collect();
                assert!(!until.is_empty(), "Cozy: the window waits for the scene ({hold:?}); {log:?}");
                assert_eq!(pause, "1", "and the moment stands still meanwhile ({hold:?})");
            } else {
                assert_eq!((until, pause), ("", ""), "Quick: straight back, nothing held ({hold:?})");
            }
        }
    }

    #[test]
    fn start_a_mock_game_and_quit_back_to_the_library() {
        let mut ui = Driver::new("play-and-quit", true);
        ui.sees("Game Boy");
        ui.send("a\na");
        ui.wait(|d| matches!(std::fs::read_to_string(d.card.tmp.join("run-game.started")).as_deref(), Ok("started\n")));
        assert_eq!(std::fs::read_to_string(ui.card.tmp.join("run-game.started")).unwrap(), "started\n");
        ui.send("home");
        let menu = ui.sees("Quit");
        assert!(menu.contains("Tobu Tobu Girl"), "the pause menu must name the launched game: {menu}");
        // The menu wraps around: Up from Continue is Quit, one press.
        ui.send("up\na");
        let mut shelf = String::new();
        ui.wait(|d| {
            shelf = d.frame();
            shelf.contains("Tobu Tobu Girl") && !shelf.contains("Quit") && !shelf.contains("Continue")
        });
        assert!(shelf.contains("Game Boy"), "{shelf}");
        // The stand-in has no core command pipe/window; the ordinary bounded Quit escalation ends it.
        ui.wait(|d| matches!(std::fs::read_to_string(d.card.tmp.join("run-game.quit")).as_deref(), Ok("quit\n")));
        assert_eq!(
            std::fs::read_to_string(ui.card.tmp.join("run-game.quit")).unwrap(),
            "quit\n",
            "the stand-in game actually exited"
        );
    }

    /// The whole way of "This game": card (X), START, change the frame, B, play, and the file the game starts with says the
    /// same: pocket-core reads it with `GameSettings::load`, as the pause menu does (one source of truth).
    #[test]
    fn this_game_changes_the_frame_from_the_card_and_the_game_starts_with_it() {
        let mut ui = Driver::new("this-game", true);
        ui.sees("Game Boy");
        // Into the system's list, X for the card, START for the page: two presses from the list.
        ui.send("a\nx");
        // The card or the 3D case; START opens This game from both (the case's bar shows SELECT first, review 19.11. #4).
        ui.wait_frame(|f| f.contains("START") || f.contains("Turn over"));
        ui.send("start");
        let page = ui.sees("This game · Game Boy");
        for row in ["Screen scaling", "Look", "Screen frame"] {
            assert!(page.contains(row), "{row} is missing: {page}");
        }
        // Scaling, look, then the frame: one to the right is Auto.
        ui.send("down\ndown\nright");
        // The value between its drawn arrows (`kit::row_value`), recorded as "‹" and "›".
        ui.sees("\u{2039}\nAuto\n\u{203a}");
        let rom = ui.card.tmp.join("gb/Tobu Tobu Girl (World).gb");
        let data = ui.card.tmp.join("data");
        let file = data.join("game-settings/gb/Tobu Tobu Girl (World).json");
        ui.wait(|_| file.exists());
        assert!(std::fs::read_to_string(&file).unwrap().contains("\"frame\": \"auto\""), "{}", std::fs::read_to_string(&file).unwrap());
        let loaded = pocket_ui::options::GameSettings::load(&data, "gb", &rom);
        assert_eq!(loaded.effective()["frame"], "auto");
        assert_eq!(loaded.level, pocket_ui::options::Level::Game);
        // Back to the card, play: the game starts, and the page still says the same afterwards.
        ui.send("b");
        ui.sees("Tobu Tobu Girl");
        ui.send("a");
        ui.wait(|d| matches!(std::fs::read_to_string(d.card.tmp.join("run-game.started")).as_deref(), Ok("started\n")));
        ui.send("home");
        ui.sees("Quit");
        ui.send("up\na");
        ui.wait(|d| matches!(std::fs::read_to_string(d.card.tmp.join("run-game.quit")).as_deref(), Ok("quit\n")));
        ui.sees("Game Boy");
        ui.send("x");
        ui.wait_frame(|f| f.contains("START") || f.contains("Turn over"));
        ui.send("start\ndown\ndown");
        let again = ui.sees("\u{2039}\nAuto\n\u{203a}");
        assert!(again.contains("This game has its own settings."), "{again}");
    }

    /// The frames are named after the consoles they draw (DMG, GB Pocket, GBC …), a console's own frames come only with it,
    /// and each name and its finish row stand whole on the page, none cut short.
    #[test]
    fn this_game_frame_names_are_whole_and_belong_to_the_console() {
        let mut ui = Driver::new("frame-names", true);
        ui.sees("Game Boy");
        ui.send("a\nx");
        // The card or the 3D case; START opens This game from both (the case's bar shows SELECT first, review 19.11. #4).
        ui.wait_frame(|f| f.contains("START") || f.contains("Turn over"));
        ui.send("start\ndown\ndown");
        // Game Boy: none, then Auto, Ambient, the three Game Boys, then the TVs; no GBA, DS or PSP between.
        for (name, row) in [("Auto", None), ("Ambient", None), ("DMG", Some("DMG colour")), ("GB Pocket", Some("GB Pocket finish")), ("GBC", Some("GBC colour")), ("CRT", Some("TV cabinet"))] {
            ui.send("right");
            let page = ui.sees(&format!("\u{2039}\n{name}\n\u{203a}"));
            if let Some(row) = row {
                assert!(page.contains(row), "{row} is missing under {name}: {page}");
            }
            assert!(!page.contains('\u{2026}'), "{name}: something is cut short: {page}");
        }
    }

    /// Every `send` is its own open/write/close on the FIFO. The UI must lose none of them and never hand the writer a
    /// "Broken pipe" in between (the FIFO used to reach EOF after each writer and was reopened; flaky under load).
    #[test]
    fn separate_fifo_writes_never_lose_a_command_or_break_the_pipe() {
        let mut ui = Driver::new("fifo-writes", true);
        // Back to back, as `send` then `sees` do in the journeys: the next writer opens right after the last one closed.
        for _ in 0..2000 {
            ui.send("up");
        }
        ui.frame();
    }

    fn controller_words(ui: &mut Driver) -> String {
        ui.frame().split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn controller_sees(ui: &mut Driver, expected: &str) -> String {
        let mut words = String::new();
        ui.wait(|ui| {
            words = controller_words(ui);
            words.contains(expected)
        });
        words
    }

    fn controller_capture_page() -> Driver {
        let mut ui = Driver::new("controller-hotplug", true);
        ui.send("tconnect 11 builtin Pocket\ntconnect 12 bt:freeze-proof Pro Controller\nstart");
        controller_sees(&mut ui, "Wi-Fi"); // START: Connections' rows
        ui.send("r1\nr1\nr1"); // Controls
        controller_sees(&mut ui, "Test buttons");
        ui.send("down\na"); // Controller setup, the row under Buttons
        controller_sees(&mut ui, "Choose a controller");
        ui.send("right"); // external pad, header selected
        controller_sees(&mut ui, "Pro Controller");
        ui.send("down\na"); // capture the bottom button
        controller_sees(&mut ui, "HOME on any pad cancels");
        ui
    }

    /// A controller vanishes in the capture page, returns under a fresh source, and can be set up again. Frames,
    /// the Pocket's B and the timer must still run; an abandoned capture must never write a layout.
    #[test]
    fn controller_capture_survives_disconnect_and_reconnect() {
        let mut ui = controller_capture_page();
        ui.send("tdisconnect 12");
        controller_sees(&mut ui, "Controller disconnected. Reconnect and try again.");
        assert!(!ui.card.tmp.join("data/controllers.json").exists());
        ui.send("tconnect 13 bt:freeze-proof Pro Controller\na");
        controller_sees(&mut ui, "HOME on any pad cancels");
        ui.send("tbtn 11 b down\ntbtn 11 b up");
        assert!(!controller_words(&mut ui).contains("HOME on any pad cancels"));
        ui.send("a");
        controller_sees(&mut ui, "HOME on any pad cancels");
        controller_sees(&mut ui, "No button pressed. Nothing changed.");
        assert!(!ui.card.tmp.join("data/controllers.json").exists());
        ui.send("a\ntbtn 13 l2 down\ntbtn 13 l2 up");
        controller_sees(&mut ui, "Buttons swapped. Every action still has a button.");
        let profiles = pocket_ui::pad::bindings::Profiles::load(&ui.card.tmp.join("data"));
        assert_eq!(profiles.layout("bt:freeze-proof").role(pocket_ui::pad::Button::L2), pocket_ui::pad::Button::South);
        // B: back to its own row in Controls (it was a row of Buttons until the tree of 2026-11-13).
        ui.send("b");
        controller_sees(&mut ui, "Test buttons");
    }

    #[test]
    #[ignore = "requires root and /dev/uinput; creates only its own gamepad and event node"]
    fn uinput_disconnect_during_controller_capture_keeps_ui_alive() {
        use evdev::{uinput::VirtualDevice, AttributeSet, Device, InputEvent, KeyCode};
        use pocket_ui::pad::{Button, PadEvent};
        struct Pad {
            dev: Option<VirtualDevice>, node: std::path::PathBuf, created: bool,
            rx: std::sync::mpsc::Receiver<PadEvent>, reader: Option<std::thread::JoinHandle<pocket_ui::pad::PumpEnd>>,
        }
        impl Pad {
            fn new(source: usize) -> Self {
                use std::os::unix::ffi::OsStrExt;
                // Capabilities saved from the Lite 2 in S mode: KEY=ffff000000000000 0 0 0 0,
                // ABS=3001b, bus 0005, VID/PID 057e:2009, version 0001 (hid-generic).
                let mut builder = VirtualDevice::builder().unwrap().name("Pro Controller")
                    .input_id(evdev::InputId::new(evdev::BusType::BUS_BLUETOOTH, 0x057e, 0x2009, 1))
                    .with_keys(&AttributeSet::from_iter((304..=319).map(KeyCode))).unwrap();
                for code in [0, 1, 3, 4, 16, 17] {
                    let (lo, hi) = if code >= 16 { (-1, 1) } else { (-32767, 32767) };
                    builder = builder.with_absolute_axis(&evdev::UinputAbsSetup::new(evdev::AbsoluteAxisCode(code), evdev::AbsInfo::new(0, lo, hi, 0, 0, 0))).unwrap();
                }
                let mut dev = builder.build().unwrap();
                let node = dev.enumerate_dev_nodes_blocking().unwrap().next().unwrap().unwrap();
                let created = !node.exists();
                if created {
                    let ids = std::fs::read_to_string(dev.get_syspath().unwrap().join(node.file_name().unwrap()).join("dev")).unwrap();
                    let (major, minor) = ids.trim().split_once(':').unwrap();
                    let name = std::ffi::CString::new(node.as_os_str().as_bytes()).unwrap();
                    assert_eq!(unsafe { libc::mknod(name.as_ptr(), libc::S_IFCHR | 0o600, libc::makedev(major.parse().unwrap(), minor.parse().unwrap())) }, 0);
                }
                let input = Device::open(&node).unwrap();
                let (tx, rx) = std::sync::mpsc::channel();
                let reader = std::thread::spawn(move || pocket_ui::pad::linux::pump(source, input, |e| tx.send(e).is_ok()));
                Self { dev: Some(dev), node, created, rx, reader: Some(reader) }
            }
            // Only the FIFO transport is synthetic: these events were decoded by our real reader from evdev.
            fn forward(&self, ui: &mut Driver, until: impl Fn(&PadEvent) -> bool) {
                loop {
                    let e = self.rx.recv_timeout(Duration::from_secs(2)).expect("evdev reader responded");
                    match &e {
                        PadEvent::Button { source, button: Button::L2, pressed } => ui.send(&format!("tbtn {source} l2 {}", if *pressed { "down" } else { "up" })),
                        PadEvent::Disconnected { source } => ui.send(&format!("tdisconnect {source}")),
                        _ => {}
                    }
                    if until(&e) { return; }
                }
            }
        }
        impl Drop for Pad {
            fn drop(&mut self) {
                self.dev.take();
                if let Some(reader) = self.reader.take() { reader.join().unwrap(); }
                if self.created { let _ = std::fs::remove_file(&self.node); }
            }
        }
        let mut ui = controller_capture_page();
        let mut pad = Pad::new(12);
        // Disappear while the real page is waiting. The kernel wakes the blocked reader with ENODEV.
        pad.dev.take();
        pad.forward(&mut ui, |e| matches!(e, PadEvent::Disconnected { .. }));
        controller_sees(&mut ui, "Controller disconnected. Reconnect and try again.");
        assert!(!ui.card.tmp.join("data/controllers.json").exists());
        drop(pad);
        let mut pad = Pad::new(13);
        ui.send("tconnect 13 bt:freeze-proof Pro Controller\na");
        controller_sees(&mut ui, "HOME on any pad cancels");
        // SDL's matching record binds L2 to b6: with this descriptor, code 310, not BTN_TL2.
        pad.dev.as_mut().unwrap().emit(&[InputEvent::new(1, KeyCode::BTN_TL.0, 1)]).unwrap();
        pad.forward(&mut ui, |e| matches!(e, PadEvent::Button { button: Button::L2, pressed: true, .. }));
        controller_sees(&mut ui, "Buttons swapped. Every action still has a button.");
        pad.dev.take(); // disconnect with L2 held: its release must precede Disconnected
        pad.forward(&mut ui, |e| matches!(e, PadEvent::Button { button: Button::L2, pressed: false, .. }));
        pad.forward(&mut ui, |e| matches!(e, PadEvent::Disconnected { .. }));
        // B: back to its own row in Controls (it was a row of Buttons until the tree of 2026-11-13).
        ui.send("b");
        controller_sees(&mut ui, "Test buttons");
    }

    /// Settings → Controls → Test buttons: every button is a test, B, START and HOME too (they must not leave the
    /// page, open quick settings or the Settings list), two controllers keep their own lights, a trigger shows its travel,
    /// and the page is left by holding HOME for two seconds, by B and START together, or by a tap on its hint.
    /// Controllers and their presses come through the driver's own verbs (`tconnect`, `tbtn`, `taxis`), with sources of
    /// their own as the pad threads have them.
    #[test]
    fn the_button_tester_takes_every_button_and_is_left_by_a_hold() {
        let mut ui = Driver::new("test-buttons", true);
        ui.send("start");
        ui.sees("Wi-Fi");
        // Controls, the fourth category (R1 three times from Connections); its third row is Test buttons (after Buttons and
        // Controller setup).
        ui.send("r1\nr1\nr1");
        ui.sees("Test buttons");
        ui.send("down\ndown\na");
        // The keys sent so far came from no controller (the command FIFO): the Pocket's own buttons stand for them.
        ui.sees("Player 1 · Built-in");
        ui.send("tconnect 11 builtin Pocket\ntconnect 12 bt:e4:17:d8:00:00:02 8BitDo Pro 2");
        let both = ui.sees("Player 2 · Bluetooth");
        assert!(both.contains("Pocket") && both.contains("Player 1 · Built-in"), "{both}");
        // B, START, SELECT and HOME on the Pocket: each one is a test, none does what it does elsewhere.
        for (button, words) in [("b", "In menus: B"), ("start", "In menus: START"), ("select", "In menus: SELECT"), ("home", "Keep holding to leave")] {
            ui.send(&format!("tbtn 11 {button} down"));
            let text = ui.sees(words);
            assert!(text.contains("Test buttons") && text.contains("Hold to leave"), "{button} stays on the page: {text}");
            ui.send(&format!("tbtn 11 {button} up"));
            let text = ui.frame();
            assert!(text.contains("Hold to leave") && !text.contains("Brightness"), "{button} opened nothing: {text}");
        }
        // A press on the second controller moves the drawing to it, and its trigger shows the travel.
        ui.send("taxis 12 r2 0.8\ntbtn 12 r2 down");
        let text = ui.sees("80%");
        assert!(text.contains("Player 2 · 8BitDo Pro 2") && text.contains("In menus: R2"), "{text}");
        ui.send("tbtn 12 r2 up\ntaxis 12 r2 0");
        // HOME held a moment and let go stays; held for two seconds it leaves, to the row it was opened from.
        ui.send("tbtn 11 home down");
        ui.sees("Keep holding to leave");
        ui.send("tbtn 11 home up");
        let text = ui.frame();
        assert!(text.contains("Hold to leave") && !text.contains("Keep holding"), "{text}");
        ui.send("tbtn 11 home down");
        ui.sees("watch them light up");
        ui.send("tbtn 11 home up");
        // Back in the list: Test buttons selected, with its line under it. The chain below goes in again.
        ui.send("a");
        ui.sees("Hold to leave");
        // B and START together on one controller leave as well.
        ui.send("tbtn 11 b down\ntbtn 11 start down");
        ui.wait(|d| !d.frame().contains("Hold to leave"));
        ui.send("tbtn 11 start up\ntbtn 11 b up");
        // And a tap on the hint at the bottom right, for the touch screen.
        ui.send("a");
        ui.sees("Hold to leave");
        ui.send("tap 0.9 0.95");
        ui.wait(|d| !d.frame().contains("Hold to leave"));
    }

    /// The Cabinet look, then Settings over it: a tap on a category is the category's, never a case's that stood
    /// there a frame before (the cases' tap areas caught it and confirmed the selected row; review 2026-10-30).
    /// Taps come through the command FIFO ("tap x y", fractions of the screen) as the touch screen sends them.
    #[test]
    fn a_tap_on_settings_over_the_cabinet_is_settings() {
        let mut ui = Driver::start("cabinet-tap", true, |tmp, _| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"cabinet"}"#).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("start\nr1");
        ui.sees("Appearance");
        // Look's rows on the driver's 1240x1080 window: their middles at 213 + 66 px each (under the category's name on
        // its own line: the row of symbols with a whole name doesn't fit beside the widest status pill there). Taps across the whole page,
        // never twice in a row on the same row nor first on the selected one (that would open or switch it), so wherever a
        // case stood one of them lands on it; each only selects its row. A frame between taps, as on the device.
        let row = |g: usize| (213.0 + 66.0 * g as f32) / 1080.0;
        for x in [0.1, 0.3, 0.5, 0.7, 0.9] {
            for g in [2, 4, 6, 1, 3, 5, 0] {
                ui.send(&format!("tap {x} {:.4}", row(g)));
                ui.frame();
            }
        }
        // Then Cozy extras, the eighth (Zoom in the pause menu went to Playing in 0.5.1): the first tap selects it, the
        // second opens it.
        ui.send(&format!("tap 0.5 {:.4}", row(7)));
        ui.frame();
        ui.send(&format!("tap 0.5 {:.4}", row(7)));
        let text = ui.sees("TV on the cabinet");
        assert!(!text.contains("Easy"), "the Layout page (the row that was selected) did not open: {text}");
    }

    /// The cozy extras where they live since 2026-10-30 answer a finger as every Settings row does: a tap selects a
    /// row, the next one on it switches it. Look → Cozy extras → Playful cases and Stickers on cases and
    #[test]
    fn a_tap_switches_the_cozy_extras_where_they_live_now() {
        let mut ui = Driver::new("extras-tap", true);
        ui.send("start\nr1");
        ui.sees("Cozy extras");
        // On the driver's 1240x1080 window the rows' middles are at 165 + 66 px each on a page with a title, 48 px lower
        // in a category's list (under the category's name line).
        let tap_at = |ui: &mut Driver, top: f32, r: usize| {
            ui.send(&format!("tap 0.5 {:.4}", (top + 66.0 * r as f32) / 1080.0));
            ui.frame();
        };
        let tap = |ui: &mut Driver, r: usize| tap_at(ui, 165.0, r);
        let settings = ui.card.tmp.join("data/settings.json");
        let extra = move |key: &str| -> serde_json::Value {
            let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap_or_default()).unwrap_or_default();
            s["extras"][key].clone()
        };
        // Look (R1 from where START opens): its eighth row Cozy extras (since 0.5.1): the page (since the tree of 2026-11-13 with
        // Playful cases on it, its fifth row since TV plays the game); Stickers on cases, its third row: select, switch.
        tap_at(&mut ui, 213.0, 7);
        tap_at(&mut ui, 213.0, 7);
        ui.sees("TV on the cabinet");
        tap(&mut ui, 4);
        tap(&mut ui, 4);
        ui.wait(|_| extra("playful_animations").is_boolean());
        tap(&mut ui, 2);
        tap(&mut ui, 2);
        ui.wait(|_| extra("case_stickers") == serde_json::json!(false));
    }

    /// The symbols at the top of Settings answer a finger (Android's handheld view, a touch screen): a tap on R1 is R1, the
    /// next category; a tap on a symbol goes to its category (SETTINGS-IA package 2). Where they are comes from the
    /// navigation record (L1's and R1's keys are hints there); the first symbol stands right after L1.
    #[test]
    fn a_tap_on_r1_or_on_a_symbol_changes_the_category() {
        let mut d = Driver::start("settings-tabs-tap", true, |tmp, cmd| {
            cmd.env("POCKETUI_NAV_PROBE", tmp.join("nav.txt"));
        });
        d.sees("Game Boy");
        d.send("start");
        d.wait(|d| status_of(d).contains("\tplace=settings:List Connections\t"));
        let record = nav_record(&mut d);
        let hint = |key: &str| -> [i32; 4] {
            record.lines().find_map(|l| {
                let v: Vec<&str> = l.splitn(6, '\t').collect();
                (v.get(4).copied() == Some(&format!("‹nav:hint›{key}")[..])).then(|| [v[0].parse().unwrap(), v[1].parse().unwrap(), v[2].parse().unwrap(), v[3].parse().unwrap()])
            }).unwrap_or_else(|| panic!("no {key} hint at the top:\n{record}"))
        };
        let (l1, r1) = (hint("L1"), hint("R1"));
        let tap = |d: &mut Driver, x: i32, y: i32| d.send(&format!("tap {:.4} {:.4}", x as f32 / 1240.0, y as f32 / 1080.0));
        // The driver paints only when asked: `sees` asks for frames until the page is there (and the status says so).
        tap(&mut d, (r1[0] + r1[2]) / 2, (r1[1] + r1[3]) / 2);
        d.sees("Appearance");
        assert!(status_of(&d).contains("\tplace=settings:List Look\t"), "{}", status_of(&d));
        // The first symbol, Connections: 16 px after L1's key, 32 px wide.
        tap(&mut d, l1[2] + 16 + 16, (l1[1] + l1[3]) / 2);
        d.sees("Remote access (SSH)");
        assert!(status_of(&d).contains("\tplace=settings:List Connections\t"), "{}", status_of(&d));
    }

    #[test]
    fn quick_wifi_off_and_on_agrees_with_the_wifi_page() {
        let mut ui = Driver::new("wifi", true);
        // The selected switch stands between its ‹ › (Left/Right switch it too, 0.5.1).
        ui.send("home\ndown\ndown\na");
        ui.sees("Wi-Fi\n‹\nOff\n›");
        ui.send("b\nstart\na");
        let off = ui.sees("Wi-Fi is off");
        assert!(
            !off.contains("Connected") && !off.contains("Connect\n"),
            "off must not claim a connection: {off}"
        );
        // Back from Wi-Fi and Settings, switch the same radio on again, then reopen the Wi-Fi page.
        ui.send("b\nb\nb\nhome\ndown\ndown\na");
        ui.sees("Wi-Fi\n‹\nOn\n›");
        ui.send("b\nstart\na");
        let mut text = String::new();
        ui.wait(|d| {
            text = d.frame();
            // The mock device has no nmcli: the scan cannot run, and the page says that (never "No networks found" for it).
            text.contains("Wi-Fi") && !text.contains("Wi-Fi is off") && (text.contains("Couldn't search for networks") || text.contains("No networks") || text.contains("Choose your network"))
        });
    }

    /// NetworkManager with iwd for one network "Wifi" whose router takes "wwwwwwww", and the few programs the Wi-Fi page
    /// asks after a connection (`ip`, `iw`, `curl`), on PATH in front of the real ones. A profile holds the last key typed
    /// (nmcli stores it whether or not the router takes it); while one with a refused key exists, iwd keeps retrying it
    /// and NetworkManager reports it "activating" without end, as on the fresh card of 2026-10-27.
    fn wifi_programs(tmp: &Path, cmd: &mut Command) {
        let bin = tmp.join("bin");
        std::fs::create_dir_all(bin.join("profiles")).unwrap();
        executable(&bin.join("nmcli"), r#"#!/bin/sh
base=$(dirname "$0")
printf '%s\n' "$*" >> "$base/nmcli.log"
uuid=$(ls "$base/profiles" | head -1)
key=$([ -n "$uuid" ] && cat "$base/profiles/$uuid")
state=none; [ -n "$uuid" ] && { [ "$key" = wwwwwwww ] && state=activated || state=activating; }
case "$*" in
  '-t -f SSID,SIGNAL,SECURITY,IN-USE dev wifi list --rescan yes')
    [ "$state" = none ] && echo 'Wifi:70:WPA2: ' || echo 'Wifi:70:WPA2:*'; echo 'Neighbour:30:WPA2: ' ;;
  '-t -f NAME,UUID,TYPE,ACTIVE,STATE connection show')
    [ -n "$uuid" ] && echo "Wifi:$uuid:802-11-wireless:yes:$state"; true ;;
  '-t -f UUID,TYPE connection show') [ -n "$uuid" ] && echo "$uuid:802-11-wireless"; true ;;
  '-g 802-11-wireless.ssid,802-11-wireless.hidden connection show uuid '*) printf 'Wifi\nno\n' ;;
  '-e no -g 802-11-wireless.ssid connection show uuid '*) [ -f "$base/profiles/$8" ] && echo Wifi || exit 10 ;;
  '-g GENERAL.STATE,GENERAL.CON-UUID,GENERAL.REASON device show wlan0')
    case $state in activated) printf '100 (connected)\n%s\n0 (No reason given)\n' "$uuid" ;;
      activating) printf '50 (connecting (configuring))\n%s\n0 (No reason given)\n' "$uuid" ;;
      *) printf '30 (disconnected)\n--\n0 (No reason given)\n' ;; esac ;;
  '-w 2 device disconnect wlan0') ;;
  '-w 60 device wifi connect Wifi password '*' ifname wlan0')
    [ -n "$uuid" ] || { uuid="uuid-$(date +%s%N)"; }
    printf '%s' "$8" > "$base/profiles/$uuid"
    [ "$8" = wwwwwwww ] && echo "Device 'wlan0' successfully activated with '$uuid'." && exit 0
    echo 'Error: Connection activation failed: Secrets were required, but not provided.' >&2; exit 4 ;;
  'connection modify '*) ;;
  'connection delete uuid '*)
    [ -f "$base/profiles/$4" ] || { echo "Error: cannot delete unknown connection(s): uuid '$4'." >&2; exit 10; }
    rm "$base/profiles/$4"; echo "Connection 'Wifi' ($4) successfully deleted." ;;
  *) exit 0 ;;
esac
"#);
        executable(&bin.join("ip"), r#"#!/bin/sh
[ -n "$(ls "$(dirname "$0")/profiles")" ] && [ "$(cat "$(dirname "$0")"/profiles/*)" = wwwwwwww ] && echo '3: wlan0    inet 192.0.2.23/24 brd 192.0.2.255 scope global wlan0'
true
"#);
        executable(&bin.join("iw"), "#!/bin/sh\necho 'global'\necho 'country AT: DFS-ETSI'\n");
        // The connectivity check finds no way out (curl's "couldn't connect"): the page has no sign-in row to add.
        executable(&bin.join("curl"), "#!/bin/sh\nexit 7\n");
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
        cmd.env("PATH", path);
    }

    /// First start on a fresh card, the home network's password typed wrong twice, then right, then forgotten twice:
    /// every step says plainly what happened (2026-10-27: "Activation failed", six minutes of "Connected" without an
    /// address, and "couldn't forget Wifi: cannot delete unknown connection").
    #[test]
    fn first_wifi_wrong_password_then_the_right_one_then_forget() {
        let mut ui = Driver::start("wifi-first-password", false, wifi_programs);
        ui.sees("Welcome to your Pocket");
        // The language page, A: the Wi-Fi step, which scans by itself.
        ui.send("a");
        ui.sees("Neighbour");
        for _ in 0..2 {
            ui.send("a");
            ui.sees("Password for Wifi");
            // The keyboard starts on "q": eight of them.
            ui.send(&("a\n".repeat(8) + "start"));
            let refused = ui.sees("Wrong password. Try again.");
            assert!(refused.contains("Password for Wifi"), "the keyboard is back for the next try: {refused}");
            assert!(!refused.contains("Connected"), "{refused}");
            let profiles = std::fs::read_dir(ui.card.tmp.join("bin/profiles")).unwrap().count();
            assert_eq!(profiles, 0, "no refused profile left for iwd to retry or for the list to offer as saved");
            assert!(refused.contains("qqqqqqqq"), "what was typed stays, to be corrected: {refused}");
            // Back to the list: the eight letters, then out. The network wants its password, it isn't "Saved" nor "Connecting…".
            ui.send(&"b\n".repeat(9));
            let list = ui.sees("Neighbour");
            assert!(!list.contains("Connecting") && !list.contains("Saved") && !list.contains("Connected"), "{list}");
        }
        // The right one: "w" is one to the right of "q".
        ui.send("a");
        ui.sees("Password for Wifi");
        ui.send(&("right\n".to_string() + &"a\n".repeat(8) + "start"));
        ui.sees("Connected to Wifi");
        // A player who waits: the first start goes on by itself a moment later (`ADVANCE_AFTER`). This waits for that
        // instead of racing it; a loaded gate's next press came after the moment and found the next page (14.11.).
        let next = ui.wait_frame(|f| !f.contains("Scan again"));
        assert!(!next.contains("Password for Wifi"), "{next}");
        // B goes back to the Wi-Fi step, which never goes on by itself again. Its list marks the network in use (and
        // saved: Y forgets), first in the list.
        ui.send("b");
        ui.sees("Wifi\nConnected");
        // Forget it, then a second Y Y on the same row: the first forgets, the second has nothing left to forget.
        ui.send(&("up\n".repeat(8) + "y\ny"));
        ui.sees("Forgot Wifi");
        ui.send("y\ny");
        let after = ui.frame();
        assert!(!after.contains("Couldn't forget"), "{after}");
        let log = std::fs::read_to_string(ui.card.tmp.join("bin/nmcli.log")).unwrap();
        assert_eq!(log.matches("connection delete").count(), 3, "two refused profiles and the one forgotten: {log}");
        assert!(!log.contains("connection up"), "nothing connected a saved profile by itself: {log}");
    }

    /// The Game Boy and Sega CD card plus one more system, its files a byte long (an empty one is a broken game).
    fn add_system(tmp: &Path, id: &str, name: &str, exts: &str, files: &[&str]) {
        let folder = tmp.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        for f in files {
            std::fs::write(folder.join(f), "x").unwrap();
        }
        let cfg = tmp.join("es_systems.cfg");
        let text = std::fs::read_to_string(&cfg).unwrap().replace(
            "</systemList>",
            &format!(
                "  <system><name>{id}</name><fullname>{name}</fullname><path>{}</path><extension>{exts}</extension><command>true</command></system>\n</systemList>",
                folder.display()
            ),
        );
        std::fs::write(cfg, text).unwrap();
    }

    /// Xbox is not hosted by pocket-core: HOME in the game opens the guest's menu (with the note that the emulator keeps
    /// its own saves), and Quit ends the stand-in and gets back to the Xbox list (monkey test 2026-10-29).
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_xbox_guest_starts_home_shows_its_menu_and_quit_gets_back() {
        let mut ui = Driver::start("xbox-guest", true, |tmp, _| {
            add_system(tmp, "xbox", "Xbox", ".iso .xbe", &["Halo - Combat Evolved (USA).iso"]);
        });
        ui.sees("Game Boy");
        ui.send("down\ndown\na");
        ui.sees("Halo - Combat Evolved");
        ui.send("a");
        ui.wait(|d| d.card.tmp.join("run-game.started").exists());
        // The launch animation of a game that is starting: HOME before its end is not the pause menu's yet.
        std::thread::sleep(Duration::from_secs(3));
        ui.send("home");
        let menu = ui.sees("Quit");
        assert!(menu.contains("Halo - Combat Evolved") && menu.contains("Continue"), "{menu}");
        // 0.7.2 (6be6d605c): Xbox stays on hakuX inside RetroArch (no silent xemu); its menu says save states are not there yet.
        assert!(menu.contains("No save states for Xbox games yet"), "the Xbox menu says how saving works: {menu}");
        // The menu takes buttons once it has settled (a loaded host got the presses too early).
        std::thread::sleep(Duration::from_millis(600));
        // The guest's menu wraps: Up from Continue is Quit (Down ×8 came back to Continue in this four-row menu).
        ui.send("up");
        std::thread::sleep(Duration::from_millis(200));
        ui.send("a");
        // A guest is asked to close first and ended after its grace period: the shelf is back, and the stand-in is gone
        // (it may be ended before its trap writes a marker, so the process is what is looked at).
        let mut shelf = String::new();
        let until = Instant::now() + test_clock::timeout(Duration::from_secs(40));
        loop {
            shelf = if Instant::now() < until { ui.frame() } else { panic!("no shelf after Quit:\n{shelf}\nUI log:\n{}", ui.log()) };
            if shelf.contains("Halo - Combat Evolved") && !shelf.contains("Quit") && !shelf.contains("Continue") {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let pid: i32 = ui.log().lines().find_map(|l| l.strip_prefix("test game pid ")?.parse().ok()).expect("the launch log names the game");
        let gone = (0..100).any(|_| {
            std::thread::sleep(Duration::from_millis(100));
            unsafe { libc::kill(pid, 0) != 0 }
        });
        assert!(gone, "the stand-in game still runs after Quit");
    }

    /// A game that never starts says so: the screen does not just fall back to the list (`fallback::not_started_notice`;
    /// pocket-core's own missing-core message is the unit test in `fallback.rs`, pocket-core is not on the host).
    #[test]
    fn a_game_that_never_starts_says_so() {
        let mut ui = Driver::start("never-starts", true, |tmp, _| {
            add_system(tmp, "xbox", "Xbox", ".iso .xbe", &["Broken Demo (USA).iso"]);
            executable(&tmp.join("run-game"), "#!/bin/sh\nexit 1\n");
        });
        ui.sees("Game Boy");
        ui.send("down\ndown\na");
        ui.sees("Broken Demo");
        ui.send("a");
        let text = ui.sees("didn't start");
        assert!(text.contains("Broken Demo"), "the notice names the game: {text}");
    }

    /// New-user test 2026-10-30: an empty file (a copy that never finished) is told as that, not as a missing BIOS.
    #[test]
    fn a_game_file_with_nothing_in_it_says_so_and_does_not_start() {
        let mut ui = Driver::start("empty-file", true, |tmp, _| {
            add_system(tmp, "xbox", "Xbox", ".iso .xbe", &["Empty Demo (USA).iso"]);
            std::fs::write(tmp.join("xbox/Empty Demo (USA).iso"), "").unwrap();
            executable(&tmp.join("run-game"), "#!/bin/sh\nexit 1\n");
        });
        ui.sees("Game Boy");
        ui.send("down\ndown\na");
        ui.sees("Empty Demo");
        ui.send("a");
        let text = ui.sees("is empty");
        assert!(text.contains("Empty Demo") && !text.contains("BIOS"), "the notice names the game and the real reason: {text}");
    }

    /// Just Play (docs/CURATOR.md): A on its row starts exactly one game from Home, nothing else in between. The card is
    /// the default one (Curator on, no history); the index is made on a thread, so A is pressed until a game is on its way.
    fn just_play_press(ui: &mut Driver) {
        let started = ui.card.tmp.join("run-game.started");
        for _ in 0..60 {
            ui.send("a");
            let until = Instant::now() + test_clock::timeout(Duration::from_millis(250));
            while Instant::now() < until {
                if started.exists() || ui.log().contains("curator: just play") || ui.log().contains("just play found no game") {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        panic!("Just Play never answered:\n{}", ui.log());
    }

    /// Home takes buttons after a failed start: down moves off the row (the notice covers the screen's text for its 8
    /// seconds, so the frame shows the system once it has gone). A game that started and ended puts Home back on its
    /// first row, a start that never happened leaves it on Just Play: down lands on a system either way.
    fn home_answers(ui: &mut Driver) {
        // The notice stays until A or B closes it; then Home answers the D-pad again.
        ui.send("b");
        ui.send("down");
        ui.wait(|d| {
            let text = d.frame();
            (text.contains("Game Boy") || text.contains("Sega CD")) && !text.contains("didn't start")
        });
    }

    fn launches(ui: &Driver) -> usize {
        ui.log().lines().filter(|l| l.starts_with("launch ") && l.contains("(emulator")).count()
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_one_press_on_just_play_starts_exactly_one_game() {
        let mut ui = Driver::new("just-play", true);
        ui.sees("Game Boy");
        // Home opens on the first system; Just Play is the row right above Continue (one press up).
        ui.send("up");
        let row = ui.sees("Just Play");
        assert!(row.contains("Less browsing. More playing."), "{row}");
        just_play_press(&mut ui);
        ui.wait(|d| d.card.tmp.join("run-game.started").exists());
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(launches(&ui), 1, "{}", ui.log());
        assert!(ui.log().contains("curator: just play"), "{}", ui.log());
        // The optional CJK font can be missing; its text-renderer diagnostic says nothing about game selection or retries.
        assert!(!ui.log().lines().any(|line| line.contains("fallback") && !line.starts_with("text: no fallback font for CJK (")), "{}", ui.log());
    }

    /// A game whose file is gone is passed over; when every file is gone it says so, starts nothing and Home stays usable.
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_just_play_passes_over_a_missing_file_and_says_so_when_none_is_left() {
        let mut ui = Driver::new("just-play-missing", true);
        ui.sees("Game Boy");
        ui.send("up");
        ui.sees("Just Play");
        std::fs::remove_file(ui.card.tmp.join("gb/Tobu Tobu Girl (World).gb")).unwrap();
        just_play_press(&mut ui);
        ui.wait(|d| d.card.tmp.join("run-game.started").exists());
        assert_eq!(launches(&ui), 1, "{}", ui.log());
        assert!(ui.log().contains("just play Pocket CD Demo") || ui.log().contains("segacd"), "the Sega CD game, not the missing one: {}", ui.log());
        assert!(!ui.log().contains("Tobu Tobu Girl (World).gb (emulator"), "{}", ui.log());

        let mut ui = Driver::new("just-play-none", true);
        ui.sees("Game Boy");
        ui.send("up");
        ui.sees("Just Play");
        std::fs::remove_file(ui.card.tmp.join("gb/Tobu Tobu Girl (World).gb")).unwrap();
        std::fs::remove_file(ui.card.tmp.join("segacd/Pocket CD Demo (USA).cue")).unwrap();
        just_play_press(&mut ui);
        let text = ui.sees("None of the games tried would start");
        assert!(text.contains("files may be gone"), "says why: {text}");
        std::thread::sleep(Duration::from_millis(600));
        assert!(!ui.card.tmp.join("run-game.started").exists() && launches(&ui) == 0, "{}", ui.log());
        home_answers(&mut ui);
    }

    /// A start that fails (no emulator to run, or one that exits at once) ends in a plain notice, one start, no retry, and
    /// Home is as it was.
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_just_play_with_no_emulator_says_what_happened_and_does_not_retry() {
        for (name, spawn_fails) in [("just-play-exits", false), ("just-play-no-emulator", true)] {
            let mut ui = Driver::start(name, true, |tmp, _| {
                if spawn_fails {
                    let _ = std::fs::remove_file(tmp.join("run-game"));
                } else {
                    let _ = std::fs::remove_file(tmp.join("run-game"));
                    executable(&tmp.join("run-game"), "#!/bin/sh\nprintf x >> \"$0.starts\"\nexit 1\n");
                }
            });
            ui.sees("Game Boy");
            ui.send("up");
            ui.sees("Just Play");
            just_play_press(&mut ui);
            let text = ui.sees("didn't start");
            assert!(text.contains("Pocket CD Demo") || text.contains("Tobu Tobu Girl"), "{name}: the notice names the game: {text}");
            std::thread::sleep(Duration::from_secs(1));
            assert_eq!(launches(&ui), 1, "{name}: one start, no retry loop\n{}", ui.log());
            home_answers(&mut ui);
        }
    }

    /// The review's case: the one game in progress belongs to a system whose emulator core is not on this device, and an
    /// unplayed game is there. Just Play passes over the first (no start, no changed emulator choice) and starts the other.
    #[test]
    fn just_play_skips_a_game_whose_emulator_core_is_missing() {
        let mut ui = Driver::start("just-play-route", true, |tmp, _| {
            add_system(tmp, "gba", "Game Boy Advance", ".gba", &["Broken Route (World).gba"]);
            let cfg = tmp.join("es_systems.cfg");
            let text = std::fs::read_to_string(&cfg).unwrap().replace(
                "<extension>.gba</extension>",
                "<extension>.gba</extension><emulators><emulator name=\"retroarch\"><cores><core default=\"true\">sol_missing</core></cores></emulator></emulators>",
            );
            std::fs::write(&cfg, text).unwrap();
            let game = tmp.join("gba/Broken Route (World).gba");
            let lib = serde_json::json!({"version": 2, "games": {game.to_string_lossy(): {"status": "playing", "status_set": true, "sessions": 2, "secs": 3600}}});
            std::fs::write(tmp.join("data/library.json"), lib.to_string()).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("up");
        ui.sees("Just Play");
        just_play_press(&mut ui);
        ui.wait(|d| d.card.tmp.join("run-game.started").exists());
        assert_eq!(launches(&ui), 1, "{}", ui.log());
        assert!(!ui.log().contains("Broken Route"), "the game with no core is not started: {}", ui.log());
        assert!(ui.log().contains("just play"), "{}", ui.log());
    }

    /// Just Play has its own switch. A `curator.json` from before it (no key) keeps what the player saw: Lens off, no Just
    /// Play row. With the key it is the switch that counts, whatever the Lens says.
    #[test]
    fn just_play_has_its_own_switch_and_an_older_lens_off_keeps_it_off() {
        let mut ui = Driver::start("just-play-off", true, |tmp, _| {
            std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false}"#).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("up");
        // Search is the row above the systems, nothing between.
        let home = ui.sees("Find any game by its name");
        assert!(!home.contains("Just Play"), "{home}");
        let mut ui = Driver::start("just-play-own-switch", true, |tmp, _| {
            std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false,"just_play":true}"#).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("up");
        let home = ui.sees("Just Play");
        assert!(home.contains("Less browsing. More playing."), "Just Play without the Lens: {home}");
    }

    /// X on Just Play shows the game A would start, with the reason, and starts nothing; Right brings another; A plays it.
    #[test]
    fn x_on_just_play_shows_the_pick_with_its_reason_and_right_brings_another() {
        let mut ui = Driver::new("just-play-card", true);
        ui.sees("Game Boy");
        ui.send("up");
        ui.sees("Just Play");
        // The index is made on a thread: X once it is there (a second X on the card would be the note's).
        ui.wait(|d| d.log().contains("games, catalog"));
        ui.send("x");
        // The pick's card: "RIGHT Another" is in its hints only when Just Play opened it (the reason line is over the facts).
        let _card = ui.wait_frame(|t| t.contains("Another"));
        assert_eq!(launches(&ui), 0, "X only shows the pick: {}", ui.log());
        let picks = |ui: &Driver| ui.log().lines().filter(|l| l.starts_with("curator: just play")).count();
        let before = picks(&ui);
        ui.send("right");
        ui.wait(|d| picks(d) > before);
        assert_eq!(launches(&ui), 0, "{}", ui.log());
        ui.send("a");
        ui.wait(|d| d.card.tmp.join("run-game.started").exists());
    }

    /// Kids Mode has its own start: no Just Play row there.
    #[test]
    fn just_play_is_not_in_kids_mode() {
        let mut d = kids_card_with_failing_game("just-play-kids", |_| {});
        let shelf = d.sees("Tobu Tobu Girl");
        assert!(!shelf.contains("Just Play"), "{shelf}");
    }

    /// Easy plays the game in front with A: no Just Play row there, and none after moving through its shelf.
    #[test]
    fn just_play_is_not_on_easy() {
        let mut ui = Driver::start("just-play-easy", true, |tmp, _| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"big"}"#).unwrap();
        });
        let first = ui.sees("Quick settings");
        assert!(!first.contains("Just Play"), "{first}");
        ui.send("right");
        let next = ui.sees(" of ");
        assert!(!next.contains("Just Play"), "{next}");
    }

    /// Settings → Games → Game check on a card with the usual slips: a copy of a game and no BIOS for Sega CD. It runs,
    /// names them, A hides the copy (the file stays). After Add games with a new game on the card, it is offered.
    #[test]
    fn the_game_check_names_what_is_wrong_hides_a_copy_and_is_offered_after_adding_games() {
        let mut ui = Driver::start("game-check", true, |tmp, _| {
            std::fs::copy(tmp.join("gb/Tobu Tobu Girl (World).gb"), tmp.join("gb/Tobu Tobu Girl (World) (1).gb")).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("start");
        ui.sees("Wi-Fi");
        ui.send("r1\nr1\nr1\nr1\nr1"); // Games
        ui.sees("Add games");
        ui.send("down\ndown\na");
        let groups = ui.sees("Same game twice");
        assert!(groups.contains("BIOS files") && groups.contains("games ready"), "{groups}");
        ui.send("down\na");
        ui.sees("Tobu Tobu Girl (World) (1).gb");
        ui.send("a");
        let detail = ui.sees("Hide this copy");
        assert!(detail.contains("Same game twice:"), "{detail}");
        ui.send("a");
        ui.wait(|d| std::fs::read_to_string(d.card.tmp.join("data/library.json")).is_ok_and(|j| j.contains("(1).gb")));
        assert!(ui.card.tmp.join("gb/Tobu Tobu Girl (World) (1).gb").exists(), "hidden, never deleted");
        let left = ui.sees("BIOS files");
        assert!(!left.contains("Same game twice"), "{left}");

        ui.send("b");
        ui.sees("Box art & downloads");
        std::fs::write(ui.card.tmp.join("gb/Pokemon Blau.gb"), b"game").unwrap();
        ui.send("up\nup\na");
        ui.sees("Game Boy");
        ui.send("b");
        let offer = ui.sees("1 new game");
        assert!(offer.contains("Check"), "{offer}");
        ui.send("b");
        ui.sees("Box art & downloads");
    }

    #[test]
    fn a_folder_no_system_knows_can_be_corrected_without_losing_or_restarting_the_game_list() {
        let mut ui = Driver::start("typo-folder", true, |tmp, _| {
            let typo = tmp.join("Retro Stuff");
            std::fs::create_dir(&typo).unwrap();
            std::fs::write(typo.join("Folder Demo.gb"), vec![0u8; 32768]).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("start");
        ui.sees("Wi-Fi");
        ui.send("r1\nr1\nr1\nr1\nr1"); // Games
        ui.sees("Add games");
        ui.send("down\ndown\na");
        ui.sees("Wrong folder or file type");
        // This card has missing Sega CD BIOS first, then the stray-folder group.
        ui.send("down\na");
        ui.sees("Retro Stuff");
        ui.send("a");
        let remedy = ui.sees("Nothing reads this folder");
        assert!(remedy.contains("Retro Stuff"), "the folder is named: {remedy}");
        // A player corrects it on their computer, then asks Game check to look again (X).
        std::fs::rename(ui.card.tmp.join("Retro Stuff/Folder Demo.gb"), ui.card.tmp.join("gb/Folder Demo.gb")).unwrap();
        let applied_before = ui.log().matches("library: the list read again is on the shelf").count();
        ui.send("b\nb\nx");
        ui.wait(|d| d.log().matches("game check:").count() >= 2);
        let checked = ui.sees("games ready");
        assert!(!checked.contains("Wrong folder or file type"), "the corrected folder is no longer a problem: {checked}");
        // Game check and the library rescan are separate workers. If A opens
        // the old one-game list first, the later swap correctly preserves Tobu
        // instead of selecting the newly inserted Folder Demo above it.
        ui.wait(|d| d.log().matches("library: the list read again is on the shelf").count() > applied_before);
        ui.send("b\nhome");
        ui.sees("Game Boy");
        ui.send("a");
        ui.sees("Folder Demo");
        ui.send("down");
        ui.sees("Tobu Tobu Girl"); // The previous game is still reachable beside the new one.
        assert_eq!(std::fs::metadata(ui.card.tmp.join("gb/Folder Demo.gb")).unwrap().len(), 32768);
    }

    /// A folder another card calls the system ("Game Boy" for gb) is read as it, without a rename and without a word to the player.
    #[test]
    fn a_folder_with_another_cards_name_is_read_as_the_system() {
        let mut ui = Driver::start("other-name-folder", true, |tmp, _| {
            let other = tmp.join("Game Boy (GB)");
            std::fs::create_dir(&other).unwrap();
            std::fs::write(other.join("Folder Demo.gb"), vec![0u8; 32768]).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("a");
        ui.sees("Folder Demo");
        ui.send("down");
        ui.sees("Tobu Tobu Girl"); // beside the game of the system's own folder
        assert_eq!(std::fs::metadata(ui.card.tmp.join("Game Boy (GB)/Folder Demo.gb")).unwrap().len(), 32768, "the card is as it was");
        assert!(ui.card.tmp.join("Game Boy (GB)").is_dir() && !ui.card.tmp.join("gb/Folder Demo.gb").exists());
    }

    /// A fresh ROCKNIX card holds an empty roms/pico-8/Splore.png and nothing else; it is not a game without PICO-8,
    /// so the page is "No games yet", not a shelf with a game that does not start.
    #[test]
    fn a_card_with_only_rocknixs_splore_says_no_games_yet() {
        let mut ui = Driver::start("splore-only", true, |tmp, _| {
            for f in ["gb/Tobu Tobu Girl (World).gb", "segacd/Pocket CD Demo (USA).cue", "segacd/Pocket CD Demo (USA).bin"] {
                std::fs::remove_file(tmp.join(f)).unwrap();
            }
            let pico = tmp.join("pico-8");
            std::fs::create_dir_all(&pico).unwrap();
            std::fs::write(pico.join("Splore.png"), b"").unwrap();
            let cfg = std::fs::read_to_string(tmp.join("es_systems.cfg")).unwrap().replace(
                "</systemList>",
                &format!("  <system><name>pico-8</name><fullname>PICO-8</fullname><path>{}</path><extension>.p8 .png</extension><command>true</command></system>\n</systemList>", pico.display()),
            );
            std::fs::write(tmp.join("es_systems.cfg"), cfg).unwrap();
        });
        let home = ui.sees("No games yet");
        assert!(!home.contains("Splore") && !home.contains("PICO-8"), "{home}");
    }

    /// New-player walk 2026-11-02: offline with a Wi-Fi chip, A on the empty Home opens the Wi-Fi page (one press, not twelve),
    /// and B comes back to the same Home with the pack waiting.
    #[test]
    fn an_empty_home_offline_sends_a_to_the_wifi_page_and_b_comes_back() {
        let mut ui = Driver::start("starter-wifi", true, |tmp, cmd| {
            for f in ["gb/Tobu Tobu Girl (World).gb", "segacd/Pocket CD Demo (USA).cue", "segacd/Pocket CD Demo (USA).bin"] {
                std::fs::remove_file(tmp.join(f)).unwrap();
            }
            let bin = tmp.join("offline-bin");
            std::fs::create_dir(&bin).unwrap();
            executable(&bin.join("ip"), "#!/bin/sh\nexit 0\n");
            cmd.env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
        });
        let home = ui.sees("No games yet");
        assert!(home.contains("Connect to Wi-Fi") && !home.contains("Get the starter pack"), "{home}");
        ui.send("a");
        let mut page = String::new();
        ui.wait(|d| {
            page = d.frame();
            !page.contains("No games yet")
        });
        assert!(page.contains("Wi-Fi") && page.contains("Wi-Fi country"), "A opens the Wi-Fi page: {page}");
        ui.send("b");
        let back = ui.sees("No games yet");
        assert!(back.contains("Connect to Wi-Fi"), "B comes back to Home with the pack waiting: {back}");
    }

    /// A card with no games is not an empty screen: it says so, offers the starter pack, and says what is missing when
    /// the Pocket is offline.
    #[test]
    fn an_empty_card_offers_the_starter_pack_and_says_when_it_is_offline() {
        let mut ui = Driver::start("starter", true, |tmp, cmd| {
            for f in ["gb/Tobu Tobu Girl (World).gb", "segacd/Pocket CD Demo (USA).cue", "segacd/Pocket CD Demo (USA).bin"] {
                std::fs::remove_file(tmp.join(f)).unwrap();
            }
            // POCKETUI_MOCK mocks hardware, not wifi::ip_now. Keep this child's offline condition independent of
            // the build host's interfaces, helpers and inherited POCKETUI_WIFI_IFACE; no process-global env changes.
            let bin = tmp.join("offline-bin");
            std::fs::create_dir(&bin).unwrap();
            executable(&bin.join("ip"), "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$0.calls\"\nexit 0\n");
            executable(&bin.join("nmcli"), "#!/bin/sh\nexit 1\n");
            executable(&bin.join("curl"), "#!/bin/sh\nprintf 'called\\n' >> \"$0.calls\"\nexit 7\n");
            cmd.env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
        });
        let home = ui.sees("No games yet");
        // Offline with a Wi-Fi chip, the one button leads to Wi-Fi first (new-player walk 2026-11-02); the pack waits on Home.
        assert!(home.contains("Connect to Wi-Fi") && !home.contains("Get the starter pack") && home.contains("Settings › Games › Add games (phone, file sharing)"), "{home}");
        // The pack's own page, from Settings → Games → Extras → Starter pack, still says when the Pocket is offline.

        // Hold the real starter-scan at its config read. The headless driver has completed startup and skips the
        // device startup jobs, so opening this FIFO belongs to that scan. This forces the slow-worker interleaving
        // which made waiting for the page's header lose the second A press on busy build machines.
        let config = ui.card.tmp.join("es_systems.cfg");
        let xml = std::fs::read(&config).unwrap();
        std::fs::remove_file(&config).unwrap();
        let fifo = std::ffi::CString::new(config.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        ui.send("start\nr1\nr1\nr1\nr1\nr1\ndown\na\ndown\ndown\ndown\na");
        let loading = ui.sees("Looking at your games folder…");
        assert!(loading.contains("Starter pack"), "the header alone is visible before Confirm is accepted: {loading}");
        let mut fd = -1;
        ui.wait(|_| {
            fd = unsafe { libc::open(fifo.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
            fd >= 0
        });
        let mut scan = unsafe { std::fs::File::from_raw_fd(fd) };
        // The reader now owns the FIFO inode. Restore the path for future reads before releasing its current read.
        std::fs::remove_file(&config).unwrap();
        std::fs::write(&config, &xml).unwrap();
        scan.write_all(&xml).unwrap();
        drop(scan);

        let offered = pocket_ui::starter::games().iter().filter(|g| matches!(g.system.as_str(), "gb" | "segacd")).count();
        assert!(offered > 0);
        let ready = ui.sees(&format!("Get all {offered} games"));
        assert!(!ready.contains("Looking at your games folder"), "Confirm must follow the scan's result: {ready}");
        ui.send("a");
        ui.sees("This Pocket is offline");
        assert!(ui.card.tmp.join("offline-bin/ip.calls").is_file(), "the real network check used the child fixture");
        assert!(!ui.card.tmp.join("offline-bin/curl.calls").exists(), "offline refuses before trying a download");
    }

    /// A Driver on a card with no games, in this look.
    fn empty_card(name: &str, look: &str) -> Driver {
        let look = look.to_string();
        Driver::start(name, true, move |tmp, _| {
            for f in ["gb/Tobu Tobu Girl (World).gb", "segacd/Pocket CD Demo (USA).cue", "segacd/Pocket CD Demo (USA).bin"] {
                std::fs::remove_file(tmp.join(f)).unwrap();
            }
            std::fs::write(tmp.join("data/settings.json"), format!(r#"{{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"{look}"}}"#)).unwrap();
        })
    }

    /// Empty-card Home (new-player walk 2026-11-02): a second action, Add games (X), beside the starter pack. It opens
    /// the Add games page and B comes back to the hint; the first button and the way back are still there.
    #[test]
    fn an_empty_home_offers_add_games_as_a_second_action() {
        let mut ui = empty_card("empty-add", "shelf");
        let home = ui.sees("No games yet");
        // Offline (this test card has no network) the first button leads to Wi-Fi; the second one works either way.
        assert!((home.contains("Get the starter pack") || home.contains("Connect to Wi-Fi")) && home.contains("Add games") && home.contains("Look around"), "{home}");
        ui.send("x");
        let page = ui.sees("Add your games");
        assert!(!page.contains("No games yet"), "{page}");
        ui.send("b");
        let back = ui.sees("No games yet");
        assert!(back.contains("Add games") && back.contains("Look around"), "B comes back to the hint: {back}");
    }

    /// B steps past the hint: in every look the empty Home says so, nothing the pad can press crashes or opens a dead
    /// end, A brings the hint back, START still opens Settings and B leaves it again.
    #[test]
    fn looking_around_an_empty_home_works_in_every_look() {
        for look in ["shelf", "split", "cabinet"] {
            let mut ui = empty_card(&format!("empty-around-{look}"), look);
            ui.sees("Look around");
            ui.send("b");
            let around = ui.sees("Press A for the starter pack");
            assert!(around.contains("No games yet") && !around.contains("Look around"), "{look}: {around}");
            // Every button of the pad, twice; Home stays Home and says the same thing.
            ui.send("up\ndown\nleft\nright\nl1\nr1\nl2\nr2\nx\ny\nup\ndown");
            ui.send("select\nselect");
            let after = ui.sees("Press A for the starter pack");
            assert!(after.contains("No games yet"), "{look}: {after}");
            // Settings opens from here and B comes back to the same empty Home.
            ui.send("start");
            ui.wait(|d| !d.frame().contains("Press A for the starter pack"));
            ui.send("b");
            let home = ui.sees("Press A for the starter pack");
            assert!(home.contains("No games yet"), "{look}: {home}");
            // A brings the hint back, B puts it away again.
            ui.send("a");
            ui.sees("Look around");
            ui.send("b");
            ui.sees("Press A for the starter pack");
        }
    }

    /// Games arrive while the hint is away (or while it shows): the shelf is back, with them, and the hint is gone.
    #[test]
    fn games_appearing_end_the_empty_home() {
        let mut ui = empty_card("empty-games", "split");
        ui.sees("Look around");
        ui.send("b");
        ui.sees("Press A for the starter pack");
        // Copied over file sharing while the player looked around; the Add games page (from Settings) reads the card again.
        std::fs::create_dir_all(ui.card.tmp.join("gb")).unwrap();
        std::fs::write(ui.card.tmp.join("gb/Tobu Tobu Girl (World).gb"), vec![0u8; 32768]).unwrap();
        ui.send("start");
        ui.wait(|d| !d.frame().contains("Press A for the starter pack"));
        ui.send("b");
        ui.sees("Press A for the starter pack");
        ui.send("a");
        ui.sees("Look around");
        ui.send("x");
        ui.sees("Add your games");
        ui.send("b");
        let shelf = ui.sees("Game Boy");
        assert!(!shelf.contains("No games yet") && !shelf.contains("Look around"), "{shelf}");
    }

    /// Delete from the card: its own row on the card (21.11.: no player found it, as it
    /// sat under the description behind a "Hide or delete" sheet), the sheet with name and size, two A presses; the file
    /// is gone, the screen says so, and nothing is left of it: its place in Shelf space and its gamelist entry go too,
    /// its save stays.
    #[test]
    fn deleting_a_game_asks_twice_then_removes_the_file() {
        let mut ui = Driver::start("delete", true, |tmp, _| {
            let rom = tmp.join("gb/Tobu Tobu Girl (World).gb");
            std::fs::write(tmp.join("gb/Tobu Tobu Girl (World).srm"), b"save").unwrap();
            std::fs::write(tmp.join("gb/gamelist.xml"), "<gameList>\n  <game>\n    <path>./Tobu Tobu Girl (World).gb</path>\n    <name>Tobu Tobu Girl</name>\n  </game>\n</gameList>\n").unwrap();
            // Shelf space on, both games on the shelf (none in the crate, so Home looks as always).
            let cd = tmp.join("segacd/Pocket CD Demo (USA).cue");
            std::fs::write(tmp.join("data/shelf_space.json"), serde_json::json!({"limit": 3, "shelf": [rom.to_string_lossy(), cd.to_string_lossy()]}).to_string()).unwrap();
        });
        // With Shelf space on, Home opens on the crate: along the row to Game Boy.
        ui.sees("Crate");
        ui.send(&"left\n".repeat(8));
        for _ in 0..8 {
            if ui.frame().contains("Game Boy") {
                break;
            }
            ui.send("right");
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        ui.sees("Game Boy");
        let rom = ui.card.tmp.join("gb/Tobu Tobu Girl (World).gb");
        // The game's card (X), turned over (right), its second page (SELECT).
        ui.send("a\nx\nright\nselect");
        ui.sees("Stickers");
        // The last two rows: Delete from the card, then Hide from the shelf. Down stops on the last; Up is Delete.
        ui.send(&"down\n".repeat(10));
        ui.sees("Delete from the card");
        ui.sees("Hide from the shelf");
        ui.send("up\na");
        ui.sees("Delete Tobu Tobu Girl?");
        // The heading appears before the asynchronous file scan enables Delete: wait for its visible result.
        let sheet = ui.sees("Frees 1 KB: 1 file.");
        assert!(sheet.contains("Keep, 1 KB"), "saves kept unless asked: {sheet}");
        assert!(rom.exists(), "the sheet only asks: {sheet}");
        ui.send("a");
        let again = ui.sees("Press A again to delete");
        assert!(rom.exists(), "one A is not enough: {again}");
        ui.send("a");
        ui.sees("Deleted Tobu Tobu Girl");
        assert!(!rom.exists());
        assert!(ui.card.tmp.join("gb/Tobu Tobu Girl (World).srm").exists(), "the save stays");
        let gamelist = std::fs::read_to_string(ui.card.tmp.join("gb/gamelist.xml")).unwrap();
        assert_eq!(gamelist, "<gameList>\n</gameList>\n", "its gamelist entry is gone");
        let space = std::fs::read_to_string(ui.card.tmp.join("data/shelf_space.json")).unwrap_or_default();
        assert!(!space.contains("Tobu"), "its place on the shelf is free: {space}");
    }

    /// Two real ROM files and one cached checksum/DAT hit. B is deliberately absent from the DAT: resuming its
    /// hash must not create a new name-map change that accidentally repairs A's lost shelf refresh.
    fn named_cartridge_fixture(tmp: &Path, cmd: &mut Command) {
        let games = tmp.join("identified-games");
        std::fs::create_dir_all(&games).unwrap();
        // The card's own `gb` folder would be read as a second folder of the system (docs/OTHER-LAYOUTS.md); these journeys
        // are about exactly these two games.
        std::fs::remove_dir_all(tmp.join("gb")).unwrap();
        let known = games.join("Supar Mario Land 2.gb");
        std::fs::write(&known, b"recognized A").unwrap();
        std::fs::write(games.join("Unknown Homebrew.gb"), b"unknown B").unwrap();
        std::fs::write(tmp.join("es_systems.cfg"), format!(
            "<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension><command>true</command></system></systemList>", games.display()
        )).unwrap();
        std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"split","motion":"off","art":"off"}"#).unwrap();
        std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false,"stickers":false,"history":false}"#).unwrap();
        let crc = pocket_ui::rom_id::checksums("gb", &known).unwrap().unwrap().crc;
        let dat = format!("game (\n name \"Super Mario Land 2 (USA)\"\n rom ( name \"a.gb\" size 12 crc {crc:08X} )\n)\n");
        let seeded = pocket_ui::rom_id::run_until(&tmp.join("data"), &[("gb".into(), known)], &|_| Some(dat.clone()), false, || false);
        assert_eq!(seeded.names.len(), 1);
        // Only this child's helpers are mocked. No external service or process-wide test environment is used.
        let bin = tmp.join("identify-bin");
        std::fs::create_dir(&bin).unwrap();
        executable(&bin.join("ip"), "#!/bin/sh\nexit 0\n");
        executable(&bin.join("nmcli"), "#!/bin/sh\nexit 1\n");
        executable(&bin.join("curl"), "#!/bin/sh\nexit 7\n");
        cmd.env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    }

    #[test]
    fn a_stopped_identification_updates_shelf_names_after_the_game() {
        let mut ui = Driver::start("identify-stop-resume", true, named_cartridge_fixture);
        ui.sees("Game Boy");
        ui.wait(|d| d.log().contains("curator: 2 games"));
        ui.send("a");
        ui.sees("Supar Mario Land 2");
        ui.send("a");
        ui.wait(|d| d.card.tmp.join("run-game.started").exists());
        ui.wait(|d| status_of(d).contains("\tgame=playing"));
        // The first game exit starts the real identify worker. Hold its cache read until another game starts.
        let cache = ui.card.tmp.join("data/identify/checksums.tsv");
        let bytes = std::fs::read(&cache).unwrap();
        std::fs::remove_file(&cache).unwrap();
        let fifo = std::ffi::CString::new(cache.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        ui.send("home");
        ui.sees("Quit");
        ui.send("up\na");
        let mut fd = -1;
        ui.wait(|_| {
            fd = unsafe { libc::open(fifo.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
            fd >= 0
        });
        let mut held = unsafe { std::fs::File::from_raw_fd(fd) };
        ui.wait(|d| d.card.tmp.join("run-game.quit").exists());
        ui.sees("Supar Mario Land 2");
        std::fs::remove_file(ui.card.tmp.join("run-game.started")).unwrap();
        ui.send("a");
        ui.wait(|d| d.card.tmp.join("run-game.started").exists());
        ui.wait(|d| status_of(d).contains("\tgame=playing"));
        // Restore a normal pathname for later readers, then release only the old worker's open FIFO inode.
        std::fs::remove_file(&cache).unwrap();
        std::fs::write(&cache, &bytes).unwrap();
        held.write_all(&bytes).unwrap();
        drop(held);
        ui.wait(|d| d.log().contains("stopped for a game"));
        assert!(!ui.log().contains("identify: changed names kept for the shelf"), "the cancelled pass leaves the shelf alone");
        ui.send("home");
        ui.sees("Quit");
        ui.send("up\na");
        // Do not flood the headless event loop with frame requests while its timed Quit escalation runs.
        ui.wait(|d| d.log().lines().filter(|l| l.starts_with("game exited after ")).count() == 2);
        let seen = ui.sees("Super Mario Land 2");
        assert!(!seen.contains("Supar Mario Land 2") && seen.contains("Unknown Homebrew"), "actual library names after resumed identify and rescan: {seen}");
        assert!(ui.log().contains("library: read again (games recognised)"), "{}", ui.log());
    }

    #[test]
    fn manual_cartridge_lookup_changes_and_removes_the_actual_shelf_title() {
        manual_cartridge_lookup("0");
    }

    #[test]
    fn manual_cartridge_lookup_with_slow_picture_helpers() {
        manual_cartridge_lookup("0.5");
    }

    fn manual_cartridge_lookup(curl_delay: &str) {
        let mut ui = Driver::start("identify-lookup", true, |tmp, cmd| {
            named_cartridge_fixture(tmp, cmd);
            // The saved setting is art_off, not art. Unwanted automatic covers
            // could own the scrape claim when A asked for the manual lookup.
            let settings = tmp.join("data/settings.json");
            let mut saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
            saved.as_object_mut().unwrap().remove("art");
            saved["art_off"] = true.into();
            std::fs::write(settings, serde_json::to_vec(&saved).unwrap()).unwrap();
            cmd.env("POCKETUI_NAV_PROBE", tmp.join("nav.txt"));
            // Emulate slow process scheduling on a loaded gate without networking.
            // Automatic picture downloads must stay off in this checksum-name journey.
            executable(&tmp.join("identify-bin/curl"), &format!("#!/bin/sh\nsleep {curl_delay}\nexit 7\n"));
            executable(&tmp.join("identify-bin/ip"), "#!/bin/sh\nprintf '2: wlan0 inet 203.0.113.42/24 scope global wlan0\\n'\n");
        });
        ui.sees("Game Boy");
        // Extras paints the actual cached IP state on every frame. Wait for online readiness before using a
        // network-dependent card action; curl is a child-local failing stub, so no request leaves the fixture.
        ui.send("start\nr1\nr1\nr1\nr1\nr1\ndown\na");
        let extras = ui.sees("Download");
        assert!(extras.contains("Box art & downloads") && extras.contains("Box art"), "online Box art & downloads is ready: {extras}");
        ui.send("b\nb");
        ui.sees("Game Boy");
        ui.send("a");
        ui.sees("Mario Land 2");
        ui.send("a");
        ui.wait(|d| d.card.tmp.join("run-game.started").exists());
        ui.send("home");
        ui.sees("Quit");
        ui.send("up\na");
        ui.wait(|d| d.log().contains("game exited after "));
        ui.sees("Super Mario Land 2");
        let rom = ui.card.tmp.join("identified-games/Supar Mario Land 2.gb");
        std::fs::write(&rom, b"recognized replacement").unwrap();
        let crc = pocket_ui::rom_id::checksums("gb", &rom).unwrap().unwrap().crc;
        let table = ui.card.tmp.join("data/identify/Nintendo - Game Boy.tsv");
        std::fs::write(&table, format!("{crc:08X}\tTetris (World)\n")).unwrap();
        let lookup = |ui: &mut Driver, prior_results: usize, expected: Option<&str>| {
            let applied_before = ui.log().matches("library: the list read again is on the shelf").count();
            ui.send("x");
            // The card's hint bar shows four hints: Y Status stays, SELECT Details is left out (`kit::top_hints`).
            ui.sees("Status");
            ui.send("y");
            ui.sees("Wrong pictures? Look up again");
            ui.send(&"down\n".repeat(8));
            ui.send("a");
            ui.wait(|d| d.log().lines().filter(|l| l.starts_with("look up again:") && l.contains(" -> ")).count() > prior_results);
            let path = ui.card.tmp.join("identified-games/Supar Mario Land 2.gb");
            let names = pocket_ui::rom_id::known(&ui.card.tmp.join("data"), &[("gb".into(), path.clone())]);
            assert_eq!(names.get(&path).map(String::as_str), expected, "the real lookup cache must contain the expected new mapping");
            // The worker logs its result before the UI handles LookedUp. That
            // event recreates the card: B before it races a later fresh card.
            ui.wait(|d| {
                d.frame();
                !std::fs::read_to_string(d.card.tmp.join("nav.txt")).unwrap().contains("@busy\tlookup\n")
            });
            ui.send("b");
            // A card holds a library swap off. Wait for that pending list to
            // reach the shelf before opening the next status picker.
            ui.wait(|d| d.log().matches("library: the list read again is on the shelf").count() > applied_before);
        };
        lookup(&mut ui, 0, Some("Tetris (World)"));
        let renamed = ui.sees("Tetris");
        assert!(!renamed.contains("Super Mario Land 2") && !renamed.contains("Supar Mario Land 2"), "manual lookup changes the library title: {renamed}");
        // The same path now has no DAT match. A removed mapping must undo its old checksum title too.
        std::fs::write(&rom, b"unknown replacement").unwrap();
        lookup(&mut ui, 1, None);
        let removed = ui.sees("Supar Mario Land 2");
        assert!(!removed.contains("Tetris") && removed.contains("Unknown Homebrew"), "removed lookup name restores the file-name fallback: {removed}");
    }

    /// The Minimal look: X on a game opens its info page (everything on one screen, `game_info`), B goes back to the
    /// shelf; SELECT shows the card's full page and back.
    #[test]
    fn a_card_opened_before_the_catalog_can_open_connections_when_its_built_event_arrives() {
        let mut ui = Driver::start("connections-before-catalog", true, |tmp, _| {
            let games = tmp.join("connection-games");
            std::fs::create_dir_all(&games).unwrap();
            // The card's own `gb` folder would be read as a second folder of the system (docs/OTHER-LAYOUTS.md); this journey
            // is about exactly these two games.
            std::fs::remove_dir_all(tmp.join("gb")).unwrap();
            for n in [2, 3] {
                std::fs::write(games.join(format!("Pocket Review Racer {n}.gb")), "x").unwrap();
            }
            std::fs::write(tmp.join("es_systems.cfg"), format!(
                "<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension><command>true</command></system></systemList>", games.display()
            )).unwrap();
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"split","motion":"off","art":"off"}"#).unwrap();
            std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false,"stickers":false,"history":false}"#).unwrap();
            // The real catalog worker reads its checksum cache. A FIFO holds that read until the card is open;
            // releasing EOF lets the normal worker send its real Built event, with no synchronous snapshot build.
            let identify = tmp.join("data/identify");
            std::fs::create_dir_all(&identify).unwrap();
            assert!(Command::new("mkfifo").arg(identify.join("checksums.tsv")).status().unwrap().success());
        });
        let path = std::ffi::CString::new(ui.card.tmp.join("data/identify/checksums.tsv").to_str().unwrap()).unwrap();
        let mut fd = -1;
        ui.wait(|_| {
            fd = unsafe { libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
            fd >= 0
        });
        let catalog_read = unsafe { std::fs::File::from_raw_fd(fd) };
        ui.sees("Game Boy");
        ui.send("a\nx");
        let initial = ui.sees("No description yet");
        assert!(initial.contains("Pocket Review Racer 2"), "{initial}");
        ui.send("down");
        let initial = ui.frame();
        assert!(!initial.contains("Series") && !initial.contains("Open"), "no links before the worker finishes: {initial}");
        drop(catalog_read);
        ui.wait(|d| d.log().contains("curator: 2 games"));
        let updated = ui.sees("Series");
        assert!(updated.contains("Pocket Review Racer 2"), "still the same open card: {updated}");
        ui.send("down\na");
        let list = ui.sees("Pocket Review Racer series");
        assert!(list.contains("Pocket Review Racer 3"), "the updated link is usable without closing the card: {list}");
    }

    #[test]
    fn x_in_minimal_opens_the_info_page_and_b_returns() {
        let mut ui = Driver::start("info-page", true, |tmp, _| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"split"}"#).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("a");
        ui.sees("Tobu Tobu Girl");
        ui.send("x");
        let page = ui.sees("None. Press Y");
        assert!(page.contains("Tobu Tobu Girl") && page.contains("Status") && page.contains("No description yet."), "{page}");
        // And why: a homebrew game has no article, which is not the same as having no Wi-Fi yet.
        assert!(page.contains("No Wikipedia article for this game"), "{page}");
        assert!(!page.contains("Turn over"), "the info page, not the box: {page}");
        ui.send("select");
        ui.sees("Stickers");
        ui.send(&"down\n".repeat(10));
        ui.sees("Hide from the shelf");
        ui.send("select");
        ui.sees("None. Press Y");
        ui.send("b");
        let mut shelf = String::new();
        ui.wait(|d| {
            shelf = d.frame();
            shelf.contains("Tobu Tobu Girl") && !shelf.contains("None. Press Y")
        });
    }

    /// The Shelf look, a cartridge game (0.1.4: the shelf shows its box): X opens the box with the cartridge out of it,
    /// LEFT turns it to the back and back again, B closes it, A starts the game from the shelf.
    #[test]
    fn x_opens_a_cartridge_games_box_left_turns_it_b_closes_a_starts() {
        let mut ui = Driver::start("box-journey", true, |tmp, _| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","theme":"shelf"}"#).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("a");
        ui.sees("Tobu Tobu Girl");
        ui.send("x");
        let front = ui.sees("Turn over");
        assert!(front.contains("Tobu Tobu Girl"), "the box's front with the title: {front}");
        ui.send("left");
        let back = ui.sees("No description yet.");
        assert!(back.contains("Game Boy"), "the back names its system at the foot: {back}");
        ui.send("left");
        ui.wait(|d| !d.frame().contains("No description yet."));
        ui.send("b");
        ui.wait(|d| {
            let f = d.frame();
            f.contains("Tobu Tobu Girl") && !f.contains("Turn over")
        });
        ui.send("a");
        ui.wait(|d| matches!(std::fs::read_to_string(d.card.tmp.join("run-game.started")).as_deref(), Ok("started\n")));
    }

    /// Bluetooth page with a speaker/headphone-class device (the mock's JBL Flip 5): search, pair, trust, connect,
    /// and the page says it is connected.
    #[test]
    fn bluetooth_page_connects_an_audio_device() {
        let mut ui = Driver::new("bluetooth", true);
        ui.sees("Game Boy");
        ui.send("start\ndown\na");
        ui.sees("Search for devices");
        ui.send("down\na");
        ui.sees("JBL Flip 5");
        ui.send(&"down\n".repeat(5));
        ui.send("a");
        let text = ui.sees("JBL Flip 5 is connected");
        assert!(!text.contains("Couldn't") && !text.contains("Failed"), "{text}");
    }

    #[test]
    fn power_menu_powers_off_once_and_disarms_the_alarm_once() {
        let mut ui = Driver::new("power", true);
        // Start-up clears a stale alarm on its own thread (`Sleep::spawn_rtc_check`; the driver seeded "+3600"). Arm ours
        // only after that check has logged, or a late check clears our alarm as stale and the power off reads `before=none`.
        let log_path = ui.card.tmp.join("data/logs/shutdown.log");
        ui.wait(|_| std::fs::read_to_string(&log_path).is_ok_and(|text| text.contains("start: RTC alarm disarm verified")));
        let start_log = std::fs::read_to_string(&log_path).unwrap();
        assert!(start_log.contains("before=+3600"), "the start-up check clears the stale alarm the card was left with: {start_log}");
        // Verified disarm reads the alarm back: model sysfs with a regular file, not a write-only FIFO.
        let alarm = ui.card.tmp.join("wakealarm");
        std::fs::write(&alarm, "1793051031").unwrap();
        ui.send("powermenu");
        ui.sees("Power off");
        ui.send("down\ndown\na\na");
        ui.wait(|d| d.card.tmp.join("power.log").exists());
        ui.sees("Switching off");
        assert_eq!(std::fs::read_to_string(ui.card.tmp.join("power.log")).unwrap(), "poweroff\n");
        assert_eq!(std::fs::read_to_string(alarm).unwrap(), "0");
        let log = ui.card.tmp.join("data/logs/shutdown.log");
        ui.wait(|_| std::fs::read_to_string(&log).is_ok_and(|text| text.contains("ended: exit 0")));
        let log = std::fs::read_to_string(log).unwrap();
        assert_eq!(log.matches("PowerOff: RTC alarm disarm verified").count(), 1, "{log}");
        assert!(log.contains("before=1793051031 write=ok after=none"), "{log}");
        assert!(log.contains("tries=1"), "one verified clear, without retries: {log}");
        let disarm = log.find("PowerOff: RTC alarm disarm verified").unwrap();
        let command = log.find("PowerOff: running").unwrap();
        assert!(disarm < command, "verified before issuing poweroff: {log}");
    }


    /// A small PNG (a picture of 200 x 280 pixels, no game's art).
    fn picture(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut enc = png::Encoder::new(std::fs::File::create(path).unwrap(), 200, 280);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let px: Vec<u8> = (0..200 * 280).flat_map(|i| [(i % 200) as u8, (i / 200 % 256) as u8, 90]).collect();
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }


    /// The status file's fields (`<dir>/ui.status` under the test driver).
    fn status_of(d: &Driver) -> String {
        std::fs::read_to_string(d.card.tmp.join("ui.status")).unwrap_or_default()
    }

    /// A card with two recent games over its two systems (four shelf rows), a controller of its own for presses held
    /// across pages (`tbtn 21 …`), and the navigation record.
    fn release_card(name: &str) -> Driver {
        let mut d = Driver::start(name, true, |tmp, cmd| {
            let recents = serde_json::json!([
                {"system": "gb", "name": "Tobu Tobu Girl", "path": tmp.join("gb/Tobu Tobu Girl (World).gb"), "last_played": 1794100000u64, "play_secs": 60},
                {"system": "segacd", "name": "Pocket CD Demo", "path": tmp.join("segacd/Pocket CD Demo (USA).cue"), "last_played": 1794000000u64, "play_secs": 60},
            ]);
            std::fs::write(tmp.join("data/recents.json"), recents.to_string()).unwrap();
            // The controls card of a console's first start would take the first key of a game start.
            std::fs::write(tmp.join("data/controls-card-seen"), "gb\nsegacd\n").unwrap();
            // A list of rows with the focus mark (`kit::shelf_row`), which the navigation record reads.
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"split"}"#).unwrap();
            cmd.env("POCKETUI_NAV_PROBE", tmp.join("nav.txt"));
        });
        d.sees("Game Boy");
        d.send("tconnect 21 builtin Pocket");
        d.wait(|d| status_of(d).contains("\tplace=shelf:Home\tcover=none\t"));
        d
    }

    /// One rule for releases (2026-11-09, audit #4): a release, and the repeat of a held direction, count on the page
    /// that saw the press. A direction held on the shelf repeated on behind a page that opened without a key (the power
    /// menu from a long press elsewhere, the dialog SELECT + START opens): the shelf moved under it. Missed: SOL-08 ended the
    /// repeat on the next key, and no key came.
    #[test]
    fn a_held_direction_never_moves_the_shelf_behind_a_page_that_opened_over_it() {
        let mut d = release_card("release-repeat");
        let first = focused_in(&nav_record(&mut d));
        d.send("tbtn 21 down down");
        let mut after_step = None;
        d.wait(|d| {
            after_step = focused_in(&nav_record(d));
            after_step.is_some() && after_step != first
        });
        d.send("powermenu");
        d.wait(|d| status_of(d).contains("cover=power"));
        // Well past the repeat's delay and several of its steps.
        std::thread::sleep(Duration::from_millis(1500));
        d.send("tbtn 21 down up\nb");
        d.wait(|d| status_of(d).contains("\tplace=shelf:Home\tcover=none\t"));
        assert_eq!(focused_in(&nav_record(&mut d)), after_step, "the shelf stayed where the press left it");
    }

    /// START and SELECT open Settings and Recent games on their release, alone on the shelf. Pressed on the shelf and let go
    /// after the power menu came over it, their release opened Settings (or Recent games) behind the menu.
    #[test]
    fn a_tap_let_go_on_another_page_opens_nothing() {
        for (key, opens) in [("start", "settings:"), ("select", "recent games")] {
            let mut d = release_card(&format!("release-tap-{key}"));
            d.send(&format!("tbtn 21 {key} down"));
            std::thread::sleep(Duration::from_millis(150));
            d.send("powermenu");
            d.wait(|d| status_of(d).contains("cover=power"));
            d.send(&format!("tbtn 21 {key} up"));
            std::thread::sleep(Duration::from_millis(400));
            d.send("b");
            d.wait(|d| !status_of(d).contains("cover=power"));
            std::thread::sleep(Duration::from_millis(300));
            let status = status_of(&d);
            assert!(status.contains("\tplace=shelf:Home\tcover=none\t") && !status.contains(opens), "{key}'s release behind the power menu: {status}");
        }
    }

    /// A press that changes the page by itself (another key opening one) still ends the repeat (SOL-08), and B comes back
    /// to the same row.
    #[test]
    fn a_held_direction_ends_when_another_key_opens_a_page() {
        let mut d = release_card("release-repeat-key");
        d.send("tbtn 21 down down");
        let mut after_step = None;
        d.wait(|d| {
            after_step = focused_in(&nav_record(d));
            after_step.as_deref().is_some_and(|f| !f.starts_with("Tobu"))
        });
        d.send("start");
        d.wait(|d| status_of(d).contains("\tplace=settings:List Connections\t"));
        std::thread::sleep(Duration::from_millis(1500));
        d.send("tbtn 21 down up\nb");
        d.wait(|d| status_of(d).contains("\tplace=shelf:Home\tcover=none\t"));
        assert_eq!(focused_in(&nav_record(&mut d)), after_step);
    }

    /// HOME on a Settings page goes to the shelf on its press; its release there is no HOME tap that opens the quick
    /// settings (the case `Sys::cancel_home_tap` was made for).
    #[test]
    fn home_from_settings_lands_on_the_shelf_without_the_quick_settings() {
        let mut d = release_card("release-home-settings");
        d.send("start");
        d.wait(|d| status_of(d).contains("\tplace=settings:List Connections\t"));
        d.send("tbtn 21 home down");
        d.wait(|d| status_of(d).contains("\tplace=shelf:Home\t"));
        d.send("tbtn 21 home up");
        std::thread::sleep(Duration::from_millis(400));
        assert!(status_of(&d).contains("\tplace=shelf:Home\tcover=none\t"), "{}", status_of(&d));
    }

    /// HOME held while the game ends by itself (a crash, the emulator's own quit): its release lands on the shelf and is no
    /// HOME tap there (the quick settings stay closed).
    #[test]
    fn home_held_while_the_game_ends_opens_nothing_on_the_shelf() {
        let mut d = release_card("release-home-game");
        // The recent game on the first row: A starts it (the stand-in).
        d.send("a");
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        d.wait(|d| status_of(d).contains("\tgame=playing"));
        d.send("tbtn 21 home down");
        // Synchronize over the same FIFO so pocket-ui has processed HOME-down while game=playing before SIGTERM.
        let _ = d.frame();
        // The game ends by itself while HOME is down.
        let pid = d.log().lines().find_map(|l| l.strip_prefix("test game pid ").and_then(|p| p.trim().parse::<i32>().ok())).expect("the stand-in's pid");
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        d.wait(|d| status_of(d).contains("\tplace=shelf:Home\t") && status_of(d).contains("\tgame=none"));
        d.send("tbtn 21 home up");
        std::thread::sleep(Duration::from_millis(400));
        assert!(status_of(&d).contains("\tcover=none\t"), "the release opened something: {}", status_of(&d));
    }

    /// The text drawn inside the focus mark of a navigation record (`‹nav:focus›`), if any.
    fn focused_in(record: &str) -> Option<String> {
        let rect = |l: &str| -> Option<([i32; 4], String)> {
            let v: Vec<&str> = l.splitn(5, '\t').collect();
            let n = |i: usize| v.get(i)?.parse::<i32>().ok();
            Some(([n(0)?, n(1)?, n(2)?, n(3)?], v.get(4)?.to_string()))
        };
        let all: Vec<([i32; 4], String)> = record.lines().filter_map(rect).collect();
        let (f, _) = all.iter().find(|(_, s)| s == "‹nav:focus›")?;
        all.iter()
            .filter(|(r, s)| !s.starts_with('‹') && r[0] >= f[0] - 2 && r[2] <= f[2] + 2 && r[1] >= f[1] - 2 && r[3] <= f[3] + 2)
            .map(|(_, s)| s.clone())
            .next()
    }

    /// The navigation record now (`nav`), without a key.
    fn nav_record(d: &mut Driver) -> String {
        let nav = d.nav.clone().expect("POCKETUI_NAV_PROBE");
        let mut record = String::new();
        let _ = std::fs::remove_file(&nav);
        d.send("nav");
        d.wait(|_| {
            record = std::fs::read_to_string(&nav).unwrap_or_default();
            record.ends_with("@end\n")
        });
        record
    }

    /// Report a problem (from the dialog SELECT + START opens) writes its report on a thread, and About shows "Writing…"
    /// until it is done; the "OK" row comes by itself then. The navigation record says the page is busy meanwhile, as it
    /// does for a Wi-Fi scan: the explorer took that row's coming for a move of Down's (2026-11-09, gate, 640x480:
    /// "settings:About: 1 move, 0 ticks, 0 sounds"). Held here by the one file only the report reads, a FIFO.
    #[test]
    fn about_says_it_is_busy_while_its_report_is_written() {
        let mut d = Driver::start("report-busy", true, |tmp, cmd| {
            std::fs::create_dir_all(tmp.join("data/logs")).unwrap();
            let fifo = std::ffi::CString::new(tmp.join("data/logs/portal-last.html").to_str().unwrap()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
            cmd.env("POCKETUI_NAV_PROBE", tmp.join("nav.txt")).env("POCKETUI_REPORTS", tmp.join("reports"));
        });
        d.sees("Game Boy");
        d.send("leave");
        d.sees("Report a problem");
        for _ in 0..8 {
            if focused_in(&nav_record(&mut d)).is_some_and(|f| f.starts_with("Report a problem")) {
                break;
            }
            d.send("down");
        }
        assert!(focused_in(&nav_record(&mut d)).is_some_and(|f| f.starts_with("Report a problem")), "the dialog's Report a problem row");
        d.send("a");
        let mut record = String::new();
        d.wait(|d| {
            record = nav_record(d);
            record.contains("@where\tsettings:About")
        });
        assert!(record.contains("@busy\treport"), "while the report is written the page says it is busy:\n{record}");
        // Let the report read its last file: it finishes, and the page is no longer busy.
        let fifo = d.card.tmp.join("data/logs/portal-last.html");
        let writer = std::thread::spawn(move || {
            let _ = std::fs::write(fifo, "<html>sign-in</html>\n");
        });
        d.wait(|d| !nav_record(d).contains("@busy\treport"));
        writer.join().unwrap();
    }

    /// A navigation record taken apart: where the UI is, its list's selected row and length, each row's area (`@hit`), and
    /// every string and mark with its place.
    struct Record {
        at: String,
        list: Option<(usize, usize)>,
        hits: Vec<[i32; 4]>,
        placed: Vec<([i32; 4], String)>,
    }

    impl Record {
        fn of(record: &str) -> Record {
            let mut r = Record { at: String::new(), list: None, hits: Vec::new(), placed: Vec::new() };
            for l in record.lines() {
                let v: Vec<&str> = l.split('\t').collect();
                let n = |i: usize| v.get(i).and_then(|s| s.parse::<i32>().ok());
                match v[0] {
                    "@where" => r.at = v.get(1).unwrap_or(&"").to_string(),
                    "@list" => r.list = Some((n(1).unwrap_or(0) as usize, n(2).unwrap_or(0) as usize)),
                    "@hit" => r.hits.push([n(1).unwrap(), n(2).unwrap(), n(3).unwrap(), n(4).unwrap()]),
                    _ => {
                        if let (Some(x0), Some(y0), Some(x1), Some(y1)) = (n(0), n(1), n(2), n(3)) {
                            r.placed.push(([x0, y0, x1, y1], l.splitn(5, '\t').nth(4).unwrap_or_default().to_string()));
                        }
                    }
                }
            }
            r
        }
        /// The strings and marks inside `area` (the probe's own marks left out), left to right.
        fn inside(&self, a: [i32; 4]) -> Vec<String> {
            let mut v: Vec<&([i32; 4], String)> = self.placed.iter()
                .filter(|(b, s)| !s.starts_with("‹nav:") && b[0] >= a[0] - 2 && b[2] <= a[2] + 2 && (b[1] + b[3]) / 2 >= a[1] && (b[1] + b[3]) / 2 <= a[3])
                .collect();
            v.sort_by_key(|(b, _)| b[0]);
            v.into_iter().map(|(_, s)| s.clone()).collect()
        }
        /// Each row's strings and marks (its name first).
        fn rows(&self) -> Vec<Vec<String>> {
            self.hits.iter().map(|h| self.inside(*h)).collect()
        }
        /// The selected row's: the row the focus mark is on.
        fn selected(&self) -> Vec<String> {
            let f = self.placed.iter().find(|(_, s)| s == "‹nav:focus›").map(|(b, _)| *b).expect("a focus mark");
            let mid = (f[1] + f[3]) / 2;
            // A panel over the page (the quick settings) keeps its own touch map: its rows are the focus mark's own.
            self.hits.iter().find(|h| h[1] <= mid && mid <= h[3]).map_or_else(|| self.inside(f), |h| self.inside(*h))
        }
        /// Every string and mark of the frame but the clock's figures.
        fn text(&self) -> Vec<String> {
            self.placed.iter().map(|(_, s)| s.clone())
                .filter(|s| !s.starts_with("‹nav:") && !(s.contains(':') && s.chars().all(|c| c.is_ascii_digit() || c == ':')))
                .collect()
        }
        fn says(&self, word: &str) -> bool {
            self.placed.iter().any(|(_, s)| s == word)
        }
    }

    /// Records until one satisfies `ok`, for at most 10 seconds (a level or a language takes a moment to show).
    fn record_until(d: &mut Driver, ok: impl Fn(&Record) -> bool) -> Option<Record> {
        let until = Instant::now() + test_clock::timeout(Duration::from_secs(10));
        loop {
            let r = Record::of(&nav_record(d));
            if ok(&r) {
                return Some(r);
            }
            if Instant::now() > until {
                return None;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// A hand walk for 0.5.1: Zoom in the pause menu changed only with A (or X), and which rows open a page and which
    /// change in place wasn't clear. The whole class, as a player walks it: every row of every Settings category either
    /// opens a page (the hint "A Open", its value ends in the drawn angle ">" on every row) or changes in place (the
    /// hint "< > Change", no angle; the selected one between ‹ ›), and every row that changes in place takes Left and
    /// Right: one changes it, the other takes it back. A still changes it (or does nothing, on a level) and never
    /// leaves the list. Then the same for the quick settings' rows with ‹ ›. Missed before: tests asserted the rows' names
    /// and places, never what Left/Right do on each of them.
    #[test]
    fn every_row_that_changes_in_place_takes_left_and_right_and_every_page_row_shows_its_angle() {
        let mut d = Driver::start("settings-rows", true, |tmp, cmd| {
            // POCKETUI_LANG would pin the language, and the Language row could not change it: English from the locale.
            cmd.env("POCKETUI_NAV_PROBE", tmp.join("nav.txt")).env_remove("POCKETUI_LANG").env("LANG", "en_US.UTF-8").env("LC_ALL", "en_US.UTF-8");
        });
        d.sees("Game Boy");
        d.send("start");
        d.sees("Bluetooth");
        let (mut values, mut pages, mut seen) = (Vec::new(), Vec::new(), std::collections::HashSet::new());
        for _ in 0..8 {
            let first = Record::of(&nav_record(&mut d));
            let (_, len) = first.list.expect("a category's list");
            for _ in 0..len {
                let r = Record::of(&nav_record(&mut d));
                assert!(r.at.starts_with("settings:List"), "{}", r.at);
                let row = r.selected();
                let name = row.first().cloned().unwrap_or_default();
                if !seen.insert(name.clone()) {
                    break;
                }
                // Page rows end in their angle on every row, the selected one and the others alike.
                for other in r.rows() {
                    let page = other.last().is_some_and(|m| m == ">");
                    let arrows = other.iter().any(|m| m == "‹" || m == "›");
                    assert!(!(page && arrows), "{name}: a row has both a page's angle and a switch's arrows: {other:?}");
                }
                if r.says("Change") {
                    assert!(!row.iter().any(|m| m == ">"), "{name} changes in place, so it has no page's angle: {row:?}");
                    assert!(row.iter().any(|m| m == "‹") && row.last().is_some_and(|m| m == "›"), "{name}: the selected value between ‹ ›: {row:?}");
                    // Shelf space switched on asks which games go on the shelf first (a dialog over the list): its own
                    // journey, `shelf_space_on_a_swap_and_off_again_puts_every_game_back`.
                    if name == "Shelf space" {
                        values.push(name);
                        d.send("down");
                        continue;
                    }
                    // Brightness and Volume load their levels on a background thread (`Sys::watch_levels`): wait until
                    // any visible level row has reported its `%` before snapshotting `before`, so the background reply
                    // does not land mid-step and make `left` appear not to restore `before`.
                    let mut r = r;
                    if matches!(name.as_str(), "Brightness" | "Volume") {
                        let settled = record_until(&mut d, |q| {
                            q.rows().iter().filter(|row| matches!(row.first().map(String::as_str), Some("Brightness" | "Volume"))).all(|row| row.iter().any(|s| s.contains('%') || s == "Muted"))
                        });
                        if let Some(q) = settled {
                            r = q;
                        }
                    }
                    let before = r.rows();
                    // One way changes it, the other takes it back (a level at its end: the other way first).
                    d.send("right");
                    let (one, back) = match record_until(&mut d, |q| q.rows() != before) {
                        Some(_) => ("right", "left"),
                        None => {
                            d.send("left");
                            assert!(record_until(&mut d, |q| q.rows() != before).is_some(), "{name}: neither Right nor Left changes it");
                            ("left", "right")
                        }
                    };
                    d.send(back);
                    assert!(record_until(&mut d, |q| q.rows() == before).is_some(), "{name}: {back} takes {one} back");
                    // A: changes it in place (or nothing on a level), never opens a page.
                    d.send("a");
                    let after = record_until(&mut d, |q| q.at.starts_with("settings:List") && q.rows() != before)
                        .or_else(|| record_until(&mut d, |q| q.at.starts_with("settings:List")));
                    let after = after.unwrap_or_else(|| panic!("{name}: A left the list"));
                    if after.rows() != before {
                        for key in ["left", "right"] {
                            if Record::of(&nav_record(&mut d)).rows() != before {
                                d.send(key);
                                let _ = record_until(&mut d, |q| q.rows() == before);
                            }
                        }
                        assert!(Record::of(&nav_record(&mut d)).rows() == before, "{name}: A then Left/Right comes back");
                    }
                    values.push(name);
                } else {
                    assert!(r.says("Open"), "{name}: a row that doesn't change in place opens a page: {:?}", r.placed);
                    assert!(row.last().is_some_and(|m| m == ">"), "{name} opens a page and ends in its angle: {row:?}");
                    pages.push(name);
                }
                d.send("down");
            }
            d.send("r1");
        }
        // The rows of the hand walk, and the four switches that took only A.
        for name in ["Zoom in the pause menu", "Continue where I quit", "Console boot screen", "Remind me where I was", "Text size", "Language", "Lived-in shelf", "Shelf space"] {
            assert!(values.iter().any(|v| v == name), "{name} was walked as a row that changes in place: {values:?}");
        }
        assert!(pages.iter().any(|p| p == "Layout") && pages.iter().any(|p| p == "Cozy extras"), "{pages:?}");
        // The quick settings: every row with a value but Power… changes with Left/Right, the selected one between ‹ ›.
        d.send("b");
        d.sees("Game Boy");
        d.send("home");
        d.sees("Brightness");
        // A radio says "Switching…" until it has: the other way only counts once it is done.
        let settled = |q: &Record| !q.text().iter().any(|t| t.starts_with("Switching"));
        let mut walked = Vec::new();
        for _ in 0..12 {
            let r = Record::of(&nav_record(&mut d));
            let row = r.selected();
            let name = row.first().cloned().unwrap_or_default();
            if walked.contains(&name) {
                break;
            }
            walked.push(name.clone());
            if row.iter().any(|m| m == "‹") {
                let before = r.text();
                d.send("left");
                let (one, back) = if record_until(&mut d, |q| q.text() != before && settled(q)).is_some() {
                    ("left", "right")
                } else {
                    d.send("right");
                    assert!(record_until(&mut d, |q| q.text() != before && settled(q)).is_some(), "quick settings, {name}: neither Left nor Right changes it");
                    ("right", "left")
                };
                d.send(back);
                assert!(record_until(&mut d, |q| q.text() == before).is_some(), "quick settings, {name}: {back} takes {one} back");
            }
            d.send("down");
        }
        for name in ["Brightness", "Volume", "Wi-Fi"] {
            assert!(walked.iter().any(|w| w == name), "{name} in the quick settings: {walked:?}");
        }
    }

    /// The hand test of 0.5 (17.11.2026), in the game: which pause menu rows open a page wasn't clear, and a value
    /// changed only with A. The whole class, as a player walks it with the stand-in game: on the main page every row
    /// but the ones that act right there (Continue, Save, Load, Earlier, Disc, Quit) opens a page, ends in the drawn
    /// angle ">" and A's hint says Open; on Options and every list it opens, a row either opens a page (hint Open, its
    /// angle, A leaves for that page) or changes in place (the selected value between ‹ ›, the hint "< > Change", Left or
    /// Right changes it and the other way takes it back) or does a thing (neither). Missed before: the pause menu's
    /// tests read its rows' names and titles, never their marks or what Left/Right do; Settings had its walk (the test
    /// above), the pause menu not.
    #[test]
    fn every_pause_menu_row_that_changes_in_place_takes_left_and_right_and_every_page_row_shows_its_angle() {
        // A pocket-core game (the stand-in host), as most games of the handhelds: Save, Load and Options' pages.
        let mut d = Driver::start("pause-rows", true, |tmp, cmd| {
            std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false}"#).unwrap();
            let host = tmp.join("pocket-core-stand-in");
            std::fs::write(&host, "").unwrap();
            cmd.env("POCKETUI_HOST_BIN", host).env("POCKETUI_STATES", tmp.join("states")).env("POCKETUI_NAV_PROBE", tmp.join("nav.txt"));
        });
        d.sees("Game Boy");
        d.send("a");
        d.sees("Tobu Tobu Girl");
        d.send("a");
        d.wait(|d| matches!(std::fs::read_to_string(d.card.tmp.join("run-game.started")).as_deref(), Ok("started\n")));
        d.send("home");
        d.sees("Continue");
        let acts = ["Continue", "Save", "Load", "Earlier", "Disc", "Save & quit", "Quit"];
        // The page with its focus painted (a frame taken while it opens may have neither yet).
        let on_main = |q: &Record| q.at == "pause:Main" && q.placed.iter().any(|(_, s)| s == "‹nav:focus›");
        // The main page from its selected row down, until a name comes again (the list wraps).
        let mut main: Vec<String> = Vec::new();
        for _ in 0..12 {
            let r = record_until(&mut d, on_main).unwrap_or_else(|| panic!("the pause menu's main page: {main:?}\n{}", nav_record(&mut d)));
            let row = r.selected();
            let name = row.first().cloned().unwrap_or_default();
            if main.contains(&name) {
                break;
            }
            main.push(name.clone());
            if acts.contains(&name.as_str()) {
                assert!(!row.iter().any(|m| m == ">"), "main page, {name} acts right there, so no page's angle: {row:?}");
                if row.iter().any(|m| m == "‹") {
                    let before = r.selected();
                    d.send("right");
                    assert!(record_until(&mut d, |q| on_main(q) && q.selected() != before).is_some(), "main page, {name}: Right steps its value");
                    d.send("left");
                    assert!(record_until(&mut d, |q| on_main(q) && q.selected() == before).is_some(), "main page, {name}: Left takes it back");
                }
            } else {
                assert!(row.last().is_some_and(|m| m == ">"), "main page, {name} opens a page and ends in its angle: {row:?}");
                assert!(r.says("Open"), "main page, {name}: A's hint says Open: {:?}", r.text());
            }
            d.send("down");
        }
        // The walk ended on its first row again: down to Options, A.
        let to = main.iter().position(|n| n == "Options").unwrap_or_else(|| panic!("Options on the main page: {main:?}"));
        for _ in 0..to {
            d.send("down");
        }
        d.send("a");
        assert!(record_until(&mut d, |q| q.at == "pause:Options").is_some(), "A on Options opens it: {main:?}\n{}", nav_record(&mut d));
        let mut opened = Vec::new();
        let rows = walk_pause_page(&mut d, "pause:Options", &mut opened);
        // Zoom no longer second, under Picture: after the groups (0.5.1, as Settings' Zoom in the pause menu went down to
        // Playing).
        let pos = |n: &str| rows.iter().position(|r| r == n);
        assert!(pos("Speed, rumble, fan").is_some() && pos("Zoom") > pos("Speed, rumble, fan"), "Options: {rows:?}");
        for name in ["Picture", "Shortcuts", "Speed, rumble, fan", "Runs on"] {
            assert!(opened.iter().any(|o| o == name), "Options › {name} was opened: {opened:?}");
        }
    }

    /// Options → Picture → Mode 7 (`mode7_hd`), as a player reaches it in every language: a Super Nintendo game on a
    /// handheld that offers it (the Mini V2's chip, bsnes-hd's file on the card), the pause menu, Options, Picture, down
    /// to Mode 7; its value changes in place (‹ Original ›, "< > Change", Right → HD) and is kept, so the next start plays
    /// HD (`mode7_hd::start_core`'s test takes it from the file to the core). Other consoles have no such row.
    #[test]
    fn mode7_is_reached_in_the_pause_menu_in_every_language_and_its_change_is_kept() {
        let tr = |lang: &str, en: &str| -> String {
            if lang == "en" {
                return en.to_string();
            }
            let tsv = std::fs::read_to_string(format!("{}/assets/i18n/{lang}.tsv", env!("CARGO_MANIFEST_DIR"))).unwrap();
            tsv.lines().find_map(|l| l.split_once('\t').filter(|(k, _)| *k == en).map(|(_, v)| v.to_string())).unwrap_or_else(|| en.to_string())
        };
        for lang in ["en", "de", "fr", "es", "it", "pt"] {
            let mut d = Driver::start(&format!("mode7-{lang}"), true, |tmp, cmd| {
                // A Super Nintendo shelf only, a real-looking game, bsnes-hd's file where the card has it.
                let snes = tmp.join("snes");
                std::fs::create_dir_all(&snes).unwrap();
                std::fs::write(snes.join("F-Zero (USA).sfc"), "x").unwrap();
                std::fs::write(tmp.join("es_systems.cfg"), format!("<?xml version=\"1.0\"?>\n<systemList>\n  <system><name>snes</name><fullname>Super Nintendo</fullname><path>{}</path><extension>.sfc</extension><command>true</command></system>\n</systemList>\n", snes.display())).unwrap();
                std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false}"#).unwrap();
                let cores = tmp.join("cores");
                std::fs::create_dir_all(&cores).unwrap();
                std::fs::write(cores.join("bsnes_hd_beta_libretro.so"), "x").unwrap();
                let host = tmp.join("pocket-core-stand-in");
                std::fs::write(&host, "").unwrap();
                cmd.env("POCKETUI_HOST_BIN", host).env("POCKETUI_STATES", tmp.join("states")).env("POCKETUI_NAV_PROBE", tmp.join("nav.txt"))
                    .env("POCKETUI_DEVICE", "miniv2").env("POCKETUI_CORES", &cores).env("POCKETUI_LANG", lang);
            });
            d.sees("Super Nintendo");
            d.send("a");
            d.sees("F-Zero");
            d.send("a");
            d.wait(|d| matches!(std::fs::read_to_string(d.card.tmp.join("run-game.started")).as_deref(), Ok("started\n")));
            d.send("home");
            let on = |at: &str| { let at = at.to_string(); move |q: &Record| q.at == at && q.placed.iter().any(|(_, s)| s == "‹nav:focus›") };
            // Main page: down to Options.
            let options = tr(lang, "Options");
            let mut seen = Vec::new();
            loop {
                let r = record_until(&mut d, on("pause:Main")).unwrap_or_else(|| panic!("{lang}: the pause menu\n{}", nav_record(&mut d)));
                let name = r.selected().first().cloned().unwrap_or_default();
                if name == options {
                    break;
                }
                assert!(!seen.contains(&name) && seen.len() < 12, "{lang}: no {options} on the main page: {seen:?}");
                seen.push(name);
                d.send("down");
            }
            d.send("a");
            // Options: Picture.
            let picture = tr(lang, "Picture");
            let mut seen = Vec::new();
            loop {
                let r = record_until(&mut d, on("pause:Options")).unwrap_or_else(|| panic!("{lang}: Options\n{}", nav_record(&mut d)));
                let name = r.selected().first().cloned().unwrap_or_default();
                if name == picture {
                    break;
                }
                assert!(!seen.contains(&name) && seen.len() < 20, "{lang}: no {picture} on Options: {seen:?}");
                seen.push(name);
                d.send("down");
            }
            d.send("a");
            let group = record_until(&mut d, |q| q.at.starts_with("pause:Group") && q.placed.iter().any(|(_, s)| s == "‹nav:focus›"))
                .unwrap_or_else(|| panic!("{lang}: Picture opens\n{}", nav_record(&mut d)));
            let at = group.at.clone();
            // Picture: down to Mode 7, the last row.
            let mut seen = Vec::new();
            let row = loop {
                let r = record_until(&mut d, on(&at)).unwrap_or_else(|| panic!("{lang}: back on {at}"));
                let row = r.selected();
                let name = row.first().cloned().unwrap_or_default();
                if name == "Mode 7" {
                    assert!(r.says(&tr(lang, "Change")), "{lang}: Mode 7 changes in place: {:?}", r.text());
                    break row;
                }
                assert!(!seen.contains(&name) && seen.len() < 20, "{lang}: no Mode 7 on Picture: {seen:?}");
                seen.push(name);
                d.send("down");
            };
            assert!(row.iter().any(|m| *m == tr(lang, "Original")), "{lang}: Mode 7 starts on Original: {row:?}");
            assert!(row.iter().any(|m| m == "‹"), "{lang}: its value between ‹ ›: {row:?}");
            d.send("right");
            let hd = record_until(&mut d, |q| on(&at)(q) && q.selected().iter().any(|m| m == "HD")).unwrap_or_else(|| panic!("{lang}: Right → HD\n{}", nav_record(&mut d)));
            assert!(hd.selected().first().is_some_and(|n| n == "Mode 7"));
            // Kept: the next start reads HD from the game's or the console's file.
            d.send("b");
            d.wait(|d| {
                let dir = d.card.tmp.join("data/game-settings");
                [dir.join("snes.json"), dir.join("snes/F-Zero (USA).json")].iter().any(|f| std::fs::read_to_string(f).is_ok_and(|t| t.contains("\"mode7\": \"hd\"") || t.contains("\"mode7\":\"hd\"")))
            });
        }
    }

    /// Walks the pause menu page whose `@where` is `at` from its selected row down until a name comes again: each row
    /// opens a page (hint Open, its angle; A leaves for it, a list there is walked the same way, B comes back; the
    /// row's name goes to `opened`), changes in place (‹ ›, "< > Change", Left/Right) or does a thing (neither).
    /// Returns the rows' names in order.
    fn walk_pause_page(d: &mut Driver, at: &str, opened: &mut Vec<String>) -> Vec<String> {
        let here = |q: &Record| q.at == at && q.placed.iter().any(|(_, s)| s == "‹nav:focus›");
        let mut names: Vec<String> = Vec::new();
        for _ in 0..40 {
            let r = record_until(d, here).unwrap_or_else(|| panic!("back on {at}"));
            let row = r.selected();
            let name = row.first().cloned().unwrap_or_default();
            if names.contains(&name) {
                break;
            }
            names.push(name.clone());
            let page_row = row.last().is_some_and(|m| m == ">");
            assert_eq!(page_row, r.says("Open"), "{at}, {name}: a page's angle and A's hint Open go together: {row:?} {:?}", r.text());
            if page_row {
                assert!(!row.iter().any(|m| m == "‹"), "{at}, {name}: a page's angle and a value's arrows: {row:?}");
                // Zoom shows the paused game's picture larger: the stand-in has none, and Zoom refuses then (`app_zoom`).
                if name == "Zoom" {
                    d.send("down");
                    continue;
                }
                d.send("a");
                let next = record_until(d, |q| q.at != at && !q.at.is_empty()).unwrap_or_else(|| panic!("{at}, {name}: A opens its page"));
                opened.push(name.clone());
                let list = ["pause:Group", "pause:Controls", "pause:Cheats", "pause:Emulator"].iter().any(|p| next.at.starts_with(p));
                if list && next.placed.iter().any(|(_, s)| s == "‹nav:focus›") {
                    walk_pause_page(d, &next.at.clone(), opened);
                }
                d.send("b");
                assert!(record_until(d, here).is_some(), "{at}, {name}: B comes back from {}", next.at);
            } else if row.iter().any(|m| m == "‹") {
                assert!(r.says("Change"), "{at}, {name} changes in place: the hint says < > Change: {:?}", r.text());
                let before = r.selected();
                d.send("right");
                let (one, back) = match record_until(d, |q| here(q) && q.selected() != before) {
                    Some(_) => ("right", "left"),
                    None => {
                        d.send("left");
                        assert!(record_until(d, |q| here(q) && q.selected() != before).is_some(), "{at}, {name}: neither Right nor Left changes it");
                        ("left", "right")
                    }
                };
                d.send(back);
                assert!(record_until(d, |q| here(q) && q.selected() == before).is_some(), "{at}, {name}: {back} takes {one} back");
            } else {
                assert!(!r.says("Change"), "{at}, {name}: the hint says Change, so its value stands between ‹ ›: {row:?}");
            }
            d.send("down");
        }
        names
    }


    /// The navigation record after `key` (`POCKETUI_NAV_PROBE`): where the UI is and what the key asked for (`@feedback`:
    /// "tick" for the haptic tick, "sound:Nav"…), whether or not menu haptics and sounds are on.
    fn feedback_of(d: &mut Driver, key: &str) -> (String, String) {
        let nav = d.nav.clone().expect("POCKETUI_NAV_PROBE");
        let mut record = String::new();
        // A record first, so only `key`'s feedback is in the next one.
        for keys in ["nav".to_string(), format!("{key}\nnav")] {
            let _ = std::fs::remove_file(&nav);
            d.send(&keys);
            d.wait(|_| {
                record = std::fs::read_to_string(&nav).unwrap_or_default();
                record.ends_with("@end\n")
            });
        }
        let field = |name: &str| record.lines().find_map(|l| l.strip_prefix(&format!("@{name}\t"))).unwrap_or_default().to_string();
        (field("where"), field("feedback"))
    }

    /// Every move of the selection asks for the haptic tick and a sound, on the pages of the shelf too (the navigation
    /// explorer walks the menus): until 2026-11-08 the switcher, a card's pickers and the console page moved without a tick
    /// (`App::ui_position` didn't list them). The settings stay as they are: haptics off by default, the sounds as set.
    #[test]
    fn every_move_on_the_shelf_side_asks_for_a_tick_and_a_sound() {
        let mut d = Driver::start("feedback-shelf", true, |tmp, cmd| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"split"}"#).unwrap();
            let recents = serde_json::json!([
                {"system": "gb", "name": "Tobu Tobu Girl", "path": tmp.join("gb/Tobu Tobu Girl (World).gb"), "last_played": 1794100000u64, "play_secs": 60},
                {"system": "segacd", "name": "Pocket CD Demo", "path": tmp.join("segacd/Pocket CD Demo (USA).cue"), "last_played": 1794000000u64, "play_secs": 60},
            ]);
            std::fs::write(tmp.join("data/recents.json"), recents.to_string()).unwrap();
            cmd.env("POCKETUI_NAV_PROBE", tmp.join("nav.txt"));
        });
        d.sees("Game Boy");
        let mut gaps = Vec::new();
        let mut moved = |d: &mut Driver, want: &str, key: &str| {
            let (at, said) = feedback_of(d, key);
            assert!(at.starts_with(want), "expected {want}, at {at}");
            let words: Vec<&str> = said.split(' ').collect();
            if !words.contains(&"tick") || !words.iter().any(|w| w.starts_with("sound:")) {
                gaps.push(format!("{at}: {key} gave \"{said}\""));
            }
        };
        moved(&mut d, "shelf:Home", "down");
        moved(&mut d, "shelf:Home", "up");
        // Recent games (SELECT).
        d.send("select");
        d.sees("Recent games");
        moved(&mut d, "recent games", "down");
        d.send("b");
        d.wait(|d| !d.frame().contains("Recent games"));
        // A game's card (X on the first recent game), and its status picker (Y).
        d.send("x");
        d.sees("None. Press Y");
        d.send("y");
        d.wait(|d| d.frame().contains("Playing now"));
        moved(&mut d, "card:Status", "down");
        d.send("b");
        d.send("b");
        d.wait(|d| !d.frame().contains("None. Press Y"));
        // A system's console page (X on Game Boy, below the recent games).
        let (at, _) = feedback_of(&mut d, "down\ndown");
        assert!(at.starts_with("shelf:"), "{at}");
        d.send("x");
        d.wait(|d| d.nav.clone().is_some_and(|_| true) && d.frame().contains("Game Boy"));
        let (at, _) = feedback_of(&mut d, "");
        if at == "console" {
            moved(&mut d, "console", "down");
        }
        assert!(gaps.is_empty(), "moves without a tick or a sound:\n{}", gaps.join("\n"));
    }

    /// scripts/ui-status.sh, as a helper on a device runs it, against this UI's status file and command FIFO: its exit code,
    /// what it printed and what it said on stderr.
    fn ui_status(d: &Driver, args: &[&str]) -> (i32, String, String) {
        let out = Command::new("sh")
            .arg(format!("{ROOT}/scripts/ui-status.sh"))
            .args(args)
            .env("POCKETUI_STATUS", d.card.tmp.join("ui.status"))
            .env("POCKETUI_FIFO", d.card.tmp.join("ui.in"))
            .output()
            .unwrap();
        (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
    }

    /// A second UI on the Brick once replaced the owner's public status. A private probe must keep the
    /// old destination intact while publishing its own changing page and PID to the requested path.
    #[test]
    fn a_probe_publishes_its_own_status_without_replacing_the_owner_status() {
        let mut d = Driver::start("private-status", true, |tmp, cmd| {
            std::fs::write(tmp.join("ui.status"), "owner status\n").unwrap();
            cmd.env("POCKETUI_STATUS", tmp.join("probe.status"));
        });
        d.sees("Settings");
        d.wait(|d| std::fs::read_to_string(d.card.tmp.join("probe.status")).is_ok_and(|s| s.contains("place=shelf:Home")));
        d.send("start");
        d.wait(|d| std::fs::read_to_string(d.card.tmp.join("probe.status")).is_ok_and(|s| s.contains("place=settings:List Connections")));
        let status = std::fs::read_to_string(d.card.tmp.join("probe.status")).unwrap();
        assert!(status.contains(&format!("\tpid={}\t", d.child.id())), "{status}");
        assert_eq!(std::fs::read_to_string(d.card.tmp.join("ui.status")).unwrap(), "owner status\n");
    }

    /// Device-test incident 2026-11-08: a key helper pressed B to leave a page, then A for the page it expected, and the
    /// A landed on the shelf and started a game. The helper reads where the UI is (/tmp/pocket-ui.status, here
    /// <dir>/ui.status) before every key: `--at` waits for the page, A where it would start a game is refused, and a dark
    /// screen is woken by `wake`, which no page takes as a button. Missed before because nothing on a device could say
    /// where the UI was: `@where` existed only for the headless explorer.
    #[test]
    fn a_helper_reads_where_the_ui_is_and_never_starts_a_game_blind() {
        let mut d = Driver::start("ui-status", true, |tmp, cmd| {
            std::fs::write(tmp.join("data/sleep.json"), r#"{"screen_off_secs":0,"sleep_after_min":0,"short_press":"screen_off"}"#).unwrap();
            cmd.env("POCKETUI_POWER_KEY", "1").env("POCKETUI_BACKLIGHT", tmp.join("no-backlight"));
        });
        d.sees("Settings");
        let (rc, out, err) = ui_status(&d, &["expect", "shelf:Home", "20"]);
        assert_eq!((rc, out.trim()), (0, "shelf:Home"), "{err}");
        let line = std::fs::read_to_string(d.card.tmp.join("ui.status")).unwrap();
        assert!(line.starts_with("v=1\trev=") && line.contains("\tplace=shelf:Home\tcover=none\tsleep=awake\tgame=none"), "{line}");
        // The incident's shape, guarded: Settings open, then "B, A" for a page that is no longer there.
        assert_eq!(ui_status(&d, &["send", "--at", "shelf:Home", "start"]).0, 0);
        assert_eq!(ui_status(&d, &["expect", "settings:List Connections", "10"]).0, 0);
        let (rc, _, err) = ui_status(&d, &["send", "--at", "settings:List Connections", "b", "a"]);
        assert_eq!(rc, 3, "A on the shelf after B must be refused: {err}");
        assert!(err.contains("starts a game"), "{err}");
        assert_eq!(ui_status(&d, &["expect", "shelf:Home", "5"]).0, 0, "B went back to the shelf");
        // --at a page the UI isn't on: nothing pressed.
        let (rc, _, err) = ui_status(&d, &["send", "--at", "settings:*", "a"]);
        assert_eq!(rc, 3, "{err}");
        std::thread::sleep(Duration::from_millis(500));
        assert!(!d.card.tmp.join("run-game.started").exists(), "no game was started: {}", d.log());
        // A dark screen: a key would only wake it (and the next one would land on a page the helper didn't mean).
        assert_eq!(ui_status(&d, &["send", "start"]).0, 0);
        assert_eq!(ui_status(&d, &["expect", "settings:List Connections", "10"]).0, 0);
        d.send("power");
        d.wait(|d| d.log().contains("sleep: power key, screen off"));
        d.wait(|d| std::fs::read_to_string(d.card.tmp.join("ui.status")).is_ok_and(|l| l.contains("\tsleep=light\t")));
        let (rc, _, err) = ui_status(&d, &["send", "b"]);
        assert_eq!(rc, 4, "a key on a dark screen is refused: {err}");
        let (rc, _, err) = ui_status(&d, &["send", "wake"]);
        assert_eq!(rc, 0, "{err}");
        let (rc, out, err) = ui_status(&d, &["expect", "settings:List Connections", "5"]);
        assert_eq!((rc, out.trim()), (0, "settings:List Connections"), "wake pressed nothing: still on the page it woke on ({err})");
    }

    #[test]
    fn an_analog_trigger_wakes_the_screen_without_moving_the_shelf() {
        let mut ui = Driver::start("trigger-wake", true, |tmp, cmd| {
            std::fs::write(tmp.join("data/sleep.json"), r#"{"screen_off_secs":0,"sleep_after_min":0,"short_press":"screen_off"}"#).unwrap();
            // No real backlight or suspend backend: only the application input/sleep state is exercised.
            cmd.env("POCKETUI_POWER_KEY", "1").env("POCKETUI_BACKLIGHT", tmp.join("no-backlight"));
        });
        let clockless = |t: String| t.lines().filter(|l| !(l.len() == 5 && l.as_bytes()[2] == b':')).collect::<Vec<_>>().join("\n");
        let shelf = clockless(ui.sees("Settings"));
        ui.send("power");
        ui.wait(|d| d.log().contains("sleep: power key, screen off"));
        // Extra travel after the waking edge belongs to that same pull, including inside the wake guard.
        ui.send("r2axis");
        ui.wait(|d| d.log().contains("sleep: awake"));
        assert_eq!(clockless(ui.frame()), shelf, "the entire wake pull is swallowed: {}", ui.log());
        assert!(!ui.card.tmp.join("run-game.started").exists());
    }

    /// Settings → Games → Shelf space (`app_crate`), as a player does it: switched on (10 places, the games played last),
    /// a game taken out of the crate for one on the shelf, switched off again; then every game is back on the shelf
    /// and Home has no crate. Fourteen Game Boy games and the Sega CD one: five in the crate.
    #[test]
    fn shelf_space_on_a_swap_and_off_again_puts_every_game_back() {
        let mut d = Driver::start("shelf-space", true, |tmp, cmd| {
            for i in 0..13 {
                std::fs::write(tmp.join(format!("gb/Made-up Puzzle {i:02} (World).gb")), "x").unwrap();
            }
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"shelf"}"#).unwrap();
            cmd.env("POCKETUI_NAV_PROBE", tmp.join("nav.txt"));
        });
        let data = d.card.tmp.join("data");
        let space = |d: &Driver| std::fs::read_to_string(d.card.tmp.join("data/shelf_space.json")).unwrap_or_default();
        // Settings → Games (the sixth category), down to Shelf space (the end of its line: the line wraps).
        let to_the_row = |d: &mut Driver| {
            d.send("start");
            d.sees("Bluetooth");
            d.send("r1\nr1\nr1\nr1\nr1");
            d.sees("Systems on the shelf");
            for _ in 0..12 {
                if d.frame().contains("Against the backlog.") {
                    return;
                }
                d.send("down");
            }
            panic!("Shelf space is not in Games: {}", d.frame());
        };
        // Home's entries one by one from the first: the one whose words contain `want`, or None.
        let find_on_home = |d: &mut Driver, want: &str| -> Option<String> {
            let len = Record::of(&nav_record(d)).list.map_or(30, |l| l.1);
            d.send(&"left\n".repeat(len + 2));
            for _ in 0..len {
                let f = d.frame();
                if f.contains(want) {
                    return Some(f);
                }
                d.send("right");
            }
            None
        };
        d.sees("Game Boy");
        to_the_row(&mut d);
        // Right: Off → 10, and the question; the games played last is chosen already.
        d.send("right");
        let ask = d.sees("Which games go on the shelf?");
        assert!(ask.contains("The games I played last") && ask.contains("My favourites") && ask.contains("Cancel"), "{ask}");
        d.send("a");
        d.wait(|d| space(d).contains(r#""limit":10"#));
        // Home: the crate, saying what is in it.
        d.send("b");
        d.sees("Game Boy");
        assert!(find_on_home(&mut d, "5 games in the crate, the shelf is full").is_some(), "Home's Crate row: {}", d.frame());
        // Into the crate; A on its first game: the shelf is full, so the shelf's game played longest ago makes room.
        d.send("a");
        d.sees("Swap");
        d.send("a");
        d.sees("goes into the crate for");
        let before = space(&d);
        d.send("a");
        d.sees("is on the shelf now");
        let after = space(&d);
        assert_ne!(before, after, "the swap is saved");
        assert_eq!(after.matches(".gb\"").count() + after.matches(".cue\"").count(), 10, "still ten on the shelf: {after}");
        // Off again: every game back on the shelf, Home without the crate, Game Boy's list all fourteen. B from the crate
        // lands on its row.
        d.send("b");
        d.sees("games in the crate");
        to_the_row(&mut d);
        d.send("left");
        d.wait(|d| space(d).contains(r#""limit":null"#));
        d.send("b");
        d.wait_frame(|f| f.contains("Quick settings"));
        assert!(find_on_home(&mut d, "in the crate").is_none(), "Off: no crate on Home");
        find_on_home(&mut d, "Your games").expect("Game Boy on Home");
        d.send("a");
        d.wait(|d| Record::of(&nav_record(d)).list.is_some_and(|l| l.1 == 14));
    }

    /// Shelf space in the Easy look (19.11.2026: Easy listed every game on the card, the crate's too, and counted them all):
    /// only the shelf's games, counted as the shelf's. Three of the fifteen games on the shelf.
    #[test]
    fn easy_shows_only_the_shelf_s_games_with_shelf_space_on() {
        let mut d = Driver::start("shelf-space-easy", true, |tmp, _cmd| {
            for i in 0..13 {
                std::fs::write(tmp.join(format!("gb/Made-up Puzzle {i:02} (World).gb")), "x").unwrap();
            }
            let shelf: Vec<String> = (0..3).map(|i| tmp.join(format!("gb/Made-up Puzzle {i:02} (World).gb")).to_string_lossy().into_owned()).collect();
            std::fs::write(tmp.join("data/shelf_space.json"), serde_json::json!({"limit": 3, "shelf": shelf}).to_string()).unwrap();
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"big"}"#).unwrap();
        });
        let f = d.wait_frame(|f| f.contains(" of ") && f.contains("Game Boy"));
        assert!(f.contains("1 of 3"), "Easy counts the shelf's three games: {f}");
        // Right through the list: it goes round after the third.
        d.send("right\nright\nright");
        let f = d.wait_frame(|f| f.contains("1 of 3"));
        assert!(f.contains("Made-up Puzzle 00"), "back on the first after three: {f}");
    }


}

#[test]
fn wifi_timeout_explains_the_next_step_without_nmcli_diagnostics() {
    let card = Card::new("wifi-timeout");
    let text = shot(&card.tmp, card.stub.as_ref(), "setup:wifi-timeout", "large");
    assert!(text.contains("Network timed out. Try again."), "{text}");
    assert!(!text.contains("Timeout expired") && !text.contains("45 seconds") && !text.contains("Error:"), "{text}");
}

/// Large text on a 4:3 screen (960 lines, the Mini V1): the readout panel of Performance & fan keeps clear of the hint bar,
/// the descriptions above it give way (found by `scripts/size-sweep.sh`, 2026-10-29: "Battery draw" ran under the hints).
#[test]
fn performance_readout_stays_above_the_hints_at_large_text_on_960_lines() {
    let card = Card::new("perf-960");
    let boxes = card.tmp.join("perf.boxes");
    let png = card.tmp.join("perf.png");
    std::fs::create_dir_all(card.tmp.join("data")).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(stub) = &card.stub {
        cmd.env("WAYLAND_DISPLAY", &stub.path);
    }
    let out = cmd
        .args(["--snapshot", "perf:2:veryquiet", png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", "large")
        .env("POCKETUI_SNAPSHOT_SIZE", "1280x960")
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_DATA", card.tmp.join("data"))
        .env("POCKETUI_ES_SYSTEMS", card.tmp.join("es_systems.cfg"))
        .output()
        .unwrap();
    assert!(out.status.success() && png.exists(), "{}", String::from_utf8_lossy(&out.stderr));
    let found = std::fs::read_to_string(boxes).unwrap_or_default();
    assert!(found.trim().is_empty(), "text runs into other text or the hints:\n{found}");
}

/// The visible lines between `from` and `to` (the free slots' numbers between the picture and the note).
fn between<'a>(text: &'a str, from: &str, to: &str) -> Vec<&'a str> {
    text.lines().skip_while(|l| !l.starts_with(from)).skip(1).take_while(|l| !l.starts_with(to)).collect()
}

#[test]
fn save_and_load_show_the_slim_row_and_with_the_setting_the_memory_card() {
    let card = Card::new("save-media");
    // A PlayStation game at Large text: under the picture the slim row of blocks (no words, nothing but the note);
    // Save is on slot 3 and says what it replaces.
    let eight = ["1", "2", "3", "4", "5", "6", "7", "8"];
    let text = card.shot("slot-save:psx", "large");
    assert!(between(&text, "No picture", "Slot 3").is_empty(), "the slim row draws no text: {text}");
    // The save's real time ("today 14:02", "yesterday 23:50"), not "just now"; at Large text the clock may wrap to a line of its own.
    let note = text.replace('\n', " ");
    let when = note.split("Slot 3 · replaces the save from ").nth(1).unwrap_or_default();
    let (day, clock) = when.split_once(' ').unwrap_or_default();
    let clock = clock.split(' ').next().unwrap_or_default();
    assert!(matches!(day, "today" | "yesterday"), "a real time, not \"just now\": {text}");
    assert!(clock.len() == 5 && clock.as_bytes()[2] == b':' && clock.bytes().filter(u8::is_ascii_digit).count() == 4, "{text}");
    assert!(!text.contains("min ago") && !text.contains("just now"), "{text}");
    let boxes = std::fs::read_to_string(card.tmp.join("slot-save-psx.boxes")).unwrap();
    assert!(boxes.is_empty(), "the slim row leaves its room or runs into text: {boxes}");
    // With Memory card in the pause menu: the card beside its eight numbered blocks (slots 1, 3 and 6 show the cover
    // with their number), the same note; a SNES game's Load the cartridge beside its save label.
    let text = card.shot("slot-save:psx:card", "large");
    assert_eq!(between(&text, "No picture", "Slot 3"), eight, "{text}");
    let boxes = std::fs::read_to_string(card.tmp.join("slot-save-psx-card.boxes")).unwrap();
    assert!(boxes.is_empty(), "the memory card leaves its room or runs into text: {boxes}");
    let text = card.shot("slot-load:snes:card", "normal");
    assert_eq!(between(&text, "No picture", "Slot 3"), eight, "{text}");
    // An empty slot on a game without a cover.
    let text = card.shot("slot-empty:gba:nocover", "large");
    assert!(text.contains("No save in slot 4 yet"), "{text}");
}

#[test]
fn the_info_page_says_how_many_slots_hold_a_save_and_says_so_when_none_do() {
    let card = Card::new("card-saves");
    let text = card.shot("info:split:gb:tobu:saves", "large");
    assert!(text.contains("Save slots") && text.contains("3 of 8") && text.contains("2 days ago"), "{text}");
    let text = card.shot("info:split:gb:tobu:saves=none", "normal");
    assert!(text.contains("Save slots") && text.contains("All empty"), "an empty medium says so: {text}");
}

/// The full card (X from a list) says under "Save slots" that a moment waits, and says nothing when none does, in German too.
#[test]
fn the_full_card_says_when_a_moment_waits_under_the_save_slots() {
    let card = Card::new("card-continue");
    let text = card.shot("curator:card:gb:tobu:saves:continue", "normal");
    assert!(text.contains("Save slots") && text.contains("Continue: 2 h ago"), "{text}");
    let text = card.shot("curator:card:gb:tobu:saves", "normal");
    assert!(text.contains("Save slots") && !text.contains("Continue"), "no moment, no line: {text}");
    // A game without slots (RetroArch, a standalone emulator): the same words as a row of "Your game".
    let text = card.shot("curator:card:gb:tobu:noslots:continue", "normal");
    assert!(!text.contains("Save slots") && text.contains("Continue") && text.contains("2 h ago"), "{text}");
}

/// The line about two games of one file name, on the smallest panel, in every language: over a pocket-core game and as RetroArch's
/// notice, the sentence whole and on the panel.
/// 21.11.2026 (review): settings and lists the SD card refused to save were logged and gone at the next start. The shelf
/// now says it (`save_trouble::CardRefusals`); on the smallest screen and in every language the sentence is whole, so a
/// player learns what is lost and what helps.
#[test]
fn the_card_refused_notice_fits_640x480_whole_in_every_language() {
    let card = Card::new("card-refused");
    let (data, png) = (card.tmp.join("data"), card.tmp.join("refused.png"));
    std::fs::create_dir_all(&data).unwrap();
    for (lang, last_word) in [("en", "start"), ("de", "weg"), ("fr", "démarrage"), ("it", "avvio"), ("es", "inicio"), ("pt", "inicialização")] {
        for state in ["notice:card-full", "notice:card-unwritable"] {
            let (words, boxes) = (card.tmp.join("refused.tsv"), card.tmp.join("refused.boxes"));
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
            if let Some(stub) = &card.stub {
                cmd.env("WAYLAND_DISPLAY", &stub.path);
            }
            let out = cmd
                .args(["--snapshot", state, png.to_str().unwrap()])
                .env("POCKETUI_MOCK", "1")
                .env("POCKETUI_LANG", lang)
                .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
                .env("POCKETUI_VISIBLE_TEXT", &words)
                .env("POCKETUI_TEXT_BOXES", &boxes)
                .env("POCKETUI_DATA", &data)
                .env("POCKETUI_ES_SYSTEMS", card.tmp.join("es_systems.cfg"))
                .output()
                .unwrap();
            assert!(out.status.success(), "{state} {lang}: {}", String::from_utf8_lossy(&out.stderr));
            let shown = std::fs::read_to_string(&words).unwrap_or_default();
            assert!(shown.contains(last_word) && !shown.contains('…'), "{state} {lang}: the sentence whole: {shown}");
            assert!(std::fs::read_to_string(&boxes).unwrap_or_default().is_empty(), "{state} {lang}: overlaps");
        }
    }
}

#[test]
fn the_twin_notice_fits_640x480_whole_in_every_language() {
    let card = Card::new("twin-notice");
    let (data, png) = (card.tmp.join("data"), card.tmp.join("twin.png"));
    std::fs::create_dir_all(&data).unwrap();
    for (lang, last_word) in [("en", "apart"), ("de", "trennen"), ("es", "separarlos")] {
        for state in ["game-twin-note", "notice:twin"] {
            let (words, boxes) = (card.tmp.join("twin.tsv"), card.tmp.join("twin.boxes"));
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
            if let Some(stub) = &card.stub {
                cmd.env("WAYLAND_DISPLAY", &stub.path);
            }
            let out = cmd
                .args(["--snapshot", state, png.to_str().unwrap()])
                .env("POCKETUI_MOCK", "1")
                .env("POCKETUI_LANG", lang)
                .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
                .env("POCKETUI_VISIBLE_TEXT", &words)
                .env("POCKETUI_TEXT_BOXES", &boxes)
                .env("POCKETUI_DATA", &data)
                .env("POCKETUI_ES_SYSTEMS", card.tmp.join("es_systems.cfg"))
                .output()
                .unwrap();
            assert!(out.status.success(), "{state} {lang}: {}", String::from_utf8_lossy(&out.stderr));
            if state == "game-twin-note" {
                let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                let at = stdout.find("pill=").unwrap_or_else(|| panic!("{stdout}")) + "pill=".len();
                let (w, h) = stdout[at..].split_whitespace().next().unwrap().split_once('x').unwrap();
                let (w, h): (i32, i32) = (w.parse().unwrap(), h.parse().unwrap());
                assert!(w <= 640 - 48 && h <= 240, "{lang}: {w}x{h} on a 640x480 panel");
                // Its own small picture, outside what the transcript records; lines wrap and are never cut.
                continue;
            }
            let shown = std::fs::read_to_string(&words).unwrap_or_default();
            assert!(shown.contains(last_word) && !shown.contains('…'), "{state} {lang}: the sentence whole: {shown}");
            assert!(std::fs::read_to_string(&boxes).unwrap_or_default().is_empty(), "{state} {lang}: overlaps");
        }
    }
}
