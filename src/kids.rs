//! Kids Mode (docs/KIDS-MODE.md, docs/user/kids-mode.md): what a grown-up chose for a child, and the small state machines that
//! don't need the App: the PIN's dials and tries, the setup's steps, the day's play time, the recovery file. The App's
//! part (shelf, pause, pages) is `app_kids.rs`.
//!
//! Everything lives in `kids.json` and `kids-time.json`. The normal settings are not touched while the mode is on, so
//! turning it off brings them all back.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::pad::Button;

/// How many games a grown-up picks (fewer when the library has fewer).
pub const MIN_GAMES: usize = 5;
pub const MAX_GAMES: usize = 20;
pub const PIN_LEN: usize = 4;
/// The file that turns the mode off when the PIN is forgotten.
pub const RESET_FILE: &str = "kids-mode-off.txt";
/// How long SELECT + START are held to leave.
pub const LEAVE_HOLD: Duration = Duration::from_secs(5);
/// From this many wrong PINs on, the page says what to do when it is forgotten.
pub const WRONG_BEFORE_HELP: u32 = 3;
/// From this many on, each further try waits.
pub const WRONG_BEFORE_WAIT: u32 = 5;
pub const WRONG_WAIT: Duration = Duration::from_secs(60);
/// A grown-up's "a bit longer".
pub const MORE_MINUTES: u32 = 30;
/// How long before the time is up the child is told.
pub const WARN_SECONDS: u64 = 5 * 60;

/// The choices for the time per day, None: no limit.
pub const LIMITS: [Option<u32>; 6] = [None, Some(30), Some(45), Some(60), Some(90), Some(120)];
/// The choices for the volume ceiling, None: no limit.
pub const VOLUMES: [Option<u8>; 5] = [None, Some(40), Some(55), Some(70), Some(85)];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Blue,
    Pink,
}

/// A theme's colours: the background's gradient and shapes, the text on it, the highlight.
#[derive(Clone, Copy, Debug)]
pub struct Colours {
    pub top: u32,
    pub bottom: u32,
    pub shape: u32,
    pub spark: u32,
    pub ink: u32,
    pub accent: u32,
    pub on_accent: u32,
}

