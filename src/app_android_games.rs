//! Part of the pocket-ui binary: Android's handheld view's side of `pocket_ui::android_games`. The games the player marked
//! (the Mobile view's list in `launcher.json`, one list for both views) stand on the shelf as the system "Android": a folder of
//! this view's own with one small file per game and a gamelist with their names and covers (`android_games::write_folder`),
//! which the library reads as it reads any system. A on one starts its app, as a home screen starts it (`spawn_game` in
//! `android/handheld/app_android.rs`); the way back is the view coming to the front again, on the same place on the shelf.
//! Settings › Games › Android games lists every app Android lists: A puts one on the shelf or takes it off.
//!
//! Android's list is read on a thread (its first read draws the icons), at the start and whenever the view comes back.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pocket_ui::android_games::{self as ag, Action, Page};
use pocket_ui::gfx::Canvas;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Android games";

#[derive(Default)]
pub(crate) struct AndroidGamesUi {
    /// Android's list at its last answer; None before the first.
    apps: Option<Vec<ag::App>>,
    /// The marks, as `launcher.json` had them at the last read or as this view wrote them.
    marked: BTreeMap<String, String>,
    /// A read of Android's list is on its way.
    reading: bool,
    page: Option<Page>,
}

/// Android's list of apps with a start icon (`AppDrawer.list`). Elsewhere (the Linux build in Android's profile, tests): what
/// `POCKETUI_ANDROID_APPS` says, else none.
fn read_apps() -> Vec<ag::App> {
    #[cfg(all(target_os = "android", feature = "handheld"))]
    return ag::parse(&super::app_android::launcher_apps());
    #[cfg(not(all(target_os = "android", feature = "handheld")))]
    return ag::parse(&std::env::var(ag::APPS_ENV).unwrap_or_default());
}

impl App {
    fn android_games_dir(&self) -> PathBuf {
        self.data_dir.join(ag::FOLDER)
    }

    /// The marks of `launcher.json` and Android's list read again (the list on a thread): at the start, whenever the view
    /// comes back to the front, and when the page opens. Only in Android's profile.
    pub(crate) fn android_games_refresh(&mut self, why: &'static str) {
        if !pocket_ui::device::android_profile() {
            return;
        }
        self.android_games.marked = ag::marked(&std::fs::read_to_string(self.phone_prefs_path()).unwrap_or_default());
        self.android_games_write(why);
        if self.android_games.reading {
            return;
        }
        self.android_games.reading = true;
        let proxy = self.proxy.clone();
        let spawned = std::thread::Builder::new().name("android-apps".into()).spawn(move || {
            let _ = proxy.send_event(UserEvent::AndroidApps(read_apps(), why));
        });
        if spawned.is_err() {
            self.android_games.reading = false;
        }
    }

    /// Android answered: names follow their app's, the shelf loses apps that went, the open page keeps its app selected.
    pub(crate) fn android_apps_event(&mut self, apps: Vec<ag::App>, why: &'static str) {
        self.android_games.reading = false;
        let mut marked = ag::marked(&std::fs::read_to_string(self.phone_prefs_path()).unwrap_or_default());
        if ag::follow_names(&mut marked, &apps) {
            self.android_games_save(&marked);
        }
        self.android_games.marked = marked;
        if let Some(page) = self.android_games.page.as_mut() {
            let at = page.selected().map(|a| a.package.clone());
            page.sel = at.and_then(|p| apps.iter().position(|a| a.package == p)).unwrap_or(page.sel).min(apps.len().saturating_sub(1));
            page.apps = Some(apps.clone());
        }
        self.android_games.apps = Some(apps);
        self.android_games_write(why);
        self.request_redraw();
    }

    /// The folder after the marks or Android's list changed: the covers of its games (Android draws them), its files and
    /// gamelist; the library is read again when anything in it changed.
    fn android_games_write(&mut self, why: &'static str) {
        let games = ag::shelf(&self.android_games.marked, self.android_games.apps.as_deref());
        let dir = self.android_games_dir();
        #[cfg(all(target_os = "android", feature = "handheld"))]
        if self.android_games.apps.is_some() {
            for (package, _) in &games {
                super::app_android::app_cover(package, &ag::cover_path(&dir, package));
            }
        }
        match ag::write_folder(&dir, &games) {
            Ok(true) => {
                println!("android games: {} on the shelf ({why})", games.len());
                self.library_rescan(why);
            }
            Ok(false) => {}
            Err(e) => eprintln!("android games: the folder can't be written: {e}"),
        }
    }

    /// The marks into `launcher.json`, the Mobile view's file: everything else in it stays.
    fn android_games_save(&self, marked: &BTreeMap<String, String>) {
        let path = self.phone_prefs_path();
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        if let Err(e) = pocket_ui::fsutil::write_atomic(&path, ag::json_with_marked(&old, marked).as_bytes()) {
            eprintln!("android games: launcher.json not written: {e}");
        }
    }

    /// The Settings row's value: how many games stand on the shelf.
    pub(crate) fn android_games_list_value(&self) -> String {
        pocket_ui::settings_values::page(&ag::count(ag::shelf(&self.android_games.marked, self.android_games.apps.as_deref()).len()))
    }

