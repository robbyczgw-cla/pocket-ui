//! Part of the pocket-ui binary: the pause menu's Zoom page (`pocket_ui::zoom`). It shows a part of the frozen picture of
//! the game, 2x or 3x as large, over the whole screen: the D-pad moves the part, A switches 2x and 3x, B comes back.
//!
//! The picture is the one behind the menu (`QuickMenu::frame`, pocket-core's `shot`), already in memory: opening the
//! page asks the game for nothing and reads no file. Until that picture has come (a game still starting) the row
//! refuses, as a Load without a state does.
//!
//! Where it is reached: Pause → Options → Zoom always; a row on the pause menu's main page only while Easy to read is on
//! or Settings → Sound & feel → Zoom in the pause menu is (`zoom_row_on`); and, off by default, HOME + X in the game
//! (Options → Shortcuts → Zoom, `zoom_key`), which opens the menu straight on this page. B goes back to where it came from.

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::text::Face;
use pocket_ui::zoom::{Action, Zoom};

use super::{App, MainItem, Page, CANCEL, CONFIRM, DARK, INK, NU_BTN_PAD, NU_PAD, NU_SMALL};

/// The level the page opens at: the one used last in this run.
pub(crate) const FIRST_LEVEL: u32 = 2;

/// Where B on the Zoom page goes: the pause menu's main page, its Options list (on the row Zoom), or on into the game
/// (HOME + X opened the menu for Zoom alone).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Back {
    Main,
    Options { sel: usize },
    Game,
}

impl App {
    /// Whether the pause menu lists Zoom on its main page: Easy to read is on (`features::reading_on`) or the player
    /// switched on Settings → Sound & feel → Zoom in the pause menu.
    pub(crate) fn zoom_row_on(&self) -> bool {
        self.zoom_in_pause || pocket_ui::features::reading_on(pocket_ui::type_scale::get(), pocket_ui::motion::level())
    }

    /// A on "Zoom" in the pause menu (or in Options), coming back to `back`.
    pub(crate) fn open_zoom(&mut self, back: Back) {
        let Some(menu) = self.menu.as_mut() else { return };
        if menu.frame.is_none() {
            pocket_ui::ui_sound::refuse();
            return;
        }
        menu.zoom_back = back;
        // A handheld's dialogue boxes are at the bottom of the picture: it opens there.
        let low = self.running.as_ref().is_some_and(|r| pocket_ui::zoom::starts_low(&r.system));
        menu.zoom = Some(if low { Zoom::low(self.zoom_level) } else { Zoom::new(self.zoom_level) });
        menu.page = Page::Zoom;
    }

    /// HOME + X in a pocket-core game (Options → Shortcuts → Zoom on): the pause menu opens straight on Zoom, B goes back
    /// into the game. Returns whether the press was ours. The picture comes a frame or two after the menu (`app_menu_open`),
    /// so until then the page is up and black (`menu_shot_arrived` opens it); no main page flashes by.
    pub(crate) fn zoom_key(&mut self, button: Button) -> bool {
        use pocket_ui::button_layout;
        // Positional like HOME + Y and HOME + A: the top face button, whatever the layout calls it.
        if !self.home_down || self.menu.is_some() || button_layout::get().role(Button::North) != button {
            return false;
        }
        if self.kids_live() || self.big_on() {
            return false;
        }
        let Some(run) = self.running.as_ref().filter(|r| r.hosted()) else { return false };
        if run.options.open(&self.data_dir, &run.system, &run.path).value_of("hk_zoom") != "on" {
            return false;
        }
        // Not a HOME tap: the release must not open the menu on its own.
        self.home_tap = false;
        self.open_menu();
        let Some(menu) = self.menu.as_mut() else { return true };
        if self.menu_shot.is_asked() {
            menu.zoom_back = Back::Game;
            menu.zoom_pending = true;
            menu.zoom = Some(Zoom::new(self.zoom_level));
            menu.page = Page::Zoom;
        }
        true
    }

