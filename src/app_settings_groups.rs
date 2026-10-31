//! Part of the pocket-ui binary: Settings in categories (`SETTINGS_GROUPS` in main.rs). START opens the short list
//! of categories, one bare line each (icon and name, as NextUI's), the selected one's line naming what is in it at the
//! bottom as in every list; A opens a category's list, which is the old Settings list with only that category's rows
//! (same values, same behaviour); B goes back to the categories with the cursor on the category it came from.
//!
//! A category's list is `SettingsPage::List(row)` with `row` an index into the flat `SETTINGS`, as before: every page
//! that comes back with `List(settings_row(NAME))` (its B, a deep link's B) lands in its own category's list, and B
//! from there on the categories. Nothing here reads a file or starts work: the categories have no values, and a
//! category's list works out the values of its own rows only.
//!
//! A row's value (`settings_row_value`): switches show their state, pages a short value ("Home", "Shelf · Dark",
//! "332 games"); only the selected row adds its marks, ‹ › on a switch and ">" on a page (`settings_values::quiet`),
//! cut with "…" to the room the name leaves. Look, Add games, Battery, Updates and About come from memory; Wi-Fi's
//! network, Remote access, Time's zone and the paired PCs need a process or a file and come from `settings_values`'
//! thread (`settings_values_refresh`, when a list with one of them is painted, at most every 5 s); until then the row
//! shows nothing, or ">" when selected.

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
use super::{window_start, App, Opens, SettingsPage, SettingsRow, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL, SETTINGS, SETTINGS_GROUPS};

/// The most lines the selected row's line takes over the hints when it doesn't fit beside them; every row's and
/// category's line fits in two at Large text in English and German (`every_line_fits_at_the_bottom`).
const DESC_LINES: usize = 2;

/// The room a description `s` has: beside the hints that start at `hints_at`, or the screen's width over them.
fn desc_beside(hints_at: i32) -> i32 {
    hints_at - 24 - (NU_PAD + NU_BTN_PAD)
}

/// `s` in the lines it takes over the hints on a screen `w` pixels wide (`App::paint_desc`).
fn desc_lines(text: &mut pocket_ui::text::Text, s: &str, w: i32) -> Vec<String> {
    text.wrap_at_most(Face::Body, NU_SMALL, s, w - 2 * (NU_PAD + NU_BTN_PAD), DESC_LINES)
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
    shown_on(opens, store, pocket_ui::device::current().has_sticks(), pocket_ui::device::current().host_owned())
}

fn shown_on(opens: Opens, store: bool, sticks: bool, host_owned: bool) -> bool {
    (store || !matches!(opens, Opens::Store)) && !device_key(opens).is_some_and(|k| pocket_ui::device::hides(k, sticks, host_owned))
}

/// The pages a device may lack (`pocket_ui::device::hides`, also behind the Tour cards): the lights around the sticks;
/// on a Steam Deck the radios, fan, clock, updates, ROCKNIX's tools, Sync (ROCKNIX's Syncthing service) and Haptics (the
/// handhelds' motor). Sleep & power stays with its slideshow alone (`sleep_settings::rows`): SteamOS owns sleep and the
/// power button.
fn device_key(opens: Opens) -> Option<&'static str> {
    Some(match opens {
        Opens::Leds => "leds",
        Opens::Haptics => "haptics",
        Opens::Sync => "sync",
        Opens::Tailscale => "tailscale",
        Opens::Tools => "tools",
        Opens::PerfFan => "perf_fan",
        Opens::Setup(Only::Wifi | Only::Remote) | Opens::Bluetooth => "radios",
        Opens::Time => "time",
        Opens::Updates => "updates",
        Opens::Store => "store",
        Opens::Extra(pocket_ui::cozy_extras::Extra::FrozenGames) => "frozen_games",
        _ => return None,
    })
}

/// The rows of category `g` that are shown, as indexes into `SETTINGS`, in their order. Painting, moving, touch and
/// the hint bar all count in this list, so a row that is not there leaves no gap and no dead tap.
pub(crate) fn rows_with(g: usize, store: bool) -> Vec<usize> {
    span(g).filter(|i| shown(SETTINGS[*i].1, store)).collect()
}

