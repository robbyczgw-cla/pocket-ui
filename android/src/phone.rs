//! Touch-first phone presentation: a bottom tab bar, dp-sized type and touch targets, scrolling shelves with
//! momentum, and controller hints only while a controller is the input. The same Ui state, painters and shared
//! media as the controller path; sizes come from the density (`Ui::dp`), not from a second set of features.
use super::*;
use pocket_ui::ui_art::{Art, Icon};

/// Material's minimum touch target.
const TARGET: f32 = 48.0;
/// The tabs, in order: what they open, their label and their icon. Curator only with Settings → Curator tab (`Ui::has_tab`).
const TABS: [(Screen, &str, Icon); 5] = [
    (Screen::Library, "Games", Icon::Games),
    (Screen::Favorites, "Favourites", Icon::Favourites),
    (Screen::Search, "Search", Icon::Search),
    (Screen::Curator, "Curator", Icon::Curator),
    (Screen::Settings, "Settings", Icon::System),
];

/// The room the current page has between its header and the bars at the bottom.
#[derive(Clone, Copy)]
pub(super) struct Room {
    pub top: i32,
    pub bottom: i32,
    pub margin: i32,
}

/// One row of a sheet list: its name, its value at the right, a tick box for a filter.
pub(super) struct MenuRow {
    pub label: String,
    pub value: String,
    pub check: Option<bool>,
    /// The value has arrows beside it (`Action::Year`).
    pub stepper: bool,
}

