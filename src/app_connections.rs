//! Part of the pocket-ui binary: Connections and Rediscover (docs/CURATOR.md "Connections", "Rediscover"). The logic is
//! `pocket_ui::connections`; this is the App's side:
//! - what Curator's catalog thread hands it ([`entries`], [`rediscover_input`]): the index is built there with the
//!   catalog, after the library is read, so a card only looks its facts up;
//! - the card's facts that lead somewhere (`card_connections`), and the list A opens from one: a shelf of the player's
//!   games (`Screen::Connection`), in every look, with B back to the card it came from;
//! - Rediscover in the Curator list (`Screen::Rediscover`).

use std::path::PathBuf;
use std::sync::Arc;

use pocket_ui::connections::{self, Kind, Why};
use pocket_ui::curator::{Catalog, Player};
use pocket_ui::game_card::CardData;

use super::app_library::{picked, Picked};
use super::app_card::OpenCard;
use super::{App, Item, Screen};

/// How many lists deep B remembers its way back (a card, its list, a game's card there, its list…).
const BACK_MAX: usize = 8;

/// What the App keeps for Connections and Rediscover (in `CuratorUi`).
#[derive(Default)]
pub(crate) struct ConnectionsUi {
    /// Built with Curator's catalog (`app_curator`); empty until then, so no fact leads anywhere for a moment.
    pub index: Arc<connections::Index>,
    /// Rediscover's games, from the same thread.
    pub rediscover: Vec<Rediscovered>,
    /// The list open as `Screen::Connection`.
    pub open: Option<OpenList>,
    /// Where B goes from a list: the screen, its selection and the card it was opened from, and the list before.
    back: Vec<Back>,
    /// The gamelists changed since the index was made: the next index build makes it again from the same catalog
    /// (a few milliseconds), instead of a whole new catalog.
    pub stale: bool,
}

/// A game of Rediscover: where the catalog had it, its file, why it is there and when it was last played.
pub(crate) type Rediscovered = ((usize, usize), PathBuf, Why, u64);

#[derive(Clone)]
pub(crate) struct OpenList {
    /// "More from Criterion Games".
    pub title: String,
    /// (system, game) when the list was made, and the file: checked against the library before use.
    pub games: Vec<((usize, usize), PathBuf)>,
}

struct Back {
    screen: Screen,
    sel: usize,
    /// A Games screen and Home entry by stable identity, since a rescan may move their indices.
    screen_system: Option<String>,
    home_item: Option<Picked>,
    list: Option<OpenList>,
    /// The card A came from: system and ROM path, never a saved library index.
    card: (String, PathBuf),
}

/// The catalog's games for the index, with what the player's EmulationStation gamelists add (a developer, a
/// publisher, a genre, a year). On Curator's thread.
pub(crate) fn entries(catalog: &Catalog, gamelist: &pocket_ui::gamelist::Index) -> Vec<connections::Entry> {
    catalog
        .games
        .iter()
        .map(|f| {
            let e = gamelist.get(&f.system, &f.path);
            let more = |own: &[String], es: Option<&String>| own.iter().cloned().chain(es.filter(|s| !s.is_empty()).cloned()).collect::<Vec<_>>();
            connections::Entry {
                at: f.at,
                path: f.path.clone(),
                name: f.name.clone(),
                year: f.year.or(e.map(|e| e.year).filter(|y| *y != 0)),
                values: [
                    more(&f.developers, e.map(|e| &e.developer)),
                    more(&f.publishers, e.map(|e| &e.publisher)),
                    f.series.clone(),
                    more(&f.genres, e.map(|e| &e.genre)),
                ],
            }
        })
        .collect()
}

