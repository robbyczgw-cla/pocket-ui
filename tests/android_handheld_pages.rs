//! Android's handheld view (2026-11-13, HANDHELD-ROUGH-EDGES): the About page and a game's "This game" page show only what
//! Android has. The real program paints each page in Android's profile (`POCKETUI_ANDROID_PROFILE=1`, the Odin 2 Mini's size);
//! the words on the screen are read back, as the other page tests do.

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

impl Card {
    fn new() -> Card {
        let dir = tempfile::Builder::new().prefix("pui-ahp-").tempdir().unwrap();
        let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(ok.success());
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), dir.path().join("wl").to_str().unwrap()]).spawn().unwrap();
            test_clock::wait_for(|| dir.path().join("wl").exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
            Stub(child)
        });
        Card { dir, _stub: stub }
    }

    /// The words the state paints on the Odin 2 Mini's screen, one per line; `android` puts the program in Android's profile.
    fn screen(&self, state: &str, android: bool) -> String {
        self.screen_in(state, android, "en")
    }

    /// [`Card::screen`] with the menus in `lang`.
    fn screen_in(&self, state: &str, android: bool, lang: &str) -> String {
        let tmp = self.dir.path();
        let (png, words) = (tmp.join("shot.png"), tmp.join("words.tsv"));
        let _ = std::fs::remove_file(&words);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if tmp.join("wl").exists() {
            cmd.env("WAYLAND_DISPLAY", tmp.join("wl"));
        }
        if android {
            cmd.env("POCKETUI_ANDROID_PROFILE", "1");
        }
        let out = cmd
            .args(["--snapshot", &format!("{state}:device=odin2mini"), png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_VISIBLE_TEXT", &words)
            .env("POCKETUI_DATA", tmp.join("data"))
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .output()
            .unwrap();
        assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        std::fs::read_to_string(words).unwrap_or_default()
    }
}

fn sees(screen: &str, what: &str) -> bool {
    screen.lines().any(|l| l.contains(what))
}

/// About on Android: the version and the games, no Linux facts (the address of a Wi-Fi Android owns, the logs' and the
/// licences' folders of a Linux card), no report, no EmulationStation. Linux's About keeps them all.
#[test]
fn about_shows_what_android_has_and_not_a_linux_card() {
    let card = Card::new();
    let page = card.screen("about", true);
    for want in ["Version", "Games"] {
        assert!(sees(&page, want), "{want} is missing: {page}");
    }
    for gone in ["Network", "Not connected", "Connected,", "Logs", "Licences", "Report a problem", "EmulationStation", "/storage"] {
        assert!(!sees(&page, gone), "{gone} on Android's About: {page}");
    }
    let linux = card.screen("about", false);
    for want in ["Network", "Logs", "Report a problem", "EmulationStation"] {
        assert!(sees(&linux, want), "{want} is missing on Linux: {linux}");
    }
}

/// "This game" on Android: the picture rows, Controls and Runs on; none of the pocket-core rows the Android player has no
/// switch for (Rewind, Earlier states, the shortcuts ...). Linux's page keeps them.
#[test]
fn this_game_shows_only_the_rows_the_android_player_has() {
    let card = Card::new();
    let page = card.screen("this-game:gba", true);
    for gone in ["Rewind", "Earlier states", "Rumble", "Fast-forward", "Fan", "Performance", "Run-ahead", "Screen orientation", "Over the game", "Shortcut:", "runs on pocket-core"] {
        assert!(!sees(&page, gone), "{gone} on Android's This game: {page}");
    }
    for want in ["Screen scaling", "Screen frame", "Controls", "Runs on"] {
        assert!(sees(&page, want), "{want} is missing: {page}");
    }
    // Linux's list scrolls to its end (row 99 is clamped to the last): its shortcuts are there.
    let linux = card.screen("this-game:gba:99", false);
    for want in ["Shortcut:", "Controls", "Runs on"] {
        assert!(sees(&linux, want), "{want} is missing on Linux: {linux}");
    }
}

/// The picture rows on Android: Screen scaling, Look and Screen frame only (the player's four effects are the Look's four values,
/// so Custom's Sharpness and Effect rows are not there), and the frame row's words are the player's, not Linux's Auto/Ambient/Glow.
#[test]
fn this_game_s_picture_rows_are_the_players() {
    let card = Card::new();
    let page = card.screen("this-game:gba", true);
    for gone in ["Sharpness", "Effect", "LCD colours", "GBC colour", "Ambient", "Glow", "Woodgrain", "Integer, smaller"] {
        assert!(!sees(&page, gone), "{gone} on Android's This game: {page}");
    }
    for want in ["Screen scaling", "Look", "Screen frame"] {
        assert!(sees(&page, want), "{want} is missing: {page}");
    }
}

/// Help & tools on Android: the first start's line names the steps Android has (no Wi-Fi), About's line no network. Linux's names them.
#[test]
fn help_and_tools_lines_match_what_android_has() {
    let card = Card::new();
    // First-start setup, the fourth row of Help & tools (category 7) on both (Manual, Tour, How you like to play before it).
    let page = card.screen("settings:7:3", true);
    assert!(sees(&page, "First-start setup"), "{page}");
    assert!(!sees(&page, "Wi-Fi"), "Wi-Fi on Android's Help & tools: {page}");
    let linux = card.screen("settings:7:3", false);
    assert!(sees(&linux, "Wi-Fi"), "Linux's line names Wi-Fi: {linux}");
}

/// About and its row in Settings say the APK's version (`android:versionName`, what Android's app info says), not the library's
/// Cargo version with "-dev (unknown)" (the Odin 2 Mini's About said "pocket-ui 0.2.0-dev (unknown)" on 0.4.0-android.3, 13.11.).
#[test]
fn about_says_the_apk_s_version() {
    let manifest = std::fs::read_to_string(format!("{ROOT}/android/AndroidManifest.xml")).unwrap();
    let apk = manifest.split("android:versionName=\"").nth(1).and_then(|r| r.split('"').next()).unwrap();
    assert!(apk.contains("-android."), "{apk}");
    let card = Card::new();
    let page = card.screen("about", true);
    assert!(sees(&page, &format!("pocket-ui {apk}")), "About on Android doesn't say {apk}: {page}");
    assert!(!sees(&page, "-dev") && !sees(&page, "unknown)"), "{page}");
    let linux = card.screen("about", false);
    assert!(!sees(&linux, apk), "Linux's About says the APK's version: {linux}");
}

/// Settings › System › Updates (android.9) and its row say the APK's version too, the one Android's list offers by: the row
/// said "0.2.0", the library's Cargo version, in the emulator (17.11.2026).
#[test]
fn updates_and_its_row_say_the_apk_s_version() {
    let manifest = std::fs::read_to_string(format!("{ROOT}/android/AndroidManifest.xml")).unwrap();
    let apk = manifest.split("android:versionName=\"").nth(1).and_then(|r| r.split('"').next()).unwrap();
    let card = Card::new();
    let page = card.screen("updates", true);
    assert!(sees(&page, apk), "Updates on Android doesn't say {apk}: {page}");
    let system = card.screen("settings:6", true);
    assert!(sees(&system, "Updates") && sees(&system, apk), "the Updates row in System doesn't say {apk}: {system}");
    let linux = card.screen("settings:6", false);
    assert!(!sees(&linux, apk), "Linux's Updates row says the APK's version: {linux}");
}

/// The Linux handheld's words that the pages below must not show on Android (the class test over every text is
/// `android_pages_name_nothing_only_linux_has` in app_settings_groups; this one sees the real program paint them).
const LINUX_WORDS: &[&str] = &["ROCKNIX", "pocket-core", "SD card", "SD-Karte", "tarjeta SD", "Wi-Fi", "WLAN", "Bluetooth", "Lüfter", "ventilador", "Vol+", "Pocket ", "rewind", "Zoom in the pause menu"];

/// The pages Android's handheld view paints where the Odin 2 Mini showed Linux's words (13.11.): `(snapshot state, a phrase
/// only Android's text has)`. Each in English, German and Spanish.
const ANDROID_SURFACES: &[(&str, &str)] = &[
    ("tour:quick", "HOME"),
    ("tour:add_games", "ROMs"),
    ("tour:easy_to_read", ""),
    ("sys:quick:0", ""),
    ("sys:quick:1", ""),
    ("manual:0", "Android"),
    ("manual:1", ""),
    ("manual:2", ""),
    ("achievements", "pocket-ui"),
];

#[test]
fn android_pages_paint_no_linux_words() {
    let card = Card::new();
    let mut found = Vec::new();
    for (state, own) in ANDROID_SURFACES {
        for lang in ["en", "de", "es", "pt", "fr", "it"] {
            let page = card.screen_in(state, true, lang);
            assert!(!page.trim().is_empty(), "{state} ({lang}) painted no words");
            if !own.is_empty() {
                assert!(sees(&page, own), "{state} ({lang}) lacks {own:?}: {page}");
            }
            for word in LINUX_WORDS.iter().filter(|w| sees(&page, w)) {
                found.push(format!("{state} ({lang}): {word:?} in {page}"));
            }
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
    // Linux's Tour card and quick settings keep their words.
    assert!(sees(&card.screen("tour:quick", false), "Wi-Fi"));
    assert!(sees(&card.screen("manual:0", false), "ROCKNIX"));
}
