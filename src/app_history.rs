//! Part of the pocket-ui binary: play history (docs/HISTORY.md). The log is `pocket_ui::play_log` (recorded off
//! this thread), the numbers `pocket_ui::play_history`, the pages' presses and painting `pocket_ui::history_page`;
//! this is the App's side:
//! - after the first frame: the log's writer thread starts, and the totals from before are handed over once;
//! - the History and "Year in review" rows of the Curator list (with the Lens off, History stands on Home once
//!   something was played). The review never opens by itself and has no row on Home: only the player opens it;
//! - the History page and the review as Curator pages. Their numbers are counted on a thread of their own
//!   (`count`), never on this one: the page shows its title at once and the numbers a moment later
//!   (`UserEvent::Curator(Event::History(_))`). The genres and merged versions come from Curator's index, the
//!   "You might like next" card from Echoes; A opens a game's card from either page, and B on it comes back;
//! - `--snapshot curator:history…` with made-up history (counted right here, as a snapshot has one frame).
//!
//! main.rs carries three one-line hooks (a game's start and end, `play_log::settle` at exit) and the Item variants.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::curator::features::Axis;
use pocket_ui::curator::Index;
use pocket_ui::history_page::{self as hp, Hit, HistoryPage, Out, ReviewOut, ReviewPage};
use pocket_ui::pad::Button;
use pocket_ui::play_history::{self as ph, Card, Describe, Genre, GenreKind, Period, Span};
use pocket_ui::play_log::{self, Game, Log};

use super::app_curator::{Event, Page};
use super::{system_label, system_short, App, Item, UserEvent};

/// What the App keeps for play history.
#[derive(Default)]
pub(crate) struct HistoryUi {
    /// A History or review page put aside while a game's card is open from it: B on the card comes back to it.
    pub aside: Option<(String, Page)>,
    /// The History page a review was opened from (Y): B on the review goes back to it.
    pub review_from: Option<Box<HistoryPage>>,
    /// Snapshots: the moment the pages take as now.
    pub now: Option<u64>,
    /// Snapshots: count right here, not on a thread (a snapshot paints one frame).
    pub here: bool,
    /// The log was there at the last tick (Home's rows depend on it).
    seen_log: bool,
}

/// What a thread counted for a page; a page takes it only if it still shows that period (`HistoryPage::set`).
pub(crate) enum Counted {
    Summary(Box<ph::Summary>),
    Review(Box<ph::Review>),
}

/// What to count.
#[derive(Clone, Copy)]
enum Ask {
    Summary(Period),
    Review(Period),
}

/// Curator's tags and versions as the history wants them (the index is shared, not copied).
struct Tags {
    index: Option<Arc<Index>>,
    merge: bool,
}

impl Describe for Tags {
    fn genres(&self, game: &Game) -> Vec<Genre> {
        let Some(ix) = self.index.as_deref() else { return Vec::new() };
        let Some(i) = ix.find(Path::new(&game.path)) else { return Vec::new() };
        ix.game(i)
            .terms
            .iter()
            .filter_map(|t| {
                let kind = match t.axis? {
                    Axis::Family => GenreKind::Family,
                    Axis::SubGenre => GenreKind::SubGenre,
                    Axis::Mechanic => GenreKind::Mechanic,
                    _ => return None,
                };
                Some(Genre { label: t.label.clone(), kind, weight: t.weight })
            })
            .collect()
    }

    fn name(&self, game: &Game) -> Option<String> {
        let ix = self.index.as_deref()?;
        Some(ix.game(ix.find(Path::new(&game.path))?).name.clone())
    }

    fn work(&self, game: &Game) -> Option<String> {
        let ix = self.index.as_deref().filter(|_| self.merge)?;
        ix.game(ix.find(Path::new(&game.path))?).work.clone()
    }
}

/// A period's review with "You might like next": Up next's pick for the top game (Echoes on).
fn make_review(log: &Log, period: Period, today: i64, tags: &Tags, echoes: bool) -> ph::Review {
    let mut r = ph::review(log, period, today, tags);
    let ix = tags.index.as_deref().filter(|_| echoes);
    let next = r.summary.games.first().zip(ix).and_then(|(top, ix)| {
        let g = ix.find(Path::new(&top.path))?;
        let e = ix.up_next(g)?;
        let f = ix.game(e.game);
        Some(Card::Next { name: f.name.clone(), system: f.system.clone(), path: f.path.to_string_lossy().into_owned(), reason: e.reason })
    });
    r.cards.extend(next);
    r
}

