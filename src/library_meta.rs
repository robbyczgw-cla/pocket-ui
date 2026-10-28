//! What the player says about their games: a play status (Not started, Playing, Paused, Completed,
//! Dropped) and one short note per game, plus the number of sessions, which lets the game card suggest
//! "Playing" after the second one. One JSON file in the data directory, `library.json`, written with
//! `fsutil::write_json` (atomic, with a `.bak` of the last good copy). Games are keyed by their path,
//! like favourites.
//!
//! Curator (docs/CURATOR.md) keeps its per-game data here too: the player's stickers (a list with names and order,
//! and the sticker ids of each game) and the preferred version of a game owned on several systems. Version 2 of the
//! file adds them; a version 1 file reads as it is, and an older pocket-ui reading a version 2 file skips them.
//!
//! The player's own rating of a game (1 to 5, none until they give one) sits next to the status and the note; it is
//! written only when there is one, so files without ratings look as before and older readers skip it.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A game played this long or longer counts as a session (a start that fails at once does not).
pub const MIN_SESSION_SECS: u64 = 60;
/// Sessions after which an unplayed game is suggested as Playing.
pub const SUGGEST_AFTER: u32 = 2;
/// Characters a note may have: one line on the card.
pub const NOTE_MAX: usize = 80;
/// The highest rating; 0 is "not rated".
pub const RATING_MAX: u8 = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    NotStarted,
    Playing,
    Paused,
    Completed,
    Dropped,
}

impl Status {
    pub const ALL: [Status; 5] = [Status::NotStarted, Status::Playing, Status::Paused, Status::Completed, Status::Dropped];

