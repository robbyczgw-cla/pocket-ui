//! Part of the pocket-ui binary: Settings in categories (`SETTINGS_GROUPS` in main.rs). START opens the short list
//! of categories, each with a line naming what is in it; A opens a category's list, which is the old Settings list
//! with only that category's rows (same values, same behaviour); B goes back to the categories with the cursor on the
//! category it came from.
//!
//! A category's list is `SettingsPage::List(row)` with `row` an index into the flat `SETTINGS`, as before: every page
//! that comes back with `List(settings_row(NAME))` (its B, a deep link's B) lands in its own category's list, and B
//! from there on the categories. Nothing here reads a file or starts work: the categories have no values, and a
//! category's list works out the values of its own rows only.
//!
//! A row's value (`settings_row_value`): switches show their state, pages a short value and ">" ("Home >",
//! "Shelf · Dark >", "332 games >"), cut with "…" to the room the name leaves. Look, Add games, Battery, Updates and
//! About come from memory; Wi-Fi's network, Remote access, Time's zone and the paired PCs need a process or a file
//! and come from `settings_values`' thread (`settings_values_refresh`, when a list with one of them is painted, at
//! most every 5 s); until then the row shows ">" alone.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use pocket_ui::settings_values::{self as values, page};
use pocket_ui::setup::Only;
use pocket_ui::update;

use super::app_motion;
use super::{window_start, App, Opens, GAMES_BLURB_WITHOUT_STORE, SettingsPage, SettingsRow, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL, SETTINGS, SETTINGS_GROUPS};

/// The most lines a category's line may take under its name. Two hold every line at Large text in English and German
/// (`every_category_line_fits_its_row`); a third would only be cut with "…".
const BLURB_LINES: usize = 2;
/// The gap between the lines of a category's line (`type_scale::line`).
const BLURB_GAP: i32 = 12;

/// A category's line `s` in the lines it takes in `room` pixels: one when it fits, as in Normal text.
fn blurb_lines(text: &mut pocket_ui::text::Text, s: &str, room: i32) -> Vec<String> {
    text.wrap_at_most(Face::Body, NU_SMALL, s, room, BLURB_LINES)
}

/// Whether a row's value needs `settings_values`' thread.
fn needs_facts(opens: Opens) -> bool {
    matches!(opens, Opens::Setup(Only::Wifi | Only::Remote) | Opens::Time | Opens::PcStreaming)
}

/// How many rows the categories hold together: the length of `SETTINGS`.
pub(crate) const fn count() -> usize {
    let (mut g, mut n) = (0, 0);
    while g < SETTINGS_GROUPS.len() {
        n += SETTINGS_GROUPS[g].2.len();
        g += 1;
    }
    n
}

/// The rows of every category, one category after the other: `SETTINGS`.
pub(crate) const fn flat<const N: usize>() -> [SettingsRow; N] {
    let mut out = [SETTINGS_GROUPS[0].2[0]; N];
    let (mut g, mut k) = (0, 0);
    while g < SETTINGS_GROUPS.len() {
        let rows = SETTINGS_GROUPS[g].2;
        let mut r = 0;
        while r < rows.len() {
            out[k] = rows[r];
            k += 1;
            r += 1;
        }
        g += 1;
    }
    out
}

/// Where category `g`'s rows are in `SETTINGS`, shown or not.
pub(crate) fn span(g: usize) -> Range<usize> {
    let start: usize = SETTINGS_GROUPS[..g].iter().map(|c| c.2.len()).sum();
    start..start + SETTINGS_GROUPS[g].2.len()
}

/// Whether the Store row is there: only when a store catalog is set up (`store::catalog_url`). There is no public
/// catalog yet, so on most devices the row, and the page behind it, would only say "Couldn't reach the store". Set
/// once at the start from settings.json (`set_store_offered`); the row comes back by itself when a catalog is set.
static STORE_OFFERED: AtomicBool = AtomicBool::new(false);

