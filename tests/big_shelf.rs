//! The Easy look ("Big" in the code) as a player meets it: the real program paints each state, and the words on the
//! screen say what is there (docs/BIG-SHELF.md). The moves are `pocket_ui::big_shelf`'s tests.

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

struct Shown {
    stdout: String,
    words: String,
}

impl Card {
    fn new() -> Card {
        // A short path: the stub's socket has to fit.
        let dir = tempfile::Builder::new().prefix("pui-big-").tempdir_in("/tmp").unwrap();
        let script = format!("{ROOT}/scripts/curator-fake-library.py");
        let status = Command::new("python3").args(["-I", &script, dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(status.success());
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), dir.path().join("wl").to_str().unwrap()]).spawn().unwrap();
            test_clock::wait_for(|| dir.path().join("wl").exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
            Stub(child)
        });
        Card { dir, _stub: stub }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn show_at(&self, state: &str, size: &str, text: &str, lang: &str) -> Shown {
        let tmp = self.path();
        let (png, words) = (tmp.join("shot.png"), tmp.join("words.tsv"));
        let _ = std::fs::remove_file(&words);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if tmp.join("wl").exists() {
            cmd.env("WAYLAND_DISPLAY", tmp.join("wl"));
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", lang)
            // A file there: the games route to pocket-core, as on a device.
            .env("POCKETUI_HOST_BIN", "/bin/sh")
            .env("POCKETUI_VISIBLE_TEXT", &words)
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_TEXT_SIZE", text)
            .env("POCKETUI_DATA", tmp.join("data"))
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .output()
            .unwrap();
        assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        Shown { stdout: String::from_utf8_lossy(&out.stdout).into_owned(), words: std::fs::read_to_string(words).unwrap_or_default() }
    }

    fn show(&self, state: &str) -> Shown {
        self.show_at(state, "640x480", "normal", "en")
    }

    /// A card with exactly these games: `n` of them over `systems` systems, nothing played, nothing starred.
    fn with_games(n: usize, systems: &[&str]) -> Card {
        let card = Card::new();
        let roms = card.path().join("roms");
        std::fs::remove_dir_all(&roms).unwrap();
        for system in systems {
            std::fs::create_dir_all(roms.join(system)).unwrap();
        }
        for i in 0..n {
            let system = systems[i % systems.len()];
            let ext = if system == "gba" { "gba" } else { "sfc" };
            std::fs::write(roms.join(system).join(format!("Test Game {i:02}.{ext}")), b"x").unwrap();
        }
        std::fs::write(card.path().join("data/recents.json"), "[]").unwrap();
        std::fs::write(card.path().join("data/favorites.json"), "[]").unwrap();
        card
    }

    fn play(&self, system: &str, name: &str, file: &str) {
        let path = self.path().join("roms").join(system).join(file);
        let recents = serde_json::json!([{"system": system, "name": name, "path": path, "last_played": 1_790_000_000u64, "play_secs": 600}]);
        std::fs::write(self.path().join("data/recents.json"), recents.to_string()).unwrap();
    }
}

fn games_shown(stdout: &str) -> usize {
    let at = stdout.find("games=").expect("the state says how many games") + "games=".len();
    stdout[at..].split_whitespace().next().unwrap().parse().unwrap()
}

#[test]
fn zero_one_three_twenty_and_twenty_one_games() {
    let empty = Card::with_games(0, &["gba"]);
    assert!(empty.show("big-shelf").words.contains("No games yet"));
    for (n, systems) in [(1, vec!["gba"]), (3, vec!["gba"]), (3, vec!["gba", "snes"]), (20, vec!["gba", "snes"]), (21, vec!["gba", "snes"]), (21, vec!["gba"])] {
        let card = Card::with_games(n, &systems);
        let shelf = card.show("big-shelf");
        assert_eq!(games_shown(&shelf.stdout), n);
        let w = &shelf.words;
        // Easy shows three hints: A, HOME and START; Details, Favourite and the jump are in the tour and the manual.
        assert!(w.contains("Test Game") && w.contains("Settings") && w.contains("Quick settings"), "{n}: {w}");
        assert!(!w.contains("Details") && !w.contains("Favourite") && !w.contains("Systems"), "{n}: three hints at most: {w}");
        assert_eq!(w.contains(&format!("1 of {n}")), n > 1, "the place is always there, except with one game: {w}");
        assert!(w.contains("Game Boy Advance") || w.contains("Super Nintendo"), "the game's system in the top line: {w}");
        assert!(!w.contains("Recently played") && !w.contains("Keep playing"), "{w}");
    }
}

