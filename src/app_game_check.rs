//! Part of the pocket-ui binary: the App's side of `pocket_ui::game_check` and `pocket_ui::game_check_page`.
//! Settings → Games → Game check: runs the check on its own low-priority thread, stops it when the page is left or a game
//! starts, paints the result, hides a duplicate when asked. It also makes the offer after games were added
//! (`game_check_games_added`): every way games arrive ends in a list read again, so `library_rescanned` and
//! `finish_setup` call it, and a later feature only needs to read the list again as well.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pocket_ui::game_check::{self, Group, Input};
use pocket_ui::game_check_page::{Action, Event, GameCheckPage, Level};
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::setup::Only;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, Opens, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list.
pub(crate) const SETTINGS_NAME: &str = "Game check";
/// Rows on a page, at most (`docs/DESIGN.md`); a longer group scrolls.
const MAX_ROWS: usize = 8;
/// Progress reaches the page at most this often.
/// The newest check; events of an older one are dropped (`game_check_event`).
static CHECK_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

const PROGRESS_EVERY_MS: u128 = 150;

#[derive(Default)]
pub(crate) struct GameCheckUi {
    page: Option<GameCheckPage>,
    stop: Option<Arc<AtomicBool>>,
    /// The Settings row's value: what the last finished check found, this session.
    last: Option<String>,
    /// The next list read again is the starter pack's: its games were checked by their own hash.
    no_offer: bool,
}

impl App {
    pub(crate) fn open_game_check(&mut self) -> Option<SettingsPage> {
        self.game_check.page = Some(GameCheckPage::running());
        self.game_check_start();
        Some(SettingsPage::GameCheck)
    }

    fn game_check_stop(&mut self) {
        if let Some(stop) = self.game_check.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }

    fn game_check_start(&mut self) {
        self.game_check_stop();
        let stop = Arc::new(AtomicBool::new(false));
        self.game_check.stop = Some(stop.clone());
        let gen = CHECK_GEN.fetch_add(1, Ordering::Relaxed) + 1;
        // The list a rescan found is the shelf that is about to be there.
        let systems = self.library_pending().unwrap_or(&self.systems);
        let shelf: Vec<(String, Vec<PathBuf>)> = systems.iter().map(|s| (s.name.clone(), s.games.iter().map(|g| g.path.clone()).collect())).collect();
        let (proxy, es_systems) = (self.proxy.clone(), self.es_systems.clone());
        let h700 = pocket_ui::device::current().model.is_some_and(|m| m.is_h700());
        let spawned = std::thread::Builder::new().name("game-check".into()).spawn(move || {
            #[cfg(target_os = "linux")]
            // SAFETY: setpriority on this thread's own id only changes its scheduling.
            unsafe {
                let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
                libc::setpriority(libc::PRIO_PROCESS, tid, 10);
            }
            let t0 = Instant::now();
            let Ok(catalog) = pocket_ui::library::all_systems(&es_systems) else {
                let _ = proxy.send_event(UserEvent::GameCheck(gen, Event::Failed));
                return;
            };
            let roms = pocket_ui::library::roms_root(&catalog).unwrap_or_else(|| std::path::Path::new(pocket_ui::hostsys::BIOS_DIR).parent().unwrap_or(std::path::Path::new("/storage/roms")).to_path_buf());
            let input = Input { catalog, shelf, bios_dir: roms.join("bios"), roms, h700 };
            let mut sent = Instant::now();
            let mut progress = |done: usize, total: usize, system: &str| {
                if sent.elapsed().as_millis() >= PROGRESS_EVERY_MS {
                    sent = Instant::now();
                    let _ = proxy.send_event(UserEvent::GameCheck(gen, Event::Progress { done, total, system: system.to_string() }));
                }
            };
            let report = game_check::check(&input, &stop, &mut progress);
            println!("game check: {} in {:?}", report.as_ref().map_or("stopped".to_string(), |r| format!("{} of {} ready, {} findings", r.ready, r.total, r.findings.len())), t0.elapsed());
            if let Some(report) = report {
                let _ = proxy.send_event(UserEvent::GameCheck(gen, Event::Done(report)));
            }
        });
        if spawned.is_err() {
            if let Some(p) = self.game_check.page.as_mut() {
                p.event(Event::Failed);
            }
        }
    }