/// `rows_with` for a made-up device (the Deck's lines are checked against the Deck's rows on any machine).
#[cfg(test)]
fn rows_on(g: usize, store: bool, sticks: bool, host_owned: bool) -> Vec<usize> {
    span(g).filter(|i| shown_on(SETTINGS[*i].1, store, sticks, host_owned)).collect()
}

/// The line under row `i` of `SETTINGS`: its own, except where the Steam Deck has less than the handhelds (no file sharing,
/// no PortMaster).
pub(crate) fn row_line(i: usize) -> &'static str {
    if pocket_ui::device::current().host_owned() {
        match SETTINGS[i].1 {
            Opens::Setup(Only::Games) => return "Where each system's games go.",
            Opens::Setup(Only::Extras) => return "Cover art and game pictures.",
            Opens::SleepPower => return "A slideshow of your games when the Deck sits idle.",
            _ => {}
        }
    }
    SETTINGS[i].2
}

/// `rows_with` as this device has it.
pub(crate) fn rows(g: usize) -> Vec<usize> {
    rows_with(g, STORE_OFFERED.load(Ordering::Relaxed))
}

/// The line naming what is in category `g` on this device, in the interface's language through `tr`: the parts of
/// rows that are not there (the Store without a catalog, LEDs without sticks, what SteamOS owns on a Steam Deck) left
/// out, as the list leaves them out.
pub(crate) fn blurb(g: usize, tr: impl FnMut(&str) -> String) -> String {
    let device = pocket_ui::device::current();
    blurb_with(g, STORE_OFFERED.load(Ordering::Relaxed), device.has_sticks(), device.host_owned(), tr)
}

/// `blurb` with the Store offered (`store`) or not, a device with sticks or without, owned by its host (the Deck) or not. A line has one part per row,
/// in the rows' order, also in German (`every_category_line_names_its_rows_in_order`), so the parts can be left out
/// by the row they stand for.
pub(crate) fn blurb_with(g: usize, store: bool, sticks: bool, host_owned: bool, mut tr: impl FnMut(&str) -> String) -> String {
    let line = tr(SETTINGS_GROUPS[g].1);
    let shown: Vec<bool> = span(g).map(|i| shown_on(SETTINGS[i].1, store, sticks, host_owned)).collect();
    let parts: Vec<&str> = line.trim_end_matches('.').split(", ").collect();
    if shown.iter().all(|s| *s) || parts.len() != shown.len() {
        return line;
    }
    let kept: Vec<&str> = parts.iter().zip(&shown).filter(|(_, s)| **s).map(|(p, _)| *p).collect();
    let joined = kept.join(", ");
    // A first part left out: the line still starts with a capital.
    let mut chars = joined.chars();
    chars.next().map_or_else(String::new, |c| format!("{}{}.", c.to_uppercase(), chars.as_str()))
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

/// Where Settings' switches without a page of their own stand (Look → Appearance, Colours, Font; Sound & feel →
/// Motion; Playing → Buttons' two): part of `App::sound_view`, so that switching one clicks as any other change does.
/// Each process-wide choice is read where it lives; the App's own two are handed in.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Switches {
    appearance: pocket_ui::settings::Appearance,
    layout: pocket_ui::button_layout::ButtonLayout,
    motion: pocket_ui::motion::Motion,
    colours: pocket_ui::palette::Colours,
    font: pocket_ui::font_style::FontStyle,
    prompts: pocket_ui::button_prompts::PromptStyle,
}

impl Switches {
    pub(crate) fn now(appearance: pocket_ui::settings::Appearance, layout: pocket_ui::button_layout::ButtonLayout) -> Switches {
        Switches {
            appearance,
            layout,
            motion: pocket_ui::motion::level(),
            colours: pocket_ui::palette::colours(),
            font: pocket_ui::font_style::get(),
            prompts: pocket_ui::button_prompts::get(),
        }
    }
}

/// The icon of each category of `SETTINGS_GROUPS`, in its order (the length keeps one each).
const ICONS: [pocket_ui::ui_art::Icon; SETTINGS_GROUPS.len()] = {
    use pocket_ui::ui_art::Icon;
    [Icon::Look, Icon::Sound, Icon::Playing, Icon::Games, Icon::Connections, Icon::System, Icon::Help]
};

