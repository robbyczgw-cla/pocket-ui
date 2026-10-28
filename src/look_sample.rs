//! What the look step's previews show: a few systems and one game with box art, from the real library when it
//! has games, else from a small built-in set with plain names ("Sample game"), never ones that pass for real
//! commercial games. The app paints its real shelf with this, once per look, and scales it down.

use std::path::{Path, PathBuf};

use crate::library::{Game, System};
use crate::recents::Recent;

/// The box art of the sample game.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Cover {
    /// Its downloaded cover.
    File(PathBuf),
    /// None on the card: the app draws a plain box with the console and the name.
    Drawn,
}

#[derive(Clone, Debug)]
pub struct Sample {
    /// The systems on the previewed shelf.
    pub systems: Vec<System>,
    /// The game shown first ("Continue"), as a recent game.
    pub game: Recent,
    pub cover: Cover,
    /// True for the built-in set.
    pub demo: bool,
    /// The game is on the recent list (or it is the built-in one): the shelf shows it as the one to continue.
    /// Otherwise it is shown as a game of its system that was not played yet.
    pub played: bool,
}

/// The built-in systems: (id, full name, number of games). All have a console picture.
pub const DEMO: [(&str, &str, usize); 3] = [("gba", "Game Boy Advance", 3), ("snes", "Super Nintendo", 2), ("megadrive", "Mega Drive", 4)];

/// Picks the sample. `systems`: the library; `hidden`: systems left off the shelf; `recents`: newest first;
/// `has_cover(system, rom)`: whether that game's cover is on the card.
pub fn pick(systems: &[System], hidden: &[String], recents: &[Recent], has_cover: impl Fn(&str, &Path) -> Option<PathBuf>) -> Sample {
    let shown: Vec<System> = systems.iter().filter(|s| !s.games.is_empty() && !hidden.contains(&s.name)).cloned().collect();
    if shown.is_empty() {
        return demo();
    }
    let in_library = |system: &str, path: &Path| shown.iter().any(|s| s.name == system && s.games.iter().any(|g| g.path == path));
    let played: Vec<&Recent> = recents.iter().filter(|r| in_library(&r.system, &r.path)).collect();
    let all: Vec<(&System, &Game)> = shown.iter().flat_map(|s| s.games.iter().map(move |g| (s, g))).collect();
    let as_recent = |s: &System, g: &Game| Recent { system: s.name.clone(), name: g.name.clone(), path: g.path.clone(), last_played: 0, play_secs: 0 };
    // A played game with box art, any game with box art, the last played, the first one.
    let (game, cover) = played
        .iter()
        .find_map(|r| has_cover(&r.system, &r.path).map(|c| ((*r).clone(), Cover::File(c))))
        .or_else(|| all.iter().find_map(|(s, g)| has_cover(&s.name, &g.path).map(|c| (as_recent(s, g), Cover::File(c)))))
        .or_else(|| played.first().map(|r| ((*r).clone(), Cover::Drawn)))
        .unwrap_or_else(|| (as_recent(all[0].0, all[0].1), Cover::Drawn));
    // A game nobody played yet still reads as one played a little while ago.
    // A game from the recent list is shown as the one to continue; any other as a game not played yet.
    let played = recents.iter().any(|r| r.path == game.path);
    Sample { systems: shown, game, cover, demo: false, played }
}

/// The built-in set: three systems with consoles, games called "Sample game", "Sample game 2" and so on.
fn demo() -> Sample {
    let systems: Vec<System> = DEMO
        .iter()
        .map(|(id, name, n)| System {
            name: id.to_string(),
            fullname: name.to_string(),
            emulator: None,
            core: None,
            emulators: Vec::new(),
            games: (1..=*n)
                .map(|i| {
                    let title = if i == 1 { "Sample game".to_string() } else { format!("Sample game {i}") };
                    game(&title, &format!("/pocket-ui-sample/{id}/{title}"))
                })
                .collect(),
        })
        .collect();
    let first = &systems[0].games[0];
    let game = Recent {
        system: systems[0].name.clone(),
        name: first.name.clone(),
        path: first.path.clone(),
        last_played: crate::recents::now_secs().saturating_sub(2 * 3600),
        play_secs: 40 * 60,
    };
    Sample { systems, game, cover: Cover::Drawn, demo: true, played: true }
}

