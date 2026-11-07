//! The D-pad explorer (docs/DEVELOPING.md "The navigation explorer"): the real pocket-ui binary, driven over `--test-driver`
//! with the buttons a player has, through every page it can reach from Settings, the quick settings, the power menu and the
//! first start. After every press it reads what the frame offered (`pocket_ui::nav_probe`: the strings with their places,
//! where an operable row is drawn, where the focus mark is, which hints the bar shows, and where the App is), never pixels at
//! guessed places. A page fails when
//!
//! - a row it draws is never focused by Up/Down (or Left/Right where those move the focus),
//! - the list says a row is selected and no focus mark is on it, or the selection moves and the picture stays the same,
//! - B (or the key its hint names for going back) does not lead back to the page it came from, on the row it came from,
//! - the hint bar names a key that does nothing there, or A or B work there and the bar doesn't name them.
//!
//! Escaped bug (Brick, 07.11.2026): Settings › Look › Layout drew Appearance, Motion and Animations as rows Up/Down never
//! reached, and its hint bar left out B, which left the page; thousands of snapshot tests passed because they read a
//! picture, and the walkthroughs stopped a page where Down stopped changing it.
//!
//! Each profile (device, size, text size) is its own test, so they run side by side. Network programs and anything that could
//! switch the computer off are stand-ins on PATH; every process starts on a fresh copy of the same made-up card.
//!
//! Needs python3 for scripts/wayland-stub.py on Linux without a compositor (as tests/user_journeys.rs).
#![cfg(target_os = "linux")]

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

include!("common/root.rs");

// ---- the record of a frame -------------------------------------------------------------------------------------

type Rect = (i32, i32, i32, i32);

const FOCUS: &str = "‹nav:focus›";
const ROW: &str = "‹nav:row›";
const HINT: &str = "‹nav:hint›";

/// One operable row of a frame: where it is, its name (the first string in it) and whether the focus mark is on it.
#[derive(Clone, Debug)]
struct Control {
    rect: Rect,
    name: String,
    focused: bool,
}

/// What one frame offered (`nav_probe`, `app_nav_probe`).
#[derive(Clone, Debug, Default)]
struct Frame {
    at: String,
    strings: Vec<(Rect, String)>,
    rows: Vec<Rect>,
    focus: Vec<Rect>,
    /// The hints the bar shows: key and word.
    hints: Vec<(String, String)>,
    list: Option<(usize, usize)>,
    hits: Vec<(Rect, usize)>,
    hash: String,
}

fn centre(r: &Rect) -> (i32, i32) {
    ((r.0 + r.2) / 2, (r.1 + r.3) / 2)
}

fn inside(p: (i32, i32), r: &Rect) -> bool {
    p.0 >= r.0 && p.0 < r.2 && p.1 >= r.1 && p.1 < r.3
}

fn overlap(a: &Rect, b: &Rect) -> i64 {
    let (w, h) = (a.2.min(b.2) - a.0.max(b.0), a.3.min(b.3) - a.1.max(b.1));
    if w <= 0 || h <= 0 {
        0
    } else {
        i64::from(w) * i64::from(h)
    }
}

fn area(r: &Rect) -> i64 {
    i64::from((r.2 - r.0).max(0)) * i64::from((r.3 - r.1).max(0))
}

/// The status corner's clock ("09:41"), which changes by itself.
fn clock(s: &str) -> bool {
    let b = s.trim().as_bytes();
    (b.len() == 5 && b[2] == b':' || b.len() == 4 && b[1] == b':') && b.iter().filter(|c| c.is_ascii_digit()).count() == b.len() - 1
}

impl Frame {
    fn parse(record: &str) -> Frame {
        let mut f = Frame::default();
        for line in record.lines() {
            let mut parts = line.splitn(2, '\t');
            let head = parts.next().unwrap_or_default();
            let rest = parts.next().unwrap_or_default();
            if let Some(key) = head.strip_prefix('@') {
                let v: Vec<&str> = rest.split('\t').collect();
                let n = |i: usize| v.get(i).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
                match key {
                    "where" => f.at = rest.to_string(),
                    "list" => f.list = Some((n(0) as usize, n(1) as usize)),
                    "hit" => f.hits.push(((n(0), n(1), n(2), n(3)), n(4) as usize)),
                    "hash" => f.hash = rest.to_string(),
                    _ => {}
                }
                continue;
            }
            let v: Vec<&str> = line.splitn(5, '\t').collect();
            if v.len() < 5 {
                continue;
            }
            let p = |i: usize| v[i].parse::<i32>().unwrap_or(0);
            let r = (p(0), p(1), p(2), p(3));
            let s = v[4];
            if s == FOCUS {
                f.focus.push(r);
            } else if s == ROW {
                f.rows.push(r);
            } else if let Some(h) = s.strip_prefix(HINT) {
                let (k, l) = h.split_once('\t').unwrap_or((h, ""));
                f.hints.push((k.to_string(), l.to_string()));
            } else if !s.starts_with('‹') && s != ">" && s != "<" && !s.trim().is_empty() {
                f.strings.push((r, s.to_string()));
            }
        }
        f
    }

    /// What the player reads, without the clock and with "Looking…"'s moving dots at rest.
    fn text(&self) -> String {
        self.strings.iter().filter(|(_, s)| !clock(s)).map(|(_, s)| s.trim_end_matches(['.', '…'])).collect::<Vec<_>>().join("\n")
    }

    /// Dots that move by themselves ("Looking at your games.." then "..."): the picture changes while nothing happens.
    fn moving_dots(&self) -> bool {
        self.strings.iter().any(|(_, s)| s.ends_with("..") || s.ends_with('.') && s.trim_end_matches('.').ends_with(|c: char| c.is_alphabetic()) && s.contains("Looking"))
    }

    fn clock_text(&self) -> String {
        self.strings.iter().filter(|(_, s)| clock(s)).map(|(_, s)| s.as_str()).collect()
    }

    /// The operable rows: the painters' marks and the rows the list registered for touch, each once, named by the first string
    /// in it; a focus mark with a name but no row is a row too (a page that marks its selection only).
    fn controls(&self) -> Vec<Control> {
        let mut rects: Vec<Rect> = Vec::new();
        for r in self.rows.iter().chain(self.hits.iter().map(|(r, _)| r)) {
            if !rects.iter().any(|o| overlap(o, r) * 2 > area(o).min(area(r))) {
                rects.push(*r);
            }
        }
        for r in &self.focus {
            if !rects.iter().any(|o| overlap(o, r) * 2 > area(o).min(area(r))) {
                rects.push(*r);
            }
        }
        let mut out = Vec::new();
        for r in rects {
            // The name: the leftmost string whose middle is in the row, the top one of a row of two lines.
            let mut inner: Vec<&(Rect, String)> = self.strings.iter().filter(|(s, _)| inside(centre(s), &r)).collect();
            inner.sort_by_key(|(s, _)| (s.0 / 8, s.1));
            let Some((name_at, name)) = inner.first() else { continue };
            // The mark across the row, or a pill round its name (the older lists: Sync, Home's Minimal).
            let focused = self.focus.iter().any(|f| inside(centre(&r), f) || overlap(f, &r) * 2 > area(&r) || (overlap(f, &r) > 0 && inside(centre(name_at), f)));
            out.push(Control { rect: r, name: name.clone(), focused });
        }
        out
    }

    fn focused(&self) -> Option<String> {
        self.controls().into_iter().find(|c| c.focused).map(|c| c.name)
    }

    /// The focused row is `other`'s focused row: the same name, or the same place (a tile whose words changed meanwhile:
    /// "Connect to Wi-Fi first" became "File sharing").
    fn same_row(&self, other: &Frame) -> bool {
        let at = |f: &Frame| f.controls().into_iter().find(|c| c.focused);
        match (at(self), at(other)) {
            (Some(a), Some(b)) => a.name == b.name || a.rect == b.rect,
            (a, b) => a.is_none() && b.is_none(),
        }
    }

    /// The same as `other` to a player: the same page, words and focus (the picture is compared too while the clock stands).
    fn same(&self, other: &Frame) -> bool {
        self.at == other.at && self.text() == other.text() && self.focused() == other.focused() && self.hints == other.hints
            && (self.hash == other.hash || self.clock_text() != other.clock_text() || self.moving_dots() || other.moving_dots())
    }

    fn hint_keys(&self) -> Vec<String> {
        self.hints.iter().map(|(k, _)| k.clone()).collect()
    }

    fn names_hint(&self, key: &str) -> bool {
        self.hints.iter().any(|(k, _)| keys_of(k).contains(&key))
    }
}

