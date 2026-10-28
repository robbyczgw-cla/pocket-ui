//! Part of the pocket-ui binary: the App's side of `pocket_ui::gamelist` and `pocket_ui::gamelist_page`. The player's
//! EmulationStation gamelists are read on a thread after the first frame, and again when the library is read again
//! (`gamelist_refresh`); the App keeps the result in memory (`GamelistUi::index`) for the game card, for covers pocket-ui
//! has none of, and for Settings → Games → From EmulationStation, the page that offers the favorites and the ratings.
//! The names a gamelist gives are in the list itself (`library::load_shown`). Nothing here runs on the UI thread but
//! lookups in memory and, on the page, the writes the player asked for.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pocket_ui::gamelist::{Index, Locations};
use pocket_ui::gamelist_page::{self as page_mod, Action, Found, Page, Row};
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "From EmulationStation";
/// Below the list: the details of the selected row.
const DETAILS_H: i32 = 330;

#[derive(Default)]
pub(crate) struct GamelistUi {
    /// What the gamelists said at the last read (empty until the first read is through, and without gamelists).
    pub index: Arc<Index>,
    /// The cover file found for a game (None: there is none), looked for once per game.
    covers: std::cell::RefCell<HashMap<PathBuf, Option<PathBuf>>>,
    /// A read is running (on its thread).
    loading: Arc<AtomicBool>,
    page: Option<Page>,
    /// The Settings row's "N to add", worked out once (the row is painted often; this walks the shelf): forgotten when
    /// the gamelists are read again, the page opens or an offer is taken.
    to_add: std::cell::Cell<Option<usize>>,
}

impl App {
    /// Reads the gamelists on a thread; a different result comes back as `UserEvent::Gamelist`. One at a time. Without
    /// gamelists this is a few `stat`s per system.
    pub(crate) fn gamelist_refresh(&mut self, why: &'static str) {
        if self.gamelist.loading.swap(true, Ordering::SeqCst) {
            return;
        }
        let busy = self.gamelist.loading.clone();
        let (proxy, data_dir, es_systems, current) = (self.proxy.clone(), self.data_dir.clone(), self.es_systems.clone(), self.gamelist.index.clone());
        let spawned = std::thread::Builder::new().name("gamelist".into()).spawn(move || {
            let t0 = Instant::now();
            let sources: Vec<(String, PathBuf)> = pocket_ui::library::all_systems(&es_systems).unwrap_or_default().into_iter().map(|s| (s.id, s.path)).collect();
            let fresh = Index::load(&data_dir, &Locations::device(), &sources);
            let same = fresh.same_as(&current);
            if !same || !fresh.is_empty() {
                println!("gamelist: {} files read ({why}) in {:?}{}", fresh.files(), t0.elapsed(), if same { ", nothing changed" } else { "" });
            }
            busy.store(false, Ordering::SeqCst);
            if !same {
                let _ = proxy.send_event(UserEvent::Gamelist(Arc::new(fresh)));
            }
        });
        if spawned.is_err() {
            self.gamelist.loading.store(false, Ordering::SeqCst);
        }
    }

    /// The gamelists were read and differ from what the App has.
    pub(crate) fn gamelist_event(&mut self, index: Arc<Index>) {
        self.gamelist.index = index;
        self.gamelist.covers.borrow_mut().clear();
        self.gamelist.to_add.set(None);
        // Games that had no cover may have one now.
        self.cabinet.covers_changed();
        let found = self.gamelist_found();
        if let Some(page) = self.gamelist.page.as_mut() {
            page.found = found;
        }
        self.request_redraw();
    }

    /// The cover a gamelist (or ES-DE's picture folders) has for the game, for a game pocket-ui has no cover of its own
    /// for. Looked for once per game (a few `stat`s); a picture over `gamelist::COVER_MAX_BYTES` is passed over.
    pub(crate) fn gamelist_cover(&self, system: &str, rom: &Path) -> Option<PathBuf> {
        if self.gamelist.index.is_empty() {
            return None;
        }
        if let Some(found) = self.gamelist.covers.borrow().get(rom) {
            return found.clone();
        }
        let found = pocket_ui::gamelist::usable_cover(self.gamelist.index.cover_candidates(system, rom));
        self.gamelist.covers.borrow_mut().insert(rom.to_path_buf(), found.clone());
        found
    }

