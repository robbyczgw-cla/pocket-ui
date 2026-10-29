//! The App's side of the starter pack (`pocket_ui::starter`, docs/STARTER-PACK.md): Settings → Games → Starter pack,
//! and the calm hint on Home while the card has no games at all ("Get the starter pack", A).
//!
//! The page lists the pack's games, each with its size or "On your card"; A downloads one, or "Get all". A download
//! runs on a thread of its own (the Store's curl, the SHA-256 checked before anything is placed) and reports back as
//! `UserEvent::Starter`; B leaves the page while it runs. When a game has been added the library is read again
//! (`library_rescan`, `app_library.rs`), so the games appear on the shelf by themselves. Everything that touches the
//! card (finding the folders, what is there already, free space) happens on that thread, never on the UI's.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use pocket_ui::gfx::Canvas;
use pocket_ui::palette::*;
use pocket_ui::pad::Button;
use pocket_ui::starter::{self, Failure, Game, Outcome};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use crate::store_page::{row_h, Marker};
use crate::{App, Screen, SettingsPage, UserEvent, CANCEL, CONFIRM, EMPTY_ART_TOP, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name of the Settings row.
pub(crate) const SETTINGS_NAME: &str = "Starter pack";

#[derive(Debug)]
pub enum Event {
    /// The folders of the pack's systems, and which games are in them already.
    Scanned { folders: BTreeMap<String, PathBuf>, there: BTreeMap<String, bool> },
    /// The thread began on this game.
    Started(String),
    /// Bytes of the download so far.
    Progress(u64),
    /// One game is done, or why not.
    Done { id: String, result: Result<Outcome, Failure> },
    /// The whole job ended (all done, or stopped at the first thing that needs the player).
    Finished,
}

/// A running job: which game it is on, how far, and how many there are.
struct Busy {
    id: String,
    title: String,
    position: usize,
    count: usize,
    total: u64,
    done: u64,
    /// For `update::busy_blocks_sleep`: a stuck job stops keeping the Pocket awake after a while.
    since: Instant,
}

#[derive(Default)]
pub(crate) struct StarterUi {
    /// The folder of each system the pack has games for and the shelf knows (`es_systems.cfg`).
    folders: BTreeMap<String, PathBuf>,
    /// Game id -> whether its file is in the folder.
    there: BTreeMap<String, bool>,
    scanned: bool,
    sel: usize,
    busy: Option<Busy>,
    /// What the last job said: text, and whether it is a problem.
    message: Option<(String, bool)>,
    /// Opened from Home's hint: B goes back to Home, not to Settings.
    from_home: bool,
    /// Games added by the job that runs or ran last.
    added: usize,
}

/// A row of the page.
#[derive(Clone, Copy)]
enum Row {
    All,
    Game(&'static Game),
}

impl Busy {
    /// "39 %", or "…" while the size is not known.
    fn percent(&self) -> String {
        (self.done.min(self.total) * 100).checked_div(self.total).map_or("…".to_string(), |p| format!("{p}%"))
    }
}

impl StarterUi {
    pub(crate) fn busy_since(&self) -> Option<Instant> {
        self.busy.as_ref().map(|b| b.since)
    }

    /// What changes when the page moves, for the UI sounds.
    pub(crate) fn mark(&self) -> (usize, bool) {
        (self.sel, self.busy.is_some())
    }

    /// The games the page offers: all of them until the folders are known, then those whose system the shelf has.
    fn games(&self) -> Vec<&'static Game> {
        starter::games().iter().filter(|g| !self.scanned || self.folders.contains_key(&g.system)).collect()
    }

    fn rows(&self) -> Vec<Row> {
        std::iter::once(Row::All).chain(self.games().into_iter().map(Row::Game)).collect()
    }

    fn is_there(&self, g: &Game) -> bool {
        self.there.get(&g.id).copied().unwrap_or(false)
    }

    fn missing(&self) -> Vec<&'static Game> {
        self.games().into_iter().filter(|g| !self.is_there(g)).collect()
    }

    /// "Getting Tobu Tobu Girl (2 of 5), 25 %" while a job runs.
    fn busy_text(&self) -> Option<String> {
        let b = self.busy.as_ref()?;
        let pct = b.percent();
        Some(if b.count > 1 { format!("Getting {} ({} of {}), {}", b.title, b.position, b.count, pct) } else { format!("Getting {}, {}", b.title, pct) })
    }
}

