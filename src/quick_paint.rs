//! Painting of `Sys`: the quick settings panel, the time pages, the OSD and the colour tint.
//! Same row style as the Settings list and the pause menu (see `paint_settings` in main.rs).

use std::time::Instant;

use crate::audio_route::Sound;
use crate::clock_check::{self, Clock};
use crate::gfx::Canvas;
use crate::manual_time::{self, Picker};
use crate::osd;
use crate::palette::*;
use crate::power_menu::{self, Choice};
use crate::quick::*;
use crate::pad::Button;
use crate::quick_profiles::Which;
use crate::system_ctl::NtpState;
use crate::touch::Area;
use crate::text::{Face, Text};
use crate::timezone;

const PAD: i32 = 30;
const PILL: i32 = 90;
const BTN_PAD: i32 = 36;
const FONT: u32 = 36;
const SMALL: u32 = 30;
/// A row's note (`RowView::note`).
const NOTE: u32 = 26;
/// The accent bar of the power menu's selected row: the Settings band of the dark design (the panel is dark in both
/// appearances).
const ACCENT: u32 = crate::palette::DARK.band;
/// The selected row's name pill: the label's paper, as in the pause menu and Settings.
const NAME_PILL: u32 = crate::palette::DARK.select;

/// A row: 66 px around `FONT`, taller with Settings → Text size (`type_scale`).
fn row() -> i32 {
    crate::type_scale::row(66, FONT)
}

/// The Volume row's value: "45 %"; "Muted" at 0; "5 %, muted" while the output is muted at a level (Left/Right still
/// move the level, which shows).
pub fn volume_label(vol: u8, muted: bool) -> String {
    match (vol, muted) {
        (0, _) => "Muted".to_string(),
        (v, true) => format!("{v}%, muted"),
        (v, false) => format!("{v}%"),
    }
}

/// The Volume row's note: where the sound goes. "…" before the first look; a Bluetooth device's name is cut short to
/// keep the row's value and bar in place (the Bluetooth page has the full name).
pub fn sound_label(sound: Option<&Sound>) -> String {
    const NAME_MAX: usize = 14;
    match sound {
        None => NOT_YET.to_string(),
        Some(Sound::Speaker) => "Speaker".to_string(),
        Some(Sound::Headphones) => "Headphones".to_string(),
        Some(Sound::Usb) => "USB audio".to_string(),
        Some(Sound::Hdmi) => "HDMI".to_string(),
        Some(Sound::Bluetooth(None)) => "Bluetooth".to_string(),
        Some(Sound::Bluetooth(Some(name))) => {
            let name = name.trim();
            if name.chars().count() > NAME_MAX {
                format!("Bluetooth: {}…", name.chars().take(NAME_MAX - 1).collect::<String>().trim_end())
            } else {
                format!("Bluetooth: {name}")
            }
        }
        Some(Sound::Unknown) => "Output unknown".to_string(),
    }
}

/// Local time as "13:28" (zone changes show at once: `tzset` re-reads /etc/localtime).
pub fn clock() -> String {
    let tm = local_tm();
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

fn local_tm() -> libc::tm {
    local_tm_at(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64)
}

fn local_tm_at(unix: i64) -> libc::tm {
    let now = unix as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: tzset and localtime_r only touch libc's own state and the struct we hand in.
    unsafe {
        crate::timezone::tzset();
        libc::localtime_r(&now, &mut tm);
    }
    tm
}

/// "Wed 21 Oct 2026", of the real time or of a given one (a snapshot of a wrong clock).
fn date_label_at(unix: Option<i64>) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let tm = unix.map_or_else(local_tm, local_tm_at);
    format!(
        "{} {} {} {}",
        DAYS[tm.tm_wday.rem_euclid(7) as usize],
        tm.tm_mday,
        MONTHS[tm.tm_mon.rem_euclid(12) as usize],
        tm.tm_year + 1900
    )
}

/// "17:42" in a zone `offset` seconds east of UTC.
fn clock_at(unix: i64, offset: i32) -> String {
    timezone::clock_at(unix, offset)
}

fn window_start(sel: usize, visible: usize, len: usize) -> usize {
    if len <= visible {
        0
    } else {
        sel.saturating_sub(visible / 2).min(len - visible)
    }
}

struct RowView {
    name: String,
    value: String,
    /// Small and quieter, left of the value: what the value is for, or what the game runs with instead.
    note: Option<String>,
    /// Left/Right change the value: arrows round it while selected, each a button for a finger.
    arrows: bool,
    /// The note says the game keeps a choice of its own: it stands out.
    note_own: bool,
    /// A level bar left of the value: (0..=1, centred).
    bar: Option<(f32, bool)>,
    /// A dot before the name: the current choice.
    current: bool,
    /// Space before the row, for groups.
    gap: i32,
    dim: bool,
}

impl RowView {
    fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        RowView {
            name: name.into(),
            value: value.into(),
            note: None,
            arrows: false,
            note_own: false,
            bar: None,
            current: false,
            gap: 0,
            dim: false,
        }
    }
}