/// Rediscover's games on Curator's thread: when each was last played (the recent games, the play history, an
/// EmulationStation gamelist's "last played") and the play history's sessions, for `connections::rediscover`.
pub(crate) fn rediscover_input(catalog: &Catalog, player: &Player, gamelist: &pocket_ui::gamelist::Index, log: Option<&pocket_ui::play_log::Log>, now: u64, tz: i32) -> Vec<Rediscovered> {
    let mut last: std::collections::HashMap<&std::path::Path, (u64, u64)> = std::collections::HashMap::new();
    let mut days = Vec::new();
    if let Some(log) = log {
        for s in &log.sessions {
            let Some(g) = log.games.get(s.game) else { continue };
            let p = std::path::Path::new(&g.path);
            // Only the sessions near a year ago today are looked at by day; the rest only for "last played".
            if now.saturating_sub(s.start) > connections::YEAR_SECS - 2 * 86_400 && now.saturating_sub(s.start) < connections::YEAR_SECS + 2 * 86_400 {
                days.push((p.to_path_buf(), s.start, s.tz));
            }
            let e = last.entry(p).or_default();
            // Synced history.json carries u64 values, unlike the own binary log's bounded u32 fields.
            e.0 = e.0.max(s.start.saturating_add(s.secs));
            e.1 = e.1.saturating_add(s.secs);
        }
    }
    let played: Vec<(PathBuf, u64, u64)> = catalog
        .games
        .iter()
        .filter_map(|f| {
            let meta = player.meta.get(&f.path.to_string_lossy().into_owned());
            let recent = player.recents.get(&f.path).copied().unwrap_or_default();
            let logged = last.get(f.path.as_path()).copied().unwrap_or_default();
            let es = gamelist.get(&f.system, &f.path).map_or(0, |e| e.last_played);
            let at = recent.0.max(logged.0).max(es);
            let secs = recent.1.max(logged.1).max(meta.map_or(0, |m| m.secs));
            (at > 0).then(|| (f.path.clone(), at, secs))
        })
        .collect();
    let at: std::collections::HashMap<&std::path::Path, (usize, usize)> = catalog.games.iter().map(|f| (f.path.as_path(), f.at)).collect();
    let when: std::collections::HashMap<&std::path::Path, u64> = played.iter().map(|(p, t, _)| (p.as_path(), *t)).collect();
    connections::rediscover(&played, &days, now, tz)
        .into_iter()
        .filter_map(|(p, w)| Some((*at.get(p.as_path())?, when.get(p.as_path()).copied().unwrap_or(0), p, w)))
        .map(|(at, last, p, w)| (at, p, w, last))
        .collect()
}

fn fill_connections(data: &mut CardData, path: &std::path::Path, ix: &connections::Index) {
    if !data.facts.iter().any(|(k, _)| *k == "Series") {
        if let Some(s) = ix.derived_series(path) {
            data.facts.push(("Series", s.to_string()));
        }
    }
    data.links = data
        .facts
        .iter()
        .enumerate()
        .filter_map(|(i, (k, v))| Some((i, ix.link(Kind::of_label(k)?, v, path)?)))
        .map(|(i, l)| (i, pocket_ui::game_card::CardLink { title: l.title(), games: l.games.iter().map(|&g| (ix.games[g].at, ix.games[g].path.clone())).collect() }))
        .collect();
}

impl OpenCard {
    fn refresh_connections(&mut self, ix: &connections::Index, entry: Option<&pocket_ui::gamelist::Entry>) {
        let picked = |link: Option<usize>| {
            link.and_then(|i| self.card.data.links.get(i)).and_then(|(f, _)| self.card.data.facts.get(*f)).cloned()
        };
        let info = picked(self.card.info.as_ref().and_then(|i| i.link));
        let boxed = picked(self.card.boxed.as_ref().and_then(|b| b.link));
        // Rebuild from the actual source, not the previous catalog's derived Series or old gamelist values.
        self.card.data.facts.clone_from(&self.source_facts);
        if let Some(e) = entry {
            pocket_ui::gamelist::fill_card(&mut self.card.data, e, true);
        }
        fill_connections(&mut self.card.data, &self.path, ix);
        let remap = |fact: Option<(&str, String)>| {
            fact.and_then(|fact| self.card.data.links.iter().position(|(f, _)| self.card.data.facts.get(*f) == Some(&fact)))
        };
        let (info, boxed) = (remap(info), remap(boxed));
        if let Some(i) = self.card.info.as_mut() {
            i.link = info;
        }
        if let Some(b) = self.card.boxed.as_mut() {
            b.link = boxed;
        }
    }
}

