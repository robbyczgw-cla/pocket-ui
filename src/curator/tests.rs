//! Echoes quality on a realistic library (the real gamedb and curatordb, file names as No-Intro and Redump write
//! them), Lens and smart shelves, Versions and remakes, and the speed targets of docs/CURATOR.md.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::gamedb::GameDb;
use crate::library_meta::{GameMeta, Status, Sticker};

/// (system, file names): a library like a player's, with games owned on several systems (Chrono Trigger on SNES,
/// PS1 and DS; Final Fantasy VI on SNES as a fan translation and on GBA; Burnout Dominator on PS2 and PSP; Link's
/// Awakening on GB and GBC), a remake (Ocarina of Time and its 3DS remake) and a PortMaster port without data.
pub(crate) const LIBRARY: &[(&str, &[&str])] = &[
    ("3ds", &["Legend of Zelda, The - Ocarina of Time 3D (USA) (En,Fr,Es).3ds"]),
    ("gamecube", &["Animal Crossing (USA).rvz", "Mario Kart - Double Dash!! (USA).rvz", "F-Zero GX (USA).rvz", "Super Smash Bros. Melee (USA) (En,Ja) (Rev 2).rvz", "Burnout 2 - Point of Impact (USA).rvz"]),
    ("gb", &["Tetris (World) (Rev 1).gb", "Legend of Zelda, The - Link's Awakening (USA, Europe) (Rev 2).gb", "Kirby's Dream Land (USA, Europe).gb"]),
    ("gbc", &["Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev 2) (SGB Enhanced) (GB Compatible).gbc", "Pokemon - Crystal Version (USA, Europe) (Rev 1).gbc"]),
    (
        "gba",
        &[
            "Advance Wars (USA) (Rev 1).gba",
            "Astro Boy - Omega Factor (USA).gba",
            "Castlevania - Aria of Sorrow (USA).gba",
            "F-Zero - Maximum Velocity (USA, Europe).gba",
            "Final Fantasy VI Advance (USA).gba",
            "Golden Sun (USA, Europe).gba",
            "Gunstar Super Heroes (USA).gba",
            "Kirby - Nightmare in Dream Land (USA).gba",
            "Legend of Zelda, The - The Minish Cap (USA).gba",
            "Mario & Luigi - Superstar Saga (USA).gba",
            "Mario Kart - Super Circuit (USA).gba",
            "Mega Man Zero (USA).gba",
            "Metroid - Zero Mission (USA).gba",
            "Metroid Fusion (USA).gba",
            "Mother 3 (Japan) [T-En by Chewy v1.3].gba",
            "Pokemon - Emerald Version (USA, Europe).gba",
            "WarioWare, Inc. - Mega Microgame$! (USA).gba",
        ],
    ),
    ("megadrive", &["Sonic The Hedgehog 2 (World) (Rev A).md", "Streets of Rage 2 (USA).md", "Phantasy Star IV (USA).md", "Gunstar Heroes (USA).md"]),
    ("n64", &["Legend of Zelda, The - Ocarina of Time (USA) (Rev 2).z64", "Mario Kart 64 (USA).z64", "F-Zero X (USA).z64", "Super Mario 64 (USA).z64", "GoldenEye 007 (USA).z64"]),
    ("nds", &["Chrono Trigger (USA) (En,Fr).nds", "Mario Kart DS (USA, Australia) (En,Fr,De,Es,It).nds", "New Super Mario Bros. (USA, Australia).nds", "Advance Wars - Dual Strike (USA, Australia).nds"]),
    ("ports", &["Celeste Classic.sh"]),
    (
        "ps2",
        &[
            "Burnout (USA).iso",
            "Burnout 2 - Point of Impact (USA).iso",
            "Burnout 3 - Takedown (USA).iso",
            "Burnout Dominator (USA).iso",
            "Burnout Revenge (USA).iso",
            "Gran Turismo 4 (USA).iso",
            "Katamari Damacy (USA).iso",
            "Need for Speed - Hot Pursuit 2 (USA).iso",
            "OutRun 2006 - Coast 2 Coast (USA).iso",
            "Shadow of the Colossus (USA).iso",
        ],
    ),
    ("psp", &["Burnout Legends (USA).iso", "Burnout Dominator (USA).iso", "Ridge Racer (USA).iso", "Lumines (USA).iso"]),
    ("psx", &["Castlevania - Symphony of the Night (USA).chd", "Chrono Cross (USA).m3u", "Chrono Trigger (Japan).chd", "Crash Team Racing (USA).chd", "R4 - Ridge Racer Type 4 (USA).chd", "Wipeout 2097 (Europe).chd"]),
    (
        "snes",
        &[
            "Chrono Trigger (USA).sfc",
            "Donkey Kong Country (USA) (Rev 2).sfc",
            "EarthBound (USA).sfc",
            "F-Zero (USA).sfc",
            "Final Fantasy VI (Japan) [T-En by RPGe v1.1].sfc",
            "Legend of Zelda, The - A Link to the Past (USA).sfc",
            "Secret of Mana (USA).sfc",
            "Street Fighter II - The World Warrior (USA).sfc",
            "Super Bomberman (USA).sfc",
            "Super Mario All-Stars (USA).sfc",
            "Super Mario Kart (USA).sfc",
            "Super Mario World (USA).sfc",
            "Super Metroid (Japan, USA) (En,Ja).sfc",
        ],
    ),
];

pub(crate) fn systems() -> Vec<SystemName> {
    [
        ("3ds", "Nintendo 3DS", "3DS"),
        ("gamecube", "GameCube", "GameCube"),
        ("gb", "Game Boy", "Game Boy"),
        ("gbc", "Game Boy Color", "Game Boy Color"),
        ("gba", "Game Boy Advance", "GBA"),
        ("megadrive", "Mega Drive", "Mega Drive"),
        ("n64", "Nintendo 64", "N64"),
        ("nds", "Nintendo DS", "DS"),
        ("ports", "Ports", "Ports"),
        ("ps2", "PlayStation 2", "PS2"),
        ("psp", "PlayStation Portable", "PSP"),
        ("psx", "PlayStation", "PS1"),
        ("snes", "Super Nintendo", "SNES"),
    ]
        .iter()
        .map(|(id, name, short)| SystemName { id: id.to_string(), name: name.to_string(), short: short.to_string() })
        .collect()
}

pub(crate) fn library() -> Vec<LibraryGame> {
    let mut out = Vec::new();
    for (s, (system, files)) in LIBRARY.iter().enumerate() {
        for (g, file) in files.iter().enumerate() {
            let stem = Path::new(file).file_stem().unwrap().to_str().unwrap();
            out.push(LibraryGame {
                at: (s, g),
                system: system.to_string(),
                path: PathBuf::from(format!("/storage/roms/{system}/{file}")),
                name: crate::library::display_name(stem),
                choice: None,
                identified: None,
            });
        }
    }
    out
}

fn catalog(feel: &dyn FeelSource) -> Arc<Catalog> {
    let mut db = GameDb::embedded();
    Arc::new(build_catalog(&library(), systems(), &mut db, feel))
}

fn path(system: &str, file: &str) -> PathBuf {
    PathBuf::from(format!("/storage/roms/{system}/{file}"))
}

