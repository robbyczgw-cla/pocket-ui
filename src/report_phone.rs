//! Settings → Report a problem on Android: one text with what a bug report needs, made on a press and handed on the
//! Android way: the share sheet (`ACTION_SEND`) or the clipboard, so the player picks where it goes. Nothing is sent by
//! the app, and there is no file on the phone to find (docs/user/android-mvp.md, "Report a problem").
//!
//! - **What**: the app (version, build, update channel), the device (model, Android version, CPU, screen, density,
//!   text size, game controllers by their USB ids), the look and language, the games per system with the core that plays
//!   each, the sound counters of the player's last audio line, and the end of the logs of pocket-ui and of the
//!   player (a process of its own), from logcat.
//! - **Privacy**: every line goes through [`Redactor::clean_phone`]: the library's game names become a system and a
//!   short code, `content://` addresses and folders in the phone's storage lose their folder names, and addresses,
//!   MACs, e-mail addresses, user names in paths and `key=value` passwords go as on Linux. The names of game
//!   controllers (a Bluetooth pad may carry its owner's name) and the phone's own name are never collected.
//! - **Size**: at most [`MOST_BYTES`], so it pastes into a GitHub issue (65,536 characters) and fits an Intent; the
//!   logs give way first, the newest lines stay.
//! - **Missing data** is said in words ("No game has been started on this phone yet."), never left blank.

use crate::report_redact::{GameName, Redactor};

/// The whole report, at most: under GitHub's 65,536 characters for an issue.
pub const MOST_BYTES: usize = 60_000;
/// A log line is cut here (a stack trace line or a dumped list can be very long).
const LINE_BYTES: usize = 400;

// ---- what the player sees ------------------------------------------------------------------------------------

/// Settings' row, and the page's title.
pub const ROW: &str = "Report a problem";
/// Above the rows before anything was done.
pub const ABOUT: &str = "One text with your device, cores, sound counters and the end of the logs. You choose where it goes; the app sends nothing.";
pub const WRITING: &str = "Writing the report\u{2026}";
pub const SHARED: &str = "The share sheet is open. Choose where the report goes, for example a message to yourself.";
pub const COPIED: &str = "Copied. Paste it into your report on GitHub.";
pub const NO_SHARE: &str = "Android has no app to share with. Use Copy the report instead.";
pub const NO_COPY: &str = "Android would not copy the report. Use Share the report instead.";
pub const FAILED: &str = "The report could not be written. Try again after restarting the app.";
/// What the cleaning takes out, in the player's words.
pub const LEFT_OUT: &str = "Left out: addresses, passwords, tokens, folder names. Games show as a short code, so name yours if it matters.";

// ---- what goes in ------------------------------------------------------------------------------------------------

/// One system of the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemLine {
    pub system: String,
    pub games: usize,
    /// The core or emulator that plays it ("mGBA", "optional download, not installed"), as the host knows it.
    pub core: String,
}

/// What the host knows; the report puts it in order.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// Android's side (`PocketActivity.deviceReport`): `key=value` lines, see [`device_value`].
    pub device: String,
    /// The look, appearance, language, text size: `("Look", "Shelf")`.
    pub settings: Vec<(String, String)>,
    pub systems: Vec<SystemLine>,
    /// Cores chosen for download and those installed.
    pub downloads: Vec<(String, bool)>,
    /// Android's side (`PocketActivity.recentLog`): the first line says `main=<pid> player=<pid>`, then logcat lines.
    pub log: String,
}

/// A value of the device lines (`model=AYN Odin2 Mini`), empty when absent.
pub fn device_value<'a>(device: &'a str, key: &str) -> &'a str {
    device.lines().find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('='))).map_or("", str::trim)
}

fn or_unknown(v: &str) -> &str {
    if v.is_empty() { "unknown" } else { v }
}

