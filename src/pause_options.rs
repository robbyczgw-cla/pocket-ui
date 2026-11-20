//! The Options page of Android's player in the handhelds' pause menu (`pause_list`, a game from the Handheld view), laid
//! out as Linux's (`main.rs` `opt_entries`, `paint_options`): Picture ›, Emulator ›, Controls ›, Cheats ›, then
//! what stands under Speed, rumble, fan on Linux and the player has (Earlier states, Over the game), Keep for, Restore
//! defaults. The same names, the same order, the same lines under the rows; Picture has Resolution where Linux has it on
//! the same handheld (`render_scale`, the chip class from Android's system properties). What the player has no counterpart
//! for is left out, each row with its reason in the tests' `NOT_ON_ANDROID`; Screenshot, which Linux has as HOME + A,
//! stays as the page's last row.
//!
//! Until 17.11.2026 the page was a list of its own: "This game: Zoom, Screenshot" first (the focus landed on Zoom), then
//! every option of the emulator by section (the Retroid Pocket Mini V2 hand test: "the options look completely different
//! from Linux, with the useless Zoom at the top").
//!
//! The Picture page is Linux's own table (`option_table`'s Picture group, since 18.11.2026 in the player's library too):
//! the rows, values, words and lines Linux shows for the console, as far as the player draws them (`picture_settings`),
//! the values Linux's keys in the game's `game_settings` files. Until then it was a copy of three rows with indexes of its own.
//!
//! The other rows' words are written here (the player has no `options`/`over_game`), and the tests (on Linux) hold them to
//! Linux's, so a change there fails here until both read the same.

use std::collections::BTreeMap;

/// A row of the Options page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Picture,
    /// The emulator's own options (the core's), by section, with "Advanced (this emulator)" last.
    Emulator,
    /// The player's control rows (Hold to toggle, the Disc of a game of several).
    Controls,
    Zoom,
    Cheats,
    /// Linux's group "Speed, rumble, fan" with the rows of it the player has (Earlier states, Over the game).
    Extras,
    /// Linux's Speed, rumble, fan › Fast-forward speed (`play_speed`): how fast HOME + R2 runs the game.
    FastForward,
    /// Off / On: Linux's Speed, rumble, fan › Rewind, HOME + L2 held plays backwards.
    Rewind,
    /// Off / On: a state every 5 minutes of play for the pause menu's Earlier (Linux: Speed, rumble, fan › Earlier states).
    Earlier,
    /// Off / FPS: the frame rate and the CPU in the top left corner (Linux: Speed, rumble, fan › Over the game).
    OverGame,
    /// Off / Low / Medium / High: Linux's Speed, rumble, fan › Rumble, how strongly games shake the handheld or the pad.
    Rumble,
    /// Where the Picture and Over the game choices are kept: this game, or every game of the console.
    Keep,
    /// Back to the defaults for the console and the game; asks for a second A (Linux's Restore defaults).
    Restore,
    Screenshot,
}

/// What the game has: an emulator with options, control rows, cheats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Has {
    pub emulator: bool,
    pub controls: bool,
    pub cheats: bool,
}

/// The page's rows in Linux's order (`opt_entries`: the groups, Emulator and Controls after Picture, Cheats after Controls,
/// Speed, rumble, fan (Shortcuts, Linux's HOME layer, is not on Android), Zoom after the groups (0.5.1: no longer second,
/// under Picture), Keep for, Restore defaults), Screenshot last.
pub fn top(has: Has) -> Vec<Entry> {
    let mut v = vec![Entry::Picture];
    if has.emulator {
        v.push(Entry::Emulator);
    }
    if has.controls {
        v.push(Entry::Controls);
    }
    if has.cheats {
        v.push(Entry::Cheats);
    }
    v.extend([Entry::Extras, Entry::Zoom, Entry::Keep, Entry::Restore, Entry::Screenshot]);
    v
}

/// A row of Linux's Options page (`main.rs` `opt_entries`): a group of `options::GROUPS` (Picture, Shortcuts, Speed,
/// rumble, fan), the emulator's pages, Zoom, Runs on, Keep for, Restore defaults.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinuxEntry {
    Group(usize),
    Emulator,
    Controls,
    /// The frozen picture larger (`app_zoom`).
    Zoom,
    Cheats,
    /// Which emulator plays the game (`app_emu_choice`): a page with its own Save changes.
    RunsOn,
    /// Where the changes are kept: this game, or every game of the console (`options::GameSettings::keep`).
    Keep,
    /// Back to the defaults for the console and the game; asks for a second A.
    Restore,
}

/// Linux's Options page (not the Brick's) for a game with `groups` groups, an emulator whose options page has rows or
/// not (`emulator`: None without pocket-core's snapshot), and cheats or not: the groups with Emulator and Controls after
/// the first one (Picture), Cheats after Controls, then Zoom, Runs on, Keep for, Restore defaults.
pub fn linux_entries(groups: usize, emulator: Option<bool>, cheats: bool) -> Vec<LinuxEntry> {
    let mut v: Vec<LinuxEntry> = (0..groups).map(LinuxEntry::Group).collect();
    if let Some(has_options) = emulator {
        let at = 1.min(v.len());
        v.insert(at, LinuxEntry::Controls);
        if has_options {
            v.insert(at, LinuxEntry::Emulator);
        }
    }
    if cheats {
        // After Controls (or Picture when there is no snapshot).
        let at = v.iter().position(|e| *e == LinuxEntry::Controls).or_else(|| (!v.is_empty()).then_some(0)).map_or(0, |i| i + 1);
        v.insert(at, LinuxEntry::Cheats);
    }
    // Zoom after what changes the game (`app_zoom`): it stood second, under Picture, and a niche help read as one of the
    // main things (0.5.1, the hand test: the Zoom row in Settings was too prominent too; it went down to Playing).
    v.extend([LinuxEntry::Zoom, LinuxEntry::RunsOn, LinuxEntry::Keep, LinuxEntry::Restore]);
    v
}

/// The rows of Speed, rumble, fan the player has, in Linux's order (`options::GROUPS`).
pub const EXTRAS_ROWS: [Entry; 5] = [Entry::FastForward, Entry::Rewind, Entry::Earlier, Entry::OverGame, Entry::Rumble];

/// Speed, rumble, fan's line: what the player's group holds (no performance, no fan: Android's own; review 19.11. #2).
pub const EXTRAS_DESC: &str = "Fast-forward, rewind, earlier states, the frame rate and rumble.";

impl Entry {
    /// The row's name, Linux's words.
    pub fn label(self) -> &'static str {
        match self {
            Entry::Picture => "Picture",
            Entry::Emulator => "Emulator",
            Entry::Controls => "Controls",
            Entry::Zoom => "Zoom",
            Entry::Cheats => "Cheats",
            // Android sets no fan: the group by its name without one (`i18n::FANLESS_NAMES`, review 19.11. #2).
            Entry::Extras => "Speed & rumble",
            Entry::FastForward => "Fast-forward speed",
            Entry::Rewind => "Rewind",
            Entry::Earlier => "Earlier states",
            Entry::OverGame => "Over the game",
            Entry::Rumble => "Rumble",
            Entry::Keep => "Keep for",
            Entry::Restore => "Restore defaults",
            Entry::Screenshot => "Screenshot",
        }
    }

    /// The game setting the row is on Linux's page (`options::GROUPS`), None for a page of its own or a row Linux has
    /// outside the groups (Keep for, Restore defaults).
    pub fn setting_key(self) -> Option<&'static str> {
        match self {
            Entry::FastForward => Some(crate::play_speed::FF_KEY),
            Entry::Rewind => Some(crate::play_speed::REWIND_KEY),
            Entry::Earlier => Some(crate::timeline_policy::KEY),
            Entry::OverGame => Some("over_game"),
            Entry::Rumble => Some(RUMBLE_KEY),
            _ => None,
        }
    }

    /// Whether A opens a page (">" at the right), as Linux's groups.
    pub fn opens(self) -> bool {
        matches!(self, Entry::Picture | Entry::Emulator | Entry::Controls | Entry::Zoom | Entry::Cheats | Entry::Extras)
    }

    /// The line at the bottom left while the row is selected: Linux's (`paint_options`) where Linux has the row.
    pub fn desc(self, keep_game: bool) -> &'static str {
        match self {
            Entry::Picture => PICTURE_DESC,
            Entry::Emulator => "Settings of the emulator itself, such as colours and palettes.",
            Entry::Controls => "Which button does what, and the left stick.",
            Entry::Zoom => "The picture larger, to read small text. Move with the D-pad.",
            Entry::Cheats => "Infinite lives and the like, from the libretro cheat database.",
            Entry::Extras => EXTRAS_DESC,
            Entry::FastForward => FF_DESC,
            Entry::Rewind => REWIND_DESC,
            Entry::Earlier => EARLIER_DESC,
            Entry::OverGame => OVER_GAME_DESC,
            Entry::Rumble => RUMBLE_DESC,
            Entry::Keep if keep_game => "Changes show at once and stay for this game. Left or right: every game on this console.",
            Entry::Keep => "Changes show at once and stay for every game on this console. Left or right: this game only.",
            Entry::Restore => RESTORE_DESC,
            Entry::Screenshot => "A picture of the game as it is now.",
        }
    }

    /// The heading of the page the row opens, Linux's (`paint_options`): a group's "Options › Picture", the Emulator,
    /// Controls and Cheats pages their own name.
    pub fn heading(self) -> String {
        match self {
            Entry::Emulator | Entry::Controls | Entry::Cheats => self.label().to_string(),
            _ => format!("Options \u{203a} {}", self.label()),
        }
    }
}

