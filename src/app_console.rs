//! Part of the pocket-ui binary: the console pages (`pocket_ui::console_page`, X on a system) and the short facts
//! about a selected system on the shelf (`pocket_ui::consoles`). What the page says about this Pocket (games, time
//! played, the emulator, the BIOS) is gathered here from the library, the recent games and the launch routing, and
//! so are the links of Credits & links (`pocket_ui::credits_page`, X on the page): the related consoles and the
//! consoles with the same chip that are in the library, and the games of the library by the console's makers.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pocket_ui::console_credits;
use pocket_ui::console_page::{self, Look, Out, Page, PageData};
use pocket_ui::consoles::{self, Here};
use pocket_ui::credits_page::{CreditsData, Link, LinkRow, GAMES_MAX};
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::pal;
use pocket_ui::settings::Theme;
use pocket_ui::text::Face;

use super::{stripe, system_short, App, Item, Screen, HOST_BIN};

/// The open console page and the system (index into the library) it is about.
pub(crate) struct OpenConsole {
    pub page: Page,
    pub system: usize,
    /// The pages a link of Credits & links was followed from, newest last: B goes back to them.
    pub back: Vec<(Page, usize)>,
}

/// How many console pages B can go back through.
const BACK_MAX: usize = 8;

/// `--snapshot` draws the pages as on the device, where pocket-core is installed.
static ASSUME_POCKET_CORE: AtomicBool = AtomicBool::new(false);

fn pocket_core_installed() -> bool {
    ASSUME_POCKET_CORE.load(Ordering::Relaxed) || Path::new(HOST_BIN).exists()
}

impl App {
    /// Whether X on this item opens a console page: a system the fact sheets know.
    pub(crate) fn has_console_page(&self, item: Option<Item>) -> bool {
        matches!(item, Some(Item::System(s)) if self.systems.get(s).is_some_and(|sys| consoles::for_system(&sys.name).is_some()))
    }

    /// The fact sheet's headline for a system item ("Sega · 1998 · sixth generation").
    pub(crate) fn console_headline(&self, item: Item) -> Option<String> {
        let Item::System(s) = item else { return None };
        consoles::for_system(&self.systems.get(s)?.name)?.lines.headline.clone()
    }

    /// X: the console page on a system, the game's card on a game.
    pub(crate) fn open_details(&mut self) {
        match self.items.get(self.sel).copied() {
            Some(Item::System(s)) => self.open_console(s),
            _ => self.open_card(),
        }
    }

    fn open_console(&mut self, s: usize) {
        if let Some(page) = self.console_page(s) {
            self.console = Some(OpenConsole { page, system: s, back: Vec::new() });
        }
    }

    /// The console page of a system of the library, with what this Pocket knows about it.
    fn console_page(&self, s: usize) -> Option<Page> {
        let sys = self.systems.get(s)?;
        let console = consoles::for_system(&sys.name)?;
        let data = PageData {
            console,
            stripe: stripe(&sys.name),
            games: sys.games.len(),
            played_secs: self.system_played_secs(s),
            last: self.system_last_played(s).map(|(name, at, _)| (name, at)),
            runs_on: self.system_runs_on(s),
            bios: consoles::bios_status(&sys.name, Path::new(pocket_ui::hostsys::BIOS_DIR)),
            achievements: self.console_achievements(s),
        };
        Some(Page::new(data))
    }

    /// The library system that plays `target`'s games (a link's ROCKNIX id), as a link: its console page when it has
    /// one, else its games.
    fn link_to_system(&self, target: &str) -> Option<Link> {
        let s = self.systems.iter().position(|x| !x.games.is_empty() && console_credits::folder_plays(&x.name, target))?;
        Some(if consoles::for_system(&self.systems[s].name).is_some() { Link::Console(s) } else { Link::Games(s) })
    }