/// What a job that added `n` games says at its end.
fn added_text(n: usize) -> String {
    if n == 1 { "Added 1 game. B shows your shelf.".to_string() } else { format!("Added {n} games. B shows your shelf.") }
}

fn short_system(id: &str) -> &'static str {
    match id {
        "gb" => "Game Boy",
        "gbc" => "Game Boy Color",
        "gba" => "GBA",
        "nes" => "NES",
        "snes" => "SNES",
        "megadrive" => "Mega Drive",
        _ => "",
    }
}

impl App {
    /// Whether Home shows the starter pack's hint: the card has no games at all (and the list could be read).
    pub(crate) fn starter_home(&self) -> bool {
        self.screen == Screen::Home && self.load_error.is_none() && self.systems.is_empty() && !starter::games().is_empty()
    }

    /// Settings → Games → Starter pack (`from_home`: opened by A on Home's hint).
    pub(crate) fn open_starter(&mut self, from_home: bool) -> Option<SettingsPage> {
        self.starter.from_home = from_home;
        self.starter.sel = 0;
        if self.starter.busy.is_none() {
            self.starter.message = None;
        }
        self.starter_scan();
        Some(SettingsPage::Starter)
    }

    /// Reads, on a thread, where each system's games live and which of the pack's are there.
    fn starter_scan(&mut self) {
        let (proxy, es) = (self.proxy.clone(), self.es_systems.clone());
        let _ = std::thread::Builder::new().name("starter-scan".into()).spawn(move || {
            let known = pocket_ui::library::all_systems(&es);
            let mut folders = BTreeMap::new();
            for g in starter::games() {
                let path = match &known {
                    Ok(list) => list.iter().find(|s| s.id == g.system).map(|s| s.path.clone()),
                    // No es_systems.cfg to read (a computer): the places ROCKNIX uses.
                    Err(_) => Some(PathBuf::from("/storage/roms").join(&g.system)),
                };
                if let Some(path) = path {
                    folders.insert(g.system.clone(), path);
                }
            }
            let there = starter::scan(&folders);
            let _ = proxy.send_event(UserEvent::Starter(Event::Scanned { folders, there }));
        });
    }

    /// Starts adding `games` on a thread. One job at a time.
    fn starter_start(&mut self, games: Vec<&'static Game>) {
        if self.starter.busy.is_some() || games.is_empty() {
            return;
        }
        let count = games.len();
        self.starter.added = 0;
        self.starter.message = None;
        self.starter.busy = Some(Busy { id: games[0].id.clone(), title: games[0].title.clone(), position: 1, count, total: games[0].file.size, done: 0, since: Instant::now() });
        let (proxy, folders, work) = (self.proxy.clone(), self.starter.folders.clone(), self.data_dir.join("starter"));
        let _ = std::thread::Builder::new().name("starter".into()).spawn(move || {
            let send = |e| {
                let _ = proxy.send_event(UserEvent::Starter(e));
            };
            // A cheap "is there a network at all" first (a process, up to a second: here, not on the UI's thread).
            let online = pocket_ui::wifi::ip_now().is_some();
            for g in games {
                send(Event::Started(g.id.clone()));
                let Some(dest) = folders.get(&g.system) else { continue };
                let env = starter::Env { dest_dir: dest, work_dir: &work, free_dest: starter::free_bytes(dest), free_work: starter::free_bytes(&work), online };
                let result = starter::install(g, &env, &starter::curl, &|n| send(Event::Progress(n)));
                // Without a network, or without room, the next game would meet the same wall.
                let stop = matches!(result, Err(Failure::Offline | Failure::NoSpace { .. }));
                send(Event::Done { id: g.id.clone(), result });
                if stop {
                    break;
                }
            }
            send(Event::Finished);
        });
    }