/// Over the game on Android: Off and FPS (the sensors and Pixel Path are Linux's).
pub const OVER_GAME_VALUES: [&str; 2] = ["off", "fps"];
/// The row's line: Linux's first line, what the player draws.
pub const OVER_GAME_DESC: &str = "FPS shows frames per second and CPU load in the top left corner.";

/// Fast-forward speed's values and their labels, Linux's (`options`' row), and its line.
pub const FF_VALUES: [(&str, &str); 5] = [("2", "2x"), ("3", "3x"), ("4", "4x"), ("8", "8x"), ("max", "As fast as possible")];
pub const FF_DESC: &str = "How fast the game runs while fast-forward is on.\nSound is muted meanwhile.";
/// The label of a speed; Left/Right step through `FF_VALUES` (`ff_step`).
pub fn ff_label(speed: crate::play_speed::FastForward) -> &'static str {
    let v = speed.value();
    FF_VALUES.iter().find(|(k, _)| *k == v).map_or("4x", |(_, l)| *l)
}
/// The speed `delta` steps from `speed` through Linux's choices, round.
pub fn ff_step(speed: crate::play_speed::FastForward, delta: i32) -> crate::play_speed::FastForward {
    let v = speed.value();
    let at = FF_VALUES.iter().position(|(k, _)| *k == v).unwrap_or(2) as i32;
    crate::play_speed::FastForward::parse(FF_VALUES[(at + delta).rem_euclid(FF_VALUES.len() as i32) as usize].0)
}
/// Rewind's line, Linux's.
pub const REWIND_DESC: &str = "Remembers the last minute so you can play it backwards.\nHold HOME + L2. Uses some extra power.";

/// Earlier states' line, Linux's (`options`' row).
pub const EARLIER_DESC: &str = "Keeps a state every 5 minutes of play, half an hour back,\nand where each session began: Earlier in this menu.\nOff by default for N64, PSP, DS, Dreamcast and Saturn (big states).";

/// "Off" / "On": Earlier states' value, Linux's.
pub fn earlier_label(on: bool) -> &'static str {
    if on {
        "On"
    } else {
        "Off"
    }
}

/// Restore defaults' line, and what it says after the first A and after the second: Linux's page uses these too.
pub const RESTORE_DESC: &str = "Removes the saved settings for this console and this game.";
pub const RESTORE_ASK: &str = "Press A again to restore the defaults. B keeps your settings.";
pub const RESTORE_DONE: &str = "Back to defaults.";

/// Linux's Picture group line names the orientation, which the player hasn't: the page says what it has.
pub const PICTURE_DESC: &str = "Size, look and frame.";

/// Controls → Hold to toggle's line, Linux's (`emu_menu::control_rows`).
pub const HOLD_DESC: &str = "Pick a button: one press holds it down, the next lets go.\nFor buttons you would hold, like run.";

/// "Off" / "FPS": the value's label, Linux's.
pub fn over_game_label(on: bool) -> &'static str {
    if on {
        "FPS"
    } else {
        "Off"
    }
}

/// "This game" / "This console": Keep for's value, Linux's.
pub fn keep_label(game: bool) -> &'static str {
    if game {
        "This game"
    } else {
        "This console"
    }
}

/// Where the handhelds' Options pages stand in Android's player: the page (None: the top page), the selected row, the
/// Emulator page's open category, and the rows a page or a category was opened from, so B goes back onto them (Linux's
/// `Page::Options` / `Page::Group` / `Page::Emulator`). The player does what a row does; this only moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OptNav {
    pub page: Option<Entry>,
    pub sel: usize,
    pub cat: Option<usize>,
    top_sel: usize,
    cat_sel: usize,
}

impl OptNav {
    /// Up (`-1`) or Down (`1`) on a page of `rows` rows, round.
    pub fn step(&mut self, delta: i32, rows: usize) {
        if rows > 0 {
            self.sel = (self.sel.min(rows - 1) as i32 + delta).rem_euclid(rows as i32) as usize;
        }
    }

    /// A on a top row that opens a page: the page, on its first row.
    pub fn open(&mut self, page: Entry) {
        self.top_sel = self.sel;
        self.page = Some(page);
        self.cat = None;
        self.sel = 0;
    }

    /// A on one of the core's categories on the Emulator page: its page, on its first row.
    pub fn open_category(&mut self, cat: usize) {
        self.cat_sel = self.sel;
        self.cat = Some(cat);
        self.sel = 0;
    }

    /// B: a category back onto its row of the Emulator page, a page back onto its row of the top page; false on the
    /// top page (the pause menu takes B).
    pub fn back(&mut self) -> bool {
        if self.cat.take().is_some() {
            self.sel = self.cat_sel;
            return true;
        }
        if self.page.take().is_some() {
            self.sel = self.top_sel;
            return true;
        }
        false
    }

    /// The pause menu opened Options: the top page, on Picture.
    pub fn reset(&mut self) {
        *self = OptNav::default();
    }
}

/// Resolution's line, Linux's.
pub const RESOLUTION_DESC: &str = "The resolution the 3D picture is drawn at. Original is the console's own; 2x and up are sharper and cost the GPU more. Takes effect the next time the game starts.";

/// Resolution's index (factor less one) after one step Left or Right, round through the factors this handheld offers
/// (`render_scale::offers`: Original up to the highest that held full speed).
pub fn resolution_step(range: Option<crate::render_scale::Range>, index: usize, delta: i32) -> usize {
    let n = range.map_or(1, |r| i32::from(r.max.clamp(1, 4)));
    (index.min(3) as i32 + delta).rem_euclid(n) as usize
}

/// Resolution's index for a saved choice (`render_scale::VALUES`, "1" to "4"), the handheld's default where none is
/// saved or it is more than offered: what pocket-core gives the core at the start (`render_scale::for_start`).
pub fn resolution_index(range: Option<crate::render_scale::Range>, chosen: Option<u8>) -> usize {
    let Some(r) = range else { return 0 };
    let f = chosen.filter(|f| (1..=r.max).contains(f)).unwrap_or(r.default);
    usize::from(f.clamp(1, 4)) - 1
}

/// Whether the Picture page of Android's player shows a row for `system`: Resolution where `range` says, Mode 7 where this
/// handheld offers it for the console (`mode7_hd::here_for`), every other row as Linux's (`applies_in`, Android's profile).
fn picture_row_shown(key: &str, system: &str, range: Option<crate::render_scale::Range>) -> bool {
    if key == crate::render_scale::KEY {
        range.is_some()
    } else if key == crate::mode7_hd::KEY {
        crate::mode7_hd::here_for(system).is_some()
    } else {
        crate::option_table::applies_in(key, system, true)
    }
}

/// Rumble's key, values and labels, and its line: Linux's (`option_table`).
pub const RUMBLE_KEY: &str = "rumble";
pub const RUMBLE_VALUES: [(&str, &str); 4] = [("off", "Off"), ("low", "Low"), ("medium", "Medium"), ("high", "High")];
pub const RUMBLE_DESC: &str = "How strongly games shake the console, for games that rumble.\nOff keeps it still.";
/// The label of a saved rumble value (Medium where none or an unknown one is saved, Linux's default).
pub fn rumble_label(value: Option<&str>) -> &'static str {
    RUMBLE_VALUES.iter().find(|(v, _)| Some(*v) == value).map_or("Medium", |(_, l)| *l)
}
/// The rumble value `delta` steps from `value`, round.
pub fn rumble_step(value: Option<&str>, delta: i32) -> &'static str {
    let at = RUMBLE_VALUES.iter().position(|(v, _)| Some(*v) == value).unwrap_or(2) as i32;
    RUMBLE_VALUES[(at + delta).rem_euclid(4) as usize].0
}

