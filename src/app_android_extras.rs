//! Part of the pocket-ui binary: Settings › System › Android extras (Android's handheld view only), the App's side of
//! `pocket_ui::android_extras`. Android is asked on a thread (`PocketBase.extrasStatus`, `extrasAction` → `Extras.java`) when
//! the page opens, after each press and whenever the view comes back to the front (from Shizuku's question, Android's
//! accessibility page); the page shows "…" until the first answer. docs/ANDROID-EXTRAS.md.

use pocket_ui::android_extras::{self as ax, Action, Page, Status};
use pocket_ui::gfx::Canvas;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_SMALL};

pub(crate) const SETTINGS_NAME: &str = pocket_ui::android_extras::SETTINGS_NAME;

#[derive(Default)]
pub(crate) struct AndroidExtrasUi {
    /// The last answer, for the Settings row's value.
    status: Option<Status>,
    page: Option<Page>,
}

/// Android's answer (`Extras.status`); elsewhere (tests, snapshots) what `POCKETUI_ANDROID_EXTRAS` says.
fn read_status() -> String {
    #[cfg(all(target_os = "android", feature = "handheld"))]
    return super::app_android::extras_status();
    #[cfg(not(all(target_os = "android", feature = "handheld")))]
    return std::env::var(ax::STATUS_ENV).unwrap_or_default();
}

/// A press of the page carried out by Android ("" when done, else why not; only logged).
fn act(what: &str, arg: &str) -> String {
    #[cfg(all(target_os = "android", feature = "handheld"))]
    return super::app_android::extras_action(what, arg);
    #[cfg(not(all(target_os = "android", feature = "handheld")))]
    return format!("not on Android: {what} {arg}");
}