/// The player: finished Burnout 3, played Gran Turismo 4 a lot, dropped Pokémon, starred Chrono Trigger (SNES).
fn player() -> Player {
    let mut meta = std::collections::BTreeMap::new();
    let mut set = |p: PathBuf, status: Status, secs: u64, stickers: Vec<u32>| {
        meta.insert(p.to_string_lossy().into_owned(), GameMeta { status, status_set: true, secs, sessions: 3, stickers, ..Default::default() });
    };
    set(path("ps2", "Burnout 3 - Takedown (USA).iso"), Status::Completed, 14 * 3600, vec![1]);
    set(path("ps2", "Gran Turismo 4 (USA).iso"), Status::Playing, 30 * 3600, vec![]);
    set(path("gba", "Pokemon - Emerald Version (USA, Europe).gba"), Status::Dropped, 2 * 3600, vec![]);
    set(path("snes", "Super Mario Kart (USA).sfc"), Status::NotStarted, 0, vec![1]);
    let mut recents = HashMap::new();
    recents.insert(path("ps2", "Gran Turismo 4 (USA).iso"), (1_700_000_000, 30 * 3600));
    recents.insert(path("nds", "Chrono Trigger (USA) (En,Fr).nds"), (1_000_000_000, 3 * 3600));
    Player {
        meta,
        stickers: vec![Sticker { id: 1, name: "Couch co-op".into(), pinned: true }],
        preferred: Default::default(),
        favorites: HashSet::from([path("snes", "Chrono Trigger (USA).sfc")]),
        recents,
        now: 1_700_100_000,
    }
}

fn index(feel: &dyn FeelSource) -> Index {
    let cat = catalog(feel);
    let user = user_state(&cat, &player());
    Index::build(cat, user)
}

fn find(ix: &Index, name: &str, system: &str) -> usize {
    ix.catalog.games.iter().position(|g| g.name == name && g.system == system).unwrap_or_else(|| panic!("{name} ({system}) not in the library"))
}

fn names(ix: &Index, echoes: &[echoes::Echo]) -> Vec<String> {
    echoes.iter().map(|e| ix.game(e.game).name.clone()).collect()
}

/// Prints a seed's Echoes with their reasons and the time they took (`cargo test --release -- --nocapture`).
fn show(ix: &Index, seed: usize) -> Vec<echoes::Echo> {
    let t = Instant::now();
    let e = ix.echoes(seed, echoes::FULL_LIST);
    let took = t.elapsed();
    println!("Echoes of {} ({}), {:?}:", ix.game(seed).name, ix.game(seed).system, took);
    for x in &e {
        println!("  {:5.2}  {:<34} {:<5} {}", x.score, ix.game(x.game).name, ix.catalog.system_short(&ix.game(x.game).system), x.reason);
    }
    e
}

#[test]
fn every_test_game_is_found_in_gamedb_but_the_port() {
    let cat = catalog(&NoFeel);
    let missing: Vec<&str> = cat.games.iter().filter(|g| g.qid.is_none()).map(|g| g.name.as_str()).collect();
    assert_eq!(missing, ["Celeste Classic"], "no gamedb entry");
}

/// The seeds of the report, with the shipped tags.
#[test]
fn echoes_with_the_shipped_tags() {
    let ix = index(&CuratorDbFeel);
    for (name, system) in [
        ("Burnout Revenge", "ps2"),
        ("Chrono Trigger", "snes"),
        ("F-Zero - Maximum Velocity", "gba"),
        ("Super Mario All-Stars", "snes"),
        ("Mario Kart - Double Dash!!", "gamecube"),
        ("Celeste Classic", "ports"),
    ] {
        show(&ix, find(&ix, name, system));
    }
}

#[test]
fn echoes_of_burnout_revenge_are_its_series_then_arcade_racers_before_a_sim() {
    let ix = index(&CuratorDbFeel);
    let e = ix.echoes(find(&ix, "Burnout Revenge", "ps2"), echoes::FULL_LIST);
    let n = names(&ix, &e);
    assert!(n[0].starts_with("Burnout"), "a game of the series first: {n:?}");
    assert_eq!(n.iter().filter(|x| x.starts_with("Burnout")).count(), 3, "at most three of one series: {n:?}");
    let pos = |x: &str| n.iter().position(|y| y == x).unwrap_or(99);
    assert!(pos("Need for Speed - Hot Pursuit 2") < pos("Gran Turismo 4"), "an arcade racer before a simulation: {n:?}");
    assert!(!n.iter().any(|x| x.starts_with("Pokemon")), "a dropped game is never an Echo");
    assert!(!n.contains(&"Burnout Revenge".to_string()), "not the seed itself");
    assert!(e.iter().all(|x| !x.reason.is_empty() && x.reason.chars().count() < 70), "short reasons: {e:?}");
}

#[test]
fn echoes_of_chrono_trigger_are_story_rpgs_and_never_its_own_versions() {
    let ix = index(&CuratorDbFeel);
    let e = ix.echoes(find(&ix, "Chrono Trigger", "snes"), echoes::FULL_LIST);
    let n = names(&ix, &e);
    assert!(!n.iter().any(|x| x == "Chrono Trigger"), "the DS and PS1 versions are Versions, not Echoes: {n:?}");
    assert!(n[..3].contains(&"Chrono Cross".to_string()), "{n:?}");
    assert!(n[..5].iter().any(|x| x.starts_with("Final Fantasy VI")), "{n:?}");
    let racers = ["Burnout", "Gran Turismo", "Need for Speed", "Mario Kart", "F-Zero", "R4"];
    let first_racer = n.iter().position(|x| racers.iter().any(|r| x.contains(r))).unwrap_or(99);
    assert!(first_racer >= 8, "role-playing games before racers: {n:?}");
    assert_eq!(n.iter().filter(|x| x.starts_with("Final Fantasy VI")).count(), 1, "owned twice, listed once: {n:?}");
}

#[test]
fn a_game_without_data_has_no_echoes_and_no_trouble() {
    let ix = index(&CuratorDbFeel);
    let celeste = find(&ix, "Celeste Classic", "ports");
    assert!(ix.echoes(celeste, echoes::FULL_LIST).is_empty());
    assert!(!ix.has_versions(celeste) && ix.remakes(celeste).is_empty());
}

#[test]
fn reasons_use_the_vocabulary_s_words() {
    // The fixture keeps this one still when curatordb is rebuilt.
    let ix = index(&fixture::Fixture);
    let e = show(&ix, find(&ix, "Burnout Revenge", "ps2"));
    let b3 = e.iter().find(|x| ix.game(x.game).name == "Burnout 3 - Takedown").expect("Burnout 3 is an Echo");
    assert_eq!(b3.reason, "Same series · crash takedowns", "{b3:?}");
    let nfs = e.iter().find(|x| ix.game(x.game).name == "Need for Speed - Hot Pursuit 2").expect("NFS is an Echo");
    assert!(nfs.reason.contains("arcade racing") || nfs.reason.contains("Arcade racing"), "{nfs:?}");
}

#[test]
fn without_feel_tags_echoes_still_work_from_wikidata_and_the_genre_map() {
    let ix = index(&NoFeel);
    let e = show(&ix, find(&ix, "Burnout Revenge", "ps2"));
    let n = names(&ix, &e);
    assert!(n[..3].iter().all(|x| x.starts_with("Burnout")), "series and developer carry it: {n:?}");
    assert!(e.iter().take(3).all(|x| x.reason.starts_with("Same series")), "{e:?}");
    let e = show(&ix, find(&ix, "Chrono Trigger", "snes"));
    assert!(names(&ix, &e)[..2].contains(&"Chrono Cross".to_string()));
}

