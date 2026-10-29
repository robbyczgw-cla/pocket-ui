//! Settings > Performance & fan: the state of the page and what a press on it asks for. The App saves the choices and
//! applies the fan outside games (`perf_fan`), and a thread brings the readout (`Event::Readout`); painting is in the
//! binary (`app_perf_fan.rs`). Nothing here touches the system, so every path is a plain test.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::pad::Button;
use crate::perf_fan::{Prefs, Readout};
use crate::power::{Fan, Performance};

/// A row of the page, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    GamesPerformance,
    GamesFan,
    OutsideFan,
    SmoothFan,
}

pub const ROWS: [Row; 4] = [Row::GamesPerformance, Row::GamesFan, Row::OutsideFan, Row::SmoothFan];

/// The choices of each row, in the order Left/Right walk through them.
const PERFORMANCE: [Performance; 3] = [Performance::Battery, Performance::Auto, Performance::Performance];
const GAMES_FAN: [Fan; 4] = [Fan::VeryQuiet, Fan::Quiet, Fan::Auto, Fan::Cool];
const OUTSIDE_FAN: [Fan; 4] = [Fan::Auto, Fan::VeryQuiet, Fan::Quiet, Fan::Cool];

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    /// Save these choices (and apply the fan outside games when it changed).
    Save(Prefs),
}

/// News for the page and the App from their threads.
#[derive(Debug)]
pub enum Event {
    /// The sensors, about once a second while the page is open.
    Readout(Readout),
    /// A `pocket-core --restore-system` the App waited for has finished (`perf_fan::Moment::settling`).
    Settled,
}

pub struct PerfFanPage {
    pub sel: usize,
    pub prefs: Prefs,
    /// None until the first reading arrives.
    pub readout: Option<Readout>,
    /// Stops the readout thread when the page closes.
    pub alive: Arc<AtomicBool>,
}

/// One step through `choices` from `now`, wrapping around like the pause menu's options.
fn step<T: Copy + PartialEq>(choices: &[T], now: T, delta: i32) -> T {
    let n = choices.len() as i32;
    let i = choices.iter().position(|c| *c == now).unwrap_or(0) as i32;
    choices[(i + delta).rem_euclid(n) as usize]
}

/// "Battery", "Auto", "Max": the same words in Quick settings, Settings, a port's page and the pause menu's Options
/// (`options.rs`, `emu_settings.rs`). Auto as on the fan rows: the system decides.
pub fn performance_label(p: Performance) -> &'static str {
    match p {
        Performance::Battery => "Battery",
        Performance::Auto => "Auto",
        Performance::Performance => "Max",
    }
}

pub fn fan_label(f: Fan) -> &'static str {
    match f {
        Fan::VeryQuiet => "Very quiet",
        Fan::Quiet => "Quiet",
        Fan::Auto => "Auto",
        Fan::Cool => "Cool",
    }
}

/// The fan rows cannot be changed here: ROCKNIX owns the fan (SM8550), or there is none (the H700 handhelds).
fn fan_not_ours(row: Row) -> bool {
    row != Row::GamesPerformance && (crate::device::current().screen_off_only() || !crate::device::current().has_fan())
}

