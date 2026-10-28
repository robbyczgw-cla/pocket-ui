//! A port's "Performance & fan" page (START on a PortMaster port's card): the port's own performance and fan, saved at
//! once and used from its next start (`port_perf`), "Use the defaults" (Settings → Performance & fan again), and, when
//! the port was seen keeping a core busy, the offer to run it on Battery in one press.
//!
//! The state and what a press asks for; the App writes the choice (`port_perf::Store::set`) and paints the page
//! (`app_port_perf.rs`). Nothing here touches a file, so every path is a plain test.

use std::path::PathBuf;

use crate::pad::Button;
use crate::perf_fan_page::{fan_blurb, fan_label, performance_label};
use crate::port_perf::{Choice, Note, HINT};
use crate::power::{Fan, Performance};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    /// "Use Battery for this port": only while the port keeps a core busy and isn't on Battery.
    UseBattery,
    Performance,
    Fan,
    /// Back to Settings → Performance & fan; only while the port has a choice of its own.
    Defaults,
}

const PERFORMANCE: [Performance; 3] = [Performance::Battery, Performance::Auto, Performance::Performance];
const FAN: [Fan; 3] = [Fan::Quiet, Fan::Auto, Fan::Cool];

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    /// Save the port's own `key` = `value` (`port_perf::Store::set`).
    Set {
        key: &'static str,
        value: &'static str,
    },
    /// Drop the port's own choice.
    Defaults,
}

pub struct PortPerfPage {
    /// The port's name and its launcher.
    pub title: String,
    pub rom: PathBuf,
    pub choice: Choice,
    /// Settings → Performance & fan's Games: performance and Games: fan: what "the defaults" are.
    pub defaults: (Performance, Fan),
    pub smooth_fan: bool,
    /// What the busy-wait look found, if it looked.
    pub note: Option<Note>,
    pub sel: usize,
}

fn step<T: Copy + PartialEq>(choices: &[T], now: T, delta: i32) -> T {
    let n = choices.len() as i32;
    let i = choices.iter().position(|c| *c == now).unwrap_or(0) as i32;
    choices[(i + delta).rem_euclid(n) as usize]
}

impl PortPerfPage {
    /// The page as it opens: on the offer when there is one, else on Performance.
    pub fn new(title: &str, rom: PathBuf, choice: Choice, defaults: (Performance, Fan), smooth_fan: bool, note: Option<Note>) -> PortPerfPage {
        PortPerfPage {
            title: title.to_string(),
            rom,
            choice,
            defaults,
            smooth_fan,
            note,
            sel: 0,
        }
    }

    /// Whether the offer is on the page.
    pub fn offers_battery(&self) -> bool {
        crate::port_perf::offers_battery(self.note.as_ref(), self.choice.performance)
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::with_capacity(4);
        if self.offers_battery() {
            rows.push(Row::UseBattery);
        }
        rows.extend([Row::Performance, Row::Fan]);
        if self.choice.own_performance || self.choice.own_fan {
            rows.push(Row::Defaults);
        }
        rows
    }

    pub fn row(&self) -> Row {
        let rows = self.rows();
        rows[self.sel.min(rows.len() - 1)]
    }

    fn select(&mut self, row: Row) {
        self.sel = self.rows().iter().position(|r| *r == row).unwrap_or(0);
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = self.rows().len();
        let delta = match b {
            Button::Up => {
                self.sel = (self.sel.min(n - 1) + n - 1) % n;
                return Action::Nothing;
            }
            Button::Down => {
                self.sel = (self.sel.min(n - 1) + 1) % n;
                return Action::Nothing;
            }
            b if b == cancel => return Action::Close,
            Button::Left => -1,
            Button::Right => 1,
            b if b == confirm => 1,
            _ => return Action::Nothing,
        };
        let row = self.row();
        let action = match row {
            Row::UseBattery if b == confirm => {
                self.choice.performance = Performance::Battery;
                self.choice.own_performance = true;
                Action::Set {
                    key: "performance",
                    value: Performance::Battery.value(),
                }
            }
            Row::UseBattery => return Action::Nothing,
            Row::Performance => {
                self.choice.performance = step(&PERFORMANCE, self.choice.performance, delta);
                self.choice.own_performance = true;
                Action::Set {
                    key: "performance",
                    value: self.choice.performance.value(),
                }
            }
            Row::Fan => {
                self.choice.fan = step(&FAN, self.choice.fan, delta);
                self.choice.own_fan = true;
                Action::Set {
                    key: "fan",
                    value: self.choice.fan.value(),
                }
            }
            Row::Defaults if b == confirm => {
                self.choice = Choice {
                    performance: self.defaults.0,
                    fan: self.defaults.1,
                    own_performance: false,
                    own_fan: false,
                };
                Action::Defaults
            }
            Row::Defaults => return Action::Nothing,
        };
        // The rows may have changed (the offer is taken, "Use the defaults" comes or goes): stay on the row, or on
        // Performance when it went.
        let rows = self.rows();
        if rows.contains(&row) {
            self.select(row);
        } else {
            self.select(Row::Performance);
        }
        action
    }

