//! A card that comes from another handheld or frontend is read as it is (docs/OTHER-LAYOUTS.md): its system folders by
//! whatever name they have, its games with the names its gamelists give, the box art already on it before anything is
//! downloaded, and nothing on the card is written. One small fixture tree per layout: fake ROMs, tiny pictures, a gamelist.xml.

use pocket_ui::gamelist::{self, Index, Locations};
use pocket_ui::library::{self, Gamelists};
use std::path::{Path, PathBuf};

/// A 1x1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
    0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00,
    0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// The device: es_systems.cfg (ROCKNIX's names, `<card>/roms/<id>` as the folders) and a data folder outside the card.
struct Card {
    root: tempfile::TempDir,
    cfg: PathBuf,
}

const SYSTEMS: &[(&str, &str, &str)] = &[
    ("gba", "Game Boy Advance", ".gba .zip"),
    ("nes", "Nintendo Entertainment System", ".nes .zip"),
    ("snes", "Super Nintendo", ".sfc .smc .zip"),
    ("megadrive", "Sega Mega Drive", ".md .bin .zip"),
    ("psx", "Sony PlayStation", ".cue .chd .m3u"),
];

impl Card {
    fn new() -> Card {
        let root = tempfile::tempdir().unwrap();
        let roms = root.path().join("card/roms");
        std::fs::create_dir_all(&roms).unwrap();
        let mut xml = String::from("<systemList>");
        for (id, name, exts) in SYSTEMS {
            xml += &format!("<system><name>{id}</name><fullname>{name}</fullname><path>{}</path><extension>{exts}</extension><command>true</command></system>", roms.join(id).display());
        }
        xml += "</systemList>";
        let cfg = root.path().join("es_systems.cfg");
        std::fs::write(&cfg, xml).unwrap();
        std::fs::create_dir_all(root.path().join("data")).unwrap();
        Card { root, cfg }
    }
    fn card(&self) -> PathBuf {
        self.root.path().join("card")
    }
    fn roms(&self) -> PathBuf {
        self.card().join("roms")
    }
    fn data(&self) -> PathBuf {
        self.root.path().join("data")
    }
    /// A file of the card (`bytes` empty: a game with some content).
    fn put(&self, rel: &str, bytes: impl AsRef<[u8]>) {
        let p = self.card().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let bytes = bytes.as_ref();
        std::fs::write(p, if bytes.is_empty() { b"rom" } else { bytes }).unwrap();
    }
    fn locations(&self) -> Locations {
        Locations {
            es_dirs: vec![self.card().join(".config/emulationstation")],
            es_de: self.card().join("ES-DE"),
            catalogues: vec![self.card().join("MUOS/info/catalogue")],
        }
    }
    fn systems(&self) -> Vec<library::System> {
        let places = self.locations();
        library::load_with(&self.cfg, Some(&Gamelists { data_dir: &self.data(), places: &places })).unwrap()
    }
    fn index(&self) -> Index {
        Index::load(&self.data(), &self.locations(), &library::read_sources(&self.cfg))
    }
    fn cover(&self, index: &Index, system: &str, rom: &str) -> Option<PathBuf> {
        let game = self.systems().into_iter().find(|s| s.name == system)?.games.into_iter().find(|g| g.path.file_name().unwrap() == rom)?;
        gamelist::usable_cover(index.cover_candidates(system, &game.path))
    }
    /// Every file and folder of the card with size and modification time: what "nothing was written" compares.
    fn snapshot(&self) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64, std::time::SystemTime)>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let m = e.metadata().unwrap();
                out.push((e.path(), m.len(), m.modified().unwrap()));
                if m.is_dir() {
                    walk(&e.path(), out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.card(), &mut out);
        out.sort();
        out
    }
}

fn names(systems: &[library::System], system: &str) -> Vec<String> {
    systems.iter().find(|s| s.name == system).map(|s| s.games.iter().map(|g| g.name.clone()).collect()).unwrap_or_default()
}

/// The last folder and the file name of a cover: "images/Sonic-thumb.png".
fn file(path: &Option<PathBuf>) -> String {
    let Some(p) = path else { return String::new() };
    let parts: Vec<_> = p.components().rev().take(2).map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    format!("{}/{}", parts[1], parts[0])
}