impl PerfFanPage {
    pub fn new(prefs: Prefs) -> PerfFanPage {
        PerfFanPage {
            sel: 0,
            prefs,
            readout: None,
            alive: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn row(&self) -> Row {
        ROWS[self.sel.min(ROWS.len() - 1)]
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = ROWS.len();
        let delta = match b {
            Button::Up => {
                self.sel = (self.sel + n - 1) % n;
                return Action::Nothing;
            }
            Button::Down => {
                self.sel = (self.sel + 1) % n;
                return Action::Nothing;
            }
            b if b == cancel => {
                self.close();
                return Action::Close;
            }
            Button::Left => -1,
            Button::Right => 1,
            // A (and a tap on the selected row) moves on to the next choice.
            b if b == confirm => 1,
            _ => return Action::Nothing,
        };
        let row = self.row();
        if fan_not_ours(row) {
            return Action::Nothing;
        }
        let p = &mut self.prefs;
        match row {
            Row::GamesPerformance => p.games_performance = step(&PERFORMANCE, p.games_performance, delta),
            Row::GamesFan => p.games_fan = step(&GAMES_FAN, p.games_fan, delta),
            Row::OutsideFan => p.outside_fan = step(&OUTSIDE_FAN, p.outside_fan, delta),
            Row::SmoothFan => p.smooth_fan = !p.smooth_fan,
        }
        Action::Save(self.prefs)
    }

    /// The page is going away: the readout thread stops.
    pub fn close(&self) {
        self.alive.store(false, Ordering::Relaxed);
    }

    pub fn label(&self, row: Row) -> &'static str {
        match row {
            Row::GamesPerformance => "Games: performance",
            Row::GamesFan => "Games: fan",
            Row::OutsideFan => "Fan outside games",
            Row::SmoothFan => "Smooth fan",
        }
    }

    pub fn value(&self, row: Row) -> &'static str {
        if !crate::device::current().has_fan() && row != Row::GamesPerformance {
            return "No fan";
        }
        if fan_not_ours(row) {
            return "ROCKNIX";
        }
        match row {
            Row::GamesPerformance => performance_label(self.prefs.games_performance),
            Row::GamesFan => fan_label(self.prefs.games_fan),
            Row::OutsideFan => fan_label(self.prefs.outside_fan),
            Row::SmoothFan => on_off(self.prefs.smooth_fan),
        }
    }

    /// Under the list for the selected row: what the row is for, then what its current choice does.
    pub fn describe(&self, row: Row) -> [String; 2] {
        if !crate::device::current().has_fan() && row != Row::GamesPerformance {
            return ["This device has no fan.".into(), "Only the processor speed can be changed here.".into()];
        }
        if fan_not_ours(row) {
            return ["ROCKNIX controls the fan on this device.".into(), "".into()];
        }
        let what = match row {
            Row::GamesPerformance => "How fast the processor may run while you play.",
            Row::GamesFan => "How early the fan starts while you play.",
            Row::OutsideFan => "The fan on the shelf, in the lists and in Settings. While the Pocket sleeps it keeps the system's own cooling.",
            Row::SmoothFan => "How the fan changes its speed.",
        };
        let choice = match row {
            Row::GamesPerformance => match self.prefs.games_performance {
                Performance::Battery => "Battery: about half speed, which is enough for Game Boy and NES. The battery lasts longer.".to_string(),
                Performance::Auto => "Auto: the system decides; it speeds up and slows down as the game needs it.".to_string(),
                Performance::Performance => "Max: full speed all the time, for demanding games. Uses more battery and gets warmer.".to_string(),
            },
            Row::GamesFan => fan_blurb(self.prefs.games_fan, self.prefs.smooth_fan),
            Row::OutsideFan => fan_blurb(self.prefs.outside_fan, self.prefs.smooth_fan),
            Row::SmoothFan if self.prefs.smooth_fan => format!(
                "On: gently up and down with the temperature instead of on and off, in every game. Full speed above {} °C.",
                crate::fan_curve::GUARD_ON as i32
            ),
            Row::SmoothFan => "Off: the system's steps. Games: fan then only applies to games with a pause menu.".to_string(),
        };
        [what.to_string(), choice]
    }
}

/// "On", "Off".
pub fn on_off(on: bool) -> &'static str {
    if on {
        "On"
    } else {
        "Off"
    }
}

