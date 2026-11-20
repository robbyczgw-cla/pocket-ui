//! What Pocket sync did, kept for the player (docs/SYNC-OWN.md, "Status"): how each paired Pocket stands (synced
//! when, not reachable since when, what went wrong) and the last saves that came and went, each with its game, the
//! other Pocket and the time. `<data>/pocket-sync/log.json`, written after each sync and each save a Pocket sent
//! here. The Pockets page, the shelf line and the game card read it; nothing in sync's decisions does.
//!
//! A lost or unreadable log is an empty one: it only says what happened, so nothing breaks without it.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use super::net::{Report, SyncError};

/// Saves kept in the list of what came and went (newest first): enough for every paired Pocket's page.
pub const MAX_EVENTS: usize = 100;

/// Which way a save went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// From the other Pocket to this one.
    From,
    /// From this Pocket to the other.
    To,
}

/// One save that came or went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// Unix seconds.
    pub at: u64,
    /// The other Pocket's id.
    pub peer: String,
    pub dir: Dir,
    /// The save, as this Pocket has it (relative to its saves folder).
    pub rel: String,
}

/// How the last sync with a Pocket ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Last {
    #[default]
    Never,
    Synced,
    /// A save was played on both: the player chooses.
    Asked,
    /// It answered, but some saves did not go or come (each is tried again next time).
    Failed(usize),
    Unreachable,
    /// A game runs there.
    Busy,
    /// It does not know this Pocket any more.
    Unpaired,
    /// It answered in a way that was not signed right or not understood.
    Bad,
}

/// A paired Pocket's state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub last: Last,
    /// The last sync that went through (Unix seconds).
    pub ok_at: Option<u64>,
    /// Not reachable since (the first try that failed after the last that went through).
    pub down_since: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    pub peers: BTreeMap<String, State>,
    /// Newest first.
    pub events: Vec<Event>,
}

const FILE: &str = "log.json";

impl Log {
    pub fn load(dir: &Path) -> Log {
        let v: Value = std::fs::read(dir.join(FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let peers = v["peers"]
            .as_object()
            .map(|m| m.iter().map(|(id, s)| (id.clone(), state_of(s))).collect())
            .unwrap_or_default();
        let events = v["events"].as_array().map(|a| a.iter().filter_map(event_of).take(MAX_EVENTS).collect()).unwrap_or_default();
        Log { peers, events }
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let peers: serde_json::Map<String, Value> = self.peers.iter().map(|(id, s)| (id.clone(), state_json(s))).collect();
        let events: Vec<Value> = self.events.iter().map(event_json).collect();
        crate::fsutil::write_atomic(&dir.join(FILE), json!({"peers": peers, "events": events}).to_string().as_bytes())
    }

    /// A sync this Pocket ran with `peer` at `at`.
    pub fn synced(&mut self, peer: &str, at: u64, r: &Result<Report, SyncError>) {
        let s = self.peers.entry(peer.to_string()).or_default();
        s.last = match r {
            Ok(rep) if !rep.failed.is_empty() => Last::Failed(rep.failed.len()),
            Ok(rep) if !rep.asked.is_empty() => Last::Asked,
            Ok(_) => Last::Synced,
            Err(SyncError::Unreachable) => Last::Unreachable,
            Err(SyncError::Busy) => Last::Busy,
            Err(SyncError::Unpaired) => Last::Unpaired,
            Err(SyncError::Bad) => Last::Bad,
        };
        if s.last == Last::Unreachable {
            s.down_since = s.down_since.or(Some(at));
        } else {
            s.down_since = None;
        }
        if let Ok(rep) = r {
            s.ok_at = Some(at);
            let came = rep.fetched.iter().map(|rel| (Dir::From, rel));
            let went = rep.sent.iter().map(|rel| (Dir::To, rel));
            self.add(peer, at, came.chain(went));
        }
    }

    /// `peer` sent saves here (its own sync, after a game there): it reached this Pocket, so it stands synced.
    pub fn received(&mut self, peer: &str, at: u64, rels: &[String]) {
        if rels.is_empty() {
            return;
        }
        let s = self.peers.entry(peer.to_string()).or_default();
        s.ok_at = Some(at);
        s.down_since = None;
        if matches!(s.last, Last::Never | Last::Unreachable | Last::Busy) {
            s.last = Last::Synced;
        }
        self.add(peer, at, rels.iter().map(|r| (Dir::From, r)));
    }

    fn add<'a>(&mut self, peer: &str, at: u64, saves: impl Iterator<Item = (Dir, &'a String)>) {
        let new: Vec<Event> = saves.map(|(dir, rel)| Event { at, peer: peer.to_string(), dir, rel: rel.clone() }).collect();
        if new.is_empty() {
            return;
        }
        self.events.splice(0..0, new);
        self.events.truncate(MAX_EVENTS);
    }