    /// Credits & links of a system: the file's credits with the links found in the library.
    fn credits_data(&self, s: usize) -> Option<CreditsData> {
        let started = Instant::now();
        let name = self.systems.get(s)?.name.clone();
        let credits = console_credits::for_system(&name)?;
        let console = consoles::for_system(&name)?;
        let same_chip: Vec<LinkRow> = credits
            .same_chips()
            .into_iter()
            .filter_map(|(other, chips)| {
                let link = self.link_to_system(other)?;
                let names: Vec<&str> = chips.iter().map(|&i| credits.chips[i].name.as_str()).collect();
                Some(LinkRow {
                    name: consoles::for_system(other).map_or(other, |c| c.name.as_str()).to_string(),
                    note: names.join(" · "),
                    stripe: stripe(other),
                    link,
                })
            })
            .collect();
        let mut family_links = Vec::new();
        let mut rest = Vec::new();
        for r in &credits.family {
            match r.system.as_deref().and_then(|t| self.link_to_system(t).map(|l| (t, l))) {
                Some((target, link)) => family_links.push(LinkRow {
                    name: r.name.clone(),
                    note: r.relation.word().to_string(),
                    stripe: stripe(target),
                    link,
                }),
                None => rest.push((r.relation, r.name.clone())),
            }
        }
        let by_makers = self.games_by_makers(&credits.game_makers);
        let games_ready = by_makers.is_some();
        let (games, games_more) = by_makers.unwrap_or_default();
        println!(
            "credits {name}: {} links, {} games by the makers{} in {:?}",
            same_chip.len() + family_links.len(),
            games.len() + games_more,
            if games_ready { "" } else { " (catalog not ready)" },
            started.elapsed()
        );
        Some(CreditsData {
            credits,
            console_name: console.name.clone(),
            stripe: stripe(&name),
            same_chip,
            family_links,
            family_text: CreditsData::group_text(&rest),
            games,
            games_more,
            games_ready,
        })
    }

    /// Games of the library whose developer is one of `makers` (gamedb's names), oldest first, one per game: the first
    /// `GAMES_MAX` as links, and how many more there are. From Curator's catalog, which is built in the background
    /// after the first frame: None until it is there (the page says it is getting ready and `credits_refresh` fills
    /// it in), so opening the page never looks up a game's facts on this thread.
    fn games_by_makers(&self, makers: &[String]) -> Option<(Vec<LinkRow>, usize)> {
        if makers.is_empty() {
            return Some((Vec::new(), 0));
        }
        let ix = self.curator.index.as_ref()?;
        // The catalog's games by the makers, with the catalog's indices, which are the library's when it was built: a
        // game that moved since is left out.
        let mut found: Vec<&pocket_ui::curator::Features> = ix
            .catalog
            .games
            .iter()
            .filter(|f| {
                let (si, gi) = f.at;
                self.systems.get(si).and_then(|s| s.games.get(gi)).is_some_and(|g| g.path == f.path) && f.developers.iter().any(|d| makers.contains(d))
            })
            .collect();
        found.sort_by_cached_key(|f| (f.year, f.name.to_lowercase()));
        // A game kept as two files, or on two systems, is listed once.
        let mut seen = std::collections::HashSet::new();
        found.retain(|f| f.qid.as_deref().is_none_or(|q| seen.insert(q)));
        let more = found.len().saturating_sub(GAMES_MAX);
        let rows = found
            .into_iter()
            .take(GAMES_MAX)
            .map(|f| {
                let (year, name, (si, gi)) = (f.year, f.name.as_str(), f.at);
                let system = &self.systems[si].name;
                let short = system_short(&self.systems, system);
                LinkRow {
                    name: name.to_string(),
                    note: year.map_or(short.clone(), |y| format!("{short} · {y}")),
                    stripe: stripe(system),
                    link: Link::Game(si, gi),
                }
            })
            .collect();
        Some((rows, more))
    }

    /// Curator's catalog was (re)built: an open Credits & links that waited for it gets its games.
    pub(crate) fn credits_refresh(&mut self) {
        let Some(open) = self.console.as_ref() else { return };
        let Some(credits) = open.page.credits.as_ref() else { return };
        let makers = &credits.data.credits.game_makers;
        if let Some((games, more)) = self.games_by_makers(makers) {
            if let Some(c) = self.console.as_mut().and_then(|o| o.page.credits.as_mut()) {
                c.set_games(games, more);
            }
        }
    }