    pub(crate) fn starter_event(&mut self, e: Event) {
        match e {
            Event::Scanned { folders, there } => {
                self.starter.folders = folders;
                self.starter.there = there;
                self.starter.scanned = true;
                self.starter.sel = self.starter.sel.min(self.starter.rows().len().saturating_sub(1));
            }
            Event::Started(id) => {
                if let (Some(b), Some(g)) = (self.starter.busy.as_mut(), starter::games().iter().find(|g| g.id == id)) {
                    if b.id != id {
                        b.position += 1;
                    }
                    b.id = id;
                    b.title = g.title.clone();
                    b.total = g.file.size;
                    b.done = 0;
                }
            }
            Event::Progress(n) => {
                if let Some(b) = self.starter.busy.as_mut() {
                    b.done = n;
                }
            }
            Event::Done { id, result } => {
                let title = starter::games().iter().find(|g| g.id == id).map_or(id.clone(), |g| g.title.clone());
                match result {
                    Ok(Outcome::Added(path)) => {
                        println!("starter pack: added {} ({})", title, path.display());
                        self.starter.there.insert(id, true);
                        self.starter.added += 1;
                        self.starter.message = Some((format!("{title} added."), false));
                        // The shelf reads the folders again and shows it.
                        self.starter_library_changed();
                    }
                    Ok(Outcome::AlreadyThere(_)) => {
                        self.starter.there.insert(id, true);
                        self.starter.message = Some((format!("{title} is on your SD card already. Nothing was changed."), false));
                    }
                    Err(f) => {
                        println!("starter pack: {title}: {f:?}");
                        self.starter.message = Some((f.message(), true));
                    }
                }
            }
            Event::Finished => {
                self.starter.busy = None;
                let added = self.starter.added;
                if added > 0 && !self.starter.message.as_ref().is_some_and(|m| m.1) {
                    self.starter.message = Some((added_text(added), false));
                }
                if added > 0 {
                    // Once more, in case the first read was still running when the last game arrived.
                    self.starter_library_changed();
                }
            }
        }
    }

    /// Games arrived: read the folders again (on a thread); the list that comes back is swapped in at the next moment
    /// nothing on screen holds a game by its place (`library_apply_pending`), which is when the page is left.
    fn starter_library_changed(&mut self) {
        if pocket_ui::library_cache::enabled() {
            self.library_rescan("starter pack");
        } else if let Ok(systems) = pocket_ui::library::load_shown(&self.es_systems, &self.data_dir) {
            self.systems = systems;
            self.recents.follow_library(&self.systems);
            self.curator.library_changed();
            self.find_library_changed();
            self.rebuild();
        }
    }

    pub(crate) fn starter_input(&mut self, b: Button) -> Option<SettingsPage> {
        let rows = self.starter.rows();
        let n = rows.len();
        // Moving puts away what the last job said (a running job keeps its line).
        if matches!(b, Button::Up | Button::Down | Button::Left | Button::Right | Button::L1 | Button::R1) && self.starter.busy.is_none() {
            self.starter.message = None;
        }
        match b {
            Button::Up | Button::Left if n > 0 => self.starter.sel = (self.starter.sel + n - 1) % n,
            Button::Down | Button::Right if n > 0 => self.starter.sel = (self.starter.sel + 1) % n,
            Button::L1 => self.starter.sel = self.starter.sel.saturating_sub(5),
            Button::R1 if n > 0 => self.starter.sel = (self.starter.sel + 5).min(n - 1),
            b if b == CANCEL => {
                return if self.starter.from_home { None } else { Some(SettingsPage::List(crate::settings_row(SETTINGS_NAME))) };
            }
            b if b == CONFIRM && self.starter.busy.is_none() && self.starter.scanned => match rows.get(self.starter.sel).copied() {
                Some(Row::All) => {
                    let missing = self.starter.missing();
                    if missing.is_empty() {
                        self.starter.message = Some(("Every game in the pack is on your SD card.".into(), false));
                    } else {
                        self.starter_start(missing);
                    }
                }
                Some(Row::Game(g)) if self.starter.is_there(g) => {
                    self.starter.message = Some((format!("{} is on your SD card already.", g.title), false));
                }
                Some(Row::Game(g)) => self.starter_start(vec![g]),
                None => {}
            },
            _ => {}
        }
        Some(SettingsPage::Starter)
    }

