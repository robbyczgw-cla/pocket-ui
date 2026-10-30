//! Cover art without asking: when Wi-Fi is up and games have no box art yet, `covers::scrape` runs in the
//! background (after the first start's Wi-Fi step, at boot once online, and when the library finds new games).
//! It is quiet: a small note on the shelf at most, never while a game runs (it would take CPU and network from
//! the game), and a game that starts stops it after the cover it is on. Settings > Extras > Cover art stays the
//! way to ask for it by hand.
//!
//! With Settings > Games > Extras > Game pictures on (the default) the same run goes on to the games'
//! screenshots and title screens once the box art is done (`game_pictures`): the same rules, the same note on the
//! shelf ("Pictures 3 of 40" for that part).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::covers;

/// Whether an automatic run may start now, and if not, why.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    Start,
    InGame,
    /// One is running already (automatic, or the manual one in Extras).
    Running,
    Offline,
    NothingMissing,
}

/// A background job over the library that runs only online and outside a game, once at a time, stops when
/// a game starts and goes on after it: the automatic covers here, the game card's descriptions in
/// `auto_texts.rs`. What a job does is its own; this is when it may.
pub struct BackgroundRun {
    /// "Covers", "Descriptions": the start of the note on the shelf.
    label: &'static str,
    running: bool,
    /// Games a run has looked at this session; they are not asked for again until the next start.
    tried: HashSet<PathBuf>,
    cancel: Arc<AtomicBool>,
    /// A game stopped the last run before it was through.
    interrupted: bool,
    /// (done, total) of the running one, for the note on the shelf.
    progress: Option<(usize, usize)>,
}

impl BackgroundRun {
    pub fn new(label: &'static str) -> Self {
        BackgroundRun {
            label,
            running: false,
            tried: HashSet::new(),
            cancel: Arc::default(),
            interrupted: false,
            progress: None,
        }
    }

    /// `busy_elsewhere`: the same work runs outside this job (the manual cover scrape in Extras).
    pub fn decide(&self, online: bool, in_game: bool, missing: usize, busy_elsewhere: bool) -> Decision {
        if in_game {
            Decision::InGame
        } else if self.running || busy_elsewhere {
            Decision::Running
        } else if !online {
            Decision::Offline
        } else if missing == 0 {
            Decision::NothingMissing
        } else {
            Decision::Start
        }
    }

    /// Looked at already this session.
    pub fn tried(&self, rom: &Path) -> bool {
        self.tried.contains(rom)
    }

    /// A run of `total` games starts; the flag tells it to stop.
    pub fn begin(&mut self, total: usize) -> Arc<AtomicBool> {
        self.running = true;
        self.interrupted = false;
        self.progress = Some((0, total));
        self.cancel = Arc::new(AtomicBool::new(false));
        self.cancel.clone()
    }

    pub fn progress(&mut self, done: usize, total: usize) {
        if self.running {
            self.progress = Some((done, total));
        }
    }

    /// The run ended: `looked_up` are the games it got to; `stopped` when a game cut it short.
    pub fn finish(&mut self, looked_up: Vec<PathBuf>, stopped: bool) {
        self.running = false;
        self.progress = None;
        self.interrupted = stopped;
        self.tried.extend(looked_up);
    }

