//! Part of the pocket-ui binary: the App's side of `pocket_ui::backup` and `pocket_ui::backup_page`.
//! Settings > Backup & reset: back up the saves, or pocket-ui's settings and library notes, to a dated folder on the card,
//! restore one, or reset the settings (after a backup). Every file operation runs on a thread; after a restore or a reset of
//! the settings pocket-ui starts again (as after an app update), so nothing in memory writes the old values back.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use std::sync::atomic::Ordering;

use pocket_ui::backup::{self, Backup, Manifest, Reason};
use pocket_ui::backup_saves::{self as saves, Plan, RestorePlan, SavesBackup};
use pocket_ui::storage::size;
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
            let _ = proxy.send_event(UserEvent::Backup(Event::Listed(backup::list(Path::new(backup::ROOT)), saves::list(Path::new(saves::ROOT)))));
        });
        Some(SettingsPage::Backup)
    }

    pub(crate) fn backup_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.backup.page.as_mut() else { return back };
        let (proxy, data_dir, app) = (self.proxy.clone(), self.data_dir.clone(), pocket_ui::update::app_version().to_string());
        let root = PathBuf::from(backup::ROOT);
        let (storage, saves_root) = (PathBuf::from(saves::STORAGE), PathBuf::from(saves::ROOT));
        let job: Option<Box<dyn FnOnce() -> Result<Done, String> + Send>> = match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => None,
            Action::Close => {
                self.backup.page = None;
                return back;
            }
            Action::BackUp => Some(Box::new(move || backup::create(&data_dir, &root, Reason::Manual, now(), &app).map(Done::BackedUp))),
            Action::Reset => Some(Box::new(move || backup::reset(&data_dir, &root, now(), &app).map(Done::Reset))),
            Action::Restore(dir) => Some(Box::new(move || backup::restore(&data_dir, &dir, &root, now(), &app).map(Done::Restored))),
            Action::MeasureSaves => {
                let proxy = proxy.clone();
                std::thread::spawn(move || {
                    let _ = proxy.send_event(UserEvent::Backup(Event::SavesMeasured(saves::measure(&storage, &saves_root, saves::second_card()))));
                });
                None
            }
            Action::CompareSaves(dir) => {
                let proxy = proxy.clone();
                std::thread::spawn(move || {
                    let _ = proxy.send_event(UserEvent::Backup(Event::SavesCompared(saves::compare(&dir, &storage, &saves_root))));
                });
                None
            }
            Action::BackUpSaves(plan) => {
                let progress = page.progress.clone().unwrap_or_default();
                Some(Box::new(move || saves::create(&storage, &saves_root, &plan.items, Reason::Manual, now(), &app, &progress).map(Done::SavesBackedUp)))
            }
            Action::RestoreSaves(plan) => {
                let progress = page.progress.clone().unwrap_or_default();
                Some(Box::new(move || saves::restore(&plan, &storage, &saves_root, now(), &app, &progress).map(Done::SavesRestored)))
            }
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

    /// A job is under way: no idle sleep, or it stops halfway until someone wakes the handheld.
    pub(crate) fn backup_working(&self) -> bool {
        self.backup.page.as_ref().is_some_and(|p| p.busy.is_some())
    }

    pub(crate) fn backup_animating(&self) -> bool {
        self.backup.page.as_ref().is_some_and(|p| p.busy.is_some() || p.list.is_none())
    }

    /// `--snapshot backup[:<row>][:reset|:restore|:done|:empty|:saves-counted|:saves-copying|:saves-full|:saves-restore|:saves-done]`:
    /// the page with made-up backups.
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
        let item = |path: &str, bytes: u64| saves::Item { path: path.into(), stored: format!("files/{path}"), bytes, modified: 1_792_000_000 };
        let saves_made = |name: &str, created: u64, reason: Reason| SavesBackup {
            dir: Path::new(saves::ROOT).join(name),
            manifest: saves::Manifest { version: 1, created, reason, app: "0.9.0".into(), items: vec![item("roms/gba/Game.srm", 3_550_000); 412], unreadable: Vec::new() },
        };
        let saves_list = if mode == "empty" { Vec::new() } else { vec![saves_made("2026-10-23_1430", 1_792_765_800, Reason::Manual)] };
        let counted = Plan { items: vec![item("roms/gba/Game.srm", 3_550_000); 412], bytes: 1_460_000_000, free: Some(18_300_000_000), needed: 1_530_000_000, skipped: 0, second_card: false };
        let mut page = BackupPage::new();
        page.event(Event::Listed(list, saves_list));
        page.sel = row.min(page.rows().len() - 1);
        match mode {
            "reset" => {
                page.sel = 2;
                page.press(Button::East, Button::East, Button::South);
            }
            "restore" => {
                page.sel = 5;
                page.press(Button::East, Button::East, Button::South);
            }
            "saves-counted" | "saves-copying" | "saves-card" => {
                page.sel = 0;
                let second_card = mode == "saves-card";
                page.event(Event::SavesMeasured(Plan { second_card, ..counted }));
                if mode == "saves-copying" {
                    page.press(Button::East, Button::East, Button::South);
                    if let Some(p) = &page.progress {
                        p.total.store(412, Ordering::Relaxed);
                        p.done.store(137, Ordering::Relaxed);
                    }
                }
            }
            "saves-full" => {
                page.sel = 0;
                page.event(Event::SavesMeasured(Plan { free: Some(300_000_000), ..counted }));
            }
            "saves-restore" => {
                page.sel = 3;
                page.event(Event::SavesCompared(Ok(RestorePlan {
                    dir: Path::new(saves::ROOT).join("2026-10-23_1430"),
                    new: vec![item("roms/gba/Game.srm", 20_000); 390],
                    replace: vec![item("roms/gba/Other.srm", 20_000); 12],
                    keep: vec!["roms/gba/Newer.srm".into(); 3],
                    same: 7,
                    damaged: 0,
                    free: Some(18_300_000_000),
                    needed: 1_530_000_000,
                })));
            }
            "saves-done" => {
                page.sel = 0;
                page.event(Event::Done(Ok(Done::SavesBackedUp(saves_made("2026-10-23_1502", 1_792_767_720, Reason::Manual)))));
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
                Row::BackUpSaves => ("Back up saves".to_string(), String::new()),
                Row::BackUp => ("Back up settings".to_string(), String::new()),
                Row::Reset => ("Reset settings".to_string(), String::new()),
                Row::Saves(k) => match page.saves.get(*k) {
                    Some(b) if b.manifest.reason == Reason::BeforeRestore => (backup::when_text(b.manifest.created), format!("Before restore · {}", size(b.bytes()))),
                    Some(b) => (backup::when_text(b.manifest.created), format!("Saves · {}", size(b.bytes()))),
                    None => (String::new(), String::new()),
                },
                Row::Backup(k) => match page.backup(*k) {
                    Some(b) => (b.when(), format!("{} · {} KB", b.manifest.reason.label(), b.bytes.div_ceil(1024))),
                    None => (String::new(), String::new()),
                },
            };
            let value_w = if value.is_empty() { 0 } else { self.text.width(Face::Body, font, &value) + 40 };
            let name = self.text.fit(Face::Body, font, &name, right - pad - 2 * NU_BTN_PAD - value_w);
            let selected = i == page.sel;
            if selected {
                pocket_ui::kit::selection(c, pad, y, right - pad, row);
            }
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, if selected { pal().ink } else { pal().text }, &name);
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, if selected { pal().ink } else { pal().dust }, &value);
        }
        self.hits.list(page.sel, rows.len(), 0.0, row as f32);

        let left = pad + NU_BTN_PAD;
        let width = c.w - pad - NU_BTN_PAD - left;
        let y0 = list_bottom + 14;
        c.rect(left, y0, width, 2, pal().spine);
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        let confirming = page.confirm.is_some() && page.confirm == Some(page.row());
        let (head, lines, color): (String, Vec<String>, u32) = if let (Some(b), Some(p)) = (&page.busy, &page.progress) {
            let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed));
            (format!("{b}{dots}"), vec![format!("{done} of {total} files."), "B stops it. A backup that is stopped leaves nothing behind.".into()], pal().chalk)
        } else if let Some(b) = &page.busy {
            (format!("{b}{dots}"), vec!["This takes a moment.".into()], pal().chalk)
        } else if let Some((m, err)) = &page.message {
            (if *err { "That did not work".into() } else { "Done".into() }, vec![m.clone()], if *err { pal().alert } else { pal().chalk })
        } else if confirming && page.row() == Row::Reset {
            (
                "Reset pocket-ui's settings?".into(),
                vec!["Look, sounds, haptics, LEDs, sleep, performance, Curator and every Options page go back to their defaults. A backup is made first. Press A again; B keeps them.".into()],
                pal().alert,
            )
        } else if let (true, Some(plan)) = (confirming, &page.plan) {
            let mut lines = vec![match plan.free {
                Some(f) => format!("Files: {}. Free on the card: {}.", plan.items.len(), size(f)),
                None => format!("Files: {}. The card doesn't say how much is free.", plan.items.len()),
            }];
            if plan.skipped > 0 {
                lines.push(format!("Could not be read, staying out: {}.", plan.skipped));
            }
            lines.push(if plan.second_card {
                "It goes on your games card, which flashing the system card does not touch.".into()
            } else {
                "Flashing this card erases the backup too: copy it to a computer over file sharing, in the backup folder.".into()
            });
            lines.push("Press A again; B keeps things as they are.".into());
            (format!("Back up {}?", size(plan.bytes)), lines, pal().chalk)
        } else if let (true, Some(plan)) = (confirming, &page.restore_plan) {
            let mut lines = vec![format!("Coming back: {}. Older ones replaced, after a copy of them: {}.", plan.new.len(), plan.replace.len())];
            if !plan.keep.is_empty() {
                lines.push(format!("Newer on the card, staying: {}. The backup keeps its copies.", plan.keep.len()));
            }
            if plan.damaged > 0 {
                lines.push(format!("Damaged in this backup, staying out: {}.", plan.damaged));
            }
            lines.push("Press A again; B keeps things as they are.".into());
            ("Put these saves back?".into(), lines, pal().alert)
        } else if confirming {
            (
                "Restore this backup?".into(),
                vec!["Settings and library notes go back to how they were then; what you have now is backed up first. Press A again; B keeps it.".into()],
                pal().alert,
            )
        } else {
            match page.row() {
                Row::BackUpSaves => (
                    "Back up saves".into(),
                    vec![
                        "Battery saves, save states, and the memory cards and save data of PS2, GameCube, Wii and 3DS. No games, no BIOS files.".into(),
                        "A counts them first and shows the size.".into(),
                        format!("To {} on the SD card. Flashing the card erases it, so copy it to a computer too.", saves::ROOT),
                    ],
                    pal().chalk,
                ),
                Row::Saves(k) => (
                    page.saves.get(k).map(|b| backup::when_text(b.manifest.created)).unwrap_or_default(),
                    vec![
                        page.saves.get(k).map_or(String::new(), |b| format!("Files: {}, {}, made by pocket-ui {}. {}", b.manifest.items.len(), size(b.bytes()), b.manifest.app, b.dir.display())),
                        "A compares it with the card and says what would change. What is newer on the card stays.".into(),
                    ],
                    pal().chalk,
                ),
                Row::BackUp => (
                    "Back up settings".into(),
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
            for line in self.text.wrap_prose(Face::Body, NU_SMALL, &text, width).into_iter().take(3) {
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

#[cfg(test)]
mod tests {
    /// Sleep's timers only know what `sleep_ctx` tells them; the page's busy state is gone when the job's Done arrives
    /// (`backup_page`'s tests), a stop included.
    #[test]
    fn a_job_under_way_holds_idle_sleep() {
        let main = include_str!("main.rs");
        let ctx = &main[main.find("fn sleep_ctx").unwrap()..];
        assert!(ctx[..ctx.find("power_menu:").unwrap()].contains("self.backup_working()"));
    }
}
