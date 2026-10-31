//! Android's first start: portable Pocket painters, persisted progress, and framework SAF on a worker.
use super::*;
const NEXT: u16 = 100;
const SKIP: u16 = 101;
const BACK: u16 = 102;

impl Ui {
    fn setup_items(&self) -> Vec<(String, u16)> {
        let mut items = match self.prefs.onboarding_step.min(6) {
            1 => vec![("English".into(), 0), ("Deutsch".into(), 1)],
            2 => vec![(
                tr(if self.library.is_some() {
                    "Choose another folder"
                } else {
                    "Choose your ROM folder"
                }),
                0,
            )],
            3 => {
                let mut rows: Vec<_> = Theme::ALL
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (format!("{}{}", if self.prefs.theme == *t { "• " } else { "" }, tr(t.name())), i as u16))
                    .collect();
                rows.push((
                    format!("{} · {}", tr("Animations"), tr(if self.prefs.cozy.unwrap_or(true) { "Cozy" } else { "Quick" })),
                    5,
                ));
                rows.push((format!("{} · {}", tr("Appearance"), tr(if self.prefs.light { "Light" } else { "Dark" })), 6));
                rows
            }
            4 => vec![
                (
                    format!("{}{}", if self.prefs.download_media == Some(true) { "• " } else { "" }, tr("Download on Wi-Fi")),
                    0,
                ),
                (
                    format!("{}{}", if self.prefs.download_media == Some(false) { "• " } else { "" }, tr("Not now")),
                    1,
                ),
            ],
            _ => vec![],
        };
        items.push((tr("Continue"), NEXT));
        if self.prefs.onboarding_step < 6 {
            items.push((tr("Skip"), SKIP));
        }
        if self.prefs.onboarding_step > 0 {
            items.push((tr("Back"), BACK));
        }
        items
    }
    fn setup_activate(&mut self, index: usize) -> Command {
        let Some((_, action)) = self.setup_items().get(index).cloned() else {
            return Command::None;
        };
        self.setup_row = index;
        let step = self.prefs.onboarding_step.min(6);
        match action {
            NEXT | SKIP => {
                if step == 4 && self.prefs.download_media.is_none() {
                    self.prefs.download_media = Some(false);
                }
                if step == 6 {
                    self.prefs.onboarding_done = true;
                    self.navigate(Screen::Library);
                } else {
                    self.prefs.onboarding_step = step + 1;
                }
                self.setup_row = 0;
            }
            BACK => {
                self.prefs.onboarding_step = step.saturating_sub(1);
                self.setup_row = 0;
            }
            _ => match step {
                1 => {
                    let de = action == 1;
                    self.prefs.german = Some(de);
                    pocket_ui::i18n::set(if de { pocket_ui::i18n::Lang::De } else { pocket_ui::i18n::Lang::En });
                }
                2 => return Command::Folder,
                3 => {
                    match action {
                        0..=4 => self.prefs.theme = Theme::ALL[action as usize],
                        5 => self.prefs.cozy = Some(!self.prefs.cozy.unwrap_or(true)),
                        6 => self.prefs.light = !self.prefs.light,
                        _ => (),
                    }
                    self.cover_changed();
                }
                4 => self.prefs.download_media = Some(action == 0),
                _ => (),
            },
        }
        Command::PersistSetup
    }
    pub(super) fn setup_press(&mut self, button: Button) -> Command {
        let items = self.setup_items();
        match button {
            Button::Up | Button::Left => self.setup_row = self.setup_row.saturating_sub(1),
            Button::Down | Button::Right => self.setup_row = (self.setup_row + 1).min(items.len() - 1),
            Button::East => return self.setup_activate(self.setup_row),
            Button::South => {
                if let Some(i) = items.iter().position(|(_, a)| *a == BACK) {
                    return self.setup_activate(i);
                }
            }
            Button::Start => {
                if let Some(i) = items.iter().position(|(_, a)| *a == SKIP) {
                    return self.setup_activate(i);
                }
            }
            _ => (),
        }
        Command::None
    }
    pub(super) fn setup_tap(&mut self, x: f32, y: f32) -> Command {
        let index = self.regions.iter().rev().find_map(|r| {
            if r.area.contains(x, y) {
                if let Action::Setup(i) = r.action {
                    Some(i)
                } else {
                    None
                }
            } else {
                None
            }
        });
        index.map_or(Command::None, |i| self.setup_activate(i))
    }
    /// Whether choice `action` of this step is the one in use (a radio mark), or `None` for a row that acts.
    fn setup_chosen(&self, action: u16) -> Option<bool> {
        match self.prefs.onboarding_step.min(6) {
            1 => Some((action == 1) == (pocket_ui::i18n::current() == pocket_ui::i18n::Lang::De)),
            3 if action < 5 => Some(Theme::ALL.get(action as usize) == Some(&self.prefs.theme)),
            4 => Some(self.prefs.download_media == Some(action == 0)),
            _ => None,
        }
    }
    pub(super) fn paint_setup(&mut self, c: &mut Canvas, text: &mut Text) {
        let step = self.prefs.onboarding_step.min(6);
        let wide = c.w > c.h;
        let margin = self.d(if wide { 32.0 } else { 24.0 });
        let body_w = if wide { (c.w - 2 * margin) * 46 / 100 } else { c.w - 2 * margin };
        let titles = [
            "Welcome to pocket-ui",
            "Language",
            "Where your games are",
            "Your look",
            "Covers and pictures",
            "Your controls",
            "Ready to play",
        ];
        let descriptions = [
            "Your games, a shelf of your own, and a player built in. Let's make it yours.",
            "Choose the language for menus and help. You can change it in Settings.",
            "Choose your ROM folders. Android remembers access; your game files stay where they are.",
            "Pick a look. Cozy takes time to open the case; Quick gets straight into the game.",
            "Covers and pictures download on unmetered Wi-Fi as you browse. Your choice stays in Settings.",
            "Tap a game to select it, tap again to play. Swipe along the shelf. Back goes one step back.",
            "Everything is ready. You can change these choices in Settings and add more game folders later.",
        ];
        let hints = self.controller_hints();
        let items = self.setup_items();
        // The step indicator: seven segments, the ones done in Chalk, and "Step n of 7" under them.
        let seg_gap = self.d(6.0);
        let skip_w = if step < 6 { self.d(88.0) } else { 0 };
        let bar_w = body_w - skip_w - self.d(16.0);
        let seg = (bar_w - 6 * seg_gap) / 7;
        let bar_y = self.d(20.0);
        for i in 0..7 {
            let color = if i <= step as i32 { pal().chalk } else { pal().pill };
            c.round_rect(margin + i * (seg + seg_gap), bar_y, seg, self.d(4.0), self.d(2.0), color);
        }
        let progress = tr(&format!("Step {} of 7", step + 1));
        let small = self.sp(14.0);
        text.draw(c, Face::Body, small, margin, bar_y + self.d(30.0), pal().dust, &progress);
        self.region(Area::new(margin, bar_y - self.d(8.0), bar_w, self.d(44.0)), progress, Action::Label);
        let target = self.d(48.0);
        let mut nav = Vec::new();
        for (i, (label, action)) in items.iter().enumerate() {
            if *action >= NEXT {
                nav.push((i, label.clone(), *action));
            }
        }
        if let Some((i, label, _)) = nav.iter().find(|(_, _, a)| *a == SKIP).cloned() {
            // Skip: a quiet text button at the top right, a whole 48 dp target.
            let area = Area::new(margin + body_w - skip_w, bar_y - self.d(12.0), skip_w, target);
            let on = hints && i == self.setup_row;
            if on {
                c.round_rect(area.x, area.y, area.w, area.h, area.h / 2, pal().pill);
            }
            let sw = text.width(Face::Title, self.sp(16.0), &label);
            text.draw(c, Face::Title, self.sp(16.0), area.x + (area.w - sw) / 2, area.y + area.h / 2 + phone::type_mid(self.sp(16.0)), pal().chalk, &label);
            self.region(area, label, Action::Setup(i));
        }
        let mut y = bar_y + self.d(if wide { 60.0 } else { 72.0 });
        let ts = self.sp(if wide { 26.0 } else { 30.0 });
        for line in text.wrap_at_most(Face::Title, ts, &tr(titles[step]), body_w, 2) {
            text.draw(c, Face::Title, ts, margin, y, pal().chalk, &line);
            y += ts as i32 + self.d(4.0);
        }
        self.region(Area::new(margin, y - 2 * ts as i32, body_w, ts as i32 + self.d(8.0)), tr(titles[step]), Action::Label);
        y += self.d(4.0);
        let bs = self.sp(if wide { 15.0 } else { 16.0 });
        let line_h = bs as i32 + self.d(8.0);
        for line in text.wrap(Face::Body, bs, &tr(descriptions[step]), body_w) {
            text.draw(c, Face::Body, bs, margin, y, pal().dust, &line);
            y += line_h;
        }
        y += self.d(8.0);
        // Back and Continue: at the bottom of the text column, Continue the primary one and the wider.
        let bh = self.d(if wide { 48.0 } else { 52.0 });
        let buttons_y = c.h - bh - self.d(16.0);
        let back = nav.iter().find(|(_, _, a)| *a == BACK).cloned();
        let next = nav.iter().find(|(_, _, a)| *a == NEXT).cloned();
        let gap = self.d(12.0);
        let back_w = if back.is_some() { (body_w - gap) / 3 } else { 0 };
        if let Some((i, label, _)) = back {
            let area = Area::new(margin, buttons_y, back_w, bh);
            self.setup_button(c, text, area, &label, false, i);
        }
        if let Some((i, label, _)) = next {
            let x = margin + if back_w > 0 { back_w + gap } else { 0 };
            let area = Area::new(x, buttons_y, margin + body_w - x, bh);
            self.setup_button(c, text, area, &label, true, i);
        }
        // What the step shows and asks: under the words in portrait, in the right-hand column in landscape.
        let (rx, rw, mut ry, r_bottom) = if wide {
            let x = margin + body_w + self.d(32.0);
            (x, c.w - x - margin, self.d(20.0), c.h - self.d(16.0))
        } else {
            (margin, body_w, y, buttons_y - self.d(16.0))
        };
        if step == 2 {
            let status = if self.scan_running {
                tr("Reading library...")
            } else if let Some(lib) = &self.library {
                let systems: HashSet<_> = lib.games.iter().map(|g| g.system_id).collect();
                let found = tr(&format!("Found {} games on {} systems", lib.games.len(), systems.len()));
                if lib.games.is_empty() {
                    format!("{found}. {}", tr("No games found. Choose another folder or skip for now."))
                } else {
                    found
                }
            } else {
                tr("No folder chosen yet. You can add games later.")
            };
            let panel_lines = text.wrap(Face::Body, bs, &status, rw - self.d(32.0));
            let ph = panel_lines.len() as i32 * line_h + self.d(24.0);
            kit::panel(c, rx, ry, rw, ph);
            let mut ly = ry + self.d(12.0) + bs as i32;
            for line in &panel_lines {
                text.draw(c, Face::Body, bs, rx + self.d(16.0), ly, pal().chalk, line);
                ly += line_h;
            }
            self.region(Area::new(rx, ry, rw, ph), status, Action::Label);
            ry += ph + self.d(12.0);
            if !self.message.is_empty() && !self.scan_running {
                let message = text.fit(Face::Body, small, &self.message, rw);
                text.draw(c, Face::Body, small, rx, ry + small as i32, pal().alert, &message);
                ry += line_h;
            }
        }
        if step == 5 {
            let status = tr(if self.controller_connected { "Controller detected" } else { "Touch" });
            let ss = self.sp(22.0);
            text.draw(c, Face::Title, ss, rx, ry + ss as i32, pal().chalk, &status);
            self.region(Area::new(rx, ry, rw, ss as i32 + self.d(8.0)), status, Action::Label);
            ry += ss as i32 + self.d(16.0);
            for line in text.wrap(Face::Body, small, &tr("Controller: D-pad moves, A chooses, B goes back, X opens the case, START opens Settings."), rw) {
                text.draw(c, Face::Body, small, rx, ry + small as i32, pal().dust, &line);
                ry += small as i32 + self.d(8.0);
            }
            ry += self.d(8.0);
        }
        if step == 4 {
            let label = tr(if self.wifi_ready { "Wi-Fi ready" } else { "Downloads wait for unmetered Wi-Fi." });
            text.draw(c, Face::Body, small, rx, ry + small as i32, pal().dust, &label);
            ry += small as i32 + self.d(16.0);
        }
        // The choices: rows with a radio mark for a setting, a pill for an action. The looks go two to a row so their
        // preview keeps room; Animations and Appearance share the last row.
        let choices: Vec<(usize, String, u16)> = items.iter().enumerate().filter(|(_, (_, a))| *a < NEXT).map(|(i, (l, a))| (i, l.clone(), *a)).collect();
        let pitch = self.d(if wide { 52.0 } else { 60.0 });
        let col_gap = self.d(10.0);
        let half = (rw - col_gap) / 2;
        // Each choice's (column, row, width): the looks two to a row; Animations and Appearance side by side in
        // landscape, each across the column in portrait (their German values need the width).
        let cells: Vec<(i32, i32, i32)> = (0..choices.len() as i32)
            .map(|n| match (step, n) {
                (3, 0..=4) => (n % 2, n / 2, half),
                (3, _) if wide => (n - 5, 3, half),
                (3, _) => (0, n - 2, rw),
                _ => (0, n, rw),
            })
            .collect();
        let list_h = cells.iter().map(|(_, r, _)| r + 1).max().unwrap_or(0) * pitch;
        // The look's preview goes above its choices; everything else's picture below them.
        let preview_h = if step == 3 { (r_bottom - ry - list_h - self.d(12.0)).min(self.d(220.0)) } else { 0 };
        if step == 3 && preview_h > self.d(60.0) {
            let (w, h) = (rw, preview_h);
            let key = (self.prefs.theme, self.prefs.light, w, h);
            if self.setup_preview.as_ref().is_none_or(|(old, _)| *old != key) {
                self.setup_preview = Some((key, look_preview(text, self.prefs.theme, w, h, self.dp)));
            }
            if let Some((_, pic)) = &self.setup_preview {
                art::blit(c, pic, rx + (w - pic.w) / 2, ry, 256, (0, c.h));
            }
            ry += preview_h + self.d(12.0);
        }
        let top = ry.min(r_bottom - list_h);
        for (n, (i, label, action)) in choices.iter().enumerate() {
            let (col, row, cw) = cells[n];
            let area = Area::new(rx + col * (half + col_gap), top + row * pitch, cw, pitch - self.d(8.0));
            let on = hints && *i == self.setup_row;
            // The mark replaces the "• " on screen; the accessibility name keeps it (TalkBack says which is chosen).
            let spoken = label.clone();
            let label = label.trim_start_matches("• ").to_string();
            match self.setup_chosen(*action) {
                Some(chosen) => {
                    let bg = if chosen { pal().label } else { pal().spine };
                    let fg = if chosen { pal().ink } else { pal().chalk };
                    c.round_rect(area.x, area.y, area.w, area.h, self.d(12.0), bg);
                    if on {
                        c.frame(area.x - 3, area.y - 3, area.w + 6, area.h + 6, self.d(2.0), pal().chalk);
                    }
                    let r = self.d(20.0);
                    self.radio(c, area.x + self.d(14.0) + r / 2, area.y + area.h / 2, r, chosen, fg);
                    let s = self.sp(17.0);
                    let shown = text.fit(Face::Body, s, &label, area.w - r - self.d(40.0));
                    text.draw(c, Face::Body, s, area.x + r + self.d(26.0), area.y + area.h / 2 + phone::type_mid(s), fg, &shown);
                    self.region(area, spoken, Action::Setup(*i));
                }
                None => self.setup_button(c, text, area, &label, false, *i),
            }
        }
        // The same handmade art as the shelf's empty pages, as large as the free room allows.
        let after = top + list_h + self.d(12.0);
        let (ax, mut ay, aw, mut ah) = (rx, after, rw, r_bottom - after);
        if step == 0 && ah > self.d(120.0) {
            // pocket-ui's mark above the shelf, in whole pixels (it is pixel art).
            if let Some(logo) = pocket_ui::ui_art::get(pocket_ui::ui_art::Art::Logo) {
                let k = ((ah * 2 / 5).min(self.d(150.0)) / logo.h as i32).max(1);
                let (lw, lh) = (logo.w as i32 * k, logo.h as i32 * k);
                let top = ay + self.d(8.0);
                pocket_ui::ui_art::paint(c, pocket_ui::ui_art::Art::Logo, ax + (aw - lw) / 2, top, k);
                self.region(Area::new(ax + (aw - lw) / 2, top, lw, lh), "pocket-ui".into(), Action::Label);
                ay = top + lh + self.d(16.0);
                ah = r_bottom - ay;
            }
        }
        if step != 3 && aw > self.d(80.0) && ah > self.d(80.0) {
            let picture = if step == 4 {
                pocket_ui::ui_art::Art::Screenshots
            } else {
                pocket_ui::ui_art::Art::NoGames
            };
            if let Some(sprite) = pocket_ui::ui_art::get(picture) {
                let fitted = sprite.fitted(aw.min(self.d(320.0)), ah.min(self.d(300.0)));
                c.blit_sprite(&fitted, ax + (aw - fitted.w as i32) / 2, ay + (ah - fitted.h as i32) / 2, 0);
            }
        }
    }
    fn setup_button(&mut self, c: &mut Canvas, text: &mut Text, area: Area, label: &str, primary: bool, i: usize) {
        let on = self.controller_hints() && i == self.setup_row;
        let (bg, fg) = if primary { (pal().label, pal().ink) } else { (pal().pill, pal().chalk) };
        c.round_rect(area.x, area.y, area.w, area.h, area.h / 2, bg);
        if on {
            c.frame(area.x - 4, area.y - 4, area.w + 8, area.h + 8, self.d(2.0), pal().chalk);
        }
        let s = self.sp(17.0);
        let shown = text.fit(Face::Title, s, label, area.w - self.d(24.0));
        let tw = text.width(Face::Title, s, &shown);
        text.draw(c, Face::Title, s, area.x + (area.w - tw) / 2, area.y + area.h / 2 + phone::type_mid(s), fg, &shown);
        self.region(area, label.into(), Action::Setup(i));
    }
}