/// A recent game, as the migration needs it.
struct SeedRecent {
    path: String,
    name: String,
    system: String,
    secs: u64,
    last_played: u64,
}

/// The migration's seed (on the writer's thread): every game library.json counted time or sessions for, and every
/// recent game. Names and systems from the recent games, else Curator's index, else the file's own name and folder
/// (a game no longer in the library). Nothing played: an empty seed, which only marks the log as migrated.
fn seed_of(played: Vec<(String, u64, u32)>, recents: Vec<SeedRecent>, index: Option<&Index>, now: u64) -> play_log::Seed {
    let mut games: std::collections::BTreeMap<String, play_log::SeedGame> = std::collections::BTreeMap::new();
    for (path, secs, sessions) in played {
        games.insert(path.clone(), play_log::SeedGame { path, secs, sessions, ..Default::default() });
    }
    for r in recents {
        let g = games.entry(r.path.clone()).or_insert_with(|| play_log::SeedGame { path: r.path.clone(), ..Default::default() });
        (g.name, g.system) = (r.name, r.system);
        g.secs = g.secs.max(r.secs);
        g.last_played = Some(r.last_played);
    }
    for g in games.values_mut().filter(|g| g.name.is_empty()) {
        let p = Path::new(&g.path);
        (g.name, g.system) = match index.and_then(|ix| ix.find(p)).zip(index) {
            Some((i, ix)) => (ix.game(i).name.clone(), ix.game(i).system.clone()),
            // A game no longer in the library: its folder is its system, its file its name.
            None => {
                let system = p.parent().and_then(|d| d.file_name()).map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
                (pocket_ui::library::shelf_name(&p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), system)
            }
        };
    }
    println!("play log: taking over the play time of {} games from before", games.len());
    play_log::Seed { games: games.into_values().collect(), now }
}

impl App {
    /// Now, and the local day it is (the device's clock and zone; a snapshot's own moment).
    fn history_now(&self) -> (u64, i32, i64) {
        let now = self.curator.history.now.unwrap_or_else(pocket_ui::recents::now_secs);
        let tz = pocket_ui::appearance::utc_offset_now(now as i64);
        (now, tz, ph::local_day(now, tz))
    }

    /// From `curator_tick`: Play history's switch, the writer after the first frame, the one migration.
    pub(crate) fn history_tick(&mut self) {
        let on = self.curator.prefs.history;
        if play_log::enabled() != on {
            play_log::set_enabled(on);
        }
        if !on || !self.drew_first || self.setup.is_some() {
            return;
        }
        play_log::open();
        // The one migration: never while a game starts or runs (its loading has the card). Only a copy of what was
        // played is made here; the seed is put together on the writer's thread (review SOL-2026-10-24 #7).
        if self.running.is_none() && play_log::wants_seed() {
            self.history_seed();
        }
        // The log just came in (moments after the first frame): the History rows of Home (Lens off) or of the
        // Curator list open now may have something to show. Once; only when they do.
        let seen = play_log::view().is_some();
        if seen && !self.curator.history.seen_log {
            self.curator.history.seen_log = true;
            let rows_here = (self.screen == super::Screen::Home && !self.curator.prefs.lens) || self.screen == super::Screen::Curator;
            if rows_here && self.running.is_none() && self.history_shown() {
                let item = self.items.get(self.sel).copied();
                self.rebuild();
                if let Some(at) = item.and_then(|it| self.items.iter().position(|i| *i == it)) {
                    self.sel = at;
                    self.prev_sel = at;
                }
            }
        }
        // A card opened from History that was left by playing the game: nothing to come back to.
        if self.running.is_some() {
            self.curator.history.aside = None;
        }
    }