/// The report: cleaned, in order, at most [`MOST_BYTES`].
pub fn build(facts: &Facts, games: &[GameName]) -> String {
    let redactor = Redactor::new(Vec::new(), games);
    let d = |k: &str| device_value(&facts.device, k);
    let mut head = String::new();
    head.push_str("pocket-ui problem report (Android)\n");
    head.push_str(&format!("Made: {}\n", or_unknown(d("time"))));
    head.push_str("Paste this into a new issue on pocket-ui's GitHub page (Issues, New issue, Bug report), say what happened and name the game if it matters.\n");
    head.push_str(&format!("{LEFT_OUT}\n\n"));

    head.push_str("== App ==\n");
    head.push_str(&format!("pocket-ui {} (build {})\n", or_unknown(d("app")), or_unknown(d("build"))));
    head.push_str(&format!("Package: {}\nUpdate channel: {}\n", or_unknown(d("package")), or_unknown(d("channel"))));
    for (k, v) in &facts.settings {
        head.push_str(&format!("{k}: {v}\n"));
    }

    head.push_str("\n== Device ==\n");
    let model = [d("manufacturer"), d("model")].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
    head.push_str(&format!("Model: {} ({})\n", or_unknown(&model), or_unknown(d("device"))));
    head.push_str(&format!("Android {} (API {}), {}\n", or_unknown(d("release")), or_unknown(d("sdk")), or_unknown(d("abi"))));
    head.push_str(&format!("Screen: {}, density {} dpi, refresh {} Hz, text size x{}\n", or_unknown(d("screen")), or_unknown(d("dpi")), or_unknown(d("refresh")), or_unknown(d("fontscale"))));
    head.push_str(&format!("Game controllers: {}\n", if d("gamepads").is_empty() { "none connected" } else { d("gamepads") }));
    head.push_str(&format!("Battery saver: {}\nFree space for the app: {}\n", or_unknown(d("batterysaver")), or_unknown(d("free"))));

    head.push_str("\n== Games and cores ==\n");
    if facts.systems.is_empty() {
        head.push_str("No games were found yet: no folder is chosen, or it holds no games.\n");
    }
    for s in &facts.systems {
        head.push_str(&format!("{}: {} game{}, {}\n", s.system, s.games, if s.games == 1 { "" } else { "s" }, s.core));
    }
    if facts.downloads.is_empty() {
        head.push_str("No optional cores chosen for download.\n");
    }
    for (core, installed) in &facts.downloads {
        head.push_str(&format!("Optional core {core}: {}\n", if *installed { "installed" } else { "chosen, not installed" }));
    }

    let (ui_lines, player_lines, earlier_lines) = split_log(&facts.log);
    let (ui_lines, player_lines, earlier_lines) = (collapse(&ui_lines), collapse(&player_lines), collapse(&earlier_lines));
    head.push_str("\n== Sound ==\n");
    let audio: Vec<&str> = player_lines.iter().chain(earlier_lines.iter()).chain(ui_lines.iter()).map(String::as_str).filter(|l| l.contains("player audio:")).collect();
    match audio.last() {
        Some(last) => {
            head.push_str(&format!("The player's last audio line (frames, underruns, queue, dropped frames, resampler, AAudio xruns), {} such lines in the log:\n", audio.len()));
            head.push_str(&clip(last));
            head.push('\n');
        }
        None if facts.log.trim().is_empty() => head.push_str("No sound counters: the log is empty.\n"),
        None => head.push_str("No sound counters yet: no game has played sound since Android last cleared the log.\n"),
    }
    let head = redactor.clean_phone(&head);

    // The logs share what is left, the newest lines of each; the player's get the most.
    let room = MOST_BYTES.saturating_sub(head.len() + 400);
    let sections = [("pocket-ui", &ui_lines, room * 3 / 10), ("player", &player_lines, room * 5 / 10), ("earlier runs of either", &earlier_lines, room * 2 / 10)];
    let mut out = head;
    for (name, lines, budget) in sections {
        out.push_str(&format!("\n== Log: {name} ==\n"));
        if lines.is_empty() {
            out.push_str(if name == "player" { "No player lines: no game was started since Android last cleared the log.\n" } else { "No lines.\n" });
            continue;
        }
        let mut chosen: Vec<String> = Vec::new();
        let mut used = 0;
        for line in lines.iter().rev() {
            let line = redactor.clean_phone(&clip(line));
            if used + line.len() + 1 > budget {
                break;
            }
            used += line.len() + 1;
            chosen.push(line);
        }
        if chosen.len() < lines.len() {
            out.push_str(&format!("(the last {} of {} lines)\n", chosen.len(), lines.len()));
        }
        for line in chosen.iter().rev() {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.truncate(floor_boundary(&out, MOST_BYTES));
    out
}

/// The words of a logcat line without its date, time, process, thread and level: `PocketMvp: AAudio opened: rate=…`.
fn message_of(line: &str) -> &str {
    let mut rest = line.trim_start();
    for _ in 0..5 {
        rest = rest.trim_start_matches(|c: char| !c.is_whitespace()).trim_start();
    }
    rest
}

/// Lines that say the same thing one after another (an audio stream opened every three seconds) become the first of them
/// and a count, so a few hundred of them don't push the lines that matter out of the room.
fn collapse(lines: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let mut j = i + 1;
        while j < lines.len() && message_of(lines[j]) == message_of(lines[i]) {
            j += 1;
        }
        out.push(lines[i].to_string());
        if j - i > 2 {
            out.push(format!("(the line above came {} times in a row, the last one at {})", j - i, lines[j - 1].split_whitespace().take(2).collect::<Vec<_>>().join(" ")));
        } else if j - i == 2 {
            out.push(lines[i + 1].to_string());
        }
        i = j;
    }
    out
}

/// A line cut to [`LINE_BYTES`] at a character boundary.
fn clip(line: &str) -> String {
    let line = line.trim_end();
    if line.len() <= LINE_BYTES {
        return line.to_string();
    }
    format!("{}\u{2026}", &line[..floor_boundary(line, LINE_BYTES)])
}

fn floor_boundary(s: &str, at: usize) -> usize {
    let mut i = at.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The log by process: pocket-ui's, the player's, and other lines of ours (earlier runs). logcat's `threadtime` lines
/// read `date time pid tid level tag: message`; the first line of `log` names the two process ids.
fn split_log(log: &str) -> (Vec<&str>, Vec<&str>, Vec<&str>) {
    let mut lines = log.lines();
    let first = lines.next().unwrap_or("");
    let pid = |key: &str| first.split_whitespace().find_map(|w| w.strip_prefix(key).and_then(|r| r.strip_prefix('='))).map(str::to_string);
    let (main, player) = (pid("main"), pid("player"));
    let (mut ui, mut pl, mut earlier) = (Vec::new(), Vec::new(), Vec::new());
    for l in lines {
        if l.trim().is_empty() {
            continue;
        }
        let p = l.split_whitespace().nth(2);
        match p {
            Some(p) if Some(p) == main.as_deref() => ui.push(l),
            Some(p) if Some(p) == player.as_deref() => pl.push(l),
            _ => earlier.push(l),
        }
    }
    (ui, pl, earlier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn device() -> String {
        "time=2026-11-11 15:04 UTC\napp=0.1.8\nbuild=18\npackage=xyz.pocketui.app\nchannel=stable\nmanufacturer=AYN\nmodel=Odin2 Mini\ndevice=odin2mini\nrelease=13\nsdk=33\nabi=arm64-v8a\nscreen=1240x1080\ndpi=320\nrefresh=120\nfontscale=1.0\ngamepads=1 (0x1234:0x5678)\nbatterysaver=off\nfree=21.4 GB\n".into()
    }

    fn log_line(pid: u32, level: char, tag: &str, msg: &str) -> String {
        format!("11-11 15:04:05.123  {pid}  {pid} {level} {tag}: {msg}")
    }

    fn facts() -> Facts {
        let mut log = String::from("main=100 player=200\n");
        for i in 0..5 {
            log.push_str(&log_line(100, 'I', "PocketMvp", &format!("library: {i} games from SAF")));
            log.push('\n');
        }
        log.push_str(&log_line(200, 'I', "PocketMvp", "player audio: frames=48000 underruns=2 queue_ms=40.00 dropped_frames=0 input_rate=32768.0 target_ms=40.0 video_frames=60 input_frames=48000 resampled_frames=48000 aaudio_xruns=0"));
        log.push('\n');
        Facts {
            device: device(),
            settings: vec![("Look".into(), "Shelf".into()), ("Appearance".into(), "dark".into()), ("Language".into(), "English".into())],
            systems: vec![SystemLine { system: "gba".into(), games: 2, core: "mGBA".into() }, SystemLine { system: "n64".into(), games: 1, core: "Mupen64Plus-Next (optional download, not installed)".into() }],
            downloads: vec![("mupen64plus_next".into(), false)],
            log,
        }
    }

    /// What the maintainer needs, in order, with the numbers filled in.
    #[test]
    fn the_report_has_the_app_the_device_the_cores_the_sound_and_the_logs() {
        let r = build(&facts(), &[]);
        for want in [
            "pocket-ui problem report (Android)", "Made: 2026-11-11 15:04 UTC", "pocket-ui 0.1.8 (build 18)", "Package: xyz.pocketui.app", "Look: Shelf",
            "Model: AYN Odin2 Mini (odin2mini)", "Android 13 (API 33), arm64-v8a", "Screen: 1240x1080, density 320 dpi, refresh 120 Hz, text size x1.0", "Game controllers: 1 (0x1234:0x5678)",
            "gba: 2 games, mGBA", "n64: 1 game, Mupen64Plus-Next (optional download, not installed)", "Optional core mupen64plus_next: chosen, not installed",
            "underruns=2", "== Log: pocket-ui ==", "library: 4 games from SAF", "== Log: player ==", "player audio: frames=48000", "Left out:",
        ] {
            assert!(r.contains(want), "missing {want:?} in\n{r}");
        }
        assert!(r.len() <= MOST_BYTES);
    }

    /// Nothing was played, no folder chosen, no log, a device that told little: every part says so in words.
    #[test]
    fn missing_data_is_said_in_words_never_left_blank() {
        let r = build(&Facts::default(), &[]);
        for want in [
            "Made: unknown", "Model: unknown (unknown)", "Game controllers: none connected", "No games were found yet", "No optional cores chosen for download.",
            "No sound counters: the log is empty.", "== Log: player ==\nNo player lines: no game was started", "== Log: pocket-ui ==\nNo lines.",
        ] {
            assert!(r.contains(want), "missing {want:?} in\n{r}");
        }
        // A log with lines but no game played: the sound section says why it has no counters.
        let mut f = facts();
        f.log = format!("main=100 player=200\n{}\n", log_line(100, 'I', "PocketMvp", "library: 1 games from SAF"));
        let r = build(&f, &[]);
        assert!(r.contains("No sound counters yet: no game has played sound"), "{r}");
        for heading in r.lines().filter(|l| l.starts_with("== ")) {
            let after = r.split(heading).nth(1).unwrap();
            assert!(!after.trim_start_matches('\n').starts_with("\n=="), "{heading} is empty");
        }
    }

    /// Made-up private values, put together from their parts (the public export's leak scan would take them whole for
    /// real ones).
    fn fakes() -> Vec<(&'static str, String)> {
        vec![
            ("{mail}", ["alex", "example.org"].join("@")),
            ("{ip}", ["192", "0", "2", "44"].join(".")),
            ("{mac}", ["a4", "c1", "38", "9f", "2e", "11"].join(":")),
            ("{home}", ["", "home", "jdoe"].join("/")),
            ("{secret}", ["pass", "word=hunter2", "secret"].concat()),
            ("{token}", ["tok", "en=Zx81RaToken", "AbCdEf0123"].concat()),
        ]
    }

    fn fill(text: &str) -> String {
        fakes().iter().fold(text.to_string(), |t, (k, v)| t.replace(k, v))
    }

    /// The privacy test: a log with one of each private thing an Android phone's logs and settings can hold; none of it
    /// reaches the text, and the lines around them stay.
    #[test]
    fn the_report_leaves_out_game_names_folder_names_addresses_and_user_names() {
        let games = vec![
            GameName { system: "gba".into(), name: "Pokémon Emerald".into(), path: PathBuf::from("gba/Pokemon - Emerald Version (USA, Europe).gba") },
            GameName { system: "snes".into(), name: "Chrono Trigger".into(), path: PathBuf::from("snes/Chrono Trigger (USA).sfc") },
        ];
        let lines = [
            "launch: Chrono Trigger with snes9x on snes",
            "launch: document content://com.android.externalstorage.documents/tree/primary%3AAlex%20Games%2Fsnes/document/primary%3AAlex%20Games%2Fsnes%2FChrono%20Trigger%20(USA).sfc",
            "opened /storage/emulated/0/Alex Games/gba/Pokemon - Emerald Version (USA, Europe).gba for mgba",
            "card /storage/1A2B-3C4D/Retro Stuff/n64/Hello.z64 mounted",
            "own files /data/user/0/xyz.pocketui.app/files/saves/00000000000000aa/slot1.state",
            "feedback from {mail}",
            "wifi {ip} gateway via {mac}",
            "build {home}/src/pocket-ui/src/lib.rs",
            "login: user=alexjdoe {secret} {token}",
            "toast: Pokémon Emerald saved",
        ];
        let mut log = String::from("main=100 player=200\n");
        for (i, l) in lines.iter().enumerate() {
            log.push_str(&log_line(if i % 2 == 0 { 100 } else { 200 }, 'I', "PocketMvp", &fill(l)));
            log.push('\n');
        }
        let mut f = facts();
        f.log = log;
        // A game name also in the core line and in a download's name.
        f.systems.push(SystemLine { system: "snes".into(), games: 1, core: "snes9x".into() });
        let r = build(&f, &games);
        for private in [
            "Chrono Trigger", "Chrono", "Pokémon", "Pokemon", "Emerald", "Alex Games", "Alex%20Games", "primary%3A", "Retro Stuff", "Hello.z64", "alex@", "192.0.2.44",
            "a4:c1:38", "jdoe", "hunter2", "Zx81RaToken", "alexjdoe",
        ] {
            assert!(!r.contains(private), "{private:?} is still in the report:\n{r}");
        }
        // The lines are still there, with the reasons readable.
        for kept in ["launch: [game snes", "content://com.android.externalstorage.documents/[removed]", "/storage/emulated/0/[removed]", "/storage/[card]/[removed]", "/data/user/0/xyz.pocketui.app/files/saves/", "[ip]", "[mac]", "[user]", "password=[removed]", "toast: [game gba"] {
            assert!(r.contains(kept), "{kept:?} is gone from the report:\n{r}");
        }
    }

    /// The settings and device lines never name a game or the owner: the report asks Android for neither the phone's name
    /// nor the controllers' names (checked on the facts the host passes, and on a hostile device line).
    #[test]
    fn the_phones_own_name_in_a_device_line_is_not_a_field_of_the_report() {
        let mut f = facts();
        f.device.push_str("devicename=Alex's Pocket\nowner=alex\n");
        let r = build(&f, &[]);
        assert!(!r.contains("Alex's Pocket") && !r.contains("owner"), "{r}");
    }

    /// A very long log still gives a report that pastes into an issue: under the limit, newest lines kept, said so.
    #[test]
    fn a_huge_log_is_cut_to_the_limit_and_keeps_the_newest_lines() {
        let mut f = facts();
        let mut log = String::from("main=100 player=200\n");
        for i in 0..6000 {
            log.push_str(&log_line(200, 'I', "PocketMvp", &format!("player step {i} {}", "x".repeat(300))));
            log.push('\n');
        }
        log.push_str(&log_line(200, 'E', "PocketMvp", "the very last player line"));
        log.push('\n');
        f.log = log;
        let r = build(&f, &[]);
        assert!(r.len() <= MOST_BYTES, "{} bytes", r.len());
        assert!(r.contains("the very last player line") && !r.contains("player step 0 "), "newest kept, oldest cut");
        assert!(r.contains("(the last "), "the cut is said");
        // One enormous line is cut, not carried whole.
        f.log = format!("main=100 player=200\n{}\n", log_line(100, 'I', "PocketMvp", &"y".repeat(100_000)));
        assert!(build(&f, &[]).len() < 10_000);
    }

    /// The Odin's real log: an audio stream opened every three seconds. A few hundred identical lines would push the
    /// lines that matter out of the room, so a run becomes its first line and a count.
    #[test]
    fn a_run_of_identical_log_lines_is_counted_not_repeated() {
        let mut f = facts();
        let mut log = String::from("main=100 player=200\n");
        for i in 0..300 {
            log.push_str(&format!("11-11 15:37:{:02}.{:03}  100  101 I PocketMvp: AAudio opened: rate=48000 burst=144 xruns=0\n", i % 60, i));
        }
        log.push_str(&log_line(100, 'E', "PocketMvp", "the one line that matters"));
        log.push('\n');
        f.log = log;
        let r = build(&f, &[]);
        assert_eq!(r.matches("AAudio opened").count(), 1, "{r}");
        assert!(r.contains("came 300 times in a row") && r.contains("the one line that matters"), "{r}");
        // Two in a row are just two lines.
        f.log = format!("main=100 player=200\n{}\n{}\n", log_line(100, 'I', "PocketMvp", "same"), log_line(100, 'I', "PocketMvp", "same"));
        assert_eq!(build(&f, &[]).matches(": same").count(), 2);
    }

    /// The texts the page shows are in the German and Spanish tables (the audit also finds them in the source).
    #[test]
    fn the_report_texts_have_a_german_and_a_spanish_line() {
        for lang in ["de", "es"] {
            let table = std::fs::read_to_string(format!("{}/assets/i18n/{lang}.tsv", env!("CARGO_MANIFEST_DIR"))).unwrap();
            for text in [ROW, ABOUT, WRITING, SHARED, COPIED, NO_SHARE, NO_COPY, FAILED, LEFT_OUT, "Share the report", "Copy the report"] {
                assert!(table.lines().any(|l| l.split('\t').next() == Some(text) && l.split('\t').nth(1).is_some_and(|t| !t.trim().is_empty())), "{lang}: no line for {text:?}");
            }
        }
    }
}
