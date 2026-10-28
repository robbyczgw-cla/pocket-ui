//! Part of the pocket-ui binary: the pause menu's Disc row for multi-disc games on pocket-ui's own host
//! (`pocket_ui::disc`).
//!
//! When the menu opens, the row is built from what pocket-core wrote about the game's discs (`disc::FILE`, one small
//! read on tmpfs); a game with one disc, a core without disc control or no file at all leave the menu as it was.
//! Left/Right choose a disc, A sends `disc <n>` on pocket-core's FIFO and goes back to the game, which then sees the
//! tray open and close. pocket-core shows "Disc 2 of 3" over the game and writes the file again.

use pocket_ui::control;
use pocket_ui::disc::{DiscRow, Discs};

use super::{App, MainItem, Running};

impl App {
    /// The Disc row for the game the menu opens over: pocket-core games with two discs or more.
    pub(crate) fn disc_row_at_open(run: &Running) -> Option<DiscRow> {
        if !run.hosted() {
            return None;
        }
        Discs::read(&run.path).and_then(DiscRow::new)
    }

    /// A on the Disc row: the disc goes in and the game goes on. When pocket-core can't be told, the menu stays and
    /// says no with its sound.
    pub(crate) fn put_in_disc(&mut self) {
        let Some(cmd) = self.menu.as_ref().and_then(|m| m.disc).map(|d| d.command()) else { return };
        if control::pocket_core(&cmd) {
            self.resume_game(None);
        } else {
            pocket_ui::ui_sound::refuse();
        }
    }

    /// Snapshots: "…:disc=<in>/<count>" gives the menu a Disc row (`?` for the disc in the drive: the core named none)
    /// over a PlayStation game, selected.
    pub(crate) fn disc_snapshot(&mut self, state: &str) {
        let Some(spec) = state.split(':').find_map(|p| p.strip_prefix("disc=")) else { return };
        let (at, count) = spec.split_once('/').unwrap_or((spec, "3"));
        let count: u32 = count.parse().unwrap_or(3);
        let discs = Discs { current: at.parse::<u32>().ok().filter(|n| (1..=count).contains(n)).map(|n| n - 1), count };
        if let Some(run) = self.running.as_mut() {
            run.system = "psx".into();
            run.name = "Final Fantasy VII".into();
            run.path = "/storage/roms/psx/Final Fantasy VII/Final Fantasy VII.m3u".into();
        }
        let Some(menu) = self.menu.as_mut() else { return };
        menu.disc = DiscRow::new(discs);
        menu.sel = menu.items().iter().position(|i| *i == MainItem::Disc).unwrap_or(menu.sel);
    }
}
