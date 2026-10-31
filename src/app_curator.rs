//! Part of the pocket-ui binary: Curator (docs/CURATOR.md). The engine is `pocket_ui::curator` and the pages' logic
//! `pocket_ui::curator_page`; this is the App's side:
//! - the index, built on a thread of its own after the first frame (and again after a change), never while a game
//!   runs; until it is there the Curator entries say "Getting ready…";
//! - Home's Curator row, pinned smart shelves and stickers, the Curator list and the lists they open;
//! - the Lens, the Echoes and Versions pages, the stickers of a game, naming a shelf or sticker;
//! - the order of the Lens's games and of each smart shelf (SELECT on the Lens, a shelf's options), and "Surprise me"
//!   in the Curator list, which opens the picked game's card with the reason;
//! - the game card's rows, Settings → Curator, and `--snapshot curator:…`.
//!
//! main.rs only carries small hooks: the new items and screens call into here.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use pocket_ui::case_stickers::{self, CaseStickers, Kind as CaseKind};
use pocket_ui::cabinet::ShelfKey;
use pocket_ui::curator::prefs::Prefs;
use pocket_ui::curator::sort::Order;
use pocket_ui::curator::surprise::Rng;
use pocket_ui::curator::today::Today;
use pocket_ui::curator::{self, echoes, Facet, Index, Lens, LibraryGame, Player, SystemName};
use pocket_ui::curator_page::{Col, EchoesPage, LensOut, LensPage, ListOut, Sheet, SheetOut, VersionRow, VersionsPage};
use pocket_ui::game_card::{self, CuratorRow, RowKind};
use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::keyboard::{Keyboard, Outcome};
use pocket_ui::library_meta::Status;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{stripe, system_label, system_short, App, Item, Screen, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Curator";
/// Stripe ids for Curator's rows (coloured like a system without one of its own).
const CURATOR_STRIPE: &str = "curator";
const STICKER_STRIPE: &str = "sticker";
/// Games named inline on the card's Echoes row.
const INLINE: usize = 3;
/// Games not on this device named under Echoes, when switched on.
const ELSEWHERE: usize = 6;
/// Up next: a game played at least this long before it ended gets a suggestion (a start and a quick quit doesn't).
const UP_NEXT_AFTER: std::time::Duration = std::time::Duration::from_secs(60);
/// How long Up next stays on the shelf when nothing is pressed; then it goes, and A is the shelf's again.
pub(crate) const UP_NEXT_SHOWN: std::time::Duration = std::time::Duration::from_secs(8);

/// News from the index thread.
pub(crate) enum Event {
    /// A new index; `gen` is the library generation its catalog was made from. `up_next`: the suggestion after the
    /// game that just ended (its catalog number and reason), worked out on the thread with the index. `today`: what
    /// Home's Curator row offers (the day's pick, the games waiting), also from the thread.
    /// `connections`: Connections' index, made with a new catalog (`app_connections`); `rediscover`: Rediscover's games.
    Built {
        gen: u64,
        index: Arc<Index>,
        catalog_ms: u128,
        index_ms: u128,
        up_next: Option<(usize, String)>,
        today: Today,
        connections: Option<Arc<pocket_ui::connections::Index>>,
        rediscover: Vec<super::app_connections::Rediscovered>,
    },
    /// Games not on this device for the Echoes page of `seed` (catalog number).
    Elsewhere { seed: usize, names: Vec<String> },
    /// Play history: a period counted on its thread for History or the review (`app_history`).
    History(super::app_history::Counted),
    /// A case-sticker edit could not reach the SD card (also delivered after its sheet is closed).
    StickerSaveFailed(std::io::Error),
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::Built { gen, index, .. } => write!(f, "Built {{ gen: {gen}, games: {} }}", index.len()),
            Event::Elsewhere { seed, names } => write!(f, "Elsewhere {{ seed: {seed}, names: {} }}", names.len()),
            Event::History(_) => write!(f, "History"),
            Event::StickerSaveFailed(e) => write!(f, "StickerSaveFailed({e})"),
        }
    }
}

/// What a shelf's or sticker's options sheet offers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Act {
    Edit,
    Rename,
    /// The order of a smart shelf's games.
    Order,
    Pin,
    Delete,
}

/// What a name typed on the keyboard is for.
pub(crate) enum Purpose {
    SaveShelf { lens: Lens, over: Option<usize>, order: Order },
    NewSticker(PathBuf),
    RenameShelf(usize),
    RenameSticker(u32),
}

pub(crate) struct NameEntry {
    kb: Keyboard,
    title: String,
    purpose: Purpose,
    /// The page to go back to (the Lens), if any.
    back: Option<Box<Page>>,
}

/// A Curator page over the shelf or the card.
pub(crate) enum Page {
    Lens(LensPage),
    Echoes(EchoesPage),
    Versions(VersionsPage),
    /// The stickers of a game, over its card: the case stickers first (`case_stickers::Kind::ALL`, as `case` has them,
    /// Game of the year with `year`), then the player's own (`ids` line up with those rows), "New sticker…" last.
    Stickers { sheet: Sheet, path: PathBuf, ids: Vec<u32>, case: CaseStickers, year: u16 },
    /// A smart shelf's or sticker's options, over the Curator list.
    Options { sheet: Sheet, item: Item, acts: Vec<Act> },
    /// The order of the games: over the Lens (`back`, which takes the choice), or over the Curator list for smart shelf
    /// `shelf` (saved at once).
    Order { sheet: Sheet, back: Option<Box<Page>>, shelf: Option<usize> },
    Name(Box<NameEntry>),
    /// Curator → History (`app_history`).
    History(Box<pocket_ui::history_page::HistoryPage>),
    /// The Year or Month in review (`app_history`).
    Review(Box<pocket_ui::history_page::ReviewPage>),
}

/// What the App keeps for Curator.
#[derive(Default)]
pub(crate) struct CuratorUi {
    pub prefs: Prefs,
    pub index: Option<Arc<Index>>,
    /// Bumped when the library is read again: the catalog is made anew.
    library_gen: u64,
    /// The library generation of the catalog in `index`.
    built_gen: Option<u64>,
    building: bool,
    /// The player changed something the index shows: a new index when the thread is free.
    dirty: bool,
    pub page: Option<Page>,
    /// The Lens as last left, for this session, and the order its games were shown in.
    lens: Lens,
    order: Order,
    /// "Surprise me": the generator (seeded from the clock at the first pick) and the games shown since the Curator
    /// list's entry was last pressed, so "Another" brings a new one.
    rng: Option<Rng>,
    surprise_seen: Vec<usize>,
    /// Where B goes from a list opened in the Curator list.
    back: Vec<(Screen, Item)>,
    /// Games on each smart shelf and sticker, for the rows' counts.
    shelf_counts: Vec<usize>,
    sticker_counts: HashMap<u32, usize>,
    /// The first games of the smart shelves and stickers, in their order: their rows' covers on Home and in Curator's list
    /// (`app_home_panel`).
    pub shelf_tops: HashMap<usize, Vec<usize>>,
    pub sticker_tops: HashMap<u32, Vec<usize>>,
    /// What Home's Curator row offers, from the index thread with `index` (`curator::today`).
    pub today: Today,
    /// Surprise me showed the day's pick already this session; it is the first pick until then.
    today_shown: bool,
    /// Settings → Curator's selected row.
    pub settings_sel: usize,
    /// The game running at the last tick (not an app) and when it started: its end is Up next's cue.
    seen_game: Option<(PathBuf, Instant)>,
    /// A game ended: the next index build works out Up next for it.
    up_next_seed: Option<PathBuf>,
    /// Up next on the shelf, while it is shown.
    pub up_next: Option<UpNext>,
    /// Play history's pages and rows (`app_history`).
    pub history: super::app_history::HistoryUi,
    /// Connections' index and open list, and Rediscover's games (`app_connections`).
    pub connections: super::app_connections::ConnectionsUi,
}

/// "Up next: <game> — <reason>" on the shelf after a game (Roon Radio's continuation).
#[derive(Clone, Debug)]
pub(crate) struct UpNext {
    pub item: Item,
    pub name: String,
    pub system: String,
    pub reason: String,
    pub at: Instant,
}

impl CuratorUi {
    pub fn load(data_dir: &Path) -> CuratorUi {
        CuratorUi { prefs: Prefs::load(data_dir), ..Default::default() }
    }

    /// The player changed a status, a star, a sticker or a preferred version, or played: the index follows.
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    /// The library was read again: a new catalog.
    pub fn library_changed(&mut self) {
        self.library_gen += 1;
    }
}

/// Where layer B comes from: the shipped `assets/curatordb.bin` (the hand-made fixture is for tests only).
fn feel_source() -> Box<dyn curator::FeelSource + Send> {
    Box::new(curator::CuratorDbFeel)
}

/// Settings → Curator's rows: the switch, its name, what it does.
pub(crate) const SETTINGS_ROWS: [(&str, &str); 8] = [
    ("Lens & smart shelves", "The Curator row on Home: combine filters, and keep a filter as a smart shelf that updates itself."),
    ("Echoes on the game card", "More like this: games you own that play alike, each with a short reason."),
    ("Up next after a game", "When you quit a game you played for a while, one line on the shelf suggests a game of yours like it. A plays it; any other button puts it away. Nothing starts by itself."),
    ("Merge versions in mixed lists", "A game you own on several systems shows once in Favourites, smart shelves and Echoes, and starts the version you prefer."),
    ("Stickers", "Your own labels for games, such as Couch co-op. Put them on from the game card; pin one to see it on Home."),
    ("Games not on this device in Echoes", "Echoes also names a few games you don't have, from the offline game list. Names only."),
    // Its line over the game is Settings > Playing > Over the game since 2026-10-25 (`over_game`).
    ("Pixel Path in the pause menu", "How a game's picture and sound reach you, stage by stage, graded in colour, in the pause menu of every game. The line beside the picture is under Settings › Playing › Over the game."),
    ("Play history", "Curator › History: what you played this week, month and year, and a Year in review. Kept on this Pocket only. Off: nothing new is recorded, and what was recorded stays."),
];

pub(crate) fn switch(p: &mut Prefs, row: usize) -> &mut bool {
    match row {
        0 => &mut p.lens,
        1 => &mut p.echoes,
        2 => &mut p.up_next,
        3 => &mut p.merge_versions,
        4 => &mut p.stickers,
        5 => &mut p.not_on_device,
        6 => &mut p.pixel_path,
        _ => &mut p.history,
    }
}

impl App {
    // ---- the index ----------------------------------------------------------------------------------------

    /// Called as the loop goes idle: starts a build when the library or the player's state changed. Never before
    /// the first frame, never while a game runs or the first-start setup is open.
    pub(crate) fn curator_tick(&mut self) {
        self.curator_watch_game();
        // Play history's writer and migration (`app_history`), which also run while a game does.
        self.history_tick();
        let c = &self.curator;
        if c.building || self.running.is_some() || !self.drew_first || self.setup.is_some() {
            return;
        }
        let need_catalog = c.built_gen != Some(c.library_gen);
        if need_catalog || c.dirty {
            self.curator_build(need_catalog, false);
        }
    }