#[test]
fn taste_shows_up_as_like_a_game_you_finished() {
    let ix = index(&NoFeel);
    let e = show(&ix, find(&ix, "Burnout Revenge", "ps2"));
    assert!(e.iter().any(|x| x.reason.contains("ike Burnout 3 - Takedown, which you finished")), "{e:?}");
    let racer = find(&ix, "F-Zero", "snes");
    let rpg = find(&ix, "Golden Sun", "gba");
    assert!(ix.affinity(racer) > ix.affinity(rpg), "{} vs {}", ix.affinity(racer), ix.affinity(rpg));
}

#[test]
fn versions_group_by_work_and_the_preferred_one_is_shown_once() {
    let ix = index(&NoFeel);
    let snes = find(&ix, "Chrono Trigger", "snes");
    let ds = find(&ix, "Chrono Trigger", "nds");
    let ps1 = find(&ix, "Chrono Trigger", "psx");
    let mut v: Vec<usize> = ix.versions(snes).iter().map(|&x| x as usize).collect();
    v.sort();
    let mut want = vec![snes, ds, ps1];
    want.sort();
    assert_eq!(v, want, "one work on three systems");
    assert!(ix.has_versions(ds));
    assert!(!ix.has_versions(find(&ix, "Golden Sun", "gba")));
    assert_eq!(ix.preferred(snes), ds, "played longest: the DS version");
    let mut p = player();
    p.preferred.insert(ix.game(snes).work.clone().unwrap(), ix.game(snes).path.to_string_lossy().into_owned());
    let cat = ix.catalog.clone();
    let ix = Index::build(cat.clone(), user_state(&cat, &p));
    assert_eq!(ix.preferred(ps1), snes);
    assert!(ix.preferred_is_chosen(ds));
    assert_eq!(ix.merge_versions(&[ps1, ds, snes]), vec![snes]);
    assert_eq!(ix.merge_versions(&[ds, ps1]), vec![ds], "a list without the preferred version keeps its first");
    let ff_snes = find(&ix, "Final Fantasy VI", "snes");
    let ff_gba = find(&ix, "Final Fantasy VI Advance", "gba");
    assert_eq!(ix.catalog.work_of[ff_snes], ix.catalog.work_of[ff_gba], "the Advance port is the same work");
    let la = find(&ix, "The Legend of Zelda - Link's Awakening", "gb");
    assert!(ix.has_versions(la), "Link's Awakening and its DX edition");
    assert_eq!(ix.game(ff_snes).rom.translation.as_ref().map(|t| t.language.as_str()), Some("en"));
    assert_eq!(ix.game(ds).rom.describe(), "USA · English, French");
}

#[test]
fn an_edition_filed_as_its_own_item_is_a_version_of_its_work() {
    // "Wrong game? Choose another" picked the DS edition item (Q507828): still Chrono Trigger's work.
    let mut games = library();
    let ds = games.iter_mut().find(|g| g.system == "nds" && g.name == "Chrono Trigger").unwrap();
    ds.choice = Some(crate::gamedb::Choice::Entry("Q507828".into()));
    let mut db = GameDb::embedded();
    let cat = Arc::new(build_catalog(&games, systems(), &mut db, &CuratorDbFeel));
    let ix = Index::build(cat.clone(), user_state(&cat, &player()));
    let ds = find(&ix, "Chrono Trigger", "nds");
    assert_eq!(ix.game(ds).qid.as_deref(), Some("Q507828"));
    assert_eq!(ix.game(ds).work.as_deref(), Some("Q761815"));
    assert!(ix.versions(find(&ix, "Chrono Trigger", "snes")).contains(&(ds as u32)));
    assert!(ix.game(ds).terms.iter().any(|t| t.key == "t:time-travel"), "with the work's tags");
}

#[test]
fn a_remake_is_related_not_a_version() {
    let ix = index(&CuratorDbFeel);
    let n64 = find(&ix, "The Legend of Zelda - Ocarina of Time", "n64");
    let threeds = find(&ix, "The Legend of Zelda - Ocarina of Time 3D", "3ds");
    assert!(!ix.has_versions(n64), "a remake is its own work");
    assert_eq!(ix.remakes(n64), vec![threeds]);
    assert_eq!(ix.remakes(threeds), vec![n64], "found from the other side too");
    assert!(ix.remakes(find(&ix, "Golden Sun", "gba")).is_empty());
    assert!(!ix.echoes(n64, echoes::FULL_LIST).iter().any(|e| e.game == threeds), "nor an Echo: it is on the Versions page");
}

#[test]
fn lens_and_with_or_and_live_counts() {
    let ix = index(&CuratorDbFeel);
    let mut lens = Lens::default();
    assert_eq!(ix.matches(&lens).count() as usize, ix.len(), "nothing picked: everything");
    lens.toggle(Facet::Family, "racing");
    let racing = ix.matches(&lens).count();
    assert!(racing >= 20, "the racers of the library: {racing}");
    lens.toggle(Facet::System, "ps2");
    let ps2 = ix.matches(&lens).count();
    assert_eq!(ps2, 8, "Burnout ×5, Gran Turismo 4, Need for Speed, OutRun");
    lens.toggle(Facet::System, "snes");
    assert_eq!(ix.matches(&lens).count(), ps2 + 2, "PS2 or SNES racers (F-Zero, Super Mario Kart)");
    let counts = ix.counts(&lens, Facet::System, false);
    let fv = ix.facet(Facet::System);
    let count = |key: &str| counts[fv.position(key).unwrap()];
    assert_eq!((count("ps2"), count("snes"), count("gb")), (ps2, 2, 0));
    lens.toggle(Facet::Status, "completed");
    assert_eq!(ix.matches(&lens).count(), 1, "Burnout 3");
    lens.clear(Facet::Status);
    lens.toggle(Facet::Stickers, "1");
    let list = ix.list(&ix.matches(&lens), true);
    assert_eq!(list.iter().map(|&g| ix.game(g).name.as_str()).collect::<Vec<_>>(), ["Burnout 3 - Takedown", "Super Mario Kart"]);
    assert_eq!(ix.suggest_name(&lens), "PS2 or SNES · Racing · Couch co-op");
    lens.toggle(Facet::Stickers, "99");
    lens.forget(Facet::Stickers, "1");
    assert_eq!(ix.matches(&lens).count(), 0, "a sticker the index doesn't know matches nothing");
    let mut lang = Lens::default();
    lang.toggle(Facet::Language, "translation");
    assert_eq!(ix.matches(&lang).count(), 2, "Mother 3 and Final Fantasy VI");
    // The vocabulary's words on the facets.
    let shown = ix.shown_facets();
    for f in [Facet::Mechanics, Facet::Pace, Facet::Length, Facet::Session, Facet::Difficulty, Facet::Setting, Facet::Mood, Facet::Controls] {
        assert!(shown.contains(&f), "{f:?} has values");
    }
    assert_eq!(ix.facet(Facet::Players).find("couch-coop").map(|v| v.label.as_str()), Some("Co-op on one screen"));
    assert_eq!(ix.facet(Facet::Length).values[0].key, "length-short", "lengths in their order");
    assert!(!index(&NoFeel).shown_facets().contains(&Facet::Mood), "a facet without values takes no room");
    assert!(index(&NoFeel).shown_facets().contains(&Facet::Family), "the genre map gives families without tags");
}

