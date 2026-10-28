//! Part of the pocket-ui binary: the App's side of `pocket_ui::storage`, `pocket_ui::storage_page` and
//! `pocket_ui::game_files`.
//!
//! * Settings → System → **Storage**: the card measured on a thread (`storage::measure`, kept for the next visit, done
//!   again after a game, a delete and each time the page opens, `storage::measure_on_open`: the old numbers show at
//!   once with a quiet "Updating…" until the new ones are in), the clean-ups on a thread, the hidden games shown again.
//! * The **low-space notice**: a statvfs on a thread after the first frame and after every game; under 1 GB a notice on
//!   the shelf once (`storage::check`, remembered in `storage.json`).
//! * The game card's **Hide or delete** row (`game_card::RowKind::Manage`): Hide sets the flag in library.json and takes
//!   the game out of the list on screen (the cache follows); Delete works out what goes on a thread
//!   (`game_files::plan`), shows it, and deletes on a thread after the second A.
//!
//! A game hidden or deleted leaves every list at once: the shelf's game list is made without it (`library::load_shown`,
//! and here the list on screen minus the game), and the search, Curator and the web companion are made from that list.
//! The recent games, the switcher and On start skip it (`App::recent_visible`).

use std::path::{Path, PathBuf};
use std::time::Instant;

use pocket_ui::card_health::{self, Health};
use pocket_ui::game_card::{CuratorRow, DeleteInfo, Mode, RowKind};
use pocket_ui::game_files::{self, Places, Plan};
use pocket_ui::gfx::Canvas;
use pocket_ui::notice::Notice;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::storage::{self, Check, Cleanup, Kind, Played, Roots, SystemDir, Usage, Warned};
use pocket_ui::storage_page::{Action, Event as PageEvent, HiddenGame, Row, StoragePage, View};
use pocket_ui::text::Face;
use pocket_ui::type_scale;
use pocket_ui::touch::Area;