/// Whether the value of row `i` is a network's own name, painted as typed: a Wi-Fi named "Off" or "Unknown" must not be
/// translated (the Wi-Fi page shows it as it is).
pub(crate) fn value_is_typed(i: usize) -> bool {
    typed_value(SETTINGS[i].1, &values::get().wifi)
}

fn typed_value(opens: Opens, wifi: &Option<values::Wifi>) -> bool {
    matches!(opens, Opens::Setup(Only::Wifi)) && matches!(wifi, Some(values::Wifi::Connected(Some(_))))
}

impl App {
    /// The value at the right of `SETTINGS[i]` (`sel`: the selected row, whose switches show ‹ ›), at most `room`
    /// pixels wide. Nothing here reads a file or starts a process.
    pub(crate) fn settings_row_value(&mut self, i: usize, sel: usize, room: i32) -> String {
        let facts = values::get();
        let value = match SETTINGS[i].1 {
            Opens::Haptics => values::on_off(pocket_ui::rumble::menu_haptics()).to_string(),
            Opens::Extra(e) if i == sel => format!("‹ {} ›", values::on_off(pocket_ui::cozy_extras::on(e))),
            Opens::Extra(e) => values::on_off(pocket_ui::cozy_extras::on(e)).to_string(),
            Opens::OnStart if i == sel => format!("‹ {} ›", self.on_start.label()),
            Opens::OnStart => self.on_start.label().to_string(),
            Opens::RememberQuit => values::on_off(self.remember_quit).to_string(),
            Opens::Animations if i == sel => format!("‹ {} ›", self.animations_in_use().name()),
            Opens::Animations => self.animations_in_use().name().to_string(),
            Opens::Buttons => page(&self.buttons_list_value()),
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
            Opens::Setup(Only::Look) => page(self.theme.name()),
            Opens::Colours if i == sel => format!("‹ {} ›", pocket_ui::palette::colours().label()),
            Opens::Colours => pocket_ui::palette::colours().label().to_string(),
            Opens::Font if i == sel => format!("‹ {} ›", pocket_ui::font_style::get().label()),
            Opens::Font => pocket_ui::font_style::get().label().to_string(),
            Opens::Appearance if i == sel => format!("‹ {} ›", self.appearance.name()),
            Opens::Appearance => self.appearance.name().to_string(),
            Opens::Motion if i == sel => format!("‹ {} ›", pocket_ui::motion::level().name()),
            Opens::Motion => pocket_ui::motion::level().name().to_string(),
            Opens::Setup(Only::Games) => page(&values::games(self.systems.iter().map(|s| s.games.len()).sum())),
            Opens::GameCheck => page(&self.game_check_value()),
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
        values::fit(&values::quiet(&value, i == sel), max_w, |s| self.text.width(Face::Body, 36, s))
    }

    /// The line under row `i` of `SETTINGS` as it is now: a cozy extra's note while another switch keeps it from
    /// showing (`App::extra_row_line`), else its own (`row_line`).
    pub(crate) fn settings_row_desc(&self, i: usize) -> &'static str {
        match SETTINGS[i].1 {
            Opens::Extra(e) => self.extra_row_line(e, row_line(i)),
            _ => row_line(i),
        }
    }

    /// The room a list keeps over its hints (that start at `hints_at`) for the selected row's line: none when every
    /// one of `lines` fits beside them, else two lines' worth, so the list doesn't shift as the selection moves.
    pub(crate) fn desc_room(&mut self, lines: &[&str], hints_at: i32) -> i32 {
        let beside = desc_beside(hints_at);
        if lines.iter().all(|l| self.text.width(Face::Body, NU_SMALL, l) <= beside) {
            0
        } else {
            DESC_LINES as i32 * pocket_ui::type_scale::line(NU_SMALL, 10) + 14
        }
    }

