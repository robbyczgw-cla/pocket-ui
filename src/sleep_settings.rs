//! Settings → System → Sleep & power: when the screen goes off, when the Pocket sleeps and switches off, and what
//! the power button does (docs/SLEEP.md, "Sleep & power settings"). One page in the manner of NextUI's System
//! settings (Screen timeout, Suspend timeout): the times of the two sleep steps, the switch-off after sleeping, and
//! the button's short press and hold; and the cozy extra that comes before the screen goes off, the slideshow of your
//! games (`cozy_extras::Extra::IdleSlideshow`, kept in settings.json, not sleep.json: the App switches it).
//!
//! The choices live in `<data>/sleep.json` with `sleep.rs`, which acts on them (`Sleep::set_config`). Files from
//! before this page (2026-10-26) had only `light_after` (minutes: Off, 1, 3, 5) and `off_after`: `light_after` becomes
//! the screen-off time as it was (3 min stays 3 min, although the page's own choices go 30 s, 1, 2, 5 min), and it is
//! still written, rounded up to whole minutes, so an older pocket-ui (the rescue page's "previous version") reads the
//! same time. Nothing here touches the system: every path is a plain test; painting is in `app_sleep_power.rs`.

use serde::{Deserialize, Deserializer, Serialize};

use crate::pad::Button;

/// Screen off after (seconds without input; None = never).
pub const SCREEN_OFF_CHOICES: [Option<u32>; 5] = [None, Some(30), Some(60), Some(120), Some(300)];
/// 2 min. NextUI turns its screen off after 60 s, a phone after 30 s to a minute; but here a game pauses when the
/// screen goes off, and a long cutscene, a text to read or a race replay can go a minute without a button. Two
/// minutes keep those running and still save the battery a minute sooner than the 3 min of before.
pub const SCREEN_OFF_DEFAULT: Option<u32> = Some(120);
/// Sleep after (minutes with the screen off before the Pocket sleeps; None = never).
pub const SLEEP_AFTER_CHOICES: [Option<u32>; 5] = [None, Some(1), Some(2), Some(5), Some(15)];
/// 2 min, as it was fixed before the setting: long enough to pick the Pocket up again without waiting for a wake.
pub const SLEEP_AFTER_DEFAULT: Option<u32> = Some(2);

/// What a short press of the power button does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShortPress {
    /// Save the game and suspend (handhelds and phones do this).
    #[default]
    Sleep,
    /// Only the screen goes dark: the game and its sound keep running (`sleep.rs`, `Sleep::screen_off_now`).
    ScreenOff,
}

/// What holding the power button (`powerkey::LONG_PRESS`) does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Hold {
    /// The power menu (Sleep, Restart, Power off, …), as on Android.
    #[default]
    Menu,
    /// Sleep, when the button is let go; the power menu stays in the quick settings ("Power…").
    Sleep,
}

impl ShortPress {
    pub const ALL: [ShortPress; 2] = [ShortPress::Sleep, ShortPress::ScreenOff];

    pub fn label(self) -> &'static str {
        match self {
            ShortPress::Sleep => "Sleep",
            ShortPress::ScreenOff => "Screen off",
        }
    }

    fn key(self) -> &'static str {
        match self {
            ShortPress::Sleep => "sleep",
            ShortPress::ScreenOff => "screen_off",
        }
    }
}

impl Hold {
    pub const ALL: [Hold; 2] = [Hold::Menu, Hold::Sleep];

    pub fn label(self) -> &'static str {
        match self {
            Hold::Menu => "Power menu",
            Hold::Sleep => "Sleep",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Hold::Menu => "menu",
            Hold::Sleep => "sleep",
        }
    }
}

