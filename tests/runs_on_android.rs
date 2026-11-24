//! Android's handheld view: what "Runs on" says and what A on a game does where the phone has its own ways (pocket-ui's player,
//! a core to download, emulator apps), painted by the real program with a made-up phone (`POCKETUI_PHONE`, `play_route`).
//! The page has no ROCKNIX words (RetroArch settings, "pocket-core", "Standalone"), a core that is not on the phone is a Download
//! row with its size, an app the phone has is a choice, a choice the Mobile view made (launcher.json) shows here, and every row is
//! reached with the D-pad and its hint names only keys that do something.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py (as tests/live_fixes.rs).

use std::path::Path;
use std::process::Command;

include!("common/root.rs");

struct Stub(std::process::Child);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Card {
    dir: tempfile::TempDir,
    _stub: Option<Stub>,
}

/// A phone like the Odin 2 Mini: the cores of the APK except PSP, PPSSPP and Dolphin installed, two PS2 apps, an open channel.
const ODIN: &str = r#"{"apps": {"psp": [{"p": "org.ppsspp.ppsspp", "l": "PPSSPP"}],
    "dolphin": [{"p": "org.dolphinemu.dolphinemu", "l": "Dolphin Emulator"}],
    "ps2": [{"p": "com.armsx2", "l": "ARMSX2"}, {"p": "xyz.aethersx2.android", "l": "AetherSX2"}]},
    "cores": ["mgba", "bsnes", "nestopia", "pcsx_rearmed", "mupen64plus_next"],
    "offers": [{"core": "ppsspp", "name": "PSP", "size": 18600000}], "channel_open": true}"#;
const BARE: &str = r#"{"apps": {}, "cores": ["mgba"], "offers": [{"core": "ppsspp", "name": "PSP", "size": 18600000}], "channel_open": false}"#;

const PSP_GAME: &str = "Burnout Legends (USA).iso";
const PS2_GAME: &str = "Burnout Revenge (USA).iso";
const GBA_GAME: &str = "Golden Sun (USA, Europe).gba";

impl Card {
    fn new() -> Card {
        // A short path: the stub's socket has to fit.
        let dir = tempfile::Builder::new().prefix("pui-phone-").tempdir().unwrap();
        let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(status.success());
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), dir.path().join("wl").to_str().unwrap()]).spawn().unwrap();
            test_clock::wait_for(|| dir.path().join("wl").exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
            Stub(child)
        });
        Card { dir, _stub: stub }
    }

    /// Mobile's launcher.json with these choices, as the other view left it.
    fn mobile_chose(&self, json: &str) {
        std::fs::write(self.dir.path().join("data/launcher.json"), json).unwrap();
    }

    /// `(words, hints, focused row, rows)`: what the state's picture says (one line, so a wrapped sentence is whole), the hint
    /// bar's keys and words, the row the focus mark is on and the rows the page paints.
    fn look(&self, state: &str, lang: &str, phone: &str) -> (String, Vec<(String, String)>, Option<String>, Vec<String>) {
        let tmp: &Path = self.dir.path();
        let (png, words, nav, phone_file) = (tmp.join("shot.png"), tmp.join("words.tsv"), tmp.join("nav.tsv"), tmp.join("phone.json"));
        let _ = std::fs::remove_file(&words);
        let _ = std::fs::remove_file(&nav);
        std::fs::write(&phone_file, phone).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if tmp.join("wl").exists() {
            cmd.env("WAYLAND_DISPLAY", tmp.join("wl"));
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_VISIBLE_TEXT", &words).env("POCKETUI_NAV_PROBE", &nav)
            .env("POCKETUI_PHONE", &phone_file)
            .env("POCKETUI_DATA", tmp.join("data"))
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .output()
            .unwrap();
        assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        let words = std::fs::read_to_string(words).unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
        // The snapshot's record: every string with its box, the focus mark's box and the hints (`nav_probe`). The focused row is the
        // page's row name inside the focus mark; the rows are the names the page paints.
        let (mut hints, mut focus, mut strings) = (Vec::new(), None, Vec::new());
        for line in std::fs::read_to_string(nav).unwrap_or_default().lines() {
            let v: Vec<&str> = line.splitn(5, '\t').collect();
            if v.len() < 5 || v[0].starts_with('@') {
                continue;
            }
            let rect: Vec<i32> = v[..4].iter().filter_map(|n| n.parse().ok()).collect();
            if let Some(h) = v[4].strip_prefix("‹nav:hint›") {
                let (k, l) = h.split_once('\t').unwrap_or((h, ""));
                hints.push((k.to_string(), l.to_string()));
            } else if v[4] == "‹nav:focus›" {
                focus = Some(rect);
            } else if ["Runs on", "Download", "Save changes"].contains(&v[4]) && rect[0] < 200 {
                strings.push((rect, v[4].to_string()));
            }
        }
        let focused = focus.and_then(|f| strings.iter().find(|(r, _)| (r[1] + r[3]) / 2 >= f[1] && (r[1] + r[3]) / 2 <= f[3]).map(|(_, n)| n.clone()));
        (words, hints, focused, strings.into_iter().map(|(_, n)| n).collect())
    }

    fn words(&self, state: &str, lang: &str, phone: &str) -> String {
        self.look(state, lang, phone).0
    }
}

