//! A card with one damaged system must still have the other games on its shelf.
use pocket_ui::library;
use std::path::Path;

fn entry(root: &Path, id: &str, title: &str) -> String {
    let folder = root.join(id);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("Test.nes"), b"game").unwrap();
    format!("<system><name>{id}</name><fullname>{title}</fullname><path>{}</path><extension>.nes</extension></system>", folder.display())
}

#[test]
fn raw_ampersand_keeps_every_system_and_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("es_systems.cfg");
    let xml = format!("<systemList>{}{}{}</systemList>", entry(dir.path(), "nes", "Nintendo"), entry(dir.path(), "gameandwatch", "Game & Watch"), entry(dir.path(), "snes", "Super Nintendo"));
    assert!(roxmltree::Document::parse(&xml).is_err(), "the old whole-file parser rejects the card");
    std::fs::write(&cfg, xml).unwrap();
    let shelf = library::load(&cfg).unwrap();
    assert_eq!(shelf.len(), 3);
    assert_eq!(shelf.iter().find(|s| s.name == "gameandwatch").unwrap().fullname, "Game & Watch");
    assert_eq!(library::all_systems(&cfg).unwrap().len(), 3);
    assert!(library::system_list_issues(&cfg).unwrap().is_empty(), "a recovered spelling needs no warning");
}

#[test]
fn one_broken_entry_keeps_games_before_and_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("es_systems.cfg");
    for broken in ["<system><name>bad</name><fullname>Broken</path></system>", "<system><name>bad</name>", "<system><name>bad</name></system>", "<system><name></name><path>/bad</path></system>"] {
        std::fs::write(&cfg, format!("<systemList>{}{}{}</systemList>", entry(dir.path(), "nes", "Nintendo"), broken, entry(dir.path(), "snes", "Super Nintendo"))).unwrap();
        let shelf = library::load(&cfg).unwrap();
        assert_eq!(shelf.len(), 2, "{broken}");
        assert_eq!(library::all_systems(&cfg).unwrap().len(), 2, "the same systems in Add games and Game check");
        let issues = library::system_list_issues(&cfg).unwrap();
        assert_eq!(issues.len(), 1, "{broken}");
        let mut report = pocket_ui::game_check::Report::default();
        report.note_system_list(&cfg);
        assert!(!report.all_good(), "Game check must not say All good for a skipped system");
        assert_eq!(report.findings[0].place.as_deref(), Some(format!("{}:1", cfg.display()).as_str()));
        assert!(report.findings[0].what.contains("left out"));
    }
}

#[test]
fn a_damaged_entry_cannot_hide_valid_comments_cdata_or_namespaced_neighbours() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("es_systems.cfg");
    // A system without regional names (`region_pairs` names the NES's shelf itself): the list's own name is what is checked.
    let good = entry(dir.path(), "gameandwatch", "<![CDATA[Game & Watch <3]]>").replace("<system>", "<system xsi:type=\"console\">");
    let damaged = ["<system/>", "<system<name>bad</name>", "<system><name>bad</name><path>", "<system><name>bad</name><emulator name=\"oops>", "<system><!-- unclosed", "<system><fullname>&#0;</fullname></system>"];
    for bad in damaged {
        std::fs::write(&cfg, format!("<systemList xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><!-- <system><name>fake</name></system> -->{bad}{good}</systemList>")).unwrap();
        let shelf = library::load(&cfg).unwrap();
        assert_eq!(shelf.len(), 1, "{bad}");
        assert_eq!(shelf[0].fullname, "Game & Watch <3");
        assert_eq!(library::system_list_issues(&cfg).unwrap().len(), 1, "{bad}");
    }
}