    /// The totals pocket-ui kept before the log: library.json's time and sessions, the recent games' last play.
    /// Once in a device's life (the first start with Play history), after the first frame. Here, on the UI thread:
    /// a copy of the played games of library.json (not the whole file) and of the recent games (a short list), and an
    /// `Arc` clone of Curator's index; the seed is made on the writer's thread (`seed_of`). No library scan.
    fn history_seed(&self) {
        let played = self.library.played();
        let recents: Vec<SeedRecent> = self.recents.list.iter().map(|r| SeedRecent { path: r.path.to_string_lossy().into_owned(), name: r.name.clone(), system: r.system.clone(), secs: r.play_secs, last_played: r.last_played }).collect();
        let index = self.curator.index.clone();
        play_log::seed_with(Box::new(move || seed_of(played, recents, index.as_deref(), pocket_ui::recents::now_secs())));
    }

    fn tags(&self) -> Tags {
        Tags { index: self.curator.index.clone(), merge: self.curator.prefs.merge_versions }
    }

    /// Counts a period on a thread of its own and hands it to the page (`history_counted`). What it needs is
    /// gathered here (two `Arc` clones and three flags), the log is read and counted there. Snapshots count right here
    /// and get the result back.
    fn count(&mut self, ask: Ask, today: i64) -> Option<Counted> {
        let tags = self.tags();
        let echoes = self.curator.prefs.echoes;
        let job = move || {
            let t = Instant::now();
            // A page opened in the first moments waits here (not on the UI thread) for the writer to have read the file.
            let log = play_log::view().or_else(|| play_log::view_within(Duration::from_secs(2))).unwrap_or_default();
            let counted = match ask {
                Ask::Summary(period) => Counted::Summary(Box::new(ph::summarise(&log, period, today, &tags))),
                Ask::Review(period) => Counted::Review(Box::new(make_review(&log, period, today, &tags, echoes))),
            };
            let what = match &counted {
                Counted::Summary(s) => s.label.clone(),
                Counted::Review(r) => format!("review of {} ({} cards)", r.title, r.cards.len()),
            };
            println!("history: {what} from {} sessions in {:.1} ms", log.sessions.len(), t.elapsed().as_secs_f64() * 1000.0);
            counted
        };
        if self.curator.history.here {
            return Some(job());
        }
        let proxy = self.proxy.clone();
        let spawned = std::thread::Builder::new().name("history".into()).spawn(move || {
            let _ = proxy.send_event(UserEvent::Curator(Event::History(job())));
        });
        if let Err(e) = spawned {
            eprintln!("history: no thread ({e})");
        }
        None
    }

    /// Counted numbers for the page open now (a page that moved on to another period drops them).
    pub(crate) fn history_counted(&mut self, c: Counted) {
        match (c, self.curator.page.as_mut()) {
            (Counted::Summary(s), Some(Page::History(p))) => {
                p.set(*s);
            }
            (Counted::Review(r), Some(Page::Review(p))) => {
                p.set(*r);
            }
            _ => return,
        }
        self.request_redraw();
    }

    // ---- rows ---------------------------------------------------------------------------------------------------

    /// The review row of the Curator list: this year's, last year's in January.
    pub(crate) fn review_year(&self) -> i32 {
        let (_, _, today) = self.history_now();
        let (y, m, _) = ph::civil(today);
        if m == 1 {
            y - 1
        } else {
            y
        }
    }

    /// History and the Year in review have rows once something was played (Play history on): like Curator's other
    /// parts, they take no room before they have something to show.
    pub(crate) fn history_shown(&self) -> bool {
        self.curator.prefs.history && play_log::view().is_some_and(|l| !l.sessions.is_empty())
    }

    /// History on Home when the Lens (and with it the Curator row) is off.
    pub(crate) fn history_on_home(&self) -> bool {
        !self.curator.prefs.lens && self.history_shown()
    }

    /// (title, stripe id, text at the spine's end, meta line) of the History rows.
    pub(crate) fn history_describe(&self, item: Item) -> (String, String, String, String) {
        let (now, tz, today) = self.history_now();
        match item {
            Item::History => {
                // Only this week's sessions are looked at, from the newest back: a few dozen at most.
                let week = Period::of(Span::Week, today);
                let since = (week.first * ph::DAY - tz as i64).max(0) as u64;
                // A session without a recorded end has no time to add: said, not shown as "under a minute".
                let side = ph::week_side(play_log::view().map(|l| ph::played_since(&l, since, now)));
                ("History".into(), "curator".into(), side, "What you played this week, month and year".into())
            }
            Item::Review(year) => {
                let (_, m, _) = ph::civil(today);
                let meta = match m {
                    12 | 1 => format!("Your {year}: the games, the hours, the streaks"),
                    _ => format!("Your {year} so far, card by card"),
                };
                ("Year in review".into(), "curator".into(), year.to_string(), meta)
            }
            _ => Default::default(),
        }
    }

