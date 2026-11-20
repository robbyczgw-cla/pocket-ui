//! Part of the pocket-ui binary: Settings in categories (`SETTINGS_GROUPS` in main.rs). START opens the first category's
//! list straight away (`first_row`), with the categories' symbols in a row at the top, the selected one named, L1 and R1
//! drawn at its ends (`App::paint_settings_header`): L1/R1 go to the previous and next category and round at the ends, a tap on
//! a symbol goes to it. Up on a category's first row puts the focus on the symbols (`SettingsPage::Tabs`): Left/Right
//! switch the category there, Down or A go back into its rows (the tab pattern of console settings). Each list is the old Settings list with only that category's rows (same values, same
//! behaviour); B closes Settings (SETTINGS-IA-DECISION package 2: no page of categories between the shelf and the rows).
//!
//! A category's list is `SettingsPage::List(row)` with `row` an index into the flat `SETTINGS`, as before: every page
//! that comes back with `List(settings_row(NAME))` (its B, a deep link's B) lands in its own category's list, and B
//! from there on the categories. Nothing here reads a file or starts work: the categories have no values, and a
//! category's list works out the values of its own rows only.
//!
//! A row's value (`settings_row_value`): switches show their state, pages a short value ("Home", "Shelf · Dark",
//! "332 games"); a page's row ends in a drawn angle on every row, and the selected row that changes in place adds its
//! ‹ › (`changes_in_place`, `marked`, `kit::row_value`),
//! cut with "…" to the room the name leaves. Look, Add games, Battery, Updates and About come from memory; Wi-Fi's
//! network, Remote access, Time's zone and the paired PCs need a process or a file and come from `settings_values`'
//! thread (`settings_values_refresh`, when a list with one of them is painted, at most every 5 s); until then the row
//! shows its angle alone.

use std::ops::Range;
use std::time::Instant;

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use pocket_ui::settings_values::{self as values, page};
use pocket_ui::setup::Only;
use pocket_ui::update;

use super::{App, Opens, SettingsPage, SettingsRow, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL, SETTINGS, SETTINGS_GROUPS};

/// The most lines the selected row's line takes (`kit::footer_text`: the two lowest beside the hints, the rest across the
/// page over them); every row's and category's line fits whole in fewer at Large text in every language
/// (`every_line_fits_at_the_bottom`).
const DESC_LINES: usize = 4;

/// `s` in the lines it takes at the bottom on a screen `w` pixels wide beside hints that start at `hints_at`
/// (`App::paint_desc`, `kit::footer_lines`).
fn desc_lines(text: &mut pocket_ui::text::Text, s: &str, w: i32, hints_at: i32) -> Vec<String> {
    pocket_ui::kit::footer_lines(text, s, w, hints_at, DESC_LINES)
}

/// Whether a row's value needs `settings_values`' thread.
fn needs_facts(opens: Opens) -> bool {
    matches!(opens, Opens::Setup(Only::Wifi | Only::Remote) | Opens::Time | Opens::PcStreaming)
}

/// The categories the first page lists: every entry of `SETTINGS_GROUPS` (no page only gathers more settings since the
/// tree of 2026-11-13; For tinkerers' Sync and PC streaming are rows of Connections).
pub(crate) const SHOWN: usize = SETTINGS_GROUPS.len();

/// How many rows the categories hold together: the length of `SETTINGS`.
pub(crate) const fn count() -> usize {
    let (mut g, mut n) = (0, 0);
    while g < SETTINGS_GROUPS.len() {
        n += SETTINGS_GROUPS[g].1.len();
        g += 1;
    }
    n
}

/// The rows of every category, one category after the other: `SETTINGS`.
pub(crate) const fn flat<const N: usize>() -> [SettingsRow; N] {
    let mut out = [SETTINGS_GROUPS[0].1[0]; N];
    let (mut g, mut k) = (0, 0);
    while g < SETTINGS_GROUPS.len() {
        let rows = SETTINGS_GROUPS[g].1;
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
    let start: usize = SETTINGS_GROUPS[..g].iter().map(|c| c.1.len()).sum();
    start..start + SETTINGS_GROUPS[g].1.len()
}

/// Whether Settings has a row for `opens`. The LEDs row (the lights around the
/// sticks) is left out on a device without sticks (the H700 handhelds).
#[cfg(not(feature = "brick"))]
fn shown(opens: Opens) -> bool {
    shown_on(opens, pocket_ui::device::current().has_sticks(), false)
}

#[cfg(feature = "brick")]
fn shown(opens: Opens) -> bool {
    shown_on(opens, pocket_ui::device::current().has_sticks(), { #[cfg(feature = "brick")] { pocket_ui::device::brick() } #[cfg(not(feature = "brick"))] { false } })
}

fn shown_on(opens: Opens, sticks: bool, nextui_owned: bool) -> bool {
    shown_in(opens, sticks, nextui_owned, pocket_ui::device::android_profile())
}

#[cfg(not(feature = "brick"))]
fn shown_in(opens: Opens, sticks: bool, nextui_owned: bool, android: bool) -> bool {
    !device_key(opens).is_some_and(|k| pocket_ui::device::hides_in(k, sticks, nextui_owned, android))
}

#[cfg(feature = "brick")]
fn shown_in(opens: Opens, sticks: bool, nextui_owned: bool, android: bool) -> bool {
    let kept = { #[cfg(feature = "brick")] { brick_keeps(opens) } #[cfg(not(feature = "brick"))] { false } };
    (kept || brick_has(opens) || !device_key(opens).is_some_and(|k| pocket_ui::device::hides_in(k, sticks, nextui_owned, android)))
        && { #[cfg(feature = "brick")] { !brick_hides(opens) } #[cfg(not(feature = "brick"))] { true } }
}

/// The Brick has its own motor and lights (`brick_hw`): Haptics and LEDs stay on its Settings although it has no sticks and is not a
/// handheld the way ROCKNIX's are.
#[cfg(feature = "brick")]
fn brick_has(opens: Opens) -> bool {
    #[cfg(feature = "brick")]
    {
        pocket_ui::device::brick() && (matches!(opens, Opens::Haptics) || matches!(opens, Opens::Leds) && pocket_ui::brick_profile::lights_page())
    }
    #[cfg(not(feature = "brick"))]
    {
        let _ = opens;
        false
    }
}

/// The pages a device may lack (`pocket_ui::device::hides`, also behind the Tour cards): the lights around the sticks;
/// power button.
fn device_key(opens: Opens) -> Option<&'static str> {
    Some(match opens {
        Opens::Leds => "leds",
        Opens::Haptics => "haptics",
        Opens::Sync => "sync",
        Opens::Pockets => "pocket_sync",
        Opens::Tailscale => "tailscale",
        Opens::Tools => "tools",
        Opens::PerfFan => "perf_fan",
        Opens::Setup(Only::Wifi | Only::Remote) | Opens::Bluetooth => "radios",
        Opens::Time => "time",
        Opens::Kids => "kids",
        Opens::Updates => "updates",
        // What only a Linux handheld has (`device::ANDROID_HIDES`).
        Opens::SleepPower => "sleep",
        Opens::Storage => "storage",
        Opens::Web => "web",
        Opens::PcStreaming => "pc_streaming",
        Opens::NightLight => "night_light",
        Opens::ScreenColour => "screen_colour",
        Opens::Battery => "battery",
        Opens::View => "android_view",
        Opens::BackToEs => "darkos_es",
        Opens::Gamelist => "gamelist",
        Opens::Backup => "backup",
        Opens::TestButtons => "test_buttons",
        Opens::Report => "report",
        Opens::ZoomInPause => "zoom_pause",
        Opens::BootScreen => "boot_screen",
        Opens::OverGame => "over_game",
        Opens::RememberQuit => "quit_without_saving",
        Opens::Screenshots => "screenshots",
        // Android's alone: its apps on a shelf (`device::hides_in`).
        Opens::AndroidGames => "android_games",
        _ => return None,
    })
}

/// The rows of category `g` that are shown, as indexes into `SETTINGS`, in their order. Painting, moving, touch and
/// the hint bar all count in this list, so a row that is not there leaves no gap and no dead tap.
pub(crate) fn rows(g: usize) -> Vec<usize> {
    span(g).filter(|i| shown(SETTINGS[*i].1)).collect()
}

#[cfg(test)]
fn rows_in(g: usize, sticks: bool, nextui_owned: bool, android: bool) -> Vec<usize> {
    span(g).filter(|i| shown_in(SETTINGS[*i].1, sticks, nextui_owned, android)).collect()
}

/// The name of row `i` of `SETTINGS` as this device paints it: its own, except the Brick's lights row, which carries its page's title
/// (`app_brick_lights::TITLE`, "Lights & buttons") instead of the sticks' "LEDs". The table keeps "LEDs" (the handhelds, the manual's
/// tables, `settings_row`); everything that shows a row's name asks here.
pub(crate) fn row_name(i: usize) -> &'static str {
    #[cfg(feature = "brick")]
    if pocket_ui::device::brick() && matches!(SETTINGS[i].1, Opens::Leds) {
        return super::app_brick_lights::TITLE;
    }
    // No fan: the row is called as its page, "Performance" (`perf_fan_page::title`).
    if matches!(SETTINGS[i].1, Opens::PerfFan) {
        return pocket_ui::perf_fan_page::title();
    }
    SETTINGS[i].0
}

/// The row that opens the lights page, to come back to from it.
#[cfg(feature = "brick")]
pub(crate) fn leds_row() -> usize {
    SETTINGS.iter().position(|s| matches!(s.1, Opens::Leds)).unwrap_or(0)
}

/// no PortMaster).
pub(crate) fn row_line(i: usize) -> &'static str {
    row_line_in(i, pocket_ui::device::android_profile())
}

/// [`row_line`] with Android's profile given. Android's setup has no Wi-Fi step, its About has no network, its Add games has no file
/// sharing and PortMaster is a Linux program, so those lines say what is there.
pub(crate) fn row_line_in(i: usize, android: bool) -> &'static str {
    if android {
        match SETTINGS[i].1 {
            Opens::FirstStart => return "Language, your games and the look again, step by step. Your games and saves stay.",
            Opens::About => return "Version, games, free space and battery.",
            Opens::Setup(Only::Games) => return "Where each system's games and BIOS files go.",
            Opens::Setup(Only::Extras) => return "Box art and starter pack.",
            _ => {}
        }
    }
    // No fan of ours (`device::no_fan`): the row is Performance, its line names only the processor (review 19.11. #2).
    if matches!(SETTINGS[i].1, Opens::PerfFan) && pocket_ui::device::no_fan() {
        return "Processor speed, in games and in menus.";
    }
    // Controller setup names where controllers are paired, only where that row is (Android pairs in its own settings, NextUI's
    // Brick card has no Bluetooth row).
    if matches!(SETTINGS[i].1, Opens::Controllers) && (android || !shown(Opens::Bluetooth)) {
        return CONTROLLERS_LINE_UNPAIRED;
    }
    // A handheld without Wi-Fi (`device::no_wifi_chip`): its first start has no Wi-Fi step, its Add games no file sharing.
    if pocket_ui::device::no_wifi_chip() {
        match SETTINGS[i].1 {
            Opens::FirstStart => return "Language, your games and the look again, step by step. Your games and saves stay.",
            Opens::Setup(Only::Games) => return "Where each system's games and BIOS files go.",
            _ => {}
        }
    }
    #[cfg(feature = "brick")]
    if pocket_ui::device::brick() {
        match SETTINGS[i].1 {
            Opens::Leds => return "The Brick's lights, the F1 and F2 keys and the mute slider.",
            Opens::Haptics => return "A soft tick as you move, and how strong it is. Games rumble on their own (pause menu, Options).",
            // Nothing on the card updates TrimUI's system: "and the system" promised what the page can't do.
            Opens::Updates => return "New versions of pocket-ui.",
            Opens::PerfFan => return "How fast the processor may run, in games and in menus.",
            Opens::Setup(Only::Extras) if !pocket_ui::ports::offered() => return "Box art and game pictures.",
            Opens::Setup(Only::Extras) => return "Box art, PortMaster games, cheats.",
            _ => {}
        }
    }
    SETTINGS[i].2
}

/// The categories with at least one row on this device, in their order: a category whose rows are all hidden (Connections
/// in Android's handheld view, `device::ANDROID_HIDES`) is not there.
pub(crate) fn shown_groups() -> Vec<usize> {
    (0..SHOWN).filter(|g| !rows(*g).is_empty()).collect()
}

/// The category START opens on: the first with a row on this device (Connections), and Look in Android's handheld view,
/// whose Connections has only what Android leaves to pocket-ui (Pocket Sync from 0.6.0): the lead, 19.11.2026.
pub(crate) fn first_group() -> usize {
    start_group(&shown_groups(), pocket_ui::device::android_profile())
}

/// [`first_group`] among the categories `shown`.
fn start_group(shown: &[usize], android: bool) -> usize {
    let look = shown.iter().copied().find(|g| SETTINGS_GROUPS[*g].0.trim_end_matches('\u{200b}') == "Look");
    match (android, look) {
        (true, Some(look)) => look,
        _ => shown.first().copied().unwrap_or(0),
    }
}

/// What START opens: the first category's first row.
pub(crate) fn first_row() -> usize {
    rows(first_group()).first().copied().unwrap_or(0)
}

/// The category `steps` along the shown ones from `g` (negative: back), round at the ends: L1/R1 and a tap on a symbol.
fn group_after(shown: &[usize], g: usize, steps: i32) -> usize {
    let n = shown.len().max(1) as i32;
    let at = shown.iter().position(|s| *s == g).unwrap_or(0) as i32;
    shown.get((at + steps).rem_euclid(n) as usize).copied().unwrap_or(g)
}

#[cfg(test)]
fn first_group_in(android: bool) -> usize {
    let shown: Vec<usize> = (0..SHOWN).filter(|g| !rows_in(*g, true, false, android).is_empty()).collect();
    start_group(&shown, android)
}

/// The category row `row` of `SETTINGS` is in.
pub(crate) fn group_of(row: usize) -> usize {
    (0..SETTINGS_GROUPS.len()).find(|g| span(*g).contains(&row)).unwrap_or(0)
}

/// Up, Down, L1, R1 and B in a category's list (A there is the row's own business, `App::settings_input`). `None` closes
/// Settings: B goes back to the shelf at once, there is no page of categories in between.
pub(crate) fn groups_input(page: SettingsPage, b: Button) -> Option<SettingsPage> {
    match (page, b) {
        // Up on the first row: the focus goes to the categories at the top (the tab pattern of console settings).
        (SettingsPage::List(row), Button::Up) if rows(group_of(row)).first() == Some(&row) => Some(SettingsPage::Tabs(row)),
        // Moving stays in the category; Down on the last row goes round to the first, as the long list did.
        (SettingsPage::List(row), Button::Up | Button::Down) => {
            let r = rows(group_of(row));
            let (at, len) = (r.iter().position(|i| *i == row).unwrap_or(0), r.len());
            let at = if b == Button::Up { (at + len - 1) % len } else { (at + 1) % len };
            Some(SettingsPage::List(r[at]))
        }
        // On the categories: Left/Right and L1/R1 go to the previous/next one at once, round at the ends; Down or A into
        // its rows, on the first; B out of Settings; nothing else moves.
        (SettingsPage::Tabs(row), Button::Left | Button::Right | Button::L1 | Button::R1) => {
            let g = group_after(&shown_groups(), group_of(row), if matches!(b, Button::Right | Button::R1) { 1 } else { -1 });
            Some(rows(g).first().map_or(page, |r| SettingsPage::Tabs(*r)))
        }
        (SettingsPage::Tabs(row), Button::Down) => Some(SettingsPage::List(row)),
        (SettingsPage::Tabs(row), b) if b == CONFIRM => Some(SettingsPage::List(row)),
        (SettingsPage::Tabs(_), b) if b == CANCEL => None,
        // The previous or next category with a row here, at its first row; round at the ends.
        (SettingsPage::List(row), Button::L1 | Button::R1) => {
            let g = group_after(&shown_groups(), group_of(row), if b == Button::R1 { 1 } else { -1 });
            Some(rows(g).first().map_or(page, |r| SettingsPage::List(*r)))
        }
        (SettingsPage::List(_), b) if b == CANCEL => None,
        (page, _) => Some(page),
    }
}

/// Where Settings' switches without a page of their own stand (Look → Appearance, Colours, Font, Motion; Controls &
/// sound → Buttons' two): part of `App::feedback_view`, so that switching one clicks as any other change does.
/// Each process-wide choice is read where it lives; the App's own two are handed in.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Switches {
    appearance: pocket_ui::settings::Appearance,
    layout: pocket_ui::button_layout::ButtonLayout,
    motion: pocket_ui::motion::Motion,
    colours: pocket_ui::palette::Colours,
    font: pocket_ui::font_style::FontStyle,
    letters: pocket_ui::font_style::RetroLetters,
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
            letters: pocket_ui::font_style::retro_letters(),
            prompts: pocket_ui::button_prompts::get(),
        }
    }
}