    pub fn state(&self, peer: &str) -> State {
        self.peers.get(peer).copied().unwrap_or_default()
    }

    /// What came from and went to `peer`, newest first.
    pub fn of<'a>(&'a self, peer: &'a str) -> impl Iterator<Item = &'a Event> + 'a {
        self.events.iter().filter(move |e| e.peer == peer)
    }

    /// The newest save that came or went for the game at `game` (its ROM) of `system`: see [`save_of`].
    pub fn last_for(&self, system: &str, game: &Path) -> Option<&Event> {
        self.events.iter().find(|e| save_of(&e.rel, system, game))
    }

    /// A forgotten Pocket: its state and its part of the list go.
    pub fn forget(&mut self, peer: &str) {
        self.peers.remove(peer);
        self.events.retain(|e| e.peer != peer);
    }
}

/// `rel` (`gba/Golden Sun.srm`, from the saves folder) is a save of the game at `game` (`/roms/gba/Golden Sun.gba`):
/// the same name without the extension, in a folder of the same name (the battery save and the clock, `.rtc`, both
/// count; a game one level down in its system folder too). Where saves have a folder of their own (the Brick, dArkOS:
/// `<saves>/<system>/<game>.srm`), the system's id is the folder.
pub fn save_of(rel: &str, system: &str, game: &Path) -> bool {
    let in_system = rel.split_once('/').is_some_and(|(d, f)| d == system && !f.contains('/'));
    beside(rel, game) || (in_system && game.file_stem().and_then(|s| s.to_str()).is_some_and(|g| stem_of(rel) == g))
}

fn stem_of(rel: &str) -> &str {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    file.rsplit_once('.').map_or(file, |(s, _)| s)
}

fn beside(rel: &str, game: &Path) -> bool {
    let Some((dirs, file)) = rel.rsplit_once('/') else { return false };
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let (Some(game_stem), Some(parent)) = (game.file_stem().and_then(|s| s.to_str()), game.parent()) else { return false };
    if stem != game_stem {
        return false;
    }
    let mut up = parent.components().rev();
    dirs.rsplit('/').all(|d| up.next().and_then(|c| c.as_os_str().to_str()).is_some_and(|c| c.eq_ignore_ascii_case(d)))
}

fn state_of(v: &Value) -> State {
    let last = match v["last"].as_str().unwrap_or("") {
        "synced" => Last::Synced,
        "asked" => Last::Asked,
        "failed" => Last::Failed(v["failed"].as_u64().unwrap_or(1) as usize),
        "unreachable" => Last::Unreachable,
        "busy" => Last::Busy,
        "unpaired" => Last::Unpaired,
        "bad" => Last::Bad,
        _ => Last::Never,
    };
    State { last, ok_at: v["ok_at"].as_u64(), down_since: v["down_since"].as_u64() }
}

fn state_json(s: &State) -> Value {
    let (last, failed) = match s.last {
        Last::Never => ("never", 0),
        Last::Synced => ("synced", 0),
        Last::Asked => ("asked", 0),
        Last::Failed(n) => ("failed", n),
        Last::Unreachable => ("unreachable", 0),
        Last::Busy => ("busy", 0),
        Last::Unpaired => ("unpaired", 0),
        Last::Bad => ("bad", 0),
    };
    let mut v = json!({"last": last, "ok_at": s.ok_at, "down_since": s.down_since});
    if failed > 0 {
        v["failed"] = json!(failed);
    }
    v
}