    pub fn label(&self, row: Row) -> &'static str {
        match row {
            Row::UseBattery => "Use Battery for this port",
            Row::Performance => "Performance",
            Row::Fan => "Fan",
            Row::Defaults => "Use the defaults",
        }
    }

    /// The value at the right of a row, "(default)" when it comes from Settings; None for the action rows.
    pub fn value(&self, row: Row) -> Option<String> {
        let mark = |own: bool| if own { "" } else { " (default)" };
        match row {
            Row::Performance => Some(format!("{}{}", performance_label(self.choice.performance), mark(self.choice.own_performance))),
            Row::Fan => Some(format!("{}{}", fan_label(self.choice.fan), mark(self.choice.own_fan))),
            Row::UseBattery | Row::Defaults => None,
        }
    }

    /// What the selected row is for, and what its choice does.
    pub fn describe(&self, row: Row) -> [String; 2] {
        match row {
            Row::UseBattery => [HINT.to_string(), self.measured()],
            Row::Performance => [
                "How fast the processor may run while this port plays. A change applies from the port's next start.".to_string(),
                match self.choice.performance {
                    Performance::Battery => "Battery: the processor's top speeds are held at about half. Cooler, quieter, and the battery lasts longer; a demanding port may stutter.".to_string(),
                    Performance::Auto => "Auto: the system decides; it speeds up and slows down as the port needs it.".to_string(),
                    Performance::Performance => "Max: the big cores and the graphics run at full speed all the time. Uses more battery and gets warmer.".to_string(),
                },
            ],
            Row::Fan if !self.smooth_fan => [
                "How early the fan starts while this port plays.".to_string(),
                "Needs Smooth fan (Settings › Playing › Performance & fan). Without it, ports keep the system's own cooling.".to_string(),
            ],
            Row::Fan => ["How early the fan starts while this port plays.".to_string(), fan_blurb(self.choice.fan, true)],
            Row::Defaults => [
                "Forget this port's own choice: Settings › Playing › Performance & fan applies again.".to_string(),
                format!(
                    "There: Games: performance {}, Games: fan {}.",
                    performance_label(self.defaults.0),
                    fan_label(self.defaults.1)
                ),
            ],
        }
    }

    /// "Measured the last time it ran: one thread at 98 % of a core while the rest of the Pocket used 0.2 cores."
    fn measured(&self) -> String {
        match &self.note {
            Some(n) => format!(
                "Measured the last time it ran: one thread at {:.0}% of a core while the rest of the Pocket used {:.1} cores.",
                (n.top * 100.0).min(100.0),
                n.rest.max(0.0)
            ),
            None => String::new(),
        }
    }

    /// The hints at the bottom for the selected row.
    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.row() {
            // In the order of Settings → Performance & fan's.
            Row::UseBattery => vec![("B", "Back"), ("A", "Use Battery")],
            Row::Defaults => vec![("B", "Back"), ("A", "Use the defaults")],
            Row::Performance | Row::Fan => vec![("B", "Back"), ("A", "Next"), ("< >", "Change")],
        }
    }

    /// A short mark of what the page shows, for the redraw check.
    pub fn mark(&self) -> (usize, Choice) {
        (self.sel, self.choice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIRM: Button = Button::East;
    const CANCEL: Button = Button::South;

    fn page(note: Option<Note>, choice: Choice) -> PortPerfPage {
        PortPerfPage::new(
            "Deltarune",
            PathBuf::from("/storage/roms/ports/Deltarune.sh"),
            choice,
            (Performance::Auto, Fan::Auto),
            true,
            note,
        )
    }

    fn defaults() -> Choice {
        Choice {
            performance: Performance::Auto,
            fan: Fan::Auto,
            own_performance: false,
            own_fan: false,
        }
    }

    fn busy() -> Note {
        Note {
            busy: true,
            top: 0.98,
            rest: 0.24,
            at: 1,
            seen: false,
        }
    }

    #[test]
    fn a_port_that_spins_opens_on_the_offer_and_one_press_takes_it() {
        let mut p = page(Some(busy()), defaults());
        assert_eq!(p.rows(), [Row::UseBattery, Row::Performance, Row::Fan]);
        assert_eq!(p.row(), Row::UseBattery);
        assert_eq!(p.describe(Row::UseBattery)[0], HINT);
        assert!(p.describe(Row::UseBattery)[1].contains("98% of a core") && p.describe(Row::UseBattery)[1].contains("0.2 cores"));
        assert_eq!(p.press(Button::Right, CONFIRM, CANCEL), Action::Nothing, "Left/Right don't take the offer");
        assert_eq!(
            p.press(CONFIRM, CONFIRM, CANCEL),
            Action::Set {
                key: "performance",
                value: "battery"
            }
        );
        // The offer is gone, the port has its own choice now, and the selection is on Performance.
        assert_eq!(p.rows(), [Row::Performance, Row::Fan, Row::Defaults]);
        assert_eq!(p.row(), Row::Performance);
        assert_eq!(p.value(Row::Performance).as_deref(), Some("Battery"));
        assert_eq!(p.value(Row::Fan).as_deref(), Some("Auto (default)"));
    }

    #[test]
    fn left_and_right_change_a_row_and_save_it_for_the_port() {
        let mut p = page(None, defaults());
        assert_eq!(p.rows(), [Row::Performance, Row::Fan], "no offer, nothing of its own yet");
        assert_eq!(p.value(Row::Performance).as_deref(), Some("Auto (default)"));
        assert_eq!(
            p.press(Button::Left, CONFIRM, CANCEL),
            Action::Set {
                key: "performance",
                value: "battery"
            }
        );
        assert_eq!(
            p.press(Button::Right, CONFIRM, CANCEL),
            Action::Set {
                key: "performance",
                value: "auto"
            }
        );
        assert_eq!(p.value(Row::Performance).as_deref(), Some("Auto"), "its own now, though the same value");
        p.press(Button::Down, CONFIRM, CANCEL);
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::Set { key: "fan", value: "cool" });
        assert_eq!(p.row(), Row::Fan);
        // Use the defaults: everything from Settings again, and the row goes.
        p.press(Button::Down, CONFIRM, CANCEL);
        assert_eq!(p.row(), Row::Defaults);
        assert!(p.describe(Row::Defaults)[1].contains("Games: performance Auto"));
        assert_eq!(p.press(Button::Left, CONFIRM, CANCEL), Action::Nothing);
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::Defaults);
        assert_eq!(p.rows(), [Row::Performance, Row::Fan]);
        assert_eq!(p.row(), Row::Performance);
        assert_eq!(p.choice, defaults());
        assert_eq!(p.press(CANCEL, CONFIRM, CANCEL), Action::Close);
    }

    #[test]
    fn up_and_down_wrap_and_the_texts_say_what_counts() {
        let mut p = page(Some(Note { busy: false, ..busy() }), defaults());
        assert_eq!(p.rows(), [Row::Performance, Row::Fan], "a port that doesn't spin has no offer");
        p.press(Button::Up, CONFIRM, CANCEL);
        assert_eq!(p.row(), Row::Fan);
        p.press(Button::Down, CONFIRM, CANCEL);
        assert_eq!(p.row(), Row::Performance);
        assert!(p.describe(Row::Performance)[0].contains("next start"));
        assert!(p.describe(Row::Fan)[1].starts_with("Auto"));
        p.smooth_fan = false;
        assert!(p.describe(Row::Fan)[1].contains("Smooth fan"));
        // On Battery already: no offer even for a spinning port.
        let on_battery = Choice {
            performance: Performance::Battery,
            own_performance: true,
            ..defaults()
        };
        assert!(!page(Some(busy()), on_battery).rows().contains(&Row::UseBattery));
        assert_eq!(page(Some(busy()), defaults()).hints()[1], ("A", "Use Battery"));
    }
}