    /// A on Home's hint (the card has no games): the page, with B coming back to Home.
    pub(crate) fn starter_home_press(&mut self, b: Button) -> bool {
        if b == CONFIRM && self.starter_home() && self.settings_page.is_none() {
            self.settings_page = self.open_starter(true);
            return true;
        }
        false
    }

    /// Home with no games: what the card needs, and the one thing to press.
    pub(crate) fn paint_starter_home(&mut self, c: &mut Canvas, with_hints: bool) {
        // An empty shelf with a plant above the words (`ui_art`); the words move down as far as it needs.
        let dy = pocket_ui::ui_art::above_words(c, pocket_ui::ui_art::Art::NoGames, MARGIN, false, EMPTY_ART_TOP, 300, 50);
        self.text.draw(c, Face::Title, 50, MARGIN, 300 + dy, pal().chalk, "No games yet");
        let max_w = c.w - 2 * MARGIN;
        let lines = self.text.wrap(Face::Body, 28, "Get a few free games to start playing right away.", max_w);
        let mut y = 350 + dy;
        for line in &lines {
            self.text.draw(c, Face::Body, 28, MARGIN, y, pal().dust, line);
            y += 38;
        }
        for line in self.text.wrap(Face::Body, 28, "Or copy your own into /storage/roms/<system>. Settings › Games › Add games shows where.", max_w) {
            self.text.draw(c, Face::Body, 28, MARGIN, y, pal().dust, &line);
            y += pocket_ui::type_scale::line(28, 10);
        }
        y += 24;
        let busy = self.starter.busy_text();
        if let Some(text) = busy {
            // A job is running (the page was left meanwhile): say so, calmly, instead of offering it again.
            self.text.draw(c, Face::Body, 32, MARGIN, y + 36, pal().chalk, &text);
        } else {
            let label = "Get the starter pack";
            let w = self.text.width(Face::Body, 34, label) + 2 * NU_BTN_PAD;
            let h = 72;
            c.round_rect(MARGIN, y, w, h, h / 2, pal().select);
            self.text.draw(c, Face::Body, 34, MARGIN + NU_BTN_PAD, y + h / 2 + 12, pal().ink, label);
            self.hits.button(Area::new(MARGIN, y, w, h), CONFIRM);
            // Said before anyone tries: the games come over the internet.
            let note = if pocket_ui::wifi::ip().is_none() {
                "Needs Wi-Fi. Connect in Settings › Connections › Wi-Fi first.".to_string()
            } else {
                format!("{} small games, free to share, from their authors.", starter::games().len())
            };
            self.text.draw(c, Face::Body, 24, MARGIN, y + h + 44, pal().dust, &note);
            if let Some((m, true)) = &self.starter.message {
                let m = m.clone();
                self.text.draw(c, Face::Body, 26, MARGIN, y + h + 90, pal().alert, &m);
            }
        }
        if with_hints {
            // Not offered again while it is under way.
            let hints: &[(&str, &str)] = if self.starter.busy.is_some() { &[("START", "Settings")] } else { &[("A", "Get the starter pack"), ("START", "Settings")] };
            self.paint_hint_row(c, hints);
        }
    }