/// The Colours row's value: Retro's palette while it is on (`retro`), else the colours'.
fn colours_label() -> &'static str {
    if pocket_ui::retro::on() {
        pocket_ui::retro::colours().label()
    } else {
        pocket_ui::palette::colours().label()
    }
}

/// The icon of each category of `SETTINGS_GROUPS` in the Retro style (`retro`), in its order: colour pictures.
/// Controls has the controller, Screen & sound the loudspeaker, Playing the play button (Settings v2).
const RETRO_ICONS: [pocket_ui::retro::Icon; SETTINGS_GROUPS.len()] = {
    use pocket_ui::retro::Icon;
    [Icon::Connections, Icon::Look, Icon::Sound, Icon::Playing, Icon::Continue, Icon::Games, Icon::System, Icon::Help]
};

/// The same in the other styles (the length keeps one each).
const ICONS: [pocket_ui::ui_art::Icon; SETTINGS_GROUPS.len()] = {
    use pocket_ui::ui_art::Icon;
    [Icon::Connections, Icon::Look, Icon::Sound, Icon::Playing, Icon::Play, Icon::Games, Icon::System, Icon::Help]
};

/// Whether a row changes its value in place rather than opening a page: Left/Right step it (A changes it too, as it
/// always did), the selected one shows its value in ‹ ›, and it never ends in a page's angle (Settings 0.5.1: which rows
/// open a page and which change in place was not clear enough, and Zoom in the pause menu took only A; a hand walk).
/// One list for the hints, the marks and the keys, so a new row can't take A alone (tests/user_journeys.rs walks them).
pub(crate) fn changes_in_place(opens: Opens) -> bool {
    matches!(
        opens,
        Opens::Appearance | Opens::Colours | Opens::Font | Opens::TextSize | Opens::Motion | Opens::Animations
            | Opens::Brightness | Opens::Volume | Opens::ScreenColour | Opens::Sounds | Opens::Extra(_)
            | Opens::Haptics | Opens::OnStart | Opens::RememberQuit | Opens::BootScreen | Opens::MomentPhotos | Opens::RemindWhere
            | Opens::Frames | Opens::OverGame | Opens::ZoomInPause | Opens::LivedIn | Opens::ShelfSpace | Opens::Language
    )
}

/// Row `opens`'s value `value` with its marks as painted (`kit::row_value`): a page's row ends in its angle on every row
/// (">" alone without a value), a row that changes in place shows ‹ › on the selected row only and no angle.
fn marked(opens: Opens, value: String, selected: bool) -> String {
    let bare = value.strip_prefix("‹ ").and_then(|v| v.strip_suffix(" ›")).map_or(value.clone(), str::to_string);
    if changes_in_place(opens) {
        let bare = bare.strip_suffix(" >").unwrap_or(&bare).to_string();
        if selected { format!("‹ {bare} ›") } else { bare }
    } else if bare == ">" || bare.ends_with(" >") {
        bare
    } else {
        values::page(&bare)
    }
}

/// Whether the value of row `i` is a name, painted as it is: a Wi-Fi named "Off" or "Unknown" must not be translated (the
/// Wi-Fi page shows it as it is), nor a font, a layout, a language or a city.
pub(crate) fn value_is_typed(i: usize) -> bool {
    typed_value(SETTINGS[i].1, &values::get().wifi)
}

fn typed_value(opens: Opens, wifi: &Option<values::Wifi>) -> bool {
    // Rows whose value is a name in every language: the style's font ("Barlow"), the buttons' layouts ("Nintendo"), the
    // language in its own name ("Deutsch") and the time zone's city (already in the language, `timezone::city_here`).
    matches!(opens, Opens::Font | Opens::Buttons | Opens::Language | Opens::Time)
        || matches!(opens, Opens::Setup(Only::Wifi)) && matches!(wifi, Some(values::Wifi::Connected(Some(_))))
}

/// Controller setup's line where Settings has no Bluetooth row to pair in.
const CONTROLLERS_LINE_UNPAIRED: &str = "A separate layout for each controller, in menus and games.";

/// The line under Remind me where I was while it is Off.
const REMIND_OFF_LINE: &str = "No reminder after a long break.";

/// The line under Console boot screen while it is On: which starts, which consoles, and what it needs.
/// Photos of big moments switched on: what happens now (Off, the row says what it would do).
const PHOTOS_ON_LINE: &str = "On: photos are taken quietly. When you quit, they slide into the game's case: Down on its box opens it.";
/// Photos of big moments' third value (where pocket-core draws the game) and its line.
const PHOTOS_IN_PLAY: &str = "Also in play";
const PHOTOS_IN_PLAY_LINE: &str = "Also in play: a soft flash and the photo out of the screen's edge while you play. It goes into the game's case too.";
const BOOT_ON_LINE: &str = "Game Boy, Game Boy Advance, PlayStation, Dreamcast, Saturn, Mega-CD and PC Engine CD games start with their boot screen when nothing is continued. Needs your BIOS file in roms/bios.";