impl App {
    /// The card's facts that lead to the player's other games (`CardData::links`); a series the titles give is added as
    /// a fact where the data has none. A few lookups in the index, no more.
    pub(crate) fn card_connections(&self, data: &mut CardData, path: &std::path::Path) {
        fill_connections(data, path, &self.curator.connections.index);
    }

    /// The catalog finished while a card was open: facts and arrows update on that same card, without resetting its
    /// face, description or editing mode. Only in-memory facts and index lookups, no card reads or metadata resolve.
    pub(crate) fn refresh_card_connections(&mut self) {
        let Some(open) = self.card.as_mut() else { return };
        let entry = self.gamelist.index.get(&open.system, &open.path);
        open.refresh_connections(&self.curator.connections.index, entry.map(|e| e.as_ref()));
    }

    /// A on a fact of the card: its list, as a shelf of the player's games. B comes back to the card.
    pub(crate) fn open_connection(&mut self, i: usize) {
        let Some(open) = self.card.take() else { return };
        let Some((_, link)) = open.card.data.links.get(i).cloned() else {
            self.card = Some(open);
            return;
        };
        let screen_system = match self.screen {
            Screen::Games(s) => self.systems.get(s).map(|s| s.name.clone()),
            _ => None,
        };
        let home_item = self.home_item.map(|i| match i {
            Item::Recent(r) => self.recents.list.get(r).map_or(Picked::Other(i), |r| Picked::Game(r.path.clone())),
            _ => picked(i, &self.systems),
        });
        let c = &mut self.curator.connections;
        if c.back.len() >= BACK_MAX {
            c.back.remove(0);
        }
        c.back.push(Back {
            screen: self.screen,
            sel: self.sel,
            screen_system,
            home_item,
            list: c.open.take(),
            card: (open.system, open.path),
        });
        c.open = Some(OpenList {
            title: link.title,
            games: link.games,
        });
        if self.screen == Screen::Home {
            self.home_item = self.items.get(self.sel).copied();
        }
        self.screen = Screen::Connection;
        self.sel = 0;
        self.cabinet.dirty = true;
        self.rebuild();
        self.prev_sel = self.sel;
    }

    /// B on a list of Connections: back to the screen and the card it was opened from. False: not on one.
    pub(crate) fn connection_back(&mut self) -> bool {
        if self.screen != Screen::Connection {
            return false;
        }
        let Some(b) = self.curator.connections.back.pop() else {
            self.curator.connections.open = None;
            return false;
        };
        let (system, path) = b.card;
        let game = self
            .systems
            .iter()
            .enumerate()
            .find(|(_, s)| s.name == system)
            .and_then(|(s, sys)| sys.games.iter().position(|g| g.path == path).map(|g| (s, g)));
        let Some((s, g)) = game else {
            // The originating game went away while its connection list was open. No saved index is safe now.
            self.curator.connections.open = None;
            self.curator.connections.back.clear();
            self.screen = Screen::Home;
            self.home_item = None;
            self.rebuild();
            return true;
        };
        let item = Item::Game(s, g);
        let name = self.systems[s].games[g].name.clone();
        self.curator.connections.open = b.list;
        self.screen = match b.screen {
            Screen::Games(_) => b
                .screen_system
                .and_then(|name| self.systems.iter().position(|s| s.name == name))
                .map_or(Screen::Home, Screen::Games),
            other => other,
        };
        self.home_item = b.home_item.and_then(|p| match p {
            Picked::System(name) => self.systems.iter().position(|s| s.name == name).map(Item::System),
            Picked::Game(path) => self.recents.list.iter().position(|r| r.path == path).map(Item::Recent),
            Picked::Other(i) => Some(i),
        });
        self.rebuild();
        self.sel = self
            .items
            .iter()
            .position(|i| *i == item)
            .unwrap_or(b.sel.min(self.items.len().saturating_sub(1)));
        self.prev_sel = self.sel;
        self.cabinet.dirty = true;
        self.show_card(item, system, name, path);
        true
    }