/// Called at the start, with the answer of `store::catalog_url(..).is_some()`.
pub(crate) fn set_store_offered(on: bool) {
    STORE_OFFERED.store(on, Ordering::Relaxed);
}

/// Whether Settings has a row for `opens`, with the Store `store`-offered or not. The LEDs row (the lights around the
/// sticks) is left out on a device without sticks (the H700 handhelds).
fn shown(opens: Opens, store: bool) -> bool {
    shown_on(opens, store, pocket_ui::device::current().has_sticks())
}

fn shown_on(opens: Opens, store: bool, sticks: bool) -> bool {
    (store || !matches!(opens, Opens::Store)) && (sticks || !matches!(opens, Opens::Leds))
}

/// The rows of category `g` that are shown, as indexes into `SETTINGS`, in their order. Painting, moving, touch and
/// the hint bar all count in this list, so a row that is not there leaves no gap and no dead tap.
pub(crate) fn rows_with(g: usize, store: bool) -> Vec<usize> {
    span(g).filter(|i| shown(SETTINGS[*i].1, store)).collect()
}

/// `rows_with` as this device has it.
pub(crate) fn rows(g: usize) -> Vec<usize> {
    rows_with(g, STORE_OFFERED.load(Ordering::Relaxed))
}

/// The line naming what is in category `g`: the Games line leaves out the Store while it is not offered.
pub(crate) fn blurb(g: usize) -> &'static str {
    if g == group_of(SETTINGS.iter().position(|s| matches!(s.1, Opens::Store)).unwrap_or(0)) && !STORE_OFFERED.load(Ordering::Relaxed) {
        GAMES_BLURB_WITHOUT_STORE
    } else {
        SETTINGS_GROUPS[g].1
    }
}

/// The category row `row` of `SETTINGS` is in.
pub(crate) fn group_of(row: usize) -> usize {
    (0..SETTINGS_GROUPS.len()).find(|g| span(*g).contains(&row)).unwrap_or(0)
}

/// Up, Down, A and B on the categories, and Up, Down and B in a category's list (A there is the row's own business,
/// `App::settings_input`). `None` closes Settings.
pub(crate) fn groups_input(page: SettingsPage, b: Button) -> Option<SettingsPage> {
    groups_input_with(page, b, STORE_OFFERED.load(Ordering::Relaxed))
}

/// `groups_input` with the Store row there (`store`) or not.
pub(crate) fn groups_input_with(page: SettingsPage, b: Button, store: bool) -> Option<SettingsPage> {
    let n = SETTINGS_GROUPS.len();
    match (page, b) {
        (SettingsPage::Groups(g), Button::Up) => Some(SettingsPage::Groups((g + n - 1) % n)),
        (SettingsPage::Groups(g), Button::Down) => Some(SettingsPage::Groups((g + 1) % n)),
        (SettingsPage::Groups(g), b) if b == CONFIRM => Some(SettingsPage::List(rows_with(g, store)[0])),
        (SettingsPage::Groups(_), b) if b == CANCEL => None,
        // Moving stays in the category and goes round at its ends, as the long list did.
        (SettingsPage::List(row), Button::Up | Button::Down) => {
            let r = rows_with(group_of(row), store);
            let (at, len) = (r.iter().position(|i| *i == row).unwrap_or(0), r.len());
            let at = if b == Button::Up { (at + len - 1) % len } else { (at + 1) % len };
            Some(SettingsPage::List(r[at]))
        }
        (SettingsPage::List(row), b) if b == CANCEL => Some(SettingsPage::Groups(group_of(row))),
        (page, _) => Some(page),
    }
}

/// The icon of each category of `SETTINGS_GROUPS`, in its order (the length keeps one each).
const ICONS: [pocket_ui::ui_art::Icon; SETTINGS_GROUPS.len()] = {
    use pocket_ui::ui_art::Icon;
    [Icon::Look, Icon::Playing, Icon::Games, Icon::Connections, Icon::System]
};