    pub(crate) fn open_android_games(&mut self) -> Option<SettingsPage> {
        self.android_games.page = Some(Page::new(self.android_games.apps.clone()));
        self.android_games_refresh("page opened");
        Some(SettingsPage::AndroidGames)
    }

    pub(crate) fn android_games_input(&mut self, b: pocket_ui::pad::Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.android_games.page.as_mut() else { return back };
        let mut marked = self.android_games.marked.clone();
        match page.press(b, CONFIRM, CANCEL, &mut marked) {
            Action::Nothing => {}
            Action::Close => {
                self.android_games.page = None;
                return back;
            }
            Action::Marked(_, on) => {
                println!("android games: {} {}", page.selected().map_or("", |a| a.package.as_str()), if on { "on the shelf" } else { "off the shelf" });
                self.android_games_save(&marked);
                self.android_games.marked = marked;
                self.android_games_write("marked");
            }
        }
        Some(SettingsPage::AndroidGames)
    }

    /// Where the page's selection stands, and the marks: a change is what the menu sounds and haptics answer.
    pub(crate) fn android_games_row(&self) -> Option<(usize, usize, usize)> {
        self.android_games.page.as_ref().map(|p| (p.sel, p.len(), self.android_games.marked.len()))
    }

    pub(crate) fn paint_android_games(&mut self, c: &mut Canvas) {
        let Some(page) = self.android_games.page.take() else { return };
        self.paint_android_games_page(c, &page);
        self.android_games.page = Some(page);
    }

    fn paint_android_games_page(&mut self, c: &mut Canvas, page: &Page) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(58, 34), 34u32);
        self.sheet_header(c, SETTINGS_NAME);
        let marked = self.android_games.marked.clone();
        let (left, right) = (pad + NU_BTN_PAD, c.w - pad);
        let width = right - NU_BTN_PAD - left;
        // The line over the list: how games get there, or why there is no list.
        let mut y = pad + NU_PILL + 12 + pocket_ui::type_scale::line(NU_SMALL, 12);
        for line in self.text.wrap_prose(Face::Body, NU_SMALL, page.line(&marked), width).into_iter().take(3) {
            self.text.draw(c, Face::Body, NU_SMALL, left, y, pal().chalk, &line);
            y += pocket_ui::type_scale::line(NU_SMALL, 12);
        }
        let top = y + 4;
        let bottom = c.h - pad - NU_PILL - 16;
        let visible = ((bottom - top) / row).max(1) as usize;
        let first = super::window_start(page.sel, visible, page.len());
        let apps = page.apps.as_deref().unwrap_or_default();
        for (i, app) in apps.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let selected = i == page.sel;
            let note = Page::note(app, &marked);
            let note_w = if note.is_empty() { 0 } else { self.text.width(Face::Body, font, note) + 40 };
            let base = pocket_ui::kit::row_label(c, &mut self.text, (pad, y, right - pad, row), font, "", selected);
            let name = self.text.fit(Face::Body, font, &app.label, right - pad - 2 * NU_BTN_PAD - note_w);
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, if selected { pal().ink } else { pal().text }, &name);
            if !note.is_empty() {
                let on = marked.contains_key(&app.package);
                let colour = if selected { pal().ink } else if on { pal().chalk } else { pal().dust };
                self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, colour, note);
            }
        }
        self.hits.list(page.sel, page.len(), 0.0, row as f32);
        pocket_ui::kit::scrollbar(c, top, visible as i32 * row, first, visible, page.len());
        let mut hints = vec![("B", "Back")];
        if let Some(a) = page.a_hint(&marked) {
            hints.push(("A", a));
        }
        self.nu_hints(c, &hints);
    }

    /// `--snapshot android-games[:demo][:<row>]`: the page for the apps `POCKETUI_ANDROID_APPS` lists and the marks in
    /// launcher.json; `demo`: three made-up apps, a game Android files under games and one marked (the translations' states).
    pub(crate) fn android_games_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("android-games") else { return false };
        self.android_games.marked = ag::marked(&std::fs::read_to_string(self.phone_prefs_path()).unwrap_or_default());
        self.android_games.apps = Some(read_apps());
        let mut row = 0;
        for part in rest.split(':').filter(|p| !p.is_empty()) {
            if part == "demo" {
                self.android_games.apps = Some(ag::parse(r#"[{"package":"com.example.mail","label":"MailBox"},{"package":"com.example.racer","label":"Pixel Racer","game":true},{"package":"com.example.tower","label":"Tower Quest"}]"#));
                self.android_games.marked = BTreeMap::from([("com.example.tower".to_string(), "Tower Quest".to_string())]);
            } else if let Ok(r) = part.parse::<usize>() {
                row = r;
            }
        }
        let mut page = Page::new(self.android_games.apps.clone());
        page.sel = row.min(page.len().saturating_sub(1));
        self.android_games.page = Some(page);
        self.settings_page = Some(SettingsPage::AndroidGames);
        true
    }
}