impl Sys {
    /// Draws whatever `Sys` has on screen over what `main` painted, then applies the soft colour tint.
    /// `tint`: false while the pause menu shows a captured game frame (it is tinted already).
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, tint: bool) {
        self.hits.clear();
        match &self.screen {
            Screen::Closed => {}
            Screen::Quick { .. } if self.over_game => crate::palette::scoped(0, || self.paint_quick(c, text)),
            Screen::Quick { .. } => self.paint_quick(c, text),
            Screen::Time(_) => self.paint_time(c, text),
            // A panel over whatever is behind it, the game included: dark in both appearances. Opened from the quick
            // settings' Power… row, those stay visible under it (not touchable: the menu's map replaces theirs).
            Screen::Power(_) => {
                self.paint_under_power(c, text);
                self.hits.clear();
                crate::text::boxes::covered();
                crate::palette::scoped(0, || self.paint_power(c, text))
            }
        }
        if let Some(osd) = &self.osd {
            osd.paint(c, text);
        }
        if let Some((msg, _)) = &self.message {
            let w = text.width(Face::Body, SMALL, msg) + 2 * BTN_PAD;
            let y = c.h - PAD - PILL - 100;
            c.round_rect((c.w - w) / 2, y, w, 64, 32, 0x262626);
            text.draw(c, Face::Body, SMALL, (c.w - w) / 2 + BTN_PAD, y + 32 + crate::type_scale::mid(SMALL), 0xffffff, msg);
        }
        if let Some((msg, at)) = &self.toast {
            // Comes down into place and fades in and out (`motion::paint_toast`).
            let w = text.width(Face::Body, 28, msg) + 48;
            crate::motion::paint_toast(c, *at, Instant::now(), Some(TOAST_FOR), (c.w / 2 - w / 2, 36, w, 52), 20.0, |c, dy| {
                c.round_rect(c.w / 2 - w / 2, 36 + dy, w, 52, 26, KEYCAP);
                text.draw(c, Face::Body, 28, c.w / 2 - w / 2 + 24, 72 + dy, CHALK, msg);
            });
        }
        if tint && self.look.soft {
            self.look.apply_soft(c.buf);
        }
    }

    /// The quick settings under a power menu opened from them, painted as they were.
    fn paint_under_power(&mut self, c: &mut Canvas, text: &mut Text) {
        let Screen::Power(m) = &mut self.screen else { return };
        if !matches!(*m.under, Screen::Quick { .. }) {
            return;
        }
        let under = std::mem::replace(&mut *m.under, Screen::Closed);
        let Screen::Power(menu) = std::mem::replace(&mut self.screen, under) else { return };
        if self.over_game {
            crate::palette::scoped(0, || self.paint_quick(c, text));
        } else {
            self.paint_quick(c, text);
        }
        let mut menu = menu;
        *menu.under = std::mem::replace(&mut self.screen, Screen::Closed);
        self.screen = Screen::Power(menu);
    }

    fn header(&mut self, c: &mut Canvas, text: &mut Text, title: &str) {
        // Over a paused game the game stays visible behind the panel (`Sys::open_quick_over_game`).
        if !self.over_game {
            c.clear(pal().void);
        }
        // For the snapshot checks of Settings → Text size: the page behind is gone or under the panel.
        crate::text::boxes::covered();
        // The status corner in a pill, as on Settings: time, Wi-Fi while connected, battery, from memory (the App marks
        // the frame as one with the corner, `app_status_corner`). The title gives way to it.
        let pill = crate::status_corner::Pill { right: c.w - PAD, top: PAD, h: PILL - 20, px: SMALL, pad: BTN_PAD };
        let left = pill.paint(c, text, &crate::status_corner::Corner::now(), pal().pill, pal().soft);
        let title = text.fit(Face::Title, 50, title, left - 24 - (PAD + BTN_PAD));
        text.draw(c, Face::Title, 50, PAD + BTN_PAD, PAD + 58, pal().chalk, &title);
    }

    /// The rows, and where a finger can tap them (`hits`: a row selects it, the selected one is A, its arrows are
    /// Left and Right).
    fn paint_rows(&mut self, c: &mut Canvas, text: &mut Text, top: i32, rows: &[RowView], sel: usize, bottom: i32) {
        let right = c.w - PAD;
        let total: i32 = rows.iter().map(|r| row() + r.gap).sum();
        let mut visible = rows.len();
        // Long lists scroll in a window that keeps the selection near the middle.
        let room = bottom - top;
        let first = if total > room {
            visible = ((room / row()).max(3) as usize).min(rows.len());
            window_start(sel, visible, rows.len())
        } else {
            0
        };
        let mut y = top;
        for (i, r) in rows.iter().enumerate().skip(first).take(visible) {
            if i != first {
                y += r.gap;
            }
            let base = y + row() / 2 + crate::type_scale::mid(FONT);
            let selected = i == sel;
            self.hits.row(Area::new(PAD, y, right - PAD, row()), i);
            let value = if selected && r.arrows { format!("‹ {} ›", r.value) } else { r.value.clone() };
            let value_w = if value.is_empty() { 0 } else { text.width(Face::Body, FONT, &value) };
            let note_w = r.note.as_ref().map_or(0, |n| text.width(Face::Body, NOTE, n) + 24);
            let bar_w = if r.bar.is_some() { 300 } else { 0 };
            let reserve = if value_w > 0 { value_w + 24 } else { 0 } + note_w + if bar_w > 0 { bar_w + 24 } else { 0 };
            let name = text.fit(Face::Body, FONT, &r.name, right - PAD - 2 * BTN_PAD - reserve - if r.current { 30 } else { 0 });
            let dot = if r.current { 30 } else { 0 };
            let name_w = text.width(Face::Body, FONT, &name) + 2 * BTN_PAD + dot;
            let (fg, name_fg) = if selected {
                (pal().ink, pal().ink)
            } else {
                (if r.dim { pal().dust } else { pal().text }, if r.dim { pal().dust } else { pal().text })
            };
            if selected {
                c.round_rect(PAD, y + 4, right - PAD, row() - 8, (row() - 8) / 2, pal().band);
                c.round_rect(PAD, y + 4, name_w, row() - 8, (row() - 8) / 2, pal().select);
            }
            if r.current {
                c.disc(PAD + BTN_PAD + 8, y + row() / 2, 7, if selected { pal().ink } else { pal().text });
            }
            text.draw(c, Face::Body, FONT, PAD + BTN_PAD + dot, base, name_fg, &name);
            let mut vx = right - BTN_PAD;
            if value_w > 0 {
                text.draw_right(c, Face::Body, FONT, vx, base, if selected { pal().text } else { fg }, &value);
                if selected && r.arrows {
                    let aw = text.width(Face::Body, FONT, "‹ ") + 24;
                    self.hits.button(Area::new(vx - value_w - 24, y, aw, row()), Button::Left);
                    self.hits.button(Area::new(vx - aw + 24, y, aw, row()), Button::Right);
                }
                vx -= value_w + 24;
            }
            if let Some(n) = &r.note {
                // On the selected row's grey band orange would be hard to read: the footer says it there.
                let color = match (r.note_own, selected) {
                    (_, true) => pal().text,
                    (true, false) => pal().amber,
                    (false, false) => pal().dust,
                };
                text.draw_right(c, Face::Body, NOTE, vx, base - 2, color, n);
                vx -= note_w;
            }
            if let Some((frac, centered)) = r.bar {
                let (fill, track) = if selected { (pal().pick(NAME_PILL, 0x2b2925), pal().pick(0x5f5b55, 0xb3ab9c)) } else { (pal().pick(LABEL, 0x5e5a53), pal().pick(0x3c3a37, 0xd6d0c4)) };
                osd::paint_bar(c, vx - bar_w, y + row() / 2, bar_w, frac, centered, fill, track);
            }
            y += row();
        }
        self.hits.list(sel, rows.len(), 0.0, row() as f32);
        if rows.len() > visible {
            let track = visible as i32 * row();
            crate::kit::scrollbar(c, top, track, first, visible, rows.len());
        }
    }

    /// The explanation of the selected row, centred above the bottom edge (at most four lines).
    /// The explanation and the hints, as on every Settings page (`kit::footer`).
    fn footer(&mut self, c: &mut Canvas, text: &mut Text, hints: &[(&str, &str)], desc: &str, color: u32) {
        crate::kit::footer(c, text, &mut self.hits, hints, desc, 4, color);
    }

    /// Button hints in a dark pill at the bottom right (`kit::pill_hints`, as `nu_hints` in main.rs).
    fn hints(&mut self, c: &mut Canvas, text: &mut Text, hints: &[(&str, &str)]) {
        crate::kit::pill_hints(c, text, &mut self.hits, hints);
    }

    fn paint_quick(&mut self, c: &mut Canvas, text: &mut Text) {
        let now = std::time::Instant::now();
        let Screen::Quick { sel } = &self.screen else { return };
        let sel = *sel;
        self.header(c, text, "Quick settings");
        let rows = self.rows();
        let sel = sel.min(rows.len() - 1);
        let mut views = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let profile = |w: Which| {
                let mut v = RowView::new(if w == Which::Performance { Row::Performance.label() } else { Row::Fan.label() }, self.profiles.value(w));
                v.note = self.profiles.note(w);
                v.note_own = self.profiles.overridden(w);
                v.arrows = self.profiles.available(w);
                v.dim = !self.profiles.available(w);
                v
            };
            let mut v = match row {
                // From memory only (`Sys::view`, the background's look): "…" until the first look is in.
                Row::Brightness => {
                    let b = self.brightness_now(now);
                    let mut v = RowView::new(Row::Brightness.label(), b.map_or(NOT_YET.to_string(), |b| format!("{b}%")));
                    v.bar = Some((b.unwrap_or(0) as f32 / 100.0, false));
                    v
                }
                Row::Volume => {
                    let vol = self.volume_now(now);
                    let mut v = RowView::new(Row::Volume.label(), vol.map_or(NOT_YET.to_string(), |vol| volume_label(vol, self.view.muted == Some(true))));
                    v.bar = Some((vol.unwrap_or(0) as f32 / 100.0, false));
                    v.note = Some(sound_label(self.view.sound.as_ref()));
                    v
                }
                Row::Wifi => RowView::new(Row::Wifi.label(), self.switch_value(self.view.busy_wifi, self.view.wifi)),
                Row::Bluetooth => RowView::new(Row::Bluetooth.label(), self.switch_value(self.view.busy_bt, self.view.bluetooth)),
                Row::Performance => profile(Which::Performance),
                Row::Fan => profile(Which::Fan),
                Row::Power => RowView::new(Row::Power.label(), ""),
            };
            if i > 0 && matches!(row, Row::Wifi | Row::Performance | Row::Power) {
                v.gap = 18;
            }
            views.push(v);
        }
        let top = PAD + PILL + 12;
        self.paint_rows(c, text, top, &views, sel, c.h - PAD - PILL - 140);
        let (desc, color) = match rows[sel] {
            Row::Brightness => (
                "Left and right change it. On the shelf and in games: START + Vol+ or Vol-.".to_string(),
                pal().text,
            ),
            Row::Volume => ("Left and right change it. Vol+ and Vol- work everywhere.".to_string(), pal().text),
            // A switch that did not work says so here, in the alert colour, until the next try or the panel closes.
            Row::Wifi if self.wifi_failed.is_some() => (self.wifi_failed.as_ref().map_or(String::new(), |n| n.line(false).to_string()), pal().alert),
            Row::Wifi => ("A turns Wi-Fi on or off. While it is off, file sharing and updates don't work.".to_string(), pal().text),
            Row::Bluetooth => ("A turns Bluetooth on or off, for wireless controllers and headphones.".to_string(), pal().text),
            Row::Performance => (self.profiles.desc(Which::Performance), pal().text),
            Row::Fan => (self.profiles.desc(Which::Fan), pal().text),
            Row::Power if self.ram_booted() => (
                "Sleep, Restart, Power off. Careful: this ROCKNIX was started from a computer; restarting returns to Android.".to_string(),
                pal().alert,
            ),
            Row::Power => ("Sleep, Restart, Power off and Restart pocket-ui.".to_string(), pal().text),
        };
        let hints: &[(&str, &str)] = match rows[sel] {
            Row::Brightness | Row::Volume => &[("< >", "Change"), ("B", "Close")],
            Row::Performance | Row::Fan => &[("< >", "Change"), ("B", "Close"), ("A", "Next")],
            _ => &[("B", "Close"), ("A", "Choose")],
        };
        // The explanation beside the hints or over them, left aligned, as on every Settings page (`kit::footer`).
        crate::kit::footer(c, text, &mut self.hits, hints, &desc, 4, color);
    }

    /// The power menu: a panel in the middle over whatever is behind it, dimmed. The battery at the top right (the
    /// level and the time left, as on Settings → Battery); each entry with its drawn mark, its name and a line of what
    /// it does under it (`Choice::hint`). Where the device can't restart into the boot menu, the way to Android is
    /// written under the list instead.
    fn paint_power(&mut self, c: &mut Canvas, text: &mut Text) {
        let Some(menu) = self.power_menu() else { return };
        let (choices, ctx, battery) = (menu.choices.clone(), menu.ctx, menu.battery.clone());
        let sel = menu.sel.min(choices.len().saturating_sub(1));
        c.fade(200);

        const HINT_SIZE: u32 = 26;
        const LEVEL: u32 = 37;
        const MARK_W: i32 = 64;
        let (top_h, hint_h) = (crate::type_scale::row(60, FONT), crate::type_scale::line(HINT_SIZE, 14));
        let row_h = top_h + hint_h;
        let panel_w = 960.min(c.w - 2 * PAD);
        let inner_w = panel_w - 2 * BTN_PAD;
        let note: Vec<String> = if choices.contains(&Choice::BootMenu) {
            Vec::new()
        } else {
            text.wrap(Face::Body, HINT_SIZE, power_menu::NO_BOOT_MENU_HINT, inner_w)
        };
        let title_h = 112 + crate::type_scale::grow(50);
        let note_step = crate::type_scale::line(HINT_SIZE, 10);
        let note_h = if note.is_empty() { 0 } else { 28 + note.len() as i32 * note_step };
        let panel_h = title_h + choices.len() as i32 * row_h + note_h + 30;
        let (x, y) = ((c.w - panel_w) / 2, ((c.h - panel_h) / 2 - 30).max(PAD));
        c.round_rect(x - 3, y - 3, panel_w + 6, panel_h + 6, 43, 0x3c3a37);
        c.round_rect(x, y, panel_w, panel_h, 40, SPINE);
        let title_base = y + 72 + crate::type_scale::grow(50) / 2;
        text.draw(c, Face::Title, 50, x + BTN_PAD, title_base, CHALK, "Power");

        // The battery: the level over what it means in time; without a level only the line that says so.
        let right = x + panel_w - BTN_PAD;
        let title_w = text.width(Face::Title, 50, "Power") + 40;
        let room = (right - (x + BTN_PAD + title_w)).max(0);
        match &battery.level {
            Some(level) => {
                let level_base = y + 30 + crate::type_scale::px(LEVEL) as i32;
                text.draw_right(c, Face::Title, LEVEL, right, level_base, CHALK, level);
                let time = text.fit(Face::Body, HINT_SIZE, &battery.time, room);
                text.draw_right(c, Face::Body, HINT_SIZE, right, level_base + crate::type_scale::line(HINT_SIZE, 10), 0xb8b4ac, &time);
            }
            None => {
                let time = text.fit(Face::Body, HINT_SIZE, &battery.time, room);
                text.draw_right(c, Face::Body, HINT_SIZE, right, title_base, 0xb8b4ac, &time);
            }
        }

        let mut ry = y + title_h;
        for (i, choice) in choices.iter().enumerate() {
            let selected = i == sel;
            let (rx, rw) = (x + 14, panel_w - 28);
            self.hits.row(Area::new(rx, ry, rw, row_h), i);
            let label = choice.label();
            let label_x = rx + MARK_W + 10;
            let mid = ry + 4 + (top_h - 4) / 2;
            let bg = if selected {
                let name_w = MARK_W + 10 + text.width(Face::Body, FONT, label) + BTN_PAD;
                c.round_rect(rx, ry + 4, rw, row_h - 8, 30, ACCENT);
                c.round_rect(rx, ry + 4, name_w, top_h - 4, (top_h - 4) / 2, NAME_PILL);
                NAME_PILL
            } else {
                SPINE
            };
            let fg = if selected { INK } else { 0xf2f2f2 };
            paint_mark(c, rx + MARK_W / 2 + 8, mid, choice.mark(), fg, bg);
            text.draw(c, Face::Body, FONT, label_x, mid + crate::type_scale::mid(FONT), fg, label);
            let hint = text.fit(Face::Body, HINT_SIZE, choice.hint(ctx), rx + rw - BTN_PAD / 2 - label_x);
            let hint_base = ry + top_h + hint_h / 2 + crate::type_scale::mid(HINT_SIZE) - 4;
            text.draw(c, Face::Body, HINT_SIZE, label_x, hint_base, if selected { 0xffffff } else { 0xb8b4ac }, &hint);
            ry += row_h;
        }
        if !note.is_empty() {
            let line_y = ry + 14;
            c.rect(x + BTN_PAD, line_y, inner_w, 2, 0x3a3a40);
            let mut ty = line_y + 18 + crate::type_scale::px(HINT_SIZE) as i32;
            for l in &note {
                text.draw(c, Face::Body, HINT_SIZE, x + BTN_PAD, ty, 0xb8b4ac, l);
                ty += note_step;
            }
        }
        self.hits.list(sel, choices.len(), 0.0, row_h as f32);
        // The hints of what is underneath (the shelf's, the pause menu's) would peek out beside ours.
        let band = PAD + PILL + 12;
        c.rect(0, c.h - band, c.w, band, VOID);
        self.hints(c, text, &[("B", "Back"), ("A", "OK")]);
    }

    // ---- time -------------------------------------------------------------------------------------------------------

    fn paint_time(&mut self, c: &mut Canvas, text: &mut Text) {
        let Screen::Time(page) = &self.screen else { return };
        // For the snapshot checks of Settings → Text size: a page of its own over whatever was behind.
        crate::text::boxes::covered();
        let top = PAD + PILL + 12;
        match page {
            TimePage::Summary { sel } => {
                let sel = *sel;
                // From the worker's look: "…" until it is in (never a read here).
                let zone = match &self.view.timezone {
                    None => "…".to_string(),
                    Some(z) => z.clone().unwrap_or_else(|| "unknown".into()),
                };
                let ntp = if self.view.busy_sync {
                    "Asking a time server…"
                } else {
                    match self.view.ntp.unwrap_or(NtpState::Unknown) {
                        NtpState::Synced => "Synced",
                        NtpState::Waiting => "Waiting for a server",
                        NtpState::Off => "Off",
                        NtpState::Unknown => "Unknown",
                    }
                };
                self.header(c, text, "Time");
                // Date and time by hand: not while network time has set the clock (nothing to correct).
                let synced = self.view.ntp == Some(NtpState::Synced);
                let mut by_hand = RowView::new(
                    "Date and time",
                    if self.view.busy_clock {
                        "Setting…"
                    } else if synced {
                        "From network time"
                    } else {
                        ">"
                    },
                );
                by_hand.dim = synced;
                let rows = [RowView::new("Time zone", zone.clone()), RowView::new("Network time", ntp), by_hand];
                self.paint_rows(c, text, top, &rows, sel, c.h);
                let offset = match self.view.zone_offset {
                    None => "…".to_string(),
                    Some(o) => o.map_or("unknown".to_string(), timezone::fmt_offset),
                };
                // A snapshot of a wrong clock paints the made-up time (`Sys::clock_fake`).
                let (shown, at) = (self.clock_fake, self.clock_fake.unwrap_or_else(unix_now));
                let local = shown.map_or_else(clock, |t| {
                    let tm = local_tm_at(t);
                    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
                });
                let info = [("Local time", local), ("Date", date_label_at(shown)), ("Offset", offset)];
                for (i, (name, value)) in info.iter().enumerate() {
                    let base = top + 4 * row() + i as i32 * row() + row() / 2 + crate::type_scale::mid(FONT);
                    text.draw(c, Face::Body, FONT, PAD + BTN_PAD, base, pal().dust, name);
                    text.draw_right(c, Face::Body, FONT, c.w - PAD - BTN_PAD, base, pal().text, value);
                }
                let (desc, wrong) = self.time_note(sel, at);
                // Date and time while network time has set the clock: nothing to open (A only says why).
                let hints: &[(&str, &str)] = if sel == BY_HAND_ROW && synced { &[("B", "Back")] } else { &[("B", "Back"), ("A", "Open")] };
                self.footer(c, text, hints, &desc, if wrong == Clock::Fine { pal().text } else { pal().amber });
            }
            TimePage::Regions { sel } => {
                let sel = *sel;
                let mut rows = vec![RowView::new("Search…", ">")];
                rows.extend(timezone::regions(self.zone_list()).into_iter().map(|r| RowView::new(r, ">")));
                self.header(c, text, "Time zone");
                self.paint_rows(c, text, top, &rows, sel, c.h - PAD - PILL - 30);
                // While the list is still being read: Search… works, the regions follow.
                let desc = if self.view.zones.is_none() { "Looking at time zones…" } else { "" };
                self.footer(c, text, &[("L1 R1", "Page"), ("B", "Back"), ("A", "Open")], desc, pal().dust);
            }
            TimePage::Zones { heading, zones, sel } => {
                let (heading, zones, sel) = (heading.clone(), zones.clone(), *sel);
                let current = self.view.timezone.clone().flatten();
                let now = self.now_unix;
                let mut rows = Vec::new();
                for z in &zones {
                    // The offset is blank until the worker has read it.
                    let off = self.offset(z).flatten();
                    let value = off.map_or(String::new(), |o| format!("{}   {}", timezone::fmt_offset(o), clock_at(now, o)));
                    let mut v = RowView::new(timezone::city_of(z), value);
                    v.current = current.as_deref() == Some(z.as_str());
                    rows.push(v);
                }
                self.header(c, text, &heading);
                if rows.is_empty() {
                    text.draw(c, Face::Body, FONT, PAD + BTN_PAD, top + 60, pal().text, "No time zone matches that.");
                    text.draw(
                        c,
                        Face::Body,
                        SMALL,
                        PAD + BTN_PAD,
                        top + 110,
                        pal().dust,
                        "Try part of a city name, such as vie or york.",
                    );
                } else {
                    self.paint_rows(c, text, top, &rows, sel, c.h - PAD - PILL - 30);
                }
                self.hints(c, text, &[("L1 R1", "Page"), ("B", "Back"), ("A", "Set")]);
            }
            TimePage::SetClock(picker) => {
                let picker = *picker;
                self.header(c, text, "Date and time");
                self.paint_picker(c, text, top, &picker);
                self.footer(c, text, &[("B", "Cancel"), ("A", "Set")], "Up and Down change the value, Left and Right move between them.", pal().dust);
            }
            TimePage::Search(kb) => {
                c.clear(pal().void);
                text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, "Search time zones");
                crate::setup::paint_keyboard_with(c, text, kb, "Search");
                crate::setup::hints(
                    c,
                    text,
                    &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", "Search")],
                );
            }
        }
    }
}