/// The Lens examples of the report.
#[test]
fn lens_examples() {
    let ix = index(&CuratorDbFeel);
    let show = |picks: &[(Facet, &str)]| {
        let mut lens = Lens::default();
        for (f, k) in picks {
            lens.toggle(*f, k);
        }
        let list = ix.list(&ix.matches(&lens), true);
        let names: Vec<String> = list.iter().map(|&g| format!("{} ({})", ix.game(g).name, ix.catalog.system_short(&ix.game(g).system))).collect();
        println!("Lens \"{}\": {} games: {}", ix.suggest_name(&lens), ix.count(&ix.matches(&lens), true), names.join(", "));
        list.len()
    };
    // "GBA · action · short": short as in short sessions (no GBA game of the library is under 5 hours).
    assert_eq!(show(&[(Facet::System, "gba"), (Facet::Family, "action"), (Facet::Session, "session-pickup")]), 2, "Astro Boy, Mega Man Zero");
    assert_eq!(show(&[(Facet::System, "gba"), (Facet::Family, "action"), (Facet::Length, "length-short")]), 0);
    assert!(show(&[(Facet::Players, "couch-coop")]) >= 3, "co-op on one screen");
    assert!(show(&[(Facet::Players, "couch-versus"), (Facet::Family, "racing")]) >= 5);
    assert!(show(&[(Facet::Mood, "cosy")]) >= 1, "Animal Crossing");
}

#[test]
fn played_and_merged_lists() {
    let ix = index(&NoFeel);
    let fv = ix.facet(Facet::Played);
    let count = |k: &str| fv.find(k).map_or(0, |v| v.bits.count());
    assert_eq!(count("month"), 1, "Gran Turismo 4, a day ago");
    assert_eq!(count("long_ago"), 3, "Chrono Trigger DS (years ago), Burnout 3 and Pokémon (time, no date)");
    let mut lens = Lens::default();
    lens.toggle(Facet::Family, "rpg");
    lens.toggle(Facet::SubGenre, "jrpg");
    let bits = ix.matches(&lens);
    let (merged, files) = (ix.list(&bits, true).len(), ix.list(&bits, false).len());
    assert!(files >= merged + 3, "Chrono Trigger three times, Final Fantasy VI twice: {files} files, {merged} games");
    assert_eq!(ix.count(&bits, true) as usize, merged, "counted as listed");
}

/// A made-up library of `n` games over 20 systems, 40 genres, 300 series, with feel tags, for the speed tests.
fn big_catalog(n: usize) -> Arc<Catalog> {
    let genres = ["racing video game", "role-playing video game", "platform game", "puzzle video game", "fighting game", "shoot 'em up", "action game", "sports video game", "kart racing game", "Metroidvania"];
    let systems: Vec<SystemName> = (0..20).map(|s| SystemName { id: format!("sys{s}"), name: format!("System {s}"), short: format!("S{s}") }).collect();
    let features = (0..n)
        .map(|i| {
            let info = crate::gamedb::GameInfo {
                qid: format!("Q{}", i / 2 * 7 + i % 2 * (i % 5 == 0) as usize),
                label: format!("Game {i}"),
                year: Some(1985 + (i % 30) as u16),
                developers: vec![format!("Dev {}", i % 150)],
                genres: vec![genres[i % genres.len()].to_string(), genres[(i / 7) % genres.len()].to_string(), "science fiction video game".into()],
                series: if i % 3 == 0 { vec![format!("Series {}", i % 300)] } else { vec![] },
                modes: vec!["single-player video game".into()],
                ..Default::default()
            };
            let tags: Vec<features::FeelTag> = (0..8)
                .map(|k| {
                    let axis = [features::Axis::Mechanic, features::Axis::Pace, features::Axis::Length, features::Axis::Mood][k % 4];
                    features::FeelTag { axis, key: format!("tag{}", (i * 31 + k * 17) % 150), label: format!("tag {k}"), weight: (k % 3 + 1) as f32 }
                })
                .collect();
            let path = PathBuf::from(format!("/storage/roms/sys{}/Game {i} (USA).bin", i % 20));
            features_for(GameRef { at: (i % 20, i), system: &format!("sys{}", i % 20), path: &path, name: &format!("Game {i}") }, Some(&info), &tags)
        })
        .collect();
    Arc::new(Catalog::build(features, systems))
}

fn big_user(cat: &Catalog) -> UserState {
    UserState {
        games: (0..cat.len())
            .map(|i| UserGame {
                status: [Status::NotStarted, Status::Playing, Status::Completed, Status::Dropped, Status::NotStarted][i % 5],
                secs: (i % 13) as u64 * 1800,
                last_played: (i % 4 == 0).then_some(1_700_000_000 - i as u64 * 1000),
                starred: i % 17 == 0,
                stickers: if i % 11 == 0 { vec![1] } else { vec![] },
                rating: (i % 6) as u8,
            })
            .collect(),
        stickers: vec![index::StickerName { id: 1, name: "Before bed".into() }],
        now: 1_700_000_000,
        ..Default::default()
    }
}

/// CPU time the calling thread has used.
fn thread_cpu() -> Duration {
    Duration::from_secs_f64(crate::run_ahead::thread_cpu_ms() / 1000.0)
}

/// Release builds must meet the budgets of docs/CURATOR.md; debug builds only run the code.
fn budget(what: &str, took: Duration, max: Duration) {
    println!("{what}: {took:?} (budget {max:?})");
    if !cfg!(debug_assertions) {
        assert!(took <= max, "{what} took {took:?}, more than {max:?}");
    }
}