    /// The selected row's line `line` at the bottom: left of the hints (that start at `hints_at`), on their line, when it
    /// fits there; else over them in the room `desc_room` kept, wrapped, cut with "…" only past two lines.
    pub(crate) fn paint_desc(&mut self, c: &mut Canvas, line: &str, hints_at: i32) {
        let (x, beside) = (NU_PAD + NU_BTN_PAD, desc_beside(hints_at));
        if self.text.width(Face::Body, NU_SMALL, line) <= beside {
            let base = c.h - NU_PAD - NU_PILL / 2 + pocket_ui::type_scale::mid(NU_SMALL);
            self.text.draw(c, Face::Body, NU_SMALL, x, base, pal().text, line);
        } else {
            let lines = desc_lines(&mut self.text, line, c.w);
            let (last, step) = (c.h - NU_PAD - NU_PILL - 24, pocket_ui::type_scale::line(NU_SMALL, 10));
            for (n, l) in lines.iter().enumerate() {
                self.text.draw(c, Face::Body, NU_SMALL, x, last - (lines.len() - 1 - n) as i32 * step, pal().text, l);
            }
        }
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

    /// The categories: one bare line each, an icon and the name (">" on the selected one only), and the selected one's
    /// line naming what is in it at the bottom, as in every list. All seven fit on one page at Large text on 640x480.
    pub(crate) fn paint_settings_groups(&mut self, c: &mut Canvas, sel: usize) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let n = SETTINGS_GROUPS.len();
        self.sheet_header(c, "Settings");
        let hints_at = self.nu_hints(c, &[("B", "Back"), ("A", "Open")]);
        // In the interface's language already (`blurb`): measured and painted as they are, not looked up again.
        let lines: Vec<String> = (0..n).map(|g| blurb(g, |s| self.text.translated(s).to_string())).collect();
        let was = self.text.translating();
        self.text.set_translate(false);
        let reserved = self.desc_room(&lines.iter().map(String::as_str).collect::<Vec<_>>(), hints_at);
        self.text.set_translate(was);
        // Seven fit at every text size; a window that follows the selection only if a screen ever has less room.
        let visible = (((c.h - pad - NU_PILL - 16 - reserved - top) / row) as usize).max(1);
        let first = window_start(sel, visible, n);
        let now = Instant::now();
        let (first_f, hl) = self.motion.list_at(app_motion::List::Settings, first, sel, now);
        // Each category's icon (`ui_art`, one colour, painted in the name's) before its name, which moves right for it.
        let icon_k = 2;
        let indent = 16 * icon_k + 18;
        let sel_w = self.text.width(Face::Body, font, SETTINGS_GROUPS[sel].0) + 2 * NU_BTN_PAD + indent;
        let pill_w = self.motion.width_at(app_motion::List::Settings, sel_w, now);
        let mut band = c.rows(top, top + visible as i32 * row);
        let hy = ((hl - first_f) * row as f32).round() as i32;
        band.round_rect(pad, hy + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
        band.round_rect(pad, hy + 4, pill_w, row - 8, (row - 8) / 2, pal().select);
        for (g, k) in app_motion::rows_in_view(first_f, visible, n) {
            let name = SETTINGS_GROUPS[g].0;
            let y = (k * row as f32).round() as i32;
            if (0.0..visible as f32 - 0.5).contains(&k) {
                self.hits.row(Area::new(pad, top + y, right - pad, row), g);
                self.motion.row(pad, top + y, right - pad, row);
            }
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let ink = mix(pal().text, pal().ink, (app_motion::under(g, hl) * 255.0) as u32);
            pocket_ui::ui_art::paint_tinted(&mut band, pocket_ui::ui_art::Art::Icon(ICONS[g]), pad + NU_BTN_PAD, y + row / 2 - 8 * icon_k, icon_k, ink);
            self.text.draw(&mut band, Face::Body, font, pad + NU_BTN_PAD + indent, base, ink, name);
            if g == sel {
                self.text.draw_right(&mut band, Face::Body, font, right - NU_BTN_PAD, base, pal().text, ">");
            }
        }
        pocket_ui::kit::scrollbar(c, top, visible as i32 * row, first, visible, n);
        self.hits.list(sel, n, 0.0, row as f32);
        self.text.set_translate(false);
        self.paint_desc(c, &lines[sel], hints_at);
        self.text.set_translate(was);
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
    use super::super::{app_achievements, app_backup, app_cozy_extras, app_curator, app_gamelist, app_moonlight, app_night_light, app_perf_fan, app_storage, app_tools, settings_row, Opens, SETTINGS};
    use super::*;
    use pocket_ui::cozy_extras::Needs;

    fn category(name: &str) -> usize {
        SETTINGS_GROUPS
            .iter()
            .position(|c| c.0.trim_end_matches('\u{200b}') == name)
            .unwrap_or_else(|| panic!("no category {name:?}"))
    }

    /// The categories are bare rows (2026-10-30, NextUI's idea): all seven on one page at every text size on the
    /// handhelds' 960 logical lines (640x480 and 1280x720 are both painted at that height, `scaled_layout`), with room
    /// over the hints kept for the selected one's line; and every line, of a category or of a row, English or German,
    /// fits whole in the two lines it may take at the bottom at Large text (never cut with "…"). The first-day run
    /// (2026-10-26) found Playing and System cut short; with the lines under the names 640x480 Large showed 3 of 7.
    #[test]
    fn every_line_fits_at_the_bottom_and_all_categories_on_one_page() {
        use pocket_ui::type_scale::{self, TextSize};
        let mut text = pocket_ui::text::Text::new();
        text.set_translate(false);
        let de = |s: &str| pocket_ui::i18n::german().translate(s).unwrap_or_else(|| panic!("no German line for {s:?}")).to_string();
        // Every line as a device can show it (Store or not, sticks or not, the Deck), in English and in German.
        let mut lines: Vec<String> = Vec::new();
        for g in 0..SETTINGS_GROUPS.len() {
            for (store, sticks, deck) in [(true, true, false), (false, true, false), (true, false, false), (false, false, false), (false, false, true)] {
                lines.push(blurb_with(g, store, sticks, deck, str::to_string));
                lines.push(blurb_with(g, store, sticks, deck, de));
            }
        }
        for row in SETTINGS.iter() {
            lines.push(row.2.to_string());
            lines.push(de(row.2));
        }
        for note in [Needs::Sounds, Needs::CozyFull, Needs::MotionFull, Needs::RememberQuit].map(Needs::note) {
            lines.push(note.to_string());
            lines.push(de(note));
        }
        let deck_lines = ["Where each system's games go.", "Cover art and game pictures.", "A slideshow of your games when the Deck sits idle."];
        lines.extend(deck_lines.iter().flat_map(|l| [l.to_string(), de(l)]));
        for size in [TextSize::Normal, TextSize::Medium, TextSize::Large] {
            type_scale::set(size);
            for w in [1280, 1240, 1536] {
                for s in &lines {
                    let shown = desc_lines(&mut text, s, w);
                    assert!(!shown.iter().any(|l| l.ends_with('…')), "{size:?} {w}: {s:?} is cut: {shown:?}");
                    assert_eq!(shown.join(" "), *s, "{size:?} {w}: {s:?} is shown whole");
                }
            }
            // The page at 960 lines: pill and title above, the hints below, two lines' room for the selected line.
            let reserved = DESC_LINES as i32 * type_scale::line(NU_SMALL, 10) + 14;
            let visible = (960 - NU_PAD - NU_PILL - 16 - reserved - (NU_PAD + NU_PILL + 12)) / type_scale::row(66, 36);
            assert!(visible >= SETTINGS_GROUPS.len() as i32, "{size:?}: {visible} of {} categories on a page", SETTINGS_GROUPS.len());
        }
        type_scale::set(TextSize::Normal);
    }

    #[test]
    fn a_device_without_sticks_has_no_stick_lights_row() {
        let leds = SETTINGS.iter().position(|s| matches!(s.1, Opens::Leds)).unwrap();
        assert!(shown_on(Opens::Leds, true, true, false) && !shown_on(Opens::Leds, true, false, false));
        assert!(shown_on(Opens::Tools, true, false, false), "everything else stays");
        // The Steam Deck (docs/STEAMDECK.md): SteamOS owns these pages; the rest of Settings stays.
        for gone in [Opens::Setup(Only::Wifi), Opens::Bluetooth, Opens::PerfFan, Opens::Time, Opens::Updates, Opens::Tools, Opens::Sync, Opens::Haptics] {
            assert!(shown_on(gone, true, true, false) && !shown_on(gone, true, true, true), "hidden on a host-owned device only");
        }
        for kept in [Opens::Web, Opens::Storage, Opens::About, Opens::Language, Opens::Battery, Opens::SleepPower] {
            assert!(shown_on(kept, true, true, true), "still there on the Deck");
        }
        assert!(rows_with(group_of(leds), true).contains(&leds), "the test machine has sticks (it is no H700)");
    }

    #[test]
    fn the_tour_and_first_start_cards_follow_the_settings_rows() {
        use pocket_ui::features::{available_on, by_id, FEATURES};
        // A card exists on a device exactly when its Settings row does (no Stick lights on an H700 without sticks, no
        // Haptics, Sync, Tailscale, Tools or Performance & fan on the Deck).
        for (sticks, host_owned) in [(true, false), (false, false), (false, true)] {
            for (id, opens) in [("leds", Opens::Leds), ("haptics", Opens::Haptics), ("sync", Opens::Sync), ("tailscale", Opens::Tailscale), ("tools", Opens::Tools), ("perf_fan", Opens::PerfFan)] {
                let card = available_on(sticks, host_owned).any(|(_, f)| f.id == id);
                assert_eq!(card, shown_on(opens, true, sticks, host_owned), "{id} (sticks {sticks}, host-owned {host_owned})");
            }
        }
        let all = available_on(true, false).count();
        assert_eq!(all, FEATURES.iter().filter(|f| f.todo.is_none()).count(), "a handheld with sticks keeps every card");
        assert_eq!(available_on(false, false).count(), all - 1, "an H700 without sticks loses Stick lights only");
        assert_eq!(available_on(false, true).count(), all - 6, "the Deck loses the six");
        assert!(by_id("leds").is_some() || !pocket_ui::device::current().has_sticks());
    }

    #[test]
    fn a_connected_networks_name_is_painted_as_typed_not_translated() {
        let named = Some(values::Wifi::Connected(Some("Off".into())));
        assert!(typed_value(Opens::Setup(Only::Wifi), &named), "the Wi-Fi row with a name");
        assert!(!typed_value(Opens::Setup(Only::Games), &named), "no other row");
        for w in [Some(values::Wifi::Connected(None)), Some(values::Wifi::NotConnected), None] {
            assert!(!typed_value(Opens::Setup(Only::Wifi), &w), "Connected / Not connected stay words of the interface");
        }
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
        // Short lists: the point of the categories. Seven categories, eight rows at most
        // (2026-10-30): a list is never longer than one page, also with Large text.
        assert!(SETTINGS_GROUPS.len() <= 7, "at most seven categories");
        assert!(SETTINGS_GROUPS.iter().all(|c| (4..=8).contains(&c.2.len())), "every category holds 4 to 8 rows");
        let large_row = pocket_ui::type_scale::grown_in(pocket_ui::type_scale::TextSize::Large, 36) as i32 - 36;
        assert!(8 * (66 + large_row * 6 / 5) <= 1080 - 268, "eight Large rows fit on the page");
        let mut seen = std::collections::BTreeSet::new();
        assert!(SETTINGS.iter().all(|r| seen.insert(r.0)), "no row twice: settings_row finds rows by name");
    }

    /// Every name a page comes back to with `settings_row` (B on the page, and so B after a deep link to it) is a
    /// row: `settings_row` falls back to the first row, which would land in the wrong category.
    #[test]
    fn every_page_comes_back_to_its_own_row_and_then_to_its_category() {
        let back = [
            ("LEDs", "Sound & feel"),
            (app_cozy_extras::SETTINGS_NAME, "Look"),
            (super::super::app_sleep_power::SETTINGS_NAME, "System"),
            (app_night_light::SETTINGS_NAME, "Look"),
            (super::super::app_buttons::SETTINGS_NAME, "Playing"),
            (app_perf_fan::SETTINGS_NAME, "Playing"),
            ("Battery", "Playing"),
            (app_achievements::SETTINGS_NAME, "Playing"),
            (app_curator::SETTINGS_NAME, "Games"),
            (app_tools::SETTINGS_NAME, "Help & tools"),
            (app_moonlight::SETTINGS_NAME, "Games"),
            (app_gamelist::SETTINGS_NAME, "Games"),
            ("Bluetooth", "Connections"),
            ("Tailscale", "Connections"),
            ("Sync", "Connections"),
            ("Updates", "System"),
            (app_storage::SETTINGS_NAME, "System"),
            (app_backup::SETTINGS_NAME, "System"),
            ("Tour", "Help & tools"),
            ("Manual", "Help & tools"),
            ("About", "Help & tools"),
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
        assert_eq!(group_of(store), category("Help & tools"));
    }

    /// Settings in seven categories (2026-10-30): every choice has a row, none only on a button (Y, R1, L1, X, R2 on the look
    /// page were the only way to some), and each is in one place.
    #[test]
    fn every_choice_has_one_row_of_its_own() {
        for name in ["Look", "Appearance", "Colours", "Font", "Text size", "Systems on the shelf", "Animations", "Motion", "Buttons", "Language"] {
            assert_eq!(SETTINGS.iter().filter(|r| r.0 == name).count(), 1, "{name}: one row");
        }
        let opens: Vec<String> = SETTINGS.iter().map(|r| match r.1 {
            Opens::Setup(only) => format!("Setup({only:?})"),
            Opens::Extra(e) => format!("Extra({e:?})"),
            o => format!("{:?}", std::mem::discriminant(&o)),
        }).collect();
        let mut seen = std::collections::BTreeSet::new();
        assert!(opens.iter().all(|o| seen.insert(o)), "no page or switch twice: {opens:?}");
    }

    /// The line under each category names its rows, one part each, in their order, in English and in German: every
    /// part ("quit", "frames", "Bilder & Extras") is in its row's name or the other way round. A row added, moved or
    /// renamed without its line fails here; so does a German line naming a word the German rows don't use.
    #[test]
    fn every_category_line_names_its_rows_in_order() {
        let de = pocket_ui::i18n::german();
        let parts = |line: &str| -> Vec<String> { line.trim_end_matches('.').split(", ").map(|p| p.trim().to_lowercase()).collect() };
        for (store, sticks, deck) in [(true, true, false), (false, true, false), (true, false, false), (false, false, false), (false, false, true)] {
            for (g, (name, _, _)) in SETTINGS_GROUPS.iter().enumerate() {
                let rows: Vec<&str> = rows_on(g, store, sticks, deck).iter().map(|i| SETTINGS[*i].0).collect();
                for lang in ["en", "de"] {
                    let tr = |s: &str| if lang == "de" { de.translate(s).map(|t| t.to_string()).unwrap_or_else(|| s.to_string()) } else { s.to_string() };
                    let line = blurb_with(g, store, sticks, deck, tr);
                    let rows: Vec<String> = rows.iter().map(|r| tr(r).to_lowercase()).collect();
                    let got = parts(&line);
                    assert_eq!(got.len(), rows.len(), "{lang} {name} (store {store}, sticks {sticks}, Deck {deck}): {line:?} has a part per row: {rows:?}");
                    for (part, row) in got.iter().zip(&rows) {
                        assert!(row.contains(part.as_str()) || part.contains(row.as_str()), "{lang} {name}: {part:?} is not the row {row:?} ({line:?})");
                    }
                }
            }
        }
    }

    /// A device without sticks (the H700 handhelds) has no LEDs row, and Sound & feel's line doesn't promise one, in
    /// English or German (review 2026-10-30).
    #[test]
    fn without_sticks_the_sound_line_names_no_leds() {
        let g = category("Sound & feel");
        let de = |s: &str| pocket_ui::i18n::german().translate(s).map(|t| t.to_string()).unwrap_or_else(|| s.to_string());
        for tr in [&(|s: &str| s.to_string()) as &dyn Fn(&str) -> String, &de] {
            assert!(blurb_with(g, false, true, false, tr).contains("LEDs"), "with sticks");
            let line = blurb_with(g, false, false, false, tr);
            assert!(!line.contains("LED"), "{line:?}");
            assert!(line.ends_with('.') && line.chars().next().is_some_and(char::is_uppercase), "{line:?}");
        }
    }

    #[test]
    fn every_switch_without_a_page_changes_what_the_sounds_compare() {
        let h = std::thread::spawn(|| {
            use pocket_ui::settings::Appearance;
            let layout = pocket_ui::button_layout::ButtonLayout::Nintendo;
            let base = Switches::now(Appearance::Dark, layout);
            pocket_ui::palette::set_colours(pocket_ui::palette::colours().step(true));
            let coloured = Switches::now(Appearance::Dark, layout);
            assert_ne!(coloured, base, "Colours");
            let f = pocket_ui::font_style::get();
            pocket_ui::font_style::set(f.next());
            let font = Switches::now(Appearance::Dark, layout);
            assert_ne!(font, coloured, "Font");
            let m = pocket_ui::motion::level();
            pocket_ui::motion::set_level(m.next());
            assert_ne!(Switches::now(Appearance::Dark, layout), font, "Motion");
            assert_ne!(Switches::now(Appearance::Light, layout), Switches::now(Appearance::Dark, layout), "Appearance");
            assert_ne!(Switches::now(Appearance::Dark, layout.toggled()), Switches::now(Appearance::Dark, layout), "Button layout");
            let before = Switches::now(Appearance::Dark, layout);
            pocket_ui::button_prompts::set(pocket_ui::button_prompts::get().step(true));
            assert_ne!(Switches::now(Appearance::Dark, layout), before, "Button symbols");
            pocket_ui::font_style::set(f);
            pocket_ui::motion::set_level(m);
        });
        h.join().unwrap();
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

    /// No public catalog: the Store row is not there, nothing in the Help & tools list points at it, and the rows around it
    /// close up (moving, going round, touch all count in `rows_with`). With a catalog it is back where it was.
    #[test]
    fn the_store_row_is_there_only_with_a_catalog() {
        let help = category("Help & tools");
        let store = SETTINGS.iter().position(|s| matches!(s.1, Opens::Store)).unwrap();
        let (without, with) = (rows_with(help, false), rows_with(help, true));
        assert!(!without.contains(&store), "no catalog: no Store row");
        assert!(with.contains(&store), "a catalog: the Store row");
        assert_eq!(with.len(), without.len() + 1);
        assert!(without.iter().all(|i| !matches!(SETTINGS[*i].1, Opens::Store)));
        // Other categories are untouched.
        for g in (0..SETTINGS_GROUPS.len()).filter(|g| *g != help) {
            assert_eq!(rows_with(g, false), rows_with(g, true));
        }
        // The neighbours of the hidden row are neighbours now, in both directions.
        let before = SettingsPage::List(store - 1);
        let after = groups_input_with(before, Button::Down, false);
        // The row after the Store, or the category's first when the Store is its last (it goes round).
        let next = if store + 1 < span(help).end { store + 1 } else { without[0] };
        assert_eq!(after, Some(SettingsPage::List(next)), "Down skips the hidden row");
        assert_eq!(groups_input_with(after.unwrap(), Button::Up, false), Some(before), "Up skips it too");
        assert_eq!(groups_input_with(before, Button::Down, true), Some(SettingsPage::List(store)), "with a catalog Down lands on it");
        // Every shown row of Games still names what it opens (hints and touch read the same list).
        assert!(without.iter().all(|i| !SETTINGS[*i].0.is_empty() && !SETTINGS[*i].2.is_empty()));
    }

    /// The Deck's lines name exactly the rows the Deck shows, made from the handhelds' lines (which would name Wi-Fi,
    /// Sync, LEDs ...); `every_category_line_names_its_rows_in_order` checks them part by part, also in German.
    #[test]
    fn the_decks_category_lines_name_the_decks_rows() {
        let deck = |name: &str| blurb_with(category(name), false, false, true, str::to_string);
        assert_eq!(deck("Sound & feel"), "Animations, playful cases, old-TV switch-on, motion, sounds, case sounds.");
        assert_eq!(deck("Connections"), "Web companion.");
        assert_eq!(deck("System"), "Language, sleep & power, storage, backup & reset.");
        assert_eq!(deck("Help & tools"), "Tour, manual, report a problem, about.");
        assert_eq!(deck("Playing"), "On start, quit, buttons, over the game, frames, battery, achievements.");
    }

    /// The Help & tools line names the Store only while the row is there. (The device default in a test run is "no
    /// catalog": nothing here sets `STORE_OFFERED`.)
    #[test]
    fn the_help_line_leaves_out_the_store_while_it_is_hidden() {
        let help = category("Help & tools");
        let id = |s: &str| s.to_string();
        assert!(blurb_with(help, true, true, false, id).contains("Store"));
        assert!(!blurb_with(help, false, true, false, id).contains("Store"));
        assert!(!blurb(help, id).contains("Store"), "no catalog in a test run");
        assert_eq!(blurb(category("Playing"), id), SETTINGS_GROUPS[category("Playing")].1);
    }
}