/// What a fan choice means, with the temperature it starts at taken from the profile itself (the curve's with
/// Smooth fan, the trips' without).
pub fn fan_blurb(f: Fan, smooth: bool) -> String {
    use crate::fan_curve::Shape;
    if smooth {
        return match f {
            Fan::VeryQuiet => format!("Very quiet: fan only when it gets warm ({} °C); the Pocket feels a bit warmer.", Shape::VeryQuiet.start() as i32),
            Fan::Quiet => format!("Quiet: the fan starts slowly at {} °C and stays slow. The Pocket gets a little warmer.", Shape::Quiet.start() as i32),
            Fan::Cool => format!("Cool: the fan starts at {} °C and turns faster. Cooler to hold, and you hear it more.", Shape::Cool.start() as i32),
            Fan::Auto => "Auto: follows the system's cooling profile, as a smooth curve.".to_string(),
        };
    }
    match (f, f.starts_at()) {
        (Fan::VeryQuiet, Some(t)) => format!("Very quiet: fan only when it gets warm ({t} °C); the Pocket feels a bit warmer."),
        (Fan::Quiet, Some(t)) => format!("Quiet: the fan starts at {t} °C. The Pocket gets warmer, the fan stays quiet longer."),
        (Fan::Cool, Some(t)) => format!("Cool: the fan starts at {t} °C and spins faster. Cooler to hold, and you hear it more."),
        _ => "Auto: the system's own cooling.".to_string(),
    }
}

/// The note under the description: which settings win, and which games these are.
pub const PRECEDENCE_NOTE: &str = "A choice saved in the pause menu for a console or a game (Options › Extras), or for a port on its card, \
wins over these. Games: performance covers games with a pause menu and PortMaster ports; with Smooth fan, Games: fan covers every game.";

impl PerfFanPage {
    /// The readout thread's news.
    pub fn event(&mut self, e: Event) {
        if let Event::Readout(r) = e {
            self.readout = Some(r);
        }
    }