impl Theme {
    pub fn name(self) -> &'static str {
        match self {
            Theme::Blue => "Blue",
            Theme::Pink => "Pink",
        }
    }

    pub fn colours(self) -> Colours {
        match self {
            Theme::Blue => Colours { top: 0x0b1a5e, bottom: 0x1f8fe0, shape: 0x3fb4f5, spark: 0xbfeaff, ink: 0xffffff, accent: 0xffd23f, on_accent: 0x10205a },
            Theme::Pink => Colours { top: 0x6a1470, bottom: 0xe2447f, shape: 0xff86b8, spark: 0xffe6f2, ink: 0xffffff, accent: 0xfff06a, on_accent: 0x6a1464 },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub on: bool,
    pub pin: String,
    /// The chosen games by their file, in the order they stand on the shelf.
    pub games: Vec<PathBuf>,
    pub theme: Theme,
    /// Play time per day in minutes.
    pub minutes: Option<u32>,
    /// The volume ceiling in percent.
    pub volume: Option<u8>,
    /// The recovery file already acted on (its modification time): one on a card's read-only part stays where it is.
    pub reset_seen: Option<u64>,
}

impl Config {
    fn file(data_dir: &Path) -> PathBuf {
        data_dir.join("kids.json")
    }

    /// A file that can't be read is the mode off: a child never gets stuck on a broken screen, a grown-up sets it up again.
    pub fn load(data_dir: &Path) -> Config {
        std::fs::read(Self::file(data_dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) {
        if let Err(e) = crate::fsutil::write_json(&Self::file(data_dir), self) {
            eprintln!("kids.json: {e}");
        }
    }
}

/// How much was played today, in `kids-time.json`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ledger {
    pub day: i64,
    pub seconds: u64,
    /// "A bit longer" from a grown-up, for today.
    pub extra_minutes: u32,
    /// The day's minutes in all that the child was last told about (`warning_due`).
    pub warned_for: u32,
}

impl Ledger {
    fn file(data_dir: &Path) -> PathBuf {
        data_dir.join("kids-time.json")
    }

    pub fn load(data_dir: &Path) -> Ledger {
        std::fs::read(Self::file(data_dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) {
        crate::fsutil::write_json_soon(&Self::file(data_dir), self);
    }

    /// A later date starts at nought. An earlier one is a clock that lost its time (no RTC, no network): the minutes go
    /// on counting from the last date known, so a reset clock is no new day.
    pub fn roll(&mut self, day: i64) {
        if day > self.day {
            *self = Ledger { day, ..Ledger::default() };
        }
    }

    /// The clock says a day before the last one known.
    pub fn clock_behind(&self, day: i64) -> bool {
        day < self.day
    }

    /// A grown-up's "start today's time again": the count from nought, under today's date as the clock has it. The way
    /// out when a clock that was wrong kept a date in the future.
    pub fn start_over(&mut self, day: i64) {
        *self = Ledger { day, ..Ledger::default() };
    }

    pub fn is_up(&self, minutes: Option<u32>) -> bool {
        minutes.is_some_and(|m| self.seconds >= u64::from(m + self.extra_minutes) * 60)
    }

    /// The whole minutes left to tell the child, once per limit: when `WARN_SECONDS` or less remain. More time from a
    /// grown-up is a new limit, with its own warning.
    pub fn warning_due(&mut self, minutes: Option<u32>) -> Option<u32> {
        let total = minutes? + self.extra_minutes;
        let left = (u64::from(total) * 60).checked_sub(self.seconds).filter(|l| *l > 0 && *l <= WARN_SECONDS)?;
        if self.warned_for == total {
            return None;
        }
        self.warned_for = total;
        Some(left.div_ceil(60) as u32)
    }
}

/// Today's number, by the device's clock and time zone.
pub fn today() -> i64 {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let t = now as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    crate::timezone::tzset();
    // SAFETY: localtime_r only writes into the provided struct.
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return (now / 86_400) as i64;
    }
    i64::from(tm.tm_year) * 366 + i64::from(tm.tm_yday)
}

/// The four dials the PIN is entered with: Up and Down turn one, Left and Right go between them, A goes on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dials {
    pub digits: [u8; PIN_LEN],
    pub at: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DialOut {
    Stay,
    Entered,
    Back,
}

impl Dials {
    pub fn press(&mut self, b: Button) -> DialOut {
        match b {
            Button::Up => self.digits[self.at] = (self.digits[self.at] + 1) % 10,
            Button::Down => self.digits[self.at] = (self.digits[self.at] + 9) % 10,
            Button::Left => self.at = self.at.saturating_sub(1),
            Button::Right => self.at = (self.at + 1).min(PIN_LEN - 1),
            Button::East if self.at + 1 == PIN_LEN => return DialOut::Entered,
            Button::East => self.at += 1,
            Button::South if self.at == 0 => return DialOut::Back,
            Button::South => self.at -= 1,
            _ => {}
        }
        DialOut::Stay
    }

    pub fn pin(&self) -> String {
        self.digits.iter().map(|d| char::from(b'0' + d)).collect()
    }
}

/// The PIN page: the dials and how many tries went wrong.
#[derive(Clone, Debug, Default)]
pub struct Gate {
    pub dials: Dials,
    pub wrong: u32,
    pub wait_until: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GateOut {
    Stay,
    Open,
    Back,
}

impl Gate {
    pub fn waiting(&self, now: Instant) -> Option<Duration> {
        self.wait_until.filter(|t| *t > now).map(|t| t - now)
    }

    pub fn press(&mut self, b: Button, pin: &str, now: Instant) -> GateOut {
        if self.waiting(now).is_some() {
            return if b == Button::South { GateOut::Back } else { GateOut::Stay };
        }
        match self.dials.press(b) {
            DialOut::Stay => GateOut::Stay,
            DialOut::Back => GateOut::Back,
            DialOut::Entered if self.dials.pin() == pin => GateOut::Open,
            DialOut::Entered => {
                self.wrong += 1;
                self.dials = Dials::default();
                if self.wrong >= WRONG_BEFORE_WAIT {
                    self.wait_until = Some(now + WRONG_WAIT);
                }
                GateOut::Stay
            }
        }
    }
}

/// A game that can be picked.
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    pub system: String,
    pub name: String,
    pub path: PathBuf,
    /// Saves only where the game itself saves: Stop keeps no moment (Xbox, Wii channels).
    pub own_saves: bool,
}

/// The steps a grown-up goes through, at the first start or from the grown-ups' page.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Welcome,
    Pin { dials: Dials, first: Option<String>, mismatch: bool },
    Games { sel: usize },
    Theme,
    Limit,
    Volume,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlowOut {
    Stay,
    /// B on the first step: nothing is kept.
    Back,
    /// Finished: `draft` is what was chosen.
    Done,
    /// No game to pick yet: the grown-up wants Add games first, and comes back here.
    AddGames,
}

pub struct Flow {
    pub stage: Stage,
    pub draft: Config,
    pub library: Vec<Pick>,
    /// One page of the grown-ups' menu instead of the whole way: it ends after this step.
    pub single: bool,
    /// What "A" on Games last refused: the list is full.
    pub full: bool,
    /// Games on the card that can't be picked: their emulators have menus of their own (`App::kids_library`).
    pub left_out: usize,
    /// The first start goes on after these steps, with Add games among them: no need to go there from here.
    pub setup_follows: bool,
}

impl Flow {
    pub fn new(draft: Config, library: Vec<Pick>) -> Flow {
        Flow { stage: Stage::Welcome, draft, library, single: false, full: false, left_out: 0, setup_follows: false }
    }

    pub fn single(stage: Stage, draft: Config, library: Vec<Pick>) -> Flow {
        Flow { stage, draft, library, single: true, full: false, left_out: 0, setup_follows: false }
    }

    /// How many games must be picked to go on.
    pub fn needed(&self) -> usize {
        MIN_GAMES.min(self.library.len())
    }

    pub fn can_leave_games(&self) -> bool {
        self.draft.games.len() >= self.needed()
    }

    fn after(&mut self, next: Stage) -> FlowOut {
        if self.single {
            return FlowOut::Done;
        }
        self.stage = next;
        FlowOut::Stay
    }

    pub fn press(&mut self, b: Button) -> FlowOut {
        self.full = false;
        // Stepping back asks every step again, the new PIN too.
        let back = |f: &mut Flow, to: Stage| {
            if f.single {
                return FlowOut::Back;
            }
            f.stage = to;
            FlowOut::Stay
        };
        match &mut self.stage {
            Stage::Welcome => match b {
                Button::East => self.stage = Stage::Pin { dials: Dials::default(), first: None, mismatch: false },
                Button::South => return FlowOut::Back,
                _ => {}
            },
            Stage::Pin { dials, first, mismatch } => match dials.press(b) {
                DialOut::Stay => {}
                DialOut::Back if first.is_some() => {
                    *first = None;
                    *dials = Dials::default();
                }
                DialOut::Back => return back(self, Stage::Welcome),
                DialOut::Entered => match first.take() {
                    None => {
                        *first = Some(dials.pin());
                        *dials = Dials::default();
                        *mismatch = false;
                    }
                    Some(f) if f == dials.pin() => {
                        self.draft.pin = f;
                        return self.after(Stage::Games { sel: 0 });
                    }
                    Some(_) => {
                        *dials = Dials::default();
                        *mismatch = true;
                    }
                },
            },
            Stage::Games { sel } => {
                let n = self.library.len();
                match b {
                    Button::Up if n > 0 => *sel = (*sel + n - 1) % n,
                    Button::Down if n > 0 => *sel = (*sel + 1) % n,
                    Button::L1 => *sel = sel.saturating_sub(8),
                    Button::R1 if n > 0 => *sel = (*sel + 8).min(n - 1),
                    Button::East if n > 0 => {
                        let path = self.library[*sel].path.clone();
                        if let Some(at) = self.draft.games.iter().position(|g| *g == path) {
                            self.draft.games.remove(at);
                        } else if self.draft.games.len() < MAX_GAMES {
                            self.draft.games.push(path);
                        } else {
                            self.full = true;
                        }
                    }
                    Button::East if !self.setup_follows => return FlowOut::AddGames,
                    Button::East | Button::Start if self.can_leave_games() => return self.after(Stage::Theme),
                    Button::South => return back(self, Stage::Welcome),
                    _ => {}
                }
            }
            Stage::Theme => match b {
                Button::Left | Button::Right | Button::Up | Button::Down => {
                    self.draft.theme = if self.draft.theme == Theme::Blue { Theme::Pink } else { Theme::Blue };
                }
                Button::East => return self.after(Stage::Limit),
                Button::South => return back(self, Stage::Games { sel: 0 }),
                _ => {}
            },
            Stage::Limit => match b {
                Button::Left | Button::Right => self.draft.minutes = stepped(&LIMITS, self.draft.minutes, b == Button::Right),
                Button::East => return self.after(Stage::Volume),
                Button::South => return back(self, Stage::Theme),
                _ => {}
            },
            Stage::Volume => match b {
                Button::Left | Button::Right => self.draft.volume = stepped(&VOLUMES, self.draft.volume, b == Button::Right),
                Button::East => return self.after(Stage::Ready),
                Button::South => return back(self, Stage::Limit),
                _ => {}
            },
            Stage::Ready => match b {
                Button::East => return FlowOut::Done,
                Button::South => return back(self, Stage::Volume),
                _ => {}
            },
        }
        FlowOut::Stay
    }
}

/// The next or previous choice of a list, stopping at its ends.
fn stepped<T: Copy + PartialEq>(list: &[T], now: T, forward: bool) -> T {
    let at = list.iter().position(|v| *v == now).unwrap_or(0);
    let to = if forward { (at + 1).min(list.len() - 1) } else { at.saturating_sub(1) };
    list[to]
}

/// Where the recovery file counts: the card's first part, the one any phone or computer can read, and the games' folder,
/// which the Wi-Fi share and the phone upload reach.
pub fn reset_places() -> Vec<PathBuf> {
    ["/flash", "/storage/roms", "/storage/games-external"].iter().map(|d| Path::new(d).join(RESET_FILE)).collect()
}

/// The recovery file's modification time, when one lies in `places` that wasn't acted on yet.
pub fn reset_wanted(places: &[PathBuf], seen: Option<u64>) -> Option<u64> {
    places.iter().find_map(|p| {
        let t = std::fs::metadata(p).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        (seen != Some(t)).then_some(t)
    })
}

/// What the mode makes of a route, at the picker and at every start: `Some(keeps its moment)` where the mode can close the
/// emulator's own menus and shortcuts, None where it can't (RetroArch's menu, PortMaster, pocket-core on a system without states).
/// `hosted`: pocket-core has this system with its states.
pub fn route_keeps_moment(system: &str, emulator: &str, core: &str, rom: &Path, hosted: bool) -> Option<bool> {
    if emulator == "pocket-core" {
        hosted.then_some(true)
    } else {
        crate::standalone::kids_keeps_moment(system, emulator, core, rom)
    }
}

/// Whether the day's time counts now: a game on the screen and running, not its pause menu or its way in, and not a game
/// paused behind a sleeping screen. `dark_game`: the screen was turned off with the game going on, which is still play.
pub fn time_counts(game: bool, menu: bool, starting: bool, awake: bool, dark_game: bool) -> bool {
    game && !menu && !starting && (awake || dark_game)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_game_paused_by_sleep_uses_none_of_the_days_time() {
        assert!(time_counts(true, false, false, true, false), "playing");
        assert!(!time_counts(true, false, false, false, false), "light sleep: the game is paused behind the dark screen");
        assert!(time_counts(true, false, false, false, true), "the screen off with the game going on is still play");
        assert!(!time_counts(true, true, false, true, false), "the pause menu");
        assert!(!time_counts(true, false, true, true, false), "on its way in");
        assert!(!time_counts(false, false, false, true, false), "no game");
    }

    #[test]
    fn a_route_with_menus_of_its_own_is_never_a_childs() {
        let rom = Path::new("/roms/snes/Game.sfc");
        assert_eq!(route_keeps_moment("snes", "pocket-core", "snes9x", rom, true), Some(true));
        assert_eq!(route_keeps_moment("gb", "pocket-core", "gambatte", rom, false), None, "pocket-core without states");
        assert_eq!(route_keeps_moment("snes", "retroarch", "snes9x", rom, true), None, "where a failed start would have gone");
        assert_eq!(route_keeps_moment("ports", "", "", rom, false), None);
    }

    use super::*;

    fn enter(gate: &mut Gate, pin: &str, now: Instant) -> GateOut {
        let mut out = GateOut::Stay;
        for d in pin.bytes().map(|b| b - b'0') {
            for _ in 0..d {
                gate.press(Button::Up, "", now);
            }
            out = gate.press(Button::East, pin, now);
        }
        out
    }

    #[test]
    fn the_right_pin_opens_and_wrong_ones_ask_to_wait() {
        let now = Instant::now();
        let mut gate = Gate::default();
        assert_eq!(enter(&mut gate, "1234", now), GateOut::Open);

        let mut gate = Gate::default();
        for _ in 0..WRONG_BEFORE_WAIT {
            for _ in 0..PIN_LEN {
                gate.press(Button::East, "1234", now);
            }
        }
        assert_eq!(gate.wrong, WRONG_BEFORE_WAIT);
        assert!(gate.waiting(now).is_some());
        assert_eq!(gate.press(Button::East, "0000", now), GateOut::Stay);
        assert_eq!(gate.press(Button::South, "1234", now), GateOut::Back);
        assert!(gate.waiting(now + WRONG_WAIT).is_none());
    }

    fn library(n: usize) -> Vec<Pick> {
        (0..n).map(|i| Pick { system: "snes".into(), name: format!("Game {i}"), path: format!("/g/{i}.sfc").into(), own_saves: false }).collect()
    }

    fn pin_twice(flow: &mut Flow, second: u8) {
        for _ in 0..PIN_LEN {
            flow.press(Button::East);
        }
        for _ in 0..second {
            flow.press(Button::Up);
        }
        for _ in 0..PIN_LEN {
            flow.press(Button::East);
        }
    }

    #[test]
    fn the_whole_way_through_setup_keeps_what_was_chosen() {
        let mut flow = Flow::new(Config::default(), library(6));
        flow.press(Button::East);
        // Two different PINs: asked again.
        pin_twice(&mut flow, 1);
        assert!(matches!(flow.stage, Stage::Pin { mismatch: true, .. }));
        // The same twice.
        flow.stage = Stage::Pin { dials: Dials::default(), first: None, mismatch: false };
        pin_twice(&mut flow, 0);
        assert!(matches!(flow.stage, Stage::Games { .. }));
        assert_eq!(flow.draft.pin, "0000");
        // START does nothing below five games.
        flow.press(Button::Start);
        assert!(matches!(flow.stage, Stage::Games { .. }));
        for _ in 0..5 {
            flow.press(Button::East);
            flow.press(Button::Down);
        }
        flow.press(Button::Start);
        assert_eq!(flow.stage, Stage::Theme);
        flow.press(Button::Right);
        flow.press(Button::East);
        flow.press(Button::Right);
        flow.press(Button::East);
        flow.press(Button::Right);
        flow.press(Button::East);
        assert_eq!(flow.press(Button::East), FlowOut::Done);
        assert_eq!(flow.draft.games.len(), 5);
        assert_eq!(flow.draft.theme, Theme::Pink);
        assert_eq!(flow.draft.minutes, Some(30));
        assert_eq!(flow.draft.volume, Some(40));
    }

    #[test]
    fn a_small_or_empty_library_can_still_go_on_and_twenty_is_the_most() {
        let mut flow = Flow::single(Stage::Games { sel: 0 }, Config::default(), library(2));
        assert_eq!(flow.needed(), 2);
        flow.press(Button::East);
        flow.press(Button::Down);
        flow.press(Button::East);
        assert_eq!(flow.press(Button::Start), FlowOut::Done);

        // No games: A is the way to Add games, START goes on without; in the first start, Add games comes after it anyway.
        let mut flow = Flow::single(Stage::Games { sel: 0 }, Config::default(), library(0));
        assert_eq!(flow.press(Button::East), FlowOut::AddGames);
        assert_eq!(flow.press(Button::Start), FlowOut::Done);
        let mut flow = Flow::new(Config::default(), library(0));
        flow.setup_follows = true;
        flow.stage = Stage::Games { sel: 0 };
        assert_eq!(flow.press(Button::East), FlowOut::Stay);
        assert_eq!(flow.stage, Stage::Theme);

        let mut flow = Flow::single(Stage::Games { sel: 0 }, Config::default(), library(25));
        for _ in 0..25 {
            flow.press(Button::East);
            if flow.full {
                break;
            }
            flow.press(Button::Down);
        }
        assert_eq!(flow.draft.games.len(), MAX_GAMES);
        assert!(flow.full);
    }

    #[test]
    fn time_is_counted_per_day_and_a_grown_up_can_add_some() {
        let mut l = Ledger { day: 5, seconds: 1800, ..Ledger::default() };
        assert!(l.is_up(Some(30)));
        assert!(!l.is_up(None));
        assert!(!l.is_up(Some(45)));
        l.extra_minutes = 30;
        assert!(!l.is_up(Some(30)));
        l.roll(6);
        assert_eq!(l, Ledger { day: 6, ..Ledger::default() });
    }

    #[test]
    fn a_clock_that_lost_its_time_is_no_new_day() {
        let mut l = Ledger { day: 100, seconds: 1200, extra_minutes: 30, warned_for: 0 };
        l.roll(3);
        assert_eq!((l.day, l.seconds, l.extra_minutes), (100, 1200, 30), "the count goes on under the last date known");
        assert!(l.clock_behind(3));
        l.roll(100);
        assert_eq!(l.seconds, 1200);
        assert!(!l.clock_behind(100));
        l.roll(101);
        assert_eq!((l.day, l.seconds, l.extra_minutes), (101, 0, 0), "a real new date");
        // A grown-up's way out of a date kept from a clock that ran ahead.
        let mut l = Ledger { day: 900, seconds: 3600, ..Ledger::default() };
        l.start_over(100);
        assert_eq!((l.day, l.seconds), (100, 0));
        l.roll(101);
        assert_eq!(l.day, 101);
    }

    #[test]
    fn the_warning_comes_once_five_minutes_before_and_again_after_more_time() {
        let limit = Some(30);
        let mut l = Ledger { day: 1, seconds: 24 * 60, ..Ledger::default() };
        assert_eq!(l.warning_due(limit), None, "six minutes left");
        assert_eq!(l.warning_due(None), None, "no limit");
        l.seconds = 25 * 60;
        assert_eq!(l.warning_due(limit), Some(5));
        assert_eq!(l.warning_due(limit), None, "once");
        l.seconds = 30 * 60;
        assert_eq!(l.warning_due(limit), None, "time up: the goodbye says it");
        l.extra_minutes = 30;
        assert_eq!(l.warning_due(limit), None, "plenty again");
        l.seconds = 57 * 60 + 30;
        assert_eq!(l.warning_due(limit), Some(3), "a game started with little left says what is left");
    }

    #[test]
    fn a_broken_or_missing_file_is_the_mode_off_and_the_settings_come_back_as_saved() {
        let dir = crate::testutil::TestDir::new("kids");
        assert_eq!(Config::load(&dir), Config::default());
        std::fs::write(dir.join("kids.json"), b"{ not json").unwrap();
        assert!(!Config::load(&dir).on);
        let c = Config { on: true, pin: "1234".into(), games: vec!["/g/a.sfc".into()], theme: Theme::Pink, minutes: Some(45), volume: Some(55), reset_seen: None };
        c.save(&dir);
        assert_eq!(Config::load(&dir), c);
    }

    #[test]
    fn the_recovery_file_counts_once() {
        let dir = crate::testutil::TestDir::new("kids");
        let places = vec![dir.join("nope").join(RESET_FILE), dir.join(RESET_FILE)];
        assert_eq!(reset_wanted(&places, None), None);
        std::fs::write(&places[1], b"").unwrap();
        let t = reset_wanted(&places, None).expect("the file is seen");
        assert_eq!(reset_wanted(&places, Some(t)), None);
    }
}
