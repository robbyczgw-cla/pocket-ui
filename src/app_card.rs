//! Part of the pocket-ui binary: the App's side of the game card (`pocket_ui::game_card`), the play
//! status and notes (`pocket_ui::library_meta`) and the offline game facts (`pocket_ui::gamedb`).
//! Kept out of main.rs so the shelf only carries small hooks.

use std::path::{Path, PathBuf};
use std::time::Instant;

use pocket_ui::game_card::{self, Card, CardData, ChoiceRow, CuratorRow, Out, RelatedGame, Summary};
use pocket_ui::gamedb::{self, Choice, GameInfo, Owned};
use pocket_ui::gfx::Canvas;
use pocket_ui::library_meta::Status;
use pocket_ui::pad::Button;
use pocket_ui::settings::Theme;

use super::{stripe, system_label, system_short, App, Item, UserEvent};

/// Games listed under "More by" and "Other games in".
const RELATED_MAX: usize = 5;
/// Entries offered by "Wrong game? Choose another", before "None of these".
const CHOICES: usize = 6;

/// The open card and the game it is about.
pub(crate) struct OpenCard {
    pub card: Card,
    /// Facts from the offline entry/starter pack, before gamelist and derived-series additions. A new Connections
    /// catalog can rebuild those additions without preserving a stale inferred Series (`app_connections`).
    pub source_facts: Vec<(&'static str, String)>,
    pub item: Item,
    pub system: String,
    pub name: String,
    pub path: PathBuf,
    /// The Wikipedia article whose lead is being loaded or shown.
    pub article: Option<String>,
}

impl App {
    /// The facts for a game of the library, looked up once per file: the entry the player chose on the
    /// card, else the automatic match by the name its checksum gave (`app_identify`), else by its file name.
    pub(crate) fn game_info(&mut self, system: &str, path: &Path) -> Option<GameInfo> {
        if let Some(hit) = self.game_infos.get(path) {
            return hit.clone();
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let choice = self.library.get(path).choice;
        let identified = self.identify.names.get(path).cloned();
        let hit = self.gamedb.resolve_rom(system, stem, identified.as_deref(), choice.as_ref());
        self.game_infos.insert(path.to_path_buf(), hit.clone());
        hit
    }

    /// X on a game: its card.
    pub(crate) fn open_card(&mut self) {
        let Some(item) = self.items.get(self.sel).copied() else { return };
        let (system, name, path) = match item {
            Item::Recent(i) => {
                let r = &self.recents.list[i];
                (r.system.clone(), r.name.clone(), r.path.clone())
            }
            Item::Game(s, g) => (
                self.systems[s].name.clone(),
                self.systems[s].games[g].name.clone(),
                self.systems[s].games[g].path.clone(),
            ),
            _ => return,
        };
        self.show_card(item, system, name, path);
    }

    /// Builds the card of a game and starts loading its description if it isn't on the card yet.
    pub(crate) fn show_card(&mut self, item: Item, system: String, name: String, path: PathBuf) {
        let started = Instant::now();
        let info = self.game_info(&system, &path);
        let mut data = self.card_data(&system, &name, &path, info.as_ref());
        let source_facts = data.facts.clone();
        let article = info.as_ref().and_then(|i| i.enwiki.clone());
        // Wikipedia's text, when it is on the card's disk already, comes before anything else.
        let cached = article.as_ref().and_then(|t| pocket_ui::wiki_text::cached(&self.data_dir, t));
        // What the player's EmulationStation gamelist knows, for what pocket-ui's own data lacks (`gamelist::fill_card`).
        if let Some(e) = self.gamelist.index.get(&system, &path) {
            pocket_ui::gamelist::fill_card(&mut data, e, cached.is_some());
        }
        // Connections: which facts lead to the player's other games (a few lookups in the index, `app_connections`).
        self.card_connections(&mut data, &path);
        println!(
            "card {}: {} in {:?}",
            path.display(),
            info.as_ref().map_or("no entry", |i| i.qid.as_str()),
            started.elapsed()
        );
        let mut open = OpenCard {
            card: Card::new(data),
            source_facts,
            item,
            system,
            name,
            path,
            article: article.clone(),
        };
        if let Some(title) = article {
            open.card.data.summary = match cached {
                Some(text) => Summary::Wikipedia { text, title },
                // The gamelist's description is on the card: no wait for Wikipedia, no request for it.
                None if matches!(open.card.data.summary, Summary::Gamelist(_)) => open.card.data.summary.clone(),
                None => {
                    let (proxy, dir) = (self.proxy.clone(), self.data_dir.clone());
                    std::thread::spawn(move || {
                        let text = pocket_ui::wiki_text::fetch(&dir, &title);
                        let _ = proxy.send_event(UserEvent::CardText { title, text });
                    });
                    Summary::Loading
                }
            };
        }
        // The Minimal, Cards and Split looks show the info page (`game_info`); Shelf and Cabinet the box (`game_box`)
        // unless SELECT chose the page last time.
        if matches!(self.theme, Theme::Minimal | Theme::Cards | Theme::Split) {
            open.card.show_info();
        } else if self.game_box {
            open.card.boxed = Some(pocket_ui::game_box::BoxView::new());
            // The case opens: plastic or cardboard, instead of the confirm click.
            self.play_sound(pocket_ui::ui_sound::opening(self.medium(item)));
            pocket_ui::ui_sound::played();
        }
        self.card = Some(open);
    }

    /// The Wikipedia lead arrived (or could not be loaded).
    pub(crate) fn card_text(&mut self, title: String, text: Result<String, String>) {
        let Some(open) = self.card.as_mut().filter(|o| o.article.as_deref() == Some(title.as_str())) else {
            return;
        };
        open.card.data.summary = match text {
            Ok(text) => Summary::Wikipedia { text, title },
            Err(why) => Summary::Unavailable(why),
        };
    }

    /// The card's rows: a save with two versions first (`app_sync_conflicts`), then Echoes, Versions, Stickers
    /// (`app_curator`), Screenshots (`app_gallery`) and Hide or delete at the end of the card (`app_storage`). The one
    /// list for a new card and for every refresh of an open one, so a refresh never drops a row.
    pub(crate) fn card_rows(&self, system: &str, path: &Path) -> Vec<CuratorRow> {
        self.conflict_card_row(system, path)
            .into_iter()
            .chain(self.curator_card_rows(path))
            .chain(self.card_screenshots_row(system, path))
            .chain(std::iter::once(self.card_manage_row()))
            .collect()
    }

    fn card_data(&mut self, system: &str, name: &str, path: &Path, info: Option<&GameInfo>) -> CardData {
        let meta = self.library.get(path);
        let played = self.recents.list.iter().find(|r| r.path == path).map(|r| (r.last_played, r.play_secs));
        let mut data = CardData {
            title: name.to_string(),
            system: system_label(&self.systems, system),
            stripe: stripe(system),
            matched: info.is_some(),
            listed_as: None,
            year: None,
            players: None,
            facts: Vec::new(),
            scores: None,
            sentence: None,
            summary: Summary::None,
            played,
            sessions: meta.sessions,
            status: meta.status,
            suggestion: self.library.suggestion(path, played.map_or(0, |p| p.1)),
            rating: meta.rating,
            note: meta.note,
            more_by: None,
            series: None,
            rows: self.card_rows(system, path),
            achievements: None,
            // How long the charge lasts in this game, from memory (`battery_games`); "Measuring this game…" before,
            // "Loading…" in the moments before the sampler has read its files.
            battery: pocket_ui::battery_history::game_card(&pocket_ui::battery_games::Key::new(system, path, ""), &system_short(&self.systems, system)),
            // A port: its performance and fan, and the busy-core hint (`app_port_perf`).
            port: self.port_card_line(system, path),
            port_update: system == pocket_ui::port_perf::SYSTEM && path.file_name().and_then(|n| n.to_str()).is_some_and(pocket_ui::ports::update_waiting),
            // "Surprise me" sets the reason after the card is made (`app_curator`).
            surprise: None,
            // "Wrong pictures? Look up again" for games whose pictures come from the collection (`app_identify`).
            look_up: pocket_ui::covers::playlist(system).is_some(),
            // `gamelist::fill_card` sets it (`show_card`).
            from_gamelist: false,
            // A patch next to the game (a few file checks, no reading): pocket-core applies it at the start (`softpatch`).
            patch: Some(pocket_ui::softpatch::find(path).iter().map(|p| pocket_ui::softpatch::name(p)).collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty()),
            // The title is already on the card; reserve the problem's room for the cause and remedy. The start
            // notice still names the game in full (`library_cache::empty_game`).
            broken: pocket_ui::library_cache::is_empty_file(path).then(|| "The file is empty (0 bytes). Copy the game to the card again.".into()),
            favorite: self.favorites.contains(path),
            // Its save slots on their medium (`save_media`): a few file checks, pocket-core's games only.
            saves: game_card::CardSaves::of(system, path, stripe(system)),
            // The facts that lead to the player's other games (`show_card`, `app_connections`).
            links: Vec::new(),
        };
        // RetroAchievements: what is known, and a lookup in the background when that is old (`app_achievements`).
        data.achievements = self.card_achievements(system, path);
        // A game of the starter pack (docs/STARTER-PACK.md): whose it is and under what licence it may be shared.
        if let Some(g) = path.file_name().and_then(|n| n.to_str()).and_then(|n| pocket_ui::starter::find(system, n)) {
            data.facts.push(("By", g.author.clone()));
            data.facts.push(("Licence", g.license_short.clone()));
        }
        let Some(info) = info else { return data };
        data.year = info.year;
        data.players = gamedb::players_text(info);
        let join = |v: &[String], n: usize| (!v.is_empty()).then(|| v.iter().take(n).cloned().collect::<Vec<_>>().join(", "));
        for (label, value) in [
            ("Developer", join(&info.developers, 2)),
            ("Publisher", join(&info.publishers, 2)),
            ("Genre", join(&info.genres, 2).map(|g| capitalise(&g.replace(" video game", "")))),
            ("Series", join(&info.series, 1)),
        ] {
            if let Some(v) = value {
                data.facts.push((label, v));
            }
        }
        let scores = gamedb::top_scores(info, 2);
        if !scores.is_empty() {
            data.scores = Some(format!("{} · via Wikidata", scores.join(" · ")));
        }
        if words_key(&info.label) != words_key(name) {
            data.listed_as = Some(info.label.clone());
        }
        data.sentence = game_card::sentence(info);
        let related = self.related_games(info, path);
        data.more_by = related.0;
        data.series = related.1;
        data
    }

    /// "More by" and "Other games in" from the player's own library.
    #[allow(clippy::type_complexity)]
    fn related_games(&mut self, info: &GameInfo, path: &Path) -> (Option<(String, Vec<RelatedGame>)>, Option<(String, Vec<RelatedGame>)>) {
        let games: Vec<(String, String, PathBuf)> = self
            .systems
            .iter()
            .flat_map(|s| s.games.iter().map(move |g| (s.name.clone(), g.name.clone(), g.path.clone())))
            .collect();
        let infos: Vec<Option<GameInfo>> = games.iter().map(|(sys, _, p)| self.game_info(sys, p)).collect();
        let library: Vec<Owned> = games
            .iter()
            .zip(&infos)
            .map(|((sys, name, p), i)| Owned {
                system: sys,
                path: p,
                name,
                info: i.as_ref(),
            })
            .collect();
        let r = gamedb::related(info, path, &library);
        let to_list = |(key, owned): (String, Vec<Owned>)| {
            let list = owned
                .iter()
                .take(RELATED_MAX)
                .map(|o| RelatedGame {
                    name: o.name.to_string(),
                    system: system_short(&self.systems, o.system),
                    stripe: stripe(o.system),
                    year: o.info.and_then(|i| i.year),
                })
                .collect();
            (key, list)
        };
        (r.by_developer.map(to_list), r.in_series.map(to_list))
    }

    pub(crate) fn card_input(&mut self, b: Button) {
        let Some(open) = self.card.as_mut() else { return };
        match open.card.press(b) {
            Out::None => {}
            Out::Back => {
                let path = open.path.clone();
                self.card = None;
                // Opened from Curator → History or the review: back to it (`app_history`).
                self.history_card_closed(&path);
            }
            Out::Play => {
                let open = self.card.take().unwrap();
                self.launch(open.system, open.name, open.path);
            }
            Out::SetStatus(status) => {
                let path = open.path.clone();
                let saved = self.library.set_status(&path, status);
                if saved.is_ok() {
                    // Play history: "Completed in 2026" needs the day (`play_log`).
                    pocket_ui::play_log::status(&open.system, &open.name, &path, status);
                }
                self.card_saved(saved, &format!("Status: {}", status.label()));
                self.curator.touch();
            }
            Out::SetRating(rating) => {
                let path = open.path.clone();
                let saved = self.library.set_rating(&path, rating);
                let msg = match rating {
                    0 => "Rating removed".to_string(),
                    1 => "Rated 1 star".to_string(),
                    n => format!("Rated {n} stars"),
                };
                self.card_saved(saved, &msg);
                // The Lens's My rating, the shelves' order and the taste profile follow.
                self.curator.touch();
            }
            // "Surprise me": another pick, on its own card (`app_curator`).
            Out::Another => self.curator_surprise(),
            Out::Row(kind) => self.curator_card_row(kind),
            // A port has one emulator: START is its Performance & fan instead (`app_port_perf`).
            Out::Emulator if open.system == pocket_ui::port_perf::SYSTEM => self.open_port_perf_for_card(),
            Out::Emulator => self.this_game_for_card(),
            Out::Details { boxed } => {
                self.game_box = boxed;
                self.save_settings();
            }
            // Hide or delete (`app_storage`).
            Out::Hide => self.card_hide(),
            Out::PlanDelete => self.card_plan_delete(),
            Out::Delete { with_saves } => self.card_delete(with_saves),
            Out::SetNote(note) => {
                let path = open.path.clone();
                let saved = self.library.set_note(&path, &note);
                self.card_saved(saved, if note.is_empty() { "Note removed" } else { "Note saved" });
            }
            // "Wrong pictures? Look up again" (`app_identify`).
            Out::LookUpAgain => self.card_look_up_again(),
            // A on a developer, publisher, series or genre: the player's games with it (`app_connections`).
            Out::Connection(i) => self.open_connection(i),
            Out::ChooseGame => {
                let (system, path) = (open.system.clone(), open.path.clone());
                let rows = self.choice_rows(&system, &path);
                if let Some(open) = self.card.as_mut() {
                    open.card.open_chooser(rows);
                }
            }
            Out::SetChoice(choice) => {
                let (item, system, name, path) = (open.item, open.system.clone(), open.name.clone(), open.path.clone());
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
                // Picking what the name finds anyway is the automatic match again, not a pin.
                let identified = self.identify.names.get(&path).cloned();
                let automatic = self.gamedb.lookup_rom(&system, &stem, identified.as_deref()).map(|g| g.qid);
                let keep = match &choice {
                    Choice::Entry(q) if Some(q) == automatic.as_ref() => None,
                    other => Some(other.clone()),
                };
                let saved = self.library.set_choice(&path, keep);
                self.game_infos.remove(&path);
                let now = self.game_info(&system, &path).map(|g| g.label);
                self.show_card(item, system, name, path);
                self.card_saved(saved, &now.map_or("No facts for this game".to_string(), |l| format!("Now: {l}")));
            }
        }
    }

    /// The entries "Wrong game? Choose another" offers: the closest names of the system, the one shown now
    /// among them, then "None of these".
    fn choice_rows(&mut self, system: &str, path: &Path) -> Vec<ChoiceRow> {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        let now = self.game_info(system, path);
        let mut games = self.gamedb.candidates(system, &stem, CHOICES);
        if let Some(now) = &now {
            if !games.iter().any(|g| g.qid == now.qid) {
                games.pop();
                games.insert(0, now.clone());
            }
        }
        let short = system_short(&self.systems, system);
        let mut rows: Vec<ChoiceRow> = games
            .into_iter()
            .map(|g| {
                let mut meta = vec![short.clone()];
                meta.extend(g.year.map(|y| y.to_string()));
                meta.extend(g.developers.first().cloned());
                ChoiceRow {
                    current: now.as_ref().is_some_and(|n| n.qid == g.qid),
                    title: g.label,
                    meta: meta.join(" · "),
                    choice: Choice::Entry(g.qid),
                }
            })
            .collect();
        rows.push(ChoiceRow {
            title: "None of these".into(),
            meta: "No facts for this game".into(),
            choice: Choice::NoEntry,
            current: now.is_none(),
        });
        rows
    }

    /// Starts loading the cards' descriptions in the background when it may run: the rules of the automatic
    /// covers (`BackgroundRun::decide`: online, no game, not running already). The worker matches the games
    /// itself, so the shelf doesn't wait for it.
    pub(crate) fn auto_texts(&mut self, why: &str) {
        use pocket_ui::auto_covers::Decision;
        if self.setup.as_ref().is_some_and(|s| !s.past_wifi()) {
            return;
        }
        let in_game = self.running.is_some();
        let online = !in_game && !self.text_run.running() && pocket_ui::wifi::ip().is_some();
        let games = if online { self.text_run.candidates(&self.all_games()) } else { Vec::new() };
        let decision = self.text_run.decide(online, in_game, games.len(), false);
        println!("descriptions ({why}): {decision:?}, {} games to look at", games.len());
        if decision != Decision::Start {
            return;
        }
        let games: Vec<(String, PathBuf, Option<Choice>)> = games
            .into_iter()
            .map(|(system, rom)| {
                let choice = self.library.get(&rom).choice;
                (system, rom, choice)
            })
            .collect();
        let stop = self.text_run.begin(0);
        let (proxy, data_dir) = (self.proxy.clone(), self.data_dir.clone());
        std::thread::spawn(move || {
            use pocket_ui::auto_texts::{self, Event};
            let mut db = gamedb::GameDb::embedded();
            let todo = auto_texts::todo(&data_dir, &mut db, &games);
            // Games with nothing to load (no entry, no article, text cached) are done with as well.
            let to_load: std::collections::HashSet<&PathBuf> = todo.iter().map(|t| &t.0).collect();
            let mut looked_up: Vec<PathBuf> = games.iter().filter(|g| !to_load.contains(&g.1)).map(|g| g.1.clone()).collect();
            let _ = proxy.send_event(UserEvent::Texts(Event::Progress { done: 0, total: todo.len() }));
            let progress = |done, total| {
                let _ = proxy.send_event(UserEvent::Texts(Event::Progress { done, total }));
            };
            let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
            let run = auto_texts::fetch_until(
                &todo,
                progress,
                stopped,
                |title| pocket_ui::wiki_text::fetch(&data_dir, title),
                auto_texts::DELAY,
            );
            looked_up.extend(run.looked_up);
            let _ = proxy.send_event(UserEvent::Texts(Event::Done {
                looked_up,
                fetched: run.fetched,
                stopped: run.stopped,
            }));
        });
    }

    pub(crate) fn texts_event(&mut self, e: pocket_ui::auto_texts::Event) {
        use pocket_ui::auto_texts::Event;
        match e {
            Event::Progress { done, total } => self.text_run.progress(done, total),
            Event::Done { looked_up, fetched, stopped } => {
                println!(
                    "descriptions: {fetched} loaded, {} games looked at{}",
                    looked_up.len(),
                    if stopped { ", stopped" } else { "" }
                );
                self.text_run.finish(looked_up, stopped);
            }
        }
    }

    fn card_saved(&mut self, saved: std::io::Result<()>, what: &str) {
        let msg = match saved {
            Ok(()) => what.to_string(),
            Err(e) => {
                eprintln!("library: not saved: {e}");
                "Could not save: is the SD card full?".to_string()
            }
        };
        self.toast = Some((msg, Instant::now()));
    }

    /// Builds the box front, the cartridge and the big cover of the game the player rests on, so the first X finds them
    /// made (they cost ~50 ms on a Retro Handheld's UI thread, twice: before and after the cover arrives). Three tries
    /// per game, after 0.3, 0.7 and 1.2 s without input, with nothing else open; the later ones pick up the cover.
    pub(crate) fn box_warm_tick(&mut self, now: Instant) -> Option<Instant> {
        const AFTER_MS: [u64; 3] = [300, 700, 1200];
        let idle = self.game_box && matches!(self.theme, Theme::Shelf | Theme::Cabinet) && self.card.is_none() && self.running.is_none() && self.menu.is_none() && self.switcher.is_none() && self.settings_page.is_none();
        let item = self.items.get(self.sel).copied().filter(|i| idle && matches!(i, Item::Game(..) | Item::Recent(_)));
        let Some(item) = item.filter(|_| self.window.is_some()) else {
            self.box_warm = None;
            return None;
        };
        let tries = match self.box_warm {
            Some((i, n)) if i == item => n,
            _ => 0,
        };
        let Some(&after) = AFTER_MS.get(tries as usize) else { return None };
        let due = self.last_input + std::time::Duration::from_millis(after);
        if now < due {
            return Some(due);
        }
        let (w, h) = self.screen_size();
        let (w, h) = (w as i32, h as i32);
        self.cabinet_poll();
        let case_h = pocket_ui::game_box::case_height(w, h, self.medium(item));
        let (_, _, front) = self.box_front(item, case_h);
        if let Some(f) = &front {
            self.box_game(item, f);
        }
        self.cover(item, w, h);
        self.box_warm = Some((item, tries + 1));
        AFTER_MS.get(tries as usize + 1).map(|a| self.last_input + std::time::Duration::from_millis(*a))
    }

    pub(crate) fn paint_card(&mut self, c: &mut Canvas) {
        let Some(item) = self.card.as_ref().map(|o| o.item) else { return };
        let pictures = self.card_pictures(item, c.w);
        let corner = self.status_corner();
        if self.card.as_ref().is_some_and(|o| o.card.boxed.is_some()) {
            // The box: the case's front from the shelf's own machinery, at the box's height (`game_box`). Its cover is
            // read on the cabinet's thread: taken here, since the shelf that usually takes it is not painted under a card.
            self.cabinet_poll();
            let case_h = pocket_ui::game_box::case_height(c.w, c.h, self.medium(item));
            let (medium, look, front) = self.box_front(item, case_h);
            let game = front.as_ref().and_then(|f| self.box_game(item, f));
            let big = self.cover(item, c.w, c.h);
            let console = self.item_rom(item).and_then(|(system, _)| pocket_ui::system_art::get(&system));
            let Some(open) = self.card.as_mut() else { return };
            open.card.pictures = pictures;
            let inputs = pocket_ui::game_box::Inputs { medium, look, front: front.as_deref(), game: game.as_deref(), cover: big.as_deref(), console };
            open.card.paint_box(c, &mut self.text, &inputs, Some(&corner));
            self.paint_card_toast(c);
            return;
        }
        if self.card.as_ref().is_some_and(|o| o.card.info.is_some()) {
            // The info page (`game_info`): the cover as large as its room, the console for a game without one.
            let Some(open) = self.card.as_ref() else { return };
            let (bw, bh) = pocket_ui::game_info::cover_room(c.w, c.h, &open.card.data);
            let cover = self.cover(item, bw, bh);
            let console = self.item_rom(item).and_then(|(system, _)| pocket_ui::system_art::get(&system));
            let Some(open) = self.card.as_mut() else { return };
            open.card.pictures = pictures;
            open.card.paint_info(c, &mut self.text, cover.as_ref(), console, Some(&corner));
            self.paint_card_toast(c);
            return;
        }
        let (bw, bh) = game_card::cover_box();
        let cover = self.cover(item, bw, bh);
        let Some(open) = self.card.as_mut() else { return };
        open.card.pictures = pictures;
        open.card.paint(c, &mut self.text, cover.as_ref(), Some(&corner));
        // Curator's rows can be tapped (`app_curator`).
        self.curator_card_hits(c.w);
        self.paint_card_toast(c);
    }

    /// A toast over the card's top line.
    fn paint_card_toast(&mut self, c: &mut Canvas) {
        if let Some((msg, at)) = self.toast.clone() {
            if at.elapsed() < super::TOAST {
                use pocket_ui::text::Face;
                // The line takes the place of the system's name and the clock while it shows, so a long one ("Found 3
                // pictures for …", `app_identify`) is not read across them; a long game name is shortened to the screen.
                c.rect(0, 0, c.w, 104, pocket_ui::palette::pal().void);
                pocket_ui::text::boxes::painted_over(c, (0, 0, c.w, 104));
                let msg = self.text.fit(Face::Body, 28, &msg, c.w - 2 * pocket_ui::palette::MARGIN - 48);
                let w = self.text.width(Face::Body, 28, &msg) + 48;
                let h = pocket_ui::type_scale::row(52, 28);
                c.round_rect(c.w / 2 - w / 2, 36, w, h, h / 2, pocket_ui::palette::KEYCAP);
                self.text.draw(c, Face::Body, 28, c.w / 2 - w / 2 + 24, 36 + h / 2 + pocket_ui::type_scale::base(10, 28), pocket_ui::palette::CHALK, &msg);
            }
        }
    }

    /// The status of a game for the shelf's mark and label.
    pub(crate) fn item_status(&self, item: Item) -> Status {
        self.item_rom(item).map_or(Status::NotStarted, |(_, path)| self.library.status(&path))
    }
}

/// "Platform game" from "platform game".
fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// Letters and digits only, lowercase: whether Wikidata's name says the same as the file's.
fn words_key(s: &str) -> String {
    let s = s.strip_prefix("The ").unwrap_or(s);
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

impl App {
    /// `--snapshot menu:<kind>[:<row>]:battery=<case>` and `curator:card:<system>:<name>:battery=<case>`: the pause
    /// menu's battery line (for the snapshot's GBC game) or the open card's, from made-up measurements through the
    /// real wording (`battery_games`). `<case>`: `game` (this game's own), `system`, `all`, `measuring`, `charging`.
    pub(crate) fn battery_snapshot(&mut self, state: &str) {
        use pocket_ui::battery_games::{card_line, left, Basis, Rate};
        let Some(case) = state.split(':').find_map(|p| p.strip_prefix("battery=")) else { return };
        let t = pocket_ui::recents::now_secs();
        let rate = |per_hour: f64, basis| Some(Rate { per_hour, basis, secs: 3600.0 });
        let (rate, plugged) = match case {
            "system" => (rate(14.4, Basis::System), false),
            "all" => (rate(10.6, Basis::AllGames), false),
            "measuring" => (None, false),
            "charging" => (rate(18.0, Basis::Game), true),
            _ => (rate(18.0, Basis::Game), false),
        };
        let now = pocket_ui::battery::Sample {
            t,
            capacity: 60,
            charging: plugged,
            full: false,
            mv: 3900,
            ma: 500,
            temp: None,
            tag: 0,
        };
        match self.card.as_mut() {
            Some(open) => {
                let short = system_short(&self.systems, &open.system);
                open.card.data.battery = card_line(rate, Some(&now), t, &short);
            }
            None => self.menu_battery = left(rate, Some(&now), t).map(|l| l.line("GBC")),
        }
    }

    /// `--snapshot curator:card:<system>:<name>:patch`: the open card with a patch next to its game (`softpatch`); the
    /// snapshot's library has no files to find one.
    pub(crate) fn patch_snapshot(&mut self, state: &str) {
        if let Some(open) = self.card.as_mut().filter(|_| state.split(':').any(|p| p == "patch")) {
            let stem = open.path.file_stem().and_then(|s| s.to_str()).unwrap_or("Game").to_string();
            open.card.data.patch = Some(format!("{stem}.ips"));
        }
    }
}