    pub(crate) fn paint_starter(&mut self, c: &mut Canvas) {
        c.clear(pal().void);
        self.sheet_header(c, "Starter pack");
        let top = NU_PAD + NU_PILL + 12;
        let rows = self.starter.rows();
        let sel = self.starter.sel.min(rows.len().saturating_sub(1));
        let visible = ((c.h - top - NU_PILL - 190) / row_h()).max(1) as usize;
        let first = crate::window_start(sel, visible, rows.len());
        let missing = self.starter.missing();
        let (busy_id, busy_pct) = match &self.starter.busy {
            Some(b) => (Some(b.id.clone()), b.percent()),
            None => (None, String::new()),
        };
        for (i, row) in rows.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row_h();
            let (label, marker) = match row {
                Row::All if !self.starter.scanned => ("Get all games".to_string(), Marker::None),
                Row::All if rows.len() == 1 => ("Nothing to get here".to_string(), Marker::None),
                Row::All if missing.is_empty() => ("All games are on your SD card".to_string(), Marker::None),
                Row::All => (format!("Get all {} games", missing.len()), Marker::Text(starter::size_label(missing.iter().map(|g| g.file.size).sum()))),
                Row::Game(g) => {
                    let marker = if busy_id.as_deref() == Some(g.id.as_str()) {
                        Marker::Text(busy_pct.clone())
                    } else if self.starter.is_there(g) {
                        Marker::Text("On your SD card".into())
                    } else {
                        Marker::Text(format!("{} · {}", short_system(&g.system), starter::size_label(g.file.size)))
                    };
                    (g.title.clone(), marker)
                }
            };
            self.store_row(c, y, &label, i == sel, marker);
            self.hits.row(Area::new(NU_PAD, y, c.w - 2 * NU_PAD, row_h()), i);
        }
        self.hits.list(sel, rows.len(), 0.0, row_h() as f32);