/// Everything on the page, as `sleep.json` keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "Stored", into = "Stored")]
pub struct Config {
    /// Seconds without input before the screen goes off (light sleep); None = never.
    pub screen_off: Option<u32>,
    /// Minutes with the screen off before the system suspends (deep sleep); None = never.
    pub sleep_after: Option<u32>,
    /// Hours in deep sleep before the Pocket saves and switches off (`auto_off`); None = never.
    pub off_after: Option<u32>,
    pub short_press: ShortPress,
    pub hold: Hold,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            screen_off: SCREEN_OFF_DEFAULT,
            sleep_after: SLEEP_AFTER_DEFAULT,
            off_after: crate::auto_off::DEFAULT,
            short_press: ShortPress::default(),
            hold: Hold::default(),
        }
    }
}

/// `sleep.json` as written. The times are numbers with 0 for never, so that a missing key (a file from before) and
/// "never" are told apart; `light_after` is the old key, read when `screen_off_secs` is missing and still written.
#[derive(Serialize, Deserialize)]
struct Stored {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    screen_off_secs: Option<u32>,
    /// Some(None): written as null, "Off". None: not in the file.
    #[serde(default, deserialize_with = "present")]
    light_after: Option<Option<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sleep_after_min: Option<u32>,
    #[serde(default = "default_off")]
    off_after: Option<u32>,
    /// Words, not an enum, so that a value of a newer version leaves the rest of the file readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    short_press: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hold: Option<String>,
}

fn default_off() -> Option<u32> {
    crate::auto_off::DEFAULT
}

/// A key that is there, even as null.
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<u32>>, D::Error> {
    Option::<u32>::deserialize(d).map(Some)
}

fn never_is_zero(v: u32) -> Option<u32> {
    (v > 0).then_some(v)
}

impl From<Stored> for Config {
    fn from(s: Stored) -> Config {
        let screen_off = match (s.screen_off_secs, s.light_after) {
            (Some(secs), _) => never_is_zero(secs),
            // Before this page: the minutes of Settings → Playing → Battery's "Sleep after".
            (None, Some(minutes)) => minutes.filter(|m| *m > 0).map(|m| m.saturating_mul(60)),
            (None, None) => SCREEN_OFF_DEFAULT,
        };
        Config {
            screen_off,
            sleep_after: s.sleep_after_min.map_or(SLEEP_AFTER_DEFAULT, never_is_zero),
            off_after: s.off_after,
            short_press: ShortPress::ALL.into_iter().find(|p| s.short_press.as_deref() == Some(p.key())).unwrap_or_default(),
            hold: Hold::ALL.into_iter().find(|h| s.hold.as_deref() == Some(h.key())).unwrap_or_default(),
        }
    }
}

impl From<Config> for Stored {
    fn from(c: Config) -> Stored {
        Stored {
            screen_off_secs: Some(c.screen_off.unwrap_or(0)),
            light_after: Some(c.screen_off.map(|s| s.div_ceil(60))),
            sleep_after_min: Some(c.sleep_after.unwrap_or(0)),
            off_after: c.off_after,
            short_press: Some(c.short_press.key().into()),
            hold: Some(c.hold.key().into()),
        }
    }
}

/// "Never", "30 s", "1 min", "2 min"; a time from an older file that is no choice here as it is ("3 min", "90 s").
pub fn screen_off_label(secs: Option<u32>) -> String {
    match secs {
        None => "Never".into(),
        Some(s) if s % 60 == 0 => format!("{} min", s / 60),
        Some(s) => format!("{s} s"),
    }
}

/// "Never", "1 min", "15 min".
pub fn sleep_after_label(minutes: Option<u32>) -> String {
    minutes.map_or("Never".into(), |m| format!("{m} min"))
}

/// The next choice of `choices` from `now` in the direction of `delta`, stopping at the ends; `round` (A) goes on from
/// the last to the first. A value that is no choice (an older file's 3 min) goes to its neighbour on that side. Never
/// (None) is the first choice and counts as the lowest.
fn step_time(choices: &[Option<u32>], now: Option<u32>, delta: i32, round: bool) -> Option<u32> {
    let key = |v: Option<u32>| v.map_or(0u64, |n| n as u64 + 1);
    let next = if delta < 0 {
        choices.iter().rev().find(|c| key(**c) < key(now)).copied()
    } else {
        choices.iter().find(|c| key(**c) > key(now)).copied()
    };
    match next {
        Some(v) => v,
        None if round && delta > 0 => choices[0],
        None => now,
    }
}