fn game(name: &str, path: &str) -> Game {
    Game { name: name.into(), path: path.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(id: &str, games: &[&str]) -> System {
        System {
            name: id.into(),
            fullname: id.to_uppercase(),
            emulator: None,
            core: None,
            emulators: Vec::new(),
            games: games.iter().map(|g| game(g, &format!("/storage/roms/{id}/{g}.rom"))).collect(),
        }
    }

    fn recent(system: &str, name: &str) -> Recent {
        Recent { system: system.into(), name: name.into(), path: format!("/storage/roms/{system}/{name}.rom").into(), last_played: 1, play_secs: 60 }
    }

    fn no_covers(_: &str, _: &Path) -> Option<PathBuf> {
        None
    }

    #[test]
    fn an_empty_card_gets_the_built_in_set_with_plain_names() {
        let s = pick(&[], &[], &[], no_covers);
        assert!(s.demo);
        assert!((2..=3).contains(&s.systems.len()), "{}", s.systems.len());
        for sys in &s.systems {
            assert!(crate::system_art::png(&sys.name).is_some(), "{} has a console picture", sys.name);
            assert!(sys.games.iter().all(|g| g.name.starts_with("Sample game")), "{:?}", sys.games);
        }
        assert_eq!(s.game.name, "Sample game");
        assert_eq!(s.game.system, s.systems[0].name);
        assert_eq!(s.cover, Cover::Drawn, "a drawn box, not somebody's box art");
    }

    #[test]
    fn a_card_whose_systems_are_all_hidden_also_gets_the_built_in_set() {
        let s = pick(&[system("gba", &["A"])], &["gba".into()], &[], no_covers);
        assert!(s.demo);
    }

    #[test]
    fn the_real_library_and_a_game_that_has_box_art() {
        let lib = [system("gb", &["Plain"]), system("gba", &["Boxed", "Other"]), system("snes", &["Third"])];
        let covers = |_: &str, rom: &Path| rom.ends_with("Boxed.rom").then(|| PathBuf::from("/covers/gba/Boxed.png"));
        let s = pick(&lib, &[], &[], covers);
        assert!(!s.demo);
        assert_eq!(s.systems.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["gb", "gba", "snes"]);
        assert_eq!((s.game.system.as_str(), s.game.name.as_str()), ("gba", "Boxed"));
        assert_eq!(s.cover, Cover::File("/covers/gba/Boxed.png".into()));
        assert!(!s.played, "shown as not played yet, no made-up play time");
    }

    #[test]
    fn a_recent_game_with_box_art_comes_first() {
        let lib = [system("gb", &["Boxed one"]), system("gba", &["Boxed two"])];
        let covers = |_: &str, rom: &Path| Some(rom.with_extension("png"));
        let s = pick(&lib, &[], &[recent("gba", "Boxed two")], covers);
        assert_eq!(s.game.name, "Boxed two");
        assert!(s.played);
    }

    #[test]
    fn without_any_box_art_the_last_played_game_gets_a_drawn_box() {
        let lib = [system("gb", &["One"]), system("gba", &["Two"])];
        let s = pick(&lib, &[], &[recent("gba", "Two")], no_covers);
        assert_eq!((s.game.name.as_str(), &s.cover), ("Two", &Cover::Drawn));
        let s = pick(&lib, &[], &[], no_covers);
        assert_eq!(s.game.name, "One", "nothing played: the first game");
    }

    #[test]
    fn hidden_systems_stay_out_of_the_preview() {
        let lib = [system("gb", &["One"]), system("ports", &["Boxed"])];
        let covers = |sys: &str, rom: &Path| (sys == "ports").then(|| rom.with_extension("png"));
        let s = pick(&lib, &["ports".into()], &[recent("ports", "Boxed")], covers);
        assert_eq!(s.systems.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["gb"]);
        assert_eq!(s.game.system, "gb", "nor its games");
    }
}
