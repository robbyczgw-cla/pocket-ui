//! The game card's descriptions, loaded ahead: when Wi-Fi is up and no game runs, the Wikipedia lead of
//! every game that matched a `gamedb` entry with an article, and has no text on the card yet, is fetched in
//! the background and cached (`wiki_text`), so cards open with their description even offline later.
//! When it may run is `auto_covers::BackgroundRun`'s, shared with the automatic covers: online, not in a
//! game, one run at a time, stopped by a game and resumed after it. Polite to Wikipedia: one request at a
//! time with a pause, User-Agent `pocket-ui-wiki/0.1`. The shelf shows "Descriptions 7 of 30".

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::auto_covers::BackgroundRun;
use crate::gamedb::{Choice, GameDb};

/// Pause between two requests to Wikipedia.
pub const DELAY: Duration = Duration::from_millis(1500);

/// News from the worker, for the app's event loop.
#[derive(Debug)]
pub enum Event {
    Progress {
        done: usize,
        total: usize,
    },
    /// `looked_up`: the games the run got to (or had nothing to fetch for); `stopped`: cut short.
    Done {
        looked_up: Vec<PathBuf>,
        fetched: usize,
        stopped: bool,
    },
}

pub struct AutoTexts {
    run: BackgroundRun,
}

impl Default for AutoTexts {
    fn default() -> Self {
        AutoTexts {
            run: BackgroundRun::new("Descriptions"),
        }
    }
}

impl std::ops::Deref for AutoTexts {
    type Target = BackgroundRun;
    fn deref(&self) -> &BackgroundRun {
        &self.run
    }
}

impl std::ops::DerefMut for AutoTexts {
    fn deref_mut(&mut self) -> &mut BackgroundRun {
        &mut self.run
    }
}

impl AutoTexts {
    /// The games (system, ROM path) a run should look at: those not looked at yet this session.
    pub fn candidates(&self, games: &[(String, PathBuf)]) -> Vec<(String, PathBuf)> {
        games.iter().filter(|(_, rom)| !self.run.tried(rom)).cloned().collect()
    }
}

/// What a run loads: (ROM, article) for games whose entry (the player's choice first) has an English
/// Wikipedia article without a cached text, each article once.
pub fn todo(data_dir: &Path, db: &mut GameDb, games: &[(String, PathBuf, Option<Choice>)]) -> Vec<(PathBuf, String)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    // The names the games' checksums gave (`rom_id`), matched before the file names.
    let names = crate::rom_id::known(data_dir, &games.iter().map(|(s, r, _)| (s.clone(), r.clone())).collect::<Vec<_>>());
    for (system, rom, choice) in games {
        let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let Some(title) = db.resolve_rom(system, stem, names.get(rom).map(String::as_str), choice.as_ref()).and_then(|g| g.enwiki) else {
            continue;
        };
        if seen.insert(title.clone()) && crate::wiki_text::cached(data_dir, &title).is_none() {
            out.push((rom.clone(), title));
        }
    }
    out
}

/// What a run did.
#[derive(Debug, Default, PartialEq)]
pub struct Run {
    /// Games whose article was asked for (found or not): not asked again this session.
    pub looked_up: Vec<PathBuf>,
    pub fetched: usize,
    /// It ended before the end of `todo`: a game started, or Wikipedia could not be reached.
    pub stopped: bool,
}