#[test]
fn speed_index_build_lens_keypress_and_echoes_for_5000_games() {
    // Everything below runs on this one thread: the budgets count its CPU time, which another test or a build on the
    // same machine can't stretch (catalog and index print the wall clock as well).
    let (t, c) = (Instant::now(), thread_cpu());
    let cat = big_catalog(5000);
    let built = thread_cpu() - c;
    println!("catalog, wall clock: {:?}", t.elapsed());
    let (t, c) = (Instant::now(), thread_cpu());
    let ix = Index::build(cat.clone(), big_user(&cat));
    let indexed = thread_cpu() - c;
    println!("index, wall clock: {:?}", t.elapsed());
    budget("catalog for 5,000 games (features already made)", built, Duration::from_millis(200));
    budget("index for 5,000 games", indexed, Duration::from_millis(50));
    assert_eq!(ix.len(), 5000);

    // A keypress in the Lens: toggle a value, then the result count, the counts of the facet on screen, and the
    // picks line of every facet on the left.
    let mut lens = Lens::default();
    lens.toggle(Facet::Family, "racing");
    lens.toggle(Facet::Status, "not_started");
    // The 90th percentile, not the single worst: with the whole suite running in parallel one sample in fifty can
    // lose its time slice to another test, which says nothing about the code (the worst stays printed).
    let mut took = Vec::new();
    for k in 0..50 {
        let t = thread_cpu();
        let facet = [Facet::System, Facet::Era, Facet::SubGenre, Facet::Mood][k % 4];
        let key = ix.facet(facet).values[k % ix.facet(facet).values.len()].key.clone();
        lens.toggle(facet, &key);
        let n = ix.matches(&lens).count();
        let counts = ix.counts(&lens, facet, true);
        let lines: Vec<String> = ix.shown_facets().into_iter().map(|f| ix.picks_line(&lens, f)).collect();
        std::hint::black_box((n, counts, lines));
        took.push(thread_cpu() - t);
    }
    budget("Lens keypress (90th percentile of 50)", percentile(&mut took, 90), Duration::from_millis(1));
    println!("Lens keypress, worst of 50: {:?}", took.last().copied().unwrap_or_default());
    // Best of three, for the same reason.
    let mut shown = Vec::new();
    let mut best = Duration::MAX;
    for _ in 0..3 {
        let t = thread_cpu();
        shown = ix.list(&ix.matches(&Lens::default()), true);
        best = best.min(thread_cpu() - t);
    }
    budget("START: the merged list of all 5,000 (best of 3)", best, Duration::from_millis(16));
    assert!(shown.len() < 5000, "versions merged");
    // The same in another order (only the list is sorted; the keypresses above don't change), worst of the orders.
    let mut worst = Duration::ZERO;
    for order in sort::Order::ALL {
        let mut best = Duration::MAX;
        for _ in 0..3 {
            let t = thread_cpu();
            std::hint::black_box(ix.sorted(&ix.matches(&Lens::default()), true, order));
            best = best.min(thread_cpu() - t);
        }
        worst = worst.max(best);
    }
    budget("START: the merged list of all 5,000 in the slowest order (best of 3)", worst, Duration::from_millis(16));
    // "Surprise me": one pick among 5,000 (on the UI thread, when its entry or Right is pressed).
    let mut rng = surprise::Rng::new(7);
    let mut took = Vec::new();
    let mut seen = Vec::new();
    for _ in 0..8 {
        let t = thread_cpu();
        let pick = ix.surprise(&mut rng, &seen).expect("a pick");
        took.push(thread_cpu() - t);
        seen.push(pick.game);
    }
    budget("Surprise me, one pick (median of 8)", percentile(&mut took, 50), Duration::from_millis(5));

    let mut took = Vec::new();
    for seed in [0, 1234, 2500, 4999, 100, 3000, 4000, 777] {
        let t = thread_cpu();
        let e = ix.echoes(seed, echoes::FULL_LIST);
        took.push(thread_cpu() - t);
        assert_eq!(e.len(), echoes::FULL_LIST);
    }
    budget("Echoes for one seed (median of 8)", percentile(&mut took, 50), Duration::from_millis(5));
    println!("Echoes, worst of 8: {:?}", took.last().copied().unwrap_or_default());

    // "Why these?" when the Echoes page opens (on the UI thread), and Up next (on the index thread).
    let mut why = Vec::new();
    let mut next = Vec::new();
    for seed in [0, 1234, 2500, 4999, 100, 3000, 4000, 777] {
        let e = ix.echoes(seed, echoes::FULL_LIST);
        let t = thread_cpu();
        std::hint::black_box(ix.why(seed, &e));
        why.push(thread_cpu() - t);
        let t = thread_cpu();
        std::hint::black_box(ix.up_next(seed));
        next.push(thread_cpu() - t);
    }
    budget("Why these? (median of 8)", percentile(&mut why, 50), Duration::from_millis(1));
    budget("Up next (median of 8, on the index thread)", percentile(&mut next, 50), Duration::from_millis(5));

    // Home's Curator row: the day's pick and the games waiting, with every new index (on the index thread).
    let mut took = Vec::new();
    for _ in 0..8 {
        let t = thread_cpu();
        std::hint::black_box(ix.today());
        took.push(thread_cpu() - t);
    }
    budget("Today on Home (median of 8, on the index thread)", percentile(&mut took, 50), Duration::from_millis(5));
}

/// The `p`th percentile of `took` (sorted in place, so `last` is the worst afterwards).
fn percentile(took: &mut [Duration], p: usize) -> Duration {
    took.sort();
    took.get((took.len() * p / 100).min(took.len().saturating_sub(1))).copied().unwrap_or_default()
}

#[test]
fn speed_catalog_from_gamedb_for_a_real_library() {
    // The thread's real work: gamedb matching for every file. The test library has 28 games over 7 systems.
    let t = Instant::now();
    let cat = catalog(&NoFeel);
    println!("catalog of {} games with gamedb: {:?}", cat.len(), t.elapsed());
    assert_eq!(cat.len(), library().len());
}


#[test]
fn story_grinding_and_reading_are_facets_only_where_the_data_knows() {
    let ix = index(&CuratorDbFeel);
    let has = |facet: Facet, key: &str, name: &str, system: &str| ix.facet(facet).find(key).is_some_and(|v| v.bits.get(find(&ix, name, system)));
    assert!(has(Facet::Story, "story-driven", "Chrono Trigger", "snes"));
    assert!(has(Facet::Story, "story-some", "Castlevania - Symphony of the Night", "psx"), "a weight-1 story tag is some story");
    assert!(!has(Facet::Story, "story-driven", "Castlevania - Symphony of the Night", "psx"));
    assert!(has(Facet::Story, "cinematic", "Final Fantasy VI", "snes") && has(Facet::Story, "lore", "Chrono Cross", "psx"));
    assert!(has(Facet::Story, "story-little", "Super Mario Kart", "snes"), "a tagged game without a story tag");
    assert!(has(Facet::Grinding, "grinding", "Pokemon - Crystal Version", "gbc"));
    assert!(has(Facet::Grinding, "grinding-not-noted", "Burnout 3 - Takedown", "ps2"));
    assert!(has(Facet::Reading, "text-heavy", "EarthBound", "snes") && has(Facet::Reading, "text-light", "Burnout 3 - Takedown", "ps2"));
    // A game without data is in none of them, so "little" never means "unknown".
    let celeste = find(&ix, "Celeste Classic", "ports");
    for f in [Facet::Story, Facet::Grinding, Facet::Reading] {
        assert!(ix.facet(f).values.iter().all(|v| !v.bits.get(celeste)), "{f:?}");
        assert_eq!(ix.facet(f).values.iter().map(|v| v.label.clone()).collect::<Vec<_>>().len(), ix.facet(f).values.len());
    }
    // "Little" values are Lens values only, never Echoes terms.
    assert!(ix.catalog.games.iter().all(|g| g.terms.iter().all(|t| !["t:story-little", "t:grinding-not-noted", "t:text-light", "t:story-some"].contains(&t.key.as_str()))));
    // Values in their order, labelled as the user guide names them (`Facet::fixed_labels`).
    let labels = |f: Facet| ix.facet(f).values.iter().map(|v| v.label.clone()).collect::<Vec<_>>();
    assert_eq!(labels(Facet::Story), ["Story-driven", "Some story", "Cinematic", "Rich world and lore", "Little story"]);
    assert_eq!(labels(Facet::Grinding), ["Grinding", "Not known for grinding"]);
    assert_eq!(labels(Facet::Reading), ["Little reading", "Lots of reading"]);
    for f in [Facet::Story, Facet::Grinding, Facet::Reading, Facet::Length] {
        assert!(labels(f).iter().all(|l| f.fixed_labels().contains(l)), "{f:?}: {:?} in {:?}", labels(f), f.fixed_labels());
    }
}

