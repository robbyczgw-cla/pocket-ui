//! The first-start shortcuts of 2026-11-02 (new-player walk): the real program paints each page at 640x480, the H700
//! handhelds' size, in English, German and Spanish, and nothing may run into the hints or off the edge.

use std::path::PathBuf;
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

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
        let dir = tempfile::Builder::new().prefix("pui-fs-").tempdir_in("/tmp").unwrap();
        let ok = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(ok.success());
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), dir.path().join("wl").to_str().unwrap()]).spawn().unwrap();
            for _ in 0..200 {
                if dir.path().join("wl").exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Stub(child)
        });
        Card { dir, _stub: stub }
    }

    /// What the state paints at `size` in `lang`: the words, one per line. Fails when the text painter found text running
    /// into other text or the hints, or cut by an edge.
    fn screen(&self, state: &str, lang: &str, size: &str) -> String {
        let tmp = self.dir.path();
        let (png, words, boxes): (PathBuf, PathBuf, PathBuf) = (tmp.join("shot.png"), tmp.join("words.tsv"), tmp.join("boxes.tsv"));
        let _ = std::fs::remove_file(&words);
        let _ = std::fs::remove_file(&boxes);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if tmp.join("wl").exists() {
            cmd.env("WAYLAND_DISPLAY", tmp.join("wl"));
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_VISIBLE_TEXT", &words)
            .env("POCKETUI_TEXT_BOXES", &boxes)
            .env("POCKETUI_DATA", tmp.join("data"))
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .output()
            .unwrap();
        assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        let problems = std::fs::read_to_string(&boxes).unwrap_or_default();
        assert!(problems.trim().is_empty(), "{state} {lang} {size}: text clipped or overlapping: {problems}");
        std::fs::read_to_string(words).unwrap_or_default()
    }
}

fn sees(screen: &str, what: &str) -> bool {
    screen.lines().any(|l| l.contains(what))
}

/// B: no Wi-Fi chip (RG28XX): Welcome, look, play style, time zone, games. The Wi-Fi and Extras pages are not there.
#[test]
fn a_handheld_without_wifi_counts_five_steps_and_goes_from_the_play_style_to_the_zone() {
    let card = Card::new();
    for (lang, zone, steps_of) in [("en", "Time zone", "Step 4 of 5"), ("de", "Zeitzone", "Schritt 4 von 5"), ("es", "Zona horaria", "Paso 4 de 5")] {
        // Welcome, A, A (the play style), A (Simple): the zone, not Wi-Fi.
        let page = card.screen("setup:first:aaa:device=rg28xx", lang, "640x480");
        assert!(sees(&page, zone) && sees(&page, steps_of), "{lang}: {page}");
        // A on the zone, then the games: the fifth step.
        let games = card.screen("setup:first:aaaa:device=rg28xx", lang, "640x480");
        assert!(games.contains(&steps_of.replace('4', "5")), "{lang}: {games}");
    }
    // A handheld with Wi-Fi still has seven.
    let page = card.screen("setup:first:aa:device=rg35xx-sp", "en", "640x480");
    assert!(sees(&page, "Step 3 of 7"), "{page}");
}

/// C: Kids Mode's page with no games offers the starter pack (X), in every language, without crowding the hints.
#[test]
fn the_kids_page_with_no_games_offers_the_starter_pack() {
    let card = Card::new();
    for (lang, label, none) in [("en", "Starter pack", "No games to pick yet"), ("de", "Startpaket", "Noch keine Spiele zum Aussuchen"), ("es", "Pack inicial", "Aún no hay juegos")] {
        let page = card.screen("kids-flow:games-empty", lang, "640x480");
        assert!(sees(&page, none), "{lang}: {page}");
        assert!(sees(&page, label), "{lang}: the starter pack is offered: {page}");
        assert!(sees(&page, "X"), "{lang}: {page}");
    }
}

/// D: the profile page opened from Settings: three answers, none "For a child", at 640x480 in all three languages.
#[test]
fn the_play_style_page_from_settings_has_three_answers_and_no_child_mode() {
    let card = Card::new();
    for (lang, names) in [("en", ["Simple", "Enthusiast", "Choose for yourself"]), ("de", ["Einfach", "Enthusiast", "Selbst wählen"]), ("es", ["Simple", "Entusiasta", "Elige tú"])] {
        let page = card.screen("setup:play-later", lang, "640x480");
        for name in names {
            assert!(sees(&page, name), "{lang} {name}: {page}");
        }
        assert!(!sees(&page, "For a child") && !sees(&page, "Für ein Kind") && !sees(&page, "Para un niño"), "{lang}: {page}");
    }
    // The row itself: first in Help & tools, with where the switches stand.
    let row = card.screen("settings:6:0", "en", "640x480");
    assert!(sees(&row, "How you like to play") && sees(&row, "Simple"), "{row}");
}