#[test]
fn special_characters_in_rom_paths_find_the_original_game() {
    let dir = tempfile::tempdir().unwrap();
    let roms = dir.path().join("für ÄÖÜ & 'quoted' \"games\" < 3");
    let cfg = dir.path().join("es_systems.cfg");
    let xml = entry(&roms, "nes", "Nintendo");
    for entry in [xml.clone(), xml.replace('&', "&amp;").replace("< 3", "&lt; 3")] {
        std::fs::write(&cfg, format!("<systemList>{entry}</systemList>")).unwrap();
        let shelf = library::load(&cfg).unwrap();
        assert_eq!(shelf[0].games[0].path, roms.join("nes/Test.nes"));
        assert_eq!(library::all_systems(&cfg).unwrap()[0].path, roms.join("nes"));
    }
}

#[test]
fn special_characters_keep_their_spelling_and_paths() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("es_systems.cfg");
    for (encoded, shown) in [("Game &amp; Watch", "Game & Watch"), ("A &lt; B &gt; C", "A < B > C"), ("A < B", "A < B"), ("Game &Unknown; Watch", "Game &Unknown; Watch"), ("L&apos;été &quot;Österreich&quot;", "L'été \"Österreich\""), ("Pokémon – für ÄÖÜ äöü ß", "Pokémon – für ÄÖÜ äöü ß"), ("A &#38; B &#x26; C", "A & B & C")] {
        let xml = entry(dir.path(), "gameandwatch", encoded);
        std::fs::write(&cfg, format!("<systemList>{xml}</systemList>")).unwrap();
        assert_eq!(library::load(&cfg).unwrap()[0].fullname, shown);
        assert_eq!(library::all_systems(&cfg).unwrap()[0].name, shown);
    }
}

// ---- The whole list can't be read: the shelf runs on the built-in list and says so ----

/// The built-in list's standard folders, under one place all tests of this file share (`POCKETUI_BUILT_IN_ROMS` is the
/// computer's stand-in for `/storage/roms`). Two games in two standard folders.
fn standard_roms() -> &'static Path {
    static ROMS: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    ROMS.get_or_init(|| {
        // test-tmp-ok: removed by `remove_at_exit` below
        let dir = tempfile::Builder::new().prefix("pocketui-built-in-roms-").tempdir().unwrap().keep();
        // One folder for the whole test process, so no `TempDir` can drop it: it goes when the process exits.
        static LEFT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        extern "C" fn remove_at_exit() {
            if let Some(dir) = LEFT.get() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        let _ = LEFT.set(dir.clone());
        unsafe { libc::atexit(remove_at_exit) };
        std::fs::create_dir_all(dir.join("gba")).unwrap();
        std::fs::write(dir.join("gba/Tobu.gba"), b"game").unwrap();
        std::fs::create_dir_all(dir.join("nes")).unwrap();
        std::fs::write(dir.join("nes/Tetra.nes"), b"game").unwrap();
        // Safe: set once, before any test of this file reads the list through the built-in one.
        unsafe { std::env::set_var("POCKETUI_BUILT_IN_ROMS", &dir) };
        dir
    })
}

/// Every way a card's list can be unusable as a whole, with what Game check names.
fn unusable_lists(dir: &Path) -> Vec<(&'static str, Option<String>)> {
    let broken = "<system><name>bad</name><fullname>Broken</path></system>";
    vec![
        ("missing file", None),
        ("empty file", Some(String::new())),
        ("not XML at all", Some("this is not a system list".into())),
        ("cut off in the first entry", Some("<systemList><system><name>nes".into())),
        ("cut off in a tag", Some("<systemList><sys".into())),
        ("empty list", Some("<systemList></systemList>".into())),
        ("list with only comments", Some("<systemList><!-- nothing yet --></systemList>".into())),
        ("every entry broken", Some(format!("<systemList>{broken}<system><name>x</name></system><system><path>{}/x</path></system></systemList>", dir.display()))),
        ("binary garbage", Some("\u{0}\u{1}\u{2}<<<&&&>>>".into())),
    ]
}

