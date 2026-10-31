//! Small player-facing render journeys with a messy throwaway card, using the real application binary.
//! Assertions read the text recorded during rendering and the layout diagnostics, rather than private App fields.
//! No device, user library or external service is used.

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
    assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
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
    assert!(text.contains("turns itself off") && text.contains("Pocket restarts"), "{text}");
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

#[test]
fn offline_tailscale_stays_calm_before_asking_for_credentials() {
    let card = Card::new("tailscale-offline");
    let text = card.shot("tailscale:offline-note", "normal");
    assert!(text.contains("No network yet"), "{text}");
    assert!(text.contains("Connect to Wi-Fi"), "{text}");
    assert!(!text.contains("Timed out") && !text.contains("login.tailscale.com"), "{text}");
}

#[test]
fn offline_achievements_stays_calm_before_asking_for_credentials() {
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

    struct Driver {
        card: Card,
        child: std::process::Child,
    }
    /// Writes an executable in a child, as testutil::write_executable does: another test's fork must not inherit a
    /// descriptor that can write it (ETXTBSY).
    fn executable(path: &Path, body: &str) {
        let mut writer = Command::new("sh")
            .args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"])
            .arg(path)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = writer.stdin.take().unwrap();
        input.write_all(body.as_bytes()).unwrap();
        drop(input);
        assert!(writer.wait().unwrap().success());
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
            if onboarded {
                std::fs::write(data.join("settings.json"), r#"{"onboarded":true,"motion":"off","art":"off"}"#).unwrap();
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
            let child = cmd.spawn().unwrap();
            let mut driver = Self { card, child };
            driver.wait(|d| d.card.tmp.join("ui.in").exists());
            driver
        }
        fn wait(&mut self, mut condition: impl FnMut(&mut Self) -> bool) {
            let until = Instant::now() + Duration::from_secs(8);
            while Instant::now() < until {
                if condition(self) {
                    return;
                }
                assert!(self.child.try_wait().unwrap().is_none(), "UI exited: {}", self.log());
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!(
                "journey did not finish; visible text:\n{}\nUI log:\n{}",
                std::fs::read_to_string(self.card.tmp.join("frame.txt")).unwrap_or_default(),
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
            self.send("frame");
            self.wait(|_| png.exists());
            std::fs::read_to_string(self.card.tmp.join("frame.txt")).unwrap()
        }
        fn sees(&mut self, expected: &str) -> String {
            let mut text = String::new();
            self.wait(|d| {
                text = d.frame();
                text.contains(expected)
            });
            text
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

    #[test]
    fn changing_case_stickers_refreshes_the_standing_shelf_spine_in_the_same_list() {
        let mut ui = Driver::start("live-case-stickers", true, |tmp, cmd| {
            let data = tmp.join("data");
            std::fs::write(tmp.join("gb/Zeta (World).gb"), b"").unwrap();
            std::fs::write(data.join("settings.json"), r#"{"onboarded":true,"motion":"off","art":"off","theme":"shelf","details_page":true}"#).unwrap();
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
            std::fs::write(data.join("settings.json"), r#"{"onboarded":true,"motion":"off","art":"off","theme":"shelf","details_page":true}"#).unwrap();
            std::fs::write(data.join("curator.json"), r#"{"lens":false,"echoes":false,"stickers":false,"history":false}"#).unwrap();
            // Atomic replacement cannot rename a file over this directory: deterministic SD-write failure.
            std::fs::create_dir(data.join("library.json")).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("a");
        ui.sees("Details");
        ui.sticker_sheet();
        ui.send("a\nb\nb");
        let text = ui.sees("Could not save: is the SD card full?");
        assert!(text.contains("Details"), "the error must remain visible after returning to the shelf: {text}");
    }

    #[test]
    fn fresh_card_through_setup_to_a_visible_game() {
        let mut ui = Driver::new("first-start", false);
        let first = ui.frame();
        assert!(first.contains("Welcome to your Pocket"), "first start should explain setup: {first}");
        // Welcome, Look, Simple, skip Wi-Fi/time/extras, Games, Done. These are ordinary FIFO buttons.
        ui.send("a\na\na\nstart\nstart\nstart\nstart\na");
        ui.sees("Game Boy");
        ui.send("a");
        let text = ui.sees("Tobu Tobu Girl");
        assert!(!text.contains("Ghost") && !text.contains("README"), "{text}");
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
            shelf.contains("Tobu Tobu Girl") && !shelf.contains("Quit") && !shelf.contains("Resume")
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
        ui.sees("START");
        ui.send("start");
        let page = ui.sees("This game · Game Boy");
        for row in ["Screen scaling", "Screen sharpness", "Screen frame"] {
            assert!(page.contains(row), "{row} is missing: {page}");
        }
        // Scaling, sharpness, effect, then the frame: one to the right is Auto.
        ui.send("down\ndown\ndown\nright");
        ui.sees("\u{2039} Auto \u{203a}");
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
        ui.sees("START");
        ui.send("start\ndown\ndown\ndown");
        let again = ui.sees("\u{2039} Auto \u{203a}");
        assert!(again.contains("This game has its own settings."), "{again}");
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

    /// The Cabinet look, then Settings over it: a tap on a category is the category's, never a case's that stood
    /// there a frame before (the cases' tap areas caught it and confirmed the selected row; review 2026-10-30).
    /// Taps come through the command FIFO ("tap x y", fractions of the screen) as the touch screen sends them.
    #[test]
    fn a_tap_on_settings_over_the_cabinet_is_settings() {
        let mut ui = Driver::start("cabinet-tap", true, |tmp, _| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"motion":"off","art":"off","theme":"cabinet"}"#).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("start");
        ui.sees("Sound & feel");
        // The categories' rows on the driver's 1240x1080 window: their middles at 165 + 66 px each. Taps across the
        // whole page, never twice in a row on the same category nor first on the selected one (that would open it), so wherever a case
        // stood one of them lands on it; each only selects its category. A frame between taps, as on the device.
        let row = |g: usize| (165.0 + 66.0 * g as f32) / 1080.0;
        for x in [0.1, 0.3, 0.5, 0.7, 0.9] {
            for g in [2, 4, 6, 1, 3, 5, 0] {
                ui.send(&format!("tap {x} {:.4}", row(g)));
                ui.frame();
            }
        }
        // Then Playing, the third: the first tap selects it, the second opens it.
        ui.send(&format!("tap 0.5 {:.4}", row(2)));
        ui.frame();
        ui.send(&format!("tap 0.5 {:.4}", row(2)));
        let text = ui.sees("Remember where I quit");
        assert!(!text.contains("Systems on the shelf"), "the Look list (the row that was selected) did not open: {text}");
    }

    /// The cozy extras where they live since 2026-10-30 answer a finger as every Settings row does: a tap selects a
    /// row, the next one on it switches it. Sound & feel → Playful cases, Look → Cozy extras → Stickers on cases and
    /// System → Sleep & power → Slideshow before screen off, each landing in settings.json's "extras".
    #[test]
    fn a_tap_switches_the_cozy_extras_where_they_live_now() {
        let mut ui = Driver::new("extras-tap", true);
        ui.send("start");
        ui.sees("Sound & feel");
        // On the driver's 1240x1080 window the rows' middles are at 165 + 66 px each, the categories' and the lists' alike.
        let tap = |ui: &mut Driver, r: usize| {
            ui.send(&format!("tap 0.5 {:.4}", (165.0 + 66.0 * r as f32) / 1080.0));
            ui.frame();
        };
        let settings = ui.card.tmp.join("data/settings.json");
        let extra = move |key: &str| -> serde_json::Value {
            let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap_or_default()).unwrap_or_default();
            s["extras"][key].clone()
        };
        // Sound & feel, the second category: select, open; Playful cases, its second row: select, switch.
        tap(&mut ui, 1);
        tap(&mut ui, 1);
        ui.sees("Playful cases");
        tap(&mut ui, 1);
        tap(&mut ui, 1);
        ui.wait(|_| extra("playful_animations").is_boolean());
        // Look, then its last row Cozy extras: the page; Stickers on cases, its second row.
        ui.send("b");
        ui.sees("Help & tools");
        tap(&mut ui, 0);
        tap(&mut ui, 0);
        ui.sees("Systems on the shelf");
        tap(&mut ui, 7);
        tap(&mut ui, 7);
        ui.sees("TV on the cabinet");
        tap(&mut ui, 1);
        tap(&mut ui, 1);
        ui.wait(|_| extra("case_stickers") == serde_json::json!(false));
        // System → Sleep & power: the slideshow is the page's second row, off until switched.
        ui.send("b\nb");
        ui.sees("Help & tools");
        tap(&mut ui, 5);
        tap(&mut ui, 5);
        ui.sees("Sleep & power");
        tap(&mut ui, 2);
        tap(&mut ui, 2);
        ui.sees("Slideshow before screen off");
        assert!(extra("idle_slideshow").is_null(), "off until switched");
        tap(&mut ui, 1);
        tap(&mut ui, 1);
        ui.wait(|_| extra("idle_slideshow") == serde_json::json!(true));
    }

    #[test]
    fn quick_wifi_off_and_on_agrees_with_the_wifi_page() {
        let mut ui = Driver::new("wifi", true);
        ui.send("home\ndown\ndown\na");
        ui.sees("Wi-Fi\nOff");
        ui.send("b\nstart\ndown\ndown\ndown\ndown\na\na");
        let off = ui.sees("Wi-Fi is off");
        assert!(
            !off.contains("Connected") && !off.contains("Connect\n"),
            "off must not claim a connection: {off}"
        );
        // Back from Wi-Fi and Settings, switch the same radio on again, then reopen the Wi-Fi page.
        ui.send("b\nb\nb\nhome\ndown\ndown\na");
        ui.sees("Wi-Fi\nOn");
        ui.send("b\nstart\ndown\ndown\ndown\ndown\na\na");
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
        // Welcome, Look, Simple: the Wi-Fi step, which scans by itself.
        ui.send("a\na\na");
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
            // Back to the list: the network wants its password, it isn't "Saved" nor "Connecting…".
            ui.send("b");
            let list = ui.sees("Neighbour");
            assert!(!list.contains("Connecting") && !list.contains("Saved") && !list.contains("Connected"), "{list}");
        }
        // The right one: "w" is one to the right of "q".
        ui.send("a");
        ui.sees("Password for Wifi");
        ui.send(&("right\n".to_string() + &"a\n".repeat(8) + "start"));
        ui.sees("Connected to Wifi");
        // The list after the connection marks it in use (and saved: Y forgets).
        ui.sees("Wifi\nConnected");
        // Forget it, then a second Y Y on the same row: the first forgets, the second has nothing left to forget.
        ui.send("up\nup\ny\ny");
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
    fn xbox_guest_starts_home_shows_its_menu_and_quit_gets_back() {
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
        assert!(menu.contains("keeps its own saves"), "a guest emulator's menu says that saves are its own: {menu}");
        // The menu takes buttons once it has settled (a loaded host got the presses too early).
        std::thread::sleep(Duration::from_millis(600));
        // The guest's menu wraps: Up from Continue is Quit (Down ×8 came back to Continue in this four-row menu).
        ui.send("up");
        std::thread::sleep(Duration::from_millis(200));
        ui.send("a");
        // A guest is asked to close first and ended after its grace period: the shelf is back, and the stand-in is gone
        // (it may be ended before its trap writes a marker, so the process is what is looked at).
        let mut shelf = String::new();
        let until = Instant::now() + Duration::from_secs(40);
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
        assert!(home.contains("Get the starter pack") && home.contains("/storage/roms/<system>"), "{home}");

        // Hold the real starter-scan at its config read. The headless driver has completed startup and skips the
        // device startup jobs, so opening this FIFO belongs to that scan. This forces the slow-worker interleaving
        // which made waiting for the page's header lose the second A press on busy build machines.
        let config = ui.card.tmp.join("es_systems.cfg");
        let xml = std::fs::read(&config).unwrap();
        std::fs::remove_file(&config).unwrap();
        let fifo = std::ffi::CString::new(config.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        ui.send("a");
        let loading = ui.sees("Looking at your SD card…");
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
        assert!(!ready.contains("Looking at your SD card"), "Confirm must follow the scan's result: {ready}");
        ui.send("a");
        ui.sees("The Pocket isn't online");
        assert!(ui.card.tmp.join("offline-bin/ip.calls").is_file(), "the real network check used the child fixture");
        assert!(!ui.card.tmp.join("offline-bin/curl.calls").exists(), "offline refuses before trying a download");
    }

    /// Delete from the SD card: the card's last row, the sheet, two A presses; the file is gone and the screen says so.
    #[test]
    fn deleting_a_game_asks_twice_then_removes_the_file() {
        let mut ui = Driver::new("delete", true);
        ui.sees("Game Boy");
        let rom = ui.card.tmp.join("gb/Tobu Tobu Girl (World).gb");
        // The game's card (X), turned over (right), its second page (SELECT), down to the last row: Hide or delete.
        ui.send("a\nx\nright\nselect");
        ui.sees("Hide or delete");
        ui.send(&"down\n".repeat(10));
        ui.send("a");
        ui.sees("Hide from the shelf");
        ui.send("down\na");
        let sheet = ui.sees("Delete Tobu Tobu Girl?");
        assert!(rom.exists(), "the sheet only asks: {sheet}");
        ui.send("a");
        let again = ui.sees("Press A again to delete");
        assert!(rom.exists(), "one A is not enough: {again}");
        ui.send("a");
        ui.sees("Deleted Tobu Tobu Girl");
        assert!(!rom.exists());
    }

    /// Two real ROM files and one cached checksum/DAT hit. B is deliberately absent from the DAT: resuming its
    /// hash must not create a new name-map change that accidentally repairs A's lost shelf refresh.
    fn named_cartridge_fixture(tmp: &Path, cmd: &mut Command) {
        let games = tmp.join("identified-games");
        std::fs::create_dir_all(&games).unwrap();
        let known = games.join("Supar Mario Land 2.gb");
        std::fs::write(&known, b"recognized A").unwrap();
        std::fs::write(games.join("Unknown Homebrew.gb"), b"unknown B").unwrap();
        std::fs::write(tmp.join("es_systems.cfg"), format!(
            "<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension><command>true</command></system></systemList>", games.display()
        )).unwrap();
        std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"theme":"minimal","motion":"off","art":"off"}"#).unwrap();
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
    fn a_stopped_identification_keeps_its_shelf_name_refresh_until_the_same_names_resume() {
        let mut ui = Driver::start("identify-stop-resume", true, named_cartridge_fixture);
        ui.sees("Game Boy");
        ui.wait(|d| d.log().contains("curator: 2 games"));
        ui.send("a");
        ui.sees("Supar Mario Land 2");
        ui.send("a");
        ui.wait(|d| d.card.tmp.join("run-game.started").exists());
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
        // Restore a normal pathname for later readers, then release only the old worker's open FIFO inode.
        std::fs::remove_file(&cache).unwrap();
        std::fs::write(&cache, &bytes).unwrap();
        held.write_all(&bytes).unwrap();
        drop(held);
        ui.wait(|d| d.log().contains("stopped for a game") && d.log().contains("identify: changed names kept for the shelf"));
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
        let mut ui = Driver::start("identify-lookup", true, |tmp, cmd| {
            named_cartridge_fixture(tmp, cmd);
            executable(&tmp.join("identify-bin/ip"), "#!/bin/sh\nprintf '2: wlan0 inet 203.0.113.42/24 scope global wlan0\\n'\n");
        });
        ui.sees("Game Boy");
        // Extras paints the actual cached IP state on every frame. Wait for online readiness before using a
        // network-dependent card action; curl is a child-local failing stub, so no request leaves the fixture.
        ui.send("start\ndown\ndown\ndown\na\ndown\ndown\na");
        ui.sees("Optional. You can get these any time later.");
        ui.send("b\nb\nb\nb");
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
            ui.send("x");
            ui.sees("Details");
            ui.send("y");
            ui.sees("Wrong pictures? Look up again");
            ui.send(&"down\n".repeat(8));
            ui.send("a");
            ui.wait(|d| d.log().lines().filter(|l| l.starts_with("look up again:") && l.contains(" -> ")).count() > prior_results);
            let path = ui.card.tmp.join("identified-games/Supar Mario Land 2.gb");
            let names = pocket_ui::rom_id::known(&ui.card.tmp.join("data"), &[("gb".into(), path.clone())]);
            assert_eq!(names.get(&path).map(String::as_str), expected, "the real lookup cache must contain the expected new mapping");
            ui.send("b");
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
            for n in [2, 3] {
                std::fs::write(games.join(format!("Pocket Review Racer {n}.gb")), "x").unwrap();
            }
            std::fs::write(tmp.join("es_systems.cfg"), format!(
                "<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension><command>true</command></system></systemList>", games.display()
            )).unwrap();
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"theme":"minimal","motion":"off","art":"off"}"#).unwrap();
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
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"motion":"off","art":"off","theme":"minimal"}"#).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("a");
        ui.sees("Tobu Tobu Girl");
        ui.send("x");
        let page = ui.sees("None. Press Y");
        assert!(page.contains("Tobu Tobu Girl") && page.contains("Details") && page.contains("No description yet."), "{page}");
        assert!(!page.contains("Turn over"), "the info page, not the box: {page}");
        ui.send("select");
        ui.sees("Hide or delete");
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
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"motion":"off","theme":"shelf"}"#).unwrap();
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
        ui.send("start\ndown\ndown\ndown\ndown\na\ndown\na");
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

    // ---- The idle slideshow (`app_slideshow`): the shelf left alone, then any button ----

    /// A small PNG (a picture of 200 x 280 pixels, no game's art).
    fn picture(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut enc = png::Encoder::new(std::fs::File::create(path).unwrap(), 200, 280);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let px: Vec<u8> = (0..200 * 280).flat_map(|i| [(i % 200) as u8, (i / 200 % 256) as u8, 90]).collect();
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }

    /// The driver on the throwaway card: a cover for the Game Boy game unless `covers` is false, the show after
    /// `after_ms`, and `settings` as the settings.json (Motion Off: plain fades, so frames are steady).
    fn quiet_shelf(name: &str, covers: bool, after_ms: u32, settings: &'static str) -> Driver {
        Driver::start(name, true, move |tmp, cmd| {
            std::fs::write(tmp.join("data/settings.json"), settings).unwrap();
            if covers {
                picture(&tmp.join("data/covers/gb/Tobu Tobu Girl (World).png"));
            }
            cmd.env("POCKETUI_SLIDESHOW_AFTER_MS", after_ms.to_string()).env("POCKETUI_SCREENSHOTS", tmp.join("screenshots"));
        })
    }

    #[test]
    fn the_idle_slideshow_plays_the_games_pictures_and_a_button_only_takes_you_back() {
        let mut ui = quiet_shelf("slideshow-back", true, 4000, r#"{"onboarded":true,"motion":"off","extras":{"idle_slideshow":true}}"#);
        let clockless = |t: String| t.lines().filter(|l| !(l.len() == 5 && l.as_bytes()[2] == b':')).collect::<Vec<_>>().join("\n");
        let shelf = clockless(ui.sees("Settings"));
        // After the quiet time: the game's name and its system, none of the shelf's hints.
        let mut show = String::new();
        ui.wait(|d| {
            show = d.frame();
            !show.contains("Settings") && show.contains("Tobu Tobu Girl")
        });
        assert!(show.contains("Game Boy"), "the system under the title: {show}");
        assert!(!show.contains("START") && !show.contains("Open"), "no hint bar over the show: {show}");
        // START would open Settings, A the game's card: either only ends the show.
        for button in ["start", "a", "x", "r2axis"] {
            ui.send(button);
            let back = clockless(ui.frame());
            assert_eq!(back, shelf, "after {button}: the shelf as it was, nothing opened");
            // The next quiet spell plays it again.
            ui.wait(|d| {
                show = d.frame();
                !show.contains("Settings")
            });
        }
        assert!(!ui.card.tmp.join("run-game.started").exists(), "no game was started by a press on the show: {}", ui.log());
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

    #[test]
    fn the_idle_slideshow_leaves_a_shelf_without_a_single_picture_alone() {
        let mut ui = quiet_shelf("slideshow-none", false, 800, r#"{"onboarded":true,"motion":"off","extras":{"idle_slideshow":true}}"#);
        ui.wait(|d| d.log().contains("slideshow: no pictures"));
        let text = ui.frame();
        assert!(text.contains("Settings") && text.contains("Game Boy"), "the shelf is still there: {text}");
    }

    #[test]
    fn the_idle_slideshow_is_off_in_every_look_until_the_page_switches_it_on() {
        // As it starts (the Shelf, then a plain look with pictures all the same): the shelf stays a shelf, however long it
        // is left alone.
        for (name, settings) in [
            ("slideshow-default-shelf", r#"{"onboarded":true,"motion":"off"}"#),
            ("slideshow-default-split", r#"{"onboarded":true,"motion":"off","theme":"split"}"#),
        ] {
            let mut ui = quiet_shelf(name, true, 600, settings);
            std::thread::sleep(Duration::from_millis(2500));
            let text = ui.frame();
            assert!(text.contains("Settings"), "{name}: still the shelf: {text}");
            assert!(!ui.log().contains("slideshow:"), "{name}: {}", ui.log());
        }
        // Switched on in the page (kept as extras.idle_slideshow): a plain look plays it too.
        let mut ui = quiet_shelf("slideshow-split-on", true, 600, r#"{"onboarded":true,"motion":"off","theme":"split","extras":{"idle_slideshow":true}}"#);
        ui.wait(|d| d.log().contains("slideshow: 1 pictures"));
    }
}

#[test]
fn wifi_timeout_explains_the_next_step_without_nmcli_diagnostics() {
    let card = Card::new("wifi-timeout");
    let text = shot(&card.tmp, card.stub.as_ref(), "setup:wifi-timeout", "large");
    assert!(text.contains("The network took too long to respond. Try again."), "{text}");
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
    assert!(between(&text, "No preview", "Slot 3").is_empty(), "the slim row draws no text: {text}");
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
    assert_eq!(between(&text, "No preview", "Slot 3"), eight, "{text}");
    let boxes = std::fs::read_to_string(card.tmp.join("slot-save-psx-card.boxes")).unwrap();
    assert!(boxes.is_empty(), "the memory card leaves its room or runs into text: {boxes}");
    let text = card.shot("slot-load:snes:card", "normal");
    assert_eq!(between(&text, "No preview", "Slot 3"), eight, "{text}");
    // An empty slot on a game without a cover.
    let text = card.shot("slot-empty:gba:nocover", "large");
    assert!(text.contains("Slot 4 · empty"), "{text}");
}

#[test]
fn the_info_page_says_how_many_slots_hold_a_save_and_says_so_when_none_do() {
    let card = Card::new("card-saves");
    let text = card.shot("info:split:gb:tobu:saves", "large");
    assert!(text.contains("Save slots") && text.contains("3 of 8") && text.contains("2 days ago"), "{text}");
    let text = card.shot("info:split:gb:tobu:saves=none", "normal");
    assert!(text.contains("Save slots") && text.contains("All empty"), "an empty medium says so: {text}");
}