/// E: Home with no games, offline: the one button leads to Wi-Fi, and says what waits afterwards. Online: the pack.
#[test]
fn an_empty_home_offline_offers_wifi_and_online_the_pack() {
    let card = Card::new();
    for (lang, connect) in [("en", "Connect to Wi-Fi"), ("de", "Mit dem WLAN verbinden"), ("es", "Conectar a Wi-Fi")] {
        let page = card.screen("starter-home:wifi-off", lang, "640x480");
        assert!(sees(&page, connect), "{lang}: {page}");
        assert!(!sees(&page, "Get the starter pack") && !sees(&page, "Startpaket holen"), "{lang}: {page}");
    }
    let online = card.screen("starter-home", "en", "640x480");
    assert!(sees(&online, "Get the starter pack") && !sees(&online, "Connect to Wi-Fi"), "{online}");
}

/// F: Home with no games has a second action (Add games, X) and a way past the hint (B, Look around); the empty Home after B
/// says so and how to bring the hint back. In en, de and es, small and large screens, nothing clipped or overlapping.
#[test]
fn an_empty_home_offers_add_games_and_looking_around_in_every_language_and_size() {
    let card = Card::new();
    let words = [
        ("en", "Add games", "Look around", "No games yet", "Show the starter pack"),
        ("de", "Spiele hinzufügen", "Umsehen", "Noch keine Spiele", "Startpaket zeigen"),
        ("es", "Añadir juegos", "Echa un vistazo", "Aún sin juegos", "Mostrar el pack inicial"),
    ];
    for (lang, add, around, none, back) in words {
        for size in ["640x480", "800x600", "1024x768", "1280x800"] {
            for state in ["starter-home", "starter-home:wifi-off", "starter-home:busy"] {
                let hint = card.screen(state, lang, size);
                assert!(sees(&hint, add) && sees(&hint, around) && sees(&hint, none), "{state} {lang} {size}: {hint}");
            }
            let empty = card.screen("starter-home:around", lang, size);
            assert!(sees(&empty, none) && sees(&empty, back), "{lang} {size}: {empty}");
            assert!(!sees(&empty, around), "the hint is away: {empty}");
        }
    }
}

/// The password keyboard after a refused password: the hint for emptying the field is there, and nothing is clipped.
#[test]
fn the_password_keyboard_offers_to_empty_the_field() {
    let card = Card::new();
    for (lang, clear) in [("en", "Clear"), ("de", "Leeren"), ("es", "Limpiar")] {
        // One letter typed: L1 empties the field. Nothing typed: B is the way back.
        let page = card.screen("setup:wifi-refused:a", lang, "640x480");
        assert!(sees(&page, clear), "{lang}: {page}");
        let empty = card.screen("setup:wifi-refused", lang, "640x480");
        assert!(!sees(&empty, clear), "{lang}: {empty}");
    }
}


/// The starter pack's page over Kids Mode's picker says that B goes back to picking, not to a shelf.
#[test]
fn the_starter_page_in_kids_mode_says_where_b_goes() {
    let card = Card::new();
    for (lang, want, not) in [("en", "B goes back to pick games", "B shows your shelf"), ("de", "B geht zurück zur Auswahl", "B zeigt dein Regal"), ("es", "B vuelve a la selección", "B muestra tu estante")] {
        let kids = card.screen("starter:0:there-kids", lang, "640x480");
        assert!(sees(&kids, want) && !sees(&kids, not), "{lang}: {kids}");
        let plain = card.screen("starter:0:there", lang, "640x480");
        assert!(sees(&plain, not), "{lang}: {plain}");
    }
}

/// The first start's last page ("You're all set") names HOME in a plain line (a first game in RetroArch or another emulator has no line over
/// it), fits 640x480 in all three languages, and stays away in Kids Mode and in Settings' Add games.
#[test]
fn the_last_page_of_the_first_start_says_what_home_does() {
    let card = Card::new();
    for (lang, line) in [
        ("en", "In a game, HOME opens the menu: save, load, quit."),
        ("de", "Im Spiel öffnet HOME das Menü: Speichern, Laden, Beenden."),
        ("es", "En un juego, HOME abre el menú: guardar, cargar, salir."),
    ] {
        // Welcome, look, play style, Simple, time zone, games (START): "You're all set" on a handheld without Wi-Fi.
        let page = card.screen("setup:first:aaaas:device=rg28xx", lang, "640x480");
        assert!(sees(&page, line), "{lang}: {page}");
        // Settings' Add games is not the first start.
        assert!(!sees(&card.screen("setup:games", lang, "640x480"), line), "{lang}: Settings");
    }
    let kids = Card::new();
    std::fs::create_dir_all(kids.dir.path().join("data")).unwrap();
    std::fs::write(kids.dir.path().join("data/kids.json"), br#"{"on":true}"#).unwrap();
    let page = kids.screen("setup:first:aaaas:device=rg28xx", "en", "640x480");
    assert!(!sees(&page, "HOME"), "Kids Mode: {page}");
}