#[test]
fn batocera_and_knulli_use_images_and_the_gamelists_thumbnail_as_the_box() {
    let c = Card::new();
    c.put("roms/gba/Advance Wars (USA).gba", b"");
    c.put("roms/gba/sf2ce.gba", b"");
    c.put("roms/gba/Metroid Fusion (USA).gba", b"");
    c.put("roms/gba/images/Advance Wars (USA)-image.png", PNG);
    c.put("roms/gba/images/Advance Wars (USA)-thumb.png", PNG);
    c.put("roms/gba/images/Advance Wars (USA)-marquee.png", PNG);
    c.put("roms/gba/images/sf2ce-image.png", PNG);
    // No gamelist entry for this one: the folder alone says which picture is the box.
    c.put("roms/gba/images/Metroid Fusion (USA)-thumb.png", PNG);
    c.put(
        "roms/gba/gamelist.xml",
        r#"<gameList><game><path>./sf2ce.gba</path><name>Street Fighter II' Special Champion Edition</name><image>./images/sf2ce-image.png</image></game>
<game><path>./Advance Wars (USA).gba</path><name>Advance Wars</name><image>./images/Advance Wars (USA)-image.png</image><thumbnail>./images/Advance Wars (USA)-thumb.png</thumbnail><marquee>./images/Advance Wars (USA)-marquee.png</marquee></game></gameList>"#,
    );
    let before = c.snapshot();
    let systems = c.systems();
    assert_eq!(names(&systems, "gba"), ["Advance Wars", "Metroid Fusion", "Street Fighter II' Special Champion Edition"], "a cryptic file name gets the gamelist's name");
    let index = c.index();
    assert_eq!(file(&c.cover(&index, "gba", "Advance Wars (USA).gba")), "images/Advance Wars (USA)-thumb.png", "the thumbnail is the box, not the image or the marquee");
    assert_eq!(file(&c.cover(&index, "gba", "Metroid Fusion (USA).gba")), "images/Metroid Fusion (USA)-thumb.png");
    assert_eq!(file(&c.cover(&index, "gba", "sf2ce.gba")), "images/sf2ce-image.png", "a lone picture the gamelist names is the cover");
    assert!(index.has_box_art("gba", &c.roms().join("gba/Advance Wars (USA).gba")));
    assert_eq!(before, c.snapshot(), "nothing is written into the card");
}