impl Ui {
    /// `v` dp in canvas pixels.
    pub(super) fn d(&self, v: f32) -> i32 {
        (v * self.dp).round() as i32
    }
    /// A type size of `v` dp (sp: Android's font scale reaches it through `type_scale`).
    pub(super) fn sp(&self, v: f32) -> u32 {
        (v * self.dp).round().max(8.0) as u32
    }
    /// Controller hints and the selection's band show only while a controller (or keyboard) is the input.
    pub(super) fn controller_hints(&self) -> bool {
        !self.touch_mode
    }
    /// Phones and small tablets: the shared game card is drawn on a smaller canvas and enlarged, so its type is
    /// as readable as the rest of the phone layout. 1.0 elsewhere.
    pub fn zoom(&self, landscape: bool) -> f32 {
        if !self.prefs.onboarding_done || self.screen != Screen::Box {
            1.0
        } else if landscape && self.prefs.theme == Theme::Split {
            // The info page needs 900 px beside its buttons for its two columns (cover left, facts right).
            0.8
        } else {
            0.62
        }
    }
    /// The canvas the page is painted on for a safe area of `w` x `h` screen pixels at `density`: the screen's own
    /// pixels, or a smaller one that is enlarged (the shared game card, the controls card). Sets `dp` to the canvas'
    /// pixels per dp. The launcher's window and the phone-format tests use this one rule.
    pub fn canvas_for(&mut self, (w, h): (u32, u32), density: f32) -> (u32, u32) {
        let zoom = self.zoom(w > h);
        let (lw, lh) = if self.controls_open {
            // This shared prose painter uses the same mobile canvas as the player, without the box's extra enlargement.
            pocket_ui::scaled_layout::mobile_size((w, h), density)
        } else if zoom < 1.0 {
            let (mw, mh) = pocket_ui::scaled_layout::mobile_size((w, h), density);
            let k = (w.min(h) as f32 * zoom / mw.min(mh) as f32).clamp(0.5, 1.0);
            (((mw as f32 * k) as u32).max(1), ((mh as f32 * k) as u32).max(1))
        } else {
            (w, h)
        };
        self.dp = lw as f32 / (w as f32 / density.max(0.5));
        (lw, lh)
    }
    /// A region that does `action` when tapped; its label is the accessibility name (translated).
    pub(super) fn target(&mut self, area: Area, label: &str, action: Action) {
        self.region(area, label.into(), action);
    }
    /// A game/system data name remains literal for screen readers too.
    fn name_target(&mut self, area: Area, name: &str, action: Action) {
        self.regions.push(Region { area, label: name.into(), action });
    }
    /// What Play does for the selected game: goes back to its moment when it stands open.
    fn play_word(&self) -> &'static str {
        if self.game().is_some_and(|g| self.is_frozen(&g.key)) {
            "Continue"
        } else {
            "Play"
        }
    }
    /// The primary button of the selected game. Named Play for screen readers and tests whatever it says: the line
    /// under the name reads out Continue.
    fn play_pill(&mut self, c: &mut Canvas, text: &mut Text, area: Area) {
        let word = self.play_word();
        self.pill(c, text, area, word, true, Action::Press(Button::East));
        let experimental = self.game().is_some_and(|g| self.experimental(g.system_id));
        if let Some(region) = self.regions.last_mut() {
            region.label = if experimental {
                format!("{} · {}", super::tr("Play"), super::tr("Experimental"))
            } else { super::tr("Play") };
        }
    }
    /// The line under a game's name: its system, after "Continue" when it stands open.
    fn system_line(&self, game: &Game) -> String {
        if self.is_frozen(&game.key) {
            format!("{} · {}", super::tr("Continue"), super::play_cards::system_label(self.experimental(game.system_id), game.system))
        } else {
            super::play_cards::system_label(self.experimental(game.system_id), game.system)
        }
    }
    /// A pill button: paper with ink for the primary action, the sheet's pill colour otherwise. The label is
    /// centred and never cut while it fits at a slightly smaller size.
    pub(super) fn pill(&mut self, c: &mut Canvas, text: &mut Text, area: Area, label: &str, primary: bool, action: Action) {
        let colors = if primary { (pal().label, pal().ink) } else { (pal().pill, pal().chalk) };
        self.pill_in(c, text, area, label, colors, action);
    }
    pub(super) fn pill_in(&mut self, c: &mut Canvas, text: &mut Text, area: Area, label: &str, (bg, fg): (u32, u32), action: Action) {
        if pocket_ui::retro::on() { pocket_ui::retro::frame_at(c, area.x, area.y, area.w, area.h, bg, self.d(2.0).max(1)); }
        else { c.round_rect(area.x, area.y, area.w, area.h, area.h / 2, bg); }
        let mut size = self.sp(16.0);
        let room = area.w - self.d(24.0);
        while size > self.sp(12.0) && text.width(Face::Title, size, label) > room {
            size -= 1;
        }
        let shown = text.fit(Face::Title, size, label, room);
        let tw = text.width(Face::Title, size, &shown);
        text.draw(c, Face::Title, size, area.x + (area.w - tw) / 2, area.y + area.h / 2 + type_mid(size), fg, &shown);
        self.target(area, label, action);
    }
    /// The page's title at the top left and a quiet note at its right: returns where the content starts.
    /// `keep`: pixels at the right for what stands beside the title.
    fn header(&mut self, c: &mut Canvas, text: &mut Text, title: &str, note: &str, keep: i32) -> i32 {
        let margin = self.margin(c);
        let h = self.d(56.0);
        let size = self.sp(26.0);
        let base = h / 2 + type_mid(size);
        let small = self.sp(14.0);
        let gap = self.d(12.0);
        let right = c.w - margin;
        let note_w = if note.is_empty() { 0 } else { text.width(Face::Body, small, note) };
        let note_room = if note.is_empty() { 0 } else { note_w + gap };
        // The status corner, as on the handhelds, at the right end of the top line: it gives way (a step at a time,
        // down to nothing) before it cuts a letter of the title or runs into the note or the shelf buttons.
        let room = if keep > 0 {
            // Landscape shelf: the free end above the side panel, right of the two shelf buttons.
            right - (c.w - keep + self.d(112.0) + self.d(8.0)) - note_room
        } else {
            let line = right - margin;
            // The title comes first: the corner is shown only where the whole title stands beside it.
            line - text.width(Face::Title, size, title) - gap - note_room
        };
        let corner = self.corner_for(text, room, small);
        let corner_w = -corner.layout_at(0, small, |s| text.width(Face::Body, small, s)).1;
        let reserve = (if corner_w > 0 { corner_w + gap } else { 0 } + note_room).max(keep);
        let shown = text.fit(Face::Title, size, title, c.w - 2 * margin - reserve);
        text.draw(c, Face::Title, size, margin, base, pal().chalk, &shown);
        #[cfg(test)]
        {
            self.header_rects = vec![("margin", margin, h), ("title", margin, margin + text.width(Face::Title, size, &shown))];
            if corner_w > 0 {
                self.header_rects.push(("corner", right - corner_w, right));
            }
            if !note.is_empty() {
                let end = if corner_w > 0 { right - corner_w - gap } else { right };
                self.header_rects.push(("note", end - note_w, end));
            }
        }
        // A title that is also a tab's name is already spoken by the tab (and taps by name find the tab).
        if TABS.iter().all(|(_, label, _)| tr(label) != tr(title)) {
            self.target(Area::new(margin, 0, c.w - 2 * margin - reserve, h), title, Action::Label);
        }
        if corner_w > 0 {
            corner.paint_at(c, text, right, base, small, pal().dust);
        }
        if !note.is_empty() {
            let end = if corner_w > 0 { right - corner_w - gap } else { right };
            text.draw_right(c, Face::Body, small, end, base, pal().dust, note);
        }
        h
    }
    /// The status corner for a header with `room` pixels for it at type size `px`: empty while Android's own status
    /// bar shows (no second clock), where nothing is known (a device without a battery still shows the time) and when
    /// not even the time fits.
    pub(super) fn corner_for(&self, text: &mut Text, room: i32, px: u32) -> pocket_ui::status_corner::Corner {
        if self.system_bar {
            return Default::default();
        }
        self.corner.fit(pocket_ui::status_corner::unix_now(), room, px, |s| text.width(Face::Body, px, s))
    }
    pub(super) fn margin(&self, c: &Canvas) -> i32 {
        self.d(if c.w > c.h { 24.0 } else { 16.0 })
    }
    /// The bottom tab bar: icon over label in portrait, beside it in landscape (Material's compact-height bar).
    /// Returns its top. Hidden while the on-screen keyboard has the bottom of the page.
    pub(super) fn tab_bar(&mut self, c: &mut Canvas, text: &mut Text) -> i32 {
        let landscape = c.w > c.h;
        let h = self.d(if landscape { 56.0 } else { 72.0 });
        let top = c.h - h;
        c.rect(0, top, c.w, h, pal().void);
        c.rect(0, top, c.w, 1.max(self.d(0.5)), pal().pill);
        // The sticker sheet belongs where its card came from.
        let current = match if self.screen == Screen::Stickers { self.prior } else { self.screen } {
            Screen::Results | Screen::Facet => Screen::Curator,
            // A fact's list belongs to the card's tab, Rediscover to Curator.
            Screen::Connection if self.list_from_curator() => Screen::Curator,
            Screen::Connection | Screen::Apps => Screen::Library,
            Screen::Quick | Screen::Extras | Screen::GameCheck | Screen::Updates | Screen::Licences | Screen::Saves | Screen::Report | Screen::Help | Screen::Achievements | Screen::PlaysWith => Screen::Settings,
            s => s,
        };
        let margin = if landscape { self.d(24.0) } else { 0 };
        let tabs: Vec<_> = TABS.into_iter().filter(|(screen, _, _)| self.has_tab(*screen)).collect();
        let tw = (c.w - 2 * margin) / tabs.len() as i32;
        let k = (self.d(24.0) / 16).max(1);
        let icon = 16 * k;
        let size = self.sp(if landscape { 14.0 } else { 12.5 });
        for (i, (screen, label, glyph)) in tabs.into_iter().enumerate() {
            let x = margin + i as i32 * tw;
            let on = current == screen;
            let shown_label = tr(label);
            // Smaller before cut: "Einstellungen" keeps its whole word on a 360 dp phone.
            let mut s = size;
            let room = if landscape { tw - icon - self.d(28.0) } else { tw - self.d(6.0) };
            while s > self.sp(10.0) && text.width(Face::Title, s, &shown_label) > room {
                s -= 1;
            }
            let shown = text.fit(Face::Title, s, label, room);
            let lw = text.width(Face::Title, s, &shown);
            let ink = if on { pal().chalk } else { pal().dust };
            if landscape {
                let pill_w = icon + lw + self.d(36.0);
                let px = x + (tw - pill_w) / 2;
                let ph = self.d(40.0);
                let py = top + (h - ph) / 2;
                if on {
                    c.round_rect(px, py, pill_w, ph, ph / 2, pal().label);
                }
                let fg = if on { pal().ink } else { ink };
                pocket_ui::ui_art::paint_tinted(c, Art::Icon(glyph), px + self.d(14.0), py + (ph - icon) / 2, k, fg);
                text.draw(c, Face::Title, s, px + self.d(22.0) + icon, py + ph / 2 + type_mid(s), fg, &shown);
            } else {
                let pill_w = self.d(56.0);
                let ph = self.d(30.0);
                let py = top + self.d(8.0);
                if on {
                    c.round_rect(x + (tw - pill_w) / 2, py, pill_w, ph, ph / 2, pal().label);
                }
                pocket_ui::ui_art::paint_tinted(c, Art::Icon(glyph), x + (tw - icon) / 2, py + (ph - icon) / 2, k, if on { pal().ink } else { ink });
                text.draw(c, Face::Title, s, x + (tw - lw) / 2, py + ph + self.d(18.0), ink, &shown);
            }
            self.target(Area::new(x, top, tw, h), label, Action::Tab(screen));
        }
        top
    }
    /// The controller's keys for this page, above the tab bar: round key caps and their words in Dust.
    /// Painted only while a controller is the input (touch users have the buttons on the page instead).
    pub(super) fn hint_row(&mut self, c: &mut Canvas, text: &mut Text, bottom: i32, hints: &[(&str, &str)]) -> i32 {
        if !self.controller_hints() || hints.is_empty() {
            return bottom;
        }
        let h = self.d(40.0);
        let top = bottom - h;
        let size = self.sp(13.0);
        let key_h = self.d(26.0);
        let mut x = self.margin(c);
        for (key, label) in hints {
            let kw = (text.width(Face::Body, size, key) + self.d(12.0)).max(key_h);
            let lw = text.width(Face::Body, size, label);
            if x + kw + self.d(6.0) + lw > c.w - self.margin(c) {
                break;
            }
            let ky = top + (h - key_h) / 2;
            c.round_rect(x, ky, kw, key_h, key_h / 2, pal().keycap);
            let tw = text.width(Face::Body, size, key);
            text.draw(c, Face::Body, size, x + (kw - tw) / 2, ky + key_h / 2 + type_mid(size), pal().chalk, key);
            x += kw + self.d(6.0);
            text.draw(c, Face::Body, size, x, ky + key_h / 2 + type_mid(size), pal().dust, label);
            x += lw + self.d(16.0);
        }
        top
    }
    /// A tick box `s` square at (x, y): a frame, filled with a drawn tick when on.
    pub(super) fn check_box(&self, c: &mut Canvas, x: i32, y: i32, s: i32, on: bool) {
        let t = self.d(2.0).max(2);
        if on {
            c.round_rect(x, y, s, s, s / 6, pal().chalk);
            // The tick: a short and a long stroke, drawn as squares along each line.
            let (ax, ay) = (x + s * 22 / 100, y + s * 52 / 100);
            let (bx, by) = (x + s * 42 / 100, y + s * 72 / 100);
            let (cx, cy) = (x + s * 78 / 100, y + s * 30 / 100);
            for (x0, y0, x1, y1) in [(ax, ay, bx, by), (bx, by, cx, cy)] {
                let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
                for i in 0..=n {
                    c.rect(x0 + (x1 - x0) * i / n - t / 2, y0 + (y1 - y0) * i / n - t / 2, t + 1, t + 1, pal().void);
                }
            }
        } else {
            c.round_rect(x, y, s, s, s / 6, pal().dust);
            c.round_rect(x + t, y + t, s - 2 * t, s - 2 * t, s / 8, pal().void);
        }
    }
    /// A radio mark `s` across centred at (cx, cy): a ring, with a dot when chosen.
    pub(super) fn radio(&self, c: &mut Canvas, cx: i32, cy: i32, s: i32, on: bool, color: u32) {
        let t = self.d(2.0).max(2);
        c.ring(cx, cy, s / 2, t, color);
        if on {
            c.disc(cx, cy, s / 2 - 2 * t - 1, color);
        }
    }

    /// The whole page in the phone layout (everything after the first start).
    pub(super) fn paint_phone(&mut self, c: &mut Canvas, text: &mut Text, cover: Option<&Image>) {
        if self.screen == Screen::Box {
            self.paint_card_phone(c, text, cover);
            return;
        }
        let keyboard = self.keyboard.is_some();
        let bar = if keyboard { c.h } else { self.tab_bar(c, text) };
        let hints: Vec<(&str, &str)> = match self.screen {
            _ if keyboard => vec![("A", "Choose"), ("B", "Delete"), ("START", "Done")],
            Screen::Updates if self.updates.busy() && self.updates.cancellable => vec![("B", "Cancel")],
            Screen::Updates => vec![("A", "Choose"), ("B", "Back")],
            Screen::GameCheck => vec![("A", "Choose"), ("B", "Back"), ("X", "Check again")],
            Screen::Stickers | Screen::Saves | Screen::Report | Screen::Help | Screen::Achievements | Screen::PlaysWith | Screen::Licences => vec![("A", "Choose"), ("B", "Back")],
            Screen::Quick => vec![("A", "Choose"), ("X", "Show the pick"), ("B", "Back"), ("L1/R1", "Tabs")],
            Screen::Settings | Screen::Extras | Screen::Curator | Screen::Facet => vec![("A", "Choose"), ("B", "Back"), ("L1/R1", "Tabs")],
            Screen::Apps => vec![("A", "Open"), ("Y", "Android shelf"), ("B", "Back"), ("L1/R1", "Tabs")],
            Screen::Library => vec![("A", self.play_word()), ("X", "Info"), ("Y", "Favourite"), ("R2", "Apps"), ("L1/R1", "Tabs"), ("START", "Settings")],
            _ => vec![("A", self.play_word()), ("X", "Info"), ("Y", "Favourite"), ("L1/R1", "Tabs"), ("START", "Settings")],
        };
        let bottom = if keyboard { bar } else { self.hint_row(c, text, bar, &hints) };
        let (title, note) = match self.screen {
            // One shelf at a time in landscape: the page says which.
            Screen::Library if c.w > c.h && self.shelf_row().is_some() => {
                let row = self.shelf_row().unwrap();
                let rows = self.rows.as_ref().unwrap();
                (tr(if row.key == pocket_ui::cabinet::ShelfKey::Continue { "Continue" } else { rows.title(rows.row_of(row.start).unwrap_or(0)) }), games_note(row.end - row.start))
            }
            Screen::Library => ("Games".to_string(), if self.library.is_some() { self.count_note() } else { String::new() }),
            Screen::Favorites => ("Favourites".into(), self.count_note()),
            Screen::Search => ("Search".into(), if self.query.is_empty() { String::new() } else { self.count_note() }),
            Screen::Results => ("Curator".into(), self.count_note()),
            Screen::Connection => (self.connection_title(), self.count_note()),
            Screen::Facet => (tr(self.facet.label()), String::new()),
            Screen::Curator => ("Curator".into(), String::new()),
            Screen::Quick => ("Quick settings".into(), String::new()),
            Screen::Extras => ("Cozy extras".into(), String::new()),
            Screen::Stickers => ("Stickers".into(), self.sticker_note()),
            Screen::GameCheck => (self.check_title(), String::new()),
            Screen::Saves => ("Save backup".into(), String::new()),
            Screen::Report => ("Report a problem".into(), String::new()),
            Screen::Help => ("Help".into(), String::new()),
            Screen::Achievements => ("RetroAchievements".into(), String::new()),
            Screen::PlaysWith => ("Plays with".into(), String::new()),
            Screen::Updates => ("Updates".into(), String::new()),
            Screen::Licences => ("Licences".into(), String::new()),
            Screen::Apps => ("Android apps".into(), self.apps_note()),
            _ => ("Settings".into(), String::new()),
        };
        let keep = if self.screen == Screen::Library && self.shelf_row().is_some() && c.w > c.h {
            c.w - self.landscape_shelf_w(c) + self.d(112.0)
        } else {
            0
        };
        let mut top = self.header(c, text, &title, &note, keep);
        if self.screen == Screen::Extras && !self.core_note.is_empty() {
            // Download status is a sentence, not the short right-aligned game count of a header.
            let message = self.core_note.clone();
            let margin = self.margin(c);
            let size = self.sp(14.0);
            let height = type_scale::px(size) as i32;
            let start = top;
            for line in text.wrap(Face::Body, size, &message, c.w - 2 * margin) {
                text.draw(c, Face::Body, size, margin, top + height, pal().dust, &line);
                top += height + self.d(6.0);
            }
            self.target(Area::new(margin, start, c.w - 2 * margin, top - start), &message, Action::Label);
            top += self.d(12.0);
        }
        // Why the stickers don't show is read out too.
        if self.screen == Screen::Stickers && !note.is_empty() {
            self.target(Area::new(c.w / 2, 0, c.w / 2, top), &note, Action::Label);
        }
        self.grid_cols = 0;
        self.row_pitch = 0;
        self.row_bands.clear();
        let room = Room { top, bottom, margin: self.margin(c) };
        match self.screen {
            // Curator has nothing to sort before there are games: say so, with the way to add them.
            Screen::Curator | Screen::Facet if self.library.as_ref().is_none_or(|l| l.games.is_empty()) => self.paint_empty_phone(c, text, room),
            Screen::Settings | Screen::Extras | Screen::Quick | Screen::Curator | Screen::Facet | Screen::Stickers => {
                let rows = self.menu_rows();
                self.paint_menu_phone(c, text, room, rows);
            }
            Screen::GameCheck => self.paint_check(c, text, room),
            Screen::Saves => self.paint_saves(c, text, room),
            Screen::Report => self.paint_report(c, text, room),
            Screen::Help => self.paint_help(c, text, room),
            Screen::Achievements => self.paint_achievements(c, text, room),
            Screen::PlaysWith => self.paint_plays_with(c, text, room),
            Screen::Updates => self.paint_updates(c, text, room),
            Screen::Licences => self.paint_licences(c, text, room),
            Screen::Search => self.paint_search_phone(c, text, room, cover),
            Screen::Apps => self.paint_apps(c, text, room),
            _ => self.paint_games_phone(c, text, room, cover),
        }
        if keyboard {
            self.paint_keyboard_phone(c, text);
        }
    }
    fn count_note(&self) -> String {
        // A game can stand on several shelves: the library is what counts there.
        let n = match (&self.rows, &self.library) {
            (Some(_), Some(lib)) => lib.games.len(),
            _ => self.visible.len(),
        };
        games_note(n)
    }
    /// Why the stickers can't be seen, where they can't.
    fn sticker_note(&self) -> String {
        if !self.prefs.extras.case_stickers {
            tr("Stickers on cases is off")
        } else if self.prefs.theme == Theme::Split {
            tr("This look shows no cases")
        } else {
            String::new()
        }
    }

    /// Settings, quick settings, Curator and a facet's values: one-line rows 56 dp high, the name at the left and
    /// the value at the right in Dust; a filter's values with tick boxes. Taps choose at once.
    pub(super) fn paint_menu_phone(&mut self, c: &mut Canvas, text: &mut Text, room: Room, mut rows: Vec<MenuRow>) {
        // Curator's "Show games" is what the page is for: a primary button that stays above the tabs.
        let mut room = room;
        if rows.last().is_some_and(|r| r.label == tr("Show games")) {
            let last = rows.len() - 1;
            rows.pop();
            let bh = self.d(52.0);
            let area = Area::new(room.margin, room.bottom - bh - self.d(12.0), c.w - 2 * room.margin, bh);
            let label = match (self.screen, self.library.as_ref()) {
                (Screen::Facet | Screen::Curator, Some(lib)) => {
                    let n = lib.index.list(&lib.index.matches(&self.lens), false).len();
                    tr(&if n == 1 { "Show 1 game".to_string() } else { format!("Show {n} games") })
                }
                _ => tr("Show games"),
            };
            if self.controller_hints() && self.menu == last {
                c.frame(area.x - 4, area.y - 4, area.w + 8, area.h + 8, self.d(2.0), pal().chalk);
            }
            self.pill(c, text, area, &label, true, Action::Row(last));
            room.bottom = area.y - self.d(8.0);
        }
        let pitch = self.d(56.0).max(type_scale::row(self.d(56.0), self.sp(16.0)));
        let size = self.sp(16.0);
        let top = room.top + self.d(4.0);
        self.scroll_max = (rows.len() as i32 * pitch - (room.bottom - top)).max(0) as f32;
        if self.follow {
            self.reveal(self.menu as i32 * pitch, pitch, room.bottom - top);
        }
        self.scroll = self.scroll.clamp(0.0, self.scroll_max);
        let offset = self.scroll.round() as i32;
        let w = c.w;
        let margin = room.margin;
        let hints = self.controller_hints();
        let mut targets = Vec::new();
        let mut steppers = Vec::new();
        {
            let mut clip = c.rows(top, room.bottom);
            for (i, row) in rows.iter().enumerate() {
                let y = i as i32 * pitch - offset;
                if y + pitch < 0 || y > clip.h {
                    continue;
                }
                let selected = hints && i == self.menu;
                if selected {
                    c_band(&mut clip, margin / 2, y + self.d(4.0), w - margin, pitch - self.d(8.0), pal().select);
                }
                let (fg, quiet) = if selected { (pal().ink, pal().ink) } else { (pal().text, pal().dust) };
                let base = y + pitch / 2 + type_mid(size);
                let mut x = margin;
                if let Some(on) = row.check {
                    let s = self.d(22.0);
                    self.check_box(&mut clip, x, y + (pitch - s) / 2, s, on);
                    x += s + self.d(16.0);
                }
                let arrows = if row.stepper { self.d(TARGET) } else { 0 };
                let value_w = if row.value.is_empty() { 0 } else { (text.width(Face::Body, size, &row.value) + self.d(16.0) + 2 * arrows).min((w - x - margin) * 2 / 3) };
                // A long name ("Buscar actualizaciones automáticamente") goes a step smaller, then on to a second line,
                // before a word of it is cut: a 360 dp phone has 200 dp for it beside a switch.
                let room_w = w - x - margin - value_w;
                let small = [size, self.sp(15.0), self.sp(14.0)].into_iter().find(|s| text.width(Face::Body, *s, &row.label) <= room_w).unwrap_or(self.sp(14.0));
                let lines = text.wrap_at_most(Face::Body, small, &row.label, room_w, 2);
                if lines.last().is_some_and(|l| l.ends_with('…')) {
                    self.cut.push(row.label.clone());
                }
                let line_px = type_scale::px(small) as i32 * 6 / 5;
                let first = y + pitch / 2 + type_mid(small) - (lines.len() as i32 - 1) * line_px / 2;
                for (k, line) in lines.iter().enumerate() {
                    text.draw(&mut clip, Face::Body, small, x, first + k as i32 * line_px, fg, line);
                }
                if !row.value.is_empty() {
                    let value = text.fit(Face::Body, size, &row.value, value_w - self.d(16.0) - 2 * arrows);
                    if value != row.value {
                        self.cut.push(row.value.clone());
                    }
                    text.draw_right(&mut clip, Face::Body, size, w - margin - arrows, base, quiet, &value);
                    if row.stepper {
                        for (ax, back) in [(w - margin - value_w + self.d(16.0), true), (w - margin - arrows, false)] {
                            let mid = ax + arrows / 2;
                            let big = self.d(7.0);
                            for dx in 0..=big {
                                let reach = if back { dx } else { big - dx };
                                clip.rect(mid - big / 2 + dx, y + pitch / 2 - reach, 1, 2 * reach + 1, fg);
                            }
                        }
                        steppers.push((Area::new(w - margin - value_w + self.d(16.0), top + y, arrows, pitch).clip_y(top, room.bottom), -1));
                        steppers.push((Area::new(w - margin - arrows, top + y, arrows, pitch).clip_y(top, room.bottom), 1));
                    }
                }
                if i + 1 < rows.len() {
                    clip.rect(margin, y + pitch - 1, w - 2 * margin, 1, pal().pill);
                }
                // A tick box says its state to a screen reader, where no number stands beside it.
                let spoken = match (row.value.is_empty(), row.check) {
                    (true, Some(on)) => format!("{} · {}", row.label, tr(if on { "On" } else { "Off" })),
                    (true, None) => row.label.clone(),
                    (false, _) => format!("{} · {}", row.label, row.value),
                };
                targets.push((Area::new(0, top + y, w, pitch).clip_y(top, room.bottom), spoken, i));
            }
        }
        for (area, label, i) in targets {
            if area.h > 0 {
                self.target(area, &label, Action::Row(i));
            }
        }
        for (area, delta) in steppers {
            if area.h > 0 {
                self.target(area, if delta < 0 { "Earlier" } else { "Later" }, Action::Year(delta));
            }
        }
        self.scrollbar(c, top, room.bottom - top, rows.len() as i32 * pitch);
    }
    /// How many lines of the words above a page's rows there is room for: half the page, or all but one row
    /// (a phone held across has 150 dp for both; the rows scroll, the words do not): the words may take all but one row.
    pub(super) fn words_limit(&self, room: &Room, step: i32) -> usize {
        let height = room.bottom - room.top;
        let row = self.d(56.0).max(type_scale::row(self.d(56.0), self.sp(16.0)));
        (height / 2 / step).max((height - row - self.d(10.0)) / step).max(2) as usize
    }
    /// The words above a page's rows wrapped to `w`: the usual size, or a step smaller when that is the only way they
    /// all have room (a 720p phone held across at larger text). Returns the lines, their size and step, and how many fit.
    pub(super) fn words_lines(&mut self, text: &mut Text, room: &Room, words: &str, w: i32) -> (Vec<String>, u32, i32, usize) {
        let mut out = None;
        for dp in [15.0, 13.0] {
            let size = self.sp(dp);
            let step = size as i32 + self.d(7.0);
            let most = self.words_limit(room, step);
            let lines: Vec<String> = words.lines().flat_map(|line| text.wrap(Face::Body, size, line, w)).collect();
            let fits = lines.len() <= most;
            out = Some((lines, size, step, most));
            if fits {
                break;
            }
        }
        out.unwrap()
    }
    /// A thin bar at the right edge while a page scrolls.
    pub(super) fn scrollbar(&self, c: &mut Canvas, top: i32, view: i32, content: i32) {
        if content <= view || view <= 0 {
            return;
        }
        let bar = (view * view / content).max(self.d(24.0));
        let y = top + ((view - bar) as f32 * self.scroll / self.scroll_max.max(1.0)) as i32;
        c.round_rect(c.w - self.d(5.0), y, self.d(3.0), bar, self.d(1.5), pal().chip);
    }
    /// Scrolls so `y..y + h` (content coordinates) is inside a view `view` high.
    pub(super) fn reveal(&mut self, y: i32, h: i32, view: i32) {
        let (y, h, view) = (y as f32, h as f32, view as f32);
        if y < self.scroll {
            self.scroll = y;
        } else if y + h > self.scroll + view {
            self.scroll = y + h - view;
        }
        self.follow = false;
    }

    /// Games, Favourites and Curator's results: the shelf in the selected look. Portrait: several shelves of
    /// fronts that scroll up and down (or a list in the list looks) with the selected game and its Play button in a
    /// strip above the tabs. Landscape: the handheld's shelf, as tall as the page, the selected game beside it.
    fn paint_games_phone(&mut self, c: &mut Canvas, text: &mut Text, room: Room, cover: Option<&Image>) {
        let mut room = room;
        // The Games page's top row: Just Play, and the drawer with the phone's other apps (also before any games, and
        // in Easy, where it is the only button up there).
        if self.screen == Screen::Library {
            let h = self.d(TARGET);
            let just_play = self.prefs.theme != Theme::Big && !self.visible.is_empty();
            let gap = self.d(8.0);
            let apps_w = if just_play { (c.w - 2 * room.margin - gap) / 3 } else { c.w - 2 * room.margin };
            if just_play {
                self.pill(c, text, Area::new(room.margin, room.top, c.w - 2 * room.margin - apps_w - gap, h), "Just Play", false, Action::JustPlay);
            }
            self.pill(c, text, Area::new(c.w - room.margin - apps_w, room.top, apps_w, h), "Apps", false, Action::Apps);
            if let Some(region) = self.regions.last_mut() {
                region.label = tr("Android apps");
            }
            room.top += h + gap;
        }
        if self.visible.is_empty() {
            self.paint_empty_phone(c, text, room);
            return;
        }
        if self.prefs.theme == Theme::Big {
            self.paint_easy_phone(c, text, room, cover);
            return;
        }
        let landscape = c.w > c.h;
        let list = self.prefs.theme == Theme::Split;
        if landscape {
            let side = c.w - self.landscape_shelf_w(c) - room.margin;
            let area = Room { bottom: room.bottom, ..room };
            let shelf_w = self.landscape_shelf_w(c);
            if list {
                self.paint_list_phone(c, text, area, shelf_w);
            } else {
                self.paint_shelf_landscape(c, text, area, shelf_w, cover);
                self.shelf_arrows(c, shelf_w);
            }
            self.side_panel(c, text, (shelf_w + self.d(16.0), room.top, side - self.d(16.0), room.bottom - room.top), cover);
        } else {
            let strip = self.name_band() + self.d(TARGET + 26.0) + pocket_ui::type_scale::px(self.sp(14.0)) as i32;
            let area = Room { bottom: room.bottom - strip, ..room };
            if list {
                self.paint_list_phone(c, text, area, c.w);
            } else if self.rows.is_some() && self.screen == Screen::Library {
                self.paint_rows(c, text, area, cover);
            } else {
                self.paint_grid(c, text, area, cover);
            }
            self.strip(c, text, room.bottom - strip, strip);
        }
    }
    /// Easy keeps one case and its direct Play/Continue action on screen. Navigation
    /// uses the existing selection, so touch, controller, return and rotation agree.
    fn paint_easy_phone(&mut self, c: &mut Canvas, text: &mut Text, room: Room, cover: Option<&Image>) {
        self.grid_cols = 1;
        let Some(index) = self.visible.get(self.selected).copied() else { return };
        let h = self.d(TARGET);
        let gap = self.d(8.0);
        let bottom = room.bottom - h - gap;
        let name_h = self.name_band().min(((bottom - room.top - pocket_ui::type_scale::px(self.sp(14.0)) as i32 - self.d(20.0)) / 2).max(1));
        let picture_bottom = bottom - name_h - pocket_ui::type_scale::px(self.sp(14.0)) as i32 - self.d(20.0);
        let available = (picture_bottom - room.top).max(1);
        if let Some(pic) = self.front_pic(text, index, true, (c.w - 2 * room.margin, available), cover) {
            art::blit(c, &pic, (c.w - pic.w) / 2, room.top + (available - pic.h) / 2, 256, (room.top, picture_bottom));
        }
        if let Some(game) = self.game().cloned() {
            self.paint_name(c, text, &game.name, Area::new(room.margin, picture_bottom, c.w - 2 * room.margin, name_h));
            let line = self.system_line(&game);
            text.draw_name(c, Face::Body, self.sp(14.0), room.margin, bottom - self.d(12.0), pal().dust, &line);
            self.name_target(Area::new(room.margin, picture_bottom, c.w - 2 * room.margin, bottom - picture_bottom), &game.name, Action::Press(Button::North));
            let arrow = h;
            self.pill(c, text, Area::new(room.margin, bottom, arrow, h), "‹", false, Action::Press(Button::Left));
            self.pill(c, text, Area::new(c.w - room.margin - arrow, bottom, arrow, h), "›", false, Action::Press(Button::Right));
            self.play_pill(c, text, Area::new(room.margin + arrow + gap, bottom, c.w - 2 * (room.margin + arrow + gap), h));
        }
    }
    /// How wide the landscape shelf is, beside the panel with the selected game.
    fn landscape_shelf_w(&self, c: &Canvas) -> i32 {
        c.w - (c.w * 34 / 100).clamp(self.d(220.0), self.d(360.0)) - self.margin(c)
    }
    /// Landscape: two buttons in the page's header, at the shelf's right end, go to the shelf above and below.
    fn shelf_arrows(&mut self, c: &mut Canvas, right: i32) {
        let Some(rows) = &self.rows else { return };
        if self.screen != Screen::Library {
            return;
        }
        let Some(at) = rows.row_of(self.selected) else { return };
        let last = rows.rows.len() - 1;
        let (w, h) = (self.d(48.0), self.d(44.0));
        let y = (self.d(56.0) - h) / 2;
        for (x, up, can, delta, label) in [(right - 2 * w - self.d(8.0), true, at > 0, -1, "Shelf above"), (right - w, false, at < last, 1, "Shelf below")] {
            c.round_rect(x, y, w, h, h / 2, pal().pill);
            let big = self.d(8.0);
            let ink = if can { pal().chalk } else { pal().dust };
            for dy in 0..=big {
                let reach = if up { dy } else { big - dy };
                c.rect(x + w / 2 - reach, y + h / 2 - big / 2 + dy, 2 * reach + 1, 1, ink);
            }
            self.target(Area::new(x, y, w, h), label, Action::Shelf(delta));
        }
    }
    /// Portrait: the selected game's name and system, Info and the Play button.
    fn strip(&mut self, c: &mut Canvas, text: &mut Text, y: i32, h: i32) {
        let Some(game) = self.game().cloned() else { return };
        let margin = self.margin(c);
        c.rect(0, y, c.w, h, pal().void);
        c.rect(0, y, c.w, 1, pal().pill);
        let bh = self.d(TARGET);
        let by = y + h - bh - self.d(8.0);
        let info_w = self.d(88.0);
        let info = Area::new(margin, by, info_w, bh);
        let play = Area::new(info.x + info.w + self.d(8.0), by, c.w - 2 * margin - info_w - self.d(8.0), bh);
        let name_w = c.w - 2 * margin;
        self.paint_name(c, text, &game.name, Area::new(margin, y + self.d(4.0), name_w, self.name_band()));
        let line = self.system_line(&game);
        let size = self.sp(14.0);
        let system = text.fit_name(Face::Body, size, &line, name_w);
        let system_base = by - self.d(8.0);
        text.draw_name(c, Face::Body, size, margin, system_base, pal().dust, &system);
        self.name_target(Area::new(margin, system_base - pocket_ui::type_scale::px(size) as i32, name_w, pocket_ui::type_scale::px(size) as i32), &line, Action::Label);
        self.name_target(Area::new(margin, y, name_w, by - y), &game.name, Action::Press(Button::North));
        self.pill(c, text, info, "Info", false, Action::Press(Button::North));
        self.play_pill(c, text, play);
    }
    /// Landscape: the selected game's picture (or the Cabinet's TV), its name and system, Info and Play.
    fn side_panel(&mut self, c: &mut Canvas, text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), cover: Option<&Image>) {
        let Some(game) = self.game().cloned() else { return };
        let bh = self.d(TARGET);
        let buttons_y = y + h - bh - self.d(8.0);
        let gap = self.d(8.0);
        let info_w = (w - gap) * 2 / 5;
        self.pill(c, text, Area::new(x, buttons_y, info_w, bh), "Info", false, Action::Press(Button::North));
        self.play_pill(c, text, Area::new(x + info_w + gap, buttons_y, w - info_w - gap, bh));
        let system_base = buttons_y - self.d(14.0);
        let line = self.system_line(&game);
        let system_px = pocket_ui::type_scale::px(self.sp(14.0)) as i32;
        // A phone held across with a hint bar leaves this panel under 110 dp: the name before the system line, which the
        // card says again (without the room the two stood on each other).
        let show_system = system_base - system_px - self.d(8.0) - (y + self.d(8.0)) >= pocket_ui::type_scale::px(self.sp(22.0)) as i32;
        let name_bottom = if show_system { system_base - system_px - self.d(8.0) } else { buttons_y - self.d(8.0) };
        if show_system {
            let system = text.fit_name(Face::Body, self.sp(14.0), &line, w);
            text.draw_name(c, Face::Body, self.sp(14.0), x, system_base, pal().dust, &system);
            self.name_target(Area::new(x, system_base - self.d(14.0), w, self.d(18.0)), &line, Action::Label);
        }
        let name_top = (name_bottom - self.name_band()).max(y + self.d(8.0));
        self.paint_name(c, text, &game.name, Area::new(x, name_top, w, (name_bottom - name_top).max(1)));
        self.name_target(Area::new(x, name_top, w, (if show_system { system_base } else { name_bottom }) - name_top), &game.name, Action::Press(Button::North));
        let pic_h = name_top - y - self.d(16.0);
        if pic_h < self.d(60.0) {
            return;
        }
        if self.prefs.theme == Theme::Cabinet && self.prefs.extras.cabinet_tv {
            self.paint_tv(c, (x, y + self.d(8.0), w, pic_h), cover);
        } else if let Some(image) = self.snapshot.as_ref().or(cover) {
            let (iw, ih) = image.fit(w, pic_h);
            c.blit_scaled(image, x + (w - iw) / 2, y + self.d(8.0) + (pic_h - ih) / 2, iw, ih);
        } else if let Some(sprite) = pocket_ui::system_art::get(game.system_id) {
            let fitted = sprite.fitted(w * 3 / 4, pic_h * 3 / 4);
            c.blit_sprite(&fitted, x + (w - fitted.w as i32) / 2, y + (pic_h - fitted.h as i32) / 2, 0);
        }
    }
    /// A stable three-line band: changing games never moves the system line or play buttons.
    fn name_band(&self) -> i32 {
        3 * pocket_ui::type_scale::px(self.sp(22.0)) as i32 + self.d(12.0)
    }
    fn paint_name(&self, c: &mut Canvas, text: &mut Text, name: &str, area: Area) {
        let lines = pocket_ui::title_block::name_layout_in(text, Face::Title, self.sp(22.0), name, area.w, area.h);
        let baseline = area.y + lines.first().map_or(0, |l| pocket_ui::type_scale::px(l.px) as i32);
        pocket_ui::title_block::paint(text, c, Face::Title, &lines, area.x, baseline, pal().chalk, pal().dust, false);
    }
    /// The Cabinet's TV showing the game's picture, in (x, y, w, h).
    fn paint_tv(&mut self, c: &mut Canvas, (tx, ty, tw, th): (i32, i32, i32, i32), cover: Option<&Image>) {
        let Some(&index) = self.visible.get(self.selected) else { return };
        let key = (index, tw, th);
        if self.tv.as_ref().is_none_or(|(old, _)| *old != key) {
            let f = pocket_ui::cabinet_tv::scale_for(tw, th, 8.0).unwrap_or(0.5);
            let mut body = pocket_ui::cabinet_tv::body(f);
            let (_, _, gw, gh) = body.glass;
            let game = &self.library.as_ref().unwrap().games[index];
            if let Some(screen) = self
                .snapshot
                .as_ref()
                .or(cover)
                .map(|i| pocket_ui::cabinet_tv::crt(i, gw, gh))
                .or_else(|| pocket_ui::cabinet_tv::console_screen(game.system_id, gw, gh))
            {
                pocket_ui::cabinet_tv::show(&mut body.pic, body.glass, &body.mask, &screen, 256);
            }
            self.tv = Some((key, body.pic));
        }
        if let Some((_, pic)) = &self.tv {
            art::blit(c, pic, tx + (tw - pic.w) / 2, ty + (th - pic.h) / 2, 256, (0, c.h));
        }
    }
    /// The front of game `index` (the cover on the selected one) fitted into `w` x `h`, from the cache.
    fn front_pic(&mut self, text: &mut Text, index: usize, selected: bool, (w, h): (i32, i32), cover: Option<&Image>) -> Option<art::Pic> {
        if self.cards.len() > 96 {
            self.cards.clear();
        }
        let lib = self.library.as_ref()?;
        let game = &lib.games[index];
        let key = (index, selected);
        // A frozen game's front keeps its size: what is inside comes out into the gap beside it.
        let frozen = self.frozen.contains(&game.key);
        let peek = self.d(AJAR_MOST_DP);
        // Just back from it (`eject`): what is inside comes out on its own clock, the picture made each frame meanwhile.
        let out = if frozen { self.ajar_out(&game.key, std::time::Instant::now()) } else { 1.0 };
        if let Some(pic) = self.cards.get(&key).filter(|_| out >= 1.0).filter(|p| {
            let front_w = p.w - if frozen { peek } else { 0 };
            front_w <= w && p.h <= h && (front_w + 2 >= w || p.h + 2 >= h)
        }) {
            return Some(pic.clone());
        }
        let m = Medium::of(game.system_id);
        let (fw, fh) = media::front_size(m, 1.0);
        let k = (w as f32 / fw as f32).min(h as f32 / fh as f32);
        let look = art::Look {
            color: game.color,
            band: game.color,
            plastic: art::plastic(game.system_id),
        };
        let console = pocket_ui::system_art::get(game.system_id);
        let label = if selected { cover } else { None };
        let mut pic = media::front(text, m, k, look, &game.name, label, console, game.system);
        let stickers = self.prefs.stickers_shown(&game.key);
        pocket_ui::case_stickers::on_front(text, &mut pic, &stickers, media::label_rect(m, k), index as u64);
        if pic.w > w || pic.h > h {
            let s = (w as f32 / pic.w as f32).min(h as f32 / pic.h as f32);
            pic = pic.scaled((pic.w as f32 * s) as i32, (pic.h as f32 * s) as i32);
        }
        if frozen {
            // What is inside comes a little out of the side (`app_cabinet::paint_ajar_game`); always this wide, so the
            // cached picture's front is known.
            if let Some(inside) = super::game_inside(text, game, (m, pic.h), look, label, self.prefs.extras.printed_discs) {
                pic = ajar_front(pic, &inside, peek, (peek as f32 * out).round() as i32);
            } else {
                let mut wider = art::Pic::new(pic.w + peek, pic.h, 0);
                for y in 0..pic.h {
                    wider.px[(y * wider.w) as usize..][..pic.w as usize].copy_from_slice(&pic.px[(y * pic.w) as usize..][..pic.w as usize]);
                }
                pic = wider;
            }
        }
        if out >= 1.0 {
            self.cards.insert(key, pic.clone());
        }
        Some(pic)
    }
    /// Portrait shelves: rows of fronts standing on boards (the Cabinet's compartments in the Cabinet look). The
    /// selected one is lifted; tap selects, tap again plays, a long press opens its card.
    fn paint_grid(&mut self, c: &mut Canvas, text: &mut Text, room: Room, cover: Option<&Image>) {
        let margin = room.margin;
        let cols = ((c.w as f32 / self.dp / 116.0) as usize).clamp(2, 6);
        self.grid_cols = cols;
        let gap = self.d(12.0);
        let cell_w = (c.w - 2 * margin - (cols as i32 - 1) * gap) / cols as i32;
        let case_h = cell_w * 3 / 2;
        let lift = self.d(16.0);
        let board = self.d(10.0);
        let row_h = lift + case_h + board + self.d(18.0);
        let rows = self.visible.len().div_ceil(cols) as i32;
        // The Shelf and Cabinet looks: the room's light on the wood and the cases, and one more shelf at the end with
        // what the player earned (Cozy extras → Light by time of day, Decorations).
        let (light, earned) = if matches!(self.prefs.theme, Theme::Shelf | Theme::Cabinet) {
            self.room_sync()
        } else {
            (pocket_ui::room_light::Light::NEUTRAL, None)
        };
        let shade = light.case_shade().min(255);
        let total = rows + earned.is_some() as i32;
        let view = room.bottom - room.top;
        self.scroll_max = (total * row_h + self.d(8.0) - view).max(0) as f32;
        if self.follow {
            self.reveal((self.selected / cols) as i32 * row_h, row_h, view);
        }
        self.scroll = self.scroll.clamp(0.0, self.scroll_max);
        self.step = (0.0, row_h as f32 / cols as f32);
        let offset = self.scroll.round() as i32;
        let mut targets = Vec::new();
        let first = (offset / row_h).max(0) as usize;
        let last = ((offset + view) / row_h + 1) as usize;
        let mut clip = c.rows(room.top, room.bottom);
        for r in first..=last.min(total.max(1) as usize - 1) {
            let y = r as i32 * row_h - offset + self.d(4.0);
            let base = y + lift + case_h;
            self.paint_board(&mut clip, r, base, (case_h, lift, board), margin, &light);
            if let (true, Some(earned)) = (r as i32 == rows, &earned) {
                let right = clip.w - margin;
                self.paint_deco_props(text, &mut clip, (margin, right), base, case_h, shade, (0, i32::MAX), earned);
                continue;
            }
            for col in 0..cols {
                let pos = r * cols + col;
                let Some(&index) = self.visible.get(pos) else { break };
                let selected = pos == self.selected;
                let Some(pic) = self.front_pic(text, index, selected, (cell_w, case_h), cover) else { continue };
                let x = margin + col as i32 * (cell_w + gap) + ((cell_w - pic.w) / 2).max(0);
                self.stand_case(&mut clip, &pic, (x, base, room.top), selected, lift, shade);
                let name = self.library.as_ref().map(|l| l.games[index].name.clone()).unwrap_or_default();
                targets.push((Area::new(margin + col as i32 * (cell_w + gap), room.top + y, cell_w + gap, row_h).clip_y(room.top, room.bottom), name, pos));
            }
        }
        for (area, name, pos) in targets {
            if area.h > self.d(12.0) {
                self.name_target(area, &name, Action::Row(pos));
            }
        }
        self.scrollbar(c, room.top, view, total * row_h);
    }
    /// A board of the portrait shelves, or a Cabinet's compartment, under cases standing on `base`.
    fn paint_board(&mut self, clip: &mut Canvas, r: usize, base: i32, (case_h, lift, board): (i32, i32, i32), margin: i32, light: &pocket_ui::room_light::Light) {
        if self.prefs.theme == Theme::Cabinet {
            let inner = case_h + lift + self.d(8.0);
            let key = (clip.w, inner, r % 3);
            if self.shelves.iter().all(|(k, _)| *k != key) {
                if self.shelves.len() > 6 {
                    self.shelves.clear();
                }
                let mut pic = art::compartment(clip.w - 2 * margin + 2 * self.d(8.0), inner, r as u32 % 3);
                light.tint_all(&mut pic.px);
                self.shelves.push((key, pic));
            }
            if let Some((_, pic)) = self.shelves.iter().find(|(k, _)| *k == key) {
                art::blit(clip, pic, margin - self.d(8.0), base - inner, 255, (0, i32::MAX));
            }
        } else {
            clip.rect(margin - self.d(6.0), base, clip.w - 2 * margin + self.d(12.0), board / 2, light.tint(pal().spine));
            clip.rect(margin - self.d(6.0), base + board / 2, clip.w - 2 * margin + self.d(12.0), board / 2, light.tint(pal().dust));
        }
    }
    /// A case standing on `base` at `x` of a portrait shelf, the selected one lifted. `top`: where `clip` starts on the page.
    fn stand_case(&mut self, clip: &mut Canvas, pic: &art::Pic, (x, base, top): (i32, i32, i32), selected: bool, lift: i32, shade: u32) {
        let up = if selected { lift } else { 0 };
        let py = base - pic.h - up;
        // Its start is on its way (`insert`): the case goes from here, its place stays empty.
        let leaving = selected && self.launching;
        if leaving {
            // In the light it stood in: the case leaves and comes home as the page shows it.
            self.front_at = Some(((x, top + py, pic.w, pic.h), art::lit(pic, shade)));
        }
        if selected && (!leaving || self.returning) {
            // The lifted case's shadow on the board.
            clip.shade_rect(x + self.d(4.0), base - self.d(3.0), pic.w - self.d(8.0), self.d(3.0), 0, 120);
        }
        if !leaving {
            art::blit(clip, pic, x, py, shade, (0, i32::MAX));
        }
        if selected && (!leaving || self.returning) && self.controller_hints() {
            clip.frame(x - self.d(3.0), py - self.d(3.0), pic.w + self.d(6.0), pic.h + self.d(6.0), self.d(2.0), pal().chalk);
        }
    }
    /// Portrait with several shelves: the Cabinet's order from top to bottom, each with its name and sliding along by
    /// itself, a little of the next case showing at the right end to say there is more.
    fn paint_rows(&mut self, c: &mut Canvas, text: &mut Text, room: Room, cover: Option<&Image>) {
        let Some(shelves) = self.rows.clone() else { return };
        let margin = room.margin;
        let cols = ((c.w as f32 / self.dp / 116.0) as usize).clamp(2, 6) as i32;
        let gap = self.d(12.0);
        let cell_w = ((c.w - margin - cols * gap) as f32 / (cols as f32 + 0.35)) as i32;
        let pitch = cell_w + gap;
        self.row_pitch = pitch;
        let case_h = cell_w * 3 / 2;
        let lift = self.d(16.0);
        let board = self.d(10.0);
        let heading = self.d(30.0);
        // Each shelf as high as its tallest case: a shelf of cartridges leaves no gap under its name.
        let heights: Vec<i32> = shelves
            .rows
            .iter()
            .map(|row| {
                let games = &shelves.games[row.start..row.end];
                // One system's games all have the same case.
                let games = if matches!(row.key, pocket_ui::cabinet::ShelfKey::System(_)) { &games[..1] } else { games };
                let lib = self.library.as_ref();
                let tall = games
                    .iter()
                    .filter_map(|&g| Some(media::front_size(Medium::of(lib?.games[g].system_id), 1.0)))
                    .map(|(fw, fh)| cell_w * fh / fw.max(1))
                    .max()
                    .unwrap_or(case_h)
                    .min(case_h);
                heading + lift + tall + board + self.d(14.0)
            })
            .collect();
        let (light, earned) = if matches!(self.prefs.theme, Theme::Shelf | Theme::Cabinet) {
            self.room_sync()
        } else {
            (pocket_ui::room_light::Light::NEUTRAL, None)
        };
        let shade = light.case_shade().min(255);
        let count = shelves.rows.len() as i32;
        let earned_h = lift + cell_w + board + self.d(14.0);
        let content = heights.iter().sum::<i32>() + if earned.is_some() { earned_h } else { 0 };
        let view = room.bottom - room.top;
        self.scroll_max = (content + self.d(8.0) - view).max(0) as f32;
        let picked = shelves.row_of(self.selected);
        if self.follow {
            if let Some(at) = picked {
                let row = shelves.rows[at];
                let x = (self.selected - row.start) as i32 * pitch;
                let most = ((row.end - row.start) as i32 * pitch - gap + 2 * margin - c.w).max(0) as f32;
                let scroll = self.row_scroll.entry(row.key).or_default();
                if (x as f32) < *scroll {
                    *scroll = x as f32;
                } else if (x + pitch + margin) as f32 > *scroll + c.w as f32 {
                    *scroll = (x + pitch + margin - c.w) as f32;
                }
                *scroll = scroll.clamp(0.0, most);
                self.reveal(heights[..at].iter().sum(), heights[at], view);
            }
        }
        self.scroll = self.scroll.clamp(0.0, self.scroll_max);
        self.step = (0.0, 0.0);
        let offset = self.scroll.round() as i32;
        let mut targets = Vec::new();
        let title_size = self.sp(15.0);
        let mut clip = c.rows(room.top, room.bottom);
        let mut next_y = self.d(4.0) - offset;
        for r in 0..count + earned.is_some() as i32 {
            let row_h = heights.get(r as usize).copied().unwrap_or(earned_h);
            let y = next_y;
            next_y += row_h;
            if y + row_h < 0 || y > view {
                continue;
            }
            let tall = row_h - heading - lift - board - self.d(14.0);
            let base = y + heading + lift + tall;
            if r == count {
                let base = y + lift + cell_w;
                self.paint_board(&mut clip, r as usize, base, (cell_w, lift, board), margin, &light);
                if let Some(earned) = &earned {
                    let right = clip.w - margin;
                    self.paint_deco_props(text, &mut clip, (margin, right), base, cell_w, shade, (0, i32::MAX), earned);
                }
                continue;
            }
            self.paint_board(&mut clip, r as usize, base, (tall, lift, board), margin, &light);
            let row = shelves.rows[r as usize];
            let n = row.end - row.start;
            let title = tr(if row.key == pocket_ui::cabinet::ShelfKey::Continue { "Continue" } else { shelves.title(r as usize) });
            let note = games_note(n);
            let note_w = text.width(Face::Body, self.sp(13.0), &note);
            let shown = text.fit(Face::Title, title_size, &title, clip.w - 2 * margin - note_w - self.d(12.0));
            text.draw(&mut clip, Face::Title, title_size, margin, y + heading / 2 + type_mid(title_size), pal().chalk, &shown);
            let right = clip.w - margin;
            text.draw_right(&mut clip, Face::Body, self.sp(13.0), right, y + heading / 2 + type_mid(title_size), pal().dust, &note);
            targets.push((Area::new(margin, room.top + y, clip.w - 2 * margin, heading).clip_y(room.top, room.bottom), title, None));
            let most = (n as i32 * pitch - gap + 2 * margin - clip.w).max(0) as f32;
            self.row_bands.push(((room.top + y).max(room.top), (room.top + y + row_h).min(room.bottom), row.key, most));
            let along = self.row_scroll.get(&row.key).copied().unwrap_or(0.0).clamp(0.0, most).round() as i32;
            let from = row.start + (along / pitch) as usize;
            for pos in from..row.end {
                let cell_x = margin + (pos - row.start) as i32 * pitch - along;
                if cell_x >= clip.w {
                    break;
                }
                let index = shelves.games[pos];
                let selected = pos == self.selected;
                let Some(pic) = self.front_pic(text, index, selected, (cell_w, tall), cover) else { continue };
                let x = cell_x + ((cell_w - pic.w) / 2).max(0);
                self.stand_case(&mut clip, &pic, (x, base, room.top), selected, lift, shade);
                let name = self.library.as_ref().map(|l| l.games[index].name.clone()).unwrap_or_default();
                let (x0, x1) = (cell_x.max(0), (cell_x + pitch).min(clip.w));
                targets.push((Area::new(x0, room.top + y + heading, x1 - x0, row_h - heading).clip_y(room.top, room.bottom), name, Some(pos)));
            }
        }
        for (area, name, pos) in targets {
            if area.h > self.d(12.0) && area.w > self.d(12.0) {
                self.name_target(area, &name, pos.map_or(Action::Label, Action::Row));
            }
        }
        self.scrollbar(c, room.top, view, content);
    }
    /// The list look (Split): a row per game with its system's stripe at the left edge, 64 dp high.
    fn paint_list_phone(&mut self, c: &mut Canvas, text: &mut Text, room: Room, right: i32) {
        let pitch = type_scale::row(self.d(64.0), self.sp(18.0));
        let view = room.bottom - room.top;
        self.scroll_max = (self.visible.len() as i32 * pitch - view).max(0) as f32;
        if self.follow {
            self.reveal(self.selected as i32 * pitch, pitch, view);
        }
        self.scroll = self.scroll.clamp(0.0, self.scroll_max);
        self.step = (0.0, pitch as f32);
        let offset = self.scroll.round() as i32;
        let margin = room.margin;
        let mut targets = Vec::new();
        {
            let mut clip = c.rows(room.top, room.bottom);
            let lib = self.library.as_ref().unwrap();
            let first = (offset / pitch).max(0) as usize;
            for (pos, &index) in self.visible.iter().enumerate().skip(first).take((view / pitch + 2) as usize) {
                let game = &lib.games[index];
                let y = pos as i32 * pitch - offset;
                let selected = pos == self.selected;
                let (x, w, h) = (margin, right - 2 * margin, pitch - self.d(6.0));
                kit::shelf_row(&mut clip, x, y, w, h, selected);
                clip.rect(x, y, self.d(6.0), h, game.color);
                let (fg, quiet) = if selected { (pal().ink, pal().ink) } else { (pal().chalk, pal().dust) };
                let favourite = self.prefs.favorites.contains(&game.key);
                let k = (self.d(16.0) / 16).max(1);
                let mut star_w = if favourite { 16 * k + self.d(12.0) } else { 0 };
                if favourite {
                    pocket_ui::ui_art::paint_tinted(&mut clip, Art::Icon(Icon::Favourites), x + w - self.d(12.0) - 16 * k, y + (h - 16 * k) / 2, k, if selected { pal().ink } else { pal().amber });
                }
                if self.frozen.contains(&game.key) {
                    text.draw_right(&mut clip, Face::Body, self.sp(13.0), x + w - self.d(12.0) - star_w, y + h / 2 + self.d(5.0), fg, &super::tr("Continue"));
                    star_w += text.width(Face::Body, self.sp(13.0), &super::tr("Continue")) + self.d(12.0);
                }
                let (name, name_size) = text.literal(|text| pocket_ui::title_block::row(text, Face::Title, self.sp(18.0), &game.name, w - self.d(32.0) - star_w));
                text.draw_name(&mut clip, Face::Title, name_size, x + self.d(18.0), y + h / 2 - self.d(2.0), fg, &name);
                let system = super::play_cards::system_label(self.experimental(game.system_id), game.system);
                let shown = text.fit_name(Face::Body, self.sp(13.0), &system, w - self.d(32.0));
                text.draw_name(&mut clip, Face::Body, self.sp(13.0), x + self.d(18.0), y + h / 2 + self.d(18.0), quiet, &shown);
                targets.push((Area::new(x, room.top + y, w, pitch).clip_y(room.top, room.bottom), if self.experimental(game.system_id) { format!("{} · {}", game.name, system) } else { game.name.clone() }, pos));
            }
        }
        for (area, name, pos) in targets {
            if area.h > self.d(12.0) {
                self.name_target(area, &name, Action::Row(pos));
            }
        }
        self.scrollbar(c, room.top, view, self.visible.len() as i32 * pitch);
    }
    /// Landscape Shelf and Cabinet: the handheld's shelf filling the page's height. The selected game
    /// stands out with its front; its neighbours are spines. Drag along it with momentum.
    fn paint_shelf_landscape(&mut self, c: &mut Canvas, text: &mut Text, room: Room, right: i32, cover: Option<&Image>) {
        let cabinet = self.prefs.theme == Theme::Cabinet;
        let (light, earned) = if matches!(self.prefs.theme, Theme::Shelf | Theme::Cabinet) {
            self.room_sync()
        } else {
            (pocket_ui::room_light::Light::NEUTRAL, None)
        };
        let shade = light.case_shade().min(255);
        let margin = room.margin;
        let scene_h = room.bottom - room.top - self.d(28.0);
        let base = room.top + self.d(8.0) + scene_h;
        let width = right - margin;
        if cabinet {
            let key = (width, scene_h, 9usize);
            if self.shelves.iter().all(|(k, _)| *k != key) {
                self.shelves.clear();
                let mut pic = art::compartment(width + self.d(16.0), scene_h + self.d(4.0), 0);
                light.tint_all(&mut pic.px);
                self.shelves.push((key, pic));
            }
            if let Some((_, pic)) = self.shelves.iter().find(|(k, _)| *k == key) {
                art::blit(c, pic, margin - self.d(8.0), base - scene_h - self.d(4.0), 255, (room.top, room.bottom));
            }
        } else {
            c.rect(margin - self.d(6.0), base, width + self.d(12.0), self.d(5.0), light.tint(pal().spine));
            c.rect(margin - self.d(6.0), base + self.d(5.0), width + self.d(12.0), self.d(5.0), light.tint(pal().dust));
        }
        let case_h = scene_h - self.d(cabinet as u8 as f32 * 14.0) - self.d(10.0);
        let gap = self.d(4.0);
        // Neighbours first, to find how many fit left of the selected game (a third of the shelf).
        // With several shelves, only the selection's.
        let (lo, lib_len) = self.shelf_row().filter(|_| self.screen == Screen::Library).map_or((0, self.visible.len()), |r| (r.start, r.end));
        let mut pics: Vec<(usize, art::Pic)> = Vec::new();
        let mut used = 0;
        let mut start = self.selected;
        // The selected game's open case always fits: neighbours only take what it leaves (on a Mini V2's 1240x1080 a
        // third of the shelf plus the case was wider than the shelf, and the selected game was left out).
        let selected_w = if self.selected < lib_len { self.shelf_pic(text, self.selected, true, case_h, cover).w } else { 0 };
        let left_room = (width / 3).min(width - self.d(8.0) - selected_w - gap);
        while start > lo {
            let pic = self.shelf_pic(text, start - 1, false, case_h, cover);
            if used + pic.w + gap > left_room {
                break;
            }
            used += pic.w + gap;
            start -= 1;
            pics.insert(0, (start, pic));
        }
        let mut x = margin + self.d(8.0);
        let mut pos = start;
        let mut targets = Vec::new();
        let mut idx = 0;
        let mut pitch_sum = 0;
        let mut pitch_n = 0;
        while pos < lib_len {
            let selected = pos == self.selected;
            let pic = if idx < pics.len() { pics[idx].1.clone() } else { self.shelf_pic(text, pos, selected, case_h, cover) };
            idx += 1;
            if x + pic.w > right {
                break;
            }
            let up = if selected { self.d(10.0) } else { 0 };
            let leaving = selected && self.launching;
            if leaving {
                self.front_at = Some(((x, base - pic.h - up, pic.w, pic.h), art::lit(&pic, shade)));
            } else {
                art::blit(c, &pic, x, base - pic.h - up, shade, (room.top, room.bottom));
            }
            if selected && (!leaving || self.returning) && self.controller_hints() {
                c.frame(x - self.d(3.0), base - pic.h - up - self.d(3.0), pic.w + self.d(6.0), pic.h + self.d(6.0), self.d(2.0), pal().chalk);
            }
            if !selected {
                pitch_sum += pic.w + gap;
                pitch_n += 1;
            }
            let name = self.library.as_ref().map(|l| l.games[self.visible[pos]].name.clone()).unwrap_or_default();
            targets.push((Area::new(x - gap / 2, room.top, pic.w + gap, room.bottom - room.top), name, pos));
            x += pic.w + gap;
            pos += 1;
        }
        // After the last case, what the player earned, where it fits whole.
        if let (true, Some(earned)) = (pos >= lib_len, &earned) {
            self.paint_deco_props(text, c, (x + self.d(8.0), right), base, case_h, shade, (room.top, room.bottom), earned);
        }
        self.step = ((pitch_sum / pitch_n.max(1)).max(self.d(24.0)) as f32, 0.0);
        self.scroll_max = 0.0;
        for (area, name, pos) in targets {
            self.name_target(area, &name, Action::Row(pos));
        }
    }
    /// What the player earned, standing on the board at `base` from `from.0` to `from.1`: the plant at its stage, the
    /// trophy and the framed "100 %" when earned, a figurine for each system played long. About half a case high;
    /// whatever does not fit whole is left out (`room_deco::place`).
    #[allow(clippy::too_many_arguments)]
    fn paint_deco_props(&mut self, text: &mut Text, c: &mut Canvas, (from, to): (i32, i32), base: i32, case_h: i32, shade: u32, clip: (i32, i32), earned: &pocket_ui::room_deco::Earned) {
        use pocket_ui::media::{self, Prop};
        let f = media::SHELF_SCALE * (case_h as f32 * 0.5) / Prop::PlantBig.pic().h.max(1) as f32;
        let mut props = vec![Prop::Plant];
        if earned.trophy {
            props.push(Prop::Trophy);
        }
        if earned.hundred {
            props.push(Prop::Hundred);
        }
        props.extend(earned.figures.iter().map(|s| Prop::Figure(pocket_ui::room_deco::Figure::of(s))));
        let pics: Vec<_> = props.into_iter().map(|p| pocket_ui::room_props::prop_pic(&mut self.room.pics, text, p, earned, f)).collect();
        let widths: Vec<i32> = pics.iter().map(|p| p.w).collect();
        for (i, x) in pocket_ui::room_deco::place(from, to, self.d(14.0), &widths) {
            art::blit(c, &pics[i], x, base - pics[i].h, shade, clip);
        }
    }
    /// A game's spine (or front, selected) at the shelf's case height.
    fn shelf_pic(&mut self, text: &mut Text, pos: usize, selected: bool, case_h: i32, cover: Option<&Image>) -> art::Pic {
        let index = self.visible[pos];
        if selected {
            return self.front_pic(text, index, selected, (case_h, case_h), cover).unwrap_or_else(|| art::Pic::new(1, 1, 0));
        }
        if self.cards.len() > 96 {
            self.cards.clear();
        }
        let key = (index, false);
        let game = &self.library.as_ref().unwrap().games[index];
        let m = Medium::of(game.system_id);
        let k = case_h as f32 / m.size().h as f32;
        let frozen = self.frozen.contains(&game.key);
        let want = case_h + if frozen { media::ajar_peek(k) } else { 0 };
        if let Some(pic) = self.cards.get(&key).filter(|p| p.h == want) {
            return pic.clone();
        }
        let look = art::Look {
            color: game.color,
            band: game.color,
            plastic: art::plastic(game.system_id),
        };
        let mut pic = media::spine(text, m, k, look, &game.name);
        let stickers = self.prefs.stickers_shown(&game.key);
        let (lx, ly, lw, _) = media::spine_label_rect(m, k);
        pocket_ui::case_stickers::on_spine(text, &mut pic, &stickers, (lx, 0, lw, ly), index as u64);
        if pic.h != case_h && pic.h > 0 {
            pic = pic.scaled(pic.w * case_h / pic.h, case_h);
        }
        if frozen {
            pic = media::ajar_spine(text, pic, game.system_id, std::path::Path::new(&game.relative), k, look);
        }
        self.cards.insert(key, pic.clone());
        pic
    }
    /// Nothing to show: a vignette, what is going on, and what to do next as a full-width button.
    fn paint_empty_phone(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        let (picture, heading, line, button): (Art, &str, String, Option<(&str, Action)>) = if self.library.is_none() {
            (
                Art::NoGames,
                if self.scan_running { "Reading library..." } else { "No games yet" },
                if self.message.is_empty() {
                    tr("Choose the folder with your games. Android remembers access; your game files stay where they are.")
                } else {
                    tr(&self.message)
                },
                (!self.scan_running).then_some(("Choose your ROM folder", Action::Folder)),
            )
        } else {
            match self.screen {
                Screen::Favorites => (
                    Art::NoFavorites,
                    "No favourites yet",
                    tr(if self.controller_hints() { "Press Y on a game to add it." } else { "Open a game's card and tap Favourite." }),
                    None,
                ),
                Screen::Results => (Art::Search, "No games match", tr("Clear a filter in Curator."), Some(("Curator", Action::Tab(Screen::Curator)))),
                Screen::Connection => (
                    Art::Search,
                    "Nothing to rediscover yet",
                    tr("Games you haven't played for a year, or played a year ago today, show up here."),
                    Some(("Curator", Action::Tab(Screen::Curator))),
                ),
                _ => (
                    Art::NoGames,
                    "No games found",
                    tr("Choose the folder that contains your system folders, or a system folder such as psp or gba."),
                    Some(("Choose your ROM folder", Action::Folder)),
                ),
            }
        };
        self.empty_page(c, text, room, picture, heading, &line, button);
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn empty_page(&mut self, c: &mut Canvas, text: &mut Text, room: Room, picture: Art, heading: &str, line: &str, button: Option<(&str, Action)>) {
        let margin = room.margin;
        let room_h = room.bottom - room.top;
        // Landscape: the vignette at the left, the words and the button beside it, both in the middle of the page.
        // On a phone held across (under 240 dp of room) the words take the whole width: the vignette would leave them four lines too many.
        let side = c.w > c.h && room_h >= self.d(240.0) && pocket_ui::ui_art::get(picture).is_some();
        let (tx, w) = if side {
            let art_w = (c.w - 2 * margin) * 2 / 5;
            if let Some(sprite) = pocket_ui::ui_art::get(picture) {
                let fitted = sprite.fitted(art_w.min(self.d(260.0)), (room_h - self.d(24.0)).min(self.d(200.0)));
                c.blit_sprite(&fitted, margin + (art_w - fitted.w as i32) / 2, room.top + (room_h - fitted.h as i32) / 2, 0);
            }
            (margin + art_w + self.d(24.0), c.w - 2 * margin - art_w - self.d(24.0))
        } else {
            (margin, c.w - 2 * margin)
        };
        let button_h = self.d(52.0);
        // The words say what to do next: never cut them short (a small landscape screen needs four or five lines). A page
        // that does not fit in the room (a 720p phone held across, German at larger text) goes a step or two smaller, which
        // reads better than words over the button.
        let mut chosen = None;
        for (body, head, pitch) in [(16.0, 24.0, 24.0), (14.0, 20.0, 20.0), (13.0, 18.0, 18.0)] {
            let (bs, hs) = (self.sp(body), self.sp(head));
            let lines = text.wrap_at_most(Face::Body, bs, line, w, 6);
            // The heading on two lines rather than cut ("No se encontraron juegos" in large type).
            let heading_lines = text.wrap_at_most(Face::Title, hs, heading, w, 2);
            let heading_px = pocket_ui::type_scale::px(hs) as i32;
            let text_h = heading_lines.len() as i32 * (heading_px + self.d(6.0)) + self.d(4.0) + lines.len() as i32 * self.d(pitch) + if button.is_some() { button_h + self.d(28.0) } else { 0 };
            chosen = Some((bs, hs, self.d(pitch), lines, heading_lines, heading_px, text_h));
            if text_h + self.d(12.0) <= room_h {
                break;
            }
        }
        let (bs, hs, pitch, lines, heading_lines, heading_px, text_h) = chosen.unwrap();
        let art_h = if side { 0 } else { (room_h - text_h - self.d(48.0)).min(self.d(200.0)) };
        let block = text_h + if art_h > self.d(64.0) { art_h + self.d(16.0) } else { 0 };
        let mut y = if side { (room.top + (room_h - block) / 2).max(room.top + self.d(4.0)) } else { room.top + self.d(16.0) };
        if !side && art_h > self.d(64.0) {
            if let Some(sprite) = pocket_ui::ui_art::get(picture) {
                let fitted = sprite.fitted(w.min(self.d(260.0)), art_h);
                c.blit_sprite(&fitted, tx + (w - fitted.w as i32) / 2, y + (art_h - fitted.h as i32) / 2, 0);
                y += art_h + self.d(16.0);
            }
        }
        let heading_top = y;
        for (k, shown) in heading_lines.iter().enumerate() {
            let hw = text.width(Face::Title, hs, shown);
            y += if k == 0 { hs as i32 } else { heading_px + self.d(6.0) };
            text.draw(c, Face::Title, hs, tx + (w - hw) / 2, y, pal().chalk, shown);
        }
        self.target(Area::new(tx, heading_top, w, y - heading_top + self.d(8.0)), heading, Action::Label);
        y += self.d(10.0);
        let line_top = y;
        for l in &lines {
            y += pitch;
            let lw = text.width(Face::Body, bs, l);
            text.draw(c, Face::Body, bs, tx + (w - lw) / 2, y, pal().dust, l);
        }
        // The words are read out too, not only the heading.
        if !lines.is_empty() {
            self.target(Area::new(tx, line_top, w, y - line_top + self.d(4.0)), line, Action::Label);
        }
        if let Some((label, action)) = button {
            y += self.d(28.0);
            self.pill(c, text, Area::new(tx, y.min(room.bottom - button_h - self.d(8.0)), w, button_h), label, true, action);
        }
    }

    /// Search: the field at the top (tap to type), the matches as rows, the keyboard over the bottom.
    fn paint_search_phone(&mut self, c: &mut Canvas, text: &mut Text, room: Room, _cover: Option<&Image>) {
        let margin = room.margin;
        let field_h = self.d(52.0);
        let field = Area::new(margin, room.top + self.d(4.0), c.w - 2 * margin, field_h);
        c.round_rect(field.x, field.y, field.w, field.h, field.h / 2, pal().pill);
        let k = (self.d(20.0) / 16).max(1);
        pocket_ui::ui_art::paint_tinted(c, Art::Icon(Icon::Search), field.x + self.d(16.0), field.y + (field.h - 16 * k) / 2, k, pal().dust);
        let size = self.sp(17.0);
        let tx = field.x + self.d(24.0) + 16 * k;
        let (shown, color) = if self.query.is_empty() {
            (tr("Search games"), pal().dust)
        } else {
            (text.fit(Face::Body, size, &self.query, field.w - (tx - field.x) - self.d(20.0)), pal().chalk)
        };
        let end = text.draw(c, Face::Body, size, tx, field.y + field.h / 2 + type_mid(size), color, &shown);
        if self.keyboard.is_some() {
            let cx = if self.query.is_empty() { tx - self.d(4.0) } else { tx + end + self.d(2.0) };
            c.rect(cx, field.y + self.d(14.0), self.d(2.0), field.h - self.d(28.0), pal().chalk);
        }
        self.target(field, "Search games", Action::Keyboard);
        let below = Room { top: field.y + field.h + self.d(12.0), ..room };
        let bottom = if self.keyboard.is_some() { c.h - self.keyboard_height(c) } else { room.bottom };
        let below = Room { bottom, ..below };
        // The keyboard covers the page: one line above it, never an empty gap while typing.
        let typing_line = |me: &mut Self, c: &mut Canvas, text: &mut Text, line: String| {
            // A German sentence at larger text takes a step smaller type and a second line, not a cut word.
            let width = c.w - 2 * room.margin;
            let size = [me.sp(16.0), me.sp(14.0)].into_iter().find(|s| text.width(Face::Body, *s, &line) <= width).unwrap_or(me.sp(14.0));
            let step = size as i32 + me.d(6.0);
            let lines = text.wrap_at_most(Face::Body, size, &line, width, 2);
            let mut shown = 0;
            for (i, l) in lines.iter().enumerate() {
                let y = below.top + me.d(8.0) + size as i32 + i as i32 * step;
                if y < below.bottom {
                    text.draw(c, Face::Body, size, room.margin, y, pal().dust, l);
                    shown += 1;
                }
            }
            if shown > 0 {
                me.target(Area::new(room.margin, below.top, width, shown * step + me.d(16.0)), &line, Action::Label);
            }
        };
        if self.query.is_empty() {
            // Nothing typed yet: what to do, not "No games found".
            if self.keyboard.is_none() {
                self.empty_page(c, text, below, Art::Search, "Find a game", &tr("Type a few letters of its name."), None);
            } else {
                typing_line(self, c, text, tr("Type a few letters of its name."));
            }
        } else if self.visible.is_empty() {
            if self.keyboard.is_none() {
                self.empty_page(c, text, below, Art::Search, "No games found", &tr("Try fewer letters."), None);
            } else {
                typing_line(self, c, text, format!("{} · {}", tr("No games found"), tr("Try fewer letters.")));
            }
        } else if below.bottom - below.top >= self.d(64.0) {
            self.paint_list_phone(c, text, below, c.w);
        } else {
            // No room for the list above the keyboard (a small landscape screen): the matches as one line, so typing
            // shows that it found something; Done shows the list.
            let names: Vec<&str> = self.library.as_ref().map_or(vec![], |lib| self.visible.iter().take(3).map(|&i| lib.games[i].name.as_str()).collect());
            let line = format!("{} · {}", games_note(self.visible.len()), names.join(", "));
            typing_line(self, c, text, line);
        }
    }
    pub(super) fn keyboard_height(&self, c: &Canvas) -> i32 {
        let rows = self.keyboard.as_ref().map_or(0, |k| k.rows().len()) as i32;
        let key_h = self.d(if c.w > c.h { 40.0 } else { 52.0 });
        // The keys give way before the search field does: a 720p phone held across has 304 dp, of which the header and the
        // field keep 118 (the keys then stand 34 dp high, never under 28).
        (rows * key_h + self.d(16.0)).min(c.h - self.d(118.0)).max(rows * self.d(28.0) + self.d(16.0))
    }
    /// The search's own keyboard, as large as a phone's: 52 dp keys across the width.
    fn paint_keyboard_phone(&mut self, c: &mut Canvas, text: &mut Text) {
        let Some(k) = &self.keyboard else { return };
        let rows = k.rows();
        let selected = (k.row, k.col);
        let height = self.keyboard_height(c);
        let top = c.h - height;
        c.rect(0, top, c.w, height, pal().spine);
        let key_h = (height - self.d(16.0)) / rows.len() as i32;
        let margin = self.d(4.0);
        let unit = (c.w - 2 * margin) / rows.iter().map(|r| r.len()).max().unwrap_or(10) as i32;
        let size = self.sp(18.0);
        let hints = self.controller_hints();
        for (r, keys) in rows.iter().enumerate() {
            let specials = keys.iter().all(|k| matches!(k, Key::Special(_)));
            let kw = if specials { (c.w - 2 * margin) / keys.len() as i32 } else { unit };
            let x0 = margin + (c.w - 2 * margin - kw * keys.len() as i32) / 2;
            for (j, key) in keys.iter().enumerate() {
                let label = match key {
                    Key::Char(ch) => ch.to_string(),
                    Key::Special(s) => tr(&format!("{s:?}")),
                };
                let area = Area::new(x0 + j as i32 * kw, top + self.d(8.0) + r as i32 * key_h, kw, key_h);
                let inner = Area::new(area.x + self.d(2.0), area.y + self.d(3.0), area.w - self.d(4.0), area.h - self.d(6.0));
                let on = hints && selected == (r, j);
                let done = matches!(key, Key::Special(pocket_ui::keyboard::Special::Done));
                let bg = if on || done { pal().label } else { pal().pill };
                c.round_rect(inner.x, inner.y, inner.w, inner.h, self.d(6.0), bg);
                let mut s = if label.chars().count() > 1 { self.sp(14.0) } else { size };
                // A long key name (German "Leerzeichen") gets smaller, down to a floor, before it would be cut.
                while s > self.sp(9.0) && text.width(Face::Body, s, &label) > inner.w - self.d(8.0) { s -= 1; }
                let shown = text.fit(Face::Body, s, &label, inner.w - self.d(8.0));
                let tw = text.width(Face::Body, s, &shown);
                text.draw(c, Face::Body, s, inner.x + (inner.w - tw) / 2, inner.y + inner.h / 2 + type_mid(s), if on || done { pal().ink } else { pal().chalk }, &shown);
                self.region(area, label, Action::Key(r, j));
            }
        }
    }

    /// The game's card: the shared box (or info page) above, and touch buttons at the bottom: Play across the
    /// width, then Back, Favourite and Turn over.
    fn paint_card_phone(&mut self, c: &mut Canvas, text: &mut Text, cover: Option<&Image>) {
        let landscape = c.w > c.h;
        let margin = self.margin(c);
        let bh = self.d(TARGET);
        let gap = self.d(8.0);
        let hints = self.controller_hints();
        let hint_h = if hints { self.d(40.0) } else { 0 };
        let favourite = self.game().is_some_and(|g| self.prefs.favorites.contains(&g.key));
        let boxed = self.prefs.theme != Theme::Split;
        let mut secondary: Vec<(&str, Action)> = vec![("Back", Action::Press(Button::South)), ("Favourite", Action::Press(Button::West))];
        if boxed {
            secondary.push(("Turn over", Action::Press(Button::North)));
        }
        secondary.push(("Stickers", Action::Press(Button::Select)));
        if self.game().is_some_and(|g| super::super::bundled_core(g.system_id).is_some() && self.plays_in_app(g).is_none()) {
            secondary.push(("Controls", Action::Controls));
        }
        // A favourite's button is amber, the colour of Favourites on the shelf.
        let colors = |label: &str| {
            if label == "Favourite" && favourite {
                (pal().amber, pal().ink)
            } else {
                (pal().pill, pal().chalk)
            }
        };
        if landscape {
            // The box (or the info page) takes the page's height at the left; the name and the buttons stand in a
            // column at the right, Play at the bottom where the thumb is.
            let col_w = (c.w * 30 / 100).clamp(self.d(200.0), self.d(300.0));
            let col_x = c.w - margin - col_w;
            let (bw, bh_card) = (col_x - self.d(8.0), c.h - hint_h);
            self.card_buf.resize((bw.max(1) * bh_card.max(1)) as usize, 0);
            let mut buf = std::mem::take(&mut self.card_buf);
            {
                let mut card = Canvas::new(&mut buf, bw.max(1) as u32, bh_card.max(1) as u32);
                card.clear(pal().void);
                self.card_origin = (0, 0);
                self.paint_box(&mut card, text, cover);
            }
            for y in 0..bh_card.min(c.h) {
                let (src, dst) = ((y * bw) as usize, (y * c.w) as usize);
                c.buf[dst..dst + bw as usize].copy_from_slice(&buf[src..src + bw as usize]);
            }
            self.card_buf = buf;
            self.link_targets((0, 0));
            // The info page has the name at its top already; the box shows only its cover.
            let mut header_bottom = 0;
            if let Some(game) = self.game().cloned().filter(|_| boxed) {
                let ts = self.sp(22.0);
                let mut y = self.d(16.0);
                for line in text.literal(|text| text.wrap_at_most(Face::Title, ts, &game.name, col_w, 2)) {
                    y += pocket_ui::type_scale::px(ts) as i32;
                    text.draw_name(c, Face::Title, ts, col_x, y, pal().chalk, &line);
                    y += self.d(4.0);
                }
                y += pocket_ui::type_scale::px(self.sp(14.0)) as i32 + self.d(8.0);
                let line = self.system_line(&game);
                let system = text.fit_name(Face::Body, self.sp(14.0), &line, col_w);
                text.draw_name(c, Face::Body, self.sp(14.0), col_x, y, pal().dust, &system);
                self.name_target(Area::new(col_x, 0, col_w, y + self.d(8.0)), &game.name, Action::Label);
                header_bottom = y + self.d(8.0);
            }
            // Two to a line, so a long name above them keeps its room; on a phone held across (304 dp high with its
            // gesture bar) a name of two lines leaves too little for three rows of buttons and Play: they stand a
            // little closer and lower rather than over the name.
            let lines = secondary.len().div_ceil(2) as i32;
            let (mut bh, mut gap, mut play_h) = (bh, gap, self.d(52.0));
            if header_bottom + lines * (bh + gap) + play_h + self.d(12.0) > c.h - hint_h {
                (bh, gap, play_h) = (self.d(40.0), self.d(6.0), self.d(44.0));
            }
            let mut y = c.h - hint_h - self.d(12.0) - play_h;
            self.play_pill(c, text, Area::new(col_x, y, col_w, play_h));
            let half = (col_w - gap) / 2;
            y -= lines * (bh + gap);
            for (i, (label, action)) in secondary.into_iter().enumerate() {
                let x = col_x + (i as i32 % 2) * (half + gap);
                self.pill_in(c, text, Area::new(x, y + (i as i32 / 2) * (bh + gap), half, bh), label, colors(label), action);
            }
        } else {
            // Back, Favourite, Turn over, Stickers and Controls in one row are 58 dp each on a 360 dp phone: "Favo…",
            // "Stick…". When the words do not fit their pills at a readable size they stand in two rows of up to three.
            let w = c.w - 2 * margin;
            let one_row = (w - gap * (secondary.len() as i32 - 1)) / secondary.len() as i32;
            let size = self.sp(14.0);
            let words_fit = secondary.iter().all(|(label, _)| text.width(Face::Title, size, label) <= one_row - self.d(24.0));
            let rows = if words_fit { 1 } else { secondary.len().div_ceil(3) as i32 };
            let bar = (rows + 1) * bh + (rows + 2) * gap;
            let top = c.h - bar - hint_h;
            // The game's name and system above its box, where the phone has the room.
            let head = if boxed {
                let game = self.game().cloned();
                let mut y = self.d(28.0);
                if let Some(game) = game {
                    let ts = self.sp(26.0);
                    for line in text.literal(|text| text.wrap_at_most(Face::Title, ts, &game.name, c.w - 2 * margin, 2)) {
                        y += pocket_ui::type_scale::px(ts) as i32;
                        text.draw_name(c, Face::Title, ts, margin, y, pal().chalk, &line);
                        y += self.d(4.0);
                    }
                    y += pocket_ui::type_scale::px(self.sp(15.0)) as i32 + self.d(8.0);
                    let line = self.system_line(&game);
                    let line = text.fit_name(Face::Body, self.sp(15.0), &line, c.w - 2 * margin);
                    text.draw_name(c, Face::Body, self.sp(15.0), margin, y, pal().dust, &line);
                    self.name_target(Area::new(margin, 0, c.w - 2 * margin, y + self.d(8.0)), &game.name, Action::Label);
                }
                y + self.d(8.0)
            } else {
                0
            };
            // The front of a wide case (a SNES box beside its cartridge) needs little height: the box goes straight
            // under the name and the game's facts fill the room under it. The back keeps the whole height.
            let full = top + 96 - head;
            let mut box_h = full;
            let mut facts_at = None;
            if boxed && !self.box_view.back {
                if let Some(m) = self.game().map(|g| Medium::of(g.system_id)) {
                    let case_h = pocket_ui::game_box::case_height(c.w, full, m);
                    let mut h = case_h + 120;
                    while h < full && pocket_ui::game_box::front_height(c.w, h) < case_h {
                        h += 4;
                    }
                    if h + self.d(120.0) < full {
                        box_h = h;
                        facts_at = Some(head + h - 96 + self.d(12.0));
                    }
                }
            }
            {
                // The shared painter keeps a hint strip of its own at the bottom; it goes under the buttons.
                let mut card = c.rows(head, head + box_h);
                self.card_origin = (0, head);
                self.paint_box(&mut card, text, cover);
            }
            self.link_targets((0, head));
            if let Some(y) = facts_at {
                c.rect(0, y - self.d(8.0), c.w, top - y + self.d(8.0), pal().void);
                self.card_facts(c, text, (margin, y, c.w - 2 * margin, top - self.d(12.0)));
            }
            c.rect(0, top, c.w, c.h - top, pal().void);
            self.play_pill(c, text, Area::new(margin, top + gap, w, bh));
            // The pills share the rows evenly (5 as 3 and 2), each row across the whole width.
            let n = secondary.len();
            let per_row = n.div_ceil(rows as usize);
            let mut labels = secondary.into_iter();
            for row in 0..rows as usize {
                let count = per_row.min(n - row * per_row);
                let sw = (w - gap * (count as i32 - 1)) / count as i32;
                let y = top + (row as i32 + 2) * gap + (row as i32 + 1) * bh;
                for i in 0..count {
                    let (label, action) = labels.next().unwrap();
                    self.pill_in(c, text, Area::new(margin + i as i32 * (sw + gap), y, sw, bh), label, colors(label), action);
                }
            }
        }
        if hints {
            // What only a key reaches first: a narrow phone leaves out what doesn't fit, and A and B say themselves.
            let mut keys = vec![("SELECT", "Stickers"), ("Y", "Favourite"), ("A", self.play_word()), ("B", "Back")];
            if self.game().is_some_and(|g| super::super::bundled_core(g.system_id).is_some()) {keys.insert(0,("R2","Controls"));}
            if boxed {
                keys.insert(1, ("X", "Turn over"));
            }
            self.hint_row(c, text, c.h, &keys);
        }
    }
}

