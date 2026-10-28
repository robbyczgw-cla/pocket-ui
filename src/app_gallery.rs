//! Part of the pocket-ui binary: the App's side of the screenshot gallery (`pocket_ui::gallery`). Two ways in, one
//! page: Settings → Games → Screenshots lists every screenshot, the game card's "Screenshots (N)" row only that game's
//! (B comes back to the card).
//!
//! - **What the folder holds** is read on a thread, after the first frame, after every game and when the page opens
//!   (`gallery_refresh`); the App keeps the list in memory (`GalleryUi::index`), so the card's row and the Settings
//!   row know a number without touching the card. The first reading is a few `readdir`s.
//! - **Pictures** are decoded on the picture loader's thread (`request_picture`, the one the shelf's pictures use) into
//!   two small caches of their own: 24 thumbnails and 3 full-size pictures (the one shown and its neighbours, so Left
//!   and Right are instant). Nothing is decoded on the UI thread; the picture's 24-byte header is read for the full
//!   view, to show small pictures at a whole multiple of their size.
//! - **Delete** is one `unlink` after the second A; **Y** copies the file to where the shelf looks for the game's "In
//!   the game" picture (`game_pictures::path`) on a thread of its own.
//! - `--snapshot gallery[:…]` with made-up screenshots (see `gallery_snapshot`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pocket_ui::game_card::{CuratorRow, RowKind};
use pocket_ui::gallery::{self as g, Gallery, Index, Out, PictureCache, Row, Scope, Shot, View, Want};
use pocket_ui::game_pictures::{self, Kind};
use pocket_ui::gfx::Canvas;
use pocket_ui::image::{self, Image};
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::picture_loader::{self, Key};
use pocket_ui::screenshot;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;
use pocket_ui::type_scale;

