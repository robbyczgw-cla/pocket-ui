//! Part of the pocket-ui binary: the App's side of the game switcher (`pocket_ui::switcher`),
//! favourites (`pocket_ui::favorites`) and scrolling titles (`pocket_ui::marquee`). Kept out of
//! main.rs so the shelf and pause menu only carry small hooks.

use std::time::{Duration, Instant, SystemTime};

use pocket_ui::gfx::Canvas;
use pocket_ui::lastframe;
use pocket_ui::pad::Button;
use pocket_ui::recents;
use pocket_ui::switcher::{self, Action, Entry, Switcher};
use pocket_ui::text::Face;

use super::{stripe, system_label, system_short, App, Item, Page, Screen, CONFIRM};

/// Star next to a starred game's name: on black, and on the paper label.
const STAR: u32 = 0xf2c14e;
const STAR_ON_PAPER: u32 = 0xb07a12;

/// Room a star takes after a title set in `px`.
fn star_room(px: u32) -> i32 {
    px as i32 * 4 / 5
}

impl App {
    /// The Favorites shelf: starred games that are still in the library, by name.
    pub(crate) fn favorite_items(&self) -> Vec<Item> {
        let items = self
            .favorites
            .list
            .iter()
            .filter_map(|f| {
                let s = self.systems.iter().position(|s| s.name == f.system)?;
                let g = self.systems[s].games.iter().position(|g| g.path == f.path)?;
                Some(Item::Game(s, g))
            })
            .collect::<Vec<_>>();
        // A game starred on two systems shows once (Settings > Curator > Merge versions).
        self.curator_merge(items)
    }

    pub(crate) fn is_favorite(&self, item: Item) -> bool {
        self.item_rom(item).is_some_and(|(_, path)| self.favorites.contains(&path))
    }

    /// Y on a game: star or unstar it.
    pub(crate) fn toggle_favorite(&mut self) {
        let (system, name, path) = match self.items.get(self.sel).copied() {
            Some(Item::Recent(i)) => {
                let r = &self.recents.list[i];
                (r.system.clone(), r.name.clone(), r.path.clone())
            }
            Some(Item::Game(s, g)) => (
                self.systems[s].name.clone(),
                self.systems[s].games[g].name.clone(),
                self.systems[s].games[g].path.clone(),
            ),
            _ => return,
        };
        let starred = self.favorites.toggle(&system, &name, &path);
        self.curator.touch();
        self.toast = Some((
            if starred { "Added to Favourites" } else { "Removed from Favourites" }.to_string(),
            Instant::now(),
        ));
        self.rebuild();
        // Unstarring the last favourite empties its shelf: back to Home.
        if self.screen == Screen::Favorites && self.items.is_empty() {
            self.screen = Screen::Home;
            self.rebuild();
            // On Continue, under the Search row (`app_search`).
            self.sel = Item::first_row(&self.items);
            self.prev_sel = self.sel;
        }
    }

    /// Heading of a game shelf.
    pub(crate) fn screen_title(&self) -> Option<String> {
        match self.screen {
            Screen::Home => None,
            Screen::Games(s) => Some(self.systems[s].fullname.clone()),
            Screen::Favorites => Some("Favourites".to_string()),
            screen => self.curator_screen_title(screen),
        }
    }

    /// Width of a title with its star, before any clipping.
    pub(crate) fn title_width(&mut self, face: Face, px: u32, title: &str, fav: bool) -> i32 {
        self.text.width(face, px, title) + if fav { star_room(px) } else { 0 }
    }