    /// What the gamelists hold for the games on the shelf, against the player's Favorites and ratings.
    fn gamelist_found(&self) -> Found {
        let games: Vec<(&str, &Path)> = self.systems.iter().flat_map(|s| s.games.iter().map(move |g| (s.name.as_str(), g.path.as_path()))).collect();
        Found::count(&self.gamelist.index, &games, &|p| self.favorites.contains(p), &|p| self.library.get(p).rating)
    }

    /// The game list on screen was swapped: the Settings row's count and the open page's offers are worked out again.
    pub(crate) fn gamelist_library_changed(&mut self) {
        self.gamelist.to_add.set(None);
        if self.gamelist.page.is_some() {
            let found = self.gamelist_found();
            if let Some(page) = self.gamelist.page.as_mut() {
                page.found = found;
            }
        }
    }

    /// The value of the Settings row: how many favorites wait, else nothing.
    pub(crate) fn gamelist_list_value(&self) -> String {
        if self.gamelist.index.is_empty() {
            return pocket_ui::settings_values::page("");
        }
        let n = self.gamelist.to_add.get().unwrap_or_else(|| {
            let n = self.gamelist_found().new_favorites;
            self.gamelist.to_add.set(Some(n));
            n
        });
        pocket_ui::settings_values::page(&if n == 0 { String::new() } else { format!("{n} to add") })
    }

    pub(crate) fn open_gamelist(&mut self) -> Option<SettingsPage> {
        // A gamelist copied since the start is found when the page opens.
        self.gamelist_refresh("page opened");
        self.gamelist.to_add.set(None);
        self.gamelist.page = Some(Page::new(self.gamelist_found()));
        Some(SettingsPage::Gamelist)
    }