/// Over the game's setting key (Linux's `over_game::KEY`).
pub const OVER_GAME_KEY: &str = crate::option_table::OVER_GAME_KEY;

/// The Picture page of the handhelds' menu in Android's player, as Linux's (`option_table`'s Picture group): the rows Linux
/// shows for `system` and the values in use (`values`: Linux's keys) that Android's player draws (`applies_in`, Android's
/// profile: Screen scaling, Look, LCD colours under the LCD grid on the GBA, Screen frame and its finish or TV cabinet),
/// and Resolution last where this handheld has it for the console (`range`: `render_scale::here`). Sharpness and Effect stay
/// out: Custom is not a Look there.
pub fn picture_settings(system: &str, range: Option<crate::render_scale::Range>, values: &BTreeMap<String, String>) -> Vec<&'static crate::option_table::Setting> {
    use crate::option_table::GROUPS;
    let Some(group) = GROUPS.iter().find(|g| g.name == "Picture") else { return Vec::new() };
    group
        .settings
        .iter()
        .filter(|s| picture_row_shown(s.key, system, range))
        .filter(|s| s.shown_when.is_none_or(|(key, value)| setting_value_of(key, system, range, values) == value))
        .collect()
}

/// The most rows the Picture page can show for `system` over any values (Linux's `Group::max_rows`): the rows every game
/// has and the largest set that shows under one value (a frame's finish, the TV's cabinet, LCD colours). The page is laid
/// out for it, so the preview keeps its place and size while a row comes and goes.
pub fn picture_max_rows(system: &str, range: Option<crate::render_scale::Range>) -> usize {
    use crate::option_table::GROUPS;
    let Some(group) = GROUPS.iter().find(|g| g.name == "Picture") else { return 0 };
    let shown: Vec<_> = group.settings.iter().filter(|s| picture_row_shown(s.key, system, range)).collect();
    let mut under: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for s in shown.iter().filter_map(|s| s.shown_when) {
        *under.entry(s).or_default() += 1;
    }
    // Rows under different rows show together (LCD colours under the Look, a finish under the frame): the largest set
    // under each row, added up.
    let mut largest: BTreeMap<&str, usize> = BTreeMap::new();
    for ((parent, _), n) in under {
        let m = largest.entry(parent).or_default();
        *m = (*m).max(n);
    }
    shown.iter().filter(|s| s.shown_when.is_none()).count() + largest.values().sum::<usize>()
}

/// The value a row of the Picture page shows for `values` (Linux's `GameSettings::index`, Android's reading): the saved
/// value as the player draws it, else the console's starting value, else the row's default. Resolution: the factor chosen
/// where this handheld offers it, else its default (what the start gives the core, `render_scale::for_start`).
pub fn setting_value_of(key: &str, system: &str, range: Option<crate::render_scale::Range>, values: &BTreeMap<String, String>) -> &'static str {
    use crate::option_table::{android_look, android_reading, console_default, current_value, setting, LOOK};
    let Some(s) = setting(key) else { return "" };
    if key == crate::render_scale::KEY {
        let chosen = values.get(key).and_then(|v| v.parse::<u8>().ok());
        return crate::render_scale::VALUES[resolution_index(range, chosen)];
    }
    if key == crate::mode7_hd::KEY {
        // What the start uses: the choice where offered, else the nearest (`mode7_hd::mode`).
        return crate::mode7_hd::mode(crate::mode7_hd::here_for(system), values.get(key).map(String::as_str));
    }
    if key == LOOK {
        let get = |k: &str| values.get(k).map(String::as_str).or_else(|| console_default(system, k)).unwrap_or_else(|| setting(k).map_or("", |s| s.choices[s.default].value));
        let look = android_look(get("sharpness"), get("effect"));
        return s.choices.iter().find(|c| c.value == look.value()).map_or(s.choices[s.default].value, |c| c.value);
    }
    let find = |v: &str| s.choices.iter().find(|c| c.value == android_reading(key, current_value(key, v))).map(|c| c.value);
    values.get(key).and_then(|v| find(v)).or_else(|| console_default(system, key).and_then(find)).unwrap_or(s.choices[s.default].value)
}

/// The values a row offers on Android for `system`, in Linux's order (`offered_in`; Resolution: what this handheld runs).
pub fn setting_choices(key: &str, system: &str, range: Option<crate::render_scale::Range>) -> Vec<&'static crate::option_table::Choice> {
    use crate::option_table::{offered_in, setting, LOOK};
    let Some(s) = setting(key) else { return Vec::new() };
    if key == crate::render_scale::KEY {
        return s.choices.iter().filter(|c| crate::render_scale::offers(range, c.value)).collect();
    }
    if key == crate::mode7_hd::KEY {
        return s.choices.iter().filter(|c| crate::mode7_hd::offers(crate::mode7_hd::here_for(system), c.value)).collect();
    }
    s.choices.iter().filter(|c| offered_in(s, c.value, system, true) && !(key == LOOK && c.value == "custom")).collect()
}

/// Left/Right on a row: the next offered value, round, as Linux's keys and values to save (a Look writes its sharpness and
/// effect, `look::Look::canonical`).
pub fn setting_step(key: &str, system: &str, range: Option<crate::render_scale::Range>, values: &BTreeMap<String, String>, delta: i32) -> Vec<(&'static str, String)> {
    let choices = setting_choices(key, system, range);
    let Some(s) = crate::option_table::setting(key).filter(|_| !choices.is_empty()) else { return Vec::new() };
    let now = setting_value_of(key, system, range, values);
    let at = choices.iter().position(|c| c.value == now).unwrap_or(0) as i32;
    let next = choices[(at + delta).rem_euclid(choices.len() as i32) as usize].value;
    if key == crate::option_table::LOOK {
        if let Some((sharpness, effect)) = crate::look::Look::from_value(next).and_then(|l| l.canonical()) {
            return vec![("sharpness", sharpness.to_string()), ("effect", effect.to_string())];
        }
    }
    vec![(s.key, next.to_string())]
}

/// A row's line for its value: Linux's (the Look's explanation, a value's own line, else the row's; Screen scaling's
/// without Integer smaller, which the player has not).
pub fn setting_desc(key: &str, system: &str, range: Option<crate::render_scale::Range>, values: &BTreeMap<String, String>) -> &'static str {
    use crate::option_table::{android_desc, setting, value_descs, LOOK};
    let Some(s) = setting(key) else { return "" };
    if let Some(text) = android_desc(key) {
        return text;
    }
    let value = setting_value_of(key, system, range, values);
    if key == LOOK {
        let gba = values.get("lcd_colour").map(String::as_str) == Some("on") && crate::option_table::applies_in("lcd_colour", system, true);
        return crate::look::Look::from_value(value).map_or(s.desc, |l| crate::look::explain(l, gba));
    }
    value_descs(key).iter().find(|(v, _)| *v == value).map_or(s.desc, |(_, d)| *d)
}