    /// A row's title in `max_w` pixels: the selected row's scrolls when it is too long, the others end in
    /// an ellipsis. A starred game gets its star right after the name.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_title(
        &mut self,
        c: &mut Canvas,
        face: Face,
        px: u32,
        x: i32,
        baseline: i32,
        max_w: i32,
        color: u32,
        title: &str,
        selected: bool,
        fav: bool,
    ) {
        let room = max_w - if fav { star_room(px) } else { 0 };
        let w = if selected {
            self.marquee.draw(&mut self.text, c, face, px, x, baseline, room, color, title);
            self.text.width(face, px, title).min(room)
        } else {
            let fitted = self.text.fit(face, px, title, room);
            self.text.draw(c, face, px, x, baseline, color, &fitted)
        };
        if fav {
            let r = (px as i32 * 3 / 10).max(7);
            let luma = (color >> 16 & 0xff) * 3 + (color >> 8 & 0xff) * 6 + (color & 0xff);
            let star = if luma < 1000 { STAR_ON_PAPER } else { STAR };
            switcher::star(c, x + w + px as i32 / 5 + r, baseline - px as i32 * 36 / 100, r, star);
        }
    }

    fn switcher_entry(&self, system: &str, name: &str, path: &std::path::Path, meta: String) -> Entry {
        let mut cover = pocket_ui::covers::cover_path(&self.data_dir, system, path);
        // No box art of its own: the picture in the player's EmulationStation gamelist (`app_gamelist`).
        if !cover.exists() {
            cover = self.gamelist_cover(system, path).unwrap_or(cover);
        }
        Entry::new(
            system,
            name,
            path,
            system_short(&self.systems, system),
            meta,
            stripe(system),
            self.favorites.contains(path),
            Some(cover),
        )
        .with_frame(&lastframe::frame_path(&self.data_dir, system, path))
    }

    /// A game has ended: keeps the picture the pause menu showed when Quit was chosen as the game's last
    /// frame for the switcher, unless the emulator wrote a resume preview of its own during the session. The
    /// menu grabbed that picture with grim when it opened (time limits included), so nothing is captured
    /// here; scaling and writing happen on a thread. A session that ended any other way (crash, hotkey) has
    /// no frame, and the one of an earlier session goes, so the picture is never older than the last quit.
    pub(crate) fn keep_last_frame(&mut self, run: &super::Running) {
        let started = SystemTime::now() - run.started.elapsed() - Duration::from_secs(2);
        let quit_from_menu = self.menu.as_ref().is_some_and(|m| m.quitting);
        // A Quit without saving (Settings > Remember where I quit off): the next start goes back to the last saving quit, and so does the picture of it
        // kept then (an emulator's own resume preview was not written either).
        if self.menu.as_ref().is_some_and(|m| m.discarding) {
            return;
        }
        let screen = if quit_from_menu && lastframe::needs_frame(pocket_ui::switcher::resume_preview_written(&run.system, &run.path), started) {
            self.menu.as_mut().and_then(|m| m.frame.take())
        } else {
            None
        };
        let (dir, system, rom) = (self.data_dir.clone(), run.system.clone(), run.path.clone());
        match screen {
            Some(screen) => {
                std::thread::spawn(move || {
                    if !lastframe::keep(&dir, &system, &rom, &screen) {
                        lastframe::forget(&dir, &system, &rom);
                    }
                });
            }
            None => lastframe::forget(&dir, &system, &rom),
        }
    }

    /// Opens the switcher: on the shelf with the recent games, in a game with the running one first.
    pub(crate) fn open_switcher(&mut self, in_game: bool, from_menu: bool) {
        let mut entries = Vec::new();
        let current = self
            .running
            .as_ref()
            .filter(|_| in_game)
            .map(|r| (r.system.clone(), r.name.clone(), r.path.clone()));
        if let Some(e) = current.as_ref().and_then(|_| self.now_playing_entry()) {
            entries.push(e);
        }
        for r in self
            .recents
            .list
            .iter()
            // A hidden system's games stay out (Settings > Look > Systems on the shelf), and games hidden on their card;
            // the one being played stays in.
            .filter(|r| current.as_ref().is_none_or(|c| c.2 != r.path) && self.recent_visible(r))
        {
            let meta = format!(
                "{}, played {}, {} in total",
                system_label(&self.systems, &r.system),
                recents::ago(r.last_played),
                recents::duration(r.play_secs)
            );
            entries.push(self.switcher_entry(&r.system, &r.name, &r.path, meta));
        }
        let only_hidden = entries.is_empty() && self.recents.list.iter().any(|r| r.path.exists() && !pocket_ui::shelf::recent_shown(&r.system, &self.hidden));
        let mut sw = Switcher::new(entries, in_game);
        sw.from_menu = from_menu;
        sw.only_hidden = only_hidden;
        self.switcher = Some(sw);
        self.held = None;
        self.marquee.restart();
    }

    /// The running game's card: the screen as the pause menu has it, the game's part of it.
    fn now_playing_entry(&self) -> Option<Entry> {
        let r = self.running.as_ref()?;
        let meta = format!("{}, playing now", system_label(&self.systems, &r.system));
        let screen = self.menu.as_ref().and_then(|m| m.frame.as_ref());
        // pocket-core draws the game where its Picture setting says; other emulators: trim the black.
        let scaling = self
            .menu
            .as_ref()
            .filter(|m| m.hosted)
            .and_then(|m| m.settings.as_ref())
            .map(|s| pocket_ui::video::VideoSettings::from_values(&s.effective()).scaling);
        Some(self.switcher_entry(&r.system, &r.name, &r.path, meta).now_playing(screen, scaling))
    }

    /// pocket-core's picture of the paused game came after the switcher opened (HOME + SELECT, `app_menu_open`):
    /// the "Now playing" card shows it too.
    pub(crate) fn switcher_now_playing_picture(&mut self) {
        let Some(at) = self.switcher.as_ref().filter(|s| s.in_game).and_then(|s| s.entries.iter().position(|e| e.current)) else { return };
        if let Some(e) = self.now_playing_entry() {
            if let Some(sw) = self.switcher.as_mut() {
                sw.entries[at] = e;
            }
        }
    }

    pub(crate) fn switcher_input(&mut self, b: Button) {
        let Some(sw) = self.switcher.as_mut() else { return };
        match sw.input(b) {
            Action::None => {}
            Action::Close => {
                let back_to_game = sw.in_game && !sw.from_menu;
                self.switcher = None;
                if back_to_game {
                    self.resume_game(None);
                }
            }
            Action::Play(i) => {
                let e = &sw.entries[i];
                let target = (e.system.clone(), e.name.clone(), e.path.clone());
                if !sw.in_game {
                    self.switcher = None;
                    self.launch(target.0, target.1, target.2);
                } else if e.current {
                    self.switcher = None;
                    self.resume_game(None);
                } else {
                    // The running game saves and quits the way Quit in the pause menu does; the chosen one
                    // starts when it has exited (`after_game_for_switcher`) and resumes from its own state.
                    pocket_ui::fast_switch::mark("switcher: A");
                    sw.switching = true;
                    self.switch_to = Some(target);
                    self.quit_for_switch();
                    pocket_ui::fast_switch::mark("switcher: quit sent");
                }
            }
            Action::Remove(i) => {
                let path = sw.entries[i].path.clone();
                sw.remove(i);
                self.recents.remove(&path);
                self.rebuild();
            }
            Action::Favorite(i) => {
                let e = &sw.entries[i];
                let (system, name, path) = (e.system.clone(), e.name.clone(), e.path.clone());
                let starred = self.favorites.toggle(&system, &name, &path);
                if let Some(e) = self.switcher.as_mut().and_then(|s| s.entries.get_mut(i)) {
                    e.favorite = starred;
                }
                if self.running.is_none() {
                    self.rebuild();
                }
            }
        }
    }

    /// Quit from the pause menu, which saves the game first (pocket-core and RetroArch write their
    /// resume state, ARMSX2 and Dolphin theirs).
    fn quit_for_switch(&mut self) {
        // Between two pocket-core games the next one starts as soon as this one has saved (`app_fast_switch`).
        if let Some(target) = self.switch_to.clone() {
            if self.quit_for_handoff(&target) {
                return;
            }
        }
        let Some(menu) = self.menu.as_mut() else { return };
        menu.page = Page::Main;
        // Always the saving Quit, whatever Settings > Remember where I quit says: switching sets a game aside, and the
        // switcher shows and continues it from where it was left.
        match menu.items().iter().position(|i| *i == super::MainItem::Quit) {
            Some(q) => menu.sel = q,
            None => return,
        }
        menu.without_saving = false;
        self.menu_input(CONFIRM);
    }

    /// In a game: HOME + SELECT opens the switcher (over the pause menu, which pauses the game), and
    /// SELECT does in the pause menu. Returns true when the event was the switcher's.
    pub(crate) fn switcher_in_game_input(&mut self, button: Button, pressed: bool) -> bool {
        if button == Button::Mode {
            self.home_down = pressed;
        }
        if self.switcher.is_some() {
            if pressed {
                self.switcher_input(button);
            }
            return true;
        }
        if !pressed || button != Button::Select {
            return false;
        }
        if self.menu.is_none() && self.home_down {
            // Not a HOME tap: the release must not open the pause menu on its own.
            self.home_tap = false;
            self.open_menu();
            if self.menu.is_some() {
                self.open_switcher(true, false);
            }
            return true;
        }
        if self.menu.as_ref().is_some_and(|m| m.page == Page::Main && !m.quitting) {
            self.open_switcher(true, true);
            return true;
        }
        false
    }

    /// After a game ended: start the game chosen in the switcher, if any.
    pub(crate) fn after_game_for_switcher(&mut self, retried: bool) {
        self.home_down = false;
        self.switcher = None;
        if let Some((system, name, path)) = self.switch_to.take() {
            if !retried {
                pocket_ui::fast_switch::mark("switcher: old game done, starting the next");
                println!("switcher: starting {}", path.display());
                self.launch(system, name, path);
            }
        }
    }

    pub(crate) fn paint_switcher(&mut self, c: &mut Canvas) {
        let Some(mut sw) = self.switcher.take() else { return };
        sw.paint(c, &mut self.text, &mut self.marquee);
        let (hints, switching) = (sw.hints(), sw.switching);
        // Cards farthest first, the selected one on top: later areas win, as the cards do.
        for (x, y, w, h, i) in &sw.card_areas {
            self.hits.row(pocket_ui::touch::Area::new(*x, *y, *w, *h), *i);
        }
        self.hits.list(sw.sel, sw.entries.len(), switcher::STEP / 2.0, 0.0);
        self.switcher = Some(sw);
        self.status_pill(c);
        if !switching {
            self.nu_hints(c, &hints);
        }
    }
}