/// The buttons a hint's key stands for: one of them must do something.
fn keys_of(hint: &str) -> Vec<&'static str> {
    match hint {
        "A" => vec!["a"],
        "B" => vec!["b"],
        "X" => vec!["x"],
        "Y" => vec!["y"],
        "L1" => vec!["l1"],
        "R1" => vec!["r1"],
        "L2" => vec!["l2"],
        "R2" => vec!["r2"],
        "SELECT" => vec!["select"],
        "START" => vec!["start"],
        "UP" => vec!["up"],
        "DOWN" => vec!["down"],
        "RIGHT" => vec!["right"],
        "LEFT" => vec!["left"],
        "Left/Right" | "< >" | "\u{2039} \u{203a}" | "LEFT/RIGHT" => vec!["left", "right"],
        "Up/Down" | "UP/DOWN" => vec!["up", "down"],
        "L1 R1" | "L1/R1" => vec!["l1", "r1"],
        "L2 R2" | "L2/R2" => vec!["l2", "r2"],
        _ => vec![],
    }
}

// ---- the driver ------------------------------------------------------------------------------------------------

/// Where and how pocket-ui runs: a device of `POCKETUI_DEVICE`, its screen, the text size.
#[derive(Clone, Copy, Debug)]
struct Profile {
    name: &'static str,
    device: &'static str,
    text_size: &'static str,
    extra_env: &'static [(&'static str, &'static str)],
}

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

/// The made-up card every process starts on: a few systems and games, the settings of a player past the first start.
struct Card {
    dir: tempfile::TempDir,
    stub: Option<Stub>,
}

impl Card {
    fn new(profile: &Profile) -> Card {
        let dir = tempfile::Builder::new().prefix(&format!("pocketui-nav-{}-", profile.name)).tempdir().unwrap();
        let tmp = dir.path();
        let mut cfg = String::from("<?xml version=\"1.0\"?>\n<systemList>\n");
        for (id, name, exts, files) in [
            ("gb", "Game Boy", ".gb .zip", &["Tobu Tobu Girl (World).gb", "Deadeus (World).gb"][..]),
            ("gba", "Game Boy Advance", ".gba .zip", &["Goodboy Galaxy (World).gba"][..]),
            ("snes", "Super Nintendo", ".sfc .smc .zip", &["Super Boss Gaiden (World).sfc"][..]),
        ] {
            let folder = tmp.join("roms").join(id);
            std::fs::create_dir_all(&folder).unwrap();
            for f in files {
                std::fs::write(folder.join(f), "x").unwrap();
            }
            cfg += &format!(
                "  <system><name>{id}</name><fullname>{name}</fullname><path>{}</path><extension>{exts}</extension><command>true</command></system>\n",
                folder.display()
            );
        }
        cfg += "</systemList>\n";
        std::fs::write(tmp.join("es_systems.cfg"), cfg).unwrap();
        // Programs that would reach the network or the computer's power: stand-ins that say no (network) or only write down
        // that they were asked.
        let bin = tmp.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        for (name, body) in [
            ("curl", "exit 6"),
            ("wget", "exit 4"),
            ("systemctl", "echo \"$0 $*\" >> \"$(dirname \"$0\")/asked.log\"; exit 0"),
            ("reboot", "echo \"$0 $*\" >> \"$(dirname \"$0\")/asked.log\"; exit 0"),
            ("poweroff", "echo \"$0 $*\" >> \"$(dirname \"$0\")/asked.log\"; exit 0"),
            ("shutdown", "echo \"$0 $*\" >> \"$(dirname \"$0\")/asked.log\"; exit 0"),
            ("halt", "echo \"$0 $*\" >> \"$(dirname \"$0\")/asked.log\"; exit 0"),
            ("loginctl", "echo \"$0 $*\" >> \"$(dirname \"$0\")/asked.log\"; exit 0"),
            ("rtcwake", "echo \"$0 $*\" >> \"$(dirname \"$0\")/asked.log\"; exit 0"),
        ] {
            executable(&bin.join(name), &format!("#!/bin/sh\n{body}\n"));
        }
        // The stand-in for a game (`launch` under --test-driver): bounded, it never outlives its test.
        executable(&tmp.join("template-run-game"), "#!/bin/sh\nprintf 'started\\n' > \"$0.started\"\ntrap 'exit 0' TERM INT\ni=0\nwhile [ \"$i\" -lt 2400 ]; do sleep 0.05; i=$((i+1)); done\n");
        let data = tmp.join("template-data");
        std::fs::create_dir_all(&data).unwrap();
        // The language as a player picked it (`POCKETUI_LANG` would pin it, and Settings › System › Language could not change it).
        std::fs::write(data.join("settings.json"), r#"{"onboarded":true,"seen_version":"9.9.9","motion":"off","art":"off","language":"en"}"#).unwrap();
        let stub = if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
            None
        } else {
            let path = tmp.join("wl");
            let child = Command::new("python3")
                .args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()])
                .spawn()
                .expect("python3 for scripts/wayland-stub.py");
            let stub = Stub { path, child };
            test_clock::wait_for(|| stub.path.exists(), Duration::from_secs(2), "Wayland stub listening");
            Some(stub)
        };
        Card { dir, stub }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// Writes an executable in a child, as tests/user_journeys.rs does: another test's fork must not inherit a descriptor that can
/// write it (ETXTBSY).
fn executable(path: &Path, body: &str) {
    let mut writer = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(std::process::Stdio::piped()).spawn().unwrap();
    let mut input = writer.stdin.take().unwrap();
    input.write_all(body.as_bytes()).unwrap();
    drop(input);
    assert!(writer.wait().unwrap().success());
}

/// Also lets a regression test be replayed against the saved pre-fix executable.
fn ui_binary() -> PathBuf {
    std::env::var_os("POCKETUI_NAV_BINARY").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from)
}

/// One running pocket-ui on a fresh copy of the card.
struct Ui {
    child: std::process::Child,
    run: PathBuf,
    /// Frames asked for, for the report.
    frames: usize,
}