/// The line under Continue where I quit while it is Off.
const REMEMBER_OFF_LINE: &str = "Quit keeps no moment: the next start is fresh, and the older moment stays under Load.";

impl App {
    /// Photos of big moments' value: Off, On, or Also in play (settings.json's `moment_in_play`, where the device has it).
    pub(crate) fn moment_photos_value(&self) -> &'static str {
        match (self.moment_photos, self.moment_in_play && !pocket_ui::device::hides("moment_in_play", false, false)) {
            (false, _) => values::on_off(false),
            (true, false) => values::on_off(true),
            (true, true) => PHOTOS_IN_PLAY,
        }
    }

    /// The value at the right of `SETTINGS[i]` (`sel`: the selected row, whose switches show ‹ ›), at most `room`
    /// pixels wide. Nothing here reads a file or starts a process.
    pub(crate) fn settings_row_value(&mut self, i: usize, sel: usize, room: i32) -> String {
        let facts = values::get();
        let value = match SETTINGS[i].1 {
            #[cfg(feature = "brick")]
            Opens::Haptics if pocket_ui::device::brick() => self.brick_haptics_value(i == sel),
            Opens::Haptics => values::on_off(pocket_ui::rumble::menu_haptics()).to_string(),
            Opens::Extra(e) if i == sel => format!("‹ {} ›", values::on_off(pocket_ui::cozy_extras::on(e))),
            Opens::Extra(e) => values::on_off(pocket_ui::cozy_extras::on(e)).to_string(),
            Opens::OnStart if i == sel => format!("‹ {} ›", self.on_start.label()),
            Opens::OnStart => self.on_start.label().to_string(),
            Opens::RememberQuit => values::on_off(self.remember_quit).to_string(),
            Opens::RemindWhere => values::on_off(self.remind_where).to_string(),
            Opens::ZoomInPause => values::on_off(self.zoom_in_pause).to_string(),
            Opens::LivedIn => values::on_off(self.lived_in).to_string(),
            Opens::ShelfSpace if i == sel => format!("‹ {} ›", self.shelf_space_value()),
            Opens::ShelfSpace => self.shelf_space_value(),
            Opens::BootScreen => values::on_off(self.boot_screen).to_string(),
            Opens::MomentPhotos => self.moment_photos_value().to_string(),
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
            // The quick settings' levels, from memory (`Sys::level_label`); a look in the background keeps them fresh.
            Opens::Brightness | Opens::Volume => {
                let now = Instant::now();
                self.sys.watch_levels(now);
                let row = if matches!(SETTINGS[i].1, Opens::Brightness) { pocket_ui::quick::Row::Brightness } else { pocket_ui::quick::Row::Volume };
                let level = self.sys.level_label(row, now);
                if i == sel { format!("‹ {level} ›") } else { level }
            }
            Opens::ScreenColour if i == sel => format!("‹ {} ›", self.sys.look().warmth_label()),
            Opens::ScreenColour => self.sys.look().warmth_label(),
            Opens::Tour => self.tour_list_value(),
            Opens::View => page("Handheld"),
            Opens::Sounds if i == sel && self.sound.on => format!("‹ {} ›", self.sound.label()),
            Opens::Sounds => self.sound.label(),
            Opens::Sync => page(values::on_off(self.sync.prefs.enabled)),
            Opens::Pockets => page(&self.pocket_sync_list_value()),
            // The free space of the last look (at start, after each game): memory only (`app_storage`).
            Opens::Storage => self.storage_list_value(),
            // How many favorites wait to be added: memory only (`app_gamelist`).
            Opens::Gamelist => self.gamelist_list_value(),
            // How many screenshots there are: memory only (`app_gallery`).
            Opens::Screenshots => self.gallery_list_value(),
            // How many Android games are on the shelf: memory only (`app_android_games`).
            Opens::AndroidGames => self.android_games_list_value(),
            #[cfg(feature = "brick")]
            Opens::Leds if pocket_ui::device::brick() => page(super::app_brick_lights::list_value(&self.leds)),
            Opens::Leds => page(self.leds.summary()),
            Opens::NightLight => page(&self.night.summary()),
            Opens::Tailscale => self.tailscale_list_value(),
            Opens::Romm => self.romm_list_value(),
            Opens::Web => self.web_list_value(),
            Opens::Bluetooth => self.bluetooth_list_value(),
            Opens::Curator => self.curator_list_value(),
            Opens::Achievements => self.achievements_list_value(),
            Opens::SleepPower => page(&self.sleep_power_value()),
            Opens::PerfFan => page(&values::perf_fan(self.perf_prefs().games_performance, self.perf_prefs().games_fan)),
            Opens::Setup(Only::Look) => page(self.theme.name()),
            Opens::Setup(Only::Play) => pocket_ui::features::Profile::of(&self.switches()).name().to_string(),
            Opens::Colours if i == sel => format!("‹ {} ›", colours_label()),
            Opens::Colours => colours_label().to_string(),
            Opens::Font if i == sel => format!("‹ {} ›", pocket_ui::font_style::row_label()),
            Opens::Font => pocket_ui::font_style::row_label().to_string(),
            Opens::Appearance if i == sel => format!("‹ {} ›", self.appearance.name()),
            Opens::Appearance => self.appearance.name().to_string(),
            Opens::Motion if i == sel => format!("‹ {} ›", pocket_ui::motion::level().name()),
            Opens::Motion => pocket_ui::motion::level().name().to_string(),
            Opens::Setup(Only::Games) => page(&values::games(self.systems.iter().map(|s| s.games.len()).sum())),
            Opens::GameCheck => page(&self.game_check_value()),
            // Hidden systems that have games: the row says so, since Home shows nothing of them.
            Opens::ShelfSystems => page(&values::hidden_systems(self.systems.iter().filter(|s| !s.games.is_empty() && self.hidden.contains(&s.name)).count())),
            Opens::Setup(Only::Wifi) => page(&facts.wifi.map(|w| w.label()).unwrap_or_default()),
            Opens::Setup(Only::Remote) => page(facts.ssh.map(values::on_off).unwrap_or_default()),
            Opens::Time => page(&facts.zone.as_deref().map(values::zone).unwrap_or_default()),
            Opens::PcStreaming => page(&facts.pcs.map(values::pcs).unwrap_or_default()),
            // The figure alone: "charging" is in the pill above.
            Opens::Battery => page(&pocket_ui::status_corner::status().battery.map(|b| format!("{}%", b.percent)).unwrap_or_default()),
            Opens::Updates => {
                let newer = self.updates.app_available().map(|a| a.version.as_str());
                #[cfg(not(feature = "brick"))]
                let shown = values::updates(update::shown_version(), newer);
                #[cfg(feature = "brick")]
                let shown = values::updates(&update::brick_short(update::app_version()), newer.map(update::brick_short).as_deref());
                page(&shown)
            }
            #[cfg(not(feature = "brick"))]
            Opens::About => page(update::shown_version()),
            #[cfg(feature = "brick")]
            Opens::About => page(&update::brick_short(update::app_version())),
            _ => ">".to_string(),
        };
        let max_w = room.min(values::MAX_W);
        let shown = marked(SETTINGS[i].1, value, i == sel);
        let lang = pocket_ui::i18n::current();
        // The words only are looked up and drawn (`kit::row_value`): "Shelf >" and "‹ Home ›" are no lines of the table, and asking
        // for them ran every pattern of it. A value the code can say in the language ("Auto · Quiet") is said and kept as it is.
        let shown = match values::translated_in_code(lang, values::split_decoration(&shown).1) {
            Some(words) => {
                self.text.keep_as_is(&words);
                let (pre, _, post) = values::split_decoration(&shown);
                format!("{pre}{words}{post}")
            }
            None => shown,
        };
        let text = &mut self.text;
        values::fit_translated(&shown, max_w, |v| pocket_ui::i18n::tr_in(lang, v), |s| {
            let (pre, words, post) = values::split_decoration(s);
            text.width(Face::Body, 36, pre) + text.width(Face::Body, 36, words) + text.width(Face::Body, 36, post)
        })
    }

    /// The line under row `i` of `SETTINGS` as it is now: a cozy extra's note while another switch keeps it from
    /// showing (`App::extra_row_line`), else its own (`row_line`).
    pub(crate) fn settings_row_desc(&self, i: usize) -> &'static str {
        match SETTINGS[i].1 {
            Opens::Extra(e) => self.extra_row_line(e, row_line(i)),
            // Off says what Off does (it kept saying "Quit keeps the moment", walk 2026-11-02).
            Opens::RememberQuit if !self.remember_quit => REMEMBER_OFF_LINE,
            Opens::RemindWhere if !self.remind_where => REMIND_OFF_LINE,
            Opens::BootScreen if self.boot_screen => BOOT_ON_LINE,
            Opens::MomentPhotos if self.moment_photos && self.moment_photos_value() == PHOTOS_IN_PLAY => PHOTOS_IN_PLAY_LINE,
            Opens::MomentPhotos if self.moment_photos => PHOTOS_ON_LINE,
            _ => row_line(i),
        }
    }

    /// The lowest a category's rows may reach on a screen `w`×`h`: over the hint
    /// bar, and over the top of the tallest of `lines` as `paint_desc` paints it (a gap above its capitals), the same
    /// for every row, so the list doesn't shift as the selection moves. It kept two lines and a gap as soon as one line
    /// didn't fit beside the hints, a row's height of empty screen on the Brick (hand test, 17.11.); the lines now take
    /// the column beside the hints first, as on every page of the Settings family (`kit::footer`).
    pub(crate) fn rows_bottom(&mut self, rows: &[usize], w: i32, h: i32) -> i32 {
        let mut bottom = pocket_ui::kit::hint_bar_top(h) - 16;
        for &i in rows {
            let line = self.settings_row_desc(i);
            if line.trim().is_empty() {
                continue;
            }
            // Beside the hints its row shows; with the focus on the symbols the rows keep the same room.
            let hints = self.settings_row_hints(i);
            let hints_at = w - NU_PAD - pocket_ui::kit::pill_width(&mut self.text, (w, h), hints);
            let (top, _) = pocket_ui::kit::footer_top(&mut self.text, line, w, h, hints_at, DESC_LINES);
            bottom = bottom.min(top - 10);
        }
        bottom
    }

    /// The hints of a category's row `i` while it is selected: Brightness and Volume (and Screen colour at Neutral)
    /// change with Left/Right; Screen colour off Neutral offers A for Neutral (A would do nothing at Neutral; the
    /// navigation explorer, 2026-11-07); a row that changes in place says Left/Right (A still changes it too); a page's
    /// row says A opens it.
    pub(crate) fn settings_row_hints(&self, i: usize) -> &'static [(&'static str, &'static str)] {
        let screen_colour = matches!(SETTINGS[i].1, Opens::ScreenColour);
        if matches!(SETTINGS[i].1, Opens::Brightness | Opens::Volume) || screen_colour && self.sys.look().warmth == 0 {
            &[("B", "Back"), ("< >", "Change")]
        } else if screen_colour {
            &[("B", "Back"), ("< >", "Change"), ("A", "Neutral")]
        } else if changes_in_place(SETTINGS[i].1) {
            &[("B", "Back"), ("< >", "Change")]
        } else {
            &[("B", "Back"), ("A", "Open")]
        }
    }

    /// The selected row's line `line` at the bottom beside the hints (that start at `hints_at`), as every page of the
    /// Settings family has it (`kit::footer_text`): its last line on the hints' line, the two lowest in the column beside
    /// them, the rest across the page in the room `rows_bottom` left.
    pub(crate) fn paint_desc(&mut self, c: &mut Canvas, line: &str, hints_at: i32) {
        pocket_ui::kit::footer_text(c, &mut self.text, line, hints_at, DESC_LINES, pal().text);
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

    /// The top of a category's list, instead of a title: the status pill at the right; at the left L1 drawn as its key,
    /// the symbol of every category this device has (`shown_groups`), the selected one in the text's colour with its name
    /// after it (its short name, `tab_label`, where the whole one doesn't fit), the others dimmed, and R1 drawn as its key.
    /// Where some category's short name doesn't fit beside the widest pill (German with Large text, the Brick) every
    /// category's name stands alone as the title and the symbols with L1 and R1 take a slim line under it: one layout for
    /// all categories, so L1/R1 never move the row or the rows (tests/settings_tabbar_stable.rs).
    /// L1 and R1 are hints (the navigation explorer reads them, `nav_probe::hint`), each symbol a tap that goes there. With
    /// the focus up here (`tabs`, `SettingsPage::Tabs`) the selected symbol and its name stand on the selection's paper, the
    /// focus mark. Returns where the rows start.
    pub(crate) fn paint_settings_header(&mut self, c: &mut Canvas, g: usize, tabs: bool) -> i32 {
        let pill_left = self.status_pill(c);
        let shown = shown_groups();
        let n = shown.len() as i32;
        let at = shown.iter().position(|s| *s == g).unwrap_or(0) as i32;
        let name = SETTINGS_GROUPS[g].0;
        let left = NU_PAD + NU_BTN_PAD;
        let cap_w = (self.text.width(Face::Body, CAP_PX, "L1") + 28).max(CAP_W);
        let symbols_w = n * SYM + (n - 1).max(0) * SYM_GAP;
        // The room beside the widest pill (a two-digit hour, Wi-Fi, "100%, charging"), not beside today's, which changes
        // with the battery: the RG28XX's "15:48 24%, charging" squeezed the line that the plain clock left room for.
        let room = self.widest_pill_left(c) - 24 - left;
        // One layout for every category of this screen, language, style and text size, so that L1/R1 don't move the row
        // or the rows under it (the RG28XX's "Controls & sound" took a title line of its own and Look didn't, hand test of
        // 0.5.0): one line when every category's short name fits beside its symbol, else the title line on all of them.
        let widest = shown.iter().map(|&k| self.text.width(Face::Title, 50, tab_label(k))).max().unwrap_or(0);
        let one_line = header_fits(cap_w, symbols_w, widest, room);
        // On the line the whole name where it fits, else the short one; the hints (L1, R1) say the whole names.
        let full_w = self.text.width(Face::Title, 50, name);
        let tab = if header_fits(cap_w, symbols_w, full_w, room) { name } else { tab_label(g) };
        let name_w = if tab == name { full_w } else { self.text.width(Face::Title, 50, tab) };
        // The middle of the line the symbols are on, and where the rows start.
        let (mid, top) = if one_line {
            (pocket_ui::kit::header_top(c.h) + pocket_ui::kit::header_pill(c.h) / 2, pocket_ui::kit::header_bottom(c.h) + 12)
        } else {
            let fitted = self.text.fit(Face::Title, 50, name, pill_left - 24 - left);
            self.text.draw(c, Face::Title, 50, left, pocket_ui::kit::header_title(c.h), pal().chalk, &fitted);
            let line_top = pocket_ui::kit::header_bottom(c.h) + 2;
            (line_top + CAP_H / 2, line_top + CAP_H + 14)
        };
        let names = |k: i32| SETTINGS_GROUPS[group_after(&shown, g, k)].0.trim_end_matches('\u{200b}');
        let (prev, next) = (names(-1).to_string(), names(1).to_string());
        let retro = pocket_ui::retro::on();
        let mut x = left;
        for (key, button, label) in [("L1", Button::L1, prev.as_str()), ("R1", Button::R1, next.as_str())] {
            if key == "R1" {
                x += 16;
            }
            let (y, base) = (mid - CAP_H / 2, mid + pocket_ui::type_scale::mid(CAP_PX));
            pocket_ui::kit::key_cap(c, &mut self.text, (x, y, cap_w, CAP_H), key, CAP_PX, base, (pal().chip, pal().pill));
            self.hits.button(Area::new(x - 8, y - 10, cap_w + 16, CAP_H + 20), button);
            pocket_ui::nav_probe::hint(c, (x, y, cap_w, CAP_H), key, label);
            x += cap_w;
            if key == "L1" {
                x += 16;
                for (i, &k) in shown.iter().enumerate() {
                    let on = i as i32 == at;
                    let ink = if on && tabs { pal().ink } else if on { pal().chalk } else { pal().dust };
                    let y = mid - SYM / 2;
                    if on && (tabs || !one_line) {
                        // The paper round the symbol, and its name beside it on one line: where the focus is, or (the slim
                        // line under the title) which of the symbols the title names.
                        let paper = if one_line { (x - 14, pocket_ui::kit::header_top(c.h) + 4, SYM + 12 + name_w + 28, pocket_ui::kit::chrome(NU_PILL - 8, c.h)) } else { (x - 8, y - 8, SYM + 16, SYM + 16) };
                        // Orange is the focus, as on a selected row; the slim line's marker of the category named above
                        // is the quiet chip while the focus is in the rows.
                        c.round_rect(paper.0, paper.1, paper.2, paper.3, 12, if tabs { pal().select } else { pal().chip });
                        if tabs {
                            pocket_ui::nav_probe::focus(c, paper);
                        }
                    }
                    if retro {
                        pocket_ui::retro::icon(c, RETRO_ICONS[k], x, y, SYM / pocket_ui::retro::ICON);
                        if !on {
                            c.shade_rect(x, y, SYM, SYM, pal().void, 130);
                        }
                    } else {
                        pocket_ui::ui_art::paint_tinted(c, pocket_ui::ui_art::Art::Icon(ICONS[k]), x, y, SYM / 16, ink);
                    }
                    if SETTINGS_GROUPS[k].0 == "System" && self.update_dot() {
                        Self::paint_update_dot(c, x + SYM, y + 4, 24);
                    }
                    // A tap goes there: as many presses of L1 or R1 as the shorter way round takes.
                    let steps = (i as i32 - at).rem_euclid(n.max(1));
                    let area = Area::new(x - SYM_GAP / 2, mid - CAP_H / 2 - 10, SYM + SYM_GAP, CAP_H + 20);
                    if steps != 0 && steps <= n / 2 {
                        self.hits.presses(area, Button::R1, steps as u8);
                    } else if steps != 0 {
                        self.hits.presses(area, Button::L1, (n - steps) as u8);
                    }
                    x += SYM;
                    if on && one_line {
                        x += 12;
                        self.text.draw(c, Face::Title, 50, x, pocket_ui::kit::header_title(c.h), if tabs { pal().ink } else { pal().chalk }, tab);
                        x += name_w;
                    }
                    if i + 1 < shown.len() {
                        x += SYM_GAP;
                    }
                }
            }
        }
        top
    }
}

