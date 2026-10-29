//! Quick settings' Performance and Fan rows: what they show, what Left/Right change, and what has to be applied.
//! Pure logic over values in memory (`Profiles`); `quick.rs` keeps one, `quick_paint.rs` draws it, the App
//! (`app_quick_profiles.rs`) fills it in and applies a change on a thread.
//!
//! The rows set the same choices as Settings → Playing → Performance & fan (`perf_fan::Prefs`):
//! - **Performance** is always Games: performance, on the shelf and over a game.
//! - **Fan** is Fan outside games on the shelf, Games: fan over a game.
//!
//! A game (or its console, or a port) with a choice of its own in Options keeps it: the row then says so ("this game:
//! Max") and a change only moves the default, which a toast repeats. Where the device has no fan there is no Fan row;
//! without cpufreq the Performance row says it is not available.

use std::path::Path;
use std::sync::OnceLock;

use crate::perf_fan::Prefs;
use crate::perf_fan_page::{fan_label, performance_label};
use crate::power::{Fan, Performance};

/// Left to right: less to more.
pub const PERFORMANCE: [Performance; 3] = [Performance::Battery, Performance::Auto, Performance::Performance];
pub const FAN: [Fan; 4] = [Fan::VeryQuiet, Fan::Quiet, Fan::Auto, Fan::Cool];

/// What runs the game the panel was opened over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// pocket-core: Options → Extras has Performance and Fan.
    PocketCore,
    /// ARMSX2, Dolphin, Azahar: Options has Performance (`emu_settings`).
    Standalone,
    /// A PortMaster port: its card's Performance & fan (`port_perf`).
    Port,
    RetroArch,
    /// Any other emulator.
    Other,
}

/// Whose own choice a game runs with instead of the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Whose {
    Game,
    /// The console's, saved in a pocket-core game's Options for every game of it.
    Console,
    Port,
}

/// The game the panel was opened over, as far as these rows care.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GameView {
    pub kind: Kind,
    pub own_performance: Option<(Performance, Whose)>,
    pub own_fan: Option<(Fan, Whose)>,
}

/// Everything the two rows need, in memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Profiles {
    pub prefs: Prefs,
    /// The device has cpufreq policies (`power::Power` can set a profile).
    pub cpufreq: bool,
    /// The device has the PWM fan (`power::fan_zone`).
    pub fan: bool,
    /// None on the shelf.
    pub game: Option<GameView>,
}

impl Default for Profiles {
    fn default() -> Profiles {
        Profiles {
            prefs: Prefs::default(),
            cpufreq: true,
            fan: true,
            game: None,
        }
    }
}

/// One of the two rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Which {
    Performance,
    Fan,
}

/// What a change asks the App to do besides saving the choices (Smooth fan's plan follows the saved choices by
/// itself, `app_perf_fan`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// A line for the running pocket-core game's command FIFO (`set performance …`, `set fan …`).
    PocketCore(String),
    /// The running PS2, GameCube, Wii or 3DS game's profile (`standalone::set_performance`).
    Standalone(&'static str),
}

/// "this game", "this console", "this port".
fn whose_label(w: Whose) -> &'static str {
    match w {
        Whose::Game => "this game",
        Whose::Console => "this console",
        Whose::Port => "this port",
    }
}

/// "in Options" for a game or console, "on its card" for a port.
fn where_set(w: Whose) -> &'static str {
    match w {
        Whose::Port => "on its card",
        _ => "in Options",
    }
}

fn step<T: Copy + PartialEq>(choices: &[T], now: T, dir: i32, wrap: bool) -> Option<T> {
    let n = choices.len() as i32;
    let i = choices.iter().position(|c| *c == now).unwrap_or(0) as i32;
    let next = if wrap { (i + dir).rem_euclid(n) } else { (i + dir).clamp(0, n - 1) };
    (next != i).then(|| choices[next as usize])
}

impl Profiles {
    /// The rows in panel order: no Fan row without a fan.
    pub fn rows(&self) -> Vec<Which> {
        let mut v = vec![Which::Performance];
        if self.fan {
            v.push(Which::Fan);
        }
        v
    }

    fn in_game(&self) -> bool {
        self.game.is_some()
    }

    /// The fan choice the row changes: Games: fan over a game, Fan outside games on the shelf.
    pub fn fan_choice(&self) -> Fan {
        if self.in_game() {
            self.prefs.games_fan
        } else {
            self.prefs.outside_fan
        }
    }