        // Under the list: progress, what the last job said, or what the selected game is.
        let lines: Vec<(String, u32)> = if let Some(text) = self.starter.busy_text() {
            let (w, by) = (c.w - 2 * NU_PAD - 2 * NU_BTN_PAD, c.h - NU_PAD - NU_PILL - 110);
            let b = self.starter.busy.as_ref();
            let frac = b.map_or(0.0, |b| if b.total > 0 { (b.done as f32 / b.total as f32).clamp(0.0, 1.0) } else { 0.0 });
            c.round_rect(NU_PAD + NU_BTN_PAD, by, w, 12, 6, pal().keycap);
            c.round_rect(NU_PAD + NU_BTN_PAD, by, ((w as f32 * frac) as i32).max(12), 12, 6, pal().label);
            vec![(text, pal().chalk)]
        } else if let Some((m, err)) = self.starter.message.clone() {
            vec![(m, if err { pal().alert } else { pal().chalk })]
        } else if !self.starter.scanned {
            vec![("Looking at your SD card…".to_string(), pal().dust)]
        } else {
            match rows.get(sel) {
                _ if rows.len() == 1 => vec![("This Pocket has none of the systems the pack's games are for.".to_string(), pal().chalk)],
                Some(Row::Game(g)) => {
                    vec![(g.description.clone(), pal().chalk), (format!("By {} · {}", g.author, g.license_short), pal().dust)]
                }
                _ => vec![("Free games their authors share for anyone to play. They come over the internet and are added only when you press A.".to_string(), pal().chalk)],
            }
        };
        self.starter_footer(c, &lines);
        let hint: &[(&str, &str)] = if self.starter.busy.is_some() {
            &[("B", "Back")]
        } else {
            match rows.get(sel) {
                Some(Row::All) if missing.is_empty() => &[("B", "Back")],
                Some(Row::All) => &[("B", "Back"), ("A", "Get all")],
                Some(Row::Game(g)) if self.starter.is_there(g) => &[("B", "Back")],
                _ => &[("B", "Back"), ("A", "Get")],
            }
        };
        self.nu_hints(c, hint);
    }

    /// Wrapped lines, centred, ending just above the hint bar.
    fn starter_footer(&mut self, c: &mut Canvas, lines: &[(String, u32)]) {
        let max_w = c.w - 2 * NU_PAD - 2 * NU_BTN_PAD;
        let mut all: Vec<(String, u32)> = Vec::new();
        for (text, colour) in lines {
            for l in self.text.wrap(Face::Body, NU_SMALL, text, max_w) {
                all.push((l, *colour));
            }
        }
        let mut y = c.h - NU_PAD - NU_PILL - 30 - 40 * (all.len() as i32 - 1).max(0);
        for (line, colour) in &all {
            let w = self.text.width(Face::Body, NU_SMALL, line);
            self.text.draw(c, Face::Body, NU_SMALL, (c.w - w) / 2, y, *colour, line);
            y += 40;
        }
    }

    /// `pocket-ui --snapshot starter[:<row>[:<state>]]`: the page with the folders under `data_dir/starter-roms` (or the
    /// ones in es_systems), the row selected, and a state: `busy` (a download at 40 %), `there` (the first game
    /// is on the card), `offline`, `failed`. `starter-home`: Home with no games.
    pub(crate) fn starter_snapshot(&mut self, state: &str) -> bool {
        let mut parts = state.split(':');
        match parts.next() {
            Some("starter-home") => {
                self.systems.clear();
                self.screen = Screen::Home;
                self.rebuild();
                if parts.next() == Some("busy") {
                    self.starter_fake_busy();
                }
                true
            }
            Some("starter") => {
                self.settings_page = Some(SettingsPage::Starter);
                let root = self.data_dir.join("starter-roms");
                for g in starter::games() {
                    self.starter.folders.insert(g.system.clone(), root.join(&g.system));
                }
                self.starter.scanned = true;
                self.starter.sel = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
                match parts.next() {
                    Some("busy") => self.starter_fake_busy(),
                    Some("there") => {
                        for g in starter::games().iter().take(3) {
                            self.starter.there.insert(g.id.clone(), true);
                        }
                        self.starter.message = Some((added_text(3), false));
                    }
                    Some("offline") => self.starter.message = Some((Failure::Offline.message(), true)),
                    Some("failed") => self.starter.message = Some((Failure::Damaged.message(), true)),
                    _ => {}
                }
                true
            }
            _ => false,
        }
    }

    fn starter_fake_busy(&mut self) {
        if let Some(g) = starter::games().get(1) {
            self.starter.busy = Some(Busy { id: g.id.clone(), title: g.title.clone(), position: 2, count: 4, total: g.file.size, done: g.file.size * 2 / 5, since: Instant::now() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_offers_all_then_each_game_of_a_system_the_shelf_has() {
        let mut ui = StarterUi::default();
        assert_eq!(ui.rows().len(), 1 + starter::games().len(), "before the folders are known every game is listed");
        ui.scanned = true;
        assert_eq!(ui.rows().len(), 1, "no system known: only the 'all' row");
        ui.folders.insert("gb".into(), PathBuf::from("/x/gb"));
        let n = starter::games().iter().filter(|g| g.system == "gb").count();
        assert_eq!(ui.rows().len(), 1 + n);
        assert_eq!(ui.missing().len(), n);
        let first = starter::games().iter().find(|g| g.system == "gb").unwrap();
        ui.there.insert(first.id.clone(), true);
        assert_eq!(ui.missing().len(), n - 1);
    }

    #[test]
    fn progress_reads_in_percent_and_in_order() {
        let mut ui = StarterUi::default();
        assert_eq!(ui.busy_text(), None);
        ui.busy = Some(Busy { id: "a".into(), title: "Tobu Tobu Girl".into(), position: 2, count: 5, total: 200, done: 50, since: Instant::now() });
        assert_eq!(ui.busy_text().unwrap(), "Getting Tobu Tobu Girl (2 of 5), 25%");
        ui.busy.as_mut().unwrap().count = 1;
        assert_eq!(ui.busy_text().unwrap(), "Getting Tobu Tobu Girl, 25%");
    }
}