    pub(crate) fn game_check_event(&mut self, gen: u64, e: Event) {
        // Something of a check that was stopped or replaced.
        if gen != CHECK_GEN.load(Ordering::Relaxed) {
            return;
        }
        // A game has started meanwhile: the card is the game's now.
        if self.running.is_some() {
            self.game_check_stop();
            return;
        }
        if let Event::Done(report) = &e {
            self.game_check.last = Some(report.summary());
            self.game_check.stop = None;
        }
        if let Some(p) = self.game_check.page.as_mut() {
            p.event(e);
        }
    }

    pub(crate) fn game_check_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.game_check.page.as_mut() else { return back };
        match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                self.game_check_stop();
                self.game_check.page = None;
                return back;
            }
            Action::Start => {
                // X: the shelf reads the folders too, so the two say the same afterwards.
                self.library_rescan("game check");
                self.game_check_start();
            }
            Action::Fix(game_check::Fix::Hide(path)) => match self.library.set_hidden(&path, true) {
                Ok(()) => {
                    println!("library: {} hidden (game check)", path.display());
                    self.game_left_the_list(&path);
                    self.toast = Some(("Hidden. Settings › System › Storage brings it back.".into(), Instant::now()));
                    if let Some(p) = self.game_check.page.as_mut() {
                        p.fixed();
                    }
                }
                Err(e) => {
                    eprintln!("library: hidden flag not saved: {e}");
                    self.toast = Some(("Could not save: is the SD card full?".into(), Instant::now()));
                }
            },
        }
        Some(SettingsPage::GameCheck)
    }

    /// The starter pack brought games: they were checked by their own hash, no offer.
    pub(crate) fn game_check_no_offer(&mut self) {
        self.game_check.no_offer = true;
    }

    /// `added` games have arrived (the list was read again; `feat/phone-upload` ends the same way). From Add games the
    /// page offers to look at them at once; anywhere else a toast says where the check is.
    /// The folders were read again and the list changed: a check that is showing (or still running on) the old list
    /// looks again at the new one.
    pub(crate) fn game_check_list_changed(&mut self) {
        if self.game_check.page.is_some() && self.running.is_none() && self.settings_page == Some(SettingsPage::GameCheck) {
            self.game_check.page = Some(GameCheckPage::running());
            self.game_check_start();
        }
    }

    pub(crate) fn game_check_games_added(&mut self, added: usize) {
        if std::mem::take(&mut self.game_check.no_offer) || added == 0 || self.running.is_some() {
            return;
        }
        let from_add_games = matches!(self.settings_page, Some(SettingsPage::List(r)) if matches!(super::SETTINGS[r].1, Opens::Setup(Only::Games)));
        if from_add_games {
            self.game_check.page = Some(GameCheckPage::offer(added));
            self.settings_page = Some(SettingsPage::GameCheck);
        } else if self.settings_page.is_none() && self.onboarded {
            let line = if added == 1 { "1 new game. Settings › Games › Game check looks it over.".to_string() } else { format!("{added} new games. Settings › Games › Game check looks them over.") };
            self.toast = Some((line, Instant::now()));
        }
    }

    /// The Settings row's value: what the last check found.
    pub(crate) fn game_check_value(&self) -> String {
        self.game_check.last.clone().unwrap_or_default()
    }

    pub(crate) fn game_check_animating(&self) -> bool {
        self.settings_page == Some(SettingsPage::GameCheck) && self.game_check.page.as_ref().is_some_and(|p| p.level == Level::Running)
    }

    pub(crate) fn game_check_mark(&self) -> Option<(Level, usize, usize, usize)> {
        self.game_check.page.as_ref().map(GameCheckPage::mark)
    }

    pub(crate) fn paint_game_check(&mut self, c: &mut Canvas) {
        let Some(page) = self.game_check.page.take() else { return };
        c.clear(pal().void);
        match page.level {
            Level::Offer(n) => self.paint_game_check_offer(c, n),
            Level::Running => self.paint_game_check_running(c, &page),
            Level::Failed => self.paint_game_check_failed(c),
            Level::Groups => self.paint_game_check_groups(c, &page),
            Level::Items(g) => self.paint_game_check_items(c, &page, g),
            Level::Detail(_) => self.paint_game_check_detail(c, &page),
        }
        self.game_check.page = Some(page);
    }

    /// Words on an empty page, the way the others say it (`kit::empty_state`).
    fn game_check_words(&mut self, c: &mut Canvas, art: Option<pocket_ui::ui_art::Art>, head: &str, line: &str) -> i32 {
        let top = NU_PAD + NU_PILL + 12;
        let (left, base) = (NU_PAD + NU_BTN_PAD, top + 60 + pocket_ui::type_scale::grow(37));
        pocket_ui::kit::empty_state(c, &mut self.text, art, (left, c.w - 2 * left), top + 8, base, c.h - NU_PAD - NU_PILL - 16, head, line)
    }

    fn paint_game_check_offer(&mut self, c: &mut Canvas, n: usize) {
        self.sheet_header(c, SETTINGS_NAME);
        let head = if n == 1 { "1 new game".to_string() } else { format!("{n} new games") };
        self.game_check_words(c, Some(pocket_ui::ui_art::Art::Search), &head, "Want to see if they will start? This takes a moment and changes nothing.");
        self.nu_hints(c, &[("B", "Not now"), ("A", "Check")]);
    }

    fn paint_game_check_running(&mut self, c: &mut Canvas, page: &GameCheckPage) {
        self.sheet_header(c, SETTINGS_NAME);
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        let (done, total, system) = &page.progress;
        let line = if *total == 0 { String::new() } else if system.is_empty() { format!("{done} of {total} games") } else { format!("{done} of {total} games · {system}") };
        let y = self.game_check_words(c, None, &format!("Looking at your games{dots}"), &line);
        if *total > 0 {
            let (x, w) = (NU_PAD + NU_BTN_PAD, c.w - 2 * (NU_PAD + NU_BTN_PAD));
            let frac = (*done as f32 / *total as f32).clamp(0.0, 1.0);
            c.round_rect(x, y, w, 12, 6, pal().keycap);
            c.round_rect(x, y, ((w as f32 * frac) as i32).max(12), 12, 6, pal().label);
        }
        self.nu_hints(c, &[("B", "Stop")]);
    }

    fn paint_game_check_failed(&mut self, c: &mut Canvas) {
        self.sheet_header(c, SETTINGS_NAME);
        self.game_check_words(c, Some(pocket_ui::ui_art::Art::Search), "Can't read the list of systems", "The games are safe. Restart the Pocket and try again.");
        self.nu_hints(c, &[("B", "Back"), ("X", "Check again")]);
    }

    fn paint_game_check_groups(&mut self, c: &mut Canvas, page: &GameCheckPage) {
        let Some(report) = page.report.as_ref() else { return };
        self.sheet_header(c, SETTINGS_NAME);
        let groups = page.groups();
        if report.total == 0 && groups.is_empty() {
            self.game_check_words(c, Some(pocket_ui::ui_art::Art::NoGames), "No games yet", "Copy games into their system's folder (Settings › Games › Add games) or get the starter pack.");
            self.nu_hints(c, &[("B", "Back"), ("X", "Check again")]);
            return;
        }
        let top = NU_PAD + NU_PILL + 12;
        if report.all_good() {
            let ready = if report.total == 1 { "1 game ready".to_string() } else { format!("{} games ready", report.total) };
            // The moment: the mark, big words, nothing else. A tip is one quiet row under it.
            let left = NU_PAD + NU_BTN_PAD;
            let tips = !groups.is_empty();
            let bottom = c.h - NU_PAD - NU_PILL - 16 - if tips { pocket_ui::kit::row() + 12 } else { 0 };
            let words = pocket_ui::type_scale::line(67, 18) + pocket_ui::type_scale::line(36, 12);
            let art = pocket_ui::ui_art::Art::Logo;
            let scale = pocket_ui::ui_art::empty_scale(c.h) * 2;
            let art_h = pocket_ui::ui_art::size(art, scale).1;
            let room = bottom - top;
            let mut y = top + (room - words - art_h - 28).max(0) / 2;
            if art_h + 28 + words <= room {
                pocket_ui::ui_art::paint(c, art, left, y, scale);
                y += art_h + 28;
            }
            let base = y + 54;
            let head = self.text.fit(Face::Title, 67, "All good", c.w - 2 * left);
            self.text.draw(c, Face::Title, 67, left, base, pal().text, &head);
            self.text.draw(c, Face::Body, 36, left, base + pocket_ui::type_scale::line(36, 18), pal().text, &ready);
            if tips {
                self.game_check_rows(c, bottom + 12 - pocket_ui::kit::row(), &[(Group::Tips.name().to_string(), groups[0].1.to_string())], 0, false);
            }
            let hints: &[(&str, &str)] = if tips { &[("B", "Back"), ("X", "Check again"), ("A", "Open")] } else { &[("B", "Back"), ("X", "Check again")] };
            self.nu_hints(c, hints);
            return;
        }
        let line = format!("{} of {} games ready", report.ready, report.total);
        self.text.draw(c, Face::Body, NU_SMALL, NU_PAD + NU_BTN_PAD, top + 34 + pocket_ui::type_scale::grow(NU_SMALL), pal().quiet, &line);
        let rows: Vec<(String, String)> = groups.iter().map(|(g, n)| (g.name().to_string(), n.to_string())).collect();
        let list_top = top + 34 + pocket_ui::type_scale::line(NU_SMALL, 18);
        self.game_check_rows(c, list_top, &rows, page.group_sel, false);
        self.nu_hints(c, &[("B", "Back"), ("X", "Check again"), ("A", "Open")]);
    }

    fn paint_game_check_items(&mut self, c: &mut Canvas, page: &GameCheckPage, g: Group) {
        self.sheet_header(c, g.name());
        let items = page.items(g);
        let more = page.report.as_ref().and_then(|r| r.more.iter().find(|(m, _)| *m == g)).map_or(0, |(_, n)| *n);
        let rows: Vec<(String, String)> = items.iter().map(|f| (f.name.clone(), f.system.clone())).collect();
        self.game_check_rows(c, NU_PAD + NU_PILL + 12, &rows, page.item_sel, true);
        let line = match items.get(page.item_sel) {
            Some(_) if more > 0 && page.item_sel + 1 == items.len() => format!("And {more} more. Fix these first, then check again."),
            Some(f) => f.what.clone(),
            None => String::new(),
        };
        let at = self.nu_hints(c, &[("B", "Back"), ("A", "Open")]);
        self.paint_desc(c, &line, at);
    }

    fn paint_game_check_detail(&mut self, c: &mut Canvas, page: &GameCheckPage) {
        let Some(f) = page.finding().cloned() else { return };
        self.sheet_header(c, &f.name);
        let (left, w) = (NU_PAD + NU_BTN_PAD, c.w - 2 * (NU_PAD + NU_BTN_PAD));
        let bottom = c.h - NU_PAD - NU_PILL - 16;
        let step = pocket_ui::type_scale::line(NU_SMALL, 12);
        let mut y = NU_PAD + NU_PILL + 12 + 36 + pocket_ui::type_scale::grow(NU_SMALL);
        for (s, colour) in [(f.what.as_str(), pal().text), (f.todo.as_str(), pal().quiet)] {
            for l in self.text.wrap(Face::Body, NU_SMALL, s, w) {
                if y + step > bottom {
                    break;
                }
                self.text.draw(c, Face::Body, NU_SMALL, left, y, colour, &l);
                y += step;
            }
            y += 18;
        }
        if let Some(place) = f.place.as_deref().filter(|_| y + step <= bottom) {
            let shown = self.text.fit(Face::Body, 26, place, w);
            self.text.draw(c, Face::Body, 26, left, y, pal().quiet, &shown);
        }
        if f.fix.is_some() {
            self.nu_hints(c, &[("B", "Back"), ("A", "Hide this copy")]);
        } else {
            self.nu_hints(c, &[("B", "Back")]);
        }
    }

    /// A list in the Settings style, at most eight rows at a time, the value at the right; `line`: room for the selected
    /// row's line under it.
    fn game_check_rows(&mut self, c: &mut Canvas, top: i32, rows: &[(String, String)], sel: usize, line: bool) {
        let row = pocket_ui::kit::row();
        let font = pocket_ui::kit::ROW_FONT;
        let right = c.w - NU_PAD;
        let room = c.h - NU_PAD - NU_PILL - 16 - top - if line { 2 * pocket_ui::type_scale::line(NU_SMALL, 12) } else { 0 };
        let visible = ((room / row).max(1) as usize).min(MAX_ROWS);
        let first = super::window_start(sel, visible, rows.len());
        if rows.len() > visible {
            pocket_ui::kit::scrollbar(c, top, visible as i32 * row, first, visible, rows.len());
        }
        for (i, (name, value)) in rows.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(NU_PAD, y, right - NU_PAD, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let value = self.text.fit(Face::Body, NU_SMALL, value, (right - NU_PAD) / 3);
            let value_w = self.text.width(Face::Body, NU_SMALL, &value) + 40;
            let name = self.text.fit(Face::Body, font, name, right - NU_PAD - 2 * NU_BTN_PAD - value_w);
            let selected = i == sel;
            if selected {
                let w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD;
                pocket_ui::kit::selection(c, NU_PAD, y + 4, right - NU_PAD, row - 8, w);
            }
            self.text.draw(c, Face::Body, font, NU_PAD + NU_BTN_PAD, base, if selected { pal().ink } else { pal().text }, &name);
            self.text.draw_right(c, Face::Body, NU_SMALL, right - NU_BTN_PAD, base, if selected { pal().text } else { pal().dust }, &value);
        }
        self.hits.list(sel, rows.len(), 0.0, row as f32);
    }

    /// `--snapshot game-check[:running|:offer|:green|:none|:failed|:real[:<row>]|:group[:<row>]|:detail[:<row>]]`: the page
    /// with a made-up result (`real`: the check of the library in use, on this thread). `:group:<row>` opens the row's group; `:detail:<row>` the first game of the row's group.
    pub(crate) fn game_check_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("game-check") else { return false };
        let mut parts = rest.trim_start_matches(':').split(':');
        let mode = parts.next().unwrap_or("");
        let row: usize = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        let mut page = GameCheckPage::running();
        match mode {
            "running" => page.event(Event::Progress { done: 1800, total: 5000, system: "Super Nintendo".into() }),
            "offer" => page = GameCheckPage::offer(12),
            "failed" => page.event(Event::Failed),
            "none" => page.event(Event::Done(game_check::Report::default())),
            "green" => page.event(Event::Done(game_check::demo(true))),
            // What the check says of the library of POCKETUI_ES_SYSTEMS (the tests' folders, a copy on a device).
            "real" => {
                let catalog = pocket_ui::library::all_systems(&self.es_systems).unwrap_or_default();
                let roms = pocket_ui::library::roms_root(&catalog).unwrap_or_default();
                let shelf = self.systems.iter().map(|s| (s.name.clone(), s.games.iter().map(|g| g.path.clone()).collect())).collect();
                let input = Input { catalog, shelf, bios_dir: roms.join("bios"), roms, h700: pocket_ui::device::current().model.is_some_and(|m| m.is_h700()) };
                let report = game_check::check(&input, &AtomicBool::new(false), &mut |_, _, _| {});
                page.event(Event::Done(report.unwrap_or_default()));
                page.group_sel = row.min(page.groups().len().saturating_sub(1));
            }
            _ => page.event(Event::Done(game_check::demo(false))),
        }
        if matches!(mode, "" | "group" | "detail") {
            page.group_sel = row.min(page.groups().len().saturating_sub(1));
        }
        if mode == "group" || mode == "detail" {
            page.press(CONFIRM, CONFIRM, CANCEL);
        }
        if mode == "detail" {
            page.press(CONFIRM, CONFIRM, CANCEL);
        }
        self.game_check.page = Some(page);
        self.settings_page = Some(SettingsPage::GameCheck);
        true
    }
}
