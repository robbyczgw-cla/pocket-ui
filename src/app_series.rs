//! Part of the pocket-ui binary: the shelf by series (Settings › Look › Layout › Order: By series; docs/SERIES-SHELF.md).
//! The arrangement is `pocket_ui::series_shelf`'s, made with Connections' index on Curator's thread; here Home takes
//! it in place of its systems (`Item::Series`), A opens a series' games (`Screen::Series`), X its card
//! (`pocket_ui::series_page`, in place of the console page), and the Split look and the Shelf look's label say what a
//! series is. Until the index is there, and in Easy and Kids Mode, Home is by console as before.

use std::path::PathBuf;

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::pal;
use pocket_ui::series_facts;
use pocket_ui::series_page::{self, Out, Page};
use pocket_ui::series_shelf::Shown;
use pocket_ui::settings::{ShelfOrder, Theme};
use pocket_ui::text::Face;

use super::{stripe, system_short, App, Item, Screen};

/// One shelf by series as the App keeps it: the games as (system, game) and their files, checked against the library
/// before use (a rescan moves indices, as Connections' lists).
#[derive(Clone, Debug)]
pub(crate) struct SeriesShelf {
    pub name: String,
    pub facts: Option<usize>,
    pub alone: bool,
    /// (system, game), file and release year.
    pub games: Vec<((usize, usize), PathBuf, Option<u16>)>,
}

/// What the App keeps for the shelf by series.
#[derive(Default)]
pub(crate) struct SeriesUi {
    /// Settings › Look › Layout › Order (settings.json's `shelf_order`): by console unless chosen.
    pub order: ShelfOrder,
    /// Home's shelves by series, made when Home is (`series_home`); `Item::Series` and `Screen::Series` count in it.
    pub shelves: Vec<SeriesShelf>,
    /// The index they were made from: a new one (Curator built again) makes them again.
    /// The series card open over the shelf (X on a series).
    pub page: Option<OpenSeries>,
    /// The selected series' facts for the Split look's panel, made once when the selection lands on it.
    panel: Option<(usize, series_page::Data)>,
    /// Covers the Layout page's previews by series asked for and had not yet: when the last is in, they are made again.
    preview_covers: std::collections::HashSet<pocket_ui::picture_loader::Key>,
}

impl SeriesUi {
    pub(crate) fn with_order(order: ShelfOrder) -> SeriesUi {
        SeriesUi { order, ..Default::default() }
    }
}

pub(crate) struct OpenSeries {
    pub page: Page,
    pub shelf: usize,
}

impl App {
    /// Home is by series: chosen, in a look that has shelves of systems (not Easy, not Kids Mode), and Connections'
    /// index is there (until then, a moment after the start, Home is by console: never an empty Home).
    pub(crate) fn series_on(&self) -> bool {
        self.series.order == ShelfOrder::Series && self.theme != Theme::Big && !self.kids_live() && !self.curator.connections.index.is_empty()
    }

    /// The Layout page's previews by series: Order is By series and the index is there, whatever the look in use (Easy
    /// keeps no shelves by series of its own); the shelves are made for them when the look had none.
    pub(crate) fn series_preview_ready(&mut self) -> bool {
        if self.series.order != ShelfOrder::Series || self.kids_live() || self.curator.connections.index.is_empty() {
            return false;
        }
        if self.series.shelves.is_empty() {
            self.series_arrange();
        }
        !self.series.shelves.is_empty()
    }

    /// After the Layout page's previews by series: the covers they asked for that were not decoded yet.
    pub(crate) fn series_previews_wait(&mut self) {
        self.series.preview_covers = self.pictures.visible.iter().filter(|k| !self.covers.contains_key(*k)).cloned().collect();
    }

    /// A cover came in from the picture loader: the previews are made again once all they waited for are here.
    pub(crate) fn series_cover_in(&mut self, key: &pocket_ui::picture_loader::Key) {
        if self.series.preview_covers.remove(key) && self.series.preview_covers.is_empty() {
            self.look_previews = None;
            self.request_redraw();
        }
    }

