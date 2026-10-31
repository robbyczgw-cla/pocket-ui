//! Part of the pocket-ui binary: the "This game" page (`pocket_ui::this_game_page`) over a game's card.
//!
//! START on the card opens it (one press); "Runs on" inside it is the emulator page (`app_emu_choice`), drawn over it.
//! The page reads the card's game once when it opens (the settings files and, for a standalone emulator, system.cfg:
//! a handful of small files, logged) and writes each change at once, where the pause menu and pocket-core read them.

use std::path::Path;
use std::time::Instant;

use pocket_ui::emu_settings::Files;
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::this_game_page::{Action, Mode, Row, Subject, ThisGamePage};
use pocket_ui::touch::Area;

use super::app_emu_choice::Opener;
use super::{App, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

impl App {
    /// What plays the game at `path` now, as the page's subject: the console, the resolved emulator, whether pocket-core
    /// could host the system.
    fn this_game_subject(&self, system: &str, name: &str, path: &Path) -> Option<Subject> {
        let sys = self.systems.iter().find(|s| s.name == system)?;
        let hosted = self.emu_hostable(sys, path);
        let runner = self.resolve_runner(sys, Some(path), std::path::Path::new(super::host_bin()).exists()).runner;
        Some(Subject {
            system: sys.name.clone(),
            system_name: sys.fullname.clone(),
            name: name.to_string(),
            rom: path.to_path_buf(),
            hosted,
            runs_on: pocket_ui::consoles::runner_text(&runner.emulator, &runner.core).unwrap_or_default(),
            on_pocket_core: runner.emulator == pocket_ui::emu_choice::POCKET_CORE,
            emu: pocket_ui::standalone::Emu::detect(&sys.name, &runner.emulator, &runner.core),
        })
    }

    /// START on a game's card.
    pub(crate) fn this_game_for_card(&mut self) {
        let Some(open) = self.card.as_ref() else { return };
        let (system, name, path) = (open.system.clone(), open.name.clone(), open.path.clone());
        let started = Instant::now();
        let Some(subject) = self.this_game_subject(&system, &name, &path) else {
            pocket_ui::ui_sound::refuse();
            return;
        };
        self.this_game = Some(ThisGamePage::new(subject, &self.data_dir, Files::default()));
        println!("this game: page for {system} opened in {:?}", started.elapsed());
    }

    /// The page is open over the card (and "Runs on" is not drawn over it).
    pub(crate) fn this_game_on_shelf(&self) -> bool {
        self.this_game.is_some() && self.running.is_none() && !self.emu_choice_on_shelf()
    }

    /// Open at all, also while "Runs on" is on top of it.
    pub(crate) fn this_game_open(&self) -> bool {
        self.this_game.is_some() && self.running.is_none()
    }

    pub(crate) fn this_game_input(&mut self, b: Button) {
        let Some(page) = self.this_game.as_mut() else { return };
        match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => self.this_game = None,
            Action::RunsOn => {
                let Some(open) = self.card.as_ref() else { return };
                let (system, name, path) = (open.system.clone(), open.name.clone(), open.path.clone());
                if !self.open_emu_choice(&system, Some((&name, &path)), Opener::Shelf) {
                    pocket_ui::ui_sound::refuse();
                }
            }
        }
    }

    /// Where the page stands, for the sounds and the repaint check.
    pub(crate) fn this_game_mark(&self) -> Option<(usize, Mode, u32)> {
        self.this_game.as_ref().map(|p| p.mark())
    }

    pub(crate) fn paint_this_game(&mut self, c: &mut Canvas) {
        let Some(mut page) = self.this_game.take() else { return };
        // "Runs on" may have been changed since the page last painted.
        if let Some(open) = self.card.as_ref() {
            if let Some(s) = self.this_game_subject(&open.system, &open.name, &open.path) {
                page.subject.runs_on = s.runs_on;
                page.subject.on_pocket_core = s.on_pocket_core;
            }
        }
        c.clear(pal().void);
        self.paint_this_game_page(c, &page);
        self.this_game = Some(page);
    }

    fn paint_this_game_page(&mut self, c: &mut Canvas, page: &ThisGamePage) {
        let pad = NU_PAD;
        self.status_pill(c);
        let right = c.w - pad;
        let title = self.text.fit(Face::Title, 50, &page.subject.name, c.w - 2 * pad - 2 * NU_BTN_PAD - 330);
        self.text.draw(c, Face::Title, 50, pad + NU_BTN_PAD, pad + 58, pal().chalk, &title);
        let sub = match page.mode {
            Mode::List => format!("This game · {}", page.subject.system_name),
            Mode::Controls(_) => format!("Controls · {}", page.subject.system_name),
        };
        let sub = self.text.fit(Face::Body, 28, &sub, c.w - 2 * pad - 2 * NU_BTN_PAD);
        self.text.draw(c, Face::Body, 28, pad + NU_BTN_PAD, pad + 104, pal().dust, &sub);
        let top = pad + NU_PILL + 46;

        // (name, value or ">" for a page, explanation) of every line, and the selected one.
        // `arrows`: Left and Right change the value (not on a row that opens a page).
        let (lines, arrows, sel): (Vec<(String, String, String)>, Vec<bool>, usize) = match page.mode {
            Mode::List => {
                let rows = page.rows();
                let arrows = rows.iter().map(|r| matches!(r, Row::Setting(_) | Row::Standalone(_))).collect();
                (rows.into_iter().map(|r| page.describe(r)).collect(), arrows, page.sel)
            }
            Mode::Controls(sel) => {
                let rows = page.control_rows();
                (rows.iter().map(|r| (r.name.clone(), r.value.clone(), r.desc.clone())).collect(), vec![true; rows.len()], sel)
            }
        };
        let sel = sel.min(lines.len().saturating_sub(1));
        let font = 34u32;
        let row = pocket_ui::type_scale::row(54, font);
        // Under the rows: the selected row's explanation (up to three lines), then up to two notes. The rest is rows.
        let hints_top = c.h - pad - NU_PILL - 16;
        let below = 4 * pocket_ui::type_scale::line(NU_SMALL, 12) + 3 * pocket_ui::type_scale::line(24, 8) + 28;
        let visible = (((hints_top - top - below) / row).max(3) as usize).min(lines.len().max(1));
        let first = super::window_start(sel, visible, lines.len());
        for (k, (name, value, _)) in lines.iter().enumerate().skip(first).take(visible) {
            let y = top + (k - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), k);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let selected = k == sel;
            let value_w = if value.is_empty() { 0 } else { self.text.width(Face::Body, font, value) + 60 };
            let name = self.text.fit(Face::Body, font, name, right - pad - 2 * NU_BTN_PAD - value_w);
            if selected {
                let w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, &name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, &name);
            }
            if value.is_empty() {
                continue;
            }
            let vx = right - NU_BTN_PAD;
            let name_w = self.text.width(Face::Body, font, &name);
            if value == ">" {
                self.text.draw_right(c, Face::Body, font, vx, base, pal().text, ">");
                continue;
            }
            let room = right - pad - 2 * NU_BTN_PAD - name_w - 60;
            let shown = if selected && arrows[k] { format!("‹ {value} ›") } else { value.clone() };
            let shown = self.text.fit(Face::Body, font, &shown, room);
            self.text.draw_right(c, Face::Body, font, vx, base, pal().text, &shown);
            if selected && arrows[k] {
                let vw = self.text.width(Face::Body, font, &shown);
                let aw = self.text.width(Face::Body, font, "‹ ") + 24;
                self.hits.button(Area::new(vx - vw - 24, y, aw, row), Button::Left);
                self.hits.button(Area::new(vx - aw + 24, y, aw, row), Button::Right);
            }
        }
        self.hits.list(sel, lines.len(), 0.0, row as f32);
        if lines.len() > visible {
            // The window's place in the list, as the pause menu's lists show it.
            let track = visible as i32 * row;
            let (bar_h, bar_y) = (track * visible as i32 / lines.len() as i32, track * first as i32 / lines.len() as i32);
            c.rect(c.w - 12, top, 4, track, pal().band);
            c.rect(c.w - 12, top + bar_y, 4, bar_h.max(12), pal().dust);
        }

        // What the selected row is for, then where the settings come from.
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let mut y = top + visible as i32 * row + 30;
        let desc = lines.get(sel).map(|l| l.2.clone()).unwrap_or_default();
        for line in desc.lines() {
            y = self.text.paragraph(c, Face::Body, NU_SMALL, (left, y), width, pocket_ui::type_scale::line(NU_SMALL, 12), hints_top, pal().dust, line);
        }
        y += 8;
        let mut notes: Vec<(String, u32)> = Vec::new();
        if let Some(n) = &page.note {
            notes.push((n.clone(), pal().amber));
        }
        if page.picture_selected() {
            notes.push((pocket_ui::this_game_page::PICTURE_HINT.to_string(), pal().chalk));
        }
        if matches!(page.mode, Mode::List) {
            let level = page.level_line();
            if !level.is_empty() {
                notes.push((level.to_string(), pal().quiet));
            }
            if let Some(f) = page.footer() {
                notes.push((f.to_string(), pal().quiet));
            }
        }
        for (text, color) in notes {
            y = self.text.paragraph(c, Face::Body, 24, (left, y), width, pocket_ui::type_scale::line(24, 8), hints_top, color, &text);
            y += 4;
        }

        let hints: &[(&str, &str)] = match page.mode {
            Mode::Controls(_) => &[("B", "Back"), ("< >", "Change")],
            Mode::List => match page.row() {
                Row::Setting(_) | Row::Standalone(_) => &[("B", "Back"), ("< >", "Change")],
                Row::Reset => &[("B", "Back"), ("A", "Reset")],
                Row::Controls | Row::RunsOn => &[("B", "Back"), ("A", "Open")],
            },
        };
        self.nu_hints(c, hints);
    }

    /// `--snapshot this-game[:<system>[:<game file>]][:<row>][:<key>=<value>...]`: the page for a game of the snapshot
    /// library, with some values set in memory (nothing is written) to show a frame's sub-choices.
    pub(crate) fn this_game_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("this-game") else { return false };
        let mut parts = rest.split(':').filter(|p| !p.is_empty());
        let (mut system, mut file, mut row, mut sets) = (None::<String>, None::<String>, 0usize, Vec::new());
        for p in parts.by_ref() {
            if let Ok(n) = p.parse() {
                row = n;
            } else if let Some((k, v)) = p.split_once('=') {
                sets.push((k.to_string(), v.to_string()));
            } else if system.is_none() {
                system = Some(p.to_string());
            } else {
                file = Some(p.to_string());
            }
        }
        let system = system.unwrap_or_else(|| "gba".into());
        let Some(sys) = self.systems.iter().find(|s| s.name == system) else {
            eprintln!("this-game:{system}: no such system in the library");
            return false;
        };
        let game = file
            .as_ref()
            .and_then(|f| sys.games.iter().find(|g| g.path.file_name().and_then(|n| n.to_str()) == Some(f.as_str())))
            .or_else(|| sys.games.first());
        let (name, path) = match game {
            Some(g) => (g.name.clone(), g.path.clone()),
            None => ("Game".to_string(), std::path::PathBuf::from(format!("/storage/roms/{system}/Game.rom"))),
        };
        let Some(mut subject) = self.this_game_subject(&system, &name, &path) else { return false };
        // "emu=hakux": the standalone emulator the snapshot library's EmulationStation file doesn't list.
        if let Some((_, e)) = sets.iter().find(|(k, _)| k == "emu") {
            subject.emu = pocket_ui::standalone::Emu::detect(&system, e, "");
        }
        let mut page = ThisGamePage::new(subject, &self.data_dir, Files::default());
        for (k, v) in sets.into_iter().filter(|(k, _)| k != "emu") {
            page.demo_set(&k, &v);
        }
        page.sel = row.min(page.rows().len() - 1);
        self.this_game = Some(page);
        true
    }
}