    /// X on a console page: Credits & links.
    fn open_credits(&mut self) {
        let Some(s) = self.console.as_ref().map(|o| o.system) else { return };
        if let Some(data) = self.credits_data(s) {
            if let Some(open) = self.console.as_mut() {
                open.page.open_credits(data);
            }
        }
    }

    /// A on a link of Credits & links.
    fn follow_link(&mut self, link: Link) {
        match link {
            Link::Console(s) => {
                let Some(page) = self.console_page(s) else { return };
                let Some(mut open) = self.console.take() else { return };
                let from = std::mem::replace(&mut open.page, page);
                open.back.push((from, open.system));
                if open.back.len() > BACK_MAX {
                    open.back.remove(0);
                }
                open.system = s;
                self.console = Some(open);
            }
            Link::Games(s) => self.open_games(s),
            Link::Game(s, g) => {
                let Some(sys) = self.systems.get(s) else { return };
                let Some(game) = sys.games.get(g) else { return };
                let (system, name, path) = (sys.name.clone(), game.name.clone(), game.path.clone());
                // The card lies over the console page; B on it comes back here.
                self.show_card(Item::Game(s, g), system, name, path);
            }
        }
    }

    /// A system's list of games, as A on its row; the console pages close.
    fn open_games(&mut self, s: usize) {
        self.console = None;
        self.home_item = Some(Item::System(s));
        self.screen = Screen::Games(s);
        self.sel = 0;
        self.rebuild();
    }

    /// The game of the system played last, when, and its file; only games still in the library.
    fn system_last_played(&self, s: usize) -> Option<(String, u64, PathBuf)> {
        let sys = self.systems.get(s)?;
        self.recents
            .list
            .iter()
            .find(|r| r.system == sys.name && sys.games.iter().any(|g| g.path == r.path))
            .map(|r| (r.name.clone(), r.last_played, r.path.clone()))
    }

    /// Seconds played on the system: per game the larger of the library's count and the recent games' (which
    /// also has time from before sessions were counted, but only for the last 20 games).
    fn system_played_secs(&self, s: usize) -> u64 {
        let Some(sys) = self.systems.get(s) else { return 0 };
        sys.games
            .iter()
            .map(|g| {
                let recent = self.recents.list.iter().find(|r| r.path == g.path).map_or(0, |r| r.play_secs);
                self.library.get(&g.path).secs.max(recent)
            })
            .sum()
    }

    /// What plays the system here, as `launch` would start one of its games.
    fn system_runs_on(&self, s: usize) -> Option<String> {
        let sys = self.systems.get(s)?;
        let (emulator, core) = self.route(sys, None, pocket_core_installed());
        consoles::runner_text(&emulator, &core)
    }

    pub(crate) fn console_input(&mut self, b: Button) {
        let Some(open) = self.console.as_mut() else { return };
        match open.page.press(b) {
            Out::None => {}
            // Back to the console a link was followed from (its Credits & links as they were), or to Home.
            Out::Back => match open.back.pop() {
                Some((page, system)) => {
                    open.page = page;
                    open.system = system;
                }
                None => self.console = None,
            },
            Out::Emulator => {
                let s = open.system;
                self.emu_choice_for_console(s);
            }
            // As A on the system's row.
            Out::Games => {
                let s = open.system;
                self.open_games(s);
            }
            Out::Credits => self.open_credits(),
            Out::Open(link) => self.follow_link(link),
        }
    }

