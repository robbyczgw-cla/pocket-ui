//! Part of the pocket-ui binary: the pause menu's Earlier row on pocket-ui's own host (`pocket_ui::timeline`).
//!
//! When the menu opens, the row lists the game's earlier states, newest first: the ring (one every 5 minutes of play)
//! and where sessions began. That is a few `stat`s of known file names, no folder listing. The row is there while
//! Earlier states is On for the game (Options → Extras) or while there are states of it to load. Left/Right walk
//! through them with their pictures; A loads one: pocket-core keeps what runs now as Before load first, as every Load
//! does. With none yet the row says when the first comes; an empty file says it can't be loaded.

use std::time::SystemTime;

use pocket_ui::image::{self, Image};
use pocket_ui::options::GameSettings;
use pocket_ui::pause_saves::Saves;
use pocket_ui::slots;
use pocket_ui::timeline::{self, Entry, Kind, Name, Row};

use super::{App, MainItem, Running};

/// The row for the game the menu opens over; None: no row (not pocket-core, no save states, or Earlier off and
/// nothing kept).
pub(crate) fn row_at_open(run: &Running, saves: Saves, settings: Option<&GameSettings>) -> Option<Row> {
    if !run.hosted() || saves != Saves::Slots {
        return None;
    }
    let stem = run.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let entries = timeline::entries(&slots::base(&run.system, stem));
    let now = pocket_ui::recents::now_secs();
    (settings.is_some_and(on) || !entries.is_empty()).then(|| Row::new(entries, pocket_ui::appearance::utc_offset_now(now as i64)))
}

/// Earlier states is On for this game.
fn on(s: &GameSettings) -> bool {
    pocket_ui::options::setting(timeline::KEY).is_some_and(|set| set.choices[s.index(set)].value == "on")
}

/// The picture and time of the state chosen; None when there is none.
pub(crate) fn preview(row: &Row) -> Option<(Option<Image>, SystemTime)> {
    row.selected().map(|e| (image::load_png(&slots::preview_path(&e.path)), e.at))
}

/// The chip beside Earlier ("12 min ago"); none while there are no states.
pub(crate) fn chip(row: &Row) -> Option<String> {
    row.selected().map(|e| timeline::chip(e, pocket_ui::recents::now_secs(), row.tz))
}

/// The line under the picture.
pub(crate) fn note(row: &Row) -> String {
    timeline::note(row.selected(), pocket_ui::recents::now_secs(), row.tz)
}

impl App {
    /// Snapshots: "slot-earlier" (Earlier chosen, on the state from 12 min ago with its picture), "slot-earlier:start"
    /// (on where the session began, today 14:02), "slot-earlier:none" (none yet), "slot-earlier:damaged" (an empty
    /// file, without a picture).
    pub(crate) fn earlier_snapshot(&mut self, state: &str, picture: Option<Image>) {
        if state.split(':').next() != Some("slot-earlier") {
            return;
        }
        let case = state.split(':').nth(1).unwrap_or("");
        let now = SystemTime::now();
        let ago = |m: u64| now - std::time::Duration::from_secs(m * 60);
        let entry = |kind, n, at, bytes| Entry { name: Name { kind, n }, path: Default::default(), at, bytes };
        // Today 14:02 in the snapshot's zone: the zone's offset is taken as UTC (`tz` 0) to stay the same anywhere.
        let start = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs({
            let secs = pocket_ui::recents::now_secs();
            let day = secs - secs % 86_400;
            (day + 14 * 3600 + 2 * 60).min(secs)
        });
        let mut entries = vec![entry(Kind::Every, 3, ago(12), 400_000), entry(Kind::Every, 2, ago(17), 400_000), entry(Kind::Every, 1, ago(22), 400_000)];
        entries.push(entry(Kind::Started, 1, start, 400_000));
        let (entries, at) = match case {
            "none" => (Vec::new(), 0),
            "start" => (entries, 3),
            "damaged" => {
                entries[0].bytes = 0;
                (entries, 0)
            }
            _ => (entries, 0),
        };
        let Some(menu) = self.menu.as_mut() else { return };
        let mut row = Row::new(entries, 0);
        row.at = at;
        menu.saved = row.selected().map(|e| (picture.filter(|_| !row.selected().is_some_and(Entry::damaged)), e.at));
        menu.earlier = Some(row);
        menu.sel = menu.items().iter().position(|i| *i == MainItem::Earlier).unwrap_or(menu.sel);
    }
}