impl App {
    /// The answer, after an action when there was one; the overlay's lines go to Java first, in the language now.
    fn android_extras_ask(&mut self, action: Option<(&'static str, String)>) {
        if let Some(p) = self.android_extras.page.as_mut() {
            p.busy = true;
        }
        let texts = ax::texts_tsv(pocket_ui::i18n::tr);
        let waits_for_answer = matches!(action, Some(("shizuku-request", _)));
        let proxy = self.proxy.clone();
        let spawned = std::thread::Builder::new().name("android-extras".into()).spawn(move || {
            if let Some((what, arg)) = action {
                let said = act(what, &arg);
                println!("android extras: {what} {}", if said.is_empty() { "done" } else { said.as_str() });
            }
            let _ = act("texts", &texts);
            let mut status = read_status();
            // Shizuku's own question waits for the player: its answer comes to Java later, so the page asks again until
            // Shizuku no longer waits (a minute at most; B works meanwhile, A waits).
            if waits_for_answer {
                let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
                while Status::parse(&status).shizuku == "ask" && std::time::Instant::now() < until {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    status = read_status();
                }
            }
            let _ = proxy.send_event(UserEvent::AndroidExtras(status));
        });
        if spawned.is_err() {
            if let Some(p) = self.android_extras.page.as_mut() {
                p.busy = false;
            }
        }
    }

    pub(crate) fn android_extras_event(&mut self, json: String) {
        let status = Status::parse(&json);
        if let Some(p) = self.android_extras.page.as_mut() {
            p.status = Some(status.clone());
            p.busy = false;
            p.sel = p.sel.min(p.rows().len() - 1);
        }
        self.android_extras.status = Some(status);
        self.request_redraw();
    }

    /// The view is in front again: the page, when open, reads what changed meanwhile.
    pub(crate) fn android_extras_front(&mut self) {
        if self.android_extras.page.is_some() {
            self.android_extras_ask(None);
        }
    }

    /// The Settings row's value: the level as last read.
    pub(crate) fn android_extras_list_value(&self) -> String {
        // Here, not in `android_extras`: the player's library (android/player) has no `settings_values`.
        pocket_ui::settings_values::page(self.android_extras.status.as_ref().map_or("", |s| s.level().label()))
    }

    pub(crate) fn open_android_extras(&mut self) -> Option<SettingsPage> {
        self.android_extras.page = Some(Page::new(self.android_extras.status.clone()));
        self.android_extras_ask(None);
        Some(SettingsPage::AndroidExtras)
    }

    pub(crate) fn android_extras_input(&mut self, b: pocket_ui::pad::Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.android_extras.page.as_mut() else { return back };
        match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                self.android_extras.page = None;
                return back;
            }
            Action::Refresh => self.android_extras_ask(None),
            Action::Refuse => pocket_ui::ui_sound::refuse(),
            Action::Bridge(what, arg) => self.android_extras_ask(Some((what, arg))),
        }
        Some(SettingsPage::AndroidExtras)
    }

    /// Where the selection stands and what Android said: a change is what the menu sounds answer.
    pub(crate) fn android_extras_row(&self) -> Option<(usize, usize, bool)> {
        self.android_extras.page.as_ref().map(|p| (p.sel, p.rows().len(), p.status.is_some()))
    }

    pub(crate) fn paint_android_extras(&mut self, c: &mut Canvas) {
        let Some(page) = self.android_extras.page.take() else { return };
        self.paint_android_extras_page(c, &page);
        self.android_extras.page = Some(page);
    }

    fn paint_android_extras_page(&mut self, c: &mut Canvas, page: &Page) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(58, 34), 34u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (left, right) = (pad + NU_BTN_PAD, c.w - pad);
        let top = pocket_ui::kit::header_bottom(c.h) + 12;
        let hints_top = pocket_ui::kit::hint_bar_top(c.h);
        let rows = page.rows();

        // Under the list: what the selected row is for and the steps, beside them Shizuku's QR code, under them the ADB line.
        let qr = page.qr();
        let line_h = pocket_ui::type_scale::line(NU_SMALL, 12);
        let qr_room = if qr.is_some() { (hints_top - top) / 3 } else { 0 };
        let text_w = right - NU_BTN_PAD - left - if qr_room > 0 { qr_room + 24 } else { 0 };
        let mut lines: Vec<String> = Vec::new();
        // The ADB line goes after the first sentence ("… then type this line:"), before what comes after typing it.
        let mut split = usize::MAX;
        for (i, l) in page.describe().into_iter().enumerate() {
            if i == 1 {
                split = lines.len();
            }
            lines.extend(self.text.wrap_prose(Face::Body, NU_SMALL, l, text_w));
        }
        // Broken at spaces like any line (`keep.txt` keeps the pieces English: it is typed as it is).
        let adb = page.adb().map(|line| self.text.wrap(Face::Body, 26, &line, right - NU_BTN_PAD - left - 2 * pocket_ui::kit::PANEL_PAD));
        let adb_h = adb.as_ref().map_or(0, |l| l.len() as i32 * pocket_ui::type_scale::line(26, 10) + 28);
        let want = (lines.len() as i32 * line_h).max(qr_room) + adb_h + 28;
        // The rows keep at least three in view; the lines take what is left and are cut, never the rows.
        let visible = (((hints_top - 16 - top - want) / row).max(3) as usize).min(rows.len());
        let first = super::window_start(page.sel, visible, rows.len());
        for (i, r) in rows.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let selected = i == page.sel;
            let base = pocket_ui::kit::row_label(c, &mut self.text, (pad, y, right - pad, row), font, Page::label(*r), selected);
            pocket_ui::kit::row_value(c, &mut self.text, font, right - NU_BTN_PAD, base, page.value(*r), selected);
        }
        self.hits.list(page.sel, rows.len(), 0.0, row as f32);
        pocket_ui::kit::scrollbar(c, top, visible as i32 * row, first, visible, rows.len());

        let mut y = top + visible as i32 * row + 20 + pocket_ui::type_scale::line(NU_SMALL, 12) - 8;
        let desc_top = y;
        let bottom = hints_top - 12;
        let mut adb = adb;
        for (n, line) in lines.iter().enumerate() {
            if n == split {
                if let Some(cmd) = adb.take() {
                    y = self.paint_command(c, &cmd, y - line_h + 10, adb_h) + line_h;
                }
            }
            if y > bottom {
                break;
            }
            // The last line that fits says there is more, rather than ending mid-sentence.
            let line = if y + line_h > bottom && n + 1 < lines.len() { self.text.fit(Face::Body, NU_SMALL, &format!("{line} …"), text_w) } else { line.clone() };
            self.text.draw(c, Face::Body, NU_SMALL, left, y, if n == 0 { pal().chalk } else { pal().text }, &line);
            y += line_h;
        }
        if let Some(data) = qr {
            let size = qr_room.min(hints_top - 16 - (desc_top - line_h));
            if size >= 96 {
                pocket_ui::kit::qr(c, right - NU_BTN_PAD - size, desc_top - line_h + 4, size, data, 2, false);
            }
        }
        if let Some(cmd) = adb {
            self.paint_command(c, &cmd, y - line_h + 10, adb_h);
        }
        let mut hints = vec![("B", "Back")];
        if let Some(a) = page.a_hint().filter(|_| !page.busy) {
            hints.push(("A", a));
        }
        self.nu_hints(c, &hints);
    }

    /// The ADB line in a panel from `y`, `h` high; returns the panel's bottom.
    fn paint_command(&mut self, c: &mut Canvas, cmd: &[String], y: i32, h: i32) -> i32 {
        let (pad, right) = (NU_PAD, c.w - NU_PAD);
        pocket_ui::kit::panel(c, pad, y, right - pad, h - 8);
        let step = pocket_ui::type_scale::line(26, 10);
        let mut ly = y + 14 + step - 6;
        for l in cmd {
            self.text.draw(c, Face::Body, 26, pad + pocket_ui::kit::PANEL_PAD, ly, pal().chalk, l);
            ly += step;
        }
        y + h
    }

    /// `--snapshot android-extras[:<level>][:<row>]`: the page as `POCKETUI_ANDROID_EXTRAS` says, or a made-up state:
    /// `plain` (nothing given), `adb`, `shizuku`, `root`, `old` (Android 10 with its bars row); `wait` before the answer.
    pub(crate) fn android_extras_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("android-extras") else { return false };
        let mut status = Some(Status::parse(&read_status()));
        let mut row = 0;
        for part in rest.split(':').filter(|p| !p.is_empty()) {
            let json = match part {
                "plain" => r#"{"sdk":34,"package":"xyz.pocketui.app","secure":false,"shizuku":"missing","root":"","watch":false,"switches":{"heads_up":"off","animations":"off","awake":"off","bars":""}}"#,
                "adb" => r#"{"sdk":34,"package":"xyz.pocketui.app","secure":true,"shizuku":"stopped","root":"","watch":true,"switches":{"heads_up":"on","animations":"off","awake":"on","bars":""}}"#,
                "shizuku" => r#"{"sdk":34,"package":"xyz.pocketui.app","secure":true,"shizuku":"ready","root":"","watch":true,"switches":{"heads_up":"on","animations":"on","awake":"off","bars":""}}"#,
                "root" => r#"{"sdk":33,"package":"xyz.pocketui.app","secure":true,"shizuku":"missing","root":"yes","watch":false,"switches":{"heads_up":"off","animations":"off","awake":"off","bars":""}}"#,
                "old" => r#"{"sdk":29,"package":"xyz.pocketui.app","secure":true,"shizuku":"ask","root":"no","watch":false,"switches":{"heads_up":"on","animations":"off","awake":"off","bars":"on"}}"#,
                "wait" => {
                    status = None;
                    continue;
                }
                p => {
                    if let Ok(r) = p.parse::<usize>() {
                        row = r;
                    }
                    continue;
                }
            };
            status = Some(Status::parse(json));
        }
        let mut page = Page::new(status.clone());
        page.sel = row.min(page.rows().len() - 1);
        self.android_extras.status = status;
        self.android_extras.page = Some(page);
        self.settings_page = Some(SettingsPage::AndroidExtras);
        true
    }
}