/// Loads the articles one after the other with `delay` between them, calling `progress(done, total)` after
/// each. Stops after the current one when `stop()` says so, and at once when Wikipedia can't be reached
/// ("offline"): those games are not counted as looked up, a later run tries them again.
pub fn fetch_until(
    todo: &[(PathBuf, String)],
    progress: impl Fn(usize, usize),
    stop: impl Fn() -> bool,
    fetch: impl FnMut(&str) -> Result<String, String>,
    delay: Duration,
) -> Run {
    let mut fetch = fetch;
    let mut run = Run::default();
    for (i, (rom, title)) in todo.iter().enumerate() {
        if stop() {
            run.stopped = true;
            break;
        }
        if i > 0 {
            std::thread::sleep(delay);
        }
        match fetch(title) {
            Ok(_) => run.fetched += 1,
            // No connection: stop asking; these games stay to do.
            Err(why) if why == "offline" => {
                run.stopped = true;
                break;
            }
            // No article, no text: asked, nothing to have.
            Err(_) => {}
        }
        run.looked_up.push(rom.clone());
        progress(i + 1, todo.len());
    }
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::borrow::Cow;
    use std::cell::RefCell;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("autotexts-{name}"))
    }

    const SNES: &str = "Q1\tSuper Metroid\t1994\t\t\t\t\t\t\t\t\tSuper Metroid\t50\t\n\
Q2\tEarthBound\t1994\t\t\t\t\t\t\t\t\tEarthBound\t50\t\n\
Q3\tObscure Game\t1994\t\t\t\t\t\t\t\t\t\t1\t\n\
Q4\tSecret of Mana\t1993\t\t\t\t\t\t\t\t\tSecret of Mana\t50\t\n";

    fn rom(stem: &str) -> PathBuf {
        PathBuf::from(format!("/storage/roms/snes/{stem}.sfc"))
    }

    #[test]
    fn only_matched_games_with_an_article_and_no_cached_text() {
        let d = dir("todo");
        let mut db = GameDb::parse(Cow::Owned(crate::gamedb::encode(&[("snes", SNES)]))).unwrap();
        std::fs::create_dir_all(d.join("wikipedia")).unwrap();
        std::fs::write(crate::wiki_text::cache_path(&d, "EarthBound"), "cached").unwrap();
        let g = |stem: &str, choice: Option<Choice>| ("snes".to_string(), rom(stem), choice);
        let games = vec![
            g("Super Metroid (Japan, USA) (En,Ja)", None),
            g("EarthBound (USA)", None),
            g("Obscure Game (Japan)", None),
            g("Homebrew Thing", None),
            g("Seiken Densetsu 2 (Japan)", Some(Choice::Entry("Q4".into()))),
            g("Super Metroid (Europe)", None),
            g("Secret of Mana (USA)", Some(Choice::NoEntry)),
        ];
        let t = todo(&d, &mut db, &games);
        assert_eq!(
            t,
            vec![
                (rom("Super Metroid (Japan, USA) (En,Ja)"), "Super Metroid".to_string()),
                (rom("Seiken Densetsu 2 (Japan)"), "Secret of Mana".to_string())
            ],
            "not cached EarthBound, not the game without an article or without an entry, the player's choice \
             first, one article once, nothing for None of these"
        );
    }

    #[test]
    fn one_at_a_time_with_progress_and_a_pause() {
        let todo = vec![(rom("A"), "A".to_string()), (rom("B"), "B".to_string())];
        let asked = RefCell::new(Vec::new());
        let seen = RefCell::new(Vec::new());
        let run = fetch_until(
            &todo,
            |done, total| seen.borrow_mut().push((done, total)),
            || false,
            |title| {
                asked.borrow_mut().push(title.to_string());
                if title == "B" {
                    Err("no article".into())
                } else {
                    Ok("text".into())
                }
            },
            Duration::from_millis(1),
        );
        assert_eq!(*asked.borrow(), ["A", "B"]);
        assert_eq!(*seen.borrow(), [(1, 2), (2, 2)]);
        assert_eq!(
            run,
            Run {
                looked_up: vec![rom("A"), rom("B")],
                fetched: 1,
                stopped: false
            },
            "a missing article is looked up too"
        );
    }

    #[test]
    fn a_game_or_a_lost_connection_stops_the_run() {
        let todo = vec![(rom("A"), "A".to_string()), (rom("B"), "B".to_string()), (rom("C"), "C".to_string())];
        // A game starts after the first one.
        let n = RefCell::new(0);
        let run = fetch_until(
            &todo,
            |_, _| {},
            || *n.borrow() >= 1,
            |_| {
                *n.borrow_mut() += 1;
                Ok("t".into())
            },
            Duration::ZERO,
        );
        assert_eq!(
            run,
            Run {
                looked_up: vec![rom("A")],
                fetched: 1,
                stopped: true
            }
        );
        // Wikipedia unreachable at B: B and C stay to do, nothing more is asked.
        let asked = RefCell::new(0);
        let run = fetch_until(
            &todo,
            |_, _| {},
            || false,
            |t| {
                *asked.borrow_mut() += 1;
                if t == "A" {
                    Ok("t".into())
                } else {
                    Err("offline".into())
                }
            },
            Duration::ZERO,
        );
        assert_eq!(*asked.borrow(), 2, "no more requests once offline");
        assert_eq!(
            run,
            Run {
                looked_up: vec![rom("A")],
                fetched: 1,
                stopped: true
            }
        );
    }

    #[test]
    fn games_looked_at_are_not_asked_again_and_the_note_counts() {
        let games = vec![("snes".to_string(), rom("A")), ("snes".to_string(), rom("B"))];
        let mut a = AutoTexts::default();
        assert_eq!(a.candidates(&games).len(), 2);
        a.begin(30);
        a.progress(7, 30);
        assert_eq!(a.note().as_deref(), Some("Descriptions 7 of 30"));
        a.finish(vec![rom("A")], false);
        assert_eq!(a.candidates(&games), vec![("snes".to_string(), rom("B"))]);
        assert!(a.note().is_none());
    }
}