    /// A rescan may reorder every system and game. Remap the lists that outlive the swap once, by ROM path.
    pub(crate) fn connections_follow_library(&mut self, fresh: &[pocket_ui::library::System]) {
        let c = &mut self.curator.connections;
        if c.open.is_none() && c.back.is_empty() && c.rediscover.is_empty() {
            return;
        }
        let at: std::collections::HashMap<_, _> = fresh
            .iter()
            .enumerate()
            .flat_map(|(s, sys)| sys.games.iter().enumerate().map(move |(g, game)| (game.path.as_path(), (s, g))))
            .collect();
        for list in c.open.iter_mut().chain(c.back.iter_mut().filter_map(|b| b.list.as_mut())) {
            list.games.retain_mut(|(place, path)| match at.get(path.as_path()) {
                Some(now) => {
                    *place = *now;
                    true
                }
                None => false,
            });
        }
        c.rediscover.retain_mut(|(place, path, _, _)| match at.get(path.as_path()) {
            Some(now) => {
                *place = *now;
                true
            }
            None => false,
        });
    }

    /// The real rescan/Back transition for Connections' render regressions. Both libraries are throwaway fixtures;
    /// the second configuration changes system order, available games, or removes the originating system entirely.
    pub(crate) fn connection_rescan_snapshot(&mut self, system: &str, title: &str) {
        self.theme = pocket_ui::settings::Theme::Minimal;
        let s = self.systems.iter().position(|s| s.name == system).expect("snapshot's system");
        let g = self.systems[s]
            .games
            .iter()
            .position(|g| g.name.to_lowercase().contains(title))
            .expect("snapshot's game");
        let path = self.systems[s].games[g].path.clone();
        self.screen = Screen::Games(s);
        self.rebuild();
        self.sel = g;
        self.open_card();
        assert!(!self.card.as_ref().unwrap().card.data.links.is_empty(), "snapshot's fact must lead somewhere");
        self.open_connection(0);
        let before: Vec<_> = self.curator.connections.open.as_ref().unwrap().games.iter().map(|(_, p)| p.clone()).collect();
        let fresh = pocket_ui::library::load_shown(&self.es_systems.with_file_name("rescan.cfg"), &self.data_dir).expect("snapshot's rescanned library");
        let origin = fresh
            .iter()
            .enumerate()
            .find(|(_, sys)| sys.name == system)
            .and_then(|(s, sys)| sys.games.iter().position(|g| g.path == path).map(|g| (s, g)));
        let surviving = before.iter().filter(|p| fresh.iter().any(|s| s.games.iter().any(|g| &g.path == *p))).count();
        self.library_rescanned(fresh, None);
        assert!(self.library_pending().is_none(), "an open connection must allow the rescan to apply");
        assert_eq!(self.curator.connections.open.as_ref().unwrap().games.len(), surviving);
        // The old Curator catalog must not drop these fresh indices while its replacement is still being built.
        assert_eq!(self.items.len(), surviving, "the visible connection list keeps its surviving games before Built");
        for (at, p) in &self.curator.connections.open.as_ref().unwrap().games {
            assert!(self.item_at(*at, p).is_some(), "surviving games must follow the rescan");
            assert!(self.items.contains(&Item::Game(at.0, at.1)), "the remapped game must actually be visible");
        }
        self.press(super::CANCEL);
        if let Some((s, g)) = origin {
            assert!(self.screen == Screen::Games(s));
            assert_eq!(self.items[self.sel], Item::Game(s, g));
            let card = self.card.as_ref().expect("back to the originating card");
            assert_eq!(card.item, Item::Game(s, g));
            assert_eq!(card.path, path);
            assert_eq!(card.name, self.systems[s].games[g].name);
        } else {
            assert!(self.screen == Screen::Home);
            assert!(self.card.is_none(), "a deleted origin has no card");
        }
    }