#[test]
fn it_opens_on_the_game_played_last_and_playing_moves_nothing() {
    let card = Card::with_games(6, &["gba", "snes"]);
    let before = card.show("big-shelf:3").words;
    card.play("snes", "Test Game 03", "Test Game 03.sfc");
    let opens = card.show("big-shelf").words;
    assert!(opens.contains("Test Game 03") && opens.contains("Super Nintendo"), "{opens}");
    // Also when it is the saved look, through the start's own first list (seen on a device: the first game).
    let file = card.path().join("data/settings.json");
    let mut settings: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    settings["theme"] = serde_json::json!("big");
    std::fs::write(&file, settings.to_string()).unwrap();
    let opens = card.show("big-shelf").words;
    assert!(opens.contains("Test Game 03"), "{opens}");
    let after = card.show("big-shelf:3").words;
    let title = |w: &str| w.lines().find(|l| l.contains("Test Game")).map(str::to_string);
    assert_eq!(title(&before), title(&after), "the same game in the same place");
}

#[test]
fn favourites_stand_first_with_no_number_and_the_systems_count_in_their_order() {
    let card = Card::with_games(4, &["gba", "snes"]);
    let path = card.path().join("roms/snes/Test Game 03.sfc");
    let favourites = serde_json::json!([{"system": "snes", "name": "Test Game 03", "path": path}]);
    std::fs::write(card.path().join("data/favorites.json"), favourites.to_string()).unwrap();
    let front = card.show("big-shelf:0");
    assert_eq!(games_shown(&front.stdout), 5, "the favourite in front, and on its system: {}", front.stdout);
    assert!(front.words.contains("Test Game 03") && front.words.contains("Favourites"), "{}", front.words);
    assert!(!front.words.contains(" of "), "no place in the count there: {}", front.words);
    // The last place is the last number, not the favourite's 1 again.
    let last = card.show("big-shelf:4");
    assert!(last.words.contains("Test Game 03") && last.words.contains("4 of 4") && !last.words.contains("Favourites"), "{}", last.words);
    let first = card.show("big-shelf:1");
    assert!(first.words.contains("1 of 4"), "{}", first.words);
}

#[test]
fn resume_only_where_a_moment_is_kept() {
    let card = Card::with_games(3, &["gba"]);
    card.play("gba", "Test Game 00", "Test Game 00.gba");
    let shelf = card.show("big-shelf");
    assert!(shelf.words.contains("Play") && !shelf.words.contains("Continue"), "played before, no moment: {}", shelf.words);
    let frozen = card.show("big-frozen");
    assert!(frozen.words.contains("Continue"), "{}", frozen.words);
    let kids = card.show("kids-shelf");
    assert!(!kids.words.contains("Continue"), "Kids Mode's words stay: {}", kids.words);
}

/// First try on a device: the case that stands open "sometimes" was read as a bug. One line says what open means, the first
/// time one is on screen, once ever, in every language, never in Kids Mode.
#[test]
fn an_open_case_is_explained_once_ever_and_not_to_a_child() {
    let card = Card::with_games(3, &["gba"]);
    card.play("gba", "Test Game 00", "Test Game 00.gba");
    let line = [("en", "Open means your moment is waiting"), ("de", "Offen heißt: Dein Moment wartet"), ("es", "Abierta significa que tu momento te espera")];
    for (lang, said) in line {
        let open = card.show_at("big-frozen", "640x480", "normal", lang);
        assert!(open.words.contains(said), "{lang}: {}", open.words);
        let closed = card.show_at("big-shelf", "640x480", "normal", lang);
        assert!(!closed.words.contains(said), "{lang}: a closed case needs no words: {}", closed.words);
    }
    // At 1920x1080 and with large text it still fits.
    for (size, text) in [("1920x1080", "normal"), ("640x480", "large")] {
        let open = card.show_at("big-frozen", size, text, "de");
        assert!(open.words.contains("Offen heißt: Dein Moment wartet"), "{size} {text}: {}", open.words);
    }
    let kids = card.show("kids-shelf");
    assert!(!kids.words.contains("Open means"), "{}", kids.words);
    // Told once: the setting is kept and the line stays away.
    let file = card.path().join("data/settings.json");
    let mut settings: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap_or_else(|_| b"{}".to_vec())).unwrap();
    settings["open_case_told"] = serde_json::json!(true);
    std::fs::write(&file, settings.to_string()).unwrap();
    let again = card.show("big-frozen");
    assert!(again.words.contains("Continue") && !again.words.contains("Open means"), "{}", again.words);
}

