//! Part of the pocket-ui binary: Settings → System → About's rows, "Report a problem" and "Open EmulationStation…",
//! and the sheet that says where the report went (`pocket_ui::report`). One press writes the zip on a thread of its
//! own; the sheet says "Writing the report…" until it is done, then the file's name, where it is, and what was left
//! out, or why it could not be written.

use std::sync::{Arc, Mutex};

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::report::{self, Failure, GameName, Saved};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{app_rescue, App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// About's rows, in order.
const ROW_LIST: [&str; 2] = [report::ROW, "Open EmulationStation\u{2026}"];
/// The rows About has: no EmulationStation on the Steam Deck (docs/STEAMDECK.md).
fn rows() -> &'static [&'static str] {
    if pocket_ui::device::current().host_owned() { &ROW_LIST[..1] } else { &ROW_LIST }
}
/// Under "Open EmulationStation…" while it is selected.
const ES_HELP: &str = "ROCKNIX's own menu, after a warning. Restart the Pocket to come back.";

/// What the sheet over the About page shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Sheet {
    Writing,
    Saved(Saved),
    Failed(Failure),
}

/// The About page's state: the selected row, the sheet, and where the thread leaves its result.
#[derive(Default)]
pub(crate) struct AboutUi {
    sel: usize,
    sheet: Option<Sheet>,
    done: Arc<Mutex<Option<Result<Saved, Failure>>>>,
    /// The Settings row B goes back to: About, or "Report a problem" when that row opened the page.
    back_to: Option<&'static str>,
}

fn back_row(back_to: Option<&str>) -> usize {
    super::settings_row(back_to.unwrap_or("About"))
}

impl App {
    /// From the Settings list: the first row, no sheet, no result of an earlier visit.
    pub(crate) fn open_about(&mut self) -> Option<SettingsPage> {
        self.about.sel = 0;
        self.about.sheet = None;
        self.about.back_to = None;
        self.about.done.lock().map(|mut d| d.take()).ok();
        Some(SettingsPage::About)
    }

    /// The "Report a problem" row: About carries the result, and B goes back to that row, not to About's.
    pub(crate) fn open_report(&mut self) -> Option<SettingsPage> {
        let page = self.open_about();
        self.about.back_to = Some(report::ROW);
        self.write_report();
        page
    }

    /// The row of the Settings list B returns to from About.
    fn about_back_row(&self) -> usize {
        back_row(self.about.back_to)
    }

    pub(crate) fn about_input(&mut self, b: Button) -> Option<SettingsPage> {
        if let Some(sheet) = &self.about.sheet {
            // A or B closes the result; while writing, B hides the sheet (the zip is still written).
            if b == CANCEL || (b == CONFIRM && *sheet != Sheet::Writing) {
                self.about.sheet = None;
            }
            return Some(SettingsPage::About);
        }
        match b {
            Button::Up => self.about.sel = (self.about.sel + rows().len() - 1) % rows().len(),
            Button::Down => self.about.sel = (self.about.sel + 1) % rows().len(),
            b if b == CONFIRM && self.about.sel == 0 => self.write_report(),
            // "Open EmulationStation…", behind the warning (`app_rescue`).
            b if b == CONFIRM => self.open_leave(pocket_ui::rescue::DialogKind::OpenEs),
            b if b == CANCEL => return Some(SettingsPage::List(self.about_back_row())),
            _ => {}
        }
        Some(SettingsPage::About)
    }

    /// Writes the report on a thread; the result comes back through `done` and a redraw.
    pub(crate) fn write_report(&mut self) {
        // The library's names, for taking them out of the logs: a copy, made once per press.
        let mut games: Vec<GameName> = self.systems.iter().flat_map(|s| s.games.iter().map(|g| GameName { system: s.name.clone(), name: g.name.clone(), path: g.path.clone() })).collect();
        games.extend(self.recents.list.iter().map(|r| GameName { system: r.system.clone(), name: r.name.clone(), path: r.path.clone() }));
        let src = report::Sources::system(&self.data_dir, std::path::Path::new(super::host_bin()));
        let (done, proxy) = (self.about.done.clone(), self.proxy.clone());
        self.about.sheet = Some(Sheet::Writing);
        let spawned = std::thread::Builder::new().name("report".into()).spawn(move || {
            pocket_ui::web::lower_priority();
            let now = pocket_ui::recents::now_secs();
            let result = report::write(&src, &games, now, &report::Stamp::at(now as i64));
            match &result {
                Ok(s) => println!("report: wrote {} ({} bytes)", s.path.display(), s.bytes),
                Err(e) => println!("report: not written: {e:?}"),
            }
            if let Ok(mut d) = done.lock() {
                *d = Some(result);
            }
            let _ = proxy.send_event(UserEvent::SettingsValues);
        });
        if let Err(e) = spawned {
            self.about.sheet = Some(Sheet::Failed(Failure::Other(e.to_string())));
        }
    }