    /// Gathers what the thread needs (paths and names, a copy of library.json's games) and builds the catalog (when
    /// `catalog`) and the index on it; `now`: right here, for snapshots.
    pub(crate) fn curator_build(&mut self, catalog: bool, now: bool) {
        let old = self.curator.index.as_ref().map(|ix| ix.catalog.clone()).filter(|_| !catalog);
        let games: Vec<LibraryGame> = if old.is_some() {
            Vec::new()
        } else {
            self.systems
                .iter()
                .enumerate()
                .flat_map(|(s, sys)| {
                    sys.games.iter().enumerate().map(move |(g, game)| LibraryGame { at: (s, g), system: sys.name.clone(), path: game.path.clone(), name: game.name.clone(), choice: None, identified: None })
                })
                .collect()
        };
        let systems: Vec<SystemName> = self.systems.iter().map(|s| SystemName { id: s.name.clone(), name: s.fullname.clone(), short: system_short(&self.systems, &s.name) }).collect();
        let player = Player {
            meta: self.library.games(),
            stickers: self.library.stickers().to_vec(),
            preferred: self.library.preferred().clone(),
            favorites: self.favorites.list.iter().map(|f| f.path.clone()).collect(),
            recents: self.recents.list.iter().map(|r| (r.path.clone(), (r.last_played, r.play_secs))).collect(),
            now: pocket_ui::recents::now_secs(),
        };
        let gen = self.curator.library_gen;
        self.curator.building = true;
        self.curator.dirty = false;
        // Up next for the game that just ended: one Echoes list on the thread, with the index that knows the session.
        let seed = self.curator.up_next_seed.take();
        let data_dir = self.data_dir.clone();
        // Connections and Rediscover take what the EmulationStation gamelists and the play history add (`app_connections`).
        let (gamelist, log) = (self.gamelist.index.clone(), pocket_ui::play_log::view());
        let connections_stale = std::mem::take(&mut self.curator.connections.stale);
        let tz = pocket_ui::appearance::utc_offset_now(player.now as i64);
        let job = move || {
            let t = Instant::now();
            let make_connections = old.is_none() || connections_stale;
            let catalog = match old {
                Some(c) => c,
                None => {
                    let mut games = games;
                    // The names the games' checksums gave (`rom_id`), from the card: nothing is read through here.
                    let pairs: Vec<(String, PathBuf)> = games.iter().map(|g| (g.system.clone(), g.path.clone())).collect();
                    let mut names = pocket_ui::rom_id::known(&data_dir, &pairs);
                    for g in &mut games {
                        g.choice = player.meta.get(&g.path.to_string_lossy().into_owned()).and_then(|m| m.choice.clone());
                        g.identified = names.remove(&g.path);
                    }
                    let mut db = pocket_ui::gamedb::GameDb::embedded();
                    Arc::new(curator::build_catalog(&games, systems, &mut db, feel_source().as_ref()))
                }
            };
            // Connections' index with a new catalog or new gamelists: the same games, their developers, publishers, series, genres.
            let connections = make_connections.then(|| Arc::new(pocket_ui::connections::Index::build(super::app_connections::entries(&catalog, &gamelist))));
            let catalog_ms = t.elapsed().as_millis();
            let t = Instant::now();
            let rediscover = super::app_connections::rediscover_input(&catalog, &player, &gamelist, log.as_deref(), player.now, tz);
            let user = curator::user_state(&catalog, &player);
            let index = Arc::new(Index::build(catalog, user));
            let index_ms = t.elapsed().as_millis();
            let up_next = seed.and_then(|p| index.find(&p)).and_then(|g| index.up_next(g)).map(|e| (e.game, e.reason));
            // Home's Curator row: one draw and one pass over the works, well under a millisecond (`curator::today`).
            let today = index.today();
            Event::Built { gen, index, catalog_ms, index_ms, up_next, today, connections, rediscover }
        };
        if now {
            let e = job();
            self.curator_event(e);
            return;
        }
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            // Behind everything the shelf does: the little cores' spare time is enough.
            #[cfg(target_os = "linux")]
            // SAFETY: setpriority on this thread's own id only changes its scheduling.
            unsafe {
                let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
                libc::setpriority(libc::PRIO_PROCESS, tid, 10);
            }
            let _ = proxy.send_event(UserEvent::Curator(job()));
        });
    }

    pub(crate) fn curator_event(&mut self, e: Event) {
        match e {
            Event::Built { gen, index, catalog_ms, index_ms, up_next, today, connections, rediscover } => {
                self.curator.building = false;
                println!("curator: {} games, catalog {catalog_ms} ms, index {index_ms} ms (library {gen})", index.len());
                let new_catalog = self.curator.index.as_ref().is_none_or(|old| !Arc::ptr_eq(&old.catalog, &index.catalog));
                if gen != self.curator.library_gen {
                    // The library changed while it was built: the next tick makes another.
                    return;
                }
                if new_catalog && matches!(self.curator.page, Some(Page::Echoes(_) | Page::Versions(_))) {
                    // Their numbers are the old catalog's.
                    self.curator.page = None;
                }
                if new_catalog {
                    // So are the games "Surprise me" showed.
                    self.curator.surprise_seen.clear();
                }
                self.curator.built_gen = Some(gen);
                self.curator.index = Some(index);
                if let Some(c) = connections {
                    self.curator.connections.index = c;
                    self.refresh_card_connections();
                }
                self.curator.connections.rediscover = rediscover;
                // A new day's pick (or another library's) is shown first again.
                if today.pick.as_ref().map(|p| p.game) != self.curator.today.pick.as_ref().map(|p| p.game) {
                    self.curator.today_shown = false;
                }
                self.curator.today = today;
                self.curator_counts();
                if let Some(Page::Lens(p)) = self.curator.page.as_mut() {
                    if let Some(ix) = self.curator.index.as_ref() {
                        p.clamp(ix);
                    }
                }
                // An open card shows its rows from the new index ("Getting ready…" turns into Echoes).
                self.curator_card_refresh();
                // And an open Credits & links its games by the console's makers (`app_console`).
                self.credits_refresh();
                let item = self.items.get(self.sel).copied();
                self.rebuild();
                // Home's rows may have moved by one (a count, merged Favorites): stay on the same entry.
                if let Some(at) = item.and_then(|it| self.items.iter().position(|i| *i == it)) {
                    self.sel = at;
                    self.prev_sel = at;
                }
                // The cabinet's Curator shelves follow the counts and pins.
                self.cabinet.dirty = true;
                if let Some((game, reason)) = up_next {
                    self.offer_up_next(game, reason);
                }
            }
            Event::History(c) => self.history_counted(c),
            Event::StickerSaveFailed(e) => self.curator_saved::<()>(Err(e), ""),
            Event::Elsewhere { seed, names } => {
                if let Some(Page::Echoes(p)) = self.curator.page.as_mut() {
                    if p.seed == seed {
                        p.elsewhere = names;
                    }
                }
            }
        }
    }

    /// The games on each smart shelf and sticker, for their rows.
    pub(crate) fn curator_counts(&mut self) {
        let Some(ix) = self.curator.index.clone() else { return };
        let merge = self.curator.prefs.merge_versions;
        self.curator.shelf_counts = self.curator.prefs.shelves.iter().map(|s| ix.list(&ix.matches(&s.lens), merge).len()).collect();
        self.curator.sticker_counts = self.library.stickers().iter().map(|st| (st.id, ix.list(&ix.matches(&sticker_lens(st.id)), merge).len())).collect();
        // The covers of the rows on Home and in Curator's list: every shelf and sticker shows its first games (the
        // counts above matched them all already).
        let top = |lens: &Lens, order: Order| ix.sorted(&ix.matches(lens), merge, order).into_iter().take(pocket_ui::home_panel::MAX_COVERS).collect::<Vec<_>>();
        self.curator.shelf_tops = self.curator.prefs.shelves.iter().enumerate().map(|(i, s)| (i, top(&s.lens, s.order))).collect();
        self.curator.sticker_tops = self.library.stickers().iter().map(|st| (st.id, top(&sticker_lens(st.id), Order::Name))).collect();
    }

    /// The App's item for a game of the catalog, while the library still has that file there.
    pub(crate) fn curator_item(&self, game: usize) -> Option<Item> {
        let f = self.curator.index.as_ref()?.catalog.games.get(game)?;
        let (s, g) = f.at;
        (self.systems.get(s)?.games.get(g)?.path == f.path).then_some(Item::Game(s, g))
    }

    /// Favorites (and other lists the player made) with each game once, when Settings → Curator says so.
    pub(crate) fn curator_merge(&self, items: Vec<Item>) -> Vec<Item> {
        let Some(ix) = self.curator.index.as_ref().filter(|_| {
            self.curator.prefs.merge_versions && self.curator.built_gen == Some(self.curator.library_gen)
        }) else { return items };
        let games: Vec<usize> = items.iter().filter_map(|it| self.item_rom(*it)).filter_map(|(_, p)| ix.find(&p)).collect();
        if games.len() != items.len() {
            return items;
        }
        ix.merge_versions(&games).into_iter().filter_map(|g| self.curator_item(g)).collect()
    }

    // ---- the Cabinet look ------------------------------------------------------------------------------------------

    /// Curator's shelves in the cabinet (`app_cabinet`), top to bottom: its own boxes (the Lens, every smart shelf and
    /// sticker, as Home's Curator row lists them), the list open from there if it isn't a shelf of its own (the
    /// Lens's games, an unpinned smart shelf or sticker), then the pinned smart shelves and stickers, as on Home.
    /// Only keys: cheap enough to compare every frame, so the shelves are made again when one comes or goes.
    pub(crate) fn cabinet_curator_keys(&self) -> Vec<ShelfKey> {
        let p = &self.curator.prefs;
        let mut keys = Vec::new();
        // The Curator shelf holds History and the review too, once something was played (`app_history`).
        if p.lens || self.history_shown() {
            keys.push(ShelfKey::Curator);
        }
        let mut pinned: Vec<ShelfKey> = Vec::new();
        if p.lens {
            pinned.extend(p.shelves.iter().enumerate().filter(|(_, s)| s.pinned).map(|(i, _)| ShelfKey::SmartShelf(i)));
        }
        if p.stickers {
            pinned.extend(self.library.stickers().iter().filter(|s| s.pinned).map(|s| ShelfKey::Sticker(s.id)));
        }
        if let Some(open) = screen_shelf(self.screen).filter(|k| *k != ShelfKey::Curator && !pinned.contains(k)) {
            keys.push(open);
        }
        keys.extend(pinned);
        keys
    }

    /// What stands on one of Curator's cabinet shelves.
    pub(crate) fn cabinet_curator_items(&self, key: ShelfKey) -> Vec<Item> {
        match key {
            ShelfKey::Curator => self.curator_hub_items(),
            ShelfKey::Lens => self.curator_items(Screen::LensResults),
            ShelfKey::SmartShelf(i) => self.curator_items(Screen::Shelf(i)),
            ShelfKey::Sticker(id) => self.curator_items(Screen::Sticker(id)),
            ShelfKey::Connection => self.curator_items(Screen::Connection),
            ShelfKey::Rediscover => self.curator_items(Screen::Rediscover),
            _ => Vec::new(),
        }
    }

    /// The name on a Curator shelf's plate, and whether it is a sticker's (the plate is a sticker then).
    pub(crate) fn cabinet_curator_plate(&self, key: ShelfKey) -> Option<(String, bool)> {
        match key {
            ShelfKey::Curator => Some(("Curator".into(), false)),
            ShelfKey::Lens => Some((self.curator_screen_title(Screen::LensResults)?, false)),
            ShelfKey::SmartShelf(i) => Some((self.curator.prefs.shelves.get(i)?.name.clone(), false)),
            ShelfKey::Sticker(id) => Some((self.library.stickers().iter().find(|s| s.id == id)?.name.clone(), true)),
            ShelfKey::Connection | ShelfKey::Rediscover => Some((self.connection_title(shelf_screen(key)?)?, false)),
            _ => None,
        }
    }

    /// Whether B on this screen goes back along the way it came (the Lens for its games, the Curator list).
    pub(crate) fn curator_has_back(&self) -> bool {
        // A card's list of games goes back to the card (`app_connections`).
        matches!(self.screen, Screen::LensResults | Screen::Connection) || (screen_shelf(self.screen).is_some() && !self.curator.back.is_empty())
    }

    /// The cursor moved to another shelf in the cabinet: B no longer goes back along the way it came.
    pub(crate) fn curator_left(&mut self) {
        self.curator.back.clear();
    }

    /// In the cabinet a list with no games has no shelf to show: it stays closed, with a word.
    fn cabinet_refuses_empty(&mut self, screen: Screen) -> bool {
        if self.theme != pocket_ui::settings::Theme::Cabinet || screen == Screen::Curator || !self.curator_items(screen).is_empty() {
            return false;
        }
        let msg = if self.curator.index.is_none() { "Getting ready…" } else { "No games here right now" };
        self.toast = Some((msg.into(), Instant::now()));
        true
    }

    // ---- Up next ------------------------------------------------------------------------------------------------

    /// Watches the running game (each tick): when a game that ran for a while is gone and the shelf is back, the
    /// next index build works out Up next for it. A game started again (options, a fallback) or handed over to the
    /// next one (`app_fast_switch`) never leaves `running` empty, so it gets none; an app (PC streaming, a tool)
    /// neither. Nothing at all when Settings → Curator says no.
    fn curator_watch_game(&mut self) {
        match self.running.as_ref() {
            Some(r) if !r.is_app() => {
                if self.curator.seen_game.as_ref().is_none_or(|(p, _)| *p != r.path) {
                    self.curator.seen_game = Some((r.path.clone(), r.started));
                }
            }
            Some(_) => self.curator.seen_game = None,
            None => {
                if let Some((path, started)) = self.curator.seen_game.take() {
                    // A game that ended with a word on the shelf (it failed) gets none.
                    if self.curator.prefs.up_next && started.elapsed() >= UP_NEXT_AFTER && self.notice.is_none() {
                        self.curator.up_next_seed = Some(path);
                        self.curator.touch();
                    }
                }
            }
        }
    }

    /// Whether the shelf is what is on screen, with nothing over it: the only place Up next shows.
    fn up_next_room(&self) -> bool {
        self.running.is_none()
            && self.launch_at.is_none()
            && self.card.is_none()
            && self.curator.page.is_none()
            && self.settings_page.is_none()
            && self.setup.is_none()
            && self.menu.is_none()
            && self.switcher.is_none()
            && self.console.is_none()
            && self.port_files.is_none()
            && !self.find.is_open()
            && self.notice.is_none()
            && !self.sys.is_open()
    }

    /// The index thread found a game to suggest: shown when the shelf is free (and the game still where it was).
    fn offer_up_next(&mut self, game: usize, reason: String) {
        let Some(item @ Item::Game(s, g)) = self.curator_item(game) else { return };
        if !self.curator.prefs.up_next || !self.up_next_room() {
            return;
        }
        let (name, system) = (self.systems[s].games[g].name.clone(), self.systems[s].name.clone());
        println!("curator: up next {name} ({system}): {reason}");
        self.curator.up_next = Some(UpNext { item, name, system, reason, at: Instant::now() });
        self.request_redraw();
    }

    /// Up next while it is on screen (shown, not timed out, the shelf free).
    pub(crate) fn up_next_shown(&self) -> Option<&UpNext> {
        self.curator.up_next.as_ref().filter(|u| u.at.elapsed() < UP_NEXT_SHOWN && self.up_next_room())
    }

    /// A button on the shelf while Up next shows: A plays it, B puts it away; anything else puts it away and goes on
    /// as usual (true: the press is used up).
    pub(crate) fn curator_up_next_press(&mut self, b: Button) -> bool {
        let Some(up) = self.up_next_shown().cloned() else {
            self.curator.up_next = None;
            return false;
        };
        self.curator.up_next = None;
        if b == CONFIRM {
            if let Item::Game(s, g) = up.item {
                if let Some(game) = self.systems.get(s).and_then(|sys| sys.games.get(g)) {
                    let (system, name, path) = (self.systems[s].name.clone(), game.name.clone(), game.path.clone());
                    self.launch(system, name, path);
                }
            }
            return true;
        }
        b == CANCEL
    }

    /// The loop's next wake-up for Up next: a frame per refresh while it slides in, then when it goes by itself
    /// (it is put away here once its time is up). Nothing while there is none.
    pub(crate) fn curator_up_next_wake(&mut self, now: Instant) -> Option<Instant> {
        let up = self.curator.up_next.as_ref()?;
        let end = up.at + UP_NEXT_SHOWN;
        if now >= end || !self.up_next_room() {
            self.curator.up_next = None;
            self.request_redraw();
            return None;
        }
        if pocket_ui::motion::appearing(up.at, now) {
            // One frame per refresh, as `animating()` paces the others.
            let due = self.frame_at + super::FRAME;
            if now < due {
                return Some(due);
            }
            self.request_redraw();
            return Some(now + super::FRAME);
        }
        Some(end)
    }

    /// Up next over the bottom of the shelf, in every look: a paper label like the Shelf look's selected game, with
    /// its box, the game and the reason; the hint row says what A and B do now. False when it isn't showing (the
    /// shelf's own hints then).
    pub(crate) fn paint_up_next(&mut self, c: &mut Canvas) -> bool {
        let Some(up) = self.up_next_shown().cloned() else { return false };
        // Three lines: taller by their growth with Settings → Text size.
        let (g1, g2, g3) = (pocket_ui::type_scale::grow(21), pocket_ui::type_scale::grow(37), pocket_ui::type_scale::grow(28));
        let (x, w, h) = (MARGIN, c.w - 2 * MARGIN, 132 + (g1 + g2 + g3) * 6 / 5);
        let y = c.h - 112 - h;
        let cover = self.cover(up.item, 96, h - 24);
        let colour = stripe(&up.system);
        let short = system_short(&self.systems, &up.system);
        // The shelf fades out under it, so no row is cut off at its edge.
        const FADE: i32 = 40;
        for k in 0..FADE {
            c.shade_rect(0, y - FADE + k, c.w, 1, pal().void, (k * 255 / FADE) as u8);
        }
        c.rect(0, y, c.w, c.h - y, pal().void);
        pocket_ui::text::boxes::painted_over(c, (0, y, c.w, c.h - y));
        let text = &mut self.text;
        pocket_ui::motion::paint_toast(c, up.at, Instant::now(), None, (x, y - 24, w, h + 48), 20.0, |c, dy| {
            let y = y + dy;
            c.rect(x, y, w, h, pal().label);
            game_card::lift_shadow(c, x, y + h, w, pal().t.max(120));
            c.rect(x, y, STRIPE, h, colour);
            let mut tx = x + STRIPE + 24;
            match &cover {
                Some(img) => {
                    let (cw, ch) = (img.w as i32, img.h as i32);
                    c.blit_scaled(img, tx + (96 - cw) / 2, y + (h - ch) / 2, cw, ch);
                }
                None => c.round_rect(tx + 12, y + 16, 72, h - 32, 6, mix(colour, pal().label, 170)),
            }
            tx += 96 + 24;
            let tw = x + w - 28 - tx;
            text.draw(c, Face::Body, 21, tx, y + 36 + g1, pal().label_dust, "Up next");
            let name = text.fit(Face::Title, 37, &up.name, tw);
            text.draw(c, Face::Title, 37, tx, y + 80 + g1 + g2, pal().ink, &name);
            let line = text.fit(Face::Body, 28, &format!("{short} · {}", up.reason), tw);
            text.draw(c, Face::Body, 28, tx, y + 116 + g1 + g2 + g3, pal().label_dust, &line);
        });
        // A tap on it plays it, as A does.
        self.hits.button(Area::new(x, y, w, h), CONFIRM);
        self.paint_hint_row(c, &[("A", "Play it"), ("B", "Not now")]);
        true
    }

    // ---- Home, the Curator list and its lists ----------------------------------------------------------------

    /// Home's Curator rows: the Curator row, then pinned shelves and stickers.
    pub(crate) fn curator_home_rows(&self) -> Vec<Item> {
        let p = &self.curator.prefs;
        let mut rows = Vec::new();
        if p.lens {
            rows.push(Item::Curator);
            rows.extend(p.shelves.iter().enumerate().filter(|(_, s)| s.pinned).map(|(i, _)| Item::SmartShelf(i)));
        } else if self.history_on_home() {
            // Without the Lens there is no Curator row: History stands on Home itself, once something was played.
            rows.push(Item::History);
        }
        if p.stickers {
            rows.extend(self.library.stickers().iter().filter(|s| s.pinned).map(|s| Item::Sticker(s.id)));
        }
        rows
    }

    /// The Curator list: the Lens, the smart shelves, History and the Year in review, the stickers.
    fn curator_hub_items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        if self.curator.prefs.lens {
            // "Surprise me" right under the Lens: Curator's list is where finding something to play lives, so it adds
            // no row to Home and is two presses from it (docs/CURATOR.md "Surprise me").
            items.extend([Item::Lens, Item::Surprise]);
            items.extend((0..self.curator.prefs.shelves.len()).map(Item::SmartShelf));
        }
        // Rediscover while there is something in it: a year ago today, or not played for a year; not with its Cozy
        // extra off.
        if pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::Rediscover) && self.rediscover_counts().0 > 0 {
            items.push(Item::Rediscover);
        }
        if self.history_shown() {
            items.extend([Item::History, Item::Review(self.review_year())]);
        }
        if self.curator.prefs.stickers {
            items.extend(self.library.stickers().iter().map(|s| Item::Sticker(s.id)));
        }
        items
    }

    /// The items of a Curator screen.
    pub(crate) fn curator_items(&self, screen: Screen) -> Vec<Item> {
        if screen == Screen::Curator {
            return self.curator_hub_items();
        }
        if matches!(screen, Screen::Connection | Screen::Rediscover) {
            return self.connection_items(screen);
        }
        let Some(ix) = self.curator.index.as_ref() else { return Vec::new() };
        let lens = match screen {
            Screen::Shelf(i) => self.curator.prefs.shelves.get(i).map(|s| (s.lens.clone(), s.order)),
            Screen::Sticker(id) => Some((sticker_lens(id), Order::Name)),
            Screen::LensResults => Some((self.curator.lens.clone(), self.curator.order)),
            _ => None,
        };
        let Some((lens, order)) = lens else { return Vec::new() };
        ix.sorted(&ix.matches(&lens), self.curator.prefs.merge_versions, order).into_iter().filter_map(|g| self.curator_item(g)).collect()
    }

    pub(crate) fn curator_screen_title(&self, screen: Screen) -> Option<String> {
        let (title, order) = match screen {
            Screen::Curator => ("Curator".to_string(), Order::Name),
            Screen::Shelf(i) => {
                let s = self.curator.prefs.shelves.get(i)?;
                (s.name.clone(), s.order)
            }
            Screen::Sticker(id) => (self.library.stickers().iter().find(|s| s.id == id)?.name.clone(), Order::Name),
            Screen::Connection | Screen::Rediscover => return self.connection_title(screen),
            Screen::LensResults => match self.curator.index.as_ref() {
                Some(ix) if !self.curator.lens.is_empty() => (format!("Lens: {}", ix.suggest_name(&self.curator.lens)), self.curator.order),
                _ => ("Lens: all games".to_string(), self.curator.order),
            },
            _ => return None,
        };
        // A list in another order than by name says so after its name: "Short RPGs · last played first".
        Some(match order.suffix() {
            Some(how) => format!("{title} · {how}"),
            None => title,
        })
    }

    /// What a Curator screen with nothing on it says.
    pub(crate) fn curator_empty_text(&self) -> Option<(&'static str, &'static str)> {
        if !matches!(self.screen, Screen::Curator | Screen::Shelf(_) | Screen::Sticker(_) | Screen::LensResults | Screen::Connection | Screen::Rediscover) {
            return None;
        }
        // The games went (deleted, the card taken out) while the list was open.
        if matches!(self.screen, Screen::Connection | Screen::Rediscover) {
            return Some(("No games here", "These games aren't on this Pocket any more."));
        }
        Some(match self.curator.index {
            None => ("Getting ready…", "Curator is reading your library. This takes a moment once."),
            Some(_) if matches!(self.screen, Screen::Sticker(_)) => ("No games with this sticker", "Put it on a game from its card: X, then Stickers."),
            Some(_) => ("No games here", "Nothing in your library matches these filters right now."),
        })
    }

    /// (title, stripe id, text at the spine's end, meta line) of a Curator item.
    pub(crate) fn curator_describe(&self, item: Item) -> (String, String, String, String) {
        let ready = self.curator.index.is_some();
        let games = |n: usize| if n == 1 { "1 game".to_string() } else { format!("{n} games") };
        match item {
            Item::Curator => {
                let (shelves, stickers) = (self.curator.prefs.shelves.len(), if self.curator.prefs.stickers { self.library.stickers().len() } else { 0 });
                let today = self.curator_today();
                let side = if !ready {
                    "Getting ready…".to_string()
                } else if let Some((pick, _)) = today.as_ref().and_then(|t| t.pick.as_ref()) {
                    // The day's pick at the spine's end: Curator says what it has for you before it is opened.
                    format!("Today: {}", pick.title)
                } else {
                    let mut parts = Vec::new();
                    if shelves > 0 {
                        parts.push(if shelves == 1 { "1 shelf".to_string() } else { format!("{shelves} shelves") });
                    }
                    if stickers > 0 {
                        parts.push(if stickers == 1 { "1 sticker".to_string() } else { format!("{stickers} stickers") });
                    }
                    if parts.is_empty() { "Lens".to_string() } else { parts.join(" · ") }
                };
                // The pick and why (`home_panel::curator`); "Getting ready…" before the index.
                let meta = match today {
                    Some(t) if t.pick.is_some() => pocket_ui::home_panel::curator(Some(t)).line,
                    Some(_) if self.history_shown() => "Lens, smart shelves, stickers and your play history".into(),
                    Some(_) => "Lens, smart shelves and stickers".into(),
                    None => "Getting ready…".into(),
                };
                ("Curator".into(), CURATOR_STRIPE.into(), side, meta)
            }
            Item::Lens => {
                let side = self.curator.index.as_ref().map_or("Getting ready…".to_string(), |ix| {
                    if self.curator.lens.is_empty() {
                        // Nothing picked: what it looks through, not a blank (Split's panel showed an empty slate).
                        let all = ix.count(&ix.matches(&self.curator.lens), self.curator.prefs.merge_versions);
                        pocket_ui::curator::lens::overview(all, ix.shown_facets().len())
                    } else {
                        games(ix.list(&ix.matches(&self.curator.lens), self.curator.prefs.merge_versions).len())
                    }
                });
                ("Lens".into(), CURATOR_STRIPE.into(), side, "Combine filters; the count follows as you pick".into())
            }
            Item::Surprise => {
                // Never a blank where Split shows the figure: what it picks from.
                let side = self.curator.index.as_ref().map_or("Getting ready…".to_string(), |ix| match ix.catalog.works.len() {
                    0 => "No games yet".to_string(),
                    1 => "From 1 game".to_string(),
                    n => format!("From {n} games"),
                });
                // The day's pick, which A shows first (`curator_surprise`).
                let pick = self.curator_today().and_then(|t| t.pick);
                let meta = pocket_ui::home_panel::surprise(pick, "One game picked for you, and why. Right picks another".into()).line;
                ("Surprise me".into(), CURATOR_STRIPE.into(), side, meta)
            }
            Item::SmartShelf(i) => {
                let Some(s) = self.curator.prefs.shelves.get(i) else { return Default::default() };
                let side = if ready { games(self.curator.shelf_counts.get(i).copied().unwrap_or(0)) } else { "Getting ready…".into() };
                let meta = if s.pinned { "Smart shelf, on Home. It updates itself" } else { "Smart shelf. It updates itself" };
                // Its order, when it isn't by name: "… · most played first".
                let meta = match s.order.suffix() {
                    Some(how) => format!("{meta} · {how}"),
                    None => meta.to_string(),
                };
                (s.name.clone(), CURATOR_STRIPE.into(), side, meta)
            }
            Item::Sticker(id) => {
                let Some(s) = self.library.stickers().iter().find(|s| s.id == id) else { return Default::default() };
                let side = if ready { games(self.curator.sticker_counts.get(&id).copied().unwrap_or(0)) } else { "Getting ready…".into() };
                let meta = if s.pinned { "Sticker, on Home" } else { "Sticker" };
                (s.name.clone(), STICKER_STRIPE.into(), side, meta.into())
            }
            Item::History | Item::Review(_) => self.history_describe(item),
            Item::Rediscover => {
                let (all, today) = self.rediscover_counts();
                let side = if all == 1 { "1 game".to_string() } else { format!("{all} games") };
                // Short: Split's panel and the Cards look show it on one line.
                let meta = match today {
                    0 => "Not played for a year".to_string(),
                    n => format!("{n} played a year ago today"),
                };
                ("Rediscover".into(), CURATOR_STRIPE.into(), side, meta)
            }
            _ => Default::default(),
        }
    }

    /// A on a Curator item.
    pub(crate) fn curator_activate(&mut self, item: Item) {
        let screen = match item {
            Item::Curator => Screen::Curator,
            Item::Lens => return self.open_lens(self.curator.lens.clone(), None),
            Item::Surprise => {
                self.curator.surprise_seen.clear();
                return self.curator_surprise();
            }
            Item::History | Item::Review(_) => return self.history_activate(item),
            Item::SmartShelf(i) => Screen::Shelf(i),
            Item::Sticker(id) => Screen::Sticker(id),
            Item::Rediscover => Screen::Rediscover,
            _ => return,
        };
        if self.cabinet_refuses_empty(screen) {
            return;
        }
        self.cabinet.dirty = true;
        if self.screen == Screen::Home {
            self.home_item = Some(item);
            self.curator.back.clear();
        } else {
            self.curator.back.push((self.screen, item));
        }
        self.screen = screen;
        self.sel = 0;
        self.rebuild();
    }

    /// B on a Curator screen: back to the list it was opened from (the Lens for its results). False: not ours,
    /// the shelf goes Home as always.
    pub(crate) fn curator_back(&mut self) -> bool {
        if self.screen == Screen::LensResults {
            let lens = self.curator.lens.clone();
            self.back_to_curator_parent();
            self.open_lens(lens, None);
            return true;
        }
        if !matches!(self.screen, Screen::Shelf(_) | Screen::Sticker(_) | Screen::Curator | Screen::Rediscover) {
            return false;
        }
        self.back_to_curator_parent()
    }

    fn back_to_curator_parent(&mut self) -> bool {
        let Some((screen, item)) = self.curator.back.pop() else { return false };
        self.screen = screen;
        self.rebuild();
        self.sel = self.items.iter().position(|i| *i == item).unwrap_or(0);
        self.prev_sel = self.sel;
        true
    }

    /// Y on a shelf or sticker: pin it to Home or take it off. False when the selection is something else.
    pub(crate) fn curator_y(&mut self) -> bool {
        match self.items.get(self.sel).copied() {
            Some(Item::SmartShelf(i)) => {
                let Some(s) = self.curator.prefs.shelves.get_mut(i) else { return true };
                s.pinned = !s.pinned;
                let msg = if s.pinned { "On Home" } else { "Off Home" };
                self.save_curator_prefs(msg);
                true
            }
            Some(Item::Sticker(id)) => {
                let pinned = !self.library.stickers().iter().any(|s| s.id == id && s.pinned);
                let saved = self.library.set_sticker_pinned(id, pinned);
                self.curator_saved(saved, if pinned { "On Home" } else { "Off Home" });
                self.rebuild();
                true
            }
            _ => false,
        }
    }

    /// Whether a shelf or sticker is pinned (the Y hint).
    pub(crate) fn curator_pinned(&self, item: Item) -> bool {
        match item {
            Item::SmartShelf(i) => self.curator.prefs.shelves.get(i).is_some_and(|s| s.pinned),
            Item::Sticker(id) => self.library.stickers().iter().any(|s| s.id == id && s.pinned),
            _ => false,
        }
    }

    /// X on a shelf or sticker: its options. False when the selection is something else.
    pub(crate) fn curator_x(&mut self) -> bool {
        let Some(item) = self.items.get(self.sel).copied().filter(|i| matches!(i, Item::SmartShelf(_) | Item::Sticker(_))) else { return false };
        let pinned = self.curator_pinned(item);
        let mut acts = Vec::new();
        if matches!(item, Item::SmartShelf(_)) {
            acts.extend([Act::Edit, Act::Order]);
        }
        acts.extend([Act::Rename, Act::Pin, Act::Delete]);
        let order = match item {
            Item::SmartShelf(i) => self.curator.prefs.shelves.get(i).map_or(Order::Name, |s| s.order),
            _ => Order::Name,
        };
        let rows = acts
            .iter()
            .map(|a| {
                let label = match a {
                    Act::Edit => "Change the filters".to_string(),
                    Act::Order => format!("Order: {}", order.label()),
                    Act::Rename => "Rename".to_string(),
                    Act::Pin if pinned => "Take off Home".to_string(),
                    Act::Pin => "Show on Home".to_string(),
                    Act::Delete => "Delete".to_string(),
                };
                (label, None)
            })
            .collect();
        let title = self.curator_describe(item).0;
        self.curator.page = Some(Page::Options { sheet: Sheet { title, rows, sel: 0, single: false, adjust: None }, item, acts });
        true
    }

    /// Deletes a sticker everywhere: off every game, out of the smart shelves that filtered on it and the Lens (also
    /// for the web companion, `app_web.rs`).
    pub(crate) fn curator_delete_sticker(&mut self, id: u32) -> std::io::Result<()> {
        let saved = self.library.delete_sticker(id);
        for s in &mut self.curator.prefs.shelves {
            s.lens.forget(Facet::Stickers, &id.to_string());
        }
        self.curator.lens.forget(Facet::Stickers, &id.to_string());
        self.curator.prefs.save(&self.data_dir);
        self.curator.touch();
        self.rebuild();
        saved
    }

    /// The smart shelves changed from the web companion (`app_web.rs`): saved, counted and shown, without a toast.
    pub(crate) fn curator_prefs_changed(&mut self) -> Result<(), String> {
        self.cabinet.dirty = true;
        let ok = self.curator.prefs.save(&self.data_dir);
        self.curator_counts();
        self.rebuild();
        if ok {
            Ok(())
        } else {
            Err("The Pocket could not save it.".into())
        }
    }

    fn save_curator_prefs(&mut self, msg: &str) {
        self.cabinet.dirty = true;
        let ok = self.curator.prefs.save(&self.data_dir);
        self.curator_saved(if ok { Ok(()) } else { Err(std::io::Error::other("not saved")) }, msg);
        self.curator_counts();
        self.rebuild();
    }

    fn curator_saved<T>(&mut self, saved: std::io::Result<T>, what: &str) {
        let msg = match saved {
            Ok(_) => what.to_string(),
            Err(e) => {
                eprintln!("curator: not saved: {e}");
                "Could not save: is the SD card full?".to_string()
            }
        };
        self.toast = Some((msg, Instant::now()));
    }

    /// No SD flush on a sticker key press or repeat; only failures need a reply on the UI thread.
    fn save_case_stickers(&mut self, path: &Path, case: CaseStickers) -> std::io::Result<()> {
        let proxy = self.proxy.clone();
        self.library.set_case_stickers_soon(path, case, move |saved| {
            if let Err(e) = saved {
                let _ = proxy.send_event(UserEvent::Curator(Event::StickerSaveFailed(e)));
            }
        })
    }

    // ---- pages ----------------------------------------------------------------------------------------------

    pub(crate) fn open_lens(&mut self, lens: Lens, shelf: Option<usize>) {
        let mut page = LensPage::new(lens, shelf);
        page.order = self.curator.order;
        if let Some(ix) = self.curator.index.as_ref() {
            page.clamp(ix);
        }
        self.curator.page = Some(Page::Lens(page));
    }

    /// Input while a Curator page is open.
    pub(crate) fn curator_input(&mut self, b: Button) {
        let Some(page) = self.curator.page.take() else { return };
        self.curator.page = match page {
            Page::Lens(mut p) => {
                let Some(ix) = self.curator.index.clone() else {
                    // Getting ready: only B does something.
                    return self.curator.page = if b == CANCEL { None } else { Some(Page::Lens(p)) };
                };
                match p.press(b, &ix) {
                    LensOut::None => Some(Page::Lens(p)),
                    LensOut::Back => {
                        self.curator.lens = p.lens;
                        // A shelf's order stays the shelf's; the Lens of this session keeps its own.
                        if p.shelf.is_none() {
                            self.curator.order = p.order;
                        }
                        None
                    }
                    LensOut::Order => {
                        let labels: Vec<&str> = Order::ALL.iter().map(|o| o.label()).collect();
                        let at = Order::ALL.iter().position(|o| *o == p.order).unwrap_or(0);
                        Some(Page::Order { sheet: Sheet::choice("Order", &labels, at), back: Some(Box::new(Page::Lens(p))), shelf: None })
                    }
                    LensOut::Show => {
                        self.curator.lens = p.lens.clone();
                        self.curator.order = p.order;
                        if self.cabinet_refuses_empty(Screen::LensResults) {
                            return self.curator.page = Some(Page::Lens(p));
                        }
                        self.cabinet.dirty = true;
                        self.curator.back.push((self.screen, self.items.get(self.sel).copied().unwrap_or(Item::Lens)));
                        self.screen = Screen::LensResults;
                        self.sel = 0;
                        self.rebuild();
                        None
                    }
                    LensOut::Save => {
                        let name = match p.shelf.and_then(|i| self.curator.prefs.shelves.get(i)) {
                            Some(s) => s.name.clone(),
                            None => ix.suggest_name(&p.lens),
                        };
                        let purpose = Purpose::SaveShelf { lens: p.lens.clone(), over: p.shelf, order: p.order };
                        let title = if p.shelf.is_some() { "Save the shelf" } else { "Name the shelf" };
                        Some(name_page(title, &name, purpose, Some(Page::Lens(p))))
                    }
                }
            }
            Page::Echoes(mut p) => match p.press(b) {
                ListOut::None | ListOut::Prefer(_) => Some(Page::Echoes(p)),
                ListOut::Back => None,
                ListOut::Play(i) => {
                    self.curator_play(p.echoes[i].game);
                    None
                }
                ListOut::Details(i) => {
                    self.curator_card(p.echoes[i].game);
                    None
                }
            },
            Page::Versions(mut p) => match p.press(b) {
                ListOut::None | ListOut::Details(_) => Some(Page::Versions(p)),
                ListOut::Back => None,
                ListOut::Play(i) => {
                    self.curator_play(p.rows[i].game);
                    None
                }
                ListOut::Prefer(i) => {
                    self.curator_prefer(p.rows[i].game);
                    p.prefer(i);
                    Some(Page::Versions(p))
                }
            },
            Page::Stickers { mut sheet, path, ids, mut case, mut year } => match sheet.press(b) {
                SheetOut::None => Some(Page::Stickers { sheet, path, ids, case, year }),
                SheetOut::Back => {
                    self.curator_card_refresh();
                    None
                }
                // A case sticker: on or off at once, the case on the shelf follows (`case_stickers`).
                SheetOut::Pick(i) if i < CaseKind::ALL.len() => {
                    let k = CaseKind::ALL[i];
                    case.set(k, !case.has(k), year);
                    let saved = self.save_case_stickers(&path, case);
                    sheet.rows[i].1 = Some(case.has(k));
                    let msg = if case.has(k) { format!("Sticker on: {}", case.name(k)) } else { format!("Sticker off: {}", case.name(k)) };
                    self.curator_saved(saved, &msg);
                    Some(Page::Stickers { sheet, path, ids, case, year })
                }
                // Left/Right on Game of the year: another year (kept at once when the sticker is on).
                SheetOut::Adjust(i, d) => {
                    year = (i32::from(year) + d).clamp(i32::from(case_stickers::YEAR_FIRST), i32::from(case_stickers::YEAR_LAST)) as u16;
                    sheet.rows[i].0 = format!("Game of the year {year}");
                    if case.goty.is_some() {
                        case.goty = Some(year);
                        let saved = self.save_case_stickers(&path, case);
                        if let Err(e) = saved {
                            self.curator_saved::<()>(Err(e), "");
                        }
                    }
                    Some(Page::Stickers { sheet, path, ids, case, year })
                }
                SheetOut::Pick(i) if i - CaseKind::ALL.len() < ids.len() => {
                    let at = i - CaseKind::ALL.len();
                    let saved = self.library.toggle_sticker(&path, ids[at]);
                    if let Ok(on) = saved {
                        sheet.rows[i].1 = Some(on);
                    }
                    self.curator_saved(saved.map(|_| ()), &format!("Sticker: {}", sheet.rows[i].0));
                    self.curator.touch();
                    Some(Page::Stickers { sheet, path, ids, case, year })
                }
                SheetOut::Pick(_) => {
                    let back = Page::Stickers { sheet, path: path.clone(), ids, case, year };
                    let name = self.suggest_sticker_name(&path);
                    Some(name_page("New sticker", &name, Purpose::NewSticker(path), Some(back)))
                }
            },
            Page::Options { mut sheet, item, acts } => match sheet.press(b) {
                SheetOut::None | SheetOut::Adjust(..) => Some(Page::Options { sheet, item, acts }),
                SheetOut::Back => None,
                SheetOut::Pick(i) => self.curator_option(item, acts[i]),
            },
            Page::Order { mut sheet, back, shelf } => match sheet.press(b) {
                SheetOut::None | SheetOut::Adjust(..) => Some(Page::Order { sheet, back, shelf }),
                SheetOut::Back => back.map(|b| *b),
                SheetOut::Pick(i) => {
                    let order = Order::ALL.get(i).copied().unwrap_or_default();
                    match (back.map(|b| *b), shelf) {
                        // Over the Lens: START shows the games in it, X saves it with the shelf.
                        (Some(Page::Lens(mut l)), _) => {
                            l.order = order;
                            Some(Page::Lens(l))
                        }
                        (_, Some(i)) => {
                            if let Some(s) = self.curator.prefs.shelves.get_mut(i) {
                                s.order = order;
                            }
                            self.save_curator_prefs(&format!("Order: {}", order.label()));
                            None
                        }
                        (other, None) => other,
                    }
                }
            },
            Page::History(p) => self.history_input(p, b),
            Page::Review(p) => self.review_input(p, b),
            Page::Name(mut n) => {
                let outcome = n.kb.press(b);
                let max = match n.purpose {
                    Purpose::SaveShelf { .. } | Purpose::RenameShelf(_) => curator::prefs::SHELF_NAME_MAX,
                    _ => pocket_ui::library_meta::STICKER_MAX,
                };
                if n.kb.text.chars().count() > max {
                    n.kb.text = n.kb.text.chars().take(max).collect();
                }
                match outcome {
                    Outcome::Editing => Some(Page::Name(n)),
                    Outcome::Cancelled => n.back.map(|b| *b),
                    Outcome::Done(text) => self.curator_named(*n, text.trim().to_string()),
                }
            }
        };
    }

    /// A name was typed: save what it was for, and go back.
    fn curator_named(&mut self, n: NameEntry, name: String) -> Option<Page> {
        match n.purpose {
            Purpose::SaveShelf { lens, over, order } => {
                let name = if name.is_empty() { "Shelf".to_string() } else { name };
                let i = match over.filter(|i| *i < self.curator.prefs.shelves.len()) {
                    Some(i) => {
                        let s = &mut self.curator.prefs.shelves[i];
                        s.lens = lens;
                        s.name = name;
                        i
                    }
                    None => self.curator.prefs.add_shelf(&name, lens),
                };
                self.curator.prefs.shelves[i].order = order;
                let shown = self.curator.prefs.shelves[i].name.clone();
                self.save_curator_prefs(&format!("Saved “{shown}”. It is on Home"));
                n.back.map(|b| *b).map(|p| match p {
                    // The Lens goes on as that shelf's.
                    Page::Lens(mut l) => {
                        l.shelf = Some(i);
                        Page::Lens(l)
                    }
                    other => other,
                })
            }
            Purpose::NewSticker(path) => {
                let added = self.library.add_sticker(&name);
                let mut back = n.back.map(|b| *b);
                if let Ok(id) = added {
                    let on = self.library.get(&path).stickers.contains(&id) || self.library.toggle_sticker(&path, id).unwrap_or(false);
                    // The sheet shows the new sticker, ticked, above "New sticker…".
                    if let Some(Page::Stickers { sheet, ids, .. }) = back.as_mut() {
                        if !ids.contains(&id) {
                            let at = CaseKind::ALL.len() + ids.len();
                            ids.push(id);
                            let label = self.library.stickers().iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default();
                            sheet.rows.insert(at, (label, Some(on)));
                            sheet.sel = at;
                        }
                    }
                }
                self.curator_saved(added, "Sticker made");
                self.curator.touch();
                back
            }
            Purpose::RenameShelf(i) => {
                if let Some(s) = self.curator.prefs.shelves.get_mut(i) {
                    s.name = if name.is_empty() { s.name.clone() } else { name };
                }
                self.save_curator_prefs("Renamed");
                None
            }
            Purpose::RenameSticker(id) => {
                let saved = self.library.rename_sticker(id, &name);
                self.curator_saved(saved, "Renamed");
                self.curator.touch();
                self.rebuild();
                None
            }
        }
    }

    /// An entry of a shelf's or sticker's options.
    fn curator_option(&mut self, item: Item, act: Act) -> Option<Page> {
        match (item, act) {
            (Item::SmartShelf(i), Act::Edit) => {
                let shelf = self.curator.prefs.shelves.get(i)?;
                let mut page = LensPage::new(shelf.lens.clone(), Some(i));
                page.order = shelf.order;
                if let Some(ix) = self.curator.index.as_ref() {
                    page.clamp(ix);
                }
                Some(Page::Lens(page))
            }
            (Item::SmartShelf(i), Act::Order) => {
                let order = self.curator.prefs.shelves.get(i)?.order;
                let labels: Vec<&str> = Order::ALL.iter().map(|o| o.label()).collect();
                let at = Order::ALL.iter().position(|o| *o == order).unwrap_or(0);
                Some(Page::Order { sheet: Sheet::choice("Order", &labels, at), back: None, shelf: Some(i) })
            }
            (Item::SmartShelf(i), Act::Rename) => {
                let name = self.curator.prefs.shelves.get(i)?.name.clone();
                Some(name_page("Rename the shelf", &name, Purpose::RenameShelf(i), None))
            }
            (Item::Sticker(id), Act::Rename) => {
                let name = self.library.stickers().iter().find(|s| s.id == id)?.name.clone();
                Some(name_page("Rename the sticker", &name, Purpose::RenameSticker(id), None))
            }
            (_, Act::Pin) => {
                self.curator_y();
                None
            }
            (Item::SmartShelf(i), Act::Delete) => {
                if i < self.curator.prefs.shelves.len() {
                    let name = self.curator.prefs.shelves.remove(i).name;
                    self.save_curator_prefs(&format!("Deleted “{name}”"));
                }
                None
            }
            (Item::Sticker(id), Act::Delete) => {
                let saved = self.curator_delete_sticker(id);
                self.curator_saved(saved, "Sticker deleted");
                None
            }
            _ => None,
        }
    }

    /// A for Play on an Echo or a version: the card and pages close, the game starts.
    fn curator_play(&mut self, game: usize) {
        let Some(Item::Game(s, g)) = self.curator_item(game) else { return };
        self.card = None;
        let (sys, game) = (&self.systems[s], &self.systems[s].games[g]);
        self.launch(sys.name.clone(), game.name.clone(), game.path.clone());
    }

    /// "Surprise me" (A on its entry, Right on the card it opened): a game picked by `Index::surprise`, none shown since the
    /// entry was pressed, on its card with the reason. Before the index or with no games it says so in a toast.
    pub(crate) fn curator_surprise(&mut self) {
        let Some(ix) = self.curator.index.clone() else {
            self.toast = Some(("Getting ready…".into(), Instant::now()));
            return;
        };
        // The day's pick first, once a session: it is the game the Curator row and this entry name.
        let today = self.curator.today.pick.clone().filter(|_| !self.curator.today_shown && self.curator.surprise_seen.is_empty());
        self.curator.today_shown = true;
        let rng = self.curator.rng.get_or_insert_with(|| Rng::new(clock_seed()));
        let pick = today.or_else(|| ix.surprise(rng, &self.curator.surprise_seen));
        let Some((pick, item @ Item::Game(s, g))) = pick.and_then(|p| Some((p.clone(), self.curator_item(p.game)?))) else {
            self.toast = Some(("No games to pick from yet".into(), Instant::now()));
            return;
        };
        self.curator.surprise_seen.push(pick.game);
        let (system, name, path) = (self.systems[s].name.clone(), self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
        println!("curator: surprise {name} ({system}): {}", pick.reason);
        self.show_card(item, system, name, path);
        if let Some(open) = self.card.as_mut() {
            open.card.data.surprise = Some(pick.reason);
        }
    }

    /// X on an Echo: that game's card in place of the one open.
    fn curator_card(&mut self, game: usize) {
        let Some(item @ Item::Game(s, g)) = self.curator_item(game) else { return };
        let (system, name, path) = (self.systems[s].name.clone(), self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
        self.show_card(item, system, name, path);
    }

    /// Y on the Versions page.
    fn curator_prefer(&mut self, game: usize) {
        let Some(ix) = self.curator.index.clone() else { return };
        let f = ix.game(game);
        let Some(work) = f.work.clone() else { return };
        let saved = self.library.set_preferred(&work, &f.path);
        self.curator_saved(saved, &format!("Preferred: {}", system_short(&self.systems, &f.system)));
        self.curator.touch();
    }

    /// The card's rows again (after stickers changed, or a new index after a rating). All of them (`card_rows`): the
    /// Curator rows alone took Screenshots and Hide or delete off the card.
    fn curator_card_refresh(&mut self) {
        let Some((system, path)) = self.card.as_ref().map(|o| (o.system.clone(), o.path.clone())) else { return };
        let rows = self.card_rows(&system, &path);
        if let Some(open) = self.card.as_mut() {
            if open.card.focus.is_some_and(|f| f >= rows.len()) {
                open.card.focus = None;
            }
            open.card.data.rows = rows;
        }
    }

    /// A sticker name to start from: the ones a player would make first, the first not taken yet.
    fn suggest_sticker_name(&self, _path: &Path) -> String {
        let taken: HashSet<String> = self.library.stickers().iter().map(|s| s.name.to_lowercase()).collect();
        ["Couch co-op", "Before bed", "On the train", "Play next", "With the kids"]
            .into_iter()
            .find(|n| !taken.contains(&n.to_lowercase()))
            .unwrap_or("Sticker")
            .to_string()
    }

    // ---- the game card ----------------------------------------------------------------------------------------

    /// The card's Curator rows for a game: Echoes (switched on, and something to show), Versions (two or more owned),
    /// Stickers (switched on).
    pub(crate) fn curator_card_rows(&self, path: &Path) -> Vec<CuratorRow> {
        let mut rows = Vec::new();
        let p = &self.curator.prefs;
        let ix = self.curator.index.as_ref();
        let game = ix.and_then(|ix| ix.find(path));
        if p.echoes {
            match (ix, game) {
                (None, _) => rows.push(CuratorRow { kind: RowKind::Echoes, title: "Echoes".into(), line: "Getting ready…".into() }),
                (Some(ix), Some(g)) => {
                    let e = ix.echoes(g, INLINE);
                    if !e.is_empty() {
                        let names: Vec<String> = e.iter().map(|x| ix.game(x.game).name.clone()).collect();
                        rows.push(CuratorRow { kind: RowKind::Echoes, title: "Echoes".into(), line: names.join(" · ") });
                    }
                }
                _ => {}
            }
        }
        if let (Some(ix), Some(g)) = (ix, game) {
            let remakes = ix.remakes(g);
            let related = |r: &[usize]| r.iter().map(|&x| format!("{} on {}", ix.game(x).name, system_short(&self.systems, &ix.game(x).system))).collect::<Vec<_>>().join(", ");
            if ix.has_versions(g) {
                let v = ix.versions(g);
                let pref = ix.preferred(g);
                let systems: Vec<String> = v.iter().map(|&x| system_short(&self.systems, &ix.game(x as usize).system)).collect();
                let mut line = format!("{} on this Pocket: {}. Plays {}", v.len(), systems.join(", "), system_short(&self.systems, &ix.game(pref).system));
                if !remakes.is_empty() {
                    line.push_str(&format!(". Related: {}", related(&remakes)));
                }
                rows.push(CuratorRow { kind: RowKind::Versions, title: "Versions".into(), line });
            } else if !remakes.is_empty() {
                // One version, but a remake (or the original of this remake) is on the Pocket too.
                rows.push(CuratorRow { kind: RowKind::Versions, title: "Versions".into(), line: format!("Related: remake, {}", related(&remakes)) });
            }
        }
        // Always there: the case stickers (Completed, 100 %, …) are on this row too, Curator's own ones with them.
        let meta = self.library.get(path);
        let mut on = meta.case.names();
        if p.stickers {
            on.extend(meta.stickers.iter().filter_map(|id| self.library.stickers().iter().find(|s| s.id == *id)).map(|s| s.name.clone()));
        }
        // A line of parts, so each sticker's name is translated on its own.
        let line = if on.is_empty() { "None yet. Press A to add one".to_string() } else { on.join(" · ") };
        rows.push(CuratorRow { kind: RowKind::Stickers, title: "Stickers".into(), line });
        rows
    }

    /// A on one of the card's Curator rows.
    pub(crate) fn curator_card_row(&mut self, kind: RowKind) {
        let Some(path) = self.card.as_ref().map(|o| o.path.clone()) else { return };
        match kind {
            RowKind::Echoes => self.open_echoes(&path),
            RowKind::Versions => self.open_versions(&path),
            RowKind::Stickers => self.open_sticker_sheet(&path),
            // The save's versions (`app_sync_conflicts`).
            RowKind::SaveVersions => self.open_conflicts_for_card(),
            // Not Curator's: Hide or delete (`app_storage`).
            RowKind::Manage => self.open_card_manage(),
            // The game's screenshots (`app_gallery`).
            RowKind::Screenshots => self.open_gallery_for_card(),
        }
    }

    /// After the card is painted: its Curator rows are what touch can reach on it.
    pub(crate) fn curator_card_hits(&mut self, screen_w: i32) {
        let Some(open) = self.card.as_ref() else { return };
        for (i, area) in open.card.row_areas().into_iter().enumerate() {
            if let Some((y, h)) = area {
                self.hits.row(Area::new(MARGIN, y, screen_w - 2 * MARGIN, h), i);
            }
        }
    }

    /// A tap on the card at height `y`: on a Curator row (they run the card's width), Some(true) when it was the
    /// selected one (open it, as A does), Some(false) when the selection is on it now; None elsewhere.
    pub(crate) fn curator_card_tap(&mut self, y: f32) -> Option<bool> {
        let open = self.card.as_mut().filter(|o| matches!(o.card.mode, game_card::Mode::View))?;
        let i = open.card.row_areas().into_iter().position(|a| a.is_some_and(|(top, h)| y >= top as f32 && y < (top + h) as f32))?;
        if open.card.focus == Some(i) {
            return Some(true);
        }
        open.card.focus_row(i);
        Some(false)
    }

    fn open_echoes(&mut self, path: &Path) {
        let Some(ix) = self.curator.index.clone() else { return };
        let Some(seed) = ix.find(path) else { return };
        let t = Instant::now();
        let echoes = ix.echoes(seed, echoes::FULL_LIST);
        println!("curator: Echoes of {} in {:?}", path.display(), t.elapsed());
        // "Why these?": only when taste changed this list's order, naming the games behind it (12 games, microseconds).
        let why = ix.why(seed, &echoes);
        self.curator.page = Some(Page::Echoes(EchoesPage { seed, seed_name: ix.game(seed).name.clone(), echoes, sel: 0, elsewhere: Vec::new(), why }));
        if self.curator.prefs.not_on_device {
            self.find_elsewhere(&ix, seed);
        }
    }

    /// "Not on this device": games in gamedb on the library's systems, by the seed's series or developer, that the
    /// library doesn't have; names only, on a thread (a system's list has to be unpacked).
    fn find_elsewhere(&self, ix: &Arc<Index>, seed: usize) {
        let f = ix.game(seed).clone();
        let owned: HashSet<String> = ix.catalog.games.iter().filter_map(|g| g.qid.clone()).collect();
        let systems: Vec<String> = self.systems.iter().map(|s| s.name.clone()).collect();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let mut db = pocket_ui::gamedb::GameDb::embedded();
            let mut found: Vec<(u8, u32, String)> = Vec::new();
            let mut seen = HashSet::new();
            for sys in std::iter::once(f.system.clone()).chain(systems) {
                let Some(list) = db.system(&sys) else { continue };
                for g in &list.games {
                    if owned.contains(&g.qid) || !seen.insert(g.qid.clone()) {
                        continue;
                    }
                    let series = f.series.iter().any(|s| g.series.contains(s));
                    let dev = f.developers.iter().any(|d| g.developers.contains(d));
                    if series || dev {
                        found.push((series as u8 + dev as u8, g.sitelinks, g.label.clone()));
                    }
                }
            }
            found.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
            let names = found.into_iter().take(ELSEWHERE).map(|x| x.2).collect();
            let _ = proxy.send_event(UserEvent::Curator(Event::Elsewhere { seed, names }));
        });
    }

    fn open_versions(&mut self, path: &Path) {
        let Some(ix) = self.curator.index.clone() else { return };
        let Some(game) = ix.find(path) else { return };
        let pref = ix.preferred(game);
        // The versions, then games that are a remake of it (or that it remakes): "Related: remake".
        let related = ix.remakes(game);
        let members: Vec<(usize, bool)> = ix.versions(game).iter().map(|&v| (v as usize, false)).chain(related.iter().map(|&r| (r, true))).collect();
        let rows = members
            .into_iter()
            .map(|(v, related)| {
                let f = ix.game(v);
                let mut release = f.rom.describe();
                if let Some(rev) = &f.rom.revision {
                    release = if release.is_empty() { rev.clone() } else { format!("{release} · {rev}") };
                }
                let u = &ix.user.games[v];
                let mut saves = Vec::new();
                let n = save_states(&f.system, &f.path);
                saves.push(match n {
                    0 => "No save states".to_string(),
                    1 => "1 save state".to_string(),
                    n => format!("{n} save states"),
                });
                match u.last_played {
                    Some(t) => saves.push(format!("last played {}", pocket_ui::recents::ago(t))),
                    None if u.secs == 0 => saves.push("not played yet".into()),
                    None => {}
                }
                if u.secs > 0 {
                    saves.push(format!("{} played", pocket_ui::recents::duration(u.secs)));
                }
                VersionRow {
                    game: v,
                    system: f.system.clone(),
                    system_name: system_label(&self.systems, &f.system),
                    title: f.name.clone(),
                    release: if release.is_empty() { "Release not named in the file".into() } else { release },
                    runs: self.runs_on(&f.system, &f.path),
                    saves: saves.join(" · "),
                    preferred: v == pref && !related,
                    related,
                }
            })
            .collect();
        let rows: Vec<VersionRow> = rows;
        let sel = rows.iter().position(|r| r.game == game).unwrap_or(0);
        self.curator.page = Some(Page::Versions(VersionsPage { title: ix.game(game).name.clone(), rows, sel }));
    }

    /// "Snes9x on pocket-core · performance Auto": how a file starts, as `launch` would start it.
    fn runs_on(&self, system: &str, path: &Path) -> String {
        let Some(sys) = self.systems.iter().find(|s| s.name == system) else { return String::new() };
        let host = Path::new(super::host_bin()).exists() || std::env::var_os("POCKETUI_SNAPSHOT_POCKET_CORE").is_some();
        let (emulator, core) = self.route(sys, Some(path), host);
        let mut runs = pocket_ui::consoles::runner_text(&emulator, &core).unwrap_or_default();
        if emulator == "pocket-core" {
            if let Some(setting) = pocket_ui::options::setting("performance") {
                let s = pocket_ui::options::GameSettings::load(&self.data_dir, system, path);
                let label = setting.choices[s.index(setting)].label;
                runs.push_str(&format!(" · performance {}", label.to_lowercase()));
            }
        }
        runs
    }

    /// The card's Stickers sheet: the five case stickers, then (with Curator's stickers on) the player's own and
    /// "New sticker…".
    fn open_sticker_sheet(&mut self, path: &Path) {
        let meta = self.library.get(path);
        let case = meta.case;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let this_year = pocket_ui::play_history::civil(now / 86_400).0.clamp(0, i32::from(u16::MAX)) as u16;
        let release = self.card.as_ref().and_then(|o| o.card.data.year);
        let year = case.goty.unwrap_or_else(|| case_stickers::default_year(release, this_year));
        let shown = CaseStickers { goty: Some(year), ..case };
        let mut rows: Vec<(String, Option<bool>)> = CaseKind::ALL.iter().map(|k| (shown.name(*k), Some(case.has(*k)))).collect();
        let mut ids = Vec::new();
        if self.curator.prefs.stickers {
            rows.extend(self.library.stickers().iter().map(|s| (s.name.clone(), Some(meta.stickers.contains(&s.id)))));
            ids = self.library.stickers().iter().map(|s| s.id).collect();
            rows.push(("New sticker…".into(), None));
        }
        let goty = CaseKind::ALL.iter().position(|k| *k == CaseKind::Goty);
        let sheet = Sheet { title: "Stickers".into(), rows, sel: 0, single: false, adjust: goty };
        self.curator.page = Some(Page::Stickers { sheet, path: path.to_path_buf(), ids, case, year });
    }

    // ---- painting -----------------------------------------------------------------------------------------------

    pub(crate) fn paint_curator(&mut self, c: &mut Canvas) {
        let Some(page) = self.curator.page.take() else { return };
        match &page {
            Page::Lens(p) => self.paint_lens(c, p, true),
            Page::Echoes(p) => self.paint_echoes(c, p),
            Page::Versions(p) => self.paint_versions(c, p),
            Page::History(p) => self.paint_history_page(c, p),
            Page::Review(p) => self.paint_review_page(c, p),
            Page::Stickers { sheet, .. } => {
                if self.card.is_some() {
                    self.paint_card(c);
                }
                self.hits.clear();
                self.paint_sheet(c, sheet);
            }
            Page::Options { sheet, .. } => {
                self.paint_home(c, false);
                self.hits.clear();
                self.paint_sheet(c, sheet);
            }
            Page::Order { sheet, back, .. } => {
                match back.as_deref() {
                    // Without its hints: the sheet's are the ones on screen (two sets a frame would crossfade forever).
                    Some(Page::Lens(l)) => self.paint_lens(c, l, false),
                    _ => self.paint_home(c, false),
                }
                self.hits.clear();
                self.paint_sheet(c, sheet);
            }
            Page::Name(n) => {
                c.clear(pal().void);
                let title = self.text.fit(Face::Title, 50, &n.title, c.w - 2 * MARGIN);
                self.text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, &title);
                pocket_ui::setup::paint_keyboard_with(c, &mut self.text, &n.kb, "Save");
                self.paint_hint_row(c, &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", "Save")]);
            }
        }
        self.curator.page = Some(page);
        self.paint_curator_toast(c);
    }

    fn paint_curator_toast(&mut self, c: &mut Canvas) {
        if let Some((msg, at)) = self.toast.clone() {
            if at.elapsed() < super::TOAST {
                let w = self.text.width(Face::Body, 28, &msg) + 48;
                c.round_rect(c.w / 2 - w / 2, 36, w, 52, 26, KEYCAP);
                self.text.draw(c, Face::Body, 28, c.w / 2 - w / 2 + 24, 72, CHALK, &msg);
            }
        }
    }

    /// The top line of a Curator page: where it is, left; the status corner, right.
    fn page_top(&mut self, c: &mut Canvas, place: &str) {
        self.paint_top_line(c, Some(place), pal().dust);
    }

    /// A spine or, selected, the paper label: the row's background and stripe; returns (text, secondary) colours.
    fn row_back(&self, c: &mut Canvas, (x, y, w, h): (i32, i32, i32, i32), stripe_colour: u32, selected: bool) -> (u32, u32) {
        pocket_ui::kit::shelf_row(c, x, y, w, h, selected);
        c.rect(x, y, STRIPE, h, if selected { stripe_colour } else { mix(stripe_colour, pal().spine, 150) });
        if selected {
            (pal().ink, pal().label_dust)
        } else {
            (pal().chalk, pal().dust)
        }
    }

    /// The Lens; `hints`: with its hint row (not under a sheet, which has its own).
    fn paint_lens(&mut self, c: &mut Canvas, p: &LensPage, hints: bool) {
        c.clear(pal().void);
        let place = match p.shelf.and_then(|i| self.curator.prefs.shelves.get(i)) {
            Some(s) => format!("Curator · {}", s.name),
            None => "Curator".to_string(),
        };
        self.page_top(c, &place);
        self.text.draw(c, Face::Title, 67, MARGIN, 160, pal().chalk, "Lens");
        let Some(ix) = self.curator.index.clone() else {
            self.text.draw(c, Face::Title, 50, MARGIN, 330, pal().dust, "Getting ready…");
            self.text.draw(c, Face::Body, 28, MARGIN, 380, pal().dust, "Curator is reading your library. This takes a moment once.");
            if hints {
                self.paint_hint_row(c, &[("B", "Back")]);
            }
            return;
        };
        let merge = self.curator.prefs.merge_versions;
        let n = ix.count(&ix.matches(&p.lens), merge);
        let count = if n == 1 { "1 game".to_string() } else { format!("{n} games") };
        self.text.draw_right(c, Face::Title, 50, c.w - MARGIN, 160, pal().chalk, &count);
        let sub = if p.lens.is_empty() { "Nothing picked yet: every game".to_string() } else { ix.suggest_name(&p.lens) };
        // Another order than by name is said after what is picked; SELECT changes it.
        let sub = match p.order.suffix() {
            Some(how) => format!("{sub} · {how}"),
            None => sub,
        };
        let sub = self.text.fit(Face::Body, 28, &sub, c.w - 2 * MARGIN);
        self.text.draw(c, Face::Body, 28, MARGIN, 208, pal().dust, &sub);

        let facets = ix.shown_facets();
        let (top, bottom, row, gap) = (246, c.h - 110, pocket_ui::type_scale::row(64, 37), 8);
        let pitch = row + gap;
        let visible = ((bottom - top) / pitch).max(1) as usize;
        let (lx, lw) = (MARGIN, pocket_ui::type_scale::wide(360, 37));
        let rx = lx + lw + 28;
        let rw = c.w - MARGIN - rx;
        let col = p.column();
        // The facets.
        let first = super::window_start(p.facet, visible, facets.len());
        // Both columns scroll smoothly under their edges (`app_motion`); a selection changes at once.
        let band_end = top + visible as i32 * pitch - gap;
        let first_f = self.motion.scroll_at(1, 0, first, Instant::now());
        let mut band = c.rows(top, band_end);
        for (k, off) in super::app_motion::rows_in_view(first_f, visible, facets.len()) {
            let (f, c) = (&facets[k], &mut band);
            let y = (off * pitch as f32).round() as i32;
            let selected = k == p.facet;
            let picks = p.lens.keys(*f).len();
            let paper = selected && col == Col::Facets;
            let stripe_colour = if picks > 0 { pal().chalk } else { pal().dust };
            let (fg, dim) = self.row_back(c, (lx, y, lw, row), stripe_colour, paper);
            if selected && !paper {
                // The facet whose values are open: a brighter spine.
                c.rect(lx + STRIPE, y, lw - STRIPE, row, mix(pal().spine, pal().label, 40));
                c.rect(lx, y, STRIPE, row, pal().chalk);
            }
            self.text.draw(c, Face::Title, 37, lx + STRIPE + 24, y + row / 2 + pocket_ui::type_scale::base(13, 37), fg, f.label());
            if picks > 0 {
                self.text.draw_right(c, Face::Body, 28, lx + lw - 24, y + row / 2 + pocket_ui::type_scale::base(10, 28), dim, &picks.to_string());
            }
            if col == Col::Facets {
                self.hits.row(Area::new(lx, top + y, lw, row), k);
            } else {
                self.hits.button(Area::new(lx, top + y, lw, row), Button::Left);
            }
        }
        scroll_bar(c, lx - 20, top, visible as i32 * pitch - gap, first, visible, facets.len());
        // The values of the selected facet, with what each would leave.
        if let Some(f) = facets.get(p.facet).copied() {
            let fv = ix.facet(f);
            let counts = ix.counts(&p.lens, f, merge);
            let first = super::window_start(p.value, visible, fv.values.len());
            let first_f = self.motion.scroll_at(2, p.facet as u64, first, Instant::now());
            let mut band = c.rows(top, band_end);
            for (k, off) in super::app_motion::rows_in_view(first_f, visible, fv.values.len()) {
                let (v, c) = (&fv.values[k], &mut band);
                let y = (off * pitch as f32).round() as i32;
                let paper = k == p.value && col == Col::Values;
                let stripe_colour = if f == Facet::System { stripe(&v.key) } else { pal().dust };
                let (fg, dim) = self.row_back(c, (rx, y, rw, row), stripe_colour, paper);
                let picked = p.lens.picked(f, &v.key);
                let n = counts.get(k).copied().unwrap_or(0);
                let mark = if picked { Status::Completed } else { Status::NotStarted };
                game_card::paint_mark(c, rx + STRIPE + 30, y + row / 2, mark, if picked { fg } else { dim });
                let count = n.to_string();
                let cw = self.text.width(Face::Body, 28, &count);
                let label = self.text.fit(Face::Title, 37, &v.label, rw - STRIPE - 64 - cw - 48);
                let colour = if n == 0 && !picked { dim } else { fg };
                self.text.draw(c, Face::Title, 37, rx + STRIPE + 60, y + row / 2 + pocket_ui::type_scale::base(13, 37), colour, &label);
                self.text.draw_right(c, Face::Body, 28, rx + rw - 24, y + row / 2 + pocket_ui::type_scale::base(10, 28), dim, &count);
                if col == Col::Values {
                    self.hits.row(Area::new(rx, top + y, rw, row), k);
                } else {
                    self.hits.button(Area::new(rx, top + y, rw, row), Button::Right);
                }
            }
            scroll_bar(c, c.w - MARGIN + 16, top, visible as i32 * pitch - gap, first, visible, fv.values.len());
            // What the values mean, where that needs saying (Length is a rough guide …), under them when there's room.
            if let Some(note) = f.note().filter(|_| fv.values.len() < visible) {
                let mut ny = top + fv.values.len() as i32 * pitch + 32;
                for line in self.text.wrap(Face::Body, 28, note, rw - 24).iter().take(3) {
                    self.text.draw(c, Face::Body, 28, rx + 12, ny, pal().dust, line);
                    ny += pocket_ui::type_scale::line(28, 12);
                }
            }
            match col {
                Col::Facets => self.hits.list(p.facet, facets.len(), 0.0, pitch as f32),
                Col::Values => self.hits.list(p.value, fv.values.len(), 0.0, pitch as f32),
            }
        }
        if hints {
            self.paint_hint_row(c, &p.hints());
        }
    }

    fn paint_echoes(&mut self, c: &mut Canvas, p: &EchoesPage) {
        c.clear(pal().void);
        self.page_top(c, "Echoes");
        let title = self.text.fit(Face::Title, 67, &format!("More like {}", p.seed_name), c.w - 2 * MARGIN);
        self.text.draw(c, Face::Title, 67, MARGIN, 160, pal().chalk, &title);
        let sub = match (p.echoes.len(), &p.why) {
            (0, _) => "Nothing in your library is alike enough yet.".to_string(),
            (_, Some(why)) => format!("Why these? {why}"),
            (n, None) => format!("{n} games you own, the closest first"),
        };
        let sub = self.text.fit(Face::Body, 28, &sub, c.w - 2 * MARGIN);
        self.text.draw(c, Face::Body, 28, MARGIN, 208, pal().dust, &sub);
        // A name and a line under it: taller by both lines' growth with Settings → Text size.
        let (g1, g2) = (pocket_ui::type_scale::grow(37), pocket_ui::type_scale::grow(28));
        let (top, row, gap) = (246, 124 + (g1 + g2) * 6 / 5, 10);
        let pitch = row + gap;
        let bottom = c.h - 110;
        let elsewhere_h = if p.elsewhere.is_empty() { 0 } else { 120 };
        let visible = (((bottom - top) / pitch) as usize).max(1);
        let first = super::window_start(p.sel, visible, p.echoes.len());
        let w = c.w - 2 * MARGIN;
        let Some(ix) = self.curator.index.clone() else { return };
        let mut y = top;
        // The list scrolls smoothly under its edges (`app_motion`).
        let first_f = self.motion.scroll_at(3, p.seed as u64, first, Instant::now());
        let mut band = c.rows(top, top + visible as i32 * pitch - gap);
        for (k, off) in super::app_motion::rows_in_view(first_f, visible, p.echoes.len()) {
            let (e, c) = (&p.echoes[k], &mut band);
            y = top + (off * pitch as f32).round() as i32;
            let y = y - top;
            let f = ix.game(e.game);
            let selected = k == p.sel;
            let (fg, dim) = self.row_back(c, (MARGIN, y, w, row), stripe(&f.system), selected);
            let mut tx = MARGIN + STRIPE + 24;
            let cover = self.curator_item(e.game).and_then(|item| self.cover(item, 104, row - 16));
            match cover {
                Some(cover) => {
                    let (cw, ch) = (cover.w as i32, cover.h as i32);
                    c.blit_scaled(&cover, tx + (104 - cw) / 2, y + (row - ch) / 2, cw, ch);
                }
                // No box art: a quiet box in the system's colour keeps the titles in line.
                None => {
                    let bg = if selected { pal().label } else { pal().spine };
                    c.round_rect(tx + 12, y + 14, 80, row - 28, 6, mix(stripe(&f.system), bg, 170));
                }
            }
            tx += 104 + 24;
            let tw = MARGIN + w - 24 - tx;
            let name = self.text.fit(Face::Title, 37, &f.name, tw);
            self.text.draw(c, Face::Title, 37, tx, y + 54 + g1, fg, &name);
            let meta = self.text.fit(Face::Body, 28, &format!("{} · {}", system_short(&self.systems, &f.system), e.reason), tw);
            self.text.draw(c, Face::Body, 28, tx, y + 98 + g1 + g2, dim, &meta);
            self.hits.row(Area::new(MARGIN, top + y, w, row), k);
            self.motion.row(MARGIN, top + y, w, row + gap);
        }
        scroll_bar(c, c.w - MARGIN + 16, top, visible as i32 * pitch - gap, first, visible, p.echoes.len());
        self.hits.list(p.sel, p.echoes.len(), 0.0, pitch as f32);
        // "Not on this device": names only, under the list once its end is in view.
        let end_shown = first + visible >= p.echoes.len();
        if !p.elsewhere.is_empty() && end_shown {
            let y0 = if p.echoes.is_empty() { top } else { y + pitch + 20 };
            if y0 + elsewhere_h <= bottom + 20 {
                self.text.draw(c, Face::Body, 21, MARGIN, y0 + 10, pal().dust, "Not on this device");
                let names = p.elsewhere.join(" · ");
                for (i, line) in self.text.wrap(Face::Body, 28, &names, w).iter().take(2).enumerate() {
                    self.text.draw(c, Face::Body, 28, MARGIN, y0 + 50 + i as i32 * 38, pal().dust, line);
                }
            }
        }
        self.paint_hint_row(c, &p.hints());
    }

    fn paint_versions(&mut self, c: &mut Canvas, p: &VersionsPage) {
        c.clear(pal().void);
        self.page_top(c, "Versions");
        let title = self.text.fit(Face::Title, 67, &p.title, c.w - 2 * MARGIN);
        self.text.draw(c, Face::Title, 67, MARGIN, 160, pal().chalk, &title);
        let versions = p.rows.iter().filter(|r| !r.related).count();
        let sub = match versions {
            1 => "One version on this Pocket, and a remake.".to_string(),
            n => format!("{n} versions on this Pocket. The preferred one starts from mixed lists."),
        };
        let sub = self.text.fit(Face::Body, 28, &sub, c.w - 2 * MARGIN);
        self.text.draw(c, Face::Body, 28, MARGIN, 208, pal().dust, &sub);
        // A heading and three lines under it.
        let (g1, g2) = (pocket_ui::type_scale::grow(37), pocket_ui::type_scale::grow(28));
        let (top, row, gap) = (246, 196 + (g1 + 3 * g2) * 6 / 5, 12);
        let pitch = row + gap;
        let visible = (((c.h - 110 - top) / pitch) as usize).max(1);
        let first = super::window_start(p.sel, visible, p.rows.len());
        let w = c.w - 2 * MARGIN;
        // The list scrolls smoothly under its edges (`app_motion`).
        let first_f = self.motion.scroll_at(4, 0, first, Instant::now());
        let mut band = c.rows(top, top + visible as i32 * pitch - gap);
        for (k, off) in super::app_motion::rows_in_view(first_f, visible, p.rows.len()) {
            let (r, c) = (&p.rows[k], &mut band);
            let y = (off * pitch as f32).round() as i32;
            let selected = k == p.sel;
            let (fg, dim) = self.row_back(c, (MARGIN, y, w, row), stripe(&r.system), selected);
            let tx = MARGIN + STRIPE + 28;
            let tw = w - STRIPE - 56;
            let mut side_w = 0;
            if r.related {
                side_w = self.text.width(Face::Body, 28, "Related: remake") + 32;
                self.text.draw_right(c, Face::Body, 28, MARGIN + w - 28, y + 52 + g1, dim, "Related: remake");
            }
            if r.preferred {
                side_w = self.text.width(Face::Body, 28, "Preferred") + 32;
                self.text.draw_right(c, Face::Body, 28, MARGIN + w - 28, y + 52 + g1, fg, "Preferred");
                game_card::paint_mark(c, MARGIN + w - 28 - side_w + 8, y + 42 + g1, Status::Completed, fg);
                side_w += 24;
            }
            let head = if r.related { format!("{} · {}", r.title, r.system_name) } else { r.system_name.clone() };
            let head = self.text.fit(Face::Title, 37, &head, tw - side_w);
            self.text.draw(c, Face::Title, 37, tx, y + 54 + g1, fg, &head);
            let release = if r.title != p.title && !r.related { format!("{} · {}", r.title, r.release) } else { r.release.clone() };
            let lines = [(release, fg), (r.runs.clone(), dim), (r.saves.clone(), dim)];
            for (i, (line, colour)) in lines.into_iter().filter(|(l, _)| !l.is_empty()).enumerate() {
                let line = self.text.fit(Face::Body, 28, &line, tw);
                self.text.draw(c, Face::Body, 28, tx, y + 98 + g1 + g2 + i as i32 * pocket_ui::type_scale::line(28, 12), colour, &line);
            }
            self.hits.row(Area::new(MARGIN, top + y, w, row), k);
            self.motion.row(MARGIN, top + y, w, row + gap);
        }
        scroll_bar(c, c.w - MARGIN + 16, top, visible as i32 * pitch - gap, first, visible, p.rows.len());
        self.hits.list(p.sel, p.rows.len(), 0.0, pitch as f32);
        self.paint_hint_row(c, &p.hints());
    }

    /// A short list on a sheet over the dimmed page, like the card's status picker.
    fn paint_sheet(&mut self, c: &mut Canvas, sheet: &Sheet) {
        c.shade_rect(0, 0, c.w, c.h, pal().void, 225);
        // For the snapshot checks of Settings → Text size: the page under the sheet is dimmed, not read.
        pocket_ui::text::boxes::covered();
        let (w, row, gap) = (pocket_ui::type_scale::wide(640, 37), pocket_ui::type_scale::row(76, 37), 8);
        let x = (c.w - w) / 2;
        // The whole sheet (title, rows and bottom padding) stays above the hints. In Large, eight rows no longer
        // fit on a native 768-line output; the window must shrink with the available height as well as grow in type.
        let room = (c.h - 100 - 32 - 168).max(row);
        let shown = sheet.rows.len().min(8).min(((room + gap) / (row + gap)).max(1) as usize);
        let first = super::window_start(sheet.sel, shown, sheet.rows.len());
        let total = shown as i32 * (row + gap) - gap;
        let top = (c.h - 100 - total - 168) / 2 + 120;
        let (sx, sy, sw, sh) = (x - 48, top - 120, w + 96, total + 168);
        c.rect(sx, sy, sw, sh, pal().void);
        if pal().t > 0 {
            c.frame(sx, sy, sw, sh, 1, pal().pick(VOID, 0xd6d0c4));
            game_card::lift_shadow(c, sx, sy + sh, sw, pal().t);
        }
        let title = self.text.fit(Face::Title, 50, &sheet.title, w);
        self.text.draw(c, Face::Title, 50, x, top - 36, pal().chalk, &title);
        let stripe_colour = stripe(if sheet.single { CURATOR_STRIPE } else { STICKER_STRIPE });
        for (k, (label, tick)) in sheet.rows.iter().enumerate().skip(first).take(shown) {
            let y = top + (k - first) as i32 * (row + gap);
            let selected = k == sheet.sel;
            let (fg, dim) = match tick {
                Some(_) => self.row_back(c, (x, y, w, row), stripe_colour, selected),
                None => {
                    if selected {
                        c.rect(x, y, w, row, pal().label);
                    }
                    if selected {
                        (pal().ink, pal().label_dust)
                    } else {
                        (pal().dust, pal().dust)
                    }
                }
            };
            match tick {
                Some(on) => {
                    game_card::paint_mark(c, x + STRIPE + 36, y + row / 2, if *on { Status::Completed } else { Status::NotStarted }, if *on { fg } else { dim });
                    let label = self.text.fit(Face::Title, 37, label, w - STRIPE - 100);
                    self.text.draw(c, Face::Title, 37, x + STRIPE + 72, y + row / 2 + pocket_ui::type_scale::base(13, 37), fg, &label);
                }
                None => {
                    self.text.draw(c, Face::Body, 28, x + STRIPE + 72, y + row / 2 + pocket_ui::type_scale::base(10, 28), fg, label);
                }
            }
            self.hits.row(Area::new(x, y, w, row), k);
        }
        // Longer than its window (the case stickers, then the player's own): a scroll bar at the sheet's right edge.
        if sheet.rows.len() > shown {
            let n = sheet.rows.len() as i32;
            let (bar_h, bar_y) = (total * shown as i32 / n, total * first as i32 / n);
            c.rect(sx + sw - 24, top, 4, total, pal().pill);
            c.rect(sx + sw - 24, top + bar_y, 4, bar_h.max(12), pal().chip);
        }
        self.hits.list(sheet.sel, sheet.rows.len(), 0.0, (row + gap) as f32);
        c.rect(0, c.h - 100, c.w, 100, pal().void);
        self.paint_hint_row(c, &sheet.hints());
    }

    // ---- Settings → Curator ---------------------------------------------------------------------------------------

    pub(crate) fn open_curator_settings(&mut self) -> Option<SettingsPage> {
        self.curator.settings_sel = 0;
        Some(SettingsPage::Curator)
    }

    /// "On >" in the Settings list.
    pub(crate) fn curator_list_value(&self) -> String {
        let p = &self.curator.prefs;
        let on = [p.lens, p.echoes, p.up_next, p.merge_versions, p.stickers, p.history].iter().filter(|x| **x).count();
        if on == 0 {
            "Off >".to_string()
        } else {
            "On >".to_string()
        }
    }

    pub(crate) fn curator_settings_input(&mut self, b: Button) -> Option<SettingsPage> {
        let n = SETTINGS_ROWS.len();
        let sel = &mut self.curator.settings_sel;
        match b {
            Button::Up => *sel = (*sel + n - 1) % n,
            Button::Down => *sel = (*sel + 1) % n,
            b if b == CANCEL => return Some(SettingsPage::List(super::settings_row(SETTINGS_NAME))),
            b if b == CONFIRM || b == Button::Left || b == Button::Right => {
                let row = *sel;
                let s = switch(&mut self.curator.prefs, row);
                *s = !*s;
                if !self.curator.prefs.save(&self.data_dir) {
                    pocket_ui::ui_sound::refuse();
                }
                // The pause menu reads the flag, not the file (`app_pixel_path.rs`).
                self.pixel_path = self.curator.prefs.pixel_path;
                self.curator_counts();
                self.rebuild();
            }
            _ => {}
        }
        Some(SettingsPage::Curator)
    }

    pub(crate) fn paint_curator_settings(&mut self, c: &mut Canvas) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let sel = self.curator.settings_sel;
        for (i, (name, _)) in SETTINGS_ROWS.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            if i == sel {
                let w = self.text.width(Face::Body, font, name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, name);
            }
            let on = *switch(&mut self.curator.prefs.clone(), i);
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, pal().text, if on { "On" } else { "Off" });
        }
        self.hits.list(sel, SETTINGS_ROWS.len(), 0.0, row as f32);
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let mut y = top + SETTINGS_ROWS.len() as i32 * row + 44;
        for line in self.text.wrap(Face::Body, NU_SMALL, SETTINGS_ROWS[sel].1, width) {
            self.text.draw(c, Face::Body, NU_SMALL, left, y, pal().text, &line);
            y += pocket_ui::type_scale::line(NU_SMALL, 12);
        }
        y += 18;
        let note = "Everything Curator knows comes with pocket-ui and stays on the Pocket: it works without a network.";
        // Above the hints (with larger text it may be shortened with "…").
        let bottom = c.h - pad - NU_PILL - 16;
        self.text.paragraph(c, Face::Body, 24, (left, y), width, pocket_ui::type_scale::line(24, 8), bottom, pal().quiet, note);
        self.nu_hints(c, &[("B", "Back"), ("A", "Switch")]);
    }

    // ---- haptics, sounds, snapshots ---------------------------------------------------------------------------------

    /// Where Curator's pages stand, for the UI sounds and haptics.
    pub(crate) fn curator_mark(&self) -> (u8, usize, usize, usize, usize) {
        match &self.curator.page {
            None => (0, self.curator.settings_sel, 0, 0, 0),
            Some(Page::Lens(p)) => {
                let (f, v, col, picks) = p.mark();
                (1, f, v, col as usize, picks)
            }
            Some(Page::Echoes(p)) => (2, p.sel, p.elsewhere.len(), 0, 0),
            Some(Page::Versions(p)) => (3, p.sel, p.rows.iter().position(|r| r.preferred).unwrap_or(0), 0, 0),
            Some(Page::Stickers { sheet, year, .. }) => (4, sheet.sel, sheet.rows.iter().filter(|r| r.1 == Some(true)).count(), sheet.rows.len(), usize::from(*year)),
            Some(Page::Options { sheet, .. }) => (5, sheet.sel, 0, 0, 0),
            Some(Page::Order { sheet, .. }) => (9, sheet.sel, 0, 0, 0),
            Some(Page::Name(n)) => (6, n.kb.row * 16 + n.kb.col, n.kb.text.len(), n.kb.shift as usize, n.kb.symbols as usize),
            Some(Page::History(p)) => (7, p.sel, p.period.span as usize, p.period.first as usize, p.ready() as usize),
            Some(Page::Review(p)) => (8, p.card, p.period.first as usize, p.ready() as usize, 0),
        }
    }

    /// `--snapshot curator:<what>`: Curator's screens with the library of `POCKETUI_ES_SYSTEMS` and the player's files in
    /// `POCKETUI_DATA`; the index is built right away. `<what>`:
    /// `home[:<look>]` (Home on the Curator row), `hub[:<look>:<row>]` (on a smart shelf, or in that look on that row; 0 is the Lens), `shelf:<i>`, `options[:<keys>]`, `lens[:<keys>]` (keys r l u d a b x y s, as for
    /// `setup:`, and L R for L1 R1), `save[:<keys>]` (naming the shelf those picks make), `card:<system>:<name>[:<downs>]`, `echoes:<system>:<name>`,
    /// `versions:<system>:<name>`, `stickers:<system>:<name>`, `settings[:<row>]`, `options`, `ready[:lens]` (before the
    /// index is there), `surprise[:<seed>[:<n>]]` ("Surprise me"'s card); play history's `history…` and `review…`
    /// (`App::history_snapshot`). In the keys, `S` is SELECT (the Lens's order) and `s` START.
    pub(crate) fn curator_snapshot(&mut self, rest: &str) {
        std::env::set_var("POCKETUI_SNAPSHOT_POCKET_CORE", "1");
        self.curator_build(true, true);
        let parts: Vec<&str> = rest.split(':').collect();
        let what = parts.first().copied().unwrap_or("home");
        let game = |app: &App| -> Option<(usize, usize)> {
            let (sys, name) = (parts.get(1)?, parts.get(2)?);
            let s = app.systems.iter().position(|x| x.name == *sys)?;
            let g = app.systems[s].games.iter().position(|g| g.name.to_lowercase().contains(&name.to_lowercase()))?;
            Some((s, g))
        };
        let keys = |app: &mut App, keys: &str| {
            for k in keys.chars() {
                let b = match k {
                    'r' => Button::Right,
                    'l' => Button::Left,
                    'u' => Button::Up,
                    'd' => Button::Down,
                    'a' => CONFIRM,
                    'b' => CANCEL,
                    'x' => Button::North,
                    'y' => Button::West,
                    's' => Button::Start,
                    'S' => Button::Select,
                    'R' => Button::R1,
                    'L' => Button::L1,
                    _ => continue,
                };
                if app.curator.page.is_some() {
                    app.curator_input(b);
                } else {
                    app.press(b);
                }
            }
        };
        self.screen = Screen::Home;
        self.rebuild();
        // Covers (Home's panels, `app_home_panel`) are decoded on the spot, not on the loader's thread.
        self.pictures.sync = true;
        match what {
            // "home[:<look>[:<row>]]": Home in a look on the Curator row, or on `favorites`, the first pinned `shelf` or
            // `sticker`, or `nofavs` (Favorites whose games are all gone: the row as it would be with none).
            "home" => {
                if let Some(look) = parts.get(1) {
                    self.theme = look_named(look);
                    self.rebuild();
                }
                let row = parts.get(2).copied().unwrap_or("curator");
                if row == "nofavs" {
                    self.favorites.list.clear();
                    self.rebuild();
                    let at = self.items.iter().rposition(|i| matches!(i, Item::Recent(_))).map_or(Item::first_row(&self.items), |i| i + 1);
                    self.items.insert(at, Item::Favorites);
                }
                let want = |i: &Item| match row {
                    "favorites" | "nofavs" => *i == Item::Favorites,
                    "shelf" => matches!(i, Item::SmartShelf(_)),
                    "sticker" => matches!(i, Item::Sticker(_)),
                    _ => *i == Item::Curator,
                };
                if let Some(i) = self.items.iter().position(want) {
                    self.sel = i;
                    self.prev_sel = i;
                }
                self.moved_at = Instant::now() - std::time::Duration::from_secs(2);
            }
            "hub" | "options" | "shelf" => {
                self.curator_activate(Item::Curator);
                // "hub:<look>:<row>": the list in that look with that row selected (0 is the Lens).
                if let (Some(look), true) = (parts.get(1), what == "hub") {
                    self.theme = look_named(look);
                    self.sel = parts.get(2).and_then(|n| n.parse().ok()).unwrap_or(0).min(self.items.len().saturating_sub(1));
                    self.prev_sel = self.sel;
                } else if what == "shelf" {
                    let i: usize = parts.get(1).and_then(|n| n.parse().ok()).unwrap_or(0);
                    self.curator_activate(Item::SmartShelf(i));
                } else if let Some(i) = self.items.iter().position(|i| matches!(i, Item::SmartShelf(_))) {
                    self.sel = i;
                    self.prev_sel = i;
                    if what == "options" {
                        self.curator_x();
                        // `options:<keys>`: on in the sheet ("options:da" opens the shelf's order).
                        keys(self, parts.get(1).copied().unwrap_or(""));
                    }
                }
            }
            "lens" | "save" => {
                self.open_lens(Lens::default(), None);
                keys(self, parts.get(1).copied().unwrap_or(""));
                if what == "save" {
                    self.curator_input(Button::North);
                }
            }
            "ready" => {
                // As before the index is there: "Getting ready…".
                self.curator.index = None;
                self.rebuild();
                if parts.get(1) == Some(&"lens") {
                    self.open_lens(Lens::default(), None);
                } else if let Some(i) = self.items.iter().position(|i| *i == Item::Curator) {
                    self.sel = i;
                    self.prev_sel = i;
                }
            }
            "settings" => {
                self.settings_page = self.open_curator_settings();
                self.curator.settings_sel = parts.get(1).and_then(|n| n.parse().ok()).unwrap_or(0);
            }
            "card" | "echoes" | "versions" | "stickers" => {
                let Some((s, g)) = game(self) else {
                    eprintln!("curator:{rest}: no such game");
                    return;
                };
                let (system, name, path) = (self.systems[s].name.clone(), self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
                self.show_card(Item::Game(s, g), system, name, path.clone());
                match what {
                    "echoes" => {
                        self.open_echoes(&path);
                        if let Some(Page::Echoes(p)) = self.curator.page.as_mut() {
                            if self.curator.prefs.not_on_device {
                                p.elsewhere = vec!["Burnout Paradise".into(), "Black".into(), "Need for Speed: Most Wanted".into()];
                            }
                        }
                    }
                    "versions" => self.open_versions(&path),
                    // The keys after the game's name press on the sheet: `stickers:snes:chrono:a` puts Completed on.
                    "stickers" => {
                        self.open_sticker_sheet(&path);
                        keys(self, parts.get(3).copied().unwrap_or(""));
                    }
                    _ => {}
                }
            }
            // The Cabinet look: the cabinet after `<keys>` (r l u d, a b; `cabinet_snapshot`), or the Lens's games
            // as a shelf of their own after the Lens keys (`cabinetlens:<keys>`).
            "cabinet" => self.cabinet_snapshot(parts.get(1).copied().unwrap_or("")),
            "cabinetlens" => {
                self.theme = pocket_ui::settings::Theme::Cabinet;
                self.open_lens(Lens::default(), None);
                keys(self, parts.get(1).copied().unwrap_or(""));
                self.curator_input(Button::Start);
                self.cabinet_snapshot("");
            }
            // Up next on the shelf after `<system>:<name>` ended, in a look (`:<look>`, as for `home`).
            "upnext" => {
                let Some((s, g)) = game(self) else {
                    eprintln!("curator:{rest}: no such game");
                    return;
                };
                if let Some(look) = parts.get(3) {
                    self.theme = match *look {
                        "minimal" => pocket_ui::settings::Theme::Minimal,
                        "split" => pocket_ui::settings::Theme::Split,
                        "cards" => pocket_ui::settings::Theme::Cards,
                        "cabinet" => pocket_ui::settings::Theme::Cabinet,
                        _ => pocket_ui::settings::Theme::Shelf,
                    };
                }
                let path = self.systems[s].games[g].path.clone();
                let ix = self.curator.index.clone();
                match ix.as_ref().and_then(|ix| ix.find(&path)).and_then(|g| ix.as_ref()?.up_next(g)) {
                    Some(e) => {
                        self.offer_up_next(e.game, e.reason);
                        // Settled, as a second after it came in.
                        if let Some(u) = self.curator.up_next.as_mut() {
                            u.at = Instant::now() - std::time::Duration::from_secs(1);
                        }
                    }
                    None => eprintln!("curator:{rest}: nothing to suggest"),
                }
                if self.theme == pocket_ui::settings::Theme::Cabinet {
                    self.cabinet_snapshot("");
                }
            }
            // "Surprise me" from the Curator list with a fixed seed (`surprise[:<seed>[:<n>]]`, `n` more picks with Right), so
            // the snapshot shows the same game every time; `surprise:empty` before the index is there.
            "surprise" => {
                self.curator_activate(Item::Curator);
                if parts.get(1) == Some(&"empty") {
                    self.curator.index = None;
                } else {
                    self.curator.rng = Some(Rng::new(parts.get(1).and_then(|n| n.parse().ok()).unwrap_or(1)));
                    // The seed's games, not the day's pick (which changes with the date).
                    self.curator.today_shown = true;
                }
                if let Some(i) = self.items.iter().position(|i| *i == Item::Surprise) {
                    self.sel = i;
                    self.prev_sel = i;
                }
                self.curator_activate(Item::Surprise);
                for _ in 0..parts.get(2).and_then(|n| n.parse().ok()).unwrap_or(0) {
                    self.card_input(Button::Right);
                }
            }
            "history" | "history-empty" | "history-cutoff" | "history-mixed" | "history-hub" | "history-home" | "review" => self.history_snapshot(what, &parts),
            // Actual library replacement while a connection is open, then B (tests/connections.rs).
            "connection-rescan" => self.connection_rescan_snapshot(parts.get(1).copied().unwrap_or("ps2"), parts.get(2).copied().unwrap_or("revenge")),
            // "rediscover[:<look>]": Curator's Rediscover in a look, opened from the Curator list (`app_connections`).
            "rediscover" => {
                if let Some(look) = parts.get(1) {
                    self.theme = look_named(look);
                }
                self.curator_activate(Item::Curator);
                if let Some(i) = self.items.iter().position(|i| *i == Item::Rediscover) {
                    self.sel = i;
                    self.prev_sel = i;
                    self.curator_activate(Item::Rediscover);
                }
                if self.theme == pocket_ui::settings::Theme::Cabinet {
                    self.cabinet_snapshot("");
                }
            }
            _ => eprintln!("curator:{rest}: unknown"),
        }
    }

    /// After the first paint of a snapshot (which measures the card): `<downs>` presses of Down on the card, or keys
    /// (`y` Y, `d` Down, `u` Up, `r` Right, `l` Left, `a` A, `b` B: `card:ps2:revenge:yddddrr` is the status picker on the
    /// rating, two stars, `info:minimal:ps2:revenge:da` the list of the developer's games;
    /// `f` makes the open card's rows again as a new Curator index does, `c` as the sync conflicts' scan does).
    pub(crate) fn curator_snapshot_after(&mut self, rest: &str) {
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.first() == Some(&"card") {
            let arg = parts.get(3).copied().unwrap_or("");
            let keys: Vec<char> = match arg.parse::<usize>() {
                Ok(downs) => vec!['d'; downs],
                // Only a word of these keys ("battery=game" is the Battery line's, `battery_snapshot`).
                Err(_) if !arg.chars().all(|k| "ydrlfcuab".contains(k)) => Vec::new(),
                Err(_) => arg.chars().collect(),
            };
            for k in keys {
                let b = match k {
                    'y' => Button::West,
                    'd' => Button::Down,
                    'r' => Button::Right,
                    'l' => Button::Left,
                    'u' => Button::Up,
                    // A and B as the app takes them: A on a fact opens its list of games (Connections), B on that
                    // list comes back to the card.
                    'a' | 'b' => {
                        let b = if k == 'a' { CONFIRM } else { CANCEL };
                        if self.card.is_some() {
                            self.card_input(b);
                        } else {
                            self.press(b);
                        }
                        continue;
                    }
                    'f' => {
                        self.curator_card_refresh();
                        continue;
                    }
                    _ => {
                        self.conflicts_refresh();
                        continue;
                    }
                };
                if let Some(open) = self.card.as_mut() {
                    open.card.press(b);
                }
            }
        }
    }
}