fn no_rocknix_words(words: &str) {
    for word in ["ROCKNIX", "RetroArch", "pocket-core", "Standalone", "Not installed on this Pocket", "shortcuts"] {
        assert!(!words.contains(word), "{word} in {words}");
    }
}

#[test]
fn a_core_to_download_is_a_row_with_its_size_and_the_page_has_no_rocknix_words() {
    let card = Card::new();
    let words = card.words(&format!("emu:psp:{PSP_GAME}"), "en", ODIN);
    assert!(words.contains("Runs on ‹ Default (pocket-ui player) ›"), "{words}");
    assert!(words.contains("Download PSP · 18.6 MB"), "{words}");
    no_rocknix_words(&words);
    // On the Download row it says what is missing and what A does.
    let (words, hints, _, _) = card.look(&format!("emu:psp:{PSP_GAME}:1"), "en", ODIN);
    assert!(words.contains("This system is not installed yet. Choose Download now to add it on Wi-Fi."), "{words}");
    assert_eq!(hints, [("B".to_string(), "Back".to_string()), ("A".to_string(), "Download now".to_string())], "{words}");
    no_rocknix_words(&words);
}

#[test]
fn a_core_on_the_phone_has_no_download_row_and_a_downloaded_one_none_either() {
    let card = Card::new();
    let words = card.words(&format!("emu:gba:{GBA_GAME}"), "en", ODIN);
    assert!(words.contains("Runs on ‹ Default (pocket-ui player) ›") && !words.contains("Download"), "{words}");
    let downloaded = ODIN.replace(r#""cores": ["#, r#""cores": ["ppsspp", "#);
    let words = card.words(&format!("emu:psp:{PSP_GAME}"), "en", &downloaded);
    assert!(words.contains("Default (pocket-ui player)") && !words.contains("Download"), "{words}");
}

#[test]
fn a_closed_channel_says_coming_soon_and_offers_no_key_that_does_nothing() {
    let card = Card::new();
    let (words, hints, _, _) = card.look(&format!("emu:psp:{PSP_GAME}:1"), "en", BARE);
    assert!(words.contains("Download") && words.contains("Not available here"), "{words}");
    // No emulator app for PSP on this phone: the page says so, and promises none (Odin 2 Mini 13.11.: "An emulator app on this
    // device can play it" with no app installed).
    assert!(words.contains("This version of pocket-ui can't play this system, and no emulator app on this device plays it."), "{words}");
    assert!(!words.contains("can play it"), "{words}");
    assert_eq!(hints, [("B".to_string(), "Back".to_string())], "{words}");
    // With PPSSPP on the phone the line names it and where to choose it.
    let with_app = BARE.replace(r#""apps": {}"#, r#""apps": {"psp": [{"p": "org.ppsspp.ppsspp", "l": "PPSSPP"}]}"#);
    let (words, _, _, _) = card.look(&format!("emu:psp:{PSP_GAME}:1"), "en", &with_app);
    assert!(words.contains("This version of pocket-ui can't download this system. PPSSPP on this device plays it: choose it under Runs on."), "{words}");
}

#[test]
fn a_system_without_a_player_of_ours_plays_in_the_first_app_and_both_are_a_choice() {
    let card = Card::new();
    let words = card.words(&format!("emu:ps2:{PS2_GAME}"), "en", ODIN);
    assert!(words.contains("Runs on ‹ Default (ARMSX2) ›"), "{words}");
    assert!(!words.contains("Download") && !words.contains("pocket-ui player"), "{words}");
    assert!(words.contains("Another app starts the game and brings you back here."), "{words}");
    no_rocknix_words(&words);
}

#[test]
fn a_choice_the_mobile_view_made_is_the_choice_here() {
    let card = Card::new();
    // The system's app, and one game's own player while its system plays in an app.
    card.mobile_chose(r#"{"theme": "Big", "plays_with": {"psp": "org.ppsspp.ppsspp", "ps2": "xyz.aethersx2.android"}}"#);
    // A game's page: the console's choice is what it starts with without a choice of its own, named as the default.
    let words = card.words(&format!("emu:psp:{PSP_GAME}"), "en", ODIN);
    assert!(words.contains("Runs on ‹ Default (PPSSPP) ›") && !words.contains("Download"), "{words}");
    // The console's page: the choice itself, by the name the phone gave the app.
    let words = card.words("emu:psp", "en", ODIN);
    assert!(words.contains("Runs on ‹ PPSSPP ›") && !words.contains("Default (") && !words.contains("Download"), "{words}");
    let words = card.words("emu:ps2", "en", ODIN);
    assert!(words.contains("Runs on ‹ AetherSX2 ›"), "{words}");
    // An app that is gone from the phone is no choice: the system goes back to its own way.
    let words = card.words("emu:psp", "en", &ODIN.replace("\"org.ppsspp.ppsspp\"", "\"org.ppsspp.other\""));
    assert!(words.contains("Runs on ‹ Default (pocket-ui player) ›"), "{words}");
}

#[test]
fn every_row_is_reached_with_the_d_pad_and_the_hints_name_what_works() {
    let card = Card::new();
    for (row, name) in [(0, "Runs on"), (1, "Download"), (2, "Save changes")] {
        let (words, hints, focused, rows) = card.look(&format!("emu:psp:{PSP_GAME}:{row}"), "en", ODIN);
        assert_eq!(rows, ["Runs on", "Download", "Save changes"], "{words}");
        assert_eq!(focused.as_deref(), Some(name), "row {row}: {words}");
        let keys: Vec<&str> = hints.iter().map(|(k, _)| k.as_str()).collect();
        match row {
            // Runs on: A steps to the next way, Left/Right change it (PPSSPP is there); A stands last in the bar.
            0 => assert_eq!(keys, ["B", "< >", "A"], "{hints:?}"),
            _ => assert_eq!(keys, ["B", "A"], "{hints:?}"),
        }
    }
    // Where our core is on the phone there is no Download row to reach.
    let (_, _, _, rows) = card.look(&format!("emu:gba:{GBA_GAME}"), "en", ODIN);
    assert_eq!(rows, ["Runs on", "Save changes"]);
}

#[test]
fn a_on_a_game_whose_core_is_missing_opens_the_download_not_a_dead_end() {
    let card = Card::new();
    let (words, _, focused, _) = card.look(&format!("emu:psp:{PSP_GAME}:launch"), "en", ODIN);
    // The game did not start: its page is open on the Download row with the reason.
    assert_eq!(focused.as_deref(), Some("Download"), "{words}");
    assert!(words.contains("This system is not installed yet. Choose Download now to add it on Wi-Fi."), "{words}");
    // No app and no player of ours: which app it needs, as Mobile says it.
    let words = card.words(&format!("emu:ps2:{PS2_GAME}:launch"), "en", BARE);
    assert!(words.contains("PlayStation 2 games play in ARMSX2, which isn't installed."), "{words}");
}

#[test]
fn german_and_spanish_have_the_new_words() {
    let card = Card::new();
    for (lang, default, download, fragment) in [("de", "Standard (", "Laden", "noch nicht installiert"), ("es", "Por defecto (", "Descargar", "aún no está instalado")] {
        let words = card.words(&format!("emu:psp:{PSP_GAME}:1"), lang, ODIN);
        assert!(words.contains(download) && words.contains(fragment), "{lang}: {words}");
        let words = card.words(&format!("emu:psp:{PSP_GAME}"), lang, ODIN);
        assert!(words.contains(default) && !words.contains("pocket-ui player"), "{lang}: {words}");
    }
}

#[test]
fn the_games_page_names_the_phones_ways_on_its_runs_on_row() {
    let card = Card::new();
    // The last row of "This game" is Runs on: what it says is the phone's, not ROCKNIX's emulators.
    let words = card.words(&format!("this-game:psp:{PSP_GAME}:99"), "en", ODIN);
    assert!(words.contains("Runs on") && words.contains("in pocket-ui"), "{words}");
    assert!(words.contains("Which way plays this game: pocket-ui's own player or another emulator app on this device."), "{words}");
    assert!(!words.contains("pocket-core, RetroArch or another"), "{words}");
}

/// A game in our own player starts as on Linux (Lead, 13.11.2026: the ritual first), and since the 0.5 hand test
/// (15.11.2026) without the controls card at a console's first start: the one A goes on to the game. A game in another
/// app (ARMSX2) starts at once too.
#[test]
fn a_consoles_first_start_in_our_player_or_an_app_shows_no_controls_card() {
    let card = Card::new();
    let words = card.words(&format!("emu:gba:{GBA_GAME}:play"), "en", ODIN);
    assert!(!words.contains("Controls ·"), "no controls card before our player's game: {words}");
    let words = card.words(&format!("emu:ps2:{PS2_GAME}:play"), "en", ODIN);
    assert!(!words.contains("Controls ·"), "no card before another app's game: {words}");
}

const GC_GAME: &str = "F-Zero GX (USA).rvz";

/// GameCube and Wii play in the Dolphin app, which takes no start options for its pad, its touch buttons or its picture
/// (19.11.2026, a hand test on the Mini V2: a small picture, touch buttons over it, the pad not mapped, no way back).
/// A on the first such game shows a card with Dolphin's own setting names, "Start the game" selected; the second time
/// the game starts at once. The game's card keeps the card as a row.
#[test]
fn the_first_gamecube_game_in_dolphin_says_how_to_set_it_up_and_the_card_keeps_it() {
    let card = Card::new();
    let (words, hints, _, _) = card.look(&format!("emu:gamecube:{GC_GAME}:play"), "en", ODIN);
    assert!(words.contains("Setting up Dolphin"), "{words}");
    for step in ["GameCube Input", "Standard Controller", "Toggle Controls", "Aspect Ratio", "Exit Emulation", "Start the game", "Cancel"] {
        assert!(words.contains(step), "{step}: {words}");
    }
    assert!(hints.iter().any(|(k, _)| k == "A"), "A does something on the card: {hints:?}");
    // Once ever: the next A starts the game.
    let words = card.words(&format!("emu:gamecube:{GC_GAME}:play"), "en", ODIN);
    assert!(!words.contains("Setting up Dolphin"), "only the first time: {words}");
    // A game in our own player or in another app has none.
    let fresh = Card::new();
    for state in [format!("emu:gba:{GBA_GAME}:play"), format!("emu:ps2:{PS2_GAME}:play")] {
        let words = fresh.words(&state, "en", ODIN);
        assert!(!words.contains("Setting up Dolphin"), "{state}: {words}");
    }
    // The game's card has the row (for a game that plays in Dolphin only).
    // Down to the card's end (Hide from the shelf; the row before it is Dolphin's); stop once reached.
    let walk = |game: &str| {
        let mut out = String::new();
        for d in (0..=60).step_by(4) {
            let page = card.words(&format!("curator:card:{game}:{d}"), "en", ODIN);
            let done = page.contains("Hide from the shelf");
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&page);
            if done {
                break;
            }
        }
        out
    };
    let gc = walk("gamecube:f-zero");
    assert!(gc.contains("Hide from the shelf") && gc.contains("Setting up Dolphin") && gc.contains("Controller, touch buttons, picture, the way back"), "{gc}");
    let ps2 = walk("ps2:revenge");
    assert!(ps2.contains("Hide from the shelf") && !ps2.contains("Setting up Dolphin"), "{ps2}");
    // From the game's card at any time, with OK.
    let words = card.words("dolphin", "en", ODIN);
    assert!(words.contains("Setting up Dolphin") && words.contains("OK") && !words.contains("Start the game"), "{words}");
}

/// The card in every language: the words are there (no English left over in the steps) and nothing runs out of the screen
/// or into something else, on the Mini V2 and a 20:9 phone, Normal and Large text (`POCKETUI_TEXT_BOXES`).
#[test]
fn dolphin_s_card_fits_in_every_language() {
    let card = Card::new();
    let tmp = card.dir.path();
    for (lang, title) in [("en", "Setting up Dolphin"), ("de", "Dolphin einrichten"), ("es", "Configurar Dolphin"), ("pt", "Configurar o Dolphin"), ("fr", "Configurer Dolphin"), ("it", "Configurare Dolphin")] {
        for size in ["1240x1080", "2400x1080"] {
            for text in ["normal", "large"] {
                for state in ["dolphin-start:1", "dolphin"] {
                    let (png, words, boxes, phone) = (tmp.join("fit.png"), tmp.join("fit.tsv"), tmp.join("fit.boxes"), tmp.join("phone.json"));
                    std::fs::write(&phone, ODIN).unwrap();
                    let _ = std::fs::remove_file(&boxes);
                    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                    if tmp.join("wl").exists() {
                        cmd.env("WAYLAND_DISPLAY", tmp.join("wl"));
                    }
                    let out = cmd.args(["--snapshot", state, png.to_str().unwrap()])
                        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_ANDROID_PROFILE", "1")
                        .env("POCKETUI_LANG", lang).env("POCKETUI_TEXT_SIZE", text).env("POCKETUI_SNAPSHOT_SIZE", size)
                        .env("POCKETUI_VISIBLE_TEXT", &words).env("POCKETUI_TEXT_BOXES", &boxes).env("POCKETUI_PHONE", &phone)
                        .env("POCKETUI_DATA", tmp.join("data")).env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
                        .output().unwrap();
                    let name = format!("{state} {lang} {size} {text}");
                    assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
                    let seen = std::fs::read_to_string(&words).unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
                    assert!(seen.contains(title), "{name}: {seen}");
                    if lang != "en" {
                        for english in ["Set it up once", "Touch buttons off", "Back to the shelf"] {
                            assert!(!seen.contains(english), "{name}: English left: {english}: {seen}");
                        }
                    }
                    let bad: Vec<String> = std::fs::read_to_string(&boxes).unwrap_or_default().lines()
                        .filter(|l| l.starts_with(['E', 'O', 'V', 'W', 'T'])).map(str::to_string).collect();
                    assert!(bad.is_empty(), "{name}: {}", bad.join("\n"));
                }
            }
        }
    }
}