/// pocket-ui ended (a row that restarts it, a crash): the explorer starts it again. What it said last, for a failure.
#[derive(Debug)]
struct Exited(#[allow(dead_code)] String);

impl Ui {
    fn start(card: &Card, profile: &Profile, onboarded: bool, run: usize, env: &[(&str, &str)]) -> Ui {
        let run = card.path().join(format!("run-{run}"));
        let data = run.join("data");
        std::fs::create_dir_all(&data).unwrap();
        if onboarded {
            std::fs::copy(card.path().join("template-data/settings.json"), data.join("settings.json")).unwrap();
        }
        std::fs::copy(card.path().join("template-run-game"), run.join("run-game")).unwrap();
        std::fs::write(run.join("wakealarm"), "+3600").unwrap();
        let log = std::fs::File::create(run.join("ui.log")).unwrap();
        let path = format!("{}:{}", card.path().join("bin").display(), std::env::var("PATH").unwrap_or_default());
        let mut cmd = Command::new(ui_binary());
        cmd.args(["--test-driver", run.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env_remove("POCKETUI_LANG")
            .env("LANG", "en_US.UTF-8")
            .env("POCKETUI_DEVICE", profile.device)
            .env("POCKETUI_TEXT_SIZE", profile.text_size)
            .env("POCKETUI_NAV_PROBE", run.join("nav.txt"))
            .env("POCKETUI_DATA", &data)
            .env("POCKETUI_ES_SYSTEMS", card.path().join("es_systems.cfg"))
            .env("POCKETUI_RTC_ALARM", run.join("wakealarm"))
            .env("POCKETUI_SUSPEND_CMD", "false")
            .env("POCKETUI_POWEROFF_CMD", "true")
            .env("POCKETUI_NO_AUTOSUSPEND", "1")
            .env("POCKETUI_NO_HISTORY", "1")
            .env("PATH", path)
            .env_remove("SWAYSOCK")
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        for (k, v) in profile.extra_env.iter().chain(env) {
            cmd.env(k, v);
        }
        if let Some(stub) = &card.stub {
            cmd.env("WAYLAND_DISPLAY", &stub.path);
        }
        // A test that dies takes the UI with it, and the UI its stand-in game.
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) });
        }
        let child = cmd.spawn().unwrap();
        let mut ui = Ui { child, run, frames: 0 };
        let fifo = ui.run.join("ui.in");
        ui.wait(|| fifo.exists(), "the command FIFO").unwrap();
        ui
    }

    fn log(&self) -> String {
        let log = std::fs::read_to_string(self.run.join("ui.log")).unwrap_or_default();
        log.lines().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
    }

    fn wait(&mut self, mut ready: impl FnMut() -> bool, what: &str) -> Result<(), Exited> {
        let until = Instant::now() + test_clock::timeout(Duration::from_secs(600));
        while Instant::now() < until {
            if ready() {
                return Ok(());
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                return Err(Exited(format!("pocket-ui exited ({status}) waiting for {what}:\n{}", self.log())));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("no {what} within the time; UI log:\n{}", self.log());
    }

    fn send(&mut self, line: &str) -> Result<(), Exited> {
        let path = std::ffi::CString::new(self.run.join("ui.in").to_str().unwrap()).unwrap();
        let mut fd = -1;
        self.wait(
            || {
                // Nonblocking: a UI that ended never leaves the test waiting for a reader.
                fd = unsafe { libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
                fd >= 0
            },
            "the FIFO's reader",
        )?;
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        writeln!(file, "{line}").map_err(|e| Exited(format!("FIFO: {e}")))
    }

    /// The buttons, in order, then the frame after them.
    fn press(&mut self, keys: &[&str]) -> Result<Frame, Exited> {
        let nav = self.run.join("nav.txt");
        let _ = std::fs::remove_file(&nav);
        let mut line = keys.join("\n");
        if !line.is_empty() {
            line.push('\n');
        }
        line += "nav";
        self.send(&line)?;
        let mut record = String::new();
        self.wait(
            || {
                record = std::fs::read_to_string(&nav).unwrap_or_default();
                record.ends_with("@end\n")
            },
            "the frame's record",
        )?;
        self.frames += 1;
        Ok(Frame::parse(&record))
    }

    /// The frame once it stands still: what a page reads on a thread has arrived (two frames alike, at most about a second).
    fn settled(&mut self, keys: &[&str]) -> Result<Frame, Exited> {
        self.watched(keys).map(|(f, _)| f)
    }

    /// `settled`, and every frame on the way: a key that only says "Looking…" for a moment did something.
    fn watched(&mut self, keys: &[&str]) -> Result<(Frame, Vec<Frame>), Exited> {
        let mut f = self.press(keys)?;
        let mut seen = vec![f.clone()];
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(3));
            let g = self.press(&[])?;
            seen.push(g.clone());
            if g.same(&f) {
                return Ok((g, seen));
            }
            f = g;
        }
        Ok((f, seen))
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        let _ = self.send("quit");
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until && matches!(self.child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        // A stand-in game left running: its pid is in the log.
        for line in std::fs::read_to_string(self.run.join("ui.log")).unwrap_or_default().lines() {
            if let Some(pid) = line.strip_prefix("test game pid ").and_then(|s| s.parse::<i32>().ok()) {
                let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
                let marker = self.run.to_str().unwrap().as_bytes();
                if cmd.windows(marker.len()).any(|part| part == marker) {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                }
            }
        }
    }
}

// ---- the explorer ----------------------------------------------------------------------------------------------

/// Rows of A that are not pressed: they end what is being explored for reasons that are no page's (the program restarts or
/// quits, a game starts, the card is written over). Their pages are still opened when they are pages.
const NOT_PRESSED: &[&str] = &["Restart pocket-ui", "Power off", "Switch off", "Restart", "Quit", "Save & quit", "Leave", "Start EmulationStation", "Reset", "Erase", "Delete", "Format"];

/// Unhinted keys tried once per page: a key that works there is either named or doing something no player finds.
const OTHER_KEYS: &[&str] = &["x", "y", "l1", "r1"];
/// The same, for keys that do the same on every page of a family (START, SELECT): once per entry.
const ENTRY_KEYS: &[&str] = &["select", "start"];

/// How many pages deep A goes from an entry (Settings › a category › a page › its page › ...); the first start's steps
/// follow each other, one deeper each.
const MAX_DEPTH: usize = 4;
const FIRST_START_DEPTH: usize = 14;
/// Focus states a grid's walk visits at most.
const MAX_GRID: usize = 40;
/// What a B hint says when B goes back (a B with another word goes on: the first start's "Continue offline").
const BACK_WORDS: &[&str] = &["Back", "Close", "Cancel", "Stop", "Done", "Leave", "Stay"];
/// Presses of Down (and then Up) a page's walk takes at most.
const MAX_WALK: usize = 64;
/// Of a walk cut at `MAX_WALK`, the rows first drawn this many presses before the cut are not counted as missed: the list
/// showed them, the walk stopped before it got there.
const LAST_SCREENS: usize = 16;

/// Pages that take every button themselves: Test buttons lights up what is pressed and leaves on HOME held (its own journeys
/// in tests/user_journeys.rs); the explorer only checks they can be left.
const OWN_KEYS: &[&str] = &["settings:TestButtons"];

#[derive(Default)]
struct Report {
    failures: BTreeSet<String>,
    /// Keys that do something on a page whose bar doesn't name them (not A or B): for the report, not a failure.
    unnamed: BTreeSet<String>,
    pages: BTreeMap<String, usize>,
    notes: BTreeSet<String>,
    /// Replays that had to go one button at a time.
    slow_replays: usize,
}

struct Explorer<'a> {
    card: &'a Card,
    profile: Profile,
    onboarded: bool,
    env: Vec<(&'static str, &'static str)>,
    ui: Option<Ui>,
    runs: usize,
    report: Report,
    started: Instant,
    budget: Duration,
    max_depth: usize,
    /// Time spent starting pocket-ui and replaying the buttons to a page, for the report.
    replaying: Duration,
    /// The page the explorer was on when its time ran out: the test fails, as a timeout, never as a pass.
    timed_out: Option<String>,
}

/// How a walk ended.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Walked {
    Round,
    Stopped,
    Left,
    Cut,
    Ended,
}

/// The shelf or a game: where a page that is done goes (Finish, Continue), not a page of the menus.
fn leaves(at: &str) -> bool {
    at.starts_with("shelf:") || at == "game" || at.starts_with("game / ")
}

/// The page without what covers it ("settings:Sync / notice" is "settings:Sync").
fn page_of(at: &str) -> &str {
    at.split(" / ").next().unwrap_or(at)
}

/// A frame's page and focused row, as a player would point at them.
fn spot(f: &Frame) -> String {
    match f.focused() {
        Some(n) => format!("{} › “{n}”", f.at),
        None => f.at.clone(),
    }
}

impl<'a> Explorer<'a> {
    fn new(card: &'a Card, profile: Profile, onboarded: bool, env: Vec<(&'static str, &'static str)>) -> Self {
        let max_depth = if onboarded { MAX_DEPTH } else { FIRST_START_DEPTH };
        Explorer { card, profile, onboarded, env, ui: None, runs: 0, report: Report::default(), started: Instant::now(), budget: test_clock::timeout(Duration::from_secs(900)), max_depth, replaying: Duration::ZERO, timed_out: None }
    }

    fn fail(&mut self, msg: String) {
        self.report.failures.insert(format!("[{}] {msg}", self.profile.name));
    }

    /// A fresh pocket-ui, then `path`, a frame after each button. None: it ended on the way.
    fn replay(&mut self, path: &[String]) -> Option<Frame> {
        self.replay_to(path, None)
    }

    /// A fresh pocket-ui, then `path`: all buttons in one go and one settled frame, which is how the UI takes them; when
    /// that lands elsewhere than `want` (a page that reads its rows on a thread was not ready yet), again one button and one
    /// settled frame at a time, as the walk went.
    fn replay_to(&mut self, path: &[String], want: Option<&Frame>) -> Option<Frame> {
        let t = Instant::now();
        // Directions in runs, a settled frame after each other key (it may open a page that reads its rows on a thread).
        let quick = self.start_ui().and_then(|ui| {
            let mut f = ui.settled(&[]).ok()?;
            let mut run: Vec<&str> = Vec::new();
            for k in path {
                run.push(k.as_str());
                if !["up", "down", "left", "right"].contains(&k.as_str()) {
                    f = ui.settled(&run).ok()?;
                    run.clear();
                }
            }
            if !run.is_empty() {
                f = ui.settled(&run).ok()?;
            }
            Some(f)
        });
        let landed = |f: &Frame| want.is_none_or(|w| f.at == w.at && f.focused() == w.focused());
        let f = match quick {
            Some(f) if landed(&f) => Some(f),
            _ if path.is_empty() => quick,
            _ => {
                self.report.slow_replays += 1;
                self.start_ui().and_then(|ui| {
                    let mut f = ui.settled(&[]).ok();
                    for k in path {
                        f = f.and_then(|_| ui.settled(&[k.as_str()]).ok());
                    }
                    f
                })
            }
        };
        self.replaying += t.elapsed();
        f
    }