    /// Whether the row can be changed here.
    pub fn available(&self, w: Which) -> bool {
        match w {
            Which::Performance => self.cpufreq,
            Which::Fan => self.fan,
        }
    }

    /// The row's value: the default it changes ("Auto"), or why it can't.
    pub fn value(&self, w: Which) -> String {
        match w {
            Which::Performance if !self.cpufreq => "Not available here".into(),
            Which::Performance => performance_label(self.prefs.games_performance).into(),
            Which::Fan => fan_label(self.fan_choice()).into(),
        }
    }

    /// The small text left of the value: what it is for, or what this game runs with instead.
    pub fn note(&self, w: Which) -> Option<String> {
        if !self.available(w) {
            return None;
        }
        let Some(g) = self.game else {
            return Some(
                match w {
                    Which::Performance => "for games",
                    Which::Fan => "in menus",
                }
                .into(),
            );
        };
        match w {
            Which::Performance => Some(match (g.own_performance, g.kind) {
                (Some((p, whose)), _) => format!("{}: {}", whose_label(whose), performance_label(p)),
                (None, Kind::Standalone) => {
                    format!("this game: {}", performance_label(standalone_level(self.prefs.games_performance)))
                }
                (None, Kind::Port) => "from its next start".into(),
                (None, Kind::RetroArch | Kind::Other) => "this game: the system's".into(),
                (None, Kind::PocketCore) => "this game too".into(),
            }),
            Which::Fan => Some(match (g.own_fan, g.kind) {
                (Some((f, whose)), _) => format!("{}: {}", whose_label(whose), fan_label(f)),
                (None, Kind::PocketCore) => "this game too".into(),
                (None, _) if !self.prefs.smooth_fan => "this game: the system's".into(),
                (None, _) => "this game too".into(),
            }),
        }
    }

    /// The game under the panel keeps a choice of its own for this row (its note stands out).
    pub fn overridden(&self, w: Which) -> bool {
        self.game.is_some_and(|g| match w {
            Which::Performance => g.own_performance.is_some(),
            Which::Fan => g.own_fan.is_some(),
        })
    }

    /// The line under the list while the row is selected (at most four short lines).
    pub fn desc(&self, w: Which) -> String {
        if w == Which::Performance && !self.cpufreq {
            return "This device doesn't let pocket-ui set the processor's speed.".into();
        }
        // Each line short enough for one line of the footer (four at most).
        let what = match (w, self.in_game()) {
            (Which::Performance, _) => "How fast the processor may run in games: for every game.",
            (Which::Fan, false) => "The fan on the shelf, in the lists and in Settings.",
            (Which::Fan, true) => "How early the fan starts while you play: for every game.",
        };
        let choice = match w {
            Which::Performance => match self.prefs.games_performance {
                Performance::Battery => "Battery: about half speed, enough for Game Boy and NES; the battery lasts longer.",
                Performance::Auto => "Auto: the system decides; for PS2, GameCube and 3DS that is full speed.",
                Performance::Performance => "Max: full speed all the time. Warmer, and uses more battery.",
            },
            Which::Fan => match self.fan_choice() {
                Fan::VeryQuiet => "Very quiet: fan only when it gets warm; the Pocket feels a bit warmer.",
                Fan::Quiet => "Quiet: the fan starts later and stays slow.",
                Fan::Auto => "Auto: the system's own cooling profile.",
                Fan::Cool => "Cool: the fan starts early and turns faster.",
            },
        };
        let mut lines = vec![what.to_string(), choice.to_string()];
        if let Some(extra) = self.game.and_then(|g| self.game_line(w, g)) {
            lines.push(extra);
        }
        lines.join("\n")
    }

    /// Over a game: why the game doesn't follow the row, if it doesn't.
    fn game_line(&self, w: Which, g: GameView) -> Option<String> {
        match w {
            Which::Performance => match (g.own_performance, g.kind) {
                (Some((p, whose)), _) => Some(format!(
                    "Set for {} {} ({}): this changes the default only.",
                    whose_label(whose),
                    where_set(whose),
                    performance_label(p)
                )),
                (None, Kind::Port) => Some("A port takes it from its next start.".into()),
                (None, Kind::RetroArch) => Some("RetroArch games keep the system's speed.".into()),
                (None, Kind::Other) => Some("This emulator keeps the system's speed.".into()),
                _ => None,
            },
            Which::Fan => match g.own_fan {
                Some((f, whose)) => Some(format!(
                    "Set for {} {} ({}): this changes the default only.",
                    whose_label(whose),
                    where_set(whose),
                    fan_label(f)
                )),
                None if g.kind != Kind::PocketCore && !self.prefs.smooth_fan => Some("With Smooth fan off, only pocket-ui's own games follow it.".into()),
                None => None,
            },
        }
    }

