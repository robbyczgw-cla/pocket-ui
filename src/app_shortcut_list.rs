//! Part of the pocket-ui binary: the list of a RetroArch game's HOME shortcuts in its pause menu (Options → Runs on →
//! Shortcuts, `pocket_ui::shortcut_list`). What it lists is worked out once when it opens (the game's Options →
//! Shortcuts, RetroArch's Rewind in system.cfg), so painting reads no file. Up and Down move, B or HOME go back to Runs
//! on; nothing on it changes anything.
//!
//! pocket-core's games have their list on Options → Shortcuts, with a switch per row.

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::pause_page::{self, OptRow, OptionsPage};
use pocket_ui::shortcut_list::{self, Game, Line};

use super::{App, Page, CANCEL};

impl App {
    /// The Runs on page's Shortcuts row was chosen: the list, for the running game.
    pub(crate) fn open_shortcut_list(&mut self) {
        let Some(run) = self.running.as_ref() else { return };
        let values = run.options.open(&self.data_dir, &run.system, &run.path).effective();
        let on = pocket_ui::ra_hotkeys::enabled(&values, self.kids_live(), self.big_on());
        let rewind = ra_rewind_on(&run.system, run.path.file_name().and_then(|f| f.to_str()));
        let lines = shortcut_list::retroarch(Game { on, rewind, kids: self.kids_live(), easy: self.big_on() });
        self.show_shortcut_list(lines);
    }

    pub(crate) fn show_shortcut_list(&mut self, lines: Vec<Line>) {
        if let Some(m) = self.menu.as_mut() {
            m.shortcut_list = lines;
            m.page = Page::ShortcutList { sel: 0 };
        }
    }

    pub(crate) fn shortcut_list_input(&mut self, b: Button) {
        let Some(m) = self.menu.as_mut() else { return };
        let Page::ShortcutList { sel } = m.page else { return };
        let n = m.shortcut_list.len().max(1);
        match b {
            Button::Up | Button::Down => m.page = Page::ShortcutList { sel: pocket_ui::wrap_step(sel, n, b == Button::Up) },
            // Back to Runs on, on its Shortcuts row (the page kept its state while the list was open).
            b if b == CANCEL || b == Button::Mode => m.page = Page::EmuChoice,
            _ => {}
        }
    }

    pub(crate) fn paint_shortcut_list(&mut self, c: &mut Canvas) {
        let Some(m) = self.menu.as_ref() else { return };
        let Page::ShortcutList { sel } = m.page else { return };
        // Over the game like Runs on, which it opens from.
        c.fade(150);
        let rows: Vec<OptRow> = m.shortcut_list.iter().map(|l| OptRow { fixed: true, ..OptRow::new(l.what, l.keys, l.desc) }).collect();
        let accent = self.running.as_ref().map_or(pocket_ui::palette::DUST, |r| super::stripe(&r.system));
        let corner = self.status_corner();
        let page = OptionsPage {
            heading: Some(shortcut_list::TITLE),
            counts: None,
            rows: &rows,
            sel,
            footer: None,
            description_above_hints: false,
            explanation: true,
            accent,
            corner: &corner,
            picture: None,
        };
        pause_page::paint_options(c, &mut self.text, &mut self.hits, &page);
    }
}

/// RetroArch's Rewind for this game, as setsettings.sh will read it (Runs on → Rewind; off for the systems it has none).
fn ra_rewind_on(system: &str, file: Option<&str>) -> bool {
    use pocket_ui::emu_choice;
    let cfg = emu_choice::Cfg::read(std::path::Path::new(pocket_ui::system_cfg::PATH));
    emu_choice::RA_SETTINGS
        .iter()
        .find(|s| s.key == "rewind")
        .is_some_and(|s| s.applies(system) && emu_choice::effective_value(&cfg, system, file, s) == "1")
}
