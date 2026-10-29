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
                ""
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
while :; do sleep 0.05; done
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

    #[test]
    fn quick_wifi_off_and_on_agrees_with_the_wifi_page() {
        let mut ui = Driver::new("wifi", true);
        ui.send("home\ndown\ndown\na");
        ui.sees("Wi-Fi\nOff");
        ui.send("b\nstart\ndown\ndown\ndown\na\na");
        let off = ui.sees("Wi-Fi is off");
        assert!(
            !off.contains("Connected") && !off.contains("Connect\n"),
            "off must not claim a connection: {off}"
        );
        // Back from Wi-Fi and Settings, switch the same radio on again, then reopen the Wi-Fi page.
        ui.send("b\nb\nb\nhome\ndown\ndown\na");
        ui.sees("Wi-Fi\nOn");
        ui.send("b\nstart\ndown\ndown\ndown\na\na");
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

    /// The Game Boy and Sega CD card plus one more system, its files empty.
    fn add_system(tmp: &Path, id: &str, name: &str, exts: &str, files: &[&str]) {
        let folder = tmp.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        for f in files {
            std::fs::write(folder.join(f), "").unwrap();
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

    /// A card with no games is not an empty screen: it says so, offers the starter pack, and says what is missing when
    /// the Pocket is offline.
    #[test]
    fn an_empty_card_offers_the_starter_pack_and_says_when_it_is_offline() {
        let mut ui = Driver::start("starter", true, |tmp, _| {
            for f in ["gb/Tobu Tobu Girl (World).gb", "segacd/Pocket CD Demo (USA).cue", "segacd/Pocket CD Demo (USA).bin"] {
                std::fs::remove_file(tmp.join(f)).unwrap();
            }
        });
        let home = ui.sees("No games yet");
        assert!(home.contains("Get the starter pack") && home.contains("/storage/roms/<system>"), "{home}");
        ui.send("a");
        ui.sees("Starter pack");
        ui.send("a");
        ui.sees("The Pocket isn't online");
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

    /// The Minimal look: X on a game opens its info page (everything on one screen, `game_info`), B goes back to the
    /// shelf; SELECT shows the card's full page and back.
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

    /// Bluetooth page with a speaker/headphone-class device (the mock's JBL Flip 5): search, pair, trust, connect,
    /// and the page says it is connected.
    #[test]
    fn bluetooth_page_connects_an_audio_device() {
        let mut ui = Driver::new("bluetooth", true);
        ui.sees("Game Boy");
        ui.send("start\ndown\ndown\ndown\na\ndown\na");
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