/// The cabinet shelf of a Curator screen.
pub(crate) fn screen_shelf(screen: Screen) -> Option<ShelfKey> {
    Some(match screen {
        Screen::Curator => ShelfKey::Curator,
        Screen::LensResults => ShelfKey::Lens,
        Screen::Shelf(i) => ShelfKey::SmartShelf(i),
        Screen::Sticker(id) => ShelfKey::Sticker(id),
        Screen::Connection => ShelfKey::Connection,
        Screen::Rediscover => ShelfKey::Rediscover,
        _ => return None,
    })
}

/// The screen of one of Curator's cabinet shelves.
pub(crate) fn shelf_screen(key: ShelfKey) -> Option<Screen> {
    Some(match key {
        ShelfKey::Curator => Screen::Curator,
        ShelfKey::Lens => Screen::LensResults,
        ShelfKey::SmartShelf(i) => Screen::Shelf(i),
        ShelfKey::Sticker(id) => Screen::Sticker(id),
        ShelfKey::Connection => Screen::Connection,
        ShelfKey::Rediscover => Screen::Rediscover,
        _ => return None,
    })
}

/// A seed for "Surprise me" from the clock: another sequence each time pocket-ui starts.
fn clock_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

/// The Lens of a sticker's list.
fn sticker_lens(id: u32) -> Lens {
    let mut l = Lens::default();
    l.toggle(Facet::Stickers, &id.to_string());
    l
}