fn step_of<T: Copy + PartialEq>(choices: &[T], now: T, delta: i32, round: bool) -> T {
    let n = choices.len() as i32;
    let i = choices.iter().position(|c| *c == now).unwrap_or(0) as i32;
    let j = i + delta.signum();
    if round {
        choices[j.rem_euclid(n) as usize]
    } else {
        choices[j.clamp(0, n - 1) as usize]
    }
}

/// A row of the page, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    ScreenOff,
    /// The idle slideshow before the screen goes off: On or Off (`Action::Slideshow`).
    Slideshow,
    SleepAfter,
    SwitchOff,
    ShortPress,
    Hold,
}

/// Kept short enough for the settings page in both languages.
pub const NO_SLEEP: &str = "ROCKNIX sleep is unavailable here; the screen turns off instead.";
const SCREEN_ONLY_ROWS: [Row; 4] = [Row::ScreenOff, Row::Slideshow, Row::SwitchOff, Row::ShortPress];
/// A Steam Deck: SteamOS owns sleep and the power button (docs/STEAMDECK.md); the slideshow is pocket-ui's own.
const HOST_ROWS: [Row; 1] = [Row::Slideshow];

pub fn rows() -> &'static [Row] {
    if crate::device::current().host_owned() {
        &HOST_ROWS
    } else if crate::device::current().screen_off_only() {
        &SCREEN_ONLY_ROWS
    } else {
        &ROWS
    }
}

pub const ROWS: [Row; 6] = [Row::ScreenOff, Row::Slideshow, Row::SleepAfter, Row::SwitchOff, Row::ShortPress, Row::Hold];

/// Whether the slideshow before the screen goes off is on (`cozy_extras`).
fn slideshow() -> bool {
    crate::cozy_extras::on(crate::cozy_extras::Extra::IdleSlideshow)
}

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Row::ScreenOff => "Screen off after",
            Row::Slideshow => crate::cozy_extras::Extra::IdleSlideshow.label(),
            Row::SleepAfter => "Sleep after",
            Row::SwitchOff if crate::device::current().screen_off_only() => "Switch off after",
            Row::SwitchOff => "Switch off after sleeping",
            // Under the heading "Power button" (`starts_group`).
            Row::ShortPress => "Short press",
            Row::Hold => "Hold",
        }
    }

    /// Where the power button's rows begin: a heading line goes over them.
    pub fn starts_group(self) -> bool {
        self == Row::ShortPress
    }
}

impl Config {
    /// Ignore old suspend/button settings on SM8550; keep the user's explicit power-off time.
    pub fn for_device(mut self, screen_only: bool) -> Self {
        if screen_only {
            self.sleep_after = None;
            self.short_press = ShortPress::ScreenOff;
            self.hold = Hold::Menu;
        }
        self
    }

    /// The value of `row` as the page shows it.
    pub fn value(&self, row: Row) -> String {
        match row {
            Row::ScreenOff => screen_off_label(self.screen_off),
            Row::Slideshow => if slideshow() { "On" } else { "Off" }.into(),
            Row::SleepAfter => sleep_after_label(self.sleep_after),
            Row::SwitchOff => crate::auto_off::label(self.off_after),
            Row::ShortPress => self.short_press.label().into(),
            Row::Hold => self.hold.label().into(),
        }
    }

    /// `row` one choice on: Left/Right (`delta`) stop at the ends, A (`round`) goes from the last to the first.
    pub fn step(&mut self, row: Row, delta: i32, round: bool) {
        if crate::device::current().screen_off_only() && matches!(row, Row::SleepAfter | Row::ShortPress | Row::Hold) {
            return;
        }
        match row {
            Row::ScreenOff => self.screen_off = step_time(&SCREEN_OFF_CHOICES, self.screen_off, delta, round),
            Row::SleepAfter => self.sleep_after = step_time(&SLEEP_AFTER_CHOICES, self.sleep_after, delta, round),
            Row::SwitchOff => self.off_after = step_time(&crate::auto_off::CHOICES, self.off_after, delta, round),
            Row::ShortPress => self.short_press = step_of(&ShortPress::ALL, self.short_press, delta, round),
            Row::Hold => self.hold = step_of(&Hold::ALL, self.hold, delta, round),
            // Not in sleep.json: `Page::press` hands it to the App.
            Row::Slideshow => {}
        }
    }