/// Under the name: system · year · players, a line of what the game is, time and last played; a line with nothing
/// known is left out, and the open-case line stays under them.
#[test]
fn the_info_under_the_name_shows_what_is_known_and_leaves_out_the_rest() {
    let card = Card::new();
    for (size, text) in [("640x480", "normal"), ("1024x768", "normal"), ("1240x1080", "normal"), ("1920x1080", "normal"), ("640x480", "large")] {
        let shown = card.show_at("big-frozen", size, text, "en");
        let w = &shown.words;
        for part in ["Gran Turismo 4", "PlayStation 2", "2004", "1 player or more", "A 2004 racing game", "Played 30 h 00 min", "2 h ago", "Open means your moment is waiting"] {
            assert!(w.contains(part), "{size} {text}: {part}: {w}");
        }
    }
    let de = card.show_at("big-frozen", "640x480", "normal", "de").words;
    for part in ["PlayStation 2", "2004", "Rennspiel", "Gespielt 30 h 00 min", "vor 2 h"] {
        assert!(de.contains(part), "de: {part}: {de}");
    }
    let es = card.show_at("big-frozen", "640x480", "normal", "es").words;
    for part in ["Carreras", "Jugado 30 h 00 min", "hace 2 h"] {
        assert!(es.contains(part), "es: {part}: {es}");
    }
    // A game that nothing is known of and nobody played: only its system, no "Played", no gap-making empty lines.
    let bare = Card::with_games(3, &["snes"]);
    let w = bare.show("big-shelf").words;
    assert!(w.contains("Super Nintendo") && !w.contains("Played") && !w.contains(" ago"), "{w}");
}

#[test]
fn the_pause_says_what_quit_does_on_each_route_and_names_no_emulator() {
    let card = Card::new();
    for (state, button, line) in [
        ("big-pause", "Save & quit", "Next time, A brings you back"),
        ("big-pause:stop", "Save & quit", "Next time, A brings you back"),
        ("big-pause:retroarch", "Save & quit", "Next time, A brings you back"),
        ("big-pause:forgets", "Quit", "Doesn't save where you are. In-game saves are kept."),
        ("big-pause:port", "Quit", "keeps only what you save"),
        ("big-pause:port-asked", "Quit", "Have you saved in the game?"),
    ] {
        let shown = card.show(state);
        for words in ["Continue", button, line, "More: saves, brightness, options"] {
            assert!(shown.words.contains(words), "{state} says {words:?}: {}", shown.words);
        }
        assert!(!shown.words.contains("RetroArch") && !shown.words.contains("Stop"), "{state}: {}", shown.words);
    }
    assert!(!card.show("big-pause:port").words.contains("Save & quit"));
    let normal = card.show("big-pause:more-open");
    for words in ["Options", "Quick settings", "Save & quit"] {
        assert!(normal.words.contains(words), "{words}: {}", normal.words);
    }
    // No game switcher in this look: nothing names SELECT or "Games" on the normal menu behind More; the other looks keep it.
    assert!(!normal.words.contains("SELECT") && !normal.words.contains("Games"), "{}", normal.words);
    assert!(card.show("menu:pocket-core").words.contains("SELECT"), "the normal menu keeps its Games hint");
    let child = card.show("kids-pause");
    assert!(child.words.contains("Stop") && child.words.contains("It saves by itself") && !child.words.contains("More"), "{}", child.words);
}

/// X is the normal game card of the other looks, with everything it has; B comes back to the shelf.
#[test]
fn x_opens_the_normal_game_card() {
    let card = Card::new();
    let w = card.show("big-details").words;
    for part in ["Gran Turismo 4", "Developer", "Publisher", "Genre", "Series", "Played", "Rating", "Status", "Back"] {
        assert!(w.contains(part), "{part}: {w}");
    }
}

#[test]
fn a_failed_start_stays_with_one_thing_to_do_and_no_emulator_on_it() {
    let card = Card::new();
    for (state, action) in [("big-failed:2", "Game check"), ("big-failed:2:port", "Game check"), ("big-failed:2:core", "Game check"), ("big-failed:2:bios", "Add games")] {
        let shown = card.show(state);
        let w = &shown.words;
        assert!(w.contains("This game didn't start") && w.contains("Some Game (USA).chd") && w.contains("Back") && w.contains(action), "{state}: {w}");
        assert!(!w.lines().any(|l| l == "Play" || l == "Details"), "not the shelf under it: {w}");
        for gone in ["RetroArch", "Flatpak", "log.txt", "Settings ›"] {
            assert!(!w.contains(gone), "{state}: {gone}: {w}");
        }
    }
    assert!(card.show("big-failed:2:bios").words.contains("scph5501.bin"));
}