    pub(crate) fn gamelist_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.gamelist.page.as_mut() else { return back };
        match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                self.gamelist.page = None;
                return back;
            }
            Action::AddFavorites => self.gamelist_add_favorites(),
            Action::AddRatings => self.gamelist_add_ratings(),
        }
        Some(SettingsPage::Gamelist)
    }

    fn gamelist_add_favorites(&mut self) {
        let before = self.gamelist_found();
        let wanted: Vec<(String, String, PathBuf)> = self
            .systems
            .iter()
            .flat_map(|s| s.games.iter().map(move |g| (s, g)))
            .filter(|(s, g)| self.gamelist.index.get(&s.name, &g.path).is_some_and(|e| e.favorite))
            .map(|(s, g)| (s.name.clone(), g.name.clone(), g.path.clone()))
            .collect();
        let added = wanted.iter().filter(|(s, n, p)| self.favorites.set(s, n, p, true)).count();
        let saved = if added > 0 { self.favorites.save_now() } else { Ok(()) };
        println!("gamelist: {added} favorites added from EmulationStation");
        self.gamelist_done(saved, page_mod::favorites_added(added, before.favorites.saturating_sub(before.new_favorites)));
        if added > 0 {
            self.curator.touch();
            self.rebuild();
        }
    }

    fn gamelist_add_ratings(&mut self) {
        let games: Vec<(&str, &Path)> = self.systems.iter().flat_map(|s| s.games.iter().map(move |g| (s.name.as_str(), g.path.as_path()))).collect();
        let ratings: Vec<(PathBuf, u8)> = self.gamelist.index.ratings(games.into_iter()).into_iter().map(|(p, n)| (p.to_path_buf(), n)).collect();
        let result = self.library.set_ratings_if_none(&ratings);
        let given = *result.as_ref().unwrap_or(&0);
        println!("gamelist: {given} ratings taken from EmulationStation");
        self.gamelist_done(result.map(|_| ()), page_mod::ratings_added(given));
        if given > 0 {
            // The Lens's My rating, the shelves' order and the taste profile follow.
            self.curator.touch();
        }
    }

    fn gamelist_done(&mut self, saved: std::io::Result<()>, message: String) {
        self.gamelist.to_add.set(None);
        let found = self.gamelist_found();
        if let Some(page) = self.gamelist.page.as_mut() {
            match saved {
                Ok(()) => page.done(found, message),
                Err(e) => page.failed(format!("The Pocket could not save it: {e}")),
            }
        }
    }

    /// Where the page's selection stands: a change is what the menu sounds and haptics answer.
    pub(crate) fn gamelist_row(&self) -> Option<usize> {
        self.gamelist.page.as_ref().map(|p| p.sel)
    }

    /// `--snapshot gamelist[:<row>][:none|:done|:all]`: the page for the made-up library (the gamelists in the folders
    /// `POCKETUI_ES_SYSTEMS` points at; `none`: as if there were none, `all`: favorites and ratings there already, `done`: after A).
    pub(crate) fn gamelist_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("gamelist") else { return false };
        let mut row = 0;
        let mut mode = String::new();
        for part in rest.split(':').filter(|p| !p.is_empty()) {
            match part.parse::<usize>() {
                Ok(r) => row = r,
                Err(_) => mode = part.to_string(),
            }
        }
        if mode == "none" {
            self.gamelist.index = Arc::default();
        } else {
            self.gamelist_load_now();
        }
        let mut page = Page::new(self.gamelist_found());
        page.sel = row.min(page_mod::ROWS.len() - 1);
        if mode == "all" {
            self.gamelist_add_favorites_quietly();
            page.found = self.gamelist_found();
        }
        self.gamelist.page = Some(page);
        if mode == "done" {
            self.settings_page = Some(SettingsPage::Gamelist);
            self.gamelist_input(CONFIRM);
        }
        self.settings_page = Some(SettingsPage::Gamelist);
        true
    }

    /// Reads the gamelists now, on this thread: snapshots and tests, which paint right after.
    pub(crate) fn gamelist_load_now(&mut self) {
        let sources: Vec<(String, PathBuf)> = pocket_ui::library::all_systems(&self.es_systems).unwrap_or_default().into_iter().map(|s| (s.id, s.path)).collect();
        self.gamelist.index = Arc::new(Index::load(&self.data_dir, &Locations::device(), &sources));
        self.gamelist.covers.borrow_mut().clear();
    }

    /// For the "all" snapshot: the favorites and ratings are taken over before the page opens.
    fn gamelist_add_favorites_quietly(&mut self) {
        self.gamelist.page = Some(Page::new(self.gamelist_found()));
        self.gamelist_add_favorites();
        self.gamelist_add_ratings();
    }

    pub(crate) fn paint_gamelist(&mut self, c: &mut Canvas) {
        let Some(page) = self.gamelist.page.take() else { return };
        self.paint_gamelist_page(c, &page);
        self.gamelist.page = Some(page);
    }

    fn paint_gamelist_page(&mut self, c: &mut Canvas, page: &Page) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(58, 34), 34u32);
        self.status_pill(c);
        self.text.draw(c, Face::Title, 50, pad + NU_BTN_PAD, pad + 58, pal().chalk, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let list_bottom = c.h - pad - NU_PILL - 16 - DETAILS_H;
        for (i, r) in page_mod::ROWS.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let name = match r {
                Row::Favorites => "Favourites",
                Row::Ratings => "My rating",
            };
            let value = page.value(*r);
            let value_w = self.text.width(Face::Body, font, &value) + 40;
            let name = self.text.fit(Face::Body, font, name, right - pad - 2 * NU_BTN_PAD - value_w);
            let selected = i == page.sel;
            if selected {
                let w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
            }
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, if selected { pal().ink } else { pal().text }, &name);
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, if selected { pal().text } else { pal().dust }, &value);
        }
        self.hits.list(page.sel, page_mod::ROWS.len(), 0.0, row as f32);

        let left = pad + NU_BTN_PAD;
        let width = c.w - pad - NU_BTN_PAD - left;
        let y0 = list_bottom + 14;
        c.rect(left, y0, width, 2, pal().spine);
        let (head, lines, color) = match &page.message {
            Some((m, err)) => (if *err { "That did not work".to_string() } else { "Done".to_string() }, vec![m.clone()], if *err { pal().alert } else { pal().chalk }),
            None => {
                let (head, lines) = page.details();
                (head, lines, pal().chalk)
            }
        };
        let head = self.text.fit(Face::Title, 44, &head, width);
        self.text.draw(c, Face::Title, 44, left, y0 + 58, color, &head);
        let mut y = y0 + 108;
        for text in lines {
            for line in self.text.wrap(Face::Body, NU_SMALL, &text, width).into_iter().take(4) {
                if y > y0 + DETAILS_H - 10 {
                    break;
                }
                self.text.draw(c, Face::Body, NU_SMALL, left, y, pal().text, &line);
                y += pocket_ui::type_scale::line(NU_SMALL, 12);
            }
        }
        let mut hints = vec![("B", "Back")];
        if let Some(a) = page.a_hint() {
            hints.push(("A", a));
        }
        self.nu_hints(c, &hints);
    }
}