    /// The rows under the About page's facts, from `y`, the explanation of the selected one, the hints, and the sheet.
    pub(crate) fn paint_about_rows(&mut self, c: &mut Canvas, y: i32, row: i32, font: u32) {
        // A result that came in since the last frame.
        if let Some(result) = self.about.done.lock().ok().and_then(|mut d| d.take()) {
            self.about.sheet = Some(match result {
                Ok(s) => Sheet::Saved(s),
                Err(f) => Sheet::Failed(f),
            });
        }
        let (pad, right) = (NU_PAD, c.w - NU_PAD);
        for (i, label) in rows().iter().enumerate() {
            let ry = y + i as i32 * row;
            app_rescue::paint_row(c, &mut self.text, (pad, ry, right - pad, row), font, label, i == self.about.sel);
            self.hits.row(Area::new(pad, ry, right - pad, row), i);
        }
        self.hits.list(self.about.sel, rows().len(), 0.0, row as f32);
        // What the selected row does, in the room above the hints (cut with "…" when larger text needs more).
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let step = pocket_ui::type_scale::line(NU_SMALL, 12);
        let mut ly = y + rows().len() as i32 * row + 20 + pocket_ui::type_scale::px(NU_SMALL) as i32 / 2;
        let hints_top = c.h - pad - NU_PILL - 8;
        // The last line's baseline may sit up to a line above the hints' top, as on Sleep & power.
        let room = ((hints_top - ly) / step + 1).max(1) as usize;
        let help = if self.about.sel == 0 { report::ROW_HELP } else { ES_HELP };
        for line in self.text.wrap_at_most(Face::Body, NU_SMALL, help, width, room) {
            self.text.draw(c, Face::Body, NU_SMALL, left, ly, pal().quiet, &line);
            ly += step;
        }
        // The handheld in the room left above the hints (`ui_art`; the Mini V2 where the device isn't known): twice
        // its pixels where that fits, once where only that does, none on a page with no room left.
        let model = pocket_ui::device::current().model.unwrap_or(pocket_ui::device::Model::MiniV2);
        let art = pocket_ui::ui_art::Art::Device(model);
        // `ly` is the baseline a next line would have: the last one's letters end a step above it.
        let below = ly - step + pocket_ui::type_scale::px(NU_SMALL) as i32 / 3 + 8;
        let free = hints_top - 8 - below;
        if let Some(k) = [2, 1].into_iter().find(|k| pocket_ui::ui_art::size(art, *k).1 <= free) {
            let (aw, ah) = pocket_ui::ui_art::size(art, k);
            pocket_ui::ui_art::paint(c, art, (c.w - aw) / 2, below + (free - ah) / 2, k);
        }
        self.nu_hints(c, &[("A", "Choose"), ("B", "Back")]);
        if let Some(sheet) = self.about.sheet.clone() {
            self.paint_report_sheet(c, &sheet);
        }
    }