use super::{system_short, App, Item, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Storage";
/// Below the list: what the selected row holds, or what goes.
const DETAILS_H: i32 = 260;
/// A row of the lists.
const ROW_H: i32 = 52;

/// News from the threads.
#[derive(Debug)]
pub(crate) enum Event {
    Page(PageEvent),
    /// A measurement, and which hidden games are no longer on the card.
    Measured(Box<Usage>, Vec<PathBuf>),
    /// The free space was looked at (`storage::check`).
    Free(Check, Option<u64>),
    /// What deleting the game would take.
    Planned(PathBuf, Result<Plan, String>),
    /// The game was deleted (bytes freed) or not.
    Deleted(PathBuf, Result<u64, String>),
}

/// What the App keeps.
#[derive(Default)]
pub(crate) struct StorageUi {
    page: Option<StoragePage>,
    /// The last measurement and when it came in.
    usage: Option<(Usage, Instant)>,
    /// Something changed on the card since (a game, a delete): measure again at the next visit.
    stale: bool,
    measuring: bool,
    /// The free space last read, for the Settings row.
    pub free: Option<u64>,
    /// What the card check found last (`card_health`), shown at once on the next visit while it is read again.
    health: Option<Health>,
    /// What deleting the open card's game takes.
    plan: Option<(PathBuf, Plan)>,
}

impl StorageUi {
    /// The card changed (a game wrote states, something was deleted): the page measures again when it opens.
    pub fn changed(&mut self) {
        self.stale = true;
    }
}

impl App {
    /// Settings → System → Storage.
    pub(crate) fn open_storage(&mut self) -> Option<SettingsPage> {
        let played: Vec<Played> = self
            .recents
            .list
            .iter()
            .map(|r| Played {
                system: r.system.clone(),
                stem: r.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                file: r.path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                last: r.last_played,
            })
            .collect();
        let hidden = self.hidden_games();
        let mut page = StoragePage::new(Roots::device(&self.data_dir), played, hidden, storage::now());
        if let Some((u, _)) = &self.storage.usage {
            page.event(PageEvent::Measured(Box::new(u.clone())));
        }
        page.health = self.storage.health;
        self.storage.page = Some(page);
        self.storage_health_read();
        if storage::measure_on_open(self.storage.usage.as_ref().map(|(_, at)| at.elapsed()), self.storage.stale) {
            self.storage_measure();
        } else if self.storage.measuring {
            // A walk is out already (a clean-up's): the page shows the old numbers as updating until it is in.
            if let Some(p) = self.storage.page.as_mut() {
                p.measuring = true;
            }
        }
        Some(SettingsPage::Storage)
    }

    /// Reads the Card check on a thread (a few small files and `dmesg`): every visit, it is cheap and a card can fail
    /// between two visits.
    fn storage_health_read(&self) {
        let (proxy, roms) = (self.proxy.clone(), Roots::device(&self.data_dir).roms);
        std::thread::spawn(move || {
            let health = card_health::read(&roms);
            println!("storage: card check: {health:?}");
            let _ = proxy.send_event(UserEvent::Storage(Event::Page(PageEvent::Health(health))));
        });
    }

    /// The hidden games for the page: name and system from the path (the list on screen no longer has them), the name
    /// as the shelf gave it (`storage_page::hidden_name`).
    fn hidden_games(&self) -> Vec<HiddenGame> {
        self.library
            .hidden()
            .into_iter()
            .map(|path| {
                let folder = path.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let name = pocket_ui::storage_page::hidden_name(&path, self.gamelist.index.get(&folder, &path).map(|e| e.name.as_str()));
                HiddenGame { name, system: system_short(&self.systems, &folder), there: true, path }
            })
            .collect()
    }

    /// Measures the card on a thread; one at a time.
    fn storage_measure(&mut self) {
        if let Some(p) = self.storage.page.as_mut() {
            p.measuring = true;
        }
        if self.storage.measuring {
            return;
        }
        self.storage.measuring = true;
        self.storage.stale = false;
        let (proxy, es, data) = (self.proxy.clone(), self.es_systems.clone(), self.data_dir.clone());
        let hidden = self.library.hidden();
        std::thread::spawn(move || {
            let systems: Vec<SystemDir> = pocket_ui::library::all_systems(&es)
                .unwrap_or_default()
                .into_iter()
                .map(|s| SystemDir { name: s.name, path: s.path })
                .collect();
            let u = storage::measure(&Roots::device(&data), &systems);
            println!(
                "storage: measured in {:?}: games {}, states {}, {} automatic, {} folders unreadable",
                u.took,
                storage::size(u.bytes(Kind::Games)),
                storage::size(u.bytes(Kind::States)),
                u.auto_states.len(),
                u.unreadable
            );
            let gone = hidden.into_iter().filter(|p| !p.exists()).collect();
            let _ = proxy.send_event(UserEvent::Storage(Event::Measured(Box::new(u), gone)));
        });
    }

    pub(crate) fn storage_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.storage.page.as_mut() else { return back };
        match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                self.storage.page = None;
                return back;
            }
            Action::Clean(cleanup, sel) => {
                let (proxy, roots) = (self.proxy.clone(), Roots::device(&self.data_dir));
                std::thread::spawn(move || {
                    let result = storage::clean(cleanup, &sel, &roots);
                    match &result {
                        Ok(n) => println!("storage: {cleanup:?}: {} paths, {} freed", sel.paths.len(), storage::size(*n)),
                        Err(e) => eprintln!("storage: {cleanup:?}: {e}"),
                    }
                    let _ = proxy.send_event(UserEvent::Storage(Event::Page(PageEvent::Cleaned(cleanup, result))));
                });
            }
            Action::Unhide(path) => self.show_game_again(&path),
        }
        Some(SettingsPage::Storage)
    }

    pub(crate) fn storage_event(&mut self, e: Event) {
        match e {
            Event::Page(pe) => {
                if let PageEvent::Health(h) = &pe {
                    self.storage.health = Some(*h);
                }
                let cleaned = match &pe {
                    PageEvent::Cleaned(c, _) => Some(*c),
                    _ => None,
                };
                if let Some(p) = self.storage.page.as_mut() {
                    p.event(pe);
                }
                if let Some(c) = cleaned {
                    // Pictures gone: none of them may stay on screen from memory, and the download may fetch them again.
                    if c == Cleanup::Pictures {
                        self.pictures_cleared();
                    }
                    self.storage.stale = true;
                    self.storage_measure();
                }
            }
            Event::Measured(u, gone) => {
                self.storage.measuring = false;
                self.storage.free = u.free.map(|f| f.0);
                if let Some(p) = self.storage.page.as_mut() {
                    for g in p.hidden.iter_mut() {
                        g.there = !gone.contains(&g.path);
                    }
                    p.event(PageEvent::Measured(u.clone()));
                }
                self.storage.usage = Some((*u, Instant::now()));
            }
            Event::Free(check, free) => {
                self.storage.free = free;
                self.storage_free_checked(check);
            }
            Event::Planned(path, plan) => self.card_delete_planned(path, plan),
            Event::Deleted(path, result) => self.card_deleted(path, result),
        }
    }

    /// Where the page's selection stands: a change is what the menu sounds and haptics answer.
    pub(crate) fn storage_row(&self) -> Option<(usize, usize, bool, bool, u32)> {
        self.storage.page.as_ref().map(|p| (p.sel, p.sub_sel, p.confirm.is_some(), p.busy.is_some(), p.months))
    }

    /// The value of the Settings row: the free space when it is known.
    pub(crate) fn storage_list_value(&self) -> String {
        pocket_ui::settings_values::page(&self.storage.free.map(|f| format!("{} free", storage::size(f))).unwrap_or_default())
    }

    // ---- the low-space notice -------------------------------------------------------------------------------------

    /// Looks at the free space on a thread (a statvfs can wait for the card); `storage_free_checked` gets the answer.
    pub(crate) fn storage_check(&mut self, why: &'static str) {
        let (proxy, data) = (self.proxy.clone(), self.data_dir.clone());
        self.storage.stale = true;
        std::thread::spawn(move || {
            let free = storage::free_space(&Roots::device(&data).roms).or_else(|| storage::free_space(&data)).map(|f| f.0);
            let check = storage::check(free, Warned::load(&data));
            if check != Check::Fine {
                println!("storage: {check:?} ({why})");
            }
            let _ = proxy.send_event(UserEvent::Storage(Event::Free(check, free)));
        });
    }

    /// A notice put on the shelf now would be seen: Home or a list, nothing over it and no notice up (`app_clock` asks
    /// the same).
    pub(crate) fn shelf_notice_free(&self) -> bool {
        let covered = self.running.is_some() || self.setup.is_some() || self.settings_page.is_some() || self.card.is_some() || self.menu.is_some() || self.switcher.is_some() || self.curator.page.is_some() || self.find.is_open();
        !covered && !self.notice.as_ref().is_some_and(Notice::active)
    }

    /// Nearly full and not said yet: a notice on the shelf, once (remembered only when it was shown: a game that had
    /// started in between hears it after the game). Room again: the next time it runs low it is said again.
    fn storage_free_checked(&mut self, check: Check) {
        let file = self.data_dir.join(storage::STATE_FILE);
        match check {
            Check::Fine => {}
            Check::Warn(free) => {
                if self.shelf_notice_free() {
                    self.notice = Some(Notice::new(storage::low_notice(free)));
                    pocket_ui::fsutil::write_json_soon(&file, &Warned { low_space: true });
                    self.request_redraw();
                }
            }
            Check::Recovered => pocket_ui::fsutil::write_json_soon(&file, &Warned { low_space: false }),
        }
    }

    // ---- hide and delete from the card ------------------------------------------------------------------------------

    /// The card's last row (`game_card::RowKind::Manage`), under everything else.
    pub(crate) fn card_manage_row(&self) -> CuratorRow {
        CuratorRow { kind: RowKind::Manage, title: "Hide or delete".into(), line: "Take it off the shelf, or off the SD card".into() }
    }

    /// A on "Hide or delete": the two choices.
    pub(crate) fn open_card_manage(&mut self) {
        if let Some(open) = self.card.as_mut() {
            open.card.mode = Mode::Manage { sel: 0 };
        }
    }

    /// "Hide from the shelf": the flag in library.json, the game out of every list. The card closes.
    pub(crate) fn card_hide(&mut self) {
        let Some(open) = self.card.take() else { return };
        let saved = self.library.set_hidden(&open.path, true);
        if let Err(e) = &saved {
            eprintln!("library: hidden flag not saved: {e}");
            self.toast = Some(("Could not save: is the SD card full?".into(), Instant::now()));
            self.card = Some(open);
            return;
        }
        println!("library: {} hidden", open.path.display());
        self.game_left_the_list(&open.path);
        // A notice, not a toast: long enough to read where to bring it back.
        self.notice = Some(Notice::new(format!("Hidden: {}. Settings › System › Storage brings it back.", open.name)));
        self.history_card_closed(&open.path);
    }

    /// Hidden games: A on one shows it again. The list on screen is read again on a thread (it is not in memory).
    fn show_game_again(&mut self, path: &Path) {
        if let Err(e) = self.library.set_hidden(path, false) {
            eprintln!("library: hidden flag not saved: {e}");
            self.toast = Some(("Could not save: is the SD card full?".into(), Instant::now()));
            return;
        }
        println!("library: {} shown again", path.display());
        self.library_read_shown_again();
        self.web_library_changed();
    }

    /// The game list read again on a thread without the hidden games, and swapped in when it may (a game shown again
    /// here, or hidden or shown on another device, `app_library_sync`).
    pub(crate) fn library_read_shown_again(&self) {
        let (proxy, es, data, key) = (self.proxy.clone(), self.es_systems.clone(), self.data_dir.clone(), self.library_ui.key.clone());
        std::thread::spawn(move || match pocket_ui::library::load_shown(&es, &data) {
            Ok(systems) => {
                pocket_ui::library_cache::store_soon(&data, &es, systems.clone());
                let _ = proxy.send_event(UserEvent::LibraryRescanned(systems, key));
            }
            Err(e) => eprintln!("library: not read again: {e}"),
        });
    }

    /// A game left the list (hidden or deleted): the list on screen without it goes in at once (or as soon as nothing
    /// holds a game by its place, `library_apply_pending`), the cache follows, the web companion reads its list again.
    fn game_left_the_list(&mut self, path: &Path) {
        let mut fresh = self.library_ui_pending_or_shown();
        pocket_ui::library::without_hidden(&mut fresh, &std::iter::once(path.to_path_buf()).collect());
        pocket_ui::library_cache::store_soon(&self.data_dir, &self.es_systems, fresh.clone());
        let key = self.library_ui.key.clone();
        self.library_rescanned(fresh, key);
        self.web_library_changed();
    }

    /// "Delete from the card…": what it takes is worked out on a thread.
    pub(crate) fn card_plan_delete(&mut self) {
        let Some(open) = self.card.as_ref() else { return };
        let (proxy, es, data) = (self.proxy.clone(), self.es_systems.clone(), self.data_dir.clone());
        let (system, path) = (open.system.clone(), open.path.clone());
        self.storage.plan = None;
        std::thread::spawn(move || {
            let protected = protected_folders(&es, &data);
            let states = PathBuf::from(pocket_ui::slots::STATES_DIR);
            let plan = game_files::plan(&system, &path, &Places { states: &states, data: &data, protected: &protected });
            let _ = proxy.send_event(UserEvent::Storage(Event::Planned(path, plan)));
        });
    }

    fn card_delete_planned(&mut self, path: PathBuf, plan: Result<Plan, String>) {
        let synced = self.sync.prefs.enabled && (self.sync.prefs.saves || self.sync.prefs.states);
        let Some(open) = self.card.as_mut().filter(|o| o.path == path) else { return };
        let info = plan.as_ref().map(|p| DeleteInfo {
            game_bytes: p.game_bytes(),
            files: p.game.len(),
            names: p.game.iter().take(2).map(|i| i.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).collect(),
            kept_files: p.saves.len() + p.states.len(),
            kept_bytes: p.kept_bytes(),
        });
        open.card.delete_info(info.map_err(|e| e.clone()), synced);
        if let Ok(p) = plan {
            self.storage.plan = Some((path, p));
        }
    }

    /// The second A on Delete: on a thread.
    pub(crate) fn card_delete(&mut self, with_saves: bool) {
        let Some(open) = self.card.as_ref() else { return };
        let Some((path, plan)) = self.storage.plan.take().filter(|(p, _)| *p == open.path) else { return };
        println!("library: deleting {} ({} files{})", path.display(), plan.files(with_saves), if with_saves { ", with saves and states" } else { "" });
        let (proxy, es, data) = (self.proxy.clone(), self.es_systems.clone(), self.data_dir.clone());
        std::thread::spawn(move || {
            let protected = protected_folders(&es, &data);
            let result = game_files::delete(&plan, with_saves, &protected);
            let _ = proxy.send_event(UserEvent::Storage(Event::Deleted(path, result)));
        });
    }

    fn card_deleted(&mut self, path: PathBuf, result: Result<u64, String>) {
        match result {
            Err(e) => {
                eprintln!("library: {} not deleted: {e}", path.display());
                if let Some(open) = self.card.as_mut().filter(|o| o.path == path) {
                    open.card.delete_failed(e);
                }
            }
            Ok(freed) => {
                println!("library: {} deleted, {} freed", path.display(), storage::size(freed));
                let name = self.card.as_ref().filter(|o| o.path == path).map(|o| o.name.clone());
                if name.is_some() {
                    self.card = None;
                }
                self.game_left_the_list(&path);
                if self.recents.remove(&path) {
                    self.rebuild();
                }
                self.favorites.remove(&path);
                if self.library.is_hidden(&path) {
                    let _ = self.library.set_hidden(&path, false);
                }
                self.storage.changed();
                let name = name.unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                self.notice = Some(Notice::new(format!("Deleted {name}: {} freed.", storage::size(freed))));
                self.history_card_closed(&path);
            }
        }
    }

    /// The picture cache was cleared: nothing of it stays on screen from memory, and the automatic download may look
    /// for every game again (unless one runs: it ends first).
    fn pictures_cleared(&mut self) {
        self.covers.clear();
        self.pictures_changed();
        if !self.cover_run.running() {
            self.cover_run = Default::default();
        }
        self.auto_covers("picture cache cleared");
    }

    /// The list from the last rescan still waiting to go in, else the list on screen.
    fn library_ui_pending_or_shown(&self) -> Vec<pocket_ui::library::System> {
        self.library_pending().cloned().unwrap_or_else(|| self.systems.clone())
    }

    // ---- painting -------------------------------------------------------------------------------------------------

    pub(crate) fn paint_storage(&mut self, c: &mut Canvas) {
        let Some(page) = self.storage.page.take() else { return };
        self.paint_storage_page(c, &page);
        self.storage.page = Some(page);
    }

    fn paint_storage_page(&mut self, c: &mut Canvas, page: &StoragePage) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, type_scale::row(ROW_H, 34), 34u32);
        self.status_pill(c);
        let title = match page.view {
            View::Main => SETTINGS_NAME,
            View::Systems => "Games by system",
            View::Hidden => "Hidden games",
        };
        let title_w = self.text.draw(c, Face::Title, 50, pad + NU_BTN_PAD, pad + 58, pal().chalk, title);
        if page.measuring && page.usage.is_some() {
            // The numbers below are from an earlier visit; the new ones are being counted.
            self.text.draw(c, Face::Body, NU_SMALL, pad + NU_BTN_PAD + title_w + 24, pad + 58, pal().quiet, "Updating…");
        }
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let list_bottom = c.h - pad - NU_PILL - 16 - DETAILS_H;
        let visible = (((list_bottom - top) / row) as usize).max(3);
        // (name, value, value in the alert colour)
        let rows: Vec<(String, String, bool)> = match page.view {
            View::Main => page
                .rows()
                .iter()
                .map(|r| {
                    let name = match r {
                        Row::Free => "Free on the SD card".to_string(),
                        Row::Kind(k) => k.label().to_string(),
                        Row::Earlier => "Earlier states".to_string(),
                        Row::Hidden => "Hidden games".to_string(),
                        Row::Health => "SD card check".to_string(),
                    };
                    let alert = (*r == Row::Free && page.low()) || (*r == Row::Health && matches!(page.health, Some(Health::Errors | Health::ReadOnly)));
                    (name, page.value(*r), alert)
                })
                .collect(),
            View::Systems => page.usage.as_ref().map_or_else(Vec::new, |u| u.systems.iter().map(|(n, b)| (n.clone(), storage::size(*b), false)).collect()),
            View::Hidden => page
                .hidden
                .iter()
                .map(|g| (g.name.clone(), if g.there { g.system.clone() } else { format!("{} · not on the SD card", g.system) }, false))
                .collect(),
        };
        let sel = if page.view == View::Main { page.sel } else { page.sub_sel };
        let first = super::window_start(sel, visible, rows.len());
        if rows.len() > visible {
            let track = visible as i32 * row;
            let (bar_h, bar_y) = (track * visible as i32 / rows.len() as i32, track * first as i32 / rows.len() as i32);
            c.rect(c.w - 12, top, 4, track, pal().pill);
            c.rect(c.w - 12, top + bar_y, 4, bar_h.max(12), pal().chip);
        }
        if rows.is_empty() {
            // Never an empty list: what it would hold, in words.
            let (head, line) = match page.view {
                View::Hidden => ("No hidden games", "Hide one from its card: X on a game, then Hide or delete."),
                _ => ("No games on the SD card", "Add games over file sharing: Settings › Games › Add games."),
            };
            self.text.draw(c, Face::Title, 37, pad + NU_BTN_PAD, top + 60 + type_scale::grow(37), pal().text, head);
            let mut y = top + 110 + type_scale::grow(37) + type_scale::grow(NU_SMALL);
            for line in self.text.wrap(Face::Body, NU_SMALL, line, right - pad - 2 * NU_BTN_PAD).into_iter().take(2) {
                self.text.draw(c, Face::Body, NU_SMALL, pad + NU_BTN_PAD, y, pal().quiet, &line);
                y += type_scale::line(NU_SMALL, 12);
            }
        }
        for (i, (name, value, alert)) in rows.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + type_scale::mid(font);
            let value_w = if value.is_empty() { 0 } else { self.text.width(Face::Body, font, value) + 40 };
            let name = self.text.fit(Face::Body, font, name, right - pad - 2 * NU_BTN_PAD - value_w);
            let selected = i == sel;
            if selected {
                let w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
            }
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, if selected { pal().ink } else { pal().text }, &name);
            let color = match (alert, selected) {
                (true, false) => pal().alert,
                (_, true) => pal().text,
                _ => pal().dust,
            };
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, color, value);
        }
        self.hits.list(sel, rows.len(), 0.0, row as f32);

        let left = pad + NU_BTN_PAD;
        let width = c.w - pad - NU_BTN_PAD - left;
        let y0 = list_bottom + 14;
        c.rect(left, y0, width, 2, pal().spine);
        let (head, lines, warn) = match page.view {
            View::Main => page.details(),
            View::Systems => ("Games by system".into(), vec!["Largest first. To delete a game, open its card (X on the game), then Hide or delete.".into()], false),
            View::Hidden if page.hidden.is_empty() => ("Hidden games".into(), vec!["Games you hide leave the shelf, the lists, the search, Curator and the web companion.".into()], false),
            View::Hidden => match &page.message {
                Some((m, _)) => ("Done".into(), vec![m.clone()], false),
                None => ("Hidden games".into(), vec!["A shows the game again: on the shelf, in the lists, the search, Curator and the web companion.".into()], false),
            },
        };
        let head = self.text.fit(Face::Title, 44, &head, width);
        let color = if warn { pal().alert } else { pal().chalk };
        self.text.draw(c, Face::Title, 44, left, y0 + 58 + type_scale::grow(44) / 2, color, &head);
        let mut y = y0 + 108 + type_scale::grow(44) + type_scale::grow(NU_SMALL) / 2;
        for text in lines {
            for line in self.text.wrap(Face::Body, NU_SMALL, &text, width).into_iter().take(3) {
                if y > y0 + DETAILS_H - 10 {
                    break;
                }
                self.text.draw(c, Face::Body, NU_SMALL, left, y, pal().text, &line);
                y += type_scale::line(NU_SMALL, 12);
            }
        }
        let mut hints: Vec<(&str, &str)> = vec![("B", "Back")];
        match page.view {
            View::Main => {
                if let Some(a) = page.a_hint() {
                    hints.push(("A", a));
                }
                if page.row() == Row::Kind(Kind::States) && page.busy.is_none() && page.confirm.is_none() {
                    hints.push(("< >", "Months"));
                }
            }
            View::Hidden if !page.hidden.is_empty() => hints.push(("A", "Show again")),
            _ => {}
        }
        self.nu_hints(c, &hints);
    }

    // ---- snapshots ------------------------------------------------------------------------------------------------

    /// `--snapshot storage[:<row>][:confirm|:health-wait|:health-errors|:health-readonly|:health-unknown|:low|:counting|:updating|:systems|:hidden|:hidden-empty|:done|:unreadable]`: the page
    /// with a made-up card (64 GB, 812 MB free with `low`), that row selected (0 = Free on the card), the second-A
    /// question, a list, or the line after a clean-up.
    pub(crate) fn storage_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("storage") else { return false };
        if !(rest.is_empty() || rest.starts_with(':')) {
            return false;
        }
        let mut parts = rest.trim_start_matches(':').split(':');
        let first = parts.next().unwrap_or("");
        let (row, mode) = match first.parse::<usize>() {
            Ok(r) => (r, parts.next().unwrap_or("")),
            Err(_) => (0, first),
        };
        let roots = Roots::device(&self.data_dir);
        let hidden = match mode {
            "hidden-empty" => Vec::new(),
            _ => vec![
                HiddenGame { path: "/storage/roms/gba/BIOS Test (Demo).gba".into(), name: "BIOS Test".into(), system: "GBA".into(), there: true },
                HiddenGame { path: "/storage/roms/psx/Racer (Disc 2).chd".into(), name: "Racer (Disc 2)".into(), system: "PS1".into(), there: true },
                HiddenGame { path: "/storage/roms/snes/Old Hack.sfc".into(), name: "Old Hack".into(), system: "SNES".into(), there: false },
            ],
        };
        let mut page = StoragePage::new(roots.clone(), Vec::new(), hidden, storage::now());
        if mode != "counting" {
            page.event(PageEvent::Measured(Box::new(demo_usage(&roots, mode == "low", mode == "unreadable"))));
        }
        page.sel = row.min(page.rows().len() - 1);
        match mode {
            "confirm" => {
                page.press(CONFIRM, CONFIRM, CANCEL);
            }
            "systems" => {
                page.view = View::Systems;
                page.sub_sel = 1;
            }
            "hidden" | "hidden-empty" => page.view = View::Hidden,
            "done" => page.event(PageEvent::Cleaned(Cleanup::OldStates(6), Ok(2_140_000_000))),
            // The numbers of an earlier visit, a new walk out.
            "updating" => page.measuring = true,
            _ => {}
        }
        // The card check of a made-up card: no errors, unless the state says otherwise; "health-wait": not in yet.
        page.health = match mode {
            "health-wait" => None,
            "health-errors" => Some(Health::Errors),
            "health-readonly" => Some(Health::ReadOnly),
            "health-unknown" => Some(Health::Unknown),
            _ => Some(Health::Fine),
        };
        self.storage.page = Some(page);
        self.settings_page = Some(SettingsPage::Storage);
        true
    }

    /// `--snapshot card-manage:<system>:<name>[:row|:sheet|:delete|:armed|:saves|:working|:error]` (the card of that
    /// game with Hide or delete selected at its end, the two choices, the delete sheet with made-up sizes, armed,
    /// with saves and states and Sync on, before the sizes are in, a game no longer there), and `storage-notice` (Home
    /// with the low-space notice).
    pub(crate) fn storage_shelf_snapshot(&mut self, state: &str) -> bool {
        if state == "storage-notice" {
            self.notice = Some(Notice::new(storage::low_notice(812_000_000)));
            return true;
        }
        let Some(rest) = state.strip_prefix("card-manage:") else { return false };
        let parts: Vec<&str> = rest.split(':').collect();
        let (sys, name, mode) = (parts.first().copied().unwrap_or(""), parts.get(1).copied().unwrap_or(""), parts.get(2).copied().unwrap_or("row"));
        let found = self.systems.iter().position(|s| s.name == sys).and_then(|s| {
            let g = self.systems[s].games.iter().position(|g| g.name.to_lowercase().contains(&name.to_lowercase()))?;
            Some((s, g))
        });
        let Some((s, g)) = found else {
            eprintln!("card-manage:{rest}: no such game");
            return true;
        };
        let (system, gname, path) = (self.systems[s].name.clone(), self.systems[s].games[g].name.clone(), self.systems[s].games[g].path.clone());
        self.show_card(Item::Game(s, g), system, gname, path);
        let Some(open) = self.card.as_mut() else { return true };
        let info = || DeleteInfo { game_bytes: 2_310_000_000, files: 3, names: vec!["Racer.cue".into(), "Racer (Track 1).bin".into()], kept_files: 4, kept_bytes: 41_200_000 };
        match mode {
            "sheet" => open.card.mode = Mode::Manage { sel: 1 },
            "delete" | "armed" | "saves" | "working" | "error" => {
                open.card.mode = Mode::Manage { sel: 1 };
                open.card.press(CONFIRM);
                match mode {
                    "working" => {}
                    "error" => open.card.delete_info(Err("The game's file is no longer on the SD card.".into()), false),
                    _ => open.card.delete_info(Ok(info()), mode == "saves"),
                }
                if mode == "armed" {
                    open.card.press(CONFIRM);
                }
                if mode == "saves" {
                    open.card.press(Button::Up);
                    open.card.press(CONFIRM);
                }
            }
            _ => {}
        }
        true
    }

    /// After the first paint (which places the card's rows): Down to the end of the card, onto Hide or delete.
    pub(crate) fn storage_shelf_snapshot_after(&mut self, state: &str) {
        let Some(rest) = state.strip_prefix("card-manage:") else { return };
        if !rest.ends_with(":row") && rest.split(':').count() > 2 {
            return;
        }
        if let Some(open) = self.card.as_mut() {
            let last = open.card.data.rows.len().checked_sub(1);
            if let Some(i) = last {
                open.card.focus_row(i);
            }
        }
    }
}