fn look_preview(text: &mut Text, theme: Theme, w: i32, h: i32, dp: f32) -> art::Pic {
    let mut pic = art::Pic::new(w, h, 0);
    {
        let mut c = Canvas::new(&mut pic.px, w as u32, h as u32);
        c.clear(pal().void);
        let look = art::Look {
            color: 0x8468ba,
            band: 0x8468ba,
            plastic: art::plastic("gba"),
        };
        let m = Medium::Gba;
        if matches!(theme, Theme::Minimal | Theme::Split) {
            let listw = if theme == Theme::Split { w / 2 } else { w };
            // Rows in dp, like the phone's lists, so the preview reads at the same size.
            let (row, gap) = ((40.0 * dp) as i32, (6.0 * dp) as i32);
            let rows = ((h - gap) / (row + gap)).clamp(1, 4);
            let y0 = (h - rows * (row + gap) + gap) / 2;
            for i in 0..rows {
                let y = y0 + i * (row + gap);
                kit::shelf_row(&mut c, 0, y, listw - gap, row, i == 0);
                c.rect(0, y, (5.0 * dp) as i32, row, look.color);
                let font = (16.0 * dp) as u32;
                text.draw(&mut c, Face::Title, font, (16.0 * dp) as i32, y + row / 2 + font as i32 * 7 / 20, if i == 0 { pal().ink } else { pal().chalk }, &format!("pocket-ui {}", i + 1));
            }
            if theme == Theme::Split {
                let (fw, fh) = media::front_size(m, 1.0);
                let k = ((w - listw) as f32 * 0.8 / fw as f32).min(h as f32 * 0.85 / fh as f32);
                let front = media::front(text, m, k, look, "pocket-ui", None, pocket_ui::system_art::get("gba"), "Game Boy Advance");
                art::blit(&mut c, &front, listw + (w - listw - front.w) / 2, (h - front.h) / 2, 256, (0, h));
            }
        } else {
            if theme == Theme::Cabinet {
                art::blit(&mut c, &art::compartment(w, h - 20, 0), 0, 0, 256, (0, h));
            } else {
                c.rect(0, h - 24, w, 12, pal().spine);
            }
            // Three cases standing on the board, as large as the preview's height, together in the middle.
            let k = h as f32 / m.size().h as f32 * 0.8;
            let items: Vec<_> = (0..3)
                .map(|i| {
                    let item = if i == 1 || theme == Theme::Cards {
                        media::front(text, m, k, look, "pocket-ui", None, pocket_ui::system_art::get("gba"), "Game Boy Advance")
                    } else {
                        media::spine(text, m, k, look, "pocket-ui")
                    };
                    let available = (w / 3 - 20).max(20);
                    if item.w > available {
                        media::rescaled(&item, available as f32 / item.w as f32)
                    } else {
                        item
                    }
                })
                .collect();
            let total: i32 = items.iter().map(|p| p.w + 16).sum::<i32>() - 16;
            let mut x = (w - total) / 2;
            for item in &items {
                art::blit(&mut c, item, x, h - 30 - item.h, 256, (0, h));
                x += item.w + 16;
            }
        }
    }
    for pixel in &mut pic.px {
        *pixel |= 0xff00_0000;
    }
    pic
}
