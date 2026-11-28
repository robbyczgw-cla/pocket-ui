//! Consoles sold under regional names are one shelf (`region_pairs`, `library::join_region_pairs`): the shelf, each game's
//! own system for its start, saves and pictures, the data of before the merge, and where an upload goes.

use super::{load, load_with, Gamelists, System};
use crate::region_pairs;
use crate::testutil::TestDir;
use std::path::{Path, PathBuf};

/// A ROCKNIX card as the 2026-11 images list it: every regional folder with its own entry and its own default core.
struct Card {
    dir: TestDir,
}

const ENTRIES: &[(&str, &str, &str, &str, &str)] = &[
    // id, fullname as ROCKNIX writes it, extension, default core, a game
    ("megadrive", "Mega Drive", ".md .bin", "genesis_plus_gx", "Sonic the Hedgehog (Europe).md"),
    ("genesis", "Genesis", ".md .bin", "picodrive", "Comix Zone (USA).md"),
    ("megadrive-japan", "Mega Drive", ".md .bin", "genesis_plus_gx", "Puyo Puyo (Japan).md"),
    ("megadriveh", "Mega Drive (Hacks)", ".md .bin", "genesis_plus_gx", "Sonic Hack.md"),
    ("snes", "Super Nintendo", ".sfc .smc", "snes9x", "Super Metroid (Europe).sfc"),
    ("sfc", "Super Famicom", ".sfc .smc", "snes9x2010", "Rockman X (Japan).sfc"),
    ("nes", "Nintendo Entertainment System", ".nes", "nestopia", "Kirby's Adventure (Europe).nes"),
    ("famicom", "Famicom", ".nes", "fceumm", "Akumajou Densetsu (Japan).nes"),
    ("pcengine", "PC Engine", ".pce", "beetle_pce_fast", "Bonk's Adventure (Europe).pce"),
    ("tg16", "TurboGrafx-16", ".pce", "beetle_pce", "Blazing Lazers (USA).pce"),
    ("segacd", "Sega CD", ".chd", "genesis_plus_gx", "Sonic CD (USA).chd"),
    ("megacd", "Mega CD", ".chd", "picodrive", "Snatcher (Europe).chd"),
    ("gba", "Game Boy Advance", ".gba", "mgba", "Advance Wars (Europe).gba"),
];

impl Card {
    fn new(label: &str, only: &[&str]) -> Card {
        let dir = TestDir::new(&format!("region-{label}"));
        let mut xml = String::from("<systemList>");
        for (id, full, ext, core, game) in ENTRIES {
            let folder = dir.join("roms").join(id);
            xml += &format!(
                "<system><name>{id}</name><fullname>{full}</fullname><path>{}</path><extension>{ext}</extension>\
                 <emulators><emulator name=\"retroarch\"><cores><core default=\"true\">{core}</core></cores></emulator></emulators></system>",
                folder.display()
            );
            if only.is_empty() || only.contains(id) {
                std::fs::create_dir_all(&folder).unwrap();
                std::fs::write(folder.join(game), b"x").unwrap();
            }
        }
        xml += "</systemList>";
        std::fs::write(dir.join("es_systems.cfg"), xml).unwrap();
        Card { dir }
    }

    fn es(&self) -> PathBuf {
        self.dir.join("es_systems.cfg")
    }

    fn game(&self, id: &str) -> PathBuf {
        let (_, _, _, _, game) = ENTRIES.iter().find(|e| e.0 == id).unwrap();
        self.dir.join("roms").join(id).join(game)
    }

    fn data(&self) -> PathBuf {
        self.dir.join("data")
    }
}

fn shelf<'a>(systems: &'a [System], id: &str) -> &'a System {
    systems.iter().find(|s| s.name == id).unwrap_or_else(|| panic!("no shelf {id} in {:?}", systems.iter().map(|s| &s.name).collect::<Vec<_>>()))
}

fn ids(systems: &[System]) -> Vec<&str> {
    let mut v: Vec<&str> = systems.iter().map(|s| s.name.as_str()).collect();
    v.sort();
    v
}