    /// A new pocket-ui on a fresh copy of the card, in place of the one running.
    fn start_ui(&mut self) -> Option<&mut Ui> {
        self.ui = None;
        self.runs += 1;
        self.ui = Some(Ui::start(self.card, &self.profile, self.onboarded, self.runs, &self.env));
        self.ui.as_mut()
    }

    fn press(&mut self, keys: &[&str]) -> Result<Frame, Exited> {
        let ui = self.ui.as_mut().expect("a running UI");
        ui.settled(keys)
    }

    /// `keys` did something: a frame on the way, or the one it settled on, is not `before`.
    fn changes(&mut self, keys: &[&str], before: &Frame) -> Result<(Frame, bool), Exited> {
        let ui = self.ui.as_mut().expect("a running UI");
        let (f, seen) = ui.watched(keys)?;
        let changed = seen.iter().any(|g| !g.same(before));
        Ok((f, changed))
    }

    /// Back where `want` was (its page and focused row, and its words): B while the page differs, the same key again while
    /// the words do (a switch going round), the opposite direction, and else a fresh start along `path`.
    fn restore(&mut self, want: &Frame, path: &[String], key: &str) -> Option<Frame> {
        let mut f = self.press(&[]).ok()?;
        if f.same(want) {
            return Some(f);
        }
        if f.at != want.at {
            for _ in 0..3 {
                match self.press(&["b"]) {
                    Ok(g) if g.same(want) => return Some(g),
                    Ok(g) if g.at == want.at => {
                        f = g;
                        break;
                    }
                    Ok(g) => f = g,
                    Err(_) => break,
                }
            }
        }
        if f.at == want.at && f.focused() == want.focused() {
            let again: Vec<&str> = match key {
                "left" => vec!["right"],
                "right" => vec!["left"],
                k => vec![k; 4],
            };
            for k in again {
                match self.press(&[k]) {
                    Ok(g) if g.same(want) => return Some(g),
                    Ok(g) if g.at != want.at => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        let g = self.replay_to(path, Some(want))?;
        if g.at == want.at && g.focused() == want.focused() {
            Some(g)
        } else {
            self.report.notes.insert(format!("[{}] {}: a fresh start along the same buttons arrived at {}", self.profile.name, spot(want), spot(&g)));
            None
        }
    }

    /// The time is up (the budget follows the machine's load): remembers the page it happened on, for the failure.
    fn out_of_time(&mut self, at: &str) -> bool {
        let up = self.started.elapsed() > self.budget;
        if up && self.timed_out.is_none() {
            self.timed_out = Some(at.to_string());
        }
        up
    }

    /// One walk from the UI's frame `from` in `dir`: the frames after each press, until the focus comes round to one seen,
    /// stops (a press changes nothing), leaves the page or the walk's length is reached.
    fn walk(&mut self, from: &Frame, dir: &str) -> (Vec<Frame>, Walked) {
        let mut frames: Vec<Frame> = Vec::new();
        let mut last = from.clone();
        for _ in 0..MAX_WALK {
            let Ok(f) = self.press(&[dir]) else {
                self.fail(format!("{}: {dir} ended pocket-ui", spot(&last)));
                return (frames, Walked::Ended);
            };
            if f.at != from.at {
                self.fail(format!("{}: {dir} left the page for {}", spot(&last), f.at));
                return (frames, Walked::Left);
            }
            self.check_focus(&last, &f, dir);
            if f.same(&last) {
                return (frames, Walked::Stopped);
            }
            let round = f.same(from) || frames.iter().any(|s| s.same(&f));
            frames.push(f.clone());
            if round {
                return (frames, Walked::Round);
            }
            last = f;
        }
        (frames, Walked::Cut)
    }

    /// Explores the page `path` leads to from a fresh start (the UI is on it, `entry` its frame): its rows, its keys, and the
    /// pages A (or another key) opens, one level deeper each. `ancestors`: the pages on the way, which A or B landing on is
    /// no new page.
    fn explore(&mut self, path: Vec<String>, entry: Frame, ancestors: Vec<String>, depth: usize) {
        if self.out_of_time(&entry.at) {
            return;
        }
        if leaves(&entry.at) && depth > 0 {
            // The shelf and a game are not this explorer's (tests/cabinet_navigation.rs and the journeys walk them).
            return;
        }
        *self.report.pages.entry(entry.at.clone()).or_default() += 1;
        if self.report.pages[&entry.at] > 1 {
            // Seen from another row: one walk per page.
            return;
        }
        // The rows, as Down reaches them from the entry and then Up from the entry: each focus once.
        let mut reached: BTreeSet<String> = entry.focused().into_iter().collect();
        let mut drawn: BTreeSet<String> = entry.controls().into_iter().map(|c| c.name).collect();
        // Rows a walk cut at its length showed only in its last screens: the list goes on, the walk stopped.
        let mut near_cut: BTreeSet<String> = BTreeSet::new();
        let note = |frames: &[Frame], how: &Walked, drawn: &mut BTreeSet<String>, reached: &mut BTreeSet<String>, near_cut: &mut BTreeSet<String>| {
            let tail = if *how == Walked::Cut { frames.len().saturating_sub(LAST_SCREENS) } else { frames.len() };
            for (i, f) in frames.iter().enumerate() {
                for c in f.controls() {
                    if drawn.insert(c.name.clone()) && i >= tail {
                        near_cut.insert(c.name.clone());
                    }
                }
                reached.extend(f.focused());
            }
        };
        let (downs, how) = self.walk(&entry, "down");
        note(&downs, &how, &mut drawn, &mut reached, &mut near_cut);
        if matches!(how, Walked::Ended | Walked::Left) {
            return;
        }
        let mut seq: Vec<Frame> = vec![entry.clone()];
        seq.extend(downs.iter().take_while(|f| !f.same(&entry)).cloned());
        if how != Walked::Round {
            // Back to the entry (Up as often as Down moved, else a fresh start), then Up from there.
            let moved = downs.len();
            let back: Vec<&str> = vec!["up"; moved];
            let at_entry = self.press(&back).is_ok_and(|f| f.at == entry.at && f.focused() == entry.focused());
            if at_entry || self.restore(&entry, &path, "").is_some() {
                let (ups, how) = self.walk(&entry, "up");
                note(&ups, &how, &mut drawn, &mut reached, &mut near_cut);
                if matches!(how, Walked::Ended | Walked::Left) {
                    return;
                }
            }
        }
        // A grid of tiles (Add games, the quick settings): every direction from every focus reached, going back with the
        // opposite one (a fresh start when that doesn't land), until no new focus turns up. Left/Right on a row of values
        // change the value and leave the focus: no new state, and the opposite key puts the value back.
        let mut extra: Vec<(Vec<String>, Frame)> = Vec::new();
        if near_cut.is_empty() && drawn.iter().any(|n| !reached.contains(n)) {
            let mut states: Vec<(Vec<String>, Frame)> = seq.iter().enumerate().map(|(j, f)| (vec!["down".to_string(); j], f.clone())).collect();
            let mut i = 0;
            while i < states.len() && states.len() < MAX_GRID && !self.out_of_time(&entry.at) {
                let (moves, at) = states[i].clone();
                for (dir, back) in [("right", "left"), ("left", "right"), ("down", "up"), ("up", "down")] {
                    let now = self.press(&[]).ok();
                    if !now.as_ref().is_some_and(|n| n.at == at.at && n.focused() == at.focused()) {
                        let full: Vec<String> = path.iter().chain(moves.iter()).cloned().collect();
                        match self.replay_to(&full, Some(&at)) {
                            Some(f) if f.at == at.at && f.focused() == at.focused() => {}
                            _ => break,
                        }
                    }
                    let Ok(f) = self.press(&[dir]) else { break };
                    if f.at != entry.at {
                        continue;
                    }
                    drawn.extend(f.controls().into_iter().map(|c| c.name));
                    if let Some(n) = f.focused() {
                        if reached.insert(n) {
                            let more: Vec<String> = moves.iter().cloned().chain([dir.to_string()]).collect();
                            states.push((more.clone(), f.clone()));
                            extra.push((more, f));
                        }
                    }
                    let _ = self.press(&[back]);
                }
                i += 1;
            }
        }
        if self.out_of_time(&entry.at) {
            // The grid's walk stopped for the time, not because no focus was left: no reachability verdict on rows it didn't
            // get to (a loaded gate machine, 07.11.2026, reported Add games' tiles as unreachable); the test fails as a timeout.
            return;
        }
        let missed: Vec<&String> = drawn.iter().filter(|n| !reached.contains(*n) && !near_cut.contains(*n)).collect();
        if !missed.is_empty() {
            self.fail(format!("{}: rows drawn that Up/Down never reach: {}", entry.at, missed.iter().map(|n| format!("“{n}”")).collect::<Vec<_>>().join(", ")));
        }
        if OWN_KEYS.contains(&page_of(&entry.at)) {
            self.report.notes.insert(format!("[{}] {}: takes every button itself; only leaving it is checked", self.profile.name, entry.at));
            return;
        }
        // Each row of the Down walk: A, and the keys its hints name.
        let Some(mut here) = self.restore(&entry, &path, "") else { return };
        let mut tried: BTreeSet<(String, String)> = BTreeSet::new();
        for (j, want) in seq.iter().enumerate() {
            if self.out_of_time(&entry.at) {
                return;
            }
            let row_path: Vec<String> = path.iter().cloned().chain(std::iter::repeat_n("down".to_string(), j)).collect();
            if j > 0 {
                match self.press(&["down"]) {
                    Ok(f) if f.at == want.at && f.focused() == want.focused() => here = f,
                    _ => match self.restore(want, &row_path, "") {
                        Some(f) => here = f,
                        None => continue,
                    },
                }
            }
            self.try_a(&here, &row_path, &ancestors, depth);
            match self.restore(&here, &row_path, "a") {
                Some(f) => here = f,
                None => continue,
            }
            // The keys the hints name on this row, each once per page and word. A key that opens a page of its own (X
            // "Style…", START "Skip the rest", the first start's B "Continue offline") is followed like A.
            for (k, label) in here.hints.clone() {
                let keys = keys_of(&k);
                let back = k == "B" && BACK_WORDS.contains(&label.as_str());
                if k == "A" || back || k == "HOME" || label.to_lowercase().contains("hold") || !tried.insert((k.clone(), label.clone())) {
                    continue;
                }
                if keys.is_empty() {
                    self.report.notes.insert(format!("[{}] {}: a hint key the explorer doesn't know: “{k} {label}”", self.profile.name, here.at));
                    continue;
                }
                let mut works = false;
                for key in keys {
                    match self.changes(&[key], &here) {
                        Ok((f, changed)) => {
                            works |= changed;
                            if f.at != here.at && !leaves(&f.at) && !ancestors.contains(&f.at) && !self.report.pages.contains_key(&f.at) && depth < self.max_depth {
                                let child_path: Vec<String> = row_path.iter().cloned().chain([key.to_string()]).collect();
                                let mut more = ancestors.clone();
                                more.push(here.at.clone());
                                self.explore(child_path.clone(), f.clone(), more, depth + 1);
                                // B undoes what another key opened; not what B itself did.
                                if key != "b" {
                                    self.check_back(&here, &f, &child_path, false);
                                }
                            }
                        }
                        // It ended pocket-ui: it did something.
                        Err(_) => works = true,
                    }
                    match self.restore(&here, &row_path, key) {
                        Some(f) => here = f,
                        None => break,
                    }
                    if works {
                        break;
                    }
                }
                if !works {
                    self.fail(format!("{}: the hint bar names “{k} {label}”, and it does nothing", spot(&here)));
                }
            }
        }
        // The tiles only the grid's walk found: A on each (a fresh start there each).
        for (moves, want) in extra {
            if self.out_of_time(&entry.at) {
                return;
            }
            let row_path: Vec<String> = path.iter().chain(moves.iter()).cloned().collect();
            let Some(here) = self.replay_to(&row_path, Some(&want)) else { continue };
            if here.focused() == want.focused() {
                self.try_a(&here, &row_path, &ancestors, depth);
            }
        }
        // Keys the bar doesn't name, once per page on its first row.
        let Some(first) = self.restore(&entry, &path, "") else { return };
        let keys: Vec<&str> = OTHER_KEYS.iter().chain(if depth == 0 { ENTRY_KEYS } else { &[] }).copied().collect();
        for key in keys {
            if first.names_hint(key) {
                continue;
            }
            let Ok((f, changed)) = self.changes(&[key], &first) else {
                self.report.notes.insert(format!("[{}] {}: {key} ended pocket-ui", self.profile.name, spot(&first)));
                if self.restore(&first, &path, "").is_none() {
                    return;
                }
                continue;
            };
            if changed && self.changes_again(key, &first, &path) {
                self.report.unnamed.insert(format!("[{}] {}: {} works ({}) but the hint bar names {}", self.profile.name, first.at, key.to_uppercase(), if f.at != first.at { f.at.clone() } else { "the page changes".into() }, first.hint_keys().join(" ")));
            }
            if self.restore(&first, &path, key).is_none() {
                return;
            }
        }
    }

    /// A on the row `here` (the UI is there): a page of its own is explored and B must come back from it; a change on this
    /// page needs an A hint; nothing at all must not have one.
    fn try_a(&mut self, here: &Frame, row_path: &[String], ancestors: &[String], depth: usize) {
        if here.focused().as_deref().is_some_and(|n| NOT_PRESSED.iter().any(|p| n.starts_with(p))) {
            return;
        }
        match self.changes(&["a"], here) {
            Err(_) => {
                self.report.notes.insert(format!("[{}] {}: A ended pocket-ui (a restart or quit)", self.profile.name, spot(here)));
            }
            Ok((f, _)) if f.at != here.at && leaves(&f.at) => {
                // A went to the shelf or into a game (Finish, Continue): leaving, not a page.
            }
            Ok((f, _)) if f.at != here.at && !ancestors.contains(&f.at) && depth < self.max_depth => {
                let child_path: Vec<String> = row_path.iter().cloned().chain(["a".to_string()]).collect();
                let mut more = ancestors.to_vec();
                more.push(here.at.clone());
                self.explore(child_path.clone(), f.clone(), more, depth + 1);
                self.check_back(here, &f, &child_path, true);
            }
            Ok((_, false)) if here.names_hint("a") => self.fail(format!("{}: the hint bar names A, and A does nothing", spot(here))),
            // A row of values whose hint names Left/Right for the change: A stepping it too is a second way, not a hidden one.
            Ok((f, true)) if !here.names_hint("a") && f.at == here.at && (here.names_hint("left") || here.names_hint("right")) => {
                self.report.unnamed.insert(format!("[{}] {}: A changes the value as Left/Right do", self.profile.name, spot(here)));
            }
            Ok((f, true)) if !here.names_hint("a") && f.at == here.at && self.changes_again("a", here, row_path) => {
                self.fail(format!("{}: A changes the page but the hint bar doesn't name A ({})", spot(here), here.hint_keys().join(" ")))
            }
            _ => {}
        }
    }

    /// A key seemed to change `here` though nothing names it: again from a fresh start, once the page stands still (a page
    /// still reading or counting changes by itself). True when it still does.
    fn changes_again(&mut self, key: &str, here: &Frame, row_path: &[String]) -> bool {
        let Some(mut f) = self.replay_to(row_path, Some(here)) else { return false };
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let Ok(g) = self.press(&[]) else { return false };
            if g.same(&f) {
                break;
            }
            if Instant::now() > until {
                self.report.notes.insert(format!("[{}] {}: the page keeps changing by itself; {} not judged", self.profile.name, spot(here), key.to_uppercase()));
                return false;
            }
            f = g;
        }
        // An asynchronous read can finish between the original frame and this recheck (Remote access: Off
        // without an address becomes externally enabled). Judge the key against the bar the settled page offers.
        if f.names_hint(key) {
            return false;
        }
        self.changes(&[key], &f).is_ok_and(|(_, changed)| changed)
    }

    /// The list moved its selection (`@list`): the picture changed, and the focus mark is on the selected row when it is on
    /// the list at all (a page with tiles above the list keeps the focus there).
    fn check_focus(&mut self, before: &Frame, after: &Frame, dir: &str) {
        // Grids need no touch-list metadata. A move must still leave a visible selection on a drawn control.
        if before.focused().is_some() && after.focused().is_none() && after.controls().len() > 1 {
            self.fail(format!("{}: after {dir} no drawn control shows the focus", after.at));
        }
        let (Some((sel, _)), Some((was, _))) = (after.list, before.list) else { return };
        if sel != was && after.hash == before.hash {
            self.fail(format!("{}: {dir} moves the selection (row {was} → {sel}) and the picture stays the same", spot(before)));
        }
        let on = |f: &Rect, r: &Rect| overlap(f, r) * 2 > area(r).min(area(f));
        let Some((r, _)) = after.hits.iter().find(|(_, i)| *i == sel) else { return };
        if after.focus.is_empty() && after.controls().len() > 1 {
            self.fail(format!("{}: after {dir} the list's row {sel} is selected and nothing on the page shows the focus", after.at));
        } else if !after.focus.iter().any(|f| on(f, r)) && after.focus.iter().any(|f| after.hits.iter().any(|(h, i)| *i != sel && on(f, h))) {
            self.fail(format!("{}: after {dir} the list's row {sel} is selected and the focus mark is on another row", after.at));
        }
    }

    /// B from the page `child_entry` (opened from `parent` by `child_path`'s last key) must come back to `parent`'s page and
    /// row, in at most two presses (a dialog first), and a page B leaves must say B in its bar. A Settings page opened from
    /// outside Settings (a deep link: the update card's A, the leave dialog's Report a problem) goes back into Settings.
    /// `by_a`: A opened it, so B comes back to that row; a shortcut key (Y "From your phone" on a system tile) only has to come
    /// back to the page.
    fn check_back(&mut self, parent: &Frame, child_entry: &Frame, child_path: &[String], by_a: bool) {
        // Where the walk of the child page left the UI: still on that page, B goes from there; else a fresh start.
        let here = self.press(&[]).ok().filter(|f| f.at == child_entry.at);
        let child = match here {
            Some(f) => f,
            None => match self.replay_to(child_path, Some(child_entry)) {
                Some(f) => f,
                None => return,
            },
        };
        // What the page's entry said (its hints are the ones a player sees on arriving).
        let child = Frame { hints: child_entry.hints.clone(), ..child };
        let deep_link = child.at.starts_with("settings:") && !parent.at.starts_with("settings:");
        let mut f = child.clone();
        for presses in 1..=2 {
            let Ok(g) = self.press(&["b"]) else {
                self.fail(format!("{}: B ended pocket-ui", spot(&child)));
                return;
            };
            // The parent's page, also when what covered it has closed since (the quick settings, a card said once).
            let landed = page_of(&g.at) == page_of(&parent.at) || (deep_link && g.at.starts_with("settings:") && g.at != child.at);
            if landed {
                if by_a && g.at == parent.at && !g.same_row(parent) {
                    self.fail(format!("{}: B goes back to {} but not to the row it came from ({:?} instead of {:?})", child.at, parent.at, g.focused(), parent.focused()));
                }
                if presses == 1 && !child.names_hint("b") && !OWN_KEYS.contains(&page_of(&child.at)) {
                    self.fail(format!("{}: B goes back, and the hint bar doesn't say so ({})", child.at, child.hint_keys().join(" ")));
                }
                return;
            }
            if g.same(&f) {
                break;
            }
            f = g;
        }
        // Another key the bar names for going back (the first start's L1 Back).
        let back = child.hints.iter().find(|(k, l)| k != "B" && BACK_WORDS.contains(&l.as_str())).cloned();
        if let Some((k, l)) = back {
            if let Some(f) = self.replay_to(child_path, Some(child_entry)) {
                if let Some(key) = keys_of(&k).first() {
                    if self.press(&[key]).is_ok_and(|g| g.at == parent.at) {
                        self.report.notes.insert(format!("[{}] {}: leaves with “{k} {l}”, not B", self.profile.name, f.at));
                        return;
                    }
                }
            }
        }
        self.fail(format!("{}: B does not lead back to {} (opened from {})", child.at, parent.at, spot(parent)));
    }

    /// The card on the shelf saying a newer pocket-ui is ready (its keys are in its words, "Press A for Settings › System ›
    /// Updates, B to close."): A goes to the Updates page and B from there back, B on the card closes it.
    fn update_card(&mut self) {
        let Some(card) = self.replay(&[]) else { return };
        if !card.at.ends_with(" / notice") {
            self.fail(format!("the newer-version card is not on the shelf ({})", card.at));
            return;
        }
        match self.press(&["a"]) {
            Ok(f) if f.at == "settings:Updates" => self.check_back(&card, &f, &["a".to_string()], true),
            Ok(f) => self.fail(format!("A on the newer-version card goes to {} instead of Settings › System › Updates", f.at)),
            Err(_) => self.fail("A on the newer-version card ended pocket-ui".to_string()),
        }
        let Some(card) = self.replay(&[]) else { return };
        match self.press(&["b"]) {
            Ok(f) if f.at == page_of(&card.at) => {}
            Ok(f) => self.fail(format!("B on the newer-version card leads to {} instead of closing it", f.at)),
            Err(_) => self.fail("B on the newer-version card ended pocket-ui".to_string()),
        }
        *self.report.pages.entry(card.at).or_default() += 1;
    }

    /// Explores what `keys` open from a fresh start, as a page of its own.
    fn entry(&mut self, keys: &[&str], ancestors: &[&str]) {
        let path: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        let Some(f) = self.replay(&path) else {
            self.fail(format!("{keys:?} from a fresh start ended pocket-ui"));
            return;
        };
        self.explore(path, f, ancestors.iter().map(|a| a.to_string()).collect(), 0);
    }

    fn finish(mut self) -> (Report, usize, usize) {
        let frames = self.ui.as_ref().map_or(0, |u| u.frames);
        if let Some(at) = self.timed_out.take() {
            let budget = self.budget.as_secs();
            self.fail(format!("explorer ran out of time at {at} (budget {budget} s for this machine's load): pages after it not walked, nothing proven unreachable"));
        }
        self.report.notes.insert(format!("[{}] {} starts ({} replayed one button at a time), {:.0} s starting and replaying", self.profile.name, self.runs, self.report.slow_replays, self.replaying.as_secs_f64()));
        (self.report, self.runs, frames)
    }
}

/// What one test explores.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    /// START on the shelf: every category, row and page of Settings.
    Settings,
    /// What covers the shelf or a game: the quick settings (HOME), the power menu, SELECT + START's dialog, the pause menu of
    /// a game, the card saying a newer pocket-ui is ready.
    Overlays,
    /// The first start on a fresh card, every step from the welcome.
    FirstStart,
}

fn run(profile: Profile, part: Part) {
    let card = Card::new(&profile);
    let started = Instant::now();
    let mut reports = Vec::new();
    let mut starts = 0;
    let mut explore = |onboarded: bool, env: Vec<(&'static str, &'static str)>, entries: &[&[&str]]| {
        let mut ex = Explorer::new(&card, profile, onboarded, env);
        let home = ex.replay(&[]).expect("pocket-ui starts").at;
        for keys in entries {
            if keys.is_empty() && home.ends_with(" / notice") {
                ex.update_card();
            } else {
                ex.entry(keys, &[&home]);
            }
        }
        let (report, runs, _) = ex.finish();
        starts += runs;
        reports.push(report);
    };
    match part {
        Part::Settings => explore(true, Vec::new(), &[&["start"][..]]),
        Part::Overlays => {
            // A game first (the stand-in), then HOME: the pause menu.
            explore(true, Vec::new(), &[&["home"][..], &["powermenu"][..], &["leave"][..], &["a", "a", "home"][..]]);
            explore(true, vec![("POCKETUI_UPDATE_HINT", "99.0.0")], &[&[][..]]);
        }
        Part::FirstStart => explore(false, Vec::new(), &[&[][..]]),
    }
    let mut all = Report::default();
    for r in reports {
        all.failures.extend(r.failures);
        all.unnamed.extend(r.unnamed);
        all.notes.extend(r.notes);
        for (p, n) in r.pages {
            *all.pages.entry(p).or_default() += n;
        }
    }
    let mut out = format!("{} {part:?}: {} pages, {starts} starts, {:.0} s\n", profile.name, all.pages.len(), started.elapsed().as_secs_f64());
    for p in all.pages.keys() {
        out += &format!("  page {p}\n");
    }
    for n in &all.notes {
        out += &format!("  note {n}\n");
    }
    for n in &all.unnamed {
        out += &format!("  unnamed {n}\n");
    }
    eprintln!("{out}");
    if let Ok(dir) = std::env::var("POCKETUI_NAV_REPORT") {
        let _ = std::fs::create_dir_all(&dir);
        let failures = all.failures.iter().cloned().collect::<Vec<_>>().join("\n");
        let _ = std::fs::write(Path::new(&dir).join(format!("{}-{part:?}.txt", profile.name)), format!("{out}{failures}\n"));
    }
    assert!(all.failures.is_empty(), "{} navigation failures:\n{}", all.failures.len(), all.failures.iter().cloned().collect::<Vec<_>>().join("\n"));
}

/// The Mini V2 (1240x1080), an RG35XX Plus (640x480, no touch, no fan) and the RG DS Plus (1024x768): the three sizes.
const MINI_V2: Profile = Profile { name: "miniv2-1240x1080", device: "miniv2", text_size: "normal", extra_env: &[] };
const RG35XX_PLUS: Profile = Profile { name: "rg35xx-plus-640x480", device: "rg35xx-plus", text_size: "normal", extra_env: &[] };
const RG_DS_PLUS: Profile = Profile { name: "rgdsplus-1024x768", device: "rgdsplus", text_size: "normal", extra_env: &[] };
const RG_DS_PLUS_LARGE: Profile = Profile { name: "rgdsplus-1024x768-large", device: "rgdsplus", text_size: "large", extra_env: &[] };
/// An SM8550 handheld: the screen only turns off (no sleep), ROCKNIX owns the fan; Settings has rows of its own.
/// The TrimUI Brick on its own card (1024x768), as tests/brick_settings_walk.rs runs it.
#[cfg(feature = "brick")]
const BRICK: Profile = Profile { name: "brick-1024x768", device: "brick", text_size: "normal", extra_env: &[("POCKETUI_BRICK_STANDALONE", "1")] };
const ODIN_2_MINI: Profile = Profile { name: "odin2mini", device: "odin2mini", text_size: "normal", extra_env: &[] };

// The merge gate: each part once, at a different size (1024x768 is the tightest page, where hints give way first).

#[test]
#[cfg_attr(feature = "brick", ignore = "release check: the ROCKNIX sizes; main's gate runs them, a Brick build runs the Brick's")]
fn every_settings_page_at_1024x768() {
    run(RG_DS_PLUS, Part::Settings);
}

#[test]
#[cfg_attr(feature = "brick", ignore = "release check: the ROCKNIX sizes; main's gate runs them, a Brick build runs the Brick's")]
fn quick_settings_power_menu_pause_menu_and_update_card_at_640x480() {
    run(RG35XX_PLUS, Part::Overlays);
}

#[test]
#[cfg_attr(feature = "brick", ignore = "release check: the ROCKNIX sizes; main's gate runs them, a Brick build runs the Brick's")]
fn the_first_start_step_by_step() {
    run(MINI_V2, Part::FirstStart);
}

// The release check (scripts/release-check.sh): the other sizes and Large text.

#[test]
#[ignore = "release check: the explorer at the other sizes"]
fn every_settings_page_on_the_mini_v2() {
    run(MINI_V2, Part::Settings);
}

#[test]
#[ignore = "release check: the explorer at the other sizes"]
fn every_settings_page_at_640x480() {
    run(RG35XX_PLUS, Part::Settings);
}

#[test]
#[ignore = "release check: the explorer on an SM8550 handheld"]
fn every_settings_page_on_the_odin_2_mini() {
    run(ODIN_2_MINI, Part::Settings);
}

#[test]
#[ignore = "release check: the explorer in Large text"]
fn every_settings_page_at_1024x768_in_large_text() {
    run(RG_DS_PLUS_LARGE, Part::Settings);
}

#[test]
#[ignore = "release check: the explorer at the other sizes"]
fn quick_settings_power_menu_pause_menu_and_update_card_on_the_mini_v2() {
    run(MINI_V2, Part::Overlays);
}

#[test]
#[ignore = "release check: the explorer at the other sizes"]
fn the_first_start_step_by_step_at_640x480() {
    run(RG35XX_PLUS, Part::FirstStart);
}

// A Brick build's gate: the three parts on the Brick, where Settings › Look › Layout's unreachable rows were found.

#[cfg(feature = "brick")]
#[test]
fn every_settings_page_on_the_brick() {
    run(BRICK, Part::Settings);
}

#[cfg(feature = "brick")]
#[test]
fn quick_settings_power_menu_pause_menu_and_update_card_on_the_brick() {
    run(BRICK, Part::Overlays);
}

#[cfg(feature = "brick")]
#[test]
fn the_first_start_step_by_step_on_the_brick() {
    run(BRICK, Part::FirstStart);
}

// Device regressions run in the merge gate too: the broad walk at one size cannot cover a short grid,
// a fan owned by the OS, and a saved-network sheet reached during the first start.
fn rc03_profiles() -> [Profile; 3] {
    [RG35XX_PLUS, MINI_V2, ODIN_2_MINI]
}

fn select_named(ui: &mut Ui, name: &str) -> Frame {
    let mut f = ui.settled(&[]).unwrap();
    for _ in 0..MAX_WALK {
        if f.focused().as_deref() == Some(name) { return f; }
        f = ui.settled(&["down"]).unwrap();
    }
    panic!("cannot focus {name:?} on {}:\n{}", f.at, f.text());
}

fn open_named(ui: &mut Ui, name: &str) -> Frame {
    select_named(ui, name);
    ui.settled(&["a"]).unwrap()
}

/// One working network and one saved network outside its range. The helpers only read these stand-ins.
fn saved_wifi(card: &Card) {
    executable(&card.path().join("bin/nmcli"), r#"#!/bin/sh
base=$(dirname "$0")
printf '%s\n' "$*" >> "$base/nmcli.calls"
case "$*" in
  '-g GENERAL.STATE,GENERAL.CON-UUID,GENERAL.REASON device show wlan0')
    if [ -f "$base/disconnected" ]; then printf '30 (disconnected)\n--\n'; else printf '100 (connected)\nuuid-home\n'; fi ;;
  '-e no -g 802-11-wireless.ssid connection show uuid uuid-home') echo Home ;;
  '-w 2 device disconnect wlan0') : > "$base/disconnected" ;;
  '-t -f NAME,UUID,TYPE,ACTIVE,STATE connection show') printf 'Home:uuid-home:802-11-wireless:yes:activated\nOffice:uuid-office:802-11-wireless:no:\n' ;;
  '-g 802-11-wireless.ssid,802-11-wireless.hidden connection show uuid uuid-home') printf 'Home\nno\n' ;;
  '-g 802-11-wireless.ssid,802-11-wireless.hidden connection show uuid uuid-office') printf 'Office\nno\n' ;;
  *'wifi list'*) printf 'Home:78:WPA2:*\n' ;;
  *) exit 1 ;;