    /// The four readout lines, label and value ("Unknown" for what the device doesn't report).
    pub fn readout_lines(&self) -> [(&'static str, String); 4] {
        let r = self.readout.clone().unwrap_or_default();
        let or = |v: Option<String>| match (&self.readout, v) {
            (None, _) => "…".to_string(),
            (Some(_), Some(v)) => v,
            (Some(_), None) => "Unknown".to_string(),
        };
        [
            ("Hottest", or(r.temperature())),
            ("Fan", or(r.fan())),
            ("CPU", or(r.cpu())),
            ("Battery draw", or(r.battery())),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIRM: Button = Button::East;
    const CANCEL: Button = Button::South;

    fn press(p: &mut PerfFanPage, b: Button) -> Action {
        p.press(b, CONFIRM, CANCEL)
    }

    #[test]
    fn up_and_down_wrap_and_every_row_has_words() {
        let mut p = PerfFanPage::new(Prefs::default());
        assert_eq!(p.row(), Row::GamesPerformance);
        assert_eq!(press(&mut p, Button::Up), Action::Nothing);
        assert_eq!(p.row(), Row::SmoothFan);
        press(&mut p, Button::Down);
        assert_eq!(p.row(), Row::GamesPerformance);
        for r in ROWS {
            assert!(!p.label(r).is_empty() && !p.value(r).is_empty());
            let [what, choice] = p.describe(r);
            assert!(!what.is_empty() && choice.starts_with(p.value(r)), "{r:?}: {choice}");
        }
    }

    #[test]
    fn left_and_right_step_through_the_choices_and_wrap() {
        let mut p = PerfFanPage::new(Prefs::default());
        assert_eq!(p.value(Row::GamesPerformance), "Auto");
        let mut seen = Vec::new();
        for _ in 0..3 {
            let Action::Save(prefs) = press(&mut p, Button::Right) else {
                panic!("a change saves")
            };
            seen.push(prefs.games_performance);
        }
        assert_eq!(seen, [Performance::Performance, Performance::Battery, Performance::Auto]);
        press(&mut p, Button::Left);
        assert_eq!(p.value(Row::GamesPerformance), "Battery");
        // A steps forward too (a tap on the selected row is an A): Games: fan starts on Quiet.
        press(&mut p, Button::Down);
        assert_eq!(p.value(Row::GamesFan), "Quiet");
        assert_eq!(
            press(&mut p, CONFIRM),
            Action::Save(Prefs {
                games_performance: Performance::Battery,
                games_fan: Fan::Auto,
                outside_fan: Fan::Auto,
                smooth_fan: true,
            })
        );
        press(&mut p, Button::Down);
        assert_eq!(p.value(Row::OutsideFan), "Auto");
        press(&mut p, Button::Right);
        assert_eq!(p.value(Row::OutsideFan), "Very quiet", "outside games: Auto, Very quiet, Quiet, Cool");
        press(&mut p, Button::Right);
        assert_eq!(p.value(Row::OutsideFan), "Quiet");
        press(&mut p, Button::Right);
        assert_eq!(p.prefs.outside_fan, Fan::Cool);
        assert_eq!(p.prefs.games_fan, Fan::Auto, "each row has its own value");
        // Smooth fan: on, and any of Left, Right or A switches it.
        press(&mut p, Button::Down);
        assert_eq!(p.value(Row::SmoothFan), "On");
        for b in [Button::Right, Button::Left, CONFIRM] {
            let before = p.prefs.smooth_fan;
            let Action::Save(prefs) = press(&mut p, b) else { panic!("a change saves") };
            assert_eq!(prefs.smooth_fan, !before, "{b:?}");
        }
        assert_eq!(p.value(Row::SmoothFan), "Off");
    }

    #[test]
    fn b_closes_and_stops_the_readout() {
        let mut p = PerfFanPage::new(Prefs::default());
        let alive = p.alive.clone();
        assert_eq!(press(&mut p, Button::North), Action::Nothing, "other buttons do nothing");
        assert_eq!(press(&mut p, CANCEL), Action::Close);
        assert!(!alive.load(Ordering::Relaxed));
    }

    #[test]
    fn the_explanations_say_what_the_choice_does() {
        let mut p = PerfFanPage::new(Prefs {
            games_performance: Performance::Battery,
            games_fan: Fan::Quiet,
            outside_fan: Fan::Cool,
            smooth_fan: false,
        });
        assert!(p.describe(Row::GamesPerformance)[1].contains("half speed"));
        assert!(p.describe(Row::GamesFan)[1].contains("52 °C"));
        assert!(p.describe(Row::OutsideFan)[1].contains("40 °C"));
        assert!(p.describe(Row::OutsideFan)[0].contains("sleeps"));
        assert!(p.describe(Row::SmoothFan)[1].starts_with("Off: the system's steps"));
        p.prefs.outside_fan = Fan::Auto;
        assert_eq!(p.describe(Row::OutsideFan)[1], "Auto: the system's own cooling.");
        assert!(PRECEDENCE_NOTE.contains("pause menu"));
        // With Smooth fan the curves' start temperatures, the same for Quiet and Cool as the trips'.
        p.prefs.smooth_fan = true;
        assert!(p.describe(Row::GamesFan)[1].contains("52 °C") && p.describe(Row::GamesFan)[1].contains("slow"));
        assert!(p.describe(Row::OutsideFan)[1].contains("system's cooling profile"));
        assert!(p.describe(Row::SmoothFan)[1].contains("78 °C"));
    }

    #[test]
    fn the_readout_says_waiting_then_numbers_then_unknown() {
        let mut p = PerfFanPage::new(Prefs::default());
        assert!(p.readout_lines().iter().all(|(_, v)| v == "…"), "nothing read yet");
        p.event(Event::Readout(Readout::demo()));
        let lines = p.readout_lines();
        assert_eq!(lines[0], ("Hottest", "47.3 °C".to_string()));
        assert_eq!(lines[1].1, "4400 rpm, 20% · starts at 52 °C");
        assert_eq!(lines[2].1, "Little 1.80 · Big 1.29 · Prime 1.13 GHz · GPU 305 MHz");
        assert_eq!(lines[3].1, "2.1 W");
        p.event(Event::Readout(Readout::default()));
        assert!(p.readout_lines().iter().all(|(_, v)| v == "Unknown"));
        p.event(Event::Settled);
        assert_eq!(p.readout, Some(Readout::default()), "not the page's news");
    }
}