    pub fn label(self) -> &'static str {
        match self {
            Status::NotStarted => "Not started",
            Status::Playing => "Playing",
            Status::Paused => "Paused",
            Status::Completed => "Completed",
            Status::Dropped => "Dropped",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameMeta {
    #[serde(default)]
    pub status: Status,
    /// The player chose the status themselves (even "Not started"): nothing is suggested any more.
    #[serde(default)]
    pub status_set: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// Sessions of at least [`MIN_SESSION_SECS`].
    #[serde(default)]
    pub sessions: u32,
    /// Seconds played since sessions are counted (short starts included). The recent games list may have
    /// more: time from before, which counts for the suggestion as well.
    #[serde(default)]
    pub secs: u64,
    /// "Wrong game? Choose another": the entry the player picked (or none), before the automatic match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<crate::gamedb::Choice>,
    /// Curator: the ids of the player's stickers on this game ([`Sticker::id`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stickers: Vec<u32>,
    /// The player's rating, 1 to [`RATING_MAX`]; 0 while they have given none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rating: u8,
    /// "Hide from the shelf" on the card: the game stays on the card but leaves every list (`library::without_hidden`);
    /// Settings → System → Storage → Hidden games brings it back. An older pocket-ui skips the field.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

/// A label the player made ("Couch co-op", "Before bed"). The list's order is the order they are shown in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sticker {
    pub id: u32,
    pub name: String,
    /// Shown as a row on Home.
    #[serde(default)]
    pub pinned: bool,
}

/// Characters a sticker's name may have.
pub const STICKER_MAX: usize = 32;

/// The version written now; 1 had no stickers and preferred versions.
const VERSION: u32 = 2;

#[derive(Default, Serialize, Deserialize)]
struct File {
    version: u32,
    games: BTreeMap<String, GameMeta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    stickers: Vec<Sticker>,
    /// Curator's Versions: work key (gamedb QID) -> the path of the version the player prefers.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    preferred: BTreeMap<String, String>,
}

pub struct LibraryMeta {
    file: PathBuf,
    games: BTreeMap<String, GameMeta>,
    stickers: Vec<Sticker>,
    preferred: BTreeMap<String, String>,
}

fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl LibraryMeta {
    pub fn load(data_dir: &Path) -> Self {
        let file = data_dir.join("library.json");
        let f = crate::fsutil::read_json_or_backup::<File>(&file).unwrap_or_default();
        Self { file, games: f.games, stickers: f.stickers, preferred: f.preferred }
    }

    /// What is known about the game (all defaults for a game never touched).
    pub fn get(&self, path: &Path) -> GameMeta {
        self.games.get(&key(path)).cloned().unwrap_or_default()
    }

    pub fn status(&self, path: &Path) -> Status {
        self.get(path).status
    }

    /// Sets the status the player chose and saves.
    pub fn set_status(&mut self, path: &Path, status: Status) -> io::Result<()> {
        let g = self.games.entry(key(path)).or_default();
        g.status = status;
        g.status_set = true;
        self.save()
    }

    /// Sets the rating (0 takes it away; more than [`RATING_MAX`] counts as the most) and saves.
    pub fn set_rating(&mut self, path: &Path, rating: u8) -> io::Result<()> {
        self.games.entry(key(path)).or_default().rating = rating.min(RATING_MAX);
        self.save()
    }

    /// Gives the games in `ratings` (path, 1 to [`RATING_MAX`]) that have no rating of the player's own that rating, with
    /// one save: the import of EmulationStation's stars (`gamelist_page`). Returns how many were given one; a game the
    /// player rated keeps their rating, and nothing is written when there is nothing to give.
    pub fn set_ratings_if_none(&mut self, ratings: &[(PathBuf, u8)]) -> io::Result<usize> {
        let mut given = 0;
        for (path, stars) in ratings.iter().filter(|(_, n)| *n > 0) {
            let g = self.games.entry(key(path)).or_default();
            if g.rating == 0 {
                g.rating = (*stars).min(RATING_MAX);
                given += 1;
            }
        }
        if given > 0 {
            self.save()?;
        }
        Ok(given)
    }

    /// Sets the note (trimmed, at most [`NOTE_MAX`] characters; empty removes it) and saves.
    pub fn set_note(&mut self, path: &Path, note: &str) -> io::Result<()> {
        let note: String = note.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(NOTE_MAX).collect();
        self.games.entry(key(path)).or_default().note = note.trim_end().to_string();
        self.save()
    }

    /// A game ended after `secs`: adds the time, counts a session when it was long enough, and saves.
    pub fn record_session(&mut self, path: &Path, secs: u64) {
        if self.note_session(path, secs) {
            self.save_sessions();
        }
    }

    /// `record_session` without the save, which syncs to the card (`save_sessions` follows); false when there was
    /// nothing to count.
    pub fn note_session(&mut self, path: &Path, secs: u64) -> bool {
        if secs == 0 {
            return false;
        }
        let g = self.games.entry(key(path)).or_default();
        g.secs = g.secs.saturating_add(secs);
        if secs >= MIN_SESSION_SECS {
            g.sessions = g.sessions.saturating_add(1);
        }
        true
    }

    /// Writes what `note_session` counted.
    pub fn save_sessions(&self) {
        if let Err(e) = self.save() {
            eprintln!("library: sessions not saved: {e}");
        }
    }

    /// "Playing", when the game was played twice without the player saying anything about it, or was
    /// played before sessions were counted: `recorded_secs` is its time in the recent games list, and what
    /// is more than this file counted is time from before (a game played with an older pocket-ui).
    /// Only a suggestion: the status stays what it was until the player picks one.
    pub fn suggestion(&self, path: &Path, recorded_secs: u64) -> Option<Status> {
        let g = self.get(path);
        let earlier = recorded_secs.saturating_sub(g.secs);
        let played = g.sessions >= SUGGEST_AFTER || earlier >= MIN_SESSION_SECS;
        (!g.status_set && g.status == Status::NotStarted && played).then_some(Status::Playing)
    }

    /// Keeps the player's pick for the game (`None`: back to the automatic match) and saves.
    pub fn set_choice(&mut self, path: &Path, choice: Option<crate::gamedb::Choice>) -> io::Result<()> {
        self.games.entry(key(path)).or_default().choice = choice;
        self.save()
    }

    /// Whether the player hid the game from the shelf (a look-up, nothing cloned: asked for every recent game).
    pub fn is_hidden(&self, path: &Path) -> bool {
        self.games.get(&key(path)).is_some_and(|g| g.hidden)
    }

    /// Hides the game from every list, or shows it again, and saves.
    pub fn set_hidden(&mut self, path: &Path, hidden: bool) -> io::Result<()> {
        self.games.entry(key(path)).or_default().hidden = hidden;
        self.save()
    }

    /// The hidden games' paths, sorted.
    pub fn hidden(&self) -> Vec<PathBuf> {
        self.games.iter().filter(|(_, g)| g.hidden).map(|(k, _)| PathBuf::from(k)).collect()
    }

    /// Every game the file knows something about, for Curator's index (built on its own thread).
    pub fn games(&self) -> BTreeMap<String, GameMeta> {
        self.games.clone()
    }

    /// The games with time or sessions counted: (path, seconds, sessions). Only those are copied (Play history's
    /// one migration, `play_log::Seed`).
    pub fn played(&self) -> Vec<(String, u64, u32)> {
        self.games.iter().filter(|(_, g)| g.secs > 0 || g.sessions > 0).map(|(k, g)| (k.clone(), g.secs, g.sessions)).collect()
    }

    /// The player's stickers, in their order.
    pub fn stickers(&self) -> &[Sticker] {
        &self.stickers
    }

    /// Makes a sticker (name trimmed to [`STICKER_MAX`]; an existing name is that sticker) and saves; returns its id.
    pub fn add_sticker(&mut self, name: &str) -> io::Result<u32> {
        let name = clean_name(name);
        if let Some(s) = self.stickers.iter().find(|s| s.name.eq_ignore_ascii_case(&name)) {
            return Ok(s.id);
        }
        let id = self.stickers.iter().map(|s| s.id + 1).max().unwrap_or(1);
        self.stickers.push(Sticker { id, name, pinned: false });
        self.save().map(|()| id)
    }

    pub fn rename_sticker(&mut self, id: u32, name: &str) -> io::Result<()> {
        let name = clean_name(name);
        if let Some(s) = self.stickers.iter_mut().find(|s| s.id == id) {
            s.name = name;
        }
        self.save()
    }

    pub fn set_sticker_pinned(&mut self, id: u32, pinned: bool) -> io::Result<()> {
        if let Some(s) = self.stickers.iter_mut().find(|s| s.id == id) {
            s.pinned = pinned;
        }
        self.save()
    }

    /// Deletes a sticker and takes it off every game.
    pub fn delete_sticker(&mut self, id: u32) -> io::Result<()> {
        self.stickers.retain(|s| s.id != id);
        for g in self.games.values_mut() {
            g.stickers.retain(|s| *s != id);
        }
        self.save()
    }

    /// Puts the sticker on the game, or takes it off; saves and returns whether it is on now.
    pub fn toggle_sticker(&mut self, path: &Path, id: u32) -> io::Result<bool> {
        let g = self.games.entry(key(path)).or_default();
        let on = match g.stickers.iter().position(|s| *s == id) {
            Some(i) => {
                g.stickers.remove(i);
                false
            }
            None => {
                g.stickers.push(id);
                true
            }
        };
        self.save().map(|()| on)
    }

    /// The preferred version of a work, as the path the player chose.
    pub fn preferred(&self) -> &BTreeMap<String, String> {
        &self.preferred
    }

    /// Y on the Versions page: this file is the version lists show and start.
    pub fn set_preferred(&mut self, work: &str, path: &Path) -> io::Result<()> {
        self.preferred.insert(work.to_string(), key(path));
        self.save()
    }

    fn save(&self) -> io::Result<()> {
        crate::fsutil::write_json(&self.file, &self.contents())
    }

    /// A game's entry to change in place, made when there is none: for `library_sync`, which applies what came from
    /// the player's other devices in one batch and then saves once ([`save_now`](Self::save_now)).
    pub fn game_mut(&mut self, path: &Path) -> &mut GameMeta {
        self.games.entry(key(path)).or_default()
    }

    /// Every game's entry, for `library_sync`'s batch (taking a sticker off every game).
    pub fn games_mut(&mut self) -> impl Iterator<Item = &mut GameMeta> {
        self.games.values_mut()
    }

    /// The stickers to change in place, for `library_sync`'s batch.
    pub fn stickers_mut(&mut self) -> &mut Vec<Sticker> {
        &mut self.stickers
    }

    /// Writes the file now (after a batch of `game_mut` / `stickers_mut`).
    pub fn save_now(&self) -> io::Result<()> {
        self.save()
    }

    /// `save_sessions` on the card writer (`fsutil::write_json_soon`): a game's end doesn't wait for the card.
    pub fn save_sessions_soon(&self) {
        crate::fsutil::write_json_soon(&self.file, &self.contents());
    }

    fn contents(&self) -> File {
        // Games with nothing left to remember are not written.
        let games: BTreeMap<String, GameMeta> = self
            .games
            .iter()
            .filter(|(_, g)| **g != GameMeta::default())
            .map(|(k, g)| (k.clone(), g.clone()))
            .collect();
        File { version: VERSION, games, stickers: self.stickers.clone(), preferred: self.preferred.clone() }
    }
}

/// The hidden games of `data_dir`'s library.json, for a thread that reads the game list (`library_cache`, the web
/// companion) and has no `LibraryMeta` of its own. Empty when the file is missing or damaged beyond its backup.
pub fn hidden_paths(data_dir: &Path) -> std::collections::HashSet<PathBuf> {
    #[derive(Deserialize)]
    struct Hidden {
        #[serde(default)]
        hidden: bool,
    }
    #[derive(Deserialize)]
    struct Only {
        #[serde(default)]
        games: BTreeMap<String, Hidden>,
    }
    crate::fsutil::read_json_or_backup::<Only>(&data_dir.join("library.json"))
        .map(|f| f.games.into_iter().filter(|(_, g)| g.hidden).map(|(k, _)| PathBuf::from(k)).collect())
        .unwrap_or_default()
}

/// A sticker's name: one line, trimmed, at most [`STICKER_MAX`] characters; "Sticker" when nothing is left.
fn clean_name(name: &str) -> String {
    let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(STICKER_MAX).collect();
    let name = name.trim().to_string();
    if name.is_empty() {
        "Sticker".to_string()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::fs;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("libmeta-{name}"))
    }

    const ZELDA: &str = "/storage/roms/gb/Legend of Zelda, The - Link's Awakening (USA, Europe) (Rev 2).gb";
    const TETRIS: &str = "/storage/roms/gb/Tetris (World) (Rev 1).gb";

    #[test]
    fn status_and_note_survive_a_restart() {
        let d = dir("restart");
        let mut m = LibraryMeta::load(&d);
        assert_eq!(m.get(Path::new(ZELDA)), GameMeta::default(), "a new library knows nothing");
        m.set_status(Path::new(ZELDA), Status::Paused).unwrap();
        m.set_note(Path::new(ZELDA), "  after the first boss  ").unwrap();
        m.set_status(Path::new(TETRIS), Status::Completed).unwrap();
        let again = LibraryMeta::load(&d);
        let z = again.get(Path::new(ZELDA));
        assert_eq!((z.status, z.note.as_str(), z.status_set), (Status::Paused, "after the first boss", true));
        assert_eq!(again.status(Path::new(TETRIS)), Status::Completed);
        // An empty note removes it; a long one is cut to one line.
        let mut m = again;
        m.set_note(Path::new(ZELDA), "").unwrap();
        m.set_note(Path::new(TETRIS), &"x".repeat(200)).unwrap();
        let again = LibraryMeta::load(&d);
        assert_eq!(again.get(Path::new(ZELDA)).note, "");
        assert_eq!(again.get(Path::new(TETRIS)).note.chars().count(), NOTE_MAX);
        // One file holds it all, with its backup.
        let names: BTreeMap<_, _> = fs::read_dir(&d).unwrap().map(|e| (e.unwrap().file_name().into_string().unwrap(), ())).collect();
        assert_eq!(names.keys().collect::<Vec<_>>(), ["library.json", "library.json.bak"]);
    }

    #[test]
    fn a_hidden_game_stays_hidden_after_a_restart_and_comes_back_when_shown() {
        let d = dir("hidden");
        let mut m = LibraryMeta::load(&d);
        assert!(hidden_paths(&d).is_empty(), "no file: nothing hidden");
        m.set_note(Path::new(TETRIS), "keep me").unwrap();
        m.set_hidden(Path::new(TETRIS), true).unwrap();
        m.set_hidden(Path::new(ZELDA), true).unwrap();
        let again = LibraryMeta::load(&d);
        assert!(again.is_hidden(Path::new(TETRIS)) && again.is_hidden(Path::new(ZELDA)));
        assert_eq!(again.hidden(), vec![PathBuf::from(ZELDA), PathBuf::from(TETRIS)], "sorted by path");
        assert_eq!(hidden_paths(&d).len(), 2, "a thread reads the same without a LibraryMeta");
        let mut m = again;
        m.set_hidden(Path::new(ZELDA), false).unwrap();
        m.set_hidden(Path::new(TETRIS), false).unwrap();
        let again = LibraryMeta::load(&d);
        assert!(again.hidden().is_empty() && hidden_paths(&d).is_empty());
        assert_eq!(again.get(Path::new(TETRIS)).note, "keep me", "showing it again keeps the rest");
        let text = fs::read_to_string(d.join("library.json")).unwrap();
        assert!(!text.contains("hidden"), "a shown game writes no flag: {text}");
        assert!(!text.contains("Link's Awakening"), "a game with nothing left is not written");
    }

    #[test]
    fn saving_is_atomic_and_a_damaged_file_comes_back_from_the_backup() {
        let d = dir("atomic");
        let mut m = LibraryMeta::load(&d);
        m.set_status(Path::new(ZELDA), Status::Playing).unwrap();
        m.set_note(Path::new(ZELDA), "water temple").unwrap();
        // A power cut that left 0 bytes (or half a file): the backup has the last good state.
        for bad in [&b""[..], b"{\"version\":1,\"games\":{"] {
            fs::write(d.join("library.json"), bad).unwrap();
            let back = LibraryMeta::load(&d);
            assert_eq!(back.get(Path::new(ZELDA)).note, "water temple", "{bad:?}");
            assert_eq!(back.status(Path::new(ZELDA)), Status::Playing);
        }
        // A save that cannot be written leaves the file as it was and says so.
        let mut m = LibraryMeta::load(&d);
        crate::fsutil::fail_next_write();
        assert!(m.set_status(Path::new(ZELDA), Status::Dropped).is_err(), "the failed write is reported");
        assert_eq!(LibraryMeta::load(&d).status(Path::new(ZELDA)), Status::Playing, "the old file is untouched");
    }

    #[test]
    fn playing_is_suggested_after_the_second_session_never_set() {
        let d = dir("suggest");
        let mut m = LibraryMeta::load(&d);
        let z = Path::new(ZELDA);
        // The second argument is the play time the recent games list has for the game, all sessions included.
        m.record_session(z, 5);
        m.record_session(z, 30);
        assert_eq!(m.suggestion(z, 35), None, "starts that ended at once are not sessions");
        m.record_session(z, 600);
        assert_eq!(m.suggestion(z, 635), None, "one session is not enough");
        m.record_session(z, 900);
        assert_eq!(m.suggestion(z, 1535), Some(Status::Playing), "after the second session");
        assert_eq!(m.status(z), Status::NotStarted, "only suggested, not set");
        assert_eq!(LibraryMeta::load(&d).suggestion(z, 1535), Some(Status::Playing), "sessions are saved");
        // Taking the suggestion, or choosing anything else (even Not started), ends it.
        m.set_status(z, Status::NotStarted).unwrap();
        assert_eq!(m.suggestion(z, 1535), None);
        m.record_session(z, 900);
        assert_eq!(m.suggestion(z, 2435), None, "the player's choice stands");
        // A game the player already marked is never suggested.
        let t = Path::new(TETRIS);
        m.set_status(t, Status::Completed).unwrap();
        m.record_session(t, 900);
        m.record_session(t, 900);
        assert_eq!(m.suggestion(t, 1800), None);
    }

    #[test]
    fn play_time_from_before_the_session_count_is_enough_for_the_suggestion() {
        let d = dir("earlier");
        let mut m = LibraryMeta::load(&d);
        let b = Path::new("/storage/roms/ps2/Burnout 3 - Takedown (USA).chd");
        // 10 minutes in the recent games, played before sessions were counted: Playing at once.
        assert_eq!(m.suggestion(b, 600), Some(Status::Playing));
        assert_eq!(m.suggestion(b, 30), None, "half a minute is no play time");
        assert_eq!(m.suggestion(b, 0), None);
        // A game played once since: that session is not the earlier time, one session alone is not enough.
        let z = Path::new(ZELDA);
        m.record_session(z, 600);
        assert_eq!(m.suggestion(z, 600), None, "the one counted session, not time from before");
        // Short starts count as time, so they don't turn into time from before either.
        let t = Path::new(TETRIS);
        m.record_session(t, 40);
        m.record_session(t, 40);
        assert_eq!(m.suggestion(t, 80), None);
        // A status the player chose stands, whatever the time.
        m.set_status(b, Status::Dropped).unwrap();
        assert_eq!(m.suggestion(b, 6000), None);
    }

    #[test]
    fn the_chosen_entry_is_kept_per_rom_and_can_go_back_to_automatic() {
        use crate::gamedb::Choice;
        let d = dir("choice");
        let mut m = LibraryMeta::load(&d);
        let z = Path::new(ZELDA);
        assert_eq!(m.get(z).choice, None, "automatic until the player picks");
        m.set_choice(z, Some(Choice::Entry("Q1060323".into()))).unwrap();
        m.set_choice(Path::new(TETRIS), Some(Choice::NoEntry)).unwrap();
        let again = LibraryMeta::load(&d);
        assert_eq!(again.get(z).choice, Some(Choice::Entry("Q1060323".into())));
        assert_eq!(again.get(Path::new(TETRIS)).choice, Some(Choice::NoEntry));
        // Written like the rest: atomically, with the backup.
        let text = fs::read_to_string(d.join("library.json")).unwrap();
        assert!(text.contains("\"entry\": \"Q1060323\"") && text.contains("\"no_entry\""), "{text}");
        assert_eq!(fs::read_to_string(d.join("library.json.bak")).unwrap(), text);
        let mut m = again;
        m.set_choice(z, None).unwrap();
        assert_eq!(LibraryMeta::load(&d).get(z), GameMeta::default(), "back to automatic leaves nothing behind");
    }

    #[test]
    fn stickers_are_kept_per_game_and_deleting_one_takes_it_off_every_game() {
        let d = dir("stickers");
        let mut m = LibraryMeta::load(&d);
        let couch = m.add_sticker("  Couch   co-op ").unwrap();
        let bed = m.add_sticker("Before bed").unwrap();
        assert_eq!(m.add_sticker("couch co-op").unwrap(), couch, "the same name is the same sticker");
        assert_eq!(m.stickers().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Couch co-op", "Before bed"]);
        assert!(m.toggle_sticker(Path::new(ZELDA), couch).unwrap());
        assert!(m.toggle_sticker(Path::new(ZELDA), bed).unwrap());
        assert!(m.toggle_sticker(Path::new(TETRIS), bed).unwrap());
        assert!(!m.toggle_sticker(Path::new(TETRIS), bed).unwrap(), "a second A takes it off");
        m.set_sticker_pinned(bed, true).unwrap();
        m.rename_sticker(couch, "Two players").unwrap();
        m.set_preferred("Q761815", Path::new("/storage/roms/snes/Chrono Trigger (USA).sfc")).unwrap();
        let again = LibraryMeta::load(&d);
        assert_eq!(again.get(Path::new(ZELDA)).stickers, [couch, bed]);
        assert_eq!(again.get(Path::new(TETRIS)), GameMeta::default(), "nothing left on Tetris, so it is not written");
        assert_eq!(again.stickers()[0].name, "Two players");
        assert!(again.stickers()[1].pinned);
        assert_eq!(again.preferred().get("Q761815").map(String::as_str), Some("/storage/roms/snes/Chrono Trigger (USA).sfc"));
        let mut m = again;
        m.delete_sticker(couch).unwrap();
        let again = LibraryMeta::load(&d);
        assert_eq!(again.get(Path::new(ZELDA)).stickers, [bed]);
        assert_eq!(again.stickers().len(), 1);
        let mut m = again;
        assert_eq!(m.add_sticker("   ").unwrap(), bed + 1, "ids are never reused while a higher one exists");
        assert_eq!(m.stickers().last().unwrap().name, "Sticker");
    }

    #[test]
    fn a_version_1_file_reads_and_a_version_2_file_reads_in_the_old_shape() {
        let d = dir("migrate");
        fs::create_dir_all(&d).unwrap();
        // Written by the pocket-ui before Curator.
        fs::write(
            d.join("library.json"),
            format!(r#"{{"version":1,"games":{{"{ZELDA}":{{"status":"paused","note":"after the first boss","sessions":3,"secs":5400}}}}}}"#),
        )
        .unwrap();
        let mut m = LibraryMeta::load(&d);
        let z = m.get(Path::new(ZELDA));
        assert_eq!((z.status, z.note.as_str(), z.sessions, z.secs, z.stickers.len()), (Status::Paused, "after the first boss", 3, 5400, 0));
        assert!(m.stickers().is_empty() && m.preferred().is_empty());
        let id = m.add_sticker("Before bed").unwrap();
        m.toggle_sticker(Path::new(ZELDA), id).unwrap();
        let text = fs::read_to_string(d.join("library.json")).unwrap();
        assert!(text.contains("\"version\": 2"), "{text}");
        // What the older pocket-ui does with it: its own types, which know nothing of stickers.
        #[derive(Deserialize)]
        struct OldGame {
            status: Status,
            note: String,
            secs: u64,
        }
        #[derive(Deserialize)]
        struct OldFile {
            games: BTreeMap<String, OldGame>,
        }
        let old: OldFile = serde_json::from_str(&text).expect("the old reader still reads it");
        let z = &old.games[ZELDA];
        assert_eq!((z.status, z.note.as_str(), z.secs), (Status::Paused, "after the first boss", 5400));
        // And a file without the new keys at all still writes none of them.
        let d2 = dir("migrate-plain");
        let mut m = LibraryMeta::load(&d2);
        m.set_status(Path::new(TETRIS), Status::Completed).unwrap();
        let text = fs::read_to_string(d2.join("library.json")).unwrap();
        assert!(!text.contains("stickers") && !text.contains("preferred"), "{text}");
    }

    /// EmulationStation's stars (`gamelist_page`): given only to games without a rating of the player's own, saved once.
    #[test]
    fn imported_ratings_never_replace_the_players_own() {
        let dir = TestDir::new("meta-import");
        let mut m = LibraryMeta::load(&dir);
        m.set_rating(Path::new("/r/a.gba"), 2).unwrap();
        let list = vec![(PathBuf::from("/r/a.gba"), 5), (PathBuf::from("/r/b.gba"), 4), (PathBuf::from("/r/c.gba"), 0), (PathBuf::from("/r/d.gba"), 9)];
        assert_eq!(m.set_ratings_if_none(&list).unwrap(), 2, "b and d; a is the player's, c has no stars");
        assert_eq!((m.get(Path::new("/r/a.gba")).rating, m.get(Path::new("/r/b.gba")).rating, m.get(Path::new("/r/c.gba")).rating), (2, 4, 0));
        assert_eq!(m.get(Path::new("/r/d.gba")).rating, RATING_MAX, "more than five counts as five");
        assert_eq!(LibraryMeta::load(&dir).get(Path::new("/r/b.gba")).rating, 4, "saved");
        let before = std::fs::metadata(dir.join("library.json")).unwrap().modified().unwrap();
        assert_eq!(m.set_ratings_if_none(&list).unwrap(), 0, "the second time there is nothing to give");
        assert_eq!(std::fs::metadata(dir.join("library.json")).unwrap().modified().unwrap(), before, "and nothing is written");
    }

    #[test]
    fn a_rating_is_kept_per_game_written_only_when_given_and_an_old_reader_skips_it() {
        let d = dir("rating");
        let mut m = LibraryMeta::load(&d);
        assert_eq!(m.get(Path::new(ZELDA)).rating, 0, "not rated until the player says so");
        m.set_rating(Path::new(ZELDA), 4).unwrap();
        m.set_rating(Path::new(TETRIS), 9).unwrap();
        let again = LibraryMeta::load(&d);
        assert_eq!(again.get(Path::new(ZELDA)).rating, 4);
        assert_eq!(again.get(Path::new(TETRIS)).rating, RATING_MAX, "more than five is five");
        let text = fs::read_to_string(d.join("library.json")).unwrap();
        assert!(text.contains("\"rating\": 4"), "{text}");
        // An older pocket-ui's types know nothing of ratings and still read the file.
        #[derive(Deserialize)]
        struct OldGame {
            status: Status,
        }
        #[derive(Deserialize)]
        struct OldFile {
            games: BTreeMap<String, OldGame>,
        }
        let old: OldFile = serde_json::from_str(&text).expect("the old reader still reads it");
        assert_eq!(old.games[ZELDA].status, Status::NotStarted);
        // Taking the rating away leaves nothing behind for a game with nothing else.
        let mut m = again;
        m.set_rating(Path::new(TETRIS), 0).unwrap();
        assert_eq!(LibraryMeta::load(&d).get(Path::new(TETRIS)), GameMeta::default());
        assert!(!fs::read_to_string(d.join("library.json")).unwrap().contains("Tetris"));
    }

    #[test]
    fn an_older_or_hand_edited_file_still_reads() {
        let d = dir("compat");
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("library.json"),
            format!(r#"{{"version":1,"games":{{"{TETRIS}":{{"status":"completed"}}}}}}"#),
        )
        .unwrap();
        let m = LibraryMeta::load(&d);
        assert_eq!(m.status(Path::new(TETRIS)), Status::Completed);
        assert_eq!(m.get(Path::new(TETRIS)).sessions, 0);
    }
}
