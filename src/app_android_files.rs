//! Part of the pocket-ui binary: Android's "Read your games" (the Handheld view, android/handheld), a sheet of pocket-ui's
//! own instead of Android's AlertDialog (docs/user/android-index.md.inc "Your games"). The view reads the games where they are
//! (ROMs/<system>), which Android allows only with All files access (storage read before Android 11).
//!
//! While Android doesn't allow it, the sheet stands over whatever page is up (the first start's, the shelf): what the
//! access is for, what pocket-ui does with it, and the other way. A opens Android's page for the access (that page is
//! Android's own and can't be ours), X the Mobile view, where a folder is picked instead. Each time the view comes back in
//! front it looks again (`android_files_check`): access given, the sheet is gone and the folders are read. Every other
//! button but volume, power and HOME waits behind it, as the dialog did (it couldn't be cancelled either). On a touch
//! screen the two hint pills are the ways (a tap is that button's press); nothing else of the page can be touched.

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::type_scale;

use super::{App, CONFIRM, NU_PAD};

pub(crate) const TITLE: &str = "Read your games";
/// What it is for, what pocket-ui does there, and the way without it (three paragraphs, each a line of the tables).
pub(crate) const WHAT: &str = "The Handheld view reads your games straight from the ROMs folder (for example ROMs/gba, ROMs/snes). For that, Android asks for access to all files. pocket-ui only reads games and pictures there.";
pub(crate) const SAVES: &str = "Your saves live in the app's own folder; if the Mobile view has a games folder, a copy of them goes into its folder \"pocket-ui saves\" when a game ends.";
pub(crate) const WITHOUT: &str = "Without this access, use the Mobile view: there you pick a folder.";
pub(crate) const ALLOW: &str = "Allow access";
pub(crate) const MOBILE: &str = "Mobile view";

/// Whether Android lets the view read the files in the games folders now (one quick call into Java).
fn readable() -> bool {
    #[cfg(all(target_os = "android", feature = "handheld"))]
    {
        pocket_ui::platform::android::string("filesAccess") == "true"
    }
    #[cfg(not(all(target_os = "android", feature = "handheld")))]
    {
        true
    }
}

/// The buttons the sheet keeps from the page behind it: everything a page would act on. Volume, power and HOME (the
/// quick settings) keep their own ways.
pub(crate) fn takes(b: Button) -> bool {
    !matches!(b, Button::VolUp | Button::VolDown | Button::Power | Button::Mode)
}

impl App {
    /// At start and each time the view comes back in front: the sheet is up exactly while Android doesn't allow the access.
    pub(crate) fn android_files_check(&mut self) {
        let ask = pocket_ui::device::android_profile() && !super::snapshot_run() && !readable();
        if ask != self.android_files_ask {
            println!("files access: {}", if ask { "not allowed, asking" } else { "allowed" });
            self.android_files_ask = ask;
            self.request_redraw();
        }
    }

    /// A: Android's page for the access; X: the Mobile view. Every other button the sheet takes waits.
    pub(crate) fn android_files_press(&mut self, b: Button) {
        let call = |name: &str, args: &[&str]| {
            #[cfg(all(target_os = "android", feature = "handheld"))]
            if let Err(e) = pocket_ui::platform::android::strings(name, args) {
                eprintln!("files access: {name}: {e}");
            }
            #[cfg(not(all(target_os = "android", feature = "handheld")))]
            let _ = (name, args);
        };
        if b == CONFIRM {
            self.play_sound(pocket_ui::ui_sound::Sound::Confirm);
            call("askFilesAccess", &[]);
        } else if b == Button::North {
            self.play_sound(pocket_ui::ui_sound::Sound::Confirm);
            call("setView", &["mobile"]);
        }
    }