#[test]
fn the_hints_teach_quick_settings_and_never_show_a_key_without_its_word_at_640x480() {
    let card = Card::new();
    for (lang, b, settings) in [("en", "Quick settings", "Settings"), ("de", "Schnellmenü", "Einstellungen"), ("es", "Ajustes rápidos", "Ajustes")] {
        let shelf = card.show_at("big-shelf", "640x480", "normal", lang);
        // Easy shows three hints, each with its word: A, HOME (the quick settings) and START. Favourite, Details and the jump
        // between systems are in the tour and the manual, so no key ever stands without its word.
        for word in [b, settings] {
            assert!(shelf.words.contains(word), "{lang}: {word}: {}", shelf.words);
        }
        assert!(shelf.words.contains("HOME") && !shelf.words.lines().any(|l| l == "B" || l == "X" || l == "Y"), "{lang}: {}", shelf.words);
        let pause = card.show_at("big-pause", "640x480", "normal", lang);
        assert!(pause.words.contains("START") && pause.words.contains(b), "{lang}: {}", pause.words);
    }
    assert!(!card.show("kids-pause").words.contains("Quick settings"), "Kids Mode's pause stays as it is");
}

#[test]
fn the_look_step_has_just_play_second_and_says_when_a_card_is_big() {
    let set = |card: &Card| {
        let file = card.path().join("data/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        settings["theme"] = serde_json::json!("big");
        std::fs::write(&file, settings.to_string()).unwrap();
    };
    let many = Card::new();
    set(&many);
    let w = many.show("setup:look").words;
    assert!(w.contains("Easy") && w.contains("Best with a handful of games"), "{w}");
    let few = Card::with_games(30, &["gba"]);
    set(&few);
    let w = few.show("setup:look").words;
    assert!(w.contains("One big game at a time, simple menus.") && !w.contains("Best with"), "{w}");
}

/// Seen on the Odin: coming back from the card the shelf flashed the neighbour case's colour.
#[test]
fn a_page_transition_fades_from_the_background_not_from_a_neighbour() {
    let card = Card::new();
    let shown = card.show("big-back:1");
    let hex = |key: &str| {
        let at = shown.stdout.find(key).unwrap_or_else(|| panic!("{}", shown.stdout)) + key.len();
        u32::from_str_radix(&shown.stdout[at..at + 6], 16).unwrap()
    };
    let (bg, side) = (hex("bg="), hex("neighbour-side="));
    let spread = |c: u32| { let (r, g, b) = (c >> 16 & 255, c >> 8 & 255, c & 255); r.max(g).max(b) - r.min(g).min(b) };
    assert!(spread(bg) < 12, "a grey of the page, not a case's colour: {bg:06x}");
    assert_ne!(bg, side, "a case stands on that pixel, which is why it can't be the page's colour");
}

#[test]
fn the_first_start_offers_the_shelf_then_just_play_and_all_five_rows_fit() {
    let card = Card::new();
    for (lang, shelf, just, enth, choose, child) in [("en", "The shelf", "Easy", "Enthusiast", "Choose for yourself", "For a child"), ("de", "Das Regal", "Easy", "Enthusiast", "Selbst wählen", "Für ein Kind"), ("es", "El estante", "Easy", "Entusiasta", "Elige tú", "Para un niño")] {
        for text in ["normal", "large"] {
            for state in ["setup:play", "setup:play:d"] {
                let w = card.show_at(state, "640x480", text, lang).words;
                let at: Vec<usize> = [shelf, just, enth, choose, child].iter().map(|r| w.find(r).unwrap_or_else(|| panic!("{lang} {text}: {r}: {w}"))).collect();
                assert!(at.windows(2).all(|p| p[0] < p[1]), "{lang} {text}: the order: {w}");
                assert!(!w.contains('…'), "{lang} {text}: no description cut: {w}");
            }
        }
    }
}

/// Found on a device, where covers come from the thread and a snapshot's are read at once: the case stayed blank.
#[test]
fn a_cover_read_on_the_thread_reaches_the_big_case() {
    let card = Card::new();
    let recents: serde_json::Value = serde_json::from_slice(&std::fs::read(card.path().join("data/recents.json")).unwrap()).unwrap();
    let first = &recents[0];
    let stem = Path::new(first["path"].as_str().unwrap()).file_stem().unwrap().to_str().unwrap().to_string();
    let dir = card.path().join("data/covers").join(first["system"].as_str().unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(Path::new(&ROOT).join("assets/media/3ds-front.png"), dir.join(format!("{stem}.png"))).unwrap();
    let shown = card.show("big-thread");
    assert!(shown.stdout.contains("big-cover ready=true"), "{}", shown.stdout);
}

/// It began as a switch: a saved one becomes the look; the other looks stay as they are.
#[test]
fn just_play_is_a_look_and_the_old_switch_becomes_it() {
    let card = Card::new();
    let file = card.path().join("data/settings.json");
    let mut settings: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    settings["big_shelf"] = serde_json::json!(true);
    std::fs::write(&file, settings.to_string()).unwrap();
    assert!(card.show("home").words.contains("Quick settings"), "the Easy look shows HOME for the quick settings");
    settings["big_shelf"] = serde_json::json!(false);
    settings["theme"] = serde_json::json!("big");
    std::fs::write(&file, settings.to_string()).unwrap();
    assert!(card.show("home").words.contains("Quick settings"), "the Easy look shows HOME for the quick settings");
    settings["theme"] = serde_json::json!("shelf");
    std::fs::write(&file, settings.to_string()).unwrap();
    assert!(!card.show("home").words.contains("Systems"), "the other looks as ever");
}

#[test]
fn resume_stands_on_the_case_where_a_moment_is_kept() {
    let card = Card::with_games(3, &["gba"]);
    let open = card.show("big-frozen");
    assert_eq!(open.words.matches("Continue").count(), 2, "on the case and on A: {}", open.words);
    let closed = card.show("big-shelf");
    assert!(!closed.words.contains("Continue"), "{}", closed.words);
}

/// Two games of one file name share their states: no Resume for a moment that may be the other's, and the note about it fits.
#[test]
fn the_twin_note_fits_640x480_and_a_shared_moment_is_no_resume() {
    let card = Card::with_games(0, &["gba"]);
    for dir in ["one", "two"] {
        std::fs::create_dir_all(card.path().join("roms/gba").join(dir)).unwrap();
        std::fs::write(card.path().join("roms/gba").join(dir).join("Same Game.gba"), b"x").unwrap();
    }
    std::fs::write(card.path().join("roms/gba/Other Game.gba"), b"x").unwrap();
    for lang in ["en", "de", "es"] {
        let shown = card.show_at("big-twin", "640x480", "normal", lang);
        let at = shown.stdout.find("candidates=").unwrap_or_else(|| panic!("{}", shown.stdout)) + "candidates=".len();
        let candidates: usize = shown.stdout[at..].split_whitespace().next().unwrap().parse().unwrap();
        assert_eq!((games_shown(&shown.stdout), candidates), (3, 1), "only the game of its own name may resume: {}", shown.stdout);
        let at = shown.stdout.find("pill=").unwrap() + "pill=".len();
        let (w, h) = shown.stdout[at..].split_whitespace().next().unwrap().split_once('x').unwrap();
        let (w, h): (i32, i32) = (w.parse().unwrap(), h.parse().unwrap());
        assert!(w <= 640 - 48 && h <= 240, "{lang}: on a 640x480 panel, {w}x{h}");
        assert!(!shown.words.contains('…'), "{lang}: the sentence whole: {}", shown.words);
    }
}

#[test]
fn it_fits_640x480_with_large_text_in_german_and_spanish() {
    let card = Card::new();
    for (lang, settings, more, quit) in [("de", "Einstellungen", "Mehr:", "Sichern & Ende"), ("es", "Ajustes", "Más:", "Guardar y salir")] {
        for text in ["normal", "large"] {
            let shelf = card.show_at("big-shelf", "640x480", text, lang);
            assert!(shelf.words.contains("START"), "the way out's key stays: {lang} {text} {}", shelf.words);
            if text == "normal" {
                assert!(shelf.words.contains(settings), "every label fits: {lang}: {}", shelf.words);
            }
            let pause = card.show_at("big-pause", "640x480", text, lang);
            assert!(pause.words.contains(more) && pause.words.contains(quit), "{lang} {text}: {}", pause.words);
            assert!(!pause.words.contains('…'), "no line cut: {lang} {text}: {}", pause.words);
            let asked = card.show_at("big-pause:port-asked", "640x480", text, lang);
            assert!(!asked.words.contains('…'), "no line cut: {lang} {text}: {}", asked.words);
        }
    }
}

/// Not what it is for, and it says so in the look step; it must still not break.
#[test]
fn eight_hundred_games_dont_break_it() {
    let card = Card::with_games(800, &["gba", "snes"]);
    let far = card.show("big-shelf:700");
    assert_eq!(games_shown(&far.stdout), 800);
    assert!(far.words.contains("701 of 800"), "{}", far.words);
}