/// The folders "Delete from the card" never takes: the games folder, every system's ROM folder, BIOS, states,
/// screenshots and backups, pocket-ui's data folder.
fn protected_folders(es: &Path, data: &Path) -> Vec<PathBuf> {
    let roms = PathBuf::from("/storage/roms");
    let mut out: Vec<PathBuf> = pocket_ui::library::all_systems(es).unwrap_or_default().into_iter().map(|s| s.path).collect();
    out.extend([roms.join("bios"), roms.join("savestates"), roms.join("screenshots"), roms.join("backup"), roms, PathBuf::from("/storage"), data.to_path_buf()]);
    out
}

/// A card for the snapshots: 64 GB with 41 GB free (812 MB with `low`), a PS2-heavy library.
fn demo_usage(roots: &Roots, low: bool, unreadable: bool) -> Usage {
    let mut u = Usage::default();
    for (k, n) in [
        (Kind::Games, 38_400_000_000),
        (Kind::States, 3_120_000_000),
        (Kind::Saves, 46_000_000),
        (Kind::Screenshots, 212_000_000),
        (Kind::Pictures, 341_000_000),
        (Kind::Logs, 31_000_000),
        (Kind::Backups, 1_200_000),
        (Kind::Other, 2_900_000_000),
    ] {
        u.set_bytes(k, n);
    }
    u.systems = vec![
        ("PlayStation 2".into(), 24_100_000_000),
        ("GameCube".into(), 8_300_000_000),
        ("PlayStation".into(), 4_900_000_000),
        ("Super Nintendo".into(), 610_000_000),
        ("Game Boy Advance".into(), 402_000_000),
        ("Game Boy".into(), 88_000_000),
    ];
    let old = storage::now() - 300 * 24 * 3600;
    u.auto_states = (0..14)
        .map(|i| storage::AutoState { path: roots.roms.join(format!("savestates/ps2/Game {i}.99.p2s")), system: "ps2".into(), base: format!("Game {i}"), bytes: 150_000_000, modified: old })
        .collect();
    u.earlier = 84_000_000;
    u.old_logs = vec![storage::Folder { path: roots.data.join("logs/boot-0007"), bytes: 18_000_000 }, storage::Folder { path: roots.data.join("logs/boot-0008"), bytes: 9_000_000 }];
    u.backups = (0..3).map(|i| storage::Folder { path: roots.backups.join(format!("b{i}")), bytes: 400_000 }).collect();
    u.free = Some((if low { 812_000_000 } else { 41_200_000_000 }, 63_900_000_000));
    u.unreadable = usize::from(unreadable) * 2;
    u
}
