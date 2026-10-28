//! Part of the pocket-ui binary: the App's side of `pocket_ui::sync_conflicts` and `pocket_ui::sync_conflict_page`.
//! A thread looks for saves with two versions (after the start, after each game, when Settings → Sync opens, and
//! every few minutes on the shelf, never while a game runs); the App ties them to its games, puts a line on their
//! cards, tells once on the shelf, and runs the choice on a thread.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use pocket_ui::game_card::{CuratorRow, RowKind};
use pocket_ui::gfx::Canvas;
use pocket_ui::notice::Notice;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::sync::{self, Prefs};
use pocket_ui::sync_conflict_page::{self as page, ConflictPage, Entry, View};
use pocket_ui::sync_conflicts::{self as conflicts, Group, Kind, Layout, Owner, Version};
use pocket_ui::sync_page::Event;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, Item, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL};

/// The first look, once the start's own reading is done.
const FIRST_LOOK: Duration = Duration::from_secs(20);
/// On the shelf, Syncthing may bring a conflict in at any time (another device comes online).
const EVERY: Duration = Duration::from_secs(300);
/// After a game, a second look once Syncthing had time to pull what the other device changed meanwhile.
const AFTER_GAME_AGAIN: Duration = Duration::from_secs(30);
/// The conflict copies the shelf has told about, so each is told once.
const SEEN_FILE: &str = "sync-conflicts.json";

enum Msg {
    /// Look (again) in `after`, with these choices.
    Look { prefs: Prefs, after: Duration },
    /// A game started (true) or ended: no looking while one runs.
    Playing(bool),
}

static WATCH: OnceLock<Mutex<mpsc::Sender<Msg>>> = OnceLock::new();