impl App {
    /// The value at the right of `SETTINGS[i]` (`sel`: the selected row, whose switches show ‹ ›), at most `room`
    /// pixels wide. Nothing here reads a file or starts a process.
    pub(crate) fn settings_row_value(&mut self, i: usize, sel: usize, room: i32) -> String {
        let facts = values::get();
        let value = match SETTINGS[i].1 {
            Opens::Haptics => values::on_off(pocket_ui::rumble::menu_haptics()).to_string(),
            Opens::OnStart if i == sel => format!("‹ {} ›", self.on_start.label()),
            Opens::OnStart => self.on_start.label().to_string(),
            Opens::RememberQuit => values::on_off(self.remember_quit).to_string(),
            Opens::Animations if i == sel => format!("‹ {} ›", self.animations_in_use().name()),
            Opens::Animations => self.animations_in_use().name().to_string(),
            Opens::ButtonLayout if i == sel => format!("‹ {} ›", self.button_layout.label()),
            Opens::ButtonLayout => self.button_layout.label().to_string(),
            Opens::Frames if i == sel => format!("‹ {} ›", self.frames.label()),
            Opens::Frames => self.frames.label().to_string(),
            Opens::OverGame if i == sel => format!("‹ {} ›", self.over_game.label()),
            Opens::OverGame => self.over_game.label().to_string(),
            // In its own name, never translated (`i18n::Cache::get`).
            Opens::Language if i == sel => format!("‹ {} ›", pocket_ui::i18n::current().name()),
            Opens::Language => pocket_ui::i18n::current().name().to_string(),
            Opens::TextSize if i == sel => format!("‹ {} ›", pocket_ui::type_scale::get().label()),
            Opens::TextSize => pocket_ui::type_scale::get().label().to_string(),
            Opens::Tour => self.tour_list_value(),
            Opens::Sounds if i == sel && self.sound.on => format!("‹ {} ›", self.sound.label()),
            Opens::Sounds => self.sound.label(),
            Opens::Sync => page(values::on_off(self.sync.prefs.enabled)),
            // The free space of the last look (at start, after each game): memory only (`app_storage`).
            Opens::Storage => self.storage_list_value(),
            // How many favorites wait to be added: memory only (`app_gamelist`).
            Opens::Gamelist => self.gamelist_list_value(),
            // How many screenshots there are: memory only (`app_gallery`).
            Opens::Screenshots => self.gallery_list_value(),
            Opens::Leds => page(self.leds.summary()),
            Opens::NightLight => page(&self.night.summary()),
            Opens::Tailscale => self.tailscale_list_value(),
            Opens::Web => self.web_list_value(),
            Opens::Bluetooth => self.bluetooth_list_value(),
            Opens::Curator => self.curator_list_value(),
            Opens::Achievements => self.achievements_list_value(),
            Opens::SleepPower => page(&self.sleep_power_value()),
            Opens::PerfFan => page(&values::perf_fan(self.perf_prefs().games_performance, self.perf_prefs().games_fan)),
            Opens::Setup(Only::Look) => page(&values::look(self.theme, self.appearance)),
            Opens::Setup(Only::Games) => page(&values::games(self.systems.iter().map(|s| s.games.len()).sum())),
            Opens::Setup(Only::Wifi) => page(&facts.wifi.map(|w| w.label()).unwrap_or_default()),
            Opens::Setup(Only::Remote) => page(facts.ssh.map(values::on_off).unwrap_or_default()),
            Opens::Time => page(&facts.zone.as_deref().map(values::zone).unwrap_or_default()),
            Opens::PcStreaming => page(&facts.pcs.map(values::pcs).unwrap_or_default()),
            // The figure alone: "charging" is in the pill above.
            Opens::Battery => page(&pocket_ui::status_corner::status().battery.map(|b| format!("{}%", b.percent)).unwrap_or_default()),
            Opens::Updates => {
                let newer = self.updates.manifest.as_ref().and_then(|m| m.app.as_ref()).map(|a| a.version.as_str()).filter(|v| update::newer(v, update::app_version()));
                page(&values::updates(update::app_version(), newer))
            }
            Opens::About => page(update::app_version()),
            _ => ">".to_string(),
        };
        let max_w = room.min(values::MAX_W);
        values::fit(&value, max_w, |s| self.text.width(Face::Body, 36, s))
    }