    /// The sheet over the page, and its hints (as the Battery page's "Reset the battery reading?").
    pub(crate) fn paint_android_files(&mut self, c: &mut Canvas) {
        // Only the sheet can be touched: the page's targets go, its two ways are the hint pills (a tap is that button's
        // press, `tap_button`), so a device without a pad gets out too (Sol P1-1, 26.11.).
        self.hits.clear();
        c.shade_rect(0, 0, c.w, c.h, pal().void, 225);
        pocket_ui::text::boxes::covered();
        let w = type_scale::wide(900, 30).min(c.w - 2 * NU_PAD - 96);
        let small = 30u32;
        let step = type_scale::line(small, 10);
        let paragraphs: Vec<Vec<String>> = [WHAT, SAVES, WITHOUT].into_iter().map(|p| self.text.wrap_prose(Face::Body, small, p, w)).collect();
        let title_h = type_scale::px(50) as i32;
        let body_h: i32 = paragraphs.iter().map(|p| p.len() as i32 * step).sum::<i32>() + (paragraphs.len() as i32 - 1) * 16;
        let h = title_h + 36 + body_h;
        let top = pocket_ui::kit::header_bottom(c.h);
        let bottom = pocket_ui::kit::hint_bar_top(c.h) - 24;
        let (x, y) = ((c.w - w) / 2, (top + (bottom - top - h) / 2).max(top + 48));
        pocket_ui::kit::sheet(c, x - 48, y - 48, w + 96, h + 96);
        let head = self.text.fit(Face::Title, 50, TITLE, w);
        self.text.draw(c, Face::Title, 50, x, y + title_h, pal().chalk, &head);
        let mut by = y + title_h + 36;
        for (i, lines) in paragraphs.iter().enumerate() {
            for l in lines {
                by += step;
                self.text.draw(c, Face::Body, small, x, by - 10, if i == 0 { pal().text } else { pal().quiet }, l);
            }
            by += 16;
        }
        // The page's own hints go, as under a dialog (`app_rescue::paint_dialog`): only the sheet's are left to read and tap.
        c.rect(0, pocket_ui::kit::hint_bar_top(c.h) - 12, c.w, c.h - pocket_ui::kit::hint_bar_top(c.h) + 12, pal().void);
        self.nu_hints(c, &[("A", ALLOW), ("X", MOBILE)]);
    }

    /// `--snapshot android-files` (any page with `:files-ask`): the sheet as Android shows it without the access.
    pub(crate) fn android_files_snapshot(&mut self, state: &str) {
        if state == "android-files" || state.split(':').any(|p| p == "files-ask") {
            self.android_files_ask = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The words the Java dialog had (13.11.2026 rollout) stay true on the sheet: what the app writes (a copy of the
    /// saves in "pocket-ui saves" when the Mobile view has a games folder, as `save_copy` does), in every language; and
    /// Android's dialog is gone from the Handheld view, its A asks Android through `askFilesAccess`.
    #[test]
    fn read_your_games_is_our_sheet_and_tells_the_truth_about_saves() {
        for lang in pocket_ui::i18n::Lang::ALL {
            let saves = pocket_ui::i18n::tr_in(lang, SAVES);
            assert!(saves.contains("pocket-ui saves"), "{lang:?} names the copy's folder: {saves}");
            for lie in ["saves stay in the app's own folder", "Spielstände bleiben im App-Ordner", "quedan en la carpeta de la app"] {
                assert!(!saves.contains(lie), "{lang:?}: {lie}");
            }
        }
        let java = include_str!("../android/java/xyz/pocketui/app/HandheldActivity.java");
        assert!(!java.contains("AlertDialog"), "no Android dialog in the Handheld view");
        assert!(java.contains("public String askFilesAccess()"), "A on the sheet asks Android");
        assert!(java.contains("ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION"));
    }

    #[test]
    fn the_sheet_keeps_the_pages_buttons_and_leaves_volume_power_and_home() {
        for b in [Button::East, Button::South, Button::North, Button::West, Button::Up, Button::Down, Button::Start, Button::Select, Button::L1, Button::R1] {
            assert!(takes(b), "{b:?} waits behind the sheet");
        }
        for b in [Button::VolUp, Button::VolDown, Button::Power, Button::Mode] {
            assert!(!takes(b), "{b:?} keeps its own way");
        }
    }
}