impl Sys {
    /// The Time page's line at the bottom, and what `clock_check` says of the clock at `at`: a problem with the clock
    /// (always with a way forward that works offline), else what the selected row does.
    pub(crate) fn time_note(&self, sel: usize, at: i64) -> (String, Clock) {
        let ntp = self.view.ntp.unwrap_or(NtpState::Unknown);
        let clock = clock_check::check(at, clock_check::built_unix(), ntp, self.view.wifi, self.clock_by_hand);
        let line = if let Some(line) = clock_check::page_line(clock, self.view.wifi) {
            line
        } else if sel == TIME_ZONE_ROW {
            "Pick your region and city. The clock changes at once."
        } else if sel == BY_HAND_ROW {
            match ntp {
                NtpState::Synced => "Network time has set the clock over Wi-Fi, so there is nothing to set by hand.",
                NtpState::Off => "Set the date and time by hand. Network time is off, so the clock keeps what you set.",
                _ if self.clock_by_hand => "Set by hand. Once Wi-Fi reaches a time server, network time sets the clock again.",
                _ => "Set the date and time by hand. Once Wi-Fi reaches a time server, network time sets it again.",
            }
        } else if ntp == NtpState::Waiting {
            "The clock is set from a time server over Wi-Fi. No answer yet: check that Wi-Fi is connected. A asks again."
        } else {
            "The clock sets itself from a time server over Wi-Fi. A asks again now."
        };
        (line.to_string(), clock)
    }

