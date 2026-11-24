use pocket_ui::{
    cabinet::{self, FormFactor},
    document_library,
    eject::{Screen, Way},
    insert_rig, insert_scene,
    media::{Game, Medium},
    ui_sound::{self, Sound},
};
use std::time::Duration;

fn start(system: &str, file: &str, playful: bool) -> Vec<insert_scene::Cue> {
    let game = Game::of(system, file);
    let rig = mechanism(system, game, playful);
    insert_scene::cues(true, Medium::of(system), game, FormFactor::of(system), rig, playful)
}

fn mechanism(system: &str, game: Game, playful: bool) -> Option<insert_rig::Kind> {
    playful.then(|| insert_rig::moving_kind(system, game)).flatten()
}

fn way(system: &str, file: &str, playful: bool) -> Way {
    let lid = mechanism(system, Game::of(system, file), playful).is_some();
    Way::new(Screen::of(FormFactor::of(system)), true, lid)
}

fn back(system: &str, file: &str, playful: bool) -> Vec<(Sound, Duration)> {
    let game = Game::of(system, file);
    way(system, file, playful).sounds(Medium::of(system), game, FormFactor::of(system), mechanism(system, game, playful))
}

const HANDHELD_CARTS: &[(&str, &str)] = &[
    ("gb", "gb"), ("gbh", "gb"), ("gbc", "gbc"), ("gbch", "gbc"), ("gba", "gba"), ("gbah", "gba"), ("gbav", "gba"),
    ("gamegear", "gg"), ("ggh", "gg"), ("atarilynx", "lnx"), ("lynx", "lnx"), ("ngp", "ngp"), ("ngpc", "ngc"),
    ("wonderswan", "ws"), ("wonderswancolor", "wsc"), ("virtualboy", "vb"), ("pokemini", "min"),
    ("supervision", "sv"), ("gamate", "bin"), ("gameandwatch", "mgw"), ("arduboy", "hex"),
    ("gp32", "fxe"), ("megaduck", "bin"), ("palm", "prc"),
];

#[test]
fn handheld_cartridges_have_handheld_insert_and_eject_sounds() {
    let mut mismatches = Vec::new();
    for &(system, ext) in HANDHELD_CARTS {
        let file = format!("game.{ext}");
        let launch = start(system, &file, true)[1].sound;
        let eject = back(system, &file, true)[1].0;
        if (launch, eject) != (Sound::HandheldCartIn, Sound::HandheldCartOut) {
            mismatches.push((system, launch, eject));
        }
    }
    assert!(mismatches.is_empty(), "handheld cartridges with wrong cues: {mismatches:?}");
}

#[test]
fn the_psp_door_shuts_with_its_own_cue() {
    let close = start("psp", "game.iso", true)[2];
    assert_eq!(close.sound, Sound::UmdShut);
    let gamecube = start("gamecube", "game.iso", true)[2];
    assert_eq!(close.after + ui_sound::hit_at(close.sound), gamecube.after + ui_sound::hit_at(gamecube.sound));
}

#[test]
fn the_psp_door_opens_once_at_the_visible_movement() {
    let clicks: Vec<_> = back("psp", "game.cso", true)
        .into_iter()
        .filter(|(sound, _)| matches!(sound, Sound::LidOpen | Sound::UmdOpen))
        .collect();
    let time = way("psp", "game.cso", true).back_at(cabinet::IN_AT_COZY + insert_rig::SHUT_AFTER);
    assert_eq!(clicks, [(Sound::UmdOpen, time)]);
    assert!(back("psp", "game.cso", false).iter().all(|(sound, _)| *sound != Sound::UmdOpen));
}

#[test]
fn jaguar_cartridges_and_the_aes_use_home_cartridge_cues() {
    for (system, file) in [("atarijaguar", "Tempest 2000.j64"), ("jaguar", "Tempest 2000.jag"), ("neogeoaes", "game.zip")] {
        assert_eq!(start(system, file, true)[1].sound, Sound::CartIn, "{system}/{file}");
        assert!(back(system, file, true).iter().any(|(s, _)| *s == Sound::CartOut), "{system}/{file}");
    }
}