#[test]
fn story_little_grinding_and_a_language_narrow_down_like_any_lens() {
    // "PS1 · story · little grinding · English" as a player would ask it.
    let ix = index(&CuratorDbFeel);
    let mut lens = Lens::default();
    lens.toggle(Facet::System, "psx");
    lens.toggle(Facet::Story, "story-driven");
    lens.toggle(Facet::Story, "story-some");
    lens.toggle(Facet::Grinding, "grinding-not-noted");
    lens.toggle(Facet::Language, "en");
    let games: Vec<&str> = ix.list(&ix.matches(&lens), true).into_iter().map(|g| ix.game(g).name.as_str()).collect();
    assert_eq!(games, ["Castlevania - Symphony of the Night", "Chrono Cross"], "the Japanese Chrono Trigger is not in English");
    assert_eq!(ix.suggest_name(&lens), "PS1 · story-driven or some story · not known for grinding …");
    lens.toggle(Facet::Reading, "text-light");
    assert_eq!(ix.list(&ix.matches(&lens), true).len(), 1, "Symphony of the Night: little reading");
}

#[test]
fn length_reads_neighbouring_steps_as_a_range() {
    use super::lens::length_range;
    assert_eq!(length_range(&["length-short", "length-medium"]).as_deref(), Some("under 15 hours"));
    assert_eq!(length_range(&["length-medium", "length-long"]).as_deref(), Some("5–40 hours"));
    assert_eq!(length_range(&["length-long", "length-epic", "length-medium"]).as_deref(), Some("5+ hours"));
    assert_eq!(length_range(&["length-short", "length-medium", "length-long", "length-epic"]).as_deref(), Some("any length"));
    assert_eq!(length_range(&["length-short"]), None, "one step is its own label");
    assert_eq!(length_range(&["length-short", "length-long"]), None, "a gap is two values");
    let ix = index(&CuratorDbFeel);
    let mut lens = Lens::default();
    lens.toggle(Facet::Length, "length-medium");
    lens.toggle(Facet::Length, "length-short");
    assert_eq!(ix.suggest_name(&lens), "Under 15 hours", "\"under ~8 h\" as the data can answer it");
    assert_eq!(ix.picks_line(&lens, Facet::Length), "under 15 hours");
    lens.toggle(Facet::Length, "length-endless");
    assert_eq!(ix.suggest_name(&lens), "Under 15 hours or endless");
    assert_eq!(ix.matches(&lens).count(), 2 + 30 + 15, "OR within the facet");
    lens.toggle(Facet::Length, "length-medium");
    assert_eq!(ix.suggest_name(&lens), "Under 5 hours or endless");
}

/// The player of [`player`], with Burnout Legends played an hour ago.
fn index_with(edit: impl FnOnce(&mut Player)) -> Index {
    let cat = catalog(&CuratorDbFeel);
    let mut p = player();
    edit(&mut p);
    let user = user_state(&cat, &p);
    Index::build(cat, user)
}

#[test]
fn up_next_is_the_first_echo_you_might_play_next() {
    let ix = index(&CuratorDbFeel);
    let revenge = find(&ix, "Burnout Revenge", "ps2");
    let e = ix.up_next(revenge).expect("an Echo to suggest");
    assert_eq!(ix.game(e.game).name, "Burnout Legends");
    assert!(e.reason.starts_with("Same series"), "{e:?}");
    // Never a game you completed (Burnout 3 is an Echo of it, and high), dropped, or the same work.
    for seed in 0..ix.len() {
        let Some(e) = ix.up_next(seed) else { continue };
        let u = &ix.user.games[e.game];
        assert!(!matches!(u.status, Status::Completed | Status::Dropped), "{}: {}", ix.game(seed).name, ix.game(e.game).name);
        assert_ne!(ix.catalog.work_of[e.game], ix.catalog.work_of[seed], "{}", ix.game(seed).name);
        assert!(e.score >= echoes::UP_NEXT_MIN);
        assert!(ix.echoes(seed, echoes::FULL_LIST).iter().any(|x| x.game == e.game), "an Echo of it");
    }
    assert!(ix.up_next(find(&ix, "Celeste Classic", "ports")).is_none(), "nothing alike, nothing suggested");
    // Played an hour ago: not suggested again (no back and forth between two games).
    let ix = index_with(|p| {
        p.recents.insert(path("psp", "Burnout Legends (USA).iso"), (p.now - 3600, 1800));
    });
    let e = ix.up_next(find(&ix, "Burnout Revenge", "ps2")).unwrap();
    assert_ne!(ix.game(e.game).name, "Burnout Legends");
    assert_ne!(ix.game(e.game).name, "Burnout 3 - Takedown", "completed");
}

#[test]
fn why_these_names_the_games_of_your_taste_only_when_they_changed_the_order() {
    let ix = index(&CuratorDbFeel);
    let seed = find(&ix, "Ridge Racer", "psp");
    let e = ix.echoes(seed, echoes::FULL_LIST);
    assert_eq!(ix.why(seed, &e).as_deref(), Some("You finished Burnout 3 - Takedown"));
    assert!(e.iter().any(|x| x.lift > 0.0));
    // Taste is only claimed where it put a game above one that would be above it without taste.
    for seed in 0..ix.len() {
        let e = ix.echoes(seed, echoes::FULL_LIST);
        let swapped = e.iter().enumerate().any(|(i, a)| a.lift > 0.0 && e[i + 1..].iter().any(|b| a.score >= b.score && a.score - a.lift < b.score - b.lift));
        assert!(ix.why(seed, &e).is_none() || swapped, "{}", ix.game(seed).name);
    }
    // No player, no taste, no line.
    let cat = catalog(&CuratorDbFeel);
    let blank = Index::build(cat.clone(), user_state(&cat, &Player::default()));
    for seed in 0..blank.len() {
        let e = blank.echoes(seed, echoes::FULL_LIST);
        assert!(e.iter().all(|x| x.lift == 0.0));
        assert_eq!(blank.why(seed, &e), None);
    }
}

#[test]
fn the_lens_row_says_what_it_looks_through_before_anything_is_picked() {
    assert_eq!(lens::overview(332, 9), "332 games · 9 filters");
    assert_eq!(lens::overview(1, 1), "1 game · 1 filter");
    assert_eq!(lens::overview(0, 0), "0 games", "an empty library: still a figure, not a blank");
}