use super::{stripe, system_short, App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Screenshots";

/// A thumbnail's box in a row of the list.
const THUMB: (i32, i32) = (112, 84);
/// Decoded thumbnails and full-size pictures kept. A screen shows at most about nine rows.
const THUMBS_KEPT: usize = 24;
const FULL_KEPT: usize = 3;
/// Requests out at a time: a fast scroll never queues pictures that are long off screen.
const THUMBS_ASKED: usize = 6;
const FULL_ASKED: usize = 3;

/// What a thread brings back.
#[derive(Debug)]
pub(crate) enum Event {
    /// The folder was read.
    Scanned(Index),
    /// A screenshot was copied to the shelf's picture of its game (or was not).
    Shelf(Result<(), String>),
}

pub(crate) struct GalleryUi {
    /// What the folder held at the last reading.
    pub index: Index,
    scanning: Arc<AtomicBool>,
    page: Option<Gallery>,
    /// Opened from a game's card: B goes back to it.
    over_card: bool,
    /// Scope Game: the game's ROM, for Y.
    rom: Option<PathBuf>,
    thumbs: PictureCache,
    full: PictureCache,
    /// What the header of a file said: its size, or None for a file that is no picture to decode.
    sizes: HashMap<PathBuf, Option<(u32, u32)>>,
    /// `--snapshot`: made-up screenshots in this folder, today as this date, pictures decoded on the spot.
    made_up: Option<((i32, u32, u32), PathBuf)>,
}

impl Default for GalleryUi {
    fn default() -> Self {
        GalleryUi {
            index: Index::default(),
            scanning: Arc::default(),
            page: None,
            over_card: false,
            rom: None,
            thumbs: PictureCache::new(THUMBS_KEPT),
            full: PictureCache::new(FULL_KEPT),
            sizes: HashMap::new(),
            made_up: None,
        }
    }
}

/// How a full-size picture stands.
enum Shown {
    Picture(Rc<Image>),
    Loading,
    /// The file is no picture the Pocket can read.
    Broken,
}

impl App {
    fn gallery_dir(&self) -> PathBuf {
        self.gallery.made_up.as_ref().map_or_else(screenshot::dir, |(_, d)| d.clone())
    }

    fn gallery_today(&self) -> (i32, u32, u32) {
        self.gallery.made_up.as_ref().map_or_else(
            || {
                let s = screenshot::Stamp::now();
                (s.year, s.month, s.day)
            },
            |(t, _)| *t,
        )
    }

    /// Reads the screenshots folder on a thread; the list comes back as `UserEvent::Gallery`. One at a time.
    pub(crate) fn gallery_refresh(&mut self, why: &'static str) {
        if self.gallery.made_up.is_some() || self.gallery.scanning.swap(true, Ordering::SeqCst) {
            return;
        }
        let (busy, proxy, dir) = (self.gallery.scanning.clone(), self.proxy.clone(), self.gallery_dir());
        let spawned = std::thread::Builder::new().name("gallery".into()).spawn(move || {
            let t0 = Instant::now();
            let index = Index::load(&dir);
            println!("gallery: {} screenshots ({why}) in {:?}", index.len(), t0.elapsed());
            busy.store(false, Ordering::SeqCst);
            let _ = proxy.send_event(UserEvent::Gallery(Event::Scanned(index)));
        });
        if spawned.is_err() {
            self.gallery.scanning.store(false, Ordering::SeqCst);
        }
    }

    pub(crate) fn gallery_event(&mut self, e: Event) {
        match e {
            Event::Scanned(index) => {
                self.gallery.index = index;
                self.gallery_page_refresh();
                // A card open under it (or beside it) says how many there are now.
                if self.gallery.page.is_none() {
                    self.gallery_card_row_refresh();
                }
            }
            Event::Shelf(result) => {
                let done = result.is_ok();
                if let Some(page) = self.gallery.page.as_mut() {
                    match result {
                        Ok(()) => page.said(SHELF_SAID),
                        Err(why) => page.failed(&why),
                    }
                }
                if done {
                    // The shelf's picture of the game changed under the caches that hold it.
                    self.covers.clear();
                    self.pictures_changed();
                }
                self.request_redraw();
            }
        }
    }

    /// The open page takes the new list.
    fn gallery_page_refresh(&mut self) {
        let shots = self.gallery_scope_shots();
        if let Some(page) = self.gallery.page.as_mut() {
            page.refresh(shots);
        }
        self.request_redraw();
    }

    fn gallery_scope_shots(&self) -> Vec<Shot> {
        match self.gallery.page.as_ref().map(|p| &p.scope) {
            Some(Scope::Game(_)) => match self.card.as_ref() {
                Some(open) => self.gallery.index.of_game(&open.system, &stem_of(&open.path)),
                None => Vec::new(),
            },
            _ => self.gallery.index.all().to_vec(),
        }
    }

    /// The open card's "Screenshots (N)" row follows the index: its count, a row that comes or goes.
    fn gallery_card_row_refresh(&mut self) {
        let Some(open) = self.card.as_ref() else { return };
        let fresh = self.card_screenshots_row(&open.system, &open.path);
        let Some(open) = self.card.as_mut() else { return };
        let rows = &mut open.card.data.rows;
        match (rows.iter().position(|r| r.kind == RowKind::Screenshots), fresh) {
            (Some(i), Some(row)) => rows[i] = row,
            (Some(i), None) => {
                rows.remove(i);
                open.card.focus = None;
            }
            // Before "Hide or delete", the last row.
            (None, Some(row)) => {
                let at = rows.iter().position(|r| r.kind == RowKind::Manage).unwrap_or(rows.len());
                rows.insert(at, row);
                open.card.focus = None;
            }
            (None, None) => {}
        }
    }

    /// The game card's row (`game_card::RowKind::Screenshots`): only for a game that has some.
    pub(crate) fn card_screenshots_row(&self, system: &str, path: &Path) -> Option<CuratorRow> {
        let stem = stem_of(path);
        let n = self.gallery.index.count(system, &stem);
        if n == 0 {
            return None;
        }
        let newest = self.gallery.index.latest(system, &stem)?;
        Some(CuratorRow { kind: RowKind::Screenshots, title: format!("Screenshots ({n})"), line: format!("Newest: {}", g::day_label(newest.when.date(), self.gallery_today())) })
    }

    /// The Settings row's value: how many there are, once the folder has been read.
    pub(crate) fn gallery_list_value(&self) -> String {
        let n = self.gallery.index.len();
        pocket_ui::settings_values::page(&if n == 0 { String::new() } else { n.to_string() })
    }

    /// Settings → Games → Screenshots.
    pub(crate) fn open_gallery(&mut self) -> Option<SettingsPage> {
        self.gallery_refresh("page opened");
        let page = Gallery::new(Scope::All, self.gallery.index.all().to_vec(), self.gallery.index.loaded(), self.gallery_today(), self.pictures.wanted());
        self.gallery.page = Some(page);
        self.gallery.over_card = false;
        self.gallery.rom = None;
        Some(SettingsPage::Screenshots)
    }

    /// A on the card's "Screenshots (N)" row.
    pub(crate) fn open_gallery_for_card(&mut self) {
        let Some(open) = self.card.as_ref() else { return };
        let (title, system, path) = (open.name.clone(), open.system.clone(), open.path.clone());
        self.gallery_refresh("card row");
        let shots = self.gallery.index.of_game(&system, &stem_of(&path));
        self.gallery.page = Some(Gallery::new(Scope::Game(title), shots, self.gallery.index.loaded(), self.gallery_today(), self.pictures.wanted()));
        self.gallery.over_card = true;
        self.gallery.rom = Some(path);
    }

    /// The page is open over a game's card.
    pub(crate) fn gallery_over_card(&self) -> bool {
        self.gallery.page.is_some() && self.gallery.over_card && self.running.is_none() && self.settings_page.is_none()
    }

    /// Where the page's selection stands (the menu sounds and haptics follow a change).
    pub(crate) fn gallery_row(&self) -> Option<(usize, bool, bool)> {
        self.gallery.page.as_ref().map(|p| (p.sel, p.view == View::Full, p.armed))
    }

    /// A button on the page opened from Settings.
    pub(crate) fn gallery_input(&mut self, b: Button) -> Option<SettingsPage> {
        if self.gallery_press(b) {
            return Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        }
        Some(SettingsPage::Screenshots)
    }

    /// A button on the page opened from a card.
    pub(crate) fn gallery_card_input(&mut self, b: Button) {
        self.gallery_press(b);
    }

    /// One press; true when the page was closed.
    fn gallery_press(&mut self, b: Button) -> bool {
        let Some(page) = self.gallery.page.as_mut() else { return true };
        match page.press(b, CONFIRM, CANCEL) {
            Out::None => {}
            Out::Close => {
                self.gallery.page = None;
                self.gallery.over_card = false;
                // The full-size pictures are big: let them go with the page.
                self.gallery.thumbs.clear();
                self.gallery.full.clear();
                // Back on the card: its row counts what is left.
                self.gallery_card_row_refresh();
                return true;
            }
            Out::Delete(path) => self.gallery_delete(&path),
            Out::UseOnShelf(shot) => self.gallery_use_on_shelf(&shot),
        }
        false
    }

    /// The second A: the file goes (`gallery::delete`: one `unlink`). A card that is read-only says so.
    fn gallery_delete(&mut self, path: &Path) {
        let result = g::delete(&self.gallery_dir(), path);
        let Some(page) = self.gallery.page.as_mut() else { return };
        match result {
            Ok(()) => {
                println!("gallery: deleted {}", path.display());
                page.deleted(path);
                self.gallery.index.remove(path);
                self.gallery.thumbs.forget(path);
                self.gallery.full.forget(path);
            }
            Err(e) => {
                eprintln!("gallery: can't delete {}: {e}", path.display());
                page.failed("Could not delete the screenshot");
            }
        }
    }

    /// The ROM a screenshot belongs to: the card's game, or the shelf's game of that name in that system.
    fn gallery_rom(&self, shot: &Shot) -> Option<PathBuf> {
        if let Some(rom) = self.gallery.rom.as_ref() {
            return Some(rom.clone());
        }
        let sys = self.systems.iter().find(|s| s.name == shot.system)?;
        sys.games.iter().find(|g| screenshot::clean(&stem_of(&g.path)) == shot.game).map(|g| g.path.clone())
    }

    /// Y: the screenshot becomes the game's "In the game" picture on the shelf. The copy runs on a thread.
    fn gallery_use_on_shelf(&mut self, shot: &Shot) {
        let Some(rom) = self.gallery_rom(shot) else {
            if let Some(page) = self.gallery.page.as_mut() {
                page.failed("That game is not on the shelf");
            }
            return;
        };
        let dest = game_pictures::path(&self.data_dir, Kind::Snap, &shot.system, &rom);
        let (from, proxy) = (shot.path.clone(), self.proxy.clone());
        if self.pictures.sync {
            let result = g::use_on_shelf(&from, &dest);
            self.gallery_event(Event::Shelf(result));
            return;
        }
        let spawned = std::thread::Builder::new().name("gallery-copy".into()).spawn(move || {
            let result = g::use_on_shelf(&from, &dest);
            let _ = proxy.send_event(UserEvent::Gallery(Event::Shelf(result)));
        });
        if spawned.is_err() {
            if let Some(page) = self.gallery.page.as_mut() {
                page.failed("Could not save it as the shelf picture");
            }
        }
    }

    /// The loader answered. `None`: it was the gallery's and is in its cache; `Some`: somebody else's, passed on.
    pub(crate) fn gallery_picture_arrived(&mut self, key: &Key, img: Option<Image>) -> Option<Option<Image>> {
        // `img` is moved into whichever cache asked; a cache that did not ask leaves it alone.
        let g = &mut self.gallery;
        let mine = if g.thumbs.owns(key) {
            g.thumbs.arrived(key, img)
        } else if g.full.owns(key) {
            g.full.arrived(key, img)
        } else {
            return Some(img);
        };
        debug_assert!(mine);
        self.request_redraw();
        None
    }

    /// A thumbnail, from the cache; asks the loader for it when it is not there. `Some(None)`: a file that could not
    /// be read.
    fn gallery_thumb(&mut self, file: &Path) -> Option<Option<Rc<Image>>> {
        let key: Key = (file.to_path_buf(), THUMB.0, THUMB.1);
        self.gallery_picture(&key, false)
    }

    fn gallery_picture(&mut self, key: &Key, full: bool) -> Option<Option<Rc<Image>>> {
        let sync = self.pictures.sync;
        let cache = if full { &mut self.gallery.full } else { &mut self.gallery.thumbs };
        match cache.want(key, if full { FULL_ASKED } else { THUMBS_ASKED }) {
            Want::Have => return cache.get(key).cloned(),
            Want::Wait => return None,
            Want::Ask => {}
        }
        if sync {
            let img = picture_loader::load_fitted(key);
            cache.arrived(key, img);
            return cache.get(key).cloned();
        }
        self.request_picture(key.clone());
        None
    }

    /// The full-size picture of a file in a room of `room` pixels.
    fn gallery_full(&mut self, file: &Path, room: (i32, i32)) -> Shown {
        let size = *self.gallery.sizes.entry(file.to_path_buf()).or_insert_with(|| image::png_size(file));
        let Some(size) = size else { return Shown::Broken };
        let (bw, bh) = g::full_box(size, room);
        match self.gallery_picture(&(file.to_path_buf(), bw, bh), true) {
            Some(Some(img)) => Shown::Picture(img),
            Some(None) => Shown::Broken,
            None => Shown::Loading,
        }
    }

    // ---- painting ----------------------------------------------------------------------------------------------

    pub(crate) fn paint_gallery(&mut self, c: &mut Canvas) {
        let Some(mut page) = self.gallery.page.take() else { return };
        c.clear(pal().void);
        if page.view == View::Full && page.current().is_some() {
            self.paint_gallery_full(c, &mut page);
        } else {
            self.paint_gallery_list(c, &mut page);
        }
        if page.armed {
            self.paint_gallery_question(c, &page);
        }
        let x0 = self.nu_hints(c, &page.hints());
        // Left of the hints: what the last delete or Y came to, else how many there are.
        let (text, color) = match &page.message {
            Some((m, true)) => (m.clone(), pal().alert),
            Some((m, false)) => (m.clone(), pal().chalk),
            // The full view says "3 of 12" at the top already.
            None if page.shots().is_empty() || page.view == View::Full => (String::new(), pal().dust),
            None => (count_text(page.shots().len()), pal().dust),
        };
        let left = NU_PAD + NU_BTN_PAD;
        // The count is left out when it would be cut ("10 screen…"); a message (what a delete or Y came to) is not: one
        // too long for the room beside four hints ("This is no…") gets its own line above the hint bar, on a band of
        // the page's colour so it reads over the picture or the list.
        let counts = page.message.is_none();
        let fits = self.text.width(Face::Body, NU_SMALL, &text) <= x0 - left - 24;
        if !text.is_empty() && fits {
            self.text.draw(c, Face::Body, NU_SMALL, left, c.h - NU_PAD - NU_PILL / 2 + type_scale::mid(NU_SMALL), color, &text);
        } else if !text.is_empty() && !counts {
            let line_h = type_scale::line(NU_SMALL, 8);
            let bottom = c.h - NU_PAD - NU_PILL - 12;
            c.shade_rect(0, bottom - line_h - 8, c.w, line_h + 12, pal().void, 220);
            let text = self.text.fit(Face::Body, NU_SMALL, &text, c.w - 2 * left);
            self.text.draw(c, Face::Body, NU_SMALL, left, bottom - line_h / 2 - 2 + type_scale::mid(NU_SMALL), color, &text);
        }
        self.gallery.page = Some(page);
    }

    /// The list, in groups by day: a spine per screenshot with a small picture, or the words for an empty page.
    fn paint_gallery_list(&mut self, c: &mut Canvas, page: &mut Gallery) {
        let pad = NU_PAD;
        let pill_left = self.status_pill(c);
        let (left, right) = (pad + NU_BTN_PAD, c.w - pad - NU_BTN_PAD);
        let title = match &page.scope {
            Scope::All => SETTINGS_NAME.to_string(),
            Scope::Game(t) => t.clone(),
        };
        let title = self.text.fit(Face::Title, 50, &title, pill_left - 24 - left);
        self.text.draw(c, Face::Title, 50, left, pad + 58, pal().chalk, &title);
        let (top, bottom) = (pad + NU_PILL + 12, c.h - pad - NU_PILL - 16);

        if page.shots().is_empty() {
            let (head, line) = match (page.loaded, &page.scope) {
                (false, _) => ("Looking for screenshots…", ""),
                (true, Scope::All) => ("No screenshots yet", "Hold HOME and press A while you play to take one. It shows up here."),
                (true, Scope::Game(_)) => ("No screenshots of this game", "Hold HOME and press A while you play to take one. It shows up here."),
            };
            // A camera with a blank photo above the words once the folder is read (`ui_art`).
            let base = top + 60 + type_scale::grow(37);
            let dy = if page.loaded { pocket_ui::ui_art::above_words(c, pocket_ui::ui_art::Art::Screenshots, left, false, top + 8, base, 37) } else { 0 };
            self.text.draw(c, Face::Title, 37, left, base + dy, pal().text, head);
            let mut y = top + 110 + type_scale::grow(37) + type_scale::grow(NU_SMALL) + dy;
            for l in self.text.wrap(Face::Body, NU_SMALL, line, right - left).into_iter().take(3) {
                self.text.draw(c, Face::Body, NU_SMALL, left, y, pal().quiet, &l);
                y += type_scale::line(NU_SMALL, 12);
            }
            return;
        }

        let all = matches!(page.scope, Scope::All);
        let (day_h, shot_h, gap) = (type_scale::row(56, 37), type_scale::row(92, 37) + type_scale::grow(28), 8);
        let first = page.follow(bottom - top, (day_h, shot_h + gap));
        let rows = page.rows();
        let today = page.today;
        let mut y = top;
        let mut last_drawn = first;
        for (at, row) in rows.iter().enumerate().skip(first) {
            let h = if matches!(row, Row::Day(_)) { day_h } else { shot_h + gap };
            // A day's heading is never the last thing on screen: its first picture has to fit under it.
            let need = if matches!(row, Row::Day(_)) { h + shot_h } else { h };
            if y + need > bottom + gap {
                break;
            }
            last_drawn = at;
            match *row {
                Row::Day(d) => {
                    let label = g::day_label(d, today);
                    self.text.draw(c, Face::Title, 37, left, y + day_h - 14 - type_scale::grow(37) / 3, pal().dust, &label);
                }
                Row::Shot(i) => {
                    let shot = page.shots()[i].clone();
                    let selected = i == page.sel;
                    self.hits.row(Area::new(left, y, right - left, shot_h), i);
                    self.paint_gallery_row(c, &shot, (left, y, right - left, shot_h), selected, all);
                }
            }
            y += h;
        }
        self.hits.list(page.sel, page.shots().len(), 0.0, (shot_h + gap) as f32);
        if rows.len() > last_drawn - first + 1 {
            let track = bottom - top;
            let (bar_h, bar_y) = (track * (last_drawn - first + 1) as i32 / rows.len() as i32, track * first as i32 / rows.len() as i32);
            c.rect(c.w - 12, top, 4, track, pal().pill);
            c.rect(c.w - 12, top + bar_y, 4, bar_h.max(12), pal().chip);
        }
    }

    /// One screenshot's spine: its system's stripe, the small picture, the game (or the time) and what else.
    fn paint_gallery_row(&mut self, c: &mut Canvas, shot: &Shot, (x, y, w, h): (i32, i32, i32, i32), selected: bool, all: bool) {
        c.rect(x, y, w, h, if selected { pal().label } else { pal().spine });
        c.rect(x, y, STRIPE, h, if selected { stripe(&shot.system) } else { pocket_ui::gfx::mix(stripe(&shot.system), pal().spine, 150) });
        let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
        // The small picture in a tile that is there before the picture is: nothing moves when it arrives.
        let (tx, ty) = (x + STRIPE + 16, y + (h - THUMB.1) / 2);
        c.rect(tx, ty, THUMB.0, THUMB.1, pal().void);
        match self.gallery_thumb(&shot.path) {
            Some(Some(img)) => c.blit_scaled(&img, tx + (THUMB.0 - img.w as i32) / 2, ty + (THUMB.1 - img.h as i32) / 2, img.w as i32, img.h as i32),
            // A file that can't be read: a mark in the tile, so it is clear that it is the file, not the wait.
            Some(None) => {
                let mw = self.text.width(Face::Title, 37, "?");
                self.text.draw(c, Face::Title, 37, tx + (THUMB.0 - mw) / 2, ty + THUMB.1 / 2 + type_scale::base(13, 37), pal().dust, "?");
            }
            None => {}
        }
        let text_x = tx + THUMB.0 + 20;
        let clock = if all { shot.when.clock() } else { format!("{}:{:02}", shot.when.clock(), shot.when.sec) };
        let clock_w = self.text.width(Face::Body, 28, &clock);
        if all {
            // Two lines: the game and when, the console under it.
            let block = type_scale::px(37) as i32 + 6 + type_scale::px(28) as i32;
            let top = y + (h - block) / 2;
            let base1 = top + type_scale::px(37) as i32 * 4 / 5;
            let base2 = top + type_scale::px(37) as i32 + 6 + type_scale::px(28) as i32 * 4 / 5;
            self.text.draw_right(c, Face::Body, 28, x + w - 24, base1, dim, &clock);
            let title = self.text.fit(Face::Title, 37, &shot.title(), x + w - 24 - clock_w - 24 - text_x);
            self.text.draw(c, Face::Title, 37, text_x, base1, fg, &title);
            let side = self.text.fit(Face::Body, 28, &system_short(&self.systems, &shot.system), x + w - 24 - text_x);
            self.text.draw(c, Face::Body, 28, text_x, base2, dim, &side);
        } else {
            // The game's own list: the time, with the seconds that tell two in a minute apart.
            let n = if shot.n > 1 { format!("  ({})", shot.n) } else { String::new() };
            let base = y + h / 2 + type_scale::base(13, 37);
            self.text.draw(c, Face::Title, 37, text_x, base, fg, &clock);
            if !n.is_empty() {
                let cw = self.text.width(Face::Title, 37, &clock);
                self.text.draw(c, Face::Body, 28, text_x + cw + 4, base, dim, &n);
            }
        }
    }

    /// The picture alone on black, with a line over it saying whose and when.
    fn paint_gallery_full(&mut self, c: &mut Canvas, page: &mut Gallery) {
        let Some(shot) = page.current().cloned() else { return };
        let (pad, all) = (NU_PAD, matches!(page.scope, Scope::All));
        let left = pad + NU_BTN_PAD;
        let right = c.w - pad - NU_BTN_PAD;
        // Over the picture: the game (or the day) and the time, how far along the list we are.
        let line1 = type_scale::px(37) as i32 + pad;
        let line2 = line1 + type_scale::line(28, 8);
        let place = g_place(page);
        let place_w = self.text.width(Face::Body, 28, &place);
        self.text.draw_right(c, Face::Body, 28, right, line1, pal().dust, &place);
        let head = if all { shot.title() } else { g::day_label(shot.when.date(), page.today) };
        let head = self.text.fit(Face::Title, 37, &head, right - left - place_w - 32);
        self.text.draw(c, Face::Title, 37, left, line1, pal().chalk, &head);
        let mut x = left;
        if all {
            x += self.text.draw(c, Face::Body, 28, left, line2, pal().dust, &g::day_label(shot.when.date(), page.today)) + 20;
        }
        let when = format!("{}:{:02}", shot.when.clock(), shot.when.sec);
        self.text.draw(c, Face::Body, 28, x, line2, pal().dust, &when);

        let (top, bottom) = (line2 + 24, c.h - pad - NU_PILL - 16);
        let room = (c.w - 2 * pad, bottom - top);
        let shown = self.gallery_full(&shot.path, room);
        match &shown {
            Shown::Picture(img) => {
                let (w, h) = (img.w as i32, img.h as i32);
                c.blit_scaled(img, (c.w - w) / 2, top + (room.1 - h) / 2, w, h);
            }
            Shown::Loading => self.centered_gallery(c, room, top, "Loading…", ""),
            Shown::Broken => self.centered_gallery(c, room, top, "This screenshot can't be opened", "The file may be damaged. You can still delete it with X."),
        }
        // Next to read: the neighbours, once this one is on screen, so Left and Right are instant.
        if matches!(shown, Shown::Picture(_)) {
            for at in [page.sel.wrapping_sub(1), page.sel + 1] {
                if let Some(next) = page.shots().get(at) {
                    self.gallery_full(&next.path.clone(), room);
                }
            }
        }
        // Touch: a swipe browses; a tap on the left or right third does too.
        self.hits.button(Area::new(0, top, c.w / 3, room.1), Button::Left);
        self.hits.button(Area::new(c.w - c.w / 3, top, c.w / 3, room.1), Button::Right);
        self.hits.list(page.sel, page.shots().len(), c.w as f32 / 4.0, 0.0);
    }

    fn centered_gallery(&mut self, c: &mut Canvas, room: (i32, i32), top: i32, head: &str, line: &str) {
        let w = room.0 - 2 * NU_BTN_PAD;
        let head = self.text.fit(Face::Title, 37, head, w);
        let hw = self.text.width(Face::Title, 37, &head);
        let mid = top + room.1 / 2;
        self.text.draw(c, Face::Title, 37, (c.w - hw) / 2, mid, pal().text, &head);
        let mut y = mid + type_scale::line(NU_SMALL, 18);
        for l in self.text.wrap(Face::Body, NU_SMALL, line, w).into_iter().take(2) {
            let lw = self.text.width(Face::Body, NU_SMALL, &l);
            self.text.draw(c, Face::Body, NU_SMALL, (c.w - lw) / 2, y, pal().quiet, &l);
            y += type_scale::line(NU_SMALL, 12);
        }
    }

    /// X was pressed: the question, on a sheet over the dimmed page. The hints say A and B.
    fn paint_gallery_question(&mut self, c: &mut Canvas, page: &Gallery) {
        let Some(shot) = page.current() else { return };
        c.shade_rect(0, 0, c.w, c.h, pal().void, 225);
        pocket_ui::text::boxes::covered();
        let w = type_scale::wide(820, 37).min(c.w - 2 * NU_PAD - 96);
        let h = 150 + type_scale::grow(50) + 2 * type_scale::grow(28);
        let (x, y) = ((c.w - w) / 2, (c.h - h) / 2 - 40);
        c.rect(x - 48, y - 48, w + 96, h + 96, pal().void);
        if pal().t > 0 {
            c.frame(x - 48, y - 48, w + 96, h + 96, 1, pal().pick(VOID, 0xd6d0c4));
        }
        let head = self.text.fit(Face::Title, 50, "Delete this screenshot?", w);
        self.text.draw(c, Face::Title, 50, x, y + type_scale::px(50) as i32, pal().chalk, &head);
        let what = format!("{} · {}", shot.title(), shot.when.clock());
        let what = self.text.fit(Face::Body, 28, &what, w);
        let b1 = y + 60 + type_scale::px(50) as i32 + type_scale::px(28) as i32;
        self.text.draw(c, Face::Body, 28, x, b1, pal().dust, &what);
        let mut by = b1 + type_scale::line(28, 14);
        for l in self.text.wrap(Face::Body, 28, "It is taken off the SD card for good.", w).into_iter().take(2) {
            self.text.draw(c, Face::Body, 28, x, by, pal().chalk, &l);
            by += type_scale::line(28, 8);
        }
    }

    // ---- snapshots -------------------------------------------------------------------------------------------

    /// `--snapshot gallery[:<what>][:<row>]` with made-up screenshots in `<data>/gallery-made-up` (today is 24 October
    /// 2026): `gallery` the list of Settings → Games → Screenshots, `:<row>` another one selected, `:full` the picture
    /// alone, `:delete` and `:full-delete` the question X asks, `:broken` the list with a damaged file on top selected
    /// (`:broken-full` the picture view of it), `:empty` nothing there, `:loading` before the folder is read, `:game` the
    /// list of one game, `:game-full` its picture, `:card` the game's card on its "Screenshots (N)" row, `:shelf` and
    /// `:full-shelf` the list and the picture after Y made it the shelf's picture.
    pub(crate) fn gallery_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("gallery") else { return false };
        let mut what = "list";
        let mut row = 0;
        for part in rest.split(':').filter(|p| !p.is_empty()) {
            match part.parse::<usize>() {
                Ok(r) => row = r,
                Err(_) => what = part,
            }
        }
        let today = (2026, 10, 24);
        let dir = self.data_dir.join("gallery-made-up");
        self.pictures.sync = true;
        self.pictures.set_wanted(true);
        let _ = std::fs::remove_dir_all(&dir);
        let games = self.gallery_made_up_games();
        if what != "empty" && what != "loading" {
            let named: Vec<(String, String, (u32, u32))> = games.iter().map(|&(s, i, size)| (self.systems[s].name.clone(), stem_of(&self.systems[s].games[i].path), size)).collect();
            made_up_screenshots(&dir, &named, what.starts_with("broken"));
        }
        self.gallery.made_up = Some((today, dir.clone()));
        self.gallery.index = if what == "loading" { Index::default() } else { Index::load(&dir) };
        let full = what.ends_with("full") || what == "full-delete" || what == "full-shelf";
        let scope_game = what.starts_with("game") || what == "card";
        if scope_game {
            let Some((s, i, _)) = games.first().cloned() else { return true };
            let (system, name, path) = (self.systems[s].name.clone(), self.systems[s].games[i].name.clone(), self.systems[s].games[i].path.clone());
            self.show_card(super::Item::Game(s, i), system, name, path);
            if what == "card" {
                return true;
            }
            self.open_gallery_for_card();
        } else {
            self.settings_page = Some(self.open_gallery().unwrap_or(SettingsPage::Screenshots));
        }
        if let Some(page) = self.gallery.page.as_mut() {
            page.loaded = what != "loading";
            page.sel = row.min(page.shots().len().saturating_sub(1));
            if full {
                page.view = View::Full;
            }
            if what.ends_with("delete") {
                page.press(Button::North, CONFIRM, CANCEL);
            }
            if what.ends_with("shelf") {
                page.said(SHELF_SAID);
            }
        }
        true
    }

    /// After the first paint of the card snapshot: Down until the card is on its Screenshots row.
    pub(crate) fn gallery_snapshot_after(&mut self, state: &str) {
        if state != "gallery:card" {
            return;
        }
        for _ in 0..12 {
            let Some(open) = self.card.as_mut() else { return };
            if open.card.focus.and_then(|i| open.card.data.rows.get(i)).is_some_and(|r| r.kind == RowKind::Screenshots) {
                return;
            }
            open.card.press(Button::Down);
        }
    }

    /// Games of the made-up library to take pictures of: (system index, game index, size of its screen), the first
    /// games of a few systems.
    fn gallery_made_up_games(&self) -> Vec<(usize, usize, (u32, u32))> {
        let mut out = Vec::new();
        for (id, size) in [("gba", (240, 160)), ("gb", (160, 144)), ("snes", (256, 224)), ("ps2", (640, 448))] {
            if let Some(s) = self.systems.iter().position(|s| s.name == id) {
                if !self.systems[s].games.is_empty() {
                    out.push((s, 0, size));
                }
            }
        }
        out
    }
}