    /// What the selected row does with its value now: the lines under the rows.
    pub fn explain(&self, row: Row) -> &'static str {
        if crate::device::current().screen_off_only() && row == Row::SwitchOff {
            return "After this long with the screen off on battery, the Pocket saves the game and switches off.";
        }
        match row {
            Row::ScreenOff if self.screen_off.is_none() => {
                "The screen stays on. A short press of the power button still turns it off or puts the Pocket to sleep."
            }
            Row::ScreenOff => "How long without a button press before the screen goes dark. A game pauses then; any button brings it back.",
            Row::Slideshow => crate::cozy_extras::Extra::IdleSlideshow.blurb(),
            Row::SleepAfter if self.sleep_after.is_none() => {
                "When the screen turns off by itself, the Pocket stays awake until you press a button. That uses the battery."
            }
            Row::SleepAfter => "How long the screen stays dark before the Pocket sleeps. The game is saved first; the power button wakes it.",
            Row::SwitchOff => crate::auto_off::explain(self.off_after),
            Row::ShortPress => match self.short_press {
                ShortPress::Sleep => "Saves the game and puts the Pocket to sleep. Press the power button again to wake it.",
                ShortPress::ScreenOff => "Only the screen goes dark: the game and its sound keep running. Any button turns it back on.",
            },
            Row::Hold => match self.hold {
                Hold::Menu => "Hold it for about a second: Sleep, Restart, Power off and Restart pocket-ui.",
                Hold::Sleep => "Holding it also puts the Pocket to sleep. The power menu is still in the quick settings, under Power….",
            },
        }
    }
}

/// What a press on the page asks the App to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    /// Keep and apply these choices.
    Save(Config),
    /// Switch the slideshow before the screen goes off (a cozy extra, kept with the other settings).
    Slideshow,
}

/// The page: the selected row and the choices as shown.
#[derive(Clone, Copy, Debug)]
pub struct Page {
    pub sel: usize,
    pub config: Config,
}

impl Page {
    pub fn new(config: Config) -> Page {
        Page {
            sel: 0,
            config: config.for_device(crate::device::current().screen_off_only()),
        }
    }

    pub fn row(&self) -> Row {
        rows()[self.sel.min(rows().len() - 1)]
    }