esac
"#);
    executable(&card.path().join("bin/iw"), "#!/bin/sh\nprintf 'country AT: DFS-ETSI\\n'\n");
    executable(&card.path().join("bin/ip"), "#!/bin/sh\nexit 0\n");
}

#[test]
fn rc03_first_start_saved_network_menu_b_returns_to_the_list_on_each_device() {
    for profile in rc03_profiles() {
        let card = Card::new(&profile);
        saved_wifi(&card);
        let mut ui = Ui::start(&card, &profile, false, 1, &[]);
        ui.settled(&["a"]).unwrap();
        // A scan is asynchronous: wait for the named network, not a fixed delay.
        let mut f = ui.settled(&[]).unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while !f.text().contains("Office") && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
            f = ui.settled(&[]).unwrap();
        }
        let list = select_named(&mut ui, "Office");
        for _ in 0..2 {
            let menu = ui.settled(&["a"]).unwrap();
            assert!(menu.text().contains("Connect") && menu.text().contains("Forget"), "{}: {}", profile.name, menu.text());
            let back = ui.settled(&["b"]).unwrap();
            assert_eq!(back.at, list.at, "{}: B must close the menu before leaving Wi-Fi", profile.name);
            assert!(back.same_row(&list), "{}: back on the same network", profile.name);
        }
        let welcome = ui.settled(&["b"]).unwrap();
        assert!(welcome.at.contains("welcome"), "{}", welcome.at);
        let wifi = ui.settled(&["a"]).unwrap();
        assert!(!wifi.text().contains("Forget"), "returning to Wi-Fi has no stale menu");
    }
}