impl App {
    /// The status pill's left edge (`App::status_pill`) with the widest it shows: the clock with two-digit numbers,
    /// Wi-Fi, and the battery full and charging.
    fn widest_pill_left(&mut self, c: &Canvas) -> i32 {
        use pocket_ui::status_corner::{Battery, Corner, Pill};
        let clock = pocket_ui::status_corner::clock().map_or_else(|| "88:88".to_string(), |t| widest_clock(&t));
        let widest = Corner { clock: Some(clock), wifi: true, battery: Some(Battery { percent: 100, charging: true }.label()) };
        let pill = Pill { right: c.w - NU_PAD, top: pocket_ui::kit::header_top(c.h), h: pocket_ui::kit::header_pill(c.h), px: NU_SMALL, pad: NU_BTN_PAD };
        let text = &mut self.text;
        pill.layout(&widest, |s| text.width(Face::Body, NU_SMALL, s)).map_or(pill.right, |(_, left)| left)
    }
}

/// The clock `t` ("9:05 AM", "13:28") with the widest figures and a two-digit hour ("88:88 AM", "88:88").
fn widest_clock(t: &str) -> String {
    let t: String = t.chars().map(|c| if c.is_ascii_digit() { '8' } else { c }).collect();
    if t.find(':') == Some(1) { format!("8{t}") } else { t }
}

/// The symbols' size on screen, the room between them, and L1's and R1's keys.
const SYM: i32 = 32;
const SYM_GAP: i32 = 18;
const CAP_W: i32 = 64;
const CAP_H: i32 = 44;
const CAP_PX: u32 = 28;

/// Category `k`'s name on the row of symbols where its whole name is too long there: the part before " & "
/// ("Screen & sound" → "Screen"), any name alike, so a new or renamed category needs no code here, only that part's
/// line in the language tables (`short_tab_names_are_translated`). The whole name stays the title of the two-line layout
/// and the L1/R1 hints' word.
fn tab_label(k: usize) -> &'static str {
    let name = SETTINGS_GROUPS[k].0;
    name.split(" & ").next().unwrap_or(name)
}

