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
/// page lists it only for a player who asked (Easy to read, or Settings → Sound & feel → Zoom in the pause menu); everyone can
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
        // Easy to read is Large text and a Motion that isn't Full: either half alone is not it.
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
        assert!(first.contains("The shelf") && first.contains("Your games on a shelf, as it comes."), "{size}: {first}");
        assert!(!first.contains("Simple") && !first.contains("Picked now"), "{size}: the first start: nothing to set back yet: {first}");
        let later = card.shot("setup:play-later", size);
        assert!(later.contains("The shelf") && later.contains("Picked now") && later.contains("On start"), "{size}: {later}");
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
                page.contains(games) || page.contains("Settings › System › Time has the rest.") || page.contains("Several time zones here")
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

    /// A newer pocket-ui is said once on the free shelf (`POCKETUI_UPDATE_HINT` stands in for the signed list): A goes
    /// to Settings → System → Updates, B only closes it, and neither brings it back.
    fn update_ready(name: &str) -> Driver {
        Driver::start(name, true, |_, cmd| {
            cmd.env("POCKETUI_UPDATE_HINT", "99.0.0");
        })
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
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"minimal","motion":"off","appearance":"dark"}"#).unwrap();
        });
        let dark_words = clockless(dark.frame());
        let dark_brightness = brightness(&mut dark);
        let mut light = Driver::start("appearance-light", true, |tmp, _| {
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"minimal","motion":"off","appearance":"light"}"#).unwrap();
        });
        assert_eq!(dark_words, clockless(light.frame()), "the same game and controls remain visible in either appearance");
        assert!(brightness(&mut light) > dark_brightness + 100, "the player's Light setting must paint a light page");
        // Look's fourth row, Appearance (after Layout, Systems on the shelf, Cozy extras).
        dark.send("start\na\ndown\ndown\ndown\na\nb\nb");
        assert_eq!(dark_words, clockless(dark.frame()), "changing Appearance comes back to the same game");
        assert!(brightness(&mut dark) > dark_brightness + 100, "a Light choice in Settings takes effect in the next capture too");
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
        read_new_cards_without_refreshing_the_hidden_shelf(new_version_over_shelf("minimal", false), "A very long game title");
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

    fn controls_card_game(name: &str, resume: Option<&str>) -> Driver {
        Driver::start(name, true, |tmp, cmd| {
            std::fs::write(tmp.join("gb/Tobu Two.gb"), "game").unwrap();
            std::fs::write(tmp.join("data/search.json"), r#"{"list":["Tobu"]}"#).unwrap();
            std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false}"#).unwrap();
            let host = tmp.join("pocket-core-stand-in");
            std::fs::write(&host, "").unwrap();
            let states = tmp.join("states");
            cmd.env("POCKETUI_HOST_BIN", host).env("POCKETUI_STATES", &states);
            if let Some(suffix) = resume {
                std::fs::create_dir_all(states.join("gb")).unwrap();
                std::fs::write(states.join("gb").join(format!("Tobu Tobu Girl (World).pstate.auto{suffix}")), "resume").unwrap();
            }
        })
    }

    fn ask_for_controls_game(d: &mut Driver) {
        d.sees("Game Boy");
        d.send("a");
        d.sees("Tobu Tobu Girl");
        d.send("a");
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_controls_card_home_starts_immediately_without_opening_a_shortcut() {
        let mut d = controls_card_game("controls-home", None);
        ask_for_controls_game(&mut d);
        d.sees("Controls · Game Boy");
        assert!(!d.card.tmp.join("run-game.started").exists(), "no game input while the card is up: the game hasn't started");
        assert!(std::fs::read_to_string(d.card.tmp.join("data/controls-card-seen")).unwrap().contains("gb\n"), "remember before starting, even if the game fails");
        let pressed = Instant::now();
        d.send("home");
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        assert!(pressed.elapsed() < test_clock::timeout(Duration::from_secs(3)), "one press must start without waiting for the card's timeout");
        let text = d.frame();
        assert!(!text.contains("Brightness") && !text.contains("Controls ·"), "the dismissing HOME is consumed: {text}");
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_controls_card_starts_after_twelve_seconds_even_without_more_input() {
        let mut d = controls_card_game("controls-timeout", None);
        ask_for_controls_game(&mut d);
        d.sees("Controls · Game Boy");
        // No further frame requests or button events: the card must schedule its own deadline.
        let started = Instant::now();
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        assert!(started.elapsed() >= Duration::from_secs(10), "give the player time to read the card");
        assert!(started.elapsed() < test_clock::timeout(Duration::from_secs(15)), "the card must not wait for unrelated events");
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_controls_card_never_interrupts_a_resume_after_quit_or_a_crash() {
        for suffix in ["", ".taken", ".taken.retry"] {
            let mut d = controls_card_game("controls-resume", Some(suffix));
            ask_for_controls_game(&mut d);
            let started = Instant::now();
            d.wait(|d| d.card.tmp.join("run-game.started").exists());
            assert!(started.elapsed() < test_clock::timeout(Duration::from_secs(3)), "Continue must start immediately ({suffix})");
            assert!(!d.log().contains("controls card: first start"), "no first-start card over a resume: {}", d.log());
        }
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_controls_card_tap_cannot_select_a_search_result_underneath() {
        let mut d = controls_card_game("controls-search-tap", None);
        d.sees("Game Boy");
        d.send("up\na");
        d.sees("Recent searches");
        d.send("start\na");
        d.sees("Tobu Two");
        d.send("a");
        let card = d.sees("Controls · Game Boy");
        let chosen = if card.contains("Tobu Two") { "Tobu Two.gb" } else { "Tobu Tobu Girl (World).gb" };
        // This is another result's row under the overlay. It must dismiss the card, not select that result.
        let tapped = Instant::now();
        d.send("tap 0.5 0.35");
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        assert!(tapped.elapsed() < test_clock::timeout(Duration::from_secs(3)), "a tap must start immediately, not wait for auto-continue");
        assert!(d.log().contains(chosen), "the game named on the card must start ({chosen}): {}", d.log());
    }

    #[test]
    fn controls_card_also_precedes_a_consoles_first_game_from_just_play() {
        let mut d = Driver::start("controls-just-play", true, |tmp, cmd| {
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
        // The row's one-time explanation consumes the first A; the following A plays.
        d.send("a");
        if d.frame().contains("Just Play") && !d.log().contains("controls card: first start") {
            d.send("a");
        }
        d.sees("Controls · Game Boy");
        assert!(!d.card.tmp.join("run-game.started").exists());
        d.send("a");
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_controls_card_for_a_console_without_names_explains_home_and_accepts_a_tap() {
        let mut d = Driver::start("controls-unknown-tap", true, |tmp, cmd| {
            let games = tmp.join("megadrive");
            std::fs::create_dir_all(&games).unwrap();
            std::fs::write(games.join("Card Demo.md"), "game").unwrap();
            std::fs::write(tmp.join("es_systems.cfg"), format!("<systemList><system><name>megadrive</name><fullname>Mega Drive</fullname><path>{}</path><extension>.md</extension><command>true</command></system></systemList>", games.display())).unwrap();
            let host = tmp.join("pocket-core-stand-in");
            std::fs::write(&host, "").unwrap();
            cmd.env("POCKETUI_HOST_BIN", host).env("POCKETUI_STATES", tmp.join("states"));
        });
        d.sees("Mega Drive");
        d.send("a");
        d.sees("Card Demo");
        d.send("a");
        d.sees("This console's button names");
        assert!(!d.card.tmp.join("run-game.started").exists());
        let tapped = Instant::now();
        d.send("tap 0.5 0.5");
        d.wait(|d| d.card.tmp.join("run-game.started").exists());
        assert!(tapped.elapsed() < test_clock::timeout(Duration::from_secs(3)), "a tap must start immediately, not wait for auto-continue");
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
        d.sees("Sound & feel");
        // Two levels down: a category's list.
        d.send("a");
        d.sees("Appearance");
        d.send("home");
        let shelf = d.sees("Game Boy");
        assert!(!shelf.contains("Sound & feel") && !shelf.contains("Appearance"), "HOME leaves Settings for Home: {shelf}");
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
        d.sees("Kids Mode comes next");
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
        // On again: the look (Kids Mode was skipped), and A there finishes.
        d.send("a");
        d.sees("How your games stand");
        d.send("a");
        d.sees("Game Boy");
        // Again, all the way: the PIN twice, a game, the colour, the time, the volume, done.
        let mut d = Driver::start("kids-first-games-2", false, |_, _| {});
        d.sees("Welcome to your Pocket");
        d.send("y");
        d.sees("Kids Mode is set up after your games");
        d.first_start_to_games("Your games");
        d.sees("Kids Mode comes next");
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
        let text = ui.sees("Could not save: is the SD card full?");
        assert!(text.contains("Details"), "the error must remain visible after returning to the shelf: {text}");
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
        ui.sees("Settings › System › Time has the rest.");
        ui.send("a");
        ui.sees("Your games");
        // B back to Wi-Fi and on offline again: straight to your games.
        ui.send("b");
        ui.sees("Neighbour");
        ui.wifi_continue();
        let games = ui.sees("Your games");
        assert!(!games.contains("Time has the rest"), "{games}");
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
        assert!(still.contains("Send from your phone") && !still.contains("Time has the rest"), "{still}");
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
        ui.sees("Settings › System › Time has the rest.");
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
        assert!(look.contains("These are your own games"), "{look}");
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
        // The language, Wi-Fi left by its Continue row (and the time zone when it is asked), Your games: Continue, the look: Finish.
        ui.first_start_to_games("Your games");
        // Each console with its name, not only a picture (first-start walk 2026-11-06: Game Gear, Master System).
        let found = ui.sees("Found 2 games");
        assert!(found.contains("Game Boy"), "{found}");
        ui.send("a");
        // The style is a row of the look, with Retro among its choices (2026-11-07: shown, not behind a key).
        let look = ui.sees("How your games stand");
        assert!(look.lines().any(|l| l == "Style") && look.lines().any(|l| l == "Retro"), "{look}");
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
        for (emulator, state) in [("pocket-core", false), ("pocket-core", true), ("retroarch", false), ("retroarch", true)] {
            let mut ui = Driver::start("reminder-moment", true, |tmp, cmd| {
                let cfg = tmp.join("es_systems.cfg");
                let text = std::fs::read_to_string(&cfg).unwrap().replace("</command>", "</command><emulators><emulator name=\"retroarch\"><cores><core default=\"true\">gambatte</core></cores></emulator></emulators>");
                std::fs::write(cfg, text).unwrap();
                let game = tmp.join("gb/Tobu Tobu Girl (World).gb");
                std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"minimal","motion":"off","art":"off"}"#).unwrap();
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
            ui.wait(|d| d.card.tmp.join("run-game.started").exists());
            let hold = std::fs::read_to_string(ui.card.tmp.join("run-game.hold")).unwrap();
            assert_eq!(!hold.trim().is_empty(), state && emulator == "pocket-core", "{emulator}, state {state}: {hold:?}");
            if state {
                ui.sees("Last played 3 weeks ago");
                ui.send("volup");
                ui.wait(|d| !d.frame().contains("Last played"));
                assert!(ui.card.tmp.join("pocket-launch-skip").exists(), "even a volume key dismisses the reminder and releases the game");
            } else {
                assert!(!ui.frame().contains("Last played"), "{emulator}: no reminder on a fresh start");
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
        ui.sees("START");
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
        ui.sees("START");
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
        ui.sees("START");
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
        controller_sees(&mut ui, "Sound & feel");
        ui.send("down\ndown\na"); // Playing
        controller_sees(&mut ui, "Continue where I quit");
        ui.send("down\ndown\ndown\na"); // Buttons
        controller_sees(&mut ui, "Button symbols");
        ui.send("down\ndown\na"); // Controller setup
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
        ui.send("b");
        controller_sees(&mut ui, "Button symbols");
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
        ui.send("b");
        controller_sees(&mut ui, "Button symbols");
    }

    /// Settings → Help & tools → Test buttons: every button is a test, B, START and HOME too (they must not leave the
    /// page, open quick settings or the Settings list), two controllers keep their own lights, a trigger shows its travel,
    /// and the page is left by holding HOME for two seconds, by B and START together, or by a tap on its hint.
    /// Controllers and their presses come through the driver's own verbs (`tconnect`, `tbtn`, `taxis`), with sources of
    /// their own as the pad threads have them.
    #[test]
    fn the_button_tester_takes_every_button_and_is_left_by_a_hold() {
        let mut ui = Driver::new("test-buttons", true);
        ui.send("start");
        ui.sees("Sound & feel");
        // Help & tools, the last category; its sixth row is Test buttons.
        ui.send("down\ndown\ndown\ndown\ndown\ndown\na");
        ui.sees("Test buttons");
        ui.send("down\ndown\ndown\ndown\ndown\na");
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
        let text = ui.sees("Continue where I quit");
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
        // Look, then its third row Cozy extras (after Layout and Systems on the shelf): the page; Stickers on cases, its
        // second row.
        ui.send("b");
        ui.sees("Help & tools");
        tap(&mut ui, 0);
        tap(&mut ui, 0);
        ui.sees("Systems on the shelf");
        tap(&mut ui, 2);
        tap(&mut ui, 2);
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
        assert!(!ui.log().contains("fallback"), "{}", ui.log());
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

    /// With Curator's Lens switch off Home has no Just Play row (Settings → Curator is where Curator's parts go off).
    #[test]
    fn just_play_goes_with_the_curator_switch() {
        let mut ui = Driver::start("just-play-off", true, |tmp, _| {
            std::fs::write(tmp.join("data/curator.json"), r#"{"lens":false}"#).unwrap();
        });
        ui.sees("Game Boy");
        ui.send("up");
        // Search is the row above the systems, nothing between.
        let home = ui.sees("Find any game by its name");
        assert!(!home.contains("Just Play"), "{home}");
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
        ui.sees("Sound & feel");
        ui.send("down\ndown\ndown\na");
        ui.sees("Add games");
        ui.send("down\na");
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
        ui.sees("Extras");
        std::fs::write(ui.card.tmp.join("gb/Pokemon Blau.gb"), b"game").unwrap();
        ui.send("up\na");
        ui.sees("Game Boy");
        ui.send("b");
        let offer = ui.sees("1 new game");
        assert!(offer.contains("Check"), "{offer}");
        ui.send("b");
        ui.sees("Extras");
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
        ui.sees("Sound & feel");
        ui.send("down\ndown\ndown\na");
        ui.sees("Add games");
        ui.send("down\na");
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
        assert!(home.contains("Connect to Wi-Fi") && !home.contains("Get the starter pack") && home.contains("Settings › Games › Add games shows the ways"), "{home}");
        // The pack's own page, from Settings → Games → Extras → Starter pack, still says when the Pocket is offline.

        // Hold the real starter-scan at its config read. The headless driver has completed startup and skips the
        // device startup jobs, so opening this FIFO belongs to that scan. This forces the slow-worker interleaving
        // which made waiting for the page's header lose the second A press on busy build machines.
        let config = ui.card.tmp.join("es_systems.cfg");
        let xml = std::fs::read(&config).unwrap();
        std::fs::remove_file(&config).unwrap();
        let fifo = std::ffi::CString::new(config.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        ui.send("start\ndown\ndown\ndown\na\ndown\ndown\na\ndown\ndown\ndown\na");
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
        ui.sees("The Pocket isn't online");
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
        for look in ["shelf", "minimal", "split", "cards", "cabinet"] {
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
        let mut ui = empty_card("empty-games", "minimal");
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

    /// Delete from the SD card: the card's last row, the sheet, two A presses; the file is gone and the screen says so.
    #[test]
    fn deleting_a_game_asks_twice_then_removes_the_file() {
        let mut ui = Driver::new("delete", true);
        ui.sees("Game Boy");
        let rom = ui.card.tmp.join("gb/Tobu Tobu Girl (World).gb");
        // The game's card (X), turned over (right), its second page (SELECT), down to the last row: Hide or delete.
        ui.send("a\nx\nright\nselect");
        // The rows run on past the window (Controls came before it): Down scrolls to the last one.
        ui.sees("Controls");
        ui.send(&"down\n".repeat(10));
        ui.sees("Hide or delete");
        ui.send("a");
        ui.sees("Hide from the shelf");
        ui.send("down\na");
        ui.sees("Delete Tobu Tobu Girl?");
        // The heading appears before the asynchronous file scan enables Delete: wait for its visible result.
        let sheet = ui.sees("Frees 1 KB: 1 files.");
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
        // The card's own `gb` folder would be read as a second folder of the system (docs/OTHER-LAYOUTS.md); these journeys
        // are about exactly these two games.
        std::fs::remove_dir_all(tmp.join("gb")).unwrap();
        let known = games.join("Supar Mario Land 2.gb");
        std::fs::write(&known, b"recognized A").unwrap();
        std::fs::write(games.join("Unknown Homebrew.gb"), b"unknown B").unwrap();
        std::fs::write(tmp.join("es_systems.cfg"), format!(
            "<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension><command>true</command></system></systemList>", games.display()
        )).unwrap();
        std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"minimal","motion":"off","art":"off"}"#).unwrap();
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
        ui.send("start\ndown\ndown\ndown\na\ndown\ndown\na");
        let extras = ui.sees("Download");
        assert!(extras.contains("Extras") && extras.contains("Box art"), "online Extras is ready: {extras}");
        ui.send("b\nb\nb");
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
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","theme":"minimal","motion":"off","art":"off"}"#).unwrap();
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
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"minimal"}"#).unwrap();
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
        ui.sees("Controls");
        ui.send(&"down\n".repeat(10));
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
        let mut ui = quiet_shelf("slideshow-back", true, 4000, r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","extras":{"idle_slideshow":true}}"#);
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
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"minimal"}"#).unwrap();
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
        d.wait(|d| status_of(d).contains("\tplace=settings:Groups\t"));
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
        d.wait(|d| status_of(d).contains("\tplace=settings:Groups\t"));
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
            std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","theme":"minimal"}"#).unwrap();
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
        assert_eq!(ui_status(&d, &["expect", "settings:Groups", "10"]).0, 0);
        let (rc, _, err) = ui_status(&d, &["send", "--at", "settings:Groups", "b", "a"]);
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
        assert_eq!(ui_status(&d, &["expect", "settings:Groups", "10"]).0, 0);
        d.send("power");
        d.wait(|d| d.log().contains("sleep: power key, screen off"));
        d.wait(|d| std::fs::read_to_string(d.card.tmp.join("ui.status")).is_ok_and(|l| l.contains("\tsleep=light\t")));
        let (rc, _, err) = ui_status(&d, &["send", "b"]);
        assert_eq!(rc, 4, "a key on a dark screen is refused: {err}");
        let (rc, _, err) = ui_status(&d, &["send", "wake"]);
        assert_eq!(rc, 0, "{err}");
        let (rc, out, err) = ui_status(&d, &["expect", "settings:Groups", "5"]);
        assert_eq!((rc, out.trim()), (0, "settings:Groups"), "wake pressed nothing: still on the page it woke on ({err})");
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
        let mut ui = quiet_shelf("slideshow-none", false, 800, r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","extras":{"idle_slideshow":true}}"#);
        ui.wait(|d| d.log().contains("slideshow: no pictures"));
        let text = ui.frame();
        assert!(text.contains("Settings") && text.contains("Game Boy"), "the shelf is still there: {text}");
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_the_idle_slideshow_is_off_in_every_look_until_the_page_switches_it_on() {
        // As it starts (the Shelf, then a plain look with pictures all the same): the shelf stays a shelf, however long it
        // is left alone.
        for (name, settings) in [
            ("slideshow-default-shelf", r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off"}"#),
            ("slideshow-default-split", r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","theme":"split"}"#),
        ] {
            let mut ui = quiet_shelf(name, true, 600, settings);
            std::thread::sleep(Duration::from_millis(2500));
            let text = ui.frame();
            assert!(text.contains("Settings"), "{name}: still the shelf: {text}");
            assert!(!ui.log().contains("slideshow:"), "{name}: {}", ui.log());
        }
        // Switched on in the page (kept as extras.idle_slideshow): a plain look plays it too.
        let mut ui = quiet_shelf("slideshow-split-on", true, 600, r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","theme":"split","extras":{"idle_slideshow":true}}"#);
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