fn name_page(title: &str, name: &str, purpose: Purpose, back: Option<Page>) -> Page {
    Page::Name(Box::new(NameEntry { kb: Keyboard { text: name.to_string(), ..Default::default() }, title: title.to_string(), purpose, back: back.map(Box::new) }))
}

/// Save states of a file: pocket-core's slots and auto state, RetroArch's state.
fn save_states(system: &str, path: &Path) -> usize {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    let base = pocket_ui::slots::base(system, stem);
    let slots = pocket_ui::slots::filled(&base).iter().filter(|f| **f).count();
    let auto = pocket_ui::slots::auto_path(&base).exists() as usize;
    let retroarch = Path::new(pocket_ui::slots::STATES_DIR).join(system).join(format!("{stem}.state")).exists() as usize;
    slots + auto + retroarch
}

/// A thin bar at `x` from `top`, `h` tall, when a list has more rows than it shows.
fn scroll_bar(c: &mut Canvas, x: i32, top: i32, h: i32, first: usize, visible: usize, len: usize) {
    if len <= visible || len == 0 {
        return;
    }
    let bar_h = (h * visible as i32 / len as i32).max(40);
    let bar_y = top + (h - bar_h) * first as i32 / (len - visible).max(1) as i32;
    c.rect(x, top, 4, h, pal().spine);
    c.rect(x, bar_y, 4, bar_h, pal().dust);
}

/// A look by its snapshot name ("split"); the Shelf for anything else.
pub(crate) fn look_named(name: &str) -> pocket_ui::settings::Theme {
    use pocket_ui::settings::Theme;
    match name {
        "minimal" => Theme::Minimal,
        "split" => Theme::Split,
        "cards" => Theme::Cards,
        "cabinet" => Theme::Cabinet,
        _ => Theme::Shelf,
    }
}