    /// The picture behind the menu came while the page waited for it (`zoom_key`): the page proper, opened low for a
    /// handheld's games. Without a picture (`None`) the menu is the ordinary one.
    pub(crate) fn zoom_picture_came(&mut self, came: bool) {
        let Some(menu) = self.menu.as_mut() else { return };
        if !std::mem::take(&mut menu.zoom_pending) {
            return;
        }
        if came {
            self.open_zoom(Back::Game);
        } else {
            menu.zoom = None;
            menu.page = Page::Main;
        }
    }

    /// A button on the page.
    pub(crate) fn zoom_input(&mut self, b: Button) {
        let Some(menu) = self.menu.as_mut() else { return };
        let Some(zoom) = menu.zoom.as_mut() else {
            menu.page = Page::Main;
            return;
        };
        match zoom.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Moved => {
                self.zoom_level = zoom.level();
                self.request_redraw();
            }
            Action::Close => {
                menu.zoom = None;
                menu.zoom_pending = false;
                match std::mem::replace(&mut menu.zoom_back, Back::Main) {
                    Back::Main => {
                        menu.page = Page::Main;
                        menu.sel = menu.items().iter().position(|i| *i == MainItem::Zoom).unwrap_or(0);
                    }
                    Back::Options { sel } => menu.page = Page::Options { sel },
                    Back::Game => self.resume_game(None),
                }
            }
        }
    }

    /// The part of the picture over the whole screen, "2x" in a pill at the top left and the hints at the bottom right.
    pub(crate) fn paint_zoom(&mut self, c: &mut Canvas) {
        let Some(menu) = self.menu.as_ref() else { return };
        let (Some(zoom), Some(frame)) = (menu.zoom, menu.frame.as_ref()) else {
            // HOME + X: the picture is still on its way.
            c.clear(0x000000);
            return;
        };
        // The game's part, `level` times as large as the screen shows it; a game smaller than that is shown whole, centred.
        let part = zoom.part(frame.w, frame.h, menu.game_rect);
        let (dw, dh) = ((part.2 as u64 * c.w as u64 * zoom.level() as u64 / frame.w as u64) as i32, (part.3 as u64 * c.h as u64 * zoom.level() as u64 / frame.h as u64) as i32);
        c.clear(0x000000);
        c.blit_region(frame, part, (c.w - dw) / 2, (c.h - dh) / 2, dw, dh);
        let label = zoom.label();
        let (pad, pill) = (NU_PAD, pocket_ui::kit::PILL);
        let w = self.text.width(Face::Body, NU_SMALL, &label) + 2 * NU_BTN_PAD;
        c.round_rect(pad, pad, w, pill, pill / 2, DARK.select);
        self.text.draw(c, Face::Body, NU_SMALL, pad + NU_BTN_PAD, pad + (pill - 20) / 2 + pocket_ui::type_scale::mid(NU_SMALL), INK, &label);
        self.nu_hints(c, &[("D-pad", "Move"), ("A", "2x / 3x"), ("B", "Back")]);
    }

    /// `--snapshot zoom[:<2|3>][:low][:<left|right|top|bottom|step>...]`: the page over the background picture given.
    pub(crate) fn zoom_snapshot(&mut self, state: &str) {
        let words: Vec<&str> = state.split(':').skip(1).collect();
        let level = words.iter().find_map(|w| w.parse().ok()).unwrap_or(FIRST_LEVEL);
        let mut zoom = if words.contains(&"low") { Zoom::low(level) } else { Zoom::new(level) };
        // Moves from the middle: "left", "right", "top" and "bottom" go to those edges, "step" is one press right.
        for part in words {
            let presses: &[Button] = match part {
                "left" => &[Button::Left; 8],
                "right" => &[Button::Right; 8],
                "top" => &[Button::Up; 8],
                "bottom" => &[Button::Down; 8],
                "step" => &[Button::Right],
                _ => &[],
            };
            for b in presses {
                zoom.press(*b, CONFIRM, CANCEL);
            }
        }
        let (w, h) = super::snapshot_dims();
        let Some(menu) = self.menu.as_mut() else { return };
        menu.saved = None;
        // No picture given: the cards' sample scene.
        if menu.frame.is_none() {
            let (frame, rect) = pocket_ui::feature_previews::game_shot(w, h);
            menu.frame = Some(frame);
            menu.game_rect = Some(rect);
        }
        menu.zoom = Some(zoom);
        menu.page = Page::Zoom;
    }
}