/// Red before 0.8: a ROCKNIX card with games in `megadrive` and `genesis` had two Mega Drive shelves, SNES and Super
/// Famicom two, and so on. Now each console is one shelf; its hacks folder and every other system stay their own.
#[test]
fn a_console_sold_under_regional_names_is_one_shelf_with_the_games_of_all_its_folders() {
    let _n = region_pairs::for_this_thread(&[], false);
    let card = Card::new("one-shelf", &[]);
    let systems = load(&card.es()).unwrap();
    assert_eq!(ids(&systems), ["gba", "megadrive", "megadriveh", "nes", "pcengine", "segacd", "snes"]);
    let names = |id: &str| {
        let mut v: Vec<String> = shelf(&systems, id).games.iter().map(|g| g.path.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    };
    assert_eq!(names("megadrive"), ["genesis", "megadrive", "megadrive-japan"]);
    assert_eq!(names("snes"), ["sfc", "snes"]);
    assert_eq!(names("nes"), ["famicom", "nes"]);
    assert_eq!(names("pcengine"), ["pcengine", "tg16"]);
    assert_eq!(names("segacd"), ["megacd", "segacd"]);
    assert_eq!(names("megadriveh"), ["megadriveh"], "the hacks folder stays a shelf of its own");
    // In name order, as one list.
    let md: Vec<&str> = shelf(&systems, "megadrive").games.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(md, ["Comix Zone", "Puyo Puyo", "Sonic the Hedgehog"]);
    // Named for the start outside America, whatever ROCKNIX calls the folders.
    let full = |id: &str| shelf(&systems, id).fullname.clone();
    assert_eq!([full("megadrive"), full("snes"), full("nes"), full("pcengine"), full("segacd")], ["Mega Drive", "Super Nintendo", "Nintendo Entertainment System", "PC Engine", "Mega-CD"]);
    assert_eq!(crate::system_names::short("genesis"), Some("Mega Drive"));
}

#[test]
fn the_shelf_is_named_as_chosen_and_for_an_american_start() {
    let card = Card::new("names", &[]);
    {
        let _n = region_pairs::for_this_thread(&[], true);
        let systems = load(&card.es()).unwrap();
        let full: Vec<&str> = ["megadrive", "snes", "nes", "pcengine", "segacd"].iter().map(|id| shelf(&systems, id).fullname.as_str()).collect();
        assert_eq!(full, ["Genesis", "Super Nintendo", "Nintendo Entertainment System", "TurboGrafx-16", "Sega CD"]);
        assert_eq!(crate::system_names::spine_name("pcengine", "PC Engine"), "TurboGrafx-16");
    }
    let _n = region_pairs::for_this_thread(&[("nes", "famicom"), ("snes", "sfc")], false);
    let mut systems = load(&card.es()).unwrap();
    assert_eq!(shelf(&systems, "nes").fullname, "Famicom", "Famicom when chosen");
    assert_eq!(shelf(&systems, "snes").fullname, "Super Famicom");
    // A change on the page renames the shelves in place (`App::console_names_changed`).
    region_pairs::step(region_pairs::pair("megadrive").unwrap(), 1);
    super::name_region_pairs(&mut systems);
    assert_eq!(shelf(&systems, "megadrive").fullname, "Genesis");
    assert_eq!(shelf(&systems, "segacd").fullname, "Sega CD", "the CD follows its console");
}

/// Each game still starts as its folder's system: `-P genesis` with the core es_systems.cfg gives `genesis`, the
/// main folder's games with the shelf's own. Nothing on the card moved.
#[test]
fn every_game_on_a_merged_shelf_keeps_its_folders_system_and_emulator() {
    let _n = region_pairs::for_this_thread(&[], false);
    let card = Card::new("own", &[]);
    let systems = load(&card.es()).unwrap();
    let md = shelf(&systems, "megadrive");
    assert_eq!(md.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["genesis", "megadrive-japan"]);
    for (id, core) in [("megadrive", "genesis_plus_gx"), ("genesis", "picodrive"), ("megadrive-japan", "genesis_plus_gx")] {
        let game = card.game(id);
        assert!(game.is_file(), "{id}: the file stays where it was");
        assert_eq!(md.own_id(&game), id);
        let own = md.for_game(&game);
        assert_eq!((own.name.as_str(), own.core.as_deref()), (id, Some(core)), "{id}");
        assert!(own.games.is_empty() || id == "megadrive", "a part's facts come without the shelf's games");
        assert_eq!(region_pairs::own_system("megadrive", &game), id, "{id}: what covers and states are kept under");
    }
    let snes = shelf(&systems, "snes");
    assert_eq!(snes.for_game(&card.game("sfc")).core.as_deref(), Some("snes9x2010"));
    assert_eq!(snes.for_game(&card.game("snes")).core.as_deref(), Some("snes9x"));
    let pce = shelf(&systems, "pcengine");
    assert_eq!(pce.for_game(&card.game("tg16")).name, "tg16");
}

/// The migration: a player of 0.7 had covers, pictures, states, a custom cover, achievements, favourites and Recent
/// for games in `genesis/` under the system `genesis`. On the merged shelf all of it is still found.
#[test]
fn the_data_of_a_genesis_shelf_from_before_stays_with_its_games() {
    let _n = region_pairs::for_this_thread(&[], false);
    let card = Card::new("migrate", &[]);
    let data = card.data();
    let game = card.game("genesis");
    let stem = "Comix Zone (USA)";
    let put = |p: PathBuf| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"kept").unwrap();
        p
    };
    let cover = put(data.join("covers/genesis").join(format!("{stem}.png")));
    let snap = put(crate::game_pictures::path(&data, crate::game_pictures::Kind::Snap, "genesis", &game));
    let frame = put(crate::lastframe::frame_path(&data, "genesis", &game));
    let states = card.dir.join("roms/savestates");
    let state = put(states.join("genesis").join(format!("{stem}.state1")));
    let tally = put(crate::achievements::snapshot::Summary::path(&data, "genesis", &game));
    let custom = crate::covers::custom::path(&data, "genesis", &game);
    let mut favourites = crate::favorites::Favorites::load(&data);
    favourites.toggle("genesis", "Comix Zone", &game);
    let mut recents = crate::recents::Recents::load(&data);
    recents.record("genesis", "Comix Zone", &game, 600);

    let systems = load_with(&card.es(), Some(&Gamelists { data_dir: &data, places: &crate::gamelist::Locations::none() })).unwrap();
    let md = shelf(&systems, "megadrive");
    assert!(md.games.iter().any(|g| g.path == game), "the game is on the Mega Drive shelf");
    // Every caller passes the shelf's id now; each place still names `genesis`.
    assert_eq!(crate::covers::cover_path(&data, "megadrive", &game), cover);
    assert_eq!(crate::game_pictures::path(&data, crate::game_pictures::Kind::Snap, "megadrive", &game), snap);
    assert_eq!(crate::lastframe::frame_path(&data, "megadrive", &game), frame);
    assert_eq!(crate::achievements::snapshot::Summary::path(&data, "megadrive", &game), tally);
    assert_eq!(crate::covers::custom::path(&data, "megadrive", &game), custom, "a custom cover is keyed by the game's own system");
    let protected = [card.dir.join("roms/genesis"), card.dir.join("roms")];
    let places = crate::game_files::Places { states: &states, data: &data, protected: &protected, shared: false, saves_dir: None };
    let plan = crate::game_files::plan("megadrive", &game, &places).unwrap();
    assert_eq!(plan.states.iter().map(|i| i.path.clone()).collect::<Vec<_>>(), [state.clone()], "Delete game finds its states");
    assert!(plan.pictures.contains(&cover) && plan.pictures.contains(&snap) && plan.pictures.contains(&frame));
    let roots = crate::web::files::Roots::under(&card.dir.join("roms"));
    let listed: Vec<String> = crate::web::saves::files_of(&roots, "megadrive", &game, false).iter().map(|f| f.name.clone()).collect();
    assert!(listed.iter().any(|n| n == &format!("{stem}.state1")), "the web companion lists its state: {listed:?}");
    // Favourites and Recent keep `genesis`: they find the shelf it is on now.
    for stored in [&favourites.list[0].system, &recents.list[0].system] {
        assert_eq!(stored, "genesis");
        let found = systems.iter().find(|s| region_pairs::same_shelf(&s.name, stored)).unwrap();
        assert_eq!((found.name.as_str(), found.own_id(&game)), ("megadrive", "genesis"));
    }
    // The main folder's games keep theirs.
    let sonic = card.game("megadrive");
    assert_eq!(crate::covers::cover_path(&data, "megadrive", &sonic), data.join("covers/megadrive/Sonic the Hedgehog (Europe).png"));
}