fn send(m: Msg) {
    if let Some(tx) = WATCH.get() {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(m);
        }
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Seen {
    seen: Vec<PathBuf>,
}

/// What the App keeps of the saves with two versions.
#[derive(Default)]
pub(crate) struct ConflictsUi {
    /// The last search, tied to the library's games.
    pub found: Vec<Entry>,
    /// Conflict copies already told about on the shelf.
    seen: HashSet<PathBuf>,
    /// Something found that the shelf has not told about yet.
    untold: bool,
    pub page: Option<ConflictPage>,
    /// The page is over a game's card (else over Settings → Sync).
    from_card: bool,
}

impl ConflictsUi {
    /// Read with the rest of sync's state, after the first frame (`startup::Later`).
    pub fn load(data_dir: &Path) -> ConflictsUi {
        let seen: Seen = pocket_ui::fsutil::read_json_or_backup(&data_dir.join(SEEN_FILE)).unwrap_or_default();
        ConflictsUi {
            seen: seen.seen.into_iter().collect(),
            ..Default::default()
        }
    }

    fn save_seen(&self, data_dir: &Path) {
        let mut seen: Vec<PathBuf> = self.seen.iter().cloned().collect();
        seen.sort();
        pocket_ui::fsutil::write_json_soon(&data_dir.join(SEEN_FILE), &Seen { seen });
    }
}

fn copies(e: &Entry) -> impl Iterator<Item = &PathBuf> {
    e.group.others.iter().map(|v| &v.path)
}

/// Whether `e` shows on the card of the game `path` of `system`.
fn on_card(e: &Entry, system: &str, path: &Path) -> bool {
    match &e.owner {
        Owner::Game(p) => p == path,
        Owner::System(s) => s == system,
        Owner::Nobody => false,
    }
}

/// The watcher thread: looks when asked and every few minutes, never while a game runs, and reports a change.
fn watch(rx: mpsc::Receiver<Msg>, data_dir: PathBuf, report: impl Fn(Vec<Group>)) {
    let layout = Layout::device();
    let mut prefs = Prefs::load(&data_dir);
    let mut due = vec![Instant::now() + FIRST_LOOK];
    let (mut playing, mut was_on) = (false, false);
    let mut last: Vec<Group> = Vec::new();
    loop {
        let now = Instant::now();
        let asked = due.iter().min().copied();
        match rx.recv_timeout(asked.unwrap_or(now + EVERY).saturating_duration_since(now)) {
            Ok(Msg::Look { prefs: p, after }) => {
                prefs = p;
                due.push(Instant::now() + after);
                continue;
            }
            Ok(Msg::Playing(on)) => {
                playing = on;
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let now = Instant::now();
        due.retain(|d| *d > now);
        if playing {
            continue;
        }
        // Syncthing on: switched on here, or running by ROCKNIX's own setting (asked when looking was asked for, or
        // when it ran at the last look; not every few minutes for nothing).
        let on = prefs.enabled || ((asked.is_some() || was_on) && sync::running());
        was_on = on;
        let t = Instant::now();
        let groups = if on { conflicts::find(&conflicts::roots(&prefs, &layout), &layout) } else { Vec::new() };
        if groups != last {
            println!("sync: {} saves with two versions (looked in {:?})", groups.len(), t.elapsed());
            last = groups.clone();
            report(groups);
        }
    }
}

impl App {
    /// After the first frame: the thread that looks for saves with two versions (it waits a while before its first look).
    pub(crate) fn sync_conflicts_start(&mut self) {
        let (tx, rx) = mpsc::channel();
        if WATCH.set(Mutex::new(tx)).is_err() {
            return;
        }
        let (proxy, data_dir) = (self.proxy.clone(), self.data_dir.clone());
        let report = move |groups| {
            let _ = proxy.send_event(UserEvent::Sync(Event::Conflicts(groups)));
        };
        let _ = std::thread::Builder::new().name("sync-conflicts".into()).spawn(move || watch(rx, data_dir, report));
    }

    /// Look again (now, or after a game a little later too). Returns at once.
    pub(crate) fn sync_conflicts_look(&self, prefs: &Prefs, after_game: bool) {
        send(Msg::Playing(false));
        send(Msg::Look { prefs: prefs.clone(), after: Duration::ZERO });
        if after_game {
            send(Msg::Look { prefs: prefs.clone(), after: AFTER_GAME_AGAIN });
        }
    }

    /// A game starts: nothing is looked for until it ends.
    pub(crate) fn sync_conflicts_playing(&self) {
        send(Msg::Playing(true));
    }

    /// The groups the search found, tied to the library's games.
    fn conflict_entries(&self, groups: Vec<Group>) -> Vec<Entry> {
        let layout = Layout::device();
        let games: Vec<conflicts::Game> = self
            .systems
            .iter()
            .flat_map(|s| s.games.iter().map(move |g| conflicts::Game { system: &s.name, path: &g.path }))
            .collect();
        groups
            .into_iter()
            .map(|group| {
                let owner = conflicts::owner(&group, &games, &layout);
                let title = match &owner {
                    Owner::Game(p) => self
                        .systems
                        .iter()
                        .flat_map(|s| s.games.iter())
                        .find(|g| &g.path == p)
                        .map_or_else(|| group.name(), |g| g.name.clone()),
                    _ => group.name(),
                };
                Entry { group, owner, title }
            })
            .collect()
    }

    /// A search came back.
    pub(crate) fn conflicts_found(&mut self, groups: Vec<Group>) {
        let entries = self.conflict_entries(groups);
        let card = self.card.as_ref().map(|o| (o.system.clone(), o.path.clone()));
        let data_dir = self.data_dir.clone();
        let ui = &mut self.sync.conflicts;
        let present: HashSet<&PathBuf> = entries.iter().flat_map(copies).collect();
        let before = ui.seen.len();
        ui.seen.retain(|p| present.contains(p));
        if ui.seen.len() != before {
            ui.save_seen(&data_dir);
        }
        ui.untold = present.iter().any(|p| !ui.seen.contains(*p));
        ui.found = entries;
        if let Some(p) = ui.page.as_mut() {
            let list = match (&card, ui.from_card) {
                (Some((system, path)), true) => ui.found.iter().filter(|e| on_card(e, system, path)).cloned().collect(),
                _ => ui.found.clone(),
            };
            p.update(list);
        }
        self.conflicts_refresh();
    }

    /// The card's row and the Sync page's count follow the list.
    pub(crate) fn conflicts_refresh(&mut self) {
        let n = self.sync.conflicts.found.len();
        if let Some(p) = self.sync.page.as_mut() {
            p.waiting = n;
        }
        let Some((system, path)) = self.card.as_ref().map(|o| (o.system.clone(), o.path.clone())) else { return };
        let rows = self.card_rows(&system, &path);
        if let Some(open) = self.card.as_mut() {
            if open.card.data.rows.len() != rows.len() {
                open.card.focus = None;
            }
            open.card.data.rows = rows;
        }
    }

    /// The card's line: "Two versions of this game's save. Choose one…", when there are. Never waits for sync's
    /// state (read after the first frame).
    pub(crate) fn conflict_card_row(&self, system: &str, path: &Path) -> Option<CuratorRow> {
        let ui = &self.sync.loaded()?.conflicts;
        let mine: Vec<&Group> = ui.found.iter().filter(|e| on_card(e, system, path)).map(|e| &e.group).collect();
        Some(CuratorRow {
            kind: RowKind::SaveVersions,
            title: conflicts::what(mine.first()?.kind).into(),
            line: conflicts::card_line(&mine)?,
        })
    }

    /// A on the card's line: the versions of this game's saves.
    pub(crate) fn open_conflicts_for_card(&mut self) {
        let Some((system, path)) = self.card.as_ref().map(|o| (o.system.clone(), o.path.clone())) else { return };
        let ui = &mut self.sync.conflicts;
        let mine: Vec<Entry> = ui.found.iter().filter(|e| on_card(e, &system, &path)).cloned().collect();
        if mine.is_empty() {
            pocket_ui::ui_sound::refuse();
            return;
        }
        ui.page = Some(ConflictPage::new(mine, true));
        ui.from_card = true;
    }

    /// A on Settings → Sync → Two versions: every save waiting.
    pub(crate) fn open_conflicts_all(&mut self) {
        let ui = &mut self.sync.conflicts;
        ui.page = Some(ConflictPage::new(ui.found.clone(), false));
        ui.from_card = false;
    }

    /// The page is open over a card (Settings' one is painted by the Sync page).
    pub(crate) fn conflicts_over_card(&self) -> bool {
        self.running.is_none() && self.settings_page.is_none() && self.sync.loaded().is_some_and(|s| s.conflicts.page.is_some() && s.conflicts.from_card)
    }

    /// The page is open over Settings → Sync.
    pub(crate) fn conflicts_over_sync(&self) -> bool {
        self.sync.loaded().is_some_and(|s| s.conflicts.page.is_some() && !s.conflicts.from_card)
    }

    pub(crate) fn conflicts_mark(&self) -> Option<(usize, usize, bool, usize)> {
        self.sync.loaded()?.conflicts.page.as_ref().map(ConflictPage::mark)
    }

    pub(crate) fn conflicts_input(&mut self, b: Button) {
        let Some(p) = self.sync.conflicts.page.as_mut() else { return };
        match p.press(b, CONFIRM, CANCEL) {
            page::Action::Nothing => {}
            page::Action::Close => self.sync.conflicts.page = None,
            page::Action::Keep { entry, pick } => {
                let Some(group) = p.entries.get(entry).map(|e| e.group.clone()) else { return };
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let layout = Layout::device();
                    let folder = conflicts::day_folder(&layout, pocket_ui::recents::now_secs());
                    // Our .stignore learns the backup folder before anything is moved into it.
                    sync::refresh_saves_ignore();
                    let result = conflicts::keep(&group, pick, &folder, &layout).map(|kept| {
                        println!("sync: kept version {pick} of {}: {:?}", group.original.display(), kept.moved);
                        folder
                    });
                    if let Err(e) = &result {
                        println!("sync: keeping a version of {}: {e}", group.original.display());
                    }
                    let _ = proxy.send_event(UserEvent::Sync(Event::Kept { original: group.original, result }));
                });
            }
        }
    }

    /// The choice was carried out (or not).
    pub(crate) fn conflict_kept(&mut self, original: PathBuf, result: Result<PathBuf, String>) {
        let done = result.is_ok();
        let text = result.map(|folder| format!("Kept. The other version is in {}.", folder.display()));
        let ui = &mut self.sync.conflicts;
        if done {
            ui.found.retain(|e| e.group.original != original);
        }
        if let Some(p) = ui.page.as_mut() {
            match p.entries.iter().position(|e| e.group.original == original) {
                Some(i) => p.kept(i, text),
                None => p.busy = false,
            }
        }
        self.conflicts_refresh();
        if done {
            let prefs = self.sync.prefs.clone();
            self.sync_conflicts_look(&prefs, false);
        }
    }

    /// On the shelf, once: "A save has two versions: <game>" for what was found since the last time. Not while a game
    /// runs or starts, nor over another notice.
    pub(crate) fn sync_conflict_notice(&mut self) {
        if self.running.is_some() || self.launch_at.is_some() || self.notice.as_ref().is_some_and(Notice::active) {
            return;
        }
        if !self.sync.loaded().is_some_and(|s| s.conflicts.untold) {
            return;
        }
        let data_dir = self.data_dir.clone();
        let ui = &mut self.sync.conflicts;
        let new: Vec<&Entry> = ui.found.iter().filter(|e| copies(e).any(|p| !ui.seen.contains(p))).collect();
        let text = page::notice(&new);
        let all: Vec<PathBuf> = ui.found.iter().flat_map(copies).cloned().collect();
        ui.seen.extend(all);
        ui.untold = false;
        ui.save_seen(&data_dir);
        if let Some(text) = text {
            println!("sync: told on the shelf: {text}");
            self.notice = Some(Notice::new(text));
        }
    }

    /// The page: the list of saves waiting, or one save's versions.
    pub(crate) fn paint_conflicts(&mut self, c: &mut Canvas) {
        let Some(p) = self.sync.conflicts.page.take() else { return };
        c.clear(pal().void);
        let pad = NU_PAD;
        let (left, right) = (pad + NU_BTN_PAD, c.w - pad - NU_BTN_PAD);
        self.status_pill(c);
        let n_versions = p.entry().map_or(2, |e| e.group.versions().len());
        let title = if n_versions == 2 { "Two versions".to_string() } else { format!("{n_versions} versions") };
        self.text.draw(c, Face::Title, 50, left, pad + 58, pal().chalk, &title);
        let top = pad + NU_PILL + 24;
        match (p.view, p.entry()) {
            (View::Versions(_), Some(e)) => {
                self.paint_conflict_versions(c, &p, e, top, left, right);
                self.nu_hints(c, &[("B", "Back"), ("A", "Keep this one")]);
            }
            _ => {
                self.paint_conflict_list(c, &p, top, left, right);
                let hints: &[(&str, &str)] = if p.entries.is_empty() { &[("B", "Back")] } else { &[("B", "Back"), ("A", "Choose")] };
                self.nu_hints(c, hints);
            }
        }
        self.sync.conflicts.page = Some(p);
    }

    fn paint_conflict_list(&mut self, c: &mut Canvas, p: &ConflictPage, top: i32, left: i32, right: i32) {
        let mut y = top;
        // The last choice's outcome, until B.
        if let Some((msg, failed)) = &p.message {
            for line in self.text.wrap(Face::Body, 28, msg, right - left) {
                self.text.draw(c, Face::Body, 28, left, y + 28, if *failed { pal().alert } else { pal().chalk }, &line);
                y += pocket_ui::type_scale::line(28, 10);
            }
            y += 24;
        }
        if p.entries.is_empty() {
            let line = if p.message.is_some() { "Nothing else is waiting for a choice." } else { "Nothing is waiting for a choice." };
            self.text.draw(c, Face::Body, 28, left, y + 28 + pocket_ui::type_scale::grow(28), pal().dust, line);
            y += 64 + pocket_ui::type_scale::grow(28);
            for line in self.text.wrap(Face::Body, 26, "When a save changes on two devices before they sync, both versions show here.", right - left) {
                self.text.draw(c, Face::Body, 26, left, y, pal().dust, &line);
                y += pocket_ui::type_scale::line(26, 8);
            }
            return;
        }
        let (row, font) = (pocket_ui::type_scale::row(64, 34), 34u32);
        let x0 = NU_PAD;
        for (i, e) in p.entries.iter().enumerate() {
            let ry = y + i as i32 * row;
            self.hits.row(Area::new(x0, ry, c.w - 2 * x0, row), i);
            let base = ry + row / 2 + pocket_ui::type_scale::mid(font);
            let kind = conflicts::what(e.group.kind);
            let kind_w = self.text.width(Face::Body, 28, kind);
            let name = self.text.fit(Face::Body, font, &e.title, right - left - kind_w - 40);
            let (fg, side) = if i == p.sel {
                let w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD;
                c.round_rect(x0, ry + 4, c.w - 2 * x0, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(x0, ry + 4, w, row - 8, (row - 8) / 2, pal().select);
                (pal().ink, pal().text)
            } else {
                (pal().text, pal().dust)
            };
            self.text.draw(c, Face::Body, font, left, base, fg, &name);
            self.text.draw_right(c, Face::Body, 28, right, base, side, kind);
        }
        self.hits.list(p.sel, p.entries.len(), 0.0, row as f32);
        y += p.entries.len() as i32 * row + 36;
        for line in self.text.wrap(Face::Body, 26, "Saves changed on two devices before they could sync. Choose which version to keep.", right - left) {
            self.text.draw(c, Face::Body, 26, left, y, pal().dust, &line);
            y += pocket_ui::type_scale::line(26, 8);
        }
    }

    fn paint_conflict_versions(&mut self, c: &mut Canvas, p: &ConflictPage, e: &Entry, top: i32, left: i32, right: i32) {
        // Each line lower by its type's growth with Settings → Text size (0 in Normal).
        let mut y = top + 36 + pocket_ui::type_scale::grow(37);
        let name = self.text.fit(Face::Title, 37, &e.title, right - left);
        self.text.draw(c, Face::Title, 37, left, y, pal().chalk, &name);
        y += 44 + pocket_ui::type_scale::grow(28);
        // What it is, and the file: "Game save  Pokémon Emerald.srm".
        let what = conflicts::what(e.group.kind);
        let w = self.text.draw(c, Face::Body, 28, left, y, pal().dust, what);
        // A memory card or save data no game owns has the file's name as its title already.
        if e.title != e.group.name() {
            let file = self.text.fit(Face::Body, 28, &e.group.name(), right - left - w - 24);
            self.text.draw(c, Face::Body, 28, left + w + 24, y, pal().dust, &file);
        }
        y += 20;
        if let Some(warn) = page::card_warning(e.group.kind) {
            y += 20;
            for line in self.text.wrap(Face::Body, 28, warn, right - left) {
                y += pocket_ui::type_scale::line(28, 8);
                self.text.draw(c, Face::Body, 28, left, y, pal().amber, &line);
            }
        }
        y += 30;
        let rows = page::version_rows(&e.group);
        let (row, x0) = (pocket_ui::type_scale::row(84, 37), NU_PAD);
        let date_x = left + pocket_ui::type_scale::wide(360, 37);
        for (i, r) in rows.iter().enumerate() {
            let ry = y + i as i32 * row;
            self.hits.row(Area::new(x0, ry, c.w - 2 * x0, row), i);
            let base = ry + row / 2 + pocket_ui::type_scale::base(12, 37);
            let selected = i == p.sel;
            if selected {
                c.round_rect(x0, ry + 6, c.w - 2 * x0, row - 12, (row - 12) / 2, pal().select);
            }
            let (fg, dim) = if selected { (pal().ink, pal().ink) } else { (pal().text, pal().text) };
            let lw = self.text.draw(c, Face::Title, 37, left, base + 2, fg, &r.label);
            if r.newest {
                self.text.draw(c, Face::Body, 24, left + lw + 18, base, if selected { pal().ink } else { pal().amber }, "Newer");
            }
            self.text.draw(c, Face::Body, 28, date_x, base, dim, &r.when);
            self.text.draw_right(c, Face::Body, 28, right, base, dim, &r.size);
        }
        self.hits.list(p.sel, rows.len(), 0.0, row as f32);
        y += rows.len() as i32 * row + 40 + pocket_ui::type_scale::grow(26);
        let folder = conflicts::day_folder(&Layout::device(), pocket_ui::recents::now_secs());
        let explain = page::keep_text(&folder.display().to_string());
        // Above the hints; with larger text it may end with "…" (a failed keep below still shows).
        let bottom = c.h - NU_PAD - NU_PILL - 16 - if matches!(&p.message, Some((_, true))) || p.busy { pocket_ui::type_scale::line(28, 8) + 22 } else { 0 };
        y = self.text.paragraph(c, Face::Body, 26, (left, y), right - left, pocket_ui::type_scale::line(26, 8), bottom, pal().dust, &explain);
        y += 12;
        if p.busy {
            self.text.draw(c, Face::Body, 28, left, y + 10, pal().dust, "Keeping…");
        } else if let Some((msg, true)) = &p.message {
            for line in self.text.wrap(Face::Body, 28, msg, right - left) {
                self.text.draw(c, Face::Body, 28, left, y + 10, pal().alert, &line);
                y += pocket_ui::type_scale::line(28, 8);
            }
        }
    }

    /// `--snapshot conflicts:<card|choose|notice|none>[:<system>][:<row>]`: a game's card with its line, the
    /// versions over it, the shelf's notice, the list with nothing in it; made-up versions of the save of the first
    /// game of `<system>` (gba by default; ps2 gives the memory card, which every PS2 game shares).
    pub(crate) fn conflicts_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("conflicts:") else { return false };
        let mut parts = rest.split(':');
        let what = parts.next().unwrap_or_default();
        let system = parts.next().filter(|s| !s.is_empty()).unwrap_or("gba").to_string();
        let row: usize = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        if what == "none" {
            // Settings → Sync → Two versions with nothing found (A does nothing there; the page is opened directly).
            self.sync.page = Some(pocket_ui::sync_page::SyncPage::new(self.sync.prefs.clone(), None));
            self.sync.conflicts.page = Some(ConflictPage::new(Vec::new(), false));
            self.sync.conflicts.from_card = false;
            self.settings_page = Some(super::SettingsPage::Sync);
            return true;
        }
        let Some((s, g)) = self.systems.iter().enumerate().find(|(_, s)| s.name == system).and_then(|(i, s)| (!s.games.is_empty()).then_some((i, 0))) else {
            eprintln!("conflicts:{rest}: no {system} game in the library");
            return true;
        };
        let (name, path) = (self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
        let groups = vec![demo_group(&system, &path)];
        self.conflicts_found(groups);
        match what {
            "notice" => {}
            _ => {
                self.show_card(Item::Game(s, g), system, name, path);
                if what == "choose" {
                    self.open_conflicts_for_card();
                    if let Some(p) = self.sync.conflicts.page.as_mut() {
                        p.sel = row.min(p.len().saturating_sub(1));
                    }
                } else if let Some(open) = self.card.as_mut() {
                    open.card.focus_row(0);
                }
            }
        }
        true
    }

    /// `sync:conflicts[:list|:card]`: Settings → Sync with two saves waiting, the row selected; their list; the memory
    /// card's versions opened from it.
    pub(crate) fn sync_conflicts_snapshot(&mut self, mode: &str) {
        // The first GBA game's save and the PS2 memory card, for the games the library has.
        let groups: Vec<Group> = ["gba", "ps2"]
            .into_iter()
            .filter_map(|system| {
                let game = self.systems.iter().find(|s| s.name == system).and_then(|s| s.games.first())?;
                Some(demo_group(system, &game.path))
            })
            .collect();
        self.conflicts_found(groups);
        if let Some(p) = self.sync.page.as_mut() {
            p.sel = 2;
        }
        if mode == "list" || mode == "card" {
            self.open_conflicts_all();
            if mode == "card" {
                if let Some(p) = self.sync.conflicts.page.as_mut() {
                    p.press(Button::Down, CONFIRM, CANCEL);
                    p.press(CONFIRM, CONFIRM, CANCEL);
                }
            }
        }
    }
}

/// A made-up conflict for the snapshots: the game's save (or, for PS2, the memory card), with this Pocket's version
/// from yesterday evening and the other device's from the afternoon.
fn demo_group(system: &str, game: &Path) -> Group {
    let (original, kind) = if system == "ps2" {
        (PathBuf::from("/storage/roms/ps2/Mcd001.ps2"), Kind::Card)
    } else {
        (game.with_extension("srm"), Kind::Save)
    };
    let name = original.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, ""));
    let copy = original.with_file_name(format!("{stem}.sync-conflict-20261024-181502-K3QW7ZD.{ext}"));
    let size = if kind == Kind::Card { 8 * 1024 * 1024 + 16 * 1024 } else { 128 * 1024 };
    // 24 October 2026, 18:02 and 15:40 UTC.
    let (mine, theirs) = (1_792_864_920, 1_792_856_400);
    Group {
        current: Some(Version { path: original.clone(), mtime: mine, size }),
        others: vec![Version { path: copy, mtime: theirs, size }],
        original,
        kind,
        previews: Vec::new(),
    }
}
