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
    let good = entry(dir.path(), "nes", "<![CDATA[Game & Watch <3]]>").replace("<system>", "<system xsi:type=\"console\">");
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
        let xml = entry(dir.path(), "nes", encoded);
        std::fs::write(&cfg, format!("<systemList>{xml}</systemList>")).unwrap();
        assert_eq!(library::load(&cfg).unwrap()[0].fullname, shown);
        assert_eq!(library::all_systems(&cfg).unwrap()[0].name, shown);
    }
}