    /// The library's item for a game of a list, while the library still has that file there.
    fn item_at(&self, (s, g): (usize, usize), path: &std::path::Path) -> Option<Item> {
        (self.systems.get(s)?.games.get(g)?.path == path).then_some(Item::Game(s, g))
    }

    /// The games of the open list or of Rediscover, as the library has them now.
    pub(crate) fn connection_items(&self, screen: Screen) -> Vec<Item> {
        let c = &self.curator.connections;
        let items = match screen {
            Screen::Connection => c.open.iter().flat_map(|l| l.games.iter()).filter_map(|(at, p)| self.item_at(*at, p)).collect(),
            Screen::Rediscover => c.rediscover.iter().filter_map(|(at, p, _, _)| self.item_at(*at, p)).collect(),
            _ => Vec::new(),
        };
        // One game on two systems once, the version the player prefers, as Curator's other lists (Settings → Curator).
        self.curator_merge(items)
    }

    pub(crate) fn connection_title(&self, screen: Screen) -> Option<String> {
        match screen {
            Screen::Connection => self.curator.connections.open.as_ref().map(|l| l.title.clone()),
            Screen::Rediscover => Some("Rediscover".into()),
            _ => None,
        }
    }

    /// Rediscover's games in the library, and how many were played a year ago today.
    pub(crate) fn rediscover_counts(&self) -> (usize, usize) {
        let c = &self.curator.connections;
        let here = c.rediscover.iter().filter(|(at, p, _, _)| self.item_at(*at, p).is_some());
        let (all, today) = here.fold((0, 0), |(a, t), (_, _, w, _)| (a + 1, t + usize::from(*w == Why::YearAgoToday)));
        (all, today)
    }