    /// Up/Down move round, Left/Right change and stop at the ends, A goes on to the next choice (round), B closes.
    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = rows().len();
        let (delta, round) = match b {
            Button::Up => {
                self.sel = (self.sel + n - 1) % n;
                return Action::Nothing;
            }
            Button::Down => {
                self.sel = (self.sel + 1) % n;
                return Action::Nothing;
            }
            b if b == cancel => return Action::Close,
            Button::Left => (-1, false),
            Button::Right => (1, false),
            b if b == confirm => (1, true),
            _ => return Action::Nothing,
        };
        if self.row() == Row::Slideshow {
            return Action::Slideshow;
        }
        let before = self.config;
        self.config.step(self.row(), delta, round);
        if self.config == before {
            Action::Nothing
        } else {
            Action::Save(self.config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn parse(json: &str) -> Config {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn screen_only_normalises_old_sleep_choices_but_keeps_the_explicit_off_timer() {
        let old = Config { sleep_after: Some(1), off_after: Some(8), short_press: ShortPress::Sleep, hold: Hold::Sleep, ..Config::default() };
        let safe = old.for_device(true);
        assert_eq!((safe.sleep_after, safe.short_press, safe.hold), (None, ShortPress::ScreenOff, Hold::Menu));
        assert_eq!(safe.off_after, Some(8));
        assert_eq!(old.for_device(false), old, "SM8250 settings stay exactly as before");
    }

    #[test]
    fn the_defaults_are_two_minutes_two_minutes_no_switch_off_sleep_and_the_power_menu() {
        let c = Config::default();
        assert_eq!(c.screen_off, Some(120));
        assert_eq!(c.sleep_after, Some(2));
        assert_eq!(c.off_after, None, "Switch off after sleeping is opt-in: a sleeping Pocket stays asleep");
        assert_eq!((c.short_press, c.hold), (ShortPress::Sleep, Hold::Menu), "what handhelds and phones do");
        assert_eq!(parse("{}"), c, "no file, or an empty one");
    }

    #[test]
    fn a_file_from_before_the_page_keeps_its_time() {
        // Settings → Playing → Battery's "Sleep after": Off, 1, 3 (the old default), 5 min.
        assert_eq!(parse(r#"{"light_after":3,"off_after":3}"#).screen_off, Some(180), "3 min stays 3 min");
        assert_eq!(parse(r#"{"light_after":1}"#).screen_off, Some(60));
        assert_eq!(parse(r#"{"light_after":5,"off_after":8}"#).screen_off, Some(300));
        assert_eq!(parse(r#"{"light_after":null}"#).screen_off, None, "Off is Never");
        let old = parse(r#"{"light_after":3,"off_after":null}"#);
        assert_eq!(old.off_after, None, "Switch off after sleeping: Off");
        assert_eq!((old.sleep_after, old.short_press, old.hold), (Some(2), ShortPress::Sleep, Hold::Menu));
    }

    #[test]
    fn what_is_written_reads_back_the_same_and_older_versions_still_find_their_minutes() {
        let all = [
            Config::default(),
            Config { screen_off: None, sleep_after: None, off_after: None, short_press: ShortPress::ScreenOff, hold: Hold::Sleep },
            Config { screen_off: Some(30), sleep_after: Some(15), off_after: Some(8), ..Config::default() },
            Config { screen_off: Some(180), ..Config::default() },
        ];
        for c in all {
            let json = serde_json::to_string(&c).unwrap();
            assert_eq!(parse(&json), c, "{json}");
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            // An older pocket-ui reads `light_after` in minutes: 30 s is 1 min for it, never is null.
            let minutes = c.screen_off.map(|s| s.div_ceil(60));
            assert_eq!(v["light_after"], serde_json::to_value(minutes).unwrap(), "{json}");
        }
    }

    #[test]
    fn values_of_a_newer_version_leave_the_rest_readable() {
        let c = parse(r#"{"screen_off_secs":60,"short_press":"wink","hold":"dance","sleep_after_min":5}"#);
        assert_eq!((c.screen_off, c.sleep_after), (Some(60), Some(5)));
        assert_eq!((c.short_press, c.hold), (ShortPress::Sleep, Hold::Menu));
        let c = parse(r#"{"screen_off_secs":0,"sleep_after_min":0,"short_press":"screen_off","hold":"sleep"}"#);
        assert_eq!((c.screen_off, c.sleep_after, c.short_press, c.hold), (None, None, ShortPress::ScreenOff, Hold::Sleep));
    }

    #[test]
    fn the_choices_are_the_ones_on_the_page() {
        let mut c = Config { screen_off: None, ..Config::default() };
        let mut seen = vec![c.value(Row::ScreenOff)];
        for _ in 0..6 {
            c.step(Row::ScreenOff, 1, false);
            seen.push(c.value(Row::ScreenOff));
        }
        assert_eq!(seen, ["Never", "30 s", "1 min", "2 min", "5 min", "5 min", "5 min"], "Right stops at the end");
        c.step(Row::ScreenOff, 1, true);
        assert_eq!(c.value(Row::ScreenOff), "Never", "A goes round");
        c.step(Row::ScreenOff, -1, false);
        assert_eq!(c.value(Row::ScreenOff), "Never", "Left stops at the start");

        let mut c = Config { sleep_after: None, ..Config::default() };
        let mut seen = vec![c.value(Row::SleepAfter)];
        for _ in 0..4 {
            c.step(Row::SleepAfter, 1, false);
            seen.push(c.value(Row::SleepAfter));
        }
        assert_eq!(seen, ["Never", "1 min", "2 min", "5 min", "15 min"]);

        let mut c = Config { off_after: None, ..Config::default() };
        let mut seen = vec![c.value(Row::SwitchOff)];
        for _ in 0..3 {
            c.step(Row::SwitchOff, 1, false);
            seen.push(c.value(Row::SwitchOff));
        }
        assert_eq!(seen, ["Off", "1 h", "3 h", "8 h"]);

        let mut c = Config::default();
        assert_eq!((c.value(Row::ShortPress), c.value(Row::Hold)), ("Sleep".into(), "Power menu".into()));
        c.step(Row::ShortPress, 1, false);
        c.step(Row::Hold, 1, false);
        assert_eq!((c.value(Row::ShortPress), c.value(Row::Hold)), ("Screen off".into(), "Sleep".into()));
        c.step(Row::ShortPress, 1, true);
        assert_eq!(c.short_press, ShortPress::Sleep, "A goes round");
    }

    #[test]
    fn an_old_three_minutes_shows_as_it_is_and_steps_to_its_neighbours() {
        let mut c = parse(r#"{"light_after":3}"#);
        assert_eq!(c.value(Row::ScreenOff), "3 min");
        c.step(Row::ScreenOff, -1, false);
        assert_eq!(c.screen_off, Some(120));
        let mut c = parse(r#"{"light_after":3}"#);
        c.step(Row::ScreenOff, 1, false);
        assert_eq!(c.screen_off, Some(300));
        assert_eq!(screen_off_label(Some(90)), "90 s");
    }

    #[test]
    fn the_page_moves_round_changes_saves_and_closes() {
        let mut p = Page::new(Config::default());
        assert_eq!(p.press(Button::Up, A, B), Action::Nothing);
        assert_eq!(p.row(), Row::Hold, "Up from the top goes round");
        assert_eq!(p.press(Button::Down, A, B), Action::Nothing);
        assert_eq!(p.row(), Row::ScreenOff);
        let Action::Save(c) = p.press(Button::Left, A, B) else { panic!("a change is saved") };
        assert_eq!(c.screen_off, Some(60));
        assert_eq!(p.press(Button::Down, A, B), Action::Nothing);
        assert_eq!(p.row(), Row::Slideshow, "the slideshow comes right after the screen-off time");
        assert_eq!(p.press(Button::Right, A, B), Action::Slideshow, "switched by the App, not in sleep.json");
        assert_eq!(p.press(A, A, B), Action::Slideshow);
        p.press(Button::Down, A, B);
        p.press(Button::Down, A, B);
        p.press(Button::Down, A, B);
        assert_eq!(p.row(), Row::ShortPress);
        let Action::Save(c) = p.press(A, A, B) else { panic!() };
        assert_eq!(c.short_press, ShortPress::ScreenOff);
        // Nothing to save at an end.
        p.sel = 0;
        p.config.screen_off = Some(300);
        assert_eq!(p.press(Button::Right, A, B), Action::Nothing);
        assert_eq!(p.press(B, A, B), Action::Close);
    }

    #[test]
    fn every_row_and_value_has_its_explanation() {
        for c in [
            Config::default(),
            Config { screen_off: None, sleep_after: None, off_after: None, short_press: ShortPress::ScreenOff, hold: Hold::Sleep },
        ] {
            for row in ROWS {
                assert!(!c.explain(row).is_empty(), "{row:?}");
            }
        }
        let never = Config { screen_off: None, ..Config::default() };
        assert!(never.explain(Row::ScreenOff).contains("stays on"));
        let screen = Config { short_press: ShortPress::ScreenOff, ..Config::default() };
        assert!(screen.explain(Row::ShortPress).contains("keep running"));
        assert!(Config { hold: Hold::Sleep, ..Config::default() }.explain(Row::Hold).contains("quick settings"));
    }
}