    /// Date and time's five fields in boxes, the selected one light with arrows above and below, a label over each;
    /// under them the date with its weekday and the zone it is in. Touch: a field selects it, the top and bottom half
    /// of the selected one are Up and Down.
    fn paint_picker(&mut self, c: &mut Canvas, text: &mut Text, top: i32, p: &Picker) {
        const BIG: u32 = 56;
        const LABEL: u32 = SMALL;
        // Each box as wide as its widest value ("2026", "May", "28", "23", "59"), so nothing moves while it changes.
        let widest = |f: usize| -> &'static [&'static str] {
            match f {
                0 => &["2088"],
                1 => &["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
                _ => &["88"],
            }
        };
        let pad = 28;
        let widths: Vec<i32> = (0..manual_time::FIELDS.len())
            .map(|f| {
                let v = widest(f).iter().map(|s| text.width(Face::Title, BIG, s)).max().unwrap_or(0);
                (v + 2 * pad).max(text.width(Face::Body, LABEL, manual_time::FIELDS[f]) + 16)
            })
            .collect();
        // The date's three, a wider gap, the time's two.
        let (gap, mid_gap) = (16, 56);
        let total: i32 = widths.iter().sum::<i32>() + 3 * gap + mid_gap;
        let mut x = (c.w - total) / 2;
        let box_h = crate::type_scale::row(104, BIG);
        let label_base = top + 40 + crate::type_scale::line(LABEL, 16);
        let box_top = label_base + 52;
        for (f, w) in widths.iter().copied().enumerate() {
            let selected = f == p.sel;
            let label = manual_time::FIELDS[f];
            let lw = text.width(Face::Body, LABEL, label);
            text.draw(c, Face::Body, LABEL, x + (w - lw) / 2, label_base, if selected { pal().text } else { pal().dust }, label);
            let (bg, fg) = if selected { (pal().select, pal().ink) } else { (pal().pill, pal().text) };
            if selected {
                c.round_rect(x - 4, box_top - 4, w + 8, box_h + 8, 22, pal().band);
            }
            c.round_rect(x, box_top, w, box_h, 18, bg);
            let value = p.value(f);
            let vw = text.width(Face::Title, BIG, &value);
            text.draw(c, Face::Title, BIG, x + (w - vw) / 2, box_top + box_h / 2 + crate::type_scale::mid(BIG), fg, &value);
            if selected {
                chevron(c, x + w / 2, box_top - 24, true, pal().chalk);
                chevron(c, x + w / 2, box_top + box_h + 24, false, pal().chalk);
                self.hits.button(Area::new(x - gap / 2, box_top - 40, w + gap, box_h / 2 + 40), Button::Up);
                self.hits.button(Area::new(x - gap / 2, box_top + box_h / 2, w + gap, box_h / 2 + 40), Button::Down);
            } else {
                let (b, n) = if f > p.sel { (Button::Right, f - p.sel) } else { (Button::Left, p.sel - f) };
                self.hits.presses(Area::new(x - gap / 2, label_base - 40, w + gap, box_top + box_h + 40 - (label_base - 40)), b, n as u8);
            }
            x += w + if f == 2 { mid_gap } else { gap };
        }
        // What it comes to: the date with its weekday, and in which zone the time is.
        let date_base = box_top + box_h + 40 + crate::type_scale::line(FONT, 30);
        let date = p.date_label();
        let dw = text.width(Face::Body, FONT, &date);
        text.draw(c, Face::Body, FONT, (c.w - dw) / 2, date_base, pal().text, &date);
        let zone = match self.view.timezone.clone().flatten() {
            Some(z) => format!("Local time in {}", timezone::city_of(&z)),
            None => "Local time on the Pocket".to_string(),
        };
        let zone = text.fit(Face::Body, SMALL, &zone, c.w - 4 * PAD);
        let zw = text.width(Face::Body, SMALL, &zone);
        text.draw(c, Face::Body, SMALL, (c.w - zw) / 2, date_base + crate::type_scale::line(SMALL, 18), pal().dust, &zone);
    }
}