    /// The line under a game in Rediscover: why it is there ("played a year ago today", "last played 412 days ago"),
    /// in place of the shelf's usual "not played yet" (it may have been played before pocket-ui knew it).
    pub(crate) fn rediscover_why(&self, path: &std::path::Path) -> Option<String> {
        if self.screen != Screen::Rediscover {
            return None;
        }
        self.curator.connections.rediscover.iter().find(|(_, p, _, _)| p == path).map(|(_, _, w, last)| match w {
            Why::YearAgoToday => "played a year ago today".to_string(),
            Why::NotForAYear => format!("last played {}", pocket_ui::recents::ago(*last)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocket_ui::{curator, library_sync, play_log};
    use pocket_ui::game_card::{Card, Out};
    use pocket_ui::pad::Button;

    /// Use the actual synced JSON schema and conversion; the own binary log alone cannot supply these values.
    fn synced_log(sessions: serde_json::Value) -> play_log::Log {
        let history: library_sync::History = serde_json::from_value(serde_json::json!({
            "version": 1, "sessions": sessions
        })).unwrap();
        let own = play_log::Log::default();
        let extra = library_sync::extra(&history, &own, &library_sync::Local::default(), &|key| Some(PathBuf::from(format!("/roms/{key}"))));
        play_log::combine(&own, &extra)
    }

    #[test]
    fn synced_history_cannot_wrap_rediscover_end_times_or_ranking() {
        let paths = [PathBuf::from("/roms/gba/a.gba"), PathBuf::from("/roms/gba/b.gba")];
        let games = paths.iter().enumerate().map(|(g, path)| curator::features_for(
            curator::GameRef { at: (0, g), system: "gba", path, name: "Test game" }, None, &[]
        )).collect();
        let catalog = Catalog::build(games, Vec::new());
        let now = 1_800_000_000;
        let mut player = Player { now, ..Default::default() };
        player.recents.insert(paths[0].clone(), (1, 0));
        let log = synced_log(serde_json::json!([
            {"g": "gba/a.gba", "s": 1, "secs": u64::MAX}
        ]));
        assert!(rediscover_input(&catalog, &player, &Default::default(), Some(&log), now, 0).is_empty(),
            "a huge session end must not wrap to an old timestamp and offer a false rediscovery");
        let log = synced_log(serde_json::json!([
            {"g": "gba/a.gba", "s": i64::MAX as u64, "secs": 0, "tz": 3600}
        ]));
        assert!(rediscover_input(&catalog, &player, &Default::default(), Some(&log), now, 3600).is_empty(),
            "future synced timestamps must be rejected before local_day adds the real UTC offset");
        let year_ago = now - connections::YEAR_SECS;
        let log = synced_log(serde_json::json!([
            {"g": "gba/a.gba", "s": year_ago, "secs": u64::MAX},
            {"g": "gba/a.gba", "s": year_ago + 1, "secs": 2},
            {"g": "gba/b.gba", "s": year_ago, "secs": u64::MAX - 1}
        ]));
        let found = rediscover_input(&catalog, &player, &Default::default(), Some(&log), now, 0);
        assert_eq!(found.iter().map(|(_, p, _, _)| p).collect::<Vec<_>>(), paths.iter().collect::<Vec<_>>(),
            "total play time saturates instead of wrapping the longest-played game behind the other one");
        assert!(found.iter().all(|(_, _, why, last)| *why == Why::YearAgoToday && *last == u64::MAX));
    }

    fn entry(i: usize, name: &str) -> connections::Entry {
        connections::Entry {
            at: (0, i), path: format!("/roms/{i}.gb").into(), name: name.into(),
            values: [vec!["Example Studio".into()], vec![], vec![], vec![]], ..Default::default()
        }
    }

    #[test]
    fn an_open_card_gets_links_and_drops_obsolete_derived_facts_on_catalog_reload() {
        let source_facts = vec![("Developer", "Example Studio".into())];
        let mut open = OpenCard {
            card: Card::new(CardData { facts: source_facts.clone(), ..Default::default() }),
            source_facts, item: Item::Game(0, 0), system: "gb".into(), name: "Pocket 2".into(),
            path: "/roms/0.gb".into(), article: None,
        };
        open.card.show_info();
        assert_eq!(open.card.press(Button::Down), Out::None);
        assert!(open.card.data.links.is_empty(), "card opens before the catalog is ready");
        let first = connections::Index::build(vec![entry(0, "Pocket 2"), entry(1, "Pocket 3")]);
        open.refresh_connections(&first, None);
        assert_eq!(open.card.data.facts.last(), Some(&("Series", "Pocket".into())));
        open.card.press(Button::Down);
        assert_eq!(open.card.press(Button::East), Out::Connection(0), "usable without closing the card");
        open.card.press(Button::Down);
        assert_eq!(open.card.info.as_ref().unwrap().link, Some(1));
        let second = connections::Index::build(vec![entry(0, "Pocket 2"), entry(1, "Elsewhere")]);
        open.refresh_connections(&second, None);
        assert_eq!(open.card.data.facts, open.source_facts, "the old inferred series has gone");
        assert_eq!(open.card.info.as_ref().unwrap().link, None, "a disappeared selected fact is released");
        assert_eq!(open.card.press(Button::East), Out::Play);
        // Real source metadata survives an index with no inferred series; changed gamelist values replace old ones.
        open.source_facts.push(("Series", "Actual series".into()));
        let gamelist = pocket_ui::gamelist::Entry { publisher: "Old Publisher".into(), ..Default::default() };
        open.refresh_connections(&first, Some(&gamelist));
        let gamelist = pocket_ui::gamelist::Entry { publisher: "New Publisher".into(), ..Default::default() };
        open.refresh_connections(&second, Some(&gamelist));
        assert!(open.card.data.facts.contains(&("Series", "Actual series".into())));
        assert!(open.card.data.facts.contains(&("Publisher", "New Publisher".into())));
        assert!(!open.card.data.facts.iter().any(|(_, value)| value == "Old Publisher"));
        open.card.boxed = Some(pocket_ui::game_box::BoxView::new());
        open.card.boxed.as_mut().unwrap().link = Some(usize::MAX);
        open.refresh_connections(&second, None);
        assert!(open.card.boxed.as_ref().unwrap().link.is_none(), "stale box focus is also released");
    }
}