impl Ui {
    /// What is known about the selected game, under its box on a portrait phone: year and players, the offline
    /// list's facts (developer, publisher, genre, series), then the description, as far as `bottom`.
    fn card_facts(&mut self, c: &mut Canvas, text: &mut Text, (x, mut y, w, bottom): (i32, i32, i32, i32)) {
        let Some(card) = self.visible.get(self.selected).and_then(|&i| self.library.as_ref().map(|l| l.cards[i].clone())) else {
            return;
        };
        let size = self.sp(15.0);
        let line = size as i32 + self.d(9.0);
        // (key, value, the link it leads by): a fact that leads to the player's other games has an arrow and is a button.
        let mut rows: Vec<(String, String, Option<usize>)> = Vec::new();
        if let Some(year) = card.year {
            rows.push((tr("Year"), year.to_string(), None));
        }
        if let Some(players) = &card.players {
            rows.push((tr("Players"), players.clone(), None));
        }
        for (i, (label, value)) in card.facts.iter().enumerate() {
            rows.push((tr(label), value.clone(), card.links.iter().position(|(f, _)| *f == i)));
        }
        // The names of the facts are ours, so they stand whole ("Herausgeber", "Desarrollador" at larger text); the room
        // they take is the widest of them, between 96 dp and 45 % of the card.
        let key_w = rows.iter().map(|(label, _, _)| text.width(Face::Body, size, label) + self.d(12.0)).max().unwrap_or(0).max(self.d(96.0)).min(w * 45 / 100);
        if rows.is_empty() && !card.matched {
            for l in text.wrap_at_most(Face::Body, size, &tr("Not in the offline game list, so there are no facts to show."), w, 4) {
                y += line;
                text.draw(c, Face::Body, size, x, y, pal().dust, &l);
            }
        }
        for (label, value, link) in rows {
            if y + line > bottom {
                break;
            }
            y += line;
            let label = text.fit(Face::Body, size, &label, key_w - self.d(8.0));
            text.draw(c, Face::Body, size, x, y, pal().dust, &label);
            if let Some(l) = link {
                // The arrow after the value; the whole line is the button.
                let arrow = self.d(14.0);
                let value_shown = text.fit(Face::Body, size, &value, w - key_w - arrow - self.d(8.0));
                let vw = text.draw(c, Face::Body, size, x + key_w, y, pal().chalk, &value_shown);
                let (ax, ay) = (x + key_w + vw + self.d(8.0), y - type_mid(size));
                for dy in -(arrow / 3)..=(arrow / 3) {
                    c.rect(ax, ay + dy, (arrow / 2 - dy.abs() * 3 / 2).max(1), 1, pal().dust);
                }
                self.target(Area::new(x, y - line + self.d(6.0), w, line), &format!("{label}: {value}"), Action::Link(l));
            } else {
                let value = text.fit(Face::Body, size, &value, w - key_w);
                text.draw(c, Face::Body, size, x + key_w, y, pal().chalk, &value);
            }
        }
        let about = match &card.summary {
            pocket_ui::game_card_model::Summary::Wikipedia { text, .. } => Some(text.clone()),
            _ => None,
        };
        if let Some(about) = about {
            y += self.d(10.0);
            let room = ((bottom - y) / line).max(0) as usize;
            for l in text.wrap_at_most(Face::Body, size, &about, w, room) {
                y += line;
                text.draw(c, Face::Body, size, x, y, pal().text, &l);
            }
        }
    }
}

