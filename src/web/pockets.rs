//! Pocket Sync in the browser, read only (docs/SYNC-OWN.md, "Status"): how each paired Pocket stands ("Synced 3 min
//! ago", "Not reachable since 14:05"), the last saves that came and went, and the saves with two versions that wait for
//! the player's choice here. From the files Pocket Sync keeps in `<data>/pocket-sync/`: `prefs.json` (on or off),
//! `peers.json` (only each Pocket's name is read out: its key and address never leave), `log.json` and the record
//! `versions.json` (the open questions). Pairing, syncing and choosing stay on the Pocket: nothing here writes.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use super::api::{game_id, Ctx};
use crate::pocket_sync::log::{Dir, Last, Log};

/// Saves that came or went, at most this many (newest first).
const RECENT: usize = 20;
/// Open questions listed, at most.
const QUESTIONS: usize = 50;

fn last_key(l: Last) -> (&'static str, usize) {
    match l {
        Last::Never => ("never", 0),
        Last::Synced => ("synced", 0),
        Last::Asked => ("asked", 0),
        Last::Failed(n) => ("failed", n),
        Last::Unreachable => ("unreachable", 0),
        Last::Busy => ("busy", 0),
        Last::Unpaired => ("unpaired", 0),
        Last::Bad => ("bad", 0),
    }
}

/// The library's games by their folder below the ROMs folder and their name without the type, in small letters:
/// a save's path in Pocket Sync (`gba/Golden Sun.srm`, relative to the saves' folder) finds its game.
fn games_by_save(ctx: &Ctx) -> HashMap<(String, String), (String, String)> {
    let lib = ctx.library();
    let mut out = HashMap::new();
    for sys in &lib.systems {
        for g in &sys.games {
            let dir = g.path.parent().and_then(|d| d.strip_prefix(&ctx.roots.roms).ok()).map(|d| d.to_string_lossy().to_lowercase()).unwrap_or_default();
            let stem = g.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            out.entry((dir, stem)).or_insert_with(|| (game_id(&g.path), g.name.clone()));
        }
    }
    out
}

/// The game of a save's path, and the save's own file name.
fn game_of(rel: &str, games: &HashMap<(String, String), (String, String)>) -> Value {
    let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let found = games.get(&(dir.to_lowercase(), stem.to_string()));
    json!({ "file": file, "id": found.map(|g| g.0.clone()), "game": found.map(|g| g.1.clone()) })
}

/// `GET /api/pockets`.
pub fn overview(ctx: &Ctx) -> Value {
    let dir = ctx.shared.data_dir.join("pocket-sync");
    overview_in(ctx, &dir)
}

pub fn overview_in(ctx: &Ctx, dir: &Path) -> Value {
    let enabled = crate::pocket_sync_page::Prefs::load(dir).enabled;
    // Names only: the keys in that file stay where they are.
    let names: Vec<(String, String)> = crate::pocket_sync::net::Peers::load(dir).0.into_iter().map(|p| (p.id, p.name)).collect();
    let log = Log::load(dir);
    let questions: Vec<String> = crate::pocket_sync::Versions::load(&dir.join("versions.json"))
        .map(|(v, _)| v.0.into_iter().filter(|(_, r)| r.asking.is_some()).map(|(rel, _)| rel).take(QUESTIONS).collect())
        .unwrap_or_default();
    let games = if log.events.is_empty() && questions.is_empty() { HashMap::new() } else { games_by_save(ctx) };
    let name_of = |id: &str| names.iter().find(|(i, _)| i == id).map(|(_, n)| n.clone());
    let pockets: Vec<Value> = names
        .iter()
        .map(|(id, name)| {
            let s = log.state(id);
            let (last, failed) = last_key(s.last);
            json!({ "name": name, "last": last, "failed": failed, "okAt": s.ok_at, "downSince": s.down_since })
        })
        .collect();
    let recent: Vec<Value> = log
        .events
        .iter()
        .take(RECENT)
        .map(|e| {
            let mut v = game_of(&e.rel, &games);
            v["at"] = json!(e.at);
            v["dir"] = json!(if e.dir == Dir::From { "from" } else { "to" });
            v["pocket"] = json!(name_of(&e.peer));
            v
        })
        .collect();
    let questions: Vec<Value> = questions.iter().map(|rel| game_of(rel, &games)).collect();
    json!({
        "enabled": enabled,
        "now": crate::recents::now_secs(),
        "pockets": pockets,
        "recent": recent,
        "questions": questions,
    })
}