    /// Home's series in place of its systems (the shelves `series_make` made for this rebuild).
    pub(crate) fn series_home(&self, items: Vec<Item>) -> Vec<Item> {
        if !self.series_on() {
            return items;
        }
        let at = items.iter().position(|i| matches!(i, Item::System(_))).unwrap_or(items.len());
        let mut out: Vec<Item> = items.into_iter().filter(|i| !matches!(i, Item::System(_))).collect();
        let at = at.min(out.len());
        out.splice(at..at, (0..self.series.shelves.len()).map(Item::Series));
        out
    }

    /// Before Home's list is made: the shelves made again from the index with the systems and games the shelf shows
    /// now (Systems on the shelf, Shelf space). One pass over the games (`series_shelf::Arrangement::shown`).
    pub(crate) fn series_before_home(&mut self) {
        self.series_make();
    }

    /// Makes the shelves from the index. A series open on screen, on its card or remembered as Home's place stays the
    /// same series by name, wherever it now stands; one that is gone sends its screen Home and closes its card (a new
    /// series sorted in before it must not take its place under A).
    pub(crate) fn series_make(&mut self) {
        let old: Vec<String> = self.series.shelves.iter().map(|s| s.name.clone()).collect();
        if self.series_on() {
            self.series_arrange();
        } else {
            self.series.shelves.clear();
            self.series.panel = None;
        }
        self.series_remap(&old);
        // Home just became by series: the series' logos are fetched now (ScreenScraper's run, when it may run), not
        // only with the next run.
        if old.is_empty() && !self.series.shelves.is_empty() {
            self.screenscraper_series_appeared();
        }
    }

    /// After the shelves were made again from `old` (their names before): every place that remembers a series by its
    /// number follows it.
    fn series_remap(&mut self, old: &[String]) {
        if old.is_empty() {
            return;
        }
        // Old place -> new place, by name; None: the series is gone.
        let to: Vec<Option<usize>> = old.iter().map(|n| self.series.shelves.iter().position(|s| s.name == *n)).collect();
        let map = |i: usize| to.get(i).copied().flatten();
        if let Screen::Series(i) = self.screen {
            match map(i) {
                Some(n) => self.screen = Screen::Series(n),
                None => {
                    self.screen = Screen::Home;
                    self.sel = 0;
                }
            }
        }
        if let Some(Item::Series(i)) = self.home_item {
            self.home_item = map(i).map(Item::Series);
        }
        // The list on screen until it is made again: its selected row names the same series (`curator_event` looks it
        // up in the new list).
        for it in &mut self.items {
            if let Item::Series(i) = *it {
                *it = map(i).map_or(Item::Search, Item::Series);
            }
        }
        match self.series.page.as_ref().map(|o| map(o.shelf)) {
            Some(Some(n)) => self.series.page.as_mut().unwrap().shelf = n,
            Some(None) => self.series.page = None,
            None => {}
        }
        // The ways back that remember a series: Connections' lists and Curator's.
        self.connections_series_moved(&map);
        self.curator_series_moved(&map);
    }

    /// The library was read again: each shelf's games where the new library has their files (indices move).
    pub(crate) fn series_follow_library(&mut self, fresh: &[pocket_ui::library::System]) {
        if self.series.shelves.is_empty() {
            return;
        }
        let at: std::collections::HashMap<&std::path::Path, (usize, usize)> =
            fresh.iter().enumerate().flat_map(|(s, sys)| sys.games.iter().enumerate().map(move |(g, game)| (game.path.as_path(), (s, g)))).collect();
        let before: usize = self.series.shelves.iter().map(|s| s.games.len()).sum();
        for shelf in &mut self.series.shelves {
            shelf.games.retain_mut(|(place, path, _)| match at.get(path.as_path()) {
                Some(now) => {
                    *place = *now;
                    true
                }
                None => false,
            });
        }
        self.series.panel = None;
        // A game gone: its case may be on a series' cover.
        if self.series.shelves.iter().map(|s| s.games.len()).sum::<usize>() != before {
            self.cabinet.series_changed();
        }
    }