#[test]
fn an_unusable_system_list_still_shows_the_games_in_their_standard_folders() {
    let roms = standard_roms();
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("es_systems.cfg");
    for (what, content) in unusable_lists(dir.path()) {
        let _ = std::fs::remove_file(&cfg);
        if let Some(text) = content { std::fs::write(&cfg, text).unwrap(); }
        let shelf = library::load(&cfg).unwrap_or_else(|e| panic!("{what}: the shelf must not fail: {e}"));
        let mut games: Vec<String> = shelf.iter().flat_map(|s| s.games.iter().map(|g| g.path.strip_prefix(roms).unwrap().display().to_string())).collect();
        games.sort();
        assert_eq!(games, ["gba/Tobu.gba", "nes/Tetra.nes"], "{what}: games in standard folders show");
        let catalog = library::all_systems(&cfg).unwrap_or_else(|e| panic!("{what}: Add games must not fail: {e}"));
        assert!(catalog.iter().any(|s| s.id == "gba") && catalog.iter().any(|s| s.id == "nes"), "{what}: the same systems in Add games");
    }
}

#[test]
fn an_unusable_system_list_is_named_not_silent() {
    standard_roms();
    use library::SystemListFallback as Why;
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("es_systems.cfg");
    for (what, content) in unusable_lists(dir.path()) {
        let _ = std::fs::remove_file(&cfg);
        if let Some(text) = content { std::fs::write(&cfg, text).unwrap(); }
        let status = library::system_list_status(&cfg);
        let why = status.fallback.clone().unwrap_or_else(|| panic!("{what}: the list is given up, and that is known"));
        match what {
            "missing file" => assert_eq!(why, Why::Missing),
            "every entry broken" | "cut off in the first entry" => {
                assert_eq!(why, Why::AllBroken);
                assert_eq!(status.skipped.len(), if what == "every entry broken" { 3 } else { 1 }, "each damaged entry is still listed");
            }
            _ => assert_eq!(why, Why::NoSystems, "{what}"),
        }
        let mut report = pocket_ui::game_check::Report::default();
        report.note_system_list(&cfg);
        assert!(!report.all_good(), "{what}: Game check must not say All good");
        let finding = report.findings.iter().find(|f| f.what.contains("built-in list")).unwrap_or_else(|| panic!("{what}: Game check says the built-in list is used: {:?}", report.findings));
        assert_eq!(finding.place.as_deref(), Some(cfg.display().to_string().as_str()), "{what}: names the file");
        assert!(finding.todo.contains("fix this file"), "{what}: and what to do: {}", finding.todo);
    }
}

#[test]
fn a_usable_list_is_never_replaced_and_says_nothing() {
    standard_roms();
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("es_systems.cfg");
    std::fs::write(&cfg, format!("<systemList>{}</systemList>", entry(dir.path(), "snes", "Super Nintendo"))).unwrap();
    let shelf = library::load(&cfg).unwrap();
    assert_eq!(shelf.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["snes"], "the player's own list, not the built-in one");
    assert!(library::system_list_status(&cfg).is_clean());
    // Tools and Music only: still the player's list, not "no systems".
    std::fs::write(&cfg, "<systemList><system><name>tools</name><fullname>Tools</fullname><path>/x</path></system></systemList>").unwrap();
    assert!(library::system_list_status(&cfg).fallback.is_none());
}

#[test]
fn a_list_in_another_text_encoding_keeps_its_systems() {
    standard_roms();
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("es_systems.cfg");
    let xml = format!("<systemList>{}</systemList>", entry(dir.path(), "nes", "Pok\u{e9}mon")).into_bytes();
    // Latin-1 "é" is the single byte 0xE9, no UTF-8: the list is still the player's.
    let latin1: Vec<u8> = String::from_utf8(xml).unwrap().chars().map(|c| c as u32 as u8).collect();
    std::fs::write(&cfg, latin1).unwrap();
    let shelf = library::load(&cfg).unwrap();
    assert_eq!(shelf.len(), 1, "one system, the player's");
    assert!(library::system_list_status(&cfg).fallback.is_none());
}
