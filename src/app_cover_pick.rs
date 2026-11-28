//! Part of the pocket-ui binary: a game's own cover, chosen on the card in Android's handheld view (24.11.2026, a player's
//! wish). "Choose cover…" opens Android's photo picker (`HandheldActivity.pickCover`, no storage permission); the picture
//! comes back as a file, is checked and scaled on a thread (`covers::custom::from_picture`) and kept where the web
//! companion keeps a chosen cover (`covers::custom::apply`: `<data>/covers/.custom`, a copy of the one it replaces).
//! "Reset to automatic" brings back the usual cover. Every outcome says something on the card, a cancel included.
//!
//! Linux has no picker on the device: there the web companion's "Choose cover…" does it (docs/user/web-companion.md).

use std::path::{Path, PathBuf};
use std::time::Instant;

use pocket_ui::covers::custom::{self, Refused};
use pocket_ui::game_card_model::{CuratorRow, RowKind};

use super::{App, UserEvent};

pub(crate) const CHOOSE_TITLE: &str = "Choose cover…";
pub(crate) const CHOOSE_LINE: &str = "Pick a picture from this device";
pub(crate) const RESET_TITLE: &str = "Reset to automatic";
pub(crate) const RESET_LINE: &str = "Back to the automatic cover";

pub(crate) const CHANGED: &str = "Cover updated";
pub(crate) const RESET_DONE: &str = "Cover reset";
pub(crate) const READING: &str = "Loading picture…";
pub(crate) const CANCELLED: &str = "No picture chosen. Your cover wasn't changed.";
pub(crate) const UNREADABLE: &str = "Couldn't open that picture. Your cover wasn't changed.";
pub(crate) const NO_PICKER: &str = "No picture picker found on this device.";
pub(crate) const RUNNING: &str = "A game is running. Quit it first to change the cover.";
pub(crate) const GONE: &str = "That game isn't in your library anymore.";
pub(crate) const NOT_SAVED: &str = "Could not save: is the SD card full?";

/// Why a cover was not changed (`App::set_custom_cover`); the card and the web companion each say it in their words.
#[derive(Debug)]
pub(crate) enum NotChanged {
    Running,
    Gone,
    Io(std::io::Error),
}

impl NotChanged {
    fn on_card(&self) -> String {
        match self {
            NotChanged::Running => RUNNING.into(),
            NotChanged::Gone => GONE.into(),
            NotChanged::Io(_) => NOT_SAVED.into(),
        }
    }
}

/// Where the picker's copy of the chosen picture waits until it is read (the app's cache folder on Android).
fn picked_file() -> PathBuf {
    std::env::temp_dir().join("pocket-ui-cover-pick")
}

/// Android: the photo picker, which copies the chosen picture (up to the limit) to `dest` and answers through
/// `UserEvent::CoverPicked`.
#[cfg(all(target_os = "android", feature = "handheld"))]
fn open_picker(dest: &Path) -> Result<(), String> {
    let said = pocket_ui::platform::android::strings("pickCover", &[&dest.to_string_lossy(), &custom::MAX_BYTES.to_string()])?;
    if said.is_empty() { Ok(()) } else { Err(said) }
}

#[cfg(not(all(target_os = "android", feature = "handheld")))]
fn open_picker(_dest: &Path) -> Result<(), String> {
    Err("no picker outside Android".into())
}

/// What the picker's answer says on the card: None when a picture is there to be read.
pub(crate) fn picker_says(outcome: &str) -> Option<&'static str> {
    match outcome {
        "picked" => None,
        "cancelled" => Some(CANCELLED),
        "too-big" => Some(Refused::TooBig.message()),
        "no-picker" => Some(NO_PICKER),
        _ => Some(UNREADABLE),
    }
}

/// The picked file, read and turned into the cover's PNG (on a thread: a 12-megapixel photo takes a moment).
pub(crate) fn read_picked(file: &Path) -> Result<Vec<u8>, Refused> {
    use std::io::Read;
    let mut bytes = Vec::new();
    let read = std::fs::File::open(file).and_then(|f| f.take(custom::MAX_BYTES + 1).read_to_end(&mut bytes));
    let _ = std::fs::remove_file(file);
    read.map_err(|_| Refused::NotAPicture)?;
    custom::from_picture(&bytes)
}

impl App {
    /// The card's "Choose cover…" and, with a chosen cover, "Reset to automatic": Android's handheld view only (Linux
    /// chooses in the web companion). Before "Delete from the card" and "Hide from the shelf".
    pub(crate) fn card_cover_rows(&self, system: &str, path: &Path) -> Vec<CuratorRow> {
        if !pocket_ui::device::android_profile() {
            return Vec::new();
        }
        let mut rows = vec![CuratorRow { kind: RowKind::ChooseCover, title: CHOOSE_TITLE.into(), line: CHOOSE_LINE.into() }];
        if custom::existing(&self.data_dir, system, path).is_some() {
            rows.push(CuratorRow { kind: RowKind::ResetCover, title: RESET_TITLE.into(), line: RESET_LINE.into() });
        }
        rows
    }