#[test]
fn my_rating_is_a_lens_facet_with_not_rated_before_the_first_rating() {
    // No ratings yet: the facet is there with "Not rated" for every game, and its note says how to give one.
    let ix = index(&CuratorDbFeel);
    let fv = ix.facet(Facet::Rating);
    assert_eq!(fv.values.iter().map(|v| v.label.as_str()).collect::<Vec<_>>(), ["Not rated"]);
    assert_eq!(fv.values[0].bits.count() as usize, ix.len());
    assert!(ix.shown_facets().contains(&Facet::Rating));
    assert!(Facet::Rating.note().unwrap().contains("on its card"));
    // Two games rated 5, one 4, one 1: values high to low, then Not rated; "5 or 4" reads as one.
    let ix = index_with(|p| {
        for (sys, file, r) in [("snes", "Super Metroid (Japan, USA) (En,Ja).sfc", 5), ("gba", "Metroid Fusion (USA).gba", 5), ("ps2", "Burnout 3 - Takedown (USA).iso", 4), ("gb", "Tetris (World) (Rev 1).gb", 1)] {
            p.meta.entry(path(sys, file).to_string_lossy().into_owned()).or_default().rating = r;
        }
    });
    let fv = ix.facet(Facet::Rating);
    assert_eq!(fv.values.iter().map(|v| v.label.as_str()).collect::<Vec<_>>(), ["5 stars", "4 stars", "1 star", "Not rated"]);
    let mut lens = Lens::default();
    lens.toggle(Facet::Rating, "5");
    assert_eq!(ix.matches(&lens).count(), 2);
    lens.toggle(Facet::Rating, "4");
    assert_eq!(ix.matches(&lens).count(), 3);
    assert_eq!(ix.suggest_name(&lens), "Rated 4 and up");
    lens.toggle(Facet::Family, "platformer");
    lens.toggle(Facet::Family, "action-adventure");
    let n = ix.matches(&lens).count();
    assert!((1..=3).contains(&n), "intersects like any facet: {n}");
    assert_eq!(lens::rating_range(&["5", "3"]), None, "a gap: said one by one");
    assert_eq!(lens::rating_range(&["4"]), None);
    assert_eq!(lens::rating_range(&["3", "4", "5", "unrated"]).as_deref(), Some("rated 3 and up"));
    // The rating counts in the taste profile: 5 stars as much as a star, 1 star against.
    let u = |rating| UserGame { rating, ..Default::default() };
    assert_eq!((index::taste_weight(&u(5)), index::taste_weight(&u(3)), index::taste_weight(&u(1)), index::taste_weight(&u(0))), (1.0, 0.0, -1.0, 0.0));
}