    pub(crate) fn paint_console(&mut self, c: &mut Canvas) {
        let Some(s) = self.console.as_ref().map(|o| o.system) else { return };
        let (bw, bh) = console_page::art_box();
        let art = self.system_art(Item::System(s), bw, bh);
        let corner = self.status_corner();
        let look = if self.theme == Theme::Split { Look::split() } else { Look::plain() };
        let Some(open) = self.console.as_mut() else { return };
        let hits = open.page.paint(c, &mut self.text, art.as_deref(), Some(&corner), look);
        for (area, b) in hits.buttons {
            self.hits.button(area, b);
        }
        // Credits & links: a tap on a link selects it, a second tap opens it.
        for (area, row) in hits.rows {
            self.hits.row(area, row);
        }
        // A swipe scrolls the page as up and down do.
        if let Some((row, rows, pitch)) = hits.scroll {
            self.hits.list(row, rows, 0.0, pitch);
        }
    }

    /// The Split look's room under a selected system's name: the fact sheet's headline at `y`, then the short facts
    /// down to `bottom`. A system without a fact sheet gets its own rows (Your games, Runs on) at `y`, so the room is
    /// never empty. False (nothing drawn) for anything that is not a system.
    pub(crate) fn paint_system_facts(&mut self, c: &mut Canvas, item: Item, x: i32, y: i32, w: i32, bottom: i32) -> bool {
        let Item::System(s) = item else { return false };
        let Some(sys) = self.systems.get(s) else { return false };
        let console = consoles::for_system(&sys.name);
        if let Some(h) = console.and_then(|c| c.lines.headline.as_ref()) {
            let h = self.text.fit(Face::Body, 28, h, w);
            self.text.draw(c, Face::Body, 28, x, y, pal().split_dim, &h);
        }
        let here = Here {
            games: sys.games.len(),
            last: self.system_last_played(s).map(|(name, _, _)| name),
            runs_on: self.system_runs_on(s),
        };
        let (rows, top) = match console {
            Some(console) => (consoles::shelf_rows(console, &here), y + 60),
            None => (consoles::here_rows(&here), y + 20),
        };
        console_page::paint_facts(c, &mut self.text, &rows, x, top, w, bottom, pal().dust, pal().chalk);
        true
    }

    /// `--snapshot console:<system>[:sources|:credits][:<n>]`: the system's console page, its sources with `sources`,
    /// Credits & links with `credits`.
    pub(crate) fn console_snapshot(&mut self, rest: &str) {
        ASSUME_POCKET_CORE.store(true, Ordering::Relaxed);
        let id = rest.split(':').next().unwrap_or_default();
        let Some(s) = self.systems.iter().position(|sys| sys.name == id) else {
            eprintln!("console:{id}: no such system in the library");
            return;
        };
        self.screen = Screen::Home;
        self.rebuild();
        self.open_console(s);
        if rest.split(':').any(|p| p == "sources") {
            if let Some(open) = self.console.as_mut() {
                open.page.press(Button::West);
            }
        }
        if rest.split(':').any(|p| p == "credits") {
            // As on the device a moment after the start: Curator's catalog is there.
            self.curator_build(true, true);
            self.open_credits();
        }
    }

    /// After the first paint (which measures the page): `<n>` presses of Down.
    pub(crate) fn console_snapshot_scroll(&mut self, rest: &str) {
        let downs: usize = rest.split(':').skip(1).find_map(|p| p.parse().ok()).unwrap_or(0);
        if let Some(open) = self.console.as_mut() {
            for _ in 0..downs {
                open.page.press(Button::Down);
            }
        }
    }