#[test]
fn rc03_add_games_grid_keeps_every_selected_tile_visible_on_each_device() {
    for profile in rc03_profiles() {
        let card = Card::new(&profile);
        // A real card offers more than one row. Empty systems count too, as in ROCKNIX's catalog.
        let cfg = card.path().join("es_systems.cfg");
        let mut xml = std::fs::read_to_string(&cfg).unwrap();
        let extra: String = pocket_ui::systems_grid::POPULAR.iter().filter(|id| !["gb", "gba", "snes"].contains(id)).map(|id| {
            format!("<system><name>{id}</name><fullname>{id}</fullname><path>{}/roms/{id}</path><extension>.zip</extension><command>true</command></system>\n", card.path().display())
        }).collect();
        xml = xml.replace("</systemList>", &format!("{extra}</systemList>"));
        std::fs::write(cfg, xml).unwrap();
        let mut ui = Ui::start(&card, &profile, true, 1, &[]);
        ui.settled(&["start"]).unwrap();
        open_named(&mut ui, "Games");
        let grid = open_named(&mut ui, "Add games");
        assert!(grid.at.contains("games"), "{}", grid.at);
        let mut names = BTreeSet::new();
        // Across every row and into the ways, then back through the grid's columns.
        for key in ["down", "down", "down", "down", "up", "up", "up", "right", "down", "down", "left", "up"] {
            let f = ui.settled(&[key]).unwrap();
            let name = f.focused().unwrap_or_else(|| panic!("{}: {key} leaves no visible focus:\n{}", profile.name, f.text()));
            names.insert(name);
        }
        assert!(names.len() >= 4, "walk must reach several different tile rows");
    }
}