/// A card with only one of a pair's folders keeps that shelf under its own id (its data is there), named as chosen.
#[test]
fn a_card_with_only_the_genesis_folder_has_one_genesis_shelf_named_as_chosen() {
    let _n = region_pairs::for_this_thread(&[], false);
    let card = Card::new("only", &["genesis", "sfc", "gba"]);
    let systems = load(&card.es()).unwrap();
    assert_eq!(ids(&systems), ["gba", "genesis", "sfc"]);
    let genesis = shelf(&systems, "genesis");
    assert!(genesis.parts.is_empty());
    assert_eq!(genesis.fullname, "Mega Drive", "the name of the start, not the folder's");
    assert_eq!(genesis.for_game(&card.game("genesis")).name, "genesis");
    assert_eq!(crate::covers::cover_path(&card.data(), "genesis", &card.game("genesis")), card.data().join("covers/genesis/Comix Zone (USA).png"));
    assert_eq!(shelf(&systems, "sfc").fullname, "Super Nintendo");
}

/// The list is cached between starts: a merged shelf comes back with its parts, and the names chosen since then.
#[test]
fn a_cached_list_keeps_the_parts_and_takes_the_names_chosen_since() {
    let card = Card::new("cache", &[]);
    let data = card.data();
    std::fs::create_dir_all(&data).unwrap();
    let key = crate::library_cache::Key::now(&card.es()).unwrap();
    {
        let _n = region_pairs::for_this_thread(&[], false);
        crate::library_cache::write(&data, &key, &load(&card.es()).unwrap()).unwrap();
    }
    let _n = region_pairs::for_this_thread(&[("megadrive", "genesis")], false);
    let back = crate::library_cache::read(&data, &key).unwrap();
    let md = shelf(&back, "megadrive");
    assert_eq!(md.fullname, "Genesis");
    assert_eq!(md.own_id(&card.game("genesis")), "genesis");
    assert_eq!(region_pairs::own_system("megadrive", &card.game("megadrive-japan")), "megadrive-japan");
}