/// "12 games", nothing for none: an empty page says so itself.
fn games_note(n: usize) -> String {
    match n {
        0 => String::new(),
        1 => tr("1 game"),
        n => tr(&format!("{n} games")),
    }
}
/// The middle of a line of `size` px to its baseline.
pub(super) fn type_mid(size: u32) -> i32 {
    type_scale::mid(size)
}
/// How far a frozen game's front shows what is inside.
const AJAR_MOST_DP: f32 = 10.0;
/// The case `front` with `inside` behind it, out of its side by `out` of `peek` (0 while it is still going back in, after
/// a game). The picture only grows sideways, always by `peek`, so the shelf around it stays where it is.
fn ajar_front(front: art::Pic, inside: &art::Pic, peek: i32, out: i32) -> art::Pic {
    let (gw, gh) = (inside.w, inside.h);
    let mut pic = art::Pic::new(front.w + peek, front.h.max(gh), 0);
    let top = (pic.h - front.h) / 2;
    let (gx, gy) = (front.w + out.clamp(0, peek) - gw, (pic.h - gh) / 2);
    for y in (0..gh).filter(|_| out > 0) {
        for x in 0..gw {
            let (px, at) = (inside.px[(y * gw + x) as usize], (y + gy) * pic.w + x + gx);
            if px >> 24 != 0 && gx + x >= 0 {
                pic.px[at as usize] = px;
            }
        }
    }
    for y in 0..front.h {
        for x in 0..front.w {
            let px = front.px[(y * front.w + x) as usize];
            let at = ((y + top) * pic.w + x) as usize;
            // A soft edge keeps what lies behind it only where there is nothing under the front to mix with.
            if px >> 24 >= 128 || pic.px[at] == 0 {
                pic.px[at] = px;
            }
        }
    }
    pic
}
fn c_band(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: u32) {
    if pocket_ui::retro::on() { pocket_ui::retro::plain_bar(c, x, y, w, h, color); }
    else { c.round_rect(x, y, w, h, h / 2, color); }
}
use pocket_ui::type_scale;

pub(super) trait ClipY {
    fn clip_y(self, top: i32, bottom: i32) -> Area;
}
impl ClipY for Area {
    /// The part of a scrolled target that is on the page, between `top` and `bottom`.
    fn clip_y(self, top: i32, bottom: i32) -> Area {
        let y0 = self.y.max(top);
        let y1 = (self.y + self.h).min(bottom);
        Area::new(self.x, y0, self.w, (y1 - y0).max(0))
    }
}