    /// The sheet over the dimmed page, in the manner of the dialog before EmulationStation (`app_rescue::paint_dialog`).
    fn paint_report_sheet(&mut self, c: &mut Canvas, sheet: &Sheet) {
        self.hits.clear();
        c.shade_rect(0, 0, c.w, c.h, pal().void, 215);
        pocket_ui::text::boxes::covered();
        let (w, row, font, small) = (980, pocket_ui::type_scale::row(66, 36), 36u32, NU_SMALL);
        let x = (c.w - w) / 2;
        let inner = w - 2 * 48;
        let (title, paragraphs): (&str, Vec<(String, u32)>) = match sheet {
            Sheet::Writing => (report::WRITING, vec![]),
            Sheet::Saved(s) => (report::SAVED_TITLE, vec![(s.name.clone(), pal().chalk), (report::WHERE.into(), pal().text), (report::ATTACH.into(), pal().text), (report::LEFT_OUT.into(), pal().quiet)]),
            Sheet::Failed(f) => (report::FAILED_TITLE, vec![(f.message(), pal().text)]),
        };
        let line_h = pocket_ui::type_scale::line(small, 12);
        // The sheet fits between the top and the hints; with larger text the last paragraph gives way first.
        let max_h = c.h - NU_PILL - 2 * NU_PAD - 24;
        let fixed = 48 + 58 + 26 + 16 + row + 36;
        let mut left_h = max_h - fixed;
        let mut wrapped: Vec<(Vec<String>, u32)> = Vec::new();
        for (p, colour) in &paragraphs {
            let fit = ((left_h - 14) / line_h).max(0) as usize;
            if fit == 0 {
                break;
            }
            let lines = self.text.wrap_at_most(Face::Body, small, p, inner, fit);
            left_h -= lines.len() as i32 * line_h + 14;
            wrapped.push((lines, *colour));
        }
        let body_h: i32 = wrapped.iter().map(|(l, _)| l.len() as i32 * line_h + 14).sum();
        let h = fixed + body_h;
        let y = ((c.h - NU_PILL - NU_PAD - h) / 2).max(NU_PAD);
        c.rect(x, y, w, h, pal().void);
        c.frame(x, y, w, h, 1, if pal().t > 0 { pal().pick(VOID, 0xd6d0c4) } else { pal().pill });
        let left = x + 48;
        let mut ty = y + 48 + 46;
        let title = self.text.fit(Face::Title, 50, title, inner);
        self.text.draw(c, Face::Title, 50, left, ty, pal().chalk, &title);
        ty += 26;
        for (lines, colour) in &wrapped {
            for line in lines {
                ty += line_h;
                self.text.draw(c, Face::Body, small, left, ty, *colour, line);
            }
            ty += 14;
        }
        ty += 16;
        let hints: &[(&str, &str)] = if *sheet == Sheet::Writing {
            &[("B", "Hide")]
        } else {
            app_rescue::paint_row(c, &mut self.text, (left, ty, inner, row), font, "OK", true);
            self.hits.row(Area::new(left, ty, inner, row), 0);
            self.hits.list(0, 1, 0.0, row as f32);
            &[("A", "OK"), ("B", "Back")]
        };
        // The page's own hints go: only the sheet's are left to read and to touch.
        c.rect(0, c.h - NU_PAD - NU_PILL - 12, c.w, NU_PAD + NU_PILL + 12, pal().void);
        self.nu_hints(c, hints);
    }

    /// `--snapshot report[:writing|:saved|:full|:readonly|:es]`: Settings → System → About with "Report a problem"
    /// selected (`es`: Open EmulationStation… selected), and the sheet while writing, after it was saved, or with the
    /// card full or read-only. Nothing is written.
    pub(crate) fn report_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("report") else { return false };
        if !(rest.is_empty() || rest.starts_with(':')) {
            return false;
        }
        self.settings_page = Some(SettingsPage::About);
        self.about = AboutUi::default();
        let name = report::file_name(&report::Stamp { year: 2026, month: 10, day: 26, hour: 16, min: 5, sec: 7 });
        self.about.sheet = match rest.trim_start_matches(':') {
            "writing" => Some(Sheet::Writing),
            "saved" => Some(Sheet::Saved(Saved { path: std::path::Path::new(report::DEFAULT_DIR).join(&name), name, bytes: 180_000 })),
            "full" => Some(Sheet::Failed(Failure::Full)),
            "readonly" => Some(Sheet::Failed(Failure::ReadOnly)),
            "es" => {
                self.about.sel = 1;
                None
            }
            _ => None,
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_goes_back_to_the_row_that_opened_the_page() {
        let (about, report_row) = (back_row(None), back_row(Some(report::ROW)));
        assert!(matches!(super::super::SETTINGS[about].1, super::super::Opens::About));
        assert!(matches!(super::super::SETTINGS[report_row].1, super::super::Opens::Report), "Report a problem, not About");
        assert_ne!(about, report_row);
    }
}
