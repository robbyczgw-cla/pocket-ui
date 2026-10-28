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
        // Navigation clamps at the ends; repeated Down reaches the visible Quit row.
        ui.send(&("down\n".repeat(12) + "a"));
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