    /// A on History or the review row.
    pub(crate) fn history_activate(&mut self, item: Item) {
        match item {
            Item::History => self.open_history(Span::Week),
            Item::Review(year) => self.open_review(Period::of(Span::Year, ph::day_of(year, 6, 1)), None),
            _ => {}
        }
    }

    // ---- pages -----------------------------------------------------------------------------------------------------

    /// Opens History on the running week (month, year): the title at once, the numbers from the thread.
    pub(crate) fn open_history(&mut self, span: Span) {
        let (now, tz, today) = self.history_now();
        let period = Period::current(span, now, tz);
        self.curator.page = Some(Page::History(Box::new(HistoryPage::new(period, today))));
        if let Some(c) = self.count(Ask::Summary(period), today) {
            self.history_counted(c);
        }
    }

    /// The review of a period; `from`: the History page it was opened from (B goes back to it).
    pub(crate) fn open_review(&mut self, period: Period, from: Option<Box<HistoryPage>>) {
        let (_, _, today) = self.history_now();
        self.curator.history.review_from = from;
        self.curator.page = Some(Page::Review(Box::new(ReviewPage::new(period, today))));
        if let Some(c) = self.count(Ask::Review(period), today) {
            self.history_counted(c);
        }
    }

    /// The library's item for a file, if it is still there.
    fn history_item(&self, path: &str) -> Option<Item> {
        self.systems.iter().enumerate().find_map(|(s, sys)| sys.games.iter().position(|g| g.path.as_os_str() == path).map(|g| Item::Game(s, g)))
    }