    fn series_arrange(&mut self) {
        let ix = self.curator.connections.index.clone();
        // Where each game of the index is in the library now: its place when the index was made, or (after a rescan,
        // before the next index) found by its file.
        let moved: std::cell::OnceCell<std::collections::HashMap<&std::path::Path, (usize, usize)>> = std::cell::OnceCell::new();
        let place = |g: &pocket_ui::connections::Game| -> Option<(usize, usize)> {
            let (s, i) = g.at;
            if self.systems.get(s).and_then(|sys| sys.games.get(i)).is_some_and(|game| game.path == g.path) {
                return Some(g.at);
            }
            let map = moved.get_or_init(|| {
                self.systems.iter().enumerate().flat_map(|(s, sys)| sys.games.iter().enumerate().map(move |(i, game)| (game.path.as_path(), (s, i)))).collect()
            });
            map.get(g.path.as_path()).copied()
        };
        let keep = |g: &pocket_ui::connections::Game| {
            place(g).is_some_and(|(s, i)| !self.hidden.contains(&self.systems[s].name) && self.shelf_space.on_shelf(&self.systems[s].games[i].path))
        };
        let shelves: Vec<Shown> = ix.series_shelves(keep);
        let shelves: Vec<SeriesShelf> = shelves
            .into_iter()
            .map(|s| SeriesShelf {
                name: s.name,
                facts: s.facts,
                alone: s.alone,
                games: s.games.iter().filter_map(|&i| Some((place(&ix.games[i])?, ix.games[i].path.clone(), ix.games[i].year))).collect(),
            })
            .collect();
        // Home is made again often (back from a game, a favourite): the series' covers, logos, spines and fronts are made
        // again only when a series or its games changed (`app_series_look`; Sol's recheck).
        let same = self.series.shelves.len() == shelves.len()
            && self.series.shelves.iter().zip(&shelves).all(|(a, b)| a.name == b.name && a.games.len() == b.games.len() && a.games.iter().zip(&b.games).all(|(x, y)| x.1 == y.1));
        self.series.shelves = shelves;
        if !same {
            self.cabinet.series_changed();
        }
        // A series' name is painted as it is, whatever the tables have for its words ("The Legend of Zelda").
        pocket_ui::i18n::learn_names(self.series.shelves.iter().filter(|s| !s.alone).map(|s| s.name.as_str()));
        self.series.panel = None;
    }

    /// A series' games as the library has them now.
    pub(crate) fn series_items(&self, i: usize) -> Vec<Item> {
        self.series_games(i).collect()
    }