#[test]
fn sort_orders_put_what_was_asked_first_and_games_without_the_figure_last_by_name() {
    use sort::Order;
    let ix = index_with(|p| {
        p.meta.entry(path("snes", "Super Metroid (Japan, USA) (En,Ja).sfc").to_string_lossy().into_owned()).or_default().rating = 5;
        p.meta.entry(path("gb", "Tetris (World) (Rev 1).gb").to_string_lossy().into_owned()).or_default().rating = 2;
        p.recents.insert(path("gb", "Tetris (World) (Rev 1).gb"), (p.now - 100, 600));
    });
    let all = ix.matches(&Lens::default());
    let names = |order: Order| ix.sorted(&all, true, order).into_iter().map(|g| ix.game(g).name.clone()).collect::<Vec<_>>();
    let by_name = names(Order::Name);
    assert_eq!(by_name, ix.list(&all, true).into_iter().map(|g| ix.game(g).name.clone()).collect::<Vec<_>>(), "by name is the old order");
    // Last played: Tetris (100 s ago), Gran Turismo 4, then Chrono Trigger (merged: the DS version was played); then the
    // never played by name.
    let last = names(Order::LastPlayed);
    assert_eq!(&last[..3], ["Tetris", "Gran Turismo 4", "Chrono Trigger"]);
    let rest: Vec<&String> = last[3..].iter().collect();
    let mut sorted_rest = rest.clone();
    sorted_rest.sort_by_key(|n| by_name.iter().position(|b| b == *n));
    assert_eq!(rest, sorted_rest, "the never played keep the order by name");
    // Most played: Gran Turismo 4 (30 h), Burnout 3 (14 h), Chrono Trigger (3 h), Pokémon (2 h), Tetris (10 min).
    assert_eq!(&names(Order::PlayTime)[..5], ["Gran Turismo 4", "Burnout 3 - Takedown", "Chrono Trigger", "Pokemon - Emerald Version", "Tetris"]);
    assert_eq!(&names(Order::Rating)[..2], ["Super Metroid", "Tetris"]);
    // Oldest first: every game with a year before every game without one, years rising.
    let years: Vec<Option<u16>> = ix.sorted(&all, true, Order::Year).into_iter().map(|g| ix.game(g).year).collect();
    let known = years.iter().take_while(|y| y.is_some()).count();
    assert!(known > 50 && years[known..].iter().all(Option::is_none), "{years:?}");
    assert!(years[..known].windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(names(Order::Year).last().map(String::as_str), Some("Celeste Classic"), "the port without data last");
    // Shortest first: the buckets in order, unknown last.
    let bucket = |g: usize| ["length-short", "length-medium", "length-long", "length-epic", "length-endless"].iter().position(|k| ix.game(g).facets.iter().any(|(f, key, _)| *f == Facet::Length && key == k));
    let lengths: Vec<Option<usize>> = ix.sorted(&all, true, Order::Length).into_iter().map(bucket).collect();
    let known = lengths.iter().take_while(|l| l.is_some()).count();
    assert!(known > 20 && lengths[known..].iter().all(Option::is_none), "{lengths:?}");
    assert!(lengths[..known].windows(2).all(|w| w[0] <= w[1]));
    // Every order lists the same games, only in another order.
    for order in Order::ALL {
        let mut n = names(order);
        n.sort();
        let mut b = by_name.clone();
        b.sort();
        assert_eq!(n, b, "{order:?}");
        assert!(order.suffix().is_some() != (order == Order::Name));
    }
    // An empty library sorts to nothing, in any order.
    let empty = Index::build(Arc::new(Catalog::build(Vec::new(), Vec::new())), UserState::default());
    for order in Order::ALL {
        assert!(empty.sorted(&empty.matches(&Lens::default()), true, order).is_empty());
    }
}

#[test]
fn surprise_me_is_seeded_weighted_by_status_and_recency_and_says_why() {
    use surprise::Rng;
    let ix = index(&CuratorDbFeel);
    // The same seed, the same picks.
    let picks = |seed: u64| {
        let mut rng = Rng::new(seed);
        (0..5).map(|_| ix.surprise(&mut rng, &[]).unwrap().game).collect::<Vec<_>>()
    };
    assert_eq!(picks(42), picks(42));
    assert_ne!(picks(42), picks(43));
    // Finished and dropped games come up rarely; one played in the last day hardly ever.
    let ix = index_with(|p| {
        p.recents.insert(path("psp", "Lumines (USA).iso"), (p.now - 3600, 1800));
    });
    let mut rng = Rng::new(1);
    let mut times: HashMap<usize, u32> = HashMap::new();
    for _ in 0..20_000 {
        *times.entry(ix.surprise(&mut rng, &[]).unwrap().game).or_default() += 1;
    }
    let of = |name: &str, sys: &str| times.get(&find(&ix, name, sys)).copied().unwrap_or(0);
    let unplayed = of("Super Metroid", "snes");
    assert!(unplayed > 100, "an unplayed game comes up: {unplayed}");
    assert!(of("Burnout 3 - Takedown", "ps2") * 3 < unplayed, "finished: rarer");
    assert!(of("Pokemon - Emerald Version", "gba") * 6 < unplayed, "dropped: much rarer");
    assert!(of("Lumines", "psp") * 4 < unplayed, "played an hour ago: rarer");
    // Each work once: Chrono Trigger on three systems comes up as one game, its preferred version.
    let chrono: Vec<usize> = ["snes", "psx", "nds"].iter().map(|s| find(&ix, "Chrono Trigger", s)).collect();
    assert_eq!(chrono.iter().filter(|g| times.contains_key(g)).count(), 1);
    // "Another" brings a game not shown yet, until every one was; then it starts over.
    let mut rng = Rng::new(9);
    let mut seen = Vec::new();
    let works = ix.catalog.works.len();
    for _ in 0..works {
        let pick = ix.surprise(&mut rng, &seen).unwrap();
        assert!(!seen.iter().any(|s| ix.catalog.work_of[*s] == ix.catalog.work_of[pick.game]), "{} again", ix.game(pick.game).name);
        seen.push(pick.game);
    }
    assert!(ix.surprise(&mut rng, &seen).is_some(), "all shown: from the start again");
    // The reasons say what is true of the pick.
    let gt4 = find(&ix, "Gran Turismo 4", "ps2");
    assert_eq!(ix.surprise_reason(gt4), "Started, played yesterday · you like sim racing games", "30 hours of it shaped the taste");
    let ct = find(&ix, "Chrono Trigger", "nds");
    assert_eq!(ix.surprise_reason(ct), "Not played for 22 years · you like Japanese RPGs", "the starred SNES version shaped the taste");
    let metroid = find(&ix, "Super Metroid", "snes");
    assert!(ix.surprise_reason(metroid).starts_with("Never played"), "{}", ix.surprise_reason(metroid));
    // Taste is said only where the profile leans to the game: an arcade racer after finishing Burnout 3.
    let ridge = find(&ix, "Ridge Racer", "psp");
    let why = ix.surprise_reason(ridge);
    assert!(why.starts_with("Never played · you like ") && why.ends_with(" games"), "{why}");
    // A rating of 4 or 5 is said instead.
    let ix5 = index_with(|p| {
        p.meta.entry(path("snes", "Super Metroid (Japan, USA) (En,Ja).sfc").to_string_lossy().into_owned()).or_default().rating = 5;
    });
    assert_eq!(ix5.surprise_reason(find(&ix5, "Super Metroid", "snes")), "Never played · you gave it 5 stars");
    // No player at all (Curator knows nothing of your taste): picks and reasons still work, without "you like".
    let cat = catalog(&CuratorDbFeel);
    let blank = Index::build(cat.clone(), user_state(&cat, &Player::default()));
    let mut rng = Rng::new(3);
    for _ in 0..50 {
        let p = blank.surprise(&mut rng, &[]).unwrap();
        assert_eq!(p.reason, "Never played");
    }
    // An empty library: nothing to pick, no panic.
    let empty = Index::build(Arc::new(Catalog::build(Vec::new(), Vec::new())), UserState::default());
    assert_eq!(empty.surprise(&mut rng, &[]), None);
}

#[test]
fn surprise_reasons_read_in_plain_words() {
    let day = 24 * 3600;
    assert_eq!(surprise::since(3600), "Played today");
    assert_eq!(surprise::since(day + 5), "Played yesterday");
    assert_eq!(surprise::since(5 * day), "Played 5 days ago");
    assert_eq!(surprise::since(20 * day), "Not played for 2 weeks");
    assert_eq!(surprise::since(100 * day), "Not played for 3 months");
    assert_eq!(surprise::since(800 * day), "Not played for 2 years");
    assert_eq!(surprise::kind_of_games("platformer"), "platformer games");
    assert_eq!(surprise::kind_of_games("Japanese RPG"), "Japanese RPGs");
    assert_eq!(surprise::kind_of_games("party game"), "party games");
}

#[test]
fn a_game_its_checksum_named_is_matched_under_that_name_and_the_file_name_still_counts() {
    let mut db = GameDb::embedded();
    let game = |file: &str, identified: Option<&str>| LibraryGame {
        at: (0, 0),
        system: "gba".into(),
        path: path("gba", file),
        name: file.into(),
        choice: None,
        identified: identified.map(str::to_string),
    };
    let games = vec![
        // A scene name nothing matches by itself, recognised by its checksum.
        game("0144 - aos-final.gba", Some("Castlevania - Aria of Sorrow (USA)")),
        // Recognised under a name the game list doesn't know: the file name is used.
        game("Advance Wars (USA) (Rev 1).gba", Some("Some Prototype (USA) (Proto)")),
        game("0144 - aos-final.gba", None),
    ];
    let c = build_catalog(&games, systems(), &mut db, &NoFeel);
    let castlevania = db.lookup("gba", "Castlevania - Aria of Sorrow (USA)").map(|g| g.qid);
    assert!(castlevania.is_some());
    assert_eq!(c.games[0].qid, castlevania, "by its checksum's name");
    assert_eq!(c.games[1].qid, db.lookup("gba", "Advance Wars (USA)").map(|g| g.qid), "the file name after it");
    assert_eq!(c.games[2].qid, None, "not recognised: the file name alone finds nothing");
}

#[test]
fn today_is_one_pick_a_day_and_the_games_waiting_without_it() {
    use today::{Waiting, WAITING_SHOWN};
    const DAY: u64 = 24 * 3600;
    let on = |day: u64| index_with(move |p| p.now = 1_700_100_000 + day * DAY);
    // The same day, the same pick (between starts too: the seed is the day); another day, mostly another.
    let first = on(0).today();
    assert_eq!(first, on(0).today());
    assert!(first.pick.is_some());
    assert!((1..6).any(|d| on(d).today().pick.map(|p| p.game) != first.pick.as_ref().map(|p| p.game)), "the pick changes with the day");
    // Chrono Trigger on the DS, played 22 years ago and not finished, is to rediscover (once, as its work); the
    // finished, dropped and recent games are not; the pick is never among the covers.
    for d in 0..6 {
        let ix = on(d);
        let t = ix.today();
        let pick = t.pick.as_ref().unwrap();
        assert_eq!(pick.reason, ix.surprise_reason(pick.game), "the pick says why, as Surprise me does");
        let (kind, n, shown) = t.waiting.clone().expect("something waits");
        assert!(shown.len() <= WAITING_SHOWN && shown.len() <= n);
        assert!(!shown.iter().any(|g| ix.catalog.work_of[*g] == ix.catalog.work_of[pick.game]), "day {d}: the pick twice");
        let chrono = ix.catalog.work_of[find(&ix, "Chrono Trigger", "nds")];
        if ix.catalog.work_of[pick.game] == chrono {
            // Nothing else is to rediscover: the games never started wait instead.
            assert_eq!(kind, Waiting::NotStarted);
            continue;
        }
        assert_eq!((kind, n), (Waiting::Rediscover, 1), "day {d}");
        assert_eq!(ix.catalog.work_of[shown[0]], chrono);
    }
    // Gran Turismo 4, last played a month and a day ago and still Playing, waits too.
    let later = index_with(|p| p.now = 1_700_000_000 + 31 * DAY);
    let (kind, n, _) = later.today().waiting.unwrap();
    assert_eq!((kind, n), (Waiting::Rediscover, 2), "the pick counts when it waits too");
    // A player Curator knows nothing of: every game but the pick waits to be started, the likeliest first.
    let cat = catalog(&CuratorDbFeel);
    let blank = Index::build(cat.clone(), user_state(&cat, &Player::default()));
    let t = blank.today();
    let (kind, n, shown) = t.waiting.unwrap();
    // The count is every work never played, the pick included: what the Lens's Played → Never says (which counts each
    // version of a game; Home counts a game once).
    assert_eq!((kind, n, shown.len()), (Waiting::NotStarted, cat.works.len(), WAITING_SHOWN));
    let never = blank.facet(Facet::Played).find("never").map_or(0, |v| v.bits.count() as usize);
    assert_eq!(never, cat.games.len(), "Lens: every game never played");
    // No games: nothing to pick, nothing waits, no panic.
    let empty = Index::build(Arc::new(Catalog::build(Vec::new(), Vec::new())), UserState::default());
    assert_eq!(empty.today(), today::Today::default());
}