fn info(roms: &Path, entries: &[(&str, &str)]) -> Vec<super::SysInfo> {
    entries.iter().map(|(id, folder)| super::SysInfo { id: id.to_string(), name: format!("ES {id}"), folder: format!("roms/{folder}"), path: roms.join(folder), extensions: vec!["md".into(), "sfc".into()] }).collect()
}

/// Uploads go into the console's main folder, or the other one when only that is on the card; one entry per console.
#[test]
fn an_upload_goes_into_the_main_folder_or_the_only_one_there() {
    let _n = region_pairs::for_this_thread(&[], false);
    let t = TestDir::new("region-upload");
    let roms = t.join("roms");
    let all = info(&roms, &[("megadrive", "megadrive"), ("genesis", "genesis"), ("megadrive-japan", "megadrive-japan"), ("megadriveh", "megadriveh"), ("gba", "gba")]);
    let offered = |list: &[super::SysInfo]| super::upload_folders(list).into_iter().map(|s| (s.id, s.name)).collect::<Vec<_>>();
    // No folder made yet: the main one.
    let want_main = vec![("megadrive".to_string(), "Mega Drive".to_string()), ("megadriveh".into(), "ES megadriveh".into()), ("gba".into(), "ES gba".into())];
    assert_eq!(offered(&all), want_main);
    // Only `genesis` is on the card: there.
    std::fs::create_dir_all(roms.join("genesis")).unwrap();
    assert_eq!(offered(&all)[0], ("genesis".to_string(), "Mega Drive".to_string()));
    // Both: the main one.
    std::fs::create_dir_all(roms.join("megadrive")).unwrap();
    assert_eq!(offered(&all), want_main);
    // dArkOS lists "snes" twice (`/roms/sufami/` is its SuFami Turbo): the folder named for the console.
    std::fs::create_dir_all(roms.join("sufami")).unwrap();
    let dark = info(&roms, &[("snes", "sufami"), ("snes", "snes"), ("sfc", "sfc")]);
    let got = super::upload_folders(&dark);
    assert_eq!(got.iter().map(|s| s.path.clone()).collect::<Vec<_>>(), [roms.join("snes")]);
}

/// The console's page and its credits name it as the shelf does (seen 26.11.: "Mega Drive" over a chosen Genesis);
/// a fuller name the fact sheet has for the same choice stays.
#[test]
fn the_console_page_is_named_as_chosen() {
    let name = |id: &str| crate::consoles::shown_name(crate::consoles::for_system(id).unwrap()).to_string();
    {
        let _n = region_pairs::for_this_thread(&[], false);
        assert_eq!([name("megadrive"), name("genesis"), name("snes"), name("nes"), name("segacd")], ["Mega Drive", "Mega Drive", "Super Nintendo Entertainment System", "Nintendo Entertainment System", "Mega-CD"]);
    }
    let _n = region_pairs::for_this_thread(&[("megadrive", "genesis"), ("nes", "famicom"), ("pcengine", "tg16"), ("snes", "sfc")], false);
    assert_eq!([name("megadrive"), name("nes"), name("pcengine"), name("snes"), name("segacd")], ["Genesis", "Famicom", "TurboGrafx-16", "Super Famicom", "Sega CD"]);
    assert_eq!(name("gba"), crate::consoles::for_system("gba").unwrap().name, "every other console keeps the file's name");
}