#[test]
fn the_ritual_uses_the_games_medium_for_every_packaged_system_and_extension() {
    use Game::*;
    let brick = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("deploy/brick/es_systems.cfg");
    let mut systems: Vec<_> = document_library::SYSTEM_IDS.iter()
        .map(|&id| (id.to_owned(), document_library::takes(id).iter().map(|&ext| ext.to_owned()).collect::<Vec<_>>()))
        .collect();
    systems.extend(pocket_ui::library::all_systems(&brick).unwrap().into_iter().map(|row| (row.id, row.extensions)));
    systems.extend(HANDHELD_CARTS.iter().map(|&(id, ext)| (id.into(), vec![ext.into()])));
    systems.extend(insert_rig::rig_ids().map(|id| (id.to_owned(), vec!["rom".into()])));

    for (system, extensions) in systems {
        let system = system.as_str();
        for ext in extensions {
            for region in ["USA", "Europe", "Canada", "Japan"] {
                let file = format!("Game ({region}).{ext}");
                let game = Game::of(system, &file);
                for playful in [false, true] {
                    let lid = way(system, &file, playful).lid;
                    let expected = match game {
                        Gb | Gbc | Gba | GameGear | Lynx | NeoGeoPocket | WonderSwan => Sound::HandheldCartIn,
                        Cart if FormFactor::of(system) == FormFactor::HandheldCart => Sound::HandheldCartIn,
                        Card | Card3ds | Memory => Sound::CardIn,
                        Nes if lid => Sound::NesSlide,
                        Disc | MiniDisc | Umd if lid => Sound::DiscDrop,
                        Disc | MiniDisc | Umd => Sound::DiscIn,
                        _ => Sound::CartIn,
                    };
                    assert_eq!(start(system, &file, playful)[1].sound, expected, "{system}/{file}, Playful {playful}");
                    let cues = back(system, &file, playful);
                    let removal = cues.iter().find(|(sound, _)| matches!(sound, Sound::CartOut | Sound::HandheldCartOut | Sound::CardOut)).map(|(sound, _)| *sound);
                    assert_eq!(removal, match expected {
                        Sound::CartIn | Sound::NesSlide => Some(Sound::CartOut),
                        Sound::HandheldCartIn => Some(Sound::HandheldCartOut),
                        Sound::CardIn => Some(Sound::CardOut),
                        _ => None,
                    }, "{system}/{file}, Playful {playful}");
                    let opens = cues.iter().filter(|(sound, _)| matches!(sound, Sound::LidOpen | Sound::UmdOpen | Sound::TrayOpen)).count();
                    assert_eq!(opens, usize::from(lid), "{system}/{file}, Playful {playful}");
                }
            }
        }
    }
}


#[test]
fn media_without_a_new_recording_keep_the_same_cues() {
    for (system, file, insert, remove) in [
        ("fds", "game.fds", Sound::CartIn, Some(Sound::CartOut)),
        ("64dd", "game.ndd", Sound::CartIn, Some(Sound::CartOut)),
        ("wii", "game.wbfs", Sound::DiscIn, None),
        ("pspminis", "game.pbp", Sound::CardIn, Some(Sound::CardOut)),
    ] {
        assert_eq!(start(system, file, true)[1].sound, insert, "{system}");
        let way = back(system, file, true);
        let got = way.iter().find(|(s, _)| matches!(s, Sound::CartOut | Sound::CardOut));
        assert_eq!(got.map(|(s, _)| *s), remove, "{system}");
    }
    assert_eq!(start("nes", "game.nes", false).iter().map(|cue| cue.sound).collect::<Vec<_>>(), [Sound::BoxOpen, Sound::CartIn]);
    assert!(back("nes", "game.nes", true).iter().any(|(s, _)| *s == Sound::LidOpen));
}

#[test]
fn the_nes_front_loader_has_distinct_slide_press_and_close_sounds() {
    let launch = start("nes", "game.nes", true);
    let hits: Vec<_> = launch[1..].iter().map(|cue| {
        (cue.sound, (cue.after + ui_sound::hit_at(cue.sound) + Duration::from_millis(10)).as_millis())
    }).collect();
    assert_eq!(hits, [(Sound::NesSlide, 1090), (Sound::NesPress, 1200), (Sound::NesDoorShut, 1480)]);
}

#[test]
fn motorized_trays_use_the_same_mechanism_in_both_directions() {
    for (system, file) in [("xbox", "game.iso"), ("ps2", "game.chd")] {
        let close = start(system, file, true)[2];
        assert_eq!(close.sound, Sound::TrayShut, "{system}");
        let shut = cabinet::IN_AT_COZY + insert_rig::SHUT_AFTER;
        assert_eq!(close.after + ui_sound::hit_at(close.sound) + Duration::from_millis(10), shut, "{system}");
        let opening = back(system, file, true).into_iter().find(|(sound, _)| *sound == Sound::TrayOpen);
        assert_eq!(opening, Some((Sound::TrayOpen, way(system, file, true).back_at(shut))), "{system}");
    }
}

#[test]
fn jewel_and_dvd_cases_have_separate_open_and_close_recordings() {
    for (system, file, open, close) in [
        ("psx", "game.chd", Sound::JewelOpen, Sound::JewelShut),
        ("gamecube", "game.rvz", Sound::DvdOpen, Sound::DvdShut),
    ] {
        let opening = start(system, file, true)[0];
        assert_eq!(opening.sound, open, "{system}");
        assert_eq!(opening.after + ui_sound::hit_at(open) + Duration::from_millis(10), insert_rig::case_opens_at(), "{system}");
        assert_eq!(back(system, file, true).last(), Some(&(close, way(system, file, true).case_shut_at())), "{system}");
    }
}
