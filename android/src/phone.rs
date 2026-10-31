//! Touch-first phone presentation: a bottom tab bar, dp-sized type and touch targets, scrolling shelves with
//! momentum, and controller hints only while a controller is the input. The same Ui state, painters and shared
//! media as the controller path; sizes come from the density (`Ui::dp`), not from a second set of features.
use super::*;
use pocket_ui::ui_art::{Art, Icon};

/// Material's minimum touch target.
const TARGET: f32 = 48.0;
/// The tabs, in order: what they open, their label and their icon.
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
        } else if landscape && matches!(self.prefs.theme, Theme::Minimal | Theme::Cards | Theme::Split) {
            // The info page needs 900 px beside its buttons for its two columns (cover left, facts right).
            0.8
        } else {
            0.62
        }
    }
    /// A region that does `action` when tapped; its label is the accessibility name (translated).
    pub(super) fn target(&mut self, area: Area, label: &str, action: Action) {
        self.region(area, label.into(), action);
    }
    /// A pill button: paper with ink for the primary action, the sheet's pill colour otherwise. The label is
    /// centred and never cut while it fits at a slightly smaller size.
    pub(super) fn pill(&mut self, c: &mut Canvas, text: &mut Text, area: Area, label: &str, primary: bool, action: Action) {
        let colors = if primary { (pal().label, pal().ink) } else { (pal().pill, pal().chalk) };
        self.pill_in(c, text, area, label, colors, action);
    }
    pub(super) fn pill_in(&mut self, c: &mut Canvas, text: &mut Text, area: Area, label: &str, (bg, fg): (u32, u32), action: Action) {
        c.round_rect(area.x, area.y, area.w, area.h, area.h / 2, bg);
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
    pub(super) fn header(&mut self, c: &mut Canvas, text: &mut Text, title: &str, note: &str) -> i32 {
        let margin = self.margin(c);
        let h = self.d(56.0);
        let size = self.sp(26.0);
        let base = h / 2 + type_mid(size);
        let note_w = if note.is_empty() { 0 } else { text.width(Face::Body, self.sp(14.0), note) + self.d(12.0) };
        let shown = text.fit(Face::Title, size, title, c.w - 2 * margin - note_w);
        text.draw(c, Face::Title, size, margin, base, pal().chalk, &shown);
        // A title that is also a tab's name is already spoken by the tab (and taps by name find the tab).
        if TABS.iter().all(|(_, label, _)| tr(label) != tr(title)) {
            self.target(Area::new(margin, 0, c.w - 2 * margin - note_w, h), title, Action::Label);
        }
        if !note.is_empty() {
            text.draw_right(c, Face::Body, self.sp(14.0), c.w - margin, base, pal().dust, note);
        }
        h
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
        let current = match self.screen {
            Screen::Results | Screen::Facet => Screen::Curator,
            // A fact's list belongs to the card's tab, Rediscover to Curator.
            Screen::Connection if self.list_from_curator() => Screen::Curator,
            Screen::Connection => Screen::Library,
            Screen::Quick | Screen::Extras => Screen::Settings,
            s => s,
        };
        let margin = if landscape { self.d(24.0) } else { 0 };
        let tw = (c.w - 2 * margin) / TABS.len() as i32;
        let k = (self.d(24.0) / 16).max(1);
        let icon = 16 * k;
        let size = self.sp(if landscape { 14.0 } else { 12.5 });
        for (i, (screen, label, glyph)) in TABS.into_iter().enumerate() {
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
            Screen::Settings | Screen::Extras | Screen::Quick | Screen::Curator | Screen::Facet => vec![("A", "Choose"), ("B", "Back"), ("L1/R1", "Tabs")],
            _ => vec![("A", "Play"), ("X", "Info"), ("Y", "Favourite"), ("L1/R1", "Tabs"), ("START", "Settings")],
        };
        let bottom = if keyboard { bar } else { self.hint_row(c, text, bar, &hints) };
        let (title, note) = match self.screen {
            Screen::Library => ("Games".to_string(), if self.library.is_some() { self.count_note() } else { String::new() }),
            Screen::Favorites => ("Favourites".into(), self.count_note()),
            Screen::Search => ("Search".into(), if self.query.is_empty() { String::new() } else { self.count_note() }),
            Screen::Results => ("Curator".into(), self.count_note()),
            Screen::Connection => (self.connection_title(), self.count_note()),
            Screen::Facet => (tr(self.facet.label()), String::new()),
            Screen::Curator => ("Curator".into(), String::new()),
            Screen::Quick => ("Quick settings".into(), String::new()),
            Screen::Extras => ("Cozy extras".into(), String::new()),
            _ => ("Settings".into(), String::new()),
        };
        let top = self.header(c, text, &title, &note);
        self.grid_cols = 0;
        let room = Room { top, bottom, margin: self.margin(c) };
        match self.screen {
            // Curator has nothing to sort before there are games: say so, with the way to add them.
            Screen::Curator | Screen::Facet if self.library.as_ref().is_none_or(|l| l.games.is_empty()) => self.paint_empty_phone(c, text, room),
            Screen::Settings | Screen::Extras | Screen::Quick | Screen::Curator | Screen::Facet => {
                let rows = self.menu_rows();
                self.paint_menu_phone(c, text, room, rows);
            }
            Screen::Search => self.paint_search_phone(c, text, room, cover),
            _ => self.paint_games_phone(c, text, room, cover),
        }
        if keyboard {
            self.paint_keyboard_phone(c, text);
        }
    }
    fn count_note(&self) -> String {
        // An empty page says so itself; "0 games" over it would only repeat it.
        if self.visible.is_empty() {
            String::new()
        } else if self.visible.len() == 1 {
            tr("1 game")
        } else {
            tr(&format!("{} games", self.visible.len()))
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
                let value_w = if row.value.is_empty() { 0 } else { (text.width(Face::Body, size, &row.value) + self.d(16.0)).min((w - x - margin) / 2) };
                let label = text.fit(Face::Body, size, &row.label, w - x - margin - value_w);
                text.draw(&mut clip, Face::Body, size, x, base, fg, &label);
                if !row.value.is_empty() {
                    let value = text.fit(Face::Body, size, &row.value, value_w - self.d(16.0));
                    text.draw_right(&mut clip, Face::Body, size, w - margin, base, quiet, &value);
                }
                if i + 1 < rows.len() {
                    clip.rect(margin, y + pitch - 1, w - 2 * margin, 1, pal().pill);
                }
                targets.push((Area::new(0, top + y, w, pitch).clip_y(top, room.bottom), if row.value.is_empty() { row.label.clone() } else { format!("{} · {}", row.label, row.value) }, i));
            }
        }
        for (area, label, i) in targets {
            if area.h > 0 {
                self.target(area, &label, Action::Row(i));
            }
        }
        self.scrollbar(c, top, room.bottom - top, rows.len() as i32 * pitch);
    }
    /// A thin bar at the right edge while a page scrolls.
    fn scrollbar(&self, c: &mut Canvas, top: i32, view: i32, content: i32) {
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
        if self.visible.is_empty() {
            self.paint_empty_phone(c, text, room);
            return;
        }
        let landscape = c.w > c.h;
        let list = matches!(self.prefs.theme, Theme::Minimal | Theme::Split);
        if landscape {
            let side = (c.w * 34 / 100).clamp(self.d(220.0), self.d(360.0));
            let area = Room { bottom: room.bottom, ..room };
            let shelf_w = c.w - side - room.margin;
            if list {
                self.paint_list_phone(c, text, area, shelf_w);
            } else {
                self.paint_shelf_landscape(c, text, area, shelf_w, cover);
            }
            self.side_panel(c, text, (shelf_w + self.d(16.0), room.top, side - self.d(16.0), room.bottom - room.top), cover);
        } else {
            let strip = self.d(76.0);
            let area = Room { bottom: room.bottom - strip, ..room };
            if list {
                self.paint_list_phone(c, text, area, c.w);
            } else {
                self.paint_grid(c, text, area, cover);
            }
            self.strip(c, text, room.bottom - strip, strip);
        }
    }
    /// Portrait: the selected game's name and system, Info and the Play button.
    fn strip(&mut self, c: &mut Canvas, text: &mut Text, y: i32, h: i32) {
        let Some(game) = self.game().cloned() else { return };
        let margin = self.margin(c);
        c.rect(0, y, c.w, h, pal().void);
        c.rect(0, y, c.w, 1, pal().pill);
        let bh = self.d(TARGET);
        let by = y + (h - bh) / 2;
        let play_w = self.d(104.0);
        let info_w = self.d(72.0);
        let play = Area::new(c.w - margin - play_w, by, play_w, bh);
        let info = Area::new(play.x - self.d(8.0) - info_w, by, info_w, bh);
        let name_w = info.x - margin - self.d(12.0);
        let name = text.fit(Face::Title, self.sp(20.0), &game.name, name_w);
        text.draw(c, Face::Title, self.sp(20.0), margin, y + h / 2 - self.d(2.0), pal().chalk, &name);
        let system = text.fit(Face::Body, self.sp(14.0), game.system, name_w);
        text.draw(c, Face::Body, self.sp(14.0), margin, y + h / 2 + self.d(20.0), pal().dust, &system);
        self.target(Area::new(margin, y, name_w, h), &game.name, Action::Press(Button::North));
        self.pill(c, text, info, "Info", false, Action::Press(Button::North));
        self.pill(c, text, play, "Play", true, Action::Press(Button::East));
    }
    /// Landscape: the selected game's picture (or the Cabinet's TV), its name and system, Info and Play.
    fn side_panel(&mut self, c: &mut Canvas, text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), cover: Option<&Image>) {
        let Some(game) = self.game().cloned() else { return };
        let bh = self.d(TARGET);
        let buttons_y = y + h - bh - self.d(8.0);
        let gap = self.d(8.0);
        let info_w = (w - gap) * 2 / 5;
        self.pill(c, text, Area::new(x, buttons_y, info_w, bh), "Info", false, Action::Press(Button::North));
        self.pill(c, text, Area::new(x + info_w + gap, buttons_y, w - info_w - gap, bh), "Play", true, Action::Press(Button::East));
        let system_base = buttons_y - self.d(14.0);
        let system = text.fit(Face::Body, self.sp(14.0), game.system, w);
        text.draw(c, Face::Body, self.sp(14.0), x, system_base, pal().dust, &system);
        let size = self.sp(22.0);
        let lines = text.wrap_at_most(Face::Title, size, &game.name, w, 2);
        let step = size as i32 + self.d(2.0);
        let mut base = system_base - self.d(22.0) - (lines.len() as i32 - 1) * step;
        let name_top = base - size as i32;
        for line in &lines {
            text.draw(c, Face::Title, size, x, base, pal().chalk, line);
            base += step;
        }
        self.target(Area::new(x, name_top, w, system_base - name_top), &game.name, Action::Press(Button::North));
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
        if let Some(pic) = self.cards.get(&key).filter(|p| p.w <= w && p.h <= h && (p.w + 2 >= w || p.h + 2 >= h)) {
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
        let mut pic = media::front(text, m, k, look, &game.name, if selected { cover } else { None }, console, game.system);
        let mut stickers = pocket_ui::case_stickers::CaseStickers::default();
        stickers.set(pocket_ui::case_stickers::Kind::Favourite, self.prefs.extras.case_stickers && self.prefs.favorites.contains(&game.key), 2026);
        pocket_ui::case_stickers::on_front(text, &mut pic, &stickers, media::label_rect(m, k), index as u64);
        if pic.w > w || pic.h > h {
            let s = (w as f32 / pic.w as f32).min(h as f32 / pic.h as f32);
            pic = pic.scaled((pic.w as f32 * s) as i32, (pic.h as f32 * s) as i32);
        }
        self.cards.insert(key, pic.clone());
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
        let cabinet = self.prefs.theme == Theme::Cabinet;
        let mut targets = Vec::new();
        let first = (offset / row_h).max(0) as usize;
        let last = ((offset + view) / row_h + 1) as usize;
        let mut clip = c.rows(room.top, room.bottom);
        for r in first..=last.min(total.max(1) as usize - 1) {
            let y = r as i32 * row_h - offset + self.d(4.0);
            let base = y + lift + case_h;
            if cabinet {
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
                    art::blit(&mut clip, pic, margin - self.d(8.0), base - inner, 255, (0, i32::MAX));
                }
            } else {
                clip.rect(margin - self.d(6.0), base, clip.w - 2 * margin + self.d(12.0), board / 2, light.tint(pal().spine));
                clip.rect(margin - self.d(6.0), base + board / 2, clip.w - 2 * margin + self.d(12.0), board / 2, light.tint(pal().dust));
            }
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
                let x = margin + col as i32 * (cell_w + gap) + (cell_w - pic.w) / 2;
                let up = if selected { lift } else { 0 };
                let py = base - pic.h - up;
                if selected {
                    // The lifted case's shadow on the board.
                    clip.shade_rect(x + self.d(4.0), base - self.d(3.0), pic.w - self.d(8.0), self.d(3.0), 0, 120);
                }
                art::blit(&mut clip, &pic, x, py, shade, (0, i32::MAX));
                if selected && self.controller_hints() {
                    clip.frame(x - self.d(3.0), py - self.d(3.0), pic.w + self.d(6.0), pic.h + self.d(6.0), self.d(2.0), pal().chalk);
                }
                let name = self.library.as_ref().map(|l| l.games[index].name.clone()).unwrap_or_default();
                targets.push((Area::new(margin + col as i32 * (cell_w + gap), room.top + y, cell_w + gap, row_h).clip_y(room.top, room.bottom), name, pos));
            }
        }
        for (area, name, pos) in targets {
            if area.h > self.d(12.0) {
                self.target(area, &name, Action::Row(pos));
            }
        }
        self.scrollbar(c, room.top, view, total * row_h);
    }
    /// The list looks (Minimal, Split): a row per game with its system's stripe at the left edge, 64 dp high.
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
                let star_w = if favourite { 16 * k + self.d(12.0) } else { 0 };
                if favourite {
                    pocket_ui::ui_art::paint_tinted(&mut clip, Art::Icon(Icon::Favourites), x + w - self.d(12.0) - 16 * k, y + (h - 16 * k) / 2, k, if selected { pal().ink } else { pal().amber });
                }
                let name = text.fit(Face::Title, self.sp(18.0), &game.name, w - self.d(32.0) - star_w);
                text.draw(&mut clip, Face::Title, self.sp(18.0), x + self.d(18.0), y + h / 2 - self.d(2.0), fg, &name);
                text.draw(&mut clip, Face::Body, self.sp(13.0), x + self.d(18.0), y + h / 2 + self.d(18.0), quiet, game.system);
                targets.push((Area::new(x, room.top + y, w, pitch).clip_y(room.top, room.bottom), game.name.clone(), pos));
            }
        }
        for (area, name, pos) in targets {
            if area.h > self.d(12.0) {
                self.target(area, &name, Action::Row(pos));
            }
        }
        self.scrollbar(c, room.top, view, self.visible.len() as i32 * pitch);
    }
    /// Landscape Shelf, Cabinet and Cards: the handheld's shelf filling the page's height. The selected game
    /// stands out with its front; its neighbours are spines (fronts in Cards). Drag along it with momentum.
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
        let mut case_h = scene_h - self.d(cabinet as u8 as f32 * 14.0) - self.d(10.0);
        if self.prefs.theme == Theme::Cards {
            // Cards are all fronts: smaller, so four or five stand on the shelf instead of two.
            case_h = case_h.min((width - margin) / 4);
        }
        let gap = self.d(4.0);
        // Neighbours first, to find how many fit left of the selected game (a third of the shelf).
        let lib_len = self.visible.len();
        let mut pics: Vec<(usize, art::Pic)> = Vec::new();
        let mut used = 0;
        let mut start = self.selected;
        while start > 0 {
            let pic = self.shelf_pic(text, start - 1, false, case_h, cover);
            if used + pic.w + gap > width / 3 {
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
            art::blit(c, &pic, x, base - pic.h - up, shade, (room.top, room.bottom));
            if selected && self.controller_hints() {
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
            self.target(area, &name, Action::Row(pos));
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
    /// A game's spine (or front, selected or in Cards) at the shelf's case height.
    fn shelf_pic(&mut self, text: &mut Text, pos: usize, selected: bool, case_h: i32, cover: Option<&Image>) -> art::Pic {
        let index = self.visible[pos];
        let cards = self.prefs.theme == Theme::Cards;
        if selected || cards {
            return self.front_pic(text, index, selected, (case_h, case_h), cover).unwrap_or_else(|| art::Pic::new(1, 1, 0));
        }
        if self.cards.len() > 96 {
            self.cards.clear();
        }
        let key = (index, false);
        if let Some(pic) = self.cards.get(&key).filter(|p| p.h == case_h) {
            return pic.clone();
        }
        let game = &self.library.as_ref().unwrap().games[index];
        let m = Medium::of(game.system_id);
        let k = case_h as f32 / m.size().h as f32;
        let look = art::Look {
            color: game.color,
            band: game.color,
            plastic: art::plastic(game.system_id),
        };
        let mut pic = media::spine(text, m, k, look, &game.name);
        let mut stickers = pocket_ui::case_stickers::CaseStickers::default();
        stickers.set(pocket_ui::case_stickers::Kind::Favourite, self.prefs.extras.case_stickers && self.prefs.favorites.contains(&game.key), 2026);
        let (lx, ly, lw, _) = media::spine_label_rect(m, k);
        pocket_ui::case_stickers::on_spine(text, &mut pic, &stickers, (lx, 0, lw, ly), index as u64);
        if pic.h != case_h && pic.h > 0 {
            pic = pic.scaled(pic.w * case_h / pic.h, case_h);
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
                    tr("Choose another folder."),
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
        let side = c.w > c.h && pocket_ui::ui_art::get(picture).is_some();
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
        let lines = text.wrap_at_most(Face::Body, self.sp(16.0), line, w, 3);
        let hs = self.sp(24.0);
        let text_h = hs as i32 + self.d(10.0) + lines.len() as i32 * self.d(24.0) + if button.is_some() { button_h + self.d(28.0) } else { 0 };
        let art_h = if side { 0 } else { (room_h - text_h - self.d(48.0)).min(self.d(200.0)) };
        let block = text_h + if art_h > self.d(64.0) { art_h + self.d(16.0) } else { 0 };
        let mut y = if side { room.top + (room_h - block) / 2 } else { room.top + self.d(16.0) };
        if !side && art_h > self.d(64.0) {
            if let Some(sprite) = pocket_ui::ui_art::get(picture) {
                let fitted = sprite.fitted(w.min(self.d(260.0)), art_h);
                c.blit_sprite(&fitted, tx + (w - fitted.w as i32) / 2, y + (art_h - fitted.h as i32) / 2, 0);
                y += art_h + self.d(16.0);
            }
        }
        let heading_shown = text.fit(Face::Title, hs, heading, w);
        let hw = text.width(Face::Title, hs, &heading_shown);
        y += hs as i32;
        text.draw(c, Face::Title, hs, tx + (w - hw) / 2, y, pal().chalk, &heading_shown);
        self.target(Area::new(tx, y - hs as i32, w, hs as i32 + self.d(8.0)), heading, Action::Label);
        y += self.d(10.0);
        for l in &lines {
            y += self.d(24.0);
            let lw = text.width(Face::Body, self.sp(16.0), l);
            text.draw(c, Face::Body, self.sp(16.0), tx + (w - lw) / 2, y, pal().dust, l);
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
        if self.query.is_empty() && self.keyboard.is_none() {
            self.empty_page(c, text, below, Art::Search, "Find a game", &tr("Type a few letters of its name."), None);
        } else if self.visible.is_empty() {
            if self.keyboard.is_none() {
                self.empty_page(c, text, below, Art::Search, "No games found", &tr("Try fewer letters."), None);
            }
        } else if below.bottom - below.top >= self.d(64.0) {
            self.paint_list_phone(c, text, below, c.w);
            if self.keyboard.is_none() && c.w < c.h {
                // The matches are a list; Play and Info go in the strip like the shelf's.
            }
        }
    }
    pub(super) fn keyboard_height(&self, c: &Canvas) -> i32 {
        let rows = self.keyboard.as_ref().map_or(0, |k| k.rows().len()) as i32;
        let key_h = self.d(if c.w > c.h { 40.0 } else { 52.0 });
        rows * key_h + self.d(16.0)
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
                let s = if label.chars().count() > 1 { self.sp(14.0) } else { size };
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
        let boxed = !matches!(self.prefs.theme, Theme::Minimal | Theme::Cards | Theme::Split);
        let mut secondary: Vec<(&str, Action)> = vec![("Back", Action::Press(Button::South)), ("Favourite", Action::Press(Button::West))];
        if boxed {
            secondary.push(("Turn over", Action::Press(Button::North)));
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
                self.paint_box(&mut card, text, cover);
            }
            for y in 0..bh_card.min(c.h) {
                let (src, dst) = ((y * bw) as usize, (y * c.w) as usize);
                c.buf[dst..dst + bw as usize].copy_from_slice(&buf[src..src + bw as usize]);
            }
            self.card_buf = buf;
            self.link_targets((0, 0));
            // The info page has the name at its top already; the box shows only its cover.
            if let Some(game) = self.game().cloned().filter(|_| boxed) {
                let ts = self.sp(22.0);
                let mut y = self.d(16.0);
                for line in text.wrap_at_most(Face::Title, ts, &game.name, col_w, 2) {
                    y += ts as i32;
                    text.draw(c, Face::Title, ts, col_x, y, pal().chalk, &line);
                    y += self.d(4.0);
                }
                y += self.d(18.0);
                let system = text.fit(Face::Body, self.sp(14.0), game.system, col_w);
                text.draw(c, Face::Body, self.sp(14.0), col_x, y, pal().dust, &system);
                self.target(Area::new(col_x, 0, col_w, y + self.d(8.0)), &game.name, Action::Label);
            }
            let play_h = self.d(52.0);
            let mut y = c.h - hint_h - self.d(12.0) - play_h;
            self.pill(c, text, Area::new(col_x, y, col_w, play_h), "Play", true, Action::Press(Button::East));
            for (label, action) in secondary.into_iter().rev() {
                y -= bh + gap;
                self.pill_in(c, text, Area::new(col_x, y, col_w, bh), label, colors(label), action);
            }
        } else {
            let bar = 2 * bh + 3 * gap;
            let top = c.h - bar - hint_h;
            // The game's name and system above its box, where the phone has the room.
            let head = if boxed {
                let game = self.game().cloned();
                let mut y = self.d(28.0);
                if let Some(game) = game {
                    let ts = self.sp(26.0);
                    for line in text.wrap_at_most(Face::Title, ts, &game.name, c.w - 2 * margin, 2) {
                        y += ts as i32;
                        text.draw(c, Face::Title, ts, margin, y, pal().chalk, &line);
                        y += self.d(4.0);
                    }
                    y += self.d(20.0);
                    text.draw(c, Face::Body, self.sp(15.0), margin, y, pal().dust, game.system);
                    self.target(Area::new(margin, 0, c.w - 2 * margin, y + self.d(8.0)), &game.name, Action::Label);
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
                self.paint_box(&mut card, text, cover);
            }
            self.link_targets((0, head));
            if let Some(y) = facts_at {
                c.rect(0, y - self.d(8.0), c.w, top - y + self.d(8.0), pal().void);
                self.card_facts(c, text, (margin, y, c.w - 2 * margin, top - self.d(12.0)));
            }
            c.rect(0, top, c.w, c.h - top, pal().void);
            let w = c.w - 2 * margin;
            self.pill(c, text, Area::new(margin, top + gap, w, bh), "Play", true, Action::Press(Button::East));
            let n = secondary.len() as i32;
            let sw = (w - gap * (n - 1)) / n;
            for (i, (label, action)) in secondary.into_iter().enumerate() {
                self.pill_in(c, text, Area::new(margin + i as i32 * (sw + gap), top + 2 * gap + bh, sw, bh), label, colors(label), action);
            }
        }
        if hints {
            let mut keys = vec![("A", "Play"), ("B", "Back"), ("Y", "Favourite")];
            if boxed {
                keys.insert(2, ("X", "Turn over"));
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
        let key_w = self.d(96.0);
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
        if rows.is_empty() && !card.matched {
            for l in text.wrap_at_most(Face::Body, size, &tr("Not in the offline game list, so there are no facts to show."), w, 2) {
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

/// The middle of a line of `size` px to its baseline.
pub(super) fn type_mid(size: u32) -> i32 {
    type_scale::mid(size)
}
fn c_band(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: u32) {
    c.round_rect(x, y, w, h, h / 2, color);
}
use pocket_ui::type_scale;

trait ClipY {
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