/// "3 of 12".
fn g_place(page: &Gallery) -> String {
    page.place()
}

/// What Y came to, beside the hint bar (or above it, where four hints leave no room).
const SHELF_SAID: &str = "This is now the game's picture on the shelf";

/// "12 screenshots".
fn count_text(n: usize) -> String {
    if n == 1 {
        "1 screenshot".into()
    } else {
        format!("{n} screenshots")
    }
}

/// The ROM's file name without the extension: what the screenshot's file name is made from.
fn stem_of(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Made-up screenshots for the snapshots: ten pictures over five weeks of the first games of a few systems (the first
/// game most), in the file names the screenshot key makes. `broken`: two damaged files, a PNG cut off after its header and
/// one that is no PNG at all, are the newest.
fn made_up_screenshots(dir: &Path, games: &[(String, String, (u32, u32))], broken: bool) {
    const WHEN: [(usize, &str); 10] = [
        (0, "2026-10-24 18-05-09"),
        (0, "2026-10-24 17-40-31"),
        (1, "2026-10-24 12-10-02"),
        (0, "2026-10-23 21-30-48"),
        (2, "2026-10-23 08-15-20"),
        (0, "2026-10-20 19-02-11"),
        (3, "2026-10-02 22-45-00"),
        (1, "2026-09-18 16-20-37"),
        (0, "2026-09-18 16-02-12"),
        (1, "2026-09-12 10-00-00"),
    ];
    if games.is_empty() {
        return;
    }
    for (n, (who, when)) in WHEN.iter().enumerate() {
        let (system, stem, (w, h)) = &games[who % games.len()];
        let folder = dir.join(screenshot::clean(system));
        let _ = std::fs::create_dir_all(&folder);
        let png = made_up_picture(*w, *h, n as u32);
        let _ = std::fs::write(folder.join(format!("{} {when}.png", screenshot::clean(stem))), png);
    }
    if broken {
        let (system, stem, (w, h)) = &games[0];
        let folder = dir.join(screenshot::clean(system));
        let name = screenshot::clean(stem);
        let whole = made_up_picture(*w, *h, 3);
        let _ = std::fs::write(folder.join(format!("{name} 2026-10-24 18-30-00.png")), &whole[..whole.len().min(60)]);
        let _ = std::fs::write(folder.join(format!("{name} 2026-10-24 18-20-00.png")), b"this is not a picture");
    }
}

/// A picture of `w` x `h` that looks a little like a game: a sky going from one colour to another by `seed`, a
/// ground, a few blocks and a figure. Deterministic.
fn made_up_picture(w: u32, h: u32, seed: u32) -> Vec<u8> {
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let hue = |k: u32| -> [u32; 3] { [40 + (seed * 53 + k * 90) % 160, 60 + (seed * 31 + k * 40) % 140, 110 + (seed * 17 + k * 20) % 120] };
    let (sky_a, sky_b) = (hue(0), hue(1));
    let ground = h * 7 / 10;
    let block = (w / 12).max(2);
    for y in 0..h {
        for x in 0..w {
            let t = y * 255 / h;
            let mut px = if y >= ground {
                let check = ((x / block) + (y / block)) % 2;
                [60 + check * 30, 110 + check * 25, 50]
            } else {
                [(sky_a[0] * (255 - t) + sky_b[0] * t) / 255, (sky_a[1] * (255 - t) + sky_b[1] * t) / 255, (sky_a[2] * (255 - t) + sky_b[2] * t) / 255]
            };
            // Floating blocks.
            let by = ground.saturating_sub(block * 4);
            if y >= by && y < by + block && (x / block) % 3 == seed % 3 && x / block > 1 && x / block < 10 {
                px = [200, 150, 60];
            }
            // The figure.
            let fx = w / 4 + (seed * block) % (w / 2);
            if x >= fx && x < fx + block && y >= ground.saturating_sub(block * 2) && y < ground {
                px = [220, 60, 60];
            }
            rgb.extend(px.iter().map(|v| (*v).min(255) as u8));
        }
    }
    screenshot::encode_rgb(w, h, &rgb).unwrap_or_default()
}