/// Every line a row of the Picture page can show for `system` (each value's), for the room the page leaves for them.
pub fn picture_descs(system: &str, range: Option<crate::render_scale::Range>) -> Vec<&'static str> {
    use crate::option_table::{GROUPS, LOOK};
    let Some(group) = GROUPS.iter().find(|g| g.name == "Picture") else { return Vec::new() };
    let mut out = Vec::new();
    for s in group.settings.iter().filter(|s| picture_row_shown(s.key, system, range)) {
        for c in setting_choices(s.key, system, range) {
            let mut values = BTreeMap::new();
            if s.key == LOOK {
                if let Some((sh, ef)) = crate::look::Look::from_value(c.value).and_then(|l| l.canonical()) {
                    values.insert("sharpness".to_string(), sh.to_string());
                    values.insert("effect".to_string(), ef.to_string());
                }
            } else {
                values.insert(s.key.to_string(), c.value.to_string());
            }
            let d = setting_desc(s.key, system, range, &values);
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options;

    /// The page's rows stand in Linux's order and carry Linux's names; the player's extras (Screenshot) come after them.
    #[test]
    fn the_page_is_linux_s_order_with_linux_s_names() {
        let all = top(Has { emulator: true, controls: true, cheats: true });
        let names: Vec<&str> = all.iter().map(|e| e.label()).collect();
        assert_eq!(names, ["Picture", "Emulator", "Controls", "Cheats", "Speed & rumble", "Zoom", "Keep for", "Restore defaults", "Screenshot"]);
        assert_eq!(Entry::Picture.label(), options::GROUPS[0].name);
        // Speed & rumble is Linux's group as a handheld without a fan names it, its rows the ones the player has in Linux's order.
        let extras = options::GROUPS.iter().find(|g| crate::i18n::fanless_text(g.name) == Entry::Extras.label()).unwrap();
        assert!(!Entry::Extras.desc(false).contains("fan") && !Entry::Extras.desc(false).contains("performance"), "Android sets neither");
        let order: Vec<&str> = extras.settings.iter().map(|s| s.key).filter(|k| EXTRAS_ROWS.iter().any(|e| e.setting_key() == Some(*k))).collect();
        assert_eq!(order, EXTRAS_ROWS.iter().map(|e| e.setting_key().unwrap()).collect::<Vec<_>>());
        assert_eq!(Entry::Extras.heading(), "Options \u{203a} Speed & rumble");
        assert_eq!((Entry::Emulator.heading(), Entry::Controls.heading(), Entry::Cheats.heading()), ("Emulator".into(), "Controls".into(), "Cheats".into()));
        assert_eq!(Entry::OverGame.label(), options::setting(crate::over_game::KEY).unwrap().name);
        // Zoom is never first and never the row the page opens on.
        for has in [Has::default(), Has { emulator: true, ..Has::default() }, Has { emulator: true, controls: true, cheats: false }] {
            let rows = top(has);
            assert_eq!(rows[0], Entry::Picture);
            assert!(rows.iter().position(|e| *e == Entry::Zoom).unwrap() > 0);
        }
        assert_eq!(Entry::Picture.heading(), "Options \u{203a} Picture");
        assert!(Entry::Picture.opens() && !Entry::OverGame.opens() && !Entry::Keep.opens() && !Entry::Screenshot.opens());
    }

    /// The Picture page is Linux's rows for the console and the values in use: the frame's finish and the TV's cabinet under
    /// their frame, LCD colours under the LCD grid on the GBA, the Looks the console's kind has; every value a row offers is
    /// reached with Left/Right and shows as itself; a Look writes its Linux pair.
    #[test]
    fn the_picture_page_is_linux_s_rows_for_the_console_and_its_values() {
        let keys = |system: &str, pairs: &[(&str, &str)]| {
            let values: BTreeMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            picture_settings(system, None, &values).iter().map(|s| s.key).collect::<Vec<_>>()
        };
        assert_eq!(keys("snes", &[]), ["scaling", "look", "frame"]);
        assert_eq!(keys("snes", &[("frame", "crt")]), ["scaling", "look", "frame", "frame_tv"]);
        assert_eq!(keys("gb", &[("frame", "pocket")]), ["scaling", "look", "frame", "frame_pocket"]);
        assert_eq!(keys("gba", &[("frame", "clamshell"), ("effect", "grid")]), ["scaling", "look", "lcd_colour", "frame", "frame_clamshell"]);
        assert_eq!(keys("snes", &[("effect", "grid")]), ["scaling", "look", "frame"], "no LCD grid on a TV console");
        let looks = |system: &str| setting_choices(crate::option_table::LOOK, system, None).iter().map(|c| c.value).collect::<Vec<_>>();
        assert_eq!(looks("gba"), ["clean", "smooth", "soft", "bloom", "lcd"]);
        assert_eq!(looks("snes"), ["clean", "smooth", "soft", "bloom", "scanlines", "oldtv"]);
        for system in ["gba", "gb", "snes", "psx", "nds"] {
            for key in ["scaling", crate::option_table::LOOK, "frame"] {
                let mut values = BTreeMap::new();
                let choices = setting_choices(key, system, None);
                assert!(!choices.is_empty(), "{system} {key}");
                let start = setting_value_of(key, system, None, &values);
                let mut seen = vec![start];
                for _ in 0..choices.len() {
                    for (k, v) in setting_step(key, system, None, &values, 1) {
                        values.insert(k.to_string(), v);
                    }
                    seen.push(setting_value_of(key, system, None, &values));
                }
                assert_eq!(seen.first(), seen.last(), "{system} {key}: round");
                let mut want: Vec<&str> = choices.iter().map(|c| c.value).collect();
                let mut got = seen[..choices.len()].to_vec();
                want.sort();
                got.sort();
                assert_eq!(got, want, "{system} {key}: every offered value once");
            }
        }
        let mut values = BTreeMap::new();
        for (k, v) in setting_step(crate::option_table::LOOK, "snes", None, &values, 5) {
            values.insert(k.to_string(), v);
        }
        assert_eq!((values["sharpness"].as_str(), values["effect"].as_str()), ("crisp", "crt"), "Old TV writes Linux's pair");
        assert_eq!(setting_desc(crate::option_table::LOOK, "snes", None, &values), crate::look::explain(crate::look::Look::OldTv, false));
    }

    /// Rumble: Linux's row (name, values, labels, line, default Medium), under Speed, rumble, fan in Linux's order.
    #[test]
    fn rumble_reads_as_linux_s() {
        let linux = options::setting(RUMBLE_KEY).unwrap();
        assert_eq!((Entry::Rumble.label(), Entry::Rumble.desc(false)), (linux.name, linux.desc));
        assert_eq!(RUMBLE_VALUES.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect::<Vec<_>>(), linux.choices.iter().map(|c| (c.value.to_string(), c.label.to_string())).collect::<Vec<_>>());
        assert_eq!(rumble_label(None), linux.choices[usize::from(linux.default)].label);
        assert_eq!((rumble_step(None, 1), rumble_step(Some("high"), 1), rumble_step(Some("off"), -1)), ("high", "off", "high"));
        for (v, _) in RUMBLE_VALUES {
            assert_eq!(crate::rumble::Strength::parse(v), crate::rumble::Strength::parse(linux.choices.iter().find(|c| c.value == v).unwrap().value));
        }
    }

    #[test]
    fn over_the_game_and_keep_for_read_as_linux_s() {
        let over = options::setting(crate::over_game::KEY).unwrap();
        for (on, value) in [(false, "off"), (true, "fps")] {
            assert_eq!(over_game_label(on), over.choices.iter().find(|c| c.value == value).unwrap().label);
        }
        assert_eq!(OVER_GAME_VALUES, ["off", "fps"]);
        assert_eq!(OVER_GAME_DESC, over.desc.lines().next().unwrap());
        assert_eq!((keep_label(true), keep_label(false)), ("This game", "This console"));
        assert_eq!(Entry::OverGame.setting_key(), Some(crate::over_game::KEY));
    }

    /// Fast-forward speed and Rewind: Linux's rows (name, values, labels, line, starting value), in Linux's order under
    /// Speed, rumble, fan.
    #[test]
    fn fast_forward_and_rewind_read_as_linux_s() {
        let ff = options::setting(crate::play_speed::FF_KEY).unwrap();
        assert_eq!(Entry::FastForward.label(), ff.name);
        assert_eq!(Entry::FastForward.desc(false), ff.desc);
        assert_eq!(FF_VALUES.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect::<Vec<_>>(), ff.choices.iter().map(|c| (c.value.to_string(), c.label.to_string())).collect::<Vec<_>>());
        let start = crate::play_speed::FastForward::default();
        assert_eq!(start.value(), ff.choices[usize::from(ff.default)].value, "the same starting speed");
        assert_eq!(ff_label(start), "4x");
        assert_eq!(ff_step(start, 1).value(), "8");
        assert_eq!(ff_step(crate::play_speed::FastForward::Max, 1).value(), "2", "round");
        assert_eq!(ff_step(crate::play_speed::FastForward::Times(2), -1), crate::play_speed::FastForward::Max);
        let rewind = options::setting(crate::play_speed::REWIND_KEY).unwrap();
        assert_eq!(Entry::Rewind.label(), rewind.name);
        assert_eq!(Entry::Rewind.desc(false), rewind.desc);
        assert_eq!(rewind.choices[usize::from(rewind.default)].value, "off", "Rewind starts off, as the player's");
        for (on, value) in [(false, "off"), (true, "on")] {
            assert_eq!(earlier_label(on), rewind.choices.iter().find(|c| c.value == value).unwrap().label);
        }
    }

    #[test]
    fn earlier_states_and_restore_defaults_read_as_linux_s() {
        let earlier = options::setting(crate::timeline::KEY).unwrap();
        assert_eq!(Entry::Earlier.label(), earlier.name);
        assert_eq!(Entry::Earlier.desc(false), earlier.desc);
        for (on, value) in [(false, "off"), (true, "on")] {
            assert_eq!(earlier_label(on), earlier.choices.iter().find(|c| c.value == value).unwrap().label);
        }
        // The starting value is Linux's (On, Off for the consoles with big states), from the one place both read.
        for sys in ["gba", "snes", "psx", "n64", "psp", "nds", "dreamcast"] {
            let linux = options::console_default(sys, crate::timeline::KEY).map_or(earlier.choices[usize::from(earlier.default)].value, |v| v);
            assert_eq!(crate::timeline_policy::default_on(sys), linux == "on", "{sys}");
        }
        assert!(!Entry::Restore.opens() && !Entry::Earlier.opens());
        assert_eq!(Entry::Restore.desc(false), RESTORE_DESC);
    }

    /// Resolution on the Picture page: Linux's row (name, values, labels, line), only where this handheld has it for the
    /// console, last, stepping through what the handheld offers, starting on its default.
    #[test]
    fn resolution_reads_and_steps_as_linux_s_row() {
        use crate::render_scale::{range, Class, Range};
        let key = crate::render_scale::KEY;
        let none = BTreeMap::new();
        assert!(picture_settings("n64", None, &none).iter().all(|s| s.key != key), "no row where the handheld wasn't measured");
        assert_eq!(picture_settings("n64", range(Class::Sm8250, "n64"), &none).last().map(|s| s.key), Some(key), "last, as on Linux");
        let r = Some(Range { default: 2, max: 3 });
        assert_eq!(resolution_index(r, None), 1, "nothing chosen: the default, 2x");
        assert_eq!(resolution_index(r, Some(1)), 0);
        assert_eq!(resolution_index(r, Some(4)), 1, "more than offered here: the default, as the start does");
        assert_eq!(setting_value_of(key, "n64", r, &none), "2");
        let mut values = BTreeMap::new();
        let mut seen = Vec::new();
        for _ in 0..3 {
            for (k, v) in setting_step(key, "n64", r, &values, 1) {
                values.insert(k.to_string(), v);
            }
            seen.push(setting_value_of(key, "n64", r, &values));
        }
        assert_eq!(seen, ["3", "1", "2"], "Right from 2x: 3x, then round to Original; never 4x where 3x is the highest");
        let dc = range(Class::Sm8250, "dreamcast");
        assert_eq!(setting_choices(key, "dreamcast", dc).len(), 2, "Dreamcast: Original and 2x");
    }

    /// Mode 7 on Android's Picture page (`mode7_hd`): the row for the Super Nintendo on a handheld that offers it, Linux's
    /// name, values and labels, the app's own line (no emulator switch), Original and HD only (the app's bsnes has no
    /// widescreen); Left/Right step between them; nothing on other consoles or on a handheld without it.
    #[test]
    fn mode7_is_on_the_picture_page_for_the_snes_where_offered() {
        use crate::mode7_hd::{with_offer, Offer, KEY};
        let values = BTreeMap::new();
        let android = Some(Offer { scale: 2, wide: false });
        with_offer(android, || {
            let keys: Vec<_> = picture_settings("snes", None, &values).iter().map(|s| s.key).collect();
            assert_eq!(keys.last(), Some(&KEY), "{keys:?}");
            assert!(!picture_settings("gba", None, &values).iter().any(|s| s.key == KEY), "only the Super Nintendo");
            assert_eq!(setting_value_of(KEY, "snes", None, &values), "original", "nothing chosen: the console's own");
            let choices: Vec<_> = setting_choices(KEY, "snes", None).iter().map(|c| (c.value, c.label)).collect();
            assert_eq!(choices, [("original", "Original"), ("hd", "HD")], "no widescreen in the app");
            assert_eq!(setting_step(KEY, "snes", None, &values, 1), vec![(KEY, "hd".to_string())]);
            let hd: BTreeMap<String, String> = [(KEY.to_string(), "hd".to_string())].into();
            assert_eq!(setting_step(KEY, "snes", None, &hd, 1), vec![(KEY, "original".to_string())], "round");
            let wide: BTreeMap<String, String> = [(KEY.to_string(), "wide".to_string())].into();
            assert_eq!(setting_value_of(KEY, "snes", None, &wide), "hd", "widescreen chosen on a card: HD here");
            assert_eq!(setting_desc(KEY, "snes", None, &values), crate::mode7_hd::ANDROID_DESC);
            assert!(!crate::mode7_hd::ANDROID_DESC.contains("bsnes-hd") && crate::mode7_hd::DESC.contains("bsnes-hd"));
        });
        with_offer(None, || {
            assert!(!picture_settings("snes", None, &values).iter().any(|s| s.key == KEY), "a handheld without it: no row");
        });
        let card = Some(Offer { scale: 2, wide: true });
        with_offer(card, || {
            let s = crate::option_table::setting(KEY).unwrap();
            let offered: Vec<_> = s.choices.iter().filter(|c| crate::option_table::offered_in(s, c.value, "snes", false)).map(|c| c.value).collect();
            assert_eq!(offered, ["original", "hd", "wide"], "a wide card: all three");
            assert!(crate::option_table::applies_in(KEY, "snes", false) && !crate::option_table::applies_in(KEY, "n64", false));
        });
    }

    /// Linux's rows the player has no counterpart for, each with why. Every other row of Linux's Options page on the same
    /// handheld is on Android's page too (`android_s_page_has_linux_s_rows_on_the_same_handheld`).
    const NOT_ON_ANDROID: &[(&str, &str)] = &[
        // Picture: every Look but Custom, every frame but Ambient (pocket-core draws it live from the picture).
        ("sharpness", "feature missing on Android: the player has no Custom look"),
        ("effect", "feature missing on Android: the player has no Custom look"),
        ("orientation", "feature missing on Android: the player doesn't turn the picture and the D-pad"),
        // Shortcuts: HOME + a button. Android owns Home; the player's pause menu opens on Back / Mode.
        ("hk_save", "Linux only: no HOME layer, Android owns Home"),
        ("hk_load", "Linux only: no HOME layer, Android owns Home"),
        ("hk_slot", "Linux only: no HOME layer, Android owns Home"),
        ("hk_ff", "Linux only: the player's HOME + R2 is always on (no Shortcuts page, Android owns the rest of Home)"),
        ("hk_rewind", "Linux only: the player's HOME + L2 is always on (no Shortcuts page, Android owns the rest of Home)"),
        ("hk_shot", "Linux only: no HOME layer, Android owns Home"),
        ("hk_over", "Linux only: no HOME layer, Android owns Home"),
        ("hk_zoom", "Linux only: no HOME layer, Android owns Home"),
        // Speed, rumble, fan.
        ("run_ahead", "feature missing on Android: the player's loop has no run-ahead"),
        ("performance", "Linux only: Android owns the CPU's governor"),
        ("fan", "Linux only: Android owns the fan"),
        // Linux's page outside the groups.
        ("Runs on", "Linux only: on Android the emulator is chosen before the start (Plays with)"),
    ];

    /// Linux's Options page on a handheld of `class` for `system`: the groups' rows it lists there (`options::applies_in`,
    /// Resolution by the class's numbers, the fan on these Snapdragon handhelds), then the rows outside the groups
    /// (`main.rs` `opt_entries`: Emulator, Controls, Cheats, Zoom, Runs on, Keep for, Restore defaults).
    fn linux_rows(class: Option<crate::render_scale::Class>, system: &str) -> std::collections::BTreeSet<&'static str> {
        let mut rows = std::collections::BTreeSet::new();
        for setting in options::GROUPS.iter().flat_map(|g| g.settings.iter()) {
            let shown = match setting.key {
                crate::render_scale::KEY => class.and_then(|c| crate::render_scale::range(c, system)).is_some(),
                "fan" => class.is_some(),
                crate::mode7_hd::KEY => crate::mode7_hd::is_snes(system) && class.is_some(),
                key => options::applies_in(key, system, false),
            };
            if shown {
                rows.insert(setting.key);
            }
        }
        rows.extend(["Emulator", "Controls", "Zoom", "Cheats", "Runs on", "Keep for", "Restore defaults"]);
        rows
    }

    /// Android's page for the same game, built as the player builds it (`top`, `picture_rows`), as setting keys where
    /// Linux has the row as a setting.
    fn android_rows(class: Option<crate::render_scale::Class>, system: &str) -> std::collections::BTreeSet<&'static str> {
        let mut rows = std::collections::BTreeSet::new();
        for entry in top(Has { emulator: true, controls: true, cheats: true }) {
            match entry {
                Entry::Picture => {
                    // Every row the page can show for the console, whatever the values (a finish under its frame, LCD colours under the grid).
                    let range = class.and_then(|c| crate::render_scale::range(c, system));
                    let group = options::GROUPS.iter().find(|g| g.name == "Picture").unwrap();
                    rows.extend(group.settings.iter().map(|s| s.key).filter(|k| match *k {
                        crate::render_scale::KEY => range.is_some(),
                        crate::mode7_hd::KEY => crate::mode7_hd::is_snes(system) && class.is_some(),
                        k => options::applies_in(k, system, true),
                    }));
                }
                Entry::Screenshot => (), // the player's own (Linux: HOME + A)
                Entry::Extras => rows.extend(EXTRAS_ROWS.iter().filter_map(|e| e.setting_key())),
                e => {
                    rows.insert(e.setting_key().unwrap_or(e.label()));
                }
            }
        }
        rows
    }

    /// The class of rows, not one row: on an Android handheld of a chip class Linux has numbers for, the player's Options
    /// page has every row Linux's has on that chip, Resolution included, except `NOT_ON_ANDROID`; and nothing Linux lacks
    /// but Screenshot. Until 17.11.2026 Resolution was missing because Android never found the chip (no device tree in
    /// the app's sandbox), and Earlier states and Restore defaults for no reason.
    #[test]
    fn android_s_page_has_linux_s_rows_on_the_same_handheld() {
        use crate::render_scale::Class;
        let out: std::collections::BTreeSet<&str> = NOT_ON_ANDROID.iter().map(|(k, _)| *k).collect();
        for class in [Some(Class::Sm8250), Some(Class::Sm8550), None] {
            for sys in ["n64", "psp", "dreamcast", "psx", "gba", "gbc", "snes", "nes", "megadrive", "nds", "saturn"] {
                let linux = linux_rows(class, sys);
                let android = android_rows(class, sys);
                let missing: Vec<_> = linux.difference(&android).filter(|k| !out.contains(*k)).collect();
                assert!(missing.is_empty(), "{class:?} {sys}: Linux has {missing:?}, Android's page doesn't");
                let extra: Vec<_> = android.difference(&linux).collect();
                assert!(extra.is_empty(), "{class:?} {sys}: Android has {extra:?}, Linux's page doesn't");
                for key in &out {
                    assert!(!android.contains(key), "{key}: listed as not on Android, but the page has it");
                }
            }
        }
        // Resolution for the four 3D systems on both Snapdragon classes.
        for sys in ["n64", "psp", "dreamcast", "psx"] {
            assert!(android_rows(Some(Class::Sm8250), sys).contains(crate::render_scale::KEY), "{sys}");
        }
        // Every exception is a real Linux row of this handheld, with a reason.
        let all_linux: std::collections::BTreeSet<&str> = ["n64", "gba", "snes"].iter().flat_map(|s| linux_rows(Some(Class::Sm8250), s)).collect();
        for (key, why) in NOT_ON_ANDROID {
            assert!(all_linux.contains(key) || options::setting(key).is_some(), "{key}: not a Linux row");
            assert!(why.starts_with("feature missing on Android") || why.starts_with("Linux only"), "{key}: {why}");
        }
    }

    /// Every line under a row of the Options family of pages reads whole in Android's player, never cut with "…" (the
    /// Mini V2 showed "Die Auflösung, in der das 3D-Bild gezeichnet wird. Original ist …", 17.11.): each row's line (both
    /// Keep for lines, every value's line of the Picture rows, Resolution, Speed, rumble, fan's rows, Hold to toggle,
    /// Restore's two answers and a long emulator option), in every language and text size, on the handhelds' screens as
    /// the player lays the menu out (Linux's size for the screen, `scaled_layout::logical_size`), painted by the painter
    /// both menus use (`pause_page::paint_options`): all its words, the selected row above it.
    #[test]
    fn every_row_s_line_reads_whole_on_every_screen_language_and_text_size() {
        use crate::pause_page::{paint_options, OptRow, OptionsPage};
        use crate::text_size::TextSize;
        let mut descs: Vec<(String, &str)> = Vec::new(); // (line, the row's value)
        for e in top(Has { emulator: true, controls: true, cheats: true }).into_iter().chain(EXTRAS_ROWS) {
            for keep in [true, false] {
                descs.push((e.desc(keep).to_string(), if e.opens() { ">" } else if matches!(e, Entry::FastForward | Entry::Rewind | Entry::Earlier | Entry::OverGame | Entry::Rumble | Entry::Keep) { "On" } else { "" }));
            }
        }
        // On a handheld with Mode 7 HD (`mode7_hd`), so its line is among them.
        let mode7 = Some(crate::mode7_hd::Offer { scale: 2, wide: false });
        for system in ["gba", "gb", "snes", "n64"] {
            for d in crate::mode7_hd::with_offer(mode7, || picture_descs(system, crate::render_scale::range(crate::render_scale::Class::Sm8250, system))) {
                if !descs.iter().any(|(x, _)| x == d) {
                    descs.push((d.to_string(), "On"));
                }
            }
        }
        descs.push((HOLD_DESC.to_string(), "Off"));
        descs.push((RESTORE_ASK.to_string(), ""));
        descs.push((RESTORE_DONE.to_string(), ""));
        // An emulator option's own line, as long as the cores' longest ones the player shows, with the restart note.
        descs.push((format!("Renders the 3D picture at a multiple of the console's own resolution. Higher values are sharper but need a faster graphics chip; some games show seams or lines at the edges of their 2D pictures.\n{}", crate::core_options::RESTART_NOTE), "On"));
        // The handhelds' screens Android's player runs on, laid out as Linux lays them out (all landscape: the handhelds'
        // menu is a handheld's; a portrait 1080x1240 cut a Portuguese line in `kit::footer_lines`' narrow column, Linux's
        // too, noted 17.11.2026).
        let screens = [(1240, 1080), (1920, 1080), (1280, 960), (1280, 720)];
        let corner = crate::status_corner::Corner::battery_only("80%");
        let names = ["Picture", "Emulator", "Controls", "Cheats", "Speed, rumble, fan", "Zoom", "Keep for", "Restore defaults", "Screenshot"];
        let mut text = crate::text::Text::builtin();
        let mut faults = Vec::new();
        for lang in crate::i18n::Lang::ALL {
            crate::i18n::set(lang);
            for size in [TextSize::Normal, TextSize::Medium, TextSize::Large] {
                crate::type_scale::set(size);
                for (pw, ph) in screens {
                    let (w, h) = crate::scaled_layout::logical_size((pw, ph));
                    let _wide = crate::type_scale::panel(crate::type_scale::wide_page(w as i32, h as i32));
                    let mut px = vec![0u32; (w * h) as usize];
                    let mut c = crate::gfx::Canvas::new(&mut px, w, h);
                    for (desc, value) in &descs {
                        for sel in [0, names.len() - 1] {
                            let rows: Vec<OptRow> = names.iter().enumerate().map(|(i, n)| OptRow::new(*n, if i == sel { *value } else { "" }, if i == sel { desc.as_str() } else { "" })).collect();
                            let mut hits = crate::touch::HitMap::default();
                            let page = OptionsPage { heading: Some("Options"), counts: None, rows: &rows, sel, footer: None, explanation: false, accent: 0x808080, corner: &corner, picture: None };
                            let out = paint_options(&mut c, &mut text, &mut hits, &page);
                            let at = format!("{lang:?} {size:?} {pw}x{ph} ({w}x{h}) sel {sel}: {:?}", desc.chars().take(40).collect::<String>());
                            let painted: Vec<&str> = out.desc.iter().flat_map(|l| l.split_whitespace()).collect();
                            // Each paragraph (a line of the text) is translated on its own, as `kit::footer_lines` does.
                            let whole: Vec<String> = desc.lines().filter(|p| !p.trim().is_empty()).map(|p| text.translated(p).to_string()).collect();
                            let want: Vec<&str> = whole.iter().flat_map(|p| p.split_whitespace()).collect();
                            if painted != want || out.desc.iter().any(|l| l.contains('\u{2026}')) {
                                faults.push(format!("{at}: cut: {:?}", out.desc));
                            }
                            let step = crate::type_scale::line(crate::kit::SMALL, 10);
                            let desc_top = crate::kit::footer_baseline(h as i32) - out.desc.len() as i32 * step;
                            match out.rows[sel] {
                                Some(a) if a.y + a.h <= desc_top + step / 2 => {}
                                r => faults.push(format!("{at}: the selected row {r:?} isn't above the line (top {desc_top})")),
                            }
                        }
                    }
                }
            }
        }
        crate::type_scale::set(TextSize::Normal);
        crate::i18n::set(crate::i18n::Lang::En);
        assert!(faults.is_empty(), "{} of the Options pages' lines don't read whole:\n{}", faults.len(), faults.iter().take(25).cloned().collect::<Vec<_>>().join("\n"));
    }

    /// The Picture page's rows on the pad (nav explorer for Android's player, 18.11.2026): every row Linux shows for the
    /// console and its values is reached with Down from the first and Up from the first (round), Left and Right change it
    /// and come back, and on the handhelds' screens in every language at Normal and Large text the page laid out for its
    /// most rows (`picture_max_rows`) shows all of them, each name and value whole, beside the preview.
    #[test]
    fn every_picture_row_is_reached_and_read_whole_in_every_language() {
        use crate::pause_page::{paint_options, picture_page, OptRow, OptionsPage};
        use crate::text::Face;
        use crate::text_size::TextSize;
        let range = crate::render_scale::range(crate::render_scale::Class::Sm8250, "n64");
        // (console, values that open its sub-rows, resolution range): the frame's finishes, the TV's cabinet, LCD colours.
        let cases: Vec<(&str, Vec<(&str, &str)>, Option<crate::render_scale::Range>)> = vec![
            ("gba", vec![("frame", "clamshell"), ("effect", "grid"), ("lcd_colour", "on")], None),
            ("gba", vec![("frame", "wide")], None),
            ("gb", vec![("frame", "pocket")], None),
            ("gb", vec![("frame", "brick")], None),
            ("gbc", vec![("frame", "shell")], None),
            ("snes", vec![("frame", "crt"), ("frame_tv", "wood")], None),
            ("n64", vec![("frame", "crt")], range),
            ("psx", vec![], None),
        ];
        let mut faults = Vec::new();
        let mut text = crate::text::Text::builtin();
        let corner = crate::status_corner::Corner::battery_only("80%");
        let live = crate::preview::Live::default();
        for (system, pairs, range) in &cases {
            let values: BTreeMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            let rows = picture_settings(system, *range, &values);
            let keys: Vec<&str> = rows.iter().map(|s| s.key).collect();
            for (k, _) in pairs.iter().filter(|(k, _)| !["effect", "sharpness"].contains(k)) {
                assert!(keys.contains(k) || *k == "frame", "{system}: {k} is not on the page: {keys:?}");
            }
            assert!(rows.len() <= picture_max_rows(system, *range), "{system}: more rows than the page is laid out for");
            // The pad: Down from the first goes through every row and round; Up from the first is the last.
            let mut nav = OptNav::default();
            nav.open(Entry::Picture);
            let mut seen = Vec::new();
            for _ in 0..rows.len() {
                seen.push(nav.sel);
                nav.step(1, rows.len());
            }
            assert_eq!((seen, nav.sel), ((0..rows.len()).collect::<Vec<_>>(), 0), "{system}: Down");
            nav.step(-1, rows.len());
            assert_eq!(nav.sel, rows.len() - 1, "{system}: Up from the first");
            // Left/Right on every row: another value, and back.
            for s in &rows {
                if setting_choices(s.key, system, *range).len() < 2 {
                    continue;
                }
                let before = setting_value_of(s.key, system, *range, &values);
                let mut v = values.clone();
                for (k, x) in setting_step(s.key, system, *range, &v, 1) {
                    v.insert(k.to_string(), x);
                }
                assert_ne!(setting_value_of(s.key, system, *range, &v), before, "{system} {}: Right", s.key);
                for (k, x) in setting_step(s.key, system, *range, &v, -1) {
                    v.insert(k.to_string(), x);
                }
                assert_eq!(setting_value_of(s.key, system, *range, &v), before, "{system} {}: Left back", s.key);
            }
            let descs = picture_descs(system, *range);
            for lang in crate::i18n::Lang::ALL {
                crate::i18n::set(lang);
                for size in [TextSize::Normal, TextSize::Large] {
                    crate::type_scale::set(size);
                    for (pw, ph) in [(1240, 1080), (1920, 1080), (1280, 720)] {
                        let (w, h) = crate::scaled_layout::logical_size((pw, ph));
                        let _wide = crate::type_scale::panel(crate::type_scale::wide_page(w as i32, h as i32));
                        // The page as the player lays it out: the preview below, the list showing what the room leaves and
                        // scrolling to the selected row (as Linux's).
                        let (shown, layout) = picture_page(&mut text, w as i32, h as i32, picture_max_rows(system, *range), &descs);
                        let at = format!("{lang:?} {size:?} {pw}x{ph} {system} {pairs:?}");
                        if shown == 0 {
                            faults.push(format!("{at}: the page shows no row"));
                        }
                        let opt: Vec<OptRow> = rows
                            .iter()
                            .map(|s| OptRow::new(s.name, crate::option_table::label_in(s, setting_value_of(s.key, system, *range, &values), true), setting_desc(s.key, system, *range, &values)))
                            .collect();
                        let mut px = vec![0u32; (w * h) as usize];
                        let mut c = crate::gfx::Canvas::new(&mut px, w, h);
                        for sel in 0..rows.len() {
                            let mut hits = crate::touch::HitMap::default();
                            let preview = crate::pause_page::PicturePreview { rows: shown, layout, live: &live };
                            let page = OptionsPage { heading: Some("Options \u{203a} Picture"), counts: None, rows: &opt, sel, footer: None, explanation: false, accent: 0x808080, corner: &corner, picture: Some(preview) };
                            let out = paint_options(&mut c, &mut text, &mut hits, &page);
                            if out.rows.get(sel).copied().flatten().is_none() {
                                faults.push(format!("{at}: row {} not on the screen when selected", rows[sel].key));
                            }
                        }
                        // Each row's name and value side by side on the row, translated, with room between.
                        for (s, r) in rows.iter().zip(&opt) {
                            let name = text.translated(s.name).to_string();
                            let value = text.translated(&r.value).to_string();
                            let need = text.width(Face::Body, 36, &name) + text.width(Face::Body, 36, &value) + 3 * PAD_FOR_TEST;
                            if need > w as i32 {
                                faults.push(format!("{at}: {} = {value:?} needs {need} of {w}", s.key));
                            }
                        }
                    }
                }
            }
        }
        crate::type_scale::set(TextSize::Normal);
        crate::i18n::set(crate::i18n::Lang::En);
        assert!(faults.is_empty(), "{} faults:\n{}", faults.len(), faults.iter().take(25).cloned().collect::<Vec<_>>().join("\n"));
    }
    /// The space a row keeps around and between its name and value (`pause_page`'s padding), for the width check above.
    const PAD_FOR_TEST: i32 = 40;

    /// Linux's top Options rows the player has no counterpart for, and the player's own, each with why.
    const TOP_NOT_ON_ANDROID: &[(&str, &str)] = &[
        ("Shortcuts", "Linux only: HOME + a button; Android owns Home, the player's menu opens on Back / Mode"),
        ("Runs on", "Linux only: on Android the emulator is chosen before the start (Plays with)"),
    ];
    /// The Controls page's rows Linux has and the player hasn't.
    const CONTROLS_NOT_ON_ANDROID: &[(&str, &str)] = &[("Controller setup", "Linux only: it opens Linux's Settings → Controls → Controller setup; Android sets its controllers in Android's own settings")];
    const TOP_ANDROID_ONLY: &[(&str, &str)] = &[("Screenshot", "Android only: Linux takes it with HOME + A, which Android hasn't")];

    /// The class of pages, not one page: the Options page of Android's player is Linux's (`linux_entries`, the very list
    /// `main.rs` shows) for every game the player can have (with or without the emulator's options, with or without
    /// cheats): the same rows in the same order, the same rows open a page, the same lines; only `TOP_NOT_ON_ANDROID` and
    /// `TOP_ANDROID_ONLY` differ, each with a reason. Painted by one painter (`pause_page`), so layout, letters, spacing,
    /// hints and the preview follow from these rows.
    #[test]
    fn the_options_page_is_linux_s_for_every_game() {
        let name = |e: LinuxEntry| match e {
            // As a handheld without a fan names it: Android sets none (`i18n::FANLESS_NAMES`).
            LinuxEntry::Group(g) => match crate::i18n::FANLESS_NAMES.iter().find(|(long, _)| *long == options::GROUPS[g].name) { Some((_, short)) => short, None => options::GROUPS[g].name },
            LinuxEntry::Emulator => "Emulator",
            LinuxEntry::Controls => "Controls",
            LinuxEntry::Zoom => "Zoom",
            LinuxEntry::Cheats => "Cheats",
            LinuxEntry::RunsOn => "Runs on",
            LinuxEntry::Keep => "Keep for",
            LinuxEntry::Restore => "Restore defaults",
        };
        let opens = |e: LinuxEntry| !matches!(e, LinuxEntry::Keep | LinuxEntry::Restore);
        let out: Vec<&str> = TOP_NOT_ON_ANDROID.iter().map(|(k, _)| *k).collect();
        let only: Vec<&str> = TOP_ANDROID_ONLY.iter().map(|(k, _)| *k).collect();
        for emulator in [false, true] {
            for cheats in [false, true] {
                let linux: Vec<LinuxEntry> = linux_entries(options::GROUPS.len(), Some(emulator), cheats).into_iter().filter(|e| !out.contains(&name(*e))).collect();
                let android: Vec<Entry> = top(Has { emulator, controls: true, cheats }).into_iter().filter(|e| !only.contains(&e.label())).collect();
                let at = format!("emulator {emulator}, cheats {cheats}");
                assert_eq!(android.iter().map(|e| e.label()).collect::<Vec<_>>(), linux.iter().map(|e| name(*e)).collect::<Vec<_>>(), "{at}");
                for (a, l) in android.iter().zip(&linux) {
                    assert_eq!(a.opens(), opens(*l), "{at}: {} opens a page on one side only", a.label());
                    if let LinuxEntry::Group(g) = l {
                        // A group's line is Linux's, but for Picture: the player has no Screen orientation (`NOT_ON_ANDROID`);
                        // and Speed & rumble's names only the rows the player has (no performance, no fan).
                        if !["Picture", "Speed, rumble, fan"].contains(&options::GROUPS[*g].name) {
                            assert_eq!(a.desc(false), options::GROUPS[*g].desc, "{at}: {}", a.label());
                        }
                    }
                }
                // Each exception is a real row of its side, with a reason of the agreed kinds.
                let all_linux: Vec<&str> = linux_entries(options::GROUPS.len(), Some(emulator), cheats).into_iter().map(name).collect();
                for (key, why) in TOP_NOT_ON_ANDROID {
                    assert!(all_linux.contains(key), "{key}: not a Linux row");
                    assert!(why.starts_with("Linux only") || why.starts_with("feature missing on Android"), "{key}: {why}");
                }
                for (key, why) in TOP_ANDROID_ONLY {
                    assert!(top(Has { emulator, controls: true, cheats }).iter().any(|e| e.label() == *key), "{key}: not an Android row");
                    assert!(why.starts_with("Android only"), "{key}: {why}");
                }
            }
        }
        // The pages' headings are Linux's: a group's "Options › …", the emulator's pages their own name.
        for e in [Entry::Picture, Entry::Extras] {
            let g = options::GROUPS.iter().find(|g| crate::i18n::fanless_text(g.name) == e.label()).unwrap();
            assert_eq!(e.heading(), format!("Options \u{203a} {}", crate::i18n::fanless_text(g.name)));
        }
        // Controls is Linux's page (`controls::ControlsPage`, which `emu_menu` shows): only Controller setup is Linux's.
        let dir0 = crate::testutil::TestDir::new("controls-page");
        let settings = options::GameSettings::load(&dir0, "n64", std::path::Path::new("Mario.z64"));
        let mut linux: Vec<String> = crate::emu_menu::EmuMenu::offline("n64", &settings).control_rows().into_iter().map(|r| r.name).collect();
        linux.push("Controller setup".into()); // `main.rs` adds it under the rows
        let page = crate::controls::ControlsPage { mapping: crate::controls::Mapping::defaults("n64"), targets: Vec::new(), system: "n64".into() };
        let android: Vec<String> = page.rows().into_iter().map(|r| r.0).collect();
        let out: Vec<&str> = CONTROLS_NOT_ON_ANDROID.iter().map(|(k, _)| *k).collect();
        assert_eq!(android, linux.iter().filter(|n| !out.contains(&n.as_str())).cloned().collect::<Vec<_>>());
        for (key, why) in CONTROLS_NOT_ON_ANDROID {
            assert!(linux.iter().any(|n| n == key) && why.starts_with("Linux only"), "{key}: {why}");
        }
        // Controls → Hold to toggle reads as Linux's row.
        let dir = crate::testutil::TestDir::new("hold-row");
        let linux = crate::emu_menu::EmuMenu::offline("gbc", &options::GameSettings::load(&dir, "gbc", std::path::Path::new("uCity.gbc"))).control_rows();
        let hold = linux.iter().find(|r| r.name == "Hold to toggle").unwrap();
        assert_eq!(hold.desc, HOLD_DESC);
    }

    /// The navigation explorer's walk of Options › Controls in Android's player, key by key, in every language and text size
    /// on the Mini V2 as the player lays the menu out: from the top page D-pad Down reaches Controls and A opens it; Down
    /// reaches every row (the selected one painted whole on screen above its line, never cut), Right on a button gives it
    /// the next console button and the page says so, Reset controls puts the layout back, B goes back onto Controls, B
    /// again leaves Options. The page's rows are Linux's (`ControlsPage`), painted by Linux's painter.
    #[test]
    fn controls_are_reached_assigned_reset_and_left_with_the_d_pad_in_every_language() {
        use crate::controls::{ControlsPage, Mapping, MENU_ORDER};
        use crate::pause_page::{paint_options, OptRow, OptionsPage};
        use crate::text_size::TextSize;
        let corner = crate::status_corner::Corner::battery_only("80%");
        let (w, h) = crate::scaled_layout::logical_size((1240, 1080));
        let mut px = vec![0u32; (w * h) as usize];
        let mut text = crate::text::Text::builtin();
        let mut faults = Vec::new();
        let top_rows = top(Has { emulator: true, controls: true, cheats: true });
        let paint = |c: &mut crate::gfx::Canvas, text: &mut crate::text::Text, heading: &str, rows: &[OptRow], sel: usize| {
            let mut hits = crate::touch::HitMap::default();
            let page = OptionsPage { heading: Some(heading), counts: None, rows, sel, footer: None, explanation: false, accent: 0x808080, corner: &corner, picture: None };
            paint_options(c, text, &mut hits, &page)
        };
        for lang in crate::i18n::Lang::ALL {
            crate::i18n::set(lang);
            for size in [TextSize::Normal, TextSize::Large] {
                crate::type_scale::set(size);
                let _wide = crate::type_scale::panel(crate::type_scale::wide_page(w as i32, h as i32));
                let mut c = crate::gfx::Canvas::new(&mut px, w, h);
                let at = |what: &str| format!("{lang:?} {size:?}: {what}");
                let mut nav = OptNav::default();
                // The top page: Down until Controls, A opens it.
                let controls_at = top_rows.iter().position(|e| *e == Entry::Controls).unwrap();
                for _ in 0..controls_at {
                    nav.step(1, top_rows.len());
                }
                assert_eq!(top_rows[nav.sel], Entry::Controls, "{}", at("Down reaches Controls"));
                nav.open(Entry::Controls);
                let mut page = ControlsPage { mapping: Mapping::defaults("snes"), targets: crate::controls::targets_from(&[]), system: "snes".into() };
                let rows = |page: &ControlsPage| page.rows().into_iter().map(|(n, v, d)| OptRow::new(n, v, d)).collect::<Vec<_>>();
                // Every row reached with Down, on screen and whole while selected.
                for k in 0..page.count() {
                    let r = rows(&page);
                    let out = paint(&mut c, &mut text, "Controls", &r, nav.sel);
                    match out.rows[nav.sel] {
                        Some(a) if a.y >= 0 && a.y + a.h <= h as i32 => {}
                        other => faults.push(at(&format!("row {k} ({}) not on screen when selected: {other:?}", r[nav.sel].name))),
                    }
                    if out.desc.iter().any(|l| l.contains('\u{2026}')) {
                        faults.push(at(&format!("row {k}'s line is cut: {:?}", out.desc)));
                    }
                    nav.step(1, page.count());
                }
                assert_eq!(nav.sel, 0, "{}", at("Down goes round"));
                // Right on the first button row: it presses the next console button, and the page says so.
                let button = page.stick_rows();
                nav.sel = button;
                let before = page.rows()[button].1.clone();
                page.step(nav.sel, 1);
                let after = page.rows()[button].1.clone();
                if before == after || page.mapping == Mapping::defaults("snes") {
                    faults.push(at(&format!("Right didn't assign: {before} → {after}")));
                }
                assert_eq!(page.mapping.target[MENU_ORDER[0] as usize], Some(MENU_ORDER[1]), "{}", at("A now presses the next button"));
                // Up from the first row reaches Reset controls (round); A resets.
                nav.sel = 0;
                nav.step(-1, page.count());
                assert_eq!(page.rows()[nav.sel].0, "Reset controls", "{}", at("Up goes round to Reset controls"));
                page.step(nav.sel, 1);
                assert_eq!(page.mapping, Mapping::defaults("snes"), "{}", at("Reset controls"));
                // B back onto Controls, B again leaves Options.
                assert!(nav.back(), "{}", at("B goes back"));
                assert_eq!((nav.page, top_rows[nav.sel]), (None, Entry::Controls), "{}", at("onto the Controls row"));
                assert!(!nav.back(), "{}", at("B on the top page is the pause menu's"));
            }
        }
        crate::type_scale::set(TextSize::Normal);
        crate::i18n::set(crate::i18n::Lang::En);
        assert!(faults.is_empty(), "{}", faults.join("\n"));
    }
}