/// Whether L1, the symbols (`symbols_w` together), the selected one's name (`name_w`) and R1 fit on one line of `room`.
fn header_fits(cap_w: i32, symbols_w: i32, name_w: i32, room: i32) -> bool {
    2 * cap_w + 2 * 16 + symbols_w + 12 + name_w <= room
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
    /// One whole frame as the window paints it (the shelf's or Settings' page, the status corner, the page transition).
    fn settings_frame(&mut self, buf: &mut [u32], w: u32, h: u32) {
        let now = Instant::now();
        self.motion_frame_begin(now);
        self.paint_in_appearance(&mut Canvas::new(buf, w, h), &mut |app, c| {
            app.paint(c);
            app.motion_after_paint(c);
            app.sys.paint(c, &mut app.text, app.menu.is_none());
            if app.sys.is_open() {
                app.status_corner();
            }
            app.paint_leave(c);
        });
    }

    /// The first page of every category this device has, with the focus in the list and on the tab bar, each painted the
    /// way START or L1/R1 brings it (from the shelf's frame): how many pages, and the strings that ran the language table's
    /// patterns on the way (`--snapshot bench-settings-pages`, tests/settings_open.rs). Every row's value and name must be
    /// a line of the table, a name or a figure by now, never a string that needs the patterns tried (the first start of
    /// Settings on the Mini V2 paid a frame's worth for each).
    pub(crate) fn settings_pages_patterns(&mut self, w: u32, h: u32) -> (usize, usize, Vec<String>) {
        let mut buf = vec![0u32; (w * h) as usize];
        pocket_ui::status_corner::start(|| {});
        self.text.prewarm();
        self.text.finish_prewarm();
        // The shelf, as the player sees it before START.
        self.settings_page = None;
        for _ in 0..2 {
            self.hits.clear();
            self.paint(&mut Canvas::new(&mut buf, w, h));
        }
        self.settings_frame(&mut buf, w, h);
        let mut pages = 0;
        let before = self.text.pattern_runs();
        let mut seen = self.text.pattern_log().len();
        let mut found = Vec::new();
        for g in shown_groups() {
            let first = rows(g).first().copied().unwrap_or(0);
            for page in [SettingsPage::List(first), SettingsPage::Tabs(first)] {
                self.settings_page = None;
                self.settings_frame(&mut buf, w, h);
                self.settings_page = Some(page);
                self.settings_frame(&mut buf, w, h);
                pages += 1;
                let log = self.text.pattern_log();
                found.extend(log[seen.min(log.len())..].iter().map(|s| format!("{} / {}: {s}", SETTINGS_GROUPS[g].0, if matches!(page, SettingsPage::List(_)) { "list" } else { "tabs" })));
                seen = log.len();
            }
        }
        self.settings_page = None;
        (pages, self.text.pattern_runs() - before, found)
    }

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
        let frame = |app: &mut App, buf: &mut [u32]| app.settings_frame(buf, w, h);
        // The shelf's own frame has set the page; START opens the categories.
        frame(self, &mut buf);
        self.settings_page = Some(SettingsPage::List(first_row()));
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
            self.settings_page = Some(SettingsPage::List(first_row()));
            let t = cpu();
            frame(self, &mut buf);
            steady.push(us(t));
            let t = cpu();
            self.status_pill(&mut Canvas::new(&mut buf, w, h));
            pill.push(us(t));
            // The transition alone, on a finished picture.
            self.settings_page = None;
            frame(self, &mut buf);
            self.settings_page = Some(SettingsPage::List(first_row()));
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
    use super::super::{app_moonlight, app_pocket_sync, app_web, settings_row, Opens, SETTINGS};
    use super::*;
    use pocket_ui::cozy_extras::Needs;

    /// A page's row ends in its angle on every row, whatever its value said; a row that changes in place never does,
    /// and only the selected one has its ‹ › (0.5.1: "How you like to play" opened a page with no angle, the on/off
    /// rows had no ‹ › and took no Left/Right).
    #[test]
    fn page_rows_keep_their_angle_and_only_the_selected_value_row_its_arrows() {
        assert_eq!(marked(Opens::Setup(Only::Play), "Enthusiast".into(), false), "Enthusiast >");
        assert_eq!(marked(Opens::Manual, ">".into(), true), ">");
        assert_eq!(marked(Opens::Sync, "Off >".into(), false), "Off >");
        assert_eq!(marked(Opens::ZoomInPause, "Off".into(), true), "‹ Off ›");
        assert_eq!(marked(Opens::ZoomInPause, "Off".into(), false), "Off");
        assert_eq!(marked(Opens::TextSize, "‹ Large ›".into(), false), "Large");
        assert_eq!(marked(Opens::BootScreen, "On".into(), true), "‹ On ›");
        // Every row of the tree is one or the other, never both: the hint and the keys follow the same list.
        for (name, opens, _) in SETTINGS.iter() {
            let (on, off) = (marked(*opens, "x".into(), true), marked(*opens, "x".into(), false));
            assert!(on.ends_with(" >") == !changes_in_place(*opens) && off.ends_with(" >") == !changes_in_place(*opens), "{name}: {on:?} {off:?}");
        }
    }

    /// The pill the tab row leaves room for: the widest figures of today's clock, a one-digit hour made two.
    #[test]
    fn the_widest_clock_has_two_digit_hours_and_eights() {
        assert_eq!(widest_clock("13:28"), "88:88");
        assert_eq!(widest_clock("9:05 AM"), "88:88 AM");
        assert_eq!(widest_clock("12:00 PM"), "88:88 PM");
    }

    /// The short names on the tab row are words every language table has (a new "X & Y" category needs a line for X).
    #[test]
    fn short_tab_names_are_translated() {
        for k in 0..SETTINGS_GROUPS.len() {
            let (full, short) = (SETTINGS_GROUPS[k].0.trim_end_matches('\u{200b}'), tab_label(k).trim_end_matches('\u{200b}'));
            assert!(full.starts_with(short), "{full} / {short}");
            if short == full {
                continue;
            }
            for lang in [pocket_ui::i18n::Lang::De, pocket_ui::i18n::Lang::Fr, pocket_ui::i18n::Lang::Es, pocket_ui::i18n::Lang::It, pocket_ui::i18n::Lang::Pt] {
                assert_ne!(pocket_ui::i18n::tr_in(lang, tab_label(k)), tab_label(k), "{short} has no {lang:?} line");
            }
        }
    }

    fn category(name: &str) -> usize {
        SETTINGS_GROUPS
            .iter()
            .position(|c| c.0.trim_end_matches('\u{200b}') == name)
            .unwrap_or_else(|| panic!("no category {name:?}"))
    }

    /// Every row's line, in every language, fits whole in the lines it may take at the bottom at Large text, beside the
    /// widest hints a category shows (never cut with "…"). The first-day run (2026-10-26) found Playing and System cut
    /// short.
    #[test]
    fn every_line_fits_at_the_bottom() {
        use pocket_ui::type_scale::{self, TextSize};
        let mut text = pocket_ui::text::Text::new();
        text.set_translate(false);
        // Every line as a device with or without sticks can show it, in every language.
        let mut lines: Vec<String> = Vec::new();
        for lang in pocket_ui::i18n::Lang::ALL {
            let tr = |s: &str| match lang.table() {
                Some(t) => t.translate(s).unwrap_or_else(|| panic!("no {lang:?} line for {s:?}")).to_string(),
                None => s.to_string(),
            };
            for row in SETTINGS.iter() {
                lines.push(tr(row.2));
            }
            for note in [Needs::Sounds, Needs::CozyFull, Needs::MotionFull].map(Needs::note) {
                lines.push(tr(note));
            }
            lines.push(tr(REMEMBER_OFF_LINE));
        }
        for size in [TextSize::Normal, TextSize::Medium, TextSize::Large] {
            type_scale::set(size);
            for w in [1280, 1240, 1536] {
                let hints_at = w - NU_PAD - pocket_ui::kit::pill_width(&mut text, (w, 960), &[("B", "Back"), ("< >", "Change"), ("A", "Neutral")]);
                for s in &lines {
                    let shown = desc_lines(&mut text, s, w, hints_at);
                    assert!(!shown.iter().any(|l| l.ends_with('…')), "{size:?} {w}: {s:?} is cut: {shown:?}");
                    assert_eq!(shown.join(" "), *s, "{size:?} {w}: {s:?} is shown whole");
                }
            }
        }
        type_scale::set(TextSize::Normal);
    }

    #[test]
    fn a_device_without_sticks_has_no_stick_lights_row() {
        let leds = SETTINGS.iter().position(|s| matches!(s.1, Opens::Leds)).unwrap();
        assert!(shown_on(Opens::Leds, true, false) && !shown_on(Opens::Leds, false, false));
        assert!(shown_on(Opens::Tools, false, false), "everything else stays");
        assert!(rows(group_of(leds)).contains(&leds), "the test machine has sticks (it is no H700)");
    }

    #[test]
    fn the_tour_and_first_start_cards_follow_the_settings_rows() {
        use pocket_ui::features::{available_on, by_id, FEATURES};
        // A card exists on a device exactly when its Settings row does (no Stick lights on an H700 without sticks, no
        // Haptics, Sync, Tailscale, Tools or Performance & fan under NextUI).
        for (sticks, nextui_owned) in [(true, false), (false, false)] {
            for (id, opens) in [("leds", Opens::Leds), ("haptics", Opens::Haptics), ("sync", Opens::Sync), ("tailscale", Opens::Tailscale), ("tools", Opens::Tools), ("perf_fan", Opens::PerfFan)] {
                let card = available_on(sticks, nextui_owned).any(|(_, f)| f.id == id);
                assert_eq!(card, shown_on(opens, sticks, nextui_owned), "{id} (sticks {sticks}, NextUI {nextui_owned})");
            }
        }
        let all = available_on(true, false).count();
        assert_eq!(all, FEATURES.iter().filter(|f| f.todo.is_none()).count(), "a handheld with sticks keeps every card");
        assert_eq!(available_on(false, false).count(), all - 1, "an H700 without sticks loses Stick lights only");
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
        let names: Vec<&str> = SETTINGS_GROUPS.iter().flat_map(|c| c.1.iter().map(|r| r.0)).collect();
        assert_eq!(SETTINGS.iter().map(|r| r.0).collect::<Vec<_>>(), names);
        for g in 0..SETTINGS_GROUPS.len() {
            for row in span(g) {
                assert_eq!(group_of(row), g);
            }
        }
        // Short lists: the point of the categories. Seven categories, nine rows at most
        // (2026-10-30, nine since Screen colour): a list is never longer than one page, also with Large text.
        assert!(SHOWN <= 8, "at most eight categories (Settings v2)");
        // A category counts its rows as each platform shows them (`device::hides_in`): a Linux handheld (every row any of them
        // has), dArkOS (with Back to EmulationStation) and Android (its own rows, without the ones only Linux has). Until 19.11.
        // only View and Back to EmulationStation were told apart, and Games counted Android games beside From EmulationStation
        // and Screenshots, which no platform shows together.
        for (platform, android, darkos) in [("Linux", false, false), ("dArkOS", false, true), ("Android", true, false)] {
            let shown = |k: &str| !pocket_ui::device::hides_in(k, true, false, android) || (darkos && k == "darkos_es");
            let seen = |c: &(&str, &[SettingsRow])| c.1.iter().filter(|r| device_key(r.1).is_none_or(shown)).count();
            for c in SETTINGS_GROUPS.iter() {
                assert!(seen(c) <= 9, "{} holds {} rows on {platform}: at most 9", c.0, seen(c));
            }
        }
        // And no category is a list of one or two rows where every row is there (Android hides whole parts of some).
        assert!(SETTINGS_GROUPS.iter().all(|c| c.1.len() >= 3), "every category holds at least 3 rows");
        let large_row = pocket_ui::type_scale::grown_in(pocket_ui::type_scale::TextSize::Large, 36) as i32 - 36;
        assert!(9 * (66 + large_row * 6 / 5) <= 1080 - 268, "nine Large rows fit on the page");
        let mut seen = std::collections::BTreeSet::new();
        assert!(SETTINGS.iter().all(|r| seen.insert(r.0)), "no row twice: settings_row finds rows by name");
    }

    /// The tree of 2026-11-13 (Alex: "For tinkerers" hid Tailscale and SSH): every network service is a row of
    /// Connections, the first category, and B on any of them closes Settings; no row opens a list of rows.
    #[test]
    fn every_network_service_is_a_row_of_connections_and_b_closes_settings() {
        let connections = category("Connections");
        assert_eq!(connections, 0, "Connections is the first category: START lands on it");
        assert_eq!(SETTINGS_GROUPS[connections].1.iter().map(|r| r.0).collect::<Vec<_>>(),
            ["Wi-Fi", "Bluetooth", "Tailscale", "Remote access (SSH)", "Sync", app_pocket_sync::SETTINGS_NAME, app_web::SETTINGS_NAME, app_moonlight::SETTINGS_NAME]);
        for name in ["Tailscale", "Remote access (SSH)", "Sync", app_moonlight::SETTINGS_NAME] {
            let at = settings_row(name);
            assert_eq!(SETTINGS[at].0, name, "{name} is a row of Settings");
            assert_eq!(group_of(at), connections, "{name} is in Connections");
            assert_eq!(groups_input(SettingsPage::List(at), CANCEL), None, "{name}: B closes Settings");
            assert!(!rows_in(connections, true, false, true).contains(&at), "Android hides {name}");
        }
        for name in ["Tailscale", "Remote access (SSH)", "Sync"] {
            assert!(!rows_in(connections, false, true, false).contains(&settings_row(name)), "host-owned networking hides {name}");
        }
        let android: Vec<&str> = rows_in(connections, true, false, true).iter().map(|r| SETTINGS[*r].0).collect();
        assert_eq!(android, ["Pocket Sync"], "Android's handheld view: Pocket Sync alone (0.6.0)");
        assert!(!SETTINGS.iter().any(|r| r.0 == "For tinkerers"), "no page of rows behind a row");
    }

    /// START lands on Connections, the first category; Android's handheld view lands on Look (A there opens Look's list:
    /// with a fixed first category, A did nothing while Look was painted selected), also since its Connections has Pocket
    /// Sync (0.6.0).
    #[test]
    fn start_opens_the_first_category_this_device_has() {
        assert_eq!(first_group_in(false), category("Connections"));
        // Android's handheld view keeps opening on Look when its Connections has a row (Pocket Sync, 0.6.0).
        let all: Vec<usize> = (0..SHOWN).collect();
        assert_eq!(start_group(&all, true), category("Look"), "Android with Connections shown: Look");
        assert_eq!(start_group(&all, false), category("Connections"));
        let android = first_group_in(true);
        assert_eq!(android, category("Look"));
        assert!(!rows_in(android, true, false, true).is_empty());
        assert_eq!(first_group(), category("Connections"), "the test machine is a Linux handheld");
    }

    /// Settings v2 (2026-11-16, SETTINGS-IA-REVIEW and -FABLE): every row in its category, in its order. Controls & sound
    /// mixed buttons, menu clicks and a reading aid under a loudspeaker, and Volume and Brightness were nowhere in Settings.
    #[test]
    fn every_row_is_in_its_category_of_settings_v2() {
        let tree: Vec<(&str, Vec<&str>)> = SETTINGS_GROUPS.iter().map(|c| (c.0.trim_end_matches('\u{200b}'), c.1.iter().map(|r| r.0).collect())).collect();
        let want: [(&str, &[&str]); 8] = [
            ("Connections", &["Wi-Fi", "Bluetooth", "Tailscale", "Remote access (SSH)", "Sync", "Pocket Sync", "Web companion", "PC streaming"]),
            ("Look", &["Layout", "Appearance", "Colours", "Style", "Text size", "Motion", "Game start animation", "Cozy extras", "Lived-in shelf"]),
            ("Screen & sound", &["Brightness", "Volume", "Screen colour", "Night light", "Menu sounds", "Case sounds", "Photos of big moments"]),
            ("Controls", &["Buttons", "Controller setup", "Test buttons", "Haptics", "LEDs"]),
            // Zoom in the pause menu since 0.5.1 (too prominent in Look, under Text size; a hand walk).
            ("Playing", &["On start", "Continue where I quit", "Console boot screen", "Remind me where I was", "Screen frames", "Over the game", "Zoom in the pause menu", "Performance & fan", "RetroAchievements"]),
            ("Games", &["Add games", "Box art & downloads", "Game check", "Systems on the shelf", "Shelf space", "Curator", "Screenshots", "From EmulationStation", "Android games", "RomM"]),
            ("System", &["Back to EmulationStation", "Updates", "Language", "View", "Sleep & power", "Battery", "Time", "Storage", "Backup & reset", "Kids mode"]),
            ("Help & tools", &["Manual", "Tour", "How you like to play", "First-start setup", "Tools", "Report a problem", "About"]),
        ];
        let want: Vec<(&str, Vec<&str>)> = want.iter().map(|(c, r)| (*c, r.to_vec())).collect();
        assert_eq!(tree, want);
        // The switches keep their pages and values: only the place and the name moved.
        assert!(matches!(SETTINGS[settings_row("Game start animation")].1, Opens::Animations));
        assert!(matches!(SETTINGS[settings_row("Menu sounds")].1, Opens::Sounds));
        assert!(matches!(SETTINGS[settings_row("Controller setup")].1, Opens::Controllers));
        // Android: Controls keeps Buttons and Controller setup, Screen & sound the levels and the sounds.
        let android = |g: usize| rows_in(g, true, false, true).iter().map(|i| SETTINGS[*i].0).collect::<Vec<_>>();
        assert_eq!(android(category("Controls")), ["Buttons", "Controller setup"]);
        assert_eq!(android(category("Screen & sound")), ["Brightness", "Volume", "Menu sounds", "Case sounds", "Photos of big moments"]);
        // Controller setup's line names where pairing is, only where that row is.
        let ctl = settings_row("Controller setup");
        assert!(row_line_in(ctl, false).contains("Connections › Bluetooth"), "{}", row_line_in(ctl, false));
        assert!(!row_line_in(ctl, true).contains("Bluetooth"), "Android pairs in its own settings");
    }

    /// Each category its own symbol (Settings v2): the controller is Controls', the loudspeaker Screen & sound's, and
    /// Playing has the play button, so no symbol means two things in the row.
    #[test]
    fn every_category_has_a_symbol_of_its_own() {
        use pocket_ui::ui_art::Icon;
        assert_eq!(ICONS[category("Controls")], Icon::Playing, "the gamepad");
        assert_eq!(ICONS[category("Screen & sound")], Icon::Sound);
        assert_eq!(ICONS[category("Playing")], Icon::Play);
        assert_eq!(RETRO_ICONS[category("Controls")], pocket_ui::retro::Icon::Playing, "Retro's controller");
        assert_eq!(RETRO_ICONS[category("Playing")], pocket_ui::retro::Icon::Continue, "Retro's play button");
        for (k, i) in ICONS.iter().enumerate() {
            assert!(ICONS.iter().skip(k + 1).all(|j| j != i), "{i:?} twice");
        }
        for (k, i) in RETRO_ICONS.iter().enumerate() {
            assert!(RETRO_ICONS.iter().skip(k + 1).all(|j| j != i), "Retro {i:?} twice");
        }
    }

    /// Settings in seven categories (2026-10-30): every choice has a row, none only on a button (Y, R1, L1, X, R2 on the look
    /// page were the only way to some), and each is in one place.
    #[test]
    fn every_choice_has_one_row_of_its_own() {
        for name in ["Layout", "Appearance", "Colours", "Style", "Text size", "Screen colour", "Systems on the shelf", "Game start animation", "Motion", "Buttons", "Language"] {
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

    /// What takes up each row Android's handheld view shows (HANDHELD-LEFTOVERS, 2026-11-13). A row there is a switch that does
    /// something on Android: the Linux interface itself reads it (the shelf, its pages and sounds), the player is handed it
    /// (`android_handheld::launch_settings`, `GamePicture`), or it is Android's own page. A new row has no entry until somebody
    /// has looked at who reads it on Android; then this test fails and says so.
    const ANDROID_ROWS: &[(&str, &str)] = &[
        ("Layout", "the shelf"),
        ("Systems on the shelf", "the shelf"),
        ("Cozy extras", "the shelf; the player is handed extras"),
        ("Lived-in shelf", "the shelf (`app_cabinet::shelf_row`)"),
        ("Shelf space", "Home, the systems' rows and the crate (`app_crate`)"),
        ("Appearance", "every page"),
        ("Colours", "every page; handed to the player"),
        ("Style", "every page; the font is handed to the player"),
        ("Text size", "every page; handed to the player"),
        ("Game start animation", "the shelf; handed to the player as cozy"),
        ("Motion", "every page; handed to the player"),
        ("Brightness", "this app's window (`android_controls`), as the quick settings' row"),
        ("Volume", "the media volume (`android_controls`), as the quick settings' row"),
        ("Menu sounds", "the UI sounds (AAudio)"),
        ("Case sounds", "the shelf"),
        ("On start", "the start of the app (`on_start::decide`)"),
        ("Remind me where I was", "the shelf's Where was I"),
        ("Console boot screen", "the player: its switch from this view's settings, the BIOS from its private folder (`boot_screen`)"),
        ("Buttons", "the hints; the prompts are handed to the player"),
        ("Controller setup", "the menus' buttons per controller (`pad::bindings`), as from the pause menu's Controls"),
        ("Screen frames", "the frame every game starts with (`options` globals, `GamePicture`)"),
        ("RetroAchievements", "the player scores into the shared tallies"),
        ("Add games", "Android's ROMs folders"),
        ("Game check", "the library"),
        ("Box art & downloads", "box art and the starter pack"),
        ("Curator", "the shelf"),
        ("Photos of big moments", "the player's camera (`album::Camera` in android/src/player.rs) and the case's album; no Also in play there"),
        ("Android games", "the Android shelf (app_android_games, launcher.json's marks)"),
        ("RomM", "the shelf's RomM games and their downloads (`app_romm`, `romm_net::AndroidNet`)"),
        ("Language", "every page; handed to the player"),
        ("View", "Android's own row"),
        ("Updates", "Android's signed list and installer (app_update_hint)"),
        ("Pocket Sync", "Pocket-to-Pocket sync with the player's saves (pocket_sync::Layout::android, app_android::pocket_sync_layout)"),
        ("Kids mode", "the shelf"),
        ("First-start setup", "the setup"),
        ("How you like to play", "the setup"),
        ("Tour", "the cards"),
        ("Manual", "the manual"),
        ("About", "the page"),
    ];

    /// The class behind "First-start setup names Wi-Fi" and "Screen frame offers Glow": every row of Settings that Android's handheld
    /// view shows is one something there reads (`ANDROID_ROWS` names who), and every row that nothing reads is out. Zoom in the pause
    /// menu, Over the game, Continue where I quit and Screenshots were switches that did nothing on Android.
    #[test]
    fn every_settings_row_android_shows_has_an_effect_there() {
        let mut shown: Vec<&str> = Vec::new();
        for g in 0..SETTINGS_GROUPS.len() {
            shown.extend(rows_in(g, true, false, true).iter().map(|i| SETTINGS[*i].0));
        }
        for row in &shown {
            assert!(ANDROID_ROWS.iter().any(|(n, _)| n == row), "Android shows the row {row:?}, and nothing says who reads it there: hide it (device::ANDROID_HIDES) or name its reader in ANDROID_ROWS");
        }
        for (name, _) in ANDROID_ROWS {
            assert!(shown.contains(name), "ANDROID_ROWS lists {name:?}, which Android does not show");
        }
        for gone in ["Zoom in the pause menu", "Over the game", "Continue where I quit", "Screenshots"] {
            assert!(!shown.contains(&gone), "{gone} on Android");
        }
        let linux: Vec<&str> = (0..SETTINGS_GROUPS.len()).flat_map(|g| rows_in(g, true, false, false)).map(|i| SETTINGS[i].0).collect();
        for kept in ["Zoom in the pause menu", "Over the game", "Continue where I quit", "Screenshots"] {
            assert!(linux.contains(&kept), "{kept} stays on Linux");
        }
    }

    /// Words of the Linux handheld, in the three languages, that no page of Android's handheld view may say: it runs no ROCKNIX or
    /// pocket-core, has no SD-card folders, Wi-Fi, Bluetooth, fan or power rows of ours, no file sharing, no rewind, no Zoom in the
    /// pause menu, START + Vol keys or "Pocket" (ANDROID-READINESS 13.11.: the Odin 2 Mini's Tour, Manual, quick settings,
    /// RetroAchievements and Runs on pages said them). A phrase is found anywhere, a single word as a word ("fan", not "fans").
    const LINUX_ONLY_WORDS: &[&str] = &[
        "ROCKNIX", "pocket-core", "EmulationStation", "SD card", "SD-Karte", "tarjeta SD", "Wi-Fi", "WLAN", "Bluetooth", "fan", "Lüfter",
        "ventilador", "rewind", "Zurückspulen", "retroceso", "Pocket", "Vol+", "Vol-", "file sharing", "Dateifreigabe", "share them",
        "freigeben", "compartirlos", "Zoom in the pause menu", "Lupe", "lupa", "Power off", "Sleep & power", "START in the pause menu",
        "An emulator app on this device can play", "Linux handheld", "Linux-Handheld", "consola con Linux", "Screen colour",
        "Bildschirmfarbe", "Color de pantalla",
    ];

    fn names_word(text: &str, word: &str) -> bool {
        if word.chars().all(char::is_alphanumeric) {
            text.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '+')).any(|w| w == word)
        } else {
            text.contains(word)
        }
    }

    /// Every text Android's handheld view shows where the Linux handheld's words once stood, by page: `(page, translated, texts)`.
    /// The manual is English on every device (`paint_manual`), the rest goes through the language tables. A new Android page with
    /// its own words belongs here.
    fn android_texts() -> Vec<(&'static str, bool, Vec<String>)> {
        use pocket_ui::features::{available_in, basic_in, line_in, BASICS};
        let own = |v: Vec<&str>| v.into_iter().map(String::from).collect::<Vec<_>>();
        vec![
            ("Tour: the basics", true, own(BASICS.iter().map(|b| basic_in(b, true)).flat_map(|b| [b.title, b.text, b.place]).collect())),
            ("Tour and first start: the feature cards", true, own(available_in(true, false, true).flat_map(|(_, f)| [f.title, line_in(f, true), f.place]).collect())),
            ("Quick settings", true, own([pocket_ui::quick::Row::Brightness, pocket_ui::quick::Row::Volume].map(|r| pocket_ui::quick_paint::level_line(r, true)).to_vec())),
            ("Runs on without a download channel", true, vec![
                pocket_ui::emu_choice_page::closed_line(&[]),
                pocket_ui::emu_choice_page::closed_line(&[("org.ppsspp.ppsspp".into(), "PPSSPP".into())]),
            ]),
            ("RetroAchievements", true, own(pocket_ui::achievements_page::about_in(true).into_iter().chain([pocket_ui::achievements_page::password_note_in(true)]).collect())),
            ("Manual", false, own(pocket_ui::manual::ANDROID_PAGES.iter().map(|(_, md)| *md).collect())),
            ("First start: the welcome", true, vec![pocket_ui::setup::welcome_title(true).to_string()]),
            ("Tour: Big text's picture", true, pocket_ui::feature_previews::easy_to_read_rows().into_iter().map(|(name, _)| name.to_string()).collect()),
            // The files dialog is Java with its own three languages: all of them are in the text.
            ("The files dialog", false, vec![{
                let java = include_str!("../android/java/xyz/pocketui/app/HandheldActivity.java");
                let at = java.find("private void askForFiles()").expect("the dialog");
                java[at..at + java[at..].find("onRequestPermissionsResult").unwrap()].to_string()
            }]),
        ]
    }

    /// The class behind "the Tour promises Wi-Fi, Bluetooth, fan and power", "Manual is the ROCKNIX guide", "Vol+ on Android" and
    /// "an emulator app can play it" with no app (Odin 2 Mini, 13.11.): no Android page names what only Linux has, in any language.
    /// Linux keeps its words (the same table on Linux finds them), so the test can see them.
    #[test]
    fn android_pages_name_nothing_only_linux_has() {
        let mut found = Vec::new();
        for (page, translated, texts) in android_texts() {
            let langs: Vec<pocket_ui::i18n::Lang> = if translated { pocket_ui::i18n::Lang::ALL.to_vec() } else { vec![pocket_ui::i18n::Lang::En] };
            for text in &texts {
                for lang in &langs {
                    let shown = if translated { pocket_ui::i18n::tr_in(*lang, text) } else { text.clone() };
                    // A console's own name is no Linux word.
                    let shown = shown.replace("Neo Geo Pocket", "Neo Geo");
                    for word in LINUX_ONLY_WORDS.iter().filter(|w| names_word(&shown, w)) {
                        found.push(format!("{page} ({lang:?}) says {word:?}: {shown:?}"));
                    }
                }
            }
        }
        assert!(found.is_empty(), "Android's handheld view names what only Linux has:\n{}", found.join("\n"));
        // The same pages on Linux keep their words.
        let tour: Vec<&str> = pocket_ui::features::BASICS.iter().map(|b| pocket_ui::features::basic_in(b, false).text).collect();
        assert!(tour.iter().any(|t| t.contains("Wi-Fi")) && tour.iter().any(|t| t.contains("SD card")), "{tour:?}");
        assert!(pocket_ui::quick_paint::level_line(pocket_ui::quick::Row::Brightness, false).contains("Vol+"));
        assert!(pocket_ui::achievements_page::about_in(false)[0].contains("pocket-core"));
        assert!(pocket_ui::manual::PAGES[0].1.contains("ROCKNIX"));
        assert_eq!(pocket_ui::setup::welcome_title(false), "Welcome to your Pocket");
    }

    /// The emulator app the line names is one the phone has; without one the line says so, and never names an app.
    #[test]
    fn a_closed_channel_names_an_app_only_where_the_phone_has_one() {
        use pocket_ui::emu_choice_page::closed_line;
        let with = closed_line(&[("org.ppsspp.ppsspp".into(), "PPSSPP".into())]);
        assert!(with.contains("PPSSPP") && with.contains("Runs on"), "{with}");
        let without = closed_line(&[]);
        assert!(without.contains("no emulator app") && !without.contains("can play it"), "{without}");
        for lang in pocket_ui::i18n::Lang::ALL {
            for line in [&with, &without] {
                let shown = pocket_ui::i18n::tr_in(lang, line);
                assert!(lang == pocket_ui::i18n::Lang::En || shown != *line, "{lang:?} has no translation of {line:?}");
            }
        }
    }

    /// Under each row Settings says what it does. On Android no line names what the row does not have there: the setup has no Wi-Fi
    /// step, About no network, Add games no file sharing, Extras no PortMaster. Linux keeps its lines.
    #[test]
    fn android_s_row_lines_name_nothing_it_does_not_have() {
        let banned = ["Wi-Fi", "WLAN", "PortMaster", "network", "Netzwerk", "file sharing", "Dateifreigabe", "SD card", "SD-Karte", "EmulationStation", "fan", "Lüfter"];
        for g in 0..SETTINGS_GROUPS.len() {
            for i in rows_in(g, true, false, true) {
                let line = row_line_in(i, true);
                for lang in pocket_ui::i18n::Lang::ALL {
                    let text = pocket_ui::i18n::tr_in(lang, line);
                    for word in banned {
                        assert!(!text.contains(word), "{lang:?} {}: {text:?} names {word}, which Android does not have", SETTINGS[i].0);
                    }
                }
            }
        }
        let first = SETTINGS.iter().position(|s| matches!(s.1, Opens::FirstStart)).unwrap();
        assert!(row_line_in(first, false).contains("Wi-Fi"), "Linux's setup has the Wi-Fi step");
    }

    /// The Tour's and the first start's cards follow the same list: no card for what Android has no switch for.
    #[test]
    fn android_s_cards_are_the_features_it_has() {
        use pocket_ui::features::available_in;
        let ids: Vec<&str> = available_in(true, false, true).map(|(_, f)| f.id).collect();
        for gone in ["quit_without_saving", "pixel_path", "pixel_path_live", "run_ahead", "haptics", "leds", "sync", "tailscale", "tools", "perf_fan"] {
            assert!(!ids.contains(&gone), "the card {gone} on Android");
        }
        let linux: Vec<&str> = available_in(true, false, false).map(|(_, f)| f.id).collect();
        for kept in ["quit_without_saving", "pixel_path", "run_ahead"] {
            assert!(linux.contains(&kept), "the card {kept} stays on Linux");
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
    fn moving_stays_in_the_category_and_goes_round_at_its_ends() {
        let r = rows(category("Connections"));
        let first = SettingsPage::List(r[0]);
        let last = SettingsPage::List(*r.last().unwrap());
        assert_eq!(groups_input(SettingsPage::List(r[1]), Button::Up), Some(first), "Up moves a row up");
        assert_eq!(groups_input(last, Button::Down), Some(first), "Down on the last row: the category's first");
        // Left/Right are the row's own (a switch changes); here they move nothing.
        assert_eq!(groups_input(first, Button::Right), Some(first));
    }

    /// Package 2 of the tree (SETTINGS-IA-DECISION): START shows rows at once, L1/R1 go through the categories and round at
    /// the ends, each arriving on its category's first row, and B leaves Settings for the shelf in one press (it took two).
    /// On every profile: a Linux handheld, one without sticks, NextUI's Brick card, Android's handheld view (Connections: Pocket Sync).
    #[test]
    fn l1_and_r1_reach_every_category_and_b_closes_settings() {
        let next = |shown: &[usize], g: usize| group_after(shown, g, 1);
        for (sticks, nextui_owned, android) in [(true, false, false), (false, false, false), (true, true, false), (true, false, true)] {
            let shown: Vec<usize> = (0..SHOWN).filter(|g| !rows_in(*g, sticks, nextui_owned, android).is_empty()).collect();
            let mut g = shown[0];
            let mut seen = vec![g];
            for _ in 1..shown.len() {
                g = next(&shown, g);
                seen.push(g);
            }
            assert_eq!(seen, shown, "R1 visits every category once, in order ({sticks} {nextui_owned} {android})");
            assert_eq!(next(&shown, g), shown[0], "R1 on the last: the first again");
            assert_eq!(group_after(&shown, shown[0], -1), *shown.last().unwrap(), "L1 on the first: the last");
        }
        assert_eq!(first_group_in(true), category("Look"), "Android starts on Look");
        // On this machine, through the input itself: R1 from the start, all the way round, B out.
        let start = SettingsPage::List(first_row());
        assert_eq!(group_of(first_row()), first_group());
        let mut page = start;
        let mut visited = Vec::new();
        for _ in 0..shown_groups().len() {
            let SettingsPage::List(row) = page else { panic!("{page:?}") };
            assert_eq!(rows(group_of(row)).first(), Some(&row), "a category opens on its first row");
            visited.push(group_of(row));
            page = groups_input(page, Button::R1).unwrap();
        }
        assert_eq!(visited, shown_groups());
        assert_eq!(page, start, "round again to the start");
        assert_eq!(groups_input(groups_input(start, Button::L1).unwrap(), Button::R1), Some(start));
        assert_eq!(groups_input(SettingsPage::List(first_row() + 1), CANCEL), None, "B in a list: out of Settings");
    }

    /// A hand test of 0.5 (15.11.): L1/R1 as the only way between the categories is "meh" on a simple handheld. The
    /// tab pattern of console and TV settings: Up on a category's first row puts the focus on the row of symbols at the top;
    /// there Left/Right (and L1/R1) go to the previous/next category at once, round at the ends, the rows under it showing
    /// that category; Down or A goes back into the rows, on the first; Up stays; B leaves Settings as from the rows.
    #[test]
    fn up_on_the_first_row_reaches_the_categories_and_left_right_switch_them() {
        let start = SettingsPage::List(first_row());
        let bar = groups_input(start, Button::Up).unwrap();
        assert_eq!(bar, SettingsPage::Tabs(first_row()), "Up on the first row: the categories at the top");
        assert_eq!(groups_input(bar, Button::Up), Some(bar), "Up on the categories: stays");
        assert_eq!(groups_input(bar, Button::Down), Some(start), "Down: back to the first row");
        assert_eq!(groups_input(bar, CONFIRM), Some(start), "A: into the rows, on the first");
        assert_eq!(groups_input(bar, CANCEL), None, "B on the categories: out of Settings");
        let shown = shown_groups();
        let mut page = bar;
        let mut visited = Vec::new();
        for _ in 0..shown.len() {
            let SettingsPage::Tabs(row) = page else { panic!("Right left the categories: {page:?}") };
            assert_eq!(rows(group_of(row)).first(), Some(&row), "the rows under it are the category's, from its first");
            visited.push(group_of(row));
            page = groups_input(page, Button::Right).unwrap();
        }
        assert_eq!(visited, shown, "Right visits every category once, in order");
        assert_eq!(page, bar, "and comes round to the first");
        assert_eq!(groups_input(groups_input(bar, Button::Left).unwrap(), Button::Right), Some(bar), "Left is Right's way back");
        assert_eq!(groups_input(bar, Button::R1), groups_input(bar, Button::Right), "R1 there is Right");
        assert_eq!(groups_input(bar, Button::L1), groups_input(bar, Button::Left), "L1 there is Left");
        let look = SettingsPage::Tabs(rows(category("Look"))[0]);
        let SettingsPage::List(row) = groups_input(look, Button::Down).unwrap() else { panic!() };
        assert_eq!(group_of(row), category("Look"), "Down goes into the category chosen up there");
    }

    /// The header fits on one line exactly when its parts and the gaps do; otherwise the slim line under the title takes it.
    #[test]
    fn the_header_takes_a_line_of_its_own_only_when_it_does_not_fit() {
        let (cap, symbols) = (CAP_W, 7 * SYM + 6 * SYM_GAP);
        let need = 2 * cap + 2 * 16 + symbols + 12 + 300;
        assert!(header_fits(cap, symbols, 300, need));
        assert!(!header_fits(cap, symbols, 300, need - 1));
    }


    /// A stranger looks for volume, brightness and BIOS in Settings (daily-use walk): "Sounds 70%" looked like the game
    /// volume, and nothing named BIOS. The lines say where those really are.
    #[test]
    fn beginners_find_volume_brightness_and_bios_from_settings() {
        let line = |name: &str| SETTINGS.iter().find(|r| r.0 == name).unwrap().2;
        // Settings v2: Volume and Brightness are rows of their own, in Screen & sound, and the menus' clicks say what they are.
        let screen = category(super::super::SCREEN_SOUND_CATEGORY);
        for name in ["Brightness", "Volume"] {
            assert_eq!(group_of(settings_row(name)), screen, "{name} is a row of Screen & sound");
            assert!(line(name).contains("quick settings"), "{}", line(name));
        }
        assert!(!SETTINGS.iter().any(|r| r.0 == "Sounds"), "no row \"Sounds\" that reads like the volume");
        assert!(line("Menu sounds").contains("menus"), "{}", line("Menu sounds"));
        assert!(line("Add games").contains("BIOS"), "{}", line("Add games"));
        // Pixel Path is the enthusiasts' (0.4): the row everyone reads names what everyone gets.
        assert!(line("Over the game").contains("frame rate") && !line("Over the game").contains("Pixel Path"), "{}", line("Over the game"));
    }
}

/// The rows the TrimUI Brick leaves out beside what NextUI owns (docs/TRIMUI-BRICK.md): everything that needs a
/// compositor effect (night light, the picture over the game, screenshots; screen frames only while the bridge has no GPU path, `brick_picture`), ROCKNIX (From EmulationStation, Sleep & power
/// through systemd, PC streaming), or a network state this system does not give pocket-ui (Wi-Fi is NextUI's): the starter pack,
/// pictures & extras, the web companion, RetroAchievements. A row that does nothing is worse than none. The Smart Pro has sticks, so `hides` keeps its lights row: it has nothing of ours to light or assign there (`brick_profile::lights_page`).
#[cfg(feature = "brick")]
fn brick_hides(opens: Opens) -> bool {
    pocket_ui::device::brick() && (brick_hides_with(opens, pocket_ui::brick_profile::standalone()) || matches!(opens, Opens::Leds) && !pocket_ui::brick_profile::lights_page())
}

/// The Brick's own card has these of what NextUI's card leaves out (docs/TRIMUI-BRICK-REMOTE.md): Wi-Fi, Remote access (the card's
/// dropbear), Tailscale (the card's), Updates (its private channel) and Sync (the card's Syncthing, docs/TRIMUI-BRICK-SYNC.md).
/// Bluetooth uses the card service (docs/TRIMUI-BRICK-BLUETOOTH.md); the other firmware-owned rows stay out.
#[cfg(feature = "brick")]
fn brick_keeps(opens: Opens) -> bool {
    pocket_ui::device::brick() && brick_keeps_with(opens, pocket_ui::brick_profile::standalone())
}

#[cfg(feature = "brick")]
fn brick_keeps_with(opens: Opens, standalone: bool) -> bool {
    standalone && matches!(opens, Opens::Setup(Only::Wifi | Only::Remote) | Opens::Tailscale | Opens::Updates | Opens::Kids | Opens::Sync | Opens::PerfFan | Opens::Bluetooth)
}

/// `brick_hides` on a Brick, with NextUI's card or `standalone` (its own card has Wi-Fi of its own, so the pictures row stays: the
/// automatic cover fetch is on by default there, and this is where it is switched off or box art only is chosen).
#[cfg(feature = "brick")]
fn brick_hides_with(opens: Opens, standalone: bool) -> bool {
    if matches!(opens, Opens::Frames) && pocket_ui::brick_profile::picture_rows_out() {
        return true;
    }
    !(standalone && matches!(opens, Opens::Setup(Only::Extras | Only::Games) | Opens::SleepPower | Opens::NightLight | Opens::Achievements | Opens::OverGame | Opens::Screenshots | Opens::Web))
        && matches!(
            opens,
            Opens::NightLight
                | Opens::OverGame
                | Opens::Screenshots
                | Opens::Web
                | Opens::Gamelist
                | Opens::SleepPower
                | Opens::PcStreaming
                | Opens::Achievements
                | Opens::Setup(Only::Extras | Only::Games)
        )
}

#[cfg(all(test, feature = "brick"))]
mod brick_standalone_tests {
    use super::*;


    /// Alex's standalone boot (2026-11-03): no covers appeared. The pictures row (Settings > Games > Box art & downloads) is the one place the
    /// automatic fetch is switched off, so on the card without NextUI it stays reachable and the fetch is on until chosen off.
    #[test]
    fn the_standalone_card_keeps_the_pictures_row_and_fetches_by_default() {
        let extras = Opens::Setup(Only::Extras);
        assert!(brick_hides_with(extras, false), "with NextUI's card the pictures row stays out");
        assert!(!brick_hides_with(extras, true), "the standalone card keeps it");
        for still_hidden in [Opens::PcStreaming] {
            assert!(brick_hides_with(still_hidden, true));
        }
        let games = Opens::Setup(Only::Games);
        assert!(brick_hides_with(games, false), "NextUI's card has its own way to add games");
        assert!(!brick_hides_with(games, true), "the standalone card shows Add games (phone page, file sharing)");
        for (n, kept) in [Opens::SleepPower, Opens::NightLight, Opens::Achievements].into_iter().enumerate() {
            assert!(brick_hides_with(kept, false), "with NextUI's card page {n} stays out");
            assert!(!brick_hides_with(kept, true), "the standalone card keeps page {n}");
        }
        let fresh = pocket_ui::settings::Settings::default();
        assert!(!fresh.art_off, "nothing is switched off on a new card");
        assert_eq!(pocket_ui::settings::Art::from_flags(fresh.box_art_only, fresh.art_off), pocket_ui::settings::Art::Pictures);
    }

    /// The starter pack on the Brick's own card: every system it holds games for is a folder the card's shelf reads, so a game
    /// that is added shows up (the download goes to the shelf's folder, the work folder is in the data dir on the card).
    #[test]
    fn the_starter_pack_only_asks_for_systems_the_cards_shelf_has() {
        let cfg = include_str!("../deploy/brick/es_systems.cfg");
        let games = pocket_ui::starter::games();
        assert!(!games.is_empty());
        for g in games {
            assert!(cfg.contains(&format!("<name>{}</name>", g.system)), "{} ({}) has no shelf on the card", g.title, g.system);
        }
    }

    /// docs/TRIMUI-BRICK-MORE.md: Kids mode, the screenshots gallery, the web companion and Over the game are pocket-ui's own and
    /// need nothing of NextUI; the standalone card shows them, NextUI's card keeps them out.
    #[test]
    fn the_standalone_card_shows_kids_screenshots_web_and_over_the_game() {
        for (name, opens) in [("Screenshots", Opens::Screenshots), ("Web companion", Opens::Web), ("Over the game", Opens::OverGame)] {
            assert!(!brick_hides_with(opens, true), "{name} on the card");
            assert!(brick_hides_with(opens, false), "{name} with NextUI's card");
        }
        assert!(brick_keeps_with(Opens::Kids, true) && !brick_keeps_with(Opens::Kids, false));
    }
}