fn event_of(v: &Value) -> Option<Event> {
    let dir = match v["dir"].as_str()? {
        "from" => Dir::From,
        "to" => Dir::To,
        _ => return None,
    };
    Some(Event { at: v["at"].as_u64()?, peer: v["peer"].as_str()?.to_string(), dir, rel: v["rel"].as_str()?.to_string() })
}

fn event_json(e: &Event) -> Value {
    json!({"at": e.at, "peer": e.peer, "dir": if e.dir == Dir::From { "from" } else { "to" }, "rel": e.rel})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(sent: &[&str], fetched: &[&str]) -> Report {
        Report { sent: sent.iter().map(|s| s.to_string()).collect(), fetched: fetched.iter().map(|s| s.to_string()).collect(), ..Report::default() }
    }

    #[test]
    fn a_sync_keeps_how_it_stands_and_each_save_that_came_or_went() {
        let mut l = Log::default();
        assert_eq!(l.state("P"), State::default(), "never synced");
        l.synced("P", 100, &Ok(rep(&["gba/Golden Sun.srm"], &["gbc/Pokemon Gold.srm"])));
        assert_eq!(l.state("P"), State { last: Last::Synced, ok_at: Some(100), down_since: None });
        let e: Vec<_> = l.of("P").map(|e| (e.dir, e.rel.as_str())).collect();
        assert_eq!(e, vec![(Dir::From, "gbc/Pokemon Gold.srm"), (Dir::To, "gba/Golden Sun.srm")]);
        // Not reachable: since the first try that failed; the last good one stays.
        l.synced("P", 200, &Err(SyncError::Unreachable));
        l.synced("P", 300, &Err(SyncError::Unreachable));
        assert_eq!(l.state("P"), State { last: Last::Unreachable, ok_at: Some(100), down_since: Some(200) });
        l.synced("P", 400, &Ok(Report::default()));
        assert_eq!(l.state("P"), State { last: Last::Synced, ok_at: Some(400), down_since: None });
        // What went wrong, each its own.
        l.synced("P", 500, &Ok(Report { failed: vec!["a/b.srm".into(), "a/c.srm".into()], ..Report::default() }));
        assert_eq!(l.state("P").last, Last::Failed(2));
        l.synced("P", 510, &Ok(Report { asked: vec!["a/b.srm".into()], ..Report::default() }));
        assert_eq!(l.state("P").last, Last::Asked);
        for (err, last) in [(SyncError::Busy, Last::Busy), (SyncError::Unpaired, Last::Unpaired), (SyncError::Bad, Last::Bad)] {
            l.synced("P", 600, &Err(err));
            assert_eq!(l.state("P").last, last);
            assert_eq!(l.state("P").down_since, None);
        }
    }

    #[test]
    fn saves_another_pocket_sent_count_as_a_sync_with_it() {
        let mut l = Log::default();
        l.synced("Q", 50, &Err(SyncError::Unreachable));
        l.received("Q", 90, &["gba/Mother 3.srm".to_string()]);
        assert_eq!(l.state("Q"), State { last: Last::Synced, ok_at: Some(90), down_since: None });
        assert_eq!(l.of("Q").next().map(|e| e.dir), Some(Dir::From));
        // A question open stays a question.
        l.synced("Q", 95, &Ok(Report { asked: vec!["x/y.srm".into()], ..Report::default() }));
        l.received("Q", 99, &["gba/Mother 3.srm".to_string()]);
        assert_eq!(l.state("Q").last, Last::Asked);
        l.received("Q", 120, &[]);
        assert_eq!(l.state("Q").ok_at, Some(99), "nothing came, nothing changes");
    }

    #[test]
    fn the_list_keeps_the_newest_and_a_forgotten_pocket_leaves_it() {
        let mut l = Log::default();
        for i in 0..(MAX_EVENTS as u64 + 30) {
            l.synced(if i % 2 == 0 { "A" } else { "B" }, i, &Ok(rep(&[&format!("gba/G{i}.srm")], &[])));
        }
        assert_eq!(l.events.len(), MAX_EVENTS);
        assert_eq!(l.events[0].at, MAX_EVENTS as u64 + 29, "newest first");
        l.forget("A");
        assert!(l.events.iter().all(|e| e.peer == "B"));
        assert_eq!(l.state("A"), State::default());
    }

    fn save_of_t(rel: &str, game: &Path) -> bool {
        save_of(rel, "gba", game)
    }

    #[test]
    fn a_save_belongs_to_its_game_by_folder_and_name() {
        let g = Path::new("/storage/roms/gba/Golden Sun (USA, Europe).gba");
        assert!(save_of_t("gba/Golden Sun (USA, Europe).srm", g));
        assert!(save_of_t("gba/Golden Sun (USA, Europe).rtc", g), "the clock too");
        assert!(save_of_t("GBA/Golden Sun (USA, Europe).srm", g), "a card that does not care for letter case");
        assert!(!save_of_t("gbc/Golden Sun (USA, Europe).srm", g), "another system");
        assert!(!save_of_t("gba/Golden Sun (USA).srm", g));
        assert!(!save_of_t("Golden Sun (USA, Europe).srm", g), "no folder");
        let deeper = Path::new("/storage/roms/gba/RPG/Golden Sun.gba");
        assert!(save_of_t("gba/RPG/Golden Sun.srm", deeper));
        // pocket-core keeps a save in `<saves>/<system>/` for a game in any folder of its system (the Brick, dArkOS): the
        // system's top is that game's save too.
        assert!(save_of_t("gba/Golden Sun.srm", deeper));
        assert!(!save_of_t("gba/Other/Golden Sun.srm", deeper), "a namesake in another folder is another game");
        // Saves in a folder of their own (Brick, dArkOS): by the system's id, whatever the games' folder is called.
        let brick = Path::new("/mnt/SDCARD/Roms/Game Boy Advance (GBA)/Golden Sun.gba");
        assert!(save_of("gba/Golden Sun.srm", "gba", brick));
        assert!(!save_of("gbc/Golden Sun.srm", "gba", brick));
        assert!(!save_of("gba/RPG/Golden Sun.srm", "gba", brick));
        let mut l = Log::default();
        l.synced("P", 10, &Ok(rep(&["gba/Golden Sun (USA, Europe).srm"], &[])));
        l.synced("P", 20, &Ok(rep(&[], &["gbc/Tetris.srm"])));
        assert_eq!(l.last_for("gba", g).map(|e| e.at), Some(10));
        assert_eq!(l.last_for("snes", Path::new("/r/snes/Zelda.sfc")), None);
    }

    #[test]
    fn the_log_is_kept_on_the_card() {
        let d = crate::testutil::TestDir::new("pocket-sync-log");
        let mut l = Log::default();
        l.synced("P", 100, &Ok(rep(&["gba/A.srm"], &["gba/B.srm"])));
        l.synced("Q", 200, &Err(SyncError::Unreachable));
        l.synced("R", 300, &Ok(Report { failed: vec!["x/y.srm".into(); 3], ..Report::default() }));
        l.save(&d).unwrap();
        assert_eq!(Log::load(&d), l);
    }

    // The file-fault rule (tests/data_sources.rs, scripts/reader-inventory.py): the log missing, empty, cut, flipped
    // or in another encoding never panics and reads as an empty or a shorter log (it only tells what happened).
    #[test]
    fn file_source_rule_pocket_sync_log() {
        let d = crate::testutil::TestDir::new("pocket-sync-log-faults");
        let mut l = Log::default();
        l.synced("P", 100, &Ok(rep(&["gba/A.srm"], &["gba/B.srm"])));
        l.synced("Q", 200, &Err(SyncError::Unreachable));
        l.save(&d).unwrap();
        let seed = std::fs::read(d.join(FILE)).unwrap();
        for (label, bytes) in crate::file_faults::faults(&seed) {
            let _ = std::fs::remove_file(d.join(FILE));
            if let Some(b) = &bytes {
                std::fs::write(d.join(FILE), b).unwrap();
            }
            let got = Log::load(&d);
            if label == "valid" {
                assert_eq!(got, l);
            }
            assert!(got.events.len() <= l.events.len(), "{label}");
        }
    }
}