#[test]
fn rc03_performance_fan_rows_only_offer_arrows_when_they_can_change() {
    for profile in rc03_profiles() {
        let card = Card::new(&profile);
        let mut ui = Ui::start(&card, &profile, true, 1, &[]);
        ui.settled(&["start"]).unwrap();
        open_named(&mut ui, "Playing");
        open_named(&mut ui, "Performance & fan");
        for name in ["Games: fan", "Fan outside games", "Smooth fan"] {
            let before = select_named(&mut ui, name);
            let changeable = before.names_hint("left");
            assert_eq!(changeable, profile.device == "miniv2", "{}: {name}", profile.name);
            let rect = before.controls().into_iter().find(|c| c.focused).unwrap().rect;
            let arrows = before.strings.iter().any(|(r, s)| inside(centre(r), &rect) && s.contains('›'));
            assert_eq!(arrows, changeable, "{}: {name} must not look editable when it is read-only:\n{}", profile.name, before.text());
            let after = ui.settled(&["right"]).unwrap();
            assert!(after.same_row(&before), "Left/Right keep the row");
            let row_text = |f: &Frame| {
                let rect = f.controls().into_iter().find(|c| c.focused).unwrap().rect;
                f.strings.iter().filter(|(r, _)| inside(centre(r), &rect)).map(|(_, s)| s.clone()).collect::<Vec<_>>()
            };
            assert_eq!(row_text(&before) != row_text(&after), changeable, "{}: {name}", profile.name);
        }
    }
}