    /// A on "Choose cover…": Android's picker opens over the view; its answer comes as `UserEvent::CoverPicked`.
    pub(crate) fn card_choose_cover(&mut self) {
        let Some((system, path)) = self.card.as_ref().map(|o| (o.system.clone(), o.path.clone())) else { return };
        if self.running.is_some() || pocket_ui::covers::game_on() {
            self.warn(RUNNING);
            return;
        }
        let dest = picked_file();
        let _ = std::fs::remove_file(&dest);
        match open_picker(&dest) {
            Ok(()) => {
                println!("cover: picker opened for {}", path.display());
                self.cover_pick = Some((system, path));
            }
            Err(e) => {
                eprintln!("cover: no picker: {e}");
                self.warn(NO_PICKER);
            }
        }
    }

    /// Android's picker answered: "picked" (the file is there), "cancelled", "too-big", "no-picker" or "failed".
    pub(crate) fn cover_picked(&mut self, outcome: String) {
        let Some((system, rom)) = self.cover_pick.take() else { return };
        println!("cover: picker said {outcome}");
        if let Some(say) = picker_says(&outcome) {
            let _ = std::fs::remove_file(picked_file());
            if outcome == "cancelled" {
                self.toast = Some((say.to_string(), Instant::now()));
                self.request_redraw();
            } else {
                self.warn(say);
            }
            return;
        }
        self.toast = Some((READING.to_string(), Instant::now()));
        self.request_redraw();
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("cover-pick".into()).spawn(move || {
            let png = read_picked(&picked_file());
            let _ = proxy.send_event(UserEvent::CoverReady { system, rom, png });
        });
    }

    /// The chosen picture, checked and scaled: it is the game's cover now, or the card says why not.
    pub(crate) fn cover_ready(&mut self, system: String, rom: PathBuf, png: Result<Vec<u8>, Refused>) {
        let said = match png {
            Err(why) => {
                eprintln!("cover: refused: {why:?}");
                Err(why.message().to_string())
            }
            Ok(png) => self.set_custom_cover(&system, &rom, Some(&png)).map(|()| CHANGED).map_err(|e| e.on_card()),
        };
        self.cover_said(said);
    }

    /// A on "Reset to automatic".
    pub(crate) fn card_reset_cover(&mut self) {
        let Some((system, path)) = self.card.as_ref().map(|o| (o.system.clone(), o.path.clone())) else { return };
        let said = self.set_custom_cover(&system, &path, None).map(|()| RESET_DONE).map_err(|e| e.on_card());
        let done = said.is_ok();
        self.cover_said(said);
        // The reset row is gone: the focus stays on the cover's rows ("Choose cover…"), never on the row after it
        // ("Hide from the shelf"), where a second A would take the game off the shelf.
        if done {
            if let Some(open) = self.card.as_mut() {
                if let Some(i) = open.card.data.rows.iter().position(|r| r.kind == RowKind::ChooseCover) {
                    open.card.focus_row(i);
                }
            }
        }
    }

    fn cover_said(&mut self, said: Result<&str, String>) {
        match said {
            Ok(done) => {
                self.toast = Some((done.to_string(), Instant::now()));
                self.curator_card_refresh();
                self.request_redraw();
            }
            Err(why) => self.warn(why),
        }
    }

    /// A chosen cover (`Some`) or the automatic one again (`None`), for a game of the library: the shelf, the case, the
    /// spine's colour and search follow at once. The web companion's edit (`app_web`) and the card's rows use it.
    pub(crate) fn set_custom_cover(&mut self, system: &str, rom: &Path, png: Option<&[u8]>) -> Result<(), NotChanged> {
        if self.running.is_some() || pocket_ui::covers::game_on() {
            return Err(NotChanged::Running);
        }
        if !self.systems.iter().any(|s| pocket_ui::region_pairs::same_shelf(&s.name, &system) && s.games.iter().any(|g| g.path == rom)) || !rom.exists() {
            return Err(NotChanged::Gone);
        }
        custom::apply(&self.data_dir, system, rom, png).map_err(|e| {
            eprintln!("cover: not saved for {}: {e}", rom.display());
            NotChanged::Io(e)
        })?;
        self.covers.clear();
        self.pictures_changed();
        self.cabinet.cover_replaced(rom);
        self.search_covers_changed();
        self.look_previews = None;
        if png.is_none() {
            self.auto_covers("cover reset");
        }
        Ok(())
    }
}