    /// Left (-1) or Right (+1), or A (`wrap`: on from the last to the first): the new choices, None when nothing
    /// changes (the end of the row, or not available).
    pub fn step(&self, w: Which, dir: i32, wrap: bool) -> Option<Prefs> {
        if !self.available(w) {
            return None;
        }
        let mut p = self.prefs;
        match w {
            Which::Performance => p.games_performance = step(&PERFORMANCE, p.games_performance, dir, wrap)?,
            Which::Fan if self.in_game() => p.games_fan = step(&FAN, p.games_fan, dir, wrap)?,
            Which::Fan => p.outside_fan = step(&FAN, p.outside_fan, dir, wrap)?,
        }
        Some(p)
    }

    /// After a change: the toast that says the game keeps its own choice, if it does.
    pub fn kept(&self, w: Which) -> Option<String> {
        let g = self.game?;
        match w {
            Which::Performance => g.own_performance.map(|(p, whose)| {
                format!(
                    "Default for games: {}. {} keeps {}",
                    performance_label(self.prefs.games_performance),
                    cap(whose_label(whose)),
                    performance_label(p)
                )
            }),
            Which::Fan => g.own_fan.map(|(f, whose)| {
                format!(
                    "Default for games: {}. {} keeps {}",
                    fan_label(self.prefs.games_fan),
                    cap(whose_label(whose)),
                    fan_label(f)
                )
            }),
        }
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// What a PS2, GameCube, Wii or 3DS game without its own choice runs with: Battery, else Max
/// (`emu_settings::performance_for_default`), as a level for the labels.
pub fn standalone_level(games: Performance) -> Performance {
    match crate::emu_settings::performance_for_default(games) {
        "battery" => Performance::Battery,
        _ => Performance::Performance,
    }
}

/// A standalone game's stored choice (`max`, `balanced`, `battery`) as a level.
pub fn from_standalone(stored: &str) -> Performance {
    match stored {
        "max" => Performance::Performance,
        "battery" => Performance::Battery,
        _ => Performance::Auto,
    }
}

/// What to apply to the running game for a change from `old` to `new` over `game` (nothing on the shelf: games read
/// their choice when they start, and the fan outside games follows the saved choices).
pub fn effects(old: &Prefs, new: &Prefs, game: Option<&GameView>) -> Vec<Effect> {
    let Some(g) = game else { return Vec::new() };
    let mut v = Vec::new();
    let perf_changed = old.games_performance != new.games_performance;
    let fan_changed = old.games_fan != new.games_fan;
    match g.kind {
        Kind::PocketCore => {
            if perf_changed && g.own_performance.is_none() {
                v.push(Effect::PocketCore(format!("set performance {}", new.games_performance.value())));
            }
            if fan_changed && g.own_fan.is_none() {
                v.push(Effect::PocketCore(format!("set fan {}", new.games_fan.value())));
            }
        }
        Kind::Standalone => {
            let (was, now) = (
                crate::emu_settings::performance_for_default(old.games_performance),
                crate::emu_settings::performance_for_default(new.games_performance),
            );
            if was != now && g.own_performance.is_none() {
                v.push(Effect::Standalone(now));
            }
        }
        // A port's performance is set once its process runs; RetroArch and the rest keep the system's. Their fan is
        // Smooth fan's plan, which follows the saved choices.
        Kind::Port | Kind::RetroArch | Kind::Other => {}
    }
    v
}

/// What the device has, found once on a thread at the start ([`spawn_probe`]); None until then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    pub cpufreq: bool,
    pub fan: bool,
}

static CAPS: OnceLock<Caps> = OnceLock::new();

/// Looks below `root` (`/`, or the virtual device's mock sysfs): a cpufreq policy, the PWM fan's thermal zone.
pub fn probe(root: &Path) -> Caps {
    let cpufreq = std::fs::read_dir(root.join("sys/devices/system/cpu/cpufreq"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with("policy"));
    Caps {
        cpufreq,
        fan: crate::power::fan_zone(root).is_some(),
    }
}

/// Finds the device's [`Caps`] on a thread, so opening Quick settings never reads sysfs.
pub fn spawn_probe(root: std::path::PathBuf) {
    let _ = std::thread::Builder::new().name("quick-caps".into()).spawn(move || {
        let caps = probe(&root);
        println!("quick settings: cpufreq {}, fan {}", caps.cpufreq, caps.fan);
        let _ = CAPS.set(caps);
    });
}

/// The probe's result, if it is done.
pub fn caps() -> Option<Caps> {
    CAPS.get().copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn shelf() -> Profiles {
        Profiles::default()
    }

    fn game(kind: Kind) -> GameView {
        GameView {
            kind,
            own_performance: None,
            own_fan: None,
        }
    }

    fn over(kind: Kind) -> Profiles {
        Profiles {
            game: Some(game(kind)),
            ..Profiles::default()
        }
    }

    #[test]
    fn on_the_shelf_performance_is_the_games_and_fan_the_menus() {
        let p = shelf();
        assert_eq!(p.rows(), [Which::Performance, Which::Fan]);
        assert_eq!(
            (p.value(Which::Performance), p.note(Which::Performance).as_deref()),
            ("Auto".into(), Some("for games"))
        );
        assert_eq!((p.value(Which::Fan), p.note(Which::Fan).as_deref()), ("Auto".into(), Some("in menus")));
        let n = p.step(Which::Performance, 1, false).unwrap();
        assert_eq!(
            (n.games_performance, n.games_fan, n.outside_fan),
            (Performance::Performance, Fan::Quiet, Fan::Auto)
        );
        let n = p.step(Which::Fan, -1, false).unwrap();
        assert_eq!((n.outside_fan, n.games_fan), (Fan::Quiet, Fan::Quiet), "the fan outside games, not Games: fan");
        assert!(
            effects(&p.prefs, &n, None).is_empty(),
            "nothing runs: the fan follows the saved choice by itself"
        );
    }

    #[test]
    fn over_a_game_both_rows_are_the_games_defaults() {
        let p = over(Kind::PocketCore);
        assert_eq!(p.value(Which::Fan), "Quiet", "Games: fan's start value");
        let n = p.step(Which::Fan, 1, false).unwrap();
        assert_eq!((n.games_fan, n.outside_fan), (Fan::Auto, Fan::Auto));
        assert_eq!(effects(&p.prefs, &n, p.game.as_ref()), [Effect::PocketCore("set fan auto".into())]);
        let n = p.step(Which::Performance, -1, false).unwrap();
        assert_eq!(effects(&p.prefs, &n, p.game.as_ref()), [Effect::PocketCore("set performance battery".into())]);
        assert_eq!(p.note(Which::Performance).as_deref(), Some("this game too"));
    }

    #[test]
    fn left_and_right_stop_at_the_ends_and_a_goes_round() {
        let mut p = shelf();
        p.prefs.games_performance = Performance::Performance;
        assert_eq!(p.step(Which::Performance, 1, false), None, "Max is the end");
        assert_eq!(p.step(Which::Performance, 1, true).unwrap().games_performance, Performance::Battery);
        p.prefs.games_performance = Performance::Battery;
        assert_eq!(p.step(Which::Performance, -1, false), None);
        assert_eq!(p.value(Which::Performance), "Battery");
        p.prefs.games_performance = Performance::Performance;
        assert_eq!(p.value(Which::Performance), "Max", "the one word for the top level");
    }

    #[test]
    fn a_games_own_choice_is_shown_and_kept() {
        let mut p = over(Kind::PocketCore);
        p.game = Some(GameView {
            kind: Kind::PocketCore,
            own_performance: Some((Performance::Performance, Whose::Game)),
            own_fan: Some((Fan::Cool, Whose::Console)),
        });
        assert_eq!(p.value(Which::Performance), "Auto");
        assert_eq!(p.note(Which::Performance).as_deref(), Some("this game: Max"));
        assert_eq!(p.note(Which::Fan).as_deref(), Some("this console: Cool"));
        assert!(p
            .desc(Which::Performance)
            .contains("Set for this game in Options (Max): this changes the default only."));
        let n = p.step(Which::Performance, -1, false).unwrap();
        assert_eq!(n.games_performance, Performance::Battery, "the default still changes");
        assert!(effects(&p.prefs, &n, p.game.as_ref()).is_empty(), "the game keeps its own");
        assert!(p.overridden(Which::Performance) && p.overridden(Which::Fan) && !over(Kind::PocketCore).overridden(Which::Fan));
        p.prefs = n;
        assert_eq!(p.kept(Which::Performance).as_deref(), Some("Default for games: Battery. This game keeps Max"));
        assert_eq!(over(Kind::PocketCore).kept(Which::Performance), None, "no toast without an own choice");
        // A port's own, from its card.
        let mut q = over(Kind::Port);
        q.game = Some(GameView {
            kind: Kind::Port,
            own_performance: Some((Performance::Battery, Whose::Port)),
            own_fan: None,
        });
        assert_eq!(q.note(Which::Performance).as_deref(), Some("this port: Battery"));
        assert!(q.desc(Which::Performance).contains("on its card"));
    }

    #[test]
    fn a_standalone_game_without_its_own_follows_and_auto_is_max_there() {
        let p = over(Kind::Standalone);
        assert_eq!(
            p.note(Which::Performance).as_deref(),
            Some("this game: Max"),
            "Auto is full speed for these systems"
        );
        let battery = p.step(Which::Performance, -1, false).unwrap();
        assert_eq!(effects(&p.prefs, &battery, p.game.as_ref()), [Effect::Standalone("battery")]);
        let max = Prefs {
            games_performance: Performance::Performance,
            ..p.prefs
        };
        assert!(effects(&p.prefs, &max, p.game.as_ref()).is_empty(), "Auto and Max are the same for it");
        assert_eq!(effects(&battery, &p.prefs, p.game.as_ref()), [Effect::Standalone("max")]);
        // With its own choice in Options it keeps it.
        let own = GameView {
            own_performance: Some((from_standalone("balanced"), Whose::Game)),
            ..game(Kind::Standalone)
        };
        assert!(effects(&p.prefs, &battery, Some(&own)).is_empty());
        let q = Profiles { game: Some(own), ..p };
        assert_eq!(q.note(Which::Performance).as_deref(), Some("this game: Auto"));
    }

    #[test]
    fn retroarch_keeps_the_systems_speed_and_says_so() {
        let mut p = over(Kind::RetroArch);
        assert_eq!(p.note(Which::Performance).as_deref(), Some("this game: the system's"));
        assert!(p.desc(Which::Performance).contains("RetroArch games keep the system's speed."));
        let n = p.step(Which::Performance, 1, false).unwrap();
        assert!(effects(&p.prefs, &n, p.game.as_ref()).is_empty());
        assert_eq!(p.note(Which::Fan).as_deref(), Some("this game too"), "Smooth fan drives it");
        p.prefs.smooth_fan = false;
        assert_eq!(p.note(Which::Fan).as_deref(), Some("this game: the system's"));
        assert!(p.desc(Which::Fan).contains("Smooth fan off"));
    }

    #[test]
    fn no_fan_no_fan_row_and_no_cpufreq_says_so() {
        let p = Profiles { fan: false, ..shelf() };
        assert_eq!(p.rows(), [Which::Performance]);
        let q = Profiles { cpufreq: false, ..shelf() };
        assert_eq!(q.rows(), [Which::Performance, Which::Fan], "still a row, with a reason");
        assert_eq!(q.value(Which::Performance), "Not available here");
        assert_eq!(q.note(Which::Performance), None);
        assert_eq!(q.step(Which::Performance, 1, false), None);
        assert!(q.desc(Which::Performance).contains("doesn't let"));
    }

    #[test]
    fn the_probe_finds_cpufreq_and_the_fan_or_neither() {
        let d = TestDir::new("qcaps");
        assert_eq!(probe(&d), Caps { cpufreq: false, fan: false });
        std::fs::create_dir_all(d.join("sys/devices/system/cpu/cpufreq/policy0")).unwrap();
        let zone = d.join("sys/class/thermal/thermal_zone3/cdev0");
        std::fs::create_dir_all(&zone).unwrap();
        std::fs::write(zone.join("type"), "pwm-fan\n").unwrap();
        assert_eq!(probe(&d), Caps { cpufreq: true, fan: true });
    }
}
