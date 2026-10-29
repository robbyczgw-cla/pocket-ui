//! Part of the pocket-ui binary: the App's side of `pocket_ui::backup` and `pocket_ui::backup_page`.
//! Settings > Backup & reset: back up pocket-ui's settings and library notes to a dated folder on the card, restore one,
//! or reset the settings (after a backup). Every file operation runs on a thread; after a restore or a reset pocket-ui
//! starts again (as after an app update), so nothing in memory writes the old values back.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pocket_ui::backup::{self, Backup, Manifest, Reason};
use pocket_ui::backup_page::{Action, BackupPage, Done, Event, Row};
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Backup & reset";
/// Below the list: what the selected row does.
const DETAILS_H: i32 = 290;
/// After a restore or a reset: how long the page says so before pocket-ui starts again.
const RESTART_AFTER: Duration = Duration::from_millis(1500);

/// What the App keeps: the open page.
#[derive(Default)]
pub(crate) struct BackupUi {
    page: Option<BackupPage>,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl App {
    pub(crate) fn open_backup(&mut self) -> Option<SettingsPage> {
        self.backup.page = Some(BackupPage::new());
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let _ = proxy.send_event(UserEvent::Backup(Event::Listed(backup::list(Path::new(backup::ROOT)))));
        });
        Some(SettingsPage::Backup)
    }

    pub(crate) fn backup_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.backup.page.as_mut() else { return back };
        let (proxy, data_dir, app) = (self.proxy.clone(), self.data_dir.clone(), env!("CARGO_PKG_VERSION").to_string());
        let root = PathBuf::from(backup::ROOT);
        let job: Option<Box<dyn FnOnce() -> Result<Done, String> + Send>> = match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => None,
            Action::Close => {
                self.backup.page = None;
                return back;
            }
            Action::BackUp => Some(Box::new(move || backup::create(&data_dir, &root, Reason::Manual, now(), &app).map(Done::BackedUp))),
            Action::Reset => Some(Box::new(move || backup::reset(&data_dir, &root, now(), &app).map(Done::Reset))),
            Action::Restore(dir) => Some(Box::new(move || backup::restore(&data_dir, &dir, &root, now(), &app).map(Done::Restored))),
        };
        if let Some(job) = job {
            std::thread::spawn(move || {
                let result = job();
                match &result {
                    Ok(d) => println!("backup: {d:?}"),
                    Err(e) => eprintln!("backup: {e}"),
                }
                let _ = proxy.send_event(UserEvent::Backup(Event::Done(result)));
            });
        }
        Some(SettingsPage::Backup)
    }

    pub(crate) fn backup_event(&mut self, e: Event) {
        let Some(page) = self.backup.page.as_mut() else { return };
        page.event(e);
        if page.restarting && self.updates.restart_at.is_none() {
            // Like after an app update: the launcher starts pocket-ui again, which reads the files afresh.
            self.updates.restart_at = Some(Instant::now() + RESTART_AFTER);
        }
    }

    /// Where the page's selection stands: a change is what the menu sounds and haptics answer.
    pub(crate) fn backup_row(&self) -> Option<(usize, bool, bool)> {
        self.backup.page.as_ref().map(|p| (p.sel, p.confirm.is_some(), p.busy.is_some()))
    }

    pub(crate) fn backup_animating(&self) -> bool {
        self.backup.page.as_ref().is_some_and(|p| p.busy.is_some() || p.list.is_none())
    }

    /// `--snapshot backup[:<row>][:reset|:restore|:done|:empty]`: the page with made-up backups.
    pub(crate) fn backup_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("backup") else { return false };
        let mut parts = rest.trim_start_matches(':').split(':');
        let first = parts.next().unwrap_or("");
        let (row, mode) = match first.parse::<usize>() {
            Ok(r) => (r, parts.next().unwrap_or("")),
            Err(_) => (0, first),
        };
        let made = |name: &str, created: u64, reason: Reason, files: usize| Backup {
            dir: Path::new(backup::ROOT).join(name),
            manifest: Manifest { version: 1, created, reason, app: "0.9.0".into(), files: (0..files).map(|i| format!("f{i}")).collect() },
            bytes: files as u64 * 3100,
        };
        let list = if mode == "empty" {
            Vec::new()
        } else {
            vec![
                made("2026-10-23_1420", 1_792_765_200, Reason::Manual, 14),
                made("2026-10-20_0905", 1_792_487_100, Reason::BeforeReset, 12),
                made("2026-10-12_2131", 1_791_833_460, Reason::Manual, 9),
            ]
        };
        let mut page = BackupPage::new();
        page.event(Event::Listed(list));
        page.sel = row.min(page.rows().len() - 1);
        match mode {
            "reset" => {
                page.sel = 1;
                page.press(Button::East, Button::East, Button::South);
            }
            "restore" => {
                page.sel = 3;
                page.press(Button::East, Button::East, Button::South);
            }
            "done" => {
                page.event(Event::Done(Ok(Done::BackedUp(made("2026-10-23_1502", 1_792_767_720, Reason::Manual, 14)))));
            }
            _ => {}
        }
        self.backup.page = Some(page);
        self.settings_page = Some(SettingsPage::Backup);
        true
    }

    pub(crate) fn paint_backup(&mut self, c: &mut Canvas) {
        let Some(page) = self.backup.page.take() else { return };
        self.paint_backup_page(c, &page);
        self.backup.page = Some(page);
    }

    fn paint_backup_page(&mut self, c: &mut Canvas, page: &BackupPage) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(58, 34), 34u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let rows = page.rows();
        let list_bottom = c.h - pad - NU_PILL - 16 - DETAILS_H;
        let visible = (((list_bottom - top) / row) as usize).max(3);
        let first = super::window_start(page.sel, visible, rows.len());
        if rows.len() > visible {
            let track = visible as i32 * row;
            pocket_ui::kit::scrollbar(c, top, track, first, visible, rows.len());
        }
        for (i, r) in rows.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let (name, value) = match r {
                Row::BackUp => ("Back up now".to_string(), String::new()),
                Row::Reset => ("Reset settings".to_string(), String::new()),
                Row::Backup(k) => match page.backup(*k) {
                    Some(b) => (b.when(), format!("{} · {} KB", b.manifest.reason.label(), b.bytes.div_ceil(1024))),
                    None => (String::new(), String::new()),
                },
            };
            let value_w = if value.is_empty() { 0 } else { self.text.width(Face::Body, font, &value) + 40 };
            let name = self.text.fit(Face::Body, font, &name, right - pad - 2 * NU_BTN_PAD - value_w);
            let selected = i == page.sel;
            if selected {
                let w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
            }
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, if selected { pal().ink } else { pal().text }, &name);
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, if selected { pal().text } else { pal().dust }, &value);
        }
        self.hits.list(page.sel, rows.len(), 0.0, row as f32);

        let left = pad + NU_BTN_PAD;
        let width = c.w - pad - NU_BTN_PAD - left;
        let y0 = list_bottom + 14;
        c.rect(left, y0, width, 2, pal().spine);
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        let confirming = page.confirm.is_some() && page.confirm == Some(page.row());
        let (head, lines, color): (String, Vec<String>, u32) = if let Some(b) = &page.busy {
            (format!("{b}{dots}"), vec!["This takes a moment.".into()], pal().chalk)
        } else if let Some((m, err)) = &page.message {
            (if *err { "That did not work".into() } else { "Done".into() }, vec![m.clone()], if *err { pal().alert } else { pal().chalk })
        } else if confirming && page.row() == Row::Reset {
            (
                "Reset pocket-ui's settings?".into(),
                vec!["Look, sounds, haptics, LEDs, sleep, performance, Curator and every Options page go back to their defaults. A backup is made first. Press A again; B keeps them.".into()],
                pal().alert,
            )
        } else if confirming {
            (
                "Restore this backup?".into(),
                vec!["Settings and library notes go back to how they were then; what you have now is backed up first. Press A again; B keeps it.".into()],
                pal().alert,
            )
        } else {
            match page.row() {
                Row::BackUp => (
                    "Back up now".into(),
                    vec![
                        "pocket-ui's settings, every Options page, and your library notes (status, notes, time played, favourites).".into(),
                        format!("To {} on the SD card. Not your games, saves or states.", backup::ROOT),
                    ],
                    pal().chalk,
                ),
                Row::Reset => (
                    "Reset settings".into(),
                    vec![
                        "pocket-ui's settings and Options back to their defaults, after a backup.".into(),
                        "Kept: your library notes, Sync, Tailscale, accounts and what the Store installed. pocket-ui starts again.".into(),
                    ],
                    pal().chalk,
                ),
                Row::Backup(k) => (
                    page.backup(k).map(Backup::when).unwrap_or_default(),
                    vec![
                        page.backup(k).map_or(String::new(), |b| format!("{} files, made by pocket-ui {}. {}", b.manifest.files.len(), b.manifest.app, b.dir.display())),
                        "A restores it; what you have now is backed up first. pocket-ui starts again.".into(),
                    ],
                    pal().chalk,
                ),
            }
        };
        let head = self.text.fit(Face::Title, 44, &head, width);
        self.text.draw(c, Face::Title, 44, left, y0 + 58, color, &head);
        let mut y = y0 + 108;
        for text in lines {
            for line in self.text.wrap(Face::Body, NU_SMALL, &text, width).into_iter().take(3) {
                if y > y0 + DETAILS_H - 10 {
                    break;
                }
                self.text.draw(c, Face::Body, NU_SMALL, left, y, pal().text, &line);
                y += pocket_ui::type_scale::line(NU_SMALL, 12);
            }
        }
        if page.restarting {
            self.nu_hints(c, &[]);
        } else {
            self.nu_hints(c, &[("B", "Back"), ("A", page.a_hint())]);
        }
    }
}