    /// A game starts: a run in progress stops after its current item.
    pub fn game_started(&mut self) {
        if self.running {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// After a game: the run it stopped should go on.
    pub fn resume_due(&self) -> bool {
        self.interrupted && !self.running
    }

    pub fn running(&self) -> bool {
        self.running
    }

    /// "Covers 3 of 12" while a run with something to do is under way.
    pub fn note(&self) -> Option<String> {
        self.progress
            .filter(|(_, total)| self.running && *total > 0)
            .map(|(done, total)| format!("{} {done} of {total}", self.label))
    }
}

/// The automatic cover download: a [`BackgroundRun`] and which games still lack box art or pictures.
pub struct AutoCovers {
    run: BackgroundRun,
    /// Games whose screenshots and title screens a run has looked at this session.
    tried_pictures: HashSet<PathBuf>,
    /// How many games of the running one are box art; the rest of its progress is pictures.
    covers_total: usize,
}

impl Default for AutoCovers {
    fn default() -> Self {
        AutoCovers {
            run: BackgroundRun::new("Covers"),
            tried_pictures: HashSet::new(),
            covers_total: 0,
        }
    }
}

impl std::ops::Deref for AutoCovers {
    type Target = BackgroundRun;
    fn deref(&self) -> &BackgroundRun {
        &self.run
    }
}

impl std::ops::DerefMut for AutoCovers {
    fn deref_mut(&mut self) -> &mut BackgroundRun {
        &mut self.run
    }
}

impl AutoCovers {
    /// A run starts with `covers` games of box art to look up and `pictures` games of screenshots and title
    /// screens after them; the flag tells it to stop.
    pub fn begin_run(&mut self, covers: usize, pictures: usize) -> Arc<AtomicBool> {
        self.covers_total = covers;
        self.run.begin(covers + pictures)
    }

    /// The run ended: the games whose box art and whose pictures it got to (see [`BackgroundRun::finish`]).
    pub fn finish_run(&mut self, covers: Vec<PathBuf>, pictures: Vec<PathBuf>, stopped: bool) {
        self.tried_pictures.extend(pictures);
        self.run.finish(covers, stopped);
    }

    /// "Covers 3 of 12", then "Pictures 3 of 40" for the games' screenshots and title screens.
    pub fn note(&self) -> Option<String> {
        let (done, total) = self.run.progress.filter(|(_, total)| self.run.running && *total > 0)?;
        Some(if done < self.covers_total || self.covers_total == total {
            format!("{} {done} of {}", self.run.label, self.covers_total.min(total))
        } else {
            format!("Pictures {} of {}", done - self.covers_total, total - self.covers_total)
        })
    }

    /// The games (system id, ROM path) still without screenshots or title screens that the collection may have
    /// (`game_pictures::missing`), not looked up yet this session. Nothing when the player chose box art only.
    pub fn missing_pictures(&self, data_dir: &Path, games: &[(String, PathBuf)], wanted: bool) -> Vec<(String, PathBuf)> {
        if !wanted {
            return Vec::new();
        }
        crate::game_pictures::missing(data_dir, games, &self.tried_pictures)
    }

    /// The games (system id, ROM path) still without box art that the catalog may have: a known system, no
    /// cover file, not in that system's `.missing` list, and not looked up yet this session.
    pub fn missing(&self, data_dir: &Path, games: &[(String, PathBuf)]) -> Vec<(String, PathBuf)> {
        let mut listed: std::collections::HashMap<&str, HashSet<String>> = Default::default();
        games
            .iter()
            .filter(|(sys, rom)| {
                if covers::playlist(sys).is_none() || self.run.tried(rom) || covers::cover_path(data_dir, sys, rom).exists() || crate::library_cache::is_empty_file(rom) {
                    return false;
                }
                let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
                !listed
                    .entry(sys.as_str())
                    .or_insert_with(|| covers::listed_missing(data_dir, sys))
                    .contains(stem)
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("autocovers-{name}"))
    }

    fn game(sys: &str, stem: &str) -> (String, PathBuf) {
        (sys.to_string(), PathBuf::from(format!("/storage/roms/{sys}/{stem}.rom")))
    }

    #[test]
    fn it_runs_only_online_outside_a_game_with_covers_missing() {
        let a = AutoCovers::default();
        assert_eq!(a.decide(true, false, 3, false), Decision::Start);
        assert_eq!(a.decide(false, false, 3, false), Decision::Offline);
        assert_eq!(a.decide(true, true, 3, false), Decision::InGame, "never while a game runs");
        assert_eq!(a.decide(true, false, 0, false), Decision::NothingMissing);
    }

    #[test]
    fn not_twice_at_the_same_time() {
        let mut a = AutoCovers::default();
        a.begin(3);
        assert!(a.running());
        assert_eq!(a.decide(true, false, 3, false), Decision::Running);
        a.finish(Vec::new(), false);
        assert!(!a.running());
        assert_eq!(a.decide(true, false, 3, false), Decision::Start);
    }

    #[test]
    fn not_while_the_manual_one_runs() {
        let a = AutoCovers::default();
        let claim = covers::claim().expect("nothing else is scraping");
        assert_eq!(a.decide(true, false, 3, covers::busy()), Decision::Running, "Extras > Cover art is on it");
        assert!(covers::claim().is_none(), "one scrape at a time");
        drop(claim);
        assert_eq!(a.decide(true, false, 3, covers::busy()), Decision::Start);
    }

    #[test]
    fn missing_skips_covers_on_disk_the_missing_lists_and_other_systems() {
        let d = dir("missing");
        let games = vec![
            game("gba", "Has Cover"),
            game("gba", "Not In Catalog"),
            game("gba", "New Game"),
            game("tools", "Not A Console"),
        ];
        let cover = covers::cover_path(&d, "gba", &games[0].1);
        std::fs::create_dir_all(cover.parent().unwrap()).unwrap();
        std::fs::write(&cover, b"png").unwrap();
        std::fs::write(d.join("covers/gba/.missing"), "Not In Catalog\n").unwrap();
        let a = AutoCovers::default();
        assert_eq!(a.missing(&d, &games), vec![game("gba", "New Game")]);
    }

    #[test]
    fn a_finished_run_does_not_ask_for_the_same_games_again() {
        let d = dir("tried");
        let games = vec![game("snes", "A"), game("snes", "B")];
        let mut a = AutoCovers::default();
        assert_eq!(a.missing(&d, &games).len(), 2);
        a.begin(2);
        // The catalog didn't load: no cover, no .missing entry; still not again this session.
        a.finish(vec![games[0].1.clone(), games[1].1.clone()], false);
        assert!(a.missing(&d, &games).is_empty());
        assert_eq!(a.decide(true, false, a.missing(&d, &games).len(), false), Decision::NothingMissing);
        // A game copied later is new.
        let more = vec![game("snes", "A"), game("snes", "C")];
        assert_eq!(a.missing(&d, &more), vec![game("snes", "C")]);
    }

    #[test]
    fn a_game_stops_the_run_and_it_goes_on_afterwards() {
        let d = dir("stopped");
        let games = vec![game("nes", "A"), game("nes", "B"), game("nes", "C")];
        let mut a = AutoCovers::default();
        let stop = a.begin_run(3, 0);
        a.progress(1, 3);
        assert_eq!(a.note().as_deref(), Some("Covers 1 of 3"));
        a.game_started();
        assert!(stop.load(Ordering::Relaxed), "the running scrape is told to stop");
        a.finish(vec![games[0].1.clone()], true);
        assert!(a.note().is_none());
        assert!(a.resume_due(), "after the game");
        assert_eq!(a.missing(&d, &games), vec![game("nes", "B"), game("nes", "C")], "the rest is still to do");
        a.begin(2);
        assert!(!a.resume_due());
    }

    #[test]
    fn the_note_goes_on_from_covers_to_pictures() {
        let mut a = AutoCovers::default();
        a.begin_run(2, 5);
        a.progress(1, 7);
        assert_eq!(a.note().as_deref(), Some("Covers 1 of 2"));
        a.progress(2, 7);
        a.progress(4, 7);
        assert_eq!(a.note().as_deref(), Some("Pictures 2 of 5"));
        a.finish_run(Vec::new(), Vec::new(), false);
        assert!(a.note().is_none());
        // Only pictures to get: no covers line at all.
        a.begin_run(0, 3);
        a.progress(1, 3);
        assert_eq!(a.note().as_deref(), Some("Pictures 1 of 3"));
    }

    #[test]
    fn pictures_are_part_of_the_job_only_when_wanted_and_are_remembered_per_session() {
        let d = dir("pictures");
        let games = vec![game("gba", "A"), game("gba", "B")];
        let mut a = AutoCovers::default();
        assert_eq!(a.missing_pictures(&d, &games, true).len(), 2);
        assert!(a.missing_pictures(&d, &games, false).is_empty(), "Box art only: no pictures to look for");
        assert_eq!(a.decide(true, false, a.missing_pictures(&d, &games, true).len(), false), Decision::Start, "pictures alone make a run");
        a.begin_run(0, 2);
        a.finish_run(Vec::new(), vec![games[0].1.clone()], false);
        assert_eq!(a.missing_pictures(&d, &games, true), vec![game("gba", "B")], "A was looked at");
    }

    #[test]
    fn a_stopped_scrape_touches_nothing() {
        let d = dir("scrape-stop");
        let games = vec![game("gba", "A")];
        let done = covers::scrape_until(&d, &games, |_, _, _| {}, || true);
        assert_eq!(done.found, 0);
        assert!(done.looked_up.is_empty());
        assert!(!d.join("covers").exists(), "no network, no .missing");
    }
}