    /// While a category's list with a value from `settings_values`' thread is painted: a read when one is due.
    pub(crate) fn settings_values_refresh(&self, rows: &[usize]) {
        if !rows.iter().any(|r| needs_facts(SETTINGS[*r].1)) {
            return;
        }
        let proxy = self.proxy.clone();
        let watcher = proxy.clone();
        values::watch(&self.sys.net(), move || {
            let _ = watcher.send_event(UserEvent::SettingsValues);
        });
        values::refresh(self.sys.shared_hw(), self.sys.net(), self.data_dir.clone(), move || {
            let _ = proxy.send_event(UserEvent::SettingsValues);
        });
    }

    /// The categories: an icon, name and `>` on a Settings row, the line naming what is in it under it.
    pub(crate) fn paint_settings_groups(&mut self, c: &mut Canvas, sel: usize) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let n = SETTINGS_GROUPS.len();
        // The lines under the names, wrapped to the room (one each in Normal; with larger text or in German some take a
        // second). Every row is as high as the tallest, so the rows keep one pitch for the touch map and the motion.
        let lines: Vec<Vec<String>> = (0..n).map(|g| blurb_lines(&mut self.text, blurb(g), right - pad - 2 * NU_BTN_PAD)).collect();
        let most = lines.iter().map(Vec::len).max().unwrap_or(1);
        // A row and the lines under it (both taller with Settings → Text size).
        let pitch = row + pocket_ui::type_scale::row(52, NU_SMALL) + (most as i32 - 1) * pocket_ui::type_scale::line(NU_SMALL, BLURB_GAP);
        self.sheet_header(c, "Settings");
        // Five fit with room to spare; should more ever not, the window follows the selection.
        let visible = (((c.h - pad - NU_PILL - 16 - top) / pitch) as usize).max(1);
        let first = window_start(sel, visible, n);
        let now = Instant::now();
        let (first_f, hl) = self.motion.list_at(app_motion::List::Settings, first, sel, now);
        // Each category's icon (`ui_art`, one colour, painted in the name's) before its name, which moves right for it.
        let icon_k = 2;
        let indent = 16 * icon_k + 18;
        let sel_w = self.text.width(Face::Body, font, SETTINGS_GROUPS[sel].0) + 2 * NU_BTN_PAD + indent;
        let pill_w = self.motion.width_at(app_motion::List::Settings, sel_w, now);
        let mut band = c.rows(top, top + visible as i32 * pitch);
        let hy = ((hl - first_f) * pitch as f32).round() as i32;
        band.round_rect(pad, hy + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
        band.round_rect(pad, hy + 4, pill_w, row - 8, (row - 8) / 2, pal().select);
        for (g, k) in app_motion::rows_in_view(first_f, visible, n) {
            let name = SETTINGS_GROUPS[g].0;
            let y = (k * pitch as f32).round() as i32;
            if (0.0..visible as f32 - 0.5).contains(&k) {
                self.hits.row(Area::new(pad, top + y, right - pad, pitch), g);
                self.motion.row(pad, top + y, right - pad, pitch);
            }
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let under = (app_motion::under(g, hl) * 255.0) as u32;
            let ink = mix(pal().text, pal().ink, under);
            pocket_ui::ui_art::paint_tinted(&mut band, pocket_ui::ui_art::Art::Icon(ICONS[g]), pad + NU_BTN_PAD, y + row / 2 - 8 * icon_k, icon_k, ink);
            self.text.draw(&mut band, Face::Body, font, pad + NU_BTN_PAD + indent, base, ink, name);
            self.text
                .draw_right(&mut band, Face::Body, font, right - NU_BTN_PAD, base, mix(pal().text, pal().ink, under), ">");
            let color = if g == sel { pal().text } else { pal().quiet };
            for (k, line) in lines[g].iter().enumerate() {
                let base = y + row + 30 + pocket_ui::type_scale::grow(NU_SMALL) + k as i32 * pocket_ui::type_scale::line(NU_SMALL, BLURB_GAP);
                self.text.draw(&mut band, Face::Body, NU_SMALL, pad + NU_BTN_PAD, base, color, line);
            }
        }
        // With larger text or in German not every category fits: the bar at the edge says there are more (new-user test
        // 2026-10-30: three of five showed and nothing hinted at the other two).
        pocket_ui::kit::scrollbar(c, top, visible as i32 * pitch, first, visible, n);
        self.hits.list(sel, n, 0.0, pitch as f32);
        self.nu_hints(c, &[("B", "Back"), ("A", "Open")]);
    }
}