    /// A game's card from History or the review; the page waits aside until the card is closed. A game that was
    /// deleted or renamed since has no card: a toast says so and the page stays.
    fn history_card(&mut self, path: &str, page: Page) -> Option<Page> {
        let Some(item @ Item::Game(s, g)) = self.history_item(path) else {
            self.toast = Some(("Not on this Pocket any more".into(), Instant::now()));
            pocket_ui::ui_sound::refuse();
            return Some(page);
        };
        let (system, name, p) = (self.systems[s].name.clone(), self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
        self.curator.history.aside = Some((path.to_string(), page));
        self.show_card(item, system, name, p);
        None
    }

    /// The card closed (B): back to the History page or review it was opened from.
    pub(crate) fn history_card_closed(&mut self, path: &Path) {
        if self.curator.history.aside.as_ref().is_some_and(|(p, _)| Path::new(p) == path) {
            self.curator.page = self.curator.history.aside.take().map(|(_, page)| page);
        }
    }

    /// A press on History.
    pub(crate) fn history_input(&mut self, mut p: Box<HistoryPage>, b: Button) -> Option<Page> {
        match p.press(b) {
            Out::None => Some(Page::History(p)),
            Out::Back => None,
            Out::Card(i) => {
                let Some(path) = p.summary.as_ref().and_then(|s| s.games.get(i)).map(|g| g.path.clone()) else { return Some(Page::History(p)) };
                self.history_card(&path, Page::History(p))
            }
            Out::Period(period) => {
                if let Some(Counted::Summary(s)) = self.count(Ask::Summary(period), p.today) {
                    p.set(*s);
                }
                Some(Page::History(p))
            }
            Out::Review => {
                let period = p.period;
                self.open_review(period, Some(p));
                self.curator.page.take()
            }
        }
    }

    /// A press on the review.
    pub(crate) fn review_input(&mut self, mut p: Box<ReviewPage>, b: Button) -> Option<Page> {
        match p.press(b) {
            ReviewOut::None => Some(Page::Review(p)),
            ReviewOut::Back => self.curator.history.review_from.take().map(Page::History),
            ReviewOut::Card(g) => {
                let path = g.path.clone();
                self.history_card(&path, Page::Review(p))
            }
            ReviewOut::Period(period) => {
                if let Some(Counted::Review(r)) = self.count(Ask::Review(period), p.today) {
                    p.set(*r);
                }
                Some(Page::Review(p))
            }
        }
    }

    fn take_hits(&mut self, hits: Vec<(pocket_ui::touch::Area, Hit)>, sel: usize, rows: usize) {
        for (area, hit) in hits {
            match hit {
                Hit::Row(i) => self.hits.row(area, i),
                Hit::Button(b) => self.hits.button(area, b),
                Hit::Presses(b, n) => self.hits.presses(area, b, n),
            }
        }
        if rows > 0 {
            self.hits.list(sel, rows, 0.0, 72.0);
        }
    }

    pub(crate) fn paint_history_page(&mut self, c: &mut pocket_ui::gfx::Canvas, p: &HistoryPage) {
        let corner = self.status_corner();
        let systems = &self.systems;
        let names = |id: &str| (system_label(systems, id), system_short(systems, id));
        let hits = hp::paint_history(c, &mut self.text, p, &names, Some(&corner));
        let rows = p.summary.as_ref().map_or(0, |s| s.games.len().min(hp::ROWS));
        self.take_hits(hits, p.sel, rows);
        self.paint_hint_row(c, &p.hints());
    }

    pub(crate) fn paint_review_page(&mut self, c: &mut pocket_ui::gfx::Canvas, p: &ReviewPage) {
        let corner = self.status_corner();
        let cover = p.current().and_then(hp::card_game).and_then(|g| self.history_item(&g.path)).and_then(|item| self.cover(item, 340, 460));
        let systems = &self.systems;
        let names = |id: &str| (system_label(systems, id), system_short(systems, id));
        let hits = hp::paint_review(c, &mut self.text, p, &names, Some(&corner), cover.as_deref());
        self.take_hits(hits, 0, 0);
        self.paint_hint_row(c, &p.hints());
    }

    // ---- snapshots ----------------------------------------------------------------------------------------------------

    /// `--snapshot curator:history[:week|month|year[:<keys>]]` and `curator:review[:year|month[:<card>]]`: made-up
    /// history over the library's games (`play_history::demo`), ending today (10 December for the year's review).
    /// `curator:history-empty[:week|month|year]` is History with nothing recorded yet, `curator:history-cutoff[…]` with
    /// only a session today whose end was never recorded, `curator:history-mixed[…]` the same after made-up history, `curator:history-hub[:cutoff|:quiet[:<look>]]` the
    /// Curator list on its History row (`cutoff`: on the next row, with only a session without an end; `quiet`: nothing
    /// played this week), `curator:history-home` Home with the Lens off and its History row. Keys as for
    /// the Lens: u d l r, L R for L1 R1, a, y, b.
    pub(crate) fn history_snapshot(&mut self, what: &str, parts: &[&str]) {
        self.curator.history.here = true;
        // A snapshot never reads the real clock: its text (the game "Back after a long break" names, the periods' labels)
        // depends on today's date, and a gate that goes red when the date changes is no gate. 23 October 2026, 20:00 UTC;
        // the year's review is on 10 December of that year.
        let y = 2026;
        self.curator.history.now = Some((ph::day_of(y, 10, 23) * ph::DAY + 20 * 3600) as u64);
        if what == "review" && parts.get(1).is_none_or(|s| *s == "year") {
            // 10 December, 20:00 UTC.
            self.curator.history.now = Some((ph::day_of(y, 12, 10) * ph::DAY + 20 * 3600) as u64);
        }
        let (_, tz, today) = self.history_now();
        let mut games: Vec<Game> = self.systems.iter().flat_map(|s| s.games.iter().map(move |g| Game { path: g.path.to_string_lossy().into_owned(), name: g.name.clone(), system: s.name.clone() })).collect();
        // A shuffle that is always the same, so the favourites are not all of one system.
        games.sort_by_key(|g| g.name.bytes().fold(7u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64)));
        if games.is_empty() {
            games = ph::demo_games();
        }
        let log = match what {
            "history-empty" => Log::default(),
            // pocket-ui stopped while the game ran (a crash, the battery): a session today without a recorded end,
            // alone (`history-cutoff`) or after made-up history (`history-mixed`).
            _ if matches!(what, "history-cutoff" | "history-mixed") || (what == "history-hub" && parts.get(1) == Some(&"cutoff")) => {
                let base = if what == "history-mixed" { ph::demo(&games, today - 1, tz) } else { Log::default() };
                let game = games.iter().find(|g| g.system == "gbc").or(games.first()).cloned().unwrap_or_default();
                let mut all = base.games.clone();
                let i = all.iter().position(|g| g.path == game.path).unwrap_or_else(|| {
                    all.push(game);
                    all.len() - 1
                });
                let mut sessions = base.sessions.clone();
                let start = (today * ph::DAY + 10 * 3600 - tz as i64) as u64;
                sessions.push(play_log::Session { game: i, start, tz, secs: 0, ended: false, emulator: "pocket-core".into(), ..Default::default() });
                play_log::build(all, sessions, base.statuses.clone(), base.carried.clone())
            }
            // `history-hub:quiet`: the made-up history ended last week, so nothing was played this week.
            "history-hub" if parts.get(1) == Some(&"quiet") => ph::demo(&games, today - 8, tz),
            _ => ph::demo(&games, today, tz),
        };
        play_log::show_instead(log);
        match what {
            "history" | "history-empty" | "history-cutoff" | "history-mixed" => {
                let span = match parts.get(1).copied() {
                    Some("month") => Span::Month,
                    Some("year") => Span::Year,
                    _ => Span::Week,
                };
                self.open_history(span);
                for k in parts.get(2).copied().unwrap_or("").chars() {
                    let b = match k {
                        'u' => Button::Up,
                        'd' => Button::Down,
                        'l' => Button::Left,
                        'r' => Button::Right,
                        'L' => Button::L1,
                        'R' => Button::R1,
                        'a' => super::CONFIRM,
                        'y' => Button::West,
                        'b' => super::CANCEL,
                        _ => continue,
                    };
                    self.curator_input(b);
                }
            }
            "history-hub" | "history-home" => {
                if what == "history-hub" {
                    self.curator_activate(Item::Curator);
                } else {
                    self.curator.prefs.lens = false;
                    self.rebuild();
                }
                // `history-hub:cutoff`: the row after History selected, so History's side text shows.
                let after = usize::from(parts.get(1) == Some(&"cutoff"));
                if let Some(i) = self.items.iter().position(|i| *i == Item::History) {
                    self.sel = (i + after).min(self.items.len().saturating_sub(1));
                    self.prev_sel = self.sel;
                }
                // `history-hub:<cutoff|quiet>:<look>`: in that look (`split`, `cards`, …).
                if let Some(look) = parts.get(2) {
                    self.theme = super::app_curator::look_named(look);
                }
            }
            _ => {
                let span = if parts.get(1) == Some(&"month") { Span::Month } else { Span::Year };
                let period = Period::of(span, if span == Span::Month { today - 30 } else { today });
                self.open_review(period, None);
                let card: usize = parts.get(2).and_then(|n| n.parse().ok()).unwrap_or(0);
                for _ in 0..card {
                    self.curator_input(Button::Right);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review SOL-2026-10-24 #7: the migration's seed is made from copies, off the UI thread, without a library
    /// scan; with nothing played before it is empty (only the "migrated" mark).
    #[test]
    fn the_seed_is_made_from_copies_without_a_library_scan() {
        assert_eq!(seed_of(Vec::new(), Vec::new(), None, 5).games, Vec::new(), "nothing to take over");
        let played = vec![("/roms/snes/Chrono Trigger (USA).sfc".to_string(), 36_000, 12), ("/roms/gb/Tetris.gb".to_string(), 600, 1)];
        let recents = vec![SeedRecent { path: "/roms/gb/Tetris.gb".into(), name: "Tetris DX".into(), system: "gbc".into(), secs: 900, last_played: 77 }];
        let seed = seed_of(played, recents, None, 5);
        let g: Vec<_> = seed.games.iter().map(|g| (g.name.as_str(), g.system.as_str(), g.secs, g.sessions, g.last_played)).collect();
        assert_eq!(g, [("Tetris DX", "gbc", 900, 1, Some(77)), ("Chrono Trigger", "snes", 36_000, 12, None)], "the recent games name theirs; the others are named from their file and folder");
        assert_eq!(seed.now, 5);
    }
}