    /// `--snapshot shelf:<system>[:<look>]`: Home in that look (shelf, minimal, split, cards, cabinet; Split by
    /// default) with the system selected and pulled out.
    pub(crate) fn shelf_snapshot(&mut self, rest: &str) {
        ASSUME_POCKET_CORE.store(true, Ordering::Relaxed);
        let mut parts = rest.split(':');
        let id = parts.next().unwrap_or_default();
        self.theme = match parts.next().unwrap_or("split") {
            "shelf" => Theme::Shelf,
            "minimal" => Theme::Minimal,
            "cards" => Theme::Cards,
            "cabinet" => Theme::Cabinet,
            _ => Theme::Split,
        };
        self.screen = Screen::Home;
        self.rebuild();
        // Box art is read at once, not on the thread (the games' cases, `app_cabinet`).
        self.cabinet.inline = true;
        if let Some(s) = self.systems.iter().position(|sys| sys.name == id) {
            if let Some(i) = self.items.iter().position(|it| *it == Item::System(s)) {
                self.sel = i;
                self.prev_sel = i;
            }
            // `:games[:<row>]`: the system's list of games, that row selected.
            if parts.next() == Some("games") {
                self.screen = Screen::Games(s);
                self.rebuild();
                self.sel = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0).min(self.items.len().saturating_sub(1));
                self.prev_sel = self.sel;
            }
        }
        self.moved_at = Instant::now() - Duration::from_secs(2);
    }

    /// `--snapshot bench-shelf[:<look>]`: what a move on Home costs (docs/LATENCY.md "A move on Home"). Home in that
    /// look (Split by default) with Curator's index there, as a moment after the start; then Down to the end of the
    /// list and Up to its top, and for each move the first frame `draw` makes after it, in thread CPU time (a frame
    /// preempted by a compile is not a slow one), with the letters it drew on the spot and the strings it ran through
    /// the German patterns. The speed guard's `nav` is this frame on the device (tests/shelf_moves.rs).
    pub(crate) fn bench_shelf(&mut self, look: &str, w: u32, h: u32) -> ShelfBench {
        self.shelf_snapshot(&format!(":{look}"));
        // Box art the way the app reads it, on the thread (a snapshot reads it at once).
        self.cabinet.inline = false;
        self.pictures.sync = true;
        self.curator_build(true, true);
        self.rebuild();
        self.sel = Item::first_row(&self.items);
        self.prev_sel = self.sel;
        let mut buf = vec![0u32; (w * h) as usize];
        pocket_ui::status_corner::start(|| {});
        self.text.prewarm();
        self.text.finish_prewarm();
        let cpu = || pocket_ui::run_ahead::thread_cpu_ms() * 1000.0;
        let frame = |app: &mut App, buf: &mut [u32]| {
            app.motion_frame_begin(Instant::now());
            app.paint_in_appearance(buf, w, h, &mut |app, c| {
                app.paint(c);
                app.motion_after_paint(c);
                app.sys.paint(c, &mut app.text, app.menu.is_none());
                app.paint_leave(c);
            });
        };
        // Once over the whole list first: every picture is in the cache, as after the first few moves on the device
        // (snapshots decode on the spot; on the device the loader's thread has long been done).
        let last = self.items.len().saturating_sub(1);
        let moves: Vec<Button> = std::iter::repeat_n(Button::Down, last).chain(std::iter::repeat_n(Button::Up, last)).collect();
        for _ in 0..2 {
            for b in &moves {
                self.step(*b);
                frame(self, &mut buf);
            }
        }
        let mut us = Vec::new();
        let (glyphs, patterns) = (self.text.drawn_on_the_spot(), self.text.pattern_runs());
        let mut worst = (0.0, String::new());
        for b in &moves {
            self.step(*b);
            let t = cpu();
            frame(self, &mut buf);
            let took = cpu() - t;
            if took > worst.0 {
                worst = (took, format!("{:?}", self.items.get(self.sel)));
            }
            us.push(took);
        }
        us.sort_by(f64::total_cmp);
        let at = |q: f64| us.get(((us.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(0.0);
        ShelfBench {
            moves: us.len(),
            p50_us: at(0.5),
            p95_us: at(0.95),
            max_us: at(1.0),
            glyphs: self.text.drawn_on_the_spot() - glyphs,
            patterns: self.text.pattern_runs() - patterns,
            worst: worst.1,
        }
    }
}

/// What `App::bench_shelf` measured: the first frame after a move on Home, over all moves.
pub(crate) struct ShelfBench {
    pub moves: usize,
    pub p50_us: f64,
    pub p95_us: f64,
    pub max_us: f64,
    pub glyphs: usize,
    pub patterns: usize,
    pub worst: String,
}