    /// `series_items` without making the list (a panel's first covers of a shelf of thousands, every frame).
    pub(crate) fn series_games(&self, i: usize) -> impl Iterator<Item = Item> + '_ {
        self.series
            .shelves
            .get(i)
            .into_iter()
            .flat_map(|shelf| shelf.games.iter())
            .filter(|((s, g), p, _)| self.systems.get(*s).and_then(|sys| sys.games.get(*g)).is_some_and(|game| game.path == *p))
            .map(|((s, g), _, _)| Item::Game(*s, *g))
    }

    /// A series' games in the order its cover and logo take them: its key games first, then by release year
    /// (`series_look::ranked`).
    pub(crate) fn series_ranked(&self, i: usize) -> Vec<Item> {
        let items = self.series_items(i);
        let names: Vec<&str> = items.iter().map(|it| if let Item::Game(s, g) = *it { self.systems[s].games[g].name.as_str() } else { "" }).collect();
        let key_games = self.series.shelves.get(i).and_then(|s| s.facts).map_or(&[][..], |f| series_facts::ALL[f].key);
        pocket_ui::series_look::ranked(&names, key_games).into_iter().map(|k| items[k]).collect()
    }

    /// The order a series' logo is looked for in (`series_look::logo_order`): the games named as the series first
    /// ("Metroid", "The Legend of Zelda"), then the ranked ones.
    pub(crate) fn series_logo_order(&self, i: usize) -> Vec<Item> {
        let ranked = self.series_ranked(i);
        let Some(shelf) = self.series.shelves.get(i) else { return ranked };
        let names: Vec<&str> = ranked.iter().map(|it| if let Item::Game(s, g) = *it { self.systems[s].games[g].name.as_str() } else { "" }).collect();
        let mut series = vec![shelf.name.as_str()];
        if let Some(f) = shelf.facts {
            series.extend(series_facts::ALL[f].aliases.iter().copied());
        }
        let order = pocket_ui::series_look::logo_order(&(0..ranked.len()).collect::<Vec<_>>(), &names, &series);
        order.into_iter().map(|k| ranked[k]).collect()
    }

    /// For ScreenScraper's logo pass (`app_screenscraper`): each series' first games in logo order as (system, ROM); none
    /// while Home is not by series or Real spines is Off.
    pub(crate) fn series_logo_candidates(&self) -> Vec<Vec<(String, PathBuf)>> {
        // Logos off (`series_look::SERIES_LOGOS`, 0.8): no game, so the logo pass sends nothing.
        if !pocket_ui::series_look::SERIES_LOGOS || !self.series_on() || !self.screenscraper_shows(pocket_ui::screenscraper::Face::Wheel) {
            return Vec::new();
        }
        let game = |it: Item| if let Item::Game(s, g) = it { Some((self.systems[s].name.clone(), self.systems[s].games[g].path.clone())) } else { None };
        (0..self.series.shelves.len())
            .filter(|&i| !self.series.shelves[i].alone)
            .map(|i| self.series_logo_order(i).into_iter().take(pocket_ui::series_look::LOGO_LOOK).filter_map(game).collect())
            .collect()
    }

    /// A on a series: its games, as A on a system opens the system's.
    pub(crate) fn open_series(&mut self, i: usize) {
        self.series.page = None;
        self.home_item = Some(Item::Series(i));
        self.screen = Screen::Series(i);
        self.sel = 0;
        self.rebuild();
    }

    /// A panel's tile of a game on series shelf `i`: its title without the series (`series_shelf::short_title`).
    pub(crate) fn series_tile(&self, i: usize, mut tile: pocket_ui::home_panel::Tile, at: (usize, usize)) -> pocket_ui::home_panel::Tile {
        let Some(shelf) = self.series.shelves.get(i).filter(|s| !s.alone) else { return tile };
        let mut names = vec![shelf.name.as_str()];
        if let Some(f) = shelf.facts {
            names.extend(series_facts::ALL[f].aliases.iter().copied());
        }
        let year = shelf.games.iter().find(|(g, _, _)| *g == at).and_then(|(_, _, y)| *y);
        tile.title = pocket_ui::series_shelf::short_title(&names, &tile.title, year);
        tile
    }

    /// (title, stripe id, the spine's right text, the label's line) of a series, as `describe` gives a system's.
    pub(crate) fn series_describe(&self, i: usize) -> (String, String, String, String) {
        let Some(shelf) = self.series.shelves.get(i) else { return (String::new(), "series".into(), String::new(), String::new()) };
        let n = shelf.games.len();
        let count = pocket_ui::consoles::games_text(n);
        let line = match (shelf.alone, shelf.facts.map(|f| &series_facts::ALL[f])) {
            (true, _) if n == 1 => "1 game that stands alone".to_string(),
            (true, _) => format!("{n} games that stand alone"),
            (false, Some(f)) => [count.clone(), f.by.to_string(), format!("since {}", f.since)].join(" · "),
            (false, None) => count.clone(),
        };
        (shelf.name.clone(), self.series_stripe_id(i), count, line)
    }

    /// The stripe of a series: its first game's console (a Nintendo series stands in Nintendo's colours), One of a
    /// kind Curator's.
    fn series_stripe_id(&self, i: usize) -> String {
        let shelf = &self.series.shelves[i];
        if shelf.alone {
            return "curator".into();
        }
        shelf.games.first().and_then(|((s, _), _, _)| self.systems.get(*s)).map_or_else(|| "curator".into(), |s| s.name.clone())
    }

    /// The console id of a series' first game, while the library has it there.
    pub(crate) fn series_first_system(&self, i: usize) -> Option<&str> {
        let shelf = self.series.shelves.get(i).filter(|s| !s.alone)?;
        shelf.games.first().and_then(|((s, _), _, _)| self.systems.get(*s)).map(|s| s.name.as_str())
    }

    /// The series card's facts from the shelf: the player's games of it and which key games they have.
    fn series_data(&self, i: usize) -> Option<series_page::Data> {
        let shelf = self.series.shelves.get(i)?;
        let facts = shelf.facts.map(|f| &series_facts::ALL[f]);
        let games: Vec<series_page::Game> = shelf
            .games
            .iter()
            .filter_map(|((s, g), p, year)| {
                let sys = self.systems.get(*s)?;
                let game = sys.games.get(*g).filter(|game| game.path == *p)?;
                Some(series_page::Game { name: game.name.clone(), year: *year, system: system_short(&self.systems, &sys.name) })
            })
            .collect();
        let title_key = |s: &str| pocket_ui::connections::key(pocket_ui::connections::Kind::Series, s);
        let owned: Vec<String> = games.iter().map(|g| title_key(&g.name)).collect();
        let key_owned = facts.map_or(Vec::new(), |f| f.key.iter().map(|k| owned.contains(&title_key(k))).collect());
        Some(series_page::Data {
            name: shelf.name.clone(),
            facts,
            alone: shelf.alone,
            stripe: stripe(&self.series_stripe_id(i)),
            games,
            key_owned,
            no_series: shelf.alone && self.series.shelves.iter().all(|s| s.alone),
        })
    }

    /// X on a series: its card.
    pub(crate) fn open_series_card(&mut self, i: usize) {
        if let Some(data) = self.series_data(i) {
            self.series.page = Some(OpenSeries { page: Page::new(data), shelf: i });
        }
    }

    pub(crate) fn series_card_open(&self) -> bool {
        self.series.page.is_some()
    }

    pub(crate) fn series_card_input(&mut self, b: Button) {
        let Some(open) = self.series.page.as_mut() else { return };
        match open.page.press(b) {
            Out::None => {}
            Out::Back => self.series.page = None,
            Out::Games => {
                let i = open.shelf;
                self.open_series(i);
            }
        }
    }

    pub(crate) fn paint_series_card(&mut self, c: &mut Canvas) {
        let corner = self.status_corner();
        let look = if self.theme == Theme::Split { pocket_ui::console_page::Look::split() } else { pocket_ui::console_page::Look::plain() };
        let Some(open) = self.series.page.as_mut() else { return };
        let hits = open.page.paint(c, &mut self.text, Some(&corner), look);
        for (area, b) in hits.buttons {
            self.hits.button(area, b);
        }
        if let Some((row, rows, pitch)) = hits.scroll {
            self.hits.list(row, rows, 0.0, pitch);
        }
    }

    /// The Split look's room under a selected series' name, as `paint_system_facts` for a system: the maker and year,
    /// how many of it the player has, the consoles. False for anything that is not a series.
    pub(crate) fn paint_series_facts(&mut self, c: &mut Canvas, item: Item, x: i32, y: i32, w: i32, bottom: i32) -> bool {
        let Item::Series(i) = item else { return false };
        if self.series.panel.as_ref().is_none_or(|(at, _)| *at != i) {
            self.series.panel = self.series_data(i).map(|d| (i, d));
        }
        let Some((_, d)) = self.series.panel.take() else { return false };
        let painted = self.paint_series_panel(c, &d, x, y, w, bottom);
        self.series.panel = Some((i, d));
        painted
    }

    fn paint_series_panel(&mut self, c: &mut Canvas, d: &series_page::Data, x: i32, y: i32, w: i32, bottom: i32) -> bool {
        let head = self.text.fit(Face::Body, 28, &d.headline(), w);
        self.text.draw(c, Face::Body, 28, x, y, pal().split_dim, &head);
        let mut rows = Vec::new();
        if !d.alone {
            rows.push(pocket_ui::consoles::Row { label: "You have", values: vec![d.yours()], keep: 0 });
            if let Some((a, b)) = d.years() {
                rows.push(pocket_ui::consoles::Row { label: "Years", values: vec![if a == b { a.to_string() } else { format!("{a} to {b}") }], keep: 1 });
            }
            // The whole list, then shorter ones ("NES, SNES, N64 +2") for a narrow panel (the Brick's "NES, SNES, N64, GBA, Ga…").
            let all = d.consoles();
            if !all.is_empty() {
                let values = (1..=all.len()).rev().map(|n| if n == all.len() { all.join(", ") } else { format!("{} +{}", all[..n].join(", "), all.len() - n) }).collect();
                rows.push(pocket_ui::consoles::Row { label: "Consoles", values, keep: 2 });
            }
        }
        if rows.is_empty() {
            for (k, line) in self.text.wrap_prose_at_most(Face::Body, 28, d.about(), w, 3).iter().enumerate() {
                self.text.draw(c, Face::Body, 28, x, y + 60 + k as i32 * pocket_ui::type_scale::line(28, 12), pal().split_dim, line);
            }
            return true;
        }
        pocket_ui::console_page::paint_facts(c, &mut self.text, &rows, x, y + 60, w, bottom, pal().dust, pal().chalk);
        true
    }

    /// `--snapshot series[:<look>][:<series>|:alone][:card|:games]`: Home by series in that look (Split by default) with Curator's
    /// index there, the series whose name starts with `<series>` (or One of a kind) selected; `card` opens its card,
    /// `games` its shelf.
    pub(crate) fn series_snapshot(&mut self, rest: &str) {
        let mut parts = rest.split(':').filter(|p| !p.is_empty());
        let look = parts.next().unwrap_or("split");
        self.shelf_snapshot(&format!(":{look}"));
        self.series.order = ShelfOrder::Series;
        self.curator_build(true, true);
        self.screen = Screen::Home;
        self.rebuild();
        // What Home holds, for tests/series_shelf.rs: each shelf with its games (system/name) in order, and the rows.
        for (i, shelf) in self.series.shelves.iter().enumerate() {
            let games: Vec<String> = self.series_items(i).into_iter().filter_map(|it| match it {
                Item::Game(s, g) => Some(format!("{}/{}", self.systems[s].name, self.systems[s].games[g].name)),
                _ => None,
            }).collect();
            println!("series-shelf {}|{}|{}", shelf.name, shelf.alone, games.join(";"));
        }
        let rows = |f: fn(&Item) -> bool| self.items.iter().filter(|i| f(i)).count();
        println!("home-rows systems={} series={} all={}", rows(|i| matches!(i, Item::System(_))), rows(|i| matches!(i, Item::Series(_))), self.items.len());
        // `series:<look>:previews`: the Layout page's previews made from Easy (which keeps no shelves by series), by series.
        if rest.split(':').any(|p| p == "previews") {
            self.theme = Theme::Big;
            self.rebuild();
            let _ = self.render_look_previews();
            return;
        }
        // `series:<look>::layout`: Settings › Look › Layout with Curator's index there, its previews by series.
        if rest.split(':').any(|p| p == "layout") {
            self.setup = Some(self.new_setup().only(pocket_ui::setup::Only::Look));
            return;
        }
        let want = parts.next();
        let at = want.and_then(|w| {
            self.series.shelves.iter().position(|s| if w == "alone" { s.alone } else { s.name.to_lowercase().starts_with(&w.to_lowercase()) })
        });
        let at = at.or_else(|| (!self.series.shelves.is_empty()).then_some(0));
        if let Some(i) = at {
            if let Some(pos) = self.items.iter().position(|it| *it == Item::Series(i)) {
                self.sel = pos;
                self.prev_sel = pos;
            }
            let next = parts.next();
            match next {
                Some("card") => self.open_series_card(i),
                Some("games") | Some("moved") => self.open_series(i),
                _ => {}
            }
            // `:moved`: from the series' games, a game's card, its first Connections list; then a series sorted in before
            // every other (as a new index brings one) and B back: the same series, not the one now at its old place
            // (Sol's review 26.11.2026; tests/series_shelf.rs).
            if next == Some("moved") {
                let before = self.series.shelves[i].name.clone();
                self.open_card();
                if self.card.as_ref().is_some_and(|c| !c.card.data.links.is_empty()) {
                    self.open_connection(0);
                }
                let old: Vec<String> = self.series.shelves.iter().map(|s| s.name.clone()).collect();
                self.series.shelves.insert(0, SeriesShelf { name: "Aaa first".into(), facts: None, alone: false, games: Vec::new() });
                self.series_remap(&old);
                let back = self.connection_back();
                let screen = match self.screen { Screen::Series(n) => self.series.shelves.get(n).map_or("-", |s| s.name.as_str()).to_string(), _ => "-".into() };
                self.card = None;
                self.press(super::CANCEL);
                let name = |it: Option<Item>| match it { Some(Item::Series(n)) => self.series.shelves.get(n).map_or("-".to_string(), |s| s.name.clone()), _ => "-".into() };
                println!("series-moved before={before} back={back} screen={screen} home={} selected={}", name(self.home_item), name(self.items.get(self.sel).copied()));
            }
        }
        self.moved_at = std::time::Instant::now() - std::time::Duration::from_secs(2);
    }
}