/// A small filled triangle pointing up or down, centred on (cx, cy): drawn, not a glyph (docs/DESIGN.md).
fn chevron(c: &mut Canvas, cx: i32, cy: i32, up: bool, color: u32) {
    for i in 0..13 {
        let y = if up { cy - 6 + i } else { cy + 6 - i };
        c.rect(cx - i, y, 2 * i + 1, 1, color);
    }
}

/// A row's value before the first look in the background is in (a moment after the start): neutral, never a guess.
pub const NOT_YET: &str = "…";

impl Sys {
    /// A switch row's value; "…" while nothing was looked at yet (`Polled::seq` 0), "Not available" after.
    fn switch_value(&self, busy: bool, on: Option<bool>) -> String {
        if on.is_none() && !busy && self.view.seq == 0 {
            return NOT_YET.into();
        }
        switch_label(busy, on)
    }
}

fn switch_label(busy: bool, on: Option<bool>) -> String {
    if busy {
        return "Switching…".into();
    }
    match on {
        Some(true) => "On".into(),
        Some(false) => "Off".into(),
        None => "Not available".into(),
    }
}

/// A power menu entry's mark (`power_menu::Mark`), about 24 px, centred on (cx, cy): drawn, not a glyph
/// (docs/DESIGN.md). `bg` is what is behind it, for the cut-outs (the moon's bite, the gaps of the rings).
fn paint_mark(c: &mut Canvas, cx: i32, cy: i32, mark: power_menu::Mark, fg: u32, bg: u32) {
    use power_menu::Mark;
    match mark {
        Mark::Moon => {
            c.disc(cx, cy, 12, fg);
            c.disc(cx + 7, cy - 5, 10, bg);
        }
        Mark::Round => {
            // A ring open at the top right, an arrow head at the end of the arc pointing into the gap.
            c.ring(cx, cy, 12, 3, fg);
            c.rect(cx + 1, cy - 13, 13, 11, bg);
            for dy in -6..=6 {
                c.rect(cx - 1, cy - 11 + dy, 7 - dy.abs(), 1, fg);
            }
        }
        Mark::Power => {
            c.ring(cx, cy + 1, 12, 3, fg);
            c.rect(cx - 5, cy - 12, 10, 9, bg);
            c.rect(cx - 1, cy - 13, 3, 13, fg);
        }
        Mark::Shelf => {
            // Three spines of different heights on one shelf.
            for (dx, h) in [(-11, 20), (-3, 24), (5, 17)] {
                c.round_rect(cx + dx, cy + 12 - h, 6, h, 2, fg);
            }
            c.rect(cx - 13, cy + 12, 26, 2, fg);
        }
        Mark::Lines => {
            for dy in [-8, 0, 8] {
                c.round_rect(cx - 11, cy + dy - 1, 22, 3, 1, fg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_row_says_where_the_sound_goes() {
        assert_eq!(sound_label(None), NOT_YET, "before the first look");
        assert_eq!(sound_label(Some(&Sound::Speaker)), "Speaker");
        assert_eq!(sound_label(Some(&Sound::Headphones)), "Headphones");
        assert_eq!(sound_label(Some(&Sound::Bluetooth(Some("Soundcore Q30".into())))), "Bluetooth: Soundcore Q30");
        assert_eq!(sound_label(Some(&Sound::Bluetooth(None))), "Bluetooth", "no name from BlueZ: still says Bluetooth");
        assert_eq!(sound_label(Some(&Sound::Bluetooth(Some("WH-1000XM4 Wireless Headset".into())))), "Bluetooth: WH-1000XM4 Wi…");
        assert_eq!(sound_label(Some(&Sound::Unknown)), "Output unknown", "PipeWire silent: said, not blank");
    }

    #[test]
    fn a_muted_output_says_so_beside_its_level() {
        assert_eq!(volume_label(45, false), "45%");
        assert_eq!(volume_label(5, true), "5%, muted", "the device's report: 5% although muted");
        assert_eq!(volume_label(0, false), "Muted");
        assert_eq!(volume_label(0, true), "Muted");
    }

    #[test]
    fn local_time_in_another_zone() {
        // 2026-10-21 12:00 UTC
        assert_eq!(clock_at(1_792_584_000, 7200), "14:00");
        assert_eq!(clock_at(1_792_584_000, -4 * 3600), "08:00");
        assert_eq!(clock_at(1_792_584_000, 19800), "17:30");
        assert_eq!(clock_at(0, -3600), "23:00", "wraps past midnight");
    }

    #[test]
    fn list_windows_keep_the_selection_in_view() {
        assert_eq!(window_start(0, 8, 5), 0);
        assert_eq!(window_start(20, 8, 100), 16);
        assert_eq!(window_start(99, 8, 100), 92);
    }

    #[test]
    fn switch_words() {
        assert_eq!(switch_label(false, Some(true)), "On");
        assert_eq!(switch_label(false, Some(false)), "Off");
        assert_eq!(switch_label(true, Some(true)), "Switching…");
        assert_eq!(switch_label(false, None), "Not available");
    }
}