// ---- the host benchmark (`--snapshot bench-settings`, tests/settings_open.rs) ---------------------------------------

/// What `App::bench_settings` measured, in microseconds.
pub(crate) struct Bench {
    /// The first frame of the categories after the shelf's, as `draw` makes it (paint, page transition, quick
    /// settings, dialogs), and how many glyphs it had to draw for the first time.
    pub first_us: f64,
    pub first_glyphs: usize,
    /// And how many strings it ran through the German table's patterns (none: nothing on this page is made of numbers
    /// or names, and the lines `Text::fit` gives back are German already).
    pub first_patterns: usize,
    /// The same frame again, median of many.
    pub steady_us: f64,
    pub pill_us: f64,
    pub transition_us: f64,
}

impl App {
    /// Paints the categories page the way the first frame after START does (the shelf was painted before, so its
    /// glyphs are in the cache), then again and again for the median, and the status pill and the page transition
    /// alone. The numbers are the host's; the test (`tests/settings_open.rs`) guards their order of magnitude.
    pub(crate) fn bench_settings(&mut self, w: u32, h: u32) -> Bench {
        let mut buf = vec![0u32; (w * h) as usize];
        // The status corner's thread, as the running pocket-ui has it (Wi-Fi and the battery come from memory).
        pocket_ui::status_corner::start(|| {});
        // The thread that draws the letters ahead of time (`Text::prewarm`) has long been done when START is pressed.
        self.text.prewarm();
        self.text.finish_prewarm();
        // Thread CPU time, not wall time: the test runs next to compiles, and a preempted frame is not a slow one.
        let cpu = || pocket_ui::run_ahead::thread_cpu_ms() * 1000.0;
        let us = |t: f64| cpu() - t;
        // The shelf, as the player sees it before START.
        self.settings_page = None;
        for _ in 0..2 {
            self.hits.clear();
            self.paint(&mut Canvas::new(&mut buf, w, h));
        }
        let frame = |app: &mut App, buf: &mut [u32]| {
            let now = Instant::now();
            app.motion_frame_begin(now);
            app.paint_in_appearance(buf, w, h, &mut |app, c| {
                app.paint(c);
                app.motion_after_paint(c);
                app.sys.paint(c, &mut app.text, app.menu.is_none());
                if app.sys.is_open() {
                    app.status_corner();
                }
                app.paint_leave(c);
            });
        };
        // The shelf's own frame has set the page; START opens the categories.
        frame(self, &mut buf);
        self.settings_page = Some(SettingsPage::Groups(0));
        let (glyphs, patterns) = (self.text.drawn_on_the_spot(), self.text.pattern_runs());
        let t = cpu();
        frame(self, &mut buf);
        let (first_us, first_glyphs, first_patterns) = (us(t), self.text.drawn_on_the_spot() - glyphs, self.text.pattern_runs() - patterns);
        let median = |mut v: Vec<f64>| {
            v.sort_by(f64::total_cmp);
            v[v.len() / 2]
        };
        let (mut steady, mut pill, mut transition) = (vec![], vec![], vec![]);
        for _ in 0..101 {
            // Back to the shelf and to the categories again: a new page, a new transition each time.
            self.settings_page = None;
            frame(self, &mut buf);
            self.settings_page = Some(SettingsPage::Groups(0));
            let t = cpu();
            frame(self, &mut buf);
            steady.push(us(t));
            let t = cpu();
            self.status_pill(&mut Canvas::new(&mut buf, w, h));
            pill.push(us(t));
            // The transition alone, on a finished picture.
            self.settings_page = None;
            frame(self, &mut buf);
            self.settings_page = Some(SettingsPage::Groups(0));
            self.motion_frame_begin(Instant::now());
            self.paint(&mut Canvas::new(&mut buf, w, h));
            let t = cpu();
            self.motion_after_paint(&mut Canvas::new(&mut buf, w, h));
            transition.push(us(t));
        }
        Bench { first_us, first_glyphs, first_patterns, steady_us: median(steady), pill_us: median(pill), transition_us: median(transition) }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{app_achievements, app_backup, app_curator, app_gamelist, app_moonlight, app_night_light, app_perf_fan, app_storage, app_tools, settings_row, Opens, SETTINGS};
    use super::*;

    fn category(name: &str) -> usize {
        SETTINGS_GROUPS
            .iter()
            .position(|c| c.0 == name)
            .unwrap_or_else(|| panic!("no category {name:?}"))
    }

    /// Each category's line fits under its name at every text size, in English and in German: in one line at Normal,
    /// in at most two (whole, never cut with "…") at Medium and Large, and the rows they make still leave room for
    /// at least three categories on the page. The first-day run (2026-10-26) found Playing and System cut short at
    /// Normal; the device found every long line cut at Medium and Large.
    #[test]
    fn every_category_line_fits_its_row() {
        use pocket_ui::type_scale::{self, TextSize};
        let mut text = pocket_ui::text::Text::new();
        text.set_translate(false);
        // The page's room and height (`paint_settings_groups`) on the device's screen.
        let (w, h) = (1240, 1080);
        let room = w - 2 * NU_PAD - 2 * NU_BTN_PAD;
        let mut lines: Vec<&str> = SETTINGS_GROUPS.iter().map(|g| g.1).collect();
        lines.push(GAMES_BLURB_WITHOUT_STORE);
        for size in [TextSize::Normal, TextSize::Medium, TextSize::Large] {
            type_scale::set(size);
            let mut most = 1;
            for en in &lines {
                let de = pocket_ui::i18n::german().translate(en).unwrap_or_else(|| panic!("no German line for {en:?}"));
                for s in [*en, de.as_ref()] {
                    let shown = blurb_lines(&mut text, s, room);
                    assert!(!shown.iter().any(|l| l.ends_with('…')), "{size:?}: {s:?} is cut: {shown:?}");
                    assert_eq!(shown.join(" "), s, "{size:?}: {s:?} is shown whole");
                    assert!(size != TextSize::Normal || shown.len() == 1, "Normal text keeps one line: {s:?} takes {}", shown.len());
                    most = most.max(shown.len());
                }
            }
            // The page: the pill and title above, the hints below; a row is the name and its lines.
            let pitch = type_scale::row(66, 36) + type_scale::row(52, NU_SMALL) + (most as i32 - 1) * type_scale::line(NU_SMALL, BLURB_GAP);
            let visible = (h - NU_PAD - NU_PILL - 16 - (NU_PAD + NU_PILL + 12)) / pitch;
            assert!(visible >= 3, "{size:?}: {most} lines make rows of {pitch} px, {visible} on a page");
            assert!(size != TextSize::Normal || visible >= SETTINGS_GROUPS.len() as i32, "Normal shows all categories at once");
        }
        type_scale::set(TextSize::Normal);
    }

    #[test]
    fn a_device_without_sticks_has_no_stick_lights_row() {
        let leds = SETTINGS.iter().position(|s| matches!(s.1, Opens::Leds)).unwrap();
        assert!(shown_on(Opens::Leds, true, true) && !shown_on(Opens::Leds, true, false));
        assert!(shown_on(Opens::Tools, true, false), "everything else stays");
        assert!(rows_with(group_of(leds), true).contains(&leds), "the test machine has sticks (it is no H700)");
    }

    #[test]
    fn the_flat_list_is_the_categories_one_after_another() {
        let names: Vec<&str> = SETTINGS_GROUPS.iter().flat_map(|c| c.2.iter().map(|r| r.0)).collect();
        assert_eq!(SETTINGS.iter().map(|r| r.0).collect::<Vec<_>>(), names);
        for g in 0..SETTINGS_GROUPS.len() {
            for row in span(g) {
                assert_eq!(group_of(row), g);
            }
        }
        // Short lists: the point of the categories. Ten at most (Look & feel since Cozy extras): a list is never longer
        // than one page, also with Large text, whose taller rows leave room for ten (1080 - 268 px at 76 px a row).
        assert!(SETTINGS_GROUPS.iter().all(|c| (3..=10).contains(&c.2.len())), "every category holds 3 to 10 rows");
        let large_row = pocket_ui::type_scale::grown_in(pocket_ui::type_scale::TextSize::Large, 36) as i32 - 36;
        assert!(10 * (66 + large_row * 6 / 5) <= 1080 - 268, "ten Large rows fit on the page");
        let mut seen = std::collections::BTreeSet::new();
        assert!(SETTINGS.iter().all(|r| seen.insert(r.0)), "no row twice: settings_row finds rows by name");
    }

    /// Every name a page comes back to with `settings_row` (B on the page, and so B after a deep link to it) is a
    /// row: `settings_row` falls back to the first row, which would land in the wrong category.
    #[test]
    fn every_page_comes_back_to_its_own_row_and_then_to_its_category() {
        let back = [
            ("LEDs", "Look & feel"),
            (app_night_light::SETTINGS_NAME, "Look & feel"),
            (app_perf_fan::SETTINGS_NAME, "Playing"),
            ("Battery", "Playing"),
            (app_achievements::SETTINGS_NAME, "Playing"),
            (app_curator::SETTINGS_NAME, "Playing"),
            (app_tools::SETTINGS_NAME, "Games"),
            (app_moonlight::SETTINGS_NAME, "Games"),
            (app_gamelist::SETTINGS_NAME, "Games"),
            ("Bluetooth", "Connections"),
            ("Tailscale", "Connections"),
            ("Sync", "Connections"),
            ("Updates", "System"),
            (app_storage::SETTINGS_NAME, "System"),
            (app_backup::SETTINGS_NAME, "System"),
            ("Tour", "System"),
            ("Manual", "System"),
            ("About", "System"),
        ];
        for (name, cat) in back {
            let row = settings_row(name);
            assert_eq!(SETTINGS[row].0, name, "{name} is a row of Settings");
            let list = SettingsPage::List(row);
            assert_eq!(group_of(row), category(cat), "{name} is under {cat}");
            // B from the page lands on its row; B from there on the categories, the cursor on its category.
            assert_eq!(groups_input(list, CANCEL), Some(SettingsPage::Groups(category(cat))), "{name}: B back to {cat}");
            assert_eq!(
                groups_input(SettingsPage::Groups(category(cat)), CANCEL),
                None,
                "B on the categories closes Settings"
            );
        }
        // The Store comes back by what it opens, not by name.
        let store = SETTINGS.iter().position(|s| matches!(s.1, Opens::Store)).unwrap();
        assert_eq!(group_of(store), category("Games"));
    }

    #[test]
    fn a_opens_a_category_at_its_first_row_and_b_returns_to_it() {
        for store in [false, true] {
            for g in 0..SETTINGS_GROUPS.len() {
                let rows = rows_with(g, store);
                let opened = groups_input_with(SettingsPage::Groups(g), CONFIRM, store);
                assert_eq!(opened, Some(SettingsPage::List(rows[0])));
                // Down to the last row, then B: the cursor is on the category it came from.
                let mut page = opened.unwrap();
                for _ in 1..rows.len() {
                    page = groups_input_with(page, Button::Down, store).unwrap();
                }
                assert_eq!(page, SettingsPage::List(*rows.last().unwrap()));
                assert_eq!(groups_input_with(page, CANCEL, store), Some(SettingsPage::Groups(g)));
            }
        }
    }

    #[test]
    fn moving_stays_in_the_category_and_goes_round_at_its_ends() {
        let r = rows_with(category("Connections"), false);
        let first = SettingsPage::List(r[0]);
        let last = SettingsPage::List(*r.last().unwrap());
        assert_eq!(groups_input(first, Button::Up), Some(last), "Up on the first row: the category's last");
        assert_eq!(groups_input(last, Button::Down), Some(first), "Down on the last row: the category's first");
        // The categories go round too, and Left/Right do nothing there.
        let n = SETTINGS_GROUPS.len();
        assert_eq!(groups_input(SettingsPage::Groups(0), Button::Up), Some(SettingsPage::Groups(n - 1)));
        assert_eq!(groups_input(SettingsPage::Groups(n - 1), Button::Down), Some(SettingsPage::Groups(0)));
        assert_eq!(groups_input(SettingsPage::Groups(2), Button::Right), Some(SettingsPage::Groups(2)));
    }

    /// No public catalog: the Store row is not there, nothing in the Games list points at it, and the rows around it
    /// close up (moving, going round, touch all count in `rows_with`). With a catalog it is back where it was.
    #[test]
    fn the_store_row_is_there_only_with_a_catalog() {
        let games = category("Games");
        let store = SETTINGS.iter().position(|s| matches!(s.1, Opens::Store)).unwrap();
        let (without, with) = (rows_with(games, false), rows_with(games, true));
        assert!(!without.contains(&store), "no catalog: no Store row");
        assert!(with.contains(&store), "a catalog: the Store row");
        assert_eq!(with.len(), without.len() + 1);
        assert!(without.iter().all(|i| !matches!(SETTINGS[*i].1, Opens::Store)));
        // Other categories are untouched.
        for g in (0..SETTINGS_GROUPS.len()).filter(|g| *g != games) {
            assert_eq!(rows_with(g, false), rows_with(g, true));
        }
        // The neighbours of the hidden row are neighbours now, in both directions.
        let before = SettingsPage::List(store - 1);
        let after = groups_input_with(before, Button::Down, false);
        assert_eq!(after, Some(SettingsPage::List(store + 1)), "Down skips the hidden row");
        assert_eq!(groups_input_with(after.unwrap(), Button::Up, false), Some(before), "Up skips it too");
        assert_eq!(groups_input_with(before, Button::Down, true), Some(SettingsPage::List(store)), "with a catalog Down lands on it");
        // Every shown row of Games still names what it opens (hints and touch read the same list).
        assert!(without.iter().all(|i| !SETTINGS[*i].0.is_empty() && !SETTINGS[*i].2.is_empty()));
    }

    /// The Games line names the Store only while the row is there. (The device default in a test run is "no
    /// catalog": nothing here sets `STORE_OFFERED`.)
    #[test]
    fn the_games_line_leaves_out_the_store_while_it_is_hidden() {
        let games = category("Games");
        assert!(SETTINGS_GROUPS[games].1.contains("Store"));
        assert!(!GAMES_BLURB_WITHOUT_STORE.contains("Store"));
        assert_eq!(blurb(games), GAMES_BLURB_WITHOUT_STORE);
        assert_eq!(blurb(category("Playing")), SETTINGS_GROUPS[category("Playing")].1);
    }
}