#[test]
fn rc03_starter_pack_left_right_keep_the_focused_game() {
    let profile = RG35XX_PLUS;
    let card = Card::new(&profile);
    let mut ui = Ui::start(&card, &profile, true, 1, &[]);
    ui.settled(&["start"]).unwrap();
    open_named(&mut ui, "Games");
    open_named(&mut ui, "Starter pack");
    let before = ui.settled(&["down"]).unwrap();
    for key in ["left", "right"] {
        let after = ui.settled(&[key]).unwrap();
        assert!(after.same_row(&before), "{key} on a game row without a choice must keep its focus");
    }
}

/// The same painters as the interactive walk, at a reproducible point with made-up state files.
fn rc03_snapshot(card: &Card, profile: Profile, state: &str, env: &[(&str, &str)]) -> (Frame, PathBuf) {
    let name = state.replace(':', "-");
    let png = card.path().join(format!("{name}.png"));
    let record = card.path().join(format!("{name}.nav"));
    let mut cmd = Command::new(ui_binary());
    cmd.args(["--snapshot", state]).arg(&png)
        .env("POCKETUI_MOCK", "1").env("POCKETUI_DEVICE", profile.device)
        .env("POCKETUI_LANG", "en").env("POCKETUI_NAV_PROBE", &record)
        .env("POCKETUI_DATA", card.path().join(format!("data-{name}")))
        .env("POCKETUI_ES_SYSTEMS", card.path().join("es_systems.cfg"))
        .envs(profile.extra_env.iter().copied()).envs(env.iter().copied());
    if let Some(stub) = &card.stub { cmd.env("WAYLAND_DISPLAY", &stub.path); }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    let f = Frame::parse(&std::fs::read_to_string(record).unwrap());
    (f, png)
}

#[test]
fn rc03_retro_style_description_is_complete_on_the_first_start() {
    for profile in rc03_profiles() {
        let card = Card::new(&profile);
        let (f, _) = rc03_snapshot(&card, profile, "setup:first-look", &[("POCKETUI_FONT", "retro")]);
        let description = f.strings.iter().find(|(_, s)| s.starts_with("Pixel letters"));
        assert!(description.is_some_and(|(_, s)| s.ends_with('.') && !s.contains('…')), "{}: whole Retro description must be readable:\n{}", profile.name, f.text());
    }
}

#[test]
fn rc03_available_load_is_bright_even_when_another_pause_row_is_focused() {
    let profile = RG35XX_PLUS;
    let card = Card::new(&profile);
    let (f, png) = rc03_snapshot(&card, profile, "menu:pocket-core:0", &[]);
    let (rect, _) = f.strings.iter().find(|(_, s)| s == "Load").expect("Load painted");
    let decoder = png::Decoder::new(std::fs::File::open(png).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    let stride = info.color_type.samples();
    let logical = pocket_ui::scaled_layout::logical_size((info.width, info.height));
    let rect = (rect.0 * info.width as i32 / logical.0 as i32, rect.1 * info.height as i32 / logical.1 as i32,
        rect.2 * info.width as i32 / logical.0 as i32, rect.3 * info.height as i32 / logical.1 as i32);
    // Read the painted letters at their recorded place, not a guessed screen coordinate.
    let bright = (rect.1.max(0)..rect.3.min(info.height as i32)).any(|y| {
        (rect.0.max(0)..rect.2.min(info.width as i32)).any(|x| {
            let at = (y as usize * info.width as usize + x as usize) * stride;
            pixels[at..at + 3].iter().all(|v| *v > 220)
        })
    });
    assert!(bright, "an available Load must use the active text colour while focus is elsewhere");
}

#[test]
fn rc03_connect_to_an_absent_saved_network_does_not_drop_working_wifi() {
    for profile in rc03_profiles() {
        let card = Card::new(&profile);
        saved_wifi(&card);
        let mut ui = Ui::start(&card, &profile, false, 1, &[]);
        ui.settled(&["a"]).unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while !ui.settled(&[]).unwrap().text().contains("Office") && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        open_named(&mut ui, "Office");
        let mut f = ui.settled(&["a"]).unwrap();
        while f.at.contains("connecting") && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
            f = ui.settled(&[]).unwrap();
        }
        let calls = std::fs::read_to_string(card.path().join("bin/nmcli.calls")).unwrap();
        assert!(!card.path().join("bin/disconnected").exists(), "{}: selecting an absent saved network dropped Home:\n{calls}", profile.name);
        assert!(!calls.contains("connection up"), "an absent target must not start an activation");
        assert!(f.text().contains("out of range"), "{}: {}", profile.name, f.text());
    }
}

#[test]
fn rc03_a_hint_is_checked_after_the_remote_access_page_finishes_loading() {
    let profile = RG35XX_PLUS;
    let card = Card::new(&profile);
    executable(&card.path().join("bin/systemctl"), r#"#!/bin/sh
base=$(dirname "$0")
case "$*" in
  'is-active --quiet sshd')
    while [ ! -e "$base/remote-ready" ]; do sleep 0.01; done ;;
  'stop sshd') sleep 0.2 ;;
esac
exit 0
"#);
    // A pre-fix check presses A: keep every external command in the stand-in folder.
    executable(&card.path().join("bin/rm"), "#!/bin/sh\nexit 0\n");
    let mut ex = Explorer::new(&card, profile, true, Vec::new());
    let mut path = vec!["start".to_string()];
    let mut f = ex.replay(&path).unwrap();
    for name in ["Connections", "Remote access (SSH)"] {
        for _ in 0..MAX_WALK {
            if f.focused().as_deref() == Some(name) { break; }
            path.push("down".to_string());
            f = ex.press(&["down"]).unwrap();
        }
        assert_eq!(f.focused().as_deref(), Some(name));
        path.push("a".to_string());
        f = ex.press(&["a"]).unwrap();
    }
    assert!(f.text().contains("Looking at remote access"), "{}", f.text());
    assert!(!f.names_hint("a"), "the initial, offline Off state has no A yet");
    std::fs::write(card.path().join("bin/remote-ready"), "").unwrap();
    assert!(!ex.changes_again("a", &f, &path), "the loaded page now offers A: do not report that action against its old B-only hint bar");
}