#[test]
fn a_screenshot_is_no_box_so_the_download_is_still_wanted() {
    let c = Card::new();
    c.put("roms/snes/Super Mario World (USA).sfc", b"");
    c.put("roms/snes/images/Super Mario World (USA)-image.png", PNG);
    c.put("roms/snes/gamelist.xml", r#"<gameList><game><path>./Super Mario World (USA).sfc</path><image>./images/Super Mario World (USA)-image.png</image><thumbnail>./images/missing-thumb.png</thumbnail></game></gameList>"#);
    let index = c.index();
    let rom = c.roms().join("snes/Super Mario World (USA).sfc");
    assert!(!index.has_box_art("snes", &rom), "the only picture is a screenshot");
    assert_eq!(file(&c.cover(&index, "snes", "Super Mario World (USA).sfc")), "images/Super Mario World (USA)-image.png", "it still beats an empty box on the shelf");
}

#[test]
fn anbernic_stock_folders_in_capitals_and_imgs_are_read() {
    let c = Card::new();
    c.put("roms/GBA/Wario Land 4.gba", b"");
    c.put("roms/GBA/Imgs/Wario Land 4.png", PNG);
    c.put("roms/FC/Contra (USA).nes", b"");
    c.put("roms/FC/Imgs/Contra (USA).png", PNG);
    c.put("roms/SFC/Chrono Trigger.sfc", b"");
    c.put("roms/MD/Sonic The Hedgehog.md", b"");
    c.put("roms/PS/Crash/Crash.cue", b"");
    c.put("roms/PS/Imgs/Crash.png", PNG);
    c.put("roms/MD/Imgs/sonic the hedgehog.png", PNG);
    let before = c.snapshot();
    let systems = c.systems();
    assert_eq!(names(&systems, "gba"), ["Wario Land 4"]);
    assert_eq!(names(&systems, "nes"), ["Contra"]);
    assert_eq!(names(&systems, "snes"), ["Chrono Trigger"]);
    assert_eq!(names(&systems, "megadrive"), ["Sonic The Hedgehog"]);
    assert_eq!(names(&systems, "psx"), ["Crash"]);
    let index = c.index();
    assert_eq!(file(&c.cover(&index, "gba", "Wario Land 4.gba")), "Imgs/Wario Land 4.png");
    assert_eq!(file(&c.cover(&index, "nes", "Contra (USA).nes")), "Imgs/Contra (USA).png");
    assert_eq!(file(&c.cover(&index, "megadrive", "Sonic The Hedgehog.md")), "Imgs/sonic the hedgehog.png", "the name matches in any case");
    assert_eq!(file(&c.cover(&index, "psx", "Crash.cue")), "Imgs/Crash.png", "a disc in its own folder");
    assert_eq!(before, c.snapshot());
}

#[test]
fn arkos_rocknix_and_jelos_images_with_a_gamelist() {
    let c = Card::new();
    c.put("roms/snes/Super Metroid (Japan, USA).sfc", b"");
    c.put("roms/snes/images/Super Metroid (Japan, USA)-image.png", PNG);
    c.put("roms/snes/images/Super Metroid (Japan, USA)-thumb.png", PNG);
    c.put("roms/snes/gamelist.xml", r#"<gameList><game><path>./Super Metroid (Japan, USA).sfc</path><name>Super Metroid</name><desc>Samus.</desc><image>./images/Super Metroid (Japan, USA)-image.png</image></game></gameList>"#);
    let index = c.index();
    // The gamelist names only the image; the thumbnail beside it is a box and comes first.
    assert_eq!(file(&c.cover(&index, "snes", "Super Metroid (Japan, USA).sfc")), "images/Super Metroid (Japan, USA)-thumb.png");
}

#[test]
fn nextui_and_minui_tags_in_brackets_and_media_folders() {
    let c = Card::new();
    c.put("roms/Game Boy Advance (GBA)/Golden Sun (USA).gba", b"");
    c.put("roms/Game Boy Advance (GBA)/.media/Golden Sun (USA).png", PNG);
    c.put("roms/Game Boy Advance (MGBA)/Mario Kart - Super Circuit (USA).gba", b"");
    c.put("roms/Super Nintendo Entertainment System (SFC)/F-Zero (USA).sfc", b"");
    c.put("roms/Super Nintendo Entertainment System (SFC)/.media/F-Zero (USA).png", PNG);
    c.put("roms/Sega Genesis (MD)/Streets of Rage 2 (USA).md", b"");
    c.put("roms/Nintendo Entertainment System (FC)/Kirby's Adventure (USA).nes", b"");
    c.put("roms/Sony PlayStation (PS)/Gran Turismo/Gran Turismo.cue", b"");
    c.put("roms/Sony PlayStation (PS)/Gran Turismo/Gran Turismo.bin", b"");
    c.put("roms/Sony PlayStation (PS)/.media/Gran Turismo.png", PNG);
    c.put("roms/Pico-8 (P8)/celeste.p8", b"");
    let before = c.snapshot();
    let systems = c.systems();
    assert_eq!(names(&systems, "gba"), ["Golden Sun", "Mario Kart - Super Circuit"], "GBA and MGBA are one system");
    assert_eq!(names(&systems, "snes"), ["F-Zero"]);
    assert_eq!(names(&systems, "megadrive"), ["Streets of Rage 2"]);
    assert_eq!(names(&systems, "nes"), ["Kirby's Adventure"]);
    assert_eq!(names(&systems, "psx"), ["Gran Turismo"]);
    assert_eq!(systems.len(), 5, "a folder of a system this device does not play is left alone");
    let index = c.index();
    assert_eq!(file(&c.cover(&index, "gba", "Golden Sun (USA).gba")), ".media/Golden Sun (USA).png");
    assert_eq!(file(&c.cover(&index, "snes", "F-Zero (USA).sfc")), ".media/F-Zero (USA).png");
    assert_eq!(file(&c.cover(&index, "psx", "Gran Turismo.cue")), ".media/Gran Turismo.png", "the picture of a disc in its own folder is named by the folder");
    assert!(c.cover(&index, "megadrive", "Streets of Rage 2 (USA).md").is_none(), "no picture on the card: the download will fetch one");
    assert!(!index.has_box_art("megadrive", &c.roms().join("Sega Genesis (MD)/Streets of Rage 2 (USA).md")));
    assert_eq!(before, c.snapshot());
}

#[test]
fn muos_games_in_system_name_folders_and_the_catalogue() {
    let c = Card::new();
    c.put("roms/Nintendo Game Boy Advance/Advance Wars 2 (USA).gba", b"");
    c.put("roms/Sony PlayStation/Spyro (USA)/Spyro (USA).cue", b"");
    c.put("roms/Nintendo SNES-SFC/Earthbound (USA).sfc", b"");
    c.put("MUOS/info/catalogue/Nintendo Game Boy Advance/box/Advance Wars 2 (USA).png", PNG);
    c.put("MUOS/info/catalogue/Nintendo Game Boy Advance/preview/Advance Wars 2 (USA).png", PNG);
    c.put("MUOS/info/catalogue/Nintendo SNES-SFC/box/EARTHBOUND (usa).png", PNG);
    c.put("MUOS/info/catalogue/Nintendo SNES-SFC/preview/Earthbound (USA).png", PNG);
    let before = c.snapshot();
    let systems = c.systems();
    assert_eq!(names(&systems, "gba"), ["Advance Wars 2"]);
    assert_eq!(names(&systems, "psx"), ["Spyro"]);
    assert_eq!(names(&systems, "snes"), ["Earthbound"]);
    let index = c.index();
    assert_eq!(file(&c.cover(&index, "gba", "Advance Wars 2 (USA).gba")), "box/Advance Wars 2 (USA).png", "the box, not the preview");
    assert_eq!(file(&c.cover(&index, "snes", "Earthbound (USA).sfc")), "box/EARTHBOUND (usa).png", "matched without case");
    assert_eq!(before, c.snapshot());
}

#[test]
fn es_de_media_folders_prefer_covers_over_screenshots() {
    let c = Card::new();
    c.put("roms/genesis/Sonic The Hedgehog 2 (World).md", b"");
    c.put("roms/gba/Pokemon - Emerald Version (USA).gba", b"");
    c.put("ES-DE/downloaded_media/genesis/screenshots/Sonic The Hedgehog 2 (World).png", PNG);
    c.put("ES-DE/downloaded_media/genesis/marquees/Sonic The Hedgehog 2 (World).png", PNG);
    c.put("ES-DE/downloaded_media/genesis/covers/Sonic The Hedgehog 2 (World).png", PNG);
    c.put("ES-DE/downloaded_media/gba/screenshots/Pokemon - Emerald Version (USA).jpg", PNG);
    c.put("ES-DE/downloaded_media/gba/miximages/Pokemon - Emerald Version (USA).png", PNG);
    c.put("ES-DE/gamelists/gba/gamelist.xml", r#"<gameList><game><path>./Pokemon - Emerald Version (USA).gba</path><name>Pokémon Emerald</name></game></gameList>"#);
    let systems = c.systems();
    assert_eq!(names(&systems, "megadrive"), ["Sonic The Hedgehog 2"], "ES-DE's 'genesis' folder is the Mega Drive");
    // ES-DE names its media folders after its own system ids: `genesis` is this device's `megadrive`.
    let index = c.index();
    assert_eq!(file(&c.cover(&index, "megadrive", "Sonic The Hedgehog 2 (World).md")), "covers/Sonic The Hedgehog 2 (World).png", "covers, not the screenshot or the marquee");
    assert!(index.has_box_art("megadrive", &c.roms().join("genesis/Sonic The Hedgehog 2 (World).md")));
    let gba = file(&c.cover(&index, "gba", "Pokemon - Emerald Version (USA).gba"));
    assert_eq!(gba, "miximages/Pokemon - Emerald Version (USA).png", "a mix beats a screenshot");
    assert!(!index.has_box_art("gba", &c.roms().join("gba/Pokemon - Emerald Version (USA).gba")), "a mix is no box");
}

#[test]
fn a_folder_nobody_knows_stays_unread_and_is_still_reported() {
    let c = Card::new();
    c.put("roms/Atari Jaguar/Tempest 2000.zip", b"");
    c.put("roms/images/Something.png", PNG);
    c.put("roms/gba/Real.gba", b"");
    let systems = c.systems();
    assert_eq!(systems.len(), 1);
    let catalog = library::all_systems(&c.cfg).unwrap();
    let strays: Vec<String> = library::stray_folders(&c.roms(), &catalog).into_iter().map(|s| s.name).collect();
    assert_eq!(strays, ["Atari Jaguar"]);
}

#[test]
fn a_folder_that_is_a_systems_already_is_not_read_twice() {
    let c = Card::new();
    c.put("roms/gba/One.gba", b"");
    // The same folder under a second name (a link): one folder, one list.
    std::os::unix::fs::symlink(c.roms().join("gba"), c.roms().join("Game Boy Advance (GBA)")).unwrap();
    let systems = c.systems();
    assert_eq!(names(&systems, "gba"), ["One"]);
}

/// Speed on a big card: 5,000 games and 5,000 pictures (`cargo test --profile gate --test other_layouts -- --ignored --nocapture timing_`).
/// The limits are loose (a busy build machine): the numbers it prints are the result, docs/OTHER-LAYOUTS.md.
#[test]
#[ignore = "release check: speed of a 5,000-game card, prints the numbers"]
fn timing_a_5000_game_card_in_each_layout() {
    let time = |label: &str, c: &Card, system: &str| {
        let t = std::time::Instant::now();
        let systems = c.systems();
        let read = t.elapsed();
        let t = std::time::Instant::now();
        let index = c.index();
        let art = t.elapsed();
        let games = systems.iter().find(|s| s.name == system).unwrap().games.clone();
        let t = std::time::Instant::now();
        let with_box = games.iter().filter(|g| index.has_box_art(system, &g.path)).count();
        let asked = t.elapsed();
        println!("{label}: {} games; library read {read:?}, pictures listed {art:?}, 'has a box?' for all {asked:?} ({with_box} yes)", games.len());
        assert_eq!(games.len(), 5000);
        assert!(read.as_secs() < 10 && art.as_secs() < 10 && asked.as_secs() < 10);
        with_box
    };
    // The usual card, no pictures, no other folders: what every start pays for (first, it also pays the process's one-time setup).
    let c = Card::new();
    for i in 0..5000 {
        c.put(&format!("roms/gba/Game number {i:05} (USA).gba"), b"");
    }
    assert_eq!(time("plain device folder, no pictures", &c, "gba"), 0);
    // Another card's folder with pictures beside the games.
    let c = Card::new();
    for i in 0..5000 {
        c.put(&format!("roms/Game Boy Advance (GBA)/Game number {i:05} (USA).gba"), b"");
        c.put(&format!("roms/Game Boy Advance (GBA)/.media/Game number {i:05} (USA).png"), PNG);
    }
    assert_eq!(time("NextUI-style, .media", &c, "gba"), 5000);
    // The device's own folder, a gamelist with 5,000 entries and an images folder.
    let c = Card::new();
    let mut xml = String::from("<gameList>");
    for i in 0..5000 {
        c.put(&format!("roms/gba/Game number {i:05} (USA).gba"), b"");
        c.put(&format!("roms/gba/images/Game number {i:05} (USA)-thumb.png"), PNG);
        xml += &format!("<game><path>./Game number {i:05} (USA).gba</path><name>Game number {i}</name><thumbnail>./images/Game number {i:05} (USA)-thumb.png</thumbnail></game>");
    }
    c.put("roms/gba/gamelist.xml", xml + "</gameList>");
    assert_eq!(time("Batocera-style, gamelist + images", &c, "gba"), 5000);
}
